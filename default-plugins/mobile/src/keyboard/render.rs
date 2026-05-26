//! Bottom modifier bar renderer.
//!
//! One terminal row anchored at the bottom of the plugin area, just
//! above where the OS soft keyboard surfaces. Up to nine labels
//! separated by padded `" | "` pipes: ESC | TAB | CTRL | ALT | ← | ↓
//! | ↑ | → | -.
//!
//! Painted via the host-decoded `Text` API so colours follow the
//! user's palette. The whole row uses `.selected()` for a coherent
//! ribbon; every label and pipe defaults to emphasis-3, and armed
//! CTRL / ALT cells override their label to emphasis-2 so the
//! one-shot modifier state stands out. Only CTRL and ALT are ever
//! shown as active.
//!
//! The bar is responsive: when `cols` is too narrow for the full
//! layout, three degradation axes apply in priority order
//! (separator → labels → cells):
//!   1. shrink the separator from `" | "` (3 cells) to `"|"` (1 cell);
//!   2. shrink the text labels (ESC→ES, TAB→TB, CTRL→CTL, ALT→AL —
//!      arrows and `-` cannot shrink further);
//!   3. drop low-priority cells (first `-`, then `TAB`).
//! `choose_config` walks all 12 (drop × labels × sep) combinations
//! and picks the most-preferred one whose required width fits. When
//! even the most-degraded layout cannot fit, the bar silently
//! no-ops. Each rendered cell pushes one `ClickRegion::tight`; the
//! trailing separator after cell N belongs to cell N's click region,
//! so the bar has no dead pixels within its visible span.

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::state::{ClickAction, ClickRegion};

use super::controller::{
    BAR_CELL_COUNT, CELL_ALT, CELL_CTRL, CELL_DOWN, CELL_ESC, CELL_LEFT, CELL_MINUS, CELL_RIGHT,
    CELL_TAB, CELL_UP,
};
use super::layout::CellId;
use super::modifiers::{KeyboardModifiers, Modifier};

/// Per-cell static metadata.
struct BarCell {
    id: CellId,
    /// The full label used when the bar has room.
    label: &'static str,
    /// Two-cell abbreviation used when `LabelMode::Short` is chosen.
    /// For cells that are already minimal (arrows, `-`), this equals
    /// the full label.
    short_label: &'static str,
    /// `Some(m)` when this cell toggles a modifier — painted with
    /// emphasis-2 (rather than emphasis-3) whenever `m` is armed.
    /// Non-modifier cells never enter the active state.
    modifier: Option<Modifier>,
}

const BAR: [BarCell; BAR_CELL_COUNT] = [
    BarCell { id: CELL_ESC,   label: "ESC",       short_label: "ES",        modifier: None },
    BarCell { id: CELL_TAB,   label: "TAB",       short_label: "TB",        modifier: None },
    BarCell { id: CELL_CTRL,  label: "CTRL",      short_label: "CTL",       modifier: Some(Modifier::Ctrl) },
    BarCell { id: CELL_ALT,   label: "ALT",       short_label: "AL",        modifier: Some(Modifier::Alt) },
    BarCell { id: CELL_LEFT,  label: "\u{2190}",  short_label: "\u{2190}",  modifier: None },
    BarCell { id: CELL_DOWN,  label: "\u{2193}",  short_label: "\u{2193}",  modifier: None },
    BarCell { id: CELL_UP,    label: "\u{2191}",  short_label: "\u{2191}",  modifier: None },
    BarCell { id: CELL_RIGHT, label: "\u{2192}",  short_label: "\u{2192}",  modifier: None },
    BarCell { id: CELL_MINUS, label: "-",         short_label: "-",         modifier: None },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SepMode {
    /// ` | ` — 3 terminal cells per separator.
    Wide,
    /// `|` — 1 terminal cell per separator.
    Compact,
}

impl SepMode {
    fn glyph(self) -> &'static str {
        match self {
            Self::Wide => " | ",
            Self::Compact => "|",
        }
    }
    fn width(self) -> usize {
        match self {
            Self::Wide => 3,
            Self::Compact => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabelMode {
    Full,
    Short,
}

impl LabelMode {
    fn label_for(self, cell: &BarCell) -> &'static str {
        match self {
            Self::Full => cell.label,
            Self::Short => cell.short_label,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DropMode {
    /// Show all nine cells.
    None,
    /// Drop `-` (lowest-priority — explicit dash is rarely needed
    /// on its own; the user can still type `-` via the soft keyboard).
    DropMinus,
    /// Drop `-` and `TAB`. `TAB` is reachable via the soft keyboard
    /// on most platforms but the modifier bar gives a one-tap path
    /// — sacrificed only on the narrowest devices.
    DropMinusAndTab,
}

impl DropMode {
    /// Indices into `BAR` for the visible cells, in display order.
    /// Order is preserved across drops — dropped cells are removed
    /// in place, the rest do not shift.
    fn cell_indices(self) -> &'static [usize] {
        match self {
            Self::None => &[0, 1, 2, 3, 4, 5, 6, 7, 8],
            Self::DropMinus => &[0, 1, 2, 3, 4, 5, 6, 7],
            Self::DropMinusAndTab => &[0, 2, 3, 4, 5, 6, 7],
        }
    }
}

/// Width required to render the given configuration: sum of visible
/// label widths plus all separator cells. The chosen layout
/// stretches to fill any extra cols beyond this minimum by padding
/// each cell.
fn required_width(drop: DropMode, labels: LabelMode, sep: SepMode) -> usize {
    let indices = drop.cell_indices();
    let n = indices.len();
    let labels_sum: usize = indices
        .iter()
        .map(|&i| UnicodeWidthStr::width(labels.label_for(&BAR[i])))
        .sum();
    labels_sum + n.saturating_sub(1) * sep.width()
}

/// Walk the 12 (drop × labels × sep) configurations in
/// user-preferred order — least-degraded first, with the separator
/// degrading before labels, and labels before cells — and return the
/// first one that fits in `cols`. Returns `None` when even the
/// minimum layout (7 cells, short labels, compact separator) does
/// not fit.
fn choose_config(cols: usize) -> Option<(DropMode, LabelMode, SepMode)> {
    const CONFIGS: [(DropMode, LabelMode, SepMode); 12] = [
        (DropMode::None,             LabelMode::Full,  SepMode::Wide),
        (DropMode::None,             LabelMode::Full,  SepMode::Compact),
        (DropMode::None,             LabelMode::Short, SepMode::Wide),
        (DropMode::None,             LabelMode::Short, SepMode::Compact),
        (DropMode::DropMinus,        LabelMode::Full,  SepMode::Wide),
        (DropMode::DropMinus,        LabelMode::Full,  SepMode::Compact),
        (DropMode::DropMinus,        LabelMode::Short, SepMode::Wide),
        (DropMode::DropMinus,        LabelMode::Short, SepMode::Compact),
        (DropMode::DropMinusAndTab,  LabelMode::Full,  SepMode::Wide),
        (DropMode::DropMinusAndTab,  LabelMode::Full,  SepMode::Compact),
        (DropMode::DropMinusAndTab,  LabelMode::Short, SepMode::Wide),
        (DropMode::DropMinusAndTab,  LabelMode::Short, SepMode::Compact),
    ];
    CONFIGS
        .iter()
        .copied()
        .find(|&(d, l, s)| cols >= required_width(d, l, s))
}

/// Paint the modifier bar on `row`, spanning `[0, cols)`. Pushes one
/// `ClickRegion::tight` per visible cell into `click_regions`.
/// Silently no-ops when even the most-degraded layout cannot fit
/// `cols`.
pub fn render_modifier_bar(
    modifiers: &KeyboardModifiers,
    row: usize,
    cols: usize,
    click_regions: &mut Vec<ClickRegion>,
) {
    let (drop_mode, label_mode, sep_mode) = match choose_config(cols) {
        Some(c) => c,
        None => return,
    };

    let indices = drop_mode.cell_indices();
    let n = indices.len();
    let sep_str = sep_mode.glyph();
    let sep_w = sep_mode.width();
    let sep_total = n.saturating_sub(1) * sep_w;
    let content_cols = cols - sep_total;
    // Natural cell-widths for each visible label under the chosen
    // mode. `distribute_widths` guarantees each cell gets *at least*
    // its natural width, so e.g. CTRL (4 cells wide) never gets
    // squeezed down to 3 cells and rendered as "CTR".
    let naturals: Vec<usize> = indices
        .iter()
        .map(|&i| UnicodeWidthStr::width(label_mode.label_for(&BAR[i])))
        .collect();
    let widths = distribute_widths(content_cols, &naturals);

    // Build the bar as one combined string. Track:
    // - char-indexed ranges for each label (used by `color_range`,
    //   which is char-indexed)
    // - char-indexed ranges for each separator (so pipes also paint
    //   in emphasis-3 rather than inheriting the selected-bar fg)
    // - cell-indexed boundaries for click region tiling (each cell's
    //   region includes its trailing separator, so the bar has no
    //   dead pixels).
    let mut bar = String::with_capacity(cols);
    let mut label_ranges: Vec<(std::ops::Range<usize>, bool)> = Vec::with_capacity(n);
    let mut sep_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(n.saturating_sub(1));
    let mut cell_boundaries: Vec<usize> = Vec::with_capacity(n + 1);

    let mut chars_cursor: usize = 0;
    let mut cells_cursor: usize = 0;
    cell_boundaries.push(0);

    for (slot, &cell_idx) in indices.iter().enumerate() {
        let cell = &BAR[cell_idx];
        let label = label_mode.label_for(cell);
        let width = widths[slot];
        let label_w = UnicodeWidthStr::width(label);
        let visible = if label_w <= width {
            label.to_string()
        } else {
            truncate_to_width(label, width)
        };
        let visible_w = UnicodeWidthStr::width(visible.as_str());
        let left_pad = (width - visible_w) / 2;
        let right_pad = width - visible_w - left_pad;

        for _ in 0..left_pad {
            bar.push(' ');
        }
        chars_cursor += left_pad;
        let label_chars_start = chars_cursor;
        bar.push_str(&visible);
        chars_cursor += visible.chars().count();
        let label_chars_end = chars_cursor;
        for _ in 0..right_pad {
            bar.push(' ');
        }
        chars_cursor += right_pad;

        let armed = cell
            .modifier
            .map(|m| modifiers.is_armed(m))
            .unwrap_or(false);
        label_ranges.push((label_chars_start..label_chars_end, armed));

        cells_cursor += width;

        if slot + 1 < n {
            let sep_chars_start = chars_cursor;
            bar.push_str(sep_str);
            chars_cursor += sep_str.chars().count();
            sep_ranges.push(sep_chars_start..chars_cursor);
            cells_cursor += sep_w;
        }

        cell_boundaries.push(cells_cursor);
    }

    // Disjoint ranges by construction: labels never overlap
    // separators, and per-cell emphasis is chosen by the armed flag
    // — no level conflicts.
    let mut text = Text::new(&bar).selected();
    for (range, armed) in &label_ranges {
        let level = if *armed { 2 } else { 3 };
        text = text.color_range(level, range.clone());
    }
    for range in &sep_ranges {
        text = text.color_range(3, range.clone());
    }
    print_text_with_coordinates(text, 0, row, Some(cols), None);

    for (slot, &cell_idx) in indices.iter().enumerate() {
        click_regions.push(ClickRegion::tight(
            row,
            cell_boundaries[slot],
            cell_boundaries[slot + 1],
            ClickAction::Keyboard(BAR[cell_idx].id),
        ));
    }
}

/// Split `cols` into widths whose sum equals `cols`, with each cell
/// receiving *at least* its natural label width (so e.g. CTRL is
/// never squeezed below 4 cells and rendered as "CTR"). Surplus
/// cells beyond `sum(naturals)` are distributed evenly, with the
/// remainder loaded onto the left-most slots.
///
/// Precondition: `cols >= sum(naturals)`. `choose_config` enforces
/// this; widths saturate to the natural minimum on violation.
fn distribute_widths(cols: usize, naturals: &[usize]) -> Vec<usize> {
    let n = naturals.len();
    let natural_sum: usize = naturals.iter().sum();
    let extra = cols.saturating_sub(natural_sum);
    let base = if n > 0 { extra / n } else { 0 };
    let remainder = if n > 0 { extra % n } else { 0 };
    naturals
        .iter()
        .enumerate()
        .map(|(i, &nat)| nat + base + if i < remainder { 1 } else { 0 })
        .collect()
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
        let naturals = vec![1usize; BAR_CELL_COUNT];
        for cols in [9, 10, 20, 80, 137] {
            let widths = distribute_widths(cols, &naturals);
            assert_eq!(widths.iter().sum::<usize>(), cols);
            assert_eq!(widths.len(), BAR_CELL_COUNT);
        }
    }

    #[test]
    fn distribute_widths_left_loaded_remainder() {
        // 10 cols across 9 cells whose naturals are all 1 — extra = 1,
        // left-most slot picks it up.
        let naturals = vec![1usize; 9];
        let widths = distribute_widths(10, &naturals);
        assert_eq!(widths, vec![2, 1, 1, 1, 1, 1, 1, 1, 1]);
    }

    #[test]
    fn distribute_widths_respects_label_minimums() {
        // Full-label naturals: ESC, TAB, CTRL, ALT, ←↓↑→, -.
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        // 27 = 18 (sum of naturals) + 9 extra → every cell +1.
        let widths = distribute_widths(27, &naturals);
        for (i, (&w, &nat)) in widths.iter().zip(naturals.iter()).enumerate() {
            assert!(w >= nat, "cell {} got width {} < natural {}", i, w, nat);
        }
        // CTRL slot must stay at >=4 so "CTRL" is not truncated to "CTR".
        assert!(widths[2] >= 4);
        assert_eq!(widths.iter().sum::<usize>(), 27);
    }

    #[test]
    fn truncate_to_width_drops_overflow() {
        assert_eq!(truncate_to_width("CTRL", 2), "CT");
        assert_eq!(truncate_to_width("ESC", 3), "ESC");
        assert_eq!(truncate_to_width("ESC", 0), "");
    }

    // `UnicodeWidthStr` treats ambiguous-width arrows (← ↓ ↑ →) as
    // single cells, so the label-width sums are:
    //   full  = 3 + 3 + 4 + 3 + 1 + 1 + 1 + 1 + 1 = 18
    //   short = 2 + 2 + 3 + 2 + 1 + 1 + 1 + 1 + 1 = 14
    // Required widths per config (labels + (n-1) * sep_w):
    //   (None,            Full,  Wide)    = 18 + 24 = 42
    //   (None,            Full,  Compact) = 18 +  8 = 26
    //   (None,            Short, Wide)    = 14 + 24 = 38
    //   (None,            Short, Compact) = 14 +  8 = 22
    //   (DropMinus,       Full,  Wide)    = 17 + 21 = 38
    //   (DropMinus,       Full,  Compact) = 17 +  7 = 24
    //   (DropMinus,       Short, Wide)    = 13 + 21 = 34
    //   (DropMinus,       Short, Compact) = 13 +  7 = 20
    //   (DropMinusAndTab, Full,  Wide)    = 14 + 18 = 32
    //   (DropMinusAndTab, Full,  Compact) = 14 +  6 = 20
    //   (DropMinusAndTab, Short, Wide)    = 11 + 18 = 29
    //   (DropMinusAndTab, Short, Compact) = 11 +  6 = 17

    #[test]
    fn choose_config_picks_full_layout_when_wide() {
        assert_eq!(
            choose_config(42),
            Some((DropMode::None, LabelMode::Full, SepMode::Wide))
        );
        assert_eq!(
            choose_config(80),
            Some((DropMode::None, LabelMode::Full, SepMode::Wide))
        );
    }

    #[test]
    fn choose_config_degrades_separator_before_labels() {
        // 41 is below the Wide threshold (42) but above the
        // Full/Compact threshold (26).
        assert_eq!(
            choose_config(41),
            Some((DropMode::None, LabelMode::Full, SepMode::Compact))
        );
        assert_eq!(
            choose_config(26),
            Some((DropMode::None, LabelMode::Full, SepMode::Compact))
        );
    }

    #[test]
    fn choose_config_degrades_labels_before_dropping_cells() {
        // 25 is below Full/Compact (26) but above Short/Compact (22).
        assert_eq!(
            choose_config(25),
            Some((DropMode::None, LabelMode::Short, SepMode::Compact))
        );
        assert_eq!(
            choose_config(22),
            Some((DropMode::None, LabelMode::Short, SepMode::Compact))
        );
    }

    #[test]
    fn choose_config_drops_minus_when_short_compact_falls_short() {
        // 21 is below (None, Short, Compact) (22) so MINUS drops.
        assert_eq!(
            choose_config(21),
            Some((DropMode::DropMinus, LabelMode::Short, SepMode::Compact))
        );
        assert_eq!(
            choose_config(20),
            Some((DropMode::DropMinus, LabelMode::Short, SepMode::Compact))
        );
    }

    #[test]
    fn choose_config_drops_minus_and_tab_at_minimum() {
        // 19 is below (DropMinus, Short, Compact) (20) so TAB drops too.
        assert_eq!(
            choose_config(19),
            Some((DropMode::DropMinusAndTab, LabelMode::Short, SepMode::Compact))
        );
        assert_eq!(
            choose_config(17),
            Some((DropMode::DropMinusAndTab, LabelMode::Short, SepMode::Compact))
        );
    }

    #[test]
    fn choose_config_none_when_too_narrow() {
        assert_eq!(choose_config(16), None);
        assert_eq!(choose_config(0), None);
    }

    #[test]
    fn render_modifier_bar_pushes_one_region_per_cell() {
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 5, 90, &mut regions);
        assert_eq!(regions.len(), BAR_CELL_COUNT);
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
    fn render_modifier_bar_tiles_at_compact_width() {
        // 30 cols → all 9 cells, compact separators. Regions still
        // tile [0, cols).
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 0, 30, &mut regions);
        assert_eq!(regions.len(), BAR_CELL_COUNT);
        regions.sort_by_key(|r| r.col_start);
        let mut cursor = 0usize;
        for r in &regions {
            assert_eq!(r.col_start, cursor);
            cursor = r.col_end;
        }
        assert_eq!(cursor, 30);
    }

    #[test]
    fn render_modifier_bar_drops_minus_at_narrow() {
        // 21 cols → DropMinus + Short + Compact (needs 20). 8 cells.
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 0, 21, &mut regions);
        assert_eq!(regions.len(), 8);
        for r in &regions {
            if let ClickAction::Keyboard(id) = r.action {
                assert_ne!(id, CELL_MINUS);
            }
        }
    }

    #[test]
    fn render_modifier_bar_drops_minus_and_tab_at_minimum() {
        // 17 cols → DropMinusAndTab + Short + Compact (needs 17). 7 cells.
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 0, 17, &mut regions);
        assert_eq!(regions.len(), 7);
        for r in &regions {
            if let ClickAction::Keyboard(id) = r.action {
                assert_ne!(id, CELL_MINUS);
                assert_ne!(id, CELL_TAB);
            }
        }
    }

    #[test]
    fn render_modifier_bar_noop_when_too_narrow() {
        // Below the minimum (17 cols).
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 0, 16, &mut regions);
        assert!(regions.is_empty());
    }
}
