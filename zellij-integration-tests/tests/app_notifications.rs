#![cfg(target_os = "linux")]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::{Duration, Instant};
use zellij_integration_tests::{TestRunner, TERMINAL_SIZE};
use zellij_utils::data::PaneId;
use zellij_utils::input::actions::Action;
use zellij_terminal::{NotificationProbe, ProbeEvent};

const LIMIT: Duration = Duration::from_secs(10);
const SETTLE: Duration = Duration::from_millis(2500);

fn wait_for(what: &str, mut check: impl FnMut() -> bool) {
    let deadline = Instant::now() + LIMIT;
    while !check() {
        if Instant::now() >= deadline {
            panic!("timed out waiting for {}", what);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn needs_you(text: &str) -> Vec<u8> {
    format!(
        "\u{1b}]7501;state=blocked:kind=question:msg={}\u{1b}\\",
        STANDARD.encode(text)
    )
    .into_bytes()
}

fn shown(probe: &NotificationProbe, id: &str) -> Vec<(String, String)> {
    probe
        .events()
        .into_iter()
        .filter_map(|event| match event {
            ProbeEvent::Shown {
                id: shown,
                title,
                body,
            } if shown == id => Some((title, body)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_blocked_pane_in_another_session_is_notified_once_and_withdrawn_when_marked_seen() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_indicator true")
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    let session = background.session_name().to_owned();
    let probe = NotificationProbe::start(zellij_utils::consts::ZELLIJ_SOCK_DIR.clone(), true)
        .expect("the probe to watch the socket folder");
    wait_for("the app to see the session", || {
        probe.live_sessions().contains(&session)
    });

    terminal.output(&needs_you("Allow edit to main.rs?"));
    let pane = PaneId::Terminal(terminal.terminal_id());
    let id = NotificationProbe::item_id(&session, &pane.to_string());
    wait_for("the blocked pane to be notified", || {
        !shown(&probe, &id).is_empty()
    });
    std::thread::sleep(SETTLE);
    let notifications = shown(&probe, &id);
    assert_eq!(
        notifications.len(),
        1,
        "one notification per item, never stacked: {:?}",
        probe.events()
    );
    let (title, body) = &notifications[0];
    assert!(title.starts_with(&session), "{}", title);
    assert!(body.contains("Allow edit to main.rs?"), "{}", body);

    zellij_utils::session_link::send_actions_to_session(
        &session,
        vec![Action::MarkPaneSeen {
            session: Some(session.clone()),
            pane_id: Some(pane),
        }],
    )
    .expect("the session to take the mark-seen request");
    wait_for("the notification to be withdrawn", || {
        probe
            .events()
            .contains(&ProbeEvent::Withdrawn { id: id.clone() })
    });
    assert_eq!(shown(&probe, &id).len(), 1);
}

struct WindowClient {
    sender: zellij_utils::ipc::IpcSenderWithContext<zellij_utils::ipc::ClientToServerMsg>,
}

impl WindowClient {
    fn attach_to(session_name: &str, pane_id: u32) -> Self {
        use zellij_utils::ipc::{IpcSenderWithContext, ServerToClientMsg};
        let path = zellij_utils::consts::session_socket_path(session_name);
        let stream = zellij_utils::consts::ipc_connect(&path)
            .unwrap_or_else(|e| panic!("failed to connect to {:?}: {:?}", path, e));
        let mut sender = IpcSenderWithContext::new(stream);
        let mut receiver = sender.get_receiver::<ServerToClientMsg>();
        let handshake = zellij_client::first_message(
            &zellij_client::ClientInfo::Attach(
                session_name.to_owned(),
                zellij_utils::input::options::Options::default(),
            ),
            &zellij_utils::cli::CliArgs::default(),
            &zellij_utils::input::options::Options::default(),
            TERMINAL_SIZE,
            std::collections::BTreeMap::new(),
            None,
            Some((pane_id, false)),
            None,
        );
        sender
            .send_client_msg(handshake.message)
            .expect("the attach message is sent");
        std::thread::spawn(move || while receiver.recv_server_msg().is_some() {});
        WindowClient { sender }
    }

    fn focus(&mut self, focused: bool) {
        self.sender
            .send_client_msg(
                zellij_utils::ipc::ClientToServerMsg::HostTerminalFocusChanged { focused },
            )
            .expect("the focus change is sent");
    }
}

fn failed_command(terminal: &zellij_integration_tests::FakePtyHandle) {
    terminal.output(
        b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\cargo test\x1b]133;C\x1b\\\r\n\x1b]133;D;1\x1b\\",
    );
}

fn window_on_a_background_session(
    focused: bool,
) -> (
    zellij_integration_tests::BackgroundTestSession,
    zellij_integration_tests::FakePtyHandle,
    WindowClient,
    NotificationProbe,
    String,
) {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_indicator true")
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    let session = background.session_name().to_owned();
    let probe = NotificationProbe::start(zellij_utils::consts::ZELLIJ_SOCK_DIR.clone(), true)
        .expect("the probe to watch the socket folder");
    wait_for("the app to see the session", || {
        probe.live_sessions().contains(&session)
    });
    let mut window = WindowClient::attach_to(&session, terminal.terminal_id());
    std::thread::sleep(Duration::from_millis(500));
    window.focus(focused);
    std::thread::sleep(Duration::from_millis(300));
    let id = NotificationProbe::item_id(
        &session,
        &PaneId::Terminal(terminal.terminal_id()).to_string(),
    );
    (background, terminal, window, probe, id)
}

#[test]
fn a_pane_in_an_unfocused_window_notifies_when_it_finishes() {
    let (_background, terminal, _window, probe, id) = window_on_a_background_session(false);
    failed_command(&terminal);
    wait_for("the failed command to be notified", || {
        !shown(&probe, &id).is_empty()
    });
}

#[test]
fn a_pane_in_a_focused_window_does_not_notify() {
    let (_background, terminal, _window, probe, id) = window_on_a_background_session(true);
    failed_command(&terminal);
    std::thread::sleep(SETTLE);
    assert!(
        shown(&probe, &id).is_empty(),
        "a pane in a focused window was notified: {:?}",
        probe.events()
    );
}
