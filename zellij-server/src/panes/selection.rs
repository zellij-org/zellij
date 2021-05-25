use zellij_utils::{input::mouse::Position, logging::debug_log_to_file};

#[derive(Debug)]
pub struct Selection {
    pub range: Option<Range>,
}

impl Default for Selection {
    fn default() -> Self {
        Self { range: None }
    }
}

impl Selection {
    pub fn start(&mut self, start: Position) {
        debug_log_to_file(format!("setting selection start to {:?}", start))
            .expect("could not write to log file");
        self.range = Some(Range { start, end: start })
    }

    pub fn to(&mut self, to: Position) {
        debug_log_to_file(format!("setting selection end to {:?}", to))
            .expect("could not write to log file");
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
    pub start: Position,
    pub end: Position,
}

impl Range {
    fn contains(&self, row: usize, col: usize) -> bool {
        let start = if self.start <= self.end {
            self.start
        } else {
            self.end
        };

        let end = if self.end > self.start {
            self.end
        } else {
            self.start
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
}
