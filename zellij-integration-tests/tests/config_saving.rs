#![cfg(unix)]

use std::path::Path;
use std::time::{Duration, Instant};

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, default_timeout, keys, start_zellij, TestRunner,
    TestSession, PROMPT, TERMINAL_SIZE,
};

const ARROW_DOWN: &[u8] = b"\x1b[B";

fn open_settings(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("settings screen opened", |grid_snapshot| {
        grid_snapshot.contains("Configuration") && grid_snapshot.contains("unsaved change")
    });
}

fn hide_session_name_in_frames(zellij: &TestSession) {
    zellij.send_stdin(&keys::key('/'));
    zellij.send_stdin(b"hide session");
    zellij.wait_until("search shows the setting", |grid_snapshot| {
        grid_snapshot.contains("Hide session name")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.send_stdin(&keys::SPACE);
    zellij.wait_until("the change is unsaved", |grid_snapshot| {
        grid_snapshot.contains("● unsaved") && grid_snapshot.contains("1 unsaved change ")
    });
}

fn wait_for_file_containing(path: &Path, needle: &str) -> String {
    let deadline = Instant::now() + default_timeout();
    loop {
        let contents = std::fs::read_to_string(path).unwrap_or_default();
        if contents.contains(needle) {
            return contents;
        }
        assert!(
            Instant::now() < deadline,
            "{} never contained {:?}:\n{}",
            path.display(),
            needle,
            contents
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

const HIDE_SESSION_NAME_BLOCK: &str =
    "ui {\n    pane_frames {\n        hide_session_name true\n    }\n}\n";

#[test]
fn saving_writes_only_the_changed_lines_and_keeps_comments() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("// a comment the user wrote\n")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let config_file_path = zellij.config_file_path().unwrap();
    let original = std::fs::read_to_string(&config_file_path).unwrap();
    open_settings(&zellij);
    hide_session_name_in_frames(&zellij);

    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("search closed", |grid_snapshot| {
        grid_snapshot.contains("<↓↑> - category")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(ARROW_DOWN);
    zellij.wait_until("keys page shown", |grid_snapshot| {
        grid_snapshot.contains("Save as a preset")
    });
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("preset list opened", |grid_snapshot| {
        grid_snapshot.contains("unlock-first")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("both changes are unsaved", |grid_snapshot| {
        grid_snapshot.contains("2 unsaved changes") && grid_snapshot.contains("UNLOCK")
    });

    zellij.send_stdin(&keys::ctrl('a'));
    let grid_snapshot = zellij.wait_until("saved without a confirmation", |grid_snapshot| {
        grid_snapshot.contains("Saved to")
    });
    assert!(!grid_snapshot.contains("Save settings?"));
    let written = std::fs::read_to_string(&config_file_path).unwrap();
    assert_eq!(
        written,
        format!(
            "{}{}keybinds preset=\"unlock-first\"\n",
            original, HIDE_SESSION_NAME_BLOCK
        )
    );
    assert!(written.contains("// a comment the user wrote"));
    zellij.wait_until("nothing is left unsaved", |grid_snapshot| {
        grid_snapshot.contains("0 unsaved changes")
    });
    zellij.kill_session();
}

fn change_then_edit_the_file_outside(zellij: &TestSession) -> (std::path::PathBuf, String) {
    let config_file_path = zellij.config_file_path().unwrap();
    open_settings(zellij);
    hide_session_name_in_frames(zellij);
    let mut outside = std::fs::read_to_string(&config_file_path).unwrap();
    outside.push_str("// edited in another program\n");
    std::fs::write(&config_file_path, &outside).unwrap();
    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("the outside change is reported", |grid_snapshot| {
        grid_snapshot.contains("changed outside Zellij")
            && grid_snapshot.contains("Overwrite:")
            && grid_snapshot.contains("Reload:")
            && grid_snapshot.contains("Cancel")
    });
    (config_file_path, outside)
}

#[test]
fn overwrite_after_an_outside_edit_applies_the_changes_to_the_edited_file() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let (config_file_path, outside) = change_then_edit_the_file_outside(&zellij);

    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("saved", |grid_snapshot| {
        grid_snapshot.contains("Saved to") && grid_snapshot.contains("0 unsaved changes")
    });
    let written = std::fs::read_to_string(&config_file_path).unwrap();
    assert_eq!(written, format!("{}{}", outside, HIDE_SESSION_NAME_BLOCK));
    zellij.quit();
}

#[test]
fn reload_after_an_outside_edit_keeps_the_changes_unsaved() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let (config_file_path, outside) = change_then_edit_the_file_outside(&zellij);

    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("reloaded with the change kept", |grid_snapshot| {
        grid_snapshot.contains("Reloaded the config file")
            && grid_snapshot.contains("1 unsaved change ")
    });
    assert_eq!(std::fs::read_to_string(&config_file_path).unwrap(), outside);

    zellij.send_stdin(&keys::ctrl('a'));
    zellij.wait_until("saved without asking again", |grid_snapshot| {
        grid_snapshot.contains("Saved to") && grid_snapshot.contains("0 unsaved changes")
    });
    let written = std::fs::read_to_string(&config_file_path).unwrap();
    assert_eq!(written, format!("{}{}", outside, HIDE_SESSION_NAME_BLOCK));
    zellij.quit();
}

#[test]
fn cancel_after_an_outside_edit_writes_nothing() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let (config_file_path, outside) = change_then_edit_the_file_outside(&zellij);

    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("not saved", |grid_snapshot| {
        grid_snapshot.contains("Not saved")
            && grid_snapshot.contains("1 unsaved change ")
            && !grid_snapshot.contains("changed outside Zellij")
    });
    assert_eq!(std::fs::read_to_string(&config_file_path).unwrap(), outside);
    zellij.quit();
}

#[test]
fn closing_the_settings_pane_during_a_theme_preview_restores_the_theme() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_settings(&zellij);
    zellij.send_stdin(&keys::TAB);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("theme list opened", |grid_snapshot| {
        grid_snapshot.contains("● default") && grid_snapshot.contains("ansi")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.wait_until("the highlighted theme is previewed", |grid_snapshot| {
        grid_snapshot.contains("1 unsaved change ")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));
    zellij.wait_until("settings pane closed", |grid_snapshot| {
        !grid_snapshot.contains("Configuration")
    });

    open_settings(&zellij);
    zellij.wait_until(
        "the original theme is back and nothing is unsaved",
        |grid_snapshot| grid_snapshot.contains("0 unsaved changes"),
    );
    zellij.quit();
}

const COPY_TICKED: &str = "[x] Copy Zellij to a Permanent Folder";

fn start_setup_screen() -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .without_config_file()
        .with_setup_wizard()
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("the setup screen opened", |grid_snapshot| {
        grid_snapshot.contains("Welcome to Zellij!")
            && grid_snapshot.contains("Approve and Finish")
    });
    zellij
}

fn untick_the_copy_if_offered(zellij: &TestSession) -> bool {
    let offered = zellij
        .wait_until("the desktop items are known", |grid_snapshot| {
            !grid_snapshot.contains("Checking this computer")
        })
        .contains(COPY_TICKED);
    if offered {
        zellij.send_stdin(&keys::SPACE);
        zellij.wait_until("the copy is unticked", |grid_snapshot| {
            grid_snapshot.contains("[ ] Copy Zellij to a Permanent Folder")
        });
    }
    offered
}

fn wait_for_state(needle: &str) -> String {
    let path = zellij_utils::desktop_integration::state_path();
    wait_for_file_containing(&path, needle)
}

#[test]
fn the_setup_screen_offers_the_desktop_items_and_the_keybindings_checkbox() {
    let mut zellij = start_setup_screen();
    zellij.wait_until("every part of the screen is shown", |grid_snapshot| {
        grid_snapshot.contains("You can always change this later.")
            && grid_snapshot.contains("[ ] Set up keybindings to avoid collisions with editors.")
            && grid_snapshot.contains("<Esc> Dismiss")
            && grid_snapshot.contains("<Ctrl c> Don't Show Again")
            && !grid_snapshot.contains("Checking this computer")
    });
    let screen = zellij.wait_until("without a desktop there is no desktop entry", |grid_snapshot| {
        !grid_snapshot.contains("Install Desktop Entry")
            && !grid_snapshot.contains("Make Zellij your Default Terminal")
    });
    assert!(!screen.contains("Please Choose a Keybinding Preset"));
    zellij.kill_session();
}

#[test]
fn ticking_the_keybindings_box_continues_to_the_keybindings_and_saves_the_preset() {
    let mut zellij = start_setup_screen();
    let config_file_path = zellij.config_file_path().unwrap();
    let first_run_file = std::fs::read_to_string(&config_file_path).unwrap();
    assert!(first_run_file.contains("//"), "{}", first_run_file);
    assert!(!first_run_file.contains("keybinds preset="));

    untick_the_copy_if_offered(&zellij);
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::SPACE);
    zellij.wait_until("the approve button now continues", |grid_snapshot| {
        grid_snapshot.contains("[x] Set up keybindings to avoid collisions with editors.")
            && grid_snapshot.contains("Approve and Continue")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the keybindings screen follows", |grid_snapshot| {
        grid_snapshot.contains("Please Choose a Keybinding Preset")
    });

    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("preset list opened", |grid_snapshot| {
        grid_snapshot.contains("Unlock First (Avoids Collisions)")
    });
    zellij.send_stdin(ARROW_DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("unlock-first applied", |grid_snapshot| {
        grid_snapshot.contains("UNLOCK")
    });
    zellij.send_stdin(&keys::ctrl('a'));

    let written = wait_for_file_containing(&config_file_path, "keybinds preset=\"unlock-first\"");
    assert!(
        written.starts_with(&first_run_file),
        "the first-run file was not kept:\n{}",
        written
    );
    assert!(!written.contains("AUTOGENERATED"), "{}", written);
    wait_for_state("SetupApproved");
    zellij.wait_until("the screen closed", |grid_snapshot| {
        !grid_snapshot.contains("Welcome to Zellij")
            && !grid_snapshot.contains("Please Choose a Keybinding Preset")
    });
    zellij.kill_session();
}

#[test]
fn dont_show_again_closes_the_screen_and_is_remembered() {
    let mut zellij = start_setup_screen();
    zellij.send_stdin(&keys::ctrl('c'));
    let state = wait_for_state("SetupNeverShow");
    assert!(!state.contains("SetupApproved"), "{}", state);
    zellij.wait_until("the screen closed", |grid_snapshot| {
        !grid_snapshot.contains("Welcome to Zellij!")
    });
    zellij.kill_session();
}

#[test]
fn dismiss_closes_the_screen_without_resolving_it() {
    let mut zellij = start_setup_screen();
    zellij.send_stdin(&keys::ESC);
    let state = wait_for_state("WizardDismissed");
    assert!(!state.contains("SetupNeverShow"), "{}", state);
    assert!(!state.contains("SetupApproved"), "{}", state);
    zellij.wait_until("the screen closed", |grid_snapshot| {
        !grid_snapshot.contains("Welcome to Zellij!")
    });
    zellij.kill_session();
}

const KEYS_LINE: &str = "Set up keybindings to avoid collisions with editors.";
const KEYS_INFO: &str = "Ticking this lets you";

fn click_on(zellij: &TestSession, needle: &str, offset: usize) {
    let grid_snapshot = zellij.wait_until("the text to click is shown", |grid_snapshot| {
        grid_snapshot.contains(needle)
    });
    let (column, row) = grid_snapshot.position_of(needle).unwrap();
    zellij.send_stdin(&keys::sgr_click(column + offset, row));
}

fn open_the_keybindings_info(zellij: &TestSession) {
    zellij.wait_until("the desktop items are known", |grid_snapshot| {
        !grid_snapshot.contains("Checking this computer") && grid_snapshot.contains(KEYS_LINE)
    });
    click_on(zellij, &format!("{} [?]", KEYS_LINE), KEYS_LINE.chars().count() + 2);
    zellij.wait_until("the info popup opened", |grid_snapshot| {
        grid_snapshot.contains(KEYS_INFO) && grid_snapshot.contains("<Esc> - Go back")
    });
}

#[test]
fn esc_closes_the_info_popup_and_the_next_click_reaches_the_setup_screen() {
    let mut zellij = start_setup_screen();
    open_the_keybindings_info(&zellij);
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the info popup closed", |grid_snapshot| {
        !grid_snapshot.contains(KEYS_INFO) && grid_snapshot.contains("Welcome to Zellij!")
    });
    click_on(&zellij, KEYS_LINE, 0);
    zellij.wait_until("the first click ticks the keybindings box", |grid_snapshot| {
        grid_snapshot.contains(&format!("[x] {}", KEYS_LINE))
            && grid_snapshot.contains("Approve and Continue")
    });
    zellij.kill_session();
}

#[test]
fn a_click_outside_the_info_popup_closes_it() {
    let mut zellij = start_setup_screen();
    open_the_keybindings_info(&zellij);
    click_on(&zellij, "You can always change this later.", 0);
    zellij.wait_until("the info popup closed", |grid_snapshot| {
        !grid_snapshot.contains(KEYS_INFO) && grid_snapshot.contains("Welcome to Zellij!")
    });
    let state = zellij_utils::desktop_integration::state_path();
    assert!(
        !std::fs::read_to_string(&state)
            .unwrap_or_default()
            .contains("WizardDismissed"),
        "closing the popup must not dismiss the setup screen"
    );
    zellij.kill_session();
}

#[test]
fn go_back_on_the_keybindings_screen_returns_to_the_setup_screen() {
    let mut zellij = start_setup_screen();
    zellij.wait_until("the desktop items are known", |grid_snapshot| {
        !grid_snapshot.contains("Checking this computer") && grid_snapshot.contains(KEYS_LINE)
    });
    click_on(&zellij, KEYS_LINE, 0);
    zellij.wait_until("the approve button now continues", |grid_snapshot| {
        grid_snapshot.contains("Approve and Continue")
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the keybindings screen follows", |grid_snapshot| {
        grid_snapshot.contains("Please Choose a Keybinding Preset")
            && grid_snapshot.contains("You can always change this later.")
            && grid_snapshot.contains("<Ctrl a> Finish")
    });
    click_on(&zellij, "Go back", 0);
    zellij.wait_until("the setup screen is back", |grid_snapshot| {
        grid_snapshot.contains("Welcome to Zellij!")
            && grid_snapshot.contains("Approve and Continue")
            && !grid_snapshot.contains("Please Choose a Keybinding Preset")
    });
    zellij.kill_session();
}

