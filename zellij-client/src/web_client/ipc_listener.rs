use axum_server::Handle;
use std::net::IpAddr;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use zellij_utils::consts::WEBSERVER_SOCKET_PATH;
use zellij_utils::prost::Message;
use zellij_utils::web_server_commands::{InstructionForWebServer, VersionInfo, WebServerResponse};
use zellij_utils::web_server_contract::web_server_contract::InstructionForWebServer as ProtoInstructionForWebServer;
use zellij_utils::web_server_contract::web_server_contract::WebServerResponse as ProtoWebServerResponse;

pub async fn create_webserver_receiver(
    id: &str,
) -> Result<UnixStream, Box<dyn std::error::Error + Send + Sync>> {
    std::fs::create_dir_all(&WEBSERVER_SOCKET_PATH.as_path())?;
    let socket_path = WEBSERVER_SOCKET_PATH.join(format!("{}", id));

    if socket_path.exists() {
        tokio::fs::remove_file(&socket_path).await?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    let (stream, _) = listener.accept().await?;
    Ok(stream)
}

pub async fn receive_webserver_instruction(
    receiver: &mut UnixStream,
) -> std::io::Result<InstructionForWebServer> {
    // Read length prefix (4 bytes)
    let mut len_bytes = [0u8; 4];
    receiver.read_exact(&mut len_bytes).await?;
    let len = u32::from_le_bytes(len_bytes) as usize;

    // Read protobuf message
    let mut buffer = vec![0u8; len];
    receiver.read_exact(&mut buffer).await?;

    // Decode protobuf message
    let proto_instruction = ProtoInstructionForWebServer::decode(&buffer[..])
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    // Convert to Rust type
    proto_instruction
        .try_into()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

pub async fn send_webserver_response(
    sender: &mut UnixStream,
    response: WebServerResponse,
) -> std::io::Result<()> {
    let proto_response: ProtoWebServerResponse = response.into();
    let encoded = proto_response.encode_to_vec();
    let len = encoded.len() as u32;

    sender.write_all(&len.to_le_bytes()).await?;
    sender.write_all(&encoded).await?;
    sender.flush().await?;

    Ok(())
}

pub async fn listen_to_web_server_instructions(
    server_handle: Handle,
    id: &str,
    web_server_ip: IpAddr,
    web_server_port: u16,
) {
    loop {
        let receiver = create_webserver_receiver(id).await;
        match receiver {
            Ok(mut receiver) => {
                match receive_webserver_instruction(&mut receiver).await {
                    Ok(instruction) => match instruction {
                        InstructionForWebServer::ShutdownWebServer => {
                            server_handle.shutdown();
                            break;
                        },
                        InstructionForWebServer::QueryVersion => {
                            let response = WebServerResponse::Version(VersionInfo {
                                version: zellij_utils::consts::VERSION.to_string(),
                                ip: web_server_ip.to_string(),
                                port: web_server_port,
                            });
                            let _ = send_webserver_response(&mut receiver, response).await;
                        },
                    },
                    Err(e) => {
                        log::error!("Failed to process web server instruction: {}", e);
                    },
                }
            },
            Err(e) => {
                log::error!("Failed to listen to ipc channel: {}", e);
                break;
            },
        }
    }
}
