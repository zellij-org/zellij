#![cfg(unix)]

use std::time::Duration;
use zellij_integration_tests::attention::needs_you;
use zellij_integration_tests::inbox::{
    background_session_with, open_inbox_on, press_release_key, shows_inbox, ATTENTION_CONFIG,
    SESSIONS_HELP, TYPING_INTO,
};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, FakePtyHandle,
    GridSnapshot, LayoutInfo, NestedHarness, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const BROWSER_TABS: &str = "│ Sessions (";
const CLICK_TO_TYPE: &str = zellij_integration_tests::inbox::TYPE_INTO_IT_HINT;
const SNAPSHOT_SUMMARY: &str = "Clients attached";
const END: &[u8] = b"\x1b[F";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Area {
    x: usize,
    y: usize,
    cols: usize,
    rows: usize,
}

impl Area {
    fn contains(&self, column: usize, row: usize) -> bool {
        column >= self.x && column < self.x + self.cols && row >= self.y && row < self.y + self.rows
    }
}

fn main_session() -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    (zellij, terminal)
}

fn source_session(text: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let (session, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(format!("{}\r\n", text).as_bytes());
    (session, terminal)
}

fn source_session_with_layout(
    layout: &str,
    terminals: usize,
) -> (BackgroundTestSession, Vec<FakePtyHandle>) {
    let session = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_CONFIG)
        .with_layout(LayoutInfo::Stringified(layout.to_owned()))
        .skip_concurrency_slot()
        .start_in_background();
    let terminals: Vec<FakePtyHandle> = (0..terminals)
        .map(|_| {
            let terminal = session.expect_pty_spawn();
            terminal.disable_echo();
            terminal
        })
        .collect();
    (session, terminals)
}

fn layout_size_of(terminal: &FakePtyHandle) -> (u16, u16) {
    terminal.wait_for_size("the source pane has its layout size", |cols, rows| {
        cols > 40 && rows > 10
    })
}

fn view_area(grid_snapshot: &GridSnapshot, session: &str) -> Option<Area> {
    let place = format!(" {} › ", session);
    let lines = grid_snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains(&place) && line.contains('┌'))?;
    let top: Vec<char> = lines[row].chars().collect();
    let place_column = lines[row][..lines[row].find(&place)?].chars().count();
    let column = (0..place_column).rev().find(|x| top[*x] == '┌')?;
    let right = (column + 1..top.len()).find(|x| top[*x] == '┐')?;
    let bottom = (row + 1..lines.len()).find(|y| lines[*y].chars().nth(column) == Some('└'))?;
    Some(Area {
        x: column + 1,
        y: row + 1,
        cols: right - column - 1,
        rows: bottom - row - 1,
    })
}

fn shown_inside_the_view(grid_snapshot: &GridSnapshot, session: &str, text: &str) -> bool {
    match (
        view_area(grid_snapshot, session),
        grid_snapshot.position_of(text),
    ) {
        (Some(area), Some((column, row))) => {
            area.contains(column, row) && area.contains(column + text.chars().count() - 1, row)
        },
        _ => false,
    }
}

fn wait_for_the_view_of(zellij: &TestSession, session: &str, text: &str) -> GridSnapshot {
    zellij.wait_until("the sessions tab shows the pane live", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, session, text) && grid_snapshot.contains(CLICK_TO_TYPE)
    })
}

fn view_size_of(grid_snapshot: &GridSnapshot, session: &str) -> (u16, u16) {
    let area = view_area(grid_snapshot, session).expect("the live view is drawn");
    (area.cols as u16, area.rows as u16)
}

fn listed_row(grid_snapshot: &GridSnapshot, name: &str) -> Option<usize> {
    grid_snapshot
        .lines()
        .iter()
        .position(|line| line.contains(name) && !line.contains('┌') && !line.contains('└'))
}

fn shown_session(grid_snapshot: &GridSnapshot) -> Option<String> {
    grid_snapshot.lines().iter().find_map(|line| {
        let start = line.find('┌')? + '┌'.len_utf8();
        let rest = &line[start..];
        let end = rest.find(" › ")?;
        rest[..end]
            .trim()
            .rsplit(' ')
            .next()
            .map(|name| name.to_owned())
    })
}

fn wait_until_listed(zellij: &TestSession, names: &[&str]) {
    zellij.wait_until("the sessions are listed", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP)
            && names
                .iter()
                .all(|name| listed_row(grid_snapshot, name).is_some())
    });
}

fn select_session(zellij: &TestSession, name: &str) {
    for _ in 0..12 {
        let grid_snapshot = zellij.snapshot();
        let shown = shown_session(&grid_snapshot);
        if shown.as_deref() == Some(name) {
            return;
        }
        let shown_row = shown
            .as_deref()
            .and_then(|shown| listed_row(&grid_snapshot, shown));
        let key = match (shown_row, listed_row(&grid_snapshot, name)) {
            (Some(at), Some(wanted)) if wanted < at => keys::UP,
            _ => keys::DOWN,
        };
        zellij.send_stdin(&key);
        zellij.wait_until("another session is shown", |grid_snapshot| {
            let now_shown = shown_session(grid_snapshot);
            now_shown.is_some() && now_shown != shown
        });
    }
    panic!("{} could not be selected", name);
}

fn open_the_sessions(zellij: &TestSession, names: &[&str]) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the browser opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(BROWSER_TABS) && grid_snapshot.contains(SESSIONS_HELP)
    });
    wait_until_listed(zellij, names);
}

fn wait_for_the_settled_view(
    zellij: &TestSession,
    session: &BackgroundTestSession,
    terminal: &FakePtyHandle,
    text: &str,
) -> (u16, u16) {
    let grid_snapshot = wait_for_the_view_of(zellij, session.session_name(), text);
    let view_size = view_size_of(&grid_snapshot, session.session_name());
    terminal.wait_for_size("the shown pane is resized once it settles", |cols, rows| {
        (cols, rows) == view_size
    });
    view_size
}

fn close_the_browser(zellij: &TestSession) {
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("the browser is closed", |grid_snapshot| {
        !grid_snapshot.contains(BROWSER_TABS)
    });
}

fn leave_exited_sessions(names: &[&str]) {
    let index = zellij_utils::session_index::SessionIndex::open_default().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    for name in names {
        let folder = zellij_utils::consts::session_info_folder_for_session(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-layout.kdl"), "layout {\n    pane\n}\n").unwrap();
        index.ensure_row(name, now).unwrap();
    }
}

fn wait_for_pane_rows(zellij: &TestSession) {
    zellij.wait_until("the panes of the tab are listed", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains(" focused") && !line.contains('┌'))
    });
}

#[test]
fn the_sessions_tab_shows_the_focused_pane_of_a_running_session_live() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = source_session("$ first line here");
    open_the_sessions(&zellij, &[other.session_name()]);
    select_session(&zellij, other.session_name());
    wait_for_the_view_of(&zellij, other.session_name(), "first line here");
    terminal.output(b"printed afterwards\r\n");
    wait_for_the_view_of(&zellij, other.session_name(), "printed afterwards");
    zellij.quit();
}

#[test]
fn selecting_a_tab_row_and_a_pane_row_switches_the_view_to_that_pane() {
    let (mut zellij, _) = main_session();
    let (other, terminals) = source_session_with_layout("layout {\n    pane\n    pane\n}\n", 2);
    terminals[0].output(b"$ text of the focused pane\r\n");
    terminals[1].output(b"$ text of the other pane\r\n");
    let name = other.session_name();
    open_the_sessions(&zellij, &[name]);
    select_session(&zellij, name);
    wait_for_the_view_of(&zellij, name, "text of the focused pane");
    zellij.send_stdin(&keys::RIGHT);
    zellij.wait_until("the session shows its tab", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains("Tab #1 (2 panes)"))
    });
    zellij.send_stdin(&keys::RIGHT);
    zellij.send_stdin(&keys::RIGHT);
    wait_for_pane_rows(&zellij);
    wait_for_the_view_of(&zellij, name, "text of the focused pane");
    zellij.send_stdin(&keys::RIGHT);
    zellij.send_stdin(&keys::DOWN);
    let grid_snapshot = wait_for_the_view_of(&zellij, name, "text of the other pane");
    assert!(
        !grid_snapshot.contains("text of the focused pane"),
        "{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::UP);
    wait_for_the_view_of(&zellij, name, "text of the focused pane");
    zellij.quit();
}

#[test]
fn an_exited_session_shows_its_saved_preview() {
    let (mut zellij, _) = main_session();
    leave_exited_sessions(&["gone-one"]);
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the exited group is listed", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (1)")
    });
    zellij.send_stdin(END);
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the exited group opens", |grid_snapshot| {
        grid_snapshot.contains("▾ Exited (1)")
    });
    zellij.send_stdin(&keys::DOWN);
    let grid_snapshot = zellij.wait_until("the saved preview is shown", |grid_snapshot| {
        grid_snapshot.contains("┌ gone-one ") && grid_snapshot.contains("Folder:")
    });
    assert!(
        !grid_snapshot.contains(CLICK_TO_TYPE),
        "an exited session is shown live:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn a_plugin_pane_keeps_the_snapshot_preview() {
    let (mut zellij, _) = main_session();
    let (other, terminals) = source_session_with_layout(
        "layout {\n    pane\n    pane {\n        plugin location=\"zellij:strider\"\n    }\n}\n",
        1,
    );
    terminals[0].output(b"$ the terminal pane\r\n");
    let name = other.session_name();
    open_the_sessions(&zellij, &[name]);
    select_session(&zellij, name);
    wait_for_the_view_of(&zellij, name, "the terminal pane");
    zellij.send_stdin(&keys::RIGHT);
    zellij.wait_until("the session shows its tab", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains("Tab #1 (2 panes)"))
    });
    zellij.send_stdin(&keys::RIGHT);
    zellij.send_stdin(&keys::RIGHT);
    wait_for_pane_rows(&zellij);
    zellij.send_stdin(&keys::RIGHT);
    wait_for_the_view_of(&zellij, name, "the terminal pane");
    zellij.send_stdin(&keys::DOWN);
    let title = format!("┌ {} › Tab #1 › ", name);
    let grid_snapshot = zellij.wait_until("the plugin pane has a snapshot", |grid_snapshot| {
        grid_snapshot.contains(&title)
            && grid_snapshot.contains(SNAPSHOT_SUMMARY)
            && !grid_snapshot.contains(CLICK_TO_TYPE)
    });
    assert!(
        !shown_inside_the_view(&grid_snapshot, name, "the terminal pane"),
        "{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::UP);
    wait_for_the_view_of(&zellij, name, "the terminal pane");
    zellij.quit();
}

#[test]
fn clicking_a_session_and_clicking_it_again_attaches_instead_of_typing() {
    let (mut zellij, _) = main_session();
    let (first, _) = source_session("$ first source");
    let (second, _) = source_session("$ second source");
    let (first_name, second_name) = (
        first.session_name().to_owned(),
        second.session_name().to_owned(),
    );
    open_the_sessions(&zellij, &[&first_name, &second_name]);
    select_session(&zellij, &first_name);
    wait_for_the_view_of(&zellij, &first_name, "first source");
    let grid_snapshot = zellij.snapshot();
    let row = listed_row(&grid_snapshot, &second_name).expect("the second session is listed");
    let line = grid_snapshot.lines()[row].clone();
    let column = line[..line.find(second_name.as_str()).unwrap()]
        .chars()
        .count();
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    wait_for_the_view_of(&zellij, &second_name, "second source");
    assert!(
        !zellij.main_client().has_exited(),
        "the first click attached"
    );
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    let (_switch, _previous) = zellij.follow_switch_into(second);
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn a_click_on_the_view_types_into_the_pane_and_f9_stops_typing() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = source_session("$ type in here");
    let name = other.session_name();
    open_the_sessions(&zellij, &[name]);
    select_session(&zellij, name);
    let grid_snapshot = wait_for_the_view_of(&zellij, name, "type in here");
    let area = view_area(&grid_snapshot, name).unwrap();
    zellij.send_stdin(&keys::sgr_click(area.x + 2, area.y + 1));
    zellij.wait_until("the click starts typing", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
    zellij.send_stdin(b"hello");
    terminal.wait_for_stdin("the typed text reached the pane", |stdin| {
        stdin.ends_with(b"hello")
    });
    press_release_key(&zellij);
    zellij.wait_until("F9 stops typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(CLICK_TO_TYPE)
    });
    zellij.send_stdin(b"x");
    zellij.wait_until("keys go to the search field again", |grid_snapshot| {
        !grid_snapshot.contains(CLICK_TO_TYPE)
    });
    assert!(
        terminal.stdin_bytes().ends_with(b"hello"),
        "F9 reached the pane: {:?}",
        String::from_utf8_lossy(&terminal.stdin_bytes())
    );
    zellij.quit();
}

#[test]
fn browsing_sessions_resizes_each_pane_once_it_settles_going_back_does_not_and_closing_restores_all(
) {
    let (mut zellij, _) = main_session();
    let sources: Vec<(BackgroundTestSession, FakePtyHandle, String)> = (0..3)
        .map(|index| {
            let text = format!("$ pane of source {}", index);
            let (session, terminal) = source_session(&text);
            (session, terminal, text[2..].to_owned())
        })
        .collect();
    let layouts: Vec<(u16, u16)> = sources
        .iter()
        .map(|(_, terminal, _)| layout_size_of(terminal))
        .collect();
    let names: Vec<&str> = sources
        .iter()
        .map(|(session, _, _)| session.session_name())
        .collect();
    open_the_sessions(&zellij, &names);
    let mut view_size = None;
    for (session, terminal, text) in &sources {
        select_session(&zellij, session.session_name());
        view_size = Some(wait_for_the_settled_view(&zellij, session, terminal, text));
    }
    let view_size = view_size.unwrap();
    for (_, terminal, _) in &sources {
        assert_eq!(terminal.size(), Some(view_size));
    }
    let first = 0;
    let first_sizes = sources[first].1.size_history();
    select_session(&zellij, sources[first].0.session_name());
    wait_for_the_view_of(&zellij, sources[first].0.session_name(), &sources[first].2);
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        sources[first].1.size_history(),
        first_sizes,
        "going back to a session the preview resized resized it again"
    );
    close_the_browser(&zellij);
    for ((_, terminal, _), layout_size) in sources.iter().zip(layouts) {
        terminal.wait_for_size(
            "closing the browser gives every browsed pane its layout size back",
            |cols, rows| (cols, rows) == layout_size,
        );
    }
    zellij.quit();
}

#[test]
fn switching_between_the_inbox_and_the_sessions_keeps_the_browsed_panes_until_the_browser_closes() {
    let (mut zellij, _) = main_session();
    let (item, item_terminal) = source_session("$ the inbox item pane");
    item_terminal.output(&needs_you("Switch tabs?"));
    let (plain, plain_terminal) = source_session("$ the plain session pane");
    let item_layout = layout_size_of(&item_terminal);
    let plain_layout = layout_size_of(&plain_terminal);
    zellij.wait_until("the item is counted", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .map(|line| line.contains("● 1"))
            .unwrap_or(false)
    });
    let grid_snapshot = open_inbox_on(&zellij, "Switch tabs?");
    assert!(shows_inbox(&grid_snapshot));
    let inbox_view = zellij.wait_until("the inbox shows the item live", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, item.session_name(), "the inbox item pane")
    });
    let inbox_view_size = view_size_of(&inbox_view, item.session_name());
    item_terminal.wait_for_size("the item pane is resized", |cols, rows| {
        (cols, rows) == inbox_view_size
    });
    zellij.send_stdin(&keys::TAB);
    wait_until_listed(&zellij, &[item.session_name(), plain.session_name()]);
    select_session(&zellij, plain.session_name());
    let sessions_view_size =
        wait_for_the_settled_view(&zellij, &plain, &plain_terminal, "the plain session pane");
    zellij.send_stdin(&keys::TAB);
    zellij.wait_until("the inbox shows the item again", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, item.session_name(), "the inbox item pane")
    });
    item_terminal.wait_for_size("the item pane is resized again", |cols, rows| {
        (cols, rows) == inbox_view_size
    });
    assert_eq!(plain_terminal.size(), Some(sessions_view_size));
    let plain_sizes = plain_terminal.size_history();
    zellij.send_stdin(&keys::TAB);
    wait_for_the_view_of(&zellij, plain.session_name(), "the plain session pane");
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        plain_terminal.size_history(),
        plain_sizes,
        "going back to the sessions tab resized the shown pane again"
    );
    assert_eq!(item_terminal.size(), Some(inbox_view_size));
    close_the_browser(&zellij);
    item_terminal.wait_for_size("the item pane gets its size back", |cols, rows| {
        (cols, rows) == item_layout
    });
    plain_terminal.wait_for_size("the session pane gets its size back", |cols, rows| {
        (cols, rows) == plain_layout
    });
    zellij.quit();
}

#[test]
fn a_nested_session_does_not_show_or_resize_the_pane_of_its_parent_that_runs_it() {
    let nested = NestedHarness::start_with_host_and_guest_config(
        TERMINAL_SIZE,
        ATTENTION_CONFIG,
        ATTENTION_CONFIG,
    );
    let guest_terminal = nested.guest.expect_pty_spawn();
    guest_terminal.output(PROMPT);
    nested.wait_for_host_to_acknowledge_guest();
    let host_name = nested.host.session_name().to_owned();
    let host_pane_sizes = nested.host_pane.size_history();
    open_the_sessions(&nested.guest, &[&host_name]);
    select_session(&nested.guest, &host_name);
    nested.guest.wait_until(
        "the parent pane is refused instead of shown inside itself",
        |grid_snapshot| {
            grid_snapshot.contains("This pane runs this session")
                && view_area(grid_snapshot, &host_name).is_some()
        },
    );
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        nested.host_pane.size_history(),
        host_pane_sizes,
        "the pane that runs the nested session was resized"
    );
}

#[test]
fn alt_enter_types_into_the_shown_pane_with_or_without_kitty_encoding() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = source_session("$ alt enter here");
    let name = other.session_name();
    open_the_sessions(&zellij, &[name]);
    select_session(&zellij, name);
    wait_for_the_view_of(&zellij, name, "alt enter here");
    for (alt_enter, typed) in [
        (keys::ALT_ENTER.to_vec(), "legacy"),
        (keys::ALT_ENTER_KITTY.to_vec(), "kitty"),
    ] {
        zellij.send_stdin(&alt_enter);
        zellij.wait_until("alt enter starts typing", |grid_snapshot| {
            grid_snapshot.contains(TYPING_INTO)
        });
        zellij.send_stdin(typed.as_bytes());
        terminal.wait_for_stdin("the typed text reached the pane", |stdin| {
            stdin.ends_with(typed.as_bytes())
        });
        press_release_key(&zellij);
        zellij.wait_until("F9 stops typing", |grid_snapshot| {
            !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(CLICK_TO_TYPE)
        });
    }
    assert!(
        terminal.stdin_bytes().ends_with(b"legacykitty"),
        "{:?}",
        String::from_utf8_lossy(&terminal.stdin_bytes())
    );
    zellij.quit();
}

#[test]
fn a_selection_without_a_live_pane_keeps_the_browsed_pane_at_its_size_until_the_browser_closes() {
    let (mut zellij, _) = main_session();
    let (other, terminals) = source_session_with_layout(
        "layout {\n    pane\n    pane {\n        plugin location=\"zellij:strider\"\n    }\n}\n",
        1,
    );
    terminals[0].output(b"$ the browsed pane\r\n");
    let layout_size = layout_size_of(&terminals[0]);
    let name = other.session_name();
    open_the_sessions(&zellij, &[name]);
    select_session(&zellij, name);
    let view_size = wait_for_the_settled_view(&zellij, &other, &terminals[0], "the browsed pane");
    let settled_sizes = terminals[0].size_history();
    zellij.send_stdin(&keys::RIGHT);
    zellij.wait_until("the session shows its tab", |grid_snapshot| {
        grid_snapshot
            .lines()
            .iter()
            .any(|line| line.contains("Tab #1 (2 panes)"))
    });
    zellij.send_stdin(&keys::RIGHT);
    zellij.send_stdin(&keys::RIGHT);
    wait_for_pane_rows(&zellij);
    zellij.send_stdin(&keys::RIGHT);
    wait_for_the_view_of(&zellij, name, "the browsed pane");
    zellij.send_stdin(&keys::DOWN);
    zellij.wait_until("the plugin pane has a snapshot", |grid_snapshot| {
        grid_snapshot.contains(SNAPSHOT_SUMMARY) && !grid_snapshot.contains(CLICK_TO_TYPE)
    });
    terminals[0].output(b"$ barrier after the plugin row\r\n");
    zellij.send_stdin(&keys::UP);
    wait_for_the_view_of(&zellij, name, "barrier after the plugin row");
    assert_eq!(terminals[0].size(), Some(view_size));
    assert_eq!(
        terminals[0].size_history(),
        settled_sizes,
        "selecting a pane without a live view gave the browsed pane its size back"
    );
    close_the_browser(&zellij);
    terminals[0].wait_for_size(
        "closing the browser gives the browsed pane its layout size back",
        |cols, rows| (cols, rows) == layout_size,
    );
    zellij.quit();
}
