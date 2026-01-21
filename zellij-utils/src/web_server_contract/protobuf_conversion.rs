use crate::errors::prelude::*;
use crate::web_server_commands::{
    InstructionForWebServer as RustInstructionForWebServer, VersionInfo, WebServerResponse,
};
use crate::web_server_contract::web_server_contract::{
    instruction_for_web_server, web_server_response, InstructionForWebServer as ProtoInstructionForWebServer,
    QueryVersionMsg, ShutdownWebServerMsg, VersionResponseMsg,
    WebServerResponse as ProtoWebServerResponse,
};

// Convert Rust InstructionForWebServer to protobuf
impl From<RustInstructionForWebServer> for ProtoInstructionForWebServer {
    fn from(instruction: RustInstructionForWebServer) -> Self {
        let instruction = match instruction {
            RustInstructionForWebServer::ShutdownWebServer => {
                instruction_for_web_server::Instruction::ShutdownWebServer(ShutdownWebServerMsg {})
            },
            RustInstructionForWebServer::QueryVersion => {
                instruction_for_web_server::Instruction::QueryVersion(QueryVersionMsg {})
            },
        };

        ProtoInstructionForWebServer {
            instruction: Some(instruction),
        }
    }
}

// Convert protobuf InstructionForWebServer to Rust
impl TryFrom<ProtoInstructionForWebServer> for RustInstructionForWebServer {
    type Error = anyhow::Error;

    fn try_from(proto_instruction: ProtoInstructionForWebServer) -> Result<Self> {
        match proto_instruction.instruction {
            Some(instruction_for_web_server::Instruction::ShutdownWebServer(_)) => {
                Ok(RustInstructionForWebServer::ShutdownWebServer)
            },
            Some(instruction_for_web_server::Instruction::QueryVersion(_)) => {
                Ok(RustInstructionForWebServer::QueryVersion)
            },
            None => Err(anyhow!("Missing instruction in InstructionForWebServer")),
        }
    }
}

// Convert Rust WebServerResponse to protobuf
impl From<WebServerResponse> for ProtoWebServerResponse {
    fn from(response: WebServerResponse) -> Self {
        let response = match response {
            WebServerResponse::Version(version_info) => {
                web_server_response::Response::Version(VersionResponseMsg {
                    version: version_info.version,
                    ip: version_info.ip,
                    port: version_info.port as u32,
                })
            },
        };

        ProtoWebServerResponse {
            response: Some(response),
        }
    }
}

// Convert protobuf WebServerResponse to Rust
impl TryFrom<ProtoWebServerResponse> for WebServerResponse {
    type Error = anyhow::Error;

    fn try_from(proto_response: ProtoWebServerResponse) -> Result<Self> {
        match proto_response.response {
            Some(web_server_response::Response::Version(version_msg)) => {
                Ok(WebServerResponse::Version(VersionInfo {
                    version: version_msg.version,
                    ip: version_msg.ip,
                    port: version_msg.port as u16,
                }))
            },
            None => Err(anyhow!("Missing response in WebServerResponse")),
        }
    }
}
