#![cfg(unix)]

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::engine::Engine as _;
use insta::assert_snapshot;
use zellij_integration_tests::client_screen::render_bytes;
use zellij_integration_tests::{
    claim_first_terminal_and_wait_for_prompt, col, keys, normalized,
    split_right_and_wait_for_prompt, start_zellij, FakePtyHandle, GridSnapshot, HostTerminal,
    LayoutInfo, Size, TestRunner, TestSession, PROMPT, TERMINAL_SIZE,
};
use zellij_utils::cli::CliAction;

const FAMILY: &str = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
const FLAG: &str = "\u{1f1e9}\u{1f1ea}";
const THUMBS: &str = "\u{1f44d}\u{1f3fd}";
const RAINBOW: &str = "\u{1f3f3}\u{fe0f}\u{200d}\u{1f308}";
const ENGLAND: &str = "\u{1f3f4}\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}";
const KSSA: &str = "\u{915}\u{94d}\u{937}";
const THAI: &str = "\u{e01}\u{e49}\u{e33}";
const JAMO: &str = "\u{1100}\u{1161}\u{11a8}";
const DECOMPOSED_E: &str = "e\u{301}";
const WARNING_WITH_VS16: &str = "\u{26a0}\u{fe0f}";

const TYPED_NAME: &str = "ab\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}ce\u{301}\u{1f1e9}\u{1f1ea}";
const RENAMED: &str = "ab\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}ce\u{301}";

const ENABLE_GRAPHEME_CLUSTER_MODE: &[u8] = b"\x1b[?2027h";
const DISABLE_GRAPHEME_CLUSTER_MODE: &[u8] = b"\x1b[?2027l";
const EXIT_ALTERNATE_SCREEN: &[u8] = b"\x1b[?1049l";
const BACKSPACE: [u8; 1] = [0x7f];
const MOUSE_CONFIG: &str = "mouse_mode true\ncopy_clipboard \"system\"";

const FLOATING_LEFT_BORDER: usize = 30;
const FLOATING_RIGHT_BORDER: usize = 89;

const GRAPHEME_MODE_CLUSTERS: [(&str, usize); 5] = [
    (FAMILY, 2),
    (FLAG, 2),
    (THUMBS, 2),
    (KSSA, 2),
    (DECOMPOSED_E, 1),
];

struct Sample {
    label: &'static str,
    input: &'static str,
    columns: usize,
    shown: &'static str,
}

const DEFAULT_MODE_SAMPLES: [Sample; 24] = [
    Sample {
        label: "ascii",
        input: "abc",
        columns: 3,
        shown: "abc",
    },
    Sample {
        label: "cjk",
        input: "\u{4e16}\u{754c}",
        columns: 4,
        shown: "\u{4e16}\u{754c}",
    },
    Sample {
        label: "e with a combining accent",
        input: "e\u{301}",
        columns: 1,
        shown: "e\u{301}",
    },
    Sample {
        label: "letter with stacked marks",
        input: "a\u{300}\u{301}\u{302}\u{303}",
        columns: 1,
        shown: "a\u{300}\u{301}\u{302}\u{303}",
    },
    Sample {
        label: "warning sign with VS16",
        input: "\u{26a0}\u{fe0f}",
        columns: 1,
        shown: "\u{26a0}",
    },
    Sample {
        label: "watch with VS15",
        input: "\u{231a}\u{fe0e}",
        columns: 2,
        shown: "\u{231a}\u{fe0e}",
    },
    Sample {
        label: "grinning face with VS15",
        input: "\u{1f600}\u{fe0e}",
        columns: 2,
        shown: "\u{1f600}\u{fe0e}",
    },
    Sample {
        label: "keycap",
        input: "1\u{fe0f}\u{20e3}",
        columns: 1,
        shown: "1\u{20e3}",
    },
    Sample {
        label: "thumbs up with a skin tone",
        input: THUMBS,
        columns: 4,
        shown: THUMBS,
    },
    Sample {
        label: "lone skin tone modifier",
        input: "\u{1f3fd}",
        columns: 2,
        shown: "\u{1f3fd}",
    },
    Sample {
        label: "flag",
        input: FLAG,
        columns: 2,
        shown: FLAG,
    },
    Sample {
        label: "three regional indicators",
        input: "\u{1f1e9}\u{1f1ea}\u{1f1eb}",
        columns: 3,
        shown: "\u{1f1e9}\u{1f1ea}\u{1f1eb}",
    },
    Sample {
        label: "family",
        input: FAMILY,
        columns: 6,
        shown: FAMILY,
    },
    Sample {
        label: "rainbow flag",
        input: RAINBOW,
        columns: 3,
        shown: RAINBOW,
    },
    Sample {
        label: "england flag",
        input: ENGLAND,
        columns: 2,
        shown: ENGLAND,
    },
    Sample {
        label: "devanagari conjunct",
        input: KSSA,
        columns: 2,
        shown: KSSA,
    },
    Sample {
        label: "devanagari spacing vowel sign",
        input: "\u{915}\u{93e}",
        columns: 2,
        shown: "\u{915}\u{93e}",
    },
    Sample {
        label: "thai",
        input: THAI,
        columns: 2,
        shown: THAI,
    },
    Sample {
        label: "hangul jamo",
        input: JAMO,
        columns: 2,
        shown: JAMO,
    },
    Sample {
        label: "halfwidth katakana with a voicing mark",
        input: "\u{ff76}\u{ff9e}",
        columns: 1,
        shown: "\u{ff76}\u{ff9e}",
    },
    Sample {
        label: "soft hyphen",
        input: "a\u{ad}b",
        columns: 2,
        shown: "a\u{ad}b",
    },
    Sample {
        label: "zero-width code point at column zero",
        input: "\u{301}x",
        columns: 1,
        shown: "x",
    },
    Sample {
        label: "zero width joiner between letters",
        input: "a\u{200d}b",
        columns: 2,
        shown: "a\u{200d}b",
    },
    Sample {
        label: "width three character",
        input: "\u{17d8}",
        columns: 3,
        shown: "\u{17d8}",
    },
];

fn start_zellij_with_grapheme_aware_host(config: &str) -> TestSession {
    let zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(config)
        .with_host_terminal(HostTerminal::GraphemeClusterAware(2))
        .start();
    zellij.wait_until_raw_output(
        "the client turned on grapheme cluster mode in its host terminal",
        |bytes| contains_bytes(bytes, ENABLE_GRAPHEME_CLUSTER_MODE),
    );
    zellij
}

fn claim_terminal_and_wait_for_bars(zellij: &TestSession) -> FakePtyHandle {
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("first terminal prompt rendered", |grid_snapshot| {
        grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("$ ")
    });
    terminal
}

fn open_right_pane(
    zellij: &TestSession,
    opened: impl Fn(&GridSnapshot) -> bool,
) -> FakePtyHandle {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('r'));
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("the right pane opened", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && opened(grid_snapshot)
    });
    terminal
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    find_bytes(haystack, needle).is_some()
}

fn occurrences(haystack: &[u8], needle: &[u8]) -> usize {
    if needle.is_empty() || haystack.len() < needle.len() {
        return 0;
    }
    haystack
        .windows(needle.len())
        .filter(|window| *window == needle)
        .count()
}

fn contains_complex_unit(bytes: &[u8], erased_columns: usize, text: &str) -> bool {
    let prefix = format!("\x1b[{}X{}\x1b[", erased_columns, text);
    let prefix = prefix.as_bytes();
    if bytes.len() < prefix.len() {
        return false;
    }
    bytes
        .windows(prefix.len())
        .enumerate()
        .any(|(index, window)| {
            if window != prefix {
                return false;
            }
            let rest = &bytes[index + prefix.len()..];
            let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
            digits > 0 && rest.get(digits) == Some(&b'G')
        })
}

fn cursor_reports(bytes: &[u8]) -> Vec<(usize, usize)> {
    let pattern = regex::bytes::Regex::new(r"\x1b\[(\d+);(\d+)R").unwrap();
    pattern
        .captures_iter(bytes)
        .map(|captures| {
            let row = std::str::from_utf8(&captures[1]).unwrap().parse().unwrap();
            let column = std::str::from_utf8(&captures[2]).unwrap().parse().unwrap();
            (row, column)
        })
        .collect()
}

fn report_cursor(terminal: &FakePtyHandle) -> (usize, usize) {
    let seen = cursor_reports(&terminal.stdin_bytes()).len();
    terminal.output(b"\x1b[6n");
    let stdin = terminal.wait_for_stdin("the pane reported its cursor", move |stdin| {
        cursor_reports(stdin).len() > seen
    });
    *cursor_reports(&stdin)
        .last()
        .expect("the pane reported its cursor")
}

fn copied_texts(bytes: &[u8]) -> Vec<String> {
    let marker: &[u8] = b"\x1b]52;c;";
    let mut texts = Vec::new();
    let mut rest = bytes;
    while let Some(start) = find_bytes(rest, marker) {
        let payload = &rest[start + marker.len()..];
        let end = payload
            .iter()
            .position(|byte| *byte == 0x1b || *byte == 0x07)
            .unwrap_or(payload.len());
        if let Ok(decoded) = BASE64_STANDARD.decode(&payload[..end]) {
            texts.push(String::from_utf8_lossy(&decoded).into_owned());
        }
        rest = &payload[end..];
    }
    texts
}

fn sgr_mouse(button: u8, column: usize, line: usize, final_byte: char) -> Vec<u8> {
    format!("\u{1b}[<{};{};{}{}", button, column, line, final_byte).into_bytes()
}

fn click(zellij: &TestSession, column: usize, line: usize) {
    zellij.send_stdin(&sgr_mouse(0, column, line, 'M'));
    zellij.send_stdin(&sgr_mouse(0, column, line, 'm'));
}

fn triple_click(zellij: &TestSession, column: usize, line: usize) {
    for _ in 0..3 {
        click(zellij, column, line);
    }
}

fn type_text(zellij: &TestSession, text: &str) {
    for character in text.chars() {
        let mut buffer = [0u8; 4];
        zellij.send_stdin(character.encode_utf8(&mut buffer).as_bytes());
    }
}

fn code_point_width(character: char) -> usize {
    match character as u32 {
        0x300..=0x36f | 0x200d | 0xfe0e | 0xfe0f | 0x94d | 0xe49 | 0xe0000..=0xe007f => 0,
        0x1f3f3 | 0x1f1e6..=0x1f1ff => 1,
        0x1100..=0x115f | 0x4e00..=0x9fff | 0x1f300..=0x1faff => 2,
        _ => 1,
    }
}

fn columns_in_default_mode(text: &str) -> usize {
    text.chars().map(code_point_width).sum()
}

fn units_in_grapheme_mode(text: &str) -> Vec<(usize, &str)> {
    let mut units = Vec::new();
    let mut column = 0;
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let (length, width) = GRAPHEME_MODE_CLUSTERS
            .iter()
            .find(|entry| rest.starts_with(entry.0))
            .map(|entry| (entry.0.len(), entry.1))
            .unwrap_or((first.len_utf8(), code_point_width(first)));
        units.push((column, &rest[..length]));
        column += width;
        rest = &rest[length..];
    }
    units
}

fn columns_in_grapheme_mode(text: &str) -> usize {
    let mut column = 0;
    let mut rest = text;
    while let Some(first) = rest.chars().next() {
        let (length, width) = GRAPHEME_MODE_CLUSTERS
            .iter()
            .find(|entry| rest.starts_with(entry.0))
            .map(|entry| (entry.0.len(), entry.1))
            .unwrap_or((first.len_utf8(), code_point_width(first)));
        column += width;
        rest = &rest[length..];
    }
    column
}

fn grapheme_mode_columns_of(line: &str, targets: &[char]) -> Vec<usize> {
    units_in_grapheme_mode(line)
        .into_iter()
        .filter(|(_, unit)| {
            let mut characters = unit.chars();
            match (characters.next(), characters.next()) {
                (Some(only), None) => targets.contains(&only),
                _ => false,
            }
        })
        .map(|(column, _)| column)
        .collect()
}

fn grapheme_mode_column_of(line: &str, needle: &str) -> Option<usize> {
    line.find(needle)
        .map(|byte_index| columns_in_grapheme_mode(&line[..byte_index]))
}

fn families_are_whole(line: &str) -> bool {
    let whole = line.matches(FAMILY).count();
    line.matches('\u{1f468}').count() == whole
        && line.matches('\u{1f469}').count() == whole
        && line.matches('\u{1f467}').count() == whole
        && line.matches('\u{200d}').count() == 2 * whole
}

fn line_containing(grid_snapshot: &GridSnapshot, needle: &str) -> String {
    grid_snapshot
        .lines()
        .into_iter()
        .find(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line contains {:?}:\n{}", needle, grid_snapshot))
        .trim_end()
        .to_owned()
}

fn line_followed_by(grid_snapshot: &GridSnapshot, exact: &str, next_prefix: &str) -> bool {
    let lines = grid_snapshot.lines();
    lines.windows(2).any(|pair| {
        pair[0].trim_end() == exact && pair[1].starts_with(next_prefix)
    })
}

fn highlighted_cells(grid_snapshot: &GridSnapshot, row: usize) -> Vec<usize> {
    let base = grid_snapshot.cell_style(0, row);
    grid_snapshot
        .styles
        .get(row)
        .map(|cells| {
            cells
                .iter()
                .enumerate()
                .filter(|(_, style)| **style != base)
                .map(|(index, _)| index)
                .collect()
        })
        .unwrap_or_default()
}

fn pane_screen(bytes: &[u8], size: Size) -> Vec<String> {
    render_bytes(bytes, size)
        .lines()
        .iter()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

fn pane_region(grid_snapshot: &GridSnapshot, rows: usize) -> Vec<String> {
    grid_snapshot
        .lines()
        .iter()
        .skip(1)
        .take(rows)
        .map(|line| line.trim_end().to_owned())
        .collect()
}

fn assert_no_half_glyphs(line: &str, cut_at_left_border: char, cut_at_right_border: char) {
    let parts: Vec<&str> = line.splitn(3, '│').collect();
    assert_eq!(
        parts.len(),
        3,
        "the row must show both borders of the floating pane, got {:?}",
        line
    );
    assert_eq!(
        columns_in_default_mode(parts[0]),
        FLOATING_LEFT_BORDER,
        "the left border of the floating pane moved, got {:?}",
        line
    );
    assert_eq!(
        columns_in_default_mode(parts[0]) + 1 + columns_in_default_mode(parts[1]),
        FLOATING_RIGHT_BORDER,
        "the right border of the floating pane moved, got {:?}",
        line
    );
    assert!(
        !parts[0].contains(cut_at_left_border),
        "a cell cut by the left border must not be shown in half, got {:?}",
        line
    );
    assert!(
        !parts[2].contains(cut_at_right_border),
        "a cell cut by the right border must not be shown in half, got {:?}",
        line
    );
}

fn assert_frame_row_lines_up(
    grid_snapshot: &GridSnapshot,
    title_row: usize,
    title_borders: &[char],
    content_row: usize,
) {
    let lines = grid_snapshot.lines();
    assert_eq!(
        grapheme_mode_columns_of(&lines[title_row], title_borders),
        grapheme_mode_columns_of(&lines[content_row], &['│']),
        "the title row borders must line up with the content row borders:\n{}",
        grid_snapshot
    );
}

fn assert_name_kept_whole(line: &str) {
    assert!(
        families_are_whole(line),
        "the family emoji must not be split, got {:?}",
        line
    );
    assert!(
        !line.contains('\u{1f1e9}') && !line.contains('\u{1f1ea}'),
        "backspace must remove the whole flag, got {:?}",
        line
    );
}

fn rename_focused_pane_by_typing(zellij: &TestSession) {
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('c'));
    zellij.wait_until("rename pane mode active", |grid_snapshot| {
        grid_snapshot.contains("RENAMING PANE")
    });
    type_text(zellij, TYPED_NAME);
    zellij.send_stdin(&BACKSPACE);
    zellij.send_stdin(&keys::ENTER);
}

fn wait_for_renamed_pane(zellij: &TestSession, name_visible: bool) -> GridSnapshot {
    zellij.wait_until("the pane renamed", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("RENAMING PANE")
            && (!name_visible || grid_snapshot.contains(RENAMED))
    })
}

fn click_on(zellij: &TestSession, grid_snapshot: &GridSnapshot, row: usize, needle: &str) {
    let lines = grid_snapshot.lines();
    let column = grapheme_mode_column_of(&lines[row], needle)
        .unwrap_or_else(|| panic!("{:?} is not on row {}:\n{}", needle, row, grid_snapshot));
    click(zellij, column + 2, row + 1);
}

fn highlighted_cells_after_searching(content: &str, term: &str, marker: &str) -> Vec<usize> {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let mut output = format!("\r\n{}\r\n", content);
    for line in 1..=30 {
        output.push_str(&format!("filler {:02}\r\n", line));
    }
    terminal.output(output.as_bytes());
    zellij.wait_until("the match scrolled out of view", |grid_snapshot| {
        grid_snapshot.contains("filler 30") && !grid_snapshot.contains(marker)
    });
    zellij.send_stdin(&keys::ctrl('s'));
    zellij.send_stdin(&keys::key('s'));
    zellij.wait_until("search input mode active", |grid_snapshot| {
        grid_snapshot.contains("ENTERING SEARCH TERM")
    });
    type_text(&zellij, term);
    zellij.send_stdin(&keys::ENTER);
    let grid_snapshot = zellij.wait_until("the match highlighted", |grid_snapshot| {
        grid_snapshot.contains("PgDn|PgUp")
            && grid_snapshot
                .row_of_line(marker)
                .map_or(false, |row| !highlighted_cells(grid_snapshot, row).is_empty())
    });
    let row = grid_snapshot
        .row_of_line(marker)
        .expect("the match is on screen");
    let cells = highlighted_cells(&grid_snapshot, row);
    zellij.quit();
    cells
}

#[test]
fn default_mode_keeps_every_cluster_category_at_per_code_point_widths() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    for (batch_index, batch) in DEFAULT_MODE_SAMPLES.chunks(12).enumerate() {
        terminal.output(b"\x1b[2J");
        for (index, sample) in batch.iter().enumerate() {
            let row = index + 1;
            terminal.output(format!("\x1b[{};1H{}", row, sample.input).as_bytes());
            assert_eq!(
                report_cursor(&terminal),
                (row, sample.columns + 1),
                "{}: the pane cursor must advance by per-code-point widths",
                sample.label
            );
            zellij.wait_until(sample.label, |grid_snapshot| {
                grid_snapshot.cursor_is_at(col(sample.columns).row(row))
                    && grid_snapshot
                        .lines()
                        .get(row)
                        .map_or(false, |line| line.starts_with(sample.shown))
            });
            terminal.output(b"|");
        }
        let marker = format!("BATCH-{}-END", batch_index);
        terminal.output(format!("\x1b[14;1H{}", marker).as_bytes());
        let grid_snapshot = zellij.wait_until("batch rendered", |grid_snapshot| {
            grid_snapshot.contains(&marker) && grid_snapshot.status_bar_appears()
        });
        let lines = grid_snapshot.lines();
        for (index, sample) in batch.iter().enumerate() {
            let expected = format!("{}|", sample.shown);
            assert!(
                lines[index + 1].starts_with(&expected),
                "{}: expected the row to start with {:?}, got {:?}",
                sample.label,
                expected,
                lines[index + 1]
            );
        }
        assert_snapshot!(normalized(&grid_snapshot));
    }
    zellij.quit();
}

#[test]
fn a_warning_sign_with_vs16_shows_in_one_column_and_copies_with_its_selector() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(MOUSE_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let line = format!("WARN{}|end", WARNING_WITH_VS16);
    terminal.output(format!("\r\n{}", line).as_bytes());
    let grid_snapshot = zellij.wait_until("the warning sign rendered", |grid_snapshot| {
        grid_snapshot.contains("WARN\u{26a0}|end")
    });
    assert!(
        !grid_snapshot.contains("\u{fe0f}"),
        "a VS16 in a one-column cell must be left out of the display output"
    );

    triple_click(&zellij, 2, 3);
    let raw_output = zellij.wait_until_raw_output("the line was copied", |bytes| {
        copied_texts(bytes).contains(&line)
    });
    assert!(
        !contains_bytes(&raw_output, WARNING_WITH_VS16.as_bytes()),
        "the VS16 must never reach the display output"
    );
    zellij.quit();
}

#[test]
fn a_default_mode_family_emoji_is_erased_written_and_repositioned() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    terminal.output(format!("\x1b[3;5H{}|", FAMILY).as_bytes());

    let expected = format!("\x1b[6X{}\x1b[11G", FAMILY);
    zellij.wait_until_raw_output(
        "the family emoji written as erase, text, column move",
        |bytes| contains_bytes(bytes, expected.as_bytes()),
    );
    let expected_line = format!("    {}|", FAMILY);
    let grid_snapshot = zellij.wait_until("the family emoji rendered", |grid_snapshot| {
        grid_snapshot
            .lines()
            .get(3)
            .map_or(false, |line| line.starts_with(&expected_line))
    });
    assert!(
        grid_snapshot.cursor_is_at(col(11).row(3)),
        "the cursor must follow per-code-point widths, got {:?}",
        grid_snapshot.cursor
    );
    zellij.quit();
}

#[test]
fn a_family_emoji_in_grapheme_cluster_mode_takes_two_columns() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    terminal.output(format!("\x1b[?2027h\x1b[3;1H{}", FAMILY).as_bytes());
    assert_eq!(
        report_cursor(&terminal),
        (3, 3),
        "a family emoji in grapheme cluster mode advances the cursor by two columns"
    );
    zellij.wait_until("the family emoji rendered", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(3))
            && grid_snapshot
                .lines()
                .get(3)
                .map_or(false, |line| line.starts_with(FAMILY))
    });

    terminal.output(b"|");
    let expected_line = format!("{}|", FAMILY);
    zellij.wait_until("text after the family emoji rendered", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(3).row(3))
            && grid_snapshot
                .lines()
                .get(3)
                .map_or(false, |line| line.starts_with(&expected_line))
    });
    let raw_output = zellij.raw_bytes();
    assert!(
        contains_bytes(
            &raw_output,
            format!("\x1b[2X{}\x1b[3G", FAMILY).as_bytes()
        ),
        "the two-column family cell is written as erase two, text, column move"
    );
    assert!(
        !contains_bytes(&raw_output, format!("\x1b[6X{}", FAMILY).as_bytes()),
        "the family emoji must not be drawn as a six-column group"
    );
    zellij.quit();
}

#[test]
fn a_program_can_query_grapheme_cluster_mode() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    terminal.output(b"\x1b[?2027$p");
    terminal.wait_for_stdin("the mode reported as reset", |stdin| {
        occurrences(stdin, b"\x1b[?2027;2$y") == 1
    });

    terminal.output(b"\x1b[?2027h\x1b[?2027$p");
    terminal.wait_for_stdin("the mode reported as set", |stdin| {
        occurrences(stdin, b"\x1b[?2027;1$y") == 1
    });

    terminal.output(b"\x1b[?2027l\x1b[?2027$p");
    let stdin = terminal.wait_for_stdin("the mode reported as reset again", |stdin| {
        occurrences(stdin, b"\x1b[?2027;2$y") == 2
    });
    assert_eq!(
        occurrences(&stdin, b"\x1b[?2027;1$y"),
        1,
        "the mode is reported as set only while it is set"
    );
    zellij.quit();
}

#[test]
fn widening_at_the_last_column_wraps_the_cluster_whole() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let (cols, _rows) = terminal.wait_for_size("the pane reported its size", |cols, _| cols > 4);

    terminal.output(format!("\x1b[?2027h\x1b[3;{}H\u{26a0}", cols).as_bytes());
    terminal.output("\u{fe0f}".as_bytes());
    assert_eq!(
        report_cursor(&terminal),
        (4, 3),
        "the widened cluster moves whole to the next line and the cursor follows it"
    );
    terminal.output(b"X");
    let expected_line = format!("{}X", WARNING_WITH_VS16);
    let grid_snapshot = zellij.wait_until("the wrapped cluster rendered", |grid_snapshot| {
        grid_snapshot
            .lines()
            .get(4)
            .map_or(false, |line| line.starts_with(&expected_line))
    });
    assert!(
        !grid_snapshot.lines()[3].contains('\u{26a0}'),
        "the old column of the cluster must be blank, got {:?}",
        grid_snapshot.lines()[3]
    );
    zellij.quit();
}

#[test]
fn vs15_narrows_the_cluster_and_moves_the_cursor_back() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();

    terminal.output("\x1b[?2027h\x1b[5;1H\u{231a}".as_bytes());
    assert_eq!(
        report_cursor(&terminal),
        (5, 3),
        "the watch takes two columns"
    );
    terminal.output("\u{fe0e}".as_bytes());
    assert_eq!(
        report_cursor(&terminal),
        (5, 2),
        "VS15 narrows the watch to one column and the cursor moves back"
    );
    terminal.output(b"X");
    zellij.wait_until("the narrowed cluster rendered", |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(2).row(5))
            && grid_snapshot
                .lines()
                .get(5)
                .map_or(false, |line| line.starts_with("\u{231a}\u{fe0e}X"))
    });
    zellij.quit();
}

#[test]
fn complex_units_up_to_the_right_edge_keep_the_border_and_the_neighbour_intact() {
    let mut zellij = start_zellij();
    let left = claim_first_terminal_and_wait_for_prompt(&zellij);
    left.disable_echo();
    let right = split_right_and_wait_for_prompt(&zellij);
    right.disable_echo();
    let (cols, _rows) = left.wait_for_size("the left pane narrowed", |cols, _| {
        (cols as usize) < TERMINAL_SIZE.cols
    });
    let cols = cols as usize;

    let units: [(&str, usize); 6] = [
        (FAMILY, 6),
        (FLAG, 2),
        (THUMBS, 4),
        (KSSA, 2),
        (RAINBOW, 3),
        (THAI, 2),
    ];
    let rows: Vec<String> = units
        .iter()
        .map(|(unit, width)| format!("{}{}", "x".repeat(cols % width), unit.repeat(cols / width)))
        .collect();
    let mut left_output = String::new();
    for row in &rows {
        left_output.push_str("\r\n");
        left_output.push_str(row);
    }
    left.output(left_output.as_bytes());
    let mut right_output = String::new();
    for index in 1..=rows.len() {
        right_output.push_str(&format!("\r\nright-{:02}", index));
    }
    right_output.push_str("\r\n$ ");
    right.output(right_output.as_bytes());

    let last_row = rows.last().cloned().unwrap();
    let grid_snapshot = zellij.wait_until("both panes filled", |grid_snapshot| {
        grid_snapshot.contains("right-06") && grid_snapshot.contains(&last_row)
    });
    let lines = grid_snapshot.lines();
    for (index, row) in rows.iter().enumerate() {
        let line = &lines[3 + index];
        let (left_part, right_part) = line
            .split_once('│')
            .unwrap_or_else(|| panic!("the border between the panes is missing in {:?}", line));
        assert_eq!(
            left_part,
            row.as_str(),
            "the left pane row must end exactly at the border"
        );
        assert_eq!(
            right_part.trim_end(),
            format!("right-{:02}", index + 1),
            "the right pane content must stay in place"
        );
    }
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn overwriting_part_of_a_cluster_leaves_the_client_screen_equal_to_the_pane_screen() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let (cols, rows) = terminal.wait_for_size("the pane reported its size", |cols, _| cols > 4);
    let pane_size = Size {
        cols: cols as usize,
        rows: rows as usize,
    };
    let mut pane_bytes = PROMPT.to_vec();
    let cases = [
        (
            "the middle of a family emoji overwritten",
            format!("\x1b[3;1H{}\x1b[3;3HX", FAMILY),
        ),
        (
            "the left half of a wide emoji overwritten",
            "\x1b[5;1H\u{1f600}\x1b[5;1HY".to_owned(),
        ),
        (
            "the right half of a wide emoji overwritten",
            "\x1b[7;1H\u{1f600}\x1b[7;2HZ".to_owned(),
        ),
    ];
    for (what, bytes) in cases.iter() {
        terminal.output(bytes.as_bytes());
        pane_bytes.extend_from_slice(bytes.as_bytes());
        let expected = pane_screen(&pane_bytes, pane_size);
        zellij.wait_until(what, |grid_snapshot| {
            pane_region(grid_snapshot, pane_size.rows) == expected
        });
    }
    zellij.quit();
}

#[test]
fn a_floating_pane_over_groups_and_wide_clusters_leaves_no_half_glyphs() {
    let mut zellij = start_zellij();
    let tiled = claim_first_terminal_and_wait_for_prompt(&zellij);
    tiled.disable_echo();
    let row_a = format!(
        "{}{}{}\u{1f600}{}",
        "x".repeat(27),
        FAMILY,
        "y".repeat(56),
        "z".repeat(29)
    );
    let row_b = format!(
        "{}\u{1f600}{}{}{}",
        "x".repeat(29),
        "y".repeat(55),
        FAMILY,
        "z".repeat(28)
    );
    tiled.output(format!("\x1b[11;1H{}\x1b[12;1H{}\x1b[20;1HFLOAT-END", row_a, row_b).as_bytes());
    zellij.wait_until("tiled content rendered", |grid_snapshot| {
        grid_snapshot.contains("FLOAT-END")
    });

    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating = zellij.expect_pty_spawn();
    floating.output(PROMPT);
    let rows_show_both_borders = |grid_snapshot: &GridSnapshot, rows: &[usize]| {
        rows.iter().all(|row| {
            grid_snapshot.lines().get(*row).map_or(false, |line| {
                line.matches('│').count() == 2 && line.starts_with('x')
            })
        })
    };
    let grid_snapshot = zellij.wait_until("floating pane over the clusters", |grid_snapshot| {
        grid_snapshot.contains("PIN [ ]") && rows_show_both_borders(grid_snapshot, &[11, 12])
    });
    assert_no_half_glyphs(&grid_snapshot.lines()[11], '\u{1f469}', '\u{1f600}');
    assert_no_half_glyphs(&grid_snapshot.lines()[12], '\u{1f600}', '\u{1f469}');

    tiled.output(format!("\x1b[13;1H{}\x1b[14;1H{}", row_a, row_b).as_bytes());
    let grid_snapshot = zellij.wait_until(
        "clusters written under the floating pane",
        |grid_snapshot| rows_show_both_borders(grid_snapshot, &[11, 12, 13, 14]),
    );
    assert_no_half_glyphs(&grid_snapshot.lines()[13], '\u{1f469}', '\u{1f600}');
    assert_no_half_glyphs(&grid_snapshot.lines()[14], '\u{1f600}', '\u{1f469}');
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn only_a_client_whose_host_answers_turns_on_grapheme_cluster_mode() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let second_client = zellij.attach_client(TERMINAL_SIZE);
    second_client.wait_until("second client loaded", |grid_snapshot| {
        grid_snapshot.tab_bar_appears() && grid_snapshot.status_bar_appears()
    });

    terminal.output(format!("\r\nTWO:{}|{}|END\r\n", FAMILY, KSSA).as_bytes());
    let answering_grid = zellij.wait_until("the answering client shows the clusters", |grid_snapshot| {
        grid_snapshot.contains("|END")
    });
    let silent_grid = second_client.wait_until("the silent client shows the clusters", |grid_snapshot| {
        grid_snapshot.contains("|END")
    });
    assert_eq!(
        line_containing(&answering_grid, "TWO:"),
        format!("TWO:{}    |{}|END", FAMILY, KSSA),
        "a host in grapheme cluster mode draws the family in two columns followed by blanks"
    );
    assert_eq!(
        line_containing(&silent_grid, "TWO:"),
        format!("TWO:{}|{}|END", FAMILY, KSSA),
        "a host without grapheme cluster mode draws the family per code point"
    );
    assert_eq!(
        answering_grid.cursor, silent_grid.cursor,
        "both clients show the cursor in the same place"
    );
    assert_eq!(
        occurrences(&zellij.raw_bytes(), ENABLE_GRAPHEME_CLUSTER_MODE),
        1,
        "the answering client turns the mode on exactly once"
    );

    second_client.send_stdin(&keys::ctrl('o'));
    second_client.send_stdin(&keys::key('d'));
    let silent_bytes = second_client.wait_until_raw_output("the silent client detached", |bytes| {
        contains_bytes(bytes, EXIT_ALTERNATE_SCREEN)
    });
    second_client.wait_for_client_to_exit();
    assert!(
        !contains_bytes(&silent_bytes, ENABLE_GRAPHEME_CLUSTER_MODE),
        "a client whose host did not answer must not turn the mode on"
    );
    assert!(
        !contains_bytes(&silent_bytes, DISABLE_GRAPHEME_CLUSTER_MODE),
        "a client whose host did not answer must not turn the mode off"
    );

    zellij.detach_main_client();
    let answering_bytes = zellij.raw_bytes();
    assert_eq!(
        occurrences(&answering_bytes, DISABLE_GRAPHEME_CLUSTER_MODE),
        1,
        "the answering client turns the mode off exactly once"
    );
    assert!(
        contains_bytes(
            &answering_bytes,
            &[DISABLE_GRAPHEME_CLUSTER_MODE, EXIT_ALTERNATE_SCREEN].concat()
        ),
        "the mode is turned off with the other terminal-restore sequences on detach"
    );
    assert!(
        find_bytes(&answering_bytes, ENABLE_GRAPHEME_CLUSTER_MODE)
            < find_bytes(&answering_bytes, DISABLE_GRAPHEME_CLUSTER_MODE),
        "the mode is turned off after it was turned on"
    );
    zellij.kill_session();
}

#[test]
fn a_mouse_selection_copies_whole_clusters() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config(MOUSE_CONFIG)
        .start();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let line = format!("SEL{}|{}|{}|{}|end", FAMILY, DECOMPOSED_E, FLAG, KSSA);
    terminal.output(format!("\r\n{}", line).as_bytes());
    zellij.wait_until("the cluster line rendered", |grid_snapshot| {
        grid_snapshot.contains(&line)
    });

    zellij.send_stdin(&sgr_mouse(0, 6, 3, 'M'));
    zellij.send_stdin(&sgr_mouse(32, 13, 3, 'M'));
    zellij.send_stdin(&sgr_mouse(0, 13, 3, 'm'));
    let raw_output = zellij.wait_until_raw_output("the dragged selection was copied", |bytes| {
        !copied_texts(bytes).is_empty()
    });
    let copied = copied_texts(&raw_output);
    let last_copy = copied.first().expect("a selection was copied").clone();
    assert!(
        last_copy.starts_with(FAMILY),
        "a selection starting inside the family group is widened to the whole group, got {:?}",
        last_copy
    );
    assert!(
        last_copy.contains(&format!("|{}|", DECOMPOSED_E)),
        "the copied text keeps the combining accent, got {:?}",
        last_copy
    );
    zellij.quit();
}

#[test]
fn search_highlights_the_columns_of_cluster_matches() {
    let family_cells =
        highlighted_cells_after_searching(&format!("MARK{}TAIL", FAMILY), FAMILY, "MARK");
    assert_eq!(
        family_cells,
        vec![4, 5, 6],
        "the three cells of the family group are highlighted and nothing else"
    );
    let accent_cells = highlighted_cells_after_searching(
        &format!("MARK{}TAIL", DECOMPOSED_E),
        DECOMPOSED_E,
        "MARK",
    );
    assert_eq!(
        accent_cells,
        vec![4],
        "the cell holding the decomposed letter is highlighted and nothing else"
    );
}

#[test]
fn clusters_and_groups_rewrap_whole_on_resize() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let group_line = format!(
        "{}{}{}{}",
        "x".repeat(57),
        FAMILY,
        DECOMPOSED_E,
        "y".repeat(9)
    );
    let cluster_line = format!("{}{}w", "z".repeat(59), JAMO);
    terminal.output(format!("\r\n{}\r\n{}\r\nRESIZE-END", group_line, cluster_line).as_bytes());
    zellij.wait_until("the clusters rendered", |grid_snapshot| {
        grid_snapshot.contains(&group_line)
            && grid_snapshot.contains(&cluster_line)
            && grid_snapshot.contains("RESIZE-END")
    });

    zellij.resize(Size {
        cols: 60,
        rows: TERMINAL_SIZE.rows,
    });
    terminal.wait_for_size("the pane narrowed", |cols, _| cols == 60);
    let group_head = format!("{}\u{1f468}\u{200d}", "x".repeat(57));
    let group_tail = format!("\u{1f469}\u{200d}\u{1f467}{}{}", DECOMPOSED_E, "y".repeat(9));
    let cluster_head = "z".repeat(59);
    let cluster_tail = format!("{}w", JAMO);
    zellij.wait_until("the clusters rewrapped whole", |grid_snapshot| {
        line_followed_by(grid_snapshot, &group_head, &group_tail)
            && line_followed_by(grid_snapshot, &cluster_head, &cluster_tail)
    });

    zellij.resize(TERMINAL_SIZE);
    terminal.wait_for_size("the pane widened", |cols, _| {
        cols as usize == TERMINAL_SIZE.cols
    });
    zellij.wait_until("the clusters joined again", |grid_snapshot| {
        grid_snapshot.contains(&group_line) && grid_snapshot.contains(&cluster_line)
    });
    zellij.quit();
}

#[test]
fn clusters_survive_session_resurrection() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_config("session_serialization true\nserialize_pane_viewport true")
        .start();
    let terminal = zellij.expect_pty_spawn();
    let line = format!(
        "RES:{}|{}|{}|{}|{}|END",
        FAMILY, DECOMPOSED_E, FLAG, KSSA, JAMO
    );
    terminal.output(format!("{}\r\n", line).as_bytes());
    zellij.wait_until("clusters rendered before serialization", |grid_snapshot| {
        grid_snapshot.contains(&line) && grid_snapshot.status_bar_appears()
    });

    zellij.save_session();
    zellij.wait_for_serialized_session();
    zellij.quit();

    zellij.resurrect(TERMINAL_SIZE);
    let grid_snapshot = zellij.wait_until("resurrected clusters rendered", |grid_snapshot| {
        grid_snapshot.contains(&line)
            && grid_snapshot.tab_bar_appears()
            && grid_snapshot.status_bar_appears()
    });
    assert_eq!(
        grid_snapshot
            .lines()
            .iter()
            .filter(|row| row.contains(&line))
            .count(),
        1,
        "the restored line appears once with every cluster intact"
    );
    zellij.quit();
}

#[test]
fn pane_rename_with_clusters_lines_up_in_full_frames() {
    let mut zellij = start_zellij_with_grapheme_aware_host("pane_frame_style \"full\"");
    claim_terminal_and_wait_for_bars(&zellij);
    open_right_pane(&zellij, |grid_snapshot| grid_snapshot.contains("Pane #2"));
    rename_focused_pane_by_typing(&zellij);
    let grid_snapshot = wait_for_renamed_pane(&zellij, true);
    let title_row = grid_snapshot
        .row_of_line(RENAMED)
        .expect("the new name is shown");
    assert_name_kept_whole(&grid_snapshot.lines()[title_row]);
    assert_frame_row_lines_up(&grid_snapshot, title_row, &['┌', '┐'], title_row + 1);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn pane_rename_with_clusters_lines_up_in_title_frames() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    split_right_and_wait_for_prompt(&zellij);
    rename_focused_pane_by_typing(&zellij);
    let grid_snapshot = wait_for_renamed_pane(&zellij, true);
    let title_row = grid_snapshot
        .row_of_line(RENAMED)
        .expect("the new name is shown");
    assert_name_kept_whole(&grid_snapshot.lines()[title_row]);
    assert_frame_row_lines_up(&grid_snapshot, title_row, &['│'], title_row + 1);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn pane_rename_with_clusters_keeps_borders_without_pane_frames() {
    let mut zellij = start_zellij_with_grapheme_aware_host("pane_frames false");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    open_right_pane(&zellij, |grid_snapshot| {
        grid_snapshot.cursor_is_at(col(62).row(1))
    });
    rename_focused_pane_by_typing(&zellij);
    let grid_snapshot = wait_for_renamed_pane(&zellij, false);
    let lines = grid_snapshot.lines();
    let expected = grapheme_mode_columns_of(&lines[1], &['│']);
    assert_eq!(
        expected.len(),
        1,
        "one border separates the panes:\n{}",
        grid_snapshot
    );
    for row in 1..TERMINAL_SIZE.rows - 2 {
        assert_eq!(
            grapheme_mode_columns_of(&lines[row], &['│']),
            expected,
            "the border must stay in one column on row {}:\n{}",
            row,
            grid_snapshot
        );
    }
    assert!(
        !grid_snapshot.contains("\u{1f1e9}"),
        "the removed flag must not appear anywhere"
    );
    zellij.quit();
}

#[test]
fn pane_rename_with_clusters_lines_up_in_a_stack() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('s'));
    let stacked = zellij.expect_pty_spawn();
    stacked.output(PROMPT);
    zellij.wait_until("stacked pane added and selected", |grid_snapshot| {
        grid_snapshot.status_bar_appears() && grid_snapshot.contains("> │ Pane #2")
    });
    rename_focused_pane_by_typing(&zellij);
    let grid_snapshot = wait_for_renamed_pane(&zellij, true);
    let lines = grid_snapshot.lines();
    let renamed_row = lines
        .iter()
        .position(|line| line.contains(RENAMED) && line.contains('│'))
        .expect("the new name is shown");
    let other_row = lines
        .iter()
        .position(|line| line.contains("Pane #1") && line.contains('│'))
        .expect("the other stack entry is shown");
    assert_name_kept_whole(&lines[renamed_row]);
    let renamed_borders = grapheme_mode_columns_of(&lines[renamed_row], &['│']);
    assert_eq!(
        renamed_borders.len(),
        2,
        "the renamed entry keeps both borders:\n{}",
        grid_snapshot
    );
    assert_eq!(
        renamed_borders,
        grapheme_mode_columns_of(&lines[other_row], &['│']),
        "stack entries share one width:\n{}",
        grid_snapshot
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn pane_rename_with_clusters_lines_up_in_a_floating_pane() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.send_stdin(&keys::ctrl('p'));
    zellij.send_stdin(&keys::key('w'));
    let floating = zellij.expect_pty_spawn();
    floating.output(PROMPT);
    zellij.wait_until("floating pane opened", |grid_snapshot| {
        grid_snapshot.contains("PIN [ ]") && grid_snapshot.status_bar_appears()
    });
    rename_focused_pane_by_typing(&zellij);
    let grid_snapshot = wait_for_renamed_pane(&zellij, true);
    let title_row = grid_snapshot
        .row_of_line(RENAMED)
        .expect("the new name is shown");
    assert_name_kept_whole(&grid_snapshot.lines()[title_row]);
    assert_frame_row_lines_up(&grid_snapshot, title_row, &['┌', '┐'], title_row + 1);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn tab_names_with_clusters_keep_tab_widths_and_click_regions() {
    let mut zellij = start_zellij_with_grapheme_aware_host("mouse_mode true");
    let first = claim_first_terminal_and_wait_for_prompt(&zellij);
    first.output(b"first-tab-content");
    zellij.wait_until("first tab content rendered", |grid_snapshot| {
        grid_snapshot.contains("first-tab-content")
    });
    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('n'));
    let second = zellij.expect_pty_spawn();
    second.output(b"$ second-tab-content");
    zellij.wait_until("second tab opened", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && grid_snapshot.contains("second-tab-content")
            && grid_snapshot.contains("Tab #2")
    });

    let cli_name_shown = format!("c{}[7md{}", FLAG, KSSA);
    zellij.run_cli_action(CliAction::RenameTab {
        name: format!("c{}\u{1b}[7md{}", FLAG, KSSA),
        tab_id: Some(0),
    });
    let grid_snapshot = zellij.wait_until("tab renamed through the cli", |grid_snapshot| {
        grid_snapshot
            .lines()
            .first()
            .map_or(false, |tab_bar| {
                tab_bar.contains(&cli_name_shown) && tab_bar.contains("Tab #2")
            })
    });

    click_on(&zellij, &grid_snapshot, 0, &cli_name_shown);
    let grid_snapshot = zellij.wait_until(
        "the renamed tab selected by clicking its name",
        |grid_snapshot| {
            grid_snapshot.contains("first-tab-content")
                && !grid_snapshot.contains("second-tab-content")
        },
    );
    click_on(&zellij, &grid_snapshot, 0, "Tab #2");
    let grid_snapshot = zellij.wait_until(
        "the tab to the right of the renamed tab selected",
        |grid_snapshot| {
            grid_snapshot.contains("second-tab-content")
                && !grid_snapshot.contains("first-tab-content")
        },
    );
    click_on(&zellij, &grid_snapshot, 0, &cli_name_shown);
    zellij.wait_until("back on the renamed tab", |grid_snapshot| {
        grid_snapshot.contains("first-tab-content") && grid_snapshot.status_bar_appears()
    });

    zellij.send_stdin(&keys::ctrl('t'));
    zellij.send_stdin(&keys::key('r'));
    zellij.wait_until("rename tab mode active", |grid_snapshot| {
        grid_snapshot.contains("RENAMING TAB")
    });
    type_text(&zellij, TYPED_NAME);
    zellij.send_stdin(&BACKSPACE);
    zellij.send_stdin(&keys::ENTER);
    let grid_snapshot = zellij.wait_until("tab renamed by typing", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
            && !grid_snapshot.contains("RENAMING TAB")
            && grid_snapshot
                .lines()
                .first()
                .map_or(false, |tab_bar| tab_bar.contains(RENAMED))
    });
    assert_name_kept_whole(&grid_snapshot.lines()[0]);
    click_on(&zellij, &grid_snapshot, 0, "Tab #2");
    let grid_snapshot = zellij.wait_until(
        "the tab to the right of the typed name selected",
        |grid_snapshot| {
            grid_snapshot.contains("second-tab-content")
                && !grid_snapshot.contains("first-tab-content")
                && grid_snapshot.status_bar_appears()
                && !grid_snapshot.contains("RENAMING TAB")
        },
    );
    assert!(
        columns_in_grapheme_mode(grid_snapshot.lines()[0].trim_end()) <= TERMINAL_SIZE.cols,
        "the tab bar must fit the screen, got {:?}",
        grid_snapshot.lines()[0]
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn the_compact_bar_cuts_a_too_wide_active_tab_with_an_ellipsis() {
    let mut zellij = TestRunner::new(TERMINAL_SIZE)
        .with_layout(LayoutInfo::BuiltIn("compact".to_owned()))
        .with_host_terminal(HostTerminal::GraphemeClusterAware(2))
        .start();
    zellij.wait_until_raw_output(
        "the client turned on grapheme cluster mode in its host terminal",
        |bytes| contains_bytes(bytes, ENABLE_GRAPHEME_CLUSTER_MODE),
    );
    let terminal = zellij.expect_pty_spawn();
    terminal.output(PROMPT);
    zellij.wait_until("compact bar rendered", |grid_snapshot| {
        grid_snapshot.contains("NORMAL") && grid_snapshot.contains("Tab #1")
    });

    zellij.run_cli_action(CliAction::RenameTab {
        name: format!("LONG{}TAIL", FAMILY.repeat(70)),
        tab_id: Some(0),
    });
    let grid_snapshot = zellij.wait_until("compact bar cut the active tab", |grid_snapshot| {
        grid_snapshot.lines().last().map_or(false, |bar| {
            bar.contains("LONG") && bar.contains('\u{2026}')
        })
    });
    let bar = grid_snapshot.lines().last().cloned().unwrap();
    assert!(
        !bar.contains("TAIL"),
        "the end of the too-wide name must be cut, got {:?}",
        bar
    );
    assert!(
        families_are_whole(&bar),
        "the name must be cut on a cluster boundary, got {:?}",
        bar
    );
    assert!(
        columns_in_grapheme_mode(bar.trim_end()) <= TERMINAL_SIZE.cols,
        "the compact bar must fit the screen, got {:?}",
        bar
    );
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn terminal_titles_with_clusters_show_in_frames_and_are_cut_on_cluster_boundaries() {
    let mut zellij = start_zellij_with_grapheme_aware_host("");
    let left = claim_first_terminal_and_wait_for_prompt(&zellij);
    let right = split_right_and_wait_for_prompt(&zellij);

    let left_title = format!("T0{}{}", FAMILY, DECOMPOSED_E);
    left.output(format!("\x1b]0;{}\x07", left_title).as_bytes());
    let right_title = format!("T2{}{}", FLAG, KSSA);
    right.output(format!("\x1b]2;{}\x07", right_title).as_bytes());
    let grid_snapshot = zellij.wait_until("titles shown in the frames", |grid_snapshot| {
        grid_snapshot
            .lines()
            .get(1)
            .map_or(false, |row| row.contains(&left_title) && row.contains(&right_title))
    });
    assert_frame_row_lines_up(&grid_snapshot, 1, &['│'], 2);

    let long_title_start = format!("L{}", FAMILY);
    left.output(format!("\x1b]2;L{}TAIL\x07", FAMILY.repeat(40)).as_bytes());
    let grid_snapshot = zellij.wait_until("long title cut", |grid_snapshot| {
        grid_snapshot.lines().get(1).map_or(false, |row| {
            row.contains(&long_title_start) && !row.contains(&left_title)
        })
    });
    let title_row = grid_snapshot.lines()[1].clone();
    assert!(
        families_are_whole(&title_row),
        "the title must be cut on a cluster boundary, got {:?}",
        title_row
    );
    assert!(
        columns_in_grapheme_mode(title_row.trim_end()) <= TERMINAL_SIZE.cols,
        "the title row must fit the screen, got {:?}",
        title_row
    );
    assert!(
        title_row.contains(&right_title),
        "the neighbouring title must stay intact, got {:?}",
        title_row
    );
    assert_frame_row_lines_up(&grid_snapshot, 1, &['│'], 2);
    assert_snapshot!(normalized(&grid_snapshot));
    zellij.quit();
}

#[test]
fn a_watcher_renders_clusters() {
    let mut zellij = start_zellij();
    let terminal = claim_first_terminal_and_wait_for_prompt(&zellij);
    terminal.disable_echo();
    let watcher = zellij.attach_watcher(TERMINAL_SIZE);
    watcher.wait_until("watcher connected", |grid_snapshot| {
        grid_snapshot.status_bar_appears()
    });

    let line = format!(
        "V:{}|{}|{}|\u{4e16}|{}|{}|END",
        FAMILY, DECOMPOSED_E, FLAG, KSSA, THUMBS
    );
    terminal.output(format!("\r\n{}\r\n", line).as_bytes());
    let watched = watcher.wait_until("watcher shows the clusters", |grid_snapshot| {
        grid_snapshot.contains(&line)
    });
    let main = zellij.wait_until("main client shows the clusters", |grid_snapshot| {
        grid_snapshot.contains(&line)
    });
    assert_eq!(
        line_containing(&watched, "V:"),
        line_containing(&main, "V:"),
        "the watcher and the main client must show the same clusters"
    );
    watcher.quit();
    zellij.quit();
}

#[test]
fn a_plugin_pane_stores_a_zwj_emoji_as_one_two_column_cluster() {
    let mut zellij = start_zellij();
    claim_first_terminal_and_wait_for_prompt(&zellij);
    zellij.run_cli_action(CliAction::RenameTab {
        name: format!("P{}Q", FAMILY),
        tab_id: Some(0),
    });
    let raw_output = zellij.wait_until_raw_output(
        "the tab bar drew the family emoji as one two-column cell",
        |bytes| contains_complex_unit(bytes, 2, FAMILY),
    );
    assert!(
        !contains_bytes(&raw_output, format!("\x1b[6X{}", FAMILY).as_bytes()),
        "a plugin pane starts in grapheme cluster mode, so the family is never a six-column group"
    );
    zellij.quit();
}
