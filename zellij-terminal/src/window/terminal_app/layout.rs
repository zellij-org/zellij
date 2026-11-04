use crate::config::window_state::{self, Shown, WindowState};
use crate::render::atlas::GlyphCache;
use crate::render::scene::Margins;
use crate::session::connection::{resize, Geometry};
use crate::window::fit_grid;
use crate::window::has_no_area;
use crate::window::App;
use crate::window::GridFit;

impl App {
    pub(in crate::window) fn rescale(&mut self, scale: f64) {
        if !(scale.is_finite() && scale > 0.0) || scale == self.scale {
            return;
        }
        self.rebuild_fonts(scale);
    }

    pub(in crate::window) fn rebuild_fonts(&mut self, scale: f64) -> bool {
        self.rebuild_fonts_zoomed(scale, self.zoom)
    }

    pub(in crate::window) fn rebuild_fonts_zoomed(&mut self, scale: f64, zoom: f64) -> bool {
        if !self.rebuild_fonts_quietly(scale, zoom) {
            return false;
        }
        let size = self
            .surfaces
            .as_ref()
            .map(|surfaces| surfaces.window.inner_size());
        if let Some(size) = size {
            self.relayout(size.width, size.height);
        }
        true
    }

    pub(in crate::window) fn rebuild_fonts_quietly(&mut self, scale: f64, zoom: f64) -> bool {
        match self.options.font.stack(scale * zoom) {
            Ok(fonts) => {
                self.cancel_scroll_animations();
                self.stop_momentum();
                self.mouse.selecting = false;
                self.set_selection(None);
                self.metrics = fonts.metrics();
                self.cache = GlyphCache::new(fonts);
                self.scale = scale;
                self.zoom = zoom;
            },
            Err(e) => {
                report!(
                    "keeping the font already in place; {} px is unusable: {}",
                    self.options.font.size * (scale * zoom) as f32,
                    e
                );
                return false;
            },
        }
        true
    }

    pub(in crate::window) fn relayout(&mut self, width: u32, height: u32) {
        self.cancel_scroll_animations();
        self.retained.mark_everything();
        self.schedule_draw();
        self.reflow(width, height);
    }

    pub(in crate::window) fn rescale_keeping_cells(
        &mut self,
        scale: f64,
        writer: &mut winit::event::InnerSizeWriter,
    ) {
        self.stop_momentum();
        if !(scale.is_finite() && scale > 0.0) || scale == self.scale {
            return;
        }
        let (windowed, cells) = (
            self.placement.shown == Shown::Windowed,
            self.placement.windowed_cells,
        );
        if !self.rebuild_fonts_quietly(scale, self.zoom) {
            return;
        }
        if windowed && self.surfaces.is_some() {
            let exact = self.exact_size(cells.0, cells.1);
            if writer.request_inner_size(exact).is_ok() {
                self.relayout(exact.width, exact.height);
                return;
            }
        }
        if let Some(size) = self
            .surfaces
            .as_ref()
            .map(|surfaces| surfaces.window.inner_size())
        {
            self.relayout(size.width, size.height);
        }
    }

    pub(crate) fn edge_margins(&self) -> Margins {
        self.padding_px()
    }

    pub(crate) fn header_is_blank_at(&self, position: winit::dpi::PhysicalPosition<f64>) -> bool {
        let x = position.x - self.origin.0 as f64;
        let y = position.y - self.origin.1 as f64;
        if x < 0.0 || y < 0.0 {
            return false;
        }
        let row = (y / self.metrics.height.max(1) as f64) as usize;
        let col = (x / self.metrics.width.max(1) as f64) as usize;
        let size = self.state.size();
        if row != 0 || col >= size.cols {
            return false;
        }
        let blank = |col: usize| self.state.screen().cell(0, col).character() == ' ';
        blank(col) && (col == 0 || blank(col - 1)) && (col + 1 >= size.cols || blank(col + 1))
    }

    pub(crate) fn pointer_back_to_normal(&mut self) {
        self.refresh_pointer();
    }

    pub(in crate::window) fn padding_px(&self) -> Margins {
        let pixels = |logical: f32| (logical as f64 * self.scale).round().max(0.0) as u32;
        let padding = self.options.padding;
        Margins {
            left: pixels(padding.left),
            top: pixels(padding.top),
            right: pixels(padding.right),
            bottom: pixels(padding.bottom),
        }
    }

    pub(in crate::window) fn exact_size(
        &self,
        cols: usize,
        rows: usize,
    ) -> winit::dpi::PhysicalSize<u32> {
        let padding = self.padding_px();
        winit::dpi::PhysicalSize::new(
            self.metrics.width * cols as u32 + padding.left + padding.right,
            self.metrics.height * rows as u32 + padding.top + padding.bottom,
        )
    }

    pub(in crate::window) fn settle_startup_size(&mut self) {
        if self.placement.shown != Shown::Windowed {
            return;
        }
        let exact = self.exact_size(self.placement.startup.cols, self.placement.startup.rows);
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        if surfaces.window.inner_size() == exact {
            return;
        }
        let applied = surfaces.window.request_inner_size(exact);
        if let Some(size) = applied {
            self.resized(size.width, size.height);
        }
    }

    pub(in crate::window) fn zoom_by(&mut self, factor: f64) {
        self.set_zoom(self.zoom * factor);
    }

    pub(in crate::window) fn set_zoom(&mut self, zoom: f64) {
        if !(zoom.is_finite() && zoom > 0.0) || zoom == self.zoom {
            return;
        }
        self.rebuild_fonts_zoomed(self.scale, zoom);
    }

    pub(in crate::window) fn grid(&self, width: u32, height: u32) -> GridFit {
        fit_grid(
            (width, height),
            (self.metrics.width, self.metrics.height),
            self.padding_px(),
            self.options.padding_balance,
        )
    }

    pub(in crate::window) fn fitted(&self, width: u32, height: u32) -> Geometry {
        let grid = self.grid(width, height);
        Geometry {
            rows: grid.rows,
            cols: grid.cols,
            cell_width: self.metrics.width as usize,
            cell_height: self.metrics.height as usize,
        }
    }

    pub(in crate::window) fn resized(&mut self, width: u32, height: u32) {
        if has_no_area(width, height) {
            return;
        }
        self.cancel_scroll_animations();
        self.stop_momentum();
        self.observe_window();
        if self.placement.shown == Shown::Windowed {
            let fitted = self.fitted(width, height);
            self.placement.windowed_cells = (fitted.cols, fitted.rows);
            self.placement.windowed_known = true;
        }
        self.share_window_state();
        self.reflow(width, height);
    }

    pub(in crate::window) fn observe_window(&mut self) {
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        let window = &surfaces.window;
        if window.is_minimized() == Some(true) {
            return;
        }
        self.placement.shown = if window.fullscreen().is_some() {
            Shown::Fullscreen
        } else if window.is_maximized() {
            Shown::Maximized
        } else {
            Shown::Windowed
        };
        if self.placement.shown == Shown::Windowed {
            let size = window.inner_size();
            let fitted = self.fitted(size.width, size.height);
            self.placement.windowed_cells = (fitted.cols, fitted.rows);
            self.placement.windowed_known = true;
        }
        self.share_window_state();
    }

    pub(in crate::window) fn window_state(&self) -> WindowState {
        WindowState {
            cols: self.placement.windowed_cells.0,
            rows: self.placement.windowed_cells.1,
            state: self.placement.shown,
        }
    }

    pub(in crate::window) fn remember(&mut self) {
        let Some(path) = self.placement.state_path.clone() else {
            return;
        };
        self.observe_window();
        window_state::store(
            &path,
            window_state::to_remember(self.window_state(), self.placement.windowed_known, &path),
        );
    }

    pub(in crate::window) fn share_window_state(&self) {
        self.placement.remembered.share(
            self.placement.state_path.as_ref(),
            || self.window_state(),
            self.placement.windowed_known,
        );
    }

    pub(in crate::window) fn reflow(&mut self, width: u32, height: u32) {
        if has_no_area(width, height) {
            return;
        }
        if let Some(surfaces) = &mut self.surfaces {
            surfaces.size = winit::dpi::PhysicalSize::new(width, height);
        }
        self.mouse.selecting = false;
        self.set_selection(None);
        let grid = self.grid(width, height);
        if grid.origin != self.origin {
            self.origin = grid.origin;
            self.mouse.pointer.set_origin(grid.origin.0, grid.origin.1);
            self.retained.mark_everything();
            self.schedule_draw();
        }
        let next = self.fitted(width, height);
        self.state
            .set_cell_size(self.metrics.width, self.metrics.height);
        if self.notice.is_some() {
            self.paint_notice(next.rows, next.cols);
        }
        if let Some(sender) = &self.sender {
            if let Err(e) = resize(sender, &self.geometry, next) {
                report!("failed to report a resize: {}", e);
            }
        }
    }
}
