use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{io, path::Path};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Backspace,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    BackTab,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Alt(char),
    Ctrl(char),
    Null,
    Esc,
}

pub fn get_key() -> Key {
    deserialize_from_stdin().unwrap()
}

pub fn open_file(path: &Path) {
    println!("{}", path.to_string_lossy());
    unsafe { host_open_file() };
}

pub fn set_selectable(selectable: bool) {
    let selectable = if selectable { 1 } else { 0 };
    unsafe { host_set_selectable(selectable) };
}

pub fn get_help() -> Vec<String> {
    unsafe { host_get_help() };
    deserialize_from_stdin().unwrap_or_default()
}

fn deserialize_from_stdin<T: DeserializeOwned>() -> Option<T> {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json).ok()
}

#[link(wasm_import_module = "mosaic")]
extern "C" {
    fn host_open_file();
    fn host_set_selectable(selectable: i32);
    fn host_get_help();
}
