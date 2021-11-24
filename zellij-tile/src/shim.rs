use serde::{de::DeserializeOwned, Serialize};
use std::{io, path::Path};

use crate::data::*;

// Subscription Handling

pub fn subscribe(event_types: &[EventType]) {
    object_to_stdout(&event_types);
    unsafe { host_subscribe() };
}

pub fn unsubscribe(event_types: &[EventType]) {
    object_to_stdout(&event_types);
    unsafe { host_unsubscribe() };
}

// Plugin Settings

pub fn set_selectable(selectable: bool) {
    unsafe { host_set_selectable(if selectable { 1 } else { 0 }) };
}

// Query Functions
pub fn get_plugin_ids() -> PluginIds {
    unsafe { host_get_plugin_ids() };
    object_from_stdin().unwrap()
}

pub fn get_zellij_version() -> String {
    unsafe { host_get_zellij_version() };
    string_from_stdin()
}

// Host Functions

pub fn open_file(path: &Path) {
    object_to_stdout(&path);
    unsafe { host_open_file() };
}

pub fn switch_tab_to(tab_idx: u32) {
    unsafe { host_switch_tab_to(tab_idx) };
}

pub fn set_timeout(secs: f64) {
    unsafe { host_set_timeout(secs) };
}
pub fn exec_cmd(cmd: &[&str]) {
    object_to_stdout(&cmd);
    unsafe { host_exec_cmd() };
}

// Internal Functions

#[doc(hidden)]
pub fn string_from_stdin() -> String {
    let mut buffer = String::new();
    io::stdin().read_line(&mut buffer).unwrap();
    buffer.trim().to_string()
}

#[doc(hidden)]
pub fn object_from_stdin<T: DeserializeOwned>() -> Result<T, serde_json::Error> {
    let mut json = String::new();
    io::stdin().read_line(&mut json).unwrap();
    serde_json::from_str(&json)
}

#[doc(hidden)]
pub fn object_to_stdout(object: &impl Serialize) {
    println!("{}", serde_json::to_string(object).unwrap());
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_subscribe();
    fn host_unsubscribe();
    fn host_set_selectable(selectable: i32);
    fn host_get_plugin_ids();
    fn host_get_zellij_version();
    fn host_open_file();
    fn host_switch_tab_to(tab_idx: u32);
    fn host_set_timeout(secs: f64);
    fn host_exec_cmd();
}
