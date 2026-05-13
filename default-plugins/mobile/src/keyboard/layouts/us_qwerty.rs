//! US-QWERTY layout. The only layout shipped in v1.
//!
//! The cell table covers extras, F-row, numbers, qwerty, asdf, zxcv
//! and utility — every cell visible in the three mockups in
//! `mobile_keyboard.md`. Cell IDs are stable u16s numbered in row-
//! major order; the renderer and controller never depend on the
//! numbering scheme.

use std::borrow::Cow;
use std::collections::HashMap;

use zellij_tile::prelude::*;

use crate::keyboard::layout::{CellId, KeyAction, KeyCell, KeyRow, KeyboardLayout};
use crate::keyboard::modifiers::{KeyboardModifiers, Modifier};

/// Per-cell static data. Rendered as a small table built once at
/// construction time so `label()` / `emit()` are O(1) lookups.
#[derive(Debug, Clone)]
struct UsQwertyCell {
    id: CellId,
    col_start: u16,
    col_end: u16,
    default_label: &'static str,
    shifted_label: &'static str,
    default_emit: Emit,
    shifted_emit: Emit,
    modifier: Option<Modifier>,
}

/// Compact representation of what to emit. Materialised into a full
/// `KeyAction` (which owns a `KeyWithModifier` and is therefore not
/// const-constructible) only at the moment of a tap.
#[derive(Debug, Clone, Copy)]
enum Emit {
    /// Plain ASCII char. Ctrl/Alt are folded in by the controller.
    Char(char),
    /// `BareKey` other than a printable char (Esc, Tab, Backspace,
    /// Enter, arrows).
    Bare(BareKey),
    /// `BareKey::F(n)`.
    F(u8),
    /// Toggle a modifier instead of sending bytes.
    Toggle(Modifier),
}

impl Emit {
    fn into_action(self) -> KeyAction {
        match self {
            Emit::Char(c) => KeyAction::SendKey(KeyWithModifier {
                bare_key: BareKey::Char(c),
                key_modifiers: std::collections::BTreeSet::new(),
            }),
            Emit::Bare(b) => KeyAction::SendKey(KeyWithModifier {
                bare_key: b,
                key_modifiers: std::collections::BTreeSet::new(),
            }),
            Emit::F(n) => KeyAction::SendKey(KeyWithModifier {
                bare_key: BareKey::F(n),
                key_modifiers: std::collections::BTreeSet::new(),
            }),
            Emit::Toggle(m) => KeyAction::ToggleModifier(m),
        }
    }
}

pub struct UsQwerty {
    cells: Vec<UsQwertyCell>,
    /// `CellId.0` → index into `cells`.
    by_id: HashMap<u16, usize>,
    /// Per-row cell index slices, one per logical row in row-order.
    /// `(offset_col, range)` tuples; the F-row is conditionally
    /// included by `rows()`.
    extras_idx: (u16, std::ops::Range<usize>),
    f_row_idx: (u16, std::ops::Range<usize>),
    numbers_idx: (u16, std::ops::Range<usize>),
    qwerty_idx: (u16, std::ops::Range<usize>),
    asdf_idx: (u16, std::ops::Range<usize>),
    zxcv_idx: (u16, std::ops::Range<usize>),
    utility_idx: (u16, std::ops::Range<usize>),
}

impl UsQwerty {
    pub fn new() -> Self {
        let mut cells: Vec<UsQwertyCell> = Vec::new();
        let mut next_id: u16 = 1;

        // Helper closures to keep the table compact.
        let push = |cells: &mut Vec<UsQwertyCell>,
                        next_id: &mut u16,
                        col_start: u16,
                        col_end: u16,
                        default_label: &'static str,
                        shifted_label: &'static str,
                        default_emit: Emit,
                        shifted_emit: Emit,
                        modifier: Option<Modifier>| {
            let id = CellId(*next_id);
            *next_id += 1;
            cells.push(UsQwertyCell {
                id,
                col_start,
                col_end,
                default_label,
                shifted_label,
                default_emit,
                shifted_emit,
                modifier,
            });
        };

        // ---- Extras (offset_col=0, walls at 0,4,8,12,16,20,23,26,29,32) ----
        let extras_start = cells.len();
        push(&mut cells, &mut next_id, 0, 4, "Esc", "Esc", Emit::Bare(BareKey::Esc), Emit::Bare(BareKey::Esc), None);
        push(&mut cells, &mut next_id, 4, 8, "Tab", "Tab", Emit::Bare(BareKey::Tab), Emit::Bare(BareKey::Tab), None);
        push(&mut cells, &mut next_id, 8, 12, "Ctl", "Ctl", Emit::Toggle(Modifier::Ctrl), Emit::Toggle(Modifier::Ctrl), Some(Modifier::Ctrl));
        push(&mut cells, &mut next_id, 12, 16, "Alt", "Alt", Emit::Toggle(Modifier::Alt), Emit::Toggle(Modifier::Alt), Some(Modifier::Alt));
        push(&mut cells, &mut next_id, 16, 20, " Fn", " Fn", Emit::Toggle(Modifier::Fn), Emit::Toggle(Modifier::Fn), Some(Modifier::Fn));
        push(&mut cells, &mut next_id, 20, 23, "← ", "← ", Emit::Bare(BareKey::Left), Emit::Bare(BareKey::Left), None);
        push(&mut cells, &mut next_id, 23, 26, "↓ ", "↓ ", Emit::Bare(BareKey::Down), Emit::Bare(BareKey::Down), None);
        push(&mut cells, &mut next_id, 26, 29, "↑ ", "↑ ", Emit::Bare(BareKey::Up), Emit::Bare(BareKey::Up), None);
        push(&mut cells, &mut next_id, 29, 32, "→ ", "→ ", Emit::Bare(BareKey::Right), Emit::Bare(BareKey::Right), None);
        let extras_end = cells.len();

        // ---- F-row (offset_col=0, walls at 0,3,6,...,36 — 12 cells) ----
        // F-row labels never flip under Shift; default == shifted by
        // design (Stage 8 of the implementation plan). F1..F9 fit as
        // "F1".."F9"; F10..F12 lose the F-prefix to fit in the 2-cell
        // interior.
        let f_row_start = cells.len();
        let f_labels: [&'static str; 12] = ["F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "10", "11", "12"];
        for (i, label) in f_labels.iter().enumerate() {
            let col_start = (i as u16) * 3;
            let col_end = col_start + 3;
            let n = (i + 1) as u8;
            push(&mut cells, &mut next_id, col_start, col_end, label, label, Emit::F(n), Emit::F(n), None);
        }
        let f_row_end = cells.len();

        // ---- Numbers (offset_col=0, walls at 0,3,...,39 — 13 cells) ----
        let numbers_start = cells.len();
        let numbers: [(&str, &str, char, char); 13] = [
            ("` ", "~ ", '`', '~'),
            ("1 ", "! ", '1', '!'),
            ("2 ", "@ ", '2', '@'),
            ("3 ", "# ", '3', '#'),
            ("4 ", "$ ", '4', '$'),
            ("5 ", "% ", '5', '%'),
            ("6 ", "^ ", '6', '^'),
            ("7 ", "& ", '7', '&'),
            ("8 ", "* ", '8', '*'),
            ("9 ", "( ", '9', '('),
            ("0 ", ") ", '0', ')'),
            ("- ", "_ ", '-', '_'),
            ("= ", "+ ", '=', '+'),
        ];
        for (i, (dl, sl, dc, sc)) in numbers.iter().enumerate() {
            let col_start = (i as u16) * 3;
            let col_end = col_start + 3;
            push(&mut cells, &mut next_id, col_start, col_end, dl, sl, Emit::Char(*dc), Emit::Char(*sc), None);
        }
        let numbers_end = cells.len();

        // ---- Qwerty (offset_col=0, walls at 0,3,...,39 — 13 cells) ----
        let qwerty_start = cells.len();
        let qwerty: [(&str, &str, char, char); 13] = [
            ("q ", "Q ", 'q', 'Q'),
            ("w ", "W ", 'w', 'W'),
            ("e ", "E ", 'e', 'E'),
            ("r ", "R ", 'r', 'R'),
            ("t ", "T ", 't', 'T'),
            ("y ", "Y ", 'y', 'Y'),
            ("u ", "U ", 'u', 'U'),
            ("i ", "I ", 'i', 'I'),
            ("o ", "O ", 'o', 'O'),
            ("p ", "P ", 'p', 'P'),
            ("[ ", "{ ", '[', '{'),
            ("] ", "} ", ']', '}'),
            ("\\ ", "| ", '\\', '|'),
        ];
        for (i, (dl, sl, dc, sc)) in qwerty.iter().enumerate() {
            let col_start = (i as u16) * 3;
            let col_end = col_start + 3;
            push(&mut cells, &mut next_id, col_start, col_end, dl, sl, Emit::Char(*dc), Emit::Char(*sc), None);
        }
        let qwerty_end = cells.len();

        // ---- ASDF (offset_col=1, walls at 0,3,...,33 — 11 cells) ----
        let asdf_start = cells.len();
        let asdf: [(&str, &str, char, char); 11] = [
            ("a ", "A ", 'a', 'A'),
            ("s ", "S ", 's', 'S'),
            ("d ", "D ", 'd', 'D'),
            ("f ", "F ", 'f', 'F'),
            ("g ", "G ", 'g', 'G'),
            ("h ", "H ", 'h', 'H'),
            ("j ", "J ", 'j', 'J'),
            ("k ", "K ", 'k', 'K'),
            ("l ", "L ", 'l', 'L'),
            ("; ", ": ", ';', ':'),
            ("' ", "\" ", '\'', '"'),
        ];
        for (i, (dl, sl, dc, sc)) in asdf.iter().enumerate() {
            let col_start = (i as u16) * 3;
            let col_end = col_start + 3;
            push(&mut cells, &mut next_id, col_start, col_end, dl, sl, Emit::Char(*dc), Emit::Char(*sc), None);
        }
        let asdf_end = cells.len();

        // ---- ZXCV (offset_col=0, walls at 0,3,...,36 — 12 cells) ----
        // First cell is ⇧ (sticky one-shot Shift). Last is ⌫.
        let zxcv_start = cells.len();
        push(&mut cells, &mut next_id, 0, 3, "⇧ ", "⇧ ", Emit::Toggle(Modifier::Shift), Emit::Toggle(Modifier::Shift), Some(Modifier::Shift));
        let zxcv_letters: [(&str, &str, char, char); 10] = [
            ("z ", "Z ", 'z', 'Z'),
            ("x ", "X ", 'x', 'X'),
            ("c ", "C ", 'c', 'C'),
            ("v ", "V ", 'v', 'V'),
            ("b ", "B ", 'b', 'B'),
            ("n ", "N ", 'n', 'N'),
            ("m ", "M ", 'm', 'M'),
            (", ", "< ", ',', '<'),
            (". ", "> ", '.', '>'),
            ("/ ", "? ", '/', '?'),
        ];
        for (i, (dl, sl, dc, sc)) in zxcv_letters.iter().enumerate() {
            let col_start = 3 + (i as u16) * 3;
            let col_end = col_start + 3;
            push(&mut cells, &mut next_id, col_start, col_end, dl, sl, Emit::Char(*dc), Emit::Char(*sc), None);
        }
        push(&mut cells, &mut next_id, 33, 36, "⌫ ", "⌫ ", Emit::Bare(BareKey::Backspace), Emit::Bare(BareKey::Backspace), None);
        let zxcv_end = cells.len();

        // ---- Utility (offset_col=0, walls at 0, 30, 36) ----
        let utility_start = cells.len();
        push(
            &mut cells,
            &mut next_id,
            0,
            30,
            "            space            ",
            "            space            ",
            Emit::Char(' '),
            Emit::Char(' '),
            None,
        );
        push(&mut cells, &mut next_id, 30, 36, "  ↵  ", "  ↵  ", Emit::Bare(BareKey::Enter), Emit::Bare(BareKey::Enter), None);
        let utility_end = cells.len();

        let by_id: HashMap<u16, usize> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.0, i))
            .collect();

        Self {
            cells,
            by_id,
            extras_idx: (0, extras_start..extras_end),
            f_row_idx: (0, f_row_start..f_row_end),
            numbers_idx: (0, numbers_start..numbers_end),
            qwerty_idx: (0, qwerty_start..qwerty_end),
            asdf_idx: (1, asdf_start..asdf_end),
            zxcv_idx: (0, zxcv_start..zxcv_end),
            utility_idx: (0, utility_start..utility_end),
        }
    }

    fn lookup(&self, cell: CellId) -> Option<&UsQwertyCell> {
        self.by_id.get(&cell.0).map(|i| &self.cells[*i])
    }

    fn key_row(&self, slice: &(u16, std::ops::Range<usize>)) -> KeyRow {
        let (offset_col, range) = slice;
        let cells = self.cells[range.clone()]
            .iter()
            .map(|c| KeyCell {
                col_start: c.col_start,
                col_end: c.col_end,
                id: c.id,
            })
            .collect();
        KeyRow {
            offset_col: *offset_col,
            cells,
        }
    }
}

impl KeyboardLayout for UsQwerty {
    fn id(&self) -> &'static str {
        "us-qwerty"
    }

    fn display_name(&self) -> &'static str {
        "US (QWERTY)"
    }

    fn rows(&self, mods: &KeyboardModifiers) -> Vec<KeyRow> {
        let mut out = Vec::with_capacity(if mods.fn_armed { 7 } else { 6 });
        out.push(self.key_row(&self.extras_idx));
        if mods.fn_armed {
            out.push(self.key_row(&self.f_row_idx));
        }
        out.push(self.key_row(&self.numbers_idx));
        out.push(self.key_row(&self.qwerty_idx));
        out.push(self.key_row(&self.asdf_idx));
        out.push(self.key_row(&self.zxcv_idx));
        out.push(self.key_row(&self.utility_idx));
        out
    }

    fn label(&self, cell: CellId, mods: &KeyboardModifiers) -> Cow<'static, str> {
        match self.lookup(cell) {
            Some(c) => {
                if mods.shift_armed {
                    Cow::Borrowed(c.shifted_label)
                } else {
                    Cow::Borrowed(c.default_label)
                }
            },
            None => Cow::Borrowed(""),
        }
    }

    fn emit(&self, cell: CellId, mods: &KeyboardModifiers) -> KeyAction {
        match self.lookup(cell) {
            Some(c) => {
                let emit = if mods.shift_armed {
                    c.shifted_emit
                } else {
                    c.default_emit
                };
                emit.into_action()
            },
            None => KeyAction::NoOp,
        }
    }

    fn modifier_of(&self, cell: CellId) -> Option<Modifier> {
        self.lookup(cell).and_then(|c| c.modifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `CellId` returned by `rows()` must resolve under
    /// `label()` and `emit()`. Modifier cells must report a modifier
    /// from `modifier_of()`; all others must not.
    #[test]
    fn every_cell_resolves() {
        let layout = UsQwerty::new();
        let mods = KeyboardModifiers::default();
        let rows = layout.rows(&mods);
        assert!(!rows.is_empty());
        let mut seen_modifier_cells = 0usize;
        for row in &rows {
            for cell in &row.cells {
                let label = layout.label(cell.id, &mods);
                assert!(!label.is_empty(), "empty label for cell {:?}", cell.id);
                let _action = layout.emit(cell.id, &mods);
                if layout.modifier_of(cell.id).is_some() {
                    seen_modifier_cells += 1;
                }
            }
        }
        // Shift + Ctrl + Alt + Fn — exactly four cells.
        assert_eq!(seen_modifier_cells, 4);
    }

    #[test]
    fn shifted_labels_differ_for_printable_cells() {
        let layout = UsQwerty::new();
        let unshifted = KeyboardModifiers::default();
        let mut shifted = KeyboardModifiers::default();
        shifted.shift_armed = true;

        // Spot-check a handful of the documented mappings.
        let checks: &[(char, char)] = &[
            ('1', '!'),
            ('2', '@'),
            ('a', 'A'),
            ('z', 'Z'),
            ('`', '~'),
            ('-', '_'),
            ('=', '+'),
            ('[', '{'),
            ('\\', '|'),
            (';', ':'),
            ('\'', '"'),
            (',', '<'),
            ('.', '>'),
            ('/', '?'),
        ];

        for (dc, sc) in checks {
            // Find the cell that emits `dc` under default mods.
            let mut found = None;
            for row in layout.rows(&unshifted) {
                for cell in &row.cells {
                    if let KeyAction::SendKey(k) = layout.emit(cell.id, &unshifted) {
                        if k.bare_key == BareKey::Char(*dc) {
                            found = Some(cell.id);
                            break;
                        }
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            let id = found.unwrap_or_else(|| panic!("no cell for '{}'", dc));
            let shifted_action = layout.emit(id, &shifted);
            match shifted_action {
                KeyAction::SendKey(k) => assert_eq!(
                    k.bare_key,
                    BareKey::Char(*sc),
                    "shifted variant of '{}' should be '{}'",
                    dc,
                    sc
                ),
                other => panic!("expected SendKey for shifted '{}', got {:?}", dc, other),
            }
        }
    }

    #[test]
    fn fn_armed_inserts_f_row() {
        let layout = UsQwerty::new();
        let mut mods = KeyboardModifiers::default();
        let n_default = layout.rows(&mods).len();
        mods.fn_armed = true;
        let n_fn = layout.rows(&mods).len();
        assert_eq!(n_fn, n_default + 1);
    }
}
