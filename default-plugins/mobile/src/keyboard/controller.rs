//! Universal keyboard state machine.
//!
//! Holds the active layout, the modifier flags, the press-flash map
//! and the visibility flag. `handle_tap` is the single entry point —
//! the dispatcher routes every keyboard click here and acts on the
//! returned `TapOutcome`.
//!
//! The state machine is layout-agnostic; the only thing it asks of
//! the layout is `emit(cell, mods)`. Future layouts plug in by
//! shipping a new `KeyboardLayout` impl and registering it in
//! `layouts/mod.rs`.

use std::collections::HashMap;
use std::time::Instant;

use zellij_tile::prelude::*;

use super::layout::{CellId, KeyAction, KeyboardLayout};
use super::layouts;
use super::modifiers::{KeyboardModifiers, Modifier};
use crate::keys;

/// Duration of the inverted-cell flash painted after every tap. Same
/// value the bottom-bar shortcut feedback used so the two are
/// perceptually consistent. Caller schedules a Timer at this delay so
/// `sweep_flash` can drop the entry.
pub const KEY_FEEDBACK_MS: u128 = 400;

/// What the dispatcher should do after a tap. The controller never
/// performs IO itself — the caller wires the outcome up to
/// `write_to_pane_id`, `set_soft_keyboard`, `set_timeout`, …
pub enum TapOutcome {
    /// Bytes for the underlying pane's pty. Caller does the write.
    SendBytes(Vec<u8>),
    /// Modifier (Shift/Ctrl/Alt/Fn) flipped — just a redraw needed.
    Toggled,
    /// `visible` was flipped; caller updates the OS soft-keyboard
    /// suppression accordingly.
    HideKeyboard,
    /// Inert decorative cell, or no resolvable action.
    NoOp,
}

pub struct KeyboardController {
    pub layout: Box<dyn KeyboardLayout>,
    pub modifiers: KeyboardModifiers,
    pub press_flash: HashMap<CellId, Instant>,
    pub visible: bool,
}

impl Default for KeyboardController {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardController {
    pub fn new() -> Self {
        Self {
            layout: layouts::default_layout(),
            modifiers: KeyboardModifiers::default(),
            press_flash: HashMap::new(),
            visible: true,
        }
    }

    /// Translate a tap on `cell` into the bytes / modifier flip /
    /// visibility change it represents. `ctrl_held` / `alt_held` are
    /// `&mut` references to the corresponding `State` fields so the
    /// hardware-key passthrough path and this controller share the
    /// same one-shot modifier flags.
    pub fn handle_tap(
        &mut self,
        cell: CellId,
        ctrl_held: &mut bool,
        alt_held: &mut bool,
    ) -> TapOutcome {
        // Sync `State.ctrl_held`/`alt_held` into the modifier struct
        // before consulting the layout, so a hardware-tap that armed
        // Ctrl is honoured by the next plugin-keyboard tap (and vice
        // versa). The layout sees a unified view.
        self.modifiers.ctrl_armed = *ctrl_held;
        self.modifiers.alt_armed = *alt_held;

        let action = self.layout.emit(cell, &self.modifiers);

        // Visual feedback fires for *every* tap, including modifier
        // toggles and NoOps. Cheap (one HashMap entry) and gives the
        // user confirmation that the click landed.
        self.press_flash.insert(cell, Instant::now());

        match action {
            KeyAction::ToggleModifier(m) => {
                self.modifiers.toggle(m);
                // Write Ctrl/Alt back through the shared `State`
                // refs so the hardware-key path agrees.
                if matches!(m, Modifier::Ctrl) {
                    *ctrl_held = self.modifiers.ctrl_armed;
                }
                if matches!(m, Modifier::Alt) {
                    *alt_held = self.modifiers.alt_armed;
                }
                TapOutcome::Toggled
            },
            KeyAction::SendKey(mut kwm) => {
                // Fold in any Ctrl/Alt that was armed but not already
                // present in the layout's emitted key. The layout
                // attaches modifiers it knows about (Shift-arms a
                // shifted variant intrinsically by returning the
                // shifted character); Ctrl/Alt are universally folded
                // in here so a tap on `c` while ⌃ is armed produces
                // exactly Ctrl+c, identical to the hardware path.
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
            KeyAction::ToggleVisibility => {
                self.visible = !self.visible;
                TapOutcome::HideKeyboard
            },
            KeyAction::NoOp => TapOutcome::NoOp,
        }
    }

    /// Drop press-flash entries older than `KEY_FEEDBACK_MS`. Returns
    /// `true` if at least one entry expired (caller redraws so the
    /// flash visually clears).
    pub fn sweep_flash(&mut self, now: Instant) -> bool {
        let len_before = self.press_flash.len();
        self.press_flash
            .retain(|_, t| now.saturating_duration_since(*t).as_millis() < KEY_FEEDBACK_MS);
        len_before != self.press_flash.len()
    }
}
