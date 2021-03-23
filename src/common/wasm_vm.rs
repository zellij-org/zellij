use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        mpsc::{channel, Sender},
        Arc, Mutex,
    },
};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;
use zellij_tile::data::{Event, EventType, TabInfo};

use super::{
    input::handler::get_help, pty_bus::PtyInstruction, screen::ScreenInstruction, AppInstruction,
    PaneId, SenderWithContext,
};

#[derive(Clone, Debug, PartialEq, Hash, Eq, Serialize, Deserialize)]
pub enum NaughtyEventType {
    Tab,
}

#[derive(Clone, Debug)]
pub enum PluginInputType {
    Normal(u32),
    Event(NaughtyEventType),
}

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf, Vec<NaughtyEventType>),
    Update(Option<u32>, Event), // Focused plugin / broadcast, event data
    Render(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Input(PluginInputType, Vec<u8>), // plugin id, input bytes
    Unload(u32),
    UpdateTabs(Vec<TabInfo>), // num tabs, active tab
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
    pub events: Vec<NaughtyEventType>, // FIXME: Murder this very soon (should not survive into main)
}

// Plugin API ---------------------------------------------------------------------------------------------------------

pub fn zellij_imports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
    imports! {
        "zellij" => {
            "host_subscribe" => Function::new_native_with_env(store, plugin_env.clone(), host_subscribe),
            "host_unsubscribe" => Function::new_native_with_env(store, plugin_env.clone(), host_unsubscribe),
            "host_open_file" => Function::new_native_with_env(store, plugin_env.clone(), host_open_file),
            "host_set_invisible_borders" => Function::new_native_with_env(store, plugin_env.clone(), host_set_invisible_borders),
            "host_set_max_height" => Function::new_native_with_env(store, plugin_env.clone(), host_set_max_height),
            "host_set_selectable" => Function::new_native_with_env(store, plugin_env.clone(), host_set_selectable),
            "host_get_help" => Function::new_native_with_env(store, plugin_env.clone(), host_get_help),
        }
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

fn host_open_file(plugin_env: &PluginEnv) {
    let path = PathBuf::from(wasi_stdout(&plugin_env.wasi_env).lines().next().unwrap());
    plugin_env
        .send_pty_instructions
        .send(PtyInstruction::SpawnTerminal(Some(path)))
        .unwrap();
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

fn host_get_help(plugin_env: &PluginEnv) {
    let (state_tx, state_rx) = channel();
    // FIXME: If I changed the application so that threads were sent the termination
    // signal and joined one at a time, there would be an order to shutdown, so I
    // could get rid of this .is_ok() check and the .try_send()
    if plugin_env
        .send_app_instructions
        .try_send(AppInstruction::GetState(state_tx))
        .is_ok()
    {
        let help = get_help(state_rx.recv().unwrap().input_mode);
        wasi_write_string(&plugin_env.wasi_env, &serde_json::to_string(&help).unwrap());
    }
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
