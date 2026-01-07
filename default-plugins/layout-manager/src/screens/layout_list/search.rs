use zellij_tile::prelude::*;
use crate::DisplayLayout;
use crate::text_input::TextInput;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

#[derive(Clone)]
pub struct SearchResult {
    pub layout: DisplayLayout,
    pub original_index: usize,
    pub score: i64,
    pub matched_indices: Vec<usize>,
}

#[derive(Clone)]
pub struct SearchState {
    filter_active: bool,
    filter_input: TextInput,
    typing_filter: bool,
    search_results: Vec<SearchResult>,
    selected_search_index: usize,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            filter_active: false,
            filter_input: TextInput::empty(),
            typing_filter: false,
            search_results: Vec::new(),
            selected_search_index: 0,
        }
    }
}

impl SearchState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_active(&self) -> bool {
        self.filter_active
    }

    pub fn is_typing(&self) -> bool {
        self.typing_filter
    }

    pub fn start_typing(&mut self) {
        self.typing_filter = true;
    }

    pub fn stop_typing(&mut self) {
        self.typing_filter = false;
    }

    pub fn get_filter_input(&self) -> &TextInput {
        &self.filter_input
    }

    pub fn get_filter_input_mut(&mut self) -> &mut TextInput {
        &mut self.filter_input
    }

    pub fn get_search_results(&self) -> &[SearchResult] {
        &self.search_results
    }

    pub fn get_selected_search_index(&self) -> usize {
        self.selected_search_index
    }

    pub fn set_selected_search_index(&mut self, index: usize) {
        self.selected_search_index = index;
    }

    pub fn update_filter(&mut self, display_layouts: &[DisplayLayout], base_x: usize, base_y: usize) {
        let filter_prompt = "Filter:";
        let filter_text = self.filter_input.get_text();

        // Clear results if filter is empty
        if filter_text.is_empty() && !self.typing_filter {
            self.search_results.clear();
            self.selected_search_index = 0;
            return;
        }

        // Perform fuzzy matching
        let matcher = SkimMatcherV2::default();

        let mut results: Vec<SearchResult> = display_layouts
            .iter()
            .enumerate()
            .filter_map(|(index, layout)| {
                let name = layout.name();
                matcher.fuzzy_indices(&name, filter_text).map(|(score, indices)| {
                    SearchResult {
                        layout: layout.clone(),
                        original_index: index,
                        score,
                        matched_indices: indices,
                    }
                })
            })
            .collect();

        // Sort by score descending (best matches first)
        results.sort_by(|a, b| b.score.cmp(&a.score));
        self.search_results = results;

        if self.search_results.is_empty() {
            self.filter_active = false;
        } else {
            self.filter_active = true;
        }

        // Keep selection in bounds
        if self.selected_search_index >= self.search_results.len() {
            self.selected_search_index = self.search_results.len().saturating_sub(1);
        }

        // Show cursor when typing
        if self.typing_filter {
            let cursor_pos = self.filter_input.get_cursor_position();
            show_cursor(Some((
                filter_prompt.chars().count() + 1 + cursor_pos + base_x,
                base_y
            )));
        } else {
            show_cursor(None);
        }
    }

    pub fn clear_filter(&mut self) {
        self.filter_active = false;
        self.typing_filter = false;
        self.filter_input.clear();
        self.search_results.clear();
        self.selected_search_index = 0;
        show_cursor(None);
    }

    pub fn get_current_selected_original_index(&self) -> Option<usize> {
        self.search_results.get(self.selected_search_index)
            .map(|result| result.original_index)
    }

    pub fn render_filter_line(&self, base_x: usize, base_y: usize) {
        let filter_text_str = self.filter_input.get_text();
        let filter_text = if self.typing_filter {
            let mut filter_line = Text::new(format!("Filter: {}", filter_text_str)).color_substring(2, "Filter:");
            if !filter_text_str.is_empty() {
                filter_line = filter_line.color_last_substring(3, filter_text_str)
            }
            filter_line
        } else {
            Text::new(format!("Filter: {} (<Esc> - clear)", filter_text_str))
                .color_substring(3, "<Esc>")
                .color_substring(2, "Filter:")
        };
        print_text_with_coordinates(filter_text, base_x, base_y, None, None);
    }

    pub fn get_matched_indices_for_visible(&self, hidden_above: usize, visible_count: usize) -> Vec<Option<Vec<usize>>> {
        if !self.filter_active || self.search_results.is_empty() {
            return vec![None; visible_count];
        }

        self.search_results
            .iter()
            .skip(hidden_above)
            .take(visible_count)
            .map(|result| Some(result.matched_indices.clone()))
            .collect()
    }
}
