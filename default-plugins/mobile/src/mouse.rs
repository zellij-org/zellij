//! Mouse event handling for the mobile plugin.
//!
//! Scroll gestures pan the embedded viewport (or scroll an open
//! selector); left clicks resolve against the plugin's chrome first and
//! otherwise pass through to the embedded pane. The `Event::Mouse` arm in
//! `main.rs` routes each `Mouse` variant to one of the `pub(crate)` entry
//! points here.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::click;
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;

/// A vertical scroll either scrolls the open selector's row list or, if
/// no selector is open, pans the embedded viewport. Overflow past the
/// cached viewport spills into the pane's scrollback (see
/// `handle_scroll_pan`).
pub(crate) fn scroll_or_pan(state: &mut State, lines: usize, up: bool) -> bool {
    if state.active != ActiveScreen::Viewport {
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
    state.viewport.viewport_h_pan = if right {
        state.viewport.viewport_h_pan.saturating_add(cols)
    } else {
        state.viewport.viewport_h_pan.saturating_sub(cols)
    };
    true
}

/// Resolve a left click in priority order:
/// 1. Plugin chrome (top bar / selector regions) — always wins.
/// 2. An open dropdown menu — a click outside any item dismisses it.
/// 3. The embedded pane — forward the tap to the program below.
pub(crate) fn handle_left_click(state: &mut State, line: usize, col: usize) -> bool {
    if let Some(action) = state.frame.click_to_action(line, col) {
        return click::dispatch(state, action);
    }
    if state.menu.open {
        state.menu.open = false;
        return true;
    }
    forward_click_to_pane(state, line, col)
}

/// Forward a non-chrome click to the embedded pane. Terminal panes
/// receive a synthesized SGR mouse press+release; plugin panes instead
/// receive a structured `mobile_viewport_click` pipe message. Always
/// returns `false`: the pane re-renders itself via a fresh
/// `PaneRenderReportWithAnsi`.
fn forward_click_to_pane(state: &mut State, line: usize, col: usize) -> bool {
    let Some((pane_row, pane_col)) = state.viewport.click_in_viewport(line, col) else {
        return false;
    };
    let Some(pane) = state.workspace.current_pane() else {
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
        write_to_pane_id(bytes, pane_id_of(&pane));
    }
    false
}

/// Compute the new vertical pan offset for a slide gesture and report how
/// many of the gesture's lines did not fit. The overflow count is what
/// the handler converts into `scroll_*_in_pane_id` shim calls so a
/// saturating gesture continues into the underlying pane's scrollback.
///
/// Pure function; no I/O.
fn apply_v_pan(old_pan: usize, max_pan: usize, lines: usize, up: bool) -> (usize, usize) {
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

/// Scroll the currently-open selector's row list. Selectors advance one
/// card block per scroll event, independent of the gesture's line count,
/// so swiping moves the list one session/pane at a time.
fn handle_selector_scroll(state: &mut State, lines: usize, up: bool) -> bool {
    let step = lines.min(1);
    let old = state.navigation.selector_scroll_offset;
    state.navigation.selector_scroll_offset = if up {
        old.saturating_sub(step)
    } else {
        old.saturating_add(step)
    };
    state.navigation.selector_scroll_offset != old
}

/// Apply a vertical slide gesture to the embedded viewport. Partition the
/// gesture's `lines` into "absorbed by the pan" plus "overflow", and
/// forward every overflow line to the selected pane as a single-line
/// scrollback step. Returns `true` iff the local pan moved.
fn handle_scroll_pan(state: &mut State, lines: usize, up: bool) -> bool {
    if !pan_is_allowed(state) {
        return false;
    }
    let Some(max_v_pan) = state.viewport.max_viewport_v_pan(&state.workspace) else {
        // First event tick: no frame has rendered yet, so we don't know
        // the embed height. Preserve pure-pan behaviour; the renderer
        // will clamp on the first frame.
        if up {
            state.viewport.viewport_v_pan = state.viewport.viewport_v_pan.saturating_add(lines);
        } else {
            state.viewport.viewport_v_pan = state.viewport.viewport_v_pan.saturating_sub(lines);
        }
        return true;
    };
    let old_pan = state.viewport.viewport_v_pan;
    let (new_pan, overflow) = apply_v_pan(old_pan, max_v_pan, lines, up);
    let pan_moved = new_pan != old_pan;
    state.viewport.viewport_v_pan = new_pan;
    if overflow > 0 {
        if let Some(pane) = state.workspace.current_pane() {
            let pane_id = pane_id_of(&pane);
            for _ in 0..overflow {
                if up {
                    scroll_up_in_pane_id(pane_id);
                } else {
                    scroll_down_in_pane_id(pane_id);
                }
            }
        }
    }
    pan_moved
}

/// True when a scroll event should drive the embedded-viewport pan
/// offsets rather than be dropped.
fn pan_is_allowed(state: &State) -> bool {
    // No panning while a selector is open: the menu replaces the
    // viewport, so the gesture target the user expects to scroll is the
    // menu itself, not the hidden viewport behind it.
    if state.active != ActiveScreen::Viewport {
        return false;
    }
    // Need a selected pane with cached content — otherwise the pan offset
    // has nothing to act on and the renderer would clamp it back to 0.
    if state.workspace.current_pane().is_none() {
        return false;
    }
    let len = state.workspace.current_pane_viewport_len();
    if len == 0 {
        return false;
    }
    true
}

/// Build an SGR mouse left-click press+release sequence targeting the
/// (0-based) `pane_row`/`pane_col` of the underlying pane's viewport. SGR
/// mouse coordinates are 1-based.
fn sgr_left_click(pane_row: usize, pane_col: usize) -> Vec<u8> {
    let col = pane_col + 1;
    let row = pane_row + 1;
    format!("\x1b[<0;{};{}M\x1b[<0;{};{}m", col, row, col, row).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_v_pan_up_fully_absorbed() {
        assert_eq!(apply_v_pan(0, 100, 3, true), (3, 0));
        assert_eq!(apply_v_pan(50, 100, 3, true), (53, 0));
    }

    #[test]
    fn apply_v_pan_up_partial_overflow() {
        assert_eq!(apply_v_pan(99, 100, 3, true), (100, 2));
        assert_eq!(apply_v_pan(98, 100, 5, true), (100, 3));
    }

    #[test]
    fn apply_v_pan_up_fully_overflowed() {
        assert_eq!(apply_v_pan(100, 100, 3, true), (100, 3));
    }

    #[test]
    fn apply_v_pan_up_zero_max() {
        assert_eq!(apply_v_pan(0, 0, 3, true), (0, 3));
        assert_eq!(apply_v_pan(0, 0, 0, true), (0, 0));
    }

    #[test]
    fn apply_v_pan_down_partial_overflow() {
        assert_eq!(apply_v_pan(2, 100, 3, false), (0, 1));
        assert_eq!(apply_v_pan(5, 100, 3, false), (2, 0));
    }

    #[test]
    fn apply_v_pan_down_fully_overflowed() {
        assert_eq!(apply_v_pan(0, 100, 3, false), (0, 3));
    }

    #[test]
    fn apply_v_pan_zero_lines() {
        assert_eq!(apply_v_pan(5, 100, 0, true), (5, 0));
        assert_eq!(apply_v_pan(5, 100, 0, false), (5, 0));
    }
}
