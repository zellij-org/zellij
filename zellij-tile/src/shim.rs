use serde::{de::DeserializeOwned, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::{
    io,
    path::{Path, PathBuf},
};
use zellij_utils::data::*;
use zellij_utils::errors::prelude::*;
pub use zellij_utils::plugin_api;
use zellij_utils::plugin_api::plugin_command::ProtobufPluginCommand;
use zellij_utils::plugin_api::plugin_ids::{ProtobufPluginIds, ProtobufZellijVersion};

pub use super::ui_components::*;
pub use zellij_utils::prost::{self, *};

// Subscription Handling

/// Subscribe to a list of [`Event`]s represented by their [`EventType`]s that will then trigger the `update` method
pub fn subscribe(event_types: &[EventType]) {
    let event_types: HashSet<EventType> = event_types.iter().cloned().collect();
    let plugin_command = PluginCommand::Subscribe(event_types);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Unsubscribe to a list of [`Event`]s represented by their [`EventType`]s.
pub fn unsubscribe(event_types: &[EventType]) {
    let event_types: HashSet<EventType> = event_types.iter().cloned().collect();
    let plugin_command = PluginCommand::Unsubscribe(event_types);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

// Plugin Settings

/// Sets the plugin as selectable or unselectable to the user. Unselectable plugins might be desired when they do not accept user input.
pub fn set_selectable(selectable: bool) {
    let plugin_command = PluginCommand::SetSelectable(selectable);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn request_permission(permissions: &[PermissionType]) {
    let plugin_command = PluginCommand::RequestPluginPermissions(permissions.into());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

// Query Functions
/// Returns the unique Zellij pane ID for the plugin as well as the Zellij process id.
pub fn get_plugin_ids() -> PluginIds {
    let plugin_command = PluginCommand::GetPluginIds;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let protobuf_plugin_ids =
        ProtobufPluginIds::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    PluginIds::try_from(protobuf_plugin_ids).unwrap()
}

/// Returns the version of the running Zellij instance - can be useful to check plugin compatibility
pub fn get_zellij_version() -> String {
    let plugin_command = PluginCommand::GetZellijVersion;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let protobuf_zellij_version =
        ProtobufZellijVersion::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    protobuf_zellij_version.version
}

// Host Functions

/// Open a file in the user's default `$EDITOR` in a new pane
pub fn open_file(file_to_open: FileToOpen, context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::OpenFile(file_to_open, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a file in the user's default `$EDITOR` in a new floating pane
pub fn open_file_floating(
    file_to_open: FileToOpen,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let plugin_command = PluginCommand::OpenFileFloating(file_to_open, coordinates, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a file in the user's default `$EDITOR`, replacing the focused pane
pub fn open_file_in_place(file_to_open: FileToOpen, context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::OpenFileInPlace(file_to_open, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new terminal pane to the specified location on the host filesystem
pub fn open_terminal<P: AsRef<Path>>(path: P) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command = PluginCommand::OpenTerminal(file_to_open);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new floating terminal pane to the specified location on the host filesystem
pub fn open_terminal_floating<P: AsRef<Path>>(
    path: P,
    coordinates: Option<FloatingPaneCoordinates>,
) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command = PluginCommand::OpenTerminalFloating(file_to_open, coordinates);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new terminal pane to the specified location on the host filesystem, temporarily
/// replacing the focused pane
pub fn open_terminal_in_place<P: AsRef<Path>>(path: P) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command = PluginCommand::OpenTerminalInPlace(file_to_open);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane(command_to_run: CommandToRun, context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::OpenCommandPane(command_to_run, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new floating command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane_floating(
    command_to_run: CommandToRun,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let plugin_command =
        PluginCommand::OpenCommandPaneFloating(command_to_run, coordinates, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new in place command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane_in_place(command_to_run: CommandToRun, context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::OpenCommandPaneInPlace(command_to_run, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new hidden (background) command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
pub fn open_command_pane_background(
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let plugin_command = PluginCommand::OpenCommandPaneBackground(command_to_run, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change the focused tab to the specified index (corresponding with the default tab names, to starting at `1`, `0` will be considered as `1`).
pub fn switch_tab_to(tab_idx: u32) {
    let plugin_command = PluginCommand::SwitchTabTo(tab_idx);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Set a timeout in seconds (or fractions thereof) after which the plugins [update](./plugin-api-events#update) method will be called with the [`Timer`](./plugin-api-events.md#timer) event.
pub fn set_timeout(secs: f64) {
    let plugin_command = PluginCommand::SetTimeout(secs);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

#[doc(hidden)]
pub fn exec_cmd(cmd: &[&str]) {
    let plugin_command =
        PluginCommand::ExecCmd(cmd.iter().cloned().map(|s| s.to_owned()).collect());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Run this command in the background on the host machine, optionally being notified of its output
/// if subscribed to the `RunCommandResult` Event
pub fn run_command(cmd: &[&str], context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::RunCommand(
        cmd.iter().cloned().map(|s| s.to_owned()).collect(),
        BTreeMap::new(),
        PathBuf::from("."),
        context,
    );
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Run this command in the background on the host machine, providing environment variables and a
/// cwd. Optionally being notified of its output if subscribed to the `RunCommandResult` Event
pub fn run_command_with_env_variables_and_cwd(
    cmd: &[&str],
    env_variables: BTreeMap<String, String>,
    cwd: PathBuf,
    context: BTreeMap<String, String>,
) {
    let plugin_command = PluginCommand::RunCommand(
        cmd.iter().cloned().map(|s| s.to_owned()).collect(),
        env_variables,
        cwd,
        context,
    );
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Make a web request, optionally being notified of its output
/// if subscribed to the `WebRequestResult` Event, the context will be returned verbatim in this
/// event and can be used for eg. marking the request_id
pub fn web_request<S: AsRef<str>>(
    url: S,
    verb: HttpVerb,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    context: BTreeMap<String, String>,
) where
    S: ToString,
{
    let plugin_command = PluginCommand::WebRequest(url.to_string(), verb, headers, body, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Hide the plugin pane (suppress it) from the UI
pub fn hide_self() {
    let plugin_command = PluginCommand::HideSelf;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Hide the pane (suppress it) with the specified [PaneId] from the UI
pub fn hide_pane_with_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::HidePaneWithId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Show the plugin pane (unsuppress it if it is suppressed), focus it and switch to its tab
pub fn show_self(should_float_if_hidden: bool) {
    let plugin_command = PluginCommand::ShowSelf(should_float_if_hidden);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Show the pane (unsuppress it if it is suppressed) with the specified [PaneId], focus it and switch to its tab
pub fn show_pane_with_id(pane_id: PaneId, should_float_if_hidden: bool) {
    let plugin_command = PluginCommand::ShowPaneWithId(pane_id, should_float_if_hidden);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Close this plugin pane
pub fn close_self() {
    let plugin_command = PluginCommand::CloseSelf;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch to the specified Input Mode (eg. `Normal`, `Tab`, `Pane`)
pub fn switch_to_input_mode(mode: &InputMode) {
    let plugin_command = PluginCommand::SwitchToMode(*mode);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Provide a stringified [`layout`](https://zellij.dev/documentation/layouts.html) to be applied to the current session. If the layout has multiple tabs, they will all be opened.
pub fn new_tabs_with_layout(layout: &str) {
    let plugin_command = PluginCommand::NewTabsWithLayout(layout.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Provide a LayoutInfo to be applied to the current session in a new tab. If the layout has multiple tabs, they will all be opened.
pub fn new_tabs_with_layout_info(layout_info: LayoutInfo) {
    let plugin_command = PluginCommand::NewTabsWithLayoutInfo(layout_info);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a new tab with the default layout
pub fn new_tab() {
    let plugin_command = PluginCommand::NewTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus to the next tab or loop back to the first
pub fn go_to_next_tab() {
    let plugin_command = PluginCommand::GoToNextTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus to the previous tab or loop back to the last
pub fn go_to_previous_tab() {
    let plugin_command = PluginCommand::GoToPreviousTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn report_panic(info: &std::panic::PanicInfo) {
    let panic_payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
        format!("{}", s)
    } else {
        format!("<NO PAYLOAD>")
    };
    let panic_stringified = format!("{}\n\r{:#?}", panic_payload, info).replace("\n", "\r\n");
    let plugin_command = PluginCommand::ReportPanic(panic_stringified);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Either Increase or Decrease the size of the focused pane
pub fn resize_focused_pane(resize: Resize) {
    let plugin_command = PluginCommand::Resize(resize);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Either Increase or Decrease the size of the focused pane in a specified direction (eg. `Left`, `Right`, `Up`, `Down`).
pub fn resize_focused_pane_with_direction(resize: Resize, direction: Direction) {
    let resize_strategy = ResizeStrategy {
        resize,
        direction: Some(direction),
        invert_on_boundaries: false,
    };
    let plugin_command = PluginCommand::ResizeWithDirection(resize_strategy);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus tot he next pane in chronological order
pub fn focus_next_pane() {
    let plugin_command = PluginCommand::FocusNextPane;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus to the previous pane in chronological order
pub fn focus_previous_pane() {
    let plugin_command = PluginCommand::FocusPreviousPane;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change the focused pane in the specified direction
pub fn move_focus(direction: Direction) {
    let plugin_command = PluginCommand::MoveFocus(direction);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change the focused pane in the specified direction, if the pane is on the edge of the screen, the next tab is focused (next if right edge, previous if left edge).
pub fn move_focus_or_tab(direction: Direction) {
    let plugin_command = PluginCommand::MoveFocusOrTab(direction);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Detach the user from the active session
pub fn detach() {
    let plugin_command = PluginCommand::Detach;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Edit the scrollback of the focused pane in the user's default `$EDITOR`
pub fn edit_scrollback() {
    let plugin_command = PluginCommand::EditScrollback;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Write bytes to the `STDIN` of the focused pane
pub fn write(bytes: Vec<u8>) {
    let plugin_command = PluginCommand::Write(bytes);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Write characters to the `STDIN` of the focused pane
pub fn write_chars(chars: &str) {
    let plugin_command = PluginCommand::WriteChars(chars.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Focused the previously focused tab (regardless of the tab position)
pub fn toggle_tab() {
    let plugin_command = PluginCommand::ToggleTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch the position of the focused pane with a different pane
pub fn move_pane() {
    let plugin_command = PluginCommand::MovePane;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch the position of the focused pane with a different pane in the specified direction (eg. `Down`, `Up`, `Left`, `Right`).
pub fn move_pane_with_direction(direction: Direction) {
    let plugin_command = PluginCommand::MovePaneWithDirection(direction);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Clear the scroll buffer of the focused pane
pub fn clear_screen() {
    let plugin_command = PluginCommand::ClearScreen;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane up 1 line
pub fn scroll_up() {
    let plugin_command = PluginCommand::ScrollUp;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane down 1 line
pub fn scroll_down() {
    let plugin_command = PluginCommand::ScrollDown;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane all the way to the top of the scrollbuffer
pub fn scroll_to_top() {
    let plugin_command = PluginCommand::ScrollToTop;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane all the way to the bottom of the scrollbuffer
pub fn scroll_to_bottom() {
    let plugin_command = PluginCommand::ScrollToBottom;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane up one page
pub fn page_scroll_up() {
    let plugin_command = PluginCommand::PageScrollUp;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the focused pane down one page
pub fn page_scroll_down() {
    let plugin_command = PluginCommand::PageScrollDown;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Toggle the focused pane to be fullscreen or normal sized
pub fn toggle_focus_fullscreen() {
    let plugin_command = PluginCommand::ToggleFocusFullscreen;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Toggle the UI pane frames on or off
pub fn toggle_pane_frames() {
    let plugin_command = PluginCommand::TogglePaneFrames;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Embed the currently focused pane (make it stop floating) or turn it to a float pane if it is not
pub fn toggle_pane_embed_or_eject() {
    let plugin_command = PluginCommand::TogglePaneEmbedOrEject;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn undo_rename_pane() {
    let plugin_command = PluginCommand::UndoRenamePane;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Close the focused pane
pub fn close_focus() {
    let plugin_command = PluginCommand::CloseFocus;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Turn the `STDIN` synchronization of the current tab on or off
pub fn toggle_active_tab_sync() {
    let plugin_command = PluginCommand::ToggleActiveTabSync;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Close the focused tab
pub fn close_focused_tab() {
    let plugin_command = PluginCommand::CloseFocusedTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn undo_rename_tab() {
    let plugin_command = PluginCommand::UndoRenameTab;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Compeltely quit Zellij for this and all other connected clients
pub fn quit_zellij() {
    let plugin_command = PluginCommand::QuitZellij;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change to the previous [swap layout](https://zellij.dev/documentation/swap-layouts.html)
pub fn previous_swap_layout() {
    let plugin_command = PluginCommand::PreviousSwapLayout;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change to the next [swap layout](https://zellij.dev/documentation/swap-layouts.html)
pub fn next_swap_layout() {
    let plugin_command = PluginCommand::NextSwapLayout;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus to the tab with the specified name
pub fn go_to_tab_name(tab_name: &str) {
    let plugin_command = PluginCommand::GoToTabName(tab_name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change focus to the tab with the specified name or create it if it does not exist
pub fn focus_or_create_tab(tab_name: &str) {
    let plugin_command = PluginCommand::FocusOrCreateTab(tab_name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn go_to_tab(tab_index: u32) {
    let plugin_command = PluginCommand::GoToTab(tab_index);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn start_or_reload_plugin(url: &str) {
    let plugin_command = PluginCommand::StartOrReloadPlugin(url.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Closes a terminal pane with the specified id
pub fn close_terminal_pane(terminal_pane_id: u32) {
    let plugin_command = PluginCommand::CloseTerminalPane(terminal_pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Closes a plugin pane with the specified id
pub fn close_plugin_pane(plugin_pane_id: u32) {
    let plugin_command = PluginCommand::ClosePluginPane(plugin_pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the focus to the terminal pane with the specified id, unsuppressing it if it was suppressed and switching to its tab and layer (eg. floating/tiled).
pub fn focus_terminal_pane(terminal_pane_id: u32, should_float_if_hidden: bool) {
    let plugin_command = PluginCommand::FocusTerminalPane(terminal_pane_id, should_float_if_hidden);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the focus to the plugin pane with the specified id, unsuppressing it if it was suppressed and switching to its tab and layer (eg. floating/tiled).
pub fn focus_plugin_pane(plugin_pane_id: u32, should_float_if_hidden: bool) {
    let plugin_command = PluginCommand::FocusPluginPane(plugin_pane_id, should_float_if_hidden);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the name (the title that appears in the UI) of the terminal pane with the specified id.
pub fn rename_terminal_pane<S: AsRef<str>>(terminal_pane_id: u32, new_name: S)
where
    S: ToString,
{
    let plugin_command = PluginCommand::RenameTerminalPane(terminal_pane_id, new_name.to_string());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the name (the title that appears in the UI) of the plugin pane with the specified id.
pub fn rename_plugin_pane<S: AsRef<str>>(plugin_pane_id: u32, new_name: S)
where
    S: ToString,
{
    let plugin_command = PluginCommand::RenamePluginPane(plugin_pane_id, new_name.to_string());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the name (the title that appears in the UI) of the tab with the specified position.
pub fn rename_tab<S: AsRef<str>>(tab_position: u32, new_name: S)
where
    S: ToString,
{
    let plugin_command = PluginCommand::RenameTab(tab_position, new_name.to_string());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch to a session with the given name, create one if no name is given
pub fn switch_session(name: Option<&str>) {
    let plugin_command = PluginCommand::SwitchSession(ConnectToSession {
        name: name.map(|n| n.to_string()),
        ..Default::default()
    });
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch to a session with the given name, create one if no name is given
pub fn switch_session_with_layout(name: Option<&str>, layout: LayoutInfo, cwd: Option<PathBuf>) {
    let plugin_command = PluginCommand::SwitchSession(ConnectToSession {
        name: name.map(|n| n.to_string()),
        layout: Some(layout),
        cwd,
        ..Default::default()
    });
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch to a session with the given name, focusing either the provided pane_id or the provided
/// tab position (in that order)
pub fn switch_session_with_focus(
    name: &str,
    tab_position: Option<usize>,
    pane_id: Option<(u32, bool)>,
) {
    let plugin_command = PluginCommand::SwitchSession(ConnectToSession {
        name: Some(name.to_owned()),
        tab_position,
        pane_id,
        ..Default::default()
    });
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Permanently delete a resurrectable session with the given name
pub fn delete_dead_session(name: &str) {
    let plugin_command = PluginCommand::DeleteDeadSession(name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Permanently delete aall resurrectable sessions on this machine
pub fn delete_all_dead_sessions() {
    let plugin_command = PluginCommand::DeleteAllDeadSessions;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Rename the current session
pub fn rename_session(name: &str) {
    let plugin_command = PluginCommand::RenameSession(name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Unblock the input side of a pipe, requesting the next message be sent if there is one
pub fn unblock_cli_pipe_input(pipe_name: &str) {
    let plugin_command = PluginCommand::UnblockCliPipeInput(pipe_name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Block the input side of a pipe, will only be released once this or another plugin unblocks it
pub fn block_cli_pipe_input(pipe_name: &str) {
    let plugin_command = PluginCommand::BlockCliPipeInput(pipe_name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Send output to the output side of a pipe, ths does not affect the input side of same pipe
pub fn cli_pipe_output(pipe_name: &str, output: &str) {
    let plugin_command = PluginCommand::CliPipeOutput(pipe_name.to_owned(), output.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Send a message to a plugin, it will be launched if it is not already running
pub fn pipe_message_to_plugin(message_to_plugin: MessageToPlugin) {
    let plugin_command = PluginCommand::MessageToPlugin(message_to_plugin);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Disconnect all other clients from the current session
pub fn disconnect_other_clients() {
    let plugin_command = PluginCommand::DisconnectOtherClients;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Kill all Zellij sessions in the list
pub fn kill_sessions<S: AsRef<str>>(session_names: &[S])
where
    S: ToString,
{
    let plugin_command =
        PluginCommand::KillSessions(session_names.into_iter().map(|s| s.to_string()).collect());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scan a specific folder in the host filesystem (this is a hack around some WASI runtime performance
/// issues), will not follow symlinks
pub fn scan_host_folder<S: AsRef<Path>>(folder_to_scan: &S) {
    let plugin_command = PluginCommand::ScanHostFolder(folder_to_scan.as_ref().to_path_buf());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Start watching the host folder for filesystem changes (Note: somewhat unstable at the time
/// being)
pub fn watch_filesystem() {
    let plugin_command = PluginCommand::WatchFilesystem;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Get the serialized session layout in KDL format as a CustomMessage Event
pub fn dump_session_layout() {
    let plugin_command = PluginCommand::DumpSessionLayout;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Rebind keys for the current user
pub fn reconfigure(new_config: String, save_configuration_file: bool) {
    let plugin_command = PluginCommand::Reconfigure(new_config, save_configuration_file);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Re-run command in pane
pub fn rerun_command_pane(terminal_pane_id: u32) {
    let plugin_command = PluginCommand::RerunCommandPane(terminal_pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

// Utility Functions

#[allow(unused)]
/// Returns the `TabInfo` corresponding to the currently active tab
pub fn get_focused_tab(tab_infos: &Vec<TabInfo>) -> Option<TabInfo> {
    for tab_info in tab_infos {
        if tab_info.active {
            return Some(tab_info.clone());
        }
    }
    return None;
}

#[allow(unused)]
/// Returns the `PaneInfo` corresponding to the currently active pane (ignoring plugins)
pub fn get_focused_pane(tab_position: usize, pane_manifest: &PaneManifest) -> Option<PaneInfo> {
    let panes = pane_manifest.panes.get(&tab_position);
    if let Some(panes) = panes {
        for pane in panes {
            if pane.is_focused & !pane.is_plugin {
                return Some(pane.clone());
            }
        }
    }
    None
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
pub fn bytes_from_stdin() -> Result<Vec<u8>> {
    let err_context = || "failed to deserialize bytes from stdin".to_string();
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
pub fn post_message_to(plugin_message: PluginMessage) {
    let plugin_command = PluginCommand::PostMessageTo(plugin_message);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Post a message to this plugin, for more information please see [Plugin Workers](https://zellij.dev/documentation/plugin-api-workers.md)
pub fn post_message_to_plugin(plugin_message: PluginMessage) {
    let plugin_command = PluginCommand::PostMessageToPlugin(plugin_message);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

#[link(wasm_import_module = "zellij")]
extern "C" {
    fn host_run_plugin_command();
}
