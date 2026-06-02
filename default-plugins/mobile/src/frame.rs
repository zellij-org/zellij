//! Per-frame render scratch and chrome flags shared across screens: the
//! click-region map the renderer rebuilds every frame, the cached render
//! dimensions (needed because the embedded `Size` must be computed in
//! `update()` where no dims are available), the last-emitted cursor
//! payload (for the render-storm guard), and the soft-keyboard
//! visibility that gates the modifier bar.

use zellij_tile::prelude::*;

use crate::click::{slop_key, ClickAction, ClickRegion};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LastEmittedCursor {
    /// No `show_cursor` call has been made yet — the next render must
    /// emit unconditionally so the host's initial cursor state matches
    /// what the plugin has computed.
    #[default]
    Unknown,
    /// The most recent `show_cursor` payload — `None` for "hidden",
    /// `Some((x, y))` for "shown at these plugin-coords".
    Sent(Option<(usize, usize)>),
}

/// Shared render-frame state: tap targets, cached dims, cursor mirror,
/// and the soft-keyboard flag.
#[derive(Default)]
pub struct Frame {
    /// Click regions produced by the most recent render. The renderer
    /// rebuilds this on every `render` call; mouse events look up the
    /// hit region by (row, col).
    pub click_regions: Vec<ClickRegion>,
    /// Dims `render()` was last called with — the plugin pane's content
    /// size, the source of the embedded area reported to the server.
    /// Cached because the embedded `Size` must be computed in `update()`
    /// (shim calls are forbidden in `render()`), which has no dimensions
    /// of its own.
    pub last_render_rows: usize,
    pub last_render_cols: usize,
    /// Last `show_cursor` payload the plugin emitted to the host.
    /// Calling `show_cursor` is *not* idempotent on the server side, so
    /// we cache the last value sent and only re-emit when the target
    /// position would actually change.
    pub last_emitted_cursor: LastEmittedCursor,
    /// Current OS soft-keyboard visibility on the attached web client,
    /// as last reported by the browser via
    /// `Event::SoftKeyboardVisibilityChanged`. Drives `render::render`
    /// to suppress the modifier bar when the keyboard is hidden.
    pub soft_keyboard_visible: bool,
}

impl Frame {
    /// Forward a `show_cursor` call to the host only if it would change
    /// the host's view of the plugin cursor. Without this guard we hit a
    /// render storm: every `ScreenInstruction::ShowPluginCursor` on the
    /// server runs a full `screen.render` + `log_and_report_session_state`
    /// (see `screen.rs::ShowPluginCursor`), which produces a fresh
    /// `PaneRenderReportWithAnsi` for the plugin's subscription, which
    /// drives another plugin render, which calls `show_cursor` again …
    pub fn emit_cursor(&mut self, new_pos: Option<(usize, usize)>) {
        let needs_emit = match self.last_emitted_cursor {
            LastEmittedCursor::Unknown => true,
            LastEmittedCursor::Sent(prev) => prev != new_pos,
        };
        if needs_emit {
            show_cursor(new_pos);
            self.last_emitted_cursor = LastEmittedCursor::Sent(new_pos);
        }
    }

    /// Resolve a click at (row, col) to the action it should fire, if
    /// any.
    ///
    /// Pass 1 scans **tight** regions (priority 0): first hit wins.
    /// Tight regions are guaranteed non-overlapping by the renderer.
    ///
    /// Pass 2 scans **slop** regions (priority > 0). Slop regions may
    /// overlap on shared boundaries; the candidate whose `center` is
    /// closest to the click — by squared Euclidean distance — wins.
    /// Ties break lex-first by `(center_y, center_x)`.
    pub fn click_to_action(&self, row: usize, col: usize) -> Option<ClickAction> {
        // Pass 1: tight regions.
        for region in &self.click_regions {
            if region.priority == 0
                && region.row_start <= row
                && row < region.row_end
                && col >= region.col_start
                && col < region.col_end
            {
                return Some(region.action.clone());
            }
        }
        // Pass 2: slop regions, resolved by nearest-center.
        let mut best: Option<(&ClickRegion, u64)> = None;
        for region in &self.click_regions {
            if region.priority == 0 {
                continue;
            }
            if row < region.row_start || row >= region.row_end {
                continue;
            }
            if col < region.col_start || col >= region.col_end {
                continue;
            }
            let Some((cx, cy)) = region.center else { continue };
            let dx = (cx as i64 - col as i64).unsigned_abs();
            let dy = (cy as i64 - row as i64).unsigned_abs();
            let dist_sq = dx * dx + dy * dy;
            best = Some(match best {
                None => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq < cur_d => (region, dist_sq),
                Some((cur, cur_d)) if dist_sq == cur_d => {
                    let cur_key = slop_key(cur);
                    let new_key = slop_key(region);
                    if new_key < cur_key {
                        (region, dist_sq)
                    } else {
                        (cur, cur_d)
                    }
                },
                Some(prev) => prev,
            });
        }
        best.map(|(r, _)| r.action.clone())
    }
}

/// The two vertical chrome offsets the body layout depends on, given the
/// plugin pane's `rows`: `(body_top, bar_height)`. `body_top` is the top
/// bar (1 row unless suppressed by the welcome flow or the open Sessions
/// selector); `bar_height` is the soft-keyboard modifier bar (1 row
/// while the keyboard is visible, suppressed on a pathologically short
/// body). Single source of truth shared by `render` and `embedded_size`,
/// so the embedded area reported to the server can never drift from what
/// is actually drawn.
pub fn chrome_offsets(
    rows: usize,
    suppress_top_bar: bool,
    soft_keyboard_visible: bool,
) -> (usize, usize) {
    let body_top = if suppress_top_bar { 0 } else { 1 };
    let bar_height = if soft_keyboard_visible && rows.saturating_sub(body_top) >= 2 {
        1
    } else {
        0
    };
    (body_top, bar_height)
}

#[cfg(test)]
mod tests {
    //! Dispatch tests for the layered tight/slop priority system.
    use super::*;
    use crate::modifier_bar::CellId;

    fn kb(id: u16) -> ClickAction {
        ClickAction::Keyboard(CellId(id))
    }

    /// A tight hit on a cell resolves to that cell even if a sibling
    /// cell's slop region also covers the click coordinate.
    #[test]
    fn tight_wins_over_overlapping_slop() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        f.click_regions.push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        f.click_regions.push(ClickRegion::tight(5, 13, 16, kb(2)));
        f.click_regions.push(ClickRegion::slop(5, 12, 17, kb(2), (14, 5)));
        assert_eq!(f.click_to_action(5, 12), Some(kb(1)));
        assert_eq!(f.click_to_action(5, 13), Some(kb(2)));
    }

    /// A click that misses every tight region falls back to slop,
    /// resolved by nearest-center.
    #[test]
    fn slop_resolves_by_nearest_center() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        for r in 4..=6 {
            f.click_regions.push(ClickRegion::slop(r, 9, 14, kb(1), (11, 5)));
        }
        f.click_regions.push(ClickRegion::tight(7, 10, 13, kb(2)));
        for r in 6..=8 {
            f.click_regions.push(ClickRegion::slop(r, 9, 14, kb(2), (11, 7)));
        }
        // Divider row: equidistant; tiebreaker prefers the upper cell.
        assert_eq!(f.click_to_action(6, 11), Some(kb(1)));
        assert_eq!(f.click_to_action(8, 11), Some(kb(2)));
    }

    /// Clicks outside every region return None.
    #[test]
    fn miss_returns_none() {
        let mut f = Frame::default();
        f.click_regions.push(ClickRegion::tight(5, 10, 13, kb(1)));
        f.click_regions.push(ClickRegion::slop(5, 9, 14, kb(1), (11, 5)));
        assert!(f.click_to_action(0, 0).is_none());
        assert!(f.click_to_action(5, 20).is_none());
    }
}
