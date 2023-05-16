use crate::plugins::plugin_loader::{PluginLoader, VersionMismatchError};
use crate::plugins::zellij_exports::wasi_write_object;
use crate::plugins::PluginId;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmer::Instance;
use wasmer_wasi::WasiEnv;

use crate::{thread_bus::ThreadSenders, ClientId};

use zellij_utils::errors::prelude::*;
use zellij_utils::{
    consts::VERSION, data::EventType, input::layout::RunPluginLocation,
    input::plugins::PluginConfig,
};

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
            HashMap<String, Arc<Mutex<RunningWorker>>>,
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
        HashMap<String, Arc<Mutex<RunningWorker>>>,
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
        HashMap<String, Arc<Mutex<RunningWorker>>>,
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
    pub fn clone_worker(
        &self,
        plugin_id: PluginId,
        client_id: ClientId,
        worker_name: &str,
    ) -> Option<Arc<Mutex<RunningWorker>>> {
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
    ) -> Result<Vec<PluginId>> {
        let err_context = || format!("Failed to get plugin ids for location {plugin_location}");
        let plugin_ids: Vec<PluginId> = self
            .plugin_assets
            .iter()
            .filter(|(_, (running_plugin, _subscriptions, _workers))| {
                &running_plugin.lock().unwrap().plugin_env.plugin.location == plugin_location
            })
            .map(|((plugin_id, _client_id), _)| *plugin_id)
            .collect();
        if plugin_ids.is_empty() {
            return Err(ZellijError::PluginDoesNotExist).with_context(err_context);
        }
        Ok(plugin_ids)
    }
    pub fn insert(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        running_plugin: Arc<Mutex<RunningPlugin>>,
        subscriptions: Arc<Mutex<Subscriptions>>,
        running_workers: HashMap<String, Arc<Mutex<RunningWorker>>>,
    ) {
        self.plugin_assets.insert(
            (plugin_id, client_id),
            (running_plugin, subscriptions, running_workers),
        );
    }
}

pub type Subscriptions = HashSet<EventType>;

#[derive(Clone)]
pub struct PluginEnv {
    pub plugin_id: PluginId,
    pub plugin: PluginConfig,
    pub senders: ThreadSenders,
    pub wasi_env: WasiEnv,
    pub tab_index: usize,
    pub client_id: ClientId,
    #[allow(dead_code)]
    pub plugin_own_data_dir: PathBuf,
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
}

#[derive(Eq, PartialEq, Hash)]
pub enum AtomicEvent {
    Resize,
}

pub struct RunningPlugin {
    pub instance: Instance,
    pub plugin_env: PluginEnv,
    pub rows: usize,
    pub columns: usize,
    next_event_ids: HashMap<AtomicEvent, usize>,
    last_applied_event_ids: HashMap<AtomicEvent, usize>,
}

impl RunningPlugin {
    pub fn new(instance: Instance, plugin_env: PluginEnv, rows: usize, columns: usize) -> Self {
        RunningPlugin {
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

pub struct RunningWorker {
    pub instance: Instance,
    pub name: String,
    pub plugin_config: PluginConfig,
    pub plugin_env: PluginEnv,
}

impl RunningWorker {
    pub fn new(
        instance: Instance,
        name: &str,
        plugin_config: PluginConfig,
        plugin_env: PluginEnv,
    ) -> Self {
        RunningWorker {
            instance,
            name: name.into(),
            plugin_config,
            plugin_env,
        }
    }
    pub fn send_message(&self, message: String, payload: String) -> Result<()> {
        let err_context = || format!("Failed to send message to worker");

        let work_function = self
            .instance
            .exports
            .get_function(&self.name)
            .with_context(err_context)?;
        wasi_write_object(&self.plugin_env.wasi_env, &(message, payload))
            .with_context(err_context)?;
        work_function.call(&[]).or_else::<anyError, _>(|e| {
            match e.downcast::<serde_json::Error>() {
                Ok(_) => panic!(
                    "{}",
                    anyError::new(VersionMismatchError::new(
                        VERSION,
                        "Unavailable",
                        &self.plugin_config.path,
                        self.plugin_config.is_builtin(),
                    ))
                ),
                Err(e) => Err(e).with_context(err_context),
            }
        })?;

        Ok(())
    }
}
