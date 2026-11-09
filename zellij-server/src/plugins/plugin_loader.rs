use crate::plugins::plugin_map::{
    PluginEnv, PluginMap, PluginMetadata, RunningPlugin, SharedEnv, VecDequeInputStream,
    WriteOutputStream,
};
use crate::plugins::plugin_worker::{plugin_worker, MessageToWorker, RunningWorker};
use crate::plugins::wasm_bridge::{LoadingContext, PluginCache};
use crate::plugins::zellij_exports::{wasi_write_object, zellij_exports};
use crate::plugins::PluginId;
use prost::Message;
use sha2::{Digest, Sha256};
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
    // Set when the plugin was unloaded while this loader was still publishing instances.
    unloaded_during_load: bool,
    connected_clients: Option<Arc<Mutex<Vec<ClientId>>>>,
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
            unloaded_during_load: false,
            connected_clients: Some(connected_clients),
            loading_indication,
        }
    }
    pub fn without_connected_clients(mut self) -> Self {
        self.connected_clients = None;
        self
    }
    pub fn start_plugin(&mut self) -> Result<()> {
        self.plugin_map.lock().unwrap().begin_load(self.plugin_id);
        let result = self.start_plugin_instances();
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
        let module = self.load_module()?;
        let (mut store, instance) = self.create_plugin_environment(module)?;
        store.data_mut().shared = Some(SharedEnv::default());
        self.load_plugin_instance(store, &instance, true)
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
        let wasm_bytes = self.plugin_config.resolve_wasm_bytes(&self.plugin_dir)?;
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
        let replaced_existing_instance = {
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
            let replaced_existing_instance = if shared {
                plugin_map.shared_running_plugin(self.plugin_id).is_some()
            } else {
                plugin_map.contains(self.plugin_id, self.client_id)
            };
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
            replaced_existing_instance
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
            self.retire_provisional_instance(shared, replaced_existing_instance);
            return Err(e);
        }

        Ok(plugin)
    }

    /// Remove a provisional plugin instance and retire its workers after
    /// a failed _start/load so no runnable entry, worker or pending
    /// pipe survives the failed initialization.
    fn retire_provisional_instance(&mut self, shared: bool, replaced_existing_instance: bool) {
        let (removed, other_instances_remain) = {
            let mut plugin_map = self.plugin_map.lock().unwrap();
            if shared {
                (plugin_map.remove_shared(self.plugin_id), false)
            } else {
                let removed = plugin_map.remove_plugin_instance(self.plugin_id, self.client_id);
                (removed, plugin_map.has_instance_of(self.plugin_id))
            }
        };
        if let Some((_running_plugin, _subscriptions, workers)) = removed {
            for (_worker_name, worker_sender) in workers {
                drop(worker_sender.send(MessageToWorker::Exit));
            }
        }
        // Nothing will unload a plugin that never finished loading, so its data directory
        // would stay behind for good. The directory is shared by all instances of the plugin
        // and outlives a reload, so it is only removed when this was a first load that left no
        // instance behind.
        if !replaced_existing_instance && !other_instances_remain {
            if let Err(e) = fs::remove_dir_all(&self.plugin_own_data_dir) {
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
            client_id: self.client_id,
            plugin: self.plugin_config.clone(), // TODO: change field name in PluginEnv to plugin_config
            plugin_digest: self.plugin_digest.clone(),
            permissions: Arc::new(Mutex::new(None)),
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
            client_id: self.client_id,
            plugin: plugin_config,
            plugin_digest: self.plugin_digest.clone(),
            permissions: Arc::new(Mutex::new(None)),
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

/// The longest a guest can make a WASI clock wait last. Fuel only meters guest instructions, and
/// wasi-common implements `poll_oneoff` (what `sleep` uses) by blocking the calling thread, so
/// an uncapped timeout would hold the plugin's executor thread and execution lock for as long
/// as the guest asks without consuming any fuel.
const MAX_GUEST_CLOCK_WAIT: std::time::Duration = std::time::Duration::from_millis(100);

const WASI_SUBSCRIPTION_SIZE: usize = 48;
const WASI_SUBSCRIPTION_TAG_OFFSET: usize = 8;
const WASI_EVENTTYPE_CLOCK: u8 = 0;
const WASI_CLOCK_TIMEOUT_OFFSET: usize = 24;
const WASI_CLOCK_FLAGS_OFFSET: usize = 40;
const WASI_SUBCLOCKFLAGS_ABSTIME: u16 = 1;

/// Shorten every clock subscription in a `poll_oneoff` request to `MAX_GUEST_CLOCK_WAIT`. A wait
/// on an absolute deadline becomes a relative one, as the deadline cannot be compared with the
/// clock here. The guest sees its wait end early, like any spurious wakeup.
fn cap_poll_oneoff_clock_waits(memory: &mut [u8], subscriptions_ptr: u32, count: u32) {
    let max_wait = MAX_GUEST_CLOCK_WAIT.as_nanos() as u64;
    for index in 0..count as usize {
        let Some(start) =
            (subscriptions_ptr as usize).checked_add(index.saturating_mul(WASI_SUBSCRIPTION_SIZE))
        else {
            return;
        };
        let Some(subscription) = start
            .checked_add(WASI_SUBSCRIPTION_SIZE)
            .and_then(|end| memory.get_mut(start..end))
        else {
            return; // out of bounds: the real implementation reports the error
        };
        if subscription[WASI_SUBSCRIPTION_TAG_OFFSET] != WASI_EVENTTYPE_CLOCK {
            continue;
        }
        let mut timeout_bytes = [0u8; 8];
        timeout_bytes.copy_from_slice(
            &subscription[WASI_CLOCK_TIMEOUT_OFFSET..WASI_CLOCK_TIMEOUT_OFFSET + 8],
        );
        let mut timeout = u64::from_le_bytes(timeout_bytes);
        let mut flags_bytes = [0u8; 2];
        flags_bytes
            .copy_from_slice(&subscription[WASI_CLOCK_FLAGS_OFFSET..WASI_CLOCK_FLAGS_OFFSET + 2]);
        let mut flags = u16::from_le_bytes(flags_bytes);
        if flags & WASI_SUBCLOCKFLAGS_ABSTIME != 0 {
            flags &= !WASI_SUBCLOCKFLAGS_ABSTIME;
            timeout = max_wait;
        } else {
            timeout = timeout.min(max_wait);
        }
        subscription[WASI_CLOCK_TIMEOUT_OFFSET..WASI_CLOCK_TIMEOUT_OFFSET + 8]
            .copy_from_slice(&timeout.to_le_bytes());
        subscription[WASI_CLOCK_FLAGS_OFFSET..WASI_CLOCK_FLAGS_OFFSET + 2]
            .copy_from_slice(&flags.to_le_bytes());
    }
}

/// Add the WASI functions to the linker, with `poll_oneoff` limited by `MAX_GUEST_CLOCK_WAIT`.
pub(crate) fn add_wasi_to_linker<T: 'static>(
    linker: &mut Linker<T>,
    wasi_ctx: impl Fn(&mut T) -> &mut WasiCtx + Send + Sync + Copy + 'static,
) -> Result<()> {
    use std::future::Future;
    use wasmi_wasi::wasi_common::snapshots::preview_1::wasi_snapshot_preview1;

    wasmi_wasi::add_to_linker(linker, wasi_ctx).map_err(|e| anyhow!("{:?}", e))?;
    // The replacement does what wasmi_wasi's own `poll_oneoff` does, after shortening the waits.
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
            cap_poll_oneoff_clock_waits(memory, subscriptions as u32, count as u32);
            let ctx = wasi_ctx(data);
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
    Sha256::digest(wasm_bytes)
        .iter()
        .map(|byte| format!("{:02x}", byte))
        .collect()
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
        add_wasi_to_linker, cap_poll_oneoff_clock_waits, plugin_digest_from_bytes,
        run_initialize_hook, MAX_GUEST_CLOCK_WAIT,
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
        let mut subscription = vec![0u8; super::WASI_SUBSCRIPTION_SIZE];
        subscription[super::WASI_SUBSCRIPTION_TAG_OFFSET] = super::WASI_EVENTTYPE_CLOCK;
        subscription[super::WASI_CLOCK_TIMEOUT_OFFSET..super::WASI_CLOCK_TIMEOUT_OFFSET + 8]
            .copy_from_slice(&timeout_nanos.to_le_bytes());
        subscription[super::WASI_CLOCK_FLAGS_OFFSET..super::WASI_CLOCK_FLAGS_OFFSET + 2]
            .copy_from_slice(&flags.to_le_bytes());
        subscription
    }

    fn timeout_and_flags(subscription: &[u8]) -> (u64, u16) {
        let mut timeout = [0u8; 8];
        timeout.copy_from_slice(
            &subscription[super::WASI_CLOCK_TIMEOUT_OFFSET..super::WASI_CLOCK_TIMEOUT_OFFSET + 8],
        );
        let mut flags = [0u8; 2];
        flags.copy_from_slice(
            &subscription[super::WASI_CLOCK_FLAGS_OFFSET..super::WASI_CLOCK_FLAGS_OFFSET + 2],
        );
        (u64::from_le_bytes(timeout), u16::from_le_bytes(flags))
    }

    #[test]
    fn long_relative_clock_waits_are_shortened_and_short_ones_kept() {
        let max = MAX_GUEST_CLOCK_WAIT.as_nanos() as u64;
        let mut memory = clock_subscription(60_000_000_000, 0);
        memory.extend(clock_subscription(1_000, 0));
        cap_poll_oneoff_clock_waits(&mut memory, 0, 2);
        assert_eq!(timeout_and_flags(&memory[..48]), (max, 0));
        assert_eq!(timeout_and_flags(&memory[48..]), (1_000, 0));
    }

    #[test]
    fn absolute_clock_waits_become_short_relative_ones() {
        let max = MAX_GUEST_CLOCK_WAIT.as_nanos() as u64;
        let mut memory = clock_subscription(u64::MAX, 1);
        cap_poll_oneoff_clock_waits(&mut memory, 0, 1);
        assert_eq!(timeout_and_flags(&memory), (max, 0));
    }

    #[test]
    fn subscriptions_outside_memory_are_left_to_the_real_implementation() {
        let mut memory = clock_subscription(60_000_000_000, 0);
        let before = memory.clone();
        cap_poll_oneoff_clock_waits(&mut memory, 8, 1);
        cap_poll_oneoff_clock_waits(&mut memory, u32::MAX, u32::MAX);
        assert_eq!(memory, before);
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
        let engine = Engine::default();
        let module = Module::new(&engine, wat::parse_str(wat).unwrap()).unwrap();
        let mut store = Store::new(&engine, wasmi_wasi::sync::WasiCtxBuilder::new().build());
        let mut linker = Linker::new(&engine);
        add_wasi_to_linker(&mut linker, |ctx: &mut wasmi_wasi::WasiCtx| ctx).unwrap();
        let instance = linker.instantiate_and_start(&mut store, &module).unwrap();
        let sleep = instance
            .get_typed_func::<(), i32>(&mut store, "sleep")
            .unwrap();
        let started = std::time::Instant::now();
        let errno = sleep.call(&mut store, ()).unwrap();
        assert_eq!(errno, 0);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the sleep was not shortened: {:?}",
            started.elapsed()
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
