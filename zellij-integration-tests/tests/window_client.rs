#![cfg(unix)]

use std::collections::BTreeMap;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use zellij_client::{first_message, ClientInfo};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, Size, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliArgs;
use zellij_utils::consts::{ipc_connect, ZELLIJ_SOCK_DIR};
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
}

impl WindowClient {
    fn attach(zellij: &TestSession, size: Size) -> Self {
        let path = ZELLIJ_SOCK_DIR.join(zellij.session_name());
        let stream = ipc_connect(&path)
            .unwrap_or_else(|e| panic!("failed to connect to {:?}: {:?}", path, e));
        let mut sender = IpcSenderWithContext::new(stream);
        let mut receiver = sender.get_receiver::<ServerToClientMsg>();
        let handshake = first_message(
            &ClientInfo::Attach(zellij.session_name().to_owned(), Options::default()),
            &CliArgs::default(),
            &Options::default(),
            size,
            BTreeMap::new(),
            None,
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
        view.apply(&mut self.cells, self.size.cols, self.size.rows);
        let seq = view.header().seq;
        self.send(ClientToServerMsg::RenderFrameAck { seq });
        header
    }

    fn text(&self) -> String {
        self.cells
            .chunks(self.size.cols)
            .map(|row| row.iter().map(|cell| cell.character()).collect::<String>())
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
