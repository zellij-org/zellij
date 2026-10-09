use crate::screen_buffer::{Occupancy, ScreenBuffer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub head: (usize, usize),
}

impl Selection {
    pub fn at(row: usize, col: usize) -> Self {
        Self {
            anchor: (row, col),
            head: (row, col),
        }
    }

    pub fn extended_to(self, row: usize, col: usize) -> Self {
        Self {
            head: (row, col),
            ..self
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn rows(&self) -> std::ops::RangeInclusive<usize> {
        let (start, end) = self.ordered();
        start.0..=end.0
    }

    pub fn covers(&self, row: usize, col: usize) -> bool {
        if self.is_empty() {
            return false;
        }
        let (start, end) = self.ordered();
        (start..=end).contains(&(row, col))
    }

    pub fn text(&self, buffer: &ScreenBuffer) -> String {
        if self.is_empty() {
            return String::new();
        }
        let size = buffer.size();
        let (start, end) = self.ordered();
        let last_row = end.0.min(size.rows.saturating_sub(1));
        let mut lines = Vec::new();
        for row in start.0..=last_row {
            let first = if row == start.0 { start.1 } else { 0 };
            let last = if row == end.0 {
                end.1.min(size.cols - 1)
            } else {
                size.cols - 1
            };
            let mut line = String::new();
            let mut col = first;
            if col < size.cols && col > 0 && buffer.occupancy(row, col) == Occupancy::WideTail {
                col -= 1;
            }
            while col <= last && col < size.cols {
                if buffer.occupancy(row, col) != Occupancy::WideTail {
                    line.push(buffer.cell(row, col).character());
                }
                col += 1;
            }
            lines.push(line.trim_end().to_owned());
        }
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_buffer::painter::Painter;

    fn screen(paint: impl FnOnce(&mut Painter)) -> crate::terminal::TerminalState {
        Painter::state(3, 10, paint)
    }

    #[test]
    fn a_selection_on_one_row_takes_the_cells_between_its_ends() {
        let state = screen(|p| p.text(0, 0, "hello world"));
        let selection = Selection::at(0, 2).extended_to(0, 6);
        assert_eq!(selection.text(state.screen()), "llo w");
    }

    #[test]
    fn a_selection_dragged_backwards_reads_the_same_as_forwards() {
        let state = screen(|p| p.text(0, 0, "hello world"));
        assert_eq!(
            Selection::at(0, 6).extended_to(0, 2).text(state.screen()),
            "llo w"
        );
    }

    #[test]
    fn a_selection_over_rows_flows_like_text_and_drops_trailing_blanks() {
        let state = screen(|p| {
            p.text(0, 0, "Tab #1  x");
            p.text(1, 0, "│ ls   │");
            p.text(2, 0, "status");
        });
        let selection = Selection::at(0, 4).extended_to(2, 2);
        assert_eq!(selection.text(state.screen()), "#1  x\n│ ls   │\nsta");
    }

    #[test]
    fn the_right_half_of_a_wide_character_is_not_copied_twice() {
        let state = screen(|p| {
            p.text(0, 0, "a");
            p.wide(0, 1, '漢');
            p.text(0, 3, "b");
        });
        assert_eq!(
            Selection::at(0, 0).extended_to(0, 3).text(state.screen()),
            "a漢b"
        );
        assert_eq!(
            Selection::at(0, 2).extended_to(0, 3).text(state.screen()),
            "漢b"
        );
    }

    #[test]
    fn a_selection_that_never_moved_covers_and_copies_nothing() {
        let state = screen(|p| p.text(0, 0, "hello"));
        let selection = Selection::at(0, 1);
        assert!(selection.is_empty());
        assert!(!selection.covers(0, 1));
        assert_eq!(selection.text(state.screen()), "");
    }

    #[test]
    fn coverage_follows_the_flow_of_text() {
        let selection = Selection::at(0, 5).extended_to(2, 1);
        assert!(!selection.covers(0, 4));
        assert!(selection.covers(0, 5));
        assert!(selection.covers(0, 9));
        assert!(selection.covers(1, 0));
        assert!(selection.covers(2, 1));
        assert!(!selection.covers(2, 2));
        assert_eq!(selection.rows(), 0..=2);
    }

    #[test]
    fn a_selection_beyond_a_shrunken_screen_reads_only_what_is_there() {
        let state = screen(|p| p.text(2, 0, "end"));
        assert_eq!(
            Selection::at(2, 0).extended_to(9, 40).text(state.screen()),
            "end"
        );
    }
}
