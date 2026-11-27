use crate::config::options::Change;
use crate::config::settings::Settings;
use crate::emulation::palette;
use crate::platform::blur;
use crate::render::scene::Transparency;
use crate::render::scroll_animation::ScrollSettings;
use crate::window::set_option_as_alt;
use crate::window::set_see_through;
use crate::window::App;
use crate::window::WindowLook;
use std::time::Instant;

impl App {
    pub(in crate::window) fn reconfigured(&mut self, settings: Settings) {
        self.settings = settings;
        let zoom_reset = self.zoom != 1.0;
        self.zoom = 1.0;
        self.reapply(zoom_reset);
    }

    pub(in crate::window) fn settings_pushed(&mut self, settings: Settings) {
        let font_size_changed = settings.section.font_size != self.settings.section.font_size;
        self.settings = settings;
        let zoom_reset = font_size_changed && self.zoom != 1.0;
        if zoom_reset {
            self.zoom = 1.0;
        }
        self.reapply(zoom_reset);
    }

    pub(in crate::window) fn cancel_scroll_animations(&mut self) {
        if self.scroll.is_active() {
            self.scroll.cancel();
            self.schedule_draw();
        }
    }

    pub(in crate::window) fn scroll_settings(&self) -> ScrollSettings {
        ScrollSettings {
            enabled: self.options.smooth_scrolling,
            duration: self.options.scroll_animation,
        }
    }

    pub(in crate::window) fn advance_scroll_animations(&mut self, now: Instant) {
        if !self.scroll.is_active() {
            return;
        }
        self.scroll.retire(now);
        self.schedule_draw();
    }

    pub(in crate::window) fn reapply(&mut self, zoom_reset: bool) {
        self.cancel_scroll_animations();
        self.stop_momentum();
        let mut next = crate::config::options::resolve(&self.settings, self.theme_mode);
        if matches!(self.appearance.look, WindowLook::Popup { .. }) {
            crate::launch::panel_look(&mut next);
        }
        let change = Change::between(&self.options, &next);
        if !change.is_anything() && !zoom_reset {
            return;
        }
        let previous_font = self.options.font.clone();
        self.options = next;

        if change.paints || change.minimum_contrast {
            self.retained.forget_contrast();
        }
        if change.paints {
            for msg in palette::seed_messages(&self.options.paints) {
                if let Err(e) = self.tell(msg) {
                    report!("failed to re-declare a color: {}", e);
                }
            }
        }
        if change.fonts || zoom_reset {
            if !self.rebuild_fonts(self.scale) {
                self.options.font = previous_font;
                if zoom_reset {
                    self.rebuild_fonts(self.scale);
                }
            }
        } else if change.layout {
            let size = self
                .surfaces
                .as_ref()
                .map(|surfaces| surfaces.window.inner_size());
            if let Some(size) = size {
                self.relayout(size.width, size.height);
            }
        } else if change.needs_redraw() {
            self.retained.mark_everything();
            self.schedule_draw();
        }
        if change.open_links {
            self.refresh_pointer();
        }
        if change.selection && !self.options.shift_drag_selects {
            self.mouse.selecting = false;
            self.set_selection(None);
        }
        if change.transparency {
            self.warn_if_opaque();
            if let Some(surfaces) = &self.surfaces {
                set_see_through(&surfaces.window, self.see_through());
            }
        }
        if change.blur {
            if let Some(surfaces) = &self.surfaces {
                blur::set(&surfaces.window, self.options.blur);
            }
        }
        if change.option_as_alt {
            if let Some(surfaces) = &self.surfaces {
                set_option_as_alt(&surfaces.window, self.options.option_as_alt);
            }
        }
    }

    pub(in crate::window) fn effective_transparency(&self) -> Transparency {
        if self.appearance.transparency_available {
            self.options.transparency
        } else {
            Transparency::OPAQUE
        }
    }

    pub(in crate::window) fn see_through(&self) -> bool {
        self.effective_transparency().opacity < 1.0
    }

    pub(in crate::window) fn warn_if_opaque(&mut self) {
        if self.appearance.transparency_available
            || self.appearance.warned_opaque
            || self.options.transparency.opacity >= 1.0
        {
            return;
        }
        self.appearance.warned_opaque = true;
        report!(
            "opacity {} was asked for, but the display offers no transparent \
             surface (is a compositor running?), so the window stays opaque",
            self.options.transparency.opacity
        );
    }
}
