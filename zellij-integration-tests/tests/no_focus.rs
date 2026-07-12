#![cfg(unix)]

use std::time::Instant;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, default_timeout, start_zellij, FakePtyHandle, PROMPT,
};
use zellij_utils::cli::CliAction;

fn no_focus_new_pane_action(command: &[&str]) -> CliAction {
    CliAction::NewPane {
        direction: None,
        command: command.iter().map(|part| part.to_string()).collect(),
        plugin: None,
        cwd: None,
        floating: false,
        in_place: false,
        close_replaced_pane: false,
        pane_id: None,
        name: None,
        close_on_exit: false,
        start_suspended: false,
        configuration: None,
        skip_plugin_cache: false,
        x: None,
        y: None,
        width: None,
        height: None,
        pinned: None,
        stacked: false,
        blocking: false,
        block_until_exit_success: false,
        block_until_exit_failure: false,
        block_until_exit: false,
        unblock_condition: None,
        near_current_pane: false,
        no_focus: true,
        borderless: None,
        tab_id: None,
    }
}

fn no_focus_new_tab_action() -> CliAction {
    CliAction::NewTab {
        name: None,
        layout: None,
        layout_string: None,
        layout_dir: None,
        cwd: None,
        initial_command: vec![],
        initial_plugin: None,
        close_on_exit: false,
        start_suspended: false,
        block_until_exit_success: false,
        block_until_exit_failure: false,
        block_until_exit: false,
        no_focus: true,
    }
}

fn wait_until_stdin_contains(handle: &FakePtyHandle, needle: &[u8]) {
    let deadline = Instant::now() + default_timeout();
    loop {
        let bytes = handle.stdin_bytes();
        if bytes.windows(needle.len()).any(|window| window == needle) {
            return;
        }
        if Instant::now() >= deadline {
            panic!(
                "timed out waiting for pane to receive {:?}, received {:?}",
                String::from_utf8_lossy(needle),
                String::from_utf8_lossy(&bytes),
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn stdin_contains(handle: &FakePtyHandle, needle: &[u8]) -> bool {
    handle
        .stdin_bytes()
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn new_pane_no_focus_leaves_input_on_the_original_pane() {
    let mut zellij = start_zellij();
    let original_pane = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.run_cli_action(no_focus_new_pane_action(&["unfocused-command"]));
    let unfocused_pane = zellij.expect_pty_spawn();
    unfocused_pane.output(PROMPT);
    zellij.wait_until(
        "second pane appeared alongside the first",
        |grid_snapshot| grid_snapshot.text.matches("$").count() >= 2,
    );

    let probe = b"nofocusprobe";
    zellij.send_stdin(probe);
    wait_until_stdin_contains(&original_pane, probe);
    assert!(
        !stdin_contains(&unfocused_pane, probe),
        "input leaked into the pane that was opened with --no-focus"
    );
    zellij.quit();
}

#[test]
fn new_pane_without_no_focus_moves_input_to_the_new_pane() {
    let mut zellij = start_zellij();
    let original_pane = claim_first_terminal_and_wait_for_prompt(&zellij);

    let mut focusing_action = no_focus_new_pane_action(&["focused-command"]);
    if let CliAction::NewPane { no_focus, .. } = &mut focusing_action {
        *no_focus = false;
    }
    zellij.run_cli_action(focusing_action);
    let focused_pane = zellij.expect_pty_spawn();
    focused_pane.output(PROMPT);
    zellij.wait_until(
        "second pane appeared alongside the first",
        |grid_snapshot| grid_snapshot.text.matches("$").count() >= 2,
    );

    let probe = b"focusprobe";
    zellij.send_stdin(probe);
    wait_until_stdin_contains(&focused_pane, probe);
    assert!(
        !stdin_contains(&original_pane, probe),
        "input reached the original pane even though the new pane should have taken focus"
    );
    zellij.quit();
}

#[test]
fn new_tab_no_focus_leaves_focus_on_the_original_tab() {
    let mut zellij = start_zellij();
    let original_pane = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.run_cli_action(no_focus_new_tab_action());
    let new_tab_pane = zellij.expect_pty_spawn();
    new_tab_pane.output(PROMPT);
    zellij.wait_until("a second tab exists in the tab bar", |grid_snapshot| {
        grid_snapshot.text.contains("Tab #2")
    });

    let probe = b"nofocusprobe";
    zellij.send_stdin(probe);
    wait_until_stdin_contains(&original_pane, probe);
    assert!(
        !stdin_contains(&new_tab_pane, probe),
        "input leaked into the tab that was created with --no-focus"
    );
    zellij.quit();
}
