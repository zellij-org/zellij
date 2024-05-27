//! Main input logic.
use crate::{
    os_input_output::ClientOsApi, stdin_ansi_parser::AnsiStdinInstruction, ClientId,
    ClientInstruction, CommandIsExecuting, InputInstruction,
};
use zellij_utils::{
    channels::{Receiver, SenderWithContext, OPENCALLS},
    data::{InputMode, KeyWithModifier},
    errors::{ContextType, ErrorContext, FatalError},
    input::{
        actions::Action,
        cast_termwiz_key,
        config::Config,
        mouse::{MouseButton, MouseEvent},
        options::Options,
    },
    ipc::{ClientToServerMsg, ExitReason},
    termwiz::input::InputEvent,
};

#[derive(Debug, Clone, Copy)]
enum HeldMouseButton {
    Left,
    Right,
    Middle,
}

impl Default for HeldMouseButton {
    fn default() -> Self {
        HeldMouseButton::Left
    }
}

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
    holding_mouse: Option<HeldMouseButton>,
    mouse_mode_active: bool,
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
            holding_mouse: None,
            mouse_mode_active: false,
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
            self.os_input.enable_mouse().non_fatal();
            self.mouse_mode_active = true;
        }
        loop {
            if self.should_exit {
                break;
            }
            match self.receive_input_instructions.recv() {
                Ok((InputInstruction::KeyEvent(input_event, raw_bytes), _error_context)) => {
                    match input_event {
                        InputEvent::Key(key_event) => {
                            let key = cast_termwiz_key(
                                key_event,
                                &raw_bytes,
                                Some((&self.config.keybinds, &self.mode)),
                            );
                            self.handle_key(&key, raw_bytes, false);
                        },
                        InputEvent::Mouse(mouse_event) => {
                            let mouse_event =
                                zellij_utils::input::mouse::MouseEvent::from(mouse_event);
                            self.handle_mouse_event(&mouse_event);
                        },
                        InputEvent::Paste(pasted_text) => {
                            if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
                                self.dispatch_action(
                                    Action::Write(None, bracketed_paste_start.clone(), false),
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write(None, pasted_text.as_bytes().to_vec(), false),
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write(None, bracketed_paste_end.clone(), false),
                                    None,
                                );
                            }
                            if self.mode == InputMode::EnterSearch {
                                self.dispatch_action(
                                    Action::SearchInput(pasted_text.as_bytes().to_vec()),
                                    None,
                                );
                            }
                            if self.mode == InputMode::RenameTab {
                                self.dispatch_action(
                                    Action::TabNameInput(pasted_text.as_bytes().to_vec()),
                                    None,
                                );
                            }
                            if self.mode == InputMode::RenamePane {
                                self.dispatch_action(
                                    Action::PaneNameInput(pasted_text.as_bytes().to_vec()),
                                    None,
                                );
                            }
                        },
                        _ => {},
                    }
                },
                Ok((
                    InputInstruction::KeyWithModifierEvent(key_with_modifier, raw_bytes),
                    _error_context,
                )) => {
                    self.handle_key(&key_with_modifier, raw_bytes, true);
                },
                Ok((InputInstruction::SwitchToMode(input_mode), _error_context)) => {
                    self.mode = input_mode;
                },
                Ok((
                    InputInstruction::AnsiStdinInstructions(ansi_stdin_instructions),
                    _error_context,
                )) => {
                    for ansi_instruction in ansi_stdin_instructions {
                        self.handle_stdin_ansi_instruction(ansi_instruction);
                    }
                },
                Ok((InputInstruction::StartedParsing, _error_context)) => {
                    self.send_client_instructions
                        .send(ClientInstruction::StartedParsingStdinQuery)
                        .unwrap();
                },
                Ok((InputInstruction::DoneParsing, _error_context)) => {
                    self.send_client_instructions
                        .send(ClientInstruction::DoneParsingStdinQuery)
                        .unwrap();
                },
                Ok((InputInstruction::Exit, _error_context)) => {
                    self.should_exit = true;
                },
                Err(err) => panic!("Encountered read error: {:?}", err),
            }
        }
    }
    fn handle_key(
        &mut self,
        key: &KeyWithModifier,
        raw_bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    ) {
        let keybinds = &self.config.keybinds;
        for action in keybinds.get_actions_for_key_in_mode_or_default_action(
            &self.mode,
            key,
            raw_bytes,
            is_kitty_keyboard_protocol,
        ) {
            let should_exit = self.dispatch_action(action, None);
            if should_exit {
                self.should_exit = true;
            }
        }
    }
    fn handle_stdin_ansi_instruction(&mut self, ansi_stdin_instructions: AnsiStdinInstruction) {
        match ansi_stdin_instructions {
            AnsiStdinInstruction::PixelDimensions(pixel_dimensions) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::TerminalPixelDimensions(pixel_dimensions));
            },
            AnsiStdinInstruction::BackgroundColor(background_color_instruction) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::BackgroundColor(
                        background_color_instruction,
                    ));
            },
            AnsiStdinInstruction::ForegroundColor(foreground_color_instruction) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::ForegroundColor(
                        foreground_color_instruction,
                    ));
            },
            AnsiStdinInstruction::ColorRegisters(color_registers) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::ColorRegisters(color_registers));
            },
            AnsiStdinInstruction::SynchronizedOutput(enabled) => {
                self.send_client_instructions
                    .send(ClientInstruction::SetSynchronizedOutput(enabled))
                    .unwrap();
            },
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
                    if self.holding_mouse.is_some() {
                        self.dispatch_action(Action::MouseHoldLeft(point), None);
                    } else {
                        self.dispatch_action(Action::LeftClick(point), None);
                    }
                    self.holding_mouse = Some(HeldMouseButton::Left);
                },
                MouseButton::Right => {
                    if self.holding_mouse.is_some() {
                        self.dispatch_action(Action::MouseHoldRight(point), None);
                    } else {
                        self.dispatch_action(Action::RightClick(point), None);
                    }
                    self.holding_mouse = Some(HeldMouseButton::Right);
                },
                MouseButton::Middle => {
                    if self.holding_mouse.is_some() {
                        self.dispatch_action(Action::MouseHoldMiddle(point), None);
                    } else {
                        self.dispatch_action(Action::MiddleClick(point), None);
                    }
                    self.holding_mouse = Some(HeldMouseButton::Middle);
                },
            },
            MouseEvent::Release(point) => {
                let button_released = self.holding_mouse.unwrap_or_default();
                match button_released {
                    HeldMouseButton::Left => {
                        self.dispatch_action(Action::LeftMouseRelease(point), None)
                    },
                    HeldMouseButton::Right => {
                        self.dispatch_action(Action::RightMouseRelease(point), None)
                    },
                    HeldMouseButton::Middle => {
                        self.dispatch_action(Action::MiddleMouseRelease(point), None)
                    },
                };
                self.holding_mouse = None;
            },
            MouseEvent::Hold(point) => {
                let button_held = self.holding_mouse.unwrap_or_default();
                match button_held {
                    HeldMouseButton::Left => {
                        self.dispatch_action(Action::MouseHoldLeft(point), None)
                    },
                    HeldMouseButton::Right => {
                        self.dispatch_action(Action::MouseHoldRight(point), None)
                    },
                    HeldMouseButton::Middle => {
                        self.dispatch_action(Action::MouseHoldMiddle(point), None)
                    },
                };
                self.holding_mouse = Some(button_held);
            },
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
    fn dispatch_action(&mut self, action: Action, client_id: Option<ClientId>) -> bool {
        let mut should_break = false;

        match action {
            Action::NoOp => {},
            Action::Quit => {
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, None, client_id));
                self.exit(ExitReason::Normal);
                should_break = true;
            },
            Action::Detach => {
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, None, client_id));
                self.exit(ExitReason::NormalDetached);
                should_break = true;
            },
            Action::SwitchToMode(mode) => {
                // this is an optimistic update, we should get a SwitchMode instruction from the
                // server later that atomically changes the mode as well
                self.mode = mode;
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, None, None));
            },
            Action::CloseFocus
            | Action::ClearScreen
            | Action::NewPane(..)
            | Action::Run(_)
            | Action::NewTiledPane(..)
            | Action::NewFloatingPane(..)
            | Action::ToggleFloatingPanes
            | Action::TogglePaneEmbedOrFloating
            | Action::NewTab(..)
            | Action::GoToNextTab
            | Action::GoToPreviousTab
            | Action::CloseTab
            | Action::GoToTab(_)
            | Action::MoveTab(_)
            | Action::GoToTabName(_, _)
            | Action::ToggleTab
            | Action::MoveFocusOrTab(_) => {
                self.command_is_executing.blocking_input_thread();
                self.os_input
                    .send_to_server(ClientToServerMsg::Action(action, None, client_id));
                self.command_is_executing
                    .wait_until_input_thread_is_unblocked();
            },
            Action::ToggleMouseMode => {
                if self.mouse_mode_active {
                    self.os_input.disable_mouse().non_fatal();
                    self.mouse_mode_active = false;
                } else {
                    self.os_input.enable_mouse().non_fatal();
                    self.mouse_mode_active = true;
                }
            },
            _ => self
                .os_input
                .send_to_server(ClientToServerMsg::Action(action, None, client_id)),
        }

        should_break
    }

    /// Routine to be called when the input handler exits (at the moment this is the
    /// same as quitting Zellij).
    fn exit(&mut self, reason: ExitReason) {
        self.send_client_instructions
            .send(ClientInstruction::Exit(reason))
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
