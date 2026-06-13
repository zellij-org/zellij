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

fn split_pane_right(zellij: &TestSession) {
    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
}

fn split_right_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    split_pane_right(zellij);
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });
    terminal
}

fn resize_focused_pane_left(zellij: &TestSession) {
    zellij.send_stdin(&keys::RESIZE_MODE);
    zellij.send_stdin(&keys::RESIZE_LEFT_IN_RESIZE_MODE);
    zellij.send_stdin(&keys::ENTER);
}

fn column_count(terminal: &FakePtyHandle, what: &str) -> u16 {
    terminal.wait_for_size(what, |_, _| true).0
}

#[test]
fn starts_with_one_terminal() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("steady loaded state", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn split_terminals_vertically() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("split rendered in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_pane() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = column_count(&right_terminal, "right terminal initial width");

    resize_focused_pane_left(&zellij);

    let resized_cols = right_terminal
        .wait_for_size("right terminal grew", |cols, _| cols > initial_cols)
        .0;
    first_terminal.wait_for_size("left terminal shrank", |cols, _| cols < initial_cols);

    let cursor_shift_left = (resized_cols.saturating_sub(initial_cols)) as usize;
    let grid_snapshot = zellij.wait_until("resized layout in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X - cursor_shift_left, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn command_pane_closes_on_exit() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    right_terminal.output(b"about to exit");
    zellij.wait_until("right terminal output rendered", |grid_snapshot| {
        grid_snapshot.contains("about to exit")
    });

    right_terminal.exit(Some(0));
    zellij.wait_until(
        "right terminal closed, focus back on first",
        |grid_snapshot| {
            !grid_snapshot.contains("about to exit")
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    zellij.quit();
}
