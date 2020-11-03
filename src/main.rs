#[cfg(test)]
mod tests;

mod os_input_output;
mod terminal_pane;
mod pty_bus;
mod screen;
mod boundaries;
mod utils;

use std::io::{Read, Write};
use std::thread;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender, Receiver};

use serde::{Serialize, Deserialize};
use structopt::StructOpt;

use crate::os_input_output::{get_os_input, OsApi};
use crate::pty_bus::{VteEvent, PtyBus, PtyInstruction};
use crate::screen::{Screen, ScreenInstruction};

#[derive(Serialize, Deserialize, Debug)]
enum ApiCommand {
    OpenFile(PathBuf),
    SplitHorizontally,
    SplitVertically,
    MoveFocus,
}

#[derive(StructOpt, Debug, Default)]
#[structopt(name = "mosaic")]
pub struct Opt {
    #[structopt(short, long)]
    /// Send "split (direction h == horizontal / v == vertical)" to active mosaic session
    split: Option<char>,
    #[structopt(short, long)]
    /// Send "move focused pane" to active mosaic session
    move_focus: bool,
    #[structopt(short, long)]
    /// Send "open file in new pane" to active mosaic session
    open_file: Option<PathBuf>,
    #[structopt(long)]
    /// Maximum panes on screen, caution: opening more panes will close old ones
    max_panes: Option<usize>
}

pub fn main() {
    let os_input = get_os_input();
    let opts = Opt::from_args();
    if opts.split.is_some() {
        match opts.split {
            Some('h') => {
                let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitHorizontally).unwrap();
                stream.write_all(&api_command).unwrap();
            },
            Some('v') => {
                let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitVertically).unwrap();
                stream.write_all(&api_command).unwrap();
            },
            _ => {}
        };
    } else if opts.move_focus {
        let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
        let api_command = bincode::serialize(&ApiCommand::MoveFocus).unwrap();
        stream.write_all(&api_command).unwrap();
    } else if opts.open_file.is_some() {
        let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
        let file_to_open = opts.open_file.unwrap();
        let api_command = bincode::serialize(&ApiCommand::OpenFile(file_to_open)).unwrap();
        stream.write_all(&api_command).unwrap();
    } else {
        start(Box::new(os_input), opts);
    }
}

pub fn start(mut os_input: Box<dyn OsApi>, opts: Opt) {
    let mut active_threads = vec![];

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.into_raw_mode(0);
    let (send_screen_instructions, receive_screen_instructions): (Sender<ScreenInstruction>, Receiver<ScreenInstruction>) = channel();
    let (send_pty_instructions, receive_pty_instructions): (Sender<PtyInstruction>, Receiver<PtyInstruction>) = channel();
    let mut screen = Screen::new(receive_screen_instructions, send_pty_instructions.clone(), &full_screen_ws, os_input.clone(), opts.max_panes);
    let mut pty_bus = PtyBus::new(receive_pty_instructions, send_screen_instructions.clone(), os_input.clone());

    active_threads.push(
        thread::Builder::new()
            .name("pty".to_string())
            .spawn({
                move || {
                    pty_bus.spawn_terminal_vertically(None);
                    loop {
                        let event = pty_bus.receive_pty_instructions
                            .recv()
                            .expect("failed to receive event on channel");
                        match event {
                            PtyInstruction::SpawnTerminal(file_to_open) => {
                                pty_bus.spawn_terminal(file_to_open);
                            }
                            PtyInstruction::SpawnTerminalVertically(file_to_open) => {
                                pty_bus.spawn_terminal_vertically(file_to_open);
                            }
                            PtyInstruction::SpawnTerminalHorizontally(file_to_open) => {
                                pty_bus.spawn_terminal_horizontally(file_to_open);
                            }
                            PtyInstruction::ClosePane(id) => {
                                pty_bus.close_pane(id);
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
                            ScreenInstruction::NewPane(pid) => {
                                screen.new_pane(pid);
                            }
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
                            ScreenInstruction::CloseFocusedPane => {
                                screen.close_focused_pane();
                            }
                            ScreenInstruction::ClosePane(id) => {
                                screen.close_pane(id);
                            }
                            ScreenInstruction::Quit => {
                                break;
                            }
                        }
                    }
                }
            }).unwrap()
    );

    // TODO: currently we don't push this into active_threads
    // because otherwise the app will hang. Need to fix this so it both
    // listens to the ipc-bus and is able to quit cleanly
    #[cfg(not(test))]
    let _ipc_thread = thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            let send_pty_instructions = send_pty_instructions.clone();
            let send_screen_instructions = send_screen_instructions.clone();
            move || {
                ::std::fs::remove_file("/tmp/mosaic").ok();
                let listener = ::std::os::unix::net::UnixListener::bind("/tmp/mosaic").expect("could not listen on ipc socket");

                for stream in listener.incoming() {
                    match stream {
                        Ok(mut stream) => {
                            let mut buffer = [0; 65535]; // TODO: more accurate
                            stream.read(&mut buffer).expect("failed to parse ipc message");
                            let decoded: ApiCommand = bincode::deserialize(&buffer).expect("failed to deserialize ipc message");
                            match &decoded {
                                ApiCommand::OpenFile(file_name) => {
                                    let path = PathBuf::from(file_name);
                                    send_pty_instructions.send(PtyInstruction::SpawnTerminal(Some(path))).unwrap();
                                }
                                ApiCommand::SplitHorizontally => {
                                    send_pty_instructions.send(PtyInstruction::SpawnTerminalHorizontally(None)).unwrap();
                                }
                                ApiCommand::SplitVertically => {
                                    send_pty_instructions.send(PtyInstruction::SpawnTerminalVertically(None)).unwrap();
                                }
                                ApiCommand::MoveFocus => {
                                    send_screen_instructions.send(ScreenInstruction::MoveFocus).unwrap();
                                }
                            }
                        }
                        Err(err) => {
                            panic!("err {:?}", err);
                        }
                    }
                }
            }
        }).unwrap();

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
        } else if buffer[0] == 13 { // ctrl-m
            send_pty_instructions.send(PtyInstruction::SpawnTerminal(None)).unwrap();
        } else if buffer[0] == 14 { // ctrl-n
            send_pty_instructions.send(PtyInstruction::SpawnTerminalVertically(None)).unwrap();
        } else if buffer[0] == 2 { // ctrl-b
            send_pty_instructions.send(PtyInstruction::SpawnTerminalHorizontally(None)).unwrap();
        } else if buffer[0] == 17 { // ctrl-q
            send_screen_instructions.send(ScreenInstruction::Quit).unwrap();
            send_pty_instructions.send(PtyInstruction::Quit).unwrap();
            break;
        } else if buffer[0] == 27 { // ctrl-[
            send_screen_instructions.send(ScreenInstruction::ScrollUp).unwrap();
        } else if buffer[0] == 29 { // ctrl-]
            send_screen_instructions.send(ScreenInstruction::ScrollDown).unwrap();
        } else if buffer[0] == 24 { // ctrl-x
            send_screen_instructions.send(ScreenInstruction::CloseFocusedPane).unwrap();
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
