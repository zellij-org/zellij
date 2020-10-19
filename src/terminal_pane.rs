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
    styles: CharacterStyles {
        foreground: Some(AnsiCode::Reset),
        background: Some(AnsiCode::Reset),
        strike: Some(AnsiCode::Reset),
        hidden: Some(AnsiCode::Reset),
        reverse: Some(AnsiCode::Reset),
        slow_blink: Some(AnsiCode::Reset),
        fast_blink: Some(AnsiCode::Reset),
        underline: Some(AnsiCode::Reset),
        bold: Some(AnsiCode::Reset),
        dim: Some(AnsiCode::Reset),
        italic: Some(AnsiCode::Reset),
    }
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnsiCode {
    Reset,
    NamedColor(NamedColor),
    Code((Option<u16>, Option<u16>))
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum NamedColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl NamedColor {
    fn to_foreground_ansi_code(&self) -> String {
        match self {
            NamedColor::Black => format!("{}", 30),
            NamedColor::Red => format!("{}", 31),
            NamedColor::Green => format!("{}", 32),
            NamedColor::Yellow => format!("{}", 33),
            NamedColor::Blue => format!("{}", 34),
            NamedColor::Magenta => format!("{}", 35),
            NamedColor::Cyan => format!("{}", 36),
            NamedColor::White => format!("{}", 37),
        }
    }
    fn to_background_ansi_code(&self) -> String {
        match self {
            NamedColor::Black => format!("{}", 40),
            NamedColor::Red => format!("{}", 41),
            NamedColor::Green => format!("{}", 42),
            NamedColor::Yellow => format!("{}", 43),
            NamedColor::Blue => format!("{}", 44),
            NamedColor::Magenta => format!("{}", 45),
            NamedColor::Cyan => format!("{}", 46),
            NamedColor::White => format!("{}", 47),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharacterStyles {
    pub foreground: Option<AnsiCode>,
    pub background: Option<AnsiCode>,
    pub strike: Option<AnsiCode>,
    pub hidden: Option<AnsiCode>,
    pub reverse: Option<AnsiCode>,
    pub slow_blink: Option<AnsiCode>,
    pub fast_blink: Option<AnsiCode>,
    pub underline: Option<AnsiCode>,
    pub bold: Option<AnsiCode>,
    pub dim: Option<AnsiCode>,
    pub italic: Option<AnsiCode>,
}

impl CharacterStyles {
    pub fn new() -> Self {
        CharacterStyles {
            foreground: None,
            background: None,
            strike: None,
            hidden: None,
            reverse: None,
            slow_blink: None,
            fast_blink: None,
            underline: None,
            bold: None,
            dim: None,
            italic: None,
        }
    }
    pub fn foreground(mut self, foreground_code: Option<AnsiCode>) -> Self {
        self.foreground = foreground_code;
        self
    }
    pub fn background(mut self, background_code: Option<AnsiCode>) -> Self {
        self.background = background_code;
        self
    }
    pub fn bold(mut self, bold_code: Option<AnsiCode>) -> Self {
        self.bold = bold_code;
        self
    }
    pub fn dim(mut self, dim_code: Option<AnsiCode>) -> Self {
        self.dim = dim_code;
        self
    }
    pub fn italic(mut self, italic_code: Option<AnsiCode>) -> Self {
        self.italic = italic_code;
        self
    }
    pub fn underline(mut self, underline_code: Option<AnsiCode>) -> Self {
        self.underline = underline_code;
        self
    }
    pub fn blink_slow(mut self, slow_blink_code: Option<AnsiCode>) -> Self {
        self.slow_blink = slow_blink_code;
        self
    }
    pub fn blink_fast(mut self, fast_blink_code: Option<AnsiCode>) -> Self {
        self.fast_blink = fast_blink_code;
        self
    }
    pub fn reverse(mut self, reverse_code: Option<AnsiCode>) -> Self {
        self.reverse = reverse_code;
        self
    }
    pub fn hidden(mut self, hidden_code: Option<AnsiCode>) -> Self {
        self.hidden = hidden_code;
        self
    }
    pub fn strike(mut self, strike_code: Option<AnsiCode>) -> Self {
        self.strike = strike_code;
        self
    }
    pub fn clear(&mut self) {
        self.foreground = None;
        self.background = None;
        self.strike = None;
        self.hidden = None;
        self.reverse = None;
        self.slow_blink = None;
        self.fast_blink = None;
        self.underline = None;
        self.bold = None;
        self.dim = None;
        self.italic = None;
    }
    pub fn update_and_return_diff(&mut self, new_styles: &CharacterStyles) -> Option<CharacterStyles> {
        let mut diff: Option<CharacterStyles> = None;
        if self.foreground != new_styles.foreground {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.foreground(new_styles.foreground));
                self.foreground = new_styles.foreground;
            } else {
                diff = Some(CharacterStyles::new().foreground(new_styles.foreground));
                self.foreground = new_styles.foreground;
            }
        }
        if self.background != new_styles.background {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.background(new_styles.background));
                self.background = new_styles.background;
            } else {
                diff = Some(CharacterStyles::new().background(new_styles.background));
                self.background = new_styles.background;
            }
        }
        if self.strike != new_styles.strike {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.strike(new_styles.strike));
                self.strike = new_styles.strike;
            } else {
                diff = Some(CharacterStyles::new().strike(new_styles.strike));
                self.strike = new_styles.strike;
            }
        }
        if self.hidden != new_styles.hidden {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.hidden(new_styles.hidden));
                self.hidden = new_styles.hidden;
            } else {
                diff = Some(CharacterStyles::new().hidden(new_styles.hidden));
                self.hidden = new_styles.hidden;
            }
        }
        if self.reverse != new_styles.reverse {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.reverse(new_styles.reverse));
                self.reverse= new_styles.reverse;
            } else {
                diff = Some(CharacterStyles::new().reverse(new_styles.reverse));
                self.reverse= new_styles.reverse;
            }
        }
        if self.slow_blink != new_styles.slow_blink {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.blink_slow(new_styles.slow_blink));
                self.slow_blink = new_styles.slow_blink;
            } else {
                diff = Some(CharacterStyles::new().blink_slow(new_styles.slow_blink));
                self.slow_blink = new_styles.slow_blink;
            }
        }
        if self.fast_blink != new_styles.fast_blink {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.blink_fast(new_styles.fast_blink));
                self.fast_blink = new_styles.fast_blink;
            } else {
                diff = Some(CharacterStyles::new().blink_fast(new_styles.fast_blink));
                self.fast_blink = new_styles.fast_blink;
            }
        }
        if self.underline != new_styles.underline {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.underline(new_styles.underline));
                self.underline= new_styles.underline;
            } else {
                diff = Some(CharacterStyles::new().underline(new_styles.underline));
                self.underline= new_styles.underline;
            }
        }
        if self.bold != new_styles.bold {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.bold(new_styles.bold));
                self.bold= new_styles.bold;
            } else {
                diff = Some(CharacterStyles::new().bold(new_styles.bold));
                self.bold= new_styles.bold;
            }
        }
        if self.dim != new_styles.dim {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.dim(new_styles.dim));
                self.dim= new_styles.dim;
            } else {
                diff = Some(CharacterStyles::new().dim(new_styles.dim));
                self.dim= new_styles.dim;
            }
        }
        if self.italic != new_styles.italic {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.italic(new_styles.italic));
                self.italic = new_styles.italic;
            } else {
                diff = Some(CharacterStyles::new().italic(new_styles.italic));
                self.italic = new_styles.italic;
            }
        }
        diff
    }
}

impl Display for CharacterStyles {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.foreground == Some(AnsiCode::Reset) &&
            self.background == Some(AnsiCode::Reset) &&
            self.strike == Some(AnsiCode::Reset) &&
            self.hidden == Some(AnsiCode::Reset) &&
            self.reverse == Some(AnsiCode::Reset) &&
            self.fast_blink == Some(AnsiCode::Reset) &&
            self.slow_blink == Some(AnsiCode::Reset) &&
            self.underline == Some(AnsiCode::Reset) &&
            self.bold == Some(AnsiCode::Reset) &&
            self.dim == Some(AnsiCode::Reset) &&
            self.italic == Some(AnsiCode::Reset) {

            write!(f, "\u{1b}[m")?; // reset all
            return Ok(());
        }
        if let Some(ansi_code) = self.foreground {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[38;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[38;{}m", param1)?;
                        },
                        (_, _) => {
                            // TODO: can this happen?
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[39m")?;
                },
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_foreground_ansi_code())?;
                }
            }
        };
        if let Some(ansi_code) = self.background {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[48;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[48;{}m", param1)?;
                        },
                        (_, _) => {
                            // TODO: can this happen?
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[49m")?;
                }
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_background_ansi_code())?;
                }
            }
        }
        if let Some(ansi_code) = self.strike {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[9;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[9;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[9m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[29m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.hidden {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[8;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[8;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[8m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[28m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.reverse {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[7;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[7;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[7m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[27m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.fast_blink {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[6;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[6;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[6m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.slow_blink {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[5;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[5;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[5m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.bold {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[1;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[1;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[1m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[22m\u{1b}[24m")?;
                    // character_ansi_codes.push_str(&format!("\u{1b}[22m"));
                    // TODO: this cancels bold + underline, if this behaviour is indeed correct, we
                    // need to properly handle it in the struct methods etc like dim
                },
                _ => {}
            }
        }
        // notice the order is important here, bold must be before underline
        // because the bold reset also resets underline, and would override it
        // otherwise
        if let Some(ansi_code) = self.underline {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[4;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[4;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[4m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[24m")?;
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.dim {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[2;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[2;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[2m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    if let Some(bold) = self.bold {
                        // we only reset dim if both dim and bold should be reset
                        match bold {
                            AnsiCode::Reset => {
                                write!(f, "\u{1b}[22m")?;
                            },
                            _ => {}
                        }
                    }
                },
                _ => {}
            }
        }
        if let Some(ansi_code) = self.italic {
            match ansi_code {
                AnsiCode::Code((param1, param2)) => {
                    match (param1, param2) {
                        (Some(param1), Some(param2)) => {
                            write!(f, "\u{1b}[3;{};{}m", param1, param2)?;
                        },
                        (Some(param1), None) => {
                            write!(f, "\u{1b}[3;{}m", param1)?;
                        },
                        (_, _) => {
                            write!(f, "\u{1b}[3m")?;
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[23m")?;
                },
                _ => {}
            }
        };
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub struct TerminalCharacter {
    pub character: char,
    pub styles: CharacterStyles,
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
                characters.push(*character);
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
    pub scroll_up_count: Option<usize>,
    pub x_coords: u16,
    pub y_coords: u16,
    cursor_position: usize,
    newline_indices: Vec<usize>, // canonical line breaks we get from the vt interpreter
    linebreak_indices: Vec<usize>, // linebreaks from line wrapping
    scroll_region: Option<(usize, usize)>, // top line index / bottom line index
    pending_foreground_code: Option<AnsiCode>,
    pending_background_code: Option<AnsiCode>,
    pending_bold_code: Option<AnsiCode>,
    pending_dim_code: Option<AnsiCode>,
    pending_italic_code: Option<AnsiCode>,
    pending_underline_code: Option<AnsiCode>,
    pending_slow_blink_code: Option<AnsiCode>,
    pending_fast_blink_code: Option<AnsiCode>,
    pending_reverse_code: Option<AnsiCode>,
    pending_hidden_code: Option<AnsiCode>,
    pending_strike_code: Option<AnsiCode>,
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
        let characters = Vec::with_capacity(100000);
        TerminalOutput {
            pid,
            characters,
            cursor_position: 0,
            newline_indices: Vec::new(),
            linebreak_indices: Vec::new(),
            scroll_region: None,
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: true,
            scroll_up_count: None,
            pending_foreground_code: None,
            pending_background_code: None,
            pending_bold_code: None,
            pending_dim_code: None,
            pending_italic_code: None,
            pending_underline_code: None,
            pending_slow_blink_code: None,
            pending_fast_blink_code: None,
            pending_reverse_code: None,
            pending_hidden_code: None,
            pending_strike_code: None,
            x_coords,
            y_coords,
        }
    }
    pub fn handle_event(&mut self, event: VteEvent) {
        // self.should_render = true; // TODO: more accurately
        match event {
            VteEvent::Print(c) => {
                self.print(c);
                self.should_render = true; // TODO: more accurately
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
            let mut character_styles = CharacterStyles::new();
            for (row, line) in buffer_lines.iter().enumerate() {
                vte_output = format!("{}\u{1b}[{};{}H\u{1b}[m", vte_output, self.y_coords as usize + row + 1, self.x_coords + 1); // goto row/col
                for (col, t_character) in line.iter().enumerate() {
                    if (col as u16) < *display_cols {
                        // in some cases (eg. while resizing) some characters will spill over
                        // before they are corrected by the shell (for the prompt) or by reflowing
                        // lines
                        if let Some(new_styles) = character_styles.update_and_return_diff(&t_character.styles) {
                            vte_output = format!("{}{}", vte_output, new_styles);
                        }
                        vte_output.push(t_character.character);
                    }
                }
                character_styles.clear();
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

        let mut scroll_up_count = self.scroll_up_count.unwrap_or(0);

        loop {
            if let Some(newline_index) = next_newline_index {
                if *newline_index == i {
                    // pad line
                    for _ in current_line.len()..self.display_cols as usize {
                        current_line.push_back(&EMPTY_TERMINAL_CHARACTER);
                    }
                    if scroll_up_count > 0 {
                        scroll_up_count -= 1;
                        current_line.clear();
                    } else {
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
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
                        if scroll_up_count > 0 {
                            scroll_up_count -= 1;
                            current_line.clear();
                        } else {
                            output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                        }
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
                    if scroll_up_count > 0 {
                        scroll_up_count -= 1;
                        current_line.clear();
                    } else {
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
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
                    if scroll_up_count > 0 {
                        scroll_up_count -= 1;
                        current_line.clear();
                    } else {
                        output.push_front(Vec::from(current_line.drain(..).collect::<Vec<&TerminalCharacter>>()));
                    }
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
                if scroll_up_count > 0 {
                    scroll_up_count -= 1;
                    empty_line.clear(); // TODO: better
                } else {
                    output.push_back(Vec::from(empty_line.clone()));
                }
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
    pub fn scroll_up(&mut self, count: usize) {
        if let Some(scroll_up_count) = self.scroll_up_count.as_mut() {
            *scroll_up_count += count;
        } else {
            self.scroll_up_count = Some(count);
        }
        // TODO: do not render if we're at the top line
        self.should_render = true;
    }
    pub fn scroll_down(&mut self, count: usize) {
        if let Some(scroll_up_count) = self.scroll_up_count.as_mut() {
            if *scroll_up_count > count {
                *scroll_up_count -= count;
                self.should_render = true;
            } else {
                self.scroll_up_count = None;
                self.should_render = true;
            }
        }
    }
    pub fn clear_scroll(&mut self) {
        if self.scroll_up_count.is_some() {
            self.should_render = true;
        }
        self.scroll_up_count = None;
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
        self.reset_all_ansi_codes();

        self.should_render = true;
    }
    fn move_to_beginning_of_line (&mut self) {
        let last_newline_index = self.index_of_beginning_of_line(self.cursor_position);
        self.cursor_position = last_newline_index;
        self.should_render = true;
    }
    fn reset_all_ansi_codes(&mut self) {
        self.pending_foreground_code = None;
        self.pending_background_code = None;
        self.pending_bold_code = None;
        self.pending_dim_code = None;
        self.pending_italic_code = None;
        self.pending_underline_code = None;
        self.pending_slow_blink_code = None;
        self.pending_fast_blink_code = None;
        self.pending_reverse_code = None;
        self.pending_hidden_code = None;
        self.pending_strike_code = None;
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
        // apparently, building TerminalCharacter like this without a "new" method
        // is a little faster
        let terminal_character = TerminalCharacter {
            character: c,
            styles: CharacterStyles {
                foreground: self.pending_foreground_code,
                background: self.pending_background_code,
                bold: self.pending_bold_code,
                dim: self.pending_dim_code,
                italic: self.pending_italic_code,
                underline: self.pending_underline_code,
                slow_blink: self.pending_slow_blink_code,
                fast_blink: self.pending_fast_blink_code,
                reverse: self.pending_reverse_code,
                hidden: self.pending_hidden_code,
                strike: self.pending_strike_code,
            }
        };

        let length_of_characters = self.characters.len();
        let current_character_capacity = self.characters.capacity();

        if current_character_capacity <= self.characters.len() {
            self.characters.reserve(current_character_capacity);
        }

        if length_of_characters > self.cursor_position {
            // this is a little hacky but significantly more performant
            // than removing self.cursor_position and then inserting terminal_character
            self.characters.push(terminal_character);
            self.characters.swap_remove(self.cursor_position);
            if !self.newline_indices.contains(&(self.cursor_position + 1)) {
                // advancing the cursor beyond the borders of the line has to be done explicitly
                self.cursor_position += 1;
            }
        } else {
            for _ in length_of_characters..self.cursor_position {
                self.characters.push(EMPTY_TERMINAL_CHARACTER);
            };
            self.characters.push(terminal_character);
            let start_of_last_line = max(self.newline_indices.last(), self.linebreak_indices.last()).unwrap_or(&0);
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
            if params.is_empty() || params[0] == 0 {
                // reset all
                self.pending_foreground_code = Some(AnsiCode::Reset);
                self.pending_background_code = Some(AnsiCode::Reset);
                self.pending_bold_code = Some(AnsiCode::Reset);
                self.pending_dim_code = Some(AnsiCode::Reset);
                self.pending_italic_code = Some(AnsiCode::Reset);
                self.pending_underline_code = Some(AnsiCode::Reset);
                self.pending_slow_blink_code = Some(AnsiCode::Reset);
                self.pending_fast_blink_code = Some(AnsiCode::Reset);
                self.pending_reverse_code = Some(AnsiCode::Reset);
                self.pending_hidden_code = Some(AnsiCode::Reset);
                self.pending_strike_code = Some(AnsiCode::Reset);
            } else if params[0] == 39 {
                self.pending_foreground_code = Some(AnsiCode::Reset);
            } else if params[0] == 49 {
                self.pending_background_code = Some(AnsiCode::Reset);
            } else if params[0] == 21 {
                // reset bold
                self.pending_bold_code = Some(AnsiCode::Reset);
            } else if params[0] == 22 {
                // reset bold and dim
                self.pending_bold_code = Some(AnsiCode::Reset);
                self.pending_dim_code = Some(AnsiCode::Reset);
            } else if params[0] == 23 {
                // reset italic
                self.pending_italic_code = Some(AnsiCode::Reset);
            } else if params[0] == 24 {
                // reset underline
                self.pending_underline_code = Some(AnsiCode::Reset);
            } else if params[0] == 25 {
                // reset blink
                self.pending_slow_blink_code = Some(AnsiCode::Reset);
                self.pending_fast_blink_code = Some(AnsiCode::Reset);
            } else if params[0] == 27 {
                // reset reverse
                self.pending_reverse_code = Some(AnsiCode::Reset);
            } else if params[0] == 28 {
                // reset hidden
                self.pending_hidden_code = Some(AnsiCode::Reset);
            } else if params[0] == 29 {
                // reset strike
                self.pending_strike_code = Some(AnsiCode::Reset);
            } else if params[0] == 38 {
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_foreground_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_foreground_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_foreground_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 48 {
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_background_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_background_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_background_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 1 {
                // bold
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_bold_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_bold_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_bold_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 2 {
                // dim
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_dim_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_dim_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_dim_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 3 {
                // italic
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_italic_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_italic_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_italic_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 4 {
                // underline
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_underline_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_underline_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_underline_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 5 {
                // blink slow
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_slow_blink_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_slow_blink_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_slow_blink_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 6 {
                // blink fast
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_fast_blink_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_fast_blink_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_fast_blink_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 7 {
                // reverse
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_reverse_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_reverse_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_reverse_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 8 {
                // hidden
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_hidden_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_hidden_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_hidden_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 9 {
                // strike
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_strike_code = Some(AnsiCode::Code((Some(*param1 as u16), Some(*param2 as u16))));
                    },
                    (Some(param1), None) => {
                        self.pending_strike_code = Some(AnsiCode::Code((Some(*param1 as u16), None)));
                    }
                    (_, _) => {
                        self.pending_strike_code = Some(AnsiCode::Code((None, None)));
                    }
                };
            } else if params[0] == 30 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Black));
            } else if params[0] == 31 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Red));
            } else if params[0] == 32 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Green));
            } else if params[0] == 33 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Yellow));
            } else if params[0] == 34 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Blue));
            } else if params[0] == 35 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Magenta));
            } else if params[0] == 36 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::Cyan));
            } else if params[0] == 37 {
                self.pending_foreground_code = Some(AnsiCode::NamedColor(NamedColor::White));
            } else if params[0] == 40 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Black));
            } else if params[0] == 41 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Red));
            } else if params[0] == 42 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Green));
            } else if params[0] == 43 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Yellow));
            } else if params[0] == 44 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Blue));
            } else if params[0] == 45 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Magenta));
            } else if params[0] == 46 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::Cyan));
            } else if params[0] == 47 {
                self.pending_background_code = Some(AnsiCode::NamedColor(NamedColor::White));
            } else {
                debug_log_to_file(format!("unhandled csi m code {:?}", params), self.pid);
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
