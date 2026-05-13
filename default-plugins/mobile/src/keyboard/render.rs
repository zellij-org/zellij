//! Generic, layout-agnostic keyboard renderer.
//!
//! Reads `&dyn KeyboardLayout` and renders into the plugin's ANSI
//! stream via `print!`. Border merging is purely geometric — driven
//! by adjacent rows' wall positions and horizontal extents — so a
//! new layout drops in without renderer changes.
//!
//! Each visible cell is also pushed into `click_regions` so taps land
//! through the same dispatch path the rest of the plugin uses.

use std::collections::HashMap;
use std::time::Instant;

use unicode_width::UnicodeWidthStr;

use crate::state::{ClickAction, ClickRegion};

use super::layout::{CellId, KeyRow, KeyboardLayout};
use super::modifiers::KeyboardModifiers;

/// Reset SGR — emitted between cells so leftover styling from the
/// inverted-flash cell does not bleed into adjacent cells.
const RESET: &str = "\x1b[0m";
/// Enable reverse-video — used to mark armed modifier cells and the
/// transient press-flash highlight.
const REV: &str = "\x1b[7m";
/// Disable reverse-video.
const NOREV: &str = "\x1b[27m";

/// Total terminal-rows the keyboard occupies for the given layout
/// under the supplied modifier state. Equal to `2 * rows.len() + 1`
/// (top border + per-row [content + divider] - 1 internal-divider +
/// bottom border).
pub fn keyboard_rows(layout: &dyn KeyboardLayout, mods: &KeyboardModifiers) -> usize {
    let rows = layout.rows(mods);
    if rows.is_empty() {
        0
    } else {
        2 * rows.len() + 1
    }
}

/// Render the keyboard at `(plugin_row_start, 0)` with at most `cols`
/// terminal columns. Pushes a `ClickRegion` per visible cell into
/// `click_regions`. Returns the number of terminal rows actually
/// drawn (matches `keyboard_rows`).
pub fn render_keyboard(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    plugin_row_start: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) -> usize {
    let rows = layout.rows(mods);
    if rows.is_empty() {
        return 0;
    }

    // Compute the keyboard's outermost extent so border lines extend
    // far enough to enclose every row. Cells beyond `cols` get clipped
    // by the host; we still position cells at their natural column.
    let keyboard_width = rows
        .iter()
        .map(|r| r.offset_col as usize + max_col_end(r))
        .max()
        .unwrap_or(0)
        + 1; // include the rightmost wall column itself

    let mut term_row = plugin_row_start;

    // Top border — derived from the first row's geometry.
    let top = horizontal_line(None, Some(&rows[0]), keyboard_width);
    print_line(term_row, 0, &top, cols);
    term_row += 1;

    for (i, row) in rows.iter().enumerate() {
        render_row_content(
            layout,
            mods,
            press_flash,
            row,
            term_row,
            cols,
            click_regions,
        );
        term_row += 1;
        let below = rows.get(i + 1);
        let line = horizontal_line(Some(row), below, keyboard_width);
        print_line(term_row, 0, &line, cols);
        term_row += 1;
    }

    term_row - plugin_row_start
}

fn max_col_end(row: &KeyRow) -> usize {
    row.cells
        .iter()
        .map(|c| c.col_end as usize)
        .max()
        .unwrap_or(0)
}

fn min_col_start(row: &KeyRow) -> usize {
    row.cells
        .iter()
        .map(|c| c.col_start as usize)
        .min()
        .unwrap_or(0)
}

/// Build the box-drawing line that sits between `above` and `below`.
/// Either side may be `None` (top / bottom border).
fn horizontal_line(
    above: Option<&KeyRow>,
    below: Option<&KeyRow>,
    keyboard_width: usize,
) -> String {
    // Precompute wall sets and horizontal extents in absolute
    // (offset-applied) column space.
    let walls_above = above.map(walls_of).unwrap_or_default();
    let walls_below = below.map(walls_of).unwrap_or_default();
    let extent_above = above.map(extent_of);
    let extent_below = below.map(extent_of);

    let mut out = String::new();
    for c in 0..keyboard_width {
        let up = walls_above.contains(&c) && in_extent(c, extent_above);
        let down = walls_below.contains(&c) && in_extent(c, extent_below);
        // For c=0 there is no column to the left, so the left half of
        // the divider is always empty. Without this guard `c - 1`
        // would underflow and `horiz_segment` would mis-classify the
        // huge wrapped value as being inside the row's extent.
        let left = if c > 0 {
            horiz_segment(c - 1, c, extent_above, extent_below)
        } else {
            false
        };
        let right = horiz_segment(c, c + 1, extent_above, extent_below);
        out.push(box_char(up, down, left, right));
    }
    out
}

/// Set of column positions where this row has a vertical wall, after
/// applying `offset_col`. Each cell contributes both its `col_start`
/// and `col_end` (cells that share a wall yield duplicate-but-equal
/// entries — fine for a `HashSet`).
fn walls_of(row: &KeyRow) -> std::collections::HashSet<usize> {
    let mut set = std::collections::HashSet::new();
    let off = row.offset_col as usize;
    for cell in &row.cells {
        set.insert(off + cell.col_start as usize);
        set.insert(off + cell.col_end as usize);
    }
    set
}

/// Horizontal extent of a row in absolute (offset-applied) columns:
/// `(left_edge, right_edge)` inclusive of the rightmost wall.
fn extent_of(row: &KeyRow) -> (usize, usize) {
    let off = row.offset_col as usize;
    let left = off + min_col_start(row);
    let right = off + max_col_end(row);
    (left, right)
}

fn in_extent(c: usize, extent: Option<(usize, usize)>) -> bool {
    match extent {
        Some((l, r)) => c >= l && c <= r,
        None => false,
    }
}

/// True iff the half-open segment `(a, b)` (in column space) is fully
/// covered by at least one of the supplied extents. `a` may underflow
/// (we pass `wrapping_sub(1)` for c=0) — handled by the `c >= l`
/// check failing for the giant wrapped value.
fn horiz_segment(
    a: usize,
    b: usize,
    extent_above: Option<(usize, usize)>,
    extent_below: Option<(usize, usize)>,
) -> bool {
    let check = |ext: Option<(usize, usize)>| {
        if let Some((l, r)) = ext {
            a >= l && b <= r
        } else {
            false
        }
    };
    check(extent_above) || check(extent_below)
}

/// Map a (up, down, left, right) flag tuple to a box-drawing char.
/// Pure corners (exactly two perpendicular legs) use rounded glyphs
/// — matches the mockup.
fn box_char(up: bool, down: bool, left: bool, right: bool) -> char {
    match (up, down, left, right) {
        (false, false, false, false) => ' ',
        (true, false, false, false) => '│',
        (false, true, false, false) => '│',
        (false, false, true, false) => '─',
        (false, false, false, true) => '─',
        (true, true, false, false) => '│',
        (false, false, true, true) => '─',
        (true, false, true, false) => '╯',
        (true, false, false, true) => '╰',
        (false, true, true, false) => '╮',
        (false, true, false, true) => '╭',
        (true, true, true, false) => '┤',
        (true, true, false, true) => '├',
        (true, false, true, true) => '┴',
        (false, true, true, true) => '┬',
        (true, true, true, true) => '┼',
    }
}

/// Emit `line` at `(row, col)`. Truncates to `cols` cells so a wide
/// keyboard layout in a narrow viewport degrades to clipping rather
/// than scrolling the plugin grid.
fn print_line(row: usize, col: usize, line: &str, cols: usize) {
    let visible: String = clip_to_cells(line, cols);
    print!("{}\x1b[{};{}H{}", RESET, row + 1, col + 1, visible);
}

fn clip_to_cells(s: &str, max_cells: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let mut tmp = [0u8; 4];
        let encoded = ch.encode_utf8(&mut tmp);
        let w = UnicodeWidthStr::width(encoded as &str);
        if used + w > max_cells {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

/// Render one row's content line (walls + cell labels). Pushes a
/// `ClickRegion` per cell into `click_regions`.
fn render_row_content(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    term_row: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    let off = row.offset_col as usize;
    let row_left = off + min_col_start(row);
    let row_right = off + max_col_end(row);

    // Build the line as a string, then emit it. We treat the entire
    // keyboard width as a single horizontal slice; the `move_to` call
    // positions the first char at col 0 of the plugin.
    let mut buf = String::new();
    // Lead-in spaces for `offset_col` shifts (the asdf row).
    for _ in 0..row_left {
        buf.push(' ');
    }

    for cell in &row.cells {
        let abs_start = off + cell.col_start as usize;
        let abs_end = off + cell.col_end as usize;
        // Wall at the cell's left edge. For the leftmost cell this is
        // the row's overall left border; for inner cells it doubles as
        // the previous cell's right wall — but each cell owns its own
        // left wall in this pass, which is fine: cells are emitted in
        // ascending col_start order so the previous cell's right wall
        // is naturally NOT emitted (only the start).
        buf.push('│');
        // Label fills the interior (col_start+1 .. col_end). Interior
        // width is col_end - col_start - 1 terminal cells.
        let interior_width = (cell.col_end - cell.col_start).saturating_sub(1) as usize;
        let label = layout.label(cell.id, mods);
        let inverted = is_inverted(layout, mods, press_flash, cell.id);
        if inverted {
            buf.push_str(REV);
        }
        let padded = pad_label(label.as_ref(), interior_width);
        buf.push_str(&padded);
        if inverted {
            buf.push_str(NOREV);
        }

        click_regions.push(ClickRegion {
            row: term_row,
            col_start: abs_start,
            col_end: abs_end,
            action: ClickAction::Keyboard(cell.id),
        });
    }
    // Closing right wall of the rightmost cell.
    buf.push('│');

    // Compute the print position: we already padded `row_left` leading
    // spaces, so the line starts at col 0 of the plugin and the row's
    // left wall lands at column `row_left`. Pass cols-aware clip.
    print!("{}\x1b[{};1H", RESET, term_row + 1);
    print!("{}", clip_buf(&buf, cols));
    let _ = row_right; // suppress unused: used implicitly via cells
}

/// Cell is inverted if its associated modifier is armed, or it has a
/// live press-flash entry. The renderer never re-checks ages — the
/// controller's `sweep_flash` prunes stale entries; mere presence
/// here means "still flashing".
fn is_inverted(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    cell: CellId,
) -> bool {
    if let Some(m) = layout.modifier_of(cell) {
        if mods.is_armed(m) {
            return true;
        }
    }
    press_flash.contains_key(&cell)
}

/// Left-align `label` to exactly `width` terminal cells. Pads with
/// trailing spaces if shorter; truncates char-by-char if longer.
fn pad_label(label: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in label.chars() {
        let mut tmp = [0u8; 4];
        let encoded = ch.encode_utf8(&mut tmp);
        let w = UnicodeWidthStr::width(encoded as &str);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    while used < width {
        out.push(' ');
        used += 1;
    }
    out
}

/// Clip a line containing inline ANSI reverse-video escapes to
/// `max_cells` visible cells. Pure-text variant `clip_to_cells` is
/// fine for borders; this one is for content rows where `\x1b[7m` /
/// `\x1b[27m` runs are interspersed.
fn clip_buf(s: &str, max_cells: usize) -> String {
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut used = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Copy the entire CSI sequence verbatim; it does not
            // consume terminal cells.
            let start = i;
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            } else if i < bytes.len() {
                i += 1;
            }
            out.push_str(std::str::from_utf8(&bytes[start..i]).unwrap_or(""));
        } else {
            let ch_len = utf8_char_len(bytes[i]);
            if i + ch_len > bytes.len() {
                break;
            }
            let chunk = std::str::from_utf8(&bytes[i..i + ch_len]).unwrap_or("");
            let w = UnicodeWidthStr::width(chunk);
            if used + w > max_cells {
                break;
            }
            out.push_str(chunk);
            used += w;
            i += ch_len;
        }
    }
    out
}

fn utf8_char_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte < 0xc0 {
        1
    } else if byte < 0xe0 {
        2
    } else if byte < 0xf0 {
        3
    } else {
        4
    }
}

#[cfg(test)]
mod tests {
    //! Border-merge geometry tests. The exact glyphs come from the
    //! mockups in `mobile_keyboard.md`; if the renderer drifts from
    //! them these tests fail.
    use super::*;
    use crate::keyboard::layout::{CellId, KeyCell};

    fn cells_aligned(walls: &[u16]) -> Vec<KeyCell> {
        let mut out = Vec::new();
        for w in walls.windows(2) {
            out.push(KeyCell {
                col_start: w[0],
                col_end: w[1],
                id: CellId(0),
            });
        }
        out
    }

    #[test]
    fn top_border_extras_only() {
        // Extras strip: walls at 0, 4, 8, 12, 16, 20, 23, 26, 29, 32.
        let extras = KeyRow {
            offset_col: 0,
            cells: cells_aligned(&[0, 4, 8, 12, 16, 20, 23, 26, 29, 32]),
        };
        let line = horizontal_line(None, Some(&extras), 33);
        assert_eq!(line, "╭───┬───┬───┬───┬───┬──┬──┬──┬──╮");
    }

    #[test]
    fn divider_extras_to_numbers() {
        // Mismatched walls force the alternating ┴/┬ pattern.
        let extras = KeyRow {
            offset_col: 0,
            cells: cells_aligned(&[0, 4, 8, 12, 16, 20, 23, 26, 29, 32]),
        };
        let mut numbers_walls = Vec::new();
        for i in 0..=13 {
            numbers_walls.push(i * 3);
        }
        let numbers = KeyRow {
            offset_col: 0,
            cells: cells_aligned(&numbers_walls),
        };
        let line = horizontal_line(Some(&extras), Some(&numbers), 40);
        assert_eq!(line, "├──┬┴─┬─┴┬──┼──┬┴─┬─┴┬─┴┬─┴┬─┴┬─┴┬──┬──╮");
    }

    #[test]
    fn divider_qwerty_to_asdf_with_stagger() {
        // Qwerty: 13 cells of width 3 → walls 0,3,...,39.
        // ASDF: offset 1, 11 cells of width 3 → absolute walls 1,4,...,34.
        let qwerty_walls: Vec<u16> = (0..=13).map(|i| i * 3).collect();
        let qwerty = KeyRow {
            offset_col: 0,
            cells: cells_aligned(&qwerty_walls),
        };
        let asdf_walls: Vec<u16> = (0..=11).map(|i| i * 3).collect();
        let asdf = KeyRow {
            offset_col: 1,
            cells: cells_aligned(&asdf_walls),
        };
        let line = horizontal_line(Some(&qwerty), Some(&asdf), 40);
        assert_eq!(line, "╰┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴┬─┴──╯");
    }

    #[test]
    fn bottom_border_utility() {
        let utility = KeyRow {
            offset_col: 0,
            cells: cells_aligned(&[0, 30, 36]),
        };
        let line = horizontal_line(Some(&utility), None, 37);
        assert_eq!(line, "╰─────────────────────────────┴─────╯");
    }
}

