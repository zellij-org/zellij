//! US-QWERTY layout. The only layout shipped in v1.
//!
//! Three layers visible to the user, switched via the bottom bar's
//! toggle cell and Fn cell:
//!
//! - **Letters** — staggered `qwerty` / `asdf` / `zxcv` with a wide
//!   backspace on row 3. Shift uppercases the letter labels.
//! - **Symbols** (`?123`) — digits + shell punctuation.
//! - **Functions** (`Fn`) — F1–F12 with seven inert filler cells.
//!
//! Layouts come in two tiers driven by the available row budget:
//!
//! - **Natural tier**: 5 rows on Letters / Symbols (extras strip on
//!   top + 3 content rows + bottom bar) and 4 rows on Functions.
//! - **Compact tier**: 4 rows per layer. Letters drops the extras
//!   strip; Symbols / Functions replace it with a terminal-affordances
//!   row (Esc / Tab / Ctl / Alt + arrows) inline.
//!
//! Both tiers produce cells already sized to the requested
//! `target_block_width` (the renderer does not scale). Letter cells
//! share a uniform width within a layer when the cell count divides
//! the target; otherwise the smaller cells sit on the row edges in
//! symmetric pairs (`edge_first_widths`).
//!
//! Cell IDs are stable u16s spanning the union of all layers' cells;
//! the controller and renderer never depend on the numbering scheme,
//! they round-trip the opaque `CellId` only.

use std::borrow::Cow;

use zellij_tile::prelude::*;

use crate::keyboard::layout::{CellId, KeyAction, KeyCell, KeyRow, KeyboardLayout};
use crate::keyboard::modifiers::{KeyLayer, KeyboardModifiers, Modifier};

// -------------------------------------------------------------------
// Cell IDs.
//
// Ranges are grouped per row for readability. The exact numbering is
// not load-bearing — only stability across re-renders matters since
// click regions reference cell IDs.
// -------------------------------------------------------------------

// Extras strip / terminal-affordances row cells.
const ID_ESC: u16 = 0;
const ID_TAB: u16 = 1;
const ID_CTL: u16 = 2;
const ID_ALT: u16 = 3;
const ID_ARROW_LEFT: u16 = 4;
const ID_ARROW_DOWN: u16 = 5;
const ID_ARROW_UP: u16 = 6;
const ID_ARROW_RIGHT: u16 = 7;

// Letters row 1: q w e r t y u i o p → cells 10..=19.
const ID_LETTERS_R1_START: u16 = 10;
// Letters row 2: a s d f g h j k l → cells 20..=28.
const ID_LETTERS_R2_START: u16 = 20;
// Letters row 3: ⇧ z x c v b n m . / ⌫.
const ID_LETTERS_R3_SHIFT: u16 = 30;
const ID_LETTERS_R3_LETTERS_START: u16 = 31;
const ID_LETTERS_R3_PERIOD: u16 = 38;
const ID_LETTERS_R3_SLASH: u16 = 39;
const ID_LETTERS_R3_BACKSPACE: u16 = 79;

// Symbols row 1: 1234567890-=~ → cells 40..=52.
const ID_SYMBOLS_R1_START: u16 = 40;
// Symbols row 2: !@#$&*()[]{} + ⌫ → cells 53..=65.
const ID_SYMBOLS_R2_START: u16 = 53;
const ID_SYMBOLS_R2_BACKSPACE: u16 = 65;
// Symbols row 3: /\:;|<>?'",.` → cells 66..=78.
const ID_SYMBOLS_R3_START: u16 = 66;

// Functions row 1: F1..F10 → cells 80..=89.
const ID_FUNCTIONS_R1_START: u16 = 80;
// Functions row 2: F11, F12, 7×inert, ⌫ → cells 90..=99.
const ID_FUNCTIONS_R2_F11: u16 = 90;
const ID_FUNCTIONS_R2_F12: u16 = 91;
const ID_FUNCTIONS_R2_INERT_START: u16 = 92;
const ID_FUNCTIONS_R2_BACKSPACE: u16 = 99;

// Bottom bar.
const ID_BOTTOM_TOGGLE: u16 = 200;
const ID_BOTTOM_FN: u16 = 201;
const ID_BOTTOM_SPACE: u16 = 202;
const ID_BOTTOM_ENTER: u16 = 203;

// -------------------------------------------------------------------
// Layout content tables. Static `&'static str` labels so `label()`
// returns borrowed Cows without allocating.
// -------------------------------------------------------------------

/// Letters row 1 (`q`..`p`).
const LETTERS_R1: &[(&str, &str, char, char)] = &[
    ("q", "Q", 'q', 'Q'),
    ("w", "W", 'w', 'W'),
    ("e", "E", 'e', 'E'),
    ("r", "R", 'r', 'R'),
    ("t", "T", 't', 'T'),
    ("y", "Y", 'y', 'Y'),
    ("u", "U", 'u', 'U'),
    ("i", "I", 'i', 'I'),
    ("o", "O", 'o', 'O'),
    ("p", "P", 'p', 'P'),
];

/// Letters row 2 (`a`..`l`).
const LETTERS_R2: &[(&str, &str, char, char)] = &[
    ("a", "A", 'a', 'A'),
    ("s", "S", 's', 'S'),
    ("d", "D", 'd', 'D'),
    ("f", "F", 'f', 'F'),
    ("g", "G", 'g', 'G'),
    ("h", "H", 'h', 'H'),
    ("j", "J", 'j', 'J'),
    ("k", "K", 'k', 'K'),
    ("l", "L", 'l', 'L'),
];

/// Letters row 3 letters (`z`..`m`). The flanking ⇧ and ⌫ cells live
/// outside this table because they have distinct widths and actions.
const LETTERS_R3_LETTERS: &[(&str, &str, char, char)] = &[
    ("z", "Z", 'z', 'Z'),
    ("x", "X", 'x', 'X'),
    ("c", "C", 'c', 'C'),
    ("v", "V", 'v', 'V'),
    ("b", "B", 'b', 'B'),
    ("n", "N", 'n', 'N'),
    ("m", "M", 'm', 'M'),
];

/// Symbols row 1 — digits + tail punctuation. Shift has no effect on
/// the Symbols layer (the symbol set is already a curated mix).
const SYMBOLS_R1: &[(&str, char)] = &[
    ("1", '1'),
    ("2", '2'),
    ("3", '3'),
    ("4", '4'),
    ("5", '5'),
    ("6", '6'),
    ("7", '7'),
    ("8", '8'),
    ("9", '9'),
    ("0", '0'),
    ("-", '-'),
    ("=", '='),
    ("~", '~'),
];

/// Symbols row 2 — shift-digits and brackets. Trailing cell (index
/// 12) is ⌫, handled separately so the row table stays uniform.
const SYMBOLS_R2: &[(&str, char)] = &[
    ("!", '!'),
    ("@", '@'),
    ("#", '#'),
    ("$", '$'),
    ("&", '&'),
    ("*", '*'),
    ("(", '('),
    (")", ')'),
    ("[", '['),
    ("]", ']'),
    ("{", '{'),
    ("}", '}'),
];

/// Symbols row 3 — shell punctuation.
const SYMBOLS_R3: &[(&str, char)] = &[
    ("/", '/'),
    ("\\", '\\'),
    (":", ':'),
    (";", ';'),
    ("|", '|'),
    ("<", '<'),
    (">", '>'),
    ("?", '?'),
    ("'", '\''),
    ("\"", '"'),
    (",", ','),
    (".", '.'),
    ("`", '`'),
];

// -------------------------------------------------------------------
// Sizing helpers (Variant B).
//
// `edge_first_widths` distributes `target` cols across `count` cells
// using two widths `base` and `base+1`. Smaller (`base`) cells go on
// the row edges as symmetric pairs from outside in; any odd leftover
// goes to the center cell. Examples:
//
//   edge_first_widths(28, 10) → [2,3,3,3,3,3,3,3,3,2]
//   edge_first_widths(28, 11) → [2,2,3,3,3,2,3,3,3,2,2]
//   edge_first_widths(28,  6) → [4,5,5,5,5,4]
//   edge_first_widths(40, 10) → [4,4,4,4,4,4,4,4,4,4]   (exact)
//
// `build_cells` materializes a sequence of `(id, width)` pairs into
// `KeyCell`s with cumulative `col_start`/`col_end`. The returned
// cells already carry absolute positions relative to the row's
// `offset_col`.
// -------------------------------------------------------------------

pub(super) fn edge_first_widths(target: u16, count: u16) -> Vec<u16> {
    debug_assert!(count > 0);
    let base = target / count;
    let big = base + 1;
    let n_big = target - base * count;
    let n_small = count - n_big;
    let mut widths = vec![big; count as usize];
    let pairs = n_small / 2;
    let has_center = (n_small % 2) == 1;
    for i in 0..pairs {
        widths[i as usize] = base;
        widths[(count - 1 - i) as usize] = base;
    }
    if has_center {
        widths[(count / 2) as usize] = base;
    }
    widths
}

fn build_cells(items: &[(u16, u16)]) -> Vec<KeyCell> {
    let mut col: u16 = 0;
    let mut out = Vec::with_capacity(items.len());
    for (id, width) in items {
        out.push(KeyCell {
            col_start: col,
            col_end: col + *width,
            id: CellId(*id),
        });
        col += *width;
    }
    out
}

/// Spread `slack` cols across `n` items, returning a Vec of extras
/// (sums to `slack`). Extras are distributed as evenly as possible:
/// `slack / n` to every item, with the leftover going to the first
/// `slack % n` items.
fn spread_slack(slack: u16, n: usize) -> Vec<u16> {
    if n == 0 {
        return Vec::new();
    }
    let per = slack / n as u16;
    let rem = slack % n as u16;
    (0..n)
        .map(|i| per + if (i as u16) < rem { 1 } else { 0 })
        .collect()
}

// -------------------------------------------------------------------
// UsQwerty: cells are constructed on demand from `target_block_width`.
// No precomputed cell map — the layout is stateless past its ID.
// -------------------------------------------------------------------

pub struct UsQwerty;

impl Default for UsQwerty {
    fn default() -> Self {
        Self::new()
    }
}

impl UsQwerty {
    pub fn new() -> Self {
        Self
    }

    // ---------------------------------------------------------------
    // Natural-tier rows
    // ---------------------------------------------------------------

    /// Extras strip at the top of natural-tier Letters / Symbols / Fn.
    /// Eight cells: Esc / Tab / Ctl / Alt (modifiers) and four arrows.
    /// Slack is distributed to modifiers first, then to arrows.
    fn natural_extras_row(&self, target: u16) -> KeyRow {
        let labels = [
            ID_ESC,
            ID_TAB,
            ID_CTL,
            ID_ALT,
            ID_ARROW_LEFT,
            ID_ARROW_DOWN,
            ID_ARROW_UP,
            ID_ARROW_RIGHT,
        ];
        // Base widths: modifiers 5, arrows 3 (= 32 cols natural).
        let mut widths = [5u16, 5, 5, 5, 3, 3, 3, 3];
        let base_total: u16 = widths.iter().sum();
        if target > base_total {
            let extras = spread_slack(target - base_total, widths.len());
            for (w, e) in widths.iter_mut().zip(extras.iter()) {
                *w += *e;
            }
        } else if target < base_total {
            // Shrink the modifiers (they have headroom) so the row fits.
            // This branch is only hit on very narrow natural-tier
            // viewports; the compact tier engages there in practice.
            let mut deficit = base_total - target;
            for w in widths.iter_mut() {
                while deficit > 0 && *w > 1 {
                    *w -= 1;
                    deficit -= 1;
                }
                if deficit == 0 {
                    break;
                }
            }
        }
        let items: Vec<(u16, u16)> = labels.iter().zip(widths.iter()).map(|(id, w)| (*id, *w)).collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Natural-tier Letters R1 — 10 letters. Edge-first widths.
    fn natural_letters_r1(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target, 10);
        let items: Vec<(u16, u16)> = (0..LETTERS_R1.len() as u16)
            .zip(widths.iter())
            .map(|(i, w)| (ID_LETTERS_R1_START + i, *w))
            .collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Natural-tier Letters R2 — 9 letters filling the full target
    /// width via `edge_first_widths`. No stagger: cells are sized to
    /// be as wide as the row allows, with smaller cells on the edges
    /// and the bigger ones absorbing the remainder near the middle.
    fn natural_letters_r2(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target.max(9), 9);
        let items: Vec<(u16, u16)> = (0..LETTERS_R2.len() as u16)
            .zip(widths.iter())
            .map(|(i, w)| (ID_LETTERS_R2_START + i, *w))
            .collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Natural-tier Letters R3 — ⇧ + 7 letters + . + / + ⌫.
    ///
    /// Period and slash get a fixed floor of 2 cols (never narrower);
    /// shift and backspace also get a floor of 2 cols. When the anchor
    /// budget can't host all four anchors at >= 2 cols (target = 20, 21
    /// with lw=2), `.` and `/` are dropped and the row falls back to
    /// the 9-cell ⇧ + 7 letters + ⌫ layout that the compact tier uses.
    /// No R3 cell is ever narrower than 2 cols.
    fn natural_letters_r3(&self, target: u16) -> KeyRow {
        const PUNCT_FLOOR: u16 = 2;
        const BRACKET_MIN: u16 = 2;
        let lw = (target / 10).max(1);
        let letters_total = 7 * lw;
        let anchor_budget = target.saturating_sub(letters_total);
        let needed = 2 * PUNCT_FLOOR + 2 * BRACKET_MIN;
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(11);
        if anchor_budget >= needed {
            // Full 11-cell row: ⇧ + 7 letters + . + / + ⌫.
            let punct_reserved = 2 * PUNCT_FLOOR;
            let period_w = (punct_reserved + 1) / 2;
            let slash_w = punct_reserved - period_w;
            let bracket_budget = anchor_budget - punct_reserved;
            let shift_w = (bracket_budget + 1) / 2;
            let backspace_w = bracket_budget - shift_w;
            items.push((ID_LETTERS_R3_SHIFT, shift_w));
            for i in 0..LETTERS_R3_LETTERS.len() as u16 {
                items.push((ID_LETTERS_R3_LETTERS_START + i, lw));
            }
            items.push((ID_LETTERS_R3_PERIOD, period_w));
            items.push((ID_LETTERS_R3_SLASH, slash_w));
            items.push((ID_LETTERS_R3_BACKSPACE, backspace_w));
        } else {
            // Drop . and /: 9-cell row (⇧ + 7 letters + ⌫). Same shape
            // as compact-tier R3. Keeps every cell >= 2 cols wide at
            // the narrowest natural-tier widths.
            let shift_w = (anchor_budget + 1) / 2;
            let backspace_w = anchor_budget - shift_w;
            items.push((ID_LETTERS_R3_SHIFT, shift_w));
            for i in 0..LETTERS_R3_LETTERS.len() as u16 {
                items.push((ID_LETTERS_R3_LETTERS_START + i, lw));
            }
            items.push((ID_LETTERS_R3_BACKSPACE, backspace_w));
        }
        KeyRow::tall(0, build_cells(&items))
    }

    /// Symbols row 1 / R2 / R3 — 13 cells, edge-first widths.
    /// `start_id` selects which row's IDs to populate.
    fn natural_symbols_row(&self, target: u16, start_id: u16, has_backspace_at_end: bool) -> KeyRow {
        let widths = edge_first_widths(target, 13);
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(13);
        if has_backspace_at_end {
            // 12 punctuation cells + ⌫ anchor at index 12.
            for i in 0..12u16 {
                items.push((start_id + i, widths[i as usize]));
            }
            items.push((ID_SYMBOLS_R2_BACKSPACE, widths[12]));
        } else {
            for i in 0..13u16 {
                items.push((start_id + i, widths[i as usize]));
            }
        }
        KeyRow::tall(0, build_cells(&items))
    }

    /// Functions row 1 — F1..F10. 10 cells, edge-first widths.
    fn natural_functions_row_1(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target, 10);
        let items: Vec<(u16, u16)> = (0..10u16)
            .zip(widths.iter())
            .map(|(i, w)| (ID_FUNCTIONS_R1_START + i, *w))
            .collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Functions row 2 — F11, F12, 7×inert filler, ⌫. 10 cells,
    /// edge-first widths.
    fn natural_functions_row_2(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target, 10);
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(10);
        items.push((ID_FUNCTIONS_R2_F11, widths[0]));
        items.push((ID_FUNCTIONS_R2_F12, widths[1]));
        for i in 0..7u16 {
            items.push((ID_FUNCTIONS_R2_INERT_START + i, widths[(2 + i) as usize]));
        }
        items.push((ID_FUNCTIONS_R2_BACKSPACE, widths[9]));
        KeyRow::tall(0, build_cells(&items))
    }

    /// Bottom bar — `?123` / `ABC` (anchor), `Fn` (anchor), `space`
    /// (flex), `↵` (anchor). `space` absorbs all remaining slack.
    /// Natural-tier anchors are deliberately oversized relative to
    /// their label width (`?123` 4 chars → 8 cell-cols, `Fn` 2 → 6,
    /// `↵` 1 → 5) so the modal-switch and enter actions are easier
    /// touch targets on a large viewport.
    fn natural_bottom_bar(&self, target: u16, layer: KeyLayer) -> KeyRow {
        let toggle_w: u16 = match layer {
            KeyLayer::Letters => 8,             // ?123
            KeyLayer::Symbols | KeyLayer::Functions => 7, // ABC
        };
        let fn_w: u16 = 6;
        let enter_w: u16 = 5;
        let anchor_total = toggle_w + fn_w + enter_w;
        let space_w = target.saturating_sub(anchor_total).max(1);
        let items = [
            (ID_BOTTOM_TOGGLE, toggle_w),
            (ID_BOTTOM_FN, fn_w),
            (ID_BOTTOM_SPACE, space_w),
            (ID_BOTTOM_ENTER, enter_w),
        ];
        KeyRow::tall(0, build_cells(&items))
    }

    // ---------------------------------------------------------------
    // Compact-tier rows
    // ---------------------------------------------------------------

    /// Compact-tier terminal-affordances row (8 cells). Mirrors the
    /// natural extras strip but with a smaller base width budget.
    fn compact_terminal_row(&self, target: u16) -> KeyRow {
        let labels = [
            ID_ESC,
            ID_TAB,
            ID_CTL,
            ID_ALT,
            ID_ARROW_LEFT,
            ID_ARROW_DOWN,
            ID_ARROW_UP,
            ID_ARROW_RIGHT,
        ];
        // Compact base widths: modifiers 4, arrows 2 (= 24).
        let mut widths = [4u16, 4, 4, 4, 2, 2, 2, 4];
        let base_total: u16 = widths.iter().sum();
        if target > base_total {
            let extras = spread_slack(target - base_total, widths.len());
            for (w, e) in widths.iter_mut().zip(extras.iter()) {
                *w += *e;
            }
        }
        let items: Vec<(u16, u16)> = labels.iter().zip(widths.iter()).map(|(id, w)| (*id, *w)).collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier Letters R1 — 10 letters, edge-first widths.
    fn compact_letters_r1(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target, 10);
        let items: Vec<(u16, u16)> = (0..LETTERS_R1.len() as u16)
            .zip(widths.iter())
            .map(|(i, w)| (ID_LETTERS_R1_START + i, *w))
            .collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier Letters R2 — 9 letters with a 1-col half-cell stagger.
    fn compact_letters_r2(&self, target: u16) -> KeyRow {
        let lw = (target.saturating_sub(1) / 9).max(1);
        let stagger = 1u16;
        let letters_total = 9 * lw;
        let right_pad = target.saturating_sub(stagger + letters_total);
        let items: Vec<(u16, u16)> = (0..LETTERS_R2.len() as u16)
            .map(|i| (ID_LETTERS_R2_START + i, lw))
            .collect();
        let mut row = KeyRow::tall(stagger, build_cells(&items));
        row.right_pad = right_pad;
        row
    }

    /// Compact-tier Letters R3 — ⇧ + 7 letters + ⌫. Compact omits
    /// the `.` and `/` cells; they remain on the natural-tier layout.
    fn compact_letters_r3(&self, target: u16) -> KeyRow {
        // Anchor minimums for shift (3) and backspace (4); letters fill
        // the remainder with edge-first distribution if not divisible.
        let shift_min: u16 = 3;
        let bs_min: u16 = 4;
        let anchor_total = shift_min + bs_min;
        let letter_budget = target.saturating_sub(anchor_total);
        let lw = (letter_budget / 7).max(1);
        let letters_used = 7 * lw;
        let mut shift_w = shift_min;
        let mut bs_w = bs_min;
        let mut remainder = letter_budget - letters_used;
        // Spread remainder between shift and ⌫, ⌫ first.
        while remainder > 0 {
            if remainder > 0 {
                bs_w += 1;
                remainder -= 1;
            }
            if remainder > 0 {
                shift_w += 1;
                remainder -= 1;
            }
        }
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(9);
        items.push((ID_LETTERS_R3_SHIFT, shift_w));
        for i in 0..LETTERS_R3_LETTERS.len() as u16 {
            items.push((ID_LETTERS_R3_LETTERS_START + i, lw));
        }
        items.push((ID_LETTERS_R3_BACKSPACE, bs_w));
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier Symbols digits — `1 2 3 4 5 6 7 8 9 0 ~` (11 cells).
    /// The natural-tier `- =` cells are omitted in compact. `~` keeps
    /// its natural-tier ID (`ID_SYMBOLS_R1_START + 12`).
    fn compact_symbols_digits(&self, target: u16) -> KeyRow {
        let widths = edge_first_widths(target, 11);
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(11);
        for i in 0..10u16 {
            items.push((ID_SYMBOLS_R1_START + i, widths[i as usize]));
        }
        items.push((ID_SYMBOLS_R1_START + 12, widths[10]));
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier Symbols punctuation — `/ \ : ; | < > ? . ⌫`
    /// (10 cells). 9 letters share an edge-first distribution across
    /// the row minus the backspace anchor on the right. `.` keeps its
    /// natural-tier ID (`ID_SYMBOLS_R3_START + 11`); ⌫ reuses the
    /// Symbols R2 backspace ID.
    fn compact_symbols_punctuation(&self, target: u16) -> KeyRow {
        let bs_w: u16 = 4;
        let letter_target = target.saturating_sub(bs_w);
        let letter_widths = edge_first_widths(letter_target, 9);
        let mut items: Vec<(u16, u16)> = Vec::with_capacity(10);
        // SYMBOLS_R3 indices 0..=7 are / \ : ; | < > ?
        for i in 0..8u16 {
            items.push((ID_SYMBOLS_R3_START + i, letter_widths[i as usize]));
        }
        // `.` is SYMBOLS_R3[11].
        items.push((ID_SYMBOLS_R3_START + 11, letter_widths[8]));
        items.push((ID_SYMBOLS_R2_BACKSPACE, bs_w));
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier Functions row — 6 cells, edge-first widths.
    fn compact_functions_row(&self, target: u16, ids: [u16; 6]) -> KeyRow {
        let widths = edge_first_widths(target, 6);
        let items: Vec<(u16, u16)> = ids
            .iter()
            .zip(widths.iter())
            .map(|(id, w)| (*id, *w))
            .collect();
        KeyRow::tall(0, build_cells(&items))
    }

    /// Compact-tier bottom bar.
    fn compact_bottom_bar(&self, target: u16, layer: KeyLayer) -> KeyRow {
        let (toggle_w, fn_w, enter_w): (u16, u16, u16) = match layer {
            KeyLayer::Letters => (4, 2, 3),
            KeyLayer::Symbols | KeyLayer::Functions => (3, 2, 3),
        };
        let anchor_total = toggle_w + fn_w + enter_w;
        let space_w = target.saturating_sub(anchor_total).max(1);
        let items = [
            (ID_BOTTOM_TOGGLE, toggle_w),
            (ID_BOTTOM_FN, fn_w),
            (ID_BOTTOM_SPACE, space_w),
            (ID_BOTTOM_ENTER, enter_w),
        ];
        KeyRow::tall(0, build_cells(&items))
    }
}

impl KeyboardLayout for UsQwerty {
    fn id(&self) -> &'static str {
        "us-qwerty"
    }

    fn display_name(&self) -> &'static str {
        "US (QWERTY)"
    }

    fn rows(&self, mods: &KeyboardModifiers, target_block_width: u16) -> Vec<KeyRow> {
        let t = target_block_width.max(1);
        match mods.layer {
            KeyLayer::Letters => vec![
                self.natural_extras_row(t),
                self.natural_letters_r1(t),
                self.natural_letters_r2(t),
                self.natural_letters_r3(t),
                self.natural_bottom_bar(t, KeyLayer::Letters),
            ],
            KeyLayer::Symbols => vec![
                self.natural_extras_row(t),
                self.natural_symbols_row(t, ID_SYMBOLS_R1_START, false),
                self.natural_symbols_row(t, ID_SYMBOLS_R2_START, true),
                self.natural_symbols_row(t, ID_SYMBOLS_R3_START, false),
                self.natural_bottom_bar(t, KeyLayer::Symbols),
            ],
            KeyLayer::Functions => vec![
                self.natural_extras_row(t),
                self.natural_functions_row_1(t),
                self.natural_functions_row_2(t),
                self.natural_bottom_bar(t, KeyLayer::Functions),
            ],
        }
    }

    fn compact_rows(&self, mods: &KeyboardModifiers, target_block_width: u16) -> Vec<KeyRow> {
        let t = target_block_width.max(1);
        match mods.layer {
            KeyLayer::Letters => vec![
                self.compact_letters_r1(t),
                self.compact_letters_r2(t),
                self.compact_letters_r3(t),
                self.compact_bottom_bar(t, KeyLayer::Letters),
            ],
            KeyLayer::Symbols => vec![
                self.compact_terminal_row(t),
                self.compact_symbols_digits(t),
                self.compact_symbols_punctuation(t),
                self.compact_bottom_bar(t, KeyLayer::Symbols),
            ],
            KeyLayer::Functions => vec![
                self.compact_functions_row(
                    t,
                    [
                        ID_FUNCTIONS_R1_START,
                        ID_FUNCTIONS_R1_START + 1,
                        ID_FUNCTIONS_R1_START + 2,
                        ID_FUNCTIONS_R1_START + 3,
                        ID_FUNCTIONS_R1_START + 4,
                        ID_FUNCTIONS_R1_START + 5,
                    ],
                ),
                self.compact_functions_row(
                    t,
                    [
                        ID_FUNCTIONS_R1_START + 6,
                        ID_FUNCTIONS_R1_START + 7,
                        ID_FUNCTIONS_R1_START + 8,
                        ID_FUNCTIONS_R1_START + 9,
                        ID_FUNCTIONS_R2_F11,
                        ID_FUNCTIONS_R2_F12,
                    ],
                ),
                self.compact_terminal_row(t),
                self.compact_bottom_bar(t, KeyLayer::Functions),
            ],
        }
    }

    fn label(&self, cell: CellId, mods: &KeyboardModifiers) -> Cow<'static, str> {
        // Extras strip / terminal affordances (layer-independent).
        match cell.0 {
            ID_ESC => return Cow::Borrowed("Esc"),
            ID_TAB => return Cow::Borrowed("Tab"),
            ID_CTL => return Cow::Borrowed("Ctl"),
            ID_ALT => return Cow::Borrowed("Alt"),
            ID_ARROW_LEFT => return Cow::Borrowed("←"),
            ID_ARROW_DOWN => return Cow::Borrowed("↓"),
            ID_ARROW_UP => return Cow::Borrowed("↑"),
            ID_ARROW_RIGHT => return Cow::Borrowed("→"),
            _ => {},
        }

        // Bottom bar.
        if cell.0 == ID_BOTTOM_TOGGLE {
            return match mods.layer {
                KeyLayer::Letters => Cow::Borrowed("?123"),
                KeyLayer::Symbols | KeyLayer::Functions => Cow::Borrowed("ABC"),
            };
        }
        if cell.0 == ID_BOTTOM_FN {
            return Cow::Borrowed("Fn");
        }
        if cell.0 == ID_BOTTOM_SPACE {
            return Cow::Borrowed("space");
        }
        if cell.0 == ID_BOTTOM_ENTER {
            return Cow::Borrowed("↵");
        }

        // Letters layer printables.
        if (ID_LETTERS_R1_START..=ID_LETTERS_R1_START + 9).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R1_START) as usize;
            let (lo, up, _, _) = LETTERS_R1[i];
            return Cow::Borrowed(if mods.shift_armed { up } else { lo });
        }
        if (ID_LETTERS_R2_START..=ID_LETTERS_R2_START + 8).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R2_START) as usize;
            let (lo, up, _, _) = LETTERS_R2[i];
            return Cow::Borrowed(if mods.shift_armed { up } else { lo });
        }
        if (ID_LETTERS_R3_LETTERS_START..=ID_LETTERS_R3_LETTERS_START + 6).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R3_LETTERS_START) as usize;
            let (lo, up, _, _) = LETTERS_R3_LETTERS[i];
            return Cow::Borrowed(if mods.shift_armed { up } else { lo });
        }
        if cell.0 == ID_LETTERS_R3_SHIFT {
            return Cow::Borrowed("⇧");
        }
        if cell.0 == ID_LETTERS_R3_PERIOD {
            return Cow::Borrowed(".");
        }
        if cell.0 == ID_LETTERS_R3_SLASH {
            return Cow::Borrowed("/");
        }
        if cell.0 == ID_LETTERS_R3_BACKSPACE {
            return Cow::Borrowed("⌫");
        }

        // Symbols layer.
        if (ID_SYMBOLS_R1_START..ID_SYMBOLS_R1_START + SYMBOLS_R1.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R1_START) as usize;
            return Cow::Borrowed(SYMBOLS_R1[i].0);
        }
        if (ID_SYMBOLS_R2_START..ID_SYMBOLS_R2_START + SYMBOLS_R2.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R2_START) as usize;
            return Cow::Borrowed(SYMBOLS_R2[i].0);
        }
        if cell.0 == ID_SYMBOLS_R2_BACKSPACE {
            return Cow::Borrowed("⌫");
        }
        if (ID_SYMBOLS_R3_START..ID_SYMBOLS_R3_START + SYMBOLS_R3.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R3_START) as usize;
            return Cow::Borrowed(SYMBOLS_R3[i].0);
        }

        // Functions layer.
        const FN_LABELS: [&str; 12] = [
            "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
        ];
        if (ID_FUNCTIONS_R1_START..ID_FUNCTIONS_R1_START + 10).contains(&cell.0) {
            let i = (cell.0 - ID_FUNCTIONS_R1_START) as usize;
            return Cow::Borrowed(FN_LABELS[i]);
        }
        if cell.0 == ID_FUNCTIONS_R2_F11 {
            return Cow::Borrowed(FN_LABELS[10]);
        }
        if cell.0 == ID_FUNCTIONS_R2_F12 {
            return Cow::Borrowed(FN_LABELS[11]);
        }
        if (ID_FUNCTIONS_R2_INERT_START..ID_FUNCTIONS_R2_INERT_START + 7).contains(&cell.0) {
            return Cow::Borrowed("");
        }
        if cell.0 == ID_FUNCTIONS_R2_BACKSPACE {
            return Cow::Borrowed("⌫");
        }

        Cow::Borrowed("")
    }

    fn emit(&self, cell: CellId, mods: &KeyboardModifiers) -> KeyAction {
        // Extras strip / terminal affordances.
        match cell.0 {
            ID_ESC => return send_bare(BareKey::Esc),
            ID_TAB => return send_bare(BareKey::Tab),
            ID_CTL => return KeyAction::ToggleModifier(Modifier::Ctrl),
            ID_ALT => return KeyAction::ToggleModifier(Modifier::Alt),
            ID_ARROW_LEFT => return send_bare(BareKey::Left),
            ID_ARROW_DOWN => return send_bare(BareKey::Down),
            ID_ARROW_UP => return send_bare(BareKey::Up),
            ID_ARROW_RIGHT => return send_bare(BareKey::Right),
            _ => {},
        }

        // Bottom bar.
        if cell.0 == ID_BOTTOM_TOGGLE {
            return match mods.layer {
                KeyLayer::Letters => KeyAction::SwitchLayer(KeyLayer::Symbols),
                KeyLayer::Symbols | KeyLayer::Functions => {
                    KeyAction::SwitchLayer(KeyLayer::Letters)
                },
            };
        }
        if cell.0 == ID_BOTTOM_FN {
            return match mods.layer {
                KeyLayer::Letters | KeyLayer::Symbols => {
                    KeyAction::SwitchLayer(KeyLayer::Functions)
                },
                KeyLayer::Functions => KeyAction::SwitchLayer(KeyLayer::Letters),
            };
        }
        if cell.0 == ID_BOTTOM_SPACE {
            return send_char(' ');
        }
        if cell.0 == ID_BOTTOM_ENTER {
            return send_bare(BareKey::Enter);
        }

        // Letters layer printables.
        if (ID_LETTERS_R1_START..=ID_LETTERS_R1_START + 9).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R1_START) as usize;
            let (_, _, lo, up) = LETTERS_R1[i];
            return send_char(if mods.shift_armed { up } else { lo });
        }
        if (ID_LETTERS_R2_START..=ID_LETTERS_R2_START + 8).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R2_START) as usize;
            let (_, _, lo, up) = LETTERS_R2[i];
            return send_char(if mods.shift_armed { up } else { lo });
        }
        if (ID_LETTERS_R3_LETTERS_START..=ID_LETTERS_R3_LETTERS_START + 6).contains(&cell.0) {
            let i = (cell.0 - ID_LETTERS_R3_LETTERS_START) as usize;
            let (_, _, lo, up) = LETTERS_R3_LETTERS[i];
            return send_char(if mods.shift_armed { up } else { lo });
        }
        if cell.0 == ID_LETTERS_R3_SHIFT {
            return KeyAction::ToggleModifier(Modifier::Shift);
        }
        if cell.0 == ID_LETTERS_R3_PERIOD {
            return send_char('.');
        }
        if cell.0 == ID_LETTERS_R3_SLASH {
            return send_char('/');
        }
        if cell.0 == ID_LETTERS_R3_BACKSPACE {
            return send_bare(BareKey::Backspace);
        }

        // Symbols layer.
        if (ID_SYMBOLS_R1_START..ID_SYMBOLS_R1_START + SYMBOLS_R1.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R1_START) as usize;
            return send_char(SYMBOLS_R1[i].1);
        }
        if (ID_SYMBOLS_R2_START..ID_SYMBOLS_R2_START + SYMBOLS_R2.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R2_START) as usize;
            return send_char(SYMBOLS_R2[i].1);
        }
        if cell.0 == ID_SYMBOLS_R2_BACKSPACE {
            return send_bare(BareKey::Backspace);
        }
        if (ID_SYMBOLS_R3_START..ID_SYMBOLS_R3_START + SYMBOLS_R3.len() as u16).contains(&cell.0) {
            let i = (cell.0 - ID_SYMBOLS_R3_START) as usize;
            return send_char(SYMBOLS_R3[i].1);
        }

        // Functions layer.
        if (ID_FUNCTIONS_R1_START..ID_FUNCTIONS_R1_START + 10).contains(&cell.0) {
            let n = (cell.0 - ID_FUNCTIONS_R1_START + 1) as u8;
            return send_bare(BareKey::F(n));
        }
        if cell.0 == ID_FUNCTIONS_R2_F11 {
            return send_bare(BareKey::F(11));
        }
        if cell.0 == ID_FUNCTIONS_R2_F12 {
            return send_bare(BareKey::F(12));
        }
        if (ID_FUNCTIONS_R2_INERT_START..ID_FUNCTIONS_R2_INERT_START + 7).contains(&cell.0) {
            return KeyAction::NoOp;
        }
        if cell.0 == ID_FUNCTIONS_R2_BACKSPACE {
            return send_bare(BareKey::Backspace);
        }

        KeyAction::NoOp
    }

    fn modifier_of(&self, cell: CellId) -> Option<Modifier> {
        match cell.0 {
            ID_CTL => Some(Modifier::Ctrl),
            ID_ALT => Some(Modifier::Alt),
            ID_LETTERS_R3_SHIFT => Some(Modifier::Shift),
            _ => None,
        }
    }

    fn layer_of(&self, cell: CellId) -> Option<KeyLayer> {
        if cell.0 == ID_BOTTOM_FN {
            Some(KeyLayer::Functions)
        } else {
            None
        }
    }
}

fn send_char(c: char) -> KeyAction {
    KeyAction::SendKey(KeyWithModifier {
        bare_key: BareKey::Char(c),
        key_modifiers: std::collections::BTreeSet::new(),
    })
}

fn send_bare(b: BareKey) -> KeyAction {
    KeyAction::SendKey(KeyWithModifier {
        bare_key: b,
        key_modifiers: std::collections::BTreeSet::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `edge_first_widths` produces the canonical distributions used
    /// throughout the layout.
    #[test]
    fn edge_first_widths_canonical_cases() {
        assert_eq!(
            edge_first_widths(28, 10),
            vec![2, 3, 3, 3, 3, 3, 3, 3, 3, 2],
        );
        assert_eq!(
            edge_first_widths(28, 11),
            vec![2, 2, 3, 3, 3, 2, 3, 3, 3, 2, 2],
        );
        assert_eq!(
            edge_first_widths(28, 6),
            vec![4, 5, 5, 5, 5, 4],
        );
        assert_eq!(
            edge_first_widths(40, 10),
            vec![4, 4, 4, 4, 4, 4, 4, 4, 4, 4],
        );
    }

    /// Every `CellId` returned by `rows()` must resolve under
    /// `label()` and `emit()` on every layer.
    #[test]
    fn every_cell_resolves_on_every_layer() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers {
                layer,
                ..KeyboardModifiers::default()
            };
            let rows = layout.rows(&mods, 40);
            assert!(!rows.is_empty(), "no rows for layer {:?}", layer);
            for row in &rows {
                for cell in &row.cells {
                    let _label = layout.label(cell.id, &mods);
                    let _action = layout.emit(cell.id, &mods);
                }
            }
        }
    }

    /// Modifier cells must each report exactly one `Modifier` from
    /// `modifier_of`; non-modifier cells must report `None`.
    #[test]
    fn modifier_cells_round_trip() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let mut shift = 0;
        let mut ctrl = 0;
        let mut alt = 0;
        for row in layout.rows(&mods, 40) {
            for cell in &row.cells {
                match layout.modifier_of(cell.id) {
                    Some(Modifier::Shift) => shift += 1,
                    Some(Modifier::Ctrl) => ctrl += 1,
                    Some(Modifier::Alt) => alt += 1,
                    None => {},
                }
            }
        }
        assert_eq!(shift, 1);
        assert_eq!(ctrl, 1);
        assert_eq!(alt, 1);
    }

    /// Tapping a letter while Shift is armed must emit the uppercase
    /// variant.
    #[test]
    fn shifted_letters_emit_uppercase() {
        let layout = UsQwerty::new();
        let mut shifted = KeyboardModifiers::default();
        shifted.shift_armed = true;
        for (c, expected) in [('q', 'Q'), ('a', 'A'), ('z', 'Z'), ('m', 'M')] {
            let cell = find_cell_emitting(&layout, &KeyboardModifiers::default(), c)
                .expect("default letter cell");
            match layout.emit(cell, &shifted) {
                KeyAction::SendKey(k) => assert_eq!(k.bare_key, BareKey::Char(expected)),
                other => panic!("expected SendKey, got {:?}", other),
            }
        }
    }

    /// Bottom-bar toggle on the Letters layer reads `?123` and emits
    /// `SwitchLayer(Symbols)`; from Symbols/Functions it reads `ABC`.
    #[test]
    fn toggle_cell_label_and_action_flip_with_layer() {
        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        assert_eq!(layout.label(CellId(ID_BOTTOM_TOGGLE), &mods).as_ref(), "?123");
        assert!(matches!(
            layout.emit(CellId(ID_BOTTOM_TOGGLE), &mods),
            KeyAction::SwitchLayer(KeyLayer::Symbols)
        ));
        mods.layer = KeyLayer::Symbols;
        assert_eq!(layout.label(CellId(ID_BOTTOM_TOGGLE), &mods).as_ref(), "ABC");
        assert!(matches!(
            layout.emit(CellId(ID_BOTTOM_TOGGLE), &mods),
            KeyAction::SwitchLayer(KeyLayer::Letters)
        ));
        mods.layer = KeyLayer::Functions;
        assert_eq!(layout.label(CellId(ID_BOTTOM_TOGGLE), &mods).as_ref(), "ABC");
        assert!(matches!(
            layout.emit(CellId(ID_BOTTOM_TOGGLE), &mods),
            KeyAction::SwitchLayer(KeyLayer::Letters)
        ));
    }

    #[test]
    fn fn_cell_layer_of_returns_functions() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        assert_eq!(layout.layer_of(CellId(ID_BOTTOM_FN)), Some(KeyLayer::Functions));
        assert_eq!(layout.layer_of(CellId(ID_BOTTOM_TOGGLE)), None);
        for row in layout.rows(&mods, 40) {
            for cell in &row.cells {
                if cell.id.0 == ID_BOTTOM_FN {
                    continue;
                }
                assert_eq!(layout.layer_of(cell.id), None, "cell {:?}", cell.id);
            }
        }
    }

    /// Natural Letters layer has 5 rows; Symbols 5 rows; Functions 4 rows.
    #[test]
    fn row_counts_per_layer() {
        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        assert_eq!(layout.rows(&mods, 40).len(), 5);
        mods.layer = KeyLayer::Symbols;
        assert_eq!(layout.rows(&mods, 40).len(), 5);
        mods.layer = KeyLayer::Functions;
        assert_eq!(layout.rows(&mods, 40).len(), 4);
    }

    /// All natural-tier letter rows start at column 0 — no stagger.
    /// R2 fills the full target width so its cells are as wide as the
    /// row allows.
    #[test]
    fn letters_rows_have_no_stagger() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.rows(&mods, 40);
        assert_eq!(rows[1].offset_col, 0); // R1
        assert_eq!(rows[2].offset_col, 0); // R2
        assert_eq!(rows[3].offset_col, 0); // R3
    }

    /// At 40 cols natural, every Letters R1 cell is width 4 (10*4=40
    /// exactly — Variant B's signature property).
    #[test]
    fn natural_letters_r1_uniform_at_40() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.rows(&mods, 40);
        for cell in &rows[1].cells {
            assert_eq!(cell.col_end - cell.col_start, 4);
        }
        assert_eq!(rows[1].cells.last().unwrap().col_end, 40);
    }

    /// At 40 cols natural, every row spans exactly 40 cols (offset +
    /// last cell + right_pad).
    #[test]
    fn natural_rows_fill_target_block() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            for (i, row) in layout.rows(&mods, 40).iter().enumerate() {
                let last_end = row.cells.last().map(|c| c.col_end).unwrap_or(0);
                let extent = row.offset_col + last_end + row.right_pad;
                assert_eq!(extent, 40, "{:?} row {} extent={}", layer, i, extent);
            }
        }
    }

    /// Natural-tier Letters R3 never emits an anchor cell narrower
    /// than 2 cols. Anchors are shift, period, slash, backspace.
    /// (Letter cells inherit `lw` from R1 and may be 1 wide at very
    /// narrow widths; that is a separate concern.)
    #[test]
    fn natural_letters_r3_anchors_at_least_two_wide() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        for target in 13u16..=80 {
            let rows = layout.rows(&mods, target);
            let r3 = &rows[3];
            for cell in &r3.cells {
                let is_letter = (ID_LETTERS_R3_LETTERS_START
                    ..ID_LETTERS_R3_LETTERS_START + LETTERS_R3_LETTERS.len() as u16)
                    .contains(&cell.id.0);
                if is_letter {
                    continue;
                }
                let w = cell.col_end - cell.col_start;
                assert!(
                    w >= 2,
                    "target={} anchor id={} width={} < 2",
                    target,
                    cell.id.0,
                    w,
                );
            }
        }
    }

    /// At target widths where the anchor budget can't host all four
    /// anchors at >= 2 cols (target = 20, 21 with lw=2), Letters R3
    /// drops `.` and `/` and emits the 9-cell ⇧ + 7 letters + ⌫ row.
    #[test]
    fn natural_letters_r3_drops_punctuation_when_tight() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        for target in [20u16, 21] {
            let rows = layout.rows(&mods, target);
            let r3 = &rows[3];
            assert_eq!(r3.cells.len(), 9, "target {target}");
            for cell in &r3.cells {
                assert_ne!(cell.id.0, ID_LETTERS_R3_PERIOD, "target {target}");
                assert_ne!(cell.id.0, ID_LETTERS_R3_SLASH, "target {target}");
            }
        }
    }

    /// At target widths where the budget fits all four anchors at
    /// >= 2 cols (anchor_budget >= 8), Letters R3 keeps the full
    /// 11-cell layout with `.` and `/` present.
    #[test]
    fn natural_letters_r3_keeps_punctuation_when_fits() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        for target in [22u16, 25, 30, 40, 60] {
            let rows = layout.rows(&mods, target);
            let r3 = &rows[3];
            assert_eq!(r3.cells.len(), 11, "target {target}");
            let has_period = r3.cells.iter().any(|c| c.id.0 == ID_LETTERS_R3_PERIOD);
            let has_slash = r3.cells.iter().any(|c| c.id.0 == ID_LETTERS_R3_SLASH);
            assert!(has_period, "period absent at target {target}");
            assert!(has_slash, "slash absent at target {target}");
        }
    }

    /// Natural Letters R3 punctuation cells emit their literal chars
    /// regardless of Shift state.
    #[test]
    fn letters_row_3_punctuation_emits_literal() {
        let layout = UsQwerty::new();
        let mut shifted = KeyboardModifiers::default();
        shifted.shift_armed = true;
        for (id, ch) in [
            (ID_LETTERS_R3_PERIOD, '.'),
            (ID_LETTERS_R3_SLASH, '/'),
        ] {
            for m in [&KeyboardModifiers::default(), &shifted] {
                match layout.emit(CellId(id), m) {
                    KeyAction::SendKey(k) => assert_eq!(k.bare_key, BareKey::Char(ch)),
                    other => panic!("expected SendKey({ch:?}), got {other:?}"),
                }
            }
        }
    }

    /// Inert filler cells in Functions row 2 emit `NoOp` and have an
    /// empty label.
    #[test]
    fn functions_row_2_inert_cells_are_noop() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Functions,
            ..KeyboardModifiers::default()
        };
        for i in 0..7u16 {
            let id = CellId(ID_FUNCTIONS_R2_INERT_START + i);
            assert!(matches!(layout.emit(id, &mods), KeyAction::NoOp));
            assert_eq!(layout.label(id, &mods).as_ref(), "");
        }
    }

    fn find_cell_emitting(
        layout: &UsQwerty,
        mods: &KeyboardModifiers,
        c: char,
    ) -> Option<CellId> {
        for row in layout.rows(mods, 40) {
            for cell in &row.cells {
                if let KeyAction::SendKey(k) = layout.emit(cell.id, mods) {
                    if k.bare_key == BareKey::Char(c) {
                        return Some(cell.id);
                    }
                }
            }
        }
        None
    }

    // -----------------------------------------------------------------
    // Compact-tier tests.
    // -----------------------------------------------------------------

    /// Compact layers all expose exactly 4 rows.
    #[test]
    fn compact_rows_count_per_layer() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            assert_eq!(layout.compact_rows(&mods, 28).len(), 4, "{:?}", layer);
        }
    }

    /// At 28 cols compact, Letters R1 follows the edge-first pattern
    /// `[2,3,3,3,3,3,3,3,3,2]`.
    #[test]
    fn compact_letters_r1_widths_at_28() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, 28);
        let widths: Vec<u16> = rows[0]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![2, 3, 3, 3, 3, 3, 3, 3, 3, 2]);
    }

    /// At 28 cols compact, Letters R2 letters are uniform width 3
    /// (9*3=27 + 1 offset = 28).
    #[test]
    fn compact_letters_r2_widths_at_28() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, 28);
        let widths: Vec<u16> = rows[1]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3; 9]);
        assert_eq!(rows[1].offset_col, 1);
    }

    /// At 28 cols compact, Letters R3 letters are uniform width 3;
    /// anchors absorb the rest (⇧=3, ⌫=4).
    #[test]
    fn compact_letters_r3_widths_at_28() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, 28);
        let widths: Vec<u16> = rows[2]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3, 3, 3, 3, 3, 3, 3, 3, 4]);
    }

    /// Compact Symbols layer has exactly 4 rows AND exactly one ⌫
    /// (regression: an earlier mock had two ⌫ on the Symbols layer).
    #[test]
    fn compact_symbols_has_exactly_one_backspace() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Symbols,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, 28);
        assert_eq!(rows.len(), 4);
        let mut bs_count = 0;
        for row in &rows {
            for cell in &row.cells {
                if cell.id.0 == ID_SYMBOLS_R2_BACKSPACE
                    || cell.id.0 == ID_LETTERS_R3_BACKSPACE
                    || cell.id.0 == ID_FUNCTIONS_R2_BACKSPACE
                {
                    bs_count += 1;
                }
            }
        }
        assert_eq!(bs_count, 1, "compact Symbols layer should have one ⌫");
    }

    /// Compact Symbols top row carries the terminal affordances
    /// (Esc / Tab / Ctl / Alt + arrows).
    #[test]
    fn compact_symbols_top_row_carries_terminal_affordances() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Symbols,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, 28);
        let ids: Vec<u16> = rows[0].cells.iter().map(|c| c.id.0).collect();
        assert_eq!(
            ids,
            vec![
                ID_ESC,
                ID_TAB,
                ID_CTL,
                ID_ALT,
                ID_ARROW_LEFT,
                ID_ARROW_DOWN,
                ID_ARROW_UP,
                ID_ARROW_RIGHT,
            ],
        );
    }

    /// Every compact row's extent equals the target_block_width.
    #[test]
    fn compact_rows_fill_target_block() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            for (i, row) in layout.compact_rows(&mods, 28).iter().enumerate() {
                let last_end = row.cells.last().map(|c| c.col_end).unwrap_or(0);
                let extent = row.offset_col + last_end + row.right_pad;
                assert_eq!(extent, 28, "{:?} row {} extent={}", layer, i, extent);
            }
        }
    }

    /// Compact Functions layer carries F1..F12 across two 6-cell rows
    /// plus the terminal-affordances mirror.
    #[test]
    fn compact_functions_layout_is_complete() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Functions,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, 28);
        assert_eq!(rows[0].cells.len(), 6);
        assert_eq!(rows[1].cells.len(), 6);
        // R2 carries F11 / F12 as the last two cells.
        let last_two: Vec<u16> = rows[1].cells.iter().rev().take(2).map(|c| c.id.0).collect();
        assert!(last_two.contains(&ID_FUNCTIONS_R2_F11));
        assert!(last_two.contains(&ID_FUNCTIONS_R2_F12));
    }
}
