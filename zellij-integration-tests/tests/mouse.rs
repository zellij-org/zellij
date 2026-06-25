#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

fn start_zellij() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start()
}

fn sgr_mouse_report(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

#[test]
fn focus_pane_with_mouse() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&sgr_mouse_report(2, 5, 0));
    let grid_snapshot = zellij.wait_until("focus moved back to the left pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(3).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scrolling_inside_a_pane_with_mouse() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    for line in 1..=21 {
        right_terminal.output(format!("line{}\r\n", line).as_bytes());
    }
    zellij.wait_until("right pane filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains("line21")
    });

    zellij.send_stdin(&sgr_mouse_report(64, 2, 64));
    let grid_snapshot = zellij.wait_until("scrolled up inside the right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("SCROLL:")
            && grid_snapshot.contains("line1 ")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn pin_floating_panes() {
    let mut zellij = start_zellij();
    let tiled_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    zellij.wait_until("floating pane shows the unpinned button", |grid_snapshot| {
        grid_snapshot.contains("PIN [ ]")
    });

    zellij.send_stdin(&sgr_mouse_report(87, 8, 0));
    zellij.wait_until("floating pane became pinned", |grid_snapshot| {
        grid_snapshot.contains("PIN [+]")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    zellij.wait_until(
        "focus settled on the tiled pane while the pinned pane stays up",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("PIN [+]")
                && grid_snapshot.cursor_is_at(col(3).row(2))
        },
    );

    for line in 1..=13 {
        tiled_terminal.output(format!("line{}{}\r\n", line, "a".repeat(112)).as_bytes());
    }

    let grid_snapshot = zellij.wait_until(
        "pinned pane stays visible over the filled tiled pane",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("PIN [+]")
                && grid_snapshot.contains("line13")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
