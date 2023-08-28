// NOTE: This file is generated automatically, do *NOT* edit it by hand!
// Refer to [the PR introducing this change][1] to learn more about the reasons.
//
// [1]: https://github.com/zellij-org/zellij/pull/2711#issuecomment-1695015818
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginIds {
    #[prost(int32, tag = "1")]
    pub plugin_id: i32,
    #[prost(int32, tag = "2")]
    pub zellij_pid: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ZellijVersion {
    #[prost(string, tag = "1")]
    pub version: ::prost::alloc::string::String,
}
