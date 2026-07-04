use std::collections::{BTreeMap, HashMap, HashSet};

use zellij_utils::errors::prelude::*;
use zellij_utils::input::layout::{Run, RunPluginOrAlias, TiledPaneLayout};
use zellij_utils::pane_size::Size;

use crate::panes::PaneId;
use crate::tab::Tab;
use crate::ClientId;

pub(crate) const FIT_RESIZE_MAX_ITERS: usize = 3;

const MOBILE_PLUGIN_URL: &str = "zellij:mobile";

#[derive(Debug, Default)]
pub(crate) struct MobileState {
    mobile_tab_for_client: HashMap<ClientId, usize>,
    tab_before_mobile_for_client: HashMap<ClientId, usize>,
    auto_entered_clients: HashSet<ClientId>,
    fit_override_for_tab: HashMap<usize, FitOverride>,
}

#[derive(Debug, Clone, Copy)]
struct FitOverride {
    owning_client: ClientId,
    fullscreened_pane: PaneId,
    embedded_content_size: Size,
    pane_was_fullscreen_before_fit: bool,
}

const CLEAR_SCREEN: &str = "\u{1b}[2J\u{1b}[H";

#[derive(Debug, Default)]
pub(crate) struct MobileRenderGate {
    awaiting_first_render: HashSet<ClientId>,
    clients_with_settled_size: HashSet<ClientId>,
    last_paint_size: HashMap<ClientId, Size>,
}

impl MobileRenderGate {
    pub(crate) fn gate(&mut self, client_id: ClientId) {
        self.awaiting_first_render.insert(client_id);
    }

    pub(crate) fn is_gated(&self, client_id: ClientId) -> bool {
        self.awaiting_first_render.contains(&client_id)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.awaiting_first_render.is_empty()
    }

    pub(crate) fn gated_clients(&self) -> Vec<ClientId> {
        self.awaiting_first_render.iter().copied().collect()
    }

    pub(crate) fn ungate(&mut self, client_id: ClientId) {
        self.awaiting_first_render.remove(&client_id);
        self.clients_with_settled_size.remove(&client_id);
        self.last_paint_size.remove(&client_id);
    }

    pub(crate) fn record_settled_size(&mut self, client_id: ClientId) {
        self.clients_with_settled_size.insert(client_id);
    }

    pub(crate) fn record_paint_size(&mut self, client_id: ClientId, size: Size) {
        self.last_paint_size.insert(client_id, size);
    }

    pub(crate) fn try_reveal(
        &mut self,
        client_id: ClientId,
        is_web_client: bool,
        reported_size: Option<Size>,
    ) -> bool {
        if !self.is_gated(client_id) {
            return false;
        }
        let paint_size = self.last_paint_size.get(&client_id).copied();
        let size_settled = self.clients_with_settled_size.contains(&client_id);
        let ready_to_reveal = match paint_size {
            None => false,
            Some(paint_size) if is_web_client => size_settled && reported_size == Some(paint_size),
            Some(_) => true,
        };
        if ready_to_reveal {
            self.ungate(client_id);
        }
        ready_to_reveal
    }

    pub(crate) fn blank_gated_clients(&self, serialized_output: &mut HashMap<ClientId, String>) {
        for (client_id, bytes) in serialized_output.iter_mut() {
            if self.is_gated(*client_id) {
                *bytes = CLEAR_SCREEN.to_string();
            }
        }
    }
}

impl MobileState {
    pub(crate) fn is_in_mobile_mode(&self, client_id: ClientId) -> bool {
        self.mobile_tab_for_client.contains_key(&client_id)
    }

    pub(crate) fn mobile_tab_id(&self, client_id: ClientId) -> Option<usize> {
        self.mobile_tab_for_client.get(&client_id).copied()
    }

    pub(crate) fn mobile_tab_ids(&self) -> HashSet<usize> {
        self.mobile_tab_for_client.values().copied().collect()
    }

    pub(crate) fn mobile_tab_count(&self) -> usize {
        self.mobile_tab_for_client.len()
    }

    pub(crate) fn set_previous_tab(&mut self, client_id: ClientId, previous_tab: Option<usize>) {
        if let Some(previous_tab) = previous_tab {
            self.tab_before_mobile_for_client
                .insert(client_id, previous_tab);
        }
    }

    pub(crate) fn register_tab(&mut self, client_id: ClientId, mobile_tab_id: usize) {
        self.mobile_tab_for_client.insert(client_id, mobile_tab_id);
    }

    pub(crate) fn begin_exit(&mut self, client_id: ClientId) -> Option<(usize, Option<usize>)> {
        let mobile_tab_id = self.mobile_tab_for_client.remove(&client_id)?;
        self.auto_entered_clients.remove(&client_id);
        let previous_tab = self.tab_before_mobile_for_client.remove(&client_id);
        Some((mobile_tab_id, previous_tab))
    }

    pub(crate) fn forget_client(&mut self, client_id: ClientId) {
        self.mobile_tab_for_client.remove(&client_id);
        self.tab_before_mobile_for_client.remove(&client_id);
        self.auto_entered_clients.remove(&client_id);
    }

    pub(crate) fn was_auto_entered(&self, client_id: ClientId) -> bool {
        self.auto_entered_clients.contains(&client_id)
    }

    pub(crate) fn mark_auto_entered(&mut self, client_id: ClientId) {
        self.auto_entered_clients.insert(client_id);
    }

    pub(crate) fn mobile_tab_layout() -> Result<TiledPaneLayout> {
        let mut mobile_tab_layout = TiledPaneLayout::default();
        let mobile_plugin = RunPluginOrAlias::from_url(MOBILE_PLUGIN_URL, &None, None, None)
            .map_err(|e| anyhow!("invalid mobile plugin url: {e}"))?;
        mobile_tab_layout.run = Some(Run::Plugin(mobile_plugin));
        mobile_tab_layout.borderless = Some(true);
        Ok(mobile_tab_layout)
    }

    pub(crate) fn clear_shadow_focus(&self, client_id: ClientId, tabs: &mut BTreeMap<usize, Tab>) {
        let mobile_tab_id = self.mobile_tab_id(client_id);
        for tab in tabs.values_mut() {
            if Some(tab.id) == mobile_tab_id {
                continue;
            }
            tab.clear_shadow_focus(client_id);
        }
    }

    pub(crate) fn apply_shadow_focus(
        &self,
        client_id: ClientId,
        pane_id: PaneId,
        tabs: &mut BTreeMap<usize, Tab>,
    ) -> ShadowFocusOutcome {
        let mobile_tab_id = self.mobile_tab_id(client_id);
        for tab in tabs.values() {
            if Some(tab.id) == mobile_tab_id {
                continue;
            }
            if tab.has_shadow_focus_on(client_id, pane_id) {
                return ShadowFocusOutcome::AlreadyApplied;
            }
        }

        self.clear_shadow_focus(client_id, tabs);
        for tab in tabs.values_mut() {
            if Some(tab.id) == mobile_tab_id {
                continue;
            }
            if tab.set_shadow_focus(client_id, pane_id) {
                break;
            }
        }
        ShadowFocusOutcome::NewlyApplied
    }

    pub(crate) fn has_fit(&self, tab_id: usize) -> bool {
        self.fit_override_for_tab.contains_key(&tab_id)
    }

    pub(crate) fn compute_fit_size(
        &self,
        tab_id: usize,
        tabs: &BTreeMap<usize, Tab>,
    ) -> Option<Size> {
        let fit = self.fit_override_for_tab.get(&tab_id)?;
        let target_tab = tabs.get(&tab_id)?;
        let viewport = target_tab.get_viewport();
        let display_area = target_tab.get_display_area();
        let tab_bar_rows = display_area.rows.saturating_sub(viewport.rows);
        let tab_bar_cols = display_area.cols.saturating_sub(viewport.cols);
        let (frame_rows, frame_cols) = target_tab
            .get_pane_with_id(fit.fullscreened_pane)
            .map(|pane| {
                (
                    pane.rows().saturating_sub(pane.get_content_rows()),
                    pane.cols().saturating_sub(pane.get_content_columns()),
                )
            })
            .unwrap_or((0, 0));

        Some(Size {
            rows: fit.embedded_content_size.rows + tab_bar_rows + frame_rows,
            cols: fit.embedded_content_size.cols + tab_bar_cols + frame_cols,
        })
    }

    pub(crate) fn set_fit(
        &mut self,
        client_id: ClientId,
        tab_id: usize,
        pane_id: PaneId,
        embedded_content_size: Size,
        tabs: &mut BTreeMap<usize, Tab>,
    ) {
        if let Some(existing_fit) = self.fit_override_for_tab.get_mut(&tab_id) {
            existing_fit.embedded_content_size = embedded_content_size;
            existing_fit.owning_client = client_id;
            existing_fit.fullscreened_pane = pane_id;
            return;
        }

        let pane_was_fullscreen_before_fit = tabs
            .get(&tab_id)
            .map(|tab| tab.is_fullscreen_active() && tab.fullscreen_pane_id() == Some(pane_id))
            .unwrap_or(false);
        if !pane_was_fullscreen_before_fit {
            Self::toggle_fullscreen(tabs, tab_id, pane_id);
        }
        self.fit_override_for_tab.insert(
            tab_id,
            FitOverride {
                owning_client: client_id,
                fullscreened_pane: pane_id,
                embedded_content_size,
                pane_was_fullscreen_before_fit,
            },
        );
    }

    pub(crate) fn clear_fit_owned_by(
        &mut self,
        client_id: ClientId,
        tabs: &mut BTreeMap<usize, Tab>,
    ) -> Option<usize> {
        let tab_id = self
            .fit_override_for_tab
            .iter()
            .find(|(_, fit)| fit.owning_client == client_id)
            .map(|(&tab_id, _)| tab_id)?;
        let fit = self.fit_override_for_tab.remove(&tab_id)?;
        if !fit.pane_was_fullscreen_before_fit {
            Self::toggle_fullscreen(tabs, tab_id, fit.fullscreened_pane);
        }
        Some(tab_id)
    }

    pub(crate) fn clear_fit_for_pane(&mut self, pane_id: PaneId) -> Option<usize> {
        let tab_id = self
            .fit_override_for_tab
            .iter()
            .find(|(_, fit)| fit.fullscreened_pane == pane_id)
            .map(|(&tab_id, _)| tab_id)?;
        self.fit_override_for_tab.remove(&tab_id);
        Some(tab_id)
    }

    pub(crate) fn remove_fit_for_tab(&mut self, tab_id: usize) {
        self.fit_override_for_tab.remove(&tab_id);
    }

    pub(crate) fn clear_fits_owned_by(
        &mut self,
        client_id: ClientId,
        tabs: &mut BTreeMap<usize, Tab>,
    ) -> Vec<usize> {
        let owned_fits: Vec<(usize, FitOverride)> = self
            .fit_override_for_tab
            .iter()
            .filter(|(_, fit)| fit.owning_client == client_id)
            .map(|(&tab_id, fit)| (tab_id, *fit))
            .collect();
        let mut tabs_to_recompute = Vec::with_capacity(owned_fits.len());
        for (tab_id, fit) in owned_fits {
            self.fit_override_for_tab.remove(&tab_id);
            if !fit.pane_was_fullscreen_before_fit {
                Self::toggle_fullscreen(tabs, tab_id, fit.fullscreened_pane);
            }
            tabs_to_recompute.push(tab_id);
        }
        tabs_to_recompute
    }

    fn toggle_fullscreen(tabs: &mut BTreeMap<usize, Tab>, tab_id: usize, pane_id: PaneId) {
        if let Some(tab) = tabs.get_mut(&tab_id) {
            if tab.has_pane_with_pid(&pane_id) {
                tab.toggle_pane_fullscreen(pane_id);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShadowFocusOutcome {
    NewlyApplied,
    AlreadyApplied,
}

#[cfg(test)]
impl MobileState {
    pub(crate) fn previous_tab(&self, client_id: ClientId) -> Option<usize> {
        self.tab_before_mobile_for_client.get(&client_id).copied()
    }

    pub(crate) fn fit_count(&self) -> usize {
        self.fit_override_for_tab.len()
    }

    pub(crate) fn fit_owner(&self, tab_id: usize) -> Option<ClientId> {
        self.fit_override_for_tab
            .get(&tab_id)
            .map(|fit| fit.owning_client)
    }

    pub(crate) fn fit_pane(&self, tab_id: usize) -> Option<PaneId> {
        self.fit_override_for_tab
            .get(&tab_id)
            .map(|fit| fit.fullscreened_pane)
    }

    pub(crate) fn fit_embedded_size(&self, tab_id: usize) -> Option<Size> {
        self.fit_override_for_tab
            .get(&tab_id)
            .map(|fit| fit.embedded_content_size)
    }

    pub(crate) fn fit_pane_was_fullscreen_before(&self, tab_id: usize) -> Option<bool> {
        self.fit_override_for_tab
            .get(&tab_id)
            .map(|fit| fit.pane_was_fullscreen_before_fit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLIENT_A: ClientId = 1;
    const CLIENT_B: ClientId = 2;

    fn embedded_size() -> Size {
        Size { rows: 24, cols: 80 }
    }

    fn no_tabs() -> BTreeMap<usize, Tab> {
        BTreeMap::new()
    }

    fn settled_size() -> Size {
        Size { rows: 25, cols: 31 }
    }

    #[test]
    fn try_reveal_is_false_for_an_ungated_client() {
        let mut gate = MobileRenderGate::default();
        assert!(!gate.try_reveal(CLIENT_A, true, Some(settled_size())));
    }

    #[test]
    fn web_client_is_not_revealed_before_size_settles() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_paint_size(CLIENT_A, settled_size());
        // Paint arrived but no settled size has been reported yet.
        assert!(!gate.try_reveal(CLIENT_A, true, Some(settled_size())));
        assert!(gate.is_gated(CLIENT_A));
    }

    #[test]
    fn web_client_is_not_revealed_without_a_paint() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_settled_size(CLIENT_A);
        assert!(!gate.try_reveal(CLIENT_A, true, Some(settled_size())));
        assert!(gate.is_gated(CLIENT_A));
    }

    #[test]
    fn web_client_is_not_revealed_when_reported_size_differs_from_paint() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_settled_size(CLIENT_A);
        gate.record_paint_size(CLIENT_A, settled_size());
        let stale = Size { rows: 38, cols: 46 };
        assert!(!gate.try_reveal(CLIENT_A, true, Some(stale)));
        assert!(gate.is_gated(CLIENT_A));
    }

    #[test]
    fn web_client_is_revealed_on_settled_matching_paint() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_settled_size(CLIENT_A);
        gate.record_paint_size(CLIENT_A, settled_size());
        assert!(gate.try_reveal(CLIENT_A, true, Some(settled_size())));
        assert!(!gate.is_gated(CLIENT_A));
    }

    #[test]
    fn non_web_client_is_revealed_on_any_paint() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_paint_size(CLIENT_A, embedded_size());
        // No settled size, mismatched reported size — still revealed for non-web.
        assert!(gate.try_reveal(CLIENT_A, false, None));
        assert!(!gate.is_gated(CLIENT_A));
    }

    #[test]
    fn blank_gated_clients_replaces_only_gated_output() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        let mut output = HashMap::new();
        output.insert(CLIENT_A, "real-a".to_string());
        output.insert(CLIENT_B, "real-b".to_string());
        gate.blank_gated_clients(&mut output);
        assert_eq!(output.get(&CLIENT_A).unwrap(), CLEAR_SCREEN);
        assert_eq!(output.get(&CLIENT_B).unwrap(), "real-b");
    }

    #[test]
    fn ungate_clears_settled_and_paint_state() {
        let mut gate = MobileRenderGate::default();
        gate.gate(CLIENT_A);
        gate.record_settled_size(CLIENT_A);
        gate.record_paint_size(CLIENT_A, settled_size());
        gate.ungate(CLIENT_A);
        assert!(!gate.is_gated(CLIENT_A));
        // A fresh gate must not inherit the prior settled/paint state.
        gate.gate(CLIENT_A);
        assert!(!gate.try_reveal(CLIENT_A, true, Some(settled_size())));
    }

    #[test]
    fn client_is_not_in_mobile_mode_by_default() {
        let state = MobileState::default();
        assert!(!state.is_in_mobile_mode(CLIENT_A));
        assert_eq!(state.mobile_tab_count(), 0);
        assert!(state.mobile_tab_ids().is_empty());
    }

    #[test]
    fn registering_a_tab_puts_the_client_in_mobile_mode() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        assert!(state.is_in_mobile_mode(CLIENT_A));
        assert_eq!(state.mobile_tab_count(), 1);
        assert!(state.mobile_tab_ids().contains(&7));
    }

    #[test]
    fn mobile_tab_ids_collects_every_clients_tab() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        state.register_tab(CLIENT_B, 9);
        let ids = state.mobile_tab_ids();
        assert!(ids.contains(&7));
        assert!(ids.contains(&9));
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn begin_exit_returns_the_mobile_and_previous_tabs() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        state.set_previous_tab(CLIENT_A, Some(3));
        assert_eq!(state.begin_exit(CLIENT_A), Some((7, Some(3))));
    }

    #[test]
    fn set_previous_tab_ignores_none() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        state.set_previous_tab(CLIENT_A, None);
        assert_eq!(state.begin_exit(CLIENT_A), Some((7, None)));
    }

    #[test]
    fn begin_exit_returns_none_when_not_in_mobile_mode() {
        let mut state = MobileState::default();
        assert_eq!(state.begin_exit(CLIENT_A), None);
    }

    #[test]
    fn begin_exit_clears_membership_and_auto_entry() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        state.mark_auto_entered(CLIENT_A);
        state.set_previous_tab(CLIENT_A, Some(3));

        state.begin_exit(CLIENT_A);

        assert!(!state.is_in_mobile_mode(CLIENT_A));
        assert!(!state.was_auto_entered(CLIENT_A));
        assert_eq!(state.begin_exit(CLIENT_A), None);
    }

    #[test]
    fn forget_client_drops_all_per_client_bookkeeping() {
        let mut state = MobileState::default();
        state.register_tab(CLIENT_A, 7);
        state.set_previous_tab(CLIENT_A, Some(3));
        state.mark_auto_entered(CLIENT_A);

        state.forget_client(CLIENT_A);

        assert!(!state.is_in_mobile_mode(CLIENT_A));
        assert!(!state.was_auto_entered(CLIENT_A));
    }

    #[test]
    fn auto_entered_marker_round_trips() {
        let mut state = MobileState::default();
        assert!(!state.was_auto_entered(CLIENT_A));
        state.mark_auto_entered(CLIENT_A);
        assert!(state.was_auto_entered(CLIENT_A));
    }

    #[test]
    fn mobile_tab_layout_is_a_borderless_plugin() {
        let layout = MobileState::mobile_tab_layout().expect("layout builds");
        assert_eq!(layout.borderless, Some(true));
        assert!(matches!(layout.run, Some(Run::Plugin(_))));
    }

    #[test]
    fn no_fit_override_by_default() {
        let state = MobileState::default();
        assert!(!state.has_fit(5));
    }

    #[test]
    fn set_fit_records_an_override() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        assert!(state.has_fit(5));
    }

    #[test]
    fn refitting_a_tab_reassigns_ownership() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        state.set_fit(
            CLIENT_B,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );

        assert_eq!(state.clear_fit_owned_by(CLIENT_A, &mut tabs), None);
        assert_eq!(state.clear_fit_owned_by(CLIENT_B, &mut tabs), Some(5));
    }

    #[test]
    fn compute_fit_size_is_none_without_a_target_tab() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        assert_eq!(state.compute_fit_size(5, &tabs), None);
    }

    #[test]
    fn compute_fit_size_is_none_without_a_fit() {
        let state = MobileState::default();
        let tabs = no_tabs();
        assert_eq!(state.compute_fit_size(5, &tabs), None);
    }

    #[test]
    fn clear_fit_owned_by_removes_and_returns_the_tab() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );

        assert_eq!(state.clear_fit_owned_by(CLIENT_A, &mut tabs), Some(5));
        assert!(!state.has_fit(5));
    }

    #[test]
    fn clear_fit_owned_by_is_none_when_client_owns_nothing() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        assert_eq!(state.clear_fit_owned_by(CLIENT_B, &mut tabs), None);
        assert!(state.has_fit(5));
    }

    #[test]
    fn clear_fit_for_pane_matches_by_fullscreened_pane() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );

        assert_eq!(state.clear_fit_for_pane(PaneId::Terminal(11)), Some(5));
        assert!(!state.has_fit(5));
    }

    #[test]
    fn clear_fit_for_pane_is_none_when_no_fit_targets_it() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        assert_eq!(state.clear_fit_for_pane(PaneId::Terminal(99)), None);
        assert!(state.has_fit(5));
    }

    #[test]
    fn remove_fit_for_tab_drops_the_override() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            5,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );

        state.remove_fit_for_tab(5);
        assert!(!state.has_fit(5));
    }

    #[test]
    fn clear_fits_owned_by_removes_only_the_clients_fits() {
        let mut state = MobileState::default();
        let mut tabs = no_tabs();
        state.set_fit(
            CLIENT_A,
            1,
            PaneId::Terminal(11),
            embedded_size(),
            &mut tabs,
        );
        state.set_fit(
            CLIENT_A,
            2,
            PaneId::Terminal(12),
            embedded_size(),
            &mut tabs,
        );
        state.set_fit(
            CLIENT_B,
            3,
            PaneId::Terminal(13),
            embedded_size(),
            &mut tabs,
        );

        let mut cleared = state.clear_fits_owned_by(CLIENT_A, &mut tabs);
        cleared.sort_unstable();

        assert_eq!(cleared, vec![1, 2]);
        assert!(!state.has_fit(1));
        assert!(!state.has_fit(2));
        assert!(state.has_fit(3));
    }
}
