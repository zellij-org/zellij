use crate::state::{State, ROOT, CURRENT_SEARCH_TERM};

use zellij_tile::prelude::*;
use unicode_width::UnicodeWidthStr;

use walkdir::WalkDir;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use serde::{Serialize, Deserialize};

use std::io::{ self, BufRead };

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
    }
}

impl SearchResult {
    pub fn new_file_name(score: i64, indices: Vec<usize>, path: String) -> Self {
        SearchResult::File {
            path,
            score,
            indices
        }
    }
    pub fn new_file_line(score: i64, indices: Vec<usize>, path: String, line: String, line_number: usize) -> Self {
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
    pub fn render(&self, max_width: usize, is_selected: bool) -> String {
        let green_code = 154;
        let orange_code = 166;
        let bold_code = "\u{1b}[1m";
        let green_foreground = format!("\u{1b}[38;5;{}m", green_code);
        let orange_foreground = format!("\u{1b}[38;5;{}m", orange_code);
        let reset_code = "\u{1b}[m";
        let max_width = max_width.saturating_sub(3); // for the UI left line separator
        match self {
            SearchResult::File { path, indices, .. } =>  {
                if is_selected {
                    let line = self.render_line_with_indices(path, indices, max_width, None, Some(green_code), true);
                    format!("{} | {}{}", green_foreground, reset_code, line)
                } else {
                    let line = self.render_line_with_indices(path, indices, max_width, None, None, true);
                    format!(" | {}", line)
                }
            }
            SearchResult::LineInFile { path, line, line_number, indices, .. } => {
                if is_selected {
                    let first_line = self.render_line_with_indices(path, &vec![], max_width, None, Some(green_code), true);
                    let line_indication_text = format!("{}-> {}", bold_code, line_number);
                    let line_indication = format!("{}{}{}", orange_foreground, line_indication_text, reset_code); // TODO: also truncate
                    let second_line = self.render_line_with_indices(line, indices, max_width.saturating_sub(line_indication_text.width()), None, Some(orange_code), false);
                    format!(" {}│{} {}\n {}│{} {} {}", green_foreground, reset_code, first_line, green_foreground, reset_code, line_indication, second_line)
                } else {
                    let first_line = self.render_line_with_indices(path, &vec![], max_width, None, None, true); // TODO:
                    let line_indication_text = format!("{}-> {}", bold_code, line_number);
                    let second_line = self.render_line_with_indices(line, indices, max_width.saturating_sub(line_indication_text.width()), None, None, false);
                    format!(" │ {}\n │ {} {}", first_line, line_indication_text, second_line)
                }
            }
        }
    }
    fn render_line_with_indices(&self, line_to_render: &String, indices: &Vec<usize>, max_width: usize, background_color: Option<usize>, foreground_color: Option<usize>, is_bold: bool) -> String {
        // TODO: get these from Zellij
        let reset_code = "\u{1b}[m";
        let underline_code = "\u{1b}[4m";
        let foreground_color = foreground_color.map(|c| format!("\u{1b}[38;5;{}m", c)).unwrap_or_else(|| format!(""));
        let background_color = background_color.map(|c| format!("\u{1b}[48;5;{}m", c)).unwrap_or_else(|| format!(""));
        let bold = if is_bold { "\u{1b}[1m" } else { "" };
        let non_index_character_style = format!("{}{}{}", background_color, foreground_color, bold);
        let index_character_style = format!("{}{}{}{}", background_color, foreground_color, bold, underline_code);

        let mut truncate_start_position = None;
        let mut truncate_end_position = None;
        if line_to_render.width() > max_width {
            let length_of_each_half = max_width.saturating_sub(4) / 2;
            truncate_start_position = Some(length_of_each_half);
            truncate_end_position = Some(line_to_render.width().saturating_sub(length_of_each_half));
        }
        let mut first_half = format!("{}", reset_code);
        let mut second_half = format!("{}", reset_code);
        for (i, character) in line_to_render.chars().enumerate() {
            if (truncate_start_position.is_none() && truncate_end_position.is_none()) || Some(i) < truncate_start_position {
                if indices.contains(&i) {
                    first_half.push_str(&index_character_style);
                    first_half.push(character);
                    first_half.push_str(reset_code);
                } else {
                    first_half.push_str(&non_index_character_style);
                    first_half.push(character);
                    first_half.push_str(reset_code);
                }
            } else if Some(i) > truncate_end_position {
                if indices.contains(&i) {
                    second_half.push_str(&index_character_style);
                    second_half.push(character);
                    second_half.push_str(reset_code);
                } else {
                    second_half.push_str(&non_index_character_style);
                    second_half.push(character);
                    second_half.push_str(reset_code);
                }
            }
        }
        if let Some(_truncate_start_position) = truncate_start_position {
            format!("{}{}{}[..]{}{}{}", first_half, reset_code, foreground_color, reset_code, second_half, reset_code)
        } else {
            format!("{}{}", first_half, reset_code)
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResultsOfSearch {
    pub search_term: String,
    pub search_results: Vec<SearchResult>,
}

impl ResultsOfSearch {
    pub fn new(search_term: String, search_results: Vec<SearchResult>) -> Self {
        ResultsOfSearch {
            search_term,
            search_results,
        }
    }
    pub fn limit_search_results(mut self, max_results: usize) -> Self {
        self.search_results.sort_by(|a, b| b.score().cmp(&a.score()));
        self.search_results = if self.search_results.len() > max_results {
            self.search_results.drain(..max_results).collect()
        } else {
            self.search_results.drain(..).collect()
        };
        self
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct SearchWorker {
    pub search_paths: Vec<String>,
    pub search_file_contents: Vec<(String, usize, String)>, // file_name, line_number, line
    skip_hidden_files: bool,
}

impl SearchWorker {
    pub fn new() -> Self {
        SearchWorker {
            search_paths: vec![],
            search_file_contents: vec![],
            skip_hidden_files: true,
        }
    }
    pub fn on_message(&mut self, message: String, payload: String) {
        match message.as_str() { // TODO: deserialize to type
            "scan_folder" => {
                if let Err(e) = std::fs::remove_file("/data/search_data") {
                    eprintln!("Warning: failed to remove cache file: {:?}", e);
                }
                self.populate_search_paths();
                post_message_to_plugin("done_scanning_folder".into(), "".into());
            }
            "search" => {
                let search_term = payload;
                let (search_term, matches) = self.search(search_term);
                let search_results = ResultsOfSearch::new(search_term, matches).limit_search_results(100);
                post_message_to_plugin("update_search_results".into(), serde_json::to_string(&search_results).unwrap());
            }
            "skip_hidden_files" => {
                match serde_json::from_str::<bool>(&payload) {
                    Ok(should_skip_hidden_files) => {
                        self.skip_hidden_files = should_skip_hidden_files;
                    },
                    Err(e) => {
                        eprintln!("Failed to deserialize payload: {:?}", e);
                    }
                }
            }
            _ => {}
        }
    }
    fn search(&mut self, search_term: String) -> (String, Vec<SearchResult>) {
        if self.search_paths.is_empty() {
            self.populate_search_paths();
        }
        let mut matches = vec![];
        let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(100); // TODO: no hard
                                                                                   // coded limit!
        self.search_file_names(&search_term, &mut matcher, &mut matches);
        self.search_file_contents(&search_term, &mut matcher, &mut matches);

        // if the search term changed before we finished, let's search again!
        if let Ok(current_search_term) = std::fs::read(CURRENT_SEARCH_TERM) {
            let current_search_term = String::from_utf8_lossy(&current_search_term); // TODO: not lossy, search can be lots of stuff
            if current_search_term != search_term {
                return self.search(current_search_term.into());
            }
        }
        (search_term, matches)
    }
    fn populate_search_paths(&mut self) {
        // TODO: CONTINUE HERE - when we start, check to see if /data/search_data exists, if it is
        // deserialize it and place it in our own state, if not, do the below and then write to it
        if let Ok(search_data) = std::fs::read("/data/search_data") { // TODO: add cwd to here
            if let Ok(mut existing_state) = serde_json::from_str::<Self>(&String::from_utf8_lossy(&search_data)) {
                std::mem::swap(self, &mut existing_state);
                return;
            }
        }
        for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
            if self.skip_hidden_files && entry.file_name().to_str().map(|s| s.starts_with('.')).unwrap_or(false) {
                continue;
            }
            let file_path = entry.path().display().to_string();

            if entry.metadata().unwrap().is_file() {
                if let Ok(file) = std::fs::File::open(&file_path) {
                    let lines = io::BufReader::new(file).lines();
                    for (index, line) in lines.enumerate() {
                        match line {
                            Ok(line) => {
                                self.search_file_contents.push((file_path.clone(), index + 1, line));
                            },
                            Err(_) => {
                                break; // probably a binary file, skip it
                            }
                        }
                    }
                }
            }

            self.search_paths.push(file_path);
        }
        let serialized_state = serde_json::to_string(&self).unwrap(); // TODO: unwrap city
        std::fs::write("/data/search_data", serialized_state.as_bytes()).unwrap();
        if let Ok(search_data) = std::fs::read("/data/search_data") {
            if let Ok(mut existing_state) = serde_json::from_str::<Self>(&String::from_utf8_lossy(&search_data)) {
                std::mem::swap(self, &mut existing_state);
                return;
            }
        }
    }
    fn search_file_names(&self, search_term: &str, matcher: &mut SkimMatcherV2, matches: &mut Vec<SearchResult>) {
        for entry in &self.search_paths {
            if let Some((score, indices)) = matcher.fuzzy_indices(&entry, &search_term) {
                matches.push(SearchResult::new_file_name(score, indices, entry.to_owned()));
            }
        }
    }
    fn search_file_contents(&self, search_term: &str, matcher: &mut SkimMatcherV2, matches: &mut Vec<SearchResult>) {
        for (file_name, line_number, line_entry) in &self.search_file_contents {
            if let Some((score, indices)) = matcher.fuzzy_indices(&line_entry, &search_term) {
                matches.push(SearchResult::new_file_line(score, indices, file_name.clone(), line_entry.clone(), *line_number));
            }
        }
    }
}

impl State {
    pub fn render_search(&mut self, rows: usize, cols: usize) {
        if let Some(search_term) = self.search_term.as_ref() {
            let mut to_render = String::new();
            to_render.push_str(&format!(" \u{1b}[38;5;51;1mSEARCH:\u{1b}[m {}\n", search_term));
            let mut rows_left_to_render = rows.saturating_sub(3);
            if self.loading && self.search_results.is_empty() {
                to_render.push_str(&self.render_loading());
            }
            for (i, result) in self.search_results.iter().enumerate().take(rows.saturating_sub(3)) {
                let result_height = result.rendered_height();
                if result_height + 1 > rows_left_to_render {
                    break;
                }
                rows_left_to_render -= result_height;
                rows_left_to_render -= 1; // space between
                let is_selected = i == self.selected_search_result;
                let rendered_result = result.render(cols, is_selected);
                to_render.push_str(&format!("\n{}\n", rendered_result));
            }
            print!("{}", to_render);
        }
    }
    pub fn render_loading(&self) -> String {
        let mut rendered = String::from("Scanning folder");
        let dot_count = self.loading_animation_offset % 4;
        for _ in 0..dot_count {
            rendered.push('.');
        }
        rendered
    }
}
