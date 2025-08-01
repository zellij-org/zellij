mod file_list_view;
mod search_view;
mod shared;
mod state;

use shared::{render_current_path, render_instruction_line, render_search_term};
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
            EventType::HostFolderChanged,
            EventType::PermissionRequestResult,
        ]);
        self.file_list_view.clear_selected();

        match configuration.get("caller_cwd").map(|c| PathBuf::from(c)) {
            Some(caller_cwd) => {
                self.file_list_view.path = caller_cwd;
            },
            None => {
                self.file_list_view.path = self.initial_cwd.clone();
            },
        }
        if self.initial_cwd != self.file_list_view.path {
            change_host_folder(self.file_list_view.path.clone());
        } else {
            scan_host_folder(&"/host");
        }
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::FileSystemUpdate(paths) => {
                self.update_files(paths);
                should_render = true;
            },
            Event::HostFolderChanged(_new_host_folder) => {
                scan_host_folder(&"/host");
                should_render = true;
            },
            Event::Key(key) => match key.bare_key {
                BareKey::Char(character) if key.has_no_modifiers() => {
                    self.update_search_term(character);
                    should_render = true;
                },
                BareKey::Backspace if key.has_no_modifiers() => {
                    self.handle_backspace();
                    should_render = true;
                },
                BareKey::Esc if key.has_no_modifiers() => {
                    if self.is_searching {
                        self.clear_search_term();
                    } else {
                        self.file_list_view.clear_selected();
                    }
                    should_render = true;
                },
                BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                    self.clear_search_term_or_descend();
                },
                BareKey::Up if key.has_no_modifiers() => {
                    self.move_selection_up();
                    should_render = true;
                },
                BareKey::Down if key.has_no_modifiers() => {
                    self.move_selection_down();
                    should_render = true;
                },
                BareKey::Right | BareKey::Tab | BareKey::Enter if key.has_no_modifiers() => {
                    self.traverse_dir();
                    should_render = true;
                },
                BareKey::Right if key.has_no_modifiers() => {
                    self.traverse_dir();
                    should_render = true;
                },
                BareKey::Left if key.has_no_modifiers() => {
                    self.descend_to_previous_path();
                    should_render = true;
                },
                BareKey::Char('e') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
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
                },
                Mouse::Hover(line, _) => {
                    if line >= 0 {
                        self.handle_mouse_hover(line);
                        should_render = true;
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
        if pipe_message.is_private && pipe_message.name == "filepicker" {
            if let PipeSource::Cli(pipe_id) = &pipe_message.source {
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
        self.current_rows = Some(rows);
        let rows_for_list = rows.saturating_sub(6);
        render_search_term(&self.search_term);
        render_current_path(
            &self.file_list_view.path,
            self.file_list_view.path_is_dir,
            self.handling_filepick_request_from.is_some(),
            cols,
        );
        if self.is_searching {
            self.search_view.render(rows_for_list, cols);
        } else {
            self.file_list_view.render(rows_for_list, cols);
        }
        render_instruction_line(rows, cols);
    }
}
