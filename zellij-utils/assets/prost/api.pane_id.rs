#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneId {
    #[prost(enumeration = "PaneType", tag = "1")]
    pub pane_type: i32,
    #[prost(uint32, tag = "2")]
    pub id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PaneType {
    Terminal = 0,
    Plugin = 1,
}
impl PaneType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            PaneType::Terminal => "Terminal",
            PaneType::Plugin => "Plugin",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Terminal" => Some(Self::Terminal),
            "Plugin" => Some(Self::Plugin),
            _ => None,
        }
    }
}
