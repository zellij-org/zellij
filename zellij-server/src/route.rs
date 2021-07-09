use std::sync::{Arc, RwLock};

use zellij_utils::zellij_tile::data::Event;

use crate::{
    os_input_output::ServerOsApi, pty::PtyInstruction, screen::ScreenInstruction,
    wasm_vm::PluginInstruction, ServerInstruction, SessionMetaData, SessionState,
};
use zellij_utils::{
    channels::SenderWithContext,
    input::{
        actions::{Action, Direction},
        command::TerminalAction,
        get_mode_info,
    },
    ipc::{ClientToServerMsg, ExitReason, ServerToClientMsg},
};

fn route_action(
    action: Action,
    session: &SessionMetaData,
    _os_input: &dyn ServerOsApi,
    to_server: &SenderWithContext<ServerInstruction>,
) -> bool {
    let mut should_break = false;
    match action {
        Action::Write(val) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ClearScroll)
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::WriteCharacter(val))
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
                .send_to_screen(ScreenInstruction::ChangeMode(get_mode_info(
                    mode,
                    palette,
                    session.capabilities,
                )))
                .unwrap();
            session
                .senders
                .send_to_screen(ScreenInstruction::Render)
                .unwrap();
        }
        Action::Resize(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::ResizeLeft,
                Direction::Right => ScreenInstruction::ResizeRight,
                Direction::Up => ScreenInstruction::ResizeUp,
                Direction::Down => ScreenInstruction::ResizeDown,
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::SwitchFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchFocus)
                .unwrap();
        }
        Action::FocusNextPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusNextPane)
                .unwrap();
        }
        Action::FocusPreviousPane => {
            session
                .senders
                .send_to_screen(ScreenInstruction::FocusPreviousPane)
                .unwrap();
        }
        Action::MoveFocus(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeft,
                Direction::Right => ScreenInstruction::MoveFocusRight,
                Direction::Up => ScreenInstruction::MoveFocusUp,
                Direction::Down => ScreenInstruction::MoveFocusDown,
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::MoveFocusOrTab(direction) => {
            let screen_instr = match direction {
                Direction::Left => ScreenInstruction::MoveFocusLeftOrPreviousTab,
                Direction::Right => ScreenInstruction::MoveFocusRightOrNextTab,
                _ => unreachable!(),
            };
            session.senders.send_to_screen(screen_instr).unwrap();
        }
        Action::ScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUp)
                .unwrap();
        }
        Action::ScrollUpAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollUpAt(point))
                .unwrap();
        }
        Action::ScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDown)
                .unwrap();
        }
        Action::ScrollDownAt(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDownAt(point))
                .unwrap();
        }
        Action::PageScrollUp => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollUp)
                .unwrap();
        }
        Action::PageScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::PageScrollDown)
                .unwrap();
        }
        Action::ToggleFocusFullscreen => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveTerminalFullscreen)
                .unwrap();
        }
        Action::NewPane(direction) => {
            let shell = session.default_shell.clone();
            let pty_instr = match direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(shell),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(shell),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(shell),
                Some(Direction::Down) => PtyInstruction::SpawnTerminalHorizontally(shell),
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(shell),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        }
        Action::Run(command) => {
            let run_cmd = Some(TerminalAction::RunCommand(command.clone().into()));
            let pty_instr = match command.direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(run_cmd),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(run_cmd),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(run_cmd),
                Some(Direction::Down) => PtyInstruction::SpawnTerminalHorizontally(run_cmd),
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(run_cmd),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        }
        Action::CloseFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane)
                .unwrap();
        }
        Action::NewTab(tab_layout) => {
            let shell = session.default_shell.clone();
            session
                .senders
                .send_to_pty(PtyInstruction::NewTab(shell, tab_layout))
                .unwrap();
        }
        Action::GoToNextTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabNext)
                .unwrap();
        }
        Action::GoToPreviousTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::SwitchTabPrev)
                .unwrap();
        }
        Action::ToggleActiveSyncTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ToggleActiveSyncTab)
                .unwrap();
        }
        Action::CloseTab => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseTab)
                .unwrap();
        }
        Action::GoToTab(i) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::GoToTab(i))
                .unwrap();
        }
        Action::TabNameInput(c) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::UpdateTabName(c))
                .unwrap();
        }
        Action::Quit => {
            to_server.send(ServerInstruction::ClientExit).unwrap();
            should_break = true;
        }
        Action::Detach => {
            to_server.send(ServerInstruction::DetachSession).unwrap();
            should_break = true;
        }
        Action::LeftClick(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::LeftClick(point))
                .unwrap();
        }
        Action::MouseRelease(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseRelease(point))
                .unwrap();
        }
        Action::MouseHold(point) => {
            session
                .senders
                .send_to_screen(ScreenInstruction::MouseHold(point))
                .unwrap();
        }
        Action::Copy => {
            session
                .senders
                .send_to_screen(ScreenInstruction::Copy)
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
) {
    loop {
        let (instruction, err_ctx) = os_input.recv_from_client();
        err_ctx.update_thread_ctx();
        let rlocked_sessions = session_data.read().unwrap();

        match instruction {
            ClientToServerMsg::Action(action) => {
                if let Some(rlocked_sessions) = rlocked_sessions.as_ref() {
                    if route_action(action, rlocked_sessions, &*os_input, &to_server) {
                        break;
                    }
                }
            }
            ClientToServerMsg::TerminalResize(new_size) => {
                rlocked_sessions
                    .as_ref()
                    .unwrap()
                    .senders
                    .send_to_screen(ScreenInstruction::TerminalResize(new_size))
                    .unwrap();
            }
            ClientToServerMsg::NewClient(..) => {
                if *session_state.read().unwrap() != SessionState::Uninitialized {
                    os_input.send_to_temp_client(ServerToClientMsg::Exit(ExitReason::Error(
                        "Cannot add new client".into(),
                    )));
                } else {
                    os_input.add_client_sender();
                    to_server.send(instruction.into()).unwrap();
                }
            }
            ClientToServerMsg::AttachClient(_, force, _) => {
                if *session_state.read().unwrap() == SessionState::Attached && !force {
                    os_input.send_to_temp_client(ServerToClientMsg::Exit(ExitReason::CannotAttach));
                } else {
                    os_input.add_client_sender();
                    to_server.send(instruction.into()).unwrap();
                }
            }
            ClientToServerMsg::ClientExited => break,
        }
    }
}
