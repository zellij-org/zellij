use super::Selection;
use crate::panes::grapheme_join;
use crate::panes::terminal_character::TerminalCharacter;
use crate::panes::{Grid, Row};
use std::borrow::Cow;
use std::collections::VecDeque;
use std::fmt::Debug;
use zellij_utils::input::actions::SearchDirection;
use zellij_utils::position::Position;

/// A search that has moved the viewport and still has history to scan.
/// One turn moves at most [`crate::panes::grid::SCREEN_ROW_QUANTUM`] rows.
#[derive(Clone)]
pub(crate) struct PendingSearch {
    dir: SearchDirection,
    phase: SearchPhase,
    rows: isize,
    origin: Option<Selection>,
    /// The direction to try if this scan finishes without a hit.
    fallback: Option<SearchDirection>,
}

#[derive(Clone, Copy)]
enum SearchPhase {
    /// Walking toward a hit from the current viewport.
    Move,
    /// Putting the viewport back after a scan that found nothing.
    Undo,
    /// Wrapped search is moving to the opposite end.
    GoToEnd,
    /// Wrapped to the opposite end and is walking back.
    WrapBack,
}

fn is_word_boundary(x: &Option<char>) -> bool {
    x.map_or(true, |c| !c.is_alphanumeric() && c != '_')
}

struct HaystackCell {
    byte_start: usize,
    row: usize,
    column: usize,
    width: usize,
}

struct HaystackRow {
    byte_start: usize,
    byte_end: usize,
}

struct Haystack {
    text: String,
    cells: Vec<HaystackCell>,
    row: HaystackRow,
    folded: Option<FoldedText>,
}

impl Haystack {
    /// Materialize `first` plus just enough continuation rows for a match that
    /// starts in `first` to finish and see one boundary character after it.
    fn new<'a>(
        ridx: usize,
        first: &Row,
        below: impl Iterator<Item = (usize, &'a Row)>,
        needle_len: usize,
        case_insensitive: bool,
    ) -> Self {
        let mut text = String::new();
        let mut cells = Vec::new();
        let mut folded = case_insensitive.then(FoldedTextBuilder::default);
        let push_row = |text: &mut String,
                        cells: &mut Vec<HaystackCell>,
                        folded: &mut Option<FoldedTextBuilder>,
                        row_offset: usize,
                        current: &Row| {
            let mut column = 0;
            for cell in &current.columns {
                cells.push(HaystackCell {
                    byte_start: text.len(),
                    row: ridx + row_offset,
                    column,
                    width: cell.width(),
                });
                let cell_start = text.len();
                cell.push_text_to(text);
                if let Some(folded) = folded.as_mut() {
                    for (offset, c) in text[cell_start..].char_indices() {
                        folded.push_char(cell_start + offset, c);
                    }
                }
                column += cell.width();
            }
        };
        push_row(&mut text, &mut cells, &mut folded, 0, first);
        let row = HaystackRow {
            byte_start: 0,
            byte_end: text.len(),
        };
        let measure_len = |text: &String, folded: &Option<FoldedTextBuilder>| {
            folded
                .as_ref()
                .map_or(text.len(), |folded| folded.text.len())
        };
        let target = measure_len(&text, &folded).saturating_add(needle_len);
        let mut rows_pushed = 1;
        let mut below = below;
        let mut line_ended = false;
        while measure_len(&text, &folded) < target {
            match below.next() {
                Some((row_offset, row)) if !row.is_canonical => {
                    push_row(&mut text, &mut cells, &mut folded, row_offset, row);
                    rows_pushed += 1;
                },
                _ => {
                    line_ended = true;
                    break;
                },
            }
        }
        // A match can end exactly at `target`; keep one following character
        // when the logical line continues so boundary checks see real context.
        if !line_ended && measure_len(&text, &folded) == target {
            if let Some((row_offset, row)) = below.next().filter(|(_, row)| !row.is_canonical) {
                push_row(&mut text, &mut cells, &mut folded, row_offset, row);
                rows_pushed += 1;
            }
        }
        #[cfg(test)]
        SEARCH_WORK.with(|counter| {
            let work = counter.get();
            counter.set(SearchWork {
                windows: work.windows + 1,
                rows: work.rows + rows_pushed,
                cells: work.cells + cells.len(),
                bytes: work.bytes + text.len(),
            });
        });
        #[cfg(not(test))]
        let _ = rows_pushed;
        let original_len = text.len();
        Haystack {
            text,
            cells,
            row,
            folded: folded.map(|folded| folded.finish(original_len)),
        }
    }

    fn cell_containing(&self, byte_offset: usize) -> Option<&HaystackCell> {
        let index = self
            .cells
            .partition_point(|cell| cell.byte_start <= byte_offset)
            .checked_sub(1)?;
        self.cells.get(index)
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchWork {
    pub windows: usize,
    pub rows: usize,
    pub cells: usize,
    pub bytes: usize,
}

#[cfg(test)]
thread_local! {
    static SEARCH_WORK: std::cell::Cell<SearchWork> = std::cell::Cell::new(SearchWork {
        windows: 0,
        rows: 0,
        cells: 0,
        bytes: 0,
    });
}

#[cfg(test)]
pub(crate) fn search_work() -> SearchWork {
    SEARCH_WORK.with(|work| work.get())
}

#[cfg(test)]
pub(crate) fn reset_search_work() {
    SEARCH_WORK.with(|work| work.set(SearchWork::default()));
}

/// The rows of the logical line that starts at `index`: the row itself plus
/// the non-canonical rows that continue it.
fn viewport_line<'a>(viewport: &'a VecDeque<Row>, index: usize) -> Vec<&'a Row> {
    let mut rows = Vec::new();
    for (offset, row) in viewport.iter().skip(index).enumerate() {
        if offset > 0 && row.is_canonical {
            break;
        }
        rows.push(row);
    }
    rows
}

struct FoldedText {
    text: String,
    original_starts: Vec<(usize, usize)>,
}

#[derive(Default)]
struct FoldedTextBuilder {
    text: String,
    original_starts: Vec<(usize, usize)>,
}

impl FoldedTextBuilder {
    fn push_char(&mut self, original_offset: usize, c: char) {
        self.original_starts
            .push((self.text.len(), original_offset));
        self.text.extend(c.to_lowercase());
    }

    fn finish(mut self, original_len: usize) -> FoldedText {
        self.original_starts.push((self.text.len(), original_len));
        FoldedText {
            text: self.text,
            original_starts: self.original_starts,
        }
    }
}

impl FoldedText {
    fn original_offset(&self, folded_offset: usize) -> Option<usize> {
        self.original_starts
            .binary_search_by_key(&folded_offset, |(folded, _)| *folded)
            .ok()
            .map(|index| self.original_starts[index].1)
    }

    /// The offset in the folded text of a char boundary in the original text.
    fn offset_of_original(&self, original_offset: usize) -> Option<usize> {
        self.original_starts
            .binary_search_by_key(&original_offset, |(_, original)| *original)
            .ok()
            .map(|index| self.original_starts[index].0)
    }
}

fn fold_case(text: &str) -> String {
    text.chars().flat_map(|c| c.to_lowercase()).collect()
}

/// Round `offset` up to the next char boundary, clamped to `text.len()`.
fn ceil_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset < text.len() && !text.is_char_boundary(offset) {
        offset += 1;
    }
    offset
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

    /// Find the matches that start in one row of a haystack.
    ///
    /// The row is matched as if it were the first row of its own haystack: a
    /// match must start before the row ends, non-overlapping matching restarts
    /// at the row start, and that start counts as a grapheme/word boundary.
    /// Matches may extend past the row into the rows below it.
    fn find_matches_in_row(
        &self,
        original: &str,
        row: &HaystackRow,
        search_text: &str,
        folded: Option<&FoldedText>,
        needle: &str,
    ) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        if needle.is_empty() {
            return found;
        }
        let (search_start, search_end) = match folded {
            Some(folded) => match (
                folded.offset_of_original(row.byte_start),
                folded.offset_of_original(row.byte_end),
            ) {
                (Some(start), Some(end)) => (start, end),
                _ => return found,
            },
            None => (row.byte_start, row.byte_end),
        };
        // The match may start in this row and continue below it, so the window
        // extends by up to a needle's length past the row boundary.
        let window_end = ceil_char_boundary(
            search_text,
            search_text
                .len()
                .min(search_end.saturating_add(needle.len().saturating_sub(1))),
        );
        let mut position = search_start;
        while position < search_end && position <= window_end {
            let Some(relative) = search_text[position..window_end].find(needle) else {
                break;
            };
            let start = position + relative;
            if start >= search_end {
                break;
            }
            let end = start + needle.len();
            let original_range = if let Some(folded) = folded {
                folded
                    .original_offset(start)
                    .zip(folded.original_offset(end))
            } else {
                Some((start, end))
            };
            let accepted = original_range.filter(|(original_start, original_end)| {
                self.accepts_match(original, *original_start, *original_end, row.byte_start)
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

    fn accepts_match(&self, text: &str, start: usize, end: usize, row_start: usize) -> bool {
        if start >= end {
            return false;
        }
        if start != row_start && !grapheme_join::is_boundary_at(text, start) {
            return false;
        }
        if !grapheme_join::is_boundary_at(text, end) {
            return false;
        }
        if self.whole_word_only {
            let before = if start == row_start {
                None
            } else {
                text[..start].chars().next_back()
            };
            let after = text[end..].chars().next();
            if !is_word_boundary(&before) || !is_word_boundary(&after) {
                return false;
            }
        }
        true
    }

    /// Search the rows of one logical line, materializing one bounded window
    /// per starting row instead of the whole continuation tail.
    pub(crate) fn search_rows(&self, ridx: usize, rows: &[&Row]) -> Vec<Selection> {
        let mut res = Vec::new();
        if self.needle.is_empty() || rows.is_empty() {
            return res;
        }
        // Empty rows contribute no searchable text. Index the content rows
        // once so a long blank continuation tail is never walked per starting
        // row.
        let content_rows: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.columns.is_empty())
            .map(|(index, _)| index)
            .collect();
        let mut next_content_row = 0;
        for (offset, row) in rows.iter().enumerate() {
            if row.columns.is_empty() {
                continue;
            }
            while next_content_row < content_rows.len() && content_rows[next_content_row] <= offset
            {
                next_content_row += 1;
            }
            let below = content_rows[next_content_row..]
                .iter()
                .copied()
                .map(move |index| (index - offset, rows[index]))
                .take_while(|(_, row)| !row.is_canonical);
            res.extend(self.search_starting_row(ridx + offset, row, below));
        }
        res
    }

    pub(crate) fn search_starting_row<'a>(
        &self,
        ridx: usize,
        row: &Row,
        below: impl Iterator<Item = (usize, &'a Row)>,
    ) -> Vec<Selection> {
        let mut res = Vec::new();
        if self.needle.is_empty() || row.columns.is_empty() {
            return res;
        }
        let folded_needle = self.case_insensitive.then(|| fold_case(&self.needle));
        let needle_len = folded_needle
            .as_ref()
            .map_or(self.needle.len(), |needle| needle.len());
        let haystack = Haystack::new(ridx, row, below, needle_len, self.case_insensitive);
        if haystack.text.is_empty() {
            return res;
        }
        let (search_text, needle) = match (&haystack.folded, &folded_needle) {
            (Some(folded), Some(needle)) => (folded.text.as_str(), needle.as_str()),
            _ => (haystack.text.as_str(), self.needle.as_str()),
        };
        for (start, end) in self.find_matches_in_row(
            &haystack.text,
            &haystack.row,
            search_text,
            haystack.folded.as_ref(),
            needle,
        ) {
            let (Some(first), Some(last)) = (
                haystack.cell_containing(start),
                haystack.cell_containing(end - 1),
            ) else {
                continue;
            };
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

    /// Search a row and its tail.
    /// The tail are all the non-canonical lines below `row`, with `row` not necessarily being canonical itself.
    pub(crate) fn search_row(&self, ridx: usize, row: &Row, tail: &[&Row]) -> Vec<Selection> {
        self.search_starting_row(
            ridx,
            row,
            tail.iter()
                .copied()
                .enumerate()
                .map(|(offset, row)| (offset + 1, row))
                .take_while(|(_, row)| !row.is_canonical),
        )
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
            if viewport.front().is_some() {
                let rows = viewport_line(viewport, 0);
                let selections = self.search_row(0, rows[0], &rows[1..]);
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
                let below = lines_below
                    .iter()
                    .take_while(|row| !row.is_canonical)
                    .enumerate()
                    .map(|(offset, row)| (offset + 1, row));
                let selections = self.search_starting_row(viewport.len() - 1, row, below);
                found_something = !selections.is_empty();
                self.selections.extend(selections);
            }
        }
        found_something
    }

    /// `move_down` for a viewport that jumped `amount` rows in one transfer.
    /// Existing hits shift by the whole distance; only lines still on screen
    /// are searched, at their final indexes.
    pub(crate) fn move_down_rows(
        &mut self,
        amount: usize,
        viewport: &VecDeque<Row>,
        grid_height: usize,
    ) {
        if amount == 0 {
            return;
        }
        self.selections
            .iter_mut()
            .chain(self.active.iter_mut())
            .for_each(|selection| selection.move_down(amount));
        self.adjust_selections_to_moved_viewport(grid_height);
        if self.needle.is_empty() || viewport.is_empty() {
            return;
        }
        let visible_new = amount.min(viewport.len());
        let mut new_selections = Vec::new();
        let mut index = 0;
        while index < visible_new {
            let rows = viewport_line(viewport, index);
            new_selections.extend(
                self.search_rows(index, &rows)
                    .into_iter()
                    .filter(|selection| (selection.start.line() as usize) < visible_new),
            );
            index += rows.len();
        }
        for selection in new_selections.into_iter().rev() {
            self.selections.insert(0, selection);
        }
    }

    /// `move_up` for a viewport that jumped `amount` rows in one transfer.
    pub(crate) fn move_up_rows(
        &mut self,
        amount: usize,
        viewport: &VecDeque<Row>,
        lines_below: &VecDeque<Row>,
        grid_height: usize,
    ) {
        if amount == 0 {
            return;
        }
        self.selections
            .iter_mut()
            .chain(self.active.iter_mut())
            .for_each(|selection| selection.move_up(amount));
        self.adjust_selections_to_moved_viewport(grid_height);
        if self.needle.is_empty() || viewport.is_empty() {
            return;
        }
        let start = viewport.len().saturating_sub(amount);
        let mut index = start;
        while index < viewport.len() {
            let below = viewport
                .iter()
                .skip(index + 1)
                .take_while(|row| !row.is_canonical)
                .chain(lines_below.iter().take_while(|row| !row.is_canonical))
                .enumerate()
                .map(|(offset, row)| (offset + 1, row));
            self.selections
                .extend(self.search_starting_row(index, &viewport[index], below));
            index += 1;
        }
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
        self.pending_search = None;
        self.search_scrollbuffer(SearchDirection::Down);
    }

    pub fn search_up(&mut self) {
        self.pending_search = None;
        self.search_scrollbuffer(SearchDirection::Up);
    }

    /// Continue a search that stopped at the row quantum. Returns true
    /// when more viewport movement is still queued.
    pub(crate) fn continue_pending_search(&mut self) -> bool {
        let Some(pending) = self.pending_search.clone() else {
            return false;
        };
        self.search_scrollbuffer(pending.dir);
        if self.pending_search.is_none() && self.search_results.selections.is_empty() {
            if let Some(fallback) = pending.fallback {
                self.search_scrollbuffer(fallback);
            }
        }
        self.pending_search.is_some()
    }

    pub fn clear_search(&mut self) {
        self.pending_search = None;
        // Clearing all previous highlights
        for res in &self.search_results.selections {
            self.output_buffer
                .update_lines(res.start.line() as usize, res.end.line() as usize);
        }
        self.search_results = Default::default();
    }

    pub fn set_search_string(&mut self, needle: &str) {
        self.pending_search = None;
        self.search_results.needle = needle.to_string();
        self.search_viewport();
        // If the current viewport does not contain any hits,
        // we jump around until we find something. Starting
        // going backwards.
        if self.search_results.selections.is_empty() {
            self.search_scrollbuffer(SearchDirection::Up);
            if self.search_results.selections.is_empty() {
                match self.pending_search.as_mut() {
                    Some(pending) => pending.fallback = Some(SearchDirection::Down),
                    None => self.search_scrollbuffer(SearchDirection::Down),
                }
            }
        }
        // We still don't want to pre-select anything at this stage
        self.search_results.active = None;
        self.is_scrolled = true;
    }

    pub fn search_viewport(&mut self) {
        let mut index = 0;
        while index < self.viewport.len() {
            let rows = viewport_line(&self.viewport, index);
            let line_len = rows.len();
            let selections = self.search_results.search_rows(index, &rows);
            for sel in &selections {
                // Cast works because we can' be negative here
                self.output_buffer
                    .update_lines(sel.start.line() as usize, sel.end.line() as usize);
            }
            self.search_results.selections.extend(selections);
            index += line_len;
        }
    }

    pub fn toggle_search_case_sensitivity(&mut self) {
        self.pending_search = None;
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
        self.pending_search = None;
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
        if let Some(pending) = self.pending_search.clone() {
            match pending.phase {
                SearchPhase::Move | SearchPhase::Undo => {
                    match self.search_viewport_move(pending.dir) {
                        None => return,
                        Some(found_something) => {
                            if !found_something && self.search_results.wrap_search {
                                self.search_viewport_wrap(pending.dir);
                            }
                            return;
                        },
                    }
                },
                SearchPhase::GoToEnd | SearchPhase::WrapBack => {
                    self.search_viewport_wrap(pending.dir);
                    return;
                },
            }
        }
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
            // Need to move the viewport. `None` means this turn hit the
            // row quantum; the search stays pending and must not wrap yet.
            match self.search_viewport_move(dir) {
                None => {},
                Some(found_something) => {
                    if !found_something && self.search_results.wrap_search {
                        self.search_viewport_wrap(dir);
                    }
                },
            }
        }
    }

    fn search_viewport_again(
        &mut self,
        search_viewport_for_the_first_time: bool,
        dir: SearchDirection,
    ) {
        // Caller only invokes when selections is non-empty.
        let Some(new_active) = (match dir {
            SearchDirection::Up => self.search_results.selections.last().cloned(),
            SearchDirection::Down => self.search_results.selections.first().cloned(),
        }) else {
            return;
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

    pub(super) fn search_viewport_move(&mut self, dir: SearchDirection) -> Option<bool> {
        // We need to move the viewport. `None` means the row quantum was
        // spent before the scan finished.
        if let Some(pending) = self.pending_search.clone() {
            if pending.dir == dir && matches!(pending.phase, SearchPhase::Undo) {
                return self.undo_search_move(dir, pending.rows, pending.origin, pending.fallback);
            }
        }
        let resume = self
            .pending_search
            .clone()
            .filter(|pending| pending.dir == dir && matches!(pending.phase, SearchPhase::Move));
        let mut rows = resume.as_ref().map(|pending| pending.rows).unwrap_or(0);
        let fallback = resume.as_ref().and_then(|pending| pending.fallback);
        let mut found_something = false;

        // We might loose the current selection, if we can't find anything
        let current_active_selection = resume
            .as_ref()
            .map(|pending| pending.origin)
            .unwrap_or(self.search_results.active);
        let mut stepped = 0isize;
        while !found_something && !self.search_reached_opposite_end(dir) {
            if stepped >= crate::panes::grid::SCREEN_ROW_QUANTUM as isize {
                self.pending_search = Some(PendingSearch {
                    dir,
                    phase: SearchPhase::Move,
                    rows,
                    origin: current_active_selection,
                    fallback,
                });
                return None;
            }
            let scroll_result = match dir {
                SearchDirection::Up => self.scroll_up_one_line_with_progress(),
                SearchDirection::Down => self.scroll_down_one_line_with_progress(),
            };
            if !scroll_result.moved {
                break;
            }
            rows += 1;
            stepped += 1;
            found_something = scroll_result.found_something;
        }
        if found_something {
            self.pending_search = None;
            self.search_adjust_to_new_selection(dir);
            return Some(true);
        }
        self.undo_search_move(dir, rows, current_active_selection, fallback)
    }

    fn undo_search_move(
        &mut self,
        dir: SearchDirection,
        rows_left: isize,
        origin: Option<Selection>,
        fallback: Option<SearchDirection>,
    ) -> Option<bool> {
        // Scroll back in bulk quanta. A scan that crossed the whole
        // history must not undo it one line at a time in this turn.
        if rows_left > 0 {
            let step = (rows_left as usize).min(crate::panes::grid::SCREEN_ROW_QUANTUM);
            let moved = match dir {
                SearchDirection::Up => self.scroll_down_rows(step),
                SearchDirection::Down => self.scroll_up_rows(step),
            };
            let remaining = rows_left.saturating_sub(moved as isize);
            if remaining > 0 && moved > 0 {
                self.pending_search = Some(PendingSearch {
                    dir,
                    phase: SearchPhase::Undo,
                    rows: remaining,
                    origin,
                    fallback,
                });
                return None;
            }
        }
        self.pending_search = None;
        self.search_results.active = origin;
        Some(false)
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
                            if !self.scroll_down_one_line_with_progress().moved {
                                break;
                            }
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
        let current_active_selection = self
            .pending_search
            .as_ref()
            .filter(|pending| {
                pending.dir == dir
                    && matches!(pending.phase, SearchPhase::GoToEnd | SearchPhase::WrapBack)
            })
            .map(|pending| pending.origin)
            .unwrap_or(self.search_results.active);
        let resuming_walk = self.pending_search.as_ref().is_some_and(|pending| {
            pending.dir == dir && matches!(pending.phase, SearchPhase::WrapBack)
        });
        let fallback = self
            .pending_search
            .as_ref()
            .and_then(|pending| pending.fallback);
        // UP
        // Go to the opposite end (bottom when searching up and top when searching down)
        let mut rows = if resuming_walk {
            self.pending_search
                .as_ref()
                .map(|pending| pending.rows)
                .unwrap_or(0)
        } else {
            match self.move_viewport_to_opposite_end(dir, current_active_selection) {
                Some(rows) => rows,
                None => return,
            }
        };

        // We are at the bottom or top. Maybe we found already something there
        // If not, scroll back again, until we find something
        let mut found_something = match dir {
            SearchDirection::Up => self.search_results.selections.last().is_some(),
            SearchDirection::Down => self.search_results.selections.first().is_some(),
        };

        // We didn't find anything at the opposing end of the scrollbuffer, so we scroll back until we find something
        if !found_something {
            let mut stepped = 0isize;
            while rows > 0 && !found_something {
                if stepped >= crate::panes::grid::SCREEN_ROW_QUANTUM as isize {
                    self.pending_search = Some(PendingSearch {
                        dir,
                        phase: SearchPhase::WrapBack,
                        rows,
                        origin: current_active_selection,
                        fallback,
                    });
                    return;
                }
                rows -= 1;
                stepped += 1;
                let scroll_result = match dir {
                    SearchDirection::Up => self.scroll_up_one_line_with_progress(),
                    SearchDirection::Down => self.scroll_down_one_line_with_progress(),
                };
                if !scroll_result.moved {
                    break;
                }
                found_something = scroll_result.found_something;
            }
        }
        self.pending_search = None;
        if found_something {
            self.search_results.active = match dir {
                SearchDirection::Up => self.search_results.selections.last().cloned(),
                SearchDirection::Down => {
                    // We need to scroll until the found item is at the top
                    if let Some(first) = self.search_results.selections.first() {
                        for _ in 0..first.start.line() {
                            if !self.scroll_down_one_line_with_progress().moved {
                                break;
                            }
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

    fn move_viewport_to_opposite_end(
        &mut self,
        dir: SearchDirection,
        origin: Option<Selection>,
    ) -> Option<isize> {
        let mut rows = self
            .pending_search
            .as_ref()
            .filter(|pending| pending.dir == dir && matches!(pending.phase, SearchPhase::GoToEnd))
            .map(|pending| pending.rows)
            .unwrap_or(0);
        let fallback = self
            .pending_search
            .as_ref()
            .and_then(|pending| pending.fallback);
        let mut stepped = 0usize;
        loop {
            let done = match dir {
                SearchDirection::Up => self.lines_below.is_empty(),
                SearchDirection::Down => self.lines_above.is_empty(),
            };
            if done {
                self.pending_search = None;
                return Some(rows);
            }
            if stepped >= crate::panes::grid::SCREEN_ROW_QUANTUM {
                self.pending_search = Some(PendingSearch {
                    dir,
                    phase: SearchPhase::GoToEnd,
                    rows,
                    origin,
                    fallback,
                });
                return None;
            }
            let budget = crate::panes::grid::SCREEN_ROW_QUANTUM - stepped;
            let moved = match dir {
                SearchDirection::Up => self.scroll_down_rows(budget),
                SearchDirection::Down => self.scroll_up_rows(budget),
            };
            if moved == 0 {
                self.pending_search = None;
                return Some(rows);
            }
            rows += moved as isize;
            stepped = stepped.saturating_add(moved);
        }
    }
}
