#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, start_zellij, GridSnapshot, TestRunner, TestSession,
    TERMINAL_SIZE,
};

#[test]
fn quit_with_keybinding() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('q'));

    zellij.wait_until("zellij exited after the quit keybinding", |grid_snapshot| {
        grid_snapshot.contains("Bye from Zellij!")
    });
}

#[test]
fn write_byte_to_focused_pane_via_tmux_mode() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('b'));
    zellij.send_stdin(&keys::ctrl('b'));

    let stdin_bytes = terminal.wait_for_stdin("ctrl-b byte reached the pane", |stdin_bytes| {
        stdin_bytes.contains(&0x02)
    });
    assert!(stdin_bytes.contains(&0x02));
    let grid_snapshot = zellij.wait_until("mode settled back to normal", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn focus_next_pane_via_tmux_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('b'));
    zellij.send_stdin(&keys::key('o'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus cycled to the other pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_group_marking() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('P'));

    let grid_snapshot = zellij.wait_until("group marking engaged", |grid_snapshot| {
        grid_snapshot.contains("GROUP ACTIONS")
            && grid_snapshot.contains("SELECTED PANE")
            && grid_snapshot.contains("Follow Focus")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_pane_in_group() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('p'));

    let grid_snapshot = zellij.wait_until("pane added to group", |grid_snapshot| {
        grid_snapshot.contains("GROUP ACTIONS")
            && grid_snapshot.contains("SELECTED PANE")
            && grid_snapshot.contains("Follow Focus")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

fn start_zellij_with_layout_notifications() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("swap_layout_notification true")
        .start()
}

fn draws_a_popup_border(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("╭") || grid_snapshot.contains("╰")
}

fn wait_for_layout_notification(zellij: &TestSession, layout_name: &str) {
    let expected = format!("layout: {}", layout_name);
    let broken_frames = std::cell::RefCell::new(Vec::new());
    zellij.wait_until(
        &format!("notification for {}", layout_name),
        |grid_snapshot| {
            let shown = grid_snapshot.text.to_lowercase().contains(&expected);
            if !grid_snapshot.contains("Layout:") && draws_a_popup_border(grid_snapshot) {
                broken_frames.borrow_mut().push(grid_snapshot.text.clone());
            }
            shown
        },
    );
    let broken_frames = broken_frames.into_inner();
    assert!(
        broken_frames.is_empty(),
        "a notification was drawn without its text:\n{}",
        broken_frames.join("\n---\n")
    );
}

fn wait_for_notifications_to_close(zellij: &TestSession) -> GridSnapshot {
    let broken_frames = std::cell::RefCell::new(Vec::new());
    let grid_snapshot = zellij.wait_until("layout notifications closed", |grid_snapshot| {
        let text_gone = !grid_snapshot.contains("Layout:");
        let border_left = draws_a_popup_border(grid_snapshot);
        if text_gone && border_left {
            broken_frames.borrow_mut().push(grid_snapshot.text.clone());
        }
        grid_snapshot.status_bar_appears() && text_gone && !border_left
    });
    let broken_frames = broken_frames.into_inner();
    assert!(
        broken_frames.is_empty(),
        "a notification border stayed on screen without its text:\n{}",
        broken_frames.join("\n---\n")
    );
    grid_snapshot
}

#[test]
fn next_swap_layout() {
    let mut zellij = start_zellij_with_layout_notifications();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt(']'));
    wait_for_layout_notification(&zellij, "vertical");
    zellij.send_stdin(&keys::alt(']'));
    wait_for_layout_notification(&zellij, "horizontal");
    let grid_snapshot = wait_for_notifications_to_close(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn previous_swap_layout() {
    let mut zellij = start_zellij_with_layout_notifications();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('['));
    wait_for_layout_notification(&zellij, "stacked");
    let grid_snapshot = wait_for_notifications_to_close(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
