use zellij_tile::prelude::*;

use crate::components::modifier_bar::{CellId, ModifierBarController, TapOutcome};

#[derive(Default)]
pub struct Input {
    pub modifier_bar: ModifierBarController,
    pub ctrl_held: bool,
    pub alt_held: bool,
}

impl Input {
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

    pub fn handle_tap(&mut self, cell: CellId) -> TapOutcome {
        self.modifier_bar
            .handle_tap(cell, &mut self.ctrl_held, &mut self.alt_held)
    }
}
