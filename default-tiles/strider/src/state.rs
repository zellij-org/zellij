use pretty_bytes::converter as pb;
use std::{collections::HashMap, path::PathBuf};

#[derive(Default)]
pub struct State {
    pub path: PathBuf,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, (usize, usize)>,
    pub hide_hidden_files: bool,
}

impl State {
    pub fn selected_mut(&mut self) -> &mut usize {
        &mut self.cursor_hist.entry(self.path.clone()).or_default().0
    }
    pub fn selected(&self) -> usize {
        self.cursor_hist.get(&self.path).unwrap_or(&(0, 0)).0
    }
    pub fn scroll_mut(&mut self) -> &mut usize {
        &mut self.cursor_hist.entry(self.path.clone()).or_default().1
    }
    pub fn scroll(&self) -> usize {
        self.cursor_hist.get(&self.path).unwrap_or(&(0, 0)).1
    }
    pub fn toggle_hidden_files(&mut self) {
        self.hide_hidden_files = !self.hide_hidden_files;
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FsEntry {
    Dir(PathBuf, usize),
    File(PathBuf, u64),
}

impl FsEntry {
    pub fn name(&self) -> String {
        let path = match self {
            FsEntry::Dir(p, _) => p,
            FsEntry::File(p, _) => p,
        };
        path.file_name().unwrap().to_string_lossy().into_owned()
    }

    pub fn as_line(&self, width: usize) -> String {
        let info = match self {
            FsEntry::Dir(_, s) => s.to_string(),
            FsEntry::File(_, s) => pb::convert(*s as f64),
        };
        let space = width - info.len();
        let name = self.name();
        if space - 1 < name.len() {
            [&name[..space - 2], &info].join("~ ")
        } else {
            let padding = " ".repeat(space - name.len());
            [name, padding, info].concat()
        }
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }
}
