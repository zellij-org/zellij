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

use crate::keyboard;
use crate::unix_now;
use crate::state::{
    pane_id_of, ClickAction, ClickRegion, LastEmittedCursor, Selector, State,
    ViewportRegion,
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

    // Top bar always sits at row 0; the body fills the remaining
    // rows. The in-plugin keyboard reserves the bottom-most rows when
    // visible. Its row footprint scales with the plugin's `rows` so
    // the keyboard stays at ~40% of the screen across pinch zoom —
    // see `KEYBOARD_PCT_NUM/DEN` in `keyboard/render.rs`. When the
    // keyboard cannot fit (very short windows) `compute_geometry`
    // returns `None` and the viewport expands to use the full body.
    let body_top = 1;
    let keyboard_geometry = if state.keyboard.visible {
        keyboard::compute_geometry(
            state.keyboard.layout.as_ref(),
            &state.keyboard.modifiers,
            rows,
            cols,
        )
    } else {
        None
    };
    let keyboard_height = keyboard_geometry
        .as_ref()
        .map(|g| g.total_height())
        .unwrap_or(0);
    // Reserve at least one row for the viewport; if the keyboard is
    // bigger than the body, suppress it for this frame.
    let keyboard_fits = keyboard_height + 1 <= rows.saturating_sub(body_top);
    let effective_keyboard_height = if keyboard_fits { keyboard_height } else { 0 };
    let body_bottom = rows.saturating_sub(effective_keyboard_height);
    let viewport_height = body_bottom.saturating_sub(body_top);

    // Cursor mapping only matters when the embedded viewport is
    // visible. Hide the host cursor whenever a selector is open so the
    // pane cursor doesn't blink behind the menu. The skip and h_offset
    // computed here MUST match what `render_embedded_viewport` will
    // pick — otherwise the cursor lands at the wrong row when the user
    // is panned away from the bottom-right corner.
    let new_cursor = if state.expanded.is_none() {
        let viewport_lines_len = state.current_pane_viewport_len();
        let max_v_pan = viewport_lines_len.saturating_sub(viewport_height);
        let v_pan = state.viewport_v_pan.min(max_v_pan);
        let skip = max_v_pan - v_pan;
        let h_offset = state.viewport_h_pan;
        compute_cursor_position(state, body_top, viewport_height, cols, skip, h_offset)
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

    // Disable DECAWM (autowrap) for the entire plugin paint. Confirmed
    // via host grid dump that the top bar's `print_text_with_coordinates`
    // call over-emits past `cols`, triggering autowrap from row 0 into
    // row 1 — which marks row 1 as a wrap-continuation (`(W)` in
    // Grid Debug) and corrupts transmission to the client (xterm.js
    // treats row 0 + row 1 as a single soft-wrapped line). DECAWM-off
    // makes the host's `Grid::add_character` drop overflow past the
    // right edge instead, preserving canonical row boundaries. The
    // matching `\x1b[?7h` re-enable at the end of `render()` restores
    // global state before any unrelated chrome later in the frame.
    print!("\x1b[?7l");

    // Always start the chrome paint clean — `\x1b[2J` clears the
    // visible area and we rewrite each region from (0, 0).
    print!("{}\x1b[2J", RESET);

    render_top_bar(state, 0, cols);

    if body_bottom > body_top {
        match state.expanded {
            None => render_embedded_viewport(state, body_top, body_bottom, cols),
            Some(Selector::Sessions) => {
                render_sessions_menu(state, body_top, body_bottom, cols)
            },
            Some(Selector::Tabs) => render_tabs_menu(state, body_top, body_bottom, cols),
            Some(Selector::Panes) => render_panes_menu(state, body_top, body_bottom, cols),
        }
    }

    if effective_keyboard_height > 0 {
        if let Some(geometry) = keyboard_geometry.as_ref() {
            keyboard::render::render_keyboard(
                state.keyboard.layout.as_ref(),
                &state.keyboard.modifiers,
                &state.keyboard.press_flash,
                geometry,
                body_bottom,
                cols,
                &mut state.click_regions,
            );
        }
    }

    // Re-enable DECAWM. Pairs with the `\x1b[?7l` at the top of this
    // function — see comment there for rationale.
    print!("\x1b[?7h");
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
    h_offset: usize,
) -> Option<(usize, usize)> {
    if viewport_height == 0 {
        return None;
    }
    let pane = state.current_pane()?;
    let pane_id = pane_id_of(&pane);
    let (cursor_x, cursor_y) = state.latest_pane_contents.get(&pane_id)?.cursor?;
    if cursor_y < skip {
        return None; // above the rendered slice (user has panned up)
    }
    let row_in_slice = cursor_y - skip;
    if row_in_slice >= viewport_height {
        return None; // below the rendered slice (shouldn't normally happen)
    }
    if cursor_x < h_offset {
        return None; // left of the rendered slice (user has panned right)
    }
    let plugin_x = cursor_x - h_offset;
    if plugin_x >= cols {
        return None; // past the right edge of the rendered slice
    }
    let plugin_y = viewport_top + row_in_slice;
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
/// The keyboard glyph (`⌨`) toggles `state.keyboard.visible` and
/// asks the browser to suppress the OS soft keyboard while the
/// plugin keyboard is up. When the keyboard is visible the glyph is
/// drawn in the success palette colour (typically green) so the
/// user can tell at a glance which state they are in. The hamburger
/// glyph (`☰`) opens the panes selector when collapsed and
/// collapses back when a selector is open — a single right-anchored
/// "menu" affordance.
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
///
/// The keyboard (`⌨`) and hamburger (`☰`) glyphs must remain visible
/// even when the natural bar width exceeds `cols`. To honour that,
/// segment widths are reduced in priority order — tab first, then
/// pane, then session — until the total fits. If even all segments at
/// their minimum width still overflow, rendering degrades to best
/// effort and the trailing icons may be clipped by the host.
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
    let fit_icon = "\u{26F6}"; // ⛶ (square four corners)
    let typing_icon = "\u{2328}"; // ⌨
    let hamburger = "\u{2630}"; // ☰

    // Priority truncation: keep the trailing icons visible by shrinking
    // segments in tab → pane → session order until the row fits.
    //
    // Fixed chrome that can never be reduced: prefix + four pipes +
    // fit icon + typing icon + at least one cell of separator +
    // hamburger. The saturating subtraction means `available` is 0
    // when even the chrome alone exceeds `cols` — at that point all
    // three segments collapse to their minimums and the host clips
    // whatever spills.
    const MIN_SEG: usize = 3;
    let prefix_w = UnicodeWidthStr::width(prefix);
    let pipe_w = UnicodeWidthStr::width(pipe);
    let fit_icon_w = UnicodeWidthStr::width(fit_icon);
    let typing_icon_w = UnicodeWidthStr::width(typing_icon);
    let hamburger_w = UnicodeWidthStr::width(hamburger);
    let fixed_w = prefix_w + pipe_w * 4 + fit_icon_w + typing_icon_w + 1 + hamburger_w;
    let available = cols.saturating_sub(fixed_w);

    let session_w = UnicodeWidthStr::width(session_name.as_str());
    let tab_w = UnicodeWidthStr::width(tab_name.as_str());
    let pane_w = UnicodeWidthStr::width(pane_name.as_str());

    let session_min = session_w.min(MIN_SEG);
    let tab_min = tab_w.min(MIN_SEG);
    let pane_min = pane_w.min(MIN_SEG);

    let natural = session_w + tab_w + pane_w;
    let (target_session, target_tab, target_pane) = if natural <= available {
        (session_w, tab_w, pane_w)
    } else {
        let mut overflow = natural - available;
        let tab_shrink = overflow.min(tab_w - tab_min);
        let target_tab = tab_w - tab_shrink;
        overflow -= tab_shrink;
        let pane_shrink = overflow.min(pane_w - pane_min);
        let target_pane = pane_w - pane_shrink;
        overflow -= pane_shrink;
        let session_shrink = overflow.min(session_w - session_min);
        let target_session = session_w - session_shrink;
        // Any remaining overflow falls to best-effort clipping.
        (target_session, target_tab, target_pane)
    };

    let session_display = pad_or_truncate(&session_name, target_session);
    let tab_display = pad_or_truncate(&tab_name, target_tab);
    let pane_display = pad_or_truncate(&pane_name, target_pane);

    let mut bar = String::with_capacity(cols + 16);
    let mut chars: usize = 0;
    let mut cells: usize = 0;

    append_segment(&mut bar, &mut chars, &mut cells, prefix);
    let (session_chars_s, session_chars_e, session_cells_s, session_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &session_display);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (tab_chars_s, tab_chars_e, tab_cells_s, tab_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &tab_display);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (pane_chars_s, pane_chars_e, pane_cells_s, pane_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, &pane_display);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (fit_chars_s, fit_chars_e, fit_cells_s, fit_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, fit_icon);
    append_segment(&mut bar, &mut chars, &mut cells, pipe);
    let (typing_chars_s, typing_chars_e, typing_cells_s, typing_cells_e) =
        append_segment(&mut bar, &mut chars, &mut cells, typing_icon);

    // Right-align the hamburger. With successful truncation `pad_cells`
    // collapses to 1 (segments already consumed all available room);
    // with extra slack it absorbs the leftover and pushes the hamburger
    // to the right edge. `.max(1)` still prevents glyph collision in
    // the best-effort case.
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

    // Compose the styled bar. The keyboard and fit icons switch
    // between emphasis-3 (unarmed) and success-colour (armed);
    // both are clear signals against the selected-bar background.
    let mut text = Text::new(&bar)
        .selected()
        .color_range(0, session_chars_s..session_chars_e)
        .color_range(1, tab_chars_s..tab_chars_e)
        .color_range(2, pane_chars_s..pane_chars_e)
        .color_range(3, hamburger_chars_s..hamburger_chars_e);
    text = if state.fit_active {
        text.success_color_range(fit_chars_s..fit_chars_e)
    } else {
        text.color_range(3, fit_chars_s..fit_chars_e)
    };
    text = if state.keyboard.visible {
        text.success_color_range(typing_chars_s..typing_chars_e)
    } else {
        text.color_range(3, typing_chars_s..typing_chars_e)
    };
    print_text_with_coordinates(text, 0, row, Some(cols), None);

    // Tile the entire row across the visible elements so taps on
    // separator/whitespace fall through to the nearest meaningful
    // action. The row is partitioned into 5 contiguous regions by
    // computing midpoints between adjacent label centers; every cell
    // in [0, cols) lands in exactly one region.
    //
    // The "Zellij " prefix to the left of the session name has no
    // associated action, so we fold it into the session region —
    // tapping the prefix is treated as a tap on the session name.
    let session_center = (session_cells_s + session_cells_e) / 2;
    let tab_center = (tab_cells_s + tab_cells_e) / 2;
    let pane_center = (pane_cells_s + pane_cells_e) / 2;
    let fit_center = (fit_cells_s + fit_cells_e) / 2;
    let typing_center = (typing_cells_s + typing_cells_e) / 2;
    let hamburger_center = (hamburger_cells_s + hamburger_cells_e) / 2;
    let mid_session_tab = (session_center + tab_center) / 2;
    let mid_tab_pane = (tab_center + pane_center) / 2;
    let mid_pane_fit = (pane_center + fit_center) / 2;
    let mid_fit_typing = (fit_center + typing_center) / 2;
    let mid_typing_hamburger = (typing_center + hamburger_center) / 2;
    state.click_regions.push(ClickRegion::tight(
        row,
        0,
        mid_session_tab,
        ClickAction::ExpandSessions,
    ));
    state.click_regions.push(ClickRegion::tight(
        row,
        mid_session_tab,
        mid_tab_pane,
        ClickAction::ExpandTabs,
    ));
    state.click_regions.push(ClickRegion::tight(
        row,
        mid_tab_pane,
        mid_pane_fit,
        ClickAction::ExpandPanes,
    ));
    state.click_regions.push(ClickRegion::tight(
        row,
        mid_pane_fit,
        mid_fit_typing,
        ClickAction::ToggleFit,
    ));
    state.click_regions.push(ClickRegion::tight(
        row,
        mid_fit_typing,
        mid_typing_hamburger,
        ClickAction::ToggleKeyboard,
    ));
    state.click_regions.push(ClickRegion::tight(
        row,
        mid_typing_hamburger,
        cols,
        ClickAction::ExpandPanes,
    ));
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
                .session_name
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            0usize,
            "Switch Tab",
        ),
        Selector::Panes => (
            state
                .current_tab()
                .map(|t| t.name.clone())
                .unwrap_or_else(|| "—".to_string()),
            1usize,
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
    state.click_regions.push(ClickRegion::tight(
        row,
        0,
        cols,
        ClickAction::CollapseSelector,
    ));
}

/// One pre-styled cell paired with its visible width. Width is
/// tracked separately because `Text` only exposes the fully-encoded
/// content stream, and the centering logic needs the plain
/// cell-width for column sizing.
struct SelectorCell {
    text: Text,
    width: usize,
}

/// One row in a centered selector table. Holds an arbitrary number
/// of cells so each menu can pick its own column count (Sessions has
/// 2, Tabs has 3, Panes has 3).
struct SelectorRow {
    cells: Vec<SelectorCell>,
    action: ClickAction,
}

/// Render the title + table block centered within
/// `row_start..row_end`. The title is a single-line `Text` coloured
/// with emphasis 3 (per the user-facing spec for switch menus). The
/// table sits one blank row below the title and uses the
/// `print_table_with_coordinates` primitive — each row index `i`
/// (where `i = 0` is the empty header convention used by other
/// built-in plugins) maps deterministically to terminal row
/// `table_y + i`, which is what `register_row_clicks` relies on.
fn render_centered_selector(
    state: &mut State,
    row_start: usize,
    row_end: usize,
    cols: usize,
    title: &str,
    rows: Vec<SelectorRow>,
) {
    let body_height = row_end.saturating_sub(row_start);
    if body_height == 0 || cols == 0 {
        return;
    }

    if rows.is_empty() {
        // Empty list — render only the title, centered vertically and
        // horizontally. Avoids drawing a degenerate one-row table.
        let title_w = UnicodeWidthStr::width(title);
        let title_x = cols.saturating_sub(title_w) / 2;
        let title_y = row_start + body_height.saturating_sub(1) / 2;
        print_text_with_coordinates(
            Text::new(title).color_range(3, ..),
            title_x,
            title_y,
            None,
            None,
        );
        return;
    }

    // Column widths drive both the table-width parameter passed to
    // `print_table_with_coordinates` and the click-region span. The
    // host's table component pads each cell to the column-max and
    // inserts a single space between columns (see
    // `zellij-server/src/ui/components/table.rs`).
    //
    // Quirk: `stringify_table_rows` adds `max_column_width + 1` to its
    // running width for *every* column — including the last — and
    // breaks out the moment that running width exceeds the
    // coordinates' `width`. The actual rendered row, however, omits
    // the trailing pad after the final column. Net: the layout
    // reservation is `sum(col_w) + n_cols`, while the visible row is
    // `sum(col_w) + (n_cols - 1)`. Pass the bigger value so the last
    // column doesn't get clipped; center on the smaller value so the
    // visible row really is centered.
    let n_cols = rows.iter().map(|r| r.cells.len()).max().unwrap_or(0);
    let mut col_widths = vec![0usize; n_cols];
    for row in &rows {
        for (i, cell) in row.cells.iter().enumerate() {
            if cell.width > col_widths[i] {
                col_widths[i] = cell.width;
            }
        }
    }
    let sum_col_w: usize = col_widths.iter().sum();
    let table_layout_w = (sum_col_w + n_cols).min(cols);
    let table_visual_w = (sum_col_w + n_cols.saturating_sub(1)).min(cols);

    // Block layout: title + 1 empty header row + `rows.len()` data
    // rows. If the block exceeds the body, items are truncated to
    // fit; the rest of the layout still centers.
    let max_data_rows = body_height.saturating_sub(2);
    let visible_data_rows = rows.len().min(max_data_rows);
    let block_height = 2 + visible_data_rows;
    let leftover = body_height.saturating_sub(block_height);
    let title_y = row_start + leftover / 2;
    let table_y = title_y + 1;

    // Title — coloured uniformly with emphasis 3, centered to `cols`
    // (not to the table) so the title sits on the screen's vertical
    // axis even if the table is narrow.
    let title_w = UnicodeWidthStr::width(title);
    let title_x = cols.saturating_sub(title_w) / 2;
    print_text_with_coordinates(
        Text::new(title).color_range(3, ..),
        title_x,
        title_y,
        None,
        None,
    );

    let table_x = cols.saturating_sub(table_visual_w) / 2;

    // Convention from the other built-in plugins: row 0 is an empty
    // header row that the host renders with the table-title style.
    // We use it to absorb that styling so our data rows render with
    // the regular cell colours.
    let header_row: Vec<Text> = (0..n_cols).map(|_| Text::new(" ")).collect();
    let mut table = Table::new().add_styled_row(header_row);

    for row in rows.iter().take(visible_data_rows) {
        let cells: Vec<Text> = row.cells.iter().map(|c| c.text.clone()).collect();
        table = table.add_styled_row(cells);
    }

    print_table_with_coordinates(
        table,
        table_x,
        table_y,
        Some(table_layout_w),
        Some(visible_data_rows + 1),
    );

    // Click region per visible item. The header sits at `table_y`;
    // item `i` lands at `table_y + 1 + i`. Spans the visible table
    // width so a tap anywhere on the row hits.
    for (i, row) in rows.iter().take(visible_data_rows).enumerate() {
        state.click_regions.push(ClickRegion::tight(
            table_y + 1 + i,
            table_x,
            table_x + table_visual_w,
            row.action.clone(),
        ));
    }
}

/// Build a `SelectorCell` whose text is `text` and whose only
/// emphasis is the digit run starting at `digits_start` of length
/// `digit_count` painted at `digit_color`. The rest of the cell
/// renders with the table's default cell foreground (no emphasis
/// level applied), which keeps surrounding labels (e.g. "panes",
/// "tabs") visually neutral while the count itself stays vivid.
fn count_cell(text: String, digits_start: usize, digit_count: usize, digit_color: usize) -> SelectorCell {
    let width = UnicodeWidthStr::width(text.as_str());
    let mut t = Text::new(&text);
    if digit_count > 0 {
        t = t.color_range(digit_color, digits_start..digits_start + digit_count);
    }
    SelectorCell { text: t, width }
}

/// Build a neutral cell for the last-activity column: no emphasis
/// colour and unbold (the table component bolds every cell by
/// default; `unbold_all` flips that off via the level-5 mechanism in
/// `zellij-server/src/ui/components/text.rs::is_unbold_at`).
fn activity_cell(text: String) -> SelectorCell {
    let width = UnicodeWidthStr::width(text.as_str());
    let t = Text::new(&text).unbold_all();
    SelectorCell { text: t, width }
}

/// Cell carrying a plain entity name in the supplied emphasis
/// colour. Used for session / tab / pane name cells.
fn named_cell(text: String, color: usize) -> SelectorCell {
    let width = UnicodeWidthStr::width(text.as_str());
    let t = Text::new(&text).color_range(color, ..);
    SelectorCell { text: t, width }
}

/// Most recent activity stamp across `tab_position`'s panes, used
/// for the Tabs menu's third column. `None` when no pane in the tab
/// has been mentioned in any `PaneRenderReportWithAnsi` yet (true
/// right after attach until the first delta arrives).
fn tab_last_activity(state: &State, tab_position: usize) -> Option<u64> {
    state
        .panes_for_tab(tab_position)
        .into_iter()
        .filter_map(|p| state.pane_last_activity.get(&pane_id_of(p)).copied())
        .max()
}

/// Sessions selector. Three rows total: name (color 0), tab count
/// (digits in color 1), pane count (digits in color 2). Per the
/// spec only the digits are coloured — the trailing word stays in
/// the table-cell base colour.
fn render_sessions_menu(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let mut entries: Vec<(String, usize, usize, bool)> = state
        .sessions
        .iter()
        .map(|s| {
            let pane_count: usize = s
                .panes
                .panes
                .values()
                .map(|panes| {
                    panes
                        .iter()
                        .filter(|p| p.is_selectable && !p.is_suppressed)
                        .count()
                })
                .sum();
            (
                s.name.clone(),
                s.tabs.len(),
                pane_count,
                s.is_current_session,
            )
        })
        .collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let rows: Vec<SelectorRow> = entries
        .into_iter()
        .map(|(name, tabs, panes, is_current)| {
            let name_label = if is_current {
                format!("{} (current)", name)
            } else {
                name.clone()
            };

            let tabs_text = format!("{} tabs", tabs);
            let tabs_digits = tabs.to_string().chars().count();
            let tabs_cell = count_cell(tabs_text, 0, tabs_digits, 1);

            let panes_text = format!("{} panes", panes);
            let panes_digits = panes.to_string().chars().count();
            let panes_cell = count_cell(panes_text, 0, panes_digits, 2);

            SelectorRow {
                cells: vec![named_cell(name_label, 0), tabs_cell, panes_cell],
                action: ClickAction::SelectSession(name),
            }
        })
        .collect();

    render_centered_selector(state, row_start, row_end, cols, "Switch Session", rows);
}

/// Tabs selector. Columns: name (color 1), pane count (digits in
/// color 2), last activity (neutral / unbold). Last activity for a
/// tab is the max activity stamp across that tab's panes.
fn render_tabs_menu(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let tabs: Vec<TabInfo> = state.tabs_in_order().into_iter().cloned().collect();
    let now = unix_now();
    let rows: Vec<SelectorRow> = tabs
        .into_iter()
        .map(|tab| {
            let pane_count =
                tab.selectable_tiled_panes_count + tab.selectable_floating_panes_count;
            let panes_text = format!("{} panes", pane_count);
            let panes_digits = pane_count.to_string().chars().count();
            let panes_cell = count_cell(panes_text, 0, panes_digits, 2);

            let last_activity = tab_last_activity(state, tab.position);
            let activity_text = format_time_ago(last_activity, now);

            SelectorRow {
                cells: vec![
                    named_cell(tab.name.clone(), 1),
                    panes_cell,
                    activity_cell(activity_text),
                ],
                action: ClickAction::SelectTab(tab.position),
            }
        })
        .collect();

    render_centered_selector(state, row_start, row_end, cols, "Switch Tab", rows);
}

/// Panes selector. Lists panes across **every** visible tab so the
/// "tab" column carries useful disambiguation. Columns: pane title
/// (color 2), tab name (color 1), last activity (neutral / unbold).
fn render_panes_menu(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let now = unix_now();
    let tabs: Vec<TabInfo> = state.tabs_in_order().into_iter().cloned().collect();
    let mut rows: Vec<SelectorRow> = Vec::new();
    for tab in tabs {
        let panes: Vec<PaneInfo> = state.panes_for_tab(tab.position).into_iter().cloned().collect();
        for pane in panes {
            let id = pane_id_of(&pane);
            let title = if pane.title.is_empty() {
                format!("#{}", pane.id)
            } else {
                pane.title.clone()
            };

            let last_activity = state.pane_last_activity.get(&id).copied();
            let activity_text = format_time_ago(last_activity, now);

            rows.push(SelectorRow {
                cells: vec![
                    named_cell(title, 2),
                    named_cell(tab.name.clone(), 1),
                    activity_cell(activity_text),
                ],
                action: ClickAction::SelectPane {
                    tab_position: tab.position,
                    pane_id: id,
                },
            });
        }
    }

    render_centered_selector(state, row_start, row_end, cols, "Switch Pane", rows);
}

/// Format a timestamp as `Active <time> ago`, relative to `now`.
/// Returns `"—"` when no activity has been recorded yet (the cache
/// is delta-only, so a freshly-attached client sees `None` for any
/// pane that has not redrawn since attach). The "Active" prefix is
/// dropped in that case because `"Active —"` reads awkwardly.
fn format_time_ago(then_unix_secs: Option<u64>, now_unix_secs: u64) -> String {
    let Some(then) = then_unix_secs else {
        return "—".to_string();
    };
    let diff = now_unix_secs.saturating_sub(then);
    let body = if diff < 5 {
        "just now".to_string()
    } else if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    };
    format!("Active {}", body)
}

/// Pad `text` with trailing spaces or truncate (with `…`) so its cell
/// width is exactly `width`. Width 0 returns empty.
fn pad_or_truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let text_w = UnicodeWidthStr::width(text);
    if text_w == width {
        return text.to_string();
    }
    if text_w < width {
        let mut s = text.to_string();
        for _ in 0..(width - text_w) {
            s.push(' ');
        }
        return s;
    }
    // Truncate. Reserve 1 cell for the ellipsis if width >= 2.
    if width == 1 {
        // Just take the first char's worth.
        let mut out = String::new();
        for ch in text.chars() {
            let mut tmp = [0u8; 4];
            let s = ch.encode_utf8(&mut tmp);
            if UnicodeWidthStr::width(s as &str) <= 1 {
                out.push(ch);
                break;
            }
        }
        if out.is_empty() {
            out.push(' ');
        }
        return out;
    }
    let mut out = String::new();
    let mut taken = 0;
    let target = width - 1; // leave room for the ellipsis
    for ch in text.chars() {
        let mut tmp = [0u8; 4];
        let s = ch.encode_utf8(&mut tmp);
        let w = UnicodeWidthStr::width(s as &str);
        if taken + w > target {
            break;
        }
        out.push(ch);
        taken += w;
    }
    out.push('…');
    let out_w = UnicodeWidthStr::width(out.as_str());
    if out_w < width {
        for _ in 0..(width - out_w) {
            out.push(' ');
        }
    }
    out
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
    // and most-recent terminal output live. `viewport_v_pan` shifts
    // that slice upward (toward older content). Clamp here so a stale
    // pan offset survives viewport-length changes without flipping
    // into negative territory or pinning the user past the new top.
    let max_v_pan = viewport_lines.len().saturating_sub(height);
    state.viewport_v_pan = state.viewport_v_pan.min(max_v_pan);
    let skip = max_v_pan - state.viewport_v_pan;
    // Horizontal pan: anchor the slice to col 0 by default, and let
    // `viewport_h_pan` walk it to the right. Use `pane_content_columns`
    // from the most recent `PaneInfo` as the authoritative width — that
    // is the visible cell count of the pane's grid. Fall back to the
    // widest cached line when no `PaneInfo` is available (transient
    // race during pane teardown).
    let pane_width = pane
        .as_ref()
        .map(|p| p.pane_content_columns)
        .filter(|&w| w > 0)
        .unwrap_or_else(|| {
            viewport_lines
                .iter()
                .map(|l| visible_width(l))
                .max()
                .unwrap_or(0)
        });
    let max_h_pan = pane_width.saturating_sub(cols);
    state.viewport_h_pan = state.viewport_h_pan.min(max_h_pan);
    let h_offset = state.viewport_h_pan;

    // Record where the viewport landed so the mouse handler can
    // reverse-map clicks into pane coordinates. We store this even when
    // we have no cached lines yet, so the user's first viewport tap
    // still maps to row 0 of an eventually-populated cache.
    state.viewport_region = Some(ViewportRegion {
        row_start,
        row_end,
        cols,
        skip,
        h_offset,
    });

    // If Fit is active, the server's tab-size override should track
    // the embedded viewport area: keyboard toggles, rotation, and
    // pinch-zoom all change the embedded area's dimensions, and the
    // pane must follow or the user is back to panning. We can't
    // call `update_fit_size` directly here — see the doc on
    // `fit_pending_target`. Instead, stash the target for the next
    // `update()` to flush. The diff against `fit_last_sent_size`
    // (also done in update) avoids a feedback loop where the
    // server's resize triggers a fresh `PaneRenderReportWithAnsi`,
    // which triggers another render, which would re-send the same
    // size, ad infinitum.
    if state.fit_active {
        if let (Some(pane), Some(tab)) =
            (state.current_pane(), state.current_tab().cloned())
        {
            let region = state.viewport_region.unwrap(); // just assigned
            let target = crate::fit_target_tab_size(&pane, &tab, &region);
            state.fit_pending_target = Some(target);
        }
    } else {
        state.fit_pending_target = None;
    }

    // Disable autowrap (DECAWM, `\x1b[?7l`) for the duration of the
    // viewport emit. The cached viewport lines come from the
    // *underlying* pane's grid — that pane may be wider than our
    // embedded area, so its rendered lines can carry more visible
    // cells than our `cols`. Without DECAWM off, a line that
    // overflows the right edge wraps to the next row; on the very
    // last row of our render area that wrap forces the host's
    // plugin-pane grid to scroll, which silently pushes the chrome
    // (top bar at row 0) off-screen. With DECAWM off the host's
    // `Grid::add_character` (`zellij-server/src/panes/grid.rs:1925`)
    // simply drops anything past the right edge — which is exactly
    // what we want for a cropped embedded view.
    print!("\x1b[?7l");

    // Reset before each row to keep the chrome's styling separate from
    // the pane's emitted SGR runs. When `h_offset` is non-zero the
    // slicer trims each line down to the visible window before
    // emission, so DECAWM-off is still doing the same job — dropping
    // overflow past `cols` cells from a now-trimmed string.
    for i in 0..height {
        let row = row_start + i;
        print!("{}{}", RESET, move_to(row, 0));
        if let Some(line) = viewport_lines.get(skip + i) {
            if h_offset == 0 {
                // Fast path: no horizontal pan, no slicing needed —
                // trust the ANSI; xterm style resets at end of pane
                // line are already part of the rendered stream.
                print!("{}", line);
            } else {
                let sliced = slice_ansi_visible(line, h_offset, cols);
                print!("{}", sliced);
            }
        } else if i == 0 && pane_id.is_none() {
            print!("{}(no pane selected)", RESET);
        } else if i == 0 && viewport_lines.is_empty() {
            print!("{}(awaiting first render…)", RESET);
        }
        // Clear any overrun from the previous frame. When the user
        // is panned right, only the slice of the line past `h_offset`
        // contributes to the rendered width — anything to the left of
        // `h_offset` was trimmed by the slicer and never emitted.
        let raw_width = viewport_lines
            .get(skip + i)
            .map(|l| visible_width(l))
            .unwrap_or(0);
        let printed_width = raw_width.saturating_sub(h_offset).min(cols);
        if printed_width < cols {
            print!("{}\x1b[K", RESET);
        }
    }

    // Restore autowrap before the function returns so subsequent
    // chrome rendering on later frames is unaffected.
    print!("\x1b[?7h");
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

/// Slice `line` so the output, when emitted at column 0, renders the
/// same visible cells that the original would have rendered at columns
/// `[h_offset, h_offset + max_cols)`. ANSI escape sequences are
/// preserved verbatim so style state propagates correctly into the
/// visible window. Wide characters straddling the left boundary are
/// replaced with a single space placeholder so the rest of the line
/// stays column-aligned; wide characters straddling the right boundary
/// are dropped entirely (caller pads with `\x1b[K`).
///
/// A trailing `RESET` is appended so any open SGR run does not bleed
/// into the next row's chrome.
pub(crate) fn slice_ansi_visible(line: &str, h_offset: usize, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let mut out = String::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut cell_index = 0usize;
    let right_edge = h_offset.saturating_add(max_cols);
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Emit the entire escape sequence regardless of where the
            // visible cell cursor sits — escapes cost zero visible
            // cells, and replaying every escape we have walked past
            // means the first visible cell inside the slice arrives
            // with the correct SGR state.
            let start = i;
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                // CSI: ESC [ <params> <final byte in 0x40..=0x7E>
                i += 1;
                while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            } else if i < bytes.len() && bytes[i] == b']' {
                // OSC: ESC ] <body> BEL | ESC ] <body> ESC \
                i += 1;
                while i < bytes.len()
                    && bytes[i] != 0x07
                    && !(bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\')
                {
                    i += 1;
                }
                if i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                    } else if bytes[i] == 0x1b {
                        i += 2.min(bytes.len() - i);
                    }
                }
            } else if i < bytes.len() {
                // Two-byte ESC sequence (rare in our viewport but
                // walked over so we don't desynchronise on stray
                // ESC + letter).
                i += 1;
            }
            // Safe because the walker only advances on valid UTF-8
            // boundaries inside the escape body (ASCII control range).
            if let Ok(esc) = std::str::from_utf8(&bytes[start..i]) {
                out.push_str(esc);
            }
            continue;
        }
        let ch_len = utf8_char_len(bytes[i]).max(1);
        let end = (i + ch_len).min(bytes.len());
        let ch_bytes = &bytes[i..end];
        let ch_str = match std::str::from_utf8(ch_bytes) {
            Ok(s) => s,
            Err(_) => {
                i = end;
                continue;
            },
        };
        let w = UnicodeWidthStr::width(ch_str);
        if w == 0 {
            // Zero-width chars (e.g. combining marks) ride along with
            // the previous cell if any visible content has been
            // emitted; otherwise they are dropped to avoid orphan
            // marks at the start of the slice.
            if cell_index > h_offset && cell_index <= right_edge && !out.is_empty() {
                out.push_str(ch_str);
            }
            i = end;
            continue;
        }
        if cell_index + w <= h_offset {
            // Still left of the visible window.
            cell_index += w;
            i = end;
            continue;
        }
        if cell_index < h_offset {
            // Wide char straddling the left boundary. Emit a single
            // space to preserve column alignment for the cells that
            // follow.
            out.push(' ');
            cell_index += w;
            i = end;
            continue;
        }
        if cell_index >= right_edge {
            break;
        }
        if cell_index + w > right_edge {
            // Wide char straddling the right boundary: drop it. The
            // caller pads with `\x1b[K`, so leaving the cell blank
            // here is correct.
            break;
        }
        out.push_str(ch_str);
        cell_index += w;
        i = end;
    }
    out.push_str(RESET);
    out
}

#[cfg(test)]
mod tests {
    //! Tests for the horizontal slicer used by the embedded viewport.
    //!
    //! Each test pins one concern: ASCII window math, escape preservation,
    //! wide-character handling at boundaries, and the empty / oversize
    //! cases. The slicer always appends a trailing `\x1b[0m` so the next
    //! row's chrome cannot inherit a stale SGR run.
    use super::*;

    /// Trailing `RESET` is part of the contract; tests assert the visible
    /// portion separately.
    fn visible(s: &str) -> &str {
        s.strip_suffix(RESET).unwrap_or(s)
    }

    #[test]
    fn ascii_slice_inside_line() {
        let line = "abcdefghij";
        let sliced = slice_ansi_visible(line, 2, 4);
        assert_eq!(visible(&sliced), "cdef");
    }

    #[test]
    fn ascii_slice_at_left_edge() {
        let line = "abcdefghij";
        let sliced = slice_ansi_visible(line, 0, 4);
        assert_eq!(visible(&sliced), "abcd");
    }

    #[test]
    fn ascii_slice_past_right_edge() {
        let line = "abcd";
        let sliced = slice_ansi_visible(line, 1, 10);
        // Visible portion is the rest of the line; padding is the
        // caller's job (via \x1b[K).
        assert_eq!(visible(&sliced), "bcd");
    }

    #[test]
    fn empty_when_offset_past_line_width() {
        let line = "abcd";
        let sliced = slice_ansi_visible(line, 10, 4);
        assert_eq!(visible(&sliced), "");
        assert!(sliced.ends_with(RESET));
    }

    #[test]
    fn max_cols_zero_returns_empty() {
        let sliced = slice_ansi_visible("abcd", 0, 0);
        assert_eq!(sliced, "");
    }

    #[test]
    fn ansi_escape_preserved_when_in_window() {
        let line = "\x1b[31mred\x1b[0m end";
        let sliced = slice_ansi_visible(line, 0, 7);
        // Visible cells: "red end" — escapes ride along verbatim.
        assert!(sliced.contains("\x1b[31m"));
        assert!(sliced.contains("\x1b[0m"));
        assert!(sliced.contains("red"));
        assert!(sliced.contains("end"));
    }

    #[test]
    fn ansi_escape_replayed_when_offset_skips_text() {
        // The slicer must emit every escape it walked past so the
        // first visible cell renders with the correct SGR state.
        let line = "\x1b[31maaaa\x1b[32mbbbb";
        let sliced = slice_ansi_visible(line, 4, 4);
        // Window covers the four 'b' cells; the red escape was walked
        // past but should still appear, followed by the green escape
        // that styles the visible region.
        assert!(sliced.contains("\x1b[31m"));
        assert!(sliced.contains("\x1b[32m"));
        // Visible payload is "bbbb" (plus the final RESET).
        assert!(sliced.contains("bbbb"));
        assert!(!sliced.contains("aaaa"));
    }

    #[test]
    fn wide_char_straddling_left_boundary_becomes_space() {
        // A CJK char "中" is 2 cells wide. Place it so its left half
        // is at cell 0 and the slice starts at cell 1.
        let line = "中abc";
        let sliced = slice_ansi_visible(line, 1, 3);
        // Cell 1 (right half of the wide char) becomes a space; then
        // 'a' and 'b' fill cells 2 and 3.
        assert_eq!(visible(&sliced), " ab");
    }

    #[test]
    fn wide_char_straddling_right_boundary_dropped() {
        // Window covers cells [0, 3); "中" spans cells 2..=3 so its
        // right half falls outside the window. The whole char is
        // dropped — the caller pads with \x1b[K.
        let line = "ab中cd";
        let sliced = slice_ansi_visible(line, 0, 3);
        assert_eq!(visible(&sliced), "ab");
    }

    #[test]
    fn wide_char_entirely_inside_window() {
        let line = "ab中cd";
        let sliced = slice_ansi_visible(line, 0, 4);
        assert_eq!(visible(&sliced), "ab中");
    }
}

