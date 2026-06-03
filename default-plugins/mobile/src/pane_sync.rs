//! Reconcile plugin [`State`] against a fresh pane manifest (the
//! authoritative per-tab pane list, broadcast on every structural
//! change). Kept out of `workspace.rs` because these call host shims,
//! unlike the pure accessors there.

use zellij_tile::prelude::*;

use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;
use crate::filter_sessions_for_client;

/// Store the manifest and prune per-pane caches down to panes that still
/// exist. If the selected pane vanished, reset the local fit mirror only
/// — the server already drops its own override on close, so no
/// `set_tab_fit` shim is sent here.
pub fn refresh_pane_manifest(state: &mut State, manifest: PaneManifest) {
    state.workspace.panes_by_tab_position = manifest.panes;
    let live_pane_ids: std::collections::HashSet<PaneId> = state
        .workspace
        .panes_by_tab_position
        .values()
        .flat_map(|panes| panes.iter().map(pane_id_of))
        .collect();
    state
        .workspace
        .latest_pane_contents
        .retain(|id, _| live_pane_ids.contains(id));
    state
        .workspace
        .pane_last_activity
        .retain(|id, _| live_pane_ids.contains(id));
    if let Some(selected) = state.workspace.selected_pane_id {
        if !live_pane_ids.contains(&selected) {
            state.fit.reset_local();
        }
    }
}

/// Keep the selected tab valid and visible. If it vanished, clear its
/// bound fit (the server keys the override by tab_id, lost once the tab
/// is gone) and fall back to the first non-mobile tab, dropping the pane
/// selection too. If none was selected, default to the first.
///
/// Re-run on every manifest, not just tab changes: tab visibility is
/// derived from pane data.
pub fn reconcile_selected_tab(state: &mut State) {
    let Some(pos) = state.workspace.selected_tab_position else {
        state.workspace.selected_tab_position =
            state.workspace.tabs_in_order().first().map(|t| t.position);
        return;
    };
    let still_visible = state
        .workspace
        .tabs_in_order()
        .iter()
        .any(|t| t.position == pos);
    if !still_visible {
        state.fit.clear_if_active();
        state.workspace.selected_tab_position =
            state.workspace.tabs_in_order().first().map(|t| t.position);
        state.workspace.selected_pane_id = None;
    }
}

/// Resolve a pending "+ New Tab" auto-select: select the new tab's first
/// pane, reset panning and any open selector, and clear the pending
/// intent. The tab position is known synchronously from
/// `new_tab_unfocused`, but the pane id only arrives with a manifest —
/// so this is a no-op until that pane shows up.
pub fn resolve_pending_new_tab(state: &mut State) {
    let Some(target) = state.workspace.pending_new_tab_position else {
        return;
    };
    let Some(id) = state
        .workspace
        .panes_for_tab(target)
        .into_iter()
        .next()
        .map(pane_id_of)
    else {
        return;
    };
    state.workspace.selected_tab_position = Some(target);
    state.workspace.selected_pane_id = Some(id);
    state.viewport.reset_pan();
    state.active = ActiveScreen::Viewport;
    state.workspace.pending_new_tab_position = None;
}

/// Default the selection to the selected tab's first pane when none is
/// picked. Avoids `is_focused` on purpose — it is global (true if any
/// client focuses the pane), so seeding from it would track another
/// client's focus. No-op once the user has picked a pane.
pub fn ensure_pane_selected(state: &mut State) {
    if state.workspace.selected_pane_id.is_none() {
        if let Some(pane) = state.workspace.current_tab_panes().into_iter().next() {
            state.workspace.selected_pane_id = Some(pane_id_of(pane));
        }
    }
}

/// When the underlying pane is the session-manager welcome plugin, close
/// it and run the welcome flow natively in the Sessions selector — the
/// embedded one renders full-width and needs horizontal panning to read.
/// Fires once; the user can re-close the selector to see the raw pane.
pub fn maybe_take_over_welcome(state: &mut State) {
    if state.sessions.is_welcome_screen
        || state.active != ActiveScreen::Viewport
        || !state.workspace.current_pane_is_welcome()
    {
        return;
    }
    if let Some(pane) = state.workspace.current_pane() {
        close_plugin_pane(pane.id);
    }
    state.active = ActiveScreen::Sessions;
    state.navigation.selector_scroll_offset = 0;
    state.menu.open = false;
    state.sessions.is_welcome_screen = true;
    // The standing `SessionUpdate` only carries the current session until
    // a scan is requested, so pull the snapshot now — else the selector
    // renders empty on first show.
    if let Ok(snapshot) = get_session_list() {
        let filtered = filter_sessions_for_client(snapshot.live_sessions, state);
        state.sessions.sessions = filtered;
    }
}
