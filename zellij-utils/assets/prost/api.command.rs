#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Command {
    #[prost(string, tag="1")]
    pub path: ::prost::alloc::string::String,
    #[prost(string, repeated, tag="2")]
    pub args: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
