use super::super::{Grid, Row};
use crate::panes::cluster_table;
use crate::panes::grapheme_join::{self, JOIN_CLASS_TABLE};
use crate::panes::grid::SixelImageStore;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::link_handler::LinkHandler;
use insta::assert_snapshot;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use unicode_segmentation::GraphemeCursor;
use unicode_width::UnicodeWidthChar;
use zellij_utils::consts::SCROLL_BUFFER_SIZE;
use zellij_utils::data::{Palette, Style};
use zellij_utils::position::Position;

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
const THUMBS_TONE: &str = "\u{1F44D}\u{1F3FD}";
const FLAG_DE: &str = "\u{1F1E9}\u{1F1EA}";
const RAINBOW: &str = "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}";
const ENGLAND: &str = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
const KSSA: &str = "\u{0915}\u{094D}\u{0937}";
const KAA: &str = "\u{0915}\u{093E}";
const THAI: &str = "\u{0E01}\u{0E49}\u{0E33}";
const JAMO_LVT: &str = "\u{1100}\u{1161}\u{11A8}";

fn new_grid(rows: usize, cols: usize) -> Grid {
    Grid::new(
        rows,
        cols,
        Rc::new(RefCell::new(Palette::default())),
        Rc::new(RefCell::new(HashMap::new())),
        Rc::new(RefCell::new(LinkHandler::new())),
        Rc::new(RefCell::new(None)),
        Rc::new(RefCell::new(SixelImageStore::default())),
        Rc::new(RefCell::new(KittyImageStore::default())),
        Style::default(),
        false,
        true,
        true,
        true,
        false,
    )
}

fn feed(grid: &mut Grid, content: &str) {
    let mut parser = vte::Parser::new();
    parser.advance(grid, content.as_bytes());
}

fn grid_with(content: &str, grapheme_mode: bool) -> Grid {
    let mut grid = new_grid(10, 20);
    if grapheme_mode {
        feed(&mut grid, "\u{1b}[?2027h");
    }
    feed(&mut grid, content);
    grid
}

fn cells(grid: &Grid, row: usize) -> Vec<(String, usize)> {
    grid.viewport[row]
        .columns
        .iter()
        .map(|cell| (cell.text().into_owned(), cell.width()))
        .collect()
}

fn cell(text: &str, width: usize) -> (String, usize) {
    (text.to_string(), width)
}

fn cursor(grid: &Grid) -> (usize, usize) {
    (grid.cursor.x, grid.cursor.y)
}

fn assert_cached_width_is_fresh(row: &mut Row) {
    let fresh: usize = row.columns.iter().map(|c| c.width()).sum();
    assert_eq!(row.width_cached(), fresh);
    assert_eq!(row.width(), fresh);
}

fn check(content: &str, grapheme_mode: bool, expected: &[(&str, usize)]) {
    let mut grid = grid_with(content, grapheme_mode);
    let expected: Vec<(String, usize)> = expected.iter().map(|(t, w)| cell(t, *w)).collect();
    assert_eq!(
        cells(&grid, 0),
        expected,
        "content={:?} grapheme_mode={}",
        content,
        grapheme_mode
    );
    let columns: usize = expected.iter().map(|(_, w)| *w).sum();
    assert_eq!(cursor(&grid), (columns, 0), "content={:?}", content);
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
}

#[test]
fn every_category_in_default_mode() {
    check("abc", false, &[("a", 1), ("b", 1), ("c", 1)]);
    check("\u{4F60}\u{597D}", false, &[("\u{4F60}", 2), ("\u{597D}", 2)]);
    check("e\u{0301}", false, &[("e\u{0301}", 1)]);
    check("\u{26A0}\u{FE0F}", false, &[("\u{26A0}\u{FE0F}", 1)]);
    check("\u{231A}\u{FE0E}", false, &[("\u{231A}\u{FE0E}", 2)]);
    check("\u{1F600}\u{FE0E}", false, &[("\u{1F600}\u{FE0E}", 2)]);
    check("1\u{FE0F}\u{20E3}", false, &[("1\u{FE0F}\u{20E3}", 1)]);
    check(THUMBS_TONE, false, &[("\u{1F44D}", 2), ("\u{1F3FD}", 2)]);
    check("\u{1F3FD}", false, &[("\u{1F3FD}", 2)]);
    check(FLAG_DE, false, &[("\u{1F1E9}", 1), ("\u{1F1EA}", 1)]);
    check(
        "\u{1F1E9}\u{1F1EA}\u{1F1EB}",
        false,
        &[("\u{1F1E9}", 1), ("\u{1F1EA}", 1), ("\u{1F1EB}", 1)],
    );
    check(
        FAMILY,
        false,
        &[
            ("\u{1F468}\u{200D}", 2),
            ("\u{1F469}\u{200D}", 2),
            ("\u{1F467}", 2),
        ],
    );
    check(
        RAINBOW,
        false,
        &[("\u{1F3F3}\u{FE0F}\u{200D}", 1), ("\u{1F308}", 2)],
    );
    check(ENGLAND, false, &[(ENGLAND, 2)]);
    check(KSSA, false, &[("\u{0915}\u{094D}", 1), ("\u{0937}", 1)]);
    check(KAA, false, &[("\u{0915}", 1), ("\u{093E}", 1)]);
    check(THAI, false, &[("\u{0E01}\u{0E49}", 1), ("\u{0E33}", 1)]);
    check(JAMO_LVT, false, &[(JAMO_LVT, 2)]);
    check("\u{FF76}\u{FF9E}", false, &[("\u{FF76}\u{FF9E}", 1)]);
    check("a\u{00AD}b", false, &[("a\u{00AD}", 1), ("b", 1)]);
    check("a\u{200D}b", false, &[("a\u{200D}", 1), ("b", 1)]);
    check("\u{17D8}", false, &[("\u{17D8}", 3)]);
}

#[test]
fn every_category_in_grapheme_mode() {
    check("abc", true, &[("a", 1), ("b", 1), ("c", 1)]);
    check("\u{4F60}\u{597D}", true, &[("\u{4F60}", 2), ("\u{597D}", 2)]);
    check("e\u{0301}", true, &[("e\u{0301}", 1)]);
    check("\u{26A0}\u{FE0F}", true, &[("\u{26A0}\u{FE0F}", 2)]);
    check("\u{1F600}\u{FE0E}", true, &[("\u{1F600}\u{FE0E}", 2)]);
    check("1\u{FE0F}\u{20E3}", true, &[("1\u{FE0F}\u{20E3}", 2)]);
    check(THUMBS_TONE, true, &[(THUMBS_TONE, 2)]);
    check("\u{1F3FD}", true, &[("\u{1F3FD}", 2)]);
    check(FLAG_DE, true, &[(FLAG_DE, 2)]);
    check(
        "\u{1F1E9}\u{1F1EA}\u{1F1EB}",
        true,
        &[(FLAG_DE, 2), ("\u{1F1EB}", 1)],
    );
    check(FAMILY, true, &[(FAMILY, 2)]);
    check(RAINBOW, true, &[(RAINBOW, 2)]);
    check(ENGLAND, true, &[(ENGLAND, 2)]);
    check(KSSA, true, &[(KSSA, 2)]);
    check(KAA, true, &[(KAA, 2)]);
    check(THAI, true, &[(THAI, 2)]);
    check(JAMO_LVT, true, &[(JAMO_LVT, 2)]);
    check("\u{FF76}\u{FF9E}", true, &[("\u{FF76}\u{FF9E}", 1)]);
    check("a\u{00AD}b", true, &[("a\u{00AD}", 1), ("b", 1)]);
    check("a\u{200D}b", true, &[("a\u{200D}", 1), ("b", 1)]);
    check("\u{17D8}", true, &[("\u{17D8}", 3)]);
}

#[test]
fn hangul_leading_jamo_joins_a_precomposed_syllable_in_grapheme_mode() {
    let grid = grid_with("\u{1100}\u{AC00}", true);
    let row = cells(&grid, 0);
    assert_eq!(row.len(), 1);
    assert_eq!(row[0].0, "\u{1100}\u{AC00}");
    assert_eq!(row[0].1, grid.viewport[0].columns[0].natural_width().max(1));
}

#[test]
fn a_zero_width_code_point_with_nothing_on_its_left_is_dropped() {
    let grid = grid_with("\u{0301}a", false);
    assert_eq!(cells(&grid, 0), vec![cell("a", 1)]);
}

#[test]
fn a_mark_after_cursor_back_attaches_to_the_character_left_of_the_cursor() {
    let grid = grid_with("abc\u{1b}[D\u{0301}", false);
    assert_eq!(
        cells(&grid, 0),
        vec![cell("a", 1), cell("b\u{0301}", 1), cell("c", 1)]
    );
    assert_eq!(cursor(&grid), (2, 0));
}

#[test]
fn a_mark_at_column_zero_is_dropped() {
    let grid = grid_with("abc\r\u{0301}", false);
    assert_eq!(
        cells(&grid, 0),
        vec![cell("a", 1), cell("b", 1), cell("c", 1)]
    );
}

#[test]
fn a_mark_in_the_pending_wrap_state_attaches_to_the_last_cell_of_the_line() {
    let mut grid = new_grid(5, 4);
    feed(&mut grid, "abcd\u{0301}");
    assert_eq!(cells(&grid, 0)[3], cell("d\u{0301}", 1));
    assert_eq!(cursor(&grid), (4, 0));
    feed(&mut grid, "e");
    assert_eq!(cells(&grid, 1), vec![cell("e", 1)]);
}

#[test]
fn a_mark_after_a_wide_cell_attaches_to_the_wide_cell() {
    let grid = grid_with("\u{4F60}\u{0301}x", false);
    assert_eq!(cells(&grid, 0), vec![cell("\u{4F60}\u{0301}", 2), cell("x", 1)]);
}

#[test]
fn grapheme_mode_widening_overwrites_the_cell_to_the_right() {
    let grid = grid_with("\u{26A0}X\u{1b}[2G\u{FE0F}", true);
    assert_eq!(cells(&grid, 0), vec![cell("\u{26A0}\u{FE0F}", 2)]);
    assert_eq!(cursor(&grid), (2, 0));
}

#[test]
fn grapheme_mode_widening_over_half_of_a_wide_cell_leaves_a_blank() {
    let mut grid = grid_with("\u{26A0}\u{4F60}\u{1b}[2G\u{FE0F}", true);
    assert_eq!(
        cells(&grid, 0),
        vec![cell("\u{26A0}\u{FE0F}", 2), cell(" ", 1)]
    );
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
}

#[test]
fn grapheme_mode_widening_at_the_right_edge_wraps_the_cell() {
    let mut grid = new_grid(5, 5);
    feed(&mut grid, "\u{1b}[?2027habcd\u{26A0}\u{FE0F}");
    assert_eq!(
        cells(&grid, 0),
        vec![
            cell("a", 1),
            cell("b", 1),
            cell("c", 1),
            cell("d", 1),
            cell(" ", 1)
        ]
    );
    assert_eq!(cells(&grid, 1), vec![cell("\u{26A0}\u{FE0F}", 2)]);
    assert_eq!(cursor(&grid), (2, 1));
    assert!(!grid.viewport[1].is_canonical);
}

#[test]
fn grapheme_mode_widening_without_auto_wrap_keeps_the_width() {
    let mut grid = new_grid(5, 5);
    feed(&mut grid, "\u{1b}[?2027h\u{1b}[?7labcd\u{26A0}\u{FE0F}");
    assert_eq!(cells(&grid, 0)[4], cell("\u{26A0}\u{FE0F}", 1));
    assert_eq!(grid.viewport.len(), 1);
}

#[test]
fn grapheme_mode_drops_a_code_point_that_makes_a_cluster_wider_than_the_pane() {
    let mut grid = new_grid(5, 1);
    feed(&mut grid, "\u{1b}[?2027h\u{26A0}\u{FE0F}");
    assert_eq!(cells(&grid, 0), vec![cell("\u{26A0}", 1)]);
}

#[test]
fn vs15_narrows_a_cell_in_grapheme_mode() {
    let mut grid = grid_with("\u{231A}\u{FE0E}Y", true);
    assert_eq!(cells(&grid, 0), vec![cell("\u{231A}\u{FE0E}", 1), cell("Y", 1)]);
    assert_eq!(cursor(&grid), (2, 0));
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
    let grid = grid_with("\u{1F600}\u{FE0E}", true);
    assert_eq!(cells(&grid, 0), vec![cell("\u{1F600}\u{FE0E}", 2)]);
}

#[test]
fn grapheme_mode_can_be_queried_and_switched() {
    let mut grid = new_grid(5, 20);
    feed(&mut grid, "\u{1b}[?2027$p");
    feed(&mut grid, "\u{1b}[?2027h\u{1b}[?2027$p");
    let replies: Vec<String> = grid
        .pending_messages_to_pty
        .iter()
        .map(|m| String::from_utf8_lossy(m).to_string())
        .collect();
    assert_eq!(
        replies,
        vec!["\u{1b}[?2027;2$y".to_string(), "\u{1b}[?2027;1$y".to_string()]
    );
    feed(&mut grid, "\u{1b}[?1049h");
    assert!(grid.grapheme_mode);
    feed(&mut grid, "\u{1b}[?1049l");
    assert!(grid.grapheme_mode);
    feed(&mut grid, "\u{1b}[?2027l");
    assert!(!grid.grapheme_mode);
    feed(&mut grid, "\u{1b}[?2027h\u{1b}c");
    assert!(!grid.grapheme_mode);
}

#[test]
fn switching_grapheme_mode_leaves_existing_cells_alone() {
    let mut grid = grid_with(THUMBS_TONE, false);
    feed(&mut grid, "\u{1b}[?2027h");
    assert_eq!(
        cells(&grid, 0),
        vec![cell("\u{1F44D}", 2), cell("\u{1F3FD}", 2)]
    );
}

#[test]
fn rep_repeats_a_full_cluster() {
    let grid = grid_with("e\u{0301}\u{1b}[2b", false);
    assert_eq!(cells(&grid, 0), vec![cell("e\u{0301}", 1); 3]);
    let grid = grid_with(&format!("{}\u{1b}[1b", THUMBS_TONE), true);
    assert_eq!(cells(&grid, 0), vec![cell(THUMBS_TONE, 2); 2]);
    assert_eq!(cursor(&grid), (4, 0));
}

#[test]
fn rep_after_a_lone_mark_repeats_nothing_visible() {
    let grid = grid_with("\u{0301}\u{1b}[3b", false);
    assert!(grid.viewport[0].columns.iter().all(|c| c.is_plain_blank()));
    assert_eq!(cursor(&grid), (0, 0));
}

#[test]
fn insert_mode_keeps_attached_code_points_and_widening_pushes_the_rest_right() {
    let grid = grid_with("abc\u{1b}[1G\u{1b}[4hX\u{0301}", false);
    assert_eq!(
        cells(&grid, 0),
        vec![cell("X\u{0301}", 1), cell("a", 1), cell("b", 1), cell("c", 1)]
    );
    let mut grid = grid_with("ab\u{1b}[1G\u{1b}[4h\u{26A0}\u{FE0F}", true);
    assert_eq!(
        cells(&grid, 0),
        vec![cell("\u{26A0}\u{FE0F}", 2), cell("a", 1), cell("b", 1)]
    );
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
}

#[test]
fn deleting_inserting_and_erasing_work_on_whole_cells() {
    let grid = grid_with("e\u{0301}\u{0302}xyz\u{1b}[1G\u{1b}[P", false);
    assert_eq!(
        cells(&grid, 0)[..3].to_vec(),
        vec![cell("x", 1), cell("y", 1), cell("z", 1)]
    );
    drop(grid);
    assert_eq!(cluster_table::live_entries(), 0);
    let grid = grid_with("a\u{0301}\u{0302}b\u{1b}[1G\u{1b}[X", false);
    assert_eq!(cells(&grid, 0)[..2].to_vec(), vec![cell(" ", 1), cell("b", 1)]);
    drop(grid);
    assert_eq!(cluster_table::live_entries(), 0);
    let grid = grid_with("a\u{0301}b\u{1b}[1G\u{1b}[@", false);
    assert_eq!(
        cells(&grid, 0)[..3].to_vec(),
        vec![cell(" ", 1), cell("a\u{0301}", 1), cell("b", 1)]
    );
    let grid = grid_with(&format!("x{}y\u{1b}[3G\u{1b}[K", FAMILY), false);
    assert_eq!(
        cells(&grid, 0)[..1].to_vec(),
        vec![cell("x", 1)]
    );
    assert!(grid.viewport[0].columns[1..].iter().all(|c| c.is_plain_blank()));
}

#[test]
fn a_letter_keeps_at_most_thirty_two_code_points() {
    let content = format!("a{}", "\u{0301}".repeat(33));
    let grid = grid_with(&content, false);
    assert_eq!(grid.viewport[0].columns.len(), 1);
    assert_eq!(grid.viewport[0].columns[0].code_point_count(), 32);
    assert_eq!(cursor(&grid), (1, 0));
}

#[test]
fn rows_that_may_hold_groups_are_flagged() {
    for content in [FAMILY, THUMBS_TONE, FLAG_DE, KSSA, KAA] {
        let grid = grid_with(content, false);
        assert!(grid.viewport[0].may_have_groups, "content={:?}", content);
    }
    for content in ["plain ascii", "\u{4F60}\u{597D}", "e\u{0301}"] {
        let grid = grid_with(content, false);
        assert!(!grid.viewport[0].may_have_groups, "content={:?}", content);
    }
}

#[test]
fn the_group_flag_survives_rewrap_joins_and_splits() {
    let mut grid = new_grid(5, 10);
    feed(&mut grid, &format!("abcdefgh{}", FAMILY));
    assert!(grid.viewport[0].may_have_groups || grid.viewport[1].may_have_groups);
    grid.change_size(5, 20);
    assert!(grid.viewport[0].may_have_groups);
    assert_eq!(grid.viewport[0].columns.len(), 11);
    grid.change_size(5, 6);
    assert!(grid.viewport.iter().take(3).all(|row| row.may_have_groups));
    let total: usize = grid.viewport.iter().map(|row| row.columns.len()).sum();
    assert_eq!(total, 11);
}

#[test]
fn rewrap_keeps_cells_whole_and_a_widened_cell_wraps_whole() {
    let mut grid = new_grid(5, 10);
    feed(&mut grid, "\u{1b}[?2027habcdefgh\u{26A0}\u{FE0F}x");
    assert_eq!(cells(&grid, 0)[8], cell("\u{26A0}\u{FE0F}", 2));
    grid.change_size(5, 9);
    let first: Vec<(String, usize)> = cells(&grid, 0);
    assert_eq!(first.len(), 8);
    assert_eq!(cells(&grid, 1)[0], cell("\u{26A0}\u{FE0F}", 2));
}

#[test]
fn a_space_with_an_attached_mark_is_not_trimmed_on_resize() {
    let mut grid = new_grid(5, 10);
    feed(&mut grid, "ab \u{0301}");
    grid.change_size(5, 5);
    grid.change_size(5, 10);
    let text: String = cells(&grid, 0).into_iter().map(|(t, _)| t).collect();
    assert!(text.starts_with("ab \u{0301}"), "text={:?}", text);
}

#[test]
fn clearing_and_scrolling_out_free_the_cluster_table() {
    let mut grid = new_grid(5, 20);
    feed(&mut grid, "a\u{0301}\u{0302}\u{0303}x\r\n");
    assert_eq!(cluster_table::live_entries(), 1);
    feed(&mut grid, "\u{1b}[2J\u{1b}[3J");
    assert_eq!(cluster_table::live_entries(), 0);
    assert_eq!(cluster_table::bytes(), 0);
    feed(&mut grid, "b\u{0301}\u{0302}\u{0303}y\r\n");
    assert_eq!(cluster_table::live_entries(), 1);
    let scroll_buffer_size = *SCROLL_BUFFER_SIZE.get().unwrap();
    for _ in 0..(scroll_buffer_size + grid.height + 1) {
        grid.add_canonical_line();
    }
    assert_eq!(cluster_table::live_entries(), 0);
    drop(grid);
    assert_eq!(cluster_table::bytes(), 0);
}

#[test]
fn dropping_a_grid_full_of_clusters_empties_the_table() {
    let mut grid = new_grid(10, 20);
    let mut content = String::new();
    for i in 0..150 {
        content.push((b'a' + (i % 26) as u8) as char);
        content.push(char::from_u32(0x300 + (i % 0x60) as u32).unwrap());
        content.push(char::from_u32(0x301 + (i % 0x50) as u32).unwrap());
    }
    feed(&mut grid, &content);
    assert!(cluster_table::live_entries() > 0);
    drop(grid);
    assert_eq!(cluster_table::live_entries(), 0);
}

#[test]
fn width_three_cells_are_replaced_deleted_erased_and_inserted_around() {
    let mut grid = grid_with("\u{17D8}x\u{1b}[2GY", false);
    assert_eq!(
        cells(&grid, 0),
        vec![cell(" ", 1), cell("Y", 1), cell(" ", 1), cell("x", 1)]
    );
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
    let grid = grid_with("a\u{17D8}b\u{1b}[3G\u{1b}[P", false);
    assert_eq!(cells(&grid, 0)[..2].to_vec(), vec![cell("a", 1), cell("b", 1)]);
    let mut grid = grid_with("a\u{17D8}b\u{1b}[3G\u{1b}[X", false);
    assert_eq!(
        cells(&grid, 0)[..5].to_vec(),
        vec![
            cell("a", 1),
            cell(" ", 1),
            cell(" ", 1),
            cell(" ", 1),
            cell("b", 1)
        ]
    );
    assert_cached_width_is_fresh(&mut grid.viewport[0]);
    let grid = grid_with("a\u{17D8}b\u{1b}[3G\u{1b}[@", false);
    let text: String = cells(&grid, 0).into_iter().map(|(t, _)| t).collect();
    assert!(text.starts_with("a \u{17D8}b"), "text={:?}", text);
    assert_eq!(grid.viewport[0].absolute_character_index(4), 2);
}

#[test]
fn copying_keeps_every_category_whole() {
    let samples = [
        "e\u{0301}",
        "\u{26A0}\u{FE0F}",
        THUMBS_TONE,
        FLAG_DE,
        FAMILY,
        RAINBOW,
        ENGLAND,
        KSSA,
        THAI,
        JAMO_LVT,
    ];
    for grapheme_mode in [false, true] {
        let line = samples.join(" ");
        let mut grid = new_grid(5, 80);
        if grapheme_mode {
            feed(&mut grid, "\u{1b}[?2027h");
        }
        feed(&mut grid, &line);
        let copied = grid
            .text_in_range(Position::new(0, 0), Position::new(0, 80))
            .unwrap();
        assert_eq!(copied.trim_end(), line, "grapheme_mode={}", grapheme_mode);
    }
}

#[test]
fn a_group_split_by_a_line_wrap_is_copied_whole() {
    let mut grid = new_grid(5, 5);
    feed(&mut grid, &format!("abc{}", FAMILY));
    let copied = grid
        .text_in_range(Position::new(0, 0), Position::new(1, 5))
        .unwrap();
    assert_eq!(copied.trim_end(), format!("abc{}", FAMILY));
}

#[test]
fn a_selection_edge_inside_a_group_is_widened() {
    let mut grid = new_grid(5, 20);
    feed(&mut grid, &format!("x{}y", FAMILY));
    let copied = grid
        .text_in_range(Position::new(0, 3), Position::new(0, 8))
        .unwrap();
    assert_eq!(copied, format!("{}y", FAMILY));
    let copied = grid
        .text_in_range(Position::new(0, 0), Position::new(0, 4))
        .unwrap();
    assert_eq!(copied, format!("x{}", FAMILY));
}

#[test]
fn dump_screen_and_pane_contents_hold_full_clusters() {
    let mut grid = new_grid(5, 20);
    feed(&mut grid, &format!("a{}e\u{0301}\r\n\u{1b}[31m{}\u{1b}[0m", FAMILY, KSSA));
    let dump = grid.dump_screen(true);
    assert!(dump.contains(&format!("a{}e\u{0301}", FAMILY)), "dump={:?}", dump);
    assert!(dump.contains(KSSA));
    let dump_with_ansi = grid.dump_screen_with_ansi(true);
    assert!(dump_with_ansi.contains(KSSA));
    let contents = grid.pane_contents(false, None);
    assert_eq!(contents.viewport[0].trim_end(), format!("a{}e\u{0301}", FAMILY));
    let contents_with_ansi = grid.pane_contents_with_ansi(false, None);
    assert!(contents_with_ansi.viewport[1].contains(KSSA));
}

fn has_boundary(left: char, right: char) -> bool {
    let mut text = String::new();
    text.push(left);
    let offset = text.len();
    text.push(right);
    let mut cursor = GraphemeCursor::new(offset, text.len(), true);
    cursor.is_boundary(&text, 0).unwrap_or(true)
}

struct SampleRng(u64);

impl SampleRng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

#[test]
fn the_join_pre_check_never_misses_a_pair_without_a_boundary() {
    let mut sample: Vec<char> = Vec::new();
    for (lo, hi, _) in JOIN_CLASS_TABLE {
        for value in [*lo, *hi, lo.saturating_sub(1), hi + 1] {
            if let Some(c) = char::from_u32(value) {
                sample.push(c);
            }
        }
    }
    let mut rng = SampleRng(0x2027_2027_2027);
    for plane in [0u32, 1, 14] {
        let mut added = 0;
        while added < 10_000 {
            let value = plane * 0x10000 + (rng.next() % 0x10000) as u32;
            if let Some(c) = char::from_u32(value) {
                sample.push(c);
                added += 1;
            }
        }
    }
    sample.sort_unstable();
    sample.dedup();
    let partners: Vec<char> = [
        'a', '1', ' ', '\u{4F60}', '\u{1F468}', '\u{1F3FD}', '\u{200D}', '\u{FE0F}', '\u{0301}',
        '\u{1F1E9}', '\u{1100}', '\u{1161}', '\u{11A8}', '\u{AC00}', '\u{AC01}', '\u{093E}',
        '\u{0600}', '\u{0915}', '\u{094D}', '\u{0E33}', '\u{E0067}', '\u{20E3}', '\u{00AD}',
        '\u{1F3F3}', '\u{231A}', '\u{0E49}',
    ]
    .to_vec();
    for &c in &sample {
        for &p in &partners {
            if p.width().unwrap_or(0) != 0 && !has_boundary(c, p) {
                assert!(
                    grapheme_join::may_join(c, p),
                    "missed pair U+{:04X} U+{:04X}",
                    c as u32,
                    p as u32
                );
            }
            if c.width().unwrap_or(0) != 0 && !has_boundary(p, c) {
                assert!(
                    grapheme_join::may_join(p, c),
                    "missed pair U+{:04X} U+{:04X}",
                    p as u32,
                    c as u32
                );
            }
        }
    }
}

#[test]
fn the_join_pre_check_rejects_plain_text_quickly() {
    assert!(!grapheme_join::may_join('a', 'b'));
    assert!(!grapheme_join::may_join('\u{4F60}', '\u{597D}'));
    assert!(grapheme_join::may_join('\u{200D}', 'b'));
    assert!(grapheme_join::may_join('\u{1F44D}', '\u{1F3FD}'));
    assert!(grapheme_join::may_join('\u{0915}', '\u{093E}'));
}

fn fixture(name: &str) -> String {
    let path = std::path::PathBuf::from("../src/tests/fixtures").join(name);
    String::from_utf8(std::fs::read(&path).unwrap()).unwrap()
}

fn fixture_snapshot(name: &str) -> String {
    let mut grid = new_grid(30, 20);
    feed(&mut grid, &fixture(name));
    let mut out = format!("{:?}", grid);
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("cursor: {:?}\n", cursor(&grid)));
    for (index, row) in grid.viewport.iter().enumerate() {
        let widths: Vec<usize> = row.columns.iter().map(|c| c.width()).collect();
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("{:02} groups={} widths={:?}\n", index, row.may_have_groups, widths),
        );
    }
    out
}

#[test]
fn grapheme_clusters_fixture() {
    assert_snapshot!(fixture_snapshot("grapheme_clusters"));
}

#[test]
fn grapheme_clusters_2027_fixture() {
    assert_snapshot!(fixture_snapshot("grapheme_clusters_2027"));
}

#[test]
fn zalgo_fixture() {
    assert_snapshot!(fixture_snapshot("zalgo"));
}

#[test]
fn indic_thai_scripts_fixture() {
    assert_snapshot!(fixture_snapshot("indic_thai_scripts"));
}

#[test]
fn grapheme_wrap_edges_fixture() {
    assert_snapshot!(fixture_snapshot("grapheme_wrap_edges"));
}

#[test]
fn session_serialization_round_trips_clusters() {
    let mut grid = new_grid(5, 20);
    feed(
        &mut grid,
        &format!("a{}e\u{0301}\r\n{} {}", FAMILY, KSSA, ENGLAND),
    );
    let serialized = grid.serialize(None).unwrap();
    assert!(!serialized.contains("\u{1b}[6X"));
    let mut restored = new_grid(5, 20);
    feed(&mut restored, &serialized);
    let offset = restored
        .viewport
        .iter()
        .position(|row| !row.columns.is_empty())
        .unwrap();
    let trimmed = |row: Vec<(String, usize)>| -> Vec<(String, usize)> {
        let mut row = row;
        while row.last().map(|(t, _)| t == " ").unwrap_or(false) {
            row.pop();
        }
        row
    };
    assert_eq!(trimmed(cells(&restored, offset)), trimmed(cells(&grid, 0)));
    assert_eq!(trimmed(cells(&restored, offset + 1)), trimmed(cells(&grid, 1)));
}

#[test]
fn the_print_shortcuts_agree_with_the_general_width_and_join_rules() {
    for value in 0..=0x10FFFFu32 {
        let Some(c) = char::from_u32(value) else {
            continue;
        };
        let width = c.width().unwrap_or(0);
        assert_eq!(
            grapheme_join::width_and_may_group(c),
            (
                width,
                width != 0 && grapheme_join::new_cell_may_join_a_neighbour(c)
            ),
            "U+{:04X}",
            value
        );
    }
}

#[test]
fn plain_wide_characters_are_two_columns_and_never_join_a_neighbour() {
    let left_partners = ['a', ' ', '\u{4F60}', '\u{3042}', '\u{200D}', '\u{1F1E9}', '\u{1F468}'];
    let right_partners = ['a', ' ', '\u{4F60}', '\u{3042}', '\u{1F1E9}', '\u{1100}', '\u{AC00}'];
    for (lo, hi) in grapheme_join::PLAIN_WIDE_RANGES {
        for value in lo..=hi {
            let c = char::from_u32(value).unwrap();
            assert!(grapheme_join::is_plain_wide(c), "U+{:04X}", value);
            assert_eq!(c.width(), Some(2), "U+{:04X}", value);
            assert_eq!(grapheme_join::join_class(c), 0, "U+{:04X}", value);
            for left in left_partners {
                assert!(has_boundary(left, c), "U+{:04X} after U+{:04X}", value, left as u32);
            }
            for right in right_partners {
                assert!(has_boundary(c, right), "U+{:04X} before U+{:04X}", value, right as u32);
            }
        }
        if let Some(below) = char::from_u32(lo - 1) {
            assert!(!grapheme_join::is_plain_wide(below));
        }
        if let Some(above) = char::from_u32(hi + 1) {
            assert!(!grapheme_join::is_plain_wide(above));
        }
    }
}
