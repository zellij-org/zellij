use crate::panes::grid::Row;
use crate::panes::link_handler::LinkHandler;
use crate::panes::terminal_character::{LinkAnchor, RcCharacterStyles};
use std::collections::VecDeque;

const URL_SCHEMES: [&str; 4] = ["http://", "https://", "ftp://", "mailto:"];
/// Automatic URLs longer than this are not linked. Scans keep at most this
/// many cells, plus the characters added since the previous scan.
const MAX_URL_CHARS: usize = 2048;
const COLON_FREE_LOOKBEHIND: usize = 16;

#[derive(Clone, Copy)]
struct Cell {
    character: char,
    y: isize,
    index: usize,
}

fn row_ref<'a>(
    lines_above: &'a VecDeque<Row>,
    viewport: &'a VecDeque<Row>,
    y: isize,
) -> Option<&'a Row> {
    if y < 0 {
        let index = lines_above.len() as isize + y;
        return usize::try_from(index)
            .ok()
            .and_then(|index| lines_above.get(index));
    }
    viewport.get(y as usize)
}

fn row_mut<'a>(
    lines_above: &'a mut VecDeque<Row>,
    viewport: &'a mut VecDeque<Row>,
    y: isize,
) -> Option<&'a mut Row> {
    if y < 0 {
        let index = lines_above.len() as isize + y;
        return usize::try_from(index)
            .ok()
            .and_then(|index| lines_above.get_mut(index));
    }
    viewport.get_mut(y as usize)
}

#[cfg(test)]
thread_local! {
    static HYPERLINK_CELLS_MATERIALIZED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static HYPERLINK_CELLS_VISITED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static HYPERLINK_ROW_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn note_cells_materialized(count: usize) {
    #[cfg(test)]
    HYPERLINK_CELLS_MATERIALIZED.with(|counter| counter.set(counter.get() + count));
    let _ = count;
}

fn note_cells_visited(count: usize) {
    #[cfg(test)]
    HYPERLINK_CELLS_VISITED.with(|counter| counter.set(counter.get() + count));
    let _ = count;
}

fn note_row_visited() {
    #[cfg(test)]
    HYPERLINK_ROW_VISITS.with(|counter| counter.set(counter.get() + 1));
}

#[cfg(test)]
pub(crate) fn hyperlink_cells_materialized() -> usize {
    HYPERLINK_CELLS_MATERIALIZED.with(|counter| counter.get())
}

#[cfg(test)]
pub(crate) fn hyperlink_cells_visited() -> usize {
    HYPERLINK_CELLS_VISITED.with(|counter| counter.get())
}

#[cfg(test)]
pub(crate) fn hyperlink_row_visits() -> usize {
    HYPERLINK_ROW_VISITS.with(|counter| counter.get())
}

#[cfg(test)]
pub(crate) fn reset_hyperlink_scan_counters() {
    HYPERLINK_CELLS_MATERIALIZED.with(|counter| counter.set(0));
    HYPERLINK_CELLS_VISITED.with(|counter| counter.set(0));
    HYPERLINK_ROW_VISITS.with(|counter| counter.set(0));
}

#[cfg(test)]
fn line_start(lines_above: &VecDeque<Row>, viewport: &VecDeque<Row>, y: isize) -> isize {
    let mut start = y;
    let top = -(lines_above.len() as isize);
    while start > top {
        match row_ref(lines_above, viewport, start) {
            Some(row) if !row.is_canonical => start -= 1,
            _ => break,
        }
    }
    start
}

fn line_rows(lines_above: &VecDeque<Row>, viewport: &VecDeque<Row>, start: isize) -> isize {
    let mut end = start + 1;
    while let Some(row) = row_ref(lines_above, viewport, end) {
        note_row_visited();
        if row.is_canonical {
            break;
        }
        end += 1;
    }
    end
}

pub fn is_url_terminator(character: char) -> bool {
    matches!(
        character,
        ' ' | '\n' | '\r' | '\t' | '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '⏎'
    )
}

pub fn trim_trailing_punctuation(url: &str) -> &str {
    url.trim_end_matches(['.', ',', ';', '!', '?', '\n', '\r', '⏎'])
}

pub fn is_valid_url(url: &str) -> bool {
    if url.len() < 8 {
        return false;
    }
    if let Some(after_protocol) = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
    {
        return !after_protocol.is_empty() && after_protocol.contains('.');
    }
    if let Some(after_protocol) = url.strip_prefix("ftp://") {
        return !after_protocol.is_empty();
    }
    if let Some(after_colon) = url.strip_prefix("mailto:") {
        return after_colon.contains('@');
    }
    false
}

fn scheme_at(cells: &[Cell], at: usize) -> bool {
    URL_SCHEMES.iter().any(|scheme| {
        scheme.len() <= cells.len() - at
            && scheme
                .chars()
                .zip(&cells[at..])
                .all(|(expected, cell)| cell.character == expected)
    })
}

pub fn link_urls_in_lines(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    lines: &[isize],
    cursor: Option<(usize, usize)>,
    link_handler: &mut LinkHandler,
) {
    // Rows retain their own scan watermarks. A dirty suffix therefore
    // resumes at the first row with unscanned text instead of walking the
    // logical line from its start. Rows sorted by position let one scan
    // cover every later dirty row of the same line.
    let mut dirty: Vec<isize> = lines
        .iter()
        .copied()
        .filter(|y| *y < viewport.len() as isize && row_ref(lines_above, viewport, *y).is_some())
        .collect();
    dirty.sort_unstable();
    dirty.dedup();
    if dirty.iter().all(|&y| {
        row_ref(lines_above, viewport, y).is_some_and(|row| {
            !row.link_scan_pending
                && !row.link_scan_dirty
                && row.link_prefix_scanned as usize == row.columns.len()
        })
    }) {
        return;
    }
    let mut covered_until = isize::MIN;
    for y in dirty {
        if y < covered_until {
            continue;
        }
        let Some((resume_y, resume_scanned, prior_colon)) =
            resume_position(lines_above, viewport, y)
        else {
            continue;
        };
        let end = line_rows(lines_above, viewport, resume_y);
        covered_until = end;
        scan_suffix(
            lines_above,
            viewport,
            resume_y,
            resume_scanned,
            prior_colon,
            end,
            cursor,
            link_handler,
        );
    }
}

fn row_needs_scan(row: &Row) -> bool {
    row.link_scan_dirty
        || row.link_scan_pending
        || (row.link_prefix_scanned as usize) != row.columns.len()
}

/// The first row of `y`'s logical line that still has unscanned text, the
/// number of cells already accepted there, and whether a colon precedes it.
///
/// Only rows that actually have unscanned text are walked. A render-paced
/// suffix append normally finds its resume row in one step; overwrite,
/// reflow, and eviction invalidate watermarks, so the walk extends exactly
/// as far as the invalidated prefix.
fn resume_position(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    y: isize,
) -> Option<(isize, usize, bool)> {
    let mut resume_y = y;
    loop {
        note_row_visited();
        let current = row_ref(lines_above, viewport, resume_y)?;
        if current.is_canonical || resume_y == -(lines_above.len() as isize) {
            break;
        }
        match row_ref(lines_above, viewport, resume_y - 1) {
            Some(previous) if row_needs_scan(previous) => resume_y -= 1,
            _ => break,
        }
    }
    let row = row_ref(lines_above, viewport, resume_y)?;
    let len = row.columns.len();
    let scanned =
        if row.link_scan_dirty || row.link_scan_pending || row.link_prefix_scanned as usize > len {
            0
        } else {
            row.link_prefix_scanned as usize
        };
    let prior_colon = if scanned > 0 {
        row.link_prefix_has_colon
    } else if row.is_canonical {
        false
    } else {
        row_ref(lines_above, viewport, resume_y - 1)
            .map(|previous| previous.link_prefix_has_colon)
            .unwrap_or(false)
    };
    Some((resume_y, scanned, prior_colon))
}

/// Cells of the logical line starting at `resume_y`, beginning `start`
/// cells before (negative) or after (non-negative) the resume position.
fn collect_window(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    resume_y: isize,
    resume_scanned: usize,
    start: isize,
    max: usize,
    end: isize,
) -> (Vec<Cell>, isize) {
    let mut cells = Vec::new();
    let mut forward_from = resume_y;
    let mut forward_scanned = resume_scanned;
    let actual_start;
    if start < 0 {
        let mut needed = start.unsigned_abs().min(MAX_URL_CHARS);
        let mut prefix = Vec::new();
        let mut y = resume_y;
        let mut index = resume_scanned;
        'rows: while needed > 0 {
            note_row_visited();
            let Some(row) = row_ref(lines_above, viewport, y) else {
                break;
            };
            while index > 0 && needed > 0 {
                index -= 1;
                prefix.push(Cell {
                    character: row.columns[index].character,
                    y,
                    index,
                });
                needed -= 1;
            }
            if row.is_canonical {
                break;
            }
            y -= 1;
            let Some(previous) = row_ref(lines_above, viewport, y) else {
                break 'rows;
            };
            index = previous.columns.len();
        }
        prefix.reverse();
        cells = prefix;
        actual_start = -(cells.len() as isize);
        forward_from = resume_y;
        forward_scanned = resume_scanned;
    } else {
        actual_start = start;
        let mut skip = start as usize;
        while skip > 0 {
            note_row_visited();
            let Some(row) = row_ref(lines_above, viewport, forward_from) else {
                return (cells, actual_start);
            };
            let available = row.columns.len().saturating_sub(forward_scanned);
            if skip < available {
                forward_scanned += skip;
                skip = 0;
            } else {
                skip -= available;
                forward_from += 1;
                forward_scanned = 0;
            }
        }
    }
    for y in forward_from..end {
        note_row_visited();
        let Some(row) = row_ref(lines_above, viewport, y) else {
            continue;
        };
        let skip = if y == forward_from {
            forward_scanned
        } else {
            0
        };
        for (index, cell) in row.columns[skip..].iter().enumerate() {
            if cells.len() >= max {
                return (cells, actual_start);
            }
            cells.push(Cell {
                character: cell.character,
                y,
                index: index + skip,
            });
        }
    }
    (cells, actual_start)
}

fn clear_scan_state(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    start: isize,
    end: isize,
) {
    for y in start..end {
        if let Some(row) = row_mut(lines_above, viewport, y) {
            row.link_scan_pending = false;
            if row.link_scan_dirty || row.link_prefix_scanned as usize > row.columns.len() {
                row.link_prefix_scanned = 0;
                row.link_prefix_has_colon = false;
                row.link_scan_dirty = false;
            }
        }
    }
}

fn scan_suffix(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    resume_y: isize,
    resume_scanned: usize,
    prior_colon: bool,
    end: isize,
    cursor: Option<(usize, usize)>,
    link_handler: &mut LinkHandler,
) {
    clear_scan_state(lines_above, viewport, resume_y, end);
    let resume_scanned = row_ref(lines_above, viewport, resume_y)
        .map(|row| resume_scanned.min(row.columns.len()))
        .unwrap_or(resume_scanned);
    let mut total_cells = 0usize;
    for y in resume_y..end {
        note_row_visited();
        let Some(row) = row_ref(lines_above, viewport, y) else {
            continue;
        };
        let skip = if y == resume_y { resume_scanned } else { 0 };
        total_cells += row.columns.len().saturating_sub(skip);
    }
    if total_cells == 0 {
        return;
    }
    if !prior_colon {
        let mut suffix_has_colon = false;
        let mut suffix_has_anchor = false;
        for y in resume_y..end {
            note_row_visited();
            let Some(row) = row_ref(lines_above, viewport, y) else {
                continue;
            };
            let skip = if y == resume_y { resume_scanned } else { 0 };
            for cell in &row.columns[skip..] {
                note_cells_visited(1);
                if cell.character == ':' {
                    suffix_has_colon = true;
                }
                if link_handler.is_detected(cell.styles.link_anchor) {
                    suffix_has_anchor = true;
                }
            }
        }
        if !suffix_has_colon && !suffix_has_anchor {
            for y in resume_y..end {
                if let Some(row) = row_mut(lines_above, viewport, y) {
                    row.link_prefix_scanned = row.columns.len().min(u32::MAX as usize) as u32;
                    row.link_prefix_has_colon = false;
                    row.link_scan_dirty = false;
                    row.link_scan_pending = false;
                }
            }
            return;
        }
    }
    let lookback = if prior_colon {
        MAX_URL_CHARS
    } else {
        COLON_FREE_LOOKBEHIND
    };
    let mut covered_until = 0usize;
    let mut deferred_line = false;
    while covered_until < total_cells {
        let window_end = (covered_until + MAX_URL_CHARS).min(total_cells);
        let requested_start =
            covered_until as isize - lookback.min(MAX_URL_CHARS.saturating_sub(1)) as isize;
        let want = (window_end as isize - requested_start) as usize;
        let (cells, actual_start) = collect_window(
            lines_above,
            viewport,
            resume_y,
            resume_scanned,
            requested_start,
            want,
            end,
        );
        if cells.is_empty() {
            break;
        }
        let examined_end = actual_start + cells.len() as isize;
        if examined_end <= covered_until as isize {
            break;
        }
        note_cells_materialized(cells.len());
        let cursor_cell = cursor.and_then(|(cursor_x, cursor_y)| {
            let cursor_y = cursor_y as isize;
            cells
                .iter()
                .position(|cell| cell.y == cursor_y && cell.index >= cursor_x)
                .or_else(|| (resume_y..end).contains(&cursor_y).then_some(cells.len()))
        });
        let deferred = link_urls_in_line(lines_above, viewport, &cells, cursor_cell, link_handler);
        if deferred {
            deferred_line = true;
            let pending_y = cursor_cell
                .and_then(|position| cells.get(position))
                .map(|cell| cell.y)
                .or_else(|| {
                    cursor
                        .map(|(_, cursor_y)| cursor_y as isize)
                        .filter(|y| *y >= resume_y && *y < end)
                })
                .unwrap_or(end - 1);
            if let Some(row) = row_mut(lines_above, viewport, pending_y) {
                row.link_scan_pending = true;
            }
            break;
        }
        covered_until = examined_end as usize;
    }
    if deferred_line {
        return;
    }
    let mut offset = 0usize;
    let mut aggregate_colon = prior_colon;
    for y in resume_y..end {
        note_row_visited();
        let Some(row) = row_mut(lines_above, viewport, y) else {
            continue;
        };
        let len = row.columns.len();
        let base = if y == resume_y {
            resume_scanned.min(len)
        } else {
            0
        };
        let remaining = len.saturating_sub(base);
        let covered = covered_until.saturating_sub(offset).min(remaining);
        let new_prefix = base + covered;
        let previous = (row.link_prefix_scanned as usize).min(len);
        if previous > 0 {
            aggregate_colon |= row.link_prefix_has_colon;
        }
        if new_prefix > previous {
            if row.columns[previous..new_prefix]
                .iter()
                .any(|cell| cell.character == ':')
            {
                aggregate_colon = true;
            }
            row.link_prefix_scanned = new_prefix.min(u32::MAX as usize) as u32;
        }
        row.link_prefix_has_colon = aggregate_colon;
        row.link_scan_dirty = false;
        row.link_scan_pending = false;
        offset += remaining;
        if covered < remaining {
            break;
        }
    }
}

fn link_urls_in_line(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    cursor_cell: Option<usize>,
    link_handler: &mut LinkHandler,
) -> bool {
    let mut deferred = false;
    // Inclusive cell ranges `(start, end)` of the URL-shaped text that was
    // handled, in order.
    let mut handled: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < cells.len() {
        if !matches!(cells[at].character, 'h' | 'f' | 'm') || !scheme_at(cells, at) {
            at += 1;
            continue;
        }
        let end = cells[at..]
            .iter()
            .position(|cell| is_url_terminator(cell.character))
            .map_or(cells.len(), |offset| at + offset);
        if cursor_cell.is_some_and(|cursor| (at..=end).contains(&cursor)) {
            deferred = true;
            handled.push((at, end));
            at = end + 1;
            continue;
        }
        let text: String = cells[at..end].iter().map(|cell| cell.character).collect();
        let url = trim_trailing_punctuation(&text);
        let kept = url.chars().count();
        if is_valid_url(url) {
            handled.push((at, end));
            // Explicit OSC 8 ownership wins over automatic detection. An app
            // may show URL-looking text whose target differs, so skip any
            // candidate that overlaps a non-detected span.
            if overlaps_explicit_link(lines_above, viewport, &cells[at..end], link_handler) {
                at = end + 1;
                continue;
            }
            let previous = start_anchor_at(lines_above, viewport, cells[at]);
            if !linked_exactly(lines_above, viewport, &cells[at..], kept, previous) {
                if let Some(anchor) = link_handler.new_link_from_url(url.to_owned()) {
                    mark_link(lines_above, viewport, &cells[at..at + kept], anchor);
                    clear_stale_link(lines_above, viewport, &cells[at + kept..], previous);
                    mark_end(lines_above, viewport, cells, at + kept);
                } else {
                    clear_stale_link(lines_above, viewport, &cells[at..], previous);
                }
            }
        }
        at = end + 1;
    }
    clear_orphaned_detected_links(lines_above, viewport, cells, &handled, link_handler);
    deferred
}

fn overlaps_explicit_link(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cells: &[Cell],
    link_handler: &LinkHandler,
) -> bool {
    cells.iter().any(|cell| {
        let anchor = row_ref(lines_above, viewport, cell.y)
            .and_then(|row| row.columns.get(cell.index))
            .and_then(|character| character.styles.link_anchor);
        matches!(anchor, Some(LinkAnchor::Start(_))) && !link_handler.is_detected(anchor)
    })
}

/// Drop detected links that no longer sit on URL text, for example because
/// the first characters of the URL were overwritten so that no URL starts
/// there any more. The anchored run is cleared up to and including its end
/// marker. Text in `handled` ranges is left to the URL handling above.
fn clear_orphaned_detected_links(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    handled: &[(usize, usize)],
    link_handler: &LinkHandler,
) {
    let mut orphaned = Vec::new();
    let mut range = 0;
    let mut after_orphan = false;
    for (position, cell) in cells.iter().enumerate() {
        while range < handled.len() && handled[range].1 < position {
            range += 1;
        }
        let is_handled = handled
            .get(range)
            .is_some_and(|(first, last)| (*first..=*last).contains(&position));
        let anchor = row_ref(lines_above, viewport, cell.y)
            .and_then(|row| row.columns.get(cell.index))
            .and_then(|character| character.styles.link_anchor);
        if !is_handled {
            if link_handler.is_detected(anchor) {
                orphaned.push(position);
                after_orphan = true;
                continue;
            }
            if anchor == Some(LinkAnchor::End) && after_orphan {
                orphaned.push(position);
            }
        }
        after_orphan = false;
    }
    for position in orphaned {
        let cell = cells[position];
        if let Some(character) =
            row_mut(lines_above, viewport, cell.y).and_then(|row| row.columns.get_mut(cell.index))
        {
            character.styles.update(|styles| styles.link_anchor = None);
        }
    }
}

fn start_anchor_at(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cell: Cell,
) -> Option<LinkAnchor> {
    row_ref(lines_above, viewport, cell.y)
        .and_then(|row| row.columns.get(cell.index))
        .and_then(|character| character.styles.link_anchor)
        .filter(|anchor| matches!(anchor, LinkAnchor::Start(_)))
}

fn linked_exactly(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cells: &[Cell],
    kept: usize,
    previous: Option<LinkAnchor>,
) -> bool {
    let Some(previous) = previous else {
        return false;
    };
    let anchor_of = |cell: &Cell| {
        row_ref(lines_above, viewport, cell.y)
            .and_then(|row| row.columns.get(cell.index))
            .and_then(|character| character.styles.link_anchor)
    };
    cells[..kept]
        .iter()
        .all(|cell| anchor_of(cell) == Some(previous))
        && cells
            .get(kept)
            .map_or(true, |cell| anchor_of(cell) != Some(previous))
        && stale_tail(lines_above, viewport, &cells[kept..], previous).is_empty()
}

/// Positions in `cells` (the cells after a URL) that still belong to the
/// `previous` link, plus its end marker when any do.
///
/// Overwriting a cell inside a linked URL replaces that cell's styles, so
/// the old anchor can have a hole in the middle and the cells behind the
/// hole keep pointing at the old target. The scan therefore does not stop at
/// the first cell that no longer carries `previous`: it continues to the old
/// link's end marker, or to a different link starting directly after the
/// last cell of `previous`.
fn stale_tail(
    lines_above: &VecDeque<Row>,
    viewport: &VecDeque<Row>,
    cells: &[Cell],
    previous: LinkAnchor,
) -> Vec<usize> {
    let mut stale = Vec::new();
    let mut after_previous = false;
    for (position, cell) in cells.iter().enumerate() {
        let Some(character) =
            row_ref(lines_above, viewport, cell.y).and_then(|row| row.columns.get(cell.index))
        else {
            break;
        };
        let anchor = character.styles.link_anchor;
        if anchor == Some(previous) {
            stale.push(position);
            after_previous = true;
            continue;
        }
        match anchor {
            Some(LinkAnchor::End) => {
                if !stale.is_empty() {
                    stale.push(position);
                }
                break;
            },
            Some(LinkAnchor::Start(_)) if after_previous => break,
            _ => {},
        }
        after_previous = false;
    }
    stale
}

/// Drop what is left of the `previous` link in `cells` after its URL changed.
fn clear_stale_link(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    previous: Option<LinkAnchor>,
) {
    let Some(previous) = previous else {
        return;
    };
    for position in stale_tail(lines_above, viewport, cells, previous) {
        let cell = cells[position];
        if let Some(character) =
            row_mut(lines_above, viewport, cell.y).and_then(|row| row.columns.get_mut(cell.index))
        {
            character.styles.update(|styles| styles.link_anchor = None);
        }
    }
}

fn mark_link(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    anchor: LinkAnchor,
) {
    let mut last_update: Option<(RcCharacterStyles, RcCharacterStyles)> = None;
    for cell in cells {
        let Some(character) =
            row_mut(lines_above, viewport, cell.y).and_then(|row| row.columns.get_mut(cell.index))
        else {
            continue;
        };
        match &last_update {
            Some((before, after)) if character.styles.ptr_eq(before) => {
                character.styles = after.clone();
            },
            _ => {
                let before = character.styles.clone();
                character
                    .styles
                    .update(|styles| styles.link_anchor = Some(anchor));
                last_update = Some((before, character.styles.clone()));
            },
        }
    }
}

fn mark_end(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut VecDeque<Row>,
    cells: &[Cell],
    after: usize,
) {
    let (y, index) = match cells.get(after) {
        Some(cell) => (cell.y, cell.index),
        None => match cells.last() {
            Some(last) => (last.y, last.index + 1),
            None => return,
        },
    };
    let Some(row) = row_mut(lines_above, viewport, y) else {
        return;
    };
    if index >= row.columns.len() {
        row.expand_trailing_blanks();
    }
    if let Some(character) = row.columns.get_mut(index) {
        if character.styles.link_anchor.is_none() {
            character
                .styles
                .update(|styles| styles.link_anchor = Some(LinkAnchor::End));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER};

    fn row_with(text: &str, width: usize, canonical: bool) -> Row {
        let mut columns: Vec<TerminalCharacter> =
            text.chars().map(TerminalCharacter::new).collect();
        columns.resize(width.max(columns.len()), TerminalCharacter::new(' '));
        let row = Row::from_columns(columns);
        if canonical {
            row.canonical()
        } else {
            row
        }
    }

    fn screen(lines: &[(&str, bool)], width: usize) -> VecDeque<Row> {
        lines
            .iter()
            .map(|(text, canonical)| row_with(text, width, *canonical))
            .collect()
    }

    fn anchors(row: &Row) -> Vec<Option<LinkAnchor>> {
        row.columns
            .iter()
            .map(|character| character.styles.link_anchor)
            .collect()
    }

    fn is_start(anchor: &Option<LinkAnchor>) -> bool {
        matches!(anchor, Some(LinkAnchor::Start(_)))
    }

    fn link_all(
        lines_above: &mut VecDeque<Row>,
        viewport: &mut VecDeque<Row>,
        cursor: Option<(usize, usize)>,
        link_handler: &mut LinkHandler,
    ) {
        let changed: Vec<isize> = (0..viewport.len() as isize).collect();
        link_urls_in_lines(lines_above, viewport, &changed, cursor, link_handler);
    }

    fn uris(link_handler: &LinkHandler) -> Vec<String> {
        let mut uris: Vec<String> = link_handler
            .links()
            .values()
            .map(|link| link.uri.clone())
            .collect();
        uris.sort();
        uris
    }

    #[test]
    fn every_supported_scheme_is_linked() {
        for url in [
            "http://example.com",
            "https://example.com/path?q=1",
            "ftp://files.example.com",
            "mailto:user@example.com",
        ] {
            let mut viewport = screen(&[(&format!("see {} now", url), true), ("", true)], 60);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert_eq!(uris(&handler), vec![url.to_owned()], "{}", url);
            let marked = anchors(&viewport[0]);
            assert!(marked[4..4 + url.len()].iter().all(is_start), "{}", url);
            assert_eq!(marked[4 + url.len()], Some(LinkAnchor::End), "{}", url);
            assert!(marked[..4].iter().all(|a| a.is_none()), "{}", url);
        }
    }

    #[test]
    fn invalid_urls_are_not_linked() {
        for text in ["http:// x", "http://nodot y", "mailto:nobody z", "ftp:// q"] {
            let mut viewport = screen(&[(text, true)], 40);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert!(uris(&handler).is_empty(), "{}", text);
        }
    }

    #[test]
    fn trailing_punctuation_is_left_out_of_the_link() {
        let mut viewport = screen(&[("go to https://example.com/a.", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/a".to_owned()]);
        let marked = anchors(&viewport[0]);
        assert!(is_start(&marked[26]));
        assert_eq!(marked[27], Some(LinkAnchor::End));
    }

    #[test]
    fn each_terminator_ends_a_url() {
        for terminator in [
            ' ', '\t', '"', '\'', '<', '>', '(', ')', '[', ']', '{', '}', '⏎',
        ] {
            let text = format!("http://example.com{}rest", terminator);
            let mut viewport = screen(&[(&text, true)], 40);
            let mut handler = LinkHandler::new();
            link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
            assert_eq!(
                uris(&handler),
                vec!["http://example.com".to_owned()],
                "{:?}",
                terminator
            );
        }
    }

    #[test]
    fn a_url_wrapped_over_two_rows_is_one_link() {
        let mut viewport = screen(
            &[
                ("https://example.com/", true),
                ("path x", false),
                ("", true),
            ],
            20,
        );
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/path".to_owned()]);
        assert!(anchors(&viewport[0]).iter().all(is_start));
        let second = anchors(&viewport[1]);
        assert!(second[..4].iter().all(is_start));
        assert_eq!(second[4], Some(LinkAnchor::End));
        assert!(second[5..].iter().all(|a| a.is_none()));
    }

    #[test]
    fn a_url_ending_a_line_does_not_continue_onto_the_next_line() {
        let url = "https://www.fastmail.com";
        let mut viewport = screen(&[(url, true), ("2", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(uris(&handler), vec![url.to_owned()]);
        assert!(anchors(&viewport[1]).iter().all(|a| a.is_none()));
    }

    #[test]
    fn a_url_that_starts_in_scrollback_and_wraps_into_the_screen_is_one_link() {
        let mut lines_above = screen(&[("https://example.", true)], 16);
        let mut viewport = screen(&[("com/rest done", false), ("", true)], 16);
        let mut handler = LinkHandler::new();
        link_urls_in_lines(&mut lines_above, &mut viewport, &[0], None, &mut handler);
        assert_eq!(uris(&handler), vec!["https://example.com/rest".to_owned()]);
        assert!(anchors(&lines_above[0]).iter().all(is_start));
        assert!(anchors(&viewport[0])[..8].iter().all(is_start));
    }

    #[test]
    fn a_url_the_cursor_may_still_extend_is_left_for_later() {
        let mut viewport = screen(&[("https://example.com", true)], 19);
        let mut handler = LinkHandler::new();
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((19, 0)),
            &mut handler,
        );
        assert!(uris(&handler).is_empty());
        viewport[0] = row_with("https://example.com/more ", 30, true);
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((25, 0)),
            &mut handler,
        );
        assert_eq!(uris(&handler), vec!["https://example.com/more".to_owned()]);
    }

    #[test]
    fn a_url_the_cursor_sits_right_after_is_left_for_later() {
        let mut viewport = screen(&[("> http://www.e", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((14, 0)),
            &mut handler,
        );
        assert!(uris(&handler).is_empty());
        assert!(viewport[0].link_scan_pending);
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((0, 1)),
            &mut handler,
        );
        assert_eq!(uris(&handler), vec!["http://www.e".to_owned()]);
        assert!(!viewport[0].link_scan_pending);
    }

    #[test]
    fn a_linked_url_that_grew_is_relinked_whole() {
        let mut viewport = screen(&[("http://www.e", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let first = anchors(&viewport[0])[0];
        for (index, character) in "xample.com".chars().enumerate() {
            viewport[0].columns[12 + index].character = character;
        }
        viewport[0].columns[12].styles = viewport[0].columns[0].styles.clone();
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let marked = anchors(&viewport[0]);
        assert_ne!(marked[0], first);
        assert!(marked[..22].iter().all(|anchor| *anchor == marked[0]));
        assert_eq!(marked[22], Some(LinkAnchor::End));
        assert!(handler
            .links()
            .values()
            .any(|link| link.uri == "http://www.example.com"));
    }

    #[test]
    fn a_linked_url_that_shrank_drops_its_stale_tail() {
        let mut viewport = screen(&[("http://www.example.com", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        viewport[0].columns[15].character = ' ';
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let marked = anchors(&viewport[0]);
        assert!(marked[..15]
            .iter()
            .all(|anchor| *anchor == marked[0] && is_start(anchor)));
        assert_eq!(marked[15], Some(LinkAnchor::End));
        assert!(marked[16..].iter().all(|anchor| anchor.is_none()));
    }

    #[test]
    fn scanning_the_same_line_twice_creates_one_link() {
        let mut viewport = screen(&[("x https://example.com y", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(handler.links().len(), 1);
    }

    #[test]
    fn scanning_an_already_scanned_row_does_not_walk_its_line() {
        let mut viewport = screen(&[("x https://example.com y", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        reset_hyperlink_scan_counters();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(hyperlink_row_visits(), 0);
    }

    #[test]
    fn a_watermark_past_a_shrunk_row_is_repaired_without_a_dirty_flag() {
        let mut viewport = screen(&[("https://example.com", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        viewport[0].link_prefix_scanned = 1_000;
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(
            viewport[0].link_prefix_scanned as usize,
            viewport[0].columns.len()
        );
    }

    #[test]
    fn several_urls_on_one_line_get_their_own_links() {
        let mut viewport = screen(&[("http://first.com and https://second.com", true)], 60);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(
            uris(&handler),
            vec![
                "http://first.com".to_owned(),
                "https://second.com".to_owned()
            ]
        );
        let marked = anchors(&viewport[0]);
        assert_ne!(marked[0], marked[21]);
    }

    #[test]
    fn dirty_rows_in_one_wrapped_line_scan_one_suffix() {
        // A burst of dirty rows inside one wrapped logical line is scanned
        // from the first resume row, not once per dirty row.
        let mut viewport: VecDeque<Row> = (0..20)
            .map(|i| row_with("https://example.com/a", 12, i == 0))
            .collect();
        let mut lines_above = VecDeque::new();
        let mut handler = LinkHandler::new();
        reset_hyperlink_scan_counters();
        let changed: Vec<isize> = (0..20).collect();
        link_urls_in_lines(
            &mut lines_above,
            &mut viewport,
            &changed,
            None,
            &mut handler,
        );
        assert!(hyperlink_row_visits() < 200, "{}", hyperlink_row_visits());
        assert!(!uris(&handler).is_empty());
    }

    #[test]
    fn the_end_marker_does_not_clobber_a_link_that_follows() {
        let url = "https://example.com";
        let mut viewport = screen(&[(&format!("{} ", url), true)], 40);
        let mut handler = LinkHandler::new();
        let neighbour = handler
            .dispatch_osc8(&[b"8", b"", b"https://example.com/neighbour"])
            .expect("an osc 8 anchor");
        viewport[0].columns[url.len()]
            .styles
            .update(|styles| styles.link_anchor = Some(neighbour));
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert_eq!(anchors(&viewport[0])[url.len()], Some(neighbour));
    }

    #[test]
    fn a_url_before_compressed_scrollback_blanks_gets_its_end_marker() {
        let url = "http://example.com";
        let mut columns: Vec<TerminalCharacter> = url.chars().map(TerminalCharacter::new).collect();
        columns.resize(80, EMPTY_TERMINAL_CHARACTER);
        let mut stored = Row::from_columns(columns).canonical();
        stored.compress_trailing_blanks();
        assert_eq!(stored.columns.len(), url.len());
        let mut lines_above = VecDeque::from(vec![stored]);
        let mut viewport = screen(&[("", true)], 80);
        let mut handler = LinkHandler::new();
        let start = line_start(&lines_above, &viewport, -1);
        let end = line_rows(&lines_above, &viewport, start);
        let cells: Vec<Cell> = (start..end)
            .flat_map(|y| {
                let row = row_ref(&lines_above, &viewport, y).unwrap();
                row.columns
                    .iter()
                    .enumerate()
                    .map(move |(index, c)| Cell {
                        character: c.character,
                        y,
                        index,
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        link_urls_in_line(&mut lines_above, &mut viewport, &cells, None, &mut handler);
        let expanded = lines_above[0].with_trailing_blanks();
        assert!(expanded.columns[..url.len()]
            .iter()
            .all(|c| is_start(&c.styles.link_anchor)));
        assert_eq!(
            expanded.columns[url.len()].styles.link_anchor,
            Some(LinkAnchor::End)
        );
    }

    #[test]
    fn changed_url_clears_the_old_anchor_when_link_ids_are_exhausted() {
        let old_url = "https://example.com";
        let changed_url = "https://example.com/extra";
        let mut viewport = screen(&[(old_url, true)], 80);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(is_start(&anchors(&viewport[0])[0]));

        let mut filler = 0;
        while handler
            .new_link_from_url(format!("https://filler.example/{filler}"))
            .is_some()
        {
            filler += 1;
        }
        for (cell, character) in viewport[0].columns.iter_mut().zip(changed_url.chars()) {
            cell.character = character;
        }
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        assert!(anchors(&viewport[0]).iter().all(Option::is_none));
    }

    #[test]
    fn overwriting_inside_a_linked_url_clears_the_whole_old_link_when_ids_are_exhausted() {
        let old_url = "https://example.com/path";
        let mut viewport = screen(&[(old_url, true)], 80);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(anchors(&viewport[0])[..old_url.len()].iter().all(is_start));

        let mut filler = 0;
        while handler
            .new_link_from_url(format!("https://filler.example/{filler}"))
            .is_some()
        {
            filler += 1;
        }
        // An ordinary overwrite: the new character carries no link anchor.
        viewport[0].columns[12] = TerminalCharacter::new('X');
        viewport[0].invalidate_link_scan();
        assert!(anchors(&viewport[0])[12].is_none());
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        assert!(
            anchors(&viewport[0]).iter().all(Option::is_none),
            "{:?}",
            anchors(&viewport[0])
        );
    }

    #[test]
    fn overwriting_a_url_character_with_a_space_drops_the_stale_tail() {
        let mut viewport = screen(&[("http://www.example.com", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        viewport[0].columns[15] = TerminalCharacter::new(' ');
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        let marked = anchors(&viewport[0]);
        assert!(marked[..15]
            .iter()
            .all(|anchor| *anchor == marked[0] && is_start(anchor)));
        assert_eq!(marked[15], Some(LinkAnchor::End));
        assert!(marked[16..].iter().all(|anchor| anchor.is_none()));
    }

    #[test]
    fn overwriting_the_start_of_a_linked_url_drops_the_link() {
        let mut viewport = screen(&[("see https://example.com/path now", true)], 60);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(is_start(&anchors(&viewport[0])[4]));

        // the `h` is overwritten, so no URL starts here any more
        viewport[0].columns[4] = TerminalCharacter::new('X');
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        assert!(
            anchors(&viewport[0]).iter().all(Option::is_none),
            "{:?}",
            anchors(&viewport[0])
        );
    }

    #[test]
    fn overwriting_the_only_colon_of_a_linked_url_drops_the_link() {
        let mut viewport = screen(&[("https://example.com", true)], 40);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        viewport[0].columns[5] = TerminalCharacter::new('-');
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(anchors(&viewport[0]).iter().all(Option::is_none));
    }

    #[test]
    fn dropping_an_orphaned_detected_link_leaves_application_links_alone() {
        let mut viewport = screen(&[("zzzz: some text", true)], 40);
        let mut handler = LinkHandler::new();
        let explicit = handler
            .dispatch_osc8(&[b"8", b"", b"https://example.com/explicit"])
            .expect("an osc 8 anchor");
        for cell in &mut viewport[0].columns[0..4] {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(anchors(&viewport[0])[..4]
            .iter()
            .all(|anchor| *anchor == Some(explicit)));
    }

    #[test]
    fn explicit_osc8_url_looking_label_keeps_its_target() {
        // Visible text looks like a different URL than the OSC 8 target.
        let label = "https://wrong.example";
        let mut viewport = screen(&[(label, true)], 40);
        let mut handler = LinkHandler::new();
        let explicit = handler
            .dispatch_osc8(&[b"8", b"", b"https://right.example/target"])
            .expect("an osc 8 anchor");
        for cell in &mut viewport[0].columns[..label.len()] {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        viewport[0].columns[label.len()]
            .styles
            .update(|styles| styles.link_anchor = Some(LinkAnchor::End));
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(
            anchors(&viewport[0])[..label.len()]
                .iter()
                .all(|anchor| *anchor == Some(explicit)),
            "{:?}",
            anchors(&viewport[0])
        );
        assert_eq!(
            handler.uri(Some(explicit)),
            Some("https://right.example/target")
        );
        // Detection must not register a competing target for the label.
        assert!(!handler
            .links()
            .values()
            .any(|link| link.detected && link.uri == label));
    }

    #[test]
    fn explicit_osc8_span_containing_url_text_is_not_replaced() {
        let text = "see https://wrong.example now";
        let mut viewport = screen(&[(text, true)], 40);
        let mut handler = LinkHandler::new();
        let explicit = handler
            .dispatch_osc8(&[b"8", b"", b"https://right.example/target"])
            .expect("an osc 8 anchor");
        for cell in &mut viewport[0].columns[..text.len()] {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(
            anchors(&viewport[0])[..text.len()]
                .iter()
                .all(|anchor| *anchor == Some(explicit)),
            "{:?}",
            anchors(&viewport[0])
        );
        assert_eq!(
            handler.uri(Some(explicit)),
            Some("https://right.example/target")
        );
    }

    #[test]
    fn mixed_explicit_and_plain_url_on_one_line() {
        let explicit_label = "https://wrong.example";
        let plain = "https://plain.example";
        let text = format!("{explicit_label} and {plain}");
        let mut viewport = screen(&[(&text, true)], 80);
        let mut handler = LinkHandler::new();
        let explicit = handler
            .dispatch_osc8(&[b"8", b"", b"https://right.example/target"])
            .expect("an osc 8 anchor");
        for cell in &mut viewport[0].columns[..explicit_label.len()] {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        viewport[0].columns[explicit_label.len()]
            .styles
            .update(|styles| styles.link_anchor = Some(LinkAnchor::End));
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        let marked = anchors(&viewport[0]);
        assert!(
            marked[..explicit_label.len()]
                .iter()
                .all(|anchor| *anchor == Some(explicit)),
            "{marked:?}"
        );
        assert_eq!(
            handler.uri(Some(explicit)),
            Some("https://right.example/target")
        );

        let plain_at = text.find(plain).expect("plain url");
        assert!(
            marked[plain_at..plain_at + plain.len()]
                .iter()
                .all(is_start),
            "{marked:?}"
        );
        assert_eq!(marked[plain_at + plain.len()], Some(LinkAnchor::End));
        assert!(handler
            .links()
            .values()
            .any(|link| link.detected && link.uri == plain));
    }

    #[test]
    fn detected_only_orphaned_cleanup_still_clears_broken_urls() {
        let mut viewport = screen(&[("see https://example.com/path now", true)], 60);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(is_start(&anchors(&viewport[0])[4]));

        // Overwriting the scheme start leaves an orphaned detected run.
        viewport[0].columns[4] = TerminalCharacter::new('X');
        viewport[0].invalidate_link_scan();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(
            anchors(&viewport[0]).iter().all(Option::is_none),
            "{:?}",
            anchors(&viewport[0])
        );
    }

    #[test]
    fn wrapped_explicit_osc8_is_not_replaced_by_cut_first_row_detection() {
        // App-owned OSC 8 often wraps mid-URL (Neovim). Only the first row
        // starts with a scheme, so automatic detection would invent a cut
        // target if it were allowed to overwrite the explicit span.
        let row1 = "https://example.com/very-";
        let row2 = "long/path";
        let mut viewport = screen(&[(row1, true), (row2, false), ("", true)], row1.len());
        let mut handler = LinkHandler::new();
        let explicit = handler
            .dispatch_osc8(&[b"8", b"", b"https://example.com/very-long/path"])
            .expect("an osc 8 anchor");
        for cell in &mut viewport[0].columns {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        for cell in &mut viewport[1].columns[..row2.len()] {
            cell.styles
                .update(|styles| styles.link_anchor = Some(explicit));
        }
        viewport[1].columns[row2.len()]
            .styles
            .update(|styles| styles.link_anchor = Some(LinkAnchor::End));

        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        assert!(
            anchors(&viewport[0])
                .iter()
                .all(|anchor| *anchor == Some(explicit)),
            "{:?}",
            anchors(&viewport[0])
        );
        assert!(
            anchors(&viewport[1])[..row2.len()]
                .iter()
                .all(|anchor| *anchor == Some(explicit)),
            "{:?}",
            anchors(&viewport[1])
        );
        assert_eq!(
            handler.uri(Some(explicit)),
            Some("https://example.com/very-long/path")
        );
        assert!(
            !handler
                .links()
                .values()
                .any(|link| link.detected && link.uri == row1),
            "detection must not publish a cut first-row URL over OSC 8"
        );
    }

    #[test]
    fn helper_rules() {
        assert_eq!(trim_trailing_punctuation("http://a.com..."), "http://a.com");
        assert_eq!(trim_trailing_punctuation("http://a.com"), "http://a.com");
        assert!(is_valid_url("https://sub.domain.example.com/path"));
        assert!(!is_valid_url("https://"));
        assert!(!is_valid_url("short"));
    }

    #[test]
    fn incremental_append_does_not_rematerialize_the_prefix() {
        let prefix = "http://example.org/";
        // Width equals the prefix so the row is not padded with blanks.
        // Appends stay on this row; each frame may copy at most one
        // max-length lookback, not the whole growing line.
        let mut viewport = screen(&[(prefix, true)], prefix.len());
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        reset_hyperlink_scan_counters();
        let frames = 64;
        let chunk = "a".repeat(64);
        for _ in 0..frames {
            let row = &mut viewport[0];
            let scanned = row.link_prefix_scanned as usize;
            for character in chunk.chars() {
                row.columns.push(TerminalCharacter::new(character));
            }
            assert_eq!(row.link_prefix_scanned as usize, scanned);
            link_urls_in_lines(
                &mut VecDeque::new(),
                &mut viewport,
                &[0],
                None,
                &mut handler,
            );
        }
        // A fresh copy of the growing prefix each frame is about
        // sum_i (prefix + i*64) ≈ 134_000 cells. Lookback is capped.
        assert!(
            hyperlink_cells_materialized() < 120_000,
            "materialized {}",
            hyperlink_cells_materialized()
        );
        assert!(uris(&handler)
            .iter()
            .any(|uri| uri.starts_with("http://example.org/")));
    }

    #[test]
    fn no_colon_append_does_not_walk_the_prefix() {
        let mut viewport = screen(&[("hello", true)], 8_000);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        reset_hyperlink_scan_counters();
        for _ in 0..48 {
            for _ in 0..64 {
                viewport[0].columns.push(TerminalCharacter::new('b'));
            }
            link_urls_in_lines(
                &mut VecDeque::new(),
                &mut viewport,
                &[0],
                None,
                &mut handler,
            );
        }
        assert!(
            hyperlink_cells_visited() < 48 * 64 + 100,
            "visited {}",
            hyperlink_cells_visited()
        );
        assert_eq!(hyperlink_cells_materialized(), 0);
    }

    fn row_with_scanned_colon(scanned: usize, total: usize) -> Row {
        let mut columns = Vec::with_capacity(total);
        for index in 0..total {
            let character = if index == 10 { ':' } else { 'x' };
            columns.push(TerminalCharacter::new(character));
        }
        let mut row = Row::from_columns(columns).canonical();
        row.link_prefix_scanned = scanned.min(u32::MAX as usize) as u32;
        row.link_prefix_has_colon = true;
        row.link_scan_dirty = false;
        row
    }

    #[test]
    fn a_scanned_colon_prefix_still_links_a_url_appended_after_it() {
        let prefix = 3000usize;
        let url = "https://new.example/path";
        let mut row = row_with_scanned_colon(prefix, prefix);
        row.columns.push(TerminalCharacter::new(' '));
        for character in url.chars() {
            row.columns.push(TerminalCharacter::new(character));
        }
        row.columns.push(TerminalCharacter::new(' '));
        let mut viewport = VecDeque::from([row]);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(
            uris(&handler).iter().any(|uri| uri == url),
            "uris {:?}",
            uris(&handler)
        );
        assert_eq!(
            viewport[0].link_prefix_scanned as usize,
            viewport[0].columns.len()
        );
    }

    #[test]
    fn a_cursor_on_a_new_url_does_not_advance_the_watermark_past_it() {
        let prefix = 3000usize;
        let url = "https://new.example/path";
        let mut row = row_with_scanned_colon(prefix, prefix);
        row.columns.push(TerminalCharacter::new(' '));
        let url_start = row.columns.len();
        for character in url.chars() {
            row.columns.push(TerminalCharacter::new(character));
        }
        row.columns.push(TerminalCharacter::new(' '));
        let mut viewport = VecDeque::from([row]);
        let mut handler = LinkHandler::new();
        link_all(
            &mut VecDeque::new(),
            &mut viewport,
            Some((url_start, 0)),
            &mut handler,
        );
        assert!(uris(&handler).is_empty(), "{:?}", uris(&handler));
        assert_eq!(viewport[0].link_prefix_scanned as usize, prefix);
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(uris(&handler).iter().any(|uri| uri == url));
    }

    #[test]
    fn a_url_past_eight_thousand_cells_is_linked_in_the_same_scan() {
        let prefix = 64usize;
        let mut row = row_with_scanned_colon(prefix, prefix);
        for _ in 0..9_000 {
            row.columns.push(TerminalCharacter::new('x'));
        }
        let url = "https://late.example/z";
        row.columns.push(TerminalCharacter::new(' '));
        for character in url.chars() {
            row.columns.push(TerminalCharacter::new(character));
        }
        row.columns.push(TerminalCharacter::new(' '));
        let full = row.columns.len();
        let mut viewport = VecDeque::from([row]);
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(
            uris(&handler).iter().any(|uri| uri == url),
            "scan uris {:?}",
            uris(&handler)
        );
        assert_eq!(viewport[0].link_prefix_scanned as usize, full);
    }

    #[test]
    fn colon_suffix_appends_resume_without_walking_the_row_prefix() {
        let width = 80;
        let rows = 400;
        let first = format!("http://example.com/{}", "a".repeat(width - 21));
        let mut viewport: VecDeque<Row> = (0..rows)
            .map(|index| {
                if index == 0 {
                    row_with(&first, width, true)
                } else {
                    row_with(&"x".repeat(width), width, false)
                }
            })
            .collect();
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);
        assert!(uris(&handler)
            .iter()
            .any(|uri| uri.starts_with("http://example.com/")));
        let first_anchor = anchors(&viewport[0])[0];
        assert!(is_start(&first_anchor));

        reset_hyperlink_scan_counters();
        let frames = 32;
        let appended = "https://appended.example/path";
        for frame in 0..frames {
            let row = viewport.back_mut().expect("a last row");
            if frame == frames / 2 {
                row.columns.push(TerminalCharacter::new(' '));
                for character in appended.chars() {
                    row.columns.push(TerminalCharacter::new(character));
                }
                row.columns.push(TerminalCharacter::new(' '));
            } else {
                for _ in 0..64 {
                    row.columns.push(TerminalCharacter::new('x'));
                }
            }
            let last = viewport.len() as isize - 1;
            link_urls_in_lines(
                &mut VecDeque::new(),
                &mut viewport,
                &[last],
                None,
                &mut handler,
            );
        }
        assert!(
            hyperlink_row_visits() < frames * 128,
            "row visits {}",
            hyperlink_row_visits()
        );
        assert!(
            hyperlink_cells_materialized() < frames * 4_096,
            "materialized {}",
            hyperlink_cells_materialized()
        );
        assert!(uris(&handler).iter().any(|uri| uri == appended));
        assert_eq!(anchors(&viewport[0])[0], first_anchor);
    }

    #[test]
    fn colon_free_suffix_appends_keep_the_prefix_out_of_the_scan() {
        let width = 80;
        let rows = 400;
        let mut viewport: VecDeque<Row> = (0..rows)
            .map(|index| row_with(&"x".repeat(width), width, index == 0))
            .collect();
        let mut handler = LinkHandler::new();
        link_all(&mut VecDeque::new(), &mut viewport, None, &mut handler);

        reset_hyperlink_scan_counters();
        let frames = 32;
        for _ in 0..frames {
            let row = viewport.back_mut().expect("a last row");
            for _ in 0..64 {
                row.columns.push(TerminalCharacter::new('x'));
            }
            let last = viewport.len() as isize - 1;
            link_urls_in_lines(
                &mut VecDeque::new(),
                &mut viewport,
                &[last],
                None,
                &mut handler,
            );
        }
        assert!(
            hyperlink_row_visits() < frames * 16,
            "row visits {}",
            hyperlink_row_visits()
        );
        assert_eq!(hyperlink_cells_materialized(), 0);
        assert!(uris(&handler).is_empty());
    }
}
