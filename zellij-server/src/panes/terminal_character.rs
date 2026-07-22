use std::convert::From;
use std::fmt::{self, Debug, Display, Formatter};
use std::ops::{Index, IndexMut};
use std::rc::Rc;
use unicode_width::UnicodeWidthChar;

use unicode_width::UnicodeWidthStr;
use vte::ParamsIter;
use zellij_utils::data::StyleDeclaration;
use zellij_utils::data::{PaletteColor, Style};
use zellij_utils::input::command::RunCommand;

use crate::output::CharacterChunk;
use crate::panes::alacritty_functions::parse_sgr_color;

pub const EMPTY_TERMINAL_CHARACTER: TerminalCharacter = TerminalCharacter {
    character: ' ',
    width: 1,
    styles: RcCharacterStyles::Reset,
};

pub const RESET_STYLES: CharacterStyles = CharacterStyles {
    foreground: Some(AnsiCode::Reset),
    background: Some(AnsiCode::Reset),
    underline_color: Some(AnsiCode::Reset),
    strike: Some(AnsiCode::Reset),
    hidden: Some(AnsiCode::Reset),
    reverse: Some(AnsiCode::Reset),
    slow_blink: Some(AnsiCode::Reset),
    fast_blink: Some(AnsiCode::Reset),
    underline: Some(AnsiCode::Reset),
    bold: Some(AnsiCode::Reset),
    dim: Some(AnsiCode::Reset),
    italic: Some(AnsiCode::Reset),
    link_anchor: Some(LinkAnchor::End),
    styled_underlines_enabled: false,
};

// Prefer to use RcCharacterStyles::default() where it makes sense
// as it will reduce memory usage
pub const DEFAULT_STYLES: CharacterStyles = CharacterStyles {
    foreground: None,
    background: None,
    underline_color: None,
    strike: None,
    hidden: None,
    reverse: None,
    slow_blink: None,
    fast_blink: None,
    underline: None,
    bold: None,
    dim: None,
    italic: None,
    link_anchor: None,
    styled_underlines_enabled: false,
};

thread_local! {
    static RC_DEFAULT_STYLES: RcCharacterStyles =
        RcCharacterStyles::Rc(Rc::new(DEFAULT_STYLES));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnsiCode {
    On,
    Reset,
    NamedColor(NamedColor),
    RgbCode((u8, u8, u8)),
    ColorIndex(u8),
    Underline(Option<AnsiStyledUnderline>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnsiStyledUnderline {
    Double,
    Undercurl,
    Underdotted,
    Underdashed,
}

impl From<PaletteColor> for AnsiCode {
    fn from(palette_color: PaletteColor) -> Self {
        match palette_color {
            PaletteColor::Rgb((r, g, b)) => AnsiCode::RgbCode((r, g, b)),
            PaletteColor::EightBit(index) => AnsiCode::ColorIndex(index),
        }
    }
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
        let v = match self {
            NamedColor::Black => 30,
            NamedColor::Red => 31,
            NamedColor::Green => 32,
            NamedColor::Yellow => 33,
            NamedColor::Blue => 34,
            NamedColor::Magenta => 35,
            NamedColor::Cyan => 36,
            NamedColor::White => 37,
            NamedColor::BrightBlack => 90,
            NamedColor::BrightRed => 91,
            NamedColor::BrightGreen => 92,
            NamedColor::BrightYellow => 93,
            NamedColor::BrightBlue => 94,
            NamedColor::BrightMagenta => 95,
            NamedColor::BrightCyan => 96,
            NamedColor::BrightWhite => 97,
        };

        v.to_string()
    }
    fn to_background_ansi_code(self) -> String {
        let v = match self {
            NamedColor::Black => 40,
            NamedColor::Red => 41,
            NamedColor::Green => 42,
            NamedColor::Yellow => 43,
            NamedColor::Blue => 44,
            NamedColor::Magenta => 45,
            NamedColor::Cyan => 46,
            NamedColor::White => 47,
            NamedColor::BrightBlack => 100,
            NamedColor::BrightRed => 101,
            NamedColor::BrightGreen => 102,
            NamedColor::BrightYellow => 103,
            NamedColor::BrightBlue => 104,
            NamedColor::BrightMagenta => 105,
            NamedColor::BrightCyan => 106,
            NamedColor::BrightWhite => 107,
        };

        v.to_string()
    }
}

// This enum carefully only has two variants so
// enum niche optimisations can keep it at the same byte size as pointers
#[derive(Clone, Debug, PartialEq)]
pub enum RcCharacterStyles {
    Reset,
    Rc(Rc<CharacterStyles>),
}

#[cfg(target_arch = "x86_64")]
const _: [(); 8] = [(); std::mem::size_of::<RcCharacterStyles>()];

impl From<CharacterStyles> for RcCharacterStyles {
    fn from(styles: CharacterStyles) -> Self {
        if styles == RESET_STYLES {
            RcCharacterStyles::Reset
        } else {
            RcCharacterStyles::Rc(Rc::new(styles))
        }
    }
}

impl Default for RcCharacterStyles {
    fn default() -> Self {
        RC_DEFAULT_STYLES.with(|s| s.clone())
    }
}

impl std::ops::Deref for RcCharacterStyles {
    type Target = CharacterStyles;

    fn deref(&self) -> &Self::Target {
        match self {
            RcCharacterStyles::Reset => &RESET_STYLES,
            RcCharacterStyles::Rc(styles) => &*styles,
        }
    }
}

impl Display for RcCharacterStyles {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let styles: &CharacterStyles = &*self;
        Display::fmt(&styles, f)
    }
}

impl RcCharacterStyles {
    pub fn reset() -> Self {
        Self::Reset
    }

    pub fn update(&mut self, f: impl FnOnce(&mut CharacterStyles)) {
        let mut styles: CharacterStyles = **self;
        f(&mut styles);
        *self = styles.into();
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CharacterStyles {
    pub foreground: Option<AnsiCode>,
    pub background: Option<AnsiCode>,
    pub underline_color: Option<AnsiCode>,
    pub strike: Option<AnsiCode>,
    pub hidden: Option<AnsiCode>,
    pub reverse: Option<AnsiCode>,
    pub slow_blink: Option<AnsiCode>,
    pub fast_blink: Option<AnsiCode>,
    pub underline: Option<AnsiCode>,
    pub bold: Option<AnsiCode>,
    pub dim: Option<AnsiCode>,
    pub italic: Option<AnsiCode>,
    pub link_anchor: Option<LinkAnchor>,
    pub styled_underlines_enabled: bool,
}

impl PartialEq for CharacterStyles {
    fn eq(&self, other: &Self) -> bool {
        self.foreground == other.foreground
            && self.background == other.background
            && self.underline_color == other.underline_color
            && self.strike == other.strike
            && self.hidden == other.hidden
            && self.reverse == other.reverse
            && self.slow_blink == other.slow_blink
            && self.fast_blink == other.fast_blink
            && self.underline == other.underline
            && self.bold == other.bold
            && self.dim == other.dim
            && self.italic == other.italic
            && self.link_anchor == other.link_anchor
    }
}

impl CharacterStyles {
    pub fn foreground(mut self, foreground_code: Option<AnsiCode>) -> Self {
        self.foreground = foreground_code;
        self
    }
    pub fn background(mut self, background_code: Option<AnsiCode>) -> Self {
        self.background = background_code;
        self
    }
    pub fn underline_color(mut self, underline_color_code: Option<AnsiCode>) -> Self {
        self.underline_color = underline_color_code;
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
    pub fn link_anchor(mut self, link_anchor: Option<LinkAnchor>) -> Self {
        self.link_anchor = link_anchor;
        self
    }
    pub fn enable_styled_underlines(mut self, enabled: bool) -> Self {
        self.styled_underlines_enabled = enabled;
        self
    }
    pub fn clear(&mut self) {
        self.foreground = None;
        self.background = None;
        self.underline_color = None;
        self.strike = None;
        self.hidden = None;
        self.reverse = None;
        self.slow_blink = None;
        self.fast_blink = None;
        self.underline = None;
        self.bold = None;
        self.dim = None;
        self.italic = None;
        self.link_anchor = None;
    }
    pub fn update_and_return_diff(
        &mut self,
        new_styles: &CharacterStyles,
        changed_colors: Option<[Option<AnsiCode>; 256]>,
    ) -> Option<CharacterStyles> {
        if self == new_styles && changed_colors.is_none() {
            return None;
        }

        if *new_styles == RESET_STYLES {
            *self = RESET_STYLES.enable_styled_underlines(self.styled_underlines_enabled);
            return Some(RESET_STYLES.enable_styled_underlines(self.styled_underlines_enabled));
        }

        // create diff from all changed styles
        let mut diff = DEFAULT_STYLES.enable_styled_underlines(self.styled_underlines_enabled);

        if self.foreground != new_styles.foreground {
            diff.foreground = new_styles.foreground;
        }
        if self.background != new_styles.background {
            diff.background = new_styles.background;
        }
        if self.underline_color != new_styles.underline_color {
            diff.underline_color = new_styles.underline_color;
        }
        if self.strike != new_styles.strike {
            diff.strike = new_styles.strike;
        }
        if self.hidden != new_styles.hidden {
            diff.hidden = new_styles.hidden;
        }
        if self.reverse != new_styles.reverse {
            diff.reverse = new_styles.reverse;
        }
        if self.slow_blink != new_styles.slow_blink {
            diff.slow_blink = new_styles.slow_blink;
        }
        if self.fast_blink != new_styles.fast_blink {
            diff.fast_blink = new_styles.fast_blink;
        }
        if self.underline != new_styles.underline {
            diff.underline = new_styles.underline;
        }
        if self.bold != new_styles.bold {
            diff.bold = new_styles.bold;
        }
        if self.dim != new_styles.dim {
            diff.dim = new_styles.dim;
        }
        if self.italic != new_styles.italic {
            diff.italic = new_styles.italic;
        }
        if self.link_anchor != new_styles.link_anchor {
            diff.link_anchor = new_styles.link_anchor;
        }

        // apply new styles
        *self = new_styles.enable_styled_underlines(self.styled_underlines_enabled);

        if let Some(changed_colors) = changed_colors {
            if let Some(AnsiCode::ColorIndex(color_index)) = diff.foreground {
                if let Some(changed_color) = changed_colors[color_index as usize] {
                    diff.foreground = Some(changed_color);
                }
            }
            if let Some(AnsiCode::ColorIndex(color_index)) = diff.background {
                if let Some(changed_color) = changed_colors[color_index as usize] {
                    diff.background = Some(changed_color);
                }
            }
        }
        Some(diff)
    }
    fn reset_ansi(&mut self) {
        self.foreground = Some(AnsiCode::Reset);
        self.background = Some(AnsiCode::Reset);
        self.underline_color = Some(AnsiCode::Reset);
        self.bold = Some(AnsiCode::Reset);
        self.dim = Some(AnsiCode::Reset);
        self.italic = Some(AnsiCode::Reset);
        self.underline = Some(AnsiCode::Reset);
        self.slow_blink = Some(AnsiCode::Reset);
        self.fast_blink = Some(AnsiCode::Reset);
        self.reverse = Some(AnsiCode::Reset);
        self.hidden = Some(AnsiCode::Reset);
        self.strike = Some(AnsiCode::Reset);
        // Deliberately don't end link anchor
    }
    pub fn add_style_from_ansi_params(&mut self, params: &mut ParamsIter) {
        while let Some(param) = params.next() {
            match param {
                [] | [0] => self.reset_ansi(),
                [1] => *self = self.bold(Some(AnsiCode::On)),
                [2] => *self = self.dim(Some(AnsiCode::On)),
                [3] => *self = self.italic(Some(AnsiCode::On)),
                [4, 0] => *self = self.underline(Some(AnsiCode::Reset)),
                [4, 1] => *self = self.underline(Some(AnsiCode::Underline(None))),
                [4, 2] => {
                    *self =
                        self.underline(Some(AnsiCode::Underline(Some(AnsiStyledUnderline::Double))))
                },
                [4, 3] => {
                    *self = self.underline(Some(AnsiCode::Underline(Some(
                        AnsiStyledUnderline::Undercurl,
                    ))))
                },
                [4, 4] => {
                    *self = self.underline(Some(AnsiCode::Underline(Some(
                        AnsiStyledUnderline::Underdotted,
                    ))))
                },
                [4, 5] => {
                    *self = self.underline(Some(AnsiCode::Underline(Some(
                        AnsiStyledUnderline::Underdashed,
                    ))))
                },
                [4] => *self = self.underline(Some(AnsiCode::Underline(None))),
                [5] => *self = self.blink_slow(Some(AnsiCode::On)),
                [6] => *self = self.blink_fast(Some(AnsiCode::On)),
                [7] => *self = self.reverse(Some(AnsiCode::On)),
                [8] => *self = self.hidden(Some(AnsiCode::On)),
                [9] => *self = self.strike(Some(AnsiCode::On)),
                [21] => *self = self.bold(Some(AnsiCode::Reset)),
                [22] => {
                    *self = self.bold(Some(AnsiCode::Reset));
                    *self = self.dim(Some(AnsiCode::Reset));
                },
                [23] => *self = self.italic(Some(AnsiCode::Reset)),
                [24] => *self = self.underline(Some(AnsiCode::Reset)),
                [25] => {
                    *self = self.blink_slow(Some(AnsiCode::Reset));
                    *self = self.blink_fast(Some(AnsiCode::Reset));
                },
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
                },
                [38, params @ ..] => {
                    let rgb_start = if params.len() > 4 { 2 } else { 1 };
                    let rgb_iter = params[rgb_start..].iter().copied();
                    let mut iter = std::iter::once(params[0]).chain(rgb_iter);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.foreground(Some(ansi_code));
                    }
                },
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
                },
                [48, params @ ..] => {
                    let rgb_start = if params.len() > 4 { 2 } else { 1 };
                    let rgb_iter = params[rgb_start..].iter().copied();
                    let mut iter = std::iter::once(params[0]).chain(rgb_iter);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.background(Some(ansi_code));
                    }
                },
                [49] => *self = self.background(Some(AnsiCode::Reset)),
                [58] => {
                    let mut iter = params.map(|param| param[0]);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.underline_color(Some(ansi_code));
                    }
                },
                [58, params @ ..] => {
                    let rgb_start = if params.len() > 4 { 2 } else { 1 };
                    let rgb_iter = params[rgb_start..].iter().copied();
                    let mut iter = std::iter::once(params[0]).chain(rgb_iter);
                    if let Some(ansi_code) = parse_sgr_color(&mut iter) {
                        *self = self.underline_color(Some(ansi_code));
                    }
                },
                [59] => *self = self.underline_color(Some(AnsiCode::Reset)),
                [90] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightBlack)))
                },
                [91] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightRed))),
                [92] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightGreen)))
                },
                [93] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightYellow)))
                },
                [94] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightBlue))),
                [95] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightMagenta)))
                },
                [96] => *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightCyan))),
                [97] => {
                    *self = self.foreground(Some(AnsiCode::NamedColor(NamedColor::BrightWhite)))
                },
                [100] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightBlack)))
                },
                [101] => *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightRed))),
                [102] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightGreen)))
                },
                [103] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightYellow)))
                },
                [104] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightBlue)))
                },
                [105] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightMagenta)))
                },
                [106] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightCyan)))
                },
                [107] => {
                    *self = self.background(Some(AnsiCode::NamedColor(NamedColor::BrightWhite)))
                },
                _ => {
                    return;
                },
            }
        }
    }
}

impl Display for CharacterStyles {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.foreground == Some(AnsiCode::Reset)
            && self.background == Some(AnsiCode::Reset)
            && self.underline_color == Some(AnsiCode::Reset)
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
                },
                AnsiCode::ColorIndex(color_index) => {
                    write!(f, "\u{1b}[38;5;{}m", color_index)?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[39m")?;
                },
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_foreground_ansi_code())?;
                },
                _ => {},
            }
        };
        if let Some(ansi_code) = self.background {
            match ansi_code {
                AnsiCode::RgbCode((r, g, b)) => {
                    write!(f, "\u{1b}[48;2;{};{};{}m", r, g, b)?;
                },
                AnsiCode::ColorIndex(color_index) => {
                    write!(f, "\u{1b}[48;5;{}m", color_index)?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[49m")?;
                },
                AnsiCode::NamedColor(named_color) => {
                    write!(f, "\u{1b}[{}m", named_color.to_background_ansi_code())?;
                },
                _ => {},
            }
        }
        if self.styled_underlines_enabled {
            if let Some(ansi_code) = self.underline_color {
                match ansi_code {
                    AnsiCode::RgbCode((r, g, b)) => {
                        write!(f, "\u{1b}[58:2::{}:{}:{}m", r, g, b)?;
                    },
                    AnsiCode::ColorIndex(color_index) => {
                        write!(f, "\u{1b}[58:5:{}m", color_index)?;
                    },
                    AnsiCode::Reset => {
                        write!(f, "\u{1b}[59m")?;
                    },
                    _ => {},
                }
            };
        }
        if let Some(ansi_code) = self.strike {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[9m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[29m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.hidden {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[8m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[28m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.reverse {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[7m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[27m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.fast_blink {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[6m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.slow_blink {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[5m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[25m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.bold {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[1m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[22m")?;
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.underline {
            match ansi_code {
                AnsiCode::Underline(None) => {
                    write!(f, "\u{1b}[4m")?;
                },
                AnsiCode::Underline(Some(styled)) => {
                    if self.styled_underlines_enabled {
                        match styled {
                            AnsiStyledUnderline::Double => {
                                write!(f, "\u{1b}[4:2m")?;
                            },
                            AnsiStyledUnderline::Undercurl => {
                                write!(f, "\u{1b}[4:3m")?;
                            },
                            AnsiStyledUnderline::Underdotted => {
                                write!(f, "\u{1b}[4:4m")?;
                            },
                            AnsiStyledUnderline::Underdashed => {
                                write!(f, "\u{1b}[4:5m")?;
                            },
                        }
                    }
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[24m")?;
                },
                _ => {},
            }
        }

        if let Some(ansi_code) = self.dim {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[2m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[22m")?;
                    // ⬑ this SGR also clears bold, so reapply it
                    if let Some(AnsiCode::On) = self.bold {
                        write!(f, "\u{1b}[1m")?;
                    }
                },
                _ => {},
            }
        }
        if let Some(ansi_code) = self.italic {
            match ansi_code {
                AnsiCode::On => {
                    write!(f, "\u{1b}[3m")?;
                },
                AnsiCode::Reset => {
                    write!(f, "\u{1b}[23m")?;
                },
                _ => {},
            }
        };
        Ok(())
    }
}

impl From<StyleDeclaration> for CharacterStyles {
    fn from(declaration: StyleDeclaration) -> Self {
        RESET_STYLES.foreground(Some(declaration.base.into()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
    UK,
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
            StandardCharset::UK => match c {
                '#' => '£',
                _ => c,
            },
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

impl CursorShape {
    pub fn get_csi_str(&self) -> &str {
        match self {
            CursorShape::Initial => "\u{1b}[0 q",
            CursorShape::Block => "\u{1b}[2 q",
            CursorShape::BlinkingBlock => "\u{1b}[1 q",
            CursorShape::Underline => "\u{1b}[4 q",
            CursorShape::BlinkingUnderline => "\u{1b}[3 q",
            CursorShape::Beam => "\u{1b}[6 q",
            CursorShape::BlinkingBeam => "\u{1b}[5 q",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
    pub pending_styles: RcCharacterStyles,
    pub charsets: Charsets,
    shape: CursorShape,
}

impl Cursor {
    pub fn new(x: usize, y: usize, styled_underlines: bool) -> Self {
        Cursor {
            x,
            y,
            pending_styles: RESET_STYLES
                .enable_styled_underlines(styled_underlines)
                .into(),
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

#[derive(Clone, PartialEq)]
pub struct TerminalCharacter {
    pub character: char,
    pub styles: RcCharacterStyles,
    width: u8,
}

// This size has significant memory and CPU implications for long lines,
// be careful about allowing it to grow
#[cfg(target_arch = "x86_64")]
const _: [(); 16] = [(); std::mem::size_of::<TerminalCharacter>()];

impl TerminalCharacter {
    #[inline]
    pub fn new(character: char) -> Self {
        Self::new_styled(character, Default::default())
    }

    #[inline]
    pub fn new_styled(character: char, styles: RcCharacterStyles) -> Self {
        TerminalCharacter {
            character,
            styles,
            width: character.width().unwrap_or(0) as u8,
        }
    }

    #[inline]
    pub fn new_singlewidth(character: char) -> Self {
        Self::new_singlewidth_styled(character, Default::default())
    }

    #[inline]
    pub fn new_singlewidth_styled(character: char, styles: RcCharacterStyles) -> Self {
        TerminalCharacter {
            character,
            styles,
            width: 1,
        }
    }

    pub fn width(&self) -> usize {
        self.width as usize
    }
}

impl ::std::fmt::Debug for TerminalCharacter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.character)
    }
}

pub fn render_first_run_banner(
    columns: usize,
    rows: usize,
    style: &Style,
    run_command: Option<&RunCommand>,
) -> String {
    let middle_row = rows / 2;
    let middle_column = columns / 2;
    match run_command {
        Some(run_command) => {
            let bold_text = RESET_STYLES.bold(Some(AnsiCode::On));
            let command_color_text = RESET_STYLES
                .foreground(Some(AnsiCode::from(
                    style.colors.text_unselected.emphasis_2,
                )))
                .bold(Some(AnsiCode::On));
            let waiting_to_run_text = "Waiting to run: ";
            let command_text = run_command.to_string();
            let waiting_to_run_text_width = waiting_to_run_text.width() + command_text.width();
            let column_start_postion = middle_column.saturating_sub(waiting_to_run_text_width / 2);
            let waiting_to_run_line = format!(
                "\u{1b}[{};{}H{}{}{}{}{}",
                middle_row,
                column_start_postion,
                bold_text,
                waiting_to_run_text,
                command_color_text,
                command_text,
                RESET_STYLES
            );

            let controls_bare_text_first_part = "<";
            let enter_bare_text = "ENTER";
            let controls_bare_text_second_part = "> run, <";
            let esc_bare_text = "ESC";
            let controls_bare_text_third_part = "> drop to shell, <";
            let ctrl_c_bare_text = "Ctrl-c";
            let controls_bare_text_fourth_part = "> exit";
            let controls_color = RESET_STYLES
                .foreground(Some(AnsiCode::from(
                    style.colors.text_unselected.emphasis_0,
                )))
                .bold(Some(AnsiCode::On));
            let controls_line_length = controls_bare_text_first_part.len()
                + enter_bare_text.len()
                + controls_bare_text_second_part.len()
                + esc_bare_text.len()
                + controls_bare_text_third_part.len()
                + ctrl_c_bare_text.len()
                + controls_bare_text_fourth_part.len();
            let controls_column_start_position =
                middle_column.saturating_sub(controls_line_length / 2);
            let controls_line = format!(
                "\u{1b}[{};{}H{}<{}{}{}{}> run, <{}{}{}{}> drop to shell, <{}{}{}{}> exit",
                middle_row + 2,
                controls_column_start_position,
                bold_text,
                controls_color,
                enter_bare_text,
                RESET_STYLES,
                bold_text,
                controls_color,
                esc_bare_text,
                RESET_STYLES,
                bold_text,
                controls_color,
                ctrl_c_bare_text,
                RESET_STYLES,
                bold_text
            );
            format!(
                "\u{1b}[?25l{}{}{}{}",
                RESET_STYLES, waiting_to_run_line, controls_line, RESET_STYLES
            )
        },
        None => {
            let bare_text = format!("Waiting to start...");
            let bare_text_width = bare_text.width();
            let column_start_postion = middle_column.saturating_sub(bare_text_width / 2);
            let bold_text = RESET_STYLES.bold(Some(AnsiCode::On));
            let waiting_to_run_line = format!(
                "\u{1b}[?25l\u{1b}[{};{}H{}{}{}",
                middle_row, column_start_postion, bold_text, bare_text, RESET_STYLES
            );

            let controls_bare_text_first_part = "<";
            let enter_bare_text = "ENTER";
            let controls_bare_text_second_part = "> run, <";
            let esc_bare_text = "ESC";
            let controls_bare_text_third_part = "> drop to shell, <";
            let ctrl_c_bare_text = "Ctrl-c";
            let controls_bare_text_fourth_part = "> exit";
            let controls_color = RESET_STYLES
                .foreground(Some(AnsiCode::from(
                    style.colors.text_unselected.emphasis_0,
                )))
                .bold(Some(AnsiCode::On));
            let controls_line_length = controls_bare_text_first_part.len()
                + enter_bare_text.len()
                + controls_bare_text_second_part.len()
                + esc_bare_text.len()
                + controls_bare_text_third_part.len()
                + ctrl_c_bare_text.len()
                + controls_bare_text_fourth_part.len();
            let controls_column_start_position =
                middle_column.saturating_sub(controls_line_length / 2);
            let controls_line = format!(
                "\u{1b}[{};{}H{}<{}{}{}{}> run, <{}{}{}{}> drop to shell, <{}{}{}{}> exit",
                middle_row + 2,
                controls_column_start_position,
                bold_text,
                controls_color,
                enter_bare_text,
                RESET_STYLES,
                bold_text,
                controls_color,
                esc_bare_text,
                RESET_STYLES,
                bold_text,
                controls_color,
                ctrl_c_bare_text,
                RESET_STYLES,
                bold_text
            );
            format!(
                "\u{1b}[?25l{}{}{}{}",
                RESET_STYLES, waiting_to_run_line, controls_line, RESET_STYLES
            )
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct GuestModalShortcuts {
    pub zoom: Vec<String>,
    pub ascend: Vec<String>,
    pub descend: Vec<String>,
}

type GuestModalSegment = (String, RcCharacterStyles);

fn guest_modal_row_width(segments: &[GuestModalSegment]) -> usize {
    segments.iter().map(|(text, _)| text.width()).sum()
}

fn guest_modal_empty_row(
    columns: usize,
    content_x: usize,
    absolute_y: usize,
    fill_styles: RcCharacterStyles,
) -> CharacterChunk {
    guest_modal_segment_row(
        &[],
        0,
        0,
        columns,
        content_x,
        absolute_y,
        fill_styles.clone(),
        fill_styles,
    )
}

#[allow(clippy::too_many_arguments)]
fn guest_modal_segment_row(
    segments: &[GuestModalSegment],
    left_pad: usize,
    block_width: usize,
    columns: usize,
    content_x: usize,
    absolute_y: usize,
    outer_fill: RcCharacterStyles,
    block_fill: RcCharacterStyles,
) -> CharacterChunk {
    let mut characters: Vec<TerminalCharacter> = Vec::with_capacity(columns);
    let block_end = (left_pad + block_width).min(columns);
    let mut used = 0;
    while used < left_pad && used < columns {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            outer_fill.clone(),
        ));
        used += 1;
    }
    for (text, styles) in segments {
        for character in text.chars() {
            let width = character.width().unwrap_or(0).max(1);
            if used + width > columns {
                break;
            }
            characters.push(TerminalCharacter::new_styled(character, styles.clone()));
            used += width;
        }
    }
    while used < block_end {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            block_fill.clone(),
        ));
        used += 1;
    }
    while used < columns {
        characters.push(TerminalCharacter::new_singlewidth_styled(
            ' ',
            outer_fill.clone(),
        ));
        used += 1;
    }
    CharacterChunk::new(characters, content_x, absolute_y)
}

fn guest_modal_hint_segments(
    hint: &str,
    fill_styles: RcCharacterStyles,
    keycode_styles: RcCharacterStyles,
) -> Vec<GuestModalSegment> {
    let mut segments: Vec<GuestModalSegment> = Vec::new();
    let mut buffer = String::new();
    let mut in_keycode = false;
    for character in hint.chars() {
        match character {
            '<' => {
                if !buffer.is_empty() {
                    segments.push((std::mem::take(&mut buffer), fill_styles.clone()));
                }
                buffer.push('<');
                in_keycode = true;
            },
            '>' => {
                buffer.push('>');
                segments.push((std::mem::take(&mut buffer), keycode_styles.clone()));
                in_keycode = false;
            },
            _ => buffer.push(character),
        }
    }
    if !buffer.is_empty() {
        let styles = if in_keycode {
            keycode_styles
        } else {
            fill_styles
        };
        segments.push((buffer, styles));
    }
    segments
}

struct GuestModalStyles {
    fill: RcCharacterStyles,
    title: RcCharacterStyles,
    session: RcCharacterStyles,
    keycode: RcCharacterStyles,
    prompt: RcCharacterStyles,
    option_marker: RcCharacterStyles,
    option_title: RcCharacterStyles,
    selected_marker: RcCharacterStyles,
    selected_title: RcCharacterStyles,
    selected_fill: RcCharacterStyles,
    selected_keycode: RcCharacterStyles,
}

fn guest_modal_keycode_words(
    keys: &[String],
    fill_styles: RcCharacterStyles,
    keycode_styles: RcCharacterStyles,
) -> Vec<GuestModalSegment> {
    if keys.is_empty() {
        return vec![("<unbound>".to_string(), fill_styles)];
    }
    let mut words: Vec<GuestModalSegment> = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            words.push(("+".to_string(), fill_styles.clone()));
        }
        words.push((format!("<{}>", key), keycode_styles.clone()));
    }
    words
}

fn guest_modal_text_words(text: &str, styles: RcCharacterStyles) -> Vec<GuestModalSegment> {
    text.split_whitespace()
        .map(|word| (word.to_string(), styles.clone()))
        .collect()
}

fn guest_modal_wrap_words(
    words: &[GuestModalSegment],
    indent: usize,
    indent_styles: RcCharacterStyles,
    width: usize,
) -> Vec<Vec<GuestModalSegment>> {
    let content_width = width.saturating_sub(indent).max(1);
    let mut lines: Vec<Vec<GuestModalSegment>> = Vec::new();
    let mut current: Vec<GuestModalSegment> = Vec::new();
    let mut current_width = 0;
    for (word, styles) in words {
        let word_width = word.width();
        let extra = if current.is_empty() {
            word_width
        } else {
            1 + word_width
        };
        if !current.is_empty() && current_width + extra > content_width {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if current.is_empty() {
            current.push((word.clone(), styles.clone()));
            current_width = word_width;
        } else {
            current.push((" ".to_string(), styles.clone()));
            current.push((word.clone(), styles.clone()));
            current_width += 1 + word_width;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(Vec::new());
    }
    for line in lines.iter_mut() {
        line.insert(0, (" ".repeat(indent), indent_styles.clone()));
    }
    lines
}

pub fn guest_modal_chunks(
    columns: usize,
    rows: usize,
    content_x: usize,
    content_y: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> Vec<CharacterChunk> {
    if rows < 1 || columns < 1 {
        return vec![];
    }
    let styles = guest_modal_styles(style);
    let fill_styles = styles.fill.clone();

    let mut chunks: Vec<CharacterChunk> = Vec::with_capacity(rows);
    for row in 0..rows {
        chunks.push(guest_modal_empty_row(
            columns,
            content_x,
            content_y + row,
            fill_styles.clone(),
        ));
    }

    let layout = guest_modal_layout(columns, rows, &styles, session_name, selected, shortcuts);
    for (offset, row) in layout.rows.iter().enumerate() {
        let current_row = layout.start_row + offset;
        if current_row >= rows {
            break;
        }
        chunks[current_row] = guest_modal_segment_row(
            &row.segments,
            layout.left_pad,
            layout.block_width,
            columns,
            content_x,
            content_y + current_row,
            fill_styles.clone(),
            row.block_fill.clone(),
        );
    }

    chunks
}

fn guest_modal_styles(style: &Style) -> GuestModalStyles {
    let base_declaration = style.colors.text_unselected;
    let selected_declaration = style.colors.text_selected;
    let fill: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .into();
    let title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_0)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let session: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_2)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let keycode: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_3)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let prompt: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let option_marker: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.base)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let option_title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(base_declaration.emphasis_1)))
        .background(Some(AnsiCode::from(base_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_marker: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.base)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_title: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.emphasis_1)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    let selected_fill: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.base)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .into();
    let selected_keycode: RcCharacterStyles = RESET_STYLES
        .foreground(Some(AnsiCode::from(selected_declaration.emphasis_3)))
        .background(Some(AnsiCode::from(selected_declaration.background)))
        .bold(Some(AnsiCode::On))
        .into();
    GuestModalStyles {
        fill,
        title,
        session,
        keycode,
        prompt,
        option_marker,
        option_title,
        selected_marker,
        selected_title,
        selected_fill,
        selected_keycode,
    }
}

struct GuestModalRow {
    segments: Vec<GuestModalSegment>,
    block_fill: RcCharacterStyles,
}

struct GuestModalLayout {
    rows: Vec<GuestModalRow>,
    block_width: usize,
    option_rows: Vec<usize>,
    left_pad: usize,
    start_row: usize,
}

fn guest_modal_fallback_layout(
    columns: usize,
    rows: usize,
    styles: &GuestModalStyles,
    session_name: &str,
) -> GuestModalLayout {
    let fallback: Vec<GuestModalSegment> = vec![
        ("Nested Zellij session: ".to_string(), styles.title.clone()),
        (session_name.to_string(), styles.session.clone()),
        (" ".to_string(), styles.fill.clone()),
        ("<Esc>".to_string(), styles.keycode.clone()),
        (" dismiss".to_string(), styles.fill.clone()),
    ];
    let block_width = guest_modal_row_width(&fallback).min(columns);
    let left_pad = center_modal_pad(block_width, columns);
    GuestModalLayout {
        rows: vec![GuestModalRow {
            segments: fallback,
            block_fill: styles.fill.clone(),
        }],
        block_width,
        option_rows: vec![],
        left_pad,
        start_row: rows / 2,
    }
}

fn guest_modal_layout(
    columns: usize,
    rows: usize,
    styles: &GuestModalStyles,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> GuestModalLayout {
    if columns < 20 {
        return guest_modal_fallback_layout(columns, rows, styles, session_name);
    }

    let title: Vec<GuestModalSegment> = vec![
        (
            "Nested Zellij session detected: ".to_string(),
            styles.title.clone(),
        ),
        (session_name.to_string(), styles.session.clone()),
    ];
    let prompt: Vec<GuestModalSegment> = vec![(
        "What would you like to do?".to_string(),
        styles.prompt.clone(),
    )];
    let options = [
        ("1. ", "Zoom in and control this session"),
        ("2. ", "Control this session automatically on focus (AUTO)"),
        ("3. ", "Leave it be, enter manually later (MANUAL)"),
    ];
    let explanation_texts = [
        "The session will take up the whole screen. Toggle this on/off with",
        "When this pane gains focus, keybindings will be sent to this session. Ascend to the current session with",
        "Manually descend into this session when focused with",
    ];
    let shortcut_keys = [&shortcuts.zoom, &shortcuts.ascend, &shortcuts.descend];
    let hint = "<↓↑> select  <Enter> confirm  <1-3> direct  <Esc> dismiss";
    let hint_segments =
        guest_modal_hint_segments(hint, styles.fill.clone(), styles.keycode.clone());

    let fixed_lines: Vec<&[GuestModalSegment]> = vec![
        title.as_slice(),
        prompt.as_slice(),
        hint_segments.as_slice(),
    ];
    let mut block_width = fixed_lines
        .iter()
        .map(|segments| guest_modal_row_width(segments))
        .max()
        .unwrap_or(0);
    for (number, option_title) in options.iter() {
        block_width = block_width.max(2 + number.width() + option_title.width());
    }
    let block_width = block_width.min(columns);

    let mut rows_content: Vec<GuestModalRow> = Vec::new();
    let mut option_rows: Vec<usize> = Vec::new();
    rows_content.push(GuestModalRow {
        segments: title,
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: prompt,
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    for (index, (number, option_title)) in options.iter().enumerate() {
        let is_selected = index == selected;
        let (marker_styles, title_styles, text_styles, keycode_styles, block_fill) = if is_selected
        {
            (
                styles.selected_marker.clone(),
                styles.selected_title.clone(),
                styles.selected_fill.clone(),
                styles.selected_keycode.clone(),
                styles.selected_fill.clone(),
            )
        } else {
            (
                styles.option_marker.clone(),
                styles.option_title.clone(),
                styles.fill.clone(),
                styles.keycode.clone(),
                styles.fill.clone(),
            )
        };
        let marker = if is_selected { "> " } else { "  " };
        option_rows.push(rows_content.len());
        rows_content.push(GuestModalRow {
            segments: vec![
                (format!("{}{}", marker, number), marker_styles),
                (option_title.to_string(), title_styles),
            ],
            block_fill: block_fill.clone(),
        });
        let mut words = guest_modal_text_words(explanation_texts[index], text_styles.clone());
        words.extend(guest_modal_keycode_words(
            shortcut_keys[index],
            text_styles.clone(),
            keycode_styles,
        ));
        for line in guest_modal_wrap_words(&words, 5, text_styles.clone(), block_width) {
            rows_content.push(GuestModalRow {
                segments: line,
                block_fill: block_fill.clone(),
            });
        }
    }
    rows_content.push(GuestModalRow {
        segments: Vec::new(),
        block_fill: styles.fill.clone(),
    });
    rows_content.push(GuestModalRow {
        segments: hint_segments,
        block_fill: styles.fill.clone(),
    });

    let block_height = rows_content.len();
    if rows < block_height {
        return guest_modal_fallback_layout(columns, rows, styles, session_name);
    }
    let left_pad = center_modal_pad(block_width, columns);
    let start_row = rows.saturating_sub(block_height) / 2;

    GuestModalLayout {
        rows: rows_content,
        block_width,
        option_rows,
        left_pad,
        start_row,
    }
}

pub fn guest_modal_option_at_content_row(
    rows: usize,
    columns: usize,
    row: usize,
    style: &Style,
    session_name: &str,
    selected: usize,
    shortcuts: &GuestModalShortcuts,
) -> Option<usize> {
    if columns < 20 {
        return None;
    }
    let styles = guest_modal_styles(style);
    let layout = guest_modal_layout(columns, rows, &styles, session_name, selected, shortcuts);
    if layout.option_rows.is_empty() {
        return None;
    }
    layout
        .option_rows
        .iter()
        .position(|&option_row| layout.start_row + option_row == row)
}

fn center_modal_pad(block_width: usize, columns: usize) -> usize {
    if block_width >= columns {
        return 0;
    }
    (columns - block_width) / 2
}
