//! Fit-to-screen state. A server-side per-tab override resizes the
//! focused pane's tab to match this plugin's embedded viewport area,
//! fullscreening the pane. The server holds the authoritative state
//! (`Screen::fit_states`); `Fit` is the plugin's mirror so the next tap
//! takes the off path and the renderer can colour the glyph.

use zellij_tile::prelude::*;

use crate::frame::{chrome_offsets, Frame};
use crate::workspace::{pane_id_of, Workspace};

/// Plugin-side mirror of the active fit override.
#[derive(Default)]
pub struct Fit {
    /// True while Fit mode is active (the ⛶ glyph is "armed"). Reset to
    /// `false` when the user picks a different tab or pane, or when the
    /// previously-selected pane disappears.
    pub active: bool,
    /// The tab the local fit is bound to. Set when a fit is armed;
    /// cleared by every path that clears `active`. Threaded through
    /// `set_tab_fit` so the server can look up the override entry by
    /// tab_id.
    pub tab_id: Option<usize>,
    /// The embedded `Size` most recently pushed to the server for the
    /// active fit. Lets `notify_size` dedupe redundant pushes when
    /// `update()` reconciles on every event.
    pub last_sent_size: Option<Size>,
}

impl Fit {
    /// Toggle the fit override through the server. On entry we need the
    /// focused pane (the pane to fit) and its tab; if either is missing
    /// we silently bail. Returns whether to re-render.
    pub fn toggle(
        &mut self,
        ws: &Workspace,
        frame: &Frame,
        suppress_top_bar: bool,
    ) -> bool {
        if self.active {
            // `tab_id` is ignored by the server's clear path (it looks
            // the fit up by client), but pass the real one.
            let tab_id = self.tab_id.unwrap_or_default();
            self.active = false;
            self.tab_id = None;
            self.last_sent_size = None;
            set_tab_fit(tab_id, None);
            true
        } else {
            let Some(pane) = ws.current_pane() else {
                return false;
            };
            let Some(tab) = ws.current_tab().cloned() else {
                return false;
            };
            self.active = true;
            self.tab_id = Some(tab.tab_id);
            let size = embedded_size(frame, suppress_top_bar);
            set_tab_fit(tab.tab_id, Some((pane_id_of(&pane), size)));
            self.last_sent_size = Some(size);
            true
        }
    }

    /// Clear an armed fit, telling the server to drop the override (and
    /// revert any fit-induced fullscreen). Used by flows that
    /// invalidate a fit but where the server is NOT already tearing it
    /// down on its own — explicit pane/tab selection and new-pane/
    /// new-tab creation.
    pub fn clear_if_active(&mut self) {
        if self.active {
            let tab_id = self.tab_id.unwrap_or_default();
            self.active = false;
            self.tab_id = None;
            self.last_sent_size = None;
            set_tab_fit(tab_id, None);
        }
    }

    /// Local-only reset of the fit mirror, with no `set_tab_fit` shim.
    /// Used when the server already drops its own override (e.g. the
    /// target pane closed) and only the plugin's mirror needs clearing.
    pub fn reset_local(&mut self) {
        self.active = false;
        self.tab_id = None;
        self.last_sent_size = None;
    }

    /// Push the current embedded `Size` to the server for the active
    /// fit, deduped against the last push. Called from a single
    /// reconcile point at the end of `update()`, so it covers every
    /// cause of a size change. No-op when fit is inactive, `tab_id` is
    /// unset, no pane resolves, or the size is unchanged.
    pub fn notify_size(
        &mut self,
        ws: &Workspace,
        frame: &Frame,
        suppress_top_bar: bool,
    ) {
        if !self.active {
            return;
        }
        let Some(tab_id) = self.tab_id else {
            return;
        };
        let Some(pane) = ws.current_pane() else {
            return;
        };
        let size = embedded_size(frame, suppress_top_bar);
        if self.last_sent_size == Some(size) {
            return;
        }
        set_tab_fit(tab_id, Some((pane_id_of(&pane), size)));
        self.last_sent_size = Some(size);
    }
}

/// The exact embedded content `Size` the plugin draws the pane into:
/// the cached plugin-pane dims minus the vertical chrome (top bar +
/// soft-keyboard bar). The server grows this by the target tab's bars
/// and the target pane's frame so the pane content rectangle matches it
/// exactly. Shares `chrome_offsets` with the renderer so the reported
/// area can never drift from what is actually drawn.
pub fn embedded_size(frame: &Frame, suppress_top_bar: bool) -> Size {
    let (body_top, bar_height) = chrome_offsets(
        frame.last_render_rows,
        suppress_top_bar,
        frame.soft_keyboard_visible,
    );
    Size {
        rows: frame
            .last_render_rows
            .saturating_sub(bar_height)
            .saturating_sub(body_top),
        cols: frame.last_render_cols,
    }
}

#[cfg(test)]
mod tests {
    //! `embedded_size` math and the `Fit` clear/notify paths. Shim calls
    //! resolve to the native-build stubs (see `zellij-tile/src/shim.rs`),
    //! so the tests observe state mutation only.
    use super::*;

    fn frame_with_dims(rows: usize, cols: usize, soft_keyboard: bool) -> Frame {
        let mut frame = Frame::default();
        frame.last_render_rows = rows;
        frame.last_render_cols = cols;
        frame.soft_keyboard_visible = soft_keyboard;
        frame
    }

    /// Resting state: the title bar takes one row, no soft keyboard —
    /// embedded area is the pane minus the top row.
    #[test]
    fn embedded_size_default_top_bar_only() {
        let frame = frame_with_dims(20, 80, false);
        assert_eq!(embedded_size(&frame, false), Size { rows: 19, cols: 80 });
    }

    /// Soft keyboard visible reserves the modifier-bar row at the bottom.
    #[test]
    fn embedded_size_soft_keyboard_adds_bottom() {
        let frame = frame_with_dims(20, 80, true);
        assert_eq!(embedded_size(&frame, false), Size { rows: 18, cols: 80 });
    }

    /// A suppressed top bar (welcome flow / open Sessions selector) frees
    /// the top row for the body.
    #[test]
    fn embedded_size_suppressed_top_bar() {
        let frame = frame_with_dims(20, 80, false);
        assert_eq!(embedded_size(&frame, true), Size { rows: 20, cols: 80 });
    }

    /// `notify_size` is a no-op when fit is inactive or `tab_id` is unset
    /// (the shim must not be addressed without a target tab).
    #[test]
    fn notify_size_gated_off() {
        let ws = Workspace::default();
        let frame = frame_with_dims(20, 80, false);
        let mut fit = Fit::default();
        fit.active = false;
        fit.tab_id = Some(7);
        fit.notify_size(&ws, &frame, false);
        assert_eq!(fit.last_sent_size, None);
        // active but no tab id
        fit.active = true;
        fit.tab_id = None;
        fit.notify_size(&ws, &frame, false);
        assert!(fit.active);
        assert_eq!(fit.last_sent_size, None);
    }

    /// `clear_if_active` resets both fit fields; calling it again while
    /// inactive is a no-op (required because dispatch paths invoke it
    /// unconditionally on tab/pane switch).
    #[test]
    fn clear_if_active_round_trip() {
        let mut fit = Fit::default();
        fit.active = true;
        fit.tab_id = Some(7);
        fit.clear_if_active();
        assert!(!fit.active);
        assert_eq!(fit.tab_id, None);
        fit.clear_if_active();
        assert!(!fit.active);
        assert_eq!(fit.tab_id, None);
    }
}
