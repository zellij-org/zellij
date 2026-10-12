#![cfg(unix)]

use std::sync::Mutex;
use std::time::{Duration, Instant};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, default_timeout, keys, TestRunner, TERMINAL_SIZE,
};

// both tests pick "the next session" from the sessions running in this process, so they must
// not see each other's sessions
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

const DELETE: [u8; 4] = *b"\x1b[3~";
// the manager lists the current session only when session suggestions are on
const SESSION_FEATURES_ON: &str = "session_card true\nsession_indicator true\nmouse_mode true";

fn socket_of(session_name: &str) -> std::path::PathBuf {
    zellij_utils::consts::ZELLIJ_SOCK_DIR.join(session_name)
}

fn wait_for_path_to_disappear(path: &std::path::Path, what: &str) {
    let deadline = Instant::now() + default_timeout();
    while path.exists() {
        if Instant::now() >= deadline {
            panic!("timed out waiting for {} to be removed", what);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn deleting_the_current_session_in_the_manager_stays_in_the_manager() {
    let _one_at_a_time = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let killed = zellij.session_name().to_owned();
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("the manager lists the current session", |grid_snapshot| {
        grid_snapshot.contains("Session: ") && grid_snapshot.contains("[CURRENT]")
    });
    // onto the first row, the current session
    zellij.send_stdin(&keys::DOWN);
    zellij.send_stdin(&keys::UP);
    zellij.send_stdin(&DELETE);
    zellij.wait_until("the kill asks for a second <Del>", |grid_snapshot| {
        grid_snapshot.contains("Press <Del> again")
    });
    zellij.send_stdin(&DELETE);
    let switch = zellij.follow_switch();
    assert_eq!(switch.killed_session.as_deref(), Some(killed.as_str()));
    wait_for_path_to_disappear(&socket_of(&killed), "the killed session socket");
    zellij.wait_until(
        "the manager is open in the new session, with the killed one grayed out",
        |grid_snapshot| {
            grid_snapshot.contains("Session: ")
                && grid_snapshot.contains(&format!("{} [KILLED]", killed))
                && grid_snapshot.contains("<Enter> to resurrect")
                && grid_snapshot.contains("[CURRENT]")
        },
    );
    zellij.quit();
}

#[test]
fn k_twice_in_session_mode_kills_the_session_without_the_manager() {
    let _one_at_a_time = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let killed = zellij.session_name().to_owned();
    // one k, then any other key, disarms
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('k'));
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        socket_of(&killed).exists(),
        "a single k must not kill the session"
    );
    zellij.send_stdin(&keys::key('k'));
    let switch = zellij.follow_switch();
    assert_eq!(switch.killed_session, None);
    assert_ne!(switch.name.as_deref(), Some(killed.as_str()));
    wait_for_path_to_disappear(&socket_of(&killed), "the killed session socket");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the new session shows no manager", |grid_snapshot| {
        !grid_snapshot.contains("Session: ")
    });
    zellij.quit();
}
