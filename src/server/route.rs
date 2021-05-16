use std::sync::{Arc, RwLock};

use zellij_tile::data::Event;

use crate::common::input::actions::{Action, Direction};
use crate::common::input::handler::get_mode_info;
use crate::common::ipc::ClientToServerMsg;
use crate::common::os_input_output::ServerOsApi;
use crate::common::pty::PtyInstruction;
use crate::common::screen::ScreenInstruction;
use crate::common::thread_bus::SenderWithContext;
use crate::common::wasm_vm::PluginInstruction;
use crate::server::{ServerInstruction, SessionMetaData};

fn route_action(action: Action, session: &SessionMetaData, os_input: &dyn ServerOsApi) {
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
            let palette = os_input.load_palette();
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
        Action::ScrollDown => {
            session
                .senders
                .send_to_screen(ScreenInstruction::ScrollDown)
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
            let pty_instr = match direction {
                Some(Direction::Left) => PtyInstruction::SpawnTerminalVertically(None),
                Some(Direction::Right) => PtyInstruction::SpawnTerminalVertically(None),
                Some(Direction::Up) => PtyInstruction::SpawnTerminalHorizontally(None),
                Some(Direction::Down) => PtyInstruction::SpawnTerminalHorizontally(None),
                // No direction specified - try to put it in the biggest available spot
                None => PtyInstruction::SpawnTerminal(None),
            };
            session.senders.send_to_pty(pty_instr).unwrap();
        }
        Action::CloseFocus => {
            session
                .senders
                .send_to_screen(ScreenInstruction::CloseFocusedPane)
                .unwrap();
        }
        Action::NewTab => {
            session.senders.send_to_pty(PtyInstruction::NewTab).unwrap();
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
        Action::NoOp => {}
        Action::Quit => panic!("Received unexpected action"),
    }
}

pub fn route_thread_main(
    sessions: Arc<RwLock<Option<SessionMetaData>>>,
    mut os_input: Box<dyn ServerOsApi>,
    to_server: SenderWithContext<ServerInstruction>,
) {
    loop {
        let (instruction, err_ctx) = os_input.recv_from_client();
        err_ctx.update_thread_ctx();
        let rlocked_sessions = sessions.read().unwrap();
        match instruction {
            ClientToServerMsg::ClientExit => {
                to_server.send(instruction.into()).unwrap();
                break;
            }
            ClientToServerMsg::Action(action) => {
                route_action(action, rlocked_sessions.as_ref().unwrap(), &*os_input);
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
                os_input.add_client_sender();
                to_server.send(instruction.into()).unwrap();
            }
        }
    }
}
