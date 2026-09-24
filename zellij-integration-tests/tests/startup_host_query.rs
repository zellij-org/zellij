#![cfg(unix)]

use std::thread::sleep;
use std::time::Duration;

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, TestClient, TestSession,
    TERMINAL_SIZE,
};

const IDLE_BEYOND_SHORT_FLUSH: Duration = Duration::from_millis(150);
const FOREGROUND_COLOR_QUERY: &[u8] = b"\x1b]10;?\x1b\\";
const FOREGROUND_QUERY_WITH_BARRIER: &[u8] = b"\x1b]10;?\x1b\\\x1b[c";
// Deliberately not stable under Zellij's 16-bit -> 8-bit cache conversion, so
// an empty forward followed by cache synthesis cannot make this test pass
const FORWARDED_FOREGROUND_REPLY: &[u8] = b"\x1b]10;rgb:3456/4567/5678\x1b\\";

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

fn lock_attached_interface(client: &TestClient) {
    client.send_stdin(&keys::ctrl('g'));
    client.wait_until("reattached interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });
}

fn unlock_attached_interface(client: &TestClient) {
    client.send_stdin(&keys::ctrl('g'));
    client.wait_until("reattached interface unlocked", |grid_snapshot| {
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
fn attach_startup_replies_are_not_forwarded_into_pane_query() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    zellij.detach_main_client();
    let reattached = zellij.attach_client(TERMINAL_SIZE);
    reattached.wait_until("reattached client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("$ ")
    });
    lock_attached_interface(&reattached);

    let baseline_len = terminal.stdin_bytes().len();
    let stdout_baseline = reattached.raw_bytes().len();
    // The focused pane asks for the host foreground colour. Waiting for the
    // query immediately followed by Zellij's Primary-DA barrier proves the
    // client's forwarding window is open; the startup OSC 10 query is followed
    // by DECRPM instead and cannot satisfy this predicate
    terminal.output(FOREGROUND_COLOR_QUERY);
    reattached.wait_until_raw_output("pane host-query forward opened", |bytes| {
        bytes.get(stdout_baseline..).is_some_and(|new_bytes| {
            new_bytes
                .windows(FOREGROUND_QUERY_WITH_BARRIER.len())
                .any(|window| window == FOREGROUND_QUERY_WITH_BARRIER)
        })
    });

    // Simulate the delayed attach startup replies from the physical Windows
    // Terminal trace, followed by the answer to the pane's query and the DA
    // barrier. Every startup report is valid Zellij state, but none belongs in
    // the pane's pty transaction
    reattached.send_stdin(
        b"\x1b[4;1160;2220t\
          \x1b[6;20;10t\
          \x1b]11;rgb:1111/1111/1111\x1b\\\
          \x1b]10;rgb:2222/2222/2222\x1b\\\
          \x1b[?2026;2$y\
          \x1b]10;rgb:3456/4567/5678\x1b\\\
          \x1b[?65;1c",
    );

    let stdin = terminal.wait_for_stdin("filtered host reply reached pane", |stdin_bytes| {
        stdin_bytes.len() > baseline_len
    });
    assert_eq!(
        stdin.get(baseline_len..),
        Some(FORWARDED_FOREGROUND_REPLY),
        "attach startup replies contaminated the pane's host-query response"
    );

    reattached.send_stdin(b"x");
    let mut expected = FORWARDED_FOREGROUND_REPLY.to_vec();
    expected.push(b'x');
    let stdin = terminal.wait_for_stdin(
        "ordinary input remained usable after filtered replies",
        |stdin_bytes| stdin_bytes.get(baseline_len..) == Some(expected.as_slice()),
    );
    assert_eq!(
        stdin.get(baseline_len..),
        Some(expected.as_slice()),
        "ordinary input was lost or altered after attach reply filtering"
    );

    unlock_attached_interface(&reattached);
    reattached.quit();
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
