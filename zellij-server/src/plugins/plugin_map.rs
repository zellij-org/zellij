use crate::plugins::wasm_bridge::PluginId;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmer::Instance;
use wasmer_wasi::WasiEnv;

use crate::{thread_bus::ThreadSenders, ClientId};

use zellij_utils::{data::EventType, input::plugins::PluginConfig};

// the idea here is to provide atomicity when adding/removing plugins from the map (eg. when a new
// client connects) but to also allow updates/renders not to block each other
// so when adding/removing from the map - everything is halted, that's life
// but when cloning the internal RunningPlugin and Subscriptions atomics, we can call methods on
// them without blocking other instances
pub type PluginMap =
    HashMap<(PluginId, ClientId), (Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>)>;
pub type Subscriptions = HashSet<EventType>;

#[derive(Clone)]
pub struct PluginEnv {
    pub plugin_id: u32,
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
        // TODO: probably not usize...
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
