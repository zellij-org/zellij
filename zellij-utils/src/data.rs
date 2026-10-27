use crate::home::default_layout_dir;
use crate::input::actions::{Action, RunCommandAction};
use crate::input::config::{ConversionError, KdlError};
use crate::input::keybinds::Keybinds;
use crate::input::layout::{
    Layout, PercentOrFixed, Run, RunPlugin, RunPluginLocation, RunPluginOrAlias,
};
pub use crate::input::options::PaneFrameStyle;
use crate::pane_size::PaneGeom;
use crate::position::Position;
use crate::shared::{
    colors as ansi_colors, default_theme_colors as default_colors, eightbit_to_rgb,
};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs::Metadata;
use std::hash::{Hash, Hasher};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::str::{self, FromStr};
use std::time::Duration;
use strum_macros::{Display, EnumDiscriminants, EnumIter, EnumString};
use unicode_width::UnicodeWidthChar;

#[cfg(not(target_family = "wasm"))]
use crate::vendored::termwiz::input::{KeyCode, KeyCodeEncodeModes, KeyboardEncoding, Modifiers};

pub type ClientId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnblockCondition {
    /// Unblock only when exit status is 0 (success)
    OnExitSuccess,
    /// Unblock only when exit status is non-zero (failure)
    OnExitFailure,
    /// Unblock on any exit (success or failure)
    OnAnyExit,
}

impl UnblockCondition {
    /// Check if the condition is met for the given exit status
    pub fn is_met(&self, exit_status: i32) -> bool {
        match self {
            UnblockCondition::OnExitSuccess => exit_status == 0,
            UnblockCondition::OnExitFailure => exit_status != 0,
            UnblockCondition::OnAnyExit => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommandOrPlugin {
    Command(RunCommandAction),
    Plugin(RunPluginOrAlias),
    File(FileToOpen), // open file in configured editor
}

impl CommandOrPlugin {
    pub fn new_command(command: Vec<String>) -> Self {
        CommandOrPlugin::Command(RunCommandAction::new(command))
    }
}

pub fn client_slot_to_colors(
    display_slot: usize,
    colors: MultiplayerColors,
) -> Option<(PaletteColor, PaletteColor)> {
    // (primary color, secondary color)
    let black = PaletteColor::EightBit(ansi_colors::BLACK);
    match display_slot {
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

pub fn client_id_to_colors(
    client_id: ClientId,
    colors: MultiplayerColors,
) -> Option<(PaletteColor, PaletteColor)> {
    client_slot_to_colors(client_id as usize, colors)
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
            "del" => Ok(BareKey::Delete),
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
    Eq, Clone, Copy, Debug, PartialEq, Hash, Deserialize, Serialize, PartialOrd, Ord, Display,
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
            Ok(num) => u32::from_str_radix(num, 10)
                .ok()
                .and_then(char::from_u32)
                .map(BareKey::Char),
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
        let shift = self.key_modifiers.contains(&KeyModifier::Shift);
        let alt = self.key_modifiers.contains(&KeyModifier::Alt);
        let ctrl = self.key_modifiers.contains(&KeyModifier::Ctrl);
        let sup = self.key_modifiers.contains(&KeyModifier::Super);
        let modifier_value = |shift: bool| -> u8 {
            1 + shift as u8 + 2 * alt as u8 + 4 * ctrl as u8 + 8 * sup as u8
        };
        let mods = modifier_value(shift);
        let csi_u = |code: u32, mods: u8| {
            if mods > 1 {
                format!("\u{1b}[{};{}u", code, mods)
            } else {
                format!("\u{1b}[{}u", code)
            }
        };
        let csi_letter = |letter: char| {
            if mods > 1 {
                format!("\u{1b}[1;{}{}", mods, letter)
            } else {
                format!("\u{1b}[{}", letter)
            }
        };
        let ss3_or_csi_letter = |letter: char| {
            if mods > 1 {
                format!("\u{1b}[1;{}{}", mods, letter)
            } else {
                format!("\u{1b}O{}", letter)
            }
        };
        let csi_tilde = |number: u8| {
            if mods > 1 {
                format!("\u{1b}[{};{}~", number, mods)
            } else {
                format!("\u{1b}[{}~", number)
            }
        };
        let plain_or_csi_u = |plain: &str, code: u32| {
            if mods > 1 {
                csi_u(code, mods)
            } else {
                plain.to_owned()
            }
        };
        let encoded = match self.bare_key {
            BareKey::Char(c) if !(alt || ctrl || sup) => {
                if shift {
                    c.to_uppercase().collect()
                } else {
                    c.to_string()
                }
            },
            BareKey::Char(c) => {
                let mut lowercase = c.to_lowercase();
                match (lowercase.next(), lowercase.next()) {
                    (Some(lower), None) if lower != c => csi_u(lower as u32, modifier_value(true)),
                    _ => csi_u(c as u32, mods),
                }
            },
            BareKey::Enter => plain_or_csi_u("\r", 13),
            BareKey::Tab => plain_or_csi_u("\t", 9),
            BareKey::Backspace => plain_or_csi_u("\u{7f}", 127),
            BareKey::Esc => csi_u(27, mods),
            BareKey::Up => csi_letter('A'),
            BareKey::Down => csi_letter('B'),
            BareKey::Right => csi_letter('C'),
            BareKey::Left => csi_letter('D'),
            BareKey::Home => csi_letter('H'),
            BareKey::End => csi_letter('F'),
            BareKey::F(1) => ss3_or_csi_letter('P'),
            BareKey::F(2) => ss3_or_csi_letter('Q'),
            BareKey::F(3) => csi_tilde(13),
            BareKey::F(4) => ss3_or_csi_letter('S'),
            BareKey::F(5) => csi_tilde(15),
            BareKey::F(6) => csi_tilde(17),
            BareKey::F(7) => csi_tilde(18),
            BareKey::F(8) => csi_tilde(19),
            BareKey::F(9) => csi_tilde(20),
            BareKey::F(10) => csi_tilde(21),
            BareKey::F(11) => csi_tilde(23),
            BareKey::F(12) => csi_tilde(24),
            BareKey::F(n @ 13..=35) => csi_u(57376 + (n as u32 - 13), mods),
            BareKey::F(_) => return None,
            BareKey::Insert => csi_tilde(2),
            BareKey::Delete => csi_tilde(3),
            BareKey::PageUp => csi_tilde(5),
            BareKey::PageDown => csi_tilde(6),
            BareKey::PrintScreen => csi_u(57361, mods),
            BareKey::Pause => csi_u(57362, mods),
            BareKey::Menu => csi_u(57363, mods),
            BareKey::CapsLock | BareKey::ScrollLock | BareKey::NumLock => return None,
        };
        Some(encoded)
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
    pub fn has_only_modifiers(&self, modifiers: &[KeyModifier]) -> bool {
        for modifier in modifiers {
            if !self.key_modifiers.contains(modifier) {
                return false;
            }
        }
        if self.key_modifiers.len() != modifiers.len() {
            return false;
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

impl Default for Direction {
    fn default() -> Self {
        Direction::Left
    }
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

impl Default for Resize {
    fn default() -> Self {
        Resize::Increase
    }
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
    ScrollUp(usize),   // number of lines
    ScrollDown(usize), // number of lines
    ScrollLeft(usize),
    ScrollRight(usize),
    LeftClick(isize, usize),  // line and column
    RightClick(isize, usize), // line and column
    Hold(isize, usize),       // line and column
    Release(isize, usize),    // line and column
    Hover(isize, usize),      // line and column
    DoubleClick(isize, usize),
}

impl Mouse {
    pub fn position(&self) -> Option<(usize, usize)> {
        // (line, column)
        match self {
            Mouse::LeftClick(line, column) => Some((*line as usize, *column as usize)),
            Mouse::DoubleClick(line, column) => Some((*line as usize, *column as usize)),
            Mouse::RightClick(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Hold(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Release(line, column) => Some((*line as usize, *column as usize)),
            Mouse::Hover(line, column) => Some((*line as usize, *column as usize)),
            _ => None,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, EnumIter,
)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    ScrollUp,
    ScrollDown,
    ScrollLeft,
    ScrollRight,
}

impl MouseButton {
    pub fn is_wheel(&self) -> bool {
        matches!(
            self,
            MouseButton::ScrollUp
                | MouseButton::ScrollDown
                | MouseButton::ScrollLeft
                | MouseButton::ScrollRight
        )
    }
    pub fn name(&self) -> &'static str {
        match self {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
            MouseButton::ScrollUp => "ScrollUp",
            MouseButton::ScrollDown => "ScrollDown",
            MouseButton::ScrollLeft => "ScrollLeft",
            MouseButton::ScrollRight => "ScrollRight",
        }
    }
    pub fn from_name(name: &str) -> Option<MouseButton> {
        use strum::IntoEnumIterator;
        MouseButton::iter().find(|button| button.name().eq_ignore_ascii_case(name))
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Serialize,
    Deserialize,
    EnumIter,
)]
pub enum MouseTarget {
    #[default]
    Any,
    Content,
    Frame,
}

impl MouseTarget {
    pub fn name(&self) -> &'static str {
        match self {
            MouseTarget::Any => "any",
            MouseTarget::Content => "content",
            MouseTarget::Frame => "frame",
        }
    }
    pub fn from_name(name: &str) -> Option<MouseTarget> {
        use strum::IntoEnumIterator;
        MouseTarget::iter().find(|target| target.name().eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MouseTrigger {
    pub button: MouseButton,
    pub modifiers: BTreeSet<KeyModifier>,
    pub click_count: u8,
    pub target: MouseTarget,
}

impl MouseTrigger {
    pub fn new(button: MouseButton) -> Self {
        MouseTrigger {
            button,
            modifiers: BTreeSet::new(),
            click_count: 1,
            target: MouseTarget::Any,
        }
    }
    pub fn with_modifiers(mut self, modifiers: BTreeSet<KeyModifier>) -> Self {
        self.modifiers = modifiers;
        self
    }
    pub fn with_click_count(mut self, click_count: u8) -> Self {
        self.click_count = click_count;
        self
    }
    pub fn with_target(mut self, target: MouseTarget) -> Self {
        self.target = target;
        self
    }
    pub fn parse(text: &str, target: MouseTarget) -> Result<MouseTrigger, String> {
        let mut parts: Vec<&str> = text.split_ascii_whitespace().collect();
        let button_text = parts
            .pop()
            .ok_or_else(|| "A mouse trigger needs a button".to_owned())?;
        let button = MouseButton::from_name(button_text).ok_or_else(|| {
            format!(
                "Unknown mouse button '{}'; use Left, Right, Middle, ScrollUp, ScrollDown, ScrollLeft or ScrollRight",
                button_text
            )
        })?;
        let mut modifiers = BTreeSet::new();
        let mut click_count = 1;
        for part in parts {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" => {
                    modifiers.insert(KeyModifier::Ctrl);
                },
                "alt" => {
                    modifiers.insert(KeyModifier::Alt);
                },
                "shift" => {
                    modifiers.insert(KeyModifier::Shift);
                },
                "super" => {
                    return Err(
                        "Super cannot be used with the mouse, because terminals do not report it for mouse events"
                            .to_owned(),
                    )
                },
                "double" => click_count = 2,
                "triple" => click_count = 3,
                _ => {
                    return Err(format!(
                        "Unknown word '{}' in the mouse trigger '{}'",
                        part, text
                    ))
                },
            }
        }
        if click_count > 1 && button.is_wheel() {
            return Err(format!(
                "Double and Triple can only be used with Left, Right and Middle, not {}",
                button.name()
            ));
        }
        Ok(MouseTrigger {
            button,
            modifiers,
            click_count,
            target,
        })
    }
    pub fn to_kdl(&self) -> String {
        let mut parts: Vec<String> = self.modifiers.iter().map(|m| m.to_string()).collect();
        match self.click_count {
            2 => parts.push("Double".to_owned()),
            3 => parts.push("Triple".to_owned()),
            _ => {},
        }
        parts.push(self.button.name().to_owned());
        parts.join(" ")
    }
    pub fn same_trigger_ignoring_target(&self, other: &MouseTrigger) -> bool {
        self.button == other.button
            && self.modifiers == other.modifiers
            && self.click_count == other.click_count
    }
}

impl fmt::Display for MouseTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.target {
            MouseTarget::Any => write!(f, "{}", self.to_kdl()),
            target => write!(f, "{} · {}", self.to_kdl(), target.name()),
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

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledText {
    pub text: String,
    pub indices: Vec<Vec<usize>>,
}

impl StyledText {
    pub fn plain(text: impl Into<String>) -> Self {
        StyledText {
            text: text.into(),
            indices: vec![],
        }
    }
    pub fn is_plain(&self) -> bool {
        self.indices.iter().all(|level| level.is_empty())
    }
}

impl From<&str> for StyledText {
    fn from(text: &str) -> Self {
        StyledText::plain(text)
    }
}

impl From<String> for StyledText {
    fn from(text: String) -> Self {
        StyledText::plain(text)
    }
}

/// These events can be subscribed to with subscribe method exported by `zellij-tile`.
/// Once subscribed to, they will trigger the `update` method of the `ZellijPlugin` trait.
#[derive(Debug, Clone, PartialEq, EnumDiscriminants, Display, Serialize, Deserialize)]
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
    MouseWithModifiers(Mouse, BTreeSet<KeyModifier>),
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
    /// An action was performed by the user (requires InterceptInput permission)
    UserAction(Action, ClientId, Option<u32>, Option<ClientId>), // Action, client_id, terminal_id, cli_client_id
    PaneRenderReport(HashMap<PaneId, PaneContents>),
    ActionComplete(Action, Option<PaneId>, BTreeMap<String, String>), // Action, pane_id, context
    CwdChanged(PaneId, PathBuf, Vec<ClientId>), // pane_id, cwd, focused_client_ids
    CommandChanged(PaneId, Vec<String>, bool, Vec<ClientId>), // pane_id, command, is_foreground, focused_client_ids
    AvailableLayoutInfo(Vec<LayoutInfo>, Vec<LayoutWithError>),
    PluginConfigurationChanged(BTreeMap<String, String>),
    HighlightClicked {
        pane_id: PaneId,
        pattern: String,
        matched_string: String,
        context: BTreeMap<String, String>,
    },
    /// Initial keybindings sent once on plugin load and on reconfiguration.
    /// Plugins that subscribe to this event signal they cache keybindings
    /// and can handle lightweight ModeUpdate events without keybindings.
    InitialKeybinds(KeybindsVec),
    /// The host terminal indicated its color palette theme mode (CSI 2031 / DSR 997).
    HostTerminalThemeChanged(HostTerminalThemeMode),
    SoftKeyboardVisibilityChanged(bool),
    HintText(BTreeMap<usize, StyledText>),
    ActivePaneScroll(Option<(usize, usize)>),
    NestedSessionModeUpdate {
        pane_id: PaneId,
        session_path: Vec<String>,
        mode: InputMode,
        base_mode: Option<InputMode>,
        keybinds_generation: u64,
    },
    NestedSessionEnded {
        pane_id: PaneId,
        reason: NestedSessionEndReason,
    },
    ContextMenu(ContextMenuContext, Vec<ContextMenuEntry>),
    ConfigChangesDropped(Vec<SettingKey>),
    AvailableKeybindPresets(Vec<KeybindPresetInfo>, Vec<KeybindPresetWithError>),
    ConfigFileChangedSinceRead,
    PromptResult(u64, crate::prompt::PromptResult),
    SessionSuggestions(SessionSuggestions),
    SessionCountsUpdate(SessionCounts),
    FolderSessionsUpdate(FolderSessions),
    TerminalCommandSubmitted,
    SessionPreview(SessionPreview),
    SavedSessionPreview(SavedSessionPreview),
    PaneAttentionChanged(PaneId, crate::attention::PaneAttention),
    PromptShown(PaneId),
    CommandStarted(PaneId, Option<crate::attention::CommandText>),
    CommandFinished(PaneId, crate::attention::CommandOutcome),
    GlobalAttentionUpdate(crate::attention::GlobalAttention),
    SessionHistoryUpdate(crate::session_history::SessionHistoryInfo),
    DesktopIntegration(String),
    LiveViewStateChanged {
        target: Option<PaneAddress>,
        state: MirrorState,
        input_captured: bool,
        input_released_by: Option<LiveViewRelease>,
        resize_allowed: bool,
        view_only: bool,
        pinned: bool,
        source_size: Option<crate::pane_size::Size>,
        release_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FolderRelation {
    Here,
    Subfolder,
    #[default]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionSuggestionColumn {
    pub fact: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionSuggestion {
    pub name: String,
    pub is_running: bool,
    pub last_used_secs_ago: u64,
    pub created_secs_ago: u64,
    pub folder: Option<String>,
    pub folder_relation: FolderRelation,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub facts: BTreeMap<String, String>,
    pub tier: Option<u32>,
    pub matching_columns: Vec<String>,
    pub script_label: Option<String>,
    pub tabs: Vec<SessionPreviewTab>,
    pub connected_clients: usize,
}

impl SessionSuggestion {
    pub fn column_matches(&self, fact: &str) -> bool {
        self.matching_columns.iter().any(|c| c == fact)
    }
    pub fn fact_value(&self, fact: &str) -> Option<String> {
        match fact {
            "directory" => self.folder.clone(),
            "git_repo" => self.repo.clone(),
            "git_branch" => self.branch.clone(),
            other => self.facts.get(other).cloned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionSuggestions {
    pub folder: Option<String>,
    pub branch: Option<String>,
    pub columns: Vec<SessionSuggestionColumn>,
    pub suggestions: Vec<SessionSuggestion>,
    pub command_submitted: bool,
    pub from_script: bool,
    #[serde(default)]
    pub desktop_app_running: bool,
    #[serde(default)]
    pub desktop_window_sessions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionCounts {
    pub running: u32,
    pub resumable_matching: u32,
}

impl SessionCounts {
    pub fn total(&self) -> u32 {
        self.running + self.resumable_matching
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FolderSessions {
    pub folder: String,
    pub running: Vec<String>,
    pub resumable: Vec<String>,
    pub full_match_running: Vec<String>,
}

impl FolderSessions {
    pub fn total(&self) -> usize {
        self.running.len() + self.resumable.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionPreviewPane {
    pub id: u32,
    pub is_plugin: bool,
    pub title: String,
    pub focused: bool,
    pub contents: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionPreviewTab {
    pub name: String,
    pub active: bool,
    pub panes: Vec<SessionPreviewPane>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SessionPreview {
    pub session_name: String,
    pub tab_index: Option<usize>,
    pub active_tab_index: Option<usize>,
    pub tab_names: Vec<String>,
    pub contents: String,
    pub pane_count: usize,
    pub connected_clients: usize,
    pub error: Option<String>,
    #[serde(default)]
    pub tabs: Vec<SessionPreviewTab>,
    #[serde(default)]
    pub pane_id: Option<(u32, bool)>,
    #[serde(default)]
    pub anchor_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PreviewAnchor {
    Bottom,
    LastCommandOutput,
    NewOutputStart,
}

impl PreviewAnchor {
    pub fn as_str(&self) -> &'static str {
        match self {
            PreviewAnchor::Bottom => "bottom",
            PreviewAnchor::LastCommandOutput => "last_command_output",
            PreviewAnchor::NewOutputStart => "new_output_start",
        }
    }
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "bottom" => Some(PreviewAnchor::Bottom),
            "last_command_output" => Some(PreviewAnchor::LastCommandOutput),
            "new_output_start" => Some(PreviewAnchor::NewOutputStart),
            _ => None,
        }
    }
    pub fn for_status(status: crate::attention::AttentionStatus) -> Self {
        use crate::attention::AttentionStatus;
        match status {
            AttentionStatus::Failed | AttentionStatus::Done => PreviewAnchor::LastCommandOutput,
            AttentionStatus::NewOutput => PreviewAnchor::NewOutputStart,
            _ => PreviewAnchor::Bottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SavedPanePreview {
    pub title: Option<String>,
    pub command: Option<String>,
    pub cwd: Option<String>,
    pub contents: Option<String>,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SavedTabPreview {
    pub name: String,
    pub focused: bool,
    pub panes: Vec<SavedPanePreview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SavedSessionPreview {
    pub session_name: String,
    pub folder: Option<String>,
    pub tabs: Vec<SavedTabPreview>,
    pub error: Option<String>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextMenuKind {
    Pane,
    PaneFrame,
    Tab,
    Bar,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMenuContext {
    pub kind: ContextMenuKind,
    pub pane_id: Option<PaneId>,
    pub pane_is_floating: bool,
    pub tab_index: Option<usize>,
    pub tab_id: Option<usize>,
    pub tab_count: usize,
    pub line: usize,
    pub column: usize,
    pub client_id: ClientId,
    #[serde(default)]
    pub paste_shortcut: Option<String>,
}

impl ContextMenuContext {
    pub fn target_pane_id(&self) -> Option<PaneId> {
        match self.kind {
            ContextMenuKind::Pane | ContextMenuKind::PaneFrame | ContextMenuKind::Bar => {
                self.pane_id
            },
            _ => None,
        }
    }
    pub fn target_tab_id(&self) -> Option<u64> {
        self.tab_id.map(|id| id as u64)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickedPaneAction {
    CloseFocus,
    ToggleFocusFullscreen,
    ToggleEmbedOrFloating,
    TogglePinned,
    ToggleInGroup,
    StartRename,
    ToggleMute,
    ToggleIgnoreOutput,
    ToggleWatch,
    HideBar,
    PasteFromClipboard,
}

impl ClickedPaneAction {
    pub const ALL: [ClickedPaneAction; 11] = [
        ClickedPaneAction::CloseFocus,
        ClickedPaneAction::ToggleFocusFullscreen,
        ClickedPaneAction::ToggleEmbedOrFloating,
        ClickedPaneAction::TogglePinned,
        ClickedPaneAction::ToggleInGroup,
        ClickedPaneAction::StartRename,
        ClickedPaneAction::ToggleMute,
        ClickedPaneAction::ToggleIgnoreOutput,
        ClickedPaneAction::ToggleWatch,
        ClickedPaneAction::HideBar,
        ClickedPaneAction::PasteFromClipboard,
    ];
    pub fn kdl_name(&self) -> &'static str {
        match self {
            ClickedPaneAction::CloseFocus => "CloseFocusByPaneId",
            ClickedPaneAction::ToggleFocusFullscreen => "ToggleFocusFullscreenByPaneId",
            ClickedPaneAction::ToggleEmbedOrFloating => "TogglePaneEmbedOrFloatingByPaneId",
            ClickedPaneAction::TogglePinned => "TogglePanePinnedByPaneId",
            ClickedPaneAction::ToggleInGroup => "TogglePaneInGroupByPaneId",
            ClickedPaneAction::StartRename => "StartRenamePaneByPaneId",
            ClickedPaneAction::ToggleMute => "MutePaneByPaneId",
            ClickedPaneAction::ToggleIgnoreOutput => "IgnorePaneOutputByPaneId",
            ClickedPaneAction::ToggleWatch => "WatchPaneByPaneId",
            ClickedPaneAction::HideBar => "HideBarByPaneId",
            ClickedPaneAction::PasteFromClipboard => "PasteFromClipboardByPaneId",
        }
    }
    pub fn from_kdl_name(name: &str) -> Option<Self> {
        ClickedPaneAction::ALL
            .into_iter()
            .find(|action| action.kdl_name() == name)
    }
    pub fn with_pane(&self, pane_id: PaneId) -> Action {
        match self {
            ClickedPaneAction::CloseFocus => Action::CloseFocusByPaneId { pane_id },
            ClickedPaneAction::ToggleFocusFullscreen => {
                Action::ToggleFocusFullscreenByPaneId { pane_id }
            },
            ClickedPaneAction::ToggleEmbedOrFloating => {
                Action::TogglePaneEmbedOrFloatingByPaneId { pane_id }
            },
            ClickedPaneAction::TogglePinned => Action::TogglePanePinnedByPaneId { pane_id },
            ClickedPaneAction::ToggleInGroup => Action::TogglePaneInGroupByPaneId { pane_id },
            ClickedPaneAction::StartRename => Action::StartRenamePaneByPaneId { pane_id },
            ClickedPaneAction::ToggleMute => Action::MutePaneByPaneId { pane_id },
            ClickedPaneAction::ToggleIgnoreOutput => Action::IgnorePaneOutputByPaneId { pane_id },
            ClickedPaneAction::ToggleWatch => Action::WatchPaneByPaneId { pane_id },
            ClickedPaneAction::HideBar => Action::HideBarByPaneId { pane_id },
            ClickedPaneAction::PasteFromClipboard => Action::PasteFromClipboardByPaneId { pane_id },
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClickedTabAction {
    Close,
    StartRename,
    Move(Direction),
    MoveToPosition(u64),
    Mute,
}

pub const TAB_POSITION_END: u64 = u64::MAX;

impl ClickedTabAction {
    pub fn kdl_name(&self) -> &'static str {
        match self {
            ClickedTabAction::Close => "CloseTabById",
            ClickedTabAction::StartRename => "StartRenameTabByTabId",
            ClickedTabAction::Move(_) => "MoveTabByTabId",
            ClickedTabAction::MoveToPosition(_) => "MoveTabToPosition",
            ClickedTabAction::Mute => "MuteTabByTabId",
        }
    }
    pub fn with_tab(&self, id: u64) -> Action {
        match self {
            ClickedTabAction::Close => Action::CloseTabById { id },
            ClickedTabAction::StartRename => Action::StartRenameTabByTabId { id },
            ClickedTabAction::Move(direction) => Action::MoveTabByTabId {
                id,
                direction: *direction,
            },
            ClickedTabAction::MoveToPosition(position) => Action::MoveTabToPosition {
                id,
                position: *position,
            },
            ClickedTabAction::Mute => Action::MuteTabByTabId { id },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ContextMenuAction {
    Action(Action),
    ClickedPane(ClickedPaneAction),
    ClickedTab(ClickedTabAction),
}

impl From<Action> for ContextMenuAction {
    fn from(action: Action) -> Self {
        ContextMenuAction::Action(action)
    }
}

impl ContextMenuAction {
    pub fn into_action(self, pane_id: Option<PaneId>, tab_id: Option<u64>) -> Option<Action> {
        match self {
            ContextMenuAction::Action(action) => Some(action),
            ContextMenuAction::ClickedPane(action) => pane_id.map(|id| action.with_pane(id)),
            ContextMenuAction::ClickedTab(action) => tab_id.map(|id| action.with_tab(id)),
        }
    }
    pub fn keybinding_equivalent(&self) -> Vec<Action> {
        match self {
            ContextMenuAction::Action(action) => action.keybinding_equivalent(),
            ContextMenuAction::ClickedPane(action) => action
                .with_pane(PaneId::Terminal(0))
                .keybinding_equivalent(),
            ContextMenuAction::ClickedTab(action) => action.with_tab(0).keybinding_equivalent(),
        }
    }
    pub fn as_action(&self) -> Option<&Action> {
        match self {
            ContextMenuAction::Action(action) => Some(action),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ContextMenuEntry {
    Item {
        label: String,
        actions: Vec<ContextMenuAction>,
        #[serde(default)]
        toggled_label: Option<String>,
    },
    Separator,
}

impl ContextMenuEntry {
    pub fn item(label: impl Into<String>, actions: Vec<ContextMenuAction>) -> Self {
        ContextMenuEntry::Item {
            label: label.into(),
            actions,
            toggled_label: None,
        }
    }
    pub fn label(&self) -> Option<&str> {
        match self {
            ContextMenuEntry::Item { label, .. } => Some(label.as_str()),
            ContextMenuEntry::Separator => None,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ContextMenuTarget {
    Tab(usize),
    Bar,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NestedSessionEndReason {
    Exited,
    Unresponsive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NestedSessionKeybinds {
    pub session_path: Vec<String>,
    pub mode: InputMode,
    pub base_mode: Option<InputMode>,
    pub keybinds: KeybindsVec,
    pub keybinds_generation: u64,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NestedSessionKeybindsError {
    NotANestedSession,
    NotSupported,
    GuestUnresponsive,
    GuestGone,
    TooLarge,
    Timeout,
}

impl fmt::Display for NestedSessionKeybindsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let description = match self {
            NestedSessionKeybindsError::NotANestedSession => "no nested session in this pane",
            NestedSessionKeybindsError::NotSupported => {
                "the nested session cannot report its keybindings"
            },
            NestedSessionKeybindsError::GuestUnresponsive => "the nested session is not responding",
            NestedSessionKeybindsError::GuestGone => "the nested session has ended",
            NestedSessionKeybindsError::TooLarge => {
                "the nested session's keybindings are too large to send"
            },
            NestedSessionKeybindsError::Timeout => "the nested session did not answer in time",
        };
        write!(f, "{}", description)
    }
}

pub type NestedSessionKeybindsResponse = Result<NestedSessionKeybinds, NestedSessionKeybindsError>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HostTerminalThemeMode {
    Dark,
    Light,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumDiscriminants, Display, Serialize, Deserialize)]
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
    Display,
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
    ReadPaneContents,
    RunActionsAsUser,
    WriteToClipboard,
    ReadSessionEnvironmentVariables,
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
            PermissionType::ReadPaneContents => {
                "Read pane contents (viewport and selection)".to_owned()
            },
            PermissionType::RunActionsAsUser => "Execute actions as the user".to_owned(),
            PermissionType::WriteToClipboard => "Write to clipboard".to_owned(),
            PermissionType::ReadSessionEnvironmentVariables => {
                "Read environment variables present upon session creation".to_owned()
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
    ValueEnum,
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
    #[serde(alias = "renamesession")]
    RenameSession,
}

impl Default for InputMode {
    fn default() -> InputMode {
        InputMode::Normal
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash, ValueEnum)]
pub enum ThemeHue {
    #[serde(alias = "light")]
    Light,
    #[serde(alias = "dark")]
    Dark,
}
impl Default for ThemeHue {
    fn default() -> ThemeHue {
        ThemeHue::Dark
    }
}

impl FromStr for ThemeHue {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "light" => Ok(ThemeHue::Light),
            "dark" => Ok(ThemeHue::Dark),
            e => Err(format!(
                "Unknown theme hue: '{}' (expected 'dark' or 'light')",
                e
            )),
        }
    }
}

impl fmt::Display for ThemeHue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeHue::Light => write!(f, "light"),
            ThemeHue::Dark => write!(f, "dark"),
        }
    }
}

impl From<ThemeHue> for HostTerminalThemeMode {
    fn from(hue: ThemeHue) -> Self {
        match hue {
            ThemeHue::Light => HostTerminalThemeMode::Light,
            ThemeHue::Dark => HostTerminalThemeMode::Dark,
        }
    }
}

impl From<HostTerminalThemeMode> for ThemeHue {
    fn from(mode: HostTerminalThemeMode) -> Self {
        match mode {
            HostTerminalThemeMode::Light => ThemeHue::Light,
            HostTerminalThemeMode::Dark => ThemeHue::Dark,
        }
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

/// Priority layer for plugin-supplied regex highlights.
/// Higher-priority layers take visual precedence over lower ones
/// when highlights overlap.  Built-in highlights (mouse selection,
/// search results) always take precedence over all plugin layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum HighlightLayer {
    Hint,           // lowest: pure pattern matching (paths, URLs, IPs)
    Tool,           // middle: backed by runtime domain knowledge (git, docker, k8s)
    ActionFeedback, // highest: result of an explicit user action (search, bookmarks)
}

impl Default for HighlightLayer {
    fn default() -> Self {
        HighlightLayer::Hint
    }
}

/// Style for a plugin-supplied regex highlight.
/// Theme-based variants reference `style.colors.text_unselected.*`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HighlightStyle {
    None,      // no color override — use with bold/italic/underline for style-only highlights
    Emphasis0, // fg = emphasis_0, no bg override
    Emphasis1, // fg = emphasis_1, no bg override
    Emphasis2, // fg = emphasis_2, no bg override
    Emphasis3, // fg = emphasis_3, no bg override
    BackgroundEmphasis0, // bg = emphasis_0, fg = background
    BackgroundEmphasis1, // bg = emphasis_1, fg = background
    BackgroundEmphasis2, // bg = emphasis_2, fg = background
    BackgroundEmphasis3, // bg = emphasis_3, fg = background
    CustomRgb {
        fg: Option<(u8, u8, u8)>,
        bg: Option<(u8, u8, u8)>,
    },
    CustomIndex {
        fg: Option<u8>,
        bg: Option<u8>,
    },
}

/// One pattern + style pair sent by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegexHighlight {
    pub pattern: String, // key for upsert; also the regex source
    pub style: HighlightStyle,
    pub layer: HighlightLayer,
    pub context: BTreeMap<String, String>, // arbitrary data echoed back verbatim on click
    pub on_hover: bool, // if true, only rendered when the cursor overlaps this match
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub tooltip_text: Option<String>, // shown at bottom of pane frame when hovering over match
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
            "renamesession" | "RenameSession" => Ok(InputMode::RenameSession),
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
pub enum LineStyle {
    #[default]
    Single,
    Double,
    Heavy,
    Dashed,
    HeavyDashed,
}

impl LineStyle {
    pub fn is_heavy(&self) -> bool {
        matches!(self, LineStyle::Heavy | LineStyle::HeavyDashed)
    }
    pub fn is_double(&self) -> bool {
        matches!(self, LineStyle::Double)
    }
    pub fn is_single(&self) -> bool {
        matches!(self, LineStyle::Single | LineStyle::Dashed)
    }
}

impl FromStr for LineStyle {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "single" | "light" | "normal" => Ok(LineStyle::Single),
            "double" => Ok(LineStyle::Double),
            "heavy" | "bold" | "thick" => Ok(LineStyle::Heavy),
            "dashed" => Ok(LineStyle::Dashed),
            "heavy_dashed" | "heavy-dashed" | "heavydashed" => Ok(LineStyle::HeavyDashed),
            _ => Err(format!(
                "Unknown line style: '{}', expected one of: single, double, heavy, dashed, heavy_dashed",
                s
            )),
        }
    }
}

impl fmt::Display for LineStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            LineStyle::Single => "single",
            LineStyle::Double => "double",
            LineStyle::Heavy => "heavy",
            LineStyle::Dashed => "dashed",
            LineStyle::HeavyDashed => "heavy_dashed",
        };
        write!(f, "{}", name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct BorderStyle {
    pub top: LineStyle,
    pub right: LineStyle,
    pub bottom: LineStyle,
    pub left: LineStyle,
    pub rounded_corners: bool,
}

impl BorderStyle {
    pub fn with_rounded_corners(rounded_corners: bool) -> Self {
        BorderStyle {
            rounded_corners,
            ..Default::default()
        }
    }
    pub fn uniform_style(&self) -> Option<LineStyle> {
        if self.top == self.right && self.right == self.bottom && self.bottom == self.left {
            Some(self.top)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct BorderStyleOverride {
    pub all: Option<LineStyle>,
    pub top: Option<LineStyle>,
    pub right: Option<LineStyle>,
    pub bottom: Option<LineStyle>,
    pub left: Option<LineStyle>,
    pub rounded_corners: Option<bool>,
}

impl BorderStyleOverride {
    pub fn is_empty(&self) -> bool {
        self.all.is_none()
            && self.top.is_none()
            && self.right.is_none()
            && self.bottom.is_none()
            && self.left.is_none()
            && self.rounded_corners.is_none()
    }
    pub fn none_if_empty(self) -> Option<Self> {
        if self.is_empty() {
            None
        } else {
            Some(self)
        }
    }
    pub fn from_cli_string(value: &str) -> Result<Self, String> {
        let mut border_style_override = BorderStyleOverride::default();
        for token in value.split(',') {
            let token = token.trim();
            if token.is_empty() {
                continue;
            }
            let (key, value) = match token.split_once(|c| c == ':' || c == '=') {
                Some((key, value)) => (key.trim().to_lowercase(), Some(value.trim())),
                None => (token.to_lowercase(), None),
            };
            match (key.as_str(), value) {
                ("rounded" | "rounded_corners" | "rounded-corners", value) => {
                    let rounded = match value {
                        None => true,
                        Some(value) => value.parse::<bool>().map_err(|_| {
                            format!("Expected true or false for rounded corners, got '{}'", value)
                        })?,
                    };
                    border_style_override.rounded_corners = Some(rounded);
                },
                ("all", Some(value)) => border_style_override.all = Some(LineStyle::from_str(value)?),
                ("top", Some(value)) => border_style_override.top = Some(LineStyle::from_str(value)?),
                ("right", Some(value)) => {
                    border_style_override.right = Some(LineStyle::from_str(value)?)
                },
                ("bottom", Some(value)) => {
                    border_style_override.bottom = Some(LineStyle::from_str(value)?)
                },
                ("left", Some(value)) => {
                    border_style_override.left = Some(LineStyle::from_str(value)?)
                },
                (_, Some(_)) => {
                    return Err(format!(
                        "Unknown border side: '{}', expected one of: all, top, right, bottom, left, rounded",
                        key
                    ))
                },
                (line_style, None) => border_style_override.all = Some(LineStyle::from_str(line_style)?),
            }
        }
        Ok(border_style_override)
    }
    pub fn from_optional_cli_string(value: Option<&str>) -> Result<Option<Self>, String> {
        match value {
            Some(value) => Self::from_cli_string(value).map(|b| b.none_if_empty()),
            None => Ok(None),
        }
    }
    pub fn from_strings(
        all: Option<String>,
        top: Option<String>,
        right: Option<String>,
        bottom: Option<String>,
        left: Option<String>,
        rounded_corners: Option<bool>,
    ) -> Result<Self, String> {
        let parse = |value: Option<String>| -> Result<Option<LineStyle>, String> {
            match value {
                Some(value) => LineStyle::from_str(&value).map(Some),
                None => Ok(None),
            }
        };
        Ok(BorderStyleOverride {
            all: parse(all)?,
            top: parse(top)?,
            right: parse(right)?,
            bottom: parse(bottom)?,
            left: parse(left)?,
            rounded_corners,
        })
    }
    pub fn apply_to(&self, base: BorderStyle) -> BorderStyle {
        let all = self.all;
        BorderStyle {
            top: self.top.or(all).unwrap_or(base.top),
            right: self.right.or(all).unwrap_or(base.right),
            bottom: self.bottom.or(all).unwrap_or(base.bottom),
            left: self.left.or(all).unwrap_or(base.left),
            rounded_corners: self.rounded_corners.unwrap_or(base.rounded_corners),
        }
    }
    pub fn merge(&self, other: &BorderStyleOverride) -> BorderStyleOverride {
        BorderStyleOverride {
            all: other.all.or(self.all),
            top: other.top.or(self.top),
            right: other.right.or(self.right),
            bottom: other.bottom.or(self.bottom),
            left: other.left.or(self.left),
            rounded_corners: other.rounded_corners.or(self.rounded_corners),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Style {
    pub colors: Styling,
    pub rounded_corners: bool,
    pub hide_session_name: bool,
    pub border_style: BorderStyle,
    pub floating_border_style: BorderStyle,
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
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    text_selected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::SURFACE),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    ribbon_unselected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::BACKGROUND),
        background: PaletteColor::Rgb(default_colors::RIBBON),
        emphasis_0: PaletteColor::Rgb(default_colors::WINE),
        emphasis_1: PaletteColor::Rgb(default_colors::RIBBON_HOVER),
        emphasis_2: PaletteColor::Rgb(default_colors::NAVY),
        emphasis_3: PaletteColor::Rgb(default_colors::BELL),
    },
    ribbon_selected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::BACKGROUND),
        background: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_0: PaletteColor::Rgb(default_colors::WINE),
        emphasis_1: PaletteColor::Rgb(default_colors::WINE),
        emphasis_2: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_3: PaletteColor::Rgb(default_colors::BELL),
    },
    exit_code_success: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::GREEN),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::TEAL),
        emphasis_1: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_2: PaletteColor::Rgb(default_colors::PINK),
        emphasis_3: PaletteColor::Rgb(default_colors::BLUE),
    },
    exit_code_error: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::ROSE),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::GOLD),
        emphasis_1: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_2: PaletteColor::Rgb(default_colors::MUTED),
        emphasis_3: PaletteColor::Rgb(default_colors::PURPLE),
    },
    frame_unselected: Some(StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::DIM),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::GOLD),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::PURPLE),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    }),
    frame_selected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::GREEN),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::GOLD),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::PINK),
        emphasis_3: PaletteColor::Rgb(default_colors::ORANGE),
    },
    frame_highlight: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::GOLD),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::TEAL),
        emphasis_1: PaletteColor::Rgb(default_colors::PURPLE),
        emphasis_2: PaletteColor::Rgb(default_colors::GOLD),
        emphasis_3: PaletteColor::Rgb(default_colors::GOLD),
    },
    table_title: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::GREEN),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    table_cell_unselected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    table_cell_selected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::SURFACE),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    list_unselected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::BACKGROUND),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    list_selected: StyleDeclaration {
        base: PaletteColor::Rgb(default_colors::FOREGROUND),
        background: PaletteColor::Rgb(default_colors::SURFACE),
        emphasis_0: PaletteColor::Rgb(default_colors::ORANGE),
        emphasis_1: PaletteColor::Rgb(default_colors::BLUE),
        emphasis_2: PaletteColor::Rgb(default_colors::GREEN),
        emphasis_3: PaletteColor::Rgb(default_colors::VIOLET),
    },
    multiplayer_user_colors: MultiplayerColors {
        player_1: PaletteColor::Rgb(default_colors::PINK),
        player_2: PaletteColor::Rgb(default_colors::BLUE),
        player_3: PaletteColor::Rgb(default_colors::PURPLE),
        player_4: PaletteColor::Rgb(default_colors::GOLD),
        player_5: PaletteColor::Rgb(default_colors::TEAL),
        player_6: PaletteColor::Rgb(default_colors::ORANGE),
        player_7: PaletteColor::Rgb(default_colors::ROSE),
        player_8: PaletteColor::Rgb(default_colors::MUTED),
        player_9: PaletteColor::Rgb(default_colors::GREEN),
        player_10: PaletteColor::Rgb(default_colors::SAND),
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
    pub pane_frame_style: Option<PaneFrameStyle>,
    pub session_dimmed: Option<bool>,
    pub session_ancestry: Vec<String>,
    pub host_fullscreen: Option<bool>,
    pub nested_ascend_keys: Vec<KeyWithModifier>,
    pub session_ascended: Option<bool>,
    pub nested_descend_keys: Vec<KeyWithModifier>,
    #[serde(default)]
    pub remote_client: Option<String>,
    #[serde(default)]
    pub hidden_bars: HiddenBars,
}

pub const ALL_BARS: &str = "all";
pub const TAB_BAR: &str = "tab-bar";
pub const STATUS_BAR: &str = "status-bar";
pub const COMPACT_BAR: &str = "compact-bar";

pub fn bar_role_label(bar_name: &str) -> Option<&'static str> {
    match bar_name {
        TAB_BAR => Some("tab bar"),
        STATUS_BAR => Some("status bar"),
        COMPACT_BAR => Some("compact bar"),
        _ => None,
    }
}

pub fn bar_shows_mode(bar_name: &str) -> bool {
    matches!(bar_name, STATUS_BAR | COMPACT_BAR)
}

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HiddenBars {
    #[default]
    None,
    All,
    Named(BTreeSet<String>),
}

impl HiddenBars {
    pub fn from_names<S: AsRef<str>>(names: &[S]) -> Self {
        if names.iter().any(|name| name.as_ref() == ALL_BARS) {
            return HiddenBars::All;
        }
        let names: BTreeSet<String> = names
            .iter()
            .map(|name| name.as_ref().trim().to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        if names.is_empty() {
            HiddenBars::None
        } else {
            HiddenBars::Named(names)
        }
    }
    pub fn all_unless_named<S: AsRef<str>>(names: &[S]) -> Self {
        if names.is_empty() {
            HiddenBars::All
        } else {
            HiddenBars::from_names(names)
        }
    }
    pub fn names(&self) -> Vec<String> {
        match self {
            HiddenBars::None => vec![],
            HiddenBars::All => vec![ALL_BARS.to_owned()],
            HiddenBars::Named(names) => names.iter().cloned().collect(),
        }
    }
    pub fn is_empty(&self) -> bool {
        matches!(self, HiddenBars::None)
    }
    pub fn hides_any<S: AsRef<str>>(&self, bar_names: &[S]) -> bool {
        match self {
            HiddenBars::None => false,
            HiddenBars::All => true,
            HiddenBars::Named(names) => bar_names
                .iter()
                .any(|bar_name| names.contains(bar_name.as_ref())),
        }
    }
    pub fn hides(&self, bar_name: &str) -> bool {
        self.hides_any(&[bar_name])
    }
    pub fn union(&self, other: &HiddenBars) -> HiddenBars {
        match (self, other) {
            (HiddenBars::All, _) | (_, HiddenBars::All) => HiddenBars::All,
            (HiddenBars::None, other) | (other, HiddenBars::None) => other.clone(),
            (HiddenBars::Named(a), HiddenBars::Named(b)) => {
                HiddenBars::Named(a.union(b).cloned().collect())
            },
        }
    }
}

impl FromStr for HiddenBars {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let names: Vec<&str> = s
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|name| !name.is_empty())
            .collect();
        if names.len() == 1 && names[0] == "none" {
            return Ok(HiddenBars::None);
        }
        Ok(HiddenBars::from_names(&names))
    }
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
    pub fn update_border_styles(
        &mut self,
        border_style: BorderStyle,
        floating_border_style: BorderStyle,
    ) {
        self.style.border_style = border_style;
        self.style.floating_border_style = floating_border_style;
    }
    pub fn update_arrow_fonts(&mut self, should_support_arrow_fonts: bool) {
        // it is honestly quite baffling to me how "arrow_fonts: false" can mean "I support arrow
        // fonts", but since this is a public API... ¯\_(ツ)_/¯
        self.capabilities.arrow_fonts = !should_support_arrow_fonts;
    }
    pub fn update_hide_session_name(&mut self, hide_session_name: bool) {
        self.style.hide_session_name = hide_session_name;
    }
    pub fn change_to_default_mode(&mut self) {
        if let Some(base_mode) = self.base_mode {
            self.mode = base_mode;
        }
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
    pub pane_history: BTreeMap<ClientId, Vec<PaneId>>,
    pub creation_time: Duration,
    #[serde(default)]
    pub attention_muted: bool,
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
    File(String, LayoutMetadata),
    Url(String),
    Stringified(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum KeybindPresetSource {
    #[default]
    BuiltIn,
    Folder,
    File,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeybindPresetInfo {
    pub name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub source: KeybindPresetSource,
    pub placeholders: Vec<String>,
    pub path: Option<String>,
    pub examples: Vec<(String, String)>,
}

impl KeybindPresetInfo {
    pub fn uses_placeholder(&self, placeholder: &str) -> bool {
        self.placeholders.iter().any(|p| p == placeholder)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeybindPresetWithError {
    pub name: String,
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct KeybindsSelectionSnapshot {
    pub preset: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub unlock: Option<String>,
    pub clears_defaults: bool,
    pub has_own_keybindings: bool,
    pub has_own_mousebindings: bool,
    pub active: KeybindPresetInfo,
    pub active_values: BTreeMap<String, String>,
    pub error: Option<String>,
    pub set_by_layout: bool,
    pub default_mode: Option<InputMode>,
    pub set_on_command_line: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LayoutWithError {
    pub layout_name: String,
    pub error: LayoutParsingError,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum LayoutParsingError {
    KdlError {
        kdl_error: KdlError,
        file_name: String,
        source_code: String,
    },
    SyntaxError,
}

impl AsRef<LayoutInfo> for LayoutInfo {
    fn as_ref(&self) -> &LayoutInfo {
        self
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LayoutMetadata {
    pub tabs: Vec<TabMetadata>,
    pub creation_time: String,
    pub update_time: String,
}

impl From<&PathBuf> for LayoutMetadata {
    fn from(path: &PathBuf) -> LayoutMetadata {
        match Layout::stringified_from_path(path) {
            Ok((path_str, stringified_layout, _swap_layouts)) => {
                match Layout::from_kdl(&stringified_layout, Some(path_str), None, None) {
                    Ok(layout) => {
                        let layout_tabs = layout.tabs();
                        let tabs = if layout_tabs.is_empty() {
                            let (tiled_pane_layout, floating_pane_layout) = layout.new_tab();
                            vec![TabMetadata::from(&(
                                None,
                                tiled_pane_layout,
                                floating_pane_layout,
                            ))]
                        } else {
                            layout
                                .tabs()
                                .into_iter()
                                .map(|tab| TabMetadata::from(&tab))
                                .collect()
                        };

                        // Get file metadata for creation and modification times as Unix epochs
                        let (creation_time, update_time) =
                            LayoutMetadata::creation_and_update_times(&path);

                        LayoutMetadata {
                            tabs,
                            creation_time,
                            update_time,
                        }
                    },
                    Err(e) => {
                        log::error!("Failed to parse layout: {}", e);
                        LayoutMetadata::default()
                    },
                }
            },
            Err(e) => {
                log::error!("Failed to read layout file: {}", e);
                LayoutMetadata::default()
            },
        }
    }
}

impl LayoutMetadata {
    fn creation_and_update_times(path: &PathBuf) -> (String, String) {
        // (creation_time, update_time) returns stringified unix epoch
        match std::fs::metadata(path) {
            Ok(metadata) => {
                let creation_time = metadata
                    .created()
                    .ok()
                    .and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs().to_string())
                    })
                    .unwrap_or_default();

                let update_time = metadata
                    .modified()
                    .ok()
                    .and_then(|t| {
                        t.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_secs().to_string())
                    })
                    .unwrap_or_default();

                (creation_time, update_time)
            },
            Err(_) => (String::new(), String::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TabMetadata {
    pub panes: Vec<PaneMetadata>,
    pub name: Option<String>,
}

impl
    From<&(
        Option<String>,
        crate::input::layout::TiledPaneLayout,
        Vec<crate::input::layout::FloatingPaneLayout>,
    )> for TabMetadata
{
    fn from(
        tab: &(
            Option<String>,
            crate::input::layout::TiledPaneLayout,
            Vec<crate::input::layout::FloatingPaneLayout>,
        ),
    ) -> Self {
        let (tab_name, tiled_pane_layout, floating_panes) = tab;

        // Collect panes from tiled layout (only leaf nodes are real panes)
        let mut panes = Vec::new();
        collect_leaf_panes(&tiled_pane_layout, &mut panes);

        // Collect panes from floating panes
        for floating_pane in floating_panes {
            panes.push(PaneMetadata::from(floating_pane));
        }

        TabMetadata {
            panes,
            name: tab_name.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaneMetadata {
    pub name: Option<String>,
    pub is_plugin: bool,
    pub is_builtin_plugin: bool,
}

impl From<&crate::input::layout::TiledPaneLayout> for PaneMetadata {
    fn from(pane: &crate::input::layout::TiledPaneLayout) -> Self {
        let mut is_plugin = false;
        let mut is_builtin_plugin = false;

        // Try to get the name from the pane's name field first
        let name = if let Some(ref name) = pane.name {
            Some(name.clone())
        } else if let Some(ref run) = pane.run {
            // If no explicit name, glean it from the run configuration
            match run {
                Run::Command(cmd) => {
                    // Use the command name
                    Some(cmd.command.to_string_lossy().to_string())
                },
                Run::EditFile(path, _line, _cwd) => {
                    // Use the file name
                    path.file_name().map(|n| n.to_string_lossy().to_string())
                },
                Run::Plugin(plugin) => {
                    is_plugin = true;
                    is_builtin_plugin = plugin.is_builtin_plugin();
                    Some(plugin.location_string())
                },
                Run::Cwd(_) => None,
            }
        } else {
            None
        };

        PaneMetadata {
            name,
            is_plugin,
            is_builtin_plugin,
        }
    }
}

impl From<&crate::input::layout::FloatingPaneLayout> for PaneMetadata {
    fn from(pane: &crate::input::layout::FloatingPaneLayout) -> Self {
        let mut is_plugin = false;
        let mut is_builtin_plugin = false;

        // Try to get the name from the pane's name field first
        let name = if let Some(ref name) = pane.name {
            Some(name.clone())
        } else if let Some(ref run) = pane.run {
            // If no explicit name, glean it from the run configuration
            match run {
                Run::Command(cmd) => {
                    // Use the command name
                    Some(cmd.command.to_string_lossy().to_string())
                },
                Run::EditFile(path, _line, _cwd) => {
                    // Use the file name
                    path.file_name().map(|n| n.to_string_lossy().to_string())
                },
                Run::Plugin(plugin) => {
                    is_plugin = true;
                    is_builtin_plugin = match plugin {
                        crate::input::layout::RunPluginOrAlias::RunPlugin(run_plugin) => {
                            matches!(run_plugin.location, RunPluginLocation::Zellij(_))
                        },
                        crate::input::layout::RunPluginOrAlias::Alias(_) => false,
                    };
                    // Use the plugin location string
                    Some(plugin.location_string())
                },
                Run::Cwd(_) => None,
            }
        } else {
            None
        };

        PaneMetadata {
            name,
            is_plugin,
            is_builtin_plugin,
        }
    }
}

// Helper function to recursively collect leaf panes from TiledPaneLayout
fn collect_leaf_panes(
    pane: &crate::input::layout::TiledPaneLayout,
    result: &mut Vec<PaneMetadata>,
) {
    if pane.children.is_empty() {
        // This is a leaf node (actual pane)
        result.push(PaneMetadata::from(pane));
    } else {
        // This is a container, recurse into children
        for child in &pane.children {
            collect_leaf_panes(child, result);
        }
    }
}

impl LayoutInfo {
    pub fn name(&self) -> &str {
        match self {
            LayoutInfo::BuiltIn(name) => &name,
            LayoutInfo::File(name, _) => &name,
            LayoutInfo::Url(url) => &url,
            LayoutInfo::Stringified(layout) => &layout,
        }
    }
    pub fn is_builtin(&self) -> bool {
        match self {
            LayoutInfo::BuiltIn(_name) => true,
            LayoutInfo::File(_name, _) => false,
            LayoutInfo::Url(_url) => false,
            LayoutInfo::Stringified(_stringified) => false,
        }
    }
    pub fn from_cli(
        layout_dir: &Option<PathBuf>,
        maybe_layout_path: &Option<PathBuf>,
        cwd: PathBuf,
    ) -> Option<Self> {
        // If we're not given a layout path, fall back to "default". Since we cannot tell ahead of
        // time whether the user has a layout named "default.kdl" in their layout directory, we
        // cannot blindly assume that this is indeed the builtin default layout. The layout
        // resolution below will correctly handle this.
        // The docs promise this behavior, so we have to abide:
        // <https://zellij.dev/documentation/layouts.html#layout-default-directory>
        let layout_path = maybe_layout_path
            .clone()
            .unwrap_or(PathBuf::from("default"));

        if layout_path.starts_with("http://") || layout_path.starts_with("https://") {
            Some(LayoutInfo::Url(layout_path.display().to_string()))
        } else if layout_path.extension().is_some() || layout_path.components().count() > 1 {
            let layout_dir = cwd;
            let file_path = layout_dir.join(layout_path);
            Some(LayoutInfo::File(
                // layout_dir.join(layout_path).display().to_string(),
                file_path.display().to_string(),
                LayoutMetadata::from(&file_path),
            ))
        } else {
            // Attempt to interpret the layout as bare layout name from the layout application
            // directory. This is described in the docs:
            // <https://zellij.dev/documentation/layouts.html#layout-default-directory>
            if let Some(layout_dir) = layout_dir
                .as_ref()
                .map(|l| l.clone())
                .or_else(default_layout_dir)
            {
                let file_path = layout_dir.join(&layout_path);
                if file_path.exists() {
                    return Some(LayoutInfo::File(
                        file_path.display().to_string(),
                        LayoutMetadata::from(&file_path),
                    ));
                }
                let file_path_with_ext = file_path.with_extension("kdl");
                if file_path_with_ext.exists() {
                    return Some(LayoutInfo::File(
                        file_path_with_ext.display().to_string(),
                        LayoutMetadata::from(&file_path_with_ext),
                    ));
                }
            }
            // Assume a builtin layout by default
            Some(LayoutInfo::BuiltIn(layout_path.display().to_string()))
        }
    }
    pub fn from_config(
        layout_dir: &Option<PathBuf>,
        maybe_layout_path: &Option<PathBuf>,
    ) -> Option<Self> {
        // If we're not given a layout path, fall back to "default". Since we cannot tell ahead of
        // time whether the user has a layout named "default.kdl" in their layout directory, we
        // cannot blindly assume that this is indeed the builtin default layout. The layout
        // resolution below will correctly handle this.
        // The docs promise this behavior, so we have to abide:
        // <https://zellij.dev/documentation/layouts.html#layout-default-directory>
        let layout_path = maybe_layout_path
            .clone()
            .unwrap_or(PathBuf::from("default"));

        if layout_path.starts_with("http://") || layout_path.starts_with("https://") {
            Some(LayoutInfo::Url(layout_path.display().to_string()))
        } else if layout_path.extension().is_some() || layout_path.components().count() > 1 {
            let Some(layout_dir) = layout_dir
                .as_ref()
                .map(|l| l.clone())
                .or_else(default_layout_dir)
            else {
                return None;
            };
            let file_path = layout_dir.join(layout_path);
            Some(LayoutInfo::File(
                // layout_dir.join(layout_path).display().to_string(),
                file_path.display().to_string(),
                LayoutMetadata::from(&file_path),
            ))
        } else {
            // Attempt to interpret the layout as bare layout name from the layout application
            // directory. This is described in the docs:
            // <https://zellij.dev/documentation/layouts.html#layout-default-directory>
            if let Some(layout_dir) = layout_dir
                .as_ref()
                .map(|l| l.clone())
                .or_else(default_layout_dir)
            {
                let file_path = layout_dir.join(&layout_path);
                if file_path.exists() {
                    return Some(LayoutInfo::File(
                        file_path.display().to_string(),
                        LayoutMetadata::from(&file_path),
                    ));
                }
                let file_path_with_ext = file_path.with_extension("kdl");
                if file_path_with_ext.exists() {
                    return Some(LayoutInfo::File(
                        file_path_with_ext.display().to_string(),
                        LayoutMetadata::from(&file_path_with_ext),
                    ));
                }
            }
            // Assume a builtin layout by default
            Some(LayoutInfo::BuiltIn(layout_path.display().to_string()))
        }
    }
}

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
    pub other_focused_client_slots: Vec<usize>,
    pub active_swap_layout_name: Option<String>,
    /// Whether the user manually changed the layout, moving out of the swap layout scheme
    pub is_swap_layout_dirty: bool,
    /// Row count in the viewport (including all non-ui panes, eg. will exclude the status bar)
    pub viewport_rows: usize,
    /// Column count in the viewport (including all non-ui panes, eg. will exclude the status bar)
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
    /// The stable identifier for this tab
    pub tab_id: usize,
    /// Whether this tab has an active (persistent) bell notification
    pub has_bell_notification: bool,
    /// Whether this tab is currently flashing its bell (transient 400ms state)
    pub is_flashing_bell: bool,
    pub attention: crate::attention::TabAttention,
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
    pub is_collapsed: bool,
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
    /// The default foreground color of this pane, if set (e.g. "#00e000")
    pub default_fg: Option<String>,
    /// The default background color of this pane, if set (e.g. "#001a3a")
    pub default_bg: Option<String>,
    pub nested_session_name: Option<String>,
    pub attention: crate::attention::PaneAttention,
    pub command_state: crate::attention::CommandState,
    pub program_status: Vec<crate::attention::ProgramStatusRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PaneListEntry {
    #[serde(flatten)]
    pub pane_info: PaneInfo,
    pub tab_id: usize,
    pub tab_position: usize,
    pub tab_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pane_cwd: Option<String>,
}

pub type ListPanesResponse = Vec<PaneListEntry>;
pub type ListTabsResponse = Vec<TabInfo>;

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

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaneRenderReport {
    pub all_pane_contents: HashMap<ClientId, HashMap<PaneId, PaneContents>>,
    pub all_pane_contents_with_ansi: HashMap<ClientId, HashMap<PaneId, PaneContents>>,
}

impl PaneRenderReport {
    pub fn add_pane_contents(
        &mut self,
        client_ids: &[ClientId],
        pane_id: PaneId,
        pane_contents: PaneContents,
    ) {
        for client_id in client_ids {
            let p = self
                .all_pane_contents
                .entry(*client_id)
                .or_insert_with(|| HashMap::new());
            p.insert(pane_id, pane_contents.clone());
        }
    }
    pub fn add_pane_contents_with_ansi(
        &mut self,
        client_ids: &[ClientId],
        pane_id: PaneId,
        pane_contents: PaneContents,
    ) {
        for client_id in client_ids {
            let p = self
                .all_pane_contents_with_ansi
                .entry(*client_id)
                .or_insert_with(|| HashMap::new());
            p.insert(pane_id, pane_contents.clone());
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PaneContents {
    // NOTE: both lines_above_viewport and lines_below_viewport are only populated if explicitly
    // requested (eg. with get_full_scrollback true in the plugin command) this is for performance
    // reasons
    pub lines_above_viewport: Vec<String>,
    pub lines_below_viewport: Vec<String>,
    pub viewport: Vec<String>,
    pub selected_text: Option<SelectedText>,
    pub cursor: Option<(usize, usize)>,
}

fn extract_text_by_columns(line: &str, start_col: usize, end_col: usize) -> String {
    let mut current_col = 0;
    let mut result = String::new();
    let mut capturing_current = false;
    for ch in line.chars() {
        let char_width = ch.width().unwrap_or(0);
        if char_width == 0 {
            if capturing_current {
                result.push(ch);
            }
            continue;
        }
        capturing_current = current_col >= start_col && current_col < end_col;
        if current_col >= end_col {
            break;
        }
        if capturing_current {
            result.push(ch);
        }
        current_col += char_width;
    }
    result
}

fn extract_text_from_column(line: &str, start_col: usize) -> String {
    extract_text_by_columns(line, start_col, usize::MAX)
}

fn extract_text_to_column(line: &str, end_col: usize) -> String {
    extract_text_by_columns(line, 0, end_col)
}

impl PaneContents {
    pub fn new(viewport: Vec<String>, selection_start: Position, selection_end: Position) -> Self {
        PaneContents {
            viewport,
            selected_text: SelectedText::from_positions(selection_start, selection_end),
            ..Default::default()
        }
    }
    pub fn new_with_scrollback(
        viewport: Vec<String>,
        selection_start: Position,
        selection_end: Position,
        lines_above_viewport: Vec<String>,
        lines_below_viewport: Vec<String>,
    ) -> Self {
        PaneContents {
            viewport,
            selected_text: SelectedText::from_positions(selection_start, selection_end),
            lines_above_viewport,
            lines_below_viewport,
            cursor: None,
        }
    }

    /// Returns the actual text content of the selection, if any exists.
    /// Selection only occurs within the viewport.
    pub fn get_selected_text(&self) -> Option<String> {
        let selected_text = self.selected_text?;

        let start_line = selected_text.start.line() as usize;
        let start_col = selected_text.start.column();
        let end_line = selected_text.end.line() as usize;
        let end_col = selected_text.end.column();

        // Handle out of bounds
        if start_line >= self.viewport.len() || end_line >= self.viewport.len() {
            return None;
        }

        if start_line == end_line {
            // Single line selection
            let line = &self.viewport[start_line];
            Some(extract_text_by_columns(line, start_col, end_col))
        } else {
            // Multi-line selection
            let mut result = String::new();

            // First line - from start column to end of line
            let first_line = &self.viewport[start_line];
            result.push_str(&extract_text_from_column(first_line, start_col));
            result.push('\n');

            // Middle lines - complete lines
            for i in (start_line + 1)..end_line {
                result.push_str(&self.viewport[i]);
                result.push('\n');
            }

            // Last line - from start to end column
            let last_line = &self.viewport[end_line];
            result.push_str(&extract_text_to_column(last_line, end_col));

            Some(result)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaneScrollbackResponse {
    Ok(PaneContents),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetPanePidResponse {
    Ok(i32),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetPaneRunningCommandResponse {
    Ok(Vec<String>),
    Err(String),
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionListSnapshot {
    pub live_sessions: Vec<SessionInfo>,
    pub resurrectable_sessions: Vec<(String, Duration)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetSessionListResponse {
    Ok(SessionListSnapshot),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KillSessionsResponse {
    Ok,
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeleteDeadSessionResponse {
    Ok,
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DeleteAllDeadSessionsResponse {
    Ok,
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetPaneCwdResponse {
    Ok(PathBuf),
    Err(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GetFocusedPaneInfoResponse {
    Ok { tab_index: usize, pane_id: PaneId },
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaveLayoutResponse {
    Ok(()),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeleteLayoutResponse {
    Ok(()),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenameLayoutResponse {
    Ok(()),
    Err(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditLayoutResponse {
    Ok(()),
    Err(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectedText {
    pub start: Position,
    pub end: Position,
}

impl SelectedText {
    pub fn new(start: Position, end: Position) -> Self {
        // Normalize: ensure start <= end
        let (normalized_start, normalized_end) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };

        // Normalize negative line values to 0
        // (column is already usize so can't be negative)
        let normalized_start = Position::new(
            normalized_start.line().max(0) as i32,
            normalized_start.column() as u16,
        );
        let normalized_end = Position::new(
            normalized_end.line().max(0) as i32,
            normalized_end.column() as u16,
        );

        SelectedText {
            start: normalized_start,
            end: normalized_end,
        }
    }

    pub fn from_positions(start: Position, end: Position) -> Option<Self> {
        if start == end {
            None
        } else {
            Some(Self::new(start, end))
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

pub type SlotId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum SlotKind {
    Pane,
    Background,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Slot {
    pub id: SlotId,
    pub kind: SlotKind,
    pub configuration: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct EventContext {
    pub slot_id: Option<SlotId>,
    pub client_id: Option<ClientId>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum RenderResponse {
    #[default]
    Nothing,
    All,
    Client(ClientId),
    Slots(Vec<SlotId>),
}

impl RenderResponse {
    pub fn merge(self, other: RenderResponse) -> RenderResponse {
        match (self, other) {
            (RenderResponse::Nothing, other) => other,
            (this, RenderResponse::Nothing) => this,
            (RenderResponse::All, _) | (_, RenderResponse::All) => RenderResponse::All,
            (RenderResponse::Client(a), RenderResponse::Client(b)) if a == b => {
                RenderResponse::Client(a)
            },
            (RenderResponse::Slots(mut a), RenderResponse::Slots(b)) => {
                for slot_id in b {
                    if !a.contains(&slot_id) {
                        a.push(slot_id);
                    }
                }
                RenderResponse::Slots(a)
            },
            _ => RenderResponse::All,
        }
    }
    pub fn from_bool(should_render: bool) -> RenderResponse {
        if should_render {
            RenderResponse::All
        } else {
            RenderResponse::Nothing
        }
    }
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

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileToOpen {
    pub path: PathBuf,
    pub line_number: Option<usize>,
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub border_style: Option<BorderStyleOverride>,
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
    pub fn with_border_style(mut self, border_style: BorderStyleOverride) -> Self {
        self.border_style = Some(border_style);
        self
    }
}

#[derive(Debug, Default, Clone)]
pub struct CommandToRun {
    pub path: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub border_style: Option<BorderStyleOverride>,
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
    pub fn with_border_style(mut self, border_style: BorderStyleOverride) -> Self {
        self.border_style = Some(border_style);
        self
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HostId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaneAddress {
    pub host: Option<HostId>,
    pub session: String,
    pub pane_id: PaneId,
}

impl PaneAddress {
    pub fn local(session: impl Into<String>, pane_id: PaneId) -> Self {
        PaneAddress {
            host: None,
            session: session.into(),
            pane_id,
        }
    }
    pub fn is_local(&self) -> bool {
        self.host.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MirrorClosedReason {
    PaneClosed,
    PaneNotFound,
    SessionEnding,
    Rejected,
    VersionMismatch,
    UnsupportedHost,
    ConnectionFailed,
    ConnectionLost,
    ProtocolError,
    ContainsViewer,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MirrorState {
    #[default]
    NoTarget,
    Connecting,
    Live,
    Closed(MirrorClosedReason),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiveView {
    pub target: PaneAddress,
    pub x: usize,
    pub y: usize,
    pub cols: usize,
    pub rows: usize,
    pub resize_allowed: Option<bool>,
    pub view_only: bool,
}

impl LiveView {
    pub fn new(target: PaneAddress, x: usize, y: usize, cols: usize, rows: usize) -> Self {
        LiveView {
            target,
            x,
            y,
            cols,
            rows,
            resize_allowed: None,
            view_only: false,
        }
    }
}

pub const DEFAULT_LIVE_VIEW_RELEASE_KEY: &str = "F9";
pub const LIVE_VIEW_DOUBLE_PRESS_MS: u64 = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveViewReleaseGesture {
    pub key: KeyWithModifier,
    pub double: bool,
}

impl LiveViewReleaseGesture {
    pub fn parse(text: &str) -> Option<Self> {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            return None;
        }
        if words.len() % 2 == 0 {
            let (first, second) = words.split_at(words.len() / 2);
            if first == second {
                if let Ok(key) = first.join(" ").parse::<KeyWithModifier>() {
                    return Some(LiveViewReleaseGesture { key, double: true });
                }
            }
        }
        words
            .join(" ")
            .parse::<KeyWithModifier>()
            .ok()
            .map(|key| LiveViewReleaseGesture { key, double: false })
    }

    pub fn display(text: &str) -> String {
        match Self::parse(text) {
            Some(gesture) if gesture.double => format!("<{}> twice", gesture.key),
            Some(gesture) => format!("<{}>", gesture.key),
            None => format!("<{}>", text.trim()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LiveViewRelease {
    Key,
    ClickOutside,
    FocusLost,
    Plugin,
    Cleared,
    NotAllowed,
}

impl Default for PaneId {
    fn default() -> Self {
        PaneId::Terminal(0)
    }
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

impl std::fmt::Display for PaneId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneId::Terminal(id) => write!(f, "terminal_{}", id),
            PaneId::Plugin(id) => write!(f, "plugin_{}", id),
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
    pub fn has_cwd(&self) -> bool {
        self.new_plugin_args
            .as_ref()
            .map(|n| n.cwd.is_some())
            .unwrap_or(false)
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionTerminalRequest {
    pub session_name: Option<String>,
    pub new_window: bool,
    pub pane_id: Option<PaneId>,
    pub layout: Option<LayoutInfo>,
    pub cwd: Option<PathBuf>,
}

impl SessionTerminalRequest {
    pub fn existing(session_name: &str, pane_id: Option<PaneId>, new_window: bool) -> Self {
        SessionTerminalRequest {
            session_name: Some(session_name.to_owned()),
            new_window,
            pane_id,
            ..Default::default()
        }
    }
    pub fn new_session(
        session_name: Option<&str>,
        layout: Option<LayoutInfo>,
        cwd: Option<PathBuf>,
    ) -> Self {
        SessionTerminalRequest {
            session_name: session_name.map(|n| n.to_owned()),
            new_window: true,
            layout,
            cwd,
            ..Default::default()
        }
    }
    pub fn creates_a_session(&self) -> bool {
        self.session_name.is_none() || self.layout.is_some()
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConnectToSession {
    pub name: Option<String>,
    pub tab_position: Option<usize>,
    pub pane_id: Option<(u32, bool)>, // (id, is_plugin)
    pub layout: Option<LayoutInfo>,
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub switch_context: Option<crate::session_history::SwitchContext>,
}

impl ConnectToSession {
    pub fn apply_layout_dir(&mut self, layout_dir: &PathBuf) {
        if let Some(LayoutInfo::File(file_path, _layout_metadata)) = self.layout.as_mut() {
            *file_path = Path::join(layout_dir, &file_path)
                .to_string_lossy()
                .to_string();
        }
    }
    pub fn startup_card(&self) -> Option<crate::input::attention::StartupCard> {
        self.switch_context.as_ref().and_then(|c| c.startup_card)
    }
    pub fn disconnects_others(&self) -> bool {
        self.switch_context
            .as_ref()
            .map(|c| c.disconnect_others)
            .unwrap_or(false)
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
    PromptRequest {
        caller_plugin_id: u32,
        request_id: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum PopupCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl PopupCorner {
    pub fn is_top(&self) -> bool {
        matches!(self, PopupCorner::TopLeft | PopupCorner::TopRight)
    }
    pub fn is_left(&self) -> bool {
        matches!(self, PopupCorner::TopLeft | PopupCorner::BottomLeft)
    }
}

impl FromStr for PopupCorner {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "top-left" => Ok(PopupCorner::TopLeft),
            "top-right" => Ok(PopupCorner::TopRight),
            "bottom-left" => Ok(PopupCorner::BottomLeft),
            "bottom-right" => Ok(PopupCorner::BottomRight),
            other => Err(format!(
                "invalid popup corner '{}', expected one of: top-left, top-right, bottom-left, bottom-right",
                other
            )),
        }
    }
}

impl fmt::Display for PopupCorner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PopupCorner::TopLeft => write!(f, "top-left"),
            PopupCorner::TopRight => write!(f, "top-right"),
            PopupCorner::BottomLeft => write!(f, "bottom-left"),
            PopupCorner::BottomRight => write!(f, "bottom-right"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum OutsideClick {
    #[default]
    Block,
    Close,
}

impl FromStr for OutsideClick {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "block" => Ok(OutsideClick::Block),
            "close" => Ok(OutsideClick::Close),
            other => Err(format!(
                "invalid outside click behavior '{}', expected one of: block, close",
                other
            )),
        }
    }
}

impl fmt::Display for OutsideClick {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OutsideClick::Block => write!(f, "block"),
            OutsideClick::Close => write!(f, "close"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct PopupOptions {
    pub focused: bool,
    pub corner: Option<PopupCorner>,
    pub capture_all_keys: bool,
    pub outside_click: Option<OutsideClick>,
}

impl Default for PopupOptions {
    fn default() -> Self {
        PopupOptions {
            focused: true,
            corner: None,
            capture_all_keys: false,
            outside_click: None,
        }
    }
}

impl PopupOptions {
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
    pub fn corner(mut self, corner: PopupCorner) -> Self {
        self.corner = Some(corner);
        self
    }
    pub fn capture_all_keys(mut self, capture_all_keys: bool) -> Self {
        self.capture_all_keys = capture_all_keys;
        self
    }
    pub fn outside_click(mut self, outside_click: OutsideClick) -> Self {
        self.outside_click = Some(outside_click);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Deserialize, Serialize)]
pub enum PipePopupPlacement {
    #[default]
    Pane,
    Center,
    Mouse,
    Cursor,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl PipePopupPlacement {
    pub fn corner(&self) -> Option<PopupCorner> {
        match self {
            PipePopupPlacement::TopLeft => Some(PopupCorner::TopLeft),
            PipePopupPlacement::TopRight => Some(PopupCorner::TopRight),
            PipePopupPlacement::BottomLeft => Some(PopupCorner::BottomLeft),
            PipePopupPlacement::BottomRight => Some(PopupCorner::BottomRight),
            _ => None,
        }
    }
}

impl FromStr for PipePopupPlacement {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "pane" => Ok(PipePopupPlacement::Pane),
            "center" | "centre" => Ok(PipePopupPlacement::Center),
            "mouse" => Ok(PipePopupPlacement::Mouse),
            "cursor" => Ok(PipePopupPlacement::Cursor),
            "top-left" => Ok(PipePopupPlacement::TopLeft),
            "top-right" => Ok(PipePopupPlacement::TopRight),
            "bottom-left" => Ok(PipePopupPlacement::BottomLeft),
            "bottom-right" => Ok(PipePopupPlacement::BottomRight),
            other => Err(format!(
                "invalid popup placement '{}', expected one of: pane, center, mouse, cursor, top-left, top-right, bottom-left, bottom-right",
                other
            )),
        }
    }
}

impl fmt::Display for PipePopupPlacement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PipePopupPlacement::Pane => write!(f, "pane"),
            PipePopupPlacement::Center => write!(f, "center"),
            PipePopupPlacement::Mouse => write!(f, "mouse"),
            PipePopupPlacement::Cursor => write!(f, "cursor"),
            PipePopupPlacement::TopLeft => write!(f, "top-left"),
            PipePopupPlacement::TopRight => write!(f, "top-right"),
            PipePopupPlacement::BottomLeft => write!(f, "bottom-left"),
            PipePopupPlacement::BottomRight => write!(f, "bottom-right"),
        }
    }
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
    pub x: Option<PercentOrFixed>,
    pub y: Option<PercentOrFixed>,
    pub width: Option<PercentOrFixed>,
    pub height: Option<PercentOrFixed>,
    pub pinned: Option<bool>,
    pub borderless: Option<bool>,
    #[serde(default)]
    pub border_style: Option<BorderStyleOverride>,
}

impl FloatingPaneCoordinates {
    pub fn new(
        x: Option<String>,
        y: Option<String>,
        width: Option<String>,
        height: Option<String>,
        pinned: Option<bool>,
        borderless: Option<bool>,
    ) -> Option<Self> {
        // Parse x/y coordinates - allows 0% or 0
        let x = x.and_then(|x| PercentOrFixed::from_str(&x).ok());
        let y = y.and_then(|y| PercentOrFixed::from_str(&y).ok());

        // Parse width/height - reject 0% or 0
        let width = width.and_then(|w| {
            PercentOrFixed::from_str(&w)
                .ok()
                .and_then(|size| match size {
                    PercentOrFixed::Percent(0) => None,
                    PercentOrFixed::Fixed(0) => None,
                    _ => Some(size),
                })
        });
        let height = height.and_then(|h| {
            PercentOrFixed::from_str(&h)
                .ok()
                .and_then(|size| match size {
                    PercentOrFixed::Percent(0) => None,
                    PercentOrFixed::Fixed(0) => None,
                    _ => Some(size),
                })
        });

        if x.is_none()
            && y.is_none()
            && width.is_none()
            && height.is_none()
            && pinned.is_none()
            && borderless.is_none()
        {
            None
        } else {
            Some(FloatingPaneCoordinates {
                x,
                y,
                width,
                height,
                pinned,
                borderless,
                border_style: None,
            })
        }
    }
    pub fn with_border_style(mut self, border_style: Option<BorderStyleOverride>) -> Self {
        self.border_style = border_style;
        self
    }
    pub fn merge_border_style(
        coordinates: Option<Self>,
        border_style: Option<BorderStyleOverride>,
    ) -> Option<Self> {
        match border_style {
            Some(border_style) => Some(
                coordinates
                    .unwrap_or_default()
                    .with_border_style(Some(border_style)),
            ),
            None => coordinates,
        }
    }
    pub fn with_x_fixed(mut self, x: usize) -> Self {
        self.x = Some(PercentOrFixed::Fixed(x));
        self
    }
    pub fn with_x_percent(mut self, x: usize) -> Self {
        if x > 100 {
            eprintln!("x must be between 0 and 100");
            return self;
        }
        self.x = Some(PercentOrFixed::Percent(x));
        self
    }
    pub fn with_y_fixed(mut self, y: usize) -> Self {
        self.y = Some(PercentOrFixed::Fixed(y));
        self
    }
    pub fn with_y_percent(mut self, y: usize) -> Self {
        if y > 100 {
            eprintln!("y must be between 0 and 100");
            return self;
        }
        self.y = Some(PercentOrFixed::Percent(y));
        self
    }
    pub fn with_width_fixed(mut self, width: usize) -> Self {
        self.width = Some(PercentOrFixed::Fixed(width));
        self
    }
    pub fn with_width_percent(mut self, width: usize) -> Self {
        if width > 100 {
            eprintln!("width must be between 0 and 100");
            return self;
        }
        self.width = Some(PercentOrFixed::Percent(width));
        self
    }
    pub fn with_height_fixed(mut self, height: usize) -> Self {
        self.height = Some(PercentOrFixed::Fixed(height));
        self
    }
    pub fn with_height_percent(mut self, height: usize) -> Self {
        if height > 100 {
            eprintln!("height must be between 0 and 100");
            return self;
        }
        self.height = Some(PercentOrFixed::Percent(height));
        self
    }
}

impl From<PaneGeom> for FloatingPaneCoordinates {
    fn from(pane_geom: PaneGeom) -> Self {
        FloatingPaneCoordinates {
            x: Some(PercentOrFixed::Fixed(pane_geom.x)),
            y: Some(PercentOrFixed::Fixed(pane_geom.y)),
            width: Some(PercentOrFixed::Fixed(pane_geom.cols.as_usize())),
            height: Some(PercentOrFixed::Fixed(pane_geom.rows.as_usize())),
            pinned: Some(pane_geom.is_pinned),
            borderless: None,
            border_style: None,
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

#[derive(ValueEnum, Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum NewPanePlacement {
    NoPreference {
        borderless: Option<bool>,
        border_style: Option<BorderStyleOverride>,
    },
    Tiled {
        direction: Option<Direction>,
        borderless: Option<bool>,
        border_style: Option<BorderStyleOverride>,
    },
    Floating(Option<FloatingPaneCoordinates>),
    InPlace {
        pane_id_to_replace: Option<PaneId>,
        close_replaced_pane: bool,
        borderless: Option<bool>,
        border_style: Option<BorderStyleOverride>,
    },
    Stacked {
        pane_id_to_stack_under: Option<PaneId>,
        borderless: Option<bool>,
        border_style: Option<BorderStyleOverride>,
    },
}

impl Default for NewPanePlacement {
    fn default() -> Self {
        NewPanePlacement::NoPreference {
            borderless: None,
            border_style: None,
        }
    }
}

impl NewPanePlacement {
    pub fn with_floating_pane_coordinates(
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    ) -> Self {
        NewPanePlacement::Floating(floating_pane_coordinates)
    }
    pub fn with_should_be_in_place(
        self,
        should_be_in_place: bool,
        close_replaced_pane: bool,
    ) -> Self {
        if should_be_in_place {
            NewPanePlacement::InPlace {
                pane_id_to_replace: None,
                close_replaced_pane,
                borderless: None,
                border_style: None,
            }
        } else {
            self
        }
    }
    pub fn with_pane_id_to_replace(
        pane_id_to_replace: Option<PaneId>,
        close_replaced_pane: bool,
    ) -> Self {
        NewPanePlacement::InPlace {
            pane_id_to_replace,
            close_replaced_pane,
            borderless: None,
            border_style: None,
        }
    }
    pub fn should_float(&self) -> Option<bool> {
        match self {
            NewPanePlacement::Floating(_) => Some(true),
            NewPanePlacement::Tiled { .. } => Some(false),
            _ => None,
        }
    }
    pub fn floating_pane_coordinates(&self) -> Option<FloatingPaneCoordinates> {
        match self {
            NewPanePlacement::Floating(floating_pane_coordinates) => {
                floating_pane_coordinates.clone()
            },
            _ => None,
        }
    }
    pub fn should_stack(&self) -> bool {
        match self {
            NewPanePlacement::Stacked { .. } => true,
            _ => false,
        }
    }
    pub fn id_of_stack_root(&self) -> Option<PaneId> {
        match self {
            NewPanePlacement::Stacked {
                pane_id_to_stack_under,
                ..
            } => *pane_id_to_stack_under,
            _ => None,
        }
    }
    pub fn get_borderless(&self) -> Option<bool> {
        match self {
            NewPanePlacement::NoPreference { borderless, .. } => *borderless,
            NewPanePlacement::Tiled { borderless, .. } => *borderless,
            NewPanePlacement::Floating(coords) => coords.as_ref().and_then(|c| c.borderless),
            NewPanePlacement::InPlace { borderless, .. } => *borderless,
            NewPanePlacement::Stacked { borderless, .. } => *borderless,
        }
    }
    pub fn get_border_style(&self) -> Option<BorderStyleOverride> {
        match self {
            NewPanePlacement::NoPreference { border_style, .. } => *border_style,
            NewPanePlacement::Tiled { border_style, .. } => *border_style,
            NewPanePlacement::Floating(coords) => {
                coords.as_ref().and_then(|c| c.border_style.clone())
            },
            NewPanePlacement::InPlace { border_style, .. } => *border_style,
            NewPanePlacement::Stacked { border_style, .. } => *border_style,
        }
    }
    pub fn with_border_style(self, border_style: Option<BorderStyleOverride>) -> Self {
        if border_style.is_none() {
            return self;
        }
        match self {
            NewPanePlacement::NoPreference { borderless, .. } => NewPanePlacement::NoPreference {
                borderless,
                border_style,
            },
            NewPanePlacement::Tiled {
                direction,
                borderless,
                ..
            } => NewPanePlacement::Tiled {
                direction,
                borderless,
                border_style,
            },
            NewPanePlacement::Floating(coords) => NewPanePlacement::Floating(Some(
                coords.unwrap_or_default().with_border_style(border_style),
            )),
            NewPanePlacement::InPlace {
                pane_id_to_replace,
                close_replaced_pane,
                borderless,
                ..
            } => NewPanePlacement::InPlace {
                pane_id_to_replace,
                close_replaced_pane,
                borderless,
                border_style,
            },
            NewPanePlacement::Stacked {
                pane_id_to_stack_under,
                borderless,
                ..
            } => NewPanePlacement::Stacked {
                pane_id_to_stack_under,
                borderless,
                border_style,
            },
        }
    }
}

type Context = BTreeMap<String, String>;

#[derive(Debug, Clone, EnumDiscriminants, Display)]
#[strum_discriminants(derive(EnumString, Hash, Serialize, Deserialize))]
#[strum_discriminants(name(CommandType))]
pub enum PluginCommand {
    Subscribe(HashSet<EventType>),
    Unsubscribe(HashSet<EventType>),
    SetSelectable(bool),
    ShowCursor(Option<(usize, usize)>),
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
    NewTabUnfocused {
        name: Option<String>,
        cwd: Option<String>,
    },
    NewTiledPaneInTab {
        tab_position: usize,
    },
    ToggleFloatingPanes {
        tab_id: Option<u64>,
    },
    NewPane,
    GoToNextTab,
    GoToPreviousTab,
    Resize(Resize),
    ResizeWithDirection(ResizeStrategy),
    FocusNextPane,
    FocusPreviousPane,
    FocusLastPane,
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
    ToggleFocusNoUiFullscreen,
    TogglePaneFrames,
    SetPaneFrameStyle(PaneFrameStyle),
    TogglePaneEmbedOrEject,
    UndoRenamePane,
    CloseFocus,
    ToggleActiveTabSync,
    CloseFocusedTab,
    UndoRenameTab,
    QuitZellij,
    PreviousSwapLayout,
    NextSwapLayout,
    ApplyTiledSwapLayout(String),
    ApplyFloatingSwapLayout(String),
    GoToTabName(String),
    FocusOrCreateTab(String),
    GoToTab(u32),                       // tab index
    StartOrReloadPlugin(String),        // plugin url (eg. file:/path/to/plugin.wasm)
    CloseTerminalPane(u32),             // terminal pane id
    ClosePluginPane(u32),               // plugin pane id
    FocusTerminalPane(u32, bool, bool), // terminal pane id, should_float_if_hidden, should_be_in_place_if_hidden
    FocusPluginPane(u32, bool, bool), // plugin pane id, should_float_if_hidden, should_be_in_place_if_hidden
    RenameTerminalPane(u32, String),  // terminal pane id, new name
    RenamePluginPane(u32, String),    // plugin pane id, new name
    RenameTab(u32, String),           // tab index, new name
    ReportPanic(String),              // stringified panic
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
    SetCliPipeExitCode(String, i32),
    MessageToPlugin(MessageToPlugin),
    DisconnectOtherClients,
    KillSessions(Vec<String>), // one or more session names
    ScanHostFolder(PathBuf),   // TODO: rename to ScanHostFolder
    WatchFilesystem,
    DumpSessionLayout {
        tab_index: Option<usize>,
    },
    CloseSelf,
    NewTabsWithLayoutInfo(LayoutInfo),
    Reconfigure(String, bool), // String -> stringified configuration, bool -> save configuration
    // file to disk
    HidePaneWithId(PaneId),
    ShowPaneWithId(PaneId, bool, bool), // bools -> should_float_if_hidden, should_focus_pane
    OpenCommandPaneBackground(CommandToRun, Context),
    RerunCommandPane(u32), // u32  - terminal pane id
    ResizePaneIdWithDirection(ResizeStrategy, PaneId),
    EditScrollbackForPaneWithId(PaneId),
    GetPaneScrollback {
        pane_id: PaneId,
        get_full_scrollback: bool,
    },
    WriteToPaneId(Vec<u8>, PaneId),
    WriteCharsToPaneId(String, PaneId),
    SendSigintToPaneId(PaneId),
    SendSigkillToPaneId(PaneId),
    GetPanePid {
        pane_id: PaneId,
    },
    GetPaneRunningCommand {
        pane_id: PaneId,
    },
    GetPaneCwd {
        pane_id: PaneId,
    },
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
    SwitchTabToId(u64),                            // u64 - tab_id
    GoToTabWithId(u64),                            // u64 - tab_id
    CloseTabWithId(u64),                           // u64 - tab_id
    RenameTabWithId(u64, String),                  // u64 - tab_id, String - new name
    BreakPanesToTabWithId(Vec<PaneId>, u64, bool), // u64 - tab_id, bool -
    // should_change_focus_to_target_tab
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
    TogglePaneBorderless(PaneId),
    SetPaneBorderless(PaneId, bool),
    SetPaneBorderStyle(PaneId, BorderStyleOverride),
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
    /// Give this plugin's own row or column in the layout back to its neighbors while it has
    /// nothing to draw, and take it again when it does. The pane keeps its place in the layout
    /// the whole time, so expanding restores exactly the size the layout asked for.
    SetSelfCollapsed(bool),
    GenerateWebLoginToken(Option<String>, bool), // (token_label, read_only)
    RevokeWebLoginToken(String), // String -> token id (provided name or generated id)
    ListWebLoginTokens,
    RevokeAllWebLoginTokens,
    RenameWebLoginToken(String, String), // (original_name, new_name)
    InterceptKeyPresses,
    ClearKeyPressesIntercepts,
    ReplacePaneWithExistingPane(PaneId, PaneId, bool), // (pane id to replace, pane id of existing,
    // suppress_replaced_pane)
    RunAction(Action, BTreeMap<String, String>),
    CopyToClipboard(String), // text to copy
    OverrideLayout(
        LayoutInfo,
        bool,                     // retain_existing_terminal_panes
        bool,                     // retain_existing_plugin_panes
        bool,                     // apply_only_to_active_tab,
        BTreeMap<String, String>, // context
    ),
    SaveLayout {
        layout_name: String,
        layout_kdl: String,
        overwrite: bool,
    },
    DeleteLayout {
        layout_name: String,
    },
    RenameLayout {
        old_layout_name: String,
        new_layout_name: String,
    },
    EditLayout {
        layout_name: String,
        context: Context,
    },
    GenerateRandomName,
    DumpLayout(String),
    ParseLayout(String), // String contains raw KDL layout
    GetLayoutDir,
    GetFocusedPaneInfo,
    SaveSession,
    CurrentSessionLastSavedTime,
    GetPaneInfo(PaneId),
    GetTabInfo(usize), // tab_id
    GetSessionEnvironmentVariables,
    OpenCommandPaneInNewTab(CommandToRun, Context),
    OpenPluginPaneInNewTab {
        plugin_url: String,
        configuration: BTreeMap<String, String>,
        context: Context,
    },
    OpenEditorPaneInNewTab(FileToOpen, Context),
    OpenCommandPaneInPlaceOfPaneId(PaneId, CommandToRun, bool, Context), // bool = close_replaced_pane
    OpenTerminalPaneInPlaceOfPaneId(PaneId, FileToOpen, bool),
    OpenEditPaneInPlaceOfPaneId(PaneId, FileToOpen, bool, Context),
    HideFloatingPanes {
        tab_id: Option<usize>,
    },
    ShowFloatingPanes {
        tab_id: Option<usize>,
    },
    SetPaneColor(PaneId, Option<String>, Option<String>), // (pane_id, fg, bg)
    SetPaneRegexHighlights(PaneId, Vec<RegexHighlight>),
    ClearPaneHighlights(PaneId),
    OpenPluginPaneFloating {
        plugin_url: String,
        configuration: BTreeMap<String, String>,
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
        context: BTreeMap<String, String>,
    },
    ListWindowsVolumes,
    GetSessionList,
    KillSessionsAndReply(Vec<String>), // one or more session names; sends a response back
    DeleteDeadSessionAndReply(String), // session name; sends a response back
    DeleteAllDeadSessionsAndReply,     // no payload; sends a response back
    SetSoftKeyboard(bool),
    FocusHostSession,
    GetNestedSessionKeybinds(PaneId),
    SetSelectableSlot(SlotId, bool),
    HideSlot(SlotId),
    ShowSlot(SlotId, bool),
    CloseSlot(SlotId),
    SetCollapsedSlot(SlotId, bool),
    OpenContextMenu {
        target: ContextMenuTarget,
        line: usize,
        column: usize,
    },
    OpenPluginPopup {
        plugin_url: String,
        configuration: BTreeMap<String, String>,
        line: usize,
        column: usize,
        width: usize,
        height: usize,
        options: PopupOptions,
    },
    SetPopupSize {
        width: usize,
        height: usize,
    },
    RunContextMenuItem(usize),
    ReadConfig,
    RevertConfig(Option<SettingKey>),
    UnsetConfigSetting(SettingKey),
    SaveConfig,
    OverwriteConfigFile,
    ReloadConfigFile,
    CopyKeybindPreset {
        preset: String,
        new_name: String,
    },
    ReplaceConfigBlocks(String),
    SaveKeybindsAsPreset {
        new_name: String,
    },
    WriteThemeFile {
        name: String,
        copy_from: Option<String>,
        colours: Vec<String>,
    },
    DeleteThemeFile {
        name: String,
    },
    ResetKeys {
        keys: Vec<(InputMode, KeyWithModifier)>,
        write_config_to_disk: bool,
    },
    ResetMousebinds {
        triggers: Vec<(InputMode, MouseTrigger)>,
        write_config_to_disk: bool,
    },
    Prompt {
        request_id: u64,
        request: crate::prompt::PromptRequest,
    },
    ReplyToPrompt {
        request_id: u64,
        result: crate::prompt::PromptResult,
    },
    GetSessionSuggestions {
        limit: usize,
    },
    SetPopupFocused(bool),
    PopupToFloatingPane(Option<FloatingPaneCoordinates>),
    SwitchSessionAndCloseCurrent(ConnectToSession),
    GetSessionPreview {
        name: String,
        tab_index: Option<usize>,
        pane_id: Option<(u32, bool)>,
        anchor: Option<PreviewAnchor>,
    },
    GetSavedSessionPreview(String),
    DesktopIntegration(String),
    MarkSessionPaneSeen {
        session_name: String,
        pane_id: PaneId,
    },
    MarkAllSeen {
        session_name: Option<String>,
    },
    OpenSessionTerminal(SessionTerminalRequest),
    WriteToSessionPane {
        session_name: String,
        pane_id: PaneId,
        bytes: Vec<u8>,
    },
    SendKeyToSessionPane {
        session_name: String,
        pane_id: PaneId,
        key: KeyWithModifier,
    },
    ToggleSessionPaneMute {
        session_name: String,
        pane_id: PaneId,
    },
    ToggleSessionPaneIgnoreOutput {
        session_name: String,
        pane_id: PaneId,
    },
    ToggleSessionPaneWatch {
        session_name: String,
        pane_id: PaneId,
    },
    FocusSessionPane {
        session_name: String,
        pane_id: PaneId,
    },
    SetLiveView(Option<LiveView>),
    SetLiveViewInput(bool),
    ToggleBars(Vec<String>),
    HideBars(Vec<String>),
    ShowBars,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SettingSection {
    TopLevel,
    PaneFrames,
    WebClient,
    Terminal,
    TerminalTheme,
    Keybinds,
    Blocks,
    Attention,
    AttentionPopups,
    AttentionBadge,
    AttentionBadgeColors,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SettingValueShape {
    Flag,
    Number,
    Decimal,
    Text,
    List,
    Colour,
}

pub const SETTING_LIST_SEPARATOR: &str = ", ";

pub fn split_setting_list(text: &str) -> Vec<String> {
    text.split(SETTING_LIST_SEPARATOR)
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .map(|item| item.to_owned())
        .collect()
}

pub fn join_setting_list<S: AsRef<str>>(items: &[S]) -> String {
    items
        .iter()
        .map(|item| item.as_ref())
        .collect::<Vec<_>>()
        .join(SETTING_LIST_SEPARATOR)
}

macro_rules! setting_keys {
    ($($variant:ident => ($section:ident, $kdl_name:literal, $requires_restart:literal, $shape:ident)),* $(,)?) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, EnumIter,
        )]
        pub enum SettingKey {
            $($variant),*
        }
        impl SettingKey {
            pub fn kdl_name(&self) -> &'static str {
                match self {
                    $(SettingKey::$variant => $kdl_name),*
                }
            }
            pub fn section(&self) -> SettingSection {
                match self {
                    $(SettingKey::$variant => SettingSection::$section),*
                }
            }
            pub fn requires_restart(&self) -> bool {
                match self {
                    $(SettingKey::$variant => $requires_restart),*
                }
            }
            pub fn value_shape(&self) -> SettingValueShape {
                match self {
                    $(SettingKey::$variant => SettingValueShape::$shape),*
                }
            }
        }
    };
}

setting_keys! {
    SimplifiedUi => (TopLevel, "simplified_ui", false, Flag),
    Theme => (TopLevel, "theme", false, Text),
    ThemeDark => (TopLevel, "theme_dark", false, Text),
    ThemeLight => (TopLevel, "theme_light", false, Text),
    ExplicitThemeHue => (TopLevel, "explicit_theme_hue", false, Text),
    DefaultMode => (TopLevel, "default_mode", false, Text),
    DefaultShell => (TopLevel, "default_shell", false, Text),
    DefaultCwd => (TopLevel, "default_cwd", false, Text),
    DefaultLayout => (TopLevel, "default_layout", true, Text),
    LayoutDir => (TopLevel, "layout_dir", false, Text),
    ThemeDir => (TopLevel, "theme_dir", false, Text),
    KeybindsDir => (TopLevel, "keybinds_dir", false, Text),
    MouseMode => (TopLevel, "mouse_mode", true, Flag),
    PaneFrames => (TopLevel, "pane_frames", false, Flag),
    PaneFrameStyle => (TopLevel, "pane_frame_style", false, Text),
    MirrorSession => (TopLevel, "mirror_session", true, Flag),
    OnForceClose => (TopLevel, "on_force_close", true, Text),
    ScrollBufferSize => (TopLevel, "scroll_buffer_size", true, Number),
    CopyCommand => (TopLevel, "copy_command", false, Text),
    CopyClipboard => (TopLevel, "copy_clipboard", false, Text),
    CopyOnSelect => (TopLevel, "copy_on_select", false, Flag),
    Osc8Hyperlinks => (TopLevel, "osc8_hyperlinks", true, Flag),
    ScrollbackEditor => (TopLevel, "scrollback_editor", false, Text),
    SessionName => (TopLevel, "session_name", true, Text),
    AttachToSession => (TopLevel, "attach_to_session", true, Flag),
    AutoLayout => (TopLevel, "auto_layout", false, Flag),
    SessionSerialization => (TopLevel, "session_serialization", true, Flag),
    SerializePaneViewport => (TopLevel, "serialize_pane_viewport", true, Flag),
    ScrollbackLinesToSerialize => (TopLevel, "scrollback_lines_to_serialize", true, Number),
    StyledUnderlines => (TopLevel, "styled_underlines", true, Flag),
    SerializationInterval => (TopLevel, "serialization_interval", true, Number),
    DisableSessionMetadata => (TopLevel, "disable_session_metadata", true, Flag),
    SupportKittyKeyboardProtocol => (TopLevel, "support_kitty_keyboard_protocol", true, Flag),
    SupportKittyGraphicsProtocol => (TopLevel, "support_kitty_graphics_protocol", true, Flag),
    GraphemeClusters => (TopLevel, "grapheme_clusters", false, Flag),
    MirrorResizesSourcePane => (TopLevel, "mirror_resizes_source_pane", false, Flag),
    LiveViewReleaseKey => (TopLevel, "live_view_release_key", false, Text),
    WebServer => (TopLevel, "web_server", true, Flag),
    WebSharing => (TopLevel, "web_sharing", true, Text),
    StackedResize => (TopLevel, "stacked_resize", false, Flag),
    StackedPaneList => (TopLevel, "stacked_pane_list", false, Flag),
    ShowStartupTips => (TopLevel, "show_startup_tips", true, Flag),
    ShowReleaseNotes => (TopLevel, "show_release_notes", true, Flag),
    AdvancedMouseActions => (TopLevel, "advanced_mouse_actions", false, Flag),
    MouseScrollResize => (TopLevel, "mouse_scroll_resize", false, Flag),
    ScrollModeSync => (TopLevel, "scroll_mode_sync", false, Flag),
    MouseHoverEffects => (TopLevel, "mouse_hover_effects", false, Flag),
    MouseHoverTips => (TopLevel, "mouse_hover_tips", false, Flag),
    VisualBell => (TopLevel, "visual_bell", false, Flag),
    FocusFollowsMouse => (TopLevel, "focus_follows_mouse", false, Flag),
    MouseClickThrough => (TopLevel, "mouse_click_through", false, Flag),
    ContextMenuEnabled => (TopLevel, "context_menu_enabled", false, Flag),
    Osc133CommandSelection => (TopLevel, "osc133_command_selection", false, Flag),
    WordSeparators => (TopLevel, "word_separators", false, Text),
    HostNotificationProtocol => (TopLevel, "host_notification_protocol", false, Text),
    WebServerIp => (TopLevel, "web_server_ip", true, Text),
    WebServerPort => (TopLevel, "web_server_port", true, Number),
    WebServerCert => (TopLevel, "web_server_cert", true, Text),
    WebServerKey => (TopLevel, "web_server_key", true, Text),
    EnforceHttpsForLocalhost => (TopLevel, "enforce_https_for_localhost", true, Flag),
    DangerouslyAllowWebServingWithoutACertificate => (TopLevel, "dangerously_allow_web_serving_without_a_certificate", true, Flag),
    PostCommandDiscoveryHook => (TopLevel, "post_command_discovery_hook", false, Text),
    ClientAsyncWorkerTasks => (TopLevel, "client_async_worker_tasks", true, Number),
    NestedSessionHandling => (TopLevel, "nested_session_handling", false, Text),
    DangerouslyEnablePasteBufferRead => (TopLevel, "dangerously_enable_paste_buffer_read", false, Flag),
    SessionIndicator => (TopLevel, "session_indicator", false, Flag),
    SwapLayoutNotification => (TopLevel, "swap_layout_notification", false, Flag),
    OnQuit => (TopLevel, "on_quit", false, Text),
    BarsHiddenOnStart => (TopLevel, "bars_hidden_on_start", false, Text),
    HiddenBarsNotification => (TopLevel, "hidden_bars_notification", false, Flag),
    ModeHintsWhenBarsHidden => (TopLevel, "mode_hints_when_bars_hidden", false, Flag),
    FrameRoundedCorners => (PaneFrames, "rounded_corners", false, Flag),
    FrameHideSessionName => (PaneFrames, "hide_session_name", false, Flag),
    FrameBorderStyle => (PaneFrames, "border_style", false, Text),
    FrameBorderTop => (PaneFrames, "border_top", false, Text),
    FrameBorderRight => (PaneFrames, "border_right", false, Text),
    FrameBorderBottom => (PaneFrames, "border_bottom", false, Text),
    FrameBorderLeft => (PaneFrames, "border_left", false, Text),
    FrameBorderRoundedCorners => (PaneFrames, "border_rounded_corners", false, Flag),
    FrameFloatingBorderStyle => (PaneFrames, "floating_border_style", false, Text),
    FrameFloatingBorderTop => (PaneFrames, "floating_border_top", false, Text),
    FrameFloatingBorderRight => (PaneFrames, "floating_border_right", false, Text),
    FrameFloatingBorderBottom => (PaneFrames, "floating_border_bottom", false, Text),
    FrameFloatingBorderLeft => (PaneFrames, "floating_border_left", false, Text),
    FrameFloatingBorderRoundedCorners => (PaneFrames, "floating_border_rounded_corners", false, Flag),
    WebClientFont => (WebClient, "font", true, Text),
    WebClientFontSize => (WebClient, "font_size", true, Number),
    WebClientCursorBlink => (WebClient, "cursor_blink", true, Flag),
    WebClientCursorStyle => (WebClient, "cursor_style", true, Text),
    WebClientCursorInactiveStyle => (WebClient, "cursor_inactive_style", true, Text),
    WebClientMacOptionIsMeta => (WebClient, "mac_option_is_meta", true, Flag),
    WebClientBaseUrl => (WebClient, "base_url", true, Text),
    TerminalFont => (Terminal, "font", false, Text),
    TerminalFontSize => (Terminal, "font_size", false, Decimal),
    TerminalFontWeight => (Terminal, "font_weight", false, Number),
    TerminalSystemFonts => (Terminal, "system_fonts", false, Flag),
    TerminalLigatures => (Terminal, "ligatures", false, Flag),
    TerminalFontFeatures => (Terminal, "font_features", false, List),
    TerminalCursorStyle => (Terminal, "cursor_style", false, Text),
    TerminalCursorBlink => (Terminal, "cursor_blink", false, Flag),
    TerminalCursorUnfocusedHollow => (Terminal, "cursor_unfocused_hollow", false, Flag),
    TerminalStartupMode => (Terminal, "startup_mode", true, Text),
    TerminalTerminalRequests => (Terminal, "terminal_requests", false, Text),
    TerminalInitialColumns => (Terminal, "initial_columns", true, Number),
    TerminalInitialRows => (Terminal, "initial_rows", true, Number),
    TerminalOpacity => (Terminal, "opacity", false, Decimal),
    TerminalOpacityMode => (Terminal, "opacity_mode", false, Text),
    TerminalBlur => (Terminal, "blur", false, Flag),
    TerminalPadding => (Terminal, "padding", false, Decimal),
    TerminalPaddingTop => (Terminal, "padding_top", false, Decimal),
    TerminalPaddingRight => (Terminal, "padding_right", false, Decimal),
    TerminalPaddingBottom => (Terminal, "padding_bottom", false, Decimal),
    TerminalPaddingLeft => (Terminal, "padding_left", false, Decimal),
    TerminalPaddingBalance => (Terminal, "padding_balance", false, Flag),
    TerminalPaddingColor => (Terminal, "padding_color", false, Text),
    TerminalLineHeight => (Terminal, "line_height", false, Decimal),
    TerminalCellWidth => (Terminal, "cell_width", false, Decimal),
    TerminalBaselineOffset => (Terminal, "baseline_offset", false, Number),
    TerminalUnderlineOffset => (Terminal, "underline_offset", false, Number),
    TerminalUnderlineThickness => (Terminal, "underline_thickness", false, Number),
    TerminalMinimumContrast => (Terminal, "minimum_contrast", false, Decimal),
    TerminalSmoothScrolling => (Terminal, "smooth_scrolling", false, Flag),
    TerminalScrollAnimationDuration => (Terminal, "scroll_animation_duration", false, Number),
    TerminalScrollMomentum => (Terminal, "scroll_momentum", false, Flag),
    TerminalScrollMomentumFriction => (Terminal, "scroll_momentum_friction", false, Decimal),
    TerminalBell => (Terminal, "bell", false, Text),
    TerminalOpenLinks => (Terminal, "open_links", false, Flag),
    TerminalOpenLinksWithShift => (Terminal, "open_links_with_shift", false, Flag),
    TerminalShiftDragSelects => (Terminal, "shift_drag_selects", false, Flag),
    TerminalReuseSessionWindows => (Terminal, "reuse_session_windows", false, Flag),
    TerminalMiddleClickPaste => (Terminal, "middle_click_paste", false, Flag),
    TerminalAllowRemoteClipboardRead => (Terminal, "allow_remote_clipboard_read", false, Flag),
    TerminalHidePointerWhileTyping => (Terminal, "hide_pointer_while_typing", false, Flag),
    TerminalMacosOptionAsAlt => (Terminal, "macos_option_as_alt", false, Text),
    TerminalPasteKeys => (Terminal, "paste_keys", false, List),
    TerminalCopyKeys => (Terminal, "copy_keys", false, List),
    TerminalZoomInKeys => (Terminal, "zoom_in_keys", false, List),
    TerminalZoomOutKeys => (Terminal, "zoom_out_keys", false, List),
    TerminalZoomResetKeys => (Terminal, "zoom_reset_keys", false, List),
    TerminalFullscreenKeys => (Terminal, "fullscreen_keys", false, List),
    TerminalColorForeground => (TerminalTheme, "foreground", false, Colour),
    TerminalColorBackground => (TerminalTheme, "background", false, Colour),
    TerminalColorCursor => (TerminalTheme, "cursor", false, Colour),
    TerminalColorBlack => (TerminalTheme, "black", false, Colour),
    TerminalColorRed => (TerminalTheme, "red", false, Colour),
    TerminalColorGreen => (TerminalTheme, "green", false, Colour),
    TerminalColorYellow => (TerminalTheme, "yellow", false, Colour),
    TerminalColorBlue => (TerminalTheme, "blue", false, Colour),
    TerminalColorMagenta => (TerminalTheme, "magenta", false, Colour),
    TerminalColorCyan => (TerminalTheme, "cyan", false, Colour),
    TerminalColorWhite => (TerminalTheme, "white", false, Colour),
    TerminalColorBrightBlack => (TerminalTheme, "bright_black", false, Colour),
    TerminalColorBrightRed => (TerminalTheme, "bright_red", false, Colour),
    TerminalColorBrightGreen => (TerminalTheme, "bright_green", false, Colour),
    TerminalColorBrightYellow => (TerminalTheme, "bright_yellow", false, Colour),
    TerminalColorBrightBlue => (TerminalTheme, "bright_blue", false, Colour),
    TerminalColorBrightMagenta => (TerminalTheme, "bright_magenta", false, Colour),
    TerminalColorBrightCyan => (TerminalTheme, "bright_cyan", false, Colour),
    TerminalColorBrightWhite => (TerminalTheme, "bright_white", false, Colour),
    Keybinds => (Keybinds, "keybinds", false, Text),
    Mousebinds => (Keybinds, "mousebinds", false, Text),
    PluginAliases => (Blocks, "plugins", false, Text),
    LoadPlugins => (Blocks, "load_plugins", true, Text),
    Env => (Blocks, "env", false, Text),
    Themes => (Blocks, "themes", false, Text),
    ContextMenu => (Blocks, "context_menu", false, Text),
    AttentionFrameMarkers => (Attention, "frame_markers", false, Flag),
    AttentionHostStatus => (Attention, "host_status", false, Flag),
    AttentionTabMarkers => (Attention, "tab_markers", false, Flag),
    AttentionIdleRule => (Attention, "idle_rule", false, Text),
    AttentionDoneThresholdSecs => (Attention, "done_threshold_secs", false, Number),
    AttentionIdleActivitySecs => (Attention, "idle_activity_secs", false, Number),
    AttentionIdleSilenceSecs => (Attention, "idle_silence_secs", false, Number),
    AttentionStartupCard => (Attention, "startup_card", false, Text),
    AttentionTrayIcon => (Attention, "tray_icon", false, Flag),
    AttentionNotificationDetails => (Attention, "notification_details", false, Flag),
    AttentionPopupNeedsYou => (AttentionPopups, "needs_you", false, Flag),
    AttentionPopupFailed => (AttentionPopups, "failed", false, Flag),
    AttentionPopupDone => (AttentionPopups, "done", false, Flag),
    AttentionBadgeNeedsYou => (AttentionBadge, "needs_you", false, Flag),
    AttentionBadgeFailed => (AttentionBadge, "failed", false, Flag),
    AttentionBadgeDone => (AttentionBadge, "done", false, Flag),
    AttentionBadgeWorking => (AttentionBadge, "working", false, Flag),
    AttentionBadgeQuiet => (AttentionBadge, "quiet", false, Flag),
    AttentionBadgeNewOutput => (AttentionBadge, "new_output", false, Flag),
    AttentionBadgeColorNeedsYou => (AttentionBadgeColors, "needs_you", false, Colour),
    AttentionBadgeColorFailed => (AttentionBadgeColors, "failed", false, Colour),
    AttentionBadgeColorDone => (AttentionBadgeColors, "done", false, Colour),
    AttentionBadgeColorWorking => (AttentionBadgeColors, "working", false, Colour),
    AttentionBadgeColorQuiet => (AttentionBadgeColors, "quiet", false, Colour),
    AttentionBadgeColorNewOutput => (AttentionBadgeColors, "new_output", false, Colour),
}

impl SettingKey {
    pub fn all() -> Vec<SettingKey> {
        use strum::IntoEnumIterator;
        SettingKey::iter().collect()
    }
    pub fn id(&self) -> String {
        match self.section() {
            SettingSection::TopLevel | SettingSection::Keybinds | SettingSection::Blocks => {
                self.kdl_name().to_owned()
            },
            SettingSection::PaneFrames => format!("ui.pane_frames.{}", self.kdl_name()),
            SettingSection::WebClient => format!("web_client.{}", self.kdl_name()),
            SettingSection::Terminal => format!("terminal.{}", self.kdl_name()),
            SettingSection::TerminalTheme => format!("terminal.theme.{}", self.kdl_name()),
            SettingSection::Attention => format!("attention.{}", self.kdl_name()),
            SettingSection::AttentionPopups => {
                format!("attention.desktop_notifications.{}", self.kdl_name())
            },
            SettingSection::AttentionBadge => format!("attention.tray_badge.{}", self.kdl_name()),
            SettingSection::AttentionBadgeColors => {
                format!("attention.tray_badge_colors.{}", self.kdl_name())
            },
        }
    }
    pub fn from_id(id: &str) -> Option<SettingKey> {
        SettingKey::all().into_iter().find(|key| key.id() == id)
    }
    pub fn parent_nodes(&self) -> &'static [&'static str] {
        match self.section() {
            SettingSection::TopLevel | SettingSection::Keybinds | SettingSection::Blocks => &[],
            SettingSection::PaneFrames => &["ui", "pane_frames"],
            SettingSection::WebClient => &["web_client"],
            SettingSection::Terminal => &["terminal"],
            SettingSection::TerminalTheme => &["terminal", "theme"],
            SettingSection::Attention => &["attention"],
            SettingSection::AttentionPopups => &["attention", "desktop_notifications"],
            SettingSection::AttentionBadge => &["attention", "tray_badge"],
            SettingSection::AttentionBadgeColors => &["attention", "tray_badge_colors"],
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.section(),
            SettingSection::Terminal | SettingSection::TerminalTheme
        )
    }
    pub fn is_block(&self) -> bool {
        matches!(
            self.section(),
            SettingSection::Keybinds | SettingSection::Blocks
        )
    }
    pub fn kdl_values(&self, value: &str) -> Option<Vec<kdl::KdlValue>> {
        match self.value_shape() {
            SettingValueShape::Flag => match value {
                "true" => Some(vec![kdl::KdlValue::Bool(true)]),
                "false" => Some(vec![kdl::KdlValue::Bool(false)]),
                _ => None,
            },
            SettingValueShape::Number => value
                .trim()
                .parse::<i64>()
                .ok()
                .map(|number| vec![kdl::KdlValue::Base10(number)]),
            SettingValueShape::Decimal => {
                let number = value.trim().parse::<f64>().ok()?;
                if !number.is_finite() {
                    return None;
                }
                if number.fract() == 0.0 && number.abs() < i64::MAX as f64 {
                    Some(vec![kdl::KdlValue::Base10(number as i64)])
                } else {
                    Some(vec![kdl::KdlValue::Base10Float(number)])
                }
            },
            SettingValueShape::Text => Some(vec![kdl::KdlValue::String(value.to_owned())]),
            SettingValueShape::List => {
                let items: Vec<kdl::KdlValue> = split_setting_list(value)
                    .into_iter()
                    .map(kdl::KdlValue::String)
                    .collect();
                if items.is_empty() {
                    None
                } else {
                    Some(items)
                }
            },
            SettingValueShape::Colour => {
                let text = value.trim();
                if text.starts_with('#') {
                    Some(vec![kdl::KdlValue::String(text.to_owned())])
                } else {
                    text.parse::<u8>()
                        .ok()
                        .map(|index| vec![kdl::KdlValue::Base10(index as i64)])
                }
            },
        }
    }
    pub fn kdl_snippet(&self, value: &str) -> String {
        let value_text = self
            .kdl_values(value)
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.to_string())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_else(|| kdl::KdlValue::String(value.to_owned()).to_string());
        let mut text = format!("{} {}", self.kdl_name(), value_text);
        for parent in self.parent_nodes().iter().rev() {
            let inner = text
                .lines()
                .map(|line| format!("    {}", line))
                .collect::<Vec<_>>()
                .join("\n");
            text = format!("{} {{\n{}\n}}", parent, inner);
        }
        text
    }
}

impl fmt::Display for SettingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSettingState {
    pub key: SettingKey,
    pub saved_value: Option<String>,
    pub current_value: Option<String>,
    pub set_in_file: bool,
}

impl ConfigSettingState {
    pub fn is_unsaved(&self) -> bool {
        self.saved_value != self.current_value
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PluginEntry {
    pub location: String,
    pub cwd: Option<String>,
    pub configuration: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginAliasEntry {
    pub name: String,
    pub plugin: PluginEntry,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVarEntry {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItemEntry {
    pub label: Option<String>,
    pub actions: Vec<String>,
    pub shortcut: Option<String>,
}

impl MenuItemEntry {
    pub fn separator() -> Self {
        MenuItemEntry::default()
    }
    pub fn is_separator(&self) -> bool {
        self.label.is_none()
    }
    pub fn same_item(&self, other: &MenuItemEntry) -> bool {
        self.label == other.label && self.actions == other.actions
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuSectionEntries {
    pub section: String,
    pub entries: Vec<MenuItemEntry>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeSource {
    #[default]
    BuiltIn,
    ConfigFile,
    ThemeFolder,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeEntry {
    pub name: String,
    pub source: ThemeSource,
    pub colours: Vec<String>,
    pub file_path: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigBlocks {
    pub plugin_aliases: Vec<PluginAliasEntry>,
    pub load_plugins: Vec<PluginEntry>,
    pub env: Vec<EnvVarEntry>,
    pub context_menu: Vec<MenuSectionEntries>,
    pub themes: Vec<ThemeEntry>,
}

impl ConfigBlocks {
    pub fn menu_section(&self, section: &str) -> Vec<MenuItemEntry> {
        self.context_menu
            .iter()
            .find(|entries| entries.section == section)
            .map(|entries| entries.entries.clone())
            .unwrap_or_default()
    }
    pub fn theme(&self, name: &str) -> Option<&ThemeEntry> {
        self.themes.iter().find(|theme| theme.name == name)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeybindingSource {
    #[default]
    Preset,
    User,
    Shared(String),
    Layout,
}

impl KeybindingSource {
    pub fn label(&self) -> String {
        match self {
            KeybindingSource::Preset => "preset".to_owned(),
            KeybindingSource::User => "user file".to_owned(),
            KeybindingSource::Shared(block) => format!("user file, {}", block),
            KeybindingSource::Layout => "layout".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeybindingEntry {
    pub mode: InputMode,
    pub key: KeyWithModifier,
    pub actions: Vec<String>,
    pub source: KeybindingSource,
    pub unbound: bool,
    pub preset_actions: Option<Vec<String>>,
    pub unsaved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MousebindingEntry {
    pub mode: InputMode,
    pub trigger: MouseTrigger,
    pub actions: Vec<String>,
    pub app_first: bool,
    pub source: KeybindingSource,
    pub unbound: bool,
    pub preset_actions: Option<Vec<String>>,
    pub unsaved: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigSnapshot {
    pub settings: Vec<ConfigSettingState>,
    pub config_file_path: Option<String>,
    pub backup_file_path: Option<String>,
    pub pending_restart_settings: Vec<SettingKey>,
    pub theme_names: Vec<String>,
    pub plugin_aliases: Vec<String>,
    pub load_plugins: Vec<String>,
    pub env_vars: Vec<String>,
    pub context_menu_items: Vec<String>,
    pub blocks: ConfigBlocks,
    pub saved_blocks: ConfigBlocks,
    pub default_blocks: ConfigBlocks,
    pub keybindings: Vec<KeybindingEntry>,
    pub keybinds: KeybindsSelectionSnapshot,
    pub mousebindings: Vec<MousebindingEntry>,
    pub font_families: Vec<String>,
}

impl ConfigSnapshot {
    pub fn setting(&self, key: SettingKey) -> Option<&ConfigSettingState> {
        self.settings.iter().find(|setting| setting.key == key)
    }
    pub fn unsaved_count(&self) -> usize {
        self.settings.iter().filter(|s| s.is_unsaved()).count()
    }
}

// Response type for plugin API methods that open a pane in a new tab
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenPaneInNewTabResponse {
    pub tab_id: Option<usize>,
    pub pane_id: Option<PaneId>,
}

// Response types for plugin API methods that create tabs
pub type NewTabResponse = Option<usize>;
pub type NewTabUnfocusedResponse = Option<usize>;
pub type NewTabsResponse = Vec<usize>;
pub type FocusOrCreateTabResponse = Option<usize>;
pub type BreakPanesToNewTabResponse = Option<usize>;
pub type BreakPanesToTabWithIndexResponse = Option<usize>;
pub type BreakPanesToTabWithIdResponse = Option<usize>;

// Response types for plugin API methods that create panes
pub type OpenFileResponse = Option<PaneId>;
pub type OpenFileFloatingResponse = Option<PaneId>;
pub type OpenFileInPlaceResponse = Option<PaneId>;
pub type OpenFileNearPluginResponse = Option<PaneId>;
pub type OpenFileFloatingNearPluginResponse = Option<PaneId>;
pub type OpenFileInPlaceOfPluginResponse = Option<PaneId>;

pub type OpenTerminalResponse = Option<PaneId>;
pub type OpenTerminalFloatingResponse = Option<PaneId>;
pub type OpenTerminalInPlaceResponse = Option<PaneId>;
pub type OpenTerminalNearPluginResponse = Option<PaneId>;
pub type OpenTerminalFloatingNearPluginResponse = Option<PaneId>;
pub type OpenTerminalInPlaceOfPluginResponse = Option<PaneId>;
pub type NewTiledPaneInTabResponse = Option<PaneId>;

pub type OpenCommandPaneResponse = Option<PaneId>;
pub type OpenCommandPaneFloatingResponse = Option<PaneId>;
pub type OpenCommandPaneInPlaceResponse = Option<PaneId>;
pub type OpenCommandPaneNearPluginResponse = Option<PaneId>;
pub type OpenCommandPaneFloatingNearPluginResponse = Option<PaneId>;
pub type OpenCommandPaneInPlaceOfPluginResponse = Option<PaneId>;
pub type OpenCommandPaneBackgroundResponse = Option<PaneId>;
pub type OpenCommandPaneInPlaceOfPaneIdResponse = Option<PaneId>;
pub type OpenTerminalPaneInPlaceOfPaneIdResponse = Option<PaneId>;
pub type OpenEditPaneInPlaceOfPaneIdResponse = Option<PaneId>;
pub type OpenPluginPaneFloatingResponse = Option<PaneId>;

#[test]
pub fn can_parse_unicode_bare_keys() {
    let key = "1087"; // п
    assert_eq!(
        BareKey::from_bytes_with_u(&key.as_bytes()),
        Some(BareKey::Char('п')),
        "Can parse a bare 'п' keypress"
    );
    let key = "1255"; // ӧ
    assert_eq!(
        BareKey::from_bytes_with_u(&key.as_bytes()),
        Some(BareKey::Char('ӧ')),
        "Can parse a bare 'ӧ' keypress"
    );
    let key = "1098"; // ъ
    assert_eq!(
        BareKey::from_bytes_with_u(&key.as_bytes()),
        Some(BareKey::Char('ъ')),
        "Can parse a bare 'ъ' keypress"
    );
}

#[test]
fn line_style_names_round_trip() {
    for line_style in [
        LineStyle::Single,
        LineStyle::Double,
        LineStyle::Heavy,
        LineStyle::Dashed,
        LineStyle::HeavyDashed,
    ] {
        assert_eq!(LineStyle::from_str(&line_style.to_string()), Ok(line_style));
    }
    assert!(LineStyle::from_str("squiggly").is_err());
}

#[test]
fn border_style_override_from_a_bare_cli_style() {
    assert_eq!(
        BorderStyleOverride::from_cli_string("double"),
        Ok(BorderStyleOverride {
            all: Some(LineStyle::Double),
            ..Default::default()
        })
    );
}

#[test]
fn border_style_override_from_a_compound_cli_string() {
    assert_eq!(
        BorderStyleOverride::from_cli_string("top:double, left=heavy ,rounded"),
        Ok(BorderStyleOverride {
            top: Some(LineStyle::Double),
            left: Some(LineStyle::Heavy),
            rounded_corners: Some(true),
            ..Default::default()
        })
    );
    assert_eq!(
        BorderStyleOverride::from_cli_string("all:dashed,rounded:false"),
        Ok(BorderStyleOverride {
            all: Some(LineStyle::Dashed),
            rounded_corners: Some(false),
            ..Default::default()
        })
    );
}

#[test]
fn border_style_override_rejects_unknown_cli_input() {
    assert!(BorderStyleOverride::from_cli_string("diagonal:double").is_err());
    assert!(BorderStyleOverride::from_cli_string("top:squiggly").is_err());
    assert!(BorderStyleOverride::from_cli_string("rounded:maybe").is_err());
}

#[test]
fn empty_cli_border_style_is_treated_as_no_override() {
    assert_eq!(
        BorderStyleOverride::from_optional_cli_string(Some("")),
        Ok(None)
    );
    assert_eq!(
        BorderStyleOverride::from_optional_cli_string(None),
        Ok(None)
    );
}

#[test]
fn border_style_override_applies_per_side_over_the_shorthand() {
    let base = BorderStyle::with_rounded_corners(true);
    let applied = BorderStyleOverride {
        all: Some(LineStyle::Double),
        top: Some(LineStyle::Heavy),
        ..Default::default()
    }
    .apply_to(base);
    assert_eq!(
        applied,
        BorderStyle {
            top: LineStyle::Heavy,
            right: LineStyle::Double,
            bottom: LineStyle::Double,
            left: LineStyle::Double,
            rounded_corners: true,
        }
    );
}

#[test]
fn an_empty_border_style_override_leaves_the_base_untouched() {
    let base = BorderStyle {
        top: LineStyle::Double,
        right: LineStyle::Heavy,
        bottom: LineStyle::Dashed,
        left: LineStyle::Single,
        rounded_corners: true,
    };
    assert!(BorderStyleOverride::default().is_empty());
    assert_eq!(BorderStyleOverride::default().apply_to(base), base);
}

#[test]
fn border_style_overrides_merge_with_the_later_one_winning() {
    let merged = BorderStyleOverride {
        all: Some(LineStyle::Single),
        top: Some(LineStyle::Single),
        ..Default::default()
    }
    .merge(&BorderStyleOverride {
        top: Some(LineStyle::Double),
        rounded_corners: Some(true),
        ..Default::default()
    });
    assert_eq!(
        merged,
        BorderStyleOverride {
            all: Some(LineStyle::Single),
            top: Some(LineStyle::Double),
            rounded_corners: Some(true),
            ..Default::default()
        }
    );
}

#[test]
fn a_uniform_border_style_reports_its_line_style() {
    assert_eq!(
        BorderStyle {
            top: LineStyle::Double,
            right: LineStyle::Double,
            bottom: LineStyle::Double,
            left: LineStyle::Double,
            rounded_corners: false,
        }
        .uniform_style(),
        Some(LineStyle::Double)
    );
    assert_eq!(
        BorderStyle {
            top: LineStyle::Double,
            ..Default::default()
        }
        .uniform_style(),
        None
    );
}

#[test]
fn new_pane_placement_carries_a_border_style() {
    let placement = NewPanePlacement::Tiled {
        direction: None,
        borderless: None,
        border_style: None,
    }
    .with_border_style(Some(BorderStyleOverride {
        all: Some(LineStyle::Heavy),
        ..Default::default()
    }));
    assert_eq!(
        placement.get_border_style(),
        Some(BorderStyleOverride {
            all: Some(LineStyle::Heavy),
            ..Default::default()
        })
    );
}

#[test]
fn a_floating_placement_carries_its_border_style_in_the_coordinates() {
    let placement = NewPanePlacement::Floating(None).with_border_style(Some(BorderStyleOverride {
        all: Some(LineStyle::Double),
        ..Default::default()
    }));
    let coordinates = placement.floating_pane_coordinates().unwrap();
    assert_eq!(
        coordinates.border_style,
        Some(BorderStyleOverride {
            all: Some(LineStyle::Double),
            ..Default::default()
        })
    );
    assert_eq!(placement.get_border_style(), coordinates.border_style);
}

#[cfg(test)]
mod popup_placement_tests {
    use super::*;

    #[test]
    fn corner_placements_parse_and_print_back() {
        for (text, placement, corner) in [
            (
                "top-left",
                PipePopupPlacement::TopLeft,
                PopupCorner::TopLeft,
            ),
            (
                "top-right",
                PipePopupPlacement::TopRight,
                PopupCorner::TopRight,
            ),
            (
                "bottom-left",
                PipePopupPlacement::BottomLeft,
                PopupCorner::BottomLeft,
            ),
            (
                "bottom-right",
                PipePopupPlacement::BottomRight,
                PopupCorner::BottomRight,
            ),
        ] {
            assert_eq!(text.parse::<PipePopupPlacement>(), Ok(placement));
            assert_eq!(placement.to_string(), text);
            assert_eq!(placement.corner(), Some(corner));
            assert_eq!(text.parse::<PopupCorner>(), Ok(corner));
            assert_eq!(corner.to_string(), text);
        }
        assert_eq!(
            "Top_Right".parse::<PipePopupPlacement>(),
            Ok(PipePopupPlacement::TopRight)
        );
        assert_eq!(PipePopupPlacement::Pane.corner(), None);
        assert!("middle".parse::<PipePopupPlacement>().is_err());
        assert!("center".parse::<PopupCorner>().is_err());
    }

    #[test]
    fn popup_options_default_to_a_focused_popup_without_a_corner() {
        assert_eq!(
            PopupOptions::default(),
            PopupOptions {
                focused: true,
                corner: None,
                capture_all_keys: false,
                outside_click: None,
            }
        );
    }

    #[test]
    fn popup_options_builders_set_each_option() {
        assert_eq!(
            PopupOptions::default()
                .focused(false)
                .corner(PopupCorner::BottomLeft)
                .capture_all_keys(true)
                .outside_click(OutsideClick::Close),
            PopupOptions {
                focused: false,
                corner: Some(PopupCorner::BottomLeft),
                capture_all_keys: true,
                outside_click: Some(OutsideClick::Close),
            }
        );
    }

    #[test]
    fn outside_click_parses_its_own_display_form() {
        for outside_click in [OutsideClick::Block, OutsideClick::Close] {
            assert_eq!(outside_click.to_string().parse(), Ok(outside_click));
        }
        assert_eq!(" Close ".parse::<OutsideClick>(), Ok(OutsideClick::Close));
        assert!("unfocus".parse::<OutsideClick>().is_err());
    }
}

#[cfg(test)]
mod extract_text_cluster_tests {
    use super::*;

    #[test]
    fn column_extraction_keeps_combining_marks_with_their_base() {
        let line = "ae\u{0301}b\u{1F468}\u{200D}\u{1F469}c";
        assert_eq!(extract_text_by_columns(line, 1, 2), "e\u{0301}");
        assert_eq!(
            extract_text_from_column(line, 2),
            "b\u{1F468}\u{200D}\u{1F469}c"
        );
        assert_eq!(extract_text_to_column(line, 2), "ae\u{0301}");
        assert_eq!(extract_text_by_columns(line, 3, 5), "\u{1F468}\u{200D}");
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod kitty_serialization_tests {
    use super::*;

    fn kitty(key: &str) -> Option<String> {
        KeyWithModifier::from_str(key).unwrap().serialize_kitty()
    }

    #[test]
    fn text_keys_stay_text() {
        assert_eq!(kitty("a").as_deref(), Some("a"));
        assert_eq!(kitty("A").as_deref(), Some("A"));
        assert_eq!(kitty("Shift a").as_deref(), Some("A"));
        assert_eq!(kitty("Space").as_deref(), Some(" "));
        assert_eq!(
            KeyWithModifier::new(BareKey::Char('é'))
                .serialize_kitty()
                .as_deref(),
            Some("é")
        );
    }

    #[test]
    fn modified_characters_use_csi_u_with_the_unshifted_codepoint() {
        assert_eq!(kitty("Ctrl a").as_deref(), Some("\u{1b}[97;5u"));
        assert_eq!(kitty("Alt a").as_deref(), Some("\u{1b}[97;3u"));
        assert_eq!(kitty("Super a").as_deref(), Some("\u{1b}[97;9u"));
        assert_eq!(kitty("Ctrl Alt Shift a").as_deref(), Some("\u{1b}[97;8u"));
        assert_eq!(kitty("Ctrl Shift v").as_deref(), Some("\u{1b}[118;6u"));
        assert_eq!(kitty("Ctrl V").as_deref(), Some("\u{1b}[118;6u"));
        assert_eq!(kitty("Ctrl v").as_deref(), Some("\u{1b}[118;5u"));
        assert_eq!(kitty("Ctrl Space").as_deref(), Some("\u{1b}[32;5u"));
        assert_eq!(kitty("Alt !").as_deref(), Some("\u{1b}[33;3u"));
    }

    #[test]
    fn enter_tab_and_backspace_are_legacy_only_when_unmodified() {
        assert_eq!(kitty("Enter").as_deref(), Some("\r"));
        assert_eq!(kitty("Tab").as_deref(), Some("\t"));
        assert_eq!(kitty("Backspace").as_deref(), Some("\u{7f}"));
        assert_eq!(kitty("Shift Enter").as_deref(), Some("\u{1b}[13;2u"));
        assert_eq!(kitty("Shift Tab").as_deref(), Some("\u{1b}[9;2u"));
        assert_eq!(kitty("Ctrl Backspace").as_deref(), Some("\u{1b}[127;5u"));
    }

    #[test]
    fn escape_is_always_disambiguated() {
        assert_eq!(kitty("Esc").as_deref(), Some("\u{1b}[27u"));
        assert_eq!(kitty("Alt Esc").as_deref(), Some("\u{1b}[27;3u"));
    }

    #[test]
    fn functional_keys_use_the_kitty_forms() {
        assert_eq!(kitty("Up").as_deref(), Some("\u{1b}[A"));
        assert_eq!(kitty("Ctrl Left").as_deref(), Some("\u{1b}[1;5D"));
        assert_eq!(kitty("Home").as_deref(), Some("\u{1b}[H"));
        assert_eq!(kitty("Shift End").as_deref(), Some("\u{1b}[1;2F"));
        assert_eq!(kitty("F1").as_deref(), Some("\u{1b}OP"));
        assert_eq!(kitty("Alt F2").as_deref(), Some("\u{1b}[1;3Q"));
        assert_eq!(kitty("F3").as_deref(), Some("\u{1b}[13~"));
        assert_eq!(kitty("Ctrl F3").as_deref(), Some("\u{1b}[13;5~"));
        assert_eq!(kitty("F5").as_deref(), Some("\u{1b}[15~"));
        assert_eq!(kitty("Shift F12").as_deref(), Some("\u{1b}[24;2~"));
        assert_eq!(kitty("Insert").as_deref(), Some("\u{1b}[2~"));
        assert_eq!(kitty("Shift Insert").as_deref(), Some("\u{1b}[2;2~"));
        assert_eq!(kitty("Delete").as_deref(), Some("\u{1b}[3~"));
        assert_eq!(kitty("PageUp").as_deref(), Some("\u{1b}[5~"));
        assert_eq!(kitty("Ctrl PageDown").as_deref(), Some("\u{1b}[6;5~"));
    }

    #[test]
    fn lock_keys_are_not_reported() {
        assert_eq!(
            KeyWithModifier::new(BareKey::CapsLock).serialize_kitty(),
            None
        );
        assert_eq!(
            KeyWithModifier::new(BareKey::NumLock).serialize_kitty(),
            None
        );
    }

    #[test]
    fn csi_u_encodings_decode_back_to_the_same_key() {
        for key in [
            "Ctrl a",
            "Ctrl Shift v",
            "Alt Esc",
            "Shift Enter",
            "Super Tab",
        ] {
            let encoded = kitty(key).unwrap();
            let body = encoded
                .strip_prefix("\u{1b}[")
                .and_then(|rest| rest.strip_suffix('u'))
                .unwrap();
            let (number, modifiers) = body.split_once(';').unwrap();
            let parsed =
                KeyWithModifier::from_bytes_with_u(number.as_bytes(), modifiers.as_bytes());
            assert_eq!(
                parsed,
                Some(KeyWithModifier::from_str(key).unwrap()),
                "{key}"
            );
        }
    }
}

#[cfg(test)]
mod live_view_release_gesture_tests {
    use super::*;

    #[test]
    fn a_key_written_twice_is_a_double_press_and_a_single_key_is_one_press() {
        assert_eq!(
            LiveViewReleaseGesture::parse("Esc Esc"),
            Some(LiveViewReleaseGesture {
                key: KeyWithModifier::new(BareKey::Esc),
                double: true,
            })
        );
        assert_eq!(
            LiveViewReleaseGesture::parse("  Ctrl q   Ctrl q "),
            Some(LiveViewReleaseGesture {
                key: KeyWithModifier::new(BareKey::Char('q')).with_ctrl_modifier(),
                double: true,
            })
        );
        assert_eq!(
            LiveViewReleaseGesture::parse("Ctrl Alt Home"),
            Some(LiveViewReleaseGesture {
                key: KeyWithModifier::new(BareKey::Home)
                    .with_ctrl_modifier()
                    .with_alt_modifier(),
                double: false,
            })
        );
        assert_eq!(
            LiveViewReleaseGesture::parse("Esc"),
            Some(LiveViewReleaseGesture {
                key: KeyWithModifier::new(BareKey::Esc),
                double: false,
            })
        );
        assert_eq!(LiveViewReleaseGesture::parse(""), None);
        assert_eq!(LiveViewReleaseGesture::parse("Esc Tab"), None);
        assert_eq!(LiveViewReleaseGesture::parse("not a key"), None);
        assert_eq!(
            LiveViewReleaseGesture::parse(DEFAULT_LIVE_VIEW_RELEASE_KEY),
            Some(LiveViewReleaseGesture {
                key: KeyWithModifier::new(BareKey::F(9)),
                double: false,
            })
        );
    }

    #[test]
    fn the_gesture_is_shown_as_a_key_hint() {
        assert_eq!(LiveViewReleaseGesture::display("Esc Esc"), "<ESC> twice");
        assert_eq!(
            LiveViewReleaseGesture::display(DEFAULT_LIVE_VIEW_RELEASE_KEY),
            "<F9>"
        );
        assert_eq!(
            LiveViewReleaseGesture::display("Ctrl Alt Home"),
            "<Ctrl Alt HOME>"
        );
        assert_eq!(LiveViewReleaseGesture::display(" odd text "), "<odd text>");
    }
}

#[cfg(test)]
mod hidden_bars_tests {
    use super::*;

    #[test]
    fn no_names_means_nothing_is_hidden_and_all_means_everything() {
        assert_eq!(HiddenBars::from_names::<&str>(&[]), HiddenBars::None);
        assert_eq!(HiddenBars::from_names(&["all"]), HiddenBars::All);
        assert_eq!(HiddenBars::all_unless_named::<&str>(&[]), HiddenBars::All);
    }

    #[test]
    fn named_bars_hide_only_themselves() {
        let hidden_bars = HiddenBars::from_names(&[TAB_BAR]);
        assert!(hidden_bars.hides(TAB_BAR));
        assert!(!hidden_bars.hides(STATUS_BAR));
        assert!(hidden_bars.hides_any(&["zellij:bars", TAB_BAR]));
        assert!(HiddenBars::All.hides(STATUS_BAR));
        assert!(!HiddenBars::None.hides(STATUS_BAR));
    }

    #[test]
    fn hiding_more_bars_adds_to_what_is_hidden() {
        let hidden_bars =
            HiddenBars::from_names(&[TAB_BAR]).union(&HiddenBars::from_names(&[STATUS_BAR]));
        assert_eq!(hidden_bars, HiddenBars::from_names(&[TAB_BAR, STATUS_BAR]));
        assert_eq!(hidden_bars.union(&HiddenBars::All), HiddenBars::All);
        assert_eq!(HiddenBars::None.union(&hidden_bars), hidden_bars);
    }

    #[test]
    fn hidden_bars_parse_from_a_comma_separated_list() {
        assert_eq!(
            "tab-bar,status-bar".parse::<HiddenBars>().unwrap(),
            HiddenBars::from_names(&[TAB_BAR, STATUS_BAR])
        );
        assert_eq!("none".parse::<HiddenBars>().unwrap(), HiddenBars::None);
        assert_eq!("all".parse::<HiddenBars>().unwrap(), HiddenBars::All);
        assert_eq!(
            HiddenBars::from_names(&[TAB_BAR, STATUS_BAR])
                .names()
                .join(",")
                .parse::<HiddenBars>()
                .unwrap(),
            HiddenBars::from_names(&[TAB_BAR, STATUS_BAR])
        );
    }

    #[test]
    fn a_built_in_bar_is_named_by_its_role_and_a_plugin_by_its_location() {
        use crate::input::layout::{Run, RunPluginOrAlias};
        let bar = |url: &str, role: Option<&str>| {
            let configuration =
                role.map(|role| BTreeMap::from([("role".to_owned(), role.to_owned())]));
            Run::Plugin(RunPluginOrAlias::from_url(url, &configuration, None, None).unwrap())
        };
        assert!(bar("zellij:bars", Some(STATUS_BAR))
            .bar_names()
            .contains(&STATUS_BAR.to_owned()));
        assert!(bar("zellij:bars", None)
            .bar_names()
            .contains(&TAB_BAR.to_owned()));
        assert!(bar("zellij:status-bar", None)
            .bar_names()
            .contains(&STATUS_BAR.to_owned()));
        assert_eq!(
            bar("file:/tmp/my-bar.wasm", None).bar_names(),
            vec!["file:/tmp/my-bar.wasm".to_owned()]
        );
    }

    #[test]
    fn only_the_status_and_compact_bars_show_the_mode() {
        assert!(bar_shows_mode(STATUS_BAR));
        assert!(bar_shows_mode(COMPACT_BAR));
        assert!(!bar_shows_mode(TAB_BAR));
        assert_eq!(bar_role_label(STATUS_BAR), Some("status bar"));
        assert_eq!(bar_role_label("my-bar"), None);
    }
}
