use ::std::fmt::{self, Display, Debug, Formatter};
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

#[derive(Clone)]
pub struct CanonicalLine {
    pub wrapped_fragments: Vec<WrappedFragment>
}

impl CanonicalLine {
    pub fn new() -> Self {
        CanonicalLine {
            wrapped_fragments: vec![WrappedFragment::new()],
        }
    }
    pub fn add_new_wrap(&mut self, terminal_character: TerminalCharacter) {
        let mut new_fragment = WrappedFragment::new();
        new_fragment.add_character(terminal_character, 0);
        self.wrapped_fragments.push(new_fragment);
    }
    pub fn change_width(&mut self, new_width: usize) {
        let mut characters: Vec<TerminalCharacter> = self.wrapped_fragments
            .iter()
            .fold(Vec::with_capacity(self.wrapped_fragments.len()), |mut characters, wrapped_fragment| {
                characters.push(wrapped_fragment.characters.iter().copied());
                characters
            })
            .into_iter()
            .flatten()
            .collect();
        let mut wrapped_fragments = Vec::with_capacity(characters.len() / new_width);

        while characters.len() > 0 {
            if characters.len() > new_width {
                wrapped_fragments.push(WrappedFragment::from_vec(characters.drain(..new_width).collect()));
            } else {
                wrapped_fragments.push(WrappedFragment::from_vec(characters.drain(..).collect()));
            }
        }
        self.wrapped_fragments = wrapped_fragments;
    }
    pub fn clear_after(&mut self, fragment_index: usize, column_index: usize) {
        let fragment_to_clear = self.wrapped_fragments.get_mut(fragment_index).expect("fragment out of bounds");
        fragment_to_clear.clear_after_and_including(column_index);
        self.wrapped_fragments.truncate(fragment_index + 1);
    }
}

impl Debug for CanonicalLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for wrapped_fragment in &self.wrapped_fragments {
            writeln!(f, "{:?}", wrapped_fragment)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct WrappedFragment {
    pub characters: Vec<TerminalCharacter>
}

impl WrappedFragment {
    pub fn new() -> Self {
        WrappedFragment {
            characters: vec![]
        }
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter, position_in_line: usize) {
        if position_in_line == self.characters.len() {
            self.characters.push(terminal_character);
        } else {
            // this is much more performant than remove/insert
            self.characters.push(terminal_character);
            self.characters.swap_remove(position_in_line);
        }
    }
    pub fn from_vec(characters: Vec<TerminalCharacter>) -> Self {
        WrappedFragment {
            characters
        }
    }
    pub fn clear_after_and_including(&mut self, character_index: usize) {
        self.characters.truncate(character_index);
    }
}

impl Debug for WrappedFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for character in &self.characters {
            write!(f, "{:?}", character)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CursorPosition {
    line_index: (usize, usize), // (canonical line index, fragment index in line)
    column_index: usize // 0 is the first character from the pane edge 
}

impl CursorPosition {
    pub fn new() -> Self {
        CursorPosition {
            line_index: (0, 0),
            column_index: 0
        }
    }
    pub fn move_forward (&mut self, count: usize) {
        // TODO: panic if out of bounds?
        self.column_index += count;
    }
    pub fn move_backwards(&mut self, count: usize) {
        self.column_index -= count;
    }
    pub fn move_to_next_linewrap(&mut self) {
        self.line_index.1 += 1;
    }
    pub fn move_to_next_canonical_line(&mut self) {
        self.line_index.0 += 1;
    }
    pub fn move_to_beginning_of_linewrap (&mut self) {
        self.column_index = 0;
    }
    pub fn move_to_beginning_of_canonical_line(&mut self) {
        self.column_index = 0;
        self.line_index.1 = 0;
    }
    pub fn move_up_by_canonical_lines(&mut self, count: usize) {
        let current_canonical_line_position = self.line_index.0;
        if count > current_canonical_line_position {
            self.line_index = (0, 0);
        } else {
            self.line_index = (current_canonical_line_position - count, 0);
        }
    }
    pub fn move_to_canonical_line(&mut self, index: usize) {
        self.line_index = (index, 0);
    }
    pub fn move_to_column(&mut self, col: usize) {
        self.column_index = col;
    }
    pub fn reset(&mut self) {
        self.column_index = 0;
        self.line_index = (0, 0);
    }
}

pub struct Scroll {
    pub canonical_lines: Vec<CanonicalLine>, // TODO: unpubify
    cursor_position: CursorPosition,
    total_columns: usize,
    lines_in_view: usize,
    viewport_bottom_offset: Option<usize>,
    scroll_region: Option<(usize, usize)> // start line, end line (if set, this is the area the will scroll)
}

impl Scroll {
    pub fn new (total_columns: usize, lines_in_view: usize) -> Self {
        let mut canonical_lines = vec![];
        canonical_lines.push(CanonicalLine::new());
        let cursor_position = CursorPosition::new();
        Scroll {
            canonical_lines: vec![CanonicalLine::new()], // The rest will be created by newlines explicitly
            total_columns,
            lines_in_view,
            cursor_position, 
            viewport_bottom_offset: None,
            scroll_region: None,
        }
    }
    pub fn as_character_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        let mut lines: VecDeque<Vec<TerminalCharacter>> = VecDeque::new(); // TODO: with capacity lines_from_end?
        let mut canonical_lines = self.canonical_lines.iter().rev();
        let mut lines_to_skip = self.viewport_bottom_offset.unwrap_or(0);
        'gather_lines: loop {
            match canonical_lines.next() {
                Some(current_canonical_line) => {
                    for wrapped_fragment in current_canonical_line.wrapped_fragments.iter().rev() {
                        let mut line: Vec<TerminalCharacter> = wrapped_fragment.characters.iter().copied().collect();
                        if lines_to_skip > 0 {
                            lines_to_skip -= 1;
                        } else {
                            for _ in line.len()..self.total_columns {
                                // pad line if needed
                                line.push(EMPTY_TERMINAL_CHARACTER);
                            }
                            lines.push_front(line);
                        }
                        if lines.len() == self.lines_in_view {
                            break 'gather_lines;
                        }
                    }
                },
                None => break, // no more lines
            }
        }
        if lines.len() < self.lines_in_view {
            // pad lines in case we don't have enough scrollback to fill the view
            let mut empty_line = vec![];
            for _ in 0..self.total_columns {
                empty_line.push(EMPTY_TERMINAL_CHARACTER);
            }
            for _ in lines.len()..self.lines_in_view {
                // pad lines in case we didn't have enough
                lines.push_back(empty_line.clone());
            }
        }
        Vec::from(lines)
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
        let (canonical_line_position, wrapped_fragment_index_in_line) = self.cursor_position.line_index;
        let cursor_position_in_line = self.cursor_position.column_index;
        let current_line = self.canonical_lines.get_mut(canonical_line_position).expect("cursor out of bounds");
        let current_wrapped_fragment = current_line.wrapped_fragments.get_mut(wrapped_fragment_index_in_line).expect("cursor out of bounds");

        if cursor_position_in_line <= self.total_columns {
            current_wrapped_fragment.add_character(terminal_character, cursor_position_in_line);
            self.cursor_position.move_forward(1);
        } else {
            current_line.add_new_wrap(terminal_character);
            self.cursor_position.move_to_next_linewrap();
            self.cursor_position.move_to_beginning_of_linewrap();
        }
    }
    pub fn add_canonical_line(&mut self) {
        let current_canonical_line_index = self.cursor_position.line_index.0;
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            // the scroll region indices start at 1, so we need to adjust them
            let scroll_region_top_index = scroll_region_top - 1;
            let scroll_region_bottom_index = scroll_region_bottom - 1;
            if current_canonical_line_index == scroll_region_bottom_index { // end of scroll region
                // when we have a scroll region set and we're at its bottom
                // we need to delete its first line, thus shifting all lines in it upwards
                // then we add an empty line at its end which will be filled by the application
                // controlling the scroll region (presumably filled by whatever comes next in the
                // scroll buffer, but that's not something we control)
                self.canonical_lines.remove(scroll_region_top_index);
                self.canonical_lines.insert(scroll_region_bottom_index, CanonicalLine::new());
                return;
            }
        }
        if current_canonical_line_index == self.canonical_lines.len() - 1 {
            let new_canonical_line = CanonicalLine::new();
            self.canonical_lines.push(new_canonical_line);
            self.cursor_position.move_to_next_canonical_line();
            self.cursor_position.move_to_beginning_of_canonical_line();
        } else if current_canonical_line_index < self.canonical_lines.len() - 1 {
            self.cursor_position.move_to_next_canonical_line();
            self.cursor_position.move_to_beginning_of_canonical_line();
        } else {
            panic!("cursor out of bounds, cannot add_canonical_line");
        }
    }
    pub fn cursor_coordinates_on_screen(&self) -> (usize, usize) { // (x, y)
        let (canonical_line_cursor_position, line_wrap_cursor_position) = self.cursor_position.line_index;
        let x = self.cursor_position.column_index;
        let mut y = 0;
        let mut indices_and_canonical_lines = self.canonical_lines.iter().enumerate().rev();
        loop {
            match indices_and_canonical_lines.next() {
                Some((current_index, current_line)) => {
                    if current_index == canonical_line_cursor_position {
                        y += current_line.wrapped_fragments.len() - line_wrap_cursor_position;
                        break;
                    } else {
                        y += current_line.wrapped_fragments.len();
                    }

                },
                None => break,
            }
        }
        let total_lines = self.canonical_lines.iter().fold(0, |total_lines, current_line| total_lines + current_line.wrapped_fragments.len()); // TODO: is this performant enough? should it be cached or kept track of?
        let y = if total_lines < self.lines_in_view {
            total_lines - y
        } else {
            self.lines_in_view - y
        };
        (x, y)
    }
    pub fn move_cursor_forward(&mut self, count: usize) {
        let (current_canonical_line_index, current_line_wrap_position) = self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self.canonical_lines.get_mut(current_canonical_line_index).expect("cursor out of bounds");
        let current_fragment = current_canonical_line.wrapped_fragments.get_mut(current_line_wrap_position).expect("cursor out of bounds");
        let move_count = if current_cursor_column_position + count > self.total_columns {
            // move to last column in the current line wrap
            self.total_columns - current_cursor_column_position
        } else {
            count
        };
        for _ in current_fragment.characters.len()..current_cursor_column_position + move_count {
            current_fragment.characters.push(EMPTY_TERMINAL_CHARACTER);
        }
        self.cursor_position.move_forward(move_count);
    }
    pub fn move_cursor_back(&mut self, count: usize) {
        let current_cursor_column_position = self.cursor_position.column_index;
        if current_cursor_column_position < count {
            self.cursor_position.move_to_beginning_of_linewrap();
        } else {
            self.cursor_position.move_backwards(count);
        }
    }
    pub fn move_cursor_to_beginning_of_canonical_line(&mut self) {
        self.cursor_position.move_to_beginning_of_canonical_line();
    }
    pub fn move_cursor_backwards(&mut self, count: usize) {
        self.cursor_position.move_backwards(count);
    }
    pub fn move_cursor_up(&mut self, count: usize) {
        self.cursor_position.move_up_by_canonical_lines(count);
    }
    pub fn change_size(&mut self, columns: usize, lines: usize) {
        if self.scroll_region.is_none() {
            for canonical_line in self.canonical_lines.iter_mut() {
                canonical_line.change_width(columns);
            }
            let cursor_line = self.canonical_lines.get(self.cursor_position.line_index.0).expect("cursor out of bounds");
            if cursor_line.wrapped_fragments.len() < self.cursor_position.line_index.1 {
                self.cursor_position.line_index.1 = cursor_line.wrapped_fragments.len();
            }
        }
        self.lines_in_view = lines;
        self.total_columns = columns;
    }
    pub fn clear_canonical_line_right_of_cursor(&mut self) {
        let (current_canonical_line_index, current_line_wrap_position) = self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self.canonical_lines.get_mut(current_canonical_line_index).expect("cursor out of bounds");
        current_canonical_line.clear_after(current_line_wrap_position, current_cursor_column_position);
    }
    pub fn clear_all_after_cursor(&mut self) {
        let (current_canonical_line_index, current_line_wrap_position) = self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self.canonical_lines.get_mut(current_canonical_line_index).expect("cursor out of bounds");
        current_canonical_line.clear_after(current_line_wrap_position, current_cursor_column_position);
        self.canonical_lines.truncate(current_canonical_line_index + 1);
    }
    pub fn clear_all(&mut self) {
        self.canonical_lines.clear();
        self.canonical_lines.push(CanonicalLine::new());
        self.cursor_position.reset();
    }
    pub fn move_cursor_to(&mut self, line: usize, col: usize) {
        if self.canonical_lines.len() > line {
            self.cursor_position.move_to_canonical_line(line);
        } else {
            for _ in self.canonical_lines.len()..=line {
                self.canonical_lines.push(CanonicalLine::new());
            }
            self.cursor_position.move_to_canonical_line(line);
        }
        let (current_canonical_line_index, current_line_wrap_position) = self.cursor_position.line_index;
        let current_canonical_line = self.canonical_lines.get_mut(current_canonical_line_index).expect("cursor out of bounds");
        let current_fragment = current_canonical_line.wrapped_fragments.get_mut(current_line_wrap_position).expect("cursor out of bounds");
        for _ in current_fragment.characters.len()..col {
            current_fragment.characters.push(EMPTY_TERMINAL_CHARACTER);
        }
        self.cursor_position.move_to_column(col);
    }
    pub fn set_scroll_region(&mut self, top_line: usize, bottom_line: usize) {
        self.scroll_region = Some((top_line, bottom_line));
        // TODO: clear linewraps in scroll region?
    }
    pub fn clear_scroll_region(&mut self) {
        self.scroll_region = None;
    }
    pub fn delete_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            // the scroll region indices start at 1, so we need to adjust them
            let scroll_region_top_index = scroll_region_top - 1;
            let scroll_region_bottom_index = scroll_region_bottom - 1;
            let current_canonical_line_index = self.cursor_position.line_index.0;
            if current_canonical_line_index >= scroll_region_top_index &&
                current_canonical_line_index <= scroll_region_bottom_index {
                // when deleting lines inside the scroll region, we must make sure it stays the
                // same size (and that other lines below it aren't shifted inside it)
                // so we delete the current line(s) and add an empty line at the end of the scroll
                // region
                for _ in 0..count {
                    self.canonical_lines.remove(current_canonical_line_index);
                    self.canonical_lines.insert(scroll_region_bottom_index, CanonicalLine::new());
                }
            }
        }
    }
    pub fn add_empty_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            // the scroll region indices start at 1, so we need to adjust them
            let scroll_region_top_index = scroll_region_top - 1;
            let scroll_region_bottom_index = scroll_region_bottom - 1;
            let current_canonical_line_index = self.cursor_position.line_index.0;
            if current_canonical_line_index >= scroll_region_top_index &&
                current_canonical_line_index <= scroll_region_bottom_index {
                // when adding empty lines inside the scroll region, we must make sure it stays the
                // same size and that lines don't "leak" outside of it
                // so we add an empty line where the cursor currently is, and delete the last line
                // of the scroll region
                for _ in 0..count {
                    self.canonical_lines.remove(scroll_region_bottom_index);
                    self.canonical_lines.insert(current_canonical_line_index, CanonicalLine::new());
                }
            }
        }
    }
    pub fn move_viewport_up (&mut self, count: usize) {
        if let Some(current_offset) = self.viewport_bottom_offset.as_mut() {
            *current_offset += count;
        } else {
            self.viewport_bottom_offset = Some(count);
        }

    }
    pub fn move_viewport_down (&mut self, count: usize) {
        if let Some(current_offset) = self.viewport_bottom_offset.as_mut() {
            if *current_offset > count {
                *current_offset -= count;
            } else {
                self.viewport_bottom_offset = None;
            }
        }
    }
    pub fn reset_viewport (&mut self) {
        self.viewport_bottom_offset = None;
    }
}

impl Debug for Scroll {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for line in &self.canonical_lines {
            writeln!(f, "{:?}", line)?;
        }
        Ok(())
    }
}

pub struct TerminalOutput {
    pub pid: RawFd,
    pub scroll: Scroll,
    pub display_rows: u16,
    pub display_cols: u16,
    pub should_render: bool,
    pub x_coords: u16,
    pub y_coords: u16,
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
        let scroll = Scroll::new(ws.ws_col as usize, ws.ws_row as usize);
        TerminalOutput {
            pid,
            scroll,
            display_rows: ws.ws_row,
            display_cols: ws.ws_col,
            should_render: true,
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
        match event {
            VteEvent::Print(c) => {
                self.print(c);
                self.should_render = true;
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
        self.scroll.change_size(self.display_cols as usize, self.display_rows as usize);
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
                            // the terminal keeps the previous styles as long as we're in the same
                            // line, so we only want to update the new styles here (this also
                            // includes resetting previous styles as needed)
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
    pub fn read_buffer_as_lines (&self) -> Vec<Vec<TerminalCharacter>> {
        self.scroll.as_character_lines()
    }
    pub fn cursor_coordinates (&self) -> (usize, usize) { // (x, y)
        self.scroll.cursor_coordinates_on_screen()
    }
    pub fn scroll_up(&mut self, count: usize) {
        self.scroll.move_viewport_up(count);
        self.should_render = true;
    }
    pub fn scroll_down(&mut self, count: usize) {
        self.scroll.move_viewport_down(count);
        self.should_render = true;
    }
    pub fn clear_scroll(&mut self) {
        self.scroll.reset_viewport();
        self.should_render = true;
    }
    fn add_newline (&mut self) {
        self.scroll.add_canonical_line(); // TODO: handle scroll region
        self.reset_all_ansi_codes();
        self.should_render = true;
    }
    fn move_to_beginning_of_line (&mut self) {
        self.scroll.move_cursor_to_beginning_of_canonical_line();
    }
    fn move_cursor_backwards(&mut self, count: usize) {
        self.scroll.move_cursor_backwards(count);
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
    if pid == 0 {
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
        self.scroll.add_character(terminal_character);
    }

    fn execute(&mut self, byte: u8) {
        if byte == 13 { // 0d, carriage return
            self.move_to_beginning_of_line();
        } else if byte == 08 { // backspace
            self.move_cursor_backwards(1);
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
            self.scroll.move_cursor_forward(move_by);
        } else if c == 'K' { // clear line (0 => right, 1 => left, 2 => all)
            if params[0] == 0 {
                self.scroll.clear_canonical_line_right_of_cursor();
            }
            // TODO: implement 1 and 2
        } else if c == 'J' { // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
            if params[0] == 0 {
                self.scroll.clear_all_after_cursor();
            } else if params[0] == 2 {
                self.scroll.clear_all();
            }
            // TODO: implement 1
        } else if c == 'H' { // goto row/col
            let (row, col) = if params.len() == 1 {
                (params[0] as usize, 0) // TODO: is this always correct ?
            } else {
                (params[0] as usize - 1, params[1] as usize - 1) // we subtract 1 here because this csi is 1 indexed and we index from 0
            };
            self.scroll.move_cursor_to(row, col);
        } else if c == 'A' { // move cursor up until edge of screen
            let move_up_count = if params[0] == 0 { 1 } else { params[0] };
            self.scroll.move_cursor_up(move_up_count as usize);
        } else if c == 'D' {
            let move_back_count = if params[0] == 0 { 1 } else { params[0] as usize };
            self.scroll.move_cursor_back(move_back_count);
        } else if c == 'l' {
            // TBD
        } else if c == 'h' {
            // TBD
        } else if c == 'r' {
            if params.len() > 1 {
                let top_line_index = params[0] as usize;
                let bottom_line_index = params[1] as usize;
                self.scroll.set_scroll_region(top_line_index, bottom_line_index);
            } else {
                self.scroll.clear_scroll_region();
            }
        } else if c == 't' {
            // TBD - title?
        } else if c == 'n' {
            // TBD - device status report
        } else if c == 'c' {
            // TBD - identify terminal
        } else if c == 'M' {
            // delete lines if currently inside scroll region
            let line_count_to_delete = if params[0] == 0 { 1 } else { params[0] as usize };
            self.scroll.delete_lines_in_scroll_region(line_count_to_delete);
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            let line_count_to_add = if params[0] == 0 { 1 } else { params[0] as usize };
            self.scroll.add_empty_lines_in_scroll_region(line_count_to_add);
        } else if c == 'q' || c == 'd' || c == 'X' || c == 'G' {
            // ignore for now to run on mac
        } else {
            panic!("unhandled csi: {:?}->{:?}", c, params);
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        // TBD
    }
}
