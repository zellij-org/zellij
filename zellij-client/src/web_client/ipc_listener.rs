use axum_server::Handle;
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use uuid::Uuid;
use zellij_utils::consts::WEBSERVER_SOCKET_PATH;
use zellij_utils::ipc::InstructionForWebServer;

async fn create_webserver_receiver() -> Result<UnixStream, Box<dyn std::error::Error + Send + Sync>>
{
    let id = Uuid::new_v4();
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
    let mut buffer = Vec::new();
    receiver.read_to_end(&mut buffer).await?;
    let cursor = std::io::Cursor::new(buffer);
    rmp_serde::decode::from_read(cursor)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

// here we listen to internal web server instructions coming over an IPC channel
pub async fn listen_to_web_server_instructions(server_handle: Handle) {
    loop {
        match create_webserver_receiver().await {
            Ok(mut receiver) => loop {
                match receive_webserver_instruction(&mut receiver).await {
                    Ok(instruction) => match instruction {
                        InstructionForWebServer::ShutdownWebServer => {
                            server_handle.shutdown();
                            break;
                        },
                    },
                    Err(e) => {
                        log::error!("Failed to process web server instruction: {}", e);
                        break;
                    },
                }
            },
            Err(e) => {
                log::error!("Failed to create receiver: {}", e);
            },
        }
    }
}
