#![cfg(unix)]

use zellij_integration_tests::{keys, GridSnapshot, NestedHarness, PROMPT, TERMINAL_SIZE};

fn modal_visible(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("Nested Zellij session detected")
}

fn descend_config() -> &'static str {
    r#"nested_session_handling "descend""#
}

fn fullscreen_config() -> &'static str {
    r#"nested_session_handling "fullscreen""#
}

fn never_config() -> &'static str {
    r#"nested_session_handling "never"
keybinds {
    normal {
        bind "Ctrl y" { FocusGuestSession; }
    }
}"#
}

#[test]
fn descend_mode_auto_descends_without_modal() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, descend_config());

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.wait_for_host_to_descend_into_guest();
    nested
        .host
        .wait_until("no modal is shown in descend mode", |host_grid| {
            !modal_visible(host_grid)
        });

    nested.guest.wait_for_app_load();
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after auto-descend",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    nested.guest.quit();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();
    nested.host.quit();
}

#[test]
fn fullscreen_mode_auto_zooms_without_modal() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, fullscreen_config());

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.wait_for_host_to_descend_into_guest();
    nested
        .host
        .wait_until("no modal is shown in fullscreen mode", |host_grid| {
            !modal_visible(host_grid)
        });

    nested.guest.wait_for_app_load();
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after auto-zoom",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    nested.guest.quit();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();
    nested.host.quit();
}

#[test]
fn never_mode_shows_no_modal_and_never_descends() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, never_config());

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.guest.wait_for_app_load();

    nested
        .host
        .wait_until("no modal is shown in never mode", |host_grid| {
            !modal_visible(host_grid)
        });
    assert_eq!(
        nested.focus_gained_count(),
        0,
        "never mode must not descend into the guest by itself"
    );

    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn never_mode_allows_manual_descend() {
    let mut nested = NestedHarness::start_with_host_config(TERMINAL_SIZE, never_config());

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.guest.wait_for_app_load();
    assert_eq!(nested.focus_gained_count(), 0);

    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('y'));
    nested.wait_for_host_to_descend_into_guest_after(descended);

    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after manual FocusGuestSession",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    nested.guest.quit();
    nested.wait_for_host_to_reclaim_focus_after_guest_exit();
    nested.host.quit();
}
