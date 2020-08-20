use std::time::{Duration, Instant};
use std::io;
use futures::future::join_all;
use std::cell::Cell;
use ::std::fmt::{self, Display, Formatter};
use std::cmp::max;
use std::io::{Read, Write};
use std::collections::VecDeque;
use nix::unistd::{read, write, ForkResult};
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::termios::{
    tcgetattr,
    cfmakeraw,
    tcsetattr,
    SetArg,
    tcdrain,
    cfsetispeed,
    cfsetospeed,
    BaudRate,
};
use nix::pty::{forkpty, Winsize};
use std::os::unix::io::RawFd;
use std::process::Command;
use ::std::thread;
use ::std::sync::{Arc, Mutex};
use vte;
use async_std::stream::*;
use async_std::task;
use async_std::task::*;
use async_std::prelude::*;
use ::std::pin::*;
use std::sync::mpsc::{channel, Sender, Receiver};



struct ReadFromPid {
    pid: RawFd,
    read_buffer: [u8; 115200],
}

impl ReadFromPid {
    fn new(pid: &RawFd) -> ReadFromPid {
        ReadFromPid {
            pid: *pid,
            read_buffer: [0; 115200], // TODO: ???
        }
    }
}

impl Stream for ReadFromPid {
    type Item = Vec<u8>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let read_result = read(self.pid, &mut self.read_buffer);
        match read_result {
            Ok(res) => {
                // TODO: this might become an issue with multiple panes sending data simultaneously
                // ...consider returning None if res == 0 and handling it in the task (or sending
                // Poll::Pending?)
                let res = Some(self.read_buffer[..=res].to_vec());
                self.read_buffer = [0; 115200];
                return Poll::Ready(res)
            },
            Err(e) => {
                match e {
                    nix::Error::Sys(errno) => {
                        if errno == nix::errno::Errno::EAGAIN {
                            return Poll::Ready(Some(vec![])) // TODO: better with timeout waker somehow
                            // task::block_on(task::sleep(Duration::from_millis(10)));
                        } else {
                            panic!("error {:?}", e);
                        }
                    },
                    _ => panic!("error {:?}", e)
                }
            }
        }
    }
}

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
                        fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::empty())).expect("could not fcntl");
                        // fcntl(pid_primary, FcntlArg::F_SETFL(OFlag::O_NONBLOCK)).expect("could not fcntl");
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
    pub pid: RawFd,
    pub characters: Vec<TerminalCharacter>,
    pub display_rows: u16,
    pub display_cols: u16,
    pub should_render: bool,
    cursor_position: usize,
    newline_indices: Vec<usize>, // canonical line breaks we get from the vt interpreter
    linebreak_indices: Vec<usize>, // linebreaks from line wrapping
    pending_ansi_code: Option<String>, // this is used eg. in a carriage return, where we need to preserve the style
}

const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter { character: ' ', ansi_code: None };

impl TerminalOutput {
    pub fn new (pid: RawFd, ws: Winsize) -> TerminalOutput {
        TerminalOutput {
            pid,
            characters: vec![],
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: false,
            pending_ansi_code: None,
        }
    }
    pub fn handle_event(&mut self, event: VteEvent) {
        match event {
            VteEvent::Print(c) => {
                self.print(c);
            },
            VteEvent::Execute(byte) => {
                self.execute(byte);
            },
            VteEvent::Hook(params, intermediates, ignore, c) => {
                self.hook(&params, &intermediates, ignore, c);
            },
            VteEvent::Put(byte) => {
                self.put(byte);
            },
            VteEvent::Unhook => {
                self.unhook();
            },
            VteEvent::OscDispatch(params, bell_terminated) => {
                self.osc_dispatch(params, bell_terminated);
            },
            VteEvent::CsiDispatch(params, intermediates, ignore, c) => {
                self.csi_dispatch(&params, &intermediates, ignore, c);
            },
            VteEvent::EscDispatch(intermediates, ignore, byte) => {
                self.esc_dispatch(&intermediates, ignore, byte);
            }
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

        let mut newline_indices = self.newline_indices.iter();
        let mut next_newline_index = newline_indices.next();

        let mut x: u64 = 0;
        for (i, _c) in self.characters.iter().enumerate() {
            if next_newline_index == Some(&i) {
                x = 0;
                next_newline_index = newline_indices.next();
            } else if x == self.display_cols as u64 && i < self.cursor_position {
                self.linebreak_indices.push(i);
                x = 0;
            }
            x += 1;
        }
    }
    pub fn read_buffer_as_lines (&self) -> Vec<Vec<&TerminalCharacter>> {
        if DEBUGGING {
            return vec![];
        }
        if self.characters.len() == 0 {
            return vec![];
        }
        let mut output: VecDeque<Vec<&TerminalCharacter>> = VecDeque::new();
        let mut i = self.characters.len();
        let mut current_line: VecDeque<&TerminalCharacter> = VecDeque::new();
        
        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline_index = newline_indices.next();
        let mut next_linebreak_index = linebreak_indices.next();

        loop {
            i -= 1;
            let terminal_character = self.characters.get(i).unwrap();
            current_line.push_front(terminal_character);
            if let Some(newline_index) = next_newline_index {
                if newline_index == &i {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_newline_index = newline_indices.next();
                    continue;
                }
            }
            if let Some(linebreak_index) = next_linebreak_index {
                if linebreak_index == &i {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_linebreak_index = linebreak_indices.next();
                    continue;
                }
            }
            if i == 0 || output.len() == self.display_rows as usize {
                break;
            }
        }
        if output.len() < self.display_rows as usize {
            let mut empty_line = vec![];
            for _ in 0..self.display_cols {
                empty_line.push(&EMPTY_TERMINAL_CHARACTER);
            }
            for _ in output.len()..self.display_rows as usize {
                output.push_front(Vec::from(empty_line.clone()));
            }
        }
        // self.should_render = false;
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
        self.pending_ansi_code = None;
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

// vte methods
impl TerminalOutput {
    fn print(&mut self, c: char) {
        if DEBUGGING {
            println!("\r[print] {:?}", c);
        } else {
            let mut terminal_character = TerminalCharacter::new(c);
            terminal_character.ansi_code = self.pending_ansi_code.clone();
            if self.characters.len() == self.cursor_position {
                self.characters.push(terminal_character);
            } else if self.characters.len() > self.cursor_position {
                self.characters.remove(self.cursor_position);
                self.characters.insert(self.cursor_position, terminal_character);
            } else {
                let mut space_character = TerminalCharacter::new(' ');
                space_character.ansi_code = self.pending_ansi_code.clone();
                for _ in self.characters.len()..self.cursor_position {
                    self.characters.push(space_character.clone());
                };
                self.characters.push(terminal_character);
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

    // fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
    // TODO: normalize vec/slices for all of these methods and the enum
    fn osc_dispatch(&mut self, params: Vec<Vec<u8>>, bell_terminated: bool) {
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
                } else {
                    // eg. \u{1b}[38;5;0m
                    let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                    self.pending_ansi_code = Some(format!("\u{1b}[{}m", param_string));
                }
            } else if c == 'C' { // move cursor
                self.cursor_position += params[0] as usize; // TODO: negative value?
            } else if c == 'K' { // clear line (0 => right, 1 => left, 2 => all)
                if params[0] == 0 {
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

enum VteEvent { // TODO: try not to allocate Vecs
    Print(char),
    Execute(u8), // byte
    Hook(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    Put(u8), // byte
    Unhook,
    OscDispatch(Vec<Vec<u8>>, bool), // params, bell_terminated
    CsiDispatch(Vec<i64>, Vec<u8>, bool, char), // params, intermediates, ignore, char
    EscDispatch(Vec<u8>, bool, u8), // intermediates, ignore, byte
}

struct VteEventSender {
    id: RawFd,
    sender: Sender<ScreenInstruction>,
}

impl VteEventSender {
    pub fn new (id: RawFd, sender: Sender<ScreenInstruction>) -> Self {
        VteEventSender { id, sender }
    }
}

impl vte::Perform for VteEventSender {
    fn print(&mut self, c: char) {
        self.sender.send(
            ScreenInstruction::Pty(self.id, VteEvent::Print(c))
        ).unwrap();
    }
    fn execute(&mut self, byte: u8) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Execute(byte))).unwrap();
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::Hook(params, intermediates, ignore, c));
        self.sender.send(instruction).unwrap();
    }

    fn put(&mut self, byte: u8) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Put(byte))).unwrap();
    }

    fn unhook(&mut self) {
        self.sender.send(ScreenInstruction::Pty(self.id, VteEvent::Unhook)).unwrap();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let params = params.iter().map(|p| p.to_vec()).collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::OscDispatch(params, bell_terminated));
        self.sender.send(instruction).unwrap();
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        let params = params.iter().copied().collect();
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::CsiDispatch(params, intermediates, ignore, c));
        self.sender.send(instruction).unwrap();
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        let intermediates = intermediates.iter().copied().collect();
        let instruction = ScreenInstruction::Pty(self.id, VteEvent::EscDispatch(intermediates, ignore, byte));
        self.sender.send(instruction).unwrap();
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

fn character_is_already_onscreen(
    last_character: &TerminalCharacter,
    current_character: &TerminalCharacter,
) -> bool {
    let last_character_style = match &last_character.ansi_code {
        Some(ansi_code) => Some(ansi_code.as_str()),
        None => None,
    };
    let current_character_style = match &current_character.ansi_code {
        Some(ansi_code) => Some(ansi_code.as_str()),
        None => None,
    };
    last_character_style == current_character_style && last_character.character == current_character.character
}

enum ScreenInstruction {
    Pty(RawFd, VteEvent),
    Render,
    AddTerminal(RawFd, Winsize),
    WriteCharacter(u8),
    ResizeLeft,
    ResizeRight,
    MoveFocus,
}

struct Screen {
    pub receiver: Receiver<ScreenInstruction>,
    pub sender: Sender<ScreenInstruction>,
    full_screen_ws: Winsize,
    last_frame: Option<Vec<TerminalCharacter>>,
    vertical_separator: TerminalCharacter, // TODO: better
    active_terminal: Option<RawFd>,
    terminal1_output: Option<TerminalOutput>,
    terminal2_output: Option<TerminalOutput>,
}

impl Screen {
    pub fn new () -> Self {
        let (sender, receiver): (Sender<ScreenInstruction>, Receiver<ScreenInstruction>) = channel();
        let full_screen_ws = get_terminal_size_using_fd(0);
        Screen {
            receiver,
            sender,
            full_screen_ws,
            last_frame: None,
            vertical_separator: TerminalCharacter::new('|').ansi_code(String::from("\u{1b}[m")), // TODO: better
            terminal1_output: None,
            terminal2_output: None,
            active_terminal: None,
        }
    }
    pub fn add_terminal(&mut self, pid: RawFd, ws: Winsize) {
        if self.terminal1_output.is_none() {
            self.terminal1_output = Some(TerminalOutput::new(pid, ws));
            self.active_terminal = Some(pid);
        } else if self.terminal2_output.is_none() {
            self.terminal2_output = Some(TerminalOutput::new(pid, ws));
        } else {
            panic!("cannot support more than 2 terminals atm");
        }
    }
    pub fn handle_pty_event(&mut self, pid: RawFd, event: VteEvent) {
        if let Some(terminal_output) = self.terminal1_output.as_mut() {
            if terminal_output.pid == pid {
                terminal_output.handle_event(event);
                return;
            }
        }
        if let Some(terminal_output) = self.terminal2_output.as_mut() {
            if terminal_output.pid == pid {
                terminal_output.handle_event(event);
                return;
            }
        }
    }
    pub fn write_to_active_terminal(&self, byte: u8) {
        if let Some(active_terminal) = &self.active_terminal {
            let mut buffer = [byte];
            write(*active_terminal, &mut buffer).expect("failed to write to terminal");
            tcdrain(*active_terminal).expect("failed to drain terminal");
        }
    }
    pub fn render (&mut self) {
        let left_terminal_lines = self.terminal1_output.as_ref().unwrap().read_buffer_as_lines();
        let right_terminal_lines = self.terminal2_output.as_ref().unwrap().read_buffer_as_lines();
        if left_terminal_lines.len() < self.full_screen_ws.ws_row as usize || right_terminal_lines.len() < self.full_screen_ws.ws_row as usize {
            // TODO: this is hacky and is only here(?) for when the terminals are not ready yet
            return;
        }

        let mut frame: Vec<&TerminalCharacter> = vec![];
        let empty_vec = vec![]; // TODO: do not allocate, and less hacky
        for i in 0..self.full_screen_ws.ws_row {
            let left_terminal_row = left_terminal_lines.get(i as usize).unwrap_or(&empty_vec);
            for terminal_character in left_terminal_row.iter() {
                frame.push(terminal_character);
            }

            frame.push(&self.vertical_separator);

            let right_terminal_row = right_terminal_lines.get(i as usize).unwrap_or(&empty_vec);
            for terminal_character in right_terminal_row.iter() {
                frame.push(terminal_character);
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
                let mut last_rendered_char_style = None;
                for i in 0..last_frame.len() {
                    let last_character = last_frame.get(i).unwrap();
                    let current_character = frame.get(i).unwrap();
                    let row = i / self.full_screen_ws.ws_col as usize + 1;
                    let col = i % self.full_screen_ws.ws_col as usize + 1;
                    if !character_is_already_onscreen(&last_character, &current_character) {
                        if !last_character_was_changed {
                            data_lines.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", row, &col)); // goto row/col
                        }
                        // TODO: only render the ansi_code if it is different from the previous
                        // rendered ansi code (previous char in this loop)
                        if last_rendered_char_style == current_character.ansi_code && last_character_was_changed {
                            data_lines.push(current_character.character);
                        } else {
                            data_lines.push_str(&current_character.to_string());
                            last_rendered_char_style = current_character.ansi_code.clone();
                        }
                        last_character_was_changed = true;
                    } else {
                        last_character_was_changed = false;
                        last_rendered_char_style = None;
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
        // TODO: consider looping through current frame and only updating the cells that changed
        self.last_frame = Some(frame.into_iter().cloned().collect::<Vec<_>>());

        let left_terminal_cursor_position = self.terminal1_output.as_ref().unwrap().cursor_position_in_last_line();
        let right_terminal_cursor_position = self.terminal2_output.as_ref().unwrap().cursor_position_in_last_line();

        let active_terminal = self.active_terminal.unwrap();
        if active_terminal == self.terminal1_output.as_ref().unwrap().pid {
            data_lines.push_str(&format!("\r\u{1b}[{}C", left_terminal_cursor_position));
        } else {
            data_lines.push_str(&format!("\r\u{1b}[{}C", right_terminal_cursor_position + (self.terminal1_output.as_ref().unwrap().display_cols + 1) as usize));
        }
        ::std::io::stdout().write_all(&data_lines.as_bytes()).expect("cannot write to stdout");
        ::std::io::stdout().flush().expect("could not flush");
    }
    pub fn resize_left (&mut self) {
        let terminal1_output = self.terminal1_output.as_mut().unwrap();
        let terminal2_output = self.terminal2_output.as_mut().unwrap();
        terminal1_output.reduce_width(10);
        terminal2_output.increase_width(10);
        set_terminal_size_using_fd(terminal1_output.pid, terminal1_output.display_cols, terminal1_output.display_rows);
        set_terminal_size_using_fd(terminal2_output.pid, terminal2_output.display_cols, terminal2_output.display_rows);
    }
    pub fn resize_right (&mut self) {
        let terminal1_output = self.terminal1_output.as_mut().unwrap();
        let terminal2_output = self.terminal2_output.as_mut().unwrap();
        terminal1_output.increase_width(10);
        terminal2_output.reduce_width(10);
        set_terminal_size_using_fd(terminal1_output.pid, terminal1_output.display_cols, terminal1_output.display_rows);
        set_terminal_size_using_fd(terminal2_output.pid, terminal2_output.display_cols, terminal2_output.display_rows);
    }
    pub fn move_focus(&mut self) {
        let terminal1_output = self.terminal1_output.as_ref().unwrap();
        let terminal2_output = self.terminal2_output.as_ref().unwrap();
        let active_terminal = self.active_terminal.unwrap();
        if active_terminal == terminal1_output.pid {
            self.active_terminal = Some(terminal2_output.pid);
        } else {
            self.active_terminal = Some(terminal1_output.pid);
        }
        self.render();
    }
}

struct PtyBus {
    sender: Sender<ScreenInstruction>,
    active_ptys: Vec<JoinHandle<()>>,
}

impl PtyBus {
    pub fn new (sender: Sender<ScreenInstruction>) -> Self {
        PtyBus {
            sender,
            active_ptys: Vec::new()
        }
    }
    pub fn spawn_terminal(&mut self, ws: &Winsize) {
        let ws = *ws;
        let (pid_primary, _pid_secondary): (RawFd, RawFd) = spawn_terminal(&ws);
        let task_handle = task::spawn({
            // let pid_primary = pid_primary.clone();
            let sender = self.sender.clone();
            async move {
                let mut vte_parser = vte::Parser::new();
                let mut vte_event_sender = VteEventSender::new(pid_primary, sender.clone());
                let mut first_terminal_bytes = ReadFromPid::new(&pid_primary);
                while let Some(bytes) = first_terminal_bytes.next().await {
                    let bytes_is_empty = bytes.is_empty();
                    for byte in bytes {
                        vte_parser.advance(&mut vte_event_sender, byte);
                    }
                    if !bytes_is_empty {
                        sender.send(ScreenInstruction::Render).unwrap();
                    }
                }
            }
        });
        self.sender.send(ScreenInstruction::AddTerminal(pid_primary, ws)).unwrap();
        self.active_ptys.push(task_handle);
    }
    pub async fn wait_for_tasks(&mut self) {
//        let task1 = self.active_ptys.get_mut(0).unwrap();
//        task1.await;
        let mut v = vec![];
        for handle in self.active_ptys.iter_mut() {
            // TODO: better, see commented lines above... can't we do this on the original vec?
            v.push(handle);
        }
        join_all(v).await;
    }
}

fn main() {
    let mut active_threads = vec![];

    let stdin = io::stdin();
    into_raw_mode(0);
    set_baud_rate(0);
    ::std::thread::sleep(std::time::Duration::from_millis(2000));
    let mut screen = Screen::new();
    let send_screen_instructions = screen.sender.clone();

    active_threads.push(
        thread::Builder::new()
            .name("pty".to_string())
            .spawn({
                let full_screen_ws = get_terminal_size_using_fd(0);
                let (first_terminal_ws, second_terminal_ws) = split_horizontally_with_gap(&full_screen_ws);
                let first_terminal_ws = first_terminal_ws.clone();
                let second_terminal_ws = second_terminal_ws.clone();
                let send_screen_instructions = send_screen_instructions.clone();
                move || {
                    let mut pty_bus = PtyBus::new(send_screen_instructions);
                    // this is done here so that we can add terminals dynamically on a different
                    // thread later
                    pty_bus.spawn_terminal(&first_terminal_ws);
                    pty_bus.spawn_terminal(&second_terminal_ws);
                    task::block_on(pty_bus.wait_for_tasks());

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
                            ScreenInstruction::AddTerminal(pid, ws) => {
                                screen.add_terminal(pid, ws);
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
                            ScreenInstruction::MoveFocus => {
                                screen.move_focus();
                            }
                        }
                    }
                }
            }).unwrap()
    );

    loop {
		let mut buffer = [0; 1];
        {
            let mut handle = stdin.lock();
            handle.read(&mut buffer).expect("failed to read stdin");
            if buffer[0] == 10 { // ctrl-j
                send_screen_instructions.send(ScreenInstruction::ResizeLeft).unwrap();
                continue;
            } else if buffer[0] == 11 { // ctrl-k
                send_screen_instructions.send(ScreenInstruction::ResizeRight).unwrap();
                continue;
            } else if buffer[0] == 16 { // ctrl-p
                send_screen_instructions.send(ScreenInstruction::MoveFocus).unwrap();
                continue;
            }
        }
        send_screen_instructions.send(ScreenInstruction::WriteCharacter(buffer[0])).unwrap();
    };
//    cleanup();
    
//    for thread_handler in active_threads {
//        thread_handler.join().unwrap();
//    }

}
