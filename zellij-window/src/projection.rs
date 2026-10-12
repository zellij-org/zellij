use std::collections::BTreeSet;
use std::fmt::Write as _;

use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::{Cell as AlacrittyCell, Flags};
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};

use crate::vte_terminal::{Blink, VteTerminal};

pub const DEFAULT_COLOR: &str = "default";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Occupancy {
    Single,
    WideHead,
    WideTail,
    LeadingSpacer,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cell {
    pub text: String,
    pub occupancy: Occupancy,
    pub fg: String,
    pub bg: String,
    pub underline: Option<String>,
    pub attrs: BTreeSet<&'static str>,
    pub link: Option<String>,
}

impl Cell {
    pub fn describe(&self) -> String {
        let mut described = format!("{:?} {:?}", self.text, self.occupancy);
        let _ = write!(described, " fg={} bg={}", self.fg, self.bg);
        if let Some(underline) = &self.underline {
            let _ = write!(described, " underline={}", underline);
        }
        if !self.attrs.is_empty() {
            let _ = write!(
                described,
                " attrs={}",
                self.attrs.iter().copied().collect::<Vec<_>>().join("|")
            );
        }
        if let Some(link) = &self.link {
            let _ = write!(described, " link={}", link);
        }
        described
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Cursor {
    pub position: Option<(usize, usize)>,
    pub visible: Option<bool>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Projection {
    pub rows: usize,
    pub cols: usize,
    pub cursor: Cursor,
    pub cells: Vec<Vec<Cell>>,
    pub wrapped_rows: Vec<usize>,
}

#[derive(Debug)]
pub struct Divergence {
    pub what: String,
    pub reference: String,
    pub actual: String,
}

impl Divergence {
    pub fn describe(&self) -> String {
        format!(
            "{}\n    reference: {}\n    window: {}",
            self.what, self.reference, self.actual
        )
    }
}

pub fn compare(reference: &Projection, actual: &Projection) -> Vec<Divergence> {
    let mut divergences = Vec::new();

    if reference.rows != actual.rows || reference.cols != actual.cols {
        divergences.push(Divergence {
            what: "size".to_owned(),
            reference: format!("{}x{}", reference.rows, reference.cols),
            actual: format!("{}x{}", actual.rows, actual.cols),
        });
        return divergences;
    }

    if let (Some(expected), Some(found)) = (reference.cursor.position, actual.cursor.position) {
        if expected != found {
            divergences.push(Divergence {
                what: "cursor position".to_owned(),
                reference: format!("{},{}", expected.0, expected.1),
                actual: format!("{},{}", found.0, found.1),
            });
        }
    }

    if let (Some(expected), Some(found)) = (reference.cursor.visible, actual.cursor.visible) {
        if expected != found {
            divergences.push(Divergence {
                what: "cursor visibility".to_owned(),
                reference: format!("{}", expected),
                actual: format!("{}", found),
            });
        }
    }

    for row in 0..reference.rows {
        for col in 0..reference.cols {
            let expected = &reference.cells[row][col];
            let found = &actual.cells[row][col];
            if expected != found {
                divergences.push(Divergence {
                    what: format!("cell {},{}", row, col),
                    reference: expected.describe(),
                    actual: found.describe(),
                });
            }
        }
    }

    divergences
}

pub fn without_blink_or_cursor(mut projection: Projection) -> Projection {
    projection.cursor = Cursor {
        position: None,
        visible: None,
    };
    for row in &mut projection.cells {
        for cell in row {
            cell.attrs.remove("blink-slow");
            cell.attrs.remove("blink-fast");
        }
    }
    projection
}

pub fn of_window_scrolled(state: &VteTerminal, rows_up: usize) -> Projection {
    let size = state.size();
    let grid = state.term().grid();
    let top = -(rows_up as i32);

    let mut cells = Vec::with_capacity(size.rows);
    for row in 0..size.rows {
        let line = Line(top + row as i32);
        let mut projected = Vec::with_capacity(size.cols);
        for col in 0..size.cols {
            projected.push(of_alacritty_cell(
                &grid[line][Column(col)],
                Blink::default(),
            ));
        }
        cells.push(projected);
    }

    without_blink_or_cursor(Projection {
        rows: size.rows,
        cols: size.cols,
        cursor: Cursor {
            position: None,
            visible: None,
        },
        cells,
        wrapped_rows: Vec::new(),
    })
}

pub fn of_window(state: &VteTerminal) -> Projection {
    let size = state.size();
    let grid = state.term().grid();

    let mut cells = Vec::with_capacity(size.rows);
    let mut wrapped_rows = Vec::new();
    for row in 0..size.rows {
        let mut projected = Vec::with_capacity(size.cols);
        for col in 0..size.cols {
            let cell = &grid[Line(row as i32)][Column(col)];
            if cell.flags.contains(Flags::WRAPLINE) && !wrapped_rows.contains(&row) {
                wrapped_rows.push(row);
            }
            projected.push(of_alacritty_cell(cell, state.blink_at(row, col)));
        }
        cells.push(projected);
    }

    let cursor = grid.cursor.point;
    Projection {
        rows: size.rows,
        cols: size.cols,
        cursor: Cursor {
            position: Some((cursor.column.0, cursor.line.0.max(0) as usize)),
            visible: Some(state.cursor_is_visible()),
        },
        cells,
        wrapped_rows,
    }
}

fn of_alacritty_cell(cell: &AlacrittyCell, blink: Blink) -> Cell {
    let mut text = String::new();
    text.push(cell.c);
    for zerowidth in cell.zerowidth().into_iter().flatten() {
        text.push(*zerowidth);
    }

    let occupancy = if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
        Occupancy::LeadingSpacer
    } else if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
        Occupancy::WideTail
    } else if cell.flags.contains(Flags::WIDE_CHAR) {
        Occupancy::WideHead
    } else {
        Occupancy::Single
    };

    let mut attrs = BTreeSet::new();
    for (flag, name) in [
        (Flags::BOLD, "bold"),
        (Flags::DIM, "dim"),
        (Flags::ITALIC, "italic"),
        (Flags::INVERSE, "inverse"),
        (Flags::HIDDEN, "hidden"),
        (Flags::STRIKEOUT, "strike"),
        (Flags::UNDERLINE, "underline"),
        (Flags::DOUBLE_UNDERLINE, "double-underline"),
        (Flags::UNDERCURL, "undercurl"),
        (Flags::DOTTED_UNDERLINE, "dotted-underline"),
        (Flags::DASHED_UNDERLINE, "dashed-underline"),
    ] {
        if cell.flags.contains(flag) {
            attrs.insert(name);
        }
    }
    if blink.slow {
        attrs.insert("blink-slow");
    }
    if blink.fast {
        attrs.insert("blink-fast");
    }

    Cell {
        text,
        occupancy,
        fg: alacritty_color(cell.fg),
        bg: alacritty_color(cell.bg),
        underline: cell.underline_color().map(alacritty_color),
        attrs,
        link: cell.hyperlink().map(|link| link.uri().to_owned()),
    }
}

fn alacritty_color(color: Color) -> String {
    match color {
        Color::Named(NamedColor::Foreground) | Color::Named(NamedColor::Background) => {
            DEFAULT_COLOR.to_owned()
        },
        Color::Named(named) => format!("{:?}", named).to_lowercase(),
        Color::Indexed(index) => format!("idx{}", index),
        Color::Spec(Rgb { r, g, b }) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected(rows: usize, cols: usize, stream: &str) -> Projection {
        let mut state = VteTerminal::new(rows, cols);
        state.apply(stream);
        of_window(&state)
    }

    #[test]
    fn a_plain_cell_projects_to_defaults() {
        let projection = projected(1, 4, "a");
        let cell = &projection.cells[0][0];
        assert_eq!(cell.text, "a");
        assert_eq!(cell.occupancy, Occupancy::Single);
        assert_eq!(cell.fg, DEFAULT_COLOR);
        assert_eq!(cell.bg, DEFAULT_COLOR);
        assert!(cell.attrs.is_empty());
        assert_eq!(cell.describe(), "\"a\" Single fg=default bg=default");
    }

    #[test]
    fn every_color_kind_has_its_own_canonical_form() {
        let projection = projected(
            1,
            8,
            "\u{1b}[31ma\u{1b}[38;5;208mb\u{1b}[38;2;1;2;3mc\u{1b}[39md",
        );
        assert_eq!(projection.cells[0][0].fg, "red");
        assert_eq!(projection.cells[0][1].fg, "idx208");
        assert_eq!(projection.cells[0][2].fg, "#010203");
        assert_eq!(projection.cells[0][3].fg, DEFAULT_COLOR);
    }

    #[test]
    fn a_wide_character_occupies_a_head_and_a_tail() {
        let projection = projected(1, 4, "\u{4f60}x");
        assert_eq!(projection.cells[0][0].occupancy, Occupancy::WideHead);
        assert_eq!(projection.cells[0][0].text, "\u{4f60}");
        assert_eq!(projection.cells[0][1].occupancy, Occupancy::WideTail);
        assert_eq!(projection.cells[0][2].text, "x");
    }

    #[test]
    fn attributes_project_by_canonical_name() {
        let projection = projected(1, 4, "\u{1b}[1;3;4:3;9ma");
        let attrs: Vec<&str> = projection.cells[0][0].attrs.iter().copied().collect();
        assert_eq!(attrs, vec!["bold", "italic", "strike", "undercurl"]);
    }

    #[test]
    fn identical_projections_report_no_divergence() {
        let one = projected(2, 6, "\u{1b}[1;31mhi");
        let other = projected(2, 6, "\u{1b}[1;31mhi");
        assert!(compare(&one, &other).is_empty());
    }

    #[test]
    fn a_differing_cell_is_reported_with_both_descriptions() {
        let one = projected(1, 4, "\u{1b}[31ma");
        let other = projected(1, 4, "\u{1b}[32ma");
        let divergences = compare(&one, &other);
        assert_eq!(divergences.len(), 1);
        assert_eq!(divergences[0].what, "cell 0,0");
        assert!(divergences[0].reference.contains("fg=red"));
        assert!(divergences[0].actual.contains("fg=green"));
    }

    #[test]
    fn a_size_difference_short_circuits_the_comparison() {
        let one = projected(1, 4, "a");
        let other = projected(2, 4, "b");
        let divergences = compare(&one, &other);
        assert_eq!(divergences.len(), 1);
        assert_eq!(divergences[0].what, "size");
    }

    #[test]
    fn a_differing_cursor_position_is_reported() {
        let one = projected(1, 4, "ab");
        let other = projected(1, 4, "a");
        let divergences = compare(&one, &other);
        assert_eq!(divergences[0].what, "cursor position");
    }

    #[test]
    fn the_cursor_is_not_compared_where_the_reference_cannot_report_it() {
        let mut visible = projected(1, 4, "ab\u{1b}[1;1H");
        let hidden = projected(1, 4, "\u{1b}[?25lab");
        let divergences = compare(&visible, &hidden);
        assert_eq!(divergences[0].what, "cursor position");
        assert_eq!(divergences[1].what, "cursor visibility");
        visible.cursor.visible = None;
        visible.cursor.position = None;
        assert!(compare(&visible, &hidden).is_empty());
    }
}
