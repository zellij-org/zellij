//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` / `Key`
//! events for selection and action dispatch.
//!
//! Architecture: the plugin is split into shared modules (`workspace`,
//! `fit`, `frame`, `input`, `navigation`) and one struct per screen
//! (`screens/`). `State` aggregates them; dispatch is a plain `match`
//! over `State::active`. Cross-module orchestration that no single
//! screen can own lives in the `impl State` block below.

mod ansi;
mod click;
mod fit;
mod frame;
mod input;
mod keys;
mod modifier_bar;
mod mouse;
mod navigation;
mod pane_sync;
mod render;
mod screens;
mod state;
mod top_bar;
mod workspace;

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::modifier_bar::{CellId, TapOutcome};
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        // Cache the plugin's own pane id so we can filter ourselves out
        // of the tab/pane lists. Without this, the mobile tab (which
        // contains only this plugin) becomes the selected-tab/pane and
        // the embedded viewport feedback-loops the plugin's own chrome.
        let ids = get_plugin_ids();
        self.workspace.own_plugin_pane_id = Some(PaneId::Plugin(ids.plugin_id));

        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Key,
            EventType::Mouse,
            EventType::PaneRenderReportWithAnsi,
            EventType::SessionUpdate,
            // Drives `soft_keyboard_visible`, which gates the modifier
            // bar so the bar appears and disappears in lockstep with the
            // browser's OS keyboard.
            EventType::SoftKeyboardVisibilityChanged,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let should_render = match event {
            Event::ModeUpdate(mode_info) => {
                self.workspace.mode_info = Some(mode_info);
                true
            },
            Event::TabUpdate(tabs) => {
                self.workspace.tabs = tabs;
                // Default selection: the first non-mobile tab visible to
                // this client. We deliberately do NOT follow the active
                // tab here — right after EnterMobileMode the active tab
                // IS the mobile tab, and selecting it would embed our own
                // viewport.
                if self.workspace.selected_tab_position.is_none() {
                    self.workspace.selected_tab_position =
                        self.workspace.tabs_in_order().first().map(|t| t.position);
                }
                // If the previously-selected tab vanished or became
                // self-only, fall back to the first non-mobile tab.
                if let Some(pos) = self.workspace.selected_tab_position {
                    let still_visible =
                        self.workspace.tabs_in_order().iter().any(|t| t.position == pos);
                    if !still_visible {
                        self.workspace.selected_tab_position =
                            self.workspace.tabs_in_order().first().map(|t| t.position);
                    }
                }
                sync_shadow_focus(self);
                true
            },
            Event::PaneUpdate(manifest) => {
                pane_sync::refresh_pane_manifest(self, manifest);
                pane_sync::reconcile_selected_tab(self);
                pane_sync::resolve_pending_new_tab(self);
                pane_sync::ensure_pane_selected(self);
                sync_shadow_focus(self);
                pane_sync::maybe_take_over_welcome(self);
                true
            },
            Event::SessionUpdate(sessions, _) => {
                // Capture this client's session name for the top bar and
                // the full session list for the session selector.
                if let Some(current) = sessions.iter().find(|s| s.is_current_session) {
                    self.workspace.session_name = Some(current.name.clone());
                }
                let filtered = filter_sessions_for_client(sessions, self);
                self.sessions.sessions = filtered;
                true
            },
            Event::PaneRenderReportWithAnsi(map) => {
                let now = unix_now();
                for id in map.keys() {
                    self.workspace.pane_last_activity.insert(*id, now);
                }
                // extend because we only get changed panes
                self.workspace.latest_pane_contents.extend(map);
                if self.active == ActiveScreen::Panes {
                    refresh_pane_titles(self);
                }
                true
            },
            Event::Mouse(mouse) => {
                match mouse {
                    Mouse::ScrollUp(lines) => return mouse::scroll_or_pan(self, lines, /*up=*/ true),
                    Mouse::ScrollDown(lines) => {
                        return mouse::scroll_or_pan(self, lines, /*up=*/ false)
                    },
                    Mouse::ScrollRight(cols) => {
                        return mouse::pan_horizontally(self, cols, /*right=*/ true)
                    },
                    Mouse::ScrollLeft(cols) => {
                        return mouse::pan_horizontally(self, cols, /*right=*/ false)
                    },
                    Mouse::LeftClick(..) => {
                        if let Some((line, col)) = mouse.position() {
                            return mouse::handle_left_click(self, line, col);
                        }
                    },
                    _ => {},
                }
                false
            },
            Event::Key(key) => match self.active {
                // While a selector / prompt is up, the active screen
                // captures every key for its own input buffer instead of
                // forwarding to the embedded pane.
                ActiveScreen::NewSessionPrompt => {
                    self.new_session.handle_key(&mut self.active, key)
                },
                ActiveScreen::Sessions => {
                    self.sessions
                        .handle_key(&mut self.active, &mut self.navigation, key)
                },
                ActiveScreen::Panes => {
                    if let Some((tab_position, pane_id)) = self.panes.handle_key(
                        &mut self.active,
                        &mut self.navigation,
                        &self.workspace,
                        key,
                    ) {
                        self.select_pane(tab_position, pane_id);
                    }
                    true
                },
                ActiveScreen::Viewport => {
                    // Esc dismisses an open dropdown menu in a single
                    // press; otherwise the key forwards to the pane.
                    if key.bare_key == BareKey::Esc && self.menu.open {
                        self.menu.open = false;
                        true
                    } else {
                        self.viewport.handle_key(&self.workspace, &mut self.input, key)
                    }
                },
            },
            Event::SoftKeyboardVisibilityChanged(visible) => {
                if self.frame.soft_keyboard_visible == visible {
                    return false;
                }
                self.frame.soft_keyboard_visible = visible;
                // The soft-keyboard bar is part of the plugin's chrome,
                // so toggling it changes the embedded area an active fit
                // must track — the end-of-update reconcile below pushes
                // the new size.
                true
            },
            _ => false,
        };
        // Single fit reconcile point. Any event that changed the embedded
        // area pushes the new `Size` here, deduped against the last push.
        if self.fit.active {
            let suppress_top_bar = self.sessions.is_welcome_screen
                || self.active == ActiveScreen::Sessions;
            self.fit
                .notify_size(&self.workspace, &self.frame, suppress_top_bar);
        }
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if rows == 0 || cols == 0 {
            return;
        }
        if self.workspace.tabs.is_empty() && self.workspace.panes_by_tab_position.is_empty() {
            render::render_stub(self, rows, cols);
            return;
        }
        render::render(self, rows, cols);
    }
}

/// Cross-module orchestration the click dispatcher and key handlers
/// invoke. Each method coordinates a state change that spans more than
/// one screen / shared module, so it lives here on `State` rather than on
/// any single screen.
impl State {
    /// Open the Sessions selector. Selectors and the hamburger menu are
    /// mutually exclusive; opening one clears the menu and resets the
    /// scroll + search. Kicks a peer-session scan and adopts the
    /// snapshot synchronously so the list is populated this tick.
    pub fn open_sessions(&mut self) -> bool {
        self.menu.open = false;
        self.navigation.selector_scroll_offset = 0;
        self.sessions.welcome_search.clear();
        if let Ok(snapshot) = get_session_list() {
            let filtered = filter_sessions_for_client(snapshot.live_sessions, self);
            self.sessions.sessions = filtered;
        }
        self.active = ActiveScreen::Sessions;
        true
    }

    /// Open the unified Panes selector. Refreshes titles once on open so
    /// the menu doesn't show the stale `Pane #N` placeholder.
    pub fn open_panes(&mut self) -> bool {
        self.menu.open = false;
        self.navigation.selector_scroll_offset = 0;
        self.panes.panes_search.clear();
        refresh_pane_titles(self);
        self.active = ActiveScreen::Panes;
        true
    }

    /// Close any open selector and return to the viewport, clearing both
    /// selectors' search buffers and the scroll offset.
    pub fn collapse_selector(&mut self) -> bool {
        self.active = ActiveScreen::Viewport;
        self.sessions.welcome_search.clear();
        self.panes.panes_search.clear();
        self.navigation.selector_scroll_offset = 0;
        true
    }

    /// Apply an internal pane selection (the mobile plugin never moves
    /// the client's real focus — that would dismount the mobile UI). Any
    /// active fit is invalidated since fit is bound to the pane that was
    /// focused when toggled on.
    pub fn select_pane(&mut self, tab_position: usize, pane_id: PaneId) -> bool {
        self.fit.clear_if_active();
        self.workspace.selected_tab_position = Some(tab_position);
        self.workspace.selected_pane_id = Some(pane_id);
        self.viewport.reset_pan();
        self.active = ActiveScreen::Viewport;
        sync_shadow_focus(self);
        true
    }

    /// Toggle the fit-to-screen override for the focused pane's tab.
    pub fn toggle_fit(&mut self) -> bool {
        let suppress_top_bar =
            self.sessions.is_welcome_screen || self.active == ActiveScreen::Sessions;
        self.fit.toggle(&self.workspace, &self.frame, suppress_top_bar)
    }

    /// Create a new tiled pane in `tab_position` and auto-select it. The
    /// client's real focus never changes (tiled-pane creation), so the
    /// mobile UI stays mounted.
    pub fn new_pane_in_tab(&mut self, tab_position: usize) -> bool {
        self.fit.clear_if_active();
        if let Some(new_id) = new_tiled_pane_in_tab(tab_position) {
            self.workspace.selected_tab_position = Some(tab_position);
            self.workspace.selected_pane_id = Some(new_id);
            self.viewport.reset_pan();
            self.active = ActiveScreen::Viewport;
            sync_shadow_focus(self);
        }
        true
    }

    /// Create a new tab without moving the client's focus. The new tab's
    /// first pane has not yet appeared in our manifest, so stash the
    /// position and resolve it in the next `PaneUpdate`.
    pub fn new_tab(&mut self) -> bool {
        self.fit.clear_if_active();
        if let Some(tab_position) = new_tab_unfocused::<&str>(None, None) {
            self.workspace.pending_new_tab_position = Some(tab_position);
        }
        true
    }

    /// Open the in-plugin "+ New Session" name-entry prompt.
    pub fn open_new_session_prompt(&mut self) -> bool {
        self.new_session.open(&mut self.active)
    }

    /// [Cancel] the New Session prompt.
    pub fn cancel_new_session_prompt(&mut self) -> bool {
        self.new_session.cancel(&mut self.active)
    }

    /// [Accept] the New Session prompt.
    pub fn accept_new_session_prompt(&mut self) -> bool {
        self.new_session.accept(&mut self.active)
    }

    /// Handle a modifier-bar cell tap: resolve to bytes (written to the
    /// selected pane) or a modifier flip.
    pub fn keyboard_tap(&mut self, cell: CellId) -> bool {
        let outcome = self.input.handle_tap(cell);
        match outcome {
            TapOutcome::SendBytes(bytes) => {
                if let Some(pane) = self.workspace.current_pane() {
                    if !bytes.is_empty() {
                        write_to_pane_id(bytes, pane_id_of(&pane));
                    }
                }
            },
            TapOutcome::Toggled | TapOutcome::NoOp => {},
        }
        true
    }
}

/// Restrict the session list to entries this client is allowed to see.
/// When the mobile plugin is driven by a web client, sessions whose
/// `web_clients_allowed` is `false` are hidden. Welcome-screen sessions
/// are always dropped
pub fn filter_sessions_for_client(
    sessions: Vec<SessionInfo>,
    state: &State,
) -> Vec<SessionInfo> {
    let is_web_client = state
        .workspace
        .mode_info
        .as_ref()
        .and_then(|m| m.is_web_client)
        .unwrap_or(false);
    sessions
        .into_iter()
        .filter(|s| !is_welcome_session(s))
        .filter(|s| !is_web_client || s.web_clients_allowed)
        .collect()
}

/// True if any pane inside the session is running the welcome-screen
/// plugin alias. Welcome sessions are created automatically for every
/// browser tab landing on the base URL and are not meaningful attach
/// targets.
fn is_welcome_session(session: &SessionInfo) -> bool {
    session
        .panes
        .panes
        .values()
        .flatten()
        .any(|p| p.is_plugin && p.plugin_url.as_deref() == Some("welcome-screen"))
}

/// Push the mobile plugin's currently-selected pane to the server as the
/// client's shadow focus, so other connected clients see the mobile
/// focus marker. No-op when no pane is resolvable.
pub fn sync_shadow_focus(state: &State) {
    if let Some(pane) = state.workspace.current_pane() {
        set_mobile_focused_pane(pane_id_of(&pane));
    }
}

/// Wall-clock seconds since the unix epoch, as returned by the
/// wasi-clocks shim. Used to stamp `pane_last_activity` and compute the
/// `<time> ago` deltas in the Panes selector.
pub fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Replace each cached pane's `title` with the latest value from the
/// host. Called on every render of the Panes selector so the menu always
/// reflects the shell's current title rather than the stale `Pane #N`
/// placeholder. `PaneUpdate` only fires on structural changes, so OSC 2
/// title sequences are otherwise missed; `get_pane_info` runs a fresh
/// `pane_info_for_pane` on the server reflecting the most-recent OSC 2.
pub fn refresh_pane_titles(state: &mut State) {
    let pane_ids: Vec<PaneId> = state
        .workspace
        .panes_by_tab_position
        .values()
        .flat_map(|panes| panes.iter().map(pane_id_of))
        .collect();
    for id in pane_ids {
        let Some(fresh) = get_pane_info(id) else {
            continue;
        };
        for panes in state.workspace.panes_by_tab_position.values_mut() {
            for p in panes.iter_mut() {
                if pane_id_of(p) == id {
                    p.title = fresh.title.clone();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::click::{self, ClickAction};
    use zellij_tile::prelude::{PaneInfo, TabInfo};

    /// Build a `State` seeded with one tab + one pane — the minimum
    /// surface required for the `ToggleFit` dispatch path.
    fn fit_ready_state() -> State {
        let mut state = State::default();
        let tab = TabInfo {
            position: 0,
            name: "shell".to_string(),
            tab_id: 7,
            display_area_rows: 24,
            display_area_columns: 80,
            viewport_rows: 22,
            viewport_columns: 80,
            ..TabInfo::default()
        };
        state.workspace.tabs.push(tab);
        state.workspace.selected_tab_position = Some(0);
        let pane = PaneInfo {
            id: 3,
            is_plugin: false,
            is_selectable: true,
            pane_rows: 22,
            pane_columns: 80,
            pane_content_rows: 20,
            pane_content_columns: 78,
            ..PaneInfo::default()
        };
        state.workspace.panes_by_tab_position.insert(0, vec![pane]);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(3));
        state
    }

    /// `dispatch(ToggleFit)` from the OFF state arms fit and records the
    /// bound tab.
    #[test]
    fn dispatch_toggle_fit_on_path_seeds_fields() {
        let mut state = fit_ready_state();
        assert!(!state.fit.active, "Pre-condition: fit is off");
        let consumed = click::dispatch(&mut state, ClickAction::ToggleFit);
        assert!(consumed);
        assert!(state.fit.active);
        assert_eq!(state.fit.tab_id, Some(7));
    }

    /// `dispatch(ToggleFit)` from the ON state clears both fit fields.
    #[test]
    fn dispatch_toggle_fit_off_path_clears_fields() {
        let mut state = fit_ready_state();
        state.fit.active = true;
        state.fit.tab_id = Some(7);
        let consumed = click::dispatch(&mut state, ClickAction::ToggleFit);
        assert!(consumed);
        assert!(!state.fit.active);
        assert_eq!(state.fit.tab_id, None);
    }

    /// `PaneUpdate` whose manifest no longer contains the selected pane
    /// resets the local fit mirror (no `set_tab_fit` shim is sent — the
    /// server already tore down its override on close).
    #[test]
    fn pane_update_clears_fit_when_selected_pane_disappears() {
        let mut state = fit_ready_state();
        state.fit.active = true;
        state.fit.tab_id = Some(7);

        let replacement_pane = PaneInfo {
            id: 99,
            is_plugin: false,
            is_selectable: true,
            ..PaneInfo::default()
        };
        let mut panes = std::collections::HashMap::new();
        panes.insert(0_usize, vec![replacement_pane]);
        let manifest = PaneManifest { panes };

        state.update(Event::PaneUpdate(manifest));

        assert!(!state.fit.active, "Local fit mirror cleared");
        assert_eq!(state.fit.tab_id, None);
    }

    /// The "+ New Tab" resolver promotes `pending_new_tab_position` into
    /// both selection fields, closes the selector, and clears the pending
    /// field once the new tab's first pane appears in the manifest.
    #[test]
    fn pane_update_resolves_pending_new_tab() {
        let mut state = State::default();
        let mut tab0 = TabInfo::default();
        tab0.position = 0;
        state.workspace.tabs.push(tab0);
        let mut pane0 = PaneInfo::default();
        pane0.id = 1;
        pane0.is_plugin = false;
        pane0.is_selectable = true;
        state.workspace.panes_by_tab_position.insert(0, vec![pane0]);
        state.workspace.selected_tab_position = Some(0);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(1));
        state.active = ActiveScreen::Panes;
        state.workspace.pending_new_tab_position = Some(1);

        let mut new_tab = TabInfo::default();
        new_tab.position = 1;
        state.workspace.tabs.push(new_tab);
        let mut new_pane = PaneInfo::default();
        new_pane.id = 7;
        new_pane.is_plugin = false;
        new_pane.is_selectable = true;
        let mut panes_map = std::collections::HashMap::new();
        panes_map.insert(
            0_usize,
            vec![PaneInfo {
                id: 1,
                is_plugin: false,
                is_selectable: true,
                ..PaneInfo::default()
            }],
        );
        panes_map.insert(1_usize, vec![new_pane]);
        let manifest = PaneManifest { panes: panes_map };

        state.update(Event::PaneUpdate(manifest));

        assert_eq!(state.workspace.selected_tab_position, Some(1));
        assert_eq!(state.workspace.selected_pane_id, Some(PaneId::Terminal(7)));
        assert_eq!(state.active, ActiveScreen::Viewport);
        assert_eq!(state.workspace.pending_new_tab_position, None);
    }

    /// If the matching pane has not yet arrived, the resolver leaves
    /// `pending_new_tab_position` in place for a later `PaneUpdate`.
    #[test]
    fn pane_update_keeps_pending_when_target_tab_empty() {
        let mut state = State::default();
        let mut tab0 = TabInfo::default();
        tab0.position = 0;
        state.workspace.tabs.push(tab0);
        let mut pane0 = PaneInfo::default();
        pane0.id = 1;
        pane0.is_plugin = false;
        pane0.is_selectable = true;
        state.workspace.panes_by_tab_position.insert(0, vec![pane0]);
        state.workspace.selected_tab_position = Some(0);
        state.workspace.selected_pane_id = Some(PaneId::Terminal(1));
        state.workspace.pending_new_tab_position = Some(5);

        let mut panes_map = std::collections::HashMap::new();
        panes_map.insert(
            0_usize,
            vec![PaneInfo {
                id: 1,
                is_plugin: false,
                is_selectable: true,
                ..PaneInfo::default()
            }],
        );
        let manifest = PaneManifest { panes: panes_map };
        state.update(Event::PaneUpdate(manifest));

        assert_eq!(state.workspace.pending_new_tab_position, Some(5));
        assert_eq!(state.workspace.selected_tab_position, Some(0));
    }
}
