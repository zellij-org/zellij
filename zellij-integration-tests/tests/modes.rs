#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized, start_zellij, PROMPT,
};

#[test]
fn switch_to_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));

    let grid_snapshot = zellij.wait_until("pane mode active", |grid_snapshot| {
        grid_snapshot.contains("PANE") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_tab_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('t'));

    let grid_snapshot = zellij.wait_until("tab mode active", |grid_snapshot| {
        grid_snapshot.contains("TAB") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_resize_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('n'));

    let grid_snapshot = zellij.wait_until("resize mode active", |grid_snapshot| {
        grid_snapshot.contains("RESIZE") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_move_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('h'));

    let grid_snapshot = zellij.wait_until("move mode active", |grid_snapshot| {
        grid_snapshot.contains("MOVE") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_scroll_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));

    let grid_snapshot = zellij.wait_until("scroll mode active", |grid_snapshot| {
        grid_snapshot.contains("Edit") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_session_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('o'));

    let grid_snapshot = zellij.wait_until("session mode active", |grid_snapshot| {
        grid_snapshot.contains("Detach") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn enter_search_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('s'));

    let grid_snapshot = zellij.wait_until("search input mode active", |grid_snapshot| {
        grid_snapshot.contains("ENTERING SEARCH TERM") && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_to_search_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("search input mode active", |grid_snapshot| {
        grid_snapshot.contains("ENTERING SEARCH TERM")
    });
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("search navigation mode active", |grid_snapshot| {
        grid_snapshot.contains("SEARCHING")
            && grid_snapshot.contains("Case")
            && !grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn return_to_normal_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.wait_until("pane mode active", |grid_snapshot| {
        grid_snapshot.contains("PANE") && !grid_snapshot.contains("LOCK")
    });

    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("returned to normal mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn lock_mode() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface locked", |grid_snapshot| {
        grid_snapshot.contains("LOCK") && !grid_snapshot.contains("PANE")
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    zellij.send_stdin(b"abc");

    terminal.wait_for_stdin("forwarded keys reached the pane", |stdin_bytes| {
        stdin_bytes.windows(3).any(|window| window == b"abc")
    });
    let grid_snapshot =
        zellij.wait_until("forwarded keys rendered in locked pane", |grid_snapshot| {
            grid_snapshot.contains("abc")
                && !grid_snapshot.contains("PANE")
                && !grid_snapshot.contains("Tab #2")
        });
    assert_snapshot!(normalized(&grid_snapshot));

    zellij.send_stdin(&keys::ctrl('g'));
    zellij.wait_until("interface unlocked", |grid_snapshot| {
        grid_snapshot.contains("PANE")
    });
    zellij.quit();
}

#[test]
fn tmux_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('b'));
    zellij.send_stdin(&keys::key('%'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("right terminal opened via tmux mode", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn bracketed_paste() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::BRACKETED_PASTE_START);
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    zellij.send_stdin(b"abc");
    zellij.send_stdin(&keys::BRACKETED_PASTE_END);

    terminal.wait_for_stdin("pasted text reached the pane", |stdin_bytes| {
        stdin_bytes.windows(3).any(|window| window == b"abc")
    });
    let grid_snapshot = zellij.wait_until("pasted text rendered, no new tab", |grid_snapshot| {
        grid_snapshot.contains("abc")
            && grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Tab #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scrolling_inside_a_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);
    zellij.wait_until("right terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });

    for line in 1..=22 {
        right_terminal.output(format!("line{}\r\n", line).as_bytes());
    }
    zellij.wait_until("right terminal filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains("line22")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::key('k'));

    let grid_snapshot =
        zellij.wait_until("scrolled up one line inside the pane", |grid_snapshot| {
            grid_snapshot.contains("SCROLL: 2/2") && grid_snapshot.contains("PgDn|PgUp")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
