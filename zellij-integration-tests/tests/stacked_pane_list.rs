#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_down_and_wait_for_prompt, start_zellij, FakePtyHandle, GridSnapshot, LayoutInfo, Size,
    TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::input::command::TerminalAction;

const STACKED_LAYOUT: &str = r#"
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="tab-bar"
        }
        children
        pane size=1 borderless=true {
            plugin location="status-bar"
        }
    }
    tab {
        pane stacked=true {
            pane
            pane
            pane
        }
    }
}
"#;

const STACK_WITH_COMMAND_MEMBER_LAYOUT: &str = r#"
layout {
    default_tab_template {
        pane size=1 borderless=true {
            plugin location="tab-bar"
        }
        children
        pane size=1 borderless=true {
            plugin location="status-bar"
        }
    }
    tab {
        pane stacked=true {
            pane
            pane command="member-command"
        }
    }
}
"#;

fn add_stacked_pane_and_wait_for_selected_entry(
    zellij: &TestSession,
    selected_entry_title: &str,
) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    let bracketed_entry = format!("[ {} ]", selected_entry_title);
    zellij.wait_until("stacked pane added and selected", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains(&bracketed_entry)
    });
    terminal
}

fn build_three_member_stack(zellij: &TestSession) -> [FakePtyHandle; 3] {
    let first_member = claim_first_terminal_and_wait_for_prompt(zellij);
    first_member.output(b"first-member\r\n$ ");
    zellij.wait_until("first member content rendered", |grid_snapshot| {
        grid_snapshot.contains("first-member")
    });
    let second_member = add_stacked_pane_and_wait_for_selected_entry(zellij, "Pane #2");
    second_member.output(b"second-member\r\n$ ");
    zellij.wait_until("second member content rendered", |grid_snapshot| {
        grid_snapshot.contains("second-member")
    });
    let third_member = add_stacked_pane_and_wait_for_selected_entry(zellij, "Pane #3");
    third_member.output(b"third-member\r\n$ ");
    zellij.wait_until("third member content rendered", |grid_snapshot| {
        grid_snapshot.contains("third-member")
    });
    [first_member, second_member, third_member]
}

fn sgr_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<0;{};{}M\u{1b}[<0;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn sgr_alt_left_click(column: usize, line: usize) -> Vec<u8> {
    format!(
        "\u{1b}[<8;{};{}M\u{1b}[<8;{};{}m",
        column, line, column, line
    )
    .into_bytes()
}

fn display_column_of(line: &str, needle: &str) -> Option<usize> {
    line.find(needle)
        .map(|byte_offset| line[..byte_offset].chars().count())
}

fn entry_click_coordinates(grid_snapshot: &GridSnapshot, entry_title: &str) -> (usize, usize) {
    let lines = grid_snapshot.lines();
    let (line_index, line) = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.contains(entry_title))
        .expect("entry title is on a header row");
    let column = display_column_of(line, entry_title).expect("entry title is on the line") + 1;
    (column, line_index + 1)
}

#[test]
fn stack_renders_as_title_list_above_the_visible_pane() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    let grid_snapshot = zellij.wait_until("stack list settled", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("[ Pane #3 ]")
            && grid_snapshot.contains("third-member")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_layout_stack_renders_as_a_title_list() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(STACKED_LAYOUT.to_string()))
        .start();
    for _ in 0..3 {
        let member = zellij.expect_pty_spawn();
        member.output(PROMPT);
    }

    let grid_snapshot = zellij.wait_until("layout stack rendered as a list", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("[ Pane #1 ]")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("Pane #3")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn disabling_stacked_pane_list_restores_the_classic_stack() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("stacked_pane_list false")
        .start();
    let first_member = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_member.output(b"first-member\r\n$ ");

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    let second_member = zellij.expect_pty_spawn();
    second_member.output(PROMPT);

    let grid_snapshot = zellij.wait_until("classic stack rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("[ Pane #2 ]")
            && grid_snapshot.cursor_is_at(col(2).row(3))
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn up_and_down_move_the_selection_through_the_list() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::alt('k'));
    let middle_member_selected = zellij.wait_until("middle member swapped in", |grid_snapshot| {
        grid_snapshot.contains("<g> LOCK")
            && grid_snapshot.contains("[ Pane #2 ]")
            && grid_snapshot.contains("second-member")
            && !grid_snapshot.contains("[ Pane #3 ]")
    });
    assert_snapshot!(normalized(&middle_member_selected));

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("top member swapped in", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #1 ]") && grid_snapshot.contains("first-member")
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until("selection moved back down", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]") && grid_snapshot.contains("second-member")
    });
    zellij.quit();
}

#[test]
fn navigating_past_the_top_entry_leaves_the_stack() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let _lower_terminal = split_down_and_wait_for_prompt(&zellij);
    add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #3");
    add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #4");

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("selection on the middle entry", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #3 ]")
    });
    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("selection on the top entry", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]")
    });

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("focus left the stack to the pane above", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]") && grid_snapshot.cursor_is_at(col(2).row(2))
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until("focus re-entered the stack", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]") && grid_snapshot.cursor_is_at(col(2).row(15))
    });
    zellij.quit();
}

#[test]
fn closing_the_visible_member_promotes_its_neighbor() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));

    let grid_snapshot =
        zellij.wait_until("third member closed, second promoted", |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("Pane #3")
                && grid_snapshot.contains("[ Pane #2 ]")
                && grid_snapshot.contains("second-member")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_stack_list_dissolves_into_a_normal_pane_at_one_member() {
    let mut zellij = start_zellij();
    let first_member = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_member.output(b"first-member\r\n$ ");
    add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #2");

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('x'));

    let grid_snapshot = zellij.wait_until("list dissolved to a single pane", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Pane #2")
            && !grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("first-member")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_hidden_member_exiting_removes_its_entry() {
    let mut zellij = start_zellij();
    let [_first_member, second_member, _third_member] = build_three_member_stack(&zellij);

    second_member.exit(Some(0));

    let grid_snapshot = zellij.wait_until("hidden member removed from the list", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("[ Pane #3 ]")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn breaking_the_visible_member_out_promotes_and_moves_it_to_a_new_tab() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('b'));
    zellij.wait_until("visible member moved to a new tab", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Tab #2")
            && grid_snapshot.contains("third-member")
            && !grid_snapshot.contains("Pane #1")
    });

    zellij.send_stdin(&keys::alt('h'));
    let grid_snapshot = zellij.wait_until("stack shrank to two members", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("[ Pane #2 ]")
            && !grid_snapshot.contains("Pane #3")
            && grid_snapshot.contains("second-member")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn floating_the_visible_member_promotes_its_neighbor() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('e'));

    let grid_snapshot = zellij.wait_until("member floated over the stack", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("[ Pane #2 ]")
            && grid_snapshot.contains("Pane #3")
            && grid_snapshot.contains("third-member")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn fullscreen_suspends_the_list_and_restores_it() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("stack member fullscreened", |grid_snapshot| {
        grid_snapshot.contains("(FULLSCREEN)") && !grid_snapshot.contains("[ Pane #3 ]")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    let grid_snapshot = zellij.wait_until("list restored after fullscreen", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("(FULLSCREEN)")
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("[ Pane #3 ]")
            && grid_snapshot.contains("third-member")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn clicking_a_hidden_entry_swaps_it_in() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true")
        .start();
    build_three_member_stack(&zellij);

    let grid_snapshot = zellij.wait_until("stack list rendered", |grid_snapshot| {
        grid_snapshot.contains("Pane #1") && grid_snapshot.contains("[ Pane #3 ]")
    });
    let (entry_column, entry_line) = entry_click_coordinates(&grid_snapshot, "Pane #1");
    zellij.send_stdin(&sgr_left_click(entry_column, entry_line));

    let swapped_in = zellij.wait_until("clicked member swapped in", |grid_snapshot| {
        grid_snapshot.contains("<g> LOCK")
            && grid_snapshot.contains("[ Pane #1 ]")
            && grid_snapshot.contains("first-member")
    });
    assert_snapshot!(normalized(&swapped_in));
    zellij.quit();
}

#[test]
fn alt_clicking_a_hidden_entry_marks_it_without_selecting() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("mouse_mode true\nadvanced_mouse_actions true")
        .start();
    build_three_member_stack(&zellij);

    let grid_snapshot = zellij.wait_until("stack list rendered", |grid_snapshot| {
        grid_snapshot.contains("Pane #1") && grid_snapshot.contains("[ Pane #3 ]")
    });
    let (entry_column, entry_line) = entry_click_coordinates(&grid_snapshot, "Pane #1");
    zellij.send_stdin(&sgr_alt_left_click(entry_column, entry_line));

    zellij.wait_until(
        "hidden member marked, selection unchanged",
        |grid_snapshot| {
            grid_snapshot.contains("GROUP ACTIONS") && grid_snapshot.contains("[ Pane #3 ]")
        },
    );
    zellij.quit();
}

#[test]
fn synced_input_reaches_hidden_members() {
    let mut zellij = start_zellij();
    let members = build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("tab marked as syncing", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("SYNC")
    });

    zellij.send_stdin(&keys::ENTER);
    zellij.send_stdin(b"synced-input");

    for member in &members {
        member.wait_for_stdin("synced input reached member", |stdin| {
            stdin.windows(12).any(|window| window == b"synced-input")
        });
    }
    zellij.quit();
}

#[test]
fn entries_share_a_uniform_width_sized_by_the_widest_title() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(
            STACK_WITH_COMMAND_MEMBER_LAYOUT.to_string(),
        ))
        .start();
    for _ in 0..2 {
        let member = zellij.expect_pty_spawn();
        member.output(PROMPT);
    }
    let grid_snapshot = zellij.wait_until("mixed-width entries rendered", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("member-command")
            && grid_snapshot.contains("[ Pane #1")
    });

    let lines = grid_snapshot.lines();
    let selected_entry_line = lines
        .iter()
        .find(|line| line.contains("[ Pane #1"))
        .expect("selected entry is on a header row");
    let hidden_entry_line = lines
        .iter()
        .find(|line| line.contains("member-command"))
        .expect("hidden entry is on a header row");
    let selected_title_column =
        display_column_of(selected_entry_line, "Pane #1").expect("selected title is on the line");
    let hidden_title_column = display_column_of(hidden_entry_line, "member-command")
        .expect("hidden title is on the line");
    assert_eq!(selected_title_column, hidden_title_column);

    let widest_title_width = "member-command".chars().count();
    let closing_bracket_column =
        display_column_of(selected_entry_line, "]").expect("selected entry has a closing bracket");
    assert_eq!(
        closing_bracket_column,
        selected_title_column + widest_title_width + 1
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_hidden_held_members_exit_code_shows_on_its_entry() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::Stringified(
            STACK_WITH_COMMAND_MEMBER_LAYOUT.to_string(),
        ))
        .start();
    let first_spawned = zellij.expect_pty_spawn();
    let second_spawned = zellij.expect_pty_spawn();
    let command_member = [&first_spawned, &second_spawned]
        .into_iter()
        .find(|terminal| match terminal.terminal_action() {
            Some(TerminalAction::RunCommand(run_command)) => run_command
                .command
                .to_string_lossy()
                .contains("member-command"),
            _ => false,
        })
        .expect("one member runs a command")
        .clone();
    zellij.wait_until(
        "stack list rendered with the command entry",
        |grid_snapshot| {
            grid_snapshot.status_bar_appears()
                && grid_snapshot.contains("member-command")
                && grid_snapshot.contains("[ Pane #1")
        },
    );

    command_member.exit(Some(2));

    let grid_snapshot =
        zellij.wait_until("exit code rendered on the hidden entry", |grid_snapshot| {
            grid_snapshot.contains("EXIT CODE: 2") && grid_snapshot.contains("[ Pane #1")
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn the_visible_members_scroll_state_shows_on_its_entry() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    let second_member = add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #2");
    let mut scrollback = String::new();
    for line_number in 0..40 {
        scrollback.push_str(&format!("line{}\r\n", line_number));
    }
    second_member.output(scrollback.as_bytes());
    zellij.wait_until("scrollback filled", |grid_snapshot| {
        grid_snapshot.contains("line39")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::ctrl('b'));

    let grid_snapshot = zellij.wait_until("scroll indicator on the entry row", |grid_snapshot| {
        grid_snapshot.contains("SCROLL: ")
            && !grid_snapshot.contains("SCROLL: 0/")
            && grid_snapshot.contains("[ Pane #2 ]")
            && grid_snapshot.contains("PgDn|PgUp")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn hidden_members_follow_whole_tab_resizes() {
    let mut zellij = start_zellij();
    let [first_member, second_member, third_member] = build_three_member_stack(&zellij);

    zellij.resize(Size {
        cols: 100,
        rows: 20,
    });

    let (resized_cols, resized_rows) =
        third_member.wait_for_size("visible member resized", |cols, _rows| cols <= 100);
    for hidden_member in [&first_member, &second_member] {
        hidden_member.wait_for_size("hidden member resized with the tab", |cols, rows| {
            (cols, rows) == (resized_cols, resized_rows)
        });
    }
    zellij.quit();
}

fn build_two_member_stack_with_scrollback_editor() -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("scrollback_editor \"fake-editor\"")
        .start();
    let first_member = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_member.output(b"first-member\r\n$ ");
    let second_member = add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #2");
    second_member.output(b"second-member\r\n$ ");
    zellij.wait_until("second member content rendered", |grid_snapshot| {
        grid_snapshot.contains("second-member")
    });

    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('e'));
    let editor = zellij.expect_pty_spawn();
    editor.output(b"editor-open");
    zellij.wait_until(
        "scrollback editor opened on the visible member",
        |grid_snapshot| grid_snapshot.contains("editor-open"),
    );
    (zellij, editor)
}

#[test]
fn a_scrollback_editor_follows_its_member_through_selection_changes() {
    let (mut zellij, editor) = build_two_member_stack_with_scrollback_editor();

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("editor member swapped out", |grid_snapshot| {
        grid_snapshot.contains("first-member") && !grid_snapshot.contains("editor-open")
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until("editor member swapped back in", |grid_snapshot| {
        grid_snapshot.contains("editor-open")
    });

    editor.exit(Some(0));
    zellij.wait_until("editor closed back into its member", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]")
            && grid_snapshot.contains("second-member")
            && !grid_snapshot.contains("editor-open")
    });
    zellij.quit();
}

#[test]
fn a_hidden_scrollback_editor_pair_survives_fullscreen() {
    let (mut zellij, editor) = build_two_member_stack_with_scrollback_editor();

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("editor member swapped out", |grid_snapshot| {
        grid_snapshot.contains("first-member") && !grid_snapshot.contains("editor-open")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("member fullscreened over the list", |grid_snapshot| {
        grid_snapshot.contains("(FULLSCREEN)")
    });
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('f'));
    zellij.wait_until("list re-formed after fullscreen", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #1") && grid_snapshot.contains("first-member")
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until("editor member swapped back in", |grid_snapshot| {
        grid_snapshot.contains("editor-open")
    });

    editor.exit(Some(0));
    zellij.wait_until("editor closed back into its member", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]") && grid_snapshot.contains("second-member")
    });
    zellij.quit();
}

#[test]
fn closing_a_hidden_editor_member_substitutes_its_parked_pane() {
    let (mut zellij, editor) = build_two_member_stack_with_scrollback_editor();

    zellij.send_stdin(&keys::alt('k'));
    zellij.wait_until("editor member swapped out", |grid_snapshot| {
        grid_snapshot.contains("first-member") && !grid_snapshot.contains("editor-open")
    });

    editor.exit(Some(0));
    zellij.wait_until("parked pane substituted on the entry", |grid_snapshot| {
        grid_snapshot.contains("Pane #2") && !grid_snapshot.contains("fake-editor")
    });

    zellij.send_stdin(&keys::alt('j'));
    zellij.wait_until("substituted member swapped in", |grid_snapshot| {
        grid_snapshot.contains("[ Pane #2 ]") && grid_snapshot.contains("second-member")
    });
    zellij.quit();
}

#[test]
fn splitting_a_new_pane_reforms_the_list_around_it() {
    let mut zellij = start_zellij();
    build_three_member_stack(&zellij);

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('d'));
    let new_pane = zellij.expect_pty_spawn();
    new_pane.output(PROMPT);

    let grid_snapshot = zellij.wait_until("list re-formed above the new pane", |grid_snapshot| {
        grid_snapshot.contains("<g> LOCK")
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("[ Pane #3 ]")
            && grid_snapshot.contains("Pane #4")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn resizing_the_visible_member_keeps_the_list() {
    let mut zellij = start_zellij();
    let first_member = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_member.output(b"first-member\r\n$ ");
    let _lower_terminal = split_down_and_wait_for_prompt(&zellij);
    add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #3");
    let before_resize = zellij.wait_until("list settled before resize", |grid_snapshot| {
        grid_snapshot.contains("<g> LOCK") && grid_snapshot.contains("[ Pane #3 ]")
    });
    let entry_row_before = before_resize
        .lines()
        .iter()
        .position(|line| line.contains("[ Pane #3 ]"))
        .expect("selected entry is on a header row");

    zellij.send_stdin(&keys::ctrl('n'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ctrl('n'));

    let grid_snapshot = zellij.wait_until("stack region grew upward", move |grid_snapshot| {
        grid_snapshot.contains("<g> LOCK")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot
                .lines()
                .iter()
                .position(|line| line.contains("[ Pane #3 ]"))
                .map_or(false, |entry_row| entry_row < entry_row_before)
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn moving_the_visible_member_reorders_the_list() {
    let mut zellij = start_zellij();
    let first_member = claim_first_terminal_and_wait_for_prompt(&zellij);
    first_member.output(b"first-member\r\n$ ");
    let _lower_terminal = split_down_and_wait_for_prompt(&zellij);
    let third_member = add_stacked_pane_and_wait_for_selected_entry(&zellij, "Pane #3");
    third_member.output(b"third-member\r\n$ ");
    zellij.wait_until("third member content rendered", |grid_snapshot| {
        grid_snapshot.contains("third-member")
    });

    zellij.send_stdin(&keys::ctrl('h'));
    zellij.send_stdin(&keys::key('k'));
    zellij.send_stdin(&keys::ctrl('h'));

    let grid_snapshot =
        zellij.wait_until("visible member moved above its neighbor", |grid_snapshot| {
            let lines = grid_snapshot.lines();
            let row_of = |needle: &str| lines.iter().position(|line| line.contains(needle));
            grid_snapshot.contains("<g> LOCK")
                && grid_snapshot.contains("third-member")
                && matches!(
                    (row_of("[ Pane #3 ]"), row_of("Pane #2")),
                    (Some(selected_row), Some(hidden_row)) if selected_row < hidden_row
                )
        });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_stack_list_survives_session_resurrection() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_serialization true")
        .start();
    build_three_member_stack(&zellij);

    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();

    zellij.resurrect(TERMINAL_SIZE);
    for _ in 0..3 {
        let member = zellij.expect_pty_spawn();
        member.output(PROMPT);
    }

    let grid_snapshot = zellij.wait_until("stack list resurrected", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1")
            && grid_snapshot.contains("Pane #2")
            && grid_snapshot.contains("[ Pane #3 ]")
    });
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}
