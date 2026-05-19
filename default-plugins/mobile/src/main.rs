//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` /
//! `Key` events for selection and action dispatch.

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
        // Flush any pending fit-size update accumulated by the last
        // render. The shim call MUST happen here (in update) and not
        // in render — `host_run_plugin_command` drains the plugin's
        // stdout pipe via `read_to_end`, so calling it mid-render
        // would consume the already-written `print!` output and the
        // host would receive an empty frame. By the time this fires
        // there is always a fresh event in flight (TabUpdate /
        // PaneUpdate from the same `RecomputeTabSize` handler that
        // resized the mobile tab), so the deferral adds at most one
        // event-loop tick before the server sees the new target.
        flush_pending_fit_size(self);

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
                // If the pane that was being "fit" disappeared (closed
                // by the user from another client, layout change),
                // the server's fit entry is now stuck on a dead pane
                // id. Clear locally + tell the server to revert.
                if let Some(selected) = self.selected_pane_id {
                    if !live_pane_ids.contains(&selected) {
                        clear_fit_if_active(self);
                    }
                }
                // Re-evaluate the tab default in case TabUpdate arrived
                // before any PaneUpdate — `tab_is_self_only` depends on
                // pane data and may have classified everything as
                // visible during the first tick.
                if let Some(pos) = self.selected_tab_position {
                    let still_visible =
                        self.tabs_in_order().iter().any(|t| t.position == pos);
                    if !still_visible {
                        // Selected tab vanished — fit was bound to
                        // its tab_id, so the server's entry is now
                        // useless. Tell it to clear before we lose
                        // the tab reference.
                        clear_fit_if_active(self);
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
                match mouse {
                    // Swipe up on the viewport pans the rendered slice
                    // toward older content (away from the bottom-anchored
                    // default). Capped by `render_embedded_viewport` on
                    // the next frame, so the pan offset cannot exceed
                    // what the current cached viewport supports.
                    Mouse::ScrollUp(lines) => {
                        if pan_is_allowed(self) {
                            self.viewport_v_pan = self.viewport_v_pan.saturating_add(lines);
                            return true;
                        }
                        return false;
                    },
                    Mouse::ScrollDown(lines) => {
                        if pan_is_allowed(self) {
                            self.viewport_v_pan =
                                self.viewport_v_pan.saturating_sub(lines);
                            return true;
                        }
                        return false;
                    },
                    // Convention (see mobile_panning.md): ScrollRight
                    // increases `viewport_h_pan` to reveal more of the
                    // right edge — mirrors swipe-up = ScrollUp =
                    // reveal more recent content. Render-side clamps
                    // against the pane's actual width on the next
                    // frame, so we don't need to know `pane_width`
                    // here.
                    Mouse::ScrollRight(cols) => {
                        if pan_is_allowed(self) {
                            self.viewport_h_pan =
                                self.viewport_h_pan.saturating_add(cols);
                            return true;
                        }
                        return false;
                    },
                    Mouse::ScrollLeft(cols) => {
                        if pan_is_allowed(self) {
                            self.viewport_h_pan =
                                self.viewport_h_pan.saturating_sub(cols);
                            return true;
                        }
                        return false;
                    },
                    _ => {},
                }
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

/// True when a scroll event should drive the embedded-viewport pan
/// offsets rather than be dropped.
///
/// The check intentionally omits any "did the gesture land inside the
/// viewport region" predicate — `Mouse::ScrollUp/Down` carry no
/// position today (see `Mouse::position` in `zellij-utils/src/data.rs`),
/// and Stage 4 of the panning plan extends the variants with coords
/// so this gate can grow a region check then. Until then the only
/// scrollable surface in the plugin is the embedded viewport, so any
/// scroll while a viewport is showing is unambiguous.
fn pan_is_allowed(state: &State) -> bool {
    // No panning while a selector is open: the menu replaces the
    // viewport, so the gesture target the user expects to scroll is
    // the menu itself, not the hidden viewport behind it. (The menu
    // is not scrollable today; the event is simply dropped.)
    if state.expanded.is_some() {
        return false;
    }
    // Need a selected pane with cached content — otherwise the pan
    // offset has nothing to act on and the renderer would clamp it
    // back to 0 on the next frame anyway.
    if state.current_pane().is_none() {
        return false;
    }
    state.current_pane_viewport_len() > 0
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
            // Tab-switch invalidates any active fit (the fit is
            // pinned to a specific (tab_id, pane_id) server-side).
            clear_fit_if_active(state);
            state.selected_tab_position = Some(position);
            state.selected_pane_id = None;
            // A new tab lands at its bottom-right corner: any
            // accumulated pan offset belongs to the previous pane and
            // makes no sense in this context.
            state.viewport_v_pan = 0;
            state.viewport_h_pan = 0;
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
            // Pane-switch invalidates any active fit — fit is bound
            // to the specific pane that was focused when toggled on.
            clear_fit_if_active(state);
            state.selected_tab_position = Some(tab_position);
            state.selected_pane_id = Some(pane_id);
            // Reset pan so the user lands at the new pane's bottom.
            state.viewport_v_pan = 0;
            state.viewport_h_pan = 0;
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
        ClickAction::ToggleFit => {
            // Round-trip the toggle through the server. On entry we
            // need (a) the focused pane, (b) its tab, (c) the most
            // recent embedded viewport region (to compute the target
            // tab size including chrome compensation). If any of
            // those are missing we silently bail rather than send a
            // malformed command.
            if state.fit_active {
                state.fit_active = false;
                state.fit_last_sent_size = None;
                state.fit_pending_target = None;
                exit_fit_mode();
                true
            } else {
                let Some(pane) = state.current_pane() else {
                    return false;
                };
                let Some(tab) = state.current_tab().cloned() else {
                    return false;
                };
                let Some(region) = state.viewport_region else {
                    return false;
                };
                let target = fit_target_tab_size(&pane, &tab, &region);
                state.fit_active = true;
                // Seed both fields so the first render+update after
                // entering fit doesn't immediately re-send the same
                // size: render's `fit_pending_target` will equal the
                // value we just sent, and the diff in
                // `flush_pending_fit_size` short-circuits.
                state.fit_last_sent_size = Some((target.0, target.1));
                state.fit_pending_target = Some((target.0, target.1));
                enter_fit_mode(
                    tab.tab_id,
                    state::pane_id_of(&pane),
                    target.0,
                    target.1,
                );
                true
            }
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

/// If fit is locally active, clear the mirror state and tell the
/// server to revert the override + any fit-induced fullscreen. Used
/// at every plugin-driven focus change (tab/pane switch, focused
/// pane disappearing) so the server's `FitState` doesn't outlive
/// the pane it was bound to.
pub fn clear_fit_if_active(state: &mut State) {
    if state.fit_active {
        state.fit_active = false;
        state.fit_last_sent_size = None;
        state.fit_pending_target = None;
        exit_fit_mode();
    }
}

/// If render() stashed a new fit target on `fit_pending_target` and
/// it differs from what we last sent, forward it to the server. The
/// diff is what prevents a render-resize loop: the server's resize
/// triggers a fresh PaneRenderReportWithAnsi → render → which
/// recomputes the *same* target → no new send. See the doc on
/// `State::fit_pending_target` for why this can't live inside
/// render itself.
pub fn flush_pending_fit_size(state: &mut State) {
    if !state.fit_active {
        return;
    }
    let Some(target) = state.fit_pending_target else {
        return;
    };
    if state.fit_last_sent_size == Some(target) {
        return;
    }
    state.fit_last_sent_size = Some(target);
    update_fit_size(target.0, target.1);
}

/// Compute the tab size the server should set so that the focused
/// pane's *content area* fills this plugin's embedded viewport area
/// exactly.
///
/// `viewport_rows`/`viewport_columns` on `TabInfo` are the bounds of
/// the selectable-pane area (status bar / tab bar already subtracted
/// from `display_area_*`). `pane_rows`/`pane_content_rows` on
/// `PaneInfo` reflect the current frame draw — non-zero deltas
/// indicate pane chrome (borders / pane frames).
///
/// Heuristic: scale up the embedded dimensions by both deltas so the
/// resulting tab, after chrome and frame are re-applied server-side,
/// leaves a pane-content rectangle that matches the embedded area.
/// Layouts with asymmetric chrome (no bottom bar; floating panes
/// dominant) may leave one row of slack — still strictly better than
/// the panning fallback.
pub fn fit_target_tab_size(
    pane: &PaneInfo,
    tab: &TabInfo,
    region: &state::ViewportRegion,
) -> (usize, usize) {
    let embedded_rows = region.row_end.saturating_sub(region.row_start);
    let embedded_cols = region.cols;
    let chrome_rows = tab
        .display_area_rows
        .saturating_sub(tab.viewport_rows);
    let chrome_cols = tab
        .display_area_columns
        .saturating_sub(tab.viewport_columns);
    let frame_rows = pane.pane_rows.saturating_sub(pane.pane_content_rows);
    let frame_cols = pane
        .pane_columns
        .saturating_sub(pane.pane_content_columns);
    (
        embedded_rows + chrome_rows + frame_rows,
        embedded_cols + chrome_cols + frame_cols,
    )
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
