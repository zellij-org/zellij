use crate::plugins::plugin_map::{
    PluginEnv, PluginMap, RunningPlugin,
};
use crate::plugins::plugin_worker::{plugin_worker, RunningWorker};
use crate::plugins::zellij_exports::{wasi_write_object, zellij_exports};
use crate::plugins::PluginId;
use prost::Message;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use url::Url;
use wasmi::{Engine, Instance, Linker, Module, Store};
use wasmi_wasi::{WasiCtx, WasiCtxBuilder};

use crate::{
    screen::ScreenInstruction, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId,
};

use zellij_utils::plugin_api::action::ProtobufPluginConfiguration;
use zellij_utils::{
    consts::{ZELLIJ_SESSION_CACHE_DIR, ZELLIJ_TMP_DIR},
    data::{InputMode, PluginCapabilities},
    errors::prelude::*,
    input::command::TerminalAction,
    input::keybinds::Keybinds,
    input::layout::Layout,
    input::plugins::PluginConfig,
    ipc::ClientAttributes,
    pane_size::Size,
};

macro_rules! display_loading_stage {
    ($loading_stage:ident, $loading_indication:expr, $senders:expr, $plugin_id:expr) => {{
        $loading_indication.$loading_stage();
        drop(
            $senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                $plugin_id,
                $loading_indication.clone(),
            )),
        );
    }};
}

pub struct PluginLoader<'a> {
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_path: PathBuf,
    loading_indication: &'a mut LoadingIndication,
    senders: ThreadSenders,
    plugin_id: PluginId,
    client_id: ClientId,
    engine: Engine,
    plugin: PluginConfig,
    plugin_dir: &'a PathBuf,
    tab_index: Option<usize>,
    plugin_own_data_dir: PathBuf,
    plugin_own_cache_dir: PathBuf,
    size: Size,
    path_to_default_shell: PathBuf,
    plugin_cwd: PathBuf,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
    default_layout: Box<Layout>,
    layout_dir: Option<PathBuf>,
    default_mode: InputMode,
    keybinds: Keybinds,
}

impl<'a> PluginLoader<'a> {
    pub fn reload_plugin_from_memory(
        plugin_id: PluginId,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        engine: Engine,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
        path_to_default_shell: PathBuf,
        plugin_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
    ) -> Result<()> {
        let err_context = || format!("failed to reload plugin {plugin_id} from memory");
        let mut connected_clients: Vec<ClientId> =
            connected_clients.lock().unwrap().iter().copied().collect();
        if connected_clients.is_empty() {
            return Err(anyhow!("No connected clients, cannot reload plugin"));
        }
        let first_client_id = connected_clients.remove(0);

        let mut plugin_loader = PluginLoader::new_from_existing_plugin_attributes(
            &plugin_cache,
            &plugin_map,
            loading_indication,
            &senders,
            plugin_id,
            first_client_id,
            engine,
            &plugin_dir,
            path_to_default_shell,
            Some(plugin_cwd),
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
        )?;
        plugin_loader
            .load_module_from_memory()
            .and_then(|module| plugin_loader.create_plugin_environment(module))
            .and_then(|(store, instance)| {
                plugin_loader.load_plugin_instance(store, &instance, &plugin_map)
            })
            .and_then(|_| {
                plugin_loader.clone_instance_for_other_clients(&connected_clients, &plugin_map)
            })
            .with_context(err_context)?;
        display_loading_stage!(end, loading_indication, senders, plugin_id);
        Ok(())
    }

    pub fn start_plugin(
        plugin_id: PluginId,
        client_id: ClientId,
        plugin: &PluginConfig,
        tab_index: Option<usize>,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        engine: Engine,
        plugin_map: Arc<Mutex<PluginMap>>,
        size: Size,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
        path_to_default_shell: PathBuf,
        plugin_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        skip_cache: bool,
        layout_dir: Option<PathBuf>,
        default_mode: InputMode,
        keybinds: Keybinds,
    ) -> Result<()> {
        log::info!("start plugin 1");
        let err_context = || format!("failed to start plugin {plugin_id} for client {client_id}");
        let mut plugin_loader = PluginLoader::new(
            &plugin_cache,
            loading_indication,
            &senders,
            plugin_id,
            client_id,
            engine,
            plugin.clone(),
            &plugin_dir,
            tab_index,
            size,
            path_to_default_shell,
            plugin_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
            default_mode,
            keybinds,
        )?;
        log::info!("start plugin 2");
        if true { // TODO: proper fix
            log::info!("start plugin 3");
            plugin_loader
                .compile_module()
                .and_then(|module| plugin_loader.create_plugin_environment(module))
                .and_then(|(store, instance)| {
                    plugin_loader.load_plugin_instance(store, &instance, &plugin_map)
                })
                .and_then(|_| {
                    plugin_loader.clone_instance_for_other_clients(
                        &connected_clients.lock().unwrap(),
                        &plugin_map,
                    )
                })
                .with_context(err_context)?;
        } else {
            log::info!("start plugin 4");
            plugin_loader
                .load_module_from_memory()
                .or_else(|_e| plugin_loader.load_module_from_hd_cache())
                .or_else(|_e| plugin_loader.compile_module())
                .and_then(|module| plugin_loader.create_plugin_environment(module))
                .and_then(|(store, instance)| {
                    plugin_loader.load_plugin_instance(store, &instance, &plugin_map)
                })
                .and_then(|_| {
                    plugin_loader.clone_instance_for_other_clients(
                        &connected_clients.lock().unwrap(),
                        &plugin_map,
                    )
                })
                .with_context(err_context)?;
        };
        display_loading_stage!(end, loading_indication, senders, plugin_id);
        Ok(())
    }

    pub fn add_client(
        client_id: ClientId,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        engine: Engine,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
        path_to_default_shell: PathBuf,
        plugin_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
        default_mode: InputMode,
        keybinds: Keybinds,
    ) -> Result<()> {
        let mut new_plugins = HashSet::new();
        for plugin_id in plugin_map.lock().unwrap().plugin_ids() {
            new_plugins.insert((plugin_id, client_id));
        }
        for (plugin_id, existing_client_id) in new_plugins {
            let mut plugin_loader = PluginLoader::new_from_different_client_id(
                &plugin_cache,
                &plugin_map,
                loading_indication,
                &senders,
                plugin_id,
                existing_client_id,
                engine.clone(),
                &plugin_dir,
                path_to_default_shell.clone(),
                plugin_cwd.clone(),
                capabilities.clone(),
                client_attributes.clone(),
                default_shell.clone(),
                default_layout.clone(),
                layout_dir.clone(),
                default_mode,
                keybinds.clone(),
            )?;
            plugin_loader
                .load_module_from_memory()
                .and_then(|module| plugin_loader.create_plugin_environment(module))
                .and_then(|(store, instance)| {
                    plugin_loader.load_plugin_instance(store, &instance, &plugin_map)
                })?
        }
        connected_clients.lock().unwrap().push(client_id);
        Ok(())
    }

    pub fn reload_plugin(
        plugin_id: PluginId,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        engine: Engine,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
        path_to_default_shell: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
    ) -> Result<()> {
        let err_context = || format!("failed to reload plugin id {plugin_id}");

        let mut connected_clients: Vec<ClientId> =
            connected_clients.lock().unwrap().iter().copied().collect();
        if connected_clients.is_empty() {
            return Err(anyhow!("No connected clients, cannot reload plugin"));
        }
        let first_client_id = connected_clients.remove(0);

        let mut plugin_loader = PluginLoader::new_from_existing_plugin_attributes(
            &plugin_cache,
            &plugin_map,
            loading_indication,
            &senders,
            plugin_id,
            first_client_id,
            engine,
            &plugin_dir,
            path_to_default_shell,
            None,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
        )?;
        plugin_loader
            .compile_module()
            .and_then(|module| plugin_loader.create_plugin_environment(module))
            .and_then(|(store, instance)| {
                plugin_loader.load_plugin_instance(store, &instance, &plugin_map)
            })
            .and_then(|_| {
                plugin_loader.clone_instance_for_other_clients(&connected_clients, &plugin_map)
            })
            .with_context(err_context)?;
        display_loading_stage!(end, loading_indication, senders, plugin_id);
        Ok(())
    }
    pub fn new(
        plugin_cache: &Arc<Mutex<HashMap<PathBuf, Module>>>,
        loading_indication: &'a mut LoadingIndication,
        senders: &ThreadSenders,
        plugin_id: PluginId,
        client_id: ClientId,
        engine: Engine,
        plugin: PluginConfig,
        plugin_dir: &'a PathBuf,
        tab_index: Option<usize>,
        size: Size,
        path_to_default_shell: PathBuf,
        plugin_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
        default_mode: InputMode,
        keybinds: Keybinds,
    ) -> Result<Self> {
        let plugin_own_data_dir = ZELLIJ_SESSION_CACHE_DIR
            .join(Url::from(&plugin.location).to_string())
            .join(format!("{}-{}", plugin_id, client_id));
        let plugin_own_cache_dir = ZELLIJ_SESSION_CACHE_DIR
            .join(Url::from(&plugin.location).to_string())
            .join(format!("plugin_cache"));
        create_plugin_fs_entries(&plugin_own_data_dir, &plugin_own_cache_dir)?;
        let plugin_path = plugin.path.clone();
        Ok(PluginLoader {
            plugin_cache: plugin_cache.clone(),
            plugin_path,
            loading_indication,
            senders: senders.clone(),
            plugin_id,
            client_id,
            engine,
            plugin,
            plugin_dir,
            tab_index,
            plugin_own_data_dir,
            plugin_own_cache_dir,
            size,
            path_to_default_shell,
            plugin_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
            default_mode,
            keybinds,
        })
    }
    pub fn new_from_existing_plugin_attributes(
        plugin_cache: &Arc<Mutex<HashMap<PathBuf, Module>>>,
        plugin_map: &Arc<Mutex<PluginMap>>,
        loading_indication: &'a mut LoadingIndication,
        senders: &ThreadSenders,
        plugin_id: PluginId,
        client_id: ClientId,
        engine: Engine,
        plugin_dir: &'a PathBuf,
        path_to_default_shell: PathBuf,
        cwd: Option<PathBuf>,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
    ) -> Result<Self> {
        let err_context = || "Failed to find existing plugin";
        let (running_plugin, _subscriptions, _workers) = {
            let mut plugin_map = plugin_map.lock().unwrap();
            plugin_map
                .remove_single_plugin(plugin_id, client_id)
                .with_context(err_context)?
        };
        let running_plugin = running_plugin.lock().unwrap();
        let tab_index = running_plugin.store.data().tab_index;
        let size = Size {
            rows: running_plugin.rows,
            cols: running_plugin.columns,
        };
        let keybinds = running_plugin.store.data().keybinds.clone();
        let default_mode = running_plugin.store.data().default_mode;
        let plugin_config = running_plugin.store.data().plugin.clone();
        // prefer the explicitly given cwd, otherwise copy it from the running plugin
        // (when reloading a plugin, we want to copy it, when starting a new plugin instance from
        // meomory, we want to reset it)
        let plugin_cwd = cwd.unwrap_or_else(|| running_plugin.store.data().plugin_cwd.clone());
        loading_indication.set_name(running_plugin.store.data().name());
        PluginLoader::new(
            plugin_cache,
            loading_indication,
            senders,
            plugin_id,
            client_id,
            engine,
            plugin_config,
            plugin_dir,
            tab_index,
            size,
            path_to_default_shell,
            plugin_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
            default_mode,
            keybinds,
        )
    }
    pub fn new_from_different_client_id(
        plugin_cache: &Arc<Mutex<HashMap<PathBuf, Module>>>,
        plugin_map: &Arc<Mutex<PluginMap>>,
        loading_indication: &'a mut LoadingIndication,
        senders: &ThreadSenders,
        plugin_id: PluginId,
        client_id: ClientId,
        engine: Engine,
        plugin_dir: &'a PathBuf,
        path_to_default_shell: PathBuf,
        plugin_cwd: PathBuf,
        capabilities: PluginCapabilities,
        client_attributes: ClientAttributes,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        layout_dir: Option<PathBuf>,
        default_mode: InputMode,
        keybinds: Keybinds,
    ) -> Result<Self> {
        let err_context = || "Failed to find existing plugin";
        let running_plugin = {
            let plugin_map = plugin_map.lock().unwrap();
            plugin_map
                .get_running_plugin(plugin_id, None)
                .with_context(err_context)?
                .clone()
        };
        let running_plugin = running_plugin.lock().unwrap();
        let tab_index = running_plugin.store.data().tab_index;
        let size = Size {
            rows: running_plugin.rows,
            cols: running_plugin.columns,
        };
        let plugin_config = running_plugin.store.data().plugin.clone();
        loading_indication.set_name(running_plugin.store.data().name());
        PluginLoader::new(
            plugin_cache,
            loading_indication,
            senders,
            plugin_id,
            client_id,
            engine,
            plugin_config,
            plugin_dir,
            tab_index,
            size,
            path_to_default_shell,
            plugin_cwd,
            capabilities,
            client_attributes,
            default_shell,
            default_layout,
            layout_dir,
            default_mode,
            keybinds,
        )
    }
    pub fn load_module_from_memory(&mut self) -> Result<Module> {
        display_loading_stage!(
            indicate_loading_plugin_from_memory,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        let module = self
            .plugin_cache
            .lock()
            .unwrap()
            .remove(&self.plugin_path)
            .ok_or(anyhow!("Plugin is not stored in memory"))?;
        display_loading_stage!(
            indicate_loading_plugin_from_memory_success,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        Ok(module)
    }
    pub fn load_module_from_hd_cache(&mut self) -> Result<Module> {
        display_loading_stage!(
            indicate_loading_plugin_from_memory_notfound,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        // Wasmi doesn't support module serialization, fall back to compilation
        Err(anyhow!("HD cache not supported with Wasmi"))
    }
    pub fn compile_module(&mut self) -> Result<Module> {
        self.loading_indication.override_previous_error();
        display_loading_stage!(
            indicate_loading_plugin_from_hd_cache_notfound,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        display_loading_stage!(
            indicate_compiling_plugin,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        let wasm_bytes = self.plugin.resolve_wasm_bytes(&self.plugin_dir)?;
        let timer = std::time::Instant::now();
        let module = Module::new(&self.engine, &wasm_bytes)?;
        log::info!(
            "Loaded plugin '{}' in {:?}",
            self.plugin_path.display(),
            timer.elapsed()
        );
        Ok(module)
    }
    pub fn create_plugin_environment(
        &mut self,
        module: Module,
    ) -> Result<(Store<PluginEnv>, Instance)> {
        let (store, instance) = self.create_plugin_instance_env(&module)?;
        // Only do an insert when everything went well!
        let cloned_plugin = self.plugin.clone();
        self.plugin_cache
            .lock()
            .unwrap()
            .insert(cloned_plugin.path, module);
        Ok((store, instance))
    }
    pub fn create_plugin_instance_and_wasi_env_for_worker(
        &mut self,
    ) -> Result<(Store<PluginEnv>, Instance)> {
        let err_context = || {
            format!(
                "Failed to create instance and plugin env for worker {}",
                self.plugin_id
            )
        };
        let module = self
            .plugin_cache
            .lock()
            .unwrap()
            .get(&self.plugin.path)
            .with_context(err_context)?
            .clone();
        let (store, instance) = self.create_plugin_instance_env(&module)?;
        Ok((store, instance))
    }
    pub fn load_plugin_instance(
        &mut self,
        mut store: Store<PluginEnv>,
        instance: &Instance,
        plugin_map: &Arc<Mutex<PluginMap>>,
    ) -> Result<()> {
        let err_context = || format!("failed to load plugin from instance {instance:#?}");
        let main_user_instance = instance.clone();
        display_loading_stage!(
            indicate_starting_plugin,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        let start_function = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .with_context(err_context)?;
        let load_function = instance
            .get_typed_func::<(), ()>(&mut store, "load")
            .with_context(err_context)?;
        let mut workers = HashMap::new();
        for function_name in instance
            .exports(&mut store)
            .filter_map(|export| export.clone().into_func().map(|_| export.name()))
        {
            if function_name.ends_with("_worker") {
                let (mut store, instance) =
                    self.create_plugin_instance_and_wasi_env_for_worker()?;

                let start_function_for_worker = instance
                    .get_typed_func::<(), ()>(&mut store, "_start")
                    .with_context(err_context)?;
                start_function_for_worker
                    .call(&mut store, ())
                    .with_context(err_context)?;

                let worker = RunningWorker::new(store, instance, &function_name);
                let worker_sender = plugin_worker(worker);
                workers.insert(function_name.into(), worker_sender);
            }
        }

        let subscriptions = store.data().subscriptions.clone();
        let plugin = Arc::new(Mutex::new(RunningPlugin::new(
            store,
            main_user_instance,
            self.size.rows,
            self.size.cols,
        )));
        plugin_map.lock().unwrap().insert(
            self.plugin_id,
            self.client_id,
            plugin.clone(),
            subscriptions,
            workers,
        );

        start_function
            .call(&mut plugin.lock().unwrap().store, ())
            .with_context(err_context)?;

        let protobuf_plugin_configuration: ProtobufPluginConfiguration = self
            .plugin
            .userspace_configuration
            .clone()
            .try_into()
            .map_err(|e| anyhow!("Failed to serialize user configuration: {:?}", e))?;
        let protobuf_bytes = protobuf_plugin_configuration.encode_to_vec();
        wasi_write_object(plugin.lock().unwrap().store.data(), &protobuf_bytes)
            .with_context(err_context)?;
        load_function
            .call(&mut plugin.lock().unwrap().store, ())
            .with_context(err_context)?;

        display_loading_stage!(
            indicate_starting_plugin_success,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        display_loading_stage!(
            indicate_writing_plugin_to_cache,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        display_loading_stage!(
            indicate_writing_plugin_to_cache_success,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );

        Ok(())
    }
    pub fn clone_instance_for_other_clients(
        &mut self,
        connected_clients: &[ClientId],
        plugin_map: &Arc<Mutex<PluginMap>>,
    ) -> Result<()> {
        if !connected_clients.is_empty() {
            display_loading_stage!(
                indicate_cloning_plugin_for_other_clients,
                self.loading_indication,
                self.senders,
                self.plugin_id
            );
            for client_id in connected_clients {
                if client_id == &self.client_id {
                    // don't reload the plugin once more for ourselves
                    continue;
                }
                let mut loading_indication = LoadingIndication::new("".into());
                let mut plugin_loader_for_client = PluginLoader::new_from_different_client_id(
                    &self.plugin_cache.clone(),
                    &plugin_map,
                    &mut loading_indication,
                    &self.senders.clone(),
                    self.plugin_id,
                    *client_id,
                    self.engine.clone(),
                    &self.plugin_dir,
                    self.path_to_default_shell.clone(),
                    self.plugin_cwd.clone(),
                    self.capabilities.clone(),
                    self.client_attributes.clone(),
                    self.default_shell.clone(),
                    self.default_layout.clone(),
                    self.layout_dir.clone(),
                    self.default_mode,
                    self.keybinds.clone(),
                )?;
                plugin_loader_for_client
                    .load_module_from_memory()
                    .and_then(|module| plugin_loader_for_client.create_plugin_environment(module))
                    .and_then(|(store, instance)| {
                        plugin_loader_for_client.load_plugin_instance(store, &instance, plugin_map)
                    })?
            }
            display_loading_stage!(
                indicate_cloning_plugin_for_other_clients_success,
                self.loading_indication,
                self.senders,
                self.plugin_id
            );
        }
        Ok(())
    }
    pub fn create_wasi_ctx(
        host_dir: &PathBuf,
        data_dir: &PathBuf,
        cache_dir: &PathBuf,
        tmp_dir: &PathBuf,
        plugin_url: &String,
        plugin_id: PluginId,
        stdin_pipe: Arc<Mutex<VecDeque<u8>>>,
        stdout_pipe: Arc<Mutex<VecDeque<u8>>>,
    ) -> Result<WasiCtx> {
        let err_context = || format!("Failed to create wasi_ctx");
        let dirs = vec![
            ("/host".to_owned(), host_dir.clone()),
            ("/data".to_owned(), data_dir.clone()),
            ("/cache".to_owned(), cache_dir.clone()),
            ("/tmp".to_owned(), tmp_dir.clone()),
        ];
        let dirs = dirs.into_iter().filter(|(_dir_name, dir)| {
            // note that this does not protect against TOCTOU errors
            // eg. if one or more of these folders existed at the time of check but was deleted
            // before we mounted in in the wasi environment, we'll crash
            // when we move to a new wasi environment, we should address this with locking if
            // there's no built-in solution
            dir.try_exists().ok().unwrap_or(false)
        });
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_env()?;
        builder.inherit_stdio();
        let wasi_ctx = builder.build();
        Ok(wasi_ctx)
    }
    fn create_plugin_instance_env(&self, module: &Module) -> Result<(Store<PluginEnv>, Instance)> {
        let err_context = || {
            format!(
                "Failed to create instance, plugin env and subscriptions for plugin {}",
                self.plugin_id
            )
        };
        let stdin_pipe = Arc::new(Mutex::new(VecDeque::new()));
        let stdout_pipe = Arc::new(Mutex::new(VecDeque::new()));

        let wasi_ctx = PluginLoader::create_wasi_ctx(
            &self.plugin_cwd,
            &self.plugin_own_data_dir,
            &self.plugin_own_cache_dir,
            &ZELLIJ_TMP_DIR,
            &self.plugin.location.to_string(),
            self.plugin_id,
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )?;
        let plugin = self.plugin.clone();
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            client_id: self.client_id,
            plugin,
            permissions: Arc::new(Mutex::new(None)),
            senders: self.senders.clone(),
            wasi_ctx,
            plugin_own_data_dir: self.plugin_own_data_dir.clone(),
            plugin_own_cache_dir: self.plugin_own_cache_dir.clone(),
            tab_index: self.tab_index,
            path_to_default_shell: self.path_to_default_shell.clone(),
            capabilities: self.capabilities.clone(),
            client_attributes: self.client_attributes.clone(),
            default_shell: self.default_shell.clone(),
            default_layout: self.default_layout.clone(),
            plugin_cwd: self.plugin_cwd.clone(),
            input_pipes_to_unblock: Arc::new(Mutex::new(HashSet::new())),
            input_pipes_to_block: Arc::new(Mutex::new(HashSet::new())),
            layout_dir: self.layout_dir.clone(),
            default_mode: self.default_mode.clone(),
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            keybinds: self.keybinds.clone(),
            intercepting_key_presses: false,
            stdin_pipe,
            stdout_pipe,
        };
        let mut store = Store::new(&self.engine, plugin_env);

        let mut linker = Linker::new(&self.engine);
        wasmi_wasi::add_to_linker(&mut linker, |plugin_env: &mut PluginEnv| {
            &mut plugin_env.wasi_ctx
        })?;
        zellij_exports(&mut linker);

        let instance = linker
            .instantiate(&mut store, module)
            .and_then(|pre| pre.start(&mut store))
            .with_context(err_context)?;

        if let Some(func) = instance.get_func(&mut store, "_initialize") {
            if let Ok(typed_func) = func.typed::<(), ()>(&store) {
                let _ = typed_func.call(&mut store, ());
            }
        }

        Ok((store, instance))
    }
}

fn create_plugin_fs_entries(
    plugin_own_data_dir: &PathBuf,
    plugin_own_cache_dir: &PathBuf,
) -> Result<()> {
    let err_context = || "failed to create plugin fs entries";
    // Create filesystem entries mounted into WASM.
    // We create them here to get expressive error messages in case they fail.
    fs::create_dir_all(&plugin_own_data_dir)
        .with_context(|| format!("failed to create datadir in {plugin_own_data_dir:?}"))
        .with_context(err_context)?;
    fs::create_dir_all(&plugin_own_cache_dir)
        .with_context(|| format!("failed to create cache dir in {plugin_own_cache_dir:?}"))
        .with_context(err_context)?;
    fs::create_dir_all(ZELLIJ_TMP_DIR.as_path())
        .with_context(|| format!("failed to create tmpdir at {:?}", &ZELLIJ_TMP_DIR.as_path()))
        .with_context(err_context)?;
    Ok(())
}
