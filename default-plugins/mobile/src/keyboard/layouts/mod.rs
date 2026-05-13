//! Registry of available `KeyboardLayout` implementations.
//!
//! v1 ships a single US-QWERTY layout. Adding a new layout consists
//! of: drop a file in this directory, implement `KeyboardLayout`,
//! register it here. Nothing else in the crate needs editing.

pub mod us_qwerty;

use super::layout::KeyboardLayout;

/// Layout used on first attach. Returned as a `Box<dyn>` so a future
/// layout-picker can swap it without touching `KeyboardController`.
pub fn default_layout() -> Box<dyn KeyboardLayout> {
    Box::new(us_qwerty::UsQwerty::new())
}

/// Every registered layout. Reserved for a future picker UI — not
/// referenced anywhere in v1, kept so the registry surface is
/// already shaped when layouts beyond US-QWERTY land.
#[allow(dead_code)]
pub fn all_layouts() -> Vec<Box<dyn KeyboardLayout>> {
    vec![Box::new(us_qwerty::UsQwerty::new())]
}
