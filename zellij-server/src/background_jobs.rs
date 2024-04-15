use zellij_utils::async_std::task;
use zellij_utils::consts::{
    session_info_cache_file_name, session_info_folder_for_session, session_layout_cache_file_name,
    ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
};
use zellij_utils::data::{Event, HttpVerb, SessionInfo};
use zellij_utils::errors::{prelude::*, BackgroundJobContext, ContextType};
use zellij_utils::surf::{
    http::{Method, Url},
    RequestBuilder,
};

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
use crate::plugins::{PluginId, PluginInstruction};
use crate::screen::ScreenInstruction;
use crate::thread_bus::Bus;
use crate::ClientId;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BackgroundJob {
    DisplayPaneError(Vec<PaneId>, String),
    AnimatePluginLoading(u32),                            // u32 - plugin_id
    StopPluginLoadingAnimation(u32),                      // u32 - plugin_id
    ReadAllSessionInfosOnMachine,                         // u32 - plugin_id
    ReportSessionInfo(String, SessionInfo),               // String - session name
    ReportLayoutInfo((String, BTreeMap<String, String>)), // BTreeMap<file_name, pane_contents>
    RunCommand(
        PluginId,
        ClientId,
        String,
        Vec<String>,
        BTreeMap<String, String>,
        PathBuf,
        BTreeMap<String, String>,
    ), // command, args, env_variables, cwd, context
    WebRequest(
        PluginId,
        ClientId,
        String, // url
        HttpVerb,
        BTreeMap<String, String>, // headers
        Vec<u8>,                  // body
        BTreeMap<String, String>, // context
    ),
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
            BackgroundJob::ReportLayoutInfo(..) => BackgroundJobContext::ReportLayoutInfo,
            BackgroundJob::RunCommand(..) => BackgroundJobContext::RunCommand,
            BackgroundJob::WebRequest(..) => BackgroundJobContext::WebRequest,
            BackgroundJob::Exit => BackgroundJobContext::Exit,
        }
    }
}

static FLASH_DURATION_MS: u64 = 1000;
static PLUGIN_ANIMATION_OFFSET_DURATION_MD: u64 = 500;
static SESSION_READ_DURATION: u64 = 1000;
static DEFAULT_SERIALIZATION_INTERVAL: u64 = 60000;

pub(crate) fn background_jobs_main(
    bus: Bus<BackgroundJob>,
    serialization_interval: Option<u64>,
    disable_session_metadata: bool,
) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut running_jobs: HashMap<BackgroundJob, Instant> = HashMap::new();
    let mut loading_plugins: HashMap<u32, Arc<AtomicBool>> = HashMap::new(); // u32 - plugin_id
    let current_session_name = Arc::new(Mutex::new(String::default()));
    let current_session_info = Arc::new(Mutex::new(SessionInfo::default()));
    let current_session_layout = Arc::new(Mutex::new((String::new(), BTreeMap::new())));
    let last_serialization_time = Arc::new(Mutex::new(Instant::now()));
    let serialization_interval = serialization_interval.map(|s| s * 1000); // convert to
                                                                           // milliseconds

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
            BackgroundJob::ReportLayoutInfo(session_layout) => {
                *current_session_layout.lock().unwrap() = session_layout;
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
                    let current_session_layout = current_session_layout.clone();
                    let last_serialization_time = last_serialization_time.clone();
                    async move {
                        loop {
                            let current_session_name =
                                current_session_name.lock().unwrap().to_string();
                            let current_session_info = current_session_info.lock().unwrap().clone();
                            let current_session_layout =
                                current_session_layout.lock().unwrap().clone();
                            if !disable_session_metadata {
                                write_session_state_to_disk(
                                    current_session_name.clone(),
                                    current_session_info,
                                    current_session_layout,
                                );
                            }
                            let session_infos_on_machine =
                                read_other_live_session_states(&current_session_name);
                            let resurrectable_sessions =
                                find_resurrectable_sessions(&session_infos_on_machine);
                            let _ = senders.send_to_screen(ScreenInstruction::UpdateSessionInfos(
                                session_infos_on_machine,
                                resurrectable_sessions,
                            ));
                            if last_serialization_time
                                .lock()
                                .unwrap()
                                .elapsed()
                                .as_millis()
                                >= serialization_interval
                                    .unwrap_or(DEFAULT_SERIALIZATION_INTERVAL)
                                    .into()
                            {
                                let _ = senders.send_to_screen(ScreenInstruction::DumpLayoutToHd);
                                *last_serialization_time.lock().unwrap() = Instant::now();
                            }
                            task::sleep(std::time::Duration::from_millis(SESSION_READ_DURATION))
                                .await;
                        }
                    }
                });
            },
            BackgroundJob::RunCommand(
                plugin_id,
                client_id,
                command,
                args,
                env_variables,
                cwd,
                context,
            ) => {
                // when async_std::process stabilizes, we should change this to be async
                std::thread::spawn({
                    let senders = bus.senders.clone();
                    move || {
                        let output = std::process::Command::new(&command)
                            .args(&args)
                            .envs(env_variables)
                            .current_dir(cwd)
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .output();
                        match output {
                            Ok(output) => {
                                let stdout = output.stdout.to_vec();
                                let stderr = output.stderr.to_vec();
                                let exit_code = output.status.code();
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(plugin_id),
                                    Some(client_id),
                                    Event::RunCommandResult(exit_code, stdout, stderr, context),
                                )]));
                            },
                            Err(e) => {
                                log::error!("Failed to run command: {}", e);
                                let stdout = vec![];
                                let stderr = format!("{}", e).as_bytes().to_vec();
                                let exit_code = Some(2);
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(plugin_id),
                                    Some(client_id),
                                    Event::RunCommandResult(exit_code, stdout, stderr, context),
                                )]));
                            },
                        }
                    }
                });
            },
            BackgroundJob::WebRequest(plugin_id, client_id, url, verb, headers, body, context) => {
                task::spawn({
                    let senders = bus.senders.clone();
                    async move {
                        async fn web_request(
                            url: String,
                            verb: HttpVerb,
                            headers: BTreeMap<String, String>,
                            body: Vec<u8>,
                        ) -> Result<
                            (u16, BTreeMap<String, String>, Vec<u8>), // status_code, headers, body
                            zellij_utils::surf::Error,
                        > {
                            let url = Url::parse(&url)?;
                            let http_method = match verb {
                                HttpVerb::Get => Method::Get,
                                HttpVerb::Post => Method::Post,
                                HttpVerb::Put => Method::Put,
                                HttpVerb::Delete => Method::Delete,
                            };
                            let mut req = RequestBuilder::new(http_method, url);
                            if !body.is_empty() {
                                req = req.body(body);
                            }
                            for (header, value) in headers {
                                req = req.header(header.as_str(), value);
                            }
                            let mut res = req.await?;
                            let status_code = res.status();
                            let headers: BTreeMap<String, String> = res
                                .iter()
                                .map(|(name, value)| (name.to_string(), value.to_string()))
                                .collect();
                            let body = res.take_body().into_bytes().await?;
                            Ok((status_code as u16, headers, body))
                        }

                        match web_request(url, verb, headers, body).await {
                            Ok((status, headers, body)) => {
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(plugin_id),
                                    Some(client_id),
                                    Event::WebRequestResult(status, headers, body, context),
                                )]));
                            },
                            Err(e) => {
                                log::error!("Failed to send web request: {}", e);
                                let error_body = e.to_string().as_bytes().to_vec();
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(plugin_id),
                                    Some(client_id),
                                    Event::WebRequestResult(
                                        400,
                                        BTreeMap::new(),
                                        error_body,
                                        context,
                                    ),
                                )]));
                            },
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

fn write_session_state_to_disk(
    current_session_name: String,
    current_session_info: SessionInfo,
    current_session_layout: (String, BTreeMap<String, String>),
) {
    let metadata_cache_file_name = session_info_cache_file_name(&current_session_name);
    let (current_session_layout, layout_files_to_write) = current_session_layout;
    let _wrote_metadata_file =
        std::fs::create_dir_all(session_info_folder_for_session(&current_session_name).as_path())
            .and_then(|_| std::fs::File::create(metadata_cache_file_name))
            .and_then(|mut f| write!(f, "{}", current_session_info.to_string()));

    if !current_session_layout.is_empty() {
        let layout_cache_file_name = session_layout_cache_file_name(&current_session_name);
        let _wrote_layout_file = std::fs::create_dir_all(
            session_info_folder_for_session(&current_session_name).as_path(),
        )
        .and_then(|_| std::fs::File::create(layout_cache_file_name))
        .and_then(|mut f| write!(f, "{}", current_session_layout))
        .and_then(|_| {
            let session_info_folder = session_info_folder_for_session(&current_session_name);
            for (external_file_name, external_file_contents) in layout_files_to_write {
                std::fs::File::create(session_info_folder.join(external_file_name))
                    .and_then(|mut f| write!(f, "{}", external_file_contents))
                    .unwrap_or_else(|e| {
                        log::error!("Failed to write layout metadata file: {:?}", e);
                    });
            }
            Ok(())
        });
    }
}

fn read_other_live_session_states(current_session_name: &str) -> BTreeMap<String, SessionInfo> {
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
        let session_cache_file_name = session_info_cache_file_name(&session_name);
        if let Ok(raw_session_info) = fs::read_to_string(&session_cache_file_name) {
            if let Ok(session_info) =
                SessionInfo::from_string(&raw_session_info, &current_session_name)
            {
                session_infos_on_machine.insert(session_name, session_info);
            }
        }
    }
    session_infos_on_machine
}

fn find_resurrectable_sessions(
    session_infos_on_machine: &BTreeMap<String, SessionInfo>,
) -> BTreeMap<String, Duration> {
    match fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let session_name = folder_name.file_name()?.to_str()?.to_owned();
                    if session_infos_on_machine.contains_key(&session_name) {
                        // this is not a dead session...
                        return None;
                    }
                    let layout_file_name = session_layout_cache_file_name(&session_name);
                    let ctime = match std::fs::metadata(&layout_file_name)
                        .and_then(|metadata| metadata.created())
                    {
                        Ok(created) => Some(created),
                        Err(e) => {
                            if e.kind() == std::io::ErrorKind::NotFound {
                                return None; // no layout file, cannot resurrect session, let's not
                                             // list it
                            } else {
                                log::error!(
                                    "Failed to read created stamp of resurrection file: {:?}",
                                    e
                                );
                            }
                            None
                        },
                    };
                    let elapsed_duration = ctime
                        .map(|ctime| {
                            Duration::from_secs(ctime.elapsed().ok().unwrap_or_default().as_secs())
                        })
                        .unwrap_or_default();
                    Some((session_name, elapsed_duration))
                })
                .collect()
        },
        Err(e) => {
            log::error!("Failed to read session info cache dir: {:?}", e);
            BTreeMap::new()
        },
    }
}
