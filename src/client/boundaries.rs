use crate::tab::Pane;
use std::collections::HashMap;
use ansi_term::Colour::*;
use super::super::utils::logging::debug_log_to_file;

use std::fmt::{Display, Error, Formatter};

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

#[derive(Clone, Copy, Debug)]
pub struct BoundarySymbol {
    boundary_type: BoundaryType,
    invisible: bool,
    should_be_colored: bool,
    color: ansi_term::Colour
}

impl BoundarySymbol {
    pub fn new(boundary_type: BoundaryType, should_be_colored: bool) -> Self {
        BoundarySymbol {
            boundary_type,
            invisible: false,
            should_be_colored,
            color: match should_be_colored {
                true => ansi_term::Colour::Red,
                false => ansi_term::Colour::White
            }
        }
    }
    pub fn invisible(mut self) -> Self {
        self.invisible = true;
        self
    }
    pub fn color(mut self, color: ansi_term::Colour) -> Self {
        self.color = color;
        self.should_be_colored = true;
        self
    }
}

impl Display for BoundarySymbol {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        match self.invisible {
            true => write!(f, " "),
            false => {
                match self.should_be_colored {
                    true => write!(f, "{}", self.color.paint(self.boundary_type)),
                    false => write!(f, "{}", self.boundary_type)
                }
            }
        }
    }
}

fn combine_symbols(
    current_symbol: BoundarySymbol,
    next_symbol: BoundarySymbol,
) -> Option<BoundarySymbol> {
    let invisible = current_symbol.invisible || next_symbol.invisible;
    let should_be_colored = current_symbol.should_be_colored || next_symbol.should_be_colored;
    let color = next_symbol.color;
    let current_symbol = current_symbol.boundary_type;
    let next_symbol = next_symbol.boundary_type;
    match (current_symbol, next_symbol) {
        (boundary_type::TOP_RIGHT, boundary_type::TOP_RIGHT) => {
            // (┐, ┐) => Some(┐)
            let boundary_type = boundary_type::TOP_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::VERTICAL) => {
            // (┐, │) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::HORIZONTAL) => {
            // (┐, ─) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::TOP_LEFT) => {
            // (┐, ┌) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::BOTTOM_RIGHT) => {
            // (┐, ┘) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::BOTTOM_LEFT) => {
            // (┐, └) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::VERTICAL_LEFT) => {
            // (┐, ┤) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::VERTICAL_RIGHT) => {
            // (┐, ├) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::HORIZONTAL_DOWN) => {
            // (┐, ┬) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::HORIZONTAL_UP) => {
            // (┐, ┴) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_RIGHT, boundary_type::CROSS) => {
            // (┐, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::HORIZONTAL) => {
            // (─, ─) => Some(─)
            let boundary_type = boundary_type::HORIZONTAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::VERTICAL) => {
            // (─, │) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::TOP_LEFT) => {
            // (─, ┌) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::BOTTOM_RIGHT) => {
            // (─, ┘) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::BOTTOM_LEFT) => {
            // (─, └) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::VERTICAL_LEFT) => {
            // (─, ┤) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::VERTICAL_RIGHT) => {
            // (─, ├) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::HORIZONTAL_DOWN) => {
            // (─, ┬) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::HORIZONTAL_UP) => {
            // (─, ┴) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL, boundary_type::CROSS) => {
            // (─, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::VERTICAL) => {
            // (│, │) => Some(│)
            let boundary_type = boundary_type::VERTICAL;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::TOP_LEFT) => {
            // (│, ┌) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::BOTTOM_RIGHT) => {
            // (│, ┘) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::BOTTOM_LEFT) => {
            // (│, └) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::VERTICAL_LEFT) => {
            // (│, ┤) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::VERTICAL_RIGHT) => {
            // (│, ├) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::HORIZONTAL_DOWN) => {
            // (│, ┬) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::HORIZONTAL_UP) => {
            // (│, ┴) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL, boundary_type::CROSS) => {
            // (│, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::TOP_LEFT) => {
            // (┌, ┌) => Some(┌)
            let boundary_type = boundary_type::TOP_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::BOTTOM_RIGHT) => {
            // (┌, ┘) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::BOTTOM_LEFT) => {
            // (┌, └) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::VERTICAL_LEFT) => {
            // (┌, ┤) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::VERTICAL_RIGHT) => {
            // (┌, ├) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::HORIZONTAL_DOWN) => {
            // (┌, ┬) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::HORIZONTAL_UP) => {
            // (┌, ┴) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::TOP_LEFT, boundary_type::CROSS) => {
            // (┌, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::BOTTOM_RIGHT) => {
            // (┘, ┘) => Some(┘)
            let boundary_type = boundary_type::BOTTOM_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::BOTTOM_LEFT) => {
            // (┘, └) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::VERTICAL_LEFT) => {
            // (┘, ┤) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::VERTICAL_RIGHT) => {
            // (┘, ├) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::HORIZONTAL_DOWN) => {
            // (┘, ┬) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::HORIZONTAL_UP) => {
            // (┘, ┴) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_RIGHT, boundary_type::CROSS) => {
            // (┘, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::BOTTOM_LEFT) => {
            // (└, └) => Some(└)
            let boundary_type = boundary_type::BOTTOM_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::VERTICAL_LEFT) => {
            // (└, ┤) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::VERTICAL_RIGHT) => {
            // (└, ├) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::HORIZONTAL_DOWN) => {
            // (└, ┬) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::HORIZONTAL_UP) => {
            // (└, ┴) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::BOTTOM_LEFT, boundary_type::CROSS) => {
            // (└, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_LEFT, boundary_type::VERTICAL_LEFT) => {
            // (┤, ┤) => Some(┤)
            let boundary_type = boundary_type::VERTICAL_LEFT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_LEFT, boundary_type::VERTICAL_RIGHT) => {
            // (┤, ├) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_LEFT, boundary_type::HORIZONTAL_DOWN) => {
            // (┤, ┬) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_LEFT, boundary_type::HORIZONTAL_UP) => {
            // (┤, ┴) => Some(┼)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_LEFT, boundary_type::CROSS) => {
            // (┤, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_RIGHT, boundary_type::VERTICAL_RIGHT) => {
            // (├, ├) => Some(├)
            let boundary_type = boundary_type::VERTICAL_RIGHT;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_RIGHT, boundary_type::HORIZONTAL_DOWN) => {
            // (├, ┬) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_RIGHT, boundary_type::HORIZONTAL_UP) => {
            // (├, ┴) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::VERTICAL_RIGHT, boundary_type::CROSS) => {
            // (├, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL_DOWN, boundary_type::HORIZONTAL_DOWN) => {
            // (┬, ┬) => Some(┬)
            let boundary_type = boundary_type::HORIZONTAL_DOWN;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL_DOWN, boundary_type::HORIZONTAL_UP) => {
            // (┬, ┴) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL_DOWN, boundary_type::CROSS) => {
            // (┬, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL_UP, boundary_type::HORIZONTAL_UP) => {
            // (┴, ┴) => Some(┴)
            let boundary_type = boundary_type::HORIZONTAL_UP;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::HORIZONTAL_UP, boundary_type::CROSS) => {
            // (┴, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (boundary_type::CROSS, boundary_type::CROSS) => {
            // (┼, ┼) => Some(┼)
            let boundary_type = boundary_type::CROSS;
            Some(BoundarySymbol {
                boundary_type,
                invisible,
                should_be_colored,
                color
            })
        }
        (_, _) => None,
    }
}

fn find_next_symbol(
    first_symbol: BoundarySymbol,
    second_symbol: BoundarySymbol,
) -> Option<BoundarySymbol> {
    if let Some(symbol) = combine_symbols(first_symbol, second_symbol) {
        Some(symbol)
    } else {
        combine_symbols(second_symbol, first_symbol)
    }
}

#[derive(PartialEq, Eq, Hash, Debug)]
pub struct Coordinates {
    x: usize,
    y: usize,
}

impl Coordinates {
    pub fn new(x: usize, y: usize) -> Self {
        Coordinates { x, y }
    }
}

pub trait Rect {
    fn x(&self) -> usize;
    fn y(&self) -> usize;
    fn rows(&self) -> usize;
    fn columns(&self) -> usize;
    fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.columns()
    }
    fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    fn is_directly_right_of(&self, other: &Self) -> bool {
        self.x() == other.x() + other.columns() + 1
    }
    fn is_directly_left_of(&self, other: &Self) -> bool {
        self.x() + self.columns() + 1 == other.x()
    }
    fn is_directly_below(&self, other: &Self) -> bool {
        self.y() == other.y() + other.rows() + 1
    }
    fn is_directly_above(&self, other: &Self) -> bool {
        self.y() + self.rows() + 1 == other.y()
    }
    fn horizontally_overlaps_with(&self, other: &Self) -> bool {
        (self.y() >= other.y() && self.y() <= (other.y() + other.rows()))
            || ((self.y() + self.rows()) <= (other.y() + other.rows())
                && (self.y() + self.rows()) > other.y())
            || (self.y() <= other.y() && (self.y() + self.rows() >= (other.y() + other.rows())))
            || (other.y() <= self.y() && (other.y() + other.rows() >= (self.y() + self.rows())))
    }
    fn get_horizontal_overlap_with(&self, other: &Self) -> usize {
        std::cmp::min(self.y() + self.rows(), other.y() + other.rows())
            - std::cmp::max(self.y(), other.y())
    }
    fn vertically_overlaps_with(&self, other: &Self) -> bool {
        (self.x() >= other.x() && self.x() <= (other.x() + other.columns()))
            || ((self.x() + self.columns()) <= (other.x() + other.columns())
                && (self.x() + self.columns()) > other.x())
            || (self.x() <= other.x()
                && (self.x() + self.columns() >= (other.x() + other.columns())))
            || (other.x() <= self.x()
                && (other.x() + other.columns() >= (self.x() + self.columns())))
    }
    fn get_vertical_overlap_with(&self, other: &Self) -> usize {
        std::cmp::min(self.x() + self.columns(), other.x() + other.columns())
            - std::cmp::max(self.x(), other.x())
    }
}

pub struct Boundaries {
    columns: usize,
    rows: usize,
    // boundary_characters: HashMap<Coordinates, BoundaryType>,
    boundary_characters: HashMap<Coordinates, BoundarySymbol>,
}

impl Boundaries {
    pub fn new(columns: u16, rows: u16) -> Self {
        let columns = columns as usize;
        let rows = rows as usize;
        Boundaries {
            columns,
            rows,
            boundary_characters: HashMap::new(),
        }
    }
    pub fn add_rect(&mut self, rect: &dyn Pane, should_be_colored: bool) {
        debug_log_to_file(format!("colored? {}", should_be_colored));
        if rect.x() > 0 {
            let boundary_x_coords = rect.x() - 1;
            let first_row_coordinates = self.rect_right_boundary_row_start(rect);
            let last_row_coordinates = self.rect_right_boundary_row_end(rect);
            for row in first_row_coordinates..last_row_coordinates {
                let coordinates = Coordinates::new(boundary_x_coords, row);
                let mut symbol_to_add = if row == first_row_coordinates && row != 0 {
                    BoundarySymbol::new(boundary_type::TOP_LEFT, should_be_colored)
                } else if row == last_row_coordinates - 1 && row != self.rows - 1 {
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT, should_be_colored)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL, should_be_colored)
                };
                if rect.invisible_borders() {
                    symbol_to_add = symbol_to_add.invisible();
                }
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| find_next_symbol(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if rect.y() > 0 {
            let boundary_y_coords = rect.y() - 1;
            let first_col_coordinates = self.rect_bottom_boundary_col_start(rect);
            let last_col_coordinates = self.rect_bottom_boundary_col_end(rect);
            for col in first_col_coordinates..last_col_coordinates {
                let coordinates = Coordinates::new(col, boundary_y_coords);
                let mut symbol_to_add = if col == first_col_coordinates && col != 0 {
                    BoundarySymbol::new(boundary_type::TOP_LEFT, should_be_colored)
                } else if col == last_col_coordinates - 1 && col != self.columns - 1 {
                    BoundarySymbol::new(boundary_type::TOP_RIGHT, should_be_colored)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL, should_be_colored)
                };
                if rect.invisible_borders() {
                    symbol_to_add = symbol_to_add.invisible();
                }
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| find_next_symbol(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if self.rect_right_boundary_is_before_screen_edge(rect) {
            // let boundary_x_coords = self.rect_right_boundary_x_coords(rect);
            let boundary_x_coords = rect.right_boundary_x_coords();
            let first_row_coordinates = self.rect_right_boundary_row_start(rect);
            let last_row_coordinates = self.rect_right_boundary_row_end(rect);
            for row in first_row_coordinates..last_row_coordinates {
                let coordinates = Coordinates::new(boundary_x_coords, row);
                let mut symbol_to_add = if row == first_row_coordinates && row != 0 {
                    BoundarySymbol::new(boundary_type::TOP_RIGHT, should_be_colored)
                } else if row == last_row_coordinates - 1 && row != self.rows - 1 {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT, should_be_colored)
                } else {
                    BoundarySymbol::new(boundary_type::VERTICAL, should_be_colored)
                };
                if rect.invisible_borders() {
                    symbol_to_add = symbol_to_add.invisible();
                }
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| find_next_symbol(current_symbol, symbol_to_add))
                    .unwrap_or(symbol_to_add);
                self.boundary_characters.insert(coordinates, next_symbol);
            }
        }
        if self.rect_bottom_boundary_is_before_screen_edge(rect) {
            let boundary_y_coords = rect.bottom_boundary_y_coords();
            let first_col_coordinates = self.rect_bottom_boundary_col_start(rect);
            let last_col_coordinates = self.rect_bottom_boundary_col_end(rect);
            for col in first_col_coordinates..last_col_coordinates {
                let coordinates = Coordinates::new(col, boundary_y_coords);
                let mut symbol_to_add = if col == first_col_coordinates && col != 0 {
                    BoundarySymbol::new(boundary_type::BOTTOM_LEFT, should_be_colored)
                } else if col == last_col_coordinates - 1 && col != self.columns - 1 {
                    BoundarySymbol::new(boundary_type::BOTTOM_RIGHT, should_be_colored)
                } else {
                    BoundarySymbol::new(boundary_type::HORIZONTAL, should_be_colored)
                };
                if rect.invisible_borders() {
                    symbol_to_add = symbol_to_add.invisible();
                }
                let next_symbol = self
                    .boundary_characters
                    .remove(&coordinates)
                    .and_then(|current_symbol| find_next_symbol(current_symbol, symbol_to_add))
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
    fn rect_right_boundary_is_before_screen_edge(&self, rect: &dyn Pane) -> bool {
        rect.x() + rect.columns() < self.columns
    }
    fn rect_bottom_boundary_is_before_screen_edge(&self, rect: &dyn Pane) -> bool {
        rect.y() + rect.rows() < self.rows
    }
    fn rect_right_boundary_row_start(&self, rect: &dyn Pane) -> usize {
        if rect.y() == 0 {
            0
        } else {
            rect.y() - 1
        }
    }
    fn rect_right_boundary_row_end(&self, rect: &dyn Pane) -> usize {
        let rect_bottom_row = rect.y() + rect.rows();
        // we do this because unless we're on the screen edge, we'd like to go one extra row to
        // connect to whatever boundary is beneath us
        if rect_bottom_row == self.rows {
            rect_bottom_row
        } else {
            rect_bottom_row + 1
        }
    }
    fn rect_bottom_boundary_col_start(&self, rect: &dyn Pane) -> usize {
        if rect.x() == 0 {
            0
        } else {
            rect.x() - 1
        }
    }
    fn rect_bottom_boundary_col_end(&self, rect: &dyn Pane) -> usize {
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
