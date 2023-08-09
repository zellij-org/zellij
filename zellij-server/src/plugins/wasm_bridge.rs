use super::{PluginId, PluginInstruction};
use crate::plugins::plugin_loader::PluginLoader;
use crate::plugins::plugin_map::{AtomicEvent, PluginEnv, PluginMap, RunningPlugin, Subscriptions};
use crate::plugins::plugin_worker::MessageToWorker;
use crate::plugins::watch_filesystem::watch_filesystem;
use crate::plugins::zellij_exports::{wasi_read_string, wasi_write_object};
use log::info;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};
use wasmer::{Instance, Module, Store, Value};
use zellij_utils::async_std::task::{self, JoinHandle};
use zellij_utils::notify_debouncer_full::{notify::RecommendedWatcher, Debouncer, FileIdMap};
use zellij_utils::plugin_api::event::ProtobufEvent;

use zellij_utils::prost::Message;

use crate::{
    background_jobs::BackgroundJob, screen::ScreenInstruction, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId,
};
use zellij_utils::{
    data::{Event, EventType, PluginCapabilities},
    errors::prelude::*,
    input::{
        command::TerminalAction,
        layout::{Layout, RunPlugin, RunPluginLocation},
        plugins::PluginsConfig,
    },
    ipc::ClientAttributes,
    pane_size::Size,
};

pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    plugins: PluginsConfig,
    senders: ThreadSenders,
    store: Store,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    next_plugin_id: PluginId,
    cached_events_for_pending_plugins: HashMap<PluginId, Vec<Event>>,
    cached_resizes_for_pending_plugins: HashMap<PluginId, (usize, usize)>, // (rows, columns)
    cached_worker_messages: HashMap<PluginId, Vec<(ClientId, String, String, String)>>, // Vec<clientid,
    // worker_name,
    // message,
    // payload>
    loading_plugins: HashMap<(PluginId, RunPlugin), JoinHandle<()>>, // plugin_id to join-handle
    pending_plugin_reloads: HashSet<RunPlugin>,
    path_to_default_shell: PathBuf,
    watcher: Option<Debouncer<RecommendedWatcher, FileIdMap>>,
    zellij_cwd: PathBuf,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
    default_layout: Box<Layout>,
}

impl WasmBridge {
    pub fn new(
        plugins: PluginsConfig,
        senders: ThreadSenders,
        store: Store,
        plugin_dir: PathBuf,
        path_to_default_shell: PathBuf,
        zellij_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
    ) -> Self {
        let plugin_map = Arc::new(Mutex::new(PluginMap::default()));
        let connected_clients: Arc<Mutex<Vec<ClientId>>> = Arc::new(Mutex::new(vec![]));
        let plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let watcher = None;
        WasmBridge {
            connected_clients,
            plugins,
            senders,
            store,
            plugin_dir,
            plugin_cache,
            plugin_map,
            path_to_default_shell,
            watcher,
            next_plugin_id: 0,
            cached_events_for_pending_plugins: HashMap::new(),
            cached_resizes_for_pending_plugins: HashMap::new(),
            cached_worker_messages: HashMap::new(),
            loading_plugins: HashMap::new(),
            pending_plugin_reloads: HashSet::new(),
            zellij_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
        }
    }
    pub fn load_plugin(
        &mut self,
        run: &RunPlugin,
        tab_index: usize,
        size: Size,
        client_id: Option<ClientId>,
    ) -> Result<PluginId> {
        // returns the plugin id
        let err_context = move || format!("failed to load plugin");

        let client_id = client_id
            .or_else(|| {
                self.connected_clients
                    .lock()
                    .unwrap()
                    .iter()
                    .next()
                    .copied()
            })
            .with_context(|| {
                "Plugins must have a client id, none was provided and none are connected"
            })?;

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
            let path_to_default_shell = self.path_to_default_shell.clone();
            let zellij_cwd = self.zellij_cwd.clone();
            let capabilities = self.capabilities.clone();
            let client_attributes = self.client_attributes.clone();
            let default_shell = self.default_shell.clone();
            let default_layout = self.default_layout.clone();
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
                    path_to_default_shell,
                    zellij_cwd.clone(),
                    capabilities,
                    client_attributes,
                    default_shell,
                    default_layout,
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
    pub fn unload_plugin(&mut self, pid: PluginId) -> Result<()> {
        info!("Bye from plugin {}", &pid);
        let mut plugin_map = self.plugin_map.lock().unwrap();
        for (running_plugin, _, workers) in plugin_map.remove_plugins(pid) {
            for (_worker_name, worker_sender) in workers {
                drop(worker_sender.send(MessageToWorker::Exit));
            }
            let running_plugin = running_plugin.lock().unwrap();
            let cache_dir = running_plugin.plugin_env.plugin_own_data_dir.clone();
            if let Err(e) = std::fs::remove_dir_all(cache_dir) {
                log::error!("Failed to remove cache dir for plugin: {:?}", e);
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
            let path_to_default_shell = self.path_to_default_shell.clone();
            let zellij_cwd = self.zellij_cwd.clone();
            let capabilities = self.capabilities.clone();
            let client_attributes = self.client_attributes.clone();
            let default_shell = self.default_shell.clone();
            let default_layout = self.default_layout.clone();
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
                    path_to_default_shell.clone(),
                    zellij_cwd.clone(),
                    capabilities.clone(),
                    client_attributes.clone(),
                    default_shell.clone(),
                    default_layout.clone(),
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
                                path_to_default_shell.clone(),
                                zellij_cwd.clone(),
                                capabilities.clone(),
                                client_attributes.clone(),
                                default_shell.clone(),
                                default_layout.clone(),
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
            self.path_to_default_shell.clone(),
            self.zellij_cwd.clone(),
            self.capabilities.clone(),
            self.client_attributes.clone(),
            self.default_shell.clone(),
            self.default_layout.clone(),
        ) {
            Ok(_) => {
                let _ = self
                    .senders
                    .send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
                Ok(())
            },
            Err(e) => Err(e),
        }
    }
    pub fn resize_plugin(
        &mut self,
        pid: PluginId,
        new_columns: usize,
        new_rows: usize,
    ) -> Result<()> {
        let err_context = move || format!("failed to resize plugin {pid}");

        let plugins_to_resize: Vec<(PluginId, ClientId, Arc<Mutex<RunningPlugin>>)> = self
            .plugin_map
            .lock()
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
                    .unwrap()
                    .next_event_id(AtomicEvent::Resize);
                task::spawn({
                    let senders = self.senders.clone();
                    let running_plugin = running_plugin.clone();
                    let plugin_id = plugin_id;
                    let client_id = client_id;
                    async move {
                        let mut running_plugin = running_plugin.lock().unwrap();
                        if running_plugin.apply_event_id(AtomicEvent::Resize, event_id) {
                            running_plugin.rows = new_rows;
                            running_plugin.columns = new_columns;
                            let rendered_bytes = running_plugin
                                .instance
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
                                    let plugin_bytes = vec![(
                                        plugin_id,
                                        client_id,
                                        rendered_bytes.as_bytes().to_vec(),
                                    )];
                                    senders
                                        .send_to_screen(ScreenInstruction::PluginBytes(
                                            plugin_bytes,
                                        ))
                                        .unwrap();
                                },
                                Err(e) => log::error!("{}", e),
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
        mut updates: Vec<(Option<PluginId>, Option<ClientId>, Event)>,
    ) -> Result<()> {
        let err_context = || "failed to update plugin state".to_string();

        let plugins_to_update: Vec<(
            PluginId,
            ClientId,
            Arc<Mutex<RunningPlugin>>,
            Arc<Mutex<Subscriptions>>,
        )> = self
            .plugin_map
            .lock()
            .unwrap()
            .running_plugins_and_subscriptions()
            .iter()
            .cloned()
            .filter(|(plugin_id, _client_id, _running_plugin, _subscriptions)| {
                !&self
                    .cached_events_for_pending_plugins
                    .contains_key(&plugin_id)
            })
            .collect();
        for (pid, cid, event) in updates.drain(..) {
            for (plugin_id, client_id, running_plugin, subscriptions) in &plugins_to_update {
                let subs = subscriptions.lock().unwrap().clone();
                // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                let event_type =
                    EventType::from_str(&event.to_string()).with_context(err_context)?;
                if subs.contains(&event_type)
                    && ((pid.is_none() && cid.is_none())
                        || (pid.is_none() && cid == Some(*client_id))
                        || (cid.is_none() && pid == Some(*plugin_id))
                        || (cid == Some(*client_id) && pid == Some(*plugin_id)))
                {
                    task::spawn({
                        let senders = self.senders.clone();
                        let running_plugin = running_plugin.clone();
                        let event = event.clone();
                        let plugin_id = *plugin_id;
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
                                    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(
                                        plugin_bytes,
                                    ));
                                },
                                Err(e) => {
                                    log::error!("{:?}", e);

                                    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
                                    let stringified_error =
                                        format!("{:?}", e).replace("\n", "\n\r");

                                    handle_plugin_crash(
                                        plugin_id,
                                        stringified_error,
                                        senders.clone(),
                                    );
                                },
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
    pub fn apply_cached_events(&mut self, plugin_ids: Vec<PluginId>) -> Result<()> {
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
        let plugin_ids = self.plugin_map.lock().unwrap().plugin_ids();
        for plugin_id in &plugin_ids {
            drop(self.unload_plugin(*plugin_id));
        }
        if let Some(watcher) = self.watcher.take() {
            watcher.stop_nonblocking();
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
            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .lock()
                .unwrap()
                .iter()
                .copied()
                .collect();
            for client_id in &all_connected_clients {
                if let Some((running_plugin, subscriptions)) = self
                    .plugin_map
                    .lock()
                    .unwrap()
                    .get_running_plugin_and_subscriptions(plugin_id, *client_id)
                {
                    let subs = subscriptions.lock().unwrap().clone();
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
                                        let _ = senders.send_to_screen(
                                            ScreenInstruction::PluginBytes(plugin_bytes),
                                        );
                                    },
                                    Err(e) => {
                                        log::error!("{}", e);
                                    },
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
        self.apply_cached_worker_messages(plugin_id)?;
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
            .find(|((_plugin_id, run_plugin), _)| &run_plugin.location == plugin_location)
            .is_some()
    }
    fn all_plugin_ids_for_plugin_location(
        &self,
        plugin_location: &RunPluginLocation,
    ) -> Result<Vec<PluginId>> {
        self.plugin_map
            .lock()
            .unwrap()
            .all_plugin_ids_for_plugin_location(plugin_location)
    }
    fn size_of_plugin_id(&self, plugin_id: PluginId) -> Option<(usize, usize)> {
        // (rows/colums)
        self.plugin_map
            .lock()
            .unwrap()
            .get_running_plugin(plugin_id, None)
            .map(|r| {
                let r = r.lock().unwrap();
                (r.rows, r.columns)
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
    pub fn post_messages_to_plugin_worker(
        &mut self,
        plugin_id: PluginId,
        client_id: ClientId,
        worker_name: String,
        mut messages: Vec<(String, String)>,
    ) -> Result<()> {
        let worker =
            self.plugin_map
                .lock()
                .unwrap()
                .worker_sender(plugin_id, client_id, &worker_name);
        match worker {
            Some(worker) => {
                for (message, payload) in messages.drain(..) {
                    if let Err(e) = worker.try_send(MessageToWorker::Message(message, payload)) {
                        log::error!("Failed to send message to worker: {:?}", e);
                    }
                }
            },
            None => {
                log::warn!("Worker {worker_name} not found, caching messages");
                for (message, payload) in messages.drain(..) {
                    self.cached_worker_messages
                        .entry(plugin_id)
                        .or_default()
                        .push((client_id, worker_name.clone(), message, payload));
                }
            },
        }
        Ok(())
    }
    pub fn start_fs_watcher_if_not_started(&mut self) {
        if self.watcher.is_none() {
            self.watcher = match watch_filesystem(self.senders.clone(), &self.zellij_cwd) {
                Ok(watcher) => Some(watcher),
                Err(e) => {
                    log::error!("Failed to watch filesystem: {:?}", e);
                    None
                },
            };
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
    error: impl std::fmt::Debug,
) {
    log::error!("{:?}", error);
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    loading_indication.indicate_loading_error(format!("{:?}", error));
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
}

pub fn apply_event_to_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    instance: &Instance,
    plugin_env: &PluginEnv,
    event: &Event,
    rows: usize,
    columns: usize,
    plugin_bytes: &mut Vec<(PluginId, ClientId, Vec<u8>)>,
) -> Result<()> {
    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    let protobuf_event: ProtobufEvent = event
        .clone()
        .try_into()
        .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
    let update = instance
        .exports
        .get_function("update")
        .with_context(err_context)?;
    wasi_write_object(&plugin_env.wasi_env, &protobuf_event.encode_to_vec())
        .with_context(err_context)?;
    let update_return = update.call(&[]).with_context(err_context)?;
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

pub fn handle_plugin_crash(plugin_id: PluginId, message: String, senders: ThreadSenders) {
    let mut loading_indication = LoadingIndication::new("Panic!".to_owned());
    loading_indication.indicate_loading_error(message);
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication,
    ));
    let _ = senders.send_to_plugin(PluginInstruction::Unload(plugin_id));
}
