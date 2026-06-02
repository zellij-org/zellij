//! Top-level aggregate for the mobile UI plugin. `State` is the plugin
//! struct (`register_plugin!(State)`); it owns the shared session model
//! and chrome (`workspace` / `fit` / `frame` / `input` / `navigation`)
//! plus one struct per screen, each of which keeps its own private UI
//! state. Dispatch is a plain `match` over `active` — see `State::update`
//! / `State::render` in `main.rs`.
//!
//! Cross-module orchestration that a single screen cannot own (opening a
//! selector, applying a pane selection, toggling fit) lives as
//! `impl State` methods in `main.rs`, where every module is reachable.

use crate::fit::Fit;
use crate::frame::Frame;
use crate::input::Input;
use crate::navigation::Navigation;
use crate::screens::{
    ActiveScreen, MenuScreen, NewSessionPromptScreen, PanesScreen, SessionsScreen,
    ViewportScreen,
};
use crate::workspace::Workspace;

#[derive(Default)]
pub struct State {
    // Shared modules (sibling fields → disjoint borrows).
    /// Mirror of the live Zellij session: tabs, panes, selection,
    /// cached viewports, activity.
    pub workspace: Workspace,
    /// Fit-to-screen override mirror.
    pub fit: Fit,
    /// Per-frame render scratch + chrome flags.
    pub frame: Frame,
    /// Sticky modifiers + on-screen modifier bar.
    pub input: Input,
    /// Selector scroll offset + shared fuzzy matcher.
    pub navigation: Navigation,
    // Active body screen; the hamburger menu overlay is tracked by
    // `menu.open` and only ever overlays the Viewport screen.
    pub active: ActiveScreen,
    // Screen structs (own their private UI state).
    pub viewport: ViewportScreen,
    pub sessions: SessionsScreen,
    pub panes: PanesScreen,
    pub new_session: NewSessionPromptScreen,
    pub menu: MenuScreen,
}
