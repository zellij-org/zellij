#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, Size, TestRunner, PROMPT, TERMINAL_SIZE,
};

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

#[test]
fn exit_zellij() {
    let zellij = TestRunner::new(TERMINAL_SIZE).start();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('q'));

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

    let grid_snapshot =
        zellij.wait_until("app re-rendered at the new window size", |grid_snapshot| {
            grid_snapshot.contains("Ctrl +")
                && grid_snapshot.tab_bar_appears()
                && grid_snapshot.cursor_is_at(col(53).row(2))
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

    let grid_snapshot = zellij.wait_until(
        "status bar reflects the custom keybindings",
        |grid_snapshot| grid_snapshot.cursor_is_at(col(3).row(2)) && grid_snapshot.contains("LOCK"),
    );
    assert_snapshot!(normalized(&grid_snapshot));
}

#[test]
fn use_custom_layout_with_relative_path() {
    let layout_dir = format!(
        "{}/../src/tests/fixtures/config-dirs/e2e-upside-down/layouts",
        env!("CARGO_MANIFEST_DIR")
    );
    let config = format!(
        "layout_dir \"{}\"\ndefault_layout \"upside-down\"\n",
        layout_dir
    );
    let mut zellij = TestRunner::new(TERMINAL_SIZE).with_config(&config).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("upside-down layout loaded from disk", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.contains("Zellij (test")
            && grid_snapshot.cursor.is_some()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn edit_scrollback() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE).start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.output(b"SCROLLBACK_MARKER_ONE\r\nSCROLLBACK_MARKER_TWO\r\n");
    zellij.wait_until("scrollback content rendered", |grid_snapshot| {
        grid_snapshot.contains("SCROLLBACK_MARKER_TWO")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('e'));
    zellij.expect_pty_spawn();

    let grid_snapshot = zellij.wait_until("scrollback editor pane open", |grid_snapshot| {
        grid_snapshot.contains("EDITING SCROLLBACK")
            && grid_snapshot.contains("SCROLLBACK_MARKER_ONE")
            && grid_snapshot.contains("SCROLLBACK_MARKER_TWO")
            && grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
