//! Bottom modifier bar. A single-row strip of nine fixed cells
//! (ESC, TAB, CTRL, ALT, ←, ↓, ↑, →, -) painted at the bottom of the
//! plugin area, just above where the OS soft keyboard surfaces.
//!
//! The bar provides the keys the native mobile keyboard does not —
//! everything else (letters, digits, punctuation) is typed on the
//! native keyboard and routed straight to the focused pane via
//! `installSoftKeyboardCapture()` in `zellij-client/assets/input.js`.
//!
//! This file owns the whole component: the one-shot modifier state
//! (`KeyboardModifiers`), the cell/action tokens (`CellId` / `KeyAction`),
//! the tap state machine (`ModifierBarController`), and the responsive
//! renderer (`render_modifier_bar`).

use std::collections::BTreeSet;

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::keys;

// ===========================================================================
// Modifier state
// ===========================================================================
//
// Ctrl / Alt are **one-shot** sticky modifiers — tapping the modifier
// cell arms the flag, the next `SendKey` consumes it. The controller
// calls `consume_one_shots` after every emitted key.
//
// Shift is intentionally absent — the bar has no shift key. The native
// OS keyboard handles letter case directly via its own shift glyph.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    /// One-shot. Aliased to `State::ctrl_held` so hardware-key
    /// passthrough and the modifier bar share the same armed state —
    /// arming Ctrl on the bar carries through to a hardware-tapped
    /// follow-up, and vice versa.
    pub ctrl_armed: bool,
    /// One-shot. Aliased to `State::alt_held` (see `ctrl_armed`).
    pub alt_armed: bool,
}

impl KeyboardModifiers {
    /// Drop the one-shot mods. Called by the controller after
    /// emitting a `SendKey`, so a `Ctrl Right` sequence sends
    /// `Ctrl+Right` and the next tap goes through with no modifier.
    pub fn consume_one_shots(&mut self) {
        self.ctrl_armed = false;
        self.alt_armed = false;
    }

    pub fn is_armed(&self, m: Modifier) -> bool {
        match m {
            Modifier::Ctrl => self.ctrl_armed,
            Modifier::Alt => self.alt_armed,
        }
    }

    pub fn toggle(&mut self, m: Modifier) {
        match m {
            Modifier::Ctrl => self.ctrl_armed = !self.ctrl_armed,
            Modifier::Alt => self.alt_armed = !self.alt_armed,
        }
    }
}

// ===========================================================================
// Cell + action tokens
// ===========================================================================
//
// `CellId` is an opaque per-cell token round-tripped through the
// click-region map. `KeyAction` is what `ModifierBarController::handle_tap`
// produces after looking up the cell — the dispatcher acts on it.

/// Opaque cell identifier. The renderer assigns these to bar cells;
/// the controller maps each back to a `KeyAction` in `handle_tap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub u16);

/// What a tap on a cell resolves to.
#[derive(Debug, Clone)]
pub enum KeyAction {
    SendKey(KeyWithModifier),
    ToggleModifier(Modifier),
    NoOp,
}

// ===========================================================================
// Tap controller
// ===========================================================================
//
// Owns the one-shot modifier flags. `handle_tap` is the single entry
// point — the dispatcher routes every bar click here and acts on the
// returned `TapOutcome`.

// Cell identifiers for the nine bar cells. The renderer assigns
// these positions left-to-right; the controller's `cell_action`
// match maps each back to the action it represents.
pub const CELL_ESC: CellId = CellId(0);
pub const CELL_TAB: CellId = CellId(1);
pub const CELL_CTRL: CellId = CellId(2);
pub const CELL_ALT: CellId = CellId(3);
pub const CELL_LEFT: CellId = CellId(4);
pub const CELL_DOWN: CellId = CellId(5);
pub const CELL_UP: CellId = CellId(6);
pub const CELL_RIGHT: CellId = CellId(7);
pub const CELL_MINUS: CellId = CellId(8);
pub const BAR_CELL_COUNT: usize = 9;

/// What the dispatcher should do after a tap. The controller never
/// performs IO itself — the caller wires the outcome up to
/// `write_to_pane_id`, `set_timeout`, …
pub enum TapOutcome {
    /// Bytes for the underlying pane's pty. Caller does the write.
    SendBytes(Vec<u8>),
    /// Modifier (Ctrl/Alt) flipped — just a redraw needed.
    Toggled,
    /// Inert decorative cell, or no resolvable action.
    NoOp,
}

#[derive(Default)]
pub struct ModifierBarController {
    pub modifiers: KeyboardModifiers,
}

impl ModifierBarController {
    pub fn new() -> Self {
        Self::default()
    }

    /// Translate a tap on `cell` into the bytes / modifier flip it
    /// represents. `ctrl_held` / `alt_held` are `&mut` references to
    /// the corresponding `State` fields so the hardware-key passthrough
    /// path and this controller share the same one-shot modifier flags.
    pub fn handle_tap(
        &mut self,
        cell: CellId,
        ctrl_held: &mut bool,
        alt_held: &mut bool,
    ) -> TapOutcome {
        // Sync `State.ctrl_held`/`alt_held` into the modifier struct
        // before resolving the action, so a hardware-tap that armed
        // Ctrl is honoured by the next bar tap (and vice versa).
        self.modifiers.ctrl_armed = *ctrl_held;
        self.modifiers.alt_armed = *alt_held;

        let action = cell_action(cell);

        match action {
            KeyAction::ToggleModifier(m) => {
                self.modifiers.toggle(m);
                // Write Ctrl/Alt back through the shared `State` refs
                // so the hardware-key path agrees.
                match m {
                    Modifier::Ctrl => *ctrl_held = self.modifiers.ctrl_armed,
                    Modifier::Alt => *alt_held = self.modifiers.alt_armed,
                }
                TapOutcome::Toggled
            },
            KeyAction::SendKey(mut kwm) => {
                // Fold in any Ctrl/Alt that was armed so a tap on `→`
                // while ⌃ is armed produces exactly Ctrl+Right.
                if self.modifiers.ctrl_armed {
                    kwm.key_modifiers.insert(KeyModifier::Ctrl);
                }
                if self.modifiers.alt_armed {
                    kwm.key_modifiers.insert(KeyModifier::Alt);
                }
                let bytes = keys::serialize_key(&kwm);
                self.modifiers.consume_one_shots();
                *ctrl_held = false;
                *alt_held = false;
                TapOutcome::SendBytes(bytes)
            },
            KeyAction::NoOp => TapOutcome::NoOp,
        }
    }
}

/// Build a bare `KeyWithModifier` (no modifiers) from a `BareKey`.
/// The controller folds armed Ctrl/Alt in afterward.
fn bare(k: BareKey) -> KeyWithModifier {
    KeyWithModifier {
        bare_key: k,
        key_modifiers: BTreeSet::new(),
    }
}

/// Resolve a cell to its action. Hard-coded for the nine bar cells —
/// any unknown id resolves to `NoOp`.
fn cell_action(cell: CellId) -> KeyAction {
    match cell {
        CELL_ESC => KeyAction::SendKey(bare(BareKey::Esc)),
        CELL_TAB => KeyAction::SendKey(bare(BareKey::Tab)),
        CELL_CTRL => KeyAction::ToggleModifier(Modifier::Ctrl),
        CELL_ALT => KeyAction::ToggleModifier(Modifier::Alt),
        CELL_LEFT => KeyAction::SendKey(bare(BareKey::Left)),
        CELL_DOWN => KeyAction::SendKey(bare(BareKey::Down)),
        CELL_UP => KeyAction::SendKey(bare(BareKey::Up)),
        CELL_RIGHT => KeyAction::SendKey(bare(BareKey::Right)),
        CELL_MINUS => KeyAction::SendKey(bare(BareKey::Char('-'))),
        _ => KeyAction::NoOp,
    }
}

// ===========================================================================
// Renderer
// ===========================================================================
//
// One terminal row anchored at the bottom of the plugin area, just
// above where the OS soft keyboard surfaces. Up to nine labels
// separated by padded `" | "` pipes: ESC | TAB | CTRL | ALT | ← | ↓
// | ↑ | → | -.
//
// Painted via the host-decoded `Text` API so colours follow the
// user's palette. The whole row uses `.selected()` for a coherent
// ribbon; every label and pipe defaults to emphasis-3, and armed
// CTRL / ALT cells override their label to emphasis-2 so the
// one-shot modifier state stands out. Only CTRL and ALT are ever
// shown as active.
//
// The bar is responsive: when `cols` is too narrow for the full
// layout, three degradation axes apply in priority order
// (separator → labels → cells):
//   1. shrink the separator from `" | "` (3 cells) to `"|"` (1 cell);
//   2. shrink the text labels (ESC→ES, TAB→TB, CTRL→CTL, ALT→AL —
//      arrows and `-` cannot shrink further);
//   3. drop low-priority cells (first `-`, then `TAB`).
// `choose_config` walks all 12 (drop × labels × sep) combinations
// and picks the most-preferred one whose required width fits. When
// even the most-degraded layout cannot fit, the bar silently
// no-ops. Each rendered cell pushes one `ClickRegion::tight`; the
// trailing separator after cell N belongs to cell N's click region,
// so the bar has no dead pixels within its visible span.

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
    // mode. `compute_bar_layout` guarantees each cell gets *at least*
    // its natural width, so e.g. CTRL (4 cells wide) never gets
    // squeezed down to 3 cells and rendered as "CTR". Each cell also
    // gets identical symmetric padding (left == right == `pad`); any
    // slack that cannot be split evenly across every cell becomes
    // outer margin, centering the cells as a group rather than
    // producing asymmetric per-cell padding.
    let naturals: Vec<usize> = indices
        .iter()
        .map(|&i| UnicodeWidthStr::width(label_mode.label_for(&BAR[i])))
        .collect();
    let layout = compute_bar_layout(content_cols, &naturals);
    let widths = &layout.widths;

    // Build the bar as one combined string. Track:
    // - char-indexed ranges for each label (used by `color_range`,
    //   which is char-indexed)
    // - char-indexed ranges for each separator (so pipes also paint
    //   in emphasis-3 rather than inheriting the selected-bar fg)
    // - cell-indexed boundaries for click region tiling. The first
    //   cell's region absorbs `left_margin` (its col_start is 0); the
    //   last cell's region absorbs `right_margin` (its col_end is
    //   `cols`). The bar still has no dead pixels within its visible
    //   span.
    let mut bar = String::with_capacity(cols);
    let mut label_ranges: Vec<(std::ops::Range<usize>, bool)> = Vec::with_capacity(n);
    let mut sep_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(n.saturating_sub(1));
    let mut cell_boundaries: Vec<usize> = Vec::with_capacity(n + 1);

    let mut chars_cursor: usize = 0;
    let mut cells_cursor: usize = 0;
    cell_boundaries.push(0);

    // Leading outer margin — selected-style spaces so the ribbon
    // still spans the full row, but the cells themselves are pushed
    // inward to centre as a group.
    for _ in 0..layout.left_margin {
        bar.push(' ');
    }
    chars_cursor += layout.left_margin;
    cells_cursor += layout.left_margin;

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
        // `compute_bar_layout` produces `width == nat + 2*pad`, so
        // `width - visible_w` is always even and the two pads match.
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
            cell_boundaries.push(cells_cursor);
        }
    }

    // Trailing outer margin, mirrored from the leading margin.
    for _ in 0..layout.right_margin {
        bar.push(' ');
    }
    chars_cursor += layout.right_margin;
    cells_cursor += layout.right_margin;
    // Last cell's region runs to the end of the row, absorbing
    // `right_margin`.
    cell_boundaries.push(cells_cursor);
    // Unused after this point; explicitly drop the warning.
    let _ = chars_cursor;

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

/// Bar layout: per-cell widths plus the outer margins centred
/// around them. The contract is:
///   - every cell width equals `natural + 2 * pad` for the same
///     `pad`, so each label receives identical symmetric padding
///     (left and right always match);
///   - any slack that cannot be split evenly across every cell
///     spills into `left_margin` / `right_margin` rather than
///     inflating arbitrary cells.
///
/// `widths.iter().sum::<usize>() + left_margin + right_margin`
/// equals the `cols` argument passed to `compute_bar_layout`.
struct BarLayout {
    widths: Vec<usize>,
    left_margin: usize,
    right_margin: usize,
}

/// Compute per-cell widths and outer margins for the modifier bar.
///
/// Each cell receives *at least* its natural label width (so CTRL is
/// never squeezed below 4 cells and rendered as "CTR") plus an
/// identical pair of padding columns on either side. Any leftover
/// slack — at most `2n - 1` columns — is split between `left_margin`
/// and `right_margin`, centring the cells as a group. This keeps
/// per-cell padding strictly symmetric: padding is either there on
/// both sides or not there at all.
///
/// Precondition: `cols >= sum(naturals)`. `choose_config` enforces
/// this; widths saturate to the natural minimum on violation.
fn compute_bar_layout(cols: usize, naturals: &[usize]) -> BarLayout {
    let n = naturals.len();
    if n == 0 {
        return BarLayout {
            widths: Vec::new(),
            left_margin: cols,
            right_margin: 0,
        };
    }
    let natural_sum: usize = naturals.iter().sum();
    let slack = cols.saturating_sub(natural_sum);
    // Largest `pad` such that every cell can take `+2 * pad` columns
    // without overflowing the slack.
    let pad = slack / (2 * n);
    let widths: Vec<usize> = naturals.iter().map(|&nat| nat + 2 * pad).collect();
    let used: usize = widths.iter().sum();
    let outer = cols.saturating_sub(used);
    let left_margin = outer / 2;
    let right_margin = outer - left_margin;
    BarLayout {
        widths,
        left_margin,
        right_margin,
    }
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

    // --- modifier state -----------------------------------------------------

    #[test]
    fn consume_one_shots_clears_modifiers() {
        let mut m = KeyboardModifiers {
            ctrl_armed: true,
            alt_armed: true,
        };
        m.consume_one_shots();
        assert!(!m.ctrl_armed);
        assert!(!m.alt_armed);
    }

    #[test]
    fn toggle_flips_modifier_state() {
        let mut m = KeyboardModifiers::default();
        assert!(!m.is_armed(Modifier::Ctrl));
        m.toggle(Modifier::Ctrl);
        assert!(m.is_armed(Modifier::Ctrl));
        m.toggle(Modifier::Ctrl);
        assert!(!m.is_armed(Modifier::Ctrl));
    }

    // --- renderer -----------------------------------------------------------

    #[test]
    fn bar_layout_totals_to_cols() {
        let naturals = vec![1usize; BAR_CELL_COUNT];
        for cols in [9, 10, 20, 80, 137] {
            let layout = compute_bar_layout(cols, &naturals);
            let total: usize = layout.widths.iter().sum::<usize>()
                + layout.left_margin
                + layout.right_margin;
            assert_eq!(total, cols, "cols={}", cols);
            assert_eq!(layout.widths.len(), BAR_CELL_COUNT);
        }
    }

    #[test]
    fn bar_layout_every_cell_has_identical_symmetric_padding() {
        // For any input, each cell's width must equal `nat + 2 * pad`
        // for the same `pad` — i.e. the per-cell padding is identical
        // and symmetric. Leftover slack lives in the outer margins,
        // not in inflated cells.
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        for cols in [18, 19, 20, 26, 27, 50, 80, 137] {
            let layout = compute_bar_layout(cols, &naturals);
            let pads: Vec<usize> = layout
                .widths
                .iter()
                .zip(naturals.iter())
                .map(|(&w, &nat)| {
                    assert!(
                        w >= nat,
                        "cols={} cell width {} < natural {}",
                        cols,
                        w,
                        nat
                    );
                    let p2 = w - nat;
                    assert_eq!(p2 % 2, 0, "cols={} cell padding {} not even", cols, p2);
                    p2 / 2
                })
                .collect();
            let first = pads[0];
            for (i, &p) in pads.iter().enumerate() {
                assert_eq!(p, first, "cols={} cell {} pad={} (expected {})", cols, i, p, first);
            }
            // Outer margin can be at most 2n - 1 — anything larger
            // would have fit another full padding round.
            assert!(
                layout.left_margin + layout.right_margin < 2 * naturals.len(),
                "cols={} outer margin overflow",
                cols
            );
            // Margins balanced to within one column (centred).
            let diff = layout.left_margin.abs_diff(layout.right_margin);
            assert!(diff <= 1, "cols={} margins {}/{} not centred", cols, layout.left_margin, layout.right_margin);
        }
    }

    #[test]
    fn bar_layout_no_padding_when_slack_too_small() {
        // 19 cols across naturals summing to 18 — only 1 slack column,
        // not enough for a symmetric +1 on every cell. Every cell
        // therefore keeps its natural width, and the single column
        // becomes outer margin.
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        let layout = compute_bar_layout(19, &naturals);
        assert_eq!(layout.widths, naturals);
        assert_eq!(layout.left_margin + layout.right_margin, 1);
    }

    #[test]
    fn bar_layout_uses_padding_when_slack_permits() {
        // 18 + 2*9 = 36 cols → pad=1 on every cell, no outer margin.
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        let layout = compute_bar_layout(36, &naturals);
        assert_eq!(layout.widths, vec![5, 5, 6, 5, 3, 3, 3, 3, 3]);
        assert_eq!(layout.left_margin, 0);
        assert_eq!(layout.right_margin, 0);
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
