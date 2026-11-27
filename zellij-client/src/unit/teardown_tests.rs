use crate::{
    terminal_teardown_message, OuterGraphemeClusterMode, DISABLE_FOCUS_REPORTING,
    ENABLE_FOCUS_REPORTING,
};

#[test]
fn the_teardown_message_cancels_focus_reporting() {
    let message = terminal_teardown_message("bye", 20, false, false);
    assert!(
        message.contains(DISABLE_FOCUS_REPORTING),
        "the host must stop emitting focus reports on teardown, got: {:?}",
        message
    );
}

#[test]
fn the_teardown_message_cancels_focus_reporting_before_leaving_the_alternate_screen() {
    let message = terminal_teardown_message("bye", 20, true, false);
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

fn run_replies(replies: &[u8]) -> (Vec<u8>, String) {
    let mut mode = OuterGraphemeClusterMode::default();
    let mut stdout: Vec<u8> = Vec::new();
    for value in replies {
        mode.handle_reply(*value, &mut stdout);
    }
    let teardown = terminal_teardown_message("bye", 20, true, mode.is_enabled());
    (stdout, teardown)
}

#[test]
fn a_supported_grapheme_cluster_mode_is_turned_on_once_and_off_on_exit() {
    for replies in [vec![1u8], vec![2u8], vec![1u8, 2u8, 1u8]] {
        let (stdout, teardown) = run_replies(&replies);
        assert_eq!(
            stdout,
            b"\x1b[?2027h".to_vec(),
            "replies {:?} must turn mode 2027 on exactly once",
            replies
        );
        assert_eq!(
            teardown.matches("\u{1b}[?2027l").count(),
            1,
            "replies {:?} must turn mode 2027 off on exit, got: {:?}",
            replies,
            teardown
        );
        let mode_off = teardown.find("\u{1b}[?2027l").unwrap();
        let exit_alternate_screen = teardown.find("\u{1b}[?1049l").unwrap();
        assert!(mode_off < exit_alternate_screen);
    }
}

#[test]
fn a_permanent_or_unsupported_grapheme_cluster_mode_is_left_alone() {
    for replies in [vec![3u8], vec![0u8], vec![4u8], vec![]] {
        let (stdout, teardown) = run_replies(&replies);
        assert!(
            stdout.is_empty(),
            "replies {:?} must not write anything, got: {:?}",
            replies,
            stdout
        );
        assert!(
            !teardown.contains("\u{1b}[?2027"),
            "replies {:?} must not touch mode 2027 on exit, got: {:?}",
            replies,
            teardown
        );
    }
}

#[test]
fn a_late_supported_reply_after_an_unsupported_one_still_turns_the_mode_on_once() {
    let (stdout, teardown) = run_replies(&[0, 3, 2, 2]);
    assert_eq!(stdout, b"\x1b[?2027h".to_vec());
    assert_eq!(teardown.matches("\u{1b}[?2027l").count(), 1);
}

#[test]
fn a_session_switch_does_not_tell_the_outer_zellij_that_the_guest_left() {
    use crate::says_goodbye_to_host;
    assert!(says_goodbye_to_host(true, false));
    assert!(!says_goodbye_to_host(true, true));
    assert!(!says_goodbye_to_host(false, false));
    assert!(!says_goodbye_to_host(false, true));
}
