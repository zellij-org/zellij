#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, start_zellij, FakePtyHandle, TestRunner, TestSession,
    TERMINAL_SIZE,
};

const CLIPBOARD_READ_QUERY: &[u8] = b"\x1b]52;c;?\x1b\\";
const CLIPBOARD_REPLY: &[u8] = b"\x1b]52;c;aGVsbG8=\x1b\\";
const EMPTY_CLIPBOARD_REPLY: &[u8] = b"\x1b]52;c;\x1b\\";
const LATE_CLIPBOARD_REPLY: &[u8] = b"\x1b]52;c;bGF0ZQ==\x1b\\";

fn start_zellij_with_paste_read_enabled() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("dangerously_enable_paste_buffer_read true")
        .start()
}

fn claim_quiet_terminal(zellij: &TestSession) -> FakePtyHandle {
    let terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    terminal.disable_echo();
    terminal
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn assert_pane_keeps_processing_output(
    zellij: &TestSession,
    terminal: &FakePtyHandle,
    sentinel: &str,
) {
    terminal.output(format!("{}\r\n", sentinel).as_bytes());
    zellij.wait_until("the pane kept processing its output", |grid_snapshot| {
        grid_snapshot.contains(sentinel)
    });
}

#[test]
fn a_clipboard_read_is_never_answered_when_the_option_is_off() {
    let mut zellij = start_zellij();
    let terminal = claim_quiet_terminal(&zellij);

    terminal.output(CLIPBOARD_READ_QUERY);
    assert_pane_keeps_processing_output(&zellij, &terminal, "read-was-dropped");

    let raw_output = zellij.raw_bytes();
    assert!(
        !contains_subslice(&raw_output, b"\x1b]52;"),
        "an opted-out clipboard read must never reach the host terminal"
    );

    let pane_stdin = terminal.stdin_bytes();
    assert!(
        !contains_subslice(&pane_stdin, b"\x1b]52;"),
        "no clipboard reply may be synthesised for the pane: {:?}",
        pane_stdin
    );

    zellij.quit();
}

#[test]
fn a_clipboard_read_is_forwarded_when_the_option_is_on() {
    let mut zellij = start_zellij_with_paste_read_enabled();
    let terminal = claim_quiet_terminal(&zellij);

    terminal.output(CLIPBOARD_READ_QUERY);
    zellij.wait_until_raw_output("the clipboard read reached the host terminal", |bytes| {
        contains_subslice(bytes, CLIPBOARD_READ_QUERY)
    });

    zellij.send_stdin(CLIPBOARD_REPLY);

    terminal.wait_for_stdin("the host reply reached the pane verbatim", |stdin_bytes| {
        contains_subslice(stdin_bytes, CLIPBOARD_REPLY)
    });
    assert_pane_keeps_processing_output(&zellij, &terminal, "reply-was-delivered");

    zellij.quit();
}

#[test]
fn an_empty_clipboard_reply_does_not_wedge_the_pane() {
    let mut zellij = start_zellij_with_paste_read_enabled();
    let terminal = claim_quiet_terminal(&zellij);

    terminal.output(CLIPBOARD_READ_QUERY);
    zellij.wait_until_raw_output("the clipboard read reached the host terminal", |bytes| {
        contains_subslice(bytes, CLIPBOARD_READ_QUERY)
    });

    zellij.send_stdin(EMPTY_CLIPBOARD_REPLY);

    terminal.wait_for_stdin("the empty reply reached the pane", |stdin_bytes| {
        contains_subslice(stdin_bytes, EMPTY_CLIPBOARD_REPLY)
    });
    assert_pane_keeps_processing_output(&zellij, &terminal, "empty-reply-was-delivered");

    zellij.quit();
}

#[test]
fn a_late_clipboard_reply_is_dropped_without_wedging_the_pane() {
    let mut zellij = start_zellij_with_paste_read_enabled();
    let terminal = claim_quiet_terminal(&zellij);

    terminal.output(CLIPBOARD_READ_QUERY);
    zellij.wait_until_raw_output("the clipboard read reached the host terminal", |bytes| {
        contains_subslice(bytes, CLIPBOARD_READ_QUERY)
    });

    zellij.send_stdin(CLIPBOARD_REPLY);
    terminal.wait_for_stdin("the host reply reached the pane", |stdin_bytes| {
        contains_subslice(stdin_bytes, CLIPBOARD_REPLY)
    });

    zellij.send_stdin(LATE_CLIPBOARD_REPLY);
    zellij.send_stdin(b"z");

    let pane_stdin = terminal.wait_for_stdin(
        "a keystroke sent after the late reply still reached the pane",
        |stdin_bytes| stdin_bytes.contains(&b'z'),
    );
    assert!(
        !contains_subslice(&pane_stdin, b"bGF0ZQ=="),
        "clipboard data arriving after the window closed must never reach the pane: {:?}",
        pane_stdin
    );
    assert_pane_keeps_processing_output(&zellij, &terminal, "late-reply-was-ignored");

    zellij.quit();
}
