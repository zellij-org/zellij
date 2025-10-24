use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use crate::thread_bus::ThreadSenders;
use crate::{
    os_input_output::ServerOsApi,
    panes::PaneId,
    plugins::PluginInstruction,
    pty::{ClientTabIndexOrPaneId, NewPanePlacement, PtyInstruction},
    screen::ScreenInstruction,
    ServerInstruction, SessionMetaData, SessionState,
};
use std::thread;
use std::time::Duration;
use uuid::Uuid;
use zellij_utils::{
    channels::SenderWithContext,
    data::{Direction, Event, InputMode, PluginCapabilities, ResizeStrategy},
    errors::prelude::*,
    input::{
        actions::{Action, SearchDirection, SearchOption},
        command::TerminalAction,
        get_mode_info,
        keybinds::Keybinds,
        layout::Layout,
    },
    ipc::{
        ClientAttributes, ClientToServerMsg, ExitReason, IpcReceiverWithContext, ServerToClientMsg,
    },
};

use crate::ClientId;

pub(crate) fn route_action(
    action: Action,
    client_id: ClientId,
    pane_id: Option<PaneId>,
    senders: ThreadSenders,
    capabilities: PluginCapabilities,
    client_attributes: ClientAttributes,
    default_shell: Option<TerminalAction>,
    default_layout: Box<Layout>,
    mut seen_cli_pipes: Option<&mut HashSet<String>>,
    client_keybinds: Keybinds,
    default_mode: InputMode,
) -> Result<bool> {
    let mut should_break = false;
    let err_context = || format!("failed to route action for client {client_id}");

    if !action.is_mouse_action() {
        // mouse actions should only send InputReceived to plugins
        // if they do not result in text being marked, this is handled in Tab
        senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                None,
                Some(client_id),
                Event::InputReceived,
            )]))
            .with_context(err_context)?;
    }

    match action {
        Action::ToggleTab => {
            senders
                .send_to_screen(ScreenInstruction::ToggleTab(client_id))
                .with_context(err_context)?;
        },
        Action::Write {
            key_with_modifier,
            bytes: raw_bytes,
            is_kitty_keyboard_protocol,
        } => {
            senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::WriteCharacter(
                    key_with_modifier,
                    raw_bytes,
                    is_kitty_keyboard_protocol,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::WriteChars { chars } => {
            senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .with_context(err_context)?;
            let chars = chars.into_bytes();
            senders
                .send_to_screen(ScreenInstruction::WriteCharacter(
                    None, chars, false, client_id,
                ))
                .with_context(err_context)?;
        },
        Action::SwitchToMode { input_mode } => {
            let attrs = &client_attributes;
            senders
                .send_to_server(ServerInstruction::ChangeMode(client_id, input_mode))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::ChangeMode(
                    get_mode_info(
                        input_mode,
                        attrs,
                        capabilities,
                        &client_keybinds,
                        Some(default_mode),
                    ),
                    client_id,
                ))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::Render)
                .with_context(err_context)?;
        },
        Action::Resize { resize, direction } => {
            let screen_instr =
                ScreenInstruction::Resize(client_id, ResizeStrategy::new(resize, direction));
            senders
                .send_to_screen(screen_instr)
                .with_context(err_context)?;
        },
        Action::SwitchFocus => {
            senders
                .send_to_screen(ScreenInstruction::SwitchFocus(client_id))
                .with_context(err_context)?;
        },
        Action::FocusNextPane => {
            senders
                .send_to_screen(ScreenInstruction::FocusNextPane(client_id))
                .with_context(err_context)?;
        },
        Action::FocusPreviousPane => {
            senders
                .send_to_screen(ScreenInstruction::FocusPreviousPane(client_id))
                .with_context(err_context)?;
        },
        Action::MoveFocus { direction } => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeft(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRight(client_id),
                Direction::Up => ScreenInstruction::MoveFocusUp(client_id),
                Direction::Down => ScreenInstruction::MoveFocusDown(client_id),
            };
            senders
                .send_to_screen(screen_instr)
                .with_context(err_context)?;
        },
        Action::MoveFocusOrTab { direction } => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRightOrNextTab(client_id),
                Direction::Up => ScreenInstruction::SwitchTabNext(client_id),
                Direction::Down => ScreenInstruction::SwitchTabPrev(client_id),
            };
            senders
                .send_to_screen(screen_instr)
                .with_context(err_context)?;
        },
        Action::MovePane { direction } => {
            let screen_instr = match direction {
                Some(Direction::Left) => ScreenInstruction::MovePaneLeft(client_id),
                Some(Direction::Right) => ScreenInstruction::MovePaneRight(client_id),
                Some(Direction::Up) => ScreenInstruction::MovePaneUp(client_id),
                Some(Direction::Down) => ScreenInstruction::MovePaneDown(client_id),
                None => ScreenInstruction::MovePane(client_id),
            };
            senders
                .send_to_screen(screen_instr)
                .with_context(err_context)?;
        },
        Action::MovePaneBackwards => {
            senders
                .send_to_screen(ScreenInstruction::MovePaneBackwards(client_id))
                .with_context(err_context)?;
        },
        Action::ClearScreen => {
            senders
                .send_to_screen(ScreenInstruction::ClearScreen(client_id))
                .with_context(err_context)?;
        },
        Action::DumpScreen {
            file_path,
            include_scrollback,
        } => {
            senders
                .send_to_screen(ScreenInstruction::DumpScreen(
                    file_path,
                    client_id,
                    include_scrollback,
                ))
                .with_context(err_context)?;
        },
        Action::DumpLayout => {
            let default_shell = match default_shell {
                Some(TerminalAction::RunCommand(run_command)) => Some(run_command.command),
                _ => None,
            };
            senders
                .send_to_screen(ScreenInstruction::DumpLayout(default_shell, client_id))
                .with_context(err_context)?;
        },
        Action::EditScrollback => {
            senders
                .send_to_screen(ScreenInstruction::EditScrollback(client_id))
                .with_context(err_context)?;
        },
        Action::ScrollUp => {
            senders
                .send_to_screen(ScreenInstruction::ScrollUp(client_id))
                .with_context(err_context)?;
        },
        Action::ScrollUpAt { position } => {
            senders
                .send_to_screen(ScreenInstruction::ScrollUpAt(position, client_id))
                .with_context(err_context)?;
        },
        Action::ScrollDown => {
            senders
                .send_to_screen(ScreenInstruction::ScrollDown(client_id))
                .with_context(err_context)?;
        },
        Action::ScrollDownAt { position } => {
            senders
                .send_to_screen(ScreenInstruction::ScrollDownAt(position, client_id))
                .with_context(err_context)?;
        },
        Action::ScrollToBottom => {
            senders
                .send_to_screen(ScreenInstruction::ScrollToBottom(client_id))
                .with_context(err_context)?;
        },
        Action::ScrollToTop => {
            senders
                .send_to_screen(ScreenInstruction::ScrollToTop(client_id))
                .with_context(err_context)?;
        },
        Action::PageScrollUp => {
            senders
                .send_to_screen(ScreenInstruction::PageScrollUp(client_id))
                .with_context(err_context)?;
        },
        Action::PageScrollDown => {
            senders
                .send_to_screen(ScreenInstruction::PageScrollDown(client_id))
                .with_context(err_context)?;
        },
        Action::HalfPageScrollUp => {
            senders
                .send_to_screen(ScreenInstruction::HalfPageScrollUp(client_id))
                .with_context(err_context)?;
        },
        Action::HalfPageScrollDown => {
            senders
                .send_to_screen(ScreenInstruction::HalfPageScrollDown(client_id))
                .with_context(err_context)?;
        },
        Action::ToggleFocusFullscreen => {
            senders
                .send_to_screen(ScreenInstruction::ToggleActiveTerminalFullscreen(client_id))
                .with_context(err_context)?;
        },
        Action::TogglePaneFrames => {
            senders
                .send_to_screen(ScreenInstruction::TogglePaneFrames)
                .with_context(err_context)?;
        },
        Action::NewPane {
            direction,
            pane_name,
            start_suppressed,
        } => {
            let shell = default_shell.clone();
            let new_pane_placement = match direction {
                Some(direction) => NewPanePlacement::Tiled(Some(direction)),
                None => NewPanePlacement::NoPreference,
            };
            let _ = senders.send_to_pty(PtyInstruction::SpawnTerminal(
                shell,
                pane_name,
                new_pane_placement,
                start_suppressed,
                ClientTabIndexOrPaneId::ClientId(client_id),
            ));
        },
        Action::EditFile {
            payload: open_file_payload,
            direction: split_direction,
            floating: should_float,
            in_place: should_open_in_place,
            start_suppressed,
            coordinates: floating_pane_coordinates,
        } => {
            let title = format!("Editing: {}", open_file_payload.path.display());
            let open_file = TerminalAction::OpenFile(open_file_payload);
            let pty_instr = if should_open_in_place {
                match pane_id {
                    Some(pane_id) => PtyInstruction::SpawnInPlaceTerminal(
                        Some(open_file),
                        Some(title),
                        false,
                        ClientTabIndexOrPaneId::PaneId(pane_id),
                    ),
                    None => PtyInstruction::SpawnInPlaceTerminal(
                        Some(open_file),
                        Some(title),
                        false,
                        ClientTabIndexOrPaneId::ClientId(client_id),
                    ),
                }
            } else {
                PtyInstruction::SpawnTerminal(
                    Some(open_file),
                    Some(title),
                    if should_float {
                        NewPanePlacement::Floating(floating_pane_coordinates)
                    } else {
                        NewPanePlacement::Tiled(split_direction)
                    },
                    start_suppressed,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                )
            };
            senders.send_to_pty(pty_instr).with_context(err_context)?;
        },
        Action::SwitchModeForAllClients { input_mode } => {
            let attrs = &client_attributes;
            senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    None,
                    Event::ModeUpdate(get_mode_info(
                        input_mode,
                        attrs,
                        capabilities,
                        &client_keybinds,
                        Some(default_mode),
                    )),
                )]))
                .with_context(err_context)?;

            senders
                .send_to_server(ServerInstruction::ChangeModeForAllClients(input_mode))
                .with_context(err_context)?;

            senders
                .send_to_screen(ScreenInstruction::ChangeModeForAllClients(get_mode_info(
                    input_mode,
                    attrs,
                    capabilities,
                    &client_keybinds,
                    Some(default_mode),
                )))
                .with_context(err_context)?;
        },
        Action::NewFloatingPane {
            command: run_command,
            pane_name: name,
            coordinates: floating_pane_coordinates,
        } => {
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    run_cmd,
                    name,
                    NewPanePlacement::Floating(floating_pane_coordinates),
                    false,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ))
                .with_context(err_context)?;
        },
        Action::NewInPlacePane {
            command: run_command,
            pane_name: name,
        } => {
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            match pane_id {
                Some(pane_id) => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
                            run_cmd,
                            name,
                            false,
                            ClientTabIndexOrPaneId::PaneId(pane_id),
                        ))
                        .with_context(err_context)?;
                },
                None => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
                            run_cmd,
                            name,
                            false,
                            ClientTabIndexOrPaneId::ClientId(client_id),
                        ))
                        .with_context(err_context)?;
                },
            }
        },
        Action::NewStackedPane {
            command: run_command,
            pane_name: name,
        } => {
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            match pane_id {
                Some(pane_id) => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnTerminal(
                            run_cmd,
                            name,
                            NewPanePlacement::Stacked(Some(pane_id)),
                            false,
                            ClientTabIndexOrPaneId::PaneId(pane_id),
                        ))
                        .with_context(err_context)?;
                },
                None => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnTerminal(
                            run_cmd,
                            name,
                            NewPanePlacement::Stacked(None),
                            false,
                            ClientTabIndexOrPaneId::ClientId(client_id),
                        ))
                        .with_context(err_context)?;
                },
            }
        },
        Action::NewTiledPane {
            direction,
            command: run_command,
            pane_name: name,
        } => {
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            let _ = senders.send_to_pty(PtyInstruction::SpawnTerminal(
                run_cmd,
                name,
                NewPanePlacement::Tiled(direction),
                false,
                ClientTabIndexOrPaneId::ClientId(client_id),
            ));
        },
        Action::TogglePaneEmbedOrFloating => {
            senders
                .send_to_screen(ScreenInstruction::TogglePaneEmbedOrFloating(client_id))
                .with_context(err_context)?;
        },
        Action::ToggleFloatingPanes => {
            senders
                .send_to_screen(ScreenInstruction::ToggleFloatingPanes(
                    client_id,
                    default_shell.clone(),
                ))
                .with_context(err_context)?;
        },
        Action::PaneNameInput { input } => {
            senders
                .send_to_screen(ScreenInstruction::UpdatePaneName(input, client_id))
                .with_context(err_context)?;
        },
        Action::UndoRenamePane => {
            senders
                .send_to_screen(ScreenInstruction::UndoRenamePane(client_id))
                .with_context(err_context)?;
        },
        Action::Run { command } => {
            let run_cmd = Some(TerminalAction::RunCommand(command.clone().into()));
            let _ = senders.send_to_pty(PtyInstruction::SpawnTerminal(
                run_cmd,
                None,
                NewPanePlacement::Tiled(command.direction),
                false,
                ClientTabIndexOrPaneId::ClientId(client_id),
            ));
        },
        Action::CloseFocus => {
            senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane(client_id))
                .with_context(err_context)?;
        },
        Action::NewTab {
            tiled_layout: tab_layout,
            floating_layouts: floating_panes_layout,
            swap_tiled_layouts,
            swap_floating_layouts,
            tab_name,
            should_change_focus_to_new_tab,
            cwd,
        } => {
            let shell = default_shell.clone();
            let swap_tiled_layouts =
                swap_tiled_layouts.unwrap_or_else(|| default_layout.swap_tiled_layouts.clone());
            let swap_floating_layouts = swap_floating_layouts
                .unwrap_or_else(|| default_layout.swap_floating_layouts.clone());
            let is_web_client = false; // actions cannot be initiated directly from the web
            senders
                .send_to_screen(ScreenInstruction::NewTab(
                    cwd,
                    shell,
                    tab_layout,
                    floating_panes_layout,
                    tab_name,
                    (swap_tiled_layouts, swap_floating_layouts),
                    should_change_focus_to_new_tab,
                    (client_id, is_web_client),
                ))
                .with_context(err_context)?;
        },
        Action::GoToNextTab => {
            senders
                .send_to_screen(ScreenInstruction::SwitchTabNext(client_id))
                .with_context(err_context)?;
        },
        Action::GoToPreviousTab => {
            senders
                .send_to_screen(ScreenInstruction::SwitchTabPrev(client_id))
                .with_context(err_context)?;
        },
        Action::ToggleActiveSyncTab => {
            senders
                .send_to_screen(ScreenInstruction::ToggleActiveSyncTab(client_id))
                .with_context(err_context)?;
        },
        Action::CloseTab => {
            senders
                .send_to_screen(ScreenInstruction::CloseTab(client_id))
                .with_context(err_context)?;
        },
        Action::GoToTab { index } => {
            senders
                .send_to_screen(ScreenInstruction::GoToTab(index, Some(client_id)))
                .with_context(err_context)?;
        },
        Action::GoToTabName { name, create } => {
            let shell = default_shell.clone();
            let swap_tiled_layouts = default_layout.swap_tiled_layouts.clone();
            let swap_floating_layouts = default_layout.swap_floating_layouts.clone();
            senders
                .send_to_screen(ScreenInstruction::GoToTabName(
                    name,
                    (swap_tiled_layouts, swap_floating_layouts),
                    shell,
                    create,
                    Some(client_id),
                ))
                .with_context(err_context)?;
        },
        Action::TabNameInput { input } => {
            senders
                .send_to_screen(ScreenInstruction::UpdateTabName(input, client_id))
                .with_context(err_context)?;
        },
        Action::UndoRenameTab => {
            senders
                .send_to_screen(ScreenInstruction::UndoRenameTab(client_id))
                .with_context(err_context)?;
        },
        Action::MoveTab { direction } => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveTabLeft(client_id),
                Direction::Right => ScreenInstruction::MoveTabRight(client_id),
                _ => return Ok(false),
            };
            senders
                .send_to_screen(screen_instr)
                .with_context(err_context)?;
        },
        Action::Quit => {
            senders
                .send_to_server(ServerInstruction::ClientExit(client_id))
                .with_context(err_context)?;
            should_break = true;
        },
        Action::Detach => {
            senders
                .send_to_server(ServerInstruction::DetachSession(vec![client_id]))
                .with_context(err_context)?;
            should_break = true;
        },
        Action::MouseEvent { event } => {
            senders
                .send_to_screen(ScreenInstruction::MouseEvent(event, client_id))
                .with_context(err_context)?;
        },
        Action::Copy => {
            senders
                .send_to_screen(ScreenInstruction::Copy(client_id))
                .with_context(err_context)?;
        },
        Action::Confirm => {
            senders
                .send_to_screen(ScreenInstruction::ConfirmPrompt(client_id))
                .with_context(err_context)?;
        },
        Action::Deny => {
            senders
                .send_to_screen(ScreenInstruction::DenyPrompt(client_id))
                .with_context(err_context)?;
        },
        #[allow(clippy::single_match)]
        Action::SkipConfirm { action } => match *action {
            Action::Quit => {
                senders
                    .send_to_server(ServerInstruction::ClientExit(client_id))
                    .with_context(err_context)?;
                should_break = true;
            },
            _ => {},
        },
        Action::NoOp => {},
        Action::SearchInput { input } => {
            senders
                .send_to_screen(ScreenInstruction::UpdateSearch(input, client_id))
                .with_context(err_context)?;
        },
        Action::Search { direction } => {
            let instruction = match direction {
                SearchDirection::Down => ScreenInstruction::SearchDown(client_id),
                SearchDirection::Up => ScreenInstruction::SearchUp(client_id),
            };
            senders
                .send_to_screen(instruction)
                .with_context(err_context)?;
        },
        Action::SearchToggleOption { option } => {
            let instruction = match option {
                SearchOption::CaseSensitivity => {
                    ScreenInstruction::SearchToggleCaseSensitivity(client_id)
                },
                SearchOption::WholeWord => ScreenInstruction::SearchToggleWholeWord(client_id),
                SearchOption::Wrap => ScreenInstruction::SearchToggleWrap(client_id),
            };
            senders
                .send_to_screen(instruction)
                .with_context(err_context)?;
        },
        Action::ToggleMouseMode => {}, // Handled client side
        Action::PreviousSwapLayout => {
            senders
                .send_to_screen(ScreenInstruction::PreviousSwapLayout(client_id))
                .with_context(err_context)?;
        },
        Action::NextSwapLayout => {
            senders
                .send_to_screen(ScreenInstruction::NextSwapLayout(client_id))
                .with_context(err_context)?;
        },
        Action::QueryTabNames => {
            senders
                .send_to_screen(ScreenInstruction::QueryTabNames(client_id))
                .with_context(err_context)?;
        },
        Action::NewTiledPluginPane {
            plugin: run_plugin,
            pane_name: name,
            skip_cache,
            cwd,
        } => {
            senders
                .send_to_screen(ScreenInstruction::NewTiledPluginPane(
                    run_plugin, name, skip_cache, cwd, client_id,
                ))
                .with_context(err_context)?;
        },
        Action::NewFloatingPluginPane {
            plugin: run_plugin,
            pane_name: name,
            skip_cache,
            cwd,
            coordinates: floating_pane_coordinates,
        } => {
            senders
                .send_to_screen(ScreenInstruction::NewFloatingPluginPane(
                    run_plugin,
                    name,
                    skip_cache,
                    cwd,
                    floating_pane_coordinates,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::NewInPlacePluginPane {
            plugin: run_plugin,
            pane_name: name,
            skip_cache,
        } => {
            if let Some(pane_id) = pane_id {
                senders
                    .send_to_screen(ScreenInstruction::NewInPlacePluginPane(
                        run_plugin, name, pane_id, skip_cache, client_id,
                    ))
                    .with_context(err_context)?;
            } else {
                log::error!("Must have pane_id in order to open in place pane");
            }
        },
        Action::StartOrReloadPlugin { plugin: run_plugin } => {
            senders
                .send_to_screen(ScreenInstruction::StartOrReloadPluginPane(run_plugin, None))
                .with_context(err_context)?;
        },
        Action::LaunchOrFocusPlugin {
            plugin: run_plugin,
            should_float,
            move_to_focused_tab,
            should_open_in_place,
            skip_cache,
        } => {
            senders
                .send_to_screen(ScreenInstruction::LaunchOrFocusPlugin(
                    run_plugin,
                    should_float,
                    move_to_focused_tab,
                    should_open_in_place,
                    pane_id,
                    skip_cache,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::LaunchPlugin {
            plugin: run_plugin,
            should_float,
            should_open_in_place,
            skip_cache,
            cwd,
        } => {
            senders
                .send_to_screen(ScreenInstruction::LaunchPlugin(
                    run_plugin,
                    should_float,
                    should_open_in_place,
                    pane_id,
                    skip_cache,
                    cwd,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::CloseTerminalPane {
            pane_id: terminal_pane_id,
        } => {
            senders
                .send_to_screen(ScreenInstruction::ClosePane(
                    PaneId::Terminal(terminal_pane_id),
                    None, // we send None here so that the terminal pane would be closed anywhere
                          // in the app, not just in the client's tab
                ))
                .with_context(err_context)?;
        },
        Action::ClosePluginPane {
            pane_id: plugin_pane_id,
        } => {
            senders
                .send_to_screen(ScreenInstruction::ClosePane(
                    PaneId::Plugin(plugin_pane_id),
                    None, // we send None here so that the terminal pane would be closed anywhere
                          // in the app, not just in the client's tab
                ))
                .with_context(err_context)?;
        },
        Action::FocusTerminalPaneWithId {
            pane_id,
            should_float_if_hidden,
        } => {
            senders
                .send_to_screen(ScreenInstruction::FocusPaneWithId(
                    PaneId::Terminal(pane_id),
                    should_float_if_hidden,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::FocusPluginPaneWithId {
            pane_id,
            should_float_if_hidden,
        } => {
            senders
                .send_to_screen(ScreenInstruction::FocusPaneWithId(
                    PaneId::Plugin(pane_id),
                    should_float_if_hidden,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::RenameTerminalPane {
            pane_id,
            name: name_bytes,
        } => {
            senders
                .send_to_screen(ScreenInstruction::RenamePane(
                    PaneId::Terminal(pane_id),
                    name_bytes,
                ))
                .with_context(err_context)?;
        },
        Action::RenamePluginPane {
            pane_id,
            name: name_bytes,
        } => {
            senders
                .send_to_screen(ScreenInstruction::RenamePane(
                    PaneId::Plugin(pane_id),
                    name_bytes,
                ))
                .with_context(err_context)?;
        },
        Action::RenameTab {
            tab_index: tab_position,
            name: name_bytes,
        } => {
            senders
                .send_to_screen(ScreenInstruction::RenameTab(
                    tab_position as usize,
                    name_bytes,
                ))
                .with_context(err_context)?;
        },
        Action::BreakPane => {
            senders
                .send_to_screen(ScreenInstruction::BreakPane(
                    default_layout.clone(),
                    default_shell.clone(),
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::BreakPaneRight => {
            senders
                .send_to_screen(ScreenInstruction::BreakPaneRight(client_id))
                .with_context(err_context)?;
        },
        Action::BreakPaneLeft => {
            senders
                .send_to_screen(ScreenInstruction::BreakPaneLeft(client_id))
                .with_context(err_context)?;
        },
        Action::RenameSession { name } => {
            senders
                .send_to_screen(ScreenInstruction::RenameSession(name, client_id))
                .with_context(err_context)?;
        },
        Action::CliPipe {
            pipe_id,
            mut name,
            payload,
            plugin,
            args,
            configuration,
            floating,
            in_place,
            skip_cache,
            cwd,
            pane_title,
            ..
        } => {
            if let Some(seen_cli_pipes) = seen_cli_pipes.as_mut() {
                if !seen_cli_pipes.contains(&pipe_id) {
                    seen_cli_pipes.insert(pipe_id.clone());
                    senders
                        .send_to_server(ServerInstruction::AssociatePipeWithClient {
                            pipe_id: pipe_id.clone(),
                            client_id,
                        })
                        .with_context(err_context)?;
                }
            }
            if let Some(name) = name.take() {
                let should_open_in_place = in_place.unwrap_or(false);
                if should_open_in_place && pane_id.is_none() {
                    log::error!("Was asked to open a new plugin in-place, but cannot identify the pane id... is the ZELLIJ_PANE_ID variable set?");
                }
                let pane_id_to_replace = if should_open_in_place { pane_id } else { None };
                senders
                    .send_to_plugin(PluginInstruction::CliPipe {
                        pipe_id,
                        name,
                        payload,
                        plugin,
                        args,
                        configuration,
                        floating,
                        pane_id_to_replace,
                        cwd,
                        pane_title,
                        skip_cache,
                        cli_client_id: client_id,
                    })
                    .with_context(err_context)?;
            } else {
                log::error!("Message must have a name");
            }
        },
        Action::KeybindPipe {
            mut name,
            payload,
            plugin,
            args,
            mut configuration,
            floating,
            in_place,
            skip_cache,
            cwd,
            pane_title,
            launch_new,
            plugin_id,
            ..
        } => {
            if let Some(name) = name.take() {
                let should_open_in_place = in_place.unwrap_or(false);
                let pane_id_to_replace = if should_open_in_place { pane_id } else { None };
                if launch_new && plugin_id.is_none() {
                    // we do this to make sure the plugin is unique (has a unique configuration parameter)
                    configuration
                        .get_or_insert_with(BTreeMap::new)
                        .insert("_zellij_id".to_owned(), Uuid::new_v4().to_string());
                }
                senders
                    .send_to_plugin(PluginInstruction::KeybindPipe {
                        name,
                        payload,
                        plugin,
                        args,
                        configuration,
                        floating,
                        pane_id_to_replace,
                        cwd,
                        pane_title,
                        skip_cache,
                        cli_client_id: client_id,
                        plugin_and_client_id: plugin_id.map(|plugin_id| (plugin_id, client_id)),
                    })
                    .with_context(err_context)?;
            } else {
                log::error!("Message must have a name");
            }
        },
        Action::ListClients => {
            let default_shell = match default_shell {
                Some(TerminalAction::RunCommand(run_command)) => Some(run_command.command),
                _ => None,
            };
            senders
                .send_to_screen(ScreenInstruction::ListClientsMetadata(
                    default_shell,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::TogglePanePinned => {
            senders
                .send_to_screen(ScreenInstruction::TogglePanePinned(client_id))
                .with_context(err_context)?;
        },
        Action::StackPanes {
            pane_ids: pane_ids_to_stack,
        } => {
            senders
                .send_to_screen(ScreenInstruction::StackPanes(
                    pane_ids_to_stack.iter().map(|p| PaneId::from(*p)).collect(),
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::ChangeFloatingPaneCoordinates {
            pane_id,
            coordinates,
        } => {
            senders
                .send_to_screen(ScreenInstruction::ChangeFloatingPanesCoordinates(vec![(
                    pane_id.into(),
                    coordinates,
                )]))
                .with_context(err_context)?;
        },
        Action::TogglePaneInGroup => {
            senders
                .send_to_screen(ScreenInstruction::TogglePaneInGroup(client_id))
                .with_context(err_context)?;
        },
        Action::ToggleGroupMarking => {
            senders
                .send_to_screen(ScreenInstruction::ToggleGroupMarking(client_id))
                .with_context(err_context)?;
        },
    }
    Ok(should_break)
}

// this should only be used for one-off startup instructions
macro_rules! send_to_screen_or_retry_queue {
    ($rlocked_sessions:expr, $message:expr, $instruction: expr, $retry_queue:expr) => {{
        match $rlocked_sessions.as_ref() {
            Some(session_metadata) => session_metadata.senders.send_to_screen($message),
            None => {
                log::warn!("Server not ready, trying to place instruction in retry queue...");
                if let Some(retry_queue) = $retry_queue.as_mut() {
                    retry_queue.push_back($instruction);
                }
                Ok(())
            },
        }
    }};
}

pub(crate) fn route_thread_main(
    session_data: Arc<RwLock<Option<SessionMetaData>>>,
    session_state: Arc<RwLock<SessionState>>,
    os_input: Box<dyn ServerOsApi>,
    to_server: SenderWithContext<ServerInstruction>,
    mut receiver: IpcReceiverWithContext<ClientToServerMsg>,
    client_id: ClientId,
) -> Result<()> {
    let mut retry_queue = VecDeque::new();
    let err_context = || format!("failed to handle instruction for client {client_id}");
    let mut seen_cli_pipes = HashSet::new();
    let mut consecutive_unknown_messages_received = 0;
    'route_loop: loop {
        match receiver.recv_client_msg() {
            Some((instruction, err_ctx)) => {
                consecutive_unknown_messages_received = 0;
                err_ctx.update_thread_ctx();
                let mut handle_instruction = |instruction: ClientToServerMsg,
                                              mut retry_queue: Option<
                    &mut VecDeque<ClientToServerMsg>,
                >|
                 -> Result<bool> {
                    let mut should_break = false;
                    let rlocked_sessions =
                        session_data.read().to_anyhow().with_context(err_context)?;

                    // Check if this is a watcher client and ignore input messages
                    let is_watcher = session_state.read().unwrap().is_watcher(&client_id);
                    if is_watcher {
                        match instruction {
                            ClientToServerMsg::Key { .. }
                            | ClientToServerMsg::Action { .. }
                            // TerminalResize removed - watchers can now report size
                            | ClientToServerMsg::TerminalPixelDimensions { .. }
                            | ClientToServerMsg::BackgroundColor { .. }
                            | ClientToServerMsg::ForegroundColor { .. }
                            | ClientToServerMsg::ColorRegisters { .. } => {
                                // Ignore all input from watcher clients
                                return Ok(should_break);
                            },
                            _ => {
                                // Allow other messages (like ClientExited, TerminalResize, etc.)
                            },
                        }
                    }

                    match instruction {
                        ClientToServerMsg::Key {
                            key,
                            raw_bytes,
                            is_kitty_keyboard_protocol,
                        } => {
                            if let Some(rlocked_sessions) = rlocked_sessions.as_ref() {
                                match rlocked_sessions.get_client_keybinds_and_mode(&client_id) {
                                    Some((keybinds, input_mode, default_input_mode)) => {
                                        for action in keybinds
                                            .get_actions_for_key_in_mode_or_default_action(
                                                &input_mode,
                                                &key,
                                                raw_bytes,
                                                default_input_mode,
                                                is_kitty_keyboard_protocol,
                                            )
                                        {
                                            if route_action(
                                                action,
                                                client_id,
                                                None,
                                                rlocked_sessions.senders.clone(),
                                                rlocked_sessions.capabilities.clone(),
                                                rlocked_sessions.client_attributes.clone(),
                                                rlocked_sessions.default_shell.clone(),
                                                rlocked_sessions.layout.clone(),
                                                Some(&mut seen_cli_pipes),
                                                keybinds.clone(),
                                                rlocked_sessions
                                                    .session_configuration
                                                    .get_client_configuration(&client_id)
                                                    .options
                                                    .default_mode
                                                    .unwrap_or(InputMode::Normal)
                                                    .clone(),
                                            )? {
                                                should_break = true;
                                            }
                                        }
                                    },
                                    None => {
                                        log::error!("Failed to get keybindings for client");
                                    },
                                }
                            }
                        },
                        ClientToServerMsg::Action {
                            action,
                            terminal_id: maybe_pane_id,
                            client_id: maybe_client_id,
                        } => {
                            let client_id = maybe_client_id.unwrap_or(client_id);
                            if let Some(rlocked_sessions) = rlocked_sessions.as_ref() {
                                if route_action(
                                    action,
                                    client_id,
                                    maybe_pane_id.map(|p| PaneId::Terminal(p)),
                                    rlocked_sessions.senders.clone(),
                                    rlocked_sessions.capabilities.clone(),
                                    rlocked_sessions.client_attributes.clone(),
                                    rlocked_sessions.default_shell.clone(),
                                    rlocked_sessions.layout.clone(),
                                    Some(&mut seen_cli_pipes),
                                    rlocked_sessions
                                        .session_configuration
                                        .get_client_keybinds(&client_id)
                                        .clone(),
                                    rlocked_sessions
                                        .session_configuration
                                        .get_client_configuration(&client_id)
                                        .options
                                        .default_mode
                                        .unwrap_or(InputMode::Normal)
                                        .clone(),
                                )? {
                                    should_break = true;
                                }
                            }
                        },
                        ClientToServerMsg::TerminalResize { new_size } => {
                            // Check if this is a watcher or regular client
                            if is_watcher {
                                // For watchers: send size to Screen for tracking, don't affect screen size
                                send_to_screen_or_retry_queue!(
                                    rlocked_sessions,
                                    ScreenInstruction::WatcherTerminalResize(client_id, new_size),
                                    instruction,
                                    retry_queue
                                )
                                .with_context(err_context)?;
                            } else {
                                // For regular clients: existing behavior
                                session_state
                                    .write()
                                    .to_anyhow()
                                    .with_context(err_context)?
                                    .set_client_size(client_id, new_size);
                                session_state
                                    .read()
                                    .to_anyhow()
                                    .and_then(|state| {
                                        state.min_client_terminal_size().ok_or(anyhow!(
                                            "failed to determine minimal client terminal size"
                                        ))
                                    })
                                    .and_then(|min_size| {
                                        rlocked_sessions
                                            .as_ref()
                                            .context("couldn't get reference to read-locked session")?
                                            .senders
                                            .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                                    })
                                    .with_context(err_context)?;
                            }
                        },
                        ClientToServerMsg::TerminalPixelDimensions { pixel_dimensions } => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalPixelDimensions(pixel_dimensions),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::BackgroundColor {
                            color: ref background_color_instruction,
                        } => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalBackgroundColor(
                                    background_color_instruction.clone()
                                ),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::ForegroundColor {
                            color: ref foreground_color_instruction,
                        } => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalForegroundColor(
                                    foreground_color_instruction.clone()
                                ),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::ColorRegisters {
                            ref color_registers,
                        } => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalColorRegisters(
                                    color_registers
                                        .iter()
                                        .map(|c| (c.index, c.color.clone()))
                                        .collect()
                                ),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::FirstClientConnected {
                            cli_assets,
                            is_web_client,
                        } => {
                            let new_client_instruction = ServerInstruction::FirstClientConnected(
                                cli_assets,
                                is_web_client,
                                client_id,
                            );
                            to_server
                                .send(new_client_instruction)
                                .with_context(err_context)?;
                        },
                        ClientToServerMsg::AttachClient {
                            cli_assets,
                            tab_position_to_focus,
                            pane_to_focus: pane_id_to_focus,
                            is_web_client,
                        } => {
                            let allow_web_connections = rlocked_sessions
                                .as_ref()
                                .map(|rlocked_sessions| {
                                    rlocked_sessions.web_sharing.web_clients_allowed()
                                })
                                .unwrap_or(false);
                            let should_allow_connection = !is_web_client || allow_web_connections;
                            if should_allow_connection {
                                let attach_client_instruction = ServerInstruction::AttachClient(
                                    cli_assets,
                                    tab_position_to_focus,
                                    pane_id_to_focus.map(|p| (p.pane_id, p.is_plugin)),
                                    is_web_client,
                                    client_id,
                                );
                                to_server
                                    .send(attach_client_instruction)
                                    .with_context(err_context)?;
                            } else {
                                let error = "This session does not allow web connections.";
                                let _ = to_server.send(ServerInstruction::LogError(
                                    vec![error.to_owned()],
                                    client_id,
                                ));
                                let _ = to_server
                                    .send(ServerInstruction::SendWebClientsForbidden(client_id));
                            }
                        },
                        ClientToServerMsg::AttachWatcherClient => {
                            let attach_watcher_instruction =
                                ServerInstruction::AttachWatcherClient(client_id);
                            to_server
                                .send(attach_watcher_instruction)
                                .with_context(err_context)?;
                        },
                        ClientToServerMsg::ClientExited => {
                            // we don't unwrap this because we don't really care if there's an error here (eg.
                            // if the main server thread exited before this router thread did)
                            let _ = to_server.send(ServerInstruction::RemoveClient(client_id));
                            return Ok(true);
                        },
                        ClientToServerMsg::KillSession => {
                            to_server
                                .send(ServerInstruction::KillSession)
                                .with_context(err_context)?;
                        },
                        ClientToServerMsg::ConnStatus => {
                            let _ = to_server.send(ServerInstruction::ConnStatus(client_id));
                            should_break = true;
                        },
                        ClientToServerMsg::DetachSession { client_ids } => {
                            let _ = to_server.send(ServerInstruction::DetachSession(client_ids));
                            should_break = true;
                        },
                        ClientToServerMsg::WebServerStarted { base_url } => {
                            let _ = to_server.send(ServerInstruction::WebServerStarted(base_url));
                        },
                        ClientToServerMsg::FailedToStartWebServer { error } => {
                            let _ =
                                to_server.send(ServerInstruction::FailedToStartWebServer(error));
                        },
                    }
                    Ok(should_break)
                };
                let mut repeat_retries = VecDeque::new();
                while let Some(instruction_to_retry) = retry_queue.pop_front() {
                    log::warn!("Server ready, retrying sending instruction.");
                    thread::sleep(Duration::from_millis(5));
                    let should_break =
                        handle_instruction(instruction_to_retry, Some(&mut repeat_retries))?;
                    if should_break {
                        break 'route_loop;
                    }
                }
                // retry on loop around
                retry_queue.append(&mut repeat_retries);
                let should_break = handle_instruction(instruction, Some(&mut retry_queue))?;
                if should_break {
                    break 'route_loop;
                }
            },
            None => {
                consecutive_unknown_messages_received += 1;
                log::error!("Received unknown message from client.");
                if consecutive_unknown_messages_received >= 1000 {
                    log::error!("Client sent over 1000 consecutive unknown messages, this is probably an infinite loop, logging client out");
                    let _ = os_input.send_to_client(
                        client_id,
                        ServerToClientMsg::Exit {
                            exit_reason: ExitReason::Error("Received empty message".to_string()),
                        },
                    );
                    let _ = to_server.send(ServerInstruction::RemoveClient(client_id));
                    break 'route_loop;
                }
            },
        }
    }
    Ok(())
}
