use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::cmp::Ordering;
use zellij_tile::prelude::*;

#[derive(Default)]
pub struct NewSessionInfo {
    name: String,
    layout_list: LayoutList,
    entering_new_session_info: EnteringState,
}

#[derive(Eq, PartialEq)]
enum EnteringState {
    EnteringName,
    EnteringLayoutSearch,
}

impl Default for EnteringState {
    fn default() -> Self {
        EnteringState::EnteringName
    }
}

impl NewSessionInfo {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn layout_search_term(&self) -> &str {
        &self.layout_list.layout_search_term
    }
    pub fn entering_new_session_info(&self) -> bool {
        true
    }
    pub fn entering_new_session_name(&self) -> bool {
        self.entering_new_session_info == EnteringState::EnteringName
    }
    pub fn entering_layout_search_term(&self) -> bool {
        self.entering_new_session_info == EnteringState::EnteringLayoutSearch
    }
    pub fn add_char(&mut self, character: char) {
        match self.entering_new_session_info {
            EnteringState::EnteringName => {
                self.name.push(character);
            },
            EnteringState::EnteringLayoutSearch => {
                self.layout_list.layout_search_term.push(character);
                self.update_layout_search_term();
            },
        }
    }
    pub fn handle_backspace(&mut self) {
        match self.entering_new_session_info {
            EnteringState::EnteringName => {
                self.name.pop();
            },
            EnteringState::EnteringLayoutSearch => {
                self.layout_list.layout_search_term.pop();
                self.update_layout_search_term();
            },
        }
    }
    pub fn handle_break(&mut self) {
        match self.entering_new_session_info {
            EnteringState::EnteringName => {
                self.name.clear();
            },
            EnteringState::EnteringLayoutSearch => {
                self.layout_list.layout_search_term.clear();
                self.entering_new_session_info = EnteringState::EnteringName;
                self.update_layout_search_term();
            },
        }
    }
    pub fn handle_key(&mut self, key: Key) {
        match key {
            Key::Backspace => {
                self.handle_backspace();
            },
            Key::Ctrl('c') | Key::Esc => {
                self.handle_break();
            },
            Key::Char(character) => {
                self.add_char(character);
            },
            Key::Up => {
                self.move_selection_up();
            },
            Key::Down => {
                self.move_selection_down();
            },
            _ => {},
        }
    }
    pub fn handle_selection(&mut self, current_session_name: &Option<String>) {
        match self.entering_new_session_info {
            EnteringState::EnteringLayoutSearch => {
                let new_session_layout: Option<LayoutInfo> = self.selected_layout_info();
                let new_session_name = if self.name.is_empty() {
                    None
                } else {
                    Some(self.name.as_str())
                };
                if new_session_name != current_session_name.as_ref().map(|s| s.as_str()) {
                    match new_session_layout {
                        Some(new_session_layout) => {
                            switch_session_with_layout(new_session_name, new_session_layout, None)
                        },
                        None => {
                            switch_session(new_session_name);
                        },
                    }
                }
                self.name.clear();
                self.layout_list.clear_selection();
                hide_self();
            },
            EnteringState::EnteringName => {
                self.entering_new_session_info = EnteringState::EnteringLayoutSearch;
            },
        }
    }
    pub fn update_layout_list(&mut self, layout_info: Vec<LayoutInfo>) {
        self.layout_list.update_layout_list(layout_info);
    }
    pub fn layout_list(&self) -> Vec<(LayoutInfo, bool)> {
        // bool - is_selected
        self.layout_list
            .layout_list
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i == self.layout_list.selected_layout_index))
            .collect()
    }
    pub fn layouts_to_render(&self) -> Vec<(LayoutInfo, Vec<usize>, bool)> {
        // (layout_info,
        // search_indices,
        // is_selected)
        if self.is_searching() {
            self.layout_search_results()
                .into_iter()
                .map(|(layout_search_result, is_selected)| {
                    (
                        layout_search_result.layout_info,
                        layout_search_result.indices,
                        is_selected,
                    )
                })
                .collect()
        } else {
            self.layout_list()
                .into_iter()
                .map(|(layout_info, is_selected)| (layout_info, vec![], is_selected))
                .collect()
        }
    }
    pub fn layout_search_results(&self) -> Vec<(LayoutSearchResult, bool)> {
        // bool - is_selected
        self.layout_list
            .layout_search_results
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i == self.layout_list.selected_layout_index))
            .collect()
    }
    pub fn is_searching(&self) -> bool {
        !self.layout_list.layout_search_term.is_empty()
    }
    pub fn layout_count(&self) -> usize {
        self.layout_list.layout_list.len()
    }
    pub fn selected_layout_info(&self) -> Option<LayoutInfo> {
        self.layout_list.selected_layout_info()
    }
    fn update_layout_search_term(&mut self) {
        if self.layout_list.layout_search_term.is_empty() {
            self.layout_list.clear_selection();
            self.layout_list.layout_search_results = vec![];
        } else {
            let mut matches = vec![];
            let matcher = SkimMatcherV2::default().use_cache(true);
            for layout_info in &self.layout_list.layout_list {
                if let Some((score, indices)) =
                    matcher.fuzzy_indices(&layout_info.name(), &self.layout_list.layout_search_term)
                {
                    matches.push(LayoutSearchResult {
                        layout_info: layout_info.clone(),
                        score,
                        indices,
                    });
                }
            }
            matches.sort_by(|a, b| b.score.cmp(&a.score));
            self.layout_list.layout_search_results = matches;
            self.layout_list.clear_selection();
        }
    }
    fn move_selection_up(&mut self) {
        self.layout_list.move_selection_up();
    }
    fn move_selection_down(&mut self) {
        self.layout_list.move_selection_down();
    }
}

#[derive(Default)]
pub struct LayoutList {
    layout_list: Vec<LayoutInfo>,
    layout_search_results: Vec<LayoutSearchResult>,
    selected_layout_index: usize,
    layout_search_term: String,
}

impl LayoutList {
    pub fn update_layout_list(&mut self, layout_list: Vec<LayoutInfo>) {
        let old_layout_length = self.layout_list.len();
        self.layout_list = layout_list;
        if old_layout_length != self.layout_list.len() {
            // honestly, this is just the UX choice that sucks the least...
            self.clear_selection();
        }
    }
    pub fn selected_layout_info(&self) -> Option<LayoutInfo> {
        if !self.layout_search_term.is_empty() {
            self.layout_search_results
                .get(self.selected_layout_index)
                .map(|l| l.layout_info.clone())
        } else {
            self.layout_list.get(self.selected_layout_index).cloned()
        }
    }
    pub fn clear_selection(&mut self) {
        self.selected_layout_index = 0;
    }
    fn max_index(&self) -> usize {
        if self.layout_search_term.is_empty() {
            self.layout_list.len().saturating_sub(1)
        } else {
            self.layout_search_results.len().saturating_sub(1)
        }
    }
    fn move_selection_up(&mut self) {
        let max_index = self.max_index();
        if self.selected_layout_index > 0 {
            self.selected_layout_index -= 1;
        } else {
            self.selected_layout_index = max_index;
        }
    }
    fn move_selection_down(&mut self) {
        let max_index = self.max_index();
        if self.selected_layout_index < max_index {
            self.selected_layout_index += 1;
        } else {
            self.selected_layout_index = 0;
        }
    }
}

#[derive(Clone)]
pub struct LayoutSearchResult {
    pub layout_info: LayoutInfo,
    pub score: i64,
    pub indices: Vec<usize>,
}
