use std::{path::PathBuf, sync::mpsc::Sender};
use wasmer::{imports, Function, ImportObject, Store, WasmerEnv};
use wasmer_wasi::WasiEnv;

use crate::{pty_bus::PtyInstruction, SenderWithContext};

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf),
    Draw(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Input(u32, Vec<u8>),                     // plugin id, input bytes
    Unload(u32),
    Quit,
}

#[derive(WasmerEnv, Clone)]
pub struct PluginEnv {
    pub send_pty_instructions: SenderWithContext<PtyInstruction>, // FIXME: This should be a big bundle of all of the channels
    pub wasi_env: WasiEnv,
}

// Plugin API ---------------------------------------------------------------------------------------------------------

pub fn mosaic_imports(store: &Store, plugin_env: &PluginEnv) -> ImportObject {
    imports! {
        "mosaic" => {
            "host_open_file" => Function::new_native_with_env(store, plugin_env.clone(), host_open_file)
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
