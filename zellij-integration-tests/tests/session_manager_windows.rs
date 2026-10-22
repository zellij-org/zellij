#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::sync::mpsc;
use std::time::Duration;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, FakePtyHandle,
    GridSnapshot, LayoutInfo, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::app_control::{
    read_request, write_reply, AppReply, AppRequest, AppState, OpenWindowRequest, WindowState,
};

const LIMIT: Duration = Duration::from_secs(10);
const QUIET: Duration = Duration::from_millis(700);
const CONFIG: &str = "session_indicator true\nmouse_mode true";

fn fake_desktop_app(windows_on: &[&str]) -> mpsc::Receiver<AppRequest> {
    zellij_integration_tests::test_env::init();
    let socket = zellij_utils::consts::app_control_socket();
    zellij_utils::consts::prepare_private_dir(socket.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&socket);
    let listener = zellij_utils::consts::ipc_bind(&socket).unwrap();
    let state = AppState {
        windows: windows_on
            .iter()
            .map(|name| WindowState {
                session: Some((*name).to_owned()),
            })
            .collect(),
        ..Default::default()
    };
    let (requests, received) = mpsc::channel();
    std::thread::spawn(move || {
        use interprocess::local_socket::prelude::*;
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let reply = match read_request(&mut stream) {
                Ok(AppRequest::Describe) => AppReply::State(state.clone()),
                Ok(AppRequest::Ping) => AppReply::Done,
                Ok(request) => {
                    let _ = requests.send(request);
                    AppReply::Done
                },
                Err(_) => AppReply::Done,
            };
            let _ = write_reply(&mut stream, &reply);
        }
    });
    received
}

fn needs_you(text: &str) -> Vec<u8> {
    format!(
        "\u{1b}]7501;state=blocked:kind=question:msg={}\u{1b}\\",
        STANDARD.encode(text)
    )
    .into_bytes()
}

fn other_session() -> BackgroundTestSession {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(CONFIG)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    background
}

fn main_session() -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE).with_config(CONFIG).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij
}

fn selected_session_line(grid_snapshot: &GridSnapshot, name: &str) -> bool {
    grid_snapshot.contains(&format!("┌ {} ", name))
}

fn open_sessions_tab(zellij: &TestSession, other: &str) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until(
        "the session manager opens on the sessions",
        |grid_snapshot| grid_snapshot.contains("<F1> all keys") && grid_snapshot.contains(other),
    );
}

fn open_sessions_with(zellij: &TestSession, other: &str, help: &str) {
    open_sessions_tab(zellij, other);
    zellij.send_stdin(&keys::DOWN);
    zellij.wait_until(
        "the session is selected and the help offers it",
        |grid_snapshot| selected_session_line(grid_snapshot, other) && grid_snapshot.contains(help),
    );
}

fn confirm_warning(zellij: &TestSession, detaching: bool) {
    let warning = zellij.wait_until("the warning is shown", |grid_snapshot| {
        grid_snapshot.contains("You are connected from a terminal")
    });
    assert!(warning.contains("The window opens on the desktop of the machine running Zellij"));
    assert_eq!(
        warning.contains("Detaching here also ends this connection"),
        detaching,
        "{}",
        warning.text
    );
    zellij.send_stdin(&keys::ENTER);
}

fn open_window(requests: &mpsc::Receiver<AppRequest>) -> OpenWindowRequest {
    match requests.recv_timeout(LIMIT).expect("the app is asked") {
        AppRequest::OpenWindow(request) => request,
        other => panic!("expected a window request, got {:?}", other),
    }
}

#[test]
fn alt_w_from_a_terminal_warns_first_then_asks_the_app_for_the_session_window() {
    let other = other_session();
    let requests = fake_desktop_app(&[other.session_name()]);
    let mut zellij = main_session();
    open_sessions_with(&zellij, other.session_name(), "<Alt w> go to its window");
    let help = zellij.snapshot();
    assert!(help.contains("(!) windows open on the Zellij machine"));
    zellij.send_stdin(&keys::alt('w'));
    confirm_warning(&zellij, false);
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some(other.session_name()));
    assert!(!request.create);
    zellij.quit();
}

#[test]
fn escape_on_the_warning_sends_nothing() {
    let other = other_session();
    let requests = fake_desktop_app(&[]);
    let mut zellij = main_session();
    open_sessions_with(&zellij, other.session_name(), "<Alt w> open it in a window");
    zellij.send_stdin(&keys::alt('w'));
    zellij.wait_until("the warning is shown", |grid_snapshot| {
        grid_snapshot.contains("You are connected from a terminal")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the list is back", |grid_snapshot| {
        !grid_snapshot.contains("You are connected from a terminal")
            && grid_snapshot.contains(other.session_name())
    });
    assert!(requests.recv_timeout(QUIET).is_err());
    zellij.quit();
}

#[test]
fn alt_w_on_a_typed_name_creates_that_session_in_a_new_window() {
    let other = other_session();
    let requests = fake_desktop_app(&[]);
    let mut zellij = main_session();
    open_sessions_tab(&zellij, other.session_name());
    zellij.send_stdin(b"brand-new");
    zellij.wait_until(
        "the help offers to create it in a window",
        |grid_snapshot| {
            grid_snapshot.contains("brand-new")
                && grid_snapshot.contains("<Alt w> create it in a window")
        },
    );
    zellij.send_stdin(&keys::alt('w'));
    confirm_warning(&zellij, false);
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some("brand-new"));
    assert!(request.create);
    assert!(
        request.layout.is_some(),
        "a new session gets the default layout"
    );
    zellij.quit();
}

#[test]
fn alt_d_asks_for_the_window_and_detaches_this_terminal() {
    let other = other_session();
    let requests = fake_desktop_app(&[other.session_name()]);
    let mut zellij = main_session();
    open_sessions_with(
        &zellij,
        other.session_name(),
        "<Alt d> same, then detach this terminal",
    );
    zellij.send_stdin(&keys::alt('d'));
    confirm_warning(&zellij, true);
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some(other.session_name()));
    assert!(zellij.join_main_client().is_none(), "the terminal detached");
}

#[test]
fn alt_w_in_the_inbox_opens_the_items_pane_in_a_window() {
    let other = TestRunner::new(TERMINAL_SIZE)
        .with_config(CONFIG)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = other.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Deploy now?"));
    let requests = fake_desktop_app(&[]);
    let mut zellij = main_session();
    zellij.wait_until("the item reached this session", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("need you")
    });
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox selects the item", |grid_snapshot| {
        grid_snapshot.contains("NEEDS YOU") && grid_snapshot.contains("Deploy now?")
    });
    zellij.send_stdin(&keys::alt('w'));
    confirm_warning(&zellij, false);
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some(other.session_name()));
    assert!(!request.create, "an open window of the session is reused");
    assert_eq!(request.pane_id, Some((terminal.terminal_id(), false)));
    zellij.quit();
}

#[test]
fn without_the_desktop_app_alt_w_is_still_offered_and_warns_before_starting_it() {
    let other = other_session();
    let mut zellij = main_session();
    open_sessions_with(&zellij, other.session_name(), "<Alt w> open it in a window");
    assert!(!zellij.snapshot().contains("<Alt d>"));
    zellij.send_stdin(&keys::alt('w'));
    zellij.wait_until("the warning is shown", |grid_snapshot| {
        grid_snapshot.contains("You are connected from a terminal")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the list is back", |grid_snapshot| {
        !grid_snapshot.contains("You are connected from a terminal")
            && grid_snapshot.contains(other.session_name())
    });
    zellij.quit();
}

#[test]
fn switching_with_alt_x_disconnects_the_other_clients_of_the_target() {
    let other = other_session();
    let bystander = other.attach_client(TERMINAL_SIZE);
    bystander.wait_until("the bystander is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    let mut zellij = main_session();
    open_sessions_with(
        &zellij,
        other.session_name(),
        "<Alt x> open here, disconnect others",
    );
    zellij.send_stdin(&keys::alt('x'));
    let (switch, _first) = zellij.follow_switch_into(other);
    assert!(switch.disconnects_others());
    let deadline = std::time::Instant::now() + LIMIT;
    while !bystander.has_exited() {
        assert!(
            std::time::Instant::now() < deadline,
            "the other client was not disconnected"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(bystander
        .received_server_messages()
        .iter()
        .any(|message| message == "Exit"));
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

fn other_session_needing_you(text: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let other = TestRunner::new(TERMINAL_SIZE)
        .with_config(CONFIG)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = other.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you(text));
    (other, terminal)
}

fn open_inbox_on(zellij: &TestSession, text: &str) {
    zellij.wait_until("the item reached this session", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("need you")
    });
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox selects the item", |grid_snapshot| {
        grid_snapshot.contains("NEEDS YOU")
            && grid_snapshot.contains(text)
            && grid_snapshot.contains("<ENTER> go there")
    });
}

fn wait_for_exit(client: &zellij_integration_tests::TestClient, what: &str) {
    let deadline = std::time::Instant::now() + LIMIT;
    while !client.has_exited() {
        assert!(std::time::Instant::now() < deadline, "{}", what);
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn alt_w_in_the_layout_picker_creates_the_named_session_in_a_new_window() {
    let other = other_session();
    let requests = fake_desktop_app(&[]);
    let mut zellij = main_session();
    open_sessions_tab(&zellij, other.session_name());
    zellij.send_stdin(b"from-layout");
    zellij.wait_until("the name is typed", |grid_snapshot| {
        grid_snapshot.contains("from-layout")
    });
    zellij.send_stdin(&keys::ctrl('l'));
    zellij.wait_until("the layout picker is shown", |grid_snapshot| {
        grid_snapshot.contains("New session layout:") && grid_snapshot.contains("from-layout")
    });
    zellij.send_stdin(&keys::alt('w'));
    confirm_warning(&zellij, false);
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some("from-layout"));
    assert!(request.create);
    assert!(request.layout.is_some(), "the chosen layout is sent");
    assert!(
        !zellij.main_client().has_exited(),
        "plain keys keep this terminal"
    );
    zellij.quit();
}

#[test]
fn alt_x_in_the_inbox_switches_to_the_items_pane_and_disconnects_the_other_clients() {
    let (other, terminal) = other_session_needing_you("Ship it?");
    let bystander = other.attach_client(TERMINAL_SIZE);
    bystander.wait_until("the bystander is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    let mut zellij = main_session();
    open_inbox_on(&zellij, "Ship it?");
    zellij.send_stdin(&keys::alt('x'));
    let (switch, _first) = zellij.follow_switch_into(other);
    assert!(switch.disconnects_others());
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    wait_for_exit(&bystander, "the other client was not disconnected");
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn alt_x_in_the_inbox_on_this_session_goes_to_the_pane_and_disconnects_the_other_clients() {
    let zellij_runner = TestRunner::new(TERMINAL_SIZE).with_config(CONFIG);
    let mut zellij = zellij_runner.start();
    let first = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second = zellij.expect_pty_spawn();
    second.output(b"$ ");
    zellij.wait_until("the second tab is open", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("Tab #2")
    });
    first.output(&needs_you("Here?"));
    open_inbox_on(&zellij, "Here?");
    let bystander = zellij.attach_client(TERMINAL_SIZE);
    bystander.wait_until("the bystander is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.send_stdin(&keys::alt('x'));
    wait_for_exit(&bystander, "the other client was not disconnected");
    let grid_snapshot = zellij.wait_until("this client is on the item's tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && !grid_snapshot.contains("<ENTER> go there")
    });
    assert!(
        !zellij.main_client().has_exited(),
        "this client stays:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

fn welcome_screen() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(CONFIG)
        .with_layout(LayoutInfo::BuiltIn("welcome".to_owned()))
        .start()
}

fn select_on_the_welcome_screen(zellij: &TestSession, other: &str) {
    let listed = zellij.wait_until(
        "the welcome screen lists the other session",
        |grid_snapshot| {
            grid_snapshot.contains("new session with a random name")
                && grid_snapshot.contains(other)
        },
    );
    if !listed.contains("<Alt w> go to its window") {
        zellij.send_stdin(&keys::DOWN);
    }
    zellij.wait_until(
        "the other session is selected with its window keys",
        |grid_snapshot| {
            grid_snapshot.contains(other) && grid_snapshot.contains("<Alt w> go to its window")
        },
    );
}

fn warning_is_shown(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("You are connected from a terminal")
}

#[test]
fn the_warning_asks_once_for_alt_w_and_every_time_for_alt_d() {
    let other = other_session();
    let requests = fake_desktop_app(&[other.session_name()]);
    let mut zellij = welcome_screen();
    select_on_the_welcome_screen(&zellij, other.session_name());
    zellij.send_stdin(&keys::alt('w'));
    confirm_warning(&zellij, false);
    assert_eq!(
        open_window(&requests).session.as_deref(),
        Some(other.session_name())
    );
    select_on_the_welcome_screen(&zellij, other.session_name());
    zellij.send_stdin(&keys::alt('w'));
    let request = open_window(&requests);
    assert_eq!(request.session.as_deref(), Some(other.session_name()));
    assert!(
        !warning_is_shown(&zellij.snapshot()),
        "alt w asked a second time"
    );
    select_on_the_welcome_screen(&zellij, other.session_name());
    zellij.send_stdin(&keys::alt('d'));
    zellij.wait_until("alt d still warns", warning_is_shown);
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the warning is dismissed", |grid_snapshot| {
        !warning_is_shown(grid_snapshot)
    });
    assert!(requests.recv_timeout(QUIET).is_err());
    zellij.send_stdin(&keys::alt('d'));
    confirm_warning(&zellij, true);
    assert_eq!(
        open_window(&requests).session.as_deref(),
        Some(other.session_name())
    );
    assert!(
        zellij.join_main_client().is_none(),
        "the welcome screen quits after alt d"
    );
}
