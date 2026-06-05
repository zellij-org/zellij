use crate::input_handler::from_termwiz;
use crate::keyboard_parser::{KittyKeyboardParser, KittyParseOutcome};
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

/// Per-WebSocket-connection parsing state. Owns the Kitty and termwiz
/// parsers so a CSI / Kitty sequence split across two WebSocket frames
/// resolves on the second frame instead of being dropped or
/// degraded-and-emitted as separate keys.
pub struct StdinSession {
    kitty_parser: KittyKeyboardParser,
    input_parser: InputParser,
    explicitly_disable_kitty_keyboard_protocol: bool,
    /// Set when the last frame left the termwiz parser holding an
    /// ambiguous-but-complete event (e.g. a bare ESC). Cleared by
    /// `finalize` when an idle timeout drains those events.
    pending_finalize: bool,
}

impl StdinSession {
    pub fn new(explicitly_disable_kitty_keyboard_protocol: bool) -> Self {
        StdinSession {
            kitty_parser: KittyKeyboardParser::new(),
            input_parser: InputParser::new(),
            explicitly_disable_kitty_keyboard_protocol,
            pending_finalize: false,
        }
    }

    pub fn pending_finalize(&self) -> bool {
        self.pending_finalize
    }

    /// Clear the pending-finalize flag without draining events. Used
    /// when the WebSocket loop hits an idle timeout but the client
    /// connection has already been removed — there's nowhere to send
    /// the drained events, and leaving the flag set would busy-loop the
    /// idle timer.
    pub fn clear_pending_finalize(&mut self) {
        self.pending_finalize = false;
    }

    /// Drain any ambiguous-but-complete events that termwiz held back
    /// on the previous `parse_stdin` call. Called from the WebSocket
    /// loop after an idle interval with no further frames, mirroring
    /// `stdin_handler::finalize_events`.
    pub fn finalize(&mut self, os_input: &dyn ClientOsApi, mouse_old_event: &mut MouseEvent) {
        let mut events = vec![];
        self.input_parser.parse(
            &[],
            |input_event: InputEvent| {
                events.push(input_event);
            },
            false,
        );
        for input_event in events {
            dispatch_termwiz_event(os_input, mouse_old_event, input_event, &[]);
        }
        self.pending_finalize = false;
    }
}

/// Dispatch a single termwiz `InputEvent` produced by either the live
/// path or the idle finalize path. `raw_bytes` is the byte slice that
/// produced the event in the live path; finalize passes an empty slice
/// because no frame is associated with the drained events.
fn dispatch_termwiz_event(
    os_input: &dyn ClientOsApi,
    mouse_old_event: &mut MouseEvent,
    input_event: InputEvent,
    raw_bytes: &[u8],
) {
    match input_event {
        InputEvent::Key(key_event) => {
            let raw_bytes_vec = raw_bytes.to_vec();
            let key = cast_termwiz_key(key_event.clone(), &raw_bytes_vec, None);
            os_input.send_to_server(ClientToServerMsg::Key {
                key: key.clone(),
                raw_bytes: raw_bytes_vec,
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
    session: &mut StdinSession,
) {
    if !session.explicitly_disable_kitty_keyboard_protocol {
        match session.kitty_parser.feed(buf) {
            KittyParseOutcome::Complete(key_with_modifier) => {
                os_input.send_to_server(ClientToServerMsg::Key {
                    key: key_with_modifier.clone(),
                    raw_bytes: buf.to_vec(),
                    is_kitty_keyboard_protocol: true,
                });
                return;
            },
            KittyParseOutcome::Incomplete | KittyParseOutcome::NoMatch => {},
        }
    }

    // maybe_more = true so termwiz buffers ambiguous prefixes across
    // WebSocket frames; the WebSocket loop calls `session.finalize`
    // after an idle interval to drain any remaining ambiguous events.
    let maybe_more = true;
    let mut events = vec![];
    session.input_parser.parse(
        buf,
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
            other => {
                dispatch_termwiz_event(&*os_input, mouse_old_event, other, buf);
            },
        }
    }

    session.pending_finalize = true;
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
        let mut session = super::StdinSession::new(false);

        parse_stdin(
            "你好".as_bytes(),
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            &mut session,
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
        let mut session = super::StdinSession::new(false);

        parse_stdin(
            "a".as_bytes(),
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            &mut session,
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

    /// Cross-frame fragmentation regression. Under WebSocket transport,
    /// a single ANSI sequence routinely arrives split across two frames.
    /// The per-connection `StdinSession` must keep parser state across
    /// `parse_stdin` calls so a Kitty CSI sequence that crosses a frame
    /// boundary still resolves on the second frame.
    #[test]
    fn fragmented_kitty_sequence_resolves_across_frames() {
        use zellij_utils::data::{BareKey, KeyWithModifier};
        let os_input = RecordingOsInput::default();
        let mut mouse_old_event = MouseEvent::new();
        let mut session = super::StdinSession::new(false);

        // Frame 1: prefix only — incomplete.
        parse_stdin(
            b"\x1b[97;",
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            &mut session,
        );
        assert!(
            os_input.take_sent_messages().is_empty(),
            "incomplete Kitty prefix must not emit a key on frame 1"
        );

        // Frame 2: completes the sequence — Kitty parser emits.
        parse_stdin(
            b"5u",
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            &mut session,
        );
        let sent = os_input.take_sent_messages();
        assert_eq!(
            sent.len(),
            1,
            "exactly one Kitty key event after both frames; got {:?}",
            sent
        );
        match &sent[0] {
            ClientToServerMsg::Key {
                key,
                is_kitty_keyboard_protocol,
                ..
            } => {
                assert!(*is_kitty_keyboard_protocol);
                assert_eq!(
                    *key,
                    KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier()
                );
            },
            other => panic!("expected Key message, got {other:?}"),
        }
    }

    /// Plain printable bytes that aren't a Kitty sequence must NOT be
    /// trapped in the long-lived Kitty parser. After `feed()` returns
    /// `NoMatch` and resets, the bytes flow through the termwiz path
    /// and reach the server as ordinary key events.
    #[test]
    fn non_kitty_bytes_pass_through_to_termwiz_path() {
        let os_input = RecordingOsInput::default();
        let mut mouse_old_event = MouseEvent::new();
        let mut session = super::StdinSession::new(false);

        parse_stdin(
            b"ab",
            Box::new(os_input.clone()),
            &mut mouse_old_event,
            &mut session,
        );

        let sent = os_input.take_sent_messages();
        // Two Char events ('a', 'b'), both via the termwiz path.
        assert_eq!(sent.len(), 2);
        for msg in &sent {
            match msg {
                ClientToServerMsg::Key {
                    is_kitty_keyboard_protocol,
                    ..
                } => assert!(!*is_kitty_keyboard_protocol),
                other => panic!("expected Key, got {other:?}"),
            }
        }
    }
}
