#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_down_and_wait_for_prompt, split_right_and_wait_for_prompt, start_zellij, Coord, Size,
    TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

#[test]
fn cannot_split_terminals_vertically_when_active_terminal_is_too_small() {
    let mut zellij = TestRunner::new(Size { cols: 8, rows: 20 }).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(1))
    });
    let (width_before_split, _) = terminal.wait_for_size("first terminal sized", |_, _| true);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));

    terminal.output(b"done");
    let tab_bar_glyph = '\u{e0b0}';
    let grid_snapshot = zellij.wait_until(
        "split attempt processed, chrome rendered",
        |grid_snapshot| {
            grid_snapshot.contains("done")
                && grid_snapshot.contains("Ctrl +")
                && grid_snapshot
                    .lines()
                    .first()
                    .is_some_and(|first_line| first_line.contains(tab_bar_glyph))
        },
    );

    assert_eq!(terminal.size().unwrap().0, width_before_split);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn key_after_mode_switch_is_interpreted_in_new_mode_not_leaked_to_pane() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));

    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("split-right interpreted in pane mode", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(62).row(2))
    });

    assert!(!first_terminal.stdin_bytes().contains(&keys::key('r')[0]));
    zellij.quit();
}

#[test]
fn toggle_pane_fullscreen() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));

    let grid_snapshot = zellij.wait_until("focused pane is fullscreen", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(2))
            && grid_snapshot.contains("LOCK")
            && grid_snapshot.contains("(FULLSCREEN)")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn close_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));

    let grid_snapshot =
        zellij.wait_until("right pane closed, focus back on first", |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn closing_last_pane_exits_zellij() {
    let zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));

    zellij.wait_until(
        "zellij exited after closing the last pane",
        |grid_snapshot| grid_snapshot.contains("Bye from Zellij!"),
    );
}

#[test]
fn pane_closes_when_its_process_exits() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);

    right_terminal.exit(Some(0));

    let grid_snapshot =
        zellij.wait_until("right pane closed, focus back on first", |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(1))
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_floating_panes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until("floating pane appeared", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(33).row(8))
            && grid_snapshot.contains("STAGGERED")
            && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn undo_rename_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('c'));
    zellij.send_stdin(b"aa");
    zellij.send_stdin(&keys::ESC);
    zellij.send_stdin(&keys::ESC);

    let grid_snapshot = zellij.wait_until("pane name reverted to default", |grid_snapshot| {
        grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("aa")
            && grid_snapshot.contains("LOCK")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

fn spawn_second_pane_and_wait_for_grid(
    zellij: &TestSession,
    prompt: Coord,
) -> zellij_integration_tests::GridSnapshot {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until(
        "second pane spawned and prompt rendered",
        move |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("Pane #2")
                && grid_snapshot.cursor_is_at(prompt)
        },
    )
}

fn appears_in_order(
    grid_snapshot: &zellij_integration_tests::GridSnapshot,
    labels: &[&str],
) -> bool {
    let mut search_from = 0;
    for label in labels {
        match grid_snapshot.text[search_from..].find(label) {
            Some(offset) => search_from += offset + label.len(),
            None => return false,
        }
    }
    true
}

#[test]
fn move_focus_up_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus moved to the upper pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_focus_down_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_down_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('k'));
    zellij.wait_until("focus moved to the upper pane", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(2))
    });

    zellij.send_stdin(&keys::key('j'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus moved back to the lower pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(13))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_pane_swaps_with_neighbor() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    left_terminal.output(b"AAA");
    right_terminal.output(b"BBB");
    zellij.wait_until("both panes labelled left to right", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["AAA", "BBB"])
    });

    zellij.send_stdin(&keys::ctrl('h'));
    zellij.send_stdin(&keys::key('n'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focused pane swapped to the left", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["BBB", "AAA"]) && grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_pane_backwards_swaps_with_neighbor() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    left_terminal.output(b"AAA");
    right_terminal.output(b"BBB");
    zellij.wait_until("both panes labelled left to right", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["AAA", "BBB"])
    });

    zellij.send_stdin(&keys::ctrl('h'));
    zellij.send_stdin(&keys::key('p'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focused pane swapped backwards", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["BBB", "AAA"]) && grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_pane_left() {
    let mut zellij = start_zellij();
    let left_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right_terminal = split_right_and_wait_for_prompt(&zellij);
    left_terminal.output(b"AAA");
    right_terminal.output(b"BBB");
    zellij.wait_until("both panes labelled left to right", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["AAA", "BBB"])
    });

    zellij.send_stdin(&keys::ctrl('h'));
    zellij.send_stdin(&keys::key('h'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focused pane moved to the left", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["BBB", "AAA"]) && grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_pane_down() {
    let mut zellij = start_zellij();
    let upper_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    let lower_terminal = split_down_and_wait_for_prompt(&zellij);
    upper_terminal.output(b"AAA");
    lower_terminal.output(b"BBB");
    zellij.wait_until("both panes labelled top to bottom", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["AAA", "BBB"])
    });

    zellij.send_stdin(&keys::ctrl('h'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focused pane moved upward", |grid_snapshot| {
        appears_in_order(grid_snapshot, &["BBB", "AAA"]) && grid_snapshot.status_bar_appears()
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_pane_embed_or_floating() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('e'));

    let grid_snapshot = zellij.wait_until("focused pane became floating", |grid_snapshot| {
        grid_snapshot.contains("STAGGERED")
            && grid_snapshot.contains("LOCK")
            && grid_snapshot.cursor_is_at(col(33).row(8))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_pane_pinned() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    zellij.wait_until("floating pane appeared", |grid_snapshot| {
        grid_snapshot.contains("STAGGERED")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('i'));

    let grid_snapshot = zellij.wait_until("floating pane pinned", |grid_snapshot| {
        grid_snapshot.contains("PIN [+]")
            && grid_snapshot.contains("LOCK")
            && grid_snapshot.cursor_is_at(col(33).row(8))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_focus_or_tab_moves_focus_between_panes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('h'));

    let grid_snapshot = zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_focus_or_tab_switches_tab_when_single_pane() {
    let mut zellij = start_zellij();
    let first_terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_terminal.output(b"oneone");
    zellij.wait_until("first tab pane labelled", |grid_snapshot| {
        grid_snapshot.contains("oneone")
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);
    zellij.wait_until("second tab focused, first tab hidden", |grid_snapshot| {
        grid_snapshot.contains("Tab #2") && !grid_snapshot.contains("oneone")
    });

    zellij.send_stdin(&keys::alt('h'));

    let grid_snapshot = zellij
        .wait_until("focus switched back to the first tab", |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.contains("oneone")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn new_pane_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('n'));
    let grid_snapshot = spawn_second_pane_and_wait_for_grid(&zellij, col(62).row(2));
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn new_pane_in_normal_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::alt('n'));
    let grid_snapshot = spawn_second_pane_and_wait_for_grid(&zellij, col(62).row(2));
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn split_pane_downward() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let grid_snapshot = spawn_second_pane_and_wait_for_grid(&zellij, col(2).row(13));
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn new_stacked_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    let grid_snapshot = spawn_second_pane_and_wait_for_grid(&zellij, col(2).row(3));
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_focus_left_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('h'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn move_focus_right_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('h'));
    zellij.wait_until("focus moved to the left pane", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(2))
    });

    zellij.send_stdin(&keys::key('l'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus moved back to the right pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn switch_focus_in_pane_mode() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('p'));
    zellij.send_stdin(&keys::ENTER);

    let grid_snapshot = zellij.wait_until("focus switched to the other pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(2).row(2))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

fn cycle_pane_frames(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('z'));
}

#[test]
fn toggle_pane_frames() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);

    zellij.wait_until("tiled panes start in titles mode", |grid_snapshot| {
        grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("┌")
    });

    cycle_pane_frames(&zellij);
    zellij.wait_until("titles cycle to frameless", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Pane #1")
            && !grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("┌")
            && grid_snapshot.cursor_is_at(col(62).row(1))
    });

    cycle_pane_frames(&zellij);
    zellij.wait_until("frameless cycles to full frames", |grid_snapshot| {
        grid_snapshot.contains("┌")
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
    });

    cycle_pane_frames(&zellij);
    let grid_snapshot = zellij.wait_until("full frames cycle back to titles", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("┌")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_frames_with_single_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.wait_until(
        "lone pane in titles mode omits its title",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("Pane #1")
                && !grid_snapshot.contains("┌")
        },
    );

    cycle_pane_frames(&zellij);
    zellij.wait_until("lone frameless pane stays untitled", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Pane #1")
            && !grid_snapshot.contains("┌")
    });

    cycle_pane_frames(&zellij);
    let grid_snapshot =
        zellij.wait_until("lone pane gains a full frame and title", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("┌")
                && grid_snapshot.contains("Pane #1")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_frames_with_floating_pane() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating_terminal = zellij.expect_pty_spawn();
    floating_terminal.output(PROMPT);
    zellij.wait_until("floating pane appeared with a frame", |grid_snapshot| {
        grid_snapshot.contains("STAGGERED") && grid_snapshot.contains("┌")
    });

    cycle_pane_frames(&zellij);
    zellij.wait_until(
        "floating pane keeps its frame in frameless mode",
        |grid_snapshot| grid_snapshot.status_bar_appears() && grid_snapshot.contains("┌"),
    );

    cycle_pane_frames(&zellij);
    let grid_snapshot = zellij.wait_until(
        "floating pane keeps its frame in full mode",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("┌")
                && grid_snapshot.contains("Pane #1")
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn toggle_frames_with_stacked_panes() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    spawn_second_pane_and_wait_for_grid(&zellij, col(2).row(3));

    zellij.wait_until("stacked panes show their titles", |grid_snapshot| {
        grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("┌")
    });

    cycle_pane_frames(&zellij);
    zellij.wait_until("stacked panes cycle to frameless", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && !grid_snapshot.contains("┌")
    });

    cycle_pane_frames(&zellij);
    let grid_snapshot = zellij.wait_until("stacked panes cycle to full frames", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("┌")
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn start_without_pane_frames() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("pane_frames false")
        .start();
    let first_terminal = zellij.expect_pty_spawn();
    first_terminal.output(PROMPT);
    zellij.wait_until(
        "first frameless terminal prompt rendered",
        |grid_snapshot| {
            grid_snapshot.tab_bar_appears()
                && grid_snapshot.status_bar_appears()
                && grid_snapshot.cursor_is_at(col(2).row(1))
        },
    );

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let right_terminal = zellij.expect_pty_spawn();
    right_terminal.output(PROMPT);

    let grid_snapshot = zellij.wait_until(
        "right frameless terminal prompt rendered",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears() && grid_snapshot.cursor_is_at(col(62).row(1))
        },
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
