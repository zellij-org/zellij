use crate::file_list_view::{FileListView, FsEntry};
use crate::platform::Platform;
use crate::search_view::SearchView;
use crate::shared::{calculate_list_bounds, render_list_tip};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

#[derive(Default)]
pub struct State {
    pub file_list_view: FileListView,
    pub search_view: SearchView,
    pub hide_hidden_files: bool,
    pub current_rows: Option<usize>,
    pub handling_filepick_request_from: Option<(PipeSource, BTreeMap<String, String>)>,
    pub initial_cwd: PathBuf,
    pub is_searching: bool,
    pub search_term: String,
    pub close_on_selection: bool,
    pub platform: Platform,
    pub is_in_virtual_root: bool,
    pub virtual_root_entries: Vec<FsEntry>,
    pub virtual_root_selected: usize,
}

impl State {
    pub fn update_search_term(&mut self, character: char) {
        if self.is_in_virtual_root {
            if character == '/' {
                self.descend_to_root_path();
            }
            return;
        }
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
        if self.is_in_virtual_root {
            return;
        }
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
    pub fn clear_search_term(&mut self) {
        self.search_term.clear();
        self.search_view
            .update_search_results(&self.search_term, &self.file_list_view.files);
        self.is_searching = false;
    }
    pub fn clear_search_term_or_descend(&mut self) {
        if self.is_in_virtual_root {
            return;
        }
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
        if self.is_in_virtual_root {
            self.virtual_root_selected = self.virtual_root_selected.saturating_sub(1);
        } else if self.is_searching {
            self.search_view.move_selection_up();
        } else {
            self.file_list_view.move_selection_up();
        }
    }
    pub fn move_selection_down(&mut self) {
        if self.is_in_virtual_root {
            if !self.virtual_root_entries.is_empty() {
                self.virtual_root_selected = std::cmp::min(
                    self.virtual_root_selected.saturating_add(1),
                    self.virtual_root_entries.len().saturating_sub(1),
                );
            }
        } else if self.is_searching {
            self.search_view.move_selection_down();
        } else {
            self.file_list_view.move_selection_down();
        }
    }
    pub fn handle_left_click(&mut self, line: isize) {
        if let Some(current_rows) = self.current_rows {
            let rows_for_list = current_rows.saturating_sub(5);
            if self.is_in_virtual_root {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.virtual_root_entries.len(),
                    rows_for_list,
                    Some(self.virtual_root_selected),
                );
                let prev_selected = self.virtual_root_selected;
                self.virtual_root_selected = (line as usize).saturating_sub(4) + start_index;
                if prev_selected == self.virtual_root_selected {
                    self.traverse_dir();
                }
            } else if self.is_searching {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.search_view.search_result_count(),
                    rows_for_list,
                    Some(self.search_view.selected_search_result),
                );
                let prev_selected = self.search_view.selected_search_result;
                self.search_view.selected_search_result =
                    (line as usize).saturating_sub(4) + start_index;
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
                    (line as usize).saturating_sub(4) + start_index;
                if prev_selected == self.file_list_view.selected() {
                    self.traverse_dir();
                }
            }
        }
    }
    pub fn handle_mouse_hover(&mut self, line: isize) {
        if let Some(current_rows) = self.current_rows {
            let rows_for_list = current_rows.saturating_sub(5);
            if self.is_in_virtual_root {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.virtual_root_entries.len(),
                    rows_for_list,
                    Some(self.virtual_root_selected),
                );
                self.virtual_root_selected = (line as usize).saturating_sub(4) + start_index;
            } else if self.is_searching {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.search_view.search_result_count(),
                    rows_for_list,
                    Some(self.search_view.selected_search_result),
                );
                self.search_view.selected_search_result =
                    (line as usize).saturating_sub(4) + start_index;
            } else {
                let (start_index, _selected_index_in_range, _end_index) = calculate_list_bounds(
                    self.file_list_view.files.len(),
                    rows_for_list,
                    self.file_list_view.selected(),
                );
                *self.file_list_view.selected_mut() =
                    (line as usize).saturating_sub(4) + start_index;
            }
        }
    }
    pub fn descend_to_previous_path(&mut self) {
        if self.is_in_virtual_root {
            return;
        }
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
        if self.platform == Platform::Windows
            && Platform::is_root(&self.file_list_view.path, self.platform)
        {
            self.enter_virtual_root();
        } else {
            self.file_list_view.descend_to_previous_path();
        }
    }
    pub fn descend_to_root_path(&mut self) {
        self.is_in_virtual_root = false;
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
        self.file_list_view.descend_to_root_path(&self.initial_cwd);
        refresh_directory(&self.file_list_view.path);
    }
    pub fn toggle_hidden_files(&mut self) {
        self.hide_hidden_files = !self.hide_hidden_files;
    }
    pub fn traverse_dir(&mut self) {
        if self.is_in_virtual_root {
            if let Some(entry) = self.virtual_root_entries.get(self.virtual_root_selected) {
                let path = entry.get_full_pathbuf();
                self.is_in_virtual_root = false;
                self.file_list_view.path = path.clone();
                self.file_list_view.path_is_dir = true;
                self.file_list_view.files.clear();
                self.file_list_view.clear_selected();
                change_host_folder(path);
            }
            return;
        }
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
                    if self.handling_filepick_request_from.is_some() {
                        self.send_filepick_response();
                    } else {
                        self.open_selected_path();
                    }
                },
            }
        } else if self.handling_filepick_request_from.is_some() {
            self.send_filepick_response();
        } else {
            self.open_selected_path();
        }
        self.is_searching = false;
        self.search_term.clear();
        self.search_view.clear_and_reset_selection();
    }
    pub fn update_files(&mut self, paths: Vec<(PathBuf, Option<FileMetadata>)>) {
        if self.is_in_virtual_root {
            self.update_virtual_root_entries(paths);
        } else {
            self.file_list_view
                .update_files(paths, self.hide_hidden_files);
        }
    }
    pub fn open_selected_path(&mut self) {
        if self.file_list_view.path_is_dir {
            if self.close_on_selection {
                open_terminal_in_place_of_plugin(&self.file_list_view.path, true);
            } else {
                open_terminal(&self.file_list_view.path);
            }
        } else {
            if let Some(parent_folder) = self.file_list_view.path.parent() {
                if self.close_on_selection {
                    open_file_in_place_of_plugin(
                        FileToOpen::new(&self.file_list_view.path).with_cwd(parent_folder.into()),
                        true,
                        BTreeMap::new(),
                    );
                } else {
                    open_file(
                        FileToOpen::new(&self.file_list_view.path).with_cwd(parent_folder.into()),
                        BTreeMap::new(),
                    );
                }
            } else {
                if self.close_on_selection {
                    open_file_in_place_of_plugin(
                        FileToOpen::new(&self.file_list_view.path),
                        true,
                        BTreeMap::new(),
                    );
                } else {
                    open_file(FileToOpen::new(&self.file_list_view.path), BTreeMap::new());
                }
            }
        }
    }
    pub fn enter_virtual_root(&mut self) {
        self.is_in_virtual_root = true;
        self.virtual_root_entries.clear();
        self.virtual_root_selected = 0;
        self.search_term.clear();
        self.is_searching = false;
        list_host_entries();
    }
    pub fn exit_virtual_root(&mut self) {
        self.is_in_virtual_root = false;
        refresh_directory(&self.file_list_view.path);
    }
    fn update_virtual_root_entries(&mut self, paths: Vec<(PathBuf, Option<FileMetadata>)>) {
        let mut entries = vec![];
        for (path, _metadata) in paths {
            let path = Platform::normalize(&path);
            entries.push(FsEntry::Dir(path));
        }
        // Drive letters first (e.g. "C:/"), then WSL distros (e.g. "//wsl.localhost/...")
        entries.sort_unstable_by(|a, b| {
            let a_is_drive = !a.get_full_pathbuf().to_string_lossy().starts_with("//");
            let b_is_drive = !b.get_full_pathbuf().to_string_lossy().starts_with("//");
            b_is_drive.cmp(&a_is_drive).then_with(|| a.cmp(b))
        });
        self.virtual_root_entries = entries;
        if self.virtual_root_selected >= self.virtual_root_entries.len() {
            self.virtual_root_selected = 0;
        }
    }
    pub fn render_virtual_root(&self, rows: usize, cols: usize) {
        let (start_index, selected_index_in_range, end_index) = calculate_list_bounds(
            self.virtual_root_entries.len(),
            rows.saturating_sub(1),
            Some(self.virtual_root_selected),
        );
        render_list_tip(3, cols);
        for i in start_index..end_index {
            if let Some(entry) = self.virtual_root_entries.get(i) {
                let is_selected = Some(i) == selected_index_in_range;
                let display_name =
                    Platform::virtual_root_display_name(&entry.get_full_pathbuf(), self.platform);
                let display_name_width = display_name.width();
                let text = if display_name_width < cols {
                    let padding = " ".repeat(cols.saturating_sub(display_name_width));
                    format!("{}{}", display_name, padding)
                } else {
                    display_name
                };
                let mut text_element = if is_selected {
                    Text::new(text).selected()
                } else {
                    Text::new(text)
                };
                text_element = text_element.color_range(0, ..);
                print_text_with_coordinates(
                    text_element,
                    0,
                    4 + i.saturating_sub(start_index),
                    Some(cols),
                    None,
                );
            }
        }
    }
    pub fn send_filepick_response(&mut self) {
        let selected_path = &self.file_list_view.path;
        let host_path = Platform::to_host_display(selected_path, self.platform);
        match &self.handling_filepick_request_from {
            Some((PipeSource::Plugin(plugin_id), args)) => {
                pipe_message_to_plugin(
                    MessageToPlugin::new("filepicker_result")
                        .with_destination_plugin_id(*plugin_id)
                        .with_args(args.clone())
                        .with_payload(host_path),
                );
                #[cfg(target_family = "wasm")]
                close_self();
            },
            #[allow(unused_variables)]
            // pipe_id is used inside #[cfg(target_family = "wasm")] blocks
            Some((PipeSource::Cli(pipe_id), _args)) => {
                #[cfg(target_family = "wasm")]
                cli_pipe_output(pipe_id, &host_path);
                #[cfg(target_family = "wasm")]
                unblock_cli_pipe_input(pipe_id);
                #[cfg(target_family = "wasm")]
                close_self();
            },
            _ => {},
        }
    }
}

pub(crate) fn refresh_directory(full_path: &Path) {
    change_host_folder(PathBuf::from(full_path));
}
