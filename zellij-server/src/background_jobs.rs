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
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::{Duration, Instant};

use crate::os_input_output::PaneEnv;
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
    RunInBackground {
        command: PathBuf,
        args: Vec<String>,
        cwd: Option<PathBuf>,
        env: PaneEnv,
    },
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
    MirrorTickAfter {
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
        anchor: Option<zellij_utils::data::PreviewAnchor>,
    },
    PeerAttention(crate::attention::peers::PeerAttentionJob),
    SavedSessionPreview {
        plugin_id: PluginId,
        client_id: ClientId,
        session_name: String,
    },
    AttentionIdleCheck(PaneId, u64),
    ReportHostProgramStatus(Option<zellij_utils::attention::HostProgramStatus>),
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
            BackgroundJob::RunInBackground { .. } => BackgroundJobContext::RunInBackground,
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
            BackgroundJob::MirrorTickAfter { .. } => BackgroundJobContext::MirrorTick,
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
            BackgroundJob::PeerAttention(..) => BackgroundJobContext::PeerAttention,
            BackgroundJob::AttentionIdleCheck(..) => BackgroundJobContext::AttentionIdleCheck,
            BackgroundJob::ReportHostProgramStatus(..) => {
                BackgroundJobContext::ReportHostProgramStatus
            },
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
const SESSION_SUGGESTIONS_EXIT_WAIT: Duration = Duration::from_secs(3);
pub const RENDER_QUIET_PERIOD: Duration = Duration::from_millis(4);
pub const RENDER_MAX_DELAY: Duration = Duration::from_millis(10);

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
static HOST_PROGRAM_STATUS_INTERVAL_MS: u64 = 500;
const RUNNING_JOBS_PRUNE_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct FrameColorGenerations {
    current: HashMap<PaneId, u64>,
    next: u64,
}

impl FrameColorGenerations {
    fn begin(&mut self, pane_ids: &[PaneId]) -> Vec<(PaneId, u64)> {
        pane_ids
            .iter()
            .map(|pane_id| {
                self.next = self.next.wrapping_add(1);
                self.current.insert(*pane_id, self.next);
                (*pane_id, self.next)
            })
            .collect()
    }

    fn take_current(&mut self, tokens: &[(PaneId, u64)]) -> Vec<PaneId> {
        let current = tokens
            .iter()
            .filter_map(|(pane_id, generation)| {
                (self.current.get(pane_id) == Some(generation)).then_some(*pane_id)
            })
            .collect::<Vec<_>>();
        for pane_id in &current {
            self.current.remove(pane_id);
        }
        current
    }

    fn invalidate(&mut self, pane_ids: &[PaneId]) {
        for pane_id in pane_ids {
            self.current.remove(pane_id);
        }
    }
}

/// Clear the overrides whose expiry is still the current one for their pane.
///
/// The generation check and the hand-off of the clear to the screen happen
/// under one lock. Releasing the lock in between would let an expiry that
/// passed the check be descheduled while a newer override is started and its
/// highlight sent, so the late clear would remove the newer highlight. With
/// the lock held, a newer override is only started (its generation is
/// bumped) either before the check, which then fails, or after the clear has
/// been queued ahead of the newer highlight.
fn expire_frame_color_overrides(
    generations: &Mutex<FrameColorGenerations>,
    tokens: &[(PaneId, u64)],
    send_clear: impl FnOnce(Vec<PaneId>),
) {
    let mut generations = lock_frame_color_generations(generations);
    let pane_ids = generations.take_current(tokens);
    if !pane_ids.is_empty() {
        send_clear(pane_ids);
    }
}

fn lock_frame_color_generations(
    generations: &Mutex<FrameColorGenerations>,
) -> std::sync::MutexGuard<'_, FrameColorGenerations> {
    generations
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

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
    session_id: String,
    live_sessions: crate::attention::peers::LiveSessions,
) -> Result<()> {
    let err_context = || "failed to write to pty".to_string();
    let mut running_jobs: HashMap<BackgroundJob, Instant> = HashMap::new();
    let mut last_running_jobs_prune = Instant::now();
    let frame_color_generations = Arc::new(Mutex::new(FrameColorGenerations::default()));
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
    let (session_suggestions, mut session_suggestions_thread) =
        crate::session_suggestions::spawn_session_suggestions_service(
            bus.senders.clone(),
            live_sessions.clone(),
        );
    let peer_attention = crate::attention::peers::spawn_peer_attention_service(
        bus.senders.clone(),
        current_session_name.clone(),
        live_sessions,
        session_suggestions.clone(),
    );
    let pending_help_text_clear: Arc<Mutex<HashMap<ClientId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let pending_command_output_flash_clear: Arc<Mutex<HashMap<PaneId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let mut flashing_pane_bells: HashMap<PaneId, Arc<AtomicBool>> = HashMap::new();
    let mut flashing_tab_bells: HashMap<usize, Arc<AtomicBool>> = HashMap::new();
    let mut nested_guest_pings: HashMap<PaneId, Arc<AtomicBool>> = HashMap::new();
    let pending_allocator_trim: Arc<Mutex<Option<Instant>>> = Arc::new(Mutex::new(None));
    let pending_attention_idle_checks: Arc<Mutex<HashMap<PaneId, Instant>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let host_program_status: Arc<
        Mutex<(
            Option<Instant>,
            Option<Option<zellij_utils::attention::HostProgramStatus>>,
        )>,
    > = Arc::new(Mutex::new((None, None)));

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
            // At most one pending serialization: while a queued pass
            // hasn't run, further ticks are redundant and must not
            // accumulate instructions behind a stalled screen thread.
            let pending = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mut ticker =
                tokio::time::interval(std::time::Duration::from_millis(serialization_ms));
            ticker.tick().await;
            loop {
                ticker.tick().await;
                if !pending.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    let _ = senders.send_to_screen(
                        ScreenInstruction::SerializeLayoutForResurrection(pending.clone()),
                    );
                }
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

    let session_metadata_writer_stopped = Arc::new(Mutex::new(false));
    if !disable_session_metadata {
        let current_session_name = current_session_name.clone();
        let current_session_info = current_session_info.clone();
        let current_session_layout = current_session_layout.clone();
        let session_id = session_id.clone();
        let stopped = session_metadata_writer_stopped.clone();
        let senders = bus.senders.clone();
        runtime.spawn(async move {
            // Captured once. A later clear() mints a new incarnation for the
            // next owner; adopting it here would reauthorize this writer.
            let session_incarnation = if session_id.is_empty() {
                None
            } else {
                match zellij_utils::sessions::read_session_incarnation(&session_id) {
                    Ok(incarnation) => Some(incarnation),
                    Err(error) => {
                        log::error!(
                            "Session {} has no publication incarnation ({}); metadata will not be published",
                            session_id,
                            error
                        );
                        None
                    },
                }
            };
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(
                SESSION_METADATA_WRITE_INTERVAL_MS,
            ));
            ticker.tick().await;
            // path -> hash of the bytes we last published there
            let mut published: HashMap<PathBuf, u64> = HashMap::new();
            let mut reported_saved = false;
            loop {
                ticker.tick().await;
                if *stopped.lock().unwrap() {
                    return;
                }
                let name = current_session_name.lock().unwrap().clone();
                if name.is_empty() || session_id.is_empty() {
                    continue;
                }
                let (writes, hashes, current_layout) = {
                    let info = current_session_info.lock().unwrap();
                    let layout = current_session_layout.lock().unwrap();
                    let (writes, hashes) =
                        collect_dirty_session_state_writes(&session_id, &info, &layout, &published);
                    (writes, hashes, layout.0.clone())
                };
                // Files that are no longer part of the session state are not
                // tracked any further.
                published.retain(|path, _| hashes.contains_key(path));
                // A session whose layout has not been serialized yet (the first
                // one arrives after a delay) has only its metadata on disk,
                // which is not a resurrectable save.
                let layout_present = layout_is_part_of_state(&session_id, &hashes);
                if writes.is_empty() {
                    // Every file was last published with the current contents
                    // and still has that size on disk, so the state counts as
                    // saved now.
                    if layout_present && !reported_saved {
                        report_session_saved(&senders);
                        reported_saved = true;
                    } else if !layout_present {
                        reported_saved = false;
                    }
                    continue;
                }
                reported_saved = false;
                let Some(incarnation) = session_incarnation else {
                    continue;
                };
                // The per-session lock covers the eligibility check, staging
                // (including create_dir_all), and persist. Exit waits on
                // `stopped` so it does not delete metadata mid-publish.
                let keep_running = {
                    let stopped_guard = stopped.lock().unwrap();
                    if *stopped_guard {
                        false
                    } else {
                        match stage_and_commit_session_generation(
                            &session_id,
                            incarnation,
                            writes,
                            &hashes,
                            &mut published,
                            &current_layout,
                            layout_present,
                        ) {
                            Ok(all_persisted) => {
                                if all_persisted && layout_present {
                                    report_session_saved(&senders);
                                    reported_saved = true;
                                }
                            },
                            Err(error)
                                if zellij_utils::sessions::is_session_publication_fenced(
                                    &error,
                                ) =>
                            {
                                log::info!(
                                    "Refusing to publish session state for deleted session id {}: {}",
                                    session_id,
                                    error
                                );
                            },
                            Err(error) => {
                                log::error!(
                                    "Failed to publish session state for session id {}: {}",
                                    session_id,
                                    error
                                );
                            },
                        }
                        true
                    }
                };
                if !keep_running {
                    return;
                }
            }
        });
    }

    loop {
        if last_running_jobs_prune.elapsed() >= RUNNING_JOBS_PRUNE_INTERVAL {
            running_jobs.retain(|_, started| {
                started.elapsed() <= Duration::from_millis(LONG_FLASH_DURATION_MS)
            });
            last_running_jobs_prune = Instant::now();
        }
        let (event, mut err_ctx) = bus.recv().with_context(err_context)?;
        err_ctx.add_call(ContextType::BackgroundJob((&event).into()));
        match event {
            BackgroundJob::DisplayPaneError(pane_ids, text) => {
                if job_already_running(
                    BackgroundJob::DisplayPaneError(pane_ids.clone(), text.clone()),
                    &mut running_jobs,
                    LONG_FLASH_DURATION_MS,
                ) {
                    continue;
                }
                let frame_color_tokens =
                    lock_frame_color_generations(&frame_color_generations).begin(&pane_ids);
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let frame_color_generations = frame_color_generations.clone();
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
                        expire_frame_color_overrides(
                            &frame_color_generations,
                            &frame_color_tokens,
                            |pane_ids| {
                                let _ = senders.send_to_screen(
                                    ScreenInstruction::ClearPaneFrameColorOverride(pane_ids),
                                );
                            },
                        );
                    }
                });
            },
            BackgroundJob::AnimatePluginLoading(pid) => {
                if loading_plugins.contains_key(&pid) {
                    continue;
                }
                let loading_plugin = Arc::new(AtomicBool::new(true));
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
                // The save timestamp is reported by the metadata writer once
                // the layout has actually been persisted to disk.
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
            BackgroundJob::RunInBackground {
                command,
                args,
                cwd,
                env,
            } => {
                runtime.spawn(run_in_background(command, args, cwd, env));
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
                if job_already_running(
                    BackgroundJob::HighlightPanesWithMessage(pane_ids.clone(), text.clone()),
                    &mut running_jobs,
                    FLASH_DURATION_MS,
                ) {
                    continue;
                }
                let frame_color_tokens =
                    lock_frame_color_generations(&frame_color_generations).begin(&pane_ids);
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let frame_color_generations = frame_color_generations.clone();
                    async move {
                        let _ = senders.send_to_screen(
                            ScreenInstruction::AddHighlightPaneFrameColorOverride(
                                pane_ids.clone(),
                                Some(text),
                            ),
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(FLASH_DURATION_MS))
                            .await;
                        expire_frame_color_overrides(
                            &frame_color_generations,
                            &frame_color_tokens,
                            |pane_ids| {
                                let _ = senders.send_to_screen(
                                    ScreenInstruction::ClearPaneFrameColorOverride(pane_ids),
                                );
                            },
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
            BackgroundJob::MirrorTickAfter { delay_ms } => {
                runtime.spawn({
                    let senders = bus.senders.clone();
                    async move {
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        let _ = senders.send_to_screen(ScreenInstruction::MirrorTick);
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
            BackgroundJob::AttentionIdleCheck(pane_id, delay_ms) => {
                let should_spawn = {
                    let mut pending = pending_attention_idle_checks.lock().unwrap();
                    let deadline = Instant::now() + Duration::from_millis(delay_ms);
                    let should_spawn = !pending.contains_key(&pane_id);
                    pending.insert(pane_id, deadline);
                    should_spawn
                };
                if should_spawn {
                    runtime.spawn({
                        let senders = bus.senders.clone();
                        let pending = pending_attention_idle_checks.clone();
                        async move {
                            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                            loop {
                                let next_sleep_duration = {
                                    let mut pending = pending.lock().unwrap();
                                    match pending.get(&pane_id) {
                                        Some(&deadline) => {
                                            let now = Instant::now();
                                            if now >= deadline {
                                                pending.remove(&pane_id);
                                                None
                                            } else {
                                                Some(deadline.saturating_duration_since(now))
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
                                        let _ = senders.send_to_screen(
                                            ScreenInstruction::AttentionIdleCheck(pane_id),
                                        );
                                        break;
                                    },
                                }
                            }
                        }
                    });
                }
            },
            BackgroundJob::ReportHostProgramStatus(status) => {
                let interval = Duration::from_millis(HOST_PROGRAM_STATUS_INTERVAL_MS);
                let send_now = {
                    let mut state = host_program_status.lock().unwrap();
                    let now = Instant::now();
                    let can_send = state
                        .0
                        .map(|last| now.duration_since(last) >= interval)
                        .unwrap_or(true);
                    let task_pending = state.1.is_some();
                    if can_send && !task_pending {
                        state.0 = Some(now);
                        Some(status)
                    } else {
                        let should_spawn = !task_pending;
                        state.1 = Some(status);
                        if should_spawn {
                            let wait = state
                                .0
                                .map(|last| interval.saturating_sub(now.duration_since(last)))
                                .unwrap_or_default();
                            runtime.spawn({
                                let senders = bus.senders.clone();
                                let state = host_program_status.clone();
                                async move {
                                    tokio::time::sleep(wait).await;
                                    let status = {
                                        let mut state = state.lock().unwrap();
                                        state.0 = Some(Instant::now());
                                        state.1.take()
                                    };
                                    if let Some(status) = status {
                                        let _ = senders.send_to_server(
                                            ServerInstruction::HostProgramStatus(status),
                                        );
                                    }
                                }
                            });
                        }
                        None
                    }
                };
                if let Some(status) = send_now {
                    let _ = bus
                        .senders
                        .send_to_server(ServerInstruction::HostProgramStatus(status));
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
                let frame_color_tokens =
                    lock_frame_color_generations(&frame_color_generations).begin(&pane_ids);
                for &pane_id in &pane_ids {
                    flashing_pane_bells.insert(pane_id, is_flashing.clone());
                }
                runtime.spawn({
                    let senders = bus.senders.clone();
                    let pane_ids_clone = pane_ids.clone();
                    let flag = is_flashing.clone();
                    let frame_color_generations = frame_color_generations.clone();
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
                            expire_frame_color_overrides(
                                &frame_color_generations,
                                &frame_color_tokens,
                                |pane_ids| {
                                    let _ = senders.send_to_screen(
                                        ScreenInstruction::ClearPaneFrameColorOverride(pane_ids),
                                    );
                                },
                            );
                        }
                    }
                });
            },
            BackgroundJob::StopFlashPaneBell(pane_ids) => {
                lock_frame_color_generations(&frame_color_generations).invalidate(&pane_ids);
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
            BackgroundJob::PeerAttention(job) => {
                let _ = peer_attention.send(job);
            },
            BackgroundJob::SessionPreview {
                plugin_id,
                client_id,
                session_name,
                tab_index,
                pane_id,
                anchor,
            } => {
                let senders = bus.senders.clone();
                let is_current = *current_session_name.lock().unwrap() == session_name;
                thread::spawn(move || {
                    let preview = if is_current {
                        crate::session_previews::current_session_preview(
                            &senders,
                            &session_name,
                            tab_index,
                            pane_id,
                            anchor,
                        )
                    } else {
                        crate::session_previews::running_session_preview(
                            &session_name,
                            tab_index,
                            pane_id,
                            anchor,
                        )
                    };
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
                *session_metadata_writer_stopped.lock().unwrap() = true;
                let _ = session_suggestions
                    .send(crate::session_suggestions::SessionSuggestionsJob::Exit);
                if let Some(handle) = session_suggestions_thread.take() {
                    crate::session_suggestions::join_with_deadline(
                        handle,
                        SESSION_SUGGESTIONS_EXIT_WAIT,
                    );
                }
                let _ = peer_attention.send(crate::attention::peers::PeerAttentionJob::Exit);
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

                // Remove the metadata cache file (keyed by session id) so the
                // session is no longer considered live; the layout file is kept
                // for resurrection.
                if !session_id.is_empty() {
                    let _ = std::fs::remove_file(session_info_cache_file_name(&session_id));
                }
                return Ok(());
            },
        }
    }
}

fn job_already_running(
    job: BackgroundJob,
    running_jobs: &mut HashMap<BackgroundJob, Instant>,
    cooldown_ms: u64,
) -> bool {
    match running_jobs.get_mut(&job) {
        Some(current_running_job_start_time) => {
            if current_running_job_start_time.elapsed() > Duration::from_millis(cooldown_ms) {
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

fn content_hash(contents: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(contents);
    hasher.finish()
}

/// Whether a resurrection layout is among the artifacts of the current
/// session state (an empty layout is not written, so it is not).
fn layout_is_part_of_state(session_id: &str, hashes: &HashMap<PathBuf, u64>) -> bool {
    hashes.contains_key(&session_layout_cache_file_name(session_id))
}

fn report_session_saved(senders: &crate::thread_bus::ThreadSenders) {
    let timestamp_millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let _ = senders.send_to_plugin(PluginInstruction::UpdateSessionSaveTime(timestamp_millis));
}

/// Session state files to (re)write, and the content hash of every current
/// artifact by path.
type DirtySessionStateWrites = (Vec<(PathBuf, Vec<u8>)>, HashMap<PathBuf, u64>);

// Returns the files whose current contents differ from what was last published
// (or that are missing/truncated on disk), plus a hash of every current
// artifact for bookkeeping.
fn collect_dirty_session_state_writes(
    session_id: &str,
    current_session_info: &SessionInfo,
    current_session_layout: &(String, BTreeMap<String, String>),
    published: &HashMap<PathBuf, u64>,
) -> DirtySessionStateWrites {
    let (current_session_layout, layout_files_to_write) = current_session_layout;
    let session_info_folder = session_info_folder_for_session(session_id);
    let mut writes = Vec::new();
    let mut hashes = HashMap::new();
    let mut consider = |path: PathBuf, contents: &[u8]| {
        let hash = content_hash(contents);
        hashes.insert(path.clone(), hash);
        let already_published = published.get(&path) == Some(&hash)
            && fs::metadata(&path)
                .map(|meta| meta.len() == contents.len() as u64)
                .unwrap_or(false);
        if !already_published {
            writes.push((path, contents.to_vec()));
        }
    };
    consider(
        session_info_cache_file_name(session_id),
        current_session_info.to_string().as_bytes(),
    );
    if !current_session_layout.is_empty() {
        // The layout names these files, so publish them before it. If one of
        // them fails, the layout stays unpublished and the next tick retries.
        for (external_file_name, external_file_contents) in layout_files_to_write {
            consider(
                session_info_folder.join(external_file_name),
                external_file_contents.as_bytes(),
            );
        }
        consider(
            session_layout_cache_file_name(session_id),
            current_session_layout.as_bytes(),
        );
    }
    (writes, hashes)
}

/// Remove pane-content files that no committed layout can reference any more.
///
/// Names are content-addressed, so a failed publication can only leave
/// unreferenced files behind. The layout currently on disk is always part of
/// the retained set and the immediately preceding committed generation is kept
/// for readers that resolved it just before publication. Readers that outlive
/// that predecessor verify that every referenced file still loaded and restart
/// from the current layout when one was reclaimed.
fn prune_stale_pane_content_files(
    session_info_folder: &Path,
    layout_path: &Path,
    previous_layout: Option<&str>,
    current_layout: &str,
) {
    let mut retained = zellij_utils::session_serialization::session_layout_referenced_content_files(
        current_layout,
    );
    if let Some(previous_layout) = previous_layout {
        retained.extend(
            zellij_utils::session_serialization::session_layout_referenced_content_files(
                previous_layout,
            ),
        );
    }
    if let Ok(on_disk) = fs::read_to_string(layout_path) {
        retained.extend(
            zellij_utils::session_serialization::session_layout_referenced_content_files(&on_disk),
        );
    }
    let Ok(entries) = fs::read_dir(session_info_folder) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.starts_with(zellij_utils::session_serialization::PANE_CONTENTS_FILE_PREFIX)
            || retained.contains(file_name)
        {
            continue;
        }
        match fs::remove_file(&path) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => log::debug!(
                "Failed to remove stale pane content file {}: {:?}",
                path.display(),
                error
            ),
        }
    }
}

/// Per-session publication lock, then `body`.
///
/// `after_check` runs after the writer is admitted and before `body`, still
/// holding the lock. `create_dir_all` belongs in `body`. If the fence is
/// discovered after this attempt created the session directory, that directory
/// is removed and the fenced error is returned.
fn with_session_publication<T>(
    session_id: &str,
    incarnation: u128,
    after_check: &dyn Fn(),
    body: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let _lock = zellij_utils::sessions::lock_session_publication(session_id)?;
    zellij_utils::sessions::ensure_session_publication_allowed(session_id, incarnation)?;
    after_check();
    let folder = session_info_folder_for_session(session_id);
    let created_folder = !folder.exists();
    let result = body();
    if let Err(error) =
        zellij_utils::sessions::ensure_session_publication_allowed(session_id, incarnation)
    {
        remove_session_folder_created_by_this_attempt(&folder, created_folder);
        return Err(error);
    }
    result
}

fn remove_session_folder_created_by_this_attempt(folder: &Path, created_folder: bool) {
    if !created_folder {
        return;
    }
    match fs::remove_dir_all(folder) {
        Ok(()) => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => log::error!(
            "Failed to remove session folder recreated by a fenced writer {}: {:?}",
            folder.display(),
            error
        ),
    }
}

fn stage_and_commit_session_generation(
    session_id: &str,
    incarnation: u128,
    writes: Vec<(PathBuf, Vec<u8>)>,
    hashes: &HashMap<PathBuf, u64>,
    published: &mut HashMap<PathBuf, u64>,
    current_layout: &str,
    prune: bool,
) -> std::io::Result<bool> {
    with_session_publication(session_id, incarnation, &|| {}, || {
        let mut stage_failed = false;
        let mut staged = Vec::with_capacity(writes.len());
        for (path, contents) in &writes {
            match stage_atomic_write(path, contents) {
                Ok(file) => staged.push((path.clone(), file)),
                Err(error) => {
                    stage_failed = true;
                    log::error!(
                        "Failed to stage session state file {}: {:?}",
                        path.display(),
                        error
                    );
                },
            }
        }
        if let Err(error) =
            zellij_utils::sessions::ensure_session_publication_allowed(session_id, incarnation)
        {
            drop(staged);
            return Err(error);
        }
        // Predecessor layout is read under the same lock as the persist, so a
        // concurrent writer cannot commit a generation we then fail to retain.
        let session_info_folder = session_info_folder_for_session(session_id);
        let layout_path = session_layout_cache_file_name(session_id);
        let previous_layout = fs::read_to_string(&layout_path).ok();
        let all_persisted =
            publish_staged_session_state(&layout_path, staged, hashes, published, !stage_failed);
        if all_persisted && prune {
            prune_stale_pane_content_files(
                &session_info_folder,
                &layout_path,
                previous_layout.as_deref(),
                current_layout,
            );
        }
        Ok(all_persisted)
    })
}

fn commit_staged_session_generation(
    session_id: &str,
    incarnation: u128,
    session_info_folder: &Path,
    layout_path: &Path,
    staged: Vec<(PathBuf, tempfile::NamedTempFile)>,
    hashes: &HashMap<PathBuf, u64>,
    published: &mut HashMap<PathBuf, u64>,
    dependencies_persisted: bool,
    current_layout: &str,
    prune: bool,
) -> std::io::Result<bool> {
    with_session_publication(session_id, incarnation, &|| {}, || {
        if let Err(error) =
            zellij_utils::sessions::ensure_session_publication_allowed(session_id, incarnation)
        {
            return Err(error);
        }
        let previous_layout = fs::read_to_string(layout_path).ok();
        let all_persisted = publish_staged_session_state(
            layout_path,
            staged,
            hashes,
            published,
            dependencies_persisted,
        );
        if all_persisted && prune {
            prune_stale_pane_content_files(
                session_info_folder,
                layout_path,
                previous_layout.as_deref(),
                current_layout,
            );
        }
        Ok(all_persisted)
    })
}

/// Persist session metadata and layout for one captured incarnation.
///
/// `incarnation` is the value read when the writer was created. It is not
/// refreshed from disk. The per-session lock is held from the eligibility
/// check through staging and persist. A refusal is
/// `ErrorKind::PermissionDenied` carrying
/// `zellij_utils::sessions::SessionPublicationFenced`
/// (`is_session_publication_fenced`), never `Ok(())`.
pub fn write_session_state_to_disk(
    session_id: String,
    incarnation: u128,
    current_session_info: SessionInfo,
    current_session_layout: (String, BTreeMap<String, String>),
) -> std::io::Result<()> {
    write_session_state_to_disk_with_hook(
        session_id,
        incarnation,
        current_session_info,
        current_session_layout,
        &|| {},
    )
}

fn write_session_state_to_disk_with_hook(
    session_id: String,
    incarnation: u128,
    current_session_info: SessionInfo,
    current_session_layout: (String, BTreeMap<String, String>),
    after_check: &dyn Fn(),
) -> std::io::Result<()> {
    with_session_publication(&session_id, incarnation, after_check, || {
        write_admitted_session_state(
            &session_id,
            incarnation,
            &current_session_info,
            &current_session_layout,
        )
    })
}

fn write_admitted_session_state(
    session_id: &str,
    incarnation: u128,
    current_session_info: &SessionInfo,
    current_session_layout: &(String, BTreeMap<String, String>),
) -> std::io::Result<()> {
    let (layout_text, layout_files_to_write) = current_session_layout;
    let metadata = current_session_info.to_string();
    let metadata_path = session_info_cache_file_name(session_id);
    let session_info_folder = session_info_folder_for_session(session_id);
    let layout_path = session_layout_cache_file_name(session_id);
    let mut first_error = None;
    let mut staged: Vec<(PathBuf, tempfile::NamedTempFile)> = Vec::new();
    let mut stage_if_changed = |path: PathBuf, contents: &[u8]| -> bool {
        if !file_content_changed(&path, contents) {
            return true;
        }
        match stage_atomic_write(&path, contents) {
            Ok(file) => {
                staged.push((path, file));
                true
            },
            Err(error) => {
                log::error!(
                    "Failed to stage session state file {}: {:?}",
                    path.display(),
                    error
                );
                first_error.get_or_insert(error);
                false
            },
        }
    };
    let _ = stage_if_changed(metadata_path.clone(), metadata.as_bytes());

    let mut content_persisted = true;
    let mut previous_layout = None;
    let mut staged_layout = None;
    if !layout_text.is_empty() {
        previous_layout = fs::read_to_string(&layout_path).ok();
        for (external_file_name, external_file_contents) in layout_files_to_write {
            content_persisted &= stage_if_changed(
                session_info_folder.join(external_file_name),
                external_file_contents.as_bytes(),
            );
        }
        // The layout names these files, so never publish it after one of them
        // failed; the next save retries the whole set.
        if content_persisted && file_content_changed(&layout_path, layout_text.as_bytes()) {
            match stage_atomic_write(&layout_path, layout_text.as_bytes()) {
                Ok(file) => staged_layout = Some(file),
                Err(error) => {
                    log::error!(
                        "Failed to stage session state file {}: {:?}",
                        layout_path.display(),
                        error
                    );
                    first_error.get_or_insert(error);
                    content_persisted = false;
                },
            }
        }
    }

    if let Err(error) =
        zellij_utils::sessions::ensure_session_publication_allowed(session_id, incarnation)
    {
        drop(staged);
        drop(staged_layout);
        return Err(error);
    }

    for (path, file) in staged {
        if let Err(error) = persist_atomic_write(file, &path) {
            log::error!(
                "Failed to write session state file {}: {:?}",
                path.display(),
                error
            );
            // Metadata failure must not suppress the layout. Content files do.
            if path != metadata_path {
                content_persisted = false;
            }
            first_error.get_or_insert(error);
        }
    }
    if content_persisted && !layout_text.is_empty() {
        if let Some(layout_file) = staged_layout {
            // Replacing the file gives it a new creation time; keep the session's own.
            zellij_utils::sessions::record_session_layout_creation_time(&layout_path);
            if let Err(error) = persist_atomic_write(layout_file, &layout_path) {
                log::error!(
                    "Failed to write session state file {}: {:?}",
                    layout_path.display(),
                    error
                );
                first_error.get_or_insert(error);
            }
        }
        prune_stale_pane_content_files(
            &session_info_folder,
            &session_layout_cache_file_name(session_id),
            previous_layout.as_deref(),
            layout_text,
        );
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn stage_atomic_write(path: &Path, contents: &[u8]) -> std::io::Result<tempfile::NamedTempFile> {
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "file path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)?;
    let mut temporary_file = tempfile::Builder::new()
        .prefix(".zellij-session-")
        .tempfile_in(parent)?;
    temporary_file.write_all(contents)?;
    temporary_file.as_file().sync_all()?;
    Ok(temporary_file)
}

fn write_file_atomically(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let file = stage_atomic_write(path, contents)?;
    persist_atomic_write(file, path)
}

fn persist_atomic_write(file: tempfile::NamedTempFile, path: &Path) -> std::io::Result<()> {
    file.persist(path).map_err(|error| error.error)?;
    sync_parent_directory(path)
}

/// Publish staged session artifacts. `layout_path` is written last and only
/// after every artifact before it succeeded, so a layout never names content
/// that failed to publish. Returns whether every staged file was published.
fn publish_staged_session_state(
    layout_path: &Path,
    staged: Vec<(PathBuf, tempfile::NamedTempFile)>,
    hashes: &HashMap<PathBuf, u64>,
    published: &mut HashMap<PathBuf, u64>,
    mut dependencies_persisted: bool,
) -> bool {
    let mut all_persisted = dependencies_persisted;
    for (path, file) in staged {
        if path == layout_path && !dependencies_persisted {
            all_persisted = false;
            continue;
        }
        if path == layout_path {
            zellij_utils::sessions::record_session_layout_creation_time(&path);
        }
        match persist_atomic_write(file, &path) {
            Ok(()) => {
                published.insert(path.clone(), hashes[&path]);
            },
            Err(error) => {
                all_persisted = false;
                if path != layout_path {
                    dependencies_persisted = false;
                }
                log::error!(
                    "Failed to publish session state file {}: {:?}",
                    path.display(),
                    error
                );
            },
        }
    }
    all_persisted
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        let directory = fs::File::open(parent)?;
        match directory.sync_all() {
            Ok(()) => {},
            Err(error) if directory_sync_unsupported(&error) => {},
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn directory_sync_unsupported(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(code) if code == libc::EINVAL || code == libc::ENOTSUP
    )
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn scan_session_list(
    current_session_name: &str,
    available_layouts: &[LayoutInfo],
    current_session_plugin_list: &BTreeMap<PluginId, RunPlugin>,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
) -> (BTreeMap<String, SessionInfo>, BTreeMap<String, Duration>) {
    scan_session_list_with_current(
        current_session_name,
        available_layouts,
        current_session_plugin_list,
        None,
        sock_dir,
        session_info_cache_dir,
    )
}

pub fn scan_session_list_with_current(
    current_session_name: &str,
    available_layouts: &[LayoutInfo],
    current_session_plugin_list: &BTreeMap<PluginId, RunPlugin>,
    current_session_info: Option<&SessionInfo>,
    sock_dir: &Path,
    session_info_cache_dir: &Path,
) -> (BTreeMap<String, SessionInfo>, BTreeMap<String, Duration>) {
    let mut session_infos_on_machine = zellij_utils::sessions::read_live_session_states(
        current_session_name,
        sock_dir,
        session_info_cache_dir,
    );
    if let Some(info) = session_infos_on_machine.get_mut(current_session_name) {
        info.populate_plugin_list(current_session_plugin_list.clone());
        info.available_layouts = available_layouts.to_vec();
        info.is_current_session = true;
    } else if !current_session_name.is_empty() {
        // The on-disk session-info scan can lag briefly after startup. Prefer
        // the in-memory current session over omitting it from get_session_list.
        let mut info = current_session_info.cloned().unwrap_or_else(|| {
            SessionInfo::new(current_session_name.to_owned())
        });
        info.name = current_session_name.to_owned();
        info.is_current_session = true;
        info.populate_plugin_list(current_session_plugin_list.clone());
        info.available_layouts = available_layouts.to_vec();
        session_infos_on_machine.insert(current_session_name.to_owned(), info);
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

pub fn scan_session_list_default_dirs_with_current(
    current_session_name: &str,
    available_layouts: &[LayoutInfo],
    current_session_plugin_list: &BTreeMap<PluginId, RunPlugin>,
    current_session_info: &SessionInfo,
) -> (BTreeMap<String, SessionInfo>, BTreeMap<String, Duration>) {
    scan_session_list_with_current(
        current_session_name,
        available_layouts,
        current_session_plugin_list,
        Some(current_session_info),
        &*ZELLIJ_SOCK_DIR,
        &*ZELLIJ_SESSION_INFO_CACHE_DIR,
    )
}

fn find_resurrectable_sessions(
    session_infos_on_machine: &BTreeMap<String, SessionInfo>,
    session_info_cache_dir: &Path,
) -> BTreeMap<String, Duration> {
    // Cache folders are keyed by session id; map each to its display name so we
    // can compare against the (name-keyed) live set and report a readable name.
    let names_by_id = zellij_utils::session_index::session_names_by_id();
    match fs::read_dir(session_info_cache_dir) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let id = folder_name.file_name()?.to_str()?.to_owned();
                    if zellij_utils::consts::is_app_socket_name(&id) {
                        return None;
                    }
                    let session_name = names_by_id.get(&id).cloned().unwrap_or_else(|| id.clone());
                    if session_infos_on_machine.contains_key(&session_name) {
                        // this is not a dead session...
                        return None;
                    }
                    let layout_file_name = folder_name.join("session-layout.kdl");
                    let ctime = match zellij_utils::sessions::session_layout_creation_time(
                        &layout_file_name,
                    ) {
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
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::sync::Barrier;
    use tempfile::tempdir;
    use zellij_utils::data::SessionInfo;

    fn admit_test_session(session_id: &str) -> u128 {
        zellij_utils::sessions::clear_session_deletion_fence(session_id);
        zellij_utils::sessions::read_session_incarnation(session_id)
            .unwrap_or_else(|error| panic!("admit {session_id}: {error}"))
    }

    fn remove_publication_sidecars(session_id: &str) {
        let _ = std::fs::remove_file(zellij_utils::consts::session_deletion_tombstone_path(
            session_id,
        ));
        let _ = std::fs::remove_file(zellij_utils::consts::session_incarnation_path(session_id));
        let _ = std::fs::remove_file(zellij_utils::consts::session_publication_lock_path(
            session_id,
        ));
        let _ = std::fs::remove_dir_all(session_info_folder_for_session(session_id));
    }

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

    fn serialized_layout_with_contents(contents: &[&str]) -> (String, BTreeMap<String, String>) {
        use zellij_utils::pane_size::{Dimension, PaneGeom};
        use zellij_utils::session_serialization::{
            serialize_session_layout, GlobalLayoutManifest, PaneLayoutManifest, TabLayoutManifest,
        };
        let tiled_panes = contents
            .iter()
            .enumerate()
            .map(|(index, contents)| PaneLayoutManifest {
                pane_contents: Some((*contents).to_owned()),
                geom: PaneGeom {
                    x: 0,
                    y: index * 10,
                    rows: Dimension::fixed(10),
                    cols: Dimension::fixed(10),
                    stacked: None,
                    is_pinned: false,
                    logical_position: None,
                },
                ..Default::default()
            })
            .collect();
        let tab_layout_manifest = TabLayoutManifest {
            tiled_panes,
            ..Default::default()
        };
        serialize_session_layout(GlobalLayoutManifest {
            tabs: vec![("Tab #1".to_owned(), tab_layout_manifest)],
            ..Default::default()
        })
        .unwrap()
    }

    fn stage_generation(
        layout_path: &Path,
        layout: &str,
        contents: &BTreeMap<String, String>,
        blocked_content: Option<&str>,
    ) -> (
        Vec<(PathBuf, tempfile::NamedTempFile)>,
        HashMap<PathBuf, u64>,
    ) {
        let folder = layout_path.parent().unwrap();
        let mut hashes = HashMap::new();
        let mut staged = Vec::new();
        for (file_name, contents) in contents {
            let path = folder.join(file_name);
            let file = stage_atomic_write(&path, contents.as_bytes()).unwrap();
            if blocked_content == Some(file_name.as_str()) {
                // Remove the staged temporary file: the rename fails without
                // touching whatever the currently committed generation has at
                // this path.
                std::fs::remove_file(file.path()).unwrap();
            }
            hashes.insert(path.clone(), content_hash(contents.as_bytes()));
            staged.push((path, file));
        }
        let layout_file = stage_atomic_write(layout_path, layout.as_bytes()).unwrap();
        hashes.insert(layout_path.to_path_buf(), content_hash(layout.as_bytes()));
        staged.push((layout_path.to_path_buf(), layout_file));
        (staged, hashes)
    }

    fn publish_generation(
        layout_path: &Path,
        layout: &str,
        contents: &BTreeMap<String, String>,
    ) -> bool {
        let (staged, hashes) = stage_generation(layout_path, layout, contents, None);
        publish_staged_session_state(layout_path, staged, &hashes, &mut HashMap::new(), true)
    }

    fn pane_contents_from_layout(layout: &zellij_utils::input::layout::Layout) -> Vec<String> {
        use zellij_utils::input::layout::{FloatingPaneLayout, TiledPaneLayout};
        fn collect_tiled(pane: &TiledPaneLayout, contents: &mut Vec<String>) {
            if let Some(pane_contents) = pane.pane_initial_contents.as_ref() {
                contents.push(pane_contents.clone());
            }
            for child in &pane.children {
                collect_tiled(child, contents);
            }
        }
        fn collect_floating(panes: &[FloatingPaneLayout], contents: &mut Vec<String>) {
            for pane in panes {
                if let Some(pane_contents) = pane.pane_initial_contents.as_ref() {
                    contents.push(pane_contents.clone());
                }
            }
        }
        let mut contents = Vec::new();
        for (_, tiled_panes, floating_panes) in &layout.tabs {
            collect_tiled(tiled_panes, &mut contents);
            collect_floating(floating_panes, &mut contents);
        }
        contents.sort();
        contents
    }

    fn loaded_pane_contents(layout_path: &Path) -> Vec<String> {
        let layout = zellij_utils::input::layout::Layout::from_path_without_config(
            &layout_path.to_path_buf(),
        )
        .unwrap();
        pane_contents_from_layout(&layout)
    }

    fn content_paths(folder: &Path, contents: &BTreeMap<String, String>) -> Vec<PathBuf> {
        contents.keys().map(|name| folder.join(name)).collect()
    }

    fn pane_content_files_on_disk(folder: &Path) -> Vec<String> {
        let mut names = std::fs::read_dir(folder)
            .unwrap()
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(|name| name.to_owned()))
            .filter(|name| {
                name.starts_with(zellij_utils::session_serialization::PANE_CONTENTS_FILE_PREFIX)
            })
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    #[test]
    fn a_failed_next_generation_content_publication_keeps_the_committed_generation_loadable() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let (layout_a, contents_a) = serialized_layout_with_contents(&["alpha one", "alpha two"]);
        let (layout_b, contents_b) = serialized_layout_with_contents(&["beta one", "beta two"]);
        let expected_a = vec!["alpha one".to_owned(), "alpha two".to_owned()];
        let expected_b = vec!["beta one".to_owned(), "beta two".to_owned()];

        assert!(publish_generation(&layout_path, &layout_a, &contents_a));
        assert_eq!(loaded_pane_contents(&layout_path), expected_a);

        for failing_index in 0..contents_b.len() {
            let blocked_name = contents_b.keys().nth(failing_index).unwrap().clone();
            let (staged, hashes) =
                stage_generation(&layout_path, &layout_b, &contents_b, Some(&blocked_name));
            let all_persisted = publish_staged_session_state(
                &layout_path,
                staged,
                &hashes,
                &mut HashMap::new(),
                true,
            );

            assert!(!all_persisted, "publication {} must fail", failing_index);
            assert_eq!(
                loaded_pane_contents(&layout_path),
                expected_a,
                "after failing publication {}",
                failing_index
            );
        }

        assert!(publish_generation(&layout_path, &layout_b, &contents_b));
        assert_eq!(loaded_pane_contents(&layout_path), expected_b);
    }

    #[test]
    fn stale_pane_content_generations_are_retained_for_one_save_then_pruned() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let (layout_a, contents_a) = serialized_layout_with_contents(&["alpha one", "alpha two"]);
        let (layout_b, contents_b) = serialized_layout_with_contents(&["beta one", "beta two"]);
        let (layout_c, contents_c) = serialized_layout_with_contents(&["gamma one", "gamma two"]);

        assert!(publish_generation(&layout_path, &layout_a, &contents_a));
        assert!(publish_generation(&layout_path, &layout_b, &contents_b));
        prune_stale_pane_content_files(dir.path(), &layout_path, Some(&layout_a), &layout_b);
        for path in content_paths(dir.path(), &contents_a) {
            assert!(path.exists(), "the previous generation must be retained");
        }
        for path in content_paths(dir.path(), &contents_b) {
            assert!(path.exists());
        }

        assert!(publish_generation(&layout_path, &layout_c, &contents_c));
        prune_stale_pane_content_files(dir.path(), &layout_path, Some(&layout_b), &layout_c);
        for path in content_paths(dir.path(), &contents_a) {
            assert!(!path.exists(), "two generations back must be pruned");
        }
        for path in content_paths(dir.path(), &contents_b) {
            assert!(path.exists(), "the previous generation must be retained");
        }
        for path in content_paths(dir.path(), &contents_c) {
            assert!(path.exists());
        }
    }

    #[test]
    fn a_held_reader_restarts_on_a_new_generation_after_two_saves() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let (layout_a, contents_a) = serialized_layout_with_contents(&["alpha one", "alpha two"]);
        let (layout_b, contents_b) = serialized_layout_with_contents(&["beta one", "beta two"]);
        let (layout_c, contents_c) = serialized_layout_with_contents(&["gamma one", "gamma two"]);
        let expected_c = vec!["gamma one".to_owned(), "gamma two".to_owned()];

        let session_id = format!("held-reader-test-{}", std::process::id());
        let incarnation = admit_test_session(&session_id);
        assert!(publish_generation(&layout_path, &layout_a, &contents_a));

        let (layout_read_tx, layout_read_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let reader_path = layout_path.clone();
        let reader = thread::spawn(move || {
            let mut paused = false;
            zellij_utils::sessions::resurrection_layout_from_path_with_reader_barrier(
                &reader_path,
                || {
                    if !paused {
                        paused = true;
                        layout_read_tx.send(()).unwrap();
                        resume_rx.recv().unwrap();
                    }
                },
            )
        });
        layout_read_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("reader should pause after reading the first layout");

        let (staged_b, hashes_b) = stage_generation(&layout_path, &layout_b, &contents_b, None);
        assert!(commit_staged_session_generation(
            &session_id,
            incarnation,
            dir.path(),
            &layout_path,
            staged_b,
            &hashes_b,
            &mut HashMap::new(),
            true,
            &layout_b,
            true,
        )
        .unwrap());
        let (staged_c, hashes_c) = stage_generation(&layout_path, &layout_c, &contents_c, None);
        assert!(commit_staged_session_generation(
            &session_id,
            incarnation,
            dir.path(),
            &layout_path,
            staged_c,
            &hashes_c,
            &mut HashMap::new(),
            true,
            &layout_c,
            true,
        )
        .unwrap());

        for path in content_paths(dir.path(), &contents_a) {
            assert!(
                !path.exists(),
                "the reader's first generation should have been reclaimed"
            );
        }

        resume_tx.send(()).unwrap();
        let layout = reader
            .join()
            .unwrap()
            .unwrap()
            .expect("the reader should restart on the newer generation");
        assert_eq!(pane_contents_from_layout(&layout), expected_c);

        let referenced_b =
            zellij_utils::session_serialization::session_layout_referenced_content_files(&layout_b);
        let referenced_c =
            zellij_utils::session_serialization::session_layout_referenced_content_files(&layout_c);
        let files_on_disk = pane_content_files_on_disk(dir.path());
        for file_name in files_on_disk {
            assert!(
                referenced_b.contains(&file_name) || referenced_c.contains(&file_name),
                "reader release left an orphan pane-content file: {file_name}"
            );
        }
        remove_publication_sidecars(&session_id);
    }

    #[test]
    fn concurrent_saves_retain_the_immediately_preceding_generation() {
        let dir = tempdir().unwrap();
        let layout_path = dir.path().join("session-layout.kdl");
        let (layout_a, contents_a) = serialized_layout_with_contents(&["alpha one", "alpha two"]);
        let (layout_b, contents_b) = serialized_layout_with_contents(&["beta one", "beta two"]);
        let (layout_c, contents_c) = serialized_layout_with_contents(&["gamma one", "gamma two"]);

        let session_id = format!("concurrent-saves-test-{}", std::process::id());
        let incarnation = admit_test_session(&session_id);
        assert!(publish_generation(&layout_path, &layout_a, &contents_a));

        let barrier = Arc::new(Barrier::new(3));
        let writers = [
            (layout_b.clone(), contents_b.clone()),
            (layout_c.clone(), contents_c.clone()),
        ]
        .into_iter()
        .map(|(layout, contents)| {
            let layout_path = layout_path.clone();
            let folder = dir.path().to_path_buf();
            let barrier = barrier.clone();
            let session_id = session_id.clone();
            thread::spawn(move || {
                let (staged, hashes) = stage_generation(&layout_path, &layout, &contents, None);
                barrier.wait();
                commit_staged_session_generation(
                    &session_id,
                    incarnation,
                    &folder,
                    &layout_path,
                    staged,
                    &hashes,
                    &mut HashMap::new(),
                    true,
                    &layout,
                    true,
                )
                .unwrap()
            })
        })
        .collect::<Vec<_>>();
        barrier.wait();
        for writer in writers {
            assert!(writer.join().unwrap());
        }

        let on_disk = std::fs::read_to_string(&layout_path).unwrap();
        assert!(on_disk == layout_b || on_disk == layout_c);
        for path in content_paths(dir.path(), &contents_a) {
            assert!(
                !path.exists(),
                "the generation before both concurrent saves must be pruned"
            );
        }
        for path in content_paths(dir.path(), &contents_b)
            .into_iter()
            .chain(content_paths(dir.path(), &contents_c))
        {
            assert!(
                path.exists(),
                "both concurrent generations must remain available"
            );
        }
        remove_publication_sidecars(&session_id);
    }

    #[test]
    fn a_newer_frame_color_override_invalidates_the_older_expiry() {
        let pane_id = PaneId::Terminal(1);
        let mut generations = FrameColorGenerations::default();
        let older = generations.begin(&[pane_id]);
        let newer = generations.begin(&[pane_id]);

        assert!(generations.take_current(&older).is_empty());
        assert_eq!(generations.take_current(&newer), vec![pane_id]);
    }

    #[test]
    fn a_stale_expiry_cannot_clear_a_newer_override_started_while_it_sends() {
        let pane_id = PaneId::Terminal(1);
        let generations = Arc::new(Mutex::new(FrameColorGenerations::default()));
        let old_tokens = lock_frame_color_generations(&generations).begin(&[pane_id]);
        let (queue_sender, queue) = std::sync::mpsc::channel::<&'static str>();
        let (in_send_sender, in_send) = std::sync::mpsc::channel::<()>();

        // The old expiry passes the generation check, then is slow to hand
        // its clear to the screen queue.
        let expiry = std::thread::spawn({
            let generations = generations.clone();
            let queue_sender = queue_sender.clone();
            move || {
                expire_frame_color_overrides(&generations, &old_tokens, |_| {
                    in_send_sender.send(()).unwrap();
                    std::thread::sleep(Duration::from_millis(300));
                    queue_sender.send("clear").unwrap();
                });
            }
        });
        in_send.recv().unwrap();
        // A newer override starts and sends its highlight meanwhile.
        let new_tokens = lock_frame_color_generations(&generations).begin(&[pane_id]);
        queue_sender.send("highlight").unwrap();
        expiry.join().unwrap();

        // The stale clear must be queued before the newer highlight, never
        // after it.
        assert_eq!(queue.try_recv().unwrap(), "clear");
        assert_eq!(queue.try_recv().unwrap(), "highlight");
        // and the newer override still expires normally.
        let mut cleared = vec![];
        expire_frame_color_overrides(&generations, &new_tokens, |pane_ids| cleared = pane_ids);
        assert_eq!(cleared, vec![pane_id]);
    }

    #[test]
    fn an_explicit_clear_invalidates_a_pending_expiry() {
        let pane_id = PaneId::Terminal(1);
        let mut generations = FrameColorGenerations::default();
        let pending = generations.begin(&[pane_id]);

        generations.invalidate(&[pane_id]);

        assert!(generations.take_current(&pending).is_empty());
    }

    #[test]
    fn atomic_file_write_replaces_contents_without_leaving_a_temporary_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session-state");
        std::fs::write(&path, b"old").unwrap();

        write_file_atomically(&path, b"new").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        let entries = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(entries, 1);
    }

    #[test]
    fn unsupported_parent_directory_sync_is_not_a_save_failure() {
        use std::io::Error;

        assert!(directory_sync_unsupported(&Error::from_raw_os_error(
            libc::EINVAL
        )));
        assert!(directory_sync_unsupported(&Error::from_raw_os_error(
            libc::ENOTSUP
        )));
        assert!(!directory_sync_unsupported(&Error::from_raw_os_error(
            libc::EIO
        )));
    }

    #[test]
    fn a_failed_content_write_leaves_the_layout_unpublished() {
        let dir = tempdir().unwrap();
        let content_path = dir.path().join("pane-1.txt");
        let layout_path = dir.path().join("session-layout.kdl");
        // A directory at the content path makes the staged file impossible to
        // publish while leaving staging itself successful.
        std::fs::create_dir(&content_path).unwrap();
        let content_file = stage_atomic_write(&content_path, b"content").unwrap();
        let layout_file = stage_atomic_write(&layout_path, b"layout").unwrap();
        let hashes = HashMap::from([
            (content_path.clone(), content_hash(b"content")),
            (layout_path.clone(), content_hash(b"layout")),
        ]);
        let mut published = HashMap::new();

        let all_persisted = publish_staged_session_state(
            &layout_path,
            vec![
                (content_path, content_file),
                (layout_path.clone(), layout_file),
            ],
            &hashes,
            &mut published,
            true,
        );

        assert!(!all_persisted);
        assert!(!layout_path.exists());
        assert!(!published.contains_key(&layout_path));
    }

    #[test]
    fn concurrent_atomic_writes_never_expose_mixed_contents() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("session-state");
        let barrier = Arc::new(std::sync::Barrier::new(9));
        let handles = (0..8)
            .map(|index| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    let contents = vec![b'a' + index; 4096];
                    barrier.wait();
                    write_file_atomically(&path, &contents).unwrap();
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        for handle in handles {
            handle.join().unwrap();
        }

        let contents = std::fs::read(&path).unwrap();
        assert_eq!(contents.len(), 4096);
        assert!(contents.windows(2).all(|window| window[0] == window[1]));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn dirty_check_restages_only_changed_or_missing_artifacts() {
        // Unique per run: this writes to the real session cache folder.
        let session_id = &format!("dirty-check-test-session-{}", std::process::id());
        let _ = std::fs::remove_dir_all(session_info_folder_for_session(session_id));
        let info = SessionInfo::new(session_id.to_string());
        let layout = (
            "layout { }".to_string(),
            BTreeMap::from([("pane-1.txt".to_string(), "contents".to_string())]),
        );

        let (writes, published) =
            collect_dirty_session_state_writes(session_id, &info, &layout, &HashMap::new());
        assert_eq!(writes.len(), 3);
        assert_eq!(
            writes
                .iter()
                .map(|(path, _)| path.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["session-metadata.kdl", "pane-1.txt", "session-layout.kdl"],
            "the layout must be published after the content it references"
        );
        for (path, contents) in &writes {
            write_file_atomically(path, contents).unwrap();
        }

        let (writes, _) =
            collect_dirty_session_state_writes(session_id, &info, &layout, &published);
        assert!(writes.is_empty());

        let changed_layout = (
            "layout { }".to_string(),
            BTreeMap::from([("pane-1.txt".to_string(), "new contents".to_string())]),
        );
        let (writes, _) =
            collect_dirty_session_state_writes(session_id, &info, &changed_layout, &published);
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].0.file_name().unwrap(), "pane-1.txt");

        std::fs::remove_file(session_info_cache_file_name(session_id)).unwrap();
        let (writes, _) =
            collect_dirty_session_state_writes(session_id, &info, &layout, &published);
        assert_eq!(
            writes
                .iter()
                .map(|(path, _)| path.clone())
                .collect::<Vec<_>>(),
            vec![session_info_cache_file_name(session_id)],
        );

        std::fs::remove_dir_all(session_info_folder_for_session(session_id)).unwrap();
        let _ = std::fs::remove_file(session_layout_cache_file_name(session_id));
        let _ = std::fs::remove_file(session_info_cache_file_name(session_id));
    }

    #[test]
    fn an_empty_layout_is_neither_written_nor_part_of_the_published_state() {
        // The save time is only reported when the layout is among the
        // published artifacts (`layout_is_part_of_state`); with an empty
        // layout it is not.
        let session_id = &format!("empty-layout-test-session-{}", std::process::id());
        let info = SessionInfo::new(session_id.to_string());
        let layout_path = session_layout_cache_file_name(session_id);

        let (writes, hashes) = collect_dirty_session_state_writes(
            session_id,
            &info,
            &(String::new(), BTreeMap::new()),
            &HashMap::new(),
        );
        assert_eq!(
            writes.iter().map(|(path, _)| path).collect::<Vec<_>>(),
            vec![&session_info_cache_file_name(session_id)],
        );
        assert!(!hashes.contains_key(&layout_path));
        assert!(!layout_is_part_of_state(session_id, &hashes));

        let (writes, hashes) = collect_dirty_session_state_writes(
            session_id,
            &info,
            &("layout { }".to_string(), BTreeMap::new()),
            &HashMap::new(),
        );
        assert!(writes.iter().any(|(path, _)| path == &layout_path));
        assert!(hashes.contains_key(&layout_path));
        assert!(layout_is_part_of_state(session_id, &hashes));
    }

    #[test]
    fn deletion_tombstone_blocks_session_state_publication() {
        let session_id = format!(
            "fenced-write-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let folder = session_info_folder_for_session(&session_id);
        let metadata_path = session_info_cache_file_name(&session_id);
        let layout_path = session_layout_cache_file_name(&session_id);
        remove_publication_sidecars(&session_id);
        let incarnation = admit_test_session(&session_id);
        std::fs::create_dir_all(&folder).unwrap();
        let info = SessionInfo::new(session_id.clone());
        let (layout, contents) = serialized_layout_with_contents(&["pane-a"]);

        write_session_state_to_disk(
            session_id.clone(),
            incarnation,
            info.clone(),
            (layout.clone(), contents.clone()),
        )
        .unwrap();
        assert!(
            metadata_path.exists() && layout_path.exists(),
            "unfenced write must publish metadata and layout"
        );

        zellij_utils::sessions::remove_fenced_session_cache(&session_id).unwrap();
        assert!(!folder.exists());
        let fenced = write_session_state_to_disk(
            session_id.clone(),
            incarnation,
            info,
            (layout.clone(), contents.clone()),
        )
        .unwrap_err();
        assert!(
            zellij_utils::sessions::is_session_publication_fenced(&fenced),
            "fenced save must be a distinct error, got {fenced}"
        );
        assert_eq!(fenced.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(
            !folder.exists() && !layout_path.exists() && !metadata_path.exists(),
            "fenced writer must not recreate deleted session state"
        );

        // A new owner drops the tombstone and mints a new incarnation. The
        // writer that still holds the old value cannot publish.
        zellij_utils::sessions::clear_session_deletion_fence(&session_id);
        assert!(!zellij_utils::sessions::session_deletion_is_fenced(
            &session_id
        ));
        let renewed = write_session_state_to_disk(
            session_id.clone(),
            incarnation,
            SessionInfo::new(session_id.clone()),
            (layout.clone(), contents.clone()),
        )
        .unwrap_err();
        assert!(zellij_utils::sessions::is_session_publication_fenced(
            &renewed
        ));
        assert!(!folder.exists());
        let admitted = zellij_utils::sessions::read_session_incarnation(&session_id).unwrap();
        assert_ne!(admitted, incarnation);
        write_session_state_to_disk(
            session_id.clone(),
            admitted,
            SessionInfo::new(session_id.clone()),
            (layout, contents),
        )
        .unwrap();
        assert!(metadata_path.exists() && layout_path.exists());
        remove_publication_sidecars(&session_id);
    }

    #[test]
    fn deletion_during_publication_cannot_leave_tombstone_and_old_layout() {
        let session_id = format!(
            "fenced-race-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        remove_publication_sidecars(&session_id);
        let incarnation = admit_test_session(&session_id);
        let folder = session_info_folder_for_session(&session_id);
        let metadata_path = session_info_cache_file_name(&session_id);
        let layout_path = session_layout_cache_file_name(&session_id);
        let info = SessionInfo::new(session_id.clone());
        let (layout, contents) = serialized_layout_with_contents(&["old-writer"]);
        let (checked_tx, checked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();

        let writer_id = session_id.clone();
        let writer_info = info.clone();
        let writer_layout = layout.clone();
        let writer_contents = contents.clone();
        let writer = thread::spawn(move || {
            write_session_state_to_disk_with_hook(
                writer_id,
                incarnation,
                writer_info,
                (writer_layout, writer_contents),
                &|| {
                    checked_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                },
            )
        });
        checked_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("writer should pass the publication check and hold the lock");
        let deleter_id = session_id.clone();
        let deleter =
            thread::spawn(move || zellij_utils::sessions::remove_fenced_session_cache(&deleter_id));
        thread::sleep(Duration::from_millis(300));
        assert!(
            !deleter.is_finished(),
            "deleter must block on the publication lock held across check and persist"
        );
        release_tx.send(()).unwrap();
        let published = writer.join().unwrap();
        let removed = deleter.join().unwrap();
        assert!(
            published.is_ok(),
            "writer committed before the deleter: {published:?}"
        );
        assert!(
            removed.is_ok(),
            "deleter should remove the folder after the lock: {removed:?}"
        );
        let tombstone = zellij_utils::consts::session_deletion_tombstone_path(&session_id);
        let tombstone_exists = tombstone.exists();
        let layout_exists = layout_path.exists();
        assert!(
            !(tombstone_exists && layout_exists),
            "tombstone and a layout from the old incarnation must not both remain"
        );
        assert!(tombstone_exists && !layout_exists && !metadata_path.exists());
        assert!(!folder.exists());

        let again =
            write_session_state_to_disk(session_id.clone(), incarnation, info, (layout, contents))
                .unwrap_err();
        assert!(zellij_utils::sessions::is_session_publication_fenced(
            &again
        ));
        assert!(
            !folder.exists() && !layout_path.exists(),
            "a second publish by the old incarnation must not recreate the directory"
        );
        remove_publication_sidecars(&session_id);
    }

    #[test]
    fn deletion_fence_allows_two_sessions_to_publish_concurrently() {
        let mut sessions = Vec::new();
        for label in ["fence-concurrent-a", "fence-concurrent-b"] {
            let session_id = format!(
                "{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            remove_publication_sidecars(&session_id);
            let incarnation = admit_test_session(&session_id);
            sessions.push((session_id, incarnation));
            thread::sleep(Duration::from_millis(2));
        }
        let arrived = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for (session_id, incarnation) in sessions.clone() {
            let arrived = Arc::clone(&arrived);
            handles.push(thread::spawn(move || {
                let info = SessionInfo::new(session_id.clone());
                write_session_state_to_disk_with_hook(
                    session_id.clone(),
                    incarnation,
                    info,
                    ("layout { }".to_string(), BTreeMap::new()),
                    &|| {
                        arrived.fetch_add(1, Ordering::SeqCst);
                        let started = std::time::Instant::now();
                        while arrived.load(Ordering::SeqCst) < 2 {
                            if started.elapsed() > Duration::from_secs(3) {
                                panic!("two sessions serialized on one publication lock");
                            }
                            thread::sleep(Duration::from_millis(10));
                        }
                    },
                )
                .unwrap();
                assert!(session_layout_cache_file_name(&session_id).exists());
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        for (session_id, _) in sessions {
            remove_publication_sidecars(&session_id);
        }
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
        // With no disk metadata yet, the current session is still injected from memory.
        assert_eq!(live.len(), 1);
        assert!(live["me"].is_current_session);
        assert!(resurrectable.is_empty());
    }

    #[test]
    fn scan_session_list_injects_in_memory_current_session_when_scan_lags() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        let mut current = SessionInfo::new("fresh".to_owned());
        current.connected_clients = 3;
        let (live, _) = scan_session_list_with_current(
            "fresh",
            &[],
            &BTreeMap::new(),
            Some(&current),
            sock_dir.path(),
            info_dir.path(),
        );
        assert_eq!(live.len(), 1);
        let injected = live.get("fresh").expect("current session present");
        assert!(injected.is_current_session);
        assert_eq!(injected.connected_clients, 3);
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
        // The current session is injected even when its metadata has not landed.
        assert_eq!(live.len(), 2);
        assert!(live["me"].is_current_session);
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
        assert_eq!(live.keys().collect::<Vec<_>>(), vec!["me"]);
        assert!(live["me"].is_current_session);
        assert_eq!(resurrectable.len(), 1);
        assert!(resurrectable.contains_key("dead-beta"));
    }

    #[test]
    fn a_dead_sessions_age_survives_replacing_its_layout_file() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        write_layout(info_dir.path(), "long-running");
        let layout = info_dir.path().join("long-running/session-layout.kdl");
        // the session started two hours ago
        let started = std::time::SystemTime::now() - Duration::from_secs(2 * 3600);
        let started_secs = started
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        std::fs::write(
            layout.with_file_name("session-layout-created"),
            started_secs.to_string(),
        )
        .unwrap();

        // every later save replaces the file, which gets a fresh timestamp
        write_file_atomically(&layout, b"layout { pane }").unwrap();
        zellij_utils::sessions::record_session_layout_creation_time(&layout);

        let (_, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert!(resurrectable["long-running"] >= Duration::from_secs(2 * 3600 - 10));
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
        assert_eq!(live.len(), 4);
        assert!(live["me"].is_current_session);
        assert_eq!(resurrectable.len(), 2);
        for name in ["live-a", "live-b", "live-c"] {
            assert!(!resurrectable.contains_key(name));
        }
    }

    #[test]
    fn the_hidden_app_sessions_are_neither_live_nor_resurrectable() {
        let sock_dir = tempdir().unwrap();
        let info_dir = tempdir().unwrap();
        let panel = zellij_utils::consts::APP_PANEL_SESSION;
        let _panel = make_socket(sock_dir.path(), panel);
        write_metadata(info_dir.path(), panel, &SessionInfo::new(panel.to_string()));
        write_layout(info_dir.path(), panel);
        write_layout(info_dir.path(), ".old-app-thing");
        let _live = make_socket(sock_dir.path(), "visible");
        write_metadata(
            info_dir.path(),
            "visible",
            &SessionInfo::new("visible".to_string()),
        );
        write_layout(info_dir.path(), "exited");

        let (live, resurrectable) = scan_session_list(
            "me",
            &[],
            &BTreeMap::new(),
            sock_dir.path(),
            info_dir.path(),
        );
        assert_eq!(live.keys().collect::<Vec<_>>(), vec!["me", "visible"]);
        assert!(live["me"].is_current_session);
        assert_eq!(resurrectable.keys().collect::<Vec<_>>(), vec!["exited"]);
        let read = zellij_utils::sessions::read_live_session_states(
            "me",
            sock_dir.path(),
            info_dir.path(),
        );
        assert_eq!(read.keys().collect::<Vec<_>>(), vec!["visible"]);
    }
}

/// Bytes of stderr retained from a background command for error
/// logging. The pipe is drained fully but only this much is kept.
const BACKGROUND_COMMAND_STDERR_RETAINED: usize = 8 * 1024;

async fn run_in_background(
    command: PathBuf,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: PaneEnv,
) {
    let mut process = tokio::process::Command::new(&command);
    process
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    for (name, value) in env {
        match value {
            Some(value) => process.env(name, value),
            None => process.env_remove(name),
        };
    }
    if let Some(cwd) = cwd {
        if cwd.is_dir() {
            process.current_dir(cwd);
        } else {
            log::error!(
                "Cannot run '{}' in '{}': not a folder",
                command.display(),
                cwd.display()
            );
        }
    }
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(e) => {
            log::error!(
                "Failed to start background command '{}': {}",
                command.display(),
                e
            );
            return;
        },
    };
    // stderr is piped only so a failing command's tail can be logged.
    // Capture is bounded: the pipe is drained fully (a chatty child
    // must not deadlock on a full pipe) but only the last
    // BACKGROUND_COMMAND_STDERR_RETAINED bytes are retained.
    let mut stderr_pipe = child.stderr.take().expect("stderr is piped");
    let drain = tokio::spawn(async move {
        use tokio::io::AsyncReadExt;
        let mut retained: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match stderr_pipe.read(&mut chunk).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    retained.extend_from_slice(&chunk[..n]);
                    if retained.len() > BACKGROUND_COMMAND_STDERR_RETAINED {
                        let overflow = retained.len() - BACKGROUND_COMMAND_STDERR_RETAINED;
                        retained.drain(..overflow);
                    }
                },
            }
        }
        retained
    });
    let status = child.wait().await;
    let stderr = drain.await.unwrap_or_default();
    match status {
        Ok(status) if !status.success() => {
            log::error!(
                "Background command '{}' exited with {}: {}",
                command.display(),
                status,
                String::from_utf8_lossy(&stderr).trim_end()
            );
        },
        Ok(_) => {},
        Err(e) => {
            log::error!(
                "Failed to wait for background command '{}': {}",
                command.display(),
                e
            );
        },
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
    fn a_redraw_split_by_a_short_stall_under_load_renders_once() {
        let stall = Duration::from_millis(3);
        assert!(stall < RENDER_QUIET_PERIOD);
        let start = Instant::now();
        let mut schedule = RenderSchedule::default();
        assert!(schedule.request(start));
        assert_ne!(schedule.step(start + stall), RenderStep::Render);
        assert!(!schedule.request(start + stall));
        assert_eq!(
            schedule.step(start + stall + RENDER_QUIET_PERIOD),
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
