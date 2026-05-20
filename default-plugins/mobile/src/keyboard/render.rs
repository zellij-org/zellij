//! Generic, layout-agnostic keyboard renderer.
//!
//! Reads `&dyn KeyboardLayout` and renders into the plugin's ANSI
//! stream via `print!`. Cells render as contiguous blocks of ANSI
//! background colour — no box-drawing characters. Cell separation is
//! purely visual contrast between adjacent backgrounds.
//!
//! Under Variant B, the layout produces cells already sized to the
//! requested `target_block_width` — the renderer does no scaling. It
//! iterates rows and draws each cell at its `col_start..col_end`
//! position relative to the row's `offset_col`.
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
/// bottom bar on Letters/Symbols layers).
const P_OUTER: [u8; 2] = [236, 240];
/// Palette pair for "inner" rows (letter row 2).
const P_INNER: [u8; 2] = [238, 242];
/// Bright blue used for armed-modifier cells, active-layer cells, and
/// the transient press-flash highlight.
const ACTIVE: u8 = 33;

/// Horizontal hit-slop, in terminal columns.
const SLOP_H: usize = 1;
/// Vertical hit-slop, in terminal rows.
const SLOP_V: usize = 1;

/// Numerator / denominator of the keyboard's target row footprint.
/// 2/5 = 40%.
const KEYBOARD_PCT_NUM: usize = 2;
const KEYBOARD_PCT_DEN: usize = 5;

/// Per-KeyRow minimum height in terminal rows.
const MIN_ROW_HEIGHT: usize = 2;

/// Minimum row count required for the compact tier to engage.
const COMPACT_MIN_ROWS: usize = 10;
/// Minimum column count required for the compact tier to engage.
const COMPACT_MIN_COLS: usize = 12;
/// The compact tier always lays out 4 KeyRows per layer.
const COMPACT_ROW_COUNT: usize = 4;
/// The natural tier lays out 5 KeyRows on Letters / Symbols and 4 on
/// Functions; the largest layer's row count drives the row budget.
const NATURAL_MAX_ROWS: usize = 5;

/// Which sizing tier the geometry was computed under. The renderer
/// uses this to decide whether to call `layout.rows()` or
/// `layout.compact_rows()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Natural,
    Compact,
}

/// Resolved keyboard geometry for the current frame.
///
/// Under Variant B the layout pre-sizes each row's cells to fill
/// `target_block_width` — so the geometry only needs to carry the
/// tier (used by `render_keyboard` to pick the row source) and the
/// row-height distribution (used to vertically inflate the keyboard
/// up to its 40% footprint).
#[derive(Debug, Clone)]
pub struct KeyboardGeometry {
    pub tier: Tier,
    pub row_heights: Vec<u16>,
    pub target_block_width: u16,
}

impl KeyboardGeometry {
    /// Sum of `row_heights` — the number of terminal rows the keyboard
    /// occupies under this geometry.
    pub fn total_height(&self) -> usize {
        self.row_heights.iter().map(|h| *h as usize).sum()
    }
}

/// Resolve the keyboard's terminal-row / terminal-col extent for the
/// given layout, modifier state, and plugin dimensions.
///
/// Two tiers are tried in priority order:
/// 1. **Natural** — 5 rows on Letters / Symbols. Wins when the row
///    budget is large enough for `NATURAL_MAX_ROWS * MIN_ROW_HEIGHT`.
/// 2. **Compact** — 4 rows per layer. Engages when natural doesn't
///    fit but the viewport still has room for `COMPACT_ROW_COUNT *
///    MIN_ROW_HEIGHT` rows and `COMPACT_MIN_COLS` cols.
///
/// Returns `None` when neither tier fits — the caller suppresses
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

/// Natural-tier geometry — engages when the row budget can host the
/// 5-row natural layout at the per-row minimum and the viewport is
/// wide enough that the layout's cells get at least 1 col each.
pub fn compute_natural_geometry(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    rows: usize,
    cols: usize,
) -> Option<KeyboardGeometry> {
    if cols < 13 {
        // Symbols rows have 13 cells; below this width cells would
        // round to width 0. The compact tier picks this up.
        return None;
    }
    let target_block = cols as u16;
    let key_rows = layout.rows(mods, target_block);
    let n = key_rows.len();
    if n == 0 {
        return None;
    }
    let target_total = rows * KEYBOARD_PCT_NUM / KEYBOARD_PCT_DEN;
    // Use the maximum row count across layers so the row-budget check
    // is layer-independent (Letters/Symbols have 5 rows; Functions has
    // 4 — picking the max guarantees the budget fits any layer the
    // user might switch to without re-running compute_geometry).
    let max_rows = NATURAL_MAX_ROWS.max(n);
    let min_total = max_rows * MIN_ROW_HEIGHT;
    if target_total < min_total {
        return None;
    }
    let row_heights = distribute_row_heights(target_total, n);
    Some(KeyboardGeometry {
        tier: Tier::Natural,
        row_heights,
        target_block_width: target_block,
    })
}

/// Compact-tier geometry — engages on narrow / short viewports where
/// the natural tier returns `None`.
pub fn compute_compact_geometry(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    rows: usize,
    cols: usize,
) -> Option<KeyboardGeometry> {
    if rows < COMPACT_MIN_ROWS || cols < COMPACT_MIN_COLS {
        return None;
    }
    let target_block = cols as u16;
    let key_rows = layout.compact_rows(mods, target_block);
    let n = key_rows.len();
    if n == 0 {
        return None;
    }
    let target_total = rows * KEYBOARD_PCT_NUM / KEYBOARD_PCT_DEN;
    let min_total = n * MIN_ROW_HEIGHT;
    if target_total < min_total {
        return None;
    }
    let row_heights = distribute_row_heights(target_total, n);
    let _ = COMPACT_ROW_COUNT; // documented invariant: compact has 4 rows
    Some(KeyboardGeometry {
        tier: Tier::Compact,
        row_heights,
        target_block_width: target_block,
    })
}

/// Spread `target_total` row units across `n` rows, putting the
/// extras at the front so the top of the keyboard grows first.
fn distribute_row_heights(target_total: usize, n: usize) -> Vec<u16> {
    let base = target_total / n;
    let rem = target_total % n;
    (0..n)
        .map(|i| if i < rem { (base + 1) as u16 } else { base as u16 })
        .collect()
}

/// Render the keyboard at `(plugin_row_start, 0)` with at most `cols`
/// terminal columns. Pushes a `ClickRegion` per visible cell into
/// `click_regions`. Returns the number of terminal rows actually
/// drawn (matches `geometry.total_height()`).
pub fn render_keyboard(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    geometry: &KeyboardGeometry,
    plugin_row_start: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) -> usize {
    let rows = match geometry.tier {
        Tier::Natural => layout.rows(mods, geometry.target_block_width),
        Tier::Compact => layout.compact_rows(mods, geometry.target_block_width),
    };
    if rows.is_empty() {
        return 0;
    }

    // DECAWM off for the duration of the keyboard paint — mirrors the
    // embedded-viewport paint guarantee.
    print!("\x1b[?7l");

    // The widest row drives horizontal centering. Under Variant B
    // every row is built to span `target_block_width`, so this is
    // typically just `target_block_width` itself, but we still
    // compute it from the actual rows in case some row carries a
    // smaller extent (e.g. a future "doesn't quite fit" branch).
    let block_width = block_width_of(&rows);
    let left_pad = cols.saturating_sub(block_width) / 2;

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
        // cell rectangle.
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
                palette,
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
            palette,
            term_row,
            height,
            label_row_offset,
            left_pad,
            cols,
            click_regions,
        );

        term_row += height;
    }

    print!("\x1b[?7h");

    term_row - plugin_row_start
}

/// Widest row in `rows` measured in terminal cells (absolute extent
/// including `offset_col` and `right_pad`).
fn block_width_of(rows: &[KeyRow]) -> usize {
    rows.iter()
        .map(|r| {
            let last_end = r.cells.last().map(|c| c.col_end as usize).unwrap_or(0);
            r.offset_col as usize + last_end + r.right_pad as usize
        })
        .max()
        .unwrap_or(0)
}

/// Pure alternation: even rows get `P_OUTER`, odd rows get `P_INNER`.
fn palette_for_row(row_index: usize) -> [u8; 2] {
    if row_index % 2 == 0 {
        P_OUTER
    } else {
        P_INNER
    }
}

#[allow(clippy::too_many_arguments)]
fn render_row(
    layout: &dyn KeyboardLayout,
    mods: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: &KeyRow,
    palette: [u8; 2],
    cell_top: usize,
    height: usize,
    label_row_offset: usize,
    left_pad: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    let offset = row.offset_col as usize;
    let cell_bottom = cell_top + height;
    let label_row = cell_top + label_row_offset;

    let mut buf = String::new();
    for _ in 0..offset {
        buf.push(' ');
    }

    for (cell_index, cell) in row.cells.iter().enumerate() {
        let shade = compute_shade(layout, mods, press_flash, cell.id, palette, cell_index);
        let cell_width = cell.col_end.saturating_sub(cell.col_start) as usize;
        let label = layout.label(cell.id, mods);
        let centered = center(label.as_ref(), cell_width);

        buf.push_str(&ansi_bg(shade));
        buf.push_str(&centered);

        let abs_start = left_pad + offset + cell.col_start as usize;
        let abs_end = left_pad + offset + cell.col_end as usize;

        click_regions.push(ClickRegion::tight_range(
            cell_top,
            cell_bottom,
            abs_start,
            abs_end,
            ClickAction::Keyboard(cell.id),
        ));

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

#[allow(clippy::too_many_arguments)]
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
        let cell_width = cell.col_end.saturating_sub(cell.col_start) as usize;
        buf.push_str(&ansi_bg(shade));
        for _ in 0..cell_width {
            buf.push(' ');
        }
    }
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

fn ansi_bg(index: u8) -> String {
    format!("\x1b[48;5;{}m", index)
}

fn center(label: &str, width: usize) -> String {
    let label_width = UnicodeWidthStr::width(label);
    if label_width >= width {
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
        assert_eq!(center("⌫", 7), "   ⌫   ");
    }

    #[test]
    fn center_asymmetric_extra_goes_right() {
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
    }

    #[test]
    fn ansi_bg_format() {
        assert_eq!(ansi_bg(236), "\x1b[48;5;236m");
        assert_eq!(ansi_bg(33), "\x1b[48;5;33m");
    }

    /// At phone-portrait baseline (30 rows × 40 cols), the Letters
    /// layer's 40% budget is 12 rows. Spread across 5 KeyRows that's
    /// base=2, remainder=2 → row_heights `[3, 3, 2, 2, 2]` summing to
    /// 12. Geometry is natural with target_block_width = 40.
    #[test]
    fn compute_geometry_phone_baseline() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 30, 40).expect("fits");
        assert_eq!(geom.tier, Tier::Natural);
        assert_eq!(geom.target_block_width, 40);
        assert_eq!(geom.row_heights, vec![3, 3, 2, 2, 2]);
        assert_eq!(geom.total_height(), 12);
    }

    /// Pinch-in doubles the grid (60 rows × 80 cols). The 40% budget
    /// becomes 24 rows distributed as `[5, 5, 5, 5, 4]`; geometry is
    /// natural with target = 80.
    #[test]
    fn compute_geometry_scales_up_on_zoom_in() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 60, 80).expect("fits");
        assert_eq!(geom.tier, Tier::Natural);
        assert_eq!(geom.target_block_width, 80);
        assert_eq!(geom.row_heights, vec![5, 5, 5, 5, 4]);
        assert_eq!(geom.total_height(), 24);
    }

    /// When the screen is too short for the per-row minimum on every
    /// row, both tiers return `None`.
    #[test]
    fn compute_geometry_hides_when_too_short() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        assert!(compute_geometry(&layout, &mods, 8, 40).is_none());
    }

    /// Compact tier engages at canonical phone dimensions (23×28).
    #[test]
    fn compute_geometry_engages_compact_at_phone_dimensions() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 23, 28).expect("compact fits");
        assert_eq!(geom.tier, Tier::Compact);
        assert_eq!(geom.target_block_width, 28);
        assert_eq!(geom.row_heights.len(), 4);
        // 23 * 2/5 = 9 row budget split across 4 rows: base=2, rem=1.
        assert_eq!(geom.row_heights, vec![3, 2, 2, 2]);
    }

    /// At a viewport size where natural geometry succeeds, the
    /// compact tier must not run.
    #[test]
    fn compute_geometry_prefers_natural_when_both_could_fit() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = compute_geometry(&layout, &mods, 30, 40).expect("fits");
        assert_eq!(geom.tier, Tier::Natural);
    }

    /// `compute_geometry` returns `None` when both tiers fail.
    #[test]
    fn compute_geometry_suppresses_below_compact_threshold() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        // 8 rows × 12 cols: natural fails on rows; compact fails on rows.
        assert!(compute_geometry(&layout, &mods, 8, 12).is_none());
        // 23 rows × 8 cols: natural fails on cols; compact fails on cols.
        assert!(compute_geometry(&layout, &mods, 23, 8).is_none());
    }

    /// `render_keyboard` paints labels on the vertically centered
    /// terminal row of each cell rectangle.
    #[test]
    fn label_row_is_vertically_centered() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::KeyboardModifiers;
        use crate::state::{ClickAction, ClickRegion};
        use std::collections::HashMap;

        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let geom = KeyboardGeometry {
            tier: Tier::Natural,
            row_heights: vec![4; layout.rows(&mods, 120).len()],
            target_block_width: 120,
        };
        let mut regions: Vec<ClickRegion> = Vec::new();
        let press_flash: HashMap<CellId, std::time::Instant> = HashMap::new();
        render_keyboard(&layout, &mods, &press_flash, &geom, 0, 120, &mut regions);

        let mut found_first_row_center = false;
        for region in &regions {
            if let (Some((_, cy)), ClickAction::Keyboard(_)) = (region.center, &region.action) {
                if region.row_start == 0 && region.row_end > region.row_start {
                    assert_eq!(cy, 2, "label row must center at height/2");
                    found_first_row_center = true;
                }
            }
        }
        assert!(found_first_row_center, "no slop region from the first row");
    }

    /// `render_keyboard` returns the number of terminal rows drawn;
    /// must equal `geometry.total_height()` (regression: the old
    /// per_row_scale dispatch could iterate too many rows).
    #[test]
    fn render_keyboard_returns_geometry_total_height() {
        use crate::keyboard::layouts::us_qwerty::UsQwerty;
        use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers};
        use crate::state::ClickRegion;
        use std::collections::HashMap;

        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        for tier_cols in [(Tier::Natural, 40), (Tier::Compact, 28)] {
            let (tier, cols) = tier_cols;
            for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
                mods.layer = layer;
                let geom = match tier {
                    Tier::Natural => compute_natural_geometry(&layout, &mods, 30, cols),
                    Tier::Compact => compute_compact_geometry(&layout, &mods, 23, cols),
                }
                .expect("fits");
                let mut regions: Vec<ClickRegion> = Vec::new();
                let press_flash: HashMap<CellId, std::time::Instant> = HashMap::new();
                let drawn = render_keyboard(
                    &layout,
                    &mods,
                    &press_flash,
                    &geom,
                    0,
                    cols,
                    &mut regions,
                );
                assert_eq!(drawn, geom.total_height(), "{:?} {:?}", tier, layer);
            }
        }
    }

    #[test]
    fn geometry_total_height_sums_row_heights() {
        let g = KeyboardGeometry {
            tier: Tier::Natural,
            row_heights: vec![3, 4, 5],
            target_block_width: 40,
        };
        assert_eq!(g.total_height(), 12);
    }
}
