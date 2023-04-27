use super::PluginInstruction;
use crate::plugins::plugin_loader::{PluginLoader, VersionMismatchError};
use log::{debug, info, warn};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt::Display,
    path::PathBuf,
    process,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wasmer::{
    imports, Function, ImportObject, Instance, Module, Store, Value,
    WasmerEnv,
};
use wasmer_wasi::WasiEnv;
use zellij_utils::async_std::task::{self, JoinHandle};

use crate::{
    background_jobs::BackgroundJob,
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication,
    ClientId,
};

use zellij_utils::{
    consts::VERSION,
    data::{Event, EventType, PluginIds},
    errors::prelude::*,
    errors::ZellijError,
    input::{
        command::TerminalAction,
        layout::{RunPlugin, RunPluginLocation},
        plugins::{PluginConfig, PluginType, PluginsConfig},
    },
    pane_size::Size,
    serde,
};

type PluginId = u32;

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
    next_event_ids: HashMap<AtomicEvent, usize>, // TODO: probably not usize
    last_applied_event_ids: HashMap<AtomicEvent, usize>, // TODO: probably not usize
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
    pub fn next_event_id(&mut self, atomic_event: AtomicEvent) -> usize { // TODO: probably not usize...
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

// the idea here is to provide atomicity when adding/removing plugins from the map (eg. when a new
// client connects) but to also allow updates/renders not to block each other
// so when adding/removing from the map - everything is halted, that's life
// but when cloning the internal RunningPlugin and Subscriptions atomics, we can call methods on
// them without blocking other instances
pub type PluginMap = HashMap<(PluginId, ClientId), (Arc<Mutex<RunningPlugin>>, Arc<Mutex<Subscriptions>>)>;
pub type Subscriptions = HashSet<EventType>;

pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugins: PluginsConfig,
    senders: ThreadSenders,
    store: Store,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    next_plugin_id: u32,
    cached_events_for_pending_plugins: HashMap<u32, Vec<Event>>, // u32 is the plugin id
    cached_resizes_for_pending_plugins: HashMap<u32, (usize, usize)>, // (rows, columns)
    loading_plugins: HashMap<(u32, RunPlugin), JoinHandle<()>>,  // plugin_id to join-handle
    pending_plugin_reloads: HashSet<RunPlugin>,
}

impl WasmBridge {
    pub fn new(
        plugins: PluginsConfig,
        senders: ThreadSenders,
        store: Store,
        plugin_dir: PathBuf,
    ) -> Self {
        let plugin_map = Arc::new(Mutex::new(HashMap::new()));
        let connected_clients: Arc<Mutex<Vec<ClientId>>> = Arc::new(Mutex::new(vec![]));
        let plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>> =
            Arc::new(Mutex::new(HashMap::new()));
        WasmBridge {
            connected_clients,
            plugins,
            senders,
            store,
            plugin_dir,
            plugin_cache,
            plugin_map,
            next_plugin_id: 0,
            cached_events_for_pending_plugins: HashMap::new(),
            cached_resizes_for_pending_plugins: HashMap::new(),
            loading_plugins: HashMap::new(),
            pending_plugin_reloads: HashSet::new(),
        }
    }
    pub fn load_plugin(
        &mut self,
        run: &RunPlugin,
        tab_index: usize,
        size: Size,
        client_id: Option<ClientId>,
    ) -> Result<u32> {
        // returns the plugin id
        let err_context = move || format!("failed to load plugin");

        let client_id = client_id.or_else(|| {
            self.connected_clients.lock().unwrap().iter().next().copied()
        }).with_context(|| "Plugins must have a client id, none was provided and none are connected")?;

        let plugin_id = self.next_plugin_id;

        let plugin = self
            .plugins
            .get(run)
            .with_context(|| format!("failed to resolve plugin {run:?}"))
            .with_context(err_context)?;
        let plugin_name = run.location.to_string();

        self.cached_events_for_pending_plugins
            .insert(plugin_id, vec![]);
        self.cached_resizes_for_pending_plugins
            .insert(plugin_id, (size.rows, size.cols));

        let load_plugin_task = task::spawn({
            let plugin_dir = self.plugin_dir.clone();
            let plugin_cache = self.plugin_cache.clone();
            let senders = self.senders.clone();
            let store = self.store.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            async move {
                let _ =
                    senders.send_to_background_jobs(BackgroundJob::AnimatePluginLoading(plugin_id));
                let mut loading_indication = LoadingIndication::new(plugin_name.clone());
                match PluginLoader::start_plugin(
                    plugin_id,
                    client_id,
                    &plugin,
                    tab_index,
                    plugin_dir,
                    plugin_cache,
                    senders.clone(),
                    store,
                    plugin_map,
                    size,
                    connected_clients.clone(),
                    &mut loading_indication,
                ) {
                    Ok(_) => handle_plugin_successful_loading(&senders, plugin_id),
                    Err(e) => handle_plugin_loading_failure(
                        &senders,
                        plugin_id,
                        &mut loading_indication,
                        e,
                    ),
                }
                let _ =
                    senders.send_to_plugin(PluginInstruction::ApplyCachedEvents(vec![plugin_id]));
            }
        });
        self.loading_plugins
            .insert((plugin_id, run.clone()), load_plugin_task);
        self.next_plugin_id += 1;
        Ok(plugin_id)
    }
    pub fn unload_plugin(&mut self, pid: u32) -> Result<()> {
        info!("Bye from plugin {}", &pid);
        // TODO: remove plugin's own data directory
        let mut plugin_map = self.plugin_map.lock().unwrap();
        let ids_in_plugin_map: Vec<(u32, ClientId)> = plugin_map.keys().copied().collect();
        for (plugin_id, client_id) in ids_in_plugin_map {
            if pid == plugin_id {
                drop(plugin_map.remove(&(plugin_id, client_id)));
            }
        }
        Ok(())
    }
    pub fn reload_plugin(&mut self, run_plugin: &RunPlugin) -> Result<()> {
        if self.plugin_is_currently_being_loaded(&run_plugin.location) {
            self.pending_plugin_reloads.insert(run_plugin.clone());
            return Ok(());
        }

        let plugin_ids = self.all_plugin_ids_for_plugin_location(&run_plugin.location)?;
        for plugin_id in &plugin_ids {
            let (rows, columns) = self.size_of_plugin_id(*plugin_id).unwrap_or((0, 0));
            self.cached_events_for_pending_plugins
                .insert(*plugin_id, vec![]);
            self.cached_resizes_for_pending_plugins
                .insert(*plugin_id, (rows, columns));
        }

        let first_plugin_id = *plugin_ids.get(0).unwrap(); // this is safe becaise the above
                                                           // methods always returns at least 1 id
        let mut loading_indication = LoadingIndication::new(run_plugin.location.to_string());
        self.start_plugin_loading_indication(&plugin_ids, &loading_indication);
        let load_plugin_task = task::spawn({
            let plugin_dir = self.plugin_dir.clone();
            let plugin_cache = self.plugin_cache.clone();
            let senders = self.senders.clone();
            let store = self.store.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            async move {
                match PluginLoader::reload_plugin(
                    first_plugin_id,
                    plugin_dir.clone(),
                    plugin_cache.clone(),
                    senders.clone(),
                    store.clone(),
                    plugin_map.clone(),
                    connected_clients.clone(),
                    &mut loading_indication,
                ) {
                    Ok(_) => {
                        handle_plugin_successful_loading(&senders, first_plugin_id);
                        for plugin_id in &plugin_ids {
                            if plugin_id == &first_plugin_id {
                                // no need to reload the plugin we just reloaded
                                continue;
                            }
                            let mut loading_indication = LoadingIndication::new("".into());
                            match PluginLoader::reload_plugin_from_memory(
                                *plugin_id,
                                plugin_dir.clone(),
                                plugin_cache.clone(),
                                senders.clone(),
                                store.clone(),
                                plugin_map.clone(),
                                connected_clients.clone(),
                                &mut loading_indication,
                            ) {
                                Ok(_) => handle_plugin_successful_loading(&senders, *plugin_id),
                                Err(e) => handle_plugin_loading_failure(
                                    &senders,
                                    *plugin_id,
                                    &mut loading_indication,
                                    e,
                                ),
                            }
                        }
                    },
                    Err(e) => {
                        for plugin_id in &plugin_ids {
                            handle_plugin_loading_failure(
                                &senders,
                                *plugin_id,
                                &mut loading_indication,
                                &e,
                            );
                        }
                    },
                }
                let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents(plugin_ids));
            }
        });
        self.loading_plugins
            .insert((first_plugin_id, run_plugin.clone()), load_plugin_task);
        Ok(())
    }
    pub fn add_client(&mut self, client_id: ClientId) -> Result<()> {
        let mut loading_indication = LoadingIndication::new("".into());
        match PluginLoader::add_client(
            client_id,
            self.plugin_dir.clone(),
            self.plugin_cache.clone(),
            self.senders.clone(),
            self.store.clone(),
            self.plugin_map.clone(),
            self.connected_clients.clone(),
            &mut loading_indication,
        ) {
            Ok(_) => {
                let _ = self.senders.send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
                Ok(())
            }
            Err(e) => {
                Err(e)
            }
        }
    }
    pub fn resize_plugin(&mut self, pid: u32, new_columns: usize, new_rows: usize) -> Result<()> {
        let err_context = move || format!("failed to resize plugin {pid}");
        for ((plugin_id, client_id), (running_plugin, _subscriptions)) in self.plugin_map.lock().unwrap().iter_mut() {
            if self
                .cached_resizes_for_pending_plugins
                .contains_key(&plugin_id)
            {
                continue;
            }
            if *plugin_id == pid {
                let event_id = running_plugin.lock().unwrap().next_event_id(AtomicEvent::Resize);
                task::spawn({
                    let senders = self.senders.clone();
                    let running_plugin = running_plugin.clone();
                    let plugin_id = *plugin_id;
                    let client_id = *client_id;
                    async move {
                        let mut running_plugin = running_plugin.lock().unwrap();
                        if running_plugin.apply_event_id(AtomicEvent::Resize, event_id) {
                            running_plugin.rows = new_rows;
                            running_plugin.columns = new_columns;
                            let rendered_bytes = running_plugin.instance
                                .exports
                                .get_function("render")
                                .map_err(anyError::new)
                                .and_then(|render| {
                                    render
                                        .call(&[
                                            Value::I32(running_plugin.rows as i32),
                                            Value::I32(running_plugin.columns as i32),
                                        ])
                                        .map_err(anyError::new)
                                })
                                .and_then(|_| wasi_read_string(&running_plugin.plugin_env.wasi_env))
                                .with_context(err_context);
                            match rendered_bytes {
                                Ok(rendered_bytes) => {
                                    let plugin_bytes = vec![(plugin_id, client_id, rendered_bytes.as_bytes().to_vec())];
                                    senders
                                        .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes)).unwrap();
                                },
                                Err(e) => log::error!("{}", e)
                            }

                        }
                    }
                });
            }
        }
        for (plugin_id, mut current_size) in self.cached_resizes_for_pending_plugins.iter_mut() {
            if *plugin_id == pid {
                current_size.0 = new_rows;
                current_size.1 = new_columns;
            }
        }
        Ok(())
    }
    pub fn update_plugins(
        &mut self,
        mut updates: Vec<(Option<u32>, Option<ClientId>, Event)>,
    ) -> Result<()> {
        let err_context = || "failed to update plugin state".to_string();

        for (pid, cid, event) in updates.drain(..) {
            for (&(plugin_id, client_id), (running_plugin, subscriptions)) in &*self.plugin_map.lock().unwrap() {
                if self
                    .cached_events_for_pending_plugins
                    .contains_key(&plugin_id)
                {
                    continue;
                }
                let subs = subscriptions
                    .lock()
                    .unwrap()
                    .clone();
                // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                let event_type =
                    EventType::from_str(&event.to_string()).with_context(err_context)?;
                if subs.contains(&event_type)
                    && ((pid.is_none() && cid.is_none())
                        || (pid.is_none() && cid == Some(client_id))
                        || (cid.is_none() && pid == Some(plugin_id))
                        || (cid == Some(client_id) && pid == Some(plugin_id)))
                {
                    task::spawn({
                        let senders = self.senders.clone();
                        let running_plugin = running_plugin.clone();
                        let event = event.clone();
                        async move {
                            let running_plugin = running_plugin.lock().unwrap();
                            let mut plugin_bytes = vec![];
                            match apply_event_to_plugin(
                                plugin_id,
                                client_id,
                                &running_plugin.instance,
                                &running_plugin.plugin_env,
                                &event,
                                running_plugin.rows,
                                running_plugin.columns,
                                &mut plugin_bytes,
                            ) {
                                Ok(()) => {
                                    let _ = senders
                                        .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
                                },
                                Err(e) => {
                                    log::error!("{}", e);
                                }
                            }
                        }
                    });
                }
            }
            for (plugin_id, cached_events) in self.cached_events_for_pending_plugins.iter_mut() {
                if pid.is_none() || pid.as_ref() == Some(plugin_id) {
                    cached_events.push(event.clone());
                }
            }
        }
        Ok(())
    }
    pub fn apply_cached_events(&mut self, plugin_ids: Vec<u32>) -> Result<()> {
        let mut applied_plugin_paths = HashSet::new();
        for plugin_id in plugin_ids {
            self.apply_cached_events_and_resizes_for_plugin(plugin_id)?;
            if let Some(run_plugin) = self.run_plugin_of_plugin_id(plugin_id) {
                applied_plugin_paths.insert(run_plugin.clone());
            }
            self.loading_plugins
                .retain(|(p_id, _run_plugin), _| p_id != &plugin_id);
        }
        for run_plugin in applied_plugin_paths.drain() {
            if self.pending_plugin_reloads.remove(&run_plugin) {
                let _ = self.reload_plugin(&run_plugin);
            }
        }
        Ok(())
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.connected_clients
            .lock()
            .unwrap()
            .retain(|c| c != &client_id);
    }
    pub fn cleanup(&mut self) {
        for (_plugin_id, loading_plugin_task) in self.loading_plugins.drain() {
            drop(loading_plugin_task.cancel());
        }
    }
    fn run_plugin_of_plugin_id(&self, plugin_id: PluginId) -> Option<&RunPlugin> {
        self.loading_plugins
            .iter()
            .find(|((p_id, _run_plugin), _)| p_id == &plugin_id)
            .map(|((_p_id, run_plugin), _)| run_plugin)
    }
    fn apply_cached_events_and_resizes_for_plugin(&mut self, plugin_id: PluginId) -> Result<()> {
        let err_context = || format!("Failed to apply cached events to plugin");
        if let Some(events) = self.cached_events_for_pending_plugins.remove(&plugin_id) {
            // let mut plugin_map = self.plugin_map.lock().unwrap();
            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .lock()
                .unwrap()
                .iter()
                .copied()
                .collect();
            for client_id in &all_connected_clients {
                // if let Some((instance, plugin_env, (rows, columns))) =
                if let Some((running_plugin, subscriptions)) =
                    self.plugin_map.lock().unwrap().get_mut(&(plugin_id, *client_id))
                {
                    let subs = subscriptions
                        .lock()
                        .unwrap()
                        .clone();
                    for event in events.clone() {
                        let event_type =
                            EventType::from_str(&event.to_string()).with_context(err_context)?;
                        if !subs.contains(&event_type) {
                            continue;
                        }
                        task::spawn({
                            let senders = self.senders.clone();
                            let running_plugin = running_plugin.clone();
                            let client_id = *client_id;
                            async move {
                                let running_plugin = running_plugin.lock().unwrap();
                                let mut plugin_bytes = vec![];
                                match apply_event_to_plugin(
                                    plugin_id,
                                    client_id,
                                    &running_plugin.instance,
                                    &running_plugin.plugin_env,
                                    &event,
                                    running_plugin.rows,
                                    running_plugin.columns,
                                    &mut plugin_bytes,
                                ) {
                                    Ok(()) => {
                                        let _ = senders
                                            .send_to_screen(ScreenInstruction::PluginBytes(plugin_bytes));
                                    },
                                    Err(e) => {
                                        log::error!("{}", e);
                                    }
                                }
                            }
                        });
                    }
                }
            }
        }
        if let Some((rows, columns)) = self.cached_resizes_for_pending_plugins.remove(&plugin_id) {
            self.resize_plugin(plugin_id, columns, rows)?;
        }
        Ok(())
    }
    fn plugin_is_currently_being_loaded(&self, plugin_location: &RunPluginLocation) -> bool {
        self.loading_plugins
            .iter()
            .find(|((_plugin_id, run_plugin), _)| &run_plugin.location == plugin_location)
            .is_some()
    }
    fn all_plugin_ids_for_plugin_location(
        &self,
        plugin_location: &RunPluginLocation,
    ) -> Result<Vec<PluginId>> {
        let err_context = || format!("Failed to get plugin ids for location {plugin_location}");
        let plugin_ids: Vec<PluginId> = self
            .plugin_map
            .lock()
            .unwrap()
            .iter()
            .filter(
                |(_, (running_plugin, _subscriptions))| {
                    &running_plugin.lock().unwrap().plugin_env.plugin.location == plugin_location // TODO:
                                                                                                  // better
                },
            )
            .map(|((plugin_id, _client_id), _)| *plugin_id)
            .collect();
        if plugin_ids.is_empty() {
            return Err(ZellijError::PluginDoesNotExist).with_context(err_context);
        }
        Ok(plugin_ids)
    }
    fn size_of_plugin_id(&self, plugin_id: PluginId) -> Option<(usize, usize)> {
        // (rows/colums)
        self.plugin_map
            .lock()
            .unwrap()
            .iter()
            .find(|((p_id, _client_id), _running_plugin)| *p_id == plugin_id)
            .map(|((_p_id, _client_id), (running_plugin, _subscriptions))| {
                let running_plugin = running_plugin.lock().unwrap();
                (running_plugin.rows, running_plugin.columns)
            })
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
                ));
            let _ = self
                .senders
                .send_to_background_jobs(BackgroundJob::AnimatePluginLoading(*plugin_id));
        }
    }
}

fn handle_plugin_successful_loading(senders: &ThreadSenders, plugin_id: PluginId) {
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    let _ = senders.send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
}

fn handle_plugin_loading_failure(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    loading_indication: &mut LoadingIndication,
    error: impl Display,
) {
    log::error!("{}", error);
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    loading_indication.indicate_loading_error(error.to_string());
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
}

#[derive(WasmerEnv, Clone)]
pub struct ForeignFunctionEnv {
    pub plugin_env: PluginEnv,
    pub subscriptions: Arc<Mutex<Subscriptions>>,
}

impl ForeignFunctionEnv {
    pub fn new(plugin_env: &PluginEnv, subscriptions: &Arc<Mutex<Subscriptions>>) -> Self {
        ForeignFunctionEnv {
            plugin_env: plugin_env.clone(),
            subscriptions: subscriptions.clone(),
        }
    }
}

pub(crate) fn zellij_exports(store: &Store, plugin_env: &PluginEnv, subscriptions: &Arc<Mutex<Subscriptions>>) -> ImportObject {
    macro_rules! zellij_export {
        ($($host_function:ident),+ $(,)?) => {
            imports! {
                "zellij" => {
                    $(stringify!($host_function) =>
                        Function::new_native_with_env(store, ForeignFunctionEnv::new(plugin_env, subscriptions), $host_function),)+
                }
            }
        }
    }

    zellij_export! {
        host_subscribe,
        host_unsubscribe,
        host_set_selectable,
        host_get_plugin_ids,
        host_get_zellij_version,
        host_open_file,
        host_switch_tab_to,
        host_set_timeout,
        host_exec_cmd,
        host_report_panic,
    }
}

fn host_subscribe(env: &ForeignFunctionEnv) {
    wasi_read_object::<HashSet<EventType>>(&env.plugin_env.wasi_env)
        .and_then(|new| {
            env.subscriptions.lock().to_anyhow()?.extend(new);
            Ok(())
        })
        .with_context(|| format!("failed to subscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_unsubscribe(env: &ForeignFunctionEnv) {
    wasi_read_object::<HashSet<EventType>>(&env.plugin_env.wasi_env)
        .and_then(|old| {
                env
                .subscriptions
                .lock()
                .to_anyhow()?
                .retain(|k| !old.contains(k));
            Ok(())
        })
        .with_context(|| format!("failed to unsubscribe for plugin {}", env.plugin_env.name()))
        .fatal();
}

fn host_set_selectable(env: &ForeignFunctionEnv, selectable: i32) {
    match env.plugin_env.plugin.run {
        PluginType::Pane(Some(tab_index)) => {
            let selectable = selectable != 0;
            env
                .plugin_env
                .senders
                .send_to_screen(ScreenInstruction::SetSelectable(
                    PaneId::Plugin(env.plugin_env.plugin_id),
                    selectable,
                    tab_index,
                ))
                .with_context(|| {
                    format!(
                        "failed to set plugin {} selectable from plugin {}",
                        selectable,
                        env.plugin_env.name()
                    )
                })
                .non_fatal();
        },
        _ => {
            debug!(
                "{} - Calling method 'host_set_selectable' does nothing for headless plugins",
                env.plugin_env.plugin.location
            )
        },
    }
}

fn host_get_plugin_ids(env: &ForeignFunctionEnv) {
    let ids = PluginIds {
        plugin_id: env.plugin_env.plugin_id,
        zellij_pid: process::id(),
    };
    wasi_write_object(&env.plugin_env.wasi_env, &ids)
        .with_context(|| {
            format!(
                "failed to query plugin IDs from host for plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_get_zellij_version(env: &ForeignFunctionEnv) {
    wasi_write_object(&env.plugin_env.wasi_env, VERSION)
        .with_context(|| {
            format!(
                "failed to request zellij version from host for plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_open_file(env: &ForeignFunctionEnv) {
    wasi_read_object::<PathBuf>(&env.plugin_env.wasi_env)
        .and_then(|path| {
            env
                .plugin_env
                .senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    Some(TerminalAction::OpenFile(path, None, None)),
                    None,
                    None,
                    ClientOrTabIndex::TabIndex(env.plugin_env.tab_index),
                ))
        })
        .with_context(|| {
            format!(
                "failed to open file on host from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_switch_tab_to(env: &ForeignFunctionEnv, tab_idx: u32) {
    env
        .plugin_env
        .senders
        .send_to_screen(ScreenInstruction::GoToTab(
            tab_idx,
            Some(env.plugin_env.client_id),
        ))
        .with_context(|| {
            format!(
                "failed to switch host to tab {tab_idx} from plugin {}",
                env.plugin_env.name()
            )
        })
        .non_fatal();
}

fn host_set_timeout(env: &ForeignFunctionEnv, secs: f64) {
    // There is a fancy, high-performance way to do this with zero additional threads:
    // If the plugin thread keeps a BinaryHeap of timer structs, it can manage multiple and easily `.peek()` at the
    // next time to trigger in O(1) time. Once the wake-up time is known, the `wasm` thread can use `recv_timeout()`
    // to wait for an event with the timeout set to be the time of the next wake up. If events come in in the meantime,
    // they are handled, but if the timeout triggers, we replace the event from `recv()` with an
    // `Update(pid, TimerEvent)` and pop the timer from the Heap (or reschedule it). No additional threads for as many
    // timers as we'd like.
    //
    // But that's a lot of code, and this is a few lines:
    let send_plugin_instructions = env.plugin_env.senders.to_plugin.clone();
    let update_target = Some(env.plugin_env.plugin_id);
    let client_id = env.plugin_env.client_id;
    let plugin_name = env.plugin_env.name();
    thread::spawn(move || {
        let start_time = Instant::now();
        thread::sleep(Duration::from_secs_f64(secs));
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
            .ok_or(anyhow!("found no sender to send plugin instruction to"))
            .and_then(|sender| {
                sender
                    .send(PluginInstruction::Update(vec![(
                        update_target,
                        Some(client_id),
                        Event::Timer(elapsed_time),
                    )]))
                    .to_anyhow()
            })
            .with_context(|| {
                format!(
                    "failed to set host timeout of {secs} s for plugin {}",
                    plugin_name
                )
            })
            .non_fatal();
    });
}

fn host_exec_cmd(env: &ForeignFunctionEnv) {
    let err_context = || {
        format!(
            "failed to execute command on host for plugin '{}'",
            env.plugin_env.name()
        )
    };

    let mut cmdline: Vec<String> = wasi_read_object(&env.plugin_env.wasi_env)
        .with_context(err_context)
        .fatal();
    let command = cmdline.remove(0);

    // Bail out if we're forbidden to run command
    if !env.plugin_env.plugin._allow_exec_host_cmd {
        warn!("This plugin isn't allow to run command in host side, skip running this command: '{cmd} {args}'.",
        	cmd = command, args = cmdline.join(" "));
        return;
    }

    // Here, we don't wait the command to finish
    process::Command::new(command)
        .args(cmdline)
        .spawn()
        .with_context(err_context)
        .non_fatal();
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn host_report_panic(env: &ForeignFunctionEnv) {
    let msg = wasi_read_string(&env.plugin_env.wasi_env)
        .with_context(|| format!("failed to report panic for plugin '{}'", env.plugin_env.name()))
        .fatal();
    panic!("{}", msg);
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> Result<String> {
    let err_context = || format!("failed to read string from WASI env '{wasi_env:?}'");

    let mut buf = vec![];
    wasi_env
        .state()
        .fs
        .stdout_mut()
        .map_err(anyError::new)
        .and_then(|stdout| {
            stdout
                .as_mut()
                .ok_or(anyhow!("failed to get mutable reference to stdout"))
        })
        .and_then(|wasi_file| wasi_file.read_to_end(&mut buf).map_err(anyError::new))
        .with_context(err_context)?;
    let buf = String::from_utf8_lossy(&buf);
    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    Ok(buf.replace("\n", "\n\r"))
}

pub fn wasi_write_string(wasi_env: &WasiEnv, buf: &str) -> Result<()> {
    wasi_env
        .state()
        .fs
        .stdin_mut()
        .map_err(anyError::new)
        .and_then(|stdin| {
            stdin
                .as_mut()
                .ok_or(anyhow!("failed to get mutable reference to stdin"))
        })
        .and_then(|stdin| writeln!(stdin, "{}\r", buf).map_err(anyError::new))
        .with_context(|| format!("failed to write string to WASI env '{wasi_env:?}'"))
}

pub fn wasi_write_object(wasi_env: &WasiEnv, object: &(impl Serialize + ?Sized)) -> Result<()> {
    serde_json::to_string(&object)
        .map_err(anyError::new)
        .and_then(|string| wasi_write_string(wasi_env, &string))
        .with_context(|| format!("failed to serialize object for WASI env '{wasi_env:?}'"))
}

pub fn wasi_read_object<T: DeserializeOwned>(wasi_env: &WasiEnv) -> Result<T> {
    wasi_read_string(wasi_env)
        .and_then(|string| serde_json::from_str(&string).map_err(anyError::new))
        .with_context(|| format!("failed to deserialize object from WASI env '{wasi_env:?}'"))
}

pub fn apply_event_to_plugin(
    plugin_id: u32,
    client_id: ClientId,
    instance: &Instance,
    plugin_env: &PluginEnv,
    event: &Event,
    rows: usize,
    columns: usize,
    plugin_bytes: &mut Vec<(u32, ClientId, Vec<u8>)>,
) -> Result<()> {
    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let update = instance
        .exports
        .get_function("update")
        .with_context(err_context)?;
    wasi_write_object(&plugin_env.wasi_env, &event).with_context(err_context)?;
    let update_return =
        update
            .call(&[])
            .or_else::<anyError, _>(|e| match e.downcast::<serde_json::Error>() {
                Ok(_) => panic!(
                    "{}",
                    anyError::new(VersionMismatchError::new(
                        VERSION,
                        "Unavailable",
                        &plugin_env.plugin.path,
                        plugin_env.plugin.is_builtin(),
                    ))
                ),
                Err(e) => Err(e).with_context(err_context),
            })?;
    let should_render = match update_return.get(0) {
        Some(Value::I32(n)) => *n == 1,
        _ => false,
    };

    if rows > 0 && columns > 0 && should_render {
        let rendered_bytes = instance
            .exports
            .get_function("render")
            .map_err(anyError::new)
            .and_then(|render| {
                render
                    .call(&[Value::I32(rows as i32), Value::I32(columns as i32)])
                    .map_err(anyError::new)
            })
            .and_then(|_| wasi_read_string(&plugin_env.wasi_env))
            .with_context(err_context)?;
        plugin_bytes.push((plugin_id, client_id, rendered_bytes.as_bytes().to_vec()));
    }
    Ok(())
}
