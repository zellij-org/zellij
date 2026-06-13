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

fn tabs_in_order(grid_snapshot: &GridSnapshot, labels: &[&str]) -> bool {
    let mut search_from = 0;
    for label in labels {
        match grid_snapshot.text[search_from..].find(label) {
            Some(offset) => search_from += offset + label.len(),
            None => return false,
        }
    }
    true
}

fn open_new_tab_and_wait_for_prompt(zellij: &TestSession, expected_tab: &str) -> FakePtyHandle {
    zellij.send_stdin(&keys::TAB_MODE);
    zellij.send_stdin(&keys::NEW_TAB_IN_TAB_MODE);
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    let expected_tab = expected_tab.to_owned();
    zellij.wait_until("new tab opened with prompt", move |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(&expected_tab)
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });
    terminal
}

#[test]
fn open_new_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    let grid_snapshot = zellij.wait_until("second tab steady in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn close_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");

    zellij.send_stdin(&keys::TAB_MODE);
    zellij.send_stdin(&keys::CLOSE_TAB_IN_TAB_MODE);

    let grid_snapshot = zellij.wait_until("second tab closed, only first tab remains", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && !grid_snapshot.contains("Tab #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn undo_rename_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::TAB_MODE);
    zellij.send_stdin(&keys::RENAME_TAB_MODE);
    zellij.send_stdin(b"aa");
    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(&keys::ESC);

    let grid_snapshot = zellij.wait_until("tab name reverted to default", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_tab_to_left() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #3");

    zellij.send_stdin(&keys::MOVE_TAB_LEFT);

    let grid_snapshot = zellij.wait_until("third tab moved one position left", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #1", "Tab #3", "Tab #2"])
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_tab_to_right() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #3");

    zellij.send_stdin(&keys::MOVE_FOCUS_LEFT_IN_NORMAL_MODE);
    zellij.send_stdin(&keys::MOVE_TAB_RIGHT);

    let grid_snapshot = zellij.wait_until("second tab moved one position right", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #1", "Tab #3", "Tab #2"])
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_tab_to_left_until_it_wraps_around() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #3");

    zellij.send_stdin(&keys::MOVE_TAB_LEFT);
    zellij.wait_until("third tab moved one position left", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #1", "Tab #3", "Tab #2"])
    });
    zellij.send_stdin(&keys::MOVE_TAB_LEFT);
    zellij.wait_until("third tab moved to the beginning", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #3", "Tab #1", "Tab #2"])
    });
    zellij.send_stdin(&keys::MOVE_TAB_LEFT);

    let grid_snapshot = zellij.wait_until("third tab wrapped to the end", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #2", "Tab #1", "Tab #3"])
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_tab_to_right_until_it_wraps_around() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #3");

    zellij.send_stdin(&keys::MOVE_TAB_RIGHT);

    let grid_snapshot = zellij.wait_until("third tab wrapped to the beginning", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #3", "Tab #2", "Tab #1"])
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
