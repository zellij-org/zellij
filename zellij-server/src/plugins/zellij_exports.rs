use super::PluginInstruction;
use crate::background_jobs::BackgroundJob;
use crate::global_async_runtime::get_tokio_runtime;
use crate::plugins::plugin_map::{next_permission_prompt_id, PluginEnv};
use crate::plugins::wasm_bridge::{
    handle_plugin_crash, redact_session_info_for_plugin, release_admitted_worker_posts,
    try_admit_plugin_worker_post,
};
use crate::plugins::{
    try_acquire_refusal_report, try_acquire_run_command, try_acquire_web_request,
    BackgroundOperationReservation, PluginId, PromptCaller,
};
use crate::pty::{ClientTabIndexOrPaneId, PtyInstruction};
use crate::route::{route_action, wait_for_action_completion, NotificationEnd};
use crate::session_suggestions::SessionSuggestionsJob;
use crate::ClientId;
use crate::ServerInstruction;
use log::warn;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashSet},
    io::Read,
    path::{Path, PathBuf},
    process,
    str::FromStr,
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use tokio::sync::oneshot;
use wasmi::{Caller, Linker};
use zellij_utils::data::{
    BorderStyleOverride, BreakPanesToNewTabResponse, BreakPanesToTabWithIdResponse,
    BreakPanesToTabWithIndexResponse, CommandType, ConfigSnapshot, ConnectToSession,
    ContextMenuTarget, DeleteAllDeadSessionsResponse, DeleteDeadSessionResponse,
    DeleteLayoutResponse, EditLayoutResponse, Event, FloatingPaneCoordinates,
    FocusOrCreateTabResponse, GetFocusedPaneInfoResponse, GetPaneCwdResponse, GetPanePidResponse,
    GetPaneRunningCommandResponse, HttpVerb, KeyWithModifier, KillSessionsResponse, LayoutInfo,
    LayoutMetadata, LayoutParsingError, MessageToPlugin, MouseTrigger, NewPanePlacement,
    NewTabResponse, NewTabUnfocusedResponse, NewTiledPaneInTabResponse,
    OpenCommandPaneBackgroundResponse, OpenCommandPaneFloatingNearPluginResponse,
    OpenCommandPaneFloatingResponse, OpenCommandPaneInPlaceOfPaneIdResponse,
    OpenCommandPaneInPlaceOfPluginResponse, OpenCommandPaneInPlaceResponse,
    OpenCommandPaneNearPluginResponse, OpenCommandPaneResponse,
    OpenEditPaneInPlaceOfPaneIdResponse, OpenFileFloatingNearPluginResponse,
    OpenFileFloatingResponse, OpenFileInPlaceOfPluginResponse, OpenFileInPlaceResponse,
    OpenFileNearPluginResponse, OpenFileResponse, OpenPaneInNewTabResponse,
    OpenPluginPaneFloatingResponse, OpenTerminalFloatingNearPluginResponse,
    OpenTerminalFloatingResponse, OpenTerminalInPlaceOfPluginResponse, OpenTerminalInPlaceResponse,
    OpenTerminalNearPluginResponse, OpenTerminalPaneInPlaceOfPaneIdResponse, OpenTerminalResponse,
    OriginatingPlugin, PaneFrameStyle, PaneScrollbackResponse, PermissionStatus, PermissionType,
    PluginPermission, PopupOptions, RegexHighlight, RenameLayoutResponse, SaveLayoutResponse,
    SettingKey, TabMetadata,
};
use zellij_utils::home::default_layout_dir;
use zellij_utils::input::permission::PermissionCache;
use zellij_utils::sessions::generate_random_name as generate_random_name_impl;
use zellij_utils::sessions::validate_session_folder_name;
#[cfg(feature = "web_server_capability")]
use zellij_utils::web_authentication_tokens::{
    create_token, list_tokens, rename_token, revoke_all_tokens, revoke_token,
};
#[cfg(feature = "web_server_capability")]
use zellij_utils::web_server_commands::shutdown_all_webserver_instances;

use crate::{
    panes::PaneId,
    screen::{ClosePaneSource, ScreenInstruction},
};
use kdl::KdlDocument;

use prost::Message;
use zellij_utils::{
    consts::{VERSION, ZELLIJ_SESSION_INFO_CACHE_DIR, ZELLIJ_SOCK_DIR, ZELLIJ_TMP_DIR},
    data::{
        CommandOrPlugin, CommandToRun, Direction, EventType, FileToOpen, InputMode,
        NestedSessionKeybindsError, PluginCommand, PluginIds, PluginMessage, Resize,
        ResizeStrategy,
    },
    errors::prelude::*,
    input::{
        actions::Action,
        command::{OpenFilePayload, RunCommand, RunCommandAction, TerminalAction},
        config::ConfigError,
        layout::{Layout, RunPluginLocation, RunPluginOrAlias, TabLayoutInfo},
    },
    plugin_api::{
        event::{
            layout_parsing_error::ErrorType as ProtobufLayoutParsingErrorType,
            nested_session_keybinds_response_to_protobuf, ProtobufLayoutParsingError,
            ProtobufPaneScrollbackResponse, ProtobufSyntaxError,
        },
        plugin_command::{
            dump_layout_response, dump_session_layout_response, hide_floating_panes_response,
            parse_layout_response, save_session_response, show_floating_panes_response,
            ProtobufBreakPanesToNewTabResponse, ProtobufBreakPanesToTabWithIdResponse,
            ProtobufBreakPanesToTabWithIndexResponse, ProtobufCopyKeybindPresetResponse,
            ProtobufDeleteAllDeadSessionsResponse, ProtobufDeleteDeadSessionResponse,
            ProtobufDeleteLayoutResponse, ProtobufDumpLayoutResponse,
            ProtobufDumpSessionLayoutResponse, ProtobufEditLayoutResponse,
            ProtobufFocusOrCreateTabResponse, ProtobufGenerateRandomNameResponse,
            ProtobufGetFocusedPaneInfoResponse, ProtobufGetLayoutDirResponse,
            ProtobufGetPaneCwdResponse, ProtobufGetPaneInfoResponse, ProtobufGetPanePidResponse,
            ProtobufGetPaneRunningCommandResponse, ProtobufGetSessionEnvironmentVariablesResponse,
            ProtobufGetSessionListResponse, ProtobufGetTabInfoResponse,
            ProtobufHideFloatingPanesResponse, ProtobufKillSessionsResponse,
            ProtobufNewTabResponse, ProtobufNewTabUnfocusedResponse, ProtobufNewTabsResponse,
            ProtobufNewTiledPaneInTabResponse, ProtobufOpenCommandPaneBackgroundResponse,
            ProtobufOpenCommandPaneFloatingNearPluginResponse,
            ProtobufOpenCommandPaneFloatingResponse,
            ProtobufOpenCommandPaneInPlaceOfPaneIdResponse,
            ProtobufOpenCommandPaneInPlaceOfPluginResponse, ProtobufOpenCommandPaneInPlaceResponse,
            ProtobufOpenCommandPaneNearPluginResponse, ProtobufOpenCommandPaneResponse,
            ProtobufOpenEditPaneInPlaceOfPaneIdResponse,
            ProtobufOpenFileFloatingNearPluginResponse, ProtobufOpenFileFloatingResponse,
            ProtobufOpenFileInPlaceOfPluginResponse, ProtobufOpenFileInPlaceResponse,
            ProtobufOpenFileNearPluginResponse, ProtobufOpenFileResponse,
            ProtobufOpenPaneInNewTabResponse, ProtobufOpenPluginPaneFloatingResponse,
            ProtobufOpenTerminalFloatingNearPluginResponse, ProtobufOpenTerminalFloatingResponse,
            ProtobufOpenTerminalInPlaceOfPluginResponse, ProtobufOpenTerminalInPlaceResponse,
            ProtobufOpenTerminalNearPluginResponse,
            ProtobufOpenTerminalPaneInPlaceOfPaneIdResponse, ProtobufOpenTerminalResponse,
            ProtobufParseLayoutResponse, ProtobufPluginCommand, ProtobufReadConfigResponse,
            ProtobufRenameLayoutResponse, ProtobufSaveLayoutResponse, ProtobufSaveSessionResponse,
            ProtobufShowFloatingPanesResponse, ProtobufSlotCommandResponse,
            ProtobufWriteThemeFileResponse,
        },
        plugin_ids::{ProtobufPluginIds, ProtobufZellijVersion},
    },
    prompt::{PromptRequest, PromptResult},
};

#[cfg(feature = "web_server_capability")]
use zellij_utils::plugin_api::plugin_command::{
    CreateTokenResponse, ListTokensResponse, RenameWebTokenResponse, RevokeAllWebTokensResponse,
    RevokeTokenResponse,
};

macro_rules! apply_action {
    ($action:ident, $error_message:ident, $env: ident) => {
        match {
            let _blocking_call_guard = BlockingCallGuard::new(&$env);
            route_action(
                $action,
                acting_client(&$env),
                None,
                source_pane(&$env),
                $env.senders.clone(),
                $env.default_shell.clone(),
                None,
                $env.default_mode.clone(),
                None,
                false,
            )
        } {
            Ok((_, result)) => result,
            Err(e) => {
                log::error!("{}: {:?}", $error_message(), e);
                None
            },
        }
    };
}

struct BlockingCallGuard {
    in_blocking_call: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
}

impl BlockingCallGuard {
    fn new(env: &PluginEnv) -> Self {
        let in_blocking_call = env
            .shared
            .as_ref()
            .map(|shared| shared.in_blocking_call.clone());
        if let Some(in_blocking_call) = &in_blocking_call {
            in_blocking_call.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        BlockingCallGuard { in_blocking_call }
    }
}

impl Drop for BlockingCallGuard {
    fn drop(&mut self) {
        if let Some(in_blocking_call) = &self.in_blocking_call {
            in_blocking_call.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

fn translate_plugin_path(env: &PluginEnv, path: PathBuf) -> PathBuf {
    if let Ok(stripped) = path.strip_prefix("/host") {
        env.plugin_cwd.join(stripped)
    } else if let Ok(stripped) = path.strip_prefix("/data") {
        env.plugin_own_data_dir.join(stripped)
    } else if let Ok(stripped) = path.strip_prefix("/cache") {
        env.plugin_own_cache_dir.join(stripped)
    } else if let Ok(stripped) = path.strip_prefix("/tmp") {
        ZELLIJ_TMP_DIR.join(stripped)
    } else if path.is_relative() {
        env.plugin_cwd.join(path)
    } else {
        // Absolute path not in any plugin special folder — pass through
        path
    }
}

fn acting_client(env: &PluginEnv) -> ClientId {
    env.shared
        .as_ref()
        .and_then(|shared| shared.current_client)
        .unwrap_or(env.client_id)
}

fn self_pane_id(env: &PluginEnv) -> PluginId {
    env.shared
        .as_ref()
        .and_then(|shared| shared.current_slot)
        .unwrap_or(env.plugin_id)
}

fn source_pane(env: &PluginEnv) -> Option<PaneId> {
    match env.shared.as_ref() {
        Some(shared) => shared.current_slot.map(PaneId::Plugin),
        None => Some(PaneId::Plugin(env.plugin_id)),
    }
}

fn permission_pane_id(env: &PluginEnv) -> PluginId {
    match env.shared.as_ref() {
        Some(shared) => shared
            .current_slot
            .or_else(|| shared.first_pane_slot())
            .unwrap_or(env.plugin_id),
        None => env.plugin_id,
    }
}

fn crash_pane_ids(env: &PluginEnv) -> Vec<PluginId> {
    match env.shared.as_ref() {
        Some(shared) => shared.pane_slot_ids(),
        None => vec![env.plugin_id],
    }
}

fn is_self_command(command: &PluginCommand) -> bool {
    matches!(
        command,
        PluginCommand::SetSelectable(..)
            | PluginCommand::ShowCursor(..)
            | PluginCommand::HideSelf
            | PluginCommand::ShowSelf(..)
            | PluginCommand::CloseSelf
            | PluginCommand::SetSelfMouseSelectionSupport(..)
            | PluginCommand::SetSelfCollapsed(..)
            | PluginCommand::SetLiveView(..)
            | PluginCommand::SetLiveViewInput(..)
    ) || self_command_has_response(command)
}

fn self_command_has_response(command: &PluginCommand) -> bool {
    matches!(
        command,
        PluginCommand::OpenFileNearPlugin(..)
            | PluginCommand::OpenFileFloatingNearPlugin(..)
            | PluginCommand::OpenFileInPlaceOfPlugin(..)
            | PluginCommand::OpenTerminalNearPlugin(..)
            | PluginCommand::OpenTerminalFloatingNearPlugin(..)
            | PluginCommand::OpenTerminalInPlaceOfPlugin(..)
            | PluginCommand::OpenCommandPaneNearPlugin(..)
            | PluginCommand::OpenCommandPaneFloatingNearPlugin(..)
            | PluginCommand::OpenCommandPaneInPlaceOfPlugin(..)
    )
}

fn self_command_is_forbidden(env: &PluginEnv, command: &PluginCommand) -> bool {
    match env.shared.as_ref() {
        Some(shared) => {
            is_self_command(command)
                && !(shared.self_commands_allowed && shared.current_slot.is_some())
        },
        None => false,
    }
}

fn slot_command_target(env: &PluginEnv, slot_id: PluginId) -> std::result::Result<(), String> {
    match env.shared.as_ref() {
        Some(shared) if shared.slots.contains_key(&slot_id) => Ok(()),
        Some(_) => Err(format!("Slot {} does not belong to this plugin", slot_id)),
        None if slot_id == env.plugin_id => Ok(()),
        None => Err(format!(
            "Slot {} does not belong to this plugin, slot commands are meant for shared plugins",
            slot_id
        )),
    }
}

fn write_slot_command_response(env: &PluginEnv, result: std::result::Result<(), String>) {
    let response = ProtobufSlotCommandResponse::from(result);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write slot command response"))
        .non_fatal();
}

fn set_selectable_slot(env: &PluginEnv, slot_id: PluginId, selectable: bool) {
    let result = slot_command_target(env, slot_id).and_then(|_| {
        env.senders
            .send_to_screen(ScreenInstruction::SetSelectable(
                PaneId::Plugin(slot_id),
                selectable,
            ))
            .map_err(|e| e.to_string())
    });
    write_slot_command_response(env, result);
}

fn hide_slot(env: &PluginEnv, slot_id: PluginId) {
    let result = slot_command_target(env, slot_id).and_then(|_| {
        env.senders
            .send_to_screen(ScreenInstruction::SuppressPane(
                PaneId::Plugin(slot_id),
                acting_client(env),
            ))
            .map_err(|e| e.to_string())
    });
    write_slot_command_response(env, result);
}

fn show_slot(env: &PluginEnv, slot_id: PluginId, should_float_if_hidden: bool) {
    let result = slot_command_target(env, slot_id).map(|_| {
        let action = Action::FocusPluginPaneWithId {
            pane_id: slot_id,
            should_float_if_hidden,
            should_be_in_place_if_hidden: false,
        };
        let error_msg = || format!("Failed to show slot");
        apply_action!(action, error_msg, env);
    });
    write_slot_command_response(env, result);
}

fn close_slot(env: &PluginEnv, slot_id: PluginId) {
    let result = slot_command_target(env, slot_id).and_then(|_| {
        env.senders
            .send_to_screen(ScreenInstruction::ClosePane(
                PaneId::Plugin(slot_id),
                None,
                None,
                None,
                ClosePaneSource::External,
                None,
            ))
            .map_err(|e| e.to_string())?;
        env.senders
            .send_to_plugin(PluginInstruction::Unload(slot_id))
            .map_err(|e| e.to_string())
    });
    write_slot_command_response(env, result);
}

fn set_collapsed_slot(env: &PluginEnv, slot_id: PluginId, collapsed: bool) {
    let result = slot_command_target(env, slot_id).and_then(|_| {
        env.senders
            .send_to_screen(ScreenInstruction::SetPaneCollapsed(
                PaneId::Plugin(slot_id),
                collapsed,
            ))
            .map_err(|e| e.to_string())
    });
    write_slot_command_response(env, result);
}

fn open_context_menu(env: &PluginEnv, target: ContextMenuTarget, line: usize, column: usize) {
    env.senders
        .send_to_screen(ScreenInstruction::OpenContextMenuFromPlugin {
            plugin_id: self_pane_id(env),
            client_id: acting_client(env),
            target,
            line,
            column,
        })
        .with_context(|| format!("failed to open context menu"))
        .non_fatal();
}

fn open_plugin_popup(
    env: &PluginEnv,
    plugin_url: String,
    configuration: BTreeMap<String, String>,
    line: usize,
    column: usize,
    width: usize,
    height: usize,
    options: PopupOptions,
) {
    let run_plugin_or_alias = match RunPluginOrAlias::from_url(
        &plugin_url,
        &Some(configuration),
        None,
        Some(env.plugin_cwd.clone()),
    ) {
        Ok(run_plugin_or_alias) => run_plugin_or_alias,
        Err(e) => {
            log::error!("Failed to open popup: {}", e);
            return;
        },
    };
    env.senders
        .send_to_screen(ScreenInstruction::OpenPluginPopup {
            requesting_plugin_id: self_pane_id(env),
            client_id: acting_client(env),
            run_plugin_or_alias,
            line,
            column,
            width,
            height,
            options,
        })
        .with_context(|| format!("failed to open popup"))
        .non_fatal();
}

fn run_context_menu_item(env: &PluginEnv, index: usize) {
    let client_id = acting_client(env);
    let plugin_id = env.plugin_id;
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    if env
        .senders
        .send_to_screen(ScreenInstruction::GetContextMenuItemActions {
            plugin_id,
            client_id,
            index,
            response_channel: response_sender,
        })
        .is_err()
    {
        log::error!("Failed to request context menu item {}", index);
        return;
    }
    let actions = match response_receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(actions) => actions,
        Err(e) => {
            log::error!("Failed to get context menu item {}: {:?}", index, e);
            return;
        },
    };
    let senders = env.senders.clone();
    let default_shell = env.default_shell.clone();
    let default_mode = env.default_mode.clone();
    thread::spawn(move || {
        for action in actions {
            if let Err(e) = route_action(
                action,
                client_id,
                None,
                Some(PaneId::Plugin(plugin_id)),
                senders.clone(),
                default_shell.clone(),
                None,
                default_mode,
                None,
                false,
            ) {
                log::error!("Failed to run context menu action: {:?}", e);
            }
        }
    });
}

fn get_session_suggestions(env: &PluginEnv, limit: usize) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::SessionSuggestions(
            SessionSuggestionsJob::GetSuggestions {
                plugin_id: env.plugin_id,
                client_id: env.client_id,
                limit,
            },
        ));
}

fn set_popup_focused(env: &PluginEnv, focused: bool) {
    env.senders
        .send_to_screen(ScreenInstruction::SetPopupFocused {
            plugin_id: env.plugin_id,
            focused,
        })
        .with_context(|| format!("failed to change popup focus"))
        .non_fatal();
}

fn popup_to_floating_pane(env: &mut PluginEnv, coordinates: Option<FloatingPaneCoordinates>) {
    env.pending_popup_size = None;
    env.senders
        .send_to_screen(ScreenInstruction::PopupToFloatingPane {
            plugin_id: env.plugin_id,
            coordinates,
        })
        .with_context(|| format!("failed to move popup to a floating pane"))
        .non_fatal();
}

fn switch_session_and_close_current(
    env: &PluginEnv,
    connect_to_session: ConnectToSession,
) -> Result<()> {
    let err_context = || format!("Failed to switch session");
    let (completion_tx, completion_rx) = oneshot::channel();
    if connect_to_session
        .name
        .as_ref()
        .map(|s| s.contains('/'))
        .unwrap_or(false)
    {
        log::error!("Session names cannot contain \'/\'");
        return Ok(());
    }
    let client_id = acting_client(env);
    let connect_to_session = ConnectToSession {
        tab_position: connect_to_session.tab_position.map(|p| p + 1),
        cwd: connect_to_session
            .cwd
            .map(|c| translate_plugin_path(env, c))
            .or_else(|| Some(env.plugin_cwd.clone())),
        ..connect_to_session
    };
    env.senders
        .send_to_server(ServerInstruction::SwitchSessionAndCloseCurrent(
            connect_to_session,
            client_id,
            Some(NotificationEnd::new(completion_tx)),
        ))
        .with_context(err_context)?;
    let _ = wait_for_action_completion(completion_rx, "switch_session_and_close_current", false);
    Ok(())
}

fn get_session_preview(
    env: &PluginEnv,
    name: String,
    tab_index: Option<usize>,
    pane_id: Option<(u32, bool)>,
    anchor: Option<zellij_utils::data::PreviewAnchor>,
) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::SessionPreview {
            plugin_id: env.plugin_id,
            client_id: env.client_id,
            session_name: name,
            tab_index,
            pane_id,
            anchor,
        });
}

fn session_pane_command(
    env: &PluginEnv,
    session_name: Option<String>,
    command: crate::screen::SessionPaneCommand,
) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::SessionPaneCommand {
            session_name,
            command,
            client_id: acting_client(env),
            completion: None,
        });
}

fn open_session_terminal(env: &PluginEnv, request: zellij_utils::data::SessionTerminalRequest) {
    let creates_a_session = request.creates_a_session();
    let mut layout_holder = ConnectToSession {
        layout: request.layout.clone(),
        ..Default::default()
    };
    if let Some(layout_dir) = env.layout_dir.clone().or_else(default_layout_dir) {
        layout_holder.apply_layout_dir(&layout_dir);
    }
    let cwd = request
        .cwd
        .clone()
        .map(|c| translate_plugin_path(env, c))
        .or_else(|| creates_a_session.then(|| env.plugin_cwd.clone()));
    let _ = std::thread::Builder::new()
        .name("open_session_terminal".to_owned())
        .spawn(move || {
            let request = zellij_utils::app_control::OpenWindowRequest {
                create: request.new_window || request.session_name.is_none(),
                session: request.session_name,
                pane_id: request.pane_id.map(|p| match p {
                    zellij_utils::data::PaneId::Terminal(id) => (id, false),
                    zellij_utils::data::PaneId::Plugin(id) => (id, true),
                }),
                layout: layout_holder.layout,
                cwd,
                ..Default::default()
            };
            if let Err(e) = crate::desktop_app::open_window(request) {
                log::info!("The desktop app did not open the session window: {}", e);
            }
        });
}

fn mark_all_seen(env: &PluginEnv, session_name: Option<String>) {
    let _ = env.senders.send_to_screen(ScreenInstruction::MarkAllSeen {
        session: session_name,
        this_session: false,
        completion: None,
    });
}

fn get_saved_session_preview(env: &PluginEnv, name: String) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::SavedSessionPreview {
            plugin_id: env.plugin_id,
            client_id: env.client_id,
            session_name: name,
        });
}

fn may_change_desktop_integration(location: &RunPluginLocation) -> bool {
    matches!(location, RunPluginLocation::Zellij(_))
}

fn desktop_integration(env: &PluginEnv, request: String) {
    let send_plugin_instructions = env.senders.to_plugin.clone();
    let plugin_id = env.plugin_id;
    let client_id = acting_client(env);
    let allowed = may_change_desktop_integration(&env.plugin.location);
    if !allowed {
        log::warn!(
            "Ignoring a desktop integration request from {}: only built-in plugins may make it",
            env.plugin.location.display()
        );
    }
    let _ = std::thread::Builder::new()
        .name("desktop_integration".to_owned())
        .spawn(move || {
            let response = if allowed {
                crate::desktop_app::desktop_integration(&request)
            } else {
                zellij_utils::desktop_integration::Response::Error(
                    "Only built-in plugins may change the desktop integration".to_owned(),
                )
                .to_json()
            };
            if let Some(sender) = send_plugin_instructions {
                let _ = sender.send(PluginInstruction::Update(vec![(
                    Some(plugin_id),
                    Some(client_id),
                    Event::DesktopIntegration(response),
                )]));
            }
        });
}

fn register_settings_entry(env: &PluginEnv, title: String) {
    let plugin_identity = (
        env.plugin.location.clone(),
        env.plugin.initial_userspace_configuration.clone(),
    );
    env.senders
        .send_to_plugin(PluginInstruction::RegisterSettingsEntry {
            plugin_id: env.plugin_id,
            plugin_identity,
            title,
        })
        .with_context(|| format!("failed to register a settings entry for {}", env.name()))
        .non_fatal();
}

fn unregister_settings_entry(env: &PluginEnv) {
    env.senders
        .send_to_plugin(PluginInstruction::UnregisterSettingsEntry(env.plugin_id))
        .with_context(|| format!("failed to unregister the settings entry of {}", env.name()))
        .non_fatal();
}

fn set_popup_size(env: &mut PluginEnv, width: usize, height: usize) {
    if env.shared.is_none() {
        env.pending_popup_size = Some((width, height));
    }
    env.senders
        .send_to_screen(ScreenInstruction::SetPopupSize {
            plugin_id: self_pane_id(env),
            width,
            height,
        })
        .with_context(|| format!("failed to set popup size"))
        .non_fatal();
}

pub fn zellij_exports(linker: &mut Linker<PluginEnv>) {
    linker
        .func_wrap("zellij", "host_run_plugin_command", host_run_plugin_command)
        // SAFETY: plugin export setup is controlled; func_wrap only fails on
        // duplicate/invalid registration during linker construction.
        .expect("register host_run_plugin_command");
}

fn host_run_plugin_command(mut caller: Caller<'_, PluginEnv>) {
    let mut env = caller.data_mut();
    if !env.is_alive() && !crate::plugins::pinned_executor::before_close_host_calls_allowed() {
        // A callback queued before unload/quarantine must not reach the host. There is nobody
        // left to consume a response, so do not execute or answer the command. BeforeClose is
        // the exception: unload revokes the load first, and that callback still reverts state.
        return;
    }
    let plugin_command = env.name();
    let err_context = || format!("failed to run plugin command {}", plugin_command);
    wasi_read_bytes(env)
        .and_then(|bytes| {
            let command: ProtobufPluginCommand = ProtobufPluginCommand::decode(bytes.as_slice())?;
            let command: PluginCommand = command
                .try_into()
                .map_err(|e| anyhow!("failed to convert serialized command: {}", e))?;
            if self_command_is_forbidden(&env, &command) {
                if self_command_has_response(&command) {
                    wasi_write_object(env, &Vec::<u8>::new())?;
                }
                return Err(anyhow!(
                    "'self' commands of shared plugins are only available inside render and slot_added, use the explicit slot form instead: {:?}",
                    CommandType::from_str(&command.to_string()).ok()
                ));
            }
            match check_command_permission(&env, &command) {
                (PermissionStatus::Granted, _) => match command {
                    PluginCommand::SetSelectableSlot(slot_id, selectable) => {
                        set_selectable_slot(env, slot_id, selectable)
                    },
                    PluginCommand::HideSlot(slot_id) => hide_slot(env, slot_id),
                    PluginCommand::ShowSlot(slot_id, should_float_if_hidden) => {
                        show_slot(env, slot_id, should_float_if_hidden)
                    },
                    PluginCommand::CloseSlot(slot_id) => close_slot(env, slot_id),
                    PluginCommand::SetCollapsedSlot(slot_id, collapsed) => {
                        set_collapsed_slot(env, slot_id, collapsed)
                    },
                    PluginCommand::OpenContextMenu {
                        target,
                        line,
                        column,
                    } => open_context_menu(env, target, line, column),
                    PluginCommand::OpenPluginPopup {
                        plugin_url,
                        configuration,
                        line,
                        column,
                        width,
                        height,
                        options,
                    } => open_plugin_popup(
                        env,
                        plugin_url,
                        configuration,
                        line,
                        column,
                        width,
                        height,
                        options,
                    ),
                    PluginCommand::SetPopupSize { width, height } => {
                        set_popup_size(env, width, height)
                    },
                    PluginCommand::GetSessionSuggestions { limit } => {
                        get_session_suggestions(env, limit)
                    },
                    PluginCommand::SetPopupFocused(focused) => set_popup_focused(env, focused),
                    PluginCommand::PopupToFloatingPane(coordinates) => {
                        popup_to_floating_pane(env, coordinates)
                    },
                    PluginCommand::SwitchSessionAndCloseCurrent(connect_to_session) => {
                        switch_session_and_close_current(env, connect_to_session)?
                    },
                    PluginCommand::GetSessionPreview {
                        name,
                        tab_index,
                        pane_id,
                        anchor,
                    } => get_session_preview(env, name, tab_index, pane_id, anchor),
                    PluginCommand::MarkSessionPaneSeen {
                        session_name,
                        pane_id,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::MarkSeen(pane_id),
                    ),
                    PluginCommand::MarkAllSeen { session_name } => {
                        mark_all_seen(env, session_name)
                    },
                    PluginCommand::OpenSessionTerminal(request) => {
                        open_session_terminal(env, request)
                    },
                    PluginCommand::WriteToSessionPane {
                        session_name,
                        pane_id,
                        bytes,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::Write(pane_id, bytes),
                    ),
                    PluginCommand::SendKeyToSessionPane {
                        session_name,
                        pane_id,
                        key,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::Key(pane_id, key),
                    ),
                    PluginCommand::ToggleSessionPaneMute {
                        session_name,
                        pane_id,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::ToggleMute(pane_id),
                    ),
                    PluginCommand::ToggleSessionPaneIgnoreOutput {
                        session_name,
                        pane_id,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::ToggleIgnoreOutput(pane_id),
                    ),
                    PluginCommand::ToggleSessionPaneWatch {
                        session_name,
                        pane_id,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::ToggleWatch(pane_id),
                    ),
                    PluginCommand::FocusSessionPane {
                        session_name,
                        pane_id,
                    } => session_pane_command(
                        env,
                        Some(session_name),
                        crate::screen::SessionPaneCommand::Focus(pane_id),
                    ),
                    PluginCommand::SetLiveView(view) => set_live_view(env, view),
                    PluginCommand::SetLiveViewInput(capture) => set_live_view_input(env, capture),
                    PluginCommand::GetSavedSessionPreview(name) => {
                        get_saved_session_preview(env, name)
                    },
                    PluginCommand::RegisterSettingsEntry(title) => {
                        register_settings_entry(env, title)
                    },
                    PluginCommand::UnregisterSettingsEntry => unregister_settings_entry(env),
                    PluginCommand::DesktopIntegration(request) => {
                        desktop_integration(env, request)
                    },
                    PluginCommand::RunContextMenuItem(index) => {
                        run_context_menu_item(env, index)
                    },
                    PluginCommand::ReadConfig => read_config(env),
                    PluginCommand::RevertConfig(key) => revert_config(env, key),
                    PluginCommand::UnsetConfigSetting(key) => unset_config_setting(env, key),
                    PluginCommand::SaveConfig => save_config(env),
                    PluginCommand::OverwriteConfigFile => overwrite_config_file(env),
                    PluginCommand::ReloadConfigFile => reload_config_file(env),
                    PluginCommand::ReplaceConfigBlocks(blocks) => replace_config_blocks(env, blocks),
                    PluginCommand::ResetKeys {
                        keys,
                        write_config_to_disk,
                    } => reset_keys(env, keys, write_config_to_disk),
                    PluginCommand::ResetMousebinds {
                        triggers,
                        write_config_to_disk,
                    } => reset_mousebinds(env, triggers, write_config_to_disk),
                    PluginCommand::WriteThemeFile {
                        name,
                        copy_from,
                        colours,
                    } => write_theme_file(env, name, copy_from, colours),
                    PluginCommand::DeleteThemeFile { name } => delete_theme_file(env, name),
                    PluginCommand::SaveKeybindsAsPreset { new_name } => {
                        save_keybinds_as_preset(env, new_name)
                    },
                    PluginCommand::Prompt {
                        request_id,
                        request,
                    } => prompt(env, request_id, request),
                    PluginCommand::ReplyToPrompt { request_id, result } => {
                        reply_to_prompt(env, request_id, result)
                    },
                    PluginCommand::CopyKeybindPreset { preset, new_name } => {
                        copy_keybind_preset(env, preset, new_name)
                    },
                    PluginCommand::Subscribe(event_list) => subscribe(env, event_list)?,
                    PluginCommand::Unsubscribe(event_list) => unsubscribe(env, event_list)?,
                    PluginCommand::SetSelectable(selectable) => set_selectable(env, selectable),
                    PluginCommand::ShowCursor(cursor_position) => show_cursor(env, cursor_position),
                    PluginCommand::GetPluginIds => get_plugin_ids(env),
                    PluginCommand::GetZellijVersion => get_zellij_version(env),
                    PluginCommand::GenerateRandomName => generate_random_name(env),
                    PluginCommand::DumpLayout(layout_name) => dump_layout(env, layout_name),
                    PluginCommand::ParseLayout(layout_string) => parse_layout(env, layout_string),
                    PluginCommand::GetLayoutDir => get_layout_dir(env),
                    PluginCommand::GetFocusedPaneInfo => get_focused_pane_info(env),
                    PluginCommand::SaveSession => save_session(env),
                    PluginCommand::CurrentSessionLastSavedTime => {
                        current_session_last_saved_time(env)
                    },
                    PluginCommand::GetPaneInfo(pane_id) => get_pane_info(env, pane_id),
                    PluginCommand::GetTabInfo(tab_id) => get_tab_info(env, tab_id),
                    PluginCommand::OpenFile(file_to_open, context) => {
                        open_file(env, file_to_open, context)
                    },
                    PluginCommand::OpenFileFloating(
                        file_to_open,
                        floating_pane_coordinates,
                        context,
                    ) => open_file_floating(env, file_to_open, floating_pane_coordinates, context),
                    PluginCommand::OpenTerminal(cwd) => {
                        let border_style = cwd.border_style;
                        open_terminal(env, cwd.path.try_into()?, border_style)
                    },
                    PluginCommand::OpenTerminalNearPlugin(cwd) => {
                        let border_style = cwd.border_style;
                        open_terminal_near_plugin(env, cwd.path.try_into()?, border_style)
                    },
                    PluginCommand::OpenTerminalFloating(cwd, floating_pane_coordinates) => {
                        let floating_pane_coordinates = FloatingPaneCoordinates::merge_border_style(
                            floating_pane_coordinates,
                            cwd.border_style,
                        );
                        open_terminal_floating(env, cwd.path.try_into()?, floating_pane_coordinates)
                    },
                    PluginCommand::OpenTerminalFloatingNearPlugin(
                        cwd,
                        floating_pane_coordinates,
                    ) => {
                        let floating_pane_coordinates = FloatingPaneCoordinates::merge_border_style(
                            floating_pane_coordinates,
                            cwd.border_style,
                        );
                        open_terminal_floating_near_plugin(
                            env,
                            cwd.path.try_into()?,
                            floating_pane_coordinates,
                        )
                    },
                    PluginCommand::OpenCommandPane(command_to_run, context) => {
                        open_command_pane(env, command_to_run, context)
                    },
                    PluginCommand::OpenCommandPaneNearPlugin(command_to_run, context) => {
                        open_command_pane_near_plugin(env, command_to_run, context)
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
                    PluginCommand::OpenCommandPaneFloatingNearPlugin(
                        command_to_run,
                        floating_pane_coordinates,
                        context,
                    ) => open_command_pane_floating_near_plugin(
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
                    PluginCommand::OverrideLayout(
                        layout_info,
                        retain_existing_terminal_panes,
                        retain_existing_plugin_panes,
                        apply_only_to_active_tab,
                        context,
                    ) => override_layout(
                        env,
                        layout_info,
                        retain_existing_terminal_panes,
                        retain_existing_plugin_panes,
                        apply_only_to_active_tab,
                        context,
                    )?,
                    PluginCommand::SaveLayout {
                        layout_name,
                        layout_kdl,
                        overwrite,
                    } => save_layout(env, layout_name, layout_kdl, overwrite),
                    PluginCommand::DeleteLayout { layout_name } => delete_layout(env, layout_name),
                    PluginCommand::RenameLayout {
                        old_layout_name,
                        new_layout_name,
                    } => rename_layout(env, old_layout_name, new_layout_name),
                    PluginCommand::EditLayout {
                        layout_name,
                        context,
                    } => edit_layout(env, layout_name, context),
                    PluginCommand::NewTab { name, cwd } => new_tab(env, name, cwd),
                    PluginCommand::NewTabUnfocused { name, cwd } => {
                        new_tab_unfocused(env, name, cwd)
                    },
                    PluginCommand::NewTiledPaneInTab { tab_position } => {
                        new_tiled_pane_in_tab(env, tab_position)
                    },
                    PluginCommand::ToggleFloatingPanes { tab_id } => {
                        toggle_floating_panes(env, tab_id)
                    },
                    PluginCommand::NewPane => new_pane(env),
                    PluginCommand::GoToNextTab => go_to_next_tab(env),
                    PluginCommand::GoToPreviousTab => go_to_previous_tab(env),
                    PluginCommand::Resize(resize_payload) => resize(env, resize_payload),
                    PluginCommand::ResizeWithDirection(resize_strategy) => {
                        resize_with_direction(env, resize_strategy)
                    },
                    PluginCommand::FocusNextPane => focus_next_pane(env),
                    PluginCommand::FocusPreviousPane => focus_previous_pane(env),
                    PluginCommand::FocusLastPane => focus_last_pane(env),
                    PluginCommand::MoveFocus(direction) => move_focus(env, direction),
                    PluginCommand::MoveFocusOrTab(direction) => move_focus_or_tab(env, direction),
                    PluginCommand::Detach => detach(env),
                    PluginCommand::EditScrollback => edit_scrollback(env),
                    PluginCommand::Write(bytes) => write(env, bytes),
                    PluginCommand::WriteChars(chars) => write_chars(env, chars),
                    PluginCommand::CopyToClipboard(text) => copy_to_clipboard(env, text)?,
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
                    PluginCommand::ToggleFocusNoUiFullscreen => toggle_focus_no_ui_fullscreen(env),
                    PluginCommand::FocusHostSession => focus_host_session(env),
                    PluginCommand::GetNestedSessionKeybinds(pane_id) => {
                        get_nested_session_keybinds(env, pane_id.into())
                    },
                    PluginCommand::TogglePaneFrames => toggle_pane_frames(env),
                    PluginCommand::SetPaneFrameStyle(pane_frame_style) => {
                        set_pane_frame_style(env, pane_frame_style)
                    },
                    PluginCommand::ToggleBars(bars) => {
                        apply_bars_action(env, Action::ToggleBars { bars })
                    },
                    PluginCommand::HideBars(bars) => {
                        apply_bars_action(env, Action::HideBars { bars })
                    },
                    PluginCommand::ShowBars => apply_bars_action(env, Action::ShowBars),
                    PluginCommand::TogglePaneEmbedOrEject => toggle_pane_embed_or_eject(env),
                    PluginCommand::UndoRenamePane => undo_rename_pane(env),
                    PluginCommand::CloseFocus => close_focus(env),
                    PluginCommand::ToggleActiveTabSync => toggle_active_tab_sync(env),
                    PluginCommand::CloseFocusedTab => close_focused_tab(env),
                    PluginCommand::UndoRenameTab => undo_rename_tab(env),
                    PluginCommand::QuitZellij => quit_zellij(env),
                    PluginCommand::PreviousSwapLayout => previous_swap_layout(env),
                    PluginCommand::NextSwapLayout => next_swap_layout(env),
                    PluginCommand::ApplyTiledSwapLayout(layout_name) => {
                        apply_tiled_swap_layout(env, layout_name)
                    },
                    PluginCommand::ApplyFloatingSwapLayout(layout_name) => {
                        apply_floating_swap_layout(env, layout_name)
                    },
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
                    PluginCommand::FocusTerminalPane(
                        terminal_pane_id,
                        should_float_if_hidden,
                        should_be_in_place_if_hidden,
                    ) => focus_terminal_pane(
                        env,
                        terminal_pane_id,
                        should_float_if_hidden,
                        should_be_in_place_if_hidden,
                    ),
                    PluginCommand::FocusPluginPane(
                        plugin_pane_id,
                        should_float_if_hidden,
                        should_be_in_place_if_hidden,
                    ) => focus_plugin_pane(
                        env,
                        plugin_pane_id,
                        should_float_if_hidden,
                        should_be_in_place_if_hidden,
                    ),
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
                        connect_to_session.switch_context,
                    )?,
                    PluginCommand::DeleteDeadSession(session_name) => {
                        delete_dead_session(session_name)?;
                        notify_session_index_changed(env);
                    },
                    PluginCommand::DeleteAllDeadSessions => {
                        delete_all_dead_sessions()?;
                        notify_session_index_changed(env);
                    },
                    PluginCommand::OpenFileInPlace(file_to_open, context) => {
                        open_file_in_place(env, file_to_open, context)
                    },
                    PluginCommand::OpenTerminalInPlace(cwd) => {
                        open_terminal_in_place(env, cwd.path.try_into()?)
                    },
                    PluginCommand::OpenTerminalInPlaceOfPlugin(cwd, close_plugin_after_replace) => {
                        open_terminal_in_place_of_plugin(
                            env,
                            cwd.path.try_into()?,
                            close_plugin_after_replace,
                        )
                    },
                    PluginCommand::OpenCommandPaneInPlace(command_to_run, context) => {
                        open_command_pane_in_place(env, command_to_run, context)
                    },
                    PluginCommand::OpenCommandPaneInPlaceOfPlugin(
                        command_to_run,
                        close_plugin_after_replace,
                        context,
                    ) => open_command_pane_in_place_of_plugin(
                        env,
                        command_to_run,
                        close_plugin_after_replace,
                        context,
                    ),
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
                    PluginCommand::SetCliPipeExitCode(pipe_name, exit_code) => {
                        set_cli_pipe_exit_code(env, pipe_name, exit_code)?
                    },
                    PluginCommand::MessageToPlugin(message) => message_to_plugin(env, message)?,
                    PluginCommand::DisconnectOtherClients => disconnect_other_clients(env),
                    PluginCommand::KillSessions(session_list) => kill_sessions(session_list),
                    PluginCommand::KillSessionsAndReply(session_list) => {
                        kill_sessions_and_reply(env, session_list)
                    },
                    PluginCommand::DeleteDeadSessionAndReply(session_name) => {
                        delete_dead_session_and_reply(env, session_name)
                    },
                    PluginCommand::DeleteAllDeadSessionsAndReply => {
                        delete_all_dead_sessions_and_reply(env);
                        notify_session_index_changed(env);
                    },
                    PluginCommand::ScanHostFolder(folder_to_scan) => {
                        scan_host_folder(env, folder_to_scan)
                    },
                    PluginCommand::WatchFilesystem => watch_filesystem(env),
                    PluginCommand::ListWindowsVolumes => list_windows_volumes(env),
                    PluginCommand::GetSessionList => get_session_list(env),
                    PluginCommand::SetSoftKeyboard(on) => set_soft_keyboard(env, on),
                    PluginCommand::DumpSessionLayout { tab_index } => {
                        dump_session_layout(env, tab_index)
                    },
                    PluginCommand::CloseSelf => close_self(env),
                    PluginCommand::Reconfigure(new_config, write_config_to_disk) => {
                        reconfigure(env, new_config, write_config_to_disk)?
                    },
                    PluginCommand::HidePaneWithId(pane_id) => {
                        hide_pane_with_id(env, pane_id.into())?
                    },
                    PluginCommand::ShowPaneWithId(
                        pane_id,
                        should_float_if_hidden,
                        should_focus_pane,
                    ) => show_pane_with_id(
                        env,
                        pane_id.into(),
                        should_float_if_hidden,
                        should_focus_pane,
                    ),
                    PluginCommand::OpenCommandPaneBackground(command_to_run, context) => {
                        open_command_pane_background(env, command_to_run, context)
                    },
                    PluginCommand::RerunCommandPane(terminal_pane_id) => {
                        rerun_command_pane(env, terminal_pane_id)
                    },
                    PluginCommand::ResizePaneIdWithDirection(resize, pane_id) => {
                        resize_pane_with_id(env, resize, pane_id.into())
                    },
                    PluginCommand::EditScrollbackForPaneWithId(pane_id) => {
                        edit_scrollback_for_pane_with_id(env, pane_id.into())
                    },
                    PluginCommand::GetPaneScrollback {
                        pane_id,
                        get_full_scrollback,
                    } => get_pane_scrollback(env, pane_id.into(), get_full_scrollback),
                    PluginCommand::WriteToPaneId(bytes, pane_id) => {
                        write_to_pane_id(env, bytes, pane_id.into())
                    },
                    PluginCommand::WriteCharsToPaneId(chars, pane_id) => {
                        write_chars_to_pane_id(env, chars, pane_id.into())
                    },
                    PluginCommand::SendSigintToPaneId(pane_id) => {
                        send_sigint_to_pane_id(env, pane_id.into())
                    },
                    PluginCommand::SendSigkillToPaneId(pane_id) => {
                        send_sigkill_to_pane_id(env, pane_id.into())
                    },
                    PluginCommand::GetPanePid { pane_id } => get_pane_pid(env, pane_id.into()),
                    PluginCommand::GetPaneRunningCommand { pane_id } => {
                        get_pane_running_command(env, pane_id.into())
                    },
                    PluginCommand::GetPaneCwd { pane_id } => get_pane_cwd(env, pane_id.into()),
                    PluginCommand::MovePaneWithPaneId(pane_id) => {
                        move_pane_with_pane_id(env, pane_id.into())
                    },
                    PluginCommand::MovePaneWithPaneIdInDirection(pane_id, direction) => {
                        move_pane_with_pane_id_in_direction(env, pane_id.into(), direction)
                    },
                    PluginCommand::ClearScreenForPaneId(pane_id) => {
                        clear_screen_for_pane_id(env, pane_id.into())
                    },
                    PluginCommand::ScrollUpInPaneId(pane_id) => {
                        scroll_up_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::ScrollDownInPaneId(pane_id) => {
                        scroll_down_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::ScrollToTopInPaneId(pane_id) => {
                        scroll_to_top_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::ScrollToBottomInPaneId(pane_id) => {
                        scroll_to_bottom_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::PageScrollUpInPaneId(pane_id) => {
                        page_scroll_up_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::PageScrollDownInPaneId(pane_id) => {
                        page_scroll_down_in_pane_id(env, pane_id.into())
                    },
                    PluginCommand::TogglePaneIdFullscreen(pane_id) => {
                        toggle_pane_id_fullscreen(env, pane_id.into())
                    },
                    PluginCommand::TogglePaneEmbedOrEjectForPaneId(pane_id, show_and_focus) => {
                        toggle_pane_embed_or_eject_for_pane_id(env, pane_id.into(), show_and_focus)
                    },
                    PluginCommand::CloseTabWithIndex(tab_index) => {
                        close_tab_with_index(env, tab_index)
                    },
                    PluginCommand::BreakPanesToNewTab(
                        pane_ids,
                        new_tab_name,
                        should_change_focus_to_new_tab,
                    ) => break_panes_to_new_tab(
                        env,
                        pane_ids.into_iter().map(|p_id| p_id.into()).collect(),
                        new_tab_name,
                        should_change_focus_to_new_tab,
                    ),
                    PluginCommand::BreakPanesToTabWithIndex(
                        pane_ids,
                        should_change_focus_to_new_tab,
                        tab_index,
                    ) => break_panes_to_tab_with_index(
                        env,
                        pane_ids.into_iter().map(|p_id| p_id.into()).collect(),
                        tab_index,
                        should_change_focus_to_new_tab,
                    ),
                    PluginCommand::SwitchTabToId(tab_id) => switch_tab_to_id(env, tab_id),
                    PluginCommand::GoToTabWithId(tab_id) => go_to_tab_with_id(env, tab_id),
                    PluginCommand::CloseTabWithId(tab_id) => close_tab_with_id(env, tab_id),
                    PluginCommand::RenameTabWithId(tab_id, new_name) => {
                        rename_tab_with_id(env, tab_id, &new_name)
                    },
                    PluginCommand::BreakPanesToTabWithId(pane_ids, tab_id, should_change_focus) => {
                        break_panes_to_tab_with_id(
                            env,
                            pane_ids.into_iter().map(|p_id| p_id.into()).collect(),
                            should_change_focus,
                            tab_id,
                        )
                    },
                    PluginCommand::ReloadPlugin(plugin_id) => reload_plugin(env, plugin_id),
                    PluginCommand::LoadNewPlugin {
                        url,
                        config,
                        load_in_background,
                        skip_plugin_cache,
                    } => load_new_plugin(env, url, config, load_in_background, skip_plugin_cache),
                    PluginCommand::RebindKeys {
                        keys_to_rebind,
                        keys_to_unbind,
                        write_config_to_disk,
                    } => rebind_keys(env, keys_to_rebind, keys_to_unbind, write_config_to_disk)?,
                    PluginCommand::ListClients => list_clients(env),
                    PluginCommand::ChangeHostFolder(new_host_folder) => {
                        change_host_folder(env, new_host_folder)
                    },
                    PluginCommand::SetFloatingPanePinned(pane_id, should_be_pinned) => {
                        set_floating_pane_pinned(env, pane_id.into(), should_be_pinned)
                    },
                    PluginCommand::StackPanes(pane_ids) => {
                        stack_panes(env, pane_ids.into_iter().map(|p_id| p_id.into()).collect())
                    },
                    PluginCommand::ChangeFloatingPanesCoordinates(pane_ids_and_coordinates) => {
                        change_floating_panes_coordinates(
                            env,
                            pane_ids_and_coordinates
                                .into_iter()
                                .map(|(p_id, coordinates)| (p_id.into(), coordinates))
                                .collect(),
                        )
                    },
                    PluginCommand::TogglePaneBorderless(pane_id) => {
                        toggle_pane_borderless(env, pane_id.into())
                    },
                    PluginCommand::SetPaneBorderStyle(pane_id, border_style) => {
                        set_pane_border_style(env, pane_id.into(), border_style)
                    },
                    PluginCommand::SetPaneBorderless(pane_id, borderless) => {
                        set_pane_borderless(env, pane_id.into(), borderless)
                    },
                    PluginCommand::SetPaneColor(pane_id, fg, bg) => {
                        set_pane_color(env, pane_id.into(), fg, bg)
                    },
                    PluginCommand::OpenFileNearPlugin(file_to_open, context) => {
                        open_file_near_plugin(env, file_to_open, context)
                    },
                    PluginCommand::OpenFileFloatingNearPlugin(
                        file_to_open,
                        floating_pane_coordinates,
                        context,
                    ) => open_file_floating_near_plugin(
                        env,
                        file_to_open,
                        floating_pane_coordinates,
                        context,
                    ),
                    PluginCommand::OpenFileInPlaceOfPlugin(
                        file_to_open,
                        close_plugin_after_replace,
                        context,
                    ) => open_file_in_place_of_plugin(
                        env,
                        file_to_open,
                        close_plugin_after_replace,
                        context,
                    ),
                    PluginCommand::GroupAndUngroupPanes(
                        panes_to_group,
                        panes_to_ungroup,
                        for_all_clients,
                    ) => group_and_ungroup_panes(
                        env,
                        panes_to_group.into_iter().map(|p| p.into()).collect(),
                        panes_to_ungroup.into_iter().map(|p| p.into()).collect(),
                        for_all_clients,
                    ),
                    PluginCommand::HighlightAndUnhighlightPanes(
                        panes_to_highlight,
                        panes_to_unhighlight,
                    ) => highlight_and_unhighlight_panes(
                        env,
                        panes_to_highlight.into_iter().map(|p| p.into()).collect(),
                        panes_to_unhighlight.into_iter().map(|p| p.into()).collect(),
                    ),
                    PluginCommand::CloseMultiplePanes(pane_ids) => {
                        close_multiple_panes(env, pane_ids.into_iter().map(|p| p.into()).collect())
                    },
                    PluginCommand::FloatMultiplePanes(pane_ids) => {
                        float_multiple_panes(env, pane_ids.into_iter().map(|p| p.into()).collect())
                    },
                    PluginCommand::EmbedMultiplePanes(pane_ids) => {
                        embed_multiple_panes(env, pane_ids.into_iter().map(|p| p.into()).collect())
                    },
                    PluginCommand::StartWebServer => start_web_server(env),
                    PluginCommand::StopWebServer => stop_web_server(env),
                    PluginCommand::QueryWebServerStatus => query_web_server_status(env),
                    PluginCommand::ShareCurrentSession => share_current_session(env),
                    PluginCommand::StopSharingCurrentSession => stop_sharing_current_session(env),
                    PluginCommand::SetSelfMouseSelectionSupport(selection_support) => {
                        set_self_mouse_selection_support(env, selection_support);
                    },
                    PluginCommand::SetSelfCollapsed(collapsed) => {
                        set_self_collapsed(env, collapsed);
                    },
                    PluginCommand::GenerateWebLoginToken(token_label, read_only) => {
                        generate_web_login_token(env, token_label, read_only);
                    },
                    PluginCommand::RevokeWebLoginToken(label) => {
                        revoke_web_login_token(env, label);
                    },
                    PluginCommand::ListWebLoginTokens => {
                        list_web_login_tokens(env);
                    },
                    PluginCommand::RevokeAllWebLoginTokens => {
                        revoke_all_web_login_tokens(env);
                    },
                    PluginCommand::RenameWebLoginToken(old_name, new_name) => {
                        rename_web_login_token(env, old_name, new_name);
                    },
                    PluginCommand::InterceptKeyPresses => intercept_key_presses(&mut env),
                    PluginCommand::ClearKeyPressesIntercepts => {
                        clear_key_presses_intercepts(&mut env)
                    },
                    PluginCommand::ReplacePaneWithExistingPane(
                        pane_id_to_replace,
                        existing_pane_id,
                        suppress_replaced_pane,
                    ) => replace_pane_with_existing_pane(
                        &mut env,
                        pane_id_to_replace.into(),
                        existing_pane_id.into(),
                        suppress_replaced_pane,
                    ),
                    PluginCommand::RunAction(action, context) => {
                        run_action(&env, action, context)?
                    }
                    PluginCommand::GetSessionEnvironmentVariables => {
                        get_session_environment_variables(env);
                    },
                    PluginCommand::OpenCommandPaneInNewTab(command_to_run, context) => {
                        open_command_pane_in_new_tab(env, command_to_run, context);
                    },
                    PluginCommand::OpenPluginPaneInNewTab {
                        plugin_url,
                        configuration,
                        context,
                    } => {
                        open_plugin_pane_in_new_tab(env, plugin_url, configuration, context);
                    },
                    PluginCommand::OpenPluginPaneFloating {
                        plugin_url,
                        configuration,
                        floating_pane_coordinates,
                        context,
                    } => {
                        open_plugin_pane_floating(
                            env,
                            plugin_url,
                            configuration,
                            floating_pane_coordinates,
                            context,
                        );
                    },
                    PluginCommand::OpenEditorPaneInNewTab(file_to_open, context) => {
                        open_editor_pane_in_new_tab(env, file_to_open, context);
                    },
                    PluginCommand::OpenCommandPaneInPlaceOfPaneId(
                        pane_id,
                        command_to_run,
                        close_replaced_pane,
                        context,
                    ) => open_command_pane_in_place_of_pane_id(
                        env,
                        pane_id,
                        command_to_run,
                        close_replaced_pane,
                        context,
                    ),
                    PluginCommand::OpenTerminalPaneInPlaceOfPaneId(
                        pane_id,
                        cwd,
                        close_replaced_pane,
                    ) => open_terminal_pane_in_place_of_pane_id(
                        env,
                        pane_id,
                        cwd,
                        close_replaced_pane,
                    ),
                    PluginCommand::OpenEditPaneInPlaceOfPaneId(
                        pane_id,
                        file_to_open,
                        close_replaced_pane,
                        context,
                    ) => open_edit_pane_in_place_of_pane_id(
                        env,
                        pane_id,
                        file_to_open,
                        close_replaced_pane,
                        context,
                    ),
                    PluginCommand::ShowFloatingPanes { tab_id } => show_floating_panes(env, tab_id),
                    PluginCommand::HideFloatingPanes { tab_id } => hide_floating_panes(env, tab_id),
                    PluginCommand::SetPaneRegexHighlights(pane_id, highlights) => {
                        set_pane_regex_highlights(env, pane_id, highlights)
                    },
                    PluginCommand::ClearPaneHighlights(pane_id) => {
                        clear_pane_highlights(env, pane_id)
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
    if event_list.contains(&EventType::SessionCountsUpdate)
        || event_list.contains(&EventType::FolderSessionsUpdate)
    {
        let _ = env
            .senders
            .send_to_background_jobs(BackgroundJob::SessionSuggestions(
                SessionSuggestionsJob::Subscribe {
                    plugin_id: env.plugin_id,
                    client_id: env.client_id,
                },
            ));
    }
    env.senders
        .send_to_plugin(PluginInstruction::PluginSubscribedToEvents(
            env.plugin_id,
            acting_client(env),
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

fn set_cli_pipe_exit_code(env: &PluginEnv, pipe_name: String, exit_code: i32) -> Result<()> {
    env.senders
        .send_to_plugin(PluginInstruction::SetCliPipeExitCode {
            pipe_id: pipe_name,
            exit_code,
            plugin_id: env.plugin_id,
        })
        .context("failed to set pipe exit code")
}

fn message_to_plugin(env: &PluginEnv, mut message_to_plugin: MessageToPlugin) -> Result<()> {
    if message_to_plugin.plugin_url.as_ref().map(|s| s.as_str()) == Some("zellij:OWN_URL") {
        message_to_plugin.plugin_url = Some(env.plugin.location.display());
    }
    if !message_to_plugin.has_cwd() {
        message_to_plugin =
            message_to_plugin.new_plugin_instance_should_have_cwd(env.plugin_cwd.clone());
    }
    if !message_to_plugin.plugin_config.contains_key("caller_cwd") {
        message_to_plugin.plugin_config.insert(
            "caller_cwd".to_owned(),
            env.plugin_cwd.display().to_string(),
        );
    }
    env.senders
        .send_to_plugin(PluginInstruction::MessageFromPlugin {
            source_plugin_id: env.plugin_id,
            message: message_to_plugin,
        })
        .context("failed to send message to plugin")
}

fn unsubscribe(env: &PluginEnv, event_list: HashSet<EventType>) -> Result<()> {
    let still_subscribed_to_sessions = {
        let mut subscriptions = env.subscriptions.lock().to_anyhow()?;
        subscriptions.retain(|k| !event_list.contains(k));
        subscriptions.contains(&EventType::SessionCountsUpdate)
            || subscriptions.contains(&EventType::FolderSessionsUpdate)
    };
    if !still_subscribed_to_sessions
        && (event_list.contains(&EventType::SessionCountsUpdate)
            || event_list.contains(&EventType::FolderSessionsUpdate))
    {
        let _ = env
            .senders
            .send_to_background_jobs(BackgroundJob::SessionSuggestions(
                SessionSuggestionsJob::Unsubscribe {
                    plugin_id: env.plugin_id,
                },
            ));
    }
    Ok(())
}

fn set_selectable(env: &PluginEnv, selectable: bool) {
    env.senders
        .send_to_screen(ScreenInstruction::SetSelectable(
            PaneId::Plugin(self_pane_id(env)),
            selectable,
        ))
        .with_context(|| {
            format!(
                "failed to set plugin {} selectable from plugin {}",
                selectable,
                env.name()
            )
        })
        .non_fatal();
}

fn show_cursor(env: &PluginEnv, cursor_position: Option<(usize, usize)>) {
    env.senders
        .send_to_screen(ScreenInstruction::ShowPluginCursor(
            self_pane_id(env),
            acting_client(env),
            cursor_position,
        ))
        .with_context(|| {
            format!(
                "failed to {} plugin cursor from plugin {}",
                if cursor_position.is_some() {
                    "show"
                } else {
                    "hide"
                },
                env.name()
            )
        })
        .non_fatal();
}

fn request_permission(env: &PluginEnv, permissions: Vec<PermissionType>) -> Result<()> {
    let permission_cache_key = permission_cache_key(env);
    if env.plugin.is_builtin()
        || PermissionCache::from_path_or_default(None)
            .check_permissions(permission_cache_key, &permissions)
    {
        // already granted for these exact module bytes, no prompt is involved
        return env
            .senders
            .send_to_plugin(PluginInstruction::GrantKnownPermissions(
                env.plugin_id,
                Some(acting_client(env)),
                permissions.to_vec(),
                env.generation,
                env.plugin_digest.clone(),
            ));
    }

    // we do this so that messages that have arrived while the user is seeing the permission screen
    // will be cached and reapplied once the permission is granted
    let prompt_id = next_permission_prompt_id();
    let _ = env
        .senders
        .send_to_plugin(PluginInstruction::CachePluginEvents {
            plugin_id: env.plugin_id,
            generation: env.generation,
            digest: env.plugin_digest.clone(),
            prompt_id,
        });

    env.senders
        .send_to_screen(ScreenInstruction::RequestPluginPermissions(
            permission_pane_id(env),
            PluginPermission::new(env.plugin.location.to_string(), permissions, prompt_id),
        ))
}

fn permission_cache_key(env: &PluginEnv) -> String {
    format!("{}#{}", env.plugin.location, env.plugin_digest)
}

fn get_plugin_ids(env: &PluginEnv) {
    let ids = PluginIds {
        plugin_id: env.plugin_id,
        zellij_pid: process::id(),
        initial_cwd: env.plugin_cwd.clone(),
        client_id: acting_client(env),
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

fn generate_random_name(env: &PluginEnv) {
    let name = generate_random_name_impl();
    let response = ProtobufGenerateRandomNameResponse { name };
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| {
            format!(
                "failed to send generated random name to plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn dump_layout(env: &PluginEnv, layout_name: String) {
    let layout_path = PathBuf::from(&layout_name);

    let layout_dir = env.layout_dir.clone().or_else(default_layout_dir);
    let response = match Layout::stringified_from_dir(&layout_path, layout_dir.as_ref()) {
        Ok((_, layout_content, _)) => ProtobufDumpLayoutResponse {
            result: Some(dump_layout_response::Result::LayoutContent(layout_content)),
        },
        Err(e) => ProtobufDumpLayoutResponse {
            result: Some(dump_layout_response::Result::Error(e.to_string())),
        },
    };

    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send layout dump to plugin {}", env.name()))
        .non_fatal();
}

fn get_layout_dir(env: &PluginEnv) {
    // Get layout dir from env or fall back to default
    let layout_dir = env.layout_dir.clone().or_else(default_layout_dir);

    // Convert PathBuf to String
    let layout_dir_string = layout_dir
        .and_then(|path| path.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| String::from(""));

    let response = ProtobufGetLayoutDirResponse {
        layout_dir: layout_dir_string,
    };

    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send layout dir to plugin {}", env.name()))
        .non_fatal();
}

fn get_session_environment_variables(env: &PluginEnv) {
    use zellij_utils::plugin_api::plugin_command::EnvVariable as ProtobufEnvVariable;

    let env_vars: Vec<ProtobufEnvVariable> = env
        .session_env_vars
        .iter()
        .map(|(name, value)| ProtobufEnvVariable {
            name: name.clone(),
            value: value.clone(),
        })
        .collect();

    let response = ProtobufGetSessionEnvironmentVariablesResponse { env_vars };

    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| {
            format!(
                "failed to send session environment variables to plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn open_command_pane_in_new_tab(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let args = command_to_run.args;
    let hold_on_close = true;
    let hold_on_start = false;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction: None,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title: false,
        notify: false,
    };
    let initial_panes = Some(vec![CommandOrPlugin::Command(run_command_action)]);
    let action = Action::NewTab {
        tiled_layout: None,
        floating_layouts: vec![],
        swap_tiled_layouts: None,
        swap_floating_layouts: None,
        tab_name: None,
        should_change_focus_to_new_tab: true,
        cwd: None,
        initial_panes,
        first_pane_unblock_condition: None,
    };
    let error_msg = || format!("Failed to open command pane in new tab");
    let result = apply_action!(action, error_msg, env);

    let tab_id = result.as_ref().and_then(|r| r.affected_tab_id);
    let pane_id = result
        .as_ref()
        .and_then(|r| r.affected_pane_id)
        .map(|p| p.into());
    let response =
        ProtobufOpenPaneInNewTabResponse::from(OpenPaneInNewTabResponse { tab_id, pane_id });
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_in_new_tab response"))
        .non_fatal();
}

fn open_plugin_pane_in_new_tab(
    env: &PluginEnv,
    plugin_url: String,
    configuration: BTreeMap<String, String>,
    context: BTreeMap<String, String>,
) {
    let run_plugin_or_alias = RunPluginOrAlias::from_url(
        &plugin_url,
        &Some(configuration),
        None,
        Some(env.plugin_cwd.clone()),
    );
    let run_plugin_or_alias = match run_plugin_or_alias {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to parse plugin url '{}': {}", plugin_url, e);
            let response =
                ProtobufOpenPaneInNewTabResponse::from(OpenPaneInNewTabResponse::default());
            wasi_write_object(env, &response.encode_to_vec())
                .with_context(|| {
                    format!("failed to write open_plugin_pane_in_new_tab error response")
                })
                .non_fatal();
            return;
        },
    };
    let _ = context; // context is not currently used for plugin panes
    let initial_panes = Some(vec![CommandOrPlugin::Plugin(run_plugin_or_alias)]);
    let action = Action::NewTab {
        tiled_layout: None,
        floating_layouts: vec![],
        swap_tiled_layouts: None,
        swap_floating_layouts: None,
        tab_name: None,
        should_change_focus_to_new_tab: true,
        cwd: None,
        initial_panes,
        first_pane_unblock_condition: None,
    };
    let error_msg = || format!("Failed to open plugin pane in new tab");
    let result = apply_action!(action, error_msg, env);

    let tab_id = result.as_ref().and_then(|r| r.affected_tab_id);
    let pane_id = result
        .as_ref()
        .and_then(|r| r.affected_pane_id)
        .map(|p| p.into());
    let response =
        ProtobufOpenPaneInNewTabResponse::from(OpenPaneInNewTabResponse { tab_id, pane_id });
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_plugin_pane_in_new_tab response"))
        .non_fatal();
}

fn open_plugin_pane_floating(
    env: &PluginEnv,
    plugin_url: String,
    configuration: BTreeMap<String, String>,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let run_plugin_or_alias = RunPluginOrAlias::from_url(
        &plugin_url,
        &Some(configuration),
        None,
        Some(env.plugin_cwd.clone()),
    );
    let run_plugin_or_alias = match run_plugin_or_alias {
        Ok(r) => r,
        Err(e) => {
            log::error!("Failed to parse plugin url '{}': {}", plugin_url, e);
            let response = ProtobufOpenPluginPaneFloatingResponse::from(
                OpenPluginPaneFloatingResponse::default(),
            );
            wasi_write_object(env, &response.encode_to_vec())
                .with_context(|| {
                    format!("failed to write open_plugin_pane_floating error response")
                })
                .non_fatal();
            return;
        },
    };
    let _ = context; // context is not currently used for plugin panes
    let action = Action::NewFloatingPluginPane {
        plugin: run_plugin_or_alias,
        pane_name: None,
        skip_cache: false,
        cwd: Some(env.plugin_cwd.clone()),
        coordinates: floating_pane_coordinates,
        no_focus: false,
        tab_id: None,
    };
    let error_msg = || format!("Failed to open floating plugin pane");
    let result = apply_action!(action, error_msg, env);

    let pane_id: OpenPluginPaneFloatingResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());
    let response = ProtobufOpenPluginPaneFloatingResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_plugin_pane_floating response"))
        .non_fatal();
}

fn open_editor_pane_in_new_tab(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    context: BTreeMap<String, String>,
) {
    let _ = context; // context is not currently used for editor panes in new tab
    let path = translate_plugin_path(env, file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let file_to_open = FileToOpen {
        border_style: None,
        path,
        cwd,
        ..file_to_open
    };
    let initial_panes = Some(vec![CommandOrPlugin::File(file_to_open)]);
    let action = Action::NewTab {
        tiled_layout: None,
        floating_layouts: vec![],
        swap_tiled_layouts: None,
        swap_floating_layouts: None,
        tab_name: None,
        should_change_focus_to_new_tab: true,
        cwd: None,
        initial_panes,
        first_pane_unblock_condition: None,
    };
    let error_msg = || format!("Failed to open editor pane in new tab");
    let result = apply_action!(action, error_msg, env);

    let tab_id = result.as_ref().and_then(|r| r.affected_tab_id);
    let pane_id = result
        .as_ref()
        .and_then(|r| r.affected_pane_id)
        .map(|p| p.into());
    let response =
        ProtobufOpenPaneInNewTabResponse::from(OpenPaneInNewTabResponse { tab_id, pane_id });
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_editor_pane_in_new_tab response"))
        .non_fatal();
}

fn get_focused_pane_info(env: &PluginEnv) {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    let err_context = || {
        format!(
            "failed to get focused pane info for client {:?} from plugin {}",
            acting_client(env),
            env.name()
        )
    };

    // Create oneshot channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Send request to screen thread
    env.senders
        .send_to_screen(ScreenInstruction::GetFocusedPaneInfo {
            client_id: acting_client(env),
            response_channel: response_sender,
        })
        .with_context(err_context)
        .non_fatal();

    // Block waiting for response with 100ms timeout
    let response = match response_receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetFocusedPaneInfo timed out for plugin {} for client {:?}",
                env.plugin_id,
                acting_client(env)
            );
            GetFocusedPaneInfoResponse::Err("Timeout retrieving focused pane info".to_string())
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetFocusedPaneInfo channel disconnected for plugin {}",
                env.plugin_id
            );
            GetFocusedPaneInfoResponse::Err(
                "Channel disconnected while retrieving focused pane info".to_string(),
            )
        },
    };

    // Convert to protobuf and write response back to plugin
    let protobuf_response = ProtobufGetFocusedPaneInfoResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(err_context)
        .non_fatal();
}

fn parse_layout(env: &PluginEnv, layout_string: String) {
    use std::time::{SystemTime, UNIX_EPOCH};

    // Parse the KDL layout - same parameters as list_available_layouts in layout.rs:1260
    let parse_result = Layout::from_kdl(
        &layout_string,
        Some("parse_layout_api".to_string()), // file_name for error reporting
        None,                                 // no swap layouts
        None,                                 // no cwd
    );

    let response = match parse_result {
        Ok(layout) => {
            // Extract tabs from layout
            // This logic matches LayoutMetadata::from in data.rs:1762-1772
            let layout_tabs = layout.tabs();
            let tabs = if layout_tabs.is_empty() {
                // Use default template if no explicit tabs defined
                let (tiled_pane_layout, floating_pane_layout) = layout.new_tab();
                vec![TabMetadata::from(&(
                    None,
                    tiled_pane_layout,
                    floating_pane_layout,
                ))]
            } else {
                layout
                    .tabs()
                    .into_iter()
                    .map(|tab| TabMetadata::from(&tab))
                    .collect()
            };

            // Get current time as Unix epoch timestamp
            let current_time = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
                .to_string();

            let layout_metadata = LayoutMetadata {
                tabs,
                creation_time: current_time.clone(),
                update_time: current_time,
            };

            // Convert LayoutMetadata to protobuf
            match layout_metadata.try_into() {
                Ok(protobuf_metadata) => ProtobufParseLayoutResponse {
                    result: Some(parse_layout_response::Result::Metadata(protobuf_metadata)),
                },
                Err(e) => {
                    log::error!("Failed to convert metadata to protobuf: {}", e);
                    // Fallback to SyntaxError if conversion fails
                    let error = LayoutParsingError::SyntaxError;
                    let protobuf_error =
                        error
                            .try_into()
                            .unwrap_or_else(|_| ProtobufLayoutParsingError {
                                error_type: Some(ProtobufLayoutParsingErrorType::SyntaxError(
                                    ProtobufSyntaxError {},
                                )),
                            });
                    ProtobufParseLayoutResponse {
                        result: Some(parse_layout_response::Result::Error(protobuf_error)),
                    }
                },
            }
        },
        Err(config_error) => {
            // Build LayoutParsingError following list_available_layouts pattern
            // See zellij-utils/src/input/layout.rs:1238-1245
            let file_name = "parse_layout_api".to_string();
            let source_code = layout_string;

            let error = match config_error {
                ConfigError::KdlError(kdl_err) => LayoutParsingError::KdlError {
                    kdl_error: kdl_err,
                    file_name,
                    source_code,
                },
                _ => LayoutParsingError::SyntaxError,
            };

            // Convert LayoutParsingError to protobuf
            // TryFrom is implemented in zellij-utils/src/plugin_api/event.rs:1293-1314
            let protobuf_error = error.try_into().unwrap_or_else(|e| {
                log::error!("Failed to convert error to protobuf: {}", e);
                ProtobufLayoutParsingError {
                    error_type: Some(ProtobufLayoutParsingErrorType::SyntaxError(
                        ProtobufSyntaxError {},
                    )),
                }
            });

            ProtobufParseLayoutResponse {
                result: Some(parse_layout_response::Result::Error(protobuf_error)),
            }
        },
    };

    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| {
            format!(
                "failed to send parse layout response to plugin {}",
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
    let border_style = file_to_open.border_style;
    let path = translate_plugin_path(env, file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let action = Action::EditFile {
        payload: OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        ),
        direction: None,
        floating,
        in_place,
        close_replaced_pane: false,
        start_suppressed,
        coordinates: FloatingPaneCoordinates::merge_border_style(None, border_style),
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenFileResponse = result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file response"))
        .non_fatal();
}

fn run_action(
    env: &PluginEnv,
    mut action: Action,
    context: BTreeMap<String, String>,
) -> Result<()> {
    // Clone the necessary data to move into the thread
    action.populate_originating_plugin(OriginatingPlugin::new(
        env.plugin_id,
        acting_client(env),
        context.clone(),
    ));
    let action_clone = action.clone();
    let client_id = acting_client(env);
    let plugin_id = env.plugin_id;
    let scheduled_generation = env.generation;
    let senders = env.senders.clone();
    let default_shell = env.default_shell.clone();
    let default_mode = env.default_mode.clone();
    let plugin_name = env.name().to_string();
    let spawned_for = plugin_name.clone();
    let Some(permit) = try_acquire_run_action(plugin_id) else {
        log::warn!(
            "Not running an action for plugin {}: run-action limit reached",
            plugin_id
        );
        return Err(anyhow!("run-action limit reached for plugin {plugin_name}"));
    };

    // The permit is dropped when this thread finishes, including a panic.
    // A failed spawn drops the closure, which drops the permit.
    thread::Builder::new()
        .name("run_action".to_owned())
        .spawn(move || {
            let _permit = permit;
            // Execute the action and capture the result
            let pane_id = match route_action(
                action,
                client_id,
                None,
                Some(PaneId::Plugin(plugin_id)),
                senders.clone(),
                default_shell,
                None,
                default_mode,
                None,
                false,
            ) {
                Ok((_should_break, result)) => {
                    // Extract pane_id from ActionCompletionResult
                    result.and_then(|r| r.affected_pane_id)
                },
                Err(e) => {
                    log::error!("failed to run action in plugin {}: {:?}", plugin_name, e);
                    None
                },
            };

            // After action completes, send ActionComplete event with context.
            // Convert server PaneId to zellij_utils PaneId. The generation stays
            // on the instruction; it is not part of the guest context.
            let pane_id_for_event = pane_id.map(|p| p.into());
            if let Err(e) = senders.send_to_plugin(PluginInstruction::ApplyLoadResult {
                plugin_id,
                client_id,
                event: Event::ActionComplete(action_clone, pane_id_for_event, context),
                generation: scheduled_generation,
                reservation: None,
            }) {
                log::error!("Failed to send ActionComplete event: {:?}", e);
            }
        })
        .map_err(|error| {
            anyhow!("failed to spawn run_action thread for plugin {spawned_for}: {error}")
        })?;
    Ok(())
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
    let border_style = file_to_open.border_style;
    let path = translate_plugin_path(env, file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let action = Action::EditFile {
        payload: OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        ),
        direction: None,
        floating,
        in_place,
        close_replaced_pane: false,
        start_suppressed,
        coordinates: FloatingPaneCoordinates::merge_border_style(
            floating_pane_coordinates,
            border_style,
        ),
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenFileFloatingResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileFloatingResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file_floating response"))
        .non_fatal();
}

fn open_file_in_place(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let floating = false;
    let in_place = true;
    let start_suppressed = false;
    let path = translate_plugin_path(env, file_to_open.path);
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));

    let action = Action::EditFile {
        payload: OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        ),
        direction: None,
        floating,
        in_place,
        close_replaced_pane: false,
        start_suppressed,
        coordinates: None,
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenFileInPlaceResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileInPlaceResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file_in_place response"))
        .non_fatal();
}

fn open_file_near_plugin(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    context: BTreeMap<String, String>,
) {
    let border_style = file_to_open.border_style;
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let path = translate_plugin_path(env, file_to_open.path);
    let open_file_payload =
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        );
    let title = format!("Editing: {}", open_file_payload.path.display());
    let start_suppressed = false;
    let open_file = TerminalAction::OpenFile(open_file_payload);

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let pty_instr = PtyInstruction::SpawnTerminal(
        Some(open_file),
        Some(title),
        NewPanePlacement::default().with_border_style(border_style),
        start_suppressed,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    );
    let _ = env.senders.send_to_pty(pty_instr);

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_file_near_plugin", false);
    let pane_id: OpenFileNearPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file_near_plugin response"))
        .non_fatal();
}

fn open_file_floating_near_plugin(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let floating_pane_coordinates = FloatingPaneCoordinates::merge_border_style(
        floating_pane_coordinates,
        file_to_open.border_style,
    );
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let path = translate_plugin_path(env, file_to_open.path);
    let open_file_payload =
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        );
    let title = format!("Editing: {}", open_file_payload.path.display());
    let start_suppressed = false;
    let open_file = TerminalAction::OpenFile(open_file_payload);

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let pty_instr = PtyInstruction::SpawnTerminal(
        Some(open_file),
        Some(title),
        NewPanePlacement::Floating(floating_pane_coordinates),
        start_suppressed,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    );
    let _ = env.senders.send_to_pty(pty_instr);

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_file_floating_near_plugin", false);
    let pane_id: OpenFileFloatingNearPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileFloatingNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file_floating_near_plugin response"))
        .non_fatal();
}

fn open_file_in_place_of_plugin(
    env: &PluginEnv,
    file_to_open: FileToOpen,
    close_plugin_after_replace: bool,
    context: BTreeMap<String, String>,
) {
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let path = translate_plugin_path(env, file_to_open.path);
    let open_file_payload =
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        );
    let title = format!("Editing: {}", open_file_payload.path.display());
    let open_file = TerminalAction::OpenFile(open_file_payload);

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let pty_instr = PtyInstruction::SpawnInPlaceTerminal(
        Some(open_file),
        Some(title),
        close_plugin_after_replace,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        None,
    );
    let _ = env.senders.send_to_pty(pty_instr);

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_file_in_place_of_plugin", false);
    let pane_id: OpenFileInPlaceOfPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenFileInPlaceOfPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_file_in_place_of_plugin response"))
        .non_fatal();
}

fn open_terminal(env: &PluginEnv, cwd: PathBuf, border_style: Option<BorderStyleOverride>) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewTiledPane {
        direction: None,
        command: run_command_action,
        pane_name: None,
        near_current_pane: false,
        no_focus: false,
        borderless: None,
        border_style,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenTerminalResponse = result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal response"))
        .non_fatal();
}

fn open_terminal_near_plugin(
    env: &PluginEnv,
    cwd: PathBuf,
    border_style: Option<BorderStyleOverride>,
) {
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    let name = None;
    default_shell.change_cwd(cwd);

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(default_shell),
        name,
        NewPanePlacement::Tiled {
            direction: None,
            borderless: None,
            border_style,
        },
        false,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    ));

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_terminal_near_plugin", false);
    let pane_id: OpenTerminalNearPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_near_plugin response"))
        .non_fatal();
}

fn open_terminal_floating(
    env: &PluginEnv,
    cwd: PathBuf,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewFloatingPane {
        command: run_command_action,
        pane_name: None,
        coordinates: floating_pane_coordinates,
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenTerminalFloatingResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalFloatingResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_floating response"))
        .non_fatal();
}

fn open_terminal_floating_near_plugin(
    env: &PluginEnv,
    cwd: PathBuf,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
) {
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let name = None;

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(default_shell),
        name,
        NewPanePlacement::Floating(floating_pane_coordinates),
        false,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    ));

    // Wait for completion
    let result =
        wait_for_action_completion(completion_rx, "open_terminal_floating_near_plugin", false);
    let pane_id: OpenTerminalFloatingNearPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalFloatingNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_floating_near_plugin response"))
        .non_fatal();
}

fn open_terminal_in_place(env: &PluginEnv, cwd: PathBuf) {
    let error_msg = || format!("failed to open file in plugin {}", env.name());
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewInPlacePane {
        command: run_command_action,
        pane_name: None,
        near_current_pane: false,
        no_focus: false,
        pane_id_to_replace: None,
        close_replaced_pane: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenTerminalInPlaceResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalInPlaceResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_in_place response"))
        .non_fatal();
}

fn open_terminal_in_place_of_plugin(
    env: &PluginEnv,
    cwd: PathBuf,
    close_plugin_after_replace: bool,
) {
    let cwd = translate_plugin_path(env, cwd);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd);
    let name = None;

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env
        .senders
        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
            Some(default_shell),
            name,
            close_plugin_after_replace,
            ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
            Some(NotificationEnd::new(completion_tx)),
            None,
        ));

    // Wait for completion
    let result =
        wait_for_action_completion(completion_rx, "open_terminal_in_place_of_plugin", false);
    let pane_id: OpenTerminalInPlaceOfPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenTerminalInPlaceOfPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_in_place_of_plugin response"))
        .non_fatal();
}

fn open_command_pane_in_place_of_plugin(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    close_plugin_after_replace: bool,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let _border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env
        .senders
        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
            Some(run_cmd),
            name,
            close_plugin_after_replace,
            ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
            Some(NotificationEnd::new(completion_tx)),
            None,
        ));

    // Wait for completion
    let result =
        wait_for_action_completion(completion_rx, "open_command_pane_in_place_of_plugin", false);
    let pane_id: OpenCommandPaneInPlaceOfPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneInPlaceOfPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_in_place_of_plugin response"))
        .non_fatal();
}

fn open_terminal_pane_in_place_of_pane_id(
    env: &PluginEnv,
    pane_id_to_replace: zellij_utils::data::PaneId,
    cwd: FileToOpen,
    close_replaced_pane: bool,
) {
    let cwd_path = translate_plugin_path(env, cwd.path);
    let mut default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    default_shell.change_cwd(cwd_path);
    let name = None;

    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env
        .senders
        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
            Some(default_shell),
            name,
            close_replaced_pane,
            ClientTabIndexOrPaneId::PaneId(pane_id_to_replace.into()),
            Some(NotificationEnd::new(completion_tx)),
            None,
        ));

    let result = wait_for_action_completion(
        completion_rx,
        "open_terminal_pane_in_place_of_pane_id",
        false,
    );
    let pane_id: OpenTerminalPaneInPlaceOfPaneIdResponse =
        result.affected_pane_id.map(|p| p.into());

    let response = ProtobufOpenTerminalPaneInPlaceOfPaneIdResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_terminal_pane_in_place_of_pane_id response"))
        .non_fatal();
}

fn open_command_pane_in_place_of_pane_id(
    env: &PluginEnv,
    pane_id_to_replace: zellij_utils::data::PaneId,
    command_to_run: CommandToRun,
    close_replaced_pane: bool,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let _border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false;
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());

    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env
        .senders
        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
            Some(run_cmd),
            name,
            close_replaced_pane,
            ClientTabIndexOrPaneId::PaneId(pane_id_to_replace.into()),
            Some(NotificationEnd::new(completion_tx)),
            None,
        ));

    let result = wait_for_action_completion(
        completion_rx,
        "open_command_pane_in_place_of_pane_id",
        false,
    );
    let pane_id: OpenCommandPaneInPlaceOfPaneIdResponse = result.affected_pane_id.map(|p| p.into());

    let response = ProtobufOpenCommandPaneInPlaceOfPaneIdResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_in_place_of_pane_id response"))
        .non_fatal();
}

fn open_edit_pane_in_place_of_pane_id(
    env: &PluginEnv,
    pane_id_to_replace: zellij_utils::data::PaneId,
    file_to_open: FileToOpen,
    close_replaced_pane: bool,
    context: BTreeMap<String, String>,
) {
    let cwd = file_to_open
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let path = translate_plugin_path(env, file_to_open.path);
    let open_file_payload =
        OpenFilePayload::new(path, file_to_open.line_number, cwd).with_originating_plugin(
            OriginatingPlugin::new(env.plugin_id, acting_client(env), context),
        );
    let title = format!("Editing: {}", open_file_payload.path.display());
    let open_file = TerminalAction::OpenFile(open_file_payload);

    let (completion_tx, completion_rx) = oneshot::channel();
    let pty_instr = PtyInstruction::SpawnInPlaceTerminal(
        Some(open_file),
        Some(title),
        close_replaced_pane,
        ClientTabIndexOrPaneId::PaneId(pane_id_to_replace.into()),
        Some(NotificationEnd::new(completion_tx)),
        None,
    );
    let _ = env.senders.send_to_pty(pty_instr);

    let result =
        wait_for_action_completion(completion_rx, "open_edit_pane_in_place_of_pane_id", false);
    let pane_id: OpenEditPaneInPlaceOfPaneIdResponse = result.affected_pane_id.map(|p| p.into());

    let response = ProtobufOpenEditPaneInPlaceOfPaneIdResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_edit_pane_in_place_of_pane_id response"))
        .non_fatal();
}

fn open_command_pane(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let action = Action::NewTiledPane {
        direction,
        command: Some(run_command_action),
        pane_name: name,
        near_current_pane: false,
        no_focus: false,
        borderless: None,
        border_style,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenCommandPaneResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane response"))
        .non_fatal();
}

fn open_command_pane_near_plugin(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(run_cmd),
        name,
        NewPanePlacement::Tiled {
            direction: None,
            borderless: None,
            border_style,
        },
        false,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    ));

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_command_pane_near_plugin", false);
    let pane_id: OpenCommandPaneNearPluginResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_near_plugin response"))
        .non_fatal();
}

fn open_command_pane_floating(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let action = Action::NewFloatingPane {
        command: Some(run_command_action),
        pane_name: name,
        coordinates: FloatingPaneCoordinates::merge_border_style(
            floating_pane_coordinates,
            border_style,
        ),
        near_current_pane: false,
        no_focus: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenCommandPaneFloatingResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneFloatingResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_floating response"))
        .non_fatal();
}

fn open_command_pane_floating_near_plugin(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(run_cmd),
        name,
        NewPanePlacement::Floating(FloatingPaneCoordinates::merge_border_style(
            floating_pane_coordinates,
            border_style,
        )),
        false,
        ClientTabIndexOrPaneId::PaneId(PaneId::Plugin(self_pane_id(env))),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    ));

    // Wait for completion
    let result = wait_for_action_completion(
        completion_rx,
        "open_command_pane_floating_near_plugin",
        false,
    );
    let pane_id: OpenCommandPaneFloatingNearPluginResponse =
        result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneFloatingNearPluginResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_floating_near_plugin response"))
        .non_fatal();
}

fn open_command_pane_in_place(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let error_msg = || format!("failed to open command in plugin {}", env.name());
    let command = command_to_run.path;
    let _border_style = command_to_run.border_style;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let action = Action::NewInPlacePane {
        command: Some(run_command_action),
        pane_name: name,
        near_current_pane: false,
        no_focus: false,
        pane_id_to_replace: None,
        close_replaced_pane: false,
        tab_id: None,
    };
    let result = apply_action!(action, error_msg, env);

    // Extract pane_id from result and convert to zellij_utils PaneId
    let pane_id: OpenCommandPaneInPlaceResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneInPlaceResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_in_place response"))
        .non_fatal();
}

fn open_command_pane_background(
    env: &PluginEnv,
    command_to_run: CommandToRun,
    context: BTreeMap<String, String>,
) {
    let command = command_to_run.path;
    let cwd = command_to_run
        .cwd
        .map(|cwd| translate_plugin_path(env, cwd))
        .or_else(|| Some(env.plugin_cwd.clone()));
    let args = command_to_run.args;
    let direction = None;
    let hold_on_close = true;
    let hold_on_start = false;
    let start_suppressed = true;
    let name = None;
    let use_terminal_title = false; // TODO: support this
    let run_command_action = RunCommandAction {
        command,
        args,
        cwd,
        direction,
        hold_on_close,
        hold_on_start,
        originating_plugin: Some(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        use_terminal_title,
        notify: false,
    };
    let run_cmd = TerminalAction::RunCommand(run_command_action.into());

    // Create completion channel
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env.senders.send_to_pty(PtyInstruction::SpawnTerminal(
        Some(run_cmd),
        name,
        NewPanePlacement::default(),
        start_suppressed,
        ClientTabIndexOrPaneId::ClientId(acting_client(env)),
        Some(NotificationEnd::new(completion_tx)),
        false, // set_blocking
        None,
    ));

    // Wait for completion
    let result = wait_for_action_completion(completion_rx, "open_command_pane_background", false);
    let pane_id: OpenCommandPaneBackgroundResponse = result.affected_pane_id.map(|p| p.into());

    // Write response to plugin
    let response = ProtobufOpenCommandPaneBackgroundResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write open_command_pane_background response"))
        .non_fatal();
}

fn rerun_command_pane(env: &PluginEnv, terminal_pane_id: u32) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::RerunCommandPane(terminal_pane_id, None));
}

fn switch_tab_to(env: &PluginEnv, tab_idx: u32) {
    env.senders
        .send_to_screen(ScreenInstruction::GoToTab(
            tab_idx,
            Some(acting_client(env)),
            None,
        ))
        .with_context(|| {
            format!(
                "failed to switch to tab {tab_idx} from plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

const MAX_DIRECTORY_SCANS_IN_FLIGHT: usize = 4;
const MAX_DIRECTORY_SCANS_PER_PLUGIN: usize = 1;
const MAX_DIRECTORY_SCAN_ENTRIES: usize = 4096;
const MAX_PLUGIN_TIMERS_PER_PLUGIN: usize = 64;
const MAX_PLUGIN_TIMERS_IN_FLIGHT: usize = 256;
// RunAction starts an OS thread. Stay with the directory-scan thread cap,
// not the 64/256 timer tasks.
const MAX_RUN_ACTIONS_PER_PLUGIN: usize = 4;
const MAX_RUN_ACTIONS_IN_FLIGHT: usize = 16;
// Separate counters from run actions. Same caps, not higher.
#[cfg(test)]
const MAX_RUN_COMMANDS_PER_PLUGIN: usize = MAX_RUN_ACTIONS_PER_PLUGIN;
#[cfg(test)]
const MAX_RUN_COMMANDS_IN_FLIGHT: usize = MAX_RUN_ACTIONS_IN_FLIGHT;
#[cfg(test)]
const MAX_WEB_REQUESTS_PER_PLUGIN: usize = MAX_RUN_ACTIONS_PER_PLUGIN;
#[cfg(test)]
const MAX_WEB_REQUESTS_IN_FLIGHT: usize = MAX_RUN_ACTIONS_IN_FLIGHT;

struct InFlightLimits {
    total: usize,
    per_plugin: BTreeMap<PluginId, usize>,
}

impl InFlightLimits {
    const fn new() -> Self {
        Self {
            total: 0,
            per_plugin: BTreeMap::new(),
        }
    }

    fn try_acquire(
        &mut self,
        plugin_id: PluginId,
        per_plugin_cap: usize,
        total_cap: usize,
    ) -> bool {
        let plugin_count = self.per_plugin.get(&plugin_id).copied().unwrap_or(0);
        if self.total >= total_cap || plugin_count >= per_plugin_cap {
            return false;
        }
        self.total += 1;
        self.per_plugin.insert(plugin_id, plugin_count + 1);
        true
    }

    fn release(&mut self, plugin_id: PluginId) {
        self.total = self.total.saturating_sub(1);
        let Some(count) = self.per_plugin.get_mut(&plugin_id) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count == 0 {
            self.per_plugin.remove(&plugin_id);
        }
    }
}

static DIRECTORY_SCANS: Mutex<InFlightLimits> = Mutex::new(InFlightLimits::new());
static PLUGIN_TIMERS: Mutex<InFlightLimits> = Mutex::new(InFlightLimits::new());
static RUN_ACTIONS: Mutex<InFlightLimits> = Mutex::new(InFlightLimits::new());

struct DirectoryScanPermit {
    plugin_id: PluginId,
}

impl Drop for DirectoryScanPermit {
    fn drop(&mut self) {
        DIRECTORY_SCANS.lock().unwrap().release(self.plugin_id);
    }
}

struct PluginTimerPermit {
    plugin_id: PluginId,
}

impl Drop for PluginTimerPermit {
    fn drop(&mut self) {
        PLUGIN_TIMERS.lock().unwrap().release(self.plugin_id);
    }
}

fn try_acquire_directory_scan(plugin_id: PluginId) -> Option<DirectoryScanPermit> {
    let mut slots = DIRECTORY_SCANS.lock().unwrap();
    if slots.try_acquire(
        plugin_id,
        MAX_DIRECTORY_SCANS_PER_PLUGIN,
        MAX_DIRECTORY_SCANS_IN_FLIGHT,
    ) {
        Some(DirectoryScanPermit { plugin_id })
    } else {
        None
    }
}

struct RunActionPermit {
    plugin_id: PluginId,
}

impl Drop for RunActionPermit {
    fn drop(&mut self) {
        RUN_ACTIONS.lock().unwrap().release(self.plugin_id);
    }
}

fn try_acquire_run_action(plugin_id: PluginId) -> Option<RunActionPermit> {
    let mut slots = RUN_ACTIONS.lock().unwrap();
    if slots.try_acquire(
        plugin_id,
        MAX_RUN_ACTIONS_PER_PLUGIN,
        MAX_RUN_ACTIONS_IN_FLIGHT,
    ) {
        Some(RunActionPermit { plugin_id })
    } else {
        None
    }
}

fn try_acquire_plugin_timer(plugin_id: PluginId) -> Option<PluginTimerPermit> {
    let mut slots = PLUGIN_TIMERS.lock().unwrap();
    if slots.try_acquire(
        plugin_id,
        MAX_PLUGIN_TIMERS_PER_PLUGIN,
        MAX_PLUGIN_TIMERS_IN_FLIGHT,
    ) {
        Some(PluginTimerPermit { plugin_id })
    } else {
        None
    }
}

/// Upper bound for a plugin timer. Timers are used for short user-visible delays; allowing
/// years-long or larger values only pins host resources and can overflow duration conversion.
pub(crate) const MAX_PLUGIN_TIMEOUT_SECONDS: f64 = 24.0 * 60.0 * 60.0;

pub(crate) fn plugin_timeout_is_valid(secs: f64) -> bool {
    secs.is_finite() && secs >= 0.0 && secs <= MAX_PLUGIN_TIMEOUT_SECONDS
}

fn set_timeout(env: &PluginEnv, secs: f64) {
    if !plugin_timeout_is_valid(secs) {
        log::error!(
            "plugin {} requested an invalid timeout: {}",
            env.name(),
            secs
        );
        return;
    }
    let send_plugin_instructions = env.senders.to_plugin.clone();
    let plugin_id = env.plugin_id;
    let client_id = acting_client(env);
    let plugin_name = env.name();
    // Own this load instance: after unload/reload/client-remove the same
    // (plugin_id, client_id) may name a replacement. Do not duration-dedupe.
    let load_alive = env.load_alive.clone();
    let scheduled_generation = env.generation;
    let Some(permit) = try_acquire_plugin_timer(plugin_id) else {
        log::warn!(
            "plugin {plugin_name} already has too many outstanding timers; not starting another"
        );
        return;
    };
    // Use tokio runtime for async I/O (timer operation).
    // The permit is dropped as soon as this load is dead, not after the
    // remaining sleep. A live load still waits out its own deadline.
    get_tokio_runtime().spawn(async move {
        let _permit = permit;
        let start_time = Instant::now();
        let sleep = tokio::time::sleep(Duration::from_secs_f64(secs));
        tokio::pin!(sleep);
        loop {
            if !load_alive.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }
            tokio::select! {
                biased;
                _ = &mut sleep => break,
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
        if !load_alive.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        // FIXME: The way that elapsed time is being calculated here is not exact; it doesn't take into account the
        // time it takes an event to actually reach the plugin after it's sent to the `wasm` thread.
        let elapsed_time = Instant::now().duration_since(start_time).as_secs_f64();

        send_plugin_instructions
            .ok_or(anyhow!("found no sender to send plugin instruction to"))
            .and_then(|sender| {
                sender
                    .send(PluginInstruction::ApplyTimer {
                        plugin_id,
                        client_id,
                        elapsed_seconds: elapsed_time,
                        generation: scheduled_generation,
                    })
                    .to_anyhow()
            })
            .with_context(|| {
                format!(
                    "failed to set host timeout of {secs} s for plugin {} (generation {})",
                    plugin_name, scheduled_generation
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
    // `remove(0)` panics on an empty vector and the process panic hook exits the session.
    if command_line.is_empty() {
        log::error!(
            "failed to execute command on host for plugin '{}': command cannot be empty",
            env.name()
        );
        return;
    }
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

fn enqueue_background_job(env: &PluginEnv, job: BackgroundJob) -> bool {
    match env.senders.to_background_jobs.as_ref() {
        Some(sender) => sender.send(job).is_ok(),
        None => false,
    }
}

fn map_bytes(map: &BTreeMap<String, String>) -> usize {
    map.iter().fold(0usize, |bytes, (key, value)| {
        bytes.saturating_add(key.len()).saturating_add(value.len())
    })
}

fn run_command_payload_bytes(
    command_line: &[String],
    env_variables: &BTreeMap<String, String>,
    cwd: &Path,
    context: &BTreeMap<String, String>,
) -> usize {
    command_line
        .iter()
        .fold(64usize, |bytes, part| bytes.saturating_add(part.len()))
        .saturating_add(map_bytes(env_variables))
        .saturating_add(cwd.as_os_str().len())
        .saturating_add(map_bytes(context))
}

fn web_request_payload_bytes(
    url: &str,
    headers: &BTreeMap<String, String>,
    body: &[u8],
    context: &BTreeMap<String, String>,
) -> usize {
    url.len()
        .saturating_add(map_bytes(headers))
        .saturating_add(body.len())
        .saturating_add(map_bytes(context))
        .saturating_add(64)
}

fn send_load_result(
    env: &PluginEnv,
    event: Event,
    reservation: Option<BackgroundOperationReservation>,
) {
    let _ = env
        .senders
        .send_to_plugin(PluginInstruction::ApplyLoadResult {
            plugin_id: env.plugin_id,
            client_id: acting_client(env),
            event,
            generation: env.generation,
            reservation,
        });
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
        return;
    }
    let command = command_line.remove(0);
    let cwd = translate_plugin_path(env, cwd);
    let plugin_id = env.plugin_id;
    let payload_bytes = run_command_payload_bytes(&command_line, &env_variables, &cwd, &context);
    let Some(reservation) =
        try_acquire_run_command(plugin_id, env.load_alive.clone(), payload_bytes)
    else {
        log::warn!(
            "Not running a command for plugin {}: run-command limit reached",
            plugin_id
        );
        match try_acquire_refusal_report(plugin_id, env.load_alive.clone()) {
            Some(report) => send_load_result(
                env,
                crate::background_jobs::run_command_limit_event(context),
                Some(report),
            ),
            None => log::warn!(
                "Dropping repeated run-command refusal for plugin {plugin_id}: refusal backlog is full"
            ),
        }
        return;
    };
    enqueue_background_job(
        env,
        BackgroundJob::RunCommand(
            plugin_id,
            acting_client(env),
            command,
            command_line,
            env_variables,
            cwd,
            context,
            env.generation,
            reservation,
        ),
    );
}

fn web_request(
    env: &PluginEnv,
    url: String,
    verb: HttpVerb,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
    context: BTreeMap<String, String>,
) {
    let plugin_id = env.plugin_id;
    let payload_bytes = web_request_payload_bytes(&url, &headers, &body, &context);
    let Some(reservation) =
        try_acquire_web_request(plugin_id, env.load_alive.clone(), payload_bytes)
    else {
        log::warn!(
            "Not sending a web request for plugin {}: web-request limit reached",
            plugin_id
        );
        match try_acquire_refusal_report(plugin_id, env.load_alive.clone()) {
            Some(report) => send_load_result(
                env,
                crate::background_jobs::web_request_limit_event(context),
                Some(report),
            ),
            None => log::warn!(
                "Dropping repeated web-request refusal for plugin {plugin_id}: refusal backlog is full"
            ),
        }
        return;
    };
    enqueue_background_job(
        env,
        BackgroundJob::WebRequest(
            plugin_id,
            acting_client(env),
            url,
            verb,
            headers,
            body,
            context,
            env.generation,
            reservation,
        ),
    );
}

fn post_message_to(env: &PluginEnv, plugin_message: PluginMessage) -> Result<()> {
    let worker_name = plugin_message
        .worker_name
        .ok_or(anyhow!("Worker name not specified in message to worker"))?;
    if !try_admit_plugin_worker_post(
        env.plugin_id,
        &worker_name,
        &plugin_message.name,
        &plugin_message.payload,
    ) {
        return Ok(());
    }
    let plugin_id = env.plugin_id;
    let messages = vec![(plugin_message.name, plugin_message.payload)];
    let sent = env
        .senders
        .send_to_plugin(PluginInstruction::PostMessagesToPluginWorker(
            plugin_id,
            acting_client(env),
            worker_name.clone(),
            messages.clone(),
        ));
    if sent.is_err() {
        release_admitted_worker_posts(plugin_id, &worker_name, &messages);
    }
    sent
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
            acting_client(env),
            plugin_message.name,
            plugin_message.payload,
        ))
}

fn hide_self(env: &PluginEnv) -> Result<()> {
    env.senders
        .send_to_screen(ScreenInstruction::SuppressPane(
            PaneId::Plugin(self_pane_id(env)),
            acting_client(env),
        ))
        .with_context(|| format!("failed to hide self"))
}

fn hide_pane_with_id(env: &PluginEnv, pane_id: PaneId) -> Result<()> {
    env.senders
        .send_to_screen(ScreenInstruction::SuppressPane(pane_id, acting_client(env)))
        .with_context(|| format!("failed to hide self"))
}

fn show_self(env: &PluginEnv, should_float_if_hidden: bool) {
    let action = Action::FocusPluginPaneWithId {
        pane_id: self_pane_id(env),
        should_float_if_hidden,
        should_be_in_place_if_hidden: false,
    };
    let error_msg = || format!("Failed to show self for plugin");
    apply_action!(action, error_msg, env);
}

fn show_pane_with_id(
    env: &PluginEnv,
    pane_id: PaneId,
    should_float_if_hidden: bool,
    should_focus_pane: bool,
) {
    if should_focus_pane {
        let _ = env
            .senders
            .send_to_screen(ScreenInstruction::FocusPaneWithId(
                pane_id,
                should_float_if_hidden,
                false,
                acting_client(env),
                None,
            ));
    } else {
        let _ = env
            .senders
            .send_to_screen(ScreenInstruction::UnsuppressOrExpandPane(
                pane_id,
                should_float_if_hidden,
            ));
    }
}

fn prompt(env: &PluginEnv, request_id: u64, request: PromptRequest) {
    let caller = PromptCaller {
        plugin_id: env.plugin_id,
        client_id: env.client_id,
        request_id,
    };
    let instruction = prompt_instruction(caller, acting_client(env), self_pane_id(env), request);
    env.senders
        .send_to_plugin(instruction)
        .with_context(|| format!("failed to send a prompt request"))
        .non_fatal();
}

fn prompt_instruction(
    caller: PromptCaller,
    owner_client_id: ClientId,
    caller_pane_plugin_id: PluginId,
    request: PromptRequest,
) -> PluginInstruction {
    match request.check() {
        Ok(()) => PluginInstruction::PromptRequest {
            caller,
            owner_client_id,
            caller_pane_id: Some(PaneId::Plugin(caller_pane_plugin_id)),
            request,
        },
        Err(error) => {
            PluginInstruction::Update(vec![caller.result_event(PromptResult::Error(error))])
        },
    }
}

fn may_reply_to_prompt(location: &RunPluginLocation) -> bool {
    matches!(location, RunPluginLocation::Zellij(tag) if tag.to_string() == zellij_utils::prompt::PROMPT_PLUGIN_ALIAS)
}

fn reply_to_prompt(env: &PluginEnv, request_id: u64, result: PromptResult) {
    if !may_reply_to_prompt(&env.plugin.location) {
        log::warn!(
            "Ignoring a prompt reply from {}: only the prompt plugin can answer prompts",
            env.plugin.location.display()
        );
        return;
    }
    env.senders
        .send_to_plugin(PluginInstruction::PromptReplied {
            prompt_plugin_id: env.plugin_id,
            request_id,
            result,
        })
        .with_context(|| format!("failed to send a prompt reply"))
        .non_fatal();
}

fn close_self(env: &PluginEnv) {
    env.senders
        .send_to_screen(ScreenInstruction::ClosePane(
            PaneId::Plugin(self_pane_id(env)),
            None,
            None,
            None,
            ClosePaneSource::External,
            None,
        ))
        .with_context(|| format!("failed to close self"))
        .non_fatal();
    env.senders
        .send_to_plugin(PluginInstruction::Unload(self_pane_id(env)))
        .with_context(|| format!("failed to close self"))
        .non_fatal();
}

fn reconfigure(env: &PluginEnv, new_config: String, write_config_to_disk: bool) -> Result<()> {
    let err_context = || "Failed to reconfigure";
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::Reconfigure {
            client_id,
            config: new_config,
            write_config_to_disk,
        })
        .with_context(err_context)?;
    Ok(())
}

fn read_config(env: &PluginEnv) {
    let client_id = acting_client(env);
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let snapshot = match env.senders.send_to_server(ServerInstruction::ReadConfig {
        client_id,
        response_channel: response_sender,
    }) {
        Ok(()) => response_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| {
                log::error!("Failed to read config: {:?}", e);
                ConfigSnapshot::default()
            }),
        Err(e) => {
            log::error!("Failed to request config: {:?}", e);
            ConfigSnapshot::default()
        },
    };
    let response: ProtobufReadConfigResponse = snapshot.into();
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send config to plugin {}", env.name()))
        .non_fatal();
}

fn save_keybinds_as_preset(env: &PluginEnv, new_name: String) {
    use zellij_utils::plugin_api::plugin_command::copy_keybind_preset_response::Result as CopyResult;
    let client_id = acting_client(env);
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let result = match env
        .senders
        .send_to_server(ServerInstruction::SaveKeybindsAsPreset {
            client_id,
            new_name,
            response_channel: response_sender,
        }) {
        Ok(()) => response_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| Err(format!("Failed to save the preset: {:?}", e))),
        Err(e) => Err(format!("Failed to save the preset: {:?}", e)),
    };
    let response = ProtobufCopyKeybindPresetResponse {
        result: Some(match result {
            Ok(new_name) => CopyResult::NewName(new_name),
            Err(error) => CopyResult::Error(error),
        }),
    };
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send save result to plugin {}", env.name()))
        .non_fatal();
}

fn write_theme_file(
    env: &PluginEnv,
    name: String,
    copy_from: Option<String>,
    colours: Vec<String>,
) {
    use zellij_utils::plugin_api::plugin_command::write_theme_file_response::Result as WriteResult;
    let client_id = acting_client(env);
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let result = match env
        .senders
        .send_to_server(ServerInstruction::WriteThemeFile {
            client_id,
            name,
            copy_from,
            colours,
            response_channel: response_sender,
        }) {
        Ok(()) => response_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| Err(format!("Failed to write the theme file: {:?}", e))),
        Err(e) => Err(format!("Failed to write the theme file: {:?}", e)),
    };
    let response = ProtobufWriteThemeFileResponse {
        result: Some(match result {
            Ok(path) => WriteResult::Path(path),
            Err(error) => WriteResult::Error(error),
        }),
    };
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send theme file result to plugin {}", env.name()))
        .non_fatal();
}

fn delete_theme_file(env: &PluginEnv, name: String) {
    use zellij_utils::plugin_api::plugin_command::write_theme_file_response::Result as WriteResult;
    let client_id = acting_client(env);
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let result = match env
        .senders
        .send_to_server(ServerInstruction::DeleteThemeFile {
            client_id,
            name,
            response_channel: response_sender,
        }) {
        Ok(()) => response_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| Err(format!("Failed to delete the theme: {:?}", e))),
        Err(e) => Err(format!("Failed to delete the theme: {:?}", e)),
    };
    let response = ProtobufWriteThemeFileResponse {
        result: Some(match result {
            Ok(path) => WriteResult::Path(path),
            Err(error) => WriteResult::Error(error),
        }),
    };
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| {
            format!(
                "failed to send theme deletion result to plugin {}",
                env.name()
            )
        })
        .non_fatal();
}

fn copy_keybind_preset(env: &PluginEnv, preset: String, new_name: String) {
    use zellij_utils::plugin_api::plugin_command::copy_keybind_preset_response::Result as CopyResult;
    let client_id = acting_client(env);
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    let result = match env
        .senders
        .send_to_server(ServerInstruction::CopyKeybindPreset {
            client_id,
            preset,
            new_name,
            response_channel: response_sender,
        }) {
        Ok(()) => response_receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|e| Err(format!("Failed to copy the preset: {:?}", e))),
        Err(e) => Err(format!("Failed to copy the preset: {:?}", e)),
    };
    let response = ProtobufCopyKeybindPresetResponse {
        result: Some(match result {
            Ok(new_name) => CopyResult::NewName(new_name),
            Err(error) => CopyResult::Error(error),
        }),
    };
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to send copy result to plugin {}", env.name()))
        .non_fatal();
}

fn revert_config(env: &PluginEnv, key: Option<SettingKey>) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::RevertConfig { client_id, key })
        .with_context(|| "Failed to revert config")
        .non_fatal();
}

fn unset_config_setting(env: &PluginEnv, key: SettingKey) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::UnsetConfigSetting { client_id, key })
        .with_context(|| "Failed to unset config setting")
        .non_fatal();
}

fn save_config(env: &PluginEnv) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::SaveConfig { client_id })
        .with_context(|| "Failed to save config")
        .non_fatal();
}

fn overwrite_config_file(env: &PluginEnv) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::OverwriteConfigFile { client_id })
        .with_context(|| "Failed to overwrite the config file")
        .non_fatal();
}

fn reload_config_file(env: &PluginEnv) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::ReloadConfigFile { client_id })
        .with_context(|| "Failed to reload the config file")
        .non_fatal();
}

fn replace_config_blocks(env: &PluginEnv, config: String) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::ReplaceConfigBlocks { client_id, config })
        .with_context(|| "Failed to replace config blocks")
        .non_fatal();
}

fn reset_keys(
    env: &PluginEnv,
    keys: Vec<(InputMode, KeyWithModifier)>,
    write_config_to_disk: bool,
) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::ResetKeys {
            client_id,
            keys,
            write_config_to_disk,
        })
        .with_context(|| "Failed to reset keys")
        .non_fatal();
}

fn reset_mousebinds(
    env: &PluginEnv,
    triggers: Vec<(InputMode, MouseTrigger)>,
    write_config_to_disk: bool,
) {
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::ResetMousebinds {
            client_id,
            triggers,
            write_config_to_disk,
        })
        .with_context(|| "Failed to reset mouse bindings")
        .non_fatal();
}

fn rebind_keys(
    env: &PluginEnv,
    keys_to_rebind: Vec<(InputMode, KeyWithModifier, Vec<Action>)>,
    keys_to_unbind: Vec<(InputMode, KeyWithModifier)>,
    write_config_to_disk: bool,
) -> Result<()> {
    let err_context = || "Failed to rebind_keys";
    let client_id = acting_client(env);
    env.senders
        .send_to_server(ServerInstruction::RebindKeys {
            client_id,
            keys_to_rebind,
            keys_to_unbind,
            write_config_to_disk,
        })
        .with_context(err_context)?;
    Ok(())
}

fn switch_to_mode(env: &PluginEnv, input_mode: InputMode) {
    let action = Action::SwitchToMode { input_mode };
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
    let cwd = None; // TODO: add this to the plugin API
    if tabs.is_empty() {
        let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
        let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
        let action = Action::NewTab {
            tiled_layout: layout.template.as_ref().map(|t| t.0.clone()),
            floating_layouts: layout.template.map(|t| t.1).unwrap_or_default(),
            swap_tiled_layouts,
            swap_floating_layouts,
            tab_name: None,
            should_change_focus_to_new_tab: true,
            cwd,
            initial_panes: None,
            first_pane_unblock_condition: None,
        };
        tabs_to_open.push(action);
    } else {
        let focused_tab_index = layout.focused_tab_index().unwrap_or(0);
        for (tab_index, (tab_name, tiled_pane_layout, floating_pane_layout)) in
            layout.tabs().into_iter().enumerate()
        {
            let should_focus_tab = tab_index == focused_tab_index;
            let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
            let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
            let action = Action::NewTab {
                tiled_layout: Some(tiled_pane_layout),
                floating_layouts: floating_pane_layout,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
                should_change_focus_to_new_tab: should_focus_tab,
                cwd: cwd.clone(),
                initial_panes: None,
                first_pane_unblock_condition: None,
            };
            tabs_to_open.push(action);
        }
    }
    let mut tab_ids = Vec::new();

    for action in tabs_to_open {
        let error_msg = || format!("Failed to create layout tab");
        let result = apply_action!(action, error_msg, env);

        // Collect tab ID from each action
        if let Some(tab_id) = result.and_then(|r| r.affected_tab_id) {
            tab_ids.push(tab_id);
        }
    }

    // Write response
    let response = ProtobufNewTabsResponse::from(tab_ids);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write layout tabs response"))
        .non_fatal();
}

fn new_tab(env: &PluginEnv, name: Option<String>, cwd: Option<String>) {
    let cwd = cwd.map(|c| translate_plugin_path(env, PathBuf::from(c)));
    let action = Action::NewTab {
        tiled_layout: None,
        floating_layouts: vec![],
        swap_tiled_layouts: None,
        swap_floating_layouts: None,
        tab_name: name,
        should_change_focus_to_new_tab: true,
        cwd,
        initial_panes: None,
        first_pane_unblock_condition: None,
    };
    let error_msg = || format!("Failed to open new tab");
    let result = apply_action!(action, error_msg, env);

    // Extract tab_id from result - return None if not present
    let tab_id: NewTabResponse = result.and_then(|r| r.affected_tab_id);

    // Write response to plugin
    let response = ProtobufNewTabResponse::from(tab_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write new_tab response"))
        .non_fatal();
}

fn new_tab_unfocused(env: &PluginEnv, name: Option<String>, cwd: Option<String>) {
    let cwd = cwd.map(|c| translate_plugin_path(env, PathBuf::from(c)));
    let action = Action::NewTab {
        tiled_layout: None,
        floating_layouts: vec![],
        swap_tiled_layouts: None,
        swap_floating_layouts: None,
        tab_name: name,
        should_change_focus_to_new_tab: false,
        cwd,
        initial_panes: None,
        first_pane_unblock_condition: None,
    };
    let error_msg = || format!("Failed to open new tab (unfocused)");
    let result = apply_action!(action, error_msg, env);

    let tab_id: NewTabUnfocusedResponse = result.and_then(|r| r.affected_tab_id);

    let response = ProtobufNewTabUnfocusedResponse::from(tab_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write new_tab_unfocused response"))
        .non_fatal();
}

fn new_tiled_pane_in_tab(env: &PluginEnv, tab_position: usize) {
    let error_msg = || format!("failed to open new tiled pane in tab {}", tab_position);
    let default_shell = env.default_shell.clone().unwrap_or_else(|| {
        TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        })
    });
    let run_command_action: Option<RunCommandAction> = match default_shell {
        TerminalAction::RunCommand(run_command) => Some(run_command.into()),
        _ => None,
    };
    let action = Action::NewTiledPane {
        direction: None,
        command: run_command_action,
        pane_name: None,
        near_current_pane: false,
        no_focus: false,
        borderless: None,
        border_style: None,
        tab_id: Some(tab_position),
    };
    let result = apply_action!(action, error_msg, env);

    let pane_id: NewTiledPaneInTabResponse =
        result.and_then(|r| r.affected_pane_id).map(|p| p.into());

    let response = ProtobufNewTiledPaneInTabResponse::from(pane_id);
    wasi_write_object(env, &response.encode_to_vec())
        .with_context(|| format!("failed to write new_tiled_pane_in_tab response"))
        .non_fatal();
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
    let action = Action::Resize {
        resize,
        direction: None,
    };
    apply_action!(action, error_msg, env);
}

fn resize_with_direction(env: &PluginEnv, resize: ResizeStrategy) {
    let error_msg = || format!("failed to resize in plugin {}", env.name());
    let action = Action::Resize {
        resize: resize.resize,
        direction: resize.direction,
    };
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

fn focus_last_pane(env: &PluginEnv) {
    let action = Action::FocusLastPane;
    let error_msg = || format!("Failed to focus last pane");
    apply_action!(action, error_msg, env);
}

fn move_focus(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move focus in plugin {}", env.name());
    let action = Action::MoveFocus { direction };
    apply_action!(action, error_msg, env);
}

fn move_focus_or_tab(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move focus in plugin {}", env.name());
    let action = Action::MoveFocusOrTab { direction };
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
    switch_context: Option<zellij_utils::session_history::SwitchContext>,
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

    let (completion_tx, completion_rx) = oneshot::channel();

    if session_name
        .as_ref()
        .map(|s| s.contains('/'))
        .unwrap_or(false)
    {
        log::error!("Session names cannot contain \'/\'");
    } else {
        let client_id = acting_client(env);
        let tab_position = tab_position.map(|p| p + 1); // ¯\_()_/¯
        let cwd = cwd
            .map(|c| translate_plugin_path(env, c))
            .or_else(|| Some(env.plugin_cwd.clone()));
        let connect_to_session = ConnectToSession {
            name: session_name,
            tab_position,
            pane_id,
            layout,
            cwd,
            switch_context: switch_context.and_then(|c| {
                zellij_utils::session_history::SwitchContext::from_plugin_request(
                    c.startup_card,
                    c.disconnect_others,
                )
            }),
        };
        env.senders
            .send_to_server(ServerInstruction::SwitchSession(
                connect_to_session,
                client_id,
                Some(NotificationEnd::new(completion_tx)),
            ))
            .with_context(err_context)?;
        let wait_forever = false;
        let _ = wait_for_action_completion(completion_rx, "switch_session", wait_forever);
    }
    Ok(())
}

fn notify_session_index_changed(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::SessionSuggestions(
            SessionSuggestionsJob::IndexChanged,
        ));
}

fn delete_session_index_row(session_name: &str) {
    if let Ok(index) = zellij_utils::session_index::SessionIndex::open_default() {
        let _ = index.delete(session_name);
    }
}

fn delete_dead_session(session_name: String) -> Result<()> {
    delete_dead_session_cache(&session_name)
}

/// Identity, folder-name eligibility, then a socket-inventory death check and
/// the publication fence (tombstone and cache removal). The index row is
/// removed only after the cache is gone. A live socket, or an inventory that
/// cannot be read, is not death: the cache and the row stay.
fn delete_dead_session_cache(session_name: &str) -> Result<()> {
    delete_dead_session_cache_with(
        session_name,
        |session_id| {
            zellij_utils::sessions::remove_fenced_dead_session_cache(
                &ZELLIJ_SOCK_DIR,
                &ZELLIJ_SESSION_INFO_CACHE_DIR,
                session_id,
            )
        },
        |display_name| delete_session_index_row(display_name),
    )
}

fn delete_dead_session_cache_with(
    session_name: &str,
    remove_cache: impl FnOnce(&str) -> std::io::Result<zellij_utils::sessions::SessionCacheRemoval>,
    mut delete_index: impl FnMut(&str),
) -> Result<()> {
    // The cache folder is keyed by session id. `Ok(None)` keeps the name as the id.
    // A lookup error must not remove that name's folder or an index row guessed from it.
    let id = zellij_utils::sessions::session_folder_id(session_name).map_err(|error| {
        anyhow!(
            "session index lookup failed; the cache and the index row were left in place: {}",
            error
        )
    })?;
    validate_session_folder_name(&id).map_err(|error| anyhow!(error))?;
    // The session index is keyed by display name, which differs from the argument
    // when called with an id. A lookup error leaves the row in place.
    let display_name = match zellij_utils::session_index::session_name_for_id(&id) {
        Ok(Some(name)) => name,
        Ok(None) => id.clone(),
        Err(error) => {
            return Err(anyhow!(
                "session index lookup failed; the cache and the index row were left in place: {}",
                error
            ));
        },
    };
    // A failed tombstone leaves the directory and the index row in place.
    match remove_cache(&id) {
        Ok(zellij_utils::sessions::SessionCacheRemoval::Removed) => {
            delete_index(&display_name);
            Ok(())
        },
        Ok(zellij_utils::sessions::SessionCacheRemoval::NotFound) => {
            Err(anyhow!("Failed to delete dead session: {:?}", session_name))
        },
        Err(error) => {
            Err(error).with_context(|| format!("Failed to delete dead session: {:?}", session_name))
        },
    }
}

fn delete_dead_session_reply(session_name: &str) -> DeleteDeadSessionResponse {
    match delete_dead_session_cache(session_name) {
        Ok(()) => DeleteDeadSessionResponse::Ok,
        Err(error) => DeleteDeadSessionResponse::Err(format!("{error:#}")),
    }
}

fn delete_all_dead_sessions() -> Result<()> {
    let index = match zellij_utils::session_index::SessionIndex::open_default() {
        Ok(index) => Some(index),
        Err(e) => {
            log::error!("Failed to open the session index: {}", e);
            None
        },
    };
    zellij_utils::session_index::delete_dead_sessions_in(
        &ZELLIJ_SOCK_DIR,
        &ZELLIJ_SESSION_INFO_CACHE_DIR,
        index.as_ref(),
    )
    .map(|_| ())
    .map_err(|e| anyhow!(e))
}

fn edit_scrollback(env: &PluginEnv) {
    let action = Action::EditScrollback { ansi: false };
    let error_msg = || format!("Failed to edit scrollback");
    apply_action!(action, error_msg, env);
}

fn write(env: &PluginEnv, bytes: Vec<u8>) {
    let error_msg = || format!("failed to write in plugin {}", env.name());
    let action = Action::Write {
        key_with_modifier: None,
        bytes,
        is_kitty_keyboard_protocol: false,
    };
    apply_action!(action, error_msg, env);
}

fn write_chars(env: &PluginEnv, chars_to_write: String) {
    let error_msg = || format!("failed to write in plugin {}", env.name());
    let action = Action::WriteChars {
        chars: chars_to_write,
    };
    apply_action!(action, error_msg, env);
}

fn copy_to_clipboard(env: &PluginEnv, text: String) -> Result<()> {
    env.senders
        .send_to_screen(ScreenInstruction::CopyTextToClipboard(text, env.plugin_id))
        .with_context(|| format!("failed to copy to clipboard in plugin {}", env.name()))
}

fn toggle_tab(env: &PluginEnv) {
    let error_msg = || format!("Failed to toggle tab");
    let action = Action::ToggleTab;
    apply_action!(action, error_msg, env);
}

fn move_pane(env: &PluginEnv) {
    let error_msg = || format!("failed to move pane in plugin {}", env.name());
    let action = Action::MovePane { direction: None };
    apply_action!(action, error_msg, env);
}

fn move_pane_with_direction(env: &PluginEnv, direction: Direction) {
    let error_msg = || format!("failed to move pane in plugin {}", env.name());
    let action = Action::MovePane {
        direction: Some(direction),
    };
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

fn toggle_focus_no_ui_fullscreen(env: &PluginEnv) {
    let error_msg = || {
        format!(
            "failed to toggle no-ui full screen in plugin {}",
            env.name()
        )
    };
    let action = Action::ToggleFocusNoUiFullscreen;
    apply_action!(action, error_msg, env);
}

fn focus_host_session(env: &PluginEnv) {
    env.senders
        .send_to_screen(ScreenInstruction::FocusHostSession(
            acting_client(env),
            None,
        ))
        .with_context(|| format!("failed to focus host session from plugin {}", env.name()))
        .non_fatal();
}

fn get_nested_session_keybinds(env: &PluginEnv, pane_id: PaneId) {
    use crossbeam::channel::RecvTimeoutError;
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);
    env.senders
        .send_to_screen(ScreenInstruction::GetNestedSessionKeybinds {
            pane_id,
            response_channel: response_sender,
        })
        .with_context(|| {
            format!(
                "failed to request nested session keybindings from plugin {}",
                env.name()
            )
        })
        .non_fatal();
    let response = match response_receiver
        .recv_timeout(zellij_utils::nested_session::KEYBINDS_REQUEST_TIMEOUT)
    {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => {
            log::warn!(
                "nested session keybindings for pane {:?} did not arrive in time for plugin {}",
                pane_id,
                env.plugin_id
            );
            Err(NestedSessionKeybindsError::Timeout)
        },
    };
    let protobuf_response = nested_session_keybinds_response_to_protobuf(response).or_else(|_| {
        nested_session_keybinds_response_to_protobuf(Err(NestedSessionKeybindsError::NotSupported))
    });
    match protobuf_response {
        Ok(protobuf_response) => {
            let _ = wasi_write_object(env, &protobuf_response.encode_to_vec());
        },
        Err(e) => log::error!("failed to encode nested session keybindings: {}", e),
    }
}

fn toggle_pane_frames(env: &PluginEnv) {
    let error_msg = || format!("failed to toggle full screen in plugin {}", env.name());
    let action = Action::TogglePaneFrames;
    apply_action!(action, error_msg, env);
}

fn set_pane_frame_style(env: &PluginEnv, pane_frame_style: PaneFrameStyle) {
    let error_msg = || format!("failed to set pane frame style in plugin {}", env.name());
    let action = Action::SetPaneFrameStyle(pane_frame_style);
    apply_action!(action, error_msg, env);
}

fn apply_bars_action(env: &PluginEnv, action: Action) {
    let error_msg = || format!("failed to change hidden bars in plugin {}", env.name());
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

fn toggle_floating_panes(env: &PluginEnv, tab_id: Option<u64>) {
    let error_msg = || format!("failed to toggle floating panes in plugin {}", env.name());
    let action = match tab_id {
        Some(id) => Action::ToggleFloatingPanesByTabId { id },
        None => Action::ToggleFloatingPanes,
    };
    apply_action!(action, error_msg, env);
}

fn new_pane(env: &PluginEnv) {
    let error_msg = || format!("failed to open new pane in plugin {}", env.name());
    let action = Action::NewPane {
        direction: None,
        pane_name: None,
        start_suppressed: false,
    };
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
    let action = Action::SkipConfirm {
        action: Box::new(Action::Quit),
    };
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

fn apply_tiled_swap_layout(env: &PluginEnv, layout_name: String) {
    let error_msg = || format!("failed to apply tiled swap layout in plugin {}", env.name());
    let action = Action::ApplyTiledSwapLayout { name: layout_name };
    apply_action!(action, error_msg, env);
}

fn apply_floating_swap_layout(env: &PluginEnv, layout_name: String) {
    let error_msg = || {
        format!(
            "failed to apply floating swap layout in plugin {}",
            env.name()
        )
    };
    let action = Action::ApplyFloatingSwapLayout { name: layout_name };
    apply_action!(action, error_msg, env);
}

fn go_to_tab_name(env: &PluginEnv, tab_name: String) {
    let error_msg = || format!("failed to change tab in plugin {}", env.name());
    let create = false;
    let action = Action::GoToTabName {
        name: tab_name,
        create,
    };
    apply_action!(action, error_msg, env);
}

fn focus_or_create_tab(env: &PluginEnv, tab_name: String) {
    let error_msg = || format!("failed to change or create tab in plugin {}", env.name());
    let create = true;
    let action = Action::GoToTabName {
        name: tab_name,
        create,
    };
    let result = apply_action!(action, error_msg, env);

    // Return Some(tab_id) if tab was created or focused, None on error
    let tab_id: FocusOrCreateTabResponse = result.and_then(|r| r.affected_tab_id);

    let response = ProtobufFocusOrCreateTabResponse::from(tab_id);
    wasi_write_object(env, &response.encode_to_vec()).non_fatal();
}

fn go_to_tab(env: &PluginEnv, tab_index: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::GoToTab {
        index: tab_index + 1,
    };
    apply_action!(action, error_msg, env);
}

fn start_or_reload_plugin(env: &PluginEnv, url: &str) -> Result<()> {
    let error_msg = || format!("failed to start or reload plugin in plugin {}", env.name());
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let run_plugin_or_alias = RunPluginOrAlias::from_url(url, &None, None, Some(cwd))
        .map_err(|e| anyhow!("Failed to parse plugin location: {}", e))?;
    let action = Action::StartOrReloadPlugin {
        plugin: run_plugin_or_alias,
    };
    apply_action!(action, error_msg, env);
    Ok(())
}

fn close_terminal_pane(env: &PluginEnv, terminal_pane_id: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::CloseTerminalPane {
        pane_id: terminal_pane_id,
    };
    apply_action!(action, error_msg, env);
    env.senders
        .send_to_pty(PtyInstruction::ClosePane(
            PaneId::Terminal(terminal_pane_id),
            None,
        ))
        .non_fatal();
}

fn close_plugin_pane(env: &PluginEnv, plugin_pane_id: u32) {
    let error_msg = || format!("failed to change tab focus in plugin {}", env.name());
    let action = Action::ClosePluginPane {
        pane_id: plugin_pane_id,
    };
    apply_action!(action, error_msg, env);
    env.senders
        .send_to_plugin(PluginInstruction::Unload(plugin_pane_id))
        .non_fatal();
}

fn focus_terminal_pane(
    env: &PluginEnv,
    terminal_pane_id: u32,
    should_float_if_hidden: bool,
    should_be_in_place_if_hidden: bool,
) {
    let action = Action::FocusTerminalPaneWithId {
        pane_id: terminal_pane_id,
        should_float_if_hidden,
        should_be_in_place_if_hidden,
    };
    let error_msg = || format!("Failed to focus terminal pane");
    apply_action!(action, error_msg, env);
}

fn focus_plugin_pane(
    env: &PluginEnv,
    plugin_pane_id: u32,
    should_float_if_hidden: bool,
    should_be_in_place_if_hidden: bool,
) {
    let action = Action::FocusPluginPaneWithId {
        pane_id: plugin_pane_id,
        should_float_if_hidden,
        should_be_in_place_if_hidden,
    };
    let error_msg = || format!("Failed to focus plugin pane");
    apply_action!(action, error_msg, env);
}

fn rename_terminal_pane(env: &PluginEnv, terminal_pane_id: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename terminal pane");
    let rename_pane_action = Action::RenameTerminalPane {
        pane_id: terminal_pane_id,
        name: new_name.as_bytes().to_vec(),
    };
    apply_action!(rename_pane_action, error_msg, env);
}

fn rename_plugin_pane(env: &PluginEnv, plugin_pane_id: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename plugin pane");
    let rename_pane_action = Action::RenamePluginPane {
        pane_id: plugin_pane_id,
        name: new_name.as_bytes().to_vec(),
    };
    apply_action!(rename_pane_action, error_msg, env);
}

fn rename_tab(env: &PluginEnv, tab_index: u32, new_name: &str) {
    let error_msg = || format!("Failed to rename tab");
    let rename_tab_action = Action::RenameTab {
        tab_index,
        name: new_name.as_bytes().to_vec(),
    };
    apply_action!(rename_tab_action, error_msg, env);
}

fn rename_session(env: &PluginEnv, new_session_name: String) {
    let error_msg = || format!("failed to rename session in plugin {}", env.name());
    let action = Action::RenameSession {
        name: new_session_name,
    };
    apply_action!(action, error_msg, env);
}

fn disconnect_other_clients(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_server(ServerInstruction::DisconnectAllClientsExcept(
            acting_client(env),
            zellij_utils::ipc::ExitReason::KickedByHost,
        ))
        .context("failed to send disconnect other clients instruction");
}

fn kill_sessions(session_names: Vec<String>) {
    for session_name in session_names {
        let resolved = match zellij_utils::sessions::resolve_session_socket_path(&session_name) {
            Ok(Some(path)) => path,
            Ok(None) => ZELLIJ_SOCK_DIR.join(&session_name),
            Err(error) => {
                log::error!(
                    "Failed to resolve session {:?}: {}. The session was not killed.",
                    session_name,
                    error
                );
                continue;
            },
        };
        // Pair the Windows request and reply pipes in one connect. The same
        // helper is a timed unix connect. Five seconds matches the session probe.
        if let Err(e) = zellij_utils::sessions::send_kill_session(&resolved, Duration::from_secs(5))
        {
            log::error!("Failed to kill session {}: {:?}", session_name, e);
        }
    }
}

// Wedge timeout: one budget for writing KillSession and reading a decoded
// Exit frame. It does not mean the server process was observed to exit.
// OutcomeUnknown (no decoded Exit) is a failure, same as a send error.
// Applied over the whole batch -- per-session kills run concurrently.
const KILL_WEDGE_TIMEOUT: Duration = Duration::from_millis(500);

fn kill_sessions_and_reply(env: &PluginEnv, session_names: Vec<String>) {
    use tokio::task::JoinSet;
    let runtime = get_tokio_runtime();
    let result: Result<(), String> = runtime.block_on(async {
        let mut set: JoinSet<(String, std::io::Result<zellij_utils::ipc::KillReply>)> =
            JoinSet::new();
        for name in session_names {
            let path = match zellij_utils::sessions::resolve_session_socket_path(&name) {
                Ok(Some(path)) => path,
                Ok(None) => ZELLIJ_SOCK_DIR.join(&name),
                Err(error) => {
                    set.spawn(async move {
                        (
                            name,
                            Err(std::io::Error::new(
                                std::io::ErrorKind::Other,
                                format!(
                                    "session index lookup failed; the session was not killed: {}",
                                    error
                                ),
                            )),
                        )
                    });
                    continue;
                },
            };
            set.spawn(async move {
                let res =
                    zellij_utils::ipc::async_send_kill_and_await(&path, KILL_WEDGE_TIMEOUT).await;
                (name, res)
            });
        }
        let drain = async {
            let mut first_err: Option<String> = None;
            while let Some(joined) = set.join_next().await {
                match joined {
                    Ok((
                        _name,
                        Ok(zellij_utils::ipc::KillReply::ShutdownAcknowledged),
                    )) => {},
                    Ok((name, Ok(zellij_utils::ipc::KillReply::OutcomeUnknown))) => {
                        if first_err.is_none() {
                            first_err = Some(format!(
                                "Session {name}: kill request was written but no Exit reply was decoded; process exit was not observed"
                            ));
                        }
                    },
                    Ok((name, Err(e))) => {
                        if first_err.is_none() {
                            first_err = Some(format!("Failed to send kill to session {name}: {e}"));
                        }
                    },
                    Err(e) => {
                        if first_err.is_none() {
                            first_err = Some(format!("Internal error in kill task: {}", e));
                        }
                    },
                }
            }
            match first_err {
                Some(e) => Err(e),
                None => Ok(()),
            }
        };
        match tokio::time::timeout(KILL_WEDGE_TIMEOUT, drain).await {
            Ok(res) => res,
            Err(_) => {
                Err("Timed out waiting for one or more sessions to acknowledge kill".to_string())
            },
        }
    });
    let response = match result {
        Ok(()) => KillSessionsResponse::Ok,
        Err(e) => KillSessionsResponse::Err(e),
    };
    let protobuf_response = ProtobufKillSessionsResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| "failed to write kill_sessions response".to_string())
        .non_fatal();
}

fn delete_dead_session_and_reply(env: &PluginEnv, session_name: String) {
    let response = delete_dead_session_reply(&session_name);
    if let DeleteDeadSessionResponse::Ok = &response {
        notify_session_index_changed(env);
    }
    let protobuf_response = ProtobufDeleteDeadSessionResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| "failed to write delete_dead_session response".to_string())
        .non_fatal();
}

fn delete_all_dead_sessions_and_reply(env: &PluginEnv) {
    // Same budget as kill-all so the UX of "y to confirm" is consistent: the
    // host-side fs work for many dead sessions is bounded in wall time.
    let runtime = get_tokio_runtime();
    let result: Result<(), String> = runtime.block_on(async {
        let fs_task = tokio::task::spawn_blocking(delete_all_dead_sessions);
        match tokio::time::timeout(KILL_WEDGE_TIMEOUT, fs_task).await {
            Ok(Ok(Ok(()))) => Ok(()),
            Ok(Ok(Err(e))) => Err(format!("Failed to delete dead sessions: {}", e)),
            Ok(Err(e)) => Err(format!("Internal error in delete-all task: {}", e)),
            Err(_) => Err("Timed out deleting dead sessions".to_string()),
        }
    });
    let response = match result {
        Ok(()) => DeleteAllDeadSessionsResponse::Ok,
        Err(e) => DeleteAllDeadSessionsResponse::Err(e),
    };
    let protobuf_response = ProtobufDeleteAllDeadSessionsResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| "failed to write delete_all_dead_sessions response".to_string())
        .non_fatal();
}

fn watch_filesystem(env: &PluginEnv) {
    let _ = env.senders.to_plugin.as_ref().map(|sender| {
        sender.send(PluginInstruction::WatchFilesystem {
            plugin_id: env.plugin_id,
            host_root: env.plugin_cwd.clone(),
        })
    });
}

// note this removes the requesting plugin
fn dump_session_layout(env: &PluginEnv, tab_index: Option<usize>) {
    use crate::plugins::DumpSessionLayoutResponse;

    // Create oneshot channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Send request to screen thread
    let send_result = env
        .senders
        .send_to_screen(ScreenInstruction::DumpLayoutToPlugin {
            plugin_id: env.plugin_id,
            tab_index,
            response_channel: response_sender,
        });

    if let Err(e) = send_result {
        let error_response = ProtobufDumpSessionLayoutResponse {
            result: Some(dump_session_layout_response::Result::Error(format!(
                "Failed to send dump layout request: {}",
                e
            ))),
            metadata: None,
        };
        let _ = wasi_write_object(env, &error_response.encode_to_vec());
        return;
    }

    // Wait for response with 1 second timeout
    let response: DumpSessionLayoutResponse =
        match response_receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(resp) => resp,
            Err(e) => {
                let error_response = ProtobufDumpSessionLayoutResponse {
                    result: Some(dump_session_layout_response::Result::Error(format!(
                        "Timeout waiting for session layout: {}",
                        e
                    ))),
                    metadata: None,
                };
                let _ = wasi_write_object(env, &error_response.encode_to_vec());
                return;
            },
        };

    // Convert LayoutMetadata to protobuf
    let protobuf_metadata = response
        .metadata
        .and_then(|metadata| match metadata.try_into() {
            Ok(pb) => Some(pb),
            Err(e) => {
                log::error!("Failed to convert LayoutMetadata to protobuf: {}", e);
                None
            },
        });

    // Build protobuf response
    let protobuf_response = match response.layout_result {
        Ok(layout_content) => ProtobufDumpSessionLayoutResponse {
            result: Some(dump_session_layout_response::Result::LayoutContent(
                layout_content,
            )),
            metadata: protobuf_metadata,
        },
        Err(error) => ProtobufDumpSessionLayoutResponse {
            result: Some(dump_session_layout_response::Result::Error(error)),
            metadata: None,
        },
    };

    // Write response back to plugin via WASI pipe
    let _ = wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| format!("failed to send session layout to plugin {}", env.name()));
}

fn save_session(env: &PluginEnv) {
    use save_session_response::Result as SaveSessionResult;
    let (completion_tx, completion_rx) = oneshot::channel();

    let send_result = env.senders.send_to_screen(ScreenInstruction::SaveSession(
        acting_client(env),
        Some(NotificationEnd::new(completion_tx)),
    ));

    let response = if let Err(e) = send_result {
        ProtobufSaveSessionResponse {
            result: Some(SaveSessionResult::Error(format!(
                "Failed to send save request: {}",
                e
            ))),
        }
    } else {
        let wait_forever = false;
        let _result = wait_for_action_completion(completion_rx, "save_session", wait_forever);
        ProtobufSaveSessionResponse {
            result: Some(SaveSessionResult::Success(true)),
        }
    };

    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn show_floating_panes(env: &PluginEnv, tab_id: Option<usize>) {
    use show_floating_panes_response::Result as ShowResult;
    let (completion_tx, completion_rx) = oneshot::channel();
    let send_result = env
        .senders
        .send_to_screen(ScreenInstruction::ShowFloatingPanes {
            client_id: acting_client(env),
            tab_id,
            completion: Some(NotificationEnd::new(completion_tx)),
        });
    let response = if let Err(e) = send_result {
        ProtobufShowFloatingPanesResponse {
            result: Some(ShowResult::Error(format!("{}", e))),
        }
    } else {
        let result = wait_for_action_completion(completion_rx, "show_floating_panes", false);
        match result.exit_status {
            Some(0) => ProtobufShowFloatingPanesResponse {
                result: Some(ShowResult::Success(true)),
            },
            Some(2) => ProtobufShowFloatingPanesResponse {
                result: Some(ShowResult::Success(false)),
            },
            _ => ProtobufShowFloatingPanesResponse {
                result: Some(ShowResult::Error("Tab not found".to_string())),
            },
        }
    };
    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn hide_floating_panes(env: &PluginEnv, tab_id: Option<usize>) {
    use hide_floating_panes_response::Result as HideResult;
    let (completion_tx, completion_rx) = oneshot::channel();
    let send_result = env
        .senders
        .send_to_screen(ScreenInstruction::HideFloatingPanes {
            client_id: acting_client(env),
            tab_id,
            completion: Some(NotificationEnd::new(completion_tx)),
        });
    let response = if let Err(e) = send_result {
        ProtobufHideFloatingPanesResponse {
            result: Some(HideResult::Error(format!("{}", e))),
        }
    } else {
        let result = wait_for_action_completion(completion_rx, "hide_floating_panes", false);
        match result.exit_status {
            Some(0) => ProtobufHideFloatingPanesResponse {
                result: Some(HideResult::Success(true)),
            },
            Some(2) => ProtobufHideFloatingPanesResponse {
                result: Some(HideResult::Success(false)),
            },
            _ => ProtobufHideFloatingPanesResponse {
                result: Some(HideResult::Error("Tab not found".to_string())),
            },
        }
    };
    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn set_pane_regex_highlights(
    env: &PluginEnv,
    pane_id: zellij_utils::data::PaneId,
    highlights: Vec<RegexHighlight>,
) {
    let err_context = || "failed to set pane regex highlights".to_string();
    let pane_id: PaneId = pane_id.into();
    env.senders
        .send_to_screen(ScreenInstruction::SetPluginRegexHighlights {
            pane_id,
            plugin_id: env.plugin_id,
            highlights,
        })
        .with_context(err_context)
        .non_fatal();
}

fn clear_pane_highlights(env: &PluginEnv, pane_id: zellij_utils::data::PaneId) {
    let err_context = || "failed to clear pane highlights".to_string();
    let pane_id: PaneId = pane_id.into();
    env.senders
        .send_to_screen(ScreenInstruction::ClearPluginHighlights {
            pane_id,
            plugin_id: env.plugin_id,
        })
        .with_context(err_context)
        .non_fatal();
}

fn current_session_last_saved_time(env: &PluginEnv) {
    use zellij_utils::plugin_api::plugin_command::ProtobufCurrentSessionLastSavedTimeResponse;

    let (response_tx, response_rx) = crossbeam::channel::bounded(1);
    let send_result = env
        .senders
        .send_to_plugin(PluginInstruction::GetLastSessionSaveTime {
            response_channel: response_tx,
        });

    let saved_timestamp_millis = if send_result.is_ok() {
        response_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .ok()
            .flatten()
    } else {
        None
    };

    let timestamp_millis = saved_timestamp_millis.map(|saved_time| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(saved_time)
    });

    let response = ProtobufCurrentSessionLastSavedTimeResponse { timestamp_millis };
    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn get_pane_info(env: &PluginEnv, pane_id: zellij_utils::data::PaneId) {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    let err_context = || {
        format!(
            "failed to get pane info for pane {:?} from plugin {}",
            pane_id,
            env.name()
        )
    };

    // Create channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Convert from plugin PaneId to server PaneId
    let server_pane_id: PaneId = pane_id.into();

    // Send request to screen thread
    env.senders
        .send_to_screen(ScreenInstruction::GetPaneInfo {
            pane_id: server_pane_id,
            response_channel: response_sender,
        })
        .with_context(err_context)
        .non_fatal();

    // Block waiting for response with 100ms timeout
    let pane_info = match response_receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(Some(pane_info)) => {
            let mut redacted =
                zellij_utils::data::Event::PaneUpdate(zellij_utils::data::PaneManifest {
                    panes: [(0, vec![pane_info])].into_iter().collect(),
                });
            crate::plugins::wasm_bridge::redact_event_for_plugin(env, &mut redacted);
            let pane_info = match redacted {
                zellij_utils::data::Event::PaneUpdate(mut manifest) => manifest
                    .panes
                    .remove(&0)
                    .and_then(|mut infos| infos.pop())
                    .unwrap_or_default(),
                _ => Default::default(),
            };
            // Convert PaneInfo to ProtobufPaneInfo
            match pane_info.try_into() {
                Ok(protobuf_pane_info) => Some(protobuf_pane_info),
                Err(e) => {
                    log::error!(
                        "Failed to convert PaneInfo to protobuf for pane {:?}: {}",
                        pane_id,
                        e
                    );
                    None
                },
            }
        },
        Ok(None) => None, // Pane not found
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetPaneInfo timed out for pane {:?} from plugin {}",
                pane_id,
                env.plugin_id
            );
            None
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetPaneInfo channel disconnected for plugin {}",
                env.plugin_id
            );
            None
        },
    };

    let response = ProtobufGetPaneInfoResponse { pane_info };
    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn get_tab_info(env: &PluginEnv, tab_id: usize) {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    let err_context = || {
        format!(
            "failed to get tab info for tab {} from plugin {}",
            tab_id,
            env.name()
        )
    };

    // Create channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Send request to screen thread
    env.senders
        .send_to_screen(ScreenInstruction::GetTabInfo {
            tab_id,
            response_channel: response_sender,
        })
        .with_context(err_context)
        .non_fatal();

    // Block waiting for response with 100ms timeout
    let tab_info = match response_receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(Some(tab_info)) => {
            // Convert TabInfo to ProtobufTabInfo
            match tab_info.try_into() {
                Ok(protobuf_tab_info) => Some(protobuf_tab_info),
                Err(e) => {
                    log::error!(
                        "Failed to convert TabInfo to protobuf for tab {}: {}",
                        tab_id,
                        e
                    );
                    None
                },
            }
        },
        Ok(None) => None, // Tab not found
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetTabInfo timed out for tab {} from plugin {}",
                tab_id,
                env.plugin_id
            );
            None
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetTabInfo channel disconnected for plugin {}",
                env.plugin_id
            );
            None
        },
    };

    let response = ProtobufGetTabInfoResponse { tab_info };
    let _ = wasi_write_object(env, &response.encode_to_vec());
}

fn list_clients(env: &PluginEnv) {
    let _ = env.senders.to_screen.as_ref().map(|sender| {
        sender.send(ScreenInstruction::ListClientsToPlugin(
            env.plugin_id,
            acting_client(env),
        ))
    });
}

fn change_host_folder(env: &PluginEnv, new_host_folder: PathBuf) {
    let _ = env.senders.to_plugin.as_ref().map(|sender| {
        sender.send(PluginInstruction::ChangePluginHostDir(
            new_host_folder,
            env.plugin_id,
            acting_client(env),
        ))
    });
}

fn set_floating_pane_pinned(env: &PluginEnv, pane_id: PaneId, should_be_pinned: bool) {
    let _ = env.senders.to_screen.as_ref().map(|sender| {
        sender.send(ScreenInstruction::SetFloatingPanePinned(
            pane_id,
            should_be_pinned,
        ))
    });
}

fn stack_panes(env: &PluginEnv, pane_ids: Vec<PaneId>) {
    let _ = env.senders.send_to_screen(ScreenInstruction::StackPanes(
        pane_ids,
        acting_client(env),
        None,
    ));
}

fn change_floating_panes_coordinates(
    env: &PluginEnv,
    pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ChangeFloatingPanesCoordinates(
            pane_ids_and_coordinates,
            false,
            None,
        ));
}

fn toggle_pane_borderless(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::TogglePaneBorderless(pane_id, None));
}

fn set_pane_borderless(env: &PluginEnv, pane_id: PaneId, borderless: bool) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::SetPaneBorderless(
            pane_id, borderless, None,
        ));
}

fn set_pane_border_style(env: &PluginEnv, pane_id: PaneId, border_style: BorderStyleOverride) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::SetPaneBorderStyle(
            pane_id,
            border_style,
            None,
        ));
}

fn set_pane_color(env: &PluginEnv, pane_id: PaneId, fg: Option<String>, bg: Option<String>) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::SetPaneColor(pane_id, fg, bg, None));
}

fn scan_host_folder(env: &PluginEnv, folder_to_scan: PathBuf) {
    if !folder_to_scan.starts_with("/host") {
        log::error!(
            "Can only scan files in the /host filesystem, found: {}",
            folder_to_scan.display()
        );
        return;
    }
    let plugin_host_folder = env
        .plugin_cwd
        .canonicalize()
        .unwrap_or_else(|_| env.plugin_cwd.clone());
    let Some(relative) = folder_to_scan.strip_prefix("/host").ok() else {
        // starts_with("/host") above, but strip_prefix can still fail for
        // non-canonical prefix edge cases (e.g. "/host" vs "/host/").
        log::error!(
            "Failed to strip /host prefix from path: {}",
            folder_to_scan.display()
        );
        return;
    };
    let folder_to_scan = plugin_host_folder.join(relative);
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
                    let Some(permit) = try_acquire_directory_scan(env.plugin_id) else {
                        log::warn!(
                            "Not scanning {} for plugin {}: directory scan limit reached",
                            folder_to_scan.display(),
                            env.plugin_id
                        );
                        return;
                    };
                    let send_plugin_instructions = env.senders.to_plugin.clone();
                    let plugin_id = env.plugin_id;
                    let update_target = Some(plugin_id);
                    let client_id = acting_client(env);
                    thread::spawn({
                        move || {
                            let _permit = permit;
                            let mut paths_in_folder = Vec::new();
                            let mut truncated = false;
                            for entry in reading_folder {
                                if paths_in_folder.len() >= MAX_DIRECTORY_SCAN_ENTRIES {
                                    truncated = true;
                                    break;
                                }
                                if let Ok(entry) = entry {
                                    let entry_metadata = entry.metadata().ok().map(|m| m.into());
                                    let entry_path = entry.path();
                                    let Ok(relative) = entry_path.strip_prefix(&plugin_host_folder)
                                    else {
                                        // Symlink / FS race can put an entry outside the host root.
                                        continue;
                                    };
                                    paths_in_folder.push((
                                        PathBuf::from("/host").join(relative),
                                        entry_metadata.into(),
                                    ));
                                }
                            }
                            if truncated {
                                log::warn!(
                                    "Directory scan for plugin {plugin_id} stopped after {MAX_DIRECTORY_SCAN_ENTRIES} entries"
                                );
                            }
                            let batch = super::file_system_update_within_host_pipe(paths_in_folder);
                            let event = if batch.truncated {
                                Event::FileSystemUpdateIncomplete(batch.entries)
                            } else {
                                Event::FileSystemUpdate(batch.entries)
                            };
                            let _ = send_plugin_instructions
                                .ok_or(anyhow!("found no sender to send plugin instruction to"))
                                .map(|sender| {
                                    let _ = sender.send(PluginInstruction::Update(vec![(
                                        update_target,
                                        Some(client_id),
                                        event,
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

#[cfg(not(windows))]
fn list_windows_volumes(_env: &PluginEnv) {
    log::error!("ListWindowsVolumes is only supported on Windows");
}

fn set_soft_keyboard(env: &PluginEnv, on: bool) {
    env.senders
        .send_to_screen(ScreenInstruction::SetSoftKeyboard {
            client_id: acting_client(env),
            on,
        })
        .with_context(|| {
            format!(
                "failed to dispatch SetSoftKeyboard for plugin {}",
                env.plugin_id
            )
        })
        .non_fatal();
}

#[cfg(windows)]
fn list_windows_volumes(env: &PluginEnv) {
    let send_plugin_instructions = env.senders.to_plugin.clone();
    let update_target = Some(env.plugin_id);
    let client_id = acting_client(env);
    thread::spawn(move || {
        let mut entries = enumerate_drives();
        entries.extend(enumerate_wsl_distributions());
        let batch = super::file_system_update_within_host_pipe(entries);
        let event = if batch.truncated {
            Event::FileSystemUpdateIncomplete(batch.entries)
        } else {
            Event::FileSystemUpdate(batch.entries)
        };
        let _ = send_plugin_instructions
            .ok_or(anyhow!("found no sender to send plugin instruction to"))
            .map(|sender| {
                let _ = sender.send(PluginInstruction::Update(vec![(
                    update_target,
                    Some(client_id),
                    event,
                )]));
            })
            .non_fatal();
    });
}

#[cfg(windows)]
fn enumerate_drives() -> Vec<(PathBuf, Option<zellij_utils::data::FileMetadata>)> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDriveStringsW;
    let mut buf = [0u16; 256];
    let len = unsafe { GetLogicalDriveStringsW(buf.len() as u32, buf.as_mut_ptr()) };
    // A short buffer returns the required length, which is at least `buf.len()`.
    // A successful length excludes the terminating null, so it is strictly shorter.
    if len == 0 || len as usize >= buf.len() {
        return vec![];
    }
    let drive_strings = &buf[..len as usize];
    let mut entries = vec![];
    for chunk in drive_strings.split(|&c| c == 0) {
        if chunk.is_empty() {
            continue;
        }
        let drive = String::from_utf16_lossy(chunk);
        let normalized = drive.replace('\\', "/");
        let drive_path = PathBuf::from(&normalized);
        let metadata = drive_path.metadata().ok().map(|m| m.into());
        entries.push((drive_path, metadata));
    }
    entries
}

#[cfg(windows)]
fn enumerate_wsl_distributions() -> Vec<(PathBuf, Option<zellij_utils::data::FileMetadata>)> {
    // read_dir on \\wsl.localhost\ or \\wsl$\ can fail with OS error 64/67
    // even when WSL is running, so enumerate distros via `wsl -l -q` instead.
    let output = match std::process::Command::new("wsl")
        .args(["-l", "-q"])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };
    // wsl -l -q outputs UTF-16LE on Windows
    let stdout: Vec<u16> = output
        .stdout
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)]))
        .collect();
    let text = String::from_utf16_lossy(&stdout);
    let mut entries = vec![];
    for line in text.lines() {
        let name = line.trim().trim_matches('\0');
        if name.is_empty() {
            continue;
        }
        // Verify the distro is reachable before listing it
        let unc = format!("\\\\wsl.localhost\\{}", name);
        let metadata = std::fs::metadata(&unc).ok().map(|m| m.into());
        let wsl_path = PathBuf::from(format!("//wsl.localhost/{}/", name));
        entries.push((wsl_path, metadata));
    }
    entries
}

fn resize_pane_with_id(env: &PluginEnv, resize: ResizeStrategy, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ResizePaneWithId(resize, pane_id));
}

fn edit_scrollback_for_pane_with_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::EditScrollbackForPaneWithId(
            pane_id, None,
        ));
}

fn get_pane_scrollback(env: &PluginEnv, pane_id: PaneId, get_full_scrollback: bool) {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    let err_context = || {
        format!(
            "failed to get pane scrollback for pane {:?} from plugin {}",
            pane_id,
            env.name()
        )
    };

    // Create oneshot channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Send request to screen thread
    env.senders
        .send_to_screen(ScreenInstruction::GetPaneScrollback {
            pane_id,
            client_id: acting_client(env),
            get_full_scrollback,
            response_channel: response_sender,
        })
        .with_context(err_context)
        .non_fatal();

    // Block waiting for response with 5 second timeout
    let response = match response_receiver.recv_timeout(Duration::from_secs(5)) {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetPaneScrollback timed out after 5s for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            PaneScrollbackResponse::Err(format!(
                "Timeout retrieving scrollback for pane {:?}",
                pane_id
            ))
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetPaneScrollback channel disconnected for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            PaneScrollbackResponse::Err(format!(
                "Channel disconnected while retrieving scrollback for pane {:?}",
                pane_id
            ))
        },
    };

    // Convert to protobuf and write back to plugin
    ProtobufPaneScrollbackResponse::try_from(response)
        .map_err(|e| anyhow!("Failed to serialize pane scrollback response: {}", e))
        .and_then(|serialized| {
            wasi_write_object(env, &serialized.encode_to_vec())?;
            Ok(())
        })
        .with_context(err_context)
        .non_fatal();
}

fn write_to_pane_id(env: &PluginEnv, bytes: Vec<u8>, pane_id: PaneId) {
    let (completion_tx, completion_rx) = oneshot::channel();
    let send_result = env.senders.send_to_screen(ScreenInstruction::WriteToPaneId(
        bytes,
        pane_id,
        Some(acting_client(env)),
        Some(NotificationEnd::new(completion_tx)),
    ));
    if send_result.is_ok() {
        let wait_forever = false;
        let _ = wait_for_action_completion(completion_rx, "write_to_pane_id", wait_forever);
    }
}

fn write_chars_to_pane_id(env: &PluginEnv, chars: String, pane_id: PaneId) {
    let bytes = chars.into_bytes();
    let (completion_tx, completion_rx) = oneshot::channel();
    let send_result = env.senders.send_to_screen(ScreenInstruction::WriteToPaneId(
        bytes,
        pane_id,
        Some(acting_client(env)),
        Some(NotificationEnd::new(completion_tx)),
    ));
    if send_result.is_ok() {
        let wait_forever = false;
        let _ = wait_for_action_completion(completion_rx, "write_chars_to_pane_id", wait_forever);
    }
}

fn send_sigint_to_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let err_context = || {
        format!(
            "failed to send SIGINT to pane {:?} from plugin {}",
            pane_id,
            env.name()
        )
    };

    env.senders
        .send_to_pty(PtyInstruction::SendSigintToPaneId(pane_id))
        .with_context(err_context)
        .non_fatal();
}

fn send_sigkill_to_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let err_context = || {
        format!(
            "failed to send SIGKILL to pane {:?} from plugin {}",
            pane_id,
            env.name()
        )
    };

    env.senders
        .send_to_pty(PtyInstruction::SendSigkillToPaneId(pane_id))
        .with_context(err_context)
        .non_fatal();
}

fn get_pane_pid(env: &PluginEnv, pane_id: PaneId) {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    let err_context = || {
        format!(
            "failed to get PID for pane {:?} from plugin {}",
            pane_id,
            env.name()
        )
    };

    // Create oneshot channel for response
    let (response_sender, response_receiver) = crossbeam::channel::bounded(1);

    // Send request directly to PTY thread
    env.senders
        .send_to_pty(PtyInstruction::GetPanePid {
            pane_id,
            response_channel: response_sender,
        })
        .with_context(err_context)
        .non_fatal();

    // Block waiting for response with 100ms timeout
    let response = match response_receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetPanePid timed out after 5s for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPanePidResponse::Err(format!("Timeout retrieving PID for pane {:?}", pane_id))
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetPanePid channel disconnected for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPanePidResponse::Err(format!(
                "Channel disconnected while retrieving PID for pane {:?}",
                pane_id
            ))
        },
    };

    // Encode response and write to plugin's stdin
    let protobuf_response = ProtobufGetPanePidResponse::from(response);
    let serialized = protobuf_response.encode_to_vec();
    wasi_write_object(env, &serialized)
        .with_context(err_context)
        .non_fatal();
}

fn get_pane_running_command(env: &PluginEnv, pane_id: PaneId) {
    let (sender, receiver) = crossbeam::channel::bounded(1);

    send_get_pane_running_command_request(env, pane_id, sender);
    let response = await_pane_running_command_response(env, pane_id, receiver);
    write_pane_running_command_response(env, pane_id, response);
}

fn get_session_list(env: &PluginEnv) {
    use crate::background_jobs::{scan_session_list_default_dirs_with_current, session_scan_state};
    use zellij_utils::data::{GetSessionListResponse, SessionListSnapshot};

    let response = match session_scan_state() {
        Some(state) => {
            let (session_name, current_info, plugin_list) = {
                let name = state.current_session_name.lock().unwrap().clone();
                let info = state.current_session_info.lock().unwrap().clone();
                let plugins = state.current_session_plugin_list.lock().unwrap().clone();
                (name, info, plugins)
            };
            let available_layouts = current_info.available_layouts.clone();

            let (live_sessions_map, resurrectable_sessions_map) =
                scan_session_list_default_dirs_with_current(
                    &session_name,
                    &available_layouts,
                    &plugin_list,
                    &current_info,
                );

            let _ = env
                .senders
                .send_to_screen(ScreenInstruction::UpdateSessionInfos(
                    live_sessions_map.clone(),
                    resurrectable_sessions_map.clone(),
                    Some((env.plugin_id, env.client_id)),
                ));

            let mut snapshot = SessionListSnapshot {
                live_sessions: live_sessions_map.into_values().collect(),
                resurrectable_sessions: resurrectable_sessions_map.into_iter().collect(),
            };
            for session_info in &mut snapshot.live_sessions {
                redact_session_info_for_plugin(env, session_info);
            }
            GetSessionListResponse::Ok(snapshot)
        },
        None => GetSessionListResponse::Err("Session-scan state not initialized".to_string()),
    };

    let protobuf_response = ProtobufGetSessionListResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| "failed to write get_session_list response".to_string())
        .non_fatal();
}

fn send_get_pane_running_command_request(
    env: &PluginEnv,
    pane_id: PaneId,
    sender: crossbeam::channel::Sender<GetPaneRunningCommandResponse>,
) {
    env.senders
        .send_to_pty(PtyInstruction::GetPaneRunningCommand {
            pane_id,
            response_channel: sender,
        })
        .with_context(|| format!("Failed to get running command for pane {:?}", pane_id))
        .non_fatal();
}

fn await_pane_running_command_response(
    env: &PluginEnv,
    pane_id: PaneId,
    receiver: crossbeam::channel::Receiver<GetPaneRunningCommandResponse>,
) -> GetPaneRunningCommandResponse {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    match receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetPaneRunningCommand timed out for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPaneRunningCommandResponse::Err(format!(
                "Timeout retrieving running command for pane {:?}",
                pane_id
            ))
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetPaneRunningCommand channel disconnected for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPaneRunningCommandResponse::Err(format!(
                "Channel disconnected while retrieving running command for pane {:?}",
                pane_id
            ))
        },
    }
}

fn write_pane_running_command_response(
    env: &PluginEnv,
    pane_id: PaneId,
    response: GetPaneRunningCommandResponse,
) {
    let protobuf_response = ProtobufGetPaneRunningCommandResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| format!("Failed to get running command for pane {:?}", pane_id))
        .non_fatal();
}

fn get_pane_cwd(env: &PluginEnv, pane_id: PaneId) {
    let (sender, receiver) = crossbeam::channel::bounded(1);

    send_get_pane_cwd_request(env, pane_id, sender);
    let response = await_pane_cwd_response(env, pane_id, receiver);
    write_pane_cwd_response(env, pane_id, response);
}

fn send_get_pane_cwd_request(
    env: &PluginEnv,
    pane_id: PaneId,
    sender: crossbeam::channel::Sender<GetPaneCwdResponse>,
) {
    env.senders
        .send_to_pty(PtyInstruction::GetPaneCwd {
            pane_id,
            response_channel: sender,
        })
        .with_context(|| format!("Failed to get CWD for pane {:?}", pane_id))
        .non_fatal();
}

fn await_pane_cwd_response(
    env: &PluginEnv,
    pane_id: PaneId,
    receiver: crossbeam::channel::Receiver<GetPaneCwdResponse>,
) -> GetPaneCwdResponse {
    use crossbeam::channel::RecvTimeoutError;
    use std::time::Duration;

    match receiver.recv_timeout(Duration::from_millis(100)) {
        Ok(response) => response,
        Err(RecvTimeoutError::Timeout) => {
            log::error!(
                "GetPaneCwd timed out for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPaneCwdResponse::Err(format!("Timeout retrieving CWD for pane {:?}", pane_id))
        },
        Err(RecvTimeoutError::Disconnected) => {
            log::error!(
                "GetPaneCwd channel disconnected for plugin {} requesting pane {:?}",
                env.plugin_id,
                pane_id
            );
            GetPaneCwdResponse::Err(format!(
                "Channel disconnected while retrieving CWD for pane {:?}",
                pane_id
            ))
        },
    }
}

fn write_pane_cwd_response(env: &PluginEnv, pane_id: PaneId, response: GetPaneCwdResponse) {
    let protobuf_response = ProtobufGetPaneCwdResponse::from(response);
    wasi_write_object(env, &protobuf_response.encode_to_vec())
        .with_context(|| format!("Failed to get CWD for pane {:?}", pane_id))
        .non_fatal();
}

fn save_layout(env: &PluginEnv, layout_name: String, layout_kdl: String, overwrite: bool) {
    let err_context = || format!("Failed to save layout '{}'", layout_name);

    let response = try_save_layout(env, &layout_name, &layout_kdl, overwrite)
        .map(|_| SaveLayoutResponse::Ok(()))
        .unwrap_or_else(SaveLayoutResponse::Err);

    // Serialize and send response back to plugin
    let protobuf_response = ProtobufSaveLayoutResponse::from(response);
    let serialized = protobuf_response.encode_to_vec();
    wasi_write_object(env, &serialized)
        .with_context(err_context)
        .non_fatal();
}

fn try_save_layout(
    env: &PluginEnv,
    layout_name: &str,
    layout_kdl: &str,
    overwrite: bool,
) -> Result<(), String> {
    // Step 1: Sanitize layout name for path traversal and invalid characters
    let safe_name = sanitize_layout_name(layout_name)?;

    // Step 2: Validate the layout by parsing it
    Layout::from_kdl(
        layout_kdl,
        Some(format!("{}.kdl", safe_name)),
        None, // No swap layouts
        None, // No cwd override
    )
    .map_err(|config_error| format!("Invalid layout KDL: {:?}", config_error))?;

    // Step 3: Get layout_dir from PluginEnv
    let layout_dir = env
        .layout_dir
        .clone()
        .or_else(default_layout_dir)
        .ok_or_else(|| "Layout directory not found".to_string())?;

    // Step 4: Create file path
    let file_path = layout_dir.join(format!("{}.kdl", safe_name));

    // Step 5: Check if file exists when overwrite=false
    if file_path.exists() && !overwrite {
        return Err(format!(
            "Layout file '{}' already exists. Use overwrite flag to replace it.",
            safe_name
        ));
    }

    // Step 6: Ensure layout directory exists
    std::fs::create_dir_all(layout_dir)
        .map_err(|e| format!("Failed to create layout directory: {}", e))?;

    // Step 7: Write to disk
    // Layout::from_kdl already validated this string; re-parse for formatting.
    let mut parsed_layout: KdlDocument = layout_kdl
        .parse()
        .map_err(|e| format!("Failed to re-parse validated layout KDL: {}", e))?;
    parsed_layout.fmt();
    std::fs::write(&file_path, &parsed_layout.to_string())
        .map_err(|io_error| format!("Failed to write layout file: {}", io_error))?;

    Ok(())
}

fn delete_layout(env: &PluginEnv, layout_name: String) {
    let err_context = || format!("Failed to delete layout '{}'", layout_name);

    let response = try_delete_layout(env, &layout_name)
        .map(|_| DeleteLayoutResponse::Ok(()))
        .unwrap_or_else(|e| DeleteLayoutResponse::Err(e));

    let protobuf_response: ProtobufDeleteLayoutResponse = response.into();
    let serialized = protobuf_response.encode_to_vec();
    wasi_write_object(env, &serialized)
        .with_context(err_context)
        .non_fatal();
}

fn rename_layout(env: &PluginEnv, old_layout_name: String, new_layout_name: String) {
    let err_context = || {
        format!(
            "Failed to rename layout '{}' to '{}'",
            old_layout_name, new_layout_name
        )
    };

    let response = try_rename_layout(env, &old_layout_name, &new_layout_name)
        .map(|_| RenameLayoutResponse::Ok(()))
        .unwrap_or_else(|e| RenameLayoutResponse::Err(e));

    let protobuf_response: ProtobufRenameLayoutResponse = response.into();
    let serialized = protobuf_response.encode_to_vec();
    wasi_write_object(env, &serialized)
        .with_context(err_context)
        .non_fatal();
}

fn try_delete_layout(env: &PluginEnv, layout_name: &str) -> Result<(), String> {
    // Sanitize the layout name to prevent directory traversal
    let safe_name =
        sanitize_layout_name(layout_name).map_err(|e| format!("Invalid layout name: {}", e))?;

    // Get the layout directory from PluginEnv
    let layout_dir = env
        .layout_dir
        .clone()
        .or_else(default_layout_dir)
        .ok_or_else(|| "Layout directory not found".to_string())?;

    // Construct the full file path
    let file_path = layout_dir.join(format!("{}.kdl", safe_name));

    // Check if the file exists
    if !file_path.exists() {
        return Err(format!("Layout '{}' not found", safe_name));
    }

    // Delete the file
    std::fs::remove_file(&file_path).map_err(|e| format!("Failed to delete layout file: {}", e))?;

    Ok(())
}

fn try_rename_layout(
    env: &PluginEnv,
    old_layout_name: &str,
    new_layout_name: &str,
) -> Result<(), String> {
    // Step 1: Sanitize both layout names
    let safe_old_name = sanitize_layout_name(old_layout_name)
        .map_err(|e| format!("Invalid old layout name: {}", e))?;

    let safe_new_name = sanitize_layout_name(new_layout_name)
        .map_err(|e| format!("Invalid new layout name: {}", e))?;

    // Step 2: Get layout directory from PluginEnv
    let layout_dir = env
        .layout_dir
        .clone()
        .or_else(default_layout_dir)
        .ok_or_else(|| "Layout directory not found".to_string())?;

    // Step 3: Construct file paths
    let old_file_path = layout_dir.join(format!("{}.kdl", safe_old_name));
    let new_file_path = layout_dir.join(format!("{}.kdl", safe_new_name));

    // Step 4: Check if source file exists
    if !old_file_path.exists() {
        return Err(format!("Layout '{}' not found", safe_old_name));
    }

    // Step 5: Check if target file already exists (fail if it does - no overwrite)
    if new_file_path.exists() {
        return Err(format!(
            "Layout '{}' already exists. Cannot rename '{}' to existing layout name.",
            safe_new_name, safe_old_name
        ));
    }

    // Step 6: Rename the file
    std::fs::rename(&old_file_path, &new_file_path)
        .map_err(|e| format!("Failed to rename layout file: {}", e))?;

    Ok(())
}

fn edit_layout(env: &PluginEnv, layout_name: String, context: BTreeMap<String, String>) {
    let err_context = || format!("Failed to edit layout '{}'", layout_name);

    let response = try_edit_layout(env, &layout_name, context)
        .map(|_| EditLayoutResponse::Ok(()))
        .unwrap_or_else(|e| EditLayoutResponse::Err(e));

    let protobuf_response: ProtobufEditLayoutResponse = response.into();
    let serialized = protobuf_response.encode_to_vec();
    wasi_write_object(env, &serialized)
        .with_context(err_context)
        .non_fatal();
}

fn try_edit_layout(
    env: &PluginEnv,
    layout_name: &str,
    context: BTreeMap<String, String>,
) -> Result<(), String> {
    // Sanitize the layout name to prevent directory traversal
    let safe_name =
        sanitize_layout_name(layout_name).map_err(|e| format!("Invalid layout name: {}", e))?;

    // Get the layout directory from PluginEnv
    let layout_dir = env
        .layout_dir
        .clone()
        .or_else(default_layout_dir)
        .ok_or_else(|| "Layout directory not found".to_string())?;

    // Construct the full file path
    let file_path = layout_dir.join(format!("{}.kdl", safe_name));

    // Create FileToOpen for the layout file
    let file_to_open = FileToOpen {
        border_style: None,
        path: file_path,
        line_number: None,
        cwd: Some(layout_dir.clone()),
    };

    // Create an Action::EditFile
    let action = Action::EditFile {
        payload: OpenFilePayload::new(
            file_to_open.path,
            file_to_open.line_number,
            file_to_open.cwd,
        )
        .with_originating_plugin(OriginatingPlugin::new(
            env.plugin_id,
            acting_client(env),
            context,
        )),
        direction: None,
        floating: false,
        in_place: true,
        close_replaced_pane: false,
        start_suppressed: false,
        coordinates: None,
        near_current_pane: true,
        no_focus: false,
        tab_id: None,
    };
    // Route the action - this is fallible
    route_action(
        action,
        acting_client(env),
        None,
        source_pane(env),
        env.senders.clone(),
        env.default_shell.clone(),
        None,
        env.default_mode.clone(),
        None,
        false,
    )
    .map(|_| ())
    .map_err(|e| format!("Failed to route edit action: {:?}", e))
}

/// Sanitize layout name to prevent path traversal and invalid filenames
/// Returns the sanitized name or an error message
fn sanitize_layout_name(name: &str) -> Result<String, String> {
    // Check for empty name
    if name.is_empty() {
        return Err("Layout name cannot be empty".to_string());
    }

    // Check for path separators (prevent directory traversal)
    if name.contains('/') || name.contains('\\') {
        return Err("Layout name cannot contain path separators".to_string());
    }

    // Check for parent directory references
    if name.contains("..") {
        return Err("Layout name cannot contain '..'".to_string());
    }

    // Check length (most filesystems have 255 char limit, leave room for .kdl)
    if name.len() > 250 {
        return Err("Layout name too long (max 250 characters)".to_string());
    }

    // Only allow alphanumeric, hyphens, underscores, and spaces
    let valid_chars = name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ');

    if !valid_chars {
        return Err(
            "Layout name can only contain letters, numbers, hyphens, underscores, and spaces"
                .to_string(),
        );
    }

    Ok(name.to_string())
}

fn move_pane_with_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::MovePaneWithPaneId(pane_id));
}

fn move_pane_with_pane_id_in_direction(env: &PluginEnv, pane_id: PaneId, direction: Direction) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::MovePaneWithPaneIdInDirection(
            pane_id, direction,
        ));
}

fn clear_screen_for_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ClearScreenForPaneId(pane_id));
}

fn scroll_up_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ScrollUpInPaneId(
            acting_client(env),
            pane_id,
        ));
}

fn scroll_down_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ScrollDownInPaneId(
            acting_client(env),
            pane_id,
        ));
}

fn scroll_to_top_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ScrollToTopInPaneId(pane_id));
}

fn scroll_to_bottom_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ScrollToBottomInPaneId(pane_id));
}

fn page_scroll_up_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::PageScrollUpInPaneId(
            acting_client(env),
            pane_id,
        ));
}

fn page_scroll_down_in_pane_id(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::PageScrollDownInPaneId(
            acting_client(env),
            pane_id,
        ));
}

fn toggle_pane_id_fullscreen(env: &PluginEnv, pane_id: PaneId) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::TogglePaneIdFullscreen(pane_id));
}

fn toggle_pane_embed_or_eject_for_pane_id(env: &PluginEnv, pane_id: PaneId, show_and_focus: bool) {
    let instruction = if show_and_focus {
        ScreenInstruction::MovePaneBetweenLayersForPaneId(pane_id, Some(acting_client(env)))
    } else {
        ScreenInstruction::TogglePaneEmbedOrEjectForPaneId(pane_id)
    };
    let _ = env.senders.send_to_screen(instruction);
}

fn close_tab_with_index(env: &PluginEnv, tab_index: usize) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::CloseTabWithIndex(tab_index));
}

fn break_panes_to_new_tab(
    env: &PluginEnv,
    pane_ids: Vec<PaneId>,
    new_tab_name: Option<String>,
    should_change_focus_to_new_tab: bool,
) {
    let default_shell = env.default_shell.clone().or_else(|| {
        Some(TerminalAction::RunCommand(RunCommand {
            command: env.path_to_default_shell.clone(),
            use_terminal_title: true,
            ..Default::default()
        }))
    });

    // Create completion channel to receive tab ID
    let (tx, rx) = tokio::sync::oneshot::channel();
    let completion_tx = Some(NotificationEnd::new(tx));

    let result = env
        .senders
        .send_to_screen(ScreenInstruction::BreakPanesToNewTab {
            pane_ids,
            default_shell,
            new_tab_name,
            should_change_focus_to_new_tab,
            client_id: acting_client(env),
            completion_tx,
        });

    let tab_id: BreakPanesToNewTabResponse = if result.is_ok() {
        wait_for_action_completion(rx, "break_panes_to_new_tab", false).affected_tab_id
    } else {
        None
    };

    let response = ProtobufBreakPanesToNewTabResponse::from(tab_id);
    wasi_write_object(env, &response.encode_to_vec()).non_fatal();
}

fn break_panes_to_tab_with_index(
    env: &PluginEnv,
    pane_ids: Vec<PaneId>,
    should_change_focus_to_new_tab: bool,
    tab_index: usize,
) {
    // Create completion channel to receive tab ID
    let (tx, rx) = tokio::sync::oneshot::channel();
    let completion_tx = Some(NotificationEnd::new(tx));

    let result = env
        .senders
        .send_to_screen(ScreenInstruction::BreakPanesToTabWithIndex {
            pane_ids,
            tab_index,
            client_id: acting_client(env),
            should_change_focus_to_new_tab,
            completion_tx,
        });

    let tab_id: BreakPanesToTabWithIndexResponse = if result.is_ok() {
        wait_for_action_completion(rx, "break_panes_to_tab_with_index", false).affected_tab_id
    } else {
        None
    };

    let response = ProtobufBreakPanesToTabWithIndexResponse::from(tab_id);
    wasi_write_object(env, &response.encode_to_vec()).non_fatal();
}

fn switch_tab_to_id(env: &PluginEnv, tab_id: u64) {
    let tab_id = tab_id as usize;
    env.senders
        .send_to_screen(ScreenInstruction::GoToTabWithId(
            tab_id,
            Some(acting_client(env)),
            None,
        ))
        .with_context(|| {
            format!(
                "failed to switch to tab {} from plugin {}",
                tab_id,
                env.name()
            )
        })
        .non_fatal();
}

fn go_to_tab_with_id(env: &PluginEnv, tab_id: u64) {
    let tab_id = tab_id as usize;
    env.senders
        .send_to_screen(ScreenInstruction::GoToTabWithId(
            tab_id,
            Some(acting_client(env)),
            None,
        ))
        .with_context(|| format!("failed to go to tab {} from plugin {}", tab_id, env.name()))
        .non_fatal();
}

fn close_tab_with_id(env: &PluginEnv, tab_id: u64) {
    let tab_id = tab_id as usize;
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::CloseTabWithId(tab_id, None));
}

fn rename_tab_with_id(env: &PluginEnv, tab_id: u64, new_name: &str) {
    let tab_id = tab_id as usize;
    let rename_tab_action =
        ScreenInstruction::RenameTabWithId(tab_id, new_name.as_bytes().to_vec(), None);
    env.senders
        .send_to_screen(rename_tab_action)
        .with_context(|| format!("Failed to rename tab {}", tab_id))
        .non_fatal();
}

fn break_panes_to_tab_with_id(
    env: &PluginEnv,
    pane_ids: Vec<PaneId>,
    should_change_focus_to_target_tab: bool,
    tab_id: u64,
) {
    let tab_id = tab_id as usize;

    // Create completion channel to receive tab ID
    let (tx, rx) = tokio::sync::oneshot::channel();
    let completion_tx = Some(NotificationEnd::new(tx));

    let result = env
        .senders
        .send_to_screen(ScreenInstruction::BreakPanesToTabWithId {
            pane_ids,
            tab_id,
            should_change_focus_to_target_tab,
            client_id: acting_client(env),
            completion_tx,
        });

    let result_tab_id: BreakPanesToTabWithIdResponse = if result.is_ok() {
        wait_for_action_completion(rx, "break_panes_to_tab_with_id", false).affected_tab_id
    } else {
        None
    };

    let response = ProtobufBreakPanesToTabWithIdResponse::from(result_tab_id);
    wasi_write_object(env, &response.encode_to_vec()).non_fatal();
}

fn reload_plugin(env: &PluginEnv, plugin_id: u32) {
    let _ = env
        .senders
        .send_to_plugin(PluginInstruction::ReloadPluginWithId(plugin_id));
}

fn load_new_plugin(
    env: &PluginEnv,
    url: String,
    config: BTreeMap<String, String>,
    load_in_background: bool,
    skip_plugin_cache: bool,
) {
    let url = if &url == "zellij:OWN_URL" {
        env.plugin.location.display()
    } else {
        url
    };
    if load_in_background {
        match RunPluginOrAlias::from_url(&url, &Some(config), None, Some(env.plugin_cwd.clone())) {
            Ok(run_plugin_or_alias) => {
                let _ = env
                    .senders
                    .send_to_plugin(PluginInstruction::LoadBackgroundPlugin(
                        run_plugin_or_alias,
                        acting_client(env),
                    ));
            },
            Err(e) => {
                log::error!("Failed to load new plugin: {:?}", e);
            },
        }
    } else {
        let should_float = Some(true);
        let should_be_open_in_place = false;
        let pane_title = None;
        let tab_index = None;
        let pane_id_to_replace = None;
        let client_id = acting_client(env);
        let size = Default::default();
        let cwd = Some(env.plugin_cwd.clone());
        let skip_cache = skip_plugin_cache;
        match RunPluginOrAlias::from_url(&url, &Some(config), None, Some(env.plugin_cwd.clone())) {
            Ok(run_plugin_or_alias) => {
                let _ = env.senders.send_to_plugin(PluginInstruction::Load(
                    should_float,
                    should_be_open_in_place,
                    false, // close_replaced_pane
                    pane_title,
                    run_plugin_or_alias,
                    tab_index,
                    pane_id_to_replace,
                    client_id,
                    size,
                    cwd,
                    None,
                    skip_cache,
                    None,
                    None,
                    None,
                    None,
                ));
            },
            Err(e) => {
                log::error!("Failed to load new plugin: {:?}", e);
            },
        }
    }
}

fn start_web_server(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_server(ServerInstruction::StartWebServer(acting_client(env)));
}

fn stop_web_server(_env: &PluginEnv) {
    #[cfg(feature = "web_server_capability")]
    let _ = shutdown_all_webserver_instances();
    #[cfg(not(feature = "web_server_capability"))]
    log::error!("This instance of Zellij was compiled without web server capabilities");
}

fn query_web_server_status(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_background_jobs(BackgroundJob::QueryZellijWebServerStatus);
}

fn share_current_session(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_server(ServerInstruction::ShareCurrentSession(acting_client(env)));
}

fn stop_sharing_current_session(env: &PluginEnv) {
    let _ = env
        .senders
        .send_to_server(ServerInstruction::StopSharingCurrentSession(acting_client(
            env,
        )));
}

fn group_and_ungroup_panes(
    env: &PluginEnv,
    panes_to_group: Vec<PaneId>,
    panes_to_ungroup: Vec<PaneId>,
    for_all_clients: bool,
) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::GroupAndUngroupPanes(
            panes_to_group,
            panes_to_ungroup,
            for_all_clients,
            acting_client(env),
        ));
}

fn highlight_and_unhighlight_panes(
    env: &PluginEnv,
    panes_to_highlight: Vec<PaneId>,
    panes_to_unhighlight: Vec<PaneId>,
) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::HighlightAndUnhighlightPanes(
            panes_to_highlight,
            panes_to_unhighlight,
            acting_client(env),
        ));
}

fn close_multiple_panes(env: &PluginEnv, pane_ids: Vec<PaneId>) {
    for pane_id in pane_ids {
        match pane_id {
            PaneId::Terminal(terminal_pane_id) => {
                close_terminal_pane(env, terminal_pane_id);
            },
            PaneId::Plugin(plugin_pane_id) => {
                close_plugin_pane(env, plugin_pane_id);
            },
        }
    }
}

fn float_multiple_panes(env: &PluginEnv, pane_ids: Vec<PaneId>) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::FloatMultiplePanes(
            pane_ids,
            acting_client(env),
        ));
}

fn embed_multiple_panes(env: &PluginEnv, pane_ids: Vec<PaneId>) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::EmbedMultiplePanes(
            pane_ids,
            acting_client(env),
        ));
}

#[cfg(feature = "web_server_capability")]
fn generate_web_login_token(env: &PluginEnv, token_label: Option<String>, read_only: bool) {
    let serialized = match create_token(token_label, read_only) {
        Ok((token, token_label)) => CreateTokenResponse {
            token: Some(token),
            token_label: Some(token_label),
            error: None,
        },
        Err(e) => CreateTokenResponse {
            token: None,
            token_label: None,
            error: Some(e.to_string()),
        },
    };
    let _ = wasi_write_object(env, &serialized.encode_to_vec());
}

#[cfg(not(feature = "web_server_capability"))]
fn generate_web_login_token(env: &PluginEnv, _token_label: Option<String>, _read_only: bool) {
    log::error!("This version of Zellij was compiled without the web server capabilities!");
    let empty_vec: Vec<&str> = vec![];
    let _ = wasi_write_object(env, &empty_vec);
}

#[cfg(feature = "web_server_capability")]
fn revoke_web_login_token(env: &PluginEnv, token_label: String) {
    let serialized = match revoke_token(&token_label) {
        Ok(true) => RevokeTokenResponse {
            successfully_revoked: true,
            error: None,
        },
        Ok(false) => RevokeTokenResponse {
            successfully_revoked: false,
            error: Some(format!("Token with label {} not found", token_label)),
        },
        Err(e) => RevokeTokenResponse {
            successfully_revoked: false,
            error: Some(e.to_string()),
        },
    };
    let _ = wasi_write_object(env, &serialized.encode_to_vec());
}

#[cfg(not(feature = "web_server_capability"))]
fn revoke_web_login_token(env: &PluginEnv, _token_label: String) {
    log::error!("This version of Zellij was compiled without the web server capabilities!");
    let empty_vec: Vec<&str> = vec![];
    let _ = wasi_write_object(env, &empty_vec);
}

#[cfg(feature = "web_server_capability")]
fn revoke_all_web_login_tokens(env: &PluginEnv) {
    let serialized = match revoke_all_tokens() {
        Ok(_) => RevokeAllWebTokensResponse {
            successfully_revoked: true,
            error: None,
        },
        Err(e) => RevokeAllWebTokensResponse {
            successfully_revoked: false,
            error: Some(e.to_string()),
        },
    };
    let _ = wasi_write_object(env, &serialized.encode_to_vec());
}

#[cfg(not(feature = "web_server_capability"))]
fn revoke_all_web_login_tokens(env: &PluginEnv) {
    log::error!("This version of Zellij was compiled without the web server capabilities!");
    let empty_vec: Vec<&str> = vec![];
    let _ = wasi_write_object(env, &empty_vec);
}

#[cfg(feature = "web_server_capability")]
fn rename_web_login_token(env: &PluginEnv, old_name: String, new_name: String) {
    let serialized = match rename_token(&old_name, &new_name) {
        Ok(_) => RenameWebTokenResponse {
            successfully_renamed: true,
            error: None,
        },
        Err(e) => RenameWebTokenResponse {
            successfully_renamed: false,
            error: Some(e.to_string()),
        },
    };
    let _ = wasi_write_object(env, &serialized.encode_to_vec());
}

#[cfg(not(feature = "web_server_capability"))]
fn rename_web_login_token(env: &PluginEnv, _old_name: String, _new_name: String) {
    log::error!("This version of Zellij was compiled without the web server capabilities!");
    let empty_vec: Vec<&str> = vec![];
    let _ = wasi_write_object(env, &empty_vec);
}

#[cfg(feature = "web_server_capability")]
fn list_web_login_tokens(env: &PluginEnv) {
    let serialized = match list_tokens() {
        Ok(token_list) => ListTokensResponse {
            tokens: token_list.iter().map(|t| t.name.clone()).collect(),
            creation_times: token_list.iter().map(|t| t.created_at.clone()).collect(),
            read_only_flags: token_list.iter().map(|t| t.read_only).collect(),
            error: None,
        },
        Err(e) => ListTokensResponse {
            tokens: vec![],
            creation_times: vec![],
            read_only_flags: vec![],
            error: Some(e.to_string()),
        },
    };
    let _ = wasi_write_object(env, &serialized.encode_to_vec());
}

#[cfg(not(feature = "web_server_capability"))]
fn list_web_login_tokens(env: &PluginEnv) {
    log::error!("This version of Zellij was compiled without the web server capabilities!");
    let empty_vec: Vec<&str> = vec![];
    let _ = wasi_write_object(env, &empty_vec);
}

fn set_self_mouse_selection_support(env: &PluginEnv, selection_support: bool) {
    env.senders
        .send_to_screen(ScreenInstruction::SetMouseSelectionSupport(
            PaneId::Plugin(self_pane_id(env)),
            selection_support,
        ))
        .with_context(|| {
            format!(
                "failed to set plugin {} selectable from plugin {}",
                selection_support,
                env.name()
            )
        })
        .non_fatal();
}

fn set_self_collapsed(env: &PluginEnv, collapsed: bool) {
    env.senders
        .send_to_screen(ScreenInstruction::SetPaneCollapsed(
            PaneId::Plugin(self_pane_id(env)),
            collapsed,
        ))
        .with_context(|| {
            format!(
                "failed to set collapsed {} from plugin {}",
                collapsed,
                env.name()
            )
        })
        .non_fatal();
}

fn intercept_key_presses(env: &mut PluginEnv) {
    env.intercepting_key_presses = true;
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::InterceptKeyPresses(
            env.plugin_id,
            acting_client(env),
        ));
}

fn clear_key_presses_intercepts(env: &mut PluginEnv) {
    env.intercepting_key_presses = false;
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ClearKeyPressesIntercepts(
            env.plugin_id,
            acting_client(env),
        ));
}

fn replace_pane_with_existing_pane(
    env: &mut PluginEnv,
    pane_to_replace: PaneId,
    existing_pane: PaneId,
    suppress_replaced_pane: bool,
) {
    let (completion_tx, completion_rx) = oneshot::channel();
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::ReplacePaneWithExistingPane(
            pane_to_replace,
            existing_pane,
            suppress_replaced_pane,
            Some(NotificationEnd::new(completion_tx)),
        ));
    let _ = wait_for_action_completion(completion_rx, "replace_pane_with_existing_pane", false);
}

fn override_layout(
    env: &mut PluginEnv,
    layout_info: LayoutInfo,
    retain_existing_terminal_panes: bool,
    retain_existing_plugin_panes: bool,
    apply_only_to_active_tab: bool,
    context: BTreeMap<String, String>,
) -> Result<()> {
    let layout = Layout::from_layout_info(&env.layout_dir, layout_info)
        .map_err(|e| anyhow!("Failed to parse layout: {:?}", e))?;

    // Convert all tabs to Vec<TabLayoutInfo>
    let tabs: Vec<TabLayoutInfo> = layout
        .tabs
        .iter()
        .enumerate()
        .map(|(index, (tab_name, tiled, floating))| TabLayoutInfo {
            tab_index: index,
            tab_name: tab_name.clone(),
            tiled_layout: tiled.clone(),
            floating_layouts: floating.clone(),
            swap_tiled_layouts: Some(layout.swap_tiled_layouts.clone()),
            swap_floating_layouts: Some(layout.swap_floating_layouts.clone()),
        })
        .collect();

    // If no tabs, create default tab
    let tabs = if tabs.is_empty() {
        let (tiled, floating) = layout.new_tab();
        vec![TabLayoutInfo {
            tab_index: 0,
            tab_name: None,
            tiled_layout: tiled,
            floating_layouts: floating,
            swap_tiled_layouts: Some(layout.swap_tiled_layouts),
            swap_floating_layouts: Some(layout.swap_floating_layouts),
        }]
    } else {
        tabs
    };

    let action = Action::OverrideLayout {
        tabs,
        retain_existing_terminal_panes,
        retain_existing_plugin_panes,
        apply_only_to_active_tab,
    };
    run_action(env, action, context)?;
    Ok(())
}

// Custom panic handler for plugins.
//
// This is called when a panic occurs in a plugin. Since most panics will likely originate in the
// code trying to deserialize an `Event` upon a plugin state update, we read some panic message,
// formatted as string from the plugin.
fn report_panic(env: &PluginEnv, msg: &str) {
    log::error!("PANIC IN PLUGIN!\n\r{}", msg);
    if env.shared.is_some() {
        let _ = env
            .senders
            .send_to_plugin(PluginInstruction::UnregisterSettingsEntry(env.plugin_id));
    }
    for pane_id in crash_pane_ids(env) {
        handle_plugin_crash(pane_id, env.generation, msg.to_owned(), env.senders.clone());
    }
    if env.shared.is_some() {
        // shared events and pipes are dispatched by instance id, see `Quarantine`
        let _ = env
            .senders
            .send_to_plugin(PluginInstruction::Quarantine(env.plugin_id, env.generation));
    }
}

// Helper Functions ---------------------------------------------------------------------------------------------------

pub fn wasi_read_string(plugin_env: &PluginEnv) -> Result<String> {
    let err_context = || format!("failed to read string from WASI env");

    let mut buf = vec![];
    plugin_env
        .stdout_pipe
        .lock()
        // SAFETY: Mutex/lock Err only on poison (prior panic); fail-fast is intentional.
        .unwrap()
        .read_to_end(&mut buf)
        .map_err(anyError::new)
        .with_context(err_context)?;
    let buf = String::from_utf8_lossy(&buf);

    // https://stackoverflow.com/questions/66450942/in-rust-is-there-a-way-to-make-literal-newlines-in-r-using-windows-c
    Ok(buf.replace("\n", "\n\r"))
}

pub fn wasi_write_string(plugin_env: &PluginEnv, buf: &str) -> Result<()> {
    // `writeln!("{buf}\r")` is the bytes plugins already read. Refuse the whole
    // line when it does not fit; a short write would be replayed as success.
    let mut stdin = plugin_env.stdin_pipe.lock().unwrap();
    crate::plugins::plugin_loader::append_capped_host_pipe(
        &mut stdin,
        &[buf.as_bytes(), b"\r\n"],
        "stdin",
    )
    .map_err(anyError::new)
    .with_context(|| format!("failed to write string to WASI env"))?;
    Ok(())
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
pub(crate) fn check_command_permission(
    plugin_env: &PluginEnv,
    command: &PluginCommand,
) -> (PermissionStatus, Option<PermissionType>) {
    if plugin_env.plugin.is_builtin() {
        // built-in plugins can do all the things because they're part of the application and
        // there's no use to deny them anything
        return (PermissionStatus::Granted, None);
    }
    let permission = match required_permission(command) {
        Some(permission) => permission,
        None => return (PermissionStatus::Granted, None),
    };

    let granted = plugin_env.permissions.lock().unwrap().clone();
    let missing = std::iter::once(permission)
        .chain(additional_required_permissions(command).iter().copied())
        .find(|required| {
            !granted
                .as_ref()
                .map(|granted| granted.contains(required))
                .unwrap_or(false)
        });
    match missing {
        None => (PermissionStatus::Granted, None),
        Some(missing) => (PermissionStatus::Denied, Some(missing)),
    }
}

fn additional_required_permissions(command: &PluginCommand) -> &'static [PermissionType] {
    match command {
        PluginCommand::SetLiveView(..) | PluginCommand::SetLiveViewInput(..) => {
            &[PermissionType::WriteToStdin]
        },
        _ => &[],
    }
}

pub(crate) fn required_permission(command: &PluginCommand) -> Option<PermissionType> {
    let permission = match command {
        PluginCommand::OpenFile(..)
        | PluginCommand::OpenFileFloating(..)
        | PluginCommand::OpenFileNearPlugin(..)
        | PluginCommand::OpenFileFloatingNearPlugin(..)
        | PluginCommand::OpenFileInPlaceOfPlugin(..)
        | PluginCommand::OpenFileInPlace(..)
        | PluginCommand::OpenEditPaneInPlaceOfPaneId(..) => PermissionType::OpenFiles,
        PluginCommand::OpenTerminal(..)
        | PluginCommand::OpenTerminalNearPlugin(..)
        | PluginCommand::StartOrReloadPlugin(..)
        | PluginCommand::OpenTerminalFloating(..)
        | PluginCommand::OpenTerminalFloatingNearPlugin(..)
        | PluginCommand::OpenTerminalInPlace(..)
        | PluginCommand::OpenTerminalInPlaceOfPlugin(..)
        | PluginCommand::OpenPluginPaneInNewTab { .. }
        | PluginCommand::OpenPluginPaneFloating { .. }
        | PluginCommand::NewTiledPaneInTab { .. }
        | PluginCommand::OpenTerminalPaneInPlaceOfPaneId(..) => {
            PermissionType::OpenTerminalsOrPlugins
        },
        PluginCommand::OpenCommandPane(..)
        | PluginCommand::OpenCommandPaneNearPlugin(..)
        | PluginCommand::OpenCommandPaneFloating(..)
        | PluginCommand::OpenCommandPaneFloatingNearPlugin(..)
        | PluginCommand::OpenCommandPaneInPlace(..)
        | PluginCommand::OpenCommandPaneInPlaceOfPlugin(..)
        | PluginCommand::OpenCommandPaneBackground(..)
        | PluginCommand::OpenCommandPaneInPlaceOfPaneId(..)
        | PluginCommand::RunCommand(..)
        | PluginCommand::ExecCmd(..) => PermissionType::RunCommands,
        PluginCommand::WebRequest(..) => PermissionType::WebAccess,
        PluginCommand::Write(..)
        | PluginCommand::WriteChars(..)
        | PluginCommand::WriteToPaneId(..)
        | PluginCommand::WriteCharsToPaneId(..)
        | PluginCommand::WriteToSessionPane { .. }
        | PluginCommand::SendKeyToSessionPane { .. } => PermissionType::WriteToStdin,
        PluginCommand::SetLiveView(..) | PluginCommand::SetLiveViewInput(..) => {
            PermissionType::ReadPaneContents
        },
        PluginCommand::MarkSessionPaneSeen { .. }
        | PluginCommand::MarkAllSeen { .. }
        | PluginCommand::OpenSessionTerminal(..)
        | PluginCommand::DesktopIntegration(..)
        | PluginCommand::ToggleSessionPaneMute { .. }
        | PluginCommand::ToggleSessionPaneIgnoreOutput { .. }
        | PluginCommand::ToggleSessionPaneWatch { .. }
        | PluginCommand::FocusSessionPane { .. }
        | PluginCommand::RegisterSettingsEntry(..)
        | PluginCommand::UnregisterSettingsEntry => PermissionType::ChangeApplicationState,
        PluginCommand::CopyToClipboard(..) => PermissionType::WriteToClipboard,
        PluginCommand::SwitchTabTo(..)
        | PluginCommand::SwitchToMode(..)
        | PluginCommand::NewTabsWithLayout(..)
        | PluginCommand::NewTabsWithLayoutInfo(..)
        | PluginCommand::NewTab { .. }
        | PluginCommand::NewTabUnfocused { .. }
        | PluginCommand::GoToNextTab
        | PluginCommand::GoToPreviousTab
        | PluginCommand::Resize(..)
        | PluginCommand::ResizeWithDirection(..)
        | PluginCommand::FocusNextPane
        | PluginCommand::FocusPreviousPane
        | PluginCommand::FocusLastPane
        | PluginCommand::MoveFocus(..)
        | PluginCommand::MoveFocusOrTab(..)
        | PluginCommand::Detach
        | PluginCommand::EditScrollback
        | PluginCommand::EditScrollbackForPaneWithId(..)
        | PluginCommand::ToggleTab
        | PluginCommand::MovePane
        | PluginCommand::MovePaneWithDirection(..)
        | PluginCommand::MovePaneWithPaneId(..)
        | PluginCommand::MovePaneWithPaneIdInDirection(..)
        | PluginCommand::ClearScreen
        | PluginCommand::ClearScreenForPaneId(..)
        | PluginCommand::ScrollUp
        | PluginCommand::ScrollUpInPaneId(..)
        | PluginCommand::ScrollDown
        | PluginCommand::ScrollDownInPaneId(..)
        | PluginCommand::ScrollToTop
        | PluginCommand::ScrollToTopInPaneId(..)
        | PluginCommand::ScrollToBottom
        | PluginCommand::ScrollToBottomInPaneId(..)
        | PluginCommand::PageScrollUp
        | PluginCommand::PageScrollUpInPaneId(..)
        | PluginCommand::PageScrollDown
        | PluginCommand::PageScrollDownInPaneId(..)
        | PluginCommand::ToggleFocusFullscreen
        | PluginCommand::ToggleFocusNoUiFullscreen
        | PluginCommand::FocusHostSession
        | PluginCommand::TogglePaneIdFullscreen(..)
        | PluginCommand::TogglePaneFrames
        | PluginCommand::SetPaneFrameStyle(..)
        | PluginCommand::ToggleBars(..)
        | PluginCommand::HideBars(..)
        | PluginCommand::ShowBars
        | PluginCommand::ToggleFloatingPanes { .. }
        | PluginCommand::NewPane
        | PluginCommand::TogglePaneEmbedOrEject
        | PluginCommand::TogglePaneEmbedOrEjectForPaneId(..)
        | PluginCommand::UndoRenamePane
        | PluginCommand::CloseFocus
        | PluginCommand::ToggleActiveTabSync
        | PluginCommand::CloseFocusedTab
        | PluginCommand::UndoRenameTab
        | PluginCommand::QuitZellij
        | PluginCommand::PreviousSwapLayout
        | PluginCommand::NextSwapLayout
        | PluginCommand::ApplyTiledSwapLayout(..)
        | PluginCommand::ApplyFloatingSwapLayout(..)
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
        | PluginCommand::RerunCommandPane(..)
        | PluginCommand::ResizePaneIdWithDirection(..)
        | PluginCommand::CloseTabWithIndex(..)
        | PluginCommand::BreakPanesToNewTab(..)
        | PluginCommand::BreakPanesToTabWithIndex(..)
        | PluginCommand::SwitchTabToId(..)
        | PluginCommand::GoToTabWithId(..)
        | PluginCommand::CloseTabWithId(..)
        | PluginCommand::RenameTabWithId(..)
        | PluginCommand::BreakPanesToTabWithId(..)
        | PluginCommand::ReloadPlugin(..)
        | PluginCommand::LoadNewPlugin { .. }
        | PluginCommand::SetFloatingPanePinned(..)
        | PluginCommand::StackPanes(..)
        | PluginCommand::ChangeFloatingPanesCoordinates(..)
        | PluginCommand::TogglePaneBorderless(..)
        | PluginCommand::SetPaneBorderless(..)
        | PluginCommand::SetPaneBorderStyle(..)
        | PluginCommand::SetPaneColor(..)
        | PluginCommand::GroupAndUngroupPanes(..)
        | PluginCommand::HighlightAndUnhighlightPanes(..)
        | PluginCommand::CloseMultiplePanes(..)
        | PluginCommand::FloatMultiplePanes(..)
        | PluginCommand::EmbedMultiplePanes(..)
        | PluginCommand::ReplacePaneWithExistingPane(..)
        | PluginCommand::KillSessions(..)
        | PluginCommand::KillSessionsAndReply(..)
        | PluginCommand::DeleteDeadSessionAndReply(..)
        | PluginCommand::DeleteAllDeadSessionsAndReply
        | PluginCommand::SendSigintToPaneId(..)
        | PluginCommand::SendSigkillToPaneId(..)
        | PluginCommand::OverrideLayout(..)
        | PluginCommand::SaveLayout { .. }
        | PluginCommand::DeleteLayout { .. }
        | PluginCommand::RenameLayout { .. }
        | PluginCommand::EditLayout { .. }
        | PluginCommand::ShowFloatingPanes { .. }
        | PluginCommand::HideFloatingPanes { .. }
        | PluginCommand::SetPaneRegexHighlights(..)
        | PluginCommand::ClearPaneHighlights(..)
        | PluginCommand::SetSoftKeyboard(..)
        | PluginCommand::OpenContextMenu { .. }
        | PluginCommand::OpenPluginPopup { .. }
        | PluginCommand::SetPopupSize { .. }
        | PluginCommand::SetPopupFocused(..)
        | PluginCommand::PopupToFloatingPane(..)
        | PluginCommand::SwitchSessionAndCloseCurrent(..) => PermissionType::ChangeApplicationState,
        PluginCommand::Prompt { .. } | PluginCommand::ReplyToPrompt { .. } => {
            PermissionType::OpenTerminalsOrPlugins
        },
        PluginCommand::RunContextMenuItem(..) => PermissionType::RunActionsAsUser,
        PluginCommand::WatchFilesystem => PermissionType::FullHdAccess,
        PluginCommand::UnblockCliPipeInput(..)
        | PluginCommand::BlockCliPipeInput(..)
        | PluginCommand::CliPipeOutput(..)
        | PluginCommand::SetCliPipeExitCode(..) => PermissionType::ReadCliPipes,
        PluginCommand::MessageToPlugin(..) => PermissionType::MessageAndLaunchOtherPlugins,
        PluginCommand::ListClients
        | PluginCommand::DumpSessionLayout { .. }
        | PluginCommand::GetPanePid { .. }
        | PluginCommand::GetPaneCwd { .. }
        | PluginCommand::GenerateRandomName
        | PluginCommand::GetLayoutDir
        | PluginCommand::GetFocusedPaneInfo
        | PluginCommand::DumpLayout(..)
        | PluginCommand::ParseLayout(..)
        | PluginCommand::SaveSession
        | PluginCommand::CurrentSessionLastSavedTime
        | PluginCommand::GetPaneInfo(..)
        | PluginCommand::GetTabInfo(..)
        | PluginCommand::GetNestedSessionKeybinds(..)
        | PluginCommand::GetSessionList
        | PluginCommand::GetSessionSuggestions { .. } => PermissionType::ReadApplicationState,
        PluginCommand::GetPaneRunningCommand { .. }
        | PluginCommand::GetSessionPreview { .. }
        | PluginCommand::GetSavedSessionPreview(..) => PermissionType::ReadPaneContents,
        PluginCommand::RebindKeys { .. }
        | PluginCommand::Reconfigure(..)
        | PluginCommand::ReadConfig
        | PluginCommand::RevertConfig(..)
        | PluginCommand::UnsetConfigSetting(..)
        | PluginCommand::CopyKeybindPreset { .. }
        | PluginCommand::SaveKeybindsAsPreset { .. }
        | PluginCommand::WriteThemeFile { .. }
        | PluginCommand::DeleteThemeFile { .. }
        | PluginCommand::SaveConfig
        | PluginCommand::OverwriteConfigFile
        | PluginCommand::ReloadConfigFile
        | PluginCommand::ReplaceConfigBlocks(..)
        | PluginCommand::ResetKeys { .. }
        | PluginCommand::ResetMousebinds { .. } => PermissionType::Reconfigure,
        PluginCommand::ChangeHostFolder(..) | PluginCommand::ListWindowsVolumes => {
            PermissionType::FullHdAccess
        },
        PluginCommand::ShareCurrentSession
        | PluginCommand::StopSharingCurrentSession
        | PluginCommand::StopWebServer
        | PluginCommand::QueryWebServerStatus
        | PluginCommand::GenerateWebLoginToken(..)
        | PluginCommand::RevokeWebLoginToken(..)
        | PluginCommand::RevokeAllWebLoginTokens
        | PluginCommand::RenameWebLoginToken(..)
        | PluginCommand::ListWebLoginTokens
        | PluginCommand::StartWebServer => PermissionType::StartWebServer,
        PluginCommand::InterceptKeyPresses | PluginCommand::ClearKeyPressesIntercepts => {
            PermissionType::InterceptInput
        },
        PluginCommand::GetPaneScrollback { .. } => PermissionType::ReadPaneContents,
        PluginCommand::RunAction(..) => PermissionType::RunActionsAsUser,
        PluginCommand::GetSessionEnvironmentVariables => {
            PermissionType::ReadSessionEnvironmentVariables
        },
        PluginCommand::OpenCommandPaneInNewTab(..) => PermissionType::RunCommands,
        PluginCommand::OpenEditorPaneInNewTab(..) => PermissionType::OpenFiles,
        // Closed set that stays ungated. A new PluginCommand must be named on a
        // permission arm above; this is not a wildcard and must not become one.
        PluginCommand::Subscribe(..)
        | PluginCommand::Unsubscribe(..)
        | PluginCommand::SetSelectable(..)
        | PluginCommand::ShowCursor(..)
        | PluginCommand::GetPluginIds
        | PluginCommand::GetZellijVersion
        | PluginCommand::SetTimeout(..)
        | PluginCommand::PostMessageTo(..)
        | PluginCommand::PostMessageToPlugin(..)
        | PluginCommand::HideSelf
        | PluginCommand::ShowSelf(..)
        | PluginCommand::ReportPanic(..)
        | PluginCommand::RequestPluginPermissions(..)
        | PluginCommand::ScanHostFolder(..)
        | PluginCommand::CloseSelf
        | PluginCommand::SetSelfMouseSelectionSupport(..)
        | PluginCommand::SetSelfCollapsed(..)
        | PluginCommand::SetSelectableSlot(..)
        | PluginCommand::HideSlot(..)
        | PluginCommand::ShowSlot(..)
        | PluginCommand::CloseSlot(..)
        | PluginCommand::SetCollapsedSlot(..) => return None,
    };
    Some(permission)
}

#[cfg(test)]
mod prompt_tests {
    use super::*;
    use std::path::PathBuf;
    use zellij_utils::data::Event;
    use zellij_utils::data::PluginTag;

    fn caller() -> PromptCaller {
        PromptCaller {
            plugin_id: 3,
            client_id: 2,
            request_id: 9,
        }
    }

    #[test]
    fn a_valid_request_opens_a_popup_for_the_acting_user_over_the_caller() {
        let request = PromptRequest::confirm("Go?");
        match prompt_instruction(caller(), 5, 4, request.clone()) {
            PluginInstruction::PromptRequest {
                caller: instruction_caller,
                owner_client_id,
                caller_pane_id,
                request: instruction_request,
            } => {
                assert_eq!(instruction_caller, caller());
                assert_eq!(owner_client_id, 5);
                assert_eq!(caller_pane_id, Some(PaneId::Plugin(4)));
                assert_eq!(instruction_request, request);
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn an_invalid_request_answers_the_caller_with_an_error_and_opens_nothing() {
        let request = PromptRequest::number("Port").min(5).max(1);
        match prompt_instruction(caller(), 5, 4, request) {
            PluginInstruction::Update(updates) => {
                assert_eq!(updates.len(), 1);
                let (plugin_id, client_id, event) = &updates[0];
                assert_eq!((*plugin_id, *client_id), (Some(3), Some(2)));
                assert!(matches!(
                    event,
                    Event::PromptResult(9, PromptResult::Error(message)) if message.contains("min")
                ));
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn only_built_in_plugins_may_change_the_desktop_integration() {
        assert!(may_change_desktop_integration(&RunPluginLocation::Zellij(
            PluginTag::new("configuration")
        )));
        assert!(!may_change_desktop_integration(&RunPluginLocation::File(
            PathBuf::from("/tmp/configuration.wasm")
        )));
        assert!(!may_change_desktop_integration(&RunPluginLocation::Remote(
            "https://example.com/configuration.wasm".to_owned()
        )));
    }

    #[test]
    fn prompting_needs_permission_to_open_plugins_and_only_the_prompt_plugin_may_reply() {
        let request = PluginCommand::Prompt {
            request_id: 1,
            request: PromptRequest::confirm("x"),
        };
        assert_eq!(
            required_permission(&request),
            Some(PermissionType::OpenTerminalsOrPlugins)
        );
        let reply = PluginCommand::ReplyToPrompt {
            request_id: 1,
            result: PromptResult::Cancelled,
        };
        assert_eq!(
            required_permission(&reply),
            Some(PermissionType::OpenTerminalsOrPlugins)
        );
        assert!(may_reply_to_prompt(&RunPluginLocation::Zellij(
            PluginTag::new("prompt")
        )));
        assert!(!may_reply_to_prompt(&RunPluginLocation::Zellij(
            PluginTag::new("configuration")
        )));
        assert!(!may_reply_to_prompt(&RunPluginLocation::File(
            PathBuf::from("/tmp/prompt.wasm")
        )));
    }
}

fn set_live_view(env: &PluginEnv, view: Option<zellij_utils::data::LiveView>) {
    let _ = env.senders.send_to_screen(ScreenInstruction::SetLiveView {
        plugin_pane_id: self_pane_id(env),
        client_id: acting_client(env),
        view,
    });
}

fn set_live_view_input(env: &PluginEnv, capture: bool) {
    let _ = env
        .senders
        .send_to_screen(ScreenInstruction::SetLiveViewInput {
            plugin_pane_id: self_pane_id(env),
            client_id: acting_client(env),
            capture,
        });
}

#[cfg(test)]
mod focus_and_dead_session_tests {
    use super::*;
    use std::time::SystemTime;
    use zellij_utils::consts::{
        session_deletion_tombstone_path, session_incarnation_path, session_info_folder_for_session,
        session_publication_lock_path,
    };
    use zellij_utils::sessions::{
        clear_session_deletion_fence, ensure_session_publication_allowed,
        is_session_publication_fenced, read_session_incarnation, SessionCacheRemoval,
    };

    fn unique_session_id(label: &str) -> String {
        format!(
            "{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    fn remove_sidecars(session_id: &str) {
        let _ = std::fs::remove_file(session_deletion_tombstone_path(session_id));
        let _ = std::fs::remove_file(session_incarnation_path(session_id));
        let _ = std::fs::remove_file(session_publication_lock_path(session_id));
        let _ = std::fs::remove_dir_all(session_info_folder_for_session(session_id));
        let _ = std::fs::remove_dir_all(session_deletion_tombstone_path(session_id));
    }

    #[test]
    fn focus_previous_and_focus_last_pane_require_change_application_state() {
        assert_eq!(
            required_permission(&PluginCommand::FocusPreviousPane),
            Some(PermissionType::ChangeApplicationState)
        );
        assert_eq!(
            required_permission(&PluginCommand::FocusLastPane),
            Some(PermissionType::ChangeApplicationState)
        );
        assert_eq!(
            required_permission(&PluginCommand::FocusNextPane),
            Some(PermissionType::ChangeApplicationState)
        );
    }

    #[test]
    fn delete_dead_session_cache_fence_refusal_does_not_drop_the_index_row() {
        let mut deleted_index = None;
        let mut removed_id = None;
        let error = delete_dead_session_cache_with(
            "plain-session",
            |session_id| {
                removed_id = Some(session_id.to_owned());
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "fence refused",
                ))
            },
            |display_name| deleted_index = Some(display_name.to_owned()),
        )
        .expect_err("a fence error must fail the deletion");
        assert_eq!(removed_id.as_deref(), Some("plain-session"));
        assert!(deleted_index.is_none());
        let message = format!("{error:#}");
        assert!(
            message.contains("fence refused"),
            "reply must carry the fence error, got {message}"
        );

        let mut deleted_index = false;
        delete_dead_session_cache_with(
            "../not-a-folder",
            |_| {
                panic!("an ineligible name must not reach the fence");
            },
            |_| deleted_index = true,
        )
        .expect_err("a path-like session id is not eligible");
        assert!(!deleted_index);

        let mut deleted_index = None;
        delete_dead_session_cache_with(
            "missing-cache",
            |_| Ok(SessionCacheRemoval::NotFound),
            |display_name| deleted_index = Some(display_name.to_owned()),
        )
        .expect_err("a missing cache is not a successful removal");
        assert!(deleted_index.is_none());
    }

    #[test]
    fn delete_dead_session_cache_removes_the_index_only_after_the_fence() {
        let mut deleted_index = None;
        delete_dead_session_cache_with(
            "removed-cache",
            |session_id| {
                assert_eq!(session_id, "removed-cache");
                Ok(SessionCacheRemoval::Removed)
            },
            |display_name| deleted_index = Some(display_name.to_owned()),
        )
        .expect("Removed is success");
        assert_eq!(deleted_index.as_deref(), Some("removed-cache"));
    }

    #[test]
    fn delete_dead_session_reply_is_ok_only_when_the_fenced_cache_is_removed() {
        let session_id = unique_session_id("dead-reply");
        remove_sidecars(&session_id);
        clear_session_deletion_fence(&session_id);
        let incarnation = read_session_incarnation(&session_id).unwrap();
        let folder = session_info_folder_for_session(&session_id);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-metadata.kdl"), "session").unwrap();

        assert_eq!(
            delete_dead_session_reply(&session_id),
            DeleteDeadSessionResponse::Ok
        );
        assert!(
            !folder.exists(),
            "successful reply must remove the cache directory"
        );
        let tombstone = std::fs::read_to_string(session_deletion_tombstone_path(&session_id))
            .expect("successful reply must publish the deletion tombstone");
        assert!(
            tombstone.contains(&format!("incarnation={incarnation}")),
            "tombstone must name the live incarnation, got {tombstone}"
        );
        let fenced = ensure_session_publication_allowed(&session_id, incarnation).unwrap_err();
        assert!(is_session_publication_fenced(&fenced));
        remove_sidecars(&session_id);

        let blocked = unique_session_id("dead-reply-blocked");
        remove_sidecars(&blocked);
        let folder = session_info_folder_for_session(&blocked);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-metadata.kdl"), "keep").unwrap();
        std::fs::create_dir(session_deletion_tombstone_path(&blocked)).unwrap();
        match delete_dead_session_reply(&blocked) {
            DeleteDeadSessionResponse::Err(error) => {
                assert!(
                    error.contains("Failed to delete dead session"),
                    "fence failure must be the reply error, got {error}"
                );
            },
            DeleteDeadSessionResponse::Ok => panic!("fence failure must not reply Ok"),
        }
        assert_eq!(
            std::fs::read_to_string(folder.join("session-metadata.kdl")).unwrap(),
            "keep"
        );
        assert!(folder.exists());
        remove_sidecars(&blocked);
    }

    #[test]
    fn live_session_is_not_removed_by_the_single_session_delete() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static LIVE_IDS: AtomicU32 = AtomicU32::new(0);

        let session_id = format!(
            "dl{:x}{:x}",
            std::process::id(),
            LIVE_IDS.fetch_add(1, Ordering::Relaxed)
        );
        let sock_path = zellij_utils::consts::ZELLIJ_SOCK_DIR.join(&session_id);
        assert!(
            sock_path.as_os_str().len() < zellij_utils::consts::ZELLIJ_SOCK_MAX_LENGTH,
            "socket path exceeds the platform limit: {}",
            sock_path.display()
        );
        remove_sidecars(&session_id);
        let _ = std::fs::remove_file(&sock_path);
        std::fs::create_dir_all(&*zellij_utils::consts::ZELLIJ_SOCK_DIR).unwrap();
        std::fs::create_dir_all(&*zellij_utils::consts::ZELLIJ_SESSION_INFO_CACHE_DIR).unwrap();

        struct Cleanup {
            session_id: String,
            sock_path: std::path::PathBuf,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_file(&self.sock_path);
                remove_sidecars(&self.session_id);
                if let Ok(index) = zellij_utils::session_index::SessionIndex::open_default() {
                    let _ = index.delete(&self.session_id);
                }
            }
        }
        let _cleanup = Cleanup {
            session_id: session_id.clone(),
            sock_path: sock_path.clone(),
        };

        let folder = session_info_folder_for_session(&session_id);
        std::fs::create_dir_all(&folder).unwrap();
        let marker = folder.join("session-metadata.kdl");
        std::fs::write(&marker, "live-cache").unwrap();
        #[cfg(unix)]
        let _listener = std::os::unix::net::UnixListener::bind(&sock_path)
            .unwrap_or_else(|error| panic!("bind {}: {error}", sock_path.display()));
        #[cfg(not(unix))]
        std::fs::write(&sock_path, b"").unwrap();

        let index =
            zellij_utils::session_index::SessionIndex::open_default().expect("session index");
        index.ensure_row(&session_id, 1).unwrap();

        match delete_dead_session_reply(&session_id) {
            DeleteDeadSessionResponse::Err(error) => {
                assert!(
                    error.contains("not definitely dead"),
                    "a live session must be refused, got {error}"
                );
            },
            DeleteDeadSessionResponse::Ok => panic!("a live session must not be deleted"),
        }
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "live-cache");
        assert!(folder.exists());
        assert!(
            index.get(&session_id).unwrap().is_some(),
            "the index row of a live session must stay"
        );
        assert!(
            !session_deletion_tombstone_path(&session_id).exists(),
            "a live session must not be tombstoned"
        );
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use zellij_utils::channels::{self, Receiver, SenderWithContext};
    use zellij_utils::data::{Event, InputMode, PermissionType};
    use zellij_utils::errors::ErrorContext;
    use zellij_utils::input::layout::PluginUserConfiguration;
    use zellij_utils::input::plugins::PluginConfig;

    fn plugin_env(
        plugin_id: PluginId,
        plugin_cwd: PathBuf,
        to_plugin: SenderWithContext<PluginInstruction>,
    ) -> PluginEnv {
        let stdin_pipe = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let stdout_pipe = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let wasi_ctx = crate::plugins::plugin_loader::PluginLoader::create_wasi_ctx(
            &plugin_cwd,
            &plugin_cwd,
            &plugin_cwd,
            &plugin_cwd,
            &"file:/tmp/scan.wasm".to_owned(),
            plugin_id,
            false,
            stdin_pipe.clone(),
            stdout_pipe.clone(),
        )
        .unwrap();
        PluginEnv {
            plugin_id,
            generation: 9,
            session_scope: 0,
            load_alive: Arc::new(AtomicBool::new(true)),
            client_id: 3,
            plugin: PluginConfig {
                path: PathBuf::from("/tmp/scan.wasm"),
                _allow_exec_host_cmd: false,
                location: RunPluginLocation::File(PathBuf::from("/tmp/scan.wasm")),
                initial_userspace_configuration: PluginUserConfiguration::new(BTreeMap::new()),
                initial_cwd: None,
            },
            plugin_digest: "scan-digest".to_owned(),
            permissions: Arc::new(Mutex::new(Some(
                [PermissionType::ReadApplicationState].into_iter().collect(),
            ))),
            quarantined: Arc::new(AtomicBool::new(false)),
            senders: crate::thread_bus::ThreadSenders {
                to_plugin: Some(to_plugin),
                should_silently_fail: true,
                ..Default::default()
            },
            wasi_ctx,
            plugin_own_data_dir: plugin_cwd.clone(),
            plugin_own_cache_dir: plugin_cwd.clone(),
            path_to_default_shell: plugin_cwd.clone(),
            default_shell: None,
            layout_dir: None,
            plugin_cwd,
            session_env_vars: BTreeMap::new(),
            input_pipes_to_unblock: Arc::new(Mutex::new(HashSet::new())),
            input_pipes_to_block: Arc::new(Mutex::new(HashSet::new())),
            default_mode: InputMode::Normal,
            subscriptions: Arc::new(Mutex::new(HashSet::new())),
            stdin_pipe,
            stdout_pipe,
            keybinds: Arc::new(Default::default()),
            intercepting_key_presses: false,
            pending_popup_size: None,
            store_limits: wasmi::StoreLimitsBuilder::new().build(),
            shared: None,
        }
    }

    fn timers_in_flight() -> usize {
        PLUGIN_TIMERS.lock().unwrap().total
    }

    fn timers_for(plugin_id: PluginId) -> usize {
        PLUGIN_TIMERS
            .lock()
            .unwrap()
            .per_plugin
            .get(&plugin_id)
            .copied()
            .unwrap_or(0)
    }

    /// Timer tests share the process-wide permit counter.
    fn lock_timer_tests() -> std::sync::MutexGuard<'static, ()> {
        static TIMER_TESTS: Mutex<()> = Mutex::new(());
        TIMER_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn scans_in_flight() -> usize {
        DIRECTORY_SCANS.lock().unwrap().total
    }

    fn recv_until(
        receiver: &Receiver<(PluginInstruction, ErrorContext)>,
        timeout: Duration,
        mut matches: impl FnMut(&PluginInstruction) -> bool,
    ) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok((instruction, _)) if matches(&instruction) => return true,
                Ok(_) => {},
                Err(_) => {},
            }
        }
        false
    }

    #[test]
    fn plugin_timeout_past_the_cap_does_not_start_another_one() {
        let _timer_tests = lock_timer_tests();
        assert_eq!(MAX_PLUGIN_TIMERS_PER_PLUGIN, 64);
        assert_eq!(MAX_PLUGIN_TIMERS_IN_FLIGHT, 256);

        let (to_plugin, receiver) = channels::unbounded();
        let dir = std::env::temp_dir();
        let env = plugin_env(7, dir.clone(), SenderWithContext::new(to_plugin));

        let holds: Vec<_> = (0..MAX_PLUGIN_TIMERS_PER_PLUGIN)
            .map(|_| try_acquire_plugin_timer(7).expect("per-plugin timer slot"))
            .collect();
        assert!(try_acquire_plugin_timer(7).is_none());
        let in_flight = timers_in_flight();
        set_timeout(&env, 0.0);
        assert_eq!(timers_in_flight(), in_flight);
        assert!(
            !recv_until(&receiver, Duration::from_secs(1), |instruction| {
                matches!(instruction, PluginInstruction::ApplyTimer { .. })
            }),
            "a timer past the per-plugin cap must not spawn"
        );
        drop(holds);
        assert_eq!(timers_in_flight(), 0);

        let (to_other, other_receiver) = channels::unbounded();
        let other = plugin_env(8, dir.clone(), SenderWithContext::new(to_other));
        let held_again: Vec<_> = (0..MAX_PLUGIN_TIMERS_PER_PLUGIN)
            .map(|_| try_acquire_plugin_timer(7).unwrap())
            .collect();
        set_timeout(&other, 0.0);
        assert!(
            recv_until(&other_receiver, Duration::from_secs(2), |instruction| {
                matches!(
                    instruction,
                    PluginInstruction::ApplyTimer {
                        plugin_id: 8,
                        generation: 9,
                        ..
                    }
                )
            }),
            "another plugin under the per-plugin cap must still start a timer"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while timers_in_flight() != held_again.len() {
            assert!(Instant::now() < deadline, "timer permit was not released");
            thread::sleep(Duration::from_millis(5));
        }
        drop(held_again);

        let global: Vec<_> = (1..=MAX_PLUGIN_TIMERS_IN_FLIGHT)
            .map(|plugin_id| {
                try_acquire_plugin_timer(plugin_id as PluginId).expect("process-wide timer slot")
            })
            .collect();
        let (to_blocked, blocked_receiver) = channels::unbounded();
        let blocked = plugin_env(1000, dir, SenderWithContext::new(to_blocked));
        let in_flight = timers_in_flight();
        set_timeout(&blocked, 0.0);
        assert_eq!(timers_in_flight(), in_flight);
        assert!(
            !recv_until(
                &blocked_receiver,
                Duration::from_secs(1),
                |instruction| matches!(instruction, PluginInstruction::ApplyTimer { .. })
            ),
            "a timer past the process-wide cap must not spawn"
        );
        drop(global);
        assert_eq!(timers_in_flight(), 0);

        set_timeout(&blocked, 0.0);
        set_timeout(&blocked, 0.0);
        let mut fired = 0;
        let deadline = Instant::now() + Duration::from_secs(2);
        while fired < 2 && Instant::now() < deadline {
            if recv_until(
                &blocked_receiver,
                Duration::from_millis(100),
                |instruction| {
                    matches!(
                        instruction,
                        PluginInstruction::ApplyTimer {
                            plugin_id: 1000,
                            ..
                        }
                    )
                },
            ) {
                fired += 1;
            }
        }
        assert_eq!(fired, 2, "timers of the same duration must not be deduped");
    }

    #[test]
    fn a_dead_load_releases_its_timer_permit_without_waiting() {
        let _timer_tests = lock_timer_tests();
        let (to_dead, dead_receiver) = channels::unbounded();
        let dead = plugin_env(90210, std::env::temp_dir(), SenderWithContext::new(to_dead));
        let dead_alive = dead.load_alive.clone();
        let (to_live, live_receiver) = channels::unbounded();
        let live = plugin_env(90211, std::env::temp_dir(), SenderWithContext::new(to_live));
        set_timeout(&dead, 30.0);
        set_timeout(&live, 0.05);
        let deadline = Instant::now() + Duration::from_secs(2);
        while timers_for(90210) != 1 || timers_for(90211) != 1 {
            assert!(Instant::now() < deadline, "timer permits were not acquired");
            thread::sleep(Duration::from_millis(5));
        }
        dead_alive.store(false, std::sync::atomic::Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(2);
        while timers_for(90210) != 0 {
            assert!(
                Instant::now() < deadline,
                "unloaded plugin kept its timer permit"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !recv_until(&dead_receiver, Duration::from_millis(200), |instruction| {
                matches!(
                    instruction,
                    PluginInstruction::ApplyTimer {
                        plugin_id: 90210,
                        ..
                    }
                )
            }),
            "a dead load must not receive ApplyTimer"
        );
        assert!(
            recv_until(&live_receiver, Duration::from_secs(2), |instruction| {
                matches!(
                    instruction,
                    PluginInstruction::ApplyTimer {
                        plugin_id: 90211,
                        generation: 9,
                        ..
                    }
                )
            }),
            "a live load must still receive its timer"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while timers_for(90211) != 0 {
            assert!(
                Instant::now() < deadline,
                "live timer permit was not released"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn lock_scan_tests() -> std::sync::MutexGuard<'static, ()> {
        static SCAN_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        SCAN_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn scan_host_folder_past_the_cap_does_not_start_another_one() {
        let _scan_tests = lock_scan_tests();
        assert_eq!(MAX_DIRECTORY_SCANS_IN_FLIGHT, 4);
        assert_eq!(MAX_DIRECTORY_SCANS_PER_PLUGIN, 1);
        assert_eq!(MAX_DIRECTORY_SCAN_ENTRIES, 4096);

        let host = tempfile::tempdir().unwrap();
        std::fs::write(host.path().join("one"), b"1").unwrap();
        let (to_plugin, receiver) = channels::unbounded();
        let env = plugin_env(
            4,
            host.path().to_path_buf(),
            SenderWithContext::new(to_plugin),
        );

        let own_slot = try_acquire_directory_scan(4).unwrap();
        scan_host_folder(&env, PathBuf::from("/host"));
        assert_eq!(scans_in_flight(), 1);
        assert!(
            !recv_until(&receiver, Duration::from_secs(1), |instruction| {
                matches!(instruction, PluginInstruction::Update(_))
            }),
            "a second scan for the same plugin must not spawn"
        );
        drop(own_slot);

        let global: Vec<_> = (11..11 + MAX_DIRECTORY_SCANS_IN_FLIGHT as u32)
            .map(|plugin_id| try_acquire_directory_scan(plugin_id).unwrap())
            .collect();
        scan_host_folder(&env, PathBuf::from("/host"));
        assert_eq!(scans_in_flight(), MAX_DIRECTORY_SCANS_IN_FLIGHT);
        assert!(
            !recv_until(&receiver, Duration::from_secs(1), |instruction| {
                matches!(instruction, PluginInstruction::Update(_))
            }),
            "a scan past the process-wide cap must not spawn"
        );
        drop(global);
        let deadline = Instant::now() + Duration::from_secs(2);
        while scans_in_flight() != 0 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }

        scan_host_folder(&env, PathBuf::from("/host"));
        let (listed, incomplete) = wait_for_scan(&receiver);
        assert_eq!(
            listed.len(),
            1,
            "an admitted scan must still report its entries"
        );
        assert!(!incomplete);
        wait_until_scans_idle();

        for index in 0..MAX_DIRECTORY_SCAN_ENTRIES {
            std::fs::write(host.path().join(format!("extra-{index}")), b"x").unwrap();
        }
        assert_eq!(
            std::fs::read_dir(host.path()).unwrap().count(),
            MAX_DIRECTORY_SCAN_ENTRIES + 1
        );
        scan_host_folder(&env, PathBuf::from("/host"));
        let (listed, incomplete) = wait_for_scan(&receiver);
        // `extra-*` names do not fit 4096-wide in the host pipe, so the entry
        // cap is not what truncates this listing. The result is still a
        // read-order prefix that fits, and one more name does not.
        assert!(
            !listed.is_empty() && listed.len() < MAX_DIRECTORY_SCAN_ENTRIES,
            "expected a pipe-sized prefix, got {}",
            listed.len()
        );
        assert!(incomplete, "a truncated listing must be marked incomplete");
        assert!(
            stdin_frame_len(&Event::FileSystemUpdate(listed.clone()))
                <= crate::plugins::WASI_HOST_PIPE_CAP
        );
        let mut observed = Vec::new();
        for entry in std::fs::read_dir(host.path()).unwrap() {
            let entry = entry.unwrap();
            observed.push(PathBuf::from("/host").join(entry.file_name()));
        }
        let listed_paths: Vec<_> = listed.iter().map(|(path, _)| path.clone()).collect();
        assert_eq!(&observed[..listed_paths.len()], listed_paths.as_slice());
        let next = &observed[listed.len()];
        let metadata = std::fs::symlink_metadata(host.path().join(next.file_name().unwrap()))
            .unwrap()
            .into();
        let mut one_more = listed;
        one_more.push((next.clone(), Some(metadata)));
        assert!(
            stdin_frame_len(&Event::FileSystemUpdate(one_more))
                > crate::plugins::WASI_HOST_PIPE_CAP
        );
        wait_until_scans_idle();
    }

    fn wait_until_scans_idle() {
        let deadline = Instant::now() + Duration::from_secs(2);
        while scans_in_flight() != 0 {
            assert!(Instant::now() < deadline, "directory scan permit leaked");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_scan(
        receiver: &Receiver<(PluginInstruction, ErrorContext)>,
    ) -> (
        Vec<(PathBuf, Option<zellij_utils::data::FileMetadata>)>,
        bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok((PluginInstruction::Update(updates), _)) => {
                    for (_, _, event) in updates {
                        match event {
                            Event::FileSystemUpdate(paths) => return (paths, false),
                            Event::FileSystemUpdateIncomplete(paths) => return (paths, true),
                            _ => {},
                        }
                    }
                },
                Ok(_) => {},
                Err(_) => {},
            }
        }
        panic!("admitted directory scan did not send FileSystemUpdate");
    }

    fn stdin_frame_len(event: &Event) -> usize {
        use prost::Message;
        let protobuf =
            zellij_utils::plugin_api::event::event_to_protobuf_with_keybinds(event.clone(), None)
                .expect("event protobuf");
        let json = serde_json::to_string(&protobuf.encode_to_vec()).expect("event json");
        json.len() + 2
    }

    #[test]
    fn scan_host_folder_long_names_stay_within_the_host_pipe() {
        let _scan_tests = lock_scan_tests();
        let host = tempfile::tempdir().unwrap();
        let name_len = 180usize;
        for index in 0..MAX_DIRECTORY_SCAN_ENTRIES {
            let name = format!("{index:04}{}", "n".repeat(name_len - 4));
            assert_eq!(name.len(), name_len);
            std::fs::write(host.path().join(name), b"x").unwrap();
        }
        assert_eq!(
            std::fs::read_dir(host.path()).unwrap().count(),
            MAX_DIRECTORY_SCAN_ENTRIES
        );

        let (to_plugin, receiver) = channels::unbounded();
        let env = plugin_env(
            44,
            host.path().to_path_buf(),
            SenderWithContext::new(to_plugin),
        );
        scan_host_folder(&env, PathBuf::from("/host"));
        let (listed, incomplete) = wait_for_scan(&receiver);
        wait_until_scans_idle();

        assert!(
            !listed.is_empty(),
            "a directory that has entries must still deliver a prefix"
        );
        assert!(
            listed.len() < MAX_DIRECTORY_SCAN_ENTRIES,
            "4096 long names must not fit in the host pipe unchanged"
        );
        assert!(incomplete, "a pipe-sized prefix must be marked incomplete");

        let mut observed = Vec::new();
        for entry in std::fs::read_dir(host.path()).unwrap() {
            let entry = entry.unwrap();
            observed.push(PathBuf::from("/host").join(entry.file_name()));
        }
        let listed_paths: Vec<_> = listed.iter().map(|(path, _)| path.clone()).collect();
        assert_eq!(
            &observed[..listed_paths.len()],
            listed_paths.as_slice(),
            "the delivered entries must be a read-order prefix"
        );

        let event = Event::FileSystemUpdate(listed.clone());
        let frame = stdin_frame_len(&event);
        assert!(
            frame <= super::super::plugin_loader::WASI_HOST_PIPE_CAP,
            "stdin frame {frame} exceeds the host pipe"
        );
        let protobuf =
            zellij_utils::plugin_api::event::event_to_protobuf_with_keybinds(event.clone(), None)
                .unwrap();
        let encoded = prost::Message::encode_to_vec(&protobuf);
        wasi_write_object(&env, &encoded).expect("fitted FileSystemUpdate must be accepted");
        assert_eq!(
            env.stdin_pipe.lock().unwrap().len(),
            frame,
            "wasi_write_object must write the measured stdin frame"
        );

        let next = &observed[listed.len()];
        let metadata = std::fs::symlink_metadata(host.path().join(next.file_name().unwrap()))
            .unwrap()
            .into();
        let mut one_more = listed;
        one_more.push((next.clone(), Some(metadata)));
        let extended = stdin_frame_len(&Event::FileSystemUpdate(one_more));
        assert!(
            extended > super::super::plugin_loader::WASI_HOST_PIPE_CAP,
            "one more omitted name still fit ({extended} bytes)"
        );
    }

    #[test]
    fn exec_cmd_empty_command_line_does_not_panic_and_denied_host_command_does_not_spawn() {
        let (to_plugin, _receiver) = channels::unbounded();
        let dir = tempfile::tempdir().unwrap();
        let mut env = plugin_env(
            21,
            dir.path().to_path_buf(),
            SenderWithContext::new(to_plugin),
        );

        // RunCommands is what lets a plugin reach this host function. The
        // separate `_allow_exec_host_cmd` flag is what must still refuse a spawn.
        env.permissions
            .lock()
            .unwrap()
            .replace([PermissionType::RunCommands].into_iter().collect());
        env.plugin._allow_exec_host_cmd = true;
        exec_cmd(&env, Vec::new());

        env.plugin._allow_exec_host_cmd = false;
        exec_cmd(&env, Vec::new());

        let marker = dir.path().join("should-not-exist");
        #[cfg(unix)]
        let command = {
            let touch = ["/usr/bin/touch", "/bin/touch"]
                .into_iter()
                .find(|path| std::path::Path::new(path).is_file())
                .expect("touch is required to prove a denied exec_cmd does not spawn");
            vec![touch.to_owned(), marker.to_string_lossy().into_owned()]
        };
        #[cfg(windows)]
        let command = vec![
            "cmd".to_owned(),
            "/C".to_owned(),
            format!("type nul > \"{}\"", marker.display()),
        ];
        exec_cmd(&env, command);
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline && !marker.exists() {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !marker.exists(),
            "exec_cmd without _allow_exec_host_cmd must not spawn"
        );
    }

    fn lock_run_action_tests() -> std::sync::MutexGuard<'static, ()> {
        static RUN_ACTION_TESTS: Mutex<()> = Mutex::new(());
        RUN_ACTION_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn run_actions_in_flight() -> usize {
        RUN_ACTIONS.lock().unwrap().total
    }

    fn run_actions_for(plugin_id: PluginId) -> usize {
        RUN_ACTIONS
            .lock()
            .unwrap()
            .per_plugin
            .get(&plugin_id)
            .copied()
            .unwrap_or(0)
    }

    fn switch_mode() -> Action {
        Action::SwitchToMode {
            input_mode: InputMode::Locked,
        }
    }

    fn saw_action_complete(instruction: &PluginInstruction) -> bool {
        match instruction {
            PluginInstruction::Update(updates) => updates
                .iter()
                .any(|(_, _, event)| matches!(event, Event::ActionComplete(..))),
            PluginInstruction::ApplyLoadResult {
                generation: 9,
                event: Event::ActionComplete(_, _, context),
                ..
            } => !context.keys().any(|key| key == "generation"),
            _ => false,
        }
    }

    #[test]
    fn run_action_past_the_cap_does_not_spawn_and_is_rejected() {
        let _lock = lock_run_action_tests();
        assert_eq!(MAX_RUN_ACTIONS_PER_PLUGIN, 4);
        assert_eq!(MAX_RUN_ACTIONS_IN_FLIGHT, 16);
        assert_eq!(run_actions_in_flight(), 0);

        let (to_plugin, receiver) = channels::unbounded();
        let env = plugin_env(
            7701,
            std::env::temp_dir(),
            SenderWithContext::new(to_plugin),
        );
        let holds: Vec<_> = (0..MAX_RUN_ACTIONS_PER_PLUGIN)
            .map(|_| try_acquire_run_action(7701).expect("per-plugin run-action slot"))
            .collect();
        let in_flight = run_actions_in_flight();
        let rejected = run_action(&env, switch_mode(), BTreeMap::new());
        assert!(
            rejected.is_err(),
            "a run action past the per-plugin cap must fail"
        );
        assert_eq!(run_actions_in_flight(), in_flight);
        assert!(
            !recv_until(&receiver, Duration::from_millis(400), saw_action_complete),
            "a rejected run action must not spawn or report success"
        );
        drop(holds);
        assert_eq!(run_actions_in_flight(), 0);

        let global: Vec<_> = (1..=MAX_RUN_ACTIONS_IN_FLIGHT)
            .map(|plugin_id| {
                try_acquire_run_action(plugin_id as PluginId).expect("process-wide run-action slot")
            })
            .collect();
        let in_flight = run_actions_in_flight();
        let rejected = run_action(&env, switch_mode(), BTreeMap::new());
        assert!(
            rejected.is_err(),
            "a run action past the process-wide cap must fail"
        );
        assert_eq!(run_actions_in_flight(), in_flight);
        assert!(
            !recv_until(&receiver, Duration::from_millis(400), saw_action_complete),
            "a rejected run action must not spawn or report success"
        );
        drop(global);
        assert_eq!(run_actions_in_flight(), 0);
    }

    #[test]
    fn admitted_run_action_releases_its_permit_when_it_finishes() {
        let _lock = lock_run_action_tests();
        assert_eq!(run_actions_in_flight(), 0);
        let (to_plugin, receiver) = channels::unbounded();
        let env = plugin_env(
            7702,
            std::env::temp_dir(),
            SenderWithContext::new(to_plugin),
        );
        run_action(&env, switch_mode(), BTreeMap::new()).expect("first run action is admitted");
        assert!(
            recv_until(&receiver, Duration::from_secs(2), saw_action_complete),
            "an admitted run action must finish and report completion"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while run_actions_for(7702) != 0 {
            assert!(
                Instant::now() < deadline,
                "run-action permit was not released"
            );
            thread::sleep(Duration::from_millis(5));
        }

        let holds: Vec<_> = (0..MAX_RUN_ACTIONS_PER_PLUGIN - 1)
            .map(|_| try_acquire_run_action(7702).expect("slot after release"))
            .collect();
        run_action(&env, switch_mode(), BTreeMap::new()).expect("released permit admits another");
        assert!(
            recv_until(&receiver, Duration::from_secs(2), saw_action_complete),
            "a later run action must run after the permit is released"
        );
        drop(holds);
        let deadline = Instant::now() + Duration::from_secs(2);
        while run_actions_for(7702) != 0 {
            assert!(
                Instant::now() < deadline,
                "the later run-action permit was not released"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn plugin_worker_post_past_the_admission_bound_is_not_enqueued() {
        use crate::plugins::wasm_bridge::{
            release_admitted_worker_posts, MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN,
            MAX_CACHED_WORKER_MESSAGE_BYTES_PER_PLUGIN,
        };
        assert_eq!(MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN, 256);
        assert_eq!(MAX_CACHED_WORKER_MESSAGE_BYTES_PER_PLUGIN, 1024 * 1024);

        let plugin_id = 900_001;
        let (to_plugin, receiver) = channels::unbounded();
        let env = plugin_env(
            plugin_id,
            std::env::temp_dir(),
            SenderWithContext::new(to_plugin),
        );
        let worker = "worker";
        let huge = "x".repeat(MAX_CACHED_WORKER_MESSAGE_BYTES_PER_PLUGIN + 1);
        post_message_to(&env, PluginMessage::new_to_worker(worker, "huge", &huge))
            .expect("a rejected post is not a send failure");
        env.senders
            .send_to_plugin(PluginInstruction::Quarantine(plugin_id, 9))
            .unwrap();

        let mut admitted = Vec::new();
        for index in 0..MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN {
            let name = format!("m{index}");
            post_message_to(&env, PluginMessage::new_to_worker(worker, &name, "p")).unwrap();
            admitted.push((name, "p".to_owned()));
        }
        struct ReleaseAdmitted {
            plugin_id: PluginId,
            worker: String,
            messages: Vec<(String, String)>,
        }
        impl Drop for ReleaseAdmitted {
            fn drop(&mut self) {
                release_admitted_worker_posts(self.plugin_id, &self.worker, &self.messages);
            }
        }
        let _release = ReleaseAdmitted {
            plugin_id,
            worker: worker.to_owned(),
            messages: admitted,
        };
        post_message_to(&env, PluginMessage::new_to_worker(worker, "overflow", "p"))
            .expect("a post past the count cap is rejected without failing the send");
        env.senders
            .send_to_plugin(PluginInstruction::Unload(plugin_id))
            .unwrap();

        let mut worker_posts = 0usize;
        let mut saw_huge = false;
        let mut saw_overflow = false;
        let mut saw_quarantine = false;
        let mut saw_unload = false;
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok((PluginInstruction::PostMessagesToPluginWorker(_, _, _, messages), _)) => {
                    worker_posts += 1;
                    for (name, _) in &messages {
                        if name == "huge" {
                            saw_huge = true;
                        }
                        if name == "overflow" {
                            saw_overflow = true;
                        }
                    }
                },
                Ok((PluginInstruction::Quarantine(id, generation), _))
                    if id == plugin_id && generation == 9 =>
                {
                    saw_quarantine = true;
                },
                Ok((PluginInstruction::Unload(id), _)) if id == plugin_id => {
                    saw_unload = true;
                },
                Ok(_) => {},
                Err(_) if saw_quarantine && saw_unload => break,
                Err(_) => {},
            }
        }
        assert!(
            !saw_huge,
            "a worker post over the byte cap must not be enqueued"
        );
        assert!(
            !saw_overflow,
            "a worker post past the count cap must not be enqueued"
        );
        assert_eq!(worker_posts, MAX_CACHED_WORKER_MESSAGES_PER_PLUGIN);
        assert!(
            saw_quarantine,
            "a retirement instruction must still be accepted"
        );
        assert!(saw_unload, "a control instruction must still be accepted");
    }

    fn lock_run_command_tests() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn lock_web_request_tests() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn run_commands_in_flight() -> usize {
        crate::plugins::background_operations::in_flight(
            crate::plugins::BackgroundOperationKind::RunCommand,
        )
    }

    fn web_requests_in_flight() -> usize {
        crate::plugins::background_operations::in_flight(
            crate::plugins::BackgroundOperationKind::WebRequest,
        )
    }

    fn command_context() -> BTreeMap<String, String> {
        let mut context = BTreeMap::new();
        context.insert("token".to_owned(), "kept".to_owned());
        context
    }

    struct JobChannels {
        env: PluginEnv,
        plugin_rx: Receiver<(PluginInstruction, ErrorContext)>,
        jobs_rx: Receiver<(BackgroundJob, ErrorContext)>,
    }

    fn job_channels(plugin_id: PluginId, generation: u64) -> JobChannels {
        let (to_plugin, plugin_rx) = channels::unbounded();
        let (to_jobs, jobs_rx) = channels::unbounded();
        let mut env = plugin_env(
            plugin_id,
            std::env::temp_dir(),
            SenderWithContext::new(to_plugin),
        );
        env.generation = generation;
        env.senders.to_background_jobs = Some(SenderWithContext::new(to_jobs));
        JobChannels {
            env,
            plugin_rx,
            jobs_rx,
        }
    }

    fn recv_load_result(
        receiver: &Receiver<(PluginInstruction, ErrorContext)>,
    ) -> PluginInstruction {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok((instruction, _))
                    if matches!(instruction, PluginInstruction::ApplyLoadResult { .. }) =>
                {
                    return instruction;
                },
                Ok(_) => {},
                Err(channels::RecvTimeoutError::Timeout) => {},
                Err(channels::RecvTimeoutError::Disconnected) => break,
            }
        }
        panic!("load result was not delivered");
    }

    #[test]
    fn run_command_past_the_cap_does_not_start_and_reports_the_failure() {
        let _lock = lock_run_command_tests();
        assert_eq!(MAX_RUN_COMMANDS_PER_PLUGIN, 4);
        assert_eq!(MAX_RUN_COMMANDS_IN_FLIGHT, 16);
        assert_eq!(run_commands_in_flight(), 0);

        let plugin_id = 8801;
        let jobs = job_channels(plugin_id, 3);
        let context = command_context();
        let holds: Vec<_> = (0..MAX_RUN_COMMANDS_PER_PLUGIN)
            .map(|_| {
                try_acquire_run_command(plugin_id, Arc::new(AtomicBool::new(true)), 0)
                    .expect("per-plugin command slot")
            })
            .collect();
        let in_flight = run_commands_in_flight();
        run_command(
            &jobs.env,
            vec!["echo".to_owned()],
            BTreeMap::new(),
            std::env::temp_dir(),
            context.clone(),
        );
        assert_eq!(run_commands_in_flight(), in_flight);
        assert!(
            jobs.jobs_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "a command past the per-plugin cap must not be enqueued"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                plugin_id: id,
                generation,
                event,
                ..
            } => {
                assert_eq!(id, plugin_id);
                assert_eq!(generation, 3);
                assert_eq!(
                    event,
                    crate::background_jobs::run_command_limit_event(context.clone())
                );
                assert!(!matches!(event, Event::ActionComplete(..)));
            },
            other => panic!("expected RunCommandResult, got {other:?}"),
        }
        drop(holds);
        assert_eq!(run_commands_in_flight(), 0);

        let global: Vec<_> = (1..=MAX_RUN_COMMANDS_IN_FLIGHT)
            .map(|id| {
                try_acquire_run_command(id as PluginId, Arc::new(AtomicBool::new(true)), 0)
                    .expect("process-wide command slot")
            })
            .collect();
        let in_flight = run_commands_in_flight();
        run_command(
            &jobs.env,
            vec!["echo".to_owned()],
            BTreeMap::new(),
            std::env::temp_dir(),
            context.clone(),
        );
        assert_eq!(run_commands_in_flight(), in_flight);
        assert!(
            jobs.jobs_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "a command past the process-wide cap must not be enqueued"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                generation, event, ..
            } => {
                assert_eq!(generation, 3);
                assert_eq!(
                    event,
                    crate::background_jobs::run_command_limit_event(context)
                );
            },
            other => panic!("expected RunCommandResult, got {other:?}"),
        }
        drop(global);
        assert_eq!(run_commands_in_flight(), 0);
    }

    #[test]
    fn a_finished_run_command_lets_another_one_start() {
        let _lock = lock_run_command_tests();
        assert_eq!(run_commands_in_flight(), 0);
        let plugin_id = 8802;
        let jobs = job_channels(plugin_id, 4);
        let context = command_context();
        run_command(
            &jobs.env,
            vec!["zellij-no-such-run-command".to_owned()],
            BTreeMap::new(),
            std::env::temp_dir(),
            context.clone(),
        );
        assert_eq!(run_commands_in_flight(), 1);
        let (job, _) = jobs
            .jobs_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("admitted command is enqueued");
        match job {
            BackgroundJob::RunCommand(
                id,
                client_id,
                command,
                args,
                env_variables,
                cwd,
                job_context,
                generation,
                reservation,
            ) => {
                assert_eq!(id, plugin_id);
                assert_eq!(generation, 4);
                assert_eq!(job_context, context);
                crate::global_async_runtime::get_tokio_runtime().block_on(
                    crate::background_jobs::perform_run_command(
                        id,
                        client_id,
                        command,
                        args,
                        env_variables,
                        cwd,
                        job_context,
                        generation,
                        jobs.env.senders.clone(),
                        reservation,
                    ),
                );
            },
            other => panic!("expected RunCommand, got {other:?}"),
        }
        assert_eq!(
            run_commands_in_flight(),
            1,
            "a command whose result is pending must still hold its slot"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                generation,
                event: Event::RunCommandResult(exit, _, _, event_context),
                reservation: result_reservation,
                ..
            } => {
                assert_eq!(generation, 4);
                assert_eq!(exit, Some(2));
                assert_eq!(event_context, context);
                drop(result_reservation);
            },
            other => panic!("expected RunCommandResult, got {other:?}"),
        }
        assert_eq!(
            run_commands_in_flight(),
            0,
            "dropping the result must release its slot"
        );

        run_command(
            &jobs.env,
            vec!["zellij-no-such-run-command".to_owned()],
            BTreeMap::new(),
            std::env::temp_dir(),
            context,
        );
        assert_eq!(
            run_commands_in_flight(),
            1,
            "a finished command must let another one start"
        );
        let (job, _) = jobs
            .jobs_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the next command is enqueued");
        let BackgroundJob::RunCommand(
            id,
            client_id,
            command,
            args,
            env_variables,
            cwd,
            job_context,
            generation,
            reservation,
        ) = job
        else {
            panic!("expected RunCommand");
        };
        crate::global_async_runtime::get_tokio_runtime().block_on(
            crate::background_jobs::perform_run_command(
                id,
                client_id,
                command,
                args,
                env_variables,
                cwd,
                job_context,
                generation,
                jobs.env.senders.clone(),
                reservation,
            ),
        );
        assert_eq!(
            run_commands_in_flight(),
            1,
            "the result still owns the command slot"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                reservation: result_reservation,
                ..
            } => drop(result_reservation),
            other => panic!("expected RunCommandResult, got {other:?}"),
        }
        assert_eq!(run_commands_in_flight(), 0);
    }

    #[test]
    fn web_request_past_the_cap_does_not_start_and_reports_the_failure() {
        let _lock = lock_web_request_tests();
        assert_eq!(MAX_WEB_REQUESTS_PER_PLUGIN, 4);
        assert_eq!(MAX_WEB_REQUESTS_IN_FLIGHT, 16);
        assert_eq!(web_requests_in_flight(), 0);

        let plugin_id = 8811;
        let jobs = job_channels(plugin_id, 5);
        let context = command_context();
        let holds: Vec<_> = (0..MAX_WEB_REQUESTS_PER_PLUGIN)
            .map(|_| {
                try_acquire_web_request(plugin_id, Arc::new(AtomicBool::new(true)), 0)
                    .expect("per-plugin web slot")
            })
            .collect();
        let in_flight = web_requests_in_flight();
        web_request(
            &jobs.env,
            "http://127.0.0.1/not-started".to_owned(),
            HttpVerb::Get,
            BTreeMap::new(),
            Vec::new(),
            context.clone(),
        );
        assert_eq!(web_requests_in_flight(), in_flight);
        assert!(
            jobs.jobs_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "a web request past the per-plugin cap must not be enqueued"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                plugin_id: id,
                generation,
                event,
                ..
            } => {
                assert_eq!(id, plugin_id);
                assert_eq!(generation, 5);
                assert_eq!(
                    event,
                    crate::background_jobs::web_request_limit_event(context.clone())
                );
                assert!(!matches!(event, Event::ActionComplete(..)));
            },
            other => panic!("expected WebRequestResult, got {other:?}"),
        }
        drop(holds);
        assert_eq!(web_requests_in_flight(), 0);

        let global: Vec<_> = (1..=MAX_WEB_REQUESTS_IN_FLIGHT)
            .map(|id| {
                try_acquire_web_request(id as PluginId, Arc::new(AtomicBool::new(true)), 0)
                    .expect("process-wide web slot")
            })
            .collect();
        let in_flight = web_requests_in_flight();
        web_request(
            &jobs.env,
            "http://127.0.0.1/not-started".to_owned(),
            HttpVerb::Get,
            BTreeMap::new(),
            Vec::new(),
            context.clone(),
        );
        assert_eq!(web_requests_in_flight(), in_flight);
        assert!(
            jobs.jobs_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err(),
            "a web request past the process-wide cap must not be enqueued"
        );
        match recv_load_result(&jobs.plugin_rx) {
            PluginInstruction::ApplyLoadResult {
                generation, event, ..
            } => {
                assert_eq!(generation, 5);
                assert_eq!(
                    event,
                    crate::background_jobs::web_request_limit_event(context)
                );
            },
            other => panic!("expected WebRequestResult, got {other:?}"),
        }
        drop(global);
        assert_eq!(web_requests_in_flight(), 0);
    }

    #[test]
    fn a_finished_web_request_lets_another_one_start() {
        let _lock = lock_web_request_tests();
        assert_eq!(web_requests_in_flight(), 0);
        let plugin_id = 8812;
        let jobs = job_channels(plugin_id, 6);
        let context = command_context();
        web_request(
            &jobs.env,
            "http://127.0.0.1/not-started".to_owned(),
            HttpVerb::Post,
            BTreeMap::new(),
            Vec::new(),
            context.clone(),
        );
        assert_eq!(web_requests_in_flight(), 1);
        let (job, _) = jobs
            .jobs_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("admitted web request is enqueued");
        let BackgroundJob::WebRequest(
            id,
            client_id,
            url,
            verb,
            headers,
            body,
            job_context,
            generation,
            reservation,
        ) = job
        else {
            panic!("expected WebRequest");
        };
        assert_eq!(generation, 6);
        assert_eq!(job_context, context);
        crate::global_async_runtime::get_tokio_runtime().block_on(
            crate::background_jobs::perform_web_request(
                id,
                client_id,
                url,
                verb,
                headers,
                body,
                job_context,
                generation,
                None,
                jobs.env.senders.clone(),
                reservation,
            ),
        );
        assert_eq!(
            web_requests_in_flight(),
            0,
            "a request with no http client releases its slot"
        );

        web_request(
            &jobs.env,
            "http://127.0.0.1/not-started".to_owned(),
            HttpVerb::Post,
            BTreeMap::new(),
            Vec::new(),
            context,
        );
        assert_eq!(
            web_requests_in_flight(),
            1,
            "a finished web request must let another one start"
        );
        let (job, _) = jobs
            .jobs_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("the next web request is enqueued");
        let BackgroundJob::WebRequest(
            id,
            client_id,
            url,
            verb,
            headers,
            body,
            job_context,
            generation,
            reservation,
        ) = job
        else {
            panic!("expected WebRequest");
        };
        crate::global_async_runtime::get_tokio_runtime().block_on(
            crate::background_jobs::perform_web_request(
                id,
                client_id,
                url,
                verb,
                headers,
                body,
                job_context,
                generation,
                None,
                jobs.env.senders.clone(),
                reservation,
            ),
        );
        assert_eq!(web_requests_in_flight(), 0);
    }
}
