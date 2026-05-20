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
                    //
                    // Once the pan saturates at `max_v_pan`, any
                    // remaining gesture lines spill into the underlying
                    // pane's scrollback via `scroll_up_in_pane_id` —
                    // see `apply_v_pan` for the partition math. The
                    // host returns a fresh `PaneRenderReportWithAnsi`
                    // after each scroll call, so we return `true` only
                    // when the local pan actually moved (otherwise the
                    // pane render event will refresh us).
                    Mouse::ScrollUp(lines) => {
                        return handle_scroll_pan(self, lines, /*up=*/true);
                    },
                    Mouse::ScrollDown(lines) => {
                        return handle_scroll_pan(self, lines, /*up=*/false);
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

/// Compute the new vertical pan offset for a slide gesture and report
/// how many of the gesture's lines did not fit (i.e. would push the
/// pan past the edge). The overflow count is what the mouse handler
/// converts into `scroll_*_in_pane_id` shim calls so a saturating
/// gesture continues into the underlying pane's scrollback instead of
/// dying at the edge.
///
/// Direction encoding matches the `Mouse::Scroll*` variants:
/// - `up = true` corresponds to `Mouse::ScrollUp` — pan increases
///   toward `max_pan` (older content). Overflow > 0 when the gesture
///   would have pushed past `max_pan`.
/// - `up = false` corresponds to `Mouse::ScrollDown` — pan decreases
///   toward 0 (newer content). Overflow > 0 when the gesture would
///   have pushed below 0.
///
/// Pure function; no I/O. Exists as a free fn so the handler's
/// branchy event-tick code stays straight-line and the partition math
/// is unit-testable on its own.
fn apply_v_pan(
    old_pan: usize,
    max_pan: usize,
    lines: usize,
    up: bool,
) -> (usize, usize) {
    if up {
        let desired = old_pan.saturating_add(lines);
        let new_pan = desired.min(max_pan);
        let absorbed = new_pan - old_pan;
        (new_pan, lines - absorbed)
    } else {
        let new_pan = old_pan.saturating_sub(lines);
        let absorbed = old_pan - new_pan;
        (new_pan, lines - absorbed)
    }
}

/// Apply a vertical slide gesture to the embedded viewport:
/// 1. Drop the gesture entirely if `pan_is_allowed` is false (no
///    selected pane, empty cache, or a selector menu is on top of the
///    viewport).
/// 2. On the very first event tick — before any frame has been laid
///    out — `viewport_region` is `None` and `max_viewport_v_pan`
///    returns `None`. With no embed height in hand the handler cannot
///    compute overflow, so we fall back to today's pure-pan behaviour
///    and let the next render clamp the offset.
/// 3. Otherwise partition the gesture's `lines` into "absorbed by the
///    pan" plus "overflow", and forward every overflow line to the
///    selected pane as a single-line scrollback step.
///
/// Returns the value the `update()` event handler should propagate
/// back to the host: `true` iff the local pan moved (a re-render is
/// useful immediately). Pure-overflow events return `false` because
/// the scroll itself produces a `PaneRenderReportWithAnsi` from the
/// host that drives the next frame — same pattern as the SGR click
/// passthrough at the bottom of the `Event::Mouse` arm.
fn handle_scroll_pan(state: &mut State, lines: usize, up: bool) -> bool {
    let dir = if up { "Up" } else { "Down" };
    eprintln!(
        "[mobile/scroll] enter dir={dir} lines={lines} v_pan={} h_pan={} \
         viewport_len={} viewport_region={:?} expanded={:?} \
         current_pane_some={}",
        state.viewport_v_pan,
        state.viewport_h_pan,
        state.current_pane_viewport_len(),
        state.viewport_region,
        state.expanded,
        state.current_pane().is_some(),
    );
    if !pan_is_allowed(state) {
        eprintln!(
            "[mobile/scroll] dropped: pan_is_allowed=false (see prior log for reason) dir={dir} lines={lines}"
        );
        return false;
    }
    let Some(max_v_pan) = state.max_viewport_v_pan() else {
        // First event tick: no frame has rendered yet, so we don't
        // know the embed height. Preserve today's pure-pan behaviour;
        // the renderer will clamp on the first frame.
        eprintln!(
            "[mobile/scroll] fallback pure-pan (max_v_pan=None, no viewport_region yet) dir={dir} lines={lines} old_pan={}",
            state.viewport_v_pan
        );
        if up {
            state.viewport_v_pan = state.viewport_v_pan.saturating_add(lines);
        } else {
            state.viewport_v_pan = state.viewport_v_pan.saturating_sub(lines);
        }
        eprintln!(
            "[mobile/scroll] fallback pure-pan new_pan={}",
            state.viewport_v_pan
        );
        return true;
    };
    let old_pan = state.viewport_v_pan;
    let (new_pan, overflow) = apply_v_pan(old_pan, max_v_pan, lines, up);
    let pan_moved = new_pan != old_pan;
    state.viewport_v_pan = new_pan;
    eprintln!(
        "[mobile/scroll] partition dir={dir} lines={lines} old_pan={old_pan} \
         max_v_pan={max_v_pan} new_pan={new_pan} overflow={overflow} pan_moved={pan_moved}"
    );
    if overflow > 0 {
        match state.current_pane() {
            Some(pane) => {
                let pane_id = state::pane_id_of(&pane);
                eprintln!(
                    "[mobile/scroll] forwarding {overflow} scroll_{} call(s) to pane_id={pane_id:?}",
                    if up { "up" } else { "down" }
                );
                for i in 0..overflow {
                    if up {
                        scroll_up_in_pane_id(pane_id);
                    } else {
                        scroll_down_in_pane_id(pane_id);
                    }
                    eprintln!(
                        "[mobile/scroll]   fired scroll_{} #{}/{overflow}",
                        if up { "up" } else { "down" },
                        i + 1
                    );
                }
            },
            None => {
                eprintln!(
                    "[mobile/scroll] WARN overflow={overflow} but current_pane()=None — scroll dropped"
                );
            },
        }
    }
    eprintln!("[mobile/scroll] return pan_moved={pan_moved}");
    pan_moved
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
        eprintln!(
            "[mobile/scroll] pan_is_allowed=false: selector open ({:?})",
            state.expanded
        );
        return false;
    }
    // Need a selected pane with cached content — otherwise the pan
    // offset has nothing to act on and the renderer would clamp it
    // back to 0 on the next frame anyway.
    if state.current_pane().is_none() {
        eprintln!("[mobile/scroll] pan_is_allowed=false: current_pane()=None");
        return false;
    }
    let len = state.current_pane_viewport_len();
    if len == 0 {
        eprintln!(
            "[mobile/scroll] pan_is_allowed=false: current_pane_viewport_len()=0"
        );
        return false;
    }
    true
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
                state.fit_tab_id = None;
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
                // Seed all four fields so the first render+update
                // after entering fit doesn't immediately re-send the
                // same size: render's `fit_pending_target` will equal
                // the value we just sent, and the diff in
                // `flush_pending_fit_size` short-circuits. `fit_tab_id`
                // is what subsequent `update_fit_size` calls use to
                // address the server-side entry by tab.
                state.fit_last_sent_size = Some((target.0, target.1));
                state.fit_pending_target = Some((target.0, target.1));
                state.fit_tab_id = Some(tab.tab_id);
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
        state.fit_tab_id = None;
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
    let Some(tab_id) = state.fit_tab_id else {
        // `fit_active` without a `fit_tab_id` is a programming error
        // — every ON path seeds both. Bail rather than send a
        // malformed UpdateFitSize against tab_id 0.
        return;
    };
    if state.fit_last_sent_size == Some(target) {
        return;
    }
    state.fit_last_sent_size = Some(target);
    update_fit_size(tab_id, target.0, target.1);
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

#[cfg(test)]
mod tests {
    //! Unit tests for the pure-math helper `fit_target_tab_size` and
    //! the gating logic of `flush_pending_fit_size`. Shim calls inside
    //! these functions resolve to the native-build stub of
    //! `host_run_plugin_command` (see `zellij-tile/src/shim.rs`), so
    //! the tests observe state mutation only; the shim's effect on
    //! the (non-existent) host is irrelevant.
    use super::*;
    use crate::state::{State, ViewportRegion};
    use zellij_tile::prelude::{PaneInfo, TabInfo};

    fn region(row_start: usize, row_end: usize, cols: usize) -> ViewportRegion {
        ViewportRegion {
            row_start,
            row_end,
            cols,
            skip: 0,
            h_offset: 0,
        }
    }

    /// Embedded viewport of (11 rows, 80 cols) inside a tab with two
    /// rows of chrome (tab + status bars) and a pane with a 1-cell
    /// border on every side. Target = embedded + chrome + frame =
    /// (15, 82).
    #[test]
    fn adds_chrome_and_frame_to_embedded() {
        let mut tab = TabInfo::default();
        tab.display_area_rows = 24;
        tab.display_area_columns = 80;
        tab.viewport_rows = 22;
        tab.viewport_columns = 80;
        let mut pane = PaneInfo::default();
        pane.pane_rows = 22;
        pane.pane_columns = 80;
        pane.pane_content_rows = 20;
        pane.pane_content_columns = 78;
        let r = region(0, 11, 80);
        let target = fit_target_tab_size(&pane, &tab, &r);
        assert_eq!(target, (15, 82));
    }

    /// Borderless pane in a tab with no surrounding chrome — the
    /// target equals the embedded area exactly.
    #[test]
    fn zero_chrome_zero_frame_passes_through() {
        let mut tab = TabInfo::default();
        tab.display_area_rows = 20;
        tab.display_area_columns = 60;
        tab.viewport_rows = 20;
        tab.viewport_columns = 60;
        let mut pane = PaneInfo::default();
        pane.pane_rows = 20;
        pane.pane_columns = 60;
        pane.pane_content_rows = 20;
        pane.pane_content_columns = 60;
        let r = region(0, 10, 40);
        let target = fit_target_tab_size(&pane, &tab, &r);
        assert_eq!(target, (10, 40));
    }

    /// Pathological inputs where `viewport_*` exceeds `display_area_*`
    /// must not panic. `saturating_sub` collapses chrome to 0 and the
    /// result remains sensible.
    #[test]
    fn saturating_subtraction_on_inverted_inputs() {
        let mut tab = TabInfo::default();
        tab.display_area_rows = 10;
        tab.display_area_columns = 40;
        tab.viewport_rows = 20;
        tab.viewport_columns = 80;
        let mut pane = PaneInfo::default();
        pane.pane_rows = 5;
        pane.pane_columns = 30;
        pane.pane_content_rows = 10;
        pane.pane_content_columns = 50;
        let r = region(0, 8, 32);
        let target = fit_target_tab_size(&pane, &tab, &r);
        assert_eq!(target, (8, 32));
    }

    /// Inactive fit must not mutate `fit_last_sent_size` even if a
    /// pending target is set. Observed via the side-effect proxy
    /// because the shim's host stub on native builds is a no-op.
    #[test]
    fn flush_skips_when_inactive() {
        let mut state = State::default();
        state.fit_active = false;
        state.fit_pending_target = Some((10, 40));
        state.fit_last_sent_size = None;
        state.fit_tab_id = Some(7);
        flush_pending_fit_size(&mut state);
        assert_eq!(state.fit_last_sent_size, None);
    }

    /// When the pending target equals what was already sent, the diff
    /// short-circuits and `fit_last_sent_size` stays untouched.
    #[test]
    fn flush_skips_when_already_sent() {
        let mut state = State::default();
        state.fit_active = true;
        state.fit_pending_target = Some((10, 40));
        state.fit_last_sent_size = Some((10, 40));
        state.fit_tab_id = Some(7);
        flush_pending_fit_size(&mut state);
        assert_eq!(state.fit_last_sent_size, Some((10, 40)));
    }

    /// A pending target that differs from the last sent size updates
    /// `fit_last_sent_size` to the new value (the shim itself fires
    /// but resolves to a host-stub no-op on native builds).
    #[test]
    fn flush_updates_on_pending_change() {
        let mut state = State::default();
        state.fit_active = true;
        state.fit_pending_target = Some((9, 36));
        state.fit_last_sent_size = Some((10, 40));
        state.fit_tab_id = Some(7);
        flush_pending_fit_size(&mut state);
        assert_eq!(state.fit_last_sent_size, Some((9, 36)));
    }

    /// `flush_pending_fit_size` must bail when `fit_tab_id` is unset:
    /// without it the server can't address the override entry and we'd
    /// send an UpdateFitSize against tab_id 0. Belt-and-braces guard
    /// against a future ON path that forgets to seed `fit_tab_id`.
    #[test]
    fn flush_skips_when_tab_id_unset() {
        let mut state = State::default();
        state.fit_active = true;
        state.fit_pending_target = Some((10, 40));
        state.fit_last_sent_size = None;
        state.fit_tab_id = None;
        flush_pending_fit_size(&mut state);
        assert_eq!(state.fit_last_sent_size, None);
    }

    /// Static canary: `render.rs` must not invoke any host shim.
    ///
    /// Every shim in `zellij-tile` is backed by
    /// `host_run_plugin_command`, which drains the plugin's stdout via
    /// `read_to_end`. If a shim is called mid-`render`, every byte
    /// already written to stdout is consumed by the host as the
    /// (malformed) protobuf reply payload and the rendered frame the
    /// user actually sees is empty. The fix is to defer the shim call
    /// to `update()` (see `State::fit_pending_target` and
    /// `flush_pending_fit_size`). This test is the canary that would
    /// have caught the original pinch-zoom regression and prevents the
    /// same shape of bug from recurring on a different shim.
    ///
    /// Comment-only lines are skipped so the existing doc reference to
    /// `update_fit_size` in `render.rs` remains legal.
    ///
    /// Located here (rather than under `tests/`) because the `mobile`
    /// crate is a wasm-only bin: a regular integration test would
    /// require linking the bin against the host shims natively, which
    /// is impossible on the test host. The static check only needs
    /// `include_str!`, so a `mod tests` `#[test]` works just as well.
    #[test]
    fn no_shim_calls_from_render() {
        const RENDER_SRC: &str = include_str!("render.rs");
        // The documented four (per fit_tests.md) plus
        // `host_run_plugin_command` to catch a manually-coded shim.
        // `show_cursor` is deliberately omitted: render calls it via
        // `emit_cursor` with an idempotence guard (see
        // `LastEmittedCursor`). The render-loop feedback the guard
        // prevents is a separate hazard from the
        // stdout-drain-during-render hazard this test guards against.
        const FORBIDDEN_SHIMS: &[&str] = &[
            "update_fit_size",
            "enter_fit_mode",
            "exit_fit_mode",
            "set_soft_keyboard",
            "switch_session",
            "write_to_pane_id",
            "set_timeout",
            "get_pane_info",
            "host_run_plugin_command",
        ];
        let mut offences: Vec<String> = Vec::new();
        for (idx, line) in RENDER_SRC.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            for name in FORBIDDEN_SHIMS {
                let needle = format!("{name}(");
                if line.contains(&needle) {
                    offences.push(format!("line {}: `{}`", idx + 1, line.trim()));
                }
            }
        }
        assert!(
            offences.is_empty(),
            "render.rs must not invoke host shims (they drain plugin \
             stdout mid-frame). Offending occurrences:\n  {}",
            offences.join("\n  ")
        );
    }

    /// Build a `State` seeded with one tab + one pane + a viewport
    /// region — the minimum surface required for the `ToggleFit`
    /// dispatch path to reach `fit_target_tab_size`. Returns the
    /// `State` ready to receive `dispatch_click(&mut state, ...)`.
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
        state.tabs.push(tab);
        state.selected_tab_position = Some(0);
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
        state.panes_by_tab_position.insert(0, vec![pane]);
        state.selected_pane_id = Some(PaneId::Terminal(3));
        state.viewport_region = Some(crate::state::ViewportRegion {
            row_start: 0,
            row_end: 11,
            cols: 80,
            skip: 0,
            h_offset: 0,
        });
        state
    }

    /// `dispatch_click(ToggleFit)` from the OFF state seeds all four
    /// fit fields. The shim itself fires (its native stub no-ops) —
    /// the test asserts on the visible state mutation that gates the
    /// later `flush_pending_fit_size`. Target value matches what
    /// `fit_target_tab_size` would compute for the seeded surface:
    /// embedded (11, 80) + chrome (2, 0) + frame (2, 2) = (15, 82).
    /// `fit_tab_id` is what subsequent `update_fit_size` calls use to
    /// address the server-side entry.
    #[test]
    fn dispatch_toggle_fit_on_path_seeds_fields() {
        let mut state = fit_ready_state();
        assert!(!state.fit_active, "Pre-condition: fit is off");

        let consumed = dispatch_click(&mut state, ClickAction::ToggleFit);

        assert!(consumed);
        assert!(state.fit_active);
        assert_eq!(state.fit_last_sent_size, Some((15, 82)));
        assert_eq!(state.fit_pending_target, Some((15, 82)));
        assert_eq!(
            state.fit_tab_id,
            Some(7),
            "tab_id from the seeded TabInfo flows into fit_tab_id"
        );
    }

    /// `dispatch_click(ToggleFit)` from the ON state clears every fit
    /// field. The state is fully reset regardless of which subset was
    /// set when the toggle was tripped — guards the symmetric path
    /// against future drift between the ON and OFF branches.
    #[test]
    fn dispatch_toggle_fit_off_path_clears_fields() {
        let mut state = fit_ready_state();
        state.fit_active = true;
        state.fit_last_sent_size = Some((15, 82));
        state.fit_pending_target = Some((15, 82));
        state.fit_tab_id = Some(7);

        let consumed = dispatch_click(&mut state, ClickAction::ToggleFit);

        assert!(consumed);
        assert!(!state.fit_active);
        assert_eq!(state.fit_last_sent_size, None);
        assert_eq!(state.fit_pending_target, None);
        assert_eq!(state.fit_tab_id, None);
    }

    /// `PaneUpdate` whose manifest no longer contains the
    /// selected pane clears the local fit mirror. This is the auto-
    /// recovery path when the fit pane is closed externally — without
    /// it, the plugin would keep showing "fit armed" against a dead
    /// pane id.
    #[test]
    fn pane_update_clears_fit_when_selected_pane_disappears() {
        let mut state = fit_ready_state();
        state.fit_active = true;
        state.fit_last_sent_size = Some((15, 82));
        state.fit_pending_target = Some((15, 82));
        state.fit_tab_id = Some(7);

        // Manifest with the same tab but pane 3 (the selected one)
        // removed — only pane 99 survives. `clear_fit_if_active`
        // should fire from the `PaneUpdate` handler.
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

        assert!(!state.fit_active, "Local fit mirror cleared");
        assert_eq!(state.fit_last_sent_size, None);
        assert_eq!(state.fit_pending_target, None);
        assert_eq!(state.fit_tab_id, None);
    }

    /// Gesture lies entirely below the edge — every line lands in the
    /// pan offset and no overflow is reported. The baseline case the
    /// pre-existing renderer already handled correctly; documented
    /// here so the helper's "absorbed = lines, overflow = 0" path is
    /// pinned.
    #[test]
    fn apply_v_pan_up_fully_absorbed() {
        assert_eq!(apply_v_pan(0, 100, 3, true), (3, 0));
        assert_eq!(apply_v_pan(50, 100, 3, true), (53, 0));
    }

    /// Gesture starts inside the legal range but its last lines would
    /// step past `max_pan`. The pan saturates at `max_pan` and the
    /// remaining lines are reported as overflow — this is the central
    /// new behaviour: pan-then-scroll inside a single event.
    #[test]
    fn apply_v_pan_up_partial_overflow() {
        assert_eq!(apply_v_pan(99, 100, 3, true), (100, 2));
        assert_eq!(apply_v_pan(98, 100, 5, true), (100, 3));
    }

    /// Already at the top edge — pan cannot move and every gesture
    /// line is overflow. Confirms the all-or-nothing degenerate case.
    #[test]
    fn apply_v_pan_up_fully_overflowed() {
        assert_eq!(apply_v_pan(100, 100, 3, true), (100, 3));
    }

    /// `max_pan == 0` (embed area covers the entire cached viewport):
    /// no pan is ever legal, so every line of every gesture is
    /// overflow.
    #[test]
    fn apply_v_pan_up_zero_max() {
        assert_eq!(apply_v_pan(0, 0, 3, true), (0, 3));
        assert_eq!(apply_v_pan(0, 0, 0, true), (0, 0));
    }

    /// Down direction mirrors the up case: pan decreases toward 0,
    /// and lines that would have pushed below 0 are reported as
    /// overflow (to be forwarded as `scroll_down_in_pane_id` calls).
    #[test]
    fn apply_v_pan_down_partial_overflow() {
        assert_eq!(apply_v_pan(2, 100, 3, false), (0, 1));
        assert_eq!(apply_v_pan(5, 100, 3, false), (2, 0));
    }

    /// Already at the bottom edge — pan saturates at 0 and the
    /// gesture's lines all become overflow.
    #[test]
    fn apply_v_pan_down_fully_overflowed() {
        assert_eq!(apply_v_pan(0, 100, 3, false), (0, 3));
    }

    /// Zero-line gestures (theoretical; the wire protocol never
    /// sends 0) must be no-ops in both directions — important so a
    /// future caller that accidentally passes 0 cannot trigger
    /// spurious shim calls.
    #[test]
    fn apply_v_pan_zero_lines() {
        assert_eq!(apply_v_pan(5, 100, 0, true), (5, 0));
        assert_eq!(apply_v_pan(5, 100, 0, false), (5, 0));
    }
}
