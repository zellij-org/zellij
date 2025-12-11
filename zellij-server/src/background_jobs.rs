use async_std::task;
use zellij_utils::consts::{
    session_info_cache_file_name, session_info_folder_for_session, session_layout_cache_file_name,
    VERSION, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
};
use zellij_utils::data::{Event, HttpVerb, SessionInfo, WebServerStatus};
use zellij_utils::errors::{prelude::*, BackgroundJobContext, ContextType};
use zellij_utils::input::layout::RunPlugin;

use isahc::prelude::*;
use isahc::AsyncReadResponseExt;
use isahc::{config::RedirectPolicy, HttpClient, Request};

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
use crate::pty::PtyInstruction;
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
    ReportPluginList(BTreeMap<PluginId, RunPlugin>),      // String - session name
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
    HighlightPanesWithMessage(Vec<PaneId>, String),
    RenderToClients,
    QueryZellijWebServerStatus,
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
            BackgroundJob::ReportPluginList(..) => BackgroundJobContext::ReportPluginList,
            BackgroundJob::RenderToClients => BackgroundJobContext::ReportPluginList,
            BackgroundJob::HighlightPanesWithMessage(..) => {
                BackgroundJobContext::HighlightPanesWithMessage
            },
            BackgroundJob::QueryZellijWebServerStatus => {
                BackgroundJobContext::QueryZellijWebServerStatus
            },
            BackgroundJob::Exit => BackgroundJobContext::Exit,
        }
    }
}

static LONG_FLASH_DURATION_MS: u64 = 1000;
static FLASH_DURATION_MS: u64 = 400; // Doherty threshold
static PLUGIN_ANIMATION_OFFSET_DURATION_MD: u64 = 500;
static SESSION_READ_DURATION: u64 = 1000;
static DEFAULT_SERIALIZATION_INTERVAL: u64 = 60000;
static REPAINT_DELAY_MS: u64 = 10;

pub(crate) fn background_jobs_main(
    bus: Bus<BackgroundJob>,
    serialization_interval: Option<u64>,
    disable_session_metadata: bool,
    web_server_base_url: String,
) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut running_jobs: HashMap<BackgroundJob, Instant> = HashMap::new();
    let mut loading_plugins: HashMap<u32, Arc<AtomicBool>> = HashMap::new(); // u32 - plugin_id
    let current_session_name = Arc::new(Mutex::new(String::default()));
    let current_session_info = Arc::new(Mutex::new(SessionInfo::default()));
    let current_session_plugin_list: Arc<Mutex<BTreeMap<PluginId, RunPlugin>>> =
        Arc::new(Mutex::new(BTreeMap::new()));
    let current_session_layout = Arc::new(Mutex::new((String::new(), BTreeMap::new())));
    let last_serialization_time = Arc::new(Mutex::new(Instant::now()));
    let serialization_interval = serialization_interval.map(|s| s * 1000); // convert to
                                                                           // milliseconds
    let last_render_request: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let http_client = HttpClient::builder()
        // TODO: timeout?
        .redirect_policy(RedirectPolicy::Follow)
        .build()
        .ok();

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
                        task::sleep(std::time::Duration::from_millis(LONG_FLASH_DURATION_MS)).await;
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
            BackgroundJob::ReportPluginList(plugin_list) => {
                *current_session_plugin_list.lock().unwrap() = plugin_list;
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
                    let current_session_plugin_list = current_session_plugin_list.clone();
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
                            let mut session_infos_on_machine =
                                read_other_live_session_states(&current_session_name);
                            for (session_name, session_info) in session_infos_on_machine.iter_mut()
                            {
                                if session_name == &current_session_name {
                                    let current_session_plugin_list =
                                        current_session_plugin_list.lock().unwrap().clone();
                                    session_info.populate_plugin_list(current_session_plugin_list);
                                }
                            }
                            let resurrectable_sessions =
                                find_resurrectable_sessions(&session_infos_on_machine);
                            let _ = senders.send_to_screen(ScreenInstruction::UpdateSessionInfos(
                                session_infos_on_machine,
                                resurrectable_sessions,
                            ));
                            let _ = senders.send_to_pty(PtyInstruction::UpdateAndReportCwds);
                            if last_serialization_time
                                .lock()
                                .unwrap()
                                .elapsed()
                                .as_millis()
                                >= serialization_interval
                                    .unwrap_or(DEFAULT_SERIALIZATION_INTERVAL)
                                    .into()
                            {
                                let _ = senders.send_to_screen(
                                    ScreenInstruction::SerializeLayoutForResurrection,
                                );
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
                    let http_client = http_client.clone();
                    async move {
                        async fn web_request(
                            url: String,
                            verb: HttpVerb,
                            headers: BTreeMap<String, String>,
                            body: Vec<u8>,
                            http_client: HttpClient,
                        ) -> Result<
                            (u16, BTreeMap<String, String>, Vec<u8>), // status_code, headers, body
                            isahc::Error,
                        > {
                            let mut request = match verb {
                                HttpVerb::Get => Request::get(url),
                                HttpVerb::Post => Request::post(url),
                                HttpVerb::Put => Request::put(url),
                                HttpVerb::Delete => Request::delete(url),
                            };
                            for (header, value) in headers {
                                request = request.header(header.as_str(), value);
                            }
                            let mut res = if !body.is_empty() {
                                let req = request.body(body)?;
                                http_client.send_async(req).await?
                            } else {
                                let req = request.body(())?;
                                http_client.send_async(req).await?
                            };

                            let status_code = res.status();
                            let headers: BTreeMap<String, String> = res
                                .headers()
                                .iter()
                                .filter_map(|(name, value)| match value.to_str() {
                                    Ok(value) => Some((name.to_string(), value.to_string())),
                                    Err(e) => {
                                        log::error!(
                                            "Failed to convert header {:?} to string: {:?}",
                                            name,
                                            e
                                        );
                                        None
                                    },
                                })
                                .collect();
                            let body = res.bytes().await?;
                            Ok((status_code.as_u16(), headers, body))
                        }
                        let Some(http_client) = http_client else {
                            log::error!("Cannot perform http request, likely due to a misconfigured http client");
                            return;
                        };

                        match web_request(url, verb, headers, body, http_client).await {
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
            BackgroundJob::QueryZellijWebServerStatus => {
                if !cfg!(feature = "web_server_capability") {
                    // no web server capability, no need to query
                    continue;
                }

                task::spawn({
                    let http_client = http_client.clone();
                    let senders = bus.senders.clone();
                    let web_server_base_url = web_server_base_url.clone();
                    async move {
                        async fn web_request(
                            http_client: HttpClient,
                            web_server_base_url: &str,
                        ) -> Result<
                            (u16, Vec<u8>), // status_code, body
                            isahc::Error,
                        > {
                            let request =
                                Request::get(format!("{}/info/version", web_server_base_url,));
                            let req = request.body(())?;
                            let mut res = http_client.send_async(req).await?;

                            let status_code = res.status();
                            let body = res.bytes().await?;
                            Ok((status_code.as_u16(), body))
                        }
                        let Some(http_client) = http_client else {
                            log::error!("Cannot perform http request, likely due to a misconfigured http client");
                            return;
                        };

                        let http_client = http_client.clone();
                        match web_request(http_client, &web_server_base_url).await {
                            Ok((status, body)) => {
                                if status == 200 && &body == VERSION.as_bytes() {
                                    // online
                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            None,
                                            None,
                                            Event::WebServerStatus(WebServerStatus::Online(
                                                web_server_base_url.clone(),
                                            )),
                                        )]));
                                } else if status == 200 {
                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            None,
                                            None,
                                            Event::WebServerStatus(
                                                WebServerStatus::DifferentVersion(
                                                    String::from_utf8_lossy(&body).to_string(),
                                                ),
                                            ),
                                        )]));
                                } else {
                                    // offline/error
                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            None,
                                            None,
                                            Event::WebServerStatus(WebServerStatus::Offline),
                                        )]));
                                }
                            },
                            Err(e) => {
                                if e.kind() == isahc::error::ErrorKind::ConnectionFailed {
                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            None,
                                            None,
                                            Event::WebServerStatus(WebServerStatus::Offline),
                                        )]));
                                } else {
                                    // no-op - otherwise we'll get errors if we were mid-request
                                    // (eg. when the server was shut down by a user action)
                                }
                            },
                        }
                    }
                });
            },
            BackgroundJob::RenderToClients => {
                // last_render_request being Some() represents a render request that is pending
                // last_render_request is only ever set to Some() if an async task is spawned to
                // send the actual render instruction
                //
                // given this:
                // - if last_render_request is None and we received this job, we should spawn an
                // async task to send the render instruction and log the current task time
                // - if last_render_request is Some(), it means we're currently waiting to render,
                // so we should log the render request and do nothing, once the async task has
                // finished running, it will check to see if the render time was updated while it
                // was running, and if so send this instruction again so the process can start anew
                let (should_run_task, current_time) = {
                    let mut last_render_request = last_render_request.lock().unwrap();
                    let should_run_task = last_render_request.is_none();
                    let current_time = Instant::now();
                    *last_render_request = Some(current_time);
                    (should_run_task, current_time)
                };
                if should_run_task {
                    task::spawn({
                        let senders = bus.senders.clone();
                        let last_render_request = last_render_request.clone();
                        let task_start_time = current_time;
                        async move {
                            task::sleep(std::time::Duration::from_millis(REPAINT_DELAY_MS)).await;
                            let _ = senders.send_to_screen(ScreenInstruction::RenderToClients);
                            {
                                let mut last_render_request = last_render_request.lock().unwrap();
                                if let Some(last_render_request) = *last_render_request {
                                    if last_render_request > task_start_time {
                                        // another render request was received while we were
                                        // sleeping, schedule this job again so that we can also
                                        // render that request
                                        let _ = senders.send_to_background_jobs(
                                            BackgroundJob::RenderToClients,
                                        );
                                    }
                                }
                                // reset the last_render_request so that the task will be spawned
                                // again once a new request is received
                                *last_render_request = None;
                            }
                        }
                    });
                }
            },
            BackgroundJob::HighlightPanesWithMessage(pane_ids, text) => {
                if job_already_running(job, &mut running_jobs) {
                    continue;
                }
                task::spawn({
                    let senders = bus.senders.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddHighlightPaneFrameColorOverride(
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
            if current_running_job_start_time.elapsed()
                > Duration::from_millis(LONG_FLASH_DURATION_MS)
            {
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
