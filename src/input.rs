/// Module for handling input
use std::io::Read;
use std::sync::mpsc::Sender;

use crate::os_input_output::OsApi;
use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::{AppInstruction, _debug_log_to_file};

struct InputHandler {
    buffer: [u8; 10], // TODO: more accurately
    mode: InputMode,
    stdin: Box<dyn Read>,
    send_screen_instructions: Sender<ScreenInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
}

impl InputHandler {
    fn new(
        os_input: Box<dyn OsApi>,
        send_screen_instructions: Sender<ScreenInstruction>,
        send_pty_instructions: Sender<PtyInstruction>,
        send_app_instructions: Sender<AppInstruction>,
    ) -> Self {
        InputHandler {
            buffer: [0; 10], // TODO: more accurately
            mode: InputMode::Normal,
            stdin: os_input.get_stdin_reader(),
            send_screen_instructions,
            send_pty_instructions,
            send_app_instructions,
        }
    }

    /// Main event loop
    fn get_input(&mut self) {
        loop {
            match self.mode {
                InputMode::Normal => self.read_normal_mode(),
                InputMode::Command => self.read_command_mode(),
                InputMode::Exiting => {
                    self.exit();
                    break;
                }
            }
        }
    }

    /// Read input to the terminal (or switch to command mode)
    fn read_normal_mode(&mut self) {
        assert_eq!(self.mode, InputMode::Normal);

        loop {
            self.stdin
                .read(&mut self.buffer)
                .expect("failed to read stdin");

            match self.buffer {
                [7, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-g
                    _debug_log_to_file(format!("switched to command mode"));
                    self.mode = InputMode::Command;
                    return;
                }
                _ => {
                    self.send_screen_instructions
                        .send(ScreenInstruction::ClearScroll)
                        .unwrap();
                    self.send_screen_instructions
                        .send(ScreenInstruction::WriteCharacter(self.buffer))
                        .unwrap();
                }
            }
        }
    }

    /// Read input and parse it as commands for mosaic
    fn read_command_mode(&mut self) {
        //@@@khs26 Add a powerbar type thing that we can write output to
        assert_eq!(self.mode, InputMode::Command);

        loop {
            self.stdin
                .read(&mut self.buffer)
                .expect("failed to read stdin");
            // uncomment this to print the entered character to a log file (/tmp/mosaic-log.txt) for debugging
            _debug_log_to_file(format!("buffer {:?}", self.buffer));
            match self.buffer {
                [7, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-g
                    self.mode = InputMode::Normal;
                    _debug_log_to_file(format!("switched to normal mode"));
                    return;
                }
                [10, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-j
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeDown)
                        .unwrap();
                }
                [11, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-k
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeUp)
                        .unwrap();
                }
                [16, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-p
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocus)
                        .unwrap();
                }
                [8, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-h
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeLeft)
                        .unwrap();
                }
                [12, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-l
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeRight)
                        .unwrap();
                }
                [26, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-z
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminal(None))
                        .unwrap();
                }
                [14, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-n
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalVertically(None))
                        .unwrap();
                }
                [2, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-b
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                        .unwrap();
                }
                [17, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-q
                    self.mode = InputMode::Exiting;
                    return;
                }
                [27, 91, 53, 94, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-PgUp
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollUp)
                        .unwrap();
                }
                [27, 91, 54, 94, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-PgDown
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollDown)
                        .unwrap();
                }
                [24, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-x
                    self.send_screen_instructions
                        .send(ScreenInstruction::CloseFocusedPane)
                        .unwrap();
                    // ::std::thread::sleep(::std::time::Duration::from_millis(10));
                }
                [5, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-e
                    self.send_screen_instructions
                        .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                        .unwrap();
                }
                //@@@khs26 Write this to the powerbar?
                _ => {}
            }
        }
    }

    /// Routine to be called when the input handler exits (at the moment this is the
    /// same as quitting mosaic)
    fn exit(&mut self) {
        self.send_screen_instructions
            .send(ScreenInstruction::Quit)
            .unwrap();
        self.send_pty_instructions
            .send(PtyInstruction::Quit)
            .unwrap();
        self.send_app_instructions
            .send(AppInstruction::Exit)
            .unwrap();
    }
}

/// Dictates whether we're in command mode, normal mode or exiting:
/// - Normal mode either writes characters to the terminal, or switches to command mode
///   using a particular key control
/// - Command mode intercepts characters to control mosaic itself, including to switch
///   back to normal mode
/// - Exiting means that we should start the shutdown process for mosaic or the given
///   input handler
#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Command,
    Exiting,
}

/// Entry point to the module that instantiates a new InputHandler and calls its
/// reading loop
pub fn input_loop(
    os_input: Box<dyn OsApi>,
    send_screen_instructions: Sender<ScreenInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
) {
    let _handler = InputHandler::new(
        os_input,
        send_screen_instructions,
        send_pty_instructions,
        send_app_instructions,
    )
    .get_input();
}
