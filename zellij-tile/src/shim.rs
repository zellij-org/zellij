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

pub fn open_file<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_file() };
}

pub fn open_file_floating<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_file_floating() };
}

pub fn open_file_with_line<P: AsRef<Path>>(path: P, line: usize) {
    object_to_stdout(&(path.as_ref(), line));
    unsafe { host_open_file_with_line() };
}

pub fn open_file_with_line_floating<P: AsRef<Path>>(path: P, line: usize) {
    object_to_stdout(&(path.as_ref(), line));
    unsafe { host_open_file_with_line_floating() };
}

pub fn open_terminal<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_terminal() };
}

pub fn open_terminal_floating<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_terminal_floating() };
}

pub fn open_command_pane<P: AsRef<Path>, A: AsRef<str>>(path: P, args: Vec<A>) {
    object_to_stdout(&(path.as_ref(), args.iter().map(|a| a.as_ref()).collect::<Vec<&str>>()));
    unsafe { host_open_command_pane() };
}

pub fn open_command_pane_floating<P: AsRef<Path>, A: AsRef<str>>(path: P, args: Vec<A>) {
    object_to_stdout(&(path.as_ref(), args.iter().map(|a| a.as_ref()).collect::<Vec<&str>>()));
    unsafe { host_open_command_pane_floating() };
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

pub fn show_self(should_float_if_hidden: bool) {
    unsafe { host_show_self(should_float_if_hidden as i32) };
}

pub fn switch_to_input_mode(mode: &InputMode) {
    object_to_stdout(&mode);
    unsafe { host_switch_to_mode() };
}

pub fn new_tabs_with_layout(layout: &str) {
    println!("{}", layout);
    unsafe { host_new_tabs_with_layout() }
}

pub fn new_tab() {
    unsafe { host_new_tab() }
}

pub fn go_to_next_tab() {
    unsafe { host_go_to_next_tab() }
}

pub fn go_to_previous_tab() {
    unsafe { host_go_to_previous_tab() }
}

pub fn report_panic(info: &std::panic::PanicInfo) {
    println!("");
    println!("A panic occured in a plugin");
    println!("{:#?}", info);
    unsafe { host_report_panic() };
}

pub fn resize_focused_pane(resize: Resize) {
    object_to_stdout(&resize);
    unsafe { host_resize() };
}

pub fn resize_focused_pane_with_direction(resize: Resize, direction: Direction) {
    object_to_stdout(&(resize, direction));
    unsafe { host_resize_with_direction() };
}

pub fn focus_next_pane() {
    unsafe { host_focus_next_pane() };
}

pub fn focus_previous_pane() {
    unsafe { host_focus_previous_pane() };
}

pub fn move_focus(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_focus() };
}

pub fn move_focus_or_tab(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_focus_or_tab() };
}

pub fn detach() {
    unsafe { host_detach() };
}

pub fn edit_scrollback() {
    unsafe { host_edit_scrollback() };
}

pub fn write(bytes: Vec<u8>) {
    object_to_stdout(&bytes);
    unsafe { host_write() };
}

pub fn write_chars(chars: &str) {
    println!("{}", chars);
    unsafe { host_write_chars() };
}

pub fn toggle_tab() {
    unsafe { host_toggle_tab() };
}

pub fn move_pane() {
    unsafe { host_move_pane() };
}

pub fn move_pane_with_direction(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_pane_with_direction() };
}

pub fn clear_screen() {
    unsafe { host_clear_screen() };
}

pub fn scroll_up() {
    unsafe { host_scroll_up() };
}

pub fn scroll_down() {
    unsafe { host_scroll_down() };
}

pub fn scroll_to_top() {
    unsafe { host_scroll_to_top() };
}

pub fn scroll_to_bottom() {
    unsafe { host_scroll_to_bottom() };
}

pub fn page_scroll_up() {
    unsafe { host_page_scroll_up() };
}

pub fn page_scroll_down() {
    unsafe { host_page_scroll_down() };
}

pub fn toggle_focus_fullscreen() {
    unsafe { host_toggle_focus_fullscreen() };
}

pub fn toggle_pane_frames() {
    unsafe { host_toggle_pane_frames() };
}

pub fn toggle_pane_embed_or_eject() {
    unsafe { host_toggle_pane_embed_or_eject() };
}

pub fn undo_rename_pane() {
    unsafe { host_undo_rename_pane() };
}

pub fn close_focus() {
    unsafe { host_close_focus() };
}

pub fn toggle_active_tab_sync() {
    unsafe { host_toggle_active_tab_sync() };
}

pub fn close_focused_tab() {
    unsafe { host_close_focused_tab() };
}

pub fn undo_rename_tab() {
    unsafe { host_undo_rename_tab() };
}

pub fn quit_zellij() {
    unsafe { host_quit_zellij() };
}

pub fn previous_swap_layout() {
    unsafe { host_previous_swap_layout() };
}

pub fn next_swap_layout() {
    unsafe { host_next_swap_layout() };
}

pub fn go_to_tab_name(tab_name: &str) {
    println!("{}", tab_name);
    unsafe { host_go_to_tab_name() };
}

pub fn focus_or_create_tab(tab_name: &str) {
    print!("{}", tab_name);
    unsafe { host_focus_or_create_tab() };
}

pub fn go_to_tab(tab_index: i32) {
    unsafe { host_go_to_tab(tab_index) };
}

pub fn start_or_reload_plugin(url: &str) {
    println!("{}", url);
    unsafe { host_start_or_reload_plugin() };
}

pub fn close_terminal_pane(terminal_pane_id: i32) {
    unsafe { host_close_terminal_pane(terminal_pane_id) };
}

pub fn close_plugin_pane(plugin_pane_id: i32) {
    unsafe { host_close_plugin_pane(plugin_pane_id) };
}

pub fn focus_terminal_pane(terminal_pane_id: i32, should_float_if_hidden: bool) {
    unsafe { host_focus_terminal_pane(terminal_pane_id, should_float_if_hidden as i32) };
}

pub fn focus_plugin_pane(plugin_pane_id: i32, should_float_if_hidden: bool) {
    unsafe { host_focus_plugin_pane(plugin_pane_id, should_float_if_hidden as i32) };
}

pub fn rename_terminal_pane<S: AsRef<str>>(terminal_pane_id: i32, new_name: S) {
    object_to_stdout(&(terminal_pane_id, new_name.as_ref()));
    unsafe { host_rename_terminal_pane() };
}

pub fn rename_plugin_pane<S: AsRef<str>>(plugin_pane_id: i32, new_name: S) {
    object_to_stdout(&(plugin_pane_id, new_name.as_ref()));
    unsafe { host_rename_plugin_pane() };
}

pub fn rename_tab<S: AsRef<str>>(tab_position: i32, new_name: S) {
    object_to_stdout(&(tab_position, new_name.as_ref()));
    unsafe { host_rename_tab() };
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
    // TODO: no crashy
    println!("{}", serde_json::to_string(object).unwrap());
}

#[doc(hidden)]
pub fn post_message_to<S: AsRef<str>>(worker_name: S, message: S, payload: S) {
    match serde_json::to_string(&(worker_name.as_ref(), message.as_ref(), payload.as_ref())) {
        Ok(serialized) => println!("{}", serialized),
        Err(e) => eprintln!("Failed to serialize message: {:?}", e),
    }
    unsafe { host_post_message_to() };
}

#[doc(hidden)]
pub fn post_message_to_plugin<S: AsRef<str>>(message: S, payload: S) {
    match serde_json::to_string(&(message.as_ref(), payload.as_ref())) {
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
    fn host_open_command_pane();
    fn host_open_command_pane_floating();
    fn host_switch_tab_to(tab_idx: u32);
    fn host_set_timeout(secs: f64);
    fn host_exec_cmd();
    fn host_report_panic();
    fn host_post_message_to();
    fn host_post_message_to_plugin();
    fn host_hide_self();
    fn host_show_self(should_float_if_hidden: i32);
    fn host_switch_to_mode();
    fn host_new_tabs_with_layout();
    fn host_new_tab();
    fn host_go_to_next_tab();
    fn host_go_to_previous_tab();
    fn host_resize();
    fn host_resize_with_direction();
    fn host_focus_next_pane();
    fn host_focus_previous_pane();
    fn host_move_focus();
    fn host_move_focus_or_tab();
    fn host_detach();
    fn host_edit_scrollback();
    fn host_write();
    fn host_write_chars();
    fn host_toggle_tab();
    fn host_move_pane();
    fn host_move_pane_with_direction();
    fn host_clear_screen();
    fn host_scroll_up();
    fn host_scroll_down();
    fn host_scroll_to_top();
    fn host_scroll_to_bottom();
    fn host_page_scroll_up();
    fn host_page_scroll_down();
    fn host_toggle_focus_fullscreen();
    fn host_toggle_pane_frames();
    fn host_toggle_pane_embed_or_eject();
    fn host_undo_rename_pane();
    fn host_close_focus();
    fn host_toggle_active_tab_sync();
    fn host_close_focused_tab();
    fn host_undo_rename_tab();
    fn host_quit_zellij();
    fn host_previous_swap_layout();
    fn host_next_swap_layout();
    fn host_go_to_tab_name();
    fn host_focus_or_create_tab();
    fn host_go_to_tab(tab_index: i32);
    fn host_start_or_reload_plugin();
    fn host_close_terminal_pane(terminal_pane: i32);
    fn host_close_plugin_pane(plugin_pane: i32);
    fn host_focus_terminal_pane(terminal_pane: i32, should_float_if_hidden: i32);
    fn host_focus_plugin_pane(plugin_pane: i32, should_float_if_hidden: i32);
    fn host_rename_terminal_pane();
    fn host_rename_plugin_pane();
    fn host_rename_tab();
}
