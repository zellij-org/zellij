mod state;
mod search_view;
mod file_list_view;
mod shared;

use shared::{render_instruction_line, render_current_path};
use state::{refresh_directory, State};
use std::collections::BTreeMap;
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
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::CustomMessage,
            EventType::Timer,
            EventType::FileSystemUpdate,
        ]);
        self.file_list_view.reset_selected();
        refresh_directory(&self.initial_cwd);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::FileSystemUpdate(paths) => {
                self.update_files(paths);
                should_render = true;
            }
            Event::Key(key) => match key {
                Key::Char(character) if character != '\n' => {
                    self.update_search_term(character);
                    should_render = true;
                }
                Key::Backspace => {
                    self.handle_backspace();
                    should_render = true;
                }
                Key::Esc | Key::Ctrl('c') => {
                    self.clear_search_term_or_descend();
                    should_render = true;
                }
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
                }
                Key::Right | Key::Char('\n') | Key::BackTab => {
                    self.handle_selection();
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
                }
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
                block_cli_pipe_input(pipe_id);
            }
            self.handling_filepick_request_from = Some((pipe_message.source, pipe_message.args));
            true
        } else {
            false
        }
    }

    fn render(&mut self, rows: usize, cols: usize) {
        self.current_rows = Some(rows);
        let rows_for_list = rows.saturating_sub(5);
        render_current_path(&self.initial_cwd, &self.file_list_view.path, &self.search_term);
        if self.is_searching {
            self.search_view.render(rows_for_list, cols);
        } else {
            self.file_list_view.render(rows_for_list, cols);
        }
        render_instruction_line(rows, cols);
    }
}
