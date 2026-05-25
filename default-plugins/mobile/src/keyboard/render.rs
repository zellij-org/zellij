//! Bottom modifier bar renderer.
//!
//! One terminal row anchored at the bottom of the plugin area, just
//! above where the OS soft keyboard surfaces. Nine cells, evenly
//! distributed across `cols`: ESC, TAB, CTRL, ALT, ←, ↓, ↑, →, -.
//!
//! Cells render as contiguous blocks of ANSI background colour — no
//! box-drawing characters. Adjacent cells alternate between two shades
//! for visual separation. Modifier cells (CTRL/ALT) paint in the
//! ACTIVE colour when armed; any cell paints ACTIVE briefly on the
//! press-flash. Each cell is also pushed into `click_regions` so taps
//! route through the standard dispatch.

use std::collections::HashMap;
use std::time::Instant;

use unicode_width::UnicodeWidthStr;

use crate::state::{ClickAction, ClickRegion};

use super::controller::{
    BAR_CELL_COUNT, CELL_ALT, CELL_CTRL, CELL_DOWN, CELL_ESC, CELL_LEFT, CELL_MINUS, CELL_RIGHT,
    CELL_TAB, CELL_UP, KEY_FEEDBACK_MS,
};
use super::layout::CellId;
use super::modifiers::{KeyboardModifiers, Modifier};

const RESET: &str = "\x1b[0m";

/// Two-shade palette for adjacent cells. Same darks the prior
/// keyboard used so the bar visually blends with the plugin chrome.
const BG_DARK: u8 = 236;
const BG_LIGHT: u8 = 240;
/// Bright blue used for armed-modifier cells and the press-flash
/// highlight. Matches the prior on-screen keyboard's ACTIVE colour.
const BG_ACTIVE: u8 = 33;
/// Foreground for inactive labels — bright grey on dark bg.
const FG_LABEL: u8 = 250;
/// Foreground for active / flashed labels — near-black on the bright
/// ACTIVE bg for contrast.
const FG_ACTIVE: u8 = 16;

/// Move the cursor to (row, col), 1-based as ANSI expects. The plugin
/// render area is 0-based, so we add 1 here.
fn move_to(row: usize, col: usize) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

/// Per-cell static metadata. Position in the array maps to the cell
/// index 0..9 used for column distribution.
struct BarCell {
    id: CellId,
    label: &'static str,
    /// `Some(m)` when this cell toggles a modifier — painted ACTIVE
    /// whenever `m` is armed.
    modifier: Option<Modifier>,
}

const BAR: [BarCell; BAR_CELL_COUNT] = [
    BarCell { id: CELL_ESC,   label: "ESC",  modifier: None },
    BarCell { id: CELL_TAB,   label: "TAB",  modifier: None },
    BarCell { id: CELL_CTRL,  label: "CTRL", modifier: Some(Modifier::Ctrl) },
    BarCell { id: CELL_ALT,   label: "ALT",  modifier: Some(Modifier::Alt) },
    BarCell { id: CELL_LEFT,  label: "\u{2190}", modifier: None },
    BarCell { id: CELL_DOWN,  label: "\u{2193}", modifier: None },
    BarCell { id: CELL_UP,    label: "\u{2191}", modifier: None },
    BarCell { id: CELL_RIGHT, label: "\u{2192}", modifier: None },
    BarCell { id: CELL_MINUS, label: "-",   modifier: None },
];

/// Paint the modifier bar on `row`, spanning `[0, cols)`. Pushes one
/// `ClickRegion::tight` per cell into `click_regions`. Silently no-ops
/// if `cols < BAR_CELL_COUNT` (each cell would be zero-wide).
pub fn render_modifier_bar(
    modifiers: &KeyboardModifiers,
    press_flash: &HashMap<CellId, Instant>,
    row: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    if cols < BAR_CELL_COUNT {
        return;
    }

    let widths = distribute_widths(cols, BAR_CELL_COUNT);
    let now = Instant::now();

    let mut col_start = 0usize;
    for (i, cell) in BAR.iter().enumerate() {
        let width = widths[i];
        let col_end = col_start + width;

        let armed = cell
            .modifier
            .map(|m| modifiers.is_armed(m))
            .unwrap_or(false);
        let flashing = press_flash
            .get(&cell.id)
            .map(|t| now.saturating_duration_since(*t).as_millis() < KEY_FEEDBACK_MS)
            .unwrap_or(false);
        let active = armed || flashing;

        let bg = if active {
            BG_ACTIVE
        } else if i % 2 == 0 {
            BG_DARK
        } else {
            BG_LIGHT
        };
        let fg = if active { FG_ACTIVE } else { FG_LABEL };

        paint_cell(row, col_start, width, cell.label, bg, fg);

        click_regions.push(ClickRegion::tight(
            row,
            col_start,
            col_end,
            ClickAction::Keyboard(cell.id),
        ));

        col_start = col_end;
    }
}

/// Split `cols` into `n` widths whose sum is exactly `cols` — the
/// remainder cells are distributed across the left-most slots so the
/// bar fills the row without truncation.
fn distribute_widths(cols: usize, n: usize) -> Vec<usize> {
    let base = cols / n;
    let extra = cols % n;
    (0..n).map(|i| if i < extra { base + 1 } else { base }).collect()
}

/// Paint a single cell: bg fill across `[col_start, col_start+width)`
/// at `row`, with `label` centered horizontally. Truncates the label
/// with no ellipsis when it doesn't fit — bar cells are tight enough
/// already that `…` would steal the only meaningful character.
fn paint_cell(row: usize, col_start: usize, width: usize, label: &str, bg: u8, fg: u8) {
    if width == 0 {
        return;
    }
    let label_w = UnicodeWidthStr::width(label);
    let visible = if label_w <= width {
        label.to_string()
    } else {
        truncate_to_width(label, width)
    };
    let visible_w = UnicodeWidthStr::width(visible.as_str());
    let left_pad = (width - visible_w) / 2;
    let right_pad = width - visible_w - left_pad;

    print!(
        "{}{}\x1b[48;5;{}m\x1b[38;5;{}m{}{}{}{}",
        RESET,
        move_to(row, col_start),
        bg,
        fg,
        " ".repeat(left_pad),
        visible,
        " ".repeat(right_pad),
        RESET,
    );
}

/// Truncate `label` so its cell-width is at most `max_w`. Drops
/// trailing characters (and any zero-width marks they carry) one at a
/// time. Used only when a cell is narrower than its label.
fn truncate_to_width(label: &str, max_w: usize) -> String {
    let mut out = String::new();
    let mut w = 0usize;
    for ch in label.chars() {
        let mut buf = [0u8; 4];
        let ch_str = ch.encode_utf8(&mut buf);
        let ch_w = UnicodeWidthStr::width(ch_str as &str);
        if w + ch_w > max_w {
            break;
        }
        out.push(ch);
        w += ch_w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribute_widths_sums_to_cols() {
        for cols in [9, 10, 20, 80, 137] {
            let widths = distribute_widths(cols, BAR_CELL_COUNT);
            assert_eq!(widths.iter().sum::<usize>(), cols);
            assert_eq!(widths.len(), BAR_CELL_COUNT);
        }
    }

    #[test]
    fn distribute_widths_left_loaded_remainder() {
        // 10 / 9 = 1 base, 1 extra → first cell wider than the rest.
        let widths = distribute_widths(10, 9);
        assert_eq!(widths, vec![2, 1, 1, 1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn truncate_to_width_drops_overflow() {
        assert_eq!(truncate_to_width("CTRL", 2), "CT");
        assert_eq!(truncate_to_width("ESC", 3), "ESC");
        assert_eq!(truncate_to_width("ESC", 0), "");
    }

    #[test]
    fn render_modifier_bar_pushes_one_region_per_cell() {
        let mods = KeyboardModifiers::default();
        let flash = HashMap::new();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, &flash, 5, 90, &mut regions);
        assert_eq!(regions.len(), BAR_CELL_COUNT);
        // Regions must tile `[0, cols)` without gaps or overlap.
        regions.sort_by_key(|r| r.col_start);
        let mut cursor = 0usize;
        for r in &regions {
            assert_eq!(r.row_start, 5);
            assert_eq!(r.row_end, 6);
            assert_eq!(r.col_start, cursor);
            cursor = r.col_end;
        }
        assert_eq!(cursor, 90);
    }

    #[test]
    fn render_modifier_bar_noop_when_too_narrow() {
        let mods = KeyboardModifiers::default();
        let flash = HashMap::new();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, &flash, 0, 5, &mut regions);
        assert!(regions.is_empty());
    }
}
