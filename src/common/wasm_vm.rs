use serde::Serialize;
use std::{
    collections::HashSet,
    path::PathBuf,
    process,
    sync::{mpsc::Sender, Arc, Mutex},
};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;
use zellij_tile::data::{Event, EventType, PluginIds};

use super::{
    pty_bus::PtyInstruction, screen::ScreenInstruction, AppInstruction, PaneId, SenderWithContext,
};

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf),
    Update(Option<u32>, Event), // Focused plugin / broadcast, event data
    Render(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Unload(u32),
    Quit,
}

#[derive(WasmerEnv, Clone)]
pub struct PluginEnv {
    pub plugin_id: u32,
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    pub send_app_instructions: SenderWithContext<AppInstruction>,
    pub send_pty_instructions: SenderWithContext<PtyInstruction>, // FIXME: This should be a big bundle of all of the channels
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
    }
}

fn host_subscribe(plugin_env: &PluginEnv) {
    let mut subscriptions = plugin_env.subscriptions.lock().unwrap();
    let new: HashSet<EventType> = serde_json::from_str(&wasi_stdout(&plugin_env.wasi_env)).unwrap();
    subscriptions.extend(new);
}

fn host_unsubscribe(plugin_env: &PluginEnv) {
    let mut subscriptions = plugin_env.subscriptions.lock().unwrap();
    let old: HashSet<EventType> = serde_json::from_str(&wasi_stdout(&plugin_env.wasi_env)).unwrap();
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
    wasi_write_json(&plugin_env.wasi_env, &ids);
}

fn host_open_file(plugin_env: &PluginEnv) {
    let path = PathBuf::from(wasi_stdout(&plugin_env.wasi_env).lines().next().unwrap());
    plugin_env
        .send_pty_instructions
        .send(PtyInstruction::SpawnTerminal(Some(path)))
        .unwrap();
}

// Helper Functions ---------------------------------------------------------------------------------------------------

// FIXME: Unwrap city
pub fn wasi_stdout(wasi_env: &WasiEnv) -> String {
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

pub fn wasi_write_json(wasi_env: &WasiEnv, object: &impl Serialize) {
    wasi_write_string(wasi_env, &serde_json::to_string(&object).unwrap());
}
