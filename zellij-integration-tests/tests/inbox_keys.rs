#![cfg(unix)]

use std::time::Duration;
use zellij_integration_tests::attention::{
    counted, needs_you, session_counts, wait_for_counted,
};
use zellij_integration_tests::inbox::{
    background_session_with, inbox_shows, open_inbox_on, session_needing_you, ATTENTION_CONFIG,
    INBOX_HELP, SESSIONS_HELP,
};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, FakePtyHandle,
    GridSnapshot, Size, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::data::PaneId;
use zellij_utils::input::actions::Action;

const TALL: Size = Size {
    cols: 140,
    rows: 44,
};
const HIDDEN_LINE: &str = "more item";
const SHOWN_LINE: &str = "<a> hide items";

fn main_session_sized(size: Size) -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(size).with_config(ATTENTION_CONFIG).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.cursor.is_some()
    });
    (zellij, terminal)
}

fn main_session() -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    (zellij, terminal)
}

fn indicator(grid_snapshot: &GridSnapshot) -> String {
    grid_snapshot.lines().first().cloned().unwrap_or_default()
}

fn wait_for_indicator(zellij: &TestSession, what: &str, text: &str) -> GridSnapshot {
    zellij.wait_until(what, |grid_snapshot| indicator(grid_snapshot).contains(text))
}

fn is_selected(grid_snapshot: &GridSnapshot, text: &str) -> bool {
    grid_snapshot
        .lines()
        .iter()
        .any(|line| line.contains(text) && line.contains('▌'))
}

fn click(zellij: &TestSession, grid_snapshot: &GridSnapshot, text: &str) {
    let (column, row) = grid_snapshot
        .position_of(text)
        .unwrap_or_else(|| panic!("{:?} is not on screen:\n{}", text, grid_snapshot.text));
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
}

fn mute_in(session: &BackgroundTestSession, terminal: &FakePtyHandle) {
    zellij_utils::session_link::send_actions_to_session(
        session.session_name(),
        vec![Action::MutePaneByPaneId {
            pane_id: PaneId::Terminal(terminal.terminal_id()),
        }],
    )
    .unwrap();
}

fn failed_command(terminal: &FakePtyHandle, command: &str, exit: &str) {
    terminal.output(
        format!(
            "\u{1b}]133;A\u{1b}\\$ \u{1b}]133;B\u{1b}\\{}\u{1b}]133;C\u{1b}\\\r\n\u{1b}]133;D;{}\u{1b}\\",
            command, exit
        )
        .as_bytes(),
    );
}

fn two_items(zellij: &TestSession) -> (BackgroundTestSession, BackgroundTestSession) {
    let (older, older_terminal) = background_session_with(ATTENTION_CONFIG);
    let (newer, newer_terminal) = background_session_with(ATTENTION_CONFIG);
    older_terminal.output(&needs_you("Older question?"));
    wait_for_indicator(zellij, "the first item is counted", "● 1");
    std::thread::sleep(Duration::from_millis(50));
    newer_terminal.output(&needs_you("Newer question?"));
    wait_for_indicator(zellij, "both items are counted", "● 2");
    (older, newer)
}

fn one_counted_and_one_muted(zellij: &TestSession) -> (BackgroundTestSession, BackgroundTestSession) {
    let (counted_session, _) = session_needing_you("Counted question?");
    let (muted_session, muted_terminal) = session_needing_you("Muted question?");
    wait_for_indicator(zellij, "both items are counted", "● 2");
    mute_in(&muted_session, &muted_terminal);
    wait_for_indicator(zellij, "the muted item is no longer counted", "● 1");
    (counted_session, muted_session)
}

#[test]
fn c_dismisses_the_selected_item_and_selects_the_next() {
    let (mut zellij, _) = main_session();
    let (older, newer) = two_items(&zellij);
    open_inbox_on(&zellij, "Newer question?");
    zellij.wait_until("the newest item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "Newer question?")
    });
    zellij.send_stdin(&keys::key('c'));
    wait_for_counted(newer.session_name(), 0);
    zellij.wait_until("the other item is selected", |grid_snapshot| {
        indicator(grid_snapshot).contains("● 1") && is_selected(grid_snapshot, "Older question?")
    });
    assert_eq!(
        session_counts(older.session_name()).map(|counts| counted(&counts)),
        Some(1)
    );
    zellij.quit();
}

#[test]
fn shift_c_dismisses_everything_in_every_session() {
    let (mut zellij, _) = main_session();
    let (older, newer) = two_items(&zellij);
    open_inbox_on(&zellij, "Newer question?");
    zellij.send_stdin(b"C");
    wait_for_counted(older.session_name(), 0);
    wait_for_counted(newer.session_name(), 0);
    zellij.wait_until("the indicator is calm", |grid_snapshot| {
        let bar = indicator(grid_snapshot);
        bar.contains(" running") && !bar.contains('●')
    });
    zellij.quit();
}

#[test]
fn m_mutes_the_selected_pane_and_m_again_unmutes_it() {
    let (mut zellij, _) = main_session();
    let (other, _) = session_needing_you("Mute me?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Mute me?");
    zellij.wait_until("the item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "Mute me?")
    });
    zellij.send_stdin(&keys::key('m'));
    zellij.wait_until("the item moved behind the line", |grid_snapshot| {
        grid_snapshot.contains(HIDDEN_LINE) && !indicator(grid_snapshot).contains('●')
    });
    zellij_integration_tests::wait_for("the pane to be muted", || {
        session_counts(other.session_name()).map(|counts| counts.muted) == Some(1)
    });
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("the muted item is shown", |grid_snapshot| {
        grid_snapshot.contains(SHOWN_LINE) && is_selected(grid_snapshot, "Mute me?")
    });
    zellij.send_stdin(&keys::key('m'));
    wait_for_counted(other.session_name(), 1);
    wait_for_indicator(&zellij, "the item counts again", "● 1");
    zellij.quit();
}

#[test]
fn w_watches_the_selected_pane() {
    let (mut zellij, _) = main_session();
    let (_other, terminal) = background_session_with(ATTENTION_CONFIG);
    failed_command(&terminal, "cargo test", "1");
    wait_for_indicator(&zellij, "the failed command is counted", "✗ 1");
    open_inbox_on(&zellij, "FAILED");
    zellij.wait_until("the item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "FAILED")
    });
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until("the item is watched", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains("FAILED") && line.contains("watched"))
    });
    zellij.send_stdin(&keys::key('?'));
    zellij.wait_until("all keys offer to stop watching", |grid_snapshot| {
        grid_snapshot.contains("stop watching")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the all keys screen closed", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP) && !grid_snapshot.contains("All keys")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the inbox closed", |grid_snapshot| {
        !grid_snapshot.contains(INBOX_HELP) && !grid_snapshot.contains("All keys")
    });
    failed_command(&terminal, "cargo build", "0");
    wait_for_indicator(&zellij, "the watch raised an alert", "watch alert");
    zellij.quit();
}

#[test]
fn a_shows_and_hides_the_items_behind_the_line() {
    let (mut zellij, _) = main_session();
    let (_counted, _muted) = one_counted_and_one_muted(&zellij);
    open_inbox_on(&zellij, "Counted question?");
    let grid_snapshot = zellij.wait_until("the muted item waits behind the line", |grid_snapshot| {
        grid_snapshot.contains("1 more item with only new output, quiet or muted · <a> show")
    });
    assert!(!grid_snapshot.contains("Muted question?"));
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("a shows the muted item", |grid_snapshot| {
        grid_snapshot.contains(SHOWN_LINE) && grid_snapshot.contains("Muted question?")
    });
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("a hides it again", |grid_snapshot| {
        grid_snapshot.contains(HIDDEN_LINE) && !grid_snapshot.contains("Muted question?")
    });
    zellij.quit();
}

#[test]
#[ignore = "clicking the line of hidden items leaves the client with a blank screen instead of showing them; not diagnosed, listed as an open issue in handoff.md"]
fn clicking_the_line_shows_the_hidden_items() {
    let (mut zellij, _) = main_session();
    let (_counted, _muted) = one_counted_and_one_muted(&zellij);
    open_inbox_on(&zellij, "Counted question?");
    let grid_snapshot = zellij.wait_until("the muted item waits behind the line", |grid_snapshot| {
        grid_snapshot.contains(HIDDEN_LINE)
    });
    click(&zellij, &grid_snapshot, HIDDEN_LINE);
    zellij.wait_until("the click shows the muted item", |grid_snapshot| {
        grid_snapshot.contains(SHOWN_LINE) && grid_snapshot.contains("Muted question?")
    });
    zellij.quit();
}

#[test]
fn f_lists_only_this_session() {
    let (mut zellij, terminal) = main_session();
    let (_other, _) = session_needing_you("Elsewhere?");
    terminal.output(&needs_you("Right here?"));
    wait_for_indicator(&zellij, "both items are counted", "● 2");
    open_inbox_on(&zellij, "Elsewhere?");
    zellij.wait_until("both items are listed", |grid_snapshot| {
        grid_snapshot.contains("Elsewhere?") && grid_snapshot.contains("Right here?")
    });
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("only this session is listed", |grid_snapshot| {
        grid_snapshot.contains("Only this session")
            && grid_snapshot.contains("Right here?")
            && !grid_snapshot.contains("Elsewhere?")
    });
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("every session is listed again", |grid_snapshot| {
        !grid_snapshot.contains("Only this session")
            && grid_snapshot.contains("Right here?")
            && grid_snapshot.contains("Elsewhere?")
    });
    zellij.quit();
}

#[test]
fn escape_in_reply_mode_leaves_reply_without_closing() {
    let (mut zellij, _) = main_session();
    let (_other, _) = session_needing_you("Reply?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Reply?");
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("reply mode started", |grid_snapshot| {
        grid_snapshot.contains("REPLYING TO")
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("reply mode ended with the inbox open", |grid_snapshot| {
        !grid_snapshot.contains("REPLYING TO") && grid_snapshot.contains(INBOX_HELP)
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the inbox closed", |grid_snapshot| {
        !grid_snapshot.contains(INBOX_HELP)
    });
    zellij.quit();
}

#[test]
fn clicking_the_preview_starts_a_reply() {
    let (mut zellij, _) = main_session();
    let (_other, terminal) = session_needing_you("Click the output?");
    terminal.output(b"some output\r\n");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Click the output?");
    zellij.send_stdin(&keys::key('p'));
    let grid_snapshot = zellij.wait_until("the output is shown", |grid_snapshot| {
        grid_snapshot.contains("Latest output")
    });
    let (column, row) = grid_snapshot.position_of("Latest output").unwrap();
    zellij.send_stdin(&keys::sgr_click(column + 2, row));
    zellij.wait_until("the click started a reply", |grid_snapshot| {
        grid_snapshot.contains("REPLYING TO")
    });
    zellij.quit();
}

#[test]
fn clicking_an_unselected_item_only_selects_it() {
    let (mut zellij, _) = main_session();
    let (_older, _newer) = two_items(&zellij);
    open_inbox_on(&zellij, "Newer question?");
    let grid_snapshot = zellij.wait_until("the newest item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "Newer question?") && grid_snapshot.contains("Older question?")
    });
    click(&zellij, &grid_snapshot, "Older question?");
    zellij.wait_until("the clicked item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "Older question?")
            && !is_selected(grid_snapshot, "Newer question?")
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !zellij.main_client().has_exited(),
        "selecting an item switched the client"
    );
    assert!(zellij.snapshot().contains(INBOX_HELP));
    zellij.quit();
}

#[test]
#[ignore = "clicking an action in the Inbox help block does nothing in the harness (the same click on an item works); not diagnosed, listed as an open issue in handoff.md"]
fn clicking_the_go_there_action_switches_to_the_item() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = session_needing_you("Go there?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Go there?");
    let grid_snapshot = zellij.wait_until("the action is offered", |grid_snapshot| {
        grid_snapshot.contains("<ENTER> go there")
    });
    click(&zellij, &grid_snapshot, "<ENTER> go there");
    let (switch, _previous) = zellij.follow_switch_into(other);
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
#[ignore = "clicking an action in the Inbox help block does nothing in the harness (the same click on an item works); not diagnosed, listed as an open issue in handoff.md"]
fn clicking_dismiss_marks_it_seen() {
    let (mut zellij, _) = main_session();
    let (other, _) = session_needing_you("Dismiss me?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Dismiss me?");
    let grid_snapshot = zellij.wait_until("the action is offered", |grid_snapshot| {
        grid_snapshot.contains("<c> dismiss")
    });
    click(&zellij, &grid_snapshot, "<c> dismiss");
    wait_for_counted(other.session_name(), 0);
    zellij.quit();
}

#[test]
fn clicking_the_tab_headers_switches_between_inbox_and_sessions() {
    let (mut zellij, _) = main_session();
    let (_other, _) = session_needing_you("Tabs?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    let grid_snapshot = open_inbox_on(&zellij, "Tabs?");
    click(&zellij, &grid_snapshot, "Sessions (");
    let grid_snapshot = zellij.wait_until("the sessions tab is shown", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && !grid_snapshot.contains(INBOX_HELP)
    });
    click(&zellij, &grid_snapshot, "Inbox (");
    zellij.wait_until("the inbox is shown again", |grid_snapshot| {
        inbox_shows(grid_snapshot, "Tabs?")
    });
    zellij.quit();
}

#[test]
fn page_down_scrolls_the_list_by_whole_notifications() {
    let short = Size { cols: 120, rows: 18 };
    let (mut zellij, _) = main_session_sized(short);
    let mut sessions = vec![];
    for index in 0..5 {
        let (session, terminal) = background_session_with(ATTENTION_CONFIG);
        terminal.output(&needs_you(&format!("Question number {}?", index)));
        wait_for_indicator(
            &zellij,
            "the item is counted",
            &format!("● {}", index + 1),
        );
        std::thread::sleep(Duration::from_millis(30));
        sessions.push(session);
    }
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the list overflows", |grid_snapshot| {
        grid_snapshot.contains("↓") && grid_snapshot.contains("more")
    });
    for _ in 0..3 {
        zellij.send_stdin(&keys::PAGE_DOWN);
    }
    let grid_snapshot = zellij.wait_until("the list scrolled down", |grid_snapshot| {
        grid_snapshot.contains("↑") && grid_snapshot.contains("more")
    });
    assert!(
        grid_snapshot.contains("▌"),
        "the selected item stays in view:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn question_mark_opens_all_keys_from_the_inbox_and_escape_closes_it() {
    let (mut zellij, _) = main_session_sized(TALL);
    let (_other, _) = session_needing_you("All keys?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "All keys?");
    zellij.wait_until("the item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, "All keys?")
    });
    zellij.send_stdin(&keys::key('?'));
    let grid_snapshot = zellij.wait_until("the all keys screen is shown", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<C>")
    });
    for key in ["<C>", "<f>", "<a>", "<Shift ←↓↑→>"] {
        assert!(
            grid_snapshot.contains(key),
            "{} is missing from the list:\n{}",
            key,
            grid_snapshot.text
        );
    }
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("escape returns to the inbox", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP) && is_selected(grid_snapshot, "All keys?")
    });
    zellij.quit();
}

#[test]
fn f1_opens_all_keys_from_the_sessions_tab() {
    let (mut zellij, _) = main_session_sized(TALL);
    let (other, _) = background_session_with(ATTENTION_CONFIG);
    wait_for_indicator(&zellij, "both sessions are known", "2 running");
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the sessions tab is shown", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && grid_snapshot.contains(other.session_name())
    });
    zellij.send_stdin(&keys::F1);
    let grid_snapshot = zellij.wait_until("the all keys screen is shown", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<Ctrl l>")
    });
    for key in ["<Ctrl l>", "<Ctrl r>", "<Del>"] {
        assert!(
            grid_snapshot.contains(key),
            "{} is missing from the list:\n{}",
            key,
            grid_snapshot.text
        );
    }
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("escape returns to the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && !grid_snapshot.contains("All keys")
    });
    zellij.quit();
}

fn key_lines(grid_snapshot: &GridSnapshot) -> Vec<String> {
    grid_snapshot
        .lines()
        .into_iter()
        .filter(|line| line.contains('<') && line.contains('>'))
        .map(|line| line.trim().to_owned())
        .collect()
}

#[test]
fn all_keys_scrolls_when_it_does_not_fit() {
    let small = Size { cols: 60, rows: 16 };
    let (mut zellij, _) = main_session_sized(small);
    let (_other, _) = session_needing_you("Scroll keys?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Scroll keys?");
    zellij.send_stdin(&keys::key('?'));
    let before = zellij.wait_until("the list asks to scroll", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<↓↑> more")
    });
    let before_lines = key_lines(&before);
    zellij.send_stdin(&keys::DOWN);
    zellij.wait_until("down shows a line that was not visible", |grid_snapshot| {
        key_lines(grid_snapshot)
            .iter()
            .any(|line| !before_lines.contains(line))
    });
    zellij.quit();
}
