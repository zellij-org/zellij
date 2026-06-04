use zellij_tile::prelude::*;

use crate::frame::{chrome_offsets, Frame};
use crate::workspace::{pane_id_of, Workspace};

#[derive(Default)]
pub struct Fit {
    pub active: bool,
    pub tab_id: Option<usize>,
    pub last_sent_size: Option<Size>,
}

impl Fit {
    pub fn toggle(
        &mut self,
        ws: &Workspace,
        frame: &Frame,
        suppress_top_bar: bool,
    ) -> bool {
        if self.active {
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

    pub fn clear_if_active(&mut self) {
        if self.active {
            let tab_id = self.tab_id.unwrap_or_default();
            self.active = false;
            self.tab_id = None;
            self.last_sent_size = None;
            set_tab_fit(tab_id, None);
        }
    }

    pub fn reset_local(&mut self) {
        self.active = false;
        self.tab_id = None;
        self.last_sent_size = None;
    }

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
    use super::*;

    fn frame_with_dims(rows: usize, cols: usize, soft_keyboard: bool) -> Frame {
        let mut frame = Frame::default();
        frame.last_render_rows = rows;
        frame.last_render_cols = cols;
        frame.soft_keyboard_visible = soft_keyboard;
        frame
    }

    #[test]
    fn embedded_size_default_top_bar_only() {
        let frame = frame_with_dims(20, 80, false);
        assert_eq!(embedded_size(&frame, false), Size { rows: 19, cols: 80 });
    }

    #[test]
    fn embedded_size_soft_keyboard_adds_bottom() {
        let frame = frame_with_dims(20, 80, true);
        assert_eq!(embedded_size(&frame, false), Size { rows: 18, cols: 80 });
    }

    #[test]
    fn embedded_size_suppressed_top_bar() {
        let frame = frame_with_dims(20, 80, false);
        assert_eq!(embedded_size(&frame, true), Size { rows: 20, cols: 80 });
    }

    #[test]
    fn notify_size_gated_off() {
        let ws = Workspace::default();
        let frame = frame_with_dims(20, 80, false);
        let mut fit = Fit::default();
        fit.active = false;
        fit.tab_id = Some(7);
        fit.notify_size(&ws, &frame, false);
        assert_eq!(fit.last_sent_size, None);
        fit.active = true;
        fit.tab_id = None;
        fit.notify_size(&ws, &frame, false);
        assert!(fit.active);
        assert_eq!(fit.last_sent_size, None);
    }

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
