#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, LayoutInfo, TestRunner,
    TERMINAL_SIZE,
};
use zellij_utils::app_control::{read_request, write_reply, AppReply, AppRequest};
use zellij_utils::cli::CliAction;
use zellij_utils::consts::APP_PANEL_SESSION;
use zellij_utils::session_link::{request_session_attention, session_answers};

const LIMIT: Duration = Duration::from_secs(10);

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

fn fake_desktop_app() -> mpsc::Receiver<AppRequest> {
    use interprocess::local_socket::prelude::*;
    let socket = zellij_utils::consts::app_control_socket();
    let _ = std::fs::remove_file(&socket);
    let listener = zellij_utils::consts::ipc_bind(&socket).unwrap();
    let (requests, received) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            if let Ok(request) = read_request(&mut stream) {
                let _ = requests.send(request);
            }
            let _ = write_reply(&mut stream, &AppReply::Done);
        }
    });
    received
}

fn session_with_an_item(text: &str) -> BackgroundTestSession {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_indicator true")
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    terminal.output(&needs_you(text));
    let name = background.session_name().to_owned();
    wait_for("the item to be counted", || needs_you_in(&name) == Some(1));
    background
}

fn needs_you_in(session: &str) -> Option<usize> {
    request_session_attention(session, Duration::from_secs(2))
        .ok()
        .map(|attention| attention.counts.needs_you)
}

#[test]
fn the_tray_icon_toggle_reaches_the_desktop_app_from_a_keybinding_and_from_the_cli() {
    let requests = fake_desktop_app();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(
            "keybinds {\n    shared {\n        bind \"Alt y\" { ToggleTrayIcon; }\n    }\n}",
        )
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::alt('y'));
    assert_eq!(
        requests
            .recv_timeout(LIMIT)
            .expect("the keybinding asks the app"),
        AppRequest::ToggleTrayIcon
    );
    assert_eq!(zellij.run_cli_action(CliAction::ToggleTrayIcon), 0);
    assert_eq!(
        requests
            .recv_timeout(LIMIT)
            .expect("the cli action asks the app"),
        AppRequest::ToggleTrayIcon
    );
    zellij.quit();
}

#[test]
fn clearing_all_notifications_empties_every_session() {
    let first = session_with_an_item("First?");
    let second = session_with_an_item("Second?");
    let names = vec![
        first.session_name().to_owned(),
        second.session_name().to_owned(),
    ];
    zellij_window::clear_all_sessions(names.clone());
    for name in &names {
        wait_for(&format!("{} to be cleared", name), || {
            needs_you_in(name) == Some(0)
        });
    }
}

#[test]
fn stopping_all_sessions_ends_every_listed_session_and_only_those() {
    let first = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let second = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let kept = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let stopped = vec![
        first.session_name().to_owned(),
        second.session_name().to_owned(),
    ];
    zellij_window::stop_all_sessions(stopped.clone());
    for name in &stopped {
        wait_for(&format!("{} to stop", name), || {
            !session_answers(name, Duration::from_millis(300))
        });
    }
    assert!(session_answers(kept.session_name(), Duration::from_secs(2)));
}

#[test]
fn the_hidden_panel_session_is_left_out_of_every_session_list() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let mut panel = TestRunner::new(TERMINAL_SIZE)
        .with_session_name(APP_PANEL_SESSION)
        .with_config("show_startup_tips false\nshow_release_notes false\npane_frames false")
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    pane\n}\n".to_owned(),
        ))
        .start();
    panel.expect_pty_spawn();
    assert!(session_answers(APP_PANEL_SESSION, Duration::from_secs(2)));
    for name in [APP_PANEL_SESSION, ".old-app-thing", "exited"] {
        let folder = zellij_utils::consts::session_info_folder_for_session(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-layout.kdl"), "layout {\n    pane\n}\n").unwrap();
    }
    let by_mtime = zellij_utils::sessions::get_sessions_sorted_by_mtime().unwrap();
    assert!(by_mtime.contains(&background.session_name().to_owned()));
    assert!(
        !by_mtime.iter().any(|name| name.starts_with('.')),
        "{:?}",
        by_mtime
    );
    let resurrectable: Vec<String> = zellij_utils::sessions::get_resurrectable_sessions()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(resurrectable.contains(&"exited".to_owned()));
    assert!(
        !resurrectable.iter().any(|name| name.starts_with('.')),
        "{:?}",
        resurrectable
    );
    let names = zellij_utils::sessions::get_resurrectable_session_names();
    assert!(names.contains(&"exited".to_owned()));
    assert!(
        !names.iter().any(|name| name.starts_with('.')),
        "{:?}",
        names
    );
    panel.quit();
}
