use crate::plugins::plugin_worker::MessageToWorker;
use crate::plugins::PluginId;
use std::io::Write;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmi::{Instance, Store, StoreLimits};
use wasmi_wasi::WasiCtx;

use crate::{thread_bus::ThreadSenders, ClientId, SharedKeybinds};

use tokio::sync::mpsc::UnboundedSender;
use zellij_utils::{
    data::{Event, EventType, RenderResponse, SlotKind},
    data::InputMode,
    input::command::TerminalAction,
    input::layout::{PluginUserConfiguration, RunPlugin, RunPluginLocation},
    input::plugins::PluginConfig,
};
use zellij_utils::{data::PermissionType, errors::prelude::*};

// the idea here is to provide atomicity when adding/removing plugins from the map (eg. when a new
// client connects) but to also allow updates/renders not to block each other
// so when adding/removing from the map - everything is halted, that's life
// but when cloning the internal RunningPlugin and Subscriptions atomics, we can call methods on
// them without blocking other instances
pub type PluginKey = (PluginId, ClientId);
pub type PluginAssets = (
    Arc<Mutex<RunningPlugin>>,
    Arc<Mutex<Subscriptions>>,
    HashMap<String, UnboundedSender<MessageToWorker>>,
);
pub type RemovedPluginAssets = HashMap<PluginKey, PluginAssets>;

#[derive(Clone, Debug)]
pub struct PluginMetadata {
    pub plugin_config: PluginConfig,
    pub tab_index: Option<usize>,
    pub rows: usize,
    pub columns: usize,
    pub cwd: PathBuf,
    pub is_background: bool,
}

#[derive(Default)]
pub struct PluginMap {
    plugin_assets: HashMap<PluginKey, PluginAssets>,
    plugin_metadata: HashMap<PluginId, PluginMetadata>,
    shared_assets: HashMap<PluginId, PluginAssets>,
}

impl PluginMap {
    pub fn remove_plugins(&mut self, pid: PluginId) -> RemovedPluginAssets {
        self.plugin_metadata.remove(&pid);
        let mut removed = HashMap::new();
        let ids_in_plugin_map: Vec<PluginKey> = self.plugin_assets.keys().copied().collect();
        for (plugin_id, client_id) in ids_in_plugin_map {
            if pid == plugin_id {
                if let Some(plugin_asset) = self.plugin_assets.remove(&(plugin_id, client_id)) {
                    removed.insert((plugin_id, client_id), plugin_asset);
                }
            }
        }
        removed
    }
    pub fn remove_plugin_clients(&mut self, client_id: ClientId) -> RemovedPluginAssets {
        let mut removed = HashMap::new();
        let ids_in_plugin_map: Vec<PluginKey> = self.plugin_assets.keys().copied().collect();
        for key in ids_in_plugin_map {
            if key.1 == client_id {
                if let Some(plugin_asset) = self.plugin_assets.remove(&key) {
                    removed.insert(key, plugin_asset);
                }
            }
        }
        removed
    }
    pub fn insert_shared(&mut self, instance_id: PluginId, assets: PluginAssets) {
        self.shared_assets.insert(instance_id, assets);
    }
    pub fn remove_shared(&mut self, instance_id: PluginId) -> Option<PluginAssets> {
        self.shared_assets.remove(&instance_id)
    }
    pub fn shared_running_plugin(
        &self,
        instance_id: PluginId,
    ) -> Option<(Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>)> {
        self.shared_assets
            .get(&instance_id)
            .map(|(running_plugin, subscriptions, _)| (running_plugin.clone(), subscriptions.clone()))
    }
    pub fn remove_metadata(&mut self, plugin_id: PluginId) {
        self.plugin_metadata.remove(&plugin_id);
    }
    pub fn insert_metadata(&mut self, plugin_id: PluginId, metadata: PluginMetadata) {
        self.plugin_metadata.insert(plugin_id, metadata);
    }
    pub fn metadata(&self, plugin_id: PluginId) -> Option<&PluginMetadata> {
        self.plugin_metadata.get(&plugin_id)
    }
    pub fn set_tab_index(&mut self, plugin_id: PluginId, tab_index: usize) {
        if let Some(metadata) = self.plugin_metadata.get_mut(&plugin_id) {
            if !metadata.is_background {
                metadata.tab_index = Some(tab_index);
            }
        }
    }
    pub fn set_size(&mut self, plugin_id: PluginId, rows: usize, columns: usize) {
        if let Some(metadata) = self.plugin_metadata.get_mut(&plugin_id) {
            metadata.rows = rows;
            metadata.columns = columns;
        }
    }
    pub fn set_cwd(&mut self, plugin_id: PluginId, cwd: PathBuf) {
        if let Some(metadata) = self.plugin_metadata.get_mut(&plugin_id) {
            metadata.cwd = cwd;
        }
    }
    pub fn plugin_ids(&self) -> Vec<PluginId> {
        self.plugin_metadata.keys().copied().collect()
    }
    pub fn running_plugins(&mut self) -> Vec<(PluginId, ClientId, Arc<Mutex<RunningPlugin>>)> {
        self.plugin_assets
            .iter()
            .map(|((plugin_id, client_id), (running_plugin, _, _))| {
                (*plugin_id, *client_id, running_plugin.clone())
            })
            .collect()
    }
    pub fn running_plugins_and_subscriptions(
        &mut self,
    ) -> Vec<(
        PluginId,
        ClientId,
        Arc<Mutex<RunningPlugin>>,
        Arc<Mutex<Subscriptions>>,
    )> {
        self.plugin_assets
            .iter()
            .map(
                |((plugin_id, client_id), (running_plugin, subscriptions, _))| {
                    (
                        *plugin_id,
                        *client_id,
                        running_plugin.clone(),
                        subscriptions.clone(),
                    )
                },
            )
            .collect()
    }
    pub fn get_running_plugin_and_subscriptions(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
    ) -> Option<(Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>)> {
        self.plugin_assets.get(&(plugin_id, client_id)).and_then(
            |(running_plugin, subscriptions, _)| {
                Some((running_plugin.clone(), subscriptions.clone()))
            },
        )
    }
    pub fn get_running_plugin(
        &self,
        plugin_id: PluginId,
        client_id: Option<ClientId>,
    ) -> Option<Arc<Mutex<RunningPlugin>>> {
        match client_id {
            Some(client_id) => self
                .plugin_assets
                .get(&(plugin_id, client_id))
                .and_then(|(running_plugin, _, _)| Some(running_plugin.clone())),
            None => self
                .plugin_assets
                .iter()
                .find(|((p_id, _), _)| *p_id == plugin_id)
                .and_then(|(_, (running_plugin, _, _))| Some(running_plugin.clone())),
        }
    }
    pub fn worker_sender(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
        worker_name: &str,
    ) -> Option<UnboundedSender<MessageToWorker>> {
        if let Some((_, _, workers)) = self.shared_assets.get(&plugin_id) {
            return workers.get(&format!("{}_worker", worker_name)).cloned();
        }
        self.plugin_assets
            .iter()
            .find(|((p_id, c_id), _)| p_id == &plugin_id && c_id == &client_id)
            .and_then(|(_, (_running_plugin, _subscriptions, workers))| {
                if let Some(worker) = workers.get(&format!("{}_worker", worker_name)) {
                    Some(worker.clone())
                } else {
                    None
                }
            })
            .clone()
    }
    pub fn all_plugin_ids_for_plugin_location(
        &self,
        plugin_location: &RunPluginLocation,
        plugin_configuration: &PluginUserConfiguration,
    ) -> Result<Vec<PluginId>> {
        let err_context = || format!("Failed to get plugin ids for location {plugin_location}");
        let plugin_ids: Vec<PluginId> = self
            .plugin_metadata
            .iter()
            .filter(|(_plugin_id, metadata)| {
                &metadata.plugin_config.location == plugin_location
                    && &metadata.plugin_config.initial_userspace_configuration
                        == plugin_configuration
            })
            .map(|(plugin_id, _metadata)| *plugin_id)
            .collect();
        if plugin_ids.is_empty() {
            return Err(ZellijError::PluginDoesNotExist).with_context(err_context);
        }
        Ok(plugin_ids)
    }
    pub fn clone_plugin_assets(
        &self,
    ) -> HashMap<RunPluginLocation, HashMap<PluginUserConfiguration, Vec<(PluginId, ClientId)>>>
    {
        let mut cloned_plugin_assets: HashMap<
            RunPluginLocation,
            HashMap<PluginUserConfiguration, Vec<(PluginId, ClientId)>>,
        > = HashMap::new();
        for ((plugin_id, client_id), (running_plugin, _, _)) in self.plugin_assets.iter() {
            let running_plugin = running_plugin.lock().unwrap();
            let plugin_config = &running_plugin.store.data().plugin;
            let running_plugin_location = &plugin_config.location;
            let running_plugin_configuration = &plugin_config.initial_userspace_configuration;
            match cloned_plugin_assets.get_mut(running_plugin_location) {
                Some(location_map) => match location_map.get_mut(running_plugin_configuration) {
                    Some(plugin_instances_info) => {
                        plugin_instances_info.push((*plugin_id, *client_id));
                    },
                    None => {
                        location_map.insert(
                            running_plugin_configuration.clone(),
                            vec![(*plugin_id, *client_id)],
                        );
                    },
                },
                None => {
                    let mut location_map = HashMap::new();
                    location_map.insert(
                        running_plugin_configuration.clone(),
                        vec![(*plugin_id, *client_id)],
                    );
                    cloned_plugin_assets.insert(running_plugin_location.clone(), location_map);
                },
            }
        }
        cloned_plugin_assets
    }
    pub fn contains(&self, plugin_id: PluginId, client_id: ClientId) -> bool {
        self.plugin_assets.contains_key(&(plugin_id, client_id))
    }
    pub fn all_plugin_ids(&self) -> Vec<(PluginId, ClientId)> {
        self.plugin_assets
            .iter()
            .map(|((plugin_id, client_id), _)| (*plugin_id, *client_id))
            .collect()
    }
    pub fn insert(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        running_plugin: Arc<Mutex<RunningPlugin>>,
        subscriptions: Arc<Mutex<Subscriptions>>,
        running_workers: HashMap<String, UnboundedSender<MessageToWorker>>,
    ) {
        self.plugin_assets.insert(
            (plugin_id, client_id),
            (running_plugin, subscriptions, running_workers),
        );
    }
    pub fn run_plugin_of_plugin_id(&self, plugin_id: PluginId) -> Option<RunPlugin> {
        self.plugin_metadata
            .get(&plugin_id)
            .map(|metadata| RunPlugin {
                _allow_exec_host_cmd: false,
                location: metadata.plugin_config.location.clone(),
                configuration: metadata
                    .plugin_config
                    .initial_userspace_configuration
                    .clone(),
                initial_cwd: metadata.plugin_config.initial_cwd.clone(),
            })
    }
    pub fn list_plugins(&self) -> BTreeMap<PluginId, RunPlugin> {
        let mut plugin_ids_to_cmds: BTreeMap<u32, RunPlugin> = BTreeMap::new();
        for plugin_id in self.plugin_metadata.keys() {
            let plugin_cmd = self.run_plugin_of_plugin_id(*plugin_id);
            match plugin_cmd {
                Some(plugin_cmd) => {
                    plugin_ids_to_cmds.insert(*plugin_id, plugin_cmd.clone());
                },
                None => log::error!("Plugin with id: {plugin_id} not found"),
            }
        }
        plugin_ids_to_cmds
    }
}

pub type Subscriptions = HashSet<EventType>;

pub struct PluginEnv {
    pub plugin_id: PluginId,
    pub plugin: PluginConfig,
    pub permissions: Arc<Mutex<Option<HashSet<PermissionType>>>>,
    pub senders: ThreadSenders,
    pub wasi_ctx: WasiCtx,
    pub client_id: ClientId,
    #[allow(dead_code)]
    pub plugin_own_data_dir: PathBuf,
    pub plugin_own_cache_dir: PathBuf,
    pub path_to_default_shell: PathBuf,
    pub default_shell: Option<TerminalAction>,
    pub layout_dir: Option<PathBuf>,
    pub plugin_cwd: PathBuf,
    pub session_env_vars: std::collections::BTreeMap<String, String>,
    pub input_pipes_to_unblock: Arc<Mutex<HashSet<String>>>,
    pub input_pipes_to_block: Arc<Mutex<HashSet<String>>>,
    pub default_mode: InputMode,
    pub subscriptions: Arc<Mutex<Subscriptions>>,
    pub stdin_pipe: Arc<Mutex<VecDeque<u8>>>,
    pub stdout_pipe: Arc<Mutex<VecDeque<u8>>>,
    pub keybinds: SharedKeybinds,
    pub intercepting_key_presses: bool,
    pub store_limits: StoreLimits,
    pub shared: Option<SharedEnv>,
}

#[derive(Debug, Clone)]
pub struct SharedSlot {
    pub kind: SlotKind,
    pub configuration: BTreeMap<String, String>,
    pub rows: usize,
    pub columns: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SharedEnv {
    pub current_slot: Option<PluginId>,
    pub current_client: Option<ClientId>,
    pub self_commands_allowed: bool,
    pub slots: BTreeMap<PluginId, SharedSlot>,
    pub clients: BTreeSet<ClientId>,
    pub visible_slots: HashMap<ClientId, HashSet<PluginId>>,
    pub last_events: HashMap<(EventType, Option<ClientId>), (Event, Option<SharedKeybinds>)>,
}

impl SharedEnv {
    pub fn first_pane_slot(&self) -> Option<PluginId> {
        self.slots
            .iter()
            .find(|(_, slot)| slot.kind == SlotKind::Pane)
            .map(|(id, _)| *id)
    }
    pub fn pane_slot_ids(&self) -> Vec<PluginId> {
        self.slots
            .iter()
            .filter(|(_, slot)| slot.kind == SlotKind::Pane)
            .map(|(id, _)| *id)
            .collect()
    }
    pub fn client_can_see(&self, client_id: ClientId, slot_id: PluginId) -> bool {
        match self.visible_slots.get(&client_id) {
            Some(visible) => visible.contains(&slot_id),
            None => true,
        }
    }
    pub fn renderable_slot(&self, slot_id: PluginId) -> Option<(usize, usize)> {
        self.slots.get(&slot_id).and_then(|slot| {
            if slot.kind == SlotKind::Pane && slot.rows > 0 && slot.columns > 0 {
                Some((slot.rows, slot.columns))
            } else {
                None
            }
        })
    }
    pub fn render_targets(&self, render: &RenderResponse) -> Vec<(PluginId, ClientId)> {
        let mut targets = vec![];
        for client_id in self.clients.iter() {
            for slot_id in self.slots.keys() {
                let wanted = match render {
                    RenderResponse::Nothing => false,
                    RenderResponse::All => true,
                    RenderResponse::Client(c) => c == client_id,
                    RenderResponse::Slots(slot_ids) => slot_ids.contains(slot_id),
                };
                if wanted
                    && self.renderable_slot(*slot_id).is_some()
                    && self.client_can_see(*client_id, *slot_id)
                {
                    targets.push((*slot_id, *client_id));
                }
            }
        }
        targets
    }
}

#[derive(Clone)]
pub struct VecDequeInputStream(pub Arc<Mutex<VecDeque<u8>>>);

impl std::io::Read for VecDequeInputStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut inner = self.0.lock().unwrap();
        let len = std::cmp::min(buf.len(), inner.len());
        for (i, byte) in inner.drain(0..len).enumerate() {
            buf[i] = byte;
        }
        Ok(len)
    }
}

pub struct WriteOutputStream<T>(pub Arc<Mutex<T>>);

impl<T> Clone for WriteOutputStream<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: Write + Send + 'static> std::io::Write for WriteOutputStream<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut inner = self.0.lock().unwrap();
        inner.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut inner = self.0.lock().unwrap();
        inner.flush()
    }
}

impl PluginEnv {
    // Get the name (path) of the containing plugin
    pub fn name(&self) -> String {
        format!(
            "{} (ID {})",
            self.plugin.path.display().to_string(),
            self.plugin_id
        )
    }

    pub fn set_permissions(&mut self, permissions: HashSet<PermissionType>) {
        self.permissions.lock().unwrap().replace(permissions);
    }
}

#[derive(Eq, PartialEq, Hash)]
pub enum AtomicEvent {
    Resize,
}

pub struct RunningPlugin {
    pub store: Store<PluginEnv>,
    pub instance: Instance,
    pub rows: usize,
    pub columns: usize,
    next_event_ids: HashMap<AtomicEvent, usize>,
    last_applied_event_ids: HashMap<AtomicEvent, usize>,
}

impl RunningPlugin {
    pub fn new(store: Store<PluginEnv>, instance: Instance, rows: usize, columns: usize) -> Self {
        RunningPlugin {
            store,
            instance,
            rows,
            columns,
            next_event_ids: HashMap::new(),
            last_applied_event_ids: HashMap::new(),
        }
    }
    pub fn next_event_id(&mut self, atomic_event: AtomicEvent) -> usize {
        let current_event_id = *self.next_event_ids.get(&atomic_event).unwrap_or(&0);
        if current_event_id < usize::MAX {
            let next_event_id = current_event_id + 1;
            self.next_event_ids.insert(atomic_event, next_event_id);
            current_event_id
        } else {
            let current_event_id = 0;
            let next_event_id = 1;
            self.last_applied_event_ids.remove(&atomic_event);
            self.next_event_ids.insert(atomic_event, next_event_id);
            current_event_id
        }
    }
    pub fn apply_event_id(&mut self, atomic_event: AtomicEvent, event_id: usize) -> bool {
        if &event_id >= self.last_applied_event_ids.get(&atomic_event).unwrap_or(&0) {
            self.last_applied_event_ids.insert(atomic_event, event_id);
            true
        } else {
            false
        }
    }
    pub fn update_keybinds(&mut self, keybinds: SharedKeybinds) {
        self.store.data_mut().keybinds = keybinds;
    }
    pub fn update_default_mode(&mut self, default_mode: InputMode) {
        self.store.data_mut().default_mode = default_mode;
    }
    pub fn update_default_shell(&mut self, default_shell: Option<TerminalAction>) {
        self.store.data_mut().default_shell = default_shell;
    }
    pub fn update_layout_dir(&mut self, layout_dir: Option<PathBuf>) {
        self.store.data_mut().layout_dir = layout_dir;
    }
    pub fn intercepting_key_presses(&self) -> bool {
        self.store.data().intercepting_key_presses
    }
}
