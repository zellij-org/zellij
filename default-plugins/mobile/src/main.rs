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

use std::collections::{BTreeMap, BTreeSet};
use zellij_tile::prelude::*;

use crate::state::{
    pane_id_of, BottomBarAction, BottomBarShortcut, ClickAction, Selector, State,
};

/// How long a bottom-bar shortcut stays painted in its "pressed"
/// colour after a tap before reverting to the resting colour. The
/// renderer reads `pressed_at` and the `Event::Timer` sweep in
/// `update` clears the stamp once the window elapses.
const BOTTOM_BAR_FEEDBACK_MS: u128 = 400;

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

        // Arm typing_mode by default so that the moment the user
        // brings up the soft keyboard (by tapping ⌨), characters flow
        // through to the selected pane without an extra step. The
        // browser-side soft keyboard stays hidden until the user asks
        // for it — the two concerns are deliberately decoupled.
        self.typing_mode = true;

        // Bottom-bar shortcuts are populated here (rather than via
        // `Default`) so future entries can carry runtime-derived
        // labels (e.g. mode-aware strings) without forcing
        // `BottomBarShortcut` itself to be `Default`.
        //
        // Order is the visual order on screen. `CTRL` and `ALT` are
        // sticky-modifier toggles whose held state is rendered in
        // place of the transient press flash; the remaining entries
        // send a key (with any held modifiers folded in) and use the
        // standard 400 ms transient feedback.
        self.bottom_bar_shortcuts = vec![
            BottomBarShortcut {
                label: "ESC".to_string(),
                action: BottomBarAction::SendKey(BareKey::Esc),
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "TAB".to_string(),
                action: BottomBarAction::SendKey(BareKey::Tab),
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "CTRL".to_string(),
                action: BottomBarAction::ToggleCtrl,
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "ALT".to_string(),
                action: BottomBarAction::ToggleAlt,
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "-".to_string(),
                action: BottomBarAction::SendKey(BareKey::Char('-')),
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "\u{2191}".to_string(), // ↑
                action: BottomBarAction::SendKey(BareKey::Up),
                pressed_at: None,
            },
            BottomBarShortcut {
                label: "\u{2193}".to_string(), // ↓
                action: BottomBarAction::SendKey(BareKey::Down),
                pressed_at: None,
            },
        ];

        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::Key,
            EventType::Mouse,
            EventType::PaneRenderReportWithAnsi,
            EventType::SessionUpdate,
            // `Timer` powers the bottom-bar press-feedback sweep —
            // each tap on a shortcut schedules `set_timeout(0.4)` and
            // the resulting Timer event clears `pressed_at` so the
            // resting colour resumes.
            EventType::Timer,
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
            Event::Timer(_) => {
                // Sweep every shortcut and clear any whose feedback
                // window has elapsed. Returning `true` only when at
                // least one entry actually changed avoids gratuitous
                // re-renders if a stray Timer arrives outside any
                // pressed window.
                let mut any_cleared = false;
                for shortcut in self.bottom_bar_shortcuts.iter_mut() {
                    if let Some(at) = shortcut.pressed_at {
                        if at.elapsed().as_millis() >= BOTTOM_BAR_FEEDBACK_MS {
                            shortcut.pressed_at = None;
                            any_cleared = true;
                        }
                    }
                }
                any_cleared
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
                // The keyboard icon in the top bar gates pty
                // forwarding: when armed, every key the plugin sees
                // (i.e. every key the server's keybinding layer did
                // not consume) is forwarded to the selected pane's
                // pty; when unarmed, the plugin swallows keys so the
                // user can browse without a stray tap or autocorrect
                // landing in the embedded program.
                if self.typing_mode {
                    if let Some(pane) = self.current_pane() {
                        // Fold any sticky modifiers held by the
                        // bottom bar into the soft-keyboard key so
                        // a user can do CTRL → 'c' across two
                        // input sources to produce Ctrl+C. The
                        // sticky flags are consumed unconditionally
                        // (see the matching comment in
                        // `dispatch_click`).
                        let key = if self.ctrl_held || self.alt_held {
                            merge_held_modifiers(
                                &key,
                                self.ctrl_held,
                                self.alt_held,
                            )
                        } else {
                            key.clone()
                        };
                        let bytes = keys::serialize_key(&key);
                        if !bytes.is_empty() {
                            write_to_pane_id(bytes, state::pane_id_of(&pane));
                        }
                    }
                    // Returning `true` triggers a render so the
                    // CTRL/ALT labels drop back to their resting
                    // colour the moment the modifier is consumed.
                    let consumed = self.ctrl_held || self.alt_held;
                    self.ctrl_held = false;
                    self.alt_held = false;
                    return consumed;
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

/// Construct a `KeyWithModifier` whose modifier set is exactly
/// `{Ctrl?, Alt?}` — used by the bottom-bar `SendKey` dispatch when
/// folding sticky modifiers into a tap. Any modifier the bare key
/// "owns" implicitly (none of the bare keys involved here do) would
/// be lost; the existing serializer in `keys::serialize_key` reads
/// only `bare_key` + `key_modifiers`, so this is sufficient.
fn build_key(bare_key: BareKey, ctrl: bool, alt: bool) -> KeyWithModifier {
    let mut mods = BTreeSet::new();
    if ctrl {
        mods.insert(KeyModifier::Ctrl);
    }
    if alt {
        mods.insert(KeyModifier::Alt);
    }
    KeyWithModifier {
        bare_key,
        key_modifiers: mods,
    }
}

/// Return a clone of `key` with `Ctrl` / `Alt` added to its modifier
/// set when the corresponding sticky flag is on. Used by the
/// typing-mode handler so a soft-keyboard tap that follows a CTRL
/// or ALT bar tap produces a properly-modified key.
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
        ClickAction::ToggleType => {
            // Flip the soft-keyboard visibility on the calling
            // client's browser. `typing_mode` (the in-plugin "keys
            // flow through" flag) is left armed at all times now —
            // the user wanted to type as soon as the keyboard appears
            // without an extra step.
            state.soft_keyboard_visible = !state.soft_keyboard_visible;
            set_soft_keyboard(state.soft_keyboard_visible);
            true
        },
        ClickAction::BottomBarShortcut(idx) => {
            // Clone the action out of the shortcut before doing
            // anything else: the dispatch step needs to read other
            // parts of `state` (current pane, held modifiers) which
            // requires releasing the mutable borrow on the shortcut.
            let action = state
                .bottom_bar_shortcuts
                .get(idx)
                .map(|s| s.action.clone());
            let Some(action) = action else { return false };
            match action {
                BottomBarAction::ToggleCtrl => {
                    // Modifier toggles are pure state changes — no
                    // bytes flow to the pane and there is no
                    // 400 ms transient flash. The held state itself
                    // is the visual feedback (rendered in colour 2
                    // while set).
                    state.ctrl_held = !state.ctrl_held;
                },
                BottomBarAction::ToggleAlt => {
                    state.alt_held = !state.alt_held;
                },
                BottomBarAction::SendKey(bare_key) => {
                    // Fold any held sticky modifiers into the key,
                    // serialize via the same encoder used by typing
                    // mode (so behaviour is consistent across both
                    // input paths), and write to the pane that is
                    // visible in the embedded viewport — *not* the
                    // host-focused pane (which is the plugin
                    // itself).
                    if let Some(pane) = state.current_pane() {
                        let key = build_key(bare_key, state.ctrl_held, state.alt_held);
                        let bytes = keys::serialize_key(&key);
                        if !bytes.is_empty() {
                            write_to_pane_id(bytes, pane_id_of(&pane));
                        }
                    }
                    // Modifiers are consumed regardless of whether
                    // the key actually produced bytes — the user's
                    // mental model is "the next tap consumed
                    // them", and stranding `ctrl_held = true`
                    // after a no-op key would surprise them on the
                    // next tap.
                    state.ctrl_held = false;
                    state.alt_held = false;
                    // Stamp the transient press flash on the
                    // tapped key. Modifier toggles deliberately
                    // skip this so their held colour is the only
                    // signal.
                    if let Some(shortcut) = state.bottom_bar_shortcuts.get_mut(idx) {
                        shortcut.pressed_at = Some(std::time::Instant::now());
                    }
                    set_timeout(BOTTOM_BAR_FEEDBACK_MS as f64 / 1000.0);
                },
            }
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
