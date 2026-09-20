use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, Sender};
use zellij_utils::nested_session::{decode_payload, NestedFrameExtractor, NestedSessionMessage};

use crate::client_screen::GridSnapshot;
use crate::fake_pty::FakePtyHandle;
use crate::runner::{GuestResizer, TestRunner, TestSession};
use crate::Size;

pub fn host_chrome_undimmed_after_guest_exit(host_grid: &GridSnapshot) -> bool {
    !host_grid.line_has_dim("Pane #2") && host_grid.char_dim_of("test-").map_or(true, |dim| !dim)
}

pub fn composite_contains_settled_guest_grid(
    host_grid: &GridSnapshot,
    guest_grid: &GridSnapshot,
) -> bool {
    let host_lines: Vec<Vec<char>> = host_grid
        .text
        .lines()
        .map(|line| line.chars().collect())
        .collect();
    let guest_lines: Vec<Vec<char>> = guest_grid
        .text
        .lines()
        .map(|line| line.chars().collect())
        .collect();
    let guest_rows = guest_lines.len();
    let guest_cols = guest_lines.iter().map(|line| line.len()).max().unwrap_or(0);
    let host_rows = host_lines.len();
    let host_cols = host_lines.iter().map(|line| line.len()).max().unwrap_or(0);
    if guest_rows == 0 || guest_cols == 0 || guest_rows > host_rows || guest_cols > host_cols {
        return false;
    }
    for row_offset in 0..=(host_rows - guest_rows) {
        'next_column_offset: for col_offset in 0..=(host_cols - guest_cols) {
            for (guest_row, guest_line) in guest_lines.iter().enumerate() {
                let host_line = &host_lines[row_offset + guest_row];
                for (guest_col, guest_char) in guest_line.iter().enumerate() {
                    let guest_cell_is_cursor = guest_grid.cursor.map_or(false, |cursor| {
                        cursor.x == guest_col && cursor.y == guest_row
                    });
                    if guest_cell_is_cursor {
                        continue;
                    }
                    let host_char = host_line
                        .get(col_offset + guest_col)
                        .copied()
                        .unwrap_or(' ');
                    if host_char != *guest_char {
                        continue 'next_column_offset;
                    }
                }
            }
            return true;
        }
    }
    false
}

pub fn host_descended_bar_settled(host_grid: &GridSnapshot) -> bool {
    host_grid
        .lines()
        .last()
        .map_or(false, |last_line| last_line.contains("Ascend:"))
}

pub fn host_descended_bar_with_ascend_keys_settled(host_grid: &GridSnapshot) -> bool {
    host_grid.lines().last().map_or(false, |last_line| {
        last_line.contains("Ascend:") && !last_line.contains("<unbound>")
    })
}

pub fn guest_ascended_bar_settled(guest_grid: &GridSnapshot) -> bool {
    guest_grid
        .lines()
        .last()
        .map_or(false, |last_line| last_line.contains("Descend:"))
}

pub fn wait_for_settled_composite(
    host: &TestSession,
    guest: &TestSession,
    what: &str,
    guest_settled: impl Fn(&GridSnapshot) -> bool,
    host_settled: impl Fn(&GridSnapshot) -> bool,
) -> GridSnapshot {
    host.wait_until(what, |host_grid| {
        let guest_grid = guest.snapshot();
        guest_settled(&guest_grid)
            && host_settled(host_grid)
            && composite_contains_settled_guest_grid(host_grid, &guest_grid)
    })
}

pub const GUEST_PING_INTERVAL_MS: u64 = 100;
pub const GUEST_PONG_TIMEOUT_MS: u64 = 4000;

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

    pub fn mark(&self) -> usize {
        self.inner.messages.lock().unwrap().len()
    }

    pub fn wait_for(&self, what: &str, matcher: impl FnMut(&NestedSessionMessage) -> bool) {
        self.wait_for_after(0, what, matcher);
    }

    pub fn wait_for_after(
        &self,
        since: usize,
        what: &str,
        mut matcher: impl FnMut(&NestedSessionMessage) -> bool,
    ) {
        let deadline = Instant::now() + crate::default_timeout();
        let mut messages = self.inner.messages.lock().unwrap();
        loop {
            if messages.iter().skip(since).any(|message| matcher(message)) {
                return;
            }
            let now = Instant::now();
            if now >= deadline {
                panic!(
                    "timed out waiting for nested frame: {} (after frame index {})\nframes seen: {:?}\n=== zellij log tail ({}) ===\n{}",
                    what,
                    since,
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

fn set_nested_timing_env() {
    std::env::set_var(
        "ZELLIJ_NESTED_PING_INTERVAL_MS",
        GUEST_PING_INTERVAL_MS.to_string(),
    );
    std::env::set_var(
        "ZELLIJ_NESTED_PONG_TIMEOUT_MS",
        GUEST_PONG_TIMEOUT_MS.to_string(),
    );
}

struct GuestBridge {
    guest: TestSession,
    guest_to_host: FrameLog,
    host_to_guest: FrameLog,
    forwarders: Vec<JoinHandle<()>>,
}

fn bridge_guest_into_pane_for_host(
    host_pane: &FakePtyHandle,
    frozen: Arc<AtomicBool>,
) -> GuestBridge {
    bridge_guest_into_pane_for_host_with_config(host_pane, frozen, "")
}

fn bridge_guest_into_pane_for_host_with_config(
    host_pane: &FakePtyHandle,
    frozen: Arc<AtomicBool>,
    guest_config_kdl: &str,
) -> GuestBridge {
    host_pane.disable_echo();
    let (guest_cols, guest_rows) = host_pane
        .wait_for_size("host guest-pane to be sized", |cols, rows| {
            cols > 0 && rows > 0
        });

    let (guest_stdout_tx, guest_stdout_rx) = crossbeam::channel::unbounded();
    let guest = TestRunner::new(Size {
        cols: guest_cols as usize,
        rows: guest_rows as usize,
    })
    .with_config(guest_config_kdl)
    .with_stdout_tap(guest_stdout_tx)
    .skip_concurrency_slot()
    .start();

    let host_stdin_rx = host_pane.tap_stdin();
    let guest_stdin_tx = guest.stdin_sender();
    let guest_resizer = guest.resize_sender();

    let guest_to_host = FrameLog::default();
    let host_to_guest = FrameLog::default();

    let forwarders = vec![
        spawn_guest_to_host(
            guest_stdout_rx,
            host_pane.clone(),
            frozen,
            guest_to_host.clone(),
        ),
        spawn_host_to_guest(host_stdin_rx, guest_stdin_tx, host_to_guest.clone()),
        spawn_pane_resize_to_guest(host_pane.clone(), guest_resizer, guest_cols, guest_rows),
    ];

    GuestBridge {
        guest,
        guest_to_host,
        host_to_guest,
        forwarders,
    }
}

impl NestedHarness {
    pub fn start(host_size: Size) -> Self {
        Self::start_with_host_config(host_size, "")
    }

    pub fn start_with_host_config(host_size: Size, host_config_kdl: &str) -> Self {
        Self::start_with_host_and_guest_config(host_size, host_config_kdl, "")
    }

    pub fn start_with_host_and_guest_config(
        host_size: Size,
        host_config_kdl: &str,
        guest_config_kdl: &str,
    ) -> Self {
        set_nested_timing_env();

        let host = TestRunner::new(host_size)
            .with_config(host_config_kdl)
            .start();
        host.wait_for_app_load();
        let host_pane = host.expect_pty_spawn();

        let frozen = Arc::new(AtomicBool::new(false));
        let bridge = bridge_guest_into_pane_for_host_with_config(
            &host_pane,
            frozen.clone(),
            guest_config_kdl,
        );

        NestedHarness {
            host,
            guest: bridge.guest,
            host_pane,
            guest_to_host: bridge.guest_to_host,
            host_to_guest: bridge.host_to_guest,
            frozen,
            _forwarders: bridge.forwarders,
        }
    }

    pub fn guest_to_host(&self) -> &FrameLog {
        &self.guest_to_host
    }

    pub fn host_to_guest(&self) -> &FrameLog {
        &self.host_to_guest
    }

    pub fn mark_host_to_guest(&self) -> usize {
        self.host_to_guest.mark()
    }

    pub fn mark_guest_to_host(&self) -> usize {
        self.guest_to_host.mark()
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
        self.wait_for_host_to_reclaim_focus_after_guest_exit();
    }

    pub fn wait_for_host_to_reclaim_focus_after_guest_exit(&self) {
        self.host.wait_until(
            "the outer (host) session to notice the guest is gone and reclaim focus (undim its chrome)",
            host_chrome_undimmed_after_guest_exit,
        );
    }

    pub fn wait_for_host_to_descend_into_guest(&self) {
        self.wait_for_host_to_descend_into_guest_after(0);
    }

    pub fn wait_for_host_to_descend_into_guest_after(&self, since: usize) {
        self.host_to_guest.wait_for_after(
            since,
            "the outer (host) session to descend into the guest pane and route the client's keys down into it",
            |message| matches!(message, NestedSessionMessage::FocusGained { .. }),
        );
    }

    pub fn descend_into_guest_via_modal(&self) {
        let descended = self.mark_host_to_guest();
        self.host.send_stdin(b"2");
        self.wait_for_host_to_descend_into_guest_after(descended);
    }

    pub fn zoom_into_guest_via_modal(&self) {
        let descended = self.mark_host_to_guest();
        self.host.send_stdin(b"1");
        self.wait_for_host_to_descend_into_guest_after(descended);
    }

    pub fn wait_for_host_to_ascend_from_guest(&self) {
        self.wait_for_host_to_ascend_from_guest_after(0);
    }

    pub fn wait_for_host_to_ascend_from_guest_after(&self, since: usize) {
        self.host_to_guest.wait_for_after(
            since,
            "the outer (host) session to ascend back out of the guest pane and take its client's keys back",
            |message| matches!(message, NestedSessionMessage::FocusLost),
        );
    }

    pub fn wait_for_guest_to_request_host_focus(&self) {
        self.wait_for_guest_to_request_host_focus_after(0);
    }

    pub fn wait_for_guest_to_request_host_focus_after(&self, since: usize) {
        self.guest_to_host.wait_for_after(
            since,
            "the inner (guest) session to ask its host to take focus back (the FocusHostSession binding)",
            |message| matches!(message, NestedSessionMessage::FocusHost { .. }),
        );
    }

    pub fn wait_until_host_composites_settled_guest(
        &self,
        what: &str,
        guest_settled: impl Fn(&GridSnapshot) -> bool,
        host_settled: impl Fn(&GridSnapshot) -> bool,
    ) -> GridSnapshot {
        wait_for_settled_composite(&self.host, &self.guest, what, guest_settled, host_settled)
    }

    pub fn focus_gained_count(&self) -> usize {
        self.host_to_guest
            .count(|message| matches!(message, NestedSessionMessage::FocusGained { .. }))
    }

    pub fn focus_lost_count(&self) -> usize {
        self.host_to_guest
            .count(|message| matches!(message, NestedSessionMessage::FocusLost))
    }

    pub fn freeze_guest(&self) {
        self.frozen.store(true, Ordering::SeqCst);
    }

    fn host_ping_count(&self) -> usize {
        self.host_to_guest
            .count(|message| matches!(message, NestedSessionMessage::Ping))
    }

    pub fn assert_host_stops_pinging_frozen_guest(&self) {
        let stable_window = Duration::from_millis(6 * GUEST_PING_INTERVAL_MS);
        let deadline = Instant::now()
            + Duration::from_millis(GUEST_PONG_TIMEOUT_MS)
            + crate::default_timeout();
        loop {
            let pings_before = self.host_ping_count();
            thread::sleep(stable_window);
            let pings_after = self.host_ping_count();
            if pings_before == pings_after {
                return;
            }
            if Instant::now() >= deadline {
                panic!(
                    "the outer host kept pinging the guest after it went silent; \
                     the host never noticed the guest was gone and cleared it \
                     (pings before={pings_before}, after={pings_after})",
                );
            }
        }
    }
}

pub struct NestedDepthThreeHarness {
    pub outer: TestSession,
    pub middle: TestSession,
    pub inner: TestSession,
    pub middle_pane: FakePtyHandle,
    pub inner_pane: FakePtyHandle,
    outer_to_middle: FrameLog,
    middle_to_inner: FrameLog,
    inner_to_middle: FrameLog,
    _frozen: Arc<AtomicBool>,
    _forwarders: Vec<JoinHandle<()>>,
}

impl NestedDepthThreeHarness {
    pub fn start_depth_three(outer_size: Size) -> Self {
        set_nested_timing_env();

        let outer = TestRunner::new(outer_size).start();
        outer.wait_for_app_load();
        let middle_pane = outer.expect_pty_spawn();

        let frozen = Arc::new(AtomicBool::new(false));
        let outer_to_middle_bridge = bridge_guest_into_pane_for_host(&middle_pane, frozen.clone());
        let middle = outer_to_middle_bridge.guest;
        let outer_to_middle = outer_to_middle_bridge.host_to_guest;

        middle.wait_for_app_load();
        let inner_pane = middle.expect_pty_spawn();

        let middle_to_inner_bridge = bridge_guest_into_pane_for_host(&inner_pane, frozen.clone());
        let inner = middle_to_inner_bridge.guest;
        let middle_to_inner = middle_to_inner_bridge.host_to_guest;
        let inner_to_middle = middle_to_inner_bridge.guest_to_host;

        let mut forwarders = outer_to_middle_bridge.forwarders;
        forwarders.extend(middle_to_inner_bridge.forwarders);

        NestedDepthThreeHarness {
            outer,
            middle,
            inner,
            middle_pane,
            inner_pane,
            outer_to_middle,
            middle_to_inner,
            inner_to_middle,
            _frozen: frozen,
            _forwarders: forwarders,
        }
    }

    pub fn outer_to_middle_frames(&self) -> &FrameLog {
        &self.outer_to_middle
    }

    pub fn middle_to_inner_frames(&self) -> &FrameLog {
        &self.middle_to_inner
    }

    pub fn inner_to_middle_frames(&self) -> &FrameLog {
        &self.inner_to_middle
    }

    pub fn wait_for_outer_to_descend_into_middle(&self) {
        self.outer_to_middle.wait_for(
            "the outer host to descend into the middle guest pane",
            |message| matches!(message, NestedSessionMessage::FocusGained { .. }),
        );
    }

    pub fn wait_for_middle_to_descend_into_inner(&self) {
        self.middle_to_inner.wait_for(
            "the middle guest to descend into the inner guest pane",
            |message| matches!(message, NestedSessionMessage::FocusGained { .. }),
        );
    }

    pub fn descend_outer_into_middle_via_modal(&self) {
        let descended = self.mark_outer_to_middle();
        self.outer.send_stdin(b"2");
        self.wait_for_outer_to_descend_into_middle_after(descended);
    }

    pub fn descend_middle_into_inner_via_modal(&self) {
        self.middle_to_inner.wait_for(
            "the inner guest to announce itself to the middle host before descending",
            |message| matches!(message, NestedSessionMessage::AnnounceAck { .. }),
        );
        let descended = self.mark_middle_to_inner();
        self.outer.send_stdin(b"2");
        self.wait_for_middle_to_descend_into_inner_after(descended);
    }

    pub fn zoom_middle_into_inner_via_modal(&self) {
        self.middle_to_inner.wait_for(
            "the inner guest to announce itself to the middle host before zooming",
            |message| matches!(message, NestedSessionMessage::AnnounceAck { .. }),
        );
        let descended = self.mark_middle_to_inner();
        self.outer.send_stdin(b"1");
        self.wait_for_middle_to_descend_into_inner_after(descended);
    }

    pub fn boot_and_descend_depth_three(&self) {
        self.descend_outer_into_middle_via_modal();
        self.middle.wait_for_app_load();
        self.descend_middle_into_inner_via_modal();
    }

    pub fn wait_for_outer_to_descend_into_middle_after(&self, since: usize) {
        self.outer_to_middle.wait_for_after(
            since,
            "the outer host to descend back into the middle guest pane",
            |message| matches!(message, NestedSessionMessage::FocusGained { .. }),
        );
    }

    pub fn wait_for_middle_to_descend_into_inner_after(&self, since: usize) {
        self.middle_to_inner.wait_for_after(
            since,
            "the middle guest to descend back into the inner guest pane",
            |message| matches!(message, NestedSessionMessage::FocusGained { .. }),
        );
    }

    pub fn mark_outer_to_middle(&self) -> usize {
        self.outer_to_middle.mark()
    }

    pub fn mark_middle_to_inner(&self) -> usize {
        self.middle_to_inner.mark()
    }

    pub fn mark_inner_to_middle(&self) -> usize {
        self.inner_to_middle.mark()
    }

    pub fn wait_for_outer_to_ascend_from_middle_after(&self, since: usize) {
        self.outer_to_middle.wait_for_after(
            since,
            "the outer host to ascend back out of the middle guest pane",
            |message| matches!(message, NestedSessionMessage::FocusLost),
        );
    }

    pub fn wait_for_outer_to_reclaim_focus_after_middle_exit(&self) {
        self.outer.wait_until(
            "the outer host to notice the middle guest is gone and reclaim focus (undim its chrome)",
            host_chrome_undimmed_after_guest_exit,
        );
    }

    pub fn wait_for_middle_to_ascend_from_inner_after(&self, since: usize) {
        self.middle_to_inner.wait_for_after(
            since,
            "the middle guest to ascend back out of the inner guest pane",
            |message| matches!(message, NestedSessionMessage::FocusLost),
        );
    }

    pub fn wait_for_inner_to_request_host_focus_after(&self, since: usize) {
        self.inner_to_middle.wait_for_after(
            since,
            "the inner guest to ask its middle host to take focus back (the FocusHostSession binding)",
            |message| matches!(message, NestedSessionMessage::FocusHost { .. }),
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

fn spawn_pane_resize_to_guest(
    host_pane: FakePtyHandle,
    guest_resizer: GuestResizer,
    initial_cols: u16,
    initial_rows: u16,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("nested_pane_resize_to_guest".to_string())
        .spawn(move || {
            let mut last_seen = (initial_cols, initial_rows);
            while let Some((cols, rows)) = host_pane.wait_for_size_change(last_seen) {
                last_seen = (cols, rows);
                let resized = guest_resizer.resize(Size {
                    cols: cols as usize,
                    rows: rows as usize,
                });
                if !resized {
                    break;
                }
            }
        })
        .unwrap()
}
