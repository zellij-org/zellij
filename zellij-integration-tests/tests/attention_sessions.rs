#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::{Duration, Instant};
use zellij_integration_tests::attention::{counted, session_counts, wait_for_counted};
use zellij_integration_tests::inbox::{inbox_shows, press_release_key, shows_inbox, ITEM_ACTIONS};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, FakePtyHandle,
    GridSnapshot, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::data::PaneId;
use zellij_utils::input::actions::Action;
use zellij_utils::session_history::SessionArrival;

const ATTENTION_ON: &str = "session_indicator true\nmouse_mode true";
const CARD_RULE_ON: &str =
    "session_indicator true\nmouse_mode true\nattention {\n    startup_card \"attention\"\n}";
const PROPAGATION_LIMIT: Duration = Duration::from_secs(3);

fn program_status(body: &str) -> Vec<u8> {
    format!("\u{1b}]7501;{}\u{1b}\\", body).into_bytes()
}

fn needs_you(text: &str) -> Vec<u8> {
    program_status(&format!(
        "state=blocked:kind=question:msg={}",
        STANDARD.encode(text)
    ))
}

fn failed_command(terminal: &FakePtyHandle, command: &str) {
    terminal.output(
        format!(
            "\u{1b}]133;A\u{1b}\\$ \u{1b}]133;B\u{1b}\\{}\u{1b}]133;C\u{1b}\\\r\n\u{1b}]133;D;1\u{1b}\\",
            command
        )
        .as_bytes(),
    );
}

fn tab_bar(grid_snapshot: &GridSnapshot) -> String {
    grid_snapshot.lines().first().cloned().unwrap_or_default()
}

fn background_session(config: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(config)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    (background, terminal)
}

fn start_main_session(config: &str) -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE).with_config(config).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    (zellij, terminal)
}

fn switch_to(zellij: &TestSession, name: &str) {
    zellij.main_client().send_action(Action::SwitchSession {
        name: name.to_owned(),
        tab_position: None,
        pane_id: None,
        layout: None,
        cwd: None,
        close_current: false,
    });
}

fn follow(
    zellij: &mut TestSession,
    target: BackgroundTestSession,
) -> (zellij_utils::data::ConnectToSession, BackgroundTestSession) {
    let followed = zellij.follow_switch_into(target);
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    followed
}

fn wait_for_indicator(zellij: &TestSession, what: &str, text: &str) {
    zellij.wait_until(what, |grid_snapshot| tab_bar(grid_snapshot).contains(text));
}

#[test]
fn attention_from_another_session_reaches_the_indicator_and_the_inbox() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(
        &zellij,
        "the calm indicator counts both sessions",
        "2 running",
    );
    let started = Instant::now();
    terminal.output(&needs_you("Which branch to use?"));
    wait_for_indicator(
        &zellij,
        "the indicator shows the other session needs you",
        "● 1",
    );
    assert!(
        started.elapsed() < PROPAGATION_LIMIT,
        "attention took {:?} to arrive",
        started.elapsed()
    );
    zellij.send_stdin(&keys::F9);
    let place = format!("{} ", background.session_name());
    zellij.wait_until("the inbox opens on the item's row", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
            && grid_snapshot.contains(&place)
            && grid_snapshot.contains("Which branch")
    });
    zellij.send_stdin(&keys::DOWN);
    let place = format!("{} ", background.session_name());
    zellij.wait_until("the inbox lists the item", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
            && grid_snapshot.contains(&place)
            && grid_snapshot.contains("Which branch")
    });
    zellij.quit();
}

#[test]
fn marking_everything_seen_clears_items_in_other_sessions() {
    let (_background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    failed_command(&terminal, "cargo test");
    wait_for_indicator(&zellij, "the failed command is counted", "✗ 1");
    zellij.main_client().send_action(Action::MarkAllSeen {
        session: None,
        this_session: false,
    });
    zellij.wait_until("the other session's item was cleared", |grid_snapshot| {
        let bar = tab_bar(grid_snapshot);
        bar.contains("2 running") && !bar.contains('✗')
    });
    zellij.quit();
}

#[test]
fn typing_into_the_live_view_writes_into_a_pane_of_another_session_and_keeps_its_target() {
    let (_background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    terminal.output(&needs_you("Allow edit?"));
    wait_for_indicator(&zellij, "the item arrived", "● 1");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox shows the item's row", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains("Allow edit?")
    });
    zellij.send_stdin(&keys::ALT_ENTER);
    zellij.wait_until("typing started", |grid_snapshot| {
        grid_snapshot.contains("typing into")
    });
    zellij.send_stdin(&keys::key('y'));
    terminal.wait_for_stdin("the key reached the other session", |stdin| {
        stdin.ends_with(b"y")
    });
    terminal.output(&program_status("state=working"));
    zellij.wait_until("the item changed its status", |grid_snapshot| {
        grid_snapshot.contains("working") && !grid_snapshot.contains("Allow edit?")
    });
    zellij.send_stdin(&keys::key('z'));
    terminal.wait_for_stdin("typing kept its target after the change", |stdin| {
        stdin.ends_with(b"yz")
    });
    press_release_key(&zellij);
    zellij.wait_until("the release key ended typing", |grid_snapshot| {
        !grid_snapshot.contains("typing into")
    });
    zellij.quit();
}

#[test]
fn toggle_back_and_forward_follow_the_client_history() {
    let (background, _) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    let first = zellij.session_name().to_owned();
    let second = background.session_name().to_owned();
    wait_for_indicator(&zellij, "both sessions are known", "2 running");

    switch_to(&zellij, &second);
    let (switch, first_session) = follow(&mut zellij, background);
    let context = switch.switch_context.expect("a switch carries the history");
    assert_eq!(context.from_session.as_deref(), Some(first.as_str()));
    assert_eq!(context.history.back, vec![first.clone()]);
    assert_eq!(context.arrival, Some(SessionArrival::Switch));
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode offers the way back", |grid_snapshot| {
        let status_bar = grid_snapshot.lines().last().cloned().unwrap_or_default();
        status_bar.contains("Back") && status_bar.contains("History")
    });
    zellij.send_stdin(&keys::ESC);

    zellij.main_client().send_action(Action::ToggleSession);
    let (switch, second_session) = follow(&mut zellij, first_session);
    let context = switch.switch_context.expect("a toggle carries the history");
    assert_eq!(switch.name.as_deref(), Some(first.as_str()));
    assert_eq!(context.arrival, Some(SessionArrival::Toggle));
    assert_eq!(context.history.previous(), Some(second.as_str()));

    zellij
        .main_client()
        .send_action(Action::GoToPreviousSession);
    let (switch, first_session) = follow(&mut zellij, second_session);
    let context = switch
        .switch_context
        .expect("going back carries the history");
    assert_eq!(switch.name.as_deref(), Some(second.as_str()));
    assert_eq!(context.arrival, Some(SessionArrival::Back));
    assert_eq!(context.history.forward, vec![first.clone()]);

    zellij.main_client().send_action(Action::GoToNextSession);
    let (switch, _second_session) = follow(&mut zellij, first_session);
    let context = switch
        .switch_context
        .expect("going forward carries the history");
    assert_eq!(switch.name.as_deref(), Some(first.as_str()));
    assert_eq!(context.arrival, Some(SessionArrival::Forward));
    zellij.quit();
}

#[test]
fn toggle_without_history_goes_to_the_next_running_session() {
    let (background, _) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    let first = zellij.session_name().to_owned();
    let second = background.session_name().to_owned();
    wait_for_indicator(&zellij, "both sessions are known", "2 running");
    zellij.main_client().send_action(Action::ToggleSession);
    let (switch, _first_session) = follow(&mut zellij, background);
    assert_eq!(switch.name.as_deref(), Some(second.as_str()));
    let context = switch
        .switch_context
        .expect("the switch carries the history");
    assert_eq!(context.arrival, Some(SessionArrival::Toggle));
    assert_eq!(context.history.previous(), Some(first.as_str()));
    zellij.quit();
}

#[test]
fn go_to_next_attention_switches_to_the_session_that_needs_you() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    terminal.output(&needs_you("Which branch?"));
    wait_for_indicator(&zellij, "the item arrived", "● 1");
    zellij.main_client().send_action(Action::GoToNextAttention);
    let (switch, _first) = follow(&mut zellij, background);
    let pane_id = PaneId::Terminal(terminal.terminal_id());
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    assert_eq!(
        switch.switch_context.and_then(|c| c.arrival),
        Some(SessionArrival::Attention { pane_id })
    );
    zellij.quit();
}

#[test]
fn the_startup_card_opens_when_another_session_needs_you() {
    let (background, terminal) = background_session(ATTENTION_ON);
    terminal.output(&needs_you("Pick one"));
    let (mut zellij, _) = start_main_session(CARD_RULE_ON);
    let place = format!("{} ", background.session_name());
    zellij.wait_until("the card lists what needs you", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains(&place)
    });
    zellij.quit();
}

#[test]
fn the_startup_card_stays_closed_when_nothing_needs_you() {
    let (_background, _terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(CARD_RULE_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("the session manager opened", |grid_snapshot| {
        grid_snapshot.contains("Session: ")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the session manager closed", |grid_snapshot| {
        !grid_snapshot.contains("Session: ")
    });
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains("Inbox ("),
        "the card opened although nothing needs you:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn tabs_show_the_status_symbol_instead_of_an_exclamation_mark() {
    let (mut zellij, first_terminal) = start_main_session(ATTENTION_ON);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(b"$ ");
    zellij.wait_until("the second tab is open", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("Tab #2")
    });
    first_terminal.output(&needs_you("Approve?"));
    let grid_snapshot = zellij.wait_until("the first tab shows needs you", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("Tab #1 ●")
    });
    assert!(!tab_bar(&grid_snapshot).contains("[!]"));
    zellij.quit();
}

fn sgr(button: u8, column: usize, line: usize, end: char) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}{}", button, column, line, end).into_bytes()
}

fn leaked_since(terminal: &FakePtyHandle, before: usize) -> String {
    String::from_utf8_lossy(&terminal.stdin_bytes()[before..]).into_owned()
}

#[test]
fn keys_and_wheel_in_the_inbox_opened_from_the_indicator_stay_out_of_the_pane() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_ON)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    for i in 0..3 {
        terminal.output(&needs_you(&format!("question {}", i)));
    }
    let pane = terminal.terminal_id();
    let mut zellij = background.attach_focusing(TERMINAL_SIZE, Some((pane, false)));
    let grid_snapshot = zellij.wait_until("the indicator shows the item", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("need you")
    });
    let grid_snapshot = settled(&zellij, grid_snapshot);
    let before = terminal.stdin_bytes().len();
    let bar = tab_bar(&grid_snapshot);
    let column = bar[..bar.rfind('●').unwrap()].chars().count() + 2;
    zellij.send_stdin(&sgr(0, column, 1, 'M'));
    zellij.send_stdin(&sgr(0, column, 1, 'm'));
    zellij.wait_until("the inbox opens focused", |grid_snapshot| {
        grid_snapshot.contains("Inbox (")
            && grid_snapshot.contains(ITEM_ACTIONS)
            && grid_snapshot.cursor.is_none()
    });
    let (rows, cols) = (TERMINAL_SIZE.rows, TERMINAL_SIZE.cols);
    for _ in 0..4 {
        zellij.send_stdin(&keys::DOWN);
        zellij.send_stdin(&sgr(65, cols / 2, rows / 2, 'M'));
    }
    zellij.send_stdin(&keys::key('?'));
    zellij.wait_until(
        "the keys before the barrier were handled",
        |grid_snapshot| grid_snapshot.contains("All keys"),
    );
    assert_eq!(
        leaked_since(&terminal, before),
        "",
        "keys or the wheel leaked"
    );
    zellij.quit();
}

fn settled(zellij: &TestSession, mut grid_snapshot: GridSnapshot) -> GridSnapshot {
    let deadline = Instant::now() + zellij_integration_tests::default_timeout();
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let current = zellij.snapshot();
        if current.text == grid_snapshot.text || Instant::now() >= deadline {
            return current;
        }
        grid_snapshot = current;
    }
}

const SESSIONS_HELP: &str = "<F1> all keys";

fn open_dropdown_on_the_sessions(zellij: &TestSession, listed: &str) {
    zellij.send_stdin(&keys::F9);
    let grid_snapshot = zellij.wait_until("the dropdown opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.contains(listed)
    });
    assert!(!shows_inbox(&grid_snapshot), "{}", grid_snapshot.text);
}

#[test]
fn the_dropdown_opens_on_the_sessions_when_nothing_is_counted_and_tab_shows_the_empty_inbox() {
    let (background, _terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    open_dropdown_on_the_sessions(&zellij, background.session_name());
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("tab shows the empty inbox", |grid_snapshot| {
        shows_inbox(grid_snapshot) && !grid_snapshot.contains(SESSIONS_HELP)
    });
    zellij.quit();
}

#[test]
fn the_dropdown_opens_on_the_newest_counted_item() {
    let (older, older_terminal) = background_session(ATTENTION_ON);
    let (newer, newer_terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    older_terminal.output(&needs_you("Older question?"));
    wait_for_indicator(&zellij, "the first item is counted", "● 1");
    std::thread::sleep(Duration::from_millis(50));
    newer_terminal.output(&needs_you("Newer question?"));
    wait_for_indicator(&zellij, "both items are counted", "● 2");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the dropdown opens on the inbox", |grid_snapshot| {
        shows_inbox(grid_snapshot)
            && grid_snapshot.contains("Newer question?")
            && grid_snapshot.contains("<ENTER> go")
    });
    zellij.send_stdin(&keys::ENTER);
    let _previous = follow(&mut zellij, newer);
    drop(older);
    zellij.quit();
}

#[test]
fn the_dropdown_opened_empty_turns_to_the_inbox_when_an_item_arrives_before_any_input() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    open_dropdown_on_the_sessions(&zellij, background.session_name());
    terminal.output(&needs_you("Arrived later?"));
    zellij.wait_until("the dropdown turns to the inbox", |grid_snapshot| {
        shows_inbox(grid_snapshot)
            && grid_snapshot.contains("Arrived later?")
            && grid_snapshot.contains(ITEM_ACTIONS)
    });
    zellij.quit();
}

#[test]
fn after_any_input_the_dropdown_stays_where_the_user_left_it() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    open_dropdown_on_the_sessions(&zellij, background.session_name());
    zellij.send_stdin(&keys::DOWN);
    let preview_frame = format!("┌ {} ", background.session_name());
    zellij.wait_until("the other session is selected", |grid_snapshot| {
        grid_snapshot.contains(&preview_frame)
    });
    terminal.output(&needs_you("Arrived later?"));
    zellij.wait_until("the item is counted", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
    });
    std::thread::sleep(Duration::from_millis(500));
    let grid_snapshot = zellij.snapshot();
    assert!(
        grid_snapshot.contains(SESSIONS_HELP) && !shows_inbox(&grid_snapshot),
        "the dropdown left the view the user was using:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn showing_a_hidden_session_manager_again_selects_the_newest_item() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until(
        "the session manager opens on the sessions",
        |grid_snapshot| {
            grid_snapshot.contains(SESSIONS_HELP)
                && grid_snapshot.contains(background.session_name())
        },
    );
    zellij.send_stdin(&keys::alt('f'));
    zellij.wait_until("the session manager is hidden", |grid_snapshot| {
        !grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.status_bar_appears()
    });
    terminal.output(&needs_you("While hidden?"));
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::alt('f'));
    zellij.wait_until(
        "the session manager shows the newest item",
        |grid_snapshot| {
            shows_inbox(grid_snapshot)
                && grid_snapshot.contains("While hidden?")
                && grid_snapshot.contains("<ENTER> go")
        },
    );
    zellij.send_stdin(&keys::ENTER);
    let _previous = follow(&mut zellij, background);
    zellij.quit();
}

fn status_bar(grid_snapshot: &GridSnapshot) -> String {
    grid_snapshot
        .lines()
        .iter()
        .rev()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

fn press_a_key_in(zellij: &TestSession, terminal: &FakePtyHandle) {
    let before = terminal.stdin_bytes().len();
    zellij.send_stdin(&keys::key('x'));
    terminal.wait_for_stdin("the key reached the pane", |stdin| stdin.len() > before);
}

#[test]
fn mark_all_seen_for_this_session_clears_only_the_session_it_is_sent_to() {
    let (mut zellij, terminal) = start_main_session(ATTENTION_ON);
    let (other, other_terminal) = background_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "both sessions are known", "2 running");
    other_terminal.output(&needs_you("Over there?"));
    terminal.output(&needs_you("Over here?"));
    wait_for_indicator(&zellij, "both items are counted", "● 2");
    zellij.run_cli_action(zellij_utils::cli::CliAction::MarkAllSeen { this_session: true });
    wait_for_counted(zellij.session_name(), 0);
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        session_counts(other.session_name()).map(|counts| counted(&counts)),
        Some(1),
        "the other session was cleared too"
    );
    zellij.quit();
}

#[test]
fn the_session_switching_commands_work_from_the_cli() {
    let (background, background_terminal) = background_session(ATTENTION_ON);
    let (mut zellij, terminal) = start_main_session(ATTENTION_ON);
    let first = zellij.session_name().to_owned();
    let second = background.session_name().to_owned();
    wait_for_indicator(&zellij, "both sessions are known", "2 running");
    press_a_key_in(&zellij, &terminal);
    zellij.run_cli_action(zellij_utils::cli::CliAction::ToggleSession);
    let (switch, first_session) = follow(&mut zellij, background);
    assert_eq!(switch.name.as_deref(), Some(second.as_str()));
    press_a_key_in(&zellij, &background_terminal);
    zellij.run_cli_action(zellij_utils::cli::CliAction::GoToPreviousSession);
    let (switch, second_session) = follow(&mut zellij, first_session);
    assert_eq!(switch.name.as_deref(), Some(first.as_str()));
    press_a_key_in(&zellij, &terminal);
    zellij.run_cli_action(zellij_utils::cli::CliAction::GoToNextSession);
    let (switch, first_session) = follow(&mut zellij, second_session);
    assert_eq!(switch.name.as_deref(), Some(second.as_str()));
    terminal.output(&needs_you("Back here?"));
    wait_for_indicator(&zellij, "the item in the first session is counted", "● 1");
    press_a_key_in(&zellij, &background_terminal);
    zellij.run_cli_action(zellij_utils::cli::CliAction::GoToNextAttention);
    let (switch, _second_session) = follow(&mut zellij, first_session);
    assert_eq!(switch.name.as_deref(), Some(first.as_str()));
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    zellij.quit();
}

fn numbered_lines(terminal: &FakePtyHandle, prefix: &str, count: usize) {
    let mut text = String::new();
    for line in 1..=count {
        text.push_str(&format!("{} {:03}\r\n", prefix, line));
    }
    terminal.output(text.as_bytes());
}

fn visible_numbers(grid_snapshot: &GridSnapshot, prefix: &str) -> Vec<usize> {
    let pattern = regex::Regex::new(&format!(r"{} (\d{{3}})", prefix)).unwrap();
    pattern
        .captures_iter(&grid_snapshot.text)
        .filter_map(|captures| captures[1].parse().ok())
        .collect()
}

#[test]
fn the_inbox_live_view_shows_the_items_pane_and_follows_its_output() {
    let (_background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    numbered_lines(&terminal, "line", 3);
    terminal.output(&needs_you("Watch me?"));
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the live view shows the pane", |grid_snapshot| {
        inbox_shows(grid_snapshot, "Watch me?")
            && visible_numbers(grid_snapshot, "line").contains(&3)
    });
    numbered_lines(&terminal, "more", 2);
    zellij.wait_until("the live view follows new output", |grid_snapshot| {
        visible_numbers(grid_snapshot, "more").contains(&2)
    });
    zellij.quit();
}

const WIDE: zellij_integration_tests::Size = zellij_integration_tests::Size {
    cols: 220,
    rows: 24,
};

fn start_wide_main_session() -> TestSession {
    let zellij = TestRunner::new(WIDE).with_config(ATTENTION_ON).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });
    zellij
}

#[test]
fn switching_sessions_shows_switched_from_for_a_few_seconds() {
    let (background, _) = background_session(ATTENTION_ON);
    let mut zellij = start_wide_main_session();
    let first = zellij.session_name().to_owned();
    wait_for_indicator(&zellij, "both sessions are known", "2 running");
    zellij.main_client().send_action(Action::ToggleSession);
    let _previous = follow(&mut zellij, background);
    let expected = format!("Switched from {}", first);
    zellij.wait_until(
        "the status bar says where the client came from",
        |grid_snapshot| status_bar(grid_snapshot).contains(&expected),
    );
    let shown_at = Instant::now();
    zellij.wait_until("the hint goes away", |grid_snapshot| {
        !status_bar(grid_snapshot).contains(&expected)
    });
    assert!(
        shown_at.elapsed() > Duration::from_secs(2),
        "the hint went away after {:?}",
        shown_at.elapsed()
    );
    zellij.quit();
}

#[test]
fn going_to_an_item_shows_done_and_the_way_back_once_it_clears() {
    let (background, terminal) = background_session(ATTENTION_ON);
    let mut zellij = start_wide_main_session();
    let first = zellij.session_name().to_owned();
    terminal.output(&needs_you("Go there?"));
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox selects the item", |grid_snapshot| {
        grid_snapshot.contains("Go there?") && grid_snapshot.contains("<ENTER> go")
    });
    zellij.send_stdin(&keys::ENTER);
    let _previous = follow(&mut zellij, background);
    terminal.output(&program_status("state=clear"));
    let expected = format!("back to {}", first);
    zellij.wait_until("the status bar offers the way back", |grid_snapshot| {
        let bar = status_bar(grid_snapshot);
        bar.contains("Done ·") && bar.contains(&expected)
    });
    zellij.quit();
}

#[test]
fn session_mode_shows_the_next_item_hint() {
    let (_background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    terminal.output(&needs_you("Next?"));
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::ctrl('o'));
    zellij.wait_until("session mode offers the next item", |grid_snapshot| {
        let bar = status_bar(grid_snapshot);
        bar.contains("next item") || bar.contains("Next")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.quit();
}

#[test]
fn the_compact_bar_shows_tab_status_symbols() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_ON)
        .with_layout(zellij_utils::data::LayoutInfo::BuiltIn(
            "compact".to_owned(),
        ))
        .start();
    let first_terminal = zellij.expect_pty_spawn();
    first_terminal.output(b"$ ");
    zellij.wait_until("the compact bar is drawn", |grid_snapshot| {
        grid_snapshot.contains("Tab #1")
    });
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(b"$ ");
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the second tab is open", |grid_snapshot| {
        grid_snapshot.contains("Tab #2")
    });
    first_terminal.output(&needs_you("Compact?"));
    zellij.wait_until("the first tab shows needs you", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains("Tab #1 ●"))
    });
    zellij.quit();
}

#[test]
fn a_muted_tab_shows_a_dim_mute_mark() {
    let (mut zellij, _first_terminal) = start_main_session(ATTENTION_ON);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(b"$ ");
    zellij.wait_until("the second tab is open", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("Tab #2")
    });
    zellij.run_cli_action(zellij_utils::cli::CliAction::MuteTab { tab_id: None });
    let grid_snapshot = zellij.wait_until("the tab shows the mute mark", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("Tab #2 ⊘")
    });
    assert_eq!(
        grid_snapshot.char_dim_of("⊘"),
        Some(true),
        "the mute mark is dim: {}",
        tab_bar(&grid_snapshot)
    );
    zellij.quit();
}

#[test]
fn the_indicator_shortens_on_a_narrow_terminal() {
    let narrow = zellij_integration_tests::Size { cols: 46, rows: 24 };
    let (background, terminal) = background_session(ATTENTION_ON);
    terminal.output(&needs_you("Narrow?"));
    wait_for_counted(background.session_name(), 1);
    let mut zellij = TestRunner::new(narrow).with_config(ATTENTION_ON).start();
    let _ = zellij.expect_pty_spawn();
    let grid_snapshot = zellij.wait_until("the indicator counts the item", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("● 1")
    });
    assert!(
        !tab_bar(&grid_snapshot).contains("need you"),
        "the indicator was not shortened: {}",
        tab_bar(&grid_snapshot)
    );
    zellij.quit();
    let mut simplified = TestRunner::new(narrow)
        .with_config(&format!("{}\nsimplified_ui true", ATTENTION_ON))
        .start();
    let _ = simplified.expect_pty_spawn();
    simplified.wait_until("the simplified indicator uses letters", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("! 1")
    });
    simplified.quit();
}
