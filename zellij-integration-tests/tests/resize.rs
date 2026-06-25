#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    col, keys, normalized, FakePtyHandle, GridSnapshot, Size, TestRunner, TestSession,
};

const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};
const PROMPT: &[u8] = b"$ ";

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
                && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );
    terminal
}

fn split_right_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(63).row(2))
    });
    terminal
}

fn split_down_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("lower terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(3).row(13))
    });
    terminal
}

fn focus_left_pane(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('h'));
    zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(3).row(2))
    });
}

fn focus_upper_pane(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('k'));
    zellij.wait_until("focus moved to the upper pane", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(3).row(2))
    });
}

fn resize_in_resize_mode(zellij: &TestSession, key: &[u8]) {
    zellij.send_stdin(&keys::ctrl('n'));
    zellij.send_stdin(key);
    zellij.send_stdin(&keys::ENTER);
}

fn width(terminal: &FakePtyHandle, what: &str) -> u16 {
    terminal.wait_for_size(what, |_, _| true).0
}

fn height(terminal: &FakePtyHandle, what: &str) -> u16 {
    terminal.wait_for_size(what, |_, _| true).1
}

fn pane_two_header_column(grid_snapshot: &GridSnapshot) -> Option<usize> {
    grid_snapshot.lines().into_iter().find_map(|line| {
        line.find("Pane #2")
            .map(|byte_index| line[..byte_index].chars().count())
    })
}

fn pane_two_header_row(grid_snapshot: &GridSnapshot) -> Option<usize> {
    grid_snapshot
        .lines()
        .into_iter()
        .position(|line| line.contains("Pane #2"))
}

fn vertical_boundary_column(zellij: &TestSession) -> usize {
    pane_two_header_column(&zellij.snapshot()).expect("vertical pane boundary rendered")
}

fn horizontal_boundary_row(zellij: &TestSession) -> usize {
    pane_two_header_row(&zellij.snapshot()).expect("horizontal pane boundary rendered")
}

#[test]
fn resize_increase_right() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    focus_left_pane(&zellij);
    let initial_cols = width(&left_terminal, "left terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('l'));

    left_terminal.wait_for_size("left terminal grew rightward", |cols, _| {
        cols > initial_cols
    });
    let grid_snapshot = zellij.wait_until("pane boundary moved right", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_left() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = width(&right_terminal, "right terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('H'));

    right_terminal.wait_for_size("right terminal shrank", |cols, _| cols < initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved right", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_right() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    focus_left_pane(&zellij);
    let initial_cols = width(&left_terminal, "left terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('L'));

    left_terminal.wait_for_size("left terminal shrank", |cols, _| cols < initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved left", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column < initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_increase_down() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);
    focus_upper_pane(&zellij);
    let initial_rows = height(&upper_terminal, "upper terminal initial height");
    let initial_boundary = horizontal_boundary_row(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('j'));

    upper_terminal.wait_for_size("upper terminal grew downward", |_, rows| {
        rows > initial_rows
    });
    let grid_snapshot = zellij.wait_until("pane boundary moved down", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_row(grid_snapshot).is_some_and(|row| row > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_increase_up() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let lower_terminal = split_down_and_wait_for_prompt(&zellij);
    let initial_rows = height(&lower_terminal, "lower terminal initial height");
    let initial_boundary = horizontal_boundary_row(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('k'));

    lower_terminal.wait_for_size("lower terminal grew upward", |_, rows| rows > initial_rows);
    let grid_snapshot = zellij.wait_until("pane boundary moved up", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_row(grid_snapshot).is_some_and(|row| row < initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_down() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);
    focus_upper_pane(&zellij);
    let initial_rows = height(&upper_terminal, "upper terminal initial height");
    let initial_boundary = horizontal_boundary_row(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('J'));

    upper_terminal.wait_for_size("upper terminal shrank", |_, rows| rows < initial_rows);
    let grid_snapshot = zellij.wait_until("pane boundary moved up", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_row(grid_snapshot).is_some_and(|row| row < initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_up() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let lower_terminal = split_down_and_wait_for_prompt(&zellij);
    let initial_rows = height(&lower_terminal, "lower terminal initial height");
    let initial_boundary = horizontal_boundary_row(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('K'));

    lower_terminal.wait_for_size("lower terminal shrank", |_, rows| rows < initial_rows);
    let grid_snapshot = zellij.wait_until("pane boundary moved down", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_row(grid_snapshot).is_some_and(|row| row > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_increase_in_resize_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = width(&right_terminal, "right terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('='));

    right_terminal.wait_for_size("right terminal grew", |cols, _| cols > initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved left", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column < initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_in_resize_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = width(&right_terminal, "right terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    resize_in_resize_mode(&zellij, &keys::key('-'));

    right_terminal.wait_for_size("right terminal shrank", |cols, _| cols < initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved right", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_increase_in_normal_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = width(&right_terminal, "right terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    zellij.send_stdin(&keys::alt('='));

    right_terminal.wait_for_size("right terminal grew", |cols, _| cols > initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved left", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column < initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_decrease_in_normal_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let initial_cols = width(&right_terminal, "right terminal initial width");
    let initial_boundary = vertical_boundary_column(&zellij);

    zellij.send_stdin(&keys::alt('-'));

    right_terminal.wait_for_size("right terminal shrank", |cols, _| cols < initial_cols);
    let grid_snapshot = zellij.wait_until("pane boundary moved right", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && pane_two_header_column(grid_snapshot).is_some_and(|column| column > initial_boundary)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
