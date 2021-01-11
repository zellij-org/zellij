/// Module for handling input
use crate::os_input_output::OsApi;
use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::CommandIsExecuting;
use crate::{errors::ContextType, wasm_vm::PluginInstruction};
use crate::{AppInstruction, SenderWithContext, OPENCALLS};

struct InputHandler {
    mode: InputMode,
    os_input: Box<dyn OsApi>,
    command_is_executing: CommandIsExecuting,
    send_screen_instructions: SenderWithContext<ScreenInstruction>,
    send_pty_instructions: SenderWithContext<PtyInstruction>,
    send_plugin_instructions: SenderWithContext<PluginInstruction>,
    send_app_instructions: SenderWithContext<AppInstruction>,
}

impl InputHandler {
    fn new(
        os_input: Box<dyn OsApi>,
        command_is_executing: CommandIsExecuting,
        send_screen_instructions: SenderWithContext<ScreenInstruction>,
        send_pty_instructions: SenderWithContext<PtyInstruction>,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        send_app_instructions: SenderWithContext<AppInstruction>,
    ) -> Self {
        InputHandler {
            mode: InputMode::Normal,
            os_input,
            command_is_executing,
            send_screen_instructions,
            send_pty_instructions,
            send_plugin_instructions,
            send_app_instructions,
        }
    }

    /// Main event loop
    fn get_input(&mut self) {
        let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
        err_ctx.add_call(ContextType::StdinHandler);
        self.send_pty_instructions.update(err_ctx);
        self.send_plugin_instructions.update(err_ctx);
        self.send_app_instructions.update(err_ctx);
        self.send_screen_instructions.update(err_ctx);
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
            let stdin_buffer = self.os_input.read_from_stdin();
            #[cfg(not(test))] // Absolutely zero clue why this breaks *all* of the tests
            drop(
                self.send_plugin_instructions
                    .send(PluginInstruction::GlobalInput(stdin_buffer.clone())),
            );
            match stdin_buffer.as_slice() {
                [7] => {
                    // ctrl-g
                    self.mode = InputMode::Command;
                    return;
                }
                _ => {
                    self.send_screen_instructions
                        .send(ScreenInstruction::ClearScroll)
                        .unwrap();
                    self.send_screen_instructions
                        .send(ScreenInstruction::WriteCharacter(stdin_buffer))
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
            let stdin_buffer = self.os_input.read_from_stdin();
            #[cfg(not(test))] // Absolutely zero clue why this breaks *all* of the tests
            drop(
                self.send_plugin_instructions
                    .send(PluginInstruction::GlobalInput(stdin_buffer.clone())),
            );
            // uncomment this to print the entered character to a log file (/tmp/mosaic/mosaic-log.txt) for debugging
            // debug_log_to_file(format!("buffer {:?}", stdin_buffer));

            match stdin_buffer.as_slice() {
                [7] => {
                    // Ctrl-g
                    // If we're in command mode, this will let us switch to persistent command mode, to execute
                    // multiple commands. If we're already in persistent mode, it'll return us to normal mode.
                    match self.mode {
                        InputMode::Command => self.mode = InputMode::CommandPersistent,
                        InputMode::CommandPersistent => {
                            self.mode = InputMode::Normal;
                            return;
                        }
                        _ => panic!(),
                    }
                }
                [27] => {
                    // Esc
                    self.mode = InputMode::Normal;
                    return;
                }
                [106] => {
                    // j
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeDown)
                        .unwrap();
                }
                [107] => {
                    // k
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeUp)
                        .unwrap();
                }
                [112] => {
                    // p
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocus)
                        .unwrap();
                }
                [104] => {
                    // h
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeLeft)
                        .unwrap();
                }
                [108] => {
                    // l
                    self.send_screen_instructions
                        .send(ScreenInstruction::ResizeRight)
                        .unwrap();
                }
                [122] => {
                    // z
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminal(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [110] => {
                    // n
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalVertically(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [98] => {
                    // b
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [113] => {
                    // q
                    self.mode = InputMode::Exiting;
                    return;
                }
                [27, 91, 53, 126] => {
                    // PgUp
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollUp)
                        .unwrap();
                }
                [27, 91, 54, 126] => {
                    // PgDown
                    self.send_screen_instructions
                        .send(ScreenInstruction::ScrollDown)
                        .unwrap();
                }
                [120] => {
                    // x
                    self.command_is_executing.closing_pane();
                    self.send_screen_instructions
                        .send(ScreenInstruction::CloseFocusedPane)
                        .unwrap();
                    self.command_is_executing.wait_until_pane_is_closed();
                }
                [101] => {
                    // e
                    self.send_screen_instructions
                        .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                        .unwrap();
                }
                [121] => {
                    // y
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocusLeft)
                        .unwrap()
                }
                [117] => {
                    // u
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocusDown)
                        .unwrap()
                }
                [105] => {
                    // i
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocusUp)
                        .unwrap()
                }
                [111] => {
                    // o
                    self.send_screen_instructions
                        .send(ScreenInstruction::MoveFocusRight)
                        .unwrap()
                }
                [49] => {
                    // 1
                    self.command_is_executing.opening_new_pane();
                    self.send_pty_instructions
                        .send(PtyInstruction::NewTab)
                        .unwrap();
                    self.command_is_executing.wait_until_new_pane_is_opened();
                }
                [50] => {
                    // 2
                    self.send_screen_instructions
                        .send(ScreenInstruction::SwitchTabPrev)
                        .unwrap()
                }
                [51] => {
                    // 3
                    self.send_screen_instructions
                        .send(ScreenInstruction::SwitchTabNext)
                        .unwrap()
                }
                [52] => {
                    // 4
                    self.command_is_executing.closing_pane();
                    self.send_screen_instructions
                        .send(ScreenInstruction::CloseTab)
                        .unwrap();
                    self.command_is_executing.wait_until_pane_is_closed();
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
        self.send_plugin_instructions
            .send(PluginInstruction::Quit)
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
    send_screen_instructions: SenderWithContext<ScreenInstruction>,
    send_pty_instructions: SenderWithContext<PtyInstruction>,
    send_plugin_instructions: SenderWithContext<PluginInstruction>,
    send_app_instructions: SenderWithContext<AppInstruction>,
) {
    let _handler = InputHandler::new(
        os_input,
        command_is_executing,
        send_screen_instructions,
        send_pty_instructions,
        send_plugin_instructions,
        send_app_instructions,
    )
    .get_input();
}
