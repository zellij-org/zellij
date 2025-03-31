#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct File {
    #[prost(string, tag="1")]
    pub path: ::prost::alloc::string::String,
    #[prost(int32, optional, tag="2")]
    pub line_number: ::core::option::Option<i32>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
