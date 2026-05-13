//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` /
//! `Key` events for selection and action dispatch. Stage 6 ships the
//! collapsing-breadcrumb v1 layout; typing-mode and viewport mouse
//! passthrough land in Stage 7.

mod keyboard;
mod keys;
mod render;
mod state;

use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::*;

use crate::keyboard::TapOutcome;
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
            // Press-flash sweep: every tap on the in-plugin keyboard
            // schedules a Timer at `KEY_FEEDBACK_MS`, and the resulting
            // `Event::Timer` clears the expired entry so the cell
            // returns to its resting colour.
            EventType::Timer,
        ]);

        // The keyboard is visible from the first frame
        // (`KeyboardController::new` sets `visible = true`), so
        // suppress the OS soft keyboard up front to avoid the two
        // stacking on first focus.
        set_soft_keyboard(false);
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
                self.pane_last_activity
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
            Event::SessionUpdate(sessions, _) => {
                // Capture this client's session name for the top bar
                // *and* the full session list for the session
                // selector. A fresh `SessionUpdate` arrives every time
                // session metadata changes.
                if let Some(current) =
                    sessions.iter().find(|s| s.is_current_session)
                {
                    self.session_name = Some(current.name.clone());
                }
                self.sessions = sessions;
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
                //
                // Receipt of a delta for a pane *is* the activity
                // signal — `PaneContents` itself carries no
                // server-side timestamp, so we stamp `now()` for
                // every pane mentioned in this report. The Panes
                // selector renders that stamp as `<time> ago`.
                let now = unix_now();
                for id in map.keys() {
                    self.pane_last_activity.insert(*id, now);
                }
                self.latest_pane_contents.extend(map);
                // While the Panes selector is open, the same delta
                // that signals "this pane's content changed" is the
                // best moment to also refresh its title — OSC 2
                // sequences land in the same byte stream that drives
                // these reports. We do this here (in `update`) and
                // not in `render` because shim calls write to the
                // plugin's stdout for their response, and `render`'s
                // own output capture would be corrupted by an
                // interleaved shim reply.
                if matches!(self.expanded, Some(Selector::Panes)) {
                    refresh_pane_titles(self);
                }
                true
            },
            Event::Mouse(mouse) => {
                if let Some((line, col)) = mouse.position() {
                    if let Mouse::LeftClick(_, _) = mouse {
                        // Top-bar / selector regions always win —
                        // they're the plugin's chrome and need to
                        // remain interactive even though the user can
                        // also tap into the embedded pane below.
                        if let Some(action) = self.click_to_action(line, col) {
                            return dispatch_click(self, action);
                        }
                        // No chrome region matched. Synthesize an SGR
                        // mouse press+release at the equivalent cell
                        // of the underlying pane so taps inside the
                        // embedded viewport reach the program below.
                        if let Some((pane_row, pane_col)) =
                            self.click_in_viewport(line, col)
                        {
                            if let Some(pane) = self.current_pane() {
                                let pane_id = state::pane_id_of(&pane);
                                let bytes = sgr_left_click(pane_row, pane_col);
                                write_to_pane_id(bytes, pane_id);
                                // No re-render: the pane will emit a
                                // fresh PaneRenderReportWithAnsi and
                                // the regular event path will refresh
                                // the cache.
                                return false;
                            }
                        }
                    }
                }
                false
            },
            Event::Key(key) => {
                // Esc always closes an open selector first — the menu
                // has hidden the embedded pane, so Esc-to-pane while a
                // menu is up would never reach the user's eye anyway,
                // and using Esc as the universal back affordance is
                // the convention soft-keyboard users expect.
                if key.bare_key == BareKey::Esc && self.expanded.is_some() {
                    self.expanded = None;
                    return true;
                }
                // Forward to the selected pane's pty. Sticky modifiers
                // (set elsewhere — eventually by the plugin keyboard's
                // ⌃/⌥ cells) are folded in and then cleared so a user
                // can produce Ctrl+C by arming ⌃ via the keyboard and
                // then typing `c` on a hardware keyboard.
                if let Some(pane) = self.current_pane() {
                    let key = if self.ctrl_held || self.alt_held {
                        merge_held_modifiers(&key, self.ctrl_held, self.alt_held)
                    } else {
                        key.clone()
                    };
                    let bytes = keys::serialize_key(&key);
                    if !bytes.is_empty() {
                        write_to_pane_id(bytes, state::pane_id_of(&pane));
                    }
                }
                // Render if the modifier state changed so the indicator
                // for whichever cell renders ⌃/⌥ returns to its resting
                // colour.
                let consumed = self.ctrl_held || self.alt_held;
                self.ctrl_held = false;
                self.alt_held = false;
                consumed
            },
            Event::Timer(_) => {
                // The only timer the plugin schedules drives keyboard
                // press-flash decay. `sweep_flash` returns true iff at
                // least one entry expired — which is the signal to
                // redraw so the cell returns to its resting colour.
                self.keyboard.sweep_flash(Instant::now())
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

/// Return a clone of `key` with `Ctrl` / `Alt` added to its modifier
/// set when the corresponding sticky flag is on. Used by the
/// `Event::Key` handler so a hardware-keyboard tap that follows a
/// `⌃` / `⌥` tap from the plugin keyboard produces a properly-
/// modified key.
fn merge_held_modifiers(key: &KeyWithModifier, ctrl: bool, alt: bool) -> KeyWithModifier {
    let mut merged = key.clone();
    if ctrl {
        merged.key_modifiers.insert(KeyModifier::Ctrl);
    }
    if alt {
        merged.key_modifiers.insert(KeyModifier::Alt);
    }
    merged
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
        ClickAction::ExpandSessions => {
            state.expanded = Some(Selector::Sessions);
            true
        },
        ClickAction::ExpandTabs => {
            state.expanded = Some(Selector::Tabs);
            true
        },
        ClickAction::ExpandPanes => {
            // Refresh titles once on open so the menu doesn't show
            // the stale `Pane #N` placeholder when the shell has
            // already emitted OSC 2 before this click. Subsequent
            // refreshes happen in the `PaneRenderReportWithAnsi`
            // event handler whenever the menu stays open.
            refresh_pane_titles(state);
            state.expanded = Some(Selector::Panes);
            true
        },
        ClickAction::CollapseSelector => {
            state.expanded = None;
            true
        },
        ClickAction::SelectSession(name) => {
            // Hand off to the host. This actually changes the
            // client's session — the mobile plugin in the destination
            // session (if any) will take over from here.
            switch_session(Some(&name));
            state.expanded = None;
            true
        },
        ClickAction::SelectTab(position) => {
            // The mobile plugin never moves the *client's* focused
            // tab — doing so would yank the client out of the mobile
            // tab (where this plugin lives) and into the destination
            // tab, making the entire mobile UI vanish. The "selected
            // tab" here is a purely internal concept: it controls
            // which tab's panes the embedded viewport reads.
            // Resetting `selected_pane_id` lets the renderer fall
            // back to the first pane in the newly-selected tab.
            state.selected_tab_position = Some(position);
            state.selected_pane_id = None;
            state.expanded = None;
            true
        },
        ClickAction::SelectPane { tab_position, pane_id } => {
            // Same rationale as SelectTab: do not call
            // `switch_tab_to` or `focus_*_pane` here — those would
            // change the client's actual focus and dismount the
            // mobile UI. The plugin embeds the chosen pane via its
            // own renderer (reading `PaneRenderReportWithAnsi` from
            // the host) and forwards keystrokes/clicks via
            // `write_to_pane_id`; neither needs the host's focus.
            state.selected_tab_position = Some(tab_position);
            state.selected_pane_id = Some(pane_id);
            state.expanded = None;
            true
        },
        ClickAction::ToggleKeyboard => {
            // Flip the plugin keyboard's visibility and mirror the
            // change to the OS soft keyboard so the two never stack.
            // When the plugin keyboard is showing the OS keyboard is
            // suppressed; when it's hidden the OS keyboard is re-
            // enabled so users without a hardware keyboard still have
            // an input affordance.
            state.keyboard.visible = !state.keyboard.visible;
            set_soft_keyboard(!state.keyboard.visible);
            true
        },
        ClickAction::Keyboard(cell) => {
            let outcome = state.keyboard.handle_tap(
                cell,
                &mut state.ctrl_held,
                &mut state.alt_held,
            );
            match outcome {
                TapOutcome::SendBytes(bytes) => {
                    if let Some(pane) = state.current_pane() {
                        if !bytes.is_empty() {
                            write_to_pane_id(bytes, state::pane_id_of(&pane));
                        }
                    }
                },
                TapOutcome::HideKeyboard => {
                    set_soft_keyboard(!state.keyboard.visible);
                },
                TapOutcome::Toggled | TapOutcome::NoOp => {},
            }
            // Schedule the press-flash decay sweep. `KEY_FEEDBACK_MS`
            // is in milliseconds; `set_timeout` takes seconds.
            set_timeout(keyboard::KEY_FEEDBACK_MS as f64 / 1000.0);
            true
        },
    }
}

/// Wall-clock seconds since the unix epoch, as returned by the
/// wasi-clocks shim. Used to stamp `pane_last_activity` on every
/// `PaneRenderReportWithAnsi` and to compute the `<time> ago` deltas
/// rendered in the Panes selector.
pub fn unix_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Replace each cached pane's `title` with the latest value from the
/// host. Called from `render_panes_menu` on every render of the
/// Panes selector so the menu always reflects the shell's current
/// title rather than the stale `Pane #N` placeholder.
///
/// The staleness happens because `Event::PaneUpdate` is only
/// dispatched on structural changes (new pane, focus change, layout
/// resize); shell-emitted OSC 2 title sequences update the host's
/// `Grid::title` without firing one. `get_pane_info` runs a fresh
/// `pane_info_for_pane` on the server, which calls
/// `pane.current_title()` and so reflects the most-recent OSC 2.
///
/// Cost: one synchronous shim call per cached pane per render of
/// the Panes selector. The selector is only on-screen transiently
/// (the user opens it, picks a pane, it closes), so the volume is
/// bounded by an interactive flow rather than by Zellij's render
/// rate.
pub fn refresh_pane_titles(state: &mut State) {
    let pane_ids: Vec<PaneId> = state
        .panes_by_tab_position
        .values()
        .flat_map(|panes| panes.iter().map(state::pane_id_of))
        .collect();
    for id in pane_ids {
        let Some(fresh) = get_pane_info(id) else { continue };
        for panes in state.panes_by_tab_position.values_mut() {
            for p in panes.iter_mut() {
                if state::pane_id_of(p) == id {
                    p.title = fresh.title.clone();
                }
            }
        }
    }
}
