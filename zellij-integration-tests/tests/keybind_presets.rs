#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, start_zellij, GridSnapshot,
    Size, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::input::options::Options;

const ARROW_DOWN: &[u8] = b"\x1b[B";
const ARROW_UP: &[u8] = b"\x1b[A";

fn open_keys_screen(zellij: &TestSession) {
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    for _ in 0..2 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("keys screen opened", |grid_snapshot| {
        grid_snapshot.contains("Save as a preset") && grid_snapshot.contains("Preset")
    });
    zellij.send_stdin(&keys::TAB);
}

fn choose_preset(zellij: &TestSession, arrow: &[u8], applied_marker: &str) {
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("preset list opened", |grid_snapshot| {
        grid_snapshot.contains("unlock-first") && grid_snapshot.contains("default")
    });
    zellij.send_stdin(arrow);
    zellij.send_stdin(&keys::ENTER);
    let applied_marker = applied_marker.to_owned();
    zellij.wait_until("preset applied to the running session", |grid_snapshot| {
        grid_snapshot.contains(&applied_marker)
    });
}

fn close_configuration(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('c'));
    zellij.wait_until("configuration plugin closed", |grid_snapshot| {
        !grid_snapshot.contains("Configuration")
    });
}

fn wait_for_unlock_first_status_bar(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until(
        "status bar groups keys the unlock-first way",
        |grid_snapshot| grid_snapshot.contains("PANE") && !grid_snapshot.contains("Ctrl +"),
    )
}

fn wait_for_default_status_bar(zellij: &TestSession) -> GridSnapshot {
    zellij.wait_until("status bar groups keys the default way", |grid_snapshot| {
        grid_snapshot.contains("PANE") && grid_snapshot.contains("Ctrl +")
    })
}

#[test]
fn switching_keybind_presets_updates_the_status_bar_without_a_new_tab() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    choose_preset(&zellij, ARROW_DOWN, "UNLOCK");
    close_configuration(&zellij);

    zellij.send_stdin(&keys::ctrl('g'));
    let grid_snapshot = wait_for_unlock_first_status_bar(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.send_stdin(&keys::key('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    choose_preset(&zellij, ARROW_UP, "Ctrl +");
    close_configuration(&zellij);

    let grid_snapshot = wait_for_default_status_bar(&zellij);
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.quit();
}

#[test]
fn saving_after_switching_to_unlock_first_writes_only_the_preset_attribute() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    choose_preset(&zellij, ARROW_DOWN, "UNLOCK");
    zellij.wait_until("the keys page describes unlock-first", |grid_snapshot| {
        grid_snapshot.contains("Ctrl g + p to enter PANE mode")
            && grid_snapshot.contains("1 unsaved change")
    });

    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("config saved", |grid_snapshot| {
        grid_snapshot.contains("Saved to")
    });

    let config_file_path = zellij.config_file_path().unwrap();
    let written = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(
        written.contains("keybinds preset=\"unlock-first\""),
        "{}",
        written
    );
    assert!(!written.contains("bind \""), "{}", written);
    assert!(!written.contains("keybinds clear-defaults"), "{}", written);
    assert!(
        !written
            .lines()
            .any(|line| line.trim_start().starts_with("default_mode")),
        "{}",
        written
    );

    close_configuration(&zellij);
    zellij.send_stdin(&keys::ctrl('g'));
    wait_for_unlock_first_status_bar(&zellij);

    zellij.quit();
}

#[test]
fn an_old_config_that_clears_the_defaults_gets_exactly_its_own_keys() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(
            r#"
keybinds clear-defaults=true {
    normal {
        bind "Ctrl y" { NewTab; }
    }
    shared {
        bind "Ctrl q" { Quit; }
    }
}
"#,
        )
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    zellij.send_stdin(&keys::ctrl('t'));
    terminal.wait_for_stdin("ctrl t reaches the terminal instead of tab mode", |bytes| {
        bytes.contains(&0x14)
    });
    zellij.send_stdin(&keys::ctrl('o'));
    terminal.wait_for_stdin(
        "ctrl o reaches the terminal instead of session mode",
        |bytes| bytes.contains(&0x0f),
    );

    zellij.send_stdin(&keys::ctrl('y'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("a second tab opened", |grid_snapshot| {
        grid_snapshot.contains("Tab #2")
    });

    zellij.quit();
}

#[test]
fn a_preset_given_on_the_command_line_applies_to_the_session_but_is_not_saved() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_cli_options(Options {
            keybinds_preset: Some("unlock-first".to_owned()),
            ..Default::default()
        })
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("unlock-first status bar in locked mode", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.contains("UNLOCK")
    });

    zellij.send_stdin(&keys::ctrl('g'));
    wait_for_unlock_first_status_bar(&zellij);
    zellij.send_stdin(&keys::key('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    zellij.wait_until(
        "the keys page names the command line preset",
        |grid_snapshot| {
            grid_snapshot.contains("command line")
                && grid_snapshot.contains("unlock-first")
                && grid_snapshot.contains("0 unsaved changes")
        },
    );

    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("config saved", |grid_snapshot| {
        grid_snapshot.contains("Saved to")
    });
    let written = std::fs::read_to_string(zellij.config_file_path().unwrap()).unwrap();
    assert!(!written.contains("unlock-first"), "{}", written);
    assert!(!written.contains("keybinds_preset"), "{}", written);

    close_configuration(&zellij);
    zellij.send_stdin(&keys::ctrl('g'));
    wait_for_unlock_first_status_bar(&zellij);

    zellij.quit();
}

#[test]
fn switching_away_from_custom_keybindings_also_drops_their_default_mode() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(
            r#"
default_mode "locked"
keybinds clear-defaults=true {
    locked {
        bind "Ctrl y" {
            LaunchOrFocusPlugin "configuration" {
                floating true
                move_to_focused_tab true
            };
        }
    }
    shared {
        bind "Ctrl q" { Quit; }
    }
}
"#,
        )
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });

    zellij.send_stdin(&keys::ctrl('y'));
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    for _ in 0..2 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("custom keybindings shown", |grid_snapshot| {
        grid_snapshot.contains("Using custom keybinds block")
    });
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("preset list opened", |grid_snapshot| {
        grid_snapshot.contains("custom") && grid_snapshot.contains("default")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "switch confirmation mentions the default mode",
        |grid_snapshot| {
            grid_snapshot.contains("Switch to a preset?") && grid_snapshot.contains("default mode")
        },
    );
    zellij.send_stdin(b"\x1b[C");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("default preset applied in normal mode", |grid_snapshot| {
        grid_snapshot.contains("Ctrl p to enter PANE mode") && grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("config saved", |grid_snapshot| {
        grid_snapshot.contains("Saved to")
    });
    let written = std::fs::read_to_string(zellij.config_file_path().unwrap()).unwrap();
    assert!(
        !written
            .lines()
            .any(|line| line.trim_start().starts_with("default_mode")),
        "{}",
        written
    );
    assert!(!written.contains("keybinds clear-defaults"), "{}", written);

    zellij.quit();
}

fn row_containing(grid_snapshot: &GridSnapshot, needle: &str) -> String {
    grid_snapshot
        .lines()
        .into_iter()
        .find(|line| line.contains(needle))
        .unwrap_or_default()
}

#[test]
fn tab_moves_through_the_leader_key_fields_before_leaving_the_keys_page() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);

    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the primary key list opened", |grid_snapshot| {
        grid_snapshot.contains("Ctrl Alt")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("primary equal to secondary is refused", |grid_snapshot| {
        grid_snapshot.contains("must differ")
    });

    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the secondary key list opened", |grid_snapshot| {
        grid_snapshot.contains("Ctrl Alt")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the secondary key changed", |grid_snapshot| {
        row_containing(grid_snapshot, "Secondary key").contains("Super")
            && grid_snapshot.contains("1 unsaved change")
    });

    zellij.send_stdin(&keys::TAB);
    zellij.wait_until(
        "the keybindings below the preset are focused",
        |grid_snapshot| grid_snapshot.contains("search keys and actions"),
    );
    for _ in 0..4 {
        zellij.send_stdin(&keys::TAB);
    }
    zellij.wait_until("focus went back to the categories", |grid_snapshot| {
        grid_snapshot.contains("<↓↑> - category")
    });

    zellij.quit();
}

#[test]
fn the_keys_page_describes_the_preset_with_examples_using_the_current_leader_keys() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 40,
    })
    .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    zellij.wait_until("the default preset examples", |grid_snapshot| {
        grid_snapshot.contains("Ctrl p to enter PANE mode")
            && grid_snapshot.contains("Ctrl t to enter TAB mode")
    });
    let grid_snapshot = zellij.snapshot();
    for needle in ["Ctrl p to enter"] {
        let row = grid_snapshot.row_of_line(needle).unwrap();
        let line = grid_snapshot.lines()[row].clone();
        let start = line[..line.find(needle).unwrap()].chars().count();
        let bold_cells = (start..start + needle.chars().count())
            .filter(|column| grid_snapshot.char_is_bold(*column, row))
            .count();
        assert_eq!(bold_cells, 0, "{} is bold", needle);
    }
    choose_preset(&zellij, ARROW_DOWN, "UNLOCK");
    zellij.wait_until("the unlock-first examples", |grid_snapshot| {
        grid_snapshot.contains("Ctrl g + p to enter PANE mode")
    });
    zellij.kill_session();
}

fn left_click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(format!("\u{1b}[<0;{};{}M", column, line).as_bytes());
    zellij.send_stdin(format!("\u{1b}[<0;{};{}m", column, line).as_bytes());
}

#[test]
fn a_preset_from_the_keybinds_folder_links_to_its_file() {
    let keybinds_dir = tempfile::tempdir().unwrap();
    let preset_path = keybinds_dir.path().join("mine.kdl");
    std::fs::write(
        &preset_path,
        "preset {\n    description \"My own keys\"\n}\nkeybinds {\n    locked {\n        bind \"Ctrl g\" { SwitchToMode \"Normal\"; }\n    }\n    shared_except \"locked\" {\n        bind \"Ctrl g\" { SwitchToMode \"Locked\"; }\n        bind \"Ctrl o\" { SwitchToMode \"Session\"; }\n    }\n    session {\n        bind \"c\" { LaunchOrFocusPlugin \"configuration\" { floating true; }; SwitchToMode \"Normal\"; }\n    }\n    shared {\n        bind \"Ctrl q\" { Quit; }\n    }\n}\n",
    )
    .unwrap();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!(
            "mouse_mode true\nkeybinds_dir \"{}\"\nkeybinds preset=\"mine\"",
            keybinds_dir.path().display()
        ))
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    for _ in 0..2 {
        zellij.send_stdin(ARROW_DOWN);
    }
    let grid_snapshot = zellij.wait_until("the preset links to its file", |grid_snapshot| {
        grid_snapshot.contains("mine.kdl") && grid_snapshot.contains("Preset")
    });
    assert!(
        !grid_snapshot.contains("My own keys"),
        "{}",
        grid_snapshot.text
    );
    let row = grid_snapshot.row_of_line("mine.kdl").unwrap();
    let line = grid_snapshot.lines()[row].clone();
    let column = line[..line.find("mine.kdl").unwrap()].chars().count();

    left_click(&zellij, column + 2, row + 1);
    let editor = zellij.expect_pty_spawn();
    let action = format!("{:?}", editor.terminal_action());
    let folder_name = keybinds_dir.path().file_name().unwrap().to_str().unwrap();
    assert!(
        action.contains(&format!("{}/mine.kdl", folder_name)),
        "{}",
        action
    );

    zellij.quit();
}

#[test]
fn the_keys_page_fits_its_fields_at_the_default_size() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    open_keys_screen(&zellij);
    zellij.wait_until("every field of the keys page is shown", |grid_snapshot| {
        grid_snapshot.contains("Ctrl p to enter PANE mode")
            && grid_snapshot.contains("Secondary key")
            && grid_snapshot.contains("Save as a preset")
    });
    for _ in 0..3 {
        zellij.send_stdin(&keys::TAB);
    }
    zellij.wait_until(
        "the last leader key is focused without scrolling the page",
        |grid_snapshot| {
            grid_snapshot.contains("<Space> - change")
                && grid_snapshot.contains("Ctrl p to enter PANE mode")
        },
    );
    zellij.quit();
}

#[test]
fn tab_past_the_fields_of_a_small_keys_page_reaches_the_keybindings_and_back() {
    let mut zellij = TestRunner::new(Size {
        cols: 120,
        rows: 20,
    })
    .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    for _ in 0..2 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("the top of the keys page is shown", |grid_snapshot| {
        grid_snapshot.contains("Ctrl p to enter PANE mode")
    });
    zellij.send_stdin(&keys::TAB);
    for _ in 0..4 {
        zellij.send_stdin(&keys::TAB);
    }
    zellij.wait_until(
        "the keybindings below the preset are focused",
        |grid_snapshot| grid_snapshot.contains("search keys and actions"),
    );
    for _ in 0..4 {
        zellij.send_stdin(&keys::TAB);
    }
    zellij.wait_until(
        "back at the top when focus leaves the page",
        |grid_snapshot| {
            grid_snapshot.contains("<↓↑> - category")
                && grid_snapshot.contains("Ctrl p to enter PANE mode")
        },
    );
    zellij.quit();
}

#[test]
fn the_preset_folder_set_on_the_files_page_lists_its_presets_on_the_keys_page() {
    let keybinds_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        keybinds_dir.path().join("from-new-folder.kdl"),
        "keybinds {\n}\n",
    )
    .unwrap();
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("configuration plugin opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration")
    });
    for _ in 0..14 {
        zellij.send_stdin(ARROW_DOWN);
    }
    zellij.wait_until("files and folders shown", |grid_snapshot| {
        grid_snapshot.contains("Keybinding preset folder")
    });
    zellij.send_stdin(&keys::TAB);
    let mut focused = false;
    for _ in 0..10 {
        let grid_snapshot = zellij.snapshot();
        if grid_snapshot.contains("Folder searched for keybinding presets") {
            focused = true;
            break;
        }
        zellij.send_stdin(ARROW_DOWN);
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(focused, "{}", zellij.snapshot().text);
    zellij.send_stdin(&keys::ENTER);
    zellij.send_stdin(keybinds_dir.path().display().to_string().as_bytes());
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the folder is applied", |grid_snapshot| {
        grid_snapshot.contains("1 unsaved change") && grid_snapshot.contains("● unsaved")
    });
    zellij.send_stdin(b"\x1b[D");
    for _ in 0..12 {
        zellij.send_stdin(ARROW_UP);
    }
    zellij.wait_until("keys page shown", |grid_snapshot| {
        grid_snapshot.contains("Save as a preset")
    });
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "the preset list shows the new folder's preset",
        |grid_snapshot| grid_snapshot.contains("from-new-folder"),
    );
    zellij.quit();
}

#[test]
fn a_command_line_preset_overrides_an_old_config_that_clears_the_defaults() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(
            r#"
keybinds clear-defaults=true {
    normal {
        bind "Ctrl y" { NewTab; }
    }
}
"#,
        )
        .with_cli_options(Options {
            keybinds_preset: Some("default".to_owned()),
            ..Default::default()
        })
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("ctrl p enters pane mode from the preset", |grid_snapshot| {
        grid_snapshot.contains("<p> PANE") && grid_snapshot.contains("Fullscreen")
    });
    assert!(!terminal.stdin_bytes().contains(&0x10));
    zellij.quit();
}

#[test]
fn a_command_line_preset_overrides_the_config_default_mode() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("default_mode \"locked\"")
        .with_cli_options(Options {
            keybinds_preset: Some("default".to_owned()),
            ..Default::default()
        })
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("the session starts in normal mode", |grid_snapshot| {
        grid_snapshot.contains("<p> PANE") && grid_snapshot.contains("Fullscreen")
    });
    assert!(!terminal.stdin_bytes().contains(&0x10));
    zellij.quit();
}
