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
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum MainKey {
        #[prost(enumeration="NamedKey", tag="2")]
        Key(i32),
        #[prost(uint32, tag="3")]
        Char(u32),
    }
}
