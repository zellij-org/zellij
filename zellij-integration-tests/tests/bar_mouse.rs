#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, split_right_and_wait_for_prompt,
    FakePtyHandle, GridSnapshot, LayoutInfo, Size, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const WIDE_TERMINAL_SIZE: Size = Size {
    cols: 190,
    rows: 24,
};

const COMPACT_LAYOUT_WITH_TOOLTIP: &str = r#"
layout {
    pane
    pane size=1 borderless=true {
        plugin location="compact-bar" {
            tooltip "F1"
        }
    }
}
"#;

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
}

fn start_wide_zellij() -> TestSession {
    TestRunner::new(WIDE_TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
}

fn start_zellij_with_compact_bar() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .with_layout(LayoutInfo::Stringified(
            COMPACT_LAYOUT_WITH_TOOLTIP.to_owned(),
        ))
        .start()
}

fn sgr(button: u8, column: usize, line: usize, final_byte: char) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}{}", button, column, line, final_byte).into_bytes()
}

fn press(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr(0, column, line, 'M'));
}

fn drag_to(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr(32, column, line, 'M'));
}

fn release(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr(0, column, line, 'm'));
}

fn left_click(zellij: &TestSession, column: usize, line: usize) {
    press(zellij, column, line);
    release(zellij, column, line);
}

fn right_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr(2, column, line, 'M'));
    zellij.send_stdin(&sgr(2, column, line, 'm'));
}

fn column_of(grid_snapshot: &GridSnapshot, row: usize, needle: &str) -> usize {
    let line = grid_snapshot
        .lines()
        .get(row)
        .cloned()
        .unwrap_or_else(|| panic!("row {} is not on screen", row));
    let byte_index = line
        .find(needle)
        .unwrap_or_else(|| panic!("{:?} is not on row {}: {:?}", needle, row, line));
    line[..byte_index].chars().count()
}

fn last_row(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot.lines().len().saturating_sub(1)
}

fn row_order(line: &str, labels: &[&str]) -> bool {
    let mut search_from = 0;
    for label in labels {
        match line[search_from..].find(label) {
            Some(offset) => search_from += offset + label.len(),
            None => return false,
        }
    }
    true
}

fn first_row_order(grid_snapshot: &GridSnapshot, labels: &[&str]) -> bool {
    grid_snapshot
        .lines()
        .first()
        .map(|line| row_order(line, labels))
        .unwrap_or(false)
}

fn open_marked_tab(zellij: &TestSession, expected_tab: &str, marker: &str) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    let expected_tab = expected_tab.to_owned();
    let marker = marker.to_owned();
    zellij.wait_until("new marked tab opened", move |grid_snapshot| {
        grid_snapshot.contains(&expected_tab) && grid_snapshot.contains(&marker)
    });
    terminal
}

fn three_marked_tabs(zellij: &TestSession) -> GridSnapshot {
    let first_terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    first_terminal.output(b"FIRST-TAB");
    zellij.wait_until("first tab marked", |grid_snapshot| {
        grid_snapshot.contains("FIRST-TAB")
    });
    open_marked_tab(zellij, "Tab #2", "SECOND-TAB");
    open_marked_tab(zellij, "Tab #3", "THIRD-TAB");
    zellij.wait_until("three tabs in order", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && first_row_order(grid_snapshot, &["Tab #1", "Tab #2", "Tab #3"])
    })
}

#[test]
fn dragging_a_tab_moves_it_live_and_releasing_focuses_it() {
    let mut zellij = start_zellij();
    let grid_snapshot = three_marked_tabs(&zellij);
    let first_tab = column_of(&grid_snapshot, 0, "Tab #1");
    let second_tab = column_of(&grid_snapshot, 0, "Tab #2");

    press(&zellij, first_tab + 2, 1);
    drag_to(&zellij, second_tab + 5, 1);
    let grid_snapshot = zellij.wait_until("tab one moved past tab two", |grid_snapshot| {
        first_row_order(grid_snapshot, &["Tab #2", "Tab #1", "Tab #3"])
    });
    assert!(grid_snapshot.contains("THIRD-TAB"));

    let third_tab = column_of(&grid_snapshot, 0, "Tab #3");
    drag_to(&zellij, third_tab + 5, 1);
    zellij.wait_until("tab one moved past tab three", |grid_snapshot| {
        first_row_order(grid_snapshot, &["Tab #2", "Tab #3", "Tab #1"])
    });

    release(&zellij, third_tab + 5, 1);
    zellij.wait_until("the dragged tab is focused", |grid_snapshot| {
        grid_snapshot.contains("FIRST-TAB")
            && !grid_snapshot.contains("THIRD-TAB")
            && first_row_order(grid_snapshot, &["Tab #2", "Tab #3", "Tab #1"])
    });
    zellij.quit();
}

#[test]
fn dragging_within_the_same_tab_does_not_move_it() {
    let mut zellij = start_zellij();
    let grid_snapshot = three_marked_tabs(&zellij);
    let third_tab = column_of(&grid_snapshot, 0, "Tab #3");

    press(&zellij, third_tab + 1, 1);
    drag_to(&zellij, third_tab + 4, 1);
    release(&zellij, third_tab + 4, 1);
    open_marked_tab(&zellij, "Tab #4", "FOURTH-TAB");
    zellij.wait_until("order is unchanged", |grid_snapshot| {
        first_row_order(grid_snapshot, &["Tab #1", "Tab #2", "Tab #3", "Tab #4"])
    });
    zellij.quit();
}

#[test]
fn clicking_a_tab_focuses_it() {
    let mut zellij = start_zellij();
    let grid_snapshot = three_marked_tabs(&zellij);
    let first_tab = column_of(&grid_snapshot, 0, "Tab #1");

    left_click(&zellij, first_tab + 2, 1);
    zellij.wait_until("first tab focused", |grid_snapshot| {
        grid_snapshot.contains("FIRST-TAB")
            && first_row_order(grid_snapshot, &["Tab #1", "Tab #2", "Tab #3"])
    });
    zellij.quit();
}

fn click_menu_item(zellij: &TestSession, grid_snapshot: &GridSnapshot, label: &str) {
    let row = grid_snapshot
        .row_of_line(label)
        .unwrap_or_else(|| panic!("menu item {} is not on screen", label));
    let column = column_of(grid_snapshot, row, label);
    left_click(zellij, column + 2, row + 1);
}

fn open_tab_menu(zellij: &TestSession, tab: &str) -> GridSnapshot {
    let grid_snapshot = zellij.snapshot();
    let tab_column = column_of(&grid_snapshot, 0, tab);
    right_click(zellij, tab_column + 2, 1);
    zellij.wait_until("tab menu opened", |grid_snapshot| {
        grid_snapshot.contains("Move tab to start") && grid_snapshot.contains("Move tab to end")
    })
}

#[test]
fn the_tab_menu_moves_a_tab_to_the_start_and_to_the_end() {
    let mut zellij = start_zellij();
    three_marked_tabs(&zellij);

    let grid_snapshot = open_tab_menu(&zellij, "Tab #3");
    click_menu_item(&zellij, &grid_snapshot, "Move tab to start");
    zellij.wait_until("tab three moved to the start", |grid_snapshot| {
        !grid_snapshot.contains("Move tab to end")
            && first_row_order(grid_snapshot, &["Tab #3", "Tab #1", "Tab #2"])
    });

    let grid_snapshot = open_tab_menu(&zellij, "Tab #3");
    click_menu_item(&zellij, &grid_snapshot, "Move tab to end");
    zellij.wait_until("tab three moved back to the end", |grid_snapshot| {
        !grid_snapshot.contains("Move tab to start")
            && first_row_order(grid_snapshot, &["Tab #1", "Tab #2", "Tab #3"])
    });
    zellij.quit();
}

#[test]
fn clicking_a_mode_in_the_status_bar_enters_it_and_clicking_it_again_leaves_it() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.snapshot();
    let status_row = last_row(&grid_snapshot);
    let pane_column = column_of(&grid_snapshot, status_row, "PANE");

    left_click(&zellij, pane_column + 2, status_row + 1);
    let grid_snapshot = zellij.wait_until("pane mode entered", |grid_snapshot| {
        !grid_snapshot.status_bar_appears() && grid_snapshot.contains("PANE")
    });

    let pane_column = column_of(&grid_snapshot, status_row, "PANE");
    left_click(&zellij, pane_column + 2, status_row + 1);
    zellij.wait_until("back in normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn clicking_an_arrow_in_the_status_bar_moves_focus() {
    let mut zellij = start_wide_zellij();
    let left_terminal = zellij.expect_pty_spawn();
    left_terminal.output(PROMPT);
    zellij.wait_until("first terminal loaded", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    let grid_snapshot = zellij.wait_until("focus hint shown", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.cursor_is_at(col(2).row(2))
            && grid_snapshot
                .lines()
                .last()
                .map(|line| line.contains('←') && line.contains("Change Focus"))
                .unwrap_or(false)
    });
    let status_row = last_row(&grid_snapshot);
    let left_arrow = column_of(&grid_snapshot, status_row, "←");

    left_click(&zellij, left_arrow + 1, status_row + 1);
    zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    zellij.quit();
}

#[test]
fn clicking_plus_in_resize_mode_grows_the_focused_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    let (initial_cols, _) =
        right_terminal.wait_for_size("right pane has a size", |cols, _| cols > 0);

    zellij.send_stdin(&keys::ctrl('n'));
    let grid_snapshot = zellij.wait_until(
        "resize mode shows the plus and minus keys",
        |grid_snapshot| {
            grid_snapshot
                .lines()
                .last()
                .map(|line| line.contains("+|-"))
                .unwrap_or(false)
        },
    );
    let status_row = last_row(&grid_snapshot);
    let plus = column_of(&grid_snapshot, status_row, "+|-");

    left_click(&zellij, plus + 1, status_row + 1);
    right_terminal.wait_for_size("right pane grew", |cols, _| cols > initial_cols);
    zellij.quit();
}

fn wait_for_compact_bar(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("compact bar loaded", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && grid_snapshot.contains("NORMAL")
    });
    terminal
}

fn open_compact_tab(zellij: &TestSession, expected_tab: &str, marker: &str) {
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    let expected_tab = expected_tab.to_owned();
    let marker = marker.to_owned();
    zellij.wait_until("compact tab opened", move |grid_snapshot| {
        grid_snapshot.contains(&expected_tab)
            && grid_snapshot.contains(&marker)
            && grid_snapshot.contains("NORMAL")
    });
}

fn last_row_order(grid_snapshot: &GridSnapshot, labels: &[&str]) -> bool {
    grid_snapshot
        .lines()
        .last()
        .map(|line| row_order(line, labels))
        .unwrap_or(false)
}

#[test]
fn dragging_a_tab_in_the_compact_bar_moves_it() {
    let mut zellij = start_zellij_with_compact_bar();
    let first_terminal = wait_for_compact_bar(&zellij);
    first_terminal.output(b"FIRST-TAB");
    open_compact_tab(&zellij, "Tab #2", "SECOND-TAB");
    let grid_snapshot = zellij.wait_until("two compact tabs", |grid_snapshot| {
        last_row_order(grid_snapshot, &["Tab #1", "Tab #2"])
    });
    let bar_row = last_row(&grid_snapshot);
    let first_tab = column_of(&grid_snapshot, bar_row, "Tab #1");
    let second_tab = column_of(&grid_snapshot, bar_row, "Tab #2");

    press(&zellij, first_tab + 2, bar_row + 1);
    drag_to(&zellij, second_tab + 5, bar_row + 1);
    zellij.wait_until("compact tabs swapped", |grid_snapshot| {
        last_row_order(grid_snapshot, &["Tab #2", "Tab #1"])
    });
    release(&zellij, second_tab + 5, bar_row + 1);
    zellij.wait_until("the dragged compact tab is focused", |grid_snapshot| {
        grid_snapshot.contains("FIRST-TAB") && !grid_snapshot.contains("SECOND-TAB")
    });
    zellij.quit();
}

#[test]
fn clicking_the_compact_mode_label_returns_to_normal_mode() {
    let mut zellij = start_zellij_with_compact_bar();
    wait_for_compact_bar(&zellij);
    zellij.send_stdin(&keys::ctrl('t'));
    let grid_snapshot = zellij.wait_until("tab mode shown", |grid_snapshot| {
        grid_snapshot.contains(" TAB ") && !grid_snapshot.contains("NORMAL")
    });
    let bar_row = last_row(&grid_snapshot);
    let mode_column = column_of(&grid_snapshot, bar_row, " TAB ");

    left_click(&zellij, mode_column + 2, bar_row + 1);
    zellij.wait_until("normal mode shown", |grid_snapshot| {
        grid_snapshot.contains("NORMAL")
    });
    zellij.quit();
}

#[test]
fn clicking_plus_in_the_compact_tooltip_grows_the_focused_pane() {
    let mut zellij = start_zellij_with_compact_bar();
    wait_for_compact_bar(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("right pane opened", |grid_snapshot| {
        grid_snapshot.contains("NORMAL") && grid_snapshot.contains("Pane #2")
    });
    let (initial_cols, _) =
        right_terminal.wait_for_size("right pane has a size", |cols, _| cols > 0);

    zellij.send_stdin(&keys::ctrl('n'));
    let grid_snapshot = zellij.wait_until("resize tooltip shown", |grid_snapshot| {
        grid_snapshot.contains("<+->") && grid_snapshot.contains("Increase or decrease size")
    });
    let row = grid_snapshot
        .row_of_line("<+->")
        .expect("tooltip row with plus and minus");
    let plus = column_of(&grid_snapshot, row, "<+->") + 1;

    left_click(&zellij, plus + 1, row + 1);
    right_terminal.wait_for_size("right pane grew", |cols, _| cols > initial_cols);
    zellij.quit();
}

fn hover(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr(35, column, line, 'M'));
}

#[test]
fn the_hover_stays_on_a_status_bar_button_after_clicking_it() {
    let mut zellij = start_wide_zellij();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    let grid_snapshot =
        zellij.wait_until("status bar shows the floating button", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot
                    .lines()
                    .last()
                    .map(|line| line.contains("Floating"))
                    .unwrap_or(false)
        });
    let status_row = last_row(&grid_snapshot);
    let floating = column_of(&grid_snapshot, status_row, "Floating") + 2;
    let plain_style = grid_snapshot.cell_style(floating, status_row);

    hover(&zellij, floating + 1, status_row + 1);
    let grid_snapshot = zellij.wait_until("floating button highlighted", |grid_snapshot| {
        grid_snapshot.cell_style(floating, status_row) != plain_style
    });
    let hovered_style = grid_snapshot.cell_style(floating, status_row);

    left_click(&zellij, floating + 1, status_row + 1);
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(b"FLOATING-MARKER");
    let grid_snapshot = zellij.wait_until("floating pane opened", |grid_snapshot| {
        grid_snapshot.contains("FLOATING-MARKER") && grid_snapshot.status_bar_appears()
    });
    assert_eq!(
        grid_snapshot.cell_style(floating, status_row),
        hovered_style
    );
    zellij.quit();
}

#[test]
fn the_hover_follows_a_mode_label_that_changes_under_a_resting_mouse() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.snapshot();
    let status_row = last_row(&grid_snapshot);
    let lock_column = column_of(&grid_snapshot, status_row, "LOCK") + 1;
    let plain_style = grid_snapshot.cell_style(lock_column, status_row);

    hover(&zellij, lock_column + 1, status_row + 1);
    zellij.wait_until("lock label highlighted", |grid_snapshot| {
        grid_snapshot.cell_style(lock_column, status_row) != plain_style
    });
    left_click(&zellij, lock_column + 1, status_row + 1);
    let grid_snapshot = zellij.wait_until("locked mode entered", |grid_snapshot| {
        !grid_snapshot.contains("PANE") && grid_snapshot.contains("LOCK")
    });
    assert_eq!(
        column_of(&grid_snapshot, status_row, "LOCK") + 1,
        lock_column
    );
    let hovered_locked_style = grid_snapshot.cell_style(lock_column, status_row);

    hover(&zellij, 30, 10);
    zellij.wait_until("hover left the locked label", |grid_snapshot| {
        grid_snapshot.cell_style(lock_column, status_row) != hovered_locked_style
    });
    zellij.quit();
}
