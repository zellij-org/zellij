use crate::plugins::PluginId;
use crate::plugins::plugin_loader::{PluginLoader, VersionMismatchError};
use crate::plugins::zellij_exports::{wasi_write_object};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmer::Instance;
use wasmer_wasi::WasiEnv;

use crate::{thread_bus::ThreadSenders, ClientId};

use zellij_utils::{
    consts::VERSION,
    data::EventType, input::plugins::PluginConfig
};
use zellij_utils::errors::prelude::*;

// the idea here is to provide atomicity when adding/removing plugins from the map (eg. when a new
// client connects) but to also allow updates/renders not to block each other
// so when adding/removing from the map - everything is halted, that's life
// but when cloning the internal RunningPlugin and Subscriptions atomics, we can call methods on
// them without blocking other instances
pub type PluginMap =
    HashMap<(PluginId, ClientId), (Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>, HashMap<String, Arc<Mutex<RunningWorker>>>)>;
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

// TODO: CONTINUE HERE (30/04) - implement this and then populate it for Strider and then get the message
// to pass
pub struct RunningWorker {
    pub instance: Instance,
    pub name: String,
    pub plugin_config: PluginConfig,
    pub plugin_env: PluginEnv,
//     pub plugin_env: PluginEnv,
//     pub rows: usize,
//     pub columns: usize,
//     next_event_ids: HashMap<AtomicEvent, usize>,
//     last_applied_event_ids: HashMap<AtomicEvent, usize>,
}

impl RunningWorker {
    pub fn new(instance: Instance, name: &str, plugin_config: PluginConfig, plugin_env: PluginEnv) -> Self {
        RunningWorker {
            instance,
            name: name.into(),
            plugin_config,
            plugin_env,
        }
    }
    pub fn send_message(&self, message: String, payload: String) -> Result<()> {
        let err_context = || format!("Failed to send message to worker");

        let work_function = self.instance
            .exports
            .get_function(&self.name)
            .with_context(err_context)?;
        wasi_write_object(&self.plugin_env.wasi_env, &(message, payload)).with_context(err_context)?;
        work_function
            .call(&[])
            .or_else::<anyError, _>(|e| match e.downcast::<serde_json::Error>() {
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
            })?;

        Ok(())
    }
}
