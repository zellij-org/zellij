#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InstructionForWebServer {
    #[prost(oneof="instruction_for_web_server::Instruction", tags="1, 2")]
    pub instruction: ::core::option::Option<instruction_for_web_server::Instruction>,
}
/// Nested message and enum types in `InstructionForWebServer`.
pub mod instruction_for_web_server {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Instruction {
        #[prost(message, tag="1")]
        ShutdownWebServer(super::ShutdownWebServerMsg),
        /// Future commands can be added here
        /// RestartWebServerMsg restart_web_server = 3;
        /// ReloadConfigMsg reload_config = 4;
        #[prost(message, tag="2")]
        QueryVersion(super::QueryVersionMsg),
    }
}
/// Empty for now, but allows for future parameters like graceful timeout
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ShutdownWebServerMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryVersionMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WebServerResponse {
    #[prost(oneof="web_server_response::Response", tags="1")]
    pub response: ::core::option::Option<web_server_response::Response>,
}
/// Nested message and enum types in `WebServerResponse`.
pub mod web_server_response {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Response {
        #[prost(message, tag="1")]
        Version(super::VersionResponseMsg),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct VersionResponseMsg {
    #[prost(string, tag="1")]
    pub version: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub ip: ::prost::alloc::string::String,
    #[prost(uint32, tag="3")]
    pub port: u32,
}
