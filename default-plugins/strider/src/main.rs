mod state;

use colored::*;
use state::{FsEntry, State};
use std::{cmp::min, fs::read_dir, path::Path};
use zellij_tile::prelude::*;

const ROOT: &str = "/host";

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self) {
        refresh_directory(self);
        subscribe(&[EventType::KeyPress]);
    }

    fn update(&mut self, event: Event) {
        if let Event::KeyPress(key) = event {
            match key {
                Key::Up | Key::Char('k') => {
                    *self.selected_mut() = self.selected().saturating_sub(1);
                }
                Key::Down | Key::Char('j') => {
                    let next = self.selected().saturating_add(1);
                    *self.selected_mut() = min(self.files.len() - 1, next);
                }
                Key::Right | Key::Char('\n') | Key::Char('l') if !self.files.is_empty() => {
                    match self.files[self.selected()].clone() {
                        FsEntry::Dir(p, _) => {
                            self.path = p;
                            refresh_directory(self);
                        }
                        FsEntry::File(p, _) => open_file(p.strip_prefix(ROOT).unwrap()),
                    }
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
            };
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

fn refresh_directory(state: &mut State) {
    state.files = read_dir(Path::new(ROOT).join(&state.path))
        .unwrap()
        .filter_map(|res| {
            res.and_then(|d| {
                if d.metadata()?.is_dir() {
                    let children = read_dir(d.path())?.count();
                    Ok(FsEntry::Dir(d.path(), children))
                } else {
                    let size = d.metadata()?.len();
                    Ok(FsEntry::File(d.path(), size))
                }
            })
            .ok()
            .filter(|d| !d.is_hidden_file() || !state.hide_hidden_files)
        })
        .collect();

    state.files.sort_unstable();
}
