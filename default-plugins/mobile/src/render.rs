
use crate::ansi::{move_to, RESET};
use crate::components::{modifier_bar, top_bar};
use crate::frame::chrome_offsets;
use crate::screens::ActiveScreen;
use crate::state::State;

/// Fallback for the very first frame, before any state has arrived.
pub fn render_stub(state: &mut State, rows: usize, cols: usize) {
    state.frame.emit_cursor(None);
    print!("{}{}mobile plugin loaded \u{2014} {}x{}", RESET, move_to(0, 0), rows, cols);
}

pub fn render(state: &mut State, rows: usize, cols: usize) {
    reset_frame(state, rows, cols);
    if rows < 4 || cols < 8 {
        render_too_small(state, rows, cols);
        return;
    }

    let suppress_top_bar = top_bar_suppressed(state);
    let (body_top, bar_height) =
        chrome_offsets(rows, suppress_top_bar, state.frame.soft_keyboard_visible);
    let body_bottom = rows.saturating_sub(bar_height);
    let viewport_height = body_bottom.saturating_sub(body_top);

    state.frame.emit_cursor(viewport_cursor(state, body_top, viewport_height, cols));

    disable_autowrap();
    if !suppress_top_bar && cols > 0 {
        top_bar::render(&state.workspace, &mut state.frame, state.active, 0, cols);
    }
    if body_bottom > body_top {
        render_active_screen(state, body_top, body_bottom, cols);
        render_menu_overlay(state, body_top, body_bottom, cols);
    }
    if bar_height > 0 {
        render_modifier_bar(state, body_bottom, cols);
    }
    enable_autowrap();
}

fn reset_frame(state: &mut State, rows: usize, cols: usize) {
    state.frame.click_regions.clear();
    state.viewport.viewport_region = None;
    state.frame.last_render_rows = rows;
    state.frame.last_render_cols = cols;
}

fn render_too_small(state: &mut State, rows: usize, cols: usize) {
    state.frame.emit_cursor(None);
    print!("{}\x1b[2J{}mobile {}x{}", RESET, move_to(0, 0), rows, cols);
}

fn top_bar_suppressed(state: &State) -> bool {
    state.sessions.is_welcome_screen || state.active == ActiveScreen::Sessions
}

fn viewport_cursor(
    state: &State,
    body_top: usize,
    viewport_height: usize,
    cols: usize,
) -> Option<(usize, usize)> {
    if state.active != ActiveScreen::Viewport {
        return None;
    }
    let lines = state.workspace.current_pane_viewport_len();
    let max_v_pan = lines.saturating_sub(viewport_height);
    let skip = max_v_pan - state.viewport.viewport_v_pan.min(max_v_pan);
    state.viewport.compute_cursor_position(
        &state.workspace,
        body_top,
        viewport_height,
        cols,
        skip,
        state.viewport.viewport_h_pan,
    )
}

fn render_active_screen(state: &mut State, body_top: usize, body_bottom: usize, cols: usize) {
    let frame = &mut state.frame;
    match state.active {
        ActiveScreen::Viewport => {
            state.viewport.render(&state.workspace, frame, body_top, body_bottom, cols)
        },
        ActiveScreen::Sessions => {
            state.sessions.render(&mut state.navigation, frame, body_top, body_bottom, cols)
        },
        ActiveScreen::Panes => {
            state.panes.render(&state.workspace, &mut state.navigation, frame, body_top, body_bottom, cols)
        },
        ActiveScreen::NewSessionPrompt => {
            state.new_session.render(frame, body_top, body_bottom, cols)
        },
    }
}

fn render_menu_overlay(state: &mut State, body_top: usize, body_bottom: usize, cols: usize) {
    if state.menu.open && state.active == ActiveScreen::Viewport {
        state.menu.render(&state.fit, &mut state.frame, body_top, body_bottom, cols);
    }
}

fn render_modifier_bar(state: &mut State, body_bottom: usize, cols: usize) {
    // Read `ctrl_held` / `alt_held` directly: `Event::Key` clears these
    // canonical flags without touching the controller's mirror, which
    // can be stale-armed.
    let armed = modifier_bar::KeyboardModifiers {
        ctrl_armed: state.input.ctrl_held,
        alt_armed: state.input.alt_held,
    };
    modifier_bar::render_modifier_bar(&armed, body_bottom, cols, &mut state.frame.click_regions);
}

/// DECAWM off: long rows must not wrap mid-paint.
fn disable_autowrap() {
    print!("\x1b[?7l");
}

fn enable_autowrap() {
    print!("\x1b[?7h");
}
