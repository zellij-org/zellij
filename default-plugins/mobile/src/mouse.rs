//! Mouse event handling for the mobile plugin.
//!
//! Scroll gestures pan the embedded viewport (or scroll an open
//! selector); left clicks resolve against the plugin's chrome first and
//! otherwise pass through to the embedded pane. The `Event::Mouse` arm
//! in `main.rs` routes each `Mouse` variant to one of the `pub(crate)`
//! entry points here.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::dispatch_click;
use crate::state::{self, Selector, State};

/// A vertical scroll either scrolls the open selector's row list or, if
/// no selector is open, pans the embedded viewport. Overflow past the
/// cached viewport spills into the pane's scrollback (see
/// `handle_scroll_pan`).
pub(crate) fn scroll_or_pan(state: &mut State, lines: usize, up: bool) -> bool {
    if state.expanded.is_some() {
        handle_selector_scroll(state, lines, up)
    } else {
        handle_scroll_pan(state, lines, up)
    }
}

/// Pan the embedded viewport horizontally. Dropped (returns `false`)
/// when panning is not allowed; otherwise moves `viewport_h_pan` and
/// returns `true` to re-render.
pub(crate) fn pan_horizontally(state: &mut State, cols: usize, right: bool) -> bool {
    if !pan_is_allowed(state) {
        return false;
    }
    state.viewport_h_pan = if right {
        state.viewport_h_pan.saturating_add(cols)
    } else {
        state.viewport_h_pan.saturating_sub(cols)
    };
    true
}

/// Resolve a left click in priority order:
/// 1. Plugin chrome (top bar / selector regions) — always wins.
/// 2. An open dropdown menu — a click outside any item dismisses it.
/// 3. The embedded pane — forward the tap to the program below.
pub(crate) fn handle_left_click(state: &mut State, line: usize, col: usize) -> bool {
    if let Some(action) = state.click_to_action(line, col) {
        return dispatch_click(state, action);
    }
    if state.menu_open {
        state.menu_open = false;
        return true;
    }
    forward_click_to_pane(state, line, col)
}

/// Forward a non-chrome click to the embedded pane. Terminal panes
/// receive a synthesized SGR mouse press+release; plugin panes (whose
/// host input parser drops SGR mouse bytes) instead receive a
/// structured `mobile_viewport_click` pipe message they may opt into.
/// Always returns `false`: the pane re-renders itself via a fresh
/// `PaneRenderReportWithAnsi`.
fn forward_click_to_pane(state: &mut State, line: usize, col: usize) -> bool {
    let Some((pane_row, pane_col)) = state.click_in_viewport(line, col) else {
        return false;
    };
    let Some(pane) = state.current_pane() else {
        return false;
    };
    if pane.is_plugin {
        let mut args = BTreeMap::new();
        args.insert("row".to_string(), pane_row.to_string());
        args.insert("col".to_string(), pane_col.to_string());
        let message = MessageToPlugin::new("mobile_viewport_click")
            .with_destination_plugin_id(pane.id)
            .with_args(args);
        pipe_message_to_plugin(message);
    } else {
        let bytes = sgr_left_click(pane_row, pane_col);
        write_to_pane_id(bytes, state::pane_id_of(&pane));
    }
    false
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
/// passthrough in the `Event::Mouse` arm.
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

#[cfg(test)]
mod tests {
    use super::*;

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
