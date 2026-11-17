use std::sync::{Arc, Mutex, MutexGuard};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::engine::Engine as _;
use zellij_utils::input::actions::Action;
use zellij_utils::ipc::ClientToServerMsg;

pub type ClipboardHandle = Arc<Mutex<Clipboard>>;

pub fn lock(handle: &ClipboardHandle) -> MutexGuard<'_, Clipboard> {
    handle.lock().unwrap_or_else(|poisoned| {
        report!("the clipboard was in use by a thread that crashed; using it anyway");
        handle.clear_poison();
        poisoned.into_inner()
    })
}

pub trait SystemClipboard: Send {
    fn set_text(&mut self, text: &str) -> Result<(), arboard::Error>;
    fn get_text(&mut self) -> Result<String, arboard::Error>;
    fn get_primary(&mut self) -> Option<String>;
    fn set_primary(&mut self, text: &str);
}

impl SystemClipboard for arboard::Clipboard {
    fn set_text(&mut self, text: &str) -> Result<(), arboard::Error> {
        arboard::Clipboard::set_text(self, text)
    }

    fn get_text(&mut self) -> Result<String, arboard::Error> {
        arboard::Clipboard::get_text(self)
    }

    fn get_primary(&mut self) -> Option<String> {
        primary_text(self)
    }

    fn set_primary(&mut self, text: &str) {
        set_primary_text(self, text)
    }
}

pub type SystemClipboardOpener =
    Box<dyn FnMut() -> Result<Box<dyn SystemClipboard>, arboard::Error> + Send>;

pub struct Clipboard {
    backend: Backend,
    opener: Option<SystemClipboardOpener>,
    reported_unavailable: bool,
}

enum Backend {
    System(Box<dyn SystemClipboard>),
    Memory { clipboard: String, primary: String },
}

impl Backend {
    fn memory() -> Self {
        Backend::Memory {
            clipboard: String::new(),
            primary: String::new(),
        }
    }
}

impl Clipboard {
    pub fn open() -> Self {
        Self::open_with(Box::new(|| {
            arboard::Clipboard::new()
                .map(|clipboard| Box::new(clipboard) as Box<dyn SystemClipboard>)
        }))
    }

    pub fn open_with(opener: SystemClipboardOpener) -> Self {
        let mut clipboard = Self {
            backend: Backend::memory(),
            opener: Some(opener),
            reported_unavailable: false,
        };
        clipboard.reopen();
        clipboard
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self {
            backend: Backend::memory(),
            opener: None,
            reported_unavailable: false,
        }
    }

    fn reopen(&mut self) -> bool {
        let Some(opener) = self.opener.as_mut() else {
            return false;
        };
        match opener() {
            Ok(mut clipboard) => {
                if let Backend::Memory {
                    clipboard: kept, ..
                } = &self.backend
                {
                    if !kept.is_empty() {
                        if let Err(e) = clipboard.set_text(kept) {
                            report!("failed to hand a copy over to the clipboard: {}", e);
                        }
                    }
                }
                self.backend = Backend::System(clipboard);
                self.reported_unavailable = false;
                true
            },
            Err(e) => {
                if !self.reported_unavailable {
                    report!(
                        "no system clipboard ({}); copy and paste stay inside this window \
                         until it can be reached",
                        e
                    );
                    self.reported_unavailable = true;
                }
                if matches!(self.backend, Backend::System(_)) {
                    self.backend = Backend::memory();
                }
                false
            },
        }
    }

    fn with_system<T>(
        &mut self,
        what: &str,
        mut f: impl FnMut(&mut dyn SystemClipboard) -> Result<T, arboard::Error>,
    ) -> Option<Result<T, arboard::Error>> {
        if let Backend::System(clipboard) = &mut self.backend {
            match f(clipboard.as_mut()) {
                Err(e) if is_connection_failure(&e) => {
                    report!(
                        "failed to {} the clipboard ({}); reconnecting to it",
                        what,
                        e
                    );
                },
                result => return Some(result),
            }
        }
        if !self.reopen() {
            return None;
        }
        match &mut self.backend {
            Backend::System(clipboard) => Some(f(clipboard.as_mut())),
            Backend::Memory { .. } => None,
        }
    }

    pub fn set(&mut self, text: &str) {
        match self.with_system("write", |clipboard| clipboard.set_text(text)) {
            Some(Ok(())) => {},
            Some(Err(e)) => report!("failed to write the clipboard: {}", e),
            None => {
                if let Backend::Memory { clipboard, .. } = &mut self.backend {
                    clipboard.clear();
                    clipboard.push_str(text);
                }
            },
        }
    }

    pub fn get(&mut self) -> Option<String> {
        match self.with_system("read", |clipboard| clipboard.get_text()) {
            Some(read) => reported(read, true),
            None => match &self.backend {
                Backend::Memory { clipboard, .. } => {
                    (!clipboard.is_empty()).then(|| clipboard.clone())
                },
                Backend::System(_) => None,
            },
        }
    }

    pub fn get_primary(&mut self) -> Option<String> {
        match &mut self.backend {
            Backend::System(clipboard) => clipboard.get_primary(),
            Backend::Memory { primary, .. } => (!primary.is_empty()).then(|| primary.clone()),
        }
    }

    pub fn set_primary(&mut self, text: &str) {
        match &mut self.backend {
            Backend::System(clipboard) => clipboard.set_primary(text),
            Backend::Memory { primary, .. } => {
                primary.clear();
                primary.push_str(text);
            },
        }
    }
}

fn is_connection_failure(error: &arboard::Error) -> bool {
    matches!(
        error,
        arboard::Error::ClipboardOccupied | arboard::Error::Unknown { .. }
    )
}

#[cfg(freedesktop)]
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

#[cfg(freedesktop)]
fn set_primary_text(clipboard: &mut arboard::Clipboard, text: &str) {
    use arboard::{LinuxClipboardKind, SetExtLinux};
    if let Err(e) = clipboard
        .set()
        .clipboard(LinuxClipboardKind::Primary)
        .text(text.to_owned())
    {
        report!("failed to write the primary selection: {}", e);
    }
}

#[cfg(not(freedesktop))]
fn set_primary_text(_clipboard: &mut arboard::Clipboard, _text: &str) {}

#[cfg(not(freedesktop))]
fn primary_text(_clipboard: &mut arboard::Clipboard) -> Option<String> {
    None
}

fn reported(read: Result<String, arboard::Error>, announce_empty: bool) -> Option<String> {
    match read {
        Ok(text) if !text.is_empty() => Some(text),
        Ok(_) => None,
        Err(arboard::Error::ContentNotAvailable) => {
            if announce_empty {
                report!("the clipboard holds no text; nothing to paste");
            }
            None
        },
        Err(e) => {
            report!("failed to read the clipboard: {}", e);
            None
        },
    }
}

pub fn osc52_reply(query_bytes: &[u8], clipboard: &ClipboardHandle) -> Option<Vec<u8>> {
    let (selection, terminator) = parse_osc52_query(query_bytes)?;
    let text = lock(clipboard).get().unwrap_or_default();

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
    paste_message_into(chars, None)
}

pub fn paste_message_into(
    chars: String,
    pane_id: Option<zellij_utils::data::PaneId>,
) -> ClientToServerMsg {
    ClientToServerMsg::Action {
        action: Action::Paste { chars, pane_id },
        terminal_id: None,
        client_id: None,
        is_cli_client: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::session::connection::{send, test_attach_at as attach_at, Capabilities, Geometry};
    use crate::testing::test_server::FakeServer;

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

    #[derive(Default)]
    struct FakeSystem {
        opens: usize,
        refused_opens: usize,
        lost_connections: usize,
        text: String,
    }

    struct FakeConnection {
        system: Arc<Mutex<FakeSystem>>,
        lost: bool,
    }

    impl SystemClipboard for FakeConnection {
        fn set_text(&mut self, text: &str) -> Result<(), arboard::Error> {
            if self.lost {
                return Err(arboard::Error::Unknown {
                    description: "the connection to the display was lost".to_owned(),
                });
            }
            self.system.lock().unwrap().text = text.to_owned();
            Ok(())
        }

        fn get_text(&mut self) -> Result<String, arboard::Error> {
            if self.lost {
                return Err(arboard::Error::Unknown {
                    description: "the connection to the display was lost".to_owned(),
                });
            }
            let text = self.system.lock().unwrap().text.clone();
            if text.is_empty() {
                Err(arboard::Error::ContentNotAvailable)
            } else {
                Ok(text)
            }
        }

        fn get_primary(&mut self) -> Option<String> {
            None
        }

        fn set_primary(&mut self, _text: &str) {}
    }

    fn fake_system(system: &Arc<Mutex<FakeSystem>>) -> Clipboard {
        let system = system.clone();
        Clipboard::open_with(Box::new(move || {
            let mut state = system.lock().unwrap();
            state.opens += 1;
            if state.refused_opens > 0 {
                state.refused_opens -= 1;
                return Err(arboard::Error::Unknown {
                    description: "no display".to_owned(),
                });
            }
            let lost = state.lost_connections > 0;
            if lost {
                state.lost_connections -= 1;
            }
            Ok(Box::new(FakeConnection {
                system: system.clone(),
                lost,
            }) as Box<dyn SystemClipboard>)
        }))
    }

    #[test]
    fn a_system_clipboard_that_was_unreachable_at_startup_is_used_once_it_can_be_reached() {
        let system = Arc::new(Mutex::new(FakeSystem {
            refused_opens: 2,
            ..FakeSystem::default()
        }));
        let mut clipboard = fake_system(&system);
        clipboard.set("made while unreachable");
        assert_eq!(system.lock().unwrap().text, "");

        assert_eq!(clipboard.get().as_deref(), Some("made while unreachable"));
        assert_eq!(
            system.lock().unwrap().text,
            "made while unreachable",
            "the copy kept in the window is handed over once the clipboard is reached"
        );

        clipboard.set("copied");
        assert_eq!(system.lock().unwrap().text, "copied");
        assert_eq!(system.lock().unwrap().opens, 3);
    }

    #[test]
    fn a_lost_connection_to_the_system_clipboard_is_reopened_and_the_copy_retried() {
        let system = Arc::new(Mutex::new(FakeSystem {
            lost_connections: 1,
            ..FakeSystem::default()
        }));
        let mut clipboard = fake_system(&system);
        clipboard.set("copied");
        assert_eq!(system.lock().unwrap().text, "copied");
        assert_eq!(system.lock().unwrap().opens, 2);

        clipboard.set("again");
        assert_eq!(system.lock().unwrap().text, "again");
        assert_eq!(
            system.lock().unwrap().opens,
            2,
            "a working connection is kept"
        );
    }

    #[test]
    fn a_copy_made_while_the_system_clipboard_cannot_be_reopened_is_kept_until_it_can() {
        let system = Arc::new(Mutex::new(FakeSystem {
            lost_connections: 1,
            ..FakeSystem::default()
        }));
        let mut clipboard = fake_system(&system);
        system.lock().unwrap().refused_opens = 1;
        clipboard.set("copied");
        assert_eq!(system.lock().unwrap().text, "");

        assert_eq!(clipboard.get().as_deref(), Some("copied"));
        assert_eq!(system.lock().unwrap().text, "copied");
    }

    #[test]
    fn an_empty_system_clipboard_is_not_mistaken_for_a_lost_connection() {
        let system = Arc::new(Mutex::new(FakeSystem::default()));
        let mut clipboard = fake_system(&system);
        assert_eq!(clipboard.get(), None);
        assert_eq!(system.lock().unwrap().opens, 1);
    }

    #[test]
    fn a_thread_that_crashed_while_copying_does_not_end_copying_for_everyone() {
        let clipboard = handle("before");
        let shared = clipboard.clone();
        let crashed = std::thread::spawn(move || {
            let _held = shared.lock().unwrap();
            panic!("a connection thread crashed");
        })
        .join();
        assert!(crashed.is_err());
        assert!(clipboard.is_poisoned());

        lock(&clipboard).set("after");
        assert_eq!(lock(&clipboard).get().as_deref(), Some("after"));
        assert!(!clipboard.is_poisoned());
        assert_eq!(
            osc52_reply(b"\x1b]52;c;?\x1b\\", &clipboard).unwrap(),
            b"\x1b]52;c;YWZ0ZXI=\x1b\\"
        );
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
