#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, split_down_and_wait_for_prompt,
    start_zellij, FakePtyHandle, TestSession,
};

const LAST_LINE: &str = "line40";

fn fill_pane_past_viewport(zellij: &TestSession) -> FakePtyHandle {
    let terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    for line in 1..=40 {
        terminal.output(format!("line{:02}\r\n", line).as_bytes());
    }
    zellij.wait_until("pane filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains(LAST_LINE)
    });
    terminal
}

#[test]
fn page_scroll_up() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::ctrl('b'));

    let grid_snapshot = zellij.wait_until("scrolled up by a page", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp")
            && !grid_snapshot.contains(LAST_LINE)
            && grid_snapshot.contains("SCROLL 19/")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn page_scroll_down() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::ctrl('b'));
    zellij.wait_until("scrolled up by a page", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp") && !grid_snapshot.contains(LAST_LINE)
    });

    zellij.send_stdin(&keys::ctrl('f'));

    let grid_snapshot =
        zellij.wait_until("scrolled back down toward the bottom", |grid_snapshot| {
            grid_snapshot.contains("line39")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("SCROLL 0/")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn half_page_scroll_up() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('u'));

    let grid_snapshot = zellij.wait_until("scrolled up by half a page", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp")
            && !grid_snapshot.contains(LAST_LINE)
            && grid_snapshot.contains("SCROLL 10/")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn half_page_scroll_down() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('u'));
    zellij.wait_until("scrolled up by half a page", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp") && !grid_snapshot.contains(LAST_LINE)
    });

    zellij.send_stdin(&keys::key('d'));

    let grid_snapshot =
        zellij.wait_until("scrolled back down toward the bottom", |grid_snapshot| {
            grid_snapshot.contains("line39")
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("SCROLL 0/")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scroll_down_one_line() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::key('k'));
    zellij.wait_until("scrolled up two lines", |grid_snapshot| {
        grid_snapshot.contains("SCROLL 2/")
    });

    zellij.send_stdin(&keys::key('j'));

    let grid_snapshot = zellij.wait_until("scrolled back down one line", |grid_snapshot| {
        grid_snapshot.contains("SCROLL 1/") && grid_snapshot.contains("PgDn|PgUp")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scroll_to_bottom() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::ctrl('b'));
    zellij.wait_until("scrolled up by a page", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp") && !grid_snapshot.contains(LAST_LINE)
    });

    zellij.send_stdin(&keys::ctrl('c'));

    let grid_snapshot =
        zellij.wait_until("returned to the bottom in normal mode", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains(LAST_LINE)
                && grid_snapshot.contains("SCROLL 0/")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn scroll_position_is_kept_when_leaving_scroll_mode() {
    let mut zellij = start_zellij();
    fill_pane_past_viewport(&zellij);

    zellij.send_stdin(&keys::ctrl('s'));
    for _ in 0..4 {
        zellij.send_stdin(&keys::key('k'));
    }
    zellij.wait_until("scrolled up four lines", |grid_snapshot| {
        grid_snapshot.contains("SCROLL 4/") && grid_snapshot.contains("PgDn|PgUp")
    });

    zellij.send_stdin(&keys::ctrl('s'));

    let grid_snapshot = zellij.wait_until(
        "normal mode restored with the viewport left where it was",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("SCROLL 4/")
                && !grid_snapshot.contains(LAST_LINE)
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn focusing_a_scrolled_pane_re_enters_scroll_mode_at_the_same_offset() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);

    for line in 1..=40 {
        upper_terminal.output(format!("upper{:02}\r\n", line).as_bytes());
    }
    zellij.wait_until("upper pane filled past its viewport", |grid_snapshot| {
        grid_snapshot.contains("upper40")
    });

    zellij.send_stdin(&keys::alt('k'));
    zellij.send_stdin(&keys::ctrl('s'));
    for _ in 0..4 {
        zellij.send_stdin(&keys::key('k'));
    }
    zellij.wait_until("upper pane scrolled up four lines", |grid_snapshot| {
        grid_snapshot.contains("SCROLL: 4/") && grid_snapshot.contains("PgDn|PgUp")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.wait_until("left scroll mode with the offset kept", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("SCROLL: 4/")
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until(
        "focusing the unscrolled pane returned the client to normal mode",
        |grid_snapshot| grid_snapshot.status_bar_appears() && !grid_snapshot.contains("PgDn|PgUp"),
    );

    zellij.send_stdin(&keys::alt('k'));

    let grid_snapshot = zellij.wait_until(
        "focusing the scrolled pane re-entered scroll mode at the same offset",
        |grid_snapshot| grid_snapshot.contains("PgDn|PgUp") && grid_snapshot.contains("SCROLL: 4/"),
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
