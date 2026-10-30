#![cfg(unix)]

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use std::time::Duration;
use zellij_integration_tests::inbox::{shows_inbox, ITEM_ACTIONS};
use zellij_integration_tests::{keys, TestRunner, TERMINAL_SIZE};
use zellij_utils::consts::APP_PANEL_SESSION;
use zellij_utils::data::LayoutInfo;

fn panel_layout(view: &str) -> String {
    format!(
        "keybinds clear-defaults=true {{\n}}\nlayout {{\n    pane {{\n        plugin location=\"zellij:session-manager\" {{\n            role \"app_panel\"\n            view \"{}\"\n        }}\n    }}\n}}\n",
        view
    )
}

fn needs_you(text: &str) -> Vec<u8> {
    format!(
        "\u{1b}]7501;state=blocked:kind=question:msg={}\u{1b}\\",
        STANDARD.encode(text)
    )
    .into_bytes()
}

fn start_panel_on(view: &str) -> zellij_integration_tests::TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_session_name(APP_PANEL_SESSION)
        .with_config("show_startup_tips false\nshow_release_notes false\npane_frames false")
        .with_layout(LayoutInfo::Stringified(panel_layout(view)))
        .start()
}

fn start_panel() -> zellij_integration_tests::TestSession {
    start_panel_on("auto")
}

#[test]
fn the_app_panel_shows_the_inbox_and_going_to_an_item_switches_the_client() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Which branch to use?"));
    let mut panel = start_panel();
    panel.wait_until("the panel lists the item", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains(background.session_name())
    });
    let listed: Vec<String> = zellij_utils::sessions::get_sessions()
        .unwrap()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(
        !listed.iter().any(|name| name == APP_PANEL_SESSION),
        "the panel session is hidden from session lists: {:?}",
        listed
    );
    panel.send_stdin(&keys::TAB);
    panel.wait_until(
        "the sessions tab lists only real sessions",
        |grid_snapshot| {
            grid_snapshot.contains("Sessions (1)") && !grid_snapshot.contains(APP_PANEL_SESSION)
        },
    );
    panel.send_stdin(&keys::TAB);
    panel.wait_until("back on the inbox", |grid_snapshot| {
        grid_snapshot.contains("Which bran") && grid_snapshot.contains("<ENTER> go")
    });
    panel.send_stdin(&keys::ENTER);
    let switched = panel
        .join_main_client()
        .expect("going to the item switches the client away");
    assert_eq!(switched.name.as_deref(), Some(background.session_name()));
    assert_eq!(switched.pane_id, Some((terminal.terminal_id(), false)));
}

#[test]
fn escape_detaches_from_the_app_panel_and_keeps_it_running() {
    let mut panel = start_panel();
    panel.wait_until("the panel draws its tabs", |grid_snapshot| {
        grid_snapshot.contains("Inbox (") && grid_snapshot.contains("Sessions (")
    });
    std::thread::sleep(Duration::from_millis(300));
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
    assert!(zellij_utils::session_link::session_answers(
        APP_PANEL_SESSION,
        Duration::from_secs(2)
    ));
    assert!(!zellij_utils::sessions::get_resurrectable_session_names()
        .iter()
        .any(|name| name == APP_PANEL_SESSION));
}

#[test]
fn the_app_panel_ignores_zellij_keybindings_and_closes_from_its_cross() {
    let mut panel = start_panel_on("sessions");
    let grid_snapshot = panel.wait_until("the panel draws its tabs and cross", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .map(|l| l.contains("Sessions (") && l.trim_end().ends_with('×'))
            .unwrap_or(false)
    });
    assert!(
        !grid_snapshot.lines()[0].contains('┌'),
        "the panel draws no zellij frame: {}",
        grid_snapshot.lines()[0]
    );
    panel.send_stdin(&keys::ctrl('g'));
    panel.send_stdin(&keys::ctrl('t'));
    panel.send_stdin(&keys::key('n'));
    std::thread::sleep(Duration::from_millis(500));
    let after = panel.snapshot();
    assert!(
        !after.contains("Tab #2")
            && !after
                .text
                .replace("<TAB> switch", "")
                .replace("<TAB> inbox", "")
                .replace("<TAB> sessions", "")
                .contains("TAB"),
        "zellij keybindings reached the panel session: {}",
        after.lines().join("\n")
    );
    let first = after.lines()[0].clone();
    let column = first[..first.rfind('×').unwrap()].chars().count();
    panel.send_stdin(&keys::sgr_click(column, 0));
    assert!(panel.join_main_client().is_none());
    assert!(zellij_utils::session_link::session_answers(
        APP_PANEL_SESSION,
        Duration::from_secs(2)
    ));
}

#[test]
fn the_quit_question_is_centered_and_confirming_asks_the_app_to_quit() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let mut panel = start_panel_on("quit");
    let grid_snapshot = panel.wait_until("the quit question lists the session", |grid_snapshot| {
        grid_snapshot.contains("Quit Zellij and stop all sessions?")
            && grid_snapshot.contains(background.session_name())
    });
    let lines = grid_snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains("Quit Zellij and stop"))
        .unwrap();
    let line = &lines[row];
    let column = line[..line.find("Quit Zellij").unwrap()].chars().count();
    let width = TERMINAL_SIZE.cols;
    assert!(
        column > width / 4 && column < width / 2,
        "the question is centred horizontally (column {}): {}",
        column,
        line
    );
    assert!(
        row > lines.len() / 4 && row < lines.len() * 3 / 4,
        "the question is centred vertically (row {})",
        row
    );
    assert!(!grid_snapshot.contains(APP_PANEL_SESSION));
    panel.send_stdin(&keys::ENTER);
    let switched = panel
        .join_main_client()
        .expect("confirming asks the app to quit");
    assert_eq!(
        switched.name.as_deref(),
        Some(zellij_utils::consts::APP_QUIT_REQUEST)
    );
}

#[test]
fn exited_sessions_are_collapsed_under_one_line_in_the_sessions_view() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one", "gone-two"]);
    let mut panel = start_panel_on("sessions");
    let grid_snapshot = panel.wait_until("the exited sessions are folded", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && grid_snapshot.contains(background.session_name())
    });
    assert!(!grid_snapshot.contains("gone-one"));
    let lines = grid_snapshot.lines();
    let row = lines.iter().position(|l| l.contains("Exited (2)")).unwrap();
    let column = lines[row][..lines[row].find("Exited").unwrap()]
        .chars()
        .count();
    panel.send_stdin(&keys::sgr_click(column, row));
    panel.wait_until("the exited sessions unfold", |grid_snapshot| {
        grid_snapshot.contains("▾ Exited (2)")
            && grid_snapshot.contains("gone-one")
            && grid_snapshot.contains("gone-two")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

const LEFT: &[u8] = b"\x1b[D";
const HOME: &[u8] = b"\x1b[H";
const END: &[u8] = b"\x1b[F";
const DELETE: &[u8] = b"\x1b[3~";
const SHIFT_DELETE: &[u8] = b"\x1b[3;2~";
const BACKSPACE: &[u8] = b"\x7f";
const SESSIONS_VIEW_ONLY: &str = "Folder";
const EMPTY_PREVIEW: &str = "Select a session to see its preview";

fn send_view(view: &str) {
    zellij_utils::session_link::send_actions_to_session(
        APP_PANEL_SESSION,
        vec![zellij_utils::input::actions::Action::KeybindPipe {
            name: Some("zellij-app-panel-view".to_owned()),
            payload: Some(view.to_owned()),
            args: None,
            plugin: None,
            plugin_id: None,
            configuration: None,
            launch_new: false,
            skip_cache: false,
            floating: None,
            in_place: None,
            cwd: None,
            pane_title: None,
        }],
    )
    .unwrap();
}

const RECENT_SESSIONS: [&str; 3] = ["recent-a", "recent-b", "recent-c"];

fn leave_exited_sessions_used(names: &[&str], seconds_ago: u64) {
    let index = zellij_utils::session_index::SessionIndex::open_default().unwrap();
    let used = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - seconds_ago;
    for name in names {
        let folder = zellij_utils::consts::session_info_folder_for_session(name);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("session-layout.kdl"), "layout {\n    pane\n}\n").unwrap();
        index.ensure_row(name, used).unwrap();
    }
}

fn leave_older_exited_sessions(names: &[&str]) {
    leave_exited_sessions_used(names, 600);
    leave_exited_sessions_used(&RECENT_SESSIONS, 0);
}

fn saved_layout_exists(name: &str) -> bool {
    zellij_utils::consts::session_info_folder_for_session(name)
        .join("session-layout.kdl")
        .exists()
}

fn position(
    grid_snapshot: &zellij_integration_tests::GridSnapshot,
    needle: &str,
) -> (usize, usize) {
    let lines = grid_snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("{:?} is not on screen:\n{}", needle, grid_snapshot.text));
    let column = lines[row][..lines[row].find(needle).unwrap()]
        .chars()
        .count();
    (column, row)
}

fn list_position(
    grid_snapshot: &zellij_integration_tests::GridSnapshot,
    needle: &str,
) -> (usize, usize) {
    let lines = grid_snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains(needle) && !line.contains('┌'))
        .unwrap_or_else(|| panic!("{:?} is not listed:\n{}", needle, grid_snapshot.text));
    let column = lines[row][..lines[row].find(needle).unwrap()]
        .chars()
        .count();
    (column, row)
}

fn is_selected(grid_snapshot: &zellij_integration_tests::GridSnapshot, needle: &str) -> bool {
    if !grid_snapshot.contains(needle) {
        return false;
    }
    if needle.starts_with("Exited") {
        return grid_snapshot.contains(EMPTY_PREVIEW);
    }
    grid_snapshot
        .lines()
        .iter()
        .any(|line| line.contains(&format!("┌ {} ", needle)))
}

fn is_coloured(code: Option<zellij_integration_tests::client_screen::AnsiCode>) -> bool {
    !matches!(
        code,
        None | Some(zellij_integration_tests::client_screen::AnsiCode::Reset)
    )
}

fn wait_for_selected(panel: &zellij_integration_tests::TestSession, needle: &str) {
    panel.wait_until(&format!("{:?} to be selected", needle), |grid_snapshot| {
        is_selected(grid_snapshot, needle)
    });
}

#[test]
fn the_automatic_panel_turns_to_the_inbox_when_an_item_arrives_before_any_input() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel();
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
            && !shows_inbox(&grid_snapshot)
    });
    terminal.output(&needs_you("Deploy now?"));
    panel.wait_until("the panel turns to the inbox", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains("Deploy now")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn after_any_input_the_automatic_panel_stays_where_it_is() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel();
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, background.session_name());
    terminal.output(&needs_you("Deploy now?"));
    panel.wait_until("the item is counted", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
    });
    std::thread::sleep(Duration::from_millis(500));
    let grid_snapshot = panel.snapshot();
    assert!(
        grid_snapshot.contains(SESSIONS_VIEW_ONLY) && !shows_inbox(&grid_snapshot),
        "the panel left the view the user was using:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn the_view_pipe_switches_a_running_panel() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Deploy now?"));
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the inbox", |grid_snapshot| {
        shows_inbox(&grid_snapshot) && grid_snapshot.contains("Deploy now")
    });
    send_view("sessions");
    panel.wait_until("the panel shows the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
            && !shows_inbox(&grid_snapshot)
    });
    send_view("quit");
    panel.wait_until("the panel asks about quitting", |grid_snapshot| {
        grid_snapshot.contains("Quit Zellij and stop all sessions?")
    });
    send_view("inbox");
    panel.wait_until("the panel is back on the inbox", |grid_snapshot| {
        shows_inbox(&grid_snapshot) && !grid_snapshot.contains("Quit Zellij and stop all sessions?")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn asking_for_an_empty_inbox_shows_the_sessions_until_tab_is_pressed() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
            && !shows_inbox(&grid_snapshot)
    });
    send_view("inbox");
    std::thread::sleep(Duration::from_millis(300));
    let grid_snapshot = panel.snapshot();
    assert!(
        grid_snapshot.contains(SESSIONS_VIEW_ONLY) && !shows_inbox(&grid_snapshot),
        "asking for an empty inbox left the sessions:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::TAB);
    panel.wait_until("tab shows the empty inbox", |grid_snapshot| {
        shows_inbox(&grid_snapshot)
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn the_keyboard_walks_onto_into_and_out_of_the_exited_group() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one", "gone-two"]);
    let live = background.session_name().to_owned();
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the exited sessions are folded", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && grid_snapshot.contains(&live)
    });
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, &live);
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, RECENT_SESSIONS[0]);
    panel.send_stdin(&keys::UP);
    wait_for_selected(&panel, &live);
    panel.send_stdin(END);
    wait_for_selected(&panel, "Exited (2)");
    panel.send_stdin(&keys::UP);
    wait_for_selected(&panel, RECENT_SESSIONS[2]);
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, "Exited (2)");
    panel.send_stdin(&keys::RIGHT);
    panel.wait_until("right opens the group", |grid_snapshot| {
        grid_snapshot.contains("▾ Exited (2)") && is_selected(grid_snapshot, "Exited (2)")
    });
    panel.send_stdin(&keys::RIGHT);
    wait_for_selected(&panel, "gone-one");
    panel.send_stdin(LEFT);
    wait_for_selected(&panel, "Exited (2)");
    panel.send_stdin(LEFT);
    panel.wait_until("left closes the group", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && !grid_snapshot.contains("gone-one")
    });
    panel.send_stdin(&keys::ENTER);
    panel.wait_until("enter opens the group", |grid_snapshot| {
        grid_snapshot.contains("▾ Exited (2)") && grid_snapshot.contains("gone-two")
    });
    panel.send_stdin(END);
    wait_for_selected(&panel, "gone-two");
    panel.send_stdin(HOME);
    wait_for_selected(&panel, &live);
    panel.send_stdin(END);
    wait_for_selected(&panel, "gone-two");
    panel.send_stdin(LEFT);
    wait_for_selected(&panel, "Exited (2)");
    panel.send_stdin(&keys::ENTER);
    panel.wait_until("enter closes the group", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)")
    });
    panel.send_stdin(HOME);
    wait_for_selected(&panel, &live);
    panel.send_stdin(END);
    wait_for_selected(&panel, "Exited (2)");
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn searching_opens_the_exited_group_and_clearing_the_search_folds_it_again() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one", "gone-two"]);
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the exited sessions are folded", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && grid_snapshot.contains(background.session_name())
    });
    for character in "two".chars() {
        panel.send_stdin(&keys::key(character));
    }
    panel.wait_until(
        "the match is shown under an opened header",
        |grid_snapshot| {
            grid_snapshot.contains("▾ Exited (1)")
                && grid_snapshot.contains("gone-two")
                && !grid_snapshot.contains("gone-one")
        },
    );
    for _ in 0..3 {
        panel.send_stdin(BACKSPACE);
    }
    panel.wait_until("clearing the search folds the group", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && !grid_snapshot.contains("gone-two")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn the_most_recent_exited_sessions_are_listed_above_the_folded_rest() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one"]);
    let mut panel = start_panel_on("sessions");
    let grid_snapshot = panel.wait_until("the recent sessions are listed", |grid_snapshot| {
        grid_snapshot.contains("Recent")
            && RECENT_SESSIONS
                .iter()
                .all(|name| grid_snapshot.contains(name))
            && grid_snapshot.contains("▸ Exited (1)")
            && grid_snapshot.contains(background.session_name())
    });
    assert!(!grid_snapshot.contains("gone-one"), "{}", grid_snapshot.text);
    let (_, heading_row) = list_position(&grid_snapshot, "Recent");
    let (_, first_row) = list_position(&grid_snapshot, RECENT_SESSIONS[0]);
    let (_, exited_row) = list_position(&grid_snapshot, "Exited (1)");
    assert!(heading_row < first_row && first_row < exited_row);
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn shift_delete_asks_and_then_deletes_every_exited_session() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one"]);
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the exited sessions are listed", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (1)") && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(SHIFT_DELETE);
    panel.wait_until("the deletion is confirmed first", |grid_snapshot| {
        grid_snapshot.contains("This will delete all 4 exited sessions")
            && grid_snapshot.contains("Are you sure? (y/n)")
    });
    panel.send_stdin(&keys::key('n'));
    panel.wait_until("no cancels", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (1)") && !grid_snapshot.contains("Are you sure")
    });
    assert!(saved_layout_exists("gone-one"));
    panel.send_stdin(SHIFT_DELETE);
    panel.wait_until("the question is asked again", |grid_snapshot| {
        grid_snapshot.contains("This will delete all 4 exited sessions")
    });
    panel.send_stdin(&keys::key('y'));
    zellij_integration_tests::wait_for("the exited sessions are deleted", || {
        !saved_layout_exists("gone-one")
            && RECENT_SESSIONS
                .iter()
                .all(|name| !saved_layout_exists(name))
    });
    panel.wait_until("the list has no exited sessions", |grid_snapshot| {
        !grid_snapshot.contains("Exited")
            && !grid_snapshot.contains("Recent")
            && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn delete_and_rename_do_nothing_on_the_exited_header_and_it_has_no_preview() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one", "gone-two"]);
    let live = background.session_name().to_owned();
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the exited sessions are folded", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (2)") && grid_snapshot.contains(&live)
    });
    panel.send_stdin(&keys::DOWN);
    panel.wait_until("the live session is previewed", |grid_snapshot| {
        is_selected(grid_snapshot, &live) && !grid_snapshot.contains(EMPTY_PREVIEW)
    });
    panel.send_stdin(END);
    panel.wait_until("the header has no preview", |grid_snapshot| {
        is_selected(grid_snapshot, "Exited (2)") && grid_snapshot.contains(EMPTY_PREVIEW)
    });
    std::thread::sleep(Duration::from_millis(300));
    let before = panel.snapshot().text;
    panel.send_stdin(DELETE);
    panel.send_stdin(&keys::ctrl('r'));
    std::thread::sleep(Duration::from_millis(500));
    let after = panel.snapshot();
    assert_eq!(before, after.text, "a key changed the screen on the header");
    for name in ["gone-one", "gone-two"] {
        assert!(zellij_utils::consts::session_info_folder_for_session(name)
            .join("session-layout.kdl")
            .exists());
    }
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn the_exited_header_is_dim_and_exited_sessions_sit_one_level_in() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&["gone-one"]);
    let live = background.session_name().to_owned();
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the exited session is folded", |grid_snapshot| {
        grid_snapshot.contains("▸ Exited (1)") && grid_snapshot.contains(&live)
    });
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, &live);
    panel.send_stdin(END);
    panel.send_stdin(&keys::RIGHT);
    panel.send_stdin(HOME);
    let grid_snapshot = panel.wait_until(
        "the group is open with the header unselected",
        |grid_snapshot| {
            grid_snapshot.contains("▾ Exited (1)")
                && grid_snapshot.contains("gone-one")
                && is_selected(grid_snapshot, &live)
        },
    );
    let (header_column, header_row) = position(&grid_snapshot, "▾ Exited (1)");
    let width = "▾ Exited (1)".chars().count();
    for column in header_column..header_column + width {
        let style = grid_snapshot.cell_style(column, header_row);
        assert!(
            style.dim,
            "the header is dim at column {}: {:?}",
            column, style
        );
        assert!(
            !is_coloured(style.foreground),
            "the header has no emphasis colour at column {}: {:?}",
            column,
            style
        );
    }
    let (live_column, live_row) = list_position(&grid_snapshot, &live);
    let (exited_column, exited_row) = list_position(&grid_snapshot, "gone-one");
    assert_eq!(
        exited_column,
        live_column + 2,
        "exited sessions are indented one level:\n{}",
        grid_snapshot.text
    );
    let lines = grid_snapshot.lines();
    let folder_column = |row: usize| {
        let line = &lines[row];
        line[..line.find('—').unwrap()].chars().count()
    };
    let (header_folder_column, _) = list_position(&grid_snapshot, "Folder");
    assert_eq!(
        folder_column(live_row),
        header_folder_column,
        "{}",
        grid_snapshot.text
    );
    assert_eq!(
        folder_column(exited_row),
        header_folder_column,
        "the columns of an indented exited session line up with the others:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn opening_the_panel_shows_the_newest_item_in_full_with_its_actions() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Which branch to use?"));
    let mut panel = start_panel();
    let row = format!("● {}", background.session_name());
    panel.wait_until(
        "the panel shows the item in full beside its live view",
        |grid_snapshot| {
            grid_snapshot.contains("Inbox (1)")
                && grid_snapshot.contains("Which branch to use?")
                && grid_snapshot.contains("<Alt ENTER> type")
                && grid_snapshot.contains("<ENTER> go")
                && grid_snapshot.contains(&row)
        },
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn a_panel_asked_for_an_empty_inbox_turns_to_it_when_an_item_arrives_before_any_input() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
            && !shows_inbox(&grid_snapshot)
    });
    terminal.output(&needs_you("Deploy now?"));
    panel.wait_until("the panel turns to the inbox", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
            && grid_snapshot.contains("Deploy now")
            && grid_snapshot.contains(ITEM_ACTIONS)
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn a_panel_asked_for_an_empty_inbox_stays_on_the_sessions_after_any_input() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(&keys::DOWN);
    wait_for_selected(&panel, background.session_name());
    terminal.output(&needs_you("Deploy now?"));
    panel.wait_until("the item is counted", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)")
    });
    std::thread::sleep(Duration::from_millis(500));
    let grid_snapshot = panel.snapshot();
    assert!(
        grid_snapshot.contains(SESSIONS_VIEW_ONLY) && !shows_inbox(&grid_snapshot),
        "the panel left the view the user was using:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn only_muted_items_waiting_still_open_the_sessions() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Deploy now?"));
    let name = background.session_name().to_owned();
    let needs_you_in = |name: &str| {
        zellij_utils::session_link::request_session_attention(name, Duration::from_secs(2))
            .ok()
            .map(|attention| attention.counts.needs_you)
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while needs_you_in(&name) != Some(1) {
        assert!(
            std::time::Instant::now() < deadline,
            "the item was not counted"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    zellij_utils::session_link::send_actions_to_session(
        &name,
        vec![zellij_utils::input::actions::Action::MutePaneByPaneId {
            pane_id: zellij_utils::data::PaneId::Terminal(terminal.terminal_id()),
        }],
    )
    .unwrap();
    while needs_you_in(&name) != Some(0) {
        assert!(
            std::time::Instant::now() < deadline,
            "the item stayed counted"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY) && grid_snapshot.contains(&name)
    });
    std::thread::sleep(Duration::from_millis(500));
    let grid_snapshot = panel.snapshot();
    assert!(
        !shows_inbox(&grid_snapshot),
        "a muted item opened the inbox:\n{}",
        grid_snapshot.text
    );
    send_view("inbox");
    std::thread::sleep(Duration::from_millis(300));
    let grid_snapshot = panel.snapshot();
    assert!(
        !shows_inbox(&grid_snapshot),
        "asking for the inbox with only a muted item left the sessions:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn the_pointer_resting_on_the_panel_does_not_stop_it_turning_to_the_inbox() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    let mut panel = start_panel_on("inbox");
    panel.wait_until("the panel opens on the sessions", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(b"\x1b[<35;20;10M");
    panel.send_stdin(b"\x1b[<35;21;10M");
    std::thread::sleep(Duration::from_millis(300));
    terminal.output(&needs_you("Deploy now?"));
    panel.wait_until("the panel turns to the inbox", |grid_snapshot| {
        grid_snapshot.contains("Inbox (1)") && grid_snapshot.contains("Deploy now")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

fn start_panel_sized(
    view: &str,
    size: zellij_integration_tests::Size,
) -> zellij_integration_tests::TestSession {
    TestRunner::new(size)
        .with_session_name(APP_PANEL_SESSION)
        .with_config("show_startup_tips false\nshow_release_notes false\npane_frames false")
        .with_layout(LayoutInfo::Stringified(panel_layout(view)))
        .start()
}

const TALL: zellij_integration_tests::Size = zellij_integration_tests::Size {
    cols: 140,
    rows: 44,
};

#[test]
fn alt_x_and_ctrl_a_are_not_offered_in_the_app_panel() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let mut panel = start_panel_sized("sessions", TALL);
    panel.wait_until("the panel lists the session", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
            && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(&keys::F1);
    let all_keys = panel.wait_until("the panel lists all keys", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<Ctrl r>")
    });
    assert!(
        !all_keys.contains("disconnect others from this session") && !all_keys.contains("<Ctrl a>"),
        "{}",
        all_keys.text
    );
    panel.send_stdin(&keys::ESC);
    panel.wait_until("the all keys screen closes", |grid_snapshot| {
        !grid_snapshot.contains("All keys") && grid_snapshot.contains(SESSIONS_VIEW_ONLY)
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
    assert!(panel.main_client().has_exited(), "the panel did not close");
    drop(panel);
    let mut zellij = TestRunner::new(TALL)
        .with_config("session_indicator true")
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"$ ");
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the sessions are shown", |grid_snapshot| {
        grid_snapshot.contains("<F1> all keys")
    });
    zellij.send_stdin(&keys::F1);
    let all_keys = zellij.wait_until("all keys are listed", |grid_snapshot| {
        grid_snapshot.contains("All keys") && grid_snapshot.contains("<Ctrl r>")
    });
    assert!(
        all_keys.contains("<Alt x>")
            && all_keys.contains("disconnect others from this session")
            && all_keys.contains("<Ctrl a>"),
        "{}",
        all_keys.text
    );
    zellij.quit();
}

#[test]
fn a_narrow_panel_lists_the_items_without_a_live_view_and_enter_goes_there() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ shown only when wide\r\n");
    terminal.output(&needs_you("Below or beside?"));
    let narrow = zellij_integration_tests::Size { cols: 50, rows: 40 };
    let mut panel = start_panel_sized("auto", narrow);
    let grid_snapshot = panel.wait_until("the panel shows the item", |grid_snapshot| {
        grid_snapshot.contains("Below or beside?") && grid_snapshot.contains("<ENTER> go there")
    });
    assert!(
        !grid_snapshot.contains("shown only when wide") && !grid_snapshot.contains(" › "),
        "a narrow panel shows a live view:\n{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ENTER);
    let switched = panel
        .join_main_client()
        .expect("enter goes to the item on a narrow panel");
    assert_eq!(switched.name.as_deref(), Some(background.session_name()));
}

#[test]
fn a_long_list_shows_more_markers() {
    let mut sessions = vec![];
    for index in 0..4 {
        let background = TestRunner::new(TERMINAL_SIZE)
            .skip_concurrency_slot()
            .start_in_background();
        let terminal = background.expect_pty_spawn();
        terminal.output(b"$ ");
        terminal.output(&needs_you(&format!("Long list item {}?", index)));
        sessions.push(background);
    }
    let short = zellij_integration_tests::Size { cols: 120, rows: 9 };
    let mut panel = start_panel_sized("inbox", short);
    panel.wait_until("the list shows a more marker", |grid_snapshot| {
        grid_snapshot.contains("Inbox (4)") && grid_snapshot.contains("more")
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}

#[test]
fn after_reopening_the_panel_unfocused_hover_marks_the_name_and_the_first_click_selects() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&[]);
    let mut panel = start_panel_on("sessions");
    panel.wait_until("the sessions are listed", |grid_snapshot| {
        grid_snapshot.contains(RECENT_SESSIONS[2]) && grid_snapshot.contains(background.session_name())
    });
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
    let reopened = panel.attach_client(TERMINAL_SIZE);
    send_view("sessions");
    let grid_snapshot = reopened.wait_until("the reopened panel lists the sessions", |grid_snapshot| {
        grid_snapshot.contains(RECENT_SESSIONS[2]) && grid_snapshot.contains(background.session_name())
    });
    reopened.send_stdin(b"\x1b[O");
    std::thread::sleep(Duration::from_millis(300));
    let (column, row) = list_position(&grid_snapshot, RECENT_SESSIONS[2]);
    reopened.send_stdin(format!("\x1b[<35;{};{}M", column + 1, row + 1).as_bytes());
    let grid_snapshot = reopened.wait_until("the hovered name turns italic", |grid_snapshot| {
        grid_snapshot.cell_style(column, row).italic
    });
    let line = &grid_snapshot.lines()[row];
    let dash = line[..line.find('—').unwrap()].chars().count();
    assert!(
        !grid_snapshot.cell_style(dash, row).italic,
        "only the name is italic:\n{}",
        grid_snapshot.text
    );
    assert!(!is_selected(&grid_snapshot, RECENT_SESSIONS[2]));
    reopened.send_stdin(&keys::sgr_click(column, row));
    reopened.wait_until("the first click selects the row", |grid_snapshot| {
        is_selected(grid_snapshot, RECENT_SESSIONS[2])
    });
}

#[test]
fn a_session_opened_by_a_mouse_press_leaves_the_reopened_panel_responsive() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    leave_older_exited_sessions(&[]);
    let live = background.session_name().to_owned();
    let mut panel = start_panel_on("sessions");
    let grid_snapshot = panel.wait_until("the sessions are listed", |grid_snapshot| {
        grid_snapshot.contains(RECENT_SESSIONS[2]) && grid_snapshot.contains(&live)
    });
    let (column, row) = list_position(&grid_snapshot, &live);
    panel.send_stdin(&keys::sgr_click(column, row));
    wait_for_selected(&panel, &live);
    panel.send_stdin(format!("\x1b[<0;{};{}M", column + 1, row + 1).as_bytes());
    let switched = panel
        .join_main_client()
        .expect("the press on the selected row opens the session");
    assert_eq!(switched.name.as_deref(), Some(live.as_str()));
    let reopened = panel.attach_client(TERMINAL_SIZE);
    send_view("sessions");
    let grid_snapshot = reopened.wait_until("the reopened panel lists the sessions", |grid_snapshot| {
        grid_snapshot.contains(RECENT_SESSIONS[2]) && grid_snapshot.contains(&live)
    });
    let (column, row) = list_position(&grid_snapshot, RECENT_SESSIONS[2]);
    reopened.send_stdin(format!("\x1b[<35;{};{}M", column + 1, row + 1).as_bytes());
    reopened.wait_until("hover works before any click", |grid_snapshot| {
        grid_snapshot.cell_style(column, row).italic
    });
    reopened.send_stdin(&keys::sgr_click(column, row));
    reopened.wait_until("the first click selects the row", |grid_snapshot| {
        is_selected(grid_snapshot, RECENT_SESSIONS[2])
    });
}

#[test]
fn the_inbox_and_sessions_symbols_use_a_badge_colour_set_in_the_config() {
    let background = TestRunner::new(TERMINAL_SIZE)
        .skip_concurrency_slot()
        .start_in_background();
    let terminal = background.expect_pty_spawn();
    terminal.output(b"$ ");
    terminal.output(&needs_you("Deploy now?"));
    let mut panel = TestRunner::new(TERMINAL_SIZE)
        .with_session_name(APP_PANEL_SESSION)
        .with_config(
            "show_startup_tips false\nshow_release_notes false\npane_frames false\nattention {\n    tray_badge_colors {\n        needs_you \"#8e4ec6\"\n    }\n}",
        )
        .with_layout(LayoutInfo::Stringified(panel_layout("inbox")))
        .start();
    let purple = Some(zellij_integration_tests::client_screen::AnsiCode::RgbCode((
        0x8e, 0x4e, 0xc6,
    )));
    let grid_snapshot = panel.wait_until("the inbox lists the item", |grid_snapshot| {
        shows_inbox(&grid_snapshot) && grid_snapshot.contains("Deploy now")
    });
    let lines = grid_snapshot.lines();
    let row = lines
        .iter()
        .position(|line| line.contains('●') && line.contains(background.session_name()))
        .expect("the item row is drawn");
    let column = lines[row][..lines[row].find('●').unwrap()].chars().count();
    assert_eq!(grid_snapshot.cell_foreground(column, row), purple);
    panel.send_stdin(&keys::TAB);
    let grid_snapshot = panel.wait_until("the sessions list is shown", |grid_snapshot| {
        grid_snapshot.contains(SESSIONS_VIEW_ONLY)
    });
    let (column, row) = list_position(&grid_snapshot, background.session_name());
    assert_eq!(
        grid_snapshot.cell_foreground(column.saturating_sub(2), row),
        purple,
        "{}",
        grid_snapshot.text
    );
    panel.send_stdin(&keys::ESC);
    assert!(panel.join_main_client().is_none());
}
