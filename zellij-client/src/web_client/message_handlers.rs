use crate::input_handler::from_termwiz;
use crate::keyboard_parser::KittyKeyboardParser;
use crate::os_input_output::ClientOsApi;
use crate::web_client::types::BRACKETED_PASTE_END;
use crate::web_client::types::BRACKETED_PASTE_START;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use zellij_utils::{
    input::{actions::Action, cast_termwiz_key, mouse::MouseEvent},
    ipc::ClientToServerMsg,
    vendored::termwiz::input::{InputEvent, InputParser},
};

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use futures::{prelude::stream::SplitSink, SinkExt};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

pub fn render_to_client(
    mut stdout_channel_rx: UnboundedReceiver<String>,
    mut client_channel_tx: SplitSink<WebSocket, Message>,
    cancellation_token: CancellationToken,
    should_not_reconnect: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = cancellation_token.cancelled() => {
                    let code = if should_not_reconnect.load(Ordering::Relaxed) {
                        4001u16
                    } else {
                        axum::extract::ws::close_code::NORMAL
                    };
                    let close_frame = CloseFrame {
                        code,
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
    // Check if this looks like multi-byte input (either UTF-8 or multi-byte ASCII from IME)
    // Multi-byte UTF-8: first byte has 0xC0 bits set
    // Multi-byte ASCII: len > 1 and all bytes are ASCII (sent as batch from IME)
    // But exclude ANSI escape sequences (ESC followed by printable chars)
    let is_multibyte_utf8 = buf.len() > 1 && (buf[0] & 0xC0) == 0xC0;
    let is_ansi_escape = buf.len() >= 2 && buf[0] == 0x1B; // ESC character
    let is_multibyte_ascii = buf.len() > 1 && buf.iter().all(|&b| b < 128);

    if (is_multibyte_utf8 || (is_multibyte_ascii && !is_ansi_escape)) {
        // For multi-byte input (IME input or UTF-8), bypass InputParser and send directly
        // to avoid each byte/character being parsed as separate events
        os_input.send_to_server(ClientToServerMsg::Action {
            action: Action::Write {
                key_with_modifier: None,
                bytes: buf.to_vec(),
                is_kitty_keyboard_protocol: false,
            },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
        return;
    }

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

    let single_event = events.len() == 1;
    for (_i, input_event) in events.into_iter().enumerate() {
        match input_event {
            InputEvent::Key(key_event) => {
                // For multi-event buffers (e.g. IME composition), avoid
                // duplicating the full buffer for each unmodified Char event.
                // Non-Char or modified keys still use the original buffer.
                let raw_bytes = if single_event {
                    buf.to_vec()
                } else {
                    use zellij_utils::vendored::termwiz::input::{KeyCode, Modifiers};
                    match (&key_event.key, key_event.modifiers) {
                        (KeyCode::Char(c), m) if m == Modifiers::NONE => {
                            let mut char_buf = [0u8; 4];
                            c.encode_utf8(&mut char_buf).as_bytes().to_vec()
                        },
                        _ => buf.to_vec(),
                    }
                };
                let key = cast_termwiz_key(key_event.clone(), &raw_bytes, None);
                os_input.send_to_server(ClientToServerMsg::Key {
                    key: key.clone(),
                    raw_bytes,
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

#[cfg(test)]
mod tests {
    use super::parse_stdin;
    use crate::os_input_output::ClientOsApi;
    use std::io::{BufRead, Cursor, Write};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use zellij_utils::{
        data::Palette,
        errors::ErrorContext,
        input::mouse::MouseEvent,
        ipc::{ClientToServerMsg, ServerToClientMsg},
        pane_size::Size,
    };

    #[derive(Clone, Debug, Default)]
    struct RecordingOsInput {
        sent_messages: Arc<Mutex<Vec<ClientToServerMsg>>>,
    }

    impl RecordingOsInput {
        fn take_sent_messages(&self) -> Vec<ClientToServerMsg> {
            self.sent_messages.lock().unwrap().clone()
        }
    }

    impl ClientOsApi for RecordingOsInput {
        fn get_terminal_size(&self) -> Size {
            Size::default()
        }
        fn set_raw_mode(&mut self) {}
        fn unset_raw_mode(&self) -> Result<(), std::io::Error> {
            Ok(())
        }
        fn get_stdout_writer(&self) -> Box<dyn Write> {
            Box::new(std::io::sink())
        }
        fn get_stdin_reader(&self) -> Box<dyn BufRead> {
            Box::new(Cursor::new(Vec::<u8>::new()))
        }
        fn update_session_name(&mut self, _new_session_name: String) {}
        fn read_from_stdin(&mut self) -> Result<Vec<u8>, &'static str> {
            Ok(vec![])
        }
        fn box_clone(&self) -> Box<dyn ClientOsApi> {
            Box::new(self.clone())
        }
        fn send_to_server(&self, msg: ClientToServerMsg) {
            self.sent_messages.lock().unwrap().push(msg);
        }
        fn recv_from_server(&self) -> Option<(ServerToClientMsg, ErrorContext)> {
            None
        }
        fn handle_signals(
            &self,
            _sigwinch_cb: Box<dyn Fn()>,
            _quit_cb: Box<dyn Fn()>,
            _resize_receiver: Option<std::sync::mpsc::Receiver<()>>,
        ) {
        }
        fn connect_to_server(&self, _path: &Path) {}
        fn load_palette(&self) -> Palette {
            Palette::default()
        }
        fn enable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }
        fn disable_mouse(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn ime_multi_char_input_uses_per_char_raw_bytes() {
        let os_input = RecordingOsInput::default();
        let mut mouse_old_event = MouseEvent::new();

        parse_stdin(
            "你好".as_bytes(),
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            false,
        );

        let sent_messages = os_input.take_sent_messages();
        assert_eq!(sent_messages.len(), 2);

        let raw_bytes: Vec<Vec<u8>> = sent_messages
            .into_iter()
            .map(|message| match message {
                ClientToServerMsg::Key { raw_bytes, .. } => raw_bytes,
                other => panic!("expected key message, got {other:?}"),
            })
            .collect();

        assert_eq!(
            raw_bytes,
            vec!["你".as_bytes().to_vec(), "好".as_bytes().to_vec()]
        );
    }

    #[test]
    fn single_char_input_keeps_original_raw_bytes() {
        let os_input = RecordingOsInput::default();
        let mut mouse_old_event = MouseEvent::new();

        parse_stdin(
            "a".as_bytes(),
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            false,
        );

        let sent_messages = os_input.take_sent_messages();
        assert_eq!(sent_messages.len(), 1);

        match &sent_messages[0] {
            ClientToServerMsg::Key { raw_bytes, .. } => {
                assert_eq!(raw_bytes, &b"a".to_vec());
            },
            other => panic!("expected key message, got {other:?}"),
        }
    }
}
