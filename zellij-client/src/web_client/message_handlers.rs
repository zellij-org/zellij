use crate::input_handler::from_termwiz;
use crate::keyboard_parser::KittyKeyboardParser;
use crate::os_input_output::ClientOsApi;
use crate::web_client::types::BRACKETED_PASTE_END;
use crate::web_client::types::BRACKETED_PASTE_START;

use zellij_utils::{
    input::{actions::Action, cast_termwiz_key, mouse::MouseEvent},
    ipc::ClientToServerMsg,
};

use axum::extract::ws::{Message, WebSocket};
use futures::{prelude::stream::SplitSink, SinkExt};
use termwiz::input::{InputEvent, InputParser};
use tokio::sync::mpsc::UnboundedReceiver;

pub fn render_to_client(
    mut stdout_channel_rx: UnboundedReceiver<String>,
    mut client_channel_tx: SplitSink<WebSocket, Message>,
) {
    tokio::spawn(async move {
        while let Some(rendered_bytes) = stdout_channel_rx.recv().await {
            if client_channel_tx
                .send(Message::Text(rendered_bytes.into()))
                .await
                .is_err()
            {
                break;
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
                os_input.send_to_server(ClientToServerMsg::Key(
                    key_with_modifier.clone(),
                    buf.to_vec(),
                    true,
                ));
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
                os_input.send_to_server(ClientToServerMsg::Key(key.clone(), buf.to_vec(), false));
            },
            InputEvent::Mouse(mouse_event) => {
                let mouse_event = from_termwiz(mouse_old_event, mouse_event);
                let action = Action::MouseEvent(mouse_event);
                os_input.send_to_server(ClientToServerMsg::Action(action, None, None));
            },
            InputEvent::Paste(pasted_text) => {
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, BRACKETED_PASTE_START.to_vec(), false),
                    None,
                    None,
                ));
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, pasted_text.as_bytes().to_vec(), false),
                    None,
                    None,
                ));
                os_input.send_to_server(ClientToServerMsg::Action(
                    Action::Write(None, BRACKETED_PASTE_END.to_vec(), false),
                    None,
                    None,
                ));
            },
            _ => {
                log::error!("Unsupported event: {:#?}", input_event);
            },
        }
    }
}
