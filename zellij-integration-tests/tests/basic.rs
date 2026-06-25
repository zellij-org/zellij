#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, split_right_and_wait_for_prompt,
    start_zellij, FakePtyHandle, TestSession,
};

fn resize_focused_pane_left(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('n'));
    zellij.send_stdin(&keys::key('h'));
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
            && grid_snapshot.cursor_is_at(col(3).row(2))
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
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(63).row(2))
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
            && grid_snapshot.cursor_is_at(col(63 - cursor_shift_left).row(2))
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
            !grid_snapshot.contains("about to exit") && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );
    zellij.quit();
}
