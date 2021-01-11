use std::{path::PathBuf, sync::mpsc::Sender};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;

use crate::{panes::PaneId, pty_bus::PtyInstruction, screen::ScreenInstruction, SenderWithContext};

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf),
    Draw(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Input(u32, Vec<u8>),                     // plugin id, input bytes
    GlobalInput(Vec<u8>),                    // input bytes
    Unload(u32),
    Quit,
}

#[derive(WasmerEnv, Clone)]
pub struct PluginEnv {
    pub plugin_id: u32,
    pub send_screen_instructions: SenderWithContext<ScreenInstruction>,
    pub send_pty_instructions: SenderWithContext<PtyInstruction>, // FIXME: This should be a big bundle of all of the channels
    pub wasi_env: WasiEnv,
}

// Plugin API ---------------------------------------------------------------------------------------------------------

pub fn mosaic_imports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
    imports! {
        "mosaic" => {
            "host_open_file" => Function::new_native_with_env(store, plugin_env.clone(), host_open_file),
            "host_set_selectable" => Function::new_native_with_env(store, plugin_env.clone(), host_set_selectable),
        }
    }
}

// FIXME: Bundle up all of the channels! Pair that with WasiEnv?
fn host_open_file(plugin_env: &PluginEnv) {
    let path = PathBuf::from(wasi_stdout(&plugin_env.wasi_env).lines().next().unwrap());
    plugin_env
        .send_pty_instructions
        .send(PtyInstruction::SpawnTerminal(Some(path)))
        .unwrap();
}

// FIXME: Think about these naming conventions – should everything be prefixed by 'host'?
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
