use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::mpsc::Sender,
};
use wasmer::{imports, Function, ImportObject, Store};
use wasmer_wasi::WasiEnv;

#[derive(Clone, Debug)]
pub enum PluginInstruction {
    Load(Sender<u32>, PathBuf),
    Draw(Sender<String>, u32, usize, usize), // String buffer, plugin id, rows, cols
    Unload(u32),
    Quit,
}

// Plugin API -----------------------------------------------------------------

pub fn mosaic_imports(store: &Store, wasi_env: &WasiEnv) -> ImportObject {
    imports! {
        "mosaic" => {
            "host_open_file" => Function::new_native_with_env(store, wasi_env.clone(), host_open_file)
        }
    }
}

fn host_open_file(wasi_env: &WasiEnv) {
    Command::new("xdg-open")
        .arg(format!(
            "./{}",
            wasi_stdout(wasi_env).lines().next().unwrap()
        ))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
}

// Helper Functions -----------------------------------------------------------

// FIXME: Unwrap city
pub fn wasi_stdout(wasi_env: &WasiEnv) -> String {
    let mut state = wasi_env.state();
    let wasi_file = state.fs.stdout_mut().unwrap().as_mut().unwrap();
    let mut buf = String::new();
    wasi_file.read_to_string(&mut buf).unwrap();
    buf
}

pub fn _wasi_write_string(wasi_env: &WasiEnv, buf: &str) {
    let mut state = wasi_env.state();
    let wasi_file = state.fs.stdin_mut().unwrap().as_mut().unwrap();
    writeln!(wasi_file, "{}\r", buf).unwrap();
}
