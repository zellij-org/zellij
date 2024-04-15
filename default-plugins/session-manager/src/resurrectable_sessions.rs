use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use humantime::format_duration;

use std::time::Duration;

use zellij_tile::shim::*;

#[derive(Debug, Default)]
pub struct ResurrectableSessions {
    pub all_resurrectable_sessions: Vec<(String, Duration)>,
    pub selected_index: Option<usize>,
    pub selected_search_index: Option<usize>,
    pub search_results: Vec<SearchResult>,
    pub is_searching: bool,
    pub search_term: String,
    pub delete_all_dead_sessions_warning: bool,
}

impl ResurrectableSessions {
    pub fn update(&mut self, mut list: Vec<(String, Duration)>) {
        list.sort_by(|a, b| a.1.cmp(&b.1));
        self.all_resurrectable_sessions = list;
        if self.is_searching {
            self.update_search_term();
        }
    }
    pub fn render(&self, rows: usize, columns: usize, x: usize, y: usize) {
        if self.delete_all_dead_sessions_warning {
            self.render_delete_all_sessions_warning(rows, columns, x, y);
            return;
        }
        let search_indication =
            Text::new(format!("Search: {}_", self.search_term)).color_range(2, ..7);
        let table_rows = rows.saturating_sub(5); // search row, toggle row and some padding
        let table_columns = columns;
        let table = if self.is_searching {
            self.render_search_results(table_rows, columns)
        } else {
            self.render_all_entries(table_rows, columns)
        };
        print_text_with_coordinates(search_indication, x.saturating_sub(1), y + 2, None, None);
        print_table_with_coordinates(table, x, y + 3, Some(table_columns), Some(table_rows));
    }
    fn render_search_results(&self, table_rows: usize, _table_columns: usize) -> Table {
        let mut table = Table::new().add_row(vec![" ", " ", " "]); // skip the title row
        let (first_row_index_to_render, last_row_index_to_render) = self.range_to_render(
            table_rows,
            self.search_results.len(),
            self.selected_search_index,
        );
        for i in first_row_index_to_render..last_row_index_to_render {
            if let Some(search_result) = self.search_results.get(i) {
                let is_selected = Some(i) == self.selected_search_index;
                let mut table_cells = vec![
                    self.render_session_name(
                        &search_result.session_name,
                        Some(search_result.indices.clone()),
                    ),
                    self.render_ctime(&search_result.ctime),
                    self.render_more_indication_or_enter_as_needed(
                        i,
                        first_row_index_to_render,
                        last_row_index_to_render,
                        self.search_results.len(),
                        is_selected,
                    ),
                ];
                if is_selected {
                    table_cells = table_cells.drain(..).map(|t| t.selected()).collect();
                }
                table = table.add_styled_row(table_cells);
            }
        }
        table
    }
    fn render_all_entries(&self, table_rows: usize, _table_columns: usize) -> Table {
        let mut table = Table::new().add_row(vec![" ", " ", " "]); // skip the title row
        let (first_row_index_to_render, last_row_index_to_render) = self.range_to_render(
            table_rows,
            self.all_resurrectable_sessions.len(),
            self.selected_index,
        );
        for i in first_row_index_to_render..last_row_index_to_render {
            if let Some(session) = self.all_resurrectable_sessions.get(i) {
                let is_selected = Some(i) == self.selected_index;
                let mut table_cells = vec![
                    self.render_session_name(&session.0, None),
                    self.render_ctime(&session.1),
                    self.render_more_indication_or_enter_as_needed(
                        i,
                        first_row_index_to_render,
                        last_row_index_to_render,
                        self.all_resurrectable_sessions.len(),
                        is_selected,
                    ),
                ];
                if is_selected {
                    table_cells = table_cells.drain(..).map(|t| t.selected()).collect();
                }
                table = table.add_styled_row(table_cells);
            }
        }
        table
    }
    fn render_delete_all_sessions_warning(&self, rows: usize, columns: usize, x: usize, y: usize) {
        if rows == 0 || columns == 0 {
            return;
        }
        let session_count = self.all_resurrectable_sessions.len();
        let session_count_len = session_count.to_string().chars().count();
        let warning_description_text =
            format!("This will delete {} resurrectable sessions", session_count,);
        let confirmation_text = "Are you sure? (y/n)";
        let warning_y_location = y + (rows / 2).saturating_sub(1);
        let confirmation_y_location = y + (rows / 2) + 1;
        let warning_x_location =
            x + columns.saturating_sub(warning_description_text.chars().count()) / 2;
        let confirmation_x_location =
            x + columns.saturating_sub(confirmation_text.chars().count()) / 2;
        print_text_with_coordinates(
            Text::new(warning_description_text).color_range(0, 17..18 + session_count_len),
            warning_x_location,
            warning_y_location,
            None,
            None,
        );
        print_text_with_coordinates(
            Text::new(confirmation_text).color_indices(2, vec![15, 17]),
            confirmation_x_location,
            confirmation_y_location,
            None,
            None,
        );
    }
    fn range_to_render(
        &self,
        table_rows: usize,
        results_len: usize,
        selected_index: Option<usize>,
    ) -> (usize, usize) {
        if table_rows <= results_len {
            let row_count_to_render = table_rows.saturating_sub(1); // 1 for the title
            let first_row_index_to_render = selected_index
                .unwrap_or(0)
                .saturating_sub(row_count_to_render / 2);
            let last_row_index_to_render = first_row_index_to_render + row_count_to_render;
            (first_row_index_to_render, last_row_index_to_render)
        } else {
            let first_row_index_to_render = 0;
            let last_row_index_to_render = results_len;
            (first_row_index_to_render, last_row_index_to_render)
        }
    }
    fn render_session_name(&self, session_name: &str, indices: Option<Vec<usize>>) -> Text {
        let text = Text::new(&session_name).color_range(0, ..);
        match indices {
            Some(indices) => text.color_indices(1, indices),
            None => text,
        }
    }
    fn render_ctime(&self, ctime: &Duration) -> Text {
        let duration = format_duration(ctime.clone()).to_string();
        let duration_parts = duration.split_whitespace();
        let mut formatted_duration = String::new();
        for part in duration_parts {
            if !part.ends_with('s') {
                if !formatted_duration.is_empty() {
                    formatted_duration.push(' ');
                }
                formatted_duration.push_str(part);
            }
        }
        if formatted_duration.is_empty() {
            formatted_duration.push_str("<1m");
        }
        let duration_len = formatted_duration.chars().count();
        Text::new(format!("Created {} ago", formatted_duration)).color_range(2, 8..9 + duration_len)
    }
    fn render_more_indication_or_enter_as_needed(
        &self,
        i: usize,
        first_row_index_to_render: usize,
        last_row_index_to_render: usize,
        results_len: usize,
        is_selected: bool,
    ) -> Text {
        if is_selected {
            Text::new(format!("<ENTER> - Resurrect Session")).color_range(3, 0..7)
        } else if i == first_row_index_to_render && i > 0 {
            Text::new(format!("+ {} more", first_row_index_to_render)).color_range(1, ..)
        } else if i == last_row_index_to_render.saturating_sub(1)
            && last_row_index_to_render < results_len
        {
            Text::new(format!(
                "+ {} more",
                results_len.saturating_sub(last_row_index_to_render)
            ))
            .color_range(1, ..)
        } else {
            Text::new(" ")
        }
    }
    pub fn move_selection_down(&mut self) {
        if self.is_searching {
            if let Some(selected_index) = self.selected_search_index.as_mut() {
                if *selected_index == self.search_results.len().saturating_sub(1) {
                    *selected_index = 0;
                } else {
                    *selected_index = *selected_index + 1;
                }
            } else {
                self.selected_search_index = Some(0);
            }
        } else {
            if let Some(selected_index) = self.selected_index.as_mut() {
                if *selected_index == self.all_resurrectable_sessions.len().saturating_sub(1) {
                    *selected_index = 0;
                } else {
                    *selected_index = *selected_index + 1;
                }
            } else {
                self.selected_index = Some(0);
            }
        }
    }
    pub fn move_selection_up(&mut self) {
        if self.is_searching {
            if let Some(selected_index) = self.selected_search_index.as_mut() {
                if *selected_index == 0 {
                    *selected_index = self.search_results.len().saturating_sub(1);
                } else {
                    *selected_index = selected_index.saturating_sub(1);
                }
            } else {
                self.selected_search_index = Some(self.search_results.len().saturating_sub(1));
            }
        } else {
            if let Some(selected_index) = self.selected_index.as_mut() {
                if *selected_index == 0 {
                    *selected_index = self.all_resurrectable_sessions.len().saturating_sub(1);
                } else {
                    *selected_index = selected_index.saturating_sub(1);
                }
            } else {
                self.selected_index = Some(self.all_resurrectable_sessions.len().saturating_sub(1));
            }
        }
    }
    pub fn get_selected_session_name(&self) -> Option<String> {
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .map(|search_result| search_result.session_name.clone())
        } else {
            self.selected_index
                .and_then(|i| self.all_resurrectable_sessions.get(i))
                .map(|session_name_and_creation_time| session_name_and_creation_time.0.clone())
        }
    }
    pub fn delete_selected_session(&mut self) {
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .map(|search_result| delete_dead_session(&search_result.session_name));
        } else {
            self.selected_index
                .and_then(|i| {
                    if self.all_resurrectable_sessions.len() > i {
                        // optimistic update
                        if i == 0 {
                            self.selected_index = None;
                        } else if i == self.all_resurrectable_sessions.len().saturating_sub(1) {
                            self.selected_index = Some(i.saturating_sub(1));
                        }
                        Some(self.all_resurrectable_sessions.remove(i))
                    } else {
                        None
                    }
                })
                .map(|session_name_and_creation_time| {
                    delete_dead_session(&session_name_and_creation_time.0)
                });
        }
    }
    fn delete_all_sessions(&mut self) {
        // optimistic update
        self.all_resurrectable_sessions = vec![];
        self.delete_all_dead_sessions_warning = false;
        delete_all_dead_sessions();
    }
    pub fn show_delete_all_sessions_warning(&mut self) {
        self.delete_all_dead_sessions_warning = true;
    }
    pub fn handle_character(&mut self, character: char) {
        if self.delete_all_dead_sessions_warning && character == 'y' {
            self.delete_all_sessions();
        } else if self.delete_all_dead_sessions_warning && character == 'n' {
            self.delete_all_dead_sessions_warning = false;
        } else {
            self.search_term.push(character);
            self.update_search_term();
        }
    }
    pub fn handle_backspace(&mut self) {
        self.search_term.pop();
        self.update_search_term();
    }
    pub fn has_session(&self, session_name: &str) -> bool {
        self.all_resurrectable_sessions
            .iter()
            .any(|s| s.0 == session_name)
    }
    fn update_search_term(&mut self) {
        let mut matches = vec![];
        let matcher = SkimMatcherV2::default().use_cache(true);
        for (session_name, ctime) in &self.all_resurrectable_sessions {
            if let Some((score, indices)) = matcher.fuzzy_indices(&session_name, &self.search_term)
            {
                matches.push(SearchResult {
                    session_name: session_name.to_owned(),
                    ctime: ctime.clone(),
                    score,
                    indices,
                });
            }
        }
        matches.sort_by(|a, b| b.score.cmp(&a.score));
        self.search_results = matches;
        self.is_searching = !self.search_term.is_empty();
        match self.selected_search_index {
            Some(search_index) => {
                if self.search_results.is_empty() {
                    self.selected_search_index = None;
                } else if search_index >= self.search_results.len() {
                    self.selected_search_index = Some(self.search_results.len().saturating_sub(1));
                }
            },
            None => {
                self.selected_search_index = Some(0);
            },
        }
    }
}

#[derive(Debug)]
pub struct SearchResult {
    score: i64,
    indices: Vec<usize>,
    session_name: String,
    ctime: Duration,
}
