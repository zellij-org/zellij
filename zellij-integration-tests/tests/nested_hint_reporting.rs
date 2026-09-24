#![cfg(unix)]

use zellij_integration_tests::keys;
use zellij_integration_tests::nested::{NestedDepthThreeHarness, NestedHarness};
use zellij_utils::data::InputMode;
use zellij_utils::nested_session::{self, NestedSessionCapability, NestedSessionMessage};
use zellij_utils::pane_size::Size;

const TERMINAL_SIZE: Size = Size {
    cols: 121,
    rows: 30,
};

const DEPTH_THREE_TERMINAL_SIZE: Size = Size {
    cols: 160,
    rows: 40,
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
fn the_guest_reports_its_mode_as_soon_as_the_handshake_completes() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();

    nested.guest_to_host().wait_for(
        "the guest to report its starting mode without being asked",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate {
                    mode: InputMode::Normal,
                    base_mode: Some(_),
                    session_path,
                    ..
                } if session_path.len() == 1
            )
        },
    );
}

#[test]
fn the_guest_answers_a_keybinding_request_with_its_table_and_current_mode() {
    let nested = NestedHarness::start(TERMINAL_SIZE);
    nested.wait_for_guest_to_announce();
    nested.wait_for_host_to_acknowledge_guest();

    let since = nested.mark_guest_to_host();
    nested.guest.send_stdin(&nested_session::encode_frame(
        &NestedSessionMessage::RequestGuestKeybinds { request_id: 41 },
    ));

    nested.guest_to_host().wait_for_after(
        since,
        "the guest to answer request 41 with its table, mode and base mode",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestKeybindsReply {
                    request_id: 41,
                    result: Ok(nested_session_keybinds),
                } if nested_session_keybinds.base_mode.is_some()
                    && nested_session_keybinds.session_path.len() == 1
                    && nested_session_keybinds
                        .keybinds
                        .iter()
                        .any(|(_, bindings)| !bindings.is_empty())
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

#[test]
fn a_middle_session_reports_and_answers_for_the_session_its_keys_go_to() {
    let nested = NestedDepthThreeHarness::start_depth_three(DEPTH_THREE_TERMINAL_SIZE);
    nested.boot_and_descend_depth_three();

    nested.middle_to_outer_frames().wait_for(
        "the middle session to report the inner session's mode once its keys go there",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate { session_path, .. }
                    if session_path.len() == 2
            )
        },
    );

    let since = nested.mark_middle_to_outer();
    nested.outer.send_stdin(&keys::ctrl('p'));
    nested.middle_to_outer_frames().wait_for_after(
        since,
        "the middle session to pass the inner session's switch to pane mode up to the outer session",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestModeUpdate {
                    mode: InputMode::Pane,
                    session_path,
                    ..
                } if session_path.len() == 2
            )
        },
    );

    let since = nested.mark_middle_to_outer();
    nested.middle.send_stdin(&nested_session::encode_frame(
        &NestedSessionMessage::RequestGuestKeybinds { request_id: 77 },
    ));
    nested.middle_to_outer_frames().wait_for_after(
        since,
        "the middle session to relay request 77 to the inner session and pass its answer up",
        |message| {
            matches!(
                message,
                NestedSessionMessage::GuestKeybindsReply {
                    request_id: 77,
                    result: Ok(nested_session_keybinds),
                } if nested_session_keybinds.session_path.len() == 2
                    && nested_session_keybinds.mode == InputMode::Pane
                    && nested_session_keybinds
                        .keybinds
                        .iter()
                        .any(|(_, bindings)| !bindings.is_empty())
            )
        },
    );
}
