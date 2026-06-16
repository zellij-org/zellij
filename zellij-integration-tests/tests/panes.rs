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

fn split_right_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });
    terminal
}

#[test]
fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let mut zellij = TestRunner::new(Size { cols: 8, rows: 20 }).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });
    let (width_before_split, _) = terminal.wait_for_size("first terminal sized", |_, _| true);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);

    terminal.output(b"done");
    let tab_bar_glyph = '\u{e0b0}';
    let grid_snapshot = zellij.wait_until(
        "split attempt processed, chrome rendered",
        |grid_snapshot| {
            grid_snapshot.contains("done")
                && grid_snapshot.contains("Ctrl +")
                && grid_snapshot
                    .lines()
                    .first()
                    .is_some_and(|first_line| first_line.contains(tab_bar_glyph))
        },
    );

    assert_eq!(terminal.size().unwrap().0, width_before_split);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn key_after_mode_switch_is_interpreted_in_new_mode_not_leaked_to_pane() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);

    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("split-right interpreted in pane mode", |grid_snapshot| {
        grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });

    assert!(!first_terminal
        .stdin_bytes()
        .contains(&keys::SPLIT_RIGHT_IN_PANE_MODE[0]));
    zellij.quit();
}

#[test]
fn toggle_pane_fullscreen() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::TOGGLE_ACTIVE_TERMINAL_FULLSCREEN_IN_PANE_MODE);

    let grid_snapshot = zellij.wait_until("focused pane is fullscreen", |grid_snapshot| {
        grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
            && grid_snapshot.contains("LOCK")
            && grid_snapshot.contains("(FULLSCREEN)")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn close_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::CLOSE_PANE_IN_PANE_MODE);

    let grid_snapshot =
        zellij.wait_until("right pane closed, focus back on first", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn closing_last_pane_exits_zellij() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::CLOSE_PANE_IN_PANE_MODE);

    zellij.wait_until(
        "zellij exited after closing the last pane",
        |grid_snapshot| grid_snapshot.contains("Bye from Zellij!"),
    );
}

#[test]
fn pane_closes_when_its_process_exits() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    right_terminal.exit(Some(0));

    let grid_snapshot =
        zellij.wait_until("right pane closed, focus back on first", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_floating_panes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::TOGGLE_FLOATING_PANES);
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("floating pane appeared", |grid_snapshot| {
        grid_snapshot.cursor_is_at(33, 8)
            && grid_snapshot.contains("STAGGERED")
            && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn undo_rename_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::RENAME_PANE_MODE);
    zellij.send_stdin(b"aa");
    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(&keys::ESC);

    let grid_snapshot = zellij.wait_until("pane name reverted to default", |grid_snapshot| {
        grid_snapshot.contains("Pane #1") && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn start_without_pane_frames() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("pane_frames false")
        .start();
    let first_terminal = zellij.expect_pty_spawn();
    first_terminal.output(PROMPT);
    zellij.wait_until(
        "first frameless terminal prompt rendered",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(2, 1)
        },
    );

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until(
        "right frameless terminal prompt rendered",
        |grid_snapshot| grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(62, 1),
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

