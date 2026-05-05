//! Rendering for the mobile plugin's v1 UI. The plugin lays out three
//! horizontal regions stacked top-to-bottom:
//!
//! 1. **Breadcrumb / selector** — collapsed view shows a single
//!    breadcrumb line ("tabs/<tab> · panes/<pane>"); expanded view
//!    shows the corresponding selector list.
//! 2. **Action bar** — always visible, always one row in v1. Tapping
//!    `⌨ Type` arms typing-mode (Stage 7 will route keys); other
//!    buttons map to the existing plugin shim helpers.
//! 3. **Embedded viewport** — slice of the latest ANSI viewport for
//!    the selected pane, occupying the remaining rows.
//!
//! The renderer also rebuilds `state.click_regions` so the input
//! handler can dispatch a `Mouse::LeftClick` to the right action.

use crate::state::{pane_id_of, ClickAction, ClickRegion, Selector, State};
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
pub fn render_stub(rows: usize, cols: usize) {
    print!("{}{}mobile plugin loaded \u{2014} {}x{}", RESET, move_to(0, 0), rows, cols);
}

/// Top-level render. Clears the screen, builds the chrome, paints the
/// embedded viewport, and rewrites `state.click_regions` for input
/// dispatch.
pub fn render(state: &mut State, rows: usize, cols: usize) {
    state.click_regions.clear();

    // Always start clean — `\x1b[2J` clears the visible area and we
    // rewrite each region from (0, 0).
    print!("{}\x1b[2J", RESET);

    if rows < 4 || cols < 8 {
        // No room for a meaningful UI — degrade to the stub.
        print!("{}{}mobile {}x{}", RESET, move_to(0, 0), rows, cols);
        return;
    }

    let action_bar_row = rows.saturating_sub(1);
    let viewport_top: usize;
    let viewport_bottom: usize; // exclusive

    match state.expanded {
        None => {
            // Collapsed: row 0 = breadcrumb, row 1 = action bar separator,
            // row N-1 = action bar (already at action_bar_row).
            // Viewport occupies rows 1..action_bar_row.
            render_breadcrumb(state, 0, cols);
            viewport_top = 1;
            viewport_bottom = action_bar_row;
        },
        Some(Selector::Tabs) => {
            let selector_rows = compute_selector_rows(state, rows, Selector::Tabs);
            render_tab_selector(state, 0, selector_rows, cols);
            // Below the selector, draw a one-row breadcrumb so the user
            // can see what is currently selected without collapsing.
            render_breadcrumb(state, selector_rows, cols);
            viewport_top = selector_rows + 1;
            viewport_bottom = action_bar_row;
        },
        Some(Selector::Panes) => {
            let selector_rows = compute_selector_rows(state, rows, Selector::Panes);
            render_pane_selector(state, 0, selector_rows, cols);
            render_breadcrumb(state, selector_rows, cols);
            viewport_top = selector_rows + 1;
            viewport_bottom = action_bar_row;
        },
    }

    if viewport_bottom > viewport_top {
        render_embedded_viewport(state, viewport_top, viewport_bottom, cols);
    }

    render_action_bar(state, action_bar_row, cols);
}

fn compute_selector_rows(state: &State, rows: usize, selector: Selector) -> usize {
    // Cap at 6 (per plan). Leave at least one row for breadcrumb,
    // one for action bar, and one for viewport.
    let max_for_selector = rows.saturating_sub(3).min(6).max(1);
    let item_count = match selector {
        Selector::Tabs => state.tabs_in_order().len(),
        Selector::Panes => state.current_tab_panes().len(),
    };
    // Always reserve at least 2 rows (header + at least one item).
    let needed = (item_count + 1).max(2);
    needed.min(max_for_selector + 1)
}

fn render_breadcrumb(state: &mut State, row: usize, cols: usize) {
    let tab_name = state
        .current_tab()
        .map(|t| t.name.clone())
        .unwrap_or_else(|| "—".to_string());
    let pane_name = state
        .current_pane()
        .map(|p| if p.title.is_empty() { format!("#{}", p.id) } else { p.title.clone() })
        .unwrap_or_else(|| "—".to_string());

    let tabs_label = format!("[tabs/{}]", tab_name);
    let panes_label = format!("[panes/{}]", pane_name);
    let separator = " · ";

    let mut col = 0;
    print!("{}{}", RESET, move_to(row, col));
    print_clipped(&tabs_label, &mut col, cols);
    state.click_regions.push(ClickRegion {
        row,
        col_start: 0,
        col_end: col,
        action: ClickAction::ExpandTabs,
    });

    let sep_start = col;
    print_clipped(separator, &mut col, cols);
    let _ = sep_start;

    let panes_start = col;
    print_clipped(&panes_label, &mut col, cols);
    state.click_regions.push(ClickRegion {
        row,
        col_start: panes_start,
        col_end: col,
        action: ClickAction::ExpandPanes,
    });

    if state.expanded.is_some() {
        // Right-aligned "collapse" affordance.
        let label = " [collapse]";
        let label_w = UnicodeWidthStr::width(label);
        if cols > label_w + 1 {
            let target_col = cols - label_w;
            print!("{}", move_to(row, target_col));
            print!("{}{}", RESET, label);
            state.click_regions.push(ClickRegion {
                row,
                col_start: target_col,
                col_end: target_col + label_w,
                action: ClickAction::Collapse,
            });
        }
    }
}

fn render_tab_selector(state: &mut State, row_start: usize, row_count: usize, cols: usize) {
    if row_count == 0 {
        return;
    }
    let header = " Select Tab ";
    print!("{}{}", RESET, move_to(row_start, 0));
    print!("\x1b[7m{:^width$}\x1b[0m", header, width = cols);

    // Collect (position, name) into an owned vec so the inner loop
    // can mutate `state.click_regions` without overlapping the
    // immutable borrow from `tabs_in_order`.
    let tab_rows: Vec<(usize, String)> = state
        .tabs_in_order()
        .into_iter()
        .map(|t| (t.position, t.name.clone()))
        .collect();
    let active_position = state
        .current_tab()
        .map(|t| t.position)
        .unwrap_or(usize::MAX);

    let max_items = row_count.saturating_sub(1);
    for (idx, (position, name)) in tab_rows.into_iter().take(max_items).enumerate() {
        let row = row_start + 1 + idx;
        let mark = if position == active_position { "▸" } else { " " };
        let line = format!(" {} {}. {}", mark, position + 1, name);
        print!("{}{}", RESET, move_to(row, 0));
        let mut col = 0;
        print_clipped(&line, &mut col, cols);
        // Clear the rest of the row so a wider previous tab name is
        // overwritten cleanly.
        clear_to_end(col, cols);
        state.click_regions.push(ClickRegion {
            row,
            col_start: 0,
            col_end: cols,
            action: ClickAction::SelectTab(position),
        });
    }
}

fn render_pane_selector(state: &mut State, row_start: usize, row_count: usize, cols: usize) {
    if row_count == 0 {
        return;
    }
    let header = " Select Pane ";
    print!("{}{}", RESET, move_to(row_start, 0));
    print!("\x1b[7m{:^width$}\x1b[0m", header, width = cols);

    // Snapshot panes/selected id outside the borrow so we can mutate
    // click_regions while iterating.
    let panes: Vec<PaneInfo> = state.current_tab_panes().into_iter().cloned().collect();
    let selected_pane_id = state.current_pane().map(|p| pane_id_of(&p));

    let max_items = row_count.saturating_sub(1);
    for (idx, pane) in panes.iter().take(max_items).enumerate() {
        let row = row_start + 1 + idx;
        let id = pane_id_of(pane);
        let mark = if Some(id) == selected_pane_id { "▸" } else { " " };
        let layer = if pane.is_floating { "F" } else { "T" };
        let title = if pane.title.is_empty() {
            format!("#{}", pane.id)
        } else {
            pane.title.clone()
        };
        let line = format!(" {} [{}] {}", mark, layer, title);
        print!("{}{}", RESET, move_to(row, 0));
        let mut col = 0;
        print_clipped(&line, &mut col, cols);
        clear_to_end(col, cols);
        state.click_regions.push(ClickRegion {
            row,
            col_start: 0,
            col_end: cols,
            action: ClickAction::SelectPane(id),
        });
    }
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

    // Reset before each row to keep the chrome's styling separate from
    // the pane's emitted SGR runs.
    for i in 0..height {
        let row = row_start + i;
        print!("{}{}", RESET, move_to(row, 0));
        if let Some(line) = viewport_lines.get(i) {
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
            .get(i)
            .map(|l| visible_width(l))
            .unwrap_or(0);
        if printed_width < cols {
            print!("{}\x1b[K", RESET);
        }
    }
}

fn render_action_bar(state: &mut State, row: usize, cols: usize) {
    // Compact one-row layout. Each button reserves a fixed width so
    // the click regions remain stable across renders.
    let mut buttons: Vec<(String, ClickAction)> = vec![
        (
            if state.typing_mode { "[\u{2328}*]".to_string() } else { "[\u{2328}]".to_string() },
            ClickAction::ToggleType,
        ),
        ("[+P]".into(), ClickAction::NewPane),
        ("[+T]".into(), ClickAction::NewTab),
        ("[\u{2192}]".into(), ClickAction::SplitRight),
        ("[\u{2193}]".into(), ClickAction::SplitDown),
        ("[\u{229E}]".into(), ClickAction::ToggleFloating),
        ("[\u{2715}]".into(), ClickAction::CloseFocus),
        ("[\u{23CF}]".into(), ClickAction::Detach),
        ("[Exit]".into(), ClickAction::ExitMobile),
    ];

    // Reverse-video the bar so it visually separates from the
    // viewport.
    print!("{}{}\x1b[7m", RESET, move_to(row, 0));
    let mut col = 0;
    while col < cols {
        print!(" ");
        col += 1;
    }
    print!("{}", RESET);

    let mut col = 0;
    for (label, action) in buttons.drain(..) {
        let label_w = UnicodeWidthStr::width(label.as_str());
        if col + label_w + 1 > cols {
            break;
        }
        print!("{}{}", move_to(row, col), label);
        state.click_regions.push(ClickRegion {
            row,
            col_start: col,
            col_end: col + label_w,
            action,
        });
        col += label_w + 1; // 1-cell gutter
    }
    print!("{}", RESET);
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
