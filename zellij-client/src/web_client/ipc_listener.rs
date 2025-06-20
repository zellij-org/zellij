use super::control_message::{SetConfigPayload, WebServerToWebClientControlMessage};
use super::types::ConnectionTable;
use axum_server::Handle;
use std::sync::{Arc, Mutex};
use tokio::io::AsyncReadExt;
use tokio::net::{UnixListener, UnixStream};
use zellij_utils::consts::WEBSERVER_SOCKET_PATH;
use zellij_utils::web_server_commands::InstructionForWebServer;

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
    let mut buffer = Vec::new();
    receiver.read_to_end(&mut buffer).await?;
    let cursor = std::io::Cursor::new(buffer);
    rmp_serde::decode::from_read(cursor)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

pub async fn listen_to_web_server_instructions(
    server_handle: Handle,
    connection_table: Arc<Mutex<ConnectionTable>>,
    id: &str,
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
                        InstructionForWebServer::ConfigWrittenToDisk(new_config) => {
                            let set_config_payload = SetConfigPayload::from(&new_config);

                            let client_ids: Vec<String> = {
                                let connection_table_lock = connection_table.lock().unwrap();
                                connection_table_lock
                                    .client_id_to_channels
                                    .keys()
                                    .cloned()
                                    .collect()
                            };

                            let config_message =
                                WebServerToWebClientControlMessage::SetConfig(set_config_payload);
                            let config_msg_json = match serde_json::to_string(&config_message) {
                                Ok(json) => json,
                                Err(e) => {
                                    log::error!("Failed to serialize config message: {}", e);
                                    continue;
                                },
                            };

                            for client_id in client_ids {
                                if let Some(control_tx) = connection_table
                                    .lock()
                                    .unwrap()
                                    .get_client_control_tx(&client_id)
                                {
                                    let ws_message = config_msg_json.clone();
                                    match control_tx.send(ws_message.into()) {
                                        Ok(_) => {}, // no-op
                                        Err(e) => {
                                            log::error!(
                                                "Failed to send config update to client {}: {}",
                                                client_id,
                                                e
                                            );
                                        },
                                    }
                                }
                            }
                            // Continue loop to recreate receiver for next message
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
