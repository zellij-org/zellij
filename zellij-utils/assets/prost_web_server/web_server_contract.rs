#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InstructionForWebServer {
    #[prost(oneof="instruction_for_web_server::Instruction", tags="1")]
    pub instruction: ::core::option::Option<instruction_for_web_server::Instruction>,
}
/// Nested message and enum types in `InstructionForWebServer`.
pub mod instruction_for_web_server {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Instruction {
        /// Future commands can be added here
        /// RestartWebServerMsg restart_web_server = 2;
        /// ReloadConfigMsg reload_config = 3;
        #[prost(message, tag="1")]
        ShutdownWebServer(super::ShutdownWebServerMsg),
    }
}
/// Empty for now, but allows for future parameters like graceful timeout
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ShutdownWebServerMsg {
}
