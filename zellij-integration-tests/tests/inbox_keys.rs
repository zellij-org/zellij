#![cfg(unix)]

use std::time::Duration;
use zellij_integration_tests::attention::{counted, needs_you, session_counts, wait_for_counted};
use zellij_integration_tests::inbox::{
    background_session_with, inbox_shows, open_inbox_on, press_esc, press_release_key,
    press_type_key, session_needing_you, ATTENTION_CONFIG, GO_KEY, INBOX_HELP, RELEASE_KEY_HINT,
    SESSIONS_HELP, TYPE_INTO_IT, TYPING_INTO,
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
    zellij.wait_until(what, |grid_snapshot| {
        indicator(grid_snapshot).contains(text)
    })
}

fn is_highlighted(code: Option<zellij_integration_tests::client_screen::AnsiCode>) -> bool {
    !matches!(
        code,
        None | Some(zellij_integration_tests::client_screen::AnsiCode::Reset)
    )
}

fn is_selected(grid_snapshot: &GridSnapshot, row: &str) -> bool {
    grid_snapshot
        .text
        .lines()
        .enumerate()
        .filter_map(|(y, line)| {
            line.find(row)
                .map(|byte_index| (line[..byte_index].chars().count(), y))
        })
        .any(|(x, y)| {
            (x..x + row.chars().count())
                .all(|column| is_highlighted(grid_snapshot.cell_style(column, y).background))
        })
}

fn row_of(symbol: &str, session: &str) -> String {
    format!("{} {}  ", symbol, session)
}

fn click(zellij: &TestSession, grid_snapshot: &GridSnapshot, text: &str) {
    let mut position = grid_snapshot.position_of(text);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let current = zellij.snapshot().position_of(text);
        if current.is_some() && current == position {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{:?} never settled on screen",
            text
        );
        position = current;
    }
    let (column, row) = position.unwrap();
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

fn one_counted_and_one_muted(
    zellij: &TestSession,
) -> (BackgroundTestSession, BackgroundTestSession) {
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
    let older_row = row_of("●", older.session_name());
    let newer_row = row_of("●", newer.session_name());
    open_inbox_on(&zellij, "Newer question?");
    zellij.wait_until("the newest item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, &newer_row) && !is_selected(grid_snapshot, &older_row)
    });
    zellij.send_stdin(&keys::key('c'));
    wait_for_counted(newer.session_name(), 0);
    zellij.wait_until("the other item is selected", |grid_snapshot| {
        indicator(grid_snapshot).contains("● 1") && is_selected(grid_snapshot, &older_row)
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
        is_selected(grid_snapshot, &row_of("●", other.session_name()))
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
        grid_snapshot.contains(SHOWN_LINE)
            && is_selected(grid_snapshot, &row_of("⊘", other.session_name()))
    });
    zellij.send_stdin(&keys::key('m'));
    wait_for_counted(other.session_name(), 1);
    wait_for_indicator(&zellij, "the item counts again", "● 1");
    zellij.quit();
}

#[test]
fn n_watches_the_selected_pane() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = background_session_with(ATTENTION_CONFIG);
    failed_command(&terminal, "cargo test", "1");
    wait_for_indicator(&zellij, "the failed command is counted", "✗ 1");
    open_inbox_on(&zellij, "exited 1");
    zellij.wait_until("the item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, &row_of("✗", other.session_name()))
    });
    zellij.send_stdin(&keys::key('n'));
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
    let (_counted, muted) = one_counted_and_one_muted(&zellij);
    let muted_row = row_of("⊘", muted.session_name());
    open_inbox_on(&zellij, "Counted question?");
    let grid_snapshot = zellij
        .wait_until("the muted item waits behind the line", |grid_snapshot| {
            grid_snapshot.contains("1 more item with only")
        });
    assert!(!grid_snapshot.contains(&muted_row));
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("a shows the muted item", |grid_snapshot| {
        grid_snapshot.contains(SHOWN_LINE) && grid_snapshot.contains(&muted_row)
    });
    zellij.send_stdin(&keys::key('a'));
    zellij.wait_until("a hides it again", |grid_snapshot| {
        grid_snapshot.contains(HIDDEN_LINE) && !grid_snapshot.contains(&muted_row)
    });
    zellij.quit();
}

#[test]
fn clicking_the_line_shows_the_hidden_items() {
    let (mut zellij, _) = main_session();
    let (_counted, muted) = one_counted_and_one_muted(&zellij);
    let muted_row = row_of("⊘", muted.session_name());
    open_inbox_on(&zellij, "Counted question?");
    let grid_snapshot = zellij
        .wait_until("the muted item waits behind the line", |grid_snapshot| {
            grid_snapshot.contains(HIDDEN_LINE)
        });
    click(&zellij, &grid_snapshot, HIDDEN_LINE);
    zellij.wait_until("the click shows the muted item", |grid_snapshot| {
        grid_snapshot.contains(SHOWN_LINE) && grid_snapshot.contains(&muted_row)
    });
    zellij.quit();
}

#[test]
fn f_lists_only_this_session() {
    let (mut zellij, terminal) = main_session();
    terminal.output(&needs_you("Right here?"));
    wait_for_indicator(&zellij, "the item here is counted", "● 1");
    std::thread::sleep(Duration::from_millis(50));
    let (other, _) = session_needing_you("Elsewhere?");
    wait_for_indicator(&zellij, "both items are counted", "● 2");
    let other_row = row_of("●", other.session_name());
    let here_row = row_of("●", "here");
    open_inbox_on(&zellij, "Elsewhere?");
    zellij.wait_until("both items are listed", |grid_snapshot| {
        grid_snapshot.contains(&other_row) && grid_snapshot.contains(&here_row)
    });
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("only this session is listed", |grid_snapshot| {
        grid_snapshot.contains("Only this session")
            && grid_snapshot.contains(&here_row)
            && !grid_snapshot.contains(&other_row)
    });
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("every session is listed again", |grid_snapshot| {
        !grid_snapshot.contains("Only this session")
            && grid_snapshot.contains(&here_row)
            && grid_snapshot.contains(&other_row)
    });
    zellij.quit();
}

#[test]
fn escape_while_typing_reaches_the_pane_and_the_release_key_returns_to_the_list() {
    let (mut zellij, _) = main_session();
    let (_other, terminal) = session_needing_you("Type here?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Type here?");
    press_type_key(&zellij);
    zellij.wait_until("typing started", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
    press_esc(&zellij);
    terminal.wait_for_stdin("escape reached the pane", |stdin| stdin.ends_with(b"\x1b"));
    zellij.send_stdin(b"x");
    terminal.wait_for_stdin("typing goes on after one escape", |stdin| {
        stdin.ends_with(b"\x1bx")
    });
    assert!(zellij.snapshot().contains(TYPING_INTO));
    press_release_key(&zellij);
    zellij.wait_until("the release key returned to the list", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(INBOX_HELP)
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the inbox closed", |grid_snapshot| {
        !grid_snapshot.contains(INBOX_HELP)
    });
    zellij.quit();
}

#[test]
fn clicking_the_live_view_starts_typing_into_the_pane() {
    let (mut zellij, _) = main_session();
    let (_other, terminal) = session_needing_you("Click the view?");
    terminal.output(b"some output\r\n");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Click the view?");
    let grid_snapshot = zellij.wait_until("the live view shows the pane", |grid_snapshot| {
        grid_snapshot.contains("some output")
    });
    let (column, row) = grid_snapshot.position_of("some output").unwrap();
    zellij.send_stdin(&keys::sgr_click(column + 2, row));
    zellij.wait_until("the click started typing", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
    zellij.send_stdin(b"k");
    terminal.wait_for_stdin("the key reached the pane", |stdin| stdin.ends_with(b"k"));
    zellij.quit();
}

#[test]
fn clicking_an_unselected_item_only_selects_it() {
    let (mut zellij, _) = main_session();
    let (older, newer) = two_items(&zellij);
    let older_row = row_of("●", older.session_name());
    let newer_row = row_of("●", newer.session_name());
    open_inbox_on(&zellij, "Newer question?");
    let grid_snapshot = zellij.wait_until("the newest item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, &newer_row)
            && grid_snapshot.contains(&older_row)
            && !is_selected(grid_snapshot, &older_row)
    });
    click(&zellij, &grid_snapshot, &older_row);
    zellij.wait_until("the clicked item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, &older_row) && !is_selected(grid_snapshot, &newer_row)
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
fn clicking_the_go_there_action_switches_to_the_item() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = session_needing_you("Go there?");
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "Go there?");
    let grid_snapshot = zellij.wait_until("the action is offered", |grid_snapshot| {
        grid_snapshot.contains(GO_KEY) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    click(&zellij, &grid_snapshot, GO_KEY);
    let (switch, _previous) = zellij.follow_switch_into(other);
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
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
    let short = Size {
        cols: 120,
        rows: 17,
    };
    let (mut zellij, _) = main_session_sized(short);
    let mut sessions = vec![];
    for index in 0..5 {
        let (session, terminal) = background_session_with(ATTENTION_CONFIG);
        terminal.output(&needs_you(&format!("Question number {}?", index)));
        wait_for_indicator(&zellij, "the item is counted", &format!("● {}", index + 1));
        std::thread::sleep(Duration::from_millis(30));
        sessions.push(session);
    }
    let more_marker = |grid_snapshot: &GridSnapshot, arrow: &str| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains(&format!("{} ", arrow)) && line.contains(" more"))
    };
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the list overflows", |grid_snapshot| {
        more_marker(grid_snapshot, "↓") && grid_snapshot.contains("Question number 4?")
    });
    for _ in 0..3 {
        zellij.send_stdin(&keys::PAGE_DOWN);
    }
    zellij.wait_until(
        "the list scrolled down to the selected oldest item",
        |grid_snapshot| {
            more_marker(grid_snapshot, "↑")
                && !more_marker(grid_snapshot, "↓")
                && grid_snapshot.contains("Question number 0?")
                && !grid_snapshot.contains("Question number 4?")
        },
    );
    zellij.quit();
}

#[test]
fn question_mark_opens_all_keys_from_the_inbox_and_escape_closes_it() {
    let (mut zellij, _) = main_session_sized(TALL);
    let (other, _) = session_needing_you("All keys?");
    let other_row = row_of("●", other.session_name());
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    open_inbox_on(&zellij, "All keys?");
    zellij.wait_until("the item is selected", |grid_snapshot| {
        is_selected(grid_snapshot, &other_row)
    });
    zellij.send_stdin(&keys::key('?'));
    let grid_snapshot = zellij.wait_until("the all keys screen is shown", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<C>")
    });
    for key in [
        "<C>",
        "<f>",
        "<a>",
        "<s>",
        "<ENTER>",
        "<Alt ENTER>",
        "<z>",
        RELEASE_KEY_HINT,
    ] {
        assert!(
            grid_snapshot.contains(key),
            "{} is missing from the list:\n{}",
            key,
            grid_snapshot.text
        );
    }
    assert!(
        !grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains('│') && (line.contains("<g>") || line.contains("<→>"))),
        "removed keys are still listed:\n{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("escape returns to the inbox", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP) && is_selected(grid_snapshot, &other_row)
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
