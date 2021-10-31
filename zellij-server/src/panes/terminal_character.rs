use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Index, IndexMut};

use zellij_utils::vte::ParamsIter;

use crate::panes::alacritty_functions::parse_sgr_color;

pub const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter {
    character: ' ',
    width: 1,
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
    },
    link_anchor: None,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnsiCode {
    On,
    Reset,
    NamedColor(NamedColor),
    RgbCode((u8, u8, u8)),
    ColorIndex(u8),
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
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl NamedColor {
    fn to_foreground_ansi_code(self) -> String {
        match self {
            NamedColor::Black => format!("{}", 30),
            NamedColor::Red => format!("{}", 31),
            NamedColor::Green => format!("{}", 32),
            NamedColor::Yellow => format!("{}", 33),
            NamedColor::Blue => format!("{}", 34),
            NamedColor::Magenta => format!("{}", 35),
            NamedColor::Cyan => format!("{}", 36),
            NamedColor::White => format!("{}", 37),
            NamedColor::BrightBlack => format!("{}", 90),
            NamedColor::BrightRed => format!("{}", 91),
            NamedColor::BrightGreen => format!("{}", 92),
            NamedColor::BrightYellow => format!("{}", 93),
            NamedColor::BrightBlue => format!("{}", 94),
            NamedColor::BrightMagenta => format!("{}", 95),
            NamedColor::BrightCyan => format!("{}", 96),
            NamedColor::BrightWhite => format!("{}", 97),
        }
    }
    fn to_background_ansi_code(self) -> String {
        match self {
            NamedColor::Black => format!("{}", 40),
            NamedColor::Red => format!("{}", 41),
            NamedColor::Green => format!("{}", 42),
            NamedColor::Yellow => format!("{}", 43),
            NamedColor::Blue => format!("{}", 44),
            NamedColor::Magenta => format!("{}", 45),
            NamedColor::Cyan => format!("{}", 46),
            NamedColor::White => format!("{}", 47),
            NamedColor::BrightBlack => format!("{}", 100),
            NamedColor::BrightRed => format!("{}", 101),
            NamedColor::BrightGreen => format!("{}", 102),
            NamedColor::BrightYellow => format!("{}", 103),
            NamedColor::BrightBlue => format!("{}", 104),
            NamedColor::BrightMagenta => format!("{}", 105),
            NamedColor::BrightCyan => format!("{}", 106),
            NamedColor::BrightWhite => format!("{}", 107),
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

impl Default for CharacterStyles {
    fn default() -> Self {
        Self {
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
}

impl CharacterStyles {
    pub fn new() -> Self {
        Self::default()
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
    pub fn update_and_return_diff(
        &mut self,
        new_styles: &CharacterStyles,
        changed_colors: Option<[Option<AnsiCode>; 256]>,
    ) -> Option<CharacterStyles> {
        let mut diff: Option<CharacterStyles> = None;

        if new_styles.foreground == Some(AnsiCode::Reset)
            && new_styles.background == Some(AnsiCode::Reset)
            && new_styles.strike == Some(AnsiCode::Reset)
            && new_styles.hidden == Some(AnsiCode::Reset)
            && new_styles.reverse == Some(AnsiCode::Reset)
            && new_styles.fast_blink == Some(AnsiCode::Reset)
            && new_styles.slow_blink == Some(AnsiCode::Reset)
            && new_styles.underline == Some(AnsiCode::Reset)
            && new_styles.bold == Some(AnsiCode::Reset)
            && new_styles.dim == Some(AnsiCode::Reset)
            && new_styles.italic == Some(AnsiCode::Reset)
        {
            self.foreground = Some(AnsiCode::Reset);
            self.background = Some(AnsiCode::Reset);
            self.strike = Some(AnsiCode::Reset);
            self.hidden = Some(AnsiCode::Reset);
            self.reverse = Some(AnsiCode::Reset);
            self.fast_blink = Some(AnsiCode::Reset);
            self.slow_blink = Some(AnsiCode::Reset);
            self.underline = Some(AnsiCode::Reset);
            self.bold = Some(AnsiCode::Reset);
            self.dim = Some(AnsiCode::Reset);
            self.italic = Some(AnsiCode::Reset);
            return Some(*new_styles);
        };

        if self.foreground != new_styles.foreground {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.foreground(new_styles.foreground));
            } else {
                diff = Some(CharacterStyles::new().foreground(new_styles.foreground));
            }
            self.foreground = new_styles.foreground;
        }
        if self.background != new_styles.background {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.background(new_styles.background));
            } else {
                diff = Some(CharacterStyles::new().background(new_styles.background));
            }
            self.background = new_styles.background;
        }
        if self.strike != new_styles.strike {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.strike(new_styles.strike));
            } else {
                diff = Some(CharacterStyles::new().strike(new_styles.strike));
            }
            self.strike = new_styles.strike;
        }
        if self.hidden != new_styles.hidden {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.hidden(new_styles.hidden));
            } else {
                diff = Some(CharacterStyles::new().hidden(new_styles.hidden));
            }
            self.hidden = new_styles.hidden;
        }
        if self.reverse != new_styles.reverse {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.reverse(new_styles.reverse));
            } else {
                diff = Some(CharacterStyles::new().reverse(new_styles.reverse));
            }
            self.reverse = new_styles.reverse;
        }
        if self.slow_blink != new_styles.slow_blink {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.blink_slow(new_styles.slow_blink));
            } else {
                diff = Some(CharacterStyles::new().blink_slow(new_styles.slow_blink));
            }
            self.slow_blink = new_styles.slow_blink;
        }
        if self.fast_blink != new_styles.fast_blink {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.blink_fast(new_styles.fast_blink));
            } else {
                diff = Some(CharacterStyles::new().blink_fast(new_styles.fast_blink));
            }
            self.fast_blink = new_styles.fast_blink;
        }
        if self.underline != new_styles.underline {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.underline(new_styles.underline));
            } else {
                diff = Some(CharacterStyles::new().underline(new_styles.underline));
            }
            self.underline = new_styles.underline;
        }
        if self.bold != new_styles.bold {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.bold(new_styles.bold));
            } else {
                diff = Some(CharacterStyles::new().bold(new_styles.bold));
            }
            self.bold = new_styles.bold;
        }
        if self.dim != new_styles.dim {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.dim(new_styles.dim));
            } else {
                diff = Some(CharacterStyles::new().dim(new_styles.dim));
            }
            self.dim = new_styles.dim;
        }
        if self.italic != new_styles.italic {
            if let Some(new_diff) = diff.as_mut() {
                diff = Some(new_diff.italic(new_styles.italic));
            } else {
                diff = Some(CharacterStyles::new().italic(new_styles.italic));
            }
            self.italic = new_styles.italic;
        }

        if let Some(changed_colors) = changed_colors {
            if let Some(AnsiCode::ColorIndex(color_index)) = diff.and_then(|diff| diff.foreground) {
                if let Some(changed_color) = changed_colors[color_index as usize] {
                    diff.as_mut().unwrap().foreground = Some(changed_color);
                }
            }
            if let Some(AnsiCode::ColorIndex(color_index)) = diff.and_then(|diff| diff.background) {
                if let Some(changed_color) = changed_colors[color_index as usize] {
                    diff.as_mut().unwrap().background = Some(changed_color);
                }
            }
        }
        diff
    }
    pub fn reset_all(&mut self) {
        self.foreground = Some(AnsiCode::Reset);
        self.background = Some(AnsiCode::Reset);
        self.bold = Some(AnsiCode::Reset);
        self.dim = Some(AnsiCode::Reset);
        self.italic = Some(AnsiCode::Reset);
        self.underline = Some(AnsiCode::Reset);
        self.slow_blink = Some(AnsiCode::Reset);
        self.fast_blink = Some(AnsiCode::Reset);
        self.reverse = Some(AnsiCode::Reset);
        self.hidden = Some(AnsiCode::Reset);
        self.strike = Some(AnsiCode::Reset);
    }
    pub fn add_style_from_ansi_params(&mut self, params: &mut ParamsIter) {
        while let Some(param) = params.next() {
            match param {
                [] | [0] => self.reset_all(),
                [1] => *self = self.bold(Some(AnsiCode::On)),
                [2] => *self = self.dim(Some(AnsiCode::On)),
                [3] => *self = self.italic(Some(AnsiCode::On)),
                [4] => *self = self.underline(Some(AnsiCode::On)),
                [5] => *self = self.blink_slow(Some(AnsiCode::On)),
                [6] => *self = self.blink_fast(Some(AnsiCode::On)),
                [7] => *self = self.reverse(Some(AnsiCode::On)),
                [8] => *self = self.hidden(Some(AnsiCode::On)),
                [9] => *self = self.strike(Some(AnsiCode::On)),
                [21] => *self = self.bold(Some(AnsiCode::Reset)),
                [22] => {
                    *self = self.bold(Some(AnsiCode::Reset));
                    *self = self.dim(Some(AnsiCode::Reset));
                }
                [23] => *self = self.italic(Some(AnsiCode::Reset)),
                [24] => *self = self.underline(Some(AnsiCode::Reset)),
                [25] => {
                    *self = self.blink_slow(Some(AnsiCode::Reset));
                    *self = self.blink_fast(Some(AnsiCode::Reset));
                }
                [27] => *self = self.reverse(Some(AnsiCode::Reset)),
                [28] => *self = self.hidden(Some(AnsiCode::Reset)),
                [29] => *self = self.strike(Some(AnsiCode::Reset)),
                [30] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Black))),
                [31] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Red))),
                [32] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Green))),
                [33] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Yellow))),
                [34] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Blue))),
                [35] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Magenta))),
                [36] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::Cyan))),
                [37] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::White))),
                [38] => {
                    let mut iter = params.map(|param| param[0]);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.foreground(Some(ansi_code));
                    }
                }
                [38, params @ ..] => {
                    let rgb_start = if params.len() > 4 { 2 } else { 1 };
                    let rgb_iter = params[rgb_start..].iter().copied();
                    let mut iter = std::iter::once(params[0]).chain(rgb_iter);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.foreground(Some(ansi_code));
                    }
                }
                [39] => *self = self.foreground(Some(AnsiCode::Reset)),
                [40] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Black))),
                [41] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Red))),
                [42] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Green))),
                [43] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Yellow))),
                [44] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Blue))),
                [45] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Magenta))),
                [46] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::Cyan))),
                [47] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::White))),
                [48] => {
                    let mut iter = params.map(|param| param[0]);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.background(Some(ansi_code));
                    }
                }
                [48, params @ ..] => {
                    let rgb_start = if params.len() > 4 { 2 } else { 1 };
                    let rgb_iter = params[rgb_start..].iter().copied();
                    let mut iter = std::iter::once(params[0]).chain(rgb_iter);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.background(Some(ansi_code));
                    }
                }
                [49] => *self = self.background(Some(AnsiCode::Reset)),
                [90] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightBlack)))
                }
                [91] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightRed))),
                [92] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightGreen)))
                }
                [93] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightYellow)))
                }
                [94] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightBlue))),
                [95] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightMagenta)))
                }
                [96] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightCyan))),
                [97] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightWhite)))
                }
                [100] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightBlack)))
                }
                [101] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightRed))),
                [102] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightGreen)))
                }
                [103] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightYellow)))
                }
                [104] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightBlue)))
                }
                [105] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightMagenta)))
                }
                [106] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightCyan)))
                }
                [107] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightWhite)))
                }
                _ => {
                    log::warn!("unhandled csi m code {:?}", param);
                    return;
                }
            }
        }
    }
}

impl Display for CharacterStyles {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.foreground == Some(AnsiCode::Reset)
            && self.background == Some(AnsiCode::Reset)
            && self.strike == Some(AnsiCode::Reset)
            && self.hidden == Some(AnsiCode::Reset)
            && self.reverse == Some(AnsiCode::Reset)
            && self.fast_blink == Some(AnsiCode::Reset)
            && self.slow_blink == Some(AnsiCode::Reset)
            && self.underline == Some(AnsiCode::Reset)
            && self.bold == Some(AnsiCode::Reset)
            && self.dim == Some(AnsiCode::Reset)
            && self.italic == Some(AnsiCode::Reset)
        {
            write!(f, "\u{1b}[m")?; // reset all
            return Ok(());
        }
        if let Some(ansi_code) = self.foreground {
            match ansi_code {
                AnsiCode::RgbCode((r, g, b)) => {
                    write!(f, "\u{1b}[38;2;{};{};{}m", r, g, b)?;
                }
                AnsiCode::ColorIndex(color_index) => {
                    write!(f, "\u{1b}[38;5;{}m", color_index)?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[39m")?;
                }
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_foreground_ansi_code())?;
                }
                _ => {}
            }
        };
        if let Some(ansi_code) = self.background {
            match ansi_code {
                AnsiCode::RgbCode((r, g, b)) => {
                    write!(f, "\u{1b}[48;2;{};{};{}m", r, g, b)?;
                }
                AnsiCode::ColorIndex(color_index) => {
                    write!(f, "\u{1b}[48;5;{}m", color_index)?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[49m")?;
                }
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_background_ansi_code())?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.strike {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[9m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[29m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.hidden {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[8m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[28m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.reverse {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[7m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[27m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.fast_blink {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[6m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.slow_blink {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[5m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.bold {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[1m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[22m\u{1b}[24m")?;
                    // TODO: this cancels bold + underline, if this behaviour is indeed correct, we
                    // need to properly handle it in the struct methods etc like dim
                }
                _ => {}
            }
        }
        // notice the order is important here, bold must be before underline
        // because the bold reset also resets underline, and would override it
        // otherwise
        if let Some(ansi_code) = self.underline {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[4m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[24m")?;
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.dim {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[2m")?;
                }
                AnsiCode::Reset => {
                    if let Some(AnsiCode::Reset) = self.bold {
                        // we only reset dim if both dim and bold should be reset
                        write!(f, "\u{1b}[22m")?;
                    }
                }
                _ => {}
            }
        }
        if let Some(ansi_code) = self.italic {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[3m")?;
                }
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[23m")?;
                }
                _ => {}
            }
        };
        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum LinkAnchor {
    Start(u16),
    End,
}

#[derive(Clone, Copy, Debug)]
pub enum CharsetIndex {
    G0,
    G1,
    G2,
    G3,
}

impl Default for CharsetIndex {
    fn default() -> Self {
        CharsetIndex::G0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StandardCharset {
    Ascii,
    SpecialCharacterAndLineDrawing,
}

impl Default for StandardCharset {
    fn default() -> Self {
        StandardCharset::Ascii
    }
}

impl StandardCharset {
    /// Switch/Map character to the active charset. Ascii is the common case and
    /// for that we want to do as little as possible.
    #[inline]
    pub fn map(self, c: char) -> char {
        match self {
            StandardCharset::Ascii => c,
            StandardCharset::SpecialCharacterAndLineDrawing => match c {
                '`' => '◆',
                'a' => '▒',
                'b' => '␉',
                'c' => '␌',
                'd' => '␍',
                'e' => '␊',
                'f' => '°',
                'g' => '±',
                'h' => '␤',
                'i' => '␋',
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
        }
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
pub struct Charsets([StandardCharset; 4]);

impl Index<CharsetIndex> for Charsets {
    type Output = StandardCharset;

    fn index(&self, index: CharsetIndex) -> &StandardCharset {
        &self.0[index as usize]
    }
}

impl IndexMut<CharsetIndex> for Charsets {
    fn index_mut(&mut self, index: CharsetIndex) -> &mut StandardCharset {
        &mut self.0[index as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub enum CursorShape {
    Initial,
    Block,
    BlinkingBlock,
    Underline,
    BlinkingUnderline,
    Beam,
    BlinkingBeam,
}

#[derive(Clone, Debug)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    pub is_hidden: bool,
    pub pending_styles: CharacterStyles,
    pub charsets: Charsets,
    shape: CursorShape,
}

impl Cursor {
    pub fn new(x: usize, y: usize) -> Self {
        Cursor {
            x,
            y,
            is_hidden: false,
            pending_styles: CharacterStyles::new(),
            charsets: Default::default(),
            shape: CursorShape::Initial,
        }
    }
    pub fn change_shape(&mut self, shape: CursorShape) {
        self.shape = shape;
    }
    pub fn get_shape(&self) -> CursorShape {
        self.shape
    }
}

#[derive(Clone, Copy)]
pub struct TerminalCharacter {
    pub character: char,
    pub styles: CharacterStyles,
    pub width: usize,
    pub link_anchor: Option<LinkAnchor>,
}

impl ::std::fmt::Debug for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.character)
    }
}
