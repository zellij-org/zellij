//! Rendering shell + shared chrome for the mobile plugin. The
//! per-screen body layout lives on each screen struct (see `screens/`);
//! this module owns the top-level `render` dispatcher, the shared top
//! bar, and the pure text helpers (`pad_or_truncate`, `visible_width`,
//! `slice_ansi_visible`, …) the screens reuse.

use crate::click::{ClickAction, ClickRegion};
use crate::frame::{chrome_offsets, Frame};
use crate::modifier_bar;
use crate::screens::ActiveScreen;
use crate::state::State;
use crate::workspace::Workspace;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

/// Single ANSI escape that resets the active style. Emitted between
/// every UI cell so a residual SGR bleed from the embedded viewport
/// does not contaminate the chrome.
pub(crate) const RESET: &str = "\x1b[0m";

/// Move the cursor to (row, col), 1-based as ANSI expects. The plugin
/// render area is 0-based, so we add 1 here.
pub(crate) fn move_to(row: usize, col: usize) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

/// Renders the stub UI used during scaffolding; kept as a fallback for
/// the very first frame before any state has been received.
pub fn render_stub(state: &mut State, rows: usize, cols: usize) {
    state.frame.emit_cursor(None);
    print!("{}{}mobile plugin loaded \u{2014} {}x{}", RESET, move_to(0, 0), rows, cols);
}

/// Top-level render. Clears the screen, paints the shared top bar at row
/// 0, and fills the body with the active screen (Viewport / Sessions /
/// Panes / NewSessionPrompt). The hamburger menu overlays the body when
/// open; the modifier bar reserves the bottom row when the soft keyboard
/// is up.
pub fn render(state: &mut State, rows: usize, cols: usize) {
    state.frame.click_regions.clear();
    state.viewport.viewport_region = None;
    state.frame.last_render_rows = rows;
    state.frame.last_render_cols = cols;

    if rows < 4 || cols < 8 {
        // No room for a meaningful UI — degrade to the stub. Hide the
        // host cursor since there's nothing meaningful to point at.
        state.frame.emit_cursor(None);
        print!("{}\x1b[2J{}mobile {}x{}", RESET, move_to(0, 0), rows, cols);
        return;
    }

    // Welcome flow / open Sessions selector suppress the top bar (the
    // welcome-style body paints its own "[← BACK]" affordance).
    let in_welcome_flow = state.sessions.is_welcome_screen;
    let in_sessions_selector = state.active == ActiveScreen::Sessions;
    let suppress_top_bar = in_welcome_flow || in_sessions_selector;
    let (body_top, bar_height) =
        chrome_offsets(rows, suppress_top_bar, state.frame.soft_keyboard_visible);
    let body_bottom = rows.saturating_sub(bar_height);
    let viewport_height = body_bottom.saturating_sub(body_top);

    // Cursor mapping only matters when the embedded viewport is visible.
    // Hide the host cursor whenever a selector is open so the pane cursor
    // doesn't blink behind the menu. The skip and h_offset computed here
    // MUST match what `ViewportScreen::render` will pick.
    let new_cursor = if state.active == ActiveScreen::Viewport {
        let viewport_lines_len = state.workspace.current_pane_viewport_len();
        let max_v_pan = viewport_lines_len.saturating_sub(viewport_height);
        let v_pan = state.viewport.viewport_v_pan.min(max_v_pan);
        let skip = max_v_pan - v_pan;
        let h_offset = state.viewport.viewport_h_pan;
        state.viewport.compute_cursor_position(
            &state.workspace,
            body_top,
            viewport_height,
            cols,
            skip,
            h_offset,
        )
    } else {
        None
    };
    // FIRST: tell the host where the embedded pane's cursor sits. Routed
    // through `emit_cursor` (deduped) to avoid a render storm.
    state.frame.emit_cursor(new_cursor);

    // Disable DECAWM (autowrap) for the entire plugin paint — see the
    // matching `\x1b[?7h` at the end for rationale.
    print!("\x1b[?7l");

    // Always start the chrome paint clean.
    print!("{}\x1b[2J", RESET);

    if !suppress_top_bar {
        render_top_bar(&state.workspace, &mut state.frame, state.active, 0, cols);
    }

    if body_bottom > body_top {
        match state.active {
            ActiveScreen::Viewport => state.viewport.render(
                &state.workspace,
                &mut state.frame,
                body_top,
                body_bottom,
                cols,
            ),
            ActiveScreen::Sessions => state.sessions.render(
                &mut state.navigation,
                &mut state.frame,
                body_top,
                body_bottom,
                cols,
            ),
            ActiveScreen::Panes => state.panes.render(
                &state.workspace,
                &mut state.navigation,
                &mut state.frame,
                body_top,
                body_bottom,
                cols,
            ),
            ActiveScreen::NewSessionPrompt => state.new_session.render(
                &mut state.frame,
                body_top,
                body_bottom,
                cols,
            ),
        }
    }

    // The dropdown menu paints AFTER the body so its cells overwrite the
    // viewport's right-edge cells where the two overlap. Gated on the
    // Viewport screen being active (selectors occupy the body entirely).
    if state.menu.open && state.active == ActiveScreen::Viewport && body_bottom > body_top {
        state
            .menu
            .render(&state.fit, &mut state.frame, body_top, body_bottom, cols);
    }

    if bar_height > 0 {
        // `state.input.ctrl_held` / `alt_held` are the canonical
        // one-shot modifier flags — `Event::Key` clears them without
        // touching the controller's internal mirror, so reading directly
        // avoids a stale-armed-emphasis bug.
        let armed = modifier_bar::KeyboardModifiers {
            ctrl_armed: state.input.ctrl_held,
            alt_armed: state.input.alt_held,
        };
        modifier_bar::render_modifier_bar(
            &armed,
            body_bottom,
            cols,
            &mut state.frame.click_regions,
        );
    }

    // Re-enable DECAWM. Pairs with the `\x1b[?7l` above.
    print!("\x1b[?7h");
}

fn render_top_bar(ws: &Workspace, frame: &mut Frame, active: ActiveScreen, row: usize, cols: usize) {
    if cols == 0 {
        return;
    }
    // Identical layout in every screen — collapsed viewport, panes
    // selector, sessions selector, and dropdown menu all share this
    // bar. The pane name shown is the currently-selected pane (the
    // one the embedded viewport reads), even while a selector is
    // open, so the user always sees what they would return to.
    render_top_bar_collapsed(ws, frame, active, row, cols);
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
fn render_top_bar_collapsed(ws: &Workspace, frame: &mut Frame, active: ActiveScreen, row: usize, cols: usize) {
    let pane_name = ws
        .current_pane()
        .map(|p| {
            if p.title.is_empty() {
                format!("#{}", p.id)
            } else {
                p.title.clone()
            }
        })
        .unwrap_or_else(|| "—".to_string());
    let session_name = ws.session_name.clone();

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
    let (pane_action, session_action) = if active != ActiveScreen::Viewport {
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
        frame.click_regions.push(region);
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
/// Format a timestamp as `Active <time> ago`, relative to `now`.
/// Returns `"—"` when no activity has been recorded yet (the cache
/// is delta-only, so a freshly-attached client sees `None` for any
/// pane that has not redrawn since attach). The "Active" prefix is
/// dropped in that case because `"Active —"` reads awkwardly.
pub(crate) fn format_time_ago(then_unix_secs: Option<u64>, now_unix_secs: u64) -> String {
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


/// Width of `text` after stripping ANSI escape sequences. Used so the
/// renderer knows how many cells of the row are actually painted.
pub(crate) fn visible_width(text: &str) -> usize {
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
        state.frame.click_regions = regions.clone();
        assert_eq!(
            state.frame.click_to_action(0, 0),
            Some(ClickAction::ExpandPanes)
        );
        assert_eq!(
            state.frame.click_to_action(0, pane_tight_end + 5),
            Some(ClickAction::ToggleMenu),
            "pad cell should fall through to slop hamburger",
        );
        assert_eq!(
            state.frame.click_to_action(0, hamburger_start),
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
        state.frame.click_regions = regions.clone();
        assert_eq!(
            state.frame.click_to_action(0, 0),
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
        state.frame.click_regions = regions;
        // Prefix cell → ExpandPanes.
        assert_eq!(state.frame.click_to_action(0, 3), Some(ClickAction::ExpandPanes));
        // Session cell → ExpandSessions.
        assert_eq!(
            state.frame.click_to_action(0, 9),
            Some(ClickAction::ExpandSessions),
        );
        // Separator cell → ExpandPanes.
        assert_eq!(state.frame.click_to_action(0, 11), Some(ClickAction::ExpandPanes));
        // Pane title cell → ExpandPanes.
        assert_eq!(state.frame.click_to_action(0, 14), Some(ClickAction::ExpandPanes));
        // Pad → slop hamburger.
        assert_eq!(state.frame.click_to_action(0, 30), Some(ClickAction::ToggleMenu));
        // Hamburger glyph → tight hamburger.
        assert_eq!(state.frame.click_to_action(0, 79), Some(ClickAction::ToggleMenu));
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
}
