#[cfg(test)]
mod tests;

mod os_input_output;
mod terminal_pane;
mod pty_bus;
mod screen;
mod boundaries;

use std::io::{Read, Write};
use ::std::thread;

use crate::os_input_output::{get_os_input, OsApi};
use crate::terminal_pane::TerminalOutput;
use crate::pty_bus::{VteEvent, PtyBus, PtyInstruction};
use crate::screen::{Screen, ScreenInstruction};

// sigwinch stuff
use ::signal_hook::iterator::Signals;

pub type OnSigWinch = dyn Fn(Box<dyn Fn()>) + Send;
pub type SigCleanup = dyn Fn() + Send;

fn debug_log_to_file (message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new().append(true).create(true).open("/tmp/mosaic-log.txt").unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

pub fn sigwinch() -> (Box<OnSigWinch>, Box<SigCleanup>) {
    let signals = Signals::new(&[signal_hook::SIGWINCH]).unwrap();
    let on_winch = {
        let signals = signals.clone();
        move |cb: Box<dyn Fn()>| {
            for signal in signals.forever() {
                match signal {
                    signal_hook::SIGWINCH => cb(),
                    _ => unreachable!(),
                }
            }
        }
    };
    let cleanup = move || {
        signals.close();
    };
    (Box::new(on_winch), Box::new(cleanup))
}

pub fn main() {
    let os_input = get_os_input();
    start(Box::new(os_input));
}

pub fn start(mut os_input: Box<dyn OsApi>) {
    let mut active_threads = vec![];

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.into_raw_mode(0);
    let mut screen = Screen::new(&full_screen_ws, os_input.clone());
    let send_screen_instructions = screen.send_screen_instructions.clone();
    let mut pty_bus = PtyBus::new(send_screen_instructions.clone(), os_input.clone());
    let send_pty_instructions = pty_bus.send_pty_instructions.clone();

    active_threads.push(
        thread::Builder::new()
            .name("pty".to_string())
            .spawn({
                move || {
                    pty_bus.spawn_terminal_vertically();
                    loop {
                        let event = pty_bus.receive_pty_instructions
                            .recv()
                            .expect("failed to receive event on channel");
                        match event {
                            PtyInstruction::SpawnTerminalVertically => {
                                pty_bus.spawn_terminal_vertically();
                            }
                            PtyInstruction::SpawnTerminalHorizontally => {
                                pty_bus.spawn_terminal_horizontally();
                            }
                            PtyInstruction::Quit => {
                                break;
                            }
                        }
                    }
                }
            }).unwrap()
    );

    active_threads.push(
        thread::Builder::new()
            .name("screen".to_string())
            .spawn({
                move || {
                    loop {
                        let event = screen.receiver
                            .recv()
                            .expect("failed to receive event on channel");
                        match event {
                            ScreenInstruction::Pty(pid, vte_event) => {
                                screen.handle_pty_event(pid, vte_event);
                            },
                            ScreenInstruction::Render => {
                                screen.render();
                            },
                            ScreenInstruction::HorizontalSplit(pid) => {
                                screen.horizontal_split(pid);
                            }
                            ScreenInstruction::VerticalSplit(pid) => {
                                screen.vertical_split(pid);
                            }
                            ScreenInstruction::WriteCharacter(byte) => {
                                screen.write_to_active_terminal(byte);
                            }
                            ScreenInstruction::ResizeLeft => {
                                screen.resize_left();
                            }
                            ScreenInstruction::ResizeRight => {
                                screen.resize_right();
                            }
                            ScreenInstruction::ResizeDown => {
                                screen.resize_down();
                            }
                            ScreenInstruction::ResizeUp => {
                                screen.resize_up();
                            }
                            ScreenInstruction::MoveFocus => {
                                screen.move_focus();
                            }
                            ScreenInstruction::ScrollUp => {
                                screen.scroll_active_terminal_up();
                            }
                            ScreenInstruction::ScrollDown => {
                                screen.scroll_active_terminal_down();
                            }
                            ScreenInstruction::ClearScroll => {
                                screen.clear_active_terminal_scroll();
                            }
                            ScreenInstruction::Quit => {
                                break;
                            }
                        }
                    }
                }
            }).unwrap()
    );

    let mut stdin = os_input.get_stdin_reader();
    loop {
		let mut buffer = [0; 1];
        stdin.read(&mut buffer).expect("failed to read stdin");
        if buffer[0] == 10 { // ctrl-j
            send_screen_instructions.send(ScreenInstruction::ResizeDown).unwrap();
        } else if buffer[0] == 11 { // ctrl-k
            send_screen_instructions.send(ScreenInstruction::ResizeUp).unwrap();
        } else if buffer[0] == 16 { // ctrl-p
            send_screen_instructions.send(ScreenInstruction::MoveFocus).unwrap();
        } else if buffer[0] == 8 { // ctrl-h
            send_screen_instructions.send(ScreenInstruction::ResizeLeft).unwrap();
        } else if buffer[0] == 12 { // ctrl-l
            send_screen_instructions.send(ScreenInstruction::ResizeRight).unwrap();
        } else if buffer[0] == 14 { // ctrl-n
            send_pty_instructions.send(PtyInstruction::SpawnTerminalVertically).unwrap();
        } else if buffer[0] == 2 { // ctrl-b
            send_pty_instructions.send(PtyInstruction::SpawnTerminalHorizontally).unwrap();
        } else if buffer[0] == 17 { // ctrl-q
            send_screen_instructions.send(ScreenInstruction::Quit).unwrap();
            send_pty_instructions.send(PtyInstruction::Quit).unwrap();
            break;
        } else if buffer[0] == 27 { // ctrl-[
            send_screen_instructions.send(ScreenInstruction::ScrollUp).unwrap();
        } else if buffer[0] == 29 { // ctrl-]
            send_screen_instructions.send(ScreenInstruction::ScrollDown).unwrap();
        } else {
            // println!("\r buffer {:?}   ", buffer[0]);
            send_screen_instructions.send(ScreenInstruction::ClearScroll).unwrap();
            send_screen_instructions.send(ScreenInstruction::WriteCharacter(buffer[0])).unwrap();
        }
    };
    
    for thread_handler in active_threads {
        thread_handler.join().unwrap();
    }
    // cleanup();
    let reset_style = "\u{1b}[m";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.ws_row, 1);
    let goodbye_message = format!("{}\n{}Bye from Mosaic!", goto_start_of_last_line, reset_style);

    os_input.get_stdout_writer().write(goodbye_message.as_bytes()).unwrap();
    os_input.get_stdout_writer().flush().unwrap();
}
