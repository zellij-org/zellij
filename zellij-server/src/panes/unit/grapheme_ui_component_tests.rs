use super::super::Grid;
use crate::panes::grid::SixelImageStore;
use crate::panes::kitty_graphics::KittyImageStore;
use crate::panes::link_handler::LinkHandler;
use crate::panes::terminal_character::{AnsiCode, TerminalCharacter};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use vte;
use zellij_utils::data::{Palette, Style};
use zellij_utils::graphemes;

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
const DECOMPOSED_E: &str = "e\u{301}";
const FLAG: &str = "\u{1F1E9}\u{1F1EA}";

fn grapheme_grid() -> Grid {
    let mut grid = Grid::new(
        41,
        110,
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
    );
    grid.grapheme_mode = true;
    grid
}

fn encoded(text: &str) -> String {
    text.bytes()
        .map(|byte| byte.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

fn render(component_name: &str, serialized_params: &str) -> Grid {
    let mut vte_parser = vte::Parser::new();
    let mut grid = grapheme_grid();
    let dcs = format!("\u{1b}Pz{};{}\u{1b}\\", component_name, serialized_params);
    vte_parser.advance(&mut grid, dcs.as_bytes());
    grid
}

fn row_cells(grid: &Grid, row: usize) -> Vec<(String, usize)> {
    grid.viewport
        .get(row)
        .map(|row| {
            row.columns
                .iter()
                .map(|cell| {
                    let mut text = String::new();
                    cell.push_text_to(&mut text);
                    (text, cell.width())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn row_text(cells: &[(String, usize)]) -> String {
    cells.iter().map(|(text, _)| text.as_str()).collect()
}

fn row_width(cells: &[(String, usize)]) -> usize {
    cells.iter().map(|(_, width)| width).sum()
}

fn assert_whole_clusters(cells: &[(String, usize)]) {
    let joined = row_text(cells);
    let expected: Vec<(String, usize)> = graphemes::clusters(&joined)
        .map(|(cluster, width)| (cluster.to_owned(), width))
        .collect();
    assert_eq!(cells, expected.as_slice(), "row: {:?}", joined);
}

fn count_of(cells: &[(String, usize)], cluster: &str) -> usize {
    cells.iter().filter(|(text, _)| text == cluster).count()
}

fn cell_of<'a>(grid: &'a Grid, row: usize, cluster: &str) -> &'a TerminalCharacter {
    grid.viewport[row]
        .columns
        .iter()
        .find(|cell| {
            let mut text = String::new();
            cell.push_text_to(&mut text);
            text == cluster
        })
        .unwrap_or_else(|| panic!("no cell holds {:?}", cluster))
}

fn column_of(cells: &[(String, usize)], cluster: &str) -> Option<usize> {
    let mut column = 0;
    for (text, width) in cells {
        if text == cluster {
            return Some(column);
        }
        column += width;
    }
    None
}

#[test]
fn text_component_puts_each_cluster_in_one_cell() {
    let content = format!("a{}b {} {} 你好", FAMILY, DECOMPOSED_E, FLAG);
    let grid = render("text", &encoded(&content));
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(row_text(&cells), content);
    assert!(cells.contains(&(FAMILY.to_owned(), 2)));
    assert!(cells.contains(&(DECOMPOSED_E.to_owned(), 1)));
    assert!(cells.contains(&(FLAG.to_owned(), 2)));
    assert!(cells.contains(&("你".to_owned(), 2)));
    assert_eq!(row_width(&cells), graphemes::display_width(&content));
}

#[test]
fn text_component_with_coordinates_stops_before_a_cluster_that_does_not_fit() {
    let grid = render(
        "text",
        &format!("0/0/4/;{}", encoded(&format!("a{}{}", FAMILY, FAMILY))),
    );
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(
        cells,
        vec![
            ("a".to_owned(), 1),
            (FAMILY.to_owned(), 2),
            (" ".to_owned(), 1)
        ]
    );
}

#[test]
fn text_component_highlights_a_cluster_by_its_first_character() {
    let style = Style::default();
    let emphasis = Some(AnsiCode::from(style.colors.text_unselected.emphasis_0));
    let base = Some(AnsiCode::from(style.colors.text_unselected.base));
    let content = format!("a{}b", FAMILY);
    let grid = render("text", &format!("1${}", encoded(&content)));
    assert_whole_clusters(&row_cells(&grid, 0));
    assert_eq!(cell_of(&grid, 0, FAMILY).styles.foreground, emphasis);
    assert_eq!(cell_of(&grid, 0, "a").styles.foreground, base);
    assert_eq!(cell_of(&grid, 0, "b").styles.foreground, base);
    let grid = render("text", &format!("3${}", encoded(&content)));
    assert_whole_clusters(&row_cells(&grid, 0));
    assert_eq!(cell_of(&grid, 0, FAMILY).styles.foreground, base);
}

#[test]
fn table_component_measures_columns_by_clusters() {
    let grid = render(
        "table",
        &format!(
            "2;2;{};{};{};{}",
            encoded("Name"),
            encoded("Flag"),
            encoded(&format!("{}x", FAMILY)),
            encoded(FLAG)
        ),
    );
    let title = row_cells(&grid, 0);
    let row = row_cells(&grid, 1);
    assert_whole_clusters(&title);
    assert_whole_clusters(&row);
    assert_eq!(count_of(&row, FAMILY), 1);
    assert_eq!(count_of(&row, FLAG), 1);
    assert_eq!(column_of(&title, "F"), Some(5));
    assert_eq!(column_of(&row, FLAG), Some(5));
    assert_eq!(row_width(&row), 9);
}

#[test]
fn ribbon_component_keeps_clusters_whole() {
    let content = format!("{}{}", DECOMPOSED_E, FLAG);
    let grid = render("ribbon", &encoded(&content));
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert!(cells.contains(&(DECOMPOSED_E.to_owned(), 1)));
    assert!(cells.contains(&(FLAG.to_owned(), 2)));
}

#[test]
fn ribbon_component_with_coordinates_stops_before_a_cluster_that_does_not_fit() {
    let grid = render(
        "ribbon",
        &format!("0/0/9/;{}", encoded(&format!("ab{}{}", FAMILY, FAMILY))),
    );
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(count_of(&cells, FAMILY), 1);
    assert_eq!(row_width(&cells), 9);
}

#[test]
fn nested_list_component_pads_lines_by_cluster_width() {
    let first = format!("{} family", FAMILY);
    let second = format!("{}t\u{e9} {}", DECOMPOSED_E, FLAG);
    let grid = render(
        "nested_list",
        &format!("{};|{}", encoded(&first), encoded(&second)),
    );
    let first_row = row_cells(&grid, 0);
    let second_row = row_cells(&grid, 1);
    assert_whole_clusters(&first_row);
    assert_whole_clusters(&second_row);
    assert!(first_row.contains(&(FAMILY.to_owned(), 2)));
    assert!(second_row.contains(&(DECOMPOSED_E.to_owned(), 1)));
    assert!(second_row.contains(&("\u{e9}".to_owned(), 1)));
    assert!(second_row.contains(&(FLAG.to_owned(), 2)));
    let width_up_to = |cells: &[(String, usize)], limit: usize| -> usize {
        let mut width = 0;
        for (_, cell_width) in cells {
            if width >= limit {
                break;
            }
            width += cell_width;
        }
        width
    };
    assert_eq!(width_up_to(&first_row, 12), 12);
    assert_eq!(width_up_to(&second_row, 12), 12);
}

#[test]
fn nested_list_component_with_coordinates_stops_before_a_cluster_that_does_not_fit() {
    let grid = render(
        "nested_list",
        &format!("0/0/8/;{}", encoded(&format!("ab{}{}", FAMILY, FAMILY))),
    );
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(count_of(&cells, FAMILY), 1);
    assert_eq!(row_width(&cells), 8);
}

#[test]
fn text_input_component_draws_the_cursor_over_the_whole_cluster() {
    let grid = render(
        "text_input",
        &format!("0/0/20/1;f,cur=2;;{}", encoded(&format!("a{}b", FAMILY))),
    );
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(count_of(&cells, FAMILY), 1);
    assert_eq!(row_width(&cells), 20);
    assert_eq!(cell_of(&grid, 0, FAMILY).styles.reverse, Some(AnsiCode::On));
    assert_ne!(cell_of(&grid, 0, "a").styles.reverse, Some(AnsiCode::On));
    assert_ne!(cell_of(&grid, 0, "b").styles.reverse, Some(AnsiCode::On));
}

#[test]
fn text_input_component_stops_before_a_cluster_that_does_not_fit() {
    let grid = render(
        "text_input",
        &format!("0/0/8/1;;;{}", encoded(&format!("a{}{}", FAMILY, FAMILY))),
    );
    let cells = row_cells(&grid, 0);
    assert_whole_clusters(&cells);
    assert_eq!(count_of(&cells, FAMILY), 1);
    assert_eq!(row_width(&cells), 8);
}

#[test]
fn menu_component_cuts_and_highlights_whole_clusters() {
    let style = Style::default();
    let first = format!("{}/0", encoded(&format!("{} fam", FAMILY)));
    let second = encoded(&format!("abcde{}{}", FAMILY, FAMILY));
    let grid = render("menu", &format!("0/0/12/2;;{};{}", first, second));
    let first_row = row_cells(&grid, 0);
    let second_row = row_cells(&grid, 1);
    assert_whole_clusters(&first_row);
    assert_whole_clusters(&second_row);
    let width_up_to = |cells: &[(String, usize)], limit: usize| -> usize {
        let mut width = 0;
        for (_, cell_width) in cells {
            if width >= limit {
                break;
            }
            width += cell_width;
        }
        width
    };
    assert_eq!(width_up_to(&first_row, 12), 12);
    assert_eq!(width_up_to(&second_row, 12), 12);
    assert_eq!(count_of(&second_row, FAMILY), 1);
    assert_eq!(count_of(&second_row, "…"), 1);
    assert_eq!(
        cell_of(&grid, 0, FAMILY).styles.foreground,
        Some(AnsiCode::from(style.colors.text_unselected.emphasis_0))
    );
    assert_eq!(
        cell_of(&grid, 0, "f").styles.foreground,
        Some(AnsiCode::from(style.colors.text_unselected.base))
    );
}
