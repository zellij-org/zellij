use crate::panes::selection::Selection;
use crate::panes::terminal_character::TerminalCharacter;
use crate::panes::Row;
use std::borrow::Cow;
use std::fmt::Debug;
use zellij_utils::position::Position;

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
                    // We wrapped a line and the end is in this row, so take from the begging to the end
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

    /// Search a row and its tail.
    /// The tail are all the non-canonical lines below `row`, with `row` not necessarily being canonical itself.
    pub(crate) fn search_row(&self, mut ridx: usize, row: &Row, tail: &[&Row]) -> Vec<Selection> {
        #[derive(Debug)]
        enum SearchSource<'a> {
            Main(&'a Row),
            Tail(&'a Row),
        }

        let mut res = Vec::new();
        if self.needle.is_empty() || row.columns.is_empty() {
            return res;
        }

        let is_word_boundary =
            |x: Option<char>| x.map_or(true, |c| !c.is_ascii_alphanumeric() && c != '_');

        let mut tailit = tail.iter();
        let mut source = SearchSource::Main(row);
        let orig_ridx = ridx;
        let mut start = None;
        let mut nidx = 0; // Needle index
        let mut hidx = 0; // Haystack index
        let mut prev_haystack_char: Option<char> = None;
        loop {
            let mut haystack_char = match source {
                SearchSource::Main(row) => row.columns[hidx].character,
                SearchSource::Tail(tail) => tail.columns[hidx].character,
            };
            let next_haystack_char = if self.whole_word_only {
                // Everything (incl. end of line) that is not [a-zA-Z0-9_] is considered a word boundary
                match source {
                    SearchSource::Main(row) => row.columns.get(hidx + 1).map(|c| c.character),
                    SearchSource::Tail(tail) => tail.columns.get(hidx + 1).map(|c| c.character),
                }
            } else {
                None // Doesn't get used
            };

            let needle_char = self.needle.chars().nth(nidx).unwrap(); // Unwrapping is safe here
            let mut chars_match = if self.case_insensitive {
                // Currently only ascii, as this whole search-function is very sub-optimal anyways
                haystack_char.to_ascii_lowercase() == needle_char.to_ascii_lowercase()
            } else {
                haystack_char == needle_char
            };

            if chars_match
                && self.whole_word_only
                && nidx == 0
                && !is_word_boundary(prev_haystack_char)
            {
                // Start of the match is not a word boundary, so this is not a hit
                chars_match = false;
            }
            if chars_match {
                // If the needle is only 1 long, the next if could also happen, so we are not merging it into one big if-else
                if nidx == 0 {
                    start = Some(Position::new(ridx as i32, hidx as u16));
                }
                if nidx == self.needle.len() - 1 {
                    let mut end_found = true;
                    if self.whole_word_only && !is_word_boundary(next_haystack_char) {
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
                // Chars don't match
                start = None;
                nidx = 0;
                if matches!(source, SearchSource::Tail(..)) {
                    // When searching the tail and we find a mismatch, just quit right now
                    break;
                }
            }
            prev_haystack_char = Some(haystack_char);

            hidx += 1;
            match source {
                SearchSource::Main(row) => {
                    if hidx >= row.columns.len() {
                        let curr_tail = tailit.next();
                        // If we are at the end and found a partial hit, we have to extend the search into the next line
                        if let Some(curr_tail) = start.and(curr_tail) {
                            ridx += 1;
                            hidx = 0;
                            source = SearchSource::Tail(curr_tail);
                            continue;
                        } else {
                            break;
                        }
                    }
                },
                SearchSource::Tail(tail) => {
                    if hidx >= tail.columns.len() {
                        // If we are still searching (didn't hit a mismatch yet) and there is still more tail to go
                        // just continue with the next line
                        if let Some(curr_tail) = tailit.next() {
                            ridx += 1;
                            hidx = 0;
                            source = SearchSource::Tail(curr_tail);
                            continue;
                        } else {
                            break;
                        }
                    }
                },
            }
        }

        // The tail may have not been wrapped yet (when coming from lines_below),
        // so it could be that the end extends across more characters than the row is wide.
        // Therefore we need to reflow the end:
        for s in res.iter_mut() {
            while s.end.column() >= row.width() {
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
        lines_below: &[Row],
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
