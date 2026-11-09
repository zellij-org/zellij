use super::pinned_executor::{on_plugin_executor, JobClass, PluginWork};
use super::{PinnedExecutor, PluginId, PluginInstruction};
use crate::global_async_runtime::get_tokio_runtime;
use crate::plugins::pipes::{
    apply_pipe_message_to_plugin, pipes_to_block_or_unblock, PendingPipes, PipeRelease,
    PipeStateChange,
};
use crate::plugins::plugin_loader::PluginLoader;
use crate::plugins::plugin_map::{
    next_plugin_generation, AtomicEvent, PluginEnv, PluginMap, PluginMetadata, RemovedPluginAssets,
    RunningPlugin, SharedSlot, Subscriptions,
};
use crate::plugins::prompt_requests::{PromptAction, PromptCaller, PromptRequests};
use crate::plugins::settings_entries::{PluginIdentity, SettingsEntries};
use crate::plugins::shared::{
    add_slot_job, apply_events_job, apply_pipes_job, client_job, host_settings_job,
    remove_slot_job, resize_job, revoke_client_loads, revoke_plugin_loads, start_instance_job,
    take_client_data_dirs, try_lock_running, visibility_job, wasm_module_exports_function,
    SharedKey, SHARED_MARKER_EXPORT, SHARED_PIPE_CLIENT,
};
use zellij_utils::prompt::PromptResult;

use crate::plugins::plugin_worker::{try_send_to_worker, MessageToWorker};
use crate::plugins::shared::SharedEventBatchEntry;
use crate::plugins::watch_filesystem::watch_filesystem;
use crate::plugins::zellij_exports::{wasi_read_string, wasi_write_object};
use crate::SharedKeybinds;
use highway::{HighwayHash, PortableHash};
use log::info;
use notify_debouncer_full::{notify::RecommendedWatcher, Debouncer, RecommendedCache};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc::Sender;
use url::Url;
use wasmi::{Engine, Module};
use zellij_utils::consts::{ZELLIJ_CACHE_DIR, ZELLIJ_SESSION_CACHE_DIR, ZELLIJ_TMP_DIR};
use zellij_utils::data::{
    FloatingPaneCoordinates, HostTerminalThemeMode, InputMode, KeybindPresetInfo,
    KeybindPresetWithError, KeybindsVec, LayoutInfo, LayoutWithError, PaneContents,
    PaneRenderReport, PermissionStatus, PermissionType, PipeMessage, PipeSource,
};
use zellij_utils::downloader::Downloader;
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::plugin_api::event::{event_to_protobuf_with_keybinds, ProtobufEvent};
use zellij_utils::plugin_api::released_contract;

use prost::Message;

use crate::panes::PaneId;
use crate::{
    background_jobs::BackgroundJob,
    route::NotificationEnd,
    screen::{ClosePaneSource, ScreenInstruction},
    thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication,
    ClientId, ServerInstruction,
};
use zellij_utils::{
    data::{Event, EventContext, EventType, SlotKind},
    errors::prelude::*,
    input::{
        command::TerminalAction,
        layout::{PluginUserConfiguration, RunPlugin, RunPluginLocation, RunPluginOrAlias},
        plugins::{canonical_run_plugin, PluginAliases, PluginConfig, SHARED_INSTANCE_KEY},
    },
    pane_size::Size,
};

/// On Windows, colons in URL strings (e.g. `zellij:tab-bar`, `file:///...`)
/// are illegal in path components. Replace them with underscores.
#[cfg(windows)]
fn make_plugin_url_path_safe(url: String) -> String {
    url.replace(':', "_")
}

#[cfg(not(windows))]
fn make_plugin_url_path_safe(url: String) -> String {
    url
}

#[derive(Debug, Clone)]
pub enum EventOrPipeMessage {
    Event(Event, Option<ClientId>, Option<u64>),
    PipeMessage(PipeMessage, Option<ClientId>),
}

const MAX_PENDING_CACHE_ENTRIES_PER_PLUGIN: usize = 1024;
const MAX_PENDING_CACHE_BYTES_PER_PLUGIN: usize = 4 * 1024 * 1024;
const MAX_PENDING_CACHE_ENTRIES_TOTAL: usize = 4096;
const MAX_PENDING_CACHE_BYTES_TOTAL: usize = 16 * 1024 * 1024;
const PENDING_CACHE_ENTRY_OVERHEAD: usize = 64;
const MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN: usize = 256;
const MAX_CACHED_WORKER_MESSAGE_BYTES_PER_PLUGIN: usize = 1024 * 1024;

#[derive(Debug, Default)]
struct PendingPluginMessages {
    entries: Vec<EventOrPipeMessage>,
    bytes: usize,
}

#[derive(Debug, Default)]
struct PendingPluginCaches {
    plugins: HashMap<PluginId, PendingPluginMessages>,
    total_entries: usize,
    total_bytes: usize,
}

#[derive(Clone, Copy, Debug)]
enum PendingCacheRejection {
    EntryLimit,
    ByteLimit,
}

impl PendingPluginCaches {
    fn insert_empty(&mut self, plugin_id: PluginId) {
        self.plugins.entry(plugin_id).or_default();
    }

    fn contains(&self, plugin_id: &PluginId) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    fn take(&mut self, plugin_id: &PluginId) -> Option<PendingPluginMessages> {
        let messages = self.plugins.remove(plugin_id)?;
        self.total_entries = self.total_entries.saturating_sub(messages.entries.len());
        self.total_bytes = self.total_bytes.saturating_sub(messages.bytes);
        Some(messages)
    }

    fn plugin_ids(&self) -> Vec<PluginId> {
        self.plugins.keys().copied().collect()
    }

    fn try_push(
        &mut self,
        plugin_id: PluginId,
        entry: EventOrPipeMessage,
        front: bool,
    ) -> Result<(), PendingCacheRejection> {
        let bytes =
            event_or_pipe_message_bytes(&entry).saturating_add(PENDING_CACHE_ENTRY_OVERHEAD);
        let Some(cache) = self.plugins.get_mut(&plugin_id) else {
            return Ok(());
        };
        if cache.entries.len() >= MAX_PENDING_CACHE_ENTRIES_PER_PLUGIN
            || self.total_entries >= MAX_PENDING_CACHE_ENTRIES_TOTAL
        {
            return Err(PendingCacheRejection::EntryLimit);
        }
        if cache.bytes.saturating_add(bytes) > MAX_PENDING_CACHE_BYTES_PER_PLUGIN
            || self.total_bytes.saturating_add(bytes) > MAX_PENDING_CACHE_BYTES_TOTAL
        {
            return Err(PendingCacheRejection::ByteLimit);
        }
        if front {
            cache.entries.insert(0, entry);
        } else {
            cache.entries.push(entry);
        }
        cache.bytes = cache.bytes.saturating_add(bytes);
        self.total_entries = self.total_entries.saturating_add(1);
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        Ok(())
    }

    #[cfg(test)]
    fn insert_entries_for_test(&mut self, plugin_id: PluginId, entries: Vec<EventOrPipeMessage>) {
        let bytes = entries
            .iter()
            .map(event_or_pipe_message_bytes)
            .fold(0usize, usize::saturating_add);
        self.total_entries = self.total_entries.saturating_add(entries.len());
        self.total_bytes = self.total_bytes.saturating_add(bytes);
        self.plugins
            .insert(plugin_id, PendingPluginMessages { entries, bytes });
    }
}

const REPLACEABLE_RESIZE: u64 = 1;
const REPLACEABLE_HOST_DIR: u64 = 2;
const REPLACEABLE_SHARED_SETTINGS: u64 = 3;
const REPLACEABLE_PLUGIN_SETTINGS: u64 = 4;
const REPLACEABLE_VISIBILITY: u64 = 5;
const REPLACEABLE_KIND_SHIFT: u64 = 56;
const REPLACEABLE_GENERATION_SHIFT: u64 = 32;
const REPLACEABLE_GENERATION_MASK: u64 = 0x00FF_FFFF;
const REPLACEABLE_DESTINATION_MASK: u64 = u32::MAX as u64;

/// Replaceable work coalesces only when operation, destination and generation
/// all match. The destination is a client for per-client instances and a slot
/// for shared instances.
fn replaceable_key(kind: u64, destination: u32, generation: u64) -> u64 {
    ((kind & 0xFF) << REPLACEABLE_KIND_SHIFT)
        | ((generation & REPLACEABLE_GENERATION_MASK) << REPLACEABLE_GENERATION_SHIFT)
        | (destination as u64 & REPLACEABLE_DESTINATION_MASK)
}

fn replaceable_host_dir_key(client_id: ClientId, generation: u64) -> u64 {
    replaceable_key(REPLACEABLE_HOST_DIR, client_id, generation)
}

fn replaceable_resize_key(destination: u32, generation: u64) -> u64 {
    replaceable_key(REPLACEABLE_RESIZE, destination, generation)
}

fn replaceable_plugin_settings_key(client_id: ClientId, generation: u64) -> u64 {
    replaceable_key(REPLACEABLE_PLUGIN_SETTINGS, client_id, generation)
}

fn replaceable_shared_settings_key(generation: u64) -> u64 {
    replaceable_key(REPLACEABLE_SHARED_SETTINGS, 0, generation)
}

fn replaceable_visibility_key(generation: u64) -> u64 {
    replaceable_key(REPLACEABLE_VISIBILITY, 0, generation)
}

fn string_map_bytes(map: &BTreeMap<String, String>) -> usize {
    map.iter()
        .map(|(key, value)| key.len().saturating_add(value.len()).saturating_add(48))
        .fold(0usize, usize::saturating_add)
}

fn pane_contents_bytes(contents: &PaneContents) -> usize {
    contents
        .lines_above_viewport
        .iter()
        .chain(contents.lines_below_viewport.iter())
        .chain(contents.viewport.iter())
        .map(|line| line.len().saturating_add(32))
        .fold(std::mem::size_of::<PaneContents>(), usize::saturating_add)
}

fn pane_render_report_bytes(panes: &HashMap<zellij_utils::data::PaneId, PaneContents>) -> usize {
    panes.values().map(pane_contents_bytes).fold(
        std::mem::size_of::<PaneRenderReport>(),
        usize::saturating_add,
    )
}

fn keybinds_vec_bytes(keybinds: &KeybindsVec) -> usize {
    keybinds
        .iter()
        .map(|(_, bindings)| {
            bindings
                .iter()
                .map(|(_, actions)| {
                    std::mem::size_of::<((), Vec<()>)>()
                        .saturating_add(actions.len().saturating_mul(96))
                })
                .fold(0usize, usize::saturating_add)
        })
        .fold(std::mem::size_of::<KeybindsVec>(), usize::saturating_add)
}

fn event_payload_bytes(event: &Event) -> usize {
    let extra = match event {
        Event::TabUpdate(tabs) => tabs
            .iter()
            .map(|tab| tab.name.len().saturating_add(256))
            .fold(0usize, usize::saturating_add),
        Event::PaneUpdate(manifest) => manifest
            .panes
            .values()
            .flatten()
            .map(|pane| pane.title.len().saturating_add(192))
            .fold(0usize, usize::saturating_add),
        Event::CustomMessage(message, payload) => message.len().saturating_add(payload.len()),
        Event::FileSystemCreate(entries)
        | Event::FileSystemRead(entries)
        | Event::FileSystemUpdate(entries)
        | Event::FileSystemDelete(entries) => entries
            .iter()
            .map(|(path, _)| path.as_os_str().len().saturating_add(64))
            .fold(0usize, usize::saturating_add),
        Event::SessionUpdate(sessions, resurrectable) => sessions
            .iter()
            .map(session_info_payload_bytes)
            .chain(
                resurrectable
                    .iter()
                    .map(|(name, _)| name.len().saturating_add(64)),
            )
            .fold(0usize, usize::saturating_add),
        Event::RunCommandResult(_, stdout, stderr, context) => stdout
            .len()
            .saturating_add(stderr.len())
            .saturating_add(string_map_bytes(context)),
        Event::WebRequestResult(_, headers, body, context) => body
            .len()
            .saturating_add(string_map_bytes(headers))
            .saturating_add(string_map_bytes(context)),
        Event::ListClients(clients) => clients.len().saturating_mul(256),
        Event::PastedText(text) => text.len(),
        Event::HostFolderChanged(path) | Event::CwdChanged(_, path, _) => path.as_os_str().len(),
        Event::FailedToChangeHostFolder(error) | Event::FailedToWriteConfigToDisk(error) => {
            error.as_ref().map(|error| error.len()).unwrap_or(0)
        },
        Event::CommandChanged(_, command, _, _) => command
            .iter()
            .map(|part| part.len().saturating_add(16))
            .fold(0usize, usize::saturating_add),
        Event::PaneRenderReport(report) => pane_render_report_bytes(report),
        Event::ActionComplete(_, _, context) => string_map_bytes(context),
        Event::AvailableLayoutInfo(layouts, errors) => layouts
            .iter()
            .map(|layout| layout.name().len().saturating_add(64))
            .chain(
                errors
                    .iter()
                    .map(|error| error.layout_name.len().saturating_add(256)),
            )
            .fold(0usize, usize::saturating_add),
        Event::PluginConfigurationChanged(configuration) => string_map_bytes(configuration),
        Event::HighlightClicked {
            pattern,
            matched_string,
            context,
            ..
        } => pattern
            .len()
            .saturating_add(matched_string.len())
            .saturating_add(string_map_bytes(context)),
        Event::InitialKeybinds(keybinds) => keybinds_vec_bytes(keybinds),
        Event::HintText(hints) => hints
            .values()
            .map(|hint| {
                hint.text
                    .len()
                    .saturating_add(hint.indices.iter().map(Vec::len).sum())
            })
            .fold(0usize, usize::saturating_add),
        Event::ContextMenu(_, entries) => entries.len().saturating_mul(256),
        Event::AvailableKeybindPresets(presets, errors) => presets
            .iter()
            .map(|preset| preset.name.len().saturating_add(128))
            .chain(
                errors
                    .iter()
                    .map(|error| error.error.len().saturating_add(128)),
            )
            .fold(0usize, usize::saturating_add),
        Event::NestedSessionModeUpdate { session_path, .. } => session_path
            .iter()
            .map(|part| part.len().saturating_add(16))
            .fold(0usize, usize::saturating_add),
        _ => 0,
    };
    std::mem::size_of::<Event>().saturating_add(extra)
}

fn session_info_payload_bytes(session: &zellij_utils::data::SessionInfo) -> usize {
    let tabs = session
        .tabs
        .iter()
        .map(|tab| tab.name.len().saturating_add(256))
        .fold(0usize, usize::saturating_add);
    let panes = session
        .panes
        .panes
        .values()
        .flatten()
        .map(|pane| {
            pane.title
                .len()
                .saturating_add(pane.terminal_command.as_deref().map_or(0, str::len))
                .saturating_add(256)
        })
        .fold(0usize, usize::saturating_add);
    let plugins = session
        .plugins
        .values()
        .map(|plugin| {
            plugin
                .location
                .len()
                .saturating_add(string_map_bytes(&plugin.configuration))
                .saturating_add(256)
        })
        .fold(0usize, usize::saturating_add);
    session
        .name
        .len()
        .saturating_add(tabs)
        .saturating_add(panes)
        .saturating_add(plugins)
        .saturating_add(512)
}

fn pipe_message_payload_bytes(message: &PipeMessage) -> usize {
    let source_bytes = match &message.source {
        PipeSource::Cli(pipe_id) => pipe_id.len(),
        _ => 0,
    };
    source_bytes
        .saturating_add(message.name.len())
        .saturating_add(message.payload.as_ref().map(String::len).unwrap_or(0))
        .saturating_add(string_map_bytes(&message.args))
        .saturating_add(std::mem::size_of::<PipeMessage>())
}

fn event_or_pipe_message_bytes(entry: &EventOrPipeMessage) -> usize {
    match entry {
        EventOrPipeMessage::Event(event, _, _) => event_payload_bytes(event),
        EventOrPipeMessage::PipeMessage(message, _) => pipe_message_payload_bytes(message),
    }
}

pub const PIPE_POPUP_CLOSED_EXIT_CODE: i32 = 1;
pub const PIPE_POPUP_ERROR_EXIT_CODE: i32 = 2;
pub const PIPE_PLUGIN_UNAVAILABLE_EXIT_CODE: i32 = 1;

#[derive(Debug, Clone)]
enum PipePopup {
    Opening {
        queued: Vec<PipeMessage>,
        cli_client_id: ClientId,
    },
    Open {
        plugin_id: PluginId,
        client_id: ClientId,
        caller_args: BTreeMap<String, String>,
    },
}

#[derive(Debug, Clone)]
pub enum PipePopupRoute {
    New(PipeMessage),
    Queued,
    Deliver(PluginId, ClientId, PipeMessage),
}

#[derive(Debug, Clone, Default)]
pub struct PluginRenderAsset {
    // TODO: naming
    pub client_id: ClientId,
    pub plugin_id: PluginId,
    pub bytes: Vec<u8>,
    pub cli_pipes: HashMap<String, PipeStateChange>,
    pub rendered_size: Option<(usize, usize)>,
}

impl PluginRenderAsset {
    pub fn new(plugin_id: PluginId, client_id: ClientId, bytes: Vec<u8>) -> Self {
        PluginRenderAsset {
            client_id,
            plugin_id,
            bytes,
            ..Default::default()
        }
    }
    pub fn with_pipes(mut self, cli_pipes: HashMap<String, PipeStateChange>) -> Self {
        self.cli_pipes = cli_pipes;
        self
    }
    pub fn rendered_at(mut self, rows: usize, columns: usize) -> Self {
        self.rendered_size = Some((rows, columns));
        self
    }
}

#[derive(Debug, Clone)]
pub struct LoadingContext {
    pub plugin_id: PluginId,
    pub client_id: ClientId,
    pub plugin_cwd: PathBuf,
    pub plugin_own_data_dir: PathBuf,
    pub plugin_own_cache_dir: PathBuf,
    pub plugin_config: PluginConfig,
    pub tab_index: Option<usize>,
    pub path_to_default_shell: PathBuf,
    pub session_env_vars: std::collections::BTreeMap<String, String>,
    pub default_shell: Option<TerminalAction>,
    pub layout_dir: Option<PathBuf>,
    pub default_mode: InputMode,
    pub keybinds: SharedKeybinds,
    pub plugin_dir: PathBuf,
    pub size: Size,
    // Bytes resolved once for shared/non-shared classification and reused for the load.
    pub wasm_bytes: Option<Vec<u8>>,
    // identifies the instance this context creates, see `Quarantine`
    pub generation: u64,
}

impl LoadingContext {
    pub fn new(
        wasm_bridge: &WasmBridge,
        cwd: Option<PathBuf>,
        plugin_config: PluginConfig,
        plugin_id: PluginId,
        client_id: ClientId,
        tab_index: Option<usize>,
        size: Size,
    ) -> Self {
        let plugin_own_data_dir = ZELLIJ_SESSION_CACHE_DIR
            .join(make_plugin_url_path_safe(
                Url::from(&plugin_config.location).to_string(),
            ))
            .join(format!("{}-{}", plugin_id, client_id));
        let plugin_own_cache_dir = ZELLIJ_CACHE_DIR
            .join(make_plugin_url_path_safe(
                Url::from(&plugin_config.location).to_string(),
            ))
            .join(format!("plugin_cache"));
        let default_mode = wasm_bridge
            .base_modes
            .get(&client_id)
            .copied()
            .unwrap_or(wasm_bridge.default_mode);
        let keybinds = wasm_bridge
            .keybinds
            .get(&client_id)
            .cloned()
            .unwrap_or_else(|| wasm_bridge.default_keybinds.clone());

        LoadingContext {
            client_id,
            plugin_id,
            path_to_default_shell: wasm_bridge.path_to_default_shell.clone(),
            session_env_vars: wasm_bridge.session_env_vars.clone(),
            plugin_cwd: cwd.unwrap_or_else(|| wasm_bridge.zellij_cwd.clone()),
            default_shell: wasm_bridge.default_shell.clone(),
            layout_dir: wasm_bridge.layout_dir.clone(),
            keybinds,
            default_mode,
            plugin_own_data_dir,
            plugin_own_cache_dir,
            plugin_config,
            tab_index,
            plugin_dir: wasm_bridge.plugin_dir.clone(),
            size,
            wasm_bytes: None,
            generation: next_plugin_generation(),
        }
    }
    pub fn update_plugin_path(&mut self, new_path: PathBuf) {
        self.plugin_config.path = new_path;
        self.wasm_bytes = None;
    }
}

/// Compiled modules keyed by path and bound to the content digest
/// seen at compile time. A cached entry is served only when the file's
/// current digest matches — editing the wasm on disk invalidates the
/// cache entry instead of silently running a stale module.
pub type PluginCache = Arc<Mutex<HashMap<PathBuf, (String, Module)>>>;

const SLOT_ADDED_WAIT_MS: u64 = 200;

struct SharedInstanceInfo {
    key: SharedKey,
    loading_context: LoadingContext,
    slots: BTreeSet<PluginId>,
    background_slots: BTreeSet<PluginId>,
}

fn shared_event_context(
    event: &Event,
    slot_id: Option<PluginId>,
    client_id: Option<ClientId>,
) -> EventContext {
    let slot_id = match event {
        Event::Key(..)
        | Event::Mouse(..)
        | Event::MouseWithModifiers(..)
        | Event::PastedText(..)
        | Event::Visible(..)
        | Event::PermissionRequestResult(..)
        | Event::PluginConfigurationChanged(..) => slot_id,
        _ => None,
    };
    let client_id = match event {
        Event::PaneUpdate(..) | Event::CwdChanged(..) | Event::Timer(..) => None,
        _ => client_id,
    };
    EventContext { slot_id, client_id }
}

/// Plugins stopped after a fatal callback error.
///
/// Fatal reports are applied asynchronously, so a report can outlive the instance that sent it.
/// Each load gets a generation number from `next_plugin_generation`, and starting a new
/// generation for a plugin id (a reload) discards reports from older ones.
#[derive(Debug, Default)]
pub(crate) struct Quarantine {
    quarantined: HashSet<PluginId>,
    // the oldest generation still allowed to report for each plugin id
    oldest_current_generation: HashMap<PluginId, u64>,
}

impl Quarantine {
    pub fn contains(&self, plugin_id: &PluginId) -> bool {
        self.quarantined.contains(plugin_id)
    }
    pub fn ids(&self) -> &HashSet<PluginId> {
        &self.quarantined
    }
    /// A new instance is replacing the plugin: release it and ignore the old instance's reports.
    pub fn begin_generation(&mut self, plugin_id: PluginId, generation: u64) {
        self.quarantined.remove(&plugin_id);
        self.oldest_current_generation.insert(plugin_id, generation);
    }
    /// Returns true if the plugin was newly quarantined; reports from superseded instances and
    /// repeated reports return false.
    /// Whether a plugin was reloaded after the instance of `generation` was created.
    pub fn is_superseded(&self, plugin_id: PluginId, generation: u64) -> bool {
        self.oldest_current_generation
            .get(&plugin_id)
            .map(|oldest| generation < *oldest)
            .unwrap_or(false)
    }
    pub fn report_fatal(&mut self, plugin_id: PluginId, generation: u64) -> bool {
        !self.is_superseded(plugin_id, generation) && self.quarantined.insert(plugin_id)
    }
    /// The plugin id is gone for good.
    pub fn forget(&mut self, plugin_id: PluginId) {
        self.quarantined.remove(&plugin_id);
        self.oldest_current_generation.remove(&plugin_id);
    }
}

#[derive(Debug, Default)]
pub(crate) struct HoverGenerations {
    latest: HashMap<(PluginId, ClientId), u64>,
}

impl HoverGenerations {
    pub fn queue(&mut self, plugin_id: PluginId, client_id: ClientId) -> u64 {
        let generation = self.latest.entry((plugin_id, client_id)).or_insert(0);
        *generation += 1;
        *generation
    }
    pub fn is_superseded(&self, plugin_id: PluginId, client_id: ClientId, generation: u64) -> bool {
        self.latest
            .get(&(plugin_id, client_id))
            .map(|latest| *latest != generation)
            .unwrap_or(false)
    }
    /// Drop all generation state for a retired plugin — the map is
    /// otherwise retained for the session's lifetime.
    pub fn evict_plugin(&mut self, plugin_id: PluginId) {
        self.latest.retain(|(pid, _), _| *pid != plugin_id);
    }
    /// Drop all generation state for a disconnected client.
    pub fn evict_client(&mut self, client_id: ClientId) {
        self.latest.retain(|(_, cid), _| *cid != client_id);
    }
}

pub(crate) fn subscription_event_type(
    event: &Event,
) -> Result<EventType, <EventType as FromStr>::Err> {
    EventType::from_str(&event.to_string()).map(|event_type| {
        if event_type == EventType::MouseWithModifiers {
            EventType::Mouse
        } else {
            event_type
        }
    })
}

pub struct WasmBridge {
    latest_hovers: Arc<Mutex<HoverGenerations>>,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    senders: ThreadSenders,
    plugin_dir: PathBuf,
    plugin_map: Arc<Mutex<PluginMap>>,
    plugin_executor: Arc<PinnedExecutor>,
    next_plugin_id: PluginId,
    plugin_ids_waiting_for_permission_request: HashSet<PluginId>,
    // plugins whose loader job has not yet published the instances of all connected clients
    plugins_with_unfinished_load: HashSet<PluginId>,
    // generation and module digest of each plugin's latest permission prompt
    permission_prompts: HashMap<PluginId, (u64, String)>,
    cached_events_for_pending_plugins: PendingPluginCaches,
    cached_resizes_for_pending_plugins: HashMap<PluginId, (usize, usize)>, // (rows, columns)
    cached_worker_messages: HashMap<PluginId, Vec<(ClientId, String, String, String)>>, // Vec<clientid,
    // worker_name,
    // message,
    // payload>
    loading_plugins: HashSet<(PluginId, RunPlugin)>, // tracks loading plugins without handles
    plugins_to_unload_once_loaded: HashSet<PluginId>,
    quarantine: Quarantine,
    pending_plugin_reloads: HashSet<RunPlugin>,
    path_to_default_shell: PathBuf,
    watchers: HashMap<PluginId, (PathBuf, Debouncer<RecommendedWatcher, RecommendedCache>)>,
    zellij_cwd: PathBuf,
    session_env_vars: std::collections::BTreeMap<String, String>,
    default_shell: Option<TerminalAction>,
    cached_plugin_map:
        HashMap<RunPluginLocation, HashMap<PluginUserConfiguration, Vec<(PluginId, ClientId)>>>,
    pending_pipes: PendingPipes,
    pipe_popups: HashMap<String, PipePopup>,
    prompt_requests: PromptRequests,
    layout_dir: Option<PathBuf>,
    available_layouts: Vec<LayoutInfo>,
    available_layout_errors: Vec<LayoutWithError>,
    available_keybind_presets: Vec<KeybindPresetInfo>,
    available_keybind_preset_errors: Vec<KeybindPresetWithError>,
    default_mode: InputMode,
    default_keybinds: SharedKeybinds,
    keybinds: HashMap<ClientId, SharedKeybinds>,
    base_modes: HashMap<ClientId, InputMode>,
    downloader: Downloader,
    previous_pane_render_report: Option<PaneRenderReport>,
    last_host_terminal_theme_mode: Option<HostTerminalThemeMode>,
    pub last_global_attention: Option<zellij_utils::attention::GlobalAttention>,
    pub session_history_infos: HashMap<ClientId, zellij_utils::session_history::SessionHistoryInfo>,
    pub last_session_save_time: Arc<Mutex<Option<u64>>>, // milliseconds since UNIX epoch
    shared_instances: HashMap<PluginId, SharedInstanceInfo>,
    shared_keys: HashMap<SharedKey, PluginId>,
    shared_slot_owner: HashMap<PluginId, PluginId>,

    settings_entries: SettingsEntries,
    /// Generation captured by `set_timeout`, consumed by the next `update_plugins`.
    scheduled_timer_generation: Option<u64>,
}

impl WasmBridge {
    pub fn new(
        senders: ThreadSenders,
        engine: Engine,
        plugin_dir: PathBuf,
        path_to_default_shell: PathBuf,
        zellij_cwd: PathBuf,
        session_env_vars: std::collections::BTreeMap<String, String>,
        default_shell: Option<TerminalAction>,
        layout_dir: Option<PathBuf>,
        available_layouts: Vec<LayoutInfo>,
        available_layout_errors: Vec<LayoutWithError>,
        default_mode: InputMode,
        default_keybinds: SharedKeybinds,
    ) -> Self {
        let plugin_map = Arc::new(Mutex::new(PluginMap::default()));
        let connected_clients: Arc<Mutex<Vec<ClientId>>> = Arc::new(Mutex::new(vec![]));
        let plugin_cache: PluginCache = Arc::new(Mutex::new(HashMap::new()));
        let watchers = HashMap::new();
        let downloader = Downloader::new(ZELLIJ_CACHE_DIR.to_path_buf());
        let max_threads = num_cpus::get().max(4).min(16);
        let plugin_executor = Arc::new(PinnedExecutor::new(
            max_threads,
            &senders,
            &plugin_map,
            &connected_clients,
            &plugin_cache,
            &engine,
        ));
        WasmBridge {
            connected_clients,
            senders,
            plugin_dir,
            plugin_map,
            plugin_executor,
            path_to_default_shell,
            watchers,
            next_plugin_id: 0,
            cached_events_for_pending_plugins: PendingPluginCaches::default(),
            plugin_ids_waiting_for_permission_request: HashSet::new(),
            plugins_with_unfinished_load: HashSet::new(),
            permission_prompts: HashMap::new(),
            cached_resizes_for_pending_plugins: HashMap::new(),
            cached_worker_messages: HashMap::new(),
            loading_plugins: HashSet::new(),
            plugins_to_unload_once_loaded: HashSet::new(),
            quarantine: Quarantine::default(),
            pending_plugin_reloads: HashSet::new(),
            zellij_cwd,
            session_env_vars,
            default_shell,
            cached_plugin_map: HashMap::new(),
            pending_pipes: Default::default(),
            pipe_popups: HashMap::new(),
            prompt_requests: PromptRequests::default(),
            layout_dir,
            available_layouts,
            available_layout_errors,
            available_keybind_presets: vec![],
            available_keybind_preset_errors: vec![],
            default_mode,
            default_keybinds,
            keybinds: HashMap::new(),
            base_modes: HashMap::new(),
            downloader,
            previous_pane_render_report: None,
            latest_hovers: Arc::new(Mutex::new(HoverGenerations::default())),
            last_host_terminal_theme_mode: None,
            last_global_attention: None,
            session_history_infos: HashMap::new(),
            last_session_save_time: Arc::new(Mutex::new(None)),
            shared_instances: HashMap::new(),
            shared_keys: HashMap::new(),
            shared_slot_owner: HashMap::new(),

            settings_entries: SettingsEntries::default(),
            scheduled_timer_generation: None,
        }
    }
    pub fn load_plugin(
        &mut self,
        run: &Option<RunPlugin>,
        tab_index: Option<usize>,
        size: Size,
        cwd: Option<PathBuf>,
        skip_cache: bool,
        client_id: Option<ClientId>,
    ) -> Result<(PluginId, ClientId)> {
        self.load_plugin_with_kind(run, tab_index, size, cwd, skip_cache, client_id, false)
    }
    pub fn load_plugin_with_kind(
        &mut self,
        run: &Option<RunPlugin>,
        tab_index: Option<usize>,
        size: Size,
        cwd: Option<PathBuf>,
        skip_cache: bool,
        client_id: Option<ClientId>,
        is_background: bool,
    ) -> Result<(PluginId, ClientId)> {
        let _err_context = move || format!("failed to load plugin");

        let client_id = client_id
            .and_then(|client_id| {
                // first attempt to use a connected client (because this might be a cli_client that
                // should not get plugins) and only if none is connected, load a "dummy" plugin for
                // the cli client
                let connected_clients = self.connected_clients.lock().unwrap();
                if connected_clients.contains(&client_id) {
                    Some(client_id)
                } else {
                    None
                }
            })
            .or_else(|| {
                // if no client id was provided, try to use the first connected client
                self.connected_clients
                    .lock()
                    // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                    .unwrap()
                    .iter()
                    .next()
                    .copied()
            })
            .or(client_id) // if we got here, this is likely a cli client with no other clients
            // connected, or loading a background plugin on app start, we use the provided client id as a dummy to load the
            // plugin anyway
            .with_context(|| {
                "Plugins must have a client id, none was provided and none are connected"
            })?;

        let preloaded_wasm_bytes = run.as_ref().and_then(|run| {
            let canonical = canonical_run_plugin(run);
            PluginConfig::from_run_plugin(&canonical)
                .and_then(|plugin_config| self.plugin_wasm_bytes(&plugin_config))
        });
        if let Some(run) = run {
            let run = canonical_run_plugin(run);
            if let Some(plugin_config) = PluginConfig::from_run_plugin(&run) {
                if Self::module_bytes_are_shared(preloaded_wasm_bytes.as_deref()) {
                    return self.load_shared_slot(
                        run,
                        plugin_config,
                        tab_index,
                        size,
                        cwd,
                        skip_cache,
                        client_id,
                        is_background,
                        preloaded_wasm_bytes,
                    );
                }
            }
        }

        let plugin_id = self.next_plugin_id;

        match run {
            Some(run) => {
                let plugin = match PluginConfig::from_run_plugin(run) {
                    Some(plugin) => plugin,
                    None => {
                        self.next_plugin_id += 1;
                        let mut loading_indication =
                            LoadingIndication::new(run.location.to_string());
                        handle_plugin_loading_failure(
                            &self.senders,
                            plugin_id,
                            &mut loading_indication,
                            format!("Failed to resolve plugin: {}", run.location),
                            Some(client_id),
                        );
                        return Ok((plugin_id, client_id));
                    },
                };
                let plugin_name = run.location.to_string();

                self.cached_events_for_pending_plugins
                    .insert_empty(plugin_id);
                self.plugins_with_unfinished_load.insert(plugin_id);
                self.cached_resizes_for_pending_plugins
                    .insert(plugin_id, (size.rows, size.cols));
                self.loading_plugins.insert((plugin_id, run.clone()));

                // Clone for threaded contexts
                let plugin_executor = self.plugin_executor.clone();
                let senders = self.senders.clone();
                let zellij_cwd = cwd.unwrap_or_else(|| self.zellij_cwd.clone());

                // Check if we need to download (async I/O required)
                let needs_download = matches!(plugin.location, RunPluginLocation::Remote(_));

                let mut loading_context = LoadingContext::new(
                    &self,
                    Some(zellij_cwd.clone()),
                    plugin.clone(), // TODO: rename to plugin_config
                    plugin_id,
                    client_id,
                    tab_index,
                    size,
                );
                loading_context.wasm_bytes = preloaded_wasm_bytes;

                if needs_download {
                    let downloader = self.downloader.clone();
                    get_tokio_runtime().spawn(async move {
                        let _ = senders.send_to_background_jobs(
                            BackgroundJob::AnimatePluginLoading(plugin_id),
                        );
                        let mut loading_indication = LoadingIndication::new(plugin_name.clone());

                        if let RunPluginLocation::Remote(url) = &plugin.location {
                            let file_name: String = PortableHash::default()
                                .hash128(url.as_bytes())
                                .iter()
                                .map(ToString::to_string)
                                .collect();

                            match downloader.download(url, Some(&file_name)).await {
                                Ok(_) => loading_context
                                    .update_plugin_path(ZELLIJ_CACHE_DIR.join(&file_name)),
                                Err(e) => {
                                    handle_plugin_loading_failure(
                                        &senders,
                                        plugin_id,
                                        &mut loading_indication,
                                        e,
                                        Some(client_id),
                                    );
                                    let _ = senders.send_to_plugin(
                                        PluginInstruction::ApplyCachedEvents {
                                            plugin_ids: vec![plugin_id],
                                            done_receiving_permissions: false,
                                        },
                                    );
                                    return;
                                },
                            }
                        }

                        let load_bytes = loading_context
                            .wasm_bytes
                            .as_ref()
                            .map(Vec::len)
                            .unwrap_or(0);
                        let executor = plugin_executor.clone();
                        let failure_indication = loading_indication.clone();
                        if let Err(e) = plugin_executor.execute_plugin_load_with_payload(
                            plugin_id,
                            load_bytes,
                            move |senders: ThreadSenders,
                                  plugin_map: Arc<Mutex<PluginMap>>,
                                  connected_clients: Arc<Mutex<Vec<ClientId>>>,
                                  plugin_cache: PluginCache,
                                  engine| {
                                match PluginLoader::new(
                                    skip_cache,
                                    loading_context,
                                    senders.clone(),
                                    engine.clone(),
                                    plugin_cache.clone(),
                                    plugin_map.clone(),
                                    connected_clients.clone(),
                                )
                                .start_plugin()
                                {
                                    Ok(_) => {
                                        let plugin_list = plugin_map.lock().unwrap().list_plugins();
                                        handle_plugin_successful_loading(
                                            &senders,
                                            plugin_id,
                                            plugin_list,
                                        );
                                    },
                                    Err(e) => handle_plugin_loading_failure(
                                        &senders,
                                        plugin_id,
                                        &mut loading_indication,
                                        e,
                                        Some(client_id),
                                    ),
                                }
                                if !plugin_map.lock().unwrap().has_instance_of(plugin_id) {
                                    executor.unregister_plugin(plugin_id);
                                }

                                let _ =
                                    senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                                        plugin_ids: vec![plugin_id],
                                        done_receiving_permissions: false,
                                    });
                            },
                        ) {
                            let mut failure_indication = failure_indication;
                            handle_plugin_loading_failure(
                                &senders,
                                plugin_id,
                                &mut failure_indication,
                                e,
                                Some(client_id),
                            );
                            let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                                plugin_ids: vec![plugin_id],
                                done_receiving_permissions: false,
                            });
                        }
                    });
                } else {
                    let _ = senders
                        .send_to_background_jobs(BackgroundJob::AnimatePluginLoading(plugin_id));
                    let mut loading_indication = LoadingIndication::new(plugin_name.clone());

                    let load_bytes = loading_context
                        .wasm_bytes
                        .as_ref()
                        .map(Vec::len)
                        .unwrap_or(0);
                    let executor = self.plugin_executor.clone();
                    let failure_indication = loading_indication.clone();
                    if let Err(e) = self.plugin_executor.execute_plugin_load_with_payload(
                        plugin_id,
                        load_bytes,
                        move |senders,
                              plugin_map,
                              connected_clients,
                              plugin_cache: PluginCache,
                              engine: Engine| {
                            match PluginLoader::new(
                                skip_cache,
                                loading_context,
                                senders.clone(),
                                engine.clone(),
                                plugin_cache.clone(),
                                plugin_map.clone(),
                                connected_clients.clone(),
                            )
                            .start_plugin()
                            {
                                Ok(_) => {
                                    let plugin_list = plugin_map.lock().unwrap().list_plugins();
                                    handle_plugin_successful_loading(
                                        &senders,
                                        plugin_id,
                                        plugin_list,
                                    );
                                },
                                Err(e) => handle_plugin_loading_failure(
                                    &senders,
                                    plugin_id,
                                    &mut loading_indication,
                                    e,
                                    Some(client_id),
                                ),
                            }
                            if !plugin_map.lock().unwrap().has_instance_of(plugin_id) {
                                executor.unregister_plugin(plugin_id);
                            }

                            let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                                plugin_ids: vec![plugin_id],
                                done_receiving_permissions: false,
                            });
                        },
                    ) {
                        let mut failure_indication = failure_indication;
                        handle_plugin_loading_failure(
                            &self.senders,
                            plugin_id,
                            &mut failure_indication,
                            e,
                            Some(client_id),
                        );
                        self.rollback_pending_plugin_load(plugin_id);
                        self.next_plugin_id += 1;
                        return Err(anyhow!(
                            "failed to admit plugin load for {}: {}",
                            plugin_id,
                            e
                        ));
                    }
                }

                self.next_plugin_id += 1;
            },
            None => {
                self.next_plugin_id += 1;
                let mut loading_indication = LoadingIndication::new(format!("{}", plugin_id));
                handle_plugin_loading_failure(
                    &self.senders,
                    plugin_id,
                    &mut loading_indication,
                    "Failed to resolve plugin alias",
                    None,
                );
            },
        }
        Ok((plugin_id, client_id))
    }
    pub fn unload_plugin(&mut self, pid: PluginId) -> Result<()> {
        info!("Bye from plugin {}", &pid);
        self.quarantine.forget(pid);
        self.permission_prompts.remove(&pid);
        self.clear_pending_plugin_state(pid);
        self.latest_hovers.lock().unwrap().evict_plugin(pid);
        let _ = self
            .senders
            .send_to_screen(ScreenInstruction::ClearLiveViewsOfPlugin(pid));
        if self.shared_slot_owner.contains_key(&pid) {
            self.unload_shared_slot(pid);
            return Ok(());
        }
        let still_loading = self
            .loading_plugins
            .iter()
            .any(|(loading_plugin_id, _)| *loading_plugin_id == pid);
        if still_loading {
            self.loading_plugins
                .retain(|(loading_plugin_id, _)| *loading_plugin_id != pid);
            self.plugins_to_unload_once_loaded.insert(pid);
        }

        // Remove from plugin_map on main thread
        let plugins_to_cleanup: Vec<_> = {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            plugin_map.remove_plugins(pid).into_iter().collect()
        };

        // Schedule cleanup of every client instance as a single job on the
        // plugin's pinned thread: all instances share one executor
        // assignment, and submitting a separate unload per instance lets the
        // first job's unregistration drop the remaining submissions before
        // their cleanup runs.
        let mut cleanups: Vec<PluginWork> = Vec::with_capacity(plugins_to_cleanup.len());
        for ((plugin_id, client_id), (running_plugin, subscriptions, workers)) in plugins_to_cleanup
        {
            // `load_alive` was cloned out at refuel. Clearing it does not take
            // the execution mutex a guest callback may be holding.
            revoke_client_loads(plugin_id, client_id);
            // Release this plugin's key intercept for the client without asking the
            // RunningPlugin whether it has one. The screen only drops an intercept
            // this plugin owns.
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(
                    plugin_id, client_id,
                ));
            // Clear any regex highlights this plugin registered across all panes
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::ClearAllPluginHighlights(plugin_id));

            // Send worker exit messages
            for (_worker_name, worker_sender) in workers {
                let _ = try_send_to_worker(&worker_sender, MessageToWorker::Exit);
            }

            cleanups.push(Box::new(
                move |senders, _plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let needs_before_close = subscriptions
                        .lock()
                        .unwrap()
                        .contains(&EventType::BeforeClose);
                    // BeforeClose is a guest callback. Run it only on the plugin
                    // executor, and only if the execution mutex is free. Inline
                    // host cleanup (queue rejection on the management thread)
                    // must not enter the guest or wait for it.
                    if on_plugin_executor() {
                        if let Some(mut rp) = try_lock_running(&running_plugin) {
                            if needs_before_close && !rp.is_quarantined() {
                                match apply_before_close_event_to_plugin(
                                    plugin_id,
                                    client_id,
                                    &mut rp,
                                    senders.clone(),
                                ) {
                                    Ok(()) => {},
                                    Err(e) => {
                                        log::error!("{:?}", e);
                                        let stringified_error =
                                            format!("{:?}", e).replace("\n", "\n\r");
                                        show_plugin_crash(
                                            plugin_id,
                                            stringified_error,
                                            senders.clone(),
                                        );
                                    },
                                }
                            }
                            rp.store.data().retire_load();
                        }
                    }
                    let mut data_dirs = take_client_data_dirs(plugin_id, client_id);
                    if data_dirs.is_empty() {
                        if let Some(rp) = try_lock_running(&running_plugin) {
                            data_dirs.push(rp.store.data().plugin_own_data_dir.clone());
                        }
                    }
                    for cache_dir in data_dirs {
                        if let Err(e) = std::fs::remove_dir_all(&cache_dir) {
                            log::error!("Failed to remove cache dir for plugin: {:?}", e);
                        }
                    }

                    drop(running_plugin);
                    drop(subscriptions);
                },
            ));
        }
        // A plugin whose initialization failed has no instance left to clean up, but it still
        // holds its executor assignment; retire that too. One that is still loading is unloaded
        // again once it has loaded, and retiring it now would drop that second unload.
        if !cleanups.is_empty() || !still_loading {
            if let Err(e) = self
                .plugin_executor
                .execute_plugin_unload_many(pid, cleanups)
            {
                log::warn!(
                    "Plugin {} was retired without guest cleanup after admission failed: {}",
                    pid,
                    e
                );
            }
        }

        // Main thread cleanup
        self.cached_plugin_map.clear();
        let pipes_to_release = self.pending_pipes.unload_plugin(&pid);
        self.send_pipe_releases(pipes_to_release);
        self.forget_pipe_popups_of_plugin(pid);
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::SessionSuggestions(
                crate::session_suggestions::SessionSuggestionsJob::Unsubscribe { plugin_id: pid },
            ));
        let prompt_actions = self.prompt_requests.plugin_unloaded(pid);
        self.apply_prompt_actions(prompt_actions);
        self.settings_entries_changed(|entries| entries.plugin_unloaded(pid));
        let plugin_list = self.plugin_map.lock().unwrap().list_plugins();
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));

        Ok(())
    }
    fn clear_pending_plugin_state(&mut self, plugin_id: PluginId) {
        self.plugin_ids_waiting_for_permission_request
            .remove(&plugin_id);
        self.cached_events_for_pending_plugins.take(&plugin_id);
        self.cached_resizes_for_pending_plugins.remove(&plugin_id);
        self.cached_worker_messages.remove(&plugin_id);
        if let Some((_, watcher)) = self.watchers.remove(&plugin_id) {
            watcher.stop_nonblocking();
        }
    }
    pub fn quarantine_plugin(&mut self, pid: PluginId, generation: u64) {
        // A report can arrive after the plugin was unloaded; recording it would leave an entry
        // for a dead id that nothing ever clears.
        if self.plugin_map.lock().unwrap().metadata(pid).is_none()
            && !self.shared_instances.contains_key(&pid)
        {
            return;
        }
        // A shared instance keeps executing under its own id, so a fatal error reported for
        // one of its pane slots stops the whole instance, not just that slot.
        if let Some(instance_id) = self.shared_slot_owner.get(&pid).copied() {
            if self.quarantine.report_fatal(instance_id, generation) {
                self.finish_quarantine(instance_id);
            }
        }
        if !self.quarantine.report_fatal(pid, generation) {
            return;
        }
        log::error!("Quarantining plugin {} after a fatal callback error", pid);
        self.finish_quarantine(pid);
    }
    fn finish_quarantine(&mut self, plugin_id: PluginId) {
        self.plugin_map.lock().unwrap().mark_quarantined(plugin_id);
        if let Some(key) = self
            .shared_instances
            .get(&plugin_id)
            .map(|info| info.key.clone())
        {
            if self.shared_keys.get(&key) == Some(&plugin_id) {
                self.shared_keys.remove(&key);
            }
        }
        // Quarantine discards the plugin's updates, so a key intercept it still holds would
        // swallow the client's typing until the plugin is unloaded.
        self.release_key_intercepts(plugin_id);
        self.clear_pending_plugin_state(plugin_id);
        let pipes_to_release = self.pending_pipes.unload_plugin(&plugin_id);
        self.send_pipe_releases(pipes_to_release);
        self.forget_pipe_popups_of_plugin(plugin_id);
    }
    /// Drop `plugin_id`'s key intercept for every connected client. The screen only drops an
    /// intercept the plugin owns, so this is harmless for clients it never intercepted.
    fn release_key_intercepts(&self, plugin_id: PluginId) {
        let clients: Vec<ClientId> = self.connected_clients.lock().unwrap().clone();
        for client_id in clients {
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(
                    plugin_id, client_id,
                ));
        }
    }
    fn prepare_plugin_reload(&mut self, plugin_id: PluginId) {
        let _ = self
            .senders
            .send_to_screen(ScreenInstruction::ClearLiveViewsOfPlugin(plugin_id));
        let prompt_actions = self.prompt_requests.caller_reloaded(plugin_id);
        self.apply_prompt_actions(prompt_actions);
        // the reloaded plugin registers again if it still wants an entry
        self.unregister_settings_entry(plugin_id);
    }
    /// Undo pending-load state when its lifecycle admission is rejected. No
    /// loader closure ran, so nothing else will settle these entries.
    fn rollback_pending_plugin_load(&mut self, plugin_id: PluginId) {
        if let Some(cached) = self.cached_events_for_pending_plugins.take(&plugin_id) {
            self.release_cached_cli_pipes_with_error(&cached.entries);
        }
        self.cached_resizes_for_pending_plugins.remove(&plugin_id);
        self.cached_worker_messages.remove(&plugin_id);
        self.plugins_with_unfinished_load.remove(&plugin_id);
        self.plugin_ids_waiting_for_permission_request
            .remove(&plugin_id);
        self.permission_prompts.remove(&plugin_id);
        self.plugins_to_unload_once_loaded.remove(&plugin_id);
        self.loading_plugins
            .retain(|(loading_plugin_id, _)| *loading_plugin_id != plugin_id);
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    }
    pub fn reload_plugin_with_id(&mut self, plugin_id: u32) -> Result<()> {
        self.reload_plugin_with_id_and_bytes(plugin_id, None)
    }
    fn reload_plugin_with_id_and_bytes(
        &mut self,
        plugin_id: u32,
        wasm_bytes: Option<Vec<u8>>,
    ) -> Result<()> {
        if let Some(instance_id) = self.shared_instance_of(plugin_id) {
            self.prepare_plugin_reload(plugin_id);
            self.reload_shared_instance_with_bytes(instance_id, wasm_bytes);
            return Ok(());
        }
        let Some(run_plugin) = self.run_plugin_of_plugin_id(plugin_id).map(|r| r.clone()) else {
            log::error!("Failed to find plugin with id: {}", plugin_id);
            return Ok(());
        };

        let Some(first_client_id) = self.get_first_client_id() else {
            log::error!("No connected clients, cannot reload plugin.");
            return Ok(());
        };
        let Some(plugin_config) = self.plugin_config_of_plugin_id(plugin_id) else {
            log::error!("Could not find running plugin with id: {}", plugin_id);
            return Ok(());
        };
        let tab_index = self.tab_index_of_plugin_id(plugin_id);
        let Some(size) = self.size_of_plugin_id(plugin_id) else {
            log::error!(
                "Could not find size of running plugin with id: {}",
                plugin_id
            );
            return Ok(());
        };
        let size = Size {
            rows: size.0,
            cols: size.1,
        };

        let cwd = self.cwd_of_plugin_id(plugin_id);
        let mut loading_context = LoadingContext::new(
            &self,
            cwd,
            plugin_config,
            plugin_id,
            first_client_id,
            tab_index,
            size,
        );
        loading_context.wasm_bytes = wasm_bytes;
        let load_bytes = loading_context
            .wasm_bytes
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0);

        let reservation = self
            .plugin_executor
            .reserve_job(plugin_id, JobClass::Lifecycle, load_bytes)
            .with_context(|| format!("failed to admit reload of plugin {plugin_id}"))?;

        self.prepare_plugin_reload(plugin_id);
        self.cached_events_for_pending_plugins
            .insert_empty(plugin_id);
        self.plugins_with_unfinished_load.insert(plugin_id);
        self.cached_resizes_for_pending_plugins
            .insert(plugin_id, (size.rows, size.cols));

        let mut loading_indication = LoadingIndication::new(run_plugin.location.to_string());
        self.start_plugin_loading_indication(&[plugin_id], &loading_indication);
        self.loading_plugins.insert((plugin_id, run_plugin.clone()));
        // reports from the instance being replaced must not quarantine its successor
        self.quarantine
            .begin_generation(plugin_id, loading_context.generation);

        reservation.commit(Box::new(
            move |senders, plugin_map, connected_clients, plugin_cache, engine| {
                let skip_cache = true; // we want to explicitly reload the plugin
                match PluginLoader::new(
                    skip_cache,
                    loading_context,
                    senders.clone(),
                    engine.clone(),
                    plugin_cache.clone(),
                    plugin_map.clone(),
                    connected_clients.clone(),
                )
                .start_plugin()
                {
                    Ok(_) => {
                        let plugin_list = plugin_map.lock().unwrap().list_plugins();
                        handle_plugin_successful_loading(&senders, plugin_id, plugin_list);
                    },
                    Err(e) => handle_plugin_loading_failure(
                        &senders,
                        plugin_id,
                        &mut loading_indication,
                        e,
                        Some(first_client_id),
                    ),
                }
                let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                    plugin_ids: vec![plugin_id],
                    done_receiving_permissions: false,
                });
            },
        ));
        Ok(())
    }
    pub fn reload_plugin(&mut self, run_plugin: &RunPlugin) -> Result<()> {
        let canonical = canonical_run_plugin(run_plugin);
        let wasm_bytes = PluginConfig::from_run_plugin(&canonical)
            .and_then(|plugin_config| self.plugin_wasm_bytes(&plugin_config));
        if Self::module_bytes_are_shared(wasm_bytes.as_deref()) {
            let instance_ids = self.shared_instances_matching(&canonical);
            if instance_ids.is_empty() {
                return Err(ZellijError::PluginDoesNotExist)
                    .with_context(|| format!("failed to reload {}", canonical.location));
            }
            for instance_id in instance_ids {
                self.reload_shared_instance_with_bytes(instance_id, wasm_bytes.clone());
            }
            return Ok(());
        }
        if self.plugin_is_currently_being_loaded(&run_plugin.location) {
            self.pending_plugin_reloads.insert(run_plugin.clone());
            return Ok(());
        }

        let plugin_ids = self
            .all_plugin_ids_for_plugin_location(&run_plugin.location, &run_plugin.configuration)?;
        for plugin_id in &plugin_ids {
            self.reload_plugin_with_id_and_bytes(*plugin_id, wasm_bytes.clone())?;
        }
        Ok(())
    }
    pub fn add_client(&mut self, client_id: ClientId) -> Result<()> {
        if self.client_is_connected(&client_id) {
            return Ok(());
        }

        let new_plugins: HashSet<PluginId> = {
            let plugin_map = self.plugin_map.lock().unwrap();
            self.connected_clients.lock().unwrap().push(client_id);
            plugin_map.plugin_ids().into_iter().collect()
        };
        self.add_client_to_shared_instances(client_id);
        for plugin_id in new_plugins {
            if self.shared_slot_owner.contains_key(&plugin_id) {
                continue;
            }
            if self
                .plugin_map
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .contains(plugin_id, client_id)
            {
                continue;
            }
            let Some(run_plugin) = self.run_plugin_of_plugin_id(plugin_id).map(|r| r.clone())
            else {
                log::error!("Failed to find plugin with id: {}", plugin_id);
                return Ok(());
            };

            let Some(plugin_config) = self.plugin_config_of_plugin_id(plugin_id) else {
                log::error!("Could not find running plugin with id: {}", plugin_id);
                return Ok(());
            };
            let tab_index = self.tab_index_of_plugin_id(plugin_id);
            let Some(size) = self.size_of_plugin_id(plugin_id) else {
                log::error!(
                    "Could not find size of running plugin with id: {}",
                    plugin_id
                );
                return Ok(());
            };
            let (rows, columns) = size;
            let size = Size {
                rows,
                cols: columns,
            };

            let cwd = self.cwd_of_plugin_id(plugin_id);

            let mut loading_indication = LoadingIndication::new(run_plugin.location.to_string());
            let plugin_executor = self.plugin_executor.clone();
            let loading_executor = plugin_executor.clone();

            let loading_context = LoadingContext::new(
                &self,
                cwd,
                plugin_config,
                plugin_id,
                client_id,
                tab_index,
                size,
            );

            let reservation = match plugin_executor.reserve_job(plugin_id, JobClass::Lifecycle, 0) {
                Ok(reservation) => reservation,
                Err(e) => {
                    handle_plugin_loading_failure(
                        &self.senders,
                        plugin_id,
                        &mut loading_indication,
                        e,
                        Some(client_id),
                    );
                    continue;
                },
            };
            self.cached_events_for_pending_plugins
                .insert_empty(plugin_id);
            self.plugins_with_unfinished_load.insert(plugin_id);
            self.cached_resizes_for_pending_plugins
                .insert(plugin_id, (rows, columns));
            self.start_plugin_loading_indication_for_client(
                plugin_id,
                client_id,
                &loading_indication,
            );
            self.loading_plugins.insert((plugin_id, run_plugin.clone()));

            reservation.commit(Box::new(
                move |senders, plugin_map, connected_clients, plugin_cache, engine| {
                    let skip_cache = false;
                    match PluginLoader::new(
                        skip_cache,
                        loading_context,
                        senders.clone(),
                        engine.clone(),
                        plugin_cache.clone(),
                        plugin_map.clone(),
                        connected_clients.clone(),
                    )
                    .without_connected_clients()
                    .start_plugin()
                    {
                        Ok(_) => {
                            let _ = senders
                                .send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
                            let _ = senders.send_to_background_jobs(
                                BackgroundJob::StopPluginLoadingAnimation(plugin_id),
                            );
                            let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                                plugin_ids: vec![plugin_id],
                                done_receiving_permissions: false,
                            });
                        },
                        Err(e) => {
                            handle_plugin_loading_failure(
                                &senders,
                                plugin_id,
                                &mut loading_indication,
                                e,
                                Some(client_id),
                            );
                            if !plugin_map.lock().unwrap().has_instance_of(plugin_id) {
                                loading_executor.unregister_plugin(plugin_id);
                            }
                            let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                                plugin_ids: vec![plugin_id],
                                done_receiving_permissions: false,
                            });
                        },
                    }
                },
            ));
        }
        Ok(())
    }
    pub fn resize_plugin(
        &mut self,
        pid: PluginId,
        new_columns: usize,
        new_rows: usize,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let err_context = move || format!("failed to resize plugin {pid}");

        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .set_size(pid, new_rows, new_columns);
        if self.quarantine.contains(&pid) {
            return Ok(());
        }
        if let Some(instance_id) = self.shared_slot_owner.get(&pid).copied() {
            let _s = shutdown_sender.clone();
            let generation = self
                .shared_instances
                .get(&instance_id)
                .map(|info| info.loading_context.generation)
                .unwrap_or(0);
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Replaceable(replaceable_resize_key(pid, generation)),
                0,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let _s = _s;
                    resize_job(senders, plugin_map, instance_id, pid, new_rows, new_columns);
                },
            );
            return Ok(());
        }

        let plugins_to_resize: Vec<(PluginId, ClientId, Arc<Mutex<RunningPlugin>>)> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins()
            .iter()
            .cloned()
            .filter(|(plugin_id, _client_id, _running_plugin)| {
                !self
                    .cached_resizes_for_pending_plugins
                    .contains_key(&plugin_id)
            })
            .collect();
        for (plugin_id, client_id, running_plugin) in plugins_to_resize {
            if plugin_id == pid {
                let event_id = running_plugin
                    .lock()
                    // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                    .unwrap()
                    .next_event_id(AtomicEvent::Resize);
                // Execute directly on pinned thread (no async I/O needed for resize/render)
                self.plugin_executor.execute_for_plugin_with(
                    plugin_id,
                    JobClass::Replaceable(replaceable_resize_key(client_id, 0)),
                    0,
                    {
                        // let senders = self.senders.clone();
                        let running_plugin = running_plugin.clone();
                        let _s = shutdown_sender.clone();
                        move |senders, _plugin_map, _connected_clients, _plugin_cache, _engine| {
                            let mut running_plugin = running_plugin.lock().unwrap();
                            let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                            if !running_plugin.is_active() {
                                return;
                            }
                            if running_plugin.apply_event_id(AtomicEvent::Resize, event_id) {
                                let old_rows = running_plugin.rows;
                                let old_columns = running_plugin.columns;
                                running_plugin.rows = new_rows;
                                running_plugin.columns = new_columns;

                                // in the below conditional, we check if event_id == 0 so that we'll
                                // make sure to always render on the first resize event
                                if old_rows != new_rows
                                    || old_columns != new_columns
                                    || event_id == 0
                                {
                                    crate::plugins::shared::refuel_for_callback(
                                        &mut running_plugin.store,
                                    );
                                    let rendered_bytes = running_plugin
                                        .instance
                                        .clone()
                                        .get_typed_func::<(i32, i32), ()>(
                                            &mut running_plugin.store,
                                            "render",
                                        )
                                        .and_then(|render| {
                                            render.call(
                                                &mut running_plugin.store,
                                                (new_rows as i32, new_columns as i32),
                                            )
                                        })
                                        .map_err(|e| anyhow!(e))
                                        .and_then(|_| {
                                            wasi_read_string(running_plugin.store.data())
                                                .map_err(|e| anyhow!(e))
                                        })
                                        .with_context(err_context);
                                    match rendered_bytes {
                                        Ok(rendered_bytes) => {
                                            let plugin_render_asset = PluginRenderAsset::new(
                                                plugin_id,
                                                client_id,
                                                rendered_bytes.as_bytes().to_vec(),
                                            )
                                            .rendered_at(new_rows, new_columns);
                                            // Best-effort: screen disconnect during teardown must
                                            // not panic the plugin worker.
                                            let _ = senders.send_to_screen(
                                                ScreenInstruction::PluginBytes(vec![
                                                    plugin_render_asset,
                                                ]),
                                            );
                                        },
                                        Err(e) => {
                                            log::error!("{}", e);
                                            // Other render failures only skip this frame, as they
                                            // always have; a plugin that exhausts its fuel while
                                            // rendering is stopped like one that does so in an
                                            // event handler.
                                            // TODO(plugin-hardening): a render that traps for any
                                            // other reason is still only logged here.
                                            if crate::plugins::shared::is_out_of_fuel(&e) {
                                                running_plugin.mark_quarantined();
                                                handle_plugin_crash(
                                                    plugin_id,
                                                    running_plugin.store.data().generation,
                                                    format!("{:?}", e).replace("\n", "\n\r"),
                                                    senders.clone(),
                                                );
                                            }
                                        },
                                    }
                                }
                            }
                        }
                    },
                );
            }
        }
        for (plugin_id, current_size) in self.cached_resizes_for_pending_plugins.iter_mut() {
            if *plugin_id == pid {
                current_size.0 = new_rows;
                current_size.1 = new_columns;
            }
        }
        Ok(())
    }
    pub fn set_scheduled_timer_generation(&mut self, generation: u64) {
        self.scheduled_timer_generation = Some(generation);
    }
    pub fn update_plugins(
        &mut self,
        mut updates: Vec<(Option<PluginId>, Option<ClientId>, Event)>,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let scheduled_timer_generation = self.scheduled_timer_generation.take();
        for (plugin_id, client_id, event) in updates.iter() {
            if plugin_id.is_none() && client_id.is_none() {
                if let Event::HostTerminalThemeChanged(mode) = event {
                    self.last_host_terminal_theme_mode = Some(*mode);
                }
            }
        }
        updates = self.dispatch_shared_updates(updates, shutdown_sender.clone());
        let plugins_to_update: Vec<(
            PluginId,
            ClientId,
            Arc<Mutex<RunningPlugin>>,
            Arc<Mutex<Subscriptions>>,
        )> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins_and_subscriptions()
            .iter()
            .cloned()
            .filter(|(plugin_id, _client_id, _running_plugin, _subscriptions)| {
                !self.cached_events_for_pending_plugins.contains(plugin_id)
                    && !self.quarantine.contains(plugin_id)
            })
            .collect();

        // Execute each plugin update on its respective pinned thread
        let plugin_executor = self.plugin_executor.clone();
        // let senders = self.senders.clone();
        for (pid, cid, event) in updates.clone().into_iter() {
            let event_bytes = event_payload_bytes(&event);
            for (plugin_id, client_id, running_plugin, subscriptions) in &plugins_to_update {
                let subs = subscriptions.lock().unwrap().clone();
                // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                if let Ok(event_type) = subscription_event_type(&event) {
                    if (subs.contains(&event_type)
                        || event_type == EventType::PermissionRequestResult)
                        && Self::message_is_directed_at_plugin(pid, cid, plugin_id, client_id)
                    {
                        let hover_generation = if is_hover(&event) {
                            Some(
                                self.latest_hovers
                                    .lock()
                                    // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                                    .unwrap()
                                    .queue(*plugin_id, *client_id),
                            )
                        } else {
                            None
                        };
                        let latest_hovers = self.latest_hovers.clone();
                        // Execute directly on pinned thread (no async I/O needed for event processing)
                        plugin_executor.execute_for_plugin_with(
                            *plugin_id,
                            JobClass::Ordered,
                            event_bytes,
                            {
                                let plugin_id = *plugin_id;
                                let client_id = *client_id;
                                let running_plugin = running_plugin.clone();
                                let event = event.clone();
                                let _s = shutdown_sender.clone();
                                let plugin_subs = subs.clone();
                                let scheduled_timer_generation = scheduled_timer_generation;
                                move |senders,
                                      _plugin_map,
                                      _connected_clients,
                                      _plugin_cache,
                                      _engine| {
                                    let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                                    if let Some(generation) = hover_generation {
                                        let superseded = latest_hovers
                                            .lock()
                                            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                                            .unwrap()
                                            .is_superseded(plugin_id, client_id, generation);
                                        if superseded {
                                            return;
                                        }
                                    }
                                    let mut running_plugin = running_plugin.lock().unwrap();
                                    if !running_plugin.is_active() {
                                        return;
                                    }
                                    let mut plugin_render_assets = vec![];
                                    match apply_event_to_plugin(
                                        plugin_id,
                                        client_id,
                                        &mut running_plugin,
                                        &event,
                                        &mut plugin_render_assets,
                                        senders.clone(),
                                        &plugin_subs,
                                        scheduled_timer_generation,
                                    ) {
                                        Ok(()) => {
                                            let _ = senders.send_to_screen(
                                                ScreenInstruction::PluginBytes(
                                                    plugin_render_assets,
                                                ),
                                            );
                                        },
                                        Err(e) => {
                                            log::error!("{:?}", e);

                                            // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
                                            let stringified_error =
                                                format!("{:?}", e).replace("\n", "\n\r");

                                            running_plugin.mark_quarantined();
                                            handle_plugin_crash(
                                                plugin_id,
                                                running_plugin.store.data().generation,
                                                stringified_error,
                                                senders.clone(),
                                            );
                                        },
                                    }
                                }
                            },
                        );
                    }
                }
            }
        }

        // loop once more to update the cached events for the pending plugins (probably currently
        // being loaded, we'll send them these events when they load)
        let quarantined_plugins = self.quarantine.ids().clone();
        for (pid, cid, event) in updates.drain(..) {
            let timer_generation = if matches!(event, Event::Timer(_)) {
                scheduled_timer_generation
            } else {
                None
            };
            let targets: Vec<PluginId> = self
                .cached_events_for_pending_plugins
                .plugin_ids()
                .into_iter()
                .filter(|plugin_id| {
                    !quarantined_plugins.contains(plugin_id)
                        && (pid.is_none() || pid.as_ref() == Some(plugin_id))
                })
                .collect();
            for plugin_id in targets {
                if let Err(rejection) = self.cached_events_for_pending_plugins.try_push(
                    plugin_id,
                    EventOrPipeMessage::Event(event.clone(), cid, timer_generation),
                    false,
                ) {
                    log::warn!(
                        "Dropping event cached for loading plugin {}: {:?}",
                        plugin_id,
                        rejection
                    );
                }
            }
        }
        Ok(())
    }
    pub fn get_plugin_cwd(&self, plugin_id: PluginId, client_id: ClientId) -> Option<PathBuf> {
        if let Some(instance_id) = self.shared_instance_of(plugin_id) {
            return self
                .plugin_map
                .lock()
                .unwrap()
                .shared_running_plugin(instance_id)
                .map(|(running_plugin, _)| {
                    running_plugin
                        .lock()
                        .unwrap()
                        .store
                        .data()
                        .plugin_cwd
                        .clone()
                });
        }
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins()
            .iter()
            .find_map(|(p_id, c_id, running_plugin)| {
                if p_id == &plugin_id && c_id == &client_id {
                    let plugin_cwd = running_plugin
                        .lock()
                        // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                        .unwrap()
                        .store
                        .data()
                        .plugin_cwd
                        .clone();
                    Some(plugin_cwd)
                } else {
                    None
                }
            })
    }
    pub fn change_plugin_host_dir(
        &mut self,
        new_host_dir: PathBuf,
        plugin_id_to_update: PluginId,
        client_id_to_update: ClientId,
    ) -> Result<()> {
        let host_dir_bytes = new_host_dir.as_os_str().len();
        if let Some(instance_id) = self.shared_instance_of(plugin_id_to_update) {
            let generation = self
                .shared_instances
                .get(&instance_id)
                .map(|info| info.loading_context.generation)
                .unwrap_or(0);
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Replaceable(replaceable_host_dir_key(client_id_to_update, generation)),
                host_dir_bytes,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let running_plugin = plugin_map
                        .lock()
                        // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                        .unwrap()
                        .shared_running_plugin(instance_id)
                        .map(|(running_plugin, _)| running_plugin);
                    let Some(running_plugin) = running_plugin else {
                        return;
                    };
                    let mut running_plugin = running_plugin.lock().unwrap();
                    if !running_plugin.is_active() {
                        return;
                    }
                    let event =
                        match change_host_dir_of_running_plugin(&mut running_plugin, &new_host_dir)
                        {
                            Ok(()) => Event::HostFolderChanged(new_host_dir.clone()),
                            Err(e) => {
                                log::error!("Failed to change host folder: {}", e);
                                Event::FailedToChangeHostFolder(Some(e.to_string()))
                            },
                        };
                    let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                        Some(instance_id),
                        Some(client_id_to_update),
                        event,
                    )]));
                },
            );
            return Ok(());
        }
        let plugins_to_change: Vec<(
            PluginId,
            ClientId,
            Arc<Mutex<RunningPlugin>>,
            Arc<Mutex<Subscriptions>>,
        )> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins_and_subscriptions()
            .iter()
            .cloned()
            .collect();

        // Execute directly on pinned thread (no async I/O needed for directory check/change)
        self.plugin_executor.execute_for_plugin_with(
            plugin_id_to_update,
            JobClass::Replaceable(replaceable_host_dir_key(client_id_to_update, 0)),
            host_dir_bytes,
            {
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    match new_host_dir.try_exists() {
                        Ok(false) => {
                            log::error!(
                                "Failed to change folder to {},: folder does not exist",
                                new_host_dir.display()
                            );
                            let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                Some(plugin_id_to_update),
                                Some(client_id_to_update),
                                Event::FailedToChangeHostFolder(Some(format!(
                                    "Folder {} does not exist",
                                    new_host_dir.display()
                                ))),
                            )]));
                            return;
                        },
                        Err(e) => {
                            log::error!(
                                "Failed to change folder to {},: {}",
                                new_host_dir.display(),
                                e
                            );
                            let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                Some(plugin_id_to_update),
                                Some(client_id_to_update),
                                Event::FailedToChangeHostFolder(Some(e.to_string())),
                            )]));
                            return;
                        },
                        _ => {},
                    }
                    for (plugin_id, client_id, running_plugin, _subscriptions) in &plugins_to_change
                    {
                        if plugin_id == &plugin_id_to_update && client_id == &client_id_to_update {
                            let mut running_plugin = running_plugin.lock().unwrap();
                            if !running_plugin.is_active() {
                                continue;
                            }
                            let plugin_env = running_plugin.store.data_mut();
                            let stdin_pipe = plugin_env.stdin_pipe.clone();
                            let stdout_pipe = plugin_env.stdout_pipe.clone();
                            let wasi_ctx = PluginLoader::create_wasi_ctx(
                                &new_host_dir,
                                &plugin_env.plugin_own_data_dir,
                                &plugin_env.plugin_own_cache_dir,
                                &ZELLIJ_TMP_DIR,
                                &plugin_env.plugin.location.to_string(),
                                plugin_env.plugin_id,
                                plugin_env.plugin.is_builtin(),
                                stdin_pipe.clone(),
                                stdout_pipe.clone(),
                            );
                            match wasi_ctx {
                                Ok(wasi_ctx) => {
                                    drop(std::mem::replace(&mut plugin_env.wasi_ctx, wasi_ctx));
                                    plugin_env.plugin_cwd = new_host_dir.clone();
                                    drop(running_plugin);
                                    plugin_map
                                        .lock()
                                        // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                                        .unwrap()
                                        .set_cwd(*plugin_id, new_host_dir.clone());

                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            Some(*plugin_id),
                                            Some(*client_id),
                                            Event::HostFolderChanged(new_host_dir.clone()),
                                        )]));
                                },
                                Err(e) => {
                                    let _ =
                                        senders.send_to_plugin(PluginInstruction::Update(vec![(
                                            Some(*plugin_id),
                                            Some(*client_id),
                                            Event::FailedToChangeHostFolder(Some(e.to_string())),
                                        )]));
                                    log::error!("Failed to create wasi ctx: {}", e);
                                },
                            }
                        }
                    }
                }
            },
        );
        Ok(())
    }
    pub fn pipe_messages(
        &mut self,
        messages: Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)>,
        shutdown_sender: Sender<()>,
        mut notification_end: Option<NotificationEnd>,
    ) -> Result<()> {
        let messages =
            self.dispatch_shared_pipes(messages, shutdown_sender.clone(), &mut notification_end);
        let plugins_to_update: Vec<(
            PluginId,
            ClientId,
            Arc<Mutex<RunningPlugin>>,
            Arc<Mutex<Subscriptions>>,
        )> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins_and_subscriptions()
            .iter()
            .cloned()
            .filter(|(plugin_id, _client_id, _running_plugin, _subscriptions)| {
                !self.cached_events_for_pending_plugins.contains(plugin_id)
                    && !self.quarantine.contains(plugin_id)
            })
            .collect();

        // Execute each pipe message on its respective plugin's pinned thread
        let plugin_executor = self.plugin_executor.clone();
        for (message_pid, message_cid, pipe_message) in messages.clone().into_iter() {
            for (plugin_id, client_id, running_plugin, _subscriptions) in &plugins_to_update {
                if Self::message_is_directed_at_plugin(
                    message_pid,
                    message_cid,
                    plugin_id,
                    client_id,
                ) {
                    let pipe_bytes = pipe_message_payload_bytes(&pipe_message);
                    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                        self.pending_pipes
                            .mark_being_processed(pipe_id, plugin_id, client_id);
                    }
                    // Execute directly on pinned thread (no async I/O needed for pipe message processing)
                    if let Err(e) = plugin_executor.try_execute_for_plugin(
                        *plugin_id,
                        JobClass::Ordered,
                        pipe_bytes,
                        {
                            let running_plugin = running_plugin.clone();
                            let pipe_message = pipe_message.clone();
                            let plugin_id = *plugin_id;
                            let client_id = *client_id;
                            let _s = shutdown_sender.clone();
                            let notification_end = notification_end.take();
                            move |senders,
                                  _plugin_map,
                                  _connected_clients,
                                  _plugin_cache,
                                  _engine| {
                                let mut running_plugin = running_plugin.lock().unwrap();
                                if !running_plugin.is_active() {
                                    return;
                                }
                                let mut plugin_render_assets = vec![];
                                let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                                match apply_pipe_message_to_plugin(
                                    plugin_id,
                                    client_id,
                                    &mut running_plugin,
                                    &pipe_message,
                                    &mut plugin_render_assets,
                                    &senders,
                                ) {
                                    Ok(()) => {
                                        let _ = senders.send_to_screen(
                                            ScreenInstruction::PluginBytes(plugin_render_assets),
                                        );
                                    },
                                    Err(e) => {
                                        log::error!("{:?}", e);

                                        // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-rust-using-windows
                                        let stringified_error =
                                            format!("{:?}", e).replace("\n", "\n\r");

                                        running_plugin.mark_quarantined();
                                        handle_plugin_crash(
                                            plugin_id,
                                            running_plugin.store.data().generation,
                                            stringified_error,
                                            senders.clone(),
                                        );
                                    },
                                }
                                drop(notification_end);
                            }
                        },
                    ) {
                        log::warn!("Plugin executor rejected a pipe for {}: {}", plugin_id, e);
                        self.release_cli_pipe_with_error(&pipe_message);
                    }
                }
            }
            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .iter()
                .copied()
                .collect();
            let cache_targets = self.cached_events_for_pending_plugins.plugin_ids();
            for plugin_id in cache_targets {
                if message_pid.is_none() || message_pid.as_ref() == Some(&plugin_id) {
                    let accepted = self
                        .cached_events_for_pending_plugins
                        .try_push(
                            plugin_id,
                            EventOrPipeMessage::PipeMessage(pipe_message.clone(), message_cid),
                            false,
                        )
                        .map_err(|rejection| {
                            log::warn!(
                                "Dropping first pipe message cached for loading plugin {}: {:?}",
                                plugin_id,
                                rejection
                            );
                            rejection
                        })
                        .is_ok();
                    if !accepted {
                        self.release_cli_pipe_with_error(&pipe_message);
                        continue;
                    }
                    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                        for client_id in &all_connected_clients {
                            if Self::message_is_directed_at_plugin(
                                message_pid,
                                message_cid,
                                &plugin_id,
                                client_id,
                            ) {
                                self.pending_pipes
                                    .mark_being_processed(pipe_id, &plugin_id, client_id);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
    pub fn apply_cached_events(
        &mut self,
        plugin_ids: Vec<PluginId>,
        done_receiving_permissions: bool,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let mut applied_plugin_paths = HashSet::new();
        for plugin_id in plugin_ids {
            if !done_receiving_permissions {
                // the loader reports that it is done with the instances of all clients
                self.plugins_with_unfinished_load.remove(&plugin_id);
            } else if self.plugins_with_unfinished_load.contains(&plugin_id) {
                // An answer that arrives while the loader is still publishing the instances of
                // the other clients must not drain the cached events: the instances published
                // later would never see them. The loader applies them when it is done.
                self.plugin_ids_waiting_for_permission_request
                    .remove(&plugin_id);
                continue;
            }
            // A plugin that was unloaded while it was loading is only waiting for this call to
            // be unloaded again; its pending permission request must not hold that back.
            if !done_receiving_permissions
                && self
                    .plugin_ids_waiting_for_permission_request
                    .contains(&plugin_id)
                && !self.plugins_to_unload_once_loaded.contains(&plugin_id)
            {
                continue;
            }
            self.plugin_ids_waiting_for_permission_request
                .remove(&plugin_id);
            self.apply_cached_events_and_resizes_for_plugin(plugin_id, shutdown_sender.clone())?;
            if let Some(run_plugin) = self.run_plugin_of_loading_plugin_id(plugin_id) {
                applied_plugin_paths.insert(run_plugin.clone());
            }
            self.loading_plugins
                .retain(|(p_id, _run_plugin)| p_id != &plugin_id);
            self.clear_plugin_map_cache();
            if self.plugins_to_unload_once_loaded.remove(&plugin_id) {
                self.unload_plugin(plugin_id).non_fatal();
            }
        }
        for run_plugin in applied_plugin_paths.drain() {
            if self.pending_plugin_reloads.remove(&run_plugin) {
                let _ = self.reload_plugin(&run_plugin);
            }
        }
        Ok(())
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.latest_hovers.lock().unwrap().evict_client(client_id);
        self.connected_clients
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .retain(|c| c != &client_id);
        self.keybinds.remove(&client_id);
        self.base_modes.remove(&client_id);
        self.remove_client_from_shared_instances(client_id);

        // Remove client from cached pane render report
        if let Some(ref mut prev_report) = self.previous_pane_render_report {
            prev_report.all_pane_contents.remove(&client_id);
        }

        let instances_to_clean_up = self
            .plugin_map
            .lock()
            .unwrap()
            .remove_plugin_clients(client_id);
        self.clean_up_plugin_instances(instances_to_clean_up);

        for messages in self.cached_worker_messages.values_mut() {
            messages.retain(|(c_id, _, _, _)| c_id != &client_id);
        }
        self.cached_worker_messages
            .retain(|_plugin_id, messages| !messages.is_empty());
    }

    pub fn update_plugin_tab_indices(&mut self, tab_indices: Vec<(PluginId, usize)>) {
        let mut plugin_map = self.plugin_map.lock().unwrap();
        for (plugin_id, tab_index) in tab_indices {
            plugin_map.set_tab_index(plugin_id, tab_index);
        }
    }

    fn clean_up_plugin_instances(&mut self, instances: RemovedPluginAssets) {
        if instances.is_empty() {
            return;
        }
        for ((plugin_id, client_id), (running_plugin, subscriptions, workers)) in instances {
            // Retire before async host work (timers) can deliver into a
            // replacement instance that reused (plugin_id, client_id).
            // The handle lives outside the execution mutex.
            revoke_client_loads(plugin_id, client_id);
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(
                    plugin_id, client_id,
                ));
            let _ = self.senders.send_to_screen(
                ScreenInstruction::UpdateBackgroundPluginSubscriptions(
                    plugin_id,
                    client_id,
                    HashSet::new(),
                ),
            );

            for (_worker_name, worker_sender) in workers {
                let _ = try_send_to_worker(&worker_sender, MessageToWorker::Exit);
            }

            // A full lifecycle queue must not drop directory cleanup.
            self.plugin_executor.submit_lifecycle_or_run(
                plugin_id,
                move |_senders, _plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let mut data_dirs = take_client_data_dirs(plugin_id, client_id);
                    if data_dirs.is_empty() {
                        if let Some(rp) = try_lock_running(&running_plugin) {
                            data_dirs.push(rp.store.data().plugin_own_data_dir.clone());
                        }
                    }
                    for cache_dir in data_dirs {
                        if let Err(e) = std::fs::remove_dir_all(&cache_dir) {
                            log::error!("Failed to remove data dir for plugin instance: {:?}", e);
                        }
                    }
                    drop(running_plugin);
                    drop(subscriptions);
                },
            );

            let pipes_to_release = self
                .pending_pipes
                .unload_plugin_client(&plugin_id, &client_id);
            self.send_pipe_releases(pipes_to_release);
            let prompt_actions = self
                .prompt_requests
                .caller_instance_gone(plugin_id, client_id);
            self.apply_prompt_actions(prompt_actions);
        }
        self.cached_plugin_map.clear();
    }

    fn get_changed_panes_per_client(
        &self,
        new_contents: &HashMap<ClientId, HashMap<zellij_utils::data::PaneId, PaneContents>>,
        previous_contents: Option<
            &HashMap<ClientId, HashMap<zellij_utils::data::PaneId, PaneContents>>,
        >,
    ) -> HashMap<ClientId, HashMap<zellij_utils::data::PaneId, PaneContents>> {
        let mut result: HashMap<ClientId, HashMap<zellij_utils::data::PaneId, PaneContents>> =
            HashMap::new();

        // First report - return everything grouped by client
        let Some(prev_contents) = previous_contents else {
            for (client_id, panes) in new_contents {
                result.insert(*client_id, panes.clone());
            }
            return result;
        };

        // Compare each client's panes
        for (client_id, new_panes) in new_contents {
            let mut client_panes: HashMap<zellij_utils::data::PaneId, PaneContents> =
                HashMap::new();
            for (pane_id, new_pane_contents) in new_panes {
                let has_changed = prev_contents
                    .get(client_id)
                    .and_then(|prev_panes| prev_panes.get(pane_id))
                    .map(|prev_pane_contents| {
                        prev_pane_contents.viewport != new_pane_contents.viewport
                            || prev_pane_contents.selected_text != new_pane_contents.selected_text
                            || prev_pane_contents.cursor != new_pane_contents.cursor
                    })
                    .unwrap_or(true);
                if has_changed {
                    client_panes.insert(*pane_id, new_pane_contents.clone());
                }
            }
            if !client_panes.is_empty() {
                result.insert(*client_id, client_panes);
            }
        }
        result
    }

    pub fn handle_pane_render_report(
        &mut self,
        pane_render_report: PaneRenderReport,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let changed_panes_per_client = self.get_changed_panes_per_client(
            &pane_render_report.all_pane_contents,
            self.previous_pane_render_report
                .as_ref()
                .map(|r| &r.all_pane_contents),
        );
        for (client_id, client_panes) in changed_panes_per_client {
            let updates = vec![(None, Some(client_id), Event::PaneRenderReport(client_panes))];
            self.update_plugins(updates, shutdown_sender.clone())?;
        }

        self.previous_pane_render_report = Some(pane_render_report);
        Ok(())
    }

    pub fn plugin_is_background(&self, plugin_id: PluginId, client_id: ClientId) -> bool {
        if self.shared_instances.contains_key(&plugin_id) {
            return self
                .shared_instances
                .get(&plugin_id)
                .map(|info| !info.background_slots.is_empty())
                .unwrap_or(false);
        }
        let plugin_map = self.plugin_map.lock().unwrap();
        plugin_map.contains(plugin_id, client_id)
            && plugin_map
                .metadata(plugin_id)
                .map(|metadata| metadata.is_background)
                .unwrap_or(false)
    }

    pub fn notify_screen_of_background_plugin_subscriptions(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
        events: HashSet<EventType>,
    ) {
        if self.shared_instances.contains_key(&plugin_id) {
            self.notify_shared_background_subscriptions(plugin_id);
            return;
        }
        if self.plugin_is_background(plugin_id, client_id) {
            let _ = self.senders.send_to_screen(
                ScreenInstruction::UpdateBackgroundPluginSubscriptions(
                    plugin_id, client_id, events,
                ),
            );
        }
    }

    pub fn send_initial_keybinds_to_plugin(&self, plugin_id: PluginId, client_id: ClientId) {
        if self.shared_instances.contains_key(&plugin_id) {
            let clients: Vec<ClientId> = self.connected_clients.lock().unwrap().clone();
            for client_id in clients {
                self.send_keybinds_payload_to_plugin(
                    plugin_id,
                    client_id,
                    self.keybinds_of_client(client_id).as_ref().clone(),
                );
            }
            return;
        }
        let keybinds = {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            plugin_map
                .running_plugins_and_subscriptions()
                .iter()
                .find(|(pid, cid, _, _)| *pid == plugin_id && *cid == client_id)
                .map(|(_, _, rp, _)| rp.lock().unwrap().store.data().keybinds.as_ref().clone())
        };
        if let Some(keybinds) = keybinds {
            self.send_keybinds_payload_to_plugin(plugin_id, client_id, keybinds);
        }
    }

    pub fn send_host_terminal_theme_mode_to_plugin(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
    ) {
        let Some(mode) = self.last_host_terminal_theme_mode else {
            return;
        };
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
            Some(plugin_id),
            Some(client_id),
            Event::HostTerminalThemeChanged(mode),
        )]));
    }

    pub fn send_global_attention_to_plugin(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let Some(global) = self.last_global_attention.clone() else {
            return Ok(());
        };
        self.send_initial_state_to_plugin(
            plugin_id,
            client_id,
            Event::GlobalAttentionUpdate(global),
            shutdown_sender,
        )
    }

    pub fn send_session_history_to_plugin(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let Some(info) = self.session_history_infos.get(&client_id).cloned() else {
            return Ok(());
        };
        self.send_initial_state_to_plugin(
            plugin_id,
            client_id,
            Event::SessionHistoryUpdate(info),
            shutdown_sender,
        )
    }

    fn send_initial_state_to_plugin(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        event: Event,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        if self.cached_events_for_pending_plugins.contains(&plugin_id) {
            if let Err(rejection) = self.cached_events_for_pending_plugins.try_push(
                plugin_id,
                EventOrPipeMessage::Event(event, Some(client_id), None),
                true,
            ) {
                log::warn!(
                    "Dropping initial state for loading plugin {}: {:?}",
                    plugin_id,
                    rejection
                );
            }
            return Ok(());
        }
        self.update_plugins(
            vec![(Some(plugin_id), Some(client_id), event)],
            shutdown_sender,
        )
    }

    fn send_keybinds_payload_to_plugin(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
        keybinds: KeybindsVec,
    ) {
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
            Some(plugin_id),
            Some(client_id),
            Event::InitialKeybinds(keybinds),
        )]));
    }

    pub fn cleanup(&mut self) {
        self.loading_plugins.clear();
        self.plugins_to_unload_once_loaded.clear();

        let plugin_ids = self.plugin_map.lock().unwrap().plugin_ids();
        for plugin_id in &plugin_ids {
            drop(self.unload_plugin(*plugin_id));
        }
        for (_, (_, watcher)) in self.watchers.drain() {
            watcher.stop_nonblocking();
        }
    }
    pub fn run_plugin_of_loading_plugin_id(&self, plugin_id: PluginId) -> Option<&RunPlugin> {
        self.loading_plugins
            .iter()
            .find(|(p_id, _run_plugin)| p_id == &plugin_id)
            .map(|(_p_id, run_plugin)| run_plugin)
    }
    pub fn run_plugin_of_plugin_id(&self, plugin_id: PluginId) -> Option<RunPlugin> {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .run_plugin_of_plugin_id(plugin_id)
    }

    pub fn reconfigure(
        &mut self,
        client_id: ClientId,
        keybinds: Option<SharedKeybinds>,
        default_mode: Option<InputMode>,
        default_shell: Option<TerminalAction>,
        layout_dir: Option<PathBuf>,
    ) -> Result<()> {
        let plugins_to_reconfigure: Vec<(PluginId, Arc<Mutex<RunningPlugin>>)> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins()
            .iter()
            .cloned()
            .filter_map(|(plugin_id, c_id, running_plugin)| {
                if c_id == client_id {
                    Some((plugin_id, running_plugin.clone()))
                } else {
                    None
                }
            })
            .collect();
        if let Some(default_mode) = default_mode.as_ref() {
            self.base_modes.insert(client_id, *default_mode);
        }
        if let Some(keybinds) = keybinds.as_ref() {
            self.keybinds.insert(client_id, keybinds.clone());
        }
        self.default_shell = default_shell.clone();
        self.layout_dir = layout_dir.clone();
        let reconfigure_bytes = keybinds
            .as_ref()
            .map(|keybinds| keybinds_vec_bytes(keybinds.as_ref()))
            .unwrap_or(0)
            .saturating_add(
                layout_dir
                    .as_ref()
                    .map(|path| path.as_os_str().len())
                    .unwrap_or(0),
            );
        let shared_instance_ids: Vec<PluginId> = self.shared_instances.keys().copied().collect();
        for instance_id in shared_instance_ids {
            let generation = self
                .shared_instances
                .get(&instance_id)
                .map(|info| info.loading_context.generation)
                .unwrap_or(0);
            let default_shell = default_shell.clone();
            let layout_dir = layout_dir.clone();
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Replaceable(replaceable_shared_settings_key(generation)),
                reconfigure_bytes,
                move |_senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    host_settings_job(plugin_map, instance_id, default_shell, layout_dir);
                },
            );
            if let Some(keybinds) = keybinds.as_ref() {
                if self.shared_instance_subscribes_to(instance_id, EventType::InitialKeybinds) {
                    self.send_keybinds_payload_to_plugin(
                        instance_id,
                        client_id,
                        keybinds.as_ref().clone(),
                    );
                }
            }
        }
        // Collect plugins subscribed to InitialKeybinds for post-reconfigure notification
        let plugins_subscribed_to_initial_keybinds: Vec<PluginId> = if keybinds.is_some() {
            self.plugin_map
                .lock()
                .unwrap()
                .running_plugins_and_subscriptions()
                .iter()
                .filter(|(_, cid, _, subs)| {
                    *cid == client_id && subs.lock().unwrap().contains(&EventType::InitialKeybinds)
                })
                .map(|(pid, _, _, _)| *pid)
                .collect()
        } else {
            vec![]
        };

        for (plugin_id, running_plugin) in plugins_to_reconfigure {
            self.plugin_executor.execute_for_plugin_with(
                plugin_id,
                JobClass::Replaceable(replaceable_plugin_settings_key(client_id, 0)),
                reconfigure_bytes,
                {
                    let running_plugin = running_plugin.clone();
                    let keybinds = keybinds.clone();
                    let default_shell = default_shell.clone();
                    let layout_dir = layout_dir.clone();
                    move |_senders, _plugin_map, _connected_clients, _plugin_cache, _engine| {
                        let mut running_plugin = running_plugin.lock().unwrap();
                        if let Some(keybinds) = keybinds {
                            running_plugin.update_keybinds(keybinds);
                        }
                        if let Some(default_mode) = default_mode {
                            running_plugin.update_default_mode(default_mode);
                        }
                        running_plugin.update_default_shell(default_shell);
                        running_plugin.update_layout_dir(layout_dir);
                    }
                },
            );
        }
        // Send InitialKeybinds to subscribed plugins after reconfiguration
        if let Some(keybinds) = keybinds.as_ref() {
            let keybinds_payload = keybinds.as_ref().clone();
            for plugin_id in plugins_subscribed_to_initial_keybinds {
                self.send_keybinds_payload_to_plugin(
                    plugin_id,
                    client_id,
                    keybinds_payload.clone(),
                );
            }
        }
        Ok(())
    }
    fn apply_cached_events_and_resizes_for_plugin(
        &mut self,
        plugin_id: PluginId,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        if self.quarantine.contains(&plugin_id) {
            return Ok(());
        }
        let err_context = || format!("Failed to apply cached events to plugin");
        let has_instance = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .all_plugin_ids()
            .iter()
            .any(|(instance_plugin_id, _)| *instance_plugin_id == plugin_id);
        if let Some(events_or_pipe_messages) =
            self.cached_events_for_pending_plugins.take(&plugin_id)
        {
            let cached_bytes = events_or_pipe_messages
                .entries
                .iter()
                .map(event_or_pipe_message_bytes)
                .fold(0usize, usize::saturating_add);
            let plugin_instances = self.plugin_map.lock().unwrap().all_plugin_ids();
            // A failed load leaves no instance to answer the pipes cached for it. Release them
            // instead of dropping them and leaving their callers blocked.
            if !has_instance {
                self.release_cached_cli_pipes_with_error(&events_or_pipe_messages.entries);
            }
            let mut replay_was_rejected = false;
            for (instance_plugin_id, client_id) in plugin_instances {
                if instance_plugin_id != plugin_id {
                    continue;
                }
                if let Some((running_plugin, subscriptions)) = self
                    .plugin_map
                    .lock()
                    .unwrap()
                    .get_running_plugin_and_subscriptions(plugin_id, client_id)
                {
                    let subs = subscriptions.lock().unwrap().clone();
                    if let Err(e) = self.plugin_executor.try_execute_for_plugin(
                        plugin_id,
                        JobClass::Lifecycle,
                        cached_bytes,
                        {
                            let running_plugin = running_plugin.clone();
                            let _s = shutdown_sender.clone();
                            let events_or_pipe_messages = events_or_pipe_messages.entries.clone();
                            move |senders,
                                  _plugin_map,
                                  _connected_clients,
                                  _plugin_cache,
                                  _engine| {
                                let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                                if !running_plugin.lock().unwrap().is_active() {
                                    return;
                                }
                                for event_or_pipe_message in events_or_pipe_messages {
                                    match event_or_pipe_message {
                                        EventOrPipeMessage::Event(
                                            event,
                                            target_client_id,
                                            scheduled_timer_generation,
                                        ) => {
                                            if target_client_id
                                                .map(|target| target != client_id)
                                                .unwrap_or(false)
                                            {
                                                continue;
                                            }
                                            match subscription_event_type(&event)
                                                .with_context(err_context)
                                            {
                                                Ok(event_type) => {
                                                    if !subs.contains(&event_type) {
                                                        continue;
                                                    }
                                                    let mut running_plugin =
                                                        running_plugin.lock().unwrap();
                                                    if !running_plugin.is_active() {
                                                        break;
                                                    }
                                                    let mut plugin_render_assets = vec![];
                                                    match apply_event_to_plugin(
                                                        plugin_id,
                                                        client_id,
                                                        &mut running_plugin,
                                                        &event,
                                                        &mut plugin_render_assets,
                                                        senders.clone(),
                                                        &subs,
                                                        scheduled_timer_generation,
                                                    ) {
                                                        Ok(()) => {
                                                            let _ = senders.send_to_screen(
                                                                ScreenInstruction::PluginBytes(
                                                                    plugin_render_assets,
                                                                ),
                                                            );
                                                        },
                                                        Err(e) => {
                                                            log::error!("{:?}", e);
                                                            running_plugin.mark_quarantined();
                                                            handle_plugin_crash(
                                                                plugin_id,
                                                                running_plugin
                                                                    .store
                                                                    .data()
                                                                    .generation,
                                                                format!("{:?}", e)
                                                                    .replace("\n", "\n\r"),
                                                                senders.clone(),
                                                            );
                                                            break;
                                                        },
                                                    }
                                                },
                                                Err(e) => {
                                                    log::error!("Failed to apply event: {:?}", e);
                                                },
                                            }
                                        },
                                        EventOrPipeMessage::PipeMessage(
                                            pipe_message,
                                            target_client_id,
                                        ) => {
                                            if target_client_id
                                                .map(|target| target != client_id)
                                                .unwrap_or(false)
                                            {
                                                continue;
                                            }
                                            let mut running_plugin = running_plugin.lock().unwrap();
                                            if !running_plugin.is_active() {
                                                break;
                                            }
                                            let mut plugin_render_assets = vec![];

                                            match apply_pipe_message_to_plugin(
                                                plugin_id,
                                                client_id,
                                                &mut running_plugin,
                                                &pipe_message,
                                                &mut plugin_render_assets,
                                                &senders,
                                            ) {
                                                Ok(()) => {
                                                    let _ = senders.send_to_screen(
                                                        ScreenInstruction::PluginBytes(
                                                            plugin_render_assets,
                                                        ),
                                                    );
                                                },
                                                Err(e) => {
                                                    log::error!("{:?}", e);
                                                    running_plugin.mark_quarantined();

                                                    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
                                                    let stringified_error =
                                                        format!("{:?}", e).replace("\n", "\n\r");

                                                    handle_plugin_crash(
                                                        plugin_id,
                                                        running_plugin.store.data().generation,
                                                        stringified_error,
                                                        senders.clone(),
                                                    );
                                                    break;
                                                },
                                            }
                                        },
                                    }
                                }
                            }
                        },
                    ) {
                        log::error!(
                            "Plugin executor rejected cached event replay for {}: {}",
                            plugin_id,
                            e
                        );
                        replay_was_rejected = true;
                    }
                }
            }
            if replay_was_rejected {
                self.release_cached_cli_pipes_with_error(&events_or_pipe_messages.entries);
            }
        }
        if let Some((rows, columns)) = self.cached_resizes_for_pending_plugins.remove(&plugin_id) {
            self.resize_plugin(plugin_id, columns, rows, shutdown_sender.clone())?;
        }
        if has_instance {
            self.apply_cached_worker_messages(plugin_id)?;
        } else {
            // A failed load has no worker to receive these; re-caching them
            // would leak one bounded cache per failed plugin id.
            self.cached_worker_messages.remove(&plugin_id);
        }
        Ok(())
    }
    pub fn apply_cached_worker_messages(&mut self, plugin_id: PluginId) -> Result<()> {
        if let Some(mut messages) = self.cached_worker_messages.remove(&plugin_id) {
            let mut worker_messages: HashMap<(ClientId, String), Vec<(String, String)>> =
                HashMap::new();
            for (client_id, worker_name, message, payload) in messages.drain(..) {
                worker_messages
                    .entry((client_id, worker_name))
                    .or_default()
                    .push((message, payload));
            }
            for ((client_id, worker_name), messages) in worker_messages.drain() {
                self.post_messages_to_plugin_worker(plugin_id, client_id, worker_name, messages)?;
            }
        }
        Ok(())
    }
    fn plugin_is_currently_being_loaded(&self, plugin_location: &RunPluginLocation) -> bool {
        self.loading_plugins
            .iter()
            .find(|(_plugin_id, run_plugin)| &run_plugin.location == plugin_location)
            .is_some()
    }
    fn plugin_id_of_loading_plugin(
        &self,
        plugin_location: &RunPluginLocation,
        plugin_configuration: &PluginUserConfiguration,
    ) -> Option<PluginId> {
        self.loading_plugins
            .iter()
            .find_map(|(plugin_id, run_plugin)| {
                if &run_plugin.location == plugin_location
                    && &run_plugin.configuration == plugin_configuration
                {
                    Some(*plugin_id)
                } else {
                    None
                }
            })
    }
    fn all_plugin_ids_for_plugin_location(
        &self,
        plugin_location: &RunPluginLocation,
        plugin_configuration: &PluginUserConfiguration,
    ) -> Result<Vec<PluginId>> {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .all_plugin_ids_for_plugin_location(plugin_location, plugin_configuration)
    }
    pub fn all_plugin_and_client_ids_for_plugin_location(
        &mut self,
        plugin_location: &RunPluginLocation,
        plugin_configuration: &PluginUserConfiguration,
    ) -> Vec<(PluginId, Option<ClientId>)> {
        if self.cached_plugin_map.is_empty() {
            self.cached_plugin_map = self.plugin_map.lock().unwrap().clone_plugin_assets();
        }
        match self
            .cached_plugin_map
            .get(plugin_location)
            .and_then(|m| m.get(plugin_configuration))
        {
            Some(plugin_and_client_ids) => plugin_and_client_ids
                .iter()
                .map(|(plugin_id, client_id)| (*plugin_id, Some(*client_id)))
                .collect(),
            None => vec![],
        }
    }
    pub fn all_plugin_ids(&self) -> Vec<(PluginId, ClientId)> {
        self.plugin_map.lock().unwrap().all_plugin_ids()
    }
    fn size_of_plugin_id(&self, plugin_id: PluginId) -> Option<(usize, usize)> {
        // (rows/columns)
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .metadata(plugin_id)
            .map(|metadata| (metadata.rows, metadata.columns))
    }
    fn cwd_of_plugin_id(&self, plugin_id: PluginId) -> Option<PathBuf> {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .metadata(plugin_id)
            .map(|metadata| metadata.cwd.clone())
    }
    fn plugin_config_of_plugin_id(&self, plugin_id: PluginId) -> Option<PluginConfig> {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .metadata(plugin_id)
            .map(|metadata| metadata.plugin_config.clone())
    }
    fn tab_index_of_plugin_id(&self, plugin_id: PluginId) -> Option<usize> {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .metadata(plugin_id)
            .and_then(|metadata| metadata.tab_index)
    }
    fn start_plugin_loading_indication(
        &self,
        plugin_ids: &[PluginId],
        loading_indication: &LoadingIndication,
    ) {
        for plugin_id in plugin_ids {
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::StartPluginLoadingIndication(
                    *plugin_id,
                    loading_indication.clone(),
                    None,
                ));
            let _ = self
                .senders
                .send_to_background_jobs(BackgroundJob::AnimatePluginLoading(*plugin_id));
        }
    }
    fn start_plugin_loading_indication_for_client(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
        loading_indication: &LoadingIndication,
    ) {
        let _ = self
            .senders
            .send_to_screen(ScreenInstruction::StartPluginLoadingIndication(
                plugin_id,
                loading_indication.clone(),
                Some(client_id),
            ));
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::AnimatePluginLoading(plugin_id));
    }
    pub fn post_messages_to_plugin_worker(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        worker_name: String,
        mut messages: Vec<(String, String)>,
    ) -> Result<()> {
        if self.quarantine.contains(&plugin_id) {
            return Ok(());
        }
        let worker = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .worker_sender(plugin_id, client_id, &worker_name);
        match worker {
            Some(worker) => {
                for (message, payload) in messages.drain(..) {
                    let _ = try_send_to_worker(&worker, MessageToWorker::Message(message, payload));
                }
            },
            None => {
                let running_plugin = {
                    let plugin_map = self.plugin_map.lock().unwrap();
                    plugin_map.get_running_plugin(plugin_id, Some(client_id))
                };
                if let Some(running_plugin) = running_plugin {
                    if !running_plugin.lock().unwrap().is_active() {
                        // The instance was retired or quarantined after this message was
                        // queued. Do not cache it for a replacement that reused the same key.
                        return Ok(());
                    }
                }
                log::warn!("Worker {worker_name} not found, caching messages");
                for (message, payload) in messages.drain(..) {
                    let cached = self.cached_worker_messages.entry(plugin_id).or_default();
                    let cached_bytes = cached
                        .iter()
                        .map(|(_, name, message, payload)| {
                            name.len()
                                .saturating_add(message.len())
                                .saturating_add(payload.len())
                        })
                        .fold(0usize, usize::saturating_add);
                    let new_bytes = worker_name
                        .len()
                        .saturating_add(message.len())
                        .saturating_add(payload.len());
                    if cached.len() >= MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN
                        || cached_bytes.saturating_add(new_bytes)
                            > MAX_CACHED_WORKER_MESSAGE_BYTES_PER_PLUGIN
                    {
                        log::warn!(
                            "Dropping worker message cached for plugin {}: pending cache is full",
                            plugin_id
                        );
                        continue;
                    }
                    cached.push((client_id, worker_name.clone(), message, payload));
                }
            },
        }
        Ok(())
    }
    pub fn start_fs_watcher_if_not_started(&mut self, plugin_id: PluginId, host_root: PathBuf) {
        if self
            .watchers
            .get(&plugin_id)
            .map(|(root, _)| root == &host_root)
            .unwrap_or(false)
        {
            return;
        }
        if let Some((_, old_watcher)) = self.watchers.remove(&plugin_id) {
            old_watcher.stop_nonblocking();
        }
        match watch_filesystem(self.senders.clone(), plugin_id, &host_root) {
            Ok(watcher) => {
                self.watchers.insert(plugin_id, (host_root, watcher));
            },
            Err(e) => {
                log::error!("Failed to watch filesystem: {:?}", e);
            },
        }
    }
    /// Whether an answer to a permission prompt still concerns the module that is running. The
    /// answer carries neither a digest nor a generation, so they are matched against what was
    /// recorded when the prompt was requested: an answer is stale when no prompt was requested,
    /// when the plugin was reloaded since, or when the running bytes are not the prompted ones.
    pub fn permission_prompt_is_current(
        &mut self,
        plugin_id: PluginId,
        client_id: Option<ClientId>,
    ) -> bool {
        let shared_instance = self.shared_instance_of(plugin_id);
        let prompt_key = shared_instance.unwrap_or(plugin_id);
        // TODO(plugin-hardening): when a prompt is stale its pane has already been dismissed by
        // the answer, so the reloaded plugin has to ask again to get a new one. The prompt of a
        // plugin that is reloaded while it is open should instead be closed at reload time
        // (`prepare_plugin_reload`), which needs a screen instruction to drop a pane's pending
        // request.
        let Some((prompt_generation, prompt_digest)) =
            self.permission_prompts.get(&prompt_key).cloned()
        else {
            return false;
        };
        if self.quarantine.is_superseded(prompt_key, prompt_generation) {
            return false;
        }
        let running_plugin = match shared_instance {
            Some(instance_id) => self
                .plugin_map
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .shared_running_plugin(instance_id)
                .map(|(running_plugin, _)| running_plugin),
            None => self
                .plugin_map
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .get_running_plugin(plugin_id, client_id),
        };
        let Some(running_plugin) = running_plugin else {
            return false;
        };
        let running_plugin = running_plugin.lock().unwrap();
        running_plugin.store.data().plugin_digest == prompt_digest
    }
    pub fn clear_permission_prompt(&mut self, plugin_id: PluginId) {
        let prompt_key = self.shared_instance_of(plugin_id).unwrap_or(plugin_id);
        self.permission_prompts.remove(&prompt_key);
    }
    pub fn cache_plugin_permissions(
        &mut self,
        plugin_id: PluginId,
        client_id: Option<ClientId>,
        permissions: Vec<PermissionType>,
        status: PermissionStatus,
        cache_path: Option<PathBuf>,
    ) -> Result<()> {
        let err_context = || format!("Failed to write plugin permission {plugin_id}");

        let shared_running_plugin = self.shared_instance_of(plugin_id).and_then(|instance_id| {
            self.plugin_map
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .shared_running_plugin(instance_id)
                .map(|(running_plugin, _)| running_plugin)
        });
        let running_plugin = match shared_running_plugin {
            Some(running_plugin) => running_plugin,
            None => self
                .plugin_map
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .get_running_plugin(plugin_id, client_id)
                .ok_or_else(|| anyhow!("Failed to get running plugin"))?,
        };

        let mut running_plugin = running_plugin.lock().unwrap();

        let mut runtime_permissions = running_plugin
            .store
            .data()
            .permissions
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .clone()
            .unwrap_or_default();
        if status == PermissionStatus::Granted {
            runtime_permissions.extend(permissions.iter().cloned());
        }
        running_plugin
            .store
            .data_mut()
            .set_permissions(runtime_permissions);

        if running_plugin.store.data().plugin.is_builtin() {
            return Ok(());
        }

        let permission_cache_key = format!(
            "{}#{}",
            running_plugin.store.data().plugin.location,
            running_plugin.store.data().plugin_digest
        );
        if status == PermissionStatus::Granted {
            // Fresh-read → apply delta → publish under a short lock. Do not hold
            // that lock across the user prompt that produced this grant.
            PermissionCache::grant_and_persist(cache_path, permission_cache_key, permissions)
                .with_context(err_context)?;
        }
        Ok(())
    }
    pub fn cache_plugin_events(&mut self, plugin_id: PluginId, generation: u64, digest: String) {
        // Remember which module instance the prompt that is about to be shown belongs to. Shared
        // instances are keyed by instance id, as that is what their requests carry.
        self.permission_prompts
            .insert(plugin_id, (generation, digest));
        if self.shared_instance_of(plugin_id).is_some() {
            return;
        }
        self.plugin_ids_waiting_for_permission_request
            .insert(plugin_id);
        self.cached_events_for_pending_plugins
            .insert_empty(plugin_id);
    }

    // gets all running plugins details matching this run_plugin, if none are running, loads one and
    // returns its details
    pub fn get_or_load_plugins(
        &mut self,
        run_plugin_or_alias: RunPluginOrAlias,
        size: Size,
        cwd: Option<PathBuf>,
        skip_cache: bool,
        should_float: bool,
        should_be_open_in_place: bool,
        pane_title: Option<String>,
        pane_id_to_replace: Option<PaneId>,
        cli_client_id: Option<ClientId>,
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
        should_focus: bool,
    ) -> Vec<(PluginId, Option<ClientId>)> {
        let run_plugin = run_plugin_or_alias
            .get_run_plugin()
            .map(|run_plugin| canonical_run_plugin(&run_plugin));
        if let Some(run_plugin) = run_plugin.as_ref() {
            if let Some(targets) = self.existing_shared_targets(run_plugin, skip_cache) {
                return targets;
            }
        }
        match run_plugin {
            Some(run_plugin) => {
                let all_plugin_ids = self.all_plugin_and_client_ids_for_plugin_location(
                    &run_plugin.location,
                    &run_plugin.configuration,
                );
                if all_plugin_ids.is_empty() {
                    if let Some(loading_plugin_id) = self.plugin_id_of_loading_plugin(
                        &run_plugin.location,
                        &run_plugin.configuration,
                    ) {
                        return vec![(loading_plugin_id, None)];
                    }
                    match self.load_plugin(
                        &Some(run_plugin),
                        None,
                        size,
                        cwd.clone(),
                        skip_cache,
                        cli_client_id,
                    ) {
                        Ok((plugin_id, client_id)) => {
                            let is_shared = self.shared_slot_owner.contains_key(&plugin_id);
                            let start_suppressed = false;
                            drop(self.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                Some(should_float),
                                should_be_open_in_place,
                                false, // close_replaced_pane
                                run_plugin_or_alias,
                                pane_title,
                                None,
                                plugin_id,
                                pane_id_to_replace,
                                cwd,
                                start_suppressed,
                                floating_pane_coordinates,
                                Some(should_focus),
                                Some(client_id),
                                None,
                                None,
                            )));
                            if is_shared {
                                vec![(plugin_id, None)]
                            } else {
                                vec![(plugin_id, Some(client_id))]
                            }
                        },
                        Err(e) => {
                            log::error!("Failed to load plugin: {e}");
                            if let Some(cli_client_id) = cli_client_id {
                                let _ = self.senders.send_to_server(ServerInstruction::LogError(
                                    vec![format!("Failed to log plugin: {e}")],
                                    cli_client_id,
                                    None,
                                ));
                            }
                            vec![]
                        },
                    }
                } else {
                    all_plugin_ids
                }
            },
            None => {
                log::error!("Plugin not found for alias");
                vec![]
            },
        }
    }
    pub fn clear_plugin_map_cache(&mut self) {
        self.cached_plugin_map.clear();
    }
    // returns the pipe names to unblock
    pub fn update_cli_pipe_state(
        &mut self,
        pipe_state_changes: Vec<PluginRenderAsset>,
    ) -> Vec<PipeRelease> {
        let mut pipes_to_release = vec![];
        for pipe_state_change in pipe_state_changes {
            let client_id = pipe_state_change.client_id;
            let plugin_id = pipe_state_change.plugin_id;
            for (cli_pipe_name, pipe_state_change) in pipe_state_change.cli_pipes {
                pipes_to_release.append(&mut self.pending_pipes.update_pipe_state_change(
                    &cli_pipe_name,
                    pipe_state_change,
                    &plugin_id,
                    &client_id,
                ));
            }
        }
        let mut seen = HashSet::new();
        pipes_to_release.retain(|release| seen.insert(release.clone()));
        pipes_to_release
    }
    pub fn set_cli_pipe_exit_code(&mut self, pipe_id: &str, exit_code: i32, plugin_id: PluginId) {
        self.pending_pipes
            .set_exit_code(pipe_id, exit_code, plugin_id);
    }
    pub fn cli_pipe_is_finished(&self, pipe_id: &str) -> bool {
        self.pending_pipes.is_finished(pipe_id)
    }
    pub fn cli_pipe_is_pending(&self, pipe_id: &str) -> bool {
        self.pending_pipes.is_pending(pipe_id)
    }
    fn send_pipe_releases(&self, releases: Vec<PipeRelease>) {
        for release in releases {
            let _ = self
                .senders
                .send_to_server(ServerInstruction::UnblockCliPipeInput(
                    release.pipe_id,
                    release.exit_code,
                ))
                .context("failed to unblock input pipe");
        }
    }
    fn release_cli_pipe_with_error(&mut self, pipe_message: &PipeMessage) {
        let PipeSource::Cli(pipe_id) = &pipe_message.source else {
            return;
        };
        if self.pending_pipes.is_finished(pipe_id) {
            return;
        }
        let release = self
            .pending_pipes
            .release_with_code(pipe_id, PIPE_PLUGIN_UNAVAILABLE_EXIT_CODE);
        self.send_pipe_releases(vec![release]);
    }
    fn release_cached_cli_pipes_with_error(&mut self, entries: &[EventOrPipeMessage]) {
        for entry in entries {
            if let EventOrPipeMessage::PipeMessage(pipe_message, _) = entry {
                self.release_cli_pipe_with_error(pipe_message);
            }
        }
    }
    pub fn route_pipe_popup_message(
        &mut self,
        pipe_id: &str,
        pipe_message: PipeMessage,
    ) -> PipePopupRoute {
        match self.pipe_popups.get_mut(pipe_id) {
            Some(PipePopup::Opening { queued, .. }) => {
                queued.push(pipe_message);
                PipePopupRoute::Queued
            },
            Some(PipePopup::Open {
                plugin_id,
                client_id,
                caller_args,
            }) => {
                let mut pipe_message = pipe_message;
                pipe_message.args.extend(caller_args.clone());
                PipePopupRoute::Deliver(*plugin_id, *client_id, pipe_message)
            },
            None => PipePopupRoute::New(pipe_message),
        }
    }
    pub fn start_pipe_popup(
        &mut self,
        pipe_id: &str,
        pipe_message: PipeMessage,
        cli_client_id: ClientId,
    ) {
        self.pipe_popups.insert(
            pipe_id.to_owned(),
            PipePopup::Opening {
                queued: vec![pipe_message],
                cli_client_id,
            },
        );
    }
    pub fn attach_pipe_popup(
        &mut self,
        pipe_id: &str,
        plugin_id: PluginId,
        client_id: ClientId,
        caller_args: BTreeMap<String, String>,
    ) -> Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)> {
        let queued = match self.pipe_popups.remove(pipe_id) {
            Some(PipePopup::Opening { queued, .. }) => queued,
            _ => vec![],
        };
        self.pipe_popups.insert(
            pipe_id.to_owned(),
            PipePopup::Open {
                plugin_id,
                client_id,
                caller_args: caller_args.clone(),
            },
        );
        self.pending_pipes
            .set_exit_code(pipe_id, PIPE_POPUP_CLOSED_EXIT_CODE, plugin_id);
        queued
            .into_iter()
            .map(|mut pipe_message| {
                pipe_message.args.extend(caller_args.clone());
                (Some(plugin_id), Some(client_id), pipe_message)
            })
            .collect()
    }
    pub fn fail_pipe_popup(&mut self, pipe_id: &str, error: String) {
        let cli_client_id = match self.pipe_popups.remove(pipe_id) {
            Some(PipePopup::Opening { cli_client_id, .. }) => Some(cli_client_id),
            _ => None,
        };
        if let Some(cli_client_id) = cli_client_id {
            let _ = self.senders.send_to_server(ServerInstruction::LogError(
                vec![error],
                cli_client_id,
                None,
            ));
        }
        let release = self
            .pending_pipes
            .release_with_code(pipe_id, PIPE_POPUP_ERROR_EXIT_CODE);
        self.send_pipe_releases(vec![release]);
    }
    pub fn start_prompt_popup(
        &mut self,
        caller: PromptCaller,
        pipe_message: PipeMessage,
        is_notice: bool,
    ) {
        self.prompt_requests.start(caller, pipe_message, is_notice);
    }
    pub fn attach_prompt_popup(
        &mut self,
        caller: PromptCaller,
        plugin_id: PluginId,
        client_id: ClientId,
        caller_args: BTreeMap<String, String>,
    ) -> Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)> {
        let actions = self
            .prompt_requests
            .attach(caller, plugin_id, client_id, caller_args);
        self.apply_prompt_actions(actions)
    }
    pub fn withdraw_prompt(&mut self, caller: PromptCaller) {
        let actions = self.prompt_requests.withdraw(caller);
        self.apply_prompt_actions(actions);
    }
    pub fn fail_prompt_popup(&mut self, caller: PromptCaller, error: String) {
        let actions = self.prompt_requests.fail(caller, error);
        self.apply_prompt_actions(actions);
    }
    pub fn prompt_replied(
        &mut self,
        prompt_plugin_id: PluginId,
        request_id: u64,
        result: PromptResult,
    ) {
        let actions = self
            .prompt_requests
            .reply(prompt_plugin_id, request_id, result);
        self.apply_prompt_actions(actions);
    }
    fn apply_prompt_actions(
        &mut self,
        actions: Vec<PromptAction>,
    ) -> Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)> {
        let mut pipe_messages = vec![];
        for action in actions {
            match action {
                PromptAction::Deliver(caller, result) if caller.is_close_dialogue() => {
                    let _ = self
                        .senders
                        .send_to_screen(ScreenInstruction::CloseDialogueAnswer {
                            client_id: caller.client_id,
                            request_id: caller.request_id,
                            result,
                        });
                },
                PromptAction::Deliver(caller, result) => {
                    let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![
                        caller.result_event(result)
                    ]));
                },
                PromptAction::Send(plugin_id, client_id, pipe_message) => {
                    pipe_messages.push((Some(plugin_id), Some(client_id), pipe_message));
                },
                PromptAction::Close(plugin_id) => {
                    let _ = self.senders.send_to_screen(ScreenInstruction::ClosePane(
                        PaneId::Plugin(plugin_id),
                        None,
                        None,
                        None,
                        ClosePaneSource::External,
                        None,
                    ));
                    let _ = self
                        .senders
                        .send_to_plugin(PluginInstruction::Unload(plugin_id));
                },
            }
        }
        pipe_messages
    }
    pub fn popup_became_pane(&mut self, plugin_id: PluginId) {
        self.forget_pipe_popups_of_plugin(plugin_id);
        let running_plugins: Vec<Arc<Mutex<RunningPlugin>>> = self
            .plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .running_plugins()
            .into_iter()
            .filter(|(id, _, _)| *id == plugin_id)
            .map(|(_, _, running_plugin)| running_plugin)
            .collect();
        for running_plugin in running_plugins {
            running_plugin
                .lock()
                // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                .unwrap()
                .store
                .data_mut()
                .pending_popup_size = None;
        }
    }
    fn forget_pipe_popups_of_plugin(&mut self, plugin_id: PluginId) {
        self.pipe_popups.retain(|_, pipe_popup| match pipe_popup {
            PipePopup::Open {
                plugin_id: popup_plugin_id,
                ..
            } => *popup_plugin_id != plugin_id,
            PipePopup::Opening { .. } => true,
        });
    }
    fn message_is_directed_at_plugin(
        message_pid: Option<PluginId>,
        message_cid: Option<ClientId>,
        plugin_id: &PluginId,
        client_id: &ClientId,
    ) -> bool {
        message_pid.is_none() && message_cid.is_none()
            || (message_pid.is_none() && message_cid == Some(*client_id))
            || (message_cid.is_none() && message_pid == Some(*plugin_id))
            || (message_cid == Some(*client_id) && message_pid == Some(*plugin_id))
    }
    pub fn client_is_connected(&self, client_id: &ClientId) -> bool {
        self.connected_clients.lock().unwrap().contains(client_id)
    }
    pub fn get_first_client_id(&self) -> Option<ClientId> {
        self.connected_clients
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .iter()
            .next()
            .copied()
    }
    pub fn update_available_layouts(
        &mut self,
        layouts: Vec<LayoutInfo>,
        errors: Vec<LayoutWithError>,
    ) {
        // Diff with existing layouts
        if self.available_layouts != layouts || self.available_layout_errors != errors {
            // Update the stored layouts
            self.available_layouts = layouts.clone();
            self.available_layout_errors = errors.clone();

            // Notify all plugins of the change
            let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
                None, // Broadcast to all plugins
                None, // Broadcast to all clients
                Event::AvailableLayoutInfo(layouts, errors),
            )]));
        }
    }
    pub fn update_available_keybind_presets(
        &mut self,
        presets: Vec<KeybindPresetInfo>,
        errors: Vec<KeybindPresetWithError>,
    ) {
        if self.available_keybind_presets != presets
            || self.available_keybind_preset_errors != errors
        {
            self.available_keybind_presets = presets.clone();
            self.available_keybind_preset_errors = errors.clone();
            let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
                None,
                None,
                Event::AvailableKeybindPresets(presets, errors),
            )]));
        }
    }
    pub fn register_settings_entry(
        &mut self,
        plugin_id: PluginId,
        plugin_identity: PluginIdentity,
        title: &str,
    ) {
        if self
            .settings_entries
            .register(plugin_id, plugin_identity, title)
        {
            self.broadcast_settings_entries();
        }
    }
    pub fn unregister_settings_entry(&mut self, plugin_id: PluginId) {
        self.settings_entries_changed(|entries| entries.unregister(plugin_id));
    }
    fn settings_entries_changed(&mut self, change: impl FnOnce(&mut SettingsEntries) -> bool) {
        if change(&mut self.settings_entries) {
            self.broadcast_settings_entries();
        }
    }
    fn broadcast_settings_entries(&self) {
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
            None,
            None,
            Event::PluginSettingsEntries(self.settings_entries.entries()),
        )]));
    }
    pub fn state_update_for_plugin(&self, plugin_id: PluginId) {
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![
            (
                Some(plugin_id),
                None,
                Event::AvailableLayoutInfo(
                    self.available_layouts.clone(),
                    self.available_layout_errors.clone(),
                ),
            ),
            (
                Some(plugin_id),
                None,
                Event::AvailableKeybindPresets(
                    self.available_keybind_presets.clone(),
                    self.available_keybind_preset_errors.clone(),
                ),
            ),
            (
                Some(plugin_id),
                None,
                Event::PluginSettingsEntries(self.settings_entries.entries()),
            ),
        ]));
    }
    pub fn shared_instance_ids(&self) -> Vec<PluginId> {
        self.shared_instances.keys().copied().collect()
    }
    fn shared_instance_of(&self, plugin_id: PluginId) -> Option<PluginId> {
        if let Some(instance_id) = self.shared_slot_owner.get(&plugin_id) {
            Some(*instance_id)
        } else if self.shared_instances.contains_key(&plugin_id) {
            Some(plugin_id)
        } else {
            None
        }
    }
    fn keybinds_of_client(&self, client_id: ClientId) -> &SharedKeybinds {
        self.keybinds
            .get(&client_id)
            .unwrap_or(&self.default_keybinds)
    }
    fn shared_instance_subscribes_to(&self, instance_id: PluginId, event_type: EventType) -> bool {
        self.plugin_map
            .lock()
            // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
            .unwrap()
            .shared_running_plugin(instance_id)
            .map(|(_, subscriptions)| subscriptions.lock().unwrap().contains(&event_type))
            .unwrap_or(false)
    }
    fn plugin_wasm_bytes(&self, plugin_config: &PluginConfig) -> Option<Vec<u8>> {
        if matches!(plugin_config.location, RunPluginLocation::Remote(_)) {
            return None;
        }
        plugin_config.resolve_wasm_bytes(&self.plugin_dir).ok()
    }
    fn module_bytes_are_shared(wasm_bytes: Option<&[u8]>) -> bool {
        wasm_bytes
            .map(|bytes| wasm_module_exports_function(bytes, SHARED_MARKER_EXPORT))
            .unwrap_or(false)
    }
    fn shared_key_of(run_plugin: &RunPlugin) -> SharedKey {
        (
            run_plugin.location.clone(),
            run_plugin
                .configuration
                .inner()
                .get(SHARED_INSTANCE_KEY)
                .cloned(),
        )
    }
    fn load_shared_slot(
        &mut self,
        run: RunPlugin,
        plugin_config: PluginConfig,
        tab_index: Option<usize>,
        size: Size,
        cwd: Option<PathBuf>,
        skip_cache: bool,
        client_id: ClientId,
        is_background: bool,
        wasm_bytes: Option<Vec<u8>>,
    ) -> Result<(PluginId, ClientId)> {
        let slot_id = self.next_plugin_id;
        self.next_plugin_id += 1;
        let key = Self::shared_key_of(&run);
        let kind = if is_background {
            SlotKind::Background
        } else {
            SlotKind::Pane
        };
        let plugin_cwd = cwd.unwrap_or_else(|| self.zellij_cwd.clone());
        self.plugin_map.lock().unwrap().insert_metadata(
            slot_id,
            PluginMetadata {
                plugin_config: plugin_config.clone(),
                tab_index,
                rows: size.rows,
                columns: size.cols,
                cwd: plugin_cwd.clone(),
                is_background,
            },
        );
        let slot = SharedSlot {
            kind,
            configuration: run.configuration.inner().clone(),
            rows: size.rows,
            columns: size.cols,
        };
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::AnimatePluginLoading(slot_id));
        let instance_id = match self.shared_keys.get(&key).copied() {
            Some(instance_id) => {
                let instance_is_running = self
                    .plugin_map
                    .lock()
                    // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                    .unwrap()
                    .shared_running_plugin(instance_id)
                    .is_some();
                let instance_is_in_blocking_call = self
                    .plugin_map
                    .lock()
                    // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
                    .unwrap()
                    .shared_instance_in_blocking_call(instance_id);
                let (slot_added_sender, slot_added_receiver) = std::sync::mpsc::channel();
                let slot_bytes = string_map_bytes(&slot.configuration);
                let reservation = match self.plugin_executor.reserve_job(
                    instance_id,
                    JobClass::Lifecycle,
                    slot_bytes,
                ) {
                    Ok(reservation) => reservation,
                    Err(e) => {
                        self.plugin_map.lock().unwrap().remove_metadata(slot_id);
                        let _ = self.senders.send_to_background_jobs(
                            BackgroundJob::StopPluginLoadingAnimation(slot_id),
                        );
                        return Err(anyhow!(
                            "failed to admit shared plugin slot {}: {}",
                            slot_id,
                            e
                        ));
                    },
                };
                self.register_shared_slot(instance_id, slot_id, kind);
                reservation.commit(Box::new(
                    move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                        add_slot_job(
                            senders,
                            plugin_map,
                            instance_id,
                            slot_id,
                            slot,
                            slot_added_sender,
                        );
                    },
                ));
                if instance_is_running && !instance_is_in_blocking_call {
                    let _ = slot_added_receiver
                        .recv_timeout(std::time::Duration::from_millis(SLOT_ADDED_WAIT_MS));
                }
                instance_id
            },
            None => {
                let instance_id = self.next_plugin_id;
                self.next_plugin_id += 1;
                let mut loading_context = LoadingContext::new(
                    &self,
                    Some(plugin_cwd),
                    plugin_config,
                    instance_id,
                    client_id,
                    None,
                    size,
                );
                loading_context.wasm_bytes = wasm_bytes;
                loading_context.plugin_own_data_dir = ZELLIJ_SESSION_CACHE_DIR
                    .join(make_plugin_url_path_safe(
                        Url::from(&loading_context.plugin_config.location).to_string(),
                    ))
                    .join(format!("{}", instance_id));
                let persisted_loading_context = {
                    let mut loading_context = loading_context.clone();
                    loading_context.wasm_bytes = None;
                    loading_context
                };
                let load_bytes = loading_context
                    .wasm_bytes
                    .as_ref()
                    .map(Vec::len)
                    .unwrap_or(0);
                let reservation = match self
                    .plugin_executor
                    .reserve_plugin_load(instance_id, load_bytes)
                {
                    Ok(reservation) => reservation,
                    Err(e) => {
                        self.plugin_map.lock().unwrap().remove_metadata(slot_id);
                        let _ = self.senders.send_to_background_jobs(
                            BackgroundJob::StopPluginLoadingAnimation(slot_id),
                        );
                        return Err(anyhow!(
                            "failed to admit shared plugin instance {}: {}",
                            instance_id,
                            e
                        ));
                    },
                };
                self.shared_instances.insert(
                    instance_id,
                    SharedInstanceInfo {
                        key: key.clone(),
                        loading_context: persisted_loading_context,
                        slots: BTreeSet::new(),
                        background_slots: BTreeSet::new(),
                    },
                );
                self.shared_keys.insert(key, instance_id);
                self.register_shared_slot(instance_id, slot_id, kind);
                let mut initial_slots = BTreeMap::new();
                initial_slots.insert(slot_id, slot);
                let executor = self.plugin_executor.clone();
                reservation.commit(Box::new(
                    move |senders, plugin_map, connected_clients, plugin_cache, engine| {
                        let failure_senders = senders.clone();
                        if !start_instance_job(
                            senders,
                            plugin_map,
                            connected_clients,
                            plugin_cache,
                            engine,
                            loading_context,
                            skip_cache,
                            instance_id,
                            initial_slots,
                        ) {
                            if failure_senders
                                .send_to_plugin(PluginInstruction::SharedInstanceLoadFailed(
                                    instance_id,
                                ))
                                .is_err()
                            {
                                executor.unregister_plugin(instance_id);
                            }
                        }
                    },
                ));
                instance_id
            },
        };
        if is_background {
            self.notify_shared_background_subscriptions(instance_id);
        }
        Ok((slot_id, client_id))
    }
    fn register_shared_slot(&mut self, instance_id: PluginId, slot_id: PluginId, kind: SlotKind) {
        self.shared_slot_owner.insert(slot_id, instance_id);
        if let Some(info) = self.shared_instances.get_mut(&instance_id) {
            info.slots.insert(slot_id);
            if kind == SlotKind::Background {
                info.background_slots.insert(slot_id);
            }
        }
    }
    pub fn shared_instance_load_failed(&mut self, instance_id: PluginId) {
        let slot_ids: Vec<PluginId> = self
            .shared_instances
            .get(&instance_id)
            .map(|info| info.slots.iter().copied().collect())
            .unwrap_or_default();
        for slot_id in slot_ids {
            self.unload_shared_slot(slot_id);
        }
        if let Some(info) = self.shared_instances.remove(&instance_id) {
            if self.shared_keys.get(&info.key) == Some(&instance_id) {
                self.shared_keys.remove(&info.key);
            }
            self.quarantine.forget(instance_id);
            self.permission_prompts.remove(&instance_id);
            let pipes_to_release = self.pending_pipes.unload_plugin(&instance_id);
            self.send_pipe_releases(pipes_to_release);
            self.forget_pipe_popups_of_plugin(instance_id);
            let prompt_actions = self.prompt_requests.plugin_unloaded(instance_id);
            self.apply_prompt_actions(prompt_actions);
            self.settings_entries_changed(|entries| entries.plugin_unloaded(instance_id));
            let _ = self
                .plugin_executor
                .execute_plugin_unload_many(instance_id, Vec::new());
        }
    }
    fn unload_shared_slot(&mut self, slot_id: PluginId) {
        let Some(instance_id) = self.shared_slot_owner.get(&slot_id).copied() else {
            return;
        };
        // Last slot tears the instance down. Revoke that load before removing
        // ownership records or waiting on a queue, so a blocked guest cannot
        // stall retirement and a rejected submit cannot leave it alive.
        let tearing_down = match self.shared_instances.get(&instance_id) {
            Some(info) => !info.slots.iter().any(|id| *id != slot_id),
            None => true,
        };
        if tearing_down {
            revoke_plugin_loads(instance_id);
            self.release_key_intercepts(instance_id);
        }
        let Some(instance_id) = self.shared_slot_owner.remove(&slot_id) else {
            return;
        };
        self.quarantine.forget(slot_id);
        self.permission_prompts.remove(&slot_id);
        self.plugin_map.lock().unwrap().remove_metadata(slot_id);
        let connected_clients: Vec<ClientId> = self.connected_clients.lock().unwrap().clone();
        let tear_down = match self.shared_instances.get_mut(&instance_id) {
            Some(info) => {
                info.slots.remove(&slot_id);
                if info.background_slots.remove(&slot_id) {
                    for client_id in &connected_clients {
                        let _ = self.senders.send_to_screen(
                            ScreenInstruction::UpdateBackgroundPluginSubscriptions(
                                slot_id,
                                *client_id,
                                HashSet::new(),
                            ),
                        );
                    }
                }
                info.slots.is_empty()
            },
            None => true,
        };
        if tear_down {
            if let Some(info) = self.shared_instances.remove(&instance_id) {
                if self.shared_keys.get(&info.key) == Some(&instance_id) {
                    self.shared_keys.remove(&info.key);
                }
            }
            self.quarantine.forget(instance_id);
            self.permission_prompts.remove(&instance_id);
            let _ = self.plugin_executor.execute_plugin_unload(
                instance_id,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    // Guest BeforeClose only on the executor thread. A rejected
                    // retirement runs this closure inline and must not enter
                    // the guest or take the execution mutex.
                    remove_slot_job(
                        senders,
                        plugin_map,
                        instance_id,
                        slot_id,
                        true,
                        on_plugin_executor(),
                    );
                },
            );
            let pipes_to_release = self.pending_pipes.unload_plugin(&instance_id);
            self.send_pipe_releases(pipes_to_release);
            let prompt_actions = self.prompt_requests.plugin_unloaded(instance_id);
            self.apply_prompt_actions(prompt_actions);
            self.settings_entries_changed(|entries| entries.plugin_unloaded(instance_id));
        } else {
            // Lifecycle admission can fail once the queue is full. The closure
            // still runs, on the caller, and skips the guest in that case.
            self.plugin_executor.submit_lifecycle_or_run(
                instance_id,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    remove_slot_job(
                        senders,
                        plugin_map,
                        instance_id,
                        slot_id,
                        false,
                        on_plugin_executor(),
                    );
                },
            );
        }
        self.cached_plugin_map.clear();
        let plugin_list = self.plugin_map.lock().unwrap().list_plugins();
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));
    }
    fn shared_instances_matching(&self, run_plugin: &RunPlugin) -> Vec<PluginId> {
        let Some(instance_id) = self.shared_keys.get(&Self::shared_key_of(run_plugin)) else {
            return vec![];
        };
        let Some(info) = self.shared_instances.get(instance_id) else {
            return vec![];
        };
        let plugin_map = self.plugin_map.lock().unwrap();
        let matches = info.slots.iter().any(|slot_id| {
            plugin_map
                .metadata(*slot_id)
                .map(|metadata| {
                    metadata.plugin_config.initial_userspace_configuration
                        == run_plugin.configuration
                })
                .unwrap_or(false)
        });
        if matches {
            vec![*instance_id]
        } else {
            vec![]
        }
    }
    fn reload_shared_instance_with_bytes(
        &mut self,
        instance_id: PluginId,
        wasm_bytes: Option<Vec<u8>>,
    ) {
        if !self.shared_instances.contains_key(&instance_id) {
            return;
        }
        let Some(info) = self.shared_instances.get(&instance_id) else {
            return;
        };
        let slot_ids: Vec<PluginId> = info.slots.iter().copied().collect();
        let mut loading_context = info.loading_context.clone();
        loading_context.wasm_bytes = wasm_bytes;
        loading_context.generation = next_plugin_generation();
        if let Some(first_client_id) = self.get_first_client_id() {
            loading_context.client_id = first_client_id;
        }
        let reload_bytes = loading_context
            .wasm_bytes
            .as_ref()
            .map(Vec::len)
            .unwrap_or(0);
        let reservation =
            match self
                .plugin_executor
                .reserve_job(instance_id, JobClass::Lifecycle, reload_bytes)
            {
                Ok(reservation) => reservation,
                Err(e) => {
                    log::error!(
                        "Failed to admit reload of shared plugin instance {}: {}",
                        instance_id,
                        e
                    );
                    return;
                },
            };
        self.unregister_settings_entry(instance_id);
        self.quarantine
            .begin_generation(instance_id, loading_context.generation);
        for slot_id in &slot_ids {
            self.quarantine
                .begin_generation(*slot_id, loading_context.generation);
            let _ = self
                .senders
                .send_to_screen(ScreenInstruction::ClearLiveViewsOfPlugin(*slot_id));
        }
        if let Some(info) = self.shared_instances.get_mut(&instance_id) {
            info.loading_context.generation = loading_context.generation;
        }
        let loading_indication =
            LoadingIndication::new(loading_context.plugin_config.location.to_string());
        self.start_plugin_loading_indication(&slot_ids, &loading_indication);
        reservation.commit(Box::new(
            move |senders, plugin_map, connected_clients, plugin_cache, engine| {
                let _ = start_instance_job(
                    senders,
                    plugin_map,
                    connected_clients,
                    plugin_cache,
                    engine,
                    loading_context,
                    true,
                    instance_id,
                    BTreeMap::new(),
                );
            },
        ));
    }
    fn existing_shared_targets(
        &mut self,
        run_plugin: &RunPlugin,
        skip_cache: bool,
    ) -> Option<Vec<(PluginId, Option<ClientId>)>> {
        // Whether an instance is already running is answered from state; classifying the module
        // reads the whole file, which every URL-addressed CLI pipe payload would otherwise pay
        // on the management thread even for a plugin that has been running for hours. A key
        // that has an instance is a shared module (only those are keyed), so the classification
        // is only needed when the caller asks for a fresh look at the file.
        let instance_id = *self.shared_keys.get(&Self::shared_key_of(run_plugin))?;
        if skip_cache {
            let plugin_config = PluginConfig::from_run_plugin(run_plugin)?;
            let wasm_bytes = self.plugin_wasm_bytes(&plugin_config);
            if !Self::module_bytes_are_shared(wasm_bytes.as_deref()) {
                return None;
            }
        }
        let info = self.shared_instances.get(&instance_id)?;
        let matching_slots: Vec<PluginId> = {
            let plugin_map = self.plugin_map.lock().unwrap();
            info.slots
                .iter()
                .copied()
                .filter(|slot_id| {
                    plugin_map
                        .metadata(*slot_id)
                        .map(|metadata| {
                            metadata.plugin_config.initial_userspace_configuration
                                == run_plugin.configuration
                        })
                        .unwrap_or(false)
                })
                .collect()
        };
        match matching_slots.as_slice() {
            [] => None,
            [slot_id] => Some(vec![(*slot_id, None)]),
            _ => Some(vec![(instance_id, None)]),
        }
    }
    fn notify_shared_background_subscriptions(&self, instance_id: PluginId) {
        let Some(info) = self.shared_instances.get(&instance_id) else {
            return;
        };
        if info.background_slots.is_empty() {
            return;
        }
        let subscriptions = self
            .plugin_map
            .lock()
            .unwrap()
            .shared_running_plugin(instance_id)
            .map(|(_, subscriptions)| subscriptions.lock().unwrap().clone());
        let Some(subscriptions) = subscriptions else {
            return;
        };
        let connected_clients: Vec<ClientId> = self.connected_clients.lock().unwrap().clone();
        for slot_id in &info.background_slots {
            for client_id in &connected_clients {
                let _ = self.senders.send_to_screen(
                    ScreenInstruction::UpdateBackgroundPluginSubscriptions(
                        *slot_id,
                        *client_id,
                        subscriptions.clone(),
                    ),
                );
            }
        }
    }
    fn add_client_to_shared_instances(&mut self, client_id: ClientId) {
        let instance_ids: Vec<PluginId> = self.shared_instances.keys().copied().collect();
        for instance_id in instance_ids {
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Lifecycle,
                0,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    client_job(senders, plugin_map, instance_id, client_id, true);
                },
            );
            if self.shared_instance_subscribes_to(instance_id, EventType::InitialKeybinds) {
                self.send_keybinds_payload_to_plugin(
                    instance_id,
                    client_id,
                    self.keybinds_of_client(client_id).as_ref().clone(),
                );
            }
            self.notify_shared_background_subscriptions(instance_id);
        }
    }
    fn remove_client_from_shared_instances(&mut self, client_id: ClientId) {
        let instance_ids: Vec<PluginId> = self.shared_instances.keys().copied().collect();
        for instance_id in instance_ids {
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Lifecycle,
                0,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    client_job(senders, plugin_map, instance_id, client_id, false);
                },
            );
            if let Some(info) = self.shared_instances.get(&instance_id) {
                for slot_id in &info.background_slots {
                    let _ = self.senders.send_to_screen(
                        ScreenInstruction::UpdateBackgroundPluginSubscriptions(
                            *slot_id,
                            client_id,
                            HashSet::new(),
                        ),
                    );
                }
            }
        }
    }
    pub fn update_client_visible_plugins(
        &mut self,
        visible_plugins: HashMap<ClientId, HashSet<PluginId>>,
    ) {
        for (instance_id, info) in self.shared_instances.iter() {
            let instance_id = *instance_id;
            let generation = info.loading_context.generation;
            let visible_slots: HashMap<ClientId, HashSet<PluginId>> = visible_plugins
                .iter()
                .map(|(client_id, plugin_ids)| {
                    (
                        *client_id,
                        plugin_ids
                            .iter()
                            .copied()
                            .filter(|plugin_id| info.slots.contains(plugin_id))
                            .collect(),
                    )
                })
                .collect();
            let visibility_bytes = visible_slots
                .values()
                .map(|slots| slots.len().saturating_mul(8).saturating_add(64))
                .fold(0usize, usize::saturating_add);
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Replaceable(replaceable_visibility_key(generation)),
                visibility_bytes,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    visibility_job(senders, plugin_map, instance_id, visible_slots);
                },
            );
        }
    }
    fn prepare_shared_event(
        &self,
        instance_id: PluginId,
        event: &Event,
        client_id: Option<ClientId>,
    ) -> (Event, Option<SharedKeybinds>) {
        match event {
            Event::ModeUpdate(mode_info) => {
                let mut mode_info = mode_info.clone();
                mode_info.keybinds = vec![];
                if mode_info.base_mode.is_none() {
                    mode_info.base_mode = Some(
                        client_id
                            .and_then(|client_id| self.base_modes.get(&client_id).copied())
                            .unwrap_or(self.default_mode),
                    );
                }
                let keybinds = if self
                    .shared_instance_subscribes_to(instance_id, EventType::InitialKeybinds)
                {
                    None
                } else {
                    Some(match client_id {
                        Some(client_id) => self.keybinds_of_client(client_id).clone(),
                        None => self.default_keybinds.clone(),
                    })
                };
                (Event::ModeUpdate(mode_info), keybinds)
            },
            event => (event.clone(), None),
        }
    }
    fn push_shared_event(
        &self,
        batches: &mut BTreeMap<PluginId, Vec<SharedEventBatchEntry>>,
        instance_id: PluginId,
        slot_id: Option<PluginId>,
        client_id: Option<ClientId>,
        event: &Event,
    ) {
        let context = shared_event_context(event, slot_id, client_id);
        let (event, keybinds) = self.prepare_shared_event(instance_id, event, client_id);
        let batch = batches.entry(instance_id).or_default();
        if batch
            .iter()
            .any(|(existing, existing_context, existing_keybinds)| {
                existing_context == &context && existing == &event && existing_keybinds == &keybinds
            })
        {
            return;
        }
        batch.push((event, context, keybinds));
    }
    fn dispatch_shared_updates(
        &mut self,
        updates: Vec<(Option<PluginId>, Option<ClientId>, Event)>,
        shutdown_sender: Sender<()>,
    ) -> Vec<(Option<PluginId>, Option<ClientId>, Event)> {
        if self.shared_instances.is_empty() {
            return updates;
        }
        let mut batches: BTreeMap<PluginId, Vec<SharedEventBatchEntry>> = BTreeMap::new();
        let mut legacy_updates = vec![];
        let all_instance_ids: Vec<PluginId> = self
            .shared_instances
            .keys()
            .copied()
            .filter(|instance_id| !self.quarantine.contains(instance_id))
            .collect();
        for (plugin_id, client_id, event) in updates {
            match plugin_id {
                Some(plugin_id) => match self.shared_instance_of(plugin_id) {
                    // a quarantined instance receives nothing until it is reloaded
                    Some(instance_id) if self.quarantine.contains(&instance_id) => {},
                    Some(instance_id) => {
                        let slot_id = if plugin_id != instance_id {
                            Some(plugin_id)
                        } else {
                            None
                        };
                        self.push_shared_event(
                            &mut batches,
                            instance_id,
                            slot_id,
                            client_id,
                            &event,
                        );
                    },
                    None => legacy_updates.push((Some(plugin_id), client_id, event)),
                },
                None => {
                    for instance_id in &all_instance_ids {
                        self.push_shared_event(&mut batches, *instance_id, None, client_id, &event);
                    }
                    legacy_updates.push((None, client_id, event));
                },
            }
        }
        for (instance_id, events) in batches {
            let _s = shutdown_sender.clone();
            let events_bytes = events
                .iter()
                .map(|(event, _, keybinds)| {
                    event_payload_bytes(event).saturating_add(
                        keybinds
                            .as_ref()
                            .map(|keybinds| keybinds_vec_bytes(keybinds.as_ref()))
                            .unwrap_or(0),
                    )
                })
                .fold(0usize, usize::saturating_add);
            self.plugin_executor.execute_for_plugin_with(
                instance_id,
                JobClass::Ordered,
                events_bytes,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let _s = _s;
                    apply_events_job(senders, plugin_map, instance_id, events);
                },
            );
        }
        legacy_updates
    }
    fn dispatch_shared_pipes(
        &mut self,
        messages: Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)>,
        shutdown_sender: Sender<()>,
        notification_end: &mut Option<NotificationEnd>,
    ) -> Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)> {
        if self.shared_instances.is_empty() {
            return messages;
        }
        let mut batches: BTreeMap<PluginId, Vec<(PipeMessage, EventContext)>> = BTreeMap::new();
        let mut legacy_messages = vec![];
        let all_instance_ids: Vec<PluginId> = self
            .shared_instances
            .keys()
            .copied()
            .filter(|instance_id| !self.quarantine.contains(instance_id))
            .collect();
        for (plugin_id, client_id, pipe_message) in messages {
            let targets: Vec<(PluginId, Option<PluginId>)> = match plugin_id {
                Some(plugin_id) => match self.shared_instance_of(plugin_id) {
                    // a quarantined instance receives nothing until it is reloaded
                    Some(instance_id) if self.quarantine.contains(&instance_id) => {
                        // The caller cannot know the plugin is gone; release the pipe so the
                        // `zellij pipe` client does not wait forever.
                        self.release_cli_pipe_with_error(&pipe_message);
                        continue;
                    },
                    Some(instance_id) => {
                        let slot_id = if plugin_id != instance_id {
                            Some(plugin_id)
                        } else {
                            None
                        };
                        vec![(instance_id, slot_id)]
                    },
                    None => {
                        legacy_messages.push((Some(plugin_id), client_id, pipe_message));
                        continue;
                    },
                },
                None => {
                    legacy_messages.push((None, client_id, pipe_message.clone()));
                    all_instance_ids.iter().map(|id| (*id, None)).collect()
                },
            };
            for (instance_id, slot_id) in targets {
                let context = EventContext { slot_id, client_id };
                let batch = batches.entry(instance_id).or_default();
                if batch.iter().any(|(existing, existing_context)| {
                    existing == &pipe_message && existing_context == &context
                }) {
                    continue;
                }
                if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                    self.pending_pipes.mark_being_processed(
                        pipe_id,
                        &instance_id,
                        &SHARED_PIPE_CLIENT,
                    );
                }
                batch.push((pipe_message.clone(), context));
            }
        }
        for (instance_id, messages) in batches {
            let _s = shutdown_sender.clone();
            let notification_end = notification_end.take();
            let messages_bytes = messages
                .iter()
                .map(|(message, _)| pipe_message_payload_bytes(message))
                .fold(0usize, usize::saturating_add);
            let messages = Arc::new(messages);
            let worker_messages = messages.clone();
            let failure_messages = messages.clone();
            if let Err(e) = self.plugin_executor.try_execute_for_plugin(
                instance_id,
                JobClass::Ordered,
                messages_bytes,
                move |senders, plugin_map, _connected_clients, _plugin_cache, _engine| {
                    let _s = _s;
                    apply_pipes_job(senders, plugin_map, instance_id, worker_messages);
                    drop(notification_end);
                },
            ) {
                log::warn!(
                    "Plugin executor rejected shared pipes for {}: {}",
                    instance_id,
                    e
                );
                for (pipe_message, _) in failure_messages.iter() {
                    self.release_cli_pipe_with_error(pipe_message);
                }
            }
        }
        legacy_messages
    }
    pub fn detect_and_notify_plugin_config_changes(
        &mut self,
        new_plugins: &PluginAliases,
        shutdown_send: Sender<()>,
    ) -> Result<()> {
        let err_context = || "failed to detect plugin config changes";

        let shared_slots: Vec<PluginId> = self.shared_slot_owner.keys().copied().collect();
        for slot_id in shared_slots {
            let Some(run_plugin) = self.run_plugin_of_plugin_id(slot_id) else {
                continue;
            };
            let candidates: Vec<RunPlugin> = new_plugins
                .aliases
                .values()
                .map(|run_plugin| canonical_run_plugin(run_plugin))
                .filter(|candidate| candidate.location == run_plugin.location)
                .collect();
            if candidates
                .iter()
                .any(|candidate| candidate.configuration == run_plugin.configuration)
            {
                continue;
            }
            if let [candidate] = candidates.as_slice() {
                let event =
                    Event::PluginConfigurationChanged(candidate.configuration.inner().clone());
                self.update_plugins(vec![(Some(slot_id), None, event)], shutdown_send.clone())
                    .with_context(err_context)?;
            }
        }

        // Get all running plugins
        let running_plugins = self.plugin_map.lock().unwrap().running_plugins();

        for (plugin_id, client_id, running_plugin) in running_plugins {
            let running_plugin = running_plugin.lock().unwrap();
            let plugin_env = &running_plugin.store.data();
            let current_config = &plugin_env.plugin.initial_userspace_configuration;
            let plugin_location = &plugin_env.plugin.location;

            // Look up this plugin in the new config by location
            // Note: PluginAliases is HashMap<String, RunPlugin>, so we need to iterate
            let new_config_for_location = new_plugins
                .aliases
                .values()
                .find(|run_plugin| &run_plugin.location == plugin_location)
                .map(|run_plugin| &run_plugin.configuration);

            if let Some(new_config) = new_config_for_location {
                // Compare configurations - only fire event if changed
                if current_config != new_config {
                    drop(running_plugin); // Release lock before sending

                    let event = Event::PluginConfigurationChanged(new_config.inner().clone());
                    let updates = vec![(Some(plugin_id), Some(client_id), event)];
                    self.update_plugins(updates, shutdown_send.clone())
                        .with_context(err_context)?;
                }
            }
        }

        Ok(())
    }
}

pub fn handle_plugin_successful_loading(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    plugin_list: BTreeMap<PluginId, RunPlugin>,
) {
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    let _ = senders.send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
    let _ = senders.send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));
    let _ = senders.send_to_plugin(PluginInstruction::RequestStateUpdateForPlugin(plugin_id));
}

pub fn handle_plugin_loading_failure(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    loading_indication: &mut LoadingIndication,
    error: impl std::fmt::Debug,
    client_id: Option<ClientId>,
) {
    log::error!("{:?}", error);
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    loading_indication.indicate_loading_error(format!("{:?}", error));
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    if let Some(client_id) = client_id {
        let _ = senders.send_to_server(ServerInstruction::LogError(
            vec![format!("{:?}", error)],
            client_id,
            None,
        ));
    }
}

/// Drop state a plugin installed before its load failed. It never produced a running instance
/// that the normal unload path could clean up, so its key intercepts, highlights, and settings
/// entry would otherwise outlive it.
pub fn clear_failed_plugin_load_side_effects(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    client_ids: impl IntoIterator<Item = ClientId>,
) {
    for client_id in client_ids {
        let _ = senders.send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(
            plugin_id, client_id,
        ));
    }
    let _ = senders.send_to_screen(ScreenInstruction::ClearAllPluginHighlights(plugin_id));
    let _ = senders.send_to_plugin(PluginInstruction::UnregisterSettingsEntry(plugin_id));
}

// TODO: move to permissions?
pub fn check_event_permission(
    plugin_env: &PluginEnv,
    event: &Event,
) -> (PermissionStatus, Option<PermissionType>) {
    if plugin_env.plugin.is_builtin() {
        // built-in plugins can do all the things because they're part of the application and
        // there's no use to deny them anything
        return (PermissionStatus::Granted, None);
    }
    let permission = match event {
        Event::ModeUpdate(..)
        | Event::TabUpdate(..)
        | Event::PaneUpdate(..)
        | Event::SessionUpdate(..)
        | Event::CopyToClipboard(..)
        | Event::SystemClipboardFailure
        | Event::CommandPaneOpened(..)
        | Event::CommandPaneExited(..)
        | Event::PaneClosed(..)
        | Event::EditPaneOpened(..)
        | Event::EditPaneExited(..)
        | Event::FailedToWriteConfigToDisk(..)
        | Event::CommandPaneReRun(..)
        | Event::CwdChanged(..)
        | Event::CommandChanged(..)
        | Event::AvailableLayoutInfo(..)
        | Event::AvailableKeybindPresets(..)
        | Event::PluginConfigurationChanged(..)
        | Event::HighlightClicked { .. }
        | Event::SoftKeyboardVisibilityChanged(..)
        | Event::HintText(..)
        | Event::ActivePaneScroll(..)
        | Event::NestedSessionModeUpdate { .. }
        | Event::NestedSessionEnded { .. }
        | Event::ContextMenu(..)
        | Event::SessionSuggestions(..)
        | Event::SessionCountsUpdate(..)
        | Event::FolderSessionsUpdate(..)
        | Event::TerminalCommandSubmitted
        | Event::SavedSessionPreview(..)
        | Event::PaneAttentionChanged(..)
        | Event::PromptShown(..)
        | Event::CommandStarted(..)
        | Event::CommandFinished(..)
        | Event::GlobalAttentionUpdate(..)
        | Event::SessionHistoryUpdate(..)
        | Event::DesktopIntegration(..)
        | Event::PluginSettingsEntries(..)
        | Event::PluginActivated(..)
        | Event::InputReceived => PermissionType::ReadApplicationState,
        Event::SessionPreview(..) => PermissionType::ReadPaneContents,
        Event::ConfigChangesDropped(..) | Event::ConfigFileChangedSinceRead => {
            PermissionType::Reconfigure
        },
        Event::WebServerStatus(..) => PermissionType::StartWebServer,
        Event::PaneRenderReport(..) => PermissionType::ReadPaneContents,
        Event::LiveViewStateChanged { .. } => PermissionType::ReadPaneContents,
        Event::FileSystemCreate(..)
        | Event::FileSystemRead(..)
        | Event::FileSystemUpdate(..)
        | Event::FileSystemDelete(..) => PermissionType::FullHdAccess,
        Event::UserAction(..) => PermissionType::InterceptInput,
        _ => return (PermissionStatus::Granted, None),
    };

    if let Some(permissions) = plugin_env.permissions.lock().unwrap().as_ref() {
        if permissions.contains(&permission) {
            return (PermissionStatus::Granted, None);
        }
    }

    (PermissionStatus::Denied, Some(permission))
}

pub(crate) fn can_read_command_text(plugin_env: &PluginEnv) -> bool {
    if plugin_env.plugin.is_builtin() {
        return true;
    }
    plugin_env
        .permissions
        .lock()
        // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
        .unwrap()
        .as_ref()
        .map(|permissions| permissions.contains(&PermissionType::ReadPaneContents))
        .unwrap_or(false)
}

pub(crate) fn redact_pane_info(pane_info: &mut zellij_utils::data::PaneInfo) {
    pane_info.attention = std::mem::take(&mut pane_info.attention).without_command_text();
    pane_info.command_state = std::mem::take(&mut pane_info.command_state).without_command_text();
    pane_info.terminal_command = None;
}

pub(crate) fn redact_session_info_for_plugin(
    plugin_env: &PluginEnv,
    session_info: &mut zellij_utils::data::SessionInfo,
) {
    if can_read_command_text(plugin_env) {
        return;
    }
    for pane_infos in session_info.panes.panes.values_mut() {
        for pane_info in pane_infos.iter_mut() {
            redact_pane_info(pane_info);
        }
    }
}

pub fn redact_event_for_plugin(plugin_env: &PluginEnv, event: &mut Event) {
    if can_read_command_text(plugin_env) {
        return;
    }
    match event {
        Event::PaneUpdate(manifest) => {
            for pane_infos in manifest.panes.values_mut() {
                for pane_info in pane_infos.iter_mut() {
                    redact_pane_info(pane_info);
                }
            }
        },
        Event::PaneAttentionChanged(_, attention) => {
            *attention = std::mem::take(attention).without_command_text();
        },
        Event::CommandStarted(_, command) => {
            *command = None;
        },
        Event::CommandFinished(_, outcome) => {
            outcome.command = None;
        },
        Event::GlobalAttentionUpdate(global) => {
            *global = std::mem::take(global).without_command_text();
        },
        Event::SessionUpdate(sessions, _) => {
            for session in sessions.iter_mut() {
                for pane_infos in session.panes.panes.values_mut() {
                    for pane_info in pane_infos.iter_mut() {
                        redact_pane_info(pane_info);
                    }
                }
            }
        },
        Event::SavedSessionPreview(preview) => {
            for tab in preview.tabs.iter_mut() {
                for pane in tab.panes.iter_mut() {
                    pane.contents = None;
                }
            }
        },
        _ => {},
    }
}

pub(crate) fn apply_pending_popup_size(running_plugin: &mut RunningPlugin) -> bool {
    let Some((columns, rows)) = running_plugin.store.data_mut().pending_popup_size.take() else {
        return false;
    };
    let changed = running_plugin.rows != rows || running_plugin.columns != columns;
    running_plugin.rows = rows;
    running_plugin.columns = columns;
    changed
}

pub(crate) fn stale_timer_for_load(
    event: &Event,
    scheduled_generation: Option<u64>,
    running_generation: u64,
) -> bool {
    match event {
        Event::Timer(_) => {
            scheduled_generation.is_some_and(|scheduled| scheduled < running_generation)
        },
        _ => false,
    }
}

pub fn apply_event_to_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    running_plugin: &mut RunningPlugin,
    event: &Event,
    plugin_render_assets: &mut Vec<PluginRenderAsset>,
    senders: ThreadSenders,
    plugin_subscriptions: &HashSet<EventType>,
    scheduled_timer_generation: Option<u64>,
) -> Result<()> {
    if stale_timer_for_load(
        event,
        scheduled_timer_generation,
        running_plugin.store.data().generation,
    ) {
        return Ok(());
    }
    let instance = running_plugin.instance.clone();

    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    match check_event_permission(running_plugin.store.data(), event) {
        (PermissionStatus::Granted, _) => {
            let mut event = event.clone();
            redact_event_for_plugin(running_plugin.store.data(), &mut event);
            let mut keybinds = None;
            if let Event::ModeUpdate(mode_info) = &mut event {
                mode_info.keybinds = vec![];
                if mode_info.base_mode.is_none() {
                    mode_info.base_mode = Some(running_plugin.store.data().default_mode);
                }
                if !plugin_subscriptions.contains(&EventType::InitialKeybinds) {
                    keybinds = Some(running_plugin.store.data().keybinds.clone());
                }
            }
            let is_permission_request_result = matches!(event, Event::PermissionRequestResult(..));
            let protobuf_event = event_to_protobuf_with_keybinds(event, keybinds.as_deref());
            let protobuf_event = match protobuf_event {
                Ok(protobuf_event) if running_plugin.built_against_0_45 => {
                    match released_contract::event_for_0_45_plugin(protobuf_event) {
                        Some(protobuf_event) => Ok(protobuf_event),
                        None => return Ok(()),
                    }
                },
                other => other,
            };
            match protobuf_event {
                Ok(protobuf_event) => {
                    let update = instance
                        .get_typed_func::<(), i32>(&mut running_plugin.store, "update")
                        .with_context(err_context)?;
                    wasi_write_object(running_plugin.store.data(), &protobuf_event.encode_to_vec())
                        .with_context(err_context)?;
                    crate::plugins::shared::refuel_for_callback(&mut running_plugin.store);
                    let should_render = update
                        .call(&mut running_plugin.store, ())
                        .with_context(err_context)?;
                    let mut should_render = should_render == 1;
                    if is_permission_request_result {
                        // we always render in this case, otherwise the request permission screen stays on
                        // screen
                        should_render = true;
                    }
                    if apply_pending_popup_size(running_plugin) {
                        should_render = true;
                    }
                    let rows = running_plugin.rows;
                    let columns = running_plugin.columns;
                    if rows > 0 && columns > 0 && should_render {
                        let rendered_bytes = instance
                            .get_typed_func::<(i32, i32), ()>(&mut running_plugin.store, "render")
                            .and_then(|render| {
                                crate::plugins::shared::refuel_for_callback(
                                    &mut running_plugin.store,
                                );
                                render
                                    .call(&mut running_plugin.store, (rows as i32, columns as i32))
                            })
                            .map_err(|e| anyhow!(e))
                            .and_then(|_| {
                                wasi_read_string(running_plugin.store.data())
                                    .map_err(|e| anyhow!(e))
                            })
                            .with_context(err_context)?;
                        let pipes_to_block_or_unblock =
                            pipes_to_block_or_unblock(running_plugin, None);
                        let plugin_render_asset = PluginRenderAsset::new(
                            plugin_id,
                            client_id,
                            rendered_bytes.as_bytes().to_vec(),
                        )
                        .with_pipes(pipes_to_block_or_unblock)
                        .rendered_at(rows, columns);
                        plugin_render_assets.push(plugin_render_asset);
                    } else {
                        // This is a bit of a hack to get around the fact that plugins are allowed not to
                        // render and still unblock CLI pipes
                        let pipes_to_block_or_unblock =
                            pipes_to_block_or_unblock(running_plugin, None);
                        let plugin_render_asset =
                            PluginRenderAsset::new(plugin_id, client_id, vec![])
                                .with_pipes(pipes_to_block_or_unblock);
                        let _ = senders
                            .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
                                plugin_render_asset,
                            ]))
                            .context("failed to unblock input pipe");
                    }
                },
                Err(e) => {
                    log::error!("Failed to convert to protobuf: {:?}", e);
                },
            }
        },
        (PermissionStatus::Denied, permission) => {
            log::error!(
                "PluginId '{}' permission '{}' is not allowed - Event '{:?}' denied",
                plugin_id,
                permission
                    .map(|p| p.to_string())
                    .unwrap_or("UNKNOWN".to_owned()),
                EventType::from_str(&event.to_string()).with_context(err_context)?
            );
        },
    }
    Ok(())
}

/// Show a plugin's fatal error in its pane and drop its settings entry.
pub fn show_plugin_crash(plugin_id: PluginId, message: String, senders: ThreadSenders) {
    let mut loading_indication = LoadingIndication::new("Panic!".to_owned());
    loading_indication.indicate_loading_error(message);
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication,
    ));
    let _ = senders.send_to_plugin(PluginInstruction::UnregisterSettingsEntry(plugin_id));
}

/// Report a fatal error from the plugin instance of the given generation and quarantine the
/// plugin unless a newer instance has replaced that one.
pub fn handle_plugin_crash(
    plugin_id: PluginId,
    generation: u64,
    message: String,
    senders: ThreadSenders,
) {
    show_plugin_crash(plugin_id, message, senders.clone());
    let _ = senders.send_to_plugin(PluginInstruction::Quarantine(plugin_id, generation));
}

pub fn apply_before_close_event_to_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    running_plugin: &mut RunningPlugin,
    senders: ThreadSenders,
) -> Result<()> {
    let instance = &running_plugin.instance;

    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let event = Event::BeforeClose;
    let protobuf_event: ProtobufEvent = event
        .clone()
        .try_into()
        .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
    let update = instance
        .get_typed_func::<(), i32>(&mut running_plugin.store, "update")
        .with_context(err_context)?;
    wasi_write_object(running_plugin.store.data(), &protobuf_event.encode_to_vec())
        .with_context(err_context)?;
    crate::plugins::shared::refuel_for_callback(&mut running_plugin.store);
    let _should_render = update
        .call(&mut running_plugin.store, ())
        .with_context(err_context)?;
    let pipes_to_block_or_unblock = pipes_to_block_or_unblock(running_plugin, None);
    let plugin_render_asset =
        PluginRenderAsset::new(plugin_id, client_id, vec![]).with_pipes(pipes_to_block_or_unblock);
    let _ = senders
        .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
            plugin_render_asset,
        ]))
        .context("failed to unblock input pipe");
    Ok(())
}

fn change_host_dir_of_running_plugin(
    running_plugin: &mut RunningPlugin,
    new_host_dir: &PathBuf,
) -> Result<()> {
    if !new_host_dir.try_exists().unwrap_or(false) {
        return Err(anyhow!("Folder {} does not exist", new_host_dir.display()));
    }
    let plugin_env = running_plugin.store.data_mut();
    let wasi_ctx = PluginLoader::create_wasi_ctx(
        new_host_dir,
        &plugin_env.plugin_own_data_dir,
        &plugin_env.plugin_own_cache_dir,
        &ZELLIJ_TMP_DIR,
        &plugin_env.plugin.location.to_string(),
        plugin_env.plugin_id,
        plugin_env.plugin.is_builtin(),
        plugin_env.stdin_pipe.clone(),
        plugin_env.stdout_pipe.clone(),
    )?;
    drop(std::mem::replace(&mut plugin_env.wasi_ctx, wasi_ctx));
    plugin_env.plugin_cwd = new_host_dir.clone();
    Ok(())
}

pub(crate) fn is_hover(event: &Event) -> bool {
    matches!(
        event,
        Event::Mouse(zellij_utils::data::Mouse::Hover(..))
            | Event::MouseWithModifiers(zellij_utils::data::Mouse::Hover(..), _)
    )
}

#[cfg(test)]
mod timer_generation_tests {
    use super::stale_timer_for_load;
    use zellij_utils::data::Event;

    #[test]
    fn a_timer_scheduled_against_generation_g_is_not_applied_to_generation_g2() {
        let generation_g = 4;
        let generation_g2 = 9;
        let timer = Event::Timer(0.25);
        assert!(
            stale_timer_for_load(&timer, Some(generation_g), generation_g2),
            "generation {generation_g2} must not receive a timer from {generation_g}"
        );
        assert!(!stale_timer_for_load(
            &timer,
            Some(generation_g2),
            generation_g2
        ));
        assert!(!stale_timer_for_load(&timer, None, generation_g2));
        assert!(!stale_timer_for_load(
            &Event::InputReceived,
            Some(generation_g),
            generation_g2
        ));
    }
}

#[cfg(test)]
#[path = "unit/hover_merge_tests.rs"]
mod hover_merge_tests;

#[cfg(test)]
#[path = "unit/quarantine_tests.rs"]
mod quarantine_tests;

#[cfg(test)]
#[path = "unit/hardening_tests.rs"]
mod hardening_tests;

#[cfg(test)]
#[path = "unit/startup_tests.rs"]
mod startup_tests;

#[cfg(test)]
#[path = "unit/permission_grant_tests.rs"]
mod permission_grant_tests;
