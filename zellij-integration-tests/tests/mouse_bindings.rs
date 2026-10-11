#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, FakePtyHandle, GridSnapshot, TestRunner,
    TestSession, PROMPT, TERMINAL_SIZE,
};

const FULL_FRAMES_CONFIG: &str = "mouse_mode true\npane_frame_style \"full\"";
const LOCKED_PASS_THROUGH_CONFIG: &str =
    "mouse_mode true\nmousebinds {\n    locked clear-defaults=true {\n    }\n}";
const COMMON_MENU_MARKER: &str = "Session manager";
const ENABLE_MOUSE_REPORTING: &[u8] = b"\x1b[?1000h\x1b[?1006h";

fn sgr_press(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn sgr_release(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}m", button, column, line).into_bytes()
}

fn click(zellij: &TestSession, column: usize, line: usize, button: u8) {
    zellij.send_stdin(&sgr_press(column, line, button));
    zellij.send_stdin(&sgr_release(column, line, button));
}

fn double_click(zellij: &TestSession, column: usize, line: usize) {
    click(zellij, column, line, 0);
    click(zellij, column, line, 0);
}

fn column_of(grid_snapshot: &GridSnapshot, row: usize, needle: &str) -> Option<usize> {
    let line = grid_snapshot.lines().get(row)?.clone();
    let byte_index = line.find(needle)?;
    Some(line[..byte_index].chars().count())
}

fn start_with_full_frames() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(FULL_FRAMES_CONFIG)
        .start()
}

fn claim_framed_terminal(zellij: &TestSession, marker: &'static str) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(marker.as_bytes());
    zellij.wait_until("framed terminal rendered", move |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(marker)
    });
    terminal
}

fn split_right_framed(zellij: &TestSession, marker: &'static str) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(marker.as_bytes());
    zellij.wait_until("right terminal rendered", move |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains(marker)
    });
    terminal
}

fn open_floating_framed(zellij: &TestSession, marker: &'static str) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(marker.as_bytes());
    zellij.wait_until("floating terminal rendered", move |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(marker)
            && grid_snapshot.contains("PIN [")
    });
    terminal
}

fn double_click_frame_title(zellij: &TestSession, title: &str, offset: usize) {
    let grid_snapshot = zellij.snapshot();
    let row = grid_snapshot
        .row_of_line(title)
        .unwrap_or_else(|| panic!("{} is not on screen:\n{}", title, grid_snapshot.text));
    let column = column_of(&grid_snapshot, row, title).unwrap();
    double_click(zellij, column + 1 + offset, row + 1);
}

fn is_wide(cols: u16) -> bool {
    cols > 100
}

#[test]
fn double_clicking_a_tiled_pane_frame_toggles_fullscreen() {
    let mut zellij = start_with_full_frames();
    let left_terminal = claim_framed_terminal(&zellij, "LEFT-MARKER");
    split_right_framed(&zellij, "RIGHT-MARKER");
    let (split_cols, _) =
        left_terminal.wait_for_size("left pane is split", |cols, _| !is_wide(cols));
    assert!(!is_wide(split_cols));

    double_click_frame_title(&zellij, "Pane #1", 2);
    left_terminal.wait_for_size("left pane went fullscreen", |cols, _| is_wide(cols));
    zellij.wait_until("only the fullscreen pane is shown", |grid_snapshot| {
        grid_snapshot.contains("LEFT-MARKER") && !grid_snapshot.contains("RIGHT-MARKER")
    });

    double_click_frame_title(&zellij, "Pane #1", 5);
    left_terminal.wait_for_size("left pane left fullscreen", |cols, _| !is_wide(cols));
    zellij.wait_until("both panes are shown again", |grid_snapshot| {
        grid_snapshot.contains("LEFT-MARKER") && grid_snapshot.contains("RIGHT-MARKER")
    });
    zellij.quit();
}

#[test]
fn double_clicking_a_floating_pane_frame_toggles_fullscreen() {
    let mut zellij = start_with_full_frames();
    claim_framed_terminal(&zellij, "TILED-MARKER");
    let floating_terminal = open_floating_framed(&zellij, "FLOAT-MARKER");
    floating_terminal.wait_for_size("floating pane has its own size", |cols, _| !is_wide(cols));

    double_click_frame_title(&zellij, "Pane #2", 2);
    floating_terminal.wait_for_size("floating pane went fullscreen", |cols, _| is_wide(cols));
    zellij.wait_until("the floating pane covers the screen", |grid_snapshot| {
        grid_snapshot.contains("FLOAT-MARKER") && !grid_snapshot.contains("TILED-MARKER")
    });

    double_click_frame_title(&zellij, "Pane #2", 5);
    floating_terminal.wait_for_size("floating pane left fullscreen", |cols, _| !is_wide(cols));
    zellij.wait_until("the tiled pane shows again", |grid_snapshot| {
        grid_snapshot.contains("FLOAT-MARKER") && grid_snapshot.contains("TILED-MARKER")
    });
    zellij.quit();
}

fn lock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });
}

fn unlock_interface(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface unlocked", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });
}

#[test]
fn cleared_locked_mouse_bindings_pass_clicks_and_scrolls_to_the_program() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(LOCKED_PASS_THROUGH_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(ENABLE_MOUSE_REPORTING);
    terminal.output(b"MOUSE-APP");
    zellij.wait_until("program output rendered", |grid_snapshot| {
        grid_snapshot.contains("MOUSE-APP")
    });
    lock_interface(&zellij);

    click(&zellij, 30, 10, 0);
    terminal.wait_for_stdin("left click reported to the program", |stdin| {
        String::from_utf8_lossy(stdin).contains("\u{1b}[<0;")
    });
    zellij.send_stdin(&sgr_press(30, 10, 64));
    terminal.wait_for_stdin("scroll up reported to the program", |stdin| {
        String::from_utf8_lossy(stdin).contains("\u{1b}[<64;")
    });
    click(&zellij, 30, 10, 2);
    terminal.wait_for_stdin("right click reported to the program", |stdin| {
        String::from_utf8_lossy(stdin).contains("\u{1b}[<2;")
    });
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains(COMMON_MENU_MARKER),
        "{}",
        grid_snapshot.text
    );
    assert!(!grid_snapshot.contains("SCROLL:"), "{}", grid_snapshot.text);
    zellij.kill_session();
}

#[test]
fn cleared_locked_mouse_bindings_stop_right_click_from_opening_the_menu() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(LOCKED_PASS_THROUGH_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    lock_interface(&zellij);

    click(&zellij, 30, 10, 2);
    terminal.output(b"AFTER-LOCKED-CLICK");
    zellij.wait_until("output after the click rendered", |grid_snapshot| {
        grid_snapshot.contains("AFTER-LOCKED-CLICK")
    });
    unlock_interface(&zellij);
    let grid_snapshot = zellij.snapshot();
    assert!(
        !grid_snapshot.contains(COMMON_MENU_MARKER),
        "{}",
        grid_snapshot.text
    );

    click(&zellij, 30, 10, 2);
    zellij.wait_until("normal mode right click opens the menu", |grid_snapshot| {
        grid_snapshot.contains(COMMON_MENU_MARKER)
    });
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("menu closed", |grid_snapshot| {
        !grid_snapshot.contains(COMMON_MENU_MARKER) && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

fn start_with_mouse() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
}

#[test]
fn double_clicking_a_tab_name_starts_renaming_the_tab() {
    let mut zellij = start_with_mouse();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.snapshot();
    let tab_column = column_of(&grid_snapshot, 0, "Tab #1").expect("tab bar shows Tab #1");

    double_click(&zellij, tab_column + 2, 1);
    zellij.wait_until("rename tab mode entered", |grid_snapshot| {
        grid_snapshot.contains("RENAMING TAB")
    });
    zellij.send_stdin(b"Renamed");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("the tab shows its new name", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .is_some_and(|tab_bar| tab_bar.contains("Renamed") && !tab_bar.contains("Tab #1"))
            && grid_snapshot.status_bar_appears()
    });
    assert!(!String::from_utf8_lossy(&terminal.stdin_bytes()).contains("Renamed"));
    zellij.quit();
}

#[test]
fn double_clicking_empty_tab_bar_space_opens_a_new_tab() {
    let mut zellij = start_with_mouse();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let grid_snapshot = zellij.snapshot();
    let tab_bar = grid_snapshot.lines()[0].clone();
    assert!(
        tab_bar.chars().nth(80).map_or(true, |c| c == ' '),
        "{}",
        tab_bar
    );

    double_click(&zellij, 81, 1);
    let new_tab_terminal = zellij.expect_pty_spawn();
    new_tab_terminal.output(PROMPT);
    zellij.wait_until("a second tab opened", |grid_snapshot| {
        grid_snapshot.contains("Tab #1")
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}
