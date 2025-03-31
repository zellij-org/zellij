#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Resize {
    #[prost(enumeration="ResizeAction", tag="1")]
    pub resize_action: i32,
    #[prost(enumeration="ResizeDirection", optional, tag="2")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveDirection {
    #[prost(enumeration="ResizeDirection", tag="1")]
    pub direction: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ResizeAction {
    Increase = 0,
    Decrease = 1,
}
impl ResizeAction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ResizeAction::Increase => "Increase",
            ResizeAction::Decrease => "Decrease",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Increase" => Some(Self::Increase),
            "Decrease" => Some(Self::Decrease),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ResizeDirection {
    Left = 0,
    Right = 1,
    Up = 2,
    Down = 3,
}
impl ResizeDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ResizeDirection::Left => "Left",
            ResizeDirection::Right => "Right",
            ResizeDirection::Up => "Up",
            ResizeDirection::Down => "Down",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Left" => Some(Self::Left),
            "Right" => Some(Self::Right),
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            _ => None,
        }
    }
}
