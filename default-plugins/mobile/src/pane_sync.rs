//! Reconcile plugin [`State`] against a fresh pane manifest (the
//! authoritative per-tab pane list, broadcast on every structural
//! change). Kept out of `state.rs` because these call host shims, unlike
//! the pure logic there.

use zellij_tile::prelude::*;

use crate::state::{self, Selector, State};
use crate::{clear_fit_if_active, filter_sessions_for_client};

/// Store the manifest and prune per-pane caches down to panes that still
/// exist. If the selected pane vanished, reset the local fit mirror only
/// — the server already drops its own override on close, so no
/// `set_tab_fit` shim is sent here.
pub fn refresh_pane_manifest(state: &mut State, manifest: PaneManifest) {
    state.panes_by_tab_position = manifest.panes;
    let live_pane_ids: std::collections::HashSet<PaneId> = state
        .panes_by_tab_position
        .values()
        .flat_map(|panes| panes.iter().map(state::pane_id_of))
        .collect();
    state
        .latest_pane_contents
        .retain(|id, _| live_pane_ids.contains(id));
    state
        .pane_last_activity
        .retain(|id, _| live_pane_ids.contains(id));
    if let Some(selected) = state.selected_pane_id {
        if !live_pane_ids.contains(&selected) {
            state.fit_active = false;
            state.fit_tab_id = None;
            state.last_sent_fit_size = None;
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
    let Some(pos) = state.selected_tab_position else {
        state.selected_tab_position =
            state.tabs_in_order().first().map(|t| t.position);
        return;
    };
    let still_visible = state.tabs_in_order().iter().any(|t| t.position == pos);
    if !still_visible {
        clear_fit_if_active(state);
        state.selected_tab_position =
            state.tabs_in_order().first().map(|t| t.position);
        state.selected_pane_id = None;
    }
}

/// Resolve a pending "+ New Tab" auto-select: select the new tab's first
/// pane, reset panning and any open selector, and clear the pending
/// intent. The tab position is known synchronously from
/// `new_tab_unfocused`, but the pane id only arrives with a manifest —
/// so this is a no-op until that pane shows up.
pub fn resolve_pending_new_tab(state: &mut State) {
    let Some(target) = state.pending_new_tab_position else {
        return;
    };
    let Some(id) = state
        .panes_for_tab(target)
        .into_iter()
        .next()
        .map(state::pane_id_of)
    else {
        return;
    };
    state.selected_tab_position = Some(target);
    state.selected_pane_id = Some(id);
    state.viewport_v_pan = 0;
    state.viewport_h_pan = 0;
    state.expanded = None;
    state.pending_new_tab_position = None;
}

/// Default the selection to the selected tab's first pane when none is
/// picked. Avoids `is_focused` on purpose — it is global (true if any
/// client focuses the pane), so seeding from it would track another
/// client's focus. No-op once the user has picked a pane.
pub fn ensure_pane_selected(state: &mut State) {
    if state.selected_pane_id.is_none() {
        if let Some(pane) = state.current_tab_panes().into_iter().next() {
            state.selected_pane_id = Some(state::pane_id_of(pane));
        }
    }
}

/// When the underlying pane is the session-manager welcome plugin, close
/// it and run the welcome flow natively in the Sessions selector —
/// the embedded one renders full-width and needs horizontal panning to
/// read. Fires once; the user can re-close the selector to see the raw
/// pane.
pub fn maybe_take_over_welcome(state: &mut State) {
    if state.welcome_auto_expand_done
        || state.expanded.is_some()
        || !state.current_pane_is_welcome()
    {
        return;
    }
    if let Some(pane) = state.current_pane() {
        close_plugin_pane(pane.id);
    }
    state.expanded = Some(Selector::Sessions);
    state.selector_scroll_offset = 0;
    state.menu_open = false;
    state.welcome_auto_expand_done = true;
    // The standing `SessionUpdate` only carries the current session
    // until a scan is requested, so pull the snapshot now — else the
    // selector renders empty on first show.
    if let Ok(snapshot) = get_session_list() {
        state.sessions =
            filter_sessions_for_client(snapshot.live_sessions, state);
    }
}
