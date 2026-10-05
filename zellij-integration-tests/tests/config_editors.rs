#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, start_zellij, GridSnapshot, TestRunner,
    TestSession, TERMINAL_SIZE,
};

const ARROW_DOWN: &[u8] = b"\x1b[B";
const ARROW_UP: &[u8] = b"\x1b[A";
const ARROW_LEFT: &[u8] = b"\x1b[D";
const SHIFT_DOWN: &[u8] = b"\x1b[1;2B";
const DELETE: &[u8] = b"\x1b[3~";
const END: &[u8] = b"\x1b[F";
const BACKSPACE: &[u8] = &[0x7f];

const KEYS_PAGE: usize = 2;
const MENU_PAGE: usize = 4;
const THEMES_PAGE: usize = 5;
const PLUGINS_PAGE: usize = 11;

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
        grid_snapshot.contains("Delete Alt n from Normal mode?")
            && grid_snapshot.contains("Don't ask again")
    });
    let question_row = grid_snapshot.row_of_line("Delete Alt n from Normal mode?").unwrap();
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
        assert_eq!(key_color, grid_snapshot.cell_foreground(delete_column + 7, question_row));
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
    let grid_snapshot = zellij.wait_until("deleting asks first", |grid_snapshot| {
        grid_snapshot.contains("Don't ask again") && grid_snapshot.contains("Cancel")
    });
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
    open_page(&zellij, THEMES_PAGE, "Themes from the theme folder");
    zellij.wait_until("themes listed with ours first", |grid_snapshot| {
        grid_snapshot.contains("mine") && grid_snapshot.contains("(active)")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("colour form shown", |grid_snapshot| {
        grid_snapshot.contains("Colours of mine (active")
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
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the colour is applied", |grid_snapshot| {
        grid_snapshot.contains("Colours of mine applied")
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
    assert!(!grid_snapshot.contains("Filter  ["), "{}", grid_snapshot.text);
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
