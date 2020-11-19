use std::collections::HashMap;

use crate::geometry::{Coordinates, EdgeType, CornerType, Rectangle};

fn _debug_log_to_file(message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("/tmp/mosaic-log.txt")
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

pub mod boundary_type {
    pub const TOP_RIGHT: &str = "┐";
    pub const VERTICAL: &str = "│";
    pub const HORIZONTAL: &str = "─";
    pub const TOP_LEFT: &str = "┌";
    pub const BOTTOM_RIGHT: &str = "┘";
    pub const BOTTOM_LEFT: &str = "└";
    pub const VERTICAL_LEFT: &str = "┤";
    pub const VERTICAL_RIGHT: &str = "├";
    pub const HORIZONTAL_DOWN: &str = "┬";
    pub const HORIZONTAL_UP: &str = "┴";
    pub const CROSS: &str = "┼";
}

pub type BoundaryType = &'static str; // easy way to refer to boundary_type above

fn combine_symbols(current_symbol: &str, next_symbol: &str) -> Option<BoundaryType> {
    use boundary_type::*;

    match (current_symbol, next_symbol) {
        (CROSS, _) => Some(CROSS), // (┼, *) => Some(┼)
        (_, CROSS) => Some(CROSS), // (*, ┼) => Some(┼)

        (TOP_RIGHT, TOP_RIGHT) => Some(TOP_RIGHT), // (┐, ┐) => Some(┐)
        (TOP_RIGHT, VERTICAL) => Some(VERTICAL_LEFT), // (┐, │) => Some(┤)
        (TOP_RIGHT, HORIZONTAL) => Some(HORIZONTAL_DOWN), // (┐, ─) => Some(┬)
        (TOP_RIGHT, TOP_LEFT) => Some(HORIZONTAL_DOWN), // (┐, ┌) => Some(┬)
        (TOP_RIGHT, BOTTOM_RIGHT) => Some(VERTICAL_LEFT), // (┐, ┘) => Some(┤)
        (TOP_RIGHT, BOTTOM_LEFT) => Some(CROSS),   // (┐, └) => Some(┼)
        (TOP_RIGHT, VERTICAL_LEFT) => Some(VERTICAL_LEFT), // (┐, ┤) => Some(┤)
        (TOP_RIGHT, VERTICAL_RIGHT) => Some(CROSS), // (┐, ├) => Some(┼)
        (TOP_RIGHT, HORIZONTAL_DOWN) => Some(HORIZONTAL_DOWN), // (┐, ┬) => Some(┬)
        (TOP_RIGHT, HORIZONTAL_UP) => Some(CROSS), // (┐, ┴) => Some(┼)

        (HORIZONTAL, HORIZONTAL) => Some(HORIZONTAL), // (─, ─) => Some(─)
        (HORIZONTAL, VERTICAL) => Some(CROSS),        // (─, │) => Some(┼)
        (HORIZONTAL, TOP_LEFT) => Some(HORIZONTAL_DOWN), // (─, ┌) => Some(┬)
        (HORIZONTAL, BOTTOM_RIGHT) => Some(HORIZONTAL_UP), // (─, ┘) => Some(┴)
        (HORIZONTAL, BOTTOM_LEFT) => Some(HORIZONTAL_UP), // (─, └) => Some(┴)
        (HORIZONTAL, VERTICAL_LEFT) => Some(CROSS),   // (─, ┤) => Some(┼)
        (HORIZONTAL, VERTICAL_RIGHT) => Some(CROSS),  // (─, ├) => Some(┼)
        (HORIZONTAL, HORIZONTAL_DOWN) => Some(HORIZONTAL_DOWN), // (─, ┬) => Some(┬)
        (HORIZONTAL, HORIZONTAL_UP) => Some(HORIZONTAL_UP), // (─, ┴) => Some(┴)

        (VERTICAL, VERTICAL) => Some(VERTICAL), // (│, │) => Some(│)
        (VERTICAL, TOP_LEFT) => Some(VERTICAL_RIGHT), // (│, ┌) => Some(├)
        (VERTICAL, BOTTOM_RIGHT) => Some(VERTICAL_LEFT), // (│, ┘) => Some(┤)
        (VERTICAL, BOTTOM_LEFT) => Some(VERTICAL_RIGHT), // (│, └) => Some(├)
        (VERTICAL, VERTICAL_LEFT) => Some(VERTICAL_LEFT), // (│, ┤) => Some(┤)
        (VERTICAL, VERTICAL_RIGHT) => Some(VERTICAL_RIGHT), // (│, ├) => Some(├)
        (VERTICAL, HORIZONTAL_DOWN) => Some(CROSS), // (│, ┬) => Some(┼)
        (VERTICAL, HORIZONTAL_UP) => Some(CROSS), // (│, ┴) => Some(┼)

        (TOP_LEFT, TOP_LEFT) => Some(TOP_LEFT), // (┌, ┌) => Some(┌)
        (TOP_LEFT, BOTTOM_RIGHT) => Some(CROSS), // (┌, ┘) => Some(┼)
        (TOP_LEFT, BOTTOM_LEFT) => Some(VERTICAL_RIGHT), // (┌, └) => Some(├)
        (TOP_LEFT, VERTICAL_LEFT) => Some(CROSS), // (┌, ┤) => Some(┼)
        (TOP_LEFT, VERTICAL_RIGHT) => Some(VERTICAL_RIGHT), // (┌, ├) => Some(├)
        (TOP_LEFT, HORIZONTAL_DOWN) => Some(HORIZONTAL_DOWN), // (┌, ┬) => Some(┬)
        (TOP_LEFT, HORIZONTAL_UP) => Some(CROSS), // (┌, ┴) => Some(┼)

        (BOTTOM_RIGHT, BOTTOM_RIGHT) => Some(BOTTOM_RIGHT), // (┘, ┘) => Some(┘)
        (BOTTOM_RIGHT, BOTTOM_LEFT) => Some(HORIZONTAL_UP), // (┘, └) => Some(┴)
        (BOTTOM_RIGHT, VERTICAL_LEFT) => Some(VERTICAL_LEFT), // (┘, ┤) => Some(┤)
        (BOTTOM_RIGHT, VERTICAL_RIGHT) => Some(CROSS),      // (┘, ├) => Some(┼)
        (BOTTOM_RIGHT, HORIZONTAL_DOWN) => Some(CROSS),     // (┘, ┬) => Some(┼)
        (BOTTOM_RIGHT, HORIZONTAL_UP) => Some(HORIZONTAL_UP), // (┘, ┴) => Some(┴)

        (BOTTOM_LEFT, BOTTOM_LEFT) => Some(BOTTOM_LEFT), // (└, └) => Some(└)
        (BOTTOM_LEFT, VERTICAL_LEFT) => Some(CROSS),     // (└, ┤) => Some(┼)
        (BOTTOM_LEFT, VERTICAL_RIGHT) => Some(VERTICAL_RIGHT), // (└, ├) => Some(├)
        (BOTTOM_LEFT, HORIZONTAL_DOWN) => Some(CROSS),   // (└, ┬) => Some(┼)
        (BOTTOM_LEFT, HORIZONTAL_UP) => Some(HORIZONTAL_UP), // (└, ┴) => Some(┴)

        (VERTICAL_LEFT, VERTICAL_LEFT) => Some(VERTICAL_LEFT), // (┤, ┤) => Some(┤)
        (VERTICAL_LEFT, VERTICAL_RIGHT) => Some(CROSS),        // (┤, ├) => Some(┼)
        (VERTICAL_LEFT, HORIZONTAL_DOWN) => Some(CROSS),       // (┤, ┬) => Some(┼)
        (VERTICAL_LEFT, HORIZONTAL_UP) => Some(HORIZONTAL_UP), // (┤, ┴) => Some(┼)

        (VERTICAL_RIGHT, VERTICAL_RIGHT) => Some(VERTICAL_RIGHT), // (├, ├) => Some(├)
        (VERTICAL_RIGHT, HORIZONTAL_DOWN) => Some(CROSS),         // (├, ┬) => Some(┼)
        (VERTICAL_RIGHT, HORIZONTAL_UP) => Some(CROSS),           // (├, ┴) => Some(┼)

        (HORIZONTAL_DOWN, HORIZONTAL_DOWN) => Some(HORIZONTAL_DOWN), // (┬, ┬) => Some(┬)
        (HORIZONTAL_DOWN, HORIZONTAL_UP) => Some(CROSS),             // (┬, ┴) => Some(┼)

        (HORIZONTAL_UP, HORIZONTAL_UP) => Some(HORIZONTAL_UP), // (┴, ┴) => Some(┴)

        (a, b) => combine_symbols(b, a), // If we didn't find a match, swap them
    }
}

pub struct ScreenCanvas {
    x: usize,
    y: usize,
    columns: usize,
    rows: usize,
    boundary_characters: HashMap<Coordinates, BoundaryType>,
    borders: HashMap<EdgeType, usize>,
}

impl ScreenCanvas {
    pub fn new(x: u16, y: u16, columns: u16, rows: u16) -> Self {
        let x = x as usize;
        let y = y as usize;
        let columns = columns as usize;
        let rows = rows as usize;
        ScreenCanvas {
            x,
            y,
            columns,
            rows,
            boundary_characters: HashMap::new(),
            borders: [
                (EdgeType::Left, 0),
                (EdgeType::Right, 0),
                (EdgeType::Top, 0),
                (EdgeType::Bottom, 0),
            ]
            .iter()
            .cloned()
            .collect(),
        }
    }
    pub fn add_rect<R: Rectangle>(&mut self, rect: &R) {
        if self.rect_right_boundary_is_before_screen_edge(rect) {
            let boundary_x_coords = rect.edge(&EdgeType::Right);
            let first_row_coordinates = self.rect_right_boundary_row_start(rect);
            let last_row_coordinates = self.rect_right_boundary_row_end(rect);
            for row in first_row_coordinates..last_row_coordinates {
                let coordinates = Coordinates::new(boundary_x_coords, row);
                let symbol_to_add = if row == first_row_coordinates && row != 0 {
                    boundary_type::TOP_RIGHT
                } else if row == last_row_coordinates - 1 && row != self.rows - 1 {
                    boundary_type::BOTTOM_RIGHT
                } else {
                    boundary_type::VERTICAL
                };
                let next_symbol = self
                    .boundary_characters
                    .get(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if self.rect_bottom_boundary_is_before_screen_edge(rect) {
            let boundary_y_coords = rect.edge(&EdgeType::Bottom);
            let first_col_coordinates = self.rect_bottom_boundary_col_start(rect);
            let last_col_coordinates = self.rect_bottom_boundary_col_end(rect);
            for col in first_col_coordinates..last_col_coordinates {
                let coordinates = Coordinates::new(col, boundary_y_coords);
                let symbol_to_add = if col == first_col_coordinates && col != 0 {
                    boundary_type::BOTTOM_LEFT
                } else if col == last_col_coordinates - 1 && col != self.columns - 1 {
                    boundary_type::BOTTOM_RIGHT
                } else {
                    boundary_type::HORIZONTAL
                };
                let next_symbol = self
                    .boundary_characters
                    .get(&coordinates)
                    .and_then(|current_symbol| combine_symbols(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
    }
    pub fn vte_output(&self) -> String {
        let mut vte_output = String::new();
        for (coordinates, boundary_character) in &self.boundary_characters {
            vte_output.push_str(&format!(
                "\u{1b}[{};{}H\u{1b}[m{}",
                coordinates.y + 1,
                coordinates.x + 1,
                boundary_character
            )); // goto row/col + boundary character
        }
        vte_output
    }
    fn rect_right_boundary_is_before_screen_edge<R: Rectangle>(&self, rect: &R) -> bool {
        rect.x() + rect.columns() < self.columns
    }
    fn rect_bottom_boundary_is_before_screen_edge<R: Rectangle>(&self, rect: &R) -> bool {
        rect.y() + rect.rows() < self.rows
    }
    fn rect_right_boundary_row_start<R: Rectangle>(&self, rect: &R) -> usize {
        if rect.y() == 0 {
            0
        } else {
            rect.y() - 1
        }
    }
    fn rect_right_boundary_row_end<R: Rectangle>(&self, rect: &R) -> usize {
        let rect_bottom_row = rect.y() + rect.rows();
        // we do this because unless we're on the screen edge, we'd like to go one extra row to
        // connect to whatever boundary is beneath us
        if rect_bottom_row == self.rows {
            rect_bottom_row
        } else {
            rect_bottom_row + 1
        }
    }
    fn rect_bottom_boundary_col_start<R: Rectangle>(&self, rect: &R) -> usize {
        if rect.x() == 0 {
            0
        } else {
            rect.x() - 1
        }
    }
    fn rect_bottom_boundary_col_end<R: Rectangle>(&self, rect: &R) -> usize {
        let rect_right_col = rect.x() + rect.columns();
        // we do this because unless we're on the screen edge, we'd like to go one extra column to
        // connect to whatever boundary is right of us
        if rect_right_col == self.columns {
            rect_right_col
        } else {
            rect_right_col + 1
        }
    }
}

impl Rectangle for ScreenCanvas {
    fn x(&self) -> usize {
        self.x
    }
    fn y(&self) -> usize {
        self.y
    }

    fn columns(&self) -> usize {
        self.columns
    }

    fn rows(&self) -> usize {
        self.rows
    }

    fn borders(&self) -> &HashMap<EdgeType, usize> {
        &self.borders
    }
}