use crate::shared::calculate_list_bounds;
use crate::state::{refresh_directory, ROOT};
use std::collections::{HashMap};
use std::path::PathBuf;
use zellij_tile::prelude::*;
use unicode_width::UnicodeWidthStr;

#[derive(Default, Debug, Clone)]
pub struct FileListView {
    pub path: PathBuf,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, usize>,

}

impl FileListView {
    pub fn descend_to_previous_path(&mut self) {
        self.path.pop();
        refresh_directory(&self.path);
    }
    pub fn descend_to_root_path(&mut self) {
        self.path.clear();
        refresh_directory(&self.path);
    }
    pub fn enter_dir(&mut self, path: PathBuf) {
        self.path = path;
        *self.selected_mut() = self.selected().unwrap_or(0);
    }
    pub fn reset_selected(&mut self) {
        *self.selected_mut() = self.selected().unwrap_or(0);
    }
    pub fn update_files(&mut self, paths: Vec<(PathBuf, Option<FileMetadata>)>, hide_hidden_files: bool) {
        let mut files = vec![];
        for (entry, entry_metadata) in paths {
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
        let (start_index, selected_index_in_range, end_index) = calculate_list_bounds(self.files.len(), rows, self.selected());

        for i in start_index..end_index {
            let is_first_line = i == 0;

            if let Some(entry) = self.files.get(i) {
                let has_selection = selected_index_in_range.is_some();
                let is_selected = Some(i) == selected_index_in_range;
                let mut file_or_folder_name = entry.name();
                if entry.is_folder() {
                    file_or_folder_name.push('/');
                }
                let padding = " ".repeat(cols.saturating_sub(file_or_folder_name.width()).saturating_sub(6));

                let mut text_element = if is_selected {
                    Text::new(format!(" <↓↑> {}{}", file_or_folder_name, padding))
                        .color_range(3, 1..5)
                        .selected()
                } else if is_first_line && !has_selection {
                    Text::new(format!(" <↓↑> {}{}", file_or_folder_name, padding))
                        .color_range(3, 1..5)
                } else {
                    Text::new(format!("      {}{}", file_or_folder_name, padding))
                };

                if entry.is_folder() {
                    text_element = text_element.color_range(0, ..);
                }
                print_text_with_coordinates(text_element, 0, 2 + i.saturating_sub(start_index), None, None);
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
    pub fn get_pathbuf_without_root_prefix(&self) -> PathBuf {
        match self {
            FsEntry::Dir(p) => p.strip_prefix(ROOT).map(|p| p.to_path_buf()).unwrap_or_else(|_| p.clone()),
            FsEntry::File(p, _) => p.strip_prefix(ROOT).map(|p| p.to_path_buf()).unwrap_or_else(|_| p.clone()),
        }
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }

    pub fn is_folder(&self) -> bool {
        match self {
            FsEntry::Dir(_) => true,
            _ => false
        }
    }
}

