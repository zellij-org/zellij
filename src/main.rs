use std::time::{Duration, Instant};
use std::iter::FromIterator;
use std::{mem, io};
use ::std::fmt::{self, Display, Formatter};
use std::cmp::max;
use std::io::{stdin, stdout, Read, Write};
use std::collections::{BTreeSet, HashSet, HashMap, VecDeque};
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

fn read_from_pid (pid: RawFd) -> Option<Vec<u8>> {
    let mut read_buffer = [0; 115200];
    let read_result = read(pid, &mut read_buffer);
    match read_result {
        Ok(res) => {
            let res = Some(read_buffer[..=res].to_vec());
            res
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

pub fn set_terminal_size_using_fd(fd: RawFd, columns: u16, rows: u16) {
    // TODO: do this with the nix ioctl
    use libc::ioctl;
    use libc::TIOCSWINSZ;

    let winsize = Winsize {
        ws_col: columns,
        ws_row: rows,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe { ioctl(fd, TIOCSWINSZ.into(), &winsize) };
}


fn spawn_terminal (ws: &Winsize) -> (RawFd, RawFd) {
    let (pid_primary, pid_secondary): (RawFd, RawFd) = {
        match forkpty(Some(ws), None) {
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
                        set_terminal_size_using_fd(0, ws.ws_col, ws.ws_row);
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
    (pid_primary, pid_secondary)
}

#[derive(Clone, Debug)]
struct TerminalCharacter {
    pub character: char,
    pub ansi_code: Option<String>,
}

impl PartialEq for TerminalCharacter {
    fn eq(&self, other: &Self) -> bool {
        match (&self.ansi_code, &other.ansi_code) {
            (Some(self_code), Some(other_code)) => {
                self_code == other_code && self.character == other.character
            },
            (None, None) => {
                self.character == other.character
            }
            _ => {
                false
            }
        }
    }
}

impl Eq for TerminalCharacter {}

impl TerminalCharacter {
    pub fn new (character: char) -> Self {
        TerminalCharacter {
            character,
            ansi_code: None
        }
    }
    pub fn ansi_code(mut self, ansi_code: String) -> Self {
        self.ansi_code = Some(ansi_code);
        self
    }
}


impl Display for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match &self.ansi_code {
            Some(code) => write!(f, "{}{}", code, self.character),
            None => write!(f, "{}", self.character)
        }
    }
}

struct TerminalOutput {
    pub characters: Vec<char>, // we use a vec rather than a string here because one char can take up multiple characters in a string
    pub display_rows: u16,
    pub display_cols: u16,
    pub should_render: bool,
    cursor_position: usize,
    newline_indices: Vec<usize>, // canonical line breaks we get from the vt interpreter
    linebreak_indices: Vec<usize>, // linebreaks from line wrapping
    unhandled_ansi_codes: HashMap<usize, String>,
    pending_ansi_code: Option<String>, // this is used eg. in a carriage return, where we need to preserve the style
}

impl TerminalOutput {
    pub fn new () -> TerminalOutput {
        TerminalOutput {
            characters: vec![],
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            display_rows: 0,
            display_cols: 0,
            should_render: false,
            unhandled_ansi_codes: HashMap::new(),
            pending_ansi_code: None,
        }
    }
    pub fn reduce_width(&mut self, count: u16) {
        self.display_cols -= count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn increase_width(&mut self, count: u16) {
        self.display_cols += count;
        self.reflow_lines();
        self.should_render = true;
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
        let newline_indices: HashSet<&usize> = HashSet::from_iter(self.newline_indices.iter());
        let mut x: u64 = 0;
        for (i, _c) in self.characters.iter().enumerate() {
            if newline_indices.contains(&i) {
                x = 0;
            } else if x == self.display_cols as u64 && i < self.cursor_position {
                self.linebreak_indices.push(i);
                x = 0;
            }
            x += 1;
        }
    }
    pub fn read_buffer_as_lines (&mut self) -> Vec<Vec<TerminalCharacter>> {
        if DEBUGGING {
            return vec![];
        }
        let mut output: VecDeque<Vec<TerminalCharacter>> = VecDeque::new();
        let mut i = self.characters.len();
        let mut current_line: VecDeque<TerminalCharacter> = VecDeque::new();
        
        let newline_indices: HashSet<&usize> = HashSet::from_iter(self.newline_indices.iter());
        let linebreak_indices: HashSet<&usize> = HashSet::from_iter(self.linebreak_indices.iter());
        loop {
            i -= 1;
            let character = self.characters.get(i).unwrap();
            let mut terminal_character = TerminalCharacter::new(*character);
            if let Some(code) = self.unhandled_ansi_codes.get(&i) {
                terminal_character.ansi_code = Some(code.clone());
            }
            current_line.push_front(terminal_character);
            if newline_indices.contains(&i) || linebreak_indices.contains(&i) {
                // pad line
                for _ in current_line.len()..self.display_cols as usize {
                    current_line.push_back(TerminalCharacter::new(' '));
                }
                output.push_front(Vec::from(current_line.drain(..).collect::<Vec<TerminalCharacter>>()));
            }
            if i == 0 || output.len() == self.display_rows as usize {
                break;
            }
        }
        if output.len() < self.display_rows as usize {
            let mut empty_line = vec![];
            for _ in 0..self.display_cols {
                empty_line.push(TerminalCharacter::new(' '));
            }
            for _ in output.len()..self.display_rows as usize {
                output.push_front(Vec::from(empty_line.clone()));
            }
        }
        self.should_render = false;
        Vec::from(output)
    }
    pub fn cursor_position_in_last_line (&self) -> usize {
        if self.cursor_position < self.characters.len() {
            let start_of_last_line = self.index_of_beginning_of_last_line();
            let difference_from_last_newline = self.cursor_position - start_of_last_line;
            difference_from_last_newline
        } else {
            self.display_cols as usize
        }
    }
    fn index_of_beginning_of_last_line (&self) -> usize {
        let last_newline_index = if self.newline_indices.is_empty() {
            None
        } else {
            // return last
            Some(*self.newline_indices.last().unwrap())
        };
        let last_linebreak_index = if self.linebreak_indices.is_empty() {
            None
        } else {
            // return last
            Some(*self.linebreak_indices.last().unwrap())
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
    fn index_of_beginning_of_line (&self, index_in_line: usize) -> usize {
        let last_newline_index = if self.newline_indices.is_empty() {
            None
        } else {
            // return last less than index_in_line
            let last_newline_index = *self.newline_indices.last().unwrap();
            if last_newline_index <= index_in_line {
                Some(last_newline_index)
            } else {
                let mut last_newline_index = 0;
                for n_i in self.newline_indices.iter() {
                    if *n_i > last_newline_index && *n_i <= index_in_line {
                        last_newline_index = *n_i;
                    } else if *n_i > index_in_line {
                        break;
                    }
                }
                Some(last_newline_index)
            }
        };
        let last_linebreak_index = if self.linebreak_indices.is_empty() {
            None
        } else {
            // return last less than index_in_line
            let last_linebreak_index = *self.linebreak_indices.last().unwrap();
            if last_linebreak_index <= index_in_line {
                Some(last_linebreak_index)
            } else {
                let mut last_linebreak_index = 0;
                for l_i in self.linebreak_indices.iter() {
                    if *l_i > last_linebreak_index && *l_i <= index_in_line {
                        last_linebreak_index = *l_i;
                    } else if *l_i > index_in_line {
                        break;
                    }
                }
                Some(last_linebreak_index)
            }
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
        self.newline_indices.push(self.characters.len()); // -1?
        self.cursor_position = self.characters.len(); // -1?
        self.should_render = true;
    }
    fn move_to_beginning_of_line (&mut self) {
        let last_newline_index = if self.newline_indices.is_empty() {
            0
        } else {
            *self.newline_indices.last().unwrap()
        };
        self.cursor_position = last_newline_index;
        self.should_render = true;
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
            if let Some(ansi_code) = &self.pending_ansi_code {
                self.unhandled_ansi_codes.insert(self.cursor_position, ansi_code.clone());
                self.pending_ansi_code = None;
            }

            let start_of_last_line = self.index_of_beginning_of_line(self.cursor_position);
            let difference_from_last_newline = self.cursor_position - start_of_last_line;
            if difference_from_last_newline == self.display_cols as usize {
                self.linebreak_indices.push(self.cursor_position);
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
                // TODO: also remove character
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
                    self.pending_ansi_code = Some(String::from("\u{1b}[m"));
                    self.unhandled_ansi_codes.insert(self.cursor_position, String::from("\u{1b}[m"));
                } else {
                    // eg. \u{1b}[38;5;0m
                    let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                    self.pending_ansi_code = Some(format!("\u{1b}[{}m", param_string));
                    self.unhandled_ansi_codes.insert(self.cursor_position, format!("\u{1b}[{}m", param_string));
                }
            } else if c == 'C' { // move cursor
                self.cursor_position += params[0] as usize; // TODO: negative value?
            } else if c == 'K' { // clear line (0 => right, 1 => left, 2 => all)
                if params[0] == 0 {
                    for i in self.cursor_position + 1..self.characters.len() {
                        self.unhandled_ansi_codes.remove(&i);
                    }
                    if let Some(position_of_first_newline_index_to_delete) = self.newline_indices.iter().position(|&ni| ni > self.cursor_position) {
                        self.newline_indices.truncate(position_of_first_newline_index_to_delete);
                    }
                    if let Some(position_of_first_linebreak_index_to_delete) = self.linebreak_indices.iter().position(|&li| li > self.cursor_position) {
                        self.newline_indices.truncate(position_of_first_linebreak_index_to_delete);
                    }
                    self.characters.truncate(self.cursor_position + 1);
                }
                // TODO: implement 1 and 2
            } else if c == 'J' { // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
                if params[0] == 0 {
                    for i in self.cursor_position + 1..self.characters.len() {
                        self.unhandled_ansi_codes.remove(&i);
                    }
                    if let Some(position_of_first_newline_index_to_delete) = self.newline_indices.iter().position(|&ni| ni > self.cursor_position) {
                        self.newline_indices.truncate(position_of_first_newline_index_to_delete);
                    }
                    if let Some(position_of_first_linebreak_index_to_delete) = self.linebreak_indices.iter().position(|&li| li > self.cursor_position) {
                        self.newline_indices.truncate(position_of_first_linebreak_index_to_delete);
                    }
                    self.characters.truncate(self.cursor_position + 1);
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

fn split_horizontally_with_gap (rect: &Winsize) -> (Winsize, Winsize) {
    let width_of_each_half = (rect.ws_col - 1) / 2;
    let mut first_rect = rect.clone();
    let mut second_rect = rect.clone();
    first_rect.ws_col = width_of_each_half;
    second_rect.ws_col = width_of_each_half;
    (first_rect, second_rect)
}

fn get_previous_style (frame: &Vec<TerminalCharacter>, current_index: usize, current_column: usize) -> Option<&str> {
    if current_index == 0 || current_column == 0 {
    // if true {
        return None
    };
    let mut character_count = current_column; // only check back until line break, styles before that are not relevant
    let mut prev_index = current_index;
    loop {
        character_count -= 1;
        prev_index -= 1;
        match frame.get(prev_index) {
            Some(previous_character) => {
                if let Some(previous_ansi_code) = &previous_character.ansi_code {
                    return Some(previous_ansi_code)
                }
            },
            None => {
                return None;
            }
        };
        if prev_index == 0 || character_count == 0 {
            return None;
        }
    }
}

fn character_is_already_onscreen(
    last_frame: &Vec<TerminalCharacter>,
    current_frame: &Vec<TerminalCharacter>,
    index: usize,
    character_column: &usize
) -> bool {
    let last_character = last_frame.get(index).unwrap();
    let current_character = current_frame.get(index).unwrap();
    let last_character_style = match &last_character.ansi_code {
        Some(ansi_code) => Some(ansi_code.as_str()),
        // None => None,
        None => get_previous_style(&last_frame, index, *character_column),
    };
    let current_character_style = match &current_character.ansi_code {
        Some(ansi_code) => Some(ansi_code.as_str()),
        // None => None,
        None => get_previous_style(&current_frame, index, *character_column),
    };
    last_character_style == current_character_style && last_character.character == current_character.character
}

struct Screen {
    last_frame: Option<Vec<TerminalCharacter>>
}

impl Screen {
    pub fn new () -> Self {
        Screen { last_frame: None }
    }
    pub fn render (&mut self, terminal1_output: &mut TerminalOutput, terminal2_output: &mut TerminalOutput, full_screen_ws: &Winsize, terminal1_is_active: bool) {
        if DEBUGGING {
            return;
        }
        let left_terminal_lines = terminal1_output.read_buffer_as_lines();
        let right_terminal_lines = terminal2_output.read_buffer_as_lines();

        let mut frame: Vec<TerminalCharacter> = vec![];
        let vertical_separator = TerminalCharacter::new('|').ansi_code(String::from("\u{1b}[m"));
        for i in 0..full_screen_ws.ws_row {
            let left_terminal_row = left_terminal_lines.get(i as usize).unwrap();
            for terminal_character in left_terminal_row.iter() {
                frame.push(terminal_character.clone());
            }

            frame.push(vertical_separator.clone());

            let right_terminal_row = right_terminal_lines.get(i as usize).unwrap();
            for terminal_character in right_terminal_row.iter() {
                frame.push(terminal_character.clone());
            }

        }

        let mut data_lines = String::new();
        match &self.last_frame {
            Some(last_frame) => {
                if last_frame.len() != frame.len() {
                    // this is not ideal
                    // right now it happens when we resize a pane, until fish resets the last line
                    return
                }
                let mut last_character_was_changed = false;
                for i in 0..last_frame.len() {
                    let current_character = frame.get(i).unwrap();
                    let row = i / full_screen_ws.ws_col as usize + 1;
                    let col = i % full_screen_ws.ws_col as usize + 1;
                    if !character_is_already_onscreen(&last_frame, &frame, i, &col) {
                    // if true {
                        if !last_character_was_changed {
                            // goto row/col
                            data_lines.push_str(&format!("\u{1b}[{};{}H", row, &col));
                            // copy the last style from the last frame, or reset the style
                            // this is so that if the first character of the changed string
                            // has no style, it will get the appropriate one and not the one
                            // from where the cursor happened to be previously
                            if i > 0 {
                                match get_previous_style(&frame, i, col) {
                                    Some(previous_ansi_code) => {
                                        data_lines.push_str("\u{1b}[m"); // reset style
                                        data_lines.push_str(&previous_ansi_code);
                                    },
                                    None => {
                                        data_lines.push_str("\u{1b}[m"); // reset style
                                    }
                                };
                            } else {
                                data_lines.push_str("\u{1b}[m"); // reset style
                            }
                        }
                        data_lines.push_str(&current_character.to_string());
                        last_character_was_changed = true;
                    } else {
                        last_character_was_changed = false;
                    }
                }
            },
            None => {
                print!("\u{1b}c"); // clear screen
                for terminal_character in frame.iter() {
                    data_lines.push_str(&terminal_character.to_string());
                }
            }
        }
        self.last_frame = Some(frame);

        let left_terminal_cursor_position = terminal1_output.cursor_position_in_last_line();
        let right_terminal_cursor_position = terminal2_output.cursor_position_in_last_line();
        // print!("\u{1b}c"); // clear screen
        if terminal1_is_active {
            data_lines.push_str(&format!("\r\u{1b}[{}C", left_terminal_cursor_position));
        } else {
            data_lines.push_str(&format!("\r\u{1b}[{}C", right_terminal_cursor_position + (terminal1_output.display_cols + 1) as usize));
        }
        ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
        ::std::io::stdout().flush().expect("could not flush");
    }
}

fn main() {
    let mut active_threads = vec![];

    let full_screen_ws = get_terminal_size_using_fd(0);
    let (first_terminal_ws, second_terminal_ws) = split_horizontally_with_gap(&full_screen_ws);
    let (first_terminal_pid, pid_secondary): (RawFd, RawFd) = spawn_terminal(&first_terminal_ws);
    let (second_terminal_pid, pid_secondary): (RawFd, RawFd) = spawn_terminal(&second_terminal_ws);
    let stdin = io::stdin();
    into_raw_mode(0);
    set_baud_rate(0);
    ::std::thread::sleep(std::time::Duration::from_millis(2000));
    let active_terminal = Arc::new(Mutex::new(first_terminal_pid));

    let first_terminal_ws = Arc::new(Mutex::new(first_terminal_ws));
    let second_terminal_ws = Arc::new(Mutex::new(second_terminal_ws));

    let terminal1_output = Arc::new(Mutex::new(TerminalOutput::new()));
    let terminal2_output = Arc::new(Mutex::new(TerminalOutput::new()));

    let screen = Arc::new(Mutex::new(Screen::new()));

    active_threads.push(
        thread::Builder::new()
            .name("terminal_stdout_handler".to_string())
            .spawn({

                let mut vte_parser_terminal1 = vte::Parser::new();
                let mut vte_parser_terminal2 = vte::Parser::new();

                let active_terminal = active_terminal.clone();
                let terminal1_output = terminal1_output.clone();
                let terminal2_output = terminal2_output.clone();
                let first_terminal_ws = first_terminal_ws.clone();
                let second_terminal_ws = second_terminal_ws.clone();
                let screen = screen.clone();
                move || {
                    let mut buffer_has_unread_data = true;
                    {
                        // TODO: better
                        let first_terminal_ws = first_terminal_ws.lock().unwrap();
                        let second_terminal_ws = second_terminal_ws.lock().unwrap();
                        let mut terminal1_output = terminal1_output.lock().unwrap();
                        let mut terminal2_output = terminal2_output.lock().unwrap();
                        terminal1_output.set_size(&first_terminal_ws);
                        terminal2_output.set_size(&second_terminal_ws);
                    }
                    loop {
                        match (read_from_pid(first_terminal_pid), read_from_pid(second_terminal_pid)) {
                            (Some(first_terminal_read_bytes), Some(second_terminal_read_bytes)) => {
                                let mut terminal1_output = terminal1_output.lock().unwrap();
                                let mut terminal2_output = terminal2_output.lock().unwrap();
                                for byte in first_terminal_read_bytes.iter() {
                                    vte_parser_terminal1.advance(&mut *terminal1_output, *byte);
                                }
                                for byte in second_terminal_read_bytes.iter() {
                                    vte_parser_terminal2.advance(&mut *terminal2_output, *byte);
                                }
                                buffer_has_unread_data = true;
                            }
                            (Some(first_terminal_read_bytes), None) => {
                                let mut terminal1_output = terminal1_output.lock().unwrap();
                                for byte in first_terminal_read_bytes.iter() {
                                    vte_parser_terminal1.advance(&mut *terminal1_output, *byte);
                                }
                            }
                            (None, Some(second_terminal_read_bytes)) => {
                                let mut terminal1_output = terminal1_output.lock().unwrap();
                                let mut terminal2_output = terminal2_output.lock().unwrap();
                                for byte in second_terminal_read_bytes.iter() {
                                    vte_parser_terminal2.advance(&mut *terminal2_output, *byte);
                                }
                            }
                            (None, None) => {
                                ::std::thread::sleep(std::time::Duration::from_millis(50)); // TODO: adjust this
                            }
                        }
                        let mut terminal1_output = terminal1_output.lock().unwrap();
                        let mut terminal2_output = terminal2_output.lock().unwrap();
                        if terminal1_output.should_render || terminal2_output.should_render {
                            let active_terminal = active_terminal.lock().unwrap();
                            let mut screen = screen.lock().unwrap();
                            // let now = Instant::now();
                            screen.render(&mut *terminal1_output, &mut *terminal2_output, &full_screen_ws, *active_terminal == first_terminal_pid);
                            // println!("\r->R rendered in {:?}", now.elapsed());
                        }
                    }
                }
            })
            .unwrap(),
    );
    loop {
		let mut buffer = [0; 1];
        {
            let mut handle = stdin.lock();
            handle.read(&mut buffer).expect("failed to read stdin");
            if buffer[0] == 10 { // ctrl-j
                let mut terminal1_output = terminal1_output.lock().unwrap();
                let mut terminal2_output = terminal2_output.lock().unwrap();
                let active_terminal = active_terminal.lock().unwrap();
                terminal1_output.reduce_width(10);
                terminal2_output.increase_width(10);
                set_terminal_size_using_fd(first_terminal_pid, terminal1_output.display_cols, terminal1_output.display_rows);
                set_terminal_size_using_fd(second_terminal_pid, terminal2_output.display_cols, terminal2_output.display_rows);
                screen.lock().unwrap().render(&mut *terminal1_output, &mut *terminal2_output, &full_screen_ws, *active_terminal == first_terminal_pid);
                continue;
            } else if buffer[0] == 11 { // ctrl-k
                let mut terminal1_output = terminal1_output.lock().unwrap();
                let mut terminal2_output = terminal2_output.lock().unwrap();
                let active_terminal = active_terminal.lock().unwrap();
                terminal1_output.increase_width(10);
                terminal2_output.reduce_width(10);
                set_terminal_size_using_fd(first_terminal_pid, terminal1_output.display_cols, terminal1_output.display_rows);
                set_terminal_size_using_fd(second_terminal_pid, terminal2_output.display_cols, terminal2_output.display_rows);
                screen.lock().unwrap().render(&mut *terminal1_output, &mut *terminal2_output, &full_screen_ws, *active_terminal == first_terminal_pid);
                continue;
            } else if buffer[0] == 16 { // ctrl-p
                let mut active_terminal = active_terminal.lock().unwrap();
                if *active_terminal == first_terminal_pid {
                    *active_terminal = second_terminal_pid;
                    let mut terminal1_output = terminal1_output.lock().unwrap();
                    let mut terminal2_output = terminal2_output.lock().unwrap();
                    screen.lock().unwrap().render(&mut *terminal1_output, &mut *terminal2_output, &full_screen_ws, *active_terminal == first_terminal_pid);
                } else {
                    *active_terminal = first_terminal_pid;
                    let mut terminal1_output = terminal1_output.lock().unwrap();
                    let mut terminal2_output = terminal2_output.lock().unwrap();
                    screen.lock().unwrap().render(&mut *terminal1_output, &mut *terminal2_output, &full_screen_ws, *active_terminal == first_terminal_pid);
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
