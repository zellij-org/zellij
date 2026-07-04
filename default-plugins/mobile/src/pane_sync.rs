use zellij_tile::prelude::*;

use crate::filter_sessions_for_client;
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;

const EMPTY_WELCOME_LIST_GRACE_SECS: f64 = 0.4;

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

pub fn ensure_pane_selected(state: &mut State) {
    if state.workspace.selected_pane_id.is_none() {
        if let Some(pane) = state.workspace.current_tab_panes().into_iter().next() {
            state.workspace.selected_pane_id = Some(pane_id_of(pane));
        }
    }
}

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
    if let Ok(snapshot) = get_session_list() {
        let filtered = filter_sessions_for_client(snapshot.live_sessions, state);
        state.sessions.sessions = filtered;
    }
    set_timeout(EMPTY_WELCOME_LIST_GRACE_SECS);
}
