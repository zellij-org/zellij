use std::sync::{Arc, Mutex};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::engine::Engine as _;
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::ClientToServerMsg;

pub type ClipboardHandle = Arc<Mutex<Clipboard>>;

pub struct Clipboard {
    backend: Backend,
}

enum Backend {
    System(Box<arboard::Clipboard>),
    Memory { clipboard: String, primary: String },
}

impl Clipboard {
    pub fn open() -> Self {
        match arboard::Clipboard::new() {
            Ok(clipboard) => Self {
                backend: Backend::System(Box::new(clipboard)),
            },
            Err(e) => {
                eprintln!(
                    "zellij-window: no system clipboard ({}); copy and paste stay inside this window",
                    e
                );
                Self::in_memory()
            },
        }
    }

    pub fn in_memory() -> Self {
        Self {
            backend: Backend::Memory {
                clipboard: String::new(),
                primary: String::new(),
            },
        }
    }

    pub fn set(&mut self, text: &str) {
        match &mut self.backend {
            Backend::System(clipboard) => {
                if let Err(e) = clipboard.set_text(text) {
                    eprintln!("zellij-window: failed to write the clipboard: {}", e);
                }
            },
            Backend::Memory { clipboard, .. } => {
                clipboard.clear();
                clipboard.push_str(text);
            },
        }
    }

    pub fn get(&mut self) -> Option<String> {
        match &mut self.backend {
            Backend::System(clipboard) => reported(clipboard.get_text(), true),
            Backend::Memory { clipboard, .. } => (!clipboard.is_empty()).then(|| clipboard.clone()),
        }
    }

    pub fn get_primary(&mut self) -> Option<String> {
        match &mut self.backend {
            Backend::System(clipboard) => primary_text(clipboard),
            Backend::Memory { primary, .. } => (!primary.is_empty()).then(|| primary.clone()),
        }
    }

    #[cfg(test)]
    pub fn set_primary(&mut self, text: &str) {
        match &mut self.backend {
            Backend::System(_) => panic!("a test must not write the host's primary selection"),
            Backend::Memory { primary, .. } => {
                primary.clear();
                primary.push_str(text);
            },
        }
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
fn primary_text(clipboard: &mut arboard::Clipboard) -> Option<String> {
    use arboard::{GetExtLinux, LinuxClipboardKind};
    reported(
        clipboard
            .get()
            .clipboard(LinuxClipboardKind::Primary)
            .text(),
        false,
    )
}

#[cfg(not(all(unix, not(target_os = "macos"), not(target_os = "android"))))]
fn primary_text(_clipboard: &mut arboard::Clipboard) -> Option<String> {
    None
}

fn reported(read: Result<String, arboard::Error>, announce_empty: bool) -> Option<String> {
    match read {
        Ok(text) if !text.is_empty() => Some(text),
        Ok(_) => None,
        Err(arboard::Error::ContentNotAvailable) => {
            if announce_empty {
                eprintln!("zellij-window: the clipboard holds no text; nothing to paste");
            }
            None
        },
        Err(e) => {
            eprintln!("zellij-window: failed to read the clipboard: {}", e);
            None
        },
    }
}

pub fn osc52_reply(query_bytes: &[u8], clipboard: &ClipboardHandle) -> Option<Vec<u8>> {
    let (selection, terminator) = parse_osc52_query(query_bytes)?;
    let text = clipboard
        .lock()
        .map_err(|_| ())
        .ok()
        .and_then(|mut clipboard| clipboard.get())
        .unwrap_or_default();

    let mut reply = format!("\u{1b}]52;{};{}", selection, BASE64.encode(text)).into_bytes();
    reply.extend_from_slice(terminator);
    Some(reply)
}

fn parse_osc52_query(bytes: &[u8]) -> Option<(char, &[u8])> {
    let rest = bytes.strip_prefix(b"\x1b]52;")?;
    let (selection, rest) = rest.split_first()?;
    let rest = rest.strip_prefix(b";")?;
    let rest = rest.strip_prefix(b"?")?;
    match rest {
        b"\x07" | b"\x1b\\" => Some((*selection as char, rest)),
        _ => None,
    }
}

pub fn paste_message(chars: String) -> ClientToServerMsg {
    ClientToServerMsg::Action {
        action: Action::Paste {
            chars,
            pane_id: None,
        },
        terminal_id: None,
        client_id: None,
        is_cli_client: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::connection::{send, test_attach_at as attach_at, Capabilities, Geometry};
    use crate::test_server::FakeServer;

    fn geometry() -> Geometry {
        Geometry {
            rows: 40,
            cols: 120,
            cell_width: 10,
            cell_height: 20,
        }
    }

    #[test]
    fn an_unwritten_clipboard_yields_nothing() {
        assert_eq!(Clipboard::in_memory().get(), None);
    }

    #[test]
    fn what_is_written_comes_back() {
        let mut clipboard = Clipboard::in_memory();
        clipboard.set("copied");
        assert_eq!(clipboard.get().as_deref(), Some("copied"));
    }

    #[test]
    fn an_empty_write_is_indistinguishable_from_no_write() {
        let mut clipboard = Clipboard::in_memory();
        clipboard.set("copied");
        clipboard.set("");
        assert_eq!(clipboard.get(), None);
    }

    #[test]
    fn the_primary_selection_is_a_second_store_and_not_the_clipboard() {
        let mut clipboard = Clipboard::in_memory();
        assert_eq!(clipboard.get_primary(), None);
        clipboard.set("copied");
        assert_eq!(
            clipboard.get_primary(),
            None,
            "a copy must not appear in the primary selection"
        );
        clipboard.set_primary("selected");
        assert_eq!(clipboard.get_primary().as_deref(), Some("selected"));
        assert_eq!(
            clipboard.get().as_deref(),
            Some("copied"),
            "the primary selection must not overwrite the clipboard"
        );
    }

    fn handle(text: &str) -> ClipboardHandle {
        let mut clipboard = Clipboard::in_memory();
        clipboard.set(text);
        Arc::new(Mutex::new(clipboard))
    }

    #[test]
    fn a_clipboard_read_is_answered_with_base64_content() {
        let reply = osc52_reply(b"\x1b]52;c;?\x1b\\", &handle("copied")).unwrap();
        assert_eq!(reply, b"\x1b]52;c;Y29waWVk\x1b\\");
    }

    #[test]
    fn the_terminator_and_selection_of_the_query_are_echoed_back() {
        let reply = osc52_reply(b"\x1b]52;p;?\x07", &handle("copied")).unwrap();
        assert_eq!(reply, b"\x1b]52;p;Y29waWVk\x07");
    }

    #[test]
    fn an_empty_clipboard_answers_with_an_empty_payload() {
        let reply = osc52_reply(
            b"\x1b]52;c;?\x1b\\",
            &Arc::new(Mutex::new(Clipboard::in_memory())),
        )
        .unwrap();
        assert_eq!(reply, b"\x1b]52;c;\x1b\\");
    }

    #[test]
    fn the_reply_round_trips_back_to_the_original_text() {
        let text = "line one\nline two\t\u{1b}[31m";
        let reply = osc52_reply(b"\x1b]52;c;?\x1b\\", &handle(text)).unwrap();
        let payload = &reply[b"\x1b]52;c;".len()..reply.len() - 2];
        assert_eq!(BASE64.decode(payload).unwrap(), text.as_bytes());
    }

    #[test]
    fn only_the_clipboard_read_query_is_answered() {
        let clipboard = handle("copied");
        for query in [
            &b"\x1b]11;?\x1b\\"[..],
            &b"\x1b]4;1;?\x07"[..],
            &b"\x1b[14t"[..],
            &b"\x1b]52;c;Y29waWVk\x1b\\"[..],
            &b"\x1b]52;c;?"[..],
            &b""[..],
        ] {
            assert!(osc52_reply(query, &clipboard).is_none(), "{:?}", query);
        }
    }

    #[test]
    fn a_paste_reaches_the_server_as_a_paste_action() {
        let msg = paste_message("pasted".to_owned());
        let server = FakeServer::spawn(|side| side.expect(9));
        let connection = attach_at(
            &server.path,
            "window-test",
            geometry(),
            Capabilities::default(),
        )
        .unwrap();
        send(&connection.sender, msg.clone()).unwrap();
        drop(connection);

        assert_eq!(
            server.finish().last().unwrap(),
            &ClientToServerMsg::Action {
                action: Action::Paste {
                    chars: "pasted".to_owned(),
                    pane_id: None
                },
                terminal_id: None,
                client_id: None,
                is_cli_client: false,
            }
        );
        assert_eq!(msg, paste_message("pasted".to_owned()));
    }
}
