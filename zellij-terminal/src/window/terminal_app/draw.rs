use crate::render::renderer::Frame;
use crate::render::retained::{self};
use crate::render::scene::{self, BlinkPhase, Margins};
use crate::window::border_rects;
use crate::window::refresh_millihertz;
use crate::window::scene::SceneInputs;
use crate::window::App;
use crate::window::BLINK_INTERVAL;
use crate::window::DISPLAY_RECHECK;
use glutin::prelude::*;
use std::time::Instant;
use zellij_utils::input::terminal::PaddingColor;

impl App {
    pub(in crate::window) fn cursor_options(&self) -> scene::CursorOptions {
        scene::CursorOptions {
            shape: self.options.cursor_shape,
            blink: self.options.cursor_blink,
            hollow: self.options.cursor_unfocused_hollow && !self.focused,
        }
    }

    pub(in crate::window) fn anything_blinks(&self) -> bool {
        self.state.has_blinking_cells()
            || (self.state.cursor_is_visible()
                && self
                    .cursor_options()
                    .blinks(self.state.cursor_is_blinking()))
    }

    pub(in crate::window) fn blink_deadline(&self) -> Instant {
        self.timing.blink_since + BLINK_INTERVAL
    }

    pub(in crate::window) fn advance_blink(&mut self) {
        self.timing.blink_since = Instant::now();
        self.timing.blink = match self.timing.blink {
            BlinkPhase::On => BlinkPhase::Off,
            BlinkPhase::Off => BlinkPhase::On,
        };
        let damage = retained::blink_damage(&self.state, self.cursor_options());
        self.retained.mark(&damage);
    }

    pub(in crate::window) fn schedule_draw(&mut self) {
        self.timing.pacer.schedule();
    }

    pub(in crate::window) fn follow_cursor_area(&mut self) {
        let (row, col) = self.state.cursor_position();
        let area = (
            self.origin.0 + (col as u32 * self.metrics.width) as i32,
            self.origin.1 + (row as u32 * self.metrics.height) as i32,
            self.metrics.width,
            self.metrics.height,
        );
        if self.ime.ime_area == Some(area) {
            return;
        }
        let Some(surfaces) = &self.surfaces else {
            return;
        };
        surfaces.window.set_ime_cursor_area(
            winit::dpi::PhysicalPosition::new(area.0, area.1),
            winit::dpi::PhysicalSize::new(area.2, area.3),
        );
        self.ime.ime_area = Some(area);
    }

    pub(in crate::window) fn follow_display(&mut self) {
        let millihertz = self
            .surfaces
            .as_ref()
            .and_then(|surfaces| refresh_millihertz(&surfaces.window));
        self.timing.pacer.follow_refresh(millihertz);
        self.timing.display_checked = Instant::now();
    }

    pub(in crate::window) fn follow_display_occasionally(&mut self) {
        if Instant::now().duration_since(self.timing.display_checked) < DISPLAY_RECHECK {
            return;
        }
        self.follow_display();
    }

    pub(in crate::window) fn draw(&mut self) {
        if self.surfaces.is_none() {
            return;
        }
        let now = Instant::now();
        self.retained
            .set_held_out(self.scroll.held_out(&self.state));
        self.refresh_scene();
        self.build_scroll_layer(now);
        self.follow_cursor_area();
        let Some(surfaces) = self.surfaces.as_mut() else {
            return;
        };
        let size = surfaces.size;
        let target = (size.width.max(1), size.height.max(1));
        let margins = match self.options.padding_color {
            PaddingColor::Background => Vec::new(),
            PaddingColor::Extend => {
                let grid = self.state.size();
                let (grid_width, grid_height) = (
                    self.metrics.width * grid.cols as u32,
                    self.metrics.height * grid.rows as u32,
                );
                let (left, top) = (self.origin.0.max(0) as u32, self.origin.1.max(0) as u32);
                scene::padding_rects(
                    &self.state,
                    &self.options.paints,
                    self.metrics,
                    self.effective_transparency(),
                    Margins {
                        left,
                        top,
                        right: target.0.saturating_sub(left + grid_width),
                        bottom: target.1.saturating_sub(top + grid_height),
                    },
                )
            },
        };
        let mut margins = margins;
        if let Some(color) = self.appearance.border {
            margins.extend(border_rects(color, self.scale, target, self.origin));
        }
        let Some(surfaces) = self.surfaces.as_mut() else {
            return;
        };
        if !surfaces.context.is_current() {
            if let Err(e) = surfaces.context.make_current(&surfaces.surface) {
                report!(
                    "the window's drawing context could not be made current: {}",
                    e
                );
                return;
            }
        }
        surfaces.renderer.draw_retained(
            &self.retained,
            self.cache.atlases(),
            target,
            Frame {
                origin: self.origin,
                margins: &margins,
                layer: self.scroll.is_active().then_some(&self.layer),
            },
        );
        if let Err(e) = surfaces.surface.swap_buffers(&surfaces.context) {
            report!("buffer swap failed: {}", e);
        }
        self.timing.pacer.drawn(Instant::now());
    }

    pub(in crate::window) fn refresh_scene(&mut self) {
        let cursor = self.cursor_options();
        let preedit = self.ime.composition.shown();
        self.retained
            .set_transparency(self.effective_transparency());
        self.retained.set_contrast(self.options.minimum_contrast);
        self.retained.refresh(
            SceneInputs {
                state: &self.state,
                phase: self.timing.blink,
                paints: &self.options.paints,
                cursor: cursor,
                hovered_link: self.mouse.hovered_link.as_ref(),
                preedit: preedit.as_ref(),
            },
            &mut self.cache,
        );
    }

    pub(in crate::window) fn build_scroll_layer(&mut self, now: Instant) {
        if !self.scroll.is_active() {
            self.layer.clear();
            return;
        }
        let cursor = self.cursor_options();
        let preedit = self.ime.composition.shown();
        self.scroll.build_layer(
            &mut self.layer,
            SceneInputs {
                state: &self.state,
                phase: self.timing.blink,
                paints: &self.options.paints,
                cursor: cursor,
                hovered_link: None,
                preedit: preedit.as_ref(),
            },
            &mut self.cache,
            self.retained.transparency(),
            self.retained.contrast(),
            self.retained.all_images(),
            now,
        );
    }
}
