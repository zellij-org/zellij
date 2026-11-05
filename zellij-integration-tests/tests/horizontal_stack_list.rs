#![cfg(unix)]

use insta::assert_snapshot;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, keys, normalized, GridSnapshot, LayoutInfo, Size,
    TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};

const ROW_OF_THREE_TITLES: &str = "Pane #1 │ Pane #2 │ Pane #3";
const COLUMN_ENTRY_MARKER: &str = "> │ ";
const MENU_ITEM_TO_ROW: &str = "Show titles in a row";
const MENU_ITEM_TO_COLUMN: &str = "Show titles in a column";
const PANE_MENU_MARKER: &str = "Close pane";

fn stacked_layout(stack_properties: &str, member_count: usize, focused_member: usize) -> String {
    let members: String = (1..=member_count)
        .map(|member| {
            if member == focused_member {
                "            pane focus=true\n".to_owned()
            } else {
                "            pane\n".to_owned()
            }
        })
        .collect();
    format!(
        r#"
layout {{
    default_tab_template {{
        pane size=1 borderless=true {{
            plugin location="tab-bar"
        }}
        children
        pane size=1 borderless=true {{
            plugin location="status-bar"
        }}
    }}
    tab {{
        pane stacked=true {} {{
{}        }}
    }}
}}
"#,
        stack_properties, members
    )
}

fn start_with_stack(
    size: Size,
    config: &str,
    stack_properties: &str,
    member_count: usize,
    focused_member: usize,
) -> TestSession {
    let zellij = TestRunner::new(size)
        .with_config(config)
        .with_layout(LayoutInfo::Stringified(stacked_layout(
            stack_properties,
            member_count,
            focused_member,
        )))
        .start();
    for member in 1..=member_count {
        let terminal = zellij.expect_pty_spawn();
        terminal.output(format!("member-{}-content\r\n$ ", member).as_bytes());
    }
    zellij
}

fn start_with_three_member_row(config: &str) -> TestSession {
    let zellij = start_with_stack(
        TERMINAL_SIZE,
        config,
        r#"stack_orientation="horizontal""#,
        3,
        1,
    );
    wait_for_row_with_selected(&zellij, "Pane #1");
    zellij
}

fn title_is_underlined(grid_snapshot: &GridSnapshot, title: &str) -> bool {
    let Some((column, row)) = grid_snapshot.position_of(title) else {
        return false;
    };
    (column..column + title.chars().count()).all(|x| grid_snapshot.cell_style(x, row).underline)
}

fn title_is_selected_in_row(grid_snapshot: &GridSnapshot, title: &str) -> bool {
    grid_snapshot.contains(ROW_OF_THREE_TITLES)
        && title_is_underlined(grid_snapshot, title)
        && ["Pane #1", "Pane #2", "Pane #3"]
            .iter()
            .filter(|other| **other != title)
            .all(|other| !title_is_underlined(grid_snapshot, other))
}

fn wait_for_row_with_selected(zellij: &TestSession, title: &str) -> GridSnapshot {
    let member_content = format!("member-{}-content", &title["Pane #".len()..]);
    zellij.wait_until("titles shown in a row", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && title_is_selected_in_row(grid_snapshot, title)
            && grid_snapshot.contains(&member_content)
            && !grid_snapshot.contains(COLUMN_ENTRY_MARKER)
    })
}

fn wait_for_column_with_selected(zellij: &TestSession, title: &str) -> GridSnapshot {
    let selected_entry = format!("{}{}", COLUMN_ENTRY_MARKER, title);
    zellij.wait_until("titles shown in a column", |grid_snapshot| {
        grid_snapshot.contains(&selected_entry) && !grid_snapshot.contains(ROW_OF_THREE_TITLES)
    })
}

fn sgr_press(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}M", button, column, line).into_bytes()
}

fn sgr_release(column: usize, line: usize, button: u8) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}m", button, column, line).into_bytes()
}

fn click(zellij: &TestSession, column: usize, line: usize, button: u8) {
    zellij.send_stdin(&sgr_press(column, line, button));
    zellij.send_stdin(&sgr_release(column, line, button));
}

fn click_on_text(zellij: &TestSession, grid_snapshot: &GridSnapshot, text: &str, button: u8) {
    let (column, row) = grid_snapshot
        .position_of(text)
        .unwrap_or_else(|| panic!("{} is not on screen", text));
    click(zellij, column + 2, row + 1, button);
}

#[test]
fn a_layout_stack_with_horizontal_orientation_shows_its_titles_in_one_row() {
    let mut zellij = start_with_three_member_row("");
    let grid_snapshot = wait_for_row_with_selected(&zellij, "Pane #1");
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_row_of_titles_that_fits_is_centered() {
    let mut zellij = start_with_three_member_row("");
    let grid_snapshot = wait_for_row_with_selected(&zellij, "Pane #1");
    let (row_start, row) = grid_snapshot.position_of(ROW_OF_THREE_TITLES).unwrap();
    let row_end = row_start + ROW_OF_THREE_TITLES.chars().count();
    let line = &grid_snapshot.lines()[row];
    let space_before = row_start;
    let space_after = TERMINAL_SIZE.cols - row_end;
    assert!(
        space_before.abs_diff(space_after) <= 1,
        "before={} after={} line={:?}",
        space_before,
        space_after,
        line
    );
    zellij.quit();
}

#[test]
fn moving_focus_left_and_right_moves_the_selection_through_the_row() {
    let mut zellij = start_with_three_member_row("");

    zellij.send_stdin(&keys::alt('l'));
    wait_for_row_with_selected(&zellij, "Pane #2");
    zellij.send_stdin(&keys::alt('l'));
    wait_for_row_with_selected(&zellij, "Pane #3");
    zellij.send_stdin(&keys::alt('h'));
    wait_for_row_with_selected(&zellij, "Pane #2");
    zellij.quit();
}

#[test]
fn shift_s_in_pane_mode_switches_a_stack_between_a_column_and_a_row() {
    let mut zellij = start_with_stack(TERMINAL_SIZE, "", "", 3, 1);
    wait_for_column_with_selected(&zellij, "Pane #1");

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('S'));
    wait_for_row_with_selected(&zellij, "Pane #1");

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('S'));
    wait_for_column_with_selected(&zellij, "Pane #1");
    zellij.quit();
}

#[test]
fn the_config_option_makes_new_stacks_show_their_titles_in_a_row() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(r#"stacked_pane_list_orientation "horizontal""#)
        .start();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    let second_terminal = zellij.expect_pty_spawn();
    second_terminal.output(PROMPT);

    zellij.wait_until("new stack shows its titles in a row", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("Pane #1 │ Pane #2")
            && title_is_underlined(grid_snapshot, "Pane #2")
            && !grid_snapshot.contains(COLUMN_ENTRY_MARKER)
    });
    zellij.quit();
}

#[test]
fn clicking_a_title_in_the_row_swaps_its_pane_in() {
    let mut zellij = start_with_three_member_row("mouse_mode true");
    let grid_snapshot = wait_for_row_with_selected(&zellij, "Pane #1");

    click_on_text(&zellij, &grid_snapshot, "Pane #3", 0);
    wait_for_row_with_selected(&zellij, "Pane #3");
    zellij.quit();
}

#[test]
fn right_clicking_the_titles_offers_to_show_them_in_a_column() {
    let mut zellij = start_with_three_member_row("mouse_mode true");
    let grid_snapshot = wait_for_row_with_selected(&zellij, "Pane #1");

    click_on_text(&zellij, &grid_snapshot, "Pane #2", 2);
    let menu = zellij.wait_until("pane menu opened on the titles", |grid_snapshot| {
        grid_snapshot.contains(PANE_MENU_MARKER) && grid_snapshot.contains(MENU_ITEM_TO_COLUMN)
    });
    assert!(!menu.contains(MENU_ITEM_TO_ROW));

    click_on_text(&zellij, &menu, MENU_ITEM_TO_COLUMN, 0);
    wait_for_column_with_selected(&zellij, "Pane #1");
    zellij.quit();
}

#[test]
fn right_clicking_column_titles_offers_to_show_them_in_a_row() {
    let mut zellij = start_with_stack(TERMINAL_SIZE, "mouse_mode true", "", 3, 1);
    let grid_snapshot = wait_for_column_with_selected(&zellij, "Pane #1");

    click_on_text(&zellij, &grid_snapshot, "Pane #3", 2);
    let menu = zellij.wait_until("pane menu opened on the titles", |grid_snapshot| {
        grid_snapshot.contains(PANE_MENU_MARKER) && grid_snapshot.contains(MENU_ITEM_TO_ROW)
    });
    click_on_text(&zellij, &menu, MENU_ITEM_TO_ROW, 0);
    wait_for_row_with_selected(&zellij, "Pane #1");
    zellij.quit();
}

#[test]
fn right_clicking_the_pane_content_does_not_offer_the_title_toggle() {
    let mut zellij = start_with_three_member_row("mouse_mode true");
    let grid_snapshot = wait_for_row_with_selected(&zellij, "Pane #1");

    click_on_text(&zellij, &grid_snapshot, "member-1-content", 2);
    let menu = zellij.wait_until("pane menu opened on the content", |grid_snapshot| {
        grid_snapshot.contains(PANE_MENU_MARKER)
    });
    assert!(!menu.contains(MENU_ITEM_TO_ROW));
    assert!(!menu.contains(MENU_ITEM_TO_COLUMN));
    zellij.send_stdin(&keys::ESC);
    zellij.wait_until("pane menu closed", |grid_snapshot| {
        !grid_snapshot.contains(PANE_MENU_MARKER) && grid_snapshot.status_bar_appears()
    });
    zellij.quit();
}

#[test]
fn a_row_that_does_not_fit_keeps_the_selected_title_in_the_middle() {
    let narrow = Size { cols: 50, rows: 24 };
    let mut zellij = start_with_stack(narrow, "", r#"stack_orientation="horizontal""#, 9, 5);

    let grid_snapshot = zellij.wait_until("selected title shown between hidden ones", |g| {
        g.contains("member-5-content")
            && title_is_underlined(g, "Pane #5")
            && g.contains("‹")
            && g.contains("›")
    });
    let (column, row) = grid_snapshot.position_of("Pane #5").unwrap();
    let title_middle = column * 2 + "Pane #5".chars().count();
    assert!(
        title_middle.abs_diff(narrow.cols) <= 2,
        "title middle (doubled) {} line {:?}",
        title_middle,
        grid_snapshot.lines()[row]
    );
    assert_eq!(
        grid_snapshot.lines()[row].trim_end(),
        "‹3         Pane #4 │ Pane #5 │ Pane #6          3›"
    );
    zellij.quit();
}

#[test]
fn a_row_scrolled_to_its_first_title_starts_at_the_left_edge() {
    let narrow = Size { cols: 50, rows: 24 };
    let mut zellij = start_with_stack(narrow, "", r#"stack_orientation="horizontal""#, 9, 1);

    let grid_snapshot = zellij.wait_until("first title selected in a long row", |g| {
        g.contains("member-1-content") && title_is_underlined(g, "Pane #1") && g.contains("›")
    });
    assert!(!grid_snapshot.contains("‹"));
    let (column, row) = grid_snapshot.position_of("Pane #1").unwrap();
    assert_eq!(column, 1, "line {:?}", grid_snapshot.lines()[row]);
    zellij.quit();
}
