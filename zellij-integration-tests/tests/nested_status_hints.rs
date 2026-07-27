#![cfg(unix)]
use zellij_integration_tests::nested::NestedHarness;
use zellij_utils::nested_session::NestedSessionMessage;
use zellij_utils::pane_size::Size;

const TERMINAL_SIZE: Size = Size {
    cols: 121,
    rows: 30,
};

#[test]
fn guest_shows_the_ascended_hint_on_boot() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.host_to_guest().wait_for(
        "the host to include its descend shortcut in the announce_ack",
        |message| matches!(message, NestedSessionMessage::AnnounceAck { descend_keys, .. } if !descend_keys.is_empty()),
    );
    nested.guest_to_host().wait_for(
        "the guest to advertise its ascend shortcut after the handshake",
        |message| matches!(message, NestedSessionMessage::ShortcutUpdate { ascend_keys, .. } if !ascend_keys.is_empty()),
    );
    nested.guest.wait_until(
        "the guest status bar to show the ascended hint",
        |guest_grid| guest_grid.contains("Descend:"),
    );
}
