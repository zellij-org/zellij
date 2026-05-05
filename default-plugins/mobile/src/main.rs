//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` /
//! `Key` events for selection and action dispatch. Stage 6 ships the
//! collapsing-breadcrumb v1 layout; typing-mode and viewport mouse
//! passthrough land in Stage 7.

mod render;
mod state;

use std::collections::BTreeMap;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

use crate::state::{ClickAction, Selector, State};

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        // Cache the plugin's own pane id so we can filter ourselves
        // out of the tab/pane lists. Without this, the mobile tab
        // (which contains only this plugin) becomes the
        // selected-tab/pane and the embedded viewport feedback-loops
        // the plugin's own chrome.
        let ids = get_plugin_ids();
        self.own_plugin_pane_id = Some(PaneId::Plugin(ids.plugin_id));

        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Key,
            EventType::Mouse,
            EventType::PaneRenderReportWithAnsi,
            EventType::SessionUpdate,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::ModeUpdate(mode_info) => {
                self.mode_info = Some(mode_info);
                true
            },
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
                // Default selection: the first non-mobile tab visible
                // to this client. We deliberately do NOT follow the
                // active tab here, because right after EnterMobileMode
                // the active tab IS the mobile tab — selecting it
                // would cause the plugin to embed its own viewport.
                if self.selected_tab_position.is_none() {
                    if let Some(first) = self.tabs_in_order().first() {
                        self.selected_tab_position = Some(first.position);
                    }
                }
                // If the previously-selected tab vanished or became
                // self-only, fall back to the first non-mobile tab.
                if let Some(pos) = self.selected_tab_position {
                    let still_visible =
                        self.tabs_in_order().iter().any(|t| t.position == pos);
                    if !still_visible {
                        self.selected_tab_position = self
                            .tabs_in_order()
                            .first()
                            .map(|t| t.position);
                    }
                }
                true
            },
            Event::PaneUpdate(manifest) => {
                self.panes_by_tab_position = manifest.panes;
                // Re-evaluate the tab default in case TabUpdate arrived
                // before any PaneUpdate — `tab_is_self_only` depends on
                // pane data and may have classified everything as
                // visible during the first tick.
                if let Some(pos) = self.selected_tab_position {
                    let still_visible =
                        self.tabs_in_order().iter().any(|t| t.position == pos);
                    if !still_visible {
                        self.selected_tab_position =
                            self.tabs_in_order().first().map(|t| t.position);
                        self.selected_pane_id = None;
                    }
                } else {
                    self.selected_tab_position =
                        self.tabs_in_order().first().map(|t| t.position);
                }

                // Default pane selection: the focused pane in the
                // selected tab, but never the mobile plugin itself.
                if self.selected_pane_id.is_none() {
                    if let Some(pane) = self
                        .current_tab_panes()
                        .into_iter()
                        .find(|p| p.is_focused)
                        .or_else(|| self.current_tab_panes().into_iter().next())
                    {
                        self.selected_pane_id = Some(state::pane_id_of(pane));
                    }
                }
                true
            },
            Event::PaneRenderReportWithAnsi(map) => {
                // Replace wholesale — the server emits the full set
                // each cycle, so any keys absent from the new map
                // correspond to closed panes.
                self.latest_pane_contents = map;
                true
            },
            Event::Mouse(mouse) => {
                if let Some((line, col)) = mouse.position() {
                    if let Mouse::LeftClick(_, _) = mouse {
                        if let Some(action) = self.click_to_action(line, col) {
                            return dispatch_click(self, action);
                        }
                    }
                }
                false
            },
            Event::Key(_) => {
                // Typing-mode and key passthrough land in Stage 7. For
                // now the plugin swallows keys quietly so they don't
                // bleed into the host UI.
                false
            },
            _ => false,
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if rows == 0 || cols == 0 {
            return;
        }
        if self.tabs.is_empty() && self.panes_by_tab_position.is_empty() {
            render::render_stub(rows, cols);
            return;
        }
        render::render(self, rows, cols);
    }
}

/// Translate a click region's `ClickAction` into the corresponding
/// shim/action call. Returns whether the plugin should re-render
/// immediately (the plugin re-renders on every `update` that returns
/// `true`).
fn dispatch_click(state: &mut State, action: ClickAction) -> bool {
    match action {
        ClickAction::ExpandTabs => {
            state.expanded = Some(Selector::Tabs);
            true
        },
        ClickAction::ExpandPanes => {
            state.expanded = Some(Selector::Panes);
            true
        },
        ClickAction::Collapse => {
            state.expanded = None;
            true
        },
        ClickAction::SelectTab(position) => {
            state.selected_tab_position = Some(position);
            // Clear the pane selection so the next render snaps to the
            // newly-selected tab's focused pane.
            state.selected_pane_id = None;
            state.expanded = None;
            true
        },
        ClickAction::SelectPane(id) => {
            state.selected_pane_id = Some(id);
            state.expanded = None;
            true
        },
        ClickAction::ToggleType => {
            state.typing_mode = !state.typing_mode;
            true
        },
        ClickAction::NewPane => {
            run_action(
                Action::NewPane {
                    direction: None,
                    pane_name: None,
                    start_suppressed: false,
                },
                BTreeMap::new(),
            );
            true
        },
        ClickAction::NewTab => {
            run_action(
                Action::NewTab {
                    tiled_layout: None,
                    floating_layouts: vec![],
                    swap_tiled_layouts: None,
                    swap_floating_layouts: None,
                    tab_name: None,
                    should_change_focus_to_new_tab: true,
                    cwd: None,
                    initial_panes: None,
                    first_pane_unblock_condition: None,
                },
                BTreeMap::new(),
            );
            true
        },
        ClickAction::SplitRight => {
            run_action(
                Action::NewPane {
                    direction: Some(Direction::Right),
                    pane_name: None,
                    start_suppressed: false,
                },
                BTreeMap::new(),
            );
            true
        },
        ClickAction::SplitDown => {
            run_action(
                Action::NewPane {
                    direction: Some(Direction::Down),
                    pane_name: None,
                    start_suppressed: false,
                },
                BTreeMap::new(),
            );
            true
        },
        ClickAction::ToggleFloating => {
            run_action(Action::ToggleFloatingPanes, BTreeMap::new());
            true
        },
        ClickAction::CloseFocus => {
            run_action(Action::CloseFocus, BTreeMap::new());
            true
        },
        ClickAction::Detach => {
            detach();
            true
        },
        ClickAction::ExitMobile => {
            run_action(Action::ToggleMobileMode, BTreeMap::new());
            true
        },
    }
}
