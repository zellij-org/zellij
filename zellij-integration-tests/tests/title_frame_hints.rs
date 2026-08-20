#![cfg(unix)]

use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, split_right_and_wait_for_prompt, FakePtyHandle,
    GridSnapshot, Size, TestRunner, TestSession, PROMPT,
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

const TIPS_DISABLED_CONFIG: &str =
    "mouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true\nmouse_hover_tips false";

const FULL_FRAME_TIPS_DISABLED_CONFIG: &str =
    "pane_frame_style \"full\"\nmouse_mode true\nadvanced_mouse_actions true\nmouse_hover_effects true\nmouse_hover_tips false";

fn two_pane_session_with_config(config: &str) -> (TestSession, FakePtyHandle, FakePtyHandle) {
    let zellij = TestRunner::new(Size {
        cols: 120,
        rows: 24,
    })
    .with_config(config)
    .start();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    zellij.wait_until("two panes settled in locked base mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.tab_bar_appears()
    });
    (zellij, left_terminal, right_terminal)
}

fn full_frame_two_pane_session_with_handles(
    cols: usize,
    config: &str,
) -> (TestSession, FakePtyHandle, FakePtyHandle) {
    let zellij = TestRunner::new(Size { cols, rows: 24 })
        .with_config(config)
        .start();
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
    (zellij, left_terminal, right_terminal)
}

fn sgr_motion(column: usize, line: usize) -> Vec<u8> {
    format!("\u{1b}[<35;{};{}M", column, line).into_bytes()
}

fn tab_bar_line(grid_snapshot: &GridSnapshot) -> String {
    grid_snapshot.lines().first().cloned().unwrap_or_default()
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
fn hovering_a_pane_shows_a_hint_on_the_tab_bar() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "resize hint shown on the tab bar when hovering the focused pane",
        |grid_snapshot| tab_bar_line(grid_snapshot).contains("resize"),
    );

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "group hint shown on the tab bar when hovering the other pane",
        |grid_snapshot| tab_bar_line(grid_snapshot).contains("group"),
    );

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
    zellij.wait_until(
        "group hint shown when crossing into the other pane",
        |grid_snapshot| grid_snapshot.contains("group"),
    );

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "resize hint shown again when crossing back",
        |grid_snapshot| grid_snapshot.contains("resize"),
    );

    zellij.quit();
}

#[test]
fn full_frame_help_renders_on_the_pane_not_the_tab_bar() {
    let mut zellij = full_frame_two_pane_session(120);

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    let grid_snapshot = zellij.wait_until("resize help drawn on the pane frame", |grid_snapshot| {
        grid_snapshot.contains("to resize")
    });
    let tab_bar = tab_bar_line(&grid_snapshot);
    assert!(
        !tab_bar.contains("resize"),
        "in full-frame mode the resize help belongs on the pane frame, not the tab bar:\n{}",
        grid_snapshot.text
    );
    let status_bar = grid_snapshot.lines().last().cloned().unwrap();
    assert!(
        !status_bar.contains("resize"),
        "in full-frame mode the resize help belongs on the pane frame, not the status bar:\n{}",
        grid_snapshot.text
    );

    zellij.quit();
}

#[test]
fn resize_hint_shortens_when_the_tab_bar_narrows() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "full-width tab bar shows the long resize hint",
        |grid_snapshot| tab_bar_line(grid_snapshot).contains("to resize"),
    );

    zellij.resize(Size { cols: 68, rows: 24 });
    zellij.wait_until(
        "narrowed tab bar shortens the resize hint",
        |grid_snapshot| {
            let tab_bar = tab_bar_line(grid_snapshot);
            tab_bar.contains("drag borders") && !tab_bar.contains("to resize")
        },
    );

    zellij.resize(Size { cols: 50, rows: 24 });
    zellij.wait_until(
        "very narrow tab bar drops the hint and keeps the tabs",
        |grid_snapshot| !grid_snapshot.contains("drag borders") && grid_snapshot.tab_bar_appears(),
    );

    zellij.quit();
}

#[test]
fn hint_replaces_the_swap_layout_indicator_until_dismissed() {
    let mut zellij = two_pane_session();
    zellij.wait_until("swap layout indicator shown", |grid_snapshot| {
        tab_bar_line(grid_snapshot).contains("BASE")
    });

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "hint takes over the swap layout indicator slot",
        |grid_snapshot| {
            let tab_bar = tab_bar_line(grid_snapshot);
            tab_bar.contains("resize") && !tab_bar.contains("BASE")
        },
    );

    zellij.send_stdin(b"x");
    zellij.wait_until(
        "swap layout indicator returns after the hint is dismissed",
        |grid_snapshot| {
            let tab_bar = tab_bar_line(grid_snapshot);
            tab_bar.contains("BASE") && !tab_bar.contains("resize")
        },
    );

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
    zellij.wait_until(
        "resize hint cleared once a single pane remains",
        |grid_snapshot| {
            !grid_snapshot.contains("LEFT_PANE_MARKER")
                && grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("resize")
        },
    );

    zellij.quit();
}

#[test]
fn disabled_tips_suppress_the_group_tip_but_keep_the_hovered_frame_highlight() {
    let (mut zellij, _left_terminal, _right_terminal) =
        full_frame_two_pane_session_with_handles(120, FULL_FRAME_TIPS_DISABLED_CONFIG);

    let before_hover = zellij.wait_until("the non-focused pane frame settled", |grid_snapshot| {
        grid_snapshot.row_of_line("Pane #1").is_some()
    });
    let frame_row = before_hover.row_of_line("Pane #1").unwrap();
    let unhovered_frame_color = before_hover.cell_foreground(0, frame_row);

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    let hovered = zellij.wait_until(
        "the hovered non-focused pane frame is still highlighted",
        |grid_snapshot| grid_snapshot.cell_foreground(0, frame_row) != unhovered_frame_color,
    );

    assert!(
        !hovered.contains("group"),
        "mouse_hover_tips false must suppress the group tip on the pane frame:\n{}",
        hovered.text
    );
    assert!(
        !tab_bar_line(&hovered).contains("group"),
        "mouse_hover_tips false must suppress the group tip on the tab bar:\n{}",
        hovered.text
    );

    zellij.quit();
}

#[test]
fn disabled_tips_suppress_the_tab_bar_hint_when_hovering_a_non_focused_pane() {
    let (mut zellij, left_terminal, _right_terminal) =
        two_pane_session_with_config(TIPS_DISABLED_CONFIG);

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    left_terminal.output(b"HOVER_SYNC_MARKER");
    let settled = zellij.wait_until("a render settles after hovering", |grid_snapshot| {
        grid_snapshot.contains("HOVER_SYNC_MARKER")
    });

    assert!(
        !settled.contains("group"),
        "mouse_hover_tips false must suppress the group tip everywhere:\n{}",
        settled.text
    );
    assert!(
        !tab_bar_line(&settled).contains("resize"),
        "mouse_hover_tips false must suppress the resize tip on the tab bar:\n{}",
        settled.text
    );

    zellij.quit();
}

#[test]
fn disabled_tips_suppress_the_full_frame_resize_help() {
    let (mut zellij, left_terminal, _right_terminal) =
        full_frame_two_pane_session_with_handles(120, FULL_FRAME_TIPS_DISABLED_CONFIG);

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    left_terminal.output(b"RESIZE_SYNC_MARKER");
    let settled = zellij.wait_until(
        "a render settles after motion inside the focused pane",
        |grid_snapshot| grid_snapshot.contains("RESIZE_SYNC_MARKER"),
    );

    assert!(
        !settled.contains("to resize"),
        "mouse_hover_tips false must suppress the resize help undertitle:\n{}",
        settled.text
    );
    assert!(
        !settled.contains("drag borders"),
        "mouse_hover_tips false must suppress every resize help tier:\n{}",
        settled.text
    );

    zellij.quit();
}

#[test]
fn hints_are_shown_when_mouse_hover_tips_is_absent_from_the_config() {
    let mut zellij = two_pane_session();

    zellij.send_stdin(&sgr_motion(FOCUSED_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "resize hint shown by default when the option is absent",
        |grid_snapshot| tab_bar_line(grid_snapshot).contains("resize"),
    );

    zellij.send_stdin(&sgr_motion(OTHER_PANE_COLUMN, HOVER_LINE));
    zellij.wait_until(
        "group hint shown by default when the option is absent",
        |grid_snapshot| tab_bar_line(grid_snapshot).contains("group"),
    );

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
    let grid_snapshot = zellij
        .wait_until("fullscreen stays settled after hovering", |grid_snapshot| {
            grid_snapshot.contains("(FULLSCREEN)") && grid_snapshot.contains("LOCK")
        });
    assert!(
        !grid_snapshot.contains("resize"),
        "fullscreen must suppress the resize hint:\n{}",
        grid_snapshot.text
    );

    zellij.quit();
}
