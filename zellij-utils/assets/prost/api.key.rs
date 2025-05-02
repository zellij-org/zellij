#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Key {
    #[prost(enumeration="key::KeyModifier", optional, tag="1")]
    pub modifier: ::core::option::Option<i32>,
    #[prost(enumeration="key::KeyModifier", repeated, tag="4")]
    pub additional_modifiers: ::prost::alloc::vec::Vec<i32>,
    #[prost(oneof="key::MainKey", tags="2, 3")]
    pub main_key: ::core::option::Option<key::MainKey>,
}
/// Nested message and enum types in `Key`.
pub mod key {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum KeyModifier {
        Ctrl = 0,
        Alt = 1,
        Shift = 2,
        Super = 3,
    }
    impl KeyModifier {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                KeyModifier::Ctrl => "CTRL",
                KeyModifier::Alt => "ALT",
                KeyModifier::Shift => "SHIFT",
                KeyModifier::Super => "SUPER",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "CTRL" => Some(Self::Ctrl),
                "ALT" => Some(Self::Alt),
                "SHIFT" => Some(Self::Shift),
                "SUPER" => Some(Self::Super),
                _ => None,
            }
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum NamedKey {
        PageDown = 0,
        PageUp = 1,
        LeftArrow = 2,
        DownArrow = 3,
        UpArrow = 4,
        RightArrow = 5,
        Home = 6,
        End = 7,
        Backspace = 8,
        Delete = 9,
        Insert = 10,
        F1 = 11,
        F2 = 12,
        F3 = 13,
        F4 = 14,
        F5 = 15,
        F6 = 16,
        F7 = 17,
        F8 = 18,
        F9 = 19,
        F10 = 20,
        F11 = 21,
        F12 = 22,
        Tab = 23,
        Esc = 24,
        CapsLock = 25,
        ScrollLock = 26,
        NumLock = 27,
        PrintScreen = 28,
        Pause = 29,
        Menu = 30,
        Enter = 31,
    }
    impl NamedKey {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                NamedKey::PageDown => "PageDown",
                NamedKey::PageUp => "PageUp",
                NamedKey::LeftArrow => "LeftArrow",
                NamedKey::DownArrow => "DownArrow",
                NamedKey::UpArrow => "UpArrow",
                NamedKey::RightArrow => "RightArrow",
                NamedKey::Home => "Home",
                NamedKey::End => "End",
                NamedKey::Backspace => "Backspace",
                NamedKey::Delete => "Delete",
                NamedKey::Insert => "Insert",
                NamedKey::F1 => "F1",
                NamedKey::F2 => "F2",
                NamedKey::F3 => "F3",
                NamedKey::F4 => "F4",
                NamedKey::F5 => "F5",
                NamedKey::F6 => "F6",
                NamedKey::F7 => "F7",
                NamedKey::F8 => "F8",
                NamedKey::F9 => "F9",
                NamedKey::F10 => "F10",
                NamedKey::F11 => "F11",
                NamedKey::F12 => "F12",
                NamedKey::Tab => "Tab",
                NamedKey::Esc => "Esc",
                NamedKey::CapsLock => "CapsLock",
                NamedKey::ScrollLock => "ScrollLock",
                NamedKey::NumLock => "NumLock",
                NamedKey::PrintScreen => "PrintScreen",
                NamedKey::Pause => "Pause",
                NamedKey::Menu => "Menu",
                NamedKey::Enter => "Enter",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "PageDown" => Some(Self::PageDown),
                "PageUp" => Some(Self::PageUp),
                "LeftArrow" => Some(Self::LeftArrow),
                "DownArrow" => Some(Self::DownArrow),
                "UpArrow" => Some(Self::UpArrow),
                "RightArrow" => Some(Self::RightArrow),
                "Home" => Some(Self::Home),
                "End" => Some(Self::End),
                "Backspace" => Some(Self::Backspace),
                "Delete" => Some(Self::Delete),
                "Insert" => Some(Self::Insert),
                "F1" => Some(Self::F1),
                "F2" => Some(Self::F2),
                "F3" => Some(Self::F3),
                "F4" => Some(Self::F4),
                "F5" => Some(Self::F5),
                "F6" => Some(Self::F6),
                "F7" => Some(Self::F7),
                "F8" => Some(Self::F8),
                "F9" => Some(Self::F9),
                "F10" => Some(Self::F10),
                "F11" => Some(Self::F11),
                "F12" => Some(Self::F12),
                "Tab" => Some(Self::Tab),
                "Esc" => Some(Self::Esc),
                "CapsLock" => Some(Self::CapsLock),
                "ScrollLock" => Some(Self::ScrollLock),
                "NumLock" => Some(Self::NumLock),
                "PrintScreen" => Some(Self::PrintScreen),
                "Pause" => Some(Self::Pause),
                "Menu" => Some(Self::Menu),
                "Enter" => Some(Self::Enter),
                _ => None,
            }
        }
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
    #[repr(i32)]
    pub enum Char {
        A = 0,
        B = 1,
        C = 2,
        D = 3,
        E = 4,
        F = 5,
        G = 6,
        H = 7,
        I = 8,
        J = 9,
        K = 10,
        L = 11,
        M = 12,
        N = 13,
        O = 14,
        P = 15,
        Q = 16,
        R = 17,
        S = 18,
        T = 19,
        U = 20,
        V = 21,
        W = 22,
        X = 23,
        Y = 24,
        Z = 25,
        Zero = 26,
        One = 27,
        Two = 28,
        Three = 29,
        Four = 30,
        Five = 31,
        Six = 32,
        Seven = 33,
        Eight = 34,
        Nine = 35,
    }
    impl Char {
        /// String value of the enum field names used in the ProtoBuf definition.
        ///
        /// The values are not transformed in any way and thus are considered stable
        /// (if the ProtoBuf definition does not change) and safe for programmatic use.
        pub fn as_str_name(&self) -> &'static str {
            match self {
                Char::A => "a",
                Char::B => "b",
                Char::C => "c",
                Char::D => "d",
                Char::E => "e",
                Char::F => "f",
                Char::G => "g",
                Char::H => "h",
                Char::I => "i",
                Char::J => "j",
                Char::K => "k",
                Char::L => "l",
                Char::M => "m",
                Char::N => "n",
                Char::O => "o",
                Char::P => "p",
                Char::Q => "q",
                Char::R => "r",
                Char::S => "s",
                Char::T => "t",
                Char::U => "u",
                Char::V => "v",
                Char::W => "w",
                Char::X => "x",
                Char::Y => "y",
                Char::Z => "z",
                Char::Zero => "zero",
                Char::One => "one",
                Char::Two => "two",
                Char::Three => "three",
                Char::Four => "four",
                Char::Five => "five",
                Char::Six => "six",
                Char::Seven => "seven",
                Char::Eight => "eight",
                Char::Nine => "nine",
            }
        }
        /// Creates an enum from field names used in the ProtoBuf definition.
        pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
            match value {
                "a" => Some(Self::A),
                "b" => Some(Self::B),
                "c" => Some(Self::C),
                "d" => Some(Self::D),
                "e" => Some(Self::E),
                "f" => Some(Self::F),
                "g" => Some(Self::G),
                "h" => Some(Self::H),
                "i" => Some(Self::I),
                "j" => Some(Self::J),
                "k" => Some(Self::K),
                "l" => Some(Self::L),
                "m" => Some(Self::M),
                "n" => Some(Self::N),
                "o" => Some(Self::O),
                "p" => Some(Self::P),
                "q" => Some(Self::Q),
                "r" => Some(Self::R),
                "s" => Some(Self::S),
                "t" => Some(Self::T),
                "u" => Some(Self::U),
                "v" => Some(Self::V),
                "w" => Some(Self::W),
                "x" => Some(Self::X),
                "y" => Some(Self::Y),
                "z" => Some(Self::Z),
                "zero" => Some(Self::Zero),
                "one" => Some(Self::One),
                "two" => Some(Self::Two),
                "three" => Some(Self::Three),
                "four" => Some(Self::Four),
                "five" => Some(Self::Five),
                "six" => Some(Self::Six),
                "seven" => Some(Self::Seven),
                "eight" => Some(Self::Eight),
                "nine" => Some(Self::Nine),
                _ => None,
            }
        }
    }
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum MainKey {
        #[prost(enumeration="NamedKey", tag="2")]
        Key(i32),
        #[prost(enumeration="Char", tag="3")]
        Char(i32),
    }
}
