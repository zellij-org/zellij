#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, TestRunner, TestSession, TERMINAL_SIZE,
};

const THEME_CONFIG: &str = r#"
theme_dark "test-dark"
theme_light "test-light"
themes {
    test-dark {
        text_unselected {
            base 11 22 33
            background 0 0 0
            emphasis_0 11 22 33
            emphasis_1 11 22 33
            emphasis_2 11 22 33
            emphasis_3 11 22 33
        }
        ribbon_unselected {
            base 11 22 33
            background 0 0 0
            emphasis_0 11 22 33
            emphasis_1 11 22 33
            emphasis_2 11 22 33
            emphasis_3 11 22 33
        }
    }
    test-light {
        text_unselected {
            base 44 55 66
            background 0 0 0
            emphasis_0 44 55 66
            emphasis_1 44 55 66
            emphasis_2 44 55 66
            emphasis_3 44 55 66
        }
        ribbon_unselected {
            base 44 55 66
            background 0 0 0
            emphasis_0 44 55 66
            emphasis_1 44 55 66
            emphasis_2 44 55 66
            emphasis_3 44 55 66
        }
    }
}
keybinds {
    normal {
        bind "Ctrl y" { SetDarkTheme; }
        bind "Ctrl e" { SetLightTheme; }
        bind "Ctrl a" { ToggleTheme; }
    }
}
"#;

const DARK_MARKER: &[u8] = b"38;2;11;22;33";
const LIGHT_MARKER: &[u8] = b"38;2;44;55;66";

const SET_DARK_THEME: [u8; 1] = [0x19];
const SET_LIGHT_THEME: [u8; 1] = [0x05];
const TOGGLE_THEME: [u8; 1] = [0x01];

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(THEME_CONFIG)
        .start()
}

fn last_occurrence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn painted_last_with(haystack: &[u8], expected: &[u8], other: &[u8]) -> bool {
    match (
        last_occurrence(haystack, expected),
        last_occurrence(haystack, other),
    ) {
        (Some(_), None) => true,
        (Some(expected_at), Some(other_at)) => expected_at > other_at,
        _ => false,
    }
}

fn wait_for_dark_palette(zellij: &TestSession, what: &str) {
    zellij.wait_until_raw_output(what, |bytes| {
        painted_last_with(bytes, DARK_MARKER, LIGHT_MARKER)
    });
}

fn wait_for_light_palette(zellij: &TestSession, what: &str) {
    zellij.wait_until_raw_output(what, |bytes| {
        painted_last_with(bytes, LIGHT_MARKER, DARK_MARKER)
    });
}

#[test]
fn set_dark_and_set_light_theme_actions_repaint_the_palette() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&SET_DARK_THEME);
    wait_for_dark_palette(&zellij, "the dark theme palette was rendered");

    zellij.send_stdin(&SET_LIGHT_THEME);
    wait_for_light_palette(&zellij, "the light theme palette was rendered");

    zellij.send_stdin(&SET_DARK_THEME);
    wait_for_dark_palette(&zellij, "the dark theme palette was rendered again");

    zellij.quit();
}

#[test]
fn toggle_theme_round_trips_between_the_configured_themes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&SET_DARK_THEME);
    wait_for_dark_palette(&zellij, "the dark theme palette was rendered");

    zellij.send_stdin(&TOGGLE_THEME);
    wait_for_light_palette(&zellij, "toggling from dark rendered the light palette");

    zellij.send_stdin(&TOGGLE_THEME);
    wait_for_dark_palette(&zellij, "toggling back rendered the dark palette again");

    zellij.quit();
}
