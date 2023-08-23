use zellij_utils::async_std::task;
use zellij_utils::consts::{ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR};
use zellij_utils::data::SessionInfo;
use zellij_utils::errors::{prelude::*, BackgroundJobContext, ContextType};

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::panes::PaneId;
use crate::screen::ScreenInstruction;
use crate::thread_bus::Bus;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BackgroundJob {
    DisplayPaneError(Vec<PaneId>, String),
    AnimatePluginLoading(u32),              // u32 - plugin_id
    StopPluginLoadingAnimation(u32),        // u32 - plugin_id
    ReadAllSessionInfosOnMachine,           // u32 - plugin_id
    ReportSessionInfo(String, SessionInfo), // String - session name
    Exit,
}

impl From<&BackgroundJob> for BackgroundJobContext {
    fn from(background_job: &BackgroundJob) -> Self {
        match *background_job {
            BackgroundJob::DisplayPaneError(..) => BackgroundJobContext::DisplayPaneError,
            BackgroundJob::AnimatePluginLoading(..) => BackgroundJobContext::AnimatePluginLoading,
            BackgroundJob::StopPluginLoadingAnimation(..) => {
                BackgroundJobContext::StopPluginLoadingAnimation
            },
            BackgroundJob::ReadAllSessionInfosOnMachine => {
                BackgroundJobContext::ReadAllSessionInfosOnMachine
            },
            BackgroundJob::ReportSessionInfo(..) => BackgroundJobContext::ReportSessionInfo,
            BackgroundJob::Exit => BackgroundJobContext::Exit,
        }
    }
}

static FLASH_DURATION_MS: u64 = 1000;
static PLUGIN_ANIMATION_OFFSET_DURATION_MD: u64 = 500;
static SESSION_READ_DURATION: u64 = 1000;

pub(crate) fn background_jobs_main(bus: Bus<BackgroundJob>) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut running_jobs: HashMap<BackgroundJob, Instant> = HashMap::new();
    let mut loading_plugins: HashMap<u32, Arc<AtomicBool>> = HashMap::new(); // u32 - plugin_id
    let current_session_name = Arc::new(Mutex::new(String::default()));
    let current_session_info = Arc::new(Mutex::new(SessionInfo::default()));

    loop {
        let (event, mut err_ctx) = bus.recv().with_context(err_context)?;
        err_ctx.add_call(ContextType::BackgroundJob((&event).into()));
        let job = event.clone();
        match event {
            BackgroundJob::DisplayPaneError(pane_ids, text) => {
                if job_already_running(job, &mut running_jobs) {
                    continue;
                }
                task::spawn({
                    let senders = bus.senders.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddRedPaneFrameColorOverride(
                                pane_ids.clone(),
                                Some(text),
                            ),
                        );
                        task::sleep(std::time::Duration::from_millis(FLASH_DURATION_MS)).await;
                        let _ = senders.send_to_screen(
                            ScreenInstruction::ClearPaneFrameColorOverride(pane_ids),
                        );
                    }
                });
            },
            BackgroundJob::AnimatePluginLoading(pid) => {
                let loading_plugin = Arc::new(AtomicBool::new(true));
                if job_already_running(job, &mut running_jobs) {
                    continue;
                }
                task::spawn({
                    let senders = bus.senders.clone();
                    let loading_plugin = loading_plugin.clone();
                    async move {
                        while loading_plugin.load(Ordering::SeqCst) {
                            let _ = senders.send_to_screen(
                                ScreenInstruction::ProgressPluginLoadingOffset(pid),
                            );
                            task::sleep(std::time::Duration::from_millis(
                                PLUGIN_ANIMATION_OFFSET_DURATION_MD,
                            ))
                            .await;
                        }
                    }
                });
                loading_plugins.insert(pid, loading_plugin);
            },
            BackgroundJob::StopPluginLoadingAnimation(pid) => {
                if let Some(loading_plugin) = loading_plugins.remove(&pid) {
                    loading_plugin.store(false, Ordering::SeqCst);
                }
            },
            BackgroundJob::ReportSessionInfo(session_name, session_info) => {
                *current_session_name.lock().unwrap() = session_name;
                *current_session_info.lock().unwrap() = session_info;
            },
            BackgroundJob::ReadAllSessionInfosOnMachine => {
                // this job should only be run once and it keeps track of other sessions (as well
                // as this one's) infos (metadata mostly) and sends it to the screen which in turn
                // forwards it to plugins and other places it needs to be
                if running_jobs.get(&job).is_some() {
                    continue;
                }
                running_jobs.insert(job, Instant::now());
                task::spawn({
                    let senders = bus.senders.clone();
                    let current_session_info = current_session_info.clone();
                    let current_session_name = current_session_name.clone();
                    async move {
                        loop {
                            // write state of current session

                            // write it to disk
                            let current_session_name =
                                current_session_name.lock().unwrap().to_string();
                            let cache_file_name =
                                session_info_cache_file_name(&current_session_name);
                            let current_session_info = current_session_info.lock().unwrap().clone();
                            let wrote_file =
                                std::fs::create_dir_all(ZELLIJ_SESSION_INFO_CACHE_DIR.as_path())
                                    .and_then(|_| std::fs::File::create(cache_file_name))
                                    .and_then(|mut f| {
                                        write!(f, "{}", current_session_info.to_string())
                                    });
                            // start a background job (if not already running) that'll periodically read this and other
                            // sesion infos and report back

                            // read state of all sessions
                            let mut other_session_names = vec![];
                            let mut session_infos_on_machine = BTreeMap::new();
                            // we do this so that the session infos will be actual and we're
                            // reasonably sure their session is running
                            if let Ok(files) = fs::read_dir(&*ZELLIJ_SOCK_DIR) {
                                files.for_each(|file| {
                                    if let Ok(file) = file {
                                        if let Ok(file_name) = file.file_name().into_string() {
                                            if file.file_type().unwrap().is_socket() {
                                                other_session_names.push(file_name);
                                            }
                                        }
                                    }
                                });
                            }

                            for session_name in other_session_names {
                                let session_cache_file_name = ZELLIJ_SESSION_INFO_CACHE_DIR
                                    .join(format!("{}.kdl", session_name));
                                if let Ok(raw_session_info) =
                                    fs::read_to_string(&session_cache_file_name)
                                {
                                    if let Ok(session_info) = SessionInfo::from_string(
                                        &raw_session_info,
                                        &current_session_name,
                                    ) {
                                        session_infos_on_machine.insert(session_name, session_info);
                                    }
                                }
                            }
                            let _ = senders.send_to_screen(ScreenInstruction::UpdateSessionInfos(
                                session_infos_on_machine,
                            ));
                            task::sleep(std::time::Duration::from_millis(SESSION_READ_DURATION))
                                .await;
                        }
                    }
                });
            },
            BackgroundJob::Exit => {
                for loading_plugin in loading_plugins.values() {
                    loading_plugin.store(false, Ordering::SeqCst);
                }

                let cache_file_name =
                    session_info_cache_file_name(&current_session_name.lock().unwrap().to_owned());
                let _ = std::fs::remove_file(cache_file_name);
                return Ok(());
            },
        }
    }
}

fn job_already_running(
    job: BackgroundJob,
    running_jobs: &mut HashMap<BackgroundJob, Instant>,
) -> bool {
    match running_jobs.get_mut(&job) {
        Some(current_running_job_start_time) => {
            if current_running_job_start_time.elapsed() > Duration::from_millis(FLASH_DURATION_MS) {
                *current_running_job_start_time = Instant::now();
                false
            } else {
                true
            }
        },
        None => {
            running_jobs.insert(job.clone(), Instant::now());
            false
        },
    }
}

fn session_info_cache_file_name(session_name: &str) -> PathBuf {
    ZELLIJ_SESSION_INFO_CACHE_DIR.join(format!("{}.kdl", &session_name))
}
