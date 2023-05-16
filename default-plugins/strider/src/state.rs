use crate::search::SearchResult;
use pretty_bytes::converter as pb;
use std::{
    collections::{HashMap, VecDeque},
    fs::read_dir,
    path::{Path, PathBuf},
    time::Instant,
};
use zellij_tile::prelude::*;

pub const ROOT: &str = "/host";
pub const CURRENT_SEARCH_TERM: &str = "/data/current_search_term";

#[derive(Default)]
pub struct State {
    pub path: PathBuf,
    pub files: Vec<FsEntry>,
    pub cursor_hist: HashMap<PathBuf, (usize, usize)>,
    pub hide_hidden_files: bool,
    pub ev_history: VecDeque<(Event, Instant)>, // stores last event, can be expanded in future
    pub search_paths: Vec<String>,
    pub search_term: Option<String>,
    pub search_results: Vec<SearchResult>,
    pub loading: bool,
    pub loading_animation_offset: u8,
    pub typing_search_term: bool,
    pub exploring_search_results: bool,
    pub selected_search_result: usize,
}

impl State {
    pub fn append_to_search_term(&mut self, key: Key) {
        match key {
            Key::Char(character) => {
                if let Some(search_term) = self.search_term.as_mut() {
                    search_term.push(character);
                }
            },
            Key::Backspace => {
                if let Some(search_term) = self.search_term.as_mut() {
                    search_term.pop();
                    if search_term.len() == 0 {
                        self.search_term = None;
                        self.typing_search_term = false;
                    }
                }
            },
            _ => {},
        }
    }
    pub fn accept_search_term(&mut self) {
        self.typing_search_term = false;
        self.exploring_search_results = true;
    }
    pub fn typing_search_term(&self) -> bool {
        self.typing_search_term
    }
    pub fn exploring_search_results(&self) -> bool {
        self.exploring_search_results
    }
    pub fn stop_exploring_search_results(&mut self) {
        self.exploring_search_results = false;
    }
    pub fn start_typing_search_term(&mut self) {
        if self.search_term.is_none() {
            self.search_term = Some(String::new());
        }
        self.typing_search_term = true;
    }
    pub fn stop_typing_search_term(&mut self) {
        self.typing_search_term = true;
    }
    pub fn move_search_selection_up(&mut self) {
        self.selected_search_result = self.selected_search_result.saturating_sub(1);
    }
    pub fn move_search_selection_down(&mut self) {
        if self.selected_search_result < self.search_results.len() {
            self.selected_search_result = self.selected_search_result.saturating_add(1);
        }
    }
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
            }
        }
    }
    pub fn open_search_result(&mut self) {
        match self.search_results.get(self.selected_search_result) {
            Some(SearchResult::File {
                path,
                score,
                indices,
            }) => {
                let file_path = PathBuf::from(path);
                open_file(file_path.strip_prefix(ROOT).unwrap());
            },
            Some(SearchResult::LineInFile {
                path,
                score,
                indices,
                line,
                line_number,
            }) => {
                let file_path = PathBuf::from(path);
                open_file_with_line(file_path.strip_prefix(ROOT).unwrap(), *line_number);
                // open_file_with_line(&file_path, *line_number); // TODO: no!!
            },
            None => {
                eprintln!("Search result not found");
            },
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
