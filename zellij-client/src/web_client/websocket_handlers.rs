use crate::web_client::authentication::SessionTokenHash;
use crate::web_client::control_message::{
    TerminalMetricsPayload, WebClientToWebServerControlMessage,
    WebClientToWebServerControlMessagePayload,
};
use crate::web_client::message_handlers::{
    parse_stdin, render_to_client, send_control_messages_to_client, StdinSession,
};
use crate::web_client::server_listener::zellij_server_listener;
use crate::web_client::types::{AppState, ControlParams, TerminalParams};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path as AxumPath, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use futures::StreamExt;
use std::sync::{atomic::AtomicBool, Arc};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use zellij_utils::{
    data::PaneId,
    input::actions::Action,
    input::mouse::MouseEvent,
    ipc::{ClientToServerMsg, PixelDimensions},
    pane_size::{Size, SizeInPixels},
};

const PING_INTERVAL_SECS: u64 = 30;
const PONG_TIMEOUT_SECS: u64 = 45;

pub async fn ws_handler_control(
    ws: WebSocketUpgrade,
    _path: Option<AxumPath<String>>,
    Query(params): Query<ControlParams>,
    State(state): State<AppState>,
    axum::Extension(session_token_hash): axum::Extension<SessionTokenHash>,
) -> Response {
    if !state
        .connection_table
        .lock()
        .unwrap()
        .verify_client_ownership(&params.web_client_id, &session_token_hash.0)
    {
        log::error!(
            "Control WebSocket: client does not own web_client_id {}",
            params.web_client_id
        );
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |socket| handle_ws_control(socket, params, state))
}

pub async fn ws_handler_terminal(
    ws: WebSocketUpgrade,
    session_name: Option<AxumPath<String>>,
    Query(params): Query<TerminalParams>,
    State(state): State<AppState>,
    axum::Extension(session_token_hash): axum::Extension<SessionTokenHash>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| {
        handle_ws_terminal(socket, session_name, params, state, session_token_hash)
    })
}

async fn handle_ws_control(socket: WebSocket, params: ControlParams, state: AppState) {
    let web_client_id = params.web_client_id;

    let (control_socket_tx, mut control_socket_rx) = socket.split();

    let (control_channel_tx, control_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    send_control_messages_to_client(control_channel_rx, control_socket_tx);

    state
        .connection_table
        .lock()
        .unwrap()
        .add_client_control_tx(&web_client_id, control_channel_tx);

    // Track the time of the last received Pong (shared with the ping task).
    // Browsers automatically reply to WebSocket protocol-level Pings with Pongs,
    // even when the page's JS event loop is blocked or the tab is throttled,
    // so this is a reliable end-to-end liveness signal.
    let last_pong = Arc::new(Mutex::new(Instant::now()));
    let ping_cancellation = CancellationToken::new();

    // Spawn the ping task: send a Ping every PING_INTERVAL_SECS, and tear down
    // the connection if no Pong has been observed within PONG_TIMEOUT_SECS.
    let ping_tx = control_channel_tx.clone();
    let ping_last_pong = last_pong.clone();
    let ping_cancel = ping_cancellation.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(tokio::time::Duration::from_secs(PING_INTERVAL_SECS));
        loop {
            tokio::select! {
                _ = ping_cancel.cancelled() => {
                    break;
                }
                _ = interval.tick() => {
                    let elapsed = ping_last_pong.lock().await.elapsed();
                    if elapsed.as_secs() > PONG_TIMEOUT_SECS {
                        log::warn!("WebSocket control connection timed out (no Pong received)");
                        break;
                    }

                    if ping_tx.send(Message::Ping(Vec::new().into())).is_err() {
                        break;
                    }
                }
            }
        }
    });

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
        let Some(client_msg) = control_payload_to_server_msg(deserialized_msg.payload) else {
            return;
        };

        let _ = client_connection.send_to_server(client_msg);
    };

    while let Some(Ok(msg)) = control_socket_rx.next().await {
        match msg {
            Message::Text(msg) => {
                let deserialized_msg: Result<WebClientToWebServerControlMessage, _> =
                    serde_json::from_str(&msg);
                match deserialized_msg {
                    Ok(deserialized_msg) => {
                        if deserialized_msg.web_client_id != web_client_id {
                            log::error!(
                                "Client attempted to use web_client_id {} that does not belong to their connection",
                                deserialized_msg.web_client_id
                            );
                            return;
                        }
                        send_message_to_server(deserialized_msg);
                    },
                    Err(e) => {
                        log::error!("Failed to deserialize client msg: {:?}", e);
                    },
                }
            },
            Message::Pong(_) => {
                *last_pong.lock().await = Instant::now();
            },
            Message::Close(_) => {
                ping_cancellation.cancel();
                return;
            },
            _ => {
                log::error!("Unsupported messagetype : {:?}", msg);
            },
        }
    }

    ping_cancellation.cancel();
}

async fn handle_ws_terminal(
    socket: WebSocket,
    session_name: Option<AxumPath<String>>,
    params: TerminalParams,
    state: AppState,
    session_token_hash: SessionTokenHash,
) {
    let client_size = match (params.rows, params.cols) {
        (Some(rows), Some(cols)) if rows > 0 && cols > 0 => Some(Size {
            rows: rows as usize,
            cols: cols as usize,
        }),
        _ => None,
    };
    let client_pixel_dims = match (params.cell_width, params.cell_height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Some(SizeInPixels {
            width: width as usize,
            height: height as usize,
        }),
        _ => None,
    };
    let web_client_id = params.web_client_id;

    // Verify the session token owns this web_client_id
    if !state
        .connection_table
        .lock()
        .unwrap()
        .verify_client_ownership(&web_client_id, &session_token_hash.0)
    {
        log::error!(
            "Terminal WebSocket: client does not own web_client_id {}",
            web_client_id
        );
        return;
    }

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

    let (client_terminal_channel_tx, mut client_terminal_channel_rx) = socket.split();
    let (stdout_channel_tx, stdout_channel_rx) = tokio::sync::mpsc::unbounded_channel();
    state
        .connection_table
        .lock()
        .unwrap()
        .add_client_terminal_tx(&web_client_id, stdout_channel_tx);

    let (attachment_complete_tx, attachment_complete_rx) = tokio::sync::oneshot::channel();

    zellij_server_listener(
        os_input.clone(),
        state.connection_table.clone(),
        session_name.map(|p| p.0),
        state.config.lock().unwrap().clone(),
        state.config_options.clone(),
        Some(state.config_file_path.clone()),
        web_client_id.clone(),
        state.session_manager.clone(),
        Some(attachment_complete_tx),
        client_size,
        client_pixel_dims,
        state.pending_welcome_sessions.clone(),
    );

    let terminal_channel_cancellation_token = CancellationToken::new();
    let should_not_reconnect = state
        .connection_table
        .lock()
        .unwrap()
        .get_should_not_reconnect_flag(&web_client_id)
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
    render_to_client(
        stdout_channel_rx,
        client_terminal_channel_tx,
        terminal_channel_cancellation_token.clone(),
        should_not_reconnect,
    );
    state
        .connection_table
        .lock()
        .unwrap()
        .add_client_terminal_channel_cancellation_token(
            &web_client_id,
            terminal_channel_cancellation_token,
        );

    let explicitly_disable_kitty_keyboard_protocol = state
        .config
        .lock()
        .unwrap()
        .options
        .support_kitty_keyboard_protocol
        .map(|e| !e)
        .unwrap_or(false);

    let _ = attachment_complete_rx.await;

    let mut mouse_old_event = MouseEvent::new();
    // Per-connection parser state. Hoisted so a CSI / Kitty sequence
    // split across two WebSocket frames resolves on the second frame.
    let mut stdin_session = StdinSession::new(explicitly_disable_kitty_keyboard_protocol);
    let finalize_idle = std::time::Duration::from_millis(50);
    loop {
        // When termwiz is holding ambiguous-but-complete events from
        // the previous frame, race the next frame against an idle
        // timeout so the held events still drain if no further frame
        // arrives.
        let result = if stdin_session.pending_finalize() {
            tokio::select! {
                msg = client_terminal_channel_rx.next() => Some(msg),
                _ = tokio::time::sleep(finalize_idle) => None,
            }
        } else {
            Some(client_terminal_channel_rx.next().await)
        };
        let msg = match result {
            Some(Some(Ok(m))) => m,
            Some(_) => break,
            None => {
                // Idle timeout fired with `pending_finalize` set:
                // drain any ambiguous-but-complete events termwiz held
                // back on the previous frame.
                if let Some(client_connection) = state
                    .connection_table
                    .lock()
                    .unwrap()
                    .get_client_os_api(&web_client_id)
                    .cloned()
                {
                    stdin_session.finalize(&*client_connection, &mut mouse_old_event);
                } else {
                    // No client to send drained events to — clear the
                    // flag so we don't busy-loop the idle timer.
                    stdin_session.clear_pending_finalize();
                }
                continue;
            },
        };
        match msg {
            Message::Binary(buf) => {
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
                    &buf,
                    client_connection.clone(),
                    &mut mouse_old_event,
                    &mut stdin_session,
                );
            },
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
                    &mut stdin_session,
                );
            },
            Message::Close(_) => {
                state
                    .connection_table
                    .lock()
                    .unwrap()
                    .remove_client(&web_client_id);
                break;
            },
            // TODO: support Message::Binary
            _ => {
                log::error!("Unsupported websocket msg type");
            },
        }
    }
    os_input.send_to_server(ClientToServerMsg::ClientExited);
}

fn control_payload_to_server_msg(
    payload: WebClientToWebServerControlMessagePayload,
) -> Option<ClientToServerMsg> {
    let client_msg = match payload {
        WebClientToWebServerControlMessagePayload::TerminalResize(size) => {
            ClientToServerMsg::TerminalResize { new_size: size }
        },
        WebClientToWebServerControlMessagePayload::TerminalMetrics(metrics) => {
            terminal_metrics_to_ipc(metrics)
        },
        WebClientToWebServerControlMessagePayload::SoftKeyboardVisibilityChanged { visible } => {
            ClientToServerMsg::SoftKeyboardVisibilityChanged { visible }
        },
        WebClientToWebServerControlMessagePayload::NestedSessionFrameFromHost { payload_bytes } => {
            ClientToServerMsg::NestedSessionFrameFromHost { payload_bytes }
        },
        WebClientToWebServerControlMessagePayload::RequestSessionList => {
            ClientToServerMsg::RequestSessionList
        },
        WebClientToWebServerControlMessagePayload::FocusPane { pane_id, is_plugin } => {
            let pane_id = if is_plugin {
                PaneId::Plugin(pane_id)
            } else {
                PaneId::Terminal(pane_id)
            };
            ClientToServerMsg::Action {
                action: Action::FocusPaneByPaneId { pane_id },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        },
        WebClientToWebServerControlMessagePayload::NewPaneInTab { .. } => {
            ClientToServerMsg::Action {
                action: Action::NewTiledPane {
                    direction: None,
                    command: None,
                    pane_name: None,
                    near_current_pane: false,
                    no_focus: false,
                    borderless: None,
                    tab_id: None,
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        },
        WebClientToWebServerControlMessagePayload::NewTab => ClientToServerMsg::Action {
            action: Action::NewTab {
                tiled_layout: None,
                floating_layouts: vec![],
                swap_tiled_layouts: None,
                swap_floating_layouts: None,
                tab_name: None,
                should_change_focus_to_new_tab: true,
                cwd: None,
                initial_panes: None,
                first_pane_unblock_condition: None,
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        },
        WebClientToWebServerControlMessagePayload::SetMobileRenderPreferences {
            single_pane,
            fit,
        } => ClientToServerMsg::SetMobileRenderPreferences { single_pane, fit },
        WebClientToWebServerControlMessagePayload::Unknown => {
            log::warn!("Ignoring unknown control message type from web client");
            return None;
        },
    };
    Some(client_msg)
}

fn terminal_metrics_to_ipc(metrics: TerminalMetricsPayload) -> ClientToServerMsg {
    ClientToServerMsg::TerminalPixelDimensions {
        pixel_dimensions: PixelDimensions {
            text_area_size: Some(SizeInPixels {
                width: metrics.text_area_pixel_width,
                height: metrics.text_area_pixel_height,
            }),
            character_cell_size: Some(SizeInPixels {
                width: metrics.cell_pixel_width,
                height: metrics.cell_pixel_height,
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_metrics_to_ipc_preserves_all_dimensions() {
        let metrics = TerminalMetricsPayload {
            cell_pixel_width: 9,
            cell_pixel_height: 18,
            text_area_pixel_width: 80 * 9,
            text_area_pixel_height: 24 * 18,
        };
        let msg = terminal_metrics_to_ipc(metrics);
        match msg {
            ClientToServerMsg::TerminalPixelDimensions { pixel_dimensions } => {
                let cell = pixel_dimensions
                    .character_cell_size
                    .expect("cell size missing");
                let area = pixel_dimensions
                    .text_area_size
                    .expect("text area size missing");
                assert_eq!(cell.width, 9);
                assert_eq!(cell.height, 18);
                assert_eq!(area.width, 720);
                assert_eq!(area.height, 432);
            },
            other => panic!("expected TerminalPixelDimensions, got {:?}", other),
        }
    }

    #[test]
    fn terminal_metrics_round_trips_through_json_payload() {
        // The browser sends this message as JSON over the control
        // socket. Verify that the on-wire shape deserializes into the
        // variant we route into terminal_metrics_to_ipc.
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": {
                "type": "TerminalMetrics",
                "cell_pixel_width": 7,
                "cell_pixel_height": 14,
                "text_area_pixel_width": 560,
                "text_area_pixel_height": 336,
            }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        let metrics = match parsed.payload {
            WebClientToWebServerControlMessagePayload::TerminalMetrics(m) => m,
            other => panic!("expected TerminalMetrics, got {:?}", other),
        };
        assert_eq!(metrics.cell_pixel_width, 7);
        assert_eq!(metrics.cell_pixel_height, 14);
        assert_eq!(metrics.text_area_pixel_width, 560);
        assert_eq!(metrics.text_area_pixel_height, 336);
    }

    #[test]
    fn terminal_resize_still_deserializes_after_adding_variant() {
        // Regression guard for the new enum variant: the existing
        // TerminalResize wire shape must continue to parse unchanged
        // (no `type` rename, no required-field changes).
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": {
                "type": "TerminalResize",
                "rows": 24,
                "cols": 80,
            }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        match parsed.payload {
            WebClientToWebServerControlMessagePayload::TerminalResize(size) => {
                assert_eq!(size.rows, 24);
                assert_eq!(size.cols, 80);
            },
            other => panic!("expected TerminalResize, got {:?}", other),
        }
    }

    #[test]
    fn focus_pane_payload_deserializes() {
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": {
                "type": "FocusPane",
                "pane_id": 7,
                "is_plugin": true,
            }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        match parsed.payload {
            WebClientToWebServerControlMessagePayload::FocusPane { pane_id, is_plugin } => {
                assert_eq!(pane_id, 7);
                assert!(is_plugin);
            },
            other => panic!("expected FocusPane, got {:?}", other),
        }
    }

    #[test]
    fn new_pane_in_tab_payload_deserializes() {
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": {
                "type": "NewPaneInTab",
                "tab_id": 2,
            }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        match parsed.payload {
            WebClientToWebServerControlMessagePayload::NewPaneInTab { tab_id } => {
                assert_eq!(tab_id, 2);
            },
            other => panic!("expected NewPaneInTab, got {:?}", other),
        }
    }

    #[test]
    fn new_tab_payload_deserializes() {
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": { "type": "NewTab" }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        assert!(matches!(
            parsed.payload,
            WebClientToWebServerControlMessagePayload::NewTab
        ));
    }

    #[test]
    fn set_mobile_render_preferences_payload_deserializes() {
        let raw = serde_json::json!({
            "web_client_id": "abc",
            "payload": {
                "type": "SetMobileRenderPreferences",
                "single_pane": false,
                "fit": true,
            }
        });
        let parsed: WebClientToWebServerControlMessage =
            serde_json::from_value(raw).expect("parse");
        match parsed.payload {
            WebClientToWebServerControlMessagePayload::SetMobileRenderPreferences {
                single_pane,
                fit,
            } => {
                assert!(!single_pane);
                assert!(fit);
            },
            other => panic!("expected SetMobileRenderPreferences, got {:?}", other),
        }
    }

    #[test]
    fn new_pane_in_tab_is_routed_to_the_requesting_client() {
        let client_msg = control_payload_to_server_msg(
            WebClientToWebServerControlMessagePayload::NewPaneInTab { tab_id: 2 },
        )
        .expect("message dropped");
        match client_msg {
            ClientToServerMsg::Action {
                action:
                    Action::NewTiledPane {
                        tab_id, no_focus, ..
                    },
                ..
            } => {
                assert_eq!(
                    tab_id, None,
                    "The pane is opened in the client's own tab so that it is focused for it, \
                     keeping single-pane mode attached to the new pane"
                );
                assert!(!no_focus, "The new pane takes focus");
            },
            other => panic!("expected a NewTiledPane action, got {:?}", other),
        }
    }

    #[test]
    fn unknown_control_message_is_dropped() {
        assert!(
            control_payload_to_server_msg(WebClientToWebServerControlMessagePayload::Unknown)
                .is_none()
        );
    }
}
