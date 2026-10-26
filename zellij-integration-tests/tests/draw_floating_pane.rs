#![cfg(unix)]

use zellij_integration_tests::{
    keys, FakePtyHandle, GridSnapshot, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::input::command::TerminalAction;

const BASE_CONFIG: &str = "mouse_mode true\npane_frame_style \"full\"";
const LEFT: u8 = 0;
const CTRL: u8 = 16;
const ALT: u8 = 8;
const DRAG: u8 = 32;
const ENABLE_MOUSE_REPORTING: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1006h";

fn sgr_press(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn sgr_motion(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button + DRAG, column, line).into_bytes()
}

fn sgr_release(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}m", button, column, line).into_bytes()
}

fn start_with_config(extra_config: &str) -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config(&format!("{}\n{}", BASE_CONFIG, extra_config))
        .start()
}

fn claim_framed_terminal(zellij: &TestSession, marker: &'static str) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    terminal.output(marker.as_bytes());
    zellij.wait_until("framed terminal rendered", move |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains(marker)
    });
    terminal
}

fn open_floating_terminal(zellij: &TestSession, marker: &'static str) -> FakePtyHandle {
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

fn enable_mouse_reporting(zellij: &TestSession, terminal: &FakePtyHandle) {
    terminal.output(ENABLE_MOUSE_REPORTING);
    terminal.output(b"REPORTING-ON");
    zellij.wait_until("the program turned mouse reporting on", |grid_snapshot| {
        grid_snapshot.contains("REPORTING-ON")
    });
}

fn cell(grid_snapshot: &GridSnapshot, line: usize, column: usize) -> Option<char> {
    grid_snapshot
        .lines()
        .get(line)
        .and_then(|row| row.chars().nth(column))
}

fn outline_is_drawn(
    grid_snapshot: &GridSnapshot,
    (start_column, start_line): (usize, usize),
    (end_column, end_line): (usize, usize),
) -> bool {
    cell(grid_snapshot, start_line - 1, start_column - 1) == Some('┌')
        && cell(grid_snapshot, start_line - 1, end_column - 1) == Some('┐')
        && cell(grid_snapshot, end_line - 1, start_column - 1) == Some('└')
        && cell(grid_snapshot, end_line - 1, end_column - 1) == Some('┘')
}

fn drag_without_releasing(
    zellij: &TestSession,
    modifiers: u8,
    start: (usize, usize),
    end: (usize, usize),
) {
    zellij.send_stdin(&sgr_press(start.0, start.1, LEFT + modifiers));
    zellij.send_stdin(&sgr_motion(end.0, end.1, LEFT + modifiers));
    zellij.wait_until("the outline follows the drag", move |grid_snapshot| {
        outline_is_drawn(grid_snapshot, start, end)
    });
}

fn release(zellij: &TestSession, modifiers: u8, end: (usize, usize)) {
    zellij.send_stdin(&sgr_release(end.0, end.1, LEFT + modifiers));
}

fn drag(zellij: &TestSession, modifiers: u8, start: (usize, usize), end: (usize, usize)) {
    drag_without_releasing(zellij, modifiers, start, end);
    release(zellij, modifiers, end);
}

fn floating_pane_count(grid_snapshot: &GridSnapshot) -> usize {
    grid_snapshot.text.matches("PIN [").count()
}

fn spawned_command(terminal: &FakePtyHandle) -> Option<String> {
    match terminal.terminal_action() {
        Some(TerminalAction::RunCommand(run_command)) => {
            Some(run_command.command.display().to_string())
        },
        _ => None,
    }
}

#[test]
fn a_ctrl_drag_opens_a_floating_terminal_that_fills_the_rectangle() {
    let mut zellij = start_with_config("");
    claim_framed_terminal(&zellij, "TILED-MARKER");

    drag(&zellij, CTRL, (11, 5), (71, 16));
    let drawn_terminal = zellij.expect_pty_spawn();
    let (cols, rows) =
        drawn_terminal.wait_for_size("the drawn pane has its size", |cols, _| cols > 0);
    assert_eq!((cols, rows), (59, 10));
    drawn_terminal.output(b"DRAWN-MARKER");
    let grid_snapshot = zellij.wait_until("the drawn pane is shown", |grid_snapshot| {
        grid_snapshot.contains("DRAWN-MARKER") && floating_pane_count(grid_snapshot) == 1
    });
    assert_eq!(cell(&grid_snapshot, 5, 11), Some('D'), "{}", grid_snapshot.text);
    zellij.quit();
}

#[test]
fn a_ctrl_drag_up_and_to_the_left_opens_the_same_rectangle() {
    let mut zellij = start_with_config("");
    claim_framed_terminal(&zellij, "TILED-MARKER");

    zellij.send_stdin(&sgr_press(71, 16, LEFT + CTRL));
    zellij.send_stdin(&sgr_motion(11, 5, LEFT + CTRL));
    zellij.wait_until("the outline follows the drag", |grid_snapshot| {
        outline_is_drawn(grid_snapshot, (11, 5), (71, 16))
    });
    release(&zellij, CTRL, (11, 5));
    let drawn_terminal = zellij.expect_pty_spawn();
    let size = drawn_terminal.wait_for_size("the drawn pane has its size", |cols, _| cols > 0);
    assert_eq!(size, (59, 10));
    zellij.quit();
}

#[test]
fn the_drawn_terminal_runs_the_configured_default_shell() {
    let mut zellij = start_with_config("default_shell \"/opt/drawn/shell\"");
    claim_framed_terminal(&zellij, "TILED-MARKER");

    drag(&zellij, CTRL, (11, 5), (71, 16));
    let drawn_terminal = zellij.expect_pty_spawn();
    assert_eq!(
        spawned_command(&drawn_terminal).as_deref(),
        Some("/opt/drawn/shell")
    );
    zellij.quit();
}

#[test]
fn a_drag_smaller_than_a_pane_opens_nothing_and_clears_the_outline() {
    let mut zellij = start_with_config("");
    let terminal = claim_framed_terminal(&zellij, "TILED-MARKER");

    drag(&zellij, CTRL, (11, 5), (14, 7));
    terminal.output(b"AFTER-SMALL-DRAG");
    let grid_snapshot = zellij.wait_until("the outline is gone", |grid_snapshot| {
        grid_snapshot.contains("AFTER-SMALL-DRAG")
            && !outline_is_drawn(grid_snapshot, (11, 5), (14, 7))
    });
    assert_eq!(floating_pane_count(&grid_snapshot), 0, "{}", grid_snapshot.text);

    drag(&zellij, CTRL, (11, 5), (71, 16));
    let drawn_terminal = zellij.expect_pty_spawn();
    drawn_terminal.output(b"DRAWN-MARKER");
    zellij.wait_until("only the second drag opened a pane", |grid_snapshot| {
        grid_snapshot.contains("DRAWN-MARKER") && floating_pane_count(grid_snapshot) == 1
    });
    zellij.quit();
}

#[test]
fn visible_floating_panes_stay_on_screen_while_drawing() {
    let mut zellij = start_with_config("");
    claim_framed_terminal(&zellij, "TILED-MARKER");
    open_floating_terminal(&zellij, "FLOAT-MARKER");
    let grid_snapshot = zellij.snapshot();
    let float_line = grid_snapshot
        .row_of_line("FLOAT-MARKER")
        .expect("floating pane is shown");
    let (start, end) = if float_line > 12 {
        ((3, 3), (40, 9))
    } else {
        ((3, 15), (40, 21))
    };

    drag_without_releasing(&zellij, CTRL, start, end);
    let grid_snapshot = zellij.snapshot();
    assert!(
        grid_snapshot.contains("FLOAT-MARKER"),
        "{}",
        grid_snapshot.text
    );
    release(&zellij, CTRL, end);
    let drawn_terminal = zellij.expect_pty_spawn();
    drawn_terminal.output(b"DRAWN-MARKER");
    zellij.wait_until("both floating panes are shown", |grid_snapshot| {
        grid_snapshot.contains("FLOAT-MARKER")
            && grid_snapshot.contains("DRAWN-MARKER")
            && floating_pane_count(grid_snapshot) == 2
    });
    zellij.quit();
}

#[test]
fn a_ctrl_drag_over_a_program_using_the_mouse_still_draws_a_pane() {
    let mut zellij = start_with_config("");
    let terminal = claim_framed_terminal(&zellij, "MOUSE-APP");
    enable_mouse_reporting(&zellij, &terminal);

    drag(&zellij, CTRL, (11, 5), (71, 16));
    let drawn_terminal = zellij.expect_pty_spawn();
    drawn_terminal.output(b"DRAWN-MARKER");
    zellij.wait_until("the drawn pane is shown", |grid_snapshot| {
        grid_snapshot.contains("DRAWN-MARKER")
    });
    let program_stdin = String::from_utf8_lossy(&terminal.stdin_bytes()).to_string();
    assert!(!program_stdin.contains("\u{1b}[<"), "{:?}", program_stdin);
    zellij.quit();
}

#[test]
fn a_ctrl_drag_goes_to_a_program_using_the_mouse_when_the_app_comes_first() {
    let mut zellij = start_with_config(
        "mousebinds {\n    shared {\n        bind \"Ctrl Left\" on=\"content\" app_first=true { DrawFloatingPane; }\n    }\n}",
    );
    let terminal = claim_framed_terminal(&zellij, "MOUSE-APP");
    enable_mouse_reporting(&zellij, &terminal);

    zellij.send_stdin(&sgr_press(11, 5, LEFT + CTRL));
    zellij.send_stdin(&sgr_motion(71, 16, LEFT + CTRL));
    zellij.send_stdin(&sgr_release(71, 16, LEFT + CTRL));
    terminal.wait_for_stdin("the release reached the program", |stdin| {
        let stdin = String::from_utf8_lossy(stdin);
        stdin.contains("\u{1b}[<16;") && stdin.ends_with('m')
    });
    terminal.output(b"AFTER-DRAG");
    let grid_snapshot = zellij.wait_until("output after the drag", |grid_snapshot| {
        grid_snapshot.contains("AFTER-DRAG")
    });
    assert_eq!(floating_pane_count(&grid_snapshot), 0, "{}", grid_snapshot.text);
    zellij.quit();
}

#[test]
fn a_drag_bound_to_a_plugin_opens_that_plugin_in_the_rectangle() {
    let mut zellij = start_with_config(
        "mousebinds {\n    shared {\n        bind \"Ctrl Alt Left\" { DrawFloatingPane \"zellij:strider\"; }\n    }\n}",
    );
    claim_framed_terminal(&zellij, "TILED-MARKER");

    drag(&zellij, CTRL + ALT, (11, 5), (71, 16));
    let grid_snapshot = zellij.wait_until("the plugin opens in a floating pane", |grid_snapshot| {
        grid_snapshot.contains("strider") && floating_pane_count(grid_snapshot) == 1
    });
    let title_line = grid_snapshot
        .row_of_line("strider")
        .expect("the plugin pane has a title");
    assert_eq!(title_line, 4, "{}", grid_snapshot.text);
    assert_eq!(cell(&grid_snapshot, 4, 10), Some('┌'), "{}", grid_snapshot.text);
    assert_eq!(cell(&grid_snapshot, 15, 70), Some('┘'), "{}", grid_snapshot.text);
    zellij.quit();
}
