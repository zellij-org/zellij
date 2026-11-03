use crate::window::notice::Notice;
use crate::window::App;
use winit::event::MouseScrollDelta;
use winit::keyboard::{Key, NamedKey};

impl App {
    pub(in crate::window) fn show_notice(&mut self, message: &str) {
        let size = self.state.size();
        self.notice = Some(Notice::new(message));
        self.cancel_scroll_animations();
        self.stop_momentum();
        self.paint_notice(size.rows, size.cols);
    }

    pub(in crate::window) fn paint_notice(&mut self, rows: usize, cols: usize) {
        let Some(notice) = self.notice.as_mut() else {
            return;
        };
        let frame = notice.frame(rows, cols);
        if let Err(e) = self.state.apply_frame(&frame) {
            report!("the message could not be drawn: {}", e);
            return;
        }
        crate::window::notice::mark_shown();
        self.retained.mark_everything();
        self.schedule_draw();
    }

    pub(in crate::window) fn scroll_notice(
        &mut self,
        scroll: impl FnOnce(&mut Notice, usize, usize) -> bool,
    ) {
        let size = self.state.size();
        let Some(notice) = self.notice.as_mut() else {
            return;
        };
        if scroll(notice, size.rows, size.cols) {
            self.paint_notice(size.rows, size.cols);
        }
    }

    pub(in crate::window) fn notice_key(&mut self, key: &Key) -> bool {
        let page = Notice::page(self.state.size().rows);
        let by = match key {
            Key::Named(NamedKey::Enter | NamedKey::Escape) => return true,
            Key::Named(NamedKey::ArrowUp) => -1,
            Key::Named(NamedKey::ArrowDown) => 1,
            Key::Named(NamedKey::PageUp) => -page,
            Key::Named(NamedKey::PageDown) => page,
            Key::Named(NamedKey::Home) => -isize::MAX,
            Key::Named(NamedKey::End) => isize::MAX,
            _ => return false,
        };
        self.scroll_notice(|notice, rows, cols| notice.scroll(by, rows, cols));
        false
    }

    pub(in crate::window) fn notice_wheel(&mut self, delta: MouseScrollDelta) {
        let lines = match delta {
            MouseScrollDelta::LineDelta(_, y) => crate::window::notice::wheel_lines(y),
            MouseScrollDelta::PixelDelta(pixels) => -pixels.y / self.metrics.height.max(1) as f64,
        };
        self.scroll_notice(|notice, rows, cols| notice.wheel(lines, rows, cols));
    }
}
