use crate::input::actions::Action;
use crate::input::config::ConversionError;
use crate::input::layout::SplitSize;
use clap::ArgEnum;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};
use std::time::Duration;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, ToString};

pub type ClientId = u16; // TODO: merge with crate type?

pub fn client_id_to_colors(
    client_id: ClientId,
    colors: Palette,
) -> Option<(PaletteColor, PaletteColor)> {
    // (primary color, secondary color)
    match client_id {
        1 => Some((colors.magenta, colors.black)),
        2 => Some((colors.blue, colors.black)),
        3 => Some((colors.purple, colors.black)),
        4 => Some((colors.yellow, colors.black)),
        5 => Some((colors.cyan, colors.black)),
        6 => Some((colors.gold, colors.black)),
        7 => Some((colors.red, colors.black)),
        8 => Some((colors.silver, colors.black)),
        9 => Some((colors.pink, colors.black)),
        10 => Some((colors.brown, colors.black)),
        _ => None,
    }
}

pub fn single_client_color(colors: Palette) -> (PaletteColor, PaletteColor) {
    (colors.green, colors.black)
}

// TODO: Add a shortened string representation (beyond `Display::fmt` below) that can be used when
// screen space is scarce. Useful for e.g. "ENTER", "SPACE", "TAB" to display as Unicode
// representations instead.
// NOTE: Do not reorder the key variants since that influences what the `status_bar` plugin
// displays!
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Key {
    PageDown,
    PageUp,
    Left,
    Down,
    Up,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Alt(CharOrArrow),
    Ctrl(char),
    BackTab,
    Null,
    Esc,
    AltF(u8),
    CtrlF(u8),
}

impl FromStr for Key {
    type Err = Box<dyn std::error::Error>;
    fn from_str(key_str: &str) -> Result<Self, Self::Err> {
        let mut modifier: Option<&str> = None;
        let mut main_key: Option<&str> = None;
        for (index, part) in key_str.split_ascii_whitespace().enumerate() {
            if index == 0 && (part == "Ctrl" || part == "Alt") {
                modifier = Some(part);
            } else if main_key.is_none() {
                main_key = Some(part)
            }
        }
        match (modifier, main_key) {
            (Some("Ctrl"), Some(main_key)) if main_key == "@" || main_key == "Space" => {
                Ok(Key::Char('\x00'))
            },
            (Some("Ctrl"), Some(main_key)) => {
                parse_main_key(main_key, key_str, Key::Ctrl, Key::CtrlF)
            },
            (Some("Alt"), Some(main_key)) => {
                match main_key {
                    // why crate::data::Direction and not just Direction?
                    // Because it's a different type that we export in this wasm mandated soup - we
                    // don't like it either! This will be solved as we chip away at our tech-debt
                    "Left" => Ok(Key::Alt(CharOrArrow::Direction(Direction::Left))),
                    "Right" => Ok(Key::Alt(CharOrArrow::Direction(Direction::Right))),
                    "Up" => Ok(Key::Alt(CharOrArrow::Direction(Direction::Up))),
                    "Down" => Ok(Key::Alt(CharOrArrow::Direction(Direction::Down))),
                    _ => parse_main_key(
                        main_key,
                        key_str,
                        |c| Key::Alt(CharOrArrow::Char(c)),
                        Key::AltF,
                    ),
                }
            },
            (None, Some(main_key)) => match main_key {
                "Backspace" => Ok(Key::Backspace),
                "Left" => Ok(Key::Left),
                "Right" => Ok(Key::Right),
                "Up" => Ok(Key::Up),
                "Down" => Ok(Key::Down),
                "Home" => Ok(Key::Home),
                "End" => Ok(Key::End),
                "PageUp" => Ok(Key::PageUp),
                "PageDown" => Ok(Key::PageDown),
                "Tab" => Ok(Key::BackTab),
                "Delete" => Ok(Key::Delete),
                "Insert" => Ok(Key::Insert),
                "Space" => Ok(Key::Char(' ')),
                "Enter" => Ok(Key::Char('\n')),
                "Esc" => Ok(Key::Esc),
                _ => parse_main_key(main_key, key_str, Key::Char, Key::F),
            },
            _ => Err(format!("Failed to parse key: {}", key_str).into()),
        }
    }
}

fn parse_main_key(
    main_key: &str,
    key_str: &str,
    to_char_key: impl FnOnce(char) -> Key,
    to_fn_key: impl FnOnce(u8) -> Key,
) -> Result<Key, Box<dyn std::error::Error>> {
    let mut key_chars = main_key.chars();
    let key_count = main_key.chars().count();
    if key_count == 1 {
        let key_char = key_chars.next().unwrap();
        Ok(to_char_key(key_char))
    } else if key_count > 1 {
        if let Some(first_char) = key_chars.next() {
            if first_char == 'F' {
                let f_index: String = key_chars.collect();
                let f_index: u8 = f_index
                    .parse()
                    .map_err(|e| format!("Failed to parse F index: {}", e))?;
                if f_index >= 1 && f_index <= 12 {
                    return Ok(to_fn_key(f_index));
                }
            }
        }
        Err(format!("Failed to parse key: {}", key_str).into())
    } else {
        Err(format!("Failed to parse key: {}", key_str).into())
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Backspace => write!(f, "BACKSPACE"),
            Key::Left => write!(f, "{}", Direction::Left),
            Key::Right => write!(f, "{}", Direction::Right),
            Key::Up => write!(f, "{}", Direction::Up),
            Key::Down => write!(f, "{}", Direction::Down),
            Key::Home => write!(f, "HOME"),
            Key::End => write!(f, "END"),
            Key::PageUp => write!(f, "PgUp"),
            Key::PageDown => write!(f, "PgDn"),
            Key::BackTab => write!(f, "TAB"),
            Key::Delete => write!(f, "DEL"),
            Key::Insert => write!(f, "INS"),
            Key::F(n) => write!(f, "F{}", n),
            Key::Char(c) => match c {
                '\n' => write!(f, "ENTER"),
                '\t' => write!(f, "TAB"),
                ' ' => write!(f, "SPACE"),
                '\x00' => write!(f, "Ctrl+SPACE"),
                _ => write!(f, "{}", c),
            },
            Key::Alt(c) => write!(f, "Alt+{}", c),
            Key::Ctrl(c) => write!(f, "Ctrl+{}", Key::Char(*c)),
            Key::AltF(n) => write!(f, "Alt+F{}", n),
            Key::CtrlF(n) => write!(f, "Ctrl+F{}", n),
            Key::Null => write!(f, "NULL"),
            Key::Esc => write!(f, "ESC"),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(untagged)]
pub enum CharOrArrow {
    Char(char),
    Direction(Direction),
}

impl fmt::Display for CharOrArrow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CharOrArrow::Char(c) => write!(f, "{}", Key::Char(*c)),
            CharOrArrow::Direction(d) => write!(f, "{}", d),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyWithModifier {
    bare_key: BareKey,
    key_modifiers: HashSet<KeyModifier>
}

#[derive(Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum BareKey {
    PageDown,
    PageUp,
    Left,
    Down,
    Up,
    Right,
    Home,
    End,
    Backspace,
    Delete,
    Insert,
    F(u8),
    Char(char),
    Tab,
    Esc,
    Enter,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    Menu,
}

#[derive(Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum KeyModifier {
    Shift,
    Alt,
    Ctrl,
    Super,
    Hyper,
    Meta,
    CapsLock,
    NumLock,
}

impl BareKey {
    pub fn from_bytes_with_u(bytes: &[u8]) -> Option<Self> {
        match str::from_utf8(bytes) {
            Ok("27") => Some(BareKey::Esc),
            Ok("13") => Some(BareKey::Enter),
            Ok("9") => Some(BareKey::Tab),
            Ok("127") => Some(BareKey::Backspace),
            Ok("57358") => Some(BareKey::CapsLock),
            Ok("57359") => Some(BareKey::ScrollLock),
            Ok("57360") => Some(BareKey::NumLock),
            Ok("57361") => Some(BareKey::PrintScreen),
            Ok("57362") => Some(BareKey::Pause),
            Ok("57363") => Some(BareKey::Menu),
            Ok(num) => {
                u8::from_str_radix(num, 10).ok().map(|n| BareKey::Char((n as char).to_ascii_lowercase()))
            }
            _ => None
        }
    }
    pub fn from_bytes_with_tilde(bytes: &[u8]) -> Option<Self> {
        match str::from_utf8(bytes) {
            Ok("2") => Some(BareKey::Insert),
            Ok("3") => Some(BareKey::Delete),
            Ok("5") => Some(BareKey::PageUp),
            Ok("6") => Some(BareKey::PageDown),
            Ok("7") => Some(BareKey::Home),
            Ok("8") => Some(BareKey::End),
            Ok("11") => Some(BareKey::F(1)),
            Ok("12") => Some(BareKey::F(2)),
            Ok("13") => Some(BareKey::F(3)),
            Ok("14") => Some(BareKey::F(4)),
            Ok("15") => Some(BareKey::F(5)),
            Ok("17") => Some(BareKey::F(6)),
            Ok("18") => Some(BareKey::F(7)),
            Ok("19") => Some(BareKey::F(8)),
            Ok("20") => Some(BareKey::F(9)),
            Ok("21") => Some(BareKey::F(10)),
            Ok("23") => Some(BareKey::F(11)),
            Ok("24") => Some(BareKey::F(12)),
            _ => None
        }
    }
    pub fn from_bytes_with_no_ending_byte(bytes: &[u8]) -> Option<Self> {
        match str::from_utf8(bytes) {
            Ok("1D") | Ok("D") => Some(BareKey::Left),
            Ok("1C") | Ok("C") => Some(BareKey::Right),
            Ok("1A") | Ok("A") => Some(BareKey::Up),
            Ok("1B") | Ok("B") => Some(BareKey::Down),
            Ok("1H") | Ok("H") => Some(BareKey::Home),
            Ok("1F") | Ok("F") => Some(BareKey::End),
            Ok("1P") | Ok("P") => Some(BareKey::F(1)),
            Ok("1Q") | Ok("Q") => Some(BareKey::F(2)),
            Ok("1S") | Ok("S") => Some(BareKey::F(4)),
            _ => None
        }
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct ModifierFlags: u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        const HYPER = 0b0001_0000;
        const META = 0b0010_0000;
        const CAPS_LOCK = 0b0100_0000;
        const NUM_LOCK = 0b1000_0000;
    }
}

impl KeyModifier {
    pub fn from_bytes(bytes: &[u8]) -> HashSet<KeyModifier> {
        let modifier_flags = str::from_utf8(bytes)
            .ok() // convert to string: (eg. "16")
            .and_then(|s| u8::from_str_radix(&s, 10).ok()) // convert to u8: (eg. 16)
            .map(|s| s.saturating_sub(1)) // subtract 1: (eg. 15)
            .and_then(|b| ModifierFlags::from_bits(b)); // bitflags: (0b0000_1111: Shift, Alt, Control, Super)
        let mut key_modifiers = HashSet::new();
        if let Some(modifier_flags) = modifier_flags {
            for name in modifier_flags.iter() {
                match name {
                    ModifierFlags::SHIFT => key_modifiers.insert(KeyModifier::Shift),
                    ModifierFlags::ALT => key_modifiers.insert(KeyModifier::Alt),
                    ModifierFlags::CONTROL => key_modifiers.insert(KeyModifier::Ctrl),
                    ModifierFlags::SUPER => key_modifiers.insert(KeyModifier::Super),
                    ModifierFlags::HYPER => key_modifiers.insert(KeyModifier::Hyper),
                    ModifierFlags::META => key_modifiers.insert(KeyModifier::Meta),
                    ModifierFlags::CAPS_LOCK => key_modifiers.insert(KeyModifier::CapsLock),
                    ModifierFlags::NUM_LOCK => key_modifiers.insert(KeyModifier::NumLock),
                    _ => false
                };
            }
        }
        key_modifiers
    }
}

impl KeyWithModifier {
    pub fn new(bare_key: BareKey) -> Self {
        KeyWithModifier {
            bare_key,
            key_modifiers: HashSet::new(),
        }
    }
    pub fn with_shift_modifier(mut self) -> Self {
        self.key_modifiers.insert(KeyModifier::Shift);
        self
    }
    pub fn with_alt_modifier(mut self) -> Self {
        self.key_modifiers.insert(KeyModifier::Alt);
        self
    }
    pub fn with_ctrl_modifier(mut self) -> Self {
        self.key_modifiers.insert(KeyModifier::Ctrl);
        self
    }
    pub fn with_super_modifier(mut self) -> Self {
        self.key_modifiers.insert(KeyModifier::Super);
        self
    }
    pub fn from_bytes_with_u(number_bytes: &[u8], modifier_bytes: &[u8]) -> Option<Self> {
        // CSI number ; modifiers u
        let bare_key = BareKey::from_bytes_with_u(number_bytes);
        match bare_key {
            Some(bare_key) => {
                let key_modifiers = KeyModifier::from_bytes(modifier_bytes);
                Some(KeyWithModifier {
                    bare_key,
                    key_modifiers
                })
            }
            _ => None
        }
    }
    pub fn from_bytes_with_tilde(number_bytes: &[u8], modifier_bytes: &[u8]) -> Option<Self> {
        // CSI number ; modifiers ~
        let bare_key = BareKey::from_bytes_with_tilde(number_bytes);
        match bare_key {
            Some(bare_key) => {
                let key_modifiers = KeyModifier::from_bytes(modifier_bytes);
                Some(KeyWithModifier {
                    bare_key,
                    key_modifiers
                })
            }
            _ => None
        }
    }
    pub fn from_bytes_with_no_ending_byte(number_bytes: &[u8], modifier_bytes: &[u8]) -> Option<Self> {
        // CSI 1; modifiers [ABCDEFHPQS]
        let bare_key = BareKey::from_bytes_with_no_ending_byte(number_bytes);
        match bare_key {
            Some(bare_key) => {
                let key_modifiers = KeyModifier::from_bytes(modifier_bytes);
                Some(KeyWithModifier {
                    bare_key,
                    key_modifiers
                })
            }
            _ => None
        }
    }
}

#[derive(Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

impl Direction {
    pub fn invert(&self) -> Direction {
        match *self {
            Direction::Left => Direction::Right,
            Direction::Down => Direction::Up,
            Direction::Up => Direction::Down,
            Direction::Right => Direction::Left,
        }
    }

    pub fn is_horizontal(&self) -> bool {
        matches!(self, Direction::Left | Direction::Right)
    }

    pub fn is_vertical(&self) -> bool {
        matches!(self, Direction::Down | Direction::Up)
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Direction::Left => write!(f, "←"),
            Direction::Right => write!(f, "→"),
            Direction::Up => write!(f, "↑"),
            Direction::Down => write!(f, "↓"),
        }
    }
}

impl FromStr for Direction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Left" | "left" => Ok(Direction::Left),
            "Right" | "right" => Ok(Direction::Right),
            "Up" | "up" => Ok(Direction::Up),
            "Down" | "down" => Ok(Direction::Down),
            _ => Err(format!(
                "Failed to parse Direction. Unknown Direction: {}",
                s
            )),
        }
    }
}

/// Resize operation to perform.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub enum Resize {
    Increase,
    Decrease,
}

impl Resize {
    pub fn invert(&self) -> Self {
        match self {
            Resize::Increase => Resize::Decrease,
            Resize::Decrease => Resize::Increase,
        }
    }
}

impl fmt::Display for Resize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Resize::Increase => write!(f, "+"),
            Resize::Decrease => write!(f, "-"),
        }
    }
}

impl FromStr for Resize {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Increase" | "increase" | "+" => Ok(Resize::Increase),
            "Decrease" | "decrease" | "-" => Ok(Resize::Decrease),
            _ => Err(format!(
                "failed to parse resize type. Unknown specifier '{}'",
                s
            )),
        }
    }
}

/// Container type that fully describes resize operations.
///
/// This is best thought of as follows:
///
/// - `resize` commands how the total *area* of the pane will change as part of this resize
///   operation.
/// - `direction` has two meanings:
///     - `None` means to resize all borders equally
///     - Anything else means to move the named border to achieve the change in area
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct ResizeStrategy {
    /// Whether to increase or resize total area
    pub resize: Resize,
    /// With which border, if any, to change area
    pub direction: Option<Direction>,
    /// If set to true (default), increasing resizes towards a viewport border will be inverted.
    /// I.e. a scenario like this ("increase right"):
    ///
    /// ```text
    /// +---+---+
    /// |   | X |->
    /// +---+---+
    /// ```
    ///
    /// turns into this ("decrease left"):
    ///
    /// ```text
    /// +---+---+
    /// |   |-> |
    /// +---+---+
    /// ```
    pub invert_on_boundaries: bool,
}

impl From<Direction> for ResizeStrategy {
    fn from(direction: Direction) -> Self {
        ResizeStrategy::new(Resize::Increase, Some(direction))
    }
}

impl From<Resize> for ResizeStrategy {
    fn from(resize: Resize) -> Self {
        ResizeStrategy::new(resize, None)
    }
}

impl ResizeStrategy {
    pub fn new(resize: Resize, direction: Option<Direction>) -> Self {
        ResizeStrategy {
            resize,
            direction,
            invert_on_boundaries: true,
        }
    }

    pub fn invert(&self) -> ResizeStrategy {
        let resize = match self.resize {
            Resize::Increase => Resize::Decrease,
            Resize::Decrease => Resize::Increase,
        };
        let direction = match self.direction {
            Some(direction) => Some(direction.invert()),
            None => None,
        };

        ResizeStrategy::new(resize, direction)
    }

    pub fn resize_type(&self) -> Resize {
        self.resize
    }

    pub fn direction(&self) -> Option<Direction> {
        self.direction
    }

    pub fn direction_horizontal(&self) -> bool {
        matches!(
            self.direction,
            Some(Direction::Left) | Some(Direction::Right)
        )
    }

    pub fn direction_vertical(&self) -> bool {
        matches!(self.direction, Some(Direction::Up) | Some(Direction::Down))
    }

    pub fn resize_increase(&self) -> bool {
        self.resize == Resize::Increase
    }

    pub fn resize_decrease(&self) -> bool {
        self.resize == Resize::Decrease
    }

    pub fn move_left_border_left(&self) -> bool {
        (self.resize == Resize::Increase) && (self.direction == Some(Direction::Left))
    }

    pub fn move_left_border_right(&self) -> bool {
        (self.resize == Resize::Decrease) && (self.direction == Some(Direction::Left))
    }

    pub fn move_lower_border_down(&self) -> bool {
        (self.resize == Resize::Increase) && (self.direction == Some(Direction::Down))
    }

    pub fn move_lower_border_up(&self) -> bool {
        (self.resize == Resize::Decrease) && (self.direction == Some(Direction::Down))
    }

    pub fn move_upper_border_up(&self) -> bool {
        (self.resize == Resize::Increase) && (self.direction == Some(Direction::Up))
    }

    pub fn move_upper_border_down(&self) -> bool {
        (self.resize == Resize::Decrease) && (self.direction == Some(Direction::Up))
    }

    pub fn move_right_border_right(&self) -> bool {
        (self.resize == Resize::Increase) && (self.direction == Some(Direction::Right))
    }

    pub fn move_right_border_left(&self) -> bool {
        (self.resize == Resize::Decrease) && (self.direction == Some(Direction::Right))
    }

    pub fn move_all_borders_out(&self) -> bool {
        (self.resize == Resize::Increase) && (self.direction == None)
    }

    pub fn move_all_borders_in(&self) -> bool {
        (self.resize == Resize::Decrease) && (self.direction == None)
    }
}

impl fmt::Display for ResizeStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resize = match self.resize {
            Resize::Increase => "increase",
            Resize::Decrease => "decrease",
        };
        let border = match self.direction {
            Some(Direction::Left) => "left",
            Some(Direction::Down) => "bottom",
            Some(Direction::Up) => "top",
            Some(Direction::Right) => "right",
            None => "every",
        };

        write!(f, "{} size on {} border", resize, border)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
// FIXME: This should be extended to handle different button clicks (not just
// left click) and the `ScrollUp` and `ScrollDown` events could probably be
// merged into a single `Scroll(isize)` event.
pub enum Mouse {
    ScrollUp(usize),          // number of lines
    ScrollDown(usize),        // number of lines
    LeftClick(isize, usize),  // line and column
    RightClick(isize, usize), // line and column
    Hold(isize, usize),       // line and column
    Release(isize, usize),    // line and column
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileMetadata {
    pub is_dir: bool,
    pub is_file: bool,
    pub is_symlink: bool,
    pub len: u64,
}

impl From<Metadata> for FileMetadata {
    fn from(metadata: Metadata) -> Self {
        FileMetadata {
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            is_symlink: metadata.is_symlink(),
            len: metadata.len(),
        }
    }
}

/// These events can be subscribed to with subscribe method exported by `zellij-tile`.
/// Once subscribed to, they will trigger the `update` method of the `ZellijPlugin` trait.
#[derive(Debug, Clone, PartialEq, EnumDiscriminants, ToString, Serialize, Deserialize)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(EventType))]
#[non_exhaustive]
pub enum Event {
    ModeUpdate(ModeInfo),
    TabUpdate(Vec<TabInfo>),
    PaneUpdate(PaneManifest),
    /// A key was pressed while the user is focused on this plugin's pane
    Key(Key),
    /// A mouse event happened while the user is focused on this plugin's pane
    Mouse(Mouse),
    /// A timer expired set by the `set_timeout` method exported by `zellij-tile`.
    Timer(f64),
    /// Text was copied to the clipboard anywhere in the app
    CopyToClipboard(CopyDestination),
    /// Failed to copy text to clipboard anywhere in the app
    SystemClipboardFailure,
    /// Input was received anywhere in the app
    InputReceived,
    /// This plugin became visible or invisible
    Visible(bool),
    /// A message from one of the plugin's workers
    CustomMessage(
        String, // message
        String, // payload
    ),
    /// A file was created somewhere in the Zellij CWD folder
    FileSystemCreate(Vec<(PathBuf, Option<FileMetadata>)>),
    /// A file was accessed somewhere in the Zellij CWD folder
    FileSystemRead(Vec<(PathBuf, Option<FileMetadata>)>),
    /// A file was modified somewhere in the Zellij CWD folder
    FileSystemUpdate(Vec<(PathBuf, Option<FileMetadata>)>),
    /// A file was deleted somewhere in the Zellij CWD folder
    FileSystemDelete(Vec<(PathBuf, Option<FileMetadata>)>),
    /// A Result of plugin permission request
    PermissionRequestResult(PermissionStatus),
    SessionUpdate(
        Vec<SessionInfo>,
        Vec<(String, Duration)>, // resurrectable sessions
    ),
    RunCommandResult(Option<i32>, Vec<u8>, Vec<u8>, BTreeMap<String, String>), // exit_code, STDOUT, STDERR,
    // context
    WebRequestResult(
        u16,
        BTreeMap<String, String>,
        Vec<u8>,
        BTreeMap<String, String>,
    ), // status,
       // headers,
       // body,
       // context
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    Copy,
    Clone,
    EnumDiscriminants,
    ToString,
    Serialize,
    Deserialize,
    PartialOrd,
    Ord,
)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize, Display, PartialOrd, Ord))]
#[strum_discriminants(name(PermissionType))]
#[non_exhaustive]
pub enum Permission {
    ReadApplicationState,
    ChangeApplicationState,
    OpenFiles,
    RunCommands,
    OpenTerminalsOrPlugins,
    WriteToStdin,
    WebAccess,
    ReadCliPipes,
    MessageAndLaunchOtherPlugins,
}

impl PermissionType {
    pub fn display_name(&self) -> String {
        match self {
            PermissionType::ReadApplicationState => {
                "Access Zellij state (Panes, Tabs and UI)".to_owned()
            },
            PermissionType::ChangeApplicationState => {
                "Change Zellij state (Panes, Tabs and UI)".to_owned()
            },
            PermissionType::OpenFiles => "Open files (eg. for editing)".to_owned(),
            PermissionType::RunCommands => "Run commands".to_owned(),
            PermissionType::OpenTerminalsOrPlugins => "Start new terminals and plugins".to_owned(),
            PermissionType::WriteToStdin => "Write to standard input (STDIN)".to_owned(),
            PermissionType::WebAccess => "Make web requests".to_owned(),
            PermissionType::ReadCliPipes => "Control command line pipes and output".to_owned(),
            PermissionType::MessageAndLaunchOtherPlugins => {
                "Send messages to and launch other plugins".to_owned()
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct PluginPermission {
    pub name: String,
    pub permissions: Vec<PermissionType>,
}

impl PluginPermission {
    pub fn new(name: String, permissions: Vec<PermissionType>) -> Self {
        PluginPermission { name, permissions }
    }
}

/// Describes the different input modes, which change the way that keystrokes will be interpreted.
#[derive(
    Debug,
    PartialEq,
    Eq,
    Hash,
    Copy,
    Clone,
    EnumIter,
    Serialize,
    Deserialize,
    ArgEnum,
    PartialOrd,
    Ord,
)]
pub enum InputMode {
    /// In `Normal` mode, input is always written to the terminal, except for the shortcuts leading
    /// to other modes
    #[serde(alias = "normal")]
    Normal,
    /// In `Locked` mode, input is always written to the terminal and all shortcuts are disabled
    /// except the one leading back to normal mode
    #[serde(alias = "locked")]
    Locked,
    /// `Resize` mode allows resizing the different existing panes.
    #[serde(alias = "resize")]
    Resize,
    /// `Pane` mode allows creating and closing panes, as well as moving between them.
    #[serde(alias = "pane")]
    Pane,
    /// `Tab` mode allows creating and closing tabs, as well as moving between them.
    #[serde(alias = "tab")]
    Tab,
    /// `Scroll` mode allows scrolling up and down within a pane.
    #[serde(alias = "scroll")]
    Scroll,
    /// `EnterSearch` mode allows for typing in the needle for a search in the scroll buffer of a pane.
    #[serde(alias = "entersearch")]
    EnterSearch,
    /// `Search` mode allows for searching a term in a pane (superset of `Scroll`).
    #[serde(alias = "search")]
    Search,
    /// `RenameTab` mode allows assigning a new name to a tab.
    #[serde(alias = "renametab")]
    RenameTab,
    /// `RenamePane` mode allows assigning a new name to a pane.
    #[serde(alias = "renamepane")]
    RenamePane,
    /// `Session` mode allows detaching sessions
    #[serde(alias = "session")]
    Session,
    /// `Move` mode allows moving the different existing panes within a tab
    #[serde(alias = "move")]
    Move,
    /// `Prompt` mode allows interacting with active prompts.
    #[serde(alias = "prompt")]
    Prompt,
    /// `Tmux` mode allows for basic tmux keybindings functionality
    #[serde(alias = "tmux")]
    Tmux,
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ThemeHue {
    Light,
    Dark,
}
impl Default for ThemeHue {
    fn default() -> ThemeHue {
        ThemeHue::Dark
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PaletteColor {
    Rgb((u8, u8, u8)),
    EightBit(u8),
}
impl Default for PaletteColor {
    fn default() -> PaletteColor {
        PaletteColor::EightBit(0)
    }
}

impl FromStr for InputMode {
    type Err = ConversionError;

    fn from_str(s: &str) -> Result<Self, ConversionError> {
        match s {
            "normal" | "Normal" => Ok(InputMode::Normal),
            "locked" | "Locked" => Ok(InputMode::Locked),
            "resize" | "Resize" => Ok(InputMode::Resize),
            "pane" | "Pane" => Ok(InputMode::Pane),
            "tab" | "Tab" => Ok(InputMode::Tab),
            "search" | "Search" => Ok(InputMode::Search),
            "scroll" | "Scroll" => Ok(InputMode::Scroll),
            "renametab" | "RenameTab" => Ok(InputMode::RenameTab),
            "renamepane" | "RenamePane" => Ok(InputMode::RenamePane),
            "session" | "Session" => Ok(InputMode::Session),
            "move" | "Move" => Ok(InputMode::Move),
            "prompt" | "Prompt" => Ok(InputMode::Prompt),
            "tmux" | "Tmux" => Ok(InputMode::Tmux),
            "entersearch" | "Entersearch" | "EnterSearch" => Ok(InputMode::EnterSearch),
            e => Err(ConversionError::UnknownInputMode(e.into())),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PaletteSource {
    Default,
    Xresources,
}
impl Default for PaletteSource {
    fn default() -> PaletteSource {
        PaletteSource::Default
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
pub struct Palette {
    pub source: PaletteSource,
    pub theme_hue: ThemeHue,
    pub fg: PaletteColor,
    pub bg: PaletteColor,
    pub black: PaletteColor,
    pub red: PaletteColor,
    pub green: PaletteColor,
    pub yellow: PaletteColor,
    pub blue: PaletteColor,
    pub magenta: PaletteColor,
    pub cyan: PaletteColor,
    pub white: PaletteColor,
    pub orange: PaletteColor,
    pub gray: PaletteColor,
    pub purple: PaletteColor,
    pub gold: PaletteColor,
    pub silver: PaletteColor,
    pub pink: PaletteColor,
    pub brown: PaletteColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Style {
    pub colors: Palette,
    pub rounded_corners: bool,
    pub hide_session_name: bool,
}

// FIXME: Poor devs hashtable since HashTable can't derive `Default`...
pub type KeybindsVec = Vec<(InputMode, Vec<(Key, Vec<Action>)>)>;

/// Provides information helpful in rendering the Zellij controls for UI bars
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub mode: InputMode,
    pub keybinds: KeybindsVec,
    pub style: Style,
    pub capabilities: PluginCapabilities,
    pub session_name: Option<String>,
}

impl ModeInfo {
    pub fn get_mode_keybinds(&self) -> Vec<(Key, Vec<Action>)> {
        self.get_keybinds_for_mode(self.mode)
    }

    pub fn get_keybinds_for_mode(&self, mode: InputMode) -> Vec<(Key, Vec<Action>)> {
        for (vec_mode, map) in &self.keybinds {
            if mode == *vec_mode {
                return map.to_vec();
            }
        }
        vec![]
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct SessionInfo {
    pub name: String,
    pub tabs: Vec<TabInfo>,
    pub panes: PaneManifest,
    pub connected_clients: usize,
    pub is_current_session: bool,
    pub available_layouts: Vec<LayoutInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum LayoutInfo {
    BuiltIn(String),
    File(String),
}

impl LayoutInfo {
    pub fn name(&self) -> &str {
        match self {
            LayoutInfo::BuiltIn(name) => &name,
            LayoutInfo::File(name) => &name,
        }
    }
    pub fn is_builtin(&self) -> bool {
        match self {
            LayoutInfo::BuiltIn(_name) => true,
            LayoutInfo::File(_name) => false,
        }
    }
}

use std::hash::{Hash, Hasher};

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for SessionInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl SessionInfo {
    pub fn new(name: String) -> Self {
        SessionInfo {
            name,
            ..Default::default()
        }
    }
    pub fn update_tab_info(&mut self, new_tab_info: Vec<TabInfo>) {
        self.tabs = new_tab_info;
    }
    pub fn update_pane_info(&mut self, new_pane_info: PaneManifest) {
        self.panes = new_pane_info;
    }
    pub fn update_connected_clients(&mut self, new_connected_clients: usize) {
        self.connected_clients = new_connected_clients;
    }
}

/// Contains all the information for a currently opened tab.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct TabInfo {
    /// The Tab's 0 indexed position
    pub position: usize,
    /// The name of the tab as it appears in the UI (if there's enough room for it)
    pub name: String,
    /// Whether this tab is focused
    pub active: bool,
    /// The number of suppressed panes this tab has
    pub panes_to_hide: usize,
    /// Whether there's one pane taking up the whole display area on this tab
    pub is_fullscreen_active: bool,
    /// Whether input sent to this tab will be synced to all panes in it
    pub is_sync_panes_active: bool,
    pub are_floating_panes_visible: bool,
    pub other_focused_clients: Vec<ClientId>,
    pub active_swap_layout_name: Option<String>,
    /// Whether the user manually changed the layout, moving out of the swap layout scheme
    pub is_swap_layout_dirty: bool,
}

/// The `PaneManifest` contains a dictionary of panes, indexed by the tab position (0 indexed).
/// Panes include all panes in the relevant tab, including `tiled` panes, `floating` panes and
/// `suppressed` panes.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaneManifest {
    pub panes: HashMap<usize, Vec<PaneInfo>>, // usize is the tab position
}

/// Contains all the information for a currently open pane
///
/// # Difference between coordinates/size and content coordinates/size
///
/// The pane basic coordinates and size (eg. `pane_x` or `pane_columns`) are the entire space taken
/// up by this pane - including its frame and title if it has a border.
///
/// The pane content coordinates and size (eg. `pane_content_x` or `pane_content_columns`)
/// represent the area taken by the pane's content, excluding its frame and title if it has a
/// border.
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PaneInfo {
    /// The id of the pane, unique to all panes of this kind (eg. id in terminals or id in panes)
    pub id: u32,
    /// Whether this pane is a plugin (`true`) or a terminal (`false`), used along with `id` can represent a unique pane ID across
    /// the running session
    pub is_plugin: bool,
    /// Whether the pane is focused in its layer (tiled or floating)
    pub is_focused: bool,
    pub is_fullscreen: bool,
    /// Whether a pane is floating or tiled (embedded)
    pub is_floating: bool,
    /// Whether a pane is suppressed - suppressed panes are not visible to the user, but still run
    /// in the background
    pub is_suppressed: bool,
    /// The full title of the pane as it appears in the UI (if there is room for it)
    pub title: String,
    /// Whether a pane exited or not, note that most panes close themselves before setting this
    /// flag, so this is only relevant to command panes
    pub exited: bool,
    /// The exit status of a pane if it did exit and is still in the UI
    pub exit_status: Option<i32>,
    /// A "held" pane is a paused pane that is waiting for user input (eg. a command pane that
    /// exited and is waiting to be re-run or closed)
    pub is_held: bool,
    pub pane_x: usize,
    pub pane_content_x: usize,
    pub pane_y: usize,
    pub pane_content_y: usize,
    pub pane_rows: usize,
    pub pane_content_rows: usize,
    pub pane_columns: usize,
    pub pane_content_columns: usize,
    /// The coordinates of the cursor - if this pane is focused - relative to the pane's
    /// coordinates
    pub cursor_coordinates_in_pane: Option<(usize, usize)>, // x, y if cursor is visible
    /// If this is a command pane, this will show the stringified version of the command and its
    /// arguments
    pub terminal_command: Option<String>,
    /// The URL from which this plugin was loaded (eg. `zellij:strider` for the built-in `strider`
    /// plugin or `file:/path/to/my/plugin.wasm` for a local plugin)
    pub plugin_url: Option<String>,
    /// Unselectable panes are often used for UI elements that do not have direct user interaction
    /// (eg. the default `status-bar` or `tab-bar`).
    pub is_selectable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginIds {
    pub plugin_id: u32,
    pub zellij_pid: u32,
    pub initial_cwd: PathBuf,
}

/// Tag used to identify the plugin in layout and config kdl files
#[derive(Debug, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd, Ord)]
pub struct PluginTag(String);

impl PluginTag {
    pub fn new(url: impl Into<String>) -> Self {
        PluginTag(url.into())
    }
}

impl From<PluginTag> for String {
    fn from(tag: PluginTag) -> Self {
        tag.0
    }
}

impl fmt::Display for PluginTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginCapabilities {
    pub arrow_fonts: bool,
}

impl Default for PluginCapabilities {
    fn default() -> PluginCapabilities {
        PluginCapabilities { arrow_fonts: true }
    }
}

/// Represents a Clipboard type
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum CopyDestination {
    Command,
    Primary,
    System,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum PermissionStatus {
    Granted,
    Denied,
}

#[derive(Debug, Default, Clone)]
pub struct FileToOpen {
    pub path: PathBuf,
    pub line_number: Option<usize>,
    pub cwd: Option<PathBuf>,
}

impl FileToOpen {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        FileToOpen {
            path: path.as_ref().to_path_buf(),
            ..Default::default()
        }
    }
    pub fn with_line_number(mut self, line_number: usize) -> Self {
        self.line_number = Some(line_number);
        self
    }
    pub fn with_cwd(mut self, cwd: PathBuf) -> Self {
        self.cwd = Some(cwd);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct CommandToRun {
    pub path: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

impl CommandToRun {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        CommandToRun {
            path: path.as_ref().to_path_buf(),
            ..Default::default()
        }
    }
    pub fn new_with_args<P: AsRef<Path>, A: AsRef<str>>(path: P, args: Vec<A>) -> Self {
        CommandToRun {
            path: path.as_ref().to_path_buf(),
            args: args.into_iter().map(|a| a.as_ref().to_owned()).collect(),
            ..Default::default()
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct MessageToPlugin {
    pub plugin_url: Option<String>,
    pub destination_plugin_id: Option<u32>,
    pub plugin_config: BTreeMap<String, String>,
    pub message_name: String,
    pub message_payload: Option<String>,
    pub message_args: BTreeMap<String, String>,
    /// these will only be used in case we need to launch a new plugin to send this message to,
    /// since none are running
    pub new_plugin_args: Option<NewPluginArgs>,
}

#[derive(Debug, Default, Clone)]
pub struct NewPluginArgs {
    pub should_float: Option<bool>,
    pub pane_id_to_replace: Option<PaneId>,
    pub pane_title: Option<String>,
    pub cwd: Option<PathBuf>,
    pub skip_cache: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum PaneId {
    Terminal(u32),
    Plugin(u32),
}

impl MessageToPlugin {
    pub fn new(message_name: impl Into<String>) -> Self {
        MessageToPlugin {
            message_name: message_name.into(),
            ..Default::default()
        }
    }
    pub fn with_plugin_url(mut self, url: impl Into<String>) -> Self {
        self.plugin_url = Some(url.into());
        self
    }
    pub fn with_destination_plugin_id(mut self, destination_plugin_id: u32) -> Self {
        self.destination_plugin_id = Some(destination_plugin_id);
        self
    }
    pub fn with_plugin_config(mut self, plugin_config: BTreeMap<String, String>) -> Self {
        self.plugin_config = plugin_config;
        self
    }
    pub fn with_payload(mut self, payload: impl Into<String>) -> Self {
        self.message_payload = Some(payload.into());
        self
    }
    pub fn with_args(mut self, args: BTreeMap<String, String>) -> Self {
        self.message_args = args;
        self
    }
    pub fn new_plugin_instance_should_float(mut self, should_float: bool) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.should_float = Some(should_float);
        self
    }
    pub fn new_plugin_instance_should_replace_pane(mut self, pane_id: PaneId) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.pane_id_to_replace = Some(pane_id);
        self
    }
    pub fn new_plugin_instance_should_have_pane_title(
        mut self,
        pane_title: impl Into<String>,
    ) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.pane_title = Some(pane_title.into());
        self
    }
    pub fn new_plugin_instance_should_have_cwd(mut self, cwd: PathBuf) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.cwd = Some(cwd);
        self
    }
    pub fn new_plugin_instance_should_skip_cache(mut self) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.skip_cache = true;
        self
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConnectToSession {
    pub name: Option<String>,
    pub tab_position: Option<usize>,
    pub pane_id: Option<(u32, bool)>, // (id, is_plugin)
    pub layout: Option<LayoutInfo>,
    pub cwd: Option<PathBuf>,
}

impl ConnectToSession {
    pub fn apply_layout_dir(&mut self, layout_dir: &PathBuf) {
        if let Some(LayoutInfo::File(file_path)) = self.layout.as_mut() {
            *file_path = Path::join(layout_dir, &file_path)
                .to_string_lossy()
                .to_string();
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct PluginMessage {
    pub name: String,
    pub payload: String,
    pub worker_name: Option<String>,
}

impl PluginMessage {
    pub fn new_to_worker(worker_name: &str, message: &str, payload: &str) -> Self {
        PluginMessage {
            name: message.to_owned(),
            payload: payload.to_owned(),
            worker_name: Some(worker_name.to_owned()),
        }
    }
    pub fn new_to_plugin(message: &str, payload: &str) -> Self {
        PluginMessage {
            name: message.to_owned(),
            payload: payload.to_owned(),
            worker_name: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVerb {
    Get,
    Post,
    Put,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PipeSource {
    Cli(String), // String is the pipe_id of the CLI pipe (used for blocking/unblocking)
    Plugin(u32), // u32 is the lugin id
    Keybind,     // TODO: consider including the actual keybind here?
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PipeMessage {
    pub source: PipeSource,
    pub name: String,
    pub payload: Option<String>,
    pub args: BTreeMap<String, String>,
    pub is_private: bool,
}

impl PipeMessage {
    pub fn new(
        source: PipeSource,
        name: impl Into<String>,
        payload: &Option<String>,
        args: &Option<BTreeMap<String, String>>,
        is_private: bool,
    ) -> Self {
        PipeMessage {
            source,
            name: name.into(),
            payload: payload.clone(),
            args: args.clone().unwrap_or_else(|| Default::default()),
            is_private,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, Default)]
pub struct FloatingPaneCoordinates {
    pub x: Option<SplitSize>,
    pub y: Option<SplitSize>,
    pub width: Option<SplitSize>,
    pub height: Option<SplitSize>,
}

impl FloatingPaneCoordinates {
    pub fn new(
        x: Option<String>,
        y: Option<String>,
        width: Option<String>,
        height: Option<String>,
    ) -> Option<Self> {
        let x = x.and_then(|x| SplitSize::from_str(&x).ok());
        let y = y.and_then(|y| SplitSize::from_str(&y).ok());
        let width = width.and_then(|width| SplitSize::from_str(&width).ok());
        let height = height.and_then(|height| SplitSize::from_str(&height).ok());
        if x.is_none() && y.is_none() && width.is_none() && height.is_none() {
            None
        } else {
            Some(FloatingPaneCoordinates {
                x,
                y,
                width,
                height,
            })
        }
    }
    pub fn with_x_fixed(mut self, x: usize) -> Self {
        self.x = Some(SplitSize::Fixed(x));
        self
    }
    pub fn with_x_percent(mut self, x: usize) -> Self {
        if x > 100 {
            eprintln!("x must be between 0 and 100");
            return self;
        }
        self.x = Some(SplitSize::Percent(x));
        self
    }
    pub fn with_y_fixed(mut self, y: usize) -> Self {
        self.y = Some(SplitSize::Fixed(y));
        self
    }
    pub fn with_y_percent(mut self, y: usize) -> Self {
        if y > 100 {
            eprintln!("y must be between 0 and 100");
            return self;
        }
        self.y = Some(SplitSize::Percent(y));
        self
    }
    pub fn with_width_fixed(mut self, width: usize) -> Self {
        self.width = Some(SplitSize::Fixed(width));
        self
    }
    pub fn with_width_percent(mut self, width: usize) -> Self {
        if width > 100 {
            eprintln!("width must be between 0 and 100");
            return self;
        }
        self.width = Some(SplitSize::Percent(width));
        self
    }
    pub fn with_height_fixed(mut self, height: usize) -> Self {
        self.height = Some(SplitSize::Fixed(height));
        self
    }
    pub fn with_height_percent(mut self, height: usize) -> Self {
        if height > 100 {
            eprintln!("height must be between 0 and 100");
            return self;
        }
        self.height = Some(SplitSize::Percent(height));
        self
    }
}

#[derive(Debug, Clone, EnumDiscriminants, ToString)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(CommandType))]
pub enum PluginCommand {
    Subscribe(HashSet<EventType>),
    Unsubscribe(HashSet<EventType>),
    SetSelectable(bool),
    GetPluginIds,
    GetZellijVersion,
    OpenFile(FileToOpen),
    OpenFileFloating(FileToOpen, Option<FloatingPaneCoordinates>),
    OpenTerminal(FileToOpen), // only used for the path as cwd
    OpenTerminalFloating(FileToOpen, Option<FloatingPaneCoordinates>), // only used for the path as cwd
    OpenCommandPane(CommandToRun),
    OpenCommandPaneFloating(CommandToRun, Option<FloatingPaneCoordinates>),
    SwitchTabTo(u32), // tab index
    SetTimeout(f64),  // seconds
    ExecCmd(Vec<String>),
    PostMessageTo(PluginMessage),
    PostMessageToPlugin(PluginMessage),
    HideSelf,
    ShowSelf(bool), // bool - should float if hidden
    SwitchToMode(InputMode),
    NewTabsWithLayout(String), // raw kdl layout
    NewTab,
    GoToNextTab,
    GoToPreviousTab,
    Resize(Resize),
    ResizeWithDirection(ResizeStrategy),
    FocusNextPane,
    FocusPreviousPane,
    MoveFocus(Direction),
    MoveFocusOrTab(Direction),
    Detach,
    EditScrollback,
    Write(Vec<u8>), // bytes
    WriteChars(String),
    ToggleTab,
    MovePane,
    MovePaneWithDirection(Direction),
    ClearScreen,
    ScrollUp,
    ScrollDown,
    ScrollToTop,
    ScrollToBottom,
    PageScrollUp,
    PageScrollDown,
    ToggleFocusFullscreen,
    TogglePaneFrames,
    TogglePaneEmbedOrEject,
    UndoRenamePane,
    CloseFocus,
    ToggleActiveTabSync,
    CloseFocusedTab,
    UndoRenameTab,
    QuitZellij,
    PreviousSwapLayout,
    NextSwapLayout,
    GoToTabName(String),
    FocusOrCreateTab(String),
    GoToTab(u32),                    // tab index
    StartOrReloadPlugin(String),     // plugin url (eg. file:/path/to/plugin.wasm)
    CloseTerminalPane(u32),          // terminal pane id
    ClosePluginPane(u32),            // plugin pane id
    FocusTerminalPane(u32, bool),    // terminal pane id, should_float_if_hidden
    FocusPluginPane(u32, bool),      // plugin pane id, should_float_if_hidden
    RenameTerminalPane(u32, String), // terminal pane id, new name
    RenamePluginPane(u32, String),   // plugin pane id, new name
    RenameTab(u32, String),          // tab index, new name
    ReportPanic(String),             // stringified panic
    RequestPluginPermissions(Vec<PermissionType>),
    SwitchSession(ConnectToSession),
    DeleteDeadSession(String),       // String -> session name
    DeleteAllDeadSessions,           // String -> session name
    OpenTerminalInPlace(FileToOpen), // only used for the path as cwd
    OpenFileInPlace(FileToOpen),
    OpenCommandPaneInPlace(CommandToRun),
    RunCommand(
        Vec<String>,              // command
        BTreeMap<String, String>, // env_variables
        PathBuf,                  // cwd
        BTreeMap<String, String>, // context
    ),
    WebRequest(
        String, // url
        HttpVerb,
        BTreeMap<String, String>, // headers
        Vec<u8>,                  // body
        BTreeMap<String, String>, // context
    ),
    RenameSession(String),         // String -> new session name
    UnblockCliPipeInput(String),   // String => pipe name
    BlockCliPipeInput(String),     // String => pipe name
    CliPipeOutput(String, String), // String => pipe name, String => output
    MessageToPlugin(MessageToPlugin),
    DisconnectOtherClients,
    KillSessions(Vec<String>), // one or more session names
    ScanHostFolder(PathBuf),   // TODO: rename to ScanHostFolder
    WatchFilesystem,
    DumpSessionLayout,
    CloseSelf,
    NewTabsWithLayoutInfo(LayoutInfo),
}
