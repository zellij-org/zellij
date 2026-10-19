#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::{Duration, Instant};
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
    let place = format!("in session {}", background.session_name());
    zellij.wait_until("the inbox opens on the item's card", |grid_snapshot| {
        grid_snapshot.contains("NEEDS YOU")
            && grid_snapshot.contains(&place)
            && grid_snapshot.contains("has a question: Which branch to use?")
    });
    zellij.send_stdin(&keys::DOWN);
    let place = format!("{} ", background.session_name());
    zellij.wait_until("the inbox lists the item", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
            && grid_snapshot.contains(&place)
            && grid_snapshot.contains("has a quest")
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
    zellij
        .main_client()
        .send_action(Action::MarkAllSeen { session: None });
    zellij.wait_until("the other session's item was cleared", |grid_snapshot| {
        let bar = tab_bar(grid_snapshot);
        bar.contains("2 running") && !bar.contains('✗')
    });
    zellij.quit();
}

#[test]
fn reply_mode_writes_into_a_pane_of_another_session_and_ends_when_its_status_changes() {
    let (_background, terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    terminal.output(&needs_you("Allow edit?"));
    wait_for_indicator(&zellij, "the item arrived", "● 1");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox shows the item on its card", |grid_snapshot| {
        grid_snapshot.contains("NEEDS YOU") && grid_snapshot.contains("has a quest")
    });
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("reply mode started", |grid_snapshot| {
        grid_snapshot.contains("REPLYING TO")
    });
    zellij.send_stdin(&keys::key('y'));
    terminal.wait_for_stdin("the reply reached the other session", |stdin| {
        stdin.ends_with(b"y")
    });
    terminal.output(&program_status("state=working"));
    zellij.wait_until("reply mode ended with the status change", |grid_snapshot| {
        !grid_snapshot.contains("REPLYING TO")
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

    zellij.main_client().send_action(Action::GoToPreviousSession);
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
    let context = switch.switch_context.expect("the switch carries the history");
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
    let zellij = background.attach_focusing(TERMINAL_SIZE, Some((pane, false)));
    let grid_snapshot = zellij.wait_until("the indicator shows the items", |grid_snapshot| {
        tab_bar(grid_snapshot).contains("need you")
    });
    std::thread::sleep(Duration::from_millis(500));
    let before = terminal.stdin_bytes().len();
    let bar = tab_bar(&grid_snapshot);
    let column = bar[..bar.rfind('●').unwrap()].chars().count() + 2;
    zellij.send_stdin(&sgr(0, column, 1, 'M'));
    zellij.send_stdin(&sgr(0, column, 1, 'm'));
    zellij.wait_until("the inbox opens", |grid_snapshot| {
        grid_snapshot.contains("Inbox (")
    });
    std::thread::sleep(Duration::from_millis(300));
    for _ in 0..4 {
        zellij.send_stdin(&keys::DOWN);
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(leaked_since(&terminal, before), "", "keys leaked");
    let (rows, cols) = (TERMINAL_SIZE.rows, TERMINAL_SIZE.cols);
    for _ in 0..4 {
        zellij.send_stdin(&sgr(65, cols / 2, rows / 2, 'M'));
        std::thread::sleep(Duration::from_millis(100));
    }
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(leaked_since(&terminal, before), "", "the wheel leaked");
}

const SESSIONS_HELP: &str = "<F1> all keys";
const INBOX_HELP: &str = "<?> all keys";

fn open_dropdown_on_the_sessions(zellij: &TestSession, listed: &str) {
    zellij.send_stdin(&keys::F9);
    let grid_snapshot = zellij.wait_until("the dropdown opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.contains(listed)
    });
    assert!(
        !grid_snapshot.contains(INBOX_HELP),
        "{}",
        grid_snapshot.text
    );
}

#[test]
fn the_dropdown_opens_on_the_sessions_when_nothing_is_counted_and_tab_shows_the_empty_inbox() {
    let (background, _terminal) = background_session(ATTENTION_ON);
    let (mut zellij, _) = start_main_session(ATTENTION_ON);
    wait_for_indicator(&zellij, "the calm indicator", "2 running");
    open_dropdown_on_the_sessions(&zellij, background.session_name());
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("tab shows the empty inbox", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP) && !grid_snapshot.contains(SESSIONS_HELP)
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
        grid_snapshot.contains(INBOX_HELP)
            && grid_snapshot.contains("Newer question?")
            && grid_snapshot.contains("<ENTER> go there")
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
        grid_snapshot.contains(INBOX_HELP)
            && grid_snapshot.contains("Arrived later?")
            && grid_snapshot.contains("<ENTER> go there")
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
    zellij.wait_until("the other session is selected", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains(background.session_name()) && line.contains('>'))
    });
    terminal.output(&needs_you("Arrived later?"));
    zellij.wait_until("the item is counted", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
    });
    std::thread::sleep(Duration::from_millis(500));
    let grid_snapshot = zellij.snapshot();
    assert!(
        grid_snapshot.contains(SESSIONS_HELP) && !grid_snapshot.contains(INBOX_HELP),
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
    zellij.wait_until("the session manager opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.contains(background.session_name())
    });
    zellij.send_stdin(&keys::alt('f'));
    zellij.wait_until("the session manager is hidden", |grid_snapshot| {
        !grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.status_bar_appears()
    });
    terminal.output(&needs_you("While hidden?"));
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::alt('f'));
    zellij.wait_until("the session manager shows the newest item", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP)
            && grid_snapshot.contains("While hidden?")
            && grid_snapshot.contains("<ENTER> go there")
    });
    zellij.send_stdin(&keys::ENTER);
    let _previous = follow(&mut zellij, background);
    zellij.quit();
}
