#![cfg(unix)]

use zellij_integration_tests::{
    keys, FakePtyHandle, Size, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const PIN_MARKER: &str = "PIN [ ]";

fn start_zellij_with_startup_tip() -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("show_startup_tips true")
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    (zellij, terminal)
}

#[test]
fn hidden_startup_tip_stays_hidden_after_terminal_resize() {
    let (mut zellij, terminal) = start_zellij_with_startup_tip();
    zellij.wait_until("startup tip floating pane shown", |grid_snapshot| {
        grid_snapshot.contains(PIN_MARKER)
    });
    zellij.send_stdin(&keys::alt('f'));
    zellij.wait_until("floating panes hidden", |grid_snapshot| {
        !grid_snapshot.contains(PIN_MARKER) && grid_snapshot.status_bar_appears()
    });

    zellij.resize(Size {
        cols: 140,
        rows: 30,
    });
    terminal.wait_for_size(
        "terminal pane resized to the larger display",
        |cols, _rows| cols > 120,
    );
    zellij.wait_until("display settled at the new size", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("pane mode entered after the resize", |grid_snapshot| {
        grid_snapshot
            .lines()
            .last()
            .map_or(false, |last_line| last_line.contains("Fullscreen"))
    });
    zellij.send_stdin(&keys::ESC);
    let settled = zellij.wait_until("normal mode restored after the resize", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });

    assert!(
        !settled.contains(PIN_MARKER),
        "hidden startup tip floating pane must stay hidden after a terminal resize\n{}",
        settled.text,
    );
    zellij.quit();
}
