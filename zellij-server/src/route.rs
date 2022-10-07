use std::sync::{Arc, RwLock};

use crate::{
    os_input_output::ServerOsApi,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    wasm_vm::PluginInstruction,
    ServerInstruction, SessionMetaData, SessionState,
};
use zellij_utils::{
    channels::SenderWithContext,
    data::Event,
    input::{
        actions::{Action, Direction, ResizeDirection, SearchDirection, SearchOption},
        command::TerminalAction,
        get_mode_info,
    },
    ipc::{ClientToServerMsg, ExitReason, IpcReceiverWithContext, ServerToClientMsg},
};

use crate::ClientId;

pub(crate) fn route_action(
    action: Action,
    session: &SessionMetaData,
    _os_input: &dyn ServerOsApi,
    to_server: &SenderWithContext<ServerInstruction>,
    client_id: ClientId,
) -> bool {
    let mut should_break = false;

    // forward the action to plugins unless it is a mousehold
    // this is a bit of a hack around the unfortunate architecture we use with plugins
    // this will change as soon as we refactor
    match action {
        Action::MouseHoldLeft(..) | Action::MouseHoldRight(..) => {},
        _ => {
            session
                .senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    Some(client_id),
                    Event::InputReceived,
                ))
                .unwrap();
        },
    }

    match action {
        Action::ToggleTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleTab(client_id))
                .unwrap();
        },
        Action::Write(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .unwrap();
        },
        Action::WriteChars(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .unwrap();
            let val = val.into_bytes();
            session
                .senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .unwrap();
        },
        Action::SwitchToMode(mode) => {
            let attrs = &session.client_attributes;
            // TODO: use the palette from the client and remove it from the server os api
            // this is left here as a stop gap measure until we shift some code around
            // to allow for this
            // TODO: Need access to `ClientAttributes` here
            session
                .senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    Some(client_id),
                    Event::ModeUpdate(get_mode_info(mode, attrs, session.capabilities)),
                ))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::ChangeMode(
                    get_mode_info(mode, attrs, session.capabilities),
                    client_id,
                ))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::Render)
                .unwrap();
        },
        Action::Resize(direction) => {
            let screen_instr = match direction {
                ResizeDirection::Left => ScreenInstruction::ResizeLeft(client_id),
                ResizeDirection::Right => ScreenInstruction::ResizeRight(client_id),
                ResizeDirection::Up => ScreenInstruction::ResizeUp(client_id),
                ResizeDirection::Down => ScreenInstruction::ResizeDown(client_id),
                ResizeDirection::Increase => ScreenInstruction::ResizeIncrease(client_id),
                ResizeDirection::Decrease => ScreenInstruction::ResizeDecrease(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        },
        Action::SwitchFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchFocus(client_id))
                .unwrap();
        },
        Action::FocusNextPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusNextPane(client_id))
                .unwrap();
        },
        Action::FocusPreviousPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusPreviousPane(client_id))
                .unwrap();
        },
        Action::MoveFocus(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeft(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRight(client_id),
                Direction::Up => ScreenInstruction::MoveFocusUp(client_id),
                Direction::Down => ScreenInstruction::MoveFocusDown(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        },
        Action::MoveFocusOrTab(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRightOrNextTab(client_id),
                Direction::Up => ScreenInstruction::SwitchTabNext(client_id),
                Direction::Down => ScreenInstruction::SwitchTabPrev(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        },
        Action::MovePane(direction) => {
            let screen_instr = match direction {
                Some(Direction::Left) => ScreenInstruction::MovePaneLeft(client_id),
                Some(Direction::Right) => ScreenInstruction::MovePaneRight(client_id),
                Some(Direction::Up) => ScreenInstruction::MovePaneUp(client_id),
                Some(Direction::Down) => ScreenInstruction::MovePaneDown(client_id),
                None => ScreenInstruction::MovePane(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        },
        Action::DumpPane(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::DumpPane(val, client_id))
                .unwrap();
        },
        Action::DumpScreen(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::DumpScreen(val, client_id))
                .unwrap();
        },
        Action::EditScrollback => {
            session
                .senders
                .send_to_screen(ScreenInstruction::EditScrollback(client_id))
                .unwrap();
        },
        Action::ScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUp(client_id))
                .unwrap();
        },
        Action::ScrollUpAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUpAt(point, client_id))
                .unwrap();
        },
        Action::ScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDown(client_id))
                .unwrap();
        },
        Action::ScrollDownAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDownAt(point, client_id))
                .unwrap();
        },
        Action::ScrollToBottom => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollToBottom(client_id))
                .unwrap();
        },
        Action::PageScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollUp(client_id))
                .unwrap();
        },
        Action::PageScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollDown(client_id))
                .unwrap();
        },
        Action::HalfPageScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::HalfPageScrollUp(client_id))
                .unwrap();
        },
        Action::HalfPageScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::HalfPageScrollDown(client_id))
                .unwrap();
        },
        Action::ToggleFocusFullscreen => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveTerminalFullscreen(client_id))
                .unwrap();
        },
        Action::TogglePaneFrames => {
            session
                .senders
                .send_to_screen(ScreenInstruction::TogglePaneFrames)
                .unwrap();
        },
        Action::NewPane(direction) => {
            let shell = session.default_shell.clone();
            let pty_instr = match direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(shell, client_id),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(shell, client_id),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(shell, client_id),
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(shell, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(shell, ClientOrTabIndex::ClientId(client_id)),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        },
        Action::EditFile(path_to_file, line_number, split_direction, should_float) => {
            match should_float {
                Some(true) => {
                    session
                        .senders
                        .send_to_screen(ScreenInstruction::ShowFloatingPanes(client_id))
                        .unwrap();
                },
                Some(false) => {
                    session
                        .senders
                        .send_to_screen(ScreenInstruction::HideFloatingPanes(client_id))
                        .unwrap();
                },
                None => {},
            };

            let open_file = TerminalAction::OpenFile(path_to_file, line_number);
            let pty_instr = match (split_direction, should_float.unwrap_or(false)) {
                (Some(Direction::Left), false) => {
                    PtyInstruction::SpawnTerminalVertically(Some(open_file), client_id)
                },
                (Some(Direction::Right), false) => {
                    PtyInstruction::SpawnTerminalVertically(Some(open_file), client_id)
                },
                (Some(Direction::Up), false) => {
                    PtyInstruction::SpawnTerminalHorizontally(Some(open_file), client_id)
                },
                (Some(Direction::Down), false) => {
                    PtyInstruction::SpawnTerminalHorizontally(Some(open_file), client_id)
                },
                // No direction specified or should float - defer placement to screen
                (None, _) | (_, true) => PtyInstruction::SpawnTerminal(
                    Some(open_file),
                    ClientOrTabIndex::ClientId(client_id),
                ),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        },
        Action::SwitchModeForAllClients(input_mode) => {
            let attrs = &session.client_attributes;
            session
                .senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    None,
                    Event::ModeUpdate(get_mode_info(input_mode, attrs, session.capabilities)),
                ))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::ChangeModeForAllClients(get_mode_info(
                    input_mode,
                    attrs,
                    session.capabilities,
                )))
                .unwrap();
        },
        Action::NewFloatingPane(run_command) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ShowFloatingPanes(client_id))
                .unwrap();
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| session.default_shell.clone());
            session
                .senders
                .send_to_pty(PtyInstruction::SpawnTerminal(
                    run_cmd,
                    ClientOrTabIndex::ClientId(client_id),
                ))
                .unwrap();
        },
        Action::NewTiledPane(direction, run_command) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::HideFloatingPanes(client_id))
                .unwrap();
            let run_cmd = run_command
                .map(|cmd| TerminalAction::RunCommand(cmd.into()))
                .or_else(|| session.default_shell.clone());
            let pty_instr = match direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                },
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                },
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                },
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => {
                    PtyInstruction::SpawnTerminal(run_cmd, ClientOrTabIndex::ClientId(client_id))
                },
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        },
        Action::TogglePaneEmbedOrFloating => {
            session
                .senders
                .send_to_screen(ScreenInstruction::TogglePaneEmbedOrFloating(client_id))
                .unwrap();
        },
        Action::ToggleFloatingPanes => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleFloatingPanes(
                    client_id,
                    session.default_shell.clone(),
                ))
                .unwrap();
        },
        Action::PaneNameInput(c) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UpdatePaneName(c, client_id))
                .unwrap();
        },
        Action::UndoRenamePane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UndoRenamePane(client_id))
                .unwrap();
        },
        Action::Run(command) => {
            let run_cmd = Some(TerminalAction::RunCommand(command.clone().into()));
            let pty_instr = match command.direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                },
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                },
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                },
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                },
                // No direction specified - try to put it in the biggest available spot
                None => {
                    PtyInstruction::SpawnTerminal(run_cmd, ClientOrTabIndex::ClientId(client_id))
                },
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        },
        Action::CloseFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane(client_id))
                .unwrap();
        },
        Action::NewTab(tab_layout, tab_name) => {
            let shell = session.default_shell.clone();
            session
                .senders
                .send_to_pty(PtyInstruction::NewTab(
                    shell, tab_layout, tab_name, client_id,
                ))
                .unwrap();
        },
        Action::GoToNextTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabNext(client_id))
                .unwrap();
        },
        Action::GoToPreviousTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabPrev(client_id))
                .unwrap();
        },
        Action::ToggleActiveSyncTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveSyncTab(client_id))
                .unwrap();
        },
        Action::CloseTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseTab(client_id))
                .unwrap();
        },
        Action::GoToTab(i) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::GoToTab(i, Some(client_id)))
                .unwrap();
        },
        Action::TabNameInput(c) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UpdateTabName(c, client_id))
                .unwrap();
        },
        Action::UndoRenameTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UndoRenameTab(client_id))
                .unwrap();
        },
        Action::Quit => {
            to_server
                .send(ServerInstruction::ClientExit(client_id))
                .unwrap();
            should_break = true;
        },
        Action::Detach => {
            to_server
                .send(ServerInstruction::DetachSession(vec![client_id]))
                .unwrap();
            should_break = true;
        },
        Action::LeftClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::LeftClick(point, client_id))
                .unwrap();
        },
        Action::RightClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::RightClick(point, client_id))
                .unwrap();
        },
        Action::MiddleClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MiddleClick(point, client_id))
                .unwrap();
        },
        Action::LeftMouseRelease(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::LeftMouseRelease(point, client_id))
                .unwrap();
        },
        Action::RightMouseRelease(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::RightMouseRelease(point, client_id))
                .unwrap();
        },
        Action::MiddleMouseRelease(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MiddleMouseRelease(point, client_id))
                .unwrap();
        },
        Action::MouseHoldLeft(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseHoldLeft(point, client_id))
                .unwrap();
        },
        Action::MouseHoldRight(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseHoldRight(point, client_id))
                .unwrap();
        },
        Action::MouseHoldMiddle(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseHoldMiddle(point, client_id))
                .unwrap();
        },
        Action::Copy => {
            session
                .senders
                .send_to_screen(ScreenInstruction::Copy(client_id))
                .unwrap();
        },
        Action::Confirm => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ConfirmPrompt(client_id))
                .unwrap();
        },
        Action::Deny => {
            session
                .senders
                .send_to_screen(ScreenInstruction::DenyPrompt(client_id))
                .unwrap();
        },
        #[allow(clippy::single_match)]
        Action::SkipConfirm(action) => match *action {
            Action::Quit => {
                to_server
                    .send(ServerInstruction::ClientExit(client_id))
                    .unwrap();
                should_break = true;
            },
            _ => {},
        },
        Action::NoOp => {},
        Action::SearchInput(c) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UpdateSearch(c, client_id))
                .unwrap();
        },
        Action::Search(d) => {
            let instruction = match d {
                SearchDirection::Down => ScreenInstruction::SearchDown(client_id),
                SearchDirection::Up => ScreenInstruction::SearchUp(client_id),
            };
            session.senders.send_to_screen(instruction).unwrap();
        },
        Action::SearchToggleOption(o) => {
            let instruction = match o {
                SearchOption::CaseSensitivity => {
                    ScreenInstruction::SearchToggleCaseSensitivity(client_id)
                },
                SearchOption::WholeWord => ScreenInstruction::SearchToggleWholeWord(client_id),
                SearchOption::Wrap => ScreenInstruction::SearchToggleWrap(client_id),
            };
            session.senders.send_to_screen(instruction).unwrap();
        },
    }
    should_break
}

// this should only be used for one-off startup instructions
macro_rules! send_to_screen_or_retry_queue {
    ($rlocked_sessions:expr, $message:expr, $instruction: expr, $retry_queue:expr) => {{
        match $rlocked_sessions.as_ref() {
            Some(session_metadata) => {
                session_metadata.senders.send_to_screen($message).unwrap();
            },
            None => {
                log::warn!("Server not ready, trying to place instruction in retry queue...");
                if let Some(retry_queue) = $retry_queue.as_mut() {
                    retry_queue.push($instruction);
                }
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
) {
    let mut retry_queue = vec![];
    'route_loop: loop {
        match receiver.recv() {
            Some((instruction, err_ctx)) => {
                err_ctx.update_thread_ctx();
                let rlocked_sessions = session_data.read().unwrap();
                let handle_instruction = |instruction: ClientToServerMsg,
                                          mut retry_queue: Option<&mut Vec<ClientToServerMsg>>|
                 -> bool {
                    let mut should_break = false;
                    match instruction {
                        ClientToServerMsg::Action(action, maybe_client_id) => {
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
                                        return true;
                                    }
                                }
                                if route_action(
                                    action,
                                    rlocked_sessions,
                                    &*os_input,
                                    &to_server,
                                    client_id,
                                ) {
                                    should_break = true;
                                }
                            }
                        },
                        ClientToServerMsg::TerminalResize(new_size) => {
                            session_state
                                .write()
                                .unwrap()
                                .set_client_size(client_id, new_size);
                            let min_size = session_state
                                .read()
                                .unwrap()
                                .min_client_terminal_size()
                                .unwrap();
                            rlocked_sessions
                                .as_ref()
                                .unwrap()
                                .senders
                                .send_to_screen(ScreenInstruction::TerminalResize(min_size))
                                .unwrap();
                        },
                        ClientToServerMsg::TerminalPixelDimensions(pixel_dimensions) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalPixelDimensions(pixel_dimensions),
                                instruction,
                                retry_queue
                            );
                        },
                        ClientToServerMsg::BackgroundColor(ref background_color_instruction) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalBackgroundColor(
                                    background_color_instruction.clone()
                                ),
                                instruction,
                                retry_queue
                            );
                        },
                        ClientToServerMsg::ForegroundColor(ref foreground_color_instruction) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalForegroundColor(
                                    foreground_color_instruction.clone()
                                ),
                                instruction,
                                retry_queue
                            );
                        },
                        ClientToServerMsg::ColorRegisters(ref color_registers) => {
                            send_to_screen_or_retry_queue!(
                                rlocked_sessions,
                                ScreenInstruction::TerminalColorRegisters(color_registers.clone()),
                                instruction,
                                retry_queue
                            );
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
                            to_server.send(new_client_instruction).unwrap();
                        },
                        ClientToServerMsg::AttachClient(client_attributes, opts) => {
                            let attach_client_instruction =
                                ServerInstruction::AttachClient(client_attributes, opts, client_id);
                            to_server.send(attach_client_instruction).unwrap();
                        },
                        ClientToServerMsg::ClientExited => {
                            // we don't unwrap this because we don't really care if there's an error here (eg.
                            // if the main server thread exited before this router thread did)
                            let _ = to_server.send(ServerInstruction::RemoveClient(client_id));
                            return true;
                        },
                        ClientToServerMsg::KillSession => {
                            to_server.send(ServerInstruction::KillSession).unwrap();
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
                    should_break
                };
                for instruction_to_retry in retry_queue.drain(..) {
                    log::warn!("Server ready, retrying sending instruction.");
                    let should_break = handle_instruction(instruction_to_retry, None);
                    if should_break {
                        break 'route_loop;
                    }
                }
                let should_break = handle_instruction(instruction, Some(&mut retry_queue));
                if should_break {
                    break 'route_loop;
                }
            },
            None => {
                log::error!("Received empty message from client");
                let _ = os_input.send_to_client(
                    client_id,
                    ServerToClientMsg::Exit(ExitReason::Error(
                        "Received empty message".to_string(),
                    )),
                );
                break;
            },
        }
    }
}
