#![cfg(unix)]

use zellij_integration_tests::attention::needs_you;
use zellij_integration_tests::inbox::{
    background_session_with, open_inbox_on, ATTENTION_CONFIG, INBOX_HELP, SESSIONS_HELP,
};
use zellij_integration_tests::{
    col, keys, FakePtyHandle, GridSnapshot, Size, TestRunner, TestSession, TERMINAL_SIZE,
};

const SHIFT_RIGHT: [u8; 6] = *b"\x1b[1;2C";
const SHIFT_PAGE_UP: [u8; 6] = *b"\x1b[5;2~";
const BROWSER_TABS: &str = "│ Sessions (";
const TALL: Size = Size {
    cols: 140,
    rows: 44,
};

fn main_session_sized(size: Size) -> (TestSession, FakePtyHandle) {
    let zellij = TestRunner::new(size).with_config(ATTENTION_CONFIG).start();
    let terminal = zellij.expect_pty_spawn();
    terminal.disable_echo();
    terminal.output(b"$ ");
    zellij.wait_until("the session is drawn", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(1))
    });
    (zellij, terminal)
}

fn wait_for_indicator(zellij: &TestSession, what: &str, text: &str) {
    zellij.wait_until(what, |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .map_or(false, |line| line.contains(text))
    });
}

fn long_numbered_lines(terminal: &FakePtyHandle, prefix: &str, count: usize) {
    let mut text = String::new();
    for line in 1..=count {
        text.push_str(&format!(
            "{} {:03} {} END {:03}\r\n",
            prefix,
            line,
            "-".repeat(50),
            line
        ));
    }
    terminal.output(text.as_bytes());
}

fn numbers(grid_snapshot: &GridSnapshot, prefix: &str) -> Vec<usize> {
    regex::Regex::new(&format!(r"{} (\d{{3}})", prefix))
        .unwrap()
        .captures_iter(&grid_snapshot.text)
        .filter_map(|captures| captures[1].parse().ok())
        .collect()
}

fn first_number(grid_snapshot: &GridSnapshot, prefix: &str) -> Option<usize> {
    numbers(grid_snapshot, prefix).into_iter().min()
}

fn shows_bottom(grid_snapshot: &GridSnapshot, prefix: &str, last: usize) -> bool {
    let shown = numbers(grid_snapshot, prefix);
    shown.contains(&last) && !shown.contains(&1)
}

fn shows_start(grid_snapshot: &GridSnapshot, prefix: &str, last: usize) -> bool {
    let shown = numbers(grid_snapshot, prefix);
    shown.contains(&1) && !shown.contains(&last)
}

fn shows_line_ends(grid_snapshot: &GridSnapshot, prefix: &str, last: usize) -> bool {
    numbers(grid_snapshot, "END").contains(&last) && numbers(grid_snapshot, prefix).is_empty()
}

fn open_sessions_and_select(zellij: &TestSession, name: &str) {
    zellij.send_stdin(&keys::F9);
    zellij.wait_until(
        "the sessions tab lists the other session",
        |grid_snapshot| {
            grid_snapshot.contains(BROWSER_TABS)
                && grid_snapshot.contains(SESSIONS_HELP)
                && grid_snapshot.contains(name)
        },
    );
    zellij.send_stdin(&keys::DOWN);
    let title = format!("┌ {}", name);
    zellij.wait_until("the preview shows the other session", |grid_snapshot| {
        grid_snapshot.contains(&title)
    });
}

fn tab_line(grid_snapshot: &GridSnapshot) -> String {
    grid_snapshot
        .lines()
        .into_iter()
        .find(|line| line.contains(BROWSER_TABS))
        .unwrap_or_default()
}

fn inside_frame(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let first = chars.iter().position(|c| *c == '│')?;
    let last = chars.iter().rposition(|c| *c == '│')?;
    if last > first + 1 {
        Some(chars[first + 1..last].iter().collect())
    } else {
        None
    }
}

fn help_has_blank_rows_around_it(grid_snapshot: &GridSnapshot) -> bool {
    let (Some(first_help), Some(last_help)) = (
        grid_snapshot.row_of_line("<ENTER> "),
        grid_snapshot.row_of_line(SESSIONS_HELP),
    ) else {
        return false;
    };
    if first_help == 0 || last_help < first_help {
        return false;
    }
    let lines = grid_snapshot.lines();
    [first_help - 1, last_help + 1].iter().all(|row| {
        lines
            .get(*row)
            .and_then(|line| inside_frame(line))
            .map_or(false, |inside| inside.trim().is_empty())
    })
}

fn more_marker(grid_snapshot: &GridSnapshot, arrow: &str) -> Option<usize> {
    regex::Regex::new(&format!(r"{} (\d+) more", arrow))
        .unwrap()
        .captures(&grid_snapshot.text)
        .and_then(|captures| captures[1].parse().ok())
}

fn open_output_of(zellij: &TestSession, text: &str) -> GridSnapshot {
    open_inbox_on(zellij, text);
    zellij.send_stdin(&keys::key('p'));
    let grid_snapshot = zellij.wait_until("the output is shown", |grid_snapshot| {
        grid_snapshot.contains("Latest output")
            && grid_snapshot.contains("hide output")
            && grid_snapshot.contains(text)
    });
    settled(zellij, grid_snapshot)
}

fn settled(zellij: &TestSession, mut grid_snapshot: GridSnapshot) -> GridSnapshot {
    let deadline = std::time::Instant::now() + zellij_integration_tests::default_timeout();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        let current = zellij.snapshot();
        if current.text == grid_snapshot.text || std::time::Instant::now() >= deadline {
            return current;
        }
        grid_snapshot = current;
    }
}

#[test]
fn shift_keys_pan_the_sessions_preview() {
    let (background, terminal) = background_session_with(ATTENTION_CONFIG);
    long_numbered_lines(&terminal, "row", 16);
    let (mut zellij, _) = main_session_sized(TERMINAL_SIZE);
    open_sessions_and_select(&zellij, background.session_name());
    let bottom = zellij.wait_until("the preview starts at the bottom", |grid_snapshot| {
        shows_bottom(grid_snapshot, "row", 16)
    });
    let first_visible = first_number(&bottom, "row").unwrap();
    assert!(first_visible > 3, "{}", bottom.text);
    let after_up = first_visible - 3;
    zellij.send_stdin(&keys::SHIFT_UP);
    zellij.wait_until("shift up shows three earlier lines", |grid_snapshot| {
        first_number(grid_snapshot, "row") == Some(after_up)
    });
    zellij.send_stdin(&SHIFT_PAGE_UP);
    zellij.wait_until(
        "shift page up shows a page of earlier lines",
        |grid_snapshot| first_number(grid_snapshot, "row").map_or(false, |first| first < after_up),
    );
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until("shift end returns to the bottom", |grid_snapshot| {
        shows_bottom(grid_snapshot, "row", 16)
    });
    zellij.send_stdin(&keys::SHIFT_HOME);
    zellij.wait_until("shift home jumps to the first line", |grid_snapshot| {
        shows_start(grid_snapshot, "row", 16)
    });
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until("shift end returns to the bottom again", |grid_snapshot| {
        shows_bottom(grid_snapshot, "row", 16)
    });
    for _ in 0..6 {
        zellij.send_stdin(&SHIFT_RIGHT);
    }
    zellij.wait_until("shift right shows the ends of the lines", |grid_snapshot| {
        shows_line_ends(grid_snapshot, "row", 16)
    });
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until(
        "shift end shows the starts of the lines again",
        |grid_snapshot| {
            shows_bottom(grid_snapshot, "row", 16) && !numbers(grid_snapshot, "END").contains(&16)
        },
    );
    zellij.quit();
}

#[test]
fn shift_keys_pan_the_inbox_output_from_where_the_notification_began() {
    let (_background, terminal) = background_session_with(ATTENTION_CONFIG);
    let (mut zellij, _) = main_session_sized(TALL);
    long_numbered_lines(&terminal, "line", 40);
    terminal.output(b"\x1b]133;A\x1b\\$ \x1b]133;B\x1b\\make\x1b]133;C\x1b\\\r\n");
    long_numbered_lines(&terminal, "res", 20);
    terminal.output(b"\x1b]133;D;1\x1b\\");
    wait_for_indicator(&zellij, "the failed command is counted", "✗ 1");
    open_output_of(&zellij, "FAILED");
    let bottom = zellij.wait_until("the output starts at the bottom", |grid_snapshot| {
        shows_bottom(grid_snapshot, "res", 20)
    });
    let first_visible = first_number(&bottom, "res").unwrap();
    assert!(first_visible > 3, "{}", bottom.text);
    let after_up = first_visible - 3;
    zellij.send_stdin(&keys::SHIFT_UP);
    zellij.wait_until("shift up shows three earlier lines", |grid_snapshot| {
        first_number(grid_snapshot, "res") == Some(after_up)
    });
    zellij.send_stdin(&SHIFT_PAGE_UP);
    zellij.wait_until(
        "shift page up shows a page of earlier lines",
        |grid_snapshot| first_number(grid_snapshot, "res").map_or(false, |first| first < after_up),
    );
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until("shift end returns to the bottom", |grid_snapshot| {
        shows_bottom(grid_snapshot, "res", 20)
    });
    zellij.send_stdin(&keys::SHIFT_HOME);
    zellij.wait_until(
        "shift home goes to where the command output begins",
        |grid_snapshot| {
            shows_start(grid_snapshot, "res", 20) && !numbers(grid_snapshot, "line").contains(&1)
        },
    );
    zellij.send_stdin(&keys::SHIFT_END);
    zellij.wait_until("shift end returns to the bottom again", |grid_snapshot| {
        shows_bottom(grid_snapshot, "res", 20)
    });
    for _ in 0..6 {
        zellij.send_stdin(&SHIFT_RIGHT);
    }
    zellij.wait_until("shift right shows the ends of the lines", |grid_snapshot| {
        shows_line_ends(grid_snapshot, "res", 20)
    });
    zellij.quit();
}

#[test]
fn the_sessions_preview_reaches_lines_that_scrolled_off_the_running_sessions_screen() {
    let (background, terminal) = background_session_with(ATTENTION_CONFIG);
    long_numbered_lines(&terminal, "row", 60);
    let (mut zellij, _) = main_session_sized(TERMINAL_SIZE);
    open_sessions_and_select(&zellij, background.session_name());
    zellij.wait_until("the preview starts at the bottom", |grid_snapshot| {
        shows_bottom(grid_snapshot, "row", 60)
    });
    zellij.send_stdin(&keys::SHIFT_HOME);
    zellij.wait_until(
        "shift home shows the first line, which is only in the scrollback",
        |grid_snapshot| shows_start(grid_snapshot, "row", 60),
    );
    zellij.quit();
}

#[test]
fn the_tab_switch_hint_is_shortened_on_a_narrow_panel() {
    let (_background, _terminal) = background_session_with(ATTENTION_CONFIG);
    let (mut wide, _) = main_session_sized(TERMINAL_SIZE);
    wide.send_stdin(&keys::F9);
    wide.wait_until("the wide panel shows the full hint", |grid_snapshot| {
        tab_line(grid_snapshot).contains("<TAB> switch")
    });
    wide.quit();
    let (mut narrow, _) = main_session_sized(Size { cols: 38, rows: 24 });
    narrow.send_stdin(&keys::F9);
    narrow.wait_until("the narrow panel shows the short hint", |grid_snapshot| {
        let line = tab_line(grid_snapshot);
        line.contains("<TAB>") && !line.contains("<TAB> switch")
    });
    narrow.quit();
}

#[test]
fn the_sessions_help_has_an_empty_row_above_and_below_it() {
    let (background, _terminal) = background_session_with(ATTENTION_CONFIG);
    let (mut zellij, _) = main_session_sized(TERMINAL_SIZE);
    zellij.send_stdin(&keys::F9);
    zellij.wait_until(
        "the sessions help has empty rows around it",
        |grid_snapshot| {
            grid_snapshot.contains(BROWSER_TABS)
                && grid_snapshot.contains(background.session_name())
                && help_has_blank_rows_around_it(grid_snapshot)
        },
    );
    zellij.quit();
}

#[test]
fn the_inbox_output_opens_below_the_list_when_narrow_and_beside_it_when_wide() {
    let text = "Below or beside?";
    let (_background, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(b"some output\r\n");
    terminal.output(&needs_you(text));

    let (mut narrow, _) = main_session_sized(Size { cols: 80, rows: 40 });
    wait_for_indicator(&narrow, "the item is counted", "● 1");
    let grid_snapshot = open_output_of(&narrow, text);
    let (output_column, output_row) = grid_snapshot.position_of("Latest output").unwrap();
    let (item_column, item_row) = grid_snapshot.position_of("Pane #1 in session").unwrap();
    assert!(
        output_row > item_row && output_column.abs_diff(item_column) <= 2,
        "the output is not below the list:\n{}",
        grid_snapshot.text
    );
    narrow.quit();

    let (mut wide, _) = main_session_sized(TALL);
    wait_for_indicator(&wide, "the item is counted", "● 1");
    let grid_snapshot = open_output_of(&wide, text);
    let (output_column, output_row) = grid_snapshot.position_of("Latest output").unwrap();
    let (item_column, item_row) = grid_snapshot.position_of(text).unwrap();
    assert!(
        output_row < item_row && output_column > item_column + text.chars().count(),
        "the output is not beside the list:\n{}",
        grid_snapshot.text
    );
    wide.quit();
}

#[test]
fn the_inbox_notifications_and_help_are_centred_in_the_panel() {
    let size = Size {
        cols: 136,
        rows: 44,
    };
    let word = "Z".repeat(200);
    let (_background, terminal) = background_session_with(ATTENTION_CONFIG);
    terminal.output(&needs_you(&word));
    let (mut zellij, _) = main_session_sized(size);
    wait_for_indicator(&zellij, "the item is counted", "● 1");
    zellij.send_stdin(&keys::F9);
    let grid_snapshot =
        zellij.wait_until("the inbox shows the item and its help", |grid_snapshot| {
            grid_snapshot.contains(INBOX_HELP)
                && grid_snapshot.contains("<ENTER> go there")
                && grid_snapshot.contains("ZZZZZZZZZZ…")
        });
    let grid_snapshot = settled(&zellij, grid_snapshot);
    let lines = grid_snapshot.lines();
    let (block_left, text_row) = grid_snapshot.position_of("ZZZZZZZZZZ").unwrap();
    let text_line: Vec<char> = lines[text_row].chars().collect();
    let block_right = text_line
        .iter()
        .enumerate()
        .skip(block_left)
        .find(|(_, c)| **c == '…')
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("the long text is not cut:\n{}", grid_snapshot.text));
    let frame_left = text_line[..block_left]
        .iter()
        .rposition(|c| *c == '│')
        .unwrap_or_else(|| panic!("no left frame:\n{}", grid_snapshot.text));
    let frame_right = text_line
        .iter()
        .enumerate()
        .skip(block_right + 1)
        .find(|(_, c)| **c == '│')
        .map(|(index, _)| index)
        .unwrap_or_else(|| panic!("no right frame:\n{}", grid_snapshot.text));
    let left_margin = block_left - frame_left - 1;
    let right_margin = frame_right - block_right - 1;
    assert!(
        left_margin.abs_diff(right_margin) <= 1,
        "the block is not centred across ({} columns left, {} right):\n{}",
        left_margin,
        right_margin,
        grid_snapshot.text
    );
    let (help_column, _) = grid_snapshot.position_of("<ENTER> go there").unwrap();
    assert_eq!(
        help_column, block_left,
        "the help does not line up with the notifications:\n{}",
        grid_snapshot.text
    );
    let blank_in_block = |row: usize| {
        lines[row]
            .chars()
            .skip(block_left)
            .take(block_right + 1 - block_left)
            .all(|c| c == ' ')
    };
    let tabs_row = grid_snapshot.row_of_line(BROWSER_TABS).unwrap();
    let help_row = grid_snapshot.row_of_line(INBOX_HELP).unwrap();
    let block_top = (tabs_row + 1..help_row)
        .find(|row| !blank_in_block(*row))
        .unwrap();
    let above = block_top - tabs_row - 1;
    let below = (help_row + 1..lines.len())
        .take_while(|row| blank_in_block(*row))
        .count();
    assert!(
        above.abs_diff(below) <= 2,
        "the block is not centred down ({} rows above, {} below):\n{}",
        above,
        below,
        grid_snapshot.text
    );
    zellij.quit();
}

#[test]
fn more_markers_count_the_notifications_out_of_view_and_follow_scrolling() {
    let (mut zellij, _) = main_session_sized(TERMINAL_SIZE);
    let mut sessions = vec![];
    for index in 0..6 {
        let (session, terminal) = background_session_with(ATTENTION_CONFIG);
        terminal.output(&needs_you(&format!("Question number {}?", index)));
        wait_for_indicator(&zellij, "the item is counted", &format!("● {}", index + 1));
        sessions.push((session, terminal));
    }
    zellij.send_stdin(&keys::F9);
    zellij.wait_until("the inbox opens", |grid_snapshot| {
        grid_snapshot.contains(INBOX_HELP)
    });
    for _ in 0..6 {
        zellij.send_stdin(&keys::UP);
    }
    let top = zellij.wait_until("at the top only the lower marker shows", |grid_snapshot| {
        more_marker(grid_snapshot, "↑").is_none() && more_marker(grid_snapshot, "↓").is_some()
    });
    let below_at_top = more_marker(&top, "↓").unwrap();
    assert!(below_at_top >= 1, "{}", top.text);
    for _ in 0..6 {
        zellij.send_stdin(&keys::DOWN);
    }
    let bottom = zellij.wait_until("at the bottom the upper marker shows", |grid_snapshot| {
        more_marker(grid_snapshot, "↑").is_some()
            && more_marker(grid_snapshot, "↓").map_or(true, |below| below < below_at_top)
    });
    assert!(more_marker(&bottom, "↑").unwrap() >= 1, "{}", bottom.text);
    zellij.quit();
}
