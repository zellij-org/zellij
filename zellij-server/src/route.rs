use std::sync::{Arc, RwLock};

use zellij_utils::zellij_tile::data::Event;

use crate::{
    os_input_output::ServerOsApi,
    pty::{ClientOrTabIndex, PtyInstruction},
    screen::ScreenInstruction,
    wasm_vm::PluginInstruction,
    ServerInstruction, SessionMetaData, SessionState,
};
use zellij_utils::{
    channels::SenderWithContext,
    input::{
        actions::{Action, Direction},
        command::TerminalAction,
        get_mode_info,
    },
    ipc::{ClientToServerMsg, IpcReceiverWithContext, ServerToClientMsg},
};

use crate::ClientId;

fn route_action(
    action: Action,
    session: &SessionMetaData,
    _os_input: &dyn ServerOsApi,
    to_server: &SenderWithContext<ServerInstruction>,
    client_id: ClientId,
) -> bool {
    let mut should_break = false;
    session
        .senders
        .send_to_plugin(PluginInstruction::Update(None, Event::InputReceived))
        .unwrap();
    match action {
        Action::ToggleTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleTab(client_id))
                .unwrap();
        }
        Action::Write(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .unwrap();
        }
        Action::WriteChars(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ClearScroll(client_id))
                .unwrap();
            let val = Vec::from(val.as_bytes());
            session
                .senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val, client_id))
                .unwrap();
        }
        Action::SwitchToMode(mode) => {
            let palette = session.palette;
            // TODO: use the palette from the client and remove it from the server os api
            // this is left here as a stop gap measure until we shift some code around
            // to allow for this
            session
                .senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    Event::ModeUpdate(get_mode_info(mode, palette, session.capabilities)),
                ))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::ChangeMode(
                    get_mode_info(mode, palette, session.capabilities),
                    client_id,
                ))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::Render)
                .unwrap();
        }
        Action::Resize(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::ResizeLeft(client_id),
                Direction::Right => ScreenInstruction::ResizeRight(client_id),
                Direction::Up => ScreenInstruction::ResizeUp(client_id),
                Direction::Down => ScreenInstruction::ResizeDown(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::SwitchFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchFocus(client_id))
                .unwrap();
        }
        Action::FocusNextPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusNextPane(client_id))
                .unwrap();
        }
        Action::FocusPreviousPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusPreviousPane(client_id))
                .unwrap();
        }
        Action::MoveFocus(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeft(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRight(client_id),
                Direction::Up => ScreenInstruction::MoveFocusUp(client_id),
                Direction::Down => ScreenInstruction::MoveFocusDown(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::MoveFocusOrTab(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id),
                Direction::Right => ScreenInstruction::MoveFocusRightOrNextTab(client_id),
                _ => unreachable!(),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::MovePane(direction) => {
            let screen_instr = match direction {
                Some(Direction::Left) => ScreenInstruction::MovePaneLeft(client_id),
                Some(Direction::Right) => ScreenInstruction::MovePaneRight(client_id),
                Some(Direction::Up) => ScreenInstruction::MovePaneUp(client_id),
                Some(Direction::Down) => ScreenInstruction::MovePaneDown(client_id),
                None => ScreenInstruction::MovePane(client_id),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::ScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUp(client_id))
                .unwrap();
        }
        Action::ScrollUpAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUpAt(point, client_id))
                .unwrap();
        }
        Action::ScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDown(client_id))
                .unwrap();
        }
        Action::ScrollDownAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDownAt(point, client_id))
                .unwrap();
        }
        Action::ScrollToBottom => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollToBottom(client_id))
                .unwrap();
        }
        Action::PageScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollUp(client_id))
                .unwrap();
        }
        Action::PageScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollDown(client_id))
                .unwrap();
        }
        Action::ToggleFocusFullscreen => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveTerminalFullscreen(client_id))
                .unwrap();
        }
        Action::TogglePaneFrames => {
            session
                .senders
                .send_to_screen(ScreenInstruction::TogglePaneFrames)
                .unwrap();
        }
        Action::NewPane(direction) => {
            let shell = session.default_shell.clone();
            let pty_instr = match direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(shell, client_id),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(shell, client_id),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(shell, client_id),
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(shell, client_id)
                }
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(shell, ClientOrTabIndex::ClientId(client_id)),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        }
        Action::Run(command) => {
            let run_cmd = Some(TerminalAction::RunCommand(command.clone().into()));
            let pty_instr = match command.direction {
                Some(Direction::Left) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                }
                Some(Direction::Right) => {
                    PtyInstruction::SpawnTerminalVertically(run_cmd, client_id)
                }
                Some(Direction::Up) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                }
                Some(Direction::Down) => {
                    PtyInstruction::SpawnTerminalHorizontally(run_cmd, client_id)
                }
                // No direction specified - try to put it in the biggest available spot
                None => {
                    PtyInstruction::SpawnTerminal(run_cmd, ClientOrTabIndex::ClientId(client_id))
                }
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        }
        Action::CloseFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane(client_id))
                .unwrap();
        }
        Action::NewTab(tab_layout) => {
            let shell = session.default_shell.clone();
            session
                .senders
                .send_to_pty(PtyInstruction::NewTab(shell, tab_layout, client_id))
                .unwrap();
        }
        Action::GoToNextTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabNext(client_id))
                .unwrap();
        }
        Action::GoToPreviousTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabPrev(client_id))
                .unwrap();
        }
        Action::ToggleActiveSyncTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveSyncTab(client_id))
                .unwrap();
        }
        Action::CloseTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseTab(client_id))
                .unwrap();
        }
        Action::GoToTab(i) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::GoToTab(i, Some(client_id)))
                .unwrap();
        }
        Action::TabNameInput(c) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UpdateTabName(c, client_id))
                .unwrap();
        }
        Action::Quit => {
            to_server
                .send(ServerInstruction::ClientExit(client_id))
                .unwrap();
            should_break = true;
        }
        Action::Detach => {
            to_server
                .send(ServerInstruction::DetachSession(client_id))
                .unwrap();
            should_break = true;
        }
        Action::LeftClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::LeftClick(point, client_id))
                .unwrap();
        }
        Action::RightClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::RightClick(point, client_id))
                .unwrap();
        }

        Action::MouseRelease(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseRelease(point, client_id))
                .unwrap();
        }
        Action::MouseHold(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseHold(point, client_id))
                .unwrap();
        }
        Action::Copy => {
            session
                .senders
                .send_to_screen(ScreenInstruction::Copy(client_id))
                .unwrap();
        }
        Action::NoOp => {}
    }
    should_break
}

pub(crate) fn route_thread_main(
    session_data: Arc<RwLock<Option<SessionMetaData>>>,
    session_state: Arc<RwLock<SessionState>>,
    os_input: Box<dyn ServerOsApi>,
    to_server: SenderWithContext<ServerInstruction>,
    mut receiver: IpcReceiverWithContext<ClientToServerMsg>,
    client_id: ClientId,
) {
    loop {
        let (instruction, err_ctx) = receiver.recv();
        err_ctx.update_thread_ctx();
        let rlocked_sessions = session_data.read().unwrap();

        match instruction {
            ClientToServerMsg::Action(action) => {
                if let Some(rlocked_sessions) = rlocked_sessions.as_ref() {
                    if let Action::SwitchToMode(input_mode) = action {
                        for client_id in session_state.read().unwrap().clients.keys() {
                            os_input.send_to_client(
                                *client_id,
                                ServerToClientMsg::SwitchToMode(input_mode),
                            );
                        }
                    }
                    if route_action(action, rlocked_sessions, &*os_input, &to_server, client_id) {
                        break;
                    }
                }
            }
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
            }
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
            }
            ClientToServerMsg::AttachClient(client_attributes, opts) => {
                let attach_client_instruction =
                    ServerInstruction::AttachClient(client_attributes, opts, client_id);
                to_server.send(attach_client_instruction).unwrap();
            }
            ClientToServerMsg::ClientExited => {
                // we don't unwrap this because we don't really care if there's an error here (eg.
                // if the main server thread exited before this router thread did)
                let _ = to_server.send(ServerInstruction::RemoveClient(client_id));
                break;
            }
            ClientToServerMsg::KillSession => {
                to_server.send(ServerInstruction::KillSession).unwrap();
            }
        }
    }
}
