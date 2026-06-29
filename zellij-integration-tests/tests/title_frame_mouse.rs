#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{normalized, FakePtyHandle, Size, TestRunner, TestSession};

const PROMPT: &[u8] = b"$ ";
const STATUS_BAR_TEST_COLS: usize = 190;

fn start_zellij(cols: usize) -> TestSession {
    TestRunner::new(Size { cols, rows: 24 })
        .with_config("mouse_mode true\npane_frame_style \"titles\"")
        .start()
}

fn sgr_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn display_column_of(line: &str, needle: &str) -> Option<usize> {
    line.find(needle)
        .map(|byte_offset| line[..byte_offset].chars().count())
}

fn claim_first_terminal_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("$ ")
    });
    terminal
}

fn click_new_pane_button_in_status_bar(zellij: &TestSession, pane_output: &[u8]) -> FakePtyHandle {
    let grid_snapshot =
        zellij.wait_until("status bar shows the new pane button", |grid_snapshot| {
            grid_snapshot
                .lines()
                .last()
                .is_some_and(|status_bar| status_bar.contains("New Pane"))
        });
    let status_bar = grid_snapshot.lines().last().cloned().unwrap();
    let new_pane_column = display_column_of(&status_bar, "New Pane")
        .expect("new pane button is on the status bar")
        + 1;
    let status_bar_line = grid_snapshot.lines().len();

    zellij.send_stdin(&sgr_left_click(new_pane_column, status_bar_line));
    let new_terminal = zellij.expect_pty_spawn();
    new_terminal.output(pane_output);
    new_terminal
}

#[test]
fn mouse_click_new_pane_in_status_bar_opens_a_new_pane() {
    let mut zellij = start_zellij(STATUS_BAR_TEST_COLS);
    claim_first_terminal_and_wait_for_prompt(&zellij);

    click_new_pane_button_in_status_bar(&zellij, b"opened from the status bar");

    let grid_snapshot = zellij.wait_until("the clicked new pane rendered", |grid_snapshot| {
        grid_snapshot.contains("opened from the status bar")
            && grid_snapshot.contains("Change Focus")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn mouse_click_new_pane_in_status_bar_twice_opens_two_panes() {
    let mut zellij = start_zellij(STATUS_BAR_TEST_COLS);
    claim_first_terminal_and_wait_for_prompt(&zellij);

    click_new_pane_button_in_status_bar(&zellij, b"first pane from the status bar");
    zellij.wait_until(
        "the first clicked pane rendered and the status bar gained the multi-pane controls",
        |grid_snapshot| {
            grid_snapshot.contains("first pane from the status bar")
                && grid_snapshot.contains("Change Focus")
        },
    );

    click_new_pane_button_in_status_bar(&zellij, b"second pane from the status bar");

    let grid_snapshot = zellij.wait_until(
        "the second clicked pane rendered alongside the first",
        |grid_snapshot| {
            grid_snapshot.contains("first pane from the status bar")
                && grid_snapshot.contains("second pane from the status bar")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

fn click_floating_ribbon_in_status_bar(zellij: &TestSession) {
    let grid_snapshot =
        zellij.wait_until("status bar shows the floating ribbon", |grid_snapshot| {
            grid_snapshot
                .lines()
                .last()
                .is_some_and(|status_bar| status_bar.contains("Floating"))
        });
    let status_bar = grid_snapshot.lines().last().cloned().unwrap();
    let floating_ribbon_column = display_column_of(&status_bar, "Floating")
        .expect("floating ribbon is on the status bar")
        + 1;
    let status_bar_line = grid_snapshot.lines().len();

    zellij.send_stdin(&sgr_left_click(floating_ribbon_column, status_bar_line));
}

#[test]
fn mouse_click_floating_ribbon_in_status_bar_opens_a_floating_pane() {
    let mut zellij = start_zellij(STATUS_BAR_TEST_COLS);
    claim_first_terminal_and_wait_for_prompt(&zellij);

    click_floating_ribbon_in_status_bar(&zellij);
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("a floating pane appeared", |grid_snapshot| {
        grid_snapshot.contains("STAGGERED") && grid_snapshot.contains("┌")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn mouse_click_new_tab_button_in_tab_bar_opens_a_new_tab() {
    let mut zellij = start_zellij(120);
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let grid_snapshot = zellij.wait_until("tab bar shows the new tab button", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .is_some_and(|tab_bar| tab_bar.contains('+'))
    });
    let tab_bar = grid_snapshot.lines().first().cloned().unwrap();
    let new_tab_button_column =
        display_column_of(&tab_bar, "+").expect("new tab button is on the tab bar") + 1;

    zellij.send_stdin(&sgr_left_click(new_tab_button_column, 1));
    let new_tab_terminal = zellij.expect_pty_spawn();
    new_tab_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("a second tab opened", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && grid_snapshot.contains("Tab #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
