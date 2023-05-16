mod search;
mod state;

use colored::*;
use search::{ResultsOfSearch, SearchWorker};
use serde_json;
use state::{refresh_directory, FsEntry, State, CURRENT_SEARCH_TERM};
use std::{cmp::min, time::Instant};
use zellij_tile::prelude::*;

register_plugin!(State);
register_worker!(SearchWorker, search_worker);

impl ZellijPlugin for State {
    fn load(&mut self) {
        refresh_directory(self);
        self.loading = true;
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::CustomMessage,
            EventType::Timer,
        ]);
        post_message_to("search", String::from("scan_folder"), String::new());
        set_timeout(0.5); // for displaying loading animation
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
            Event::Timer(_elapsed) => {
                should_render = true;
                if self.loading {
                    set_timeout(0.5);
                    if self.loading_animation_offset == u8::MAX {
                        self.loading_animation_offset = 0;
                    } else {
                        self.loading_animation_offset =
                            self.loading_animation_offset.saturating_add(1);
                    }
                }
            },
            Event::CustomMessage(message, payload) => match message.as_str() {
                "update_search_results" => {
                    if let Ok(mut results_of_search) =
                        serde_json::from_str::<ResultsOfSearch>(&payload)
                    {
                        if Some(results_of_search.search_term) == self.search_term {
                            self.search_results =
                                results_of_search.search_results.drain(..).collect();
                            should_render = true;
                        }
                    }
                },
                "done_scanning_folder" => {
                    self.loading = false;
                    should_render = true;
                },
                _ => {},
            },
            Event::Key(key) => match key {
                // modes:
                // 1. typing_search_term
                // 2. exploring_search_results
                // 3. normal
                Key::Esc | Key::Char('\n') if self.typing_search_term() => {
                    self.accept_search_term();
                },
                _ if self.typing_search_term() => {
                    self.append_to_search_term(key);
                    if let Some(search_term) = self.search_term.as_ref() {
                        std::fs::write(CURRENT_SEARCH_TERM, search_term.as_bytes()).unwrap();
                        post_message_to(
                            "search",
                            String::from("search"),
                            String::from(&self.search_term.clone().unwrap()),
                        );
                    }
                    should_render = true;
                },
                Key::Esc if self.exploring_search_results() => {
                    self.stop_exploring_search_results();
                    should_render = true;
                },
                Key::Char('/') => {
                    self.start_typing_search_term();
                    should_render = true;
                },
                Key::Esc => {
                    self.stop_typing_search_term();
                    should_render = true;
                },
                Key::Up | Key::Char('k') => {
                    if self.exploring_search_results() {
                        self.move_search_selection_up();
                        should_render = true;
                    } else {
                        let currently_selected = self.selected();
                        *self.selected_mut() = self.selected().saturating_sub(1);
                        if currently_selected != self.selected() {
                            should_render = true;
                        }
                    }
                },
                Key::Down | Key::Char('j') => {
                    if self.exploring_search_results() {
                        self.move_search_selection_down();
                        should_render = true;
                    } else {
                        let currently_selected = self.selected();
                        let next = self.selected().saturating_add(1);
                        *self.selected_mut() = min(self.files.len().saturating_sub(1), next);
                        if currently_selected != self.selected() {
                            should_render = true;
                        }
                    }
                },
                Key::Right | Key::Char('\n') | Key::Char('l') if !self.files.is_empty() => {
                    if self.exploring_search_results() {
                        self.open_search_result();
                    } else {
                        self.traverse_dir_or_open_file();
                        self.ev_history.clear();
                    }
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

    fn render(&mut self, rows: usize, cols: usize) {
        if self.typing_search_term() || self.exploring_search_results() {
            return self.render_search(rows, cols);
        }

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
                        print!("{}", path.reversed());
                    } else {
                        println!("{}", path.reversed());
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
