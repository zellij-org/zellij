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
    pub current_dir: PathBuf,
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
                FsEntry::File(p, _) => open_file(p.strip_prefix(ROOT).unwrap()),
                FsEntry::DisplayDir(_) => {},
            }
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FsEntry {
    Dir(PathBuf, usize),
    File(PathBuf, u64),
    DisplayDir(PathBuf),
}

impl FsEntry {
    pub fn name(&self) -> String {
        let path = match self {
            FsEntry::Dir(p, _) => p,
            FsEntry::File(p, _) => p,
            FsEntry::DisplayDir(p) => p,
        };
        match self {
            FsEntry::File(..) | FsEntry::Dir(..) => {
                // only use the filename
                path.file_name().unwrap().to_string_lossy().into_owned()
            },
            FsEntry::DisplayDir(..) => {
                // use full path, but we need to remove the host part
                let path = path.to_string_lossy().into_owned().replace("/host", "");
                ".".to_string() + &path
            },
        }
    }

    pub fn as_line(&self, width: usize) -> String {
        let name = self.name();
        let info = match self {
            FsEntry::Dir(_, s) => s.to_string(),
            FsEntry::File(_, s) => pb::convert(*s as f64),
            FsEntry::DisplayDir(..) => {
                let current_path = name.clone();
                FsEntry::display_with_shortened_name_start(current_path, width)
            },
        };
        // + 1 since we want to have a space between name and info
        let content_width = name.len() + info.len() + 1;
        if width < content_width {
            // The content doesn't fit on the screen
            match self {
                FsEntry::File(..) | FsEntry::Dir(..) => {
                    FsEntry::display_with_shortened_name_end(name, info, width)
                },
                FsEntry::DisplayDir(..) => {
                    let current_path = name;
                    FsEntry::display_with_shortened_name_start(current_path, width)
                },
            }
        } else {
            // The content does fit on the screen
            FsEntry::display_with_padding(name, info, width)
        }
    }

    fn display_with_shortened_name_end(name: String, extra_info: String, width: usize) -> String {
        const ENDING: &str = "~ ";
        let shortened_name_len = width.saturating_sub(ENDING.len() + extra_info.len());
        let shortened_name = &name[..shortened_name_len];
        [shortened_name, &extra_info].join(ENDING)
    }

    fn display_with_shortened_name_start(current_path: String, width: usize) -> String {
        const FRONT: &str = "./...";
        const END: &str = " ";
        const NEEDED_SPACE: usize = FRONT.len() + END.len();
        let current_path_len = current_path.len();
        let displayed_path_len = width.saturating_sub(NEEDED_SPACE);
        let display_path_start_index = current_path_len.saturating_sub(displayed_path_len);
        let shortened_path = &current_path[display_path_start_index..];
        [FRONT, shortened_path, END].join("")
    }

    fn display_with_padding(name: String, extra_info: String, width: usize) -> String {
        let content_len = name.len() + extra_info.len();
        let padding = " ".repeat(width - content_len);
        [name, padding, extra_info].concat()
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }
}

pub(crate) fn refresh_directory(state: &mut State) {
    state.current_dir = state.path.clone();
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
