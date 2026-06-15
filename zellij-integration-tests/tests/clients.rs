#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    keys, normalized, FakePtyHandle, GridSnapshot, Size, TestRunner, TestSession,
};

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
fn mirrored_sessions() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mirror_session true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });

    second_client.send_stdin(&keys::PANE_MODE);
    second_client.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let mirror_focused_right_pane = |grid_snapshot: &GridSnapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    };
    let main_grid = zellij.wait_until(
        "main client follows the second client's focus into the split it never asked for",
        mirror_focused_right_pane,
    );
    let second_grid = second_client.wait_until(
        "second client shows the split it created",
        mirror_focused_right_pane,
    );
    assert_eq!(
        normalized(&main_grid),
        normalized(&second_grid),
        "mirrored clients must render an identical view"
    );
    assert_snapshot!(normalized(&main_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_same_pane_and_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    let second_grid =
        second_client.wait_until("second client shares the focused pane", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        });
    let main_grid = zellij.wait_until(
        "main client shows the shared-focus indicator",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_different_panes_and_same_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });

    second_client.send_stdin(&keys::PANE_MODE);
    second_client.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let second_grid = second_client.wait_until(
        "second client focused the new right pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    let main_grid = zellij.wait_until(
        "main client sees the second client's split while staying on the left pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn multiple_users_in_different_tabs() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded on the first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });

    second_client.send_stdin(&keys::TAB_MODE);
    second_client.send_stdin(&keys::NEW_TAB_IN_TAB_MODE);
    let second_tab_terminal = zellij.expect_pty_spawn();
    second_tab_terminal.output(PROMPT);

    let second_grid =
        second_client.wait_until("second client moved to the new tab", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        });
    let main_grid = zellij.wait_until(
        "main client sees the new tab while staying on the first tab",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    assert_snapshot!(normalized(&main_grid));
    assert_snapshot!(normalized(&second_grid));
    second_client.quit();
    zellij.quit();
}

#[test]
fn detach_and_attach_session() {
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

    right_terminal.output(b"I am some text");
    zellij.wait_until("text rendered in the right terminal", |grid_snapshot| {
        grid_snapshot.contains("I am some text")
    });

    zellij.detach_main_client();

    let reattached_client = zellij.attach_client(TERMINAL_SIZE);
    let grid_snapshot = reattached_client.wait_until(
        "reattached client sees the restored split and text",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.contains("I am some text")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    reattached_client.quit();
    zellij.quit();
}
