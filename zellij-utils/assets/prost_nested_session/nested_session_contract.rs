#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NestedSessionMessage {
    #[prost(oneof="nested_session_message::Payload", tags="1, 2, 3, 4, 5, 8, 9, 10, 11, 12, 13")]
    pub payload: ::core::option::Option<nested_session_message::Payload>,
}
/// Nested message and enum types in `NestedSessionMessage`.
pub mod nested_session_message {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag="1")]
        Announce(super::Announce),
        #[prost(message, tag="2")]
        FocusHost(super::FocusHost),
        #[prost(message, tag="3")]
        HostFullscreen(super::ToggleHostFullscreen),
        #[prost(message, tag="4")]
        Pong(super::Pong),
        #[prost(message, tag="5")]
        Bye(super::Bye),
        #[prost(message, tag="8")]
        AnnounceAck(super::AnnounceAck),
        #[prost(message, tag="9")]
        FocusGained(super::FocusGained),
        #[prost(message, tag="10")]
        FocusLost(super::FocusLost),
        #[prost(message, tag="11")]
        FullscreenState(super::FullscreenState),
        #[prost(message, tag="12")]
        AncestryUpdate(super::AncestryUpdate),
        #[prost(message, tag="13")]
        Ping(super::Ping),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Announce {
    #[prost(string, tag="1")]
    pub session_name: ::prost::alloc::string::String,
    #[prost(enumeration="NestedCapability", repeated, tag="2")]
    pub capabilities: ::prost::alloc::vec::Vec<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusHost {
    #[prost(enumeration="NestedDirection", tag="1")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleHostFullscreen {
    #[prost(bool, tag="1")]
    pub fullscreen: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Pong {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Bye {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AnnounceAck {
    #[prost(string, repeated, tag="1")]
    pub ancestry: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(enumeration="NestedCapability", repeated, tag="2")]
    pub capabilities: ::prost::alloc::vec::Vec<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusGained {
    #[prost(enumeration="NestedDirection", tag="1")]
    pub from_direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusLost {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FullscreenState {
    #[prost(bool, tag="1")]
    pub fullscreen: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AncestryUpdate {
    #[prost(string, repeated, tag="1")]
    pub ancestry: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Ping {
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum NestedCapability {
    Unspecified = 0,
    NestedControl = 1,
}
impl NestedCapability {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            NestedCapability::Unspecified => "NESTED_CAPABILITY_UNSPECIFIED",
            NestedCapability::NestedControl => "NESTED_CAPABILITY_NESTED_CONTROL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "NESTED_CAPABILITY_UNSPECIFIED" => Some(Self::Unspecified),
            "NESTED_CAPABILITY_NESTED_CONTROL" => Some(Self::NestedControl),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum NestedDirection {
    Unspecified = 0,
    Left = 1,
    Right = 2,
    Up = 3,
    Down = 4,
}
impl NestedDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            NestedDirection::Unspecified => "NESTED_DIRECTION_UNSPECIFIED",
            NestedDirection::Left => "NESTED_DIRECTION_LEFT",
            NestedDirection::Right => "NESTED_DIRECTION_RIGHT",
            NestedDirection::Up => "NESTED_DIRECTION_UP",
            NestedDirection::Down => "NESTED_DIRECTION_DOWN",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "NESTED_DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "NESTED_DIRECTION_LEFT" => Some(Self::Left),
            "NESTED_DIRECTION_RIGHT" => Some(Self::Right),
            "NESTED_DIRECTION_UP" => Some(Self::Up),
            "NESTED_DIRECTION_DOWN" => Some(Self::Down),
            _ => None,
        }
    }
}
