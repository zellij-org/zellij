use serde::{Deserialize, Serialize};

use crate::pane_size::PositionAndSize;

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Deserialize, Serialize)]
pub struct Position {
    pub line: Line,
    pub column: Column,
}

impl Position {
    pub fn new(line: u16, column: u16) -> Self {
        Self {
            line: Line(line as isize),
            column: Column(column as usize),
        }
    }

    pub fn relative_to(&self, position_and_size: &PositionAndSize) -> Self {
        Self {
            line: Line(self.line.0 - position_and_size.y as isize),
            column: Column(self.column.0 - position_and_size.x),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, PartialOrd)]
pub struct Line(pub isize);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize, PartialOrd)]
pub struct Column(pub usize);
