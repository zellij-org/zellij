use crate::emulation::damage::Damage;
use crate::input::clipboard::{self, Clipboard};
use crate::input::composition::{Gate, Reaction};
use crate::input::dropped;
use crate::input::keyboard;
use crate::platform::Platform;
use crate::render::retained::{self};
use crate::window::App;
use crate::window::ZOOM_STEP;
use winit::event::Ime;
use winit::event::KeyEvent;
use winit::keyboard::Key;

use std::path::Path;

impl App {
    pub(in crate::window) fn on_key(&mut self, event: &KeyEvent) {
        let option_as_alt = keyboard::option_acts_as_alt(
            Platform::current(),
            self.options.option_as_alt,
            self.alt_keys,
        );
        self.press(keyboard::Press::of(event, self.modifiers, option_as_alt));
    }

    pub(in crate::window) fn press(&mut self, press: keyboard::Press) {
        let gate = self.ime.composition.gate();
        if gate == Gate::Composing {
            return;
        }
        if let Key::Dead(accent) = press.logical {
            match keyboard::dead_key_press(&press) {
                keyboard::DeadKeyOutcome::Wait => {
                    let reaction = self.ime.composition.dead_key(accent);
                    self.compose(reaction);
                },
                keyboard::DeadKeyOutcome::Type(text) => {
                    let reaction = self.ime.composition.resolve();
                    self.compose(reaction);
                    self.type_text(text);
                },
            }
            return;
        }
        if gate == Gate::Resolving {
            if keyboard::leaves_the_accent_pending(&press) {
                return;
            }
            let reaction = self.ime.composition.resolve();
            self.compose(reaction);
            match keyboard::resolve_pending_accent(&press) {
                keyboard::Resolution::Key => self.deliver(&press),
                keyboard::Resolution::Type(text) => self.type_text(text),
                keyboard::Resolution::TypeThenKey(text) => {
                    self.type_text(text);
                    self.deliver(&press);
                },
                keyboard::Resolution::Nothing => {},
            }
            return;
        }
        if let Some(text) = keyboard::unidentified_text(&press) {
            self.type_text(text);
            return;
        }
        if let Some(text) = keyboard::leftover_accent(&press) {
            self.type_text(text);
            return;
        }
        if keyboard::claims(&self.options.zoom_in_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.zoom_by(ZOOM_STEP);
            return;
        }
        if keyboard::claims(&self.options.zoom_out_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.zoom_by(1.0 / ZOOM_STEP);
            return;
        }
        if keyboard::claims(&self.options.zoom_reset_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.set_zoom(1.0);
            return;
        }
        if keyboard::claims(&self.options.fullscreen_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.toggle_fullscreen();
            return;
        }
        if keyboard::claims(&self.options.copy_keys, &press) && self.copy_selection() {
            self.hide_pointer();
            self.stop_momentum();
            return;
        }
        if keyboard::claims(&self.options.paste_keys, &press) {
            self.hide_pointer();
            self.stop_momentum();
            self.paste();
            return;
        }
        self.deliver(&press);
    }

    pub(in crate::window) fn deliver(&mut self, press: &keyboard::Press) {
        let Some(msg) = keyboard::key_message(press) else {
            return;
        };
        self.set_selection(None);
        self.stop_momentum();
        self.hide_pointer();
        if let Err(e) = self.tell(msg) {
            report!("failed to send a key: {}", e);
        }
    }

    pub(in crate::window) fn type_text(&mut self, text: String) {
        let Some(msg) = keyboard::typed_text_message(text) else {
            return;
        };
        self.set_selection(None);
        self.stop_momentum();
        self.hide_pointer();
        if let Err(e) = self.tell(msg) {
            report!("failed to send typed text: {}", e);
        }
    }

    pub(in crate::window) fn on_ime(&mut self, event: Ime) {
        let reaction = self.ime.composition.ime(event);
        let committed = match reaction {
            Reaction::Commit(text) => Some(text),
            other => {
                self.compose(other);
                None
            },
        };
        let Some(text) = committed else {
            return;
        };
        self.compose(Reaction::Redraw);
        if text.is_empty() {
            return;
        }
        self.stop_momentum();
        let Some(msg) = keyboard::typed_text_message(text) else {
            return;
        };
        self.set_selection(None);
        if let Err(e) = self.tell(msg) {
            report!("failed to send composed text: {}", e);
        }
    }

    pub(in crate::window) fn compose(&mut self, reaction: Reaction) {
        if reaction == Reaction::Nothing {
            return;
        }
        let rows = retained::composing_rows(&self.state, self.ime.composed_row.take());
        if self.ime.composition.shown().is_some() {
            self.ime.composed_row = Some(self.state.cursor_position().0);
        }
        self.retained.mark(&Damage::Rows(rows));
        self.schedule_draw();
    }

    pub(in crate::window) fn stop_composing(&mut self) {
        let reaction = self.ime.composition.cancel();
        self.compose(reaction);
    }

    pub(in crate::window) fn paste(&mut self) {
        self.set_selection(None);
        let Some(chars) = self.with_clipboard(|clipboard| clipboard.get()).flatten() else {
            return;
        };
        if let Err(e) = self.tell(clipboard::paste_message(chars)) {
            report!("failed to send a paste: {}", e);
        }
    }

    pub(in crate::window) fn dropped_file(&mut self, path: &Path) {
        self.set_selection(None);
        if let Err(e) = self.tell(clipboard::paste_message(dropped::dropped_file_text(path))) {
            report!("failed to send a dropped file's path: {}", e);
        }
    }

    pub(in crate::window) fn copy(&mut self) {
        if let Some(text) = self.state.take_copied_text() {
            self.with_clipboard(|clipboard| clipboard.set(&text));
        }
    }

    pub(in crate::window) fn with_clipboard<T>(
        &self,
        f: impl FnOnce(&mut Clipboard) -> T,
    ) -> Option<T> {
        match self.clipboard.lock() {
            Ok(mut clipboard) => Some(f(&mut clipboard)),
            Err(_) => {
                report!("the clipboard mutex was poisoned");
                None
            },
        }
    }
}
