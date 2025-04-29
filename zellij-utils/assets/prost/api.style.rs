#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Style {
    #[deprecated]
    #[prost(message, optional, tag="1")]
    pub palette: ::core::option::Option<Palette>,
    #[prost(bool, tag="2")]
    pub rounded_corners: bool,
    #[prost(bool, tag="3")]
    pub hide_session_name: bool,
    #[prost(message, optional, tag="4")]
    pub styling: ::core::option::Option<Styling>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Palette {
    #[prost(enumeration="ThemeHue", tag="1")]
    pub theme_hue: i32,
    #[prost(message, optional, tag="2")]
    pub fg: ::core::option::Option<Color>,
    #[prost(message, optional, tag="3")]
    pub bg: ::core::option::Option<Color>,
    #[prost(message, optional, tag="4")]
    pub black: ::core::option::Option<Color>,
    #[prost(message, optional, tag="5")]
    pub red: ::core::option::Option<Color>,
    #[prost(message, optional, tag="6")]
    pub green: ::core::option::Option<Color>,
    #[prost(message, optional, tag="7")]
    pub yellow: ::core::option::Option<Color>,
    #[prost(message, optional, tag="8")]
    pub blue: ::core::option::Option<Color>,
    #[prost(message, optional, tag="9")]
    pub magenta: ::core::option::Option<Color>,
    #[prost(message, optional, tag="10")]
    pub cyan: ::core::option::Option<Color>,
    #[prost(message, optional, tag="11")]
    pub white: ::core::option::Option<Color>,
    #[prost(message, optional, tag="12")]
    pub orange: ::core::option::Option<Color>,
    #[prost(message, optional, tag="13")]
    pub gray: ::core::option::Option<Color>,
    #[prost(message, optional, tag="14")]
    pub purple: ::core::option::Option<Color>,
    #[prost(message, optional, tag="15")]
    pub gold: ::core::option::Option<Color>,
    #[prost(message, optional, tag="16")]
    pub silver: ::core::option::Option<Color>,
    #[prost(message, optional, tag="17")]
    pub pink: ::core::option::Option<Color>,
    #[prost(message, optional, tag="18")]
    pub brown: ::core::option::Option<Color>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Color {
    #[prost(enumeration="ColorType", tag="1")]
    pub color_type: i32,
    #[prost(oneof="color::Payload", tags="2, 3")]
    pub payload: ::core::option::Option<color::Payload>,
}
/// Nested message and enum types in `Color`.
pub mod color {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag="2")]
        RgbColorPayload(super::RgbColorPayload),
        #[prost(uint32, tag="3")]
        EightBitColorPayload(u32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RgbColorPayload {
    #[prost(uint32, tag="1")]
    pub red: u32,
    #[prost(uint32, tag="2")]
    pub green: u32,
    #[prost(uint32, tag="3")]
    pub blue: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Styling {
    #[prost(message, repeated, tag="1")]
    pub text_unselected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="2")]
    pub text_selected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="3")]
    pub ribbon_unselected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="4")]
    pub ribbon_selected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="5")]
    pub table_title: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="6")]
    pub table_cell_unselected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="7")]
    pub table_cell_selected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="8")]
    pub list_unselected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="9")]
    pub list_selected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="10")]
    pub frame_unselected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="11")]
    pub frame_selected: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="12")]
    pub frame_highlight: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="13")]
    pub exit_code_success: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="14")]
    pub exit_code_error: ::prost::alloc::vec::Vec<Color>,
    #[prost(message, repeated, tag="15")]
    pub multiplayer_user_colors: ::prost::alloc::vec::Vec<Color>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ColorType {
    Rgb = 0,
    EightBit = 1,
}
impl ColorType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ColorType::Rgb => "Rgb",
            ColorType::EightBit => "EightBit",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Rgb" => Some(Self::Rgb),
            "EightBit" => Some(Self::EightBit),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ThemeHue {
    Dark = 0,
    Light = 1,
}
impl ThemeHue {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ThemeHue::Dark => "Dark",
            ThemeHue::Light => "Light",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Dark" => Some(Self::Dark),
            "Light" => Some(Self::Light),
            _ => None,
        }
    }
}
