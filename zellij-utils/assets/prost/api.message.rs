#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub payload: ::prost::alloc::string::String,
    #[prost(string, optional, tag="3")]
    pub worker_name: ::core::option::Option<::prost::alloc::string::String>,
}
