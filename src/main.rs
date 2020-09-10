#[cfg(test)]
mod tests;

mod os_input_output;

use ::std::fmt::{self, Display, Formatter};
use std::cmp::max;
use std::io::{Read, Write};
use std::collections::{VecDeque, HashSet, BTreeMap};
use nix::pty::Winsize;
use std::os::unix::io::RawFd;
use ::std::thread;
use vte;
use async_std::stream::*;
use async_std::task;
use async_std::task::*;
use ::std::pin::*;
use std::sync::mpsc::{channel, Sender, Receiver};

use crate::os_input_output::{get_os_input, OsApi};

use vte::Perform;

struct ReadFromPid {
    pid: RawFd,
    os_input: Box<dyn OsApi>,
}

impl ReadFromPid {
    fn new(pid: &RawFd, os_input: Box<dyn OsApi>) -> ReadFromPid {
        ReadFromPid {
            pid: *pid,
            os_input,
        }
    }
}

impl Stream for ReadFromPid {
    type Item = Vec<u8>;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut read_buffer = [0; 115200];
        let pid = self.pid;
        let read_result = &self.os_input.read_from_tty_stdout(pid, &mut read_buffer);
        match read_result {
            Ok(res) => {
                if *res == 0 {
                    // indicates end of file
                    return Poll::Ready(None);
                } else {
                    let res = Some(read_buffer[..=*res].to_vec());
                    return Poll::Ready(res);
                }
            },
            Err(e) => {
                match e {
                    nix::Error::Sys(errno) => {
                        if *errno == nix::errno::Errno::EAGAIN {
                            return Poll::Ready(Some(vec![])) // TODO: better with timeout waker somehow
                        } else {
                            Poll::Ready(None)
                        }
                    },
                    _ => Poll::Ready(None)
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct TerminalCharacter {
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

impl ::std::fmt::Debug for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.character)
    }
}

pub struct TerminalOutput {
    pub pid: RawFd,
    pub characters: Vec<TerminalCharacter>,
    pub display_rows: u16,
    pub display_cols: u16,
    pub should_render: bool,
    cursor_position: usize,
    newline_indices: Vec<usize>, // canonical line breaks we get from the vt interpreter
    linebreak_indices: Vec<usize>, // linebreaks from line wrapping
    pending_ansi_code: Option<String>, // this is used eg. in a carriage return, where we need to preserve the style
    x_coords: u16,
    y_coords: u16,
}

const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter { character: ' ', ansi_code: None };

impl TerminalOutput {
    pub fn new (pid: RawFd, ws: Winsize, x_coords: u16, y_coords: u16) -> TerminalOutput {
        TerminalOutput {
            pid,
            characters: vec![],
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: true,
            pending_ansi_code: None,
            x_coords,
            y_coords,
        }
    }
    pub fn handle_event(&mut self, event: VteEvent) {
        self.should_render = true; // TODO: more accurately
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
                let params: Vec<&[u8]> = params.iter().map(|p| &p[..]).collect();
                self.osc_dispatch(&params[..], bell_terminated);
            },
            VteEvent::CsiDispatch(params, intermediates, ignore, c) => {
                self.csi_dispatch(&params, &intermediates, ignore, c);
            },
            VteEvent::EscDispatch(intermediates, ignore, byte) => {
                self.esc_dispatch(&intermediates, ignore, byte);
            }
        }
    }
    pub fn reduce_width_right(&mut self, count: u16) {
        self.x_coords += count;
        self.display_cols -= count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn reduce_width_left(&mut self, count: u16) {
        self.display_cols -= count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn increase_width_left(&mut self, count: u16) {
        self.x_coords -= count;
        self.display_cols += count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn increase_width_right(&mut self, count: u16) {
        self.display_cols += count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn reduce_height_down(&mut self, count: u16) {
        self.y_coords += count;
        self.display_rows -= count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn increase_height_down(&mut self, count: u16) {
        self.display_rows += count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn increase_height_up(&mut self, count: u16) {
        self.y_coords -= count;
        self.display_rows += count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn reduce_height_up(&mut self, count: u16) {
        self.display_rows -= count;
        self.reflow_lines();
        self.should_render = true;
    }
    pub fn change_size(&mut self, ws: &Winsize) {
        self.display_cols = ws.ws_col;
        self.display_rows = ws.ws_row;
        self.reflow_lines();
        self.should_render = true;
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
            } else if x == self.display_cols as u64 && i <= self.cursor_position {
                self.linebreak_indices.push(i);
                x = 0;
            }
            x += 1;
        }
    }
    pub fn buffer_as_vte_output(&mut self) -> Option<String> {
        if self.should_render {
            let mut vte_output = String::new();
            let buffer_lines = &self.read_buffer_as_lines();
            let display_cols = &self.display_cols;
            for (row, line) in buffer_lines.iter().enumerate() {
                vte_output.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", self.y_coords as usize + row + 1, self.x_coords + 1)); // goto row/col
                for (col, t_character) in line.iter().enumerate() {
                    if (col as u16) < *display_cols {
                        // in some cases (eg. while resizing) some characters will spill over
                        // before they are corrected by the shell (for the prompt) or by reflowing
                        // lines
                        vte_output.push_str(&t_character.to_string());
                    }
                }
            }
            self.should_render = false;
            Some(vte_output)
        } else {
            None
        }
    }
    fn read_buffer_as_lines (&self) -> Vec<Vec<&TerminalCharacter>> {
        if DEBUGGING {
            return vec![];
        }
        if self.characters.len() == 0 {
            let mut output = vec![];
            let mut empty_line = vec![];
            for _ in 0..self.display_cols {
                empty_line.push(&EMPTY_TERMINAL_CHARACTER);
            }
            for _ in 0..self.display_rows as usize {
                output.push(Vec::from(empty_line.clone()));
            }
            return output
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
            if let Some(newline_index) = next_newline_index {
                if *newline_index == i + 1 {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_newline_index = newline_indices.next();
                }
            }
            if let Some(linebreak_index) = next_linebreak_index {
                if *linebreak_index == i + 1 {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_linebreak_index = linebreak_indices.next();
                }
            }
            let terminal_character = self.characters.get(i).unwrap();
            current_line.push_front(terminal_character);
            if i == 0 || output.len() == self.display_rows as usize - 1 {
                for _ in current_line.len()..self.display_cols as usize {
                    current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                }
                output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
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

        Vec::from(output)
    }
    pub fn cursor_position_in_last_line (&self) -> usize {
        if self.cursor_position < self.characters.len() {
            let start_of_last_line = self.index_of_beginning_of_last_line();
            if self.cursor_position < start_of_last_line {
                // TODO: why does this happen?
                return self.display_cols as usize
            };
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
    fn index_of_beginning_of_last_canonical_line (&self) -> usize {
        if self.newline_indices.is_empty() {
            0
        } else {
            // return last
            *self.newline_indices.last().unwrap()
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
        self.newline_indices.push(self.characters.len());
        self.cursor_position = self.characters.len();
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

impl vte::Perform for TerminalOutput {
    fn print(&mut self, c: char) {
        if DEBUGGING {
            println!("\r[print] {:?}", c);
        } else {
            let mut terminal_character = TerminalCharacter::new(c);
            terminal_character.ansi_code = self.pending_ansi_code.clone();
            if self.characters.len() == self.cursor_position {
                self.characters.push(terminal_character);

                let start_of_last_line = self.index_of_beginning_of_line(self.cursor_position);
                let difference_from_last_newline = self.cursor_position - start_of_last_line;
                if difference_from_last_newline == self.display_cols as usize {
                    self.linebreak_indices.push(self.cursor_position);
                }

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

                let start_of_last_line = self.index_of_beginning_of_line(self.cursor_position);
                let difference_from_last_newline = self.cursor_position - start_of_last_line;
                if difference_from_last_newline == self.display_cols as usize {
                    self.linebreak_indices.push(self.cursor_position);
                }
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

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
    // TODO: normalize vec/slices for all of these methods and the enum
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
            } else if c == 'C' { // move cursor forward
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
            } else if c == 'H' { // goto row/col
                let row = params[0] as usize - 1; // we subtract 1 here because this csi is 1 indexed and we index from 0
                let col = params[1] as usize - 1;

                match self.newline_indices.get(row as usize) {
                    Some(index_of_next_row) => {
                        let index_of_row = index_of_next_row - self.display_cols as usize;
                        self.cursor_position = index_of_row + col as usize;
                    }
                    None => {
                        let start_of_last_line = self.index_of_beginning_of_last_canonical_line();
                        let num_of_lines_to_add = row - self.newline_indices.len();
                        for i in 0..num_of_lines_to_add {
                            self.newline_indices.push(start_of_last_line + ((i + 1) * self.display_cols as usize));
                        }
                        let index_of_row = self.newline_indices.last().unwrap_or(&0); // TODO: better
                        self.cursor_position = index_of_row + col as usize;
                    }
                }
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

#[derive(Debug)]
pub enum VteEvent { // TODO: try not to allocate Vecs
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

fn split_vertically_with_gap (rect: &Winsize) -> (Winsize, Winsize) {
    let width_of_each_half = (rect.ws_col - 1) / 2;
    let mut first_rect = rect.clone();
    let mut second_rect = rect.clone();
    if rect.ws_col % 2 == 0 {
        first_rect.ws_col = width_of_each_half + 1;
    } else {
        first_rect.ws_col = width_of_each_half;
    }
    second_rect.ws_col = width_of_each_half;
    (first_rect, second_rect)
}

fn split_horizontally_with_gap (rect: &Winsize) -> (Winsize, Winsize) {
    let height_of_each_half = (rect.ws_row - 1) / 2;
    let mut first_rect = rect.clone();
    let mut second_rect = rect.clone();
    if rect.ws_row % 2 == 0 {
        first_rect.ws_row = height_of_each_half + 1;
    } else {
        first_rect.ws_row = height_of_each_half;
    }
    second_rect.ws_row = height_of_each_half;
    (first_rect, second_rect)
}

#[derive(Debug)]
enum ScreenInstruction {
    Pty(RawFd, VteEvent),
    Render,
    HorizontalSplit(RawFd),
    VerticalSplit(RawFd),
    WriteCharacter(u8),
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    MoveFocus,
    Quit,
}

struct Screen {
    pub receiver: Receiver<ScreenInstruction>,
    pub send_screen_instructions: Sender<ScreenInstruction>,
    full_screen_ws: Winsize,
    vertical_separator: TerminalCharacter, // TODO: better
    horizontal_separator: TerminalCharacter, // TODO: better
    terminals: BTreeMap<RawFd, TerminalOutput>, // BTreeMap because we need a predictable order when changing focus
    active_terminal: Option<RawFd>,
    os_api: Box<dyn OsApi>,
}

impl Screen {
    pub fn new (full_screen_ws: &Winsize, os_api: Box<dyn OsApi>) -> Self {
        let (sender, receiver): (Sender<ScreenInstruction>, Receiver<ScreenInstruction>) = channel();
        Screen {
            receiver,
            send_screen_instructions: sender,
            full_screen_ws: full_screen_ws.clone(),
            vertical_separator: TerminalCharacter::new('│').ansi_code(String::from("\u{1b}[m")), // TODO: better
            horizontal_separator: TerminalCharacter::new('─').ansi_code(String::from("\u{1b}[m")), // TODO: better
            terminals: BTreeMap::new(),
            active_terminal: None,
            os_api,
        }
    }
    pub fn horizontal_split(&mut self, pid: RawFd) {
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalOutput::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, new_terminal.display_cols, new_terminal.display_rows);
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x_coords, active_terminal_y_coords) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.display_rows,
                        ws_col: active_terminal.display_cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.x_coords,
                    active_terminal.y_coords
                )
            };
            let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&active_terminal_ws);
            let bottom_half_y = active_terminal_y_coords + top_winsize.ws_row + 1;
            let new_terminal = TerminalOutput::new(pid, bottom_winsize, active_terminal_x_coords, bottom_half_y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, bottom_winsize.ws_col, bottom_winsize.ws_row);

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&top_winsize);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(active_terminal_pid, top_winsize.ws_col, top_winsize.ws_row);
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    pub fn vertical_split(&mut self, pid: RawFd) {
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalOutput::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, new_terminal.display_cols, new_terminal.display_rows);
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x_coords, active_terminal_y_coords) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.display_rows,
                        ws_col: active_terminal.display_cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.x_coords,
                    active_terminal.y_coords
                )
            };
            let (left_winszie, right_winsize) = split_vertically_with_gap(&active_terminal_ws);
            let right_side_x = active_terminal_x_coords + left_winszie.ws_col + 1;
            let new_terminal = TerminalOutput::new(pid, right_winsize, right_side_x, active_terminal_y_coords);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, right_winsize.ws_col, right_winsize.ws_row);

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&left_winszie);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(active_terminal_pid, left_winszie.ws_col, left_winszie.ws_row);
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    fn get_active_terminal (&self) -> Option<&TerminalOutput> {
        match self.active_terminal {
            Some(active_terminal) => self.terminals.get(&active_terminal),
            None => None
        }
    }
    fn get_active_terminal_id (&self) -> Option<RawFd> {
        match self.active_terminal {
            Some(active_terminal) => Some(self.terminals.get(&active_terminal).unwrap().pid),
            None => None
        }
    }
    pub fn handle_pty_event(&mut self, pid: RawFd, event: VteEvent) {
        let terminal_output = self.terminals.get_mut(&pid).unwrap();
        terminal_output.handle_event(event);
    }
    pub fn write_to_active_terminal(&mut self, byte: u8) {
        if let Some(active_terminal_id) = &self.get_active_terminal_id() {
            let mut buffer = [byte];
            self.os_api.write_to_tty_stdin(*active_terminal_id, &mut buffer).expect("failed to write to terminal");
            self.os_api.tcdrain(*active_terminal_id).expect("failed to drain terminal");
        }
    }
    fn get_active_terminal_cursor_position(&self) -> (usize, usize) { // (x, y)
        let active_terminal = &self.get_active_terminal().unwrap();
        let x = active_terminal.x_coords as usize + active_terminal.cursor_position_in_last_line();
        let y = active_terminal.y_coords + active_terminal.display_rows - 1;
        (x, y as usize)
    }
    pub fn render (&mut self) {
        let mut stdout = self.os_api.get_stdout_writer();
        for (_pid, terminal) in self.terminals.iter_mut() {
            if let Some(vte_output) = terminal.buffer_as_vte_output() {

                // write boundaries
                if terminal.x_coords + terminal.display_cols < self.full_screen_ws.ws_col {
                    let boundary_x_coords = terminal.x_coords + terminal.display_cols;
                    let mut vte_output_boundaries = String::new();
                    for row in terminal.y_coords..terminal.y_coords + terminal.display_rows {
                        vte_output_boundaries.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", row + 1, boundary_x_coords + 1)); // goto row/col
                        vte_output_boundaries.push_str(&self.vertical_separator.to_string());
                    }
                    stdout.write_all(&vte_output_boundaries.as_bytes()).expect("cannot write to stdout");
                }
                if terminal.y_coords + terminal.display_rows < self.full_screen_ws.ws_row {
                    let boundary_y_coords = terminal.y_coords + terminal.display_rows;
                    let mut vte_output_boundaries = String::new();
                    for col in terminal.x_coords..terminal.x_coords + terminal.display_cols {
                        vte_output_boundaries.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", boundary_y_coords + 1, col + 1)); // goto row/col
                        vte_output_boundaries.push_str(&self.horizontal_separator.to_string());
                    }
                    stdout.write_all(&vte_output_boundaries.as_bytes()).expect("cannot write to stdout");
                }

                stdout.write_all(&vte_output.as_bytes()).expect("cannot write to stdout");
            }
        }
        let (cursor_position_x, cursor_position_y) = self.get_active_terminal_cursor_position();
        let goto_cursor_position = format!("\u{1b}[{};{}H\u{1b}[m", cursor_position_y + 1, cursor_position_x + 1); // goto row/col
        stdout.write_all(&goto_cursor_position.as_bytes()).expect("cannot write to stdout");
        stdout.flush().expect("could not flush");
    }
    fn terminal_ids_directly_left_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        if terminal_to_check.x_coords == 0 {
            return None;
        }
        for (pid, terminal) in self.terminals.iter() {
            if terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords - 1 {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_right_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.x_coords == terminal_to_check.x_coords + terminal_to_check.display_cols + 1 {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.y_coords == terminal_to_check.y_coords + terminal_to_check.display_rows + 1 {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above_with_same_left_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut left_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords == terminal_to_check.x_coords)
            .collect();
        left_aligned_terminals.sort_by(|a, b| { b.y_coords.cmp(&a.y_coords)});

        for terminal in left_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below_with_same_left_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut left_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords == terminal_to_check.x_coords)
            .collect();
        left_aligned_terminals.sort_by(|a, b| { a.y_coords.cmp(&b.y_coords)});

        for terminal in left_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.y_coords + terminal_to_check.display_rows + 1 == terminal.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above_with_same_right_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut right_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords + terminal_to_check.display_cols)
            .collect();
        right_aligned_terminals.sort_by(|a, b| { b.y_coords.cmp(&a.y_coords)});

        for terminal in right_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below_with_same_right_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut right_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords + terminal_to_check.display_cols)
            .collect();
        right_aligned_terminals.sort_by(|a, b| { a.y_coords.cmp(&b.y_coords)});

        for terminal in right_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.y_coords + terminal_to_check.display_rows + 1 == terminal.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_left_with_same_top_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut top_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords == terminal_to_check.y_coords)
            .collect();
        top_aligned_terminals.sort_by(|a, b| { b.x_coords.cmp(&a.x_coords)});

        for terminal in top_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.x_coords + terminal.display_cols + 1 == terminal_to_check.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_left_with_same_bottom_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords + terminal.display_rows == terminal_to_check.y_coords + terminal_to_check.display_rows)
            .collect();
        bottom_aligned_terminals.sort_by(|a, b| { b.x_coords.cmp(&a.x_coords)});

        for terminal in bottom_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.x_coords + terminal.display_cols + 1 == terminal_to_check.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_right_with_same_top_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut top_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords == terminal_to_check.y_coords)
            .collect();
        top_aligned_terminals.sort_by(|a, b| { a.x_coords.cmp(&b.x_coords)});

        for terminal in top_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.x_coords + terminal_to_check.display_cols + 1 == terminal.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_right_with_same_bottom_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords + terminal.display_rows == terminal_to_check.y_coords + terminal_to_check.display_rows)
            .collect();
        bottom_aligned_terminals.sort_by(|a, b| { a.x_coords.cmp(&b.x_coords)});

        for terminal in bottom_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.x_coords + terminal_to_check.display_cols + 1 == terminal.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    pub fn resize_left (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_to_the_left = self.terminal_ids_directly_left_of(&active_terminal_id);
            let terminals_to_the_right = self.terminal_ids_directly_right_of(&active_terminal_id);
            match (terminals_to_the_left, terminals_to_the_right) {
                (_, Some(mut terminals_to_the_right)) => {
                    // reduce to the left
                    let terminal_borders_to_the_right: HashSet<u16> = terminals_to_the_right.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_right.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_right.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.reduce_width_left(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_right.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_right {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_width_left(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_to_the_left), None) => {
                    // increase to the left 
                    let terminal_borders_to_the_left: HashSet<u16> = terminals_to_the_left.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_left.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_left.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.increase_width_left(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_left.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_left {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_width_left(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_right (&mut self) {
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_to_the_left = self.terminal_ids_directly_left_of(&active_terminal_id);
            let terminals_to_the_right = self.terminal_ids_directly_right_of(&active_terminal_id);
            match (terminals_to_the_left, terminals_to_the_right) {
                (_, Some(mut terminals_to_the_right)) => {
                    // increase to the right
                    let terminal_borders_to_the_right: HashSet<u16> = terminals_to_the_right.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_right.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_right.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };

                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.increase_width_right(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_right.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_right {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_width_right(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_to_the_left), None) => {
                    // reduce to the right
                    let terminal_borders_to_the_left: HashSet<u16> = terminals_to_the_left.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_left.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_left.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };

                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.reduce_width_right(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_left.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_left {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_width_right(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_down (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_below = self.terminal_ids_directly_below(&active_terminal_id);
            let terminals_above = self.terminal_ids_directly_above(&active_terminal_id);
            match (terminals_below, terminals_above) {
                (_, Some(mut terminals_above)) => {
                    // reduce down
                    let terminal_borders_above: HashSet<u16> = terminals_above.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_above.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_above.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border 
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.reduce_height_down(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_above.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border
                    });
                    for terminal_id in terminals_above {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_height_down(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_below), None) => {
                    // increase down
                    let terminal_borders_below: HashSet<u16> = terminals_below.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_below.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_below.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.increase_height_down(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_below.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border 
                    });
                    for terminal_id in terminals_below {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_height_down(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_up (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_below = self.terminal_ids_directly_below(&active_terminal_id);
            let terminals_above = self.terminal_ids_directly_above(&active_terminal_id);
            match (terminals_below, terminals_above) {
                (_, Some(mut terminals_above)) => {
                    // reduce down
                    let terminal_borders_above: HashSet<u16> = terminals_above.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_above.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_above.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border 
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.increase_height_up(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_above.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border
                    });
                    for terminal_id in terminals_above {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_height_up(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_below), None) => {
                    // increase down
                    let terminal_borders_below: HashSet<u16> = terminals_below.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_below.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_below.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols); // TODO: + 1?

                    active_terminal.reduce_height_up(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_below.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border 
                    });
                    for terminal_id in terminals_below {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_height_up(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn move_focus(&mut self) {
        if self.terminals.is_empty() {
            return;
        }
        let active_terminal_id = self.get_active_terminal_id().unwrap();
        let terminal_ids: Vec<RawFd> = self.terminals.keys().copied().collect(); // TODO: better, no allocations
        let first_terminal = terminal_ids.get(0).unwrap();
        let active_terminal_id_position = terminal_ids.iter().position(|id| id == &active_terminal_id).unwrap();
        if let Some(next_terminal) = terminal_ids.get(active_terminal_id_position + 1) {
            self.active_terminal = Some(*next_terminal);
        } else {
            self.active_terminal = Some(*first_terminal);
        }
        self.render();
    }
}

enum PtyInstruction {
    SpawnTerminalVertically,
    SpawnTerminalHorizontally,
    Quit
}

struct PtyBus {
    receive_pty_instructions: Receiver<PtyInstruction>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_screen_instructions: Sender<ScreenInstruction>,
    os_input: Box<dyn OsApi>,
}

impl PtyBus {
    pub fn new (send_screen_instructions: Sender<ScreenInstruction>, os_input: Box<dyn OsApi>) -> Self {
        let (send_pty_instructions, receive_pty_instructions): (Sender<PtyInstruction>, Receiver<PtyInstruction>) = channel();
        PtyBus {
            send_pty_instructions,
            send_screen_instructions,
            receive_pty_instructions,
            os_input,
        }
    }
    pub fn spawn_terminal_vertically(&mut self) {
        let (pid_primary, _pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal();
        task::spawn({
            let send_screen_instructions = self.send_screen_instructions.clone();
            let os_input = self.os_input.clone();
            async move {
                let mut vte_parser = vte::Parser::new();
                let mut vte_event_sender = VteEventSender::new(pid_primary, send_screen_instructions.clone());
                let mut first_terminal_bytes = ReadFromPid::new(&pid_primary, os_input);
                while let Some(bytes) = first_terminal_bytes.next().await {
                    let bytes_is_empty = bytes.is_empty();
                    for byte in bytes {
                        vte_parser.advance(&mut vte_event_sender, byte);
                    }
                    if !bytes_is_empty {
                        send_screen_instructions.send(ScreenInstruction::Render).unwrap();
                    } else {
                        task::sleep(::std::time::Duration::from_millis(10)).await;
                    }
                }
            }
        });
        self.send_screen_instructions.send(ScreenInstruction::VerticalSplit(pid_primary)).unwrap();
    }
    pub fn spawn_terminal_horizontally(&mut self) {
        let (pid_primary, _pid_secondary): (RawFd, RawFd) = self.os_input.spawn_terminal();
        task::spawn({
            let send_screen_instructions = self.send_screen_instructions.clone();
            let os_input = self.os_input.clone();
            async move {
                let mut vte_parser = vte::Parser::new();
                let mut vte_event_sender = VteEventSender::new(pid_primary, send_screen_instructions.clone());
                let mut first_terminal_bytes = ReadFromPid::new(&pid_primary, os_input);
                while let Some(bytes) = first_terminal_bytes.next().await {
                    let bytes_is_empty = bytes.is_empty();
                    for byte in bytes {
                        vte_parser.advance(&mut vte_event_sender, byte);
                    }
                    if !bytes_is_empty {
                        send_screen_instructions.send(ScreenInstruction::Render).unwrap();
                    } else {
                        task::sleep(::std::time::Duration::from_millis(10)).await;
                    }
                }
            }
        });
        self.send_screen_instructions.send(ScreenInstruction::HorizontalSplit(pid_primary)).unwrap();
    }
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
        } else {
            // println!("\r buffer {:?}   ", buffer[0]);
            send_screen_instructions.send(ScreenInstruction::WriteCharacter(buffer[0])).unwrap();
        }
    };
    
    for thread_handler in active_threads {
        thread_handler.join().unwrap();
    }
    // cleanup();
    let reset_style = "\u{1b}[m";
    let goodbye_message = format!("\n\r{}Bye from Mosaic!", reset_style);

    os_input.get_stdout_writer().write(goodbye_message.as_bytes()).unwrap();
    os_input.get_stdout_writer().flush().unwrap();

}
