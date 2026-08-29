#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, start_zellij, GridSnapshot,
    TestSession,
};

const ARROW_DOWN: &[u8] = b"\x1b[B";

fn open_presets_screen(zellij: &TestSession) {
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("presets screen opened", |grid_snapshot| {
        grid_snapshot.contains("1. Default") && grid_snapshot.contains("2. Unlock First")
    });
}

fn apply_selected_preset_and_close(
    zellij: &TestSession,
    times_to_move_down: usize,
    applied_marker: &str,
) {
    for _ in 0..times_to_move_down {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.send_stdin(&keys::ENTER);
    let applied_marker = applied_marker.to_owned();
    zellij.wait_until("preset applied to the running session", |grid_snapshot| {
        grid_snapshot.contains(&applied_marker)
    });
    zellij.send_stdin(&keys::ctrl('c'));
    zellij.send_stdin(&keys::ctrl('c'));
    zellij.wait_until("configuration plugin closed", |grid_snapshot| {
        !grid_snapshot.contains("Configuration")
    });
}

fn wait_for_unlock_first_status_bar(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until(
        "status bar groups keys the unlock-first way",
        |grid_snapshot| grid_snapshot.contains("PANE") && !grid_snapshot.contains("Ctrl +"),
    )
}

fn wait_for_default_status_bar(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until("status bar groups keys the default way", |grid_snapshot| {
        grid_snapshot.contains("PANE") && grid_snapshot.contains("Ctrl +")
    })
}

#[test]
fn switching_keybind_presets_updates_the_status_bar_without_a_new_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_presets_screen(&zellij);
    apply_selected_preset_and_close(&zellij, 2, "UNLOCK");

    zellij.send_stdin(&keys::ctrl('g'));
    let grid_snapshot = wait_for_unlock_first_status_bar(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.send_stdin(&keys::key('o'));
    zellij.send_stdin(&keys::key('c'));
    open_presets_screen(&zellij);
    apply_selected_preset_and_close(&zellij, 1, "Ctrl +");

    let grid_snapshot = wait_for_default_status_bar(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.quit();
}
