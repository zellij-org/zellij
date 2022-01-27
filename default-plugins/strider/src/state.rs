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
    pub ev_history: VecDeque<(Event, Instant)>, // - stores last event, can be expanded in future
    pub current_dir: PathBuf,                   // - stores current relative path with
                                                //   respect to the dir in which zellij
                                                //   was opened in
}

impl State {
    /// Same as `self.selected()` but returns a mutable reference.
    pub fn selected_mut(&mut self) -> &mut usize {
        &mut self.cursor_hist.entry(self.path.clone()).or_default().0
    }
    /// Returns the index of the selected item in a directory given as `self.path`.
    ///
    /// The corresponding FsEntry for this item can be found in `self.files` at this index.
    ///
    /// The selected item is highlighted to show that actions (like going into a dir, opening a
    /// file) are going to effect it
    pub fn selected(&self) -> usize {
        const DEFAULT_SELECTION: usize = 0;
        self.cursor_hist
            .get(&self.path)
            .map(|&(lastest_selection_state, _)| lastest_selection_state)
            .unwrap_or(DEFAULT_SELECTION)
    }
    pub fn scroll_mut(&mut self) -> &mut usize {
        &mut self.cursor_hist.entry(self.path.clone()).or_default().1
    }
    pub fn scroll(&self) -> usize {
        const DEFAULT_SCROLL: usize = 0;
        self.cursor_hist
            .get(&self.path)
            .map(|&(_, latest_scroll_state)| latest_scroll_state)
            .unwrap_or(DEFAULT_SCROLL)
    }
    pub fn toggle_hidden_files(&mut self) {
        self.hide_hidden_files = !self.hide_hidden_files;
    }
    pub fn traverse_dir_or_open_file(&mut self) {
        match self.files[self.selected()].clone() {
            FsEntry::OpenableDir(p, _) => {
                self.path = p;
                refresh_directory(self);
            }
            FsEntry::File(p, _) => open_file(p.strip_prefix(ROOT).unwrap()),
            FsEntry::DisplayDir(_) => {}
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FsEntry {
    OpenableDir(PathBuf, usize),
    File(PathBuf, u64),
    DisplayDir(PathBuf),
}

impl FsEntry {
    pub fn name(&self) -> String {
        let path = match self {
            FsEntry::OpenableDir(p, _) => p,
            FsEntry::File(p, _) => p,
            FsEntry::DisplayDir(p) => p,
        };
        match self {
            FsEntry::File(..) | FsEntry::OpenableDir(..) => {
                // only use the filename
                path.file_name().unwrap().to_string_lossy().into_owned()
            }
            FsEntry::DisplayDir(..) => {
                // use full path, but we need to remove the host part
                let path = path.to_string_lossy().into_owned().replace("/host", "");
                ".".to_string() + &path
            }
        }
    }

    pub fn as_line(&self, width: usize) -> String {
        let info = match self {
            FsEntry::OpenableDir(_, s) => s.to_string(),
            FsEntry::File(_, s) => pb::convert(*s as f64),
            FsEntry::DisplayDir(_) => "".to_string(),
        };
        let space = width.saturating_sub(info.len());
        let name = self.name();
        if space.saturating_sub(1) < name.len() {
            match self {
                FsEntry::File(..) | FsEntry::OpenableDir(..) => {
                    [&name[..space.saturating_sub(2)], &info].join("~ ")
                }
                FsEntry::DisplayDir(..) => {
                    let valid_range_start = name.len().saturating_sub(space.saturating_sub(8));
                    "./.../".to_string() + &name[valid_range_start..] + "  "
                }
            }
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
    // update the current dir
    state.current_dir = state.current_dir.join(&state.path);

    // get contents of dir with path `state.path`
    state.files = read_dir(Path::new(ROOT).join(&state.path))
        .unwrap()
        .filter_map(|res| {
            res.and_then(|d| {
                if d.metadata()?.is_dir() {
                    let children = read_dir(d.path())?.count();
                    Ok(FsEntry::OpenableDir(d.path(), children))
                } else {
                    let size = d.metadata()?.len();
                    Ok(FsEntry::File(d.path(), size))
                }
            })
            .ok()
            .filter(|d| !d.is_hidden_file() || !state.hide_hidden_files)
        })
        .collect();
    // sort contents of dir with path `state.path`
    state.files.sort_unstable();
}
