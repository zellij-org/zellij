#![cfg(unix)]

use std::collections::BTreeMap;
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use zellij_client::{first_message, ClientInfo};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, FakePtyHandle, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliArgs;
use zellij_utils::consts::ipc_connect;
use zellij_utils::input::actions::Action;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::input::options::Options;
use zellij_utils::ipc::{ClientToServerMsg, IpcSenderWithContext, ServerToClientMsg};
use zellij_utils::position::Position;

const CONFIG: &str = concat!(
    "mouse_mode true\n",
    "copy_on_select false\n",
    "copy_clipboard \"system\"\n",
);

const LINE: &str = "copy me please";
const OSC52_START: &[u8] = b"\x1b]52;c;";

struct Window {
    sender: IpcSenderWithContext<ClientToServerMsg>,
}

impl Window {
    fn attach(zellij: &TestSession) -> Self {
        let session_name = zellij.session_name();
        let path = zellij_utils::consts::session_socket_path(session_name);
        let stream = ipc_connect(&path)
            .unwrap_or_else(|e| panic!("failed to connect to {:?}: {:?}", path, e));
        let mut sender = IpcSenderWithContext::new(stream);
        let mut receiver = sender.get_receiver::<ServerToClientMsg>();
        let handshake = first_message(
            &ClientInfo::Attach(session_name.to_owned(), Options::default()),
            &CliArgs::default(),
            &Options::default(),
            TERMINAL_SIZE,
            BTreeMap::new(),
            None,
            None,
            None,
        );
        sender
            .send_client_msg(handshake.message)
            .expect("failed to send the attach message");
        std::thread::Builder::new()
            .name("copy_key_window_reader".to_owned())
            .spawn(move || while receiver.recv_server_msg().is_some() {})
            .unwrap();
        Window { sender }
    }

    fn send(&mut self, msg: ClientToServerMsg) {
        self.sender
            .send_client_msg(msg)
            .expect("failed to send a message to the session");
    }

    fn mouse(&mut self, event: MouseEvent) {
        self.send(ClientToServerMsg::Action {
            action: Action::MouseEvent { event },
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }

    fn drag(&mut self, line: usize, from: usize, to: usize) {
        let start = Position::new(line as i32, from as u16);
        let end = Position::new(line as i32, to as u16);
        self.mouse(MouseEvent::new_left_press_event(start));
        self.mouse(MouseEvent::new_left_motion_event(end));
        self.mouse(MouseEvent::new_left_release_event(end));
    }

    fn copy_key(&mut self) {
        self.send(ClientToServerMsg::Action {
            action: Action::Copy,
            terminal_id: None,
            client_id: None,
            is_cli_client: false,
        });
    }
}

fn start() -> (TestSession, FakePtyHandle, (usize, usize)) {
    let zellij = TestRunner::new(TERMINAL_SIZE).with_config(CONFIG).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    terminal.output(format!("{}\r\n", LINE).as_bytes());
    let snapshot = zellij.wait_until("the line to copy is drawn", |grid| grid.contains(LINE));
    let lines = snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains(LINE))
        .expect("the line is on screen");
    let column = lines[row][..lines[row].find(LINE).unwrap()].chars().count();
    (zellij, terminal, (row, column))
}

fn copies(bytes: &[u8]) -> Vec<String> {
    let mut found = vec![];
    let mut rest = bytes;
    while let Some(start) = rest
        .windows(OSC52_START.len())
        .position(|window| window == OSC52_START)
    {
        let body = &rest[start + OSC52_START.len()..];
        let end = body
            .iter()
            .position(|byte| *byte == 0x1b || *byte == 0x07)
            .unwrap_or(body.len());
        if let Ok(decoded) = BASE64_STANDARD.decode(&body[..end]) {
            found.push(String::from_utf8_lossy(&decoded).into_owned());
        }
        rest = &body[end..];
    }
    found
}

#[test]
fn the_copy_key_copies_a_selection_when_copy_on_select_is_off() {
    let (mut zellij, terminal, (row, column)) = start();
    let mut window = Window::attach(&zellij);
    let stdin_before = terminal.stdin_bytes().len();

    window.drag(row, column, column + 8);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        copies(&zellij.raw_bytes()).is_empty(),
        "nothing may be copied when the mouse is released"
    );

    window.copy_key();
    zellij.wait_until_raw_output("the selection to be copied", |bytes| {
        copies(bytes).iter().any(|text| text.contains("copy me"))
    });
    let leaked = terminal.stdin_bytes()[stdin_before..].to_vec();
    assert!(
        leaked.is_empty(),
        "copying must not reach the pane: {:?}",
        String::from_utf8_lossy(&leaked)
    );

    zellij.quit();
}
