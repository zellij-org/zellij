use crate::state::{State, CURRENT_SEARCH_TERM, ROOT};
use std::time::Instant;
use std::path::{PathBuf, Path};
use std::collections::{BTreeMap, BTreeSet};

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use std::io::{self, BufRead};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SearchResult {
    File {
        path: String,
        score: i64,
        indices: Vec<usize>,
    },
    LineInFile {
        path: String,
        line: String,
        line_number: usize,
        score: i64,
        indices: Vec<usize>,
    },
}

impl SearchResult {
    pub fn new_file_name(score: i64, indices: Vec<usize>, path: String) -> Self {
        SearchResult::File {
            path,
            score,
            indices,
        }
    }
    pub fn new_file_line(
        score: i64,
        indices: Vec<usize>,
        path: String,
        line: String,
        line_number: usize,
    ) -> Self {
        SearchResult::LineInFile {
            path,
            score,
            indices,
            line,
            line_number,
        }
    }
    pub fn score(&self) -> i64 {
        match self {
            SearchResult::File { score, .. } => *score,
            SearchResult::LineInFile { score, .. } => *score,
        }
    }
    pub fn rendered_height(&self) -> usize {
        match self {
            SearchResult::File { .. } => 1,
            SearchResult::LineInFile { .. } => 2,
        }
    }
    pub fn render(&self, max_width: usize, is_selected: bool, is_below_search_result: bool) -> String {
        let green_code = 154;
        let orange_code = 166;
        let gray_code = 255;
        let black_code = 0;
        let white_code = 15;
        let bold_code = "\u{1b}[1m";
        let green_foreground = format!("\u{1b}[38;5;{};1m", green_code);
        let orange_foreground = format!("\u{1b}[38;5;{}m", orange_code);
        let reset_code = "\u{1b}[m";
        let max_width = max_width.saturating_sub(4); // for the UI left line separator
        match self {
            SearchResult::File { path, indices, .. } => {
                if is_selected {
                    let line = self.render_file_name_line(
                        path,
                        indices,
                        max_width,
                        None,
                        Some(green_code),
                        true,
                    );
                    // format!("{} | {}{}", green_foreground, reset_code, line)
                    // format!("├{}", line)
                    format!("┌\u{1b}[38;5;166;1m>\u{1b}[0m {}", line)
                } else {
                    let line =
                        self.render_file_name_line(path, indices, max_width, None, None, true);
                    if is_below_search_result {
                        format!("│  {}", line)
                    } else {
                        format!("   {}", line)
                    }
                }
            },
            SearchResult::LineInFile {
                path,
                line,
                line_number,
                indices,
                ..
            } => {
                if is_selected {

//                     let file_name = PathBuf::from(path).file_name().unwrap().to_string_lossy().to_string(); // TODO: no unwrap and such
                    let file_name_indication = format!("{}", line_number);
                    let file_name_line = self.render_file_name_line(
                        path,
                        &vec![],
                        max_width.saturating_sub(2),
                        None,
                        Some(green_code),
                        true,
                    );
                    let line_in_file = self.render_file_contents_line(
                        line,
                        indices,
                        max_width.saturating_sub(3).saturating_sub(file_name_indication.width()),
                        None,
                        Some(green_code),
                        true,
                    );
                    format!(
                        "\u{1b}[38;5;166;1m┌> \u{1b}[0m{}\n│  {}\u{1b}[1m└ {} {}",
                        file_name_line,
                        green_foreground,
                        file_name_indication,
                        line_in_file
                    )
                } else {
                    let file_name_indication = format!("{}", line_number);
                    let file_name_line = self.render_file_name_line(
                        path,
                        // indices,
                        &vec![],
                        max_width.saturating_sub(2),
                        None,
                        None,
                        true,
                    );
                    let line_in_file = self.render_file_contents_line(
                        line,
                        indices,
                        max_width.saturating_sub(3).saturating_sub(file_name_indication.width()),
                        None,
                        None,
                        true,
                    );
                    if is_below_search_result {
                        format!(
                            "│  {}\n│  \u{1b}[1m└ {} {}",
                            file_name_line,
                            file_name_indication,
                            line_in_file
                        )
                    } else {
                        format!(
                            "   {}\n   \u{1b}[1m└ {} {}",
                            file_name_line,
                            file_name_indication,
                            line_in_file
                        )
                    }
                }
            },
        }
    }
    fn render_file_name_line(
        &self,
        line_to_render: &String,
        indices: &Vec<usize>,
        max_width: usize,
        background_color: Option<usize>,
        foreground_color: Option<usize>,
        is_bold: bool,
    ) -> String {
        // TODO: merge these back, I guess?
        self.render_file_contents_line(line_to_render, indices, max_width, background_color, foreground_color, is_bold)
    }
    fn render_file_contents_line(
        &self,
        line_to_render: &String,
        indices: &Vec<usize>,
        max_width: usize,
        background_color: Option<usize>,
        foreground_color: Option<usize>,
        is_bold: bool,
    ) -> String {
        // TODO: get these from Zellij
        let reset_code = "\u{1b}[m";
        let underline_code = "\u{1b}[4m";
        let foreground_color = foreground_color
            .map(|c| format!("\u{1b}[38;5;{}m", c))
            .unwrap_or_else(|| format!(""));
        let background_color = background_color
            .map(|c| format!("\u{1b}[48;5;{}m", c))
            .unwrap_or_else(|| format!(""));
        let bold = if is_bold { "\u{1b}[1m" } else { "" };
        let non_index_character_style = format!("{}{}{}", background_color, foreground_color, bold);
        let index_character_style = format!(
            "{}{}{}{}",
            "\u{1b}[48;5;237m", foreground_color, bold, underline_code
        );
        let truncate_positions = self.truncate_file_contents_line(line_to_render, indices, max_width);

        let truncate_start_position = truncate_positions.map(|p| p.0).unwrap_or(0);
        let truncate_end_position = truncate_positions.map(|p| p.1).unwrap_or(line_to_render.chars().count());
        let mut visible_portion = String::new();
        for (i, character) in line_to_render.chars().enumerate() {
            if i >= truncate_start_position && i <= truncate_end_position {
                if indices.contains(&i) {
                    visible_portion.push_str(&index_character_style);
                    visible_portion.push(character);
                    visible_portion.push_str(reset_code);
                } else {
                    visible_portion.push_str(&non_index_character_style);
                    visible_portion.push(character);
                    visible_portion.push_str(reset_code);
                }
            }
        }
        if truncate_positions.is_some() {
            let left_truncate_sign = if truncate_start_position == 0 { "" } else { ".." };
            let right_truncate_sign = if truncate_end_position == line_to_render.chars().count() { "" } else { ".." };
            format!("{}{}{}{}{}", reset_code, left_truncate_sign, visible_portion, right_truncate_sign, reset_code)
        } else {
            visible_portion
        }
    }
    fn truncate_file_contents_line(&self, line_to_render: &String, indices: &Vec<usize>, max_width: usize) -> Option<(usize, usize)> {
        let max_truncated_width = max_width.saturating_sub(3); // TODO: calculate this from the
                                                               //
        let first_index = indices.get(0).copied().unwrap_or(0);
        // let last_index = indices.last().copied().unwrap_or_else(|| line_to_render.chars().count());
        let last_index = indices.last().copied().unwrap_or_else(|| std::cmp::min(line_to_render.chars().count(), max_truncated_width));
                                                               // outside
        if line_to_render.width() <= max_truncated_width {
            None
        } else if last_index.saturating_sub(first_index) < max_truncated_width {
            let mut width_remaining = max_truncated_width.saturating_sub(1).saturating_sub(last_index.saturating_sub(first_index));

            let mut string_start_position = first_index;
            let mut string_end_position = last_index;

            let mut i = 0;
            loop {
                if i >= width_remaining {
                    break;
                }
                if string_start_position > 0 && string_end_position < line_to_render.chars().count() {
                    let take_from_start = i % 2 == 0;
                    if take_from_start {
                        string_start_position -= 1;
                        if string_start_position == 0 {
                            width_remaining += 2; // no need for truncating dots
                        }
                    } else {
                        string_end_position += 1;
                        if string_end_position == line_to_render.chars().count() {
                            width_remaining += 2; // no need for truncating dots
                        }
                    }
                } else if string_end_position < line_to_render.chars().count() {
                    string_end_position += 1;
                    if string_end_position == line_to_render.chars().count() {
                        width_remaining += 2; // no need for truncating dots
                    }
                } else if string_start_position > 0 {
                    string_start_position -= 1;
                    if string_start_position == 0 {
                        width_remaining += 2; // no need for truncating dots
                    }
                } else {
                    break;
                }
                i += 1;
            }
//             for i in 0..width_remaining {
//                 if string_start_position > 0 && string_end_position < line_to_render.chars().count() {
//                     let take_from_start = i % 2 == 0;
//                     if take_from_start {
//                         string_start_position -= 1;
//                     } else {
//                         string_end_position += 1;
//                     }
//                 } else if string_end_position < line_to_render.chars().count() {
//                     string_end_position += 1;
//                 } else if string_start_position > 0 {
//                     string_start_position -= 1;
//                 } else {
//                     break;
//                 }
//            }
            Some((string_start_position, string_end_position))
            // format!("..{}..", line_to_render.chars().skip(*string_start_position).take(*string_end_position).collect())
        } else if !indices.is_empty() {
            let mut new_indices = indices.clone();
            drop(new_indices.pop());
            self.truncate_file_contents_line(line_to_render, &new_indices, max_width)
        } else {
            // not really sure how this happens...
            Some((first_index, last_index))
        }
        // if line_to_render.width() > max_width {
//         if indices.is_empty() {
//             // TODO:
//             // 1. if we don't have indices, do the below
//             // 2. if we have indices and the length between the first and last is equal to or lower
//             //    than max_width (with the weird saturating_sub thing) truncate beginning and end
//             //    with 2 dots (add one char back and one char forward until we reach the limit)
//             // 3. if we have indices and the length between the first and last is greater than
//             //    max_width (with calc), remove the last one until we are able to do 2
//             // 4. If we're not able to truncate, return None
//             let length_of_each_half = max_truncated_width / 2; // TODO: calculate max_width
//                                                                        // from outside properly
//             let truncate_start_position = length_of_each_half;
//             let truncate_end_position =
//                 line_to_render.width().saturating_sub(length_of_each_half);
//             Some((truncate_start_position, truncate_end_position))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResultsOfSearch {
    pub search_term: (String, usize),
    pub search_results: Vec<SearchResult>,
}

impl ResultsOfSearch {
    pub fn new(search_term: (String, usize), search_results: Vec<SearchResult>) -> Self {
        ResultsOfSearch {
            search_term,
            search_results,
        }
    }
    pub fn limit_search_results(mut self, max_results: usize) -> Self {
        self.search_results
            .sort_by(|a, b| b.score().cmp(&a.score()));
        self.search_results = if self.search_results.len() > max_results {
            self.search_results.drain(..max_results).collect()
        } else {
            self.search_results.drain(..).collect()
        };
        self
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FileNameSearchWorker {
    pub search_paths: BTreeSet<String>,
    skip_hidden_files: bool,
    processed_search_index: usize,
}

impl<'de> ZellijWorker<'de> for FileNameSearchWorker {
    // TODO: handle out of order messages, likely when rendering
    fn on_message(&mut self, message: String, payload: String) {
        match message.as_str() {
            // TODO: deserialize to type
            "scan_folder" => {
                self.populate_search_paths();
                post_message_to_plugin("done_scanning_folder".into(), "".into());
            },
            "search" => {
                if let Some((search_term, search_index)) = self.read_search_term_from_hd_cache() {
                    if search_index > self.processed_search_index {
                        self.search(search_term, search_index);
                        self.processed_search_index = search_index;
                    }
                }
            },
            // "filesystem_create" | "filesystem_read" | "filesystem_update" | "filesystem_delete" => {
            "filesystem_create" | "filesystem_update" | "filesystem_delete" => {
                match serde_json::from_str::<Vec<PathBuf>>(&payload) {
                    Ok(paths) => {
                        self.remove_existing_entries(&paths);
                        if message.as_str() != "filesystem_delete" {
                            for path in paths {
                                self.add_file_entry(&path, path.metadata().ok());
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to deserialize payload for message: {:?}: {:?}", message, e);
                    }
                }
            }
            "skip_hidden_files" => match serde_json::from_str::<bool>(&payload) {
                Ok(should_skip_hidden_files) => {
                    self.skip_hidden_files = should_skip_hidden_files;
                },
                Err(e) => {
                    eprintln!("Failed to deserialize payload: {:?}", e);
                },
            },
            _ => {},
        }
    }
}

impl FileNameSearchWorker {
    fn search(&mut self, search_term: String, search_index: usize) {
        let search_start = Instant::now();
        if self.search_paths.is_empty() {
            self.populate_search_paths();
        }
        eprintln!("populated search paths in: {:?}", search_start.elapsed());
        let matcher_constructor_start = Instant::now();
        let mut file_name_matches = vec![];
        // let mut file_contents_matches = vec![];
        let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(100); // TODO: no hard
        eprintln!("constructed matcher in: {:?}", matcher_constructor_start.elapsed());
                                                                                       // coded limit!
        let file_names_start = Instant::now();
        self.search_file_names(&search_term, &mut matcher, &mut file_name_matches);
        eprintln!("searched file names in: {:?}", file_names_start.elapsed());

        let file_name_search_results =
            ResultsOfSearch::new((search_term.clone(), search_index), file_name_matches).limit_search_results(100);
        post_message_to_plugin(
            "update_file_name_search_results".into(),
            serde_json::to_string(&file_name_search_results).unwrap(),
        );

        // if the search term changed before we finished, let's search again!
        if let Some((current_search_term, _current_search_index)) = self.read_search_term_from_hd_cache() {
            if current_search_term != search_term {
                eprintln!("\nRECURSING!\n");
                return self.search(current_search_term.into(), search_index);
            }
        }
    }
    fn read_search_term_from_hd_cache(&self) -> Option<(String, usize)> {
        if let Ok(current_search_term) = std::fs::read(CURRENT_SEARCH_TERM) {
            if let Ok(current_search_term) = serde_json::from_str::<(String, usize)>(&String::from_utf8_lossy(&current_search_term)) {
                return Some(current_search_term)
            }
        }
        None
    }
    fn populate_search_paths(&mut self) {
        for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
            self.add_file_entry(entry.path(), entry.metadata().ok());
        }
    }
    fn add_file_entry(&mut self, file_name: &Path, file_metadata: Option<std::fs::Metadata>) {
        if self.skip_hidden_files && file_name
                .to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        {
            return;
        }
        let file_path = file_name.display().to_string();
        let file_path_stripped_prefix = self.strip_file_prefix(&file_name);

        self.search_paths.insert(file_path_stripped_prefix);
    }
    fn strip_file_prefix(&self, file_name: &Path) -> String {
        let mut file_path_stripped_prefix = file_name.display().to_string().split_off(ROOT.width());
        if file_path_stripped_prefix.starts_with('/') {
            file_path_stripped_prefix.remove(0);
        }
        file_path_stripped_prefix
    }
    fn remove_existing_entries(&mut self, paths: &Vec<PathBuf>) {
        let file_path_stripped_prefixes: Vec<String> = paths.iter().map(|p| self.strip_file_prefix(&p)).collect();
        self.search_paths.retain(|file_name| !file_path_stripped_prefixes.contains(file_name));
    }
    fn search_file_names(
        &self,
        search_term: &str,
        matcher: &mut SkimMatcherV2,
        matches: &mut Vec<SearchResult>,
    ) {
        for entry in &self.search_paths {
            if let Some((score, indices)) = matcher.fuzzy_indices(&entry, &search_term) {
                matches.push(SearchResult::new_file_name(
                    score,
                    indices,
                    entry.to_owned(),
                ));
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FileContentsSearchWorker {
    pub search_file_contents: BTreeMap<(String, usize), String>, // file_name, line_number, line
    skip_hidden_files: bool,
    processed_search_index: usize,
}

impl<'de> ZellijWorker<'de> for FileContentsSearchWorker {
    // TODO: handle out of order messages, likely when rendering
    fn on_message(&mut self, message: String, payload: String) {
        match message.as_str() {
            // TODO: deserialize to type
            "scan_folder" => {
                self.populate_search_paths();
                post_message_to_plugin("done_scanning_folder".into(), "".into());
            },
            "search" => {
                if let Some((search_term, search_index)) = self.read_search_term_from_hd_cache() {
                    if search_index > self.processed_search_index {
                        self.search(search_term, search_index);
                        self.processed_search_index = search_index;
                    }
                }
            },
            "filesystem_create" | "filesystem_update" | "filesystem_delete" => {
                // TODO: CONTINUE HERE - remove the various eprintln's and log::infos and then try
                // a release build to see how it behaves
                // then let's use the rest of this week's time to think of a way to test this
                // (filesystem events) as well as the other API methods
                match serde_json::from_str::<Vec<PathBuf>>(&payload) {
                    Ok(paths) => {
                        self.remove_existing_entries(&paths);
                        if message.as_str() != "filesystem_delete" {
                            for path in paths {
                                self.add_file_entry(&path, path.metadata().ok());
                            }
                        }
                    },
                    Err(e) => {
                        eprintln!("Failed to deserialize payload for message: {:?}: {:?}", message, e);
                    }
                }
            }
            "skip_hidden_files" => match serde_json::from_str::<bool>(&payload) {
                Ok(should_skip_hidden_files) => {
                    self.skip_hidden_files = should_skip_hidden_files;
                },
                Err(e) => {
                    eprintln!("Failed to deserialize payload: {:?}", e);
                },
            },
            _ => {},
        }
    }
}

impl FileContentsSearchWorker {
    fn search(&mut self, search_term: String, search_index: usize) {
        let search_start = Instant::now();
        if self.search_file_contents.is_empty() {
            self.populate_search_paths();
        }
        eprintln!("populated search paths in: {:?}", search_start.elapsed());
        let matcher_constructor_start = Instant::now();
        // let mut file_name_matches = vec![];
        let mut file_contents_matches = vec![];
        let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(100); // TODO: no hard
        eprintln!("constructed matcher in: {:?}", matcher_constructor_start.elapsed());
                                                                                       // coded limit!
        let file_contents_start = Instant::now();
        self.search_file_contents(&search_term, &mut matcher, &mut file_contents_matches);
        eprintln!("searched file contents in: {:?}", file_contents_start.elapsed());

        let file_contents_search_results =
            ResultsOfSearch::new((search_term.clone(), search_index), file_contents_matches).limit_search_results(100);
        post_message_to_plugin(
            "update_file_contents_search_results".into(),
            serde_json::to_string(&file_contents_search_results).unwrap(),
        );

        // if the search term changed before we finished, let's search again!
        if let Some((current_search_term, _current_search_index)) = self.read_search_term_from_hd_cache() {
            if current_search_term != search_term {
                eprintln!("\nRECURSING!\n");
                return self.search(current_search_term.into(), search_index);
            }
        }
    }
    fn read_search_term_from_hd_cache(&self) -> Option<(String, usize)> {
        if let Ok(current_search_term) = std::fs::read(CURRENT_SEARCH_TERM) {
            if let Ok(current_search_term) = serde_json::from_str::<(String, usize)>(&String::from_utf8_lossy(&current_search_term)) {
                return Some(current_search_term)
            }
        }
        None
    }
    fn populate_search_paths(&mut self) {
        for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
            self.add_file_entry(entry.path(), entry.metadata().ok());
        }
    }
    fn remove_existing_entries(&mut self, paths: &Vec<PathBuf>) {
        // TODO: CONTINUE HERE (24/05) - implement this, then copy the functionality to the other worker,
        // then test it (I have a feeling we might be missing some stuff (<ROOT>/./...?) when root
        // stripping...)
        let file_path_stripped_prefixes: Vec<String> = paths.iter().map(|p| self.strip_file_prefix(&p)).collect();
        self.search_file_contents.retain(|(file_name, _line_in_file), _| !file_path_stripped_prefixes.contains(file_name));
    }
    fn add_file_entry(&mut self, file_name: &Path, file_metadata: Option<std::fs::Metadata>) {
        if self.skip_hidden_files && file_name
                .to_str()
                .map(|s| s.starts_with('.'))
                .unwrap_or(false)
        {
            return;
        }
        let file_path = file_name.display().to_string();
        let file_path_stripped_prefix = self.strip_file_prefix(&file_name);

        if file_metadata.map(|f| f.is_file()).unwrap_or(false) {
            if let Ok(file) = std::fs::File::open(&file_path) {
                let lines = io::BufReader::new(file).lines();
                for (index, line) in lines.enumerate() {
                    match line {
                        Ok(line) => {
                            self.search_file_contents.insert(
                                (
                                    file_path_stripped_prefix.clone(),
                                    index + 1,
                                ),
                                line,
                            );
                        },
                        Err(_) => {
                            break; // probably a binary file, skip it
                        },
                    }
                }
            }
        }

    }
    fn strip_file_prefix(&self, file_name: &Path) -> String {
        let mut file_path_stripped_prefix = file_name.display().to_string().split_off(ROOT.width());
        if file_path_stripped_prefix.starts_with('/') {
            file_path_stripped_prefix.remove(0);
        }
        file_path_stripped_prefix
    }
    fn search_file_contents(
        &self,
        search_term: &str,
        matcher: &mut SkimMatcherV2,
        matches: &mut Vec<SearchResult>,
    ) {
        for ((file_name, line_number), line_entry) in &self.search_file_contents {
            if let Some((score, indices)) = matcher.fuzzy_indices(&line_entry, &search_term) {
                matches.push(SearchResult::new_file_line(
                    score,
                    indices,
                    file_name.clone(),
                    line_entry.clone(),
                    *line_number,
                ));
            }
        }
    }
}

impl State {
    pub fn render_search(&mut self, rows: usize, cols: usize) {
        let mut to_render = String::new();
        if let Some(search_term) = self.search_term.as_ref() {
            to_render.push_str(&format!(
                "\u{1b}[38;5;51;1mSEARCH:\u{1b}[m {}\n",
                search_term
            ));
            let mut rows_left_to_render = rows.saturating_sub(3); // title and both controls lines
            let all_search_results = self.all_search_results();
            if self.selected_search_result >= rows_left_to_render {
                self.selected_search_result = rows_left_to_render.saturating_sub(1);
            }
            for (i, result) in all_search_results.iter().enumerate() {
                let result_height = result.rendered_height();
                if result_height > rows_left_to_render {
                    break;
                }
                rows_left_to_render -= result_height;
                let is_selected = i == self.selected_search_result;
                let is_below_search_result = i > self.selected_search_result;
                let rendered_result = result.render(cols, is_selected, is_below_search_result);
                to_render.push_str(&format!("{}", rendered_result));
                to_render.push('\n')
            }
            let orange_color = 166; // TODO: from Zellij theme
            if !all_search_results.is_empty() {
                if cols >= 60 {
                    to_render.push_str(
                        &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - open in editor. \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - open terminal at location.")
                    );
                } else if cols >= 38 {
                    to_render.push_str(
                        &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - edit. \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - open terminal.")
                    );
                } else if cols >= 21 {
                    to_render.push_str(
                        &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - e \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - t")
                    );
                }
            }
            to_render.push_str(&self.render_controls(rows, cols));

        }
        print!("{}", to_render);
    }
    pub fn all_search_results(&self) -> Vec<SearchResult> {
        match self.search_filter {
            SearchFilter::NamesAndContents => {
                let mut all_search_results = self.file_name_search_results.clone();
                all_search_results.append(&mut self.file_contents_search_results.clone());
                all_search_results.sort_by(|a, b| {
                    b.score().cmp(&a.score())
                });
                all_search_results
            }
            SearchFilter::Names => {
                self.file_name_search_results.clone()
            }
            SearchFilter::Contents => {
                self.file_contents_search_results.clone()
            }
        }
    }
    fn render_controls(&self, rows: usize, columns: usize) -> String {
        let keycode_color = 238;
        let ribbon_color = 245;
        let ribbon_style = format!("\u{1b}[48;5;{};38;5;16;1m", ribbon_color);
        let black_foreground = format!("\u{1b}[38;5;16m");
        let white_foreground = format!("\u{1b}[38;5;15m");
        let red_foreground = format!("\u{1b}[38;5;124m");
        let red = 124;
        let bold = format!("\u{1b}[1m");
        let keycode_style = format!("\u{1b}[48;5;{};1m{}", keycode_color, white_foreground);
        let arrow = |foreground: usize, background: usize| format!("\u{1b}[38;5;{}m\u{1b}[48;5;{}m", foreground, background);
        let dot = move |is_active: bool| {
            let color = if is_active { red_foreground.clone() } else { black_foreground.clone() };
            format!("{}•", color)
        };

        #[derive(Default)]
        struct Control {
            key: &'static str,
            options: Vec<&'static str>,
            option_index: (usize, usize), // eg. 1 out of 2 (1, 2)
        }

        impl Control {
            pub fn new(key: &'static str, options: Vec<&'static str>, option_index: (usize, usize)) -> Self {
                Control { key, options, option_index }
            }
            pub fn short_len(&self) -> usize {
                let short_text = self.options.get(2).or_else(|| self.options.get(1)).or_else(|| self.options.get(0)).unwrap_or(&"");
                short_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
            }
            pub fn mid_len(&self) -> usize {
                let mid_text = self.options.get(1).or_else(|| self.options.get(0)).unwrap_or(&"");
                mid_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
            }
            pub fn full_len(&self) -> usize {
                let full_text = self.options.get(0).unwrap_or(&"");
                full_text.chars().count() + self.key.chars().count() + self.option_index.1 + 7 // 7 for all the spaces and decorations
            }
            pub fn render_short_length(&self) -> String {
                let short_text = self.options.get(2).or_else(|| self.options.get(1)).or_else(|| self.options.get(0)).unwrap_or(&"");
                self.render(short_text)
            }
            pub fn render_mid_length(&self) -> String {
                let mid_text = self.options.get(1).or_else(|| self.options.get(0)).unwrap_or(&"");
                self.render(mid_text)
            }
            pub fn render_full_length(&self) -> String {
                let full_text = self.options.get(0).unwrap_or(&"");
                self.render(full_text)
            }
            fn render(&self, text: &str) -> String {
                let keycode_color = 238;
                let ribbon_color = 245;
                let ribbon_style = format!("\u{1b}[48;5;{};38;5;16;1m", ribbon_color);
                let black_foreground = format!("\u{1b}[38;5;16m");
                let white_foreground = format!("\u{1b}[38;5;15m");
                let red_foreground = format!("\u{1b}[38;5;124m");
                let red = 124;
                let bold = format!("\u{1b}[1m");
                let keycode_style = format!("\u{1b}[48;5;{};1m{}", keycode_color, white_foreground);
                let arrow = |foreground: usize, background: usize| format!("\u{1b}[38;5;{}m\u{1b}[48;5;{}m", foreground, background);
                let dot = move |is_active: bool| {
                    let color = if is_active { red_foreground.clone() } else { black_foreground.clone() };
                    format!("{}•", color)
                };
                let mut selection_dots = String::new();
                for i in 1..=self.option_index.1 {
                    if i == self.option_index.0 {
                        selection_dots.push_str(&dot(true));
                    } else {
                        selection_dots.push_str(&dot(false));
                    }
                }
                format!(
                    "{} {} {}{} {} {}{} {}{}\u{1b}[0K",
                    keycode_style,
                    self.key,
                    arrow(keycode_color, ribbon_color),
                    ribbon_style,
                    selection_dots,
                    ribbon_style,
                    text,
                    arrow(ribbon_color, keycode_color),
                    keycode_style,
                )
            }
        }

        #[derive(Default)]
        struct ControlsLine {
            controls: Vec<Control>,
            scanning_indication: Option<Vec<&'static str>>,
            animation_offset: u8,
        }

        impl ControlsLine {
            pub fn new(controls: Vec<Control>, scanning_indication: Option<Vec<&'static str>>) -> Self {
                ControlsLine {
                    controls,
                    scanning_indication,
                    ..Default::default()
                }
            }
            pub fn with_animation_offset(mut self, animation_offset: u8) -> Self {
                self.animation_offset = animation_offset;
                self
            }
            pub fn render(&self, max_width: usize) -> String {
                struct LoadingAnimation {
                    scanning_indication: Option<Vec<&'static str>>,
                    animation_offset: u8,

                }
                impl LoadingAnimation {
                    pub fn new(scanning_indication: &Option<Vec<&'static str>>, animation_offset: u8) -> Self {
                        LoadingAnimation {
                            scanning_indication: scanning_indication.clone(),
                            animation_offset
                        }
                    }
                    pub fn full_len(&self) -> usize {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(0))
                            .map(|s| s.chars().count() + 3) // 3 for animation dots
                            .unwrap_or(0)
                    }
                    pub fn mid_len(&self) -> usize {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(1)
                                .or_else(|| scanning_indication.get(0))
                            )
                            .map(|s| s.chars().count() + 3) // 3 for animation dots
                            .unwrap_or(0)
                    }
                    pub fn short_len(&self) -> usize {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(2)
                                .or_else(|| scanning_indication.get(1))
                                .or_else(|| scanning_indication.get(0))
                            )
                            .map(|s| s.chars().count() + 3) // 3 for animation dots
                            .unwrap_or(0)
                    }
                    pub fn render_full_length(&self) -> String {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(0))
                            .map(|s| s.to_string() + &self.animation_dots())
                            .unwrap_or_else(String::new)
                    }
                    pub fn render_mid_length(&self) -> String {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(1)
                                .or_else(|| scanning_indication.get(0))
                            )
                            .map(|s| s.to_string() + &self.animation_dots())
                            .unwrap_or_else(String::new)
                    }
                    pub fn render_short_length(&self) -> String {
                        self.scanning_indication.as_ref()
                            .and_then(|scanning_indication| scanning_indication.get(2)
                                .or_else(|| scanning_indication.get(1))
                                .or_else(|| scanning_indication.get(0))
                            )
                            .map(|s| s.to_string() + &self.animation_dots())
                            .unwrap_or_else(String::new)
                    }
                    fn animation_dots(&self) -> String {
                        let mut to_render = String::from("");
                        let dot_count = self.animation_offset % 4;
                        for _ in 0..dot_count {
                            to_render.push('.');
                        }
                        to_render
                    }
                }

                let loading_animation = LoadingAnimation::new(&self.scanning_indication, self.animation_offset);
                let full_length = loading_animation.full_len() + self.controls.iter().map(|c| c.full_len()).sum::<usize>();
                let mid_length = loading_animation.mid_len() + self.controls.iter().map(|c| c.mid_len()).sum::<usize>();
                let short_length = loading_animation.short_len() + self.controls.iter().map(|c| c.short_len()).sum::<usize>();
                if max_width >= full_length {
                    let mut to_render = String::new();
                    for control in &self.controls {
                        to_render.push_str(&control.render_full_length());
                    }
                    to_render.push_str(&self.render_padding(max_width.saturating_sub(full_length)));
                    to_render.push_str(&loading_animation.render_full_length());
                    to_render
                } else if max_width >= mid_length {
                    let mut to_render = String::new();
                    for control in &self.controls {
                        to_render.push_str(&control.render_mid_length());
                    }
                    to_render.push_str(&self.render_padding(max_width.saturating_sub(mid_length)));
                    to_render.push_str(&loading_animation.render_mid_length());
                    to_render
                } else if max_width >= short_length {
                    let mut to_render = String::new();
                    for control in &self.controls {
                        to_render.push_str(&control.render_short_length());
                    }
                    to_render.push_str(&self.render_padding(max_width.saturating_sub(short_length)));
                    to_render.push_str(&loading_animation.render_short_length());
                    to_render
                } else {
                    format!("")
                }
            }
            fn render_padding(&self, padding: usize) -> String {
                format!("\u{1b}[{}C", padding)
            }
        }
        let tiled_floating_control = if self.should_open_floating {
            Control::new(
                "Ctrl f",
                vec!["OPEN FLOATING", "FLOATING", "F"],
                (2, 2)
            )
        } else {
            Control::new(
                "Ctrl f",
                vec!["OPEN TILED", "TILED", "T"],
                (1, 2)
            )
        };
        let names_contents_control = match &self.search_filter {
            SearchFilter::NamesAndContents => {
                Control::new(
                    "Ctrl r",
                    vec!["FILE NAMES AND CONTENTS", "NAMES + CONTENTS", "N+C"],
                    (1, 3)
                )
            }
            SearchFilter::Names => {
                Control::new(
                    "Ctrl r",
                    vec!["FILE NAMES", "NAMES", "N"],
                    (2, 3)
                )
            }
            SearchFilter::Contents => {
                Control::new(
                    "Ctrl r",
                    vec!["FILE CONTENTS", "CONTENTS", "C"],
                    (3, 3)
                )
            }
        };
        let rendered = if self.loading {
            ControlsLine::new(vec![tiled_floating_control, names_contents_control], Some(vec!["Scanning folder", "Scanning", "S"]))
                .with_animation_offset(self.loading_animation_offset)
                .render(columns)
        } else {
            ControlsLine::new(vec![tiled_floating_control, names_contents_control], None)
                .render(columns)
        };
        format!("\u{1b}[{rows};0H{}", rendered)
        // let tiled_floating_control = if self.should_open_floating {.. }
        // ControlsLine(vec![
        //     tiled_floating_control,
        //     names_and_contents_control
        // ])
        //     vec!["OPEN TILED", "TILED", "T"],
        //     vec!["OPEN TILED", "TILED", "T"],
        // * Control(vec![
        //     vec!["OPEN TILED", "TILED", "T"],
        //     vec!["OPEN FLOATING", "FLOATING", "F"],
        //  ]);
        // * Control(vec![
        //     vec!["FILE NAMES AND CONTENTS", "NAMES + CONTENTS", "N+C"],
        //     vec!["FILE NAMES", "NAMES", "N"],
        //     vec!["FILE CONTENTS", "CONTENTS", "C"],
        //  ]);
        // * ControlsLine::new(Vec<Control>)
    }
    pub fn toggle_search_filter(&mut self) {
        self.search_filter.progress()
    }

}

pub enum SearchFilter {
    NamesAndContents,
    Names,
    Contents,
}

impl SearchFilter {
    pub fn progress(&mut self) {
        match &self {
            &SearchFilter::NamesAndContents => *self = SearchFilter::Names,
            &SearchFilter::Names => *self = SearchFilter::Contents,
            &SearchFilter::Contents => *self = SearchFilter::NamesAndContents,
        }
    }
}

impl Default for SearchFilter {
    fn default() -> Self {
        SearchFilter::NamesAndContents
    }
}
