use super::actions::Action;
use super::keybinds::get_default_keybinds;
use crate::common::{update_state, AppInstruction, AppState, SenderWithContext, OPENCALLS};
/// Module for handling input
use crate::errors::ContextType;
use crate::os_input_output::OsApi;
use crate::pty_bus::PtyInstruction;
use crate::screen::ScreenInstruction;
use crate::wasm_vm::PluginInstruction;
use crate::CommandIsExecuting;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use termion::input::TermReadEventsAndRaw;

use super::keybinds::key_to_actions;

/// Handles the dispatching of [`Action`]s according to the current
/// [`InputState`], as well as changes to that state.
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

    /// Main event loop. Interprets the terminal [`Event`](termion::event::Event)s
    /// as [`Action`]s according to the current [`InputState`], and dispatches those
    /// actions.
    fn get_input(&mut self) {
        let mut err_ctx = OPENCALLS.with(|ctx| *ctx.borrow());
        err_ctx.add_call(ContextType::StdinHandler);
        self.send_pty_instructions.update(err_ctx);
        self.send_app_instructions.update(err_ctx);
        self.send_screen_instructions.update(err_ctx);
        if let Ok(keybinds) = get_default_keybinds() {
            'input_loop: loop {
                //@@@ I think this should actually just iterate over stdin directly
                let stdin_buffer = self.os_input.read_from_stdin();
                drop(
                    self.send_plugin_instructions
                        .send(PluginInstruction::GlobalInput(stdin_buffer.clone())),
                );
                for key_result in stdin_buffer.events_and_raw() {
                    match key_result {
                        Ok((event, raw_bytes)) => match event {
                            termion::event::Event::Key(key) => {
                                // FIXME this explicit break is needed because the current test
                                // framework relies on it to not create dead threads that loop 
                                // and eat up CPUs. Do not remove until the test framework has
                                // been revised. Sorry about this (@categorille)
                                if {
                                    let mut should_break = false;
                                    for action in key_to_actions( &key, raw_bytes, &self.mode, &keybinds,) {
                                        should_break |= self.dispatch_action(action);
                                    }
                                    should_break
                                } {
                                    break 'input_loop;
                                }
                            }
                            termion::event::Event::Mouse(_)
                            | termion::event::Event::Unsupported(_) => {
                                unimplemented!("Mouse and unsupported events aren't supported!");
                            }
                        },
                        Err(err) => panic!("Encountered read error: {:?}", err),
                    }
                }
            }
        } else {
            //@@@ Error handling?
            self.exit();
        }
    }

    fn dispatch_action(&mut self, action: Action) -> bool {
        let mut should_break = false;

        match action {
            Action::Write(val) => {
                self.send_screen_instructions
                    .send(ScreenInstruction::ClearScroll)
                    .unwrap();
                self.send_screen_instructions
                    .send(ScreenInstruction::WriteCharacter(val))
                    .unwrap();
            }
            Action::Quit => {
                self.exit();
                should_break = true;
            }
            Action::SwitchToMode(mode) => {
                self.mode = mode;
                update_state(&self.send_app_instructions, |_| AppState {
                    input_mode: self.mode,
                });
                self.send_screen_instructions
                    .send(ScreenInstruction::Render)
                    .unwrap();
            }
            Action::Resize(direction) => {
                let screen_instr = match direction {
                    super::actions::Direction::Left => ScreenInstruction::ResizeLeft,
                    super::actions::Direction::Right => ScreenInstruction::ResizeRight,
                    super::actions::Direction::Up => ScreenInstruction::ResizeUp,
                    super::actions::Direction::Down => ScreenInstruction::ResizeDown,
                };
                self.send_screen_instructions.send(screen_instr).unwrap();
            }
            Action::SwitchFocus(_) => {
                self.send_screen_instructions
                    .send(ScreenInstruction::MoveFocus)
                    .unwrap();
            }
            Action::MoveFocus(direction) => {
                let screen_instr = match direction {
                    super::actions::Direction::Left => ScreenInstruction::MoveFocusLeft,
                    super::actions::Direction::Right => ScreenInstruction::MoveFocusRight,
                    super::actions::Direction::Up => ScreenInstruction::MoveFocusUp,
                    super::actions::Direction::Down => ScreenInstruction::MoveFocusDown,
                };
                self.send_screen_instructions.send(screen_instr).unwrap();
            }
            Action::ScrollUp => {
                self.send_screen_instructions
                    .send(ScreenInstruction::ScrollUp)
                    .unwrap();
            }
            Action::ScrollDown => {
                self.send_screen_instructions
                    .send(ScreenInstruction::ScrollDown)
                    .unwrap();
            }
            Action::ToggleFocusFullscreen => {
                self.send_screen_instructions
                    .send(ScreenInstruction::ToggleActiveTerminalFullscreen)
                    .unwrap();
            }
            Action::NewPane(direction) => {
                let pty_instr = match direction {
                    Some(super::actions::Direction::Left) => {
                        PtyInstruction::SpawnTerminalVertically(None)
                    }
                    Some(super::actions::Direction::Right) => {
                        PtyInstruction::SpawnTerminalVertically(None)
                    }
                    Some(super::actions::Direction::Up) => {
                        PtyInstruction::SpawnTerminalHorizontally(None)
                    }
                    Some(super::actions::Direction::Down) => {
                        PtyInstruction::SpawnTerminalHorizontally(None)
                    }
                    // No direction specified - try to put it in the biggest available spot
                    None => PtyInstruction::SpawnTerminal(None),
                };
                self.command_is_executing.opening_new_pane();
                self.send_pty_instructions.send(pty_instr).unwrap();
                self.command_is_executing.wait_until_new_pane_is_opened();
            }
            Action::CloseFocus => {
                self.command_is_executing.closing_pane();
                self.send_screen_instructions
                    .send(ScreenInstruction::CloseFocusedPane)
                    .unwrap();
                self.command_is_executing.wait_until_pane_is_closed();
            }
            Action::NewTab => {
                self.command_is_executing.opening_new_pane();
                self.send_pty_instructions
                    .send(PtyInstruction::NewTab)
                    .unwrap();
                self.command_is_executing.wait_until_new_pane_is_opened();
            }
            Action::GoToNextTab => {
                self.send_screen_instructions
                    .send(ScreenInstruction::SwitchTabNext)
                    .unwrap();
            }
            Action::GoToPreviousTab => {
                self.send_screen_instructions
                    .send(ScreenInstruction::SwitchTabPrev)
                    .unwrap();
            }
            Action::CloseTab => {
                self.command_is_executing.closing_pane();
                self.send_screen_instructions
                    .send(ScreenInstruction::CloseTab)
                    .unwrap();
                self.command_is_executing.wait_until_pane_is_closed();
            }
        }

        should_break
    }

    /// Routine to be called when the input handler exits (at the moment this is the
    /// same as quitting zellij)
    fn exit(&mut self) {
        self.send_app_instructions
            .send(AppInstruction::Exit)
            .unwrap();
    }
}

/// An `InputState` is an [`InputMode`] along with its persistency, i.e.
/// whether the mode should be exited after a single action or it should
/// stay the same until it is explicitly exited.
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct InputState {
    mode: InputMode,
    persistent: bool,
}

impl Default for InputState {
    fn default() -> InputState {
        InputState {
            mode: InputMode::Normal,
            persistent: false,
        }
    }
}

/// Dictates the input mode, which is the way that keystrokes will be interpreted:
/// - Normal mode either writes characters to the terminal, or switches to Command mode
///   using a particular key control
/// - Command mode is a menu that allows choosing another mode, like Resize or Pane
/// - Resize mode is for resizing the different panes already present
/// - Pane mode is for creating and closing panes in different directions
/// - Tab mode is for creating tabs and moving between them
/// - Scroll mode is for scrolling up and down within a pane
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, EnumIter, Serialize, Deserialize)]
pub enum InputMode {
    Normal,
    Command,
    Resize,
    Pane,
    Tab,
    Scroll,
    Exiting,
}

/// Represents the help message that is printed in the status bar, indicating
/// the current [`InputMode`], whether that mode is persistent, and what the
/// keybinds for that mode are.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Help {
    pub mode: InputMode,
    pub keybinds: Vec<(String, String)>, // <shortcut> => <shortcut description>
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}

/// Creates a [`Help`] struct holding the current [`InputMode`] and its keybinds.
// TODO this should probably be automatically generated in some way
pub fn get_help(mode: InputMode) -> Help {
    let mut keybinds: Vec<(String, String)> = vec![];
    match mode {
        InputMode::Normal | InputMode::Command | InputMode::Exiting => {
            keybinds.push((format!("p"), format!("PANE")));
            keybinds.push((format!("t"), format!("TAB")));
            keybinds.push((format!("r"), format!("RESIZE")));
            keybinds.push((format!("s"), format!("SCROLL")));
        }
        InputMode::Resize => {
            keybinds.push((format!("←↓↑→"), format!("Resize")));
        }
        InputMode::Pane => {
            keybinds.push((format!("←↓↑→"), format!("Move focus")));
            keybinds.push((format!("p"), format!("Next")));
            keybinds.push((format!("n"), format!("New")));
            keybinds.push((format!("d"), format!("Split down")));
            keybinds.push((format!("r"), format!("Split right")));
            keybinds.push((format!("x"), format!("Close")));
            keybinds.push((format!("f"), format!("Fullscreen")));
        }
        InputMode::Tab => {
            keybinds.push((format!("←↓↑→"), format!("Move focus")));
            keybinds.push((format!("n"), format!("New")));
            keybinds.push((format!("x"), format!("Close")));
        }
        InputMode::Scroll => {
            keybinds.push((format!("↓↑"), format!("Scroll")));
        }
    }
    keybinds.push((format!("ESC"), format!("BACK")));
    keybinds.push((format!("q"), format!("QUIT")));
    Help {
        mode,
        keybinds,
    }
}

/// Entry point to the module. Instantiates a new InputHandler and calls its
/// input loop.
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
