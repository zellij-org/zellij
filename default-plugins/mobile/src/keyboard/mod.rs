//! In-plugin on-screen keyboard. Replaces the browser's system soft
//! keyboard with one rendered by the plugin and driven by SGR mouse
//! clicks routed through the existing touch → click pipeline.
//!
//! The three concerns separate cleanly:
//! - **Layout** (per-language data) — `layout` + `layouts/`.
//! - **Controller** (universal state) — `controller`.
//! - **Renderer & dispatch** (universal logic) — `render`.
//!
//! A new layout drops into `layouts/`, implements `KeyboardLayout`,
//! registers itself in `layouts/mod.rs`. The renderer, controller and
//! modifier state machine never inspect a `CellId` — only the owning
//! layout knows what each cell means.

pub mod controller;
pub mod layout;
pub mod layouts;
pub mod modifiers;
pub mod render;

// Public surface re-exports. The `#[allow]` covers the items that are
// part of the module's public API but not yet imported by name
// elsewhere in the crate — they will be used by future layout
// switches / picker UIs.
#[allow(unused_imports)]
pub use controller::{KeyboardController, TapOutcome, KEY_FEEDBACK_MS};
#[allow(unused_imports)]
pub use layout::{CellId, KeyAction, KeyCell, KeyRow, KeyboardLayout};
#[allow(unused_imports)]
pub use modifiers::{KeyboardModifiers, Modifier};
#[allow(unused_imports)]
pub use render::render_keyboard;
