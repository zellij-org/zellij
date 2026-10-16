#![cfg(unix)]

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use zellij_client::{first_message, ClientInfo};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, Size, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliArgs;
use zellij_utils::consts::ipc_connect;
use zellij_utils::data::{BareKey, KeyWithModifier};
use zellij_utils::input::actions::Action;
use zellij_utils::input::options::Options;
use zellij_utils::ipc::{ClientToServerMsg, ExitReason, IpcSenderWithContext, ServerToClientMsg};
use zellij_utils::structured_render::{self, WireCell};

const DEADLINE: Duration = Duration::from_secs(20);

struct WindowClient {
    sender: IpcSenderWithContext<ClientToServerMsg>,
    messages: mpsc::Receiver<ServerToClientMsg>,
    size: Size,
    cells: Vec<WireCell>,
    texts: Vec<String>,
    saw_cluster_record: bool,
    saw_group_continuation: bool,
}

impl WindowClient {
    fn attach(zellij: &TestSession, size: Size) -> Self {
        Self::attach_to(zellij.session_name(), size, None)
    }

    fn attach_to(session_name: &str, size: Size, pane_id: Option<(u32, bool)>) -> Self {
        let path = zellij_utils::consts::session_socket_path(session_name);
        let stream = ipc_connect(&path)
            .unwrap_or_else(|e| panic!("failed to connect to {:?}: {:?}", path, e));
        let mut sender = IpcSenderWithContext::new(stream);
        let mut receiver = sender.get_receiver::<ServerToClientMsg>();
        let handshake = first_message(
            &ClientInfo::Attach(session_name.to_owned(), Options::default()),
            &CliArgs::default(),
            &Options::default(),
            size,
            BTreeMap::new(),
            None,
            pane_id,
            None,
        );
        sender
            .send_client_msg(handshake.message)
            .expect("failed to send the attach message");
        sender
            .send_client_msg(ClientToServerMsg::StructuredRenderSupport { supported: true })
            .expect("failed to declare structured rendering");

        let (messages_tx, messages) = mpsc::channel();
        std::thread::Builder::new()
            .name("window_client_reader".to_owned())
            .spawn(move || {
                while let Some((msg, _)) = receiver.recv_server_msg() {
                    if messages_tx.send(msg).is_err() {
                        break;
                    }
                }
            })
            .unwrap();

        WindowClient {
            sender,
            messages,
            size,
            cells: vec![WireCell::BLANK; size.rows * size.cols],
            texts: vec![String::from(" "); size.rows * size.cols],
            saw_cluster_record: false,
            saw_group_continuation: false,
        }
    }

    fn send(&mut self, msg: ClientToServerMsg) {
        self.sender
            .send_client_msg(msg)
            .expect("failed to send a message to the session");
    }

    fn act(&mut self, action: Action) {
        self.send(ClientToServerMsg::Action {
            action,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }

    fn next(&self, deadline: Instant, what: &str) -> ServerToClientMsg {
        let remaining = deadline.saturating_duration_since(Instant::now());
        self.messages
            .recv_timeout(remaining)
            .unwrap_or_else(|e| panic!("waiting for {}: {:?}", what, e))
    }

    fn apply(&mut self, frame: &[u8]) -> (u16, u16) {
        let view = structured_render::decode(frame).expect("a frame must decode");
        let header = (view.header().cols, view.header().rows);
        if header == (self.size.cols as u16, self.size.rows as u16) {
            if view.header().full_repaint() || view.header().clear() {
                self.cells
                    .iter_mut()
                    .for_each(|cell| *cell = WireCell::BLANK);
                self.texts
                    .iter_mut()
                    .for_each(|text| *text = String::from(" "));
            }
            view.apply(&mut self.cells, self.size.cols, self.size.rows);
            self.resolve_texts(&view);
        }
        let seq = view.header().seq;
        self.send(ClientToServerMsg::RenderFrameAck { seq });
        header
    }

    fn resolve_texts(&mut self, view: &structured_render::FrameView<'_>) {
        let clusters = view.clusters();
        if !clusters.is_empty() {
            self.saw_cluster_record = true;
        }
        for (record, cells) in view.rows() {
            let start = record.y as usize * self.size.cols + record.x0 as usize;
            if start + cells.len() > self.texts.len() {
                continue;
            }
            for (offset, cell) in cells.iter().enumerate() {
                self.texts[start + offset] = cell.text(&clusters).into_owned();
                if cell.is_group_continuation() {
                    self.saw_group_continuation = true;
                }
            }
        }
    }

    fn row_text(&self, row: usize) -> String {
        let start = row * self.size.cols;
        let mut text = String::new();
        let mut column = 0;
        while column < self.size.cols {
            text.push_str(&self.texts[start + column]);
            column += (self.cells[start + column].width as usize).max(1);
        }
        text
    }

    fn text(&self) -> String {
        (0..self.size.rows)
            .map(|row| self.row_text(row))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn frame_showing(&mut self, needle: &str) -> (u16, u16) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match self.next(deadline, needle) {
                ServerToClientMsg::RenderFrame { frame } => {
                    let header = self.apply(&frame);
                    if self.text().contains(needle) {
                        return header;
                    }
                },
                ServerToClientMsg::Exit { exit_reason } => {
                    panic!(
                        "the session let the window go while waiting for {:?}: {:?}",
                        needle, exit_reason
                    )
                },
                _ => {},
            }
        }
    }

    fn exit_reason(&mut self) -> ExitReason {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match self.next(deadline, "the session to let the window go") {
                ServerToClientMsg::RenderFrame { frame } => {
                    self.apply(&frame);
                },
                ServerToClientMsg::Exit { exit_reason } => return exit_reason,
                _ => {},
            }
        }
    }
}

fn leave(mut window: WindowClient, mut zellij: TestSession) {
    window.act(Action::Detach);
    window.exit_reason();
    zellij.quit();
}

fn smaller_than_the_terminal() -> Size {
    Size {
        rows: TERMINAL_SIZE.rows - 4,
        cols: TERMINAL_SIZE.cols - 10,
    }
}

#[test]
fn a_window_attaching_to_a_session_a_terminal_is_using_is_drawn_at_its_own_size() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let size = smaller_than_the_terminal();
    let mut window = WindowClient::attach(&zellij, size);
    terminal.output(b"shared by both clients\r\n");

    let (cols, rows) = window.frame_showing("shared by both clients");
    assert_eq!(
        (cols as usize, rows as usize),
        (size.cols, size.rows),
        "the window's frames are built for the window, not for the terminal already attached"
    );
    zellij.wait_until("the terminal client to keep drawing", |grid| {
        grid.contains("shared by both clients")
    });

    terminal.output(b"still both\r\n");
    window.frame_showing("still both");
    zellij.wait_until("the terminal client to keep drawing", |grid| {
        grid.contains("still both")
    });

    leave(window, zellij);
}

#[test]
fn text_a_window_types_after_a_dead_key_reaches_the_pane() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, smaller_than_the_terminal());
    terminal.output(b"ready\r\n");
    window.frame_showing("ready");

    window.send(ClientToServerMsg::Key {
        key: KeyWithModifier::new(BareKey::Char('^')),
        raw_bytes: b"^".to_vec(),
        is_kitty_keyboard_protocol: false,
    });
    terminal.wait_for_stdin("an accent typed on its own to reach the pane", |stdin| {
        stdin.contains(&b'^')
    });

    window.act(Action::WriteChars {
        chars: "^x".to_owned(),
    });
    let stdin = terminal.wait_for_stdin(
        "an accent that could not combine to reach the pane with its letter",
        |stdin| stdin.windows(2).any(|pair| pair == b"^x"),
    );
    assert!(
        !stdin.windows(6).any(|w| w == b"\x1b[200~"),
        "typed text must not arrive as a paste, got {:?}",
        String::from_utf8_lossy(&stdin)
    );

    leave(window, zellij);
}

#[test]
fn a_window_that_detaches_leaves_the_terminal_client_working() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, smaller_than_the_terminal());
    terminal.output(b"before the window left\r\n");
    window.frame_showing("before the window left");

    window.act(Action::Detach);
    let reason = window.exit_reason();
    assert!(
        matches!(reason, ExitReason::Normal | ExitReason::NormalDetached),
        "a detach the window asked for is a normal ending, which closes it quietly, got {:?}",
        reason
    );

    terminal.output(b"after the window left\r\n");
    zellij.wait_until("the terminal client to keep drawing", |grid| {
        grid.contains("after the window left")
    });

    zellij.quit();
}

#[test]
fn a_window_keeps_the_session_after_the_terminal_client_detaches() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, smaller_than_the_terminal());
    terminal.output(b"before the terminal left\r\n");
    window.frame_showing("before the terminal left");

    zellij.send_stdin(&zellij_integration_tests::keys::ctrl('o'));
    zellij.send_stdin(&zellij_integration_tests::keys::key('d'));
    zellij.wait_for_main_client_to_exit();

    terminal.output(b"after the terminal left\r\n");
    window.frame_showing("after the terminal left");

    window.act(Action::Quit);
    assert_eq!(
        window.exit_reason(),
        ExitReason::Normal,
        "quitting is a normal ending, which closes the window quietly"
    );
    zellij.quit();
}

#[test]
fn a_window_is_told_the_session_ended_when_it_is_killed() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, smaller_than_the_terminal());
    terminal.output(b"about to be killed\r\n");
    window.frame_showing("about to be killed");

    zellij.kill_session();
    assert_eq!(
        window.exit_reason(),
        ExitReason::Normal,
        "a killed session ends normally for every client, so the window closes quietly"
    );
}

const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
const FLAG: &str = "\u{1f1e9}\u{1f1ea}";
const THUMBS: &str = "\u{1f44d}\u{1f3fd}";
const KSSA: &str = "\u{915}\u{94d}\u{937}";

fn cluster_line(prefix: &str) -> String {
    format!(
        "{}{}|e\u{301}|{}|\u{4e16}|{}|{}|END",
        prefix, FAMILY, FLAG, KSSA, THUMBS
    )
}

#[test]
fn a_window_receives_clusters_and_groups_and_reads_the_same_text_as_a_terminal() {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("grapheme_clusters false")
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, TERMINAL_SIZE);
    let line = cluster_line("W:");
    terminal.output(format!("\r\n{}\r\n", line).as_bytes());

    window.frame_showing(&line);
    assert!(
        window.saw_cluster_record,
        "a frame showing cells with attached code points must carry a cluster record"
    );
    assert!(
        window.saw_group_continuation,
        "a frame showing default-mode groups must mark their continuation cells"
    );

    let grid_snapshot = zellij.wait_until("the terminal client to show the clusters", |grid| {
        grid.contains(&line)
    });
    let terminal_line = grid_snapshot
        .lines()
        .into_iter()
        .find(|row| row.contains(&line))
        .expect("the terminal client shows the cluster line");
    let window_line = window
        .text()
        .lines()
        .find(|row| row.contains(&line))
        .map(|row| row.to_owned())
        .expect("the window shows the cluster line");
    assert_eq!(
        window_line.trim_end(),
        terminal_line.trim_end(),
        "the window and the terminal client must read the same text"
    );

    leave(window, zellij);
}

#[test]
fn a_window_receives_emoji_sequences_as_single_clusters_by_default() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    let mut window = WindowClient::attach(&zellij, TERMINAL_SIZE);
    let line = cluster_line("D:");
    terminal.output(format!("\r\n{}\r\n", line).as_bytes());

    window.frame_showing(&line);
    assert!(
        window.saw_cluster_record,
        "a frame showing emoji sequences must carry a cluster record"
    );
    assert!(
        !window.saw_group_continuation,
        "with clusters counted as one unit, no cell continues a group"
    );

    leave(window, zellij);
}

fn key(bare_key: BareKey) -> ClientToServerMsg {
    let key = KeyWithModifier::new(bare_key);
    let raw_bytes = key
        .serialize_non_kitty()
        .map(String::into_bytes)
        .unwrap_or_default();
    ClientToServerMsg::Key {
        key,
        raw_bytes,
        is_kitty_keyboard_protocol: false,
    }
}

#[test]
fn keys_a_window_sends_to_the_inbox_stay_out_of_the_pane_below() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_indicator true\nmouse_mode true")
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    for i in 0..3 {
        terminal.output(
            format!(
                "\u{1b}]7501;state=blocked:kind=question:msg={}\u{1b}\\",
                ["cTA=", "cTE=", "cTI="][i]
            )
            .as_bytes(),
        );
    }
    let pane = terminal.terminal_id();
    let mut window = WindowClient::attach_to(
        background.session_name(),
        smaller_than_the_terminal(),
        Some((pane, false)),
    );
    window.frame_showing("need you");
    let before = terminal.stdin_bytes().len();
    let top_row = window.text().lines().next().unwrap_or_default().to_owned();
    let column = top_row[..top_row.rfind('●').unwrap()].chars().count();
    let at = zellij_utils::position::Position::new(0, column as u16);
    let mut press = zellij_utils::input::mouse::MouseEvent::new_left_press_event(at);
    window.act(Action::MouseEvent { event: press });
    press.event_type = zellij_utils::input::mouse::MouseEventType::Release;
    press.left = false;
    window.act(Action::MouseEvent { event: press });
    window.frame_showing("Needs you (");
    for _ in 0..4 {
        window.send(key(BareKey::Down));
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(500));
    let leaked = terminal.stdin_bytes()[before..].to_vec();
    assert!(
        leaked.is_empty(),
        "keys reached the pane under the inbox: {:?}",
        String::from_utf8_lossy(&leaked)
    );
    let size = smaller_than_the_terminal();
    for (line, col) in [
        (size.rows / 2, size.cols / 2),
        (size.rows / 3, size.cols / 4),
        (size.rows / 2, size.cols - 20),
    ] {
        for _ in 0..3 {
            let mut wheel = zellij_utils::input::mouse::MouseEvent::new();
            wheel.event_type = zellij_utils::input::mouse::MouseEventType::Press;
            wheel.position = zellij_utils::position::Position::new(line as i32, col as u16);
            wheel.wheel_down = true;
            wheel.wheel_lines = 3;
            window.act(Action::MouseEvent { event: wheel });
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    std::thread::sleep(Duration::from_millis(500));
    let leaked = terminal.stdin_bytes()[before..].to_vec();
    assert!(
        leaked.is_empty(),
        "the wheel reached the pane under the inbox: {:?}\n{}",
        String::from_utf8_lossy(&leaked),
        window.text()
    );
}

#[test]
fn text_and_pastes_a_window_sends_while_the_inbox_is_open_go_to_the_inbox_not_the_pane() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_indicator true\nmouse_mode true")
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(b"\x1b]7501;state=blocked:kind=question:msg=cTA=\x1b\\");
    let pane = terminal.terminal_id();
    let mut window = WindowClient::attach_to(
        background.session_name(),
        smaller_than_the_terminal(),
        Some((pane, false)),
    );
    window.frame_showing("need you");
    let before = terminal.stdin_bytes().len();
    let top_row = window.text().lines().next().unwrap_or_default().to_owned();
    let column = top_row[..top_row.rfind('●').unwrap()].chars().count();
    let at = zellij_utils::position::Position::new(0, column as u16);
    let mut press = zellij_utils::input::mouse::MouseEvent::new_left_press_event(at);
    window.act(Action::MouseEvent { event: press });
    press.event_type = zellij_utils::input::mouse::MouseEventType::Release;
    press.left = false;
    window.act(Action::MouseEvent { event: press });
    window.frame_showing("Needs you (1)");
    window.act(Action::WriteChars {
        chars: "c".to_owned(),
    });
    window.frame_showing("Needs you (0)");
    window.act(Action::Paste {
        chars: "pasted".to_owned(),
        pane_id: None,
    });
    window.act(Action::WriteChars {
        chars: "xy".to_owned(),
    });
    std::thread::sleep(Duration::from_millis(500));
    let leaked = terminal.stdin_bytes()[before..].to_vec();
    assert!(
        leaked.is_empty(),
        "text meant for the inbox reached the pane: {:?}",
        String::from_utf8_lossy(&leaked)
    );
}
