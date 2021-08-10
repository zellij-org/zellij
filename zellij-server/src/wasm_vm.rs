use log::info;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::sync::{mpsc::Sender, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Serialize};
use wasmer::{
    imports, ChainableNamedResolver, Function, ImportObject, Instance, Module, Store, Value,
    WasmerEnv,
};
use wasmer_wasi::{Pipe, WasiEnv, WasiState};
use zellij_tile::data::{Event, EventType, PluginIds};

use crate::{
    logging_pipe::LoggingPipe,
    panes::PaneId,
    pty::PtyInstruction,
    screen::ScreenInstruction,
    thread_bus::{Bus, ThreadSenders},
};
use zellij_utils::errors::{ContextType, PluginContext};
use zellij_utils::{input::command::TerminalAction, serde, zellij_tile};

#[derive(Clone, Debug)]
pub(crate) enum PluginInstruction {
    Load(Sender<u32>, PathBuf, usize), // tx_pid, path_of_plugin , tab_index
    Update(Option<u32>, Event),        // Focused plugin / broadcast, event data
    Render(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Unload(u32),
    Exit,
}

impl From<&PluginInstruction> for PluginContext {
    fn from(plugin_instruction: &PluginInstruction) -> Self {
        match *plugin_instruction {
            PluginInstruction::Load(..) => PluginContext::Load,
            PluginInstruction::Update(..) => PluginContext::Update,
            PluginInstruction::Render(..) => PluginContext::Render,
            PluginInstruction::Unload(_) => PluginContext::Unload,
            PluginInstruction::Exit => PluginContext::Exit,
        }
    }
}

#[derive(WasmerEnv, Clone)]
pub(crate) struct PluginEnv {
    pub plugin_id: u32,
    pub tab_index: usize,
    pub senders: ThreadSenders,
    pub wasi_env: WasiEnv,
    pub subscriptions: Arc<Mutex<HashSet<EventType>>>,
}

// Thread main --------------------------------------------------------------------------------------------------------
pub(crate) fn wasm_thread_main(bus: Bus<PluginInstruction>, store: Store, data_dir: PathBuf) {
    info!("Wasm main thread starts");
    let mut plugin_id = 0;
    let mut plugin_map = HashMap::new();
    loop {
        let (event, mut err_ctx) = bus.recv().expect("failed to receive event on channel");
        err_ctx.add_call(ContextType::Plugin((&event).into()));
        match event {
            PluginInstruction::Load(pid_tx, path, tab_index) => {
                let plugin_dir = data_dir.join("plugins/");
                let wasm_bytes = fs::read(&path)
                    .or_else(|_| fs::read(&path.with_extension("wasm")))
                    .or_else(|_| fs::read(&plugin_dir.join(&path).with_extension("wasm")))
                    .unwrap_or_else(|_| panic!("cannot find plugin {}", &path.display()));

                // FIXME: Cache this compiled module on disk. I could use `(de)serialize_to_file()` for that
                let module = Module::new(&store, &wasm_bytes).unwrap();

                let output = Pipe::new();
                let input = Pipe::new();
                let stderr = LoggingPipe::new(
                    path.as_path().file_name().unwrap().to_str().unwrap(),
                    plugin_id,
                );
                let mut wasi_env = WasiState::new("Zellij")
                    .env("CLICOLOR_FORCE", "1")
                    .preopen(|p| {
                        p.directory(".") // FIXME: Change this to a more meaningful dir
                            .alias(".")
                            .read(true)
                            .write(true)
                            .create(true)
                    })
                    .unwrap()
                    .stdin(Box::new(input))
                    .stdout(Box::new(output))
                    .stderr(Box::new(stderr))
                    .finalize()
                    .unwrap();

                let wasi = wasi_env.import_object(&module).unwrap();

                let plugin_env = PluginEnv {
                    plugin_id,
                    tab_index,
                    senders: bus.senders.clone(),
                    wasi_env,
                    subscriptions: Arc::new(Mutex::new(HashSet::new())),
                };

                let zellij = zellij_exports(&store, &plugin_env);
                let instance = Instance::new(&module, &zellij.chain_back(wasi)).unwrap();

                let start = instance.exports.get_function("_start").unwrap();

                // This eventually calls the `.load()` method
                start.call(&[]).unwrap();

                plugin_map.insert(plugin_id, (instance, plugin_env));
                pid_tx.send(plugin_id).unwrap();
                plugin_id += 1;
            }
            PluginInstruction::Update(pid, event) => {
                for (&i, (instance, plugin_env)) in &plugin_map {
                    let subs = plugin_env.subscriptions.lock().unwrap();
                    // FIXME: This is very janky... Maybe I should write my own macro for Event -> EventType?
                    let event_type = EventType::from_str(&event.to_string()).unwrap();
                    if (pid.is_none() || pid == Some(i)) && subs.contains(&event_type) {
                        let update = instance.exports.get_function("update").unwrap();
                        wasi_write_object(&plugin_env.wasi_env, &event);
                        update.call(&[]).unwrap();
                    }
                }
                drop(bus.senders.send_to_screen(ScreenInstruction::Render));
            }
            PluginInstruction::Render(buf_tx, pid, rows, cols) => {
                let (instance, plugin_env) = plugin_map.get(&pid).unwrap();

                let render = instance.exports.get_function("render").unwrap();

                render
                    .call(&[Value::I32(rows as i32), Value::I32(cols as i32)])
                    .unwrap();

                buf_tx.send(wasi_read_string(&plugin_env.wasi_env)).unwrap();
            }
            PluginInstruction::Unload(pid) => drop(plugin_map.remove(&pid)),
            PluginInstruction::Exit => break,
        }
    }
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
        host_set_invisible_borders,
        host_set_selectable,
        host_get_plugin_ids,
        host_open_file,
        host_set_timeout,
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
    let selectable = selectable != 0;
    plugin_env
        .senders
        .send_to_screen(ScreenInstruction::SetSelectable(
            PaneId::Plugin(plugin_env.plugin_id),
            selectable,
            plugin_env.tab_index,
        ))
        .unwrap()
}

// FIXME: This should be deleted, like the `set_fixed` functions have been
fn host_set_invisible_borders(plugin_env: &PluginEnv, invisible_borders: i32) {
    let invisible_borders = invisible_borders != 0;
    plugin_env
        .senders
        .send_to_screen(ScreenInstruction::SetInvisibleBorders(
            PaneId::Plugin(plugin_env.plugin_id),
            invisible_borders,
            plugin_env.tab_index,
        ))
        .unwrap()
}

fn host_get_plugin_ids(plugin_env: &PluginEnv) {
    let ids = PluginIds {
        plugin_id: plugin_env.plugin_id,
        zellij_pid: process::id(),
    };
    wasi_write_object(&plugin_env.wasi_env, &ids);
}

fn host_open_file(plugin_env: &PluginEnv) {
    let path: PathBuf = wasi_read_object(&plugin_env.wasi_env);
    plugin_env
        .senders
        .send_to_pty(PtyInstruction::SpawnTerminal(Some(
            TerminalAction::OpenFile(path),
        )))
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
                Event::Timer(elapsed_time),
            ))
            .unwrap();
    });
}

// Helper Functions ---------------------------------------------------------------------------------------------------

// FIXME: Unwrap city
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

pub fn wasi_write_object(wasi_env: &WasiEnv, object: &impl Serialize) {
    wasi_write_string(wasi_env, &serde_json::to_string(&object).unwrap());
}

pub fn wasi_read_object<T: DeserializeOwned>(wasi_env: &WasiEnv) -> T {
    let json = wasi_read_string(wasi_env);
    serde_json::from_str(&json).unwrap()
}
