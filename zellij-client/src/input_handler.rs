//! Main input logic.
use crate::{
    os_input_output::ClientOsApi, stdin_ansi_parser::AnsiStdinInstruction, ClientId,
    ClientInstruction, CommandIsExecuting, InputInstruction,
};
use termwiz::input::{InputEvent, Modifiers, MouseButtons, MouseEvent as TermwizMouseEvent};
use zellij_utils::{
    channels::{Receiver, SenderWithContext, OPENCALLS},
    data::{InputMode, KeyWithModifier},
    errors::{ContextType, ErrorContext, FatalError},
    input::{
        actions::Action,
        cast_termwiz_key,
        config::Config,
        mouse::{MouseEvent, MouseEventType},
        options::Options,
    },
    ipc::{ClientToServerMsg, ExitReason},
    position::Position,
};

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
    mouse_old_event: MouseEvent,
    mouse_mode_active: bool,
}

fn termwiz_mouse_convert(original_event: &mut MouseEvent, event: &TermwizMouseEvent) {
    let button_bits = &event.mouse_buttons;
    original_event.left = button_bits.contains(MouseButtons::LEFT);
    original_event.right = button_bits.contains(MouseButtons::RIGHT);
    original_event.middle = button_bits.contains(MouseButtons::MIDDLE);
    original_event.wheel_up = button_bits.contains(MouseButtons::VERT_WHEEL)
        && button_bits.contains(MouseButtons::WHEEL_POSITIVE);
    original_event.wheel_down = button_bits.contains(MouseButtons::VERT_WHEEL)
        && !button_bits.contains(MouseButtons::WHEEL_POSITIVE);

    let mods = &event.modifiers;
    original_event.shift = mods.contains(Modifiers::SHIFT);
    original_event.alt = mods.contains(Modifiers::ALT);
    original_event.ctrl = mods.contains(Modifiers::CTRL);
}

pub fn from_termwiz(old_event: &mut MouseEvent, event: TermwizMouseEvent) -> MouseEvent {
    // We use the state of old_event vs new_event to determine if this
    // event is a Press, Release, or Motion.  This is an unfortunate
    // side effect of the pre-SGR-encoded X10 mouse protocol design in
    // which release events don't carry information about WHICH
    // button(s) were released, so we have to maintain a wee bit of
    // state in between events.
    //
    // Note that only Left, Right, and Middle are saved in between
    // calls.  WheelUp/WheelDown typically do not generate Release
    // events.
    let mut new_event = MouseEvent::new();
    termwiz_mouse_convert(&mut new_event, &event);
    new_event.position = Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1));

    if (new_event.left && !old_event.left)
        || (new_event.right && !old_event.right)
        || (new_event.middle && !old_event.middle)
        || new_event.wheel_up
        || new_event.wheel_down
    {
        // This is a mouse Press event.
        new_event.event_type = MouseEventType::Press;

        // Hang onto the button state.
        *old_event = new_event;
    } else if event.mouse_buttons.is_empty()
        && !old_event.left
        && !old_event.right
        && !old_event.middle
    {
        // This is a mouse Motion event (no buttons are down).
        new_event.event_type = MouseEventType::Motion;

        // Hang onto the button state.
        *old_event = new_event;
    } else if event.mouse_buttons.is_empty()
        && (old_event.left || old_event.right || old_event.middle)
    {
        // This is a mouse Release event.  Note that we set
        // old_event.{button} to false (to release), but set ONLY the
        // new_event that were released to true before sending the
        // event up.
        if old_event.left {
            old_event.left = false;
            new_event.left = true;
        }
        if old_event.right {
            old_event.right = false;
            new_event.right = true;
        }
        if old_event.middle {
            old_event.middle = false;
            new_event.middle = true;
        }
        new_event.event_type = MouseEventType::Release;
    } else {
        // Dragging with some button down.  Return it as a Motion
        // event, and hang on to the button state.
        new_event.event_type = MouseEventType::Motion;
        *old_event = new_event;
    }

    new_event
}

impl InputHandler {
    /// Returns a new [`InputHandler`] with the attributes specified as arguments.
    fn new(
        os_input: Box<dyn ClientOsApi>,
        command_is_executing: CommandIsExecuting,
        config: Config,
        options: Options,
        send_client_instructions: SenderWithContext<ClientInstruction>,
        mode: InputMode, // TODO: we can probably get rid of this now that we're tracking it on the
        // server instead
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
            mouse_old_event: MouseEvent::new(),
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
                            let mouse_event = from_termwiz(&mut self.mouse_old_event, mouse_event);
                            self.handle_mouse_event(&mouse_event);
                        },
                        InputEvent::Paste(pasted_text) => {
                            if self.mode == InputMode::Normal || self.mode == InputMode::Locked {
                                self.dispatch_action(
                                    Action::Write {
                                        key_with_modifier: None,
                                        bytes: bracketed_paste_start.clone(),
                                        is_kitty_keyboard_protocol: false,
                                    },
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write {
                                        key_with_modifier: None,
                                        bytes: pasted_text.as_bytes().to_vec(),
                                        is_kitty_keyboard_protocol: false,
                                    },
                                    None,
                                );
                                self.dispatch_action(
                                    Action::Write {
                                        key_with_modifier: None,
                                        bytes: bracketed_paste_end.clone(),
                                        is_kitty_keyboard_protocol: false,
                                    },
                                    None,
                                );
                            }
                            if self.mode == InputMode::EnterSearch {
                                self.dispatch_action(
                                    Action::SearchInput {
                                        input: pasted_text.as_bytes().to_vec(),
                                    },
                                    None,
                                );
                            }
                            if self.mode == InputMode::RenameTab {
                                self.dispatch_action(
                                    Action::TabNameInput {
                                        input: pasted_text.as_bytes().to_vec(),
                                    },
                                    None,
                                );
                            }
                            if self.mode == InputMode::RenamePane {
                                self.dispatch_action(
                                    Action::PaneNameInput {
                                        input: pasted_text.as_bytes().to_vec(),
                                    },
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
        // we interpret the keys into actions on the server side so that we can change the
        // keybinds at runtime
        self.os_input.send_to_server(ClientToServerMsg::Key {
            key: key.clone(),
            raw_bytes,
            is_kitty_keyboard_protocol,
        });
    }
    fn handle_stdin_ansi_instruction(&mut self, ansi_stdin_instructions: AnsiStdinInstruction) {
        match ansi_stdin_instructions {
            AnsiStdinInstruction::PixelDimensions(pixel_dimensions) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::TerminalPixelDimensions {
                        pixel_dimensions,
                    });
            },
            AnsiStdinInstruction::BackgroundColor(background_color_instruction) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::BackgroundColor {
                        color: background_color_instruction,
                    });
            },
            AnsiStdinInstruction::ForegroundColor(foreground_color_instruction) => {
                self.os_input
                    .send_to_server(ClientToServerMsg::ForegroundColor {
                        color: foreground_color_instruction,
                    });
            },
            AnsiStdinInstruction::ColorRegisters(color_registers) => {
                let color_registers: Vec<_> = color_registers
                    .into_iter()
                    .map(|(index, color)| zellij_utils::ipc::ColorRegister { index, color })
                    .collect();
                self.os_input
                    .send_to_server(ClientToServerMsg::ColorRegisters { color_registers });
            },
            AnsiStdinInstruction::SynchronizedOutput(enabled) => {
                self.send_client_instructions
                    .send(ClientInstruction::SetSynchronizedOutput(enabled))
                    .unwrap();
            },
        }
    }
    fn handle_mouse_event(&mut self, mouse_event: &MouseEvent) {
        // This dispatch handles all of the output(s) to terminal
        // pane(s).
        self.dispatch_action(
            Action::MouseEvent {
                event: *mouse_event,
            },
            None,
        );
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
                self.os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id,
                    is_cli_client: false,
                });
                self.exit(ExitReason::Normal);
                should_break = true;
            },
            Action::Detach => {
                self.os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id,
                    is_cli_client: false,
                });
                self.exit(ExitReason::NormalDetached);
                should_break = true;
            },
            Action::SwitchSession { .. } => {
                self.os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id,
                    is_cli_client: false,
                });
                self.exit(ExitReason::NormalDetached);
                should_break = true;
            },
            Action::CloseFocus
            | Action::SwitchToMode { .. }
            | Action::ClearScreen
            | Action::NewPane { .. }
            | Action::Run { .. }
            | Action::NewTiledPane { .. }
            | Action::NewFloatingPane { .. }
            | Action::ToggleFloatingPanes
            | Action::TogglePaneEmbedOrFloating
            | Action::NewTab { .. }
            | Action::GoToNextTab
            | Action::GoToPreviousTab
            | Action::CloseTab
            | Action::GoToTab { .. }
            | Action::MoveTab { .. }
            | Action::GoToTabName { .. }
            | Action::ToggleTab
            | Action::MoveFocusOrTab { .. } => {
                self.command_is_executing.blocking_input_thread();
                self.os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id,
                    is_cli_client: false,
                });
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
            _ => self.os_input.send_to_server(ClientToServerMsg::Action {
                action,
                terminal_id: None,
                client_id,
                is_cli_client: false,
            }),
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
