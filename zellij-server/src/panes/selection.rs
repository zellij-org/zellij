use std::{collections::HashSet, ops::Range};

use zellij_utils::position::Position;

// The selection is empty when start == end
// it includes the character at start, and everything before end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub start: Position,
    pub end: Position,
    active: bool, // used to handle moving the selection up and down
    last_added_word_position: Option<(Position, Position)>, // (start / end)
    last_added_line_index: Option<isize>,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            start: Position::new(0, 0),
            end: Position::new(0, 0),
            active: false,
            last_added_word_position: None,
            last_added_line_index: None,
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

    pub fn end(&mut self, end: Position) {
        self.active = false;
        self.end = end;
    }

    pub fn set_start_and_end_positions(&mut self, start: Position, end: Position) {
        self.active = true;
        self.start = start;
        self.end = end;
        self.last_added_word_position = Some((start, end));
        self.last_added_line_index = Some(start.line.0);
    }
    pub fn add_word_to_position(&mut self, word_start: Position, word_end: Position) {
        // here we assume word_start is smaller or equal to word_end
        let already_added = self
            .last_added_word_position
            .map(|(last_word_start, last_word_end)| {
                last_word_start == word_start && last_word_end == word_end
            })
            .unwrap_or(false);
        if already_added {
            return;
        }
        let word_is_above_last_added_word = self
            .last_added_word_position
            .map(|(l_start, _l_end)| word_start.line < l_start.line)
            .unwrap_or(false);
        let word_is_below_last_added_word = self
            .last_added_word_position
            .map(|(_l_start, l_end)| word_end.line > l_end.line)
            .unwrap_or(false);
        if word_is_above_last_added_word && word_start.line < self.start.line {
            // extend line above
            self.start = word_start;
        } else if word_is_below_last_added_word && word_end.line > self.end.line {
            // extend line below
            self.end = word_end;
        } else if word_is_below_last_added_word && word_start.line > self.start.line {
            // reduce from above
            self.start = word_start;
        } else if word_is_above_last_added_word && word_end.line < self.end.line {
            // reduce from below
            self.end = word_end;
        } else {
            let word_end_is_to_the_left_of_last_word_start = self
                .last_added_word_position
                .map(|(l_start, _l_end)| word_end.column <= l_start.column)
                .unwrap_or(false);
            let word_start_is_to_the_right_of_last_word_end = self
                .last_added_word_position
                .map(|(_l_start, l_end)| word_start.column >= l_end.column)
                .unwrap_or(false);
            let last_word_start_equals_word_end = self
                .last_added_word_position
                .map(|(l_start, _l_end)| l_start.column == word_end.column)
                .unwrap_or(false);
            let last_word_end_equals_word_start = self
                .last_added_word_position
                .map(|(_l_start, l_end)| l_end.column == word_start.column)
                .unwrap_or(false);
            let selection_start_column_is_to_the_right_of_word_start =
                self.start.column > word_start.column;
            let selection_start_is_on_same_line_as_word_start = self.start.line == word_start.line;
            let selection_end_is_to_the_left_of_word_end = self.end.column < word_end.column;
            let selection_end_is_on_same_line_as_word_end = self.end.line == word_end.line;
            if word_end_is_to_the_left_of_last_word_start
                && selection_start_column_is_to_the_right_of_word_start
                && selection_start_is_on_same_line_as_word_start
            {
                // extend selection left
                self.start.column = word_start.column;
            } else if word_start_is_to_the_right_of_last_word_end
                && selection_end_is_to_the_left_of_word_end
                && selection_end_is_on_same_line_as_word_end
            {
                // extend selection right
                self.end.column = word_end.column;
            } else if last_word_start_equals_word_end {
                // reduce selection from the right
                self.end.column = word_end.column;
            } else if last_word_end_equals_word_start {
                // reduce selection from the left
                self.start.column = word_start.column;
            }
        }
        self.last_added_word_position = Some((word_start, word_end));
    }
    pub fn add_line_to_position(&mut self, line_index: isize, last_index_in_line: usize) {
        let already_added = self
            .last_added_line_index
            .map(|last_added_line_index| last_added_line_index == line_index)
            .unwrap_or(false);
        if already_added {
            return;
        }
        let line_index_is_smaller_than_last_added_line_index = self
            .last_added_line_index
            .map(|last| line_index < last)
            .unwrap_or(false);
        let line_index_is_larger_than_last_added_line_index = self
            .last_added_line_index
            .map(|last| line_index > last)
            .unwrap_or(false);

        if line_index_is_smaller_than_last_added_line_index && self.start.line.0 > line_index {
            // extend selection one line upwards
            self.start = Position::new(line_index as i32, 0);
        } else if line_index_is_larger_than_last_added_line_index && self.end.line.0 < line_index {
            // extend selection one line downwards
            self.end = Position::new(line_index as i32, last_index_in_line as u16);
        } else if line_index_is_smaller_than_last_added_line_index && self.end.line.0 > line_index {
            // reduce selection one line from below
            self.end = Position::new(line_index as i32, last_index_in_line as u16);
        } else if line_index_is_larger_than_last_added_line_index && self.start.line.0 < line_index
        {
            // reduce selection one line from above
            self.start = Position::new(line_index as i32, 0);
        }

        self.last_added_line_index = Some(line_index);
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

    pub fn contains_row(&self, row: usize) -> bool {
        let row = row as isize;
        let (start, end) = if self.start <= self.end {
            (self.start, self.end)
        } else {
            (self.end, self.start)
        };
        start.line.0 <= row && end.line.0 >= row
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
            last_added_word_position: self.last_added_word_position,
            last_added_line_index: self.last_added_line_index,
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
    pub fn offset(mut self, offset_x: usize, offset_y: usize) -> Self {
        self.start.line.0 += offset_y as isize;
        self.end.line.0 += offset_y as isize;
        self.start.column.0 += offset_x;
        self.end.column.0 += offset_x;
        self
    }

    /// Return an iterator over the line indices, up to max, that are not present in both self and other,
    /// except for the indices of the first and last line of both self and s2, that are always included.
    pub fn diff(&self, other: &Self, max: usize) -> impl Iterator<Item = isize> {
        let mut lines_to_update = HashSet::new();

        lines_to_update.insert(self.start.line.0);
        lines_to_update.insert(self.end.line.0);
        lines_to_update.insert(other.start.line.0);
        lines_to_update.insert(other.end.line.0);

        let old_lines: HashSet<isize> = self.get_visible_indices(max).collect();
        let new_lines: HashSet<isize> = other.get_visible_indices(max).collect();

        old_lines.symmetric_difference(&new_lines).for_each(|&l| {
            let _ = lines_to_update.insert(l);
        });

        lines_to_update
            .into_iter()
            .filter(move |&l| l >= 0 && l < max as isize)
    }

    fn get_visible_indices(&self, max: usize) -> Range<isize> {
        let Selection { start, end, .. } = self.sorted();
        let start = start.line.0.max(0);
        let end = end.line.0.min(max as isize);
        start..end
    }
}

#[cfg(test)]
#[path = "./unit/selection_tests.rs"]
mod selection_tests;
