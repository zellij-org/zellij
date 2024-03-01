use pretty_bytes::converter as pb;
use std::{
    collections::{HashMap, VecDeque, BTreeMap},
    fs::read_dir,
    path::{Path, PathBuf},
    time::Instant,
};
use zellij_tile::prelude::*;

pub const ROOT: &str = "/host";

#[derive(Default)]
pub struct State {
    pub path: PathBuf,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, (usize, usize)>,
    pub hide_hidden_files: bool,
    pub ev_history: VecDeque<(Event, Instant)>, // stores last event, can be expanded in future
    pub loading: bool,
    pub loading_animation_offset: u8,
    pub should_open_floating: bool,
    pub current_rows: Option<usize>,
    pub handling_filepick_request_from: Option<(PipeSource, BTreeMap<String, String>)>,
    pub initial_cwd: PathBuf, // TODO: get this from zellij
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
                FsEntry::Dir(p) => {
                    self.path = p;
                    refresh_directory(self);
                },
                FsEntry::File(p, _) => open_file(FileToOpen {
                    path: p.strip_prefix(ROOT).unwrap().into(),
                    ..Default::default()
                }),
            }
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
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
    pub fn get_pathbuf(&self) -> PathBuf {
        match self {
            FsEntry::Dir(p) => p.clone(),
            FsEntry::File(p, _) => p.clone(),
        }
    }

    pub fn as_line(&self, width: usize) -> String {
        let info = match self {
            FsEntry::Dir(_s) => "".to_owned(),
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
    // TODO: might be good to do this asynchronously with a worker
    let mut max_lines = (state.current_rows.unwrap_or(50) + state.scroll()) * 2; // 100 is arbitrary for performance reasons
    let mut files = vec![];
    for entry in read_dir(Path::new(ROOT).join(&state.path)).unwrap() {
        if let Ok(entry) = entry {
            if max_lines == 0 {
                break;
            }
            eprintln!("entry: {:?}", entry);
            if let Ok(entry_metadata) = entry.metadata() {
                let entry = if entry_metadata.is_dir() {
                    FsEntry::Dir(entry.path())
                } else {
                    let size = entry_metadata.len();
                    FsEntry::File(entry.path(), size)
                };
                if !entry.is_hidden_file() || !state.hide_hidden_files {
                    max_lines = max_lines.saturating_sub(1);
                    files.push(entry);
                }
            }
        }
    }
    state.files = files;
    state.files.sort_unstable();
}
