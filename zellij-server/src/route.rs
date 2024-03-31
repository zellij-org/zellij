use std::collections::VecDeque;
use std::sync::{Arc, RwLock};

use crate::thread_bus::ThreadSenders;
use crate::{
    os_input_output::ServerOsApi,
    panes::PaneId,
    plugins::PluginInstruction,
    pty::{ClientTabIndexOrPaneId, PtyInstruction},
    screen::ScreenInstruction,
    ServerInstruction, SessionMetaData, SessionState,
};
use zellij_utils::ipc::ReceiveError;
use zellij_utils::{
    channels::SenderWithContext,
    data::{Direction, Event, PluginCapabilities, ResizeStrategy},
    errors::prelude::*,
    input::{
        actions::{Action, SearchDirection, SearchOption},
        command::TerminalAction,
        get_mode_info,
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
) -> Result<bool> {
    let mut should_break = false;
    let err_context = || format!("failed to route action for client {client_id}");

    // forward the action to plugins unless it is a mousehold
    // this is a bit of a hack around the unfortunate architecture we use with plugins
    // this will change as soon as we refactor
    match action {
        Action::MouseHoldLeft(..) | Action::MouseHoldRight(..) => {},
        _ => {
            senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    Some(client_id),
                    Event::InputReceived,
                )]))
                .with_context(err_context)?;
        },
    }

    match action {
        Action::ToggleTab => {
            senders
                .send_to_screen(ScreenInstruction::ToggleTab(client_id))
                .with_context(err_context)?;
        },
        Action::Write(val) => {
            senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .with_context(err_context)?;
        },
        Action::WriteChars(val) => {
            senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .with_context(err_context)?;
            let val = val.into_bytes();
            senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .with_context(err_context)?;
        },
        Action::SwitchToMode(mode) => {
            let attrs = &client_attributes;
            // TODO: use the palette from the client and remove it from the server os api
            // this is left here as a stop gap measure until we shift some code around
            // to allow for this
            // TODO: Need access to `ClientAttributes` here
            senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    Some(client_id),
                    Event::ModeUpdate(get_mode_info(mode, attrs, capabilities)),
                )]))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::ChangeMode(
                    get_mode_info(mode, attrs, capabilities),
                    client_id,
                ))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::Render)
                .with_context(err_context)?;
        },
        Action::Resize(resize, direction) => {
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
        Action::MoveFocus(direction) => {
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
        Action::MoveFocusOrTab(direction) => {
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
        Action::MovePane(direction) => {
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
        Action::DumpScreen(val, full) => {
            senders
                .send_to_screen(ScreenInstruction::DumpScreen(val, client_id, full))
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
        Action::ScrollUpAt(point) => {
            senders
                .send_to_screen(ScreenInstruction::ScrollUpAt(point, client_id))
                .with_context(err_context)?;
        },
        Action::ScrollDown => {
            senders
                .send_to_screen(ScreenInstruction::ScrollDown(client_id))
                .with_context(err_context)?;
        },
        Action::ScrollDownAt(point) => {
            senders
                .send_to_screen(ScreenInstruction::ScrollDownAt(point, client_id))
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
        Action::NewPane(direction, name) => {
            let shell = default_shell.clone();
            let pty_instr = match direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(shell, name, client_id)
                },
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(shell, name, client_id)
                },
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(shell, name, client_id)
                },
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(shell, name, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(
                    shell,
                    None,
                    name,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ),
            };
            senders.send_to_pty(pty_instr).with_context(err_context)?;
        },
        Action::EditFile(
            path_to_file,
            line_number,
            cwd,
            split_direction,
            should_float,
            should_open_in_place,
        ) => {
            let title = format!("Editing: {}", path_to_file.display());
            let open_file = TerminalAction::OpenFile(path_to_file, line_number, cwd);
            let pty_instr = match (split_direction, should_float, should_open_in_place) {
                (Some(Direction::Left), false, false) => {
                    PtyInstruction::SpawnTerminalVertically(Some(open_file), Some(title), client_id)
                },
                (Some(Direction::Right), false, false) => {
                    PtyInstruction::SpawnTerminalVertically(Some(open_file), Some(title), client_id)
                },
                (Some(Direction::Up), false, false) => PtyInstruction::SpawnTerminalHorizontally(
                    Some(open_file),
                    Some(title),
                    client_id,
                ),
                (Some(Direction::Down), false, false) => PtyInstruction::SpawnTerminalHorizontally(
                    Some(open_file),
                    Some(title),
                    client_id,
                ),
                // open terminal in place
                (_, _, true) => match pane_id {
                    Some(pane_id) => PtyInstruction::SpawnInPlaceTerminal(
                        Some(open_file),
                        Some(title),
                        ClientTabIndexOrPaneId::PaneId(pane_id),
                    ),
                    None => PtyInstruction::SpawnInPlaceTerminal(
                        Some(open_file),
                        Some(title),
                        ClientTabIndexOrPaneId::ClientId(client_id),
                    ),
                },
                // Open either floating terminal if we were asked with should_float or defer
                // placement to screen
                (None, _, _) | (_, true, _) => PtyInstruction::SpawnTerminal(
                    Some(open_file),
                    Some(should_float),
                    Some(title),
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ),
            };
            senders.send_to_pty(pty_instr).with_context(err_context)?;
        },
        Action::SwitchModeForAllClients(input_mode) => {
            let attrs = &client_attributes;
            senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    None,
                    Event::ModeUpdate(get_mode_info(input_mode, attrs, capabilities)),
                )]))
                .with_context(err_context)?;
            senders
                .send_to_screen(ScreenInstruction::ChangeModeForAllClients(get_mode_info(
                    input_mode,
                    attrs,
                    capabilities,
                )))
                .with_context(err_context)?;
        },
        Action::NewFloatingPane(run_command, name) => {
            let should_float = true;
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    run_cmd,
                    Some(should_float),
                    name,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ))
                .with_context(err_context)?;
        },
        Action::NewInPlacePane(run_command, name) => {
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            match pane_id {
                Some(pane_id) => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
                            run_cmd,
                            name,
                            ClientTabIndexOrPaneId::PaneId(pane_id),
                        ))
                        .with_context(err_context)?;
                },
                None => {
                    senders
                        .send_to_pty(PtyInstruction::SpawnInPlaceTerminal(
                            run_cmd,
                            name,
                            ClientTabIndexOrPaneId::ClientId(client_id),
                        ))
                        .with_context(err_context)?;
                },
            }
        },
        Action::NewTiledPane(direction, run_command, name) => {
            let should_float = false;
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| default_shell.clone());
            let pty_instr = match direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, name, client_id)
                },
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, name, client_id)
                },
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, name, client_id)
                },
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, name, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(
                    run_cmd,
                    Some(should_float),
                    name,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ),
            };
            senders.send_to_pty(pty_instr).with_context(err_context)?;
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
        Action::PaneNameInput(c) => {
            senders
                .send_to_screen(ScreenInstruction::UpdatePaneName(c, client_id))
                .with_context(err_context)?;
        },
        Action::UndoRenamePane => {
            senders
                .send_to_screen(ScreenInstruction::UndoRenamePane(client_id))
                .with_context(err_context)?;
        },
        Action::Run(command) => {
            let run_cmd = Some(TerminalAction::RunCommand(command.clone().into()));
            let pty_instr = match command.direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, None, client_id)
                },
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, None, client_id)
                },
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, None, client_id)
                },
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, None, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(
                    run_cmd,
                    None,
                    None,
                    ClientTabIndexOrPaneId::ClientId(client_id),
                ),
            };
            senders.send_to_pty(pty_instr).with_context(err_context)?;
        },
        Action::CloseFocus => {
            senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane(client_id))
                .with_context(err_context)?;
        },
        Action::NewTab(
            tab_layout,
            floating_panes_layout,
            swap_tiled_layouts,
            swap_floating_layouts,
            tab_name,
        ) => {
            let shell = default_shell.clone();
            let swap_tiled_layouts =
                swap_tiled_layouts.unwrap_or_else(|| default_layout.swap_tiled_layouts.clone());
            let swap_floating_layouts = swap_floating_layouts
                .unwrap_or_else(|| default_layout.swap_floating_layouts.clone());
            senders
                .send_to_screen(ScreenInstruction::NewTab(
                    None,
                    shell,
                    tab_layout,
                    floating_panes_layout,
                    tab_name,
                    (swap_tiled_layouts, swap_floating_layouts),
                    client_id,
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
        Action::GoToTab(i) => {
            senders
                .send_to_screen(ScreenInstruction::GoToTab(i, Some(client_id)))
                .with_context(err_context)?;
        },
        Action::GoToTabName(name, create) => {
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
        Action::TabNameInput(c) => {
            senders
                .send_to_screen(ScreenInstruction::UpdateTabName(c, client_id))
                .with_context(err_context)?;
        },
        Action::UndoRenameTab => {
            senders
                .send_to_screen(ScreenInstruction::UndoRenameTab(client_id))
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
        Action::LeftClick(point) => {
            senders
                .send_to_screen(ScreenInstruction::LeftClick(point, client_id))
                .with_context(err_context)?;
        },
        Action::RightClick(point) => {
            senders
                .send_to_screen(ScreenInstruction::RightClick(point, client_id))
                .with_context(err_context)?;
        },
        Action::MiddleClick(point) => {
            senders
                .send_to_screen(ScreenInstruction::MiddleClick(point, client_id))
                .with_context(err_context)?;
        },
        Action::LeftMouseRelease(point) => {
            senders
                .send_to_screen(ScreenInstruction::LeftMouseRelease(point, client_id))
                .with_context(err_context)?;
        },
        Action::RightMouseRelease(point) => {
            senders
                .send_to_screen(ScreenInstruction::RightMouseRelease(point, client_id))
                .with_context(err_context)?;
        },
        Action::MiddleMouseRelease(point) => {
            senders
                .send_to_screen(ScreenInstruction::MiddleMouseRelease(point, client_id))
                .with_context(err_context)?;
        },
        Action::MouseHoldLeft(point) => {
            senders
                .send_to_screen(ScreenInstruction::MouseHoldLeft(point, client_id))
                .with_context(err_context)?;
        },
        Action::MouseHoldRight(point) => {
            senders
                .send_to_screen(ScreenInstruction::MouseHoldRight(point, client_id))
                .with_context(err_context)?;
        },
        Action::MouseHoldMiddle(point) => {
            senders
                .send_to_screen(ScreenInstruction::MouseHoldMiddle(point, client_id))
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
        Action::SkipConfirm(action) => match *action {
            Action::Quit => {
                senders
                    .send_to_server(ServerInstruction::ClientExit(client_id))
                    .with_context(err_context)?;
                should_break = true;
            },
            _ => {},
        },
        Action::NoOp => {},
        Action::SearchInput(c) => {
            senders
                .send_to_screen(ScreenInstruction::UpdateSearch(c, client_id))
                .with_context(err_context)?;
        },
        Action::Search(d) => {
            let instruction = match d {
                SearchDirection::Down => ScreenInstruction::SearchDown(client_id),
                SearchDirection::Up => ScreenInstruction::SearchUp(client_id),
            };
            senders
                .send_to_screen(instruction)
                .with_context(err_context)?;
        },
        Action::SearchToggleOption(o) => {
            let instruction = match o {
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
        Action::NewTiledPluginPane(run_plugin, name) => {
            senders
                .send_to_screen(ScreenInstruction::NewTiledPluginPane(
                    run_plugin, name, client_id,
                ))
                .with_context(err_context)?;
        },
        Action::NewFloatingPluginPane(run_plugin, name) => {
            senders
                .send_to_screen(ScreenInstruction::NewFloatingPluginPane(
                    run_plugin, name, client_id,
                ))
                .with_context(err_context)?;
        },
        Action::NewInPlacePluginPane(run_plugin, name) => {
            if let Some(pane_id) = pane_id {
                senders
                    .send_to_screen(ScreenInstruction::NewInPlacePluginPane(
                        run_plugin, name, pane_id, client_id,
                    ))
                    .with_context(err_context)?;
            } else {
                log::error!("Must have pane_id in order to open in place pane");
            }
        },
        Action::StartOrReloadPlugin(run_plugin) => {
            senders
                .send_to_screen(ScreenInstruction::StartOrReloadPluginPane(run_plugin, None))
                .with_context(err_context)?;
        },
        Action::LaunchOrFocusPlugin(
            run_plugin,
            should_float,
            move_to_focused_tab,
            should_open_in_place,
        ) => {
            senders
                .send_to_screen(ScreenInstruction::LaunchOrFocusPlugin(
                    run_plugin,
                    should_float,
                    move_to_focused_tab,
                    should_open_in_place,
                    pane_id,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::LaunchPlugin(run_plugin, should_float, should_open_in_place) => {
            senders
                .send_to_screen(ScreenInstruction::LaunchPlugin(
                    run_plugin,
                    should_float,
                    should_open_in_place,
                    pane_id,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::CloseTerminalPane(terminal_pane_id) => {
            senders
                .send_to_screen(ScreenInstruction::ClosePane(
                    PaneId::Terminal(terminal_pane_id),
                    None, // we send None here so that the terminal pane would be closed anywhere
                          // in the app, not just in the client's tab
                ))
                .with_context(err_context)?;
        },
        Action::ClosePluginPane(plugin_pane_id) => {
            senders
                .send_to_screen(ScreenInstruction::ClosePane(
                    PaneId::Plugin(plugin_pane_id),
                    None, // we send None here so that the terminal pane would be closed anywhere
                          // in the app, not just in the client's tab
                ))
                .with_context(err_context)?;
        },
        Action::FocusTerminalPaneWithId(pane_id, should_float_if_hidden) => {
            senders
                .send_to_screen(ScreenInstruction::FocusPaneWithId(
                    PaneId::Terminal(pane_id),
                    should_float_if_hidden,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::FocusPluginPaneWithId(pane_id, should_float_if_hidden) => {
            senders
                .send_to_screen(ScreenInstruction::FocusPaneWithId(
                    PaneId::Plugin(pane_id),
                    should_float_if_hidden,
                    client_id,
                ))
                .with_context(err_context)?;
        },
        Action::RenameTerminalPane(pane_id, name_bytes) => {
            senders
                .send_to_screen(ScreenInstruction::RenamePane(
                    PaneId::Terminal(pane_id),
                    name_bytes,
                ))
                .with_context(err_context)?;
        },
        Action::RenamePluginPane(pane_id, name_bytes) => {
            senders
                .send_to_screen(ScreenInstruction::RenamePane(
                    PaneId::Plugin(pane_id),
                    name_bytes,
                ))
                .with_context(err_context)?;
        },
        Action::RenameTab(tab_position, name_bytes) => {
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
        Action::RenameSession(name) => {
            senders
                .send_to_screen(ScreenInstruction::RenameSession(name, client_id))
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
    'route_loop: loop {
        match receiver.recv() {
            Ok((instruction, err_ctx)) => {
                err_ctx.update_thread_ctx();
                let rlocked_sessions = session_data.read().to_anyhow().with_context(err_context)?;
                let handle_instruction = |instruction: ClientToServerMsg,
                                          mut retry_queue: Option<
                    &mut VecDeque<ClientToServerMsg>,
                >|
                 -> Result<bool> {
                    let mut should_break = false;
                    match instruction {
                        ClientToServerMsg::Action(action, maybe_pane_id, maybe_client_id) => {
                            let client_id = maybe_client_id.unwrap_or(client_id);
                            if let Some(rlocked_sessions) = rlocked_sessions.as_ref() {
                                if let Action::SwitchToMode(input_mode) = action {
                                    let send_res = os_input.send_to_client(
                                        client_id,
                                        ServerToClientMsg::SwitchToMode(input_mode),
                                    );
                                    if send_res.is_err() {
                                        let _ = to_server
                                            .send(ServerInstruction::RemoveClient(client_id));
                                        return Ok(true);
                                    }
                                }
                                if route_action(
                                    action,
                                    client_id,
                                    maybe_pane_id.map(|p| PaneId::Terminal(p)),
                                    rlocked_sessions.senders.clone(),
                                    rlocked_sessions.capabilities.clone(),
                                    rlocked_sessions.client_attributes.clone(),
                                    rlocked_sessions.default_shell.clone(),
                                    rlocked_sessions.layout.clone(),
                                )? {
                                    should_break = true;
                                }
                            }
                        },
                        ClientToServerMsg::TerminalResize(new_size) => {
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
                        },
                        ClientToServerMsg::TerminalPixelDimensions(pixel_dimensions) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalPixelDimensions(pixel_dimensions),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::BackgroundColor(ref background_color_instruction) => {
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
                        ClientToServerMsg::ForegroundColor(ref foreground_color_instruction) => {
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
                        ClientToServerMsg::ColorRegisters(ref color_registers) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalColorRegisters(color_registers.clone()),
                                instruction,
                                retry_queue
                            )
                            .with_context(err_context)?;
                        },
                        ClientToServerMsg::NewClient(
                            client_attributes,
                            cli_args,
                            opts,
                            layout,
                            plugin_config,
                        ) => {
                            let new_client_instruction = ServerInstruction::NewClient(
                                client_attributes,
                                cli_args,
                                opts,
                                layout,
                                client_id,
                                plugin_config,
                            );
                            to_server
                                .send(new_client_instruction)
                                .with_context(err_context)?;
                        },
                        ClientToServerMsg::AttachClient(
                            client_attributes,
                            opts,
                            tab_position_to_focus,
                            pane_id_to_focus,
                        ) => {
                            let attach_client_instruction = ServerInstruction::AttachClient(
                                client_attributes,
                                opts,
                                tab_position_to_focus,
                                pane_id_to_focus,
                                client_id,
                            );
                            to_server
                                .send(attach_client_instruction)
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
                        ClientToServerMsg::DetachSession(client_id) => {
                            let _ = to_server.send(ServerInstruction::DetachSession(client_id));
                            should_break = true;
                        },
                        ClientToServerMsg::ListClients => {
                            let _ = to_server.send(ServerInstruction::ActiveClients(client_id));
                        },
                    }
                    Ok(should_break)
                };
                while let Some(instruction_to_retry) = retry_queue.pop_front() {
                    log::warn!("Server ready, retrying sending instruction.");
                    let should_break = handle_instruction(instruction_to_retry, None)?;
                    if should_break {
                        break 'route_loop;
                    }
                }
                let should_break = handle_instruction(instruction, Some(&mut retry_queue))?;
                if should_break {
                    break 'route_loop;
                }
            },
            Err(ReceiveError::Disconnected) => {
                log::info!("Client has disconnected or crashed");
                // Client is already gone so there is no use in sending a message to the client
                break 'route_loop;
            },
            Err(e) => {
                log::error!("Received empty message from client, logging client out.");
                let _ = os_input.send_to_client(
                    client_id,
                    ServerToClientMsg::Exit(ExitReason::Error(
                        "Received empty message".to_string(),
                    )),
                );
                let _ = to_server.send(ServerInstruction::RemoveClient(client_id));
                break 'route_loop;
            },
        }
    }
    log::info!("Terminating server_router");
    Ok(())
}
