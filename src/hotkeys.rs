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

/// Enum defining the available set of base input keys that we handle
/// 
/// Per the questions above, we probably need to support multiple keyboard
/// layouts. I'm sticking with my (UK qwerty) keyboard for now, but do add your own!
/// 
/// @@@khs26 Not sure you can actually pass all these through - try it out!
#[derive(Debug, PartialEq, Eq, Hash)]
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

impl std::string::ToString for BaseInputKey {
    fn to_string(&self) -> String {
        match self {
            BaseInputKey::Esc => String::from("Esc"),
            BaseInputKey::F1 => String::from("F1"),
            BaseInputKey::F2 => String::from("F2"),
            BaseInputKey::F3 => String::from("F3"),
            BaseInputKey::F4 => String::from("F4"),
            BaseInputKey::F5 => String::from("F5"),
            BaseInputKey::F6 => String::from("F6"),
            BaseInputKey::F7 => String::from("F7"),
            BaseInputKey::F8 => String::from("F8"),
            BaseInputKey::F9 => String::from("F9"),
            BaseInputKey::F10 => String::from("F10"),
            BaseInputKey::F11 => String::from("F11"),
            BaseInputKey::F12 => String::from("F12"),
            BaseInputKey::Grave => String::from("`"),
            BaseInputKey::Main1 => String::from("1"),
            BaseInputKey::Main2 => String::from("2"),
            BaseInputKey::Main3 => String::from("3"),
            BaseInputKey::Main4 => String::from("4"),
            BaseInputKey::Main5 => String::from("5"),
            BaseInputKey::Main6 => String::from("6"),
            BaseInputKey::Main7 => String::from("7"),
            BaseInputKey::Main8 => String::from("8"),
            BaseInputKey::Main9 => String::from("9"),
            BaseInputKey::Main0 => String::from("0"),
            BaseInputKey::Hyphen => String::from("-"),
            BaseInputKey::Equals => String::from("="),
            BaseInputKey::Backspace => String::from("Backspace"),
            BaseInputKey::Tab => String::from("Tab"),
            BaseInputKey::Q => String::from("Q"),
            BaseInputKey::W => String::from("W"),
            BaseInputKey::E => String::from("E"),
            BaseInputKey::R => String::from("R"),
            BaseInputKey::T => String::from("T"),
            BaseInputKey::Y => String::from("Y"),
            BaseInputKey::U => String::from("U"),
            BaseInputKey::I => String::from("I"),
            BaseInputKey::O => String::from("O"),
            BaseInputKey::P => String::from("P"),
            BaseInputKey::LeftSquareBracket => String::from("["),
            BaseInputKey::RightSquareBracket => String::from("]"),
            BaseInputKey::CapsLock => String::from("CapsLock"),
            BaseInputKey::A => String::from("A"),
            BaseInputKey::S => String::from("S"),
            BaseInputKey::D => String::from("D"),
            BaseInputKey::F => String::from("F"),
            BaseInputKey::G => String::from("G"),
            BaseInputKey::H => String::from("H"),
            BaseInputKey::J => String::from("J"),
            BaseInputKey::K => String::from("K"),
            BaseInputKey::L => String::from("L"),
            BaseInputKey::Semicolon => String::from(";"),
            BaseInputKey::Apostrophe => String::from("'"),
            BaseInputKey::Hash => String::from("#"), 
            BaseInputKey::Return => String::from("Return"),
            BaseInputKey::Backslash => String::from("\\"),
            BaseInputKey::Z => String::from("Z"),
            BaseInputKey::X => String::from("X"),
            BaseInputKey::C => String::from("C"),
            BaseInputKey::V => String::from("V"),
            BaseInputKey::B => String::from("B"),
            BaseInputKey::N => String::from("N"),
            BaseInputKey::M => String::from("M"),
            BaseInputKey::Comma => String::from(","),
            BaseInputKey::Period => String::from("."),
            BaseInputKey::ForwardSlash => String::from("/"),
            BaseInputKey::WindowsKey => String::from("Windows"),
            BaseInputKey::Space => String::from("Space"),
            BaseInputKey::PrintScreen => String::from("PrintScreen"),
            BaseInputKey::ScrollLock => String::from("ScrollLock"),
            BaseInputKey::Pause => String::from("Pause"),
            BaseInputKey::Insert => String::from("Insert"),
            BaseInputKey::Home => String::from("Home"),
            BaseInputKey::PageUp => String::from("PageUp"),
            BaseInputKey::Delete => String::from("Delete"),
            BaseInputKey::End => String::from("End"),
            BaseInputKey::PageDown => String::from("PageDown"),
            //@@@khs26 Replace these with arrows?
            BaseInputKey::Up => String::from("Up"),
            BaseInputKey::Down => String::from("Down"),
            BaseInputKey::Left => String::from("Left"),
            BaseInputKey::Right => String::from("Right"),
            BaseInputKey::NumpadSlash => String::from("Numpad/"),
            BaseInputKey::NumpadAsterisk => String::from("Numpad*"),
            BaseInputKey::NumpadHyphen => String::from("Numpad-"),
            BaseInputKey::NumpadPlus => String::from("Numpad+"),
            BaseInputKey::Numpad1 => String::from("Numpad1"),
            BaseInputKey::Numpad2 => String::from("Numpad2"),
            BaseInputKey::Numpad3 => String::from("Numpad3"),
            BaseInputKey::Numpad4 => String::from("Numpad4"),
            BaseInputKey::Numpad5 => String::from("Numpad5"),
            BaseInputKey::Numpad6 => String::from("Numpad6"),
            BaseInputKey::Numpad7 => String::from("Numpad7"),
            BaseInputKey::Numpad8 => String::from("Numpad8"),
            BaseInputKey::Numpad9 => String::from("Numpad9"),
            BaseInputKey::Numpad0 => String::from("Numpad0"),
            BaseInputKey::NumpadPeriod => String::from("Numpad."),
            BaseInputKey::NumpadEnter => String::from("NumpadEnter"),
        }
    }
}

/// Modifier keys that can be applied to input keys
#[derive(Debug, PartialEq, Eq, Hash)]
pub enum ModifierKey {
    Control,
    Alt,
    Shift,
    AltGr,
    // @@@khs26 are these actually different
    RightShift,
    RightControl,
}

impl std::string::ToString for ModifierKey {
    fn to_string(&self) -> String {
        match self {
            ModifierKey::Control => String::from("Ctrl"),
            ModifierKey::Alt => String::from("Alt"),
            ModifierKey::Shift => String::from("Shift"),
            ModifierKey::AltGr => String::from("AltGr"),
            ModifierKey::RightShift => String::from("RShift"),
            ModifierKey::RightControl => String::from("RCtrl"),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct InputKey {
    /// The character sequence sent when this key is pressed
    base_key: BaseInputKey,
    modifiers: HashSet<ModifierKey>,
    char_sequence: Vec<u8>,
    user_string: String,
}

impl InputKey {
    pub fn new(base_key: BaseInputKey, modifiers: HashSet<ModifierKey>, char_sequence: Vec<u8>) -> InputKey {
        todo!()
    }
}

impl fmt::Display for InputKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_string)
    }
}
