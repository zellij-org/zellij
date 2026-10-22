#![cfg(unix)]

use std::time::Duration;
use zellij_integration_tests::attention::needs_you;
use zellij_integration_tests::inbox::{
    background_session_with, inbox_shows, open_inbox, open_inbox_on, press_esc, press_release_key,
    shows_inbox, ATTENTION_CONFIG, SESSIONS_HELP, TYPE_INTO_IT, TYPING_INTO,
};
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, BackgroundTestSession, FakePtyHandle,
    GridSnapshot, LayoutInfo, Size, TestClient, TestRunner, TestSession, TERMINAL_SIZE,
};
use zellij_utils::attention::{AttentionStatus, PEER_SCAN_INTERVAL_ENV_VAR};
use zellij_utils::consts::APP_PANEL_SESSION;
use zellij_utils::input::actions::Action;

const CARD_CONFIG: &str =
    "session_indicator true\nmouse_mode true\nattention {\n    startup_card \"attention\"\n}";

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
    main_session_with(TERMINAL_SIZE, ATTENTION_CONFIG)
}

fn main_session_with(size: Size, config: &str) -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(size).with_config(config).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    (zellij, terminal)
}

fn item_session(shown: &str, question: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let (background, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(format!("{}\r\n", shown).as_bytes());
    terminal.output(&needs_you(question));
    (background, terminal)
}

fn two_pane_item_session(shown: &str, question: &str) -> (BackgroundTestSession, FakePtyHandle) {
    let background = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_CONFIG)
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    pane\n    pane\n}\n".to_owned(),
        ))
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    let other_terminal = background.expect_pty_spawn();
    terminal.disable_echo();
    other_terminal.disable_echo();
    terminal.output(format!("$ {}\r\n", shown).as_bytes());
    terminal.output(&needs_you(question));
    (background, terminal)
}

fn wait_for_count(zellij: &TestSession, count: usize) {
    let text = format!("● {}", count);
    zellij.wait_until("the items are counted", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .map(|line| line.contains(&text))
            .unwrap_or(false)
    });
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

fn wait_for_the_view_of(
    zellij: &TestSession,
    session: &BackgroundTestSession,
    text: &str,
) -> GridSnapshot {
    zellij.wait_until("the live view shows the item's pane", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, session.session_name(), text)
    })
}

fn layout_size_of(terminal: &FakePtyHandle) -> (u16, u16) {
    terminal.wait_for_size("the source pane has its layout size", |cols, rows| {
        cols > 40 && rows > 10
    })
}

fn start_typing(zellij: &TestSession) {
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("typing into the view started", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
}

fn type_into(zellij: &TestSession, terminal: &FakePtyHandle, text: &str) {
    zellij.send_stdin(text.as_bytes());
    terminal.wait_for_stdin("the typed text reached the source", |stdin| {
        stdin.ends_with(text.as_bytes())
    });
}

fn pinned_marker((cols, rows): (u16, u16)) -> String {
    format!("mirrored {}x{}", cols, rows)
}

#[test]
fn opening_the_browser_on_an_item_of_another_session_shows_its_pane_live() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ first line here", "Live view?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Live view?");
    let grid_snapshot = wait_for_the_view_of(&zellij, &other, "first line here");
    assert!(
        grid_snapshot.contains(TYPE_INTO_IT),
        "{}",
        grid_snapshot.text
    );
    terminal.output(b"printed afterwards\r\n");
    wait_for_the_view_of(&zellij, &other, "printed afterwards");
    zellij.quit();
}

fn row_of(symbol: &str, session: &str) -> String {
    format!("{} {}  ", symbol, session)
}

fn shows_watched_row(grid_snapshot: &GridSnapshot, session: &str) -> bool {
    grid_snapshot.contains(&row_of("◉", session))
}

#[test]
fn watching_a_pane_lists_it_at_once_as_a_one_row_item_until_the_watch_ends() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(b"watched pane output\r\n");
    let watcher = other.attach_client(TERMINAL_SIZE);
    watcher.wait_until(
        "the client in the other session is attached",
        |grid_snapshot| grid_snapshot.status_bar_appears(),
    );
    open_inbox(&zellij);
    watcher.send_action(Action::WatchPane);
    let grid_snapshot = zellij.wait_until("the watched pane is listed", |grid_snapshot| {
        shows_watched_row(grid_snapshot, other.session_name())
    });
    assert!(
        !grid_snapshot.contains("<a> show"),
        "the watched pane waits behind the quiet line:\n{}",
        grid_snapshot.text
    );
    let heading_row = grid_snapshot
        .row_of_line("Watching")
        .expect("the watching heading is drawn");
    let item_row = grid_snapshot
        .row_of_line(&row_of("◉", other.session_name()))
        .expect("the row is drawn");
    assert_eq!(
        item_row,
        heading_row + 1,
        "the watched pane is one row under its heading:\n{}",
        grid_snapshot.text
    );
    wait_for_the_view_of(&zellij, &other, "watched pane output");
    watcher.send_action(Action::WatchPane);
    zellij.wait_until("the row goes once the watch ends", |grid_snapshot| {
        !shows_watched_row(grid_snapshot, other.session_name()) && shows_inbox(grid_snapshot)
    });
    zellij.quit();
}

#[test]
fn moving_the_selection_switches_the_view_to_the_new_items_pane() {
    let (mut zellij, _) = main_session();
    let (older, _) = item_session("$ pane of the older item", "Older?");
    wait_for_count(&zellij, 1);
    let (newer, _) = item_session("$ pane of the newer item", "Newer?");
    wait_for_count(&zellij, 2);
    open_inbox_on(&zellij, "Newer?");
    let grid_snapshot = wait_for_the_view_of(&zellij, &newer, "pane of the newer item");
    assert!(!grid_snapshot.contains("pane of the older item"));
    zellij.send_stdin(&keys::DOWN);
    let grid_snapshot = wait_for_the_view_of(&zellij, &older, "pane of the older item");
    assert!(
        !grid_snapshot.contains("pane of the newer item"),
        "{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn enter_types_into_the_pane_and_the_release_key_or_a_click_outside_ends_typing() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = two_pane_item_session("type here", "Type?");
    let source_view = other.attach_client(TERMINAL_SIZE);
    source_view.wait_until("the source view is up", |grid_snapshot| {
        grid_snapshot.contains("type here")
    });
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Type?");
    let grid_snapshot = wait_for_the_view_of(&zellij, &other, "type here");
    let area = view_area(&grid_snapshot, other.session_name()).unwrap();
    let view_size = (area.cols as u16, area.rows as u16);
    terminal.wait_for_size("the shown source is resized to the view", |cols, rows| {
        (cols, rows) == view_size
    });
    source_view.wait_until("the source shows the pin marker", |grid_snapshot| {
        grid_snapshot.contains(&pinned_marker(view_size))
    });
    start_typing(&zellij);
    type_into(&zellij, &terminal, "hello");
    press_release_key(&zellij);
    zellij.wait_until("the release key returns to the list", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    start_typing(&zellij);
    let grid_snapshot = zellij.snapshot();
    let (column, row) = grid_snapshot
        .position_of("Inbox (")
        .expect("the tabs are shown");
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    zellij.wait_until("a click outside returns to the list", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && shows_inbox(grid_snapshot)
    });
    assert!(terminal.stdin_bytes().ends_with(b"hello\x1b"));
    assert_eq!(
        terminal.size(),
        Some(view_size),
        "the shown source keeps the size of the view after typing"
    );
    source_view.detach();
    zellij.quit();
}

#[test]
fn g_goes_to_the_pane_in_its_real_session() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ go there", "Go?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Go?");
    wait_for_the_view_of(&zellij, &other, "go there");
    zellij.send_stdin(&keys::key('g'));
    let (switch, _previous) = zellij.follow_switch_into(other);
    assert_eq!(switch.pane_id, Some((terminal.terminal_id(), false)));
    zellij.wait_until("the switched client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

const Z_HINT: &str = "resize the pane to the view";

fn z_hint_says(zellij: &TestSession, other: &BackgroundTestSession, text: &str, state: &str) {
    let expected = format!("{} ({})", Z_HINT, state);
    zellij.send_stdin(&keys::key('?'));
    zellij.wait_until("all keys show the z hint", |grid_snapshot| {
        grid_snapshot.contains("All keys")
            && grid_snapshot
                .lines()
                .iter()
                .any(|line| line.contains("<z>") && line.contains(&expected))
    });
    zellij.send_stdin(&keys::ESC);
    wait_for_the_view_of(zellij, other, text);
}

fn assert_not_pinned_after_a_barrier(
    source_client: &TestClient,
    terminal: &FakePtyHandle,
    layout_size: (u16, u16),
    barrier: &str,
) {
    terminal.output(format!("\r\n$ {}\r\n", barrier).as_bytes());
    let grid_snapshot = source_client
        .wait_until("the source view shows the barrier", |grid_snapshot| {
            grid_snapshot.contains(barrier)
        });
    assert!(
        !grid_snapshot.contains("mirrored "),
        "the source pane is pinned:\n{}",
        grid_snapshot.text
    );
    assert_eq!(terminal.size(), Some(layout_size));
}

#[test]
fn z_turns_resizing_off_gives_the_source_its_size_back_and_says_the_view_is_cropped() {
    let (mut zellij, _) = main_session();
    let (other, terminal, source_client, layout_size) =
        source_with_a_client_on_the_other_pane("not resized", "Resize?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Resize?");
    z_hint_says(&zellij, &other, "not resized", "on");
    let view_size = shown_view_size(&zellij, &other, "not resized");
    terminal.wait_for_size("the shown source is resized to the view", |cols, rows| {
        (cols, rows) == view_size
    });
    source_client.wait_until("the source shows the pin marker", |grid_snapshot| {
        grid_snapshot.contains(&pinned_marker(view_size))
    });
    zellij.send_stdin(&keys::key('z'));
    zellij.wait_until("resizing is off", |grid_snapshot| {
        grid_snapshot.contains("resize off")
    });
    terminal.wait_for_size("turning resizing off gives the size back", |cols, rows| {
        (cols, rows) == layout_size
    });
    assert_not_pinned_after_a_barrier(&source_client, &terminal, layout_size, "after z");
    z_hint_says(&zellij, &other, "not resized", "off");
    start_typing(&zellij);
    type_into(&zellij, &terminal, "cropped");
    let cropped = format!("cropped from {}x{}", layout_size.0, layout_size.1);
    zellij.wait_until("the status says the view is cropped", |grid_snapshot| {
        grid_snapshot.contains(&cropped)
    });
    assert_not_pinned_after_a_barrier(&source_client, &terminal, layout_size, "while typing");
    press_release_key(&zellij);
    zellij.wait_until("typing ended", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO)
    });
    zellij.send_stdin(&keys::key('z'));
    zellij.wait_until("resizing is on again", |grid_snapshot| {
        !grid_snapshot.contains("resize off")
    });
    terminal.wait_for_size("the source is resized to the view again", |cols, rows| {
        (cols, rows) == view_size
    });
    z_hint_says(&zellij, &other, "not resized", "on");
    source_client.detach();
    zellij.quit();
}

fn open_the_browser_for(client: &TestClient, text: &str) {
    client.send_stdin(&keys::F9);
    client.wait_until("the client's browser shows the item", |grid_snapshot| {
        inbox_shows(grid_snapshot, text)
    });
}

#[test]
fn two_clients_each_see_their_own_item_live_and_type_only_into_it() {
    let (mut zellij, _) = main_session();
    let (older, older_terminal) = item_session("$ pane of the older item", "Older?");
    wait_for_count(&zellij, 1);
    let (newer, newer_terminal) = item_session("$ pane of the newer item", "Newer?");
    wait_for_count(&zellij, 2);
    let second = zellij.attach_client(TERMINAL_SIZE);
    second.wait_until("the second client is attached", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    second.send_stdin(&keys::ctrl('t'));
    second.send_stdin(&keys::key('n'));
    let second_tab_terminal = zellij.expect_pty_spawn();
    second_tab_terminal.output(b"$ ");
    second.wait_until("the second client is on its own tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.lines()[0].contains("Tab #2")
    });
    open_inbox_on(&zellij, "Newer?");
    wait_for_the_view_of(&zellij, &newer, "pane of the newer item");
    open_the_browser_for(&second, "Older?");
    second.send_stdin(&keys::DOWN);
    let second_grid =
        second.wait_until("the second client shows the older item", |grid_snapshot| {
            shown_inside_the_view(
                grid_snapshot,
                older.session_name(),
                "pane of the older item",
            )
        });
    assert!(!second_grid.contains("pane of the newer item"));
    let first_grid = wait_for_the_view_of(&zellij, &newer, "pane of the newer item");
    assert!(!first_grid.contains("pane of the older item"));
    start_typing(&zellij);
    type_into(&zellij, &newer_terminal, "first client");
    second.send_stdin(&keys::ENTER);
    second.wait_until("the second client types", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
    second.send_stdin(b"second client");
    older_terminal.wait_for_stdin("the second client's keys reached its item", |stdin| {
        stdin.ends_with(b"second client")
    });
    assert!(!String::from_utf8_lossy(&newer_terminal.stdin_bytes()).contains("second"));
    assert!(!String::from_utf8_lossy(&older_terminal.stdin_bytes()).contains("first"));
    press_release_key(&second);
    press_release_key(&zellij);
    zellij.wait_until("the first client stopped typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO)
    });
    second.detach();
    zellij.quit();
}

fn wait_until_cleared(
    zellij: &TestSession,
    terminal: &FakePtyHandle,
    kept_size: (u16, u16),
    what: &str,
    text: &str,
) {
    zellij.wait_until(what, |grid_snapshot| {
        !grid_snapshot.contains(text) && !grid_snapshot.contains(TYPING_INTO)
    });
    assert_eq!(
        terminal.size(),
        Some(kept_size),
        "the source keeps the size of the view once the view is gone"
    );
}

fn give_the_size_back(
    source_client: &TestClient,
    terminal: &FakePtyHandle,
    layout_size: (u16, u16),
) {
    source_client.send_action(Action::FocusNextPane);
    terminal.wait_for_size(
        "focusing the pane in its own session gives its size back",
        |cols, rows| (cols, rows) == layout_size,
    );
    source_client.send_action(Action::FocusNextPane);
}

fn wait_for_the_settled_view(
    zellij: &TestSession,
    other: &BackgroundTestSession,
    terminal: &FakePtyHandle,
    text: &str,
    view_size: (u16, u16),
) {
    wait_for_the_view_of(zellij, other, text);
    terminal.wait_for_size("the shown pane is resized once it settles", |cols, rows| {
        (cols, rows) == view_size
    });
}

#[test]
fn closing_the_browser_sessions_all_keys_and_hiding_each_clear_the_view() {
    let (mut zellij, _) = main_session();
    let text = "SOURCE TEXT SEVEN";
    let (other, terminal, source_client, layout_size) =
        source_with_a_client_on_the_other_pane(text, "Clear?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Clear?");
    let view_size = shown_view_size(&zellij, &other, text);
    wait_for_the_settled_view(&zellij, &other, &terminal, text, view_size);
    start_typing(&zellij);
    zellij.send_stdin(&keys::F9);
    wait_until_cleared(
        &zellij,
        &terminal,
        view_size,
        "closing the browser while typing clears the view",
        text,
    );
    zellij.wait_until("the browser is closed", |grid_snapshot| {
        !shows_inbox(grid_snapshot)
    });
    give_the_size_back(&source_client, &terminal, layout_size);
    zellij.send_stdin(&keys::F9);
    wait_for_the_settled_view(&zellij, &other, &terminal, text, view_size);
    start_typing(&zellij);
    let grid_snapshot = zellij.snapshot();
    let (column, row) = grid_snapshot
        .position_of("Sessions (")
        .expect("the tabs are shown");
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    zellij.wait_until("the sessions view is shown", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP)
    });
    wait_until_cleared(
        &zellij,
        &terminal,
        view_size,
        "the sessions view clears the view while typing",
        text,
    );
    give_the_size_back(&source_client, &terminal, layout_size);
    zellij.send_stdin(&keys::TAB);
    wait_for_the_settled_view(&zellij, &other, &terminal, text, view_size);
    zellij.send_stdin(&keys::key('?'));
    zellij.wait_until("all keys are listed", |grid_snapshot| {
        grid_snapshot.contains("All keys")
    });
    wait_until_cleared(
        &zellij,
        &terminal,
        view_size,
        "all keys clears the view",
        text,
    );
    give_the_size_back(&source_client, &terminal, layout_size);
    zellij.send_stdin(&keys::ESC);
    wait_for_the_settled_view(&zellij, &other, &terminal, text, view_size);
    start_typing(&zellij);
    zellij.send_stdin(&keys::alt('f'));
    wait_until_cleared(
        &zellij,
        &terminal,
        view_size,
        "hiding the floating panes while typing clears the view",
        text,
    );
    zellij.send_stdin(&keys::alt('f'));
    wait_for_the_view_of(&zellij, &other, text);
    source_client.detach();
    zellij.quit();
}

#[test]
fn a_closing_source_pane_shows_the_closed_message_and_the_selection_moves_on_once_it_leaves() {
    let (mut zellij, _) = main_session();
    let (staying, _) = item_session("$ the item that stays", "Stays?");
    wait_for_count(&zellij, 1);
    let closing = TestRunner::new(TERMINAL_SIZE)
        .with_config(ATTENTION_CONFIG)
        .with_layout(LayoutInfo::Stringified(
            "layout {\n    pane\n    pane\n}\n".to_owned(),
        ))
        .skip_concurrency_slot()
        .start_in_background();
    let closing_terminal = closing.expect_pty_spawn();
    let _other_terminal = closing.expect_pty_spawn();
    closing_terminal.disable_echo();
    closing_terminal.output(b"$ the item that closes\r\n");
    closing_terminal.output(&needs_you("Closes?"));
    wait_for_count(&zellij, 2);
    open_inbox_on(&zellij, "Closes?");
    wait_for_the_view_of(&zellij, &closing, "the item that closes");
    start_typing(&zellij);
    closing_terminal.exit(Some(0));
    zellij.wait_until("the view says the pane closed", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, closing.session_name(), "The pane was closed")
    });
    zellij.wait_until("the closed item left the inbox", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
    });
    press_release_key(&zellij);
    wait_for_the_view_of(&zellij, &staying, "the item that stays");
    zellij.quit();
}

#[test]
fn a_narrow_browser_lists_the_items_without_a_live_view() {
    let mut zellij = TestRunner::new(Size { cols: 80, rows: 24 })
        .with_config(ATTENTION_CONFIG)
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.cursor.is_some()
    });
    let (other, _) = item_session("$ never drawn when narrow", "Narrow?");
    wait_for_count(&zellij, 1);
    let grid_snapshot = open_inbox_on(&zellij, "Narrow?");
    assert!(
        grid_snapshot.contains("<ENTER> go there"),
        "{}",
        grid_snapshot.text
    );
    std::thread::sleep(std::time::Duration::from_millis(500));
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains("never drawn when narrow")
            && view_area(&grid_snapshot, other.session_name()).is_none(),
        "the narrow browser draws a live view:\n{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::ENTER);
    let (switch, _previous) = zellij.follow_switch_into(other);
    assert!(switch.pane_id.is_some());
    zellij.quit();
}

fn panel_layout() -> String {
    "keybinds clear-defaults=true {\n}\nlayout {\n    pane {\n        plugin location=\"zellij:session-manager\" {\n            role \"app_panel\"\n            view \"auto\"\n        }\n    }\n}\n".to_owned()
}

fn start_panel() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_session_name(APP_PANEL_SESSION)
        .with_config("show_startup_tips false\nshow_release_notes false\npane_frames false")
        .with_layout(LayoutInfo::Stringified(panel_layout()))
        .start()
}

#[test]
fn the_desktop_panel_shows_the_list_and_the_live_view_and_types_into_it() {
    let (other, terminal) = item_session("$ panel source", "Panel?");
    let mut panel = start_panel();
    wait_for_the_view_of(&panel, &other, "panel source");
    start_typing(&panel);
    type_into(&panel, &terminal, "from the panel");
    press_release_key(&panel);
    panel.wait_until("the release key returns to the list", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    panel.send_stdin(&keys::TAB);
    panel.wait_until("the sessions view clears the live view", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_HELP) && !grid_snapshot.contains("panel source")
    });
    panel.send_stdin(&keys::TAB);
    wait_for_the_view_of(&panel, &other, "panel source");
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
    let reused = panel.attach_client(TERMINAL_SIZE);
    reused.wait_until("the reused panel shows the live view", |grid_snapshot| {
        shown_inside_the_view(grid_snapshot, other.session_name(), "panel source")
    });
    reused.send_stdin(&keys::ENTER);
    reused.wait_until("the reused panel types", |grid_snapshot| {
        grid_snapshot.contains(TYPING_INTO)
    });
    reused.send_stdin(b"again");
    terminal.wait_for_stdin("the reused panel's keys reached the source", |stdin| {
        stdin.ends_with(b"again")
    });
    press_release_key(&reused);
    reused.wait_until(
        "the release key works in the reused panel",
        |grid_snapshot| !grid_snapshot.contains(TYPING_INTO),
    );
    reused.send_stdin(&keys::ESC);
    reused.wait_for_client_to_exit();
}

#[test]
fn the_startup_card_still_lists_the_items_without_a_live_view() {
    let (other, _) = item_session("$ not on the card", "On the card?");
    let (mut zellij, _) = main_session_with(TERMINAL_SIZE, CARD_CONFIG);
    let place = format!("{} ", other.session_name());
    zellij.wait_until("the card lists the item", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains(&place)
    });
    std::thread::sleep(std::time::Duration::from_millis(500));
    let card = zellij.snapshot();
    assert!(card.contains("Open inbox"), "{}", card.text);
    assert!(
        !card.contains("not on the card") && view_area(&card, other.session_name()).is_none(),
        "the card draws a live view:\n{}",
        card.text
    );
    let (column, row) = card
        .position_of("Open inbox")
        .expect("the card offers the inbox");
    zellij.send_stdin(&keys::sgr_click(column + 1, row));
    wait_for_the_view_of(&zellij, &other, "not on the card");
    zellij.quit();
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

fn watch_and_leave(session: &BackgroundTestSession) {
    let watcher = session.attach_client(TERMINAL_SIZE);
    watcher.wait_until(
        "the client in the watched session is attached",
        |grid_snapshot| grid_snapshot.status_bar_appears(),
    );
    watcher.send_action(Action::WatchPane);
    zellij_integration_tests::wait_for("the pane is watched", || {
        zellij_utils::session_link::request_session_attention(
            session.session_name(),
            Duration::from_secs(2),
        )
        .map(|attention| attention.items.iter().any(|item| item.attention.watched))
        .unwrap_or(false)
    });
    watcher.detach();
}

fn list_line(text: &str) -> String {
    format!("│  {}", text)
}

fn rows_in_order(grid_snapshot: &GridSnapshot, texts: &[&str]) -> bool {
    let rows: Vec<Option<usize>> = texts
        .iter()
        .map(|text| grid_snapshot.row_of_line(text))
        .collect();
    rows.iter().all(|row| row.is_some()) && rows.windows(2).all(|pair| pair[0] < pair[1])
}

#[test]
fn the_inbox_groups_items_by_status_with_watching_first_and_s_groups_them_by_session() {
    let (mut zellij, _) = main_session();
    let (watched, _) = background_session_with(ATTENTION_CONFIG);
    watch_and_leave(&watched);
    let (failed, failed_terminal) = background_session_with(ATTENTION_CONFIG);
    failed_command(&failed_terminal, "cargo test", "1");
    let (asking, _) = item_session("$ asking", "Group me?");
    let watched_row = row_of("◉", watched.session_name());
    let failed_row = row_of("✗", failed.session_name());
    let asking_row = row_of("●", asking.session_name());
    zellij_integration_tests::attention::wait_for_counted(failed.session_name(), 1);
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Group me?");
    let by_status = [
        list_line("Watching"),
        list_line(&watched_row),
        list_line("Needs You"),
        list_line(&asking_row),
        list_line("Failed"),
        list_line(&failed_row),
    ];
    let by_status: Vec<&str> = by_status.iter().map(String::as_str).collect();
    zellij.wait_until(
        "the sections are Watching, Needs You, then Failed",
        |grid_snapshot| rows_in_order(grid_snapshot, &by_status),
    );
    zellij.send_stdin(&keys::key('s'));
    let by_session = [
        list_line(&format!("{} ", watched.session_name())),
        list_line(&format!("{} ", asking.session_name())),
        list_line(&format!("{} ", failed.session_name())),
    ];
    let by_session: Vec<&str> = by_session.iter().map(String::as_str).collect();
    let grouped = zellij.wait_until("s groups the items by session", |grid_snapshot| {
        rows_in_order(grid_snapshot, &by_session)
    });
    assert!(
        !grouped.contains(&list_line("Needs You"))
            && !grouped.contains(&list_line("Failed"))
            && !grouped.contains(&asking_row),
        "the status headings and the session column are gone:\n{}",
        grouped.text
    );
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("s groups them by status again", |grid_snapshot| {
        rows_in_order(grid_snapshot, &by_status)
    });
    zellij.quit();
}

#[test]
fn a_double_esc_stops_typing_and_a_single_esc_reaches_the_program() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ escape here", "Escape?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Escape?");
    wait_for_the_view_of(&zellij, &other, "escape here");
    start_typing(&zellij);
    press_esc(&zellij);
    terminal.wait_for_stdin("a single esc reaches the program", |stdin| {
        stdin.ends_with(b"\x1b")
    });
    type_into(&zellij, &terminal, "q");
    assert!(zellij.snapshot().contains(TYPING_INTO));
    press_release_key(&zellij);
    zellij.wait_until("a double esc stops typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    assert!(
        terminal.stdin_bytes().ends_with(b"\x1bq\x1b"),
        "the first esc of the pair reaches the program: {:?}",
        String::from_utf8_lossy(&terminal.stdin_bytes())
    );
    zellij.quit();
}

fn source_with_a_client_on_the_other_pane(
    shown: &str,
    question: &str,
) -> (BackgroundTestSession, FakePtyHandle, TestClient, (u16, u16)) {
    let (session, terminal) = two_pane_item_session(shown, question);
    let client = session.attach_client(TERMINAL_SIZE);
    client.wait_until("the source client is up", |grid_snapshot| {
        grid_snapshot.contains(shown)
    });
    client.send_action(Action::FocusNextPane);
    let layout_size = terminal.wait_for_size(
        "the source pane has the size of the source client",
        |cols, rows| {
            cols as usize + 10 > TERMINAL_SIZE.cols && (rows as usize) < TERMINAL_SIZE.rows
        },
    );
    (session, terminal, client, layout_size)
}

fn shown_view_size(
    zellij: &TestSession,
    session: &BackgroundTestSession,
    text: &str,
) -> (u16, u16) {
    let grid_snapshot = wait_for_the_view_of(zellij, session, text);
    let area = view_area(&grid_snapshot, session.session_name()).unwrap();
    (area.cols as u16, area.rows as u16)
}

#[test]
fn browsing_resizes_a_pane_once_it_settles_and_it_keeps_that_size_until_focused_in_its_session() {
    let (mut zellij, _) = main_session();
    let (other, other_terminal) = item_session("$ the other item", "Other?");
    wait_for_count(&zellij, 1);
    let (_passed, passed_terminal) = item_session("$ the passed item", "Passed?");
    wait_for_count(&zellij, 2);
    let passed_sizes = passed_terminal.size_history();
    let (first, first_terminal, source_client, layout_size) =
        source_with_a_client_on_the_other_pane("first item", "First?");
    wait_for_count(&zellij, 3);
    open_inbox_on(&zellij, "First?");
    let view_size = shown_view_size(&zellij, &first, "first item");
    first_terminal.wait_for_size("the shown pane is resized to the view", |cols, rows| {
        (cols, rows) == view_size
    });
    let mut two_downs = keys::DOWN.to_vec();
    two_downs.extend_from_slice(&keys::DOWN);
    zellij.send_stdin(&two_downs);
    wait_for_the_view_of(&zellij, &other, "the other item");
    other_terminal.wait_for_size(
        "the item the browsing stopped at is resized once it settles",
        |cols, rows| (cols, rows) == view_size,
    );
    assert_eq!(
        passed_terminal.size_history(),
        passed_sizes,
        "an item browsed past faster than the settle time was resized"
    );
    assert_eq!(
        first_terminal.size(),
        Some(view_size),
        "the pane keeps the size of the view after the view moves on"
    );
    source_client.send_action(Action::FocusNextPane);
    first_terminal.wait_for_size(
        "focusing the pane in its own session gives it its size back",
        |cols, rows| (cols, rows) == layout_size,
    );
    source_client.detach();
    zellij.quit();
}

#[test]
fn typing_into_a_shown_pane_in_its_own_session_gives_its_size_back_until_typing_through_the_view() {
    let (mut zellij, _) = main_session();
    let (session, terminal) = two_pane_item_session("typed in place", "Local?");
    let source_client = session.attach_client(TERMINAL_SIZE);
    source_client.wait_until("the source client is up", |grid_snapshot| {
        grid_snapshot.contains("typed in place")
    });
    let layout_size = terminal.wait_for_size(
        "the source pane has the size of the source client",
        |cols, rows| {
            cols as usize + 10 > TERMINAL_SIZE.cols && (rows as usize) < TERMINAL_SIZE.rows
        },
    );
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Local?");
    let view_size = shown_view_size(&zellij, &session, "typed in place");
    terminal.wait_for_size("the shown pane is resized to the view", |cols, rows| {
        (cols, rows) == view_size
    });
    source_client.send_stdin(b"local");
    terminal.wait_for_stdin("the local keys reached the pane", |stdin| {
        stdin.ends_with(b"local")
    });
    terminal.wait_for_size(
        "typing in its own session gives the pane its size back",
        |cols, rows| (cols, rows) == layout_size,
    );
    start_typing(&zellij);
    terminal.wait_for_size(
        "typing through the view takes the size back",
        |cols, rows| (cols, rows) == view_size,
    );
    press_release_key(&zellij);
    source_client.detach();
    zellij.quit();
}

#[test]
fn new_output_of_a_watched_pane_is_new_output_and_a_watch_alert_clears_once_shown_in_the_view() {
    let (mut zellij, _) = main_session();
    let (watched, terminal) = background_session_with(ATTENTION_CONFIG);
    watch_and_leave(&watched);
    let watched_row = row_of("•", watched.session_name());
    zellij_integration_tests::wait_for(
        "output past the startup grace counts as new output",
        || {
            terminal.output(b"unseen output\r\n");
            zellij_utils::session_link::request_session_attention(
                watched.session_name(),
                Duration::from_secs(2),
            )
            .map(|attention| {
                attention
                    .items
                    .iter()
                    .any(|item| item.attention.effective_status() == AttentionStatus::NewOutput)
            })
            .unwrap_or(false)
        },
    );
    open_inbox(&zellij);
    let grid_snapshot = zellij.wait_until("the watched pane shows new output", |grid_snapshot| {
        grid_snapshot.contains(&watched_row)
    });
    let bar = grid_snapshot.lines()[0].clone();
    assert!(
        !bar.contains("need you") && !bar.contains("watch alert"),
        "new output of a watched pane needs you: {}",
        bar
    );
    assert!(
        !grid_snapshot.contains("Needs You"),
        "{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the browser is closed", |grid_snapshot| {
        !shows_inbox(grid_snapshot)
    });
    failed_command(&terminal, "make", "0");
    zellij.wait_until("the watch raised an alert", |grid_snapshot| {
        grid_snapshot.lines()[0].contains("watch alert")
    });
    zellij.send_stdin(&keys::F9);
    wait_for_the_view_of(&zellij, &watched, "make");
    zellij.wait_until(
        "the alert clears once the pane was shown for a moment",
        |grid_snapshot| !grid_snapshot.lines()[0].contains("watch alert"),
    );
    zellij.quit();
}

struct PeerScanInterval;

impl PeerScanInterval {
    fn longer_than_the_test() -> Self {
        std::env::set_var(PEER_SCAN_INTERVAL_ENV_VAR, "600000");
        PeerScanInterval
    }
}

impl Drop for PeerScanInterval {
    fn drop(&mut self) {
        std::env::remove_var(PEER_SCAN_INTERVAL_ENV_VAR);
    }
}

#[test]
fn an_attention_change_in_another_session_reaches_the_desktop_panel_at_once() {
    let _no_rescan = PeerScanInterval::longer_than_the_test();
    let (other, terminal) = background_session_with(ATTENTION_CONFIG);
    let mut panel = start_panel();
    panel.wait_until("the panel knows the other session", |grid_snapshot| {
        grid_snapshot.contains("Sessions (")
    });
    terminal.output(&needs_you("Pushed?"));
    wait_for_the_view_of(&panel, &other, "$");
    panel.wait_until("the panel lists the item", |grid_snapshot| {
        grid_snapshot.contains("Pushed?")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

fn two_typed_items(
    zellij: &TestSession,
) -> (BackgroundTestSession, FakePtyHandle, BackgroundTestSession) {
    let (older, _) = item_session("$ the older pane", "Older?");
    wait_for_count(zellij, 1);
    let (newer, newer_terminal) = item_session("$ the newer pane", "Newer?");
    wait_for_count(zellij, 2);
    open_inbox_on(zellij, "Newer?");
    wait_for_the_view_of(zellij, &newer, "the newer pane");
    (newer, newer_terminal, older)
}

#[test]
fn a_click_on_the_selected_row_while_typing_ends_typing_without_starting_it_again() {
    let (mut zellij, _) = main_session();
    let (newer, newer_terminal, _older) = two_typed_items(&zellij);
    start_typing(&zellij);
    type_into(&zellij, &newer_terminal, "before");
    let grid_snapshot = zellij.snapshot();
    let (column, row) = grid_snapshot
        .position_of(&row_of("●", newer.session_name()))
        .expect("the selected row is drawn");
    zellij.send_stdin(&keys::sgr_click(column + 2, row));
    let grid_snapshot = zellij.wait_until("the click ends typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO)
    });
    assert!(
        grid_snapshot.contains("· Newer?"),
        "the click moved the selection:\n{}",
        grid_snapshot.text
    );
    zellij.send_stdin(&keys::DOWN);
    let grid_snapshot = zellij
        .wait_until("the list takes the key after the click", |grid_snapshot| {
            grid_snapshot.contains("· Older?")
        });
    assert!(
        !grid_snapshot.contains(TYPING_INTO),
        "typing started again:\n{}",
        grid_snapshot.text
    );
    assert!(
        newer_terminal.stdin_bytes().ends_with(b"before"),
        "a key after the click reached the source: {:?}",
        String::from_utf8_lossy(&newer_terminal.stdin_bytes())
    );
    zellij.quit();
}

#[test]
fn the_wheel_does_not_move_the_selection_while_typing() {
    let (mut zellij, _) = main_session();
    let (_newer, newer_terminal, _older) = two_typed_items(&zellij);
    start_typing(&zellij);
    type_into(&zellij, &newer_terminal, "x");
    let grid_snapshot = zellij.snapshot();
    let (column, row) = grid_snapshot
        .position_of("Needs You")
        .expect("the list is drawn");
    zellij.send_stdin(format!("\u{1b}[<65;{};{}M", column + 1, row + 2).as_bytes());
    type_into(&zellij, &newer_terminal, "y");
    press_release_key(&zellij);
    zellij.wait_until("typing ended", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    let grid_snapshot = zellij.snapshot();
    assert!(
        grid_snapshot.contains("· Newer?"),
        "the wheel moved the selection while typing:\n{}",
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn z_after_the_resize_action_turns_resizing_back_on() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ toggled", "Toggle?");
    let layout_size = layout_size_of(&terminal);
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Toggle?");
    let view_size = shown_view_size(&zellij, &other, "toggled");
    terminal.wait_for_size("the shown pane is resized", |cols, rows| {
        (cols, rows) == view_size
    });
    zellij.main_client().send_action(Action::ToggleMirrorResize);
    zellij.wait_until("the action turned resizing off", |grid_snapshot| {
        grid_snapshot.contains("resize off")
    });
    terminal.wait_for_size("the pane has its size back", |cols, rows| {
        (cols, rows) == layout_size
    });
    zellij.send_stdin(&keys::key('z'));
    zellij.wait_until("z turned resizing back on", |grid_snapshot| {
        !grid_snapshot.contains("resize off")
    });
    terminal.wait_for_size("the pane is resized again", |cols, rows| {
        (cols, rows) == view_size
    });
    zellij.quit();
}

#[test]
fn esc_then_a_key_that_runs_a_keybinding_then_esc_does_not_stop_typing() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ bound key between", "Between?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Between?");
    wait_for_the_view_of(&zellij, &other, "bound key between");
    start_typing(&zellij);
    let mut presses = keys::ESC_KEY.to_vec();
    presses.extend_from_slice(&keys::ctrl('g'));
    presses.extend_from_slice(&keys::ESC_KEY);
    zellij.send_stdin(&presses);
    type_into(&zellij, &terminal, "still typing");
    assert!(
        zellij.snapshot().contains(TYPING_INTO),
        "Esc, a bound key and Esc stopped typing"
    );
    let stdin = String::from_utf8_lossy(&terminal.stdin_bytes()).replace("\u{1b}[27u", "\u{1b}");
    assert!(
        stdin.ends_with("\u{1b}\u{1b}still typing") && !stdin.contains('\u{7}'),
        "both escapes and only they reach the program: {:?}",
        stdin
    );
    press_release_key(&zellij);
    zellij.wait_until("a double esc stops typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO)
    });
    zellij.quit();
}

#[test]
fn a_click_on_the_selected_row_starts_typing_again_after_a_click_in_another_pane_ended_it() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ clicked elsewhere", "Elsewhere?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Elsewhere?");
    wait_for_the_view_of(&zellij, &other, "clicked elsewhere");
    start_typing(&zellij);
    let grid_snapshot = zellij.snapshot();
    let (tab_column, tab_row) = grid_snapshot
        .position_of("Tab #1")
        .expect("the tab bar is drawn");
    zellij.send_stdin(&keys::sgr_click(tab_column + 1, tab_row));
    zellij.wait_until("the click in the tab bar ends typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    let grid_snapshot = zellij.snapshot();
    let (column, row) = grid_snapshot
        .position_of(&row_of("●", other.session_name()))
        .expect("the selected row is drawn");
    zellij.send_stdin(&keys::sgr_click(column + 2, row));
    zellij.wait_until(
        "the click on the selected row starts typing",
        |grid_snapshot| grid_snapshot.contains(TYPING_INTO),
    );
    type_into(&zellij, &terminal, "typed again");
    press_release_key(&zellij);
    zellij.quit();
}

#[test]
fn a_double_esc_sent_as_two_plain_escape_bytes_stops_typing() {
    let (mut zellij, _) = main_session();
    let (other, terminal) = item_session("$ plain escapes", "Plain?");
    wait_for_count(&zellij, 1);
    open_inbox_on(&zellij, "Plain?");
    wait_for_the_view_of(&zellij, &other, "plain escapes");
    start_typing(&zellij);
    type_into(&zellij, &terminal, "a");
    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("two plain escapes stop typing", |grid_snapshot| {
        !grid_snapshot.contains(TYPING_INTO) && grid_snapshot.contains(TYPE_INTO_IT)
    });
    zellij.quit();
}
