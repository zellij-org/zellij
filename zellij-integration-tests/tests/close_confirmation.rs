#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, start_zellij, GridSnapshot,
    TERMINAL_SIZE,
};
use zellij_utils::input::actions::Action;

const TITLE: &str = "Close this window?";

fn dialogue_visible(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains(TITLE)
        && grid_snapshot.contains("> 1. Detach and leave running in the background.")
        && grid_snapshot.contains("2. Quit and shut down the entire session.")
        && grid_snapshot.contains("<Esc> cancel")
}

fn without_session_number(text: &str) -> String {
    regex::Regex::new(r#""test-(\d+)""#)
        .unwrap()
        .replace_all(text, |captures: &regex::Captures| {
            format!("\"test-{}\"", "N".repeat(captures[1].len()))
        })
        .to_string()
}

#[test]
fn the_close_dialogue_renders_expected_ui() {
    let mut zellij = start_zellij();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.main_client().send_action(Action::ConfirmClose);
    let shown = zellij.wait_until("the close dialogue is drawn", dialogue_visible);
    assert!(shown.contains("keeps running and can be attached again."));
    assert_snapshot!(without_session_number(&normalized(&shown)));
    zellij.send_stdin(&keys::key('n'));
    zellij.wait_until("the dialogue is gone", |grid_snapshot| {
        !grid_snapshot.contains(TITLE)
    });
    zellij.quit();
}

#[test]
fn cancelling_restores_the_pane_and_keeps_the_client() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(b"marker-before");
    zellij.wait_until("the pane shows its marker", |grid_snapshot| {
        grid_snapshot.contains("marker-before")
    });
    zellij.main_client().send_action(Action::ConfirmClose);
    zellij.wait_until("the close dialogue is drawn", dialogue_visible);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('j'));
    zellij.wait_until("down selects quit", |grid_snapshot| {
        grid_snapshot.contains("> 2. Quit and shut down the entire session.")
    });
    zellij.send_stdin(&keys::key('j'));
    zellij.wait_until("down again selects cancel", |grid_snapshot| {
        grid_snapshot.contains("> 3. Cancel")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "the pane contents are back and still in normal mode",
        |grid_snapshot| {
            !grid_snapshot.contains(TITLE)
                && grid_snapshot.contains("marker-before")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor.is_some()
        },
    );
    zellij.quit();
}

#[test]
fn detaching_closes_only_the_client_that_asked() {
    let mut zellij = start_zellij();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::ConfirmClose);
    second.wait_until("the second client sees the dialogue", dialogue_visible);
    let first = zellij.snapshot();
    assert!(
        !first.contains(TITLE),
        "the dialogue leaked to another client:\n{}",
        normalized(&first)
    );
    second.send_stdin(&keys::key('y'));
    second.wait_for_client_to_exit();
    let first = zellij.wait_until("the first client keeps its pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor.is_some()
    });
    assert!(!first.contains(TITLE));
    zellij.quit();
}

#[test]
fn a_second_close_request_detaches() {
    let mut zellij = start_zellij();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::ConfirmClose);
    second.wait_until("the second client sees the dialogue", dialogue_visible);
    second.send_action(Action::ConfirmClose);
    second.wait_for_client_to_exit();
    zellij.quit();
}

#[test]
fn quitting_ends_the_session_for_every_client() {
    let mut zellij = start_zellij();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::ConfirmClose);
    second.wait_until("the second client sees the dialogue", dialogue_visible);
    second.send_stdin(&keys::key('q'));
    second.wait_for_client_to_exit();
    zellij.wait_for_main_client_to_exit();
}
