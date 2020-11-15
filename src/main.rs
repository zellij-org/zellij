#[cfg(test)]
mod tests;

mod boundaries;
mod input;
mod os_input_output;
mod pty_bus;
mod screen;
mod terminal_pane;

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use crate::input::input_loop;
use crate::os_input_output::{get_os_input, OsApi};
use crate::pty_bus::{PtyBus, PtyInstruction, VteEvent};
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
    max_panes: Option<usize>,

    #[structopt(short, long)]
    debug: bool,
}

fn _debug_log_to_file(message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("/tmp/mosaic-log.txt")
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

fn delete_log_files() -> std::io::Result<()> {
    std::fs::remove_dir_all("/tmp/mosaic-logs").ok();
    std::fs::create_dir_all("/tmp/mosaic-logs").ok();
    Ok(())
}

pub fn main() {
    let opts = Opt::from_args();
    if opts.split.is_some() {
        match opts.split {
            Some('h') => {
                let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitHorizontally).unwrap();
                stream.write_all(&api_command).unwrap();
            }
            Some('v') => {
                let mut stream = UnixStream::connect("/tmp/mosaic").unwrap();
                let api_command = bincode::serialize(&ApiCommand::SplitVertically).unwrap();
                stream.write_all(&api_command).unwrap();
            }
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
        let os_input = get_os_input();
        start(Box::new(os_input), opts);
    }
}

pub enum AppInstruction {
    Exit,
}

pub fn start(mut os_input: Box<dyn OsApi>, opts: Opt) {
    let mut active_threads = vec![];

    delete_log_files().unwrap();

    let full_screen_ws = os_input.get_terminal_size_using_fd(0);
    os_input.into_raw_mode(0);
    let (send_screen_instructions, receive_screen_instructions): (
        Sender<ScreenInstruction>,
        Receiver<ScreenInstruction>,
    ) = channel();
    let (send_pty_instructions, receive_pty_instructions): (
        Sender<PtyInstruction>,
        Receiver<PtyInstruction>,
    ) = channel();
    let (send_app_instructions, receive_app_instructions): (
        Sender<AppInstruction>,
        Receiver<AppInstruction>,
    ) = channel();
    let mut screen = Screen::new(
        receive_screen_instructions,
        send_pty_instructions.clone(),
        send_app_instructions.clone(),
        &full_screen_ws,
        os_input.clone(),
        opts.max_panes,
    );
    let mut pty_bus = PtyBus::new(
        receive_pty_instructions,
        send_screen_instructions.clone(),
        os_input.clone(),
        opts.debug,
    );

    active_threads.push(
        thread::Builder::new()
            .name("pty".to_string())
            .spawn({
                move || {
                    pty_bus.spawn_terminal_vertically(None);
                    loop {
                        let event = pty_bus
                            .receive_pty_instructions
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
            })
            .unwrap(),
    );

    active_threads.push(
        thread::Builder::new()
            .name("screen".to_string())
            .spawn({
                move || loop {
                    let event = screen
                        .receiver
                        .recv()
                        .expect("failed to receive event on channel");
                    match event {
                        ScreenInstruction::Pty(pid, vte_event) => {
                            screen.handle_pty_event(pid, vte_event);
                        }
                        ScreenInstruction::Render => {
                            screen.render();
                        }
                        ScreenInstruction::NewPane(pid) => {
                            screen.new_pane(pid);
                        }
                        ScreenInstruction::HorizontalSplit(pid) => {
                            screen.horizontal_split(pid);
                        }
                        ScreenInstruction::VerticalSplit(pid) => {
                            screen.vertical_split(pid);
                        }
                        ScreenInstruction::WriteCharacter(bytes) => {
                            screen.write_to_active_terminal(bytes);
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
                        ScreenInstruction::ToggleActiveTerminalFullscreen => {
                            screen.toggle_active_terminal_fullscreen();
                        }
                        ScreenInstruction::Quit => {
                            break;
                        }
                    }
                }
            })
            .unwrap(),
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
                let listener = ::std::os::unix::net::UnixListener::bind("/tmp/mosaic")
                    .expect("could not listen on ipc socket");

                for stream in listener.incoming() {
                    match stream {
                        Ok(mut stream) => {
                            let mut buffer = [0; 65535]; // TODO: more accurate
                            stream
                                .read(&mut buffer)
                                .expect("failed to parse ipc message");
                            let decoded: ApiCommand = bincode::deserialize(&buffer)
                                .expect("failed to deserialize ipc message");
                            match &decoded {
                                ApiCommand::OpenFile(file_name) => {
                                    let path = PathBuf::from(file_name);
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminal(Some(path)))
                                        .unwrap();
                                }
                                ApiCommand::SplitHorizontally => {
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminalHorizontally(None))
                                        .unwrap();
                                }
                                ApiCommand::SplitVertically => {
                                    send_pty_instructions
                                        .send(PtyInstruction::SpawnTerminalVertically(None))
                                        .unwrap();
                                }
                                ApiCommand::MoveFocus => {
                                    send_screen_instructions
                                        .send(ScreenInstruction::MoveFocus)
                                        .unwrap();
                                }
                            }
                        }
                        Err(err) => {
                            panic!("err {:?}", err);
                        }
                    }
                }
            }
        })
        .unwrap();

    let _stdin_thread = thread::Builder::new()
        .name("ipc_server".to_string())
        .spawn({
            let send_screen_instructions = send_screen_instructions.clone();
            let send_pty_instructions = send_pty_instructions.clone();
            let send_app_instructions = send_app_instructions.clone();
            let os_input = os_input.clone();
            move || {
                input_loop(
                    os_input,
                    send_screen_instructions,
                    send_pty_instructions,
                    send_app_instructions,
                )
            }
        });

    loop {
        let app_instruction = receive_app_instructions
            .recv()
            .expect("failed to receive app instruction on channel");
        match app_instruction {
            AppInstruction::Exit => {
                let _ = send_screen_instructions.send(ScreenInstruction::Quit);
                let _ = send_pty_instructions.send(PtyInstruction::Quit);
                break;
            }
        }
    }

    for thread_handler in active_threads {
        thread_handler.join().unwrap();
    }
    // cleanup();
    let reset_style = "\u{1b}[m";
    let goto_start_of_last_line = format!("\u{1b}[{};{}H", full_screen_ws.ws_row, 1);
    let goodbye_message = format!(
        "{}\n{}Bye from Mosaic!",
        goto_start_of_last_line, reset_style
    );

    os_input.unset_raw_mode(0);
    os_input
        .get_stdout_writer()
        .write(goodbye_message.as_bytes())
        .unwrap();
    os_input.get_stdout_writer().flush().unwrap();
}
