#![cfg(target_os = "linux")]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::{Duration, Instant};
use zellij_integration_tests::{TestRunner, TERMINAL_SIZE};
use zellij_utils::data::PaneId;
use zellij_utils::input::actions::Action;
use zellij_window::{NotificationProbe, ProbeEvent};

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
