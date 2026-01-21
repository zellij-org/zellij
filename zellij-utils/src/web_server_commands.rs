use crate::consts::WEBSERVER_SOCKET_PATH;
use crate::errors::prelude::*;
use crate::web_server_contract::web_server_contract::InstructionForWebServer as ProtoInstructionForWebServer;
use crate::web_server_contract::web_server_contract::WebServerResponse as ProtoWebServerResponse;
use interprocess::local_socket::LocalSocketStream;
use prost::Message;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::FileTypeExt;
use std::path::PathBuf;

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
    QueryVersion,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub ip: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WebServerResponse {
    Version(VersionInfo),
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

pub fn discover_webserver_sockets() -> Result<Vec<PathBuf>> {
    let mut sockets = Vec::new();

    if !WEBSERVER_SOCKET_PATH.exists() {
        return Ok(sockets);
    }

    for entry in fs::read_dir(&*WEBSERVER_SOCKET_PATH)? {
        let entry = entry?;
        let path = entry.path();

        if entry.metadata()?.file_type().is_socket() {
            sockets.push(path);
        }
    }

    Ok(sockets)
}

pub fn query_webserver_with_response(
    path: &str,
    instruction: InstructionForWebServer,
    _timeout_ms: u64,
) -> Result<WebServerResponse> {
    let mut sender = create_webserver_sender(path)?;
    send_webserver_instruction(&mut sender, instruction)?;

    let stream = sender.into_inner()?;
    receive_webserver_response(stream)
}

fn receive_webserver_response(mut stream: LocalSocketStream) -> Result<WebServerResponse> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer)?;

    let proto_response = ProtoWebServerResponse::decode(&buffer[..])?;
    proto_response.try_into()
}
