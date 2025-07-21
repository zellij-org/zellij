use super::{PluginId, PluginInstruction};
use crate::plugins::pipes::{
    apply_pipe_message_to_plugin, pipes_to_block_or_unblock, PendingPipes, PipeStateChange,
};
use crate::plugins::plugin_loader::PluginLoader;
use crate::plugins::plugin_map::{AtomicEvent, PluginEnv, PluginMap, RunningPlugin, Subscriptions};

use crate::plugins::plugin_worker::MessageToWorker;
use crate::plugins::watch_filesystem::watch_filesystem;
use crate::plugins::zellij_exports::{wasi_read_string, wasi_write_object};
use async_channel::Sender;
use async_std::task::{self, JoinHandle};
use highway::{HighwayHash, PortableHash};
use log::info;
use notify_debouncer_full::{notify::RecommendedWatcher, Debouncer, FileIdMap};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    str::FromStr,
    sync::{Arc, Mutex},
};
use wasmtime::{Engine, Module};
use zellij_utils::consts::{ZELLIJ_CACHE_DIR, ZELLIJ_TMP_DIR};
use zellij_utils::data::{
    FloatingPaneCoordinates, InputMode, PermissionStatus, PermissionType, PipeMessage, PipeSource,
};
use zellij_utils::downloader::Downloader;
use zellij_utils::input::keybinds::Keybinds;
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::plugin_api::event::ProtobufEvent;

use prost::Message;

use crate::panes::PaneId;
use crate::{
    background_jobs::BackgroundJob, screen::ScreenInstruction, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId, ServerInstruction,
};
use zellij_utils::{
    data::{Event, EventType, PluginCapabilities},
    errors::prelude::*,
    input::{
        command::TerminalAction,
        layout::{Layout, PluginUserConfiguration, RunPlugin, RunPluginLocation, RunPluginOrAlias},
        plugins::PluginConfig,
    },
    ipc::ClientAttributes,
    pane_size::Size,
};

#[derive(Debug, Clone)]
pub enum EventOrPipeMessage {
    Event(Event),
    PipeMessage(PipeMessage),
}

#[derive(Debug, Clone, Default)]
pub struct PluginRenderAsset {
    // TODO: naming
    pub client_id: ClientId,
    pub plugin_id: PluginId,
    pub bytes: Vec<u8>,
    pub cli_pipes: HashMap<String, PipeStateChange>,
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
}

pub struct WasmBridge {
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    senders: ThreadSenders,
    engine: Engine,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_map: Arc<Mutex<PluginMap>>,
    next_plugin_id: PluginId,
    plugin_ids_waiting_for_permission_request: HashSet<PluginId>,
    cached_events_for_pending_plugins: HashMap<PluginId, Vec<EventOrPipeMessage>>,
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
    cached_plugin_map:
        HashMap<RunPluginLocation, HashMap<PluginUserConfiguration, Vec<(PluginId, ClientId)>>>,
    pending_pipes: PendingPipes,
    layout_dir: Option<PathBuf>,
    default_mode: InputMode,
    default_keybinds: Keybinds,
    keybinds: HashMap<ClientId, Keybinds>,
    base_modes: HashMap<ClientId, InputMode>,
    downloader: Downloader,
}

impl WasmBridge {
    pub fn new(
        senders: ThreadSenders,
        engine: Engine,
        plugin_dir: PathBuf,
        path_to_default_shell: PathBuf,
        zellij_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
        default_mode: InputMode,
        default_keybinds: Keybinds,
    ) -> Self {
        let plugin_map = Arc::new(Mutex::new(PluginMap::default()));
        let connected_clients: Arc<Mutex<Vec<ClientId>>> = Arc::new(Mutex::new(vec![]));
        let plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let watcher = None;
        let downloader = Downloader::new(ZELLIJ_CACHE_DIR.to_path_buf());
        WasmBridge {
            connected_clients,
            senders,
            engine,
            plugin_dir,
            plugin_cache,
            plugin_map,
            path_to_default_shell,
            watcher,
            next_plugin_id: 0,
            cached_events_for_pending_plugins: HashMap::new(),
            plugin_ids_waiting_for_permission_request: HashSet::new(),
            cached_resizes_for_pending_plugins: HashMap::new(),
            cached_worker_messages: HashMap::new(),
            loading_plugins: HashMap::new(),
            pending_plugin_reloads: HashSet::new(),
            zellij_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            cached_plugin_map: HashMap::new(),
            pending_pipes: Default::default(),
            layout_dir,
            default_mode,
            default_keybinds,
            keybinds: HashMap::new(),
            base_modes: HashMap::new(),
            downloader,
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
        cli_client_id: Option<ClientId>,
    ) -> Result<(PluginId, ClientId)> {
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

        match run {
            Some(run) => {
                let mut plugin = PluginConfig::from_run_plugin(run)
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
                    let engine = self.engine.clone();
                    let plugin_map = self.plugin_map.clone();
                    let connected_clients = self.connected_clients.clone();
                    let path_to_default_shell = self.path_to_default_shell.clone();
                    let zellij_cwd = cwd.unwrap_or_else(|| self.zellij_cwd.clone());
                    let capabilities = self.capabilities.clone();
                    let client_attributes = self.client_attributes.clone();
                    let default_shell = self.default_shell.clone();
                    let default_layout = self.default_layout.clone();
                    let layout_dir = self.layout_dir.clone();
                    let downloader = self.downloader.clone();
                    let default_mode = self
                        .base_modes
                        .get(&client_id)
                        .copied()
                        .unwrap_or(self.default_mode);
                    let keybinds = self
                        .keybinds
                        .get(&client_id)
                        .cloned()
                        .unwrap_or_else(|| self.default_keybinds.clone());
                    async move {
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

                            // if the url is already in cache, we'll use that version, otherwise
                            // we'll download it, place it in cache and then use it
                            match downloader.download(url, Some(&file_name)).await {
                                Ok(_) => plugin.path = ZELLIJ_CACHE_DIR.join(&file_name),
                                Err(e) => handle_plugin_loading_failure(
                                    &senders,
                                    plugin_id,
                                    &mut loading_indication,
                                    e,
                                    cli_client_id,
                                ),
                            }
                        }

                        match PluginLoader::start_plugin(
                            plugin_id,
                            client_id,
                            &plugin,
                            tab_index,
                            plugin_dir,
                            plugin_cache,
                            senders.clone(),
                            engine,
                            plugin_map.clone(),
                            size,
                            connected_clients.clone(),
                            &mut loading_indication,
                            path_to_default_shell,
                            zellij_cwd.clone(),
                            capabilities,
                            client_attributes,
                            default_shell,
                            default_layout,
                            skip_cache,
                            layout_dir,
                            default_mode,
                            keybinds,
                        ) {
                            Ok(_) => {
                                let plugin_list = plugin_map.lock().unwrap().list_plugins();
                                handle_plugin_successful_loading(&senders, plugin_id, plugin_list);
                            },
                            Err(e) => handle_plugin_loading_failure(
                                &senders,
                                plugin_id,
                                &mut loading_indication,
                                e,
                                cli_client_id,
                            ),
                        }
                        let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                            plugin_ids: vec![plugin_id],
                            done_receiving_permissions: false,
                        });
                    }
                });
                self.loading_plugins
                    .insert((plugin_id, run.clone()), load_plugin_task);
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
        let mut plugin_map = self.plugin_map.lock().unwrap();
        for ((plugin_id, client_id), (running_plugin, subscriptions, workers)) in
            plugin_map.remove_plugins(pid)
        {
            for (_worker_name, worker_sender) in workers {
                drop(worker_sender.send(MessageToWorker::Exit));
            }
            {
                // if the plugin was intercepting key presses and for some reason did not clear
                // this state, we make sure to do it ourselves so that the user will not get stuck
                if running_plugin.lock().unwrap().intercepting_key_presses() {
                    let _ = self
                        .senders
                        .send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(client_id));
                }
            }
            let subscriptions = subscriptions.lock().unwrap();
            if subscriptions.contains(&EventType::BeforeClose) {
                let mut running_plugin = running_plugin.lock().unwrap();
                match apply_before_close_event_to_plugin(
                    pid,
                    client_id,
                    &mut running_plugin,
                    self.senders.clone(),
                ) {
                    Ok(()) => {},
                    Err(e) => {
                        log::error!("{:?}", e);

                        // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
                        let stringified_error = format!("{:?}", e).replace("\n", "\n\r");

                        handle_plugin_crash(plugin_id, stringified_error, self.senders.clone());
                    },
                }
                let cache_dir = running_plugin.store.data().plugin_own_data_dir.clone();
                if let Err(e) = std::fs::remove_dir_all(cache_dir) {
                    log::error!("Failed to remove cache dir for plugin: {:?}", e);
                }
            } else {
                // this is duplicated because of locking/unlocking order between running_plugin and
                // subscriptions
                let running_plugin = running_plugin.lock().unwrap();
                let cache_dir = running_plugin.store.data().plugin_own_data_dir.clone();
                if let Err(e) = std::fs::remove_dir_all(cache_dir) {
                    log::error!("Failed to remove cache dir for plugin: {:?}", e);
                }
            }
        }
        self.cached_plugin_map.clear();
        let mut pipes_to_unblock = self.pending_pipes.unload_plugin(&pid);
        for pipe_name in pipes_to_unblock.drain(..) {
            let _ = self
                .senders
                .send_to_server(ServerInstruction::UnblockCliPipeInput(pipe_name))
                .context("failed to unblock input pipe");
        }
        let plugin_list = plugin_map.list_plugins();
        let _ = self
            .senders
            .send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));
        Ok(())
    }
    pub fn reload_plugin_with_id(&mut self, plugin_id: u32) -> Result<()> {
        let Some(run_plugin) = self.run_plugin_of_plugin_id(plugin_id).map(|r| r.clone()) else {
            log::error!("Failed to find plugin with id: {}", plugin_id);
            return Ok(());
        };

        let (rows, columns) = self.size_of_plugin_id(plugin_id).unwrap_or((0, 0));
        self.cached_events_for_pending_plugins
            .insert(plugin_id, vec![]);
        self.cached_resizes_for_pending_plugins
            .insert(plugin_id, (rows, columns));

        let mut loading_indication = LoadingIndication::new(run_plugin.location.to_string());
        self.start_plugin_loading_indication(&[plugin_id], &loading_indication);
        let load_plugin_task = task::spawn({
            let plugin_dir = self.plugin_dir.clone();
            let plugin_cache = self.plugin_cache.clone();
            let senders = self.senders.clone();
            let engine = self.engine.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            let path_to_default_shell = self.path_to_default_shell.clone();
            let capabilities = self.capabilities.clone();
            let client_attributes = self.client_attributes.clone();
            let default_shell = self.default_shell.clone();
            let default_layout = self.default_layout.clone();
            let layout_dir = self.layout_dir.clone();
            async move {
                match PluginLoader::reload_plugin(
                    plugin_id,
                    plugin_dir.clone(),
                    plugin_cache.clone(),
                    senders.clone(),
                    engine.clone(),
                    plugin_map.clone(),
                    connected_clients.clone(),
                    &mut loading_indication,
                    path_to_default_shell.clone(),
                    capabilities.clone(),
                    client_attributes.clone(),
                    default_shell.clone(),
                    default_layout.clone(),
                    layout_dir.clone(),
                ) {
                    Ok(_) => {
                        let plugin_list = plugin_map.lock().unwrap().list_plugins();
                        handle_plugin_successful_loading(&senders, plugin_id, plugin_list);
                    },
                    Err(e) => {
                        handle_plugin_loading_failure(
                            &senders,
                            plugin_id,
                            &mut loading_indication,
                            &e,
                            None,
                        );
                    },
                }
                let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                    plugin_ids: vec![plugin_id],
                    done_receiving_permissions: false,
                });
            }
        });
        self.loading_plugins
            .insert((plugin_id, run_plugin.clone()), load_plugin_task);
        Ok(())
    }
    pub fn reload_plugin(&mut self, run_plugin: &RunPlugin) -> Result<()> {
        if self.plugin_is_currently_being_loaded(&run_plugin.location) {
            self.pending_plugin_reloads.insert(run_plugin.clone());
            return Ok(());
        }

        let plugin_ids = self
            .all_plugin_ids_for_plugin_location(&run_plugin.location, &run_plugin.configuration)?;
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
            let engine = self.engine.clone();
            let plugin_map = self.plugin_map.clone();
            let connected_clients = self.connected_clients.clone();
            let path_to_default_shell = self.path_to_default_shell.clone();
            let zellij_cwd = self.zellij_cwd.clone();
            let capabilities = self.capabilities.clone();
            let client_attributes = self.client_attributes.clone();
            let default_shell = self.default_shell.clone();
            let default_layout = self.default_layout.clone();
            let layout_dir = self.layout_dir.clone();
            async move {
                match PluginLoader::reload_plugin(
                    first_plugin_id,
                    plugin_dir.clone(),
                    plugin_cache.clone(),
                    senders.clone(),
                    engine.clone(),
                    plugin_map.clone(),
                    connected_clients.clone(),
                    &mut loading_indication,
                    path_to_default_shell.clone(),
                    capabilities.clone(),
                    client_attributes.clone(),
                    default_shell.clone(),
                    default_layout.clone(),
                    layout_dir.clone(),
                ) {
                    Ok(_) => {
                        let plugin_list = plugin_map.lock().unwrap().list_plugins();
                        handle_plugin_successful_loading(&senders, first_plugin_id, plugin_list);
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
                                engine.clone(),
                                plugin_map.clone(),
                                connected_clients.clone(),
                                &mut loading_indication,
                                path_to_default_shell.clone(),
                                zellij_cwd.clone(),
                                capabilities.clone(),
                                client_attributes.clone(),
                                default_shell.clone(),
                                default_layout.clone(),
                                layout_dir.clone(),
                            ) {
                                Ok(_) => {
                                    let plugin_list = plugin_map.lock().unwrap().list_plugins();
                                    handle_plugin_successful_loading(
                                        &senders,
                                        *plugin_id,
                                        plugin_list,
                                    );
                                },
                                Err(e) => handle_plugin_loading_failure(
                                    &senders,
                                    *plugin_id,
                                    &mut loading_indication,
                                    e,
                                    None,
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
                                None,
                            );
                        }
                    },
                }
                let _ = senders.send_to_plugin(PluginInstruction::ApplyCachedEvents {
                    plugin_ids,
                    done_receiving_permissions: false,
                });
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
            self.engine.clone(),
            self.plugin_map.clone(),
            self.connected_clients.clone(),
            &mut loading_indication,
            self.path_to_default_shell.clone(),
            self.zellij_cwd.clone(),
            self.capabilities.clone(),
            self.client_attributes.clone(),
            self.default_shell.clone(),
            self.default_layout.clone(),
            self.layout_dir.clone(),
            self.default_mode,
            self.keybinds
                .get(&client_id)
                .cloned()
                .unwrap_or_else(|| self.default_keybinds.clone()),
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
        shutdown_sender: Sender<()>,
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
                    let _s = shutdown_sender.clone();
                    async move {
                        let mut running_plugin = running_plugin.lock().unwrap();
                        let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                        if running_plugin.apply_event_id(AtomicEvent::Resize, event_id) {
                            let old_rows = running_plugin.rows;
                            let old_columns = running_plugin.columns;
                            running_plugin.rows = new_rows;
                            running_plugin.columns = new_columns;

                            // in the below conditional, we check if event_id == 0 so that we'll
                            // make sure to always render on the first resize event
                            if old_rows != new_rows || old_columns != new_columns || event_id == 0 {
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
                                    .and_then(|_| wasi_read_string(running_plugin.store.data()))
                                    .with_context(err_context);
                                match rendered_bytes {
                                    Ok(rendered_bytes) => {
                                        let plugin_render_asset = PluginRenderAsset::new(
                                            plugin_id,
                                            client_id,
                                            rendered_bytes.as_bytes().to_vec(),
                                        );
                                        senders
                                            .send_to_screen(ScreenInstruction::PluginBytes(vec![
                                                plugin_render_asset,
                                            ]))
                                            .unwrap();
                                    },
                                    Err(e) => log::error!("{}", e),
                                }
                            }
                        }
                    }
                });
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
    pub fn update_plugins(
        &mut self,
        mut updates: Vec<(Option<PluginId>, Option<ClientId>, Event)>,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
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
        task::spawn({
            let mut updates = updates.clone();
            let senders = self.senders.clone();
            let s = shutdown_sender.clone();
            async move {
                let _s = s;
                for (pid, cid, event) in updates.drain(..) {
                    for (plugin_id, client_id, running_plugin, subscriptions) in &plugins_to_update
                    {
                        let subs = subscriptions.lock().unwrap().clone();
                        // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                        if let Ok(event_type) = EventType::from_str(&event.to_string()) {
                            if (subs.contains(&event_type)
                                || event_type == EventType::PermissionRequestResult)
                                && Self::message_is_directed_at_plugin(
                                    pid, cid, plugin_id, client_id,
                                )
                            {
                                let mut running_plugin = running_plugin.lock().unwrap();
                                let mut plugin_render_assets = vec![];
                                match apply_event_to_plugin(
                                    *plugin_id,
                                    *client_id,
                                    &mut running_plugin,
                                    &event,
                                    &mut plugin_render_assets,
                                    senders.clone(),
                                ) {
                                    Ok(()) => {
                                        let _ = senders.send_to_screen(
                                            ScreenInstruction::PluginBytes(plugin_render_assets),
                                        );
                                    },
                                    Err(e) => {
                                        log::error!("{:?}", e);

                                        // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
                                        let stringified_error =
                                            format!("{:?}", e).replace("\n", "\n\r");

                                        handle_plugin_crash(
                                            *plugin_id,
                                            stringified_error,
                                            senders.clone(),
                                        );
                                    },
                                }
                            }
                        }
                    }
                }
            }
        });
        // loop once more to update the cached events for the pending plugins (probably currently
        // being loaded, we'll send them these events when they load)
        for (pid, _cid, event) in updates.drain(..) {
            for (plugin_id, cached_events) in self.cached_events_for_pending_plugins.iter_mut() {
                if pid.is_none() || pid.as_ref() == Some(plugin_id) {
                    cached_events.push(EventOrPipeMessage::Event(event.clone()));
                }
            }
        }
        Ok(())
    }
    pub fn get_plugin_cwd(&self, plugin_id: PluginId, client_id: ClientId) -> Option<PathBuf> {
        self.plugin_map
            .lock()
            .unwrap()
            .running_plugins()
            .iter()
            .find_map(|(p_id, c_id, running_plugin)| {
                if p_id == &plugin_id && c_id == &client_id {
                    let plugin_cwd = running_plugin
                        .lock()
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
        let plugins_to_change: Vec<(
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
            .collect();
        task::spawn({
            let senders = self.senders.clone();
            async move {
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
                for (plugin_id, client_id, running_plugin, _subscriptions) in &plugins_to_change {
                    if plugin_id == &plugin_id_to_update && client_id == &client_id_to_update {
                        let mut running_plugin = running_plugin.lock().unwrap();
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
                            stdin_pipe.clone(),
                            stdout_pipe.clone(),
                        );
                        match wasi_ctx {
                            Ok(wasi_ctx) => {
                                drop(std::mem::replace(&mut plugin_env.wasi_ctx, wasi_ctx));
                                plugin_env.plugin_cwd = new_host_dir.clone();

                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
                                    Some(*plugin_id),
                                    Some(*client_id),
                                    Event::HostFolderChanged(new_host_dir.clone()),
                                )]));
                            },
                            Err(e) => {
                                let _ = senders.send_to_plugin(PluginInstruction::Update(vec![(
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
        });
        Ok(())
    }
    pub fn pipe_messages(
        &mut self,
        mut messages: Vec<(Option<PluginId>, Option<ClientId>, PipeMessage)>,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
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
        for (message_pid, message_cid, pipe_message) in messages.drain(..) {
            for (plugin_id, client_id, running_plugin, _subscriptions) in &plugins_to_update {
                if Self::message_is_directed_at_plugin(
                    message_pid,
                    message_cid,
                    plugin_id,
                    client_id,
                ) {
                    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                        self.pending_pipes
                            .mark_being_processed(pipe_id, plugin_id, client_id);
                    }
                    task::spawn({
                        let senders = self.senders.clone();
                        let running_plugin = running_plugin.clone();
                        let pipe_message = pipe_message.clone();
                        let plugin_id = *plugin_id;
                        let client_id = *client_id;
                        let _s = shutdown_sender.clone();
                        async move {
                            let mut running_plugin = running_plugin.lock().unwrap();
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
                                    let _ = senders.send_to_screen(ScreenInstruction::PluginBytes(
                                        plugin_render_assets,
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
            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .lock()
                .unwrap()
                .iter()
                .copied()
                .collect();
            for (plugin_id, cached_events) in self.cached_events_for_pending_plugins.iter_mut() {
                if message_pid.is_none() || message_pid.as_ref() == Some(plugin_id) {
                    cached_events.push(EventOrPipeMessage::PipeMessage(pipe_message.clone()));
                    if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                        for client_id in &all_connected_clients {
                            if Self::message_is_directed_at_plugin(
                                message_pid,
                                message_cid,
                                plugin_id,
                                client_id,
                            ) {
                                self.pending_pipes
                                    .mark_being_processed(pipe_id, plugin_id, client_id);
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
            if !done_receiving_permissions
                && self
                    .plugin_ids_waiting_for_permission_request
                    .contains(&plugin_id)
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
                .retain(|(p_id, _run_plugin), _| p_id != &plugin_id);
            self.clear_plugin_map_cache();
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
    pub fn run_plugin_of_loading_plugin_id(&self, plugin_id: PluginId) -> Option<&RunPlugin> {
        self.loading_plugins
            .iter()
            .find(|((p_id, _run_plugin), _)| p_id == &plugin_id)
            .map(|((_p_id, run_plugin), _)| run_plugin)
    }
    pub fn run_plugin_of_plugin_id(&self, plugin_id: PluginId) -> Option<RunPlugin> {
        self.plugin_map
            .lock()
            .unwrap()
            .run_plugin_of_plugin_id(plugin_id)
    }

    pub fn reconfigure(
        &mut self,
        client_id: ClientId,
        keybinds: Option<Keybinds>,
        default_mode: Option<InputMode>,
        default_shell: Option<TerminalAction>,
    ) -> Result<()> {
        let plugins_to_reconfigure: Vec<Arc<Mutex<RunningPlugin>>> = self
            .plugin_map
            .lock()
            .unwrap()
            .running_plugins()
            .iter()
            .cloned()
            .filter_map(|(_plugin_id, c_id, running_plugin)| {
                if c_id == client_id {
                    Some(running_plugin.clone())
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
        for running_plugin in plugins_to_reconfigure {
            task::spawn({
                let running_plugin = running_plugin.clone();
                let keybinds = keybinds.clone();
                let default_shell = default_shell.clone();
                async move {
                    let mut running_plugin = running_plugin.lock().unwrap();
                    if let Some(keybinds) = keybinds {
                        running_plugin.update_keybinds(keybinds);
                    }
                    if let Some(default_mode) = default_mode {
                        running_plugin.update_default_mode(default_mode);
                    }
                    running_plugin.update_default_shell(default_shell);
                }
            });
        }
        Ok(())
    }
    fn apply_cached_events_and_resizes_for_plugin(
        &mut self,
        plugin_id: PluginId,
        shutdown_sender: Sender<()>,
    ) -> Result<()> {
        let err_context = || format!("Failed to apply cached events to plugin");
        if let Some(events_or_pipe_messages) =
            self.cached_events_for_pending_plugins.remove(&plugin_id)
        {
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
                    task::spawn({
                        let senders = self.senders.clone();
                        let running_plugin = running_plugin.clone();
                        let client_id = *client_id;
                        let _s = shutdown_sender.clone();
                        let events_or_pipe_messages = events_or_pipe_messages.clone();
                        async move {
                            let subs = subscriptions.lock().unwrap().clone();
                            let _s = _s; // guard to allow the task to complete before cleanup/shutdown
                            for event_or_pipe_message in events_or_pipe_messages {
                                match event_or_pipe_message {
                                    EventOrPipeMessage::Event(event) => {
                                        match EventType::from_str(&event.to_string())
                                            .with_context(err_context)
                                        {
                                            Ok(event_type) => {
                                                if !subs.contains(&event_type) {
                                                    continue;
                                                }
                                                let mut running_plugin =
                                                    running_plugin.lock().unwrap();
                                                let mut plugin_render_assets = vec![];
                                                match apply_event_to_plugin(
                                                    plugin_id,
                                                    client_id,
                                                    &mut running_plugin,
                                                    &event,
                                                    &mut plugin_render_assets,
                                                    senders.clone(),
                                                ) {
                                                    Ok(()) => {
                                                        let _ = senders.send_to_screen(
                                                            ScreenInstruction::PluginBytes(
                                                                plugin_render_assets,
                                                            ),
                                                        );
                                                    },
                                                    Err(e) => {
                                                        log::error!("{}", e);
                                                    },
                                                }
                                            },
                                            Err(e) => {
                                                log::error!("Failed to apply event: {:?}", e);
                                            },
                                        }
                                    },
                                    EventOrPipeMessage::PipeMessage(pipe_message) => {
                                        let mut running_plugin = running_plugin.lock().unwrap();
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
                                    },
                                }
                            }
                        }
                    });
                }
            }
        }
        if let Some((rows, columns)) = self.cached_resizes_for_pending_plugins.remove(&plugin_id) {
            self.resize_plugin(plugin_id, columns, rows, shutdown_sender.clone())?;
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
    fn plugin_id_of_loading_plugin(
        &self,
        plugin_location: &RunPluginLocation,
        plugin_configuration: &PluginUserConfiguration,
    ) -> Option<PluginId> {
        self.loading_plugins
            .iter()
            .find_map(|((plugin_id, run_plugin), _)| {
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
    pub fn cache_plugin_permissions(
        &mut self,
        plugin_id: PluginId,
        client_id: Option<ClientId>,
        permissions: Vec<PermissionType>,
        status: PermissionStatus,
        cache_path: Option<PathBuf>,
    ) -> Result<()> {
        let err_context = || format!("Failed to write plugin permission {plugin_id}");

        let running_plugin = self
            .plugin_map
            .lock()
            .unwrap()
            .get_running_plugin(plugin_id, client_id)
            .ok_or_else(|| anyhow!("Failed to get running plugin"))?;

        let mut running_plugin = running_plugin.lock().unwrap();

        let permissions = if status == PermissionStatus::Granted {
            permissions
        } else {
            vec![]
        };

        running_plugin
            .store
            .data_mut()
            .set_permissions(HashSet::from_iter(permissions.clone()));

        let mut permission_cache = PermissionCache::from_path_or_default(cache_path);
        permission_cache.cache(
            running_plugin.store.data().plugin.location.to_string(),
            permissions,
        );

        permission_cache.write_to_file().with_context(err_context)
    }
    pub fn cache_plugin_events(&mut self, plugin_id: PluginId) {
        self.plugin_ids_waiting_for_permission_request
            .insert(plugin_id);
        self.cached_events_for_pending_plugins
            .entry(plugin_id)
            .or_insert_with(Default::default);
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
        let run_plugin = run_plugin_or_alias.get_run_plugin();
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
                        None,
                        cli_client_id,
                    ) {
                        Ok((plugin_id, client_id)) => {
                            let start_suppressed = false;
                            drop(self.senders.send_to_screen(ScreenInstruction::AddPlugin(
                                Some(should_float),
                                should_be_open_in_place,
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
                            )));
                            vec![(plugin_id, Some(client_id))]
                        },
                        Err(e) => {
                            log::error!("Failed to load plugin: {e}");
                            if let Some(cli_client_id) = cli_client_id {
                                let _ = self.senders.send_to_server(ServerInstruction::LogError(
                                    vec![format!("Failed to log plugin: {e}")],
                                    cli_client_id,
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
    ) -> Vec<String> {
        let mut pipe_names_to_unblock = vec![];
        for pipe_state_change in pipe_state_changes {
            let client_id = pipe_state_change.client_id;
            let plugin_id = pipe_state_change.plugin_id;
            for (cli_pipe_name, pipe_state_change) in pipe_state_change.cli_pipes {
                pipe_names_to_unblock.append(&mut self.pending_pipes.update_pipe_state_change(
                    &cli_pipe_name,
                    pipe_state_change,
                    &plugin_id,
                    &client_id,
                ));
            }
        }
        let pipe_names_to_unblock =
            pipe_names_to_unblock
                .into_iter()
                .fold(HashSet::new(), |mut acc, p| {
                    acc.insert(p);
                    acc
                });
        pipe_names_to_unblock.into_iter().collect()
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
            .unwrap()
            .iter()
            .next()
            .copied()
    }
}

fn handle_plugin_successful_loading(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    plugin_list: BTreeMap<PluginId, RunPlugin>,
) {
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    let _ = senders.send_to_screen(ScreenInstruction::RequestStateUpdateForPlugins);
    let _ = senders.send_to_background_jobs(BackgroundJob::ReportPluginList(plugin_list));
}

fn handle_plugin_loading_failure(
    senders: &ThreadSenders,
    plugin_id: PluginId,
    loading_indication: &mut LoadingIndication,
    error: impl std::fmt::Debug,
    cli_client_id: Option<ClientId>,
) {
    log::error!("{:?}", error);
    let _ = senders.send_to_background_jobs(BackgroundJob::StopPluginLoadingAnimation(plugin_id));
    loading_indication.indicate_loading_error(format!("{:?}", error));
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    if let Some(cli_client_id) = cli_client_id {
        let _ = senders.send_to_server(ServerInstruction::LogError(
            vec![format!("{:?}", error)],
            cli_client_id,
        ));
    }
}

// TODO: move to permissions?
fn check_event_permission(
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
        | Event::InputReceived => PermissionType::ReadApplicationState,
        Event::WebServerStatus(..) => PermissionType::StartWebServer,
        _ => return (PermissionStatus::Granted, None),
    };

    if let Some(permissions) = plugin_env.permissions.lock().unwrap().as_ref() {
        if permissions.contains(&permission) {
            return (PermissionStatus::Granted, None);
        }
    }

    (PermissionStatus::Denied, Some(permission))
}

pub fn apply_event_to_plugin(
    plugin_id: PluginId,
    client_id: ClientId,
    running_plugin: &mut RunningPlugin,
    event: &Event,
    plugin_render_assets: &mut Vec<PluginRenderAsset>,
    senders: ThreadSenders,
) -> Result<()> {
    let instance = &running_plugin.instance;
    let rows = running_plugin.rows;
    let columns = running_plugin.columns;

    let err_context = || format!("Failed to apply event to plugin {plugin_id}");
    match check_event_permission(running_plugin.store.data(), event) {
        (PermissionStatus::Granted, _) => {
            let mut event = event.clone();
            if let Event::ModeUpdate(mode_info) = &mut event {
                // we do this because there can be some cases where this event arrives here with
                // the wrong keybindings or default mode (for example: when triggered from the CLI,
                // where we do not know the target client_id and thus don't know if their keybindings are the
                // default or if they have changed at runtime), the keybindings in running_plugin
                // should always be up-to-date. Ideally, we would have changed the keybindings in
                // ModeInfo to an Option, but alas - this is already part of our contract and that
                // would be a breaking change.
                mode_info.keybinds = running_plugin.store.data().keybinds.to_keybinds_vec();
                mode_info.base_mode = Some(running_plugin.store.data().default_mode);
            }
            let protobuf_event: ProtobufEvent = event
                .clone()
                .try_into()
                .map_err(|e| anyhow!("Failed to convert to protobuf: {:?}", e))?;
            let update = instance
                .get_typed_func::<(), i32>(&mut running_plugin.store, "update")
                .with_context(err_context)?;
            wasi_write_object(running_plugin.store.data(), &protobuf_event.encode_to_vec())
                .with_context(err_context)?;
            let should_render = update
                .call(&mut running_plugin.store, ())
                .with_context(err_context)?;
            let mut should_render = should_render == 1;
            if let Event::PermissionRequestResult(..) = event {
                // we always render in this case, otherwise the request permission screen stays on
                // screen
                should_render = true;
            }
            if rows > 0 && columns > 0 && should_render {
                let rendered_bytes = instance
                    .get_typed_func::<(i32, i32), ()>(&mut running_plugin.store, "render")
                    .and_then(|render| {
                        render.call(&mut running_plugin.store, (rows as i32, columns as i32))
                    })
                    .and_then(|_| wasi_read_string(running_plugin.store.data()))
                    .with_context(err_context)?;
                let pipes_to_block_or_unblock = pipes_to_block_or_unblock(running_plugin, None);
                let plugin_render_asset = PluginRenderAsset::new(
                    plugin_id,
                    client_id,
                    rendered_bytes.as_bytes().to_vec(),
                )
                .with_pipes(pipes_to_block_or_unblock);
                plugin_render_assets.push(plugin_render_asset);
            } else {
                // This is a bit of a hack to get around the fact that plugins are allowed not to
                // render and still unblock CLI pipes
                let pipes_to_block_or_unblock = pipes_to_block_or_unblock(running_plugin, None);
                let plugin_render_asset = PluginRenderAsset::new(plugin_id, client_id, vec![])
                    .with_pipes(pipes_to_block_or_unblock);
                let _ = senders
                    .send_to_plugin(PluginInstruction::UnblockCliPipes(vec![
                        plugin_render_asset,
                    ]))
                    .context("failed to unblock input pipe");
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

pub fn handle_plugin_crash(plugin_id: PluginId, message: String, senders: ThreadSenders) {
    let mut loading_indication = LoadingIndication::new("Panic!".to_owned());
    loading_indication.indicate_loading_error(message);
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication,
    ));
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
