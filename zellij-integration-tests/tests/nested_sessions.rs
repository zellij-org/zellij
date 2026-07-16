#![cfg(unix)]

use zellij_integration_tests::{NestedHarness, TERMINAL_SIZE};

#[test]
fn a_guest_zellij_running_inside_a_host_zellij_pane_introduces_itself_and_is_kept_alive() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.wait_for_host_to_ping_guest();
    nested.wait_for_guest_to_reply_to_ping();

    nested.guest.wait_for_app_load();
    nested.host.wait_for_app_load();

    nested.guest.quit();
    nested.wait_for_host_to_release_guest_focus();
    nested.host.quit();
}

#[test]
fn a_host_zellij_stops_pinging_a_guest_that_freezes_inside_one_of_its_panes() {
    let mut nested = NestedHarness::start(TERMINAL_SIZE);

    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.wait_for_host_to_ping_guest();

    nested.freeze_guest();
    nested.assert_host_stops_pinging_frozen_guest();

    nested.guest.quit();
    nested.host.quit();
}
