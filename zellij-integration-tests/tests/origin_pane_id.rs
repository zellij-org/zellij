#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, TestRunner,
    TestSession, TERMINAL_SIZE,
};

const ORIGIN_PANE_ID: &str = "ZELLIJ_ORIGIN_PANE_ID";

fn start_with_run_bindings() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(
            r#"
            keybinds {
                shared {
                    bind "Alt y" { Run "origin-test" { floating true; }; }
                    bind "Alt u" { MoveFocus "Left"; Run "origin-test" { floating true; }; }
                }
            }
            "#,
        )
        .start()
}

#[test]
fn a_command_run_from_a_keybinding_knows_the_pane_it_was_started_from() {
    let mut zellij = start_with_run_bindings();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second_terminal = split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('y'));
    let command_terminal = zellij.expect_pty_spawn();

    let expected_origin = format!("terminal_{}", second_terminal.terminal_id());
    assert_eq!(
        command_terminal.env_var(ORIGIN_PANE_ID).as_deref(),
        Some(expected_origin.as_str())
    );
    assert_eq!(first_terminal.env_var(ORIGIN_PANE_ID), None);
    assert_eq!(second_terminal.env_var(ORIGIN_PANE_ID), None);
    zellij.quit();
}

#[test]
fn the_origin_pane_reflects_focus_changes_earlier_in_the_same_keybinding() {
    let mut zellij = start_with_run_bindings();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('u'));
    let command_terminal = zellij.expect_pty_spawn();

    let expected_origin = format!("terminal_{}", first_terminal.terminal_id());
    assert_eq!(
        command_terminal.env_var(ORIGIN_PANE_ID).as_deref(),
        Some(expected_origin.as_str())
    );
    zellij.quit();
}

#[test]
fn a_command_run_from_a_focused_plugin_pane_gets_a_plugin_origin() {
    let mut zellij = start_with_run_bindings();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("about plugin shown", |grid_snapshot| {
        grid_snapshot.contains("About Zellij")
    });

    zellij.send_stdin(&keys::alt('y'));
    let command_terminal = zellij.expect_pty_spawn();

    let origin = command_terminal
        .env_var(ORIGIN_PANE_ID)
        .expect("origin pane id is set");
    let plugin_id = origin
        .strip_prefix("plugin_")
        .unwrap_or_else(|| panic!("expected a plugin origin, got {}", origin));
    assert!(plugin_id.parse::<u32>().is_ok(), "{}", origin);
    zellij.quit();
}
