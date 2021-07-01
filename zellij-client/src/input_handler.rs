//! Main input logic.

use zellij_utils::{
    input::{
        mouse::{MouseButton, MouseEvent},
        options::Options,
    },
    termion, zellij_tile,
};

use crate::{os_input_output::ClientOsApi, ClientInstruction, CommandIsExecuting};
use zellij_utils::{
    channels::{SenderWithContext, OPENCALLS},
    errors::ContextType,
    input::{actions::Action, cast_termion_key, config::Config, keybinds::Keybinds},
    ipc::{ClientToServerMsg, ExitReason},
};

use termion::input::TermReadEventsAndRaw;
use zellij_tile::data::{InputMode, Key};

/// Handles the dispatching of [`Action`]s according to the current
/// [`InputMode`], and keep tracks of the current [`InputMode`].
struct InputHandler {
    /// The current input mode
    mode: InputMode,
    os_input: Box<dyn ClientOsApi>,
    config: Config,
    options: Options,
    command_is_executing: CommandIsExecuting,
    send_client_instructions: SenderWithContext<ClientInstruction>,
    should_exit: bool,
    pasting: bool,
}

impl InputHandler {
    /// Returns a new [`InputHandler`] with the attributes specified as arguments.
    fn new(
        os_input: Box<dyn ClientOsApi>,
        command_is_executing: CommandIsExecuting,
        config: Config,
        options: Options,
        send_client_instructions: SenderWithContext<ClientInstruction>,
        mode: InputMode,
    ) -> Self {
        InputHandler {
            mode,
            os_input,
            config,
            options,
            command_is_executing,
            send_client_instructions,
            should_exit: false,
            pasting: false,
        }
    }

    /// Main input event loop. Interprets the terminal [`Event`](termion::event::Event)s
    /// as [`Action`]s according to the current [`InputMode`], and dispatches those actions.
    fn handle_input(&mut self) {
        let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
        err_ctx.add_call(ContextType::StdinHandler);
        let alt_left_bracket = vec![27, 91];
        let bracketed_paste_start = vec![27, 91, 50, 48, 48, 126]; // \u{1b}[200~
        let bracketed_paste_end = vec![27, 91, 50, 48, 49, 126]; // \u{1b}[201

        if !self.options.disable_mouse_mode {
            self.os_input.enable_mouse();
        }
        loop {
            if self.should_exit {
                break;
            }
            let stdin_buffer = self.os_input.read_from_stdin();
            for key_result in stdin_buffer.events_and_raw() {
                match key_result {
                    Ok((event, raw_bytes)) => match event {
                        termion::event::Event::Key(key) => {
                            let key = cast_termion_key(key);
                            self.handle_key(&key, raw_bytes);
                        }
                        termion::event::Event::Mouse(me) => {
                            let mouse_event = zellij_utils::input::mouse::MouseEvent::from(me);
                            self.handle_mouse_event(&mouse_event);
                        }
                        termion::event::Event::Unsupported(unsupported_key) => {
                            // we have to do this because of a bug in termion
                            // this should be a key event and not an unsupported event
                            if unsupported_key == alt_left_bracket {
                                let key = Key::Alt('[');
                                self.handle_key(&key, raw_bytes);
                            } else if unsupported_key == bracketed_paste_start {
                                self.pasting = true;
                            } else if unsupported_key == bracketed_paste_end {
                                self.pasting = false;
                            } else {
                                // this is a hack because termion doesn't recognize certain keys
                                // in this case we just forward it to the terminal
                                self.handle_unknown_key(raw_bytes);
                            }
                        }
                    },
                    Err(err) => panic!("Encountered read error: {:?}", err),
                }
            }
        }
    }
    fn handle_unknown_key(&mut self, raw_bytes: Vec<u8>) {
        if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
            let action = Action::Write(raw_bytes);
            self.dispatch_action(action);
        }
    }
    fn handle_key(&mut self, key: &Key, raw_bytes: Vec<u8>) {
        let keybinds = &self.config.keybinds;
        if self.pasting {
            // we're inside a paste block, if we're in a mode that allows sending text to the
            // terminal, send all text directly without interpreting it
            // otherwise, just discard the input
            if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
                let action = Action::Write(raw_bytes);
                self.dispatch_action(action);
            }
        } else {
            for action in Keybinds::key_to_actions(key, raw_bytes, &self.mode, keybinds) {
                let should_exit = self.dispatch_action(action);
                if should_exit {
                    self.should_exit = true;
                }
            }
        }
    }
    fn handle_mouse_event(&mut self, mouse_event: &MouseEvent) {
        match *mouse_event {
            MouseEvent::Press(button, point) => match button {
                MouseButton::WheelUp => {
                    self.dispatch_action(Action::ScrollUpAt(point));
                }
                MouseButton::WheelDown => {
                    self.dispatch_action(Action::ScrollDownAt(point));
                }
                MouseButton::Left => {
                    self.dispatch_action(Action::LeftClick(point));
                }
                _ => {}
            },
            MouseEvent::Release(point) => {
                self.dispatch_action(Action::MouseRelease(point));
            }
            MouseEvent::Hold(point) => {
                self.dispatch_action(Action::MouseHold(point));
                self.os_input
                    .start_action_repeater(Action::MouseHold(point));
            }
        }
    }

    /// Dispatches an [`Action`].
    ///
    /// This function's body dictates what each [`Action`] actually does when
    /// dispatched.
    ///
    /// # Return value
    /// Currently, this function returns a boolean that indicates whether
    /// [`Self::handle_input()`] should break after this action is dispatched.
    /// This is a temporary measure that is only necessary due to the way that the
    /// framework works, and shouldn't be necessary anymore once the test framework
    /// is revised. See [issue#183](https://github.com/zellij-org/zellij/issues/183).
    fn dispatch_action(&mut self, action: Action) -> bool {
        let mut should_break = false;

        match action {
            Action::Quit | Action::Detach => {
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action));
                self.exit();
                should_break = true;
            }
            Action::SwitchToMode(mode) => {
                self.mode = mode;
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action));
            }
            Action::CloseFocus
            | Action::NewPane(_)
            | Action::NewTab
            | Action::GoToNextTab
            | Action::GoToPreviousTab
            | Action::CloseTab
            | Action::GoToTab(_)
            | Action::MoveFocusOrTab(_) => {
                self.command_is_executing.blocking_input_thread();
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action));
                self.command_is_executing
                    .wait_until_input_thread_is_unblocked();
            }
            _ => self
                .os_input
                .send_to_server(ClientToServerMsg::Action(action)),
        }

        should_break
    }

    /// Routine to be called when the input handler exits (at the moment this is the
    /// same as quitting Zellij).
    fn exit(&mut self) {
        self.send_client_instructions
            .send(ClientInstruction::Exit(ExitReason::Normal))
            .unwrap();
    }
}

/// Entry point to the module. Instantiates an [`InputHandler`] and starts
/// its [`InputHandler::handle_input()`] loop.
pub(crate) fn input_loop(
    os_input: Box<dyn ClientOsApi>,
    config: Config,
    options: Options,
    command_is_executing: CommandIsExecuting,
    send_client_instructions: SenderWithContext<ClientInstruction>,
    default_mode: InputMode,
) {
    let _handler = InputHandler::new(
        os_input,
        command_is_executing,
        config,
        options,
        send_client_instructions,
        default_mode,
    )
    .handle_input();
}

#[cfg(test)]
#[path = "./unit/input_handler_tests.rs"]
mod grid_tests;
