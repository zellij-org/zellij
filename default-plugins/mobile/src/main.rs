//! Mobile UI plugin (`zellij:mobile`).
//!
//! Hosted in a per-client tab with `visible_to = Some({client_id})`,
//! this plugin owns the entire mobile interface. It subscribes to
//! `PaneRenderReportWithAnsi` to embed live pane viewports, and to the
//! standard `TabUpdate` / `PaneUpdate` / `ModeUpdate` / `Mouse` /
//! `Key` events for selection and action dispatch.

mod modifier_bar;
mod keys;
mod render;
mod state;

use std::collections::BTreeMap;
use std::time::Instant;
use zellij_tile::prelude::*;

use crate::modifier_bar::TapOutcome;
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
            // Drives `soft_keyboard_visible`, which gates the modifier
            // bar so the bar appears and disappears in lockstep with
            // the browser's OS keyboard. Fired by the client whenever
            // `window.visualViewport.height` crosses the keyboard
            // show/hide threshold.
            EventType::SoftKeyboardVisibilityChanged,
        ]);

        set_soft_keyboard(true);
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
                // Push the shadow focus to the server now in case
                // PaneUpdate has not yet fired (event ordering varies on
                // first plugin load) — `sync_shadow_focus` is a no-op
                // when `current_pane()` is None, so it is safe to call
                // even before pane data is available.
                sync_shadow_focus(self);
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

                // Resolve a pending "+ New Tab" auto-select. The
                // shim returned a tab position synchronously, but the
                // matching PaneUpdate (this event) is the first
                // moment we have a concrete pane id for the new tab.
                // Pick that tab's first pane and set both selection
                // fields, then clear the pending intent.
                if let Some(target) = self.pending_new_tab_position {
                    let first_pane = self
                        .panes_for_tab(target)
                        .into_iter()
                        .next()
                        .map(state::pane_id_of);
                    if let Some(id) = first_pane {
                        self.selected_tab_position = Some(target);
                        self.selected_pane_id = Some(id);
                        self.viewport_v_pan = 0;
                        self.viewport_h_pan = 0;
                        self.expanded = None;
                        self.pending_new_tab_position = None;
                    }
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
                // Push the resolved current pane to the server as the
                // mobile client's shadow focus so other clients see
                // the focus marker on the pane the viewport is
                // rendering. Covers initial setup and any pane churn
                // (close/move) that triggers a re-pick above.
                sync_shadow_focus(self);
                // Welcome-screen UX: on first detection that the
                // underlying pane is the session-manager welcome
                // plugin, close that pane and take over the welcome
                // experience natively. The session-manager renders
                // at the underlying pane's width (typically full
                // screen) and would otherwise require horizontal
                // panning to read; running the welcome flow in this
                // plugin's own UI fits the phone width naturally and
                // lets the Sessions selector scroll under sticky
                // chrome. The welcome tab auto-closes after its only
                // pane is gone (no selectable panes → screen render
                // loop marks the tab for closure); this plugin's own
                // tab keeps the session alive.
                if !self.welcome_auto_expand_done
                    && self.expanded.is_none()
                    && self.current_pane_is_welcome()
                {
                    if let Some(pane) = self.current_pane() {
                        close_plugin_pane(pane.id);
                    }
                    self.expanded = Some(Selector::Sessions);
                    self.selector_scroll_offset = 0;
                    self.menu_open = false;
                    self.welcome_auto_expand_done = true;
                    // Pull the authoritative session snapshot, same
                    // path as `ClickAction::ExpandSessions`. The
                    // standing `Event::SessionUpdate` payload only
                    // contains the current session's metadata until
                    // a scan is requested via this shim — so without
                    // this call the selector would render empty on
                    // first show.
                    if let Ok(snapshot) = get_session_list() {
                        self.sessions = filter_sessions_for_client(
                            snapshot.live_sessions, self,
                        );
                    }
                }
                true
            },
            Event::SessionUpdate(sessions, _) => {
                // Capture this client's session name for the top bar
                // *and* the full session list for the session
                // selector. A fresh `SessionUpdate` arrives every time
                // session metadata changes — including the broadcast
                // that follows our own `get_session_list()` call in
                // `ClickAction::ExpandSessions`, so the filter applied
                // there is replicated here to keep the list stable
                // across both write paths.
                if let Some(current) =
                    sessions.iter().find(|s| s.is_current_session)
                {
                    self.session_name = Some(current.name.clone());
                }
                self.sessions = filter_sessions_for_client(sessions, self);
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
                        // When a selector menu is open, scroll the
                        // selector's row list instead of panning the
                        // (hidden) embedded viewport. The renderer
                        // clamps a stale offset against the current
                        // item count on the next frame, so we can
                        // saturate-subtract toward zero here without
                        // querying item lengths.
                        if self.expanded.is_some() {
                            return handle_selector_scroll(self, lines, /*up=*/true);
                        }
                        return handle_scroll_pan(self, lines, /*up=*/true);
                    },
                    Mouse::ScrollDown(lines) => {
                        if self.expanded.is_some() {
                            return handle_selector_scroll(self, lines, /*up=*/false);
                        }
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
                        // No chrome region matched. If the hamburger
                        // dropdown is open, the click landed outside
                        // any menu item — dismiss the menu without
                        // forwarding the click to the underlying pane.
                        // Pane passthrough resumes on the next click
                        // once the menu is closed.
                        if self.menu_open {
                            self.menu_open = false;
                            return true;
                        }
                        // No chrome region matched. Forward the
                        // click to the embedded pane so the program
                        // below receives the tap.
                        //
                        // Terminal panes: synthesize an SGR mouse
                        // press+release at the equivalent cell — the
                        // termwiz input parser used by the host
                        // converts these bytes into terminal mouse
                        // events that the underlying program reads
                        // from its pty.
                        //
                        // Plugin panes: SGR sequences are useless —
                        // the host's `parse_keys` (called via
                        // `write_to_pane_id` → `AdjustedInput::Write
                        // BytesToTerminal` → `parse_keys`) filters
                        // for `InputEvent::Key` only and drops
                        // `InputEvent::Mouse`, so the embedded
                        // plugin never sees the click. Pipe a
                        // structured "mobile_viewport_click" message
                        // instead, addressed to the destination
                        // plugin id. Plugins that opt in
                        // (session-manager's mobile welcome path
                        // does) can dispatch the tap by row;
                        // plugins that don't care silently ignore
                        // the message.
                        if let Some((pane_row, pane_col)) =
                            self.click_in_viewport(line, col)
                        {
                            if let Some(pane) = self.current_pane() {
                                let pane_id = state::pane_id_of(&pane);
                                if pane.is_plugin {
                                    let mut args = BTreeMap::new();
                                    args.insert("row".to_string(), pane_row.to_string());
                                    args.insert("col".to_string(), pane_col.to_string());
                                    let message =
                                        MessageToPlugin::new("mobile_viewport_click")
                                            .with_destination_plugin_id(pane.id)
                                            .with_args(args);
                                    pipe_message_to_plugin(message);
                                } else {
                                    let bytes = sgr_left_click(pane_row, pane_col);
                                    write_to_pane_id(bytes, pane_id);
                                }
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
                // While the "+ New Session" prompt is open, the plugin
                // captures keys for its own text-entry buffer instead
                // of forwarding them to the embedded pane. Every key
                // is consumed here (returning true) so a typo never
                // leaks through to the pane below the prompt. Sticky
                // ctrl/alt state is left untouched: the prompt does
                // not interpret modifiers, and clearing them here
                // would surprise the user after dismissing the prompt.
                if self.expanded == Some(Selector::NewSessionPrompt) {
                    match key.bare_key {
                        BareKey::Esc => {
                            // Return to the Sessions selector (the
                            // welcome screen in welcome mode, the
                            // regular session list otherwise). Mirrors
                            // the [Cancel] button dispatch.
                            self.pending_session_name.clear();
                            self.new_session_view_offset = 0;
                            self.new_session_content_w = 0;
                            self.expanded = Some(Selector::Sessions);
                        },
                        BareKey::Enter => {
                            // Move (not clone) the buffer into the
                            // shim argument. `None` triggers the
                            // host's auto-name path (see
                            // `switch_session` in zellij-tile's shim);
                            // a non-empty buffer asks for a specific
                            // name. Either way the host will switch
                            // the client into the new session, after
                            // which this plugin dismounts.
                            let name = std::mem::take(&mut self.pending_session_name);
                            let arg = if name.is_empty() { None } else { Some(name.as_str()) };
                            switch_session(arg);
                            self.new_session_view_offset = 0;
                            self.new_session_content_w = 0;
                            self.expanded = None;
                        },
                        BareKey::Backspace => {
                            self.pending_session_name.pop();
                        },
                        BareKey::Char(c) => {
                            self.pending_session_name.push(c);
                        },
                        // Every other key (arrows, function keys,
                        // Tab, …) is swallowed silently — the prompt
                        // is intentionally minimal and forwarding
                        // these to the pane would defeat the capture.
                        _ => {},
                    }
                    return true;
                }
                // Sessions selector: capture keys for the "Session:"
                // fuzzy-search prompt. Mirrors the session-manager
                // welcome screen's input model — Char/Backspace edit
                // the buffer, Enter attaches to the highest-scored
                // match, Esc clears the buffer first and only closes
                // the selector once the buffer is already empty.
                //
                // Active for both the welcome flow and the in-mobile
                // "Change Session" view (the renderer paints the same
                // welcome-style layout in both — see
                // `render_welcome_sessions`). In the welcome flow the
                // "close" branch is suppressed because there is no
                // embedded pane to return to (the welcome session is
                // the one this plugin is hosting); in the non-welcome
                // path empty-buffer Esc mirrors the "[← BACK]" tap.
                if self.expanded == Some(Selector::Sessions) {
                    match key.bare_key {
                        BareKey::Esc => {
                            if !self.welcome_search.is_empty() {
                                self.welcome_search.clear();
                                self.selector_scroll_offset = 0;
                            } else if !self.welcome_auto_expand_done {
                                self.expanded = None;
                            }
                        },
                        BareKey::Enter => {
                            if let Some(name) = self.welcome_top_match_name() {
                                switch_session(Some(&name));
                                self.expanded = None;
                            }
                        },
                        BareKey::Backspace => {
                            self.welcome_search.pop();
                            // Reset scroll so a freshly-shrunken list
                            // re-anchors at the top instead of opening
                            // mid-page.
                            self.selector_scroll_offset = 0;
                        },
                        BareKey::Char(c) => {
                            self.welcome_search.push(c);
                            self.selector_scroll_offset = 0;
                        },
                        _ => {},
                    }
                    return true;
                }
                // Panes selector: same input model as the Sessions
                // path above, applied to `panes_search`. Enter selects
                // the highest-scoring pane (or the first pane in
                // display order when the buffer is empty). Esc with a
                // non-empty buffer clears it; an empty Esc closes the
                // selector — mirroring the "[← BACK]" tap.
                if self.expanded == Some(Selector::Panes) {
                    match key.bare_key {
                        BareKey::Esc => {
                            if !self.panes_search.is_empty() {
                                self.panes_search.clear();
                                self.selector_scroll_offset = 0;
                            } else {
                                self.expanded = None;
                            }
                        },
                        BareKey::Enter => {
                            // Dispatch through `SelectPane` so the
                            // Enter path picks up every side effect a
                            // click on the card would (fit clear,
                            // viewport pan reset, shadow-focus sync,
                            // ...). Going through the ClickAction
                            // keeps the keyboard and pointer paths
                            // sharing one source of truth.
                            if let Some((tab_position, pane_id)) =
                                self.panes_top_match()
                            {
                                self.panes_search.clear();
                                self.selector_scroll_offset = 0;
                                dispatch_click(
                                    self,
                                    ClickAction::SelectPane {
                                        tab_position,
                                        pane_id,
                                    },
                                );
                            }
                        },
                        BareKey::Backspace => {
                            self.panes_search.pop();
                            self.selector_scroll_offset = 0;
                        },
                        BareKey::Char(c) => {
                            self.panes_search.push(c);
                            self.selector_scroll_offset = 0;
                        },
                        _ => {},
                    }
                    return true;
                }
                // Esc always returns to the main view: closes any
                // open selector and dismisses the dropdown menu in a
                // single press. The selector/menu have hidden (or
                // overlaid) the embedded pane, so Esc-to-pane while
                // either is up would never reach the user's eye
                // anyway; using Esc as the universal back affordance
                // is the convention soft-keyboard users expect.
                if key.bare_key == BareKey::Esc
                    && (self.expanded.is_some() || self.menu_open)
                {
                    self.expanded = None;
                    self.menu_open = false;
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
                self.modifier_bar.sweep_flash(Instant::now())
            },
            Event::SoftKeyboardVisibilityChanged(visible) => {
                if self.soft_keyboard_visible == visible {
                    return false;
                }
                self.soft_keyboard_visible = visible;
                // The soft-keyboard bar is part of the plugin's chrome,
                // so toggling it changes the embedded-viewport area an
                // active fit must track. Report the new chrome insets to
                // the server inline (insets depend only on this semantic
                // state, available right here in `update()`).
                notify_fit_chrome(self);
                // Modifier bar visibility is gated on this flag in
                // `render::render`; a redraw is required to add/remove
                // the bottom row.
                true
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

/// Scroll the currently-open selector's row list. `up = true`
/// mirrors `Mouse::ScrollUp` — saturating-decrement toward zero (the
/// top of the list, matching the viewport convention where ScrollUp
/// reveals earlier content). `up = false` increments past the end;
/// the renderer clamps against the actual item count on the next
/// frame so the offset never sticks past the last visible row.
/// Returns `true` whenever the offset moved so the host re-renders.
///
/// In welcome mode the per-event delta is capped at
/// `max(1, last_welcome_visible_count - 1)` so the last visible card
/// before the scroll stays in the new window — at least one card of
/// overlap is always preserved. A fast swipe (large `lines`) is
/// flattened to that cap, which prevents the list from "page-flipping"
/// past the user's reading position.
fn handle_selector_scroll(state: &mut State, lines: usize, up: bool) -> bool {
    let effective_lines = if state.welcome_auto_expand_done
        && state.expanded == Some(Selector::Sessions)
        && state.last_welcome_visible_count > 0
    {
        // visible - 1 keeps one card of overlap. Floor at 1 so a
        // 1-card window can still scroll (no overlap possible there
        // — the cap simply has no effect).
        let cap = state.last_welcome_visible_count.saturating_sub(1).max(1);
        lines.min(cap)
    } else {
        lines
    };
    let old = state.selector_scroll_offset;
    state.selector_scroll_offset = if up {
        old.saturating_sub(effective_lines)
    } else {
        old.saturating_add(effective_lines)
    };
    state.selector_scroll_offset != old
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
            // Selectors and the hamburger menu are mutually exclusive
            // — opening a selector (whether from the menu or from a
            // direct top-bar tap) clears the menu state. Harmless when
            // the menu was already closed. Reset scroll so each entry
            // into the selector starts anchored at the top regardless
            // of where the previous session in this selector landed,
            // and clear the fuzzy-search buffer so a freshly-opened
            // selector starts on an empty prompt (the in-mobile
            // "Change Session" view shares the welcome-style layout
            // and matcher state — see `render_welcome_sessions`).
            state.menu_open = false;
            state.selector_scroll_offset = 0;
            state.welcome_search.clear();
            // Kick the server's peer-session scan and adopt the
            // resulting snapshot directly. Without this the plugin
            // only ever sees the *current* session — the standing
            // `Event::SessionUpdate` payload contains nothing but
            // `peer_sessions_cache`, which on the server is only
            // populated after a plugin explicitly triggers a scan via
            // this shim (the session-manager plugin does the same on
            // every refresh: see
            // `default-plugins/session-manager/src/main.rs:1204`).
            // The shim returns the snapshot synchronously so we can
            // populate `state.sessions` in the same tick rather than
            // waiting for the broadcast `SessionUpdate` event that
            // the shim also triggers.
            if let Ok(snapshot) = get_session_list() {
                state.sessions =
                    filter_sessions_for_client(snapshot.live_sessions, state);
            }
            state.expanded = Some(Selector::Sessions);
            true
        },
        ClickAction::ExpandPanes => {
            state.menu_open = false;
            state.selector_scroll_offset = 0;
            // Clear the fuzzy-search buffer so a freshly-opened
            // Switch Pane view starts on an empty "Pane:" prompt —
            // mirrors `ExpandSessions` clearing `welcome_search`.
            state.panes_search.clear();
            // Refresh titles once on open so the menu doesn't show
            // the stale `Pane #N` placeholder when the shell has
            // already emitted OSC 2 before this click. Subsequent
            // refreshes happen in the `PaneRenderReportWithAnsi`
            // event handler whenever the menu stays open.
            refresh_pane_titles(state);
            state.expanded = Some(Selector::Panes);
            true
        },
        ClickAction::ToggleMenu => {
            // The hamburger toggles the dropdown menu. Since the top
            // bar is identical in every screen the hamburger is
            // tappable from selectors too — opening the menu while a
            // selector is active closes the selector first so the
            // menu (which is gated on `expanded.is_none()`) actually
            // renders. From the user's perspective a tap on ☰ always
            // takes them to the menu over the viewport.
            if state.menu_open {
                state.menu_open = false;
            } else {
                state.expanded = None;
                state.menu_open = true;
            }
            true
        },
        ClickAction::CollapseSelector => {
            // Clear both selectors' fuzzy-search buffers and reset
            // scroll alongside the close so a future reopen never
            // inherits stale prompt state. Each buffer is owned by
            // its own selector — `welcome_search` by Sessions,
            // `panes_search` by Panes — and clearing the inactive
            // one is a no-op.
            state.expanded = None;
            state.welcome_search.clear();
            state.panes_search.clear();
            state.selector_scroll_offset = 0;
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
        ClickAction::OpenNewSessionPrompt => {
            // Swap the sessions selector for the in-plugin name-entry
            // overlay. The buffer is cleared on entry so a previously
            // cancelled attempt never leaks back into a fresh prompt.
            // Reset the view/box anchors too so the prompt starts at
            // the default size rather than inheriting an old session's
            // expanded box.
            // No host call here — the actual session creation happens
            // in the prompt's Enter handler (see `Event::Key`).
            state.pending_session_name.clear();
            state.new_session_view_offset = 0;
            state.new_session_content_w = 0;
            state.expanded = Some(Selector::NewSessionPrompt);
            true
        },
        ClickAction::CancelNewSessionPrompt => {
            // Same effect as Esc in the NewSessionPrompt key handler:
            // discard the buffer and return to the Sessions selector
            // (the screen the user was on when they tapped the
            // "+ New Session" affordance). In welcome mode this is
            // the welcome screen the user just left; outside welcome
            // mode it is the regular Sessions selector — same
            // back-target either way.
            state.pending_session_name.clear();
            state.new_session_view_offset = 0;
            state.new_session_content_w = 0;
            state.expanded = Some(Selector::Sessions);
            true
        },
        ClickAction::AcceptNewSessionPrompt => {
            // Same effect as Enter in the NewSessionPrompt key handler:
            // hand the buffer to `switch_session` (None → host picks an
            // auto-name) and close the prompt. The client is moved to
            // the new session by the host, which dismounts this plugin.
            let name = std::mem::take(&mut state.pending_session_name);
            let arg = if name.is_empty() { None } else { Some(name.as_str()) };
            switch_session(arg);
            state.new_session_view_offset = 0;
            state.new_session_content_w = 0;
            state.expanded = None;
            true
        },
        ClickAction::SelectPane { tab_position, pane_id } => {
            // The mobile plugin never moves the *client's* focused
            // tab or pane — doing so would yank the client out of the
            // mobile tab (where this plugin lives) and into the
            // destination, making the entire mobile UI vanish. The
            // "selected tab/pane" here is a purely internal concept:
            // it controls which pane's viewport the embedded display
            // reads. We never call `switch_tab_to` or `focus_*_pane`
            // — those would change the client's actual focus and
            // dismount the mobile UI. The plugin embeds the chosen
            // pane via its own renderer (reading
            // `PaneRenderReportWithAnsi` from the host) and forwards
            // keystrokes/clicks via `write_to_pane_id`; neither needs
            // the host's focus. Pane-switch invalidates any active
            // fit — fit is bound to the specific pane that was
            // focused when toggled on.
            clear_fit_if_active(state);
            state.selected_tab_position = Some(tab_position);
            state.selected_pane_id = Some(pane_id);
            // Reset pan so the user lands at the new pane's bottom.
            state.viewport_v_pan = 0;
            state.viewport_h_pan = 0;
            state.expanded = None;
            // Notify the server so the shadow focus marker follows
            // the explicit pane selection.
            sync_shadow_focus(state);
            true
        },
        ClickAction::ToggleFit => {
            // Round-trip the toggle through the server. On entry we need
            // the focused pane (the pane to fit) and its tab; if either
            // is missing we silently bail rather than send a malformed
            // command. The target tab size itself is computed server-side
            // from the plugin's chrome insets — the plugin no longer
            // mirrors the size.
            if state.fit_active {
                state.fit_active = false;
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
                state.fit_active = true;
                // `fit_tab_id` is what subsequent `update_fit_insets`
                // calls use to address the server-side entry by tab.
                state.fit_tab_id = Some(tab.tab_id);
                enter_fit_mode(
                    tab.tab_id,
                    state::pane_id_of(&pane),
                    fit_chrome_insets(state),
                );
                true
            }
        },
        ClickAction::NewPaneInTab { tab_position } => {
            // Synchronous round-trip to the server via the
            // `new_tiled_pane_in_tab` shim: the host blocks here until
            // the new pane exists and returns its id. We then update
            // the mobile UI's selection so the embedded viewport
            // shows the new pane. Server uses
            // `should_change_focus_to_new_tab` semantics that do not
            // apply to NewTiledPane — the client's real focus never
            // changes for tiled-pane creation, so the mobile UI
            // stays mounted on its per-client tab.
            clear_fit_if_active(state);
            if let Some(new_id) = new_tiled_pane_in_tab(tab_position) {
                state.selected_tab_position = Some(tab_position);
                state.selected_pane_id = Some(new_id);
                state.viewport_v_pan = 0;
                state.viewport_h_pan = 0;
                state.expanded = None;
                sync_shadow_focus(state);
            }
            // Either way (success or None) re-render: success closes
            // the selector and shows the new pane; failure leaves the
            // selector open so the user can retry.
            true
        },
        ClickAction::NewTab => {
            // Synchronous round-trip via `new_tab_unfocused`: the
            // shim returns the new tab's position id but does NOT
            // move the mobile client's focus (server dispatches with
            // `should_change_focus_to_new_tab: false`). The new tab's
            // first pane has not yet appeared in our local manifest,
            // so we stash the position and resolve it in the next
            // `PaneUpdate` arm — at which point we set both
            // `selected_tab_position` and `selected_pane_id` and
            // close the selector.
            clear_fit_if_active(state);
            if let Some(tab_position) =
                new_tab_unfocused::<&str>(None, None)
            {
                state.pending_new_tab_position = Some(tab_position);
            }
            true
        },
        ClickAction::ExitMobileMode => {
            // One-way: tell the server to tear down this client's
            // mobile tab. The mobile UI dismounts as the tab closes;
            // re-entry is via reconnect / refresh (auto-detection).
            exit_mobile_mode();
            true
        },
        ClickAction::Keyboard(cell) => {
            let outcome = state.modifier_bar.handle_tap(
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
                TapOutcome::Toggled | TapOutcome::NoOp => {},
            }
            // Schedule the press-flash decay sweep. `KEY_FEEDBACK_MS`
            // is in milliseconds; `set_timeout` takes seconds.
            set_timeout(modifier_bar::KEY_FEEDBACK_MS as f64 / 1000.0);
            true
        },
    }
}

/// If fit is locally active, clear the mirror state and tell the
/// server to revert the override + any fit-induced fullscreen. Used
/// at every plugin-driven focus change (tab/pane switch, focused
/// pane disappearing) so the server's `FitState` doesn't outlive
/// the pane it was bound to.
/// Restrict the session list to entries this client is allowed to
/// see. When the mobile plugin is driven by a web client (reported via
/// `ModeInfo::is_web_client`), sessions whose
/// `SessionInfo::web_clients_allowed` is `false` are hidden — joining
/// one from a browser would fail server-side, so showing it in the
/// switcher is misleading. Terminal-client sessions are unaffected.
/// Matches the gate the session-manager plugin applies in
/// `default-plugins/session-manager/src/main.rs:1241`.
///
/// Welcome-screen sessions are always dropped — every browser tab
/// that hits the base URL spins up its own welcome session, so they
/// pile up quickly and attaching to one is meaningless (the welcome
/// flow exists to *leave* that session, not to be a destination).
/// Identified by any pane with `plugin_url == "welcome-screen"`,
/// the same alias the welcome.kdl layout uses.
fn filter_sessions_for_client(
    sessions: Vec<SessionInfo>,
    state: &State,
) -> Vec<SessionInfo> {
    let is_web_client = state
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

/// True if any pane inside the session is running the welcome-screen
/// plugin alias. Welcome sessions are created automatically for every
/// browser tab landing on the base URL and are not meaningful attach
/// targets.
fn is_welcome_session(session: &SessionInfo) -> bool {
    session
        .panes
        .panes
        .values()
        .flatten()
        .any(|p| p.is_plugin && p.plugin_url.as_deref() == Some("welcome-screen"))
}

pub fn clear_fit_if_active(state: &mut State) {
    if state.fit_active {
        state.fit_active = false;
        state.fit_tab_id = None;
        exit_fit_mode();
    }
}

/// Push the mobile plugin's currently-selected pane to the server as
/// the client's shadow focus, so other connected clients see the
/// mobile focus marker on whatever pane the viewport is rendering.
/// Should be called whenever `selected_pane_id` or
/// `selected_tab_position` changes (the latter because the resolved
/// `current_pane()` follows the selected tab when no explicit pane
/// is picked). Safe to call on every transition — the server's
/// handler deduplicates by clearing any prior entry before applying
/// the new one.
///
/// No-op when no pane is resolvable (e.g. before the first
/// `TabUpdate` has populated the plugin's tab list).
pub fn sync_shadow_focus(state: &State) {
    if let Some(pane) = state.current_pane() {
        set_mobile_focused_pane(state::pane_id_of(&pane));
    }
}

/// The plugin's chrome insets around its embedded viewport, derived
/// purely from semantic UI state (no render dimensions). The server
/// subtracts these from the live plugin-pane size to recover the
/// embedded area, then adds the target tab/pane chrome itself.
///
/// `top` is the title bar (1 row unless suppressed by the welcome flow
/// or the open Sessions selector — mirrors `body_top` in
/// `render::render`). `bottom` is the soft-keyboard modifier bar (1 row
/// while the soft keyboard is visible). There is no horizontal chrome,
/// so `left`/`right` are 0. The server applies any small-screen clamp
/// (it knows the live row count); the plugin reports intent only.
pub fn fit_chrome_insets(state: &State) -> Insets {
    let suppress_top_bar =
        state.welcome_auto_expand_done || state.expanded == Some(Selector::Sessions);
    Insets {
        top: if suppress_top_bar { 0 } else { 1 },
        bottom: if state.soft_keyboard_visible { 1 } else { 0 },
        left: 0,
        right: 0,
    }
}

/// Report the current chrome insets to the server for an active fit.
/// Called inline from each `update()` arm that changes a contributing
/// input (soft-keyboard toggle, Sessions-selector / welcome top-bar
/// transitions). The server re-derives the target tab size from live
/// geometry minus these insets, so the plugin never mirrors a size.
/// No-op when fit is inactive or `fit_tab_id` is unset.
pub fn notify_fit_chrome(state: &State) {
    if !state.fit_active {
        return;
    }
    let Some(tab_id) = state.fit_tab_id else {
        return;
    };
    update_fit_insets(tab_id, fit_chrome_insets(state));
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
    //! Unit tests for the chrome-inset helper `fit_chrome_insets` (the
    //! plugin's remaining fit logic — the size math itself now lives on
    //! the server) and the `ToggleFit` dispatch path. Shim calls inside
    //! these functions resolve to the native-build stub of
    //! `host_run_plugin_command` (see `zellij-tile/src/shim.rs`), so
    //! the tests observe state mutation only; the shim's effect on
    //! the (non-existent) host is irrelevant.
    use super::*;
    use crate::state::State;
    use zellij_tile::prelude::{PaneInfo, TabInfo};

    /// Resting state: the title bar is shown (`top = 1`), no soft
    /// keyboard, no horizontal chrome.
    #[test]
    fn insets_default_top_bar_only() {
        let state = State::default();
        assert_eq!(
            fit_chrome_insets(&state),
            Insets {
                top: 1,
                bottom: 0,
                left: 0,
                right: 0
            }
        );
    }

    /// Soft keyboard visible adds the modifier-bar row as `bottom`.
    #[test]
    fn insets_soft_keyboard_adds_bottom() {
        let mut state = State::default();
        state.soft_keyboard_visible = true;
        assert_eq!(fit_chrome_insets(&state).bottom, 1);
        assert_eq!(fit_chrome_insets(&state).top, 1);
    }

    /// The open Sessions selector suppresses the title bar (`top = 0`),
    /// mirroring `body_top` in `render::render`.
    #[test]
    fn insets_sessions_selector_suppresses_top() {
        let mut state = State::default();
        state.expanded = Some(Selector::Sessions);
        assert_eq!(fit_chrome_insets(&state).top, 0);
    }

    /// A non-Sessions selector (e.g. Panes) does NOT suppress the title
    /// bar — only the Sessions selector reuses the welcome layout.
    #[test]
    fn insets_other_selector_keeps_top() {
        let mut state = State::default();
        state.expanded = Some(Selector::Panes);
        assert_eq!(fit_chrome_insets(&state).top, 1);
    }

    /// The welcome flow suppresses the title bar for the lifetime of the
    /// welcome session.
    #[test]
    fn insets_welcome_flow_suppresses_top() {
        let mut state = State::default();
        state.welcome_auto_expand_done = true;
        assert_eq!(fit_chrome_insets(&state).top, 0);
    }

    /// `notify_fit_chrome` is a no-op when fit is inactive or
    /// `fit_tab_id` is unset (the shim must not be addressed without a
    /// target tab). Asserted indirectly: the call must not panic and
    /// leaves state untouched.
    #[test]
    fn notify_fit_chrome_gated_off() {
        let mut state = State::default();
        state.fit_active = false;
        state.fit_tab_id = Some(7);
        notify_fit_chrome(&state);
        // active but no tab id
        state.fit_active = true;
        state.fit_tab_id = None;
        notify_fit_chrome(&state);
        assert!(state.fit_active);
    }

    /// Static canary: `render.rs` must not invoke any host shim.
    ///
    /// Every shim in `zellij-tile` is backed by
    /// `host_run_plugin_command`, which drains the plugin's stdout via
    /// `read_to_end`. If a shim is called mid-`render`, every byte
    /// already written to stdout is consumed by the host as the
    /// (malformed) protobuf reply payload and the rendered frame the
    /// user actually sees is empty. The fix is to defer the shim call
    /// to `update()`. This test is the canary that would have caught
    /// the original pinch-zoom regression and prevents the same shape
    /// of bug from recurring on a different shim.
    ///
    /// Comment-only lines are skipped so any doc reference to a shim
    /// name in `render.rs` remains legal.
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
            "update_fit_insets",
            "enter_fit_mode",
            "exit_fit_mode",
            "exit_mobile_mode",
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

    /// Build a `State` seeded with one tab + one pane — the minimum
    /// surface required for the `ToggleFit` dispatch path (it resolves
    /// `current_pane()` and `current_tab()`). Returns the `State` ready
    /// to receive `dispatch_click(&mut state, ...)`.
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

    /// `dispatch_click(ToggleFit)` from the OFF state arms fit and
    /// records the bound tab. The shim itself fires (its native stub
    /// no-ops); `fit_tab_id` is what subsequent `update_fit_insets`
    /// calls use to address the server-side entry.
    #[test]
    fn dispatch_toggle_fit_on_path_seeds_fields() {
        let mut state = fit_ready_state();
        assert!(!state.fit_active, "Pre-condition: fit is off");

        let consumed = dispatch_click(&mut state, ClickAction::ToggleFit);

        assert!(consumed);
        assert!(state.fit_active);
        assert_eq!(
            state.fit_tab_id,
            Some(7),
            "tab_id from the seeded TabInfo flows into fit_tab_id"
        );
    }

    /// `dispatch_click(ToggleFit)` from the ON state clears both fit
    /// fields — guards the symmetric path against future drift between
    /// the ON and OFF branches.
    #[test]
    fn dispatch_toggle_fit_off_path_clears_fields() {
        let mut state = fit_ready_state();
        state.fit_active = true;
        state.fit_tab_id = Some(7);

        let consumed = dispatch_click(&mut state, ClickAction::ToggleFit);

        assert!(consumed);
        assert!(!state.fit_active);
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

    /// `pending_new_tab_position` is set by the "+ New Tab" dispatch
    /// arm right after `new_tab_unfocused` returns. The matching
    /// PaneUpdate (with the new tab's first pane) is the first moment
    /// the plugin has a concrete pane id to point selection at.
    /// Confirm the resolver promotes the pending position into both
    /// `selected_tab_position` and `selected_pane_id`, closes the
    /// open selector, and clears the pending field.
    #[test]
    fn pane_update_resolves_pending_new_tab() {
        let mut state = State::default();
        // Existing tab 0 with one pane.
        let mut tab0 = TabInfo::default();
        tab0.position = 0;
        state.tabs.push(tab0);
        let mut pane0 = PaneInfo::default();
        pane0.id = 1;
        pane0.is_plugin = false;
        pane0.is_selectable = true;
        state.panes_by_tab_position.insert(0, vec![pane0]);
        state.selected_tab_position = Some(0);
        state.selected_pane_id = Some(PaneId::Terminal(1));
        // Simulate the dispatch arm's bookkeeping: selector still
        // open, pending tab position recorded.
        state.expanded = Some(crate::state::Selector::Panes);
        state.pending_new_tab_position = Some(1);

        // New PaneUpdate manifest including the new tab 1 with its
        // first pane (id 7).
        let mut new_tab = TabInfo::default();
        new_tab.position = 1;
        state.tabs.push(new_tab);
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

        assert_eq!(state.selected_tab_position, Some(1));
        assert_eq!(state.selected_pane_id, Some(PaneId::Terminal(7)));
        assert_eq!(state.expanded, None);
        assert_eq!(state.pending_new_tab_position, None);
    }

    /// If the matching pane has not yet arrived in the manifest (the
    /// server response and the broadcast PaneUpdate are independent
    /// pipelines), the resolver must leave `pending_new_tab_position`
    /// in place so a subsequent PaneUpdate can still pick it up.
    #[test]
    fn pane_update_keeps_pending_when_target_tab_empty() {
        let mut state = State::default();
        let mut tab0 = TabInfo::default();
        tab0.position = 0;
        state.tabs.push(tab0);
        let mut pane0 = PaneInfo::default();
        pane0.id = 1;
        pane0.is_plugin = false;
        pane0.is_selectable = true;
        state.panes_by_tab_position.insert(0, vec![pane0]);
        state.selected_tab_position = Some(0);
        state.selected_pane_id = Some(PaneId::Terminal(1));
        state.pending_new_tab_position = Some(5);

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

        assert_eq!(state.pending_new_tab_position, Some(5));
        assert_eq!(state.selected_tab_position, Some(0));
    }
}
