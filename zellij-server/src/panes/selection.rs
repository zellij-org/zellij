use zellij_utils::input::mouse::Point;

#[derive(Debug)]
pub struct Selection {
    range: Option<Range>,
}

impl Default for Selection {
    fn default() -> Self {
        Self { range: None }
    }
}

impl Selection {
    pub fn start(&mut self, start: Point) {
        self.range = Some(Range { start, end: start })
    }

    pub fn to(&mut self, to: Point) {
        if let Some(range) = &mut self.range {
            range.end = to;
        }
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        if let Some(range) = &self.range {
            range.contains(row, col)
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub struct Range {
    pub start: Point,
    pub end: Point,
}

impl Range {
    fn contains(&self, row: usize, col: usize) -> bool {
        if (self.start.line.0 as usize) < row && row < self.end.line.0 as usize {
            return true;
        }
        if self.start.line == self.end.line {
            return row == self.start.line.0 as usize
                && self.start.column.0 as usize <= col
                && col <= self.end.column.0 as usize;
        }
        if self.start.line.0 as usize == row && col >= self.start.column.0 as usize {
            return true;
        }
        self.end.line.0 as usize == row && col <= self.end.column.0 as usize
    }
}
