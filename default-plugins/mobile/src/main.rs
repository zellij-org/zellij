mod ansi;
mod click;
mod components;
mod fit;
mod frame;
mod input;
mod keys;
mod mouse;
mod navigation;
mod pane_sync;
mod render;
mod screens;
mod state;
mod workspace;

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::components::modifier_bar::{CellId, TapOutcome};
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
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
            EventType::SoftKeyboardVisibilityChanged,
            EventType::Timer,
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
                if self.workspace.selected_tab_position.is_none() {
                    self.workspace.selected_tab_position =
                        self.workspace.tabs_in_order().first().map(|t| t.position);
                }
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
                if let Some(current) = sessions.iter().find(|s| s.is_current_session) {
                    self.workspace.session_name = Some(current.name.clone());
                }
                let filtered = filter_sessions_for_client(sessions, self);
                self.sessions.sessions = filtered;
                true
            },
            Event::Timer(_) => {
                self.empty_welcome_list_grace_elapsed = true;
                true
            },
            Event::PaneRenderReportWithAnsi(map) => {
                let now = unix_now();
                for id in map.keys() {
                    self.workspace.pane_last_activity.insert(*id, now);
                }
                self.workspace.latest_pane_contents.extend(map);
                if self.active == ActiveScreen::Panes {
                    refresh_pane_titles(self);
                }
                true
            },
            Event::Mouse(mouse) => {
                match mouse {
                    Mouse::ScrollUp(lines) => {
                        let up = true;
                        return mouse::scroll_or_pan(self, lines, up);
                    },
                    Mouse::ScrollDown(lines) => {
                        let up = false;
                        return mouse::scroll_or_pan(self, lines, up);
                    },
                    Mouse::ScrollRight(cols) => {
                        let right = true;
                        return mouse::pan_horizontally(self, cols, right);
                    },
                    Mouse::ScrollLeft(cols) => {
                        let right = false;
                        return mouse::pan_horizontally(self, cols, right);
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
                true
            },
            _ => false,
        };
        if self.fit.active {
            let suppress_top_bar = self.sessions.is_welcome_screen
                || self.active == ActiveScreen::Sessions;
            self.fit
                .notify_size(&self.workspace, &self.frame, suppress_top_bar);
        }
        should_render && self.is_ready_to_render()
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if rows == 0 || cols == 0 || !self.is_ready_to_render() {
            return;
        }
        render::render(self, rows, cols);
    }
}

impl State {
    fn is_ready_to_render(&self) -> bool {
        if self.workspace.tabs.is_empty() && self.workspace.panes_by_tab_position.is_empty() {
            return false;
        }
        if self.showing_empty_welcome_list_within_grace() {
            return false;
        }
        if self.active == ActiveScreen::Viewport {
            return self.viewport_pane_has_content();
        }
        true
    }

    fn showing_empty_welcome_list_within_grace(&self) -> bool {
        self.sessions.is_welcome_screen
            && self.sessions.sessions.is_empty()
            && !self.empty_welcome_list_grace_elapsed
    }

    fn viewport_pane_has_content(&self) -> bool {
        let Some(pane) = self.workspace.current_pane() else {
            return false;
        };
        self.workspace
            .latest_pane_contents
            .get(&pane_id_of(&pane))
            .map(|contents| !contents.viewport.is_empty())
            .unwrap_or(false)
    }

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

    pub fn open_panes(&mut self) -> bool {
        self.menu.open = false;
        self.navigation.selector_scroll_offset = 0;
        self.panes.panes_search.clear();
        refresh_pane_titles(self);
        self.active = ActiveScreen::Panes;
        true
    }

    pub fn collapse_selector(&mut self) -> bool {
        self.active = ActiveScreen::Viewport;
        self.sessions.welcome_search.clear();
        self.panes.panes_search.clear();
        self.navigation.selector_scroll_offset = 0;
        true
    }

    pub fn select_pane(&mut self, tab_position: usize, pane_id: PaneId) -> bool {
        self.fit.clear_if_active();
        self.workspace.selected_tab_position = Some(tab_position);
        self.workspace.selected_pane_id = Some(pane_id);
        self.viewport.reset_pan();
        self.active = ActiveScreen::Viewport;
        sync_shadow_focus(self);
        true
    }

    pub fn toggle_fit(&mut self) -> bool {
        let suppress_top_bar =
            self.sessions.is_welcome_screen || self.active == ActiveScreen::Sessions;
        self.fit.toggle(&self.workspace, &self.frame, suppress_top_bar)
    }

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

    pub fn new_tab(&mut self) -> bool {
        self.fit.clear_if_active();
        if let Some(tab_position) = new_tab_unfocused::<&str>(None, None) {
            self.workspace.pending_new_tab_position = Some(tab_position);
        }
        true
    }

    pub fn open_new_session_prompt(&mut self) -> bool {
        self.new_session.open(&mut self.active)
    }

    pub fn cancel_new_session_prompt(&mut self) -> bool {
        self.new_session.cancel(&mut self.active)
    }

    pub fn accept_new_session_prompt(&mut self) -> bool {
        self.new_session.accept(&mut self.active)
    }

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

fn is_welcome_session(session: &SessionInfo) -> bool {
    session
        .panes
        .panes
        .values()
        .flatten()
        .any(|p| p.is_plugin && p.plugin_url.as_deref() == Some("welcome-screen"))
}

pub fn sync_shadow_focus(state: &State) {
    if let Some(pane) = state.workspace.current_pane() {
        set_shadow_focus(pane_id_of(&pane));
    }
}

pub fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

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
    use zellij_tile::prelude::{PaneInfo, SessionInfo, TabInfo};

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

    #[test]
    fn dispatch_toggle_fit_on_path_seeds_fields() {
        let mut state = fit_ready_state();
        assert!(!state.fit.active, "Pre-condition: fit is off");
        let consumed = click::dispatch(&mut state, ClickAction::ToggleFit);
        assert!(consumed);
        assert!(state.fit.active);
        assert_eq!(state.fit.tab_id, Some(7));
    }

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

    fn welcome_state(session_names: &[&str]) -> State {
        let mut state = fit_ready_state();
        state.active = ActiveScreen::Sessions;
        state.sessions.is_welcome_screen = true;
        state.sessions.sessions = session_names
            .iter()
            .map(|name| SessionInfo {
                name: name.to_string(),
                ..SessionInfo::default()
            })
            .collect();
        state
    }

    #[test]
    fn welcome_empty_list_is_not_ready_within_grace() {
        let mut state = welcome_state(&[]);
        state.empty_welcome_list_grace_elapsed = false;
        assert!(!state.is_ready_to_render());
    }

    #[test]
    fn welcome_empty_list_is_ready_after_grace() {
        let mut state = welcome_state(&[]);
        state.empty_welcome_list_grace_elapsed = true;
        assert!(state.is_ready_to_render());
    }

    #[test]
    fn welcome_non_empty_list_is_ready_within_grace() {
        let mut state = welcome_state(&["alpha"]);
        state.empty_welcome_list_grace_elapsed = false;
        assert!(state.is_ready_to_render());
    }

    #[test]
    fn timer_event_elapses_welcome_grace() {
        let mut state = welcome_state(&[]);
        assert!(!state.empty_welcome_list_grace_elapsed);
        state.update(Event::Timer(0.0));
        assert!(state.empty_welcome_list_grace_elapsed);
        assert!(state.is_ready_to_render());
    }

    #[test]
    fn viewport_without_a_pane_is_not_ready() {
        let mut state = State::default();
        state.workspace.tabs.push(TabInfo {
            position: 0,
            ..TabInfo::default()
        });
        state.workspace.selected_tab_position = Some(0);
        state.active = ActiveScreen::Viewport;
        assert!(state.workspace.current_pane().is_none());
        assert!(!state.is_ready_to_render());
    }
}
