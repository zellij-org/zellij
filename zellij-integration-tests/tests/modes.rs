#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{keys, normalized, FakePtyHandle, Size, TestRunner, TestSession};

const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};
const PROMPT: &[u8] = b"$ ";
const PROMPT_ROW: usize = 2;
const FIRST_PANE_PROMPT_X: usize = 3;
const RIGHT_PANE_PROMPT_X: usize = 63;

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE).start()
}

fn claim_first_terminal_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until(
        "first terminal prompt rendered in loaded app",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    terminal
}

#[test]
fn lock_mode() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::LOCK_MODE);
    zellij.wait_until("interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });

    zellij.send_stdin(&keys::TAB_MODE);
    zellij.send_stdin(&keys::NEW_TAB_IN_TAB_MODE);
    zellij.send_stdin(b"abc");

    terminal.wait_for_stdin("forwarded keys reached the pane", |stdin_bytes| {
        stdin_bytes.windows(3).any(|window| window == b"abc")
    });
    let grid_snapshot =
        zellij.wait_until("forwarded keys rendered in locked pane", |grid_snapshot| {
            grid_snapshot.contains("abc")
                && !grid_snapshot.contains("PANE")
                && !grid_snapshot.contains("Tab #2")
        });
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.send_stdin(&keys::LOCK_MODE);
    zellij.wait_until("interface unlocked", |grid_snapshot| {
        grid_snapshot.contains("PANE")
    });
    zellij.quit();
}

#[test]
fn tmux_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::TMUX_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_TMUX_MODE);
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("right terminal opened via tmux mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn bracketed_paste() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::BRACKETED_PASTE_START);
    zellij.send_stdin(&keys::TAB_MODE);
    zellij.send_stdin(&keys::NEW_TAB_IN_TAB_MODE);
    zellij.send_stdin(b"abc");
    zellij.send_stdin(&keys::BRACKETED_PASTE_END);

    terminal.wait_for_stdin("pasted text reached the pane", |stdin_bytes| {
        stdin_bytes.windows(3).any(|window| window == b"abc")
    });
    let grid_snapshot = zellij.wait_until("pasted text rendered, no new tab", |grid_snapshot| {
        grid_snapshot.contains("abc")
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Tab #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scrolling_inside_a_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });

    for line in 1..=21 {
        right_terminal.output(format!("line{}\r\n", line).as_bytes());
    }
    zellij.wait_until("right terminal filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains("line21")
    });

    zellij.send_stdin(&keys::SCROLL_MODE);
    zellij.send_stdin(&keys::SCROLL_UP_IN_SCROLL_MODE);
    zellij.send_stdin(&keys::SCROLL_UP_IN_SCROLL_MODE);

    let grid_snapshot =
        zellij.wait_until("scrolled up one line inside the pane", |grid_snapshot| {
            grid_snapshot.contains("SCROLL:  2/2") && grid_snapshot.contains("PgDn|PgUp")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
