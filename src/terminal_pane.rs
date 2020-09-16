use ::std::fmt::{self, Display, Formatter};
use ::std::cmp::max;
use ::std::collections::VecDeque;
use ::std::os::unix::io::RawFd;
use ::nix::pty::Winsize;
use ::vte::Perform;

use crate::VteEvent;

const DEBUGGING: bool = false;
const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter { character: ' ', ansi_code: None };

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
    pub x_coords: u16,
    pub y_coords: u16,
    cursor_position: usize,
    newline_indices: Vec<usize>, // canonical line breaks we get from the vt interpreter
    linebreak_indices: Vec<usize>, // linebreaks from line wrapping
    pending_ansi_code: Option<String>, // this is used eg. in a carriage return, where we need to preserve the style
}

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
    pub fn read_buffer_as_lines (&self) -> Vec<Vec<&TerminalCharacter>> {
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
                    if current_line.len() > 0 {
                        for _ in current_line.len()..self.display_cols as usize {
                            current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                        }
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
                    next_newline_index = newline_indices.next();
                }
            }
            if let Some(linebreak_index) = next_linebreak_index {
                if *linebreak_index == i + 1 {
                    // pad line
                    if current_line.len() > 0 {
                        for _ in current_line.len()..self.display_cols as usize {
                            current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                        }
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
                    next_linebreak_index = linebreak_indices.next();
                }
            }
            if output.len() == self.display_rows as usize {
                if current_line.len() > 0 {
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                }
                break;
            }
            let terminal_character = self.characters.get(i).unwrap();
            current_line.push_front(terminal_character);
            if i == 0 {
                if current_line.len() > 0 {
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                }
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
        if self.cursor_position <= self.characters.len() {
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
    pub fn cursor_coordinates (&self) -> (usize, usize) { // (x, y)
        let mut lines_from_end = 0;

        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline_index = newline_indices.next().unwrap_or(&0);
        let mut next_linebreak_index = linebreak_indices.next().unwrap_or(&0);

        let next_line_start = loop {
            let next_line_start = ::std::cmp::max(*next_newline_index, *next_linebreak_index);
            if self.cursor_position >= next_line_start {
                break next_line_start;
            }
            if next_line_start == *next_newline_index {
                next_newline_index = newline_indices.next().unwrap_or(&0);
                lines_from_end += 1;
            }
            if next_line_start == *next_linebreak_index {
                next_linebreak_index = linebreak_indices.next().unwrap_or(&0);
                lines_from_end += 1;
            }
        };
        let y = self.display_rows - lines_from_end; // TODO: this might overflow, fix when introducing scrolling
        let x = self.cursor_position - next_line_start;
        (x, y as usize)
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
