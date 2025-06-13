use crate::web_client::control_message::{
    SetConfigPayload, WebClientToWebServerControlMessage,
    WebClientToWebServerControlMessagePayload, WebServerToWebClientControlMessage,
};
use crate::web_client::message_handlers::{
    parse_stdin, render_to_client, send_control_messages_to_client,
};
use crate::web_client::server_listener::zellij_server_listener;
use crate::web_client::types::{AppState, TerminalParams};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    response::IntoResponse,
};
use futures::StreamExt;
use log::info;
use zellij_utils::{input::mouse::MouseEvent, ipc::ClientToServerMsg};

pub async fn ws_handler_control(
    ws: WebSocketUpgrade,
    path: Option<AxumPath<String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!(
        "Control WebSocket connection established with path: {:?}",
        path
    );
    ws.on_upgrade(move |socket| handle_ws_control(socket, state))
}

pub async fn ws_handler_terminal(
    ws: WebSocketUpgrade,
    session_name: Option<AxumPath<String>>,
    Query(params): Query<TerminalParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    log::info!(
        "Terminal WebSocket connection established with session_name: {:?}",
        session_name
    );

    ws.on_upgrade(move |socket| handle_ws_terminal(socket, session_name, params, state))
}

async fn handle_ws_control(socket: WebSocket, state: AppState) {
    info!("New Control WebSocket connection established");

    let config = SetConfigPayload::from((&state.config, &state.config_options));
    let set_config_msg = WebServerToWebClientControlMessage::SetConfig(config);
    info!("Sending initial config to client: {:?}", set_config_msg);

    let (control_socket_tx, mut control_socket_rx) = socket.split();

    let (control_channel_tx, control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    send_control_messages_to_client(control_channel_rx, control_socket_tx);

    let _ = control_channel_tx.send(Message::Text(
        serde_json::to_string(&set_config_msg).unwrap().into(),
    ));

    let send_message_to_server = |deserialized_msg: WebClientToWebServerControlMessage| {
        let Some(client_connection) = state
            .connection_table
            .lock()
            .unwrap()
            .get_client_os_api(&deserialized_msg.web_client_id)
            .cloned()
        else {
            log::error!("Unknown web_client_id: {}", deserialized_msg.web_client_id);
            return;
        };
        let client_msg = match deserialized_msg.payload {
            WebClientToWebServerControlMessagePayload::TerminalResize(size) => {
                ClientToServerMsg::TerminalResize(size)
            },
        };

        let _ = client_connection.send_to_server(client_msg);
    };

    let mut set_client_control_channel = false;

    while let Some(Ok(msg)) = control_socket_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<WebClientToWebServerControlMessage, _> =
                    serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        if !set_client_control_channel {
                            set_client_control_channel = true;
                            state
                                .connection_table
                                .lock()
                                .unwrap()
                                .add_client_control_tx(
                                    &deserialized_msg.web_client_id,
                                    control_channel_tx.clone(),
                                );
                        }
                        send_message_to_server(deserialized_msg);
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize client msg: {:?}", e);
                    },
                }
            },
            Message::Close(_) => {
                log::info!("Control WebSocket connection closed, exiting");
                return;
            },
            _ => {
                log::error!("Unsupported messagetype : {:?}", msg);
            },
        }
    }
}

async fn handle_ws_terminal(
    socket: WebSocket,
    session_name: Option<AxumPath<String>>,
    params: TerminalParams,
    state: AppState,
) {
    let web_client_id = params.web_client_id;
    let Some(os_input) = state
        .connection_table
        .lock()
        .unwrap()
        .get_client_os_api(&web_client_id)
        .cloned()
    else {
        log::error!("Unknown web_client_id: {}", web_client_id);
        return;
    };

    let (client_channel_tx, mut client_channel_rx) = socket.split();
    info!(
        "New Terminal WebSocket connection established {:?}",
        session_name
    );
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .connection_table
        .lock()
        .unwrap()
        .add_client_terminal_tx(&web_client_id, stdout_channel_tx);

    zellij_server_listener(
        os_input.clone(),
        state.connection_table.clone(),
        session_name.map(|p| p.0),
        state.config.clone(),
        state.config_options.clone(),
        Some(state.config_file_path.clone()),
        web_client_id.clone(),
        state.session_manager.clone(),
    );

    render_to_client(stdout_channel_rx, client_channel_tx);

    let explicitly_disable_kitty_keyboard_protocol = state
        .config
        .options
        .support_kitty_keyboard_protocol
        .map(|e| !e)
        .unwrap_or(false);
    let mut mouse_old_event = MouseEvent::new();
    while let Some(Ok(msg)) = client_channel_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let Some(client_connection) = state
                    .connection_table
                    .lock()
                    .unwrap()
                    .get_client_os_api(&web_client_id)
                    .cloned()
                else {
                    log::error!("Unknown web_client_id: {}", web_client_id);
                    continue;
                };
                parse_stdin(
                    msg.as_bytes(),
                    client_connection.clone(),
                    &mut mouse_old_event,
                    explicitly_disable_kitty_keyboard_protocol,
                );
            },
            Message::Close(_) => {
                log::info!("Client WebSocket connection closed, exiting");
                state
                    .connection_table
                    .lock()
                    .unwrap()
                    .remove_client(&web_client_id);
                break;
            },
            _ => {
                log::error!("Unsupported websocket msg type");
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
}
