use crate::{terminal_teardown_message, DISABLE_FOCUS_REPORTING, ENABLE_FOCUS_REPORTING};

#[test]
fn the_teardown_message_cancels_focus_reporting() {
    let message = terminal_teardown_message("bye", 20, false);
    assert!(
        message.contains(DISABLE_FOCUS_REPORTING),
        "the host must stop emitting focus reports on teardown, got: {:?}",
        message
    );
}

#[test]
fn the_teardown_message_cancels_focus_reporting_before_leaving_the_alternate_screen() {
    let message = terminal_teardown_message("bye", 20, true);
    let disable_focus = message.find(DISABLE_FOCUS_REPORTING).unwrap();
    let exit_alternate_screen = message.find("\u{1b}[?1049l").unwrap();
    assert!(
        disable_focus < exit_alternate_screen,
        "focus reporting is cancelled while we still own the screen, got: {:?}",
        message
    );
}

#[test]
fn the_focus_reporting_sequences_are_the_decset_1004_pair() {
    assert_eq!(ENABLE_FOCUS_REPORTING, "\u{1b}[?1004h");
    assert_eq!(DISABLE_FOCUS_REPORTING, "\u{1b}[?1004l");
}
