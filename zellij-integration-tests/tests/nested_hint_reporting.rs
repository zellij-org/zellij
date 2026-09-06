#![cfg(unix)]
//! A host session cannot render hints for a guest it knows nothing about. These tests cover
//! the frames that close that gap: the guest telling its host which input mode it is in, and
//! the guest answering the host's request for its full keybinding table.

use zellij_integration_tests::keys;
use zellij_integration_tests::nested::NestedHarness;
use zellij_utils::data::InputMode;
use zellij_utils::nested_session::{self, NestedSessionCapability, NestedSessionMessage};
use zellij_utils::pane_size::Size;

const TERMINAL_SIZE: Size = Size {
    cols: 121,
    rows: 30,
};

#[test]
fn both_sides_advertise_hint_reporting_during_the_handshake() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.guest_to_host().wait_for(
        "the guest to advertise that it can report its hints",
        |message| {
            matches!(
                message,
                NestedSessionMessage::Announce { capabilities, .. }
                    if capabilities.contains(&NestedSessionCapability::HintReporting)
            )
        },
    );
    nested.host_to_guest().wait_for(
        "the host to advertise that it can consume the guest's hints",
        |message| {
            matches!(
                message,
                NestedSessionMessage::AnnounceAck { capabilities, .. }
                    if capabilities.contains(&NestedSessionCapability::HintReporting)
            )
        },
    );
}

#[test]
fn the_guest_answers_a_keybinding_request_with_its_table_and_current_mode() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();

    // A plugin in the host would trigger this frame through
    // PluginCommand::RequestNestedSessionKeybinds; writing it into the guest's stdin puts it
    // on the same wire the host writes to, without needing a plugin in the test.
    let since = nested.mark_guest_to_host();
    nested.guest.send_stdin(&nested_session::encode_frame(
        &NestedSessionMessage::RequestGuestKeybinds,
    ));

    nested.guest_to_host().wait_for_after(
        since,
        "the guest to answer with a keybinding table that has bindings in it",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestKeybindsUpdate { keybinds }
                    if keybinds.iter().any(|(_, bindings)| !bindings.is_empty())
            )
        },
    );
    nested.guest_to_host().wait_for_after(
        since,
        "the guest to send its current mode and base mode alongside the keybinding table",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate {
                    base_mode: Some(_),
                    ..
                }
            )
        },
    );
}

#[test]
fn the_guest_reports_its_mode_as_the_user_switches_modes_inside_it() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();
    nested.descend_into_guest_via_modal();
    nested.wait_for_host_to_descend_into_guest();

    let since = nested.mark_guest_to_host();
    nested.host.send_stdin(&keys::ctrl('p'));
    nested.guest_to_host().wait_for_after(
        since,
        "the guest to report that it entered pane mode",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate {
                    mode: InputMode::Pane,
                    ..
                }
            )
        },
    );

    let since = nested.mark_guest_to_host();
    nested.host.send_stdin(&keys::ESC);
    nested.guest_to_host().wait_for_after(
        since,
        "the guest to report that it left pane mode again",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate {
                    mode: InputMode::Normal,
                    ..
                }
            )
        },
    );
}
