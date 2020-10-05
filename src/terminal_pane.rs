use ::std::fmt::{self, Display, Formatter};
use ::std::cmp::max;
use ::std::collections::VecDeque;
use ::std::os::unix::io::RawFd;
use ::nix::pty::Winsize;
use ::vte::Perform;

use crate::VteEvent;

const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter {
    character: ' ',
    foreground_ansi_codes: None,
    background_ansi_codes: None,
    misc_ansi_codes: None,
    reset_foreground_ansi_code: true,
    reset_background_ansi_code: true,
    reset_misc_ansi_code: true,
};

#[derive(Clone)]
pub struct TerminalCharacter {
    pub character: char,
    pub foreground_ansi_codes: Option<Vec<String>>,
    pub background_ansi_codes: Option<Vec<String>>,
    pub misc_ansi_codes: Option<Vec<String>>,
    pub reset_foreground_ansi_code: bool,
    pub reset_background_ansi_code: bool,
    pub reset_misc_ansi_code: bool,
}

impl PartialEq for TerminalCharacter {
    fn eq(&self, other: &Self) -> bool {
        self.foreground_ansi_codes == other.foreground_ansi_codes &&
        self.background_ansi_codes == other.background_ansi_codes &&
        self.misc_ansi_codes == other.misc_ansi_codes &&
        self.reset_background_ansi_code == other.reset_background_ansi_code &&
        self.reset_foreground_ansi_code == other.reset_foreground_ansi_code &&
        self.reset_misc_ansi_code == other.reset_misc_ansi_code
    }
}

impl Eq for TerminalCharacter {}

impl TerminalCharacter {
    pub fn new (character: char) -> Self {
        TerminalCharacter {
            character,
            foreground_ansi_codes: Some(vec![]),
            background_ansi_codes: Some(vec![]),
            misc_ansi_codes: Some(vec![]),
            reset_foreground_ansi_code: false,
            reset_background_ansi_code: false,
            reset_misc_ansi_code: false,
        }
    }
    pub fn reset_all_ansi_codes(mut self) -> Self {
        if let Some(foreground_ansi_codes) = self.foreground_ansi_codes.as_mut() {
            foreground_ansi_codes.clear();
        }
        if let Some(background_ansi_codes) = self.background_ansi_codes.as_mut() {
            background_ansi_codes.clear();
        }
        if let Some(misc_ansi_codes) = self.misc_ansi_codes.as_mut() {
            misc_ansi_codes.clear();
        }
        self.reset_foreground_ansi_code = true;
        self.reset_background_ansi_code = true;
        self.reset_misc_ansi_code = true;
        self
    }
    pub fn reset_foreground_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(foreground_ansi_codes) = self.foreground_ansi_codes.as_mut() {
            if *should_reset {
                foreground_ansi_codes.clear();
            }
        }
        self.reset_foreground_ansi_code = *should_reset;
        self
    }
    pub fn reset_background_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(background_ansi_codes) = self.background_ansi_codes.as_mut() {
            if *should_reset {
                background_ansi_codes.clear();
            }
        }
        self.reset_background_ansi_code = *should_reset;
        self
    }
    pub fn reset_misc_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(misc_ansi_codes) = self.misc_ansi_codes.as_mut() {
            if *should_reset {
                misc_ansi_codes.clear();
            }
        }
        self.reset_misc_ansi_code = *should_reset;
        self
    }
    pub fn foreground_ansi_codes(mut self, foreground_ansi_codes: &[String]) -> Self {
        self.foreground_ansi_codes = Some(foreground_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn background_ansi_codes(mut self, background_ansi_codes: &[String]) -> Self {
        self.background_ansi_codes = Some(background_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn misc_ansi_codes(mut self, misc_ansi_codes: &[String]) -> Self {
        self.misc_ansi_codes = Some(misc_ansi_codes.iter().cloned().collect());
        self
    }
}


impl Display for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut code_string = String::new(); // TODO: better
        if self.reset_foreground_ansi_code && self.reset_background_ansi_code && self.reset_misc_ansi_code {
            code_string.push_str("\u{1b}[m");
        } else {
            if self.reset_foreground_ansi_code {
                code_string.push_str("\u{1b}[39m");
            }
            if self.reset_background_ansi_code {
                code_string.push_str("\u{1b}[49m");
            }
        }
        if let Some(ansi_codes) = self.foreground_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.background_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.misc_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        write!(f, "{}{}", code_string, self.character)
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
    scroll_region: (usize, usize), // top line index / bottom line index
    reset_foreground_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    reset_background_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    reset_misc_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    pending_foreground_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
    pending_background_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
    pending_misc_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
}

impl TerminalOutput {
    pub fn new (pid: RawFd, ws: Winsize, x_coords: u16, y_coords: u16) -> TerminalOutput {
        TerminalOutput {
            pid,
            characters: vec![],
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            scroll_region: (1, ws.ws_row as usize),
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: true,
            reset_foreground_ansi_code: false,
            reset_background_ansi_code: false,
            reset_misc_ansi_code: false,
            pending_foreground_ansi_codes: vec![],
            pending_background_ansi_codes: vec![],
            pending_misc_ansi_codes: vec![],
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
                // TODO: maybe remove <= self.cursor_position? why not reflow lines after cursor?
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
            if let Some(newline_index) = next_newline_index {
                if *newline_index == i {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_newline_index = newline_indices.next();
                    continue; // we continue here in case there's another new line in this index
                }
            }
            if let Some(linebreak_index) = next_linebreak_index {
                if *linebreak_index == i {
                    // pad line
                    if current_line.len() > 0 {
                        for _ in current_line.len()..self.display_cols as usize {
                            current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                        }
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
                    next_linebreak_index = linebreak_indices.next();
                    continue; // we continue here in case there's another new line in this index
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
            i -= 1;
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
    pub fn cursor_coordinates (&self) -> (usize, usize) { // (x, y)
        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline = newline_indices.next();
        let mut next_linebreak = linebreak_indices.next();

        let mut lines_from_end = 0;
        let mut current_line_start_index = 0;
        loop {
            match max(next_newline, next_linebreak) {
                Some(next_line_index) => {
                    if next_line_index <= &self.cursor_position {
                        current_line_start_index = *next_line_index;
                        break;
                    } else {
                        lines_from_end += 1;
                        if Some(next_line_index) == next_newline {
                            next_newline = newline_indices.next();
                        } else if Some(next_line_index) == next_linebreak {
                            next_linebreak = linebreak_indices.next();
                        }
                    }
                },
                None => break,
            }
        }
        let index_of_last_row = self.display_rows as usize - 1;
        let y = index_of_last_row - lines_from_end;
        let x = self.cursor_position - current_line_start_index;
        (x, y)
    }
    fn index_of_end_of_canonical_line(&self, index_in_line: usize) -> usize {
        let newlines = self.newline_indices.iter().rev();
        let mut index_of_end_of_canonical_line = self.characters.len();
        for line_index in newlines {
            if *line_index <= index_in_line {
                break
            }
            if index_of_end_of_canonical_line > *line_index {
                index_of_end_of_canonical_line = *line_index;
            }
        }
        index_of_end_of_canonical_line
    }
    fn index_of_beginning_of_line (&self, index_in_line: usize) -> usize {
        let last_newline_index = self.newline_indices.iter().rev().find(|&&n_i| n_i <= index_in_line).unwrap_or(&0);
        let last_linebreak_index = self.linebreak_indices.iter().rev().find(|&&l_i| l_i <= index_in_line).unwrap_or(&0);
        max(*last_newline_index, *last_linebreak_index)
    }
    fn scroll_region_line_indices (&self) -> (usize, usize) {
        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline = newline_indices.next();
        let mut next_linebreak = linebreak_indices.next();

        let mut lines_from_end = 0;

        let scroll_end_index_from_screen_bottom = self.display_rows as usize - self.scroll_region.1;
        let scroll_start_index_from_screen_bottom = scroll_end_index_from_screen_bottom + (self.scroll_region.1 - self.scroll_region.0);

        let mut scroll_region_start_index = None;
        let mut scroll_region_end_index = None;

        loop {
            match max(next_newline, next_linebreak) {
                Some(next_line_index) => {
                    if lines_from_end == scroll_start_index_from_screen_bottom {
                        scroll_region_start_index = Some(next_line_index);
                    }
                    if lines_from_end == scroll_end_index_from_screen_bottom {
                        scroll_region_end_index = Some(next_line_index);
                    }
                    if scroll_region_start_index.is_some() && scroll_region_end_index.is_some() {
                        break;
                    }
                    if Some(next_line_index) == next_newline {
                        next_newline = newline_indices.next();
                    } else if Some(next_line_index) == next_linebreak {
                        next_linebreak = linebreak_indices.next();
                    }
                    lines_from_end += 1;
                },
                None => break,
            }
        }
        (*scroll_region_start_index.unwrap_or(&0), *scroll_region_end_index.unwrap_or(&0))
    }
    fn index_of_next_line_after(&self, index_in_line: usize) -> usize {
        let last_newline_index = self.newline_indices.iter().find(|&&n_i| n_i >= index_in_line).unwrap_or(&0);
        let last_linebreak_index = self.linebreak_indices.iter().find(|&&l_i| l_i >= index_in_line).unwrap_or(&0);
        max(*last_newline_index, *last_linebreak_index)
    }
    fn insert_empty_lines_at_cursor(&mut self, count: usize) {
        for _ in 0..count {
            self.delete_last_line_in_scroll_region();
            let start_of_current_line = self.index_of_beginning_of_line(self.cursor_position);
            let end_of_current_line = self.index_of_next_line_after(self.cursor_position);
            for i in 0..end_of_current_line - start_of_current_line {
                self.characters.insert(start_of_current_line + i, EMPTY_TERMINAL_CHARACTER.clone())
            }
        }

    }
    fn delete_last_line_in_scroll_region(&mut self) {
        if let Some(newline_index_of_scroll_region_end) = self.get_line_position_on_screen(self.scroll_region.1) {
            let end_of_last_scroll_region_line = self.get_line_position_on_screen(self.scroll_region.1 + 1).unwrap();
            &self.characters.drain(newline_index_of_scroll_region_end..end_of_last_scroll_region_line);
        }
    }
    fn delete_first_line_in_scroll_region(&mut self) {
        if let Some(newline_index_of_scroll_region_start) = self.get_line_position_on_screen(self.scroll_region.0) {
            let end_of_first_scroll_region_line = self.get_line_position_on_screen(self.scroll_region.0 + 1).unwrap();
            let removed_count = {
                let removed_line = &self.characters.drain(newline_index_of_scroll_region_start..end_of_first_scroll_region_line);
                removed_line.len()
            };
            let newline_index_of_scroll_region_end = self.get_line_position_on_screen(self.scroll_region.1).unwrap();
            for i in 0..removed_count {
                self.characters.insert(newline_index_of_scroll_region_end + i, EMPTY_TERMINAL_CHARACTER.clone())
            }
            // TODO: if removed_count is larger than the line it was inserted it, recalculate all
            // newline_indices after it
        }
    }
    fn get_line_position_on_screen(&self, index_on_screen: usize) -> Option<usize> {
        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline = newline_indices.next();
        let mut next_linebreak = linebreak_indices.next();

        let mut lines_from_end = 0; // 1 because we're counting and not indexing TODO: fix this
        loop {
            match max(next_newline, next_linebreak) {
                Some(next_line_index) => {
                    if index_on_screen == self.display_rows as usize - lines_from_end {
                        return Some(*next_line_index);
                    } else {
                        lines_from_end += 1;
                    }
                    if lines_from_end > self.display_rows as usize {
                        return None;
                    }
                    if Some(next_line_index) == next_newline {
                        next_newline = newline_indices.next();
                    } else if Some(next_line_index) == next_linebreak {
                        next_linebreak = linebreak_indices.next();
                    }
                },
                None => {
                    if index_on_screen == self.display_rows as usize - lines_from_end {
                        return Some(0);
                    } else {
                        return None;
                    }
                }
            }
        }
    }
    // TODO: better naming of these two functions
    fn get_line_index_on_screen (&self, position_in_characters: usize) -> Option<usize> {
        let mut newline_indices = self.newline_indices.iter().rev();
        let mut linebreak_indices = self.linebreak_indices.iter().rev();

        let mut next_newline = newline_indices.next();
        let mut next_linebreak = linebreak_indices.next();

        let mut lines_from_end = 0;
        loop {
            match max(next_newline, next_linebreak) {
                Some(next_line_index) => {
                    if *next_line_index <= position_in_characters {
                        break;
                    } else {
                        lines_from_end += 1;
                        if Some(next_line_index) == next_newline {
                            next_newline = newline_indices.next();
                        } else if Some(next_line_index) == next_linebreak {
                            next_linebreak = linebreak_indices.next();
                        }
                    }
                },
                None => break,
            }
        }
        if lines_from_end > self.display_rows as usize {
            None
        } else {
            Some(self.display_rows as usize - lines_from_end)
        }
    }
    fn add_newline (&mut self) {
        let nearest_line_end = self.index_of_end_of_canonical_line(self.cursor_position);
        let current_line_index_on_screen = self.get_line_index_on_screen(self.cursor_position);
        if nearest_line_end == self.characters.len() {
            self.newline_indices.push(nearest_line_end);
            self.cursor_position = nearest_line_end;
        } else if current_line_index_on_screen == Some(self.scroll_region.1) { // end of scroll region
            // shift all lines in scroll region up
            self.delete_first_line_in_scroll_region();
        } else {
            // we shouldn't add a new line in the middle of the text
            // in this case, we'll move to the next existing line and it
            // will be overriden as we print on it
            self.cursor_position = nearest_line_end;
        }
        self.pending_foreground_ansi_codes.clear();
        self.pending_background_ansi_codes.clear();
        self.pending_misc_ansi_codes.clear();
        self.should_render = true;
    }
    fn move_to_beginning_of_line (&mut self) {
        let last_newline_index = self.index_of_beginning_of_line(self.cursor_position);
        self.cursor_position = last_newline_index;
        self.should_render = true;
    }
}

fn debug_log_to_file (message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new().append(true).create(true).open("/tmp/mosaic-log.txt").unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

impl vte::Perform for TerminalOutput {
    fn print(&mut self, c: char) {


        // while not ideal that we separate the reset and actual code logic here,
        // combining them is a question of rendering performance and not refactoring,
        // so will be addressed separately
        let terminal_character = TerminalCharacter::new(c)
            .reset_foreground_ansi_code(&self.reset_foreground_ansi_code)
            .reset_background_ansi_code(&self.reset_background_ansi_code)
            .reset_misc_ansi_code(&self.reset_misc_ansi_code)
            .foreground_ansi_codes(&self.pending_foreground_ansi_codes)
            .background_ansi_codes(&self.pending_background_ansi_codes)
            .misc_ansi_codes(&self.pending_misc_ansi_codes);

        if self.characters.len() > self.cursor_position {
            self.characters.remove(self.cursor_position);
            self.characters.insert(self.cursor_position, terminal_character);
            if !self.newline_indices.contains(&(self.cursor_position + 1)) {
                // advancing the cursor beyond the borders of the line has to be done explicitly
                self.cursor_position += 1;
            }
        } else {
            for _ in self.characters.len()..self.cursor_position {
                self.characters.push(EMPTY_TERMINAL_CHARACTER.clone());
            };
            self.characters.push(terminal_character);

            let start_of_last_line = self.index_of_beginning_of_line(self.cursor_position);
            let difference_from_last_newline = self.cursor_position - start_of_last_line;
            if difference_from_last_newline == self.display_cols as usize {
                self.linebreak_indices.push(self.cursor_position);
            }
            self.cursor_position += 1;
        }
    }

    fn execute(&mut self, byte: u8) {
        if byte == 13 { // 0d, carriage return
            self.move_to_beginning_of_line();
        } else if byte == 08 { // backspace
            self.cursor_position -= 1;
        } else if byte == 10 { // 0a, newline
            self.add_newline();
        }
    }

    fn hook(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        // TBD
    }

    fn put(&mut self, byte: u8) {
        // TBD
    }

    fn unhook(&mut self) {
        // TBD
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        // TBD
    }

    fn csi_dispatch(&mut self, params: &[i64], intermediates: &[u8], ignore: bool, c: char) {
        if c == 'm' {
            // TODO: handle misc codes specifically
            // like here: https://github.com/alacritty/alacritty/blob/46c0f352c40ecb68653421cb178a297acaf00c6d/alacritty_terminal/src/ansi.rs#L1176
            if params.is_empty() || params[0] == 0 {
                self.reset_foreground_ansi_code = true;
                self.reset_background_ansi_code = true;
                self.reset_misc_ansi_code = true;
                self.pending_foreground_ansi_codes.clear();
                self.pending_background_ansi_codes.clear();
                self.pending_misc_ansi_codes.clear();
            } else if params[0] == 39 {
                self.reset_foreground_ansi_code = true;
                self.pending_foreground_ansi_codes.clear();
            } else if params[0] == 49 {
                self.reset_background_ansi_code = true;
                self.pending_background_ansi_codes.clear();
            } else if params[0] == 38 {
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_foreground_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_foreground_ansi_code = false;
            } else if params[0] == 48 {
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_background_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_background_ansi_code = false;
            } else {
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_misc_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_misc_ansi_code = false;
            }
        } else if c == 'C' { // move cursor forward
            let move_by = params[0] as usize;
            let closest_newline = self.newline_indices.iter().find(|x| x > &&self.cursor_position).copied();
            let closest_linebreak = self.linebreak_indices.iter().find(|x| x > &&self.cursor_position).copied();
            let max_move_position = match (closest_newline, closest_linebreak) {
                (Some(closest_newline), Some(closest_linebreak)) => {
                    ::std::cmp::min(
                        closest_newline,
                        closest_linebreak
                    )
                },
                (Some(closest_newline), None) => {
                    closest_newline
                },
                (None, Some(closest_linebreak)) => {
                    closest_linebreak
                },
                (None, None) => {
                    let last_line_start = ::std::cmp::max(self.newline_indices.last(), self.linebreak_indices.last()).unwrap_or(&0);
                    let position_in_last_line = self.cursor_position - last_line_start;
                    let columns_from_last_line_end = self.display_cols as usize - position_in_last_line;
                    self.cursor_position + columns_from_last_line_end
                }
            };
            if self.cursor_position + move_by < max_move_position {
                self.cursor_position += move_by;
            } else {
                self.cursor_position = max_move_position;
            }

        } else if c == 'K' { // clear line (0 => right, 1 => left, 2 => all)
            if params[0] == 0 {
                let newlines = self.newline_indices.iter().rev();
                let mut delete_until = self.characters.len();
                for newline_index in newlines {
                    if newline_index < &self.cursor_position {
                        break;
                    }
                    delete_until = *newline_index;
                }
                // TODO: better
                for i in self.cursor_position..delete_until {
                    self.characters[i] = EMPTY_TERMINAL_CHARACTER.clone();
                }
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
            } else if params[0] == 2 {
                // TODO: this also deletes all the scrollback buffer, it needs to be adjusted
                // for scrolling
                self.characters.clear();
                self.linebreak_indices.clear();
                self.newline_indices.clear();
            }
        } else if c == 'H' { // goto row/col
            let (row, col) = if params.len() == 1 {
                (params[0] as usize, 0) // TODO: is this always correct ?
            } else {
                (params[0] as usize - 1, params[1] as usize - 1) // we subtract 1 here because this csi is 1 indexed and we index from 0
            };
            if row == 0 {
                self.cursor_position = col;
            } else if let Some(index_of_start_of_row) = self.newline_indices.get(row - 1) {
                self.cursor_position = index_of_start_of_row + col;
            } else {
                let start_of_last_line = *self.newline_indices.last().unwrap_or(&0);
                let num_of_lines_to_add = row - self.newline_indices.len();
                for i in 0..num_of_lines_to_add {
                    self.newline_indices.push(start_of_last_line + ((i + 1) * self.display_cols as usize));
                }
                let index_of_start_of_row = self.newline_indices.get(row - 1).unwrap();
                self.cursor_position = index_of_start_of_row + col;
            }
        } else if c == 'A' { // move cursor up until edge of screen
            let move_up_count = if params[0] == 0 { 1 } else { params[0] };
            let newlines = self.newline_indices.iter().rev();
            let mut position_in_line = None;
            let mut lines_traversed = 0;
            for newline_index in newlines {
                if position_in_line.is_some() {
                    lines_traversed += 1;
                }
                if newline_index < &self.cursor_position && position_in_line.is_none() {
                    // this is the current cursor line
                    position_in_line = Some(self.cursor_position - newline_index);
                }
                if lines_traversed == move_up_count {
                    self.cursor_position = newline_index + position_in_line.unwrap();
                    return;
                    // break;
                }
            }
            // if we reached this point, we were asked to move more lines than we have
            // so let's move the maximum before slipping off-screen
            // TODO: this is buggy and moves to the first line rather than the first line on screen
            // fix this
            self.cursor_position = self.newline_indices.iter().next().unwrap_or(&0) + position_in_line.unwrap_or(0);
        } else if c == 'D' {
            // move cursor backwards, stop at left edge of screen
            let reduce_by = if params[0] == 0 { 1 } else { params[0] as usize };
            let beginning_of_current_line = self.index_of_beginning_of_line(self.cursor_position);
            let max_reduce = self.cursor_position - beginning_of_current_line;
            if reduce_by > max_reduce {
                self.cursor_position -= max_reduce;
            } else {
                self.cursor_position -= reduce_by;
            }
        } else if c == 'l' {
            // TBD
        } else if c == 'h' {
            // TBD
        } else if c == 'r' {
            debug_log_to_file(format!("\rparams {:?}", params));
            if params.len() > 1 {
                // TODO: why do we need this if? what does a 1 parameter 'r' mean?
                self.scroll_region = (params[0] as usize, params[1] as usize);
            }
        } else if c == 't' {
            // TBD - title?
        } else if c == 'n' {
            // TBD - device status report
        } else if c == 'c' {
            // TBD - identify terminal
        } else if c == 'M' {
            // delete lines if currently inside scroll region
            let line_count_to_delete = params[0];
            for _ in 0..line_count_to_delete {
                // TODO: better, do this in bulk
                self.delete_first_line_in_scroll_region()
            }
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            let line_count_to_add = params[0];
            self.insert_empty_lines_at_cursor(line_count_to_add as usize);
        } else {
            println!("unhandled csi: {:?}->{:?}", c, params);
            panic!("aaa!!!");
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        // TBD
    }
}
