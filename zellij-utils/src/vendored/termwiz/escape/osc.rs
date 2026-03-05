use crate::vendored::termwiz::color::SrgbaTuple;
pub use crate::vendored::termwiz::hyperlink::Hyperlink;
use crate::vendored::termwiz::Result;
use crate::{vendored_termwiz_bail as bail, vendored_termwiz_ensure as ensure};
use base64::Engine;
use bitflags::bitflags;
use num_derive::*;
use num_traits::FromPrimitive;
use ordered_float::NotNan;
use std::collections::HashMap;
use std::fmt::{Display, Error as FmtError, Formatter, Result as FmtResult};
use std::str;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub enum ColorOrQuery {
    Color(SrgbaTuple),
    Query,
}

impl Display for ColorOrQuery {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            ColorOrQuery::Query => write!(f, "?"),
            ColorOrQuery::Color(c) => write!(f, "{}", c.to_x11_16bit_rgb_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum OperatingSystemCommand {
    SetIconNameAndWindowTitle(String),
    SetWindowTitle(String),
    SetWindowTitleSun(String),
    SetIconName(String),
    SetIconNameSun(String),
    SetHyperlink(Option<Hyperlink>),
    ClearSelection(Selection),
    QuerySelection(Selection),
    SetSelection(Selection, String),
    SystemNotification(String),
    ITermProprietary(ITermProprietary),
    FinalTermSemanticPrompt(FinalTermSemanticPrompt),
    ChangeColorNumber(Vec<ChangeColorPair>),
    ChangeDynamicColors(DynamicColorNumber, Vec<ColorOrQuery>),
    ResetDynamicColor(DynamicColorNumber),
    CurrentWorkingDirectory(String),
    ResetColors(Vec<u8>),
    RxvtExtension(Vec<String>),
    ConEmuProgress(Progress),

    Unspecified(Vec<Vec<u8>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum DynamicColorNumber {
    TextForegroundColor = 10,
    TextBackgroundColor = 11,
    TextCursorColor = 12,
    MouseForegroundColor = 13,
    MouseBackgroundColor = 14,
    TektronixForegroundColor = 15,
    TektronixBackgroundColor = 16,
    HighlightBackgroundColor = 17,
    TektronixCursorColor = 18,
    HighlightForegroundColor = 19,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChangeColorPair {
    pub palette_index: u8,
    pub color: ColorOrQuery,
}

bitflags! {
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection :u16{
    const NONE = 0;
    const CLIPBOARD = 1<<1;
    const PRIMARY=1<<2;
    const SELECT=1<<3;
    const CUT0=1<<4;
    const CUT1=1<<5;
    const CUT2=1<<6;
    const CUT3=1<<7;
    const CUT4=1<<8;
    const CUT5=1<<9;
    const CUT6=1<<10;
    const CUT7=1<<11;
    const CUT8=1<<12;
    const CUT9=1<<13;
}
}

impl Selection {
    fn try_parse(buf: &[u8]) -> Result<Selection> {
        if buf == b"" {
            Ok(Selection::SELECT | Selection::CUT0)
        } else {
            let mut s = Selection::NONE;
            for c in buf {
                s |= match c {
                    b'c' => Selection::CLIPBOARD,
                    b'p' => Selection::PRIMARY,
                    b's' => Selection::SELECT,
                    b'0' => Selection::CUT0,
                    b'1' => Selection::CUT1,
                    b'2' => Selection::CUT2,
                    b'3' => Selection::CUT3,
                    b'4' => Selection::CUT4,
                    b'5' => Selection::CUT5,
                    b'6' => Selection::CUT6,
                    b'7' => Selection::CUT7,
                    b'8' => Selection::CUT8,
                    b'9' => Selection::CUT9,
                    _ => bail!("invalid selection {:?}", buf),
                }
            }
            Ok(s)
        }
    }
}

impl Display for Selection {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        macro_rules! item {
            ($variant:ident, $s:expr) => {
                if (*self & Selection::$variant) != Selection::NONE {
                    write!(f, $s)?;
                }
            };
        }

        item!(CLIPBOARD, "c");
        item!(PRIMARY, "p");
        item!(SELECT, "s");
        item!(CUT0, "0");
        item!(CUT1, "1");
        item!(CUT2, "2");
        item!(CUT3, "3");
        item!(CUT4, "4");
        item!(CUT5, "5");
        item!(CUT6, "6");
        item!(CUT7, "7");
        item!(CUT8, "8");
        item!(CUT9, "9");
        Ok(())
    }
}

impl OperatingSystemCommand {
    pub fn parse(osc: &[&[u8]]) -> Self {
        Self::internal_parse(osc).unwrap_or_else(|err| {
            let mut vec = Vec::new();
            for slice in osc {
                vec.push(slice.to_vec());
            }
            log::trace!(
                "OSC internal parse err: {}, track as Unspecified {:?}",
                err,
                vec
            );
            OperatingSystemCommand::Unspecified(vec)
        })
    }

    fn parse_selection(osc: &[&[u8]]) -> Result<Self> {
        if osc.len() == 2 {
            Selection::try_parse(osc[1]).map(OperatingSystemCommand::ClearSelection)
        } else if osc.len() == 3 && osc[2] == b"?" {
            Selection::try_parse(osc[1]).map(OperatingSystemCommand::QuerySelection)
        } else if osc.len() == 3 {
            let sel = Selection::try_parse(osc[1])?;
            let bytes = base64_decode(osc[2])?;
            let s = String::from_utf8(bytes)?;
            Ok(OperatingSystemCommand::SetSelection(sel, s))
        } else {
            bail!("unhandled OSC 52: {:?}", osc);
        }
    }

    fn parse_reset_colors(osc: &[&[u8]]) -> Result<Self> {
        let mut colors = vec![];
        let mut iter = osc.iter();
        iter.next(); // skip the command word that we already know is present

        while let Some(index) = iter.next() {
            if index.is_empty() {
                continue;
            }
            let index: u8 = str::from_utf8(index)?.parse()?;
            colors.push(index);
        }

        Ok(OperatingSystemCommand::ResetColors(colors))
    }

    fn parse_change_color_number(osc: &[&[u8]]) -> Result<Self> {
        let mut pairs = vec![];
        let mut iter = osc.iter();
        iter.next(); // skip the command word that we already know is present

        while let (Some(index), Some(spec)) = (iter.next(), iter.next()) {
            let index: u8 = str::from_utf8(index)?.parse()?;
            let spec = str::from_utf8(spec)?;
            let spec = if spec == "?" {
                ColorOrQuery::Query
            } else {
                ColorOrQuery::Color(
                    SrgbaTuple::from_str(spec)
                        .map_err(|()| format!("invalid color spec {:?}", spec))?,
                )
            };

            pairs.push(ChangeColorPair {
                palette_index: index,
                color: spec,
            });
        }

        Ok(OperatingSystemCommand::ChangeColorNumber(pairs))
    }

    fn parse_reset_dynamic_color_number(idx: u8) -> Result<Self> {
        let which_color: DynamicColorNumber = FromPrimitive::from_u8(idx)
            .ok_or_else(|| format!("osc code is not a valid DynamicColorNumber!?"))?;

        Ok(OperatingSystemCommand::ResetDynamicColor(which_color))
    }

    fn parse_change_dynamic_color_number(idx: u8, osc: &[&[u8]]) -> Result<Self> {
        let which_color: DynamicColorNumber = FromPrimitive::from_u8(idx)
            .ok_or_else(|| format!("osc code is not a valid DynamicColorNumber!?"))?;
        let mut colors = vec![];
        for spec in osc.iter().skip(1) {
            if spec == b"?" {
                colors.push(ColorOrQuery::Query);
            } else {
                let spec = str::from_utf8(spec)?;
                colors.push(ColorOrQuery::Color(
                    SrgbaTuple::from_str(spec)
                        .map_err(|()| format!("invalid color spec {:?}", spec))?,
                ));
            }
        }

        Ok(OperatingSystemCommand::ChangeDynamicColors(
            which_color,
            colors,
        ))
    }

    fn internal_parse(osc: &[&[u8]]) -> Result<Self> {
        ensure!(!osc.is_empty(), "no params");
        let p1str = String::from_utf8_lossy(osc[0]);

        if p1str.is_empty() {
            bail!("zero length osc");
        }

        // Ugh, this is to handle "OSC ltitle" which is a legacyish
        // OSC for encoding a window title change request.  These days
        // OSC 2 is preferred for this purpose, but we need to support
        // generating and parsing the legacy form because it is the
        // response for the CSI ReportWindowTitle.
        // So, for non-numeric OSCs, we look up the prefix and use that.
        // This only works if the non-numeric OSC code has length == 1.
        let osc_code = if !p1str.chars().nth(0).unwrap().is_ascii_digit() && osc.len() == 1 {
            let mut p1 = String::new();
            p1.push(p1str.chars().nth(0).unwrap());
            OperatingSystemCommandCode::from_code(&p1)
        } else {
            OperatingSystemCommandCode::from_code(&p1str)
        }
        .ok_or_else(|| format!("unknown code"))?;

        macro_rules! single_string {
            ($variant:ident) => {{
                if osc.len() != 2 {
                    bail!("wrong param count");
                }
                let s = String::from_utf8(osc[1].to_vec())?;
                Ok(OperatingSystemCommand::$variant(s))
            }};
        }

        macro_rules! single_title_string {
            ($variant:ident) => {{
                if osc.len() < 2 {
                    bail!("wrong param count");
                }
                let mut s = String::from_utf8(osc[1].to_vec())?;
                for i in 2..osc.len() {
                    s = [s, String::from_utf8(osc[i].to_vec())?].join(";");
                }

                Ok(OperatingSystemCommand::$variant(s))
            }};
        }

        use self::OperatingSystemCommandCode::*;
        match osc_code {
            SetIconNameAndWindowTitle => single_title_string!(SetIconNameAndWindowTitle),
            SetWindowTitle => single_title_string!(SetWindowTitle),
            SetWindowTitleSun => Ok(OperatingSystemCommand::SetWindowTitleSun(
                p1str[1..].to_owned(),
            )),

            SetIconName => single_title_string!(SetIconName),
            SetIconNameSun => Ok(OperatingSystemCommand::SetIconNameSun(
                p1str[1..].to_owned(),
            )),
            SetHyperlink => Ok(OperatingSystemCommand::SetHyperlink(Hyperlink::parse(osc)?)),
            ManipulateSelectionData => Self::parse_selection(osc),
            SystemNotification => {
                if osc.len() >= 3 && osc[1] == b"4" {
                    fn get_pct(v: &&[u8]) -> u8 {
                        let number = str::from_utf8(v).unwrap_or("0");
                        number.parse::<u8>().unwrap_or(0).max(0).min(100)
                    }
                    match osc[2] {
                        b"0" => return Ok(OperatingSystemCommand::ConEmuProgress(Progress::None)),
                        b"1" => {
                            let pct = osc.get(3).map(get_pct).unwrap_or(0);
                            return Ok(OperatingSystemCommand::ConEmuProgress(
                                Progress::SetPercentage(pct),
                            ));
                        },
                        b"2" => {
                            let pct = osc.get(3).map(get_pct).unwrap_or(0);
                            return Ok(OperatingSystemCommand::ConEmuProgress(Progress::SetError(
                                pct,
                            )));
                        },
                        b"3" => {
                            return Ok(OperatingSystemCommand::ConEmuProgress(
                                Progress::SetIndeterminate,
                            ))
                        },
                        b"4" => {
                            return Ok(OperatingSystemCommand::ConEmuProgress(Progress::Paused))
                        },
                        _ => {},
                    }
                }
                single_string!(SystemNotification)
            },
            SetCurrentWorkingDirectory => single_string!(CurrentWorkingDirectory),
            ITermProprietary => {
                self::ITermProprietary::parse(osc).map(OperatingSystemCommand::ITermProprietary)
            },
            RxvtProprietary => {
                let mut vec = vec![];
                for slice in osc.iter().skip(1) {
                    vec.push(String::from_utf8_lossy(slice).to_string());
                }
                Ok(OperatingSystemCommand::RxvtExtension(vec))
            },
            FinalTermSemanticPrompt => self::FinalTermSemanticPrompt::parse(osc)
                .map(OperatingSystemCommand::FinalTermSemanticPrompt),
            ChangeColorNumber => Self::parse_change_color_number(osc),
            ResetColors => Self::parse_reset_colors(osc),

            ResetSpecialColor
            | ResetTextForegroundColor
            | ResetTextBackgroundColor
            | ResetTextCursorColor
            | ResetMouseForegroundColor
            | ResetMouseBackgroundColor
            | ResetTektronixForegroundColor
            | ResetTektronixBackgroundColor
            | ResetHighlightColor
            | ResetTektronixCursorColor
            | ResetHighlightForegroundColor => Self::parse_reset_dynamic_color_number(
                p1str.parse::<u8>().unwrap().saturating_sub(100),
            ),

            SetTextForegroundColor
            | SetTextBackgroundColor
            | SetTextCursorColor
            | SetMouseForegroundColor
            | SetMouseBackgroundColor
            | SetTektronixForegroundColor
            | SetTektronixBackgroundColor
            | SetHighlightBackgroundColor
            | SetTektronixCursorColor
            | SetHighlightForegroundColor => {
                Self::parse_change_dynamic_color_number(p1str.parse::<u8>().unwrap(), osc)
            },

            osc_code => bail!("{:?} not impl", osc_code),
        }
    }
}

macro_rules! osc_entries {
($(
    $( #[doc=$doc:expr] )*
    $label:ident = $value:expr
),* $(,)?) => {

#[derive(Debug, Clone, PartialEq, Eq, FromPrimitive, Hash, Copy)]
pub enum OperatingSystemCommandCode {
    $(
        $( #[doc=$doc] )*
        $label,
    )*
}

impl OscMap {
    fn new() -> Self {
        let mut code_to_variant = HashMap::new();
        let mut variant_to_code = HashMap::new();

        use OperatingSystemCommandCode::*;

        $(
            code_to_variant.insert($value, $label);
            variant_to_code.insert($label, $value);
        )*

        Self {
            code_to_variant,
            variant_to_code,
        }
    }
}
    };
}

osc_entries!(
    SetIconNameAndWindowTitle = "0",
    SetIconName = "1",
    SetWindowTitle = "2",
    SetXWindowProperty = "3",
    ChangeColorNumber = "4",
    ChangeSpecialColorNumber = "5",
    /// iTerm2
    ChangeTitleTabColor = "6",
    SetCurrentWorkingDirectory = "7",
    /// See https://gist.github.com/egmontkob/eb114294efbcd5adb1944c9f3cb5feda
    SetHyperlink = "8",
    /// iTerm2
    SystemNotification = "9",
    SetTextForegroundColor = "10",
    SetTextBackgroundColor = "11",
    SetTextCursorColor = "12",
    SetMouseForegroundColor = "13",
    SetMouseBackgroundColor = "14",
    SetTektronixForegroundColor = "15",
    SetTektronixBackgroundColor = "16",
    SetHighlightBackgroundColor = "17",
    SetTektronixCursorColor = "18",
    SetHighlightForegroundColor = "19",
    SetLogFileName = "46",
    SetFont = "50",
    EmacsShell = "51",
    ManipulateSelectionData = "52",
    ResetColors = "104",
    ResetSpecialColor = "105",
    ResetTextForegroundColor = "110",
    ResetTextBackgroundColor = "111",
    ResetTextCursorColor = "112",
    ResetMouseForegroundColor = "113",
    ResetMouseBackgroundColor = "114",
    ResetTektronixForegroundColor = "115",
    ResetTektronixBackgroundColor = "116",
    ResetHighlightColor = "117",
    ResetTektronixCursorColor = "118",
    ResetHighlightForegroundColor = "119",
    RxvtProprietary = "777",
    FinalTermSemanticPrompt = "133",
    ITermProprietary = "1337",
    /// Here the "Sun" suffix comes from the table in
    /// <https://invisible-island.net/xterm/ctlseqs/ctlseqs.html#h3-Miscellaneous>
    /// that lays out various window related escape sequences.
    SetWindowTitleSun = "l",
    SetIconNameSun = "L",
);

struct OscMap {
    code_to_variant: HashMap<&'static str, OperatingSystemCommandCode>,
    variant_to_code: HashMap<OperatingSystemCommandCode, &'static str>,
}

lazy_static::lazy_static! {
    static ref OSC_MAP: OscMap = OscMap::new();
}

impl OperatingSystemCommandCode {
    fn from_code(code: &str) -> Option<Self> {
        OSC_MAP.code_to_variant.get(code).copied()
    }

    fn as_code(self) -> &'static str {
        OSC_MAP.variant_to_code.get(&self).unwrap()
    }
}

impl Display for OperatingSystemCommand {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "\x1b]")?;

        macro_rules! single_string {
            ($variant:ident, $s:expr) => {{
                let code = OperatingSystemCommandCode::$variant.as_code();
                match OperatingSystemCommandCode::$variant {
                    OperatingSystemCommandCode::SetWindowTitleSun
                    | OperatingSystemCommandCode::SetIconNameSun => {
                        // For the legacy sun terminals, the `l` and `L` OSCs are
                        // not separated by `;`.
                        write!(f, "{}{}", code, $s)?;
                    },
                    _ => {
                        // In the common case, the OSC is numeric and is separated
                        // from the rest of the string
                        write!(f, "{};{}", code, $s)?;
                    },
                }
            }};
        }

        use self::OperatingSystemCommand::*;
        match self {
            SetIconNameAndWindowTitle(title) => single_string!(SetIconNameAndWindowTitle, title),
            SetWindowTitle(title) => single_string!(SetWindowTitle, title),
            SetWindowTitleSun(title) => single_string!(SetWindowTitleSun, title),
            SetIconName(title) => single_string!(SetIconName, title),
            SetIconNameSun(title) => single_string!(SetIconNameSun, title),
            SetHyperlink(Some(link)) => link.fmt(f)?,
            SetHyperlink(None) => write!(f, "8;;")?,
            RxvtExtension(params) => write!(f, "777;{}", params.join(";"))?,
            Unspecified(v) => {
                for (idx, item) in v.iter().enumerate() {
                    if idx > 0 {
                        write!(f, ";")?;
                    }
                    f.write_str(&String::from_utf8_lossy(item))?;
                }
            },
            ClearSelection(s) => write!(f, "52;{}", s)?,
            QuerySelection(s) => write!(f, "52;{};?", s)?,
            SetSelection(s, val) => write!(f, "52;{};{}", s, base64_encode(val))?,
            SystemNotification(s) => write!(f, "9;{}", s)?,
            ITermProprietary(i) => i.fmt(f)?,
            FinalTermSemanticPrompt(i) => i.fmt(f)?,
            ResetColors(colors) => {
                write!(f, "104")?;
                for c in colors {
                    write!(f, ";{}", c)?;
                }
            },
            ChangeColorNumber(specs) => {
                write!(f, "4;")?;
                for pair in specs {
                    write!(f, "{};{}", pair.palette_index, pair.color)?
                }
            },
            ChangeDynamicColors(first_color, colors) => {
                write!(f, "{}", *first_color as u8)?;
                for color in colors {
                    write!(f, ";{}", color)?
                }
            },
            ResetDynamicColor(color) => {
                write!(f, "{}", 100 + *color as u8)?;
            },
            CurrentWorkingDirectory(s) => write!(f, "7;{}", s)?,
            ConEmuProgress(Progress::None) => write!(f, "9;4;0")?,
            ConEmuProgress(Progress::SetPercentage(pct)) => write!(f, "9;4;1;{pct}")?,
            ConEmuProgress(Progress::SetError(pct)) => write!(f, "9;4;2;{pct}")?,
            ConEmuProgress(Progress::SetIndeterminate) => write!(f, "9;4;3")?,
            ConEmuProgress(Progress::Paused) => write!(f, "9;4;4")?,
        };
        // Use the longer form ST as neovim doesn't like the BEL version
        write!(f, "\x1b\\")?;
        Ok(())
    }
}

/// https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalTermClick {
    /// Allow motion only within the single input line using left/right arrow keys
    Line,
    /// Allow moving between multiple lines of input using left/right arrow keys
    MultipleLine,
    /// Allow left/right and conservative up/down arrow motion
    ConservativeVertical,
    /// Allow left/right and up/down motion, and the line editor ensures that
    /// there are no spurious trailing spaces at ends of lines and that vertical
    /// motion across shorter lines causes some horizontal cursor motion.
    SmartVertical,
}

impl std::convert::TryFrom<&str> for FinalTermClick {
    type Error = crate::vendored::termwiz::Error;
    fn try_from(s: &str) -> Result<Self> {
        match s {
            "line" => Ok(Self::Line),
            "m" => Ok(Self::MultipleLine),
            "v" => Ok(Self::ConservativeVertical),
            "w" => Ok(Self::SmartVertical),
            _ => bail!("invalid FinalTermClick {}", s),
        }
    }
}

impl Display for FinalTermClick {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Line => write!(f, "line"),
            Self::MultipleLine => write!(f, "m"),
            Self::ConservativeVertical => write!(f, "v"),
            Self::SmartVertical => write!(f, "w"),
        }
    }
}

/// https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalTermPromptKind {
    /// A normal left side primary prompt
    Initial,
    /// A right-aligned prompt
    RightSide,
    /// A continuation prompt for an input that can be edited
    Continuation,
    /// A continuation prompt where the input cannot be edited
    Secondary,
}

impl Default for FinalTermPromptKind {
    fn default() -> Self {
        Self::Initial
    }
}

impl std::convert::TryFrom<&str> for FinalTermPromptKind {
    type Error = crate::vendored::termwiz::Error;
    fn try_from(s: &str) -> Result<Self> {
        match s {
            "i" => Ok(Self::Initial),
            "r" => Ok(Self::RightSide),
            "c" => Ok(Self::Continuation),
            "s" => Ok(Self::Secondary),
            _ => bail!("invalid FinalTermPromptKind {}", s),
        }
    }
}

impl Display for FinalTermPromptKind {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        match self {
            Self::Initial => write!(f, "i"),
            Self::RightSide => write!(f, "r"),
            Self::Continuation => write!(f, "c"),
            Self::Secondary => write!(f, "s"),
        }
    }
}

/// https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinalTermSemanticPrompt {
    /// Do a "fresh line"; if the cursor is at the left margin then
    /// do nothing, otherwise perform the equivalent of "\r\n"
    FreshLine,

    /// Do a "fresh line" as above and then place the terminal into
    /// prompt mode; the output between now and the next marker is
    /// considered part of the prompt.
    FreshLineAndStartPrompt {
        aid: Option<String>,
        cl: Option<FinalTermClick>,
    },

    /// Denote the end of a command output and then perform FreshLine
    MarkEndOfCommandWithFreshLine {
        aid: Option<String>,
        cl: Option<FinalTermClick>,
    },

    /// Start a prompt
    StartPrompt(FinalTermPromptKind),

    /// Mark the end of a prompt and the start of the user input.
    /// The terminal considers all subsequent output to be "user input"
    /// until the next semantic marker.
    MarkEndOfPromptAndStartOfInputUntilNextMarker,

    /// Mark the end of a prompt and the start of the user input.
    /// The terminal considers all subsequent output to be "user input"
    /// until the end of the line.
    MarkEndOfPromptAndStartOfInputUntilEndOfLine,

    MarkEndOfInputAndStartOfOutput {
        aid: Option<String>,
    },

    /// Indicates the result of the command
    CommandStatus {
        status: i32,
        aid: Option<String>,
    },
}

impl FinalTermSemanticPrompt {
    fn parse(osc: &[&[u8]]) -> Result<Self> {
        ensure!(osc.len() > 1, "not enough args");
        let param = String::from_utf8_lossy(osc[1]);

        macro_rules! single {
            ($variant:ident, $text:expr) => {
                if osc.len() == 2 && param == $text {
                    return Ok(FinalTermSemanticPrompt::$variant);
                }
            };
        }

        single!(FreshLine, "L");
        single!(MarkEndOfPromptAndStartOfInputUntilNextMarker, "B");
        single!(MarkEndOfPromptAndStartOfInputUntilEndOfLine, "I");

        let mut params = HashMap::new();
        use std::convert::TryInto;

        for s in osc.iter().skip(if param == "D" { 3 } else { 2 }) {
            if let Some(equal) = s.iter().position(|c| *c == b'=') {
                let key = &s[..equal];
                let value = &s[equal + 1..];
                params.insert(str::from_utf8(key)?, str::from_utf8(value)?);
            } else if !s.is_empty() {
                bail!("malformed FinalTermSemanticPrompt");
            }
        }

        if param == "A" {
            return Ok(Self::FreshLineAndStartPrompt {
                aid: params.get("aid").map(|&s| s.to_owned()),
                cl: match params.get("cl") {
                    Some(&cl) => Some(cl.try_into()?),
                    None => None,
                },
            });
        }

        if param == "C" {
            return Ok(Self::MarkEndOfInputAndStartOfOutput {
                aid: params.get("aid").map(|&s| s.to_owned()),
            });
        }

        if param == "D" {
            let status = match osc.get(2).map(|&p| p) {
                Some(s) => match str::from_utf8(s) {
                    Ok(s) => s.parse().unwrap_or(0),
                    _ => 0,
                },
                _ => 0,
            };

            return Ok(Self::CommandStatus {
                status,
                aid: params.get("aid").map(|&s| s.to_owned()),
            });
        }

        if param == "N" {
            return Ok(Self::MarkEndOfCommandWithFreshLine {
                aid: params.get("aid").map(|&s| s.to_owned()),
                cl: match params.get("cl") {
                    Some(&cl) => Some(cl.try_into()?),
                    None => None,
                },
            });
        }

        if param == "P" {
            return Ok(Self::StartPrompt(match params.get("k") {
                Some(&cl) => cl.try_into()?,
                None => FinalTermPromptKind::default(),
            }));
        }

        bail!(
            "invalid FinalTermSemanticPrompt p1:{:?}, params:{:?}",
            param,
            params
        );
    }
}

impl Display for FinalTermSemanticPrompt {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "133;")?;
        match self {
            Self::FreshLine => write!(f, "L")?,
            Self::FreshLineAndStartPrompt { aid, cl } => {
                write!(f, "A")?;
                if let Some(aid) = aid {
                    write!(f, ";aid={}", aid)?;
                }
                if let Some(cl) = cl {
                    write!(f, ";cl={}", cl)?;
                }
            },
            Self::MarkEndOfCommandWithFreshLine { aid, cl } => {
                write!(f, "N")?;
                if let Some(aid) = aid {
                    write!(f, ";aid={}", aid)?;
                }
                if let Some(cl) = cl {
                    write!(f, ";cl={}", cl)?;
                }
            },
            Self::StartPrompt(kind) => {
                write!(f, "P;k={}", kind)?;
            },
            Self::MarkEndOfPromptAndStartOfInputUntilNextMarker => write!(f, "B")?,
            Self::MarkEndOfPromptAndStartOfInputUntilEndOfLine => write!(f, "I")?,
            Self::MarkEndOfInputAndStartOfOutput { aid } => {
                write!(f, "C")?;
                if let Some(aid) = aid {
                    write!(f, ";aid={}", aid)?;
                }
            },
            Self::CommandStatus {
                status,
                aid: Some(aid),
            } => {
                write!(f, "D;{};err={};aid={}", status, status, aid)?;
            },
            Self::CommandStatus { status, aid: None } => {
                write!(f, "D;{}", status)?;
            },
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    None,
    SetPercentage(u8),
    SetError(u8),
    SetIndeterminate,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ITermProprietary {
    /// The "Set Mark" command allows you to record a location and then jump back to it later
    SetMark,
    /// To bring iTerm2 to the foreground
    StealFocus,
    /// To erase the scrollback history
    ClearScrollback,
    /// To inform iTerm2 of the current directory to help semantic history
    CurrentDir(String),
    /// To change the session's profile on the fly
    SetProfile(String),
    /// Currently defined values for the string parameter are "rule", "find", "font"
    /// or an empty string.  iTerm2 will go into paste mode until EndCopy is received.
    CopyToClipboard(String),
    /// Ends CopyToClipboard mode in iTerm2.
    EndCopy,
    /// The boolean should be yes or no. This shows or hides the cursor guide
    HighlightCursorLine(bool),
    /// Request that the terminal send a ReportCellSize response
    RequestCellSize,
    /// The response to RequestCellSize.  The height and width are the dimensions
    /// of a cell measured in points according to the docs, but in practice, they
    /// are actually pixels.
    /// If scale is_some(), the width and height will be multiplied by scale to
    /// get the true device dimensions
    ReportCellSize {
        height_pixels: NotNan<f32>,
        width_pixels: NotNan<f32>,
        scale: Option<NotNan<f32>>,
    },
    /// Place a string in the systems pasteboard
    Copy(String),
    /// Each iTerm2 session has internal variables (as described in
    /// <https://www.iterm2.com/documentation-badges.html>). This escape sequence reports
    /// a variable's value.  The response is another ReportVariable.
    ReportVariable(String),
    /// User-defined variables may be set with the following escape sequence
    SetUserVar {
        name: String,
        value: String,
    },
    SetBadgeFormat(String),
    /// Download file data from the application.
    File(Box<ITermFileData>),

    /// Configure unicode version
    UnicodeVersion(ITermUnicodeVersionOp),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ITermUnicodeVersionOp {
    Set(u8),
    Push(Option<String>),
    Pop(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ITermFileData {
    /// file name
    pub name: Option<String>,
    /// size of the data in bytes; this is used by iterm to show progress
    /// while waiting for the rest of the payload
    pub size: Option<usize>,
    /// width to render
    pub width: ITermDimension,
    /// height to render
    pub height: ITermDimension,
    /// if true, preserve aspect ratio when fitting to width/height
    pub preserve_aspect_ratio: bool,
    /// if true, attempt to display in the terminal rather than downloading to
    /// the users download directory
    pub inline: bool,
    /// if true, do not move the cursor
    pub do_not_move_cursor: bool,
    /// The data to transfer
    pub data: Vec<u8>,
}

impl ITermFileData {
    fn parse(osc: &[&[u8]]) -> Result<Self> {
        let mut params = HashMap::new();

        // Unfortunately, the encoding for the file download data is
        // awkward to fit in the conventional OSC data that our parser
        // expects at a higher level.
        // We have a mix of '=', ';' and ':' separated keys and values,
        // and a number of them are optional.
        // ESC ] 1337 ; File = [optional arguments] : base-64 encoded file contents ^G

        let mut data = None;

        let last = osc.len() - 1;
        for (idx, s) in osc.iter().enumerate().skip(1) {
            let param = if idx == 1 {
                if s.len() >= 5 {
                    // skip over File=
                    &s[5..]
                } else {
                    bail!("failed to parse file data; File= not found");
                }
            } else {
                s
            };

            let param = if idx == last {
                // The final argument contains `:base64`, so look for that
                if let Some(colon) = param.iter().position(|c| *c == b':') {
                    data = Some(base64_decode(&param[colon + 1..])?);
                    &param[..colon]
                } else {
                    // If we don't find the colon in the last piece, we've
                    // got nothing useful
                    bail!("failed to parse file data; no colon found");
                }
            } else {
                param
            };

            // eg: `File=;size=1234` case. <https://github.com/wezterm/wezterm/issues/1291>
            if param.is_empty() {
                continue;
            }

            // look for k=v in param
            if let Some(equal) = param.iter().position(|c| *c == b'=') {
                let key = &param[..equal];
                let value = &param[equal + 1..];
                params.insert(str::from_utf8(key)?, str::from_utf8(value)?);
            } else if idx != last {
                bail!("failed to parse file data; no equals found");
            }
        }

        let name = params
            .get("name")
            .and_then(|s| base64_decode(s).ok())
            .and_then(|b| String::from_utf8(b).ok());
        let size = params.get("size").and_then(|s| s.parse().ok());
        let width = params
            .get("width")
            .and_then(|s| ITermDimension::parse(s).ok())
            .unwrap_or(ITermDimension::Automatic);
        let height = params
            .get("height")
            .and_then(|s| ITermDimension::parse(s).ok())
            .unwrap_or(ITermDimension::Automatic);
        let preserve_aspect_ratio = params
            .get("preserveAspectRatio")
            .map(|s| *s != "0")
            .unwrap_or(true);
        let inline = params.get("inline").map(|s| *s != "0").unwrap_or(false);
        let do_not_move_cursor = params
            .get("doNotMoveCursor")
            .map(|s| *s != "0")
            .unwrap_or(false);
        let data = data.ok_or_else(|| format!("didn't set data"))?;
        Ok(Self {
            name,
            size,
            width,
            height,
            preserve_aspect_ratio,
            inline,
            do_not_move_cursor,
            data,
        })
    }
}

impl Display for ITermFileData {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "File")?;
        let mut sep = "=";
        let emit_sep = |sep, f: &mut Formatter| -> std::result::Result<&str, FmtError> {
            write!(f, "{}", sep)?;
            Ok(";")
        };
        if let Some(size) = self.size {
            sep = emit_sep(sep, f)?;
            write!(f, "size={}", size)?;
        }
        if let Some(ref name) = self.name {
            sep = emit_sep(sep, f)?;
            write!(f, "name={}", base64_encode(name))?;
        }
        if self.width != ITermDimension::Automatic {
            sep = emit_sep(sep, f)?;
            write!(f, "width={}", self.width)?;
        }
        if self.height != ITermDimension::Automatic {
            sep = emit_sep(sep, f)?;
            write!(f, "height={}", self.height)?;
        }
        if !self.preserve_aspect_ratio {
            sep = emit_sep(sep, f)?;
            write!(f, "preserveAspectRatio=0")?;
        }
        if self.inline {
            sep = emit_sep(sep, f)?;
            write!(f, "inline=1")?;
        }
        if self.do_not_move_cursor {
            sep = emit_sep(sep, f)?;
            write!(f, "doNotMoveCursor=1")?;
        }
        // Ensure that we emit a sep if we didn't already.
        // It will still be set to '=' in that case.
        if sep == "=" {
            write!(f, "=")?;
        }
        write!(f, ":{}", base64_encode(&self.data))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ITermDimension {
    Automatic,
    Cells(i64),
    Pixels(i64),
    Percent(i64),
}

impl Default for ITermDimension {
    fn default() -> Self {
        Self::Automatic
    }
}

impl Display for ITermDimension {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        use self::ITermDimension::*;
        match self {
            Automatic => write!(f, "auto"),
            Cells(n) => write!(f, "{}", n),
            Pixels(n) => write!(f, "{}px", n),
            Percent(n) => write!(f, "{}%", n),
        }
    }
}

impl std::str::FromStr for ITermDimension {
    type Err = crate::vendored::termwiz::Error;
    fn from_str(s: &str) -> Result<Self> {
        ITermDimension::parse(s)
    }
}

impl ITermDimension {
    fn parse(s: &str) -> Result<Self> {
        if s == "auto" {
            Ok(ITermDimension::Automatic)
        } else if s.ends_with("px") {
            let s = &s[..s.len() - 2];
            let num = s.parse()?;
            Ok(ITermDimension::Pixels(num))
        } else if s.ends_with('%') {
            let s = &s[..s.len() - 1];
            let num = s.parse()?;
            Ok(ITermDimension::Percent(num))
        } else {
            let num = s.parse()?;
            Ok(ITermDimension::Cells(num))
        }
    }

    /// Convert the dimension into a number of pixels based on the provided
    /// size of a cell and number of cells in that dimension.
    /// Returns None for the Automatic variant.
    pub fn to_pixels(&self, cell_size: usize, num_cells: usize) -> Option<usize> {
        match self {
            ITermDimension::Automatic => None,
            ITermDimension::Cells(n) => Some((*n).max(0) as usize * cell_size),
            ITermDimension::Pixels(n) => Some((*n).max(0) as usize),
            ITermDimension::Percent(n) => Some(
                (((*n).max(0).min(100) as f32 / 100.0) * num_cells as f32 * cell_size as f32)
                    as usize,
            ),
        }
    }
}

impl ITermProprietary {
    #[allow(clippy::cognitive_complexity)]
    fn parse(osc: &[&[u8]]) -> Result<Self> {
        // iTerm has a number of different styles of OSC parameter
        // encodings, which makes this section of code a bit gnarly.
        ensure!(osc.len() > 1, "not enough args");

        let param = String::from_utf8_lossy(osc[1]);

        let mut iter = param.splitn(2, '=');
        let keyword = iter.next().ok_or_else(|| format!("bad params"))?;
        let p1 = iter.next();

        macro_rules! single {
            ($variant:ident, $text:expr) => {
                if osc.len() == 2 && keyword == $text && p1.is_none() {
                    return Ok(ITermProprietary::$variant);
                }
            };
        }

        macro_rules! one_str {
            ($variant:ident, $text:expr) => {
                if osc.len() == 2 && keyword == $text {
                    if let Some(p1) = p1 {
                        return Ok(ITermProprietary::$variant(p1.into()));
                    }
                }
            };
        }
        macro_rules! const_arg {
            ($variant:ident, $text:expr, $value:expr, $res:expr) => {
                if osc.len() == 2 && keyword == $text {
                    if let Some(p1) = p1 {
                        if p1 == $value {
                            return Ok(ITermProprietary::$variant($res));
                        }
                    }
                }
            };
        }

        single!(SetMark, "SetMark");
        single!(StealFocus, "StealFocus");
        single!(ClearScrollback, "ClearScrollback");
        single!(EndCopy, "EndCopy");
        single!(RequestCellSize, "ReportCellSize");
        const_arg!(HighlightCursorLine, "HighlightCursorLine", "yes", true);
        const_arg!(HighlightCursorLine, "HighlightCursorLine", "no", false);
        one_str!(CurrentDir, "CurrentDir");
        one_str!(SetProfile, "SetProfile");
        one_str!(CopyToClipboard, "CopyToClipboard");

        let p1_empty = match p1 {
            Some(p1) if p1 == "" => true,
            None => true,
            _ => false,
        };

        if osc.len() == 3 && keyword == "Copy" && p1_empty {
            return Ok(ITermProprietary::Copy(String::from_utf8(base64_decode(
                osc[2],
            )?)?));
        }
        if osc.len() == 3 && keyword == "SetBadgeFormat" && p1_empty {
            return Ok(ITermProprietary::SetBadgeFormat(String::from_utf8(
                base64_decode(osc[2])?,
            )?));
        }

        if osc.len() == 3 && keyword == "ReportCellSize" && p1.is_some() {
            if let Some(p1) = p1 {
                return Ok(ITermProprietary::ReportCellSize {
                    height_pixels: NotNan::new(p1.parse()?)?,
                    width_pixels: NotNan::new(String::from_utf8_lossy(osc[2]).parse()?)?,
                    scale: None,
                });
            }
        }
        if osc.len() == 4 && keyword == "ReportCellSize" && p1.is_some() {
            if let Some(p1) = p1 {
                return Ok(ITermProprietary::ReportCellSize {
                    height_pixels: NotNan::new(p1.parse()?)?,
                    width_pixels: NotNan::new(String::from_utf8_lossy(osc[2]).parse()?)?,
                    scale: Some(NotNan::new(String::from_utf8_lossy(osc[3]).parse()?)?),
                });
            }
        }

        if osc.len() == 2 && keyword == "SetUserVar" {
            if let Some(p1) = p1 {
                let mut iter = p1.splitn(2, '=');
                let p1 = iter.next();
                let p2 = iter.next();

                if let (Some(k), Some(v)) = (p1, p2) {
                    return Ok(ITermProprietary::SetUserVar {
                        name: k.to_string(),
                        value: String::from_utf8(base64_decode(v)?)?,
                    });
                }
            }
        }

        if osc.len() == 2 && keyword == "UnicodeVersion" {
            if let Some(p1) = p1 {
                let mut iter = p1.splitn(2, ' ');
                let keyword = iter.next();
                let label = iter.next();

                if let Some("push") = keyword {
                    return Ok(ITermProprietary::UnicodeVersion(
                        ITermUnicodeVersionOp::Push(label.map(|s| s.to_string())),
                    ));
                }
                if let Some("pop") = keyword {
                    return Ok(ITermProprietary::UnicodeVersion(
                        ITermUnicodeVersionOp::Pop(label.map(|s| s.to_string())),
                    ));
                }

                if let Ok(n) = p1.parse::<u8>() {
                    return Ok(ITermProprietary::UnicodeVersion(
                        ITermUnicodeVersionOp::Set(n),
                    ));
                }
            }
        }

        if keyword == "File" {
            return Ok(ITermProprietary::File(Box::new(ITermFileData::parse(osc)?)));
        }

        bail!("ITermProprietary {:?}", osc);
    }
}

/// base64::encode is deprecated, so make a less frustrating helper
pub(crate) fn base64_encode<T: AsRef<[u8]>>(s: T) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

/// base64::decode is deprecated, so make a less frustrating helper
pub(crate) fn base64_decode<T: AsRef<[u8]>>(
    s: T,
) -> std::result::Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::{GeneralPurpose, GeneralPurposeConfig};
    GeneralPurpose::new(
        &base64::alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_allow_trailing_bits(true),
    )
    .decode(s)
}

impl Display for ITermProprietary {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        write!(f, "1337;")?;
        use self::ITermProprietary::*;
        match self {
            SetMark => write!(f, "SetMark")?,
            StealFocus => write!(f, "StealFocus")?,
            ClearScrollback => write!(f, "ClearScrollback")?,
            CurrentDir(s) => write!(f, "CurrentDir={}", s)?,
            SetProfile(s) => write!(f, "SetProfile={}", s)?,
            CopyToClipboard(s) => write!(f, "CopyToClipboard={}", s)?,
            EndCopy => write!(f, "EndCopy")?,
            HighlightCursorLine(yes) => {
                write!(f, "HighlightCursorLine={}", if *yes { "yes" } else { "no" })?
            },
            RequestCellSize => write!(f, "ReportCellSize")?,
            ReportCellSize {
                height_pixels,
                width_pixels,
                scale: None,
            } => write!(f, "ReportCellSize={height_pixels:.1};{width_pixels:.1}")?,
            ReportCellSize {
                height_pixels,
                width_pixels,
                scale: Some(scale),
            } => write!(
                f,
                "ReportCellSize={height_pixels:.1};{width_pixels:.1};{scale:.1}",
            )?,
            Copy(s) => write!(f, "Copy=;{}", base64_encode(s))?,
            ReportVariable(s) => write!(f, "ReportVariable={}", base64_encode(s))?,
            SetUserVar { name, value } => {
                write!(f, "SetUserVar={}={}", name, base64_encode(value))?
            },
            SetBadgeFormat(s) => write!(f, "SetBadgeFormat={}", base64_encode(s))?,
            File(file) => file.fmt(f)?,
            UnicodeVersion(ITermUnicodeVersionOp::Set(n)) => write!(f, "UnicodeVersion={}", n)?,
            UnicodeVersion(ITermUnicodeVersionOp::Push(Some(label))) => {
                write!(f, "UnicodeVersion=push {}", label)?
            },
            UnicodeVersion(ITermUnicodeVersionOp::Push(None)) => write!(f, "UnicodeVersion=push")?,
            UnicodeVersion(ITermUnicodeVersionOp::Pop(Some(label))) => {
                write!(f, "UnicodeVersion=pop {}", label)?
            },
            UnicodeVersion(ITermUnicodeVersionOp::Pop(None)) => write!(f, "UnicodeVersion=pop")?,
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode(osc: &OperatingSystemCommand) -> String {
        format!("{}", osc)
    }

    fn parse(osc: &[&str], expected: &str) -> OperatingSystemCommand {
        let mut v = Vec::new();
        for s in osc {
            v.push(s.as_bytes());
        }
        let result = OperatingSystemCommand::parse(&v);

        assert_eq!(encode(&result), expected);

        result
    }

    #[test]
    fn reset_colors() {
        assert_eq!(
            parse(&["104"], "\x1b]104\x1b\\"),
            OperatingSystemCommand::ResetColors(vec![])
        );
        assert_eq!(
            parse(&["104", ""], "\x1b]104\x1b\\"),
            OperatingSystemCommand::ResetColors(vec![])
        );
        assert_eq!(
            parse(&["104", "1"], "\x1b]104;1\x1b\\"),
            OperatingSystemCommand::ResetColors(vec![1])
        );
        assert_eq!(
            parse(&["112"], "\x1b]112\x1b\\"),
            OperatingSystemCommand::ResetDynamicColor(DynamicColorNumber::TextCursorColor)
        );
    }

    #[test]
    fn title() {
        assert_eq!(
            parse(&["0", "hello"], "\x1b]0;hello\x1b\\"),
            OperatingSystemCommand::SetIconNameAndWindowTitle("hello".into())
        );

        assert_eq!(
            parse(&["0", "hello \u{1f915}"], "\x1b]0;hello \u{1f915}\x1b\\"),
            OperatingSystemCommand::SetIconNameAndWindowTitle("hello \u{1f915}".into())
        );

        assert_eq!(
            parse(
                &["0", "hello \u{1f915}", " world"],
                "\x1b]0;hello \u{1f915}; world\x1b\\"
            ),
            OperatingSystemCommand::SetIconNameAndWindowTitle("hello \u{1f915}; world".into())
        );

        // Missing title parameter
        assert_eq!(
            parse(&["0"], "\x1b]0\x1b\\"),
            OperatingSystemCommand::Unspecified(vec![b"0".to_vec()])
        );

        // parsing legacy sun OSC; why bother? This format is used in response
        // to the CSI ReportWindowTitle sequence
        assert_eq!(
            parse(&["lhello"], "\x1b]lhello\x1b\\"),
            OperatingSystemCommand::SetWindowTitleSun("hello".into())
        );
    }

    #[test]
    fn hyperlink() {
        assert_eq!(
            parse(
                &["8", "id=foo", "http://example.com"],
                "\x1b]8;id=foo;http://example.com\x1b\\"
            ),
            OperatingSystemCommand::SetHyperlink(Some(Hyperlink::new_with_id(
                "http://example.com",
                "foo"
            )))
        );

        assert_eq!(
            parse(&["8", "", ""], "\x1b]8;;\x1b\\"),
            OperatingSystemCommand::SetHyperlink(None)
        );

        // too many params
        assert_eq!(
            parse(&["8", "1", "2"], "\x1b]8;1;2\x1b\\"),
            OperatingSystemCommand::Unspecified(vec![b"8".to_vec(), b"1".to_vec(), b"2".to_vec()])
        );

        assert_eq!(
            Hyperlink::parse(&[b"8", b"", b"x"]).unwrap(),
            Some(Hyperlink::new("x"))
        );
    }

    #[test]
    fn finalterm() {
        assert_eq!(
            parse(&["133", "L"], "\x1b]133;L\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::FreshLine)
        );
        assert_eq!(
            parse(&["133", "C"], "\x1b]133;C\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfInputAndStartOfOutput { aid: None }
            )
        );

        assert_eq!(
            parse(&["133", "C", "aid=123"], "\x1b]133;C;aid=123\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfInputAndStartOfOutput {
                    aid: Some("123".to_string())
                }
            )
        );

        assert_eq!(
            parse(&["133", "D", "1"], "\x1b]133;D;1\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::CommandStatus {
                    status: 1,
                    aid: None
                }
            )
        );

        assert_eq!(
            parse(&["133", "D", "0"], "\x1b]133;D;0\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::CommandStatus {
                    status: 0,
                    aid: None
                }
            )
        );

        assert_eq!(
            parse(
                &["133", "D", "0", "aid=23"],
                "\x1b]133;D;0;err=0;aid=23\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::CommandStatus {
                    status: 0,
                    aid: Some("23".to_owned())
                }
            )
        );

        assert_eq!(
            parse(
                &["133", "D", "1", "aid=23"],
                "\x1b]133;D;1;err=1;aid=23\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::CommandStatus {
                    status: 1,
                    aid: Some("23".to_owned())
                }
            )
        );

        assert_eq!(
            parse(&["133", "P"], "\x1b]133;P;k=i\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::StartPrompt(
                FinalTermPromptKind::Initial
            ))
        );

        assert_eq!(
            parse(&["133", "P", "k=i"], "\x1b]133;P;k=i\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::StartPrompt(
                FinalTermPromptKind::Initial
            ))
        );

        assert_eq!(
            parse(&["133", "P", "k=r"], "\x1b]133;P;k=r\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::StartPrompt(
                FinalTermPromptKind::RightSide
            ))
        );

        assert_eq!(
            parse(&["133", "P", "k=c"], "\x1b]133;P;k=c\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::StartPrompt(
                FinalTermPromptKind::Continuation
            ))
        );
        assert_eq!(
            parse(&["133", "P", "k=s"], "\x1b]133;P;k=s\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(FinalTermSemanticPrompt::StartPrompt(
                FinalTermPromptKind::Secondary
            ))
        );

        assert_eq!(
            parse(&["133", "B"], "\x1b]133;B\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfPromptAndStartOfInputUntilNextMarker
            ),
        );

        assert_eq!(
            parse(&["133", "I"], "\x1b]133;I\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfPromptAndStartOfInputUntilEndOfLine
            ),
        );

        assert_eq!(
            parse(&["133", "N"], "\x1b]133;N\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: None,
                    cl: None,
                }
            ),
        );

        assert_eq!(
            parse(&["133", "N", "aid=12"], "\x1b]133;N;aid=12\x1b\\"),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: Some("12".to_owned()),
                    cl: None,
                }
            ),
        );

        assert_eq!(
            parse(
                &["133", "N", "aid=12", "cl=line"],
                "\x1b]133;N;aid=12;cl=line\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: Some("12".to_owned()),
                    cl: Some(FinalTermClick::Line),
                }
            ),
        );

        assert_eq!(
            parse(
                &["133", "N", "aid=12", "cl=m"],
                "\x1b]133;N;aid=12;cl=m\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: Some("12".to_owned()),
                    cl: Some(FinalTermClick::MultipleLine),
                }
            ),
        );

        assert_eq!(
            parse(
                &["133", "N", "aid=12", "cl=v"],
                "\x1b]133;N;aid=12;cl=v\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: Some("12".to_owned()),
                    cl: Some(FinalTermClick::ConservativeVertical),
                }
            ),
        );
        assert_eq!(
            parse(
                &["133", "N", "aid=12", "cl=w"],
                "\x1b]133;N;aid=12;cl=w\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::MarkEndOfCommandWithFreshLine {
                    aid: Some("12".to_owned()),
                    cl: Some(FinalTermClick::SmartVertical),
                }
            ),
        );

        assert_eq!(
            parse(
                &["133", "A", "aid=12", "cl=w"],
                "\x1b]133;A;aid=12;cl=w\x1b\\"
            ),
            OperatingSystemCommand::FinalTermSemanticPrompt(
                FinalTermSemanticPrompt::FreshLineAndStartPrompt {
                    aid: Some("12".to_owned()),
                    cl: Some(FinalTermClick::SmartVertical),
                }
            ),
        );
    }

    #[test]
    fn rxvt() {
        assert_eq!(
            parse(
                &["777", "notify", "alert user", "the tea is ready"],
                "\x1b]777;notify;alert user;the tea is ready\x1b\\"
            ),
            OperatingSystemCommand::RxvtExtension(vec![
                "notify".into(),
                "alert user".into(),
                "the tea is ready".into()
            ]),
        )
    }

    #[test]
    fn conemu() {
        assert_eq!(
            parse(&["9", "4", "1", "42"], "\x1b]9;4;1;42\x1b\\"),
            OperatingSystemCommand::ConEmuProgress(Progress::SetPercentage(42))
        );
        assert_eq!(
            parse(&["9", "4", "2", "64"], "\x1b]9;4;2;64\x1b\\"),
            OperatingSystemCommand::ConEmuProgress(Progress::SetError(64))
        );
        assert_eq!(
            parse(&["9", "4", "3"], "\x1b]9;4;3\x1b\\"),
            OperatingSystemCommand::ConEmuProgress(Progress::SetIndeterminate)
        );
        assert_eq!(
            parse(&["9", "4", "4"], "\x1b]9;4;4\x1b\\"),
            OperatingSystemCommand::ConEmuProgress(Progress::Paused)
        );
    }

    #[test]
    fn iterm() {
        assert_eq!(
            parse(&["1337", "SetMark"], "\x1b]1337;SetMark\x1b\\"),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::SetMark)
        );

        assert_eq!(
            parse(
                &["1337", "CurrentDir=woot"],
                "\x1b]1337;CurrentDir=woot\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::CurrentDir("woot".into()))
        );

        assert_eq!(
            parse(
                &["1337", "HighlightCursorLine=yes"],
                "\x1b]1337;HighlightCursorLine=yes\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::HighlightCursorLine(true))
        );

        assert_eq!(
            parse(
                &["1337", "Copy=", "aGVsbG8="],
                "\x1b]1337;Copy=;aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::Copy("hello".into()))
        );

        assert_eq!(
            parse(
                &["1337", "SetUserVar=foo=aGVsbG8="],
                "\x1b]1337;SetUserVar=foo=aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::SetUserVar {
                name: "foo".into(),
                value: "hello".into()
            })
        );

        assert_eq!(
            parse(
                &["1337", "SetBadgeFormat=", "aGVsbG8="],
                "\x1b]1337;SetBadgeFormat=aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::SetBadgeFormat(
                "hello".into()
            ))
        );

        assert_eq!(
            parse(
                &["1337", "ReportCellSize=12.0", "15.5"],
                "\x1b]1337;ReportCellSize=12.0;15.5\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::ReportCellSize {
                height_pixels: NotNan::new(12.0).unwrap(),
                width_pixels: NotNan::new(15.5).unwrap(),
                scale: None,
            })
        );

        assert_eq!(
            parse(
                &["1337", "ReportCellSize=12.0", "15.5", "2.0"],
                "\x1b]1337;ReportCellSize=12.0;15.5;2.0\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::ReportCellSize {
                height_pixels: NotNan::new(12.0).unwrap(),
                width_pixels: NotNan::new(15.5).unwrap(),
                scale: Some(NotNan::new(2.0).unwrap()),
            })
        );

        assert_eq!(
            parse(
                &["1337", "File=:aGVsbG8="],
                "\x1b]1337;File=:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: None,
                    size: None,
                    width: ITermDimension::Automatic,
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=name=bXluYW1l:aGVsbG8="],
                "\x1b]1337;File=name=bXluYW1l:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: None,
                    width: ITermDimension::Automatic,
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=size=123", "name=bXluYW1l:aGVsbG8="],
                "\x1b]1337;File=size=123;name=bXluYW1l:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(123),
                    width: ITermDimension::Automatic,
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=name=bXluYW1l", "size=234:aGVsbG8="],
                "\x1b]1337;File=size=234;name=bXluYW1l:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Automatic,
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &[
                    "1337",
                    "File=name=bXluYW1l",
                    "width=auto",
                    "size=234:aGVsbG8="
                ],
                "\x1b]1337;File=size=234;name=bXluYW1l:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Automatic,
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=name=bXluYW1l", "width=5", "size=234:aGVsbG8="],
                "\x1b]1337;File=size=234;name=bXluYW1l;width=5:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Cells(5),
                    height: ITermDimension::Automatic,
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &[
                    "1337",
                    "File=name=bXluYW1l",
                    "width=5",
                    "height=10%",
                    "size=234:aGVsbG8="
                ],
                "\x1b]1337;File=size=234;name=bXluYW1l;width=5;height=10%:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Cells(5),
                    height: ITermDimension::Percent(10),
                    preserve_aspect_ratio: true,
                    inline: false,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=name=bXluYW1l", "preserveAspectRatio=0", "width=5", "inline=1", "height=10px","size=234:aGVsbG8="],
                "\x1b]1337;File=size=234;name=bXluYW1l;width=5;height=10px;preserveAspectRatio=0;inline=1:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Cells(5),
                    height: ITermDimension::Pixels(10),
                    preserve_aspect_ratio: false,
                    inline: true,
                    do_not_move_cursor: false,
                    data: b"hello".to_vec(),
                }
            )))
        );

        assert_eq!(
            parse(
                &["1337", "File=name=bXluYW1l", "preserveAspectRatio=0", "width=5", "inline=1", "doNotMoveCursor=1", "height=10px","size=234:aGVsbG8="],
                "\x1b]1337;File=size=234;name=bXluYW1l;width=5;height=10px;preserveAspectRatio=0;inline=1;doNotMoveCursor=1:aGVsbG8=\x1b\\"
            ),
            OperatingSystemCommand::ITermProprietary(ITermProprietary::File(Box::new(
                ITermFileData {
                    name: Some("myname".into()),
                    size: Some(234),
                    width: ITermDimension::Cells(5),
                    height: ITermDimension::Pixels(10),
                    preserve_aspect_ratio: false,
                    inline: true,
                    do_not_move_cursor: true,
                    data: b"hello".to_vec(),
                }
            )))
        );
    }
}
