//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` /
//! `Key` events for selection and action dispatch. Stage 6 ships the
//! collapsing-breadcrumb v1 layout; typing-mode and viewport mouse
//! passthrough land in Stage 7.

mod keys;
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
                // Drop cached viewports for panes that no longer exist
                // in the manifest. `PaneRenderReportWithAnsi` carries
                // changed panes only (see `get_changed_panes_per_client`
                // in `wasm_bridge.rs`), so without this prune the cache
                // would grow unbounded as panes close.
                let live_pane_ids: std::collections::HashSet<PaneId> = self
                    .panes_by_tab_position
                    .values()
                    .flat_map(|panes| panes.iter().map(state::pane_id_of))
                    .collect();
                self.latest_pane_contents
                    .retain(|id, _| live_pane_ids.contains(id));
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

                // Default pane selection: the first pane in the
                // selected tab. We deliberately do NOT prefer the
                // `is_focused` pane — `PaneInfo.is_focused` is a global
                // flag (true if any client focuses the pane), so
                // initialising from it would make the mobile view start
                // out tracking another connected client's focused pane.
                // The user can pick a different pane via the panes
                // selector; once they do, `selected_pane_id` is sticky.
                if self.selected_pane_id.is_none() {
                    if let Some(pane) = self.current_tab_panes().into_iter().next() {
                        self.selected_pane_id = Some(state::pane_id_of(pane));
                    }
                }
                true
            },
            Event::PaneRenderReportWithAnsi(map) => {
                // Merge — the server emits *changed* panes only after
                // the first report (see `get_changed_panes_per_client`
                // in `zellij-server/src/plugins/wasm_bridge.rs`). A
                // wholesale replace would wipe every static pane's
                // viewport whenever any other pane changes (e.g. when
                // a desktop client opens a new pane), leaving the
                // mobile embedded viewport empty. Pane closures are
                // handled in the `PaneUpdate` arm above, which prunes
                // entries against the authoritative pane manifest.
                self.latest_pane_contents.extend(map);
                true
            },
            Event::Mouse(mouse) => {
                if let Some((line, col)) = mouse.position() {
                    if let Mouse::LeftClick(_, _) = mouse {
                        // Action-bar / breadcrumb / selector regions
                        // always win — they're the plugin's chrome and
                        // need to remain interactive even when the user
                        // is also typing into the embedded pane.
                        if let Some(action) = self.click_to_action(line, col) {
                            return dispatch_click(self, action);
                        }
                        // No chrome region matched. If the click landed
                        // in the embedded viewport AND typing-mode is
                        // off, synthesize an SGR mouse press+release at
                        // the equivalent cell of the underlying pane.
                        // While typing-mode is armed we keep the soft
                        // keyboard's view stable and ignore stray taps.
                        if !self.typing_mode {
                            if let Some((pane_row, pane_col)) =
                                self.click_in_viewport(line, col)
                            {
                                if let Some(pane) = self.current_pane() {
                                    let pane_id = state::pane_id_of(&pane);
                                    let bytes = sgr_left_click(pane_row, pane_col);
                                    write_to_pane_id(bytes, pane_id);
                                    // No re-render: the pane will emit
                                    // a fresh PaneRenderReportWithAnsi
                                    // and the regular event path will
                                    // refresh the cache.
                                    return false;
                                }
                            }
                        }
                    }
                }
                false
            },
            Event::Key(key) => {
                // Typing-mode forwards every key to the selected pane's
                // pty; otherwise the plugin swallows keys (chrome
                // navigation today is mouse-only — keyboard nav can
                // land later).
                if self.typing_mode {
                    if let Some(pane) = self.current_pane() {
                        let bytes = keys::serialize_key(&key);
                        if !bytes.is_empty() {
                            write_to_pane_id(bytes, state::pane_id_of(&pane));
                        }
                    }
                }
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
            render::render_stub(self, rows, cols);
            return;
        }
        render::render(self, rows, cols);
    }
}

/// Build an SGR mouse left-click press+release sequence targeting the
/// (0-based) `pane_row`/`pane_col` of the underlying pane's viewport.
/// SGR mouse coordinates are 1-based. Emits press then release in a
/// single byte stream so the receiving program sees a complete click.
fn sgr_left_click(pane_row: usize, pane_col: usize) -> Vec<u8> {
    let col = pane_col + 1;
    let row = pane_row + 1;
    format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", col, row, col, row).into_bytes()
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
