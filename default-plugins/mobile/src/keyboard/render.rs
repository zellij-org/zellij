//! Generic, layout-agnostic keyboard renderer.
//!
//! Reads `&dyn KeyboardLayout` and renders into the plugin's ANSI
//! stream via `print!`. Cells render as contiguous blocks of ANSI
//! background colour — no box-drawing characters. Cell separation is
//! purely visual contrast between adjacent backgrounds.
//!
//! Each visible cell is also pushed into `click_regions` so taps land
//! through the same dispatch path the rest of the plugin uses.

use std::collections::HashMap;
use std::time::Instant;

use unicode_width::UnicodeWidthStr;

use crate::state::{ClickAction, ClickRegion};

use super::layout::{CellId, KeyRow, KeyboardLayout};
use super::modifiers::KeyboardModifiers;

/// Reset SGR — emitted after every row to discard the bg colour.
const RESET: &str = "\x1b[0m";

/// Palette pair for "outer" rows (extras strip, letter rows 1 & 3,
/// bottom bar on Letters/Symbols layers). Two 256-color indices for
/// the alternating checker.
const P_OUTER: [u8; 2] = [236, 240];
/// Palette pair for "inner" rows (letter row 2). `P_OUTER` and
/// `P_INNER` are disjoint by construction so adjacent rows never share
/// a shade.
const P_INNER: [u8; 2] = [238, 242];
/// Bright blue used for armed-modifier cells, active-layer cells, and
/// the transient press-flash highlight.
const ACTIVE: u8 = 33;

/// Horizontal hit-slop, in terminal columns. Each cell's slop region
/// extends this many columns past its visible left/right edges so
/// clicks just past the edge still resolve to the cell.
const SLOP_H: usize = 1;
/// Vertical hit-slop, in terminal rows. Each cell's slop region
/// extends this many rows above and below its content row so clicks
/// on the surrounding boundary still resolve to the cell. Adjacent
/// rows' slop regions overlap on the shared boundary — resolved by
/// nearest-center at dispatch time.
const SLOP_V: usize = 1;

/// Total terminal-rows the keyboard occupies for the given layout
/// under the supplied modifier state. Sums each `KeyRow`'s `height`
/// so option-2b tall rows (height = 2) inflate the budget the embedded
/// viewport must subtract from its body allowance.
pub fn keyboard_rows(layout: &dyn KeyboardLayout, mods: &KeyboardModifiers) -> usize {
    layout.rows(mods).iter().map(|r| r.height as usize).sum()
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

    // Block width = widest row in the active layer. Every row in the
    // layer is indented by the same `left_pad` so the keyboard reads
    // as one centered block with a stable left edge; the per-row
    // `offset_col` (e.g. the half-key stagger on Letters row 2) is
    // applied on top of that shared indent. Per-layer rather than
    // global so each layer fits the viewport tightly — Letters,
    // Symbols and Functions have different total widths because their
    // cell counts and widths differ.
    let block_width = block_width(&rows);
    let left_pad = cols.saturating_sub(block_width) / 2;

    let mut term_row = plugin_row_start;
    for (row_index, row) in rows.iter().enumerate() {
        let palette = palette_for_row(row_index);
        let height = row.height as usize;

        // Padding row(s) above the label row paint each cell's bg
        // across its column range; no labels, no click regions. For
        // height == 1 this loop runs zero times and the cell renders
        // exactly as it did before option 2b.
        for pad_offset in 0..height.saturating_sub(1) {
            render_padding_row(
                layout,
                mods,
                press_flash,
                row,
                palette,
                term_row + pad_offset,
                left_pad,
                cols,
            );
        }

        // Label row sits at the bottom of the cell rectangle. Click
        // regions span the full `[term_row, term_row + height)` so
        // taps anywhere inside the padding-plus-label rectangle hit.
        render_row(
            layout,
            mods,
            press_flash,
            row,
            palette,
            term_row,
            height,
            left_pad,
            cols,
            click_regions,
        );

        term_row += height;
    }

    term_row - plugin_row_start
}

/// Widest row in `rows`, measured as `offset_col + last_cell.col_end`.
/// Used as the block width that centers the keyboard horizontally.
/// Returns 0 for an empty `rows` slice.
fn block_width(rows: &[KeyRow]) -> usize {
    rows.iter()
        .map(|r| {
            let last_end = r
                .cells
                .last()
                .map(|c| c.col_end as usize)
                .unwrap_or(0);
            r.offset_col as usize + last_end + r.right_pad as usize
        })
        .max()
        .unwrap_or(0)
}

/// Pure alternation: even rows get `P_OUTER`, odd rows get `P_INNER`.
/// Row 0 is the extras strip on every layer, so `P_OUTER` matches the
/// reference mock for both 5-row (Letters/Symbols) and 4-row (Fn)
/// layers. The Fn layer's bottom bar lands on `P_INNER` as a side
/// effect; the spec accepts this since palette consistency only
/// matters *within* a single layer.
fn palette_for_row(row_index: usize) -> [u8; 2] {
    if row_index % 2 == 0 {
        P_OUTER
    } else {
        P_INNER
    }
}

/// Render the label-bearing row of a key row. `cell_top` is the top
/// terminal row of the cell rectangle (which is `term_row_top` in
/// `render_keyboard`); `height` is the cell's vertical extent in
/// terminal rows; `left_pad` is the column the row starts at after
/// horizontal centering. The label paints at `cell_top + height - 1`;
/// click regions span `[cell_top, cell_top + height)` so taps anywhere
/// in the padding-plus-label rectangle resolve to the cell.
#[allow(clippy::too_many_arguments)]
fn render_row(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    palette: [u8; 2],
    cell_top: usize,
    height: usize,
    left_pad: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    let offset = row.offset_col as usize;
    let cell_bottom = cell_top + height; // exclusive
    let label_row = cell_top + height.saturating_sub(1);

    // Build the row line as a single string so we can clip it once at
    // the end. Leading `offset` columns are *unstyled* — they bear no
    // bg colour, just blanks (matches the mock's stagger gap). The
    // shared `left_pad` is applied by the cursor-positioning escape
    // below rather than padded into `buf`, keeping the buf focused on
    // the row's intrinsic geometry.
    let mut buf = String::new();
    for _ in 0..offset {
        buf.push(' ');
    }

    for (cell_index, cell) in row.cells.iter().enumerate() {
        let shade = compute_shade(layout, mods, press_flash, cell.id, palette, cell_index);
        let cell_width = (cell.col_end - cell.col_start) as usize;
        let label = layout.label(cell.id, mods);
        let centered = center(label.as_ref(), cell_width);

        buf.push_str(&ansi_bg(shade));
        buf.push_str(&centered);

        // Click region geometry uses absolute viewport columns so the
        // dispatcher can match clicks against viewport columns
        // directly: `left_pad` (centering indent) + `offset` (row
        // stagger) + cell column.
        let abs_start = left_pad + offset + cell.col_start as usize;
        let abs_end = left_pad + offset + cell.col_end as usize;

        click_regions.push(ClickRegion::tight_range(
            cell_top,
            cell_bottom,
            abs_start,
            abs_end,
            ClickAction::Keyboard(cell.id),
        ));

        // Slop halo extends ±SLOP_V rows around the cell's outer
        // rectangle, ±SLOP_H cols around its visible width. The
        // center used for nearest-center tiebreaks sits on the label
        // row (where the user visually targets the glyph) and the
        // horizontal midpoint of the cell.
        let cx = (abs_start + abs_end).saturating_sub(1) / 2;
        let cy = label_row;
        let slop_col_start = abs_start.saturating_sub(SLOP_H);
        let slop_col_end = abs_end + SLOP_H;
        let slop_row_start = cell_top.saturating_sub(SLOP_V);
        let slop_row_end = cell_bottom + SLOP_V;
        click_regions.push(ClickRegion::slop_range(
            slop_row_start,
            slop_row_end,
            slop_col_start,
            slop_col_end,
            ClickAction::Keyboard(cell.id),
            (cx, cy),
        ));
    }

    // Drop the bg colour at the end of the row so trailing terminal
    // cells (and the next row's leading offset spaces) don't inherit
    // the last cell's shade. Any `right_pad` columns are emitted as
    // unstyled spaces after the reset so they don't pick up the last
    // cell's bg.
    buf.push_str(RESET);
    for _ in 0..row.right_pad as usize {
        buf.push(' ');
    }

    let visible = cols.saturating_sub(left_pad);
    print!(
        "{}\x1b[{};{}H{}",
        RESET,
        label_row + 1,
        left_pad + 1,
        clip_buf(&buf, visible),
    );
}

/// Paint the bg-only padding row that sits above the label row of an
/// option-2b tall cell. Each cell emits its bg colour across its full
/// `[col_start, col_end)` column range — no label, no click region.
/// The shade matches what `compute_shade` would emit for the label
/// row, so press-flash, armed-modifier, and active-layer inversions
/// paint identically across both rows of the cell.
fn render_padding_row(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    palette: [u8; 2],
    term_row: usize,
    left_pad: usize,
    cols: usize,
) {
    let offset = row.offset_col as usize;
    let mut buf = String::new();
    for _ in 0..offset {
        buf.push(' ');
    }
    for (cell_index, cell) in row.cells.iter().enumerate() {
        let shade = compute_shade(layout, mods, press_flash, cell.id, palette, cell_index);
        let cell_width = (cell.col_end - cell.col_start) as usize;
        buf.push_str(&ansi_bg(shade));
        for _ in 0..cell_width {
            buf.push(' ');
        }
    }
    // Match the label row: drop bg before emitting any `right_pad` so
    // the trailing padding is unstyled, not bg-shaded.
    buf.push_str(RESET);
    for _ in 0..row.right_pad as usize {
        buf.push(' ');
    }
    let visible = cols.saturating_sub(left_pad);
    print!(
        "{}\x1b[{};{}H{}",
        RESET,
        term_row + 1,
        left_pad + 1,
        clip_buf(&buf, visible),
    );
}

/// Determine the background shade for `cell` under the current state.
/// Priority order (first matching wins):
///   1. live press-flash entry → `ACTIVE`
///   2. cell is an armed modifier → `ACTIVE`
///   3. cell's `layer_of` matches the current layer → `ACTIVE`
///   4. otherwise → palette[cell_index % 2]
fn compute_shade(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    cell: CellId,
    palette: [u8; 2],
    cell_index: usize,
) -> u8 {
    if press_flash.contains_key(&cell) {
        return ACTIVE;
    }
    if let Some(m) = layout.modifier_of(cell) {
        if mods.is_armed(m) {
            return ACTIVE;
        }
    }
    if let Some(l) = layout.layer_of(cell) {
        if mods.layer == l {
            return ACTIVE;
        }
    }
    palette[cell_index % 2]
}

/// Return the ANSI SGR set-background sequence for the supplied
/// 256-color palette index.
fn ansi_bg(index: u8) -> String {
    format!("\x1b[48;5;{}m", index)
}

/// Centre `label` in a field of exactly `width` terminal columns.
/// Asymmetric extra space goes to the right (so visual consistency is
/// preserved regardless of odd/even widths). If the label is wider
/// than `width`, it is truncated char-by-char.
fn center(label: &str, width: usize) -> String {
    let label_width = UnicodeWidthStr::width(label);
    if label_width >= width {
        // Truncate: copy chars until we hit the cap.
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
        return out;
    }
    let total_pad = width - label_width;
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    let mut out = String::with_capacity(width);
    for _ in 0..left_pad {
        out.push(' ');
    }
    out.push_str(label);
    for _ in 0..right_pad {
        out.push(' ');
    }
    out
}

/// Clip a line containing inline ANSI escape sequences to `max_cells`
/// visible terminal cells. Escape sequences pass through unchanged
/// (they consume no cells); printable chars are accounted via
/// `unicode-width`.
fn clip_buf(s: &str, max_cells: usize) -> String {
    let bytes = s.as_bytes();
    let mut out = String::new();
    let mut used = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
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
    use super::*;

    #[test]
    fn center_one_char_in_three_cols() {
        assert_eq!(center("q", 3), " q ");
    }

    #[test]
    fn center_three_chars_in_five_cols() {
        assert_eq!(center("Esc", 5), " Esc ");
    }

    #[test]
    fn center_one_char_in_wide_seven_col_cell() {
        // The Letters row-3 backspace cell.
        assert_eq!(center("⌫", 7), "   ⌫   ");
    }

    #[test]
    fn center_asymmetric_extra_goes_right() {
        // Two chars in five cols: extra = 3, left = 1, right = 2.
        assert_eq!(center("F1", 5), " F1  ");
    }

    #[test]
    fn center_empty_label_pads_full_width() {
        assert_eq!(center("", 4), "    ");
    }

    #[test]
    fn palette_alternates_outer_inner() {
        assert_eq!(palette_for_row(0), P_OUTER);
        assert_eq!(palette_for_row(1), P_INNER);
        assert_eq!(palette_for_row(2), P_OUTER);
        assert_eq!(palette_for_row(3), P_INNER);
        assert_eq!(palette_for_row(4), P_OUTER);
    }

    #[test]
    fn ansi_bg_format() {
        assert_eq!(ansi_bg(236), "\x1b[48;5;236m");
        assert_eq!(ansi_bg(33), "\x1b[48;5;33m");
    }

    /// `block_width` reports the widest row's right edge, accounting
    /// for the row's `offset_col` stagger.
    #[test]
    fn block_width_uses_widest_row() {
        use crate::keyboard::layout::{CellId, KeyCell, KeyRow};

        let r1 = KeyRow::tall(0, vec![KeyCell { col_start: 0, col_end: 30, id: CellId(0) }]);
        let r2 = KeyRow::tall(2, vec![KeyCell { col_start: 0, col_end: 27, id: CellId(1) }]);
        let r3 = KeyRow::tall(0, vec![KeyCell { col_start: 0, col_end: 34, id: CellId(2) }]);
        assert_eq!(block_width(&[r1, r2, r3]), 34);
    }

    /// `block_width` reports per-layer maxima for the US-QWERTY
    /// layout: Letters = 34 (bottom bar), Symbols = 39 (symbol rows),
    /// Functions = 41 (F-key rows).
    #[test]
    fn block_width_per_layer() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        assert_eq!(block_width(&layout.rows(&mods)), 34);
        mods.layer = KeyLayer::Symbols;
        assert_eq!(block_width(&layout.rows(&mods)), 39);
        mods.layer = KeyLayer::Functions;
        assert_eq!(block_width(&layout.rows(&mods)), 41);
    }

    /// Row budget with every visible row (including the extras strip
    /// and bottom bar) bumped to height 2. Letters and Symbols layers
    /// occupy 2 + 3·2 + 2 = 10 rows; Functions occupies 2 + 2·2 + 2
    /// = 8 rows.
    #[test]
    fn keyboard_rows_sums_heights() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        assert_eq!(keyboard_rows(&layout, &mods), 10); // Letters
        mods.layer = KeyLayer::Symbols;
        assert_eq!(keyboard_rows(&layout, &mods), 10);
        mods.layer = KeyLayer::Functions;
        assert_eq!(keyboard_rows(&layout, &mods), 8);
    }
}
