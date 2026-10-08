#![cfg(unix)]

use std::time::Instant;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, default_timeout, keys,
    split_right_and_wait_for_prompt, BackgroundTestSession, LayoutInfo, TestRunner, TestSession,
    PROMPT, TERMINAL_SIZE,
};

const SESSION_FEATURES_ON: &str = "session_card true\nsession_indicator true\nmouse_mode true";
const CARD_TITLE: &str = "Running Sessions";
const DONT_SHOW_AGAIN: &str = "Don't show again";
const SHOW_ALL_SHORTCUT: &str = "<Ctrl f>";
const RESUME_LAYOUT: &str = r#"
layout {
    pane size=1 borderless=true {
        plugin location="tab-bar"
    }
    pane
    pane size=1 borderless=true {
        plugin location="status-bar"
    }
}
"#;

fn start_running_session_in_background() -> BackgroundTestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .skip_concurrency_slot()
        .start_in_background()
}

fn start_session_with_session_features() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .start()
}

fn wait_for_card_listing(zellij: &TestSession, session_name: &str) {
    zellij.wait_until("the session card lists the other session", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE) && grid_snapshot.contains(session_name)
    });
}

fn sgr_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn position_of(grid_snapshot: &zellij_integration_tests::GridSnapshot, needle: &str) -> (usize, usize) {
    grid_snapshot
        .lines()
        .iter()
        .enumerate()
        .find_map(|(line_index, line)| {
            line.find(needle).map(|byte_offset| {
                (line[..byte_offset].chars().count() + 1, line_index + 1)
            })
        })
        .expect("the text is on screen")
}

fn focus_the_card_with_f9(zellij: &TestSession) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the session card is focused", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE) && grid_snapshot.contains(SHOW_ALL_SHORTCUT)
    });
}

fn open_and_close_session_manager(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("session manager opened", |grid_snapshot| {
        grid_snapshot.contains("Session: ")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("session manager closed", |grid_snapshot| {
        !grid_snapshot.contains("Session: ")
    });
}

fn wait_for_path_to_disappear(path: &std::path::Path, what: &str) {
    let deadline = Instant::now() + default_timeout();
    while path.exists() {
        if Instant::now() >= deadline {
            panic!("timed out waiting for {} to be removed", what);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

#[test]
fn card_shows_on_a_new_session_when_another_session_is_running() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    zellij.quit();
}

#[test]
fn without_other_running_sessions_there_is_no_card_but_the_dropdown_still_opens_it() {
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("the dropdown says there are no other sessions", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .is_some_and(|tab_bar| tab_bar.contains("No Other Sessions"))
    });
    assert!(!grid_snapshot.contains(CARD_TITLE));
    let (column, line) = position_of(&grid_snapshot, "No Other Sessions");
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until("the dropdown opened the empty card", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE) && grid_snapshot.contains("No other sessions yet")
    });
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until("the dropdown closed the card", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    zellij.quit();
}

#[test]
fn card_is_not_shown_on_attach() {
    let _other = start_running_session_in_background();
    let target = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .start_in_background();
    let mut zellij = target.attach(TERMINAL_SIZE);
    zellij.wait_until("attached session rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    open_and_close_session_manager(&zellij);
    assert!(!zellij.snapshot().contains(CARD_TITLE));
    zellij.quit();
}

#[test]
fn card_is_not_shown_on_resume() {
    let _other = start_running_session_in_background();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!("{}\nsession_serialization true", SESSION_FEATURES_ON))
        .with_layout(LayoutInfo::Stringified(RESUME_LAYOUT.to_owned()))
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("session loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();
    zellij.resurrect(TERMINAL_SIZE);
    zellij.wait_until("resurrected session rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    open_and_close_session_manager(&zellij);
    assert!(!zellij.snapshot().contains(CARD_TITLE));
    zellij.quit();
}

#[test]
fn card_closes_after_the_first_enter() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    zellij.send_stdin(b"ls");
    zellij.send_stdin(&keys::ENTER);
    terminal.wait_for_stdin("the command reached the terminal", |stdin| {
        stdin.ends_with(b"\r")
    });
    zellij.wait_until("the session card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    zellij.quit();
}

#[test]
fn switch_and_close_removes_the_unused_session() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let unused_session = zellij.session_name().to_owned();
    wait_for_card_listing(&zellij, background.session_name());
    focus_the_card_with_f9(&zellij);
    zellij.send_stdin(&keys::ENTER);
    let socket = zellij_utils::consts::ZELLIJ_SOCK_DIR.join(&unused_session);
    wait_for_path_to_disappear(&socket, "the unused session socket");
    let cache_folder = zellij_utils::consts::session_info_folder_for_session(&unused_session);
    wait_for_path_to_disappear(&cache_folder, "the unused session cache folder");
    let index = zellij_utils::session_index::SessionIndex::open_default().unwrap();
    assert!(index.get(&unused_session).unwrap().is_none());
    zellij.quit();
}

#[test]
fn space_in_tab_mode_changes_the_layout_and_shows_a_notification() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("swap_layout_notification true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::SPACE);
    zellij.wait_until("the layout notification is shown", |grid_snapshot| {
        grid_snapshot.contains("Layout:")
    });
    zellij.quit();
}

#[test]
fn automatic_layout_changes_do_not_show_a_notification() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("swap_layout_notification true")
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let third_terminal = zellij.expect_pty_spawn();
    third_terminal.output(PROMPT);
    zellij.wait_until("third terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(13))
    });
    open_and_close_session_manager(&zellij);
    assert!(!zellij.snapshot().contains("Layout:"));
    zellij.quit();
}

#[test]
fn the_card_lists_running_sessions_with_folder_and_branch_columns() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("the card shows its columns", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE)
            && grid_snapshot.contains(background.session_name())
            && grid_snapshot.contains("Folder")
            && grid_snapshot.contains("Branch")
            && grid_snapshot.contains("ago")
    });
    assert!(grid_snapshot.contains("<F9>"));
    assert!(grid_snapshot.contains(DONT_SHOW_AGAIN));
    assert!(!grid_snapshot.contains(SHOW_ALL_SHORTCUT));
    zellij.quit();
}

#[test]
fn the_bar_counts_running_sessions() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    zellij.wait_until("the bar counts the running session", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .is_some_and(|tab_bar| tab_bar.contains("1 running session"))
    });
    zellij.quit();
}

#[test]
fn f9_focuses_the_startup_card_and_closes_it_the_second_time() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    focus_the_card_with_f9(&zellij);
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the session card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("f9 opened a focused card again", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE) && grid_snapshot.contains(SHOW_ALL_SHORTCUT)
    });
    zellij.quit();
}

#[test]
fn escape_typed_into_the_terminal_closes_the_card() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the session card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    terminal.wait_for_stdin("the escape still reached the terminal", |stdin| {
        stdin.contains(&0x1b)
    });
    zellij.quit();
}

#[test]
fn the_card_closes_when_a_new_pane_opens() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    split_right_and_wait_for_prompt(&zellij);
    zellij.wait_until("the session card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    zellij.quit();
}

#[test]
fn dont_show_again_turns_the_card_off_in_the_config() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("the card offers to stop showing", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE) && grid_snapshot.contains(DONT_SHOW_AGAIN)
    });
    let (column, line) = position_of(&grid_snapshot, DONT_SHOW_AGAIN);
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until("the session card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
    let config_file_path = zellij.config_file_path().expect("the session has a config file");
    let deadline = Instant::now() + default_timeout();
    loop {
        let contents = std::fs::read_to_string(&config_file_path).unwrap_or_default();
        if contents.contains("session_card false") {
            break;
        }
        if Instant::now() >= deadline {
            panic!("session_card false was not written to the config:\n{}", contents);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    zellij.quit();
}

#[test]
fn ctrl_f_in_the_focused_card_opens_the_session_manager_with_an_expandable_list() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card_listing(&zellij, background.session_name());
    focus_the_card_with_f9(&zellij);
    zellij.send_stdin(&keys::ctrl('f'));
    zellij.wait_until("the session manager opened", |grid_snapshot| {
        grid_snapshot.contains("Session: ")
            && grid_snapshot.contains(background.session_name())
            && grid_snapshot.contains("<←↓↑→>")
            && grid_snapshot.contains("Select a session to see its preview")
    });
    zellij.send_stdin(&keys::DOWN);
    zellij.send_stdin(&keys::RIGHT);
    zellij.wait_until("the session expanded into its tabs", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && grid_snapshot.contains("pane")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the session manager closed", |grid_snapshot| {
        !grid_snapshot.contains("Session: ")
    });
    zellij.quit();
}

#[test]
fn enter_in_the_welcome_screen_starts_a_default_session_without_the_card() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .with_layout(LayoutInfo::BuiltIn("welcome".to_owned()))
        .start();
    zellij.wait_until("the welcome screen is shown", |grid_snapshot| {
        grid_snapshot.contains("Enter a session name to create it")
            && grid_snapshot.contains("New session with a random name")
    });
    zellij.send_stdin(&keys::ENTER);
    let deadline = Instant::now() + default_timeout();
    loop {
        let messages = zellij.received_server_messages();
        if messages.iter().any(|message| {
            message == "SwitchSession layout=Some(\"default\") session_card=Some(false)"
        }) {
            break;
        }
        if Instant::now() >= deadline {
            panic!("no switch to a default session without the card: {:?}", messages);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    zellij.quit();
}
