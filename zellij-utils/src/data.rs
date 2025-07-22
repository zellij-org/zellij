use crate::home::default_layout_dir;
use crate::input::actions::Action;
use crate::input::config::ConversionError;
use crate::input::keybinds::Keybinds;
use crate::input::layout::{RunPlugin, SplitSize};
use crate::pane_size::PaneGeom;
use crate::shared::{colors as default_colors, eightbit_to_rgb};
use clap::ArgEnum;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs::Metadata;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};
use std::time::Duration;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString, ToString};

#[cfg(not(target_family = "wasm"))]
use termwiz::{
    escape::csi::KittyKeyboardFlags,
    input::{KeyCode, KeyCodeEncodeModes, KeyboardEncoding, Modifiers},
};

pub type ClientId = u16; // TODO: merge with crate type?

pub fn client_id_to_colors(
    client_id: ClientId,
    colors: MultiplayerColors,
) -> Option<(PaletteColor, PaletteColor)> {
    // (primary color, secondary color)
    let black = PaletteColor::EightBit(default_colors::BLACK);
    match client_id {
        1 => Some((colors.player_1, black)),
        2 => Some((colors.player_2, black)),
        3 => Some((colors.player_3, black)),
        4 => Some((colors.player_4, black)),
        5 => Some((colors.player_5, black)),
        6 => Some((colors.player_6, black)),
        7 => Some((colors.player_7, black)),
        8 => Some((colors.player_8, black)),
        9 => Some((colors.player_9, black)),
        10 => Some((colors.player_10, black)),
        _ => None,
    }
}

pub fn single_client_color(colors: Palette) -> (PaletteColor, PaletteColor) {
    (colors.green, colors.black)
}

impl FromStr for KeyWithModifier {
    type Err = Box<dyn std::error::Error>;
    fn from_str(key_str: &str) -> Result<Self, Self::Err> {
        let mut key_string_parts: Vec<&str> = key_str.split_ascii_whitespace().collect();
        let bare_key: BareKey = BareKey::from_str(key_string_parts.pop().ok_or("empty key")?)?;
        let mut key_modifiers: BTreeSet<KeyModifier> = BTreeSet::new();
        for stringified_modifier in key_string_parts {
            key_modifiers.insert(KeyModifier::from_str(stringified_modifier)?);
        }
        Ok(KeyWithModifier {
            bare_key,
            key_modifiers,
        })
    }
}

#[derive(Debug, Clone, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct KeyWithModifier {
    pub bare_key: BareKey,
    pub key_modifiers: BTreeSet<KeyModifier>,
}

impl PartialEq for KeyWithModifier {
    fn eq(&self, other: &Self) -> bool {
        match (self.bare_key, other.bare_key) {
            (BareKey::Char(self_char), BareKey::Char(other_char))
                if self_char.to_ascii_lowercase() == other_char.to_ascii_lowercase() =>
            {
                let mut self_cloned = self.clone();
                let mut other_cloned = other.clone();
                if self_char.is_ascii_uppercase() {
                    self_cloned.bare_key = BareKey::Char(self_char.to_ascii_lowercase());
                    self_cloned.key_modifiers.insert(KeyModifier::Shift);
                }
                if other_char.is_ascii_uppercase() {
                    other_cloned.bare_key = BareKey::Char(self_char.to_ascii_lowercase());
                    other_cloned.key_modifiers.insert(KeyModifier::Shift);
                }
                self_cloned.bare_key == other_cloned.bare_key
                    && self_cloned.key_modifiers == other_cloned.key_modifiers
            },
            _ => self.bare_key == other.bare_key && self.key_modifiers == other.key_modifiers,
        }
    }
}

impl Hash for KeyWithModifier {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self.bare_key {
            BareKey::Char(character) if character.is_ascii_uppercase() => {
                let mut to_hash = self.clone();
                to_hash.bare_key = BareKey::Char(character.to_ascii_lowercase());
                to_hash.key_modifiers.insert(KeyModifier::Shift);
                to_hash.bare_key.hash(state);
                to_hash.key_modifiers.hash(state);
            },
            _ => {
                self.bare_key.hash(state);
                self.key_modifiers.hash(state);
            },
        }
    }
}

impl fmt::Display for KeyWithModifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.key_modifiers.is_empty() {
            write!(f, "{}", self.bare_key)
        } else {
            write!(
                f,
                "{} {}",
                self.key_modifiers
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                self.bare_key
            )
        }
    }
}

#[cfg(not(target_family = "wasm"))]
impl Into<Modifiers> for &KeyModifier {
    fn into(self) -> Modifiers {
        match self {
            KeyModifier::Shift => Modifiers::SHIFT,
            KeyModifier::Alt => Modifiers::ALT,
            KeyModifier::Ctrl => Modifiers::CTRL,
            KeyModifier::Super => Modifiers::SUPER,
        }
    }
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

impl fmt::Display for BareKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BareKey::PageDown => write!(f, "PgDn"),
            BareKey::PageUp => write!(f, "PgUp"),
            BareKey::Left => write!(f, "←"),
            BareKey::Down => write!(f, "↓"),
            BareKey::Up => write!(f, "↑"),
            BareKey::Right => write!(f, "→"),
            BareKey::Home => write!(f, "HOME"),
            BareKey::End => write!(f, "END"),
            BareKey::Backspace => write!(f, "BACKSPACE"),
            BareKey::Delete => write!(f, "DEL"),
            BareKey::Insert => write!(f, "INS"),
            BareKey::F(index) => write!(f, "F{}", index),
            BareKey::Char(' ') => write!(f, "SPACE"),
            BareKey::Char(character) => write!(f, "{}", character),
            BareKey::Tab => write!(f, "TAB"),
            BareKey::Esc => write!(f, "ESC"),
            BareKey::Enter => write!(f, "ENTER"),
            BareKey::CapsLock => write!(f, "CAPSlOCK"),
            BareKey::ScrollLock => write!(f, "SCROLLlOCK"),
            BareKey::NumLock => write!(f, "NUMLOCK"),
            BareKey::PrintScreen => write!(f, "PRINTSCREEN"),
            BareKey::Pause => write!(f, "PAUSE"),
            BareKey::Menu => write!(f, "MENU"),
        }
    }
}

impl FromStr for BareKey {
    type Err = Box<dyn std::error::Error>;
    fn from_str(key_str: &str) -> Result<Self, Self::Err> {
        match key_str.to_ascii_lowercase().as_str() {
            "pagedown" => Ok(BareKey::PageDown),
            "pageup" => Ok(BareKey::PageUp),
            "left" => Ok(BareKey::Left),
            "down" => Ok(BareKey::Down),
            "up" => Ok(BareKey::Up),
            "right" => Ok(BareKey::Right),
            "home" => Ok(BareKey::Home),
            "end" => Ok(BareKey::End),
            "backspace" => Ok(BareKey::Backspace),
            "delete" => Ok(BareKey::Delete),
            "insert" => Ok(BareKey::Insert),
            "f1" => Ok(BareKey::F(1)),
            "f2" => Ok(BareKey::F(2)),
            "f3" => Ok(BareKey::F(3)),
            "f4" => Ok(BareKey::F(4)),
            "f5" => Ok(BareKey::F(5)),
            "f6" => Ok(BareKey::F(6)),
            "f7" => Ok(BareKey::F(7)),
            "f8" => Ok(BareKey::F(8)),
            "f9" => Ok(BareKey::F(9)),
            "f10" => Ok(BareKey::F(10)),
            "f11" => Ok(BareKey::F(11)),
            "f12" => Ok(BareKey::F(12)),
            "tab" => Ok(BareKey::Tab),
            "esc" => Ok(BareKey::Esc),
            "enter" => Ok(BareKey::Enter),
            "capslock" => Ok(BareKey::CapsLock),
            "scrolllock" => Ok(BareKey::ScrollLock),
            "numlock" => Ok(BareKey::NumLock),
            "printscreen" => Ok(BareKey::PrintScreen),
            "pause" => Ok(BareKey::Pause),
            "menu" => Ok(BareKey::Menu),
            "space" => Ok(BareKey::Char(' ')),
            _ => {
                if key_str.chars().count() == 1 {
                    if let Some(character) = key_str.chars().next() {
                        return Ok(BareKey::Char(character));
                    }
                }
                Err("unsupported key".into())
            },
        }
    }
}

#[derive(
    Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, ToString,
)]
pub enum KeyModifier {
    Ctrl,
    Alt,
    Shift,
    Super,
}

impl FromStr for KeyModifier {
    type Err = Box<dyn std::error::Error>;
    fn from_str(key_str: &str) -> Result<Self, Self::Err> {
        match key_str.to_ascii_lowercase().as_str() {
            "shift" => Ok(KeyModifier::Shift),
            "alt" => Ok(KeyModifier::Alt),
            "ctrl" => Ok(KeyModifier::Ctrl),
            "super" => Ok(KeyModifier::Super),
            _ => Err("unsupported modifier".into()),
        }
    }
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
            Ok("57399") => Some(BareKey::Char('0')),
            Ok("57400") => Some(BareKey::Char('1')),
            Ok("57401") => Some(BareKey::Char('2')),
            Ok("57402") => Some(BareKey::Char('3')),
            Ok("57403") => Some(BareKey::Char('4')),
            Ok("57404") => Some(BareKey::Char('5')),
            Ok("57405") => Some(BareKey::Char('6')),
            Ok("57406") => Some(BareKey::Char('7')),
            Ok("57407") => Some(BareKey::Char('8')),
            Ok("57408") => Some(BareKey::Char('9')),
            Ok("57409") => Some(BareKey::Char('.')),
            Ok("57410") => Some(BareKey::Char('/')),
            Ok("57411") => Some(BareKey::Char('*')),
            Ok("57412") => Some(BareKey::Char('-')),
            Ok("57413") => Some(BareKey::Char('+')),
            Ok("57414") => Some(BareKey::Enter),
            Ok("57415") => Some(BareKey::Char('=')),
            Ok("57417") => Some(BareKey::Left),
            Ok("57418") => Some(BareKey::Right),
            Ok("57419") => Some(BareKey::Up),
            Ok("57420") => Some(BareKey::Down),
            Ok("57421") => Some(BareKey::PageUp),
            Ok("57422") => Some(BareKey::PageDown),
            Ok("57423") => Some(BareKey::Home),
            Ok("57424") => Some(BareKey::End),
            Ok("57425") => Some(BareKey::Insert),
            Ok("57426") => Some(BareKey::Delete),
            Ok(num) => u8::from_str_radix(num, 10)
                .ok()
                .map(|n| BareKey::Char((n as char).to_ascii_lowercase())),
            _ => None,
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
            _ => None,
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
            _ => None,
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
        // we don't actually use the below, left here for completeness in case we want to add them
        // later
        const HYPER = 0b0001_0000;
        const META = 0b0010_0000;
        const CAPS_LOCK = 0b0100_0000;
        const NUM_LOCK = 0b1000_0000;
    }
}

impl KeyModifier {
    pub fn from_bytes(bytes: &[u8]) -> BTreeSet<KeyModifier> {
        let modifier_flags = str::from_utf8(bytes)
            .ok() // convert to string: (eg. "16")
            .and_then(|s| u8::from_str_radix(&s, 10).ok()) // convert to u8: (eg. 16)
            .map(|s| s.saturating_sub(1)) // subtract 1: (eg. 15)
            .and_then(|b| ModifierFlags::from_bits(b)); // bitflags: (0b0000_1111: Shift, Alt, Control, Super)
        let mut key_modifiers = BTreeSet::new();
        if let Some(modifier_flags) = modifier_flags {
            for name in modifier_flags.iter() {
                match name {
                    ModifierFlags::SHIFT => key_modifiers.insert(KeyModifier::Shift),
                    ModifierFlags::ALT => key_modifiers.insert(KeyModifier::Alt),
                    ModifierFlags::CONTROL => key_modifiers.insert(KeyModifier::Ctrl),
                    ModifierFlags::SUPER => key_modifiers.insert(KeyModifier::Super),
                    _ => false,
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
            key_modifiers: BTreeSet::new(),
        }
    }
    pub fn new_with_modifiers(bare_key: BareKey, key_modifiers: BTreeSet<KeyModifier>) -> Self {
        KeyWithModifier {
            bare_key,
            key_modifiers,
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
                    key_modifiers,
                })
            },
            _ => None,
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
                    key_modifiers,
                })
            },
            _ => None,
        }
    }
    pub fn from_bytes_with_no_ending_byte(
        number_bytes: &[u8],
        modifier_bytes: &[u8],
    ) -> Option<Self> {
        // CSI 1; modifiers [ABCDEFHPQS]
        let bare_key = BareKey::from_bytes_with_no_ending_byte(number_bytes);
        match bare_key {
            Some(bare_key) => {
                let key_modifiers = KeyModifier::from_bytes(modifier_bytes);
                Some(KeyWithModifier {
                    bare_key,
                    key_modifiers,
                })
            },
            _ => None,
        }
    }
    pub fn strip_common_modifiers(&self, common_modifiers: &Vec<KeyModifier>) -> Self {
        let common_modifiers: BTreeSet<&KeyModifier> = common_modifiers.into_iter().collect();
        KeyWithModifier {
            bare_key: self.bare_key.clone(),
            key_modifiers: self
                .key_modifiers
                .iter()
                .filter(|m| !common_modifiers.contains(m))
                .cloned()
                .collect(),
        }
    }
    pub fn is_key_without_modifier(&self, key: BareKey) -> bool {
        self.bare_key == key && self.key_modifiers.is_empty()
    }
    pub fn is_key_with_ctrl_modifier(&self, key: BareKey) -> bool {
        self.bare_key == key && self.key_modifiers.contains(&KeyModifier::Ctrl)
    }
    pub fn is_key_with_alt_modifier(&self, key: BareKey) -> bool {
        self.bare_key == key && self.key_modifiers.contains(&KeyModifier::Alt)
    }
    pub fn is_key_with_shift_modifier(&self, key: BareKey) -> bool {
        self.bare_key == key && self.key_modifiers.contains(&KeyModifier::Shift)
    }
    pub fn is_key_with_super_modifier(&self, key: BareKey) -> bool {
        self.bare_key == key && self.key_modifiers.contains(&KeyModifier::Super)
    }
    pub fn is_cancel_key(&self) -> bool {
        // self.bare_key == BareKey::Esc || self.is_key_with_ctrl_modifier(BareKey::Char('c'))
        self.bare_key == BareKey::Esc
    }
    #[cfg(not(target_family = "wasm"))]
    pub fn to_termwiz_modifiers(&self) -> Modifiers {
        let mut modifiers = Modifiers::empty();
        for modifier in &self.key_modifiers {
            modifiers.set(modifier.into(), true);
        }
        modifiers
    }
    #[cfg(not(target_family = "wasm"))]
    pub fn to_termwiz_keycode(&self) -> KeyCode {
        match self.bare_key {
            BareKey::PageDown => KeyCode::PageDown,
            BareKey::PageUp => KeyCode::PageUp,
            BareKey::Left => KeyCode::LeftArrow,
            BareKey::Down => KeyCode::DownArrow,
            BareKey::Up => KeyCode::UpArrow,
            BareKey::Right => KeyCode::RightArrow,
            BareKey::Home => KeyCode::Home,
            BareKey::End => KeyCode::End,
            BareKey::Backspace => KeyCode::Backspace,
            BareKey::Delete => KeyCode::Delete,
            BareKey::Insert => KeyCode::Insert,
            BareKey::F(index) => KeyCode::Function(index),
            BareKey::Char(character) => KeyCode::Char(character),
            BareKey::Tab => KeyCode::Tab,
            BareKey::Esc => KeyCode::Escape,
            BareKey::Enter => KeyCode::Enter,
            BareKey::CapsLock => KeyCode::CapsLock,
            BareKey::ScrollLock => KeyCode::ScrollLock,
            BareKey::NumLock => KeyCode::NumLock,
            BareKey::PrintScreen => KeyCode::PrintScreen,
            BareKey::Pause => KeyCode::Pause,
            BareKey::Menu => KeyCode::Menu,
        }
    }
    #[cfg(not(target_family = "wasm"))]
    pub fn serialize_non_kitty(&self) -> Option<String> {
        let modifiers = self.to_termwiz_modifiers();
        let key_code_encode_modes = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Xterm,
            // all these flags are false because they have been dealt with before this
            // serialization
            application_cursor_keys: false,
            newline_mode: false,
            modify_other_keys: None,
        };
        self.to_termwiz_keycode()
            .encode(modifiers, key_code_encode_modes, true)
            .ok()
    }
    #[cfg(not(target_family = "wasm"))]
    pub fn serialize_kitty(&self) -> Option<String> {
        let modifiers = self.to_termwiz_modifiers();
        let key_code_encode_modes = KeyCodeEncodeModes {
            encoding: KeyboardEncoding::Kitty(KittyKeyboardFlags::DISAMBIGUATE_ESCAPE_CODES),
            // all these flags are false because they have been dealt with before this
            // serialization
            application_cursor_keys: false,
            newline_mode: false,
            modify_other_keys: None,
        };
        self.to_termwiz_keycode()
            .encode(modifiers, key_code_encode_modes, true)
            .ok()
    }
    pub fn has_no_modifiers(&self) -> bool {
        self.key_modifiers.is_empty()
    }
    pub fn has_modifiers(&self, modifiers: &[KeyModifier]) -> bool {
        for modifier in modifiers {
            if !self.key_modifiers.contains(modifier) {
                return false;
            }
        }
        true
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
    Hover(isize, usize),      // line and column
}

impl Mouse {
    pub fn position(&self) -> Option<(usize, usize)> {
        // (line, column)
        match self {
            Mouse::LeftClick(line, column) => Some((*line as usize, *column as usize)),
            Mouse::RightClick(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Hold(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Release(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Hover(line, column) => Some((*line as usize, *column as usize)),
            _ => None,
        }
    }
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
    Key(KeyWithModifier),
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
    CommandPaneOpened(u32, Context), // u32 - terminal_pane_id
    CommandPaneExited(u32, Option<i32>, Context), // u32 - terminal_pane_id, Option<i32> -
    // exit_code
    PaneClosed(PaneId),
    EditPaneOpened(u32, Context),              // u32 - terminal_pane_id
    EditPaneExited(u32, Option<i32>, Context), // u32 - terminal_pane_id, Option<i32> - exit code
    CommandPaneReRun(u32, Context),            // u32 - terminal_pane_id, Option<i32> -
    FailedToWriteConfigToDisk(Option<String>), // String -> the file path we failed to write
    ListClients(Vec<ClientInfo>),
    HostFolderChanged(PathBuf),               // PathBuf -> new host folder
    FailedToChangeHostFolder(Option<String>), // String -> the error we got when changing
    PastedText(String),
    ConfigWasWrittenToDisk,
    WebServerStatus(WebServerStatus),
    FailedToStartWebServer(String),
    BeforeClose,
    InterceptedKeyPress(KeyWithModifier),
}

#[derive(Debug, Clone, PartialEq, Eq, EnumDiscriminants, ToString, Serialize, Deserialize)]
pub enum WebServerStatus {
    Online(String), // String -> base url
    Offline,
    DifferentVersion(String), // version
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
    Reconfigure,
    FullHdAccess,
    StartWebServer,
    InterceptInput,
}

impl PermissionType {
    pub fn display_name(&self) -> String {
        match self {
            PermissionType::ReadApplicationState => {
                "Access Zellij state (Panes, Tabs and UI)".to_owned()
            },
            PermissionType::ChangeApplicationState => {
                "Change Zellij state (Panes, Tabs and UI) and run commands".to_owned()
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
            PermissionType::Reconfigure => "Change Zellij runtime configuration".to_owned(),
            PermissionType::FullHdAccess => "Full access to the hard-drive".to_owned(),
            PermissionType::StartWebServer => {
                "Start a local web server to serve Zellij sessions".to_owned()
            },
            PermissionType::InterceptInput => "Intercept Input (keyboard & mouse)".to_owned(),
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

// these are used for the web client
impl PaletteColor {
    pub fn as_rgb_str(&self) -> String {
        let (r, g, b) = match *self {
            Self::Rgb((r, g, b)) => (r, g, b),
            Self::EightBit(c) => eightbit_to_rgb(c),
        };
        format!("rgb({}, {}, {})", r, g, b)
    }
    pub fn from_rgb_str(rgb_str: &str) -> Self {
        let trimmed = rgb_str.trim();

        if !trimmed.starts_with("rgb(") || !trimmed.ends_with(')') {
            return Self::default();
        }

        let inner = trimmed
            .strip_prefix("rgb(")
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or("");

        let parts: Vec<&str> = inner.split(',').collect();

        if parts.len() != 3 {
            return Self::default();
        }

        let mut rgb_values = [0u8; 3];
        for (i, part) in parts.iter().enumerate() {
            if let Some(rgb_val) = rgb_values.get_mut(i) {
                if let Ok(parsed) = part.trim().parse::<u8>() {
                    *rgb_val = parsed;
                } else {
                    return Self::default();
                }
            }
        }

        Self::Rgb((rgb_values[0], rgb_values[1], rgb_values[2]))
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
    pub colors: Styling,
    pub rounded_corners: bool,
    pub hide_session_name: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Coloration {
    NoStyling,
    Styled(StyleDeclaration),
}

impl Coloration {
    pub fn with_fallback(&self, fallback: StyleDeclaration) -> StyleDeclaration {
        match &self {
            Coloration::NoStyling => fallback,
            Coloration::Styled(style) => *style,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct Styling {
    pub text_unselected: StyleDeclaration,
    pub text_selected: StyleDeclaration,
    pub ribbon_unselected: StyleDeclaration,
    pub ribbon_selected: StyleDeclaration,
    pub table_title: StyleDeclaration,
    pub table_cell_unselected: StyleDeclaration,
    pub table_cell_selected: StyleDeclaration,
    pub list_unselected: StyleDeclaration,
    pub list_selected: StyleDeclaration,
    pub frame_unselected: Option<StyleDeclaration>,
    pub frame_selected: StyleDeclaration,
    pub frame_highlight: StyleDeclaration,
    pub exit_code_success: StyleDeclaration,
    pub exit_code_error: StyleDeclaration,
    pub multiplayer_user_colors: MultiplayerColors,
}

#[derive(Debug, Copy, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct StyleDeclaration {
    pub base: PaletteColor,
    pub background: PaletteColor,
    pub emphasis_0: PaletteColor,
    pub emphasis_1: PaletteColor,
    pub emphasis_2: PaletteColor,
    pub emphasis_3: PaletteColor,
}

#[derive(Debug, Copy, Default, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct MultiplayerColors {
    pub player_1: PaletteColor,
    pub player_2: PaletteColor,
    pub player_3: PaletteColor,
    pub player_4: PaletteColor,
    pub player_5: PaletteColor,
    pub player_6: PaletteColor,
    pub player_7: PaletteColor,
    pub player_8: PaletteColor,
    pub player_9: PaletteColor,
    pub player_10: PaletteColor,
}

pub const DEFAULT_STYLES: Styling = Styling {
    text_unselected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BRIGHT_GRAY),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    text_selected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BRIGHT_GRAY),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    ribbon_unselected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BLACK),
        emphasis_0: PaletteColor::EightBit(default_colors::RED),
        emphasis_1: PaletteColor::EightBit(default_colors::WHITE),
        emphasis_2: PaletteColor::EightBit(default_colors::BLUE),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    ribbon_selected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BLACK),
        emphasis_0: PaletteColor::EightBit(default_colors::RED),
        emphasis_1: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_2: PaletteColor::EightBit(default_colors::MAGENTA),
        emphasis_3: PaletteColor::EightBit(default_colors::BLUE),
        background: PaletteColor::EightBit(default_colors::GREEN),
    },
    exit_code_success: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_0: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_1: PaletteColor::EightBit(default_colors::BLACK),
        emphasis_2: PaletteColor::EightBit(default_colors::MAGENTA),
        emphasis_3: PaletteColor::EightBit(default_colors::BLUE),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    exit_code_error: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::RED),
        emphasis_0: PaletteColor::EightBit(default_colors::YELLOW),
        emphasis_1: PaletteColor::EightBit(default_colors::GOLD),
        emphasis_2: PaletteColor::EightBit(default_colors::SILVER),
        emphasis_3: PaletteColor::EightBit(default_colors::PURPLE),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    frame_unselected: None,
    frame_selected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::MAGENTA),
        emphasis_3: PaletteColor::EightBit(default_colors::BROWN),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    frame_highlight: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_0: PaletteColor::EightBit(default_colors::MAGENTA),
        emphasis_1: PaletteColor::EightBit(default_colors::PURPLE),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::GREEN),
        background: PaletteColor::EightBit(default_colors::GREEN),
    },
    table_title: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    table_cell_unselected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BRIGHT_GRAY),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    table_cell_selected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::RED),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    list_unselected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::BRIGHT_GRAY),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    list_selected: StyleDeclaration {
        base: PaletteColor::EightBit(default_colors::GREEN),
        emphasis_0: PaletteColor::EightBit(default_colors::ORANGE),
        emphasis_1: PaletteColor::EightBit(default_colors::CYAN),
        emphasis_2: PaletteColor::EightBit(default_colors::RED),
        emphasis_3: PaletteColor::EightBit(default_colors::MAGENTA),
        background: PaletteColor::EightBit(default_colors::GRAY),
    },
    multiplayer_user_colors: MultiplayerColors {
        player_1: PaletteColor::EightBit(default_colors::MAGENTA),
        player_2: PaletteColor::EightBit(default_colors::BLUE),
        player_3: PaletteColor::EightBit(default_colors::PURPLE),
        player_4: PaletteColor::EightBit(default_colors::YELLOW),
        player_5: PaletteColor::EightBit(default_colors::CYAN),
        player_6: PaletteColor::EightBit(default_colors::GOLD),
        player_7: PaletteColor::EightBit(default_colors::RED),
        player_8: PaletteColor::EightBit(default_colors::SILVER),
        player_9: PaletteColor::EightBit(default_colors::PINK),
        player_10: PaletteColor::EightBit(default_colors::BROWN),
    },
};

impl Default for Styling {
    fn default() -> Self {
        DEFAULT_STYLES
    }
}

impl From<Styling> for Palette {
    fn from(styling: Styling) -> Self {
        Palette {
            theme_hue: ThemeHue::Dark,
            source: PaletteSource::Default,
            fg: styling.ribbon_unselected.background,
            bg: styling.text_unselected.background,
            red: styling.exit_code_error.base,
            green: styling.text_unselected.emphasis_2,
            yellow: styling.exit_code_error.emphasis_0,
            blue: styling.ribbon_unselected.emphasis_2,
            magenta: styling.text_unselected.emphasis_3,
            orange: styling.text_unselected.emphasis_0,
            cyan: styling.text_unselected.emphasis_1,
            black: styling.ribbon_unselected.base,
            white: styling.ribbon_unselected.emphasis_1,
            gray: styling.list_unselected.background,
            purple: styling.multiplayer_user_colors.player_3,
            gold: styling.multiplayer_user_colors.player_6,
            silver: styling.multiplayer_user_colors.player_8,
            pink: styling.multiplayer_user_colors.player_9,
            brown: styling.multiplayer_user_colors.player_10,
        }
    }
}

impl From<Palette> for Styling {
    fn from(palette: Palette) -> Self {
        let (fg, bg) = match palette.theme_hue {
            ThemeHue::Light => (palette.black, palette.white),
            ThemeHue::Dark => (palette.white, palette.black),
        };
        Styling {
            text_unselected: StyleDeclaration {
                base: fg,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: bg,
            },
            text_selected: StyleDeclaration {
                base: fg,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.bg,
            },
            ribbon_unselected: StyleDeclaration {
                base: palette.black,
                emphasis_0: palette.red,
                emphasis_1: palette.white,
                emphasis_2: palette.blue,
                emphasis_3: palette.magenta,
                background: palette.fg,
            },
            ribbon_selected: StyleDeclaration {
                base: palette.black,
                emphasis_0: palette.red,
                emphasis_1: palette.orange,
                emphasis_2: palette.magenta,
                emphasis_3: palette.blue,
                background: palette.green,
            },
            exit_code_success: StyleDeclaration {
                base: palette.green,
                emphasis_0: palette.cyan,
                emphasis_1: palette.black,
                emphasis_2: palette.magenta,
                emphasis_3: palette.blue,
                background: Default::default(),
            },
            exit_code_error: StyleDeclaration {
                base: palette.red,
                emphasis_0: palette.yellow,
                emphasis_1: palette.gold,
                emphasis_2: palette.silver,
                emphasis_3: palette.purple,
                background: Default::default(),
            },
            frame_unselected: None,
            frame_selected: StyleDeclaration {
                base: palette.green,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.magenta,
                emphasis_3: palette.brown,
                background: Default::default(),
            },
            frame_highlight: StyleDeclaration {
                base: palette.orange,
                emphasis_0: palette.magenta,
                emphasis_1: palette.purple,
                emphasis_2: palette.orange,
                emphasis_3: palette.orange,
                background: Default::default(),
            },
            table_title: StyleDeclaration {
                base: palette.green,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.gray,
            },
            table_cell_unselected: StyleDeclaration {
                base: fg,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.black,
            },
            table_cell_selected: StyleDeclaration {
                base: fg,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.bg,
            },
            list_unselected: StyleDeclaration {
                base: palette.white,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.black,
            },
            list_selected: StyleDeclaration {
                base: palette.white,
                emphasis_0: palette.orange,
                emphasis_1: palette.cyan,
                emphasis_2: palette.green,
                emphasis_3: palette.magenta,
                background: palette.bg,
            },
            multiplayer_user_colors: MultiplayerColors {
                player_1: palette.magenta,
                player_2: palette.blue,
                player_3: palette.purple,
                player_4: palette.yellow,
                player_5: palette.cyan,
                player_6: palette.gold,
                player_7: palette.red,
                player_8: palette.silver,
                player_9: palette.pink,
                player_10: palette.brown,
            },
        }
    }
}

// FIXME: Poor devs hashtable since HashTable can't derive `Default`...
pub type KeybindsVec = Vec<(InputMode, Vec<(KeyWithModifier, Vec<Action>)>)>;

/// Provides information helpful in rendering the Zellij controls for UI bars
#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModeInfo {
    pub mode: InputMode,
    pub base_mode: Option<InputMode>,
    pub keybinds: KeybindsVec,
    pub style: Style,
    pub capabilities: PluginCapabilities,
    pub session_name: Option<String>,
    pub editor: Option<PathBuf>,
    pub shell: Option<PathBuf>,
    pub web_clients_allowed: Option<bool>,
    pub web_sharing: Option<WebSharing>,
    pub currently_marking_pane_group: Option<bool>,
    pub is_web_client: Option<bool>,
    // note: these are only the configured ip/port that will be bound if and when the server is up
    pub web_server_ip: Option<IpAddr>,
    pub web_server_port: Option<u16>,
    pub web_server_capability: Option<bool>,
}

impl ModeInfo {
    pub fn get_mode_keybinds(&self) -> Vec<(KeyWithModifier, Vec<Action>)> {
        self.get_keybinds_for_mode(self.mode)
    }

    pub fn get_keybinds_for_mode(&self, mode: InputMode) -> Vec<(KeyWithModifier, Vec<Action>)> {
        for (vec_mode, map) in &self.keybinds {
            if mode == *vec_mode {
                return map.to_vec();
            }
        }
        vec![]
    }
    pub fn update_keybinds(&mut self, keybinds: Keybinds) {
        self.keybinds = keybinds.to_keybinds_vec();
    }
    pub fn update_default_mode(&mut self, new_default_mode: InputMode) {
        self.base_mode = Some(new_default_mode);
    }
    pub fn update_theme(&mut self, theme: Styling) {
        self.style.colors = theme.into();
    }
    pub fn update_rounded_corners(&mut self, rounded_corners: bool) {
        self.style.rounded_corners = rounded_corners;
    }
    pub fn update_arrow_fonts(&mut self, should_support_arrow_fonts: bool) {
        // it is honestly quite baffling to me how "arrow_fonts: false" can mean "I support arrow
        // fonts", but since this is a public API... ¯\_(ツ)_/¯
        self.capabilities.arrow_fonts = !should_support_arrow_fonts;
    }
    pub fn update_hide_session_name(&mut self, hide_session_name: bool) {
        self.style.hide_session_name = hide_session_name;
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
    pub plugins: BTreeMap<u32, PluginInfo>,
    pub web_clients_allowed: bool,
    pub web_client_count: usize,
    pub tab_history: BTreeMap<ClientId, Vec<usize>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PluginInfo {
    pub location: String,
    pub configuration: BTreeMap<String, String>,
}

impl From<RunPlugin> for PluginInfo {
    fn from(run_plugin: RunPlugin) -> Self {
        PluginInfo {
            location: run_plugin.location.display(),
            configuration: run_plugin.configuration.inner().clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum LayoutInfo {
    BuiltIn(String),
    File(String),
    Url(String),
    Stringified(String),
}

impl LayoutInfo {
    pub fn name(&self) -> &str {
        match self {
            LayoutInfo::BuiltIn(name) => &name,
            LayoutInfo::File(name) => &name,
            LayoutInfo::Url(url) => &url,
            LayoutInfo::Stringified(layout) => &layout,
        }
    }
    pub fn is_builtin(&self) -> bool {
        match self {
            LayoutInfo::BuiltIn(_name) => true,
            LayoutInfo::File(_name) => false,
            LayoutInfo::Url(_url) => false,
            LayoutInfo::Stringified(_stringified) => false,
        }
    }
    pub fn from_config(
        layout_dir: &Option<PathBuf>,
        default_layout: &Option<PathBuf>,
    ) -> Option<Self> {
        match default_layout {
            Some(default_layout) => {
                if default_layout.extension().is_some() || default_layout.components().count() > 1 {
                    let Some(layout_dir) = layout_dir
                        .as_ref()
                        .map(|l| l.clone())
                        .or_else(default_layout_dir)
                    else {
                        return None;
                    };
                    Some(LayoutInfo::File(
                        layout_dir.join(default_layout).display().to_string(),
                    ))
                } else if default_layout.starts_with("http://")
                    || default_layout.starts_with("https://")
                {
                    Some(LayoutInfo::Url(default_layout.display().to_string()))
                } else {
                    Some(LayoutInfo::BuiltIn(default_layout.display().to_string()))
                }
            },
            None => None,
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
    pub fn populate_plugin_list(&mut self, plugins: BTreeMap<u32, RunPlugin>) {
        // u32 - plugin_id
        let mut plugin_list = BTreeMap::new();
        for (plugin_id, run_plugin) in plugins {
            plugin_list.insert(plugin_id, run_plugin.into());
        }
        self.plugins = plugin_list;
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
    /// Row count in the viewport (including all non-ui panes, eg. will excluse the status bar)
    pub viewport_rows: usize,
    /// Column count in the viewport (including all non-ui panes, eg. will excluse the status bar)
    pub viewport_columns: usize,
    /// Row count in the display area (including all panes, will typically be larger than the
    /// viewport)
    pub display_area_rows: usize,
    /// Column count in the display area (including all panes, will typically be larger than the
    /// viewport)
    pub display_area_columns: usize,
    /// The number of selectable (eg. not the UI bars) tiled panes currently in this tab
    pub selectable_tiled_panes_count: usize,
    /// The number of selectable (eg. not the UI bars) floating panes currently in this tab
    pub selectable_floating_panes_count: usize,
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
    /// Grouped panes (usually through an explicit user action) that are staged for a bulk action
    /// the index is kept track of in order to preserve the pane group order
    pub index_in_pane_group: BTreeMap<ClientId, usize>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ClientInfo {
    pub client_id: ClientId,
    pub pane_id: PaneId,
    pub running_command: String,
    pub is_current_client: bool,
}

impl ClientInfo {
    pub fn new(
        client_id: ClientId,
        pane_id: PaneId,
        running_command: String,
        is_current_client: bool,
    ) -> Self {
        ClientInfo {
            client_id,
            pane_id,
            running_command,
            is_current_client,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PluginIds {
    pub plugin_id: u32,
    pub zellij_pid: u32,
    pub initial_cwd: PathBuf,
    pub client_id: ClientId,
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
    pub floating_pane_coordinates: Option<FloatingPaneCoordinates>,
}

#[derive(Debug, Default, Clone)]
pub struct NewPluginArgs {
    pub should_float: Option<bool>,
    pub pane_id_to_replace: Option<PaneId>,
    pub pane_title: Option<String>,
    pub cwd: Option<PathBuf>,
    pub skip_cache: bool,
    pub should_focus: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PaneId {
    Terminal(u32),
    Plugin(u32),
}

impl FromStr for PaneId {
    type Err = Box<dyn std::error::Error>;
    fn from_str(stringified_pane_id: &str) -> Result<Self, Self::Err> {
        if let Some(terminal_stringified_pane_id) = stringified_pane_id.strip_prefix("terminal_") {
            u32::from_str_radix(terminal_stringified_pane_id, 10)
                .map(|id| PaneId::Terminal(id))
                .map_err(|e| e.into())
        } else if let Some(plugin_pane_id) = stringified_pane_id.strip_prefix("plugin_") {
            u32::from_str_radix(plugin_pane_id, 10)
                .map(|id| PaneId::Plugin(id))
                .map_err(|e| e.into())
        } else {
            u32::from_str_radix(&stringified_pane_id, 10)
                .map(|id| PaneId::Terminal(id))
                .map_err(|e| e.into())
        }
    }
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
    pub fn with_floating_pane_coordinates(
        mut self,
        floating_pane_coordinates: FloatingPaneCoordinates,
    ) -> Self {
        self.floating_pane_coordinates = Some(floating_pane_coordinates);
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
    pub fn new_plugin_instance_should_be_focused(mut self) -> Self {
        let new_plugin_args = self.new_plugin_args.get_or_insert_with(Default::default);
        new_plugin_args.should_focus = Some(true);
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
    pub pinned: Option<bool>,
}

impl FloatingPaneCoordinates {
    pub fn new(
        x: Option<String>,
        y: Option<String>,
        width: Option<String>,
        height: Option<String>,
        pinned: Option<bool>,
    ) -> Option<Self> {
        let x = x.and_then(|x| SplitSize::from_str(&x).ok());
        let y = y.and_then(|y| SplitSize::from_str(&y).ok());
        let width = width.and_then(|width| SplitSize::from_str(&width).ok());
        let height = height.and_then(|height| SplitSize::from_str(&height).ok());
        if x.is_none() && y.is_none() && width.is_none() && height.is_none() && pinned.is_none() {
            None
        } else {
            Some(FloatingPaneCoordinates {
                x,
                y,
                width,
                height,
                pinned,
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

impl From<PaneGeom> for FloatingPaneCoordinates {
    fn from(pane_geom: PaneGeom) -> Self {
        FloatingPaneCoordinates {
            x: Some(SplitSize::Fixed(pane_geom.x)),
            y: Some(SplitSize::Fixed(pane_geom.y)),
            width: Some(SplitSize::Fixed(pane_geom.cols.as_usize())),
            height: Some(SplitSize::Fixed(pane_geom.rows.as_usize())),
            pinned: Some(pane_geom.is_pinned),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OriginatingPlugin {
    pub plugin_id: u32,
    pub client_id: ClientId,
    pub context: Context,
}

impl OriginatingPlugin {
    pub fn new(plugin_id: u32, client_id: ClientId, context: Context) -> Self {
        OriginatingPlugin {
            plugin_id,
            client_id,
            context,
        }
    }
}

#[derive(ArgEnum, Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSharing {
    #[serde(alias = "on")]
    On,
    #[serde(alias = "off")]
    Off,
    #[serde(alias = "disabled")]
    Disabled,
}

impl Default for WebSharing {
    fn default() -> Self {
        Self::Off
    }
}

impl WebSharing {
    pub fn is_on(&self) -> bool {
        match self {
            WebSharing::On => true,
            _ => false,
        }
    }
    pub fn web_clients_allowed(&self) -> bool {
        match self {
            WebSharing::On => true,
            _ => false,
        }
    }
    pub fn sharing_is_disabled(&self) -> bool {
        match self {
            WebSharing::Disabled => true,
            _ => false,
        }
    }
    pub fn set_sharing(&mut self) -> bool {
        // returns true if successfully set sharing
        match self {
            WebSharing::On => true,
            WebSharing::Off => {
                *self = WebSharing::On;
                true
            },
            WebSharing::Disabled => false,
        }
    }
    pub fn set_not_sharing(&mut self) -> bool {
        // returns true if successfully set not sharing
        match self {
            WebSharing::On => {
                *self = WebSharing::Off;
                true
            },
            WebSharing::Off => true,
            WebSharing::Disabled => false,
        }
    }
}

impl FromStr for WebSharing {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "On" | "on" => Ok(Self::On),
            "Off" | "off" => Ok(Self::Off),
            "Disabled" | "disabled" => Ok(Self::Disabled),
            _ => Err(format!("No such option: {}", s)),
        }
    }
}

type Context = BTreeMap<String, String>;

#[derive(Debug, Clone, EnumDiscriminants, ToString)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(CommandType))]
pub enum PluginCommand {
    Subscribe(HashSet<EventType>),
    Unsubscribe(HashSet<EventType>),
    SetSelectable(bool),
    GetPluginIds,
    GetZellijVersion,
    OpenFile(FileToOpen, Context),
    OpenFileFloating(FileToOpen, Option<FloatingPaneCoordinates>, Context),
    OpenTerminal(FileToOpen), // only used for the path as cwd
    OpenTerminalFloating(FileToOpen, Option<FloatingPaneCoordinates>), // only used for the path as cwd
    OpenCommandPane(CommandToRun, Context),
    OpenCommandPaneFloating(CommandToRun, Option<FloatingPaneCoordinates>, Context),
    SwitchTabTo(u32), // tab index
    SetTimeout(f64),  // seconds
    ExecCmd(Vec<String>),
    PostMessageTo(PluginMessage),
    PostMessageToPlugin(PluginMessage),
    HideSelf,
    ShowSelf(bool), // bool - should float if hidden
    SwitchToMode(InputMode),
    NewTabsWithLayout(String), // raw kdl layout
    NewTab {
        name: Option<String>,
        cwd: Option<String>,
    },
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
    OpenFileInPlace(FileToOpen, Context),
    OpenCommandPaneInPlace(CommandToRun, Context),
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
    Reconfigure(String, bool), // String -> stringified configuration, bool -> save configuration
    // file to disk
    HidePaneWithId(PaneId),
    ShowPaneWithId(PaneId, bool), // bool -> should_float_if_hidden
    OpenCommandPaneBackground(CommandToRun, Context),
    RerunCommandPane(u32), // u32  - terminal pane id
    ResizePaneIdWithDirection(ResizeStrategy, PaneId),
    EditScrollbackForPaneWithId(PaneId),
    WriteToPaneId(Vec<u8>, PaneId),
    WriteCharsToPaneId(String, PaneId),
    MovePaneWithPaneId(PaneId),
    MovePaneWithPaneIdInDirection(PaneId, Direction),
    ClearScreenForPaneId(PaneId),
    ScrollUpInPaneId(PaneId),
    ScrollDownInPaneId(PaneId),
    ScrollToTopInPaneId(PaneId),
    ScrollToBottomInPaneId(PaneId),
    PageScrollUpInPaneId(PaneId),
    PageScrollDownInPaneId(PaneId),
    TogglePaneIdFullscreen(PaneId),
    TogglePaneEmbedOrEjectForPaneId(PaneId),
    CloseTabWithIndex(usize), // usize - tab_index
    BreakPanesToNewTab(Vec<PaneId>, Option<String>, bool), // bool -
    // should_change_focus_to_new_tab,
    // Option<String> - optional name for
    // the new tab
    BreakPanesToTabWithIndex(Vec<PaneId>, usize, bool), // usize - tab_index, bool -
    // should_change_focus_to_new_tab
    ReloadPlugin(u32), // u32 - plugin pane id
    LoadNewPlugin {
        url: String,
        config: BTreeMap<String, String>,
        load_in_background: bool,
        skip_plugin_cache: bool,
    },
    RebindKeys {
        keys_to_rebind: Vec<(InputMode, KeyWithModifier, Vec<Action>)>,
        keys_to_unbind: Vec<(InputMode, KeyWithModifier)>,
        write_config_to_disk: bool,
    },
    ListClients,
    ChangeHostFolder(PathBuf),
    SetFloatingPanePinned(PaneId, bool), // bool -> should be pinned
    StackPanes(Vec<PaneId>),
    ChangeFloatingPanesCoordinates(Vec<(PaneId, FloatingPaneCoordinates)>),
    OpenCommandPaneNearPlugin(CommandToRun, Context),
    OpenTerminalNearPlugin(FileToOpen),
    OpenTerminalFloatingNearPlugin(FileToOpen, Option<FloatingPaneCoordinates>),
    OpenTerminalInPlaceOfPlugin(FileToOpen, bool), // bool -> close_plugin_after_replace
    OpenCommandPaneFloatingNearPlugin(CommandToRun, Option<FloatingPaneCoordinates>, Context),
    OpenCommandPaneInPlaceOfPlugin(CommandToRun, bool, Context), // bool ->
    // close_plugin_after_replace
    OpenFileNearPlugin(FileToOpen, Context),
    OpenFileFloatingNearPlugin(FileToOpen, Option<FloatingPaneCoordinates>, Context),
    StartWebServer,
    StopWebServer,
    ShareCurrentSession,
    StopSharingCurrentSession,
    OpenFileInPlaceOfPlugin(FileToOpen, bool, Context), // bool -> close_plugin_after_replace
    GroupAndUngroupPanes(Vec<PaneId>, Vec<PaneId>, bool), // panes to group, panes to ungroup,
    // bool -> for all clients
    HighlightAndUnhighlightPanes(Vec<PaneId>, Vec<PaneId>), // panes to highlight, panes to
    // unhighlight
    CloseMultiplePanes(Vec<PaneId>),
    FloatMultiplePanes(Vec<PaneId>),
    EmbedMultiplePanes(Vec<PaneId>),
    QueryWebServerStatus,
    SetSelfMouseSelectionSupport(bool),
    GenerateWebLoginToken(Option<String>), // String -> optional token label
    RevokeWebLoginToken(String),           // String -> token id (provided name or generated id)
    ListWebLoginTokens,
    RevokeAllWebLoginTokens,
    RenameWebLoginToken(String, String), // (original_name, new_name)
    InterceptKeyPresses,
    ClearKeyPressesIntercepts,
    ReplacePaneWithExistingPane(PaneId, PaneId), // (pane id to replace, pane id of existing)
}
