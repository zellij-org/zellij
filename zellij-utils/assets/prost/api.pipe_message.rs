#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PipeMessage {
    #[prost(enumeration="PipeSource", tag="1")]
    pub source: i32,
    #[prost(string, optional, tag="2")]
    pub cli_source_id: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="3")]
    pub plugin_source_id: ::core::option::Option<u32>,
    #[prost(string, tag="4")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, optional, tag="5")]
    pub payload: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="6")]
    pub args: ::prost::alloc::vec::Vec<Arg>,
    #[prost(bool, tag="7")]
    pub is_private: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Arg {
    #[prost(string, tag="1")]
    pub key: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub value: ::prost::alloc::string::String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PipeSource {
    Cli = 0,
    Plugin = 1,
    Keybind = 2,
}
impl PipeSource {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            PipeSource::Cli => "Cli",
            PipeSource::Plugin => "Plugin",
            PipeSource::Keybind => "Keybind",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Cli" => Some(Self::Cli),
            "Plugin" => Some(Self::Plugin),
            "Keybind" => Some(Self::Keybind),
            _ => None,
        }
    }
}
