#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, GridSnapshot, TestRunner,
    TestSession, TERMINAL_SIZE,
};
use zellij_utils::input::actions::Action;

const TITLE: &str = "Close this window?";

fn dialogue_visible(grid_snapshot: &GridSnapshot) -> bool {
    grid_snapshot.contains(TITLE)
        && grid_snapshot.contains("Detach (the session keeps running)")
        && grid_snapshot.contains("Quit (ends session")
        && grid_snapshot.contains("Cancel")
        && grid_snapshot.contains("Always do this")
        && grid_snapshot.contains("<Esc> - cancel")
}

fn without_session_number(text: &str) -> String {
    regex::Regex::new(r"test-(\d+)")
        .unwrap()
        .replace_all(text, |captures: &regex::Captures| {
            format!("test-{}", "N".repeat(captures[1].len()))
        })
        .to_string()
}

fn start_zellij_confirming_quit() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("on_quit \"ask_quit\"")
        .start()
}

#[test]
fn the_close_dialogue_renders_expected_ui() {
    let mut zellij = start_zellij_confirming_quit();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.main_client().send_action(Action::Quit);
    let shown = zellij.wait_until("the close popup is drawn", dialogue_visible);
    assert_snapshot!(without_session_number(&normalized(&shown)));
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the popup is gone", |grid_snapshot| {
        !grid_snapshot.contains(TITLE)
    });
    zellij.kill_session();
}

#[test]
fn the_quit_key_opens_the_popup_and_the_quit_choice_ends_the_session() {
    let mut zellij = start_zellij_confirming_quit();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('q'));
    zellij.wait_until("quit opens the close popup", dialogue_visible);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_for_main_client_to_exit();
}

#[test]
fn the_popup_takes_every_key_in_any_mode() {
    let mut zellij = start_zellij_confirming_quit();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(b"marker-before");
    zellij.wait_until("the pane shows its marker", |grid_snapshot| {
        grid_snapshot.contains("marker-before")
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("pane mode is on", |grid_snapshot| {
        grid_snapshot.contains("Fullscreen") && grid_snapshot.contains("Rename")
    });
    zellij.main_client().send_action(Action::Quit);
    zellij.wait_until("the close popup is drawn", dialogue_visible);
    zellij.send_stdin(&keys::ctrl('q'));
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "cancel was chosen and the client is still in pane mode",
        |grid_snapshot| {
            !grid_snapshot.contains(TITLE)
                && grid_snapshot.contains("marker-before")
                && grid_snapshot.contains("Rename")
        },
    );
    zellij.kill_session();
}

#[test]
fn detaching_closes_only_the_client_that_asked() {
    let mut zellij = start_zellij_confirming_quit();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::Quit);
    second.wait_until("the second client sees the popup", |grid_snapshot| {
        dialogue_visible(grid_snapshot) && grid_snapshot.contains("disconnects 1 other client")
    });
    let first = zellij.snapshot();
    assert!(
        !first.contains(TITLE),
        "the popup leaked to another client:\n{}",
        normalized(&first)
    );
    second.send_stdin(&keys::UP);
    second.send_stdin(&keys::ENTER);
    second.wait_for_client_to_exit();
    let first = zellij.wait_until("the first client keeps its pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor.is_some()
    });
    assert!(!first.contains(TITLE));
    zellij.kill_session();
}

#[test]
fn a_second_quit_while_the_popup_is_open_leaves() {
    let mut zellij = start_zellij_confirming_quit();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::Quit);
    second.wait_until("the second client sees the popup", dialogue_visible);
    second.send_action(Action::Quit);
    second.wait_for_client_to_exit();
    let first = zellij.wait_until("the first client keeps its pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor.is_some()
    });
    assert!(!first.contains(TITLE));
    zellij.kill_session();
}

#[test]
fn quitting_ends_the_session_for_every_client() {
    let mut zellij = start_zellij_confirming_quit();
    let _terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    second.send_action(Action::Quit);
    second.wait_until("the second client sees the popup", dialogue_visible);
    second.send_stdin(&keys::ENTER);
    second.wait_for_client_to_exit();
    zellij.wait_for_main_client_to_exit();
}
