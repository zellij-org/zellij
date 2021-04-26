mod state;

use state::State;
use zellij_tile::prelude::*;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        subscribe(&[EventType::Log, EventType::KeyPress])
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::Log(content, log_level) => {
                self.append_message(content, log_level);
            }
            Event::KeyPress(key) => match key {
                Key::Right | Key::Char('l') => {
                    self.inc_index(None);
                }
                Key::Left | Key::Char('h') => {
                    self.dec_index(None);
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        println!("{}", self)
    }
}
