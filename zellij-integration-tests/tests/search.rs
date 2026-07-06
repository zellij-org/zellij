#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, start_zellij, FakePtyHandle,
    TestRunner, TestSession, TERMINAL_SIZE,
};

fn start_zellij_with_full_frames() -> TestSession {
    TestRunner::new(TERMINAL_SIZE)
        .with_config("pane_frame_style \"full\"")
        .start()
}

fn enter_search_on_visible_match_with_full_frames(zellij: &TestSession) {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(b"NEEDLE on screen\r\n");
    zellij.wait_until("match visible on screen", |grid_snapshot| {
        grid_snapshot.contains("NEEDLE on screen")
    });
    enter_search_for_needle(zellij);
}

fn fill_pane_with_two_matches(zellij: &TestSession) -> FakePtyHandle {
    let terminal = claim_first_terminal_and_wait_for_prompt(zellij);
    for line in 1..=40 {
        let content = match line {
            5 => "line05 NEEDLE upper\r\n".to_string(),
            35 => "line35 NEEDLE lower\r\n".to_string(),
            other => format!("line{:02}\r\n", other),
        };
        terminal.output(content.as_bytes());
    }
    zellij.wait_until(
        "pane filled with the lower match visible",
        |grid_snapshot| {
            grid_snapshot.contains("NEEDLE lower") && !grid_snapshot.contains("NEEDLE upper")
        },
    );
    terminal
}

fn enter_search_for_needle(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("search input mode active", |grid_snapshot| {
        grid_snapshot.contains("ENTERING SEARCH TERM")
    });
    zellij.send_stdin(b"NEEDLE");
    zellij.send_stdin(&keys::ENTER);
    zellij.wait_until("search navigation mode active", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp")
    });
}

#[test]
fn search_input_jumps_to_a_match() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    for line in 1..=40 {
        let content = if line == 5 {
            "line05 NEEDLE\r\n".to_string()
        } else {
            format!("line{:02}\r\n", line)
        };
        terminal.output(content.as_bytes());
    }
    zellij.wait_until("upper match initially off screen", |grid_snapshot| {
        grid_snapshot.contains("line40") && !grid_snapshot.contains("line05")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("search input mode active", |grid_snapshot| {
        grid_snapshot.contains("ENTERING SEARCH TERM")
    });
    zellij.send_stdin(b"NEEDLE");

    let grid_snapshot = zellij.wait_until("scrolled to the typed match", |grid_snapshot| {
        grid_snapshot.contains("line05") && grid_snapshot.contains("ENTERING SEARCH TERM")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn search_up_to_previous_match() {
    let mut zellij = start_zellij();
    fill_pane_with_two_matches(&zellij);
    enter_search_for_needle(&zellij);

    zellij.send_stdin(&keys::key('p'));
    zellij.send_stdin(&keys::key('p'));

    let grid_snapshot =
        zellij.wait_until("upper match revealed by searching up", |grid_snapshot| {
            grid_snapshot.contains("NEEDLE upper") && grid_snapshot.contains("PgDn|PgUp")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn search_down_to_next_match() {
    let mut zellij = start_zellij();
    fill_pane_with_two_matches(&zellij);
    enter_search_for_needle(&zellij);

    zellij.send_stdin(&keys::key('p'));
    zellij.send_stdin(&keys::key('p'));
    zellij.wait_until("upper match revealed by searching up", |grid_snapshot| {
        grid_snapshot.contains("NEEDLE upper")
    });

    zellij.send_stdin(&keys::key('n'));

    let grid_snapshot =
        zellij.wait_until("lower match revealed by searching down", |grid_snapshot| {
            grid_snapshot.contains("NEEDLE lower")
                && !grid_snapshot.contains("NEEDLE upper")
                && grid_snapshot.contains("PgDn|PgUp")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn search_toggle_case_sensitivity() {
    let mut zellij = start_zellij_with_full_frames();
    enter_search_on_visible_match_with_full_frames(&zellij);

    zellij.send_stdin(&keys::key('c'));

    let grid_snapshot = zellij.wait_until("case sensitivity toggled on", |grid_snapshot| {
        grid_snapshot.contains("[c]")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn search_toggle_wrap() {
    let mut zellij = start_zellij_with_full_frames();
    enter_search_on_visible_match_with_full_frames(&zellij);

    zellij.send_stdin(&keys::key('w'));

    let grid_snapshot = zellij.wait_until("wrap toggled on", |grid_snapshot| {
        grid_snapshot.contains("[w]")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn search_toggle_whole_word() {
    let mut zellij = start_zellij_with_full_frames();
    enter_search_on_visible_match_with_full_frames(&zellij);

    zellij.send_stdin(&keys::key('o'));

    let grid_snapshot = zellij.wait_until("whole word toggled on", |grid_snapshot| {
        grid_snapshot.contains("[o]")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
