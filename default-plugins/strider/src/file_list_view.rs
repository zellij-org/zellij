use crate::shared::{calculate_list_bounds, render_list_tip};
use crate::state::refresh_directory;
use pretty_bytes::converter::convert as pretty_bytes;
use std::collections::HashMap;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

#[derive(Debug, Clone)]
pub struct FileListView {
    pub path: PathBuf,
    pub path_is_dir: bool,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, usize>,
}

impl Default for FileListView {
    fn default() -> Self {
        FileListView {
            path_is_dir: true,
            path: PathBuf::new(),
            files: Default::default(),
            cursor_hist: Default::default(),
        }
    }
}

impl FileListView {
    pub fn descend_to_previous_path(&mut self) {
        if let Some(parent) = self.path.parent() {
            self.path = parent.to_path_buf();
        } else {
            self.path = PathBuf::new();
        }
        self.path_is_dir = true;
        self.files.clear();
        self.clear_selected();
        refresh_directory(&self.path);
    }

    pub fn descend_to_root_path(&mut self, initial_cwd: &PathBuf) {
        self.path = initial_cwd.clone();
        self.path_is_dir = true;
        self.files.clear();
        self.clear_selected();
    }

    pub fn enter_dir(&mut self, entry: &FsEntry) {
        let is_dir = entry.is_folder();
        let path = entry.get_full_pathbuf();
        self.path = path;
        self.path_is_dir = is_dir;
        self.files.clear();
        self.clear_selected();
    }

    pub fn clear_selected(&mut self) {
        self.cursor_hist.remove(&self.path);
    }

    pub fn update_files(
        &mut self,
        paths: Vec<(PathBuf, Option<FileMetadata>)>,
        hide_hidden_files: bool,
    ) {
        let mut files = vec![];
        for (entry, entry_metadata) in paths {
            let entry = self
                .path
                .join(entry.strip_prefix("/host").unwrap_or(&entry));
            if entry_metadata.map(|e| e.is_symlink).unwrap_or(false) {
                continue;
            }
            let entry = if entry_metadata.map(|e| e.is_dir).unwrap_or(false) {
                FsEntry::Dir(entry)
            } else {
                let size = entry_metadata.map(|e| e.len).unwrap_or(0);
                FsEntry::File(entry, size)
            };
            if !entry.is_hidden_file() || !hide_hidden_files {
                files.push(entry);
            }
        }
        self.files = files;
        self.files.sort_unstable();
    }

    pub fn get_selected_entry(&self) -> Option<FsEntry> {
        self.selected().and_then(|f| self.files.get(f).cloned())
    }

    pub fn selected_mut(&mut self) -> &mut usize {
        self.cursor_hist.entry(self.path.clone()).or_default()
    }

    pub fn selected(&self) -> Option<usize> {
        self.cursor_hist.get(&self.path).copied()
    }

    pub fn move_selection_up(&mut self) {
        if let Some(selected) = self.selected() {
            *self.selected_mut() = selected.saturating_sub(1);
        }
    }

    pub fn move_selection_down(&mut self) {
        if let Some(selected) = self.selected() {
            let next = selected.saturating_add(1);
            *self.selected_mut() = std::cmp::min(self.files.len().saturating_sub(1), next);
        } else {
            *self.selected_mut() = 0;
        }
    }

    pub fn render(&mut self, rows: usize, cols: usize) {
        let (start_index, selected_index_in_range, end_index) =
            calculate_list_bounds(self.files.len(), rows.saturating_sub(1), self.selected());

        render_list_tip(3, cols);
        for i in start_index..end_index {
            if let Some(entry) = self.files.get(i) {
                let is_selected = Some(i) == selected_index_in_range;
                let mut file_or_folder_name = entry.name();
                let size = entry
                    .size()
                    .map(|s| pretty_bytes(s as f64))
                    .unwrap_or("".to_owned());
                if entry.is_folder() {
                    file_or_folder_name.push('/');
                }
                let file_or_folder_name_width = file_or_folder_name.width();
                let size_width = size.width();
                let text = if file_or_folder_name_width + size_width < cols {
                    let padding = " ".repeat(
                        cols.saturating_sub(file_or_folder_name_width)
                            .saturating_sub(size_width),
                    );
                    format!("{}{}{}", file_or_folder_name, padding, size)
                } else {
                    let padding = " ".repeat(cols.saturating_sub(file_or_folder_name_width));
                    format!("{}{}", file_or_folder_name, padding)
                };
                let mut text_element = if is_selected {
                    Text::new(text).selected()
                } else {
                    Text::new(text)
                };
                if entry.is_folder() {
                    text_element = text_element.color_range(0, ..);
                }
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
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
pub enum FsEntry {
    Dir(PathBuf),
    File(PathBuf, u64),
}

impl FsEntry {
    pub fn name(&self) -> String {
        let path = match self {
            FsEntry::Dir(p) => p,
            FsEntry::File(p, _) => p,
        };
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    pub fn size(&self) -> Option<u64> {
        match self {
            FsEntry::Dir(_p) => None,
            FsEntry::File(_, size) => Some(*size),
        }
    }

    pub fn get_full_pathbuf(&self) -> PathBuf {
        match self {
            FsEntry::Dir(p) => p.clone(),
            FsEntry::File(p, _) => p.clone(),
        }
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }

    pub fn is_folder(&self) -> bool {
        matches!(self, FsEntry::Dir(_))
    }
}
