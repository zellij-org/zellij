//! Mobile-friendly rendering for the welcome screen.
//!
//! The welcome screen runs the `session-manager` plugin in
//! `SingleScreen` mode (the `welcome-screen` plugin alias only sets
//! `welcome_screen=true`, leaving `is_multi_screen=false`). At narrow
//! viewports the default desktop layout — banner art, boundary
//! decorations, centred 90-col content block — does not fit. This
//! module renders the same flow vertically stacked, with no banner
//! and no decorative boundaries, so it fits in a ~50 col × ~25 row
//! mobile viewport.
//!
//! Entered via an early-return at the top of `State::render` when
//! `is_welcome_screen` is true and the viewport is small. Input
//! handling and selection logic are untouched — only the rendering
//! branch differs.
//!
//! Reuses existing helpers from `ui::components`:
//! - `render_single_screen_prompt` for the search/name input row
//! - `render_unified_results` for the active+resurrectable table
//!   (already adapts to narrow widths via `compute_reduction_tier`)
//! - `render_renaming_session_screen` for the rename overlay
//! - `render_error` for the error overlay
//! - `Colors` styling helpers
//!
//! Reuses `State::render_kill_all_sessions_warning` for the kill-all
//! confirmation overlay.

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::new_session_info::range_to_render;
use crate::single_screen::{SingleScreenMode, UnifiedSearchResult};
use crate::ui::components::{
    render_error, render_renaming_session_screen, render_single_screen_prompt,
    render_unified_results, Colors,
};
use crate::State;

/// Single-row click target emitted by the mobile renderer. A tap on
/// `row` (0-indexed plugin coordinates) dispatches the embedded
/// action.
///
/// The plugin coords match what `Mouse::LeftClick(line, col)` reports
/// once `line` is cast to `usize` — render-time placement uses the
/// same 0-indexed system through `print_text_with_coordinates` /
/// `print_table_with_coordinates`.
#[derive(Debug, Clone)]
pub enum MobileClickTarget {
    /// Tap on a session row in the unified active+resurrectable list.
    /// `unified_index` is the absolute position in
    /// `SingleScreenState::unified_results` (the unfiltered list);
    /// applying it to `selected_index` lets the shared `handle_selection`
    /// path attach / resurrect without any new business logic.
    UnifiedResult { row: usize, unified_index: usize },
    /// Tap on a layout row in the SelectingLayout sub-mode. The
    /// `layout_index` is the absolute position into
    /// `LayoutList::layout_list` (when the search term is empty) or
    /// into `layout_search_results` (when searching) — i.e. the same
    /// value `selected_layout_index` is indexed against.
    Layout { row: usize, layout_index: usize },
}

/// Viewport-width breakpoint below which the mobile renderer takes
/// over. Matches `mobile_threshold_cols` default in
/// `zellij-utils/src/input/options.rs` so the welcome screen routes to
/// mobile rendering at the same size at which a client would have
/// been routed to the mobile plugin by the server.
pub const MOBILE_MAX_COLS: usize = 60;

/// Viewport-height breakpoint, matches `mobile_threshold_rows`.
pub const MOBILE_MAX_ROWS: usize = 30;

/// Top-level mobile welcome renderer. Called from `State::render`
/// when the viewport is small. Owns the full frame: header, body,
/// controls, and overlays. The caller must early-return after this
/// so the desktop rendering path does not also draw.
pub fn render(state: &mut State, rows: usize, cols: usize) {
    // Each render rebuilds the click-target list. Cleared up-front
    // so an early-return path (overlay, blank viewport) leaves no
    // stale targets behind that a stray click could match against.
    state.mobile_click_targets.clear();

    if rows == 0 || cols == 0 {
        return;
    }

    // Kill-all confirmation overlay takes the whole frame: a
    // destructive y/n decision shouldn't share space with the
    // background list. Returns early so no other overlays draw.
    if state.show_kill_all_sessions_warning {
        state.render_kill_all_sessions_warning(rows, cols, 0, 0);
        return;
    }

    render_header(cols, state.colors);

    // Rename overlay sits in place of the body but below the header
    // so users still see context. Two rows of header padding before
    // the rename prompt.
    if let Some(new_name) = state.renaming_session_name.clone() {
        render_renaming_session_screen(&new_name, rows.saturating_sub(2), cols, 0, 2);
        return;
    }

    // body_y = 2 leaves row 0 for the header and row 1 blank.
    // body_rows reserves the last row for the controls hint.
    let body_y = 2;
    let body_rows = rows.saturating_sub(body_y + 1);

    let mode = state.single_screen_state.mode.clone();
    match &mode {
        SingleScreenMode::SearchAndSelect => {
            render_search_and_select(state, body_y, body_rows, cols);
        },
        SingleScreenMode::SelectingLayout => {
            render_selecting_layout(state, body_y, body_rows, cols);
        },
    }

    render_controls(&mode, state.colors, cols, rows.saturating_sub(1));

    if let Some(err) = state.error.as_ref() {
        // Place the error on its own row so it overrides the controls
        // line when both would otherwise share the bottom.
        let err = err.clone();
        render_error(&err, rows.saturating_sub(1), cols, 0, 0);
    }
}

/// Single-line title at row 0. Compact text-only header — no banner
/// art (the desktop welcome's `SMALL_BANNER` is ~6 rows tall, which
/// alone would eat a quarter of a mobile viewport).
fn render_header(cols: usize, colors: Colors) {
    let title = "Zellij";
    let tagline = " — start a session";
    let combined = format!("{}{}", title, tagline);
    let display = if combined.width() <= cols {
        format!(
            "{}{}",
            colors.session_name_prompt(title),
            colors.session_and_folder_entry(tagline),
        )
    } else if title.width() <= cols {
        colors.session_name_prompt(title)
    } else {
        return;
    };
    print!("\u{1b}[m\u{1b}[1;1H{}", display);
}

/// SearchAndSelect mode: a search/name input row followed by the
/// unified results table (active + resurrectable sessions). Mirrors
/// the desktop SingleScreen flow but vertically stacked at x=0 with
/// no horizontal centering.
fn render_search_and_select(
    state: &mut State,
    body_y: usize,
    body_rows: usize,
    cols: usize,
) {
    let enter_action = compute_enter_action(state);

    // Prompt takes 2 rows (the helper prints at y+2 with a trailing
    // newline). Leave the remaining body for the result table.
    let max_table_rows = body_rows.saturating_sub(2);

    render_single_screen_prompt(
        &state.single_screen_state.search_term,
        enter_action,
        state.colors,
        0,
        body_y.saturating_sub(2),
    );

    let table_y = body_y + 2;
    render_unified_results(
        &state.single_screen_state.render_cache,
        state.single_screen_state.selected_index,
        max_table_rows,
        cols,
        state.colors,
        0,
        table_y,
    );

    // Record per-row click targets. The visible window calculation
    // mirrors what `render_unified_results` uses internally so that
    // a click at plugin row R maps to the same cache row that R is
    // displaying. The table's first row is an empty header, so data
    // rows start at table_y + 1.
    let targets: Vec<MobileClickTarget> = {
        let cache = &state.single_screen_state.render_cache;
        let selected_index = state.single_screen_state.selected_index;
        let filtered_selected = selected_index
            .and_then(|sel| cache.rows.iter().position(|r| r.original_index == sel));
        let total = cache.rows.len();
        let data_rows = max_table_rows.saturating_sub(1);
        let (start, end) = if data_rows >= total {
            (0, total)
        } else {
            let anchor = filtered_selected.unwrap_or(0);
            let half = data_rows / 2;
            let mut s = anchor.saturating_sub(half);
            let mut e = s + data_rows;
            if e > total {
                e = total;
                s = total.saturating_sub(data_rows);
            }
            (s, e)
        };
        cache.rows[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| MobileClickTarget::UnifiedResult {
                row: table_y + 1 + i,
                unified_index: row.original_index,
            })
            .collect()
    };
    state.mobile_click_targets.extend(targets);
}

/// SelectingLayout mode: name has been entered, user picks a layout.
/// Stacks `Name: ...`, `Layout: ...`, then a layout list. The
/// folder prompt from the desktop variant is intentionally omitted —
/// `<Ctrl f>` is hard to hit on mobile and the default cwd is a fine
/// fallback.
fn render_selecting_layout(
    state: &mut State,
    body_y: usize,
    body_rows: usize,
    cols: usize,
) {
    let new_session_name = if state.single_screen_state.search_term.is_empty() {
        "<RANDOM>".to_string()
    } else {
        state.single_screen_state.search_term.clone()
    };
    let esc = state.colors.shortcuts("<ESC>");
    print!(
        "\u{1b}[m\u{1b}[{};1H{}: {} ({} back)",
        body_y + 1,
        state.colors.session_name_prompt("Name"),
        state.colors.session_and_folder_entry(&new_session_name),
        esc,
    );

    let layout_search_term = state
        .single_screen_state
        .layout_list
        .layout_search_term
        .clone();
    let search_term_len = layout_search_term.width();
    let layout_label = "Layout: ";
    let layout_label_len = layout_label.width();
    let layout_line = Text::new(format!(
        "{}{}_ <ENTER>",
        layout_label, layout_search_term
    ))
    .color_range(2, ..layout_label_len)
    .color_range(3, layout_label_len..layout_label_len + search_term_len)
    .color_range(
        3,
        layout_label_len + search_term_len + 2..,
    );
    print_text_with_coordinates(layout_line, 0, body_y + 2, None, None);

    // 3 rows consumed: name row, layout-search row, spacer.
    let max_layout_rows = body_rows.saturating_sub(3);
    if max_layout_rows == 0 {
        return;
    }
    let layouts = state
        .single_screen_state
        .layout_list
        .layouts_to_render(max_layout_rows);
    let mut table = Table::new();
    for (i, (layout_info, indices, is_selected)) in layouts.iter().enumerate() {
        if i >= max_layout_rows {
            break;
        }
        let layout_name = layout_info.name();
        let layout_name_len = layout_name.width();
        let is_builtin = layout_info.is_builtin();
        let mut layout_cell = if is_builtin {
            Text::new(format!("{} (built-in)", layout_name))
                .color_range(1, 0..layout_name_len)
                .color_range(0, layout_name_len + 1..)
                .color_indices(3, indices.clone())
        } else {
            Text::new(format!("{}", layout_name))
                .color_range(1, ..)
                .color_indices(3, indices.clone())
        };
        if *is_selected {
            layout_cell = layout_cell.selected();
        }
        let arrow_cell = if *is_selected {
            Text::new("<↓↑>").selected().color_range(3, ..)
        } else {
            Text::new("    ").color_range(3, ..)
        };
        table = table.add_styled_row(vec![arrow_cell, layout_cell]);
    }
    let table_y = body_y + 4;
    print_table_with_coordinates(table, 0, table_y, Some(cols), Some(max_layout_rows));

    // Record per-row click targets. `layouts_to_render` returns a
    // visible slice starting at `range.0` in the underlying list
    // (`layout_list` when not searching, `layout_search_results`
    // when searching) — mirror that math here so a tap maps back to
    // the same `selected_layout_index` the keyboard path would set.
    let list_len = if state
        .single_screen_state
        .layout_list
        .layout_search_term
        .is_empty()
    {
        state.single_screen_state.layout_list.layout_list.len()
    } else {
        state
            .single_screen_state
            .layout_list
            .layout_search_results
            .len()
    };
    let range = range_to_render(
        max_layout_rows,
        list_len,
        Some(state.single_screen_state.layout_list.selected_layout_index),
    );
    let visible_start = range.0;
    let targets: Vec<MobileClickTarget> = layouts
        .iter()
        .enumerate()
        .map(|(i, _)| MobileClickTarget::Layout {
            row: table_y + i,
            layout_index: visible_start + i,
        })
        .collect();
    state.mobile_click_targets.extend(targets);
}

/// Single-row controls hint at the bottom of the viewport. Per-mode
/// to surface only the keys that apply to the active screen.
fn render_controls(mode: &SingleScreenMode, _colors: Colors, cols: usize, y: usize) {
    let line = match mode {
        SingleScreenMode::SearchAndSelect => Text::new("<↓↑> select  <ENTER> confirm  <TAB> complete")
            .color_substring(3, "<↓↑>")
            .color_substring(3, "<ENTER>")
            .color_substring(3, "<TAB>"),
        SingleScreenMode::SelectingLayout => Text::new("<↓↑> select  <ENTER> create  <ESC> back")
            .color_substring(3, "<↓↑>")
            .color_substring(3, "<ENTER>")
            .color_substring(3, "<ESC>"),
    };
    print_text_with_coordinates(line, 0, y, Some(cols), None);
}

/// Dispatch a left-click in plugin coordinates. Looks up the row in
/// the targets recorded by the last `render()` call and applies the
/// matching action by setting the selection field and delegating to
/// `State::handle_selection` — the same path Enter uses, so attach /
/// resurrect / create-new logic stays in one place.
///
/// Returns `true` if a target matched and a render-worthy state change
/// was made; `false` when the click missed every target or the plugin
/// is not currently rendering the mobile welcome UI.
pub fn handle_click(state: &mut State, line: usize, _col: usize) -> bool {
    if !state.is_welcome_screen {
        return false;
    }
    let target = state
        .mobile_click_targets
        .iter()
        .find(|t| match t {
            MobileClickTarget::UnifiedResult { row, .. } => *row == line,
            MobileClickTarget::Layout { row, .. } => *row == line,
        })
        .cloned();
    let Some(target) = target else {
        return false;
    };
    match target {
        MobileClickTarget::UnifiedResult { unified_index, .. } => {
            state.single_screen_state.selected_index = Some(unified_index);
            state.handle_selection();
            true
        },
        MobileClickTarget::Layout { layout_index, .. } => {
            state.single_screen_state.layout_list.selected_layout_index = layout_index;
            state.handle_selection();
            true
        },
    }
}

/// Mirror of the desktop enter-action computation
/// (`main.rs::render`'s `SingleScreen` arm). Tells the user what
/// pressing Enter will do given the current search term and
/// selection, so the hint in the prompt stays accurate.
fn compute_enter_action(state: &State) -> Option<&'static str> {
    if state.single_screen_state.search_term.is_empty() {
        return None;
    }
    if let Some(result) = state.single_screen_state.get_selected_result() {
        return Some(match result {
            UnifiedSearchResult::ActiveSession { .. } => "Attach",
            UnifiedSearchResult::ResurrectableSession { .. } => "Resurrect",
        });
    }
    let typed = &state.single_screen_state.search_term;
    if state.sessions.has_session(typed) {
        Some("Attach")
    } else if state.resurrectable_sessions.has_session(typed) {
        Some("Resurrect")
    } else {
        Some("Create new")
    }
}
