use crate::consts::is_ipc_socket;
use crate::consts::WEBSERVER_SOCKET_PATH;
use crate::errors::prelude::*;
use crate::web_server_contract::web_server_contract::InstructionForWebServer as ProtoInstructionForWebServer;
use crate::web_server_contract::web_server_contract::WebServerResponse as ProtoWebServerResponse;
use interprocess::local_socket::{prelude::*, GenericFilePath, Stream as LocalSocketStream};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufWriter, Read, Write};
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

                if is_ipc_socket(&file_type) {
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
    let fs_name = path.to_fs_name::<GenericFilePath>()?;
    let stream = LocalSocketStream::connect(fs_name)?;
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

        if is_ipc_socket(&entry.metadata()?.file_type()) {
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

#[cfg(test)]
mod web_server_query_tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    // The web server's ipc_listener protocol: a length-prefixed protobuf
    // instruction in, a length-prefixed protobuf response out. This is the
    // transport `zellij web --status` and the session server's status poll
    // use since the isahc HTTP poll was removed.
    #[test]
    fn query_version_round_trip_over_ipc() {
        let dir = std::env::temp_dir().join(format!(
            "zellij-web-query-test-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let socket_path = dir.join("ipctest");
        let _ = std::fs::remove_file(&socket_path);
        let listener = UnixListener::bind(&socket_path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut len_bytes = [0u8; 4];
            stream.read_exact(&mut len_bytes).unwrap();
            let len = u32::from_le_bytes(len_bytes) as usize;
            let mut buf = vec![0u8; len];
            stream.read_exact(&mut buf).unwrap();
            let proto_instruction = ProtoInstructionForWebServer::decode(&buf[..]).unwrap();
            let instruction: InstructionForWebServer = proto_instruction.try_into().unwrap();
            assert!(matches!(
                instruction,
                InstructionForWebServer::QueryVersion
            ));
            let response: ProtoWebServerResponse = WebServerResponse::Version(VersionInfo {
                version: "0.43.1".to_owned(),
                ip: "0.0.0.0".to_owned(),
                port: 8082,
            })
            .into();
            let encoded = response.encode_to_vec();
            stream
                .write_all(&(encoded.len() as u32).to_le_bytes())
                .unwrap();
            stream.write_all(&encoded).unwrap();
        });
        let response = query_webserver_with_response(
            socket_path.to_str().unwrap(),
            InstructionForWebServer::QueryVersion,
            2000,
        )
        .unwrap();
        server.join().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        match response {
            WebServerResponse::Version(info) => {
                assert_eq!(info.version, "0.43.1");
                assert_eq!(info.ip, "0.0.0.0");
                assert_eq!(info.port, 8082);
            },
        }
    }
}
