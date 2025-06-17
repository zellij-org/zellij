use crate::panes::selection::Selection;
use crate::panes::terminal_character::TerminalCharacter;
use crate::panes::{Grid, Row};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Debug;
use zellij_utils::input::actions::SearchDirection;
use zellij_utils::position::Position;

// If char is neither alphanumeric nor an underscore do we consider it a word-boundary
fn is_word_boundary(x: &Option<char>) -> bool {
    x.map_or(true, |c| !c.is_ascii_alphanumeric() && c != '_')
}

#[derive(Debug)]
enum SearchSource<'a> {
    Main(&'a Row),
    Tail(&'a Row),
}

impl<'a> SearchSource<'a> {
    /// Returns true, if a new source was found, false otherwise (reached the end of the tail).
    /// If we are in the middle of a line, nothing will be changed.
    /// Only, when we have to switch to a new line, will the source update itself,
    /// as well as the corresponding indices.
    fn get_next_source(
        &mut self,
        ridx: &mut usize,
        hidx: &mut usize,
        tailit: &mut std::slice::Iter<&'a Row>,
        start: &Option<Position>,
    ) -> bool {
        match self {
            SearchSource::Main(row) => {
                // If we are at the end of the main row, we need to start looking into the tail
                if hidx >= &mut row.columns.len() {
                    let curr_tail = tailit.next();
                    // If we are at the end and found a partial hit, we have to extend the search into the next line
                    if let Some(curr_tail) = start.and(curr_tail) {
                        *ridx += 1; // Go one line down
                        *hidx = 0; // and start from the beginning of the new line
                        *self = SearchSource::Tail(curr_tail);
                    } else {
                        return false; // We reached the end of the tail
                    }
                }
            },
            SearchSource::Tail(tail) => {
                if hidx >= &mut tail.columns.len() {
                    // If we are still searching (didn't hit a mismatch yet) and there is still more tail to go
                    // just continue with the next line
                    if let Some(curr_tail) = tailit.next() {
                        *ridx += 1; // Go one line down
                        *hidx = 0; // and start from the beginning of the new line
                        *self = SearchSource::Tail(curr_tail);
                    } else {
                        return false; // We reached the end of the tail
                    }
                }
            },
        }
        // We have found a new source, or we are in the middle of a line, so no need to change anything
        true
    }

    // Get the char at hidx and, if existing, the following char as well
    fn get_next_two_chars(&self, hidx: usize, whole_word_search: bool) -> (char, Option<char>) {
        // Get the current haystack character
        let haystack_char = match self {
            SearchSource::Main(row) => row.columns[hidx].character,
            SearchSource::Tail(tail) => tail.columns[hidx].character,
        };

        // Get the next haystack character (relevant for whole-word search only)
        let next_haystack_char = if whole_word_search {
            // Everything (incl. end of line) that is not [a-zA-Z0-9_] is considered a word boundary
            match self {
                SearchSource::Main(row) => row.columns.get(hidx + 1).map(|c| c.character),
                SearchSource::Tail(tail) => tail.columns.get(hidx + 1).map(|c| c.character),
            }
        } else {
            None // Doesn't get used, when not doing whole-word search
        };
        (haystack_char, next_haystack_char)
    }
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
                        // Just mark the rest of the line. This number is certainly too big but the iterator takes care of this
                        row.columns.len()
                    };
                    (skip, take)
                } else if ridx as isize == s.end.line() {
                    // We wrapped a line and the end is in this row, so take from the beginning to the end
                    (0, s.end.column())
                } else {
                    // We are in the middle (start is above and end is below), so mark all
                    (0, row.columns.len())
                };

                row.to_mut()
                    .columns
                    .iter_mut()
                    .skip(skip)
                    .take(take)
                    .for_each(|x| *x = TerminalCharacter::new(replacement_char));
            }
        }
    }

    pub fn has_modifiers_set(&self) -> bool {
        self.wrap_search || self.whole_word_only || self.case_insensitive
    }

    fn check_if_haystack_char_matches_needle(
        &self,
        nidx: usize,
        needle_char: char,
        haystack_char: char,
        prev_haystack_char: Option<char>,
    ) -> bool {
        let mut chars_match = if self.case_insensitive {
            // Case insensitive search
            // Currently only ascii, as this whole search-function is very sub-optimal anyways
            haystack_char.to_ascii_lowercase() == needle_char.to_ascii_lowercase()
        } else {
            // Case sensitive search
            haystack_char == needle_char
        };

        // Whole-word search
        // It's a match only, if the first haystack char that is _not_ a hit, is a word-boundary
        if chars_match
            && self.whole_word_only
            && nidx == 0
            && !is_word_boundary(&prev_haystack_char)
        {
            // Start of the match is not a word boundary, so this is not a hit
            chars_match = false;
        }

        chars_match
    }

    /// Search a row and its tail.
    /// The tail are all the non-canonical lines below `row`, with `row` not necessarily being canonical itself.
    pub(crate) fn search_row(&self, mut ridx: usize, row: &Row, tail: &[&Row]) -> Vec<Selection> {
        let mut res = Vec::new();
        if self.needle.is_empty() || row.columns.is_empty() {
            return res;
        }

        let mut tailit = tail.iter();
        let mut source = SearchSource::Main(row); // Where we currently get the haystack-characters from
        let orig_ridx = ridx;
        let mut start = None; // If we find a hit, this is where it starts
        let mut nidx = 0; // Needle index
        let mut hidx = 0; // Haystack index
        let mut prev_haystack_char: Option<char> = None;
        loop {
            // Get the current and next haystack character
            let (mut haystack_char, next_haystack_char) =
                source.get_next_two_chars(hidx, self.whole_word_only);

            // Get current needle character
            let needle_char = self.needle.chars().nth(nidx).unwrap(); // Unwrapping is safe here

            // Check if needle and haystack match (with search-options)
            let chars_match = self.check_if_haystack_char_matches_needle(
                nidx,
                needle_char,
                haystack_char,
                prev_haystack_char,
            );

            if chars_match {
                // If the needle is only 1 long, the next `if` could also happen, so we are not merging it into one big if-else
                if nidx == 0 {
                    start = Some(Position::new(ridx as i32, hidx as u16));
                }
                if nidx == self.needle.len() - 1 {
                    let mut end_found = true;
                    // If we search whole-word-only, the next non-needle char needs to be a word-boundary,
                    // otherwise its not a hit (e.g. some occurrence inside a longer word).
                    if self.whole_word_only && !is_word_boundary(&next_haystack_char) {
                        // The end of the match is not a word boundary, so this is not a hit!
                        // We have to jump back from where we started (plus one char)
                        nidx = 0;
                        ridx = start.unwrap().line() as usize;
                        hidx = start.unwrap().column(); // Will be incremented below
                        if start.unwrap().line() as usize == orig_ridx {
                            source = SearchSource::Main(row);
                            haystack_char = row.columns[hidx].character; // so that prev_char gets set correctly
                        } else {
                            // The -1 comes from the main row
                            let tail_idx = start.unwrap().line() as usize - orig_ridx - 1;
                            // We have to reset the tail-iterator as well.
                            tailit = tail[tail_idx..].iter();
                            let trow = tailit.next().unwrap();
                            haystack_char = trow.columns[hidx].character; // so that prev_char gets set correctly
                            source = SearchSource::Tail(trow);
                        }
                        start = None;
                        end_found = false;
                    }
                    if end_found {
                        let mut selection = Selection::default();
                        selection.start(start.unwrap());
                        selection.end(Position::new(ridx as i32, (hidx + 1) as u16));
                        res.push(selection);
                        nidx = 0;
                        if matches!(source, SearchSource::Tail(..)) {
                            // When searching the tail, we can only find one additional selection, so stopping here
                            break;
                        }
                    }
                } else {
                    nidx += 1;
                }
            } else {
                // Chars don't match. Start searching the needle from the beginning
                start = None;
                nidx = 0;
                if matches!(source, SearchSource::Tail(..)) {
                    // When searching the tail and we find a mismatch, just quit right now
                    break;
                }
            }

            hidx += 1;
            prev_haystack_char = Some(haystack_char);
            // We might need to switch to a new line in the tail
            if !source.get_next_source(&mut ridx, &mut hidx, &mut tailit, &start) {
                break;
            }
        }

        // The tail may have not been wrapped yet (when coming from lines_below),
        // so it could be that the end extends across more characters than the row is wide.
        // Therefore we need to reflow the end:
        for s in res.iter_mut() {
            while s.end.column() > row.width() {
                s.end.column.0 -= row.width();
                s.end.line.0 += 1;
            }
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
        viewport: &[Row],
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
            if let Some(row) = viewport.first() {
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
        viewport: &[Row],
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
            if let Some(row) = viewport.last() {
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
