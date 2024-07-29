use super::PluginInstruction;
use crate::background_jobs::BackgroundJob;
use crate::plugins::plugin_map::PluginEnv;
use crate::plugins::wasm_bridge::handle_plugin_crash;
use crate::route::route_action;
use crate::pty::{PtyInstruction, ClientTabIndexOrPaneId};
use crate::ServerInstruction;
use log::{debug, warn};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    io::{Read, Write},
    path::PathBuf,
    process,
    str::FromStr,
    thread,
    time::{Duration, Instant},
};
use wasmtime::{Caller, Linker};
use zellij_utils::data::{
    CommandType, ConnectToSession, FloatingPaneCoordinates, HttpVerb, LayoutInfo, MessageToPlugin,
    OriginatingPlugin, PermissionStatus, PermissionType, PluginPermission,
};
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::{
    async_std::task,
    interprocess::local_socket::LocalSocketStream,
    ipc::{ClientToServerMsg, IpcSenderWithContext},
};

use crate::{panes::PaneId, screen::ScreenInstruction};

use zellij_utils::{
    consts::{VERSION, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR},
    data::{
        CommandToRun, Direction, Event, EventType, FileToOpen, InputMode, PluginCommand, PluginIds,
        PluginMessage, Resize, ResizeStrategy,
    },
    errors::prelude::*,
    input::{
        actions::Action,
        command::{RunCommand, RunCommandAction, TerminalAction, OpenFilePayload},
        layout::{Layout, RunPluginOrAlias},
        plugins::PluginType,
    },
    plugin_api::{
        plugin_command::ProtobufPluginCommand,
        plugin_ids::{ProtobufPluginIds, ProtobufZellijVersion},
    },
    prost::Message,
    serde,
};

macro_rules! apply_action {
    ($action:ident, $error_message:ident, $env: ident) => {
        if let Err(e) = route_action(
            $action,
            $env.client_id,
            Some(PaneId::Plugin($env.plugin_id)),
            $env.senders.clone(),
            $env.capabilities.clone(),
            $env.client_attributes.clone(),
            $env.default_shell.clone(),
            $env.default_layout.clone(),
            None,
            $env.keybinds.clone(),
            $env.default_mode.clone(),
        ) {
            log::error!("{}: {:?}", $error_message(), e);
        }
    };
}

pub fn zellij_exports(linker: &mut Linker<PluginEnv>) {
    linker
        .func_wrap("zellij", "host_run_plugin_command", host_run_plugin_command)
        .unwrap();
}

fn host_run_plugin_command(caller: Caller<'_, PluginEnv>) {
    let env = caller.data();
    let err_context = || format!("failed to run plugin command {}", env.name());
    wasi_read_bytes(env)
        .and_then(|bytes| {
            let command: ProtobufPluginCommand = ProtobufPluginCommand::decode(bytes.as_slice())?;
            let command: PluginCommand = command
                .try_into()
                .map_err(|e| anyhow!("failed to convert serialized command: {}", e))?;
            match check_command_permission(&env, &command) {
                (PermissionStatus::Granted, _) => match command {
                    PluginCommand::Subscribe(event_list) => subscribe(env, event_list)?,
                    PluginCommand::Unsubscribe(event_list) => unsubscribe(env, event_list)?,
                    PluginCommand::SetSelectable(selectable) => set_selectable(env, selectable),
                    PluginCommand::GetPluginIds => get_plugin_ids(env),
                    PluginCommand::GetZellijVersion => get_zellij_version(env),
                    PluginCommand::OpenFile(file_to_open, context) => open_file(env, file_to_open, context),
                    PluginCommand::OpenFileFloating(file_to_open, floating_pane_coordinates, context) => {
                        open_file_floating(env, file_to_open, floating_pane_coordinates, context)
                    },
                    PluginCommand::OpenTerminal(cwd) => open_terminal(env, cwd.path.try_into()?),
                    PluginCommand::OpenTerminalFloating(cwd, floating_pane_coordinates) => {
                        open_terminal_floating(env, cwd.path.try_into()?, floating_pane_coordinates)
                    },
                    PluginCommand::OpenCommandPane(command_to_run, context) => {
                        open_command_pane(env, command_to_run, context)
                    },
                    PluginCommand::OpenCommandPaneFloating(
                        command_to_run,
                        floating_pane_coordinates,
                        context,
                    ) => open_command_pane_floating(
                        env,
                        command_to_run,
                        floating_pane_coordinates,
                        context,
                    ),
                    PluginCommand::SwitchTabTo(tab_index) => switch_tab_to(env, tab_index),
                    PluginCommand::SetTimeout(seconds) => set_timeout(env, seconds),
                    PluginCommand::ExecCmd(command_line) => exec_cmd(env, command_line),
                    PluginCommand::RunCommand(command_line, env_variables, cwd, context) => {
                        run_command(env, command_line, env_variables, cwd, context)
                    },
                    PluginCommand::WebRequest(url, verb, headers, body, context) => {
                        web_request(env, url, verb, headers, body, context)
                    },
                    PluginCommand::PostMessageTo(plugin_message) => {
                        post_message_to(env, plugin_message)?
                    },
                    PluginCommand::PostMessageToPlugin(plugin_message) => {
                        post_message_to_plugin(env, plugin_message)?
                    },
                    PluginCommand::HideSelf => hide_self(env)?,
                    PluginCommand::ShowSelf(should_float_if_hidden) => {
                        show_self(env, should_float_if_hidden)
                    },
                    PluginCommand::SwitchToMode(input_mode) => {
                        switch_to_mode(env, input_mode.try_into()?)
                    },
                    PluginCommand::NewTabsWithLayout(raw_layout) => {
                        new_tabs_with_layout(env, &raw_layout)?
                    },
                    PluginCommand::NewTabsWithLayoutInfo(layout_info) => {
                        new_tabs_with_layout_info(env, layout_info)?
                    },
                    PluginCommand::NewTab => new_tab(env),
                    PluginCommand::GoToNextTab => go_to_next_tab(env),
                    PluginCommand::GoToPreviousTab => go_to_previous_tab(env),
                    PluginCommand::Resize(resize_payload) => resize(env, resize_payload),
                    PluginCommand::ResizeWithDirection(resize_strategy) => {
                        resize_with_direction(env, resize_strategy)
                    },
                    PluginCommand::FocusNextPane => focus_next_pane(env),
                    PluginCommand::FocusPreviousPane => focus_previous_pane(env),
                    PluginCommand::MoveFocus(direction) => move_focus(env, direction),
                    PluginCommand::MoveFocusOrTab(direction) => move_focus_or_tab(env, direction),
                    PluginCommand::Detach => detach(env),
                    PluginCommand::EditScrollback => edit_scrollback(env),
                    PluginCommand::Write(bytes) => write(env, bytes),
                    PluginCommand::WriteChars(chars) => write_chars(env, chars),
                    PluginCommand::ToggleTab => toggle_tab(env),
                    PluginCommand::MovePane => move_pane(env),
                    PluginCommand::MovePaneWithDirection(direction) => {
                        move_pane_with_direction(env, direction)
                    },
                    PluginCommand::ClearScreen => clear_screen(env),
                    PluginCommand::ScrollUp => scroll_up(env),
                    PluginCommand::ScrollDown => scroll_down(env),
                    PluginCommand::ScrollToTop => scroll_to_top(env),
                    PluginCommand::ScrollToBottom => scroll_to_bottom(env),
                    PluginCommand::PageScrollUp => page_scroll_up(env),
                    PluginCommand::PageScrollDown => page_scroll_down(env),
                    PluginCommand::ToggleFocusFullscreen => toggle_focus_fullscreen(env),
                    PluginCommand::TogglePaneFrames => toggle_pane_frames(env),
                    PluginCommand::TogglePaneEmbedOrEject => toggle_pane_embed_or_eject(env),
                    PluginCommand::UndoRenamePane => undo_rename_pane(env),
                    PluginCommand::CloseFocus => close_focus(env),
                    PluginCommand::ToggleActiveTabSync => toggle_active_tab_sync(env),
                    PluginCommand::CloseFocusedTab => close_focused_tab(env),
                    PluginCommand::UndoRenameTab => undo_rename_tab(env),
                    PluginCommand::QuitZellij => quit_zellij(env),
                    PluginCommand::PreviousSwapLayout => previous_swap_layout(env),
                    PluginCommand::NextSwapLayout => next_swap_layout(env),
                    PluginCommand::GoToTabName(tab_name) => go_to_tab_name(env, tab_name),
                    PluginCommand::FocusOrCreateTab(tab_name) => focus_or_create_tab(env, tab_name),
                    PluginCommand::GoToTab(tab_index) => go_to_tab(env, tab_index),
                    PluginCommand::StartOrReloadPlugin(plugin_url) => {
                        start_or_reload_plugin(env, &plugin_url)?
                    },
                    PluginCommand::CloseTerminalPane(terminal_pane_id) => {
                        close_terminal_pane(env, terminal_pane_id)
                    },
                    PluginCommand::ClosePluginPane(plugin_pane_id) => {
                        close_plugin_pane(env, plugin_pane_id)
                    },
                    PluginCommand::FocusTerminalPane(terminal_pane_id, should_float_if_hidden) => {
                        focus_terminal_pane(env, terminal_pane_id, should_float_if_hidden)
                    },
                    PluginCommand::FocusPluginPane(plugin_pane_id, should_float_if_hidden) => {
                        focus_plugin_pane(env, plugin_pane_id, should_float_if_hidden)
                    },
                    PluginCommand::RenameTerminalPane(terminal_pane_id, new_name) => {
                        rename_terminal_pane(env, terminal_pane_id, &new_name)
                    },
                    PluginCommand::RenamePluginPane(plugin_pane_id, new_name) => {
                        rename_plugin_pane(env, plugin_pane_id, &new_name)
                    },
                    PluginCommand::RenameTab(tab_index, new_name) => {
                        rename_tab(env, tab_index, &new_name)
                    },
                    PluginCommand::ReportPanic(crash_payload) => report_panic(env, &crash_payload),
                    PluginCommand::RequestPluginPermissions(permissions) => {
                        request_permission(env, permissions)?
                    },
                    PluginCommand::SwitchSession(connect_to_session) => switch_session(
                        env,
                        connect_to_session.name,
                        connect_to_session.tab_position,
                        connect_to_session.pane_id,
                        connect_to_session.layout,
                        connect_to_session.cwd,
                    )?,
                    PluginCommand::DeleteDeadSession(session_name) => {
                        delete_dead_session(session_name)?
                    },
                    PluginCommand::DeleteAllDeadSessions => delete_all_dead_sessions()?,
                    PluginCommand::OpenFileInPlace(file_to_open, context) => {
                        open_file_in_place(env, file_to_open, context)
                    },
                    PluginCommand::OpenTerminalInPlace(cwd) => {
                        open_terminal_in_place(env, cwd.path.try_into()?)
                    },
                    PluginCommand::OpenCommandPaneInPlace(command_to_run, context) => {
                        open_command_pane_in_place(env, command_to_run, context)
                    },
                    PluginCommand::RenameSession(new_session_name) => {
                        rename_session(env, new_session_name)
                    },
                    PluginCommand::UnblockCliPipeInput(pipe_name) => {
                        unblock_cli_pipe_input(env, pipe_name)
                    },
                    PluginCommand::BlockCliPipeInput(pipe_name) => {
                        block_cli_pipe_input(env, pipe_name)
                    },
                    PluginCommand::CliPipeOutput(pipe_name, output) => {
                        cli_pipe_output(env, pipe_name, output)?
                    },
                    PluginCommand::MessageToPlugin(message) => message_to_plugin(env, message)?,
                    PluginCommand::DisconnectOtherClients => disconnect_other_clients(env),
                    PluginCommand::KillSessions(session_list) => kill_sessions(session_list),
                    PluginCommand::ScanHostFolder(folder_to_scan) => {
                        scan_host_folder(env, folder_to_scan)
                    },
                    PluginCommand::WatchFilesystem => watch_filesystem(env),
                    PluginCommand::DumpSessionLayout => dump_session_layout(env),
                    PluginCommand::CloseSelf => close_self(env),
                    PluginCommand::Reconfigure(new_config) => reconfigure(env, new_config)?,
                    PluginCommand::HidePaneWithId(pane_id) => {
                        hide_pane_with_id(env, pane_id.into())?
                    },
                    PluginCommand::ShowPaneWithId(pane_id, should_float_if_hidden) => {
                        show_pane_with_id(env, pane_id.into(), should_float_if_hidden)
                    },
                    PluginCommand::OpenCommandPaneBackground(command_to_run, context) => {
                        open_command_pane_background(env, command_to_run, context)
                    },
                },
                (PermissionStatus::Denied, permission) => {
                    log::error!(
                        "Plugin '{}' permission '{}' denied - Command '{:?}' denied",
                        env.name(),
                        permission
                            .map(|p| p.to_string())
                            .unwrap_or("UNKNOWN".to_owned()),
                        CommandType::from_str(&command.to_string()).with_context(err_context)?
                    );
                },
            };
            Ok(())
        })
        .with_context(|| format!("failed to run plugin command {}", env.name()))
        .non_fatal();
}

fn subscribe(env: &PluginEnv, event_list: HashSet<EventType>) -> Result<()> {
    env.subscriptions
        .lock()
        .to_anyhow()?
        .extend(event_list.clone());
    env.senders
        .send_to_plugin(PluginInstruction::PluginSubscribedToEvents(
            env.plugin_id,
            env.client_id,
            event_list,
        ))
}

fn unblock_cli_pipe_input(env: &PluginEnv, pipe_name: String) {
    env.input_pipes_to_unblock.lock().unwrap().insert(pipe_name);
}

fn block_cli_pipe_input(env: &PluginEnv, pipe_name: String) {
    env.input_pipes_to_block.lock().unwrap().insert(pipe_name);
}

fn cli_pipe_output(env: &PluginEnv, pipe_name: String, output: String) -> Result<()> {
    env.senders
        .send_to_server(ServerInstruction::CliPipeOutput(pipe_name, output))
        .context("failed to send pipe output")
}

fn message_to_plugin(env: &PluginEnv, mut message_to_plugin: MessageToPlugin) -> Result<()> {
    if message_to_plugin.plugin_url.as_ref().map(|s| s.as_str()) == Some("zellij:OWN_URL") {
        message_to_plugin.plugin_url = Some(env.plugin.location.display());
    }
    env.senders
        .send_to_plugin(PluginInstruction::MessageFromPlugin {
            source_plugin_id: env.plugin_id,
            message: message_to_plugin,
        })
        .context("failed to send message to plugin")
}

fn unsubscribe(env: &PluginEnv, event_list: HashSet<EventType>) -> Result<()> {
    env.subscriptions
        .lock()
        .to_anyhow()?
        .retain(|k| !event_list.contains(k));
    Ok(())
}

fn set_selectable(env: &PluginEnv, selectable: bool) {
    match env.plugin.run {
        PluginType::Pane(Some(tab_index)) => {
            // let selectable = selectable != 0;
            env.senders
                .send_to_screen(ScreenInstruction::SetSelectable(
                    PaneId::Plugin(env.plugin_id),
                    selectable,
                    tab_index,
                ))
                .with_context(|| {
                    format!(
                        "failed to set plugin {} selectable from plugin {}",
                        selectable,
                        env.name()
                    )
                })
                .non_fatal();
        },
        _ => {
            debug!(
                "{} - Calling method 'set_selectable' does nothing for headless plugins",
                env.plugin.location
            )
        },
    }
}

fn request_permission(env: &PluginEnv, permissions: Vec<PermissionType>) -> Result<()> {
    if PermissionCache::from_path_or_default(None)
        .check_permissions(env.plugin.location.to_string(), &permissions)
    {
        log::info!("PermissionRequestResult 1");
        return env
            .senders
            .send_to_plugin(PluginInstruction::PermissionRequestResult(
                env.plugin_id,
                Some(env.client_id),
                permissions.to_vec(),
                PermissionStatus::Granted,
                None,
            ));
    }

    // we do this so that messages that have arrived while the user is seeing the permission screen
    // will be cached and reapplied once the permission is granted
    let _ = env
        .senders
        .send_to_plugin(PluginInstruction::CachePluginEvents {
            plugin_id: env.plugin_id,
        });

    env.senders
        .send_to_screen(ScreenInstruction::RequestPluginPermissions(
            env.plugin_id,
            PluginPermission::new(env.plugin.location.to_string(), permissions),
        ))
}

fn get_plugin_ids(env: &PluginEnv) {
    let ids = PluginIds {
        plugin_id: env.plugin_id,
        zellij_pid: process::id(),
        initial_cwd: env.plugin_cwd.clone(),
    };
    ProtobufPluginIds::try_from(ids)
        .map_err(|e| anyhow!("Failed to serialized plugin ids: {}", e))
        .and_then(|serialized| {
            wasi_write_object(env, &serialized.encode_to_vec())?;
            Ok(())
        })
        .with_context(|| {
            format!(
                "failed to query plugin IDs from host for plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn get_zellij_version(env: &PluginEnv) {
    let protobuf_zellij_version = ProtobufZellijVersion {
        version: VERSION.to_owned(),
    };
    wasi_write_object(env, &protobuf_zellij_version.encode_to_vec())
        .with_context(|| {
            format!(
                "failed to request zellij version from host for plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn open_file(env: &PluginEnv, file_to_open: FileToOpen, context: BTreeMap<String, String>) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let floating = false;
    let in_place = false;
    let start_suppressed = false;
    let path = env.plugin_cwd.join(file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| env.plugin_cwd.join(cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let action = Action::EditFile(
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
        None,
        floating,
        in_place,
        start_suppressed,
        None,
    );
    apply_action!(action, error_msg, env);
}

fn open_file_floating(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let floating = true;
    let in_place = false;
    let start_suppressed = false;
    let path = env.plugin_cwd.join(file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| env.plugin_cwd.join(cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let action = Action::EditFile(
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
        None,
        floating,
        in_place,
        start_suppressed,
        floating_pane_coordinates,
    );
    apply_action!(action, error_msg, env);
}

fn open_file_in_place(env: &PluginEnv, file_to_open: FileToOpen, context: BTreeMap<String, String>) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let floating = false;
    let in_place = true;
    let start_suppressed = false;
    let path = env.plugin_cwd.join(file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| env.plugin_cwd.join(cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));

    let action = Action::EditFile(
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
        None,
        floating,
        in_place,
        start_suppressed,
        None,
    );
    apply_action!(action, error_msg, env);
}

fn open_terminal(env: &PluginEnv, cwd: PathBuf) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = env.plugin_cwd.join(cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewTiledPane(None, run_command_action, None);
    apply_action!(action, error_msg, env);
}

fn open_terminal_floating(
    env: &PluginEnv,
    cwd: PathBuf,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = env.plugin_cwd.join(cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewFloatingPane(run_command_action, None, floating_pane_coordinates);
    apply_action!(action, error_msg, env);
}

fn open_terminal_in_place(env: &PluginEnv, cwd: PathBuf) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = env.plugin_cwd.join(cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewInPlacePane(run_command_action, None);
    apply_action!(action, error_msg, env);
}

fn open_command_pane(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let cwd = command_to_run.cwd.map(|cwd| env.plugin_cwd.join(cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
    };
    let action = Action::NewTiledPane(direction, Some(run_command_action), name);
    apply_action!(action, error_msg, env);
}

fn open_command_pane_floating(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let cwd = command_to_run.cwd.map(|cwd| env.plugin_cwd.join(cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
    };
    let action = Action::NewFloatingPane(Some(run_command_action), name, floating_pane_coordinates);
    apply_action!(action, error_msg, env);
}

fn open_command_pane_in_place(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let cwd = command_to_run.cwd.map(|cwd| env.plugin_cwd.join(cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
    };
    let action = Action::NewInPlacePane(Some(run_command_action), name);
    apply_action!(action, error_msg, env);
}

fn open_command_pane_background(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let cwd = command_to_run.cwd.map(|cwd| env.plugin_cwd.join(cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let start_suppressed = true;
    let name = None;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            env.client_id,
            context,
        )),
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(run_cmd),
        None,
        name,
        None,
        start_suppressed,
        ClientTabIndexOrPaneId::ClientId(env.client_id),
    ));
}

fn switch_tab_to(env: &PluginEnv, tab_idx: u32) {
    env.senders
        .send_to_screen(ScreenInstruction::GoToTab(tab_idx, Some(env.client_id)))
        .with_context(|| {
            format!(
                "failed to switch to tab {tab_idx} from plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn set_timeout(env: &PluginEnv, secs: f64) {
    let send_plugin_instructions = env.senders.to_plugin.clone();
    let update_target = Some(env.plugin_id);
    let client_id = env.client_id;
    let plugin_name = env.name();
    task::spawn(async move {
        let start_time = Instant::now();
        task::sleep(Duration::from_secs_f64(secs)).await;
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
            .ok_or(anyhow!("found no sender to send plugin instruction to"))
            .and_then(|sender| {
                sender
                    .send(PluginInstruction::Update(vec![(
                        update_target,
                        Some(client_id),
                        Event::Timer(elapsed_time),
                    )]))
                    .to_anyhow()
            })
            .with_context(|| {
                format!(
                    "failed to set host timeout of {secs} s for plugin {}",
                    plugin_name
                )
            })
            .non_fatal();
    });
}

fn exec_cmd(env: &PluginEnv, mut command_line: Vec<String>) {
    log::warn!("The ExecCmd plugin command is deprecated and will be removed in a future version. Please use RunCmd instead (it has all the things and can even show you STDOUT/STDERR and an exit code!)");
    let err_context = || {
        format!(
            "failed to execute command on host for plugin '{}'",
            env.name()
        )
    };
    let command = command_line.remove(0);

    // Bail out if we're forbidden to run command
    if !env.plugin._allow_exec_host_cmd {
        warn!("This plugin isn't allow to run command in host side, skip running this command: '{cmd} {args}'.",
        	cmd = command, args = command_line.join(" "));
        return;
    }

    // Here, we don't wait the command to finish
    process::Command::new(command)
        .args(command_line)
        .spawn()
        .with_context(err_context)
        .non_fatal();
}

fn run_command(
    env: &PluginEnv,
    mut command_line: Vec<String>,
    env_variables: BTreeMap<String, String>,
    cwd: PathBuf,
    context: BTreeMap<String, String>,
) {
    if command_line.is_empty() {
        log::error!("Command cannot be empty");
    } else {
        let command = command_line.remove(0);
        let cwd = env.plugin_cwd.join(cwd);
        let _ = env
            .senders
            .send_to_background_jobs(BackgroundJob::RunCommand(
                env.plugin_id,
                env.client_id,
                command,
                command_line,
                env_variables,
                cwd,
                context,
            ));
    }
}

fn web_request(
    env: &PluginEnv,
    url: String,
    verb: HttpVerb,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    context: BTreeMap<String, String>,
) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::WebRequest(
            env.plugin_id,
            env.client_id,
            url,
            verb,
            headers,
            body,
            context,
        ));
}

fn post_message_to(env: &PluginEnv, plugin_message: PluginMessage) -> Result<()> {
    let worker_name = plugin_message
        .worker_name
        .ok_or(anyhow!("Worker name not specified in message to worker"))?;
    env.senders
        .send_to_plugin(PluginInstruction::PostMessagesToPluginWorker(
            env.plugin_id,
            env.client_id,
            worker_name,
            vec![(plugin_message.name, plugin_message.payload)],
        ))
}

fn post_message_to_plugin(env: &PluginEnv, plugin_message: PluginMessage) -> Result<()> {
    if let Some(worker_name) = plugin_message.worker_name {
        return Err(anyhow!(
            "Worker name (\"{}\") should not be specified in message to plugin",
            worker_name
        ));
    }
    env.senders
        .send_to_plugin(PluginInstruction::PostMessageToPlugin(
            env.plugin_id,
            env.client_id,
            plugin_message.name,
            plugin_message.payload,
        ))
}

fn hide_self(env: &PluginEnv) -> Result<()> {
    env.senders
        .send_to_screen(ScreenInstruction::SuppressPane(
            PaneId::Plugin(env.plugin_id),
            env.client_id,
        ))
        .with_context(|| format!("failed to hide self"))
}

fn hide_pane_with_id(env: &PluginEnv, pane_id: PaneId) -> Result<()> {
    env.senders
        .send_to_screen(ScreenInstruction::SuppressPane(pane_id, env.client_id))
        .with_context(|| format!("failed to hide self"))
}

fn show_self(env: &PluginEnv, should_float_if_hidden: bool) {
    let action = Action::FocusPluginPaneWithId(env.plugin_id, should_float_if_hidden);
    let error_msg = || format!("Failed to show self for plugin");
    apply_action!(action, error_msg, env);
}

fn show_pane_with_id(env: &PluginEnv, pane_id: PaneId, should_float_if_hidden: bool) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::FocusPaneWithId(
            pane_id,
            should_float_if_hidden,
            env.client_id,
        ));
}

fn close_self(env: &PluginEnv) {
    env.senders
        .send_to_screen(ScreenInstruction::ClosePane(
            PaneId::Plugin(env.plugin_id),
            None,
        ))
        .with_context(|| format!("failed to close self"))
        .non_fatal();
    env.senders
        .send_to_plugin(PluginInstruction::Unload(env.plugin_id))
        .with_context(|| format!("failed to close self"))
        .non_fatal();
}

fn reconfigure(env: &PluginEnv, new_config: String) -> Result<()> {
    let err_context = || "Failed to reconfigure";
    let client_id = env.client_id;
    env.senders
        .send_to_server(ServerInstruction::Reconfigure(client_id, new_config))
        .with_context(err_context)?;
    Ok(())
}

fn switch_to_mode(env: &PluginEnv, input_mode: InputMode) {
    let action = Action::SwitchToMode(input_mode);
    let error_msg = || format!("failed to switch to mode in plugin {}", env.name());
    apply_action!(action, error_msg, env);
}

fn new_tabs_with_layout(env: &PluginEnv, raw_layout: &str) -> Result<()> {
    // TODO: cwd
    let layout = Layout::from_str(
        &raw_layout,
        format!("Layout from plugin: {}", env.name()),
        None,
        None,
    )
    .map_err(|e| anyhow!("Failed to parse layout: {:?}", e))?;
    apply_layout(env, layout);
    Ok(())
}

fn new_tabs_with_layout_info(env: &PluginEnv, layout_info: LayoutInfo) -> Result<()> {
    // TODO: cwd
    let layout = Layout::from_layout_info(&env.layout_dir, layout_info)
        .map_err(|e| anyhow!("Failed to parse layout: {:?}", e))?;
    apply_layout(env, layout);
    Ok(())
}

fn apply_layout(env: &PluginEnv, layout: Layout) {
    let mut tabs_to_open = vec![];
    let tabs = layout.tabs();
    if tabs.is_empty() {
        let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
        let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
        let action = Action::NewTab(
            layout.template.as_ref().map(|t| t.0.clone()),
            layout.template.map(|t| t.1).unwrap_or_default(),
            swap_tiled_layouts,
            swap_floating_layouts,
            None,
        );
        tabs_to_open.push(action);
    } else {
        for (tab_name, tiled_pane_layout, floating_pane_layout) in layout.tabs() {
            let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
            let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
            let action = Action::NewTab(
                Some(tiled_pane_layout),
                floating_pane_layout,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
            );
            tabs_to_open.push(action);
        }
    }
    for action in tabs_to_open {
        let error_msg = || format!("Failed to create layout tab");
        apply_action!(action, error_msg, env);
    }
}

fn new_tab(env: &PluginEnv) {
    let action = Action::NewTab(None, vec![], None, None, None);
    let error_msg = || format!("Failed to open new tab");
    apply_action!(action, error_msg, env);
}

fn go_to_next_tab(env: &PluginEnv) {
    let action = Action::GoToNextTab;
    let error_msg = || format!("Failed to go to next tab");
    apply_action!(action, error_msg, env);
}

fn go_to_previous_tab(env: &PluginEnv) {
    let action = Action::GoToPreviousTab;
    let error_msg = || format!("Failed to go to previous tab");
    apply_action!(action, error_msg, env);
}

fn resize(env: &PluginEnv, resize: Resize) {
    let error_msg = || format!("failed to resize in plugin {}", env.name());
    let action = Action::Resize(resize, None);
    apply_action!(action, error_msg, env);
}

fn resize_with_direction(env: &PluginEnv, resize: ResizeStrategy) {
    let error_msg = || format!("failed to resize in plugin {}", env.name());
    let action = Action::Resize(resize.resize, resize.direction);
    apply_action!(action, error_msg, env);
}

fn focus_next_pane(env: &PluginEnv) {
    let action = Action::FocusNextPane;
    let error_msg = || format!("Failed to focus next pane");
    apply_action!(action, error_msg, env);
}

fn focus_previous_pane(env: &PluginEnv) {
    let action = Action::FocusPreviousPane;
    let error_msg = || format!("Failed to focus previous pane");
    apply_action!(action, error_msg, env);
}

fn move_focus(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move focus in plugin {}", env.name());
    let action = Action::MoveFocus(direction);
    apply_action!(action, error_msg, env);
}

fn move_focus_or_tab(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move focus in plugin {}", env.name());
    let action = Action::MoveFocusOrTab(direction);
    apply_action!(action, error_msg, env);
}

fn detach(env: &PluginEnv) {
    let action = Action::Detach;
    let error_msg = || format!("Failed to detach");
    apply_action!(action, error_msg, env);
}

fn switch_session(
    env: &PluginEnv,
    session_name: Option<String>,
    tab_position: Option<usize>,
    pane_id: Option<(u32, bool)>,
    layout: Option<LayoutInfo>,
    cwd: Option<PathBuf>,
) -> Result<()> {
    // pane_id is (id, is_plugin)
    let err_context = || format!("Failed to switch session");
    if let Some(LayoutInfo::Stringified(stringified_layout)) = layout.as_ref() {
        // we verify the stringified layout here to fail early rather than when parsing it at the
        // session-switching phase
        if let Err(e) = Layout::from_kdl(&stringified_layout, None, None, None) {
            return Err(anyhow!("Failed to deserialize layout: {}", e));
        }
    }
    let client_id = env.client_id;
    let tab_position = tab_position.map(|p| p + 1); // ¯\_()_/¯
    let connect_to_session = ConnectToSession {
        name: session_name,
        tab_position,
        pane_id,
        layout,
        cwd,
    };
    env.senders
        .send_to_server(ServerInstruction::SwitchSession(
            connect_to_session,
            client_id,
        ))
        .with_context(err_context)?;
    Ok(())
}

fn delete_dead_session(session_name: String) -> Result<()> {
    std::fs::remove_dir_all(&*ZELLIJ_SESSION_INFO_CACHE_DIR.join(&session_name))
        .with_context(|| format!("Failed to delete dead session: {:?}", &session_name))
}

fn delete_all_dead_sessions() -> Result<()> {
    use std::os::unix::fs::FileTypeExt;
    let mut live_sessions = vec![];
    if let Ok(files) = std::fs::read_dir(&*ZELLIJ_SOCK_DIR) {
        files.for_each(|file| {
            if let Ok(file) = file {
                if let Ok(file_name) = file.file_name().into_string() {
                    if file.file_type().unwrap().is_socket() {
                        live_sessions.push(file_name);
                    }
                }
            }
        });
    }
    let dead_sessions: Vec<String> = match std::fs::read_dir(&*ZELLIJ_SESSION_INFO_CACHE_DIR) {
        Ok(files_in_session_info_folder) => {
            let files_that_are_folders = files_in_session_info_folder
                .filter_map(|f| f.ok().map(|f| f.path()))
                .filter(|f| f.is_dir());
            files_that_are_folders
                .filter_map(|folder_name| {
                    let session_name = folder_name.file_name()?.to_str()?.to_owned();
                    if live_sessions.contains(&session_name) {
                        // this is not a dead session...
                        return None;
                    }
                    Some(session_name)
                })
                .collect()
        },
        Err(e) => {
            log::error!("Failed to read session info cache dir: {:?}", e);
            vec![]
        },
    };
    for session in dead_sessions {
        delete_dead_session(session)?;
    }
    Ok(())
}

fn edit_scrollback(env: &PluginEnv) {
    let action = Action::EditScrollback;
    let error_msg = || format!("Failed to edit scrollback");
    apply_action!(action, error_msg, env);
}

fn write(env: &PluginEnv, bytes: Vec<u8>) {
    let error_msg = || format!("failed to write in plugin {}", env.name());
    let action = Action::Write(None, bytes, false);
    apply_action!(action, error_msg, env);
}

fn write_chars(env: &PluginEnv, chars_to_write: String) {
    let error_msg = || format!("failed to write in plugin {}", env.name());
    let action = Action::WriteChars(chars_to_write);
    apply_action!(action, error_msg, env);
}

fn toggle_tab(env: &PluginEnv) {
    let error_msg = || format!("Failed to toggle tab");
    let action = Action::ToggleTab;
    apply_action!(action, error_msg, env);
}

fn move_pane(env: &PluginEnv) {
    let error_msg = || format!("failed to move pane in plugin {}", env.name());
    let action = Action::MovePane(None);
    apply_action!(action, error_msg, env);
}

fn move_pane_with_direction(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move pane in plugin {}", env.name());
    let action = Action::MovePane(Some(direction));
    apply_action!(action, error_msg, env);
}

fn clear_screen(env: &PluginEnv) {
    let error_msg = || format!("failed to clear screen in plugin {}", env.name());
    let action = Action::ClearScreen;
    apply_action!(action, error_msg, env);
}
fn scroll_up(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll up in plugin {}", env.name());
    let action = Action::ScrollUp;
    apply_action!(action, error_msg, env);
}

fn scroll_down(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll down in plugin {}", env.name());
    let action = Action::ScrollDown;
    apply_action!(action, error_msg, env);
}

fn scroll_to_top(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll in plugin {}", env.name());
    let action = Action::ScrollToTop;
    apply_action!(action, error_msg, env);
}

fn scroll_to_bottom(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll in plugin {}", env.name());
    let action = Action::ScrollToBottom;
    apply_action!(action, error_msg, env);
}

fn page_scroll_up(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll in plugin {}", env.name());
    let action = Action::PageScrollUp;
    apply_action!(action, error_msg, env);
}

fn page_scroll_down(env: &PluginEnv) {
    let error_msg = || format!("failed to scroll in plugin {}", env.name());
    let action = Action::PageScrollDown;
    apply_action!(action, error_msg, env);
}

fn toggle_focus_fullscreen(env: &PluginEnv) {
    let error_msg = || format!("failed to toggle full screen in plugin {}", env.name());
    let action = Action::ToggleFocusFullscreen;
    apply_action!(action, error_msg, env);
}

fn toggle_pane_frames(env: &PluginEnv) {
    let error_msg = || format!("failed to toggle full screen in plugin {}", env.name());
    let action = Action::TogglePaneFrames;
    apply_action!(action, error_msg, env);
}

fn toggle_pane_embed_or_eject(env: &PluginEnv) {
    let error_msg = || {
        format!(
            "failed to toggle pane embed or eject in plugin {}",
            env.name()
        )
    };
    let action = Action::TogglePaneEmbedOrFloating;
    apply_action!(action, error_msg, env);
}

fn undo_rename_pane(env: &PluginEnv) {
    let error_msg = || format!("failed to undo rename pane in plugin {}", env.name());
    let action = Action::UndoRenamePane;
    apply_action!(action, error_msg, env);
}

fn close_focus(env: &PluginEnv) {
    let error_msg = || format!("failed to close focused pane in plugin {}", env.name());
    let action = Action::CloseFocus;
    apply_action!(action, error_msg, env);
}

fn toggle_active_tab_sync(env: &PluginEnv) {
    let error_msg = || format!("failed to toggle active tab sync in plugin {}", env.name());
    let action = Action::ToggleActiveSyncTab;
    apply_action!(action, error_msg, env);
}

fn close_focused_tab(env: &PluginEnv) {
    let error_msg = || format!("failed to close active tab in plugin {}", env.name());
    let action = Action::CloseTab;
    apply_action!(action, error_msg, env);
}

fn undo_rename_tab(env: &PluginEnv) {
    let error_msg = || format!("failed to undo rename tab in plugin {}", env.name());
    let action = Action::UndoRenameTab;
    apply_action!(action, error_msg, env);
}

fn quit_zellij(env: &PluginEnv) {
    let error_msg = || format!("failed to quit zellij in plugin {}", env.name());
    let action = Action::Quit;
    apply_action!(action, error_msg, env);
}

fn previous_swap_layout(env: &PluginEnv) {
    let error_msg = || format!("failed to switch swap layout in plugin {}", env.name());
    let action = Action::PreviousSwapLayout;
    apply_action!(action, error_msg, env);
}

fn next_swap_layout(env: &PluginEnv) {
    let error_msg = || format!("failed to switch swap layout in plugin {}", env.name());
    let action = Action::NextSwapLayout;
    apply_action!(action, error_msg, env);
}

fn go_to_tab_name(env: &PluginEnv, tab_name: String) {
    let error_msg = || format!("failed to change tab in plugin {}", env.name());
    let create = false;
    let action = Action::GoToTabName(tab_name, create);
    apply_action!(action, error_msg, env);
}

fn focus_or_create_tab(env: &PluginEnv, tab_name: String) {
    let error_msg = || format!("failed to change or create tab in plugin {}", env.name());
    let create = true;
    let action = Action::GoToTabName(tab_name, create);
    apply_action!(action, error_msg, env);
}

fn go_to_tab(env: &PluginEnv, tab_index: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::GoToTab(tab_index + 1);
    apply_action!(action, error_msg, env);
}

fn start_or_reload_plugin(env: &PluginEnv, url: &str) -> Result<()> {
    let error_msg = || format!("failed to start or reload plugin in plugin {}", env.name());
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let run_plugin_or_alias = RunPluginOrAlias::from_url(url, &None, None, Some(cwd))
        .map_err(|e| anyhow!("Failed to parse plugin location: {}", e))?;
    let action = Action::StartOrReloadPlugin(run_plugin_or_alias);
    apply_action!(action, error_msg, env);
    Ok(())
}

fn close_terminal_pane(env: &PluginEnv, terminal_pane_id: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::CloseTerminalPane(terminal_pane_id);
    apply_action!(action, error_msg, env);
}

fn close_plugin_pane(env: &PluginEnv, plugin_pane_id: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::ClosePluginPane(plugin_pane_id);
    apply_action!(action, error_msg, env);
}

fn focus_terminal_pane(env: &PluginEnv, terminal_pane_id: u32, should_float_if_hidden: bool) {
    let action = Action::FocusTerminalPaneWithId(terminal_pane_id, should_float_if_hidden);
    let error_msg = || format!("Failed to focus terminal pane");
    apply_action!(action, error_msg, env);
}

fn focus_plugin_pane(env: &PluginEnv, plugin_pane_id: u32, should_float_if_hidden: bool) {
    let action = Action::FocusPluginPaneWithId(plugin_pane_id, should_float_if_hidden);
    let error_msg = || format!("Failed to focus plugin pane");
    apply_action!(action, error_msg, env);
}

fn rename_terminal_pane(env: &PluginEnv, terminal_pane_id: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename terminal pane");
    let rename_pane_action =
        Action::RenameTerminalPane(terminal_pane_id, new_name.as_bytes().to_vec());
    apply_action!(rename_pane_action, error_msg, env);
}

fn rename_plugin_pane(env: &PluginEnv, plugin_pane_id: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename plugin pane");
    let rename_pane_action = Action::RenamePluginPane(plugin_pane_id, new_name.as_bytes().to_vec());
    apply_action!(rename_pane_action, error_msg, env);
}

fn rename_tab(env: &PluginEnv, tab_index: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename tab");
    let rename_tab_action = Action::RenameTab(tab_index, new_name.as_bytes().to_vec());
    apply_action!(rename_tab_action, error_msg, env);
}

fn rename_session(env: &PluginEnv, new_session_name: String) {
    let error_msg = || format!("failed to rename session in plugin {}", env.name());
    let action = Action::RenameSession(new_session_name);
    apply_action!(action, error_msg, env);
}

fn disconnect_other_clients(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_server(ServerInstruction::DisconnectAllClientsExcept(env.client_id))
        .context("failed to send disconnect other clients instruction");
}

fn kill_sessions(session_names: Vec<String>) {
    for session_name in session_names {
        let path = &*ZELLIJ_SOCK_DIR.join(&session_name);
        match LocalSocketStream::connect(path) {
            Ok(stream) => {
                let _ = IpcSenderWithContext::new(stream).send(ClientToServerMsg::KillSession);
            },
            Err(e) => {
                log::error!("Failed to kill session {}: {:?}", session_name, e);
            },
        };
    }
}

fn watch_filesystem(env: &PluginEnv) {
    let _ = env
        .senders
        .to_plugin
        .as_ref()
        .map(|sender| sender.send(PluginInstruction::WatchFilesystem));
}

fn dump_session_layout(env: &PluginEnv) {
    let _ = env
        .senders
        .to_screen
        .as_ref()
        .map(|sender| sender.send(ScreenInstruction::DumpLayoutToPlugin(env.plugin_id)));
}

fn scan_host_folder(env: &PluginEnv, folder_to_scan: PathBuf) {
    if !folder_to_scan.starts_with("/host") {
        log::error!(
            "Can only scan files in the /host filesystem, found: {}",
            folder_to_scan.display()
        );
        return;
    }
    let plugin_host_folder = env.plugin_cwd.clone();
    let folder_to_scan = plugin_host_folder.join(folder_to_scan.strip_prefix("/host").unwrap());
    match folder_to_scan.canonicalize() {
        Ok(folder_to_scan) => {
            if !folder_to_scan.starts_with(&plugin_host_folder) {
                log::error!(
                    "Can only scan files in the plugin filesystem: {}, found: {}",
                    plugin_host_folder.display(),
                    folder_to_scan.display()
                );
                return;
            }
            let reading_folder = std::fs::read_dir(&folder_to_scan);
            match reading_folder {
                Ok(reading_folder) => {
                    let send_plugin_instructions = env.senders.to_plugin.clone();
                    let update_target = Some(env.plugin_id);
                    let client_id = env.client_id;
                    thread::spawn({
                        move || {
                            let mut paths_in_folder = vec![];
                            for entry in reading_folder {
                                if let Ok(entry) = entry {
                                    let entry_metadata = entry.metadata().ok().map(|m| m.into());
                                    paths_in_folder.push((
                                        PathBuf::from("/host").join(
                                            entry.path().strip_prefix(&plugin_host_folder).unwrap(),
                                        ),
                                        entry_metadata.into(),
                                    ));
                                }
                            }
                            let _ = send_plugin_instructions
                                .ok_or(anyhow!("found no sender to send plugin instruction to"))
                                .map(|sender| {
                                    let _ = sender.send(PluginInstruction::Update(vec![(
                                        update_target,
                                        Some(client_id),
                                        Event::FileSystemUpdate(paths_in_folder),
                                    )]));
                                })
                                .non_fatal();
                        }
                    });
                },
                Err(e) => {
                    log::error!("Failed to read folder {}: {e}", folder_to_scan.display());
                },
            }
        },
        Err(e) => {
            log::error!(
                "Failed to canonicalize path {folder_to_scan:?} when scanning folder: {:?}",
                e
            );
        },
    }
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn report_panic(env: &PluginEnv, msg: &str) {
    log::error!("PANIC IN PLUGIN!\n\r{}", msg);
    handle_plugin_crash(env.plugin_id, msg.to_owned(), env.senders.clone());
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(plugin_env: &PluginEnv) -> Result<String> {
    let err_context = || format!("failed to read string from WASI env");

    let mut buf = vec![];
    plugin_env
        .stdout_pipe
        .lock()
        .unwrap()
        .read_to_end(&mut buf)
        .map_err(anyError::new)
        .with_context(err_context)?;
    let buf = String::from_utf8_lossy(&buf);

    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    Ok(buf.replace("\n", "\n\r"))
}

pub fn wasi_write_string(plugin_env: &PluginEnv, buf: &str) -> Result<()> {
    let mut stdin = plugin_env.stdin_pipe.lock().unwrap();
    writeln!(stdin, "{}\r", buf)
        .map_err(anyError::new)
        .with_context(|| format!("failed to write string to WASI env"))
}

pub fn wasi_write_object(plugin_env: &PluginEnv, object: &(impl Serialize + ?Sized)) -> Result<()> {
    serde_json::to_string(&object)
        .map_err(anyError::new)
        .and_then(|string| wasi_write_string(plugin_env, &string))
        .with_context(|| format!("failed to serialize object for WASI env"))
}

pub fn wasi_read_bytes(plugin_env: &PluginEnv) -> Result<Vec<u8>> {
    wasi_read_string(plugin_env)
        .and_then(|string| serde_json::from_str(&string).map_err(anyError::new))
        .with_context(|| format!("failed to deserialize object from WASI env"))
}

// TODO: move to permissions?
fn check_command_permission(
    plugin_env: &PluginEnv,
    command: &PluginCommand,
) -> (PermissionStatus, Option<PermissionType>) {
    if plugin_env.plugin.is_builtin() {
        // built-in plugins can do all the things because they're part of the application and
        // there's no use to deny them anything
        return (PermissionStatus::Granted, None);
    }
    let permission = match command {
        PluginCommand::OpenFile(..)
        | PluginCommand::OpenFileFloating(..)
        | PluginCommand::OpenFileInPlace(..) => PermissionType::OpenFiles,
        PluginCommand::OpenTerminal(..)
        | PluginCommand::StartOrReloadPlugin(..)
        | PluginCommand::OpenTerminalFloating(..)
        | PluginCommand::OpenTerminalInPlace(..) => PermissionType::OpenTerminalsOrPlugins,
        PluginCommand::OpenCommandPane(..)
        | PluginCommand::OpenCommandPaneFloating(..)
        | PluginCommand::OpenCommandPaneInPlace(..)
        | PluginCommand::OpenCommandPaneBackground(..)
        | PluginCommand::RunCommand(..)
        | PluginCommand::ExecCmd(..) => PermissionType::RunCommands,
        PluginCommand::WebRequest(..) => PermissionType::WebAccess,
        PluginCommand::Write(..) | PluginCommand::WriteChars(..) => PermissionType::WriteToStdin,
        PluginCommand::SwitchTabTo(..)
        | PluginCommand::SwitchToMode(..)
        | PluginCommand::NewTabsWithLayout(..)
        | PluginCommand::NewTabsWithLayoutInfo(..)
        | PluginCommand::NewTab
        | PluginCommand::GoToNextTab
        | PluginCommand::GoToPreviousTab
        | PluginCommand::Resize(..)
        | PluginCommand::ResizeWithDirection(..)
        | PluginCommand::FocusNextPane
        | PluginCommand::MoveFocus(..)
        | PluginCommand::MoveFocusOrTab(..)
        | PluginCommand::Detach
        | PluginCommand::EditScrollback
        | PluginCommand::ToggleTab
        | PluginCommand::MovePane
        | PluginCommand::MovePaneWithDirection(..)
        | PluginCommand::ClearScreen
        | PluginCommand::ScrollUp
        | PluginCommand::ScrollDown
        | PluginCommand::ScrollToTop
        | PluginCommand::ScrollToBottom
        | PluginCommand::PageScrollUp
        | PluginCommand::PageScrollDown
        | PluginCommand::ToggleFocusFullscreen
        | PluginCommand::TogglePaneFrames
        | PluginCommand::TogglePaneEmbedOrEject
        | PluginCommand::UndoRenamePane
        | PluginCommand::CloseFocus
        | PluginCommand::ToggleActiveTabSync
        | PluginCommand::CloseFocusedTab
        | PluginCommand::UndoRenameTab
        | PluginCommand::QuitZellij
        | PluginCommand::PreviousSwapLayout
        | PluginCommand::NextSwapLayout
        | PluginCommand::GoToTabName(..)
        | PluginCommand::FocusOrCreateTab(..)
        | PluginCommand::GoToTab(..)
        | PluginCommand::CloseTerminalPane(..)
        | PluginCommand::ClosePluginPane(..)
        | PluginCommand::FocusTerminalPane(..)
        | PluginCommand::FocusPluginPane(..)
        | PluginCommand::RenameTerminalPane(..)
        | PluginCommand::RenamePluginPane(..)
        | PluginCommand::SwitchSession(..)
        | PluginCommand::DeleteDeadSession(..)
        | PluginCommand::DeleteAllDeadSessions
        | PluginCommand::RenameSession(..)
        | PluginCommand::RenameTab(..)
        | PluginCommand::DisconnectOtherClients
        | PluginCommand::ShowPaneWithId(..)
        | PluginCommand::HidePaneWithId(..)
        | PluginCommand::KillSessions(..) => PermissionType::ChangeApplicationState,
        PluginCommand::UnblockCliPipeInput(..)
        | PluginCommand::BlockCliPipeInput(..)
        | PluginCommand::CliPipeOutput(..) => PermissionType::ReadCliPipes,
        PluginCommand::MessageToPlugin(..) => PermissionType::MessageAndLaunchOtherPlugins,
        PluginCommand::DumpSessionLayout => PermissionType::ReadApplicationState,
        PluginCommand::Reconfigure(..) => PermissionType::Reconfigure,
        _ => return (PermissionStatus::Granted, None),
    };

    if let Some(permissions) = plugin_env.permissions.lock().unwrap().as_ref() {
        if permissions.contains(&permission) {
            return (PermissionStatus::Granted, None);
        }
    }

    (PermissionStatus::Denied, Some(permission))
}
