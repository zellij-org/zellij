use ::std::fmt::{self, Display, Debug, Formatter};
use ::std::cmp::max;
use ::std::collections::VecDeque;
use ::std::os::unix::io::RawFd;
use ::nix::pty::Winsize;
use ::vte::Perform;

use crate::VteEvent;
use crate::boundaries::Rect;

const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter {
    character: ' ',
    foreground_ansi_codes: None,
    background_ansi_codes: None,
    strike_ansi_codes:None,
    hidden_ansi_codes:None,
    reverse_ansi_codes:None,
    blink_ansi_codes:None,
    underline_ansi_codes:None,
    bold_dim_ansi_codes:None,
    italic_ansi_codes:None,
    misc_ansi_codes: None,
    reset_foreground_ansi_code: true,
    reset_background_ansi_code: true,
    reset_bold_ansi_codes: true,
    reset_italic_ansi_code: true,
    reset_underline_ansi_codes: true,
    reset_blink_ansi_code: true,
    reset_reverse_ansi_codes: true,
    reset_hidden_ansi_codes: true,
    reset_strike_ansi_codes: true,
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

    pub reset_bold_ansi_codes: bool,
    pub reset_italic_ansi_code: bool,
    pub reset_underline_ansi_codes: bool,
    pub reset_blink_ansi_code: bool,
    pub reset_reverse_ansi_codes: bool,
    pub reset_hidden_ansi_codes: bool,
    pub reset_strike_ansi_codes: bool,

    pub strike_ansi_codes: Option<Vec<String>>,
    pub hidden_ansi_codes: Option<Vec<String>>,
    pub reverse_ansi_codes: Option<Vec<String>>,
    pub blink_ansi_codes: Option<Vec<String>>,
    pub underline_ansi_codes: Option<Vec<String>>,
    pub bold_dim_ansi_codes: Option<Vec<String>>,
    pub italic_ansi_codes: Option<Vec<String>>,
}

impl TerminalCharacter {
    pub fn new (character: char) -> Self {
        TerminalCharacter {
            character,
            foreground_ansi_codes: Some(vec![]),
            background_ansi_codes: Some(vec![]),
            strike_ansi_codes: Some(vec![]),
            hidden_ansi_codes: Some(vec![]),
            reverse_ansi_codes: Some(vec![]),
            blink_ansi_codes: Some(vec![]),
            underline_ansi_codes: Some(vec![]),
            bold_dim_ansi_codes: Some(vec![]),
            italic_ansi_codes: Some(vec![]),
            misc_ansi_codes: Some(vec![]),
            reset_foreground_ansi_code: false,
            reset_background_ansi_code: false,
            reset_bold_ansi_codes: false,
            reset_italic_ansi_code: false,
            reset_underline_ansi_codes: false,
            reset_blink_ansi_code: false,
            reset_reverse_ansi_codes: false,
            reset_hidden_ansi_codes: false,
            reset_strike_ansi_codes: false,
            reset_misc_ansi_code: false,
        }
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

    pub fn reset_bold_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(bold_ansi_codes) = self.bold_dim_ansi_codes.as_mut() {
            if *should_reset {
                bold_ansi_codes.clear();
            }
        }
        self.reset_bold_ansi_codes = *should_reset;
        self
    }
    pub fn reset_bold_dim_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(bold_dim_ansi_codes) = self.bold_dim_ansi_codes.as_mut() {
            if *should_reset {
                bold_dim_ansi_codes.clear();
            }
        }
        self.reset_bold_ansi_codes = *should_reset;
        self
    }
    pub fn reset_italic_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(italic_ansi_codes) = self.italic_ansi_codes.as_mut() {
            if *should_reset {
                italic_ansi_codes.clear();
            }
        }
        self.reset_italic_ansi_code = *should_reset;
        self
    }
    pub fn reset_underline_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(underline_ansi_codes) = self.underline_ansi_codes.as_mut() {
            if *should_reset {
                underline_ansi_codes.clear();
            }
        }
        self.reset_underline_ansi_codes = *should_reset;
        self
    }
    pub fn reset_blink_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(blink_ansi_codes) = self.blink_ansi_codes.as_mut() {
            if *should_reset {
                blink_ansi_codes.clear();
            }
        }
        self.reset_blink_ansi_code = *should_reset;
        self
    }
    pub fn reset_reverse_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(reverse_ansi_codes) = self.reverse_ansi_codes.as_mut() {
            if *should_reset {
                reverse_ansi_codes.clear();
            }
        }
        self.reset_reverse_ansi_codes = *should_reset;
        self
    }
    pub fn reset_hidden_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(hidden_ansi_codes) = self.hidden_ansi_codes.as_mut() {
            if *should_reset {
                hidden_ansi_codes.clear();
            }
        }
        self.reset_hidden_ansi_codes = *should_reset;
        self
    }
    pub fn reset_strike_ansi_code(mut self, should_reset: &bool) -> Self {
        if let Some(strike_ansi_codes) = self.strike_ansi_codes.as_mut() {
            if *should_reset {
                strike_ansi_codes.clear();
            }
        }
        self.reset_strike_ansi_codes = *should_reset;
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

    pub fn bold_ansi_codes(mut self, bold_ansi_codes: &[String]) -> Self {
        self.bold_dim_ansi_codes = Some(bold_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn dim_ansi_codes(mut self, dim_ansi_codes: &[String]) -> Self {
        if let Some(bold_dim_ansi_codes) = self.bold_dim_ansi_codes.as_mut() {
            // TODO: better
            for ansi_code in dim_ansi_codes {
                bold_dim_ansi_codes.push(ansi_code.clone())
            }
        } else {
            self.bold_dim_ansi_codes = Some(dim_ansi_codes.iter().cloned().collect());
        }
        self
    }
    pub fn italic_ansi_codes(mut self, italic_ansi_codes: &[String]) -> Self {
        self.italic_ansi_codes = Some(italic_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn underline_ansi_codes(mut self, underline_ansi_codes: &[String]) -> Self {
        self.underline_ansi_codes = Some(underline_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn blink_slow_ansi_codes(mut self, blink_slow_ansi_codes: &[String]) -> Self {
        if let Some(blink_ansi_codes) = self.blink_ansi_codes.as_mut() {
            // TODO: better
            for ansi_code in blink_slow_ansi_codes {
                blink_ansi_codes.push(ansi_code.clone())
            }
        } else {
            self.blink_ansi_codes = Some(blink_slow_ansi_codes.iter().cloned().collect());
        }
        self
    }
    pub fn blink_fast_ansi_codes(mut self, blink_fast_ansi_codes: &[String]) -> Self {
        if let Some(blink_ansi_codes) = self.blink_ansi_codes.as_mut() {
            // TODO: better
            for ansi_code in blink_fast_ansi_codes {
                blink_ansi_codes.push(ansi_code.clone())
            }
        } else {
            self.blink_ansi_codes = Some(blink_fast_ansi_codes.iter().cloned().collect());
        }
        self
    }
    pub fn reverse_ansi_codes(mut self, reverse_ansi_codes: &[String]) -> Self {
        self.reverse_ansi_codes = Some(reverse_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn hidden_ansi_codes(mut self, hidden_ansi_codes: &[String]) -> Self {
        self.hidden_ansi_codes = Some(hidden_ansi_codes.iter().cloned().collect());
        self
    }
    pub fn strike_ansi_codes(mut self, strike_ansi_codes: &[String]) -> Self {
        self.strike_ansi_codes = Some(strike_ansi_codes.iter().cloned().collect());
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

        if self.reset_foreground_ansi_code {
            code_string.push_str("\u{1b}[39m");
        }
        if self.reset_background_ansi_code {
            code_string.push_str("\u{1b}[49m");
        }
        if self.reset_bold_ansi_codes {
            code_string.push_str("\u{1b}[21m");
        }
        if self.reset_italic_ansi_code {
            code_string.push_str("\u{1b}[23m");
        }
        if self.reset_underline_ansi_codes {
            code_string.push_str("\u{1b}[24m");
        }
        if self.reset_blink_ansi_code {
            code_string.push_str("\u{1b}[25m");
        }
        if self.reset_reverse_ansi_codes {
            code_string.push_str("\u{1b}[27m");
        }
        if self.reset_hidden_ansi_codes {
            code_string.push_str("\u{1b}[28m");
        }
        if self.reset_strike_ansi_codes {
            code_string.push_str("\u{1b}[29m");
        }
        if self.reset_misc_ansi_code {
            // ideally, this should not happen, it means we missed some category of ansi
            // reset/set codes
            code_string.push_str("\u{1b}[m"); // resets all styles
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

        if let Some(ansi_codes) = self.strike_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.hidden_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.reverse_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.blink_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.underline_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.bold_dim_ansi_codes.as_ref() {
            for code in ansi_codes {
                code_string.push_str(&code);
            }
        }
        if let Some(ansi_codes) = self.italic_ansi_codes.as_ref() {
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

struct Grid <'a>{
    pub cells: Vec<Vec<&'a TerminalCharacter>>, // TODO: use references
    columns: usize,
    rows: usize,
}

impl<'a> Grid <'a>{
    pub fn new(characters: &'a [TerminalCharacter], newlines: &[usize], columns: usize, rows: usize) -> Self {

        let mut output: VecDeque<Vec<&TerminalCharacter>> = VecDeque::new();
        let mut i = characters.len();
        let mut current_line: VecDeque<&TerminalCharacter> = VecDeque::new();

        let mut newlines = newlines.iter().rev();

        let mut next_newline_index = newlines.next();

        loop {
            if let Some(newline_index) = next_newline_index {
                if *newline_index == i {
                    // pad line
                    for _ in current_line.len()..columns {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    next_newline_index = newlines.next();
                    continue; // we continue here in case there's another new line in this index
                }
            }
            if output.len() == rows as usize {
                if current_line.len() > 0 {
                    for _ in current_line.len()..columns as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                }
                break;
            }
            i -= 1;
            let terminal_character = characters.get(i).unwrap();
            current_line.push_front(terminal_character);
            if i == 0 {
                if current_line.len() > 0 {
                    for _ in current_line.len()..columns as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                }
                break;
            }
        }
        if output.len() < rows {
            let mut empty_line = vec![];
            for _ in 0..columns {
                empty_line.push(&EMPTY_TERMINAL_CHARACTER);
            }
            for _ in output.len()..rows as usize {
                output.push_back(Vec::from(empty_line.clone()));
            }
        }

        let cells = Vec::from(output);

        Grid {
            cells,
            columns,
            rows,
        }
    }
    pub fn add_empty_lines(&mut self, at_index: usize, count: usize) {
        let empty_line = self.create_empty_line();
        for i in 0..count {
            self.cells.insert(at_index + i, empty_line.clone());
        }
    }
    pub fn delete_lines(&mut self, at_index: usize, count: usize) {
        for _ in 0..count {
            self.cells.remove(at_index);
        }
    }
    pub fn serialize(&self) -> (Vec<TerminalCharacter>, Vec<usize>) {
        let mut characters: Vec<TerminalCharacter> = vec![];
        let mut newline_indices: Vec<usize> = vec![];
        for line in &self.cells {
            for character in line.iter().copied() {
                characters.push(character.clone());
            }
            let last_newline_index = newline_indices.last().copied().unwrap_or(0);
            newline_indices.push(last_newline_index + line.len());
        }
        newline_indices.pop(); // no newline at the end of the grid, TODO: better
        (characters, newline_indices)
    }
    fn create_empty_line(&self) -> Vec<&'a TerminalCharacter> {
        let mut empty_line = vec![];
        for _ in 0..self.columns {
            empty_line.push(&EMPTY_TERMINAL_CHARACTER);
        }
        empty_line
    }
}
impl<'a> Debug for Grid <'a>{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for line in &self.cells {
            writeln!(f, "{:?}", line)?;
        }
        Ok(())
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
    scroll_region: Option<(usize, usize)>, // top line index / bottom line index
    reset_foreground_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    reset_background_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    reset_bold_ansi_code: bool,
    reset_bold_dim_ansi_code: bool,
    reset_italic_ansi_code: bool,
    reset_underline_ansi_code: bool,
    reset_blink_ansi_code: bool,
    reset_reverse_ansi_code: bool,
    reset_hidden_ansi_code: bool,
    reset_strike_ansi_code: bool,
    reset_misc_ansi_code: bool, // this is a performance optimization, rather than placing and looking for the ansi reset code in pending_ansi_codes
    pending_foreground_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
    pending_background_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
    pending_misc_ansi_codes: Vec<String>, // this is used eg. in a carriage return, where we need to preserve the style
    pending_bold_ansi_codes: Vec<String>,
    pending_dim_ansi_codes: Vec<String>,
    pending_italic_ansi_codes: Vec<String>,
    pending_underline_ansi_codes: Vec<String>,
    pending_blink_slow_ansi_codes: Vec<String>,
    pending_blink_fast_ansi_codes: Vec<String>,
    pending_reverse_ansi_codes: Vec<String>,
    pending_hidden_ansi_codes: Vec<String>,
    pending_strike_ansi_codes: Vec<String>,
}

impl Rect for &mut TerminalOutput {
    fn x(&self) -> usize {
        self.x_coords as usize
    }
    fn y(&self) -> usize {
        self.y_coords as usize
    }
    fn rows(&self) -> usize {
        self.display_rows as usize
    }
    fn columns(&self) -> usize {
        self.display_cols as usize
    }
}

impl TerminalOutput {
    pub fn new (pid: RawFd, ws: Winsize, x_coords: u16, y_coords: u16) -> TerminalOutput {
        TerminalOutput {
            pid,
            characters: vec![],
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            scroll_region: None,
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: true,
            reset_foreground_ansi_code: false,
            reset_background_ansi_code: false,
            reset_misc_ansi_code: false,
            reset_bold_ansi_code: false,
            reset_bold_dim_ansi_code: false,
            reset_italic_ansi_code: false,
            reset_underline_ansi_code: false,
            reset_blink_ansi_code: false,
            reset_reverse_ansi_code: false,
            reset_hidden_ansi_code: false,
            reset_strike_ansi_code: false,
            pending_foreground_ansi_codes: vec![],
            pending_background_ansi_codes: vec![],
            pending_misc_ansi_codes: vec![],
            pending_bold_ansi_codes: vec![],
            pending_dim_ansi_codes: vec![],
            pending_italic_ansi_codes: vec![],
            pending_underline_ansi_codes: vec![],
            pending_blink_slow_ansi_codes: vec![],
            pending_blink_fast_ansi_codes: vec![],
            pending_reverse_ansi_codes: vec![],
            pending_hidden_ansi_codes: vec![],
            pending_strike_ansi_codes: vec![],
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
        if self.scroll_region.is_some() {
            // TODO: still do this for lines outside the scroll region?
            return;
        }

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
            // pad lines at the bottom
            let mut empty_line = vec![];
            for _ in 0..self.display_cols {
                empty_line.push(&EMPTY_TERMINAL_CHARACTER);
            }
            for _ in output.len()..self.display_rows as usize {
                output.push_back(Vec::from(empty_line.clone()));
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
        let total_rows = self.newline_indices.len() + self.linebreak_indices.len();
        let index_of_last_row = if total_rows < self.display_rows as usize {
            total_rows
        } else {
            self.display_rows as usize - 1
        };
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
    fn canonical_line_position_of(&self, index_in_line: usize) -> usize {
        // the canonical line position, 0 being the first line in the buffer, 1 the second, etc.
        let position_from_end = self.newline_indices.iter().rev().position(|n_i| *n_i <= index_in_line).unwrap_or(self.newline_indices.len());
        self.newline_indices.len() - position_from_end
    }
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

        let total_rows = self.newline_indices.len() + self.linebreak_indices.len();
        let row_count_on_screen = if total_rows < self.display_rows as usize {
            total_rows
        } else {
            self.display_rows as usize
        };

        if lines_from_end > row_count_on_screen {
            None
        } else {
            Some(row_count_on_screen - lines_from_end)
        }
    }
    fn add_newline (&mut self) {
        let nearest_line_end = self.index_of_end_of_canonical_line(self.cursor_position);
        let current_line_index_on_screen = self.get_line_index_on_screen(self.cursor_position);
        if nearest_line_end == self.characters.len() {
            self.newline_indices.push(nearest_line_end);
            self.cursor_position = nearest_line_end;
        } else if let Some(scroll_region) = self.scroll_region {
            if current_line_index_on_screen == Some(scroll_region.1 - 1) { // end of scroll region
                let mut grid = Grid::new(&self.characters, &self.newline_indices, self.display_cols as usize, self.display_rows as usize);
                grid.delete_lines(scroll_region.0 as usize - 1, 1); // -1 because scroll_region is indexed at 1
                grid.add_empty_lines(scroll_region.1 as usize - 1, 1); // -1 because scroll_region is indexed at 1
                let (characters, newline_indices) = grid.serialize();
                self.newline_indices = newline_indices;
                self.characters = characters;
                self.reflow_lines();
            } else {
                self.cursor_position = nearest_line_end;
            }
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

fn debug_log_to_file (message: String, pid: RawFd) {
    if pid == 3 {
        use std::fs::OpenOptions;
        use std::io::prelude::*;
        let mut file = OpenOptions::new().append(true).create(true).open("/tmp/mosaic-log.txt").unwrap();
        file.write_all(message.as_bytes()).unwrap();
        file.write_all("\n".as_bytes()).unwrap();
    }
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
            .reset_bold_ansi_code(&self.reset_bold_ansi_code)
            .reset_bold_dim_ansi_code(&self.reset_bold_dim_ansi_code)
            .reset_italic_ansi_code(&self.reset_italic_ansi_code)
            .reset_underline_ansi_code(&self.reset_underline_ansi_code)
            .reset_blink_ansi_code(&self.reset_blink_ansi_code)
            .reset_reverse_ansi_code(&self.reset_reverse_ansi_code)
            .reset_hidden_ansi_code(&self.reset_hidden_ansi_code)
            .reset_strike_ansi_code(&self.reset_strike_ansi_code)
            .reset_misc_ansi_code(&self.reset_misc_ansi_code)
            .foreground_ansi_codes(&self.pending_foreground_ansi_codes)
            .background_ansi_codes(&self.pending_background_ansi_codes)
            .bold_ansi_codes(&self.pending_bold_ansi_codes)
            .dim_ansi_codes(&self.pending_dim_ansi_codes)
            .italic_ansi_codes(&self.pending_italic_ansi_codes)
            .underline_ansi_codes(&self.pending_underline_ansi_codes)
            .blink_slow_ansi_codes(&self.pending_blink_slow_ansi_codes)
            .blink_fast_ansi_codes(&self.pending_blink_fast_ansi_codes)
            .reverse_ansi_codes(&self.pending_reverse_ansi_codes)
            .hidden_ansi_codes(&self.pending_hidden_ansi_codes)
            .strike_ansi_codes(&self.pending_strike_ansi_codes)
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
            if difference_from_last_newline == self.display_cols as usize && self.scroll_region.is_none() {
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
                self.reset_strike_ansi_code = true;
                self.reset_hidden_ansi_code = true;
                self.reset_reverse_ansi_code = true;
                self.reset_blink_ansi_code = true;
                self.reset_underline_ansi_code = true;
                self.reset_italic_ansi_code = true;
                self.reset_bold_ansi_code = true;
                self.reset_bold_dim_ansi_code = true;
                self.pending_foreground_ansi_codes.clear();
                self.pending_background_ansi_codes.clear();
                self.pending_bold_ansi_codes.clear();
                self.pending_bold_ansi_codes.clear();
                self.pending_italic_ansi_codes.clear();
                self.pending_underline_ansi_codes.clear();
                self.pending_blink_fast_ansi_codes.clear();
                self.pending_blink_slow_ansi_codes.clear();
                self.pending_reverse_ansi_codes.clear();
                self.pending_hidden_ansi_codes.clear();
                self.pending_strike_ansi_codes.clear();
                self.pending_misc_ansi_codes.clear();
            } else if params[0] == 39 {
                self.reset_foreground_ansi_code = true;
                self.pending_foreground_ansi_codes.clear();
            } else if params[0] == 49 {
                self.reset_background_ansi_code = true;
                self.pending_background_ansi_codes.clear();
            } else if params[0] == 21 {
                // reset bold
                self.reset_bold_ansi_code = true;
                self.pending_bold_ansi_codes.clear();
            } else if params[0] == 22 {
                // reset bold and dim
                self.reset_bold_dim_ansi_code = true;
                self.pending_bold_ansi_codes.clear();
            } else if params[0] == 23 {
                // reset italic
                self.reset_italic_ansi_code = true;
                self.pending_italic_ansi_codes.clear();
            } else if params[0] == 24 {
                // reset underline
                self.reset_underline_ansi_code = true;
                self.pending_underline_ansi_codes.clear();
            } else if params[0] == 25 {
                // reset blink
                self.reset_blink_ansi_code = true;
                self.pending_blink_fast_ansi_codes.clear();
                self.pending_blink_slow_ansi_codes.clear();
            } else if params[0] == 27 {
                // reset reverse
                self.reset_reverse_ansi_code = true;
                self.pending_reverse_ansi_codes.clear();
            } else if params[0] == 28 {
                // reset hidden
                self.reset_hidden_ansi_code = true;
                self.pending_hidden_ansi_codes.clear();
            } else if params[0] == 29 {
                // reset strike
                self.reset_strike_ansi_code = true;
                self.pending_strike_ansi_codes.clear();
            } else if params[0] == 38 {
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_foreground_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_foreground_ansi_code = false;
            } else if params[0] == 48 {
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_background_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_background_ansi_code = false;
            } else if params[0] == 1 {
                // bold
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_bold_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_bold_ansi_code = false;
            } else if params[0] == 2 {
                // dim
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_dim_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_bold_dim_ansi_code = false;
            } else if params[0] == 3 {
                // italic
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_italic_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_italic_ansi_code = false;
            } else if params[0] == 4 {
                // underline
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_underline_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_underline_ansi_code = false;
            } else if params[0] == 5 {
                // blink slow
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_blink_slow_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_blink_ansi_code = false;
            } else if params[0] == 6 {
                // blink fast
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_blink_fast_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_blink_ansi_code = false;
            } else if params[0] == 7 {
                // reverse
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_reverse_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_reverse_ansi_code = false;
            } else if params[0] == 8 {
                // hidden
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_hidden_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_hidden_ansi_code = false;
            } else if params[0] == 9 {
                // strike
                let param_string = params.iter().map(|p| p.to_string()).collect::<Vec<String>>().join(";");
                self.pending_strike_ansi_codes.push(format!("\u{1b}[{}m", param_string));
                self.reset_strike_ansi_code = false;
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
                    if newline_index <= &self.cursor_position {
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
            if params.len() > 1 {
                self.scroll_region = Some((params[0] as usize, params[1] as usize));
                self.reflow_lines(); // TODO: this clears the linebreaks, what about stuff outside the scroll region?
            } else {
                self.scroll_region = None;
            }
        } else if c == 't' {
            // TBD - title?
        } else if c == 'n' {
            // TBD - device status report
        } else if c == 'c' {
            // TBD - identify terminal
        } else if c == 'M' {
            // delete lines if currently inside scroll region
            if let Some(scroll_region) = self.scroll_region {
                let line_count_to_delete = if params[0] == 0 { 1 } else { params[0] as usize };
                let mut grid = Grid::new(&self.characters, &self.newline_indices, self.display_cols as usize, self.display_rows as usize);
                let position_of_current_line = self.canonical_line_position_of(self.cursor_position);
                grid.add_empty_lines(scroll_region.1, line_count_to_delete);
                grid.delete_lines(position_of_current_line, line_count_to_delete);
                let (characters, newline_indices) = grid.serialize();
                self.characters = characters;
                self.newline_indices = newline_indices;
                self.reflow_lines();
            }
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            if let Some(scroll_region) = self.scroll_region {
                let line_count_to_add = if params[0] == 0 { 1 } else { params[0] as usize };
                let mut grid = Grid::new(&self.characters, &self.newline_indices, self.display_cols as usize, self.display_rows as usize);
                let position_of_current_line = self.canonical_line_position_of(self.cursor_position);
                grid.add_empty_lines(position_of_current_line, line_count_to_add as usize);
                grid.delete_lines(scroll_region.1, line_count_to_add);
                let (characters, newline_indices) = grid.serialize();
                self.characters = characters;
                self.newline_indices = newline_indices;
                self.reflow_lines();
            }
        } else {
            println!("unhandled csi: {:?}->{:?}", c, params);
            panic!("aaa!!!");
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        // TBD
    }
}
