use zellij_utils::{input::mouse::Position, logging::debug_log_to_file};

#[derive(Debug, Clone)]
pub struct Selection {
    pub start: Position,
    pub end: Position,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
        }
    }
}

impl Selection {
    pub fn start(&mut self, start: Position) {
        debug_log_to_file(format!("setting selection start to {:?}", start))
            .expect("could not write to log file");
        self.start = start;
        self.end = start;
    }

    pub fn to(&mut self, to: Position) {
        debug_log_to_file(format!("setting selection end to {:?}", to))
            .expect("could not write to log file");
        self.end = to
    }

    pub fn contains(&self, row: usize, col: usize) -> bool {
        let (start, end) = if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };

        if (start.line.0 as usize) < row && row < end.line.0 as usize {
            return true;
        }
        if start.line == end.line {
            return row == start.line.0 as usize
                && start.column.0 as usize <= col
                && col < end.column.0 as usize;
        }
        if start.line.0 as usize == row && col >= start.column.0 as usize {
            return true;
        }
        end.line.0 as usize == row && col < end.column.0 as usize
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub fn reset(&mut self) {
        self.start.line.0 = 0;
        self.start.column.0 = 0;
        self.end.line.0 = 0;
        self.end.column.0 = 0;
    }

    pub fn sorted(&self) -> Self {
        let (start, end) = if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };
        Self { start, end }
    }

    pub fn line_indices(&self) -> std::ops::RangeInclusive<usize> {
        let sorted = self.sorted();
        sorted.start.line.0..=sorted.end.line.0
    }
}
