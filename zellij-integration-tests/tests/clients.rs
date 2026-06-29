#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, start_zellij, GridSnapshot,
    TestRunner, PROMPT, TERMINAL_SIZE,
};

fn session_name_rendered(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("(test-")
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
            && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('p'));
    second_client.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let mirror_focused_right_pane = |grid_snapshot: &GridSnapshot| {
        grid_snapshot.tab_bar_appears()
            && session_name_rendered(grid_snapshot)
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.cursor_is_at(col(62).row(2))
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
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("pane_frame_style \"full\"")
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("$ ")
    });

    let second_client = zellij.attach_client(TERMINAL_SIZE);
    let second_grid =
        second_client.wait_until("second client shares the focused pane", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(col(3).row(2))
        });
    let main_grid = zellij.wait_until(
        "main client shows the shared-focus indicator",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("MY FOCUS")
                && grid_snapshot.cursor_is_at(col(3).row(2))
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
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('p'));
    second_client.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let second_grid = second_client.wait_until(
        "second client focused the new right pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(62).row(2))
        },
    );
    let main_grid = zellij.wait_until(
        "main client sees the second client's split while staying on the left pane",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(2))
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
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    second_client.send_stdin(&keys::ctrl('t'));
    second_client.send_stdin(&keys::key('n'));
    let second_tab_terminal = zellij.expect_pty_spawn();
    second_tab_terminal.output(PROMPT);

    let second_grid =
        second_client.wait_until("second client moved to the new tab", |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(col(2).row(1))
        });
    let main_grid = zellij.wait_until(
        "main client sees the new tab while staying on the first tab",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(col(2).row(1))
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

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
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
                && session_name_rendered(grid_snapshot)
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.contains("I am some text")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    reattached_client.quit();
    zellij.quit();
}
