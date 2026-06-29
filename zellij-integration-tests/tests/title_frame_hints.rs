#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, FakePtyHandle,
    Size, TestRunner, TestSession, PROMPT,
};

const FOCUSED_PANE_COLUMN: usize = 90;
const ALTERNATE_FOCUSED_PANE_COLUMN: usize = 100;
const OTHER_PANE_COLUMN: usize = 10;
const HOVER_LINE: usize = 12;

fn start_zellij_with_mouse() -> TestSession {
    TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .with_config("mouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true")
    .start()
}

fn start_full_frame_zellij_with_mouse(cols: usize) -> TestSession {
    TestRunner::new(Size { cols, rows: 24 })
        .with_config(
            "pane_frame_style \"full\"\nmouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true",
        )
        .start()
}

fn sgr_motion(column: usize, line: usize) -> Vec<u8> {
    format!("\u{1b}[<35;{};{}M", column, line).into_bytes()
}

fn two_pane_session_with_handles() -> (TestSession, FakePtyHandle, FakePtyHandle) {
    let zellij = start_zellij_with_mouse();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    zellij.wait_until("two panes settled in locked base mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    (zellij, left_terminal, right_terminal)
}

fn two_pane_session() -> TestSession {
    two_pane_session_with_handles().0
}

fn full_frame_two_pane_session(cols: usize) -> TestSession {
    let zellij = start_full_frame_zellij_with_mouse(cols);
    let left_terminal = zellij.expect_pty_spawn();
    left_terminal.output(PROMPT);
    zellij.wait_until("first full-frame pane rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("┌")
            && grid_snapshot.contains("$ ")
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("two full-frame panes rendered", |grid_snapshot| {
        grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.status_bar_appears()
    });
    zellij
}

#[test]
fn hovering_a_pane_shows_a_hint_on_the_status_bar() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("resize hint shown when hovering the focused pane", |grid_snapshot| {
        grid_snapshot.contains("resize")
    });

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("group hint shown when hovering the other pane", |grid_snapshot| {
        grid_snapshot.contains("group")
    });

    zellij.quit();
}

#[test]
fn any_input_dismisses_the_hint() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("resize hint shown", |grid_snapshot| {
        grid_snapshot.contains("resize")
    });

    zellij.send_stdin(b"x");
    zellij.wait_until("hint cleared by input", |grid_snapshot| {
        !grid_snapshot.contains("resize") && !grid_snapshot.contains("group")
    });

    zellij.quit();
}

#[test]
fn hint_only_re_fires_when_entering_a_different_pane() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("resize hint shown on first entry", |grid_snapshot| {
        grid_snapshot.contains("resize")
    });

    zellij.send_stdin(b"x");
    zellij.wait_until("hint dismissed by input", |grid_snapshot| {
        !grid_snapshot.contains("resize") && !grid_snapshot.contains("group")
    });

    zellij.send_stdin(&sgr_motion(ALTERNATE_FOCUSED_PANE_COLUMN, HOVER_LINE));
    let after_within_pane_motion = zellij.wait_until(
        "a render settles after within-pane motion",
        |grid_snapshot| grid_snapshot.status_bar_appears(),
    );
    assert!(
        !after_within_pane_motion.contains("resize"),
        "within-pane motion must not re-show the resize hint:\n{}",
        after_within_pane_motion.text
    );

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("group hint shown when crossing into the other pane", |grid_snapshot| {
        grid_snapshot.contains("group")
    });

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("resize hint shown again when crossing back", |grid_snapshot| {
        grid_snapshot.contains("resize")
    });

    zellij.quit();
}

#[test]
fn full_frame_help_renders_on_the_pane_not_the_status_bar() {
    let mut zellij = full_frame_two_pane_session(120);

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    let grid_snapshot = zellij.wait_until("resize help drawn on the pane frame", |grid_snapshot| {
        grid_snapshot.contains("to resize")
    });
    let status_bar = grid_snapshot.lines().last().cloned().unwrap();
    assert!(
        !status_bar.contains("resize"),
        "in full-frame mode the resize help belongs on the pane frame, not the status bar:\n{}",
        grid_snapshot.text
    );

    zellij.quit();
}

#[test]
fn resize_hint_shortens_when_the_status_bar_narrows() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("full-width status bar shows the long resize hint", |grid_snapshot| {
        grid_snapshot.contains("to resize")
    });

    zellij.resize(Size {
        cols: 46,
        rows: 24,
    });
    zellij.wait_until("narrowed status bar shortens the resize hint", |grid_snapshot| {
        grid_snapshot.contains("drag borders") && !grid_snapshot.contains("to resize")
    });

    zellij.quit();
}

#[test]
fn resize_hint_absent_with_a_single_pane() {
    let (mut zellij, left_terminal, _right_terminal) = two_pane_session_with_handles();
    left_terminal.output(b"LEFT_PANE_MARKER");
    zellij.wait_until("the other pane is populated", |grid_snapshot| {
        grid_snapshot.contains("LEFT_PANE_MARKER")
    });

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until("resize hint shown with two panes", |grid_snapshot| {
        grid_snapshot.contains("resize")
    });

    left_terminal.exit(Some(0));
    zellij.wait_until("resize hint cleared once a single pane remains", |grid_snapshot| {
        !grid_snapshot.contains("LEFT_PANE_MARKER")
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("resize")
    });

    zellij.quit();
}

#[test]
fn resize_hint_absent_in_fullscreen() {
    let (mut zellij, _left_terminal, _right_terminal) = two_pane_session_with_handles();

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("focused pane entered fullscreen", |grid_snapshot| {
        grid_snapshot.contains("(FULLSCREEN)") && grid_snapshot.contains("LOCK")
    });

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.send_stdin(&sgr_motion(ALTERNATE_FOCUSED_PANE_COLUMN, HOVER_LINE));
    let grid_snapshot = zellij.wait_until("fullscreen stays settled after hovering", |grid_snapshot| {
        grid_snapshot.contains("(FULLSCREEN)") && grid_snapshot.contains("LOCK")
    });
    assert!(
        !grid_snapshot.contains("resize"),
        "fullscreen must suppress the resize hint:\n{}",
        grid_snapshot.text
    );

    zellij.quit();
}
