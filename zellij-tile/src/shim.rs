use crate::data::*;
use serde::de::DeserializeOwned;
use std::{io, path::Path};

pub fn get_key() -> Key {
    deserialize_from_stdin().unwrap()
}

pub fn request_rerender() {
    unsafe { host_request_rerender() };
}

pub fn open_file(path: &Path) {
    println!("{}", path.to_string_lossy());
    unsafe { host_open_file() };
}

pub fn set_max_height(max_height: i32) {
    unsafe { host_set_max_height(max_height) };
}

pub fn set_invisible_borders(invisible_borders: bool) {
    let invisible_borders = if invisible_borders { 1 } else { 0 };
    unsafe { host_set_invisible_borders(invisible_borders) };
}

pub fn set_selectable(selectable: bool) {
    let selectable = if selectable { 1 } else { 0 };
    unsafe { host_set_selectable(selectable) };
}

pub fn get_help() -> Help {
    unsafe { host_get_help() };
    deserialize_from_stdin().unwrap_or_default()
}

pub fn get_tab_info() -> Vec<TabInfo> {
    unsafe { host_get_tab_info() };
    deserialize_from_stdin().unwrap_or_default()
}

fn deserialize_from_stdin<T: DeserializeOwned>() -> Option<T> {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json).ok()
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_request_rerender();
    fn host_open_file();
    fn host_set_max_height(max_height: i32);
    fn host_set_selectable(selectable: i32);
    fn host_set_invisible_borders(invisible_borders: i32);
    fn host_get_help();
    fn host_get_tab_info();
}
