#[allow(unused_imports)] // some imports used only with web_server_capability feature
use zellij_utils::consts::{
    session_info_cache_file_name, session_info_folder_for_session, session_layout_cache_file_name,
    VERSION, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR,
};
#[allow(unused_imports)]
use zellij_utils::data::{Event, HttpVerb, LayoutInfo, SessionInfo, WebServerStatus};
use zellij_utils::errors::{prelude::*, BackgroundJobContext, ContextType};
use zellij_utils::input::layout::RunPlugin;
#[allow(unused_imports)]
use zellij_utils::shared::parse_base_url;

#[cfg(feature = "web_server_capability")]
use zellij_utils::web_server_commands::{
    discover_webserver_sockets, query_webserver_with_response, InstructionForWebServer,
    WebServerResponse,
};

use isahc::prelude::*;
use isahc::AsyncReadResponseExt;
use isahc::{config::RedirectPolicy, HttpClient, Request};

use crate::panes::PaneId;
use crate::plugins::{PluginId, PluginInstruction};
use crate::pty::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::thread_bus::Bus;
use crate::{ClientId, ServerInstruction};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum BackgroundJob {
    DisplayPaneError(Vec<PaneId>, String),
    AnimatePluginLoading(u32),                            // u32 - plugin_id
    StopPluginLoadingAnimation(u32),                      // u32 - plugin_id
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
    ClearHelpText {
        client_id: ClientId,
    },
    FlashPaneBell(Vec<PaneId>),
    StopFlashPaneBell(Vec<PaneId>),
    FlashTabBell(usize),     // usize = tab_id
    StopFlashTabBell(usize), // usize = tab_id
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
            BackgroundJob::ClearHelpText { .. } => BackgroundJobContext::ClearHelpText,
            BackgroundJob::FlashPaneBell(..) => BackgroundJobContext::FlashPaneBell,
            BackgroundJob::StopFlashPaneBell(..) => BackgroundJobContext::StopFlashPaneBell,
            BackgroundJob::FlashTabBell(..) => BackgroundJobContext::FlashTabBell,
            BackgroundJob::StopFlashTabBell(..) => BackgroundJobContext::StopFlashTabBell,
            BackgroundJob::Exit => BackgroundJobContext::Exit,
        }
    }
}

static LONG_FLASH_DURATION_MS: u64 = 1000;
static FLASH_DURATION_MS: u64 = 400; // Doherty threshold
static PLUGIN_ANIMATION_OFFSET_DURATION_MD: u64 = 500;
static SESSION_METADATA_WRITE_INTERVAL_MS: u64 = 1000;
static UPDATE_AND_REPORT_CWDS_INTERVAL_MS: u64 = 1000;
static DEFAULT_SERIALIZATION_INTERVAL: u64 = 60000;
static REPAINT_DELAY_MS: u64 = 10;
static HELP_TEXT_DEBOUNCE_DURATION: u64 = 5000;

#[derive(Clone)]
pub struct SessionScanState {
    pub current_session_name: Arc<Mutex<String>>,
    pub current_session_info: Arc<Mutex<SessionInfo>>,
    pub current_session_plugin_list: Arc<Mutex<BTreeMap<PluginId, RunPlugin>>>,
}

static SESSION_SCAN_STATE: std::sync::OnceLock<SessionScanState> = std::sync::OnceLock::new();

pub fn session_scan_state() -> Option<&'static SessionScanState> {
    SESSION_SCAN_STATE.get()
}

#[allow(unused_variables)] // web_server_base_url used only with web_server_capability feature
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

    let _ = SESSION_SCAN_STATE.set(SessionScanState {
        current_session_name: current_session_name.clone(),
        current_session_info: current_session_info.clone(),
        current_session_plugin_list: current_session_plugin_list.clone(),
    });
    let last_serialization_time = Arc::new(Mutex::new(Instant::now()));
    let serialization_interval = serialization_interval.map(|s| s * 1000); // convert to
                                                                           // milliseconds
    let last_render_request: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let pending_help_text_clear: Arc<Mutex<HashMap<ClientId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut flashing_pane_bells: HashMap<PaneId, Arc<AtomicBool>> = HashMap::new();
    let mut flashing_tab_bells: HashMap<usize, Arc<AtomicBool>> = HashMap::new();

    let http_client = HttpClient::builder()
        // TODO: timeout?
        .redirect_policy(RedirectPolicy::Follow)
        .build()
        .ok();
    // We needn't do anything with the runtime, but it should exist at this point.
    let runtime = crate::global_async_runtime::get_tokio_runtime();

    {
        let senders = bus.senders.clone();
        let serialization_ms = serialization_interval.unwrap_or(DEFAULT_SERIALIZATION_INTERVAL);
        runtime.spawn(async move {
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_millis(serialization_ms));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _ = senders.send_to_screen(ScreenInstruction::SerializeLayoutForResurrection);
            }
        });
    }

    {
        let senders = bus.senders.clone();
        runtime.spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(
                UPDATE_AND_REPORT_CWDS_INTERVAL_MS,
            ));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let _ = senders.send_to_pty(PtyInstruction::UpdateAndReportCwds);
            }
        });
    }

    if !disable_session_metadata {
        let current_session_name = current_session_name.clone();
        let current_session_info = current_session_info.clone();
        let current_session_layout = current_session_layout.clone();
        runtime.spawn(async move {
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(
                SESSION_METADATA_WRITE_INTERVAL_MS,
            ));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                let name = current_session_name.lock().unwrap().clone();
                if name.is_empty() {
                    continue;
                }
                let info = current_session_info.lock().unwrap().clone();
                let layout = current_session_layout.lock().unwrap().clone();
                write_session_state_to_disk(name, info, layout);
            }
        });
    }

    loop {
        let (event, mut err_ctx) = bus.recv().with_context(err_context)?;
        err_ctx.add_call(ContextType::BackgroundJob((&event).into()));
        let job = event.clone();
        match event {
            BackgroundJob::DisplayPaneError(pane_ids, text) => {
                if job_already_running(job, &mut running_jobs) {
                    continue;
                }
                runtime.spawn({
                    let senders = bus.senders.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddRedPaneFrameColorOverride(
                                pane_ids.clone(),
                                Some(text),
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(
                            LONG_FLASH_DURATION_MS,
                        ))
                        .await;
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
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let loading_plugin = loading_plugin.clone();
                    async move {
                        while loading_plugin.load(Ordering::SeqCst) {
                            let _ = senders.send_to_screen(
                                ScreenInstruction::ProgressPluginLoadingOffset(pid),
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(
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

                // Update session save time for plugin query
                let timestamp_millis = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let _ = bus
                    .senders
                    .send_to_plugin(PluginInstruction::UpdateSessionSaveTime(timestamp_millis));
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
                runtime.spawn({
                    let senders = bus.senders.clone();
                    async move {
                        let output = tokio::process::Command::new(&command)
                            .args(&args)
                            .envs(env_variables)
                            .current_dir(cwd)
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .output()
                            .await;
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
                runtime.spawn({
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
                #[cfg(feature = "web_server_capability")]
                {
                    let status = query_webserver_via_ipc(&web_server_base_url)
                        .unwrap_or(WebServerStatus::Offline);
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let _web_server_base_url = web_server_base_url.clone();
                        async move {
                            let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                None,
                                None,
                                Event::WebServerStatus(status),
                            )]));
                        }
                    });
                }
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
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let last_render_request = last_render_request.clone();
                        let task_start_time = current_time;
                        async move {
                            tokio::time::sleep(std::time::Duration::from_millis(REPAINT_DELAY_MS))
                                .await;
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
                runtime.spawn({
                    let senders = bus.senders.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddHighlightPaneFrameColorOverride(
                                pane_ids.clone(),
                                Some(text),
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(FLASH_DURATION_MS))
                            .await;
                        let _ = senders.send_to_screen(
                            ScreenInstruction::ClearPaneFrameColorOverride(pane_ids),
                        );
                    }
                });
            },
            BackgroundJob::ClearHelpText { client_id } => {
                let should_spawn = {
                    let mut pending = pending_help_text_clear.lock().unwrap();
                    let current_time = Instant::now();
                    let should_spawn = !pending.contains_key(&client_id);
                    pending.insert(client_id, current_time);
                    should_spawn
                };

                if should_spawn {
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let pending = pending_help_text_clear.clone();
                        let debounce_duration = Duration::from_millis(HELP_TEXT_DEBOUNCE_DURATION);
                        async move {
                            tokio::time::sleep(debounce_duration).await;
                            loop {
                                let next_sleep_duration = {
                                    let mut pending = pending.lock().unwrap();
                                    match pending.get(&client_id) {
                                        Some(&last_motion_time) => {
                                            let time_since_motion =
                                                Instant::now().duration_since(last_motion_time);
                                            if time_since_motion >= debounce_duration {
                                                pending.remove(&client_id);
                                                None
                                            } else {
                                                let remaining = debounce_duration
                                                    .saturating_sub(time_since_motion);
                                                Some(remaining)
                                            }
                                        },
                                        None => break,
                                    }
                                };

                                match next_sleep_duration {
                                    Some(duration) => {
                                        tokio::time::sleep(duration).await;
                                    },
                                    None => {
                                        let _ = senders.send_to_server(
                                            ServerInstruction::ClearMouseHelpText(client_id),
                                        );
                                        break;
                                    },
                                }
                            }
                        }
                    });
                }
            },
            BackgroundJob::FlashPaneBell(pane_ids) => {
                let is_flashing = Arc::new(AtomicBool::new(true));
                for &pane_id in &pane_ids {
                    flashing_pane_bells.insert(pane_id, is_flashing.clone());
                }
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let pane_ids_clone = pane_ids.clone();
                    let flag = is_flashing.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddHighlightPaneFrameColorOverride(
                                pane_ids_clone.clone(),
                                None,
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(FLASH_DURATION_MS))
                            .await;
                        if flag.load(Ordering::SeqCst) {
                            let _ = senders.send_to_screen(
                                ScreenInstruction::ClearPaneFrameColorOverride(pane_ids_clone),
                            );
                        }
                    }
                });
            },
            BackgroundJob::StopFlashPaneBell(pane_ids) => {
                for &pane_id in &pane_ids {
                    if let Some(flag) = flashing_pane_bells.remove(&pane_id) {
                        flag.store(false, Ordering::SeqCst);
                    }
                }
                let _ = bus
                    .senders
                    .send_to_screen(ScreenInstruction::ClearPaneFrameColorOverride(pane_ids));
            },
            BackgroundJob::FlashTabBell(tab_id) => {
                let is_flashing = Arc::new(AtomicBool::new(true));
                flashing_tab_bells.insert(tab_id, is_flashing.clone());
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let flag = is_flashing.clone();
                    async move {
                        let _ = senders
                            .send_to_screen(ScreenInstruction::SetTabBellFlash(tab_id, true));
                        tokio::time::sleep(std::time::Duration::from_millis(FLASH_DURATION_MS))
                            .await;
                        if flag.load(Ordering::SeqCst) {
                            let _ = senders
                                .send_to_screen(ScreenInstruction::SetTabBellFlash(tab_id, false));
                        }
                    }
                });
            },
            BackgroundJob::StopFlashTabBell(tab_id) => {
                if let Some(flag) = flashing_tab_bells.remove(&tab_id) {
                    flag.store(false, Ordering::SeqCst);
                }
                let _ = bus
                    .senders
                    .send_to_screen(ScreenInstruction::SetTabBellFlash(tab_id, false));
            },
            BackgroundJob::Exit => {
                for loading_plugin in loading_plugins.values() {
                    loading_plugin.store(false, Ordering::SeqCst);
                }

                // Flush the current layout to disk before shutting down so the
                // session is resurrectable even if it exited before the
                // periodic SESSION_METADATA_WRITE_INTERVAL_MS ticker fired.
                // Writes session-layout.kdl which list-sessions uses to mark
                // the session as resurrectable.
                let name = current_session_name.lock().unwrap().clone();
                if !name.is_empty() {
                    let info = current_session_info.lock().unwrap().clone();
                    let layout = current_session_layout.lock().unwrap().clone();
                    write_session_state_to_disk(name.clone(), info, layout);
                }

                let cache_file_name = session_info_cache_file_name(&name);
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

fn file_content_changed(path: &std::path::Path, new_content: &[u8]) -> bool {
    match std::fs::read(path) {
        Ok(existing) => existing != new_content,
        Err(_) => true,
    }
}

pub fn write_session_state_to_disk(
    current_session_name: String,
    current_session_info: SessionInfo,
    current_session_layout: (String, BTreeMap<String, String>),
) {
    let metadata_cache_file_name = session_info_cache_file_name(&current_session_name);
    let (current_session_layout, layout_files_to_write) = current_session_layout;
    let new_metadata = current_session_info.to_string();
    if file_content_changed(&metadata_cache_file_name, new_metadata.as_bytes()) {
        let _wrote_metadata_file = std::fs::create_dir_all(
            session_info_folder_for_session(&current_session_name).as_path(),
        )
        .and_then(|_| std::fs::File::create(&metadata_cache_file_name))
        .and_then(|mut f| write!(f, "{}", new_metadata));
    }

    if !current_session_layout.is_empty() {
        let layout_cache_file_name = session_layout_cache_file_name(&current_session_name);
        if file_content_changed(&layout_cache_file_name, current_session_layout.as_bytes()) {
            let _wrote_layout_file = std::fs::create_dir_all(
                session_info_folder_for_session(&current_session_name).as_path(),
            )
            .and_then(|_| std::fs::File::create(&layout_cache_file_name))
            .and_then(|mut f| write!(f, "{}", current_session_layout));
        }
        let session_info_folder = session_info_folder_for_session(&current_session_name);
        for (external_file_name, external_file_contents) in layout_files_to_write {
            let external_file_path = session_info_folder.join(&external_file_name);
            if file_content_changed(&external_file_path, external_file_contents.as_bytes()) {
                std::fs::File::create(&external_file_path)
                    .and_then(|mut f| write!(f, "{}", external_file_contents))
                    .unwrap_or_else(|e| {
                        log::error!("Failed to write layout metadata file: {:?}", e);
                    });
            }
        }
    }
}

pub fn scan_session_list(
    current_session_name: &str,
    available_layouts: &[LayoutInfo],
    current_session_plugin_list: &BTreeMap<PluginId, RunPlugin>,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
) -> (BTreeMap<String, SessionInfo>, BTreeMap<String, Duration>) {
    let mut session_infos_on_machine =
        read_other_live_session_states(current_session_name, sock_dir, session_info_cache_dir);
    for (name, info) in session_infos_on_machine.iter_mut() {
        if name == current_session_name {
            info.populate_plugin_list(current_session_plugin_list.clone());
            info.available_layouts = available_layouts.to_vec();
        }
    }
    let resurrectable_sessions =
        find_resurrectable_sessions(&session_infos_on_machine, session_info_cache_dir);
    (session_infos_on_machine, resurrectable_sessions)
}

pub fn scan_session_list_default_dirs(
    current_session_name: &str,
    available_layouts: &[LayoutInfo],
    current_session_plugin_list: &BTreeMap<PluginId, RunPlugin>,
) -> (BTreeMap<String, SessionInfo>, BTreeMap<String, Duration>) {
    scan_session_list(
        current_session_name,
        available_layouts,
        current_session_plugin_list,
        &*ZELLIJ_SOCK_DIR,
        &*ZELLIJ_SESSION_INFO_CACHE_DIR,
    )
}

fn read_other_live_session_states(
    current_session_name: &str,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
) -> BTreeMap<String, SessionInfo> {
    let mut session_infos_on_machine = BTreeMap::new();
    let registry = zellij_utils::sessions::ensure_registry();

    for entry in registry.running_sessions() {
        let session_name = &entry.display_name;
        let creation_time = std::fs::metadata(sock_dir.join(&entry.id))
            .ok()
            .and_then(|f| f.created().ok().or_else(|| f.modified().ok()))
            .and_then(|d| d.elapsed().ok())
            .map(|d| Duration::from_secs(d.as_secs()))
            .unwrap_or_default();
        let session_cache_file_name = session_info_cache_dir
            .join(session_name)
            .join("session-metadata.kdl");
        if let Ok(raw_session_info) = fs::read_to_string(&session_cache_file_name) {
            if let Ok(mut session_info) =
                SessionInfo::from_string(&raw_session_info, &current_session_name)
            {
                session_info.creation_time = creation_time;
                session_infos_on_machine.insert(session_name.clone(), session_info);
            }
        }
    }
    session_infos_on_machine
}

fn find_resurrectable_sessions(
    session_infos_on_machine: &BTreeMap<String, SessionInfo>,
    session_info_cache_dir: &Path,
) -> BTreeMap<String, Duration> {
    match fs::read_dir(session_info_cache_dir) {
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
                    let layout_file_name = folder_name.join("session-layout.kdl");
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
                        .and_then(|ctime| ctime.elapsed().ok())
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

#[cfg(feature = "web_server_capability")]
fn query_webserver_via_ipc(web_server_base_url: &str) -> Result<WebServerStatus> {
    let expected_addr =
        parse_base_url(web_server_base_url).context("Failed to parse web server base URL")?;

    let sockets = discover_webserver_sockets().context("Failed to discover web server sockets")?;

    if sockets.is_empty() {
        return Ok(WebServerStatus::Offline);
    }

    for socket_path in sockets {
        let path_str = socket_path.to_str().unwrap_or("");

        match query_webserver_with_response(path_str, InstructionForWebServer::QueryVersion, 500) {
            Ok(WebServerResponse::Version(info)) => {
                let matches_expected =
                    info.ip == expected_addr.ip && info.port == expected_addr.port;

                if !matches_expected {
                    continue;
                }

                if info.version == VERSION {
                    return Ok(WebServerStatus::Online(web_server_base_url.to_string()));
                } else {
                    return Ok(WebServerStatus::DifferentVersion(info.version));
                }
            },
            Err(_) => continue,
        }
    }

    Ok(WebServerStatus::Offline)
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    use tempfile::tempdir;
    use zellij_utils::data::SessionInfo;

    fn make_socket(dir: &std::path::Path, name: &str) -> UnixListener {
        UnixListener::bind(dir.join(name)).expect("bind unix socket")
    }

    fn write_metadata(info_dir: &std::path::Path, session: &str, info: &SessionInfo) {
        let folder = info_dir.join(session);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-metadata.kdl"), info.to_string()).unwrap();
    }

    fn write_layout(info_dir: &std::path::Path, session: &str) {
        let folder = info_dir.join(session);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-layout.kdl"), "layout { }").unwrap();
    }

    #[test]
    fn scan_session_list_returns_empty_when_no_peers() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        let (live, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert!(live.is_empty());
        assert!(resurrectable.is_empty());
    }

    #[test]
    fn scan_session_list_finds_peer_from_socket_and_metadata() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        let peer = "peer-alpha";
        let _listener = make_socket(sock_dir.path(), peer);
        write_metadata(info_dir.path(), peer, &SessionInfo::new(peer.to_string()));

        let (live, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert_eq!(live.len(), 1);
        assert!(live.contains_key(peer));
        assert!(resurrectable.is_empty());
    }

    #[test]
    fn scan_session_list_finds_resurrectable_from_orphan_metadata() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        write_layout(info_dir.path(), "dead-beta");

        let (live, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert!(live.is_empty());
        assert_eq!(resurrectable.len(), 1);
        assert!(resurrectable.contains_key("dead-beta"));
    }

    #[test]
    fn scan_session_list_separates_live_from_resurrectable() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        for name in ["live-a", "live-b", "live-c"] {
            let _listener = make_socket(sock_dir.path(), name);
            write_metadata(info_dir.path(), name, &SessionInfo::new(name.to_string()));
            std::mem::forget(_listener);
        }
        for name in ["dead-a", "dead-b"] {
            write_layout(info_dir.path(), name);
        }

        let (live, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert_eq!(live.len(), 3);
        assert_eq!(resurrectable.len(), 2);
        for name in ["live-a", "live-b", "live-c"] {
            assert!(!resurrectable.contains_key(name));
        }
    }
}
