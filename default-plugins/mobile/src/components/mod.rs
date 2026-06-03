//! Standalone UI components shared across the mobile plugin's screens.
//!
//! Unlike the per-screen bodies in `screens/`, these are reusable chrome
//! widgets rendered alongside whichever screen is active: the shared
//! `top_bar` at the top of every non-welcome screen, and the bottom
//! `modifier_bar` that surfaces the keys the native soft keyboard lacks.
//! Each component owns its rendering and supporting logic in a single
//! file.

pub mod modifier_bar;
pub mod top_bar;
