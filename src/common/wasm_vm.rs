use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    process,
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;
use zellij_tile::data::{Event, EventType, PluginIds};

use super::{pty_bus::PtyInstruction, screen::ScreenInstruction, PaneId, SenderWithContext};

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf),
    Update(Option<u32>, Event), // Focused plugin / broadcast, event data
    Render(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Unload(u32),
    Exit,
}

#[derive(WasmerEnv, Clone)]
pub struct PluginEnv {
    pub plugin_id: u32,
    // FIXME: This should be a big bundle of all of the channels
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    pub send_pty_instructions: SenderWithContext<PtyInstruction>,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub wasi_env: WasiEnv,
    pub subscriptions: Arc<Mutex<HashSet<EventType>>>,
}

// Plugin API ---------------------------------------------------------------------------------------------------------

pub fn zellij_exports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
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
        host_set_max_height,
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
        .send_screen_instructions
        .send(ScreenInstruction::SetSelectable(
            PaneId::Plugin(plugin_env.plugin_id),
            selectable,
        ))
        .unwrap()
}

fn host_set_max_height(plugin_env: &PluginEnv, max_height: i32) {
    let max_height = max_height as usize;
    plugin_env
        .send_screen_instructions
        .send(ScreenInstruction::SetMaxHeight(
            PaneId::Plugin(plugin_env.plugin_id),
            max_height,
        ))
        .unwrap()
}

fn host_set_invisible_borders(plugin_env: &PluginEnv, invisible_borders: i32) {
    let invisible_borders = invisible_borders != 0;
    plugin_env
        .send_screen_instructions
        .send(ScreenInstruction::SetInvisibleBorders(
            PaneId::Plugin(plugin_env.plugin_id),
            invisible_borders,
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
        .send_pty_instructions
        .send(PtyInstruction::SpawnTerminal(Some(path)))
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
    let send_plugin_instructions = plugin_env.send_plugin_instructions.clone();
    let update_target = Some(plugin_env.plugin_id);
    thread::spawn(move || {
        let start_time = Instant::now();
        thread::sleep(Duration::from_secs_f64(secs));
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
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
