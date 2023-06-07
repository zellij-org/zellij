mod search;
mod state;

use colored::*;
use search::{FileContentsWorker, FileNameWorker, MessageToSearch, ResultsOfSearch};
use serde::{Deserialize, Serialize};
use serde_json;
use state::{refresh_directory, FsEntry, State};
use std::{cmp::min, time::Instant};
use zellij_tile::prelude::*;

register_plugin!(State);
register_worker!(FileNameWorker, file_name_search_worker, FILE_NAME_WORKER);
register_worker!(
    FileContentsWorker,
    file_contents_search_worker,
    FILE_CONTENTS_WORKER
);

impl ZellijPlugin for State {
    fn load(&mut self) {
        refresh_directory(self);
        self.search_state.loading = true;
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::CustomMessage,
            EventType::Timer,
            EventType::FileSystemCreate,
            EventType::FileSystemUpdate,
            EventType::FileSystemDelete,
        ]);
        post_message_to(
            "file_name_search",
            serde_json::to_string(&MessageToSearch::ScanFolder).unwrap(),
            "".to_owned(),
        );
        post_message_to(
            "file_contents_search",
            serde_json::to_string(&MessageToSearch::ScanFolder).unwrap(),
            "".to_owned(),
        );
        self.search_state.loading = true;
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
                if self.search_state.loading {
                    set_timeout(0.5);
                    self.search_state.progress_animation();
                    should_render = true;
                }
            },
            Event::CustomMessage(message, payload) => match serde_json::from_str(&message) {
                Ok(MessageToPlugin::UpdateFileNameSearchResults) => {
                    if let Ok(results_of_search) = serde_json::from_str::<ResultsOfSearch>(&payload)
                    {
                        self.search_state
                            .update_file_name_search_results(results_of_search);
                        should_render = true;
                    }
                },
                Ok(MessageToPlugin::UpdateFileContentsSearchResults) => {
                    if let Ok(results_of_search) = serde_json::from_str::<ResultsOfSearch>(&payload)
                    {
                        self.search_state
                            .update_file_contents_search_results(results_of_search);
                        should_render = true;
                    }
                },
                Ok(MessageToPlugin::DoneScanningFolder) => {
                    self.search_state.loading = false;
                    should_render = true;
                },
                Err(e) => eprintln!("Failed to deserialize custom message: {:?}", e),
            },
            Event::Key(key) => match key {
                Key::Esc if self.typing_search_term() => {
                    self.stop_typing_search_term();
                    self.search_state.handle_key(key);
                    should_render = true;
                },
                _ if self.typing_search_term() => {
                    self.search_state.handle_key(key);
                    should_render = true;
                },
                Key::Char('/') => {
                    self.start_typing_search_term();
                    should_render = true;
                },
                Key::Esc => {
                    self.stop_typing_search_term();
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
                    *self.selected_mut() = min(self.files.len().saturating_sub(1), next);
                    if currently_selected != self.selected() {
                        should_render = true;
                    }
                },
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
            Event::FileSystemCreate(paths) => {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                post_message_to(
                    "file_name_search",
                    serde_json::to_string(&MessageToSearch::FileSystemCreate).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
                post_message_to(
                    "file_contents_search",
                    serde_json::to_string(&MessageToSearch::FileSystemCreate).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
            },
            Event::FileSystemUpdate(paths) => {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                post_message_to(
                    "file_name_search",
                    serde_json::to_string(&MessageToSearch::FileSystemUpdate).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
                post_message_to(
                    "file_contents_search",
                    serde_json::to_string(&MessageToSearch::FileSystemUpdate).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
            },
            Event::FileSystemDelete(paths) => {
                let paths: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect();
                post_message_to(
                    "file_name_search",
                    serde_json::to_string(&MessageToSearch::FileSystemDelete).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
                post_message_to(
                    "file_contents_search",
                    serde_json::to_string(&MessageToSearch::FileSystemDelete).unwrap(),
                    serde_json::to_string(&paths).unwrap(),
                );
            },
            _ => {
                dbg!("Unknown event {:?}", event);
            },
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        if self.typing_search_term() {
            self.search_state.change_size(rows, cols);
            print!("{}", self.search_state);
            return;
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

#[derive(Serialize, Deserialize)]
pub enum MessageToPlugin {
    UpdateFileNameSearchResults,
    UpdateFileContentsSearchResults,
    DoneScanningFolder,
}
