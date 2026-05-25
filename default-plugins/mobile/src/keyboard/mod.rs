//! Bottom modifier bar. A single-row strip of nine fixed cells
//! (ESC, TAB, CTRL, ALT, ←, ↓, ↑, →, -) painted at the bottom of the
//! plugin area, just above where the OS soft keyboard surfaces.
//!
//! The bar provides the keys the native mobile keyboard does not —
//! everything else (letters, digits, punctuation) is typed on the
//! native keyboard and routed straight to the focused pane via
//! `installSoftKeyboardCapture()` in `zellij-client/assets/input.js`.

pub mod controller;
pub mod layout;
pub mod modifiers;
pub mod render;

pub use controller::{KeyboardController, TapOutcome, KEY_FEEDBACK_MS};
pub use layout::CellId;
#[allow(unused_imports)]
pub use layout::KeyAction;
#[allow(unused_imports)]
pub use modifiers::{KeyboardModifiers, Modifier};
pub use render::render_modifier_bar;
