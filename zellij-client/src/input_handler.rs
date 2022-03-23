//! Main input logic.

use zellij_utils::{
    input::{
        mouse::{MouseButton, MouseEvent},
        options::Options,
    },
    termwiz::input::InputEvent,
    zellij_tile,
};

use crate::{
    os_input_output::ClientOsApi, ClientInstruction, CommandIsExecuting, InputInstruction,
};
use zellij_utils::{
    channels::{Receiver, SenderWithContext, OPENCALLS},
    errors::{ContextType, ErrorContext},
    input::{actions::Action, cast_termwiz_key, config::Config, keybinds::Keybinds},
    ipc::{ClientToServerMsg, ExitReason},
};

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
    receive_input_instructions: Receiver<(InputInstruction, ErrorContext)>,
    holding_mouse: bool,
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
        receive_input_instructions: Receiver<(InputInstruction, ErrorContext)>,
    ) -> Self {
        InputHandler {
            mode,
            os_input,
            config,
            options,
            command_is_executing,
            send_client_instructions,
            should_exit: false,
            receive_input_instructions,
            holding_mouse: false,
        }
    }

    /// Main input event loop. Interprets the terminal Event
    /// as [`Action`]s according to the current [`InputMode`], and dispatches those actions.
    fn handle_input(&mut self) {
        let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
        err_ctx.add_call(ContextType::StdinHandler);
        let bracketed_paste_start = vec![27, 91, 50, 48, 48, 126]; // \u{1b}[200~
        let bracketed_paste_end = vec![27, 91, 50, 48, 49, 126]; // \u{1b}[201~
        if self.options.mouse_mode.unwrap_or(true) {
            self.os_input.enable_mouse();
        }
        loop {
            if self.should_exit {
                break;
            }
            match self.receive_input_instructions.recv() {
                Ok((InputInstruction::KeyEvent(input_event, raw_bytes), _error_context)) => {
                    match input_event {
                        InputEvent::Key(key_event) => {
                            let key = cast_termwiz_key(key_event, &raw_bytes);
                            self.handle_key(&key, raw_bytes);
                        }
                        InputEvent::Mouse(mouse_event) => {
                            let mouse_event =
                                zellij_utils::input::mouse::MouseEvent::from(mouse_event);
                            self.handle_mouse_event(&mouse_event);
                        }
                        InputEvent::Paste(pasted_text) => {
                            if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
                                self.dispatch_action(Action::Write(bracketed_paste_start.clone()));
                                self.dispatch_action(Action::Write(
                                    pasted_text.as_bytes().to_vec(),
                                ));
                                self.dispatch_action(Action::Write(bracketed_paste_end.clone()));
                            }
                        }
                        _ => {}
                    }
                }
                Ok((InputInstruction::SwitchToMode(input_mode), _error_context)) => {
                    self.mode = input_mode;
                }
                Err(err) => panic!("Encountered read error: {:?}", err),
            }
        }
    }
    fn handle_key(&mut self, key: &Key, raw_bytes: Vec<u8>) {
        let keybinds = &self.config.keybinds;
        for action in Keybinds::key_to_actions(key, raw_bytes, &self.mode, keybinds) {
            let should_exit = self.dispatch_action(action);
            if should_exit {
                self.should_exit = true;
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
                    if self.holding_mouse {
                        self.dispatch_action(Action::MouseHold(point));
                    } else {
                        self.dispatch_action(Action::LeftClick(point));
                    }
                    self.holding_mouse = true;
                }
                MouseButton::Right => {
                    if self.holding_mouse {
                        self.dispatch_action(Action::MouseHold(point));
                    } else {
                        self.dispatch_action(Action::RightClick(point));
                    }
                    self.holding_mouse = true;
                }
                _ => {}
            },
            MouseEvent::Release(point) => {
                self.dispatch_action(Action::MouseRelease(point));
                self.holding_mouse = false;
            }
            MouseEvent::Hold(point) => {
                self.dispatch_action(Action::MouseHold(point));
                self.holding_mouse = true;
            }
        }
    }
    fn handle_actions(&mut self, actions: Vec<Action>, session_name: &str) {
        // TODO: handle Detach correctly
        for action in actions {
            match action {
                Action::Quit => {
                    crate::sessions::kill_session(&session_name);
                    break;
                }
                Action::Detach => {
                    self.should_exit = true;
                    self.os_input
                        .send_to_server(ClientToServerMsg::DetachSession(1));
                    //self.os_input
                    //.send_to_server(ClientToServerMsg::Action(Action::Quit));
                    //std::process::exit(0);
                    //break;
                }
                _ => {
                    let should_exit = self.dispatch_action(action);
                    if should_exit {
                        break;
                    }
                }
            }
        }
        self.dispatch_action(Action::Quit);
        // is this correct? should be just for this current client
        self.should_exit = true;
        log::error!("Quitting Now. Dispatched the actions");
        std::process::exit(0);
        //self.dispatch_action(Action::NoOp);
        //self.exit();
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
            Action::NoOp => {}
            Action::Quit | Action::Detach => {
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action));
                self.exit();
                should_break = true;
            }
            Action::SwitchToMode(mode) => {
                // this is an optimistic update, we should get a SwitchMode instruction from the
                // server later that atomically changes the mode as well
                self.mode = mode;
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action));
            }
            Action::CloseFocus
            | Action::NewPane(_)
            | Action::ToggleFloatingPanes
            | Action::TogglePaneEmbedOrFloating
            | Action::NewTab(_)
            | Action::GoToNextTab
            | Action::GoToPreviousTab
            | Action::CloseTab
            | Action::GoToTab(_)
            | Action::ToggleTab
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
    receive_input_instructions: Receiver<(InputInstruction, ErrorContext)>,
) {
    let _handler = InputHandler::new(
        os_input,
        command_is_executing,
        config,
        options,
        send_client_instructions,
        default_mode,
        receive_input_instructions,
    )
    .handle_input();
}

/// Entry point to the module. Instantiates an [`InputHandler`] and starts
/// its [`InputHandler::handle_input()`] loop.
pub(crate) fn input_actions(
    os_input: Box<dyn ClientOsApi>,
    config: Config,
    options: Options,
    command_is_executing: CommandIsExecuting,
    send_client_instructions: SenderWithContext<ClientInstruction>,
    default_mode: InputMode,
    receive_input_instructions: Receiver<(InputInstruction, ErrorContext)>,
    actions: Vec<Action>,
    session_name: String,
) {
    let _handler = InputHandler::new(
        os_input,
        command_is_executing,
        config,
        options,
        send_client_instructions,
        default_mode,
        receive_input_instructions,
    )
    .handle_actions(actions, &session_name);
}

#[cfg(test)]
#[path = "./unit/input_handler_tests.rs"]
mod grid_tests;
