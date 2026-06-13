#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{keys, normalized, FakePtyHandle, Size, TestRunner, TestSession};

const TERMINAL_SIZE: Size = Size {
    cols: 120,
    rows: 24,
};
const PROMPT: &[u8] = b"$ ";
const PROMPT_ROW: usize = 2;
const FIRST_PANE_PROMPT_X: usize = 3;
const RIGHT_PANE_PROMPT_X: usize = 63;

const CHANGED_KEYS_CONFIG: &str = r#"
keybinds clear-defaults=true {
    normal {
        bind "F1" { SwitchToMode "Locked"; }
        bind "F2" { SwitchToMode "Pane"; }
        bind "F3" { SwitchToMode "Tab"; }
        bind "F4" { SwitchToMode "Resize"; }
        bind "F5" { SwitchToMode "Move"; }
        bind "F6" { SwitchToMode "Scroll"; }
        bind "Alt F7" { SwitchToMode "Session"; }
        bind "Ctrl F8" { Quit; }
    }
}
"#;

fn claim_first_terminal_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until(
        "first terminal prompt rendered in loaded app",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW)
        },
    );
    terminal
}

fn split_right_and_wait_for_prompt(zellij: &TestSession) -> FakePtyHandle {
    zellij.send_stdin(&keys::PANE_MODE);
    zellij.send_stdin(&keys::SPLIT_RIGHT_IN_PANE_MODE);
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.cursor_is_at(RIGHT_PANE_PROMPT_X, PROMPT_ROW)
    });
    terminal
}

#[test]
fn exit_zellij() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::QUIT);

    zellij.wait_until("zellij said goodbye", |grid_snapshot| {
        !grid_snapshot.status_bar_appears() && grid_snapshot.contains("Bye from Zellij!")
    });
}

#[test]
fn resize_terminal_window() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    zellij.resize(Size {
        cols: 100,
        rows: 24,
    });
    right_terminal.wait_for_size("right terminal reacted to window resize", |cols, _| {
        cols < (TERMINAL_SIZE.cols / 2) as u16
    });
    first_terminal.wait_for_size("left terminal reacted to window resize", |cols, _| {
        cols < (TERMINAL_SIZE.cols / 2) as u16
    });

    let grid_snapshot = zellij.wait_until("app re-rendered at the new window size", |grid_snapshot| {
        grid_snapshot.contains("Ctrl +")
            && grid_snapshot.tab_bar_appears()
            && grid_snapshot.cursor_is_at(53, PROMPT_ROW)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn status_bar_loads_custom_keybindings() {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(CHANGED_KEYS_CONFIG)
        .start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("status bar reflects the custom keybindings", |grid_snapshot| {
        grid_snapshot.cursor_is_at(FIRST_PANE_PROMPT_X, PROMPT_ROW) && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
}
