use crate::plugins::{PluginInstruction, wasm_bridge::{wasi_read_string, zellij_exports, PluginEnv, PluginMap}};
use highway::{HighwayHash, PortableHash};
use log::info;
use semver::Version;
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
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

fn load_plugin_instance(instance: &mut Instance) -> Result<()> {
    let err_context = || format!("failed to load plugin from instance {instance:#?}");

    let load_function = instance
        .exports
        .get_function("_start")
        .with_context(err_context)?;
    // This eventually calls the `.load()` method
    load_function.call(&[]).with_context(err_context)?;
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
    mut store: Store,
    plugin_map: Arc<Mutex<PluginMap>>,
    size: Size,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    loading_indication: &mut LoadingIndication,
) -> Result<()> {
    let err_context = || format!("failed to start plugin {plugin:#?} for client {client_id}");
    let plugin_own_data_dir = ZELLIJ_CACHE_DIR.join(Url::from(&plugin.location).to_string());
    create_plugin_fs_entries(&plugin_own_data_dir)?;

    loading_indication.indicate_loading_plugin_from_memory();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    let (module, cache_hit) = {
        let mut plugin_cache = plugin_cache.lock().unwrap();
        let (module, cache_hit) = load_module_from_memory(&mut *plugin_cache, &plugin.path);
        (module, cache_hit)
    };

    let module = match module {
        Some(module) => {
            loading_indication.indicate_loading_plugin_from_memory_success();
            let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                plugin_id,
                loading_indication.clone(),
            ));
            module
        },
        None => {
            loading_indication.indicate_loading_plugin_from_memory_notfound();
            loading_indication.indicate_loading_plugin_from_hd_cache();
            let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                plugin_id,
                loading_indication.clone(),
            ));

            let (wasm_bytes, cached_path) = plugin_bytes_and_cache_path(&plugin, &plugin_dir)?;
            let timer = std::time::Instant::now();
            match load_module_from_hd_cache(&mut store, &plugin.path, &timer, &cached_path) {
                Ok(module) => {
                    loading_indication.indicate_loading_plugin_from_hd_cache_success();
                    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                        plugin_id,
                        loading_indication.clone(),
                    ));
                    module
                },
                Err(_e) => {
                    loading_indication.indicate_loading_plugin_from_hd_cache_notfound();
                    loading_indication.indicate_compiling_plugin();
                    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                        plugin_id,
                        loading_indication.clone(),
                    ));
                    let module =
                        compile_module(&mut store, &plugin.path, &timer, &cached_path, wasm_bytes)?;
                    loading_indication.indicate_compiling_plugin_success();
                    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
                        plugin_id,
                        loading_indication.clone(),
                    ));
                    module
                },
            }
        },
    };

    let (instance, plugin_env) = create_plugin_instance_and_environment(
        plugin_id,
        client_id,
        plugin,
        &module,
        tab_index,
        plugin_own_data_dir,
        senders.clone(),
        &mut store,
    )?;

    if !cache_hit {
        // Check plugin version
        assert_plugin_version(&instance, &plugin_env).with_context(err_context)?;
    }

    // Only do an insert when everything went well!
    let cloned_plugin = plugin.clone();
    {
        let mut plugin_cache = plugin_cache.lock().unwrap();
        plugin_cache.insert(cloned_plugin.path, module);
    }

    let mut main_user_instance = instance.clone();
    let main_user_env = plugin_env.clone();
    loading_indication.indicate_starting_plugin();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    load_plugin_instance(&mut main_user_instance).with_context(err_context)?;
    loading_indication.indicate_starting_plugin_success();
    loading_indication.indicate_writing_plugin_to_cache();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));

    {
        let mut plugin_map = plugin_map.lock().unwrap();
        plugin_map.insert(
            (plugin_id, client_id),
            (main_user_instance, main_user_env, (size.rows, size.cols)),
        );
    }

    loading_indication.indicate_writing_plugin_to_cache_success();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));

    let connected_clients: Vec<ClientId> =
        connected_clients.lock().unwrap().iter().copied().collect();
    if !connected_clients.is_empty() {
        loading_indication.indicate_cloning_plugin_for_other_clients();
        let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
            plugin_id,
            loading_indication.clone(),
        ));
        let mut plugin_map = plugin_map.lock().unwrap();
        for client_id in connected_clients {
            let (instance, new_plugin_env) =
                clone_plugin_for_client(&plugin_env, client_id, &instance, &mut store)?;
            plugin_map.insert(
                (plugin_id, client_id),
                (instance, new_plugin_env, (size.rows, size.cols)),
            );
        }
        loading_indication.indicate_cloning_plugin_for_other_clients_success();
        let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
            plugin_id,
            loading_indication.clone(),
        ));
    }
    loading_indication.end();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    Ok(())
}

pub fn reload_plugin(
    plugin_id: u32,
    plugin_dir: PathBuf,
    plugin_cache: Arc<Mutex<HashMap<PathBuf, Module>>>,
    senders: ThreadSenders,
    mut store: Store,
    plugin_map: Arc<Mutex<PluginMap>>,
    connected_clients: Arc<Mutex<Vec<ClientId>>>,
    loading_indication: &mut LoadingIndication,
) -> Result<()> {
    // TODO: reset cache??
    let err_context = || format!("failed to reload plugin id {plugin_id}");

    let mut connected_clients: Vec<ClientId> =
        connected_clients.lock().unwrap().iter().copied().collect();
    if connected_clients.is_empty() {
        return Err(anyhow!("No connected clients, cannot reload plugin"));
    }
    let (client_id, tab_index, size, plugin_config) = {
        let mut plugin_map = plugin_map.lock().unwrap();
        let first_client_id = connected_clients.remove(0); // TODO combine with error above
        log::info!("plugin_map: {:?}", plugin_map.keys());
        let (old_instance, old_user_env, (rows, cols)) = plugin_map.remove(&(plugin_id, first_client_id)).unwrap(); // TODO: proper error
        let tab_index = old_user_env.tab_index;
        let size = Size { rows, cols };
        let plugin_config = old_user_env.plugin.clone();
        loading_indication.set_name(old_user_env.name());
        (first_client_id, tab_index, size, plugin_config)
//         let mut plugin_map = plugin_map.lock().unwrap();
//         plugin_map.insert(
//             (plugin_id, client_id),
//             (main_user_instance, main_user_env, (size.rows, size.cols)),
//         );
    };


    let plugin_own_data_dir = ZELLIJ_CACHE_DIR.join(Url::from(&plugin_config.location).to_string());
    let (wasm_bytes, cached_path) = plugin_bytes_and_cache_path(&plugin_config, &plugin_dir)?;
    let timer = std::time::Instant::now();
    loading_indication.indicate_compiling_plugin();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    let module =
        compile_module(&mut store, &plugin_config.path, &timer, &cached_path, wasm_bytes)?;
    loading_indication.indicate_compiling_plugin_success();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));


    let (instance, plugin_env) = create_plugin_instance_and_environment(
        plugin_id,
        client_id,
        &plugin_config,
        &module,
        tab_index,
        plugin_own_data_dir,
        senders.clone(),
        &mut store,
    )?;

    assert_plugin_version(&instance, &plugin_env).with_context(err_context)?;

    // Only do an insert when everything went well!
    let cloned_plugin = plugin_config.clone();
    {
        let mut plugin_cache = plugin_cache.lock().unwrap();
        plugin_cache.insert(cloned_plugin.path, module);
    }

    let mut main_user_instance = instance.clone();
    let main_user_env = plugin_env.clone();
    loading_indication.indicate_starting_plugin();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    load_plugin_instance(&mut main_user_instance).with_context(err_context)?;
    loading_indication.indicate_starting_plugin_success();
    loading_indication.indicate_writing_plugin_to_cache();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));

    {
        let mut plugin_map = plugin_map.lock().unwrap();
        plugin_map.insert(
            (plugin_id, client_id),
            (main_user_instance, main_user_env, (size.rows, size.cols)),
        );
    }

    loading_indication.indicate_writing_plugin_to_cache_success();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));

//     let connected_clients: Vec<ClientId> =
//         connected_clients.lock().unwrap().iter().copied().collect();
    if !connected_clients.is_empty() {
        loading_indication.indicate_cloning_plugin_for_other_clients();
        let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
            plugin_id,
            loading_indication.clone(),
        ));
        let mut plugin_map = plugin_map.lock().unwrap();
        for client_id in connected_clients {
            let (instance, new_plugin_env) =
                clone_plugin_for_client(&plugin_env, client_id, &instance, &mut store)?;
            plugin_map.remove(&(plugin_id, client_id)); // TODO: is this safe?
            plugin_map.insert(
                (plugin_id, client_id),
                (instance, new_plugin_env, (size.rows, size.cols)),
            );
        }
        loading_indication.indicate_cloning_plugin_for_other_clients_success();
        let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
            plugin_id,
            loading_indication.clone(),
        ));
    }
    loading_indication.end();
    let _ = senders.send_to_screen(ScreenInstruction::UpdatePluginLoadingStage(
        plugin_id,
        loading_indication.clone(),
    ));
    let _ = senders.send_to_plugin(PluginInstruction::Resize(
        plugin_id,
        size.cols,
        size.rows,
    ));
    Ok(())
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

fn compile_module(
    store: &mut Store,
    plugin_path: &PathBuf,
    timer: &Instant,
    cached_path: &PathBuf,
    wasm_bytes: Vec<u8>,
) -> Result<Module> {
    let err_context = || "failed to recover cache dir";
    fs::create_dir_all(ZELLIJ_CACHE_DIR.to_owned())
        .map_err(anyError::new)
        .and_then(|_| {
            // compile module
            Module::new(&*store, &wasm_bytes).map_err(anyError::new)
        })
        .map(|m| {
            // serialize module to HD cache for faster loading in the future
            m.serialize_to_file(&cached_path).map_err(anyError::new)?;
            log::info!(
                "Compiled plugin '{}' in {:?}",
                plugin_path.display(),
                timer.elapsed()
            );
            Ok(m)
        })
        .with_context(err_context)?
}

fn load_module_from_hd_cache(
    store: &mut Store,
    plugin_path: &PathBuf,
    timer: &Instant,
    cached_path: &PathBuf,
) -> Result<Module> {
    let module = unsafe { Module::deserialize_from_file(&*store, &cached_path)? };
    log::info!(
        "Loaded plugin '{}' from cache folder at '{}' in {:?}",
        plugin_path.display(),
        ZELLIJ_CACHE_DIR.display(),
        timer.elapsed(),
    );
    Ok(module)
}

fn plugin_bytes_and_cache_path(plugin: &PluginConfig, plugin_dir: &PathBuf) -> Result<(Vec<u8>, PathBuf)> {
    // Populate plugin module cache for this plugin!
    // Is it in the cache folder already?
    if plugin._allow_exec_host_cmd {
        info!(
            "Plugin({:?}) is able to run any host command, this may lead to some security issues!",
            plugin.path
        );
    }
    // The plugins blob as stored on the filesystem
    let wasm_bytes = plugin.resolve_wasm_bytes(&plugin_dir)?;
    let hash: String = PortableHash::default()
        .hash256(&wasm_bytes)
        .iter()
        .map(ToString::to_string)
        .collect();
    let cached_path = ZELLIJ_CACHE_DIR.join(&hash);
    Ok((wasm_bytes, cached_path))
}

fn load_module_from_memory(
    plugin_cache: &mut HashMap<PathBuf, Module>,
    plugin_path: &PathBuf,
) -> (Option<Module>, bool) {
    let module = plugin_cache.remove(plugin_path);
    let mut cache_hit = false;
    if module.is_some() {
        cache_hit = true;
        log::debug!(
            "Loaded plugin '{}' from plugin cache",
            plugin_path.display()
        );
    }
    (module, cache_hit)
}

fn create_plugin_instance_and_environment(
    plugin_id: u32,
    client_id: ClientId,
    plugin: &PluginConfig,
    module: &Module,
    tab_index: usize,
    plugin_own_data_dir: PathBuf,
    senders: ThreadSenders,
    store: &mut Store,
) -> Result<(Instance, PluginEnv)> {
    let err_context = || format!("Failed to create instance and plugin env for plugin {plugin_id}");
    let mut wasi_env = WasiState::new("Zellij")
        .env("CLICOLOR_FORCE", "1")
        .map_dir("/host", ".")
        .and_then(|wasi| wasi.map_dir("/data", &plugin_own_data_dir))
        .and_then(|wasi| wasi.map_dir("/tmp", ZELLIJ_TMP_DIR.as_path()))
        .and_then(|wasi| {
            wasi.stdin(Box::new(Pipe::new()))
                .stdout(Box::new(Pipe::new()))
                .stderr(Box::new(LoggingPipe::new(
                    &plugin.location.to_string(),
                    plugin_id,
                )))
                .finalize()
        })
        .with_context(err_context)?;
    let wasi = wasi_env.import_object(&module).with_context(err_context)?;

    let mut mut_plugin = plugin.clone();
    mut_plugin.set_tab_index(tab_index);
    let plugin_env = PluginEnv {
        plugin_id,
        client_id,
        plugin: mut_plugin,
        senders: senders.clone(),
        wasi_env,
        subscriptions: Arc::new(Mutex::new(HashSet::new())),
        plugin_own_data_dir,
        tab_index,
    };

    let zellij = zellij_exports(&store, &plugin_env);
    let instance = Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
    Ok((instance, plugin_env))
}

fn clone_plugin_for_client(
    plugin_env: &PluginEnv,
    client_id: ClientId,
    instance: &Instance,
    store: &Store,
) -> Result<(Instance, PluginEnv)> {
    let err_context = || format!("Failed to clone plugin for client {client_id}");
    let mut new_plugin_env = plugin_env.clone();
    new_plugin_env.client_id = client_id;
    let module = instance.module().clone();
    let wasi = new_plugin_env
        .wasi_env
        .import_object(&module)
        .with_context(err_context)?;
    let zellij = zellij_exports(store, &new_plugin_env);
    let mut instance =
        Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;
    load_plugin_instance(&mut instance).with_context(err_context)?;
    Ok((instance, new_plugin_env))
}
