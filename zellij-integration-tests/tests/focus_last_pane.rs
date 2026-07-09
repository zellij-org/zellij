#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_down_and_wait_for_prompt, split_right_and_wait_for_prompt, FakePtyHandle, Size,
    TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const FOCUS_KEYS_CONFIG: &str = r#"
keybinds {
    normal {
        bind "Alt m" { FocusLastPane; }
        bind "Alt ." { FocusNextPane; }
    }
}
"#;

fn start_with_focus_keys(size: Size) -> TestSession {
    TestRunner::new(size).with_config(FOCUS_KEYS_CONFIG).start()
}

fn split_focused_pane_down(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until(
        "newly split lower pane focused below the middle pane",
        |grid| grid.status_bar_appears() && grid.cursor.map_or(false, |cursor| cursor.y > 13),
    );
    terminal
}

#[test]
fn focus_last_pane_toggles_focus_back_to_the_previous_pane() {
    let mut zellij = start_with_focus_keys(TERMINAL_SIZE);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('m'));
    let grid = zellij.wait_until(
        "focus_last_pane moved focus back to the left pane",
        |grid| grid.status_bar_appears() && grid.cursor_is_at(col(2).row(2)),
    );
    assert_snapshot!(normalized(&grid));
    zellij.quit();
}

#[test]
fn focus_last_pane_returns_to_earlier_pane_after_closing_focused_pane() {
    let mut zellij = start_with_focus_keys(TERMINAL_SIZE);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);
    let bottom_terminal = split_focused_pane_down(&zellij);

    bottom_terminal.exit(Some(0));
    zellij.wait_until(
        "closing the bottom pane returns focus to the middle pane",
        |grid| grid.status_bar_appears() && grid.cursor_is_at(col(2).row(13)),
    );

    zellij.send_stdin(&keys::alt('m'));
    zellij.wait_until(
        "focus_last_pane moves focus to the pane active before the closed one",
        |grid| grid.status_bar_appears() && grid.cursor_is_at(col(2).row(2)),
    );
    zellij.quit();
}

#[test]
fn focus_last_pane_refills_after_cycling_focus() {
    let mut zellij = start_with_focus_keys(TERMINAL_SIZE);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('.'));
    zellij.wait_until("focus cycled away from the right pane", |grid| {
        grid.status_bar_appears() && grid.cursor.map_or(false, |cursor| cursor.x < 60)
    });

    zellij.send_stdin(&keys::alt('m'));
    zellij.wait_until(
        "focus_last_pane returns to the pane focused before cycling",
        |grid| grid.status_bar_appears() && grid.cursor_is_at(col(62).row(2)),
    );
    zellij.quit();
}

fn open_floating_pane_below(
    zellij: &TestSession,
    previous_cursor_row: usize,
) -> (FakePtyHandle, usize) {
    zellij.send_stdin(&keys::alt('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    let grid = zellij.wait_until("new floating pane focused below the previous one", |grid| {
        grid.cursor
            .map_or(false, |cursor| cursor.y > previous_cursor_row)
    });
    (terminal, grid.cursor.unwrap().y)
}

#[test]
fn focus_last_floating_pane_returns_to_earlier_pane_after_closing_focused_pane() {
    let mut zellij = start_with_focus_keys(TERMINAL_SIZE);
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let first_floating = zellij.expect_pty_spawn();
    first_floating.output(PROMPT);
    let first_floating_cursor = zellij
        .wait_until("first floating pane focused", |grid| {
            grid.contains("Pane #2") && grid.cursor.is_some()
        })
        .cursor
        .unwrap();

    let (_second_floating, second_floating_row) =
        open_floating_pane_below(&zellij, first_floating_cursor.y);
    let (third_floating, _third_floating_row) =
        open_floating_pane_below(&zellij, second_floating_row);

    third_floating.exit(Some(0));
    zellij.wait_until(
        "closing the focused floating pane returns focus to the previous floating pane",
        |grid| {
            grid.cursor
                .map_or(false, |cursor| cursor.y == second_floating_row)
        },
    );

    zellij.send_stdin(&keys::alt('m'));
    zellij.wait_until(
        "focus_last_pane moves focus to the floating pane active before the closed one",
        |grid| grid.cursor == Some(first_floating_cursor),
    );
    zellij.quit();
}

#[test]
fn focus_last_pane_with_floating_visible_leaves_tiled_focus_untouched() {
    let mut zellij = start_with_focus_keys(TERMINAL_SIZE);
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    zellij.wait_until("floating pane focused over the tiled panes", |grid| {
        grid.contains("Pane #3") && grid.cursor.is_some()
    });

    zellij.send_stdin(&keys::alt('m'));

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until(
        "the tiled pane focused before opening floating panes stays focused",
        |grid| grid.status_bar_appears() && grid.cursor_is_at(col(62).row(2)),
    );
    zellij.quit();
}
