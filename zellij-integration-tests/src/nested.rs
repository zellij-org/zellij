use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, Sender};
use zellij_utils::nested_session::{decode_payload, NestedFrameExtractor, NestedSessionMessage};

use crate::fake_pty::FakePtyHandle;
use crate::runner::{TestRunner, TestSession};
use crate::Size;

pub const GUEST_PING_INTERVAL_MS: u64 = 100;
pub const GUEST_PONG_TIMEOUT_MS: u64 = 500;

#[derive(Clone, Default)]
pub struct FrameLog {
    inner: Arc<FrameLogInner>,
}

#[derive(Default)]
struct FrameLogInner {
    messages: Mutex<Vec<NestedSessionMessage>>,
    signal: Condvar,
}

impl FrameLog {
    fn push(&self, message: NestedSessionMessage) {
        self.inner.messages.lock().unwrap().push(message);
        self.inner.signal.notify_all();
    }

    pub fn count(&self, mut matcher: impl FnMut(&NestedSessionMessage) -> bool) -> usize {
        self.inner
            .messages
            .lock()
            .unwrap()
            .iter()
            .filter(|message| matcher(message))
            .count()
    }

    pub fn wait_for(&self, what: &str, mut matcher: impl FnMut(&NestedSessionMessage) -> bool) {
        let deadline = Instant::now() + crate::default_timeout();
        let mut messages = self.inner.messages.lock().unwrap();
        loop {
            if messages.iter().any(|message| matcher(message)) {
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "timed out waiting for nested frame: {}\nframes seen: {:?}\n=== zellij log tail ({}) ===\n{}",
                    what,
                    *messages,
                    crate::test_env::log_file_path().display(),
                    crate::test_env::log_tail(40),
                );
            }
            let (guard, _) = self
                .inner
                .signal
                .wait_timeout(messages, deadline - now)
                .unwrap();
            messages = guard;
        }
    }
}

pub struct NestedHarness {
    pub host: TestSession,
    pub guest: TestSession,
    pub host_pane: FakePtyHandle,
    guest_to_host: FrameLog,
    host_to_guest: FrameLog,
    frozen: Arc<AtomicBool>,
    _forwarders: Vec<JoinHandle<()>>,
}

impl NestedHarness {
    pub fn start(host_size: Size) -> Self {
        std::env::set_var(
            "ZELLIJ_NESTED_PING_INTERVAL_MS",
            GUEST_PING_INTERVAL_MS.to_string(),
        );
        std::env::set_var(
            "ZELLIJ_NESTED_PONG_TIMEOUT_MS",
            GUEST_PONG_TIMEOUT_MS.to_string(),
        );

        let host = TestRunner::new(host_size).start();
        host.wait_for_app_load();
        let host_pane = host.expect_pty_spawn();
        host_pane.disable_echo();
        let (guest_cols, guest_rows) =
            host_pane.wait_for_size("host guest-pane to be sized", |cols, rows| {
                cols > 0 && rows > 0
            });
        let host_session_name = host.session_name().to_string();

        let (guest_stdout_tx, guest_stdout_rx) = crossbeam::channel::unbounded();
        let guest = TestRunner::new(Size {
            cols: guest_cols as usize,
            rows: guest_rows as usize,
        })
        .as_nested_guest(&host_session_name)
        .with_stdout_tap(guest_stdout_tx)
        .skip_concurrency_slot()
        .start();

        let host_stdin_rx = host_pane.tap_stdin();
        let guest_stdin_tx = guest.stdin_sender();

        let guest_to_host = FrameLog::default();
        let host_to_guest = FrameLog::default();
        let frozen = Arc::new(AtomicBool::new(false));

        let forwarders = vec![
            spawn_guest_to_host(
                guest_stdout_rx,
                host_pane.clone(),
                frozen.clone(),
                guest_to_host.clone(),
            ),
            spawn_host_to_guest(host_stdin_rx, guest_stdin_tx, host_to_guest.clone()),
        ];

        NestedHarness {
            host,
            guest,
            host_pane,
            guest_to_host,
            host_to_guest,
            frozen,
            _forwarders: forwarders,
        }
    }

    pub fn guest_to_host(&self) -> &FrameLog {
        &self.guest_to_host
    }

    pub fn host_to_guest(&self) -> &FrameLog {
        &self.host_to_guest
    }

    pub fn wait_for_guest_to_announce(&self) {
        self.guest_to_host.wait_for(
            "the inner (guest) session to announce itself to the outer (host) session it is running inside",
            |message| matches!(message, NestedSessionMessage::Announce { .. }),
        );
    }

    pub fn wait_for_host_to_acknowledge_guest(&self) {
        self.host_to_guest.wait_for(
            "the outer (host) session to acknowledge the guest session running inside one of its panes",
            |message| matches!(message, NestedSessionMessage::AnnounceAck { .. }),
        );
    }

    pub fn wait_for_host_to_ping_guest(&self) {
        self.host_to_guest.wait_for(
            "the outer (host) session to ping the guest running inside its pane to check it is still alive",
            |message| matches!(message, NestedSessionMessage::Ping),
        );
    }

    pub fn wait_for_guest_to_reply_to_ping(&self) {
        self.guest_to_host.wait_for(
            "the inner (guest) session to answer the host's ping and prove it is still alive",
            |message| matches!(message, NestedSessionMessage::Pong),
        );
    }

    pub fn wait_for_host_to_release_guest_focus(&self) {
        self.host_to_guest.wait_for(
            "the outer (host) session to notice the guest is gone and stop routing keys into its pane",
            |message| matches!(message, NestedSessionMessage::FocusLost),
        );
    }

    pub fn freeze_guest(&self) {
        self.frozen.store(true, Ordering::SeqCst);
    }

    fn host_ping_count(&self) -> usize {
        self.host_to_guest
            .count(|message| matches!(message, NestedSessionMessage::Ping))
    }

    pub fn assert_host_stops_pinging_frozen_guest(&self) {
        thread::sleep(Duration::from_millis(
            GUEST_PONG_TIMEOUT_MS + 4 * GUEST_PING_INTERVAL_MS,
        ));
        let pings_after_timeout = self.host_ping_count();
        thread::sleep(Duration::from_millis(4 * GUEST_PING_INTERVAL_MS));
        let pings_later = self.host_ping_count();
        assert_eq!(
            pings_after_timeout, pings_later,
            "the outer host kept pinging the guest after it went silent; \
             the host never noticed the guest was gone and cleared it \
             (pings after timeout={pings_after_timeout}, later={pings_later})",
        );
    }
}

fn spawn_guest_to_host(
    guest_stdout_rx: Receiver<Vec<u8>>,
    host_pane: FakePtyHandle,
    frozen: Arc<AtomicBool>,
    log: FrameLog,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("nested_guest_to_host".to_string())
        .spawn(move || {
            let mut extractor = NestedFrameExtractor::new();
            while let Ok(bytes) = guest_stdout_rx.recv() {
                if frozen.load(Ordering::SeqCst) {
                    continue;
                }
                let (_residue, payloads) = extractor.extract(&bytes);
                for payload in payloads {
                    if let Some(message) = decode_payload(&payload) {
                        log.push(message);
                    }
                }
                host_pane.try_output(&bytes);
            }
        })
        .unwrap()
}

fn spawn_host_to_guest(
    host_stdin_rx: Receiver<Vec<u8>>,
    guest_stdin_tx: Sender<Vec<u8>>,
    log: FrameLog,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("nested_host_to_guest".to_string())
        .spawn(move || {
            let mut extractor = NestedFrameExtractor::new();
            while let Ok(bytes) = host_stdin_rx.recv() {
                let (_residue, payloads) = extractor.extract(&bytes);
                for payload in payloads {
                    if let Some(message) = decode_payload(&payload) {
                        log.push(message);
                    }
                }
                if guest_stdin_tx.send(bytes).is_err() {
                    break;
                }
            }
        })
        .unwrap()
}
