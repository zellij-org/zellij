#![cfg(unix)]

use zellij_integration_tests::{
    assert_same_rendered_grid, claim_first_terminal_and_wait_for_prompt, keys,
    split_right_and_wait_for_prompt, GridSnapshot, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

const PANE_MODE_HINT: &str = "- Close pane";
const STATUS_BAR_MENU_ITEM: &str = "Hide status bar";
const HIDE_ALL_MENU_ITEM: &str = "Hide all bars";
const SHOW_BARS_MENU_ITEM: &str = "Show bars";
const PANE_HIDE_BARS_MENU_ITEM: &str = "Hide bars";

fn start_zellij_with(extra_config: &str) -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(extra_config)
        .start()
}

fn toggle_bars_via_keybinding(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('Z'));
}

fn bars_are_hidden(grid_snapshot: &GridSnapshot) -> bool {
    !grid_snapshot.tab_bar_appears() && !grid_snapshot.status_bar_appears()
}

fn sgr_press(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn sgr_release(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}m", button, column, line).into_bytes()
}

fn right_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr_press(column, line, 2));
    zellij.send_stdin(&sgr_release(column, line, 2));
}

fn left_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr_press(column, line, 0));
    zellij.send_stdin(&sgr_release(column, line, 0));
}

fn click_menu_item(zellij: &TestSession, grid_snapshot: &GridSnapshot, label: &str) {
    let row = grid_snapshot
        .row_of_line(label)
        .unwrap_or_else(|| panic!("menu item {} is not on screen", label));
    let line = grid_snapshot.lines()[row].clone();
    let byte_index = line.find(label).unwrap();
    let column = line[..byte_index].chars().count();
    left_click(zellij, column + 2, row + 1);
}

#[test]
fn the_toggle_key_hides_the_bars_and_brings_them_back_unchanged() {
    let mut zellij = start_zellij_with("hidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    let before = zellij.wait_until("two panes settled with their bars", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    toggle_bars_via_keybinding(&zellij);
    zellij.wait_until("both bars hidden", bars_are_hidden);

    toggle_bars_via_keybinding(&zellij);
    let after = zellij.wait_until("both bars shown again", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
    });
    assert_same_rendered_grid(&after, &before, "panes and bars are restored exactly");
    zellij.quit();
}

#[test]
fn hiding_one_bar_leaves_the_other_in_place() {
    let mut zellij = start_zellij_with("hidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(CliAction::HideBars {
        bars: vec!["tab-bar".to_owned()],
    });
    assert_eq!(exit_code, 0);
    zellij.wait_until("only the tab bar is hidden", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    let exit_code = zellij.run_cli_action(CliAction::ShowBars);
    assert_eq!(exit_code, 0);
    zellij.wait_until("the tab bar is back", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn hiding_the_bars_says_how_to_bring_them_back() {
    let mut zellij = start_zellij_with("hidden_bars_notification true");
    claim_first_terminal_and_wait_for_prompt(&zellij);

    toggle_bars_via_keybinding(&zellij);
    zellij.wait_until("the notice names what was hidden", |grid_snapshot| {
        bars_are_hidden(grid_snapshot) && grid_snapshot.contains("Bars hidden.")
    });
    zellij.quit();
}

#[test]
fn key_hints_appear_in_a_mode_while_the_bars_are_hidden_and_leave_with_it() {
    let mut zellij = start_zellij_with("hidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);

    toggle_bars_via_keybinding(&zellij);
    zellij.wait_until("both bars hidden", |grid_snapshot| {
        bars_are_hidden(grid_snapshot) && !grid_snapshot.contains(PANE_MODE_HINT)
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until(
        "pane mode keys shown in a framed corner popup",
        |grid_snapshot| {
            bars_are_hidden(grid_snapshot)
                && grid_snapshot.contains("╭─ PANE ")
                && grid_snapshot.contains("╰─")
                && grid_snapshot.contains(PANE_MODE_HINT)
        },
    );

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("key hints closed back in the base mode", |grid_snapshot| {
        bars_are_hidden(grid_snapshot) && !grid_snapshot.contains(PANE_MODE_HINT)
    });
    zellij.quit();
}

#[test]
fn key_hints_stay_away_while_the_status_bar_still_shows_the_mode() {
    let mut zellij = start_zellij_with("hidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);

    let exit_code = zellij.run_cli_action(CliAction::HideBars {
        bars: vec!["tab-bar".to_owned()],
    });
    assert_eq!(exit_code, 0);
    zellij.wait_until("only the tab bar is hidden", |grid_snapshot| {
        !grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('p'));
    let grid_snapshot = zellij.wait_until("the status bar shows pane mode", |grid_snapshot| {
        grid_snapshot.contains("PANE") && grid_snapshot.contains("Fullscreen")
    });
    assert!(
        !grid_snapshot.contains(PANE_MODE_HINT),
        "no key hints while the status bar shows the mode:\n{}",
        grid_snapshot.text
    );

    let exit_code = zellij.run_cli_action(CliAction::HideBars {
        bars: vec!["status-bar".to_owned()],
    });
    assert_eq!(exit_code, 0);
    zellij.wait_until(
        "key hints appear once the status bar is hidden too",
        |grid_snapshot| bars_are_hidden(grid_snapshot) && grid_snapshot.contains(PANE_MODE_HINT),
    );
    zellij.quit();
}

#[test]
fn key_hints_only_show_to_the_user_who_is_in_the_mode() {
    let mut zellij = start_zellij_with("hidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let second_client = zellij.attach_client(TERMINAL_SIZE);

    toggle_bars_via_keybinding(&zellij);
    zellij.wait_until("both bars hidden", |grid_snapshot| {
        bars_are_hidden(grid_snapshot) && !grid_snapshot.contains(PANE_MODE_HINT)
    });
    second_client.wait_until("both bars hidden for the second user", bars_are_hidden);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("the first user sees the key hints", |grid_snapshot| {
        grid_snapshot.contains(PANE_MODE_HINT)
    });
    let second_grid = second_client.snapshot();
    assert!(
        !second_grid.contains(PANE_MODE_HINT),
        "the second user is not in a mode and sees no key hints:\n{}",
        second_grid.text
    );
    zellij.quit();
}

#[test]
fn the_right_click_menu_hides_the_clicked_bar_and_shows_the_bars_again() {
    let mut zellij = start_zellij_with("mouse_mode true\nhidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("both bars shown", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    right_click(&zellij, 60, TERMINAL_SIZE.rows);
    let grid_snapshot = zellij.wait_until("the bar menu offers to hide bars", |grid_snapshot| {
        grid_snapshot.contains(STATUS_BAR_MENU_ITEM) && grid_snapshot.contains(HIDE_ALL_MENU_ITEM)
    });
    assert!(
        !grid_snapshot.contains(SHOW_BARS_MENU_ITEM),
        "nothing is hidden yet, so there is nothing to show:\n{}",
        grid_snapshot.text
    );
    click_menu_item(&zellij, &grid_snapshot, STATUS_BAR_MENU_ITEM);
    zellij.wait_until("only the status bar is hidden", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && !grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains(HIDE_ALL_MENU_ITEM)
    });

    right_click(&zellij, 30, 10);
    let grid_snapshot = zellij
        .wait_until("the pane menu offers to show the bars", |grid_snapshot| {
            grid_snapshot.contains(SHOW_BARS_MENU_ITEM)
        });
    click_menu_item(&zellij, &grid_snapshot, SHOW_BARS_MENU_ITEM);
    zellij.wait_until("both bars are back", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains(SHOW_BARS_MENU_ITEM)
    });
    zellij.quit();
}

#[test]
fn the_pane_menu_hides_every_bar_and_then_offers_to_show_them() {
    let mut zellij = start_zellij_with("mouse_mode true\nhidden_bars_notification false");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("both bars shown", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    right_click(&zellij, 30, 10);
    let grid_snapshot =
        zellij.wait_until("the pane menu offers to hide the bars", |grid_snapshot| {
            grid_snapshot.contains(PANE_HIDE_BARS_MENU_ITEM) && grid_snapshot.contains("Close pane")
        });
    assert!(
        !grid_snapshot.contains(SHOW_BARS_MENU_ITEM),
        "nothing is hidden yet, so there is nothing to show:\n{}",
        grid_snapshot.text
    );
    click_menu_item(&zellij, &grid_snapshot, PANE_HIDE_BARS_MENU_ITEM);
    zellij.wait_until("every bar is hidden", |grid_snapshot| {
        bars_are_hidden(grid_snapshot) && !grid_snapshot.contains("Close pane")
    });

    right_click(&zellij, 30, 10);
    let grid_snapshot =
        zellij.wait_until("the pane menu offers to show the bars", |grid_snapshot| {
            grid_snapshot.contains(SHOW_BARS_MENU_ITEM) && grid_snapshot.contains("Close pane")
        });
    assert!(
        !grid_snapshot.contains(PANE_HIDE_BARS_MENU_ITEM),
        "every bar is already hidden:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}
