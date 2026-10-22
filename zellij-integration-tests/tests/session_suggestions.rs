#![cfg(unix)]

use std::time::Instant;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, default_timeout, keys,
    split_right_and_wait_for_prompt, test_env, BackgroundTestSession, GridSnapshot, LayoutInfo,
    TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::input::options::Options;
use zellij_utils::session_index::{SessionIndex, SessionRow};

const SESSION_FEATURES_ON: &str =
    "session_indicator true\nmouse_mode true\nattention {\n    startup_card \"always\"\n}";
const CARD_OFF_INDICATOR_ON: &str = "session_indicator true\nmouse_mode true";
const CARD_TITLE: &str = "Inbox (";
const BROWSER_TABS: &str = "│ Sessions (";
const DONT_SHOW_AGAIN: &str = "Don't show again";
const PLACEHOLDER: &str = "—";
const EMPTY_INBOX_SECTION: &str = "Nothing in the inbox";
const SESSIONS_HELP: &str = "<F1> all keys";
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

fn running_text(count: usize) -> String {
    format!("Nothing needs you · {} running", count)
}

fn wait_for_card(zellij: &TestSession, running: usize) {
    let expected = running_text(running);
    zellij.wait_until("the inbox card is shown", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE)
            && grid_snapshot.contains(&expected)
            && !grid_snapshot.contains(BROWSER_TABS)
    });
}

fn bar_says(grid_snapshot: &GridSnapshot, text: &str) -> bool {
    grid_snapshot
        .lines()
        .first()
        .is_some_and(|tab_bar| tab_bar.contains(text))
}

fn sgr_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn position_of(
    grid_snapshot: &zellij_integration_tests::GridSnapshot,
    needle: &str,
) -> (usize, usize) {
    grid_snapshot
        .lines()
        .iter()
        .enumerate()
        .find_map(|(line_index, line)| {
            line.find(needle)
                .map(|byte_offset| (line[..byte_offset].chars().count() + 1, line_index + 1))
        })
        .expect("the text is on screen")
}

fn open_the_browser_with_f9(zellij: &TestSession) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox browser opened", |grid_snapshot| {
        grid_snapshot.contains(BROWSER_TABS)
    });
}

fn open_the_sessions_tab_with_f9(zellij: &TestSession) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the browser opened on the sessions", |grid_snapshot| {
        grid_snapshot.contains(BROWSER_TABS)
            && grid_snapshot.contains(SESSIONS_HELP)
            && !grid_snapshot.contains(EMPTY_INBOX_SECTION)
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

fn leave_a_resurrectable_session() -> String {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!(
            "{}\nsession_serialization true",
            SESSION_FEATURES_ON
        ))
        .skip_concurrency_slot()
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.save_session();
    zellij.wait_for_serialized_session();
    let session_name = zellij.session_name().to_owned();
    let session_id = zellij.session_id();
    let socket = zellij.socket_path();
    zellij.quit();
    wait_for_path_to_disappear(&socket, "the exited session socket");
    assert!(zellij_utils::consts::session_layout_cache_file_name(&session_id).exists());
    session_name
}

fn wait_for_index_row(session_name: &str, what: &str, ready: impl Fn(&SessionRow) -> bool) {
    let deadline = Instant::now() + default_timeout();
    loop {
        let row = SessionIndex::open_default()
            .ok()
            .and_then(|index| index.get(session_name).ok().flatten());
        if row.as_ref().is_some_and(&ready) {
            return;
        }
        if Instant::now() >= deadline {
            panic!("timed out waiting for {}", what);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

fn line_with(grid_snapshot: &GridSnapshot, needle: &str) -> String {
    grid_snapshot
        .lines()
        .into_iter()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{} is not on screen:\n{}", needle, grid_snapshot.text))
}

fn wait_for_the_card_to_close(zellij: &TestSession) {
    zellij.wait_until("the inbox card closed", |grid_snapshot| {
        !grid_snapshot.contains(CARD_TITLE)
    });
}

fn wait_for_the_browser_to_close(zellij: &TestSession) {
    zellij.wait_until("the inbox browser closed", |grid_snapshot| {
        !grid_snapshot.contains(BROWSER_TABS)
    });
}

fn assert_no_card_after_the_session_settles(zellij: &TestSession) {
    open_and_close_session_manager(zellij);
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains(CARD_TITLE),
        "the session card is shown:\n{}",
        grid_snapshot.text
    );
}

fn create_git_repository(name: &str, branch: &str) -> std::path::PathBuf {
    let folder = test_env::init().join(name);
    std::fs::create_dir_all(&folder).unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(&folder)
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(args)
            .status()
            .expect("git is installed");
        assert!(status.success(), "git {:?} failed", args);
    };
    git(&["init", "-q"]);
    git(&["checkout", "-q", "-b", branch]);
    git(&["commit", "-q", "--allow-empty", "-m", "init"]);
    folder
}

#[test]
fn card_shows_on_a_new_session_when_another_session_is_running() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    zellij.quit();
}

#[test]
fn without_other_running_sessions_there_is_no_card_but_the_indicator_opens_the_browser() {
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("the indicator counts this session", |grid_snapshot| {
        bar_says(grid_snapshot, "1 running")
    });
    assert!(!grid_snapshot.contains(CARD_TITLE));
    let (column, line) = position_of(&grid_snapshot, "1 running");
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until(
        "the indicator opened the browser on the sessions",
        |grid_snapshot| {
            grid_snapshot.contains(BROWSER_TABS)
                && grid_snapshot.contains(SESSIONS_HELP)
                && !grid_snapshot.contains(EMPTY_INBOX_SECTION)
        },
    );
    let grid_snapshot = zellij.snapshot();
    let (column, line) = position_of(&grid_snapshot, "1 running");
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    wait_for_the_browser_to_close(&zellij);
    zellij.quit();
}

#[test]
fn card_is_shown_on_reattach_with_the_time_away() {
    let other = start_running_session_in_background();
    other.expect_pty_spawn().output(PROMPT);
    wait_for_needs_you(other.session_name(), 0);
    let target = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .start_in_background();
    target.expect_pty_spawn().output(PROMPT);
    let mut zellij = target.attach(TERMINAL_SIZE);
    zellij.wait_until("the reattach card is shown", |grid_snapshot| {
        grid_snapshot.contains("While you were away")
    });
    zellij.quit();
}

#[test]
fn card_is_not_shown_on_resume() {
    let _other = start_running_session_in_background();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!(
            "{}\nsession_serialization true",
            SESSION_FEATURES_ON
        ))
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
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    zellij.send_stdin(b"ls");
    zellij.send_stdin(&keys::ENTER);
    terminal.wait_for_stdin("the command reached the terminal", |stdin| {
        stdin.ends_with(b"\r")
    });
    wait_for_the_card_to_close(&zellij);
    zellij.quit();
}

#[test]
fn the_indicator_opens_the_browser_after_the_startup_card_closed_by_itself() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    zellij.send_stdin(b"ls");
    zellij.send_stdin(&keys::ENTER);
    terminal.wait_for_stdin("the command reached the terminal", |stdin| {
        stdin.ends_with(b"\r")
    });
    wait_for_the_card_to_close(&zellij);
    let grid_snapshot = zellij.wait_until("the indicator shows it is closed", |grid_snapshot| {
        bar_says(grid_snapshot, "2 running ▾")
    });
    let (column, line) = position_of(&grid_snapshot, "2 running");
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until("the indicator opened the browser", |grid_snapshot| {
        grid_snapshot.contains(BROWSER_TABS)
    });
    zellij.quit();
}

#[test]
fn switch_and_close_removes_the_unused_session() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let unused_session = zellij.session_name().to_owned();
    let unused_session_id = zellij.session_id();
    let socket = zellij.socket_path();
    wait_for_card(&zellij, 2);
    open_the_browser_with_f9(&zellij);
    zellij.wait_until(
        "the sessions tab lists the other session",
        |grid_snapshot| grid_snapshot.contains(background.session_name()),
    );
    zellij.send_stdin(&keys::DOWN);
    wait_for_preview_of(
        &zellij,
        background.session_name(),
        "the other session is selected",
    );
    zellij.send_stdin(&keys::ENTER);
    let (_switch, _closed) = zellij.follow_switch_into(background);
    wait_for_path_to_disappear(&socket, "the unused session socket");
    let cache_folder = zellij_utils::consts::session_info_folder_for_session(&unused_session_id);
    let index_row_exists = || {
        SessionIndex::open_default()
            .ok()
            .and_then(|index| index.get(&unused_session).ok().flatten())
            .is_some()
    };
    stays_true_once_true(
        "the unused session cache folder and index row are removed",
        || !cache_folder.exists() && !index_row_exists(),
    );
    zellij.quit();
}

fn stays_true_once_true(what: &str, check: impl Fn() -> bool) {
    zellij_integration_tests::wait_for(what, &check);
    let settled_at = Instant::now() + std::time::Duration::from_millis(1500);
    while Instant::now() < settled_at {
        assert!(check(), "{}: it came back", what);
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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
fn the_card_shows_the_running_count_and_its_controls() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    let grid_snapshot = zellij.snapshot();
    assert!(grid_snapshot.contains("<F9>"));
    assert!(grid_snapshot.contains("Open inbox ›"));
    assert!(grid_snapshot.contains(DONT_SHOW_AGAIN));
    zellij.quit();
}

#[test]
fn the_bar_counts_running_sessions_including_this_one() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the bar counts both sessions", |grid_snapshot| {
        bar_says(grid_snapshot, "2 running")
    });
    zellij.quit();
}

#[test]
fn f9_opens_the_browser_from_the_startup_card_and_closes_it_the_second_time() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    open_the_browser_with_f9(&zellij);
    zellij.send_stdin(&keys::F9);
    wait_for_the_browser_to_close(&zellij);
    open_the_browser_with_f9(&zellij);
    zellij.quit();
}

#[test]
fn escape_typed_into_the_terminal_closes_the_card() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    zellij.send_stdin(&keys::ESC);
    wait_for_the_card_to_close(&zellij);
    terminal.wait_for_stdin("the escape still reached the terminal", |stdin| {
        stdin.contains(&0x1b)
    });
    zellij.quit();
}

#[test]
fn the_card_closes_when_a_new_pane_opens() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    split_right_and_wait_for_prompt(&zellij);
    wait_for_the_card_to_close(&zellij);
    zellij.quit();
}

#[test]
fn dont_show_again_turns_the_startup_card_off_in_the_config() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.wait_until("the card offers to stop showing", |grid_snapshot| {
        grid_snapshot.contains(CARD_TITLE)
            && grid_snapshot.contains(DONT_SHOW_AGAIN)
            && grid_snapshot.contains(&running_text(2))
    });
    let (column, line) = position_of(&grid_snapshot, DONT_SHOW_AGAIN);
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    wait_for_the_card_to_close(&zellij);
    let config_file_path = zellij
        .config_file_path()
        .expect("the session has a config file");
    let deadline = Instant::now() + default_timeout();
    loop {
        let contents = std::fs::read_to_string(&config_file_path).unwrap_or_default();
        if contents.contains("startup_card \"never\"") {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "startup_card \"never\" was not written to the config:\n{}",
                contents
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    zellij.quit();
}

#[test]
fn the_sessions_tab_lists_sessions_as_an_expandable_tree() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    open_the_browser_with_f9(&zellij);
    zellij.wait_until("the sessions tab is shown", |grid_snapshot| {
        grid_snapshot.contains("Session: ")
            && grid_snapshot.contains(background.session_name())
            && grid_snapshot.contains("<←↓↑→>")
    });
    zellij.send_stdin(&keys::DOWN);
    zellij.send_stdin(&keys::RIGHT);
    zellij.wait_until("the session expanded into its tabs", |grid_snapshot| {
        grid_snapshot.contains("Tab #1") && grid_snapshot.contains("pane")
    });
    zellij.send_stdin(&keys::ESC);
    wait_for_the_browser_to_close(&zellij);
    zellij.quit();
}

fn start_welcome_screen() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .with_layout(LayoutInfo::BuiltIn("welcome".to_owned()))
        .start()
}

#[test]
fn enter_in_the_welcome_screen_starts_a_default_session_without_the_card() {
    let background = start_running_session_in_background();
    let mut zellij = start_welcome_screen();
    zellij.wait_until("the welcome screen is shown", |grid_snapshot| {
        grid_snapshot.contains("Enter a session name to create it")
            && grid_snapshot.contains("new session with a random name")
            && grid_snapshot.contains(background.session_name())
    });
    zellij.send_stdin(&keys::ENTER);
    let deadline = Instant::now() + default_timeout();
    loop {
        let messages = zellij.received_server_messages();
        if messages.iter().any(|message| {
            message == "SwitchSession layout=Some(\"default\") startup_card=Some(\"never\")"
        }) {
            break;
        }
        if Instant::now() >= deadline {
            panic!(
                "no switch to a default session without the card: {:?}",
                messages
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    zellij.quit();
}

#[test]
fn the_startup_card_counts_only_live_sessions_and_closes_on_the_first_command() {
    let _resurrectable = leave_a_resurrectable_session();
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    zellij.send_stdin(b"ls");
    zellij.send_stdin(&keys::ENTER);
    terminal.wait_for_stdin("the command reached the terminal", |stdin| {
        stdin.ends_with(b"ls\r")
    });
    wait_for_the_card_to_close(&zellij);
    zellij.quit();
}

#[test]
fn without_other_live_sessions_there_is_no_card_even_with_resurrectable_sessions() {
    let resurrectable = leave_a_resurrectable_session();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the bar counts only this session", |grid_snapshot| {
        bar_says(grid_snapshot, "1 running")
    });
    assert_no_card_after_the_session_settles(&zellij);
    open_the_sessions_tab_with_f9(&zellij);
    zellij.wait_until(
        "the sessions tab folds the ended session",
        |grid_snapshot| grid_snapshot.contains("▸ Exited (1)"),
    );
    zellij.send_stdin(b"\x1b[F");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until(
        "the sessions tab lists the ended session",
        |grid_snapshot| grid_snapshot.contains(&resurrectable) && grid_snapshot.contains("ended"),
    );
    zellij.quit();
}

#[test]
fn a_session_created_from_the_welcome_screen_has_no_card_even_with_other_live_sessions() {
    let background = start_running_session_in_background();
    let mut zellij = start_welcome_screen();
    zellij.wait_until(
        "the welcome screen lists the live session",
        |grid_snapshot| {
            grid_snapshot.contains("new session with a random name")
                && grid_snapshot.contains(background.session_name())
        },
    );
    zellij.send_stdin(&keys::ENTER);
    let switch = zellij.follow_switch();
    assert_eq!(
        switch.startup_card(),
        Some(zellij_utils::input::attention::StartupCard::Never)
    );
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the new session sees the live session", |grid_snapshot| {
        bar_says(grid_snapshot, "running")
    });
    assert_no_card_after_the_session_settles(&zellij);
    zellij.quit();
}

#[test]
fn a_named_session_created_from_the_welcome_screen_has_no_card_even_with_other_live_sessions() {
    let background = start_running_session_in_background();
    let mut zellij = start_welcome_screen();
    zellij.wait_until(
        "the welcome screen lists the live session",
        |grid_snapshot| {
            grid_snapshot.contains("new session with a random name")
                && grid_snapshot.contains(background.session_name())
        },
    );
    zellij.send_stdin(b"brand-new");
    zellij.wait_until("the welcome screen offers to create it", |grid_snapshot| {
        grid_snapshot.contains("brand-new") && grid_snapshot.contains("create new")
    });
    zellij.send_stdin(&keys::ENTER);
    let switch = zellij.follow_switch();
    assert_eq!(switch.name.as_deref(), Some("brand-new"));
    assert_eq!(zellij.session_name(), "brand-new");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the new session sees the live session", |grid_snapshot| {
        bar_says(grid_snapshot, "running")
    });
    assert_no_card_after_the_session_settles(&zellij);
    zellij.quit();
}

#[test]
fn with_the_card_turned_off_it_is_not_shown_at_startup_but_f9_still_opens_the_browser() {
    let background = start_running_session_in_background();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    assert_no_card_after_the_session_settles(&zellij);
    open_the_sessions_tab_with_f9(&zellij);
    zellij.wait_until("the browser lists the live session", |grid_snapshot| {
        grid_snapshot.contains(background.session_name())
    });
    zellij.quit();
}

#[test]
fn f9_opens_and_closes_the_browser_in_the_default_keys_preset() {
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_the_browser_with_f9(&zellij);
    zellij.send_stdin(&keys::F9);
    wait_for_the_browser_to_close(&zellij);
    zellij.quit();
}

#[test]
fn f9_opens_and_closes_the_browser_in_locked_mode_of_the_unlock_first_preset() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .with_cli_options(Options {
            keybinds_preset: Some("unlock-first".to_owned()),
            ..Default::default()
        })
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("the session starts locked", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.contains("UNLOCK")
            && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    open_the_browser_with_f9(&zellij);
    zellij.send_stdin(&keys::F9);
    wait_for_the_browser_to_close(&zellij);
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("the session is unlocked", |grid_snapshot| {
        grid_snapshot.contains("PANE")
    });
    zellij.quit();
}

#[test]
fn escape_closes_the_browser_without_reaching_the_terminal() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    open_the_browser_with_f9(&zellij);
    zellij.send_stdin(&keys::ESC);
    wait_for_the_browser_to_close(&zellij);
    zellij.send_stdin(&keys::key('x'));
    let stdin = terminal.wait_for_stdin("the next key reached the terminal", |stdin| {
        stdin.ends_with(b"x")
    });
    assert!(
        !stdin.contains(&0x1b),
        "the escape reached the terminal: {:?}",
        stdin
    );
    zellij.quit();
}

#[test]
fn sessions_without_a_folder_or_a_branch_are_listed() {
    let never_recorded = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let outside_git = start_running_session_in_background();
    let repository = create_git_repository("gitrepo", "feat-x");
    let in_git = TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!(
            "{}\ndefault_cwd \"{}\"",
            SESSION_FEATURES_ON,
            repository.display()
        ))
        .skip_concurrency_slot()
        .start_in_background();
    wait_for_index_row(
        outside_git.session_name(),
        "the folder of the session outside git",
        |row| row.context.folder.is_some(),
    );
    wait_for_index_row(
        in_git.session_name(),
        "the branch of the session in git",
        |row| row.context.branch.as_deref() == Some("feat-x"),
    );
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 4);
    open_the_browser_with_f9(&zellij);
    let grid_snapshot =
        zellij.wait_until("the browser lists all three sessions", |grid_snapshot| {
            grid_snapshot.contains(BROWSER_TABS)
                && grid_snapshot.contains(never_recorded.session_name())
                && grid_snapshot.contains(outside_git.session_name())
                && grid_snapshot.contains(in_git.session_name())
        });
    let never_recorded_line = line_with(&grid_snapshot, never_recorded.session_name());
    assert_eq!(
        never_recorded_line.matches(PLACEHOLDER).count(),
        2,
        "{}",
        never_recorded_line
    );
    let outside_git_line = line_with(&grid_snapshot, outside_git.session_name());
    assert!(outside_git_line.contains("cwd"), "{}", outside_git_line);
    assert_eq!(
        outside_git_line.matches(PLACEHOLDER).count(),
        1,
        "{}",
        outside_git_line
    );
    let in_git_line = line_with(&grid_snapshot, in_git.session_name());
    assert!(in_git_line.contains("gitrepo"), "{}", in_git_line);
    assert!(in_git_line.contains("feat-x"), "{}", in_git_line);
    assert!(!in_git_line.contains(PLACEHOLDER), "{}", in_git_line);
    zellij.quit();
}

#[test]
fn switching_after_a_command_was_typed_keeps_the_current_session() {
    let background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let current_session = zellij.session_name().to_owned();
    wait_for_card(&zellij, 2);
    zellij.send_stdin(b"ls");
    zellij.send_stdin(&keys::ENTER);
    terminal.wait_for_stdin("the command reached the terminal", |stdin| {
        stdin.ends_with(b"ls\r")
    });
    wait_for_the_card_to_close(&zellij);
    open_the_sessions_tab_with_f9(&zellij);
    zellij.wait_until(
        "the sessions tab lists the other session",
        |grid_snapshot| grid_snapshot.contains(background.session_name()),
    );
    zellij.send_stdin(&keys::DOWN);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_for_main_client_to_exit();
    assert!(zellij
        .received_server_messages()
        .iter()
        .any(|message| message.starts_with("SwitchSession")));
    let client = zellij.attach_client(TERMINAL_SIZE);
    client.wait_until("the kept session can be attached to", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.contains(&current_session)
    });
    assert!(zellij.socket_path().exists());
    let index = SessionIndex::open_default().unwrap();
    assert!(index.get(&current_session).unwrap().is_some());
    client.quit();
}

fn blocked(text: &str) -> Vec<u8> {
    use base64::Engine as _;
    format!(
        "\u{1b}]7501;state=blocked:kind=question:msg={}\u{1b}\\",
        base64::engine::general_purpose::STANDARD.encode(text)
    )
    .into_bytes()
}

fn wait_for_needs_you(session: &str, count: usize) {
    let deadline = Instant::now() + default_timeout();
    loop {
        let counted = zellij_utils::session_link::request_session_attention(
            session,
            std::time::Duration::from_secs(2),
        )
        .map(|attention| attention.counts.needs_you)
        .unwrap_or(usize::MAX);
        if counted == count {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{} never counted {} item(s)",
            session,
            count
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn the_reattach_card_closes_itself_once_its_last_item_is_seen() {
    let other = start_running_session_in_background();
    let other_terminal = other.expect_pty_spawn();
    other_terminal.output(PROMPT);
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.detach_main_client();
    other_terminal.output(&blocked("Which branch?"));
    wait_for_needs_you(other.session_name(), 1);
    std::thread::sleep(
        (zellij_server::attention::peers::PEER_PUSH_DEBOUNCE
            + zellij_server::attention::peers::GLOBAL_PUBLISH_DEBOUNCE)
            * 5,
    );
    let reattached = zellij.attach_client(TERMINAL_SIZE);
    reattached.wait_until("the reattach card lists the item", |grid_snapshot| {
        grid_snapshot.contains("While you were away") && grid_snapshot.contains("Which br")
    });
    let name = other.session_name().to_owned();
    zellij_utils::session_link::send_actions_to_session(
        &name,
        vec![zellij_utils::input::actions::Action::MarkAllSeen {
            session: Some(name.clone()),
            this_session: false,
        }],
    )
    .unwrap();
    reattached.wait_until("the empty card closed itself", |grid_snapshot| {
        !grid_snapshot.contains("While you were away")
    });
    reattached.quit();
    zellij.quit();
}

#[test]
fn attaching_to_go_to_a_pane_skips_the_reattach_card() {
    let other = start_running_session_in_background();
    let other_terminal = other.expect_pty_spawn();
    other_terminal.output(PROMPT);
    let target = TestRunner::new(TERMINAL_SIZE)
        .with_config(SESSION_FEATURES_ON)
        .start_in_background();
    let terminal = target.expect_pty_spawn();
    terminal.output(PROMPT);
    other_terminal.output(&blocked("Which branch?"));
    wait_for_needs_you(other.session_name(), 1);
    let mut zellij = target.attach_focusing(TERMINAL_SIZE, Some((terminal.terminal_id(), false)));
    zellij.wait_until("the session counts the item", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && bar_says(grid_snapshot, "1 need you")
    });
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains("While you were away"),
        "the reattach card opened over the pane:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

fn open_sessions_and_select(zellij: &TestSession, name: &str) {
    open_the_sessions_tab_with_f9(zellij);
    zellij.wait_until("the other session is listed", |grid_snapshot| {
        grid_snapshot.contains(name)
    });
    zellij.send_stdin(&keys::DOWN);
    wait_for_preview_of(zellij, name, "the other session is selected");
}

fn preview_frame_of(grid_snapshot: &GridSnapshot, name: &str) -> bool {
    grid_snapshot.contains(&format!("┌ {} ", name))
}

fn wait_for_preview_of(zellij: &TestSession, name: &str, what: &str) {
    zellij.wait_until(what, |grid_snapshot| preview_frame_of(grid_snapshot, name));
}

fn answers(name: &str) -> bool {
    zellij_utils::session_link::session_answers(name, std::time::Duration::from_secs(1))
}

#[test]
fn delete_on_a_running_session_asks_before_stopping_it() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .skip_concurrency_slot()
        .start_in_background();
    let name = background.session_name().to_owned();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_sessions_and_select(&zellij, &name);
    let question = format!("This will stop the session {}", name);
    zellij.send_stdin(&keys::DELETE);
    zellij.wait_until("the stop is confirmed first", |grid_snapshot| {
        grid_snapshot.contains(&question)
    });
    zellij.send_stdin(&keys::key('n'));
    zellij.wait_until("the question is gone", |grid_snapshot| {
        !grid_snapshot.contains(&question) && grid_snapshot.contains(&name)
    });
    assert!(answers(&name), "cancelling stopped the session");
    zellij.send_stdin(&keys::DELETE);
    zellij.wait_until("the stop is confirmed again", |grid_snapshot| {
        grid_snapshot.contains(&question)
    });
    zellij.send_stdin(&keys::key('y'));
    let deadline = Instant::now() + default_timeout();
    while answers(&name) {
        assert!(Instant::now() < deadline, "the session was not stopped");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    zellij.quit();
}

#[test]
fn ctrl_r_renames_the_current_session() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let new_name = format!("{}-rn", zellij.session_name());
    open_the_sessions_tab_with_f9(&zellij);
    zellij.send_stdin(&keys::ctrl('r'));
    zellij.wait_until("the rename prompt is shown", |grid_snapshot| {
        grid_snapshot.contains("<ENTER> rename")
            && grid_snapshot.contains("New name for current session: _")
    });
    zellij.send_stdin(new_name.as_bytes());
    let typed = format!("New name for current session: {}_", new_name);
    zellij.wait_until("the rename prompt shows the typed name", |grid_snapshot| {
        grid_snapshot.contains(&typed)
    });
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the session has its new name", |grid_snapshot| {
        grid_snapshot.lines()[0].contains(&new_name)
    });
    zellij.quit();
}

fn lock_file_of(socket: &std::path::Path) -> std::path::PathBuf {
    let mut lock_name = std::ffi::OsString::from(".");
    lock_name.push(socket.file_name().unwrap());
    lock_name.push(".lock");
    socket.with_file_name(lock_name)
}

#[test]
fn quitting_a_renamed_session_leaves_no_socket_under_either_name() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let old_name = zellij.session_name().to_owned();
    let new_name = format!("{}-renamed", old_name);
    let socket = zellij.socket_path();
    assert!(socket.exists());
    zellij.run_cli_action(zellij_utils::cli::CliAction::RenameSession {
        name: new_name.clone(),
    });
    zellij.wait_until("the session has its new name", |grid_snapshot| {
        grid_snapshot.lines()[0].contains(&new_name)
    });
    zellij_integration_tests::wait_for("the new name to lead to the same socket", || {
        zellij_utils::consts::session_socket_path(&new_name) == socket
    });
    assert!(socket.exists());
    assert!(lock_file_of(&socket).exists());
    assert!(!zellij_utils::consts::ZELLIJ_SOCK_DIR
        .join(&old_name)
        .exists());
    assert!(!zellij_utils::consts::ZELLIJ_SOCK_DIR
        .join(&new_name)
        .exists());
    zellij.quit();
    wait_for_path_to_disappear(&socket, "the renamed session socket");
    wait_for_path_to_disappear(&lock_file_of(&socket), "the renamed session lock");
}

#[test]
fn enter_on_an_exited_session_resumes_it() {
    let resurrectable = leave_a_resurrectable_session();
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_the_sessions_tab_with_f9(&zellij);
    zellij.wait_until("the exited group is listed", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (1)")
    });
    zellij.send_stdin(b"\x1b[F");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the exited group opens", |grid_snapshot| {
        grid_snapshot.contains(&resurrectable)
    });
    zellij.send_stdin(&keys::DOWN);
    wait_for_preview_of(&zellij, &resurrectable, "the exited session is selected");
    zellij.send_stdin(&keys::ENTER);
    let switch = zellij.follow_switch();
    assert_eq!(switch.name.as_deref(), Some(resurrectable.as_str()));
    zellij.quit();
}

fn numbered_output(terminal: &zellij_integration_tests::FakePtyHandle, count: usize) {
    let mut text = String::new();
    for line in 1..=count {
        text.push_str(&format!("row {:03}\r\n", line));
    }
    terminal.output(text.as_bytes());
}

fn visible_rows(grid_snapshot: &GridSnapshot) -> Vec<usize> {
    regex::Regex::new(r"row (\d{3})")
        .unwrap()
        .captures_iter(&grid_snapshot.text)
        .filter_map(|captures| captures[1].parse().ok())
        .collect()
}

fn preview_of_a_busy_session(zellij: &TestSession, name: &str) -> GridSnapshot {
    open_sessions_and_select(zellij, name);
    zellij.wait_until("the preview shows the last line", |grid_snapshot| {
        visible_rows(grid_snapshot).contains(&60)
    })
}

#[test]
fn the_sessions_tab_preview_shows_the_bottom_of_a_running_session() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    numbered_output(&terminal, 60);
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = preview_of_a_busy_session(&zellij, background.session_name());
    assert!(
        !visible_rows(&grid_snapshot).contains(&1),
        "{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn shift_arrows_pan_the_sessions_preview() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    numbered_output(&terminal, 60);
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CARD_OFF_INDICATOR_ON)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let bottom = preview_of_a_busy_session(&zellij, background.session_name());
    let first_visible = *visible_rows(&bottom).iter().min().unwrap();
    let wanted = first_visible.saturating_sub(3);
    for _ in 0..3 {
        zellij.send_stdin(&keys::SHIFT_UP);
    }
    zellij.wait_until("shift up shows earlier lines", |grid_snapshot| {
        visible_rows(grid_snapshot).contains(&wanted)
    });
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until("shift end returns to the bottom", |grid_snapshot| {
        let rows = visible_rows(grid_snapshot);
        rows.contains(&60) && !rows.contains(&wanted)
    });
    zellij.quit();
}

#[test]
fn clicking_a_card_target_goes_there() {
    let _background = start_running_session_in_background();
    let mut zellij = start_session_with_session_features();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    wait_for_card(&zellij, 2);
    let grid_snapshot = zellij.snapshot();
    let (column, line) = position_of(&grid_snapshot, "Open inbox ›");
    zellij.send_stdin(&sgr_left_click(column + 1, line));
    zellij.wait_until("the card opened the browser", |grid_snapshot| {
        grid_snapshot.contains(BROWSER_TABS)
    });
    zellij.quit();
}
