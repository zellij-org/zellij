use std::sync::{Arc, RwLock};

use zellij_utils::zellij_tile::data::Event;

use crate::{
    os_input_output::ServerOsApi, pty::PtyInstruction, screen::ScreenInstruction,
    wasm_vm::PluginInstruction, ServerInstruction, SessionMetaData,
};
use zellij_utils::{
    channels::SenderWithContext,
    input::{
        actions::{Action, Direction},
        get_mode_info,
    },
    ipc::ClientToServerMsg,
};

fn route_action(
    action: Action,
    session: &SessionMetaData,
    os_input: &dyn ServerOsApi,
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
            let palette = os_input.load_palette();
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
        Action::Quit => {
            to_server.send(ServerInstruction::ClientExit).unwrap();
            should_break = true;
        }
        Action::Detach => {
            to_server.send(ServerInstruction::DetachSession).unwrap();
            should_break = true;
        }
        Action::NoOp => {}
    }
    should_break
}

pub(crate) fn route_thread_main(
    session_data: Arc<RwLock<Option<SessionMetaData>>>,
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
                os_input.add_client_sender();
                to_server.send(instruction.into()).unwrap();
            }
        }
    }
}
