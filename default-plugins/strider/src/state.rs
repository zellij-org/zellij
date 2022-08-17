use pretty_bytes::converter as pb;
use std::{
    collections::{HashMap, VecDeque},
    fs::read_dir,
    path::{Path, PathBuf},
    time::Instant,
};
use zellij_tile::prelude::*;

const ROOT: &str = "/host";
#[derive(Default)]
pub struct State {
    pub path: PathBuf,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, (usize, usize)>,
    pub hide_hidden_files: bool,
    pub ev_history: VecDeque<(Event, Instant)>, // stores last event, can be expanded in future
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
    pub fn traverse_dir_or_open_file(&mut self) {
        if let Some(f) = self.files.get(self.selected()) {
            match f.clone() {
                FsEntry::Dir(p, _) => {
                    self.path = p;
                    refresh_directory(self);
                },
                FsEntry::File(p, _) => {
                    open_file(p.strip_prefix(ROOT).unwrap()).expect(errors::TODO_HANDLING)
                },
            }
        }
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
        let space = width.saturating_sub(info.len());
        let name = self.name();
        if space.saturating_sub(1) < name.len() {
            [&name[..space.saturating_sub(2)], &info].join("~ ")
        } else {
            let padding = " ".repeat(space - name.len());
            [name, padding, info].concat()
        }
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }
}

pub(crate) fn refresh_directory(state: &mut State) {
    state.files = read_dir(Path::new(ROOT).join(&state.path))
        .unwrap()
        .filter_map(|res| {
            res.and_then(|d| {
                if d.metadata()?.is_dir() {
                    let children = read_dir(d.path())?.count();
                    Ok(FsEntry::Dir(d.path(), children))
                } else {
                    let size = d.metadata()?.len();
                    Ok(FsEntry::File(d.path(), size))
                }
            })
            .ok()
            .filter(|d| !d.is_hidden_file() || !state.hide_hidden_files)
        })
        .collect();

    state.files.sort_unstable();
}
