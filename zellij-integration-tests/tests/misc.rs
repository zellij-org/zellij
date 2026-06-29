#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, start_zellij,
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

#[test]
fn next_swap_layout() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    zellij.wait_until("swap layout at base", |grid_snapshot| {
        grid_snapshot.contains("BASE")
    });

    zellij.send_stdin(&keys::alt(']'));

    zellij.wait_until("swap layout advanced to vertical", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("VERTICAL")
            && !grid_snapshot.contains("BASE")
    });
    zellij.send_stdin(&keys::alt(']'));
    let grid_snapshot = zellij.wait_until("swap layout advanced to horizontal", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("VERTICAL")
            && grid_snapshot.contains("HORIZONTAL")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn previous_swap_layout() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    zellij.wait_until("swap layout at base", |grid_snapshot| {
        grid_snapshot.contains("BASE")
    });

    zellij.send_stdin(&keys::alt('['));

    let grid_snapshot =
        zellij.wait_until("swap layout moved back to horizontal", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("HORIZONTAL")
                && !grid_snapshot.contains("BASE")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
