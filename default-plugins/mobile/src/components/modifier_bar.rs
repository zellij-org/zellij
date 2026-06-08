use std::collections::BTreeSet;

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::click::{ClickAction, ClickRegion};
use crate::keys;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardModifiers {
    pub ctrl_armed: bool,
    pub alt_armed: bool,
}

impl KeyboardModifiers {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub u16);

#[derive(Debug, Clone)]
pub enum KeyAction {
    SendKey(KeyWithModifier),
    ToggleModifier(Modifier),
    NoOp,
}

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

pub enum TapOutcome {
    SendBytes(Vec<u8>),
    Toggled,
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

    pub fn handle_tap(
        &mut self,
        cell: CellId,
        ctrl_held: &mut bool,
        alt_held: &mut bool,
    ) -> TapOutcome {
        self.modifiers.ctrl_armed = *ctrl_held;
        self.modifiers.alt_armed = *alt_held;

        let action = cell_action(cell);

        match action {
            KeyAction::ToggleModifier(m) => {
                self.modifiers.toggle(m);
                match m {
                    Modifier::Ctrl => *ctrl_held = self.modifiers.ctrl_armed,
                    Modifier::Alt => *alt_held = self.modifiers.alt_armed,
                }
                TapOutcome::Toggled
            },
            KeyAction::SendKey(mut kwm) => {
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

fn bare(k: BareKey) -> KeyWithModifier {
    KeyWithModifier {
        bare_key: k,
        key_modifiers: BTreeSet::new(),
    }
}

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

struct BarCell {
    id: CellId,
    label: &'static str,
    short_label: &'static str,
    modifier: Option<Modifier>,
}

const BAR: [BarCell; BAR_CELL_COUNT] = [
    BarCell {
        id: CELL_ESC,
        label: "ESC",
        short_label: "ES",
        modifier: None,
    },
    BarCell {
        id: CELL_TAB,
        label: "TAB",
        short_label: "TB",
        modifier: None,
    },
    BarCell {
        id: CELL_CTRL,
        label: "CTRL",
        short_label: "CTL",
        modifier: Some(Modifier::Ctrl),
    },
    BarCell {
        id: CELL_ALT,
        label: "ALT",
        short_label: "AL",
        modifier: Some(Modifier::Alt),
    },
    BarCell {
        id: CELL_LEFT,
        label: "\u{2190}",
        short_label: "\u{2190}",
        modifier: None,
    },
    BarCell {
        id: CELL_DOWN,
        label: "\u{2193}",
        short_label: "\u{2193}",
        modifier: None,
    },
    BarCell {
        id: CELL_UP,
        label: "\u{2191}",
        short_label: "\u{2191}",
        modifier: None,
    },
    BarCell {
        id: CELL_RIGHT,
        label: "\u{2192}",
        short_label: "\u{2192}",
        modifier: None,
    },
    BarCell {
        id: CELL_MINUS,
        label: "-",
        short_label: "-",
        modifier: None,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SepMode {
    Wide,
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
    None,
    DropMinus,
    DropMinusAndTab,
}

impl DropMode {
    fn cell_indices(self) -> &'static [usize] {
        match self {
            Self::None => &[0, 1, 2, 3, 4, 5, 6, 7, 8],
            Self::DropMinus => &[0, 1, 2, 3, 4, 5, 6, 7],
            Self::DropMinusAndTab => &[0, 2, 3, 4, 5, 6, 7],
        }
    }
}

fn required_width(drop: DropMode, labels: LabelMode, sep: SepMode) -> usize {
    let indices = drop.cell_indices();
    let n = indices.len();
    let labels_sum: usize = indices
        .iter()
        .map(|&i| UnicodeWidthStr::width(labels.label_for(&BAR[i])))
        .sum();
    labels_sum + n.saturating_sub(1) * sep.width()
}

fn choose_config(cols: usize) -> Option<(DropMode, LabelMode, SepMode)> {
    const CONFIGS: [(DropMode, LabelMode, SepMode); 12] = [
        (DropMode::None, LabelMode::Full, SepMode::Wide),
        (DropMode::None, LabelMode::Full, SepMode::Compact),
        (DropMode::None, LabelMode::Short, SepMode::Wide),
        (DropMode::None, LabelMode::Short, SepMode::Compact),
        (DropMode::DropMinus, LabelMode::Full, SepMode::Wide),
        (DropMode::DropMinus, LabelMode::Full, SepMode::Compact),
        (DropMode::DropMinus, LabelMode::Short, SepMode::Wide),
        (DropMode::DropMinus, LabelMode::Short, SepMode::Compact),
        (DropMode::DropMinusAndTab, LabelMode::Full, SepMode::Wide),
        (DropMode::DropMinusAndTab, LabelMode::Full, SepMode::Compact),
        (DropMode::DropMinusAndTab, LabelMode::Short, SepMode::Wide),
        (
            DropMode::DropMinusAndTab,
            LabelMode::Short,
            SepMode::Compact,
        ),
    ];
    CONFIGS
        .iter()
        .copied()
        .find(|&(d, l, s)| cols >= required_width(d, l, s))
}

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
    let naturals: Vec<usize> = indices
        .iter()
        .map(|&i| UnicodeWidthStr::width(label_mode.label_for(&BAR[i])))
        .collect();
    let layout = compute_bar_layout(content_cols, &naturals);
    let widths = &layout.widths;

    let mut bar = String::with_capacity(cols);
    let mut label_ranges: Vec<(std::ops::Range<usize>, bool)> = Vec::with_capacity(n);
    let mut sep_ranges: Vec<std::ops::Range<usize>> = Vec::with_capacity(n.saturating_sub(1));
    let mut cell_boundaries: Vec<usize> = Vec::with_capacity(n + 1);

    let mut chars_cursor: usize = 0;
    let mut cells_cursor: usize = 0;
    cell_boundaries.push(0);

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

    for _ in 0..layout.right_margin {
        bar.push(' ');
    }
    chars_cursor += layout.right_margin;
    cells_cursor += layout.right_margin;
    cell_boundaries.push(cells_cursor);
    let _ = chars_cursor;

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

struct BarLayout {
    widths: Vec<usize>,
    left_margin: usize,
    right_margin: usize,
}

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

    #[test]
    fn bar_layout_totals_to_cols() {
        let naturals = vec![1usize; BAR_CELL_COUNT];
        for cols in [9, 10, 20, 80, 137] {
            let layout = compute_bar_layout(cols, &naturals);
            let total: usize =
                layout.widths.iter().sum::<usize>() + layout.left_margin + layout.right_margin;
            assert_eq!(total, cols, "cols={}", cols);
            assert_eq!(layout.widths.len(), BAR_CELL_COUNT);
        }
    }

    #[test]
    fn bar_layout_every_cell_has_identical_symmetric_padding() {
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        for cols in [18, 19, 20, 26, 27, 50, 80, 137] {
            let layout = compute_bar_layout(cols, &naturals);
            let pads: Vec<usize> = layout
                .widths
                .iter()
                .zip(naturals.iter())
                .map(|(&w, &nat)| {
                    assert!(w >= nat, "cols={} cell width {} < natural {}", cols, w, nat);
                    let p2 = w - nat;
                    assert_eq!(p2 % 2, 0, "cols={} cell padding {} not even", cols, p2);
                    p2 / 2
                })
                .collect();
            let first = pads[0];
            for (i, &p) in pads.iter().enumerate() {
                assert_eq!(
                    p, first,
                    "cols={} cell {} pad={} (expected {})",
                    cols, i, p, first
                );
            }
            assert!(
                layout.left_margin + layout.right_margin < 2 * naturals.len(),
                "cols={} outer margin overflow",
                cols
            );
            let diff = layout.left_margin.abs_diff(layout.right_margin);
            assert!(
                diff <= 1,
                "cols={} margins {}/{} not centred",
                cols,
                layout.left_margin,
                layout.right_margin
            );
        }
    }

    #[test]
    fn bar_layout_no_padding_when_slack_too_small() {
        let naturals = vec![3, 3, 4, 3, 1, 1, 1, 1, 1];
        let layout = compute_bar_layout(19, &naturals);
        assert_eq!(layout.widths, naturals);
        assert_eq!(layout.left_margin + layout.right_margin, 1);
    }

    #[test]
    fn bar_layout_uses_padding_when_slack_permits() {
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
        assert_eq!(
            choose_config(19),
            Some((
                DropMode::DropMinusAndTab,
                LabelMode::Short,
                SepMode::Compact
            ))
        );
        assert_eq!(
            choose_config(17),
            Some((
                DropMode::DropMinusAndTab,
                LabelMode::Short,
                SepMode::Compact
            ))
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
        let mods = KeyboardModifiers::default();
        let mut regions = Vec::new();
        render_modifier_bar(&mods, 0, 16, &mut regions);
        assert!(regions.is_empty());
    }
}
