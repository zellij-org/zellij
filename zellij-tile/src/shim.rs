use serde::{de::DeserializeOwned, Serialize};
use std::{io, path::Path};
use zellij_utils::data::*;
use zellij_utils::errors::prelude::*;

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
    unsafe { host_set_selectable(selectable as i32) };
}

// Query Functions
pub fn get_plugin_ids() -> PluginIds {
    unsafe { host_get_plugin_ids() };
    object_from_stdin().unwrap()
}

pub fn get_zellij_version() -> String {
    unsafe { host_get_zellij_version() };
    object_from_stdin().unwrap()
}

// Host Functions

pub fn open_file(path: &Path) {
    object_to_stdout(&path);
    unsafe { host_open_file() };
}

pub fn open_file_floating(path: &Path) {
    object_to_stdout(&path);
    unsafe { host_open_file_floating() };
}

pub fn open_file_with_line(path: &Path, line: usize) {
    object_to_stdout(&(path, line));
    unsafe { host_open_file_with_line() };
}

pub fn open_file_with_line_floating(path: &Path, line: usize) {
    object_to_stdout(&(path, line));
    unsafe { host_open_file_with_line_floating() };
}

pub fn open_terminal(path: &Path) {
    object_to_stdout(&path);
    unsafe { host_open_terminal() };
}

pub fn open_terminal_floating(path: &Path) {
    object_to_stdout(&path);
    unsafe { host_open_terminal_floating() };
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

pub fn hide_self() {
    unsafe { host_hide_self() };
}

pub fn report_panic(info: &std::panic::PanicInfo) {
    println!("");
    println!("A panic occured in a plugin");
    println!("{:#?}", info);
    unsafe { host_report_panic() };
}

// Internal Functions

#[doc(hidden)]
pub fn object_from_stdin<T: DeserializeOwned>() -> Result<T> {
    let err_context = || "failed to deserialize object from stdin".to_string();

    let mut json = String::new();
    io::stdin().read_line(&mut json).with_context(err_context)?;
    serde_json::from_str(&json).with_context(err_context)
}

#[doc(hidden)]
pub fn object_to_stdout(object: &impl Serialize) {
    println!("{}", serde_json::to_string(object).unwrap());
}

#[doc(hidden)]
pub fn post_message_to(worker_name: &str, message: String, payload: String) {
    match serde_json::to_string(&(worker_name, message, payload)) {
        Ok(serialized) => println!("{}", serialized),
        Err(e) => eprintln!("Failed to serialize message: {:?}", e),
    }
    unsafe { host_post_message_to() };
}

#[doc(hidden)]
pub fn post_message_to_plugin(message: String, payload: String) {
    match serde_json::to_string(&(message, payload)) {
        Ok(serialized) => println!("{}", serialized),
        Err(e) => eprintln!("Failed to serialize message: {:?}", e),
    }
    unsafe { host_post_message_to_plugin() };
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_subscribe();
    fn host_unsubscribe();
    fn host_set_selectable(selectable: i32);
    fn host_get_plugin_ids();
    fn host_get_zellij_version();
    fn host_open_file();
    fn host_open_file_floating();
    fn host_open_file_with_line();
    fn host_open_file_with_line_floating();
    fn host_open_terminal();
    fn host_open_terminal_floating();
    fn host_switch_tab_to(tab_idx: u32);
    fn host_set_timeout(secs: f64);
    fn host_exec_cmd();
    fn host_report_panic();
    fn host_post_message_to();
    fn host_post_message_to_plugin();
    fn host_hide_self();
}
