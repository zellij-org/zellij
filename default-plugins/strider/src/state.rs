use crate::file_list_view::{FileListView, FsEntry};
use crate::search_view::SearchView;
use crate::shared::calculate_list_bounds;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use zellij_tile::prelude::*;

pub const ROOT: &str = "/host";

#[derive(Default, Clone)]
pub enum Mode {
    #[default]
    Normal,
    // Loading,
    Searching,
    Keybinds,
    Create,
    Copy,
    Delete,
    Move,
}

#[derive(Default)]
pub struct State {
    pub file_list_view: FileListView,
    pub search_view: SearchView,
    pub hide_hidden_files: bool,
    pub loading_animation_offset: u8,
    pub should_open_floating: bool,
    pub current_rows: Option<usize>,
    pub handling_filepick_request_from: Option<(PipeSource, BTreeMap<String, String>)>,
    pub initial_cwd: PathBuf, // TODO: get this from zellij
    pub search_term: String,
    pub close_on_selection: bool,
    pub mode: Mode,
}

impl State {
    pub fn update_search_term(&mut self, character: char) {
        self.search_term.push(character);
        match self.mode {
            Mode::Create | Mode::Copy | Mode::Delete | Mode::Move => return,
            _ => {
                if self.search_term == ".." {
                    self.descend_to_previous_path();
                } else if &self.search_term == "/" {
                    self.descend_to_root_path();
                } else {
                    self.mode = Mode::Searching;
                    self.search_view
                        .update_search_results(&self.search_term, &self.file_list_view.files);
                }
            },
        }
    }
    pub fn handle_backspace(&mut self) {
        if self.search_term.is_empty() {
            match self.mode {
                Mode::Normal | Mode::Searching => self.descend_to_previous_path(),
                _ => {},
            }
        } else {
            self.search_term.pop();
            if self.search_term.is_empty() {
                match self.mode {
                    Mode::Searching => self.mode = Mode::Normal,
                    _ => {},
                }
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
        }
        self.mode = Mode::Normal;
    }
    pub fn move_selection_up(&mut self) {
        match self.mode {
            Mode::Searching => self.search_view.move_selection_up(),
            _ => self.file_list_view.move_selection_up(),
        };
    }
    pub fn move_selection_down(&mut self) {
        match self.mode {
            Mode::Searching => self.search_view.move_selection_down(),
            _ => self.file_list_view.move_selection_down(),
        }
    }
    pub fn get_selected_entry(&mut self) -> Option<FsEntry> {
        let entry = match self.mode {
            Mode::Searching => self.search_view.get_selected_entry(),
            _ => self.file_list_view.get_selected_entry(),
        };
        return entry;
    }
    pub fn move_entry_to_search(&mut self) {
        if let Some(entry) = self.get_selected_entry() {
            self.search_term = entry
                .get_pathbuf_without_root_prefix()
                .display()
                .to_string();
        }
    }
    pub fn handle_left_click(&mut self, line: isize) {
        if let Some(current_rows) = self.current_rows {
            let rows_for_list = current_rows.saturating_sub(5);
            match self.mode {
                Mode::Searching => {
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
                },
                _ => {
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
                },
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
        let entry = match self.mode {
            Mode::Searching => self.search_view.get_selected_entry(),
            _ => self.file_list_view.get_selected_entry(),
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
        self.mode = Mode::Normal;
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
            close_focus();
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
                close_focus();
            },
            Some((PipeSource::Cli(pipe_id), _args)) => {
                #[cfg(target_family = "wasm")]
                cli_pipe_output(pipe_id, &selected_path.display().to_string());
                #[cfg(target_family = "wasm")]
                unblock_cli_pipe_input(pipe_id);
                #[cfg(target_family = "wasm")]
                close_focus();
            },
            _ => {},
        }
    }

    pub fn handle_file_manipulation(&mut self) {
        let mut target = self.initial_cwd.clone();
        target.push(&self.search_term);

        let mut src = self.initial_cwd.clone();
        let entry = self
            .get_selected_entry()
            .expect("Expected a selected entry");
        src.push(entry.get_pathbuf_without_root_prefix());

        match self.mode {
            Mode::Create => self.handle_file_create(target),
            Mode::Copy => self.handle_file_copy(src, target),
            Mode::Move => self.handle_file_move(src, target),
            Mode::Delete => {
                if self.search_term == "y" {
                    self.handle_file_delete(src);
                }
            },
            _ => {},
        };
        self.clear_search_term_or_descend(); // resets mode to Normal
    }
    fn handle_file_create(&self, target: PathBuf) {
        if let Err(err) = std::fs::create_dir(target) {
            dbg!("Could not create file: {:?}", err);
        }
    }
    fn handle_file_copy(&self, source: PathBuf, target: PathBuf) {
        if let Err(err) = std::fs::copy(source, target) {
            dbg!("Cound not copy file: {:?}", err);
        }
    }
    fn handle_file_move(&mut self, source: PathBuf, target: PathBuf) {
        if let Err(err) = std::fs::rename(source, target) {
            dbg!("Cound not rename file: {:?}", err);
        }
    }
    fn handle_file_delete(&self, source: PathBuf) {
        if let Err(err) = std::fs::remove_file(source) {
            dbg!("Cound not delete file: {:?}", err);
        }
    }
}

pub(crate) fn refresh_directory(path: &Path) {
    let path_on_host = Path::new(ROOT).join(path.strip_prefix("/").unwrap_or(path));
    scan_host_folder(&path_on_host);
}
