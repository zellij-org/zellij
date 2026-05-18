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

/// Numerator / denominator of the keyboard's target row footprint. 2/5
/// = 40%. The keyboard scales its vertical extent to consume roughly
/// this fraction of the plugin's available rows, so its on-screen size
/// is stable across pinch-zoom (which changes the grid dimensions
/// rather than the plugin's UI semantics). Single tuning knob —
/// adjust here only.
const KEYBOARD_PCT_NUM: usize = 2;
const KEYBOARD_PCT_DEN: usize = 5;

/// Per-KeyRow minimum height in terminal rows. The keyboard renders at
/// this height per row when `40%` of the available rows is exactly the
/// floor; below the floor, the keyboard is suppressed entirely so the
/// embedded viewport reclaims the rows.
const MIN_ROW_HEIGHT: usize = 2;

/// Minimum unstyled padding on each side of the keyboard block at any
/// scale. Cells never sit flush against col 0 or `cols - 1` when this
/// padding fits.
const MIN_H_PAD: usize = 1;

/// Scaled geometry resolved at render time from the plugin's current
/// `rows × cols`. Horizontal scaling is a *ratio* `h_num / h_den`
/// applied to every cell's `col_start` / `col_end` (and every row's
/// `offset_col` / `right_pad`) so the widest row fills exactly
/// `cols - 2 * MIN_H_PAD` cells — the available width — while
/// preserving the relative proportions of cells within and across
/// rows. Adjacent cells share boundaries because integer rounding
/// is applied to absolute positions (`(c * h_num) / h_den`), not to
/// individual widths. `row_heights` replaces each `KeyRow.height`
/// on a per-row basis. `h_num == h_den` and `row_heights == [2; n]`
/// reproduces the unscaled rendering exactly.
///
/// `per_row_scale`, when `Some`, supplies a row-local `(num, den)`
/// ratio that overrides the global `h_num/h_den` for that row only.
/// The compact tier uses this to give each row a different scaling
/// factor: homogeneous rows (10 cells) use `(target, 10)` while
/// rows with fixed-width anchors return cells already in scaled
/// coordinates and set the row scale to `(1, 1)`. The natural tier
/// leaves this `None` and behaves exactly as before.
#[derive(Debug, Clone)]
pub struct KeyboardGeometry {
    pub h_num: u16,
    pub h_den: u16,
    pub row_heights: Vec<u16>,
    pub per_row_scale: Option<Vec<(u16, u16)>>,
}

impl KeyboardGeometry {
    /// Sum of `row_heights` — the number of terminal rows the keyboard
    /// occupies under this geometry.
    pub fn total_height(&self) -> usize {
        self.row_heights.iter().map(|h| *h as usize).sum()
    }

    /// Map a natural column coordinate to a scaled terminal column.
    /// Use this on absolute positions (`col_start`, `col_end`,
    /// `offset_col`); per-cell widths are then `scale_col(col_end) -
    /// scale_col(col_start)`, which preserves adjacent-cell boundaries.
    pub fn scale_col(&self, c: u16) -> usize {
        (c as usize * self.h_num as usize) / self.h_den as usize
    }

    /// Row-aware variant of `scale_col`. When `per_row_scale` is
    /// `Some`, the row at `row_index` uses its own `(num, den)` ratio
    /// instead of the global `h_num/h_den`; otherwise this falls back
    /// to `scale_col`. Renderer call sites use this in place of
    /// `scale_col` so they remain agnostic to the active tier.
    pub fn scale_col_for_row(&self, c: u16, row_index: usize) -> usize {
        match &self.per_row_scale {
            Some(scales) => {
                let (num, den) = scales
                    .get(row_index)
                    .copied()
                    .unwrap_or((self.h_num, self.h_den));
                if den == 0 {
                    return 0;
                }
                (c as usize * num as usize) / den as usize
            },
            None => self.scale_col(c),
        }
    }
}

/// Minimum row count required for the compact tier to engage. 4
/// compact rows × `MIN_ROW_HEIGHT` (2) = 8 rows of keyboard, plus 1
/// row of viewport reserve and 1 row for the top bar. Below this the
/// compact tier returns `None` and the keyboard is suppressed.
const COMPACT_MIN_ROWS: usize = 10;
/// Minimum column count required for the compact tier to engage. The
/// compact block is sized so the row's `cols - 2 * MIN_H_PAD` natural
/// width is at least 10 cells (one cell per Letters R1 key).
const COMPACT_MIN_COLS: usize = 12;
/// The compact tier always lays out 4 KeyRows per layer (top row +
/// two content rows + bottom bar). This drives row-height
/// distribution inside `compute_compact_geometry`.
const COMPACT_ROW_COUNT: usize = 4;

/// Resolve the keyboard's terminal-row / terminal-col extent for the
/// given layout, modifier state, and plugin dimensions.
///
/// Two tiers are tried in priority order:
/// 1. **Natural** — the legacy 5-row (Letters/Symbols) / 4-row
///    (Functions) layout. Wins whenever it fits.
/// 2. **Compact** — a 4-row narrow-screen variant that lays out each
///    row independently to fill the available width. Engages when the
///    natural tier returns `None` but the viewport is still tall and
///    wide enough to host the compact rows.
///
/// Returns `None` only when neither tier fits — the caller suppresses
/// the keyboard for that frame and the viewport reclaims the rows.
pub fn compute_geometry(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    rows: usize,
    cols: usize,
) -> Option<KeyboardGeometry> {
    if let Some(g) = compute_natural_geometry(layout, mods, rows, cols) {
        return Some(g);
    }
    compute_compact_geometry(layout, mods, rows, cols)
}

/// Natural-tier geometry — the legacy `compute_geometry` body,
/// extracted verbatim so its behaviour is pinned by the existing
/// tests. Returns `None` when the 40% budget is smaller than the
/// per-row minimum × the layer's row count (e.g. on a very short
/// screen). The caller falls back to the compact tier when this
/// happens.
pub fn compute_natural_geometry(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    rows: usize,
    cols: usize,
) -> Option<KeyboardGeometry> {
    let key_rows = layout.rows(mods);
    let n = key_rows.len();
    if n == 0 {
        return None;
    }

    let target_total = rows * KEYBOARD_PCT_NUM / KEYBOARD_PCT_DEN;
    let min_total = n * MIN_ROW_HEIGHT;
    if target_total < min_total {
        return None;
    }

    // Distribute target_total across rows: base height for all, +1 for
    // the first `rem` rows. Front-loading the remainder gives the
    // extras strip / top of the keyboard the extra row first, which
    // visually grounds the block (and matches the lowercase letters'
    // natural visual weight at the top).
    let base = target_total / n;
    let rem = target_total % n;
    let row_heights: Vec<u16> = (0..n)
        .map(|i| if i < rem { (base + 1) as u16 } else { base as u16 })
        .collect();

    // Cells stretch as a single ratio `h_num / h_den` so the widest
    // row fills exactly `cols - 2 * MIN_H_PAD` cells — the full
    // available width — while every row preserves its relative
    // proportions (narrower rows like `asdf` stay proportionally
    // narrower; the bottom bar still dominates). Adjacent cells
    // share boundaries because the scaling is applied to absolute
    // positions, not to individual widths.
    //
    // When the natural block exceeds the target width, the natural
    // tier returns `None` instead of falling back to a 1:1 ratio
    // (which would render the keyboard at full natural width and
    // let `clip_buf` truncate the right side). With the compact
    // tier available, suppressing here is the correct answer — the
    // caller will retry through `compute_compact_geometry`, which
    // renders a narrower 4-row layout instead of a clipped 5-row
    // one.
    let natural_block = natural_block_width(&key_rows);
    let target_block = cols.saturating_sub(2 * MIN_H_PAD);
    if natural_block == 0 || target_block < natural_block {
        return None;
    }
    let h_num = target_block as u16;
    let h_den = natural_block as u16;

    Some(KeyboardGeometry { h_num, h_den, row_heights, per_row_scale: None })
}

/// Compact-tier geometry — narrow / short viewports. Engages when
/// the natural tier returns `None` (typically because the 40% budget
/// is smaller than the natural row count × `MIN_ROW_HEIGHT`). Lays
/// out a 4-row block whose total height fits inside the 40% budget
/// and whose width fills `cols - 2 * MIN_H_PAD`.
///
/// Returns `None` when the viewport is too small to host even the
/// compact block (`rows < COMPACT_MIN_ROWS` or `cols < COMPACT_MIN_COLS`).
pub fn compute_compact_geometry(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    rows: usize,
    cols: usize,
) -> Option<KeyboardGeometry> {
    if rows < COMPACT_MIN_ROWS || cols < COMPACT_MIN_COLS {
        return None;
    }

    let target_block = cols.saturating_sub(2 * MIN_H_PAD);
    if target_block == 0 {
        return None;
    }

    let key_rows = layout.compact_rows(mods, target_block as u16);
    let n = key_rows.len();
    if n == 0 {
        return None;
    }

    // The compact tier always lays out the same number of rows
    // regardless of which layer is active, but defensively guard
    // against an alternate-layout implementation that returns a
    // different row count.
    let target_total = rows * KEYBOARD_PCT_NUM / KEYBOARD_PCT_DEN;
    let min_total = n * MIN_ROW_HEIGHT;
    if target_total < min_total {
        return None;
    }

    // Same row-height distribution as the natural tier: a uniform
    // base height with `rem` extra rows front-loaded into the top of
    // the keyboard. With 4 compact rows and a budget of 8..=10 the
    // common shapes are `[2, 2, 2, 2]` and `[3, 2, 2, 2]`.
    let base = target_total / n;
    let rem = target_total % n;
    let row_heights: Vec<u16> = (0..n)
        .map(|i| if i < rem { (base + 1) as u16 } else { base as u16 })
        .collect();

    // Per-row scales come from the layout. Fall back to a uniform
    // `(target_block, COMPACT_NATURAL_BLOCK)` ratio if the layout
    // returned fewer entries than rows — that keeps the renderer
    // sane for layouts whose default `compact_row_scales` returns
    // `Vec::new()`. The fallback uses the widest row's natural
    // extent as the denominator so cells still align with the
    // target width.
    let scales = layout.compact_row_scales(mods, target_block as u16);
    let per_row_scale: Vec<(u16, u16)> = if scales.len() == n {
        scales
    } else {
        let natural_block = natural_block_width(&key_rows);
        let den = (natural_block as u16).max(1);
        vec![(target_block as u16, den); n]
    };

    let _ = COMPACT_ROW_COUNT; // touchpoint for the documented 4-row design
    Some(KeyboardGeometry {
        h_num: target_block as u16,
        h_den: 1,
        row_heights,
        per_row_scale: Some(per_row_scale),
    })
}

/// Widest row in `rows` in *natural* (unscaled) cell units. Used by
/// `compute_geometry` to bound `h_scale` against the available cols.
fn natural_block_width(rows: &[KeyRow]) -> usize {
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

/// Total terminal-rows the keyboard occupies under default (unscaled)
/// geometry — retained as a test-only convenience that pins the
/// natural per-row heights from the layout. Production rendering goes
/// through `compute_geometry` and the returned `total_height()`.
#[cfg(test)]
fn keyboard_rows(layout: &dyn KeyboardLayout, mods: &KeyboardModifiers) -> usize {
    layout.rows(mods).iter().map(|r| r.height as usize).sum()
}

/// Render the keyboard at `(plugin_row_start, 0)` with at most `cols`
/// terminal columns. Pushes a `ClickRegion` per visible cell into
/// `click_regions`. Returns the number of terminal rows actually
/// drawn (matches `geometry.total_height()`).
///
/// `geometry` supplies the horizontal cell-width multiplier and
/// per-row terminal heights; passing
/// `KeyboardGeometry { h_scale: 1, row_heights: [row.height; n] }`
/// reproduces the pre-scaling rendering exactly.
pub fn render_keyboard(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    geometry: &KeyboardGeometry,
    plugin_row_start: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) -> usize {
    // The row source must match what `compute_geometry` used when it
    // built `geometry`: natural-tier geometries are sized against
    // `layout.rows(mods)`, compact-tier geometries against
    // `layout.compact_rows(mods, target_block_width)`. Mismatching the
    // two desyncs `geometry.row_heights` / `per_row_scale` from the
    // rows actually drawn — at narrow widths that produces natural
    // rows clipped on the right plus a runaway bottom bar (no
    // `per_row_scale[4]` entry → fallback to `h_num/h_den = (target,
    // 1)` blows widths up to hundreds of cells).
    let rows = if geometry.per_row_scale.is_some() {
        let target_block = cols.saturating_sub(2 * MIN_H_PAD) as u16;
        layout.compact_rows(mods, target_block)
    } else {
        layout.rows(mods)
    };
    if rows.is_empty() {
        return 0;
    }

    // Disable DECAWM (autowrap) for the duration of the keyboard paint.
    // Mirrors what `render_embedded_viewport` does: the keyboard writes
    // its bottom-most row near `rows - 1`, and `clip_buf` is only a
    // best-effort visible-cell cap — if any escape sequence or wide
    // glyph survives the clip and pushes the cursor past the right
    // edge, autowrap forces the host's plugin-pane grid to scroll,
    // which silently pushes the top bar at row 0 off-screen. With
    // DECAWM off the host's `Grid::add_character`
    // (`zellij-server/src/panes/grid.rs:1925`) drops anything past the
    // right edge — which is what we want for a clipped chrome paint.
    print!("\x1b[?7l");

    // Scaled block width = widest row in the active layer, after
    // applying the `h_num / h_den` ratio to every column extent.
    // Every row in the layer is indented by the same `left_pad` so
    // the keyboard reads as one centered block with a stable left
    // edge; the per-row `offset_col` (e.g. the half-key stagger on
    // Letters row 2) is applied on top of that shared indent and is
    // also scaled.
    let block_width = scaled_block_width(&rows, geometry);
    // Centering pad: bias toward the user-facing minimum so the
    // keyboard never sits flush against col 0 when at least
    // `MIN_H_PAD` of margin fits on both sides. When the scaled
    // block overruns `cols`, `max_left_pad` is 0 and the block
    // simply gets clipped on the right by `clip_buf` further down.
    let raw_pad = cols.saturating_sub(block_width) / 2;
    let max_left_pad = cols.saturating_sub(block_width);
    let left_pad = raw_pad.max(MIN_H_PAD).min(max_left_pad);

    let mut term_row = plugin_row_start;
    for (row_index, row) in rows.iter().enumerate() {
        let palette = palette_for_row(row_index);
        let height = geometry
            .row_heights
            .get(row_index)
            .copied()
            .unwrap_or(row.height as u16) as usize;
        let height = height.max(1);

        // Label sits at the vertically-centered terminal row of the
        // cell rectangle (integer division — height=2 ⇒ bottom row,
        // height=3 ⇒ middle row, height=4 ⇒ row 2 from the top).
        // Padding rows fill every other terminal row of the cell with
        // the same bg shade so the cell reads as one solid block.
        let label_row_offset = height / 2;

        for pad_offset in 0..height {
            if pad_offset == label_row_offset {
                continue;
            }
            render_padding_row(
                layout,
                mods,
                press_flash,
                row,
                row_index,
                palette,
                geometry,
                term_row + pad_offset,
                left_pad,
                cols,
            );
        }

        render_row(
            layout,
            mods,
            press_flash,
            row,
            row_index,
            palette,
            geometry,
            term_row,
            height,
            label_row_offset,
            left_pad,
            cols,
            click_regions,
        );

        term_row += height;
    }

    // Restore autowrap before returning so later chrome paints (top
    // bar / viewport on the next frame) are unaffected. Mirrors the
    // re-enable in `render_embedded_viewport`.
    print!("\x1b[?7h");

    term_row - plugin_row_start
}

/// Widest row in `rows` after applying the geometry's scale ratio.
/// Used as the block width that centers the keyboard horizontally.
/// Returns 0 for an empty `rows` slice. With `h_num == h_den` and
/// `per_row_scale == None` this reproduces the unscaled natural
/// width. With `per_row_scale == Some(_)` each row contributes its
/// own per-row scaled extent — every compact-tier row is constructed
/// to fill the same available width, so the max equals that width.
fn scaled_block_width(rows: &[KeyRow], geometry: &KeyboardGeometry) -> usize {
    rows.iter()
        .enumerate()
        .map(|(row_index, r)| {
            let last_end = r
                .cells
                .last()
                .map(|c| c.col_end)
                .unwrap_or(0);
            let extent = r.offset_col + last_end + r.right_pad;
            geometry.scale_col_for_row(extent, row_index)
        })
        .max()
        .unwrap_or(0)
}

/// Back-compat shim for callers / tests that want the unscaled
/// natural width.
#[cfg(test)]
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
/// terminal row of the cell rectangle; `height` is the cell's
/// vertical extent in terminal rows; `label_row_offset` is the
/// position of the label row inside the cell rectangle (0-based —
/// height/2 vertically centers the label). `left_pad` is the column
/// the row starts at after horizontal centering. Click regions span
/// the full `[cell_top, cell_top + height)` rectangle so taps
/// anywhere in the padding-plus-label rectangle resolve to the cell.
/// Cells are scaled via the geometry's `scale_col` ratio so the row
/// stretches to its share of the available width.
#[allow(clippy::too_many_arguments)]
fn render_row(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    row_index: usize,
    palette: [u8; 2],
    geometry: &KeyboardGeometry,
    cell_top: usize,
    height: usize,
    label_row_offset: usize,
    left_pad: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    let offset = geometry.scale_col_for_row(row.offset_col, row_index);
    let cell_bottom = cell_top + height; // exclusive
    let label_row = cell_top + label_row_offset;

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

    // Scaling absolute positions (rather than per-cell widths) keeps
    // adjacent cells sharing boundaries and lets the fractional
    // remainder absorb naturally across cells in the row.
    for (cell_index, cell) in row.cells.iter().enumerate() {
        let shade = compute_shade(layout, mods, press_flash, cell.id, palette, cell_index);
        let scaled_start = geometry.scale_col_for_row(cell.col_start, row_index);
        let scaled_end = geometry.scale_col_for_row(cell.col_end, row_index);
        let cell_width = scaled_end.saturating_sub(scaled_start);
        let label = layout.label(cell.id, mods);
        let centered = center(label.as_ref(), cell_width);

        buf.push_str(&ansi_bg(shade));
        buf.push_str(&centered);

        // Click region geometry uses absolute viewport columns so the
        // dispatcher can match clicks against viewport columns
        // directly: `left_pad` (centering indent) + scaled `offset`
        // (row stagger) + scaled cell column.
        let abs_start = left_pad + offset + scaled_start;
        let abs_end = left_pad + offset + scaled_end;

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
    // cell's bg. `right_pad` scales with the rest of the row.
    buf.push_str(RESET);
    let scaled_right_pad = geometry.scale_col_for_row(row.right_pad, row_index);
    for _ in 0..scaled_right_pad {
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

/// Paint a bg-only padding row of a tall cell rectangle. Each cell
/// emits its bg colour across its full scaled column range — no
/// label, no click region. The shade matches what `compute_shade`
/// would emit for the label row, so press-flash, armed-modifier, and
/// active-layer inversions paint identically across every row of the
/// cell.
#[allow(clippy::too_many_arguments)]
fn render_padding_row(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    row_index: usize,
    palette: [u8; 2],
    geometry: &KeyboardGeometry,
    term_row: usize,
    left_pad: usize,
    cols: usize,
) {
    let offset = geometry.scale_col_for_row(row.offset_col, row_index);
    let mut buf = String::new();
    for _ in 0..offset {
        buf.push(' ');
    }
    for (cell_index, cell) in row.cells.iter().enumerate() {
        let shade = compute_shade(layout, mods, press_flash, cell.id, palette, cell_index);
        let cell_width = geometry
            .scale_col_for_row(cell.col_end, row_index)
            .saturating_sub(geometry.scale_col_for_row(cell.col_start, row_index));
        buf.push_str(&ansi_bg(shade));
        for _ in 0..cell_width {
            buf.push(' ');
        }
    }
    // Match the label row: drop bg before emitting any `right_pad` so
    // the trailing padding is unstyled, not bg-shaded.
    buf.push_str(RESET);
    let scaled_right_pad = geometry.scale_col_for_row(row.right_pad, row_index);
    for _ in 0..scaled_right_pad {
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

    /// At a phone-portrait baseline (30 rows × 40 cols), the Letters
    /// layer's 40% budget is 12 rows. Spread across 5 KeyRows that is
    /// base=2, remainder=2 → row_heights `[3, 3, 2, 2, 2]` summing to
    /// 12. Horizontal scale is `38 / 34`: the widest row (the bottom
    /// bar at natural 34 cells) stretches to fill the available
    /// `cols - 2*MIN_H_PAD = 38` cells. `per_row_scale` is `None`
    /// because the natural tier never switches into per-row
    /// scaling.
    #[test]
    fn compute_geometry_phone_baseline() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 30, 40).expect("fits");
        assert_eq!(geom.h_num, 38);
        assert_eq!(geom.h_den, 34);
        assert_eq!(geom.row_heights, vec![3, 3, 2, 2, 2]);
        assert_eq!(geom.total_height(), 12);
        assert!(geom.per_row_scale.is_none(), "natural tier must not set per_row_scale");
    }

    /// Pinch-in doubles the grid (60 rows × 80 cols). The 40% budget
    /// becomes 24 rows distributed as `[5, 5, 5, 5, 4]`; the scale
    /// ratio is `78 / 34` so the bottom bar stretches to 78 cells.
    #[test]
    fn compute_geometry_scales_up_on_zoom_in() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 60, 80).expect("fits");
        assert_eq!(geom.h_num, 78);
        assert_eq!(geom.h_den, 34);
        assert_eq!(geom.row_heights, vec![5, 5, 5, 5, 4]);
        assert_eq!(geom.total_height(), 24);
    }

    /// When the screen is too short for the per-row minimum on every
    /// row, the keyboard is suppressed: `compute_geometry` returns
    /// `None` and the caller drops the keyboard from the frame so the
    /// viewport reclaims the rows.
    #[test]
    fn compute_geometry_hides_when_too_short() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        // 8 rows → 40% budget = 3, less than 5 rows × 2 = 10.
        assert!(compute_geometry(&layout, &mods, 8, 40).is_none());
    }

    /// Landscape-style viewport — wide but short. The bottom bar
    /// stretches fractionally (118 / 34) to fill the available width
    /// rather than leaving the slack as side padding. Row heights
    /// stay on the row-budget distribution — horizontal and vertical
    /// axes scale independently.
    #[test]
    fn compute_geometry_widens_keys_on_landscape() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 30, 120).expect("fits");
        assert_eq!(geom.h_num, 118);
        assert_eq!(geom.h_den, 34);
        assert_eq!(geom.row_heights, vec![3, 3, 2, 2, 2]);
        // Widest row stretches to exactly `cols - 2 * MIN_H_PAD = 118`
        // cells — full available width.
        assert_eq!(scaled_block_width(&layout.rows(&mods), &geom), 118);
    }

    /// A tall but narrow screen — the natural block (34 cells)
    /// exceeds the available width (`cols - 2 = 28`), so the natural
    /// tier returns `None` and the compact tier engages instead. The
    /// compact tier always sets `per_row_scale = Some(_)`, which is
    /// the distinguishing marker against the natural tier.
    #[test]
    fn compute_geometry_falls_back_to_compact_when_too_narrow() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        // cols=30 ⇒ target=28 < natural=34. Natural fails; compact
        // takes over with 4 rows.
        let geom = compute_geometry(&layout, &mods, 80, 30).expect("compact fits");
        assert!(
            geom.per_row_scale.is_some(),
            "narrow viewport must route through the compact tier",
        );
        assert_eq!(geom.row_heights.len(), 4);
    }

    /// `total_height` matches the sum of `row_heights` even for
    /// hand-rolled geometries.
    #[test]
    fn geometry_total_height_sums_row_heights() {
        let g = KeyboardGeometry {
            h_num: 2,
            h_den: 1,
            row_heights: vec![3, 4, 5],
            per_row_scale: None,
        };
        assert_eq!(g.total_height(), 12);
    }

    /// `scale_col` preserves cell boundaries: when adjacent cells
    /// share a column boundary at natural position `c`, both map to
    /// the same scaled position so no gaps or overlaps appear.
    #[test]
    fn scale_col_preserves_adjacent_boundaries() {
        let g = KeyboardGeometry {
            h_num: 38,
            h_den: 34,
            row_heights: vec![],
            per_row_scale: None,
        };
        // Cell A's end and cell B's start both at natural col 3 must
        // produce the same scaled col.
        assert_eq!(g.scale_col(3), g.scale_col(3));
        // And the boundary between consecutive cells (col_end of one
        // = col_start of the next) is monotonically non-decreasing.
        let positions: Vec<usize> = (0..=34).map(|c| g.scale_col(c)).collect();
        for w in positions.windows(2) {
            assert!(w[0] <= w[1], "scale_col must be monotone");
        }
    }

    /// Canonical compact-tier engagement: 23 rows × 28 cols (Firefox
    /// Android post-SetConfig resize on a 24-px-font phone). The
    /// natural tier returns `None` because 23 × 2/5 = 9 < 5 × 2 = 10;
    /// the compact tier takes over and lays out 4 rows whose
    /// per-row scale matches the layout's `compact_row_scales`
    /// output. `target_block = 28 - 2 = 26`, equal to
    /// `COMPACT_NATURAL_BLOCK`, so the per-row ratio is `(26, 26)`.
    #[test]
    fn compute_geometry_engages_compact_at_phone_dimensions() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 23, 28).expect("compact fits");
        assert_eq!(geom.row_heights.len(), 4);
        let scales = geom
            .per_row_scale
            .as_ref()
            .expect("compact tier sets per_row_scale");
        assert_eq!(scales.len(), 4);
        for (num, den) in scales {
            assert_eq!((*num, *den), (26, 26));
        }
        // 23 × 2/5 = 9 row budget split across 4 rows yields
        // base=2, rem=1 → `[3, 2, 2, 2]` summing to 9.
        assert_eq!(geom.row_heights, vec![3, 2, 2, 2]);
        assert_eq!(geom.total_height(), 9);
    }

    /// At a viewport size where natural geometry succeeds, the
    /// compact tier must not run — `per_row_scale` must stay `None`.
    /// 30 × 40 is the same canonical phone-portrait baseline used by
    /// `compute_geometry_phone_baseline`.
    #[test]
    fn compute_geometry_prefers_natural_when_both_could_fit() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 30, 40).expect("fits");
        assert!(geom.per_row_scale.is_none());
    }

    /// `compute_geometry` returns `None` when both tiers fail. The
    /// natural tier fails the row-budget check, and the compact tier
    /// fails its width threshold (`cols < COMPACT_MIN_COLS`).
    #[test]
    fn compute_geometry_suppresses_below_compact_threshold() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        // 8 rows × 12 cols: natural fails on rows (8 × 2/5 = 3 < 10),
        // compact fails on rows (8 < COMPACT_MIN_ROWS=10).
        assert!(compute_geometry(&layout, &mods, 8, 12).is_none());
        // 23 rows × 8 cols: natural fails on rows; compact fails on
        // cols (8 < COMPACT_MIN_COLS=12).
        assert!(compute_geometry(&layout, &mods, 23, 8).is_none());
    }

    /// Every compact row's scaled extent equals the target block
    /// width (`cols - 2 * MIN_H_PAD`). This is the architectural
    /// invariant that lets the renderer center the block correctly
    /// — there is no row narrower than the available width.
    #[test]
    fn compact_geometry_rows_fill_available_width() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        let cols = 28usize;
        let target_block = cols - 2 * MIN_H_PAD;
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            mods.layer = layer;
            let geom = compute_compact_geometry(&layout, &mods, 23, cols)
                .expect("compact fits");
            let key_rows = layout.compact_rows(&mods, target_block as u16);
            assert_eq!(key_rows.len(), geom.row_heights.len(), "{:?}", layer);
            for (row_index, row) in key_rows.iter().enumerate() {
                let last_end = row
                    .cells
                    .last()
                    .map(|c| c.col_end)
                    .unwrap_or(0);
                let extent = row.offset_col + last_end + row.right_pad;
                let scaled = geom.scale_col_for_row(extent, row_index);
                assert_eq!(
                    scaled, target_block,
                    "{:?} row {} scaled extent {} != target {}",
                    layer, row_index, scaled, target_block,
                );
            }
        }
    }

    /// `scale_col_for_row` preserves adjacent-cell boundaries for
    /// every row of the compact tier — the row's scaled boundary
    /// sequence is monotone non-decreasing and starts at 0.
    #[test]
    fn compact_geometry_preserves_adjacent_boundaries_per_row() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        let cols = 28usize;
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            mods.layer = layer;
            let geom = compute_compact_geometry(&layout, &mods, 23, cols)
                .expect("compact fits");
            let key_rows = layout.compact_rows(&mods, (cols - 2 * MIN_H_PAD) as u16);
            for (row_index, row) in key_rows.iter().enumerate() {
                let mut last = 0usize;
                for cell in &row.cells {
                    let start = geom.scale_col_for_row(cell.col_start, row_index);
                    let end = geom.scale_col_for_row(cell.col_end, row_index);
                    assert!(start <= end, "{:?} row {} cell {:?} reverses", layer, row_index, cell.id);
                    assert!(
                        start >= last,
                        "{:?} row {} cell {:?} starts before previous end",
                        layer,
                        row_index,
                        cell.id,
                    );
                    last = end;
                }
            }
        }
    }

    /// `render_keyboard` paints labels on the vertically centered
    /// terminal row of each cell rectangle. The slop region's
    /// `center.1` is anchored on the label row, so reading it back is
    /// the most direct way to assert label placement without parsing
    /// the printed ANSI.
    #[test]
    fn label_row_is_vertically_centered() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;
        use crate::state::{ClickAction, ClickRegion};
        use std::collections::HashMap;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        // Force row_heights = [4, 4, 4, 4, 4]: every cell rectangle is
        // 4 terminal rows tall, so the label sits at offset 2 from the
        // top (height/2 = 2).
        let geom = KeyboardGeometry {
            h_num: 1,
            h_den: 1,
            row_heights: vec![4; layout.rows(&mods).len()],
            per_row_scale: None,
        };
        let mut regions: Vec<ClickRegion> = Vec::new();
        let press_flash: HashMap<CellId, std::time::Instant> = HashMap::new();
        let plugin_row_start = 0usize;
        render_keyboard(
            &layout,
            &mods,
            &press_flash,
            &geom,
            plugin_row_start,
            120,
            &mut regions,
        );

        // The first KeyRow occupies term rows [0, 4); its slop centers
        // should all land at row 2 (= height/2).
        let mut found_first_row_center = false;
        for region in &regions {
            if let (Some((_, cy)), ClickAction::Keyboard(_)) = (region.center, &region.action) {
                if region.row_start == 0 && region.row_end > region.row_start {
                    assert_eq!(cy, 2, "label row must be vertically centered at height/2");
                    found_first_row_center = true;
                }
            }
        }
        assert!(found_first_row_center, "no slop region from the first row");
    }

    /// Pins the bug fix where `render_keyboard` used to call
    /// `layout.rows(mods)` (5 natural-tier rows) while the geometry
    /// was sized for `compact_rows(mods, target_block)` (4 rows).
    /// The mismatch produced spilling rows and a giant bottom-bar
    /// painted entirely in the background colour (the 5th row had
    /// no `per_row_scale[4]` entry → fallback to the global
    /// `h_num/h_den`).
    ///
    /// `render_keyboard` returns the number of terminal rows drawn;
    /// for a compact geometry that must equal `geometry.total_height()`.
    /// In the buggy version it returned `total_height + last_row_height`
    /// because it iterated 5 rows over a 4-element row-height vector
    /// and fell through to `row.height` on the 5th.
    #[test]
    fn render_keyboard_uses_compact_rows_under_compact_geometry() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};
        use crate::state::ClickRegion;
        use std::collections::HashMap;

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            mods.layer = layer;
            let geom = compute_compact_geometry(&layout, &mods, 23, 28).expect("compact fits");
            let mut regions: Vec<ClickRegion> = Vec::new();
            let press_flash: HashMap<CellId, std::time::Instant> = HashMap::new();
            let drawn = render_keyboard(
                &layout,
                &mods,
                &press_flash,
                &geom,
                /* plugin_row_start */ 0,
                28,
                &mut regions,
            );
            assert_eq!(
                drawn,
                geom.total_height(),
                "{:?}: render_keyboard returned {} rows, geometry expected {}",
                layer,
                drawn,
                geom.total_height(),
            );
            // Every click region must sit inside the geometry's
            // total height, plus slop. The buggy version produced
            // regions on term rows well beyond `total_height + slop`
            // because it iterated over 5 natural rows on top of the
            // 4-row compact geometry; slop alone only adds one row.
            let max_row_end = geom.total_height() + SLOP_V;
            for region in &regions {
                assert!(
                    region.row_end <= max_row_end,
                    "{:?}: region {:?} extends past total_height+slop {}",
                    layer,
                    region,
                    max_row_end,
                );
            }
        }
    }

    /// Label vertical centering still works under a compact-tier
    /// geometry — the renderer doesn't special-case the natural
    /// path, but the test pins this explicitly so a future renderer
    /// change can't quietly regress it.
    #[test]
    fn label_row_is_vertically_centered_under_compact_geometry() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;
        use crate::state::{ClickAction, ClickRegion};
        use std::collections::HashMap;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_compact_geometry(&layout, &mods, 23, 28).expect("compact fits");
        // The compact tier's row_heights at this size are `[3, 2, 2, 2]`.
        // Row 0 occupies term rows [0, 3); label sits at height/2 = 1.
        let mut regions: Vec<ClickRegion> = Vec::new();
        let press_flash: HashMap<CellId, std::time::Instant> = HashMap::new();
        render_keyboard(&layout, &mods, &press_flash, &geom, 0, 28, &mut regions);
        let mut found_first_row_center = false;
        for region in &regions {
            if let (Some((_, cy)), ClickAction::Keyboard(_)) = (region.center, &region.action) {
                if region.row_start == 0 && region.row_end > region.row_start {
                    assert_eq!(cy, 1, "compact label row must center at height/2 = 1");
                    found_first_row_center = true;
                }
            }
        }
        assert!(
            found_first_row_center,
            "compact tier produced no slop region from the first row",
        );
    }
}
