mod state;

use colored::*;
use state::{refresh_directory, FsEntry, State};
use std::{cmp::min, time::Instant};
use zellij_tile::prelude::*;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _: EmptyOptions) {
        refresh_directory(self);
        subscribe(&[EventType::Key, EventType::Mouse]);
    }

    fn update(&mut self, event: Event) {
        let prev_event = if self.ev_history.len() == 2 {
            self.ev_history.pop_front()
        } else {
            None
        };
        self.ev_history.push_back((event.clone(), Instant::now()));
        match event {
            Event::Key(key) => match key {
                Key::Up | Key::Char('k') => {
                    *self.selected_mut() = self.selected().saturating_sub(1);
                }
                Key::Down | Key::Char('j') => {
                    let next = self.selected().saturating_add(1);
                    *self.selected_mut() = min(self.files.len() - 1, next);
                }
                Key::Right | Key::Char('\n') | Key::Char('l') if !self.files.is_empty() => {
                    self.traverse_dir_or_open_file();
                    self.ev_history.clear();
                }
                Key::Left | Key::Char('h') => {
                    if self.path.components().count() > 2 {
                        // don't descend into /host
                        // the reason this is a hard-coded number (2) and not "== ROOT"
                        // or some such is that there are certain cases in which self.path
                        // is empty and this will work then too
                        self.path.pop();
                        refresh_directory(self);
                    }
                }
                Key::Char('.') => {
                    self.toggle_hidden_files();
                    refresh_directory(self);
                }

                _ => (),
            },
            Event::Mouse(mouse_event) => match mouse_event {
                Mouse::ScrollDown(_) => {
                    let next = self.selected().saturating_add(1);
                    *self.selected_mut() = min(self.files.len().saturating_sub(1), next);
                }
                Mouse::ScrollUp(_) => {
                    *self.selected_mut() = self.selected().saturating_sub(1);
                }
                Mouse::Release(Some((line, _))) => {
                    if line < 0 {
                        return;
                    }
                    let mut should_select = true;
                    if let Some((Event::Mouse(Mouse::Release(Some((prev_line, _)))), t)) =
                        prev_event
                    {
                        if prev_line == line
                            && Instant::now().saturating_duration_since(t).as_millis() < 400
                        {
                            self.traverse_dir_or_open_file();
                            self.ev_history.clear();
                            should_select = false;
                        }
                    }
                    if should_select && self.scroll() + (line as usize) < self.files.len() {
                        *self.selected_mut() = self.scroll() + (line as usize);
                    }
                }
                _ => {}
            },
            _ => {
                dbg!("Unknown event {:?}", event);
            }
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        for i in 0..rows {
            if self.selected() < self.scroll() {
                *self.scroll_mut() = self.selected();
            }
            if self.selected() - self.scroll() + 2 > rows {
                *self.scroll_mut() = self.selected() + 2 - rows;
            }

            let i = self.scroll() + i;
            if let Some(entry) = self.files.get(i) {
                let mut path = entry.as_line(cols).normal();

                if let FsEntry::Dir(..) = entry {
                    path = path.dimmed().bold();
                }

                if i == self.selected() {
                    println!("{}", path.reversed());
                } else {
                    println!("{}", path);
                }
            } else {
                println!();
            }
        }
    }
}
