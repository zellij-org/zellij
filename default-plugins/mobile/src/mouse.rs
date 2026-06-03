use std::collections::BTreeMap;
use zellij_tile::prelude::*;

use crate::click;
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::pane_id_of;

pub(crate) fn scroll_or_pan(state: &mut State, lines: usize, up: bool) -> bool {
    if state.active != ActiveScreen::Viewport {
        handle_selector_scroll(state, lines, up)
    } else {
        handle_scroll_pan(state, lines, up)
    }
}

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

fn handle_scroll_pan(state: &mut State, lines: usize, up: bool) -> bool {
    if !pan_is_allowed(state) {
        return false;
    }
    let Some(max_v_pan) = state.viewport.max_viewport_v_pan(&state.workspace) else {
        // First event tick: no frame has rendered yet, so we don't know
        // the embed height.
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

fn pan_is_allowed(state: &State) -> bool {
    if state.active != ActiveScreen::Viewport {
        return false;
    }
    if state.workspace.current_pane().is_none() {
        return false;
    }
    let len = state.workspace.current_pane_viewport_len();
    if len == 0 {
        return false;
    }
    true
}

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
