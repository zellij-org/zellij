use axum_server::Handle;
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use zellij_utils::consts::WEBSERVER_SOCKET_PATH;
use zellij_utils::prost::Message;
use zellij_utils::web_server_commands::InstructionForWebServer;
use zellij_utils::web_server_contract::web_server_contract::InstructionForWebServer as ProtoInstructionForWebServer;

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

pub async fn listen_to_web_server_instructions(server_handle: Handle, id: &str) {
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
                    },
                    Err(e) => {
                        log::error!("Failed to process web server instruction: {}", e);
                        // Continue loop to recreate receiver and try again
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
