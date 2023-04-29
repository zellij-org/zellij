use crate::plugins::plugin_map::{PluginEnv, PluginMap, RunningPlugin, Subscriptions};
use crate::plugins::zellij_exports::{wasi_read_string, zellij_exports};
use highway::{HighwayHash, PortableHash};
use log::info;
use semver::Version;
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use url::Url;
use wasmer::{ChainableNamedResolver, Instance, Module, Store};
use wasmer_wasi::{Pipe, WasiState};

use crate::{
    logging_pipe::LoggingPipe, screen::ScreenInstruction, thread_bus::ThreadSenders,
    ui::loading_indication::LoadingIndication, ClientId,
};

use zellij_utils::{
    consts::{VERSION, ZELLIJ_CACHE_DIR, ZELLIJ_TMP_DIR},
    errors::prelude::*,
    input::plugins::PluginConfig,
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

/// Custom error for plugin version mismatch.
///
/// This is thrown when, during starting a plugin, it is detected that the plugin version doesn't
/// match the zellij version. This is treated as a fatal error and leads to instantaneous
/// termination.
#[derive(Debug)]
pub struct VersionMismatchError {
    zellij_version: String,
    plugin_version: String,
    plugin_path: PathBuf,
    // true for builtin plugins
    builtin: bool,
}

impl std::error::Error for VersionMismatchError {}

impl VersionMismatchError {
    pub fn new(
        zellij_version: &str,
        plugin_version: &str,
        plugin_path: &PathBuf,
        builtin: bool,
    ) -> Self {
        VersionMismatchError {
            zellij_version: zellij_version.to_owned(),
            plugin_version: plugin_version.to_owned(),
            plugin_path: plugin_path.to_owned(),
            builtin,
        }
    }
}

impl fmt::Display for VersionMismatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let first_line = if self.builtin {
            "It seems your version of zellij was built with outdated core plugins."
        } else {
            "If you're seeing this error a plugin version doesn't match the current
zellij version."
        };

        write!(
            f,
            "{}
Detected versions:

- Plugin version: {}
- Zellij version: {}
- Offending plugin: {}

If you're a user:
    Please contact the distributor of your zellij version and report this error
    to them.

If you're a developer:
    Please run zellij with updated plugins. The easiest way to achieve this
    is to build zellij with `cargo xtask install`. Also refer to the docs:
    https://github.com/zellij-org/zellij/blob/main/CONTRIBUTING.md#building
",
            first_line,
            self.plugin_version.trim_end(),
            self.zellij_version.trim_end(),
            self.plugin_path.display()
        )
    }
}

// Returns `Ok` if the plugin version matches the zellij version.
// Returns an `Err` otherwise.
fn assert_plugin_version(instance: &Instance, plugin_env: &PluginEnv) -> Result<()> {
    let err_context = || {
        format!(
            "failed to determine plugin version for plugin {}",
            plugin_env.plugin.path.display()
        )
    };

    let plugin_version_func = match instance.exports.get_function("plugin_version") {
        Ok(val) => val,
        Err(_) => {
            return Err(anyError::new(VersionMismatchError::new(
                VERSION,
                "Unavailable",
                &plugin_env.plugin.path,
                plugin_env.plugin.is_builtin(),
            )))
        },
    };

    let plugin_version = plugin_version_func
        .call(&[])
        .map_err(anyError::new)
        .and_then(|_| wasi_read_string(&plugin_env.wasi_env))
        .and_then(|string| Version::parse(&string).context("failed to parse plugin version"))
        .with_context(err_context)?;
    let zellij_version = Version::parse(VERSION)
        .context("failed to parse zellij version")
        .with_context(err_context)?;
    if plugin_version != zellij_version {
        return Err(anyError::new(VersionMismatchError::new(
            VERSION,
            &plugin_version.to_string(),
            &plugin_env.plugin.path,
            plugin_env.plugin.is_builtin(),
        )));
    }

    Ok(())
}

pub struct PluginLoader<'a> {
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    plugin_path: PathBuf,
    loading_indication: &'a mut LoadingIndication,
    senders: ThreadSenders,
    plugin_id: u32,
    client_id: ClientId,
    store: Store,
    plugin: PluginConfig,
    plugin_dir: &'a PathBuf,
    tab_index: usize,
    plugin_own_data_dir: PathBuf,
    size: Size,
    wasm_blob_on_hd: Option<(Vec<u8>, PathBuf)>,
}

impl<'a> PluginLoader<'a> {
    pub fn reload_plugin_from_memory(
        plugin_id: u32,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        store: Store,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
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
            &store,
            &plugin_dir,
        )?;
        plugin_loader
            .load_module_from_memory()
            .and_then(|module| {
                plugin_loader.create_plugin_instance_environment_and_subscriptions(module)
            })
            .and_then(|(instance, plugin_env, subscriptions)| {
                plugin_loader.load_plugin_instance(
                    &instance,
                    &plugin_env,
                    &plugin_map,
                    &subscriptions,
                )
            })
            .and_then(|_| {
                plugin_loader.clone_instance_for_other_clients(&connected_clients, &plugin_map)
            })
            .with_context(err_context)?;
        display_loading_stage!(end, loading_indication, senders, plugin_id);
        Ok(())
    }

    pub fn start_plugin(
        plugin_id: u32,
        client_id: ClientId,
        plugin: &PluginConfig,
        tab_index: usize,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        store: Store,
        plugin_map: Arc<Mutex<PluginMap>>,
        size: Size,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
    ) -> Result<()> {
        let err_context = || format!("failed to start plugin {plugin:#?} for client {client_id}");
        let mut plugin_loader = PluginLoader::new(
            &plugin_cache,
            loading_indication,
            &senders,
            plugin_id,
            client_id,
            &store,
            plugin.clone(),
            &plugin_dir,
            tab_index,
            size,
        )?;
        plugin_loader
            .load_module_from_memory()
            .or_else(|_e| plugin_loader.load_module_from_hd_cache())
            .or_else(|_e| plugin_loader.compile_module())
            .and_then(|module| {
                plugin_loader.create_plugin_instance_environment_and_subscriptions(module)
            })
            .and_then(|(instance, plugin_env, subscriptions)| {
                plugin_loader.load_plugin_instance(
                    &instance,
                    &plugin_env,
                    &plugin_map,
                    &subscriptions,
                )
            })
            .and_then(|_| {
                plugin_loader.clone_instance_for_other_clients(
                    &connected_clients.lock().unwrap(),
                    &plugin_map,
                )
            })
            .with_context(err_context)?;
        display_loading_stage!(end, loading_indication, senders, plugin_id);
        Ok(())
    }
    pub fn add_client(
        client_id: ClientId,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        store: Store,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
    ) -> Result<()> {
        let mut new_plugins = HashSet::new();
        for (&(plugin_id, _), _) in &*plugin_map.lock().unwrap() {
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
                &store,
                &plugin_dir,
            )?;
            plugin_loader
                .load_module_from_memory()
                .and_then(|module| {
                    plugin_loader.create_plugin_instance_environment_and_subscriptions(module)
                })
                .and_then(|(instance, plugin_env, subscriptions)| {
                    plugin_loader.load_plugin_instance(
                        &instance,
                        &plugin_env,
                        &plugin_map,
                        &subscriptions,
                    )
                })?
        }
        connected_clients.lock().unwrap().push(client_id);
        Ok(())
    }

    pub fn reload_plugin(
        plugin_id: u32,
        plugin_dir: PathBuf,
        plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
        senders: ThreadSenders,
        store: Store,
        plugin_map: Arc<Mutex<PluginMap>>,
        connected_clients: Arc<Mutex<Vec<ClientId>>>,
        loading_indication: &mut LoadingIndication,
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
            &store,
            &plugin_dir,
        )?;
        plugin_loader
            .compile_module()
            .and_then(|module| {
                plugin_loader.create_plugin_instance_environment_and_subscriptions(module)
            })
            .and_then(|(instance, plugin_env, subscriptions)| {
                plugin_loader.load_plugin_instance(
                    &instance,
                    &plugin_env,
                    &plugin_map,
                    &subscriptions,
                )
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
        plugin_id: u32,
        client_id: ClientId,
        store: &Store,
        plugin: PluginConfig,
        plugin_dir: &'a PathBuf,
        tab_index: usize,
        size: Size,
    ) -> Result<Self> {
        let plugin_own_data_dir = ZELLIJ_CACHE_DIR.join(Url::from(&plugin.location).to_string());
        create_plugin_fs_entries(&plugin_own_data_dir)?;
        let plugin_path = plugin.path.clone();
        Ok(PluginLoader {
            plugin_cache: plugin_cache.clone(),
            plugin_path,
            loading_indication,
            senders: senders.clone(),
            plugin_id,
            client_id,
            store: store.clone(),
            plugin,
            plugin_dir,
            tab_index,
            plugin_own_data_dir,
            size,
            wasm_blob_on_hd: None,
        })
    }
    pub fn new_from_existing_plugin_attributes(
        plugin_cache: &Arc<Mutex<HashMap<PathBuf, Module>>>,
        plugin_map: &Arc<Mutex<PluginMap>>,
        loading_indication: &'a mut LoadingIndication,
        senders: &ThreadSenders,
        plugin_id: u32,
        client_id: ClientId,
        store: &Store,
        plugin_dir: &'a PathBuf,
    ) -> Result<Self> {
        let err_context = || "Failed to find existing plugin";
        let (running_plugin, _subscriptions) = {
            let mut plugin_map = plugin_map.lock().unwrap();
            plugin_map
                .remove(&(plugin_id, client_id))
                .with_context(err_context)?
        };
        let running_plugin = running_plugin.lock().unwrap();
        let tab_index = running_plugin.plugin_env.tab_index;
        let size = Size {
            rows: running_plugin.rows,
            cols: running_plugin.columns,
        };
        let plugin_config = running_plugin.plugin_env.plugin.clone();
        loading_indication.set_name(running_plugin.plugin_env.name());
        PluginLoader::new(
            plugin_cache,
            loading_indication,
            senders,
            plugin_id,
            client_id,
            store,
            plugin_config,
            plugin_dir,
            tab_index,
            size,
        )
    }
    pub fn new_from_different_client_id(
        plugin_cache: &Arc<Mutex<HashMap<PathBuf, Module>>>,
        plugin_map: &Arc<Mutex<PluginMap>>,
        loading_indication: &'a mut LoadingIndication,
        senders: &ThreadSenders,
        plugin_id: u32,
        client_id: ClientId,
        store: &Store,
        plugin_dir: &'a PathBuf,
    ) -> Result<Self> {
        let err_context = || "Failed to find existing plugin";
        let (running_plugin, _subscriptions) = {
            let plugin_map = plugin_map.lock().unwrap();
            plugin_map
                .iter()
                .find(|((p_id, _c_id), _)| p_id == &plugin_id)
                .with_context(err_context)?
                .1
                .clone()
        };
        let running_plugin = running_plugin.lock().unwrap();
        let tab_index = running_plugin.plugin_env.tab_index;
        let size = Size {
            rows: running_plugin.rows,
            cols: running_plugin.columns,
        };
        let plugin_config = running_plugin.plugin_env.plugin.clone();
        loading_indication.set_name(running_plugin.plugin_env.name());
        PluginLoader::new(
            plugin_cache,
            loading_indication,
            senders,
            plugin_id,
            client_id,
            store,
            plugin_config,
            plugin_dir,
            tab_index,
            size,
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
        display_loading_stage!(
            indicate_loading_plugin_from_hd_cache,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        let (_wasm_bytes, cached_path) = self.plugin_bytes_and_cache_path()?;
        let timer = std::time::Instant::now();
        let module = unsafe { Module::deserialize_from_file(&self.store, &cached_path)? };
        log::info!(
            "Loaded plugin '{}' from cache folder at '{}' in {:?}",
            self.plugin_path.display(),
            ZELLIJ_CACHE_DIR.display(),
            timer.elapsed(),
        );
        display_loading_stage!(
            indicate_loading_plugin_from_hd_cache_success,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        Ok(module)
    }
    pub fn compile_module(&mut self) -> Result<Module> {
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
        let (wasm_bytes, cached_path) = self.plugin_bytes_and_cache_path()?;
        let timer = std::time::Instant::now();
        let err_context = || "failed to recover cache dir";
        let module = fs::create_dir_all(ZELLIJ_CACHE_DIR.to_owned())
            .map_err(anyError::new)
            .and_then(|_| {
                // compile module
                Module::new(&self.store, &wasm_bytes).map_err(anyError::new)
            })
            .and_then(|m| {
                // serialize module to HD cache for faster loading in the future
                m.serialize_to_file(&cached_path).map_err(anyError::new)?;
                log::info!(
                    "Compiled plugin '{}' in {:?}",
                    self.plugin_path.display(),
                    timer.elapsed()
                );
                Ok(m)
            })
            .with_context(err_context)?;
        Ok(module)
    }
    pub fn create_plugin_instance_environment_and_subscriptions(
        &mut self,
        module: Module,
    ) -> Result<(Instance, PluginEnv, Arc<Mutex<Subscriptions>>)> {
        let err_context = || {
            format!(
                "Failed to create instance and plugin env for plugin {}",
                self.plugin_id
            )
        };
        let mut wasi_env = WasiState::new("Zellij")
            .env("CLICOLOR_FORCE", "1")
            .map_dir("/host", ".")
            .and_then(|wasi| wasi.map_dir("/data", &self.plugin_own_data_dir))
            .and_then(|wasi| wasi.map_dir("/tmp", ZELLIJ_TMP_DIR.as_path()))
            .and_then(|wasi| {
                wasi.stdin(Box::new(Pipe::new()))
                    .stdout(Box::new(Pipe::new()))
                    .stderr(Box::new(LoggingPipe::new(
                        &self.plugin.location.to_string(),
                        self.plugin_id,
                    )))
                    .finalize()
            })
            .with_context(err_context)?;
        let wasi = wasi_env.import_object(&module).with_context(err_context)?;

        let mut mut_plugin = self.plugin.clone();
        mut_plugin.set_tab_index(self.tab_index);
        let plugin_env = PluginEnv {
            plugin_id: self.plugin_id,
            client_id: self.client_id,
            plugin: mut_plugin,
            senders: self.senders.clone(),
            wasi_env,
            plugin_own_data_dir: self.plugin_own_data_dir.clone(),
            tab_index: self.tab_index,
        };

        let subscriptions = Arc::new(Mutex::new(HashSet::new()));
        let zellij = zellij_exports(&self.store, &plugin_env, &subscriptions);
        let instance =
            Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
        assert_plugin_version(&instance, &plugin_env).with_context(err_context)?;
        // Only do an insert when everything went well!
        let cloned_plugin = self.plugin.clone();
        self.plugin_cache
            .lock()
            .unwrap()
            .insert(cloned_plugin.path, module);
        Ok((instance, plugin_env, subscriptions))
    }
    pub fn load_plugin_instance(
        &mut self,
        instance: &Instance,
        plugin_env: &PluginEnv,
        plugin_map: &Arc<Mutex<PluginMap>>,
        subscriptions: &Arc<Mutex<Subscriptions>>,
    ) -> Result<()> {
        let err_context = || format!("failed to load plugin from instance {instance:#?}");
        let main_user_instance = instance.clone();
        let main_user_env = plugin_env.clone();
        display_loading_stage!(
            indicate_starting_plugin,
            self.loading_indication,
            self.senders,
            self.plugin_id
        );
        let load_function = instance
            .exports
            .get_function("_start")
            .with_context(err_context)?;
        // This eventually calls the `.load()` method
        load_function.call(&[]).with_context(err_context)?;
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
        plugin_map.lock().unwrap().insert(
            (self.plugin_id, self.client_id),
            (
                Arc::new(Mutex::new(RunningPlugin::new(
                    main_user_instance,
                    main_user_env,
                    self.size.rows,
                    self.size.cols,
                ))),
                subscriptions.clone(),
            ),
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
                let mut loading_indication = LoadingIndication::new("".into());
                let mut plugin_loader_for_client = PluginLoader::new_from_different_client_id(
                    &self.plugin_cache.clone(),
                    &plugin_map,
                    &mut loading_indication,
                    &self.senders.clone(),
                    self.plugin_id,
                    *client_id,
                    &self.store,
                    &self.plugin_dir,
                )?;
                plugin_loader_for_client
                    .load_module_from_memory()
                    .and_then(|module| {
                        plugin_loader_for_client
                            .create_plugin_instance_environment_and_subscriptions(module)
                    })
                    .and_then(|(instance, plugin_env, subscriptions)| {
                        plugin_loader_for_client.load_plugin_instance(
                            &instance,
                            &plugin_env,
                            plugin_map,
                            &subscriptions,
                        )
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
    fn plugin_bytes_and_cache_path(&mut self) -> Result<(Vec<u8>, PathBuf)> {
        match self.wasm_blob_on_hd.as_ref() {
            Some((wasm_bytes, cached_path)) => Ok((wasm_bytes.clone(), cached_path.clone())),
            None => {
                if self.plugin._allow_exec_host_cmd {
                    info!(
                        "Plugin({:?}) is able to run any host command, this may lead to some security issues!",
                        self.plugin.path
                    );
                }
                // The plugins blob as stored on the filesystem
                let wasm_bytes = self.plugin.resolve_wasm_bytes(&self.plugin_dir)?;
                let hash: String = PortableHash::default()
                    .hash256(&wasm_bytes)
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                let cached_path = ZELLIJ_CACHE_DIR.join(&hash);
                self.wasm_blob_on_hd = Some((wasm_bytes.clone(), cached_path.clone()));
                Ok((wasm_bytes, cached_path))
            },
        }
    }
}

fn create_plugin_fs_entries(plugin_own_data_dir: &PathBuf) -> Result<()> {
    let err_context = || "failed to create plugin fs entries";
    // Create filesystem entries mounted into WASM.
    // We create them here to get expressive error messages in case they fail.
    fs::create_dir_all(&plugin_own_data_dir)
        .with_context(|| format!("failed to create datadir in {plugin_own_data_dir:?}"))
        .with_context(err_context)?;
    fs::create_dir_all(ZELLIJ_TMP_DIR.as_path())
        .with_context(|| format!("failed to create tmpdir at {:?}", &ZELLIJ_TMP_DIR.as_path()))
        .with_context(err_context)?;
    Ok(())
}
