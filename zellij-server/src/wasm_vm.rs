use highway::{HighwayHash, PortableHash};
use log::{debug, info, warn};
use semver::Version;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fmt, fs,
    path::{Path, PathBuf},
    process,
    str::FromStr,
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use url::Url;
use wasmer::{
    imports, ChainableNamedResolver, Function, ImportObject, Instance, Module, Store, Value,
    WasmerEnv,
};
use wasmer_wasi::{Pipe, WasiEnv, WasiState};

use crate::{
    logging_pipe::LoggingPipe,
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    ClientId,
};

use zellij_utils::{
    consts::{DEBUG_MODE, VERSION, ZELLIJ_CACHE_DIR, ZELLIJ_TMP_DIR},
    data::{Event, EventType, PluginIds},
    errors::{prelude::*, ContextType, PluginContext},
    input::{
        command::TerminalAction,
        layout::RunPlugin,
        plugins::{PluginConfig, PluginType, PluginsConfig},
    },
    pane_size::Size,
    serde,
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
    is to build zellij with `cargo make install`. Also refer to the docs:
    https://github.com/zellij-org/zellij/blob/main/CONTRIBUTING.md#building
",
            first_line,
            self.plugin_version.trim_end(),
            self.zellij_version.trim_end(),
            self.plugin_path.display()
        )
    }
}

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, RunPlugin, usize, ClientId, Size), // tx_pid, plugin metadata, tab_index, client_ids
    Update(Option<u32>, Option<ClientId>, Event), // Focused plugin / broadcast, client_id, event data
    Unload(u32),                                  // plugin_id
    Resize(u32, usize, usize),                    // plugin_id, columns, rows
    AddClient(ClientId),
    RemoveClient(ClientId),
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Unload(..) => PluginContext::Unload,
            PluginInstruction::Resize(..) => PluginContext::Resize,
            PluginInstruction::Exit => PluginContext::Exit,
            PluginInstruction::AddClient(_) => PluginContext::AddClient,
            PluginInstruction::RemoveClient(_) => PluginContext::RemoveClient,
        }
    }
}

#[derive(WasmerEnv, Clone)]
pub(crate) struct PluginEnv {
    pub plugin_id: u32,
    pub plugin: PluginConfig,
    pub senders: ThreadSenders,
    pub wasi_env: WasiEnv,
    pub subscriptions: Arc<Mutex<HashSet<EventType>>>,
    pub tab_index: usize,
    pub client_id: ClientId,
    #[allow(dead_code)]
    plugin_own_data_dir: PathBuf,
}

// Thread main --------------------------------------------------------------------------------------------------------
pub(crate) fn wasm_thread_main(
    bus: Bus<PluginInstruction>,
    store: Store,
    data_dir: PathBuf,
    plugins: PluginsConfig,
) -> Result<()> {
    info!("Wasm main thread starts");

    let mut plugin_id = 0;
    let mut headless_plugins = HashMap::new();
    let mut plugin_map: HashMap<(u32, ClientId), (Instance, PluginEnv, (usize, usize))> =
        HashMap::new(); // u32 => pid, (usize, usize) => rows/columns TODO: clean this up into a struct or something
    let mut connected_clients: Vec<ClientId> = vec![];
    let plugin_dir = data_dir.join("plugins/");
    let plugin_global_data_dir = plugin_dir.join("data");
    // Caches the "wasm bytes" of all plugins that have been loaded since zellij was started.
    // Greatly decreases loading times of all plugins and avoids accesses to the hard-drive during
    // "regular" operation.
    let mut plugin_cache: HashMap<PathBuf, Module> = HashMap::new();

    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Plugin((&event).into()));
        match event {
            PluginInstruction::Load(pid_tx, run, tab_index, client_id, size) => {
                let err_context = || format!("failed to load plugin for client {client_id}");

                let plugin = plugins
                    .get(&run)
                    .with_context(|| format!("failed to resolve plugin {run:?}"))
                    .with_context(err_context)
                    .fatal();

                let (instance, plugin_env) = start_plugin(
                    plugin_id,
                    client_id,
                    &plugin,
                    tab_index,
                    &bus,
                    &store,
                    &plugin_dir,
                    &mut plugin_cache,
                )
                .with_context(err_context)?;

                let mut main_user_instance = instance.clone();
                let main_user_env = plugin_env.clone();
                load_plugin(&mut main_user_instance).with_context(err_context)?;

                plugin_map.insert(
                    (plugin_id, client_id),
                    (main_user_instance, main_user_env, (size.rows, size.cols)),
                );

                // clone plugins for the rest of the client ids if they exist
                for client_id in connected_clients.iter() {
                    let mut new_plugin_env = plugin_env.clone();
                    new_plugin_env.client_id = *client_id;
                    let module = instance.module().clone();
                    let wasi = new_plugin_env
                        .wasi_env
                        .import_object(&module)
                        .with_context(err_context)?;
                    let zellij = zellij_exports(&store, &new_plugin_env);
                    let mut instance = Instance::new(&module, &zellij.chain_back(wasi))
                        .with_context(err_context)?;
                    load_plugin(&mut instance).with_context(err_context)?;
                    plugin_map.insert(
                        (plugin_id, *client_id),
                        (instance, new_plugin_env, (size.rows, size.cols)),
                    );
                }
                pid_tx.send(plugin_id).with_context(err_context)?;
                plugin_id += 1;
            },
            PluginInstruction::Update(pid, cid, event) => {
                let err_context = || {
                    if *DEBUG_MODE.get().unwrap_or(&true) {
                        format!("failed to update plugin state with event: {event:#?}")
                    } else {
                        "failed to update plugin state".to_string()
                    }
                };

                for (&(plugin_id, client_id), (instance, plugin_env, (rows, columns))) in
                    &plugin_map
                {
                    let subs = plugin_env
                        .subscriptions
                        .lock()
                        .to_anyhow()
                        .with_context(err_context)?;
                    // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                    let event_type =
                        EventType::from_str(&event.to_string()).with_context(err_context)?;
                    if subs.contains(&event_type)
                        && ((pid.is_none() && cid.is_none())
                            || (pid.is_none() && cid == Some(client_id))
                            || (cid.is_none() && pid == Some(plugin_id))
                            || (cid == Some(client_id) && pid == Some(plugin_id)))
                    {
                        let update = instance
                            .exports
                            .get_function("update")
                            .with_context(err_context)?;
                        wasi_write_object(&plugin_env.wasi_env, &event);
                        let update_return =
                            update.call(&[]).or_else::<anyError, _>(|e| match e
                                .downcast::<serde_json::Error>()
                            {
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

                        if *rows > 0 && *columns > 0 && should_render {
                            let render = instance
                                .exports
                                .get_function("render")
                                .with_context(err_context)?;
                            render
                                .call(&[Value::I32(*rows as i32), Value::I32(*columns as i32)])
                                .with_context(err_context)?;
                            let rendered_bytes = wasi_read_string(&plugin_env.wasi_env);
                            drop(bus.senders.send_to_screen(ScreenInstruction::PluginBytes(
                                plugin_id,
                                client_id,
                                rendered_bytes.as_bytes().to_vec(),
                            )));
                        }
                    }
                }
            },
            PluginInstruction::Unload(pid) => {
                info!("Bye from plugin {}", &pid);
                // TODO: remove plugin's own data directory
                let ids_in_plugin_map: Vec<(u32, ClientId)> = plugin_map.keys().copied().collect();
                for (plugin_id, client_id) in ids_in_plugin_map {
                    if pid == plugin_id {
                        drop(plugin_map.remove(&(plugin_id, client_id)));
                    }
                }
            },
            PluginInstruction::Resize(pid, new_columns, new_rows) => {
                let err_context = || format!("failed to resize plugin {pid}");
                for (
                    (plugin_id, client_id),
                    (instance, plugin_env, (current_rows, current_columns)),
                ) in plugin_map.iter_mut()
                {
                    if *plugin_id == pid {
                        *current_rows = new_rows;
                        *current_columns = new_columns;

                        // TODO: consolidate with above render function
                        let render = instance
                            .exports
                            .get_function("render")
                            .with_context(err_context)?;

                        render
                            .call(&[
                                Value::I32(*current_rows as i32),
                                Value::I32(*current_columns as i32),
                            ])
                            .with_context(err_context)?;
                        let rendered_bytes = wasi_read_string(&plugin_env.wasi_env);
                        drop(bus.senders.send_to_screen(ScreenInstruction::PluginBytes(
                            *plugin_id,
                            *client_id,
                            rendered_bytes.as_bytes().to_vec(),
                        )));
                    }
                }
            },
            PluginInstruction::AddClient(client_id) => {
                let err_context = || format!("failed to add plugins for client {client_id}");

                connected_clients.push(client_id);

                let mut seen = HashSet::new();
                let mut new_plugins = HashMap::new();
                for (&(plugin_id, _), (instance, plugin_env, (rows, columns))) in &plugin_map {
                    if seen.contains(&plugin_id) {
                        continue;
                    }
                    seen.insert(plugin_id);
                    let mut new_plugin_env = plugin_env.clone();

                    new_plugin_env.client_id = client_id;
                    new_plugins.insert(
                        plugin_id,
                        (instance.module().clone(), new_plugin_env, (*rows, *columns)),
                    );
                }
                for (plugin_id, (module, mut new_plugin_env, (rows, columns))) in
                    new_plugins.drain()
                {
                    let wasi = new_plugin_env
                        .wasi_env
                        .import_object(&module)
                        .with_context(err_context)?;
                    let zellij = zellij_exports(&store, &new_plugin_env);
                    let mut instance = Instance::new(&module, &zellij.chain_back(wasi))
                        .with_context(err_context)?;
                    load_plugin(&mut instance).with_context(err_context)?;
                    plugin_map.insert(
                        (plugin_id, client_id),
                        (instance, new_plugin_env, (rows, columns)),
                    );
                }

                // load headless plugins
                for plugin in plugins.iter() {
                    if let PluginType::Headless = plugin.run {
                        let (instance, plugin_env) = start_plugin(
                            plugin_id,
                            client_id,
                            plugin,
                            0,
                            &bus,
                            &store,
                            &plugin_dir,
                            &mut plugin_cache,
                        )
                        .with_context(err_context)?;
                        headless_plugins.insert(plugin_id, (instance, plugin_env));
                        plugin_id += 1;
                    }
                }
            },
            PluginInstruction::RemoveClient(client_id) => {
                connected_clients.retain(|c| c != &client_id);
            },
            PluginInstruction::Exit => break,
        }
    }
    info!("wasm main thread exits");

    fs::remove_dir_all(&plugin_global_data_dir)
        .or_else(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                // I don't care...
                Ok(())
            } else {
                Err(err)
            }
        })
        .context("failed to cleanup plugin data directory")
}

#[allow(clippy::too_many_arguments)]
fn start_plugin(
    plugin_id: u32,
    client_id: ClientId,
    plugin: &PluginConfig,
    tab_index: usize,
    bus: &Bus<PluginInstruction>,
    store: &Store,
    plugin_dir: &Path,
    plugin_cache: &mut HashMap<PathBuf, Module>,
) -> Result<(Instance, PluginEnv)> {
    let err_context = || format!("failed to start plugin {plugin:#?} for client {client_id}");

    let plugin_own_data_dir = ZELLIJ_CACHE_DIR.join(Url::from(&plugin.location).to_string());
    let cache_hit = plugin_cache.contains_key(&plugin.path);

    // We remove the entry here and repopulate it at the very bottom, if everything went well.
    // We must do that because a `get` will only give us a borrow of the Module. This suffices for
    // the purpose of setting everything up, but we cannot return a &Module from the "None" match
    // arm, because we create the Module from scratch there. Any reference passed outside would
    // outlive the Module we create there. Hence, we remove the plugin here and reinsert it
    // below...
    let module = match plugin_cache.remove(&plugin.path) {
        Some(module) => {
            log::debug!(
                "Loaded plugin '{}' from plugin cache",
                plugin.path.display()
            );
            module
        },
        None => {
            // Populate plugin module cache for this plugin!
            // Is it in the cache folder already?
            if plugin._allow_exec_host_cmd {
                info!(
                    "Plugin({:?}) is able to run any host command, this may lead to some security issues!",
                    plugin.path
                );
            }

            // The plugins blob as stored on the filesystem
            let wasm_bytes = plugin
                .resolve_wasm_bytes(plugin_dir)
                .with_context(err_context)
                .fatal();

            fs::create_dir_all(&plugin_own_data_dir)
                .with_context(|| format!("failed to create datadir in {plugin_own_data_dir:?}"))
                .with_context(err_context)
                .non_fatal();

            // ensure tmp dir exists, in case it somehow was deleted (e.g systemd-tmpfiles)
            fs::create_dir_all(ZELLIJ_TMP_DIR.as_path())
                .with_context(|| {
                    format!("failed to create tmpdir at {:?}", &ZELLIJ_TMP_DIR.as_path())
                })
                .with_context(err_context)
                .non_fatal();

            let hash: String = PortableHash::default()
                .hash256(&wasm_bytes)
                .iter()
                .map(ToString::to_string)
                .collect();
            let cached_path = ZELLIJ_CACHE_DIR.join(&hash);

            unsafe {
                match Module::deserialize_from_file(store, &cached_path) {
                    Ok(m) => {
                        log::debug!(
                            "Loaded plugin '{}' from cache folder at '{}'",
                            plugin.path.display(),
                            ZELLIJ_CACHE_DIR.display(),
                        );
                        m
                    },
                    Err(e) => {
                        let inner_context = || format!("failed to recover from {e:?}");

                        let m = Module::new(store, &wasm_bytes)
                            .with_context(inner_context)
                            .with_context(err_context)?;
                        fs::create_dir_all(ZELLIJ_CACHE_DIR.to_owned())
                            .with_context(inner_context)
                            .with_context(err_context)?;
                        m.serialize_to_file(&cached_path)
                            .with_context(inner_context)
                            .with_context(err_context)?;
                        m
                    },
                }
            }
        },
    };

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
        senders: bus.senders.clone(),
        wasi_env,
        subscriptions: Arc::new(Mutex::new(HashSet::new())),
        plugin_own_data_dir,
        tab_index,
    };

    let zellij = zellij_exports(store, &plugin_env);
    let instance = Instance::new(&module, &zellij.chain_back(wasi)).with_context(err_context)?;

    if !cache_hit {
        // Check plugin version
        assert_plugin_version(&instance, &plugin_env).with_context(err_context)?;
    }

    // Only do an insert when everything went well!
    let cloned_plugin = plugin.clone();
    plugin_cache.insert(cloned_plugin.path, module);

    Ok((instance, plugin_env))
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
    plugin_version_func.call(&[]).with_context(err_context)?;
    let plugin_version_str = wasi_read_string(&plugin_env.wasi_env);
    let plugin_version = Version::parse(&plugin_version_str)
        .context("failed to parse plugin version")
        .with_context(err_context)?;
    let zellij_version = Version::parse(VERSION)
        .context("failed to parse zellij version")
        .with_context(err_context)?;
    if plugin_version != zellij_version {
        return Err(anyError::new(VersionMismatchError::new(
            VERSION,
            &plugin_version_str,
            &plugin_env.plugin.path,
            plugin_env.plugin.is_builtin(),
        )));
    }

    Ok(())
}

fn load_plugin(instance: &mut Instance) -> Result<()> {
    let err_context = || format!("failed to load plugin from instance {instance:#?}");

    let load_function = instance
        .exports
        .get_function("_start")
        .with_context(err_context)?;
    // This eventually calls the `.load()` method
    load_function.call(&[]).with_context(err_context)?;
    Ok(())
}

// Plugin API ---------------------------------------------------------------------------------------------------------

pub(crate) fn zellij_exports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
    macro_rules! zellij_export {
        ($($host_function:ident),+ $(,)?) => {
            imports! {
                "zellij" => {
                    $(stringify!($host_function) =>
                        Function::new_native_with_env(store, plugin_env.clone(), $host_function),)+
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

fn host_subscribe(plugin_env: &PluginEnv) {
    let mut subscriptions = plugin_env.subscriptions.lock().unwrap();
    let new: HashSet<EventType> = wasi_read_object(&plugin_env.wasi_env);
    subscriptions.extend(new);
}

fn host_unsubscribe(plugin_env: &PluginEnv) {
    let mut subscriptions = plugin_env.subscriptions.lock().unwrap();
    let old: HashSet<EventType> = wasi_read_object(&plugin_env.wasi_env);
    subscriptions.retain(|k| !old.contains(k));
}

fn host_set_selectable(plugin_env: &PluginEnv, selectable: i32) {
    match plugin_env.plugin.run {
        PluginType::Pane(Some(tab_index)) => {
            let selectable = selectable != 0;
            plugin_env
                .senders
                .send_to_screen(ScreenInstruction::SetSelectable(
                    PaneId::Plugin(plugin_env.plugin_id),
                    selectable,
                    tab_index,
                ))
                .unwrap()
        },
        _ => {
            debug!(
                "{} - Calling method 'host_set_selectable' does nothing for headless plugins",
                plugin_env.plugin.location
            )
        },
    }
}

fn host_get_plugin_ids(plugin_env: &PluginEnv) {
    let ids = PluginIds {
        plugin_id: plugin_env.plugin_id,
        zellij_pid: process::id(),
    };
    wasi_write_object(&plugin_env.wasi_env, &ids);
}

fn host_get_zellij_version(plugin_env: &PluginEnv) {
    wasi_write_object(&plugin_env.wasi_env, VERSION);
}

fn host_open_file(plugin_env: &PluginEnv) {
    let path: PathBuf = wasi_read_object(&plugin_env.wasi_env);
    plugin_env
        .senders
        .send_to_pty(PtyInstruction::SpawnTerminal(
            Some(TerminalAction::OpenFile(path, None)),
            None,
            None,
            ClientOrTabIndex::TabIndex(plugin_env.tab_index),
        ))
        .unwrap();
}

fn host_switch_tab_to(plugin_env: &PluginEnv, tab_idx: u32) {
    plugin_env
        .senders
        .send_to_screen(ScreenInstruction::GoToTab(
            tab_idx,
            Some(plugin_env.client_id),
        ))
        .unwrap();
}

fn host_set_timeout(plugin_env: &PluginEnv, secs: f64) {
    // There is a fancy, high-performance way to do this with zero additional threads:
    // If the plugin thread keeps a BinaryHeap of timer structs, it can manage multiple and easily `.peek()` at the
    // next time to trigger in O(1) time. Once the wake-up time is known, the `wasm` thread can use `recv_timeout()`
    // to wait for an event with the timeout set to be the time of the next wake up. If events come in in the meantime,
    // they are handled, but if the timeout triggers, we replace the event from `recv()` with an
    // `Update(pid, TimerEvent)` and pop the timer from the Heap (or reschedule it). No additional threads for as many
    // timers as we'd like.
    //
    // But that's a lot of code, and this is a few lines:
    let send_plugin_instructions = plugin_env.senders.to_plugin.clone();
    let update_target = Some(plugin_env.plugin_id);
    let client_id = plugin_env.client_id;
    thread::spawn(move || {
        let start_time = Instant::now();
        thread::sleep(Duration::from_secs_f64(secs));
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
            .unwrap()
            .send(PluginInstruction::Update(
                update_target,
                Some(client_id),
                Event::Timer(elapsed_time),
            ))
            .unwrap();
    });
}

fn host_exec_cmd(plugin_env: &PluginEnv) {
    let mut cmdline: Vec<String> = wasi_read_object(&plugin_env.wasi_env);
    let command = cmdline.remove(0);

    // Bail out if we're forbidden to run command
    if !plugin_env.plugin._allow_exec_host_cmd {
        warn!("This plugin isn't allow to run command in host side, skip running this command: '{cmd} {args}'.",
        	cmd = command, args = cmdline.join(" "));
        return;
    }

    // Here, we don't wait the command to finish
    process::Command::new(command)
        .args(cmdline)
        .spawn()
        .unwrap();
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn host_report_panic(plugin_env: &PluginEnv) {
    let msg = wasi_read_string(&plugin_env.wasi_env);
    panic!("{}", msg);
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> String {
    let mut state = wasi_env.state();
    let wasi_file = state.fs.stdout_mut().unwrap().as_mut().unwrap();
    let mut buf = String::new();
    wasi_file.read_to_string(&mut buf).unwrap();
    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    buf.replace("\n", "\n\r")
}

pub fn wasi_write_string(wasi_env: &WasiEnv, buf: &str) {
    let mut state = wasi_env.state();
    let wasi_file = state.fs.stdin_mut().unwrap().as_mut().unwrap();
    writeln!(wasi_file, "{}\r", buf).unwrap();
}

pub fn wasi_write_object(wasi_env: &WasiEnv, object: &(impl Serialize + ?Sized)) {
    wasi_write_string(wasi_env, &serde_json::to_string(&object).unwrap());
}

pub fn wasi_read_object<T: DeserializeOwned>(wasi_env: &WasiEnv) -> T {
    let json = wasi_read_string(wasi_env);
    serde_json::from_str(&json).unwrap()
}
