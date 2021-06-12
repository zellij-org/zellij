use zellij_utils::position::Position;

// The selection is empty when start == end
// it includes the character at start, and everything before end.
#[derive(Debug, Clone)]
pub struct Selection {
    pub start: Position,
    pub end: Position,
    active: bool, // used to handle moving the selection up and down
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
            active: false,
        }
    }
}

impl Selection {
    pub fn start(&mut self, start: Position) {
        self.active = true;
        self.start = start;
        self.end = start;
    }

    pub fn to(&mut self, to: Position) {
        self.end = to
    }

    pub fn end(&mut self, to: Option<&Position>) {
        self.active = false;
        if let Some(to) = to {
            self.end = *to
        }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        let row = row as isize;
        let (start, end) = if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };

        if (start.line.0) < row && row < end.line.0 {
            return true;
        }
        if start.line == end.line {
            return row == start.line.0 && start.column.0 <= col && col < end.column.0;
        }
        if start.line.0 == row && col >= start.column.0 {
            return true;
        }
        end.line.0 == row && col < end.column.0
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn reset(&mut self) {
        self.start = Position::new(0, 0);
        self.end = self.start;
    }

    pub fn sorted(&self) -> Self {
        let (start, end) = if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };
        Self {
            start,
            end,
            active: self.active,
        }
    }

    pub fn line_indices(&self) -> std::ops::RangeInclusive<isize> {
        let sorted = self.sorted();
        sorted.start.line.0..=sorted.end.line.0
    }

    pub fn move_up(&mut self, lines: usize) {
        self.start.line.0 -= lines as isize;
        if !self.active {
            self.end.line.0 -= lines as isize;
        }
    }

    pub fn move_down(&mut self, lines: usize) {
        self.start.line.0 += lines as isize;
        if !self.active {
            self.end.line.0 += lines as isize;
        }
    }
}

#[cfg(test)]
#[path = "./unit/selection_tests.rs"]
mod selection_tests;
