use crate::file_list_view::{FileListView, FsEntry};
use crate::search_view::SearchView;
use crate::shared::calculate_list_bounds;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use zellij_tile::prelude::*;

pub const ROOT: &str = "/host";

#[derive(Default)]
pub struct State {
    pub file_list_view: FileListView,
    pub search_view: SearchView,
    pub hide_hidden_files: bool,
    pub loading: bool,
    pub loading_animation_offset: u8,
    pub should_open_floating: bool,
    pub current_rows: Option<usize>,
    pub handling_filepick_request_from: Option<(PipeSource, BTreeMap<String, String>)>,
    pub initial_cwd: PathBuf, // TODO: get this from zellij
    pub is_searching: bool,
    pub search_term: String,
    pub close_on_selection: bool,
}

impl State {
    pub fn update_search_term(&mut self, character: char) {
        self.search_term.push(character);
        if &self.search_term == ".." {
            self.descend_to_previous_path();
        } else if &self.search_term == "/" {
            self.descend_to_root_path();
        } else {
            self.is_searching = true;
            self.search_view
                .update_search_results(&self.search_term, &self.file_list_view.files);
        }
    }
    pub fn handle_backspace(&mut self) {
        if self.search_term.is_empty() {
            self.descend_to_previous_path();
        } else {
            self.search_term.pop();
            if self.search_term.is_empty() {
                self.is_searching = false;
            }
            self.search_view
                .update_search_results(&self.search_term, &self.file_list_view.files);
        }
    }
    pub fn clear_search_term_or_descend(&mut self) {
        if self.search_term.is_empty() {
            self.descend_to_previous_path();
        } else {
            self.search_term.clear();
            self.search_view
                .update_search_results(&self.search_term, &self.file_list_view.files);
            self.is_searching = false;
        }
    }
    pub fn move_selection_up(&mut self) {
        if self.is_searching {
            self.search_view.move_selection_up();
        } else {
            self.file_list_view.move_selection_up();
        }
    }
    pub fn move_selection_down(&mut self) {
        if self.is_searching {
            self.search_view.move_selection_down();
        } else {
            self.file_list_view.move_selection_down();
        }
    }
    pub fn handle_left_click(&mut self, line: isize) {
        if let Some(current_rows) = self.current_rows {
            let rows_for_list = current_rows.saturating_sub(5);
            if self.is_searching {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.search_view.search_result_count(),
                    rows_for_list,
                    Some(self.search_view.selected_search_result),
                );
                let prev_selected = self.search_view.selected_search_result;
                self.search_view.selected_search_result =
                    (line as usize).saturating_sub(2) + start_index;
                if prev_selected == self.search_view.selected_search_result {
                    self.traverse_dir();
                }
            } else {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.file_list_view.files.len(),
                    rows_for_list,
                    self.file_list_view.selected(),
                );
                let prev_selected = self.file_list_view.selected();
                *self.file_list_view.selected_mut() =
                    (line as usize).saturating_sub(2) + start_index;
                if prev_selected == self.file_list_view.selected() {
                    self.traverse_dir();
                }
            }
        }
    }
    pub fn descend_to_previous_path(&mut self) {
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
        self.file_list_view.descend_to_previous_path();
    }
    pub fn descend_to_root_path(&mut self) {
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
        self.file_list_view.descend_to_root_path();
        refresh_directory(&self.file_list_view.path);
    }
    pub fn toggle_hidden_files(&mut self) {
        self.hide_hidden_files = !self.hide_hidden_files;
    }
    pub fn traverse_dir(&mut self) {
        let entry = if self.is_searching {
            self.search_view.get_selected_entry()
        } else {
            self.file_list_view.get_selected_entry()
        };
        if let Some(entry) = entry {
            match &entry {
                FsEntry::Dir(_p) => {
                    self.file_list_view.enter_dir(&entry);
                    self.search_view.clear_and_reset_selection();
                    refresh_directory(&self.file_list_view.path);
                },
                FsEntry::File(_p, _) => {
                    self.file_list_view.enter_dir(&entry);
                    self.search_view.clear_and_reset_selection();
                },
            }
        }
        self.is_searching = false;
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
    }
    pub fn update_files(&mut self, paths: Vec<(PathBuf, Option<FileMetadata>)>) {
        self.file_list_view
            .update_files(paths, self.hide_hidden_files);
    }
    pub fn open_selected_path(&mut self) {
        if self.file_list_view.path_is_dir {
            open_terminal(&self.file_list_view.path);
        } else {
            if let Some(parent_folder) = self.file_list_view.path.parent() {
                open_file(
                    FileToOpen::new(&self.file_list_view.path).with_cwd(parent_folder.into()),
                );
            } else {
                open_file(FileToOpen::new(&self.file_list_view.path));
            }
        }
        if self.close_on_selection {
            close_self();
        }
    }
    pub fn send_filepick_response(&mut self) {
        let selected_path = self.initial_cwd.join(
            self.file_list_view
                .path
                .strip_prefix(ROOT)
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|_| self.file_list_view.path.clone()),
        );
        match &self.handling_filepick_request_from {
            Some((PipeSource::Plugin(plugin_id), args)) => {
                pipe_message_to_plugin(
                    MessageToPlugin::new("filepicker_result")
                        .with_destination_plugin_id(*plugin_id)
                        .with_args(args.clone())
                        .with_payload(selected_path.display().to_string()),
                );
                #[cfg(target_family = "wasm")]
                close_self();
            },
            Some((PipeSource::Cli(pipe_id), _args)) => {
                #[cfg(target_family = "wasm")]
                cli_pipe_output(pipe_id, &selected_path.display().to_string());
                #[cfg(target_family = "wasm")]
                unblock_cli_pipe_input(pipe_id);
                #[cfg(target_family = "wasm")]
                close_self();
            },
            _ => {},
        }
    }
}

pub(crate) fn refresh_directory(path: &Path) {
    let path_on_host = Path::new(ROOT).join(path.strip_prefix("/").unwrap_or(path));
    scan_host_folder(&path_on_host);
}
