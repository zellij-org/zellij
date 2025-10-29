use crate::input_handler::from_termwiz;
use crate::keyboard_parser::KittyKeyboardParser;
use crate::os_input_output::ClientOsApi;
use crate::web_client::types::BRACKETED_PASTE_END;
use crate::web_client::types::BRACKETED_PASTE_START;

use zellij_utils::{
    input::{actions::Action, cast_termwiz_key, mouse::MouseEvent},
    ipc::ClientToServerMsg,
};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::{prelude::stream::SplitSink, SinkExt};
use termwiz::input::{InputEvent, InputParser};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

pub fn render_to_client(
    mut stdout_channel_rx: UnboundedReceiver<String>,
    mut client_channel_tx: SplitSink<WebSocket, Message>,
    cancellation_token: CancellationToken,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                result = stdout_channel_rx.recv() => {
                    match result {
                        Some(rendered_bytes) => {
                            if client_channel_tx
                                .send(Message::Text(rendered_bytes.into()))
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        None => break,
                    }
                }
                _ = cancellation_token.cancelled() => {
                    let close_frame = CloseFrame {
                        code: axum::extract::ws::close_code::NORMAL,
                        reason: "Connection closed".into(),
                    };
                    let close_message = Message::Close(Some(close_frame));
                    if client_channel_tx
                        .send(close_message)
                        .await
                        .is_err()
                    {
                        break;
                    }
                    break;
                }
            }
        }
    });
}

pub fn send_control_messages_to_client(
    mut control_channel_rx: UnboundedReceiver<Message>,
    mut socket_channel_tx: SplitSink<WebSocket, Message>,
) {
    tokio::spawn(async move {
        while let Some(message) = control_channel_rx.recv().await {
            if socket_channel_tx.send(message).await.is_err() {
                break;
            }
        }
    });
}

pub fn parse_stdin(
    buf: &[u8],
    os_input: Box<dyn ClientOsApi>,
    mouse_old_event: &mut MouseEvent,
    explicitly_disable_kitty_keyboard_protocol: bool,
) {
    if !explicitly_disable_kitty_keyboard_protocol {
        match KittyKeyboardParser::new().parse(&buf) {
            Some(key_with_modifier) => {
                os_input.send_to_server(ClientToServerMsg::Key {
                    key: key_with_modifier.clone(),
                    raw_bytes: buf.to_vec(),
                    is_kitty_keyboard_protocol: true,
                });
                return;
            },
            None => {},
        }
    }

    let mut input_parser = InputParser::new();
    let maybe_more = false;
    let mut events = vec![];
    input_parser.parse(
        &buf,
        |input_event: InputEvent| {
            events.push(input_event);
        },
        maybe_more,
    );

    for (_i, input_event) in events.into_iter().enumerate() {
        match input_event {
            InputEvent::Key(key_event) => {
                let key = cast_termwiz_key(key_event.clone(), &buf, None);
                os_input.send_to_server(ClientToServerMsg::Key {
                    key: key.clone(),
                    raw_bytes: buf.to_vec(),
                    is_kitty_keyboard_protocol: false,
                });
            },
            InputEvent::Mouse(mouse_event) => {
                let mouse_event = from_termwiz(mouse_old_event, mouse_event);
                let action = Action::MouseEvent { event: mouse_event };
                os_input.send_to_server(ClientToServerMsg::Action {
                    action,
                    terminal_id: None,
                    client_id: None,
                    is_cli_client: false,
                });
            },
            InputEvent::Paste(pasted_text) => {
                os_input.send_to_server(ClientToServerMsg::Action {
                    action: Action::Write {
                        key_with_modifier: None,
                        bytes: BRACKETED_PASTE_START.to_vec(),
                        is_kitty_keyboard_protocol: false,
                    },
                    terminal_id: None,
                    client_id: None,
                    is_cli_client: false,
                });
                os_input.send_to_server(ClientToServerMsg::Action {
                    action: Action::Write {
                        key_with_modifier: None,
                        bytes: pasted_text.as_bytes().to_vec(),
                        is_kitty_keyboard_protocol: false,
                    },
                    terminal_id: None,
                    client_id: None,
                    is_cli_client: false,
                });
                os_input.send_to_server(ClientToServerMsg::Action {
                    action: Action::Write {
                        key_with_modifier: None,
                        bytes: BRACKETED_PASTE_END.to_vec(),
                        is_kitty_keyboard_protocol: false,
                    },
                    terminal_id: None,
                    client_id: None,
                    is_cli_client: false,
                });
            },
            _ => {
                log::error!("Unsupported event: {:#?}", input_event);
            },
        }
    }
}
