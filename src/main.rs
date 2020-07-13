use std::{mem, io};
use std::io::{stdin, stdout, Read, Write};
use std::collections::VecDeque;
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

fn change_vmin_and_vtime(pid: RawFd) {
    let mut tio = tcgetattr(pid).expect("could not get terminal attribute");
    // tio.control["VMIN"] = 1;
    tio.control_chars[VMIN as usize] = 0;
    tio.control_chars[VTIME as usize] = 0;
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

    let mut winsize = Winsize {
        ws_row: ws.ws_row,
        ws_col: ws.ws_col,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { ioctl(fd, TIOCSWINSZ.into(), &winsize) };
}


fn spawn_terminal () -> (RawFd, RawFd, Winsize) {
    // let ws = Winsize { ws_row: 11, ws_col: 116, ws_xpixel: 0, ws_ypixel: 0 };
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

fn to_utf8_lines(buf: &[u8]) -> Vec<String> {
    let buf_utf8 = String::from_utf8(buf.to_vec()).unwrap();
    let mut lines: Vec<String> = buf_utf8.lines().map(|l| l.to_string()).collect();
    for i in 0..lines.len() - 1 {
//        lines[i].push('\r');
//        lines[i].push('\n'); // TODO: remove these?
    }
    lines
}


/// A type implementing Perform that just logs actions
struct CharacterCounter {
    pub characters: u16
}

impl CharacterCounter {
    pub fn new() -> CharacterCounter {
        CharacterCounter {
            characters: 0
        }
    }
}

impl vte::Perform for CharacterCounter {
    fn print(&mut self, c: char) {
        self.characters += 1;
        // println!("[print] {:?}", c);
    }

    fn execute(&mut self, byte: u8) {
        // println!("[execute] {:02x}", byte);
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
//        println!(
//            "[hook] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
//            params, intermediates, ignore, c
//        );
    }

    fn put(&mut self, byte: u8) {
        // println!("[put] {:02x}", byte);
    }

    fn unhook(&mut self) {
        // println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        // println!("[osc_dispatch] params={:?} bell_terminated={}", params, bell_terminated);
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
//        println!(
//            "[csi_dispatch] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
//            params, intermediates, ignore, c
//        );
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
//        println!(
//            "[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:02x}",
//            intermediates, ignore, byte
//        );
    }
}


fn wrap_row (row: &str, columns: u16) -> Vec<String> {
    // TODO:
    // * create a new character_counter
    // * loop through characters in row
    // * add each character to the character_counter
    // * once the counter reaches columns, push line into lines
    let row = row.as_bytes();
    let mut wrapped_lines = vec![];
    let mut vte_parser = vte::Parser::new();
    let mut character_counter = CharacterCounter::new();
    let mut index_in_row = 0;
    let mut line = vec![];
    loop {
        let character = row[index_in_row];
        line.push(character);
        vte_parser.advance(&mut character_counter, character);
        if character_counter.characters == columns * (wrapped_lines.len() as u16 + 1) {
            let mut string_line = String::from_utf8(line.clone()).expect("could not create utf8 string");
//            string_line.push('\r');
//            string_line.push('\n');
            wrapped_lines.push(string_line);
            line.clear();
        }
        if index_in_row == row.len() - 1 {
            if line.len() > 0 {
                let mut string_line = String::from_utf8(line.clone()).expect("could not create utf8 string");
                // string_line.push('\n');
                // string_line.push('\r');
                wrapped_lines.push(string_line);
                line.clear(); // TODO: we don't need this?
            }
            break;
        }
        index_in_row += 1;
    }
    // println!("\rwrapped_lines {:?}", wrapped_lines);
    wrapped_lines




//    let mut wrapped_lines = vec![];
//    let mut line = String::new();
//    let mut index_in_row = 0;
//    loop {
//        let (line, w) = &row.get(index_in_row..).unwrap().unicode_truncate(columns as usize);
//        // let rest_of_line = &row.get(line.chars().count()..).unwrap();
//        index_in_row += line.chars().count();
//        let mut wrapped_line = String::from(*line);
//        if index_in_row < row.chars().count() {
//            // this is not the last row, so add newline
//            wrapped_line.push('\r');
//            wrapped_line.push('\n');
//        }
//        // wrapped_lines.push(String::from(*line));
//        wrapped_lines.push(wrapped_line);
//        if index_in_row >= row.chars().count() {
//            break;
//        }
//    }
//    wrapped_lines
}

fn lines_in_buffer(buffer: &Vec<String>, ws: &Winsize) -> Vec<u8> {
    let column_count = ws.ws_col;
    let row_count = ws.ws_row;
    let mut rows = VecDeque::new();

    if buffer.is_empty() {
        return vec![]
    };
    let carriage_return = String::from("\r");
    let mut index_in_buffer = buffer.len() - 1;
    loop {
        if rows.len() >= row_count as usize {
            break;
        }
        let current_row = &buffer[index_in_buffer];
        let mut current_row_wrapped = wrap_row(current_row, column_count);
        current_row_wrapped.reverse();
        for mut line in current_row_wrapped {
            if rows.len() < row_count as usize {
                line.push('\r');
                line.push('\n');
                rows.push_front(line);
            }
        }
        index_in_buffer -= 1;
        if index_in_buffer == 0 {
            break;
        }
        // rows.push_front(String::from("\r\n"));
    }
    let rows_length = rows.len();
    rows[rows_length - 1].pop(); // remove last \n (ugly hack, TODO better)
//    println!("\rrow_count, rows.len {:?}, {:?}", row_count, rows.len());
//    for row in rows {
//        println!("\rrow: {:?}", row);
//    }
//    ::std::process::exit(2);

    rows.push_front(carriage_return); // TODO: ??
    let bytes: Vec<u8> = rows.iter().fold(vec![], |mut acc, l| {
        for byte in l.as_bytes() {
            acc.push(*byte)
        }
        acc
    });
    bytes
}

fn create_empty_lines(ws: &Winsize) -> Vec<u8> {
    let columns = ws.ws_col;
    let rows = ws.ws_row;
    let mut lines = vec![];
    let carriage_return = String::from("\r");
    lines.append(carriage_return.as_bytes().to_vec().as_mut());
    let mut empty_line = String::new();
    let empty_char = ' ';
    // for _i in 0..columns - 1 {
    for _i in 0..columns {
        empty_line.push(empty_char);
    }
    empty_line.push('\n');
    for _i in 0..rows {
        let mut line = vec![];
        let carriage_return = String::from("\r");
        line.append(carriage_return.as_bytes().to_vec().as_mut());
        line.append(empty_line.as_bytes().to_vec().as_mut());
        lines.append(&mut line);
    }
    lines
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

    let (first_terminal_pid, pid_secondary, first_terminal_ws): (RawFd, RawFd, Winsize) = spawn_terminal();
    let (second_terminal_pid, pid_secondary, second_terminal_ws): (RawFd, RawFd, Winsize) = spawn_terminal();
    let stdin = io::stdin();
    into_raw_mode(0);
    set_baud_rate(0);
    ::std::thread::sleep(std::time::Duration::from_millis(2000));
    let active_terminal = Arc::new(Mutex::new(first_terminal_pid));
    let terminal1_buffer: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let terminal2_buffer: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
    let first_terminal_ws = Arc::new(Mutex::new(first_terminal_ws));
    let second_terminal_ws = Arc::new(Mutex::new(second_terminal_ws));
    active_threads.push(
        thread::Builder::new()
            .name("terminal_stdout_handler".to_string())
            .spawn({
                let active_terminal = active_terminal.clone();
                let terminal1_buffer = terminal1_buffer.clone();
                move || {
                    let mut read_buffer = vec![];
                    loop {
                        match read_from_pid(first_terminal_pid) {
                            Some(mut read_bytes) => {
                                read_buffer.append(&mut read_bytes);
                            },
                            None => {
                                if read_buffer.len() > 0 {
                                    {
                                        let mut terminal1_buffer = terminal1_buffer.lock().unwrap();
                                        let mut lines = to_utf8_lines(&read_buffer);
                                        terminal1_buffer.append(&mut lines);
                                    }
                                    {
                                        let active_terminal = active_terminal.lock().unwrap();
                                        if *active_terminal == first_terminal_pid {
                                            ::std::io::stdout().write_all(&read_buffer).expect("cannot write to stdout");
                                            ::std::io::stdout().flush().expect("could not flush");
                                        }
                                    }
                                    read_buffer.clear();
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
                let active_terminal = active_terminal.clone();
                let terminal2_buffer = terminal2_buffer.clone();
                move || {
                    let mut read_buffer = vec![];
                    loop {
                        match read_from_pid(second_terminal_pid) {
                            Some(mut read_bytes) => {
                                read_buffer.append(&mut read_bytes);
                            },
                            None => {
                                if read_buffer.len() > 0 {
                                    {
                                        let mut terminal2_buffer = terminal2_buffer.lock().unwrap();
                                        let mut lines = to_utf8_lines(&read_buffer);
                                        terminal2_buffer.append(&mut lines);
                                    }
                                    {
                                        let active_terminal = active_terminal.lock().unwrap();
                                        if *active_terminal == second_terminal_pid {
                                            ::std::io::stdout().write_all(&read_buffer).expect("cannot write to stdout");
                                            ::std::io::stdout().flush().expect("could not flush");
                                        }
                                    }
                                    read_buffer.clear();
                                }
                                ::std::thread::sleep(std::time::Duration::from_millis(50)); // TODO: adjust this
                            }
                        }
                    }
                }
            })
            .unwrap(),
    );
    let (on_sigwinch, cleanup) = sigwinch();
    active_threads.push(
        thread::Builder::new()
            .name("resize_handler".to_string())
            .spawn({
                let active_terminal = active_terminal.clone();
                let first_terminal_ws = first_terminal_ws.clone();
                let second_terminal_ws = second_terminal_ws.clone();
                let terminal1_buffer = terminal1_buffer.clone();
                let terminal2_buffer = terminal2_buffer.clone();
                move || {
                    on_sigwinch(Box::new(move || {
                        let active_terminal = active_terminal.lock().unwrap();
                        let ws = get_terminal_size_using_fd(0);

                        let empty_lines = create_empty_lines(&ws);


                        set_terminal_size_using_fd(*active_terminal, &ws);
                        if *active_terminal == first_terminal_pid {
                            let mut first_terminal_ws = first_terminal_ws.lock().unwrap();
                            *first_terminal_ws = ws;

                            let terminal1_buffer = terminal1_buffer.lock().unwrap();
                            let new_lines = lines_in_buffer(&*terminal1_buffer, &ws);

                            ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                            ::std::io::stdout().write_all(&new_lines).expect("cannot write to stdout");
                            ::std::io::stdout().flush().expect("could not flush");
                        } else {
                            let mut second_terminal_ws = second_terminal_ws.lock().unwrap();
                            *second_terminal_ws = ws;

                            let terminal2_buffer = terminal2_buffer.lock().unwrap();
                            let new_lines = lines_in_buffer(&*terminal2_buffer, &ws);

                            ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                            ::std::io::stdout().write_all(&new_lines).expect("cannot write to stdout");
                            ::std::io::stdout().flush().expect("could not flush");
                        }
                    }));
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
                let mut active_terminal = active_terminal.lock().unwrap();
                temp_ws.ws_col -= 10;
                let empty_lines = create_empty_lines(&temp_ws);
                if *active_terminal == first_terminal_pid {
                    let mut first_terminal_ws = first_terminal_ws.lock().unwrap();
                    *first_terminal_ws = temp_ws;

                    let terminal1_buffer = terminal1_buffer.lock().unwrap();
                    let new_lines = lines_in_buffer(&*terminal1_buffer, &temp_ws);

                    ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                    ::std::io::stdout().write_all(&new_lines).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");

                    set_terminal_size_using_fd(*active_terminal, &temp_ws);
                } else {
                    panic!("not terminal 1");
                }
                continue;
            } else if buffer[0] == 11 { // ctrl-k
                let mut active_terminal = active_terminal.lock().unwrap();
                temp_ws.ws_col += 10;
                let empty_lines = create_empty_lines(&temp_ws);
                if *active_terminal == first_terminal_pid {
                    let mut first_terminal_ws = first_terminal_ws.lock().unwrap();
                    *first_terminal_ws = temp_ws;

                    let terminal1_buffer = terminal1_buffer.lock().unwrap();
                    let new_lines = lines_in_buffer(&*terminal1_buffer, &temp_ws);

                    ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                    ::std::io::stdout().write_all(&new_lines).expect("cannot write to stdout");
                    ::std::io::stdout().flush().expect("could not flush");
                } else {
                    panic!("not terminal 1");
                }
                continue;
            } else if buffer[0] == 16 { // ctrl-p
                let mut active_terminal = active_terminal.lock().unwrap();
                if *active_terminal == first_terminal_pid {
                    *active_terminal = second_terminal_pid;
                    // TODO: this is actually not correct: we need to use the first terminal width to
                    // clear and the second terminal width to write
                    let first_terminal_ws = first_terminal_ws.lock().unwrap();

                    let empty_lines = create_empty_lines(&*first_terminal_ws);

                    let second_terminal_ws = second_terminal_ws.lock().unwrap();
                    let terminal2_buffer = terminal2_buffer.lock().unwrap();
                    let new_lines = lines_in_buffer(&*terminal2_buffer, &*second_terminal_ws);

                    ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                    ::std::io::stdout().write_all(&new_lines).expect("cannot write to stdout");

                    ::std::io::stdout().flush().expect("could not flush");
                } else {
                    *active_terminal = first_terminal_pid;
                    let second_terminal_ws = second_terminal_ws.lock().unwrap();
                    let empty_lines = create_empty_lines(&*second_terminal_ws);

                    let first_terminal_ws = first_terminal_ws.lock().unwrap();
                    let terminal1_buffer = terminal1_buffer.lock().unwrap();
                    let lines = lines_in_buffer(&*terminal1_buffer, &*first_terminal_ws);

                    ::std::io::stdout().write_all(&empty_lines).expect("cannot write to stdout");
                    ::std::io::stdout().write_all(&lines).expect("cannot write to stdout");

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
