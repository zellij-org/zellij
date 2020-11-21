/// Module for handling input
use std::io::Read;
use std::sync::mpsc::Sender;

use crate::os_input_output::OsApi;
use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::AppInstruction;
use crate::CommandIsExecuting;

struct InputHandler {
    buffer: [u8; 10], // TODO: more accurately
    mode: InputMode,
    stdin: Box<dyn Read>,
    command_is_executing: CommandIsExecuting,
    send_screen_instructions: Sender<ScreenInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
}

impl InputHandler {
    fn new(
        os_input: Box<dyn OsApi>,
        command_is_executing: CommandIsExecuting,
        send_screen_instructions: Sender<ScreenInstruction>,
        send_pty_instructions: Sender<PtyInstruction>,
        send_app_instructions: Sender<AppInstruction>,
    ) -> Self {
        InputHandler {
            buffer: [0; 10], // TODO: more accurately
            mode: InputMode::Normal,
            stdin: os_input.get_stdin_reader(),
            command_is_executing,
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
                InputMode::Command => self.read_command_mode(false),
                InputMode::CommandPersistent => self.read_command_mode(true),
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
            let _ = self
                .stdin
                .read(&mut self.buffer)
                .expect("failed to read stdin");

            match self.buffer {
                [7, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // ctrl-g
                    // debug_log_to_file(format!("switched to command mode"));
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
    fn read_command_mode(&mut self, persistent: bool) {
        //@@@khs26 Add a powerbar type thing that we can write output to
        if persistent {
            assert_eq!(self.mode, InputMode::CommandPersistent);
        } else {
            assert_eq!(self.mode, InputMode::Command);
        }

        loop {
            self.buffer = [0; 10];
            let _ = self
                .stdin
                .read(&mut self.buffer)
                .expect("failed to read stdin");
            // uncomment this to print the entered character to a log file (/tmp/mosaic/mosaic-log.txt) for debugging
            // debug_log_to_file(format!("buffer {:?}", self.buffer));
            match self.buffer {
                [7, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // Ctrl-g
                    // If we're in command mode, this will let us switch to persistent command mode, to execute
                    // multiple commands. If we're already in persistent mode, it'll return us to normal mode.
                    match self.mode {
                        InputMode::Command => self.mode = InputMode::CommandPersistent,
                        InputMode::CommandPersistent => {
                            self.mode = InputMode::Normal;
                            // debug_log_to_file(format!("switched to normal mode"));
                            return;
                        }
                        _ => panic!(),
                    }
                }
                [27, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // Esc
                    self.mode = InputMode::Normal;
                    // _debug_log_to_file(format!("switched to normal mode"));
                    return;
                }
                [106, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // j
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeDown)
                        .unwrap();
                }
                [107, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // k
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeUp)
                        .unwrap();
                }
                [112, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // p
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocus)
                        .unwrap();
                }
                [104, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // h
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeLeft)
                        .unwrap();
                }
                [108, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // l
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeRight)
                        .unwrap();
                }
                [122, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // z
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminal(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [110, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // n
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalVertically(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [98, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // b
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [113, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // q
                    self.mode = InputMode::Exiting;
                    return;
                }
                [27, 91, 53, 126, 0, 0, 0, 0, 0, 0] => {
                    // PgUp
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollUp)
                        .unwrap();
                }
                [27, 91, 54, 126, 0, 0, 0, 0, 0, 0] => {
                    // PgDown
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollDown)
                        .unwrap();
                }
                [120, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // x
                    self.command_is_executing.closing_pane();
                    self.send_screen_instructions
                        .send(ScreenInstruction::CloseFocusedPane)
                        .unwrap();
                    self.command_is_executing.wait_until_pane_is_closed();
                }
                [101, 0, 0, 0, 0, 0, 0, 0, 0, 0] => {
                    // e
                    self.send_screen_instructions
                        .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                        .unwrap();
                }
                //@@@khs26 Write this to the powerbar?
                _ => {}
            }

            if self.mode == InputMode::Command {
                self.mode = InputMode::Normal;
                return;
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

/// Dictates whether we're in command mode, persistent command mode, normal mode or exiting:
/// - Normal mode either writes characters to the terminal, or switches to command mode
///   using a particular key control
/// - Command mode intercepts characters to control mosaic itself, before switching immediately
///   back to normal mode
/// - Persistent command mode is the same as command mode, but doesn't return automatically to
///   normal mode
/// - Exiting means that we should start the shutdown process for mosaic or the given
///   input handler
#[derive(Debug, PartialEq)]
pub enum InputMode {
    Normal,
    Command,
    CommandPersistent,
    Exiting,
}

/// Entry point to the module that instantiates a new InputHandler and calls its
/// reading loop
pub fn input_loop(
    os_input: Box<dyn OsApi>,
    command_is_executing: CommandIsExecuting,
    send_screen_instructions: Sender<ScreenInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
) {
    let _handler = InputHandler::new(
        os_input,
        command_is_executing,
        send_screen_instructions,
        send_pty_instructions,
        send_app_instructions,
    )
    .get_input();
}
