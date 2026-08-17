#![cfg(unix)]

use std::thread::sleep;
use std::time::Duration;

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, TestSession,
};

const IDLE_BEYOND_SHORT_FLUSH: Duration = Duration::from_millis(150);

fn lock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });
}

fn unlock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface unlocked", |grid_snapshot| {
        grid_snapshot.contains("PANE")
    });
}

#[test]
fn fragmented_attach_reply_burst_does_not_leak_into_focused_pane() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    lock_interface(&zellij);

    zellij.send_stdin(b"\x1b]4;1;rgb:abcd");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"/ef01/2345\x1b\\");

    zellij.send_stdin(b"Z");
    let stdin = terminal.wait_for_stdin("sentinel keystroke reached the pane", |stdin_bytes| {
        stdin_bytes.contains(&b'Z')
    });

    assert!(
        !stdin.windows(4).any(|window| window == b"rgb:"),
        "host reply bytes leaked into the pane: {:?}",
        stdin
    );
    assert!(
        !stdin.contains(&b'/'),
        "host reply payload leaked into the pane: {:?}",
        stdin
    );
    assert!(
        !stdin.contains(&b';'),
        "host reply payload leaked into the pane: {:?}",
        stdin
    );

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn theme_query_from_pane_is_answered_when_host_mode_is_unknown() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    terminal.output(b"\x1b[?996n");

    terminal.wait_for_stdin("theme mode reply reached the pane", |stdin_bytes| {
        stdin_bytes
            .windows(9)
            .any(|window| window == b"\x1b[?997;1n")
    });

    zellij.quit();
}

#[test]
fn fragmented_function_key_arrives_intact() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    lock_interface(&zellij);

    zellij.send_stdin(b"\x1b[1;5");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"C");

    terminal.wait_for_stdin("ctrl-right reached the pane intact", |stdin_bytes| {
        stdin_bytes.windows(6).any(|window| window == b"\x1b[1;5C")
    });

    unlock_interface(&zellij);
    zellij.quit();
}

#[test]
fn normal_typing_is_not_delayed_or_duplicated() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    lock_interface(&zellij);

    zellij.send_stdin(b"hello");
    let stdin = terminal.wait_for_stdin("typed keys reached the pane", |stdin_bytes| {
        stdin_bytes.windows(5).any(|window| window == b"hello")
    });

    let occurrences = stdin
        .windows(5)
        .filter(|window| *window == b"hello")
        .count();
    assert_eq!(
        occurrences, 1,
        "typed keys must reach the pane exactly once: {:?}",
        stdin
    );

    unlock_interface(&zellij);
    zellij.quit();
}
