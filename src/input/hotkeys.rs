/// Provides handling for customisable hotkeys
///
/// There are a few purposes to this module:
///   - Define what character sequences are available to be mapped
///   - Provide a user representation of what those sequences are (i.e. key combinations)
///   - For each mode, provide a map between the key sequences and a mosaic action (including pass-through)
///   - Provide functions for getting and setting (remapping) hotkey definitions
///   - Render the current keymap and provide an interface to change the keys
///
/// Open questions:
///   - Do we want to have different base inputs depending on locale (i.e. keyboard map)?
///   - Should the user view be a plugin?
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

use lazy_static::lazy_static;

use nom;
use serde::Serialize;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

/// Enum defining the available set of base input keys that we handle
///
/// Per the questions above, we probably need to support multiple keyboard
/// layouts. I'm sticking with my (UK qwerty) keyboard for now, but do add your own!
///
/// @@@khs26 Not sure you can actually pass all these through - try it out!
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, EnumIter, Serialize)]
pub enum BaseInputKey {
    Esc,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Grave,
    /// Main[0-9] represent keys on the top of the keyboard (as opposed to the numpad)
    Main1,
    Main2,
    Main3,
    Main4,
    Main5,
    Main6,
    Main7,
    Main8,
    Main9,
    Main0,
    /// -
    Hyphen,
    /// =
    Equals,
    Backspace,
    /// \t
    Tab,
    Q,
    W,
    E,
    R,
    T,
    Y,
    U,
    I,
    O,
    P,
    /// [
    LeftSquareBracket,
    /// ]
    RightSquareBracket,
    CapsLock,
    A,
    S,
    D,
    F,
    G,
    H,
    J,
    K,
    L,
    /// ;
    Semicolon,
    /// '
    Apostrophe,
    /// #   - you can call it pound if you really want to :)
    Hash,
    Return,
    /// \
    Backslash,
    Z,
    X,
    C,
    V,
    B,
    N,
    M,
    /// ,
    Comma,
    /// .
    Period,
    /// /
    ForwardSlash,
    WindowsKey,
    Space,
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    /// Numpad keys - not clear these do anything different from their
    /// counterparts in the rest of the keyboard
    NumpadSlash,
    NumpadAsterisk,
    NumpadHyphen,
    NumpadPlus,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    Numpad0,
    NumpadPeriod,
    NumpadEnter,
}

static BASE_INPUT_KEY_STRINGS: [(BaseInputKey, &str); 96] = [
    (BaseInputKey::Esc, "Esc"),
    (BaseInputKey::F1, "F1"),
    (BaseInputKey::F2, "F2"),
    (BaseInputKey::F3, "F3"),
    (BaseInputKey::F4, "F4"),
    (BaseInputKey::F5, "F5"),
    (BaseInputKey::F6, "F6"),
    (BaseInputKey::F7, "F7"),
    (BaseInputKey::F8, "F8"),
    (BaseInputKey::F9, "F9"),
    (BaseInputKey::F10, "F10"),
    (BaseInputKey::F11, "F11"),
    (BaseInputKey::F12, "F12"),
    (BaseInputKey::Grave, "`"),
    (BaseInputKey::Main1, "1"),
    (BaseInputKey::Main2, "2"),
    (BaseInputKey::Main3, "3"),
    (BaseInputKey::Main4, "4"),
    (BaseInputKey::Main5, "5"),
    (BaseInputKey::Main6, "6"),
    (BaseInputKey::Main7, "7"),
    (BaseInputKey::Main8, "8"),
    (BaseInputKey::Main9, "9"),
    (BaseInputKey::Main0, "0"),
    (BaseInputKey::Hyphen, "-"),
    (BaseInputKey::Equals, "="),
    (BaseInputKey::Backspace, "Backspace"),
    (BaseInputKey::Tab, "Tab"),
    (BaseInputKey::Q, "Q"),
    (BaseInputKey::W, "W"),
    (BaseInputKey::E, "E"),
    (BaseInputKey::R, "R"),
    (BaseInputKey::T, "T"),
    (BaseInputKey::Y, "Y"),
    (BaseInputKey::U, "U"),
    (BaseInputKey::I, "I"),
    (BaseInputKey::O, "O"),
    (BaseInputKey::P, "P"),
    (BaseInputKey::LeftSquareBracket, "["),
    (BaseInputKey::RightSquareBracket, "]"),
    (BaseInputKey::CapsLock, "CapsLock"),
    (BaseInputKey::A, "A"),
    (BaseInputKey::S, "S"),
    (BaseInputKey::D, "D"),
    (BaseInputKey::F, "F"),
    (BaseInputKey::G, "G"),
    (BaseInputKey::H, "H"),
    (BaseInputKey::J, "J"),
    (BaseInputKey::K, "K"),
    (BaseInputKey::L, "L"),
    (BaseInputKey::Semicolon, ";"),
    (BaseInputKey::Apostrophe, "'"),
    (BaseInputKey::Hash, "#"),
    (BaseInputKey::Return, "Return"),
    (BaseInputKey::Backslash, "\\"),
    (BaseInputKey::Z, "Z"),
    (BaseInputKey::X, "X"),
    (BaseInputKey::C, "C"),
    (BaseInputKey::V, "V"),
    (BaseInputKey::B, "B"),
    (BaseInputKey::N, "N"),
    (BaseInputKey::M, "M"),
    (BaseInputKey::Comma, ","),
    (BaseInputKey::Period, "."),
    (BaseInputKey::ForwardSlash, "/"),
    (BaseInputKey::WindowsKey, "Windows"),
    (BaseInputKey::Space, "Space"),
    (BaseInputKey::PrintScreen, "PrintScreen"),
    (BaseInputKey::ScrollLock, "ScrollLock"),
    (BaseInputKey::Pause, "Pause"),
    (BaseInputKey::Insert, "Insert"),
    (BaseInputKey::Home, "Home"),
    (BaseInputKey::PageUp, "PageUp"),
    (BaseInputKey::Delete, "Delete"),
    (BaseInputKey::End, "End"),
    (BaseInputKey::PageDown, "PageDown"),
    //@@@khs26 Replace these with arrows?
    (BaseInputKey::Up, "Up"),
    (BaseInputKey::Down, "Down"),
    (BaseInputKey::Left, "Left"),
    (BaseInputKey::Right, "Right"),
    (BaseInputKey::NumpadSlash, "Numpad/"),
    (BaseInputKey::NumpadAsterisk, "Numpad*"),
    (BaseInputKey::NumpadHyphen, "Numpad-"),
    (BaseInputKey::NumpadPlus, "Numpad+"),
    (BaseInputKey::Numpad1, "Numpad1"),
    (BaseInputKey::Numpad2, "Numpad2"),
    (BaseInputKey::Numpad3, "Numpad3"),
    (BaseInputKey::Numpad4, "Numpad4"),
    (BaseInputKey::Numpad5, "Numpad5"),
    (BaseInputKey::Numpad6, "Numpad6"),
    (BaseInputKey::Numpad7, "Numpad7"),
    (BaseInputKey::Numpad8, "Numpad8"),
    (BaseInputKey::Numpad9, "Numpad9"),
    (BaseInputKey::Numpad0, "Numpad0"),
    (BaseInputKey::NumpadPeriod, "Numpad."),
    (BaseInputKey::NumpadEnter, "NumpadEnter"),
];

lazy_static! {
    static ref BASE_INPUT_KEY_TO_STRING: HashMap<BaseInputKey, &'static str> = {
        let mut map = HashMap::new();
        for (key, key_string) in BASE_INPUT_KEY_STRINGS.iter() {
            map.insert(*key, *key_string);
        }
        map
    };
    static ref STRING_TO_BASE_INPUT_KEY: HashMap<&'static str, BaseInputKey> = {
        let mut map = HashMap::new();
        for (key, key_string) in BASE_INPUT_KEY_STRINGS.iter() {
            map.insert(*key_string, *key);
        }
        map
    };
}

impl std::string::ToString for BaseInputKey {
    fn to_string(&self) -> String {
        String::from(*BASE_INPUT_KEY_TO_STRING.get(self).unwrap())
    }
}

/// Modifier keys that can be applied to input keys
///
/// N.B. The EnumIter trait means that the order of the enum members will affect
/// the order in which the modifiers are displayed (i.e. Ctrl+Alt+<key>, not Alt+Ctrl+<key>)
#[derive(Copy, Debug, PartialEq, Eq, Hash, Clone, EnumIter, Serialize)]
pub enum ModifierKey {
    Control,
    Alt,
    Shift,
    AltGr,
    // @@@khs26 are these actually different?
    RightShift,
    RightControl,
}

static MODIFIER_KEY_STRINGS: [(ModifierKey, &str); 6] = [
    (ModifierKey::Control, "Ctrl"),
    (ModifierKey::Alt, "Alt"),
    (ModifierKey::Shift, "Shift"),
    (ModifierKey::AltGr, "AltGr"),
    (ModifierKey::RightShift, "RShift"),
    (ModifierKey::RightControl, "RCtrl"),
];

lazy_static! {
    static ref MODIFIER_KEY_TO_STRING: HashMap<ModifierKey, &'static str> = {
        let mut map = HashMap::new();
        for (key, key_string) in MODIFIER_KEY_STRINGS.iter() {
            map.insert(*key, *key_string);
        }
        map
    };
    static ref STRING_TO_MODIFIER_KEY: HashMap<&'static str, ModifierKey> = {
        let mut map = HashMap::new();
        for (key, key_string) in MODIFIER_KEY_STRINGS.iter() {
            map.insert(*key_string, *key);
        }
        map
    };
}

impl std::string::ToString for ModifierKey {
    fn to_string(&self) -> String {
        String::from(*MODIFIER_KEY_TO_STRING.get(self).unwrap())
    }
}

/// Represents a particular key combination that can be input by a user
#[derive(Debug, Serialize)]
pub struct InputKey {
    /// Base (keyboard) key
    base_key: BaseInputKey,
    /// Modifier keys
    modifiers: HashSet<ModifierKey>,
    /// How to display this key to the user
    user_string: String,
    // The character sequence sent when this key is pressed
    //@@@khs26 Actually, let's put this in a terminal map struct
    //char_sequence: Vec<u8>,
}

impl InputKey {
    pub fn new(base_key: BaseInputKey, modifiers: HashSet<ModifierKey>) -> InputKey {
        let mut user_string_list = vec![];

        // Iterating through ModifierKey rather than through modifiers ensures we
        // get a canonical ordering
        for modifier in ModifierKey::iter() {
            if modifiers.contains(&modifier) {
                user_string_list.push(modifier.to_string());
            }
        }

        user_string_list.push(base_key.to_string());

        InputKey {
            base_key,
            modifiers,
            user_string: user_string_list.join("+"),
        }
    }
}

impl std::str::FromStr for InputKey {
    //@@@khs26 What about errors?
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Ok(parsed) = nom::sequence::pair(nom::multi::many0(parse_modifier), parse_base_input)(s) {
            let new_key = InputKey::new(
                parsed.1.1,
                parsed.1.0.iter().cloned().collect());
            Ok(new_key)
        } else {
            Err(())
        }
    }
}

fn parse_modifier(s: &str) -> nom::IResult<&str, ModifierKey> {
    let (s, parsed) = nom::sequence::terminated(
        nom::character::complete::alpha1,
        nom::bytes::complete::tag("+"),
    )(s)?;
    Ok((s, *STRING_TO_MODIFIER_KEY.get(parsed).unwrap()))
}

fn parse_base_input(s: &str) -> nom::IResult<&str, BaseInputKey> {
    let (s, parsed) = nom::character::complete::alpha1(s)?;
    Ok((s, *STRING_TO_BASE_INPUT_KEY.get(parsed).unwrap()))
}

impl fmt::Display for InputKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_string)
    }
}

/// If the base key and modifiers are equal, we consider the keys equal
impl PartialEq for InputKey {
    fn eq(&self, other: &Self) -> bool {
        let mut equal = self.base_key == other.base_key;
        for modifier in &self.modifiers {
            equal |= other.modifiers.contains(modifier);
        }

        equal
    }
}

impl Eq for InputKey {}

impl std::hash::Hash for InputKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.base_key.hash(state);
        for modifier in self.modifiers.iter() {
            modifier.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_key_as_string() {
        let test_key = InputKey::new(BaseInputKey::A, [].iter().cloned().collect());
        assert_eq!("A", test_key.user_string);

        let test_key = InputKey::new(
            BaseInputKey::G,
            [ModifierKey::Control, ModifierKey::Shift]
                .iter()
                .cloned()
                .collect(),
        );
        assert_eq!("Ctrl+Shift+G", test_key.user_string);

        let test_key = InputKey::new(
            BaseInputKey::B,
            [ModifierKey::Control, ModifierKey::Alt, ModifierKey::Shift]
                .iter()
                .cloned()
                .collect(),
        );
        assert_eq!("Ctrl+Alt+Shift+B", test_key.user_string);
    }

    #[test]
    fn input_key_from_string() {
        assert_eq!(("Alt+A", ModifierKey::Control), parse_modifier("Ctrl+Alt+A").unwrap());
        assert_eq!(("A", ModifierKey::Alt), parse_modifier("Alt+A").unwrap());
        assert_eq!(("", BaseInputKey::A), parse_base_input("A").unwrap());

        assert_eq!(("Alt+Esc", ModifierKey::Control), parse_modifier("Ctrl+Alt+Esc").unwrap());
        assert_eq!(("Esc", ModifierKey::Alt), parse_modifier("Alt+Esc").unwrap());
        assert_eq!(("", BaseInputKey::Esc), parse_base_input("Esc").unwrap());

        let test_key = InputKey::new(
            BaseInputKey::B,
            [ModifierKey::Control, ModifierKey::Alt, ModifierKey::Shift]
                .iter()
                .cloned()
                .collect(),
        );
        assert_eq!(test_key, InputKey::from_str("Ctrl+Alt+Shift+B").unwrap());

        let test_key = InputKey::new(
            BaseInputKey::LeftSquareBracket,
            HashSet::new()
        );
        assert_eq!(test_key, InputKey::from_str("[").unwrap());
    }
}
