#![cfg(unix)]

use zellij_integration_tests::{claim_first_terminal_and_wait_for_prompt, start_zellij};

#[test]
fn clipboard_query_before_any_copy_gets_an_empty_reply() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    terminal.output(b"\x1b]52;c;?\x07");

    terminal.wait_for_stdin("empty clipboard reply reached the pane", |stdin_bytes| {
        stdin_bytes
            .windows(8)
            .any(|window| window == b"\x1b]52;c;\x07")
    });

    zellij.quit();
}

#[test]
fn clipboard_query_is_answered_from_the_last_copy() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    // copy "hello" through zellij via an OSC 52 write, then query
    terminal.output(b"\x1b]52;c;aGVsbG8=\x07");
    terminal.output(b"\x1b]52;c;?\x07");

    terminal.wait_for_stdin("bel-terminated reply carries the copy", |stdin_bytes| {
        stdin_bytes
            .windows(16)
            .any(|window| window == b"\x1b]52;c;aGVsbG8=\x07")
    });

    // reply mirrors the query's destination and terminator
    terminal.output(b"\x1b]52;p;?\x1b\\");

    terminal.wait_for_stdin("st-terminated reply for primary", |stdin_bytes| {
        stdin_bytes
            .windows(17)
            .any(|window| window == b"\x1b]52;p;aGVsbG8=\x1b\\")
    });

    zellij.quit();
}
