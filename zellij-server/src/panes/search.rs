use super::Selection;
use crate::panes::grapheme_join;
use crate::panes::terminal_character::TerminalCharacter;
use crate::panes::{Grid, Row};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Debug;
use zellij_utils::input::actions::SearchDirection;
use zellij_utils::position::Position;

fn is_word_boundary(x: &Option<char>) -> bool {
    x.map_or(true, |c| !c.is_alphanumeric() && c != '_')
}

struct HaystackCell {
    byte_start: usize,
    row: usize,
    column: usize,
    width: usize,
}

struct Haystack {
    text: String,
    cells: Vec<HaystackCell>,
}

impl Haystack {
    fn new(ridx: usize, row: &Row, tail: &[&Row]) -> Self {
        let mut text = String::new();
        let mut cells = Vec::new();
        for (offset, current) in std::iter::once(row).chain(tail.iter().copied()).enumerate() {
            let mut column = 0;
            for cell in &current.columns {
                cells.push(HaystackCell {
                    byte_start: text.len(),
                    row: ridx + offset,
                    column,
                    width: cell.width(),
                });
                cell.push_text_to(&mut text);
                column += cell.width();
            }
        }
        Haystack { text, cells }
    }

    fn cell_containing(&self, byte_offset: usize) -> Option<&HaystackCell> {
        let index = self
            .cells
            .partition_point(|cell| cell.byte_start <= byte_offset)
            .checked_sub(1)?;
        self.cells.get(index)
    }
}

struct FoldedText {
    text: String,
    original_starts: Vec<(usize, usize)>,
}

impl FoldedText {
    fn new(original: &str) -> Self {
        let mut text = String::with_capacity(original.len());
        let mut original_starts = Vec::with_capacity(original.len() + 1);
        for (offset, c) in original.char_indices() {
            original_starts.push((text.len(), offset));
            text.extend(c.to_lowercase());
        }
        original_starts.push((text.len(), original.len()));
        FoldedText {
            text,
            original_starts,
        }
    }

    fn original_offset(&self, folded_offset: usize) -> Option<usize> {
        self.original_starts
            .binary_search_by_key(&folded_offset, |(folded, _)| *folded)
            .ok()
            .map(|index| self.original_starts[index].1)
    }
}

fn fold_case(text: &str) -> String {
    text.chars().flat_map(|c| c.to_lowercase()).collect()
}

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    // What we have already found in the viewport
    pub selections: Vec<Selection>,
    // Which of the selections we found is currently 'active' (highlighted differently)
    pub active: Option<Selection>,
    // What we are looking for
    pub needle: String,
    // Does case matter?
    pub case_insensitive: bool,
    // Only search whole words, not parts inside a word
    pub whole_word_only: bool, // TODO
    // Jump from the bottom to the top (or vice versa), if we run out of lines to search
    pub wrap_search: bool,
}

impl SearchResult {
    /// This is only used for Debug formatting Grid, which itself is only used
    /// for tests.
    #[allow(clippy::ptr_arg)]
    pub(crate) fn mark_search_results_in_row(&self, row: &mut Cow<Row>, ridx: usize) {
        for s in &self.selections {
            if s.contains_row(ridx) {
                let replacement_char = if Some(s) == self.active.as_ref() {
                    '_'
                } else {
                    '#'
                };

                let (skip, take) = if ridx as isize == s.start.line() {
                    let skip = s.start.column();
                    let take = if s.end.line() == s.start.line() {
                        s.end.column() - s.start.column()
                    } else {
                        usize::MAX
                    };
                    (skip, take)
                } else if ridx as isize == s.end.line() {
                    // We wrapped a line and the end is in this row, so take from the beginning to the end
                    (0, s.end.column())
                } else {
                    // We are in the middle (start is above and end is below), so mark all
                    (0, usize::MAX)
                };

                let take_end = skip.saturating_add(take);
                let mut column = 0;
                for cell in row.to_mut().columns.iter_mut() {
                    let width = cell.width();
                    if column >= skip && column < take_end {
                        *cell = TerminalCharacter::new(replacement_char);
                    }
                    column += width;
                }
            }
        }
    }

    pub fn has_modifiers_set(&self) -> bool {
        self.wrap_search || self.whole_word_only || self.case_insensitive
    }

    fn find_matches_in(&self, haystack: &Haystack) -> Vec<(usize, usize)> {
        let original = haystack.text.as_str();
        let mut found = Vec::new();
        let folded;
        let folded_needle;
        let (search_text, needle) = if self.case_insensitive {
            folded = FoldedText::new(original);
            folded_needle = fold_case(&self.needle);
            (folded.text.as_str(), folded_needle.as_str())
        } else {
            folded = FoldedText {
                text: String::new(),
                original_starts: Vec::new(),
            };
            (original, self.needle.as_str())
        };
        if needle.is_empty() {
            return found;
        }
        let mut position = 0;
        while position <= search_text.len() {
            let Some(relative) = search_text[position..].find(needle) else {
                break;
            };
            let start = position + relative;
            let end = start + needle.len();
            let original_range = if self.case_insensitive {
                folded
                    .original_offset(start)
                    .zip(folded.original_offset(end))
            } else {
                Some((start, end))
            };
            let accepted = original_range.filter(|(original_start, original_end)| {
                self.accepts_match(original, *original_start, *original_end)
            });
            match accepted {
                Some(range) => {
                    found.push(range);
                    position = end;
                },
                None => {
                    position = start
                        + search_text[start..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(1);
                },
            }
        }
        found
    }

    fn accepts_match(&self, text: &str, start: usize, end: usize) -> bool {
        if start >= end {
            return false;
        }
        if !grapheme_join::is_boundary_at(text, start) || !grapheme_join::is_boundary_at(text, end)
        {
            return false;
        }
        if self.whole_word_only {
            let before = text[..start].chars().next_back();
            let after = text[end..].chars().next();
            if !is_word_boundary(&before) || !is_word_boundary(&after) {
                return false;
            }
        }
        true
    }

    /// Search a row and its tail.
    /// The tail are all the non-canonical lines below `row`, with `row` not necessarily being canonical itself.
    pub(crate) fn search_row(&self, ridx: usize, row: &Row, tail: &[&Row]) -> Vec<Selection> {
        let mut res = Vec::new();
        if self.needle.is_empty() || row.columns.is_empty() {
            return res;
        }
        let haystack = Haystack::new(ridx, row, tail);
        for (start, end) in self.find_matches_in(&haystack) {
            let (Some(first), Some(last)) = (
                haystack.cell_containing(start),
                haystack.cell_containing(end - 1),
            ) else {
                continue;
            };
            if first.row != ridx {
                continue;
            }
            let mut selection = Selection::default();
            selection.start(Position::new(first.row as i32, first.column as u16));
            selection.end(Position::new(
                last.row as i32,
                (last.column + last.width) as u16,
            ));
            res.push(selection);
        }
        res
    }

    pub(crate) fn move_active_selection_to_next(&mut self) {
        if let Some(active_idx) = self.active {
            self.active = self
                .selections
                .iter()
                .skip_while(|s| *s != &active_idx)
                .nth(1)
                .cloned();
        } else {
            self.active = self.selections.first().cloned();
        }
    }

    pub(crate) fn move_active_selection_to_prev(&mut self) {
        if let Some(active_idx) = self.active {
            self.active = self
                .selections
                .iter()
                .rev()
                .skip_while(|s| *s != &active_idx)
                .nth(1)
                .cloned();
        } else {
            self.active = self.selections.last().cloned();
        }
    }

    pub(crate) fn unset_active_selection_if_nonexistent(&mut self) {
        if let Some(active_idx) = self.active {
            if !self.selections.contains(&active_idx) {
                self.active = None;
            }
        }
    }

    pub(crate) fn move_down(
        &mut self,
        amount: usize,
        viewport: &VecDeque<Row>,
        grid_height: usize,
    ) -> bool {
        let mut found_something = false;
        self.selections
            .iter_mut()
            .chain(self.active.iter_mut())
            .for_each(|x| x.move_down(amount));

        // Throw out all search-results outside of the new viewport
        self.adjust_selections_to_moved_viewport(grid_height);

        // Search the new line for our needle
        if !self.needle.is_empty() {
            if let Some(row) = viewport.front() {
                let mut tail = Vec::new();
                loop {
                    let tail_idx = 1 + tail.len();
                    if tail_idx < viewport.len() && !viewport[tail_idx].is_canonical {
                        tail.push(&viewport[tail_idx]);
                    } else {
                        break;
                    }
                }
                let selections = self.search_row(0, row, &tail);
                for selection in selections.iter().rev() {
                    self.selections.insert(0, *selection);
                    found_something = true;
                }
            }
        }
        found_something
    }

    pub(crate) fn move_up(
        &mut self,
        amount: usize,
        viewport: &VecDeque<Row>,
        lines_below: &VecDeque<Row>,
        grid_height: usize,
    ) -> bool {
        let mut found_something = false;
        self.selections
            .iter_mut()
            .chain(self.active.iter_mut())
            .for_each(|x| x.move_up(amount));
        // Throw out all search-results outside of the new viewport
        self.adjust_selections_to_moved_viewport(grid_height);

        // Search the new line for our needle
        if !self.needle.is_empty() {
            if let Some(row) = viewport.back() {
                let tail: Vec<&Row> = lines_below.iter().take_while(|r| !r.is_canonical).collect();
                let selections = self.search_row(viewport.len() - 1, row, &tail);
                for selection in selections {
                    // We are only interested in results that start in the this new row
                    if selection.start.line() as usize == viewport.len() - 1 {
                        self.selections.push(selection);
                        found_something = true;
                    }
                }
            }
        }
        found_something
    }

    fn adjust_selections_to_moved_viewport(&mut self, grid_height: usize) {
        // Throw out all search-results outside of the new viewport
        self.selections
            .retain(|s| (s.start.line() as usize) < grid_height && s.end.line() >= 0);
        // If we have thrown out the active element, set it to None
        self.unset_active_selection_if_nonexistent();
    }
}

impl Grid {
    pub fn search_down(&mut self) {
        self.search_scrollbuffer(SearchDirection::Down);
    }

    pub fn search_up(&mut self) {
        self.search_scrollbuffer(SearchDirection::Up);
    }

    pub fn clear_search(&mut self) {
        // Clearing all previous highlights
        for res in &self.search_results.selections {
            self.output_buffer
                .update_lines(res.start.line() as usize, res.end.line() as usize);
        }
        self.search_results = Default::default();
    }

    pub fn set_search_string(&mut self, needle: &str) {
        self.search_results.needle = needle.to_string();
        self.search_viewport();
        // If the current viewport does not contain any hits,
        // we jump around until we find something. Starting
        // going backwards.
        if self.search_results.selections.is_empty() {
            self.search_up();
        }
        if self.search_results.selections.is_empty() {
            self.search_down();
        }
        // We still don't want to pre-select anything at this stage
        self.search_results.active = None;
        self.is_scrolled = true;
    }

    pub fn search_viewport(&mut self) {
        for ridx in 0..self.viewport.len() {
            let row = &self.viewport[ridx];
            let mut tail = Vec::new();
            loop {
                let tail_idx = ridx + tail.len() + 1;
                if tail_idx < self.viewport.len() && !self.viewport[tail_idx].is_canonical {
                    tail.push(&self.viewport[tail_idx]);
                } else {
                    break;
                }
            }
            let selections = self.search_results.search_row(ridx, row, &tail);
            for sel in &selections {
                // Cast works because we can' be negative here
                self.output_buffer
                    .update_lines(sel.start.line() as usize, sel.end.line() as usize);
            }

            for selection in selections {
                self.search_results.selections.push(selection);
            }
        }
    }

    pub fn toggle_search_case_sensitivity(&mut self) {
        self.search_results.case_insensitive = !self.search_results.case_insensitive;
        for line in self.search_results.selections.drain(..) {
            self.output_buffer
                .update_lines(line.start.line() as usize, line.end.line() as usize);
        }
        self.search_viewport();
        // Maybe the selection we had is now gone
        self.search_results.unset_active_selection_if_nonexistent();
    }

    pub fn toggle_search_wrap(&mut self) {
        self.search_results.wrap_search = !self.search_results.wrap_search;
    }

    pub fn toggle_search_whole_words(&mut self) {
        self.search_results.whole_word_only = !self.search_results.whole_word_only;
        for line in self.search_results.selections.drain(..) {
            self.output_buffer
                .update_lines(line.start.line() as usize, line.end.line() as usize);
        }
        self.search_results.active = None;
        self.search_viewport();
        // Maybe the selection we had is now gone
        self.search_results.unset_active_selection_if_nonexistent();
    }

    fn search_scrollbuffer(&mut self, dir: SearchDirection) {
        let first_sel = self.search_results.selections.first();
        let last_sel = self.search_results.selections.last();

        let search_viewport_for_the_first_time =
            self.search_results.active.is_none() && !self.search_results.selections.is_empty();

        // We are not at the end yet, so we can iterate to the next search-result within the current viewport
        let search_viewport_again = !self.search_results.selections.is_empty()
            && self.search_results.active.is_some()
            && match dir {
                SearchDirection::Up => self.search_results.active.as_ref() != first_sel,
                SearchDirection::Down => self.search_results.active.as_ref() != last_sel,
            };

        if search_viewport_for_the_first_time || search_viewport_again {
            // We can stay in the viewport and just move the active selection
            self.search_viewport_again(search_viewport_for_the_first_time, dir);
        } else {
            // Need to move the viewport
            let found_something = self.search_viewport_move(dir);

            // We haven't found anything, but we are allowed to wrap around
            if !found_something && self.search_results.wrap_search {
                self.search_viewport_wrap(dir);
            }
        }
    }

    fn search_viewport_again(
        &mut self,
        search_viewport_for_the_first_time: bool,
        dir: SearchDirection,
    ) {
        let new_active = match dir {
            SearchDirection::Up => self.search_results.selections.last().cloned().unwrap(),
            SearchDirection::Down => self.search_results.selections.first().cloned().unwrap(),
        };
        // We can stay in the viewport and just move the active selection
        let active_idx = self.search_results.active.get_or_insert(new_active);
        self.output_buffer.update_lines(
            active_idx.start.line() as usize,
            active_idx.end.line() as usize,
        );
        if !search_viewport_for_the_first_time {
            match dir {
                SearchDirection::Up => self.search_results.move_active_selection_to_prev(),
                SearchDirection::Down => self.search_results.move_active_selection_to_next(),
            };
            if let Some(new_active) = self.search_results.active {
                self.output_buffer.update_lines(
                    new_active.start.line() as usize,
                    new_active.end.line() as usize,
                );
            }
        }
    }

    fn search_reached_opposite_end(&mut self, dir: SearchDirection) -> bool {
        match dir {
            SearchDirection::Up => self.lines_above.is_empty(),
            SearchDirection::Down => self.lines_below.is_empty(),
        }
    }

    fn search_viewport_move(&mut self, dir: SearchDirection) -> bool {
        // We need to move the viewport
        let mut rows = 0;
        let mut found_something = false;

        // We might loose the current selection, if we can't find anything
        let current_active_selection = self.search_results.active;
        while !found_something && !self.search_reached_opposite_end(dir) {
            rows += 1;
            found_something = match dir {
                SearchDirection::Up => self.scroll_up_one_line(),
                SearchDirection::Down => self.scroll_down_one_line(),
            };
        }

        if found_something {
            self.search_adjust_to_new_selection(dir);
        } else {
            // We didn't find something, so we scroll back to the start
            for _ in 0..rows {
                match dir {
                    SearchDirection::Up => self.scroll_down_one_line(),
                    SearchDirection::Down => self.scroll_up_one_line(),
                };
            }
            self.search_results.active = current_active_selection;
        }
        found_something
    }

    fn search_adjust_to_new_selection(&mut self, dir: SearchDirection) {
        match dir {
            SearchDirection::Up => {
                self.search_results.move_active_selection_to_prev();
            },
            SearchDirection::Down => {
                // We may need to scroll a bit further, because we are at the beginning of the
                // search result, but the end might be invisible
                if let Some(last) = self.search_results.selections.last() {
                    let distance = (last.end.line() - last.start.line()) as usize;
                    if distance < self.height {
                        for _ in 0..distance {
                            self.scroll_down_one_line();
                        }
                    }
                }
                self.search_results.move_active_selection_to_next();
            },
        }
        self.output_buffer.update_all_lines();
    }

    fn search_viewport_wrap(&mut self, dir: SearchDirection) {
        // We might loose the current selection, if we can't find anything
        let current_active_selection = self.search_results.active;
        // UP
        // Go to the opposite end (bottom when searching up and top when searching down)
        let mut rows = self.move_viewport_to_opposite_end(dir);

        // We are at the bottom or top. Maybe we found already something there
        // If not, scroll back again, until we find something
        let mut found_something = match dir {
            SearchDirection::Up => self.search_results.selections.last().is_some(),
            SearchDirection::Down => self.search_results.selections.first().is_some(),
        };

        // We didn't find anything at the opposing end of the scrollbuffer, so we scroll back until we find something
        if !found_something {
            while rows >= 0 && !found_something {
                rows -= 1;
                found_something = match dir {
                    SearchDirection::Up => self.scroll_up_one_line(),
                    SearchDirection::Down => self.scroll_down_one_line(),
                };
            }
        }
        if found_something {
            self.search_results.active = match dir {
                SearchDirection::Up => self.search_results.selections.last().cloned(),
                SearchDirection::Down => {
                    // We need to scroll until the found item is at the top
                    if let Some(first) = self.search_results.selections.first() {
                        for _ in 0..first.start.line() {
                            self.scroll_down_one_line();
                        }
                    }
                    self.search_results.selections.first().cloned()
                },
            };
            self.output_buffer.update_all_lines();
        } else {
            // We didn't find anything, so we reset the old active selection
            self.search_results.active = current_active_selection;
        }
    }

    fn move_viewport_to_opposite_end(&mut self, dir: SearchDirection) -> isize {
        let mut rows = 0;
        match dir {
            SearchDirection::Up => {
                // Go to the bottom
                while !self.lines_below.is_empty() {
                    rows += 1;
                    self.scroll_down_one_line();
                }
            },
            SearchDirection::Down => {
                // Go to the top
                while !self.lines_above.is_empty() {
                    rows += 1;
                    self.scroll_up_one_line();
                }
            },
        }
        rows
    }
}
