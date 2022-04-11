//! Main input logic.
use zellij_utils::{
    input::{
        mouse::{MouseButton, MouseEvent},
        options::Options,
    },
    pane_size::SizeInPixels,
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
    ipc::{ClientToServerMsg, ExitReason, PixelDimensions},
    regex::Regex,
    lazy_static::lazy_static,
};

use zellij_tile::data::{InputMode, Key};

struct PixelCsiParser {
    expected_pixel_csi_instructions: usize,
    current_buffer: Vec<(Key, Vec<u8>)>,
}

impl PixelCsiParser {
    pub fn new() -> Self {
        PixelCsiParser {
            expected_pixel_csi_instructions: 0,
            current_buffer: vec![],
        }
    }
    pub fn increment_expected_csi_instructions(&mut self, by: usize) {
        self.expected_pixel_csi_instructions += by;
    }
    pub fn decrement_expected_csi_instructions(&mut self, by: usize) {
        self.expected_pixel_csi_instructions = self.expected_pixel_csi_instructions.saturating_sub(by);
    }
    pub fn expected_instructions(&self) -> usize {
        self.expected_pixel_csi_instructions
    }
    pub fn parse(&mut self, key: Key, raw_bytes: Vec<u8>) -> Option<PixelInstructionOrKeys> {
        if let Key::Char('t') = key {
            self.current_buffer.push((key, raw_bytes));
            match PixelInstructionOrKeys::pixel_instruction_from_keys(&self.current_buffer) {
                Ok(pixel_instruction) => {
                    self.decrement_expected_csi_instructions(1);
                    self.current_buffer.clear();
                    Some(pixel_instruction)
                },
                Err(_) => {
                    self.expected_pixel_csi_instructions = 0;
                    Some(PixelInstructionOrKeys::Keys(self.current_buffer.drain(..).collect()))
                }
            }
        } else if self.key_is_valid(key) {
            self.current_buffer.push((key, raw_bytes));
            None
        } else {
            self.current_buffer.push((key, raw_bytes));
            self.expected_pixel_csi_instructions = 0;
            Some(PixelInstructionOrKeys::Keys(self.current_buffer.drain(..).collect()))
        }
    }
    fn key_is_valid(&self, key: Key) -> bool {
        match key {
            Key::Esc | Key::Char(';') | Key::Char('[') => true,
            Key::Char(c) => {
                if let '0'..='9' = c {
                    true
                } else {
                    false
                }
            }
            _ => false
        }
    }
}

#[derive(Debug)]
enum PixelInstructionOrKeys {
    PixelInstruction(PixelInstruction),
    Keys(Vec<(Key, Vec<u8>)>),
}

impl PixelInstructionOrKeys {
    pub fn pixel_instruction_from_keys(keys: &Vec<(Key, Vec<u8>)>) -> Result<Self, &'static str> {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^\u{1b}\[(\d+);(\d+);(\d+)t$").unwrap();
        }
        let key_sequence: Vec<Option<char>> = keys.iter().map(|(key, _)| {
            match key {
                Key::Char(c) => Some(*c),
                Key::Esc => Some('\u{1b}'),
                _ => None,
            }
        }).collect();
        if key_sequence.iter().all(|k| k.is_some()) {
            let key_string: String = key_sequence.iter().map(|k| k.unwrap()).collect();
            let captures = RE.captures_iter(&key_string).next().ok_or("invalid_instruction")?;
            let csi_index = captures[1].parse::<usize>();
            let first_field = captures[2].parse::<usize>();
            let second_field = captures[3].parse::<usize>();
            if csi_index.is_err() || first_field.is_err() || second_field.is_err() {
                return Err("invalid_instruction");
            }
            return Ok(PixelInstructionOrKeys::PixelInstruction(PixelInstruction {
                csi_index: csi_index.unwrap(),
                first_field: first_field.unwrap(),
                second_field: second_field.unwrap(),
            }))
        } else {
            return Err("invalid sequence");
        }
    }
}

#[derive(Debug)]
struct PixelInstruction {
    csi_index: usize,
    first_field: usize,
    second_field: usize,
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
        // TODO:
        // * send the pixel stuff from here
        // * increment the "expected_pixel_csi_instructions" counter when doing so
        // * whenever the counter is greater than 0, we should be using it instead of the
        // handle_key stuff
        // * as soon as the parser sees something it doesn't recognize, it dumps all of its buffer
        // into handle_key one byte at a time
        // * otherwise it builds pixel instructions until its done, decrementing the counter for
        // every pixel instruction
        let get_cell_pixel_info = "\u{1b}[14t\u{1b}[16t\u{1b}[18t";
        #[cfg(not(test))] // TODO: find a way to test this, maybe by implementing the io::Write trait
        let _ = self.os_input
            .get_stdout_writer()
            .write(get_cell_pixel_info.as_bytes())
            .unwrap();
        let mut pixel_csi_parser = PixelCsiParser::new();
        pixel_csi_parser.increment_expected_csi_instructions(3);
        loop {
            if self.should_exit {
                break;
            }
            match self.receive_input_instructions.recv() {
                Ok((InputInstruction::KeyEvent(input_event, raw_bytes), _error_context)) => {
                    match input_event {
                        InputEvent::Key(key_event) => {
                            let key = cast_termwiz_key(key_event, &raw_bytes);
                            if pixel_csi_parser.expected_instructions() > 0 {
                                self.handle_possible_pixel_instruction(pixel_csi_parser.parse(key, raw_bytes));
                            } else {
                                self.handle_key(&key, raw_bytes);
                            }
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
    fn handle_possible_pixel_instruction(&mut self, pixel_instruction_or_keys: Option<PixelInstructionOrKeys>) {
        let mut text_area_size = None;
        let mut character_cell_size = None;
        match pixel_instruction_or_keys {
            Some(PixelInstructionOrKeys::PixelInstruction(pixel_instruction)) => {
                match pixel_instruction.csi_index {
                    4 => {
                        // text area size
                        text_area_size = Some(SizeInPixels {
                            height: pixel_instruction.first_field,
                            width: pixel_instruction.second_field,
                        });
                    },
                    6 => {
                        // character cell size
                        character_cell_size = Some(SizeInPixels {
                            height: pixel_instruction.first_field,
                            width: pixel_instruction.second_field,
                        });
                    },
                    _ => {}
                }
                let pixel_dimensions = PixelDimensions { text_area_size, character_cell_size };
                log::info!("pixel_dimensions: {:?}", pixel_dimensions);
                self.os_input
                    .send_to_server(ClientToServerMsg::TerminalPixelDimensions(pixel_dimensions));
                // TODO: CONTINUE HERE (09/04) -
                // - send these to the server and log them on screen - DONE
                // - calculate the pixel_instruction stuff from the csis and then them to the
                // server - DONE
                //
                // - then briefly experiment with fixing the cursor height width ratio thing with
                // them - DONE
                // - then do this whole thing on sigwinch
                // - then respond ourselves to these queries
                // - then extensively test (including unit tests) and merge
            },
            Some(PixelInstructionOrKeys::Keys(keys)) => {
                for (key, raw_bytes) in keys {
                    self.handle_key(&key, raw_bytes);
                }
            }
            None => {}
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

#[cfg(test)]
#[path = "./unit/input_handler_tests.rs"]
mod grid_tests;
