//! Rendering for the mobile plugin's UI. The plugin lays out two
//! horizontal regions stacked top-to-bottom:
//!
//! 1. **Top bar** — collapsed view shows a single line of the form
//!    "Zellij <session> <tab> <pane> <CHANGE>" with each segment
//!    coloured from the user's palette. Tapping the tab/pane segments
//!    or the trailing `<CHANGE>` button expands a selector below the
//!    bar; tapping a selector entry collapses back to the bar.
//! 2. **Embedded viewport** — slice of the latest ANSI viewport for
//!    the selected pane, occupying the remaining rows.
//!
//! The renderer also rebuilds `state.click_regions` so the input
//! handler can dispatch a `Mouse::LeftClick` to the right action.

use crate::state::{
    pane_id_of, ClickAction, ClickRegion, LastEmittedCursor, Selector, State, ViewportRegion,
};
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

/// Single ANSI escape that resets the active style. Emitted between
/// every UI cell so a residual SGR bleed from the embedded viewport
/// does not contaminate the chrome.
const RESET: &str = "\x1b[0m";

/// Move the cursor to (row, col), 1-based as ANSI expects. The plugin
/// render area is 0-based, so we add 1 here.
fn move_to(row: usize, col: usize) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

/// Renders the stub UI used during scaffolding; kept as a fallback for
/// the very first frame before any state has been received.
pub fn render_stub(state: &mut State, rows: usize, cols: usize) {
    emit_cursor(state, None);
    print!("{}{}mobile plugin loaded \u{2014} {}x{}", RESET, move_to(0, 0), rows, cols);
}

/// Forward a `show_cursor` call to the host only if it would change
/// the host's view of the plugin cursor. Without this guard we hit a
/// render storm: every `ScreenInstruction::ShowPluginCursor` on the
/// server runs a full `screen.render` + `log_and_report_session_state`
/// (see `screen.rs::ShowPluginCursor`), which produces a fresh
/// `PaneRenderReportWithAnsi` for the plugin's subscription, which
/// drives another plugin render, which calls `show_cursor` again …
fn emit_cursor(state: &mut State, new_pos: Option<(usize, usize)>) {
    let needs_emit = match state.last_emitted_cursor {
        LastEmittedCursor::Unknown => true,
        LastEmittedCursor::Sent(prev) => prev != new_pos,
    };
    if needs_emit {
        show_cursor(new_pos);
        state.last_emitted_cursor = LastEmittedCursor::Sent(new_pos);
    }
}

/// Top-level render. Clears the screen, paints the top bar at row
/// 0, and fills rows 1..rows with either the embedded pane viewport
/// (collapsed) or one of the selector menus (expanded). Selectors
/// *replace* the viewport rather than push it down — when the user
/// is browsing tabs / panes / sessions the live viewport is hidden.
pub fn render(state: &mut State, rows: usize, cols: usize) {
    state.click_regions.clear();
    state.viewport_region = None;

    if rows < 4 || cols < 8 {
        // No room for a meaningful UI — degrade to the stub. Hide the
        // host cursor since there's nothing meaningful to point at.
        emit_cursor(state, None);
        print!("{}\x1b[2J{}mobile {}x{}", RESET, move_to(0, 0), rows, cols);
        return;
    }

    // Top bar always sits at row 0; the body fills rows 1..rows.
    let body_top = 1;
    let body_bottom = rows;
    let viewport_height = body_bottom.saturating_sub(body_top);

    // Cursor mapping only matters when the embedded viewport is
    // visible. Hide the host cursor whenever a selector is open so the
    // pane cursor doesn't blink behind the menu.
    let new_cursor = if state.expanded.is_none() {
        let viewport_lines_len = state.current_pane_viewport_len();
        let skip = viewport_lines_len.saturating_sub(viewport_height);
        compute_cursor_position(state, body_top, viewport_height, cols, skip)
    } else {
        None
    };
    // FIRST: tell the host where the embedded pane's cursor sits. We
    // pipe through `emit_cursor` rather than calling `show_cursor`
    // directly because every `show_cursor` invocation on the server
    // triggers a fresh `screen.render` + session-state report — that
    // would feed `PaneRenderReportWithAnsi` straight back to the
    // plugin and drive a render loop. `emit_cursor` deduplicates
    // against the last-sent value so we only pay that cost when the
    // cursor target genuinely moves.
    emit_cursor(state, new_cursor);

    // Always start the chrome paint clean — `\x1b[2J` clears the
    // visible area and we rewrite each region from (0, 0).
    print!("{}\x1b[2J", RESET);

    render_top_bar(state, 0, cols);

    if body_bottom > body_top {
        match state.expanded {
            None => render_embedded_viewport(state, body_top, body_bottom, cols),
            Some(Selector::Sessions) => {
                render_session_selector(state, body_top, body_bottom, cols)
            },
            Some(Selector::Tabs) => {
                render_tab_selector(state, body_top, body_bottom, cols)
            },
            Some(Selector::Panes) => {
                render_pane_selector(state, body_top, body_bottom, cols)
            },
        }
    }
}

/// Map the underlying pane's reported cursor coordinates into the
/// plugin's render coordinates, returning `None` if the cursor is
/// hidden, off-screen (cropped above the bottom-anchored slice or
/// past the right edge), or no pane is selected. The renderer feeds
/// this directly to `show_cursor`.
///
/// We read the cursor from `PaneContents.cursor` rather than
/// `PaneInfo.cursor_coordinates_in_pane` because the latter is only
/// refreshed on `PaneUpdate` (structural changes — pane added,
/// removed, resized, renamed) and so misses every cursor move within
/// the pane. `PaneContents.cursor`, by contrast, is populated on
/// every render-cycle's ANSI capture, so the embedded cursor follows
/// typing in real time. The cursor field is already in viewport
/// coordinates, no frame-offset subtraction is needed.
fn compute_cursor_position(
    state: &State,
    viewport_top: usize,
    viewport_height: usize,
    cols: usize,
    skip: usize,
) -> Option<(usize, usize)> {
    if viewport_height == 0 {
        return None;
    }
    let pane = state.current_pane()?;
    let pane_id = pane_id_of(&pane);
    let (cursor_x, cursor_y) = state.latest_pane_contents.get(&pane_id)?.cursor?;
    if cursor_y < skip {
        return None; // above the bottom-anchored slice
    }
    let row_in_slice = cursor_y - skip;
    if row_in_slice >= viewport_height {
        return None; // below the slice (shouldn't happen with skip = len - height)
    }
    if cursor_x >= cols {
        return None; // past the right edge
    }
    let plugin_y = viewport_top + row_in_slice;
    let plugin_x = cursor_x;
    Some((plugin_x, plugin_y))
}

/// Top bar: `Zellij <session> | <tab> | <pane> | ⌨    ☰`. Rendered as
/// a single `Text` component with `.selected()` and a width covering
/// the entire row, so:
/// - Each segment's foreground colour comes from the host's selected
///   emphasis palette (levels 0..=3 → `text_selected.emphasis_0..3`)
///   via `color_range`. See `style_of_index` in
///   `zellij-server/src/ui/components/text.rs`.
/// - The whole row is painted with `text_selected.background`, which
///   on the standard Zellij themes is the lighter-gray "selection"
///   shade — distinct from the embedded pane content below.
///
/// The keyboard glyph (`⌨`) toggles `state.typing_mode`. When armed
/// it is drawn in the success palette colour (typically green) so the
/// user can tell at a glance whether soft-keyboard input flows
/// through to the embedded pane. The hamburger glyph (`☰`) opens the
/// panes selector when collapsed and collapses back when a selector
/// is open — a single right-anchored "menu" affordance.
fn render_top_bar(state: &mut State, row: usize, cols: usize) {
    if cols == 0 {
        return;
    }
    match state.expanded {
        None => render_top_bar_collapsed(state, row, cols),
        Some(selector) => render_top_bar_in_selector(state, row, cols, selector),
    }
}

/// Helper: append `s` to `bar`, bumping both the character cursor
/// (used for `Text::color_range`, which is character-indexed) and the
/// cell cursor (used for click-region hit testing). Returns the
/// `(char_start, char_end, cell_start, cell_end)` of the appended
/// segment so callers can paint colour ranges and click regions
/// against either coordinate space.
fn append_segment(
    bar: &mut String,
    chars: &mut usize,
    cells: &mut usize,
    s: &str,
) -> (usize, usize, usize, usize) {
    let chars_start = *chars;
    let cells_start = *cells;
    bar.push_str(s);
    *chars += s.chars().count();
    *cells += UnicodeWidthStr::width(s);
    (chars_start, *chars, cells_start, *cells)
}

/// Collapsed top bar: `Zellij <session> | <tab> | <pane> | ⌨    ☰`.
fn render_top_bar_collapsed(state: &mut State, row: usize, cols: usize) {
    let session_name = state
        .session_name
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let tab_name = state
        .current_tab()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "—".to_string());
    let pane_name = state
        .current_pane()
        .map(|p| {
            if p.title.is_empty() {
                format!("#{}", p.id)
            } else {
                p.title.clone()
            }
        })
        .unwrap_or_else(|| "—".to_string());

    let prefix = "Zellij ";
    let pipe = " | ";
    let typing_icon = "\u{2328}"; // ⌨
    let hamburger = "\u{2630}"; // ☰

    let mut bar = String::with_capacity(cols + 16);
    let mut chars: usize = 0;
    let mut cells: usize = 0;

    append_segment(&mut bar, &mut chars, &mut cells, prefix);
    let (session_chars_s, session_chars_e, session_cells_s, session_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &session_name);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (tab_chars_s, tab_chars_e, tab_cells_s, tab_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &tab_name);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (pane_chars_s, pane_chars_e, pane_cells_s, pane_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &pane_name);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (typing_chars_s, typing_chars_e, typing_cells_s, typing_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, typing_icon);

    // Right-align the hamburger. If the bar overflows, fall back to a
    // single-space gap so the glyphs don't collide.
    let hamburger_cells = UnicodeWidthStr::width(hamburger);
    let pad_cells = cols
        .saturating_sub(cells + hamburger_cells)
        .max(1);
    for _ in 0..pad_cells {
        bar.push(' ');
    }
    chars += pad_cells;
    cells += pad_cells;
    let (hamburger_chars_s, hamburger_chars_e, hamburger_cells_s, hamburger_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, hamburger);

    // Compose the styled bar. The keyboard icon switches between
    // emphasis-3 (unarmed) and success-colour (armed); both are clear
    // signals against the selected-bar background.
    let mut text = Text::new(&bar)
        .selected()
        .color_range(0, session_chars_s..session_chars_e)
        .color_range(1, tab_chars_s..tab_chars_e)
        .color_range(2, pane_chars_s..pane_chars_e)
        .color_range(3, hamburger_chars_s..hamburger_chars_e);
    text = if state.typing_mode {
        text.success_color_range(typing_chars_s..typing_chars_e)
    } else {
        text.color_range(3, typing_chars_s..typing_chars_e)
    };
    print_text_with_coordinates(text, 0, row, Some(cols), None);

    // Click regions are in cell coordinates (the mouse handler
    // receives cell columns, not char indices). Wide chars in tab /
    // pane / session names are handled correctly because we tracked
    // both metrics during composition.
    state.click_regions.push(ClickRegion {
        row,
        col_start: session_cells_s,
        col_end: session_cells_e,
        action: ClickAction::ExpandSessions,
    });
    state.click_regions.push(ClickRegion {
        row,
        col_start: tab_cells_s,
        col_end: tab_cells_e,
        action: ClickAction::ExpandTabs,
    });
    state.click_regions.push(ClickRegion {
        row,
        col_start: pane_cells_s,
        col_end: pane_cells_e,
        action: ClickAction::ExpandPanes,
    });
    state.click_regions.push(ClickRegion {
        row,
        col_start: typing_cells_s,
        col_end: typing_cells_e,
        action: ClickAction::ToggleType,
    });
    state.click_regions.push(ClickRegion {
        row,
        col_start: hamburger_cells_s,
        col_end: hamburger_cells_e,
        action: ClickAction::Menu,
    });
}

/// Selector top bar: `Zellij <current-X> | Switch <X>`. The current
/// value mirrors the entity the user is browsing — session name when
/// the Sessions selector is open, active tab name for Tabs, focused
/// pane name for Panes — and is coloured with the same emphasis
/// level the collapsed bar uses for that entity (session=0, tab=1,
/// pane=2). The keyboard and hamburger glyphs are deliberately
/// omitted; the entire bar is a single click region that closes the
/// menu and returns to the viewport.
fn render_top_bar_in_selector(
    state: &mut State,
    row: usize,
    cols: usize,
    selector: Selector,
) {
    let (current_value, entity_emphasis, action_label) = match selector {
        Selector::Sessions => (
            state
                .session_name
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            0usize,
            "Switch Session",
        ),
        Selector::Tabs => (
            state
                .current_tab()
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "—".to_string()),
            1usize,
            "Switch Tab",
        ),
        Selector::Panes => (
            state
                .current_pane()
                .map(|p| {
                    if p.title.is_empty() {
                        format!("#{}", p.id)
                    } else {
                        p.title.clone()
                    }
                })
                .unwrap_or_else(|| "—".to_string()),
            2usize,
            "Switch Pane",
        ),
    };

    let prefix = "Zellij ";
    let pipe = " | ";

    let mut bar = String::with_capacity(cols + 16);
    let mut chars: usize = 0;
    let mut cells: usize = 0;

    append_segment(&mut bar, &mut chars, &mut cells, prefix);
    let (entity_chars_s, entity_chars_e, _, _) =
        append_segment(&mut bar, &mut chars, &mut cells, &current_value);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (action_chars_s, action_chars_e, _, _) =
        append_segment(&mut bar, &mut chars, &mut cells, action_label);

    let text = Text::new(&bar)
        .selected()
        .color_range(entity_emphasis, entity_chars_s..entity_chars_e)
        .color_range(3, action_chars_s..action_chars_e);
    print_text_with_coordinates(text, 0, row, Some(cols), None);

    // Single bar-wide click region: tapping anywhere on the title
    // collapses the menu and returns to the embedded viewport. The
    // bar carries no other interactive segment in this mode.
    state.click_regions.push(ClickRegion {
        row,
        col_start: 0,
        col_end: cols,
        action: ClickAction::Menu,
    });
}

/// Render a generic full-height list selector occupying rows
/// `row_start..row_end`. The selector body is just the items — the
/// "Switch <X>" header lives in the top bar so the body stays as
/// dense as possible. Each item carries its own `ClickAction` and a
/// `selected` flag for the active row marker.
fn render_list_selector(
    state: &mut State,
    row_start: usize,
    row_end: usize,
    cols: usize,
    items: Vec<(String, ClickAction, bool)>,
) {
    if row_end <= row_start {
        return;
    }
    let max_items = row_end.saturating_sub(row_start);
    for (idx, (line, action, selected)) in items.into_iter().take(max_items).enumerate() {
        let row = row_start + idx;
        let mark = if selected { "▸" } else { " " };
        let display = format!(" {} {}", mark, line);
        print!("{}{}", RESET, move_to(row, 0));
        let mut col = 0;
        print_clipped(&display, &mut col, cols);
        // Clear the rest of the row so a wider previous entry is
        // overwritten cleanly.
        clear_to_end(col, cols);
        state.click_regions.push(ClickRegion {
            row,
            col_start: 0,
            col_end: cols,
            action,
        });
    }
}

fn render_session_selector(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let current = state.session_name.clone();
    let mut items: Vec<(String, ClickAction, bool)> = state
        .sessions
        .iter()
        .map(|s| {
            let label = if s.is_current_session {
                format!("{} (current)", s.name)
            } else {
                s.name.clone()
            };
            let selected = current.as_deref() == Some(s.name.as_str());
            (label, ClickAction::SelectSession(s.name.clone()), selected)
        })
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    render_list_selector(state, row_start, row_end, cols, items);
}

fn render_tab_selector(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let active_position = state
        .current_tab()
        .map(|t| t.position)
        .unwrap_or(usize::MAX);
    let items: Vec<(String, ClickAction, bool)> = state
        .tabs_in_order()
        .into_iter()
        .map(|t| {
            let line = format!("{}. {}", t.position + 1, t.name);
            let selected = t.position == active_position;
            (line, ClickAction::SelectTab(t.position), selected)
        })
        .collect();
    render_list_selector(state, row_start, row_end, cols, items);
}

fn render_pane_selector(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let panes: Vec<PaneInfo> = state.current_tab_panes().into_iter().cloned().collect();
    let selected_pane_id = state.current_pane().map(|p| pane_id_of(&p));
    let items: Vec<(String, ClickAction, bool)> = panes
        .into_iter()
        .map(|pane| {
            let id = pane_id_of(&pane);
            let layer = if pane.is_floating { "F" } else { "T" };
            let title = if pane.title.is_empty() {
                format!("#{}", pane.id)
            } else {
                pane.title.clone()
            };
            let line = format!("[{}] {}", layer, title);
            let selected = Some(id) == selected_pane_id;
            (line, ClickAction::SelectPane(id), selected)
        })
        .collect();
    render_list_selector(state, row_start, row_end, cols, items);
}

fn render_embedded_viewport(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let height = row_end - row_start;
    if height == 0 {
        return;
    }

    let pane = state.current_pane();
    let pane_id = pane.as_ref().map(pane_id_of);
    let viewport_lines: Vec<String> = pane_id
        .and_then(|id| state.latest_pane_contents.get(&id))
        .map(|c| c.viewport.clone())
        .unwrap_or_default();

    // Anchor the slice to the bottom of the pane's viewport: when the
    // pane is taller than our embedded area, the most recent (bottom)
    // lines are what the user wants to see — that's where the cursor
    // and most-recent terminal output live.
    let skip = viewport_lines.len().saturating_sub(height);

    // Record where the viewport landed so the mouse handler can
    // reverse-map clicks into pane coordinates. We store this even when
    // we have no cached lines yet, so the user's first viewport tap
    // still maps to row 0 of an eventually-populated cache.
    state.viewport_region = Some(ViewportRegion {
        row_start,
        row_end,
        cols,
        skip,
    });

    // Reset before each row to keep the chrome's styling separate from
    // the pane's emitted SGR runs.
    for i in 0..height {
        let row = row_start + i;
        print!("{}{}", RESET, move_to(row, 0));
        if let Some(line) = viewport_lines.get(skip + i) {
            // Trust the ANSI; xterm style resets at end of pane line
            // are already part of the rendered stream.
            print!("{}", line);
        } else if i == 0 && pane_id.is_none() {
            print!("{}(no pane selected)", RESET);
        } else if i == 0 && viewport_lines.is_empty() {
            print!("{}(awaiting first render…)", RESET);
        }
        // Clear any overrun from the previous frame.
        let printed_width = viewport_lines
            .get(skip + i)
            .map(|l| visible_width(l))
            .unwrap_or(0);
        if printed_width < cols {
            print!("{}\x1b[K", RESET);
        }
    }
}

/// Width of `text` after stripping ANSI escape sequences. Used so the
/// renderer knows how many cells of the row are actually painted.
fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Skip CSI / OSC sequences. This is a coarse approximation
            // — good enough for measuring overrun against `cols`.
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        } else {
            // Walk one UTF-8 char.
            let ch_len = utf8_char_len(bytes[i]);
            if i + ch_len <= bytes.len() {
                if let Ok(s) = std::str::from_utf8(&bytes[i..i + ch_len]) {
                    width += UnicodeWidthStr::width(s);
                }
            }
            i += ch_len.max(1);
        }
    }
    width
}

fn utf8_char_len(byte: u8) -> usize {
    if byte < 0x80 {
        1
    } else if byte < 0xc0 {
        1
    } else if byte < 0xe0 {
        2
    } else if byte < 0xf0 {
        3
    } else {
        4
    }
}

/// Print `text` starting at `*col`, clipped to `cols`. Updates `*col`
/// to the next available cell.
fn print_clipped(text: &str, col: &mut usize, cols: usize) {
    let text_w = UnicodeWidthStr::width(text);
    if *col + text_w <= cols {
        print!("{}{}", RESET, text);
        *col += text_w;
    } else {
        let remaining = cols.saturating_sub(*col);
        // Walk graphemes until we exhaust `remaining`.
        let mut taken = 0;
        let mut buf = String::new();
        for ch in text.chars() {
            let mut tmp = [0u8; 4];
            let s = ch.encode_utf8(&mut tmp);
            let w = UnicodeWidthStr::width(s as &str);
            if taken + w > remaining {
                break;
            }
            buf.push(ch);
            taken += w;
        }
        print!("{}{}", RESET, buf);
        *col += taken;
    }
}

fn clear_to_end(col: usize, cols: usize) {
    let _ = cols;
    let _ = col;
    print!("{}\x1b[K", RESET);
}
