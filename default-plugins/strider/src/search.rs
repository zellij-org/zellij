use crate::state::{State, CURRENT_SEARCH_TERM, ROOT};
use std::path::{PathBuf, Path};
use std::collections::{BTreeMap, BTreeSet};
use crate::MessageToPlugin;

use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;
use crate::search_results::SearchResult;

use std::io::{self, BufRead};

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

// #[derive(Default, Serialize, Deserialize)]
// pub struct FileNameSearchWorker {
//     pub search_paths: BTreeSet<String>,
//     skip_hidden_files: bool,
//     processed_search_index: usize,
// }
// 
// impl<'de> ZellijWorker<'de> for FileNameSearchWorker {
//     // TODO: handle out of order messages, likely when rendering
//     fn on_message(&mut self, message: String, payload: String) {
//         match message.as_str() {
//             // TODO: deserialize to type
//             "scan_folder" => {
//                 self.populate_search_paths();
//                 post_message_to_plugin("done_scanning_folder".into(), "".into());
//             },
//             "search" => {
//                 if let Some((search_term, search_index)) = self.read_search_term_from_hd_cache() {
//                     if search_index > self.processed_search_index {
//                         self.search(search_term, search_index);
//                         self.processed_search_index = search_index;
//                     }
//                 }
//             },
//             // "filesystem_create" | "filesystem_read" | "filesystem_update" | "filesystem_delete" => {
//             "filesystem_create" | "filesystem_update" | "filesystem_delete" => {
//                 match serde_json::from_str::<Vec<PathBuf>>(&payload) {
//                     Ok(paths) => {
//                         self.remove_existing_entries(&paths);
//                         if message.as_str() != "filesystem_delete" {
//                             for path in paths {
//                                 self.add_file_entry(&path, path.metadata().ok());
//                             }
//                         }
//                     },
//                     Err(e) => {
//                         eprintln!("Failed to deserialize payload for message: {:?}: {:?}", message, e);
//                     }
//                 }
//             }
//             "skip_hidden_files" => match serde_json::from_str::<bool>(&payload) {
//                 Ok(should_skip_hidden_files) => {
//                     self.skip_hidden_files = should_skip_hidden_files;
//                 },
//                 Err(e) => {
//                     eprintln!("Failed to deserialize payload: {:?}", e);
//                 },
//             },
//             _ => {},
//         }
//     }
// }
// 
// impl FileNameSearchWorker {
//     fn search(&mut self, search_term: String, search_index: usize) {
//         let search_start = Instant::now();
//         if self.search_paths.is_empty() {
//             self.populate_search_paths();
//         }
//         eprintln!("populated search paths in: {:?}", search_start.elapsed());
//         let matcher_constructor_start = Instant::now();
//         let mut file_name_matches = vec![];
//         // let mut file_contents_matches = vec![];
//         let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(100); // TODO: no hard
//         eprintln!("constructed matcher in: {:?}", matcher_constructor_start.elapsed());
//                                                                                        // coded limit!
//         let file_names_start = Instant::now();
//         self.search_file_names(&search_term, &mut matcher, &mut file_name_matches);
//         eprintln!("searched file names in: {:?}", file_names_start.elapsed());
// 
//         let file_name_search_results =
//             ResultsOfSearch::new((search_term.clone(), search_index), file_name_matches).limit_search_results(100);
//         post_message_to_plugin(
//             "update_file_name_search_results".into(),
//             serde_json::to_string(&file_name_search_results).unwrap(),
//         );
// 
//         // if the search term changed before we finished, let's search again!
//         if let Some((current_search_term, _current_search_index)) = self.read_search_term_from_hd_cache() {
//             if current_search_term != search_term {
//                 eprintln!("\nRECURSING!\n");
//                 return self.search(current_search_term.into(), search_index);
//             }
//         }
//     }
//     fn read_search_term_from_hd_cache(&self) -> Option<(String, usize)> {
//         if let Ok(current_search_term) = std::fs::read(CURRENT_SEARCH_TERM) {
//             if let Ok(current_search_term) = serde_json::from_str::<(String, usize)>(&String::from_utf8_lossy(&current_search_term)) {
//                 return Some(current_search_term)
//             }
//         }
//         None
//     }
//     fn populate_search_paths(&mut self) {
//         for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
//             self.add_file_entry(entry.path(), entry.metadata().ok());
//         }
//     }
//     fn add_file_entry(&mut self, file_name: &Path, file_metadata: Option<std::fs::Metadata>) {
//         if self.skip_hidden_files && file_name
//                 .to_str()
//                 .map(|s| s.starts_with('.'))
//                 .unwrap_or(false)
//         {
//             return;
//         }
//         let file_path = file_name.display().to_string();
//         let file_path_stripped_prefix = self.strip_file_prefix(&file_name);
// 
//         self.search_paths.insert(file_path_stripped_prefix);
//     }
//     fn strip_file_prefix(&self, file_name: &Path) -> String {
//         let mut file_path_stripped_prefix = file_name.display().to_string().split_off(ROOT.width());
//         if file_path_stripped_prefix.starts_with('/') {
//             file_path_stripped_prefix.remove(0);
//         }
//         file_path_stripped_prefix
//     }
//     fn remove_existing_entries(&mut self, paths: &Vec<PathBuf>) {
//         let file_path_stripped_prefixes: Vec<String> = paths.iter().map(|p| self.strip_file_prefix(&p)).collect();
//         self.search_paths.retain(|file_name| !file_path_stripped_prefixes.contains(file_name));
//     }
//     fn search_file_names(
//         &self,
//         search_term: &str,
//         matcher: &mut SkimMatcherV2,
//         matches: &mut Vec<SearchResult>,
//     ) {
//         for entry in &self.search_paths {
//             if let Some((score, indices)) = matcher.fuzzy_indices(&entry, &search_term) {
//                 matches.push(SearchResult::new_file_name(
//                     score,
//                     indices,
//                     entry.to_owned(),
//                 ));
//             }
//         }
//     }
// }
// 
// #[derive(Default, Serialize, Deserialize)]
// pub struct FileContentsSearchWorker {
//     pub search_file_contents: BTreeMap<(String, usize), String>, // file_name, line_number, line
//     skip_hidden_files: bool,
//     processed_search_index: usize,
// }
// 
// impl<'de> ZellijWorker<'de> for FileContentsSearchWorker {
//     // TODO: handle out of order messages, likely when rendering
//     fn on_message(&mut self, message: String, payload: String) {
//         match message.as_str() {
//             // TODO: deserialize to type
//             "scan_folder" => {
//                 self.populate_search_paths();
//                 post_message_to_plugin("done_scanning_folder".into(), "".into());
//             },
//             "search" => {
//                 if let Some((search_term, search_index)) = self.read_search_term_from_hd_cache() {
//                     if search_index > self.processed_search_index {
//                         self.search(search_term, search_index);
//                         self.processed_search_index = search_index;
//                     }
//                 }
//             },
//             "filesystem_create" | "filesystem_update" | "filesystem_delete" => {
//                 // TODO: CONTINUE HERE - remove the various eprintln's and log::infos and then try
//                 // a release build to see how it behaves
//                 // then let's use the rest of this week's time to think of a way to test this
//                 // (filesystem events) as well as the other API methods
//                 match serde_json::from_str::<Vec<PathBuf>>(&payload) {
//                     Ok(paths) => {
//                         self.remove_existing_entries(&paths);
//                         if message.as_str() != "filesystem_delete" {
//                             for path in paths {
//                                 self.add_file_entry(&path, path.metadata().ok());
//                             }
//                         }
//                     },
//                     Err(e) => {
//                         eprintln!("Failed to deserialize payload for message: {:?}: {:?}", message, e);
//                     }
//                 }
//             }
//             "skip_hidden_files" => match serde_json::from_str::<bool>(&payload) {
//                 Ok(should_skip_hidden_files) => {
//                     self.skip_hidden_files = should_skip_hidden_files;
//                 },
//                 Err(e) => {
//                     eprintln!("Failed to deserialize payload: {:?}", e);
//                 },
//             },
//             _ => {},
//         }
//     }
// }
// 
// impl FileContentsSearchWorker {
//     fn search(&mut self, search_term: String, search_index: usize) {
//         let search_start = Instant::now();
//         if self.search_file_contents.is_empty() {
//             self.populate_search_paths();
//         }
//         eprintln!("populated search paths in: {:?}", search_start.elapsed());
//         let matcher_constructor_start = Instant::now();
//         // let mut file_name_matches = vec![];
//         let mut file_contents_matches = vec![];
//         let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(100); // TODO: no hard
//         eprintln!("constructed matcher in: {:?}", matcher_constructor_start.elapsed());
//                                                                                        // coded limit!
//         let file_contents_start = Instant::now();
//         self.search_file_contents(&search_term, &mut matcher, &mut file_contents_matches);
//         eprintln!("searched file contents in: {:?}", file_contents_start.elapsed());
// 
//         let file_contents_search_results =
//             ResultsOfSearch::new((search_term.clone(), search_index), file_contents_matches).limit_search_results(100);
//         post_message_to_plugin(
//             "update_file_contents_search_results".into(),
//             serde_json::to_string(&file_contents_search_results).unwrap(),
//         );
// 
//         // if the search term changed before we finished, let's search again!
//         if let Some((current_search_term, _current_search_index)) = self.read_search_term_from_hd_cache() {
//             if current_search_term != search_term {
//                 eprintln!("\nRECURSING!\n");
//                 return self.search(current_search_term.into(), search_index);
//             }
//         }
//     }
//     fn read_search_term_from_hd_cache(&self) -> Option<(String, usize)> {
//         if let Ok(current_search_term) = std::fs::read(CURRENT_SEARCH_TERM) {
//             if let Ok(current_search_term) = serde_json::from_str::<(String, usize)>(&String::from_utf8_lossy(&current_search_term)) {
//                 return Some(current_search_term)
//             }
//         }
//         None
//     }
//     fn populate_search_paths(&mut self) {
//         for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
//             self.add_file_entry(entry.path(), entry.metadata().ok());
//         }
//     }
//     fn remove_existing_entries(&mut self, paths: &Vec<PathBuf>) {
//         // TODO: CONTINUE HERE (24/05) - implement this, then copy the functionality to the other worker,
//         // then test it (I have a feeling we might be missing some stuff (<ROOT>/./...?) when root
//         // stripping...)
//         let file_path_stripped_prefixes: Vec<String> = paths.iter().map(|p| self.strip_file_prefix(&p)).collect();
//         self.search_file_contents.retain(|(file_name, _line_in_file), _| !file_path_stripped_prefixes.contains(file_name));
//     }
//     fn add_file_entry(&mut self, file_name: &Path, file_metadata: Option<std::fs::Metadata>) {
//         if self.skip_hidden_files && file_name
//                 .to_str()
//                 .map(|s| s.starts_with('.'))
//                 .unwrap_or(false)
//         {
//             return;
//         }
//         let file_path = file_name.display().to_string();
//         let file_path_stripped_prefix = self.strip_file_prefix(&file_name);
// 
//         if file_metadata.map(|f| f.is_file()).unwrap_or(false) {
//             if let Ok(file) = std::fs::File::open(&file_path) {
//                 let lines = io::BufReader::new(file).lines();
//                 for (index, line) in lines.enumerate() {
//                     match line {
//                         Ok(line) => {
//                             self.search_file_contents.insert(
//                                 (
//                                     file_path_stripped_prefix.clone(),
//                                     index + 1,
//                                 ),
//                                 line,
//                             );
//                         },
//                         Err(_) => {
//                             break; // probably a binary file, skip it
//                         },
//                     }
//                 }
//             }
//         }
// 
//     }
//     fn strip_file_prefix(&self, file_name: &Path) -> String {
//         let mut file_path_stripped_prefix = file_name.display().to_string().split_off(ROOT.width());
//         if file_path_stripped_prefix.starts_with('/') {
//             file_path_stripped_prefix.remove(0);
//         }
//         file_path_stripped_prefix
//     }
//     fn search_file_contents(
//         &self,
//         search_term: &str,
//         matcher: &mut SkimMatcherV2,
//         matches: &mut Vec<SearchResult>,
//     ) {
//         for ((file_name, line_number), line_entry) in &self.search_file_contents {
//             if let Some((score, indices)) = matcher.fuzzy_indices(&line_entry, &search_term) {
//                 matches.push(SearchResult::new_file_line(
//                     score,
//                     indices,
//                     file_name.clone(),
//                     line_entry.clone(),
//                     *line_number,
//                 ));
//             }
//         }
//     }
// }

impl State {
//     pub fn render_search(&mut self, rows: usize, cols: usize) {
//         let mut to_render = String::new();
//         if let Some(search_term) = self.search_term.as_ref() {
//             to_render.push_str(&format!(
//                 "\u{1b}[38;5;51;1mSEARCH:\u{1b}[m {}\n",
//                 search_term
//             ));
//             let mut rows_left_to_render = rows.saturating_sub(3); // title and both controls lines
//             let all_search_results = self.all_search_results();
//             if self.selected_search_result >= rows_left_to_render {
//                 self.selected_search_result = rows_left_to_render.saturating_sub(1);
//             }
//             for (i, result) in all_search_results.iter().enumerate() {
//                 let result_height = result.rendered_height();
//                 if result_height > rows_left_to_render {
//                     break;
//                 }
//                 rows_left_to_render -= result_height;
//                 let is_selected = i == self.selected_search_result;
//                 let is_below_search_result = i > self.selected_search_result;
//                 let rendered_result = result.render(cols, is_selected, is_below_search_result);
//                 to_render.push_str(&format!("{}", rendered_result));
//                 to_render.push('\n')
//             }
//             let orange_color = 166; // TODO: from Zellij theme
//             if !all_search_results.is_empty() {
//                 if cols >= 60 {
//                     to_render.push_str(
//                         &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - open in editor. \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - open terminal at location.")
//                     );
//                 } else if cols >= 38 {
//                     to_render.push_str(
//                         &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - edit. \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - open terminal.")
//                     );
//                 } else if cols >= 21 {
//                     to_render.push_str(
//                         &format!("└ \u{1b}[38;5;{orange_color};1m<ENTER>\u{1b}[0;1m - e \u{1b}[38;5;{orange_color};1m<TAB>\u{1b}[0;1m - t")
//                     );
//                 }
//             }
//             to_render.push_str(&self.render_controls(rows, cols));
// 
//         }
//         print!("{}", to_render);
//     }
    pub fn all_search_results(&self) -> Vec<SearchResult> {
        match self.search_filter {
            SearchType::NamesAndContents => {
                let mut all_search_results = self.file_name_search_results.clone();
                all_search_results.append(&mut self.file_contents_search_results.clone());
                all_search_results.sort_by(|a, b| {
                    b.score().cmp(&a.score())
                });
                all_search_results
            }
            SearchType::Names => {
                self.file_name_search_results.clone()
            }
            SearchType::Contents => {
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
            SearchType::NamesAndContents => {
                Control::new(
                    "Ctrl r",
                    vec!["FILE NAMES AND CONTENTS", "NAMES + CONTENTS", "N+C"],
                    (1, 3)
                )
            }
            SearchType::Names => {
                Control::new(
                    "Ctrl r",
                    vec!["FILE NAMES", "NAMES", "N"],
                    (2, 3)
                )
            }
            SearchType::Contents => {
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
    }
    pub fn toggle_search_filter(&mut self) {
        self.search_filter.progress()
    }

}

#[derive(Serialize, Deserialize)]
pub enum SearchType {
    NamesAndContents,
    Names,
    Contents,
}

impl SearchType {
    pub fn progress(&mut self) {
        match &self {
            &SearchType::NamesAndContents => *self = SearchType::Names,
            &SearchType::Names => *self = SearchType::Contents,
            &SearchType::Contents => *self = SearchType::NamesAndContents,
        }
    }
}

impl Default for SearchType {
    fn default() -> Self {
        SearchType::NamesAndContents
    }
}

// AFTER REFACTOR THE BELOW SHOULD BE THIS FILE

#[derive(Default, Serialize, Deserialize)]
pub struct Search {
    search_type: SearchType,
    file_names: BTreeSet<String>,
    file_contents: BTreeMap<(String, usize), String>, // file_name, line_number, line

}

impl Search {
    pub fn new(search_type: SearchType) -> Self {
        Search {
            search_type,
            ..Default::default()
        }
    }
    fn on_message(&mut self, message: String, payload: String) {
        match serde_json::from_str::<MessageToSearch>(&message) {
            Ok(MessageToSearch::ScanFolder) => {
                self.scan_hd();
            }
            Ok(MessageToSearch::Search) => {
                if let Some(current_search_term) = self.read_search_term_from_hd_cache() {
                    self.search(current_search_term);
                }
            }
            Ok(MessageToSearch::FileSystemCreate) => {
                self.rescan_files(payload);
            }
            Ok(MessageToSearch::FileSystemUpdate) => {
                self.rescan_files(payload);
            }
            Ok(MessageToSearch::FileSystemDelete) => {
                self.delete_files(payload);
            },
            Err(e) => eprintln!("Failed to deserialize worker message {:?}", e),
        }
    }
    pub fn scan_hd(&mut self) {
        for entry in WalkDir::new(ROOT).into_iter().filter_map(|e| e.ok()) {
            self.add_file_entry(entry.path(), entry.metadata().ok());
        }
    }
    pub fn search(&self, search_term: String) {
        let search_results_limit = 100; // artificial limit to prevent probably unwanted chaos
        let mut matcher = SkimMatcherV2::default().use_cache(true).element_limit(search_results_limit);
        let mut file_names_search_results = None;
        let mut file_contents_search_results = None;
        if let SearchType::Names | SearchType::NamesAndContents = self.search_type {
            let file_names_matches = self.search_file_names(&search_term, &mut matcher);
            file_names_search_results = Some(
                ResultsOfSearch::new(search_term.clone(), file_names_matches).limit_search_results(search_results_limit)
            );
        };
        if let SearchType::Contents | SearchType::NamesAndContents = self.search_type {
            let file_contents_matches = self.search_file_contents(&search_term, &mut matcher);
            file_contents_search_results = Some(
                ResultsOfSearch::new(search_term.clone(), file_contents_matches).limit_search_results(search_results_limit)
            );
        };

        // if the search term changed before we finished, let's search again!
        if let Some(current_search_term) = self.read_search_term_from_hd_cache() {
            if current_search_term != search_term {
                return self.search(current_search_term.into());
            }
        }
        if let Some(file_names_search_results) = file_names_search_results {
            post_message_to_plugin(
                serde_json::to_string(&MessageToPlugin::UpdateFileNameSearchResults).unwrap(),
                serde_json::to_string(&file_names_search_results).unwrap(),
            );
        }
        if let Some(file_contents_search_results) = file_contents_search_results{
            post_message_to_plugin(
                serde_json::to_string(&MessageToPlugin::UpdateFileContentsSearchResults).unwrap(),
                serde_json::to_string(&file_contents_search_results).unwrap(),
            );
        }
    }
    pub fn rescan_files(&mut self, paths: String) {
        match serde_json::from_str::<Vec<PathBuf>>(&paths) {
            Ok(paths) => {
                for path in paths {
                    self.add_file_entry(&path, path.metadata().ok());
                }
            },
            Err(e) => eprintln!("Failed to deserialize paths: {:?}", e)
        }
    }
    pub fn delete_files(&mut self, paths: String) {
        match serde_json::from_str::<Vec<PathBuf>>(&paths) {
            Ok(paths) => self.remove_existing_entries(&paths),
            Err(e) => eprintln!("Failed to deserialize paths: {:?}", e)
        }
    }
    fn add_file_entry(&mut self, file_name: &Path, file_metadata: Option<std::fs::Metadata>) {
        let file_path = file_name.display().to_string();
        let file_path_stripped_prefix = self.strip_file_prefix(&file_name);

        self.file_names.insert(file_path_stripped_prefix.clone());
        if let SearchType::NamesAndContents | SearchType::Contents = self.search_type {
            if file_metadata.map(|f| f.is_file()).unwrap_or(false) {
                if let Ok(file) = std::fs::File::open(&file_path) {
                    let lines = io::BufReader::new(file).lines();
                    for (index, line) in lines.enumerate() {
                        match line {
                            Ok(line) => {
                                self.file_contents.insert(
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
    }
    fn search_file_names(
        &self,
        search_term: &str,
        matcher: &mut SkimMatcherV2,
    ) -> Vec<SearchResult> {
        let mut matches = vec![];
        for entry in &self.file_names {
            if let Some((score, indices)) = matcher.fuzzy_indices(&entry, &search_term) {
                matches.push(SearchResult::new_file_name(
                    score,
                    indices,
                    entry.to_owned(),
                ));
            }
        }
        matches
    }
    fn search_file_contents(
        &self,
        search_term: &str,
        matcher: &mut SkimMatcherV2,
    ) -> Vec<SearchResult> {
        let mut matches = vec![];
        for ((file_name, line_number), line_entry) in &self.file_contents {
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
        matches
    }
    fn strip_file_prefix(&self, file_name: &Path) -> String {
        let mut file_path_stripped_prefix = file_name.display().to_string().split_off(ROOT.width());
        if file_path_stripped_prefix.starts_with('/') {
            file_path_stripped_prefix.remove(0);
        }
        file_path_stripped_prefix
    }
    fn read_search_term_from_hd_cache(&self) -> Option<String> {
        match std::fs::read(CURRENT_SEARCH_TERM) {
            Ok(current_search_term) => Some(String::from_utf8_lossy(&current_search_term).to_string()),
            _ => None,
        }
    }
    fn remove_existing_entries(&mut self, paths: &Vec<PathBuf>) {
        let file_path_stripped_prefixes: Vec<String> = paths.iter().map(|p| self.strip_file_prefix(&p)).collect();
        self.file_names.retain(|file_name| !file_path_stripped_prefixes.contains(file_name));
        self.file_contents.retain(|(file_name, _line_in_file), _| !file_path_stripped_prefixes.contains(file_name));
    }
}

#[derive(Serialize, Deserialize)]
pub enum MessageToSearch {
    ScanFolder,
    Search,
    FileSystemCreate,
    FileSystemUpdate,
    FileSystemDelete,
}

#[derive(Serialize, Deserialize)]
pub struct FileNameWorker { // TODO: naming and replace the other implementation
    search: Search,
}

impl Default for FileNameWorker {
    fn default() -> Self {
        FileNameWorker {
            search: Search::new(SearchType::Names)
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct FileContentsWorker { // TODO: naming and replace the other implementation
    search: Search,
}

impl Default for FileContentsWorker {
    fn default() -> Self {
        FileContentsWorker {
            search: Search::new(SearchType::Contents)
        }
    }
}

impl<'de> ZellijWorker<'de> for FileNameWorker{
    // TODO: handle out of order messages, likely when rendering
    fn on_message(&mut self, message: String, payload: String) {
        self.search.on_message(message, payload);
    }
}

impl<'de> ZellijWorker<'de> for FileContentsWorker {
    // TODO: handle out of order messages, likely when rendering
    fn on_message(&mut self, message: String, payload: String) {
        self.search.on_message(message, payload);
    }
}
