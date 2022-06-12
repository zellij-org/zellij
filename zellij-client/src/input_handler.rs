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
    os_input_output::ClientOsApi,
    stdin_ansi_parser::{AnsiStdinInstructionOrKeys, StdinAnsiParser},
    ClientId, ClientInstruction, CommandIsExecuting, InputInstruction,
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
        // <ESC>[14t => get text area size in pixels,
        // <ESC>[16t => get character cell size in pixels
        // <ESC>]11;?<ESC>\ => get background color
        // <ESC>]10;?<ESC>\ => get foreground color
        let get_cell_pixel_info =
            "\u{1b}[14t\u{1b}[16t\u{1b}]11;?\u{1b}\u{5c}\u{1b}]10;?\u{1b}\u{5c}";
        let _ = self
            .os_input
            .get_stdout_writer()
            .write(get_cell_pixel_info.as_bytes())
            .unwrap();
        let mut ansi_stdin_parser = StdinAnsiParser::new();
        ansi_stdin_parser.increment_expected_ansi_instructions(4);
        loop {
            if self.should_exit {
                break;
            }
            match self.receive_input_instructions.recv() {
                Ok((InputInstruction::KeyEvent(input_event, raw_bytes), _error_context)) => {
                    match input_event {
                        InputEvent::Key(key_event) => {
                            let key = cast_termwiz_key(key_event, &raw_bytes);
                            if ansi_stdin_parser.expected_instructions() > 0 {
                                self.handle_possible_pixel_instruction(
                                    ansi_stdin_parser.parse(key, raw_bytes),
                                );
                            } else {
                                self.handle_key(&key, raw_bytes);
                            }
                        },
                        InputEvent::Mouse(mouse_event) => {
                            let mouse_event =
                                zellij_utils::input::mouse::MouseEvent::from(mouse_event);
                            self.handle_mouse_event(&mouse_event);
                        },
                        InputEvent::Paste(pasted_text) => {
                            if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
                                self.dispatch_action(
                                    Action::Write(bracketed_paste_start.clone()),
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write(pasted_text.as_bytes().to_vec()),
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write(bracketed_paste_end.clone()),
                                    None,
                                );
                            }
                        },
                        _ => {},
                    }
                },
                Ok((InputInstruction::SwitchToMode(input_mode), _error_context)) => {
                    self.mode = input_mode;
                },
                Ok((InputInstruction::PossiblePixelRatioChange, _error_context)) => {
                    let _ = self
                        .os_input
                        .get_stdout_writer()
                        .write(get_cell_pixel_info.as_bytes())
                        .unwrap();
                    ansi_stdin_parser.increment_expected_ansi_instructions(4);
                },
                Err(err) => panic!("Encountered read error: {:?}", err),
            }
        }
    }
    fn handle_key(&mut self, key: &Key, raw_bytes: Vec<u8>) {
        let keybinds = &self.config.keybinds;
        for action in Keybinds::key_to_actions(key, raw_bytes, &self.mode, keybinds) {
            let should_exit = self.dispatch_action(action, None);
            if should_exit {
                self.should_exit = true;
            }
        }
    }
    fn handle_possible_pixel_instruction(
        &mut self,
        pixel_instruction_or_keys: Option<AnsiStdinInstructionOrKeys>,
    ) {
        match pixel_instruction_or_keys {
            Some(AnsiStdinInstructionOrKeys::PixelDimensions(pixel_dimensions)) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::TerminalPixelDimensions(pixel_dimensions));
            },
            Some(AnsiStdinInstructionOrKeys::BackgroundColor(background_color_instruction)) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::BackgroundColor(
                        background_color_instruction,
                    ));
            },
            Some(AnsiStdinInstructionOrKeys::ForegroundColor(foreground_color_instruction)) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::ForegroundColor(
                        foreground_color_instruction,
                    ));
            },
            Some(AnsiStdinInstructionOrKeys::Keys(keys)) => {
                for (key, raw_bytes) in keys {
                    self.handle_key(&key, raw_bytes);
                }
            },
            None => {},
        }
    }
    fn handle_mouse_event(&mut self, mouse_event: &MouseEvent) {
        match *mouse_event {
            MouseEvent::Press(button, point) => match button {
                MouseButton::WheelUp => {
                    self.dispatch_action(Action::ScrollUpAt(point), None);
                },
                MouseButton::WheelDown => {
                    self.dispatch_action(Action::ScrollDownAt(point), None);
                },
                MouseButton::Left => {
                    if self.holding_mouse {
                        self.dispatch_action(Action::MouseHold(point), None);
                    } else {
                        self.dispatch_action(Action::LeftClick(point), None);
                    }
                    self.holding_mouse = true;
                },
                MouseButton::Right => {
                    if self.holding_mouse {
                        self.dispatch_action(Action::MouseHold(point), None);
                    } else {
                        self.dispatch_action(Action::RightClick(point), None);
                    }
                    self.holding_mouse = true;
                },
                _ => {},
            },
            MouseEvent::Release(point) => {
                self.dispatch_action(Action::MouseRelease(point), None);
                self.holding_mouse = false;
            },
            MouseEvent::Hold(point) => {
                self.dispatch_action(Action::MouseHold(point), None);
                self.holding_mouse = true;
            },
        }
    }
    fn handle_actions(&mut self, actions: Vec<Action>, session_name: &str, clients: Vec<ClientId>) {
        // TODO: handle Detach correctly
        for action in actions {
            match action {
                Action::Quit => {
                    crate::sessions::kill_session(session_name);
                    break;
                },
                Action::Detach => {
                    // self.should_exit = true;
                    // clients.split_last().into_iter().for_each(|(client_id, _)| {
                    let first = clients.first().unwrap();
                    let last = clients.last().unwrap();
                    self.os_input
                        .send_to_server(ClientToServerMsg::DetachSession(vec![*first, *last]));
                    // });
                    break;
                },
                // Actions, that are indepenedent from the specific client
                // should be specified here.
                Action::NewTab(_) | Action::Run(_) | Action::NewPane(_) => {
                    let client_id = clients.first().unwrap();
                    log::error!("Sending action to client: {}", client_id);
                    self.dispatch_action(action, Some(*client_id));
                },
                _ => {
                    // TODO only dispatch for each client, for actions that need it
                    for client_id in &clients {
                        self.dispatch_action(action.clone(), Some(*client_id));
                    }
                },
            }
        }
        // self.dispatch_action(Action::Quit, None);
        // is this correct? should be just for this current client
        self.should_exit = true;
        log::error!("Quitting Now. Dispatched the actions");
        // std::process::exit(0);
        //self.dispatch_action(Action::NoOp);
        self.exit();
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
    fn dispatch_action(&mut self, action: Action, client_id: Option<ClientId>) -> bool {
        let mut should_break = false;

        match action {
            Action::NoOp => {},
            Action::Quit | Action::Detach => {
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, client_id));
                self.exit();
                should_break = true;
            },
            Action::SwitchToMode(mode) => {
                // this is an optimistic update, we should get a SwitchMode instruction from the
                // server later that atomically changes the mode as well
                self.mode = mode;
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, None));
            },
            Action::CloseFocus
            | Action::NewPane(_)
            | Action::Run(_)
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
                log::error!("Blocking input thread.");
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, client_id));
                self.command_is_executing
                    .wait_until_input_thread_is_unblocked();
            },
            _ => self
                .os_input
                .send_to_server(ClientToServerMsg::Action(action, client_id)),
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
#[allow(clippy::too_many_arguments)]
pub(crate) fn input_actions(
    os_input: Box<dyn ClientOsApi>,
    config: Config,
    options: Options,
    command_is_executing: CommandIsExecuting,
    clients: Vec<ClientId>,
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
    .handle_actions(actions, &session_name, clients);
}

#[cfg(test)]
#[path = "./unit/input_handler_tests.rs"]
mod input_handler_tests;
