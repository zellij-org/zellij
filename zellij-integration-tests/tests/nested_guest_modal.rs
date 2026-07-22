#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{keys, normalized, GridSnapshot, NestedHarness, PROMPT, TERMINAL_SIZE};

const ARROW_DOWN: &[u8] = b"\x1b[B";

fn last_line_contains(grid_snapshot: &GridSnapshot, needle: &str) -> bool {
    grid_snapshot
        .lines()
        .last()
        .map_or(false, |last_line| last_line.contains(needle))
}

fn normal_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "LOCK")
}

fn pane_mode_bar_settled(grid_snapshot: &GridSnapshot) -> bool {
    last_line_contains(grid_snapshot, "Fullscreen")
}

fn modal_visible(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains("Nested Zellij session detected")
}

fn grid_contains_line_starting_with(grid_snapshot: &GridSnapshot, prefix: &str) -> bool {
    grid_snapshot
        .lines()
        .iter()
        .any(|line| line.trim_start().starts_with(prefix))
}

fn wait_for_modal(nested: &NestedHarness) -> GridSnapshot {
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested
        .host
        .wait_until("host shows the guest-detected modal", modal_visible)
}

fn focus_guest_binding_config() -> &'static str {
    r#"keybinds {
    normal {
        bind "Ctrl y" { FocusGuestSession; }
    }
}"#
}

fn modal_shortcut_binding_config() -> &'static str {
    r#"keybinds {
    normal {
        bind "Ctrl y" { ToggleHostFullscreen; }
        bind "Ctrl u" { FocusHostSession; }
        bind "Ctrl i" { FocusGuestSession; }
    }
}"#
}

#[test]
fn modal_renders_expected_ui() {
    let mut nested =
        NestedHarness::start_with_host_config(TERMINAL_SIZE, modal_shortcut_binding_config());

    let host_with_modal = wait_for_modal(&nested);
    assert!(host_with_modal.contains("What would you like to do?"));
    assert!(host_with_modal.contains("Zoom in and control this session"));
    assert!(host_with_modal.contains("<Ctrl y>"));
    assert!(host_with_modal.contains("<Ctrl u>"));
    assert!(host_with_modal.contains("<Ctrl i>"));
    assert!(host_with_modal.contains("<↓↑> select"));

    let steady_modal = nested.host.wait_until(
        "guest-detected modal fully rendered with all options and keybindings",
        |host_grid| {
            modal_visible(host_grid)
                && host_grid.contains("What would you like to do?")
                && host_grid.contains("(AUTO)")
                && host_grid.contains("(MANUAL)")
                && host_grid.contains("<Ctrl y>")
                && host_grid.contains("<Ctrl u>")
                && host_grid.contains("<Ctrl i>")
                && host_grid.contains("<↓↑> select")
        },
    );
    assert_snapshot!(normalized(&steady_modal));

    nested.guest.wait_for_app_load();
    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn modal_selection_moves_with_arrow_keys() {
    let mut nested =
        NestedHarness::start_with_host_config(TERMINAL_SIZE, modal_shortcut_binding_config());

    wait_for_modal(&nested);
    let first_selection = nested.host.wait_until(
        "first option selected on initial render",
        |host_grid| grid_contains_line_starting_with(host_grid, "> 1."),
    );
    assert!(grid_contains_line_starting_with(&first_selection, "> 1."));
    assert!(!grid_contains_line_starting_with(&first_selection, "> 2."));

    nested.host.send_stdin(ARROW_DOWN);
    let second_selection = nested.host.wait_until(
        "selection moved to the second option after arrow down",
        |host_grid| grid_contains_line_starting_with(host_grid, "> 2."),
    );
    assert!(grid_contains_line_starting_with(&second_selection, "> 2."));
    assert!(!grid_contains_line_starting_with(&second_selection, "> 1."));

    nested.guest.wait_for_app_load();
    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn modal_appears_on_announce_and_occludes_guest_content() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    let host_with_modal = wait_for_modal(&nested);
    assert!(modal_visible(&host_with_modal));
    assert!(host_with_modal.contains("Zoom in and control this session"));
    assert!(host_with_modal.contains("(AUTO)"));
    assert!(host_with_modal.contains("(MANUAL)"));
    assert_eq!(
        nested.focus_gained_count(),
        0,
        "the modal must not enter passthrough by itself"
    );

    nested.guest.wait_for_app_load();
    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn selecting_descend_enters_passthrough() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    wait_for_modal(&nested);
    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(ARROW_DOWN);
    nested.host.send_stdin(b"\r");
    nested.wait_for_host_to_descend_into_guest_after(descended);
    nested.host.wait_until("modal dismissed after descend", |host_grid| {
        !modal_visible(host_grid)
    });

    nested.guest.wait_for_app_load();
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after descend",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.quit();
}

#[test]
fn digit_one_zooms_and_enters_passthrough() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    wait_for_modal(&nested);
    let zoomed = nested.mark_host_to_guest();
    nested.host.send_stdin(b"1");
    nested.wait_for_host_to_descend_into_guest_after(zoomed);
    nested.host.wait_until("modal dismissed after zoom", |host_grid| {
        !modal_visible(host_grid)
    });

    nested.guest.wait_for_app_load();
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after zoom",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.quit();
}

#[test]
fn dismissing_the_modal_keeps_host_keybindings_working() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    wait_for_modal(&nested);
    nested.dismiss_guest_modal();
    nested.host.wait_until("modal dismissed", |host_grid| {
        !modal_visible(host_grid)
    });
    assert_eq!(
        nested.focus_gained_count(),
        0,
        "dismissing the modal must not enter passthrough"
    );

    nested.host.send_stdin(&keys::ctrl('p'));
    nested.host.wait_until(
        "host acts on its own pane-mode keybinding after dismissing the modal",
        pane_mode_bar_settled,
    );
    nested.host.send_stdin(&keys::ESC);
    nested.host.wait_until(
        "host returned to normal mode after dismissing the modal",
        normal_mode_bar_settled,
    );

    nested.guest.wait_for_app_load();
    nested.guest.quit();
    nested.host.quit();
}

#[test]
fn focus_guest_session_descends_after_dismissal() {
    let mut nested =
        NestedHarness::start_with_host_config(TERMINAL_SIZE, focus_guest_binding_config());

    wait_for_modal(&nested);
    nested.dismiss_guest_modal();
    nested.host.wait_until("modal dismissed", |host_grid| {
        !modal_visible(host_grid)
    });

    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(&keys::ctrl('y'));
    nested.wait_for_host_to_descend_into_guest_after(descended);

    nested.guest.wait_for_app_load();
    nested.host.send_stdin(&keys::alt('n'));
    let guest_new_pane = nested.guest.expect_pty_spawn();
    guest_new_pane.output(PROMPT);
    nested.guest.wait_until(
        "guest spawned a pane from the passed-through key after FocusGuestSession",
        |guest_grid| guest_grid.contains("Pane #2"),
    );

    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.quit();
}

#[test]
fn two_clients_answer_their_own_modals_independently() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    let first_with_modal = wait_for_modal(&nested);
    assert!(modal_visible(&first_with_modal));

    let second = nested.host.attach_client(TERMINAL_SIZE);
    let second_with_modal =
        second.wait_until("the second client also sees the guest modal", modal_visible);
    assert!(modal_visible(&second_with_modal));

    let descended = nested.mark_host_to_guest();
    nested.host.send_stdin(b"2");
    nested.wait_for_host_to_descend_into_guest_after(descended);
    nested.host.wait_until(
        "the first client's modal is dismissed after it descends",
        |host_grid| !modal_visible(host_grid),
    );

    second.wait_until(
        "the second client's modal stays up while the first client descended",
        modal_visible,
    );

    nested.guest.wait_for_app_load();
    second.quit();
    let released = nested.mark_host_to_guest();
    nested.guest.quit();
    nested.wait_for_host_to_ascend_from_guest_after(released);
    nested.host.quit();
}

#[test]
fn guest_reannounce_reshows_a_dismissed_modal() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    wait_for_modal(&nested);
    nested.guest.wait_for_app_load();
    nested.dismiss_guest_modal();
    nested.host.wait_until("modal dismissed", |host_grid| {
        !modal_visible(host_grid)
    });

    nested.freeze_guest();
    nested.assert_host_stops_pinging_frozen_guest();

    nested.guest.quit();
    nested.host.quit();
}
