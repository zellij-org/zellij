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

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::panes::PaneId;
use crate::plugins::{PluginId, PluginInstruction};
use crate::pty::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::thread_bus::Bus;
use crate::{ClientId, ServerInstruction};

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
    RenderToClientsNow,
    QueryZellijWebServerStatus,
    ClearHelpText {
        client_id: ClientId,
    },
    RevealPopupAfter {
        plugin_id: u32,
        delay_ms: u64,
    },
    ClearCommandOutputFlash {
        pane_id: PaneId,
    },
    FlashPaneBell(Vec<PaneId>),
    StopFlashPaneBell(Vec<PaneId>),
    FlashTabBell(usize),     // usize = tab_id
    StopFlashTabBell(usize), // usize = tab_id
    StartNestedGuestPing(PaneId),
    StopNestedGuestPing(PaneId),
    TrimAllocator,
    SessionSuggestions(crate::session_suggestions::SessionSuggestionsJob),
    SessionPreview {
        plugin_id: PluginId,
        client_id: ClientId,
        session_name: String,
        tab_index: Option<usize>,
        pane_id: Option<(u32, bool)>,
    },
    SavedSessionPreview {
        plugin_id: PluginId,
        client_id: ClientId,
        session_name: String,
    },
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
            BackgroundJob::RenderToClients | BackgroundJob::RenderToClientsNow => {
                BackgroundJobContext::RenderToClients
            },
            BackgroundJob::HighlightPanesWithMessage(..) => {
                BackgroundJobContext::HighlightPanesWithMessage
            },
            BackgroundJob::QueryZellijWebServerStatus => {
                BackgroundJobContext::QueryZellijWebServerStatus
            },
            BackgroundJob::ClearHelpText { .. } => BackgroundJobContext::ClearHelpText,
            BackgroundJob::RevealPopupAfter { .. } => BackgroundJobContext::RevealPopup,
            BackgroundJob::ClearCommandOutputFlash { .. } => {
                BackgroundJobContext::ClearCommandOutputFlash
            },
            BackgroundJob::FlashPaneBell(..) => BackgroundJobContext::FlashPaneBell,
            BackgroundJob::StopFlashPaneBell(..) => BackgroundJobContext::StopFlashPaneBell,
            BackgroundJob::FlashTabBell(..) => BackgroundJobContext::FlashTabBell,
            BackgroundJob::StopFlashTabBell(..) => BackgroundJobContext::StopFlashTabBell,
            BackgroundJob::StartNestedGuestPing(..) => BackgroundJobContext::StartNestedGuestPing,
            BackgroundJob::StopNestedGuestPing(..) => BackgroundJobContext::StopNestedGuestPing,
            BackgroundJob::TrimAllocator => BackgroundJobContext::TrimAllocator,
            BackgroundJob::SessionSuggestions(..) => BackgroundJobContext::SessionSuggestions,
            BackgroundJob::SessionPreview { .. } => BackgroundJobContext::SessionPreview,
            BackgroundJob::SavedSessionPreview { .. } => BackgroundJobContext::SavedSessionPreview,
            BackgroundJob::Exit => BackgroundJobContext::Exit,
        }
    }
}

static LONG_FLASH_DURATION_MS: u64 = 1000;
static TRIM_ALLOCATOR_DEBOUNCE_MS: u64 = 2000;
static STARTUP_TRIM_ALLOCATOR_DELAY_MS: u64 = 10000;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn trim_allocator() {
    unsafe {
        libc::malloc_trim(0);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn trim_allocator() {}
static FLASH_DURATION_MS: u64 = 400; // Doherty threshold
static PLUGIN_ANIMATION_OFFSET_DURATION_MD: u64 = 500;
static SESSION_METADATA_WRITE_INTERVAL_MS: u64 = 1000;
static UPDATE_AND_REPORT_CWDS_INTERVAL_MS: u64 = 1000;
static DEFAULT_SERIALIZATION_INTERVAL: u64 = 60000;
pub const RENDER_QUIET_PERIOD: Duration = Duration::from_millis(1);
pub const RENDER_MAX_DELAY: Duration = Duration::from_millis(8);

static IMMEDIATE_RENDER_REQUESTS: AtomicUsize = AtomicUsize::new(0);

pub fn immediate_render_requests() -> usize {
    IMMEDIATE_RENDER_REQUESTS.load(Ordering::Relaxed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingRender {
    first_request: Instant,
    last_request: Instant,
    immediate_at: Option<Instant>,
}

impl PendingRender {
    fn due(&self) -> Instant {
        let due =
            (self.last_request + RENDER_QUIET_PERIOD).min(self.first_request + RENDER_MAX_DELAY);
        match self.immediate_at {
            Some(immediate_at) => due.min(immediate_at),
            None => due,
        }
    }
}

#[derive(Debug, Default)]
struct RenderSchedule {
    pending: Option<PendingRender>,
    exiting: bool,
}

impl RenderSchedule {
    fn request(&mut self, now: Instant) -> bool {
        match self.pending.as_mut() {
            Some(pending) => {
                pending.last_request = now;
                false
            },
            None => {
                self.pending = Some(PendingRender {
                    first_request: now,
                    last_request: now,
                    immediate_at: None,
                });
                true
            },
        }
    }

    fn request_now(&mut self, now: Instant) {
        match self.pending.as_mut() {
            Some(pending) => {
                pending.last_request = now;
                pending.immediate_at = Some(now);
            },
            None => {
                self.pending = Some(PendingRender {
                    first_request: now,
                    last_request: now,
                    immediate_at: Some(now),
                });
            },
        }
    }

    fn step(&mut self, now: Instant) -> RenderStep {
        if self.exiting {
            return RenderStep::Exit;
        }
        match self.pending {
            None => RenderStep::Idle,
            Some(pending) if now >= pending.due() => {
                self.pending = None;
                RenderStep::Render
            },
            Some(pending) => RenderStep::WaitFor(pending.due() - now),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum RenderStep {
    Render,
    WaitFor(Duration),
    Idle,
    Exit,
}

type SharedRenderSchedule = Arc<(Mutex<RenderSchedule>, std::sync::Condvar)>;

fn spawn_render_scheduler(senders: crate::thread_bus::ThreadSenders) -> SharedRenderSchedule {
    let schedule: SharedRenderSchedule = Arc::new((
        Mutex::new(RenderSchedule::default()),
        std::sync::Condvar::new(),
    ));
    let _ = thread::Builder::new().name("repaint".to_string()).spawn({
        let schedule = schedule.clone();
        move || {
            let (lock, wake) = &*schedule;
            let mut state = lock.lock().unwrap();
            loop {
                match state.step(Instant::now()) {
                    RenderStep::Exit => break,
                    RenderStep::Idle => {
                        state = wake.wait(state).unwrap();
                    },
                    RenderStep::WaitFor(duration) => {
                        state = wake.wait_timeout(state, duration).unwrap().0;
                    },
                    RenderStep::Render => {
                        drop(state);
                        if senders
                            .send_to_screen(ScreenInstruction::RenderToClients)
                            .is_err()
                        {
                            break;
                        }
                        state = lock.lock().unwrap();
                    },
                }
            }
        }
    });
    schedule
}

static HELP_TEXT_DEBOUNCE_DURATION: u64 = 5000;
static COMMAND_OUTPUT_FLASH_DURATION_MS: u64 = 400;

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
    let render_schedule = spawn_render_scheduler(bus.senders.clone());
    let session_suggestions =
        crate::session_suggestions::spawn_session_suggestions_service(bus.senders.clone());
    let pending_help_text_clear: Arc<Mutex<HashMap<ClientId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let pending_command_output_flash_clear: Arc<Mutex<HashMap<PaneId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut flashing_pane_bells: HashMap<PaneId, Arc<AtomicBool>> = HashMap::new();
    let mut flashing_tab_bells: HashMap<usize, Arc<AtomicBool>> = HashMap::new();
    let mut nested_guest_pings: HashMap<PaneId, Arc<AtomicBool>> = HashMap::new();
    let pending_allocator_trim: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));

    let http_client = HttpClient::builder()
        // TODO: timeout?
        .redirect_policy(RedirectPolicy::Follow)
        .build()
        .ok();
    // We needn't do anything with the runtime, but it should exist at this point.
    let runtime = crate::global_async_runtime::get_tokio_runtime();

    runtime.spawn(async move {
        tokio::time::sleep(Duration::from_millis(STARTUP_TRIM_ALLOCATOR_DELAY_MS)).await;
        trim_allocator();
    });

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
                        let output = run_background_command(BackgroundCommand {
                            program: command,
                            args,
                            env: env_variables,
                            cwd: Some(cwd),
                            input: None,
                            timeout: None,
                        })
                        .await;
                        match output {
                            Ok(output) => {
                                let stdout = output.stdout;
                                let stderr = output.stderr;
                                let exit_code = output.exit_code;
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
                let (lock, wake) = &*render_schedule;
                if lock.lock().unwrap().request(Instant::now()) {
                    wake.notify_one();
                }
            },
            BackgroundJob::RenderToClientsNow => {
                IMMEDIATE_RENDER_REQUESTS.fetch_add(1, Ordering::Relaxed);
                let (lock, wake) = &*render_schedule;
                lock.lock().unwrap().request_now(Instant::now());
                wake.notify_one();
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
            BackgroundJob::RevealPopupAfter {
                plugin_id,
                delay_ms,
            } => {
                runtime.spawn({
                    let senders = bus.senders.clone();
                    async move {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        let _ = senders.send_to_screen(ScreenInstruction::RevealPopup(plugin_id));
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
            BackgroundJob::ClearCommandOutputFlash { pane_id } => {
                let should_spawn = {
                    let mut pending = pending_command_output_flash_clear.lock().unwrap();
                    let current_time = Instant::now();
                    let should_spawn = !pending.contains_key(&pane_id);
                    pending.insert(pane_id, current_time);
                    should_spawn
                };

                if should_spawn {
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let pending = pending_command_output_flash_clear.clone();
                        let flash_duration =
                            Duration::from_millis(COMMAND_OUTPUT_FLASH_DURATION_MS);
                        async move {
                            tokio::time::sleep(flash_duration).await;
                            loop {
                                let next_sleep_duration = {
                                    let mut pending = pending.lock().unwrap();
                                    match pending.get(&pane_id) {
                                        Some(&last_flash_time) => {
                                            let time_since_flash =
                                                Instant::now().duration_since(last_flash_time);
                                            if time_since_flash >= flash_duration {
                                                pending.remove(&pane_id);
                                                None
                                            } else {
                                                Some(
                                                    flash_duration.saturating_sub(time_since_flash),
                                                )
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
                                            ServerInstruction::ClearCommandOutputFlash(pane_id),
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
            BackgroundJob::StartNestedGuestPing(pane_id) => {
                if !nested_guest_pings.contains_key(&pane_id) {
                    let is_pinging = Arc::new(AtomicBool::new(true));
                    nested_guest_pings.insert(pane_id, is_pinging.clone());
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let flag = is_pinging.clone();
                        let ping_interval_ms = crate::nested_guest::ping_interval_ms();
                        async move {
                            while flag.load(Ordering::SeqCst) {
                                let _ = senders.send_to_screen(
                                    ScreenInstruction::NestedGuestPingTick { pane_id },
                                );
                                tokio::time::sleep(Duration::from_millis(ping_interval_ms)).await;
                            }
                        }
                    });
                }
            },
            BackgroundJob::StopNestedGuestPing(pane_id) => {
                if let Some(flag) = nested_guest_pings.remove(&pane_id) {
                    flag.store(false, Ordering::SeqCst);
                }
            },
            BackgroundJob::TrimAllocator => {
                let should_spawn = {
                    let mut pending = pending_allocator_trim.lock().unwrap();
                    let should_spawn = pending.is_none();
                    *pending = Some(Instant::now());
                    should_spawn
                };
                if should_spawn {
                    let pending = pending_allocator_trim.clone();
                    runtime.spawn(async move {
                        let debounce = Duration::from_millis(TRIM_ALLOCATOR_DEBOUNCE_MS);
                        tokio::time::sleep(debounce).await;
                        loop {
                            let remaining = {
                                let mut pending = pending.lock().unwrap();
                                match *pending {
                                    Some(last_request) => {
                                        let elapsed = Instant::now().duration_since(last_request);
                                        if elapsed >= debounce {
                                            *pending = None;
                                            None
                                        } else {
                                            Some(debounce.saturating_sub(elapsed))
                                        }
                                    },
                                    None => return,
                                }
                            };
                            match remaining {
                                Some(duration) => tokio::time::sleep(duration).await,
                                None => {
                                    trim_allocator();
                                    return;
                                },
                            }
                        }
                    });
                }
            },
            BackgroundJob::SessionSuggestions(job) => {
                let _ = session_suggestions.send(job);
            },
            BackgroundJob::SessionPreview {
                plugin_id,
                client_id,
                session_name,
                tab_index,
                pane_id,
            } => {
                let senders = bus.senders.clone();
                thread::spawn(move || {
                    let preview = crate::session_previews::running_session_preview(
                        &session_name,
                        tab_index,
                        pane_id,
                    );
                    let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                        Some(plugin_id),
                        Some(client_id),
                        Event::SessionPreview(preview),
                    )]));
                });
            },
            BackgroundJob::SavedSessionPreview {
                plugin_id,
                client_id,
                session_name,
            } => {
                let senders = bus.senders.clone();
                thread::spawn(move || {
                    let preview = crate::session_previews::saved_session_preview(&session_name);
                    let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                        Some(plugin_id),
                        Some(client_id),
                        Event::SavedSessionPreview(preview),
                    )]));
                });
            },
            BackgroundJob::Exit => {
                let _ = session_suggestions
                    .send(crate::session_suggestions::SessionSuggestionsJob::Exit);
                {
                    let (lock, wake) = &*render_schedule;
                    lock.lock().unwrap().exiting = true;
                    wake.notify_one();
                }
                for loading_plugin in loading_plugins.values() {
                    loading_plugin.store(false, Ordering::SeqCst);
                }
                for nested_guest_ping in nested_guest_pings.values() {
                    nested_guest_ping.store(false, Ordering::SeqCst);
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
    let mut session_infos_on_machine = zellij_utils::sessions::read_live_session_states(
        current_session_name,
        sock_dir,
        session_info_cache_dir,
    );
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

#[cfg(test)]
mod render_schedule_tests {
    use super::*;

    #[test]
    fn a_lone_request_renders_after_the_quiet_period() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        assert!(schedule.request(start));
        assert_eq!(
            schedule.step(start),
            RenderStep::WaitFor(RENDER_QUIET_PERIOD)
        );
        assert_eq!(
            schedule.step(start + RENDER_QUIET_PERIOD),
            RenderStep::Render
        );
        assert_eq!(schedule.step(start + RENDER_QUIET_PERIOD), RenderStep::Idle);
    }

    #[test]
    fn requests_arriving_within_the_quiet_period_postpone_the_render() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        assert!(schedule.request(start));
        let later = start + RENDER_QUIET_PERIOD / 2;
        assert!(!schedule.request(later));
        assert_eq!(
            schedule.step(start + RENDER_QUIET_PERIOD),
            RenderStep::WaitFor(RENDER_QUIET_PERIOD / 2)
        );
        assert_eq!(
            schedule.step(later + RENDER_QUIET_PERIOD),
            RenderStep::Render
        );
    }

    #[test]
    fn continuous_requests_still_render_once_the_maximum_delay_passes() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        let step = RENDER_QUIET_PERIOD / 2;
        let mut now = start;
        schedule.request(now);
        while now < start + RENDER_MAX_DELAY {
            assert_ne!(schedule.step(now), RenderStep::Render);
            now += step;
            schedule.request(now);
        }
        assert_eq!(schedule.step(start + RENDER_MAX_DELAY), RenderStep::Render);
        assert!(schedule.request(now));
    }

    #[test]
    fn exiting_stops_the_scheduler_even_with_a_pending_render() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        schedule.request(start);
        schedule.exiting = true;
        assert_eq!(schedule.step(start + RENDER_MAX_DELAY), RenderStep::Exit);
    }

    #[test]
    fn an_immediate_request_on_an_idle_schedule_renders_at_once() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        schedule.request_now(start);
        assert_eq!(schedule.step(start), RenderStep::Render);
        assert_eq!(schedule.step(start), RenderStep::Idle);
    }

    #[test]
    fn an_immediate_request_makes_a_pending_render_due_at_once() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        assert!(schedule.request(start));
        let later = start + RENDER_QUIET_PERIOD / 4;
        assert_eq!(
            schedule.step(later),
            RenderStep::WaitFor(RENDER_QUIET_PERIOD * 3 / 4)
        );
        schedule.request_now(later);
        assert_eq!(schedule.step(later), RenderStep::Render);
        assert_eq!(schedule.step(later), RenderStep::Idle);
    }

    #[test]
    fn a_normal_request_after_an_immediate_render_waits_for_the_quiet_period_again() {
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        schedule.request_now(start);
        assert_eq!(schedule.step(start), RenderStep::Render);
        let later = start + RENDER_QUIET_PERIOD;
        assert!(schedule.request(later));
        assert_eq!(
            schedule.step(later),
            RenderStep::WaitFor(RENDER_QUIET_PERIOD)
        );
        assert_eq!(
            schedule.step(later + RENDER_QUIET_PERIOD),
            RenderStep::Render
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundCommandOutput {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum BackgroundCommandError {
    Spawn(std::io::Error),
    Io(std::io::Error),
    TimedOut,
}

impl std::fmt::Display for BackgroundCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BackgroundCommandError::Spawn(e) => write!(f, "failed to start command: {}", e),
            BackgroundCommandError::Io(e) => write!(f, "failed to run command: {}", e),
            BackgroundCommandError::TimedOut => write!(f, "command timed out"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BackgroundCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub input: Option<Vec<u8>>,
    pub timeout: Option<Duration>,
}

pub async fn run_background_command(
    command: BackgroundCommand,
) -> std::result::Result<BackgroundCommandOutput, BackgroundCommandError> {
    use tokio::io::AsyncWriteExt;
    let mut process = tokio::process::Command::new(&command.program);
    process
        .args(&command.args)
        .envs(&command.env)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = &command.cwd {
        process.current_dir(cwd);
    }
    if command.input.is_some() {
        process.stdin(std::process::Stdio::piped());
    } else {
        process.stdin(std::process::Stdio::null());
    }
    let mut child = process.spawn().map_err(BackgroundCommandError::Spawn)?;
    if let (Some(input), Some(mut stdin)) = (command.input.clone(), child.stdin.take()) {
        tokio::spawn(async move {
            let _ = stdin.write_all(&input).await;
            let _ = stdin.shutdown().await;
        });
    }
    let output = child.wait_with_output();
    let output = match command.timeout {
        Some(timeout) => match tokio::time::timeout(timeout, output).await {
            Ok(output) => output,
            Err(_) => return Err(BackgroundCommandError::TimedOut),
        },
        None => output.await,
    }
    .map_err(BackgroundCommandError::Io)?;
    Ok(BackgroundCommandOutput {
        exit_code: output.status.code(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

pub fn run_background_command_blocking(
    command: BackgroundCommand,
) -> std::result::Result<BackgroundCommandOutput, BackgroundCommandError> {
    crate::global_async_runtime::get_tokio_runtime().block_on(run_background_command(command))
}
