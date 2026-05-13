//! Layout-trait + per-cell data types for the in-plugin keyboard.
//!
//! Each concrete layout (US-QWERTY today; future QWERTZ, AZERTY,
//! Dvorak, JP-romaji, …) implements `KeyboardLayout`. The renderer
//! and click dispatcher never inspect a `CellId` directly — they only
//! call trait methods. That is the modularity contract that lets new
//! layouts drop in without touching `render.rs` / `controller.rs`.

use std::borrow::Cow;
use zellij_tile::prelude::*;

use super::modifiers::{KeyboardModifiers, Modifier};

/// Opaque per-layout cell identifier. The renderer and dispatcher
/// only round-trip these; only the owning layout knows what each id
/// means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub u16);

/// One physical cell within a rendered row.
///
/// `col_start` and `col_end` are columns relative to the row's
/// `offset_col` (which is itself relative to the keyboard's left
/// edge). The cell's vertical walls land at those two columns; the
/// interior (where the label paints) is `col_start + 1 .. col_end`.
#[derive(Debug, Clone, Copy)]
pub struct KeyCell {
    pub col_start: u16,
    pub col_end: u16,
    pub id: CellId,
}

/// One row of cells, indented `offset_col` from the keyboard's left
/// edge. The asdf row uses `offset_col = 1` to approximate the
/// half-key stagger of a physical keyboard.
pub struct KeyRow {
    pub offset_col: u16,
    pub cells: Vec<KeyCell>,
}

/// What a tap on a cell resolves to, after consulting the layout.
///
/// `ToggleVisibility` exists for future in-keyboard hide affordances;
/// the v1 US-QWERTY layout never emits it (the top bar's ⌨ button is
/// the canonical hide path).
#[derive(Debug, Clone)]
pub enum KeyAction {
    SendKey(KeyWithModifier),
    ToggleModifier(Modifier),
    ToggleVisibility,
    NoOp,
}

/// Every concrete layout implements this. Object-safe — the
/// controller holds `Box<dyn KeyboardLayout>` and the renderer/
/// dispatcher accept `&dyn KeyboardLayout` so a layout swap at
/// runtime is structurally possible from day one.
pub trait KeyboardLayout: Send + Sync {
    /// Stable machine identifier, e.g. `"us-qwerty"`.
    fn id(&self) -> &'static str;

    /// Human-readable display name, e.g. `"US (QWERTY)"`. Reserved
    /// for a future layout-picker UI.
    fn display_name(&self) -> &'static str;

    /// Row structure for the current modifier state. Layouts that
    /// vary geometry with the modifier set (Fn-armed inserting the
    /// F-row, future IME layers, etc.) return different shapes here.
    fn rows(&self, mods: &KeyboardModifiers) -> Vec<KeyRow>;

    /// Label to paint inside `cell` given the current mods.
    fn label(&self, cell: CellId, mods: &KeyboardModifiers) -> Cow<'static, str>;

    /// Action to emit when `cell` is tapped under the current mods.
    fn emit(&self, cell: CellId, mods: &KeyboardModifiers) -> KeyAction;

    /// When non-None, the renderer paints this cell inverted whenever
    /// the corresponding modifier is armed.
    fn modifier_of(&self, _cell: CellId) -> Option<Modifier> {
        None
    }
}
