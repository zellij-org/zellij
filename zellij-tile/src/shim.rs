use serde::de::DeserializeOwned;
use std::{io, path::Path};

use crate::data::*;

pub fn get_key() -> Key {
    deserialize_from_stdin().unwrap()
}

pub fn subscribe(event_types: &[EventType]) {
    println!("{}", serde_json::to_string(event_types).unwrap());
    unsafe { host_subscribe() };
}

pub fn unsubscribe(event_types: &[EventType]) {
    println!("{}", serde_json::to_string(event_types).unwrap());
    unsafe { host_unsubscribe() };
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

pub fn get_tabs() -> Vec<TabInfo> {
    deserialize_from_stdin().unwrap_or_default()
}

fn deserialize_from_stdin<T: DeserializeOwned>() -> Option<T> {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json).ok()
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_subscribe();
    fn host_unsubscribe();
    fn host_open_file();
    fn host_set_max_height(max_height: i32);
    fn host_set_selectable(selectable: i32);
    fn host_set_invisible_borders(invisible_borders: i32);
    fn host_get_help();
}
