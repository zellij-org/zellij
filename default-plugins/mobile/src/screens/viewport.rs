//! The embedded pane viewport — the default, collapsed screen. Owns the
//! vertical/horizontal pan offsets and the last-rendered viewport region
//! (used by the mouse handler to reverse-map clicks into pane
//! coordinates). Forwards hardware-keyboard keys straight to the
//! selected pane's pty.

use zellij_tile::prelude::*;

use crate::click::ViewportRegion;
use crate::frame::Frame;
use crate::input::Input;
use crate::keys;
use crate::render::{move_to, slice_ansi_visible, visible_width, RESET};
use crate::workspace::{pane_id_of, Workspace};

/// Embedded viewport state.
#[derive(Default)]
pub struct ViewportScreen {
    /// Rows panned UP from the bottom-anchored default — 0 means
    /// "follow latest". Capped inside `render` against the cached
    /// viewport length.
    pub viewport_v_pan: usize,
    /// Cols panned RIGHT from the left edge — 0 = leftmost. Capped
    /// inside `render` against the pane content width.
    pub viewport_h_pan: usize,
    /// Where the embedded viewport ended up on the most recent render.
    /// Set by `render`; consumed by the mouse handler to dispatch
    /// viewport-passthrough clicks and by `max_viewport_v_pan`.
    pub viewport_region: Option<ViewportRegion>,
}

impl ViewportScreen {
    /// Reset the pan offsets to the bottom-anchored / leftmost default.
    /// Used on every plugin-driven focus change.
    pub fn reset_pan(&mut self) {
        self.viewport_v_pan = 0;
        self.viewport_h_pan = 0;
    }

    /// Forward a hardware-keyboard key to the selected pane's pty.
    /// Sticky modifiers (armed by the modifier bar) are folded in and
    /// then cleared so a user can produce Ctrl+C by arming ⌃ and then
    /// typing `c`.
    pub fn handle_key(&self, ws: &Workspace, input: &mut Input, key: KeyWithModifier) -> bool {
        if let Some(pane) = ws.current_pane() {
            let key = if input.ctrl_held || input.alt_held {
                input.merge_held_modifiers(&key)
            } else {
                key
            };
            let bytes = keys::serialize_key(&key);
            if !bytes.is_empty() {
                write_to_pane_id(bytes, pane_id_of(&pane));
            }
        }
        let consumed = input.ctrl_held || input.alt_held;
        input.ctrl_held = false;
        input.alt_held = false;
        consumed
    }

    /// Maximum legal `viewport_v_pan` for the current embed bounds: the
    /// number of rows we could pan UP from the bottom-anchored default.
    /// Returns `None` when no render has happened yet (no
    /// `viewport_region`), so callers fall back to pure-pan behaviour on
    /// the first event tick.
    pub fn max_viewport_v_pan(&self, ws: &Workspace) -> Option<usize> {
        let region = self.viewport_region?;
        let embed_height = region.row_end.saturating_sub(region.row_start);
        Some(ws.current_pane_viewport_len().saturating_sub(embed_height))
    }

    /// If a click at (row, col) lands inside the most recently rendered
    /// embedded-viewport region, return the equivalent (row, col) in the
    /// underlying pane's viewport coordinates. Returns `None` if outside
    /// the viewport area or no viewport has been rendered yet.
    pub fn click_in_viewport(&self, row: usize, col: usize) -> Option<(usize, usize)> {
        let region = self.viewport_region?;
        if row < region.row_start || row >= region.row_end {
            return None;
        }
        if col >= region.cols {
            return None;
        }
        let pane_row = region.skip + (row - region.row_start);
        let pane_col = region.h_offset + col;
        Some((pane_row, pane_col))
    }

    /// Map the underlying pane's reported cursor coordinates into the
    /// plugin's render coordinates, returning `None` if the cursor is
    /// hidden, off-screen, or no pane is selected. The cursor is read
    /// from `PaneContents.cursor` (refreshed every render cycle) so it
    /// follows typing in real time.
    pub fn compute_cursor_position(
        &self,
        ws: &Workspace,
        viewport_top: usize,
        viewport_height: usize,
        cols: usize,
        skip: usize,
        h_offset: usize,
    ) -> Option<(usize, usize)> {
        if viewport_height == 0 {
            return None;
        }
        let pane = ws.current_pane()?;
        let pane_id = pane_id_of(&pane);
        let (cursor_x, cursor_y) = ws.latest_pane_contents.get(&pane_id)?.cursor?;
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

    /// Render the embedded pane viewport into `[row_start, row_end)`.
    pub fn render(
        &mut self,
        ws: &Workspace,
        _frame: &mut Frame,
        row_start: usize,
        row_end: usize,
        cols: usize,
    ) {
    let height = row_end - row_start;
    if height == 0 {
        return;
    }

    let pane = ws.current_pane();
    let pane_id = pane.as_ref().map(pane_id_of);
    let viewport_lines: Vec<String> = pane_id
        .and_then(|id| ws.latest_pane_contents.get(&id))
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
    let max_v_pan = self
        .max_viewport_v_pan(ws)
        .unwrap_or_else(|| viewport_lines.len().saturating_sub(height));
    self.viewport_v_pan = self.viewport_v_pan.min(max_v_pan);
    let skip = max_v_pan - self.viewport_v_pan;
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
    self.viewport_h_pan = self.viewport_h_pan.min(max_h_pan);
    let h_offset = self.viewport_h_pan;

    // Record where the viewport landed so the mouse handler can
    // reverse-map clicks into pane coordinates. We store this even when
    // we have no cached lines yet, so the user's first viewport tap
    // still maps to row 0 of an eventually-populated cache.
    self.viewport_region = Some(ViewportRegion {
        row_start,
        row_end,
        cols,
        skip,
        h_offset,
    });

    // Render only caches the pane dims (`last_render_rows`/`cols`, set
    // at the top of `render`); the embedded `Size` is computed and
    // pushed to the server from `update()` via `notify_fit_size`.
    // Render itself touches no fit state and issues no host shims (which
    // would corrupt the in-flight frame on stdout).

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::click::ViewportRegion;

    /// Build a `(Workspace, ViewportScreen)` whose `current_pane()`
    /// resolves to a terminal pane with `viewport_len` lines cached, and
    /// whose `viewport_region` (if `Some`) spans rows `[0, embed_height)`.
    fn state_with_viewport(
        viewport_len: usize,
        embed_height: Option<usize>,
    ) -> (Workspace, ViewportScreen) {
        let mut ws = Workspace::default();
        let mut tab = TabInfo::default();
        tab.position = 0;
        ws.tabs.push(tab);
        ws.selected_tab_position = Some(0);

        let mut pane = PaneInfo::default();
        pane.id = 42;
        pane.is_plugin = false;
        pane.is_selectable = true;
        pane.is_suppressed = false;
        ws.panes_by_tab_position.insert(0, vec![pane]);
        ws.selected_pane_id = Some(PaneId::Terminal(42));

        let mut contents = PaneContents::default();
        contents.viewport = vec![String::new(); viewport_len];
        ws.latest_pane_contents.insert(PaneId::Terminal(42), contents);

        let mut vp = ViewportScreen::default();
        if let Some(h) = embed_height {
            vp.viewport_region = Some(ViewportRegion {
                row_start: 0,
                row_end: h,
                cols: 80,
                skip: 0,
                h_offset: 0,
            });
        }
        (ws, vp)
    }

    /// Without a recorded `viewport_region` the embed height is unknown,
    /// so the helper cannot compute a maximum and must return `None`.
    #[test]
    fn max_viewport_v_pan_none_without_region() {
        let (ws, vp) = state_with_viewport(100, None);
        assert_eq!(vp.max_viewport_v_pan(&ws), None);
    }

    /// Standard case: cached viewport taller than the embed area.
    #[test]
    fn max_viewport_v_pan_some_typical() {
        let (ws, vp) = state_with_viewport(100, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(80));
    }

    /// Embed area taller than (or equal to) the cached viewport — no
    /// panning possible, saturates to 0.
    #[test]
    fn max_viewport_v_pan_saturates_when_embed_larger() {
        let (ws, vp) = state_with_viewport(10, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(0));
    }

    /// Empty cache with a region still set saturates to 0.
    #[test]
    fn max_viewport_v_pan_empty_cache() {
        let (ws, vp) = state_with_viewport(0, Some(20));
        assert_eq!(vp.max_viewport_v_pan(&ws), Some(0));
    }
}
