#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, split_right_and_wait_for_prompt,
    GridSnapshot, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const PANE_MENU_MARKER: &str = "Close pane";
const TAB_MENU_MARKER: &str = "Move tab right";
const COMMON_MENU_MARKER: &str = "Session manager";

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
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

fn column_of(grid_snapshot: &GridSnapshot, row: usize, needle: &str) -> Option<usize> {
    let line = grid_snapshot.lines().get(row)?.clone();
    let byte_index = line.find(needle)?;
    Some(line[..byte_index].chars().count())
}

fn wait_for_pane_menu(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until("pane context menu opened", |grid_snapshot| {
        grid_snapshot.contains(PANE_MENU_MARKER) && grid_snapshot.contains(COMMON_MENU_MARKER)
    })
}

fn wait_for_menu_to_close(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until("context menu closed", |grid_snapshot| {
        !grid_snapshot.contains(COMMON_MENU_MARKER) && grid_snapshot.status_bar_appears()
    })
}

fn click_menu_item(zellij: &TestSession, grid_snapshot: &GridSnapshot, label: &str) {
    let row = grid_snapshot
        .row_of_line(label)
        .unwrap_or_else(|| panic!("menu item {} is not on screen", label));
    let column = column_of(grid_snapshot, row, label).unwrap();
    left_click(zellij, column + 2, row + 1);
}

#[test]
fn right_click_on_a_pane_opens_the_pane_menu_and_esc_closes_it() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    right_click(&zellij, 30, 10);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    assert!(grid_snapshot.contains("New floating pane"));
    assert!(!grid_snapshot.contains(TAB_MENU_MARKER));

    zellij.send_stdin(&keys::ESC);
    wait_for_menu_to_close(&zellij);
    zellij.quit();
}

#[test]
fn right_click_on_a_tab_opens_the_tab_menu() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.snapshot();
    let tab_column = column_of(&grid_snapshot, 0, "Tab #1").expect("tab bar shows Tab #1");

    right_click(&zellij, tab_column + 2, 1);
    let grid_snapshot = zellij.wait_until("tab context menu opened", |grid_snapshot| {
        grid_snapshot.contains(TAB_MENU_MARKER) && grid_snapshot.contains(COMMON_MENU_MARKER)
    });
    assert!(!grid_snapshot.contains(PANE_MENU_MARKER));

    zellij.send_stdin(&keys::ESC);
    wait_for_menu_to_close(&zellij);
    zellij.quit();
}

#[test]
fn right_click_on_the_status_bar_opens_the_bar_menu() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    right_click(&zellij, 60, TERMINAL_SIZE.rows);
    let grid_snapshot = zellij.wait_until("bar context menu opened", |grid_snapshot| {
        grid_snapshot.contains(COMMON_MENU_MARKER) && grid_snapshot.contains("Detach")
    });
    assert!(!grid_snapshot.contains(PANE_MENU_MARKER));
    assert!(!grid_snapshot.contains(TAB_MENU_MARKER));

    zellij.send_stdin(&keys::ESC);
    wait_for_menu_to_close(&zellij);
    zellij.quit();
}

#[test]
fn choosing_close_pane_closes_the_clicked_pane_and_not_the_focused_one() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    left_terminal.output(b"LEFT-MARKER");
    right_terminal.output(b"RIGHT-MARKER");
    zellij.wait_until("both markers rendered", |grid_snapshot| {
        grid_snapshot.contains("LEFT-MARKER") && grid_snapshot.contains("RIGHT-MARKER")
    });

    right_click(&zellij, 10, 12);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    click_menu_item(&zellij, &grid_snapshot, PANE_MENU_MARKER);

    zellij.wait_until(
        "left pane closed while the right pane stays",
        |grid_snapshot| {
            !grid_snapshot.contains(COMMON_MENU_MARKER)
                && !grid_snapshot.contains("LEFT-MARKER")
                && grid_snapshot.contains("RIGHT-MARKER")
        },
    );
    zellij.quit();
}

fn columns_of_name(grid_snapshot: &GridSnapshot, needle: &str) -> Vec<usize> {
    (0..grid_snapshot.lines().len())
        .filter_map(|row| column_of(grid_snapshot, row, needle))
        .collect()
}

#[test]
fn choosing_rename_pane_renames_the_clicked_pane_and_not_the_focused_one() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    left_terminal.output(b"LEFT-MARKER");
    right_terminal.output(b"RIGHT-MARKER");
    zellij.wait_until("both markers rendered", |grid_snapshot| {
        grid_snapshot.contains("LEFT-MARKER") && grid_snapshot.contains("RIGHT-MARKER")
    });

    right_click(&zellij, 10, 12);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    click_menu_item(&zellij, &grid_snapshot, "Rename pane");
    zellij.wait_until("rename pane mode entered", |grid_snapshot| {
        !grid_snapshot.contains(COMMON_MENU_MARKER) && grid_snapshot.contains("RENAMING PANE")
    });
    for character in "CLICKED".chars() {
        zellij.send_stdin(&keys::key(character));
    }
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until(
        "the clicked pane shows the new name",
        |grid_snapshot| {
            !grid_snapshot.contains("RENAMING PANE") && grid_snapshot.contains("CLICKED")
        },
    );
    let middle = (TERMINAL_SIZE.cols / 2) as usize;
    let new_name_columns = columns_of_name(&grid_snapshot, "CLICKED");
    assert!(
        !new_name_columns.is_empty() && new_name_columns.iter().all(|column| *column < middle),
        "the new name is only on the left (clicked) pane, found at columns {:?}",
        new_name_columns
    );
    assert!(
        columns_of_name(&grid_snapshot, "Pane #2")
            .iter()
            .any(|column| *column >= middle),
        "the focused right pane keeps its name"
    );
    zellij.quit();
}

fn tab_names_in_tab_bar(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot
        .lines()
        .first()
        .map(|line| line.matches("Tab #").count())
        .unwrap_or(0)
}

#[test]
fn choosing_close_tab_closes_the_clicked_tab_and_not_the_active_one() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_tab_terminal = zellij.expect_pty_spawn();
    second_tab_terminal.output(b"SECOND-TAB-MARKER");
    let grid_snapshot = zellij.wait_until("second tab is active", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("SECOND-TAB-MARKER")
            && tab_names_in_tab_bar(grid_snapshot) == 2
    });
    let first_tab_column = column_of(&grid_snapshot, 0, "Tab #1").expect("tab bar shows Tab #1");

    right_click(&zellij, first_tab_column + 2, 1);
    let grid_snapshot = zellij.wait_until("tab context menu opened", |grid_snapshot| {
        grid_snapshot.contains(TAB_MENU_MARKER) && grid_snapshot.contains(COMMON_MENU_MARKER)
    });
    click_menu_item(&zellij, &grid_snapshot, "Close tab");

    zellij.wait_until(
        "first tab closed while the active second tab stays",
        |grid_snapshot| {
            !grid_snapshot.contains(COMMON_MENU_MARKER)
                && grid_snapshot.contains("SECOND-TAB-MARKER")
                && tab_names_in_tab_bar(grid_snapshot) == 1
        },
    );
    zellij.quit();
}

#[test]
fn choosing_new_floating_pane_opens_a_floating_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    right_click(&zellij, 30, 10);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    click_menu_item(&zellij, &grid_snapshot, "New floating pane");

    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(b"FLOAT-MARKER");
    zellij.wait_until("floating pane opened", |grid_snapshot| {
        !grid_snapshot.contains(COMMON_MENU_MARKER) && grid_snapshot.contains("FLOAT-MARKER")
    });
    zellij.quit();
}

#[test]
fn click_outside_the_menu_closes_it_and_does_nothing_else() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    right_click(&zellij, 90, 12);
    wait_for_pane_menu(&zellij);

    left_click(&zellij, 10, 12);
    wait_for_menu_to_close(&zellij);
    zellij.wait_until("focus stayed on the right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    left_click(&zellij, 10, 12);
    zellij.wait_until(
        "a later click focuses the left pane as usual",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
        },
    );
    zellij.quit();
}

#[test]
fn opening_the_menu_keeps_floating_panes_hidden_or_visible() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(b"FLOAT-MARKER");
    zellij.wait_until("floating pane visible", |grid_snapshot| {
        grid_snapshot.contains("FLOAT-MARKER")
    });

    right_click(&zellij, 5, 20);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    assert!(grid_snapshot.contains("FLOAT-MARKER"));
    zellij.send_stdin(&keys::ESC);
    wait_for_menu_to_close(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("floating pane hidden", |grid_snapshot| {
        !grid_snapshot.contains("FLOAT-MARKER") && grid_snapshot.status_bar_appears()
    });

    right_click(&zellij, 30, 10);
    let grid_snapshot = wait_for_pane_menu(&zellij);
    assert!(!grid_snapshot.contains("FLOAT-MARKER"));
    zellij.send_stdin(&keys::ESC);
    let grid_snapshot = wait_for_menu_to_close(&zellij);
    assert!(!grid_snapshot.contains("FLOAT-MARKER"));
    zellij.quit();
}

#[test]
fn a_second_attached_user_does_not_see_the_menu() {
    let mut zellij = start_zellij();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    right_click(&zellij, 30, 10);
    wait_for_pane_menu(&zellij);
    terminal.output(b"AFTER-MENU-MARKER");
    zellij.wait_until("main client still shows the menu", |grid_snapshot| {
        grid_snapshot.contains(PANE_MENU_MARKER) && grid_snapshot.contains("AFTER-MENU-MARKER")
    });
    let second_grid = second_client
        .wait_until("second client rendered the output", |grid_snapshot| {
            grid_snapshot.contains("AFTER-MENU-MARKER")
        });
    assert!(!second_grid.contains(PANE_MENU_MARKER));
    assert!(!second_grid.contains(COMMON_MENU_MARKER));

    zellij.send_stdin(&keys::ESC);
    wait_for_menu_to_close(&zellij);
    second_client.quit();
    zellij.quit();
}

#[test]
fn a_program_that_wants_the_mouse_still_receives_plain_right_clicks() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(b"\x1b[?1000h\x1b[?1006h");
    terminal.output(b"MOUSE-APP");
    zellij.wait_until("program output rendered", |grid_snapshot| {
        grid_snapshot.contains("MOUSE-APP")
    });

    right_click(&zellij, 30, 10);
    terminal.wait_for_stdin("right click reported to the program", |stdin| {
        String::from_utf8_lossy(stdin).contains("\u{1b}[<2;")
    });
    assert!(!zellij.snapshot().contains(COMMON_MENU_MARKER));
    zellij.quit();
}
