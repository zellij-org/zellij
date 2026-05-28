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

use crate::modifier_bar;
use crate::unix_now;
use crate::state::{
    pane_id_of, ClickAction, ClickRegion, LastEmittedCursor, Selector, State,
    ViewportRegion,
};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
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

    // Top bar normally sits at row 0; the body fills the remaining
    // rows. The bottom modifier bar reserves one row at the bottom of
    // the plugin area, just above where the OS soft keyboard surfaces.
    // The reservation is gated on the OS keyboard being visible —
    // when the user dismisses the keyboard (Android back button,
    // 2-finger toggle, etc.) the bar disappears and its row is freed
    // for viewport content. On a pathologically short plugin area
    // (1-2 rows of body) the bar is suppressed so the viewport keeps
    // a usable row.
    //
    // Welcome flow: while the mobile plugin is hosting the welcome
    // experience (`welcome_auto_expand_done` is sticky and stays true
    // for the lifetime of the welcome session), the top bar is
    // suppressed entirely so the centered welcome UI uses the full
    // plugin height. The session this plugin hosts is going away the
    // moment the user attaches/creates, so there's no useful pane/tab
    // chrome to expose anyway.
    let in_welcome_flow = state.welcome_auto_expand_done;
    let body_top = if in_welcome_flow { 0 } else { 1 };
    let bar_height = if state.soft_keyboard_visible && rows.saturating_sub(body_top) >= 2 {
        1
    } else {
        0
    };
    let body_bottom = rows.saturating_sub(bar_height);
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

    if !in_welcome_flow {
        render_top_bar(state, 0, cols);
    }

    if body_bottom > body_top {
        match state.expanded {
            None => render_embedded_viewport(state, body_top, body_bottom, cols),
            Some(Selector::Sessions) => {
                render_sessions_menu(state, body_top, body_bottom, cols)
            },
            Some(Selector::Panes) => render_panes_menu(state, body_top, body_bottom, cols),
            Some(Selector::NewSessionPrompt) => {
                render_new_session_prompt(state, body_top, body_bottom, cols)
            },
        }
    }

    // The dropdown menu paints AFTER the embedded viewport so the
    // menu's cells overwrite the viewport's right-edge cells where
    // the two overlap (the viewport uses raw `print!` and would
    // otherwise overwrite the menu). Gated on `expanded.is_none()`
    // because the selectors occupy the body entirely; the menu would
    // overlay a list of session/tab/pane rows with no purpose. The
    // menu also truncates its rows to fit within `[body_top,
    // body_bottom)` so its click regions never overlap the
    // keyboard's tight regions (which would otherwise win on first-
    // hit and block keyboard taps under the menu).
    if state.menu_open && state.expanded.is_none() && body_bottom > body_top {
        render_hamburger_menu(state, body_top, body_bottom, cols);
    }

    if bar_height > 0 {
        // `state.ctrl_held` / `state.alt_held` are the canonical
        // one-shot modifier flags — `Event::Key` (hardware keyboard)
        // clears them without touching the controller's internal
        // `modifiers` mirror, so reading directly from State avoids a
        // stale-armed-emphasis bug after `arm-then-hardware-key` flows.
        let armed = modifier_bar::KeyboardModifiers {
            ctrl_armed: state.ctrl_held,
            alt_armed: state.alt_held,
        };
        modifier_bar::render_modifier_bar(
            &armed,
            body_bottom,
            cols,
            &mut state.click_regions,
        );
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

/// Top bar: `Zellij <pane>` left-aligned with `☰` right-aligned.
/// Rendered as a single `Text` component with `.selected()` and a
/// width covering the entire row, so:
/// - The pane segment's foreground colour comes from the host's
///   selected emphasis-2 palette (`text_selected.emphasis_2`) via
///   `color_range`. See `style_of_index` in
///   `zellij-server/src/ui/components/text.rs`.
/// - The whole row is painted with `text_selected.background`, which
///   on the standard Zellij themes is the lighter-gray "selection"
///   shade — distinct from the embedded pane content below.
/// - The "Zellij " prefix inherits the selected-bar foreground (no
///   `color_range` applied) so it reads as chrome rather than data.
///
/// The hamburger glyph (`☰`) toggles `state.menu_open`. The dropdown
/// menu it opens contains the toggles for the on-screen keyboard,
/// Fit-to-Screen, and the three Change-X navigation items — see
/// `render_hamburger_menu`. Tapping the prefix or pane name still
/// opens the Panes selector directly (existing behaviour
/// preserved).
fn render_top_bar(state: &mut State, row: usize, cols: usize) {
    if cols == 0 {
        return;
    }
    // Identical layout in every screen — collapsed viewport, panes
    // selector, sessions selector, and dropdown menu all share this
    // bar. The pane name shown is the currently-selected pane (the
    // one the embedded viewport reads), even while a selector is
    // open, so the user always sees what they would return to.
    render_top_bar_collapsed(state, row, cols);
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

/// Number of cells reserved to the *left* of the hamburger glyph as
/// a slop halo. The visible glyph is just one cell — at touch-target
/// scale that's nearly impossible to hit — so the layout always
/// keeps this many cells of pad between the rendered pane title and
/// the glyph, and registers a slop click region (priority 1)
/// covering the pad. Taps that miss the glyph but land on any of
/// those pad cells still toggle the menu.
const HAMBURGER_SLOP_CELLS: usize = 3;

/// Collapsed top bar: `"Zellij <session> <pane>"` left-aligned, `☰`
/// right-aligned. The session segment is painted with emphasis-0 and
/// the pane segment with emphasis-2; the `"Zellij "` prefix is
/// chrome (no `color_range`). Truncation is applied in priority
/// order:
/// 1. Natural widths fit → render `"Zellij <session> <pane>"`.
/// 2. Natural widths overflow → drop the `"Zellij "` prefix and try
///    `"<session> <pane>"` at natural widths.
/// 3. Still overflows → keep both names visible but split the
///    available cells. Each side gets at least half; if one name is
///    shorter than half, the other absorbs the slack.
/// When no session name is known the bar falls back to
/// `"Zellij <pane>"` (original behaviour).
///
/// The hamburger always stays visible with a slop halo on its left
/// so the tap target is generous.
///
/// Click behaviour depends on whether a selector is currently open:
/// - **Collapsed (no selector)**: tap on the prefix/session/pane
///   title opens the Panes selector (`ExpandPanes`).
/// - **In a selector**: tap on the prefix/session/pane title
///   collapses the selector and returns to the viewport
///   (`CollapseSelector`) — matches the existing selector escape-tap
///   gesture so the identical-looking top bar offers a one-tap way
///   home from Change Pane / Change Session.
///
/// The hamburger glyph itself (tight) and the pad to its left
/// (slop) always toggle the dropdown menu in either state.
fn render_top_bar_collapsed(state: &mut State, row: usize, cols: usize) {
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
    let session_name = state.session_name.clone();

    let prefix = "Zellij ";
    let hamburger = "\u{2630}"; // ☰

    let prefix_w = UnicodeWidthStr::width(prefix);
    let hamburger_w = UnicodeWidthStr::width(hamburger);
    // Total cells available to the left of the hamburger glyph and
    // its mandatory slop halo.
    let content_max = cols.saturating_sub(HAMBURGER_SLOP_CELLS + hamburger_w);

    let session_w_natural = session_name
        .as_ref()
        .map(|s| UnicodeWidthStr::width(s.as_str()))
        .unwrap_or(0);
    let pane_w_natural = UnicodeWidthStr::width(pane_name.as_str());

    // Pick a layout. Priority: "Zellij <session> <pane>" at natural
    // widths → "<session> <pane>" at natural widths → split the
    // content area between the two names, with a single separator
    // cell. With no session_name, fall back to "Zellij <pane>" —
    // the original behaviour.
    let sep_w: usize = 1;
    let (show_prefix, session_target, pane_target): (bool, usize, usize) =
        if session_name.is_some() {
            let with_prefix_w = prefix_w + session_w_natural + sep_w + pane_w_natural;
            if with_prefix_w <= content_max {
                (true, session_w_natural, pane_w_natural)
            } else {
                let without_prefix_w = session_w_natural + sep_w + pane_w_natural;
                if without_prefix_w <= content_max {
                    (false, session_w_natural, pane_w_natural)
                } else {
                    let available = content_max.saturating_sub(sep_w);
                    let half = available / 2;
                    let (s_t, p_t) = if session_w_natural <= half {
                        (session_w_natural, available.saturating_sub(session_w_natural))
                    } else if pane_w_natural <= half {
                        (available.saturating_sub(pane_w_natural), pane_w_natural)
                    } else {
                        (half, available.saturating_sub(half))
                    };
                    (false, s_t, p_t)
                }
            }
        } else {
            let pane_max = content_max.saturating_sub(prefix_w);
            (true, 0, pane_w_natural.min(pane_max))
        };

    let mut bar = String::with_capacity(cols + 16);
    let mut chars: usize = 0;
    let mut cells: usize = 0;

    if show_prefix {
        append_segment(&mut bar, &mut chars, &mut cells, prefix);
    }

    let (session_chars_s, session_chars_e, session_cells_range) =
        if let (Some(session), true) = (session_name.as_ref(), session_target > 0) {
            let session_display = pad_or_truncate(session, session_target);
            let (cs, ce, cell_s, cell_e) =
                append_segment(&mut bar, &mut chars, &mut cells, &session_display);
            // Separator cell between session and pane. The session
            // click region intentionally stops at the end of the
            // session text — the separator space falls into the pane
            // region so the click target boundaries line up with what
            // the user sees as session text vs. pane text.
            append_segment(&mut bar, &mut chars, &mut cells, " ");
            (cs, ce, Some((cell_s, cell_e)))
        } else {
            (0, 0, None)
        };

    let pane_display = pad_or_truncate(&pane_name, pane_target);
    let (pane_chars_s, pane_chars_e, _, _) =
        append_segment(&mut bar, &mut chars, &mut cells, &pane_display);
    // The pane tight click region ends here — at the right edge of
    // the rendered prefix + session + pane text. Anything to the
    // right is either pad (slop catches it) or the hamburger glyph
    // itself.
    let pane_tight_end_cell = cells;

    // Pad with spaces so the hamburger sits at the right edge. The
    // `content_max` reservation guarantees at least
    // HAMBURGER_SLOP_CELLS pad cells when the names are at max
    // width; shorter names produce more pad, which expands the slop
    // halo.
    let pad_cells = cols
        .saturating_sub(cells + hamburger_w)
        .max(HAMBURGER_SLOP_CELLS);
    for _ in 0..pad_cells {
        bar.push(' ');
    }
    chars += pad_cells;
    cells += pad_cells;

    let hamburger_start_cell = cells;
    let (hamburger_chars_s, hamburger_chars_e, _, _) =
        append_segment(&mut bar, &mut chars, &mut cells, hamburger);

    // Compose the styled bar. The "Zellij " prefix takes no
    // color_range — it inherits the selected-bar foreground so it
    // reads as chrome rather than data. The session name uses
    // emphasis-0, the pane title uses emphasis-2, and the hamburger
    // uses emphasis-3.
    let mut text = Text::new(&bar)
        .selected()
        .color_range(2, pane_chars_s..pane_chars_e)
        .color_range(3, hamburger_chars_s..hamburger_chars_e);
    if session_chars_e > session_chars_s {
        text = text.color_range(0, session_chars_s..session_chars_e);
    }
    print_text_with_coordinates(text, 0, row, Some(cols), None);

    // Context-sensitive actions: in selector mode both the pane and
    // the session sub-regions act as escape hatches back to the
    // viewport. In collapsed mode each opens its respective
    // selector.
    let (pane_action, session_action) = if state.expanded.is_some() {
        (ClickAction::CollapseSelector, ClickAction::CollapseSelector)
    } else {
        (ClickAction::ExpandPanes, ClickAction::ExpandSessions)
    };

    for region in top_bar_collapsed_click_regions(
        row,
        cols,
        pane_tight_end_cell,
        hamburger_start_cell,
        pane_action,
        session_cells_range,
        session_action,
    ) {
        state.click_regions.push(region);
    }
}

/// Compute the click regions for the simplified collapsed top bar.
///
/// The left content area `[0, pane_tight_end)` is partitioned into
/// up to three tight sub-regions:
/// - When `session_cells = Some((s, e))` the area splits into
///   `[0, s)` → `pane_action`, `[s, e)` → `session_action`,
///   `[e, pane_tight_end)` → `pane_action`. Zero-width slices are
///   skipped.
/// - When `session_cells = None` the entire `[0, pane_tight_end)`
///   range is a single `pane_action` region (original behaviour).
///
/// Additionally:
/// - **Tight hamburger** `[hamburger_tight_start, cols)` — just the
///   visible glyph. Fires `ToggleMenu`.
/// - **Slop hamburger** `[pane_tight_end, cols)` priority 1, centered
///   on the glyph — covers the pad cells between the title and the
///   glyph. Tapping any of these cells (which look like empty
///   spacing) also fires `ToggleMenu`, giving the small one-cell
///   glyph a generous tap halo.
///
/// `pane_action`/`session_action` are typically `ExpandPanes` /
/// `ExpandSessions` in collapsed mode and both `CollapseSelector`
/// in selector mode — matching the legacy "tap the top bar to
/// escape any open selector" gesture.
///
/// The slop region overlaps the tight hamburger region, but tight
/// wins on pass 1, so the overlap is harmless: cells in
/// `[hamburger_tight_start, cols)` resolve to tight hamburger; cells
/// in `[pane_tight_end, hamburger_tight_start)` (the pad) fall to
/// slop on pass 2. Pure / shim-free so the partition can be
/// exercised from `mod tests`.
pub fn top_bar_collapsed_click_regions(
    row: usize,
    cols: usize,
    pane_tight_end: usize,
    hamburger_tight_start: usize,
    pane_action: ClickAction,
    session_cells: Option<(usize, usize)>,
    session_action: ClickAction,
) -> Vec<ClickRegion> {
    let hamburger_center = (
        hamburger_tight_start.min(cols.saturating_sub(1)),
        row,
    );

    let mut regions: Vec<ClickRegion> = Vec::with_capacity(5);

    // Partition the left content area. The optional session split
    // carves out a middle range; otherwise the entire span is a
    // single pane region. Zero-width slices are filtered out so the
    // hit tester never sees empty ranges.
    if let Some((s, e)) = session_cells {
        let s = s.min(pane_tight_end);
        let e = e.min(pane_tight_end);
        if s < e {
            if s > 0 {
                regions.push(ClickRegion::tight(row, 0, s, pane_action.clone()));
            }
            regions.push(ClickRegion::tight(row, s, e, session_action));
            if e < pane_tight_end {
                regions.push(ClickRegion::tight(
                    row,
                    e,
                    pane_tight_end,
                    pane_action.clone(),
                ));
            }
        } else if pane_tight_end > 0 {
            regions.push(ClickRegion::tight(row, 0, pane_tight_end, pane_action.clone()));
        }
    } else if pane_tight_end > 0 {
        regions.push(ClickRegion::tight(row, 0, pane_tight_end, pane_action.clone()));
    }

    regions.push(ClickRegion::tight(
        row,
        hamburger_tight_start,
        cols,
        ClickAction::ToggleMenu,
    ));
    regions.push(ClickRegion::slop(
        row,
        pane_tight_end,
        cols,
        ClickAction::ToggleMenu,
        hamburger_center,
    ));

    regions
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

    // Block layout: title + 1 empty header row + visible data rows.
    // Once the list outgrows the body the block anchors at the top
    // (no vertical centering) so scrolling has a stable reference;
    // shorter lists keep the original vertical centering for the
    // empty-screen feel.
    let max_data_rows = body_height.saturating_sub(2);
    let max_offset = rows.len().saturating_sub(max_data_rows);
    let offset = state.selector_scroll_offset.min(max_offset);
    state.selector_scroll_offset = offset;

    let visible_data_rows = rows.len().saturating_sub(offset).min(max_data_rows);
    let needs_scroll = rows.len() > max_data_rows;
    let title_y = if needs_scroll {
        row_start
    } else {
        let block_height = 2 + visible_data_rows;
        let leftover = body_height.saturating_sub(block_height);
        row_start + leftover / 2
    };
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

    let visible_rows: Vec<&SelectorRow> =
        rows.iter().skip(offset).take(visible_data_rows).collect();
    for row in &visible_rows {
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
    for (i, row) in visible_rows.iter().enumerate() {
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

/// Cell carrying a plain entity name in the supplied emphasis
/// colour. Used for session name cells in the Sessions selector.
fn named_cell(text: String, color: usize) -> SelectorCell {
    let width = UnicodeWidthStr::width(text.as_str());
    let t = Text::new(&text).color_range(color, ..);
    SelectorCell { text: t, width }
}

/// Sessions selector. Three rows total: name (color 0), tab count
/// (digits in color 1), pane count (digits in color 2). Per the
/// spec only the digits are coloured — the trailing word stays in
/// the table-cell base colour.
///
/// When the mobile plugin is hosting the welcome flow, the rendering
/// switches to `render_welcome_sessions` which uses a custom layout:
/// an unstyled "Hi from Zellij!" title at the top, two-line session
/// cards (name + tabs/panes/clients counts), and a "+ New Session"
/// affordance pinned at the bottom — outside the scrollable list so
/// it stays visible regardless of session count or scroll position.
///
/// Welcome mode is detected via `state.welcome_auto_expand_done`
/// rather than `state.current_pane_is_welcome()`: the welcome pane
/// is closed by `Event::PaneUpdate` immediately after auto-expand
/// (see `main.rs`), so the `current_pane` check flips to `false`
/// within a frame or two, while the sticky `welcome_auto_expand_done`
/// flag stays true for the lifetime of the mobile plugin's host
/// session — which is exactly the welcome-flow lifetime.
fn render_sessions_menu(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    if state.welcome_auto_expand_done {
        render_welcome_sessions(state, row_start, row_end, cols);
        return;
    }
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

    let mut rows: Vec<SelectorRow> = entries
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

    // "+ New Session" affordance pinned at the bottom of the list.
    // Three-cell shape keeps the table column count consistent with
    // the session rows above (see `render_centered_selector`'s
    // `n_cols` derivation) so the layout maths does not have to
    // special-case a single-cell row. The trailing two cells render
    // as blank — the action label sits in the name column. Emphasis-3
    // matches the colour used for `NewPaneAction` / `NewTabAction`
    // rows in the Panes selector for visual continuity across
    // "+ New …" affordances.
    rows.push(SelectorRow {
        cells: vec![
            named_cell("+ New Session".to_string(), 3),
            SelectorCell { text: Text::new(""), width: 0 },
            SelectorCell { text: Text::new(""), width: 0 },
        ],
        action: ClickAction::OpenNewSessionPrompt,
    });

    render_centered_selector(state, row_start, row_end, cols, "Switch Session", rows);
}

/// Welcome-screen variant of the Sessions selector. Used while the
/// mobile plugin is hosting the welcome flow — detected via
/// `state.welcome_auto_expand_done`, which is set when the
/// session-manager welcome pane is auto-closed on first
/// `PaneUpdate` and stays true for the lifetime of this plugin
/// instance (the host session ends when the user attaches to /
/// creates another).
///
/// The whole block — title, prompt, sessions, "+ New Session" — is
/// vertically centered in the body region. The footer always leaves
/// one row of breathing room above the bottom edge (modifier bar or
/// screen edge) so the affordance never sits flush against the
/// chrome below. Layout (no padding between sessions):
///
///   title row                (unstyled, "Hi from Zellij!")
///   blank
///   "Session: <buffer>_"     ("Session:" unstyled; buffer + cursor emphasis-3)
///   blank (or "↑ [+N]" emphasis-1 when scrolled up)
///   session 1 name           (emphasis-0, fuzzy matches in emphasis-3)
///   session 1 counts         (digits in colors 1 / 2 / 2)
///   ...
///   session N name
///   session N counts
///   blank (or "↓ [+M]" emphasis-1 when scrolled down)
///   "+ New Session"          (emphasis-3)
///
/// The "Session:" prompt is left-aligned with the leftmost edge of
/// the visible session column (falling back to the footer's centered
/// x when no sessions are visible) instead of being independently
/// centered, so it visually anchors to the same column as the
/// session names below it.
///
/// Block height with N visible sessions: `6 + 2N` for `N > 0`,
/// otherwise `5` (title + blank + prompt + blank + new_session). The
/// visible-card count is whatever fits in `body_height`; remaining
/// cards scroll via `state.selector_scroll_offset`. When the list
/// exceeds the visible window, scroll indicators painted in
/// emphasis-1 (`↑ [+N]` / `↓ [+M]`) replace the blank rows that flank
/// the session list, telling the user how many cards are hidden in
/// each direction.
///
/// Fuzzy matching uses `SkimMatcherV2` (same matcher the session-
/// manager welcome screen uses) keyed off `state.welcome_search`.
/// When the search term is empty, every non-current session is
/// shown in alphabetical order; otherwise only matches are shown,
/// sorted by score descending and tie-broken alphabetically. The
/// matched-character indices come back from the matcher and are
/// painted in emphasis-3 on the session-name row so the user can
/// see *why* a row matched.
///
/// The current session is filtered out — the welcome session itself
/// is what the user is leaving, so listing it as an attach target
/// would be confusing. (Filtering by `is_current_session` is still
/// needed here even though `filter_sessions_for_client` drops
/// welcome sessions upstream: after we close the welcome-screen
/// pane via `close_plugin_pane`, the host session no longer matches
/// `is_welcome_session` and would otherwise reappear.)
fn render_welcome_sessions(
    state: &mut State,
    row_start: usize,
    row_end: usize,
    cols: usize,
) {
    // Reserve one row at the bottom of the body so "+ New Session"
    // never sits flush against the modifier bar (when the soft
    // keyboard is up) or the screen edge (when it is not). Shadowing
    // the parameter keeps every downstream `< row_end` check honouring
    // the reservation without scattering the `- 1` across the body.
    let row_end = row_end.saturating_sub(1);
    let body_height = row_end.saturating_sub(row_start);
    if body_height == 0 || cols == 0 {
        return;
    }

    let title = "Hi from Zellij!";
    let new_session_label = "+ New Session";

    struct Card {
        name_label: String,
        counts_label: String,
        action: ClickAction,
        tab_range: std::ops::Range<usize>,
        pane_range: std::ops::Range<usize>,
        client_range: std::ops::Range<usize>,
        /// Char indices into `name_label` that matched the fuzzy
        /// search term — painted with emphasis-3 on the name row.
        /// Empty when the search term itself is empty.
        name_indices: Vec<usize>,
    }

    // Snapshot the search term so we can borrow it freely without
    // tangling with the matcher's `&mut` field borrow below. Small
    // string clone; happens once per frame.
    let search = state.welcome_search.clone();

    // Build the ordered (session_index, name_indices) list. The two
    // branches differ in sort key and in whether the matcher runs at
    // all — empty search means "show everything, alpha order".
    let order: Vec<(usize, Vec<usize>)> = if search.is_empty() {
        let mut indexed: Vec<(usize, &str)> = state
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_current_session)
            .map(|(i, s)| (i, s.name.as_str()))
            .collect();
        indexed.sort_by(|a, b| a.1.cmp(b.1));
        indexed.into_iter().map(|(i, _)| (i, Vec::new())).collect()
    } else {
        let matcher = state
            .welcome_fuzzy_matcher
            .get_or_insert_with(|| SkimMatcherV2::default().use_cache(true));
        let mut scored: Vec<(usize, i64, Vec<usize>)> = state
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.is_current_session)
            .filter_map(|(i, s)| {
                matcher
                    .fuzzy_indices(&s.name, &search)
                    .map(|(score, indices)| (i, score, indices))
            })
            .collect();
        // Score desc, then alphabetical tiebreak. `b.cmp(&a)` keeps
        // the higher-scoring row first; `state.sessions[i].name`
        // resolves the tiebreaker against the same source data.
        scored.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                state.sessions[a.0]
                    .name
                    .cmp(&state.sessions[b.0].name)
            })
        });
        scored.into_iter().map(|(i, _, indices)| (i, indices)).collect()
    };

    let cards: Vec<Card> = order
        .into_iter()
        .map(|(session_idx, indices)| {
            let s = &state.sessions[session_idx];
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
            let name = s.name.clone();
            let tab_str = format!("{}", s.tabs.len());
            let pane_str = format!("{}", pane_count);
            let conn_str = format!("{}", s.connected_clients);
            let client_word = if s.connected_clients == 1 {
                "client"
            } else {
                "clients"
            };
            let counts_label = format!(
                "{} tabs, {} panes, {} {}",
                tab_str, pane_str, conn_str, client_word
            );
            // Byte-offset color ranges mirror the session-manager
            // welcome screen (`UnifiedResultsRenderCache::rebuild`):
            // tab count in color 1; pane and client counts in color 2.
            // Digits are ASCII so byte offsets equal column offsets.
            let tab_end = tab_str.len();
            let pane_offset = tab_str.len() + " tabs, ".len();
            let pane_end = pane_offset + pane_str.len();
            let conn_offset = pane_end + " panes, ".len();
            let conn_end = conn_offset + conn_str.len();
            Card {
                name_label: name.clone(),
                counts_label,
                action: ClickAction::SelectSession(name),
                tab_range: 0..tab_end,
                pane_range: pane_offset..pane_end,
                client_range: conn_offset..conn_end,
                name_indices: indices,
            }
        })
        .collect();

    // Decide how many cards fit. With N > 0 the block needs
    // `6 + 2N` rows (title + blank + prompt + blank + 2N + blank +
    // new_session); the empty-state block needs `5` (title + blank +
    // prompt + blank + new_session). Solving `6 + 2N <= body_height`
    // for the maximum N gives `(body_height - 6) / 2` — saturating-
    // subtracted to handle pathologically short bodies where no card
    // fits.
    let total_cards = cards.len();
    let max_visible_cards = body_height.saturating_sub(6) / 2;
    let max_visible_cards = max_visible_cards.min(total_cards);

    let max_offset = total_cards.saturating_sub(max_visible_cards);
    let offset = state.selector_scroll_offset.min(max_offset);
    state.selector_scroll_offset = offset;
    let visible_count = total_cards.saturating_sub(offset).min(max_visible_cards);
    // Publish the count so the scroll handler can cap its per-event
    // delta and preserve at least one card of overlap across scrolls.
    state.last_welcome_visible_count = visible_count;

    let block_height = if visible_count == 0 {
        5.min(body_height)
    } else {
        (6 + 2 * visible_count).min(body_height)
    };

    // Vertically center the block within the body.
    let top_y = row_start + body_height.saturating_sub(block_height) / 2;

    let visible_slice: Vec<&Card> = cards.iter().skip(offset).take(visible_count).collect();
    // Card column: name and counts left-align under each other across
    // every visible card. The block is centered on `cols` using the
    // widest of name/counts widths seen.
    let card_w = visible_slice
        .iter()
        .map(|c| {
            UnicodeWidthStr::width(c.name_label.as_str())
                .max(UnicodeWidthStr::width(c.counts_label.as_str()))
        })
        .max()
        .unwrap_or(0);
    let card_x = cols.saturating_sub(card_w) / 2;

    // Title — unstyled, centered horizontally on `cols` (not on the
    // card column) so it sits on the screen's vertical axis even when
    // the card column is narrow.
    let title_w = UnicodeWidthStr::width(title);
    let title_x = cols.saturating_sub(title_w) / 2;
    let title_y = top_y;
    if title_y < row_end {
        print_text_with_coordinates(Text::new(title), title_x, title_y, None, None);
    }

    // "Session: <buffer>_" prompt. "Session: " is rendered unstyled;
    // the user-typed buffer plus the trailing underscore cursor glyph
    // are emphasis-3 so the active input area visually pops. A static
    // underscore stands in for the cursor (same approach as the new-
    // session prompt — avoids fighting host cursor gating).
    //
    // The prompt is left-aligned with the leftmost edge of the visible
    // content rather than centered on its own width: when sessions are
    // visible, it aligns with the leftmost session-card column
    // (`card_x`); otherwise it aligns with the footer's centered x
    // (the only other rendered chunk). This anchors the prompt to the
    // same column the user is scanning below it instead of letting it
    // drift left and right with every keystroke.
    let prompt_label = "Session: ";
    let prompt_body = format!("{}_", search);
    let prompt_full = format!("{}{}", prompt_label, prompt_body);
    let new_session_w = UnicodeWidthStr::width(new_session_label);
    let new_session_x = cols.saturating_sub(new_session_w) / 2;
    let prompt_x = if visible_count > 0 {
        card_x
    } else {
        new_session_x
    };
    let prompt_y = top_y + 2;
    if prompt_y < row_end {
        let label_chars = prompt_label.chars().count();
        let total_chars = prompt_full.chars().count();
        let prompt_text =
            Text::new(&prompt_full).color_range(3, label_chars..total_chars);
        print_text_with_coordinates(prompt_text, prompt_x, prompt_y, None, None);
    }

    // Scroll indicators: when the list is scrolled, paint
    // "↑ [+N]" / "↓ [+M]" in the blank rows that flank the session
    // list. The blanks are otherwise just dead space — repurposing
    // them avoids growing the block. Emphasis-1 distinguishes them
    // from the prompt (3) and the session-name highlights (3),
    // keeping the visual hierarchy intact.
    //
    // Centered on `cols` so the indicators sit on the screen's
    // vertical axis regardless of card-column width.
    let hidden_above = offset;
    let hidden_below = total_cards.saturating_sub(offset + visible_count);
    let indicator_x = |label_w: usize| -> usize {
        cols.saturating_sub(label_w) / 2
    };
    if visible_count > 0 && hidden_above > 0 {
        let top_indicator_y = top_y + 3;
        if top_indicator_y < row_end {
            let label = format!("\u{2191} [+{}]", hidden_above);
            let label_w = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                indicator_x(label_w),
                top_indicator_y,
                None,
                None,
            );
        }
    }
    if visible_count > 0 && hidden_below > 0 {
        let bottom_indicator_y = top_y + 4 + 2 * visible_count;
        if bottom_indicator_y < row_end {
            let label = format!("\u{2193} [+{}]", hidden_below);
            let label_w = UnicodeWidthStr::width(label.as_str());
            print_text_with_coordinates(
                Text::new(&label).color_range(1, ..),
                indicator_x(label_w),
                bottom_indicator_y,
                None,
                None,
            );
        }
    }

    // Sessions — each one occupies two rows immediately under the
    // previous (no inter-card padding per spec). Two rows of padding
    // (blank + prompt + blank) above shift the first session card to
    // `top_y + 4`.
    let sessions_start_y = top_y + 4;
    for (i, c) in visible_slice.iter().enumerate() {
        let row_name = sessions_start_y + i * 2;
        let row_counts = row_name + 1;
        if row_name >= row_end {
            break;
        }
        // Base name in emphasis-0; matched indices in emphasis-3.
        // `color_indices` indices are char positions, which is what
        // `fuzzy_indices` returns (verified against session-manager's
        // index handling in `ui/components.rs`).
        let mut name_text = Text::new(&c.name_label).color_range(0, ..);
        if !c.name_indices.is_empty() {
            name_text = name_text.color_indices(3, c.name_indices.clone());
        }
        print_text_with_coordinates(name_text, card_x, row_name, None, None);
        if row_counts < row_end {
            let counts_text = Text::new(&c.counts_label)
                .color_range(1, c.tab_range.clone())
                .color_range(2, c.pane_range.clone())
                .color_range(2, c.client_range.clone());
            print_text_with_coordinates(counts_text, card_x, row_counts, None, None);
        }
        let click_w = UnicodeWidthStr::width(c.name_label.as_str())
            .max(UnicodeWidthStr::width(c.counts_label.as_str()));
        state.click_regions.push(ClickRegion::tight_range(
            row_name,
            row_counts + 1,
            card_x,
            card_x + click_w,
            c.action.clone(),
        ));
    }

    // "+ New Session" — one blank row below the last visible session,
    // or directly under the blank-after-prompt when no sessions are
    // visible. `new_session_x` / `new_session_w` were already computed
    // above (for the prompt's fallback x).
    let new_session_y = top_y + block_height.saturating_sub(1);
    if new_session_y < row_end {
        print_text_with_coordinates(
            Text::new(new_session_label).color_range(3, ..),
            new_session_x,
            new_session_y,
            None,
            None,
        );
        state.click_regions.push(ClickRegion::tight(
            new_session_y,
            new_session_x,
            new_session_x + new_session_w,
            ClickAction::OpenNewSessionPrompt,
        ));
    }
}

/// In-plugin name-entry overlay for "+ New Session". Drawn vertically
/// centered within `[row_start, row_end)`. Keyboard and mouse driven —
/// Esc/Enter and the [Cancel]/[Accept] tap targets are equivalent.
///
/// Block layout (top to bottom, each row a single terminal line):
///   1. Title   — "New Session" in emphasis-3, centered horizontally
///      against `cols`.
///   2. blank.
///   3. Input   — "Name: <buffer>_" with `_` as a static cursor glyph
///      (mirrors the session-manager plugin's name prompt; a static
///      glyph avoids fighting with the plugin's `emit_cursor` gating
///      which is wired for the embedded viewport). Anchored to the
///      same left x as the [Cancel] button below it.
///   4. blank.
///   5. Buttons — "[Cancel]      [Accept]". Cancel renders in the
///      error palette, Accept in the success palette (host theme
///      colours from the Text API). Both are click targets dispatching
///      `CancelNewSessionPrompt` / `AcceptNewSessionPrompt`, which the
///      action handler routes through the same paths the Esc / Enter
///      key handlers use.
///
/// Horizontal layout: the prompt is treated as a centered rectangular
/// box with `H_PAD` cells of internal padding on each side. The box's
/// content width is `max(default_content_w, visible_input_w)`, where
/// `default_content_w` reserves room for a reasonable session name
/// (~20 chars) and never falls below the buttons row's natural width.
/// The box is centered against `cols`.
///
/// Inside the box:
///   - Title: centered within the content area (each side has equal
///     padding to the content edges).
///   - "Name:" row: anchored to the left edge of the content area
///     (`content_x`), so it sits directly above [Cancel].
///   - Buttons: [Cancel] pinned to the left edge of the content area,
///     [Accept] pinned to the right edge. The gap between them is
///     synthesised from spaces so the buttons row always spans the
///     full content width. When the box widens (e.g., the typed name
///     overflows the default size), the gap widens with it — Cancel
///     stays put, Accept follows the right edge outward.
///
/// Truncation: when the typed name plus chrome would exceed the
/// screen, the buffer is truncated *from the beginning* and a leading
/// "…" indicator is prepended so the cursor stays visible at the
/// right edge and the user can see that content is hidden. Standard
/// shell-style trailing-edge prompt behaviour.
///
/// If the body is too short to fit the full block, the renderer falls
/// back to top-anchored rendering and clips overflow.
fn render_new_session_prompt(
    state: &mut State,
    row_start: usize,
    row_end: usize,
    cols: usize,
) {
    let body_height = row_end.saturating_sub(row_start);
    if body_height == 0 || cols == 0 {
        return;
    }

    let title = "New Session";

    let cancel_label = "[Cancel]";
    let accept_label = "[Accept]";

    // Box model: equal horizontal padding around the content area.
    // `H_PAD = 1` is the minimum visible breathing room between the
    // content edges and the (implicit) box border. The box grows as
    // the typed name grows; the centering preserves equal left/right
    // padding to the screen edges at every size.
    const H_PAD: usize = 1;
    // Reserved name field: the box never gets narrower than what's
    // needed to display a "reasonable" session name. 20 chars covers
    // most names; longer names expand the box smoothly. Bumping this
    // widens the default gap between [Cancel] and [Accept].
    const RESERVED_INPUT_CHARS: usize = 20;
    // Leading indicator prepended to the buffer when characters have
    // been dropped from the front. `\u{2026}` is the single-cell
    // horizontal ellipsis.
    const ELLIPSIS: &str = "\u{2026}";

    let title_w = UnicodeWidthStr::width(title);
    let cancel_w = UnicodeWidthStr::width(cancel_label);
    let accept_w = UnicodeWidthStr::width(accept_label);
    let input_label_w = "Name: ".len();

    // Buffer rendering with sticky leading-ellipsis truncation. The
    // available room for the buffer is the screen width minus the
    // box padding, the "Name: " label, and the trailing cursor.
    //
    // `state.new_session_view_offset` is the count of characters
    // currently hidden behind the leading "…". It only *advances*
    // when typing would push the cursor past the right edge of the
    // input area; backspace leaves it put. The result: each keystroke
    // visibly changes the input row — typing extends it (or scrolls
    // the leading chars off if it would overflow), backspace shrinks
    // it (the cursor `_` moves left and the rightmost char drops).
    // The offset is reset to 0 once the buffer is short enough to fit
    // without an ellipsis, so the user gets the full text back as
    // soon as it can be shown in one line.
    let buffer_chars = state.pending_session_name.chars().count();
    let max_input_total_w = cols.saturating_sub(2 * H_PAD);
    let max_chars_no_ellipsis = max_input_total_w
        .saturating_sub(input_label_w)
        .saturating_sub(1);
    let ellipsis_w = UnicodeWidthStr::width(ELLIPSIS);
    // -ellipsis_w from the cap because the leading "…" itself
    // occupies a cell.
    let max_chars_with_ellipsis = max_chars_no_ellipsis.saturating_sub(ellipsis_w);

    let view_offset = if buffer_chars > max_chars_no_ellipsis {
        // Truncation needed. Floor: advance enough so the cursor sits
        // inside the input area. Cap: never exceed `buffer_chars`
        // (would index past the end).
        let min_offset = buffer_chars.saturating_sub(max_chars_with_ellipsis);
        state
            .new_session_view_offset
            .max(min_offset)
            .min(buffer_chars)
    } else {
        // Buffer fits without truncation — reveal everything.
        0
    };
    state.new_session_view_offset = view_offset;

    let visible_buffer: String = state
        .pending_session_name
        .chars()
        .skip(view_offset)
        .collect();
    let input = if view_offset > 0 {
        format!("Name: {}{}_", ELLIPSIS, visible_buffer)
    } else {
        format!("Name: {}_", visible_buffer)
    };
    let visible_input_w = UnicodeWidthStr::width(input.as_str());

    // Default content width: must accommodate the title, the buttons
    // (Cancel + a comfortable gap + Accept), and a reasonable typing
    // area. The buttons' natural width (`cancel_w + DEFAULT_GAP +
    // accept_w`) acts as the floor so [Cancel] and [Accept] never
    // collide on first render; the reserved input width (`input_label_w
    // + RESERVED_INPUT_CHARS + 1`) typically wins for any reasonable
    // RESERVED value.
    const DEFAULT_BUTTON_GAP: usize = 6;
    let default_buttons_w = cancel_w + DEFAULT_BUTTON_GAP + accept_w;
    let default_input_w = input_label_w + RESERVED_INPUT_CHARS + 1;
    let default_content_w = title_w.max(default_input_w).max(default_buttons_w);

    // High-water-mark content width. The box only ever *grows* during
    // a single prompt session — typing past the current width expands
    // it, backspacing keeps it put. This anchor is what makes the
    // cursor's leftward movement visible on every backspace: without
    // it, `box_x` flips by one column on alternate presses (integer
    // division of `cols - box_w` parity) and cancels the cursor
    // motion. The mark resets to 0 when the prompt closes / reopens,
    // so a fresh prompt starts at the default width.
    let target_content_w = default_content_w.max(visible_input_w);
    let content_w = state.new_session_content_w.max(target_content_w);
    state.new_session_content_w = content_w;
    // `min(cols)` clamps pathologically wide content (e.g. very narrow
    // screen, very long content) so the box never overflows the screen.
    let box_w = (content_w + 2 * H_PAD).min(cols);
    let box_x = cols.saturating_sub(box_w) / 2;
    let content_x = box_x + H_PAD;
    let content_w_effective = box_w.saturating_sub(2 * H_PAD);

    const BLOCK_ROWS: usize = 5;
    let top = if body_height >= BLOCK_ROWS {
        row_start + (body_height - BLOCK_ROWS) / 2
    } else {
        row_start
    };

    let row_title = top;
    let row_input = top + 2;
    let row_buttons = top + 4;

    // Title: centered within the content area (equal padding to both
    // content edges). When the box grows due to a long name, the title
    // shifts with it to stay visually centered.
    if row_title < row_end {
        let title_x = content_x + content_w_effective.saturating_sub(title_w) / 2;
        print_text_with_coordinates(
            Text::new(title).color_range(3, ..),
            title_x,
            row_title,
            None,
            None,
        );
    }

    // Name row: left-anchored to the content area (`content_x`) so it
    // sits directly above [Cancel]. The truncation above already
    // capped its visible width to fit inside the box, so no further
    // clipping is needed here.
    if row_input < row_end {
        print_text_with_coordinates(Text::new(&input), content_x, row_input, None, None);
    }

    if row_buttons < row_end {
        // [Cancel] pinned to the content area's left edge; [Accept]
        // pinned to its right edge. The gap between them is synthesised
        // as a run of spaces so the buttons row always spans the full
        // content width — this is what makes the whole box "stretch"
        // when the name overflows the default size: the gap grows
        // along with the box.
        let cancel_x = content_x;
        let accept_x =
            content_x + content_w_effective.saturating_sub(accept_w);
        let gap_w = accept_x.saturating_sub(cancel_x + cancel_w);
        let gap: String = " ".repeat(gap_w);
        let buttons = format!("{}{}{}", cancel_label, gap, accept_label);
        let buttons_text = Text::new(&buttons)
            .error_color_substring(cancel_label)
            .success_color_substring(accept_label);
        print_text_with_coordinates(buttons_text, cancel_x, row_buttons, None, None);

        // Register tap targets directly from the rendered positions
        // (`cancel_x` / `accept_x`). ASCII labels → byte len == char
        // count == display width.
        state.click_regions.push(ClickRegion::tight(
            row_buttons,
            cancel_x,
            cancel_x + cancel_w,
            ClickAction::CancelNewSessionPrompt,
        ));
        state.click_regions.push(ClickRegion::tight(
            row_buttons,
            accept_x,
            accept_x + accept_w,
            ClickAction::AcceptNewSessionPrompt,
        ));
    }
}

/// One row in the unified Change Pane navigator. Tab headers are
/// visual nesting only — they carry no click action; pane rows are
/// the only clickable items, matching the user-facing rule "we
/// always select the pane".
enum PaneSelectorItem {
    /// Header row for `tab.name`. Rendered full-width in emphasis-1.
    TabHeader(String),
    /// Pane row nested under its tab. Indented two cells, pane
    /// title in emphasis-2, activity right-aligned in unbold.
    PaneRow {
        title: String,
        activity: String,
        action: ClickAction,
    },
    /// "+ New Pane" action row appended after each tab's pane list.
    /// Tapping it dispatches `ClickAction::NewPaneInTab { tab_position }`,
    /// which calls the `new_tiled_pane_in_tab` shim and auto-selects
    /// the returned pane.
    NewPaneAction { tab_position: usize },
    /// "+ New Tab" action row appended once at the bottom of the
    /// selector. Tapping it dispatches `ClickAction::NewTab`, which
    /// calls the `new_tab_unfocused` shim and stashes the returned
    /// tab id in `state.pending_new_tab_position` for resolution on
    /// the next `PaneUpdate`.
    NewTabAction,
}

/// Unified Change Pane selector. Panes are listed grouped by tab —
/// a tab-name header followed by the panes belonging to that tab,
/// indented for visual nesting. Scrollable via `Mouse::ScrollUp` /
/// `Mouse::ScrollDown` (handled in `main.rs`): the offset slices
/// into the flat item list and stale offsets are clamped here on
/// the next frame.
fn render_panes_menu(state: &mut State, row_start: usize, row_end: usize, cols: usize) {
    let body_height = row_end.saturating_sub(row_start);
    if body_height == 0 || cols == 0 {
        return;
    }

    // Build the flat item list once per frame. Order: each visible
    // tab's header followed by its panes in display order, matching
    // `tabs_in_order` / `panes_for_tab` so the user sees the same
    // ordering they would in the underlying Zellij UI.
    let now = unix_now();
    let tabs: Vec<TabInfo> = state.tabs_in_order().into_iter().cloned().collect();
    let mut items: Vec<PaneSelectorItem> = Vec::new();
    for tab in &tabs {
        items.push(PaneSelectorItem::TabHeader(tab.name.clone()));
        let panes: Vec<PaneInfo> = state
            .panes_for_tab(tab.position)
            .into_iter()
            .cloned()
            .collect();
        for pane in &panes {
            let id = pane_id_of(pane);
            let title = if pane.title.is_empty() {
                format!("#{}", pane.id)
            } else {
                pane.title.clone()
            };
            let last_activity = state.pane_last_activity.get(&id).copied();
            let activity = format_time_ago(last_activity, now);
            items.push(PaneSelectorItem::PaneRow {
                title,
                activity,
                action: ClickAction::SelectPane {
                    tab_position: tab.position,
                    pane_id: id,
                },
            });
        }
        // "+ New Pane" affordance under each tab's pane list. Tap
        // creates a tiled pane in this specific tab via the
        // `new_tiled_pane_in_tab` shim — the returned pane id is
        // auto-selected in the mobile UI.
        items.push(PaneSelectorItem::NewPaneAction {
            tab_position: tab.position,
        });
    }
    // One global "+ New Tab" affordance at the very bottom; it is not
    // nested under any tab because the new tab does not yet exist.
    items.push(PaneSelectorItem::NewTabAction);

    // Title at the top of the body, centered horizontally and
    // coloured emphasis-3 (matching `render_centered_selector`'s
    // title styling for visual continuity with the Sessions
    // selector).
    let title = "Switch Pane";
    let title_w = UnicodeWidthStr::width(title);
    let title_x = cols.saturating_sub(title_w) / 2;
    print_text_with_coordinates(
        Text::new(title).color_range(3, ..),
        title_x,
        row_start,
        None,
        None,
    );

    // One blank row between the title and the data block; data
    // starts at `row_start + 2`. When the body is too short for any
    // data row we bail rather than crowd the title.
    let data_top = row_start + 2;
    if data_top >= row_end {
        return;
    }
    let max_visible = row_end - data_top;

    // Clamp scroll offset against the current item count. The
    // handler increments `selector_scroll_offset` blindly past the
    // valid range; this is where it gets snapped back so the user
    // can never scroll past the end (which would leave a partially-
    // empty view with no rows visible).
    let max_offset = items.len().saturating_sub(max_visible);
    let offset = state.selector_scroll_offset.min(max_offset);
    state.selector_scroll_offset = offset;

    let visible_count = items.len().saturating_sub(offset).min(max_visible);

    for (i, item) in items.iter().skip(offset).take(visible_count).enumerate() {
        let row = data_top + i;
        match item {
            PaneSelectorItem::TabHeader(name) => {
                // Tab header occupies the full row width with the
                // tab name in emphasis-1 and no click region — per
                // "we should always select the pane".
                let display = pad_or_truncate(name, cols);
                let chars = display.chars().count();
                let t = Text::new(&display).color_range(1, 0..chars);
                print_text_with_coordinates(t, 0, row, Some(cols), None);
            },
            PaneSelectorItem::PaneRow { title, activity, action } => {
                let indent_w = 2usize;
                let activity_w = UnicodeWidthStr::width(activity.as_str());
                // Reserve indent + activity + 1 separator cell;
                // whatever's left is the title's maximum width
                // (truncated with `…` when the title is longer).
                let title_max_w = cols
                    .saturating_sub(indent_w + activity_w + 1);
                let title_display = pad_or_truncate(title, title_max_w);
                let title_chars = title_display.chars().count();

                // Render the indent + title as one Text (so the
                // emphasis-2 colour applies to the title only, not
                // the indent). Activity is rendered separately so
                // its `unbold_all()` doesn't bleed into the title.
                let left_str = format!("  {}", title_display);
                let left_text =
                    Text::new(&left_str).color_range(2, 2..2 + title_chars);
                print_text_with_coordinates(
                    left_text,
                    0,
                    row,
                    Some(indent_w + title_max_w),
                    None,
                );

                if activity_w > 0 && activity_w <= cols {
                    let activity_x = cols - activity_w;
                    let activity_text = Text::new(activity).unbold_all();
                    print_text_with_coordinates(
                        activity_text,
                        activity_x,
                        row,
                        Some(activity_w),
                        None,
                    );
                }

                // Click region spans the entire row so a tap
                // anywhere on the pane row selects it. Headers (no
                // region) above and below remain non-interactive.
                state.click_regions.push(ClickRegion::tight(
                    row,
                    0,
                    cols,
                    action.clone(),
                ));
            },
            PaneSelectorItem::NewPaneAction { tab_position } => {
                // Indented two cells to nest under the tab header,
                // matching the `PaneRow` indent. Emphasis-3 (matches
                // the selector title) marks it as an action row
                // distinct from the live pane rows (emphasis-2) and
                // the tab headers (emphasis-1).
                let label = "+ New Pane";
                let indent_w = 2usize;
                let display_w = indent_w + UnicodeWidthStr::width(label);
                let display_w = display_w.min(cols);
                let mut row_str = String::with_capacity(display_w);
                for _ in 0..indent_w {
                    row_str.push(' ');
                }
                row_str.push_str(label);
                let label_chars = label.chars().count();
                let text = Text::new(&row_str)
                    .color_range(3, indent_w..indent_w + label_chars);
                print_text_with_coordinates(text, 0, row, Some(display_w), None);
                state.click_regions.push(ClickRegion::tight(
                    row,
                    0,
                    cols,
                    ClickAction::NewPaneInTab {
                        tab_position: *tab_position,
                    },
                ));
            },
            PaneSelectorItem::NewTabAction => {
                // Top-level (no indent) since the new tab does not yet
                // exist and therefore has no parent in the tab tree.
                let label = "+ New Tab";
                let label_w = UnicodeWidthStr::width(label);
                let display_w = label_w.min(cols);
                let label_chars = label.chars().count();
                let text = Text::new(label).color_range(3, 0..label_chars);
                print_text_with_coordinates(text, 0, row, Some(display_w), None);
                state.click_regions.push(ClickRegion::tight(
                    row,
                    0,
                    cols,
                    ClickAction::NewTab,
                ));
            },
        }
    }
}

/// One row in the hamburger dropdown menu. Toggle items track the
/// underlying state (`Fit` mirrors `state.fit_active`); navigation
/// items are stateless.
enum HamburgerItem {
    /// "Fit to Screen" — armed when `state.fit_active == true`.
    Fit,
    /// "Change Pane" — opens the unified Panes selector (panes
    /// grouped under their tabs) and closes the menu.
    ChangePane,
    /// "Change Session" — opens the Sessions selector and closes the
    /// menu.
    ChangeSession,
    /// "Switch to Desktop" — exits mobile mode (one-way). Tears down
    /// the client's mobile tab; re-entry requires a reconnect.
    SwitchToDesktop,
}

/// One row in the hamburger dropdown. Either an interactive
/// `Item` that registers a click region, or a non-interactive
/// `Separator` that visually divides item groups. Separators
/// consume a row but never push a click region, so taps on a
/// separator row fall through (resolving to no action).
enum HamburgerEntry {
    Item(&'static str, HamburgerItem),
    Separator,
}

/// Render the hamburger dropdown menu in the upper-right corner of
/// the body region. One row per item, starting at `row_start` and
/// truncated to fit within `[row_start, row_end)` so menu rows never
/// overlap the modifier bar's click regions below.
///
/// The Fit toggle renders in the success-green palette when armed
/// and emphasis-3 when unarmed; navigation items always render in
/// emphasis-3. The menu reuses the existing `ToggleFit` and
/// `ExpandPanes / ExpandSessions` dispatch arms — the toggle
/// preserves `menu_open` (it doesn't touch it), and navigation closes
/// the menu inside the `Expand*` arms themselves.
fn render_hamburger_menu(
    state: &mut State,
    row_start: usize,
    row_end: usize,
    cols: usize,
) {
    // A `Separator` between "Change Session" and "Switch to
    // Desktop" guards against an accidental tap on the destructive
    // (one-way) Switch-to-Desktop row: separators do not register
    // click regions, so they also create a column of dead pixels
    // between the two interactive groups.
    let entries: [HamburgerEntry; 5] = [
        HamburgerEntry::Item("Fit to Screen", HamburgerItem::Fit),
        HamburgerEntry::Item("Change Pane", HamburgerItem::ChangePane),
        HamburgerEntry::Item("Change Session", HamburgerItem::ChangeSession),
        HamburgerEntry::Separator,
        HamburgerEntry::Item("Switch to Desktop", HamburgerItem::SwitchToDesktop),
    ];

    let label_max = entries
        .iter()
        .filter_map(|e| match e {
            HamburgerEntry::Item(label, _) => Some(UnicodeWidthStr::width(*label)),
            HamburgerEntry::Separator => None,
        })
        .max()
        .unwrap_or(0);
    // 1 cell of left padding + label_max + 1 cell of right padding.
    let menu_w = label_max + 2;
    if label_max == 0 || menu_w > cols {
        return;
    }
    let menu_x = cols - menu_w;

    // Truncate to fit vertically. A short body (e.g. plugin keyboard
    // takes most of the screen) clips trailing entries rather than
    // overlapping the keyboard cells below.
    let max_visible = row_end.saturating_sub(row_start);
    let visible_entries = entries.len().min(max_visible);

    for (i, entry) in entries.iter().take(visible_entries).enumerate() {
        let row = row_start + i;
        match entry {
            HamburgerEntry::Item(label, item) => {
                let label_w = UnicodeWidthStr::width(*label);
                let trailing_pad = label_max - label_w;

                // Build " <label><trailing-pad> ": one cell left pad,
                // label_max cells of label-plus-trailing-pad, one cell
                // right pad. Constant `menu_w` cells total so click
                // regions are uniform across rows.
                let mut text_str = String::with_capacity(menu_w);
                text_str.push(' ');
                text_str.push_str(label);
                for _ in 0..trailing_pad {
                    text_str.push(' ');
                }
                text_str.push(' ');

                // `color_range` is character-indexed (not cell-indexed).
                // The leading space is one char; the label starts
                // immediately after.
                let label_char_start = 1;
                let label_char_end = label_char_start + label.chars().count();

                let armed = match item {
                    HamburgerItem::Fit => state.fit_active,
                    _ => false,
                };
                let mut t = Text::new(&text_str).selected();
                t = if armed {
                    t.success_color_range(label_char_start..label_char_end)
                } else {
                    t.color_range(3, label_char_start..label_char_end)
                };
                print_text_with_coordinates(t, menu_x, row, Some(menu_w), None);

                let action = match item {
                    HamburgerItem::Fit => ClickAction::ToggleFit,
                    HamburgerItem::ChangePane => ClickAction::ExpandPanes,
                    HamburgerItem::ChangeSession => ClickAction::ExpandSessions,
                    HamburgerItem::SwitchToDesktop => ClickAction::ExitMobileMode,
                };
                state.click_regions.push(ClickRegion::tight(
                    row,
                    menu_x,
                    menu_x + menu_w,
                    action,
                ));
            },
            HamburgerEntry::Separator => {
                // Same `menu_w` width as items so the row's
                // background painting stays uniform. Filled with the
                // light-horizontal box-drawing char so the divider
                // reads visually as a rule rather than a blank gap.
                // No click region is pushed: taps here fall through
                // and resolve to no action.
                let mut text_str = String::with_capacity(menu_w);
                text_str.push(' ');
                for _ in 0..label_max {
                    text_str.push('\u{2500}'); // ─
                }
                text_str.push(' ');
                let t = Text::new(&text_str).selected();
                print_text_with_coordinates(t, menu_x, row, Some(menu_w), None);
            },
        }
    }
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
    //
    // `State::max_viewport_v_pan` encodes the same formula — it
    // returns `None` only when no `viewport_region` is recorded yet
    // (we're recording it a few lines below). On that first frame the
    // helper has nothing to read, so we fall back to the inline
    // formula against the freshly-computed `height`. Once a single
    // frame has been laid out, the handler and renderer share the
    // helper's output and cannot drift.
    let max_v_pan = state
        .max_viewport_v_pan()
        .unwrap_or_else(|| viewport_lines.len().saturating_sub(height));
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

    /// The collapsed top bar emits three regions when there's no
    /// session segment: a tight pane region for the rendered text,
    /// a tight hamburger region for the glyph cell, and a slop
    /// region covering the pad between them so the small one-cell
    /// glyph has a generous tap halo. Verifies the partition, the
    /// slop fallback, and the context-sensitive pane action.
    #[test]
    fn collapsed_top_bar_partition_with_slop() {
        // 80-col bar: pane text fills cells 0..40, pad spans 40..79,
        // hamburger sits at cell 79.
        let cols = 80;
        let pane_tight_end = 40;
        let hamburger_start = 79;
        let regions = top_bar_collapsed_click_regions(
            0,
            cols,
            pane_tight_end,
            hamburger_start,
            ClickAction::ExpandPanes,
            None,
            ClickAction::ExpandSessions,
        );

        assert_eq!(regions.len(), 3);
        // Tight pane.
        assert!(matches!(regions[0].action, ClickAction::ExpandPanes));
        assert_eq!(regions[0].priority, 0);
        assert_eq!(regions[0].col_start, 0);
        assert_eq!(regions[0].col_end, pane_tight_end);
        // Tight hamburger.
        assert!(matches!(regions[1].action, ClickAction::ToggleMenu));
        assert_eq!(regions[1].priority, 0);
        assert_eq!(regions[1].col_start, hamburger_start);
        assert_eq!(regions[1].col_end, cols);
        // Slop hamburger.
        assert!(matches!(regions[2].action, ClickAction::ToggleMenu));
        assert_eq!(regions[2].priority, 1);
        assert_eq!(regions[2].col_start, pane_tight_end);
        assert_eq!(regions[2].col_end, cols);

        // Dispatch: pane cell, slop pad cell, hamburger glyph.
        let mut state = State::default();
        state.click_regions = regions.clone();
        assert_eq!(
            state.click_to_action(0, 0),
            Some(ClickAction::ExpandPanes)
        );
        assert_eq!(
            state.click_to_action(0, pane_tight_end + 5),
            Some(ClickAction::ToggleMenu),
            "pad cell should fall through to slop hamburger",
        );
        assert_eq!(
            state.click_to_action(0, hamburger_start),
            Some(ClickAction::ToggleMenu)
        );
    }

    /// In selector mode the pane region collapses the selector
    /// instead of opening Change Pane — the simplified top bar
    /// preserves the legacy "tap the bar to escape" gesture as a
    /// one-tap return to the viewport.
    #[test]
    fn collapsed_top_bar_pane_action_collapses_in_selector_mode() {
        let cols = 80;
        let regions = top_bar_collapsed_click_regions(
            0,
            cols,
            40,
            79,
            ClickAction::CollapseSelector,
            None,
            ClickAction::CollapseSelector,
        );
        assert!(matches!(regions[0].action, ClickAction::CollapseSelector));
        let mut state = State::default();
        state.click_regions = regions.clone();
        assert_eq!(
            state.click_to_action(0, 0),
            Some(ClickAction::CollapseSelector)
        );
    }

    /// When a session segment is present, the left content area is
    /// split into three tight sub-regions: prefix → `pane_action`,
    /// session text → `session_action`, separator + pane title →
    /// `pane_action`. Verified by dispatching a click into each
    /// sub-range plus the hamburger glyph.
    #[test]
    fn collapsed_top_bar_session_sub_region_dispatches_expand_sessions() {
        // Layout: prefix "Zellij " = cells 0..7, session "demo" =
        // cells 7..11, separator " " = cells 11..12, pane "shell" =
        // cells 12..17, pad 17..79, hamburger at 79.
        let cols = 80;
        let pane_tight_end = 17;
        let hamburger_start = 79;
        let session_cells = (7usize, 11usize);
        let regions = top_bar_collapsed_click_regions(
            0,
            cols,
            pane_tight_end,
            hamburger_start,
            ClickAction::ExpandPanes,
            Some(session_cells),
            ClickAction::ExpandSessions,
        );

        // Three left-side tight regions + tight hamburger + slop = 5.
        assert_eq!(regions.len(), 5);
        // [0, 7) → pane action.
        assert_eq!(regions[0].col_start, 0);
        assert_eq!(regions[0].col_end, 7);
        assert!(matches!(regions[0].action, ClickAction::ExpandPanes));
        // [7, 11) → session action.
        assert_eq!(regions[1].col_start, 7);
        assert_eq!(regions[1].col_end, 11);
        assert!(matches!(regions[1].action, ClickAction::ExpandSessions));
        // [11, 17) → pane action (separator + pane title).
        assert_eq!(regions[2].col_start, 11);
        assert_eq!(regions[2].col_end, 17);
        assert!(matches!(regions[2].action, ClickAction::ExpandPanes));

        let mut state = State::default();
        state.click_regions = regions;
        // Prefix cell → ExpandPanes.
        assert_eq!(state.click_to_action(0, 3), Some(ClickAction::ExpandPanes));
        // Session cell → ExpandSessions.
        assert_eq!(
            state.click_to_action(0, 9),
            Some(ClickAction::ExpandSessions),
        );
        // Separator cell → ExpandPanes.
        assert_eq!(state.click_to_action(0, 11), Some(ClickAction::ExpandPanes));
        // Pane title cell → ExpandPanes.
        assert_eq!(state.click_to_action(0, 14), Some(ClickAction::ExpandPanes));
        // Pad → slop hamburger.
        assert_eq!(state.click_to_action(0, 30), Some(ClickAction::ToggleMenu));
        // Hamburger glyph → tight hamburger.
        assert_eq!(state.click_to_action(0, 79), Some(ClickAction::ToggleMenu));
    }

    /// When `show_prefix` is false (the prefix was dropped to make
    /// room) the session range starts at column 0, so no zero-width
    /// `[0, 0)` pane region should be emitted ahead of it.
    #[test]
    fn collapsed_top_bar_session_at_left_edge_skips_empty_prefix_region() {
        let cols = 40;
        let pane_tight_end = 11;
        let hamburger_start = 39;
        let session_cells = (0usize, 4usize);
        let regions = top_bar_collapsed_click_regions(
            0,
            cols,
            pane_tight_end,
            hamburger_start,
            ClickAction::ExpandPanes,
            Some(session_cells),
            ClickAction::ExpandSessions,
        );

        // 2 left-side tight regions (session + pane) + hamburger +
        // slop = 4 — the prefix region is omitted as zero-width.
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0].col_start, 0);
        assert_eq!(regions[0].col_end, 4);
        assert!(matches!(regions[0].action, ClickAction::ExpandSessions));
        assert_eq!(regions[1].col_start, 4);
        assert_eq!(regions[1].col_end, pane_tight_end);
        assert!(matches!(regions[1].action, ClickAction::ExpandPanes));
    }

    /// Build a `State` carrying `tab_count` tabs each with one
    /// terminal pane. Tabs are at positions 0..tab_count, panes use
    /// ids 100..100+tab_count. Selected tab/pane are tab 0 / pane 100.
    fn state_with_tabs_and_panes(tab_count: usize) -> State {
        use zellij_tile::prelude::TabInfo;
        let mut state = State::default();
        for i in 0..tab_count {
            let mut tab = TabInfo::default();
            tab.position = i;
            tab.name = format!("Tab {}", i);
            state.tabs.push(tab);
            let mut pane = PaneInfo::default();
            pane.id = (100 + i) as u32;
            pane.is_plugin = false;
            pane.is_selectable = true;
            pane.is_suppressed = false;
            state.panes_by_tab_position.insert(i, vec![pane]);
        }
        state.selected_tab_position = Some(0);
        state.selected_pane_id = Some(PaneId::Terminal(100));
        state
    }

    /// The hamburger dropdown emits one click region per visible
    /// item, in declaration order: Fit (row 0), Change Pane (row 1),
    /// Change Session (row 2), separator (row 3, no region), Switch
    /// to Desktop (row 4). The separator row must not dispatch any
    /// action — that gap is the guard against accidental taps on
    /// the destructive Switch-to-Desktop row.
    #[test]
    fn hamburger_menu_emits_four_click_regions_with_separator_above_exit() {
        let mut state = State::default();
        let cols = 40;
        // Plenty of vertical space so every entry is visible.
        render_hamburger_menu(&mut state, 0, 20, cols);

        assert_eq!(state.click_regions.len(), 4);
        let actions: Vec<ClickAction> = state
            .click_regions
            .iter()
            .map(|r| r.action.clone())
            .collect();
        assert!(matches!(actions[0], ClickAction::ToggleFit));
        assert!(matches!(actions[1], ClickAction::ExpandPanes));
        assert!(matches!(actions[2], ClickAction::ExpandSessions));
        assert!(matches!(actions[3], ClickAction::ExitMobileMode));

        // The interactive rows must occupy 0, 1, 2, 4 — skipping
        // row 3 (the separator). Use the row span to confirm the
        // gap is exactly where expected.
        let rows: Vec<usize> = state
            .click_regions
            .iter()
            .map(|r| r.row_start)
            .collect();
        assert_eq!(rows, vec![0, 1, 2, 4]);

        // Tapping the separator row at any column inside the menu
        // width must resolve to no action.
        let menu_x = state.click_regions[0].col_start;
        let menu_end = state.click_regions[0].col_end;
        for c in menu_x..menu_end {
            assert_eq!(
                state.click_to_action(3, c),
                None,
                "separator row should be non-interactive at col {c}",
            );
        }
    }

    /// With one tab + one pane the Panes selector lists: title row,
    /// blank row, tab header, pane row, "+ New Pane", "+ New Tab".
    /// `render_panes_menu` populates `state.click_regions` for the
    /// rows it considers interactive — pane row, "+ New Pane", and
    /// "+ New Tab" (the tab header is non-interactive).
    #[test]
    fn panes_menu_one_tab_emits_three_click_regions() {
        let mut state = state_with_tabs_and_panes(1);
        let cols = 40;
        // Plenty of vertical space so every item is visible.
        render_panes_menu(&mut state, 0, 20, cols);
        // 1 PaneRow + 1 NewPaneAction + 1 NewTabAction = 3 regions.
        assert_eq!(state.click_regions.len(), 3);
        let actions: Vec<ClickAction> = state
            .click_regions
            .iter()
            .map(|r| r.action.clone())
            .collect();
        assert!(matches!(
            actions[0],
            ClickAction::SelectPane {
                tab_position: 0,
                pane_id: PaneId::Terminal(100)
            }
        ));
        assert!(matches!(
            actions[1],
            ClickAction::NewPaneInTab { tab_position: 0 }
        ));
        assert!(matches!(actions[2], ClickAction::NewTab));
    }

    /// Two tabs ⇒ two "+ New Pane" rows (one per tab) and exactly one
    /// "+ New Tab" row at the bottom of the list.
    #[test]
    fn panes_menu_two_tabs_emits_per_tab_new_pane_rows() {
        let mut state = state_with_tabs_and_panes(2);
        let cols = 40;
        render_panes_menu(&mut state, 0, 20, cols);
        // 2 PaneRows + 2 NewPaneActions + 1 NewTabAction = 5 regions.
        assert_eq!(state.click_regions.len(), 5);

        let new_panes: Vec<usize> = state
            .click_regions
            .iter()
            .filter_map(|r| match &r.action {
                ClickAction::NewPaneInTab { tab_position } => Some(*tab_position),
                _ => None,
            })
            .collect();
        assert_eq!(new_panes, vec![0, 1]);

        let new_tabs = state
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewTab))
            .count();
        assert_eq!(new_tabs, 1);
    }

    /// "+ New Tab" is the last row of the list (highest row index)
    /// regardless of how many tabs precede it. This guarantees the
    /// row is unambiguously the trailing global affordance, not
    /// confusable with a per-tab "+ New Pane" sibling.
    #[test]
    fn new_tab_row_is_below_all_new_pane_rows() {
        let mut state = state_with_tabs_and_panes(2);
        render_panes_menu(&mut state, 0, 20, 40);
        let new_tab_row = state
            .click_regions
            .iter()
            .find(|r| matches!(r.action, ClickAction::NewTab))
            .expect("expected a NewTab region")
            .row_start;
        let max_new_pane_row = state
            .click_regions
            .iter()
            .filter(|r| matches!(r.action, ClickAction::NewPaneInTab { .. }))
            .map(|r| r.row_start)
            .max()
            .expect("expected at least one NewPaneInTab region");
        assert!(
            new_tab_row > max_new_pane_row,
            "NewTab row {} should be below all NewPaneInTab rows (max {})",
            new_tab_row,
            max_new_pane_row
        );
    }

    /// Click dispatch round-trip: tapping the "+ New Pane" row's
    /// column-0 cell resolves to `ClickAction::NewPaneInTab` with the
    /// correct `tab_position`. Confirms the click region covers the
    /// full row width (`col_start == 0`).
    #[test]
    fn click_on_new_pane_row_resolves_to_action() {
        let mut state = state_with_tabs_and_panes(2);
        render_panes_menu(&mut state, 0, 20, 40);
        let new_pane_region = state
            .click_regions
            .iter()
            .find(|r| {
                matches!(
                    r.action,
                    ClickAction::NewPaneInTab { tab_position: 1 }
                )
            })
            .expect("expected NewPaneInTab for tab 1")
            .clone();
        assert_eq!(
            state.click_to_action(new_pane_region.row_start, 0),
            Some(ClickAction::NewPaneInTab { tab_position: 1 })
        );
    }
}

