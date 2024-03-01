mod state;

use colored::*;
use state::{refresh_directory, FsEntry, State, ROOT};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{cmp::min, time::Instant};
use zellij_tile::prelude::*;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        let plugin_ids = get_plugin_ids();
        self.initial_cwd = plugin_ids.initial_cwd;
        refresh_directory(self);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::CustomMessage,
            EventType::Timer,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        let prev_event = if self.ev_history.len() == 2 {
            self.ev_history.pop_front()
        } else {
            None
        };
        self.ev_history.push_back((event.clone(), Instant::now()));
        match event {
            Event::Key(key) => match key {
                Key::Esc => {
                    hide_self();
                    should_render = true;
                },
                Key::Up | Key::Char('k') => {
                    let currently_selected = self.selected();
                    *self.selected_mut() = self.selected().saturating_sub(1);
                    if currently_selected != self.selected() {
                        should_render = true;
                    }
                },
                Key::Down | Key::Char('j') => {
                    let currently_selected = self.selected();
                    let next = self.selected().saturating_add(1);
                    if next >= self.files.len() {
                        refresh_directory(self);
                    }
                    *self.selected_mut() = min(self.files.len().saturating_sub(1), next);
                    if currently_selected != self.selected() {
                        should_render = true;
                    }
                },
                Key::Char('\n') if self.handling_filepick_request_from.is_some() && !self.files.is_empty() => {
                    if let Some(f) = self.files.get(self.selected()) {
                        match &self.handling_filepick_request_from {
                            Some((PipeSource::Plugin(plugin_id), args)) => {
                                let selected_path = f.get_pathbuf();
                                pipe_message_to_plugin(
                                    MessageToPlugin::new("filepicker_result")
                                        .with_destination_plugin_id(*plugin_id)
                                        .with_args(args.clone())
                                        .with_payload(self.initial_cwd.join(selected_path.strip_prefix(ROOT).unwrap()).display().to_string()) // TODO: no unwrap
                                );
                                close_focus();
                            },
                            Some((PipeSource::Cli(pipe_id), _args)) => {
                                // TODO: implement this
                            },
                            _ => {}
                        }
                    }
                }
                Key::Right | Key::Char('\n') | Key::Char('l') if !self.files.is_empty() => {
                    self.traverse_dir_or_open_file();
                    self.ev_history.clear();
                    should_render = true;
                },
                Key::Left | Key::Char('h') => {
                    if self.path.components().count() > 2 {
                        // don't descend into /host
                        // the reason this is a hard-coded number (2) and not "== ROOT"
                        // or some such is that there are certain cases in which self.path
                        // is empty and this will work then too
                        should_render = true;
                        self.path.pop();
                        refresh_directory(self);
                    }
                },
                Key::Char('.') => {
                    should_render = true;
                    self.toggle_hidden_files();
                    refresh_directory(self);
                },

                _ => (),
            },
            Event::Mouse(mouse_event) => match mouse_event {
                Mouse::ScrollDown(_) => {
                    let currently_selected = self.selected();
                    let next = self.selected().saturating_add(1);
                    if next >= self.files.len() {
                        refresh_directory(self);
                    }
                    *self.selected_mut() = min(self.files.len().saturating_sub(1), next);
                    if currently_selected != self.selected() {
                        should_render = true;
                    }
                },
                Mouse::ScrollUp(_) => {
                    let currently_selected = self.selected();
                    *self.selected_mut() = self.selected().saturating_sub(1);
                    if currently_selected != self.selected() {
                        should_render = true;
                    }
                },
                Mouse::Release(line, _) => {
                    if line < 0 {
                        return should_render;
                    }
                    let mut should_select = true;
                    if let Some((Event::Mouse(Mouse::Release(prev_line, _)), t)) = prev_event {
                        if prev_line == line
                            && Instant::now().saturating_duration_since(t).as_millis() < 400
                        {
                            self.traverse_dir_or_open_file();
                            self.ev_history.clear();
                            should_select = false;
                            should_render = true;
                        }
                    }
                    if should_select && self.scroll() + (line as usize) < self.files.len() {
                        let currently_selected = self.selected();
                        *self.selected_mut() = self.scroll() + (line as usize);
                        if currently_selected != self.selected() {
                            should_render = true;
                        }
                    }
                },
                _ => {},
            },
            _ => {
                dbg!("Unknown event {:?}", event);
            },
        };
        should_render
    }
    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        self.handling_filepick_request_from = Some((pipe_message.source, pipe_message.args));
        true
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.current_rows = Some(rows);
        for i in 0..rows {
            if self.selected() < self.scroll() {
                *self.scroll_mut() = self.selected();
            }
            if self.selected() - self.scroll() + 2 > rows {
                *self.scroll_mut() = self.selected() + 2 - rows;
            }

            let is_last_row = i == rows.saturating_sub(1);
            let i = self.scroll() + i;
            if let Some(entry) = self.files.get(i) {
                let mut path = entry.as_line(cols).normal();

                if let FsEntry::Dir(..) = entry {
                    path = path.dimmed().bold();
                }

                if i == self.selected() {
                    if is_last_row {
                        print!("{}", path.clone().reversed());
                    } else {
                        println!("{}", path.clone().reversed());
                    }
                } else {
                    if is_last_row {
                        print!("{}", path);
                    } else {
                        println!("{}", path);
                    }
                }
            } else if !is_last_row {
                println!();
            }
        }
    }
}
