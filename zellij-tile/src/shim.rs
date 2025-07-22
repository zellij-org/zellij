use serde::{de::DeserializeOwned, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::{
    io,
    path::{Path, PathBuf},
};
use zellij_utils::data::*;
use zellij_utils::errors::prelude::*;
use zellij_utils::input::actions::Action;
pub use zellij_utils::plugin_api;
use zellij_utils::plugin_api::plugin_command::{
    CreateTokenResponse, ListTokensResponse, ProtobufPluginCommand, RenameWebTokenResponse,
    RevokeAllWebTokensResponse, RevokeTokenResponse,
};
use zellij_utils::plugin_api::plugin_ids::{ProtobufPluginIds, ProtobufZellijVersion};

pub use super::ui_components::*;
pub use prost::{self, *};

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

/// Open a file in the user's default `$EDITOR` in a new pane near th eplugin
pub fn open_file_near_plugin(file_to_open: FileToOpen, context: BTreeMap<String, String>) {
    let plugin_command = PluginCommand::OpenFileNearPlugin(file_to_open, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a file in the user's default `$EDITOR` in a new floating pane near the plugin
pub fn open_file_floating_near_plugin(
    file_to_open: FileToOpen,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let plugin_command =
        PluginCommand::OpenFileFloatingNearPlugin(file_to_open, coordinates, context);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Open a file in the user's default `$EDITOR`, replacing the plugin pane
pub fn open_file_in_place_of_plugin(
    file_to_open: FileToOpen,
    close_plugin_after_replace: bool,
    context: BTreeMap<String, String>,
) {
    let plugin_command =
        PluginCommand::OpenFileInPlaceOfPlugin(file_to_open, close_plugin_after_replace, context);
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

/// Open a new terminal pane to the specified location on the host filesystem
/// This variant is identical to open_terminal, excpet it opens it near the plugin regardless of
/// whether the user was focused on it or not
pub fn open_terminal_near_plugin<P: AsRef<Path>>(path: P) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command = PluginCommand::OpenTerminalNearPlugin(file_to_open);
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

/// Open a new floating terminal pane to the specified location on the host filesystem
/// This variant is identical to open_terminal_floating, excpet it opens it near the plugin regardless of
/// whether the user was focused on it or not
pub fn open_terminal_floating_near_plugin<P: AsRef<Path>>(
    path: P,
    coordinates: Option<FloatingPaneCoordinates>,
) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command = PluginCommand::OpenTerminalFloatingNearPlugin(file_to_open, coordinates);
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

/// Open a new terminal pane to the specified location on the host filesystem, temporarily
/// replacing the plugin pane
pub fn open_terminal_in_place_of_plugin<P: AsRef<Path>>(path: P, close_plugin_after_replace: bool) {
    let file_to_open = FileToOpen::new(path.as_ref().to_path_buf());
    let plugin_command =
        PluginCommand::OpenTerminalInPlaceOfPlugin(file_to_open, close_plugin_after_replace);
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

/// Open a new command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
/// This variant is the same as `open_command_pane` except it opens the pane in the same tab as the
/// plugin regardless of whether the user is focused on it
pub fn open_command_pane_near_plugin(
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let plugin_command = PluginCommand::OpenCommandPaneNearPlugin(command_to_run, context);
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

/// Open a new floating command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
/// This variant is the same as `open_command_pane_floating` except it opens the pane in the same tab as the
/// plugin regardless of whether the user is focused on it
pub fn open_command_pane_floating_near_plugin(
    command_to_run: CommandToRun,
    coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let plugin_command =
        PluginCommand::OpenCommandPaneFloatingNearPlugin(command_to_run, coordinates, context);
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

/// Open a new in place command pane with the specified command and args (this sort of pane allows the user to control the command, re-run it and see its exit status through the Zellij UI).
/// This variant is the same as open_command_pane_in_place, except that it always replaces the
/// plugin pane rather than whichever pane the user is focused on
pub fn open_command_pane_in_place_of_plugin(
    command_to_run: CommandToRun,
    close_plugin_after_replace: bool,
    context: BTreeMap<String, String>,
) {
    let plugin_command = PluginCommand::OpenCommandPaneInPlaceOfPlugin(
        command_to_run,
        close_plugin_after_replace,
        context,
    );
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
pub fn new_tab<S: AsRef<str>>(name: Option<S>, cwd: Option<S>)
where
    S: ToString,
{
    let name = name.map(|s| s.to_string());
    let cwd = cwd.map(|s| s.to_string());
    let plugin_command = PluginCommand::NewTab { name, cwd };
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

pub fn report_panic(info: &std::panic::PanicHookInfo) {
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

/// Switch to a session with the given name, create one if no name is given
pub fn switch_session_with_cwd(name: Option<&str>, cwd: Option<PathBuf>) {
    let plugin_command = PluginCommand::SwitchSession(ConnectToSession {
        name: name.map(|n| n.to_string()),
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

/// Get a list of clients, their focused pane and running command or focused plugin back as an
/// Event::ListClients (note: this event must be subscribed to)
pub fn list_clients() {
    let plugin_command = PluginCommand::ListClients;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Change configuration for the current user
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

/// Sugar for close_terminal_pane and close_plugin_pane
pub fn close_pane_with_id(pane_id: PaneId) {
    let plugin_command = match pane_id {
        PaneId::Terminal(terminal_pane_id) => PluginCommand::CloseTerminalPane(terminal_pane_id),
        PaneId::Plugin(plugin_pane_id) => PluginCommand::ClosePluginPane(plugin_pane_id),
    };
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Resize the specified pane (increase/decrease) with an optional direction (left/right/up/down)
pub fn resize_pane_with_id(resize_strategy: ResizeStrategy, pane_id: PaneId) {
    let plugin_command = PluginCommand::ResizePaneIdWithDirection(resize_strategy, pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Changes the focus to the pane with the specified id, unsuppressing it if it was suppressed and switching to its tab and layer (eg. floating/tiled).
pub fn focus_pane_with_id(pane_id: PaneId, should_float_if_hidden: bool) {
    let plugin_command = match pane_id {
        PaneId::Terminal(terminal_pane_id) => {
            PluginCommand::FocusTerminalPane(terminal_pane_id, should_float_if_hidden)
        },
        PaneId::Plugin(plugin_pane_id) => {
            PluginCommand::FocusPluginPane(plugin_pane_id, should_float_if_hidden)
        },
    };
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Edit the scrollback of the specified pane in the user's default `$EDITOR` (currently only works
/// for terminal panes)
pub fn edit_scrollback_for_pane_with_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::EditScrollbackForPaneWithId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Write bytes to the `STDIN` of the specified pane
pub fn write_to_pane_id(bytes: Vec<u8>, pane_id: PaneId) {
    let plugin_command = PluginCommand::WriteToPaneId(bytes, pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Write characters to the `STDIN` of the specified pane
pub fn write_chars_to_pane_id(chars: &str, pane_id: PaneId) {
    let plugin_command = PluginCommand::WriteCharsToPaneId(chars.to_owned(), pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch the position of the pane with this id with a different pane
pub fn move_pane_with_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::MovePaneWithPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Switch the position of the pane with this id with a different pane in the specified direction (eg. `Down`, `Up`, `Left`, `Right`).
pub fn move_pane_with_pane_id_in_direction(pane_id: PaneId, direction: Direction) {
    let plugin_command = PluginCommand::MovePaneWithPaneIdInDirection(pane_id, direction);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Clear the scroll buffer of the specified pane
pub fn clear_screen_for_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::ClearScreenForPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane up 1 line
pub fn scroll_up_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::ScrollUpInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane down 1 line
pub fn scroll_down_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::ScrollDownInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane all the way to the top of the scrollbuffer
pub fn scroll_to_top_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::ScrollToTopInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane all the way to the bottom of the scrollbuffer
pub fn scroll_to_bottom_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::ScrollToBottomInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane up one page
pub fn page_scroll_up_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::PageScrollUpInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Scroll the specified pane down one page
pub fn page_scroll_down_in_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::PageScrollDownInPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Toggle the specified pane to be fullscreen or normal sized
pub fn toggle_pane_id_fullscreen(pane_id: PaneId) {
    let plugin_command = PluginCommand::TogglePaneIdFullscreen(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Embed the specified pane (make it stop floating) or turn it to a float pane if it is not
pub fn toggle_pane_embed_or_eject_for_pane_id(pane_id: PaneId) {
    let plugin_command = PluginCommand::TogglePaneEmbedOrEjectForPaneId(pane_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Close the focused tab
pub fn close_tab_with_index(tab_index: usize) {
    let plugin_command = PluginCommand::CloseTabWithIndex(tab_index);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Rename the specified pane
pub fn rename_pane_with_id<S: AsRef<str>>(pane_id: PaneId, new_name: S)
where
    S: ToString,
{
    let plugin_command = match pane_id {
        PaneId::Terminal(terminal_pane_id) => {
            PluginCommand::RenameTerminalPane(terminal_pane_id, new_name.to_string())
        },
        PaneId::Plugin(plugin_pane_id) => {
            PluginCommand::RenamePluginPane(plugin_pane_id, new_name.to_string())
        },
    };
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Create a new tab that includes the specified pane ids
pub fn break_panes_to_new_tab(
    pane_ids: &[PaneId],
    new_tab_name: Option<String>,
    should_change_focus_to_new_tab: bool,
) {
    let plugin_command = PluginCommand::BreakPanesToNewTab(
        pane_ids.to_vec(),
        new_tab_name,
        should_change_focus_to_new_tab,
    );
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Move the pane ids to the tab with the specified index
pub fn break_panes_to_tab_with_index(
    pane_ids: &[PaneId],
    tab_index: usize,
    should_change_focus_to_new_tab: bool,
) {
    let plugin_command = PluginCommand::BreakPanesToTabWithIndex(
        pane_ids.to_vec(),
        tab_index,
        should_change_focus_to_new_tab,
    );
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Reload an already-running in this session, optionally skipping the cache
pub fn reload_plugin_with_id(plugin_id: u32) {
    let plugin_command = PluginCommand::ReloadPlugin(plugin_id);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Reload an already-running in this session, optionally skipping the cache
pub fn load_new_plugin<S: AsRef<str>>(
    url: S,
    config: BTreeMap<String, String>,
    load_in_background: bool,
    skip_plugin_cache: bool,
) where
    S: ToString,
{
    let plugin_command = PluginCommand::LoadNewPlugin {
        url: url.to_string(),
        config,
        load_in_background,
        skip_plugin_cache,
    };
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

/// Rebind keys for the current user
pub fn rebind_keys(
    keys_to_unbind: Vec<(InputMode, KeyWithModifier)>,
    keys_to_rebind: Vec<(InputMode, KeyWithModifier, Vec<Action>)>,
    write_config_to_disk: bool,
) {
    let plugin_command = PluginCommand::RebindKeys {
        keys_to_rebind,
        keys_to_unbind,
        write_config_to_disk,
    };
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn change_host_folder(new_host_folder: PathBuf) {
    let plugin_command = PluginCommand::ChangeHostFolder(new_host_folder);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn set_floating_pane_pinned(pane_id: PaneId, should_be_pinned: bool) {
    let plugin_command = PluginCommand::SetFloatingPanePinned(pane_id, should_be_pinned);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn stack_panes(pane_ids: Vec<PaneId>) {
    let plugin_command = PluginCommand::StackPanes(pane_ids);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn change_floating_panes_coordinates(
    pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
) {
    let plugin_command = PluginCommand::ChangeFloatingPanesCoordinates(pane_ids_and_coordinates);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn start_web_server() {
    let plugin_command = PluginCommand::StartWebServer;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn stop_web_server() {
    let plugin_command = PluginCommand::StopWebServer;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn query_web_server_status() {
    let plugin_command = PluginCommand::QueryWebServerStatus;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn share_current_session() {
    let plugin_command = PluginCommand::ShareCurrentSession;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn stop_sharing_current_session() {
    let plugin_command = PluginCommand::StopSharingCurrentSession;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn group_and_ungroup_panes(
    pane_ids_to_group: Vec<PaneId>,
    pane_ids_to_ungroup: Vec<PaneId>,
    for_all_clients: bool,
) {
    let plugin_command = PluginCommand::GroupAndUngroupPanes(
        pane_ids_to_group,
        pane_ids_to_ungroup,
        for_all_clients,
    );
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn highlight_and_unhighlight_panes(
    pane_ids_to_highlight: Vec<PaneId>,
    pane_ids_to_unhighlight: Vec<PaneId>,
) {
    let plugin_command =
        PluginCommand::HighlightAndUnhighlightPanes(pane_ids_to_highlight, pane_ids_to_unhighlight);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn close_multiple_panes(pane_ids: Vec<PaneId>) {
    let plugin_command = PluginCommand::CloseMultiplePanes(pane_ids);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn float_multiple_panes(pane_ids: Vec<PaneId>) {
    let plugin_command = PluginCommand::FloatMultiplePanes(pane_ids);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn embed_multiple_panes(pane_ids: Vec<PaneId>) {
    let plugin_command = PluginCommand::EmbedMultiplePanes(pane_ids);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn set_self_mouse_selection_support(selection_support: bool) {
    let plugin_command = PluginCommand::SetSelfMouseSelectionSupport(selection_support);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn generate_web_login_token(token_label: Option<String>) -> Result<String, String> {
    let plugin_command = PluginCommand::GenerateWebLoginToken(token_label);
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let create_token_response =
        CreateTokenResponse::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    if let Some(error) = create_token_response.error {
        Err(error)
    } else if let Some(token) = create_token_response.token {
        Ok(token)
    } else {
        Err("Received empty response".to_owned())
    }
}

pub fn revoke_web_login_token(token_label: &str) -> Result<(), String> {
    let plugin_command = PluginCommand::RevokeWebLoginToken(token_label.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let revoke_token_response =
        RevokeTokenResponse::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    if let Some(error) = revoke_token_response.error {
        Err(error)
    } else {
        Ok(())
    }
}

pub fn list_web_login_tokens() -> Result<Vec<(String, String)>, String> {
    // (name, created_at)
    let plugin_command = PluginCommand::ListWebLoginTokens;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let list_tokens_response =
        ListTokensResponse::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    if let Some(error) = list_tokens_response.error {
        Err(error)
    } else {
        let tokens_and_creation_times = std::iter::zip(
            list_tokens_response.tokens,
            list_tokens_response.creation_times,
        )
        .collect();
        Ok(tokens_and_creation_times)
    }
}

pub fn revoke_all_web_tokens() -> Result<(), String> {
    let plugin_command = PluginCommand::RevokeAllWebLoginTokens;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let revoke_all_web_tokens_response =
        RevokeAllWebTokensResponse::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    if let Some(error) = revoke_all_web_tokens_response.error {
        Err(error)
    } else {
        Ok(())
    }
}

pub fn rename_web_token(old_name: &str, new_name: &str) -> Result<(), String> {
    let plugin_command =
        PluginCommand::RenameWebLoginToken(old_name.to_owned(), new_name.to_owned());
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
    let rename_web_token_response =
        RenameWebTokenResponse::decode(bytes_from_stdin().unwrap().as_slice()).unwrap();
    if let Some(error) = rename_web_token_response.error {
        Err(error)
    } else {
        Ok(())
    }
}

pub fn intercept_key_presses() {
    let plugin_command = PluginCommand::InterceptKeyPresses;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn clear_key_presses_intercepts() {
    let plugin_command = PluginCommand::ClearKeyPressesIntercepts;
    let protobuf_plugin_command: ProtobufPluginCommand = plugin_command.try_into().unwrap();
    object_to_stdout(&protobuf_plugin_command.encode_to_vec());
    unsafe { host_run_plugin_command() };
}

pub fn replace_pane_with_existing_pane(pane_id_to_replace: PaneId, existing_pane_id: PaneId) {
    let plugin_command =
        PluginCommand::ReplacePaneWithExistingPane(pane_id_to_replace, existing_pane_id);
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
