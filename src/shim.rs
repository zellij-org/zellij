use crate::keys::KeyEvent;
use std::{io, path::Path};

pub fn get_key() -> KeyEvent {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json).unwrap()
}

pub fn open_file(path: &Path) {
    println!("{}", path.to_string_lossy());
    unsafe { host_open_file() };
}

#[link(wasm_import_module = "mosaic")]
extern "C" {
    fn host_open_file();
}
