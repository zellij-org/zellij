//! Layout-trait + per-cell data types for the in-plugin keyboard.
//!
//! Each concrete layout (US-QWERTY today; future QWERTZ, AZERTY,
//! Dvorak, JP-romaji, …) implements `KeyboardLayout`. The renderer
//! and click dispatcher never inspect a `CellId` directly — they only
//! call trait methods. That is the modularity contract that lets new
//! layouts drop in without touching `render.rs` / `controller.rs`.

use std::borrow::Cow;
use zellij_tile::prelude::*;

use super::modifiers::{KeyLayer, KeyboardModifiers, Modifier};

/// Opaque per-layout cell identifier. The renderer and dispatcher
/// only round-trip these; only the owning layout knows what each id
/// means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellId(pub u16);

/// One physical cell within a rendered row.
///
/// `col_start` and `col_end` are columns relative to the row's
/// `offset_col` (which is itself relative to the keyboard's left
/// edge). Cell width is `col_end - col_start`; the label paints across
/// the full width (the renderer centers it).
#[derive(Debug, Clone, Copy)]
pub struct KeyCell {
    pub col_start: u16,
    pub col_end: u16,
    pub id: CellId,
}

/// One row of cells, indented `offset_col` from the keyboard's left
/// edge. Letters row 2 uses `offset_col = 2` to approximate the
/// half-key stagger of a physical keyboard.
///
/// `height` is the number of terminal rows the row occupies. Most rows
/// are 1 row tall (extras strip, bottom bar, modifier rows). Character
/// rows under option 2b are 2 rows tall — the upper row is blank
/// padding that paints the cell's background colour, the lower row
/// carries the label. The renderer reads `height` and decides whether
/// to emit a padding row before the label row; click regions cover
/// the full vertical span.
pub struct KeyRow {
    pub offset_col: u16,
    pub cells: Vec<KeyCell>,
    pub height: u8,
    /// Extra unstyled columns appended after the last cell's right
    /// edge. The renderer emits this many trailing spaces with no bg
    /// colour, giving the row a visual right-margin without making
    /// the margin a tap target. Click regions are unaffected; the
    /// layer's `block_width` calculation includes the padding so the
    /// row participates in centering at its full visual width.
    pub right_pad: u16,
}

impl KeyRow {
    /// One-terminal-row construction. Used for the extras strip, the
    /// bottom bar, and any layout that wants the dense original look.
    pub fn single(offset_col: u16, cells: Vec<KeyCell>) -> Self {
        Self { offset_col, cells, height: 1, right_pad: 0 }
    }

    /// Two-terminal-row construction (option 2b). The upper row is
    /// blank padding; the lower row carries the label. Used for
    /// character rows on US-QWERTY to raise touch-target height to ~72
    /// px on a typical phone-portrait viewport.
    pub fn tall(offset_col: u16, cells: Vec<KeyCell>) -> Self {
        Self { offset_col, cells, height: 2, right_pad: 0 }
    }

    /// Builder-style setter for the trailing unstyled padding. Use to
    /// give a row visual breathing room past its rightmost cell.
    pub fn with_right_pad(mut self, right_pad: u16) -> Self {
        self.right_pad = right_pad;
        self
    }
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
    /// Switch the keyboard's active layer. Modifier state is *not*
    /// consumed by this action.
    SwitchLayer(KeyLayer),
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

    /// Row structure for the current modifier state. Layouts return
    /// different shapes per layer (e.g. Fn layer has fewer rows than
    /// Letters / Symbols).
    fn rows(&self, mods: &KeyboardModifiers) -> Vec<KeyRow>;

    /// Compact-tier rows for narrow / short viewports.
    ///
    /// `target_block_width` is the width in cells (= `cols - 2 *
    /// MIN_H_PAD`) that each compact row should collectively fill.
    /// Implementations that handle the compact tier construct rows
    /// whose post-scaled extent matches this width — homogeneous
    /// rows lean on the renderer's per-row scaling primitive while
    /// rows with fixed-width anchors return cells whose
    /// `col_start`/`col_end` already carry post-stretch absolute
    /// positions and use a `(1, 1)` per-row scale.
    ///
    /// The default returns `self.rows(mods)` unchanged so layouts
    /// that have not been audited for compact-tier rendering simply
    /// re-use their natural rows. They are still subject to the
    /// natural-tier "doesn't fit" test inside the renderer — if the
    /// natural rows do not fit at the compact dimensions, the
    /// keyboard is suppressed instead.
    fn compact_rows(
        &self,
        mods: &KeyboardModifiers,
        target_block_width: u16,
    ) -> Vec<KeyRow> {
        let _ = target_block_width;
        self.rows(mods)
    }

    /// Per-row `(num, den)` scale factors applied by the renderer
    /// when laying out compact-tier rows. Must match the row order
    /// returned by `compact_rows(mods, target_block_width)`.
    ///
    /// The default empty vector is treated as "fall back to the
    /// global `h_num/h_den`" — appropriate for the default
    /// `compact_rows` impl, which just forwards the natural rows.
    fn compact_row_scales(
        &self,
        mods: &KeyboardModifiers,
        target_block_width: u16,
    ) -> Vec<(u16, u16)> {
        let _ = (mods, target_block_width);
        Vec::new()
    }

    /// Bare label for `cell` given the current mods. The renderer
    /// centers it inside the cell width.
    fn label(&self, cell: CellId, mods: &KeyboardModifiers) -> Cow<'static, str>;

    /// Action to emit when `cell` is tapped under the current mods.
    fn emit(&self, cell: CellId, mods: &KeyboardModifiers) -> KeyAction;

    /// When non-None, the renderer paints this cell ACTIVE whenever
    /// the corresponding modifier is armed.
    fn modifier_of(&self, _cell: CellId) -> Option<Modifier> {
        None
    }

    /// When non-None, the renderer paints this cell ACTIVE whenever
    /// the keyboard's active layer matches the returned variant. Used
    /// by the Fn cell on the Functions layer; the layer-toggle cell
    /// (which switches *to* the alternate layer) deliberately returns
    /// `None` so it never paints active.
    fn layer_of(&self, _cell: CellId) -> Option<KeyLayer> {
        None
    }
}
