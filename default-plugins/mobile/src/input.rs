//! Shared keyboard-input state: the sticky one-shot Ctrl/Alt flags and
//! the on-screen modifier-bar controller. The hardware-keyboard
//! passthrough (in the Viewport screen) and the modifier bar both read
//! and write the same `ctrl_held`/`alt_held` flags, so a `⌃` tap from
//! the bar followed by a hardware key produces a properly-modified key.

use zellij_tile::prelude::*;

use crate::modifier_bar::{CellId, ModifierBarController, TapOutcome};

/// Shared modifier / on-screen-keyboard state.
#[derive(Default)]
pub struct Input {
    /// In-plugin on-screen keyboard controller. Owns the modifier-bar
    /// cell state machine.
    pub modifier_bar: ModifierBarController,
    /// Sticky-Ctrl flag. Folded into the next non-modifier key from
    /// either the bar or the hardware keyboard, then cleared.
    pub ctrl_held: bool,
    /// Sticky-Alt flag. Same semantics as `ctrl_held`.
    pub alt_held: bool,
}

impl Input {
    /// Return a clone of `key` with `Ctrl` / `Alt` added to its
    /// modifier set when the corresponding sticky flag is on. Used by
    /// the Viewport key handler so a hardware-keyboard tap that follows
    /// a `⌃` / `⌥` tap produces a properly-modified key.
    pub fn merge_held_modifiers(&self, key: &KeyWithModifier) -> KeyWithModifier {
        let mut merged = key.clone();
        if self.ctrl_held {
            merged.key_modifiers.insert(KeyModifier::Ctrl);
        }
        if self.alt_held {
            merged.key_modifiers.insert(KeyModifier::Alt);
        }
        merged
    }

    /// Route a modifier-bar cell tap through the controller, sharing the
    /// `ctrl_held`/`alt_held` flags so the bar and hardware paths agree.
    pub fn handle_tap(&mut self, cell: CellId) -> TapOutcome {
        self.modifier_bar
            .handle_tap(cell, &mut self.ctrl_held, &mut self.alt_held)
    }
}
