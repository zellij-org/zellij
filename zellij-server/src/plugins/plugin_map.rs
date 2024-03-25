use crate::plugins::plugin_worker::MessageToWorker;
use crate::plugins::PluginId;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmer::{Instance, Store};
use wasmer_wasi::WasiEnv;

use crate::{thread_bus::ThreadSenders, ClientId};

use zellij_utils::async_channel::Sender;
use zellij_utils::{
    data::EventType,
    data::PluginCapabilities,
    input::command::TerminalAction,
    input::layout::{Layout, PluginUserConfiguration, RunPlugin, RunPluginLocation},
    input::plugins::PluginConfig,
    ipc::ClientAttributes,
};
use zellij_utils::{data::PermissionType, errors::prelude::*};

// the idea here is to provide atomicity when adding/removing plugins from the map (eg. when a new
// client connects) but to also allow updates/renders not to block each other
// so when adding/removing from the map - everything is halted, that's life
// but when cloning the internal RunningPlugin and Subscriptions atomics, we can call methods on
// them without blocking other instances
#[derive(Default)]
pub struct PluginMap {
    plugin_assets: HashMap<
        (PluginId, ClientId),
        (
            Arc<Mutex<RunningPlugin>>,
            Arc<Mutex<Subscriptions>>,
            HashMap<String, Sender<MessageToWorker>>,
        ),
    >,
}

impl PluginMap {
    pub fn remove_plugins(
        &mut self,
        pid: PluginId,
    ) -> Vec<(
        Arc<Mutex<RunningPlugin>>,
        Arc<Mutex<Subscriptions>>,
        HashMap<String, Sender<MessageToWorker>>,
    )> {
        let mut removed = vec![];
        let ids_in_plugin_map: Vec<(PluginId, ClientId)> =
            self.plugin_assets.keys().copied().collect();
        for (plugin_id, client_id) in ids_in_plugin_map {
            if pid == plugin_id {
                if let Some(plugin_asset) = self.plugin_assets.remove(&(plugin_id, client_id)) {
                    removed.push(plugin_asset);
                }
            }
        }
        removed
    }
    pub fn remove_single_plugin(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
    ) -> Option<(
        Arc<Mutex<RunningPlugin>>,
        Arc<Mutex<Subscriptions>>,
        HashMap<String, Sender<MessageToWorker>>,
    )> {
        self.plugin_assets.remove(&(plugin_id, client_id))
    }
    pub fn plugin_ids(&self) -> Vec<PluginId> {
        let mut unique_plugins: HashSet<PluginId> = self
            .plugin_assets
            .keys()
            .map(|(plugin_id, _client_id)| *plugin_id)
            .collect();
        unique_plugins.drain().into_iter().collect()
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
    ) -> Option<Sender<MessageToWorker>> {
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
            .plugin_assets
            .iter()
            .filter(|(_, (running_plugin, _subscriptions, _workers))| {
                let running_plugin = running_plugin.lock().unwrap();
                let running_plugin_location = &running_plugin.plugin_env.plugin.location;
                let running_plugin_configuration =
                    &running_plugin.plugin_env.plugin.userspace_configuration;
                running_plugin_location == plugin_location
                    && running_plugin_configuration == plugin_configuration
            })
            .map(|((plugin_id, _client_id), _)| *plugin_id)
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
            let running_plugin_location = &running_plugin.plugin_env.plugin.location;
            let running_plugin_configuration =
                &running_plugin.plugin_env.plugin.userspace_configuration;
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
        running_workers: HashMap<String, Sender<MessageToWorker>>,
    ) {
        self.plugin_assets.insert(
            (plugin_id, client_id),
            (running_plugin, subscriptions, running_workers),
        );
    }
    pub fn run_plugin_of_plugin_id(&self, plugin_id: PluginId) -> Option<RunPlugin> {
        self.plugin_assets
            .iter()
            .find_map(|((p_id, _), (running_plugin, _, _))| {
                if *p_id == plugin_id {
                    let running_plugin = running_plugin.lock().unwrap();
                    let run_plugin_location = running_plugin.plugin_env.plugin.location.clone();
                    let run_plugin_configuration = running_plugin
                        .plugin_env
                        .plugin
                        .userspace_configuration
                        .clone();
                    let initial_cwd = running_plugin.plugin_env.plugin.initial_cwd.clone();
                    Some(RunPlugin {
                        _allow_exec_host_cmd: false,
                        location: run_plugin_location,
                        configuration: run_plugin_configuration,
                        initial_cwd,
                    })
                } else {
                    None
                }
            })
    }
}

pub type Subscriptions = HashSet<EventType>;

#[derive(Clone)]
pub struct PluginEnv {
    pub plugin_id: PluginId,
    pub plugin: PluginConfig,
    pub permissions: Arc<Mutex<Option<HashSet<PermissionType>>>>,
    pub senders: ThreadSenders,
    pub wasi_env: WasiEnv,
    pub tab_index: Option<usize>,
    pub client_id: ClientId,
    #[allow(dead_code)]
    pub plugin_own_data_dir: PathBuf,
    pub path_to_default_shell: PathBuf,
    pub capabilities: PluginCapabilities,
    pub client_attributes: ClientAttributes,
    pub default_shell: Option<TerminalAction>,
    pub default_layout: Box<Layout>,
    pub plugin_cwd: PathBuf,
    pub input_pipes_to_unblock: Arc<Mutex<HashSet<String>>>,
    pub input_pipes_to_block: Arc<Mutex<HashSet<String>>>,
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
    pub store: Store,
    pub instance: Instance,
    pub plugin_env: PluginEnv,
    pub rows: usize,
    pub columns: usize,
    next_event_ids: HashMap<AtomicEvent, usize>,
    last_applied_event_ids: HashMap<AtomicEvent, usize>,
}

impl RunningPlugin {
    pub fn new(
        store: Store,
        instance: Instance,
        plugin_env: PluginEnv,
        rows: usize,
        columns: usize,
    ) -> Self {
        RunningPlugin {
            store,
            instance,
            plugin_env,
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
}
