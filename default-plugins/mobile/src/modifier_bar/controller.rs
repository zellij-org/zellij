//! State machine behind the bottom modifier bar.
//!
//! Owns the one-shot modifier flags. `handle_tap` is the single entry
//! point — the dispatcher routes every bar click here and acts on the
//! returned `TapOutcome`.

use std::collections::BTreeSet;

use zellij_tile::prelude::*;

use super::layout::{CellId, KeyAction};
use super::modifiers::{KeyboardModifiers, Modifier};
use crate::keys;

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
