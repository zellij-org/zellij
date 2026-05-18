//! US-QWERTY layout. The only layout shipped in v1.
//!
//! Three layers visible to the user, switched via the bottom bar's
//! toggle cell and Fn cell:
//!
//! - **Letters** — staggered `qwerty` / `asdf` / `zxcv` with a wide
//!   backspace on row 3. Shift uppercases the letter labels.
//! - **Symbols** (`?123`) — digits + shell punctuation, no stagger.
//! - **Functions** (`Fn`) — F1–F12 with seven inert filler cells.
//!
//! Two persistent rows surround every layer: an `extras` strip on top
//! (Esc/Tab/Ctl/Alt + four arrows) and a `bottom bar` on the bottom
//! (layer toggle, Fn toggle, space, ↵).
//!
//! Cell IDs are stable u16s spanning the union of all layers' cells;
//! the controller and renderer never depend on the numbering scheme,
//! they round-trip the opaque `CellId` only.

use std::borrow::Cow;
use std::collections::HashMap;

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

// Extras strip cells.
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
// IDs 30..=37 cover ⇧ and the seven letters z..m. The two new
// punctuation cells take IDs 38 / 39; the backspace moves to ID 79
// (the unused slot between SYMBOLS_R3 = ..=78 and FUNCTIONS_R1 =
// 80..) so the cell-ID space stays gap-friendly for future additions.
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

/// Canonical natural-block width of every compact-tier row in
/// cells. Each row is laid out so its column extents sum to this
/// value; the renderer applies a per-row scale of
/// `(target_block_width, COMPACT_NATURAL_BLOCK)` to stretch the
/// canonical block to the actual available width on viewports
/// wider than 28 cols. Matches the visual reference in
/// `compact_keyboard_mock.ansi` at the canonical 28-col target.
pub(super) const COMPACT_NATURAL_BLOCK: u16 = 26;

// -------------------------------------------------------------------
// Layout content tables. Static `&'static str` labels so `label()`
// returns borrowed Cows without allocating.
// -------------------------------------------------------------------

/// Letters row 1 (`q`..`p`). Index = cell id - ID_LETTERS_R1_START.
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
// Per-cell static data assembled at construction time.
// -------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct CellSpec {
    id: CellId,
    col_start: u16,
    col_end: u16,
}

pub struct UsQwerty {
    /// Lookup: `CellId.0` → cell metadata. The per-row helpers iterate
    /// statically-known id ranges to assemble `KeyRow`s; this map
    /// answers per-cell queries from `label`/`emit`/`modifier_of`/
    /// `layer_of` in O(1).
    cells: HashMap<u16, CellSpec>,
}

impl Default for UsQwerty {
    fn default() -> Self {
        Self::new()
    }
}

impl UsQwerty {
    pub fn new() -> Self {
        let mut cells: HashMap<u16, CellSpec> = HashMap::new();

        // ---- Extras strip (offset 0): 4×5 + 4×3 = 32 cols.
        // Esc(5) Tab(5) Ctl(5) Alt(5) ←(3) ↓(3) ↑(3) →(3)
        let extras_widths = [
            (ID_ESC, 5u16),
            (ID_TAB, 5),
            (ID_CTL, 5),
            (ID_ALT, 5),
            (ID_ARROW_LEFT, 3),
            (ID_ARROW_DOWN, 3),
            (ID_ARROW_UP, 3),
            (ID_ARROW_RIGHT, 3),
        ];
        push_row(&mut cells, &extras_widths);

        // ---- Letters row 1: 10 cells × 3 cols = 30. Offset 0.
        let r1_widths: Vec<(u16, u16)> = (0..LETTERS_R1.len() as u16)
            .map(|i| (ID_LETTERS_R1_START + i, 3u16))
            .collect();
        push_row(&mut cells, &r1_widths);

        // ---- Letters row 2: 9 cells × 3 cols = 27. Offset 2.
        let r2_widths: Vec<(u16, u16)> = (0..LETTERS_R2.len() as u16)
            .map(|i| (ID_LETTERS_R2_START + i, 3u16))
            .collect();
        push_row(&mut cells, &r2_widths);

        // ---- Letters row 3: ⇧(3) + 7×3 + .(3) + /(3) + ⌫(4) = 34.
        // Offset 0. The backspace shrank from its original 7 cols to
        // make room for the new `.` and `/` cells; it is left one
        // column wider than the surrounding letter cells so the
        // `center()` helper places the glyph with an asymmetric extra
        // space to its right — visible right-padding inside the cell's
        // bg shade. Row width (34) matches the Letters bottom bar so
        // the layer's `block_width` and centering are unchanged.
        let mut r3_widths: Vec<(u16, u16)> = vec![(ID_LETTERS_R3_SHIFT, 3)];
        for i in 0..LETTERS_R3_LETTERS.len() as u16 {
            r3_widths.push((ID_LETTERS_R3_LETTERS_START + i, 3));
        }
        r3_widths.push((ID_LETTERS_R3_PERIOD, 3));
        r3_widths.push((ID_LETTERS_R3_SLASH, 3));
        r3_widths.push((ID_LETTERS_R3_BACKSPACE, 4));
        push_row(&mut cells, &r3_widths);

        // ---- Symbols rows: 13 cells × 3 = 39 each.
        let s1_widths: Vec<(u16, u16)> = (0..SYMBOLS_R1.len() as u16)
            .map(|i| (ID_SYMBOLS_R1_START + i, 3u16))
            .collect();
        push_row(&mut cells, &s1_widths);

        let mut s2_widths: Vec<(u16, u16)> = (0..SYMBOLS_R2.len() as u16)
            .map(|i| (ID_SYMBOLS_R2_START + i, 3u16))
            .collect();
        s2_widths.push((ID_SYMBOLS_R2_BACKSPACE, 3));
        push_row(&mut cells, &s2_widths);

        let s3_widths: Vec<(u16, u16)> = (0..SYMBOLS_R3.len() as u16)
            .map(|i| (ID_SYMBOLS_R3_START + i, 3u16))
            .collect();
        push_row(&mut cells, &s3_widths);

        // ---- Functions row 1: F1..F9 (4 cols each) + F10 (5). Total
        // 9×4 + 5 = 41.
        let mut fn1_widths: Vec<(u16, u16)> = Vec::with_capacity(10);
        for i in 0..9u16 {
            fn1_widths.push((ID_FUNCTIONS_R1_START + i, 4));
        }
        fn1_widths.push((ID_FUNCTIONS_R1_START + 9, 5));
        push_row(&mut cells, &fn1_widths);

        // ---- Functions row 2: F11(5) F12(5) 7×inert(4) ⌫(3) = 41.
        let mut fn2_widths: Vec<(u16, u16)> = vec![
            (ID_FUNCTIONS_R2_F11, 5),
            (ID_FUNCTIONS_R2_F12, 5),
        ];
        for i in 0..7u16 {
            fn2_widths.push((ID_FUNCTIONS_R2_INERT_START + i, 4));
        }
        fn2_widths.push((ID_FUNCTIONS_R2_BACKSPACE, 3));
        push_row(&mut cells, &fn2_widths);

        // Bottom-bar cells are layered-dependent in width: the toggle
        // cell shrinks from 6 (`?123`) to 5 (`ABC`) when leaving the
        // Letters layer. The Symbols/Functions geometry is stored in
        // `cells`; the Letters override is applied in
        // `bottom_bar_for_layer()` so a single `CellId` covers both.
        let bottom_widths = [
            (ID_BOTTOM_TOGGLE, 5u16),
            (ID_BOTTOM_FN, 4),
            (ID_BOTTOM_SPACE, 21),
            (ID_BOTTOM_ENTER, 3),
        ];
        push_row(&mut cells, &bottom_widths);

        Self { cells }
    }

    fn cell_spec(&self, id: CellId) -> Option<CellSpec> {
        self.cells.get(&id.0).copied()
    }

    fn key_cell(&self, id: u16) -> KeyCell {
        let spec = self
            .cells
            .get(&id)
            .copied()
            .unwrap_or(CellSpec {
                id: CellId(id),
                col_start: 0,
                col_end: 0,
            });
        KeyCell {
            col_start: spec.col_start,
            col_end: spec.col_end,
            id: spec.id,
        }
    }

    fn extras_row(&self) -> KeyRow {
        let cells = [
            ID_ESC,
            ID_TAB,
            ID_CTL,
            ID_ALT,
            ID_ARROW_LEFT,
            ID_ARROW_DOWN,
            ID_ARROW_UP,
            ID_ARROW_RIGHT,
        ]
        .iter()
        .map(|id| self.key_cell(*id))
        .collect();
        KeyRow::tall(0, cells)
    }

    fn letters_row_1(&self) -> KeyRow {
        let cells = (0..LETTERS_R1.len() as u16)
            .map(|i| self.key_cell(ID_LETTERS_R1_START + i))
            .collect();
        KeyRow::tall(0, cells)
    }

    fn letters_row_2(&self) -> KeyRow {
        let cells = (0..LETTERS_R2.len() as u16)
            .map(|i| self.key_cell(ID_LETTERS_R2_START + i))
            .collect();
        KeyRow::tall(2, cells)
    }

    fn letters_row_3(&self) -> KeyRow {
        let mut cells = vec![self.key_cell(ID_LETTERS_R3_SHIFT)];
        for i in 0..LETTERS_R3_LETTERS.len() as u16 {
            cells.push(self.key_cell(ID_LETTERS_R3_LETTERS_START + i));
        }
        cells.push(self.key_cell(ID_LETTERS_R3_PERIOD));
        cells.push(self.key_cell(ID_LETTERS_R3_SLASH));
        cells.push(self.key_cell(ID_LETTERS_R3_BACKSPACE));
        KeyRow::tall(0, cells)
    }

    fn symbols_row_1(&self) -> KeyRow {
        let cells = (0..SYMBOLS_R1.len() as u16)
            .map(|i| self.key_cell(ID_SYMBOLS_R1_START + i))
            .collect();
        KeyRow::tall(0, cells)
    }

    fn symbols_row_2(&self) -> KeyRow {
        let mut cells: Vec<KeyCell> = (0..SYMBOLS_R2.len() as u16)
            .map(|i| self.key_cell(ID_SYMBOLS_R2_START + i))
            .collect();
        cells.push(self.key_cell(ID_SYMBOLS_R2_BACKSPACE));
        KeyRow::tall(0, cells)
    }

    fn symbols_row_3(&self) -> KeyRow {
        let cells = (0..SYMBOLS_R3.len() as u16)
            .map(|i| self.key_cell(ID_SYMBOLS_R3_START + i))
            .collect();
        KeyRow::tall(0, cells)
    }

    fn functions_row_1(&self) -> KeyRow {
        let cells = (0..10u16)
            .map(|i| self.key_cell(ID_FUNCTIONS_R1_START + i))
            .collect();
        KeyRow::tall(0, cells)
    }

    fn functions_row_2(&self) -> KeyRow {
        let mut cells = vec![
            self.key_cell(ID_FUNCTIONS_R2_F11),
            self.key_cell(ID_FUNCTIONS_R2_F12),
        ];
        for i in 0..7u16 {
            cells.push(self.key_cell(ID_FUNCTIONS_R2_INERT_START + i));
        }
        cells.push(self.key_cell(ID_FUNCTIONS_R2_BACKSPACE));
        KeyRow::tall(0, cells)
    }

    // ---------------------------------------------------------------
    // Compact-tier rows (narrow-viewport variant).
    //
    // Each row's column extents sum to `COMPACT_NATURAL_BLOCK` so the
    // renderer's per-row scale primitive can stretch them to the
    // available width with a single ratio. Cells are constructed
    // afresh because their `col_start`/`col_end` differ from the
    // natural-tier widths stored in `self.cells` — the existing
    // `CellId`s are reused, which is enough for `label()`, `emit()`,
    // `modifier_of()`, and `layer_of()` to keep working unchanged.
    //
    // Widths reproduce the visual reference in
    // `compact_keyboard_mock.ansi` at the canonical 28-col target.
    // ---------------------------------------------------------------

    fn compact_letters_row_1(&self) -> KeyRow {
        // q w e r t y u i o p — 10 cells × widths [3,2,3,2,3,2,3,2,3,3] = 26.
        let widths = [3u16, 2, 3, 2, 3, 2, 3, 2, 3, 3];
        let ids_widths: Vec<(u16, u16)> = (0..LETTERS_R1.len() as u16)
            .map(|i| (ID_LETTERS_R1_START + i, widths[i as usize]))
            .collect();
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_letters_row_2(&self) -> KeyRow {
        // a s d f g h j k l — 9 cells × widths [3,2,3,3,2,3,2,3,3] = 24
        // with offset 2 → row natural extent 26.
        let widths = [3u16, 2, 3, 3, 2, 3, 2, 3, 3];
        let ids_widths: Vec<(u16, u16)> = (0..LETTERS_R2.len() as u16)
            .map(|i| (ID_LETTERS_R2_START + i, widths[i as usize]))
            .collect();
        KeyRow::tall(2, lay_out_compact_cells(&ids_widths))
    }

    fn compact_letters_row_3(&self) -> KeyRow {
        // ⇧ z x c v b n m ⌫ — 9 cells with widths
        // [3, 2,3,2,3,2,3,2, 6] = 26. ⇧ and ⌫ are wider anchors; the
        // seven letter cells in the middle alternate 2/3 to match the
        // mock's per-cell widths.
        let letter_widths = [2u16, 3, 2, 3, 2, 3, 2];
        let mut ids_widths: Vec<(u16, u16)> = Vec::with_capacity(9);
        ids_widths.push((ID_LETTERS_R3_SHIFT, 3));
        for i in 0..LETTERS_R3_LETTERS.len() as u16 {
            ids_widths.push((
                ID_LETTERS_R3_LETTERS_START + i,
                letter_widths[i as usize],
            ));
        }
        ids_widths.push((ID_LETTERS_R3_BACKSPACE, 6));
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_symbols_row_1(&self) -> KeyRow {
        // Esc Tab Ctl Alt ← ↓ ↑ → — terminal affordances live here in
        // the compact tier (the natural-tier extras strip is removed).
        // Widths [4,4,4,4, 2,2,2, 4] = 26.
        let ids_widths = [
            (ID_ESC, 4u16),
            (ID_TAB, 4),
            (ID_CTL, 4),
            (ID_ALT, 4),
            (ID_ARROW_LEFT, 2),
            (ID_ARROW_DOWN, 2),
            (ID_ARROW_UP, 2),
            (ID_ARROW_RIGHT, 4),
        ];
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_symbols_row_2(&self) -> KeyRow {
        // 1 2 3 4 5 6 7 8 9 0 — 10 cells × widths [3,2,3,2,3,2,3,2,3,3]
        // = 26. Same pattern as compact Letters R1 so column edges
        // line up vertically between layers.
        let widths = [3u16, 2, 3, 2, 3, 2, 3, 2, 3, 3];
        let ids_widths: Vec<(u16, u16)> = (0..10u16)
            .map(|i| (ID_SYMBOLS_R1_START + i, widths[i as usize]))
            .collect();
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_symbols_row_3(&self) -> KeyRow {
        // / \ : ; | < > ? . ⌫ — 10 cells × widths
        // [2,2,3,3,2,3,3,2,2,4] = 26. The "." is sourced from
        // SYMBOLS_R3[11] (which the natural-tier punctuation table
        // already encodes). The backspace re-uses
        // ID_SYMBOLS_R2_BACKSPACE so the existing label/emit handling
        // continues to work.
        let widths = [2u16, 2, 3, 3, 2, 3, 3, 2, 2, 4];
        let mut ids_widths: Vec<(u16, u16)> = Vec::with_capacity(10);
        for i in 0..8u16 {
            ids_widths.push((ID_SYMBOLS_R3_START + i, widths[i as usize]));
        }
        // `.` is index 11 in SYMBOLS_R3 (after `'`, `"`, `,`).
        ids_widths.push((ID_SYMBOLS_R3_START + 11, widths[8]));
        ids_widths.push((ID_SYMBOLS_R2_BACKSPACE, widths[9]));
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_functions_row_1(&self) -> KeyRow {
        // F1 F2 F3 F4 F5 F6 — widths [4,4,5,4,4,5] = 26. F3 and F6
        // take the extra cell so their column edges line up with the
        // wider F-keys in the row below.
        let widths = [4u16, 4, 5, 4, 4, 5];
        let ids_widths: Vec<(u16, u16)> = (0..6u16)
            .map(|i| (ID_FUNCTIONS_R1_START + i, widths[i as usize]))
            .collect();
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_functions_row_2(&self) -> KeyRow {
        // F7 F8 F9 F10 F11 F12 — widths [4,4,5,4,4,5] = 26. F7..F9
        // re-use the natural-tier IDs (ID_FUNCTIONS_R1_START + 6..8);
        // F10 also lives in that range. F11/F12 keep their dedicated
        // IDs.
        let widths = [4u16, 4, 5, 4, 4, 5];
        let ids = [
            ID_FUNCTIONS_R1_START + 6, // F7
            ID_FUNCTIONS_R1_START + 7, // F8
            ID_FUNCTIONS_R1_START + 8, // F9
            ID_FUNCTIONS_R1_START + 9, // F10
            ID_FUNCTIONS_R2_F11,
            ID_FUNCTIONS_R2_F12,
        ];
        let ids_widths: Vec<(u16, u16)> = ids
            .iter()
            .zip(widths.iter())
            .map(|(id, w)| (*id, *w))
            .collect();
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    fn compact_functions_row_3(&self) -> KeyRow {
        // Mirror of the compact Symbols R1 so Esc/arrows stay
        // reachable while editing F-keys.
        self.compact_symbols_row_1()
    }

    /// Bottom bar for the compact tier. Letters keeps the wider
    /// `?123` label; Symbols / Functions show `ABC`. Widths differ
    /// from the natural-tier bottom bar (which is 34 cells natural,
    /// not 26) so the compact bottom-bar cells must be constructed
    /// fresh.
    fn compact_bottom_bar(&self, layer: KeyLayer) -> KeyRow {
        // Letters: ?123(5) Fn(3) space(15) ↵(3) = 26
        // Symbols/Functions: ABC(4) Fn(2) space(17) ↵(3) = 26
        let widths = match layer {
            KeyLayer::Letters => [5u16, 3, 15, 3],
            KeyLayer::Symbols | KeyLayer::Functions => [4u16, 2, 17, 3],
        };
        let ids_widths = [
            (ID_BOTTOM_TOGGLE, widths[0]),
            (ID_BOTTOM_FN, widths[1]),
            (ID_BOTTOM_SPACE, widths[2]),
            (ID_BOTTOM_ENTER, widths[3]),
        ];
        KeyRow::tall(0, lay_out_compact_cells(&ids_widths))
    }

    /// The bottom bar shifts the toggle cell's right edge by one
    /// column when on the Letters layer (label `?123` is 4 chars, so
    /// cell width is 6 instead of 5). Reflect that geometry change
    /// here without storing two `CellSpec`s for the same `CellId`.
    fn bottom_bar(&self, layer: KeyLayer) -> KeyRow {
        let toggle_width: u16 = match layer {
            KeyLayer::Letters => 6,
            KeyLayer::Symbols | KeyLayer::Functions => 5,
        };

        let mut col: u16 = 0;
        let mut cells = Vec::with_capacity(4);

        let push_at = |id: u16, width: u16, col: &mut u16, cells: &mut Vec<KeyCell>| {
            cells.push(KeyCell {
                col_start: *col,
                col_end: *col + width,
                id: CellId(id),
            });
            *col += width;
        };

        push_at(ID_BOTTOM_TOGGLE, toggle_width, &mut col, &mut cells);
        push_at(ID_BOTTOM_FN, 4, &mut col, &mut cells);
        push_at(ID_BOTTOM_SPACE, 21, &mut col, &mut cells);
        push_at(ID_BOTTOM_ENTER, 3, &mut col, &mut cells);

        KeyRow::tall(0, cells)
    }
}

/// Append a row's cells to `cells`. Each `(id, width)` lays a cell
/// flush against the previous one, so `col_start`/`col_end` form a
/// contiguous sequence starting at 0. This is the canonical geometry
/// used everywhere except the bottom bar, whose toggle width depends
/// on the active layer and is recomputed in `bottom_bar()`.
fn push_row(cells: &mut HashMap<u16, CellSpec>, row: &[(u16, u16)]) {
    let mut col: u16 = 0;
    for (id, width) in row {
        cells.insert(
            *id,
            CellSpec {
                id: CellId(*id),
                col_start: col,
                col_end: col + *width,
            },
        );
        col += *width;
    }
}

/// Build a fresh `Vec<KeyCell>` from a contiguous `(id, width)`
/// sequence. Used by the compact-tier helpers — those rows need
/// `col_start`/`col_end` values different from what
/// `self.cells` stores (the natural-tier widths), so they construct
/// `KeyCell`s directly instead of looking them up.
fn lay_out_compact_cells(row: &[(u16, u16)]) -> Vec<KeyCell> {
    let mut col: u16 = 0;
    let mut out = Vec::with_capacity(row.len());
    for (id, width) in row {
        out.push(KeyCell {
            col_start: col,
            col_end: col + *width,
            id: CellId(*id),
        });
        col += *width;
    }
    out
}

impl KeyboardLayout for UsQwerty {
    fn id(&self) -> &'static str {
        "us-qwerty"
    }

    fn display_name(&self) -> &'static str {
        "US (QWERTY)"
    }

    fn rows(&self, mods: &KeyboardModifiers) -> Vec<KeyRow> {
        match mods.layer {
            KeyLayer::Letters => vec![
                self.extras_row(),
                self.letters_row_1(),
                self.letters_row_2(),
                self.letters_row_3(),
                self.bottom_bar(KeyLayer::Letters),
            ],
            KeyLayer::Symbols => vec![
                self.extras_row(),
                self.symbols_row_1(),
                self.symbols_row_2(),
                self.symbols_row_3(),
                self.bottom_bar(KeyLayer::Symbols),
            ],
            KeyLayer::Functions => vec![
                self.extras_row(),
                self.functions_row_1(),
                self.functions_row_2(),
                self.bottom_bar(KeyLayer::Functions),
            ],
        }
    }

    fn compact_rows(
        &self,
        mods: &KeyboardModifiers,
        target_block_width: u16,
    ) -> Vec<KeyRow> {
        // `target_block_width` is consumed by the renderer via
        // `compact_row_scales` — the cell positions themselves are
        // laid out at the canonical 26-col extent, and the renderer
        // applies the per-row stretch factor on top.
        let _ = target_block_width;
        match mods.layer {
            KeyLayer::Letters => vec![
                self.compact_letters_row_1(),
                self.compact_letters_row_2(),
                self.compact_letters_row_3(),
                self.compact_bottom_bar(KeyLayer::Letters),
            ],
            KeyLayer::Symbols => vec![
                self.compact_symbols_row_1(),
                self.compact_symbols_row_2(),
                self.compact_symbols_row_3(),
                self.compact_bottom_bar(KeyLayer::Symbols),
            ],
            KeyLayer::Functions => vec![
                self.compact_functions_row_1(),
                self.compact_functions_row_2(),
                self.compact_functions_row_3(),
                self.compact_bottom_bar(KeyLayer::Functions),
            ],
        }
    }

    fn compact_row_scales(
        &self,
        mods: &KeyboardModifiers,
        target_block_width: u16,
    ) -> Vec<(u16, u16)> {
        // Every compact row is laid out so its natural extent is
        // exactly `COMPACT_NATURAL_BLOCK`; a single `(target, 26)`
        // ratio scales the row to fill the available width. Per-row
        // scaling stays as the architectural primitive even though
        // every row currently shares the ratio — a future change
        // could give rows with anchored cells (R3, bottom bar) their
        // own ratio without disturbing the renderer.
        let _ = mods;
        let n = match mods.layer {
            KeyLayer::Letters | KeyLayer::Symbols | KeyLayer::Functions => 4,
        };
        vec![(target_block_width, COMPACT_NATURAL_BLOCK); n]
    }

    fn label(&self, cell: CellId, mods: &KeyboardModifiers) -> Cow<'static, str> {
        // Extras strip (layer-independent).
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
            // Empty label — the renderer pads the full cell width with
            // bg colour, producing a flat filler block.
            return Cow::Borrowed("");
        }
        if cell.0 == ID_FUNCTIONS_R2_BACKSPACE {
            return Cow::Borrowed("⌫");
        }

        Cow::Borrowed("")
    }

    fn emit(&self, cell: CellId, mods: &KeyboardModifiers) -> KeyAction {
        // Extras strip emits.
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
            // Letters ↔ Symbols swap; Functions always returns to
            // Letters via the toggle cell.
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

        // Letters layer printables — Shift selects the uppercase
        // variant by emitting a different char. (The serializer
        // ignores `KeyModifier::Shift` for `Char` keys, so folding
        // Shift in would be redundant.)
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
        // Only the bottom-bar Fn cell paints ACTIVE on a layer match.
        // The toggle cell intentionally returns `None` — its job is to
        // advertise the *alternate* layer, not the current one.
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

// Suppress the unused-field warning in case `cell_spec()` is not
// consulted internally yet — the method is part of the type's natural
// surface for future debugging / tests.
#[allow(dead_code)]
impl UsQwerty {
    fn debug_cell_spec(&self, id: CellId) -> Option<CellSpec> {
        self.cell_spec(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let rows = layout.rows(&mods);
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
        for row in layout.rows(&mods) {
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
    /// variant; tapping ⇧ must emit a `ToggleModifier(Shift)`.
    #[test]
    fn shifted_letters_emit_uppercase() {
        let layout = UsQwerty::new();
        let mut shifted = KeyboardModifiers::default();
        shifted.shift_armed = true;

        for (c, expected) in [('q', 'Q'), ('a', 'A'), ('z', 'Z'), ('m', 'M')] {
            let cell = find_cell_emitting(&layout, &KeyboardModifiers::default(), c)
                .expect("default letter cell");
            match layout.emit(cell, &shifted) {
                KeyAction::SendKey(k) => assert_eq!(
                    k.bare_key,
                    BareKey::Char(expected),
                    "{} should shift to {}",
                    c,
                    expected,
                ),
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

    /// The Fn cell is the only cell whose `layer_of` returns
    /// `Functions`; on the Functions layer the renderer would paint
    /// it active.
    #[test]
    fn fn_cell_layer_of_returns_functions() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        assert_eq!(layout.layer_of(CellId(ID_BOTTOM_FN)), Some(KeyLayer::Functions));
        // Toggle cell deliberately does NOT advertise a layer.
        assert_eq!(layout.layer_of(CellId(ID_BOTTOM_TOGGLE)), None);
        // No other cell does either.
        for row in layout.rows(&mods) {
            for cell in &row.cells {
                if cell.id.0 == ID_BOTTOM_FN {
                    continue;
                }
                assert_eq!(
                    layout.layer_of(cell.id),
                    None,
                    "cell {:?} unexpectedly returns a layer",
                    cell.id,
                );
            }
        }
    }

    /// Letters layer has 5 rows; Symbols 5 rows; Functions 4 rows.
    #[test]
    fn row_counts_per_layer() {
        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        assert_eq!(layout.rows(&mods).len(), 5);
        mods.layer = KeyLayer::Symbols;
        assert_eq!(layout.rows(&mods).len(), 5);
        mods.layer = KeyLayer::Functions;
        assert_eq!(layout.rows(&mods).len(), 4);
    }

    /// Letters row 2 carries a 2-col stagger; rows 1 and 3 do not.
    #[test]
    fn letters_row_2_is_staggered() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.rows(&mods);
        assert_eq!(rows[1].offset_col, 0); // row 1
        assert_eq!(rows[2].offset_col, 2); // row 2 (staggered)
        assert_eq!(rows[3].offset_col, 0); // row 3
    }

    /// Letters row 3 ends with `. / ⌫`. `.` and `/` are 3 cols wide
    /// (uniform with the letters); the backspace is 4 cols so the
    /// `center` helper drops an asymmetric extra space on its right
    /// side — visible right-padding inside the cell's bg shade.
    #[test]
    fn letters_row_3_tail_cells() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.rows(&mods);
        let cells = &rows[3].cells;
        let n = cells.len();
        assert_eq!(cells[n - 3].id.0, ID_LETTERS_R3_PERIOD);
        assert_eq!(cells[n - 2].id.0, ID_LETTERS_R3_SLASH);
        assert_eq!(cells[n - 1].id.0, ID_LETTERS_R3_BACKSPACE);
        assert_eq!(cells[n - 3].col_end - cells[n - 3].col_start, 3);
        assert_eq!(cells[n - 2].col_end - cells[n - 2].col_start, 3);
        assert_eq!(cells[n - 1].col_end - cells[n - 1].col_start, 4);
    }

    /// `.` and `/` on Letters row 3 emit their literal chars and
    /// ignore the Shift modifier (touch-keyboard convention — Shift
    /// only changes letters, not punctuation).
    #[test]
    fn letters_row_3_punctuation_emits_literal() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let mut shifted = KeyboardModifiers::default();
        shifted.shift_armed = true;
        for (id, ch) in [
            (ID_LETTERS_R3_PERIOD, '.'),
            (ID_LETTERS_R3_SLASH, '/'),
        ] {
            for m in [&mods, &shifted] {
                match layout.emit(CellId(id), m) {
                    KeyAction::SendKey(k) => assert_eq!(k.bare_key, BareKey::Char(ch)),
                    other => panic!("expected SendKey({ch:?}), got {other:?}"),
                }
            }
        }
    }

    /// Functions row 1 has 10 cells: F1..F9 are 4 cols, F10 is 5.
    #[test]
    fn functions_row_1_widths() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Functions,
            ..KeyboardModifiers::default()
        };
        let rows = layout.rows(&mods);
        let r1 = &rows[1];
        assert_eq!(r1.cells.len(), 10);
        for cell in &r1.cells[..9] {
            assert_eq!(cell.col_end - cell.col_start, 4);
        }
        assert_eq!(r1.cells[9].col_end - r1.cells[9].col_start, 5);
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

    /// Toggle cell width on the Letters layer is 6 (label `?123`);
    /// on Symbols / Functions it is 5 (label `ABC`).
    #[test]
    fn toggle_cell_width_depends_on_layer() {
        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        let toggle = |rows: &[KeyRow]| {
            let bottom = rows.last().unwrap();
            bottom.cells[0]
        };
        assert_eq!(
            toggle(&layout.rows(&mods)).col_end - toggle(&layout.rows(&mods)).col_start,
            6,
        );
        mods.layer = KeyLayer::Symbols;
        assert_eq!(
            toggle(&layout.rows(&mods)).col_end - toggle(&layout.rows(&mods)).col_start,
            5,
        );
        mods.layer = KeyLayer::Functions;
        assert_eq!(
            toggle(&layout.rows(&mods)).col_end - toggle(&layout.rows(&mods)).col_start,
            5,
        );
    }

    fn find_cell_emitting(
        layout: &UsQwerty,
        mods: &KeyboardModifiers,
        c: char,
    ) -> Option<CellId> {
        for row in layout.rows(mods) {
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
    //
    // The compact tier ships a 4-row layout for narrow viewports. The
    // canonical target is 28 cols × 23 rows on a 24-px-font Android
    // phone in portrait. Per-row widths reproduce the visual
    // reference in `compact_keyboard_mock.ansi`.
    // -----------------------------------------------------------------

    /// All three compact layers expose exactly 4 rows, regardless of
    /// active layer. This is the invariant that lets
    /// `compute_compact_geometry` produce a uniform row-height
    /// distribution.
    #[test]
    fn compact_rows_count_per_layer() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
            assert_eq!(rows.len(), 4, "{:?}", layer);
        }
    }

    /// Compact-tier Letters R1 — `q w e r t y u i o p` with the
    /// per-cell widths pinned by `compact_keyboard_mock.ansi`. The row
    /// sums to `COMPACT_NATURAL_BLOCK = 26` cells.
    #[test]
    fn compact_letters_row_1_widths_match_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let r1 = &rows[0];
        let widths: Vec<u16> = r1
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3, 2, 3, 2, 3, 2, 3, 2, 3, 3]);
        assert_eq!(r1.offset_col, 0);
    }

    /// Compact-tier Letters R2 — `a..l` with the canonical 24-cell
    /// row plus a 2-cell stagger (`offset_col = 2`) so column edges
    /// line up with R1 and R3.
    #[test]
    fn compact_letters_row_2_widths_match_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let r2 = &rows[1];
        assert_eq!(r2.offset_col, 2);
        let widths: Vec<u16> = r2
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3, 2, 3, 3, 2, 3, 2, 3, 3]);
    }

    /// Compact-tier Letters R3 — `⇧ z..m ⌫` with fixed-width anchors
    /// (`⇧` 3 cells, `⌫` 6 cells) and 7 letters sharing the 17-cell
    /// middle span.
    #[test]
    fn compact_letters_row_3_widths_match_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let r3 = &rows[2];
        let widths: Vec<u16> = r3
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3, 2, 3, 2, 3, 2, 3, 2, 6]);
    }

    /// Compact-tier bottom bar on the Letters layer: `?123 Fn space ↵`
    /// with widths 5 / 3 / 15 / 3 = 26.
    #[test]
    fn compact_letters_bottom_bar_matches_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let bottom = rows.last().expect("bottom bar present");
        let widths: Vec<u16> = bottom
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![5, 3, 15, 3]);
    }

    /// Compact-tier Symbols R1 carries the terminal affordances
    /// (Esc / Tab / Ctl / Alt + four arrows) — that was originally
    /// the natural-tier extras strip. The compact tier removes the
    /// extras strip, so these cells live here on Symbols.
    #[test]
    fn compact_symbols_row_1_carries_terminal_affordances() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Symbols,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let r1 = &rows[0];
        let ids: Vec<u16> = r1.cells.iter().map(|c| c.id.0).collect();
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
        let widths: Vec<u16> = r1
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![4, 4, 4, 4, 2, 2, 2, 4]);
    }

    /// Compact-tier Symbols R2 — digits `1..0` with the same per-cell
    /// widths as compact Letters R1 so column edges align between
    /// layers.
    #[test]
    fn compact_symbols_row_2_widths_match_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Symbols,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let widths: Vec<u16> = rows[1]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![3, 2, 3, 2, 3, 2, 3, 2, 3, 3]);
    }

    /// Compact-tier Symbols R3 — `/ \ : ; | < > ? . ⌫` with widths
    /// summing to 26 and `.` sourced from SYMBOLS_R3[11] so the
    /// natural-tier label / emit paths handle it unchanged.
    #[test]
    fn compact_symbols_row_3_widths_match_mock() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Symbols,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        let r3 = &rows[2];
        let widths: Vec<u16> = r3
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(widths, vec![2, 2, 3, 3, 2, 3, 3, 2, 2, 4]);
        // Period cell sources its label / action from SYMBOLS_R3[11].
        let period_id = r3.cells[8].id;
        assert_eq!(layout.label(period_id, &mods).as_ref(), ".");
        match layout.emit(period_id, &mods) {
            KeyAction::SendKey(k) => assert_eq!(k.bare_key, BareKey::Char('.')),
            other => panic!("expected SendKey('.'), got {:?}", other),
        }
    }

    /// Compact-tier Symbols / Functions bottom bar: `ABC Fn space ↵`
    /// at widths 4 / 2 / 17 / 3 — Functions reuses the same widths.
    #[test]
    fn compact_non_letters_bottom_bar_matches_mock() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
            let bottom = rows.last().expect("bottom bar present");
            let widths: Vec<u16> = bottom
                .cells
                .iter()
                .map(|c| c.col_end - c.col_start)
                .collect();
            assert_eq!(widths, vec![4, 2, 17, 3], "{:?}", layer);
        }
    }

    /// Compact-tier Functions layer R1 / R2 carry F1..F12. R3 is the
    /// terminal-affordances row (mirror of compact Symbols R1) so
    /// Esc / arrows stay reachable while editing F-keys.
    #[test]
    fn compact_functions_layout_is_complete() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers {
            layer: KeyLayer::Functions,
            ..KeyboardModifiers::default()
        };
        let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
        // R1: F1..F6.
        let r1_widths: Vec<u16> = rows[0]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(r1_widths, vec![4, 4, 5, 4, 4, 5]);
        // R2: F7..F12.
        let r2_widths: Vec<u16> = rows[1]
            .cells
            .iter()
            .map(|c| c.col_end - c.col_start)
            .collect();
        assert_eq!(r2_widths, vec![4, 4, 5, 4, 4, 5]);
        // R3 is a mirror of compact Symbols R1.
        let r3_ids: Vec<u16> = rows[2].cells.iter().map(|c| c.id.0).collect();
        assert_eq!(
            r3_ids,
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

    /// Every compact-tier row sums to `COMPACT_NATURAL_BLOCK` cells —
    /// the invariant that lets the renderer apply a single per-row
    /// stretch ratio. R2's stagger is included via `offset_col`.
    #[test]
    fn every_compact_row_sums_to_natural_block() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            for (row_index, row) in layout
                .compact_rows(&mods, COMPACT_NATURAL_BLOCK)
                .iter()
                .enumerate()
            {
                let last_end = row.cells.last().map(|c| c.col_end).unwrap_or(0);
                let extent = row.offset_col + last_end + row.right_pad;
                assert_eq!(
                    extent, COMPACT_NATURAL_BLOCK,
                    "{:?} row {} extent {} != COMPACT_NATURAL_BLOCK",
                    layer, row_index, extent,
                );
            }
        }
    }

    /// `compact_row_scales` returns one entry per row, all using the
    /// uniform `(target_block_width, COMPACT_NATURAL_BLOCK)` ratio
    /// for the v1 layout.
    #[test]
    fn compact_row_scales_uniform_per_layer() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            let target = 30u16;
            let scales = layout.compact_row_scales(&mods, target);
            assert_eq!(scales.len(), 4, "{:?}", layer);
            for (num, den) in scales {
                assert_eq!((num, den), (target, COMPACT_NATURAL_BLOCK));
            }
        }
    }

    /// Compact rows still resolve their cell IDs through the same
    /// `label`/`emit` paths as the natural tier. The compact-tier
    /// helpers reuse the existing CellIds, so the only requirement
    /// is that every cell returned by `compact_rows` has a defined
    /// label and a non-`NoOp` action where appropriate.
    #[test]
    fn every_compact_cell_resolves_on_every_layer() {
        let layout = UsQwerty::new();
        for layer in [KeyLayer::Letters, KeyLayer::Symbols, KeyLayer::Functions] {
            let mods = KeyboardModifiers { layer, ..KeyboardModifiers::default() };
            let rows = layout.compact_rows(&mods, COMPACT_NATURAL_BLOCK);
            for row in &rows {
                for cell in &row.cells {
                    let _label = layout.label(cell.id, &mods);
                    let _action = layout.emit(cell.id, &mods);
                }
            }
        }
    }
}
