// NOTE: This file is generated automatically, do *NOT* edit it by hand!
// Refer to [the PR introducing this change][1] to learn more about the reasons.
//
// [1]: https://github.com/zellij-org/zellij/pull/2711#issuecomment-1695015818
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Message {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub payload: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "3")]
    pub worker_name: ::core::option::Option<::prost::alloc::string::String>,
}
