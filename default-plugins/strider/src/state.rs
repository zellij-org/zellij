use crate::search::{SearchResult, SearchFilter};
use pretty_bytes::converter as pb;
use std::{
    collections::{HashMap, VecDeque},
    fs::read_dir,
    path::{Path, PathBuf},
    time::Instant,
};
use zellij_tile::prelude::*;

pub const ROOT: &str = "/host";
// pub const ROOT: &str = "/tmp"; // TODO: no!!
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
    pub file_name_search_results: Vec<SearchResult>,
    pub file_contents_search_results: Vec<SearchResult>,
    pub loading: bool,
    pub loading_animation_offset: u8,
    pub typing_search_term: bool,
    pub selected_search_result: usize,
    pub processed_search_index: usize,
    pub should_open_floating: bool,
    pub search_filter: SearchFilter,
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
                        self.file_name_search_results.clear();
                        self.file_contents_search_results.clear();
                        // TODO: CONTINUE HERE
                        // * take search_index out of search_term and put it in
                        // self.processed_search_index instead
                        // * use self.processed_search_index whenever creating a search_index and
                        // increment it when appending
                        self.typing_search_term = false;
                    }
                }
            },
            _ => {},
        }
    }
    pub fn typing_search_term(&self) -> bool {
        self.typing_search_term
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
        if self.selected_search_result < self.file_name_search_results.len() + self.file_contents_search_results.len() {
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
    pub fn open_search_result_in_editor(&mut self) {
        let all_search_results = self.all_search_results();
        match all_search_results.get(self.selected_search_result) {
            Some(SearchResult::File {
                path,
                score,
                indices,
            }) => {
                if self.should_open_floating {
                    open_file_floating(&PathBuf::from(path));
                } else {
                    open_file(&PathBuf::from(path));
                }
            },
            Some(SearchResult::LineInFile {
                path,
                score,
                indices,
                line,
                line_number,
            }) => {
                // open_file_with_line(file_path.strip_prefix(ROOT).unwrap(), *line_number);
                if self.should_open_floating {
                    open_file_with_line_floating(&PathBuf::from(path), *line_number);
                } else {
                    open_file_with_line(&PathBuf::from(path), *line_number);
                }
                // open_file_with_line(&file_path, *line_number); // TODO: no!!
            },
            None => {
                eprintln!("Search result not found");
            },
        }
    }
    pub fn open_search_result_in_terminal(&mut self) {
        // TODO: actually open in terminal and not in editor
        let all_search_results = self.all_search_results();
        match all_search_results.get(self.selected_search_result) {
            Some(SearchResult::File {
                path,
                score,
                indices,
            }) => {
                let file_path = PathBuf::from(path);
                let mut dir_path = file_path.components();
                drop(dir_path.next_back()); // remove file name to stay with just the folder
                let dir_path = dir_path.as_path();
                eprintln!("dir_path: {:?}", dir_path);
                if self.should_open_floating {
                    open_terminal_floating(&dir_path);
                } else {
                    open_terminal(&dir_path);
                }
            },
            Some(SearchResult::LineInFile {
                path,
                score,
                indices,
                line,
                line_number,
            }) => {
                let file_path = PathBuf::from(path);
                let mut dir_path = file_path.components();
                drop(dir_path.next_back()); // remove file name to stay with just the folder
                let dir_path = dir_path.as_path();
                eprintln!("dir_path: {:?}", dir_path);
                if self.should_open_floating {
                    open_terminal_floating(dir_path);
                } else {
                    open_terminal(dir_path);
                }
                // open_file_with_line(&file_path, *line_number); // TODO: no!!
            },
            None => {
                eprintln!("Search result not found");
            },
        }
    }
    pub fn stringify_search_term(&self) -> Option<String> {
        if let Some(search_term) = self.search_term.as_ref() {
            serde_json::to_string(&(search_term, self.processed_search_index)).ok()
        } else {
            None
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
