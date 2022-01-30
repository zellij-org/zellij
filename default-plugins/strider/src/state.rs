use pretty_bytes::converter as pb;
use std::{
    collections::{HashMap, VecDeque},
    fmt::Display,
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

/// Newtype for Number of Children in a Directory
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NumChildren(usize);

impl Display for NumChildren {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Newtype for Size of File in Bytes
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileSize(u64);

impl Display for FileSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", pb::convert(self.0 as f64))
    }
}

/// Enum for File System Entry with various types of entries
#[derive(PartialEq, Eq, PartialOrd, Ord, Clone)]
pub enum FsEntry {
    OpenableDir(PathBuf, NumChildren),
    File(PathBuf, FileSize),
    DisplayDir(PathBuf),
}

impl FsEntry {
    /// Get the name of the FsEntry
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

    /// Convert FsEntry to String which perfectly fits in the given width.
    ///
    /// If the content is too long to fit in the width, it is shortened accorrding to the type of
    /// FsEntry, see
    /// `display_with_shortened_name_end`
    /// and
    /// `display_with_shortened_name_start`
    ///
    pub fn as_line(&self, width: usize) -> String {
        let info = self.get_info();
        let name = self.name();

        // + 1 since we want to have a space between name and info
        let content_width = name.len() + info.len() + 1;
        if width < content_width {
            // The content doesn't fit on the screen
            match self {
                FsEntry::File(..) | FsEntry::OpenableDir(..) => {
                    FsEntry::display_with_shortened_name_end(name, info, width)
                }
                FsEntry::DisplayDir(..) => {
                    let current_path = name;
                    FsEntry::display_with_shortened_name_start(current_path, width)
                }
            }
        } else {
            // The content does fit on the screen
            FsEntry::display_with_padding(name, info, width)
        }
    }

    /// Calculates the displayable string which is shortened at the end
    ///
    /// Example:
    /// name = "a_veeeeeeeeery_looooooong_naaaaaaame"
    /// extra_info = "10"
    /// width = 32
    ///
    /// This leads to a display looking as follows
    ///
    /// |                                 |
    /// |a_veeeeeeeeery_looooooong_naa~ 10|
    /// |                                 |
    ///
    fn display_with_shortened_name_end(name: String, extra_info: String, width: usize) -> String {
        const ENDING: &str = "~ ";
        let shortened_name_len = width.saturating_sub(ENDING.len() + extra_info.len());
        let shortened_name = &name[..shortened_name_len];
        [shortened_name, &extra_info].join(ENDING)
    }

    /// Calculates the displayable string which is shortened at the beginning, since we are
    /// interested in the last parts of the path
    ///
    /// Example:
    /// name = "path_a/path_b/path_c/path_d/path_e/path_f"
    /// width = 32
    ///
    /// This leads to a display looking as follows
    ///
    /// |                                 |
    /// |./.../ath_c/path_d/path_e/path_f |
    /// |                                 |
    ///
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

    /// Fills out the space between `name` and `extra_info` with spaces
    fn display_with_padding(name: String, extra_info: String, width: usize) -> String {
        let content_len = name.len() + extra_info.len();
        let padding = " ".repeat(width - content_len);
        [name, padding, extra_info].concat()
    }

    /// Gets additional information for FsEntry
    ///
    /// For OpenableDir : Number of Children in this Dir
    /// For File        : File Size
    /// For DisplayDir  : -
    ///
    fn get_info(&self) -> String {
        match self {
            FsEntry::OpenableDir(_, num_children) => num_children.to_string(),
            FsEntry::File(_, file_size) => file_size.to_string(),
            FsEntry::DisplayDir(_) => Default::default(),
        }
    }

    pub fn is_hidden_file(&self) -> bool {
        self.name().starts_with('.')
    }
}

pub(crate) fn refresh_directory(state: &mut State) {
    // update the current dir
    state.current_dir = state.path.clone();

    // get contents of dir with path `state.path`
    state.files = read_dir(Path::new(ROOT).join(&state.path))
        .unwrap()
        .filter_map(|res| {
            res.and_then(|d| {
                if d.metadata()?.is_dir() {
                    let children = read_dir(d.path())?.count();
                    Ok(FsEntry::OpenableDir(d.path(), NumChildren(children)))
                } else {
                    let size = d.metadata()?.len();
                    Ok(FsEntry::File(d.path(), FileSize(size)))
                }
            })
            .ok()
            .filter(|d| !d.is_hidden_file() || !state.hide_hidden_files)
        })
        .collect();
    // sort contents of dir with path `state.path`
    state.files.sort_unstable();
}
