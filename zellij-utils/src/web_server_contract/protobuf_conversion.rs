use crate::errors::prelude::*;
use crate::web_server_commands::InstructionForWebServer as RustInstructionForWebServer;
use crate::web_server_contract::web_server_contract::{
    instruction_for_web_server, InstructionForWebServer as ProtoInstructionForWebServer,
    ShutdownWebServerMsg,
};

// Convert Rust InstructionForWebServer to protobuf
impl From<RustInstructionForWebServer> for ProtoInstructionForWebServer {
    fn from(instruction: RustInstructionForWebServer) -> Self {
        let instruction = match instruction {
            RustInstructionForWebServer::ShutdownWebServer => {
                instruction_for_web_server::Instruction::ShutdownWebServer(ShutdownWebServerMsg {})
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
            None => Err(anyhow!("Missing instruction in InstructionForWebServer")),
        }
    }
}
