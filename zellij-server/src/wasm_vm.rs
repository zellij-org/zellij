use highway::{HighwayHash, PortableHash};
use log::{debug, info, warn};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
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
use zellij_tile::data::{Event, EventType, PluginIds};

use crate::{
    logging_pipe::LoggingPipe,
    panes::PaneId,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
    ClientId,
};

use zellij_utils::{
    consts::{VERSION, ZELLIJ_PROJ_DIR, ZELLIJ_TMP_DIR},
    errors::{ContextType, PluginContext},
};
use zellij_utils::{
    input::command::TerminalAction,
    input::layout::RunPlugin,
    input::plugins::{PluginConfig, PluginType, PluginsConfig},
    serde, zellij_tile,
};

#[derive(Clone, Debug)]
pub(crate) enum PluginInstruction {
    Load(Sender<u32>, RunPlugin, usize, ClientId), // tx_pid, plugin metadata, tab_index, client_ids
    Update(Option<u32>, Option<ClientId>, Event), // Focused plugin / broadcast, client_id, event data
    Render(Sender<String>, u32, ClientId, usize, usize), // String buffer, plugin id, client_id, rows, cols
    Unload(u32),                                         // plugin_id
    AddClient(ClientId),
    RemoveClient(ClientId),
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Render(..) => PluginContext::Render,
            PluginInstruction::Unload(..) => PluginContext::Unload,
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
    plugin_own_data_dir: PathBuf,
}

// Thread main --------------------------------------------------------------------------------------------------------
pub(crate) fn wasm_thread_main(
    bus: Bus<PluginInstruction>,
    store: Store,
    data_dir: PathBuf,
    plugins: PluginsConfig,
) {
    info!("Wasm main thread starts");

    let mut plugin_id = 0;
    let mut headless_plugins = HashMap::new();
    let mut plugin_map: HashMap<(u32, ClientId), (Instance, PluginEnv)> = HashMap::new(); // u32 => pid
    let mut connected_clients: Vec<ClientId> = vec![];
    let plugin_dir = data_dir.join("plugins/");
    let plugin_global_data_dir = plugin_dir.join("data");
    fs::create_dir_all(&plugin_global_data_dir).unwrap();

    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Plugin((&event).into()));
        match event {
            PluginInstruction::Load(pid_tx, run, tab_index, client_id) => {
                let plugin = plugins
                    .get(&run)
                    .unwrap_or_else(|| panic!("Plugin {:?} could not be resolved", run));

                let (instance, plugin_env) = start_plugin(
                    plugin_id,
                    client_id,
                    &plugin,
                    tab_index,
                    &bus,
                    &store,
                    &data_dir,
                    &plugin_global_data_dir,
                );

                let mut main_user_instance = instance.clone();
                let main_user_env = plugin_env.clone();
                load_plugin(&mut main_user_instance);

                plugin_map.insert((plugin_id, client_id), (main_user_instance, main_user_env));

                // clone plugins for the rest of the client ids if they exist
                for client_id in connected_clients.iter() {
                    let mut new_plugin_env = plugin_env.clone();
                    new_plugin_env.client_id = *client_id;
                    let module = instance.module().clone();
                    let wasi = new_plugin_env.wasi_env.import_object(&module).unwrap();
                    let zellij = zellij_exports(&store, &new_plugin_env);
                    let mut instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();
                    load_plugin(&mut instance);
                    plugin_map.insert((plugin_id, *client_id), (instance, new_plugin_env));
                }
                pid_tx.send(plugin_id).unwrap();
                plugin_id += 1;
            }
            PluginInstruction::Update(pid, cid, event) => {
                for (&(plugin_id, client_id), (instance, plugin_env)) in &plugin_map {
                    let subs = plugin_env.subscriptions.lock().unwrap();
                    // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                    let event_type = EventType::from_str(&event.to_string()).unwrap();
                    if subs.contains(&event_type)
                        && ((pid.is_none() && cid.is_none())
                            || (pid.is_none() && cid == Some(client_id))
                            || (cid.is_none() && pid == Some(plugin_id)))
                    {
                        let update = instance.exports.get_function("update").unwrap();
                        wasi_write_object(&plugin_env.wasi_env, &event);
                        update.call(&[]).unwrap();
                    }
                }
                drop(bus.senders.send_to_screen(ScreenInstruction::Render));
            }
            PluginInstruction::Render(buf_tx, pid, cid, rows, cols) => {
                if rows == 0 || cols == 0 {
                    buf_tx.send(String::new()).unwrap();
                } else {
                    let (instance, plugin_env) = plugin_map.get(&(pid, cid)).unwrap();
                    let render = instance.exports.get_function("render").unwrap();

                    render
                        .call(&[Value::I32(rows as i32), Value::I32(cols as i32)])
                        .unwrap();

                    buf_tx.send(wasi_read_string(&plugin_env.wasi_env)).unwrap();
                }
            }
            PluginInstruction::Unload(pid) => {
                info!("Bye from plugin {}", &pid);
                // TODO: remove plugin's own data directory
                let ids_in_plugin_map: Vec<(u32, ClientId)> = plugin_map.keys().copied().collect();
                for (plugin_id, client_id) in ids_in_plugin_map {
                    if pid == plugin_id {
                        drop(plugin_map.remove(&(plugin_id, client_id)));
                    }
                }
            }
            PluginInstruction::AddClient(client_id) => {
                connected_clients.push(client_id);
                let mut seen = HashSet::new();
                let mut new_plugins = HashMap::new();
                for (&(plugin_id, client_id), (instance, plugin_env)) in &plugin_map {
                    if seen.contains(&plugin_id) {
                        continue;
                    } else {
                        seen.insert(plugin_id);
                        let mut new_plugin_env = plugin_env.clone();
                        new_plugin_env.client_id = client_id;
                        new_plugins.insert(plugin_id, (instance.module().clone(), new_plugin_env));
                    }
                }
                for (plugin_id, (module, mut new_plugin_env)) in new_plugins.drain() {
                    let wasi = new_plugin_env.wasi_env.import_object(&module).unwrap();
                    let zellij = zellij_exports(&store, &new_plugin_env);
                    let mut instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();
                    load_plugin(&mut instance);
                    plugin_map.insert((plugin_id, client_id), (instance, new_plugin_env));
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
                            &data_dir,
                            &plugin_global_data_dir,
                        );
                        headless_plugins.insert(plugin_id, (instance, plugin_env));
                        plugin_id += 1;
                    }
                }
            }
            PluginInstruction::RemoveClient(client_id) => {
                connected_clients.retain(|c| c != &client_id);
            }
            PluginInstruction::Exit => break,
        }
    }
    info!("wasm main thread exits");
    fs::remove_dir_all(&plugin_global_data_dir).unwrap();
}

#[allow(clippy::too_many_arguments)]
fn start_plugin(
    plugin_id: u32,
    client_id: ClientId,
    plugin: &PluginConfig,
    tab_index: usize,
    bus: &Bus<PluginInstruction>,
    store: &Store,
    data_dir: &Path,
    plugin_global_data_dir: &Path,
) -> (Instance, PluginEnv) {
    if plugin._allow_exec_host_cmd {
        info!(
            "Plugin({:?}) is able to run any host command, this may lead to some security issues!",
            plugin.path
        );
    }

    let wasm_bytes = plugin
        .resolve_wasm_bytes(&data_dir.join("plugins/"))
        .unwrap_or_else(|| panic!("Cannot resolve wasm bytes for plugin {:?}", plugin));

    let hash: String = PortableHash::default()
        .hash256(&wasm_bytes)
        .iter()
        .map(ToString::to_string)
        .collect();

    let cached_path = ZELLIJ_PROJ_DIR.cache_dir().join(&hash);

    let module = unsafe {
        Module::deserialize_from_file(store, &cached_path).unwrap_or_else(|_| {
            let m = Module::new(store, &wasm_bytes).unwrap();
            fs::create_dir_all(ZELLIJ_PROJ_DIR.cache_dir()).unwrap();
            m.serialize_to_file(&cached_path).unwrap();
            m
        })
    };

    let output = Pipe::new();
    let input = Pipe::new();
    let stderr = LoggingPipe::new(&plugin.location.to_string(), plugin_id);
    let plugin_own_data_dir = plugin_global_data_dir.join(Url::from(&plugin.location).to_string());
    fs::create_dir_all(&plugin_own_data_dir).unwrap();

    let mut wasi_env = WasiState::new("Zellij")
        .env("CLICOLOR_FORCE", "1")
        .map_dir("/host", ".")
        .unwrap()
        .map_dir("/data", &plugin_own_data_dir)
        .unwrap()
        .map_dir("/tmp", ZELLIJ_TMP_DIR.as_path())
        .unwrap()
        .stdin(Box::new(input))
        .stdout(Box::new(output))
        .stderr(Box::new(stderr))
        .finalize()
        .unwrap();

    let wasi = wasi_env.import_object(&module).unwrap();
    let mut plugin = plugin.clone();
    plugin.set_tab_index(tab_index);

    let plugin_env = PluginEnv {
        plugin_id,
        client_id,
        plugin,
        senders: bus.senders.clone(),
        wasi_env,
        subscriptions: Arc::new(Mutex::new(HashSet::new())),
        plugin_own_data_dir,
        tab_index,
    };

    let zellij = zellij_exports(store, &plugin_env);
    let instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();

    (instance, plugin_env)
}

fn load_plugin(instance: &mut Instance) {
    let load_function = instance.exports.get_function("_start").unwrap();

    // This eventually calls the `.load()` method
    load_function.call(&[]).unwrap();
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
        }
        _ => {
            debug!(
                "{} - Calling method 'host_set_selectable' does nothing for headless plugins",
                plugin_env.plugin.location
            )
        }
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
            Some(TerminalAction::OpenFile(path)),
            ClientOrTabIndex::TabIndex(plugin_env.tab_index),
        ))
        .unwrap();
}

fn host_switch_tab_to(plugin_env: &PluginEnv, tab_idx: u32) {
    plugin_env
        .senders
        .send_to_screen(ScreenInstruction::GoToTab(tab_idx, None)) // this is a hack, we should be able to return the client id here
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

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(wasi_env: &WasiEnv) -> String {
    let mut state = wasi_env.state();
    let wasi_file = state.fs.stdout_mut().unwrap().as_mut().unwrap();
    let mut buf = String::new();
    wasi_file.read_to_string(&mut buf).unwrap();
    buf
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
