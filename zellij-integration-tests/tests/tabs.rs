#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, start_zellij, FakePtyHandle,
    GridSnapshot, TestSession, PROMPT,
};

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
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    let expected_tab = expected_tab.to_owned();
    zellij.wait_until("new tab opened with prompt", move |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(&expected_tab)
            && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    terminal
}

fn label_first_tab_pane(zellij: &TestSession, terminal: &FakePtyHandle, marker: &str) {
    terminal.output(marker.as_bytes());
    let marker = marker.to_owned();
    zellij.wait_until("first tab pane labelled", move |grid_snapshot| {
        grid_snapshot.contains(&marker)
    });
}

fn open_marked_tab(zellij: &TestSession, expected_tab: &str, marker: &str) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    let expected_tab = expected_tab.to_owned();
    let marker = marker.to_owned();
    zellij.wait_until("new marked tab opened", move |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(&expected_tab)
            && grid_snapshot.contains(&marker)
    });
    terminal
}

#[test]
fn go_to_previous_tab() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "oneone");
    open_marked_tab(&zellij, "Tab #2", "twotwo");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('h'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("previous tab focused", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.contains("oneone")
            && !grid_snapshot.contains("twotwo")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn go_to_next_tab() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "oneone");
    open_marked_tab(&zellij, "Tab #2", "twotwo");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('h'));
    zellij.wait_until("previous tab focused", |grid_snapshot| {
        grid_snapshot.contains("oneone") && !grid_snapshot.contains("twotwo")
    });

    zellij.send_stdin(&keys::key('l'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("next tab focused again", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.contains("twotwo")
            && !grid_snapshot.contains("oneone")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn go_to_tab_by_number() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "oneone");
    open_marked_tab(&zellij, "Tab #2", "twotwo");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('1'));

    let grid_snapshot = zellij.wait_until("first tab focused by number", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("oneone")
            && !grid_snapshot.contains("twotwo")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_tab_returns_to_last_used() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "oneone");
    open_marked_tab(&zellij, "Tab #2", "twotwo");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("toggled back to the first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.contains("oneone")
            && !grid_snapshot.contains("twotwo")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_active_sync_tab() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("second pane spawned in first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("Pane #2")
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('s'));

    let grid_snapshot = zellij.wait_until("tab marked as syncing", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("SYNC")
    });
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.send_stdin(&keys::ENTER);
    zellij.send_stdin(b"synced-input");

    let first_stdin = first_terminal.wait_for_stdin("synced input reached first pane", |stdin| {
        stdin.windows(12).any(|window| window == b"synced-input")
    });
    let second_stdin =
        second_terminal.wait_for_stdin("synced input reached second pane", |stdin| {
            stdin.windows(12).any(|window| window == b"synced-input")
        });
    assert!(first_stdin.windows(12).any(|window| window == b"synced-input"));
    assert!(second_stdin.windows(12).any(|window| window == b"synced-input"));
    zellij.quit();
}

#[test]
fn break_pane_into_new_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("second pane spawned in first tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("Pane #2")
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('b'));

    let grid_snapshot = zellij.wait_until("focused pane broken into a new tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("Pane #1")
            && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn break_pane_to_the_right() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "alpha");

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('n'));
    let beta_terminal = zellij.expect_pty_spawn();
    beta_terminal.output(PROMPT);
    beta_terminal.output(b"beta");
    zellij.wait_until("second pane spawned in first tab", |grid_snapshot| {
        grid_snapshot.contains("beta") && grid_snapshot.contains("Pane #2")
    });

    open_marked_tab(&zellij, "Tab #2", "gamma");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('h'));
    zellij.wait_until("back on the first tab with its panes", |grid_snapshot| {
        grid_snapshot.contains("beta") && !grid_snapshot.contains("gamma")
    });

    zellij.send_stdin(&keys::key(']'));

    let grid_snapshot = zellij.wait_until("beta pane moved into the right tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("beta")
            && grid_snapshot.contains("gamma")
            && !grid_snapshot.contains("alpha")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn break_pane_to_the_left() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    label_first_tab_pane(&zellij, &first_terminal, "alpha");

    open_marked_tab(&zellij, "Tab #2", "beta");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('['));

    let grid_snapshot = zellij.wait_until("beta pane moved into the left tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("alpha")
            && grid_snapshot.contains("beta")
            && !grid_snapshot.contains("Tab #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn open_new_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");
    let grid_snapshot = zellij.wait_until("second tab steady in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn close_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_new_tab_and_wait_for_prompt(&zellij, "Tab #2");

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('x'));

    let grid_snapshot = zellij.wait_until(
        "second tab closed, only first tab remains",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Tab #1")
                && !grid_snapshot.contains("Tab #2")
                && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn undo_rename_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('r'));
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

    zellij.send_stdin(&keys::alt('i'));

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

    zellij.send_stdin(&keys::alt('h'));
    zellij.send_stdin(&keys::alt('o'));

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

    zellij.send_stdin(&keys::alt('i'));
    zellij.wait_until("third tab moved one position left", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #1", "Tab #3", "Tab #2"])
    });
    zellij.send_stdin(&keys::alt('i'));
    zellij.wait_until("third tab moved to the beginning", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #3", "Tab #1", "Tab #2"])
    });
    zellij.send_stdin(&keys::alt('i'));

    let grid_snapshot = zellij.wait_until("third tab wrapped to the end", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #1", "Tab #2", "Tab #3"])
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

    zellij.send_stdin(&keys::alt('o'));

    let grid_snapshot = zellij.wait_until("third tab wrapped to the beginning", |grid_snapshot| {
        tabs_in_order(grid_snapshot, &["Tab #3", "Tab #1", "Tab #2"])
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
