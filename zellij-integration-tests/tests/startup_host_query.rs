#![cfg(unix)]

use std::thread::sleep;
use std::time::Duration;

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, HostTerminal, TestRunner,
    TestSession, TERMINAL_SIZE,
};

const IDLE_BEYOND_SHORT_FLUSH: Duration = Duration::from_millis(150);

fn start_zellij_with_a_silent_host() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_host_terminal(HostTerminal::Manual)
        .start()
}

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
fn pixel_reply_split_after_the_introducer_does_not_leak_into_focused_pane() {
    let mut zellij = start_zellij_with_a_silent_host();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    zellij.send_stdin(b"\x1b[4;600;800t\x1b[");
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    zellij.send_stdin(b"6;25;12t");

    zellij.send_stdin(b"Z");
    let stdin = terminal.wait_for_stdin("sentinel keystroke reached the pane", |stdin_bytes| {
        stdin_bytes.contains(&b'Z')
    });

    assert_eq!(
        stdin,
        b"Z",
        "host reply bytes leaked into the pane: {:?}",
        String::from_utf8_lossy(&stdin)
    );

    zellij.quit();
}

#[test]
fn startup_replies_arriving_while_a_pane_query_is_forwarded_are_not_handed_to_the_pane() {
    let mut zellij = start_zellij_with_a_silent_host();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    terminal.output(b"\x1b]11;?\x1b\\");
    zellij.wait_until_raw_output("the pane query reached the host terminal", |bytes| {
        bytes
            .windows(11)
            .any(|window| window == b"\x1b]11;?\x1b\\\x1b[c")
    });

    zellij.send_stdin(
        b"\x1b[4;600;800t\x1b[6;25;12t\x1b]11;rgb:1111/1313/1313\x1b\\\x1b]10;rgb:ffff/ffff/ffff\x1b\\\x1b[?2026;2$y\x1b[?62;22c",
    );
    zellij.send_stdin(b"\x1b]11;rgb:1111/1313/1313\x1b\\\x1b[?62;22c");

    terminal.wait_for_stdin("the forwarded reply reached the pane", |stdin_bytes| {
        stdin_bytes.windows(4).any(|window| window == b"11;r")
    });
    sleep(IDLE_BEYOND_SHORT_FLUSH);
    let stdin = terminal.stdin_bytes();

    assert!(
        !stdin.windows(3).any(|window| window == b"25;"),
        "a startup pixel reply was handed to the pane: {:?}",
        String::from_utf8_lossy(&stdin)
    );
    assert!(
        !stdin.windows(4).any(|window| window == b"2026"),
        "a startup DECRPM reply was handed to the pane: {:?}",
        String::from_utf8_lossy(&stdin)
    );

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
