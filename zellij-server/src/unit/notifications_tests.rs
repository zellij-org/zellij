use super::NotificationProtocol;
use std::collections::BTreeMap;
use zellij_utils::input::options::HostNotificationProtocol;

fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn a_kitty_host_is_detected_from_its_term() {
    assert_eq!(
        NotificationProtocol::from_host_terminal_env(&env(&[("TERM", "xterm-kitty")])),
        NotificationProtocol::Osc99
    );
}

#[test]
fn a_kitty_host_is_detected_from_its_window_id() {
    assert_eq!(
        NotificationProtocol::from_host_terminal_env(&env(&[
            ("TERM", "xterm-256color"),
            ("KITTY_WINDOW_ID", "1")
        ])),
        NotificationProtocol::Osc99
    );
}

#[test]
fn an_unrecognized_host_falls_back_to_the_legacy_protocol() {
    assert_eq!(
        NotificationProtocol::from_host_terminal_env(&env(&[("TERM", "xterm-256color")])),
        NotificationProtocol::Osc9
    );
    assert_eq!(
        NotificationProtocol::from_host_terminal_env(&BTreeMap::new()),
        NotificationProtocol::Osc9
    );
}

#[test]
fn detection_is_used_when_the_protocol_is_configured_to_auto() {
    assert_eq!(
        NotificationProtocol::resolve(
            HostNotificationProtocol::Auto,
            &env(&[("TERM", "xterm-kitty")])
        ),
        NotificationProtocol::Osc99
    );
    assert_eq!(
        NotificationProtocol::resolve(
            HostNotificationProtocol::Auto,
            &env(&[("TERM", "xterm-256color")])
        ),
        NotificationProtocol::Osc9
    );
}

#[test]
fn an_explicitly_configured_protocol_overrides_detection() {
    let kitty = env(&[("TERM", "xterm-kitty")]);
    assert_eq!(
        NotificationProtocol::resolve(HostNotificationProtocol::Osc9, &kitty),
        NotificationProtocol::Osc9
    );
    assert_eq!(
        NotificationProtocol::resolve(HostNotificationProtocol::Bell, &kitty),
        NotificationProtocol::Bell
    );
    assert_eq!(
        NotificationProtocol::resolve(HostNotificationProtocol::Off, &kitty),
        NotificationProtocol::Off
    );
    assert_eq!(
        NotificationProtocol::resolve(
            HostNotificationProtocol::Osc99,
            &env(&[("TERM", "xterm-256color")])
        ),
        NotificationProtocol::Osc99
    );
}

#[test]
fn each_protocol_renders_its_own_escape_sequence() {
    assert_eq!(
        NotificationProtocol::Osc99.render("title", "body"),
        Some("\u{1b}]99;;title: body\u{7}".to_owned())
    );
    assert_eq!(
        NotificationProtocol::Osc9.render("title", "body"),
        Some("\u{1b}]9;title: body\u{7}".to_owned())
    );
    assert_eq!(
        NotificationProtocol::Bell.render("title", "body"),
        Some("\u{7}".to_owned())
    );
    assert_eq!(NotificationProtocol::Off.render("title", "body"), None);
}

#[test]
fn a_missing_title_or_body_is_not_padded_with_a_separator() {
    assert_eq!(
        NotificationProtocol::Osc9.render("", "body"),
        Some("\u{1b}]9;body\u{7}".to_owned())
    );
    assert_eq!(
        NotificationProtocol::Osc9.render("title", ""),
        Some("\u{1b}]9;title\u{7}".to_owned())
    );
}
