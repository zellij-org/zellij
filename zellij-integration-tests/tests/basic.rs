#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{keys, normalized, Size, TestRunner};

const PROMPT: &[u8] = b"$ ";
const CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE: usize = 3;
const CURSOR_X_AFTER_PROMPT_IN_RIGHT_PANE: usize = 63;

#[test]
fn starts_with_one_terminal() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .start();
    let shell = zellij.expect_pty_spawn();
    shell.output(PROMPT);
    let grid_snapshot =
        zellij.wait_until("app loaded with cursor after the prompt", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.tab_bar_appears()
                && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE, 2)
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn split_terminals_vertically() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .start();
    let first_shell = zellij.expect_pty_spawn();
    first_shell.output(PROMPT);
    zellij.wait_until("app loaded with cursor after the prompt", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE, 2)
    });

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let second_shell = zellij.expect_pty_spawn();
    second_shell.output(PROMPT);

    let grid_snapshot = zellij.wait_until("cursor in the new right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_RIGHT_PANE, 2)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resize_pane() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .start();
    let first_shell = zellij.expect_pty_spawn();
    first_shell.output(PROMPT);
    zellij.wait_until("app loaded with cursor after the prompt", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE, 2)
    });

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let second_shell = zellij.expect_pty_spawn();
    second_shell.output(PROMPT);
    zellij.wait_until("cursor in the new right pane", |grid_snapshot| {
        grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_RIGHT_PANE, 2)
    });
    let (initial_cols, _) =
        second_shell.wait_for_size("initial size of the right pane", |_, _| true);

    zellij.send_stdin(&keys::RESIZE_MODE);
    zellij.send_stdin(&keys::RESIZE_LEFT_IN_RESIZE_MODE);
    zellij.send_stdin(&keys::ENTER);

    let (resized_cols, _) = second_shell
        .wait_for_size("right pane grows after resize left", |cols, _| {
            cols > initial_cols
        });
    first_shell.wait_for_size("left pane shrinks after resize left", |cols, _| {
        cols < initial_cols
    });

    let right_pane_growth = (resized_cols - initial_cols) as usize;
    let expected_cursor_x = CURSOR_X_AFTER_PROMPT_IN_RIGHT_PANE - right_pane_growth;
    let grid_snapshot =
        zellij.wait_until("resized layout rendered in normal mode", |grid_snapshot| {
            grid_snapshot.cursor_is_at(expected_cursor_x, 2) && grid_snapshot.status_bar_appears()
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn command_pane_closes_on_exit() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .start();
    let first_shell = zellij.expect_pty_spawn();
    first_shell.output(b"$ I am a fake shell\r\n$ ");
    zellij.wait_until("first pane output rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("I am a fake shell")
            && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE, 3)
    });

    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let second_shell = zellij.expect_pty_spawn();
    second_shell.output(b"$ about to exit\r\n$ ");
    zellij.wait_until("second pane output rendered", |grid_snapshot| {
        grid_snapshot.contains("about to exit")
            && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_RIGHT_PANE, 3)
    });

    second_shell.exit(Some(0));
    zellij.wait_until(
        "second pane closed, focus back in first pane",
        |grid_snapshot| {
            !grid_snapshot.contains("about to exit")
                && grid_snapshot.cursor_is_at(CURSOR_X_AFTER_PROMPT_IN_FIRST_PANE, 3)
        },
    );
    zellij.quit();
}
