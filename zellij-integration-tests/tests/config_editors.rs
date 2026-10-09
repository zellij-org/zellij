#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, GridSnapshot, TestRunner,
    TestSession, TERMINAL_SIZE,
};

const ARROW_DOWN: &[u8] = b"\x1b[B";
const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";
const ARROW_RIGHT: &[u8] = b"\x1b[C";
const SHIFT_DOWN: &[u8] = b"\x1b[1;2B";
const DELETE: &[u8] = b"\x1b[3~";
const END: &[u8] = b"\x1b[F";
const BACKSPACE: &[u8] = &[0x7f];

const KEYS_PAGE: usize = 2;
const MOUSE_BINDINGS_PAGE: usize = 4;
const MENU_PAGE: usize = 5;
const THEMES_PAGE: usize = 6;
const PLUGINS_PAGE: usize = 12;

fn open_settings(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("settings screen opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration") && grid_snapshot.contains("unsaved change")
    });
}

fn open_page(zellij: &TestSession, index: usize, marker: &str) {
    for _ in 0..index {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("settings page shown", |grid_snapshot| {
        grid_snapshot.contains(marker)
    });
    zellij.send_stdin(&keys::TAB);
}

fn open_keybindings(zellij: &TestSession) {
    open_settings(zellij);
    open_page(zellij, KEYS_PAGE, "Save as a preset");
    zellij.send_stdin(&keys::key('/'));
    zellij.wait_until("the keybinding search is focused", |grid_snapshot| {
        grid_snapshot.contains("search keys and actions")
    });
    for _ in 0..3 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("keybinding list focused", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
}

fn type_text(zellij: &TestSession, text: &str) {
    zellij.send_stdin(text.as_bytes());
}

fn save_and_wait(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("config saved", |grid_snapshot| {
        grid_snapshot.contains("Saved") && grid_snapshot.contains("0 unsaved changes")
    });
}

fn close_settings(zellij: &TestSession) {
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("settings closed", |grid_snapshot| {
        !grid_snapshot.contains("Configuration") && grid_snapshot.status_bar_appears()
    });
}

fn capture_key(zellij: &TestSession, key: &[u8]) {
    zellij.wait_until("ready to capture a key", |grid_snapshot| {
        grid_snapshot.contains("press a key")
    });
    zellij.send_stdin(key);
}

fn choose_action(zellij: &TestSession, search: &str, action: &str) {
    zellij.wait_until("action search shown", |grid_snapshot| {
        grid_snapshot.contains("type to search")
    });
    type_text(zellij, search);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("action argument form shown", |grid_snapshot| {
        grid_snapshot.contains("[ Apply ]") && grid_snapshot.contains("Action ")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("action added", |grid_snapshot| {
        grid_snapshot.contains(action) && grid_snapshot.contains("+ Add action")
    });
}

fn press_done(zellij: &TestSession) {
    zellij.send_stdin(ARROW_UP);
    zellij.send_stdin(ARROW_LEFT);
    zellij.send_stdin(&keys::ENTER);
}

fn choose_action_without_arguments(zellij: &TestSession, search: &str, action: &str) {
    choose_action(zellij, search, action);
    press_done(zellij);
}

#[test]
fn the_keys_page_shows_the_preset_above_the_keybindings() {
    let mut zellij = TestRunner::new(zellij_integration_tests::Size {
        cols: 120,
        rows: 50,
    })
    .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);
    open_page(&zellij, KEYS_PAGE, "Save as a preset");
    let grid_snapshot = zellij.wait_until("preset and keybindings on one page", |grid_snapshot| {
        grid_snapshot.contains("Mark all") && grid_snapshot.contains("Primary key")
    });
    assert!(
        grid_snapshot.row_of_line("Primary key").unwrap()
            < grid_snapshot.row_of_line("Mark all").unwrap()
    );
    zellij.quit();
}

#[test]
fn up_from_the_keybindings_goes_back_to_the_preset_fields() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_keybindings(&zellij);
    for _ in 0..4 {
        zellij.send_stdin(ARROW_UP);
    }
    zellij.wait_until("the preset fields are focused again", |grid_snapshot| {
        grid_snapshot.contains("<Space> - change") && grid_snapshot.contains("Primary key")
    });
    zellij.quit();
}

#[test]
fn a_key_rebound_in_normal_mode_works_live_and_saves_only_its_bind_line() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_keybindings(&zellij);

    zellij.send_stdin(&keys::key('a'));
    capture_key(&zellij, &keys::alt('y'));
    choose_action_without_arguments(&zellij, "newtab", "NewTab");
    zellij.wait_until("the new binding is listed as unsaved", |grid_snapshot| {
        grid_snapshot.contains("Alt y") && grid_snapshot.contains("1 unsaved change ")
    });
    close_settings(&zellij);

    zellij.send_stdin(&keys::alt('y'));
    zellij.expect_pty_spawn();
    zellij.wait_until("the rebound key opened a tab", |grid_snapshot| {
        grid_snapshot.contains("Tab #2")
    });

    open_settings(&zellij);
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    let added = "keybinds {\n    normal {\n        bind \"Alt y\" { NewTab; }\n    }\n}\n";
    assert!(saved.contains(added), "{}", saved);
    assert_eq!(saved.replacen(added, "", 1), original);
    zellij.quit();
}

#[test]
fn a_removed_preset_key_is_written_as_unbind_and_reset_back() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_keybindings(&zellij);

    zellij.send_stdin(&keys::key('/'));
    type_text(&zellij, "alt n");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the preset key is found", |grid_snapshot| {
        grid_snapshot.contains("Normal") && grid_snapshot.contains("Alt n")
    });
    zellij.send_stdin(DELETE);
    let grid_snapshot = zellij.wait_until("removal is confirmed first", |grid_snapshot| {
        let (Some(question_row), Some(toggle_row), Some(buttons_row)) = (
            grid_snapshot.row_of_line("Delete Alt n from Normal mode?"),
            grid_snapshot.row_of_line("Don't ask again"),
            grid_snapshot.row_of_line("Cancel"),
        ) else {
            return false;
        };
        let lines = grid_snapshot.lines();
        let Some(question_line) = lines.get(question_row) else {
            return false;
        };
        let Some(delete_offset) = question_line.find("Delete Alt n") else {
            return false;
        };
        let delete_column = question_line[..delete_offset].chars().count();
        let plain_color = grid_snapshot.cell_foreground(delete_column, question_row);
        let key_color = grid_snapshot.cell_foreground(delete_column + 7, question_row);
        question_row >= 2
            && lines[question_row - 2].contains("Confirm")
            && lines[buttons_row].contains("Delete")
            && toggle_row < buttons_row
            && key_color.is_some()
            && key_color != plain_color
            && (delete_column + 7..delete_column + 12)
                .all(|column| grid_snapshot.cell_foreground(column, question_row) == key_color)
            && grid_snapshot.cell_foreground(delete_column + 13, question_row) == plain_color
    });
    let question_row = grid_snapshot
        .row_of_line("Delete Alt n from Normal mode?")
        .unwrap();
    let toggle_row = grid_snapshot.row_of_line("Don't ask again").unwrap();
    let buttons_row = grid_snapshot.row_of_line("Cancel").unwrap();
    assert!(grid_snapshot.lines()[question_row - 2].contains("Confirm"));
    assert!(question_row < toggle_row && toggle_row < buttons_row);
    let question_line = grid_snapshot.lines()[question_row].clone();
    let delete_column = question_line[..question_line.find("Delete Alt n").unwrap()]
        .chars()
        .count();
    let key_columns = delete_column + 7..delete_column + 12;
    let plain_color = grid_snapshot.cell_foreground(delete_column, question_row);
    for column in key_columns {
        let key_color = grid_snapshot.cell_foreground(column, question_row);
        assert!(key_color.is_some() && key_color != plain_color);
        assert_eq!(
            key_color,
            grid_snapshot.cell_foreground(delete_column + 7, question_row)
        );
    }
    assert_eq!(
        grid_snapshot.cell_foreground(delete_column + 13, question_row),
        plain_color
    );
    assert!(grid_snapshot.lines()[buttons_row].contains("Delete"));
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the key is unbound", |grid_snapshot| {
        grid_snapshot.contains("(unbound)") && grid_snapshot.contains("1 unsaved change ")
    });
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(
        saved.contains("    normal {\n        unbind \"Alt n\"\n    }\n"),
        "{}",
        saved
    );

    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("the key is back to the preset", |grid_snapshot| {
        grid_snapshot.contains("back to the preset") && grid_snapshot.contains("1 unsaved change ")
    });
    save_and_wait(&zellij);
    let reset = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(!reset.contains("Alt n"), "{}", reset);
    assert!(reset.starts_with(original.trim_end()), "{}", reset);
    zellij.quit();
}

#[test]
fn binding_a_key_that_is_already_bound_warns_first() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_keybindings(&zellij);

    zellij.send_stdin(&keys::key('a'));
    capture_key(&zellij, &keys::alt('n'));
    zellij.wait_until("the conflict is reported", |grid_snapshot| {
        grid_snapshot.contains("Key already bound")
            && grid_snapshot.contains("Alt n is already bound in Normal mode")
            && grid_snapshot.contains("Replace")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the warning closed and capture goes on", |grid_snapshot| {
        !grid_snapshot.contains("Key already bound") && grid_snapshot.contains("press a key")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("capture stopped", |grid_snapshot| {
        grid_snapshot.contains("Enter captures a key")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the form is left with nothing changed", |grid_snapshot| {
        !grid_snapshot.contains("Add Key") && grid_snapshot.contains("0 unsaved changes")
    });
    zellij.quit();
}

#[test]
fn an_env_variable_and_a_plugin_alias_are_added_and_saved() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_settings(&zellij);
    open_page(&zellij, PLUGINS_PAGE, "Plugin aliases");
    zellij.wait_until("every list is shown on one page", |grid_snapshot| {
        grid_snapshot.contains("zellij:about") && grid_snapshot.contains("<a> - add")
    });

    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("alias form shown", |grid_snapshot| {
        grid_snapshot.contains("Add Plugin Alias")
    });
    type_text(&zellij, "mine");
    zellij.send_stdin(&keys::TAB);
    type_text(&zellij, "zellij:strider");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("alias added", |grid_snapshot| {
        grid_snapshot
            .row_of_line("  mine  ")
            .map(|row| grid_snapshot.lines()[row].contains("zellij:strider"))
            .unwrap_or(false)
            && grid_snapshot.contains("1 unsaved change ")
    });

    zellij.send_stdin(END);
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("variable form shown", |grid_snapshot| {
        grid_snapshot.contains("Add Variable")
    });
    type_text(&zellij, "EDITOR");
    zellij.send_stdin(&keys::TAB);
    type_text(&zellij, "vim");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("variable added", |grid_snapshot| {
        grid_snapshot
            .row_of_line("EDITOR")
            .map(|row| grid_snapshot.lines()[row].contains("vim"))
            .unwrap_or(false)
            && grid_snapshot.contains("2 unsaved changes")
    });
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    let plugins = "plugins {\n    mine location=\"zellij:strider\"\n}\n";
    let env = "env {\n    EDITOR \"vim\"\n}\n";
    assert!(saved.contains(plugins), "{}", saved);
    assert!(saved.contains(env), "{}", saved);
    assert_eq!(
        saved.replacen(plugins, "", 1).replacen(env, "", 1),
        original
    );
    zellij.quit();
}

const ADDED_VARIABLE: &str = "ZJ_TEST_ADDED";
const REMOVED_VARIABLE: &str = "ZJ_TEST_REMOVED";

#[test]
fn an_env_variable_added_at_runtime_reaches_new_panes_but_not_the_server() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);
    open_page(&zellij, PLUGINS_PAGE, "Plugin aliases");
    zellij.wait_until("every list is shown on one page", |grid_snapshot| {
        grid_snapshot.contains("zellij:about") && grid_snapshot.contains("<a> - add")
    });
    zellij.send_stdin(END);
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("variable form shown", |grid_snapshot| {
        grid_snapshot.contains("Add Variable")
    });
    type_text(&zellij, ADDED_VARIABLE);
    zellij.send_stdin(&keys::TAB);
    type_text(&zellij, "added");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("variable added", |grid_snapshot| {
        grid_snapshot.contains(ADDED_VARIABLE) && grid_snapshot.contains("1 unsaved change ")
    });
    close_settings(&zellij);
    let new_terminal = zellij_integration_tests::split_right_and_wait_for_prompt(&zellij);
    assert_eq!(
        new_terminal.env_var(ADDED_VARIABLE).as_deref(),
        Some("added")
    );
    assert_eq!(first_terminal.env_var(ADDED_VARIABLE), None);
    assert!(std::env::var(ADDED_VARIABLE).is_err());
    zellij.quit();
}

#[test]
fn an_env_variable_removed_at_runtime_is_not_set_in_new_panes() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!("env {{\n    {} \"start\"\n}}", REMOVED_VARIABLE))
        .start();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    assert_eq!(
        first_terminal.env_var(REMOVED_VARIABLE).as_deref(),
        Some("start")
    );
    open_settings(&zellij);
    open_page(&zellij, PLUGINS_PAGE, "Plugin aliases");
    zellij.wait_until("every list is shown on one page", |grid_snapshot| {
        grid_snapshot.contains("zellij:about") && grid_snapshot.contains("<a> - add")
    });
    zellij.send_stdin(END);
    zellij.wait_until("the variable is listed", |grid_snapshot| {
        grid_snapshot.contains(REMOVED_VARIABLE)
    });
    zellij.send_stdin(ARROW_UP);
    zellij.wait_until("the variable is selected", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
    zellij.send_stdin(DELETE);
    zellij.wait_until("deleting asks first", |grid_snapshot| {
        grid_snapshot.contains("Don't ask again")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the variable is removed", |grid_snapshot| {
        !grid_snapshot.contains(REMOVED_VARIABLE) && grid_snapshot.contains("1 unsaved change ")
    });
    close_settings(&zellij);
    let new_terminal = zellij_integration_tests::split_right_and_wait_for_prompt(&zellij);
    assert_eq!(new_terminal.env_var(REMOVED_VARIABLE), None);
    assert_eq!(
        first_terminal.env_var(REMOVED_VARIABLE).as_deref(),
        Some("start")
    );
    zellij.quit();
}

fn right_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(format!("\u{1b}[<2;{};{}M", column, line).as_bytes());
    zellij.send_stdin(format!("\u{1b}[<2;{};{}m", column, line).as_bytes());
}

fn sgr_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(format!("\u{1b}[<0;{};{}M", column, line).as_bytes());
    zellij.send_stdin(format!("\u{1b}[<0;{};{}m", column, line).as_bytes());
}

fn click_text(zellij: &TestSession, grid_snapshot: &GridSnapshot, needle: &str, nth: usize) {
    let mut found = 0;
    for (row, line) in grid_snapshot.lines().iter().enumerate() {
        let mut search_from = 0;
        while let Some(byte_index) = line[search_from..].find(needle) {
            let byte_index = search_from + byte_index;
            if found == nth {
                let column = line[..byte_index].chars().count();
                sgr_click(zellij, column + 2, row + 1);
                return;
            }
            found += 1;
            search_from = byte_index + needle.len();
        }
    }
    panic!("{} (#{}) is not on screen", needle, nth);
}

fn open_menu_page(zellij: &TestSession) {
    open_settings(zellij);
    open_page(zellij, MENU_PAGE, "Pane menu");
    zellij.wait_until("menu sections listed together", |grid_snapshot| {
        grid_snapshot.contains("Pane menu") && grid_snapshot.contains("<a> - add")
    });
}

#[test]
fn a_new_right_click_menu_item_shows_in_the_menu_with_its_shortcut() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    open_menu_page(&zellij);

    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("menu item form shown", |grid_snapshot| {
        grid_snapshot.contains("Add Item") && grid_snapshot.contains("Kind")
    });
    type_text(&zellij, "Hello");
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    choose_action(&zellij, "newtab", "NewTab");
    press_done(&zellij);
    zellij.wait_until("the item is listed with its shortcut", |grid_snapshot| {
        grid_snapshot
            .row_of_line("Hello")
            .map(|row| grid_snapshot.lines()[row].contains("Ctrl t, n"))
            .unwrap_or(false)
            && grid_snapshot.contains("1 unsaved change ")
    });
    close_settings(&zellij);

    right_click(&zellij, 30, 10);
    zellij.wait_until(
        "the menu shows the new item and its shortcut",
        |grid_snapshot| grid_snapshot.contains("Hello") && grid_snapshot.contains("Ctrl t, n"),
    );
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("menu closed", |grid_snapshot| {
        !grid_snapshot.contains("Hello") && grid_snapshot.status_bar_appears()
    });

    open_settings(&zellij);
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(!saved.contains("clear-defaults"), "{}", saved);
    assert!(
        saved.contains("item \"Hello\" after=\"New pane\" { NewTab; }"),
        "{}",
        saved
    );
    zellij.quit();
}

#[test]
fn menu_items_and_separators_are_edited_with_the_mouse() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    open_menu_page(&zellij);

    zellij.send_stdin(END);
    let grid_snapshot = zellij.wait_until("the common items are shown", |grid_snapshot| {
        grid_snapshot.contains("Common items") && grid_snapshot.contains("Detach")
    });
    click_text(&zellij, &grid_snapshot, "+ Add separator", 0);
    zellij.wait_until("a separator was added", |grid_snapshot| {
        grid_snapshot.contains("1 unsaved change ")
    });

    let grid_snapshot = zellij.wait_until("the item is shown", |grid_snapshot| {
        grid_snapshot.contains("Detach")
    });
    click_text(&zellij, &grid_snapshot, "Detach", 0);
    let grid_snapshot = zellij.wait_until("the item is selected", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
    click_text(&zellij, &grid_snapshot, "Detach", 0);
    zellij.wait_until("the edit popup opened", |grid_snapshot| {
        grid_snapshot.contains("Edit Item") && grid_snapshot.contains("[ Done ]")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the edit popup closed", |grid_snapshot| {
        !grid_snapshot.contains("Edit Item")
    });
    zellij.send_stdin(DELETE);
    let grid_snapshot = zellij.wait_until(
        "deleting asks first in a popup sized to its content",
        |grid_snapshot| {
            grid_snapshot.contains("Don't ask again")
                && grid_snapshot.contains("Cancel")
                && grid_snapshot.contains("<Esc> - cancel │")
        },
    );
    let row = grid_snapshot.row_of_line("Cancel").unwrap();
    let line = grid_snapshot.lines()[row].clone();
    let column = line[..line.find("Delete").unwrap()].chars().count();
    sgr_click(&zellij, column + 2, row + 1);
    zellij.wait_until("the item is gone", |grid_snapshot| {
        !grid_snapshot.contains("Detach") && grid_snapshot.contains("Common items")
    });

    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(!saved.contains("clear-defaults"), "{}", saved);
    assert!(saved.contains("remove \"Detach\""), "{}", saved);
    assert!(saved.contains("separator"), "{}", saved);
    zellij.quit();
}

fn column_of_text(grid_snapshot: &GridSnapshot, row_needle: &str, needle: &str) -> usize {
    let row = grid_snapshot
        .row_of_line(row_needle)
        .unwrap_or_else(|| panic!("no row with {}:\n{}", row_needle, grid_snapshot.text));
    let line = grid_snapshot.lines()[row].clone();
    let byte_index = line
        .find(needle)
        .unwrap_or_else(|| panic!("{} is not on the row of {}: {}", needle, row_needle, line));
    line[..byte_index].chars().count()
}

#[test]
fn menu_columns_line_up_and_rows_show_the_hover() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_menu_page(&zellij);
    let grid_snapshot = zellij.wait_until("pane items listed", |grid_snapshot| {
        grid_snapshot.contains("New pane") && grid_snapshot.contains("Fullscreen")
    });
    assert_eq!(
        column_of_text(&grid_snapshot, "New pane", "Alt n"),
        column_of_text(&grid_snapshot, "Fullscreen", "Ctrl p, f")
    );
    assert_eq!(
        column_of_text(&grid_snapshot, "New pane", "NewPane"),
        column_of_text(&grid_snapshot, "Fullscreen", "ToggleFocus")
    );
    assert!(
        column_of_text(&grid_snapshot, "New pane", "NewPane")
            < column_of_text(&grid_snapshot, "New pane", "Alt n")
    );

    let row = grid_snapshot.row_of_line("Fullscreen").unwrap();
    let column = column_of_text(&grid_snapshot, "Fullscreen", "Fullscreen");
    let before = format!("{:?}", grid_snapshot.cell_style(column, row).background);
    zellij.send_stdin(format!("\u{1b}[<35;{};{}M", column + 2, row + 1).as_bytes());
    zellij.wait_until("the hovered row is highlighted", |grid_snapshot| {
        format!("{:?}", grid_snapshot.cell_style(column, row).background) != before
    });
    zellij.quit();
}

fn tab_background(grid_snapshot: &GridSnapshot) -> String {
    let line = grid_snapshot.lines()[0].clone();
    let byte_index = line.find("Tab #1").unwrap();
    let column = line[..byte_index].chars().count();
    format!("{:?}", grid_snapshot.cell_style(column, 0).background)
}

const THEME_CONFIG: &str = "theme \"mine\"
themes {
    mine {
        fg 250
        bg 236
        red 160
        green 70
        blue 32
        yellow 178
        magenta 133
        orange 208
        cyan 37
        black 16
        white 255
    }
}
";

#[test]
fn a_colour_of_the_active_config_file_theme_applies_live_and_saves() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(THEME_CONFIG)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let before = tab_background(&zellij.wait_until("tab bar drawn", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
    }));
    open_settings(&zellij);
    open_page(&zellij, THEMES_PAGE, "+ New theme");
    zellij.wait_until("themes listed with ours first", |grid_snapshot| {
        grid_snapshot
            .row_of_line("  mine  ")
            .map(|row| grid_snapshot.lines()[row].contains("active"))
            .unwrap_or(false)
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("colour form shown", |grid_snapshot| {
        grid_snapshot.contains("Colors of mine (active: changes preview live)")
    });
    for _ in 0..19 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("the ribbon background is focused", |grid_snapshot| {
        grid_snapshot.contains("ribbon_selected.background")
    });
    zellij.send_stdin(END);
    for _ in 0..12 {
        zellij.send_stdin(BACKSPACE);
    }
    type_text(&zellij, "#010203");
    zellij.wait_until("the new colour previews on the tab bar", |grid_snapshot| {
        tab_background(grid_snapshot) != before
    });
    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("the colour is applied", |grid_snapshot| {
        grid_snapshot.contains("Colors of mine applied")
    });
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(saved.contains("ribbon_selected {"), "{}", saved);
    assert!(saved.contains("background 1 2 3"), "{}", saved);
    assert!(saved.contains("theme \"mine\""), "{}", saved);
    zellij.quit();
}

#[test]
fn the_keybinding_search_finds_keys_of_every_mode_and_hides_the_dropdowns() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);
    open_page(&zellij, KEYS_PAGE, "Save as a preset");
    zellij.send_stdin(&keys::key('/'));
    zellij.wait_until("the keybinding search is focused", |grid_snapshot| {
        grid_snapshot.contains("search keys and actions") && grid_snapshot.contains("Filter")
    });
    type_text(&zellij, "newtab");
    let grid_snapshot = zellij.wait_until("tab keys of several modes found", |grid_snapshot| {
        grid_snapshot.contains("Tmux") && grid_snapshot.contains("NewTab")
    });
    assert!(
        !grid_snapshot.contains("Filter  ["),
        "{}",
        grid_snapshot.text
    );
    assert!(
        grid_snapshot
            .lines()
            .iter()
            .filter(|line| line.contains("NewTab"))
            .all(|line| !line.contains("Switch Modes")),
        "{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the search is cleared", |grid_snapshot| {
        grid_snapshot.contains("Filter") && grid_snapshot.contains("Mark all")
    });
    zellij.quit();
}

#[test]
fn every_key_of_a_category_is_marked_and_deleted_together() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    open_keybindings(&zellij);
    zellij.send_stdin(&keys::key('C'));
    zellij.wait_until("the category is marked", |grid_snapshot| {
        grid_snapshot.contains("✓") && grid_snapshot.contains("delete marked")
    });
    zellij.send_stdin(DELETE);
    zellij.wait_until("deleting several keys asks first", |grid_snapshot| {
        grid_snapshot.contains("keys?") && grid_snapshot.contains("Don't ask again")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the keys are unbound", |grid_snapshot| {
        grid_snapshot.contains("(unbound)") && grid_snapshot.contains("1 unsaved change ")
    });
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(saved.matches("unbind").count() > 1, "{}", saved);
    zellij.quit();
}

#[test]
fn menu_items_move_with_shift_arrows_and_the_menu_search_finds_them() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_menu_page(&zellij);
    let grid_snapshot = zellij.wait_until("pane items listed", |grid_snapshot| {
        grid_snapshot.contains("New pane") && grid_snapshot.contains("New floating pane")
    });
    let new_pane_row = grid_snapshot.row_of_line("New pane ").unwrap();
    let floating_row = grid_snapshot.row_of_line("New floating pane").unwrap();
    assert!(new_pane_row < floating_row);
    zellij.send_stdin(SHIFT_DOWN);
    zellij.wait_until("the first item moved down", |grid_snapshot| {
        match (
            grid_snapshot.row_of_line("New pane "),
            grid_snapshot.row_of_line("New floating pane"),
        ) {
            (Some(new_pane), Some(floating)) => {
                new_pane > floating && grid_snapshot.contains("1 unsaved change ")
            },
            _ => false,
        }
    });
    zellij.send_stdin(&keys::key('/'));
    type_text(&zellij, "detach");
    zellij.wait_until("the search lists the matching item", |grid_snapshot| {
        grid_snapshot.contains("Common items")
            && grid_snapshot.contains("Detach")
            && !grid_snapshot.contains("New floating pane")
    });
    zellij.quit();
}

fn wait_for_mouse_bindings_page(zellij: &TestSession) {
    zellij.wait_until("mouse bindings page shown", |grid_snapshot| {
        grid_snapshot.contains("From preset:") && grid_snapshot.contains("Mouse bindings")
    });
}

fn enter_mouse_binding_search(zellij: &TestSession) {
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::key('/'));
    zellij.wait_until("the mouse binding search is focused", |grid_snapshot| {
        grid_snapshot.contains("- search mouse bindings and actions")
    });
}

fn open_mouse_binding_search(zellij: &TestSession) {
    open_settings(zellij);
    for _ in 0..MOUSE_BINDINGS_PAGE {
        zellij.send_stdin(ARROW_DOWN);
    }
    wait_for_mouse_bindings_page(zellij);
    enter_mouse_binding_search(zellij);
}

fn open_mouse_bindings(zellij: &TestSession) {
    open_mouse_binding_search(zellij);
    for _ in 0..3 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("mouse binding list focused", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
}

fn start_add_mouse_binding(zellij: &TestSession, button_steps: usize) {
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("mouse binding form shown", |grid_snapshot| {
        grid_snapshot.contains("Add Mouse Binding") && grid_snapshot.contains("Button")
    });
    for _ in 0..button_steps {
        zellij.send_stdin(ARROW_RIGHT);
    }
    for _ in 0..7 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.send_stdin(&keys::ENTER);
}

fn press_mouse_form_done(zellij: &TestSession) {
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
}

fn middle_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(format!("\u{1b}[<1;{};{}M", column, line).as_bytes());
    zellij.send_stdin(format!("\u{1b}[<1;{};{}m", column, line).as_bytes());
}

#[test]
fn a_mouse_binding_added_in_normal_mode_works_live_and_saves_only_its_bind_line() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_mouse_bindings(&zellij);

    start_add_mouse_binding(&zellij, 2);
    choose_action(&zellij, "newtab", "NewTab");
    press_mouse_form_done(&zellij);
    zellij.wait_until(
        "the new mouse binding is listed as unsaved",
        |grid_snapshot| {
            !grid_snapshot.contains("Add Mouse Binding")
                && grid_snapshot.contains("1 unsaved change ")
        },
    );
    close_settings(&zellij);

    middle_click(&zellij, 30, 10);
    zellij.expect_pty_spawn();
    zellij.wait_until("the middle click opened a tab", |grid_snapshot| {
        grid_snapshot.contains("Tab #2")
    });

    open_settings(&zellij);
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    let added = "mousebinds {\n    shared {\n        bind \"Middle\" { NewTab; }\n    }\n}\n";
    assert!(saved.contains(added), "{}", saved);
    assert_eq!(saved.replacen(added, "", 1), original);
    zellij.quit();
}

#[test]
fn a_mouse_binding_added_for_one_mode_saves_a_block_for_that_mode_only() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_mouse_binding_search(&zellij);
    zellij.send_stdin(ARROW_DOWN);
    zellij.wait_until("the mode dropdown is focused", |grid_snapshot| {
        grid_snapshot.contains("choose mode") && grid_snapshot.contains("All modes")
    });
    zellij.send_stdin(ARROW_RIGHT);
    zellij.wait_until("normal mode is chosen", |grid_snapshot| {
        grid_snapshot.contains("Normal") && !grid_snapshot.contains("All modes")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(ARROW_DOWN);
    zellij.wait_until("mouse binding list focused", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });

    start_add_mouse_binding(&zellij, 2);
    choose_action(&zellij, "newtab", "NewTab");
    press_mouse_form_done(&zellij);
    zellij.wait_until(
        "the new mouse binding is listed as unsaved",
        |grid_snapshot| {
            !grid_snapshot.contains("Add Mouse Binding")
                && grid_snapshot.contains("1 unsaved change ")
                && grid_snapshot.contains("Bound Middle in Normal mode")
        },
    );
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    let added = "mousebinds {\n    normal {\n        bind \"Middle\" { NewTab; }\n    }\n}\n";
    assert!(saved.contains(added), "{}", saved);
    assert_eq!(saved.replacen(added, "", 1), original);
    zellij.quit();
}

#[test]
fn a_removed_preset_mouse_binding_is_written_as_unbind_and_reset_back() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_mouse_binding_search(&zellij);

    type_text(&zellij, "alt right");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the preset mouse binding is found", |grid_snapshot| {
        grid_snapshot.contains("Alt Right") && grid_snapshot.contains("<Del> - delete")
    });
    zellij.send_stdin(DELETE);
    zellij.wait_until("removal is confirmed first", |grid_snapshot| {
        grid_snapshot.contains("Delete Alt Right from all modes?")
            && grid_snapshot.contains("Don't ask again")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the mouse binding is unbound", |grid_snapshot| {
        grid_snapshot.contains("(unboun") && grid_snapshot.contains("1 unsaved change ")
    });
    save_and_wait(&zellij);
    let saved = std::fs::read_to_string(&config_file_path).unwrap();
    let removed = "mousebinds {\n    unbind \"Alt Right\"\n}\n";
    assert!(saved.contains(removed), "{}", saved);
    assert_eq!(saved.replacen(removed, "", 1), original);

    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("the mouse binding is back to the preset", |grid_snapshot| {
        grid_snapshot.contains("back to the preset") && grid_snapshot.contains("1 unsaved change ")
    });
    save_and_wait(&zellij);
    let reset = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(!reset.contains("Alt Right"), "{}", reset);
    assert!(!reset.contains("mousebinds"), "{}", reset);
    assert!(reset.starts_with(original.trim_end()), "{}", reset);
    zellij.quit();
}

#[test]
fn adding_a_mouse_binding_for_an_already_bound_trigger_warns_first() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_mouse_bindings(&zellij);

    start_add_mouse_binding(&zellij, 1);
    choose_action(&zellij, "newtab", "NewTab");
    press_mouse_form_done(&zellij);
    zellij.wait_until("the conflict is reported", |grid_snapshot| {
        grid_snapshot.contains("Trigger already bound")
            && grid_snapshot.contains("Right is already bound in all modes")
            && grid_snapshot.contains("Replace")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the warning closed and the form stays", |grid_snapshot| {
        !grid_snapshot.contains("Trigger already bound")
            && grid_snapshot.contains("Add Mouse Binding")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the form is left with nothing changed", |grid_snapshot| {
        !grid_snapshot.contains("Add Mouse Binding") && grid_snapshot.contains("0 unsaved changes")
    });
    zellij.quit();
}

#[test]
fn switching_the_preset_updates_the_leader_keys_of_the_mouse_bindings() {
    let keybinds_dir = tempfile::tempdir().unwrap();
    let default_preset = include_str!("../../zellij-utils/assets/keybinds/default.kdl");
    let swapped_preset = default_preset
        .replace(
            "primary \"Ctrl\"\n        secondary \"Alt\"",
            "primary \"Alt\"\n        secondary \"Ctrl\"",
        )
        .replace("name \"Default\"", "name \"Swapped\"");
    assert!(swapped_preset.contains("primary \"Alt\""));
    std::fs::write(keybinds_dir.path().join("swapped.kdl"), swapped_preset).unwrap();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!(
            "keybinds_dir \"{}\"",
            keybinds_dir.path().display()
        ))
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_mouse_binding_search(&zellij);
    type_text(&zellij, "resizepane");
    zellij.wait_until(
        "the frame resize binding uses the default primary key",
        |grid_snapshot| {
            grid_snapshot.contains("Ctrl Left · frame")
                && !grid_snapshot.contains("Alt Left · frame")
        },
    );
    zellij.send_stdin(ARROW_DOWN);
    zellij.wait_until("search results focused", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
    zellij.send_stdin(ARROW_LEFT);
    zellij.wait_until("the category menu is focused", |grid_snapshot| {
        grid_snapshot.contains("<↓↑> - category")
    });
    zellij.send_stdin(ARROW_UP);
    zellij.send_stdin(ARROW_UP);
    zellij.wait_until("keys page shown", |grid_snapshot| {
        grid_snapshot.contains("Save as a preset")
    });
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("preset list opened", |grid_snapshot| {
        grid_snapshot.contains("unlock-first") && grid_snapshot.contains("swapped")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the swapped preset is applied", |grid_snapshot| {
        grid_snapshot.contains("Alt +") && grid_snapshot.contains("[ swapped")
    });

    zellij.send_stdin(&keys::key('/'));
    zellij.wait_until("the keybinding search is focused", |grid_snapshot| {
        grid_snapshot.contains("search keys and actions")
    });
    for _ in 0..3 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("keybinding list focused", |grid_snapshot| {
        grid_snapshot.contains("<Del> - delete")
    });
    zellij.send_stdin(ARROW_LEFT);
    zellij.wait_until("the category menu is focused", |grid_snapshot| {
        grid_snapshot.contains("<↓↑> - category")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(ARROW_DOWN);
    wait_for_mouse_bindings_page(&zellij);
    enter_mouse_binding_search(&zellij);
    zellij.wait_until(
        "the frame resize binding follows the new primary key",
        |grid_snapshot| {
            grid_snapshot.contains("Alt Left · frame")
                && !grid_snapshot.contains("Ctrl Left · frame")
        },
    );
    zellij.kill_session();
}
