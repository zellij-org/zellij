mod file_list_view;
mod search_view;
mod shared;
mod state;

use crate::file_list_view::FsEntry;
use shared::{
    render_current_path, render_instruction_line, render_instruction_tip, render_search_term,
};
use state::{refresh_directory, State};
use std::collections::BTreeMap;
use std::path::PathBuf;
use zellij_tile::prelude::*;

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        let plugin_ids = get_plugin_ids();
        self.initial_cwd = plugin_ids.initial_cwd;
        let show_hidden_files = configuration
            .get("show_hidden_files")
            .map(|v| v == "true")
            .unwrap_or(false);
        self.hide_hidden_files = !show_hidden_files;
        self.close_on_selection = configuration
            .get("close_on_selection")
            .map(|v| v == "true")
            .unwrap_or(false);
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::CustomMessage,
            EventType::Timer,
            EventType::FileSystemUpdate,
        ]);
        self.file_list_view.reset_selected();
        // the caller_cwd might be different from the initial_cwd if this plugin was defined as an
        // alias, with access to a certain part of the file system (often broader) and was called
        // from an individual pane somewhere inside this broad scope - in this case, we want to
        // start in the same cwd as the caller, giving them the full access we were granted
        match configuration
            .get("caller_cwd")
            .map(|c| PathBuf::from(c))
            .and_then(|c| {
                c.strip_prefix(&self.initial_cwd)
                    .ok()
                    .map(|c| PathBuf::from(c))
            }) {
            Some(relative_caller_path) => {
                let relative_caller_path = FsEntry::Dir(relative_caller_path.to_path_buf());
                self.file_list_view.enter_dir(&relative_caller_path);
                refresh_directory(&self.file_list_view.path);
            },
            None => {
                refresh_directory(&std::path::Path::new("/"));
            },
        }
    }

    fn update(&mut self, event: Event) -> bool {
        // TODO: Only run this if the event is a key
        match self.mode {
            state::Mode::Keybinds => {
                self.mode = state::Mode::Normal;
                return true;
            },
            _ => {},
        }

        let mut should_render = false;
        match event {
            Event::FileSystemUpdate(paths) => {
                self.update_files(paths);
                should_render = true;
            },
            Event::Key(key) => match key {
                Key::Char(character) if character != '\n' => {
                    self.update_search_term(character);
                    should_render = true;
                },
                Key::Backspace => {
                    self.handle_backspace();
                    should_render = true;
                },
                Key::Esc | Key::Ctrl('c') => {
                    self.clear_search_term_or_descend();
                    should_render = true;
                },
                Key::Up => {
                    self.move_selection_up();
                    should_render = true;
                },
                Key::Down => {
                    self.move_selection_down();
                    should_render = true;
                },
                Key::Char('\n') if self.handling_filepick_request_from.is_some() => {
                    self.send_filepick_response();
                },
                Key::Char('\n') => match self.mode {
                    state::Mode::Normal | state::Mode::Searching => self.open_selected_path(),
                    state::Mode::Create | state::Mode::Copy | state::Mode::Delete | state::Mode::Move => self.handle_file_manipulation(),
                    _ => {}
                },
                Key::Right | Key::BackTab => {
                    self.traverse_dir();
                    should_render = true;
                },
                Key::Left => {
                    self.descend_to_previous_path();
                    should_render = true;
                },
                Key::Ctrl('e') => {
                    should_render = true;
                    self.toggle_hidden_files();
                    refresh_directory(&self.file_list_view.path);
                },
                Key::Ctrl('h') => {
                    should_render = true;
                    self.mode = state::Mode::Keybinds;
                },
                Key::Ctrl('r') => {
                    should_render = true;
                    self.move_entry_to_search();
                    self.mode = state::Mode::Move
                },
                Key::Ctrl('d') => {
                    should_render = true;
                    self.search_term = "".to_string();
                    self.mode = state::Mode::Delete
                },
                Key::Ctrl('y') => {
                    should_render = true;
                    self.move_entry_to_search();
                    self.mode = state::Mode::Copy
                }
                Key::Ctrl('a') => {
                    should_render = true;
                    self.move_entry_to_search();
                    self.mode = state::Mode::Create;
                }
                _ => (),
            },
            Event::Mouse(mouse_event) => match mouse_event {
                Mouse::ScrollDown(_) => {
                    self.move_selection_down();
                    should_render = true;
                },
                Mouse::ScrollUp(_) => {
                    self.move_selection_up();
                    should_render = true;
                },
                Mouse::LeftClick(line, _) => {
                    self.handle_left_click(line);
                    should_render = true;
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
        if pipe_message.is_private && pipe_message.name == "filepicker" {
            if let PipeSource::Cli(pipe_id) = &pipe_message.source {
                // here we block the cli pipe input because we want it to wait until the user chose
                // a file
                #[cfg(target_family = "wasm")]
                block_cli_pipe_input(pipe_id);
            }
            self.handling_filepick_request_from = Some((pipe_message.source, pipe_message.args));
            true
        } else {
            false
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match self.mode {
            state::Mode::Keybinds => {
                render_instruction_line(cols);
                render_instruction_tip(rows, cols);
                return;
            },
            _ => {},
        }

        self.current_rows = Some(rows);
        let rows_for_list = rows.saturating_sub(6);

        render_search_term(&self.search_term, &self.mode);
        render_current_path(
            &self.initial_cwd,
            &self.file_list_view.path,
            self.file_list_view.path_is_dir,
            self.handling_filepick_request_from.is_some(),
            cols,
        );
        match self.mode {
            state::Mode::Searching => self.search_view.render(rows_for_list, cols),
            _ => self.file_list_view.render(rows_for_list, cols),
        }
        render_instruction_tip(rows, cols);
    }
}
