use crate::consts::WEBSERVER_SOCKET_PATH;
use crate::errors::prelude::*;
use crate::web_server_contract::web_server_contract::InstructionForWebServer as ProtoInstructionForWebServer;
use interprocess::local_socket::LocalSocketStream;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Write};
use std::os::unix::fs::FileTypeExt;

pub fn shutdown_all_webserver_instances() -> Result<()> {
    let entries = fs::read_dir(&*WEBSERVER_SOCKET_PATH)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if let Some(file_name) = path.file_name() {
            if let Some(_file_name_str) = file_name.to_str() {
                let metadata = entry.metadata()?;
                let file_type = metadata.file_type();

                if file_type.is_socket() {
                    match create_webserver_sender(path.to_str().unwrap_or("")) {
                        Ok(mut sender) => {
                            let _ = send_webserver_instruction(
                                &mut sender,
                                InstructionForWebServer::ShutdownWebServer,
                            );
                        },
                        Err(_) => {
                            // no-op
                        },
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InstructionForWebServer {
    ShutdownWebServer,
}

pub fn create_webserver_sender(path: &str) -> Result<BufWriter<LocalSocketStream>> {
    let stream = LocalSocketStream::connect(path)?;
    Ok(BufWriter::new(stream))
}

pub fn send_webserver_instruction(
    sender: &mut BufWriter<LocalSocketStream>,
    instruction: InstructionForWebServer,
) -> Result<()> {
    // Convert to protobuf and send with length prefix
    let proto_instruction: ProtoInstructionForWebServer = instruction.into();
    let encoded = proto_instruction.encode_to_vec();
    let len = encoded.len() as u32;

    // Write length prefix
    sender.write_all(&len.to_le_bytes())?;
    // Write protobuf message
    sender.write_all(&encoded)?;
    sender.flush()?;
    Ok(())
}
