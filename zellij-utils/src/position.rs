use serde::{Deserialize, Serialize};

#[derive(Debug, Hash, Copy, Clone, PartialEq, Eq, PartialOrd, Deserialize, Serialize)]
pub struct Position {
    pub line: Line,
    pub column: Column,
}

impl Position {
    pub fn new(line: i32, column: u16) -> Self {
        Self {
            line: Line(line as isize),
            column: Column(column as usize),
        }
    }
    pub fn change_line(&mut self, line: isize) {
        self.line = Line(line);
    }

    pub fn change_column(&mut self, column: usize) {
        self.column = Column(column);
    }

    pub fn relative_to(&self, line: usize, column: usize) -> Self {
        Self {
            line: Line(self.line.0 - line as isize),
            column: Column(self.column.0.saturating_sub(column)),
        }
    }

    pub fn line(&self) -> isize {
        self.line.0
    }

    pub fn column(&self) -> usize {
        self.column.0
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd)]
pub struct Line(pub isize);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize, PartialOrd)]
pub struct Column(pub usize);
