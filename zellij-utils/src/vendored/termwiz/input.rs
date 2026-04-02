//! This module provides an InputParser struct to help with parsing
//! input received from a terminal.
use crate::vendored::termwiz::keymap::{Found, KeyMap};
use crate::vendored::termwiz::readbuf::ReadBuffer;
use bitflags::bitflags;
use std::fmt::Write;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers: u16 {
        const NONE = 0;
        const SHIFT = 1 << 1;
        const ALT = 1 << 2;
        const CTRL = 1 << 3;
        const SUPER = 1 << 4;
        const LEFT_ALT = 1 << 5;
        const RIGHT_ALT = 1 << 6;
        const LEADER = 1 << 7;
        const LEFT_CTRL = 1 << 8;
        const RIGHT_CTRL = 1 << 9;
        const LEFT_SHIFT = 1 << 10;
        const RIGHT_SHIFT = 1 << 11;
        const ENHANCED_KEY = 1 << 12;
    }
}

impl Modifiers {
    pub fn encode_xterm(self) -> u8 {
        let mut number = 0;
        if self.contains(Self::SHIFT) {
            number |= 1;
        }
        if self.contains(Self::ALT) {
            number |= 2;
        }
        if self.contains(Self::CTRL) {
            number |= 4;
        }
        number
    }

    pub fn remove_positional_mods(self) -> Self {
        self - (Self::LEFT_ALT
            | Self::RIGHT_ALT
            | Self::LEFT_CTRL
            | Self::RIGHT_CTRL
            | Self::LEFT_SHIFT
            | Self::RIGHT_SHIFT
            | Self::ENHANCED_KEY)
    }
}

bitflags! {
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct KittyKeyboardFlags: u16 {
        const NONE = 0;
        const DISAMBIGUATE_ESCAPE_CODES = 1;
        const REPORT_EVENT_TYPES = 2;
        const REPORT_ALTERNATE_KEYS = 4;
        const REPORT_ALL_KEYS_AS_ESCAPE_CODES = 8;
        const REPORT_ASSOCIATED_TEXT = 16;
    }
}

pub fn ctrl_mapping(c: char) -> Option<char> {
    Some(match c {
        '@' | '`' | ' ' | '2' => '\x00',
        'A' | 'a' => '\x01',
        'B' | 'b' => '\x02',
        'C' | 'c' => '\x03',
        'D' | 'd' => '\x04',
        'E' | 'e' => '\x05',
        'F' | 'f' => '\x06',
        'G' | 'g' => '\x07',
        'H' | 'h' => '\x08',
        'I' | 'i' => '\x09',
        'J' | 'j' => '\x0a',
        'K' | 'k' => '\x0b',
        'L' | 'l' => '\x0c',
        'M' | 'm' => '\x0d',
        'N' | 'n' => '\x0e',
        'O' | 'o' => '\x0f',
        'P' | 'p' => '\x10',
        'Q' | 'q' => '\x11',
        'R' | 'r' => '\x12',
        'S' | 's' => '\x13',
        'T' | 't' => '\x14',
        'U' | 'u' => '\x15',
        'V' | 'v' => '\x16',
        'W' | 'w' => '\x17',
        'X' | 'x' => '\x18',
        'Y' | 'y' => '\x19',
        'Z' | 'z' => '\x1a',
        '[' | '3' | '{' => '\x1b',
        '\\' | '4' | '|' => '\x1c',
        ']' | '5' | '}' => '\x1d',
        '^' | '6' | '~' => '\x1e',
        '_' | '7' | '/' => '\x1f',
        '8' | '?' => '\x7f',
        _ => return None,
    })
}

bitflags! {
    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct MouseButtons: u8 {
        const NONE = 0;
        const LEFT = 1<<1;
        const RIGHT = 1<<2;
        const MIDDLE = 1<<3;
        const VERT_WHEEL = 1<<4;
        const HORZ_WHEEL = 1<<5;
        /// if set then the wheel movement was in the positive
        /// direction, else the negative direction
        const WHEEL_POSITIVE = 1<<6;
    }
}

pub const CSI: &str = "\x1b[";
pub const SS3: &str = "\x1bO";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    Key(KeyEvent),
    Mouse(MouseEvent),
    PixelMouse(PixelMouseEvent),
    /// Detected that the user has resized the terminal
    Resized {
        cols: usize,
        rows: usize,
    },
    /// For terminals that support Bracketed Paste mode,
    /// pastes are collected and reported as this variant.
    Paste(String),
    /// The program has woken the input thread.
    Wake,
    /// An Operating System Command sequence was received.
    /// Contains the raw payload between \x1b] and the terminator.
    OperatingSystemCommand(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MouseEvent {
    pub x: u16,
    pub y: u16,
    pub mouse_buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelMouseEvent {
    pub x_pixels: u16,
    pub y_pixels: u16,
    pub mouse_buttons: MouseButtons,
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Which key was pressed
    pub key: KeyCode,
    /// Which modifiers are down
    pub modifiers: Modifiers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardEncoding {
    Xterm,
    /// <http://www.leonerd.org.uk/hacks/fixterms/>
    CsiU,
    /// <https://github.com/microsoft/terminal/blob/main/doc/specs/%234999%20-%20Improved%20keyboard%20handling%20in%20Conpty.md>
    Win32,
    /// <https://sw.kovidgoyal.net/kitty/keyboard-protocol/>
    Kitty(KittyKeyboardFlags),
}

/// Specifies terminal modes/configuration that can influence how a KeyCode
/// is encoded when being sent to and application via the pty.
#[derive(Debug, Clone, Copy)]
pub struct KeyCodeEncodeModes {
    pub encoding: KeyboardEncoding,
    pub application_cursor_keys: bool,
    pub newline_mode: bool,
    pub modify_other_keys: Option<i64>,
}

/// Which key is pressed.  Not all of these are probable to appear
/// on most systems.  A lot of this list is @wez trawling docs and
/// making an entry for things that might be possible in this first pass.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// The decoded unicode character
    Char(char),

    Hyper,
    Super,
    Meta,

    /// Ctrl-break on windows
    Cancel,
    Backspace,
    Tab,
    Clear,
    Enter,
    Shift,
    Escape,
    LeftShift,
    RightShift,
    Control,
    LeftControl,
    RightControl,
    Alt,
    LeftAlt,
    RightAlt,
    Menu,
    LeftMenu,
    RightMenu,
    Pause,
    CapsLock,
    PageUp,
    PageDown,
    End,
    Home,
    LeftArrow,
    RightArrow,
    UpArrow,
    DownArrow,
    Select,
    Print,
    Execute,
    PrintScreen,
    Insert,
    Delete,
    Help,
    LeftWindows,
    RightWindows,
    Applications,
    Sleep,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    Multiply,
    Add,
    Separator,
    Subtract,
    Decimal,
    Divide,
    /// F1-F24 are possible
    Function(u8),
    NumLock,
    ScrollLock,
    Copy,
    Cut,
    Paste,
    BrowserBack,
    BrowserForward,
    BrowserRefresh,
    BrowserStop,
    BrowserSearch,
    BrowserFavorites,
    BrowserHome,
    VolumeMute,
    VolumeDown,
    VolumeUp,
    MediaNextTrack,
    MediaPrevTrack,
    MediaStop,
    MediaPlayPause,
    ApplicationLeftArrow,
    ApplicationRightArrow,
    ApplicationUpArrow,
    ApplicationDownArrow,
    KeyPadHome,
    KeyPadEnd,
    KeyPadPageUp,
    KeyPadPageDown,
    KeyPadBegin,

    #[doc(hidden)]
    InternalPasteStart,
    #[doc(hidden)]
    InternalPasteEnd,
}

impl KeyCode {
    /// if SHIFT is held and we have KeyCode::Char('c') we want to normalize
    /// that keycode to KeyCode::Char('C'); that is what this function does.
    pub fn normalize_shift_to_upper_case(self, modifiers: Modifiers) -> KeyCode {
        if modifiers.contains(Modifiers::SHIFT) {
            match self {
                KeyCode::Char(c) if c.is_ascii_lowercase() => KeyCode::Char(c.to_ascii_uppercase()),
                _ => self,
            }
        } else {
            self
        }
    }

    /// Return true if the key represents a modifier key.
    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            Self::Hyper
                | Self::Super
                | Self::Meta
                | Self::Shift
                | Self::LeftShift
                | Self::RightShift
                | Self::Control
                | Self::LeftControl
                | Self::RightControl
                | Self::Alt
                | Self::LeftAlt
                | Self::RightAlt
                | Self::LeftWindows
                | Self::RightWindows
        )
    }

    /// Returns the byte sequence that represents this KeyCode and Modifier combination.
    pub fn encode(
        &self,
        mods: Modifiers,
        modes: KeyCodeEncodeModes,
        is_down: bool,
    ) -> Result<String> {
        if !is_down {
            // We only want down events
            return Ok(String::new());
        }
        // We are encoding the key as an xterm-compatible sequence, which does not support
        // positional modifiers.
        let mods = mods.remove_positional_mods();

        use KeyCode::*;

        let key = self.normalize_shift_to_upper_case(mods);
        // Normalize the modifier state for Char's that are uppercase; remove
        // the SHIFT modifier so that reduce ambiguity below
        let mods = match key {
            Char(c)
                if (c.is_ascii_punctuation() || c.is_ascii_uppercase())
                    && mods.contains(Modifiers::SHIFT) =>
            {
                mods & !Modifiers::SHIFT
            },
            _ => mods,
        };

        // Normalize Backspace and Delete
        let key = match key {
            Char('\x7f') => Delete,
            Char('\x08') => Backspace,
            c => c,
        };

        let mut buf = String::new();

        // TODO: also respect self.application_keypad

        match key {
            Char(c)
                if is_ambiguous_ascii_ctrl(c)
                    && mods.contains(Modifiers::CTRL)
                    && modes.encoding == KeyboardEncoding::CsiU =>
            {
                csi_u_encode(&mut buf, c, mods, &modes)?;
            },
            Char(c) if c.is_ascii_uppercase() && mods.contains(Modifiers::CTRL) => {
                csi_u_encode(&mut buf, c, mods, &modes)?;
            },

            Char(c) if mods.contains(Modifiers::CTRL) && modes.modify_other_keys == Some(2) => {
                csi_u_encode(&mut buf, c, mods, &modes)?;
            },
            Char(c) if mods.contains(Modifiers::CTRL) && ctrl_mapping(c).is_some() => {
                let c = ctrl_mapping(c).unwrap();
                if mods.contains(Modifiers::ALT) {
                    buf.push(0x1b as char);
                }
                buf.push(c);
            },

            // When alt is pressed, send escape first to indicate to the peer that
            // ALT is pressed.  We do this only for ascii alnum characters because
            // eg: on macOS generates altgr style glyphs and keeps the ALT key
            // in the modifier set.  This confuses eg: zsh which then just displays
            // <fffffffff> as the input, so we want to avoid that.
            Char(c)
                if (c.is_ascii_alphanumeric() || c.is_ascii_punctuation())
                    && mods.contains(Modifiers::ALT) =>
            {
                buf.push(0x1b as char);
                buf.push(c);
            },

            Backspace => {
                // Backspace sends the default VERASE which is confusingly
                // the DEL ascii codepoint rather than BS.
                // We only send BS when CTRL is held.
                if mods.contains(Modifiers::CTRL) {
                    csi_u_encode(&mut buf, '\x08', mods, &modes)?;
                } else if mods.contains(Modifiers::SHIFT) {
                    csi_u_encode(&mut buf, '\x7f', mods, &modes)?;
                } else {
                    if mods.contains(Modifiers::ALT) {
                        buf.push(0x1b as char);
                    }
                    buf.push('\x7f');
                }
            },

            Enter | Escape => {
                let c = match key {
                    Enter => '\r',
                    Escape => '\x1b',
                    _ => unreachable!(),
                };
                if mods.contains(Modifiers::SHIFT) || mods.contains(Modifiers::CTRL) {
                    csi_u_encode(&mut buf, c, mods, &modes)?;
                } else {
                    if mods.contains(Modifiers::ALT) {
                        buf.push(0x1b as char);
                    }
                    buf.push(c);
                    if modes.newline_mode && key == Enter {
                        buf.push(0x0a as char);
                    }
                }
            },

            Tab if !mods.is_empty() && modes.modify_other_keys.is_some() => {
                csi_u_encode(&mut buf, '\t', mods, &modes)?;
            },

            Tab => {
                if mods.contains(Modifiers::ALT) {
                    buf.push(0x1b as char);
                }
                let mods = mods & !Modifiers::ALT;
                if mods == Modifiers::CTRL {
                    buf.push_str("\x1b[9;5u");
                } else if mods == Modifiers::CTRL | Modifiers::SHIFT {
                    buf.push_str("\x1b[1;5Z");
                } else if mods == Modifiers::SHIFT {
                    buf.push_str("\x1b[Z");
                } else {
                    buf.push('\t');
                }
            },

            Char(c) => {
                if mods.is_empty() {
                    buf.push(c);
                } else {
                    csi_u_encode(&mut buf, c, mods, &modes)?;
                }
            },

            Home
            | KeyPadHome
            | End
            | KeyPadEnd
            | UpArrow
            | DownArrow
            | RightArrow
            | LeftArrow
            | ApplicationUpArrow
            | ApplicationDownArrow
            | ApplicationRightArrow
            | ApplicationLeftArrow => {
                let (force_app, c) = match key {
                    UpArrow => (false, 'A'),
                    DownArrow => (false, 'B'),
                    RightArrow => (false, 'C'),
                    LeftArrow => (false, 'D'),
                    KeyPadHome | Home => (false, 'H'),
                    End | KeyPadEnd => (false, 'F'),
                    ApplicationUpArrow => (true, 'A'),
                    ApplicationDownArrow => (true, 'B'),
                    ApplicationRightArrow => (true, 'C'),
                    ApplicationLeftArrow => (true, 'D'),
                    _ => unreachable!(),
                };

                let csi_or_ss3 = if force_app || modes.application_cursor_keys {
                    // Use SS3 in application mode
                    SS3
                } else {
                    // otherwise use regular CSI
                    CSI
                };

                if mods.contains(Modifiers::ALT)
                    || mods.contains(Modifiers::SHIFT)
                    || mods.contains(Modifiers::CTRL)
                {
                    write!(buf, "{}1;{}{}", CSI, 1 + mods.encode_xterm(), c)?;
                } else {
                    write!(buf, "{}{}", csi_or_ss3, c)?;
                }
            },

            PageUp | PageDown | KeyPadPageUp | KeyPadPageDown | Insert | Delete => {
                let c = match key {
                    Insert => 2,
                    Delete => 3,
                    KeyPadPageUp | PageUp => 5,
                    KeyPadPageDown | PageDown => 6,
                    _ => unreachable!(),
                };

                if mods.contains(Modifiers::ALT)
                    || mods.contains(Modifiers::SHIFT)
                    || mods.contains(Modifiers::CTRL)
                {
                    write!(buf, "\x1b[{};{}~", c, 1 + mods.encode_xterm())?;
                } else {
                    write!(buf, "\x1b[{}~", c)?;
                }
            },

            Function(n) => {
                if mods.is_empty() && n < 5 {
                    // F1-F4 are encoded using SS3 if there are no modifiers
                    write!(
                        buf,
                        "{}",
                        match n {
                            1 => "\x1bOP",
                            2 => "\x1bOQ",
                            3 => "\x1bOR",
                            4 => "\x1bOS",
                            _ => unreachable!("wat?"),
                        }
                    )?;
                } else if n < 5 {
                    // Special case for F1-F4 with modifiers
                    let code = match n {
                        1 => 'P',
                        2 => 'Q',
                        3 => 'R',
                        4 => 'S',
                        _ => unreachable!("wat?"),
                    };
                    write!(buf, "\x1b[1;{}{code}", 1 + mods.encode_xterm())?;
                } else {
                    // Higher numbered F-keys using CSI instead of SS3.
                    let intro = match n {
                        1 => "\x1b[11",
                        2 => "\x1b[12",
                        3 => "\x1b[13",
                        4 => "\x1b[14",
                        5 => "\x1b[15",
                        6 => "\x1b[17",
                        7 => "\x1b[18",
                        8 => "\x1b[19",
                        9 => "\x1b[20",
                        10 => "\x1b[21",
                        11 => "\x1b[23",
                        12 => "\x1b[24",
                        13 => "\x1b[25",
                        14 => "\x1b[26",
                        15 => "\x1b[28",
                        16 => "\x1b[29",
                        17 => "\x1b[31",
                        18 => "\x1b[32",
                        19 => "\x1b[33",
                        20 => "\x1b[34",
                        21 => "\x1b[42",
                        22 => "\x1b[43",
                        23 => "\x1b[44",
                        24 => "\x1b[45",
                        _ => return Err(format!("unhandled fkey number {}", n).into()),
                    };
                    let encoded_mods = mods.encode_xterm();
                    if encoded_mods == 0 {
                        // If no modifiers are held, don't send the modifier
                        // sequence, as the modifier encoding is a CSI-u extension.
                        write!(buf, "{}~", intro)?;
                    } else {
                        write!(buf, "{};{}~", intro, 1 + encoded_mods)?;
                    }
                }
            },

            Numpad0 | Numpad3 | Numpad9 | Decimal => {
                let intro = match key {
                    Numpad0 => "\x1b[2",
                    Numpad3 => "\x1b[6",
                    Numpad9 => "\x1b[6",
                    Decimal => "\x1b[3",
                    _ => unreachable!(),
                };

                let encoded_mods = mods.encode_xterm();
                if encoded_mods == 0 {
                    write!(buf, "{}~", intro)?;
                } else {
                    write!(buf, "{};{}~", intro, 1 + encoded_mods)?;
                }
            },

            Numpad1 | Numpad2 | Numpad4 | Numpad5 | KeyPadBegin | Numpad6 | Numpad7 | Numpad8 => {
                let c = match key {
                    Numpad1 => "F",
                    Numpad2 => "B",
                    Numpad4 => "D",
                    KeyPadBegin | Numpad5 => "E",
                    Numpad6 => "C",
                    Numpad7 => "H",
                    Numpad8 => "A",
                    _ => unreachable!(),
                };

                let encoded_mods = mods.encode_xterm();
                if encoded_mods == 0 {
                    write!(buf, "{}{}", CSI, c)?;
                } else {
                    write!(buf, "{}1;{}{}", CSI, 1 + encoded_mods, c)?;
                }
            },

            Multiply | Add | Separator | Subtract | Divide => {},

            // Modifier keys pressed on their own don't expand to anything
            Control | LeftControl | RightControl | Alt | LeftAlt | RightAlt | Menu | LeftMenu
            | RightMenu | Super | Hyper | Shift | LeftShift | RightShift | Meta | LeftWindows
            | RightWindows | NumLock | ScrollLock | Cancel | Clear | Pause | CapsLock | Select
            | Print | PrintScreen | Execute | Help | Applications | Sleep | Copy | Cut | Paste
            | BrowserBack | BrowserForward | BrowserRefresh | BrowserStop | BrowserSearch
            | BrowserFavorites | BrowserHome | VolumeMute | VolumeDown | VolumeUp
            | MediaNextTrack | MediaPrevTrack | MediaStop | MediaPlayPause | InternalPasteStart
            | InternalPasteEnd => {},
        };

        Ok(buf)
    }
}

/// characters that when masked for CTRL could be an ascii control character
/// or could be a key that a user legitimately wants to process in their
/// terminal application
fn is_ambiguous_ascii_ctrl(c: char) -> bool {
    matches!(c, 'i' | 'I' | 'm' | 'M' | '[' | '{' | '@')
}

fn is_ascii(c: char) -> bool {
    (c as u32) < 0x80
}

fn csi_u_encode(
    buf: &mut String,
    c: char,
    mods: Modifiers,
    modes: &KeyCodeEncodeModes,
) -> Result<()> {
    if modes.encoding == KeyboardEncoding::CsiU && is_ascii(c) {
        write!(buf, "\x1b[{};{}u", c as u32, 1 + mods.encode_xterm())?;
        return Ok(());
    }

    // <https://invisible-island.net/xterm/modified-keys.html>
    match (c, modes.modify_other_keys) {
        ('c' | 'd' | '\x1b' | '\x7f' | '\x08', Some(1)) => {
            // Exclude well-known keys from modifyOtherKeys mode 1
        },
        (c, Some(_)) => {
            write!(buf, "\x1b[27;{};{}~", 1 + mods.encode_xterm(), c as u32)?;
            return Ok(());
        },
        _ => {},
    }

    let c = if mods.contains(Modifiers::CTRL) && ctrl_mapping(c).is_some() {
        ctrl_mapping(c).unwrap()
    } else {
        c
    };
    if mods.contains(Modifiers::ALT) {
        buf.push(0x1b as char);
    }
    write!(buf, "{}", c)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MouseButton {
    Button1Press,
    Button1Release,
    Button1Drag,
    Button2Press,
    Button2Release,
    Button2Drag,
    Button3Press,
    Button3Release,
    Button3Drag,
    Button4Press,
    Button4Release,
    Button5Press,
    Button5Release,
    Button6Press,
    Button6Release,
    Button7Press,
    Button7Release,
    None,
}

fn decode_mouse_button(control: u8, p0: i64) -> Option<MouseButton> {
    match (control, p0 & 0b110_0011) {
        (b'M', 0) => Some(MouseButton::Button1Press),
        (b'm', 0) => Some(MouseButton::Button1Release),
        (b'M', 1) => Some(MouseButton::Button2Press),
        (b'm', 1) => Some(MouseButton::Button2Release),
        (b'M', 2) => Some(MouseButton::Button3Press),
        (b'm', 2) => Some(MouseButton::Button3Release),
        (b'M', 64) => Some(MouseButton::Button4Press),
        (b'm', 64) => Some(MouseButton::Button4Release),
        (b'M', 65) => Some(MouseButton::Button5Press),
        (b'm', 65) => Some(MouseButton::Button5Release),
        (b'M', 66) => Some(MouseButton::Button6Press),
        (b'm', 66) => Some(MouseButton::Button6Release),
        (b'M', 67) => Some(MouseButton::Button7Press),
        (b'm', 67) => Some(MouseButton::Button7Release),
        (b'M', 32) => Some(MouseButton::Button1Drag),
        (b'M', 33) => Some(MouseButton::Button2Drag),
        (b'M', 34) => Some(MouseButton::Button3Drag),
        (b'M', 35) | (b'm', 35) | (b'M', 3) | (b'm', 3) => Some(MouseButton::None),
        _ => ::core::option::Option::None,
    }
}

impl From<MouseButton> for MouseButtons {
    fn from(button: MouseButton) -> MouseButtons {
        match button {
            MouseButton::Button1Press | MouseButton::Button1Drag => MouseButtons::LEFT,
            MouseButton::Button2Press | MouseButton::Button2Drag => MouseButtons::MIDDLE,
            MouseButton::Button3Press | MouseButton::Button3Drag => MouseButtons::RIGHT,
            MouseButton::Button4Press => MouseButtons::VERT_WHEEL | MouseButtons::WHEEL_POSITIVE,
            MouseButton::Button5Press => MouseButtons::VERT_WHEEL,
            MouseButton::Button6Press => MouseButtons::HORZ_WHEEL | MouseButtons::WHEEL_POSITIVE,
            MouseButton::Button7Press => MouseButtons::HORZ_WHEEL,
            _ => MouseButtons::NONE,
        }
    }
}

fn decode_mouse_modifiers(p0: i64) -> Modifiers {
    let mut modifiers = Modifiers::NONE;
    if p0 & 4 != 0 {
        modifiers |= Modifiers::SHIFT;
    }
    if p0 & 8 != 0 {
        modifiers |= Modifiers::ALT;
    }
    if p0 & 16 != 0 {
        modifiers |= Modifiers::CTRL;
    }
    modifiers
}

/// Try to parse an SGR mouse sequence from the buffer.
/// Returns Some((InputEvent, bytes_consumed)) on success.
/// Returns None if the buffer does not contain a complete SGR mouse sequence.
fn parse_sgr_mouse(buf: &[u8]) -> Option<(InputEvent, usize)> {
    // Must start with \x1b[<
    if buf.len() < 6 || !buf.starts_with(b"\x1b[<") {
        return None;
    }
    let rest = &buf[3..]; // skip \x1b[<

    // Find the terminating M or m
    let term_pos = rest.iter().position(|&b| b == b'M' || b == b'm')?;
    let control = rest[term_pos];
    let params_str = std::str::from_utf8(&rest[..term_pos]).ok()?;

    // Parse three semicolon-separated integers
    let mut parts = params_str.splitn(3, ';');
    let p0: i64 = parts.next()?.parse().ok()?;
    let p1: i64 = parts.next()?.parse().ok()?;
    let p2: i64 = parts.next()?.parse().ok()?;

    let button = decode_mouse_button(control, p0)?;
    let modifiers = decode_mouse_modifiers(p0);
    let mouse_buttons: MouseButtons = button.into();

    let consumed = 3 + term_pos + 1; // \x1b[< + params + M/m

    Some((
        InputEvent::Mouse(MouseEvent {
            x: p1 as u16,
            y: p2 as u16,
            mouse_buttons,
            modifiers,
        }),
        consumed,
    ))
}

/// Attempt to parse an OSC (Operating System Command) sequence from the buffer.
/// Returns `Some((InputEvent::OperatingSystemCommand(payload), len))` if a complete
/// OSC sequence is found, where `payload` is the bytes between `\x1b]` and the
/// terminator, and `len` is the total number of bytes consumed.
/// Returns `None` if the buffer does not start with `\x1b]` or the sequence is incomplete.
fn parse_osc(buf: &[u8]) -> Option<(InputEvent, usize)> {
    // OSC sequences start with ESC ] (0x1b 0x5d)
    if buf.get(0) != Some(&0x1b) || buf.get(1) != Some(&b']') {
        return None;
    }
    let mut i = 2;
    while i < buf.len() {
        match buf.get(i) {
            Some(&0x07) => {
                // BEL terminator
                let payload = buf.get(2..i).unwrap_or_default().to_vec();
                return Some((InputEvent::OperatingSystemCommand(payload), i + 1));
            },
            Some(&0x1b) => {
                // Possible ST terminator (ESC \)
                if buf.get(i + 1) == Some(&b'\\') {
                    let payload = buf.get(2..i).unwrap_or_default().to_vec();
                    return Some((InputEvent::OperatingSystemCommand(payload), i + 2));
                }
                // Bare ESC inside OSC — malformed, but don't consume further
                return None;
            },
            Some(_) => {
                i += 1;
            },
            None => {
                // Should not happen since i < buf.len(), but handle gracefully
                return None;
            },
        }
    }
    None // incomplete — no terminator found yet
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputState {
    Normal,
    EscapeMaybeAlt,
    Pasting(usize),
}

#[derive(Debug)]
pub struct InputParser {
    key_map: KeyMap<InputEvent>,
    buf: ReadBuffer,
    state: InputState,
}

#[cfg(windows)]
mod windows {
    use super::*;
    use std;
    use winapi::um::wincon::{
        INPUT_RECORD, KEY_EVENT, KEY_EVENT_RECORD, MOUSE_EVENT, MOUSE_EVENT_RECORD,
        WINDOW_BUFFER_SIZE_EVENT, WINDOW_BUFFER_SIZE_RECORD,
    };
    use winapi::um::winuser;

    fn modifiers_from_ctrl_key_state(state: u32) -> Modifiers {
        use winapi::um::wincon::*;

        let mut mods = Modifiers::NONE;

        if (state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED)) != 0 {
            mods |= Modifiers::ALT;
        }

        if (state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED)) != 0 {
            mods |= Modifiers::CTRL;
        }

        if (state & SHIFT_PRESSED) != 0 {
            mods |= Modifiers::SHIFT;
        }

        mods
    }

    impl InputParser {
        fn decode_key_record<F: FnMut(InputEvent)>(
            &mut self,
            event: &KEY_EVENT_RECORD,
            callback: &mut F,
        ) {
            if event.bKeyDown == 0 {
                return;
            }

            let key_code = match std::char::from_u32(*unsafe { event.uChar.UnicodeChar() } as u32) {
                Some(unicode) if unicode > '\x00' => {
                    let mut buf = [0u8; 4];
                    self.buf
                        .extend_with(unicode.encode_utf8(&mut buf).as_bytes());
                    self.process_bytes(callback, true);
                    return;
                },
                _ => match event.wVirtualKeyCode as i32 {
                    winuser::VK_CANCEL => KeyCode::Cancel,
                    winuser::VK_BACK => KeyCode::Backspace,
                    winuser::VK_TAB => KeyCode::Tab,
                    winuser::VK_CLEAR => KeyCode::Clear,
                    winuser::VK_RETURN => KeyCode::Enter,
                    winuser::VK_SHIFT => KeyCode::Shift,
                    winuser::VK_CONTROL => KeyCode::Control,
                    winuser::VK_MENU => KeyCode::Menu,
                    winuser::VK_PAUSE => KeyCode::Pause,
                    winuser::VK_CAPITAL => KeyCode::CapsLock,
                    winuser::VK_ESCAPE => KeyCode::Escape,
                    winuser::VK_PRIOR => KeyCode::PageUp,
                    winuser::VK_NEXT => KeyCode::PageDown,
                    winuser::VK_END => KeyCode::End,
                    winuser::VK_HOME => KeyCode::Home,
                    winuser::VK_LEFT => KeyCode::LeftArrow,
                    winuser::VK_RIGHT => KeyCode::RightArrow,
                    winuser::VK_UP => KeyCode::UpArrow,
                    winuser::VK_DOWN => KeyCode::DownArrow,
                    winuser::VK_SELECT => KeyCode::Select,
                    winuser::VK_PRINT => KeyCode::Print,
                    winuser::VK_EXECUTE => KeyCode::Execute,
                    winuser::VK_SNAPSHOT => KeyCode::PrintScreen,
                    winuser::VK_INSERT => KeyCode::Insert,
                    winuser::VK_DELETE => KeyCode::Delete,
                    winuser::VK_HELP => KeyCode::Help,
                    winuser::VK_LWIN => KeyCode::LeftWindows,
                    winuser::VK_RWIN => KeyCode::RightWindows,
                    winuser::VK_APPS => KeyCode::Applications,
                    winuser::VK_SLEEP => KeyCode::Sleep,
                    winuser::VK_NUMPAD0 => KeyCode::Numpad0,
                    winuser::VK_NUMPAD1 => KeyCode::Numpad1,
                    winuser::VK_NUMPAD2 => KeyCode::Numpad2,
                    winuser::VK_NUMPAD3 => KeyCode::Numpad3,
                    winuser::VK_NUMPAD4 => KeyCode::Numpad4,
                    winuser::VK_NUMPAD5 => KeyCode::Numpad5,
                    winuser::VK_NUMPAD6 => KeyCode::Numpad6,
                    winuser::VK_NUMPAD7 => KeyCode::Numpad7,
                    winuser::VK_NUMPAD8 => KeyCode::Numpad8,
                    winuser::VK_NUMPAD9 => KeyCode::Numpad9,
                    winuser::VK_MULTIPLY => KeyCode::Multiply,
                    winuser::VK_ADD => KeyCode::Add,
                    winuser::VK_SEPARATOR => KeyCode::Separator,
                    winuser::VK_SUBTRACT => KeyCode::Subtract,
                    winuser::VK_DECIMAL => KeyCode::Decimal,
                    winuser::VK_DIVIDE => KeyCode::Divide,
                    winuser::VK_F1 => KeyCode::Function(1),
                    winuser::VK_F2 => KeyCode::Function(2),
                    winuser::VK_F3 => KeyCode::Function(3),
                    winuser::VK_F4 => KeyCode::Function(4),
                    winuser::VK_F5 => KeyCode::Function(5),
                    winuser::VK_F6 => KeyCode::Function(6),
                    winuser::VK_F7 => KeyCode::Function(7),
                    winuser::VK_F8 => KeyCode::Function(8),
                    winuser::VK_F9 => KeyCode::Function(9),
                    winuser::VK_F10 => KeyCode::Function(10),
                    winuser::VK_F11 => KeyCode::Function(11),
                    winuser::VK_F12 => KeyCode::Function(12),
                    winuser::VK_F13 => KeyCode::Function(13),
                    winuser::VK_F14 => KeyCode::Function(14),
                    winuser::VK_F15 => KeyCode::Function(15),
                    winuser::VK_F16 => KeyCode::Function(16),
                    winuser::VK_F17 => KeyCode::Function(17),
                    winuser::VK_F18 => KeyCode::Function(18),
                    winuser::VK_F19 => KeyCode::Function(19),
                    winuser::VK_F20 => KeyCode::Function(20),
                    winuser::VK_F21 => KeyCode::Function(21),
                    winuser::VK_F22 => KeyCode::Function(22),
                    winuser::VK_F23 => KeyCode::Function(23),
                    winuser::VK_F24 => KeyCode::Function(24),
                    winuser::VK_NUMLOCK => KeyCode::NumLock,
                    winuser::VK_SCROLL => KeyCode::ScrollLock,
                    winuser::VK_LSHIFT => KeyCode::LeftShift,
                    winuser::VK_RSHIFT => KeyCode::RightShift,
                    winuser::VK_LCONTROL => KeyCode::LeftControl,
                    winuser::VK_RCONTROL => KeyCode::RightControl,
                    winuser::VK_LMENU => KeyCode::LeftMenu,
                    winuser::VK_RMENU => KeyCode::RightMenu,
                    winuser::VK_BROWSER_BACK => KeyCode::BrowserBack,
                    winuser::VK_BROWSER_FORWARD => KeyCode::BrowserForward,
                    winuser::VK_BROWSER_REFRESH => KeyCode::BrowserRefresh,
                    winuser::VK_BROWSER_STOP => KeyCode::BrowserStop,
                    winuser::VK_BROWSER_SEARCH => KeyCode::BrowserSearch,
                    winuser::VK_BROWSER_FAVORITES => KeyCode::BrowserFavorites,
                    winuser::VK_BROWSER_HOME => KeyCode::BrowserHome,
                    winuser::VK_VOLUME_MUTE => KeyCode::VolumeMute,
                    winuser::VK_VOLUME_DOWN => KeyCode::VolumeDown,
                    winuser::VK_VOLUME_UP => KeyCode::VolumeUp,
                    winuser::VK_MEDIA_NEXT_TRACK => KeyCode::MediaNextTrack,
                    winuser::VK_MEDIA_PREV_TRACK => KeyCode::MediaPrevTrack,
                    winuser::VK_MEDIA_STOP => KeyCode::MediaStop,
                    winuser::VK_MEDIA_PLAY_PAUSE => KeyCode::MediaPlayPause,
                    _ => return,
                },
            };
            let mut modifiers = modifiers_from_ctrl_key_state(event.dwControlKeyState);

            let key_code = key_code.normalize_shift_to_upper_case(modifiers);
            if let KeyCode::Char(c) = key_code {
                if c.is_ascii_uppercase() {
                    modifiers.remove(Modifiers::SHIFT);
                }
            }

            let input_event = InputEvent::Key(KeyEvent {
                key: key_code,
                modifiers,
            });
            for _ in 0..event.wRepeatCount {
                callback(input_event.clone());
            }
        }

        fn decode_mouse_record<F: FnMut(InputEvent)>(
            &self,
            event: &MOUSE_EVENT_RECORD,
            callback: &mut F,
        ) {
            use winapi::um::wincon::*;
            let mut buttons = MouseButtons::NONE;

            if (event.dwButtonState & FROM_LEFT_1ST_BUTTON_PRESSED) != 0 {
                buttons |= MouseButtons::LEFT;
            }
            if (event.dwButtonState & RIGHTMOST_BUTTON_PRESSED) != 0 {
                buttons |= MouseButtons::RIGHT;
            }
            if (event.dwButtonState & FROM_LEFT_2ND_BUTTON_PRESSED) != 0 {
                buttons |= MouseButtons::MIDDLE;
            }

            let modifiers = modifiers_from_ctrl_key_state(event.dwControlKeyState);

            if (event.dwEventFlags & MOUSE_WHEELED) != 0 {
                buttons |= MouseButtons::VERT_WHEEL;
                if (event.dwButtonState >> 8) != 0 {
                    buttons |= MouseButtons::WHEEL_POSITIVE;
                }
            } else if (event.dwEventFlags & MOUSE_HWHEELED) != 0 {
                buttons |= MouseButtons::HORZ_WHEEL;
                if (event.dwButtonState >> 8) != 0 {
                    buttons |= MouseButtons::WHEEL_POSITIVE;
                }
            }

            let mouse = InputEvent::Mouse(MouseEvent {
                x: event.dwMousePosition.X as u16,
                y: event.dwMousePosition.Y as u16,
                mouse_buttons: buttons,
                modifiers,
            });

            if (event.dwEventFlags & DOUBLE_CLICK) != 0 {
                callback(mouse.clone());
            }
            callback(mouse);
        }

        fn decode_resize_record<F: FnMut(InputEvent)>(
            &self,
            event: &WINDOW_BUFFER_SIZE_RECORD,
            callback: &mut F,
        ) {
            callback(InputEvent::Resized {
                rows: event.dwSize.Y as usize,
                cols: event.dwSize.X as usize,
            });
        }

        pub fn decode_input_records<F: FnMut(InputEvent)>(
            &mut self,
            records: &[INPUT_RECORD],
            callback: &mut F,
        ) {
            for record in records {
                match record.EventType {
                    KEY_EVENT => {
                        self.decode_key_record(unsafe { record.Event.KeyEvent() }, callback)
                    },
                    MOUSE_EVENT => {
                        self.decode_mouse_record(unsafe { record.Event.MouseEvent() }, callback)
                    },
                    WINDOW_BUFFER_SIZE_EVENT => self.decode_resize_record(
                        unsafe { record.Event.WindowBufferSizeEvent() },
                        callback,
                    ),
                    _ => {},
                }
            }
            self.process_bytes(callback, false);
        }
    }
}

impl Default for InputParser {
    fn default() -> Self {
        Self::new()
    }
}

impl InputParser {
    pub fn new() -> Self {
        Self {
            key_map: Self::build_basic_key_map(),
            buf: ReadBuffer::new(),
            state: InputState::Normal,
        }
    }

    fn build_basic_key_map() -> KeyMap<InputEvent> {
        let mut map = KeyMap::new();

        let modifier_combos = &[
            ("", Modifiers::NONE),
            (";1", Modifiers::NONE),
            (";2", Modifiers::SHIFT),
            (";3", Modifiers::ALT),
            (";4", Modifiers::ALT | Modifiers::SHIFT),
            (";5", Modifiers::CTRL),
            (";6", Modifiers::CTRL | Modifiers::SHIFT),
            (";7", Modifiers::CTRL | Modifiers::ALT),
            (";8", Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT),
        ];
        let meta = Modifiers::ALT;
        let meta_modifier_combos = &[
            (";9", meta),
            (";10", meta | Modifiers::SHIFT),
            (";11", meta | Modifiers::ALT),
            (";12", meta | Modifiers::ALT | Modifiers::SHIFT),
            (";13", meta | Modifiers::CTRL),
            (";14", meta | Modifiers::CTRL | Modifiers::SHIFT),
            (";15", meta | Modifiers::CTRL | Modifiers::ALT),
            (
                ";16",
                meta | Modifiers::CTRL | Modifiers::ALT | Modifiers::SHIFT,
            ),
        ];

        let modifier_combos_including_meta =
            || modifier_combos.iter().chain(meta_modifier_combos.iter());

        for alpha in b'A'..=b'Z' {
            // Ctrl-[A..=Z] are sent as 1..=26
            let ctrl = [alpha & 0x1f];
            map.insert(
                &ctrl,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char((alpha as char).to_ascii_lowercase()),
                    modifiers: Modifiers::CTRL,
                }),
            );

            // ALT A-Z is often sent with a leading ESC
            let alt = [0x1b, alpha];
            map.insert(
                &alt,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Char(alpha as char),
                    modifiers: Modifiers::ALT,
                }),
            );
        }

        for c in 0..=0x7fu8 {
            for (suffix, modifiers) in modifier_combos {
                // `CSI u` encodings for the ascii range;
                // see http://www.leonerd.org.uk/hacks/fixterms/
                let key = format!("\x1b[{}{}u", c, suffix);
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent {
                        key: KeyCode::Char(c as char),
                        modifiers: *modifiers,
                    }),
                );

                if !suffix.is_empty() {
                    // xterm modifyOtherKeys sequences
                    let key = format!("\x1b[27{};{}~", suffix, c);
                    map.insert(
                        key,
                        InputEvent::Key(KeyEvent {
                            key: match c {
                                8 | 0x7f => KeyCode::Backspace,
                                0x1b => KeyCode::Escape,
                                9 => KeyCode::Tab,
                                10 | 13 => KeyCode::Enter,
                                _ => KeyCode::Char(c as char),
                            },
                            modifiers: *modifiers,
                        }),
                    );
                }
            }
        }

        // Common arrow keys
        for (keycode, dir) in &[
            (KeyCode::UpArrow, b'A'),
            (KeyCode::DownArrow, b'B'),
            (KeyCode::RightArrow, b'C'),
            (KeyCode::LeftArrow, b'D'),
            (KeyCode::Home, b'H'),
            (KeyCode::End, b'F'),
        ] {
            // Arrow keys in normal mode encoded using CSI
            let arrow = [0x1b, b'[', *dir];
            map.insert(
                &arrow,
                InputEvent::Key(KeyEvent {
                    key: *keycode,
                    modifiers: Modifiers::NONE,
                }),
            );
            for (suffix, modifiers) in modifier_combos_including_meta() {
                let key = format!("\x1b[1{}{}", suffix, *dir as char);
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent {
                        key: *keycode,
                        modifiers: *modifiers,
                    }),
                );
            }
        }
        for &(keycode, dir) in &[
            (KeyCode::UpArrow, b'a'),
            (KeyCode::DownArrow, b'b'),
            (KeyCode::RightArrow, b'c'),
            (KeyCode::LeftArrow, b'd'),
        ] {
            // rxvt-specific modified arrows.
            for &(seq, mods) in &[
                ([0x1b, b'[', dir], Modifiers::SHIFT),
                ([0x1b, b'O', dir], Modifiers::CTRL),
            ] {
                map.insert(
                    &seq,
                    InputEvent::Key(KeyEvent {
                        key: keycode,
                        modifiers: mods,
                    }),
                );
            }
        }

        for (keycode, dir) in &[
            (KeyCode::ApplicationUpArrow, b'A'),
            (KeyCode::ApplicationDownArrow, b'B'),
            (KeyCode::ApplicationRightArrow, b'C'),
            (KeyCode::ApplicationLeftArrow, b'D'),
        ] {
            // Arrow keys in application cursor mode encoded using SS3
            let app = [0x1b, b'O', *dir];
            map.insert(
                &app,
                InputEvent::Key(KeyEvent {
                    key: *keycode,
                    modifiers: Modifiers::NONE,
                }),
            );
            for (suffix, modifiers) in modifier_combos {
                let key = format!("\x1bO1{}{}", suffix, *dir as char);
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent {
                        key: *keycode,
                        modifiers: *modifiers,
                    }),
                );
            }
        }

        // Function keys 1-4 with no modifiers encoded using SS3
        for (keycode, c) in &[
            (KeyCode::Function(1), b'P'),
            (KeyCode::Function(2), b'Q'),
            (KeyCode::Function(3), b'R'),
            (KeyCode::Function(4), b'S'),
        ] {
            let key = [0x1b, b'O', *c];
            map.insert(
                &key,
                InputEvent::Key(KeyEvent {
                    key: *keycode,
                    modifiers: Modifiers::NONE,
                }),
            );
        }

        // Function keys 1-4 with modifiers
        for (keycode, c) in &[
            (KeyCode::Function(1), b'P'),
            (KeyCode::Function(2), b'Q'),
            (KeyCode::Function(3), b'R'),
            (KeyCode::Function(4), b'S'),
        ] {
            for (suffix, modifiers) in modifier_combos_including_meta() {
                let key = format!("\x1b[1{suffix}{code}", code = *c as char, suffix = suffix);
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent {
                        key: *keycode,
                        modifiers: *modifiers,
                    }),
                );
            }
        }

        // Function keys with modifiers encoded using CSI.
        // http://aperiodic.net/phil/archives/Geekery/term-function-keys.html
        for (range, offset) in &[
            // F1-F5 encoded as 11-15
            (1..=5, 10),
            // F6-F10 encoded as 17-21
            (6..=10, 11),
            // F11-F14 encoded as 23-26
            (11..=14, 12),
            // F15-F16 encoded as 28-29
            (15..=16, 13),
            // F17-F20 encoded as 31-34
            (17..=20, 14),
        ] {
            for n in range.clone() {
                for (suffix, modifiers) in modifier_combos_including_meta() {
                    let key = format!("\x1b[{code}{suffix}~", code = n + offset, suffix = suffix);
                    map.insert(
                        key,
                        InputEvent::Key(KeyEvent {
                            key: KeyCode::Function(n),
                            modifiers: *modifiers,
                        }),
                    );
                }
            }
        }

        for (keycode, c) in &[
            (KeyCode::Insert, b'2'),
            (KeyCode::Delete, b'3'),
            (KeyCode::Home, b'1'),
            (KeyCode::End, b'4'),
            (KeyCode::PageUp, b'5'),
            (KeyCode::PageDown, b'6'),
            // rxvt
            (KeyCode::Home, b'7'),
            (KeyCode::End, b'8'),
        ] {
            for (suffix, modifiers) in &[
                (b'~', Modifiers::NONE),
                (b'$', Modifiers::SHIFT),
                (b'^', Modifiers::CTRL),
                (b'@', Modifiers::SHIFT | Modifiers::CTRL),
            ] {
                let key = [0x1b, b'[', *c, *suffix];
                map.insert(
                    key,
                    InputEvent::Key(KeyEvent {
                        key: *keycode,
                        modifiers: *modifiers,
                    }),
                );
            }
        }

        map.insert(
            &[0x7f],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                modifiers: Modifiers::NONE,
            }),
        );

        map.insert(
            &[0x8],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Backspace,
                modifiers: Modifiers::NONE,
            }),
        );

        map.insert(
            &[0x1b],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                modifiers: Modifiers::NONE,
            }),
        );

        map.insert(
            &[b'\t'],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::NONE,
            }),
        );
        map.insert(
            b"\x1b[Z",
            InputEvent::Key(KeyEvent {
                key: KeyCode::Tab,
                modifiers: Modifiers::SHIFT,
            }),
        );

        map.insert(
            &[b'\r'],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }),
        );
        map.insert(
            &[b'\n'],
            InputEvent::Key(KeyEvent {
                key: KeyCode::Enter,
                modifiers: Modifiers::NONE,
            }),
        );

        map.insert(
            b"\x1b[200~",
            InputEvent::Key(KeyEvent {
                key: KeyCode::InternalPasteStart,
                modifiers: Modifiers::NONE,
            }),
        );
        map.insert(
            b"\x1b[201~",
            InputEvent::Key(KeyEvent {
                key: KeyCode::InternalPasteEnd,
                modifiers: Modifiers::NONE,
            }),
        );
        map.insert(
            b"\x1b[",
            InputEvent::Key(KeyEvent {
                key: KeyCode::Char('['),
                modifiers: Modifiers::ALT,
            }),
        );

        map
    }

    /// Returns the first char from a str and the length of that char
    /// in *bytes*.
    fn first_char_and_len(s: &str) -> (char, usize) {
        let mut iter = s.chars();
        let c = iter.next().unwrap();
        (c, c.len_utf8())
    }

    /// This is a horrible function to pull off the first unicode character
    /// from the sequence of bytes and return it and the remaining slice.
    fn decode_one_char(bytes: &[u8]) -> Option<(char, usize)> {
        let bytes = &bytes[..bytes.len().min(4)];
        match std::str::from_utf8(bytes) {
            Ok(s) => {
                let (c, len) = Self::first_char_and_len(s);
                Some((c, len))
            },
            Err(err) => {
                if err.error_len().is_none() {
                    return None;
                }

                let valid_up_to = err.valid_up_to().min(bytes.len());
                let valid = &bytes[..valid_up_to];
                if !valid.is_empty() {
                    let s = unsafe { std::str::from_utf8_unchecked(valid) };
                    let (c, len) = Self::first_char_and_len(s);
                    Some((c, len))
                } else {
                    None
                }
            },
        }
    }

    fn dispatch_callback<F: FnMut(InputEvent)>(&mut self, mut callback: F, event: InputEvent) {
        match (self.state, &event) {
            (
                InputState::Normal,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::InternalPasteStart,
                    ..
                }),
            ) => {
                self.state = InputState::Pasting(0);
            },
            (
                InputState::EscapeMaybeAlt,
                InputEvent::Key(KeyEvent {
                    key: KeyCode::InternalPasteStart,
                    ..
                }),
            ) => {
                // The prior ESC was not part of an ALT sequence, so emit
                // it before we start collecting for paste.
                callback(InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: Modifiers::NONE,
                }));
                self.state = InputState::Pasting(0);
            },
            (InputState::EscapeMaybeAlt, InputEvent::Key(KeyEvent { key, modifiers })) => {
                // Treat this as ALT-key
                let key = *key;
                let modifiers = *modifiers;
                self.state = InputState::Normal;
                callback(InputEvent::Key(KeyEvent {
                    key,
                    modifiers: modifiers | Modifiers::ALT,
                }));
            },
            (InputState::EscapeMaybeAlt, _) => {
                // The prior ESC was not part of an ALT sequence, so emit
                // both it and the current event
                callback(InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: Modifiers::NONE,
                }));
                callback(event);
            },
            (_, _) => callback(event),
        }
    }

    fn process_bytes<F: FnMut(InputEvent)>(&mut self, mut callback: F, maybe_more: bool) {
        while !self.buf.is_empty() {
            match self.state {
                InputState::Pasting(offset) => {
                    let end_paste = b"\x1b[201~";
                    if let Some(idx) = self.buf.find_subsequence(offset, end_paste) {
                        let pasted =
                            String::from_utf8_lossy(&self.buf.as_slice()[0..idx]).to_string();
                        self.buf.advance(pasted.len() + end_paste.len());
                        callback(InputEvent::Paste(pasted));
                        self.state = InputState::Normal;
                    } else {
                        self.state =
                            InputState::Pasting(self.buf.len().saturating_sub(end_paste.len()));
                        return;
                    }
                },
                InputState::EscapeMaybeAlt | InputState::Normal => {
                    if self.state == InputState::Normal
                        && self.buf.as_slice().get(0) == Some(&b'\x1b')
                    {
                        if let Some((event, len)) = parse_sgr_mouse(self.buf.as_slice()) {
                            self.buf.advance(len);
                            callback(event);
                            continue;
                        }

                        // OSC sequence check — must come before the incomplete-SGR-mouse early return
                        if let Some((event, len)) = parse_osc(self.buf.as_slice()) {
                            self.buf.advance(len);
                            callback(event);
                            continue;
                        }

                        // Incomplete OSC — buffer and wait for more data
                        if maybe_more && self.buf.as_slice().starts_with(b"\x1b]") {
                            return;
                        }

                        if maybe_more && self.buf.as_slice().starts_with(b"\x1b[<") {
                            return;
                        }
                    }

                    match (
                        self.key_map.lookup(self.buf.as_slice(), maybe_more),
                        maybe_more,
                    ) {
                        // If we got an unambiguous ESC and we have more data to
                        // follow, then this is likely the Meta version of the
                        // following keypress.  Buffer up the escape key and
                        // consume it from the input.  dispatch_callback() will
                        // emit either the ESC or the ALT modified following key.
                        (
                            Found::Exact(
                                len,
                                InputEvent::Key(KeyEvent {
                                    key: KeyCode::Escape,
                                    modifiers: Modifiers::NONE,
                                }),
                            ),
                            _,
                        ) if self.state == InputState::Normal && self.buf.len() > len => {
                            self.state = InputState::EscapeMaybeAlt;
                            self.buf.advance(len);
                        },
                        (Found::Exact(len, event), _) | (Found::Ambiguous(len, event), false) => {
                            self.dispatch_callback(&mut callback, event.clone());
                            self.buf.advance(len);
                        },
                        (Found::Ambiguous(_, _), true) | (Found::NeedData, true) => {
                            return;
                        },
                        (Found::None, _) | (Found::NeedData, false) => {
                            // No pre-defined key, so pull out a unicode character
                            if let Some((c, len)) = Self::decode_one_char(self.buf.as_slice()) {
                                self.buf.advance(len);
                                self.dispatch_callback(
                                    &mut callback,
                                    InputEvent::Key(KeyEvent {
                                        key: KeyCode::Char(c),
                                        modifiers: Modifiers::NONE,
                                    }),
                                );
                            } else {
                                // We need more data to recognize the input, so
                                // yield the remainder of the slice
                                return;
                            }
                        },
                    }
                },
            }
        }
    }

    /// Push a sequence of bytes into the parser.
    /// Each time input is recognized, the provided `callback` will be passed
    /// the decoded `InputEvent`.
    /// If not enough data are available to fully decode a sequence, the
    /// remaining data will be buffered until the next call.
    /// The `maybe_more` flag controls how ambiguous partial sequences are
    /// handled. The intent is that `maybe_more` should be set to true if
    /// you believe that you will be able to provide more data momentarily.
    /// This will cause the parser to defer judgement on partial prefix
    /// matches. You should attempt to read and pass the new data in
    /// immediately afterwards. If you have attempted a read and no data is
    /// immediately available, you should follow up with a call to parse
    /// with an empty slice and `maybe_more=false` to allow the partial
    /// data to be recognized and processed.
    pub fn parse<F: FnMut(InputEvent)>(&mut self, bytes: &[u8], callback: F, maybe_more: bool) {
        self.buf.extend_with(bytes);
        self.process_bytes(callback, maybe_more);
    }

    pub fn parse_as_vec(&mut self, bytes: &[u8], maybe_more: bool) -> Vec<InputEvent> {
        let mut result = Vec::new();
        self.parse(bytes, |event| result.push(event), maybe_more);
        result
    }

    #[cfg(windows)]
    pub fn decode_input_records_as_vec(
        &mut self,
        records: &[winapi::um::wincon::INPUT_RECORD],
    ) -> Vec<InputEvent> {
        let mut result = Vec::new();
        self.decode_input_records(records, &mut |event| result.push(event));
        result
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const NO_MORE: bool = false;
    const MAYBE_MORE: bool = true;

    #[test]
    fn simple() {
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"hello", NO_MORE);
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('h'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('e'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('l'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('l'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('o'),
                }),
            ],
            inputs
        );
    }

    #[test]
    fn control_characters() {
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x03\x1bJ\x7f", NO_MORE);
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::CTRL,
                    key: KeyCode::Char('c'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::ALT,
                    key: KeyCode::Char('J'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Backspace,
                }),
            ],
            inputs
        );
    }

    #[test]
    fn arrow_keys() {
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1bOA\x1bOB\x1bOC\x1bOD", NO_MORE);
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::ApplicationUpArrow,
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::ApplicationDownArrow,
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::ApplicationRightArrow,
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::ApplicationLeftArrow,
                }),
            ],
            inputs
        );
    }

    #[test]
    fn partial() {
        let mut p = InputParser::new();
        let mut inputs = Vec::new();
        // Fragment this F-key sequence across two different pushes
        p.parse(b"\x1b[11", |evt| inputs.push(evt), true);
        p.parse(b"~", |evt| inputs.push(evt), true);
        // make sure we recognize it as just the F-key
        assert_eq!(
            vec![InputEvent::Key(KeyEvent {
                modifiers: Modifiers::NONE,
                key: KeyCode::Function(1),
            })],
            inputs
        );
    }

    #[test]
    fn partial_ambig() {
        let mut p = InputParser::new();

        assert_eq!(
            vec![InputEvent::Key(KeyEvent {
                key: KeyCode::Escape,
                modifiers: Modifiers::NONE,
            })],
            p.parse_as_vec(b"\x1b", false)
        );

        let mut inputs = Vec::new();
        // An incomplete F-key sequence fragmented across two different pushes
        p.parse(b"\x1b[11", |evt| inputs.push(evt), MAYBE_MORE);
        p.parse(b"", |evt| inputs.push(evt), NO_MORE);
        // since we finish with maybe_more false (NO_MORE), the results should be the longest matching
        // parts of said f-key sequence
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::ALT,
                    key: KeyCode::Char('['),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('1'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('1'),
                }),
            ],
            inputs
        );
    }

    #[test]
    fn partial_mouse() {
        let mut p = InputParser::new();
        let mut inputs = Vec::new();
        // Fragment this mouse sequence across two different pushes
        p.parse(b"\x1b[<0;0;0", |evt| inputs.push(evt), true);
        p.parse(b"M", |evt| inputs.push(evt), true);
        // make sure we recognize it as just the mouse event
        assert_eq!(
            vec![InputEvent::Mouse(MouseEvent {
                x: 0,
                y: 0,
                mouse_buttons: MouseButtons::LEFT,
                modifiers: Modifiers::NONE,
            })],
            inputs
        );
    }

    #[test]
    fn partial_mouse_ambig() {
        let mut p = InputParser::new();
        let mut inputs = Vec::new();
        // Fragment this mouse sequence across two different pushes
        p.parse(b"\x1b[<", |evt| inputs.push(evt), MAYBE_MORE);
        p.parse(b"0;0;0", |evt| inputs.push(evt), NO_MORE);
        // since we finish with maybe_more false (NO_MORE), the results should be the longest matching
        // parts of said mouse sequence
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::ALT,
                    key: KeyCode::Char('['),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('<'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('0'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char(';'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('0'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char(';'),
                }),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('0'),
                }),
            ],
            inputs
        );
    }

    #[test]
    fn alt_left_bracket() {
        // tests that `Alt` + `[` is recognized as a single
        // event rather than two events (one `Esc` the second `Char('[')`)
        let mut p = InputParser::new();

        let mut inputs = Vec::new();
        p.parse(b"\x1b[", |evt| inputs.push(evt), false);

        assert_eq!(
            vec![InputEvent::Key(KeyEvent {
                modifiers: Modifiers::ALT,
                key: KeyCode::Char('['),
            }),],
            inputs
        );
    }

    #[test]
    fn modify_other_keys_parse() {
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(
            b"\x1b[27;5;13~\x1b[27;5;9~\x1b[27;6;8~\x1b[27;2;127~\x1b[27;6;27~",
            NO_MORE,
        );
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Enter,
                    modifiers: Modifiers::CTRL,
                }),
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Tab,
                    modifiers: Modifiers::CTRL,
                }),
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    modifiers: Modifiers::CTRL | Modifiers::SHIFT,
                }),
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Backspace,
                    modifiers: Modifiers::SHIFT,
                }),
                InputEvent::Key(KeyEvent {
                    key: KeyCode::Escape,
                    modifiers: Modifiers::CTRL | Modifiers::SHIFT,
                }),
            ],
            inputs
        );
    }

    #[test]
    fn modify_other_keys_encode() {
        let mode = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: None,
        };
        let mode_1 = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: Some(1),
        };
        let mode_2 = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: Some(2),
        };

        assert_eq!(
            KeyCode::Enter.encode(Modifiers::CTRL, mode, true).unwrap(),
            "\r".to_string()
        );
        assert_eq!(
            KeyCode::Enter
                .encode(Modifiers::CTRL, mode_1, true)
                .unwrap(),
            "\x1b[27;5;13~".to_string()
        );
        assert_eq!(
            KeyCode::Enter
                .encode(Modifiers::CTRL | Modifiers::SHIFT, mode_1, true)
                .unwrap(),
            "\x1b[27;6;13~".to_string()
        );

        // This case is not conformant with xterm!
        // xterm just returns tab for CTRL-Tab when modify_other_keys
        // is not set.
        assert_eq!(
            KeyCode::Tab.encode(Modifiers::CTRL, mode, true).unwrap(),
            "\x1b[9;5u".to_string()
        );
        assert_eq!(
            KeyCode::Tab.encode(Modifiers::CTRL, mode_1, true).unwrap(),
            "\x1b[27;5;9~".to_string()
        );
        assert_eq!(
            KeyCode::Tab
                .encode(Modifiers::CTRL | Modifiers::SHIFT, mode_1, true)
                .unwrap(),
            "\x1b[27;6;9~".to_string()
        );

        assert_eq!(
            KeyCode::Char('c')
                .encode(Modifiers::CTRL, mode, true)
                .unwrap(),
            "\x03".to_string()
        );
        assert_eq!(
            KeyCode::Char('c')
                .encode(Modifiers::CTRL, mode_1, true)
                .unwrap(),
            "\x03".to_string()
        );
        assert_eq!(
            KeyCode::Char('c')
                .encode(Modifiers::CTRL, mode_2, true)
                .unwrap(),
            "\x1b[27;5;99~".to_string()
        );

        assert_eq!(
            KeyCode::Char('1')
                .encode(Modifiers::CTRL, mode, true)
                .unwrap(),
            "1".to_string()
        );
        assert_eq!(
            KeyCode::Char('1')
                .encode(Modifiers::CTRL, mode_2, true)
                .unwrap(),
            "\x1b[27;5;49~".to_string()
        );

        assert_eq!(
            KeyCode::Char(',')
                .encode(Modifiers::CTRL, mode, true)
                .unwrap(),
            ",".to_string()
        );
        assert_eq!(
            KeyCode::Char(',')
                .encode(Modifiers::CTRL, mode_2, true)
                .unwrap(),
            "\x1b[27;5;44~".to_string()
        );
    }

    #[test]
    fn encode_issue_892() {
        let mode = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: None,
        };

        assert_eq!(
            KeyCode::LeftArrow
                .encode(Modifiers::NONE, mode, true)
                .unwrap(),
            "\x1b[D".to_string()
        );
        assert_eq!(
            KeyCode::LeftArrow
                .encode(Modifiers::ALT, mode, true)
                .unwrap(),
            "\x1b[1;3D".to_string()
        );
        assert_eq!(
            KeyCode::Home.encode(Modifiers::NONE, mode, true).unwrap(),
            "\x1b[H".to_string()
        );
        assert_eq!(
            KeyCode::Home.encode(Modifiers::ALT, mode, true).unwrap(),
            "\x1b[1;3H".to_string()
        );
        assert_eq!(
            KeyCode::End.encode(Modifiers::NONE, mode, true).unwrap(),
            "\x1b[F".to_string()
        );
        assert_eq!(
            KeyCode::End.encode(Modifiers::ALT, mode, true).unwrap(),
            "\x1b[1;3F".to_string()
        );
        assert_eq!(
            KeyCode::Tab.encode(Modifiers::ALT, mode, true).unwrap(),
            "\x1b\t".to_string()
        );
        assert_eq!(
            KeyCode::PageUp.encode(Modifiers::ALT, mode, true).unwrap(),
            "\x1b[5;3~".to_string()
        );
        assert_eq!(
            KeyCode::Function(1)
                .encode(Modifiers::NONE, mode, true)
                .unwrap(),
            "\x1bOP".to_string()
        );
    }

    #[test]
    fn partial_bracketed_paste() {
        let mut p = InputParser::new();

        let input = b"\x1b[200~1234";
        let input2 = b"5678\x1b[201~";

        let mut inputs = vec![];

        p.parse(input, |e| inputs.push(e), false);
        p.parse(input2, |e| inputs.push(e), false);

        assert_eq!(vec![InputEvent::Paste("12345678".to_owned())], inputs)
    }

    #[test]
    fn mouse_horizontal_scroll() {
        let mut p = InputParser::new();

        let input = b"\x1b[<66;42;12M\x1b[<67;42;12M";
        let res = p.parse_as_vec(input, MAYBE_MORE);

        assert_eq!(
            vec![
                InputEvent::Mouse(MouseEvent {
                    x: 42,
                    y: 12,
                    mouse_buttons: MouseButtons::HORZ_WHEEL | MouseButtons::WHEEL_POSITIVE,
                    modifiers: Modifiers::NONE,
                }),
                InputEvent::Mouse(MouseEvent {
                    x: 42,
                    y: 12,
                    mouse_buttons: MouseButtons::HORZ_WHEEL,
                    modifiers: Modifiers::NONE,
                })
            ],
            res
        );
    }

    #[test]
    fn encode_issue_3478_xterm() {
        let mode = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: None,
        };

        assert_eq!(
            KeyCode::Numpad0
                .encode(Modifiers::NONE, mode, true)
                .unwrap(),
            "\u{1b}[2~".to_string()
        );
        assert_eq!(
            KeyCode::Numpad0
                .encode(Modifiers::SHIFT, mode, true)
                .unwrap(),
            "\u{1b}[2;2~".to_string()
        );

        assert_eq!(
            KeyCode::Numpad1
                .encode(Modifiers::NONE, mode, true)
                .unwrap(),
            "\u{1b}[F".to_string()
        );
        assert_eq!(
            KeyCode::Numpad1
                .encode(Modifiers::NONE | Modifiers::SHIFT, mode, true)
                .unwrap(),
            "\u{1b}[1;2F".to_string()
        );
    }

    #[test]
    fn encode_tab_with_modifiers() {
        let mode = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            newline_mode: false,
            application_cursor_keys: false,
            modify_other_keys: None,
        };

        let mods_to_result = [
            (Modifiers::SHIFT, "\u{1b}[Z"),
            (Modifiers::SHIFT | Modifiers::LEFT_SHIFT, "\u{1b}[Z"),
            (Modifiers::SHIFT | Modifiers::RIGHT_SHIFT, "\u{1b}[Z"),
            (Modifiers::CTRL, "\u{1b}[9;5u"),
            (Modifiers::CTRL | Modifiers::LEFT_CTRL, "\u{1b}[9;5u"),
            (Modifiers::CTRL | Modifiers::RIGHT_CTRL, "\u{1b}[9;5u"),
            (
                Modifiers::SHIFT | Modifiers::CTRL | Modifiers::LEFT_CTRL | Modifiers::LEFT_SHIFT,
                "\u{1b}[1;5Z",
            ),
        ];
        for (mods, result) in mods_to_result {
            assert_eq!(
                KeyCode::Tab.encode(mods, mode, true).unwrap(),
                result,
                "{:?}",
                mods
            );
        }
    }

    #[test]
    fn mouse_button1_press() {
        let mut p = InputParser::new();
        let res = p.parse_as_vec(b"\x1b[<0;42;12M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 42,
                y: 12,
                mouse_buttons: MouseButtons::LEFT,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_button1_release() {
        let mut p = InputParser::new();
        let res = p.parse_as_vec(b"\x1b[<0;42;12m", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 42,
                y: 12,
                mouse_buttons: MouseButtons::NONE,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_button3_with_shift() {
        let mut p = InputParser::new();
        // button 2 (right) = 2, SHIFT adds 4 to p0 -> 6
        let res = p.parse_as_vec(b"\x1b[<6;10;20M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 10,
                y: 20,
                mouse_buttons: MouseButtons::RIGHT,
                modifiers: Modifiers::SHIFT,
            })]
        );
    }

    #[test]
    fn mouse_drag() {
        let mut p = InputParser::new();
        // button1 drag = 32
        let res = p.parse_as_vec(b"\x1b[<32;5;5M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 5,
                y: 5,
                mouse_buttons: MouseButtons::LEFT,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_vertical_scroll_up() {
        let mut p = InputParser::new();
        // button4 press = 64
        let res = p.parse_as_vec(b"\x1b[<64;1;1M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 1,
                y: 1,
                mouse_buttons: MouseButtons::VERT_WHEEL | MouseButtons::WHEEL_POSITIVE,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_vertical_scroll_down() {
        let mut p = InputParser::new();
        // button5 press = 65
        let res = p.parse_as_vec(b"\x1b[<65;1;1M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 1,
                y: 1,
                mouse_buttons: MouseButtons::VERT_WHEEL,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_motion_no_buttons() {
        let mut p = InputParser::new();
        // motion with no buttons = 35
        let res = p.parse_as_vec(b"\x1b[<35;10;10M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 10,
                y: 10,
                mouse_buttons: MouseButtons::NONE,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_with_ctrl_alt() {
        let mut p = InputParser::new();
        // button1 press = 0, ALT=8, CTRL=16 -> 0+8+16=24
        let res = p.parse_as_vec(b"\x1b[<24;1;1M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 1,
                y: 1,
                mouse_buttons: MouseButtons::LEFT,
                modifiers: Modifiers::ALT | Modifiers::CTRL,
            })]
        );
    }

    #[test]
    fn mouse_large_coordinates() {
        let mut p = InputParser::new();
        let res = p.parse_as_vec(b"\x1b[<0;999;999M", true);
        assert_eq!(
            res,
            vec![InputEvent::Mouse(MouseEvent {
                x: 999,
                y: 999,
                mouse_buttons: MouseButtons::LEFT,
                modifiers: Modifiers::NONE,
            })]
        );
    }

    #[test]
    fn mouse_followed_by_key() {
        let mut p = InputParser::new();
        let res = p.parse_as_vec(b"\x1b[<0;1;1Mhello", false);
        assert_eq!(res.len(), 6); // 1 mouse + 5 chars
        assert!(matches!(res[0], InputEvent::Mouse(_)));
        assert!(matches!(res[1], InputEvent::Key(_)));
    }

    #[test]
    fn two_mouse_events_back_to_back() {
        let mut p = InputParser::new();
        let res = p.parse_as_vec(b"\x1b[<0;1;1M\x1b[<0;2;2M", true);
        assert_eq!(res.len(), 2);
    }

    #[test]
    fn invalid_sgr_mouse_falls_through() {
        let mut p = InputParser::new();
        // Invalid: missing terminator, not enough params
        let res = p.parse_as_vec(b"\x1b[<0;1M", false);
        // Should NOT parse as mouse - falls through to keymap
        assert!(res.iter().all(|e| matches!(e, InputEvent::Key(_))));
    }

    #[test]
    fn osc_bel_terminated() {
        // Complete OSC sequence with BEL terminator
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b]99;i=test:p=title;Hello\x07", NO_MORE);
        assert_eq!(
            vec![InputEvent::OperatingSystemCommand(
                b"99;i=test:p=title;Hello".to_vec()
            )],
            inputs
        );
    }

    #[test]
    fn osc_st_terminated() {
        // Complete OSC sequence with ST terminator (ESC \)
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b]99;i=test:p=title;Hello\x1b\\", NO_MORE);
        assert_eq!(
            vec![InputEvent::OperatingSystemCommand(
                b"99;i=test:p=title;Hello".to_vec()
            )],
            inputs
        );
    }

    #[test]
    fn osc_partial_across_reads() {
        // OSC sequence split across two reads — must buffer first part
        let mut p = InputParser::new();
        let mut inputs = Vec::new();
        p.parse(
            b"\x1b]99;i=test:p=title;Hel",
            |evt| inputs.push(evt),
            MAYBE_MORE,
        );
        assert!(inputs.is_empty(), "no events yet - sequence incomplete");
        p.parse(b"lo\x1b\\", |evt| inputs.push(evt), MAYBE_MORE);
        assert_eq!(
            vec![InputEvent::OperatingSystemCommand(
                b"99;i=test:p=title;Hello".to_vec()
            )],
            inputs
        );
    }

    #[test]
    fn osc_followed_by_keypress() {
        // OSC sequence then regular key in same buffer
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b]99;i=test;clicked\x07x", NO_MORE);
        assert_eq!(
            vec![
                InputEvent::OperatingSystemCommand(b"99;i=test;clicked".to_vec()),
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('x'),
                }),
            ],
            inputs
        );
    }

    #[test]
    fn keypress_followed_by_osc() {
        // Regular key then OSC sequence in same buffer
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"x\x1b]99;i=test;clicked\x07", NO_MORE);
        assert_eq!(
            vec![
                InputEvent::Key(KeyEvent {
                    modifiers: Modifiers::NONE,
                    key: KeyCode::Char('x'),
                }),
                InputEvent::OperatingSystemCommand(b"99;i=test;clicked".to_vec()),
            ],
            inputs
        );
    }

    #[test]
    fn osc_incomplete_degrades_to_keys() {
        // Incomplete OSC that never gets a terminator — when finalized with
        // maybe_more=false, must degrade to individual key events (not hang)
        let mut p = InputParser::new();
        let mut inputs = Vec::new();
        p.parse(b"\x1b]99;no-terminator", |evt| inputs.push(evt), MAYBE_MORE);
        assert!(inputs.is_empty(), "buffered while maybe_more=true");
        p.parse(b"", |evt| inputs.push(evt), NO_MORE);
        assert!(!inputs.is_empty(), "must emit something on finalization");
    }

    #[test]
    fn osc_non_99_code() {
        // Non-99 OSC codes are also captured as OperatingSystemCommand
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b]11;rgb:0000/0000/0000\x1b\\", NO_MORE);
        assert_eq!(
            vec![InputEvent::OperatingSystemCommand(
                b"11;rgb:0000/0000/0000".to_vec()
            )],
            inputs
        );
    }

    #[test]
    fn osc_empty_payload() {
        // Edge case: OSC with no payload between \x1b] and terminator
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b]\x07", NO_MORE);
        assert_eq!(
            vec![InputEvent::OperatingSystemCommand(b"".to_vec())],
            inputs
        );
    }

    #[test]
    fn csi_not_captured_as_osc() {
        // ESC [ (CSI) must NOT be captured as an OSC sequence.
        // This validates that only ESC ] triggers OSC parsing.
        let mut p = InputParser::new();
        let inputs = p.parse_as_vec(b"\x1b[A", NO_MORE);
        assert_eq!(
            vec![InputEvent::Key(KeyEvent {
                modifiers: Modifiers::NONE,
                key: KeyCode::UpArrow,
            })],
            inputs
        );
    }

    #[test]
    fn utf8_split_across_reads_waits_for_more_data() {
        let mut p = InputParser::new();
        let mut inputs = Vec::new();

        // Send first 2 bytes of 3-byte UTF-8 char '你' (0xe4 0xbd 0xa0)
        p.parse("你".as_bytes()[..2].as_ref(), |evt| inputs.push(evt), MAYBE_MORE);
        assert!(inputs.is_empty(), "incomplete utf8 should be buffered");

        // Send remaining byte
        p.parse(&"你".as_bytes()[2..], |evt| inputs.push(evt), MAYBE_MORE);
        assert_eq!(
            vec![InputEvent::Key(KeyEvent {
                modifiers: Modifiers::NONE,
                key: KeyCode::Char('你'),
            })],
            inputs
        );
    }

    #[test]
    fn finalized_incomplete_utf8_does_not_panic() {
        let mut p = InputParser::new();
        // Send first 2 bytes of 3-byte '┌' (0xe2 0x94 0x8c) with no more data coming
        let inputs = p.parse_as_vec(&"┌".as_bytes()[..2], false);
        // Should not panic — incomplete sequence is dropped gracefully
        assert!(inputs.is_empty());
    }
}
