use crate::plugins::plugin_map::{
    PluginEnv, PluginMap, PluginMetadata, RunningPlugin, SharedEnv, VecDequeInputStream,
    WriteOutputStream,
};
use crate::plugins::plugin_worker::{plugin_worker, MessageToWorker, RunningWorker};
use crate::plugins::wasm_bridge::{
    clear_failed_plugin_load_side_effects, LoadingContext, PluginCache,
};
use crate::plugins::zellij_exports::{wasi_write_object, zellij_exports};
use crate::plugins::PluginId;
use prost::Message;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use wasmi::{Engine, Instance, Linker, Module, Store, StoreLimits};
use wasmi_wasi::sync::WasiCtxBuilder;
use wasmi_wasi::wasi_common::pipe::{ReadPipe, WritePipe};
use wasmi_wasi::Dir;
use wasmi_wasi::WasiCtx;

use crate::{
    logging_pipe::LoggingPipe, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId, SharedKeybinds,
};

use zellij_utils::plugin_api::action::ProtobufPluginConfiguration;
use zellij_utils::{
    consts::ZELLIJ_TMP_DIR, data::InputMode, errors::prelude::*, input::command::TerminalAction,
    input::plugins::PluginConfig, pane_size::Size,
};

/// Open a directory as a `File` handle for WASI pre-opening.
/// On Windows, `FILE_FLAG_BACKUP_SEMANTICS` is required to open directories.
#[cfg(not(windows))]
fn open_dir(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

#[cfg(windows)]
fn open_dir(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(0x02000000) // FILE_FLAG_BACKUP_SEMANTICS
        .open(path)
}

// The supported initialization ABI: if a module exports `_initialize`, it
// must be `() -> ()` and must not trap. Either failure aborts the load so a
// failed initializer can never reach the running plugin map.
fn run_initialize_hook<T>(
    store: &mut Store<T>,
    instance: &Instance,
    plugin_id: PluginId,
) -> Result<()> {
    let Some(func) = instance.get_func(&mut *store, "_initialize") else {
        return Ok(());
    };
    let typed_func = func.typed::<(), ()>(&*store).with_context(|| {
        format!("plugin {plugin_id} exports _initialize with an unsupported signature")
    })?;
    crate::plugins::shared::refuel_for_callback(&mut *store);
    typed_func
        .call(&mut *store, ())
        .with_context(|| format!("plugin {plugin_id} initialization failed"))?;
    Ok(())
}

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

pub struct PluginLoader {
    skip_cache: bool,
    plugin_id: PluginId,
    generation: u64,
    client_id: ClientId,
    plugin_cwd: PathBuf,
    plugin_own_data_dir: PathBuf,
    plugin_own_cache_dir: PathBuf,
    plugin_config: PluginConfig,
    tab_index: Option<usize>,
    path_to_default_shell: PathBuf,
    session_env_vars: std::collections::BTreeMap<String, String>,
    default_shell: Option<TerminalAction>,
    layout_dir: Option<PathBuf>,
    default_mode: InputMode,
    keybinds: SharedKeybinds,
    plugin_dir: PathBuf,
    size: Size,
    loading_indication: LoadingIndication,
    senders: ThreadSenders,
    engine: Engine,
    plugin_cache: PluginCache,
    plugin_map: Arc<Mutex<PluginMap>>,
    plugin_digest: String,
    // The module this load runs, kept so that the other client instances and the workers use
    // exactly the module (and digest) of the first instance rather than looking the path up
    // again, where a concurrent load of the same path may have put something else.
    module: Option<Module>,
    // Bytes read for shared/non-shared classification, compiled and hashed by this load instead
    // of being read again (possibly after the file changed).
    wasm_bytes: Option<Vec<u8>>,
    // Instances this loader published, so a failure only rolls back its own work.
    published_instances: Vec<(ClientId, Arc<Mutex<RunningPlugin>>)>,
    published_shared_instance: Option<Arc<Mutex<RunningPlugin>>>,
    quarantined: Arc<std::sync::atomic::AtomicBool>,
    // Set when the plugin was unloaded while this loader was still publishing instances.
    unloaded_during_load: bool,
    // Whether this load publishes a shared instance, whose rollback is handled by its caller.
    shared_load: bool,
    connected_clients: Option<Arc<Mutex<Vec<ClientId>>>>,
}

struct LoadSnapshot {
    metadata: Option<PluginMetadata>,
    had_instance: bool,
}

impl PluginLoader {
    pub fn new(
        skip_cache: bool,
        loading_context: LoadingContext,
        senders: ThreadSenders,
        engine: Engine,
        plugin_cache: PluginCache,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
    ) -> Self {
        let loading_indication = LoadingIndication::new("".into());
        create_plugin_fs_entries(
            &loading_context.plugin_own_data_dir,
            &loading_context.plugin_own_cache_dir,
        );
        Self {
            plugin_id: loading_context.plugin_id,
            generation: loading_context.generation,
            client_id: loading_context.client_id,
            plugin_cwd: loading_context.plugin_cwd,
            plugin_own_data_dir: loading_context.plugin_own_data_dir,
            plugin_own_cache_dir: loading_context.plugin_own_cache_dir,
            plugin_config: loading_context.plugin_config,
            tab_index: loading_context.tab_index,
            path_to_default_shell: loading_context.path_to_default_shell,
            session_env_vars: loading_context.session_env_vars,
            default_shell: loading_context.default_shell,
            layout_dir: loading_context.layout_dir,
            default_mode: loading_context.default_mode,
            keybinds: loading_context.keybinds,
            plugin_dir: loading_context.plugin_dir,
            size: loading_context.size,

            skip_cache,
            senders,
            engine,
            plugin_cache,
            plugin_map,
            plugin_digest: String::new(),
            module: None,
            wasm_bytes: loading_context.wasm_bytes,
            published_instances: Vec::new(),
            published_shared_instance: None,
            quarantined: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            unloaded_during_load: false,
            shared_load: false,
            connected_clients: Some(connected_clients),
            loading_indication,
        }
    }
    pub fn without_connected_clients(mut self) -> Self {
        self.connected_clients = None;
        self
    }
    pub fn start_plugin(&mut self) -> Result<()> {
        let snapshot = self.load_snapshot();
        self.plugin_map.lock().unwrap().begin_load(self.plugin_id);
        let result = self.start_plugin_instances();
        if result.is_err() {
            if self.unloaded_during_load {
                self.remove_failed_load_data_dir_if_unused(&snapshot);
            } else {
                self.rollback_failed_load(&snapshot);
            }
        }
        self.plugin_map.lock().unwrap().end_load(self.plugin_id);
        match result {
            // the plugin is gone, there is nothing left to report
            Err(_) if self.unloaded_during_load => Ok(()),
            result => result,
        }
    }
    fn start_plugin_instances(&mut self) -> Result<()> {
        self.record_plugin_metadata();
        let module = self.load_module()?;
        let (store, instance) = self.create_plugin_environment(module)?;
        self.load_plugin_instance(store, &instance, false)?;
        self.clone_instance_for_other_clients()?;
        Ok(())
    }
    pub fn start_shared_plugin(&mut self) -> Result<Arc<Mutex<RunningPlugin>>> {
        self.shared_load = true;
        let snapshot = self.load_snapshot();
        let result = (|| {
            let module = self.load_module()?;
            let (mut store, instance) = self.create_plugin_environment(module)?;
            store.data_mut().shared = Some(SharedEnv::default());
            self.load_plugin_instance(store, &instance, true)
        })();
        if result.is_err() {
            self.rollback_failed_load(&snapshot);
        }
        result
    }
    fn load_snapshot(&self) -> LoadSnapshot {
        let plugin_map = self.plugin_map.lock().unwrap();
        LoadSnapshot {
            metadata: plugin_map.metadata(self.plugin_id).cloned(),
            had_instance: plugin_map.has_instance_of(self.plugin_id)
                || plugin_map.shared_running_plugin(self.plugin_id).is_some(),
        }
    }
    fn record_plugin_metadata(&mut self) {
        self.plugin_map.lock().unwrap().insert_metadata(
            self.plugin_id,
            PluginMetadata {
                plugin_config: self.plugin_config.clone(),
                tab_index: self.tab_index,
                rows: self.size.rows,
                columns: self.size.cols,
                cwd: self.plugin_cwd.clone(),
                is_background: self.tab_index.is_none(),
            },
        );
    }
    /// Read the plugin's current bytes once and fingerprint them, so the digest in PluginEnv
    /// and in the module cache always describes the bytes that were actually used. A cached
    /// module is served only if it was compiled from these exact bytes.
    ///
    /// The instances this loader creates for the other connected clients are part of the same
    /// load: they reuse the digest of the first one instead of reading and hashing the file
    /// again, which would also delay them behind the first instance.
    fn load_module(&mut self) -> Result<Module> {
        if let Some(module) = &self.module {
            return Ok(module.clone());
        }
        let wasm_bytes = match self.wasm_bytes.take() {
            Some(wasm_bytes) => wasm_bytes,
            None => self.plugin_config.resolve_wasm_bytes(&self.plugin_dir)?,
        };
        self.plugin_digest = plugin_digest_from_bytes(&wasm_bytes);
        let cached_module = if self.skip_cache {
            None
        } else {
            self.load_module_from_memory()
        };
        let module = match cached_module {
            Some(module) => module,
            None => self.interpret_module(&wasm_bytes)?,
        };
        self.module = Some(module.clone());
        Ok(module)
    }
    fn interpret_module(&mut self, wasm_bytes: &[u8]) -> Result<Module> {
        self.loading_indication.override_previous_error();
        let timer = std::time::Instant::now();
        let module = Module::new(&self.engine, wasm_bytes)?;
        log::info!(
            "Loaded plugin '{}' in {:?}",
            self.plugin_config.path.display(),
            timer.elapsed()
        );
        Ok(module)
    }
    fn load_module_from_memory(&mut self) -> Option<Module> {
        // The cached module is only valid for the bytes it was compiled from; serving it
        // without checking ran stale code after the file changed on disk.
        // The entry stays in the cache: concurrent loads of the same path must not take it from
        // one another.
        let cached = self
            .plugin_cache
            .lock()
            .unwrap()
            .get(&self.plugin_config.path)
            .cloned();
        match cached {
            Some((digest, module)) if digest == self.plugin_digest => Some(module),
            _ => None,
        }
    }
    fn load_plugin_instance(
        &mut self,
        mut store: Store<PluginEnv>,
        instance: &Instance,
        shared: bool,
    ) -> Result<Arc<Mutex<RunningPlugin>>> {
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
                crate::plugins::shared::refuel_for_callback(&mut store);
                start_function_for_worker
                    .call(&mut store, ())
                    .with_context(err_context)?;

                let worker = RunningWorker::new(store, instance, &function_name);
                let worker_sender = plugin_worker(worker);
                workers.insert(function_name.into(), worker_sender);
            }
        }

        let subscriptions = store.data().subscriptions.clone();
        let in_blocking_call = store
            .data()
            .shared
            .as_ref()
            .map(|shared| shared.in_blocking_call.clone());
        let plugin = Arc::new(Mutex::new(RunningPlugin::new(
            store,
            main_user_instance,
            self.size.rows,
            self.size.cols,
        )));
        {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            // Checked under the same lock as the insert, which an unload also takes: either the
            // unload sees this instance and cleans it up, or this load sees the unload.
            if !shared && plugin_map.unloaded_during_load(self.plugin_id) {
                self.unloaded_during_load = true;
                drop(plugin_map);
                for (_worker_name, worker_sender) in workers {
                    drop(worker_sender.send(MessageToWorker::Exit));
                }
                return Err(anyhow!(
                    "plugin {} was unloaded while it was loading",
                    self.plugin_id
                ));
            }
            if shared {
                plugin_map.insert_shared(
                    self.plugin_id,
                    (plugin.clone(), subscriptions, workers),
                    in_blocking_call,
                );
            } else {
                plugin_map.insert(
                    self.plugin_id,
                    self.client_id,
                    plugin.clone(),
                    subscriptions,
                    workers,
                );
            }
            if shared {
                self.published_shared_instance = Some(plugin.clone());
            } else {
                self.published_instances
                    .push((self.client_id, plugin.clone()));
            }
        };

        // The instance is already published in plugin_map at this point
        // so it can resolve itself during _start/load. Any failure below
        // must roll the provisional entry (and its workers) back out —
        // a half-initialized instance must not keep receiving events.
        let init_result: Result<()> = (|| {
            crate::plugins::shared::refuel_for_callback(&mut plugin.lock().unwrap().store);
            start_function
                .call(&mut plugin.lock().unwrap().store, ())
                .with_context(err_context)?;

            let protobuf_plugin_configuration: ProtobufPluginConfiguration = self
                .plugin_config
                .initial_userspace_configuration
                .clone()
                .try_into()
                .map_err(|e| anyhow!("Failed to serialize user configuration: {:?}", e))?;
            let protobuf_bytes = protobuf_plugin_configuration.encode_to_vec();
            wasi_write_object(plugin.lock().unwrap().store.data(), &protobuf_bytes)
                .with_context(err_context)?;
            crate::plugins::shared::refuel_for_callback(&mut plugin.lock().unwrap().store);
            load_function
                .call(&mut plugin.lock().unwrap().store, ())
                .with_context(err_context)?;
            Ok(())
        })();
        if let Err(e) = init_result {
            return Err(e);
        }

        Ok(plugin)
    }

    /// Roll back only the instances this loader published. A concurrent load of the same plugin
    /// id may have replaced any of them already, so each entry is matched by identity first.
    fn rollback_failed_load(&mut self, snapshot: &LoadSnapshot) {
        let mut published_clients = Vec::new();
        let (removed, other_instances_remain) = {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            let mut removed = Vec::new();
            for (client_id, published) in self.published_instances.drain(..) {
                published_clients.push(client_id);
                let still_published = plugin_map
                    .get_running_plugin(self.plugin_id, Some(client_id))
                    .map(|current| Arc::ptr_eq(&current, &published))
                    .unwrap_or(false);
                if still_published {
                    if let Some(assets) =
                        plugin_map.remove_plugin_instance(self.plugin_id, client_id)
                    {
                        removed.push(assets);
                    }
                }
            }
            if let Some(published) = self.published_shared_instance.take() {
                let still_published = plugin_map
                    .shared_running_plugin(self.plugin_id)
                    .map(|(current, _)| Arc::ptr_eq(&current, &published))
                    .unwrap_or(false);
                if still_published {
                    if let Some(assets) = plugin_map.remove_shared(self.plugin_id) {
                        removed.push(assets);
                    }
                }
            }
            let other_instances_remain = plugin_map.has_instance_of(self.plugin_id)
                || plugin_map.shared_running_plugin(self.plugin_id).is_some();
            if !other_instances_remain {
                match snapshot.metadata.clone() {
                    Some(metadata) => plugin_map.insert_metadata(self.plugin_id, metadata),
                    None => plugin_map.remove_metadata(self.plugin_id),
                }
            }
            (removed, other_instances_remain)
        };
        for (_running_plugin, _subscriptions, workers) in removed {
            for (_worker_name, worker_sender) in workers {
                drop(worker_sender.send(MessageToWorker::Exit));
            }
        }
        // Nothing will unload a plugin that never finished loading, so its data directory
        // would stay behind for good. The directory is shared by all instances of the plugin
        // and outlives a reload, so it is only removed when this was a first load that left no
        // instance behind.
        if !snapshot.had_instance && !other_instances_remain {
            self.remove_data_dir();
        }
        if !self.shared_load && !other_instances_remain {
            clear_failed_plugin_load_side_effects(&self.senders, self.plugin_id, published_clients);
        }
    }
    fn remove_failed_load_data_dir_if_unused(&self, snapshot: &LoadSnapshot) {
        if snapshot.had_instance {
            return;
        }
        let unused = {
            let plugin_map = self.plugin_map.lock().unwrap();
            !plugin_map.has_instance_of(self.plugin_id)
                && plugin_map.shared_running_plugin(self.plugin_id).is_none()
                && plugin_map.metadata(self.plugin_id).is_none()
        };
        if unused {
            self.remove_data_dir();
        }
    }
    fn remove_data_dir(&self) {
        if let Err(e) = fs::remove_dir_all(&self.plugin_own_data_dir) {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::error!(
                    "Failed to remove data dir of plugin {} after a failed load: {:?}",
                    self.plugin_id,
                    e
                );
            }
        }
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
            self.plugin_config.is_builtin(),
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )?;
        let plugin_path = self.plugin_config.path.clone();
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            generation: self.generation,
            load_alive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            client_id: self.client_id,
            plugin: self.plugin_config.clone(), // TODO: change field name in PluginEnv to plugin_config
            plugin_digest: self.plugin_digest.clone(),
            permissions: Arc::new(Mutex::new(None)),
            quarantined: self.quarantined.clone(),
            senders: self.senders.clone(),
            wasi_ctx,
            plugin_own_data_dir: self.plugin_own_data_dir.clone(),
            plugin_own_cache_dir: self.plugin_own_cache_dir.clone(),
            path_to_default_shell: self.path_to_default_shell.clone(),
            default_shell: self.default_shell.clone(),
            plugin_cwd: self.plugin_cwd.clone(),
            session_env_vars: self.session_env_vars.clone(),
            input_pipes_to_unblock: Arc::new(Mutex::new(HashSet::new())),
            input_pipes_to_block: Arc::new(Mutex::new(HashSet::new())),
            layout_dir: self.layout_dir.clone(),
            default_mode: self.default_mode.clone(),
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            keybinds: self.keybinds.clone(),
            intercepting_key_presses: false,
            pending_popup_size: None,
            stdin_pipe,
            stdout_pipe,
            store_limits: create_optimized_store_limits(),
            shared: None,
        };
        let mut store = Store::new(&self.engine, plugin_env);

        // Apply optimized resource limits for memory efficiency
        store.limiter(|plugin_env| &mut plugin_env.store_limits);

        let mut linker = Linker::new(&self.engine);
        add_wasi_to_linker(&mut linker, |plugin_env: &mut PluginEnv| {
            &mut plugin_env.wasi_ctx
        })?;
        zellij_exports(&mut linker);

        let instance = crate::plugins::shared::instantiate_metered(&linker, &mut store, &module)
            .with_context(err_context)?;

        run_initialize_hook(&mut store, &instance, self.plugin_id)?;

        self.plugin_cache
            .lock()
            .unwrap()
            .insert(plugin_path.clone(), (self.plugin_digest.clone(), module));
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
                if self
                    .plugin_map
                    .lock()
                    .unwrap()
                    .unloaded_during_load(self.plugin_id)
                {
                    // The plugin was unloaded after the instances published so far were
                    // collected for cleanup; anything published now would never be cleaned up.
                    self.unloaded_during_load = true;
                    break;
                }
                self.client_id = client_id;
                self.start_plugin_instances()?;
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
        let module = self.module.clone().with_context(err_context)?;
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
            self.plugin_config.is_builtin(),
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )?;
        let plugin_config = self.plugin_config.clone();
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            generation: self.generation,
            load_alive: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            client_id: self.client_id,
            plugin: plugin_config,
            plugin_digest: self.plugin_digest.clone(),
            permissions: Arc::new(Mutex::new(None)),
            quarantined: self.quarantined.clone(),
            senders: self.senders.clone(),
            wasi_ctx,
            plugin_own_data_dir: self.plugin_own_data_dir.clone(),
            plugin_own_cache_dir: self.plugin_own_cache_dir.clone(),
            path_to_default_shell: self.path_to_default_shell.clone(),
            default_shell: self.default_shell.clone(),
            plugin_cwd: self.plugin_cwd.clone(),
            session_env_vars: self.session_env_vars.clone(),
            input_pipes_to_unblock: Arc::new(Mutex::new(HashSet::new())),
            input_pipes_to_block: Arc::new(Mutex::new(HashSet::new())),
            layout_dir: self.layout_dir.clone(),
            default_mode: self.default_mode.clone(),
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            keybinds: self.keybinds.clone(),
            intercepting_key_presses: false,
            pending_popup_size: None,
            stdin_pipe,
            stdout_pipe,
            store_limits: create_optimized_store_limits(),
            shared: None,
        };
        let mut store = Store::new(&self.engine, plugin_env);

        // Apply optimized resource limits for memory efficiency
        store.limiter(|plugin_env| &mut plugin_env.store_limits);

        let mut linker = Linker::new(&self.engine);
        add_wasi_to_linker(&mut linker, |plugin_env: &mut PluginEnv| {
            &mut plugin_env.wasi_ctx
        })?;
        zellij_exports(&mut linker);

        let instance = crate::plugins::shared::instantiate_metered(&linker, &mut store, module)
            .with_context(err_context)?;

        run_initialize_hook(&mut store, &instance, self.plugin_id)?;

        Ok((store, instance))
    }
    pub fn create_wasi_ctx(
        host_dir: &PathBuf,
        data_dir: &PathBuf,
        cache_dir: &PathBuf,
        tmp_dir: &PathBuf,
        plugin_url: &String,
        plugin_id: PluginId,
        is_builtin: bool,
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
            // before we mounted in the wasi environment, we'll crash
            // when we move to a new wasi environment, we should address this with locking if
            // there's no built-in solution
            dir.try_exists().ok().unwrap_or(false)
        });

        let mut builder = WasiCtxBuilder::new();
        if is_builtin {
            builder.inherit_env()?;
        }

        // Mount directories using the builder
        for (guest_path, host_path) in dirs {
            match open_dir(&host_path) {
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

/// WASI preview1 `errno::intr`. A clock event means the clock reached the guest's
/// timestamp, so a long timer must not return success early. `poll_oneoff` returns
/// this instead of blocking past the callback host budget, and does not write events.
const WASI_ERRNO_INTR: i32 = 27;

// preview1 `subscription` is 48 bytes: userdata u64, tag at byte 8, then the
// clock payload (id u32 at 16, timeout u64 at 24, flags u16 at 40). Flag bit 0
// is an absolute deadline. Clock id 0 is realtime, 1 is monotonic.
const WASI_SUBSCRIPTION_SIZE: usize = 48;
const WASI_SUBSCRIPTION_TAG_OFFSET: usize = 8;
const WASI_EVENTTYPE_CLOCK: u8 = 0;
const WASI_CLOCK_ID_OFFSET: usize = 16;
const WASI_CLOCKID_REALTIME: u32 = 0;
const WASI_CLOCKID_MONOTONIC: u32 = 1;
const WASI_CLOCK_TIMEOUT_OFFSET: usize = 24;
const WASI_CLOCK_FLAGS_OFFSET: usize = 40;
const WASI_SUBCLOCKFLAGS_ABSTIME: u16 = 1;

/// Whether the real `poll_oneoff` may run. Guest subscription bytes are never written.
#[derive(Debug, PartialEq, Eq)]
enum PollOneoffDecision {
    /// Call the real poll.
    Run,
    /// Return `EINTR` without calling poll and without writing events.
    Interrupt,
}

enum GuestPollBound {
    /// The real implementation owns this request and does not block on it.
    Delegate,
    /// Finite upper bound, including zero when a clock is already due.
    Bounded(Duration),
    /// No finite clock caps the wait (fd-only, or a clock whose time is unavailable).
    Unbounded,
}

fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

/// How long one clock subscription can block. `None` means the current time is
/// unavailable, so this subscription does not give a finite bound.
fn clock_subscription_wait(
    subscription: &[u8],
    mut clock_now_nanos: impl FnMut(u32) -> Option<u64>,
) -> Option<Duration> {
    let timeout = le_u64(subscription, WASI_CLOCK_TIMEOUT_OFFSET);
    let flags = le_u16(subscription, WASI_CLOCK_FLAGS_OFFSET);
    if flags & WASI_SUBCLOCKFLAGS_ABSTIME == 0 {
        return Some(Duration::from_nanos(timeout));
    }
    let now = clock_now_nanos(le_u32(subscription, WASI_CLOCK_ID_OFFSET))?;
    Some(Duration::from_nanos(timeout.saturating_sub(now)))
}

/// Upper bound on how long the real poll can block. Does not write `memory`.
fn guest_poll_bound(
    memory: &[u8],
    subscriptions_ptr: u32,
    count: i32,
    mut clock_now_nanos: impl FnMut(u32) -> Option<u64>,
) -> GuestPollBound {
    if count <= 0 {
        return GuestPollBound::Delegate;
    }
    let count = count as usize;
    let Some(region_len) = count.checked_mul(WASI_SUBSCRIPTION_SIZE) else {
        return GuestPollBound::Delegate;
    };
    let start = subscriptions_ptr as usize;
    let Some(end) = start.checked_add(region_len) else {
        return GuestPollBound::Delegate;
    };
    if end > memory.len() {
        return GuestPollBound::Delegate;
    }

    // The soonest finite clock caps the poll, including when fds are mixed in.
    // A clock with no readable time does not cap anything; if it is the only
    // bound, the wait is unbounded and must not run.
    let mut soonest: Option<Duration> = None;
    for subscription in memory[start..end].chunks_exact(WASI_SUBSCRIPTION_SIZE) {
        if subscription[WASI_SUBSCRIPTION_TAG_OFFSET] != WASI_EVENTTYPE_CLOCK {
            continue;
        }
        let Some(wait) = clock_subscription_wait(subscription, &mut clock_now_nanos) else {
            continue;
        };
        soonest = Some(match soonest {
            Some(previous) => previous.min(wait),
            None => wait,
        });
    }
    match soonest {
        Some(wait) => GuestPollBound::Bounded(wait),
        None => GuestPollBound::Unbounded,
    }
}

/// Read-only admission for one `poll_oneoff`. `remaining` is the callback host
/// budget still left. A zero wait is admitted even when that budget is spent,
/// because an expired deadline does not block. Each admitted bounded wait
/// consumes its duration from the one callback budget when the real poll blocks.
fn poll_oneoff_decision(
    memory: &[u8],
    subscriptions_ptr: u32,
    count: i32,
    remaining: Duration,
    clock_now_nanos: impl FnMut(u32) -> Option<u64>,
) -> PollOneoffDecision {
    match guest_poll_bound(memory, subscriptions_ptr, count, clock_now_nanos) {
        GuestPollBound::Delegate => PollOneoffDecision::Run,
        GuestPollBound::Bounded(max_block) if max_block <= remaining => PollOneoffDecision::Run,
        GuestPollBound::Bounded(_) | GuestPollBound::Unbounded => PollOneoffDecision::Interrupt,
    }
}

/// Nanos of a WASI clock, matching preview1 `clock_time_get`. `None` if that
/// clock cannot be read; the subscription is then treated as unbounded.
fn wasi_clock_now_nanos(ctx: &WasiCtx, clock_id: u32) -> Option<u64> {
    use wasmi_wasi::wasi_common::{WasiMonotonicClock, WasiSystemClock};
    let precision = Duration::from_nanos(1);
    let nanos = match clock_id {
        WASI_CLOCKID_REALTIME => {
            let clock = ctx.clocks.system().ok()?;
            let now = WasiSystemClock::now(clock, precision).into_std();
            now.duration_since(std::time::SystemTime::UNIX_EPOCH)
                .ok()?
                .as_nanos()
        },
        WASI_CLOCKID_MONOTONIC => {
            let clock = ctx.clocks.monotonic().ok()?;
            WasiMonotonicClock::now(clock.abs_clock.as_ref(), precision)
                .duration_since(clock.creation_time)
                .as_nanos()
        },
        _ => return None,
    };
    Some(u64::try_from(nanos).unwrap_or(u64::MAX))
}

/// Add the WASI functions to the linker. `poll_oneoff` is shadowed so a guest
/// cannot block the executor past the callback host budget. Subscription bytes
/// are only read.
pub(crate) fn add_wasi_to_linker<T: 'static>(
    linker: &mut Linker<T>,
    wasi_ctx: impl Fn(&mut T) -> &mut WasiCtx + Send + Sync + Copy + 'static,
) -> Result<()> {
    use std::future::Future;
    use wasmi_wasi::wasi_common::snapshots::preview_1::wasi_snapshot_preview1;

    wasmi_wasi::add_to_linker(linker, wasi_ctx).map_err(|e| anyhow!("{:?}", e))?;
    linker.allow_shadowing(true);
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "poll_oneoff",
        move |mut caller: wasmi::Caller<'_, T>,
              subscriptions: i32,
              events: i32,
              count: i32,
              count_out: i32|
              -> std::result::Result<i32, wasmi::Error> {
            let Some(wasmi::Extern::Memory(memory)) = caller.get_export("memory") else {
                return Err(wasmi::Error::new(String::from(
                    "missing required WASI memory export",
                )));
            };
            let (memory, data) = memory.data_and_store_mut(&mut caller);
            // Unit tests never refuel. Arm once so repeated waits in this call
            // share one deadline instead of each seeing a fresh budget.
            crate::plugins::shared::ensure_callback_host_budget();
            let ctx = wasi_ctx(data);
            let realtime_now = wasi_clock_now_nanos(ctx, WASI_CLOCKID_REALTIME);
            let monotonic_now = wasi_clock_now_nanos(ctx, WASI_CLOCKID_MONOTONIC);
            let decision = poll_oneoff_decision(
                memory,
                subscriptions as u32,
                count,
                crate::plugins::shared::callback_host_budget_remaining(),
                |clock_id| match clock_id {
                    WASI_CLOCKID_REALTIME => realtime_now,
                    WASI_CLOCKID_MONOTONIC => monotonic_now,
                    _ => None,
                },
            );
            if decision == PollOneoffDecision::Interrupt {
                return Ok(WASI_ERRNO_INTR);
            }
            let mut guest_memory = wasmi_wasi::WasmiGuestMemory::Unshared(memory);
            // the synchronous scheduler never leaves a future pending
            let future = wasi_snapshot_preview1::poll_oneoff(
                ctx,
                &mut guest_memory,
                subscriptions,
                events,
                count,
                count_out,
            );
            let mut future = std::pin::pin!(future);
            let mut context = std::task::Context::from_waker(std::task::Waker::noop());
            match future.as_mut().poll(&mut context) {
                std::task::Poll::Ready(Ok(errno)) => Ok(errno),
                std::task::Poll::Ready(Err(error)) => {
                    match error.downcast::<wasmi_wasi::wasi_common::I32Exit>() {
                        Ok(wasmi_wasi::wasi_common::I32Exit(status)) => {
                            Err(wasmi::Error::i32_exit(status))
                        },
                        Err(error) => Err(wasmi::Error::new(error.to_string())),
                    }
                },
                std::task::Poll::Pending => Err(wasmi::Error::new("Cannot wait on pending future")),
            }
        },
    )?;
    linker.allow_shadowing(false);
    Ok(())
}

pub(crate) fn plugin_digest_from_bytes(wasm_bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(wasm_bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut hex, "{byte:02x}").expect("writing to a String cannot fail");
    }
    hex
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

#[cfg(test)]
mod tests {
    use super::{
        add_wasi_to_linker, guest_poll_bound, plugin_digest_from_bytes, poll_oneoff_decision,
        run_initialize_hook, GuestPollBound, PollOneoffDecision, WASI_ERRNO_INTR,
    };
    use wasmi::{Engine, Linker, Module, Store};

    fn instantiate(wat: &str) -> (Store<()>, wasmi::Instance) {
        let engine = Engine::default();
        let module = Module::new(&engine, wat::parse_str(wat).unwrap()).unwrap();
        let mut store = Store::new(&engine, ());
        let instance = Linker::new(&engine)
            .instantiate_and_start(&mut store, &module)
            .unwrap();
        (store, instance)
    }

    #[test]
    fn a_trapping_initialize_is_rejected() {
        let (mut store, instance) =
            instantiate(r#"(module (func (export "_initialize") unreachable))"#);
        assert!(run_initialize_hook(&mut store, &instance, 1).is_err());
    }

    #[test]
    fn an_initialize_with_an_unsupported_signature_is_rejected() {
        let (mut store, instance) =
            instantiate(r#"(module (func (export "_initialize") (param i32)))"#);
        assert!(run_initialize_hook(&mut store, &instance, 1).is_err());
    }

    #[test]
    fn a_well_formed_initialize_runs() {
        let (mut store, instance) = instantiate(r#"(module (func (export "_initialize")))"#);
        assert!(run_initialize_hook(&mut store, &instance, 1).is_ok());
    }

    #[test]
    fn a_module_without_initialize_is_accepted() {
        let (mut store, instance) = instantiate(r#"(module (func (export "load")))"#);
        assert!(run_initialize_hook(&mut store, &instance, 1).is_ok());
    }

    fn clock_subscription(timeout_nanos: u64, flags: u16) -> Vec<u8> {
        clock_subscription_for(timeout_nanos, flags, super::WASI_CLOCKID_MONOTONIC)
    }

    fn clock_subscription_for(timeout_nanos: u64, flags: u16, clock_id: u32) -> Vec<u8> {
        let mut subscription = vec![0u8; super::WASI_SUBSCRIPTION_SIZE];
        subscription[super::WASI_SUBSCRIPTION_TAG_OFFSET] = super::WASI_EVENTTYPE_CLOCK;
        subscription[super::WASI_CLOCK_ID_OFFSET..super::WASI_CLOCK_ID_OFFSET + 4]
            .copy_from_slice(&clock_id.to_le_bytes());
        subscription[super::WASI_CLOCK_TIMEOUT_OFFSET..super::WASI_CLOCK_TIMEOUT_OFFSET + 8]
            .copy_from_slice(&timeout_nanos.to_le_bytes());
        subscription[super::WASI_CLOCK_FLAGS_OFFSET..super::WASI_CLOCK_FLAGS_OFFSET + 2]
            .copy_from_slice(&flags.to_le_bytes());
        subscription
    }

    fn fd_subscription() -> Vec<u8> {
        let mut subscription = vec![0u8; super::WASI_SUBSCRIPTION_SIZE];
        subscription[super::WASI_SUBSCRIPTION_TAG_OFFSET] = 1;
        subscription
    }

    fn host_budget() -> std::time::Duration {
        crate::plugins::shared::PLUGIN_CALLBACK_HOST_BUDGET
    }

    #[test]
    fn long_relative_clock_waits_leave_subscriptions_unchanged() {
        let memory = clock_subscription(60_000_000_000, 0);
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 1, host_budget(), |_| None);
        assert_eq!(decision, PollOneoffDecision::Interrupt);
        assert_eq!(memory, before);

        // The soonest clock caps the poll. A short one beside a long one is admitted,
        // and neither subscription is rewritten.
        let mut memory = clock_subscription(60_000_000_000, 0);
        memory.extend(clock_subscription(1_000, 0));
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 2, host_budget(), |_| {
            panic!("relative waits do not read the clock");
        });
        assert_eq!(decision, PollOneoffDecision::Run);
        assert_eq!(memory, before);
    }

    #[test]
    fn an_expired_absolute_deadline_is_eligible_even_if_the_budget_is_spent() {
        // Already behind the clock. The raw timeout is not a relative wait:
        // with no budget left, a relative 1000ns would be refused.
        let memory = clock_subscription_for(
            1_000,
            super::WASI_SUBCLOCKFLAGS_ABSTIME,
            super::WASI_CLOCKID_MONOTONIC,
        );
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 1, std::time::Duration::ZERO, |clock_id| {
            assert_eq!(clock_id, super::WASI_CLOCKID_MONOTONIC);
            Some(5_000)
        });
        assert_eq!(decision, PollOneoffDecision::Run);
        assert_eq!(memory, before);

        let memory = clock_subscription_for(
            0,
            super::WASI_SUBCLOCKFLAGS_ABSTIME,
            super::WASI_CLOCKID_REALTIME,
        );
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 1, std::time::Duration::ZERO, |_| Some(1));
        assert_eq!(decision, PollOneoffDecision::Run);
        assert_eq!(memory, before);
    }

    #[test]
    fn fd_polls_are_bounded_only_by_a_clock() {
        let memory = fd_subscription();
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 1, host_budget(), |_| Some(0));
        assert_eq!(decision, PollOneoffDecision::Interrupt);
        assert_eq!(memory, before);

        let mut memory = fd_subscription();
        memory.extend(clock_subscription(1_000_000, 0));
        let decision = poll_oneoff_decision(&memory, 0, 2, host_budget(), |_| None);
        assert_eq!(decision, PollOneoffDecision::Run);

        let mut memory = fd_subscription();
        memory.extend(clock_subscription(60_000_000_000, 0));
        let before = memory.clone();
        let decision = poll_oneoff_decision(&memory, 0, 2, host_budget(), |_| None);
        assert_eq!(decision, PollOneoffDecision::Interrupt);
        assert_eq!(memory, before);
    }

    #[test]
    fn subscriptions_outside_memory_are_left_to_the_real_implementation() {
        let memory = clock_subscription(60_000_000_000, 0);
        let before = memory.clone();
        for (ptr, count) in [(8u32, 1i32), (u32::MAX, 1), (0, 0), (0, -1)] {
            let decision = poll_oneoff_decision(&memory, ptr, count, host_budget(), |_| Some(0));
            assert_eq!(
                decision,
                PollOneoffDecision::Run,
                "ptr {ptr} count {count} must be delegated, not rewritten or interrupted"
            );
        }
        assert_eq!(memory, before);
    }

    #[test]
    fn repeated_waits_cannot_evade_the_callback_budget() {
        // One armed budget, not a fresh 100ms per wait. Charging each admitted
        // wait by its duration is what the absolute deadline does when the real
        // poll blocks for that long. Five 50ms waits then admit two and refuse three.
        crate::plugins::shared::begin_callback_host_budget();
        let started = std::time::Instant::now();
        let memory = clock_subscription(50_000_000, 0);
        let before = memory.clone();
        let mut remaining = host_budget();
        let mut admitted = 0;
        let mut refused = 0;
        for _ in 0..5 {
            let block = match guest_poll_bound(&memory, 0, 1, |_| None) {
                GuestPollBound::Bounded(block) => block,
                GuestPollBound::Delegate | GuestPollBound::Unbounded => {
                    panic!("a 50ms clock wait is bounded");
                },
            };
            match poll_oneoff_decision(&memory, 0, 1, remaining, |_| None) {
                PollOneoffDecision::Run => {
                    assert_eq!(block, std::time::Duration::from_millis(50));
                    admitted += 1;
                    remaining = remaining.saturating_sub(block);
                },
                PollOneoffDecision::Interrupt => refused += 1,
            }
        }
        assert_eq!(
            admitted, 2,
            "a 100ms budget admits two 50ms waits and no more"
        );
        assert_eq!(refused, 3);
        assert_eq!(WASI_ERRNO_INTR, 27);
        assert_eq!(memory, before);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "deciding the waits blocked the thread: {:?}",
            started.elapsed()
        );
    }

    fn memory_prefix<T>(store: &Store<T>, instance: &wasmi::Instance, len: usize) -> Vec<u8> {
        let memory = instance.get_memory(store, "memory").unwrap();
        let mut bytes = vec![0u8; len];
        memory.read(store, 0, &mut bytes).unwrap();
        bytes
    }

    fn call_exported_sleep(wat: &str) -> (i32, std::time::Duration, Vec<u8>, Vec<u8>) {
        let engine = Engine::default();
        let module = Module::new(&engine, wat::parse_str(wat).unwrap()).unwrap();
        let mut store = Store::new(&engine, wasmi_wasi::sync::WasiCtxBuilder::new().build());
        let mut linker = Linker::new(&engine);
        add_wasi_to_linker(&mut linker, |ctx: &mut wasmi_wasi::WasiCtx| ctx).unwrap();
        let instance = linker.instantiate_and_start(&mut store, &module).unwrap();
        let before = memory_prefix(&store, &instance, super::WASI_SUBSCRIPTION_SIZE);
        let sleep = instance
            .get_typed_func::<(), i32>(&mut store, "sleep")
            .unwrap();
        let started = std::time::Instant::now();
        let errno = sleep.call(&mut store, ()).unwrap();
        let elapsed = started.elapsed();
        let after = memory_prefix(&store, &instance, super::WASI_SUBSCRIPTION_SIZE);
        (errno, elapsed, before, after)
    }

    #[test]
    fn an_expired_absolute_clock_poll_returns_success_without_rewriting_subscriptions() {
        // timeout 0, abstime flag, monotonic clock: already due
        let wat = r#"(module
            (import "wasi_snapshot_preview1" "poll_oneoff"
                (func $poll_oneoff (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 16) "\01\00\00\00")
            (data (i32.const 40) "\01\00")
            (func (export "sleep") (result i32)
                (call $poll_oneoff (i32.const 0) (i32.const 64) (i32.const 1) (i32.const 128))))"#;
        let (errno, elapsed, before, after) = call_exported_sleep(wat);
        assert_eq!(before[16], 1, "monotonic clock id");
        assert_eq!(before[40], super::WASI_SUBCLOCKFLAGS_ABSTIME as u8);
        assert_eq!(&before[24..32], &[0; 8]);
        assert_eq!(errno, 0);
        assert_eq!(before, after);
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "an expired absolute deadline blocked the thread: {elapsed:?}"
        );
    }

    #[test]
    fn a_guest_cannot_sleep_for_long_in_poll_oneoff() {
        // one relative monotonic clock subscription of 60 seconds at address 0
        let wat = r#"(module
            (import "wasi_snapshot_preview1" "poll_oneoff"
                (func $poll_oneoff (param i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 16) "\01\00\00\00")
            (data (i32.const 24) "\00\58\47\f8\0d\00\00\00")
            (func (export "sleep") (result i32)
                (call $poll_oneoff (i32.const 0) (i32.const 64) (i32.const 1) (i32.const 128))))"#;
        let (errno, elapsed, before, after) = call_exported_sleep(wat);
        assert_eq!(&before[24..32], &60_000_000_000u64.to_le_bytes());
        assert_eq!(errno, 27);
        assert_ne!(errno, 0);
        assert_eq!(before, after);
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "the 60s sleep was not interrupted: {elapsed:?}"
        );
    }

    struct LoaderFixture {
        _dir: tempfile::TempDir,
        loading_context: crate::plugins::wasm_bridge::LoadingContext,
        plugin_map: std::sync::Arc<std::sync::Mutex<crate::plugins::plugin_map::PluginMap>>,
        plugin_cache: crate::plugins::wasm_bridge::PluginCache,
        connected_clients: std::sync::Arc<std::sync::Mutex<Vec<crate::ClientId>>>,
        engine: Engine,
    }

    impl LoaderFixture {
        fn new(wat: &str, plugin_id: u32, client_id: u32, connected: &[u32]) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let wasm_path = dir.path().join("plugin.wasm");
            std::fs::write(&wasm_path, wat::parse_str(wat).unwrap()).unwrap();
            let engine = Engine::default();
            let bridge = crate::plugins::wasm_bridge::WasmBridge::new(
                Default::default(),
                engine.clone(),
                dir.path().to_path_buf(),
                "/bin/sh".into(),
                dir.path().to_path_buf(),
                Default::default(),
                None,
                None,
                vec![],
                vec![],
                Default::default(),
                Default::default(),
            );
            let run = zellij_utils::input::layout::RunPlugin::from_url(&format!(
                "file:{}",
                wasm_path.display()
            ))
            .unwrap();
            let mut loading_context = crate::plugins::wasm_bridge::LoadingContext::new(
                &bridge,
                None,
                zellij_utils::input::plugins::PluginConfig::from_run_plugin(&run).unwrap(),
                plugin_id,
                client_id,
                None,
                zellij_utils::pane_size::Size { rows: 5, cols: 80 },
            );
            loading_context.plugin_own_data_dir = dir.path().join("data");
            loading_context.plugin_own_cache_dir = dir.path().join("cache");
            LoaderFixture {
                _dir: dir,
                loading_context,
                plugin_map: Default::default(),
                plugin_cache: Default::default(),
                connected_clients: std::sync::Arc::new(std::sync::Mutex::new(connected.to_vec())),
                engine,
            }
        }
        fn loader(&self) -> super::PluginLoader {
            super::PluginLoader::new(
                false,
                self.loading_context.clone(),
                crate::thread_bus::ThreadSenders {
                    should_silently_fail: true,
                    ..Default::default()
                },
                self.engine.clone(),
                self.plugin_cache.clone(),
                self.plugin_map.clone(),
                self.connected_clients.clone(),
            )
        }
    }

    const NOOP_PLUGIN: &str = r#"(module (func (export "_start")) (func (export "load")))"#;
    const FAILING_PLUGIN: &str = r#"(module
        (func (export "_start") unreachable)
        (func (export "load")))"#;

    #[test]
    fn a_failed_first_load_does_not_leave_its_data_dir_behind() {
        let fixture = LoaderFixture::new(FAILING_PLUGIN, 5, 1, &[]);
        let data_dir = fixture.loading_context.plugin_own_data_dir.clone();
        let mut loader = fixture.loader();
        assert!(data_dir.exists());

        assert!(loader.start_plugin().is_err());

        assert!(!data_dir.exists());
        assert!(!fixture.plugin_map.lock().unwrap().has_instance_of(5));
    }

    #[test]
    fn a_failure_before_publication_rolls_back_metadata_and_data_dir() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        let data_dir = fixture.loading_context.plugin_own_data_dir.clone();
        std::fs::write(
            &fixture.loading_context.plugin_config.path,
            b"not a wasm module",
        )
        .unwrap();

        assert!(fixture.loader().start_plugin().is_err());

        let plugin_map = fixture.plugin_map.lock().unwrap();
        assert!(!data_dir.exists());
        assert!(plugin_map.metadata(5).is_none());
        assert!(!plugin_map.has_instance_of(5));
    }

    #[test]
    fn a_failed_load_clears_the_screen_state_it_installed() {
        use crate::plugins::PluginInstruction;
        use crate::screen::ScreenInstruction;
        use zellij_utils::channels::{self, SenderWithContext};

        let fixture = LoaderFixture::new(FAILING_PLUGIN, 5, 1, &[]);
        let (to_plugin, plugin_receiver) = channels::unbounded();
        let (to_screen, screen_receiver) = channels::unbounded();
        let senders = crate::thread_bus::ThreadSenders {
            to_plugin: Some(SenderWithContext::new(to_plugin)),
            to_screen: Some(SenderWithContext::new(to_screen)),
            should_silently_fail: true,
            ..Default::default()
        };
        let mut loader = super::PluginLoader::new(
            false,
            fixture.loading_context.clone(),
            senders,
            fixture.engine.clone(),
            fixture.plugin_cache.clone(),
            fixture.plugin_map.clone(),
            fixture.connected_clients.clone(),
        );

        assert!(loader.start_plugin().is_err());

        let mut screen_instructions = vec![];
        while let Ok((instruction, _)) = screen_receiver.try_recv() {
            screen_instructions.push(instruction);
        }
        assert!(screen_instructions
            .iter()
            .any(|i| matches!(i, ScreenInstruction::ClearKeyPressesIntercepts(5, 1))));
        assert!(screen_instructions
            .iter()
            .any(|i| matches!(i, ScreenInstruction::ClearAllPluginHighlights(5))));

        let mut plugin_instructions = vec![];
        while let Ok((instruction, _)) = plugin_receiver.try_recv() {
            plugin_instructions.push(instruction);
        }
        assert!(plugin_instructions
            .iter()
            .any(|i| matches!(i, PluginInstruction::UnregisterSettingsEntry(5))));
    }

    #[test]
    fn rollback_removes_only_the_instances_this_loader_published() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        fixture.loader().start_plugin().unwrap();
        let mut second_context = fixture.loading_context.clone();
        second_context.client_id = 2;
        let mut second = super::PluginLoader::new(
            false,
            second_context,
            crate::thread_bus::ThreadSenders {
                should_silently_fail: true,
                ..Default::default()
            },
            fixture.engine.clone(),
            fixture.plugin_cache.clone(),
            fixture.plugin_map.clone(),
            fixture.connected_clients.clone(),
        );
        let snapshot = second.load_snapshot();
        second.record_plugin_metadata();
        let module = second.load_module().unwrap();
        let (store, instance) = second.create_plugin_environment(module).unwrap();
        second
            .load_plugin_instance(store, &instance, false)
            .unwrap();
        assert!(fixture.plugin_map.lock().unwrap().contains(5, 2));

        second.rollback_failed_load(&snapshot);

        let plugin_map = fixture.plugin_map.lock().unwrap();
        assert!(plugin_map.contains(5, 1));
        assert!(plugin_map.metadata(5).is_some());
        assert!(!plugin_map.contains(5, 2));
        assert!(fixture.loading_context.plugin_own_data_dir.exists());
    }

    #[test]
    fn a_preloaded_module_is_not_read_from_disk_again() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        let original_wasm = std::fs::read(&fixture.loading_context.plugin_config.path).unwrap();
        let expected_digest = super::plugin_digest_from_bytes(&original_wasm);
        let mut loading_context = fixture.loading_context.clone();
        loading_context.wasm_bytes = Some(original_wasm);
        std::fs::write(
            &fixture.loading_context.plugin_config.path,
            b"not a wasm module",
        )
        .unwrap();
        let mut loader = super::PluginLoader::new(
            false,
            loading_context,
            crate::thread_bus::ThreadSenders {
                should_silently_fail: true,
                ..Default::default()
            },
            fixture.engine.clone(),
            fixture.plugin_cache.clone(),
            fixture.plugin_map.clone(),
            fixture.connected_clients.clone(),
        );

        loader.start_plugin().unwrap();

        let running_plugin = fixture
            .plugin_map
            .lock()
            .unwrap()
            .get_running_plugin(5, Some(1))
            .unwrap();
        assert_eq!(
            running_plugin.lock().unwrap().store.data().plugin_digest,
            expected_digest
        );
    }

    #[test]
    fn a_failed_load_that_replaced_a_running_instance_keeps_the_data_dir() {
        // the same plugin id loaded again, as a reload does: the directory outlives it
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        let data_dir = fixture.loading_context.plugin_own_data_dir.clone();
        fixture.loader().start_plugin().unwrap();
        std::fs::write(
            fixture.loading_context.plugin_config.path.clone(),
            wat::parse_str(FAILING_PLUGIN).unwrap(),
        )
        .unwrap();
        let mut reload = super::PluginLoader::new(
            true,
            fixture.loading_context.clone(),
            crate::thread_bus::ThreadSenders {
                should_silently_fail: true,
                ..Default::default()
            },
            fixture.engine.clone(),
            fixture.plugin_cache.clone(),
            fixture.plugin_map.clone(),
            fixture.connected_clients.clone(),
        );

        assert!(reload.start_plugin().is_err());

        assert!(data_dir.exists());
    }

    #[test]
    fn workers_run_the_module_of_the_instance_that_created_them() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        let mut loader = fixture.loader();
        let module = loader.load_module().unwrap();
        loader.create_plugin_environment(module).unwrap();
        // a concurrent load of the same path takes the cache entry, or replaces it
        fixture.plugin_cache.lock().unwrap().clear();

        assert!(loader
            .create_plugin_instance_and_wasi_env_for_worker()
            .is_ok());
    }

    #[test]
    fn a_load_leaves_the_cached_module_for_concurrent_loads_of_the_same_path() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[]);
        fixture.loader().start_plugin().unwrap();
        assert_eq!(fixture.plugin_cache.lock().unwrap().len(), 1);
        // the second load takes the module from the cache and the entry stays for the next one
        let mut second = fixture.loader();
        second.load_module().unwrap();
        assert_eq!(fixture.plugin_cache.lock().unwrap().len(), 1);
    }

    #[test]
    fn every_connected_client_gets_an_instance() {
        let fixture = LoaderFixture::new(NOOP_PLUGIN, 5, 1, &[1, 2, 3]);
        fixture.loader().start_plugin().unwrap();
        let plugin_map = fixture.plugin_map.lock().unwrap();
        assert!(plugin_map.contains(5, 1));
        assert!(plugin_map.contains(5, 2));
        assert!(plugin_map.contains(5, 3));
    }

    #[test]
    fn a_plugin_unloaded_while_loading_gets_no_further_client_instances() {
        // the first instance spends a while in _start, long enough to be unloaded meanwhile
        let fixture = LoaderFixture::new(
            r#"(module
                (func (export "_start") (local $i i32)
                    (loop $spin
                        (local.set $i (i32.add (local.get $i) (i32.const 1)))
                        (br_if $spin (i32.lt_u (local.get $i) (i32.const 20000000)))))
                (func (export "load")))"#,
            5,
            1,
            &[1, 2, 3],
        );
        let plugin_map = fixture.plugin_map.clone();
        let unloader = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
            loop {
                let mut plugin_map = plugin_map.lock().unwrap();
                if plugin_map.has_instance_of(5) {
                    // what unloading the plugin does to the map
                    return plugin_map.remove_plugins(5).len();
                }
                drop(plugin_map);
                assert!(
                    std::time::Instant::now() < deadline,
                    "the load never started"
                );
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        let result = fixture.loader().start_plugin();

        // only the instance published before the unload was collected for cleanup
        assert_eq!(unloader.join().unwrap(), 1);
        assert!(result.is_ok());
        assert!(!fixture.plugin_map.lock().unwrap().has_instance_of(5));
    }

    #[test]
    fn plugin_digest_changes_when_module_bytes_change() {
        assert_eq!(
            plugin_digest_from_bytes(b"module-one"),
            plugin_digest_from_bytes(b"module-one")
        );
        assert_ne!(
            plugin_digest_from_bytes(b"module-one"),
            plugin_digest_from_bytes(b"module-two")
        );
    }
}
