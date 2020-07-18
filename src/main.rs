use std::{mem, io};
use std::cmp::max;
use std::io::{stdin, stdout, Read, Write};
use std::collections::{HashMap, HashSet, VecDeque};
use nix::unistd::{read, write, ForkResult};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::termios::SpecialCharacterIndices::{VMIN, VTIME};
use nix::sys::termios::{
    tcgetattr,
    cfmakeraw,
    tcsetattr,
    SetArg,
    tcdrain,
    tcflush,
    FlushArg,
    cfsetispeed,
    cfsetospeed,
    BaudRate,
    InputFlags,
};
use nix::pty::{forkpty, Winsize};
use std::os::unix::io::{RawFd, FromRawFd};
use std::process::Command;
use ::std::{thread, time};
use ::std::fs::File;
use ::std::io::prelude::*;
use ::std::sync::{Arc, Mutex};

use unicode_width::UnicodeWidthStr;
use unicode_truncate::UnicodeTruncateStr;
use vte;

// fn read_from_pid (pid: RawFd) -> (usize, [u8; 115200]) {
fn read_from_pid (pid: RawFd) -> Option<Vec<u8>> {
    let mut read_buffer = [0; 115200];
    let read_result = read(pid, &mut read_buffer);
    match read_result {
        Ok(res) => {
            Some(read_buffer[..=res].to_vec())
            // (res, read_buffer)
        },
        Err(e) => {
            match e {
                nix::Error::Sys(errno) => {
                    if errno == nix::errno::Errno::EAGAIN {
                        None
                        // (0, read_buffer)
                    } else {
                        panic!("error {:?}", e);
                    }
                },
                _ => panic!("error {:?}", e)
            }
        }
    }
}

fn into_raw_mode(pid: RawFd) {
    let mut tio = tcgetattr(pid).expect("could not get terminal attribute");
    cfmakeraw(&mut tio);
    match tcsetattr(pid, SetArg::TCSANOW, &mut tio) {
        Ok(_) => {},
        Err(e) => panic!("error {:?}", e)
    };

}

fn set_baud_rate(pid: RawFd) {
    let mut tio = tcgetattr(pid).expect("could not get terminal attribute");
    cfsetospeed(&mut tio, BaudRate::B115200).expect("could not set baud rate");
    cfsetispeed(&mut tio, BaudRate::B115200).expect("could not set baud rate");
    tcsetattr(pid, SetArg::TCSANOW, &mut tio).expect("could not set attributes");
}

pub fn get_terminal_size_using_fd(fd: RawFd) -> Winsize {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCGWINSZ;

    let mut winsize = Winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    unsafe { ioctl(fd, TIOCGWINSZ.into(), &mut winsize) };
    winsize
}

pub fn set_terminal_size_using_fd(fd: RawFd, ws: &Winsize) {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCSWINSZ;

    let winsize = Winsize {
        ws_row: ws.ws_row,
        ws_col: ws.ws_col,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { ioctl(fd, TIOCSWINSZ.into(), &winsize) };
}


fn spawn_terminal () -> (RawFd, RawFd, Winsize) {
    let ws = get_terminal_size_using_fd(0);
    let (pid_primary, pid_secondary): (RawFd, RawFd) = {
        match forkpty(Some(&ws), None) {
            Ok(fork_pty_res) => {
                let pid_primary = fork_pty_res.master;
                let pid_secondary = match fork_pty_res.fork_result {
                    ForkResult::Parent { child } => {
                        fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("could not fcntl");
                        child
                    },
                    ForkResult::Child => {
                        // TODO: why does $SHELL not work?
                        // Command::new("$SHELL").spawn().expect("failed to spawn");
                        set_baud_rate(0);
                        set_terminal_size_using_fd(0, &ws);
                        Command::new("/usr/bin/fish").spawn().expect("failed to spawn");
                        ::std::thread::sleep(std::time::Duration::from_millis(300000));
                        panic!("I am secondary, why?!");
                    },
                };
                (pid_primary, pid_secondary.as_raw())
            }
            Err(e) => {
                panic!("failed to fork {:?}", e);
            }
        }
    };
    (pid_primary, pid_secondary, ws)
}

struct TerminalOutput {
    pub characters: Vec<char>, // we use a vec rather than a string here because one char can take up multiple characters in a string
    current_index_in_characters: usize,
    cursor_position: usize,
    newline_indices: HashSet<usize>,
    linebreak_indices: HashSet<usize>,
    display_rows: u16,
    display_cols: u16,
    unhandled_ansi_codes: HashMap<usize, String>,
}

impl TerminalOutput {
    pub fn new () -> TerminalOutput {
        TerminalOutput {
            characters: vec![],
            current_index_in_characters: 0,
            cursor_position: 0,
            newline_indices: HashSet::new(),
            linebreak_indices: HashSet::new(),
            display_rows: 0,
            display_cols: 0,
            unhandled_ansi_codes: HashMap::new()
        }
    }
    pub fn set_size(&mut self, ws: &Winsize) {
        let orig_cols = self.display_cols;
        self.display_rows = ws.ws_row;
        self.display_cols = ws.ws_col;
        if orig_cols != self.display_cols && orig_cols != 0 {
            self.reflow_lines();
        }
    }
    fn reflow_lines (&mut self) {
        self.linebreak_indices.clear();
        let mut x = 0;
        for (i, _c) in self.characters.iter().enumerate() {
            if self.newline_indices.contains(&i) {
                x = 0;
            } else if x == self.display_cols && i < self.cursor_position {
                self.linebreak_indices.insert(i);
                x = 0;
            }
            x += 1;
        }
    }
    pub fn read_buffer_as_lines (&mut self) -> String {
        let mut output = String::new();

        for (i, c) in self.characters.iter().enumerate() {
            if self.newline_indices.contains(&i) || self.linebreak_indices.contains(&i) {
                output.push('\r');
                output.push('\n');
            }
            if let Some(code) = self.unhandled_ansi_codes.get(&i) {
                output.push_str(code);
            }
            output.push(*c);
        }

        self.current_index_in_characters = self.characters.len();
        if self.cursor_position < self.characters.len() {
            let start_of_last_line = self.index_of_beginning_of_last_line();
            let difference_from_last_newline = self.cursor_position - start_of_last_line;
            output.push_str(&format!("\r\u{1b}[{}C", difference_from_last_newline));
        }
        output
    }
    fn index_of_beginning_of_last_line (&self) -> usize {
        let last_newline_index = if self.newline_indices.is_empty() {
            None
        } else {
            // return last
            Some(self.newline_indices.iter().fold(0, |acc, i| if acc > *i { acc } else { *i })) // TODO: better?
        };
        let last_linebreak_index = if self.linebreak_indices.is_empty() {
            None
        } else {
            // return last
            Some(self.linebreak_indices.iter().fold(0, |acc, i| if acc > *i { acc } else { *i })) // TODO: better?
        };
        match (last_newline_index, last_linebreak_index) {
            (Some(last_newline_index), Some(last_linebreak_index)) => {
                max(last_newline_index, last_linebreak_index)
            },
            (None, Some(last_linebreak_index)) => last_linebreak_index,
            (Some(last_newline_index), None) => last_newline_index,
            (None, None) => 0
        }
    }
    fn add_newline (&mut self) {
        self.newline_indices.insert(self.characters.len()); // -1?
        self.cursor_position = self.characters.len(); // -1?
    }
    fn move_to_beginning_of_line (&mut self) {
        // TODO: this always moves to the beginning of the non-wrapped line... is this the right
        // behaviour?
        let last_newline_index = if self.newline_indices.is_empty() {
            0
        } else {
            // return last
            self.newline_indices.iter().fold(0, |acc, i| if acc > *i { acc } else { *i }) // TODO: better?
        };
        self.cursor_position = last_newline_index;
    }
}

const DEBUGGING: bool = false;

impl vte::Perform for TerminalOutput {
    fn print(&mut self, c: char) {
        if DEBUGGING {
            println!("\r[print] {:?}", c);
        } else {
            if self.characters.len() == self.cursor_position {
                self.characters.push(c);
            } else if self.characters.len() > self.cursor_position {
                self.characters.splice(self.cursor_position..=self.cursor_position, [c].iter().copied()); // TODO: better
            } else {
                for _ in self.characters.len()..self.cursor_position {
                    self.characters.push(' ');
                };
                self.characters.push(c);
            }

            let start_of_last_line = self.index_of_beginning_of_last_line();
            let difference_from_last_newline = self.cursor_position - start_of_last_line;
            if difference_from_last_newline == self.display_cols as usize {
                self.linebreak_indices.insert(self.cursor_position);
            }
            self.cursor_position += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        if DEBUGGING {
            if byte == 13 { // 0d, carriage return
                println!("\rEXECUTE CARRIAGE RETURN");
            } else if byte == 10 { // 0a, newline
                println!("\rEXECUTE NEW LINE");
            } else if byte == 08 { // backspace
                println!("\rEXECUTE BACKSPACE");
            } else {
                println!("\r[execute] {:02x}", byte);
            }
        } else {
            if byte == 13 { // 0d, carriage return
                self.move_to_beginning_of_line();
            } else if byte == 08 { // backspace
                self.cursor_position -= 1;
                self.unhandled_ansi_codes.remove(&self.cursor_position);
            } else if byte == 10 { // 0a, newline
                self.add_newline();
            }
        }
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        if DEBUGGING {
            println!(
                "\r[hook] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
                params, intermediates, ignore, c
            );
        }
    }

    fn put(&mut self, byte: u8) {
        if DEBUGGING {
            println!("\r[put] {:02x}", byte);
        }
    }

    fn unhook(&mut self) {
        if DEBUGGING {
            println!("\r[unhook]");
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        if DEBUGGING {
            println!("\r[osc_dispatch] params={:?} bell_terminated={}", params, bell_terminated);
        }
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        if DEBUGGING {
            println!(
                "\r[csi_dispatch] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
                params, intermediates, ignore, c
            );
        } else {
            if c == 'm' {
                // change foreground color (only?)
                if params.len() == 1 && params[0] == 0 {
                    // eg. \u{1b}[m
                    self.unhandled_ansi_codes.insert(self.cursor_position, String::from("\u{1b}[m"));
                } else {
                    // eg. \u{1b}[38;5;0m
                    let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                    self.unhandled_ansi_codes.insert(self.cursor_position, format!("\u{1b}[{}m", param_string));
                }
            } else if c == 'C' { // move cursor
                self.cursor_position += params[0] as usize; // TODO: negative value?
            } else if c == 'K' { // clear line (0 => right, 1 => left, 2 => all)
                if params[0] == 0 {
                    for i in self.cursor_position..self.characters.len() {
                        self.characters[i] = ' ';
                        self.unhandled_ansi_codes.remove(&i);
                    }
                }
                // TODO: implement 1 and 2
            } else if c == 'J' { // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
                if params[0] == 0 {
                    for i in self.cursor_position..self.characters.len() {
                        self.characters[i] = ' ';
                        self.unhandled_ansi_codes.remove(&i);
                    }
                }
                // TODO: implement 1, 2, and 3
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if DEBUGGING {
            println!(
                // "\r[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:02x}",
                "\r[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:?}",
                intermediates, ignore, byte
            );
        }
    }
}

// sigwinch stuff
use ::signal_hook::iterator::Signals;

pub type OnSigWinch = dyn Fn(Box<dyn Fn()>) + Send;
pub type SigCleanup = dyn Fn() + Send;

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

fn main() {
    let mut active_threads = vec![];

    let (first_terminal_pid, pid_secondary, mut first_terminal_ws): (RawFd, RawFd, Winsize) = spawn_terminal();
    let (second_terminal_pid, pid_secondary, second_terminal_ws): (RawFd, RawFd, Winsize) = spawn_terminal();
    let stdin = io::stdin();
    into_raw_mode(0);
    set_baud_rate(0);
    ::std::thread::sleep(std::time::Duration::from_millis(2000));
    let active_terminal = Arc::new(Mutex::new(first_terminal_pid));

    let first_terminal_ws = Arc::new(Mutex::new(first_terminal_ws));
    let second_terminal_ws = Arc::new(Mutex::new(second_terminal_ws));

    let terminal1_output = Arc::new(Mutex::new(TerminalOutput::new()));
    let terminal2_output = Arc::new(Mutex::new(TerminalOutput::new()));

    active_threads.push(
        thread::Builder::new()
            .name("terminal_stdout_handler".to_string())
            .spawn({

                let mut vte_parser = vte::Parser::new();

                let terminal1_output = terminal1_output.clone();
                let first_terminal_ws = first_terminal_ws.clone();
                move || {
                    let mut buffer_has_unread_data = true;
                    loop {
                        match read_from_pid(first_terminal_pid) {
                            Some(read_bytes) => {
                                if DEBUGGING {
                                    println!("\n\rread_bytes: {:?}", String::from_utf8(read_bytes.to_vec()).unwrap());
                                }
                                for byte in read_bytes.iter() {
                                    let first_terminal_ws = first_terminal_ws.lock().unwrap();
                                    let mut terminal1_output = terminal1_output.lock().unwrap();
                                    terminal1_output.set_size(&*first_terminal_ws);
                                    vte_parser.advance(&mut *terminal1_output, *byte);
                                }
                                buffer_has_unread_data = true;
                            },
                            None => {
                                if DEBUGGING {
                                    buffer_has_unread_data = false;
                                } else if buffer_has_unread_data {
                                    let mut terminal1_output = terminal1_output.lock().unwrap();
                                    let data_lines = terminal1_output.read_buffer_as_lines();
                                    print!("\u{1b}c"); // clear screen
                                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                                    ::std::io::stdout().flush().expect("could not flush");
                                    buffer_has_unread_data = false;
                                }
                                ::std::thread::sleep(std::time::Duration::from_millis(50)); // TODO: adjust this
                            }
                        }
                    }
                }
            })
            .unwrap(),
    );
    active_threads.push(
        thread::Builder::new()
            .name("terminal_stdout_handler2".to_string())
            .spawn({

                let mut vte_parser = vte::Parser::new();

                let terminal2_output = terminal2_output.clone();
                let second_terminal_ws = second_terminal_ws.clone();
                move || {
                    let mut buffer_has_unread_data = true;
                    loop {
                        match read_from_pid(second_terminal_pid) {
                            Some(read_bytes) => {
                                if DEBUGGING {
                                    println!("\n\rread_bytes: {:?}", String::from_utf8(read_bytes.to_vec()).unwrap());
                                }
                                for byte in read_bytes.iter() {
                                    let second_terminal_ws = second_terminal_ws.lock().unwrap();
                                    let mut terminal2_output = terminal2_output.lock().unwrap();
                                    terminal2_output.set_size(&*second_terminal_ws);
                                    vte_parser.advance(&mut *terminal2_output, *byte);
                                }
                                buffer_has_unread_data = true;
                            },
                            None => {
                                if DEBUGGING {
                                    buffer_has_unread_data = false;
                                } else if buffer_has_unread_data {
                                    let mut terminal2_output = terminal2_output.lock().unwrap();
                                    let data_lines = terminal2_output.read_buffer_as_lines();
                                    print!("\u{1b}c"); // clear screen
                                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                                    ::std::io::stdout().flush().expect("could not flush");
                                    buffer_has_unread_data = false;
                                }
                                ::std::thread::sleep(std::time::Duration::from_millis(50)); // TODO: adjust this
                            }
                        }
                    }
                }
            })
            .unwrap(),
    );

    let mut temp_ws = get_terminal_size_using_fd(0);
    loop {
		let mut buffer = [0; 1];
        {
            let mut handle = stdin.lock();
            handle.read(&mut buffer).expect("failed to read stdin");
            if buffer[0] == 10 { // ctrl-j
                let active_terminal = active_terminal.lock().unwrap();
                temp_ws.ws_col -= 10;
                if *active_terminal == first_terminal_pid {
                    let mut first_terminal_ws = first_terminal_ws.lock().unwrap();
                    *first_terminal_ws = temp_ws;

                    let mut terminal1_output = terminal1_output.lock().unwrap();
                    terminal1_output.set_size(&*first_terminal_ws);
                    let data_lines = terminal1_output.read_buffer_as_lines();
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                    set_terminal_size_using_fd(*active_terminal, &temp_ws);
                } else {
                    let mut second_terminal_ws = second_terminal_ws.lock().unwrap();
                    *second_terminal_ws = temp_ws;

                    let mut terminal2_output = terminal2_output.lock().unwrap();
                    terminal2_output.set_size(&*second_terminal_ws);
                    let data_lines = terminal2_output.read_buffer_as_lines();
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                    set_terminal_size_using_fd(*active_terminal, &temp_ws);
                }
                continue;
            } else if buffer[0] == 11 { // ctrl-k
                let active_terminal = active_terminal.lock().unwrap();
                temp_ws.ws_col += 10;
                if *active_terminal == first_terminal_pid {
                    let mut first_terminal_ws = first_terminal_ws.lock().unwrap();
                    *first_terminal_ws = temp_ws;

                    set_terminal_size_using_fd(*active_terminal, &temp_ws);
                    let mut terminal1_output = terminal1_output.lock().unwrap();
                    terminal1_output.set_size(&*first_terminal_ws);
                    let data_lines = terminal1_output.read_buffer_as_lines();
                    print!("\u{1b}c"); // clear screen
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");

                } else {
                    let mut second_terminal_ws = second_terminal_ws.lock().unwrap();
                    *second_terminal_ws = temp_ws;

                    let mut terminal2_output = terminal2_output.lock().unwrap();
                    terminal2_output.set_size(&*second_terminal_ws);
                    let data_lines = terminal2_output.read_buffer_as_lines();
                    print!("\u{1b}c"); // clear screen
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                    set_terminal_size_using_fd(*active_terminal, &temp_ws);
                }
                continue;
            } else if buffer[0] == 16 { // ctrl-p
                let mut active_terminal = active_terminal.lock().unwrap();
                if *active_terminal == first_terminal_pid {
                    *active_terminal = second_terminal_pid;
                    let mut terminal2_output = terminal2_output.lock().unwrap();
                    let data_lines = terminal2_output.read_buffer_as_lines();
                    print!("\u{1b}c"); // clear screen
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                } else {
                    *active_terminal = first_terminal_pid;

                    let mut terminal1_output = terminal1_output.lock().unwrap();
                    let data_lines = terminal1_output.read_buffer_as_lines();
                    print!("\u{1b}c"); // clear screen
                    ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                }
                continue;
            }
        }
        let active_terminal = active_terminal.lock().unwrap();
        write(*active_terminal, &mut buffer).expect("failed to write to terminal");
        tcdrain(*active_terminal).expect("failed to drain terminal");
    };
//    cleanup();
    
//    for thread_handler in active_threads {
//        thread_handler.join().unwrap();
//    }

}
