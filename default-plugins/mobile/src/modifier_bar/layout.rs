//! Cell identifier + tap-action enum for the bottom modifier bar.
//!
//! `CellId` is an opaque per-cell token round-tripped through the
//! click-region map. `KeyAction` is what `ModifierBarController::handle_tap`
//! produces after looking up the cell — the dispatcher acts on it.

use zellij_tile::prelude::*;

use super::modifiers::Modifier;

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
