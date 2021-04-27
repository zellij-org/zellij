use serde::de::DeserializeOwned;
use std::{io, path::Path};

use crate::data::*;

// Subscription Handling

pub fn subscribe(event_types: &[EventType]) {
    println!("{}", serde_json::to_string(event_types).unwrap());
    unsafe { host_subscribe() };
}

pub fn unsubscribe(event_types: &[EventType]) {
    println!("{}", serde_json::to_string(event_types).unwrap());
    unsafe { host_unsubscribe() };
}

// Plugin Settings

pub fn set_max_height(max_height: i32) {
    unsafe { host_set_max_height(max_height) };
}

pub fn set_selectable(selectable: bool) {
    unsafe { host_set_selectable(if selectable { 1 } else { 0 }) };
}

pub fn set_invisible_borders(invisible_borders: bool) {
    unsafe { host_set_invisible_borders(if invisible_borders { 1 } else { 0 }) };
}

// Query Functions
pub fn get_plugin_ids() -> PluginIds {
    unsafe { host_get_plugin_ids() };
    object_from_stdin()
}

// Host Functions

pub fn open_file(path: &Path) {
    println!("{}", path.to_string_lossy());
    unsafe { host_open_file() };
}

// Internal Functions

#[doc(hidden)]
pub fn object_from_stdin<T: DeserializeOwned>() -> T {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json).unwrap()
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_subscribe();
    fn host_unsubscribe();
    fn host_set_max_height(max_height: i32);
    fn host_set_selectable(selectable: i32);
    fn host_set_invisible_borders(invisible_borders: i32);
    fn host_get_plugin_ids();
    fn host_open_file();
}
