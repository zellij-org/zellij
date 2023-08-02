use serde::{de::DeserializeOwned, Serialize};
use std::{io, path::Path};
use zellij_utils::data::*;
use zellij_utils::errors::prelude::*;

// Subscription Handling

/// Subscribe to a list of [`Event`]s represented by their [`EventType`]s that will then trigger the `update` method
pub fn subscribe(event_types: &[EventType]) {
    object_to_stdout(&event_types);
    unsafe { host_subscribe() };
}

/// Unsubscribe to a list of [`Event`]s represented by their [`EventType`]s.
pub fn unsubscribe(event_types: &[EventType]) {
    object_to_stdout(&event_types);
    unsafe { host_unsubscribe() };
}

// Plugin Settings

/// Sets the plugin as selectable or unselectable to the user. Unselectable plugins might be desired when they do not accept user input.
pub fn set_selectable(selectable: bool) {
    unsafe { host_set_selectable(selectable as i32) };
}

// Query Functions
/// Returns the unique Zellij pane ID for the plugin as well as the Zellij process id.
pub fn get_plugin_ids() -> PluginIds {
    unsafe { host_get_plugin_ids() };
    object_from_stdin().unwrap()
}

/// Returns the version of the running Zellij instance - can be useful to check plugin compatibility
pub fn get_zellij_version() -> String {
    unsafe { host_get_zellij_version() };
    object_from_stdin().unwrap()
}

// Host Functions

/// Open a file in the user's default `$EDITOR` in a new pane
pub fn open_file<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_file() };
}

/// Open a file in the user's default `$EDITOR` in a new floating pane
pub fn open_file_floating<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_file_floating() };
}

/// Open a file to a specific line in the user's default `$EDITOR` (if it supports it, most do) in a new pane
pub fn open_file_with_line<P: AsRef<Path>>(path: P, line: usize) {
    object_to_stdout(&(path.as_ref(), line));
    unsafe { host_open_file_with_line() };
}

/// Open a file to a specific line in the user's default `$EDITOR` (if it supports it, most do) in a new floating pane
pub fn open_file_with_line_floating<P: AsRef<Path>>(path: P, line: usize) {
    object_to_stdout(&(path.as_ref(), line));
    unsafe { host_open_file_with_line_floating() };
}

/// Open a new terminal pane to the specified location on the host filesystem
pub fn open_terminal<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_terminal() };
}

/// Open a new floating terminal pane to the specified location on the host filesystem
pub fn open_terminal_floating<P: AsRef<Path>>(path: P) {
    object_to_stdout(&path.as_ref());
    unsafe { host_open_terminal_floating() };
}

/// Open a new command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane<P: AsRef<Path>, A: AsRef<str>>(path: P, args: Vec<A>) {
    object_to_stdout(&(
        path.as_ref(),
        args.iter().map(|a| a.as_ref()).collect::<Vec<&str>>(),
    ));
    unsafe { host_open_command_pane() };
}

/// Open a new floating command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane_floating<P: AsRef<Path>, A: AsRef<str>>(path: P, args: Vec<A>) {
    object_to_stdout(&(
        path.as_ref(),
        args.iter().map(|a| a.as_ref()).collect::<Vec<&str>>(),
    ));
    unsafe { host_open_command_pane_floating() };
}

/// Change the focused tab to the specified index (corresponding with the default tab names, to starting at `1`, `0` will be considered as `1`).
pub fn switch_tab_to(tab_idx: u32) {
    unsafe { host_switch_tab_to(tab_idx) };
}

/// Set a timeout in seconds (or fractions thereof) after which the plugins [update](./plugin-api-events#update) method will be called with the [`Timer`](./plugin-api-events.md#timer) event.
pub fn set_timeout(secs: f64) {
    unsafe { host_set_timeout(secs) };
}

#[doc(hidden)]
pub fn exec_cmd(cmd: &[&str]) {
    object_to_stdout(&cmd);
    unsafe { host_exec_cmd() };
}

/// Hide the plugin pane (suppress it) from the UI
pub fn hide_self() {
    unsafe { host_hide_self() };
}

/// Show the plugin pane (unsuppress it if it is suppressed), focus it and switch to its tab
pub fn show_self(should_float_if_hidden: bool) {
    unsafe { host_show_self(should_float_if_hidden as i32) };
}

/// Switch to the specified Input Mode (eg. `Normal`, `Tab`, `Pane`)
pub fn switch_to_input_mode(mode: &InputMode) {
    object_to_stdout(&mode);
    unsafe { host_switch_to_mode() };
}

/// Provide a stringified [`layout`](https://zellij.dev/documentation/layouts.html) to be applied to the current session. If the layout has multiple tabs, they will all be opened.
pub fn new_tabs_with_layout(layout: &str) {
    println!("{}", layout);
    unsafe { host_new_tabs_with_layout() }
}

/// Open a new tab with the default layout
pub fn new_tab() {
    unsafe { host_new_tab() }
}

/// Change focus to the next tab or loop back to the first
pub fn go_to_next_tab() {
    unsafe { host_go_to_next_tab() }
}

/// Change focus to the previous tab or loop back to the last
pub fn go_to_previous_tab() {
    unsafe { host_go_to_previous_tab() }
}

pub fn report_panic(info: &std::panic::PanicInfo) {
    println!("");
    println!("A panic occured in a plugin");
    println!("{:#?}", info);
    unsafe { host_report_panic() };
}

/// Either Increase or Decrease the size of the focused pane
pub fn resize_focused_pane(resize: Resize) {
    object_to_stdout(&resize);
    unsafe { host_resize() };
}

/// Either Increase or Decrease the size of the focused pane in a specified direction (eg. `Left`, `Right`, `Up`, `Down`).
pub fn resize_focused_pane_with_direction(resize: Resize, direction: Direction) {
    object_to_stdout(&(resize, direction));
    unsafe { host_resize_with_direction() };
}

/// Change focus tot he next pane in chronological order
pub fn focus_next_pane() {
    unsafe { host_focus_next_pane() };
}

/// Change focus to the previous pane in chronological order
pub fn focus_previous_pane() {
    unsafe { host_focus_previous_pane() };
}

/// Change the focused pane in the specified direction
pub fn move_focus(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_focus() };
}

/// Change the focused pane in the specified direction, if the pane is on the edge of the screen, the next tab is focused (next if right edge, previous if left edge).
pub fn move_focus_or_tab(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_focus_or_tab() };
}

/// Detach the user from the active session
pub fn detach() {
    unsafe { host_detach() };
}

/// Edit the scrollback of the focused pane in the user's default `$EDITOR`
pub fn edit_scrollback() {
    unsafe { host_edit_scrollback() };
}

/// Write bytes to the `STDIN` of the focused pane
pub fn write(bytes: Vec<u8>) {
    object_to_stdout(&bytes);
    unsafe { host_write() };
}

/// Write characters to the `STDIN` of the focused pane
pub fn write_chars(chars: &str) {
    println!("{}", chars);
    unsafe { host_write_chars() };
}

/// Focused the previously focused tab (regardless of the tab position)
pub fn toggle_tab() {
    unsafe { host_toggle_tab() };
}

/// Switch the position of the focused pane with a different pane
pub fn move_pane() {
    unsafe { host_move_pane() };
}

/// Switch the position of the focused pane with a different pane in the specified direction (eg. `Down`, `Up`, `Left`, `Right`).
pub fn move_pane_with_direction(direction: Direction) {
    object_to_stdout(&direction);
    unsafe { host_move_pane_with_direction() };
}

/// Clear the scroll buffer of the focused pane
pub fn clear_screen() {
    unsafe { host_clear_screen() };
}

/// Scroll the focused pane up 1 line
pub fn scroll_up() {
    unsafe { host_scroll_up() };
}

/// Scroll the focused pane down 1 line
pub fn scroll_down() {
    unsafe { host_scroll_down() };
}

/// Scroll the focused pane all the way to the top of the scrollbuffer
pub fn scroll_to_top() {
    unsafe { host_scroll_to_top() };
}

/// Scroll the focused pane all the way to the bottom of the scrollbuffer
pub fn scroll_to_bottom() {
    unsafe { host_scroll_to_bottom() };
}

/// Scroll the focused pane up one page
pub fn page_scroll_up() {
    unsafe { host_page_scroll_up() };
}

/// Scroll the focused pane down one page
pub fn page_scroll_down() {
    unsafe { host_page_scroll_down() };
}

/// Toggle the focused pane to be fullscreen or normal sized
pub fn toggle_focus_fullscreen() {
    unsafe { host_toggle_focus_fullscreen() };
}

/// Toggle the UI pane frames on or off
pub fn toggle_pane_frames() {
    unsafe { host_toggle_pane_frames() };
}

/// Embed the currently focused pane (make it stop floating) or turn it to a float pane if it is not
pub fn toggle_pane_embed_or_eject() {
    unsafe { host_toggle_pane_embed_or_eject() };
}

pub fn undo_rename_pane() {
    unsafe { host_undo_rename_pane() };
}

/// Close the focused pane
pub fn close_focus() {
    unsafe { host_close_focus() };
}

/// Turn the `STDIN` synchronization of the current tab on or off
pub fn toggle_active_tab_sync() {
    unsafe { host_toggle_active_tab_sync() };
}

/// Close the focused tab
pub fn close_focused_tab() {
    unsafe { host_close_focused_tab() };
}

pub fn undo_rename_tab() {
    unsafe { host_undo_rename_tab() };
}

/// Compeltely quit Zellij for this and all other connected clients
pub fn quit_zellij() {
    unsafe { host_quit_zellij() };
}

/// Change to the previous [swap layout](https://zellij.dev/documentation/swap-layouts.html)
pub fn previous_swap_layout() {
    unsafe { host_previous_swap_layout() };
}

/// Change to the next [swap layout](https://zellij.dev/documentation/swap-layouts.html)
pub fn next_swap_layout() {
    unsafe { host_next_swap_layout() };
}

/// Change focus to the tab with the specified name
pub fn go_to_tab_name(tab_name: &str) {
    println!("{}", tab_name);
    unsafe { host_go_to_tab_name() };
}

/// Change focus to the tab with the specified name or create it if it does not exist
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

/// Closes a terminal pane with the specified id
pub fn close_terminal_pane(terminal_pane_id: i32) {
    unsafe { host_close_terminal_pane(terminal_pane_id) };
}

/// Closes a plugin pane with the specified id
pub fn close_plugin_pane(plugin_pane_id: i32) {
    unsafe { host_close_plugin_pane(plugin_pane_id) };
}

/// Changes the focus to the terminal pane with the specified id, unsuppressing it if it was suppressed and switching to its tab and layer (eg. floating/tiled).
pub fn focus_terminal_pane(terminal_pane_id: i32, should_float_if_hidden: bool) {
    unsafe { host_focus_terminal_pane(terminal_pane_id, should_float_if_hidden as i32) };
}

/// Changes the focus to the plugin pane with the specified id, unsuppressing it if it was suppressed and switching to its tab and layer (eg. floating/tiled).
pub fn focus_plugin_pane(plugin_pane_id: i32, should_float_if_hidden: bool) {
    unsafe { host_focus_plugin_pane(plugin_pane_id, should_float_if_hidden as i32) };
}

/// Changes the name (the title that appears in the UI) of the terminal pane with the specified id.
pub fn rename_terminal_pane<S: AsRef<str>>(terminal_pane_id: i32, new_name: S) {
    object_to_stdout(&(terminal_pane_id, new_name.as_ref()));
    unsafe { host_rename_terminal_pane() };
}

/// Changes the name (the title that appears in the UI) of the plugin pane with the specified id.
pub fn rename_plugin_pane<S: AsRef<str>>(plugin_pane_id: i32, new_name: S) {
    object_to_stdout(&(plugin_pane_id, new_name.as_ref()));
    unsafe { host_rename_plugin_pane() };
}

/// Changes the name (the title that appears in the UI) of the tab with the specified position.
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

/// Post a message to a worker of this plugin, for more information please see [Plugin Workers](https://zellij.dev/documentation/plugin-api-workers.md)
pub fn post_message_to<S: AsRef<str>>(worker_name: S, message: S, payload: S) {
    match serde_json::to_string(&(worker_name.as_ref(), message.as_ref(), payload.as_ref())) {
        Ok(serialized) => println!("{}", serialized),
        Err(e) => eprintln!("Failed to serialize message: {:?}", e),
    }
    unsafe { host_post_message_to() };
}

/// Post a message to this plugin, for more information please see [Plugin Workers](https://zellij.dev/documentation/plugin-api-workers.md)
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
