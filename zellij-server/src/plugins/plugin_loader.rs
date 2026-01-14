use crate::plugins::plugin_map::{
    PluginEnv, PluginMap, RunningPlugin, VecDequeInputStream, WriteOutputStream,
};
use crate::plugins::plugin_worker::{plugin_worker, RunningWorker};
use crate::plugins::wasm_bridge::{LoadingContext, PluginCache};
use crate::plugins::zellij_exports::{wasi_write_object, zellij_exports};
use crate::plugins::PluginId;
use prost::Message;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use wasmi::{Engine, Instance, Linker, Module, Store, StoreLimits};
use wasmi_wasi::sync::WasiCtxBuilder;
use wasmi_wasi::wasi_common::pipe::{ReadPipe, WritePipe};
use wasmi_wasi::Dir;
use wasmi_wasi::WasiCtx;

use crate::{
    logging_pipe::LoggingPipe, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId,
};

use zellij_utils::plugin_api::action::ProtobufPluginConfiguration;
use zellij_utils::{
    consts::ZELLIJ_TMP_DIR,
    data::{InputMode, PluginCapabilities},
    errors::prelude::*,
    input::command::TerminalAction,
    input::keybinds::Keybinds,
    input::layout::Layout,
    input::plugins::PluginConfig,
    ipc::ClientAttributes,
    pane_size::Size,
};

fn create_plugin_fs_entries(plugin_own_data_dir: &PathBuf, plugin_own_cache_dir: &PathBuf) {
    // Create filesystem entries mounted into WASM.
    // We create them here to get expressive error messages in case they fail.
    if let Err(e) = fs::create_dir_all(&plugin_own_data_dir) {
        log::error!("Failed to create plugin data dir: {}", e);
    };
    if let Err(e) = fs::create_dir_all(&plugin_own_cache_dir) {
        log::error!("Failed to create plugin cache dir: {}", e);
    }
    if let Err(e) = fs::create_dir_all(ZELLIJ_TMP_DIR.as_path()) {
        log::error!("Failed to create plugin tmp dir: {}", e);
    }
}

pub struct PluginLoader<'a> {
    skip_cache: bool,
    plugin_id: PluginId,
    client_id: ClientId,
    plugin_cwd: PathBuf,
    plugin_own_data_dir: PathBuf,
    plugin_own_cache_dir: PathBuf,
    plugin_config: PluginConfig,
    tab_index: Option<usize>,
    path_to_default_shell: PathBuf,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
    layout_dir: Option<PathBuf>,
    default_mode: InputMode,
    keybinds: Keybinds,
    plugin_dir: PathBuf,
    size: Size,
    loading_indication: LoadingIndication,
    senders: ThreadSenders,
    engine: Engine,
    default_layout: Box<Layout>,
    plugin_cache: PluginCache,
    plugin_map: &'a mut PluginMap, // we receive a mutable reference rather than the Arc so that it
    // will be held for the lifetime of this struct and thus loading
    // plugins for all connected clients will be one transaction
    connected_clients: Option<Arc<Mutex<Vec<ClientId>>>>,
}

impl<'a> PluginLoader<'a> {
    pub fn new(
        skip_cache: bool,
        loading_context: LoadingContext,
        senders: ThreadSenders,
        engine: Engine,
        default_layout: Box<Layout>,
        plugin_cache: PluginCache,
        plugin_map: &'a mut PluginMap,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
    ) -> Self {
        let loading_indication = LoadingIndication::new("".into());
        create_plugin_fs_entries(
            &loading_context.plugin_own_data_dir,
            &loading_context.plugin_own_cache_dir,
        );
        Self {
            plugin_id: loading_context.plugin_id,
            client_id: loading_context.client_id,
            plugin_cwd: loading_context.plugin_cwd,
            plugin_own_data_dir: loading_context.plugin_own_data_dir,
            plugin_own_cache_dir: loading_context.plugin_own_cache_dir,
            plugin_config: loading_context.plugin_config,
            tab_index: loading_context.tab_index,
            path_to_default_shell: loading_context.path_to_default_shell,
            capabilities: loading_context.capabilities,
            client_attributes: loading_context.client_attributes,
            default_shell: loading_context.default_shell,
            layout_dir: loading_context.layout_dir,
            default_mode: loading_context.default_mode,
            keybinds: loading_context.keybinds,
            plugin_dir: loading_context.plugin_dir,
            size: loading_context.size,

            skip_cache,
            senders,
            engine,
            default_layout,
            plugin_cache,
            plugin_map,
            connected_clients: Some(connected_clients),
            loading_indication,
        }
    }
    pub fn without_connected_clients(mut self) -> Self {
        self.connected_clients = None;
        self
    }
    pub fn start_plugin(&mut self) -> Result<()> {
        let module = if self.skip_cache {
            self.interpret_module()?
        } else {
            self.load_module_from_memory()
                .or_else(|_e| self.interpret_module())?
        };
        let (store, instance) = self.create_plugin_environment(module)?;
        self.load_plugin_instance(store, &instance)?;
        self.clone_instance_for_other_clients()?;
        Ok(())
    }
    fn interpret_module(&mut self) -> Result<Module> {
        self.loading_indication.override_previous_error();
        let wasm_bytes = self.plugin_config.resolve_wasm_bytes(&self.plugin_dir)?;
        let timer = std::time::Instant::now();
        let module = Module::new(&self.engine, &wasm_bytes)?;
        log::info!(
            "Loaded plugin '{}' in {:?}",
            self.plugin_config.path.display(),
            timer.elapsed()
        );
        Ok(module)
    }
    fn load_module_from_memory(&mut self) -> Result<Module> {
        let module = self
            .plugin_cache
            .lock()
            .unwrap()
            .remove(&self.plugin_config.path) // TODO: do we still bring it back later?
            // maybe we can forgo this dance?
            .ok_or(anyhow!("Plugin is not stored in memory"))?;
        Ok(module)
    }
    fn load_plugin_instance(
        &mut self,
        mut store: Store<PluginEnv>,
        instance: &Instance,
    ) -> Result<()> {
        let err_context = || format!("failed to load plugin from instance {instance:#?}");
        let main_user_instance = instance.clone();
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
        self.plugin_map.insert(
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
            .plugin_config
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

        Ok(())
    }
    pub fn create_plugin_environment(
        &self,
        module: Module,
    ) -> Result<(Store<PluginEnv>, Instance)> {
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
            &self.plugin_config.location.to_string(),
            self.plugin_id,
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )?;
        let plugin_path = self.plugin_config.path.clone();
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            client_id: self.client_id,
            plugin: self.plugin_config.clone(), // TODO: change field name in PluginEnv to plugin_config
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
            store_limits: create_optimized_store_limits(),
        };
        let mut store = Store::new(&self.engine, plugin_env);

        // Apply optimized resource limits for memory efficiency
        store.limiter(|plugin_env| &mut plugin_env.store_limits);

        let mut linker = Linker::new(&self.engine);
        wasmi_wasi::add_to_linker(&mut linker, |plugin_env: &mut PluginEnv| {
            &mut plugin_env.wasi_ctx
        })?;
        zellij_exports(&mut linker);

        let instance = linker
            .instantiate_and_start(&mut store, &module)
            .with_context(err_context)?;

        if let Some(func) = instance.get_func(&mut store, "_initialize") {
            if let Ok(typed_func) = func.typed::<(), ()>(&store) {
                let _ = typed_func.call(&mut store, ());
            }
        }

        self.plugin_cache
            .lock()
            .unwrap()
            .insert(plugin_path.clone(), module);
        Ok((store, instance))
    }
    pub fn clone_instance_for_other_clients(&mut self) -> Result<()> {
        let Some(connected_clients) = self.connected_clients.as_ref() else {
            return Ok(());
        };
        let connected_clients: Vec<ClientId> =
            connected_clients.lock().unwrap().iter().copied().collect();
        if !connected_clients.is_empty() {
            self.connected_clients = None; // so we don't have infinite loops
            for client_id in connected_clients {
                if client_id == self.client_id {
                    // don't reload the plugin once more for ourselves
                    continue;
                }
                self.client_id = client_id;
                self.start_plugin()?;
            }
        }
        Ok(())
    }
    pub fn create_plugin_instance_and_wasi_env_for_worker(
        &self,
    ) -> Result<(Store<PluginEnv>, Instance)> {
        let plugin_id = self.plugin_id;
        let err_context = || {
            format!(
                "Failed to create instance and plugin env for worker {}",
                plugin_id
            )
        };
        let module = self
            .plugin_cache
            .lock()
            .unwrap()
            .get(&self.plugin_config.path)
            .with_context(err_context)?
            .clone();
        let (store, instance) = self.create_plugin_instance_env(&module)?;
        Ok((store, instance))
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
            &self.plugin_config.location.to_string(),
            self.plugin_id,
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )?;
        let plugin_config = self.plugin_config.clone();
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            client_id: self.client_id,
            plugin: plugin_config,
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
            store_limits: create_optimized_store_limits(),
        };
        let mut store = Store::new(&self.engine, plugin_env);

        // Apply optimized resource limits for memory efficiency
        store.limiter(|plugin_env| &mut plugin_env.store_limits);

        let mut linker = Linker::new(&self.engine);
        wasmi_wasi::add_to_linker(&mut linker, |plugin_env: &mut PluginEnv| {
            &mut plugin_env.wasi_ctx
        })?;
        zellij_exports(&mut linker);

        let instance = linker
            .instantiate_and_start(&mut store, module)
            .with_context(err_context)?;

        if let Some(func) = instance.get_func(&mut store, "_initialize") {
            if let Ok(typed_func) = func.typed::<(), ()>(&store) {
                let _ = typed_func.call(&mut store, ());
            }
        }

        Ok((store, instance))
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
        let _err_context = || format!("Failed to create wasi_ctx");
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

        // Mount directories using the builder
        for (guest_path, host_path) in dirs {
            match std::fs::File::open(&host_path) {
                Ok(dir_file) => {
                    let dir = Dir::from_std_file(dir_file);
                    builder.preopened_dir(dir, guest_path)?;
                },
                Err(e) => {
                    log::warn!("Failed to mount directory {:?}: {}", host_path, e);
                },
            }
        }

        let ctx = builder.build();

        // Set up custom stdin/stdout/stderr
        ctx.set_stdin(Box::new(ReadPipe::new(VecDequeInputStream(
            stdin_pipe.clone(),
        ))));
        ctx.set_stdout(Box::new(WritePipe::new(WriteOutputStream(
            stdout_pipe.clone(),
        ))));
        ctx.set_stderr(Box::new(WritePipe::new(WriteOutputStream(Arc::new(
            Mutex::new(LoggingPipe::new(plugin_url, plugin_id)),
        )))));

        Ok(ctx)
    }
}

fn create_optimized_store_limits() -> StoreLimits {
    use wasmi::StoreLimitsBuilder;
    StoreLimitsBuilder::new()
        .instances(1) // One instance per plugin
        .memories(4) // Max 4 linear memories per plugin
        .memory_size(16 * 1024 * 1024) // 16MB per memory maximum
        .tables(16) // Small table element limit
        .trap_on_grow_failure(true) // Fail fast on resource exhaustion
        .build()
}
