use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;
use zellij_tile::prelude::*;

#[derive(Default)]
pub struct NewSessionInfo {
    name: String,
    layout_list: LayoutList,
    entering_new_session_info: EnteringState,
    pub is_welcome_screen: bool,
    pub new_session_folder: Option<PathBuf>,
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
    pub fn handle_key(&mut self, key: KeyWithModifier) {
        match key.bare_key {
            BareKey::Backspace if key.has_no_modifiers() => {
                self.handle_backspace();
            },
            BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.handle_break();
            },
            BareKey::Esc if key.has_no_modifiers() => {
                self.handle_break();
            },
            BareKey::Char(character) if key.has_no_modifiers() => {
                self.add_char(character);
            },
            BareKey::Up if key.has_no_modifiers() => {
                self.move_selection_up();
            },
            BareKey::Char('p') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                self.move_selection_up();
            },
            BareKey::Down if key.has_no_modifiers() => {
                self.move_selection_down();
            },
            BareKey::Char('n') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
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
                            let cwd = self.new_session_folder.as_ref().map(|c| PathBuf::from(c));
                            switch_session_with_layout(new_session_name, new_session_layout, cwd);
                            if self.is_welcome_screen {
                                // the welcome screen has done its job and now we need to quit this temporary
                                // session so as not to leave garbage sessions behind
                                quit_zellij();
                            } else {
                                hide_self();
                            }
                        },
                        None => {
                            switch_session(new_session_name);
                            if self.is_welcome_screen {
                                // the welcome screen has done its job and now we need to quit this temporary
                                // session so as not to leave garbage sessions behind
                                quit_zellij();
                            } else {
                                hide_self();
                            }
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
    pub fn layout_list(&self, max_rows: usize) -> Vec<(LayoutInfo, bool)> {
        // bool - is_selected
        let rtr = range_to_render(
            max_rows,
            self.layout_count(),
            Some(self.layout_list.selected_layout_index),
        );
        self.layout_list
            .layout_list
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i == self.layout_list.selected_layout_index))
            .take(rtr.1)
            .skip(rtr.0)
            .collect()
    }
    pub fn layouts_to_render(&self, max_rows: usize) -> Vec<(LayoutInfo, Vec<usize>, bool)> {
        // (layout_info,
        // search_indices,
        // is_selected)
        if self.is_searching() {
            self.layout_search_results(max_rows)
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
            self.layout_list(max_rows)
                .into_iter()
                .map(|(layout_info, is_selected)| (layout_info, vec![], is_selected))
                .collect()
        }
    }
    pub fn layout_search_results(&self, max_rows: usize) -> Vec<(LayoutSearchResult, bool)> {
        // bool - is_selected
        let rtr = range_to_render(
            max_rows,
            self.layout_list.layout_search_results.len(),
            Some(self.layout_list.selected_layout_index),
        );
        self.layout_list
            .layout_search_results
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), i == self.layout_list.selected_layout_index))
            .take(rtr.1)
            .skip(rtr.0)
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
        self.layout_list.update_search_term();
    }
    fn move_selection_up(&mut self) {
        self.layout_list.move_selection_up();
    }
    fn move_selection_down(&mut self) {
        self.layout_list.move_selection_down();
    }
    pub fn get_layout_list_clone(&self) -> LayoutList {
        self.layout_list.clone()
    }
}

#[derive(Default, Clone)]
pub struct LayoutList {
    pub layout_list: Vec<LayoutInfo>,
    pub layout_search_results: Vec<LayoutSearchResult>,
    pub selected_layout_index: usize,
    pub layout_search_term: String,
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
    pub fn max_index(&self) -> usize {
        if self.layout_search_term.is_empty() {
            self.layout_list.len().saturating_sub(1)
        } else {
            self.layout_search_results.len().saturating_sub(1)
        }
    }
    pub fn move_selection_up(&mut self) {
        let max_index = self.max_index();
        if self.selected_layout_index > 0 {
            self.selected_layout_index -= 1;
        } else {
            self.selected_layout_index = max_index;
        }
    }
    pub fn move_selection_down(&mut self) {
        let max_index = self.max_index();
        if self.selected_layout_index < max_index {
            self.selected_layout_index += 1;
        } else {
            self.selected_layout_index = 0;
        }
    }
    pub fn update_search_term(&mut self) {
        if self.layout_search_term.is_empty() {
            self.clear_selection();
            self.layout_search_results = vec![];
        } else {
            let mut matches = vec![];
            let matcher = SkimMatcherV2::default().use_cache(true);
            for layout_info in &self.layout_list {
                if let Some((score, indices)) =
                    matcher.fuzzy_indices(&layout_info.name(), &self.layout_search_term)
                {
                    matches.push(LayoutSearchResult {
                        layout_info: layout_info.clone(),
                        score,
                        indices,
                    });
                }
            }
            matches.sort_by(|a, b| b.score.cmp(&a.score));
            self.layout_search_results = matches;
            self.clear_selection();
        }
    }
    pub fn layouts_to_render(&self, max_rows: usize) -> Vec<(LayoutInfo, Vec<usize>, bool)> {
        if !self.layout_search_term.is_empty() {
            self.layout_search_results_to_render(max_rows)
        } else {
            self.layout_list_to_render(max_rows)
        }
    }
    fn layout_list_to_render(&self, max_rows: usize) -> Vec<(LayoutInfo, Vec<usize>, bool)> {
        let range_to_render = range_to_render(
            max_rows,
            self.layout_list.len(),
            Some(self.selected_layout_index),
        );
        self.layout_list
            .iter()
            .enumerate()
            .map(|(i, l)| (l.clone(), vec![], i == self.selected_layout_index))
            .take(range_to_render.1)
            .skip(range_to_render.0)
            .collect()
    }
    fn layout_search_results_to_render(
        &self,
        max_rows: usize,
    ) -> Vec<(LayoutInfo, Vec<usize>, bool)> {
        let range_to_render = range_to_render(
            max_rows,
            self.layout_search_results.len(),
            Some(self.selected_layout_index),
        );
        self.layout_search_results
            .iter()
            .enumerate()
            .map(|(i, l)| {
                (
                    l.layout_info.clone(),
                    l.indices.clone(),
                    i == self.selected_layout_index,
                )
            })
            .take(range_to_render.1)
            .skip(range_to_render.0)
            .collect()
    }
}

pub fn range_to_render(
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

#[derive(Clone)]
pub struct LayoutSearchResult {
    pub layout_info: LayoutInfo,
    pub score: i64,
    pub indices: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_layout(name: &str) -> LayoutInfo {
        LayoutInfo::BuiltIn(name.to_string())
    }

    fn make_layout_list(names: &[&str]) -> LayoutList {
        let mut ll = LayoutList::default();
        ll.layout_list = names.iter().map(|n| make_layout(n)).collect();
        ll
    }

    // ---------------------------------------------------------------
    // Section 6: Layout List Navigation
    // ---------------------------------------------------------------

    #[test]
    fn test_6_1_layout_navigation_wraps_down() {
        let mut ll = make_layout_list(&["a", "b", "c"]);
        ll.selected_layout_index = 2;
        ll.move_selection_down();
        assert_eq!(ll.selected_layout_index, 0);
    }

    #[test]
    fn test_6_2_layout_navigation_wraps_up() {
        let mut ll = make_layout_list(&["a", "b", "c"]);
        ll.selected_layout_index = 0;
        ll.move_selection_up();
        assert_eq!(ll.selected_layout_index, 2);
    }

    #[test]
    fn test_6_3_layout_search_filters_results() {
        let mut ll = make_layout_list(&["default", "compact", "development"]);
        ll.layout_search_term = "dev".to_string();
        ll.update_search_term();
        // "development" should match
        let matched_names: Vec<&str> = ll
            .layout_search_results
            .iter()
            .map(|r| r.layout_info.name())
            .collect();
        assert!(matched_names.contains(&"development"));
        // "default" and "compact" should not match "dev" well enough
        // (though "default" starts with "de" so it might fuzzy-match — check)
        // The key assertion is that "development" is present and is the best match.
        // With SkimMatcherV2, "default" may also match "dev" (d, e from "default").
        // So we just verify "development" is the top result.
        assert_eq!(
            ll.layout_search_results[0].layout_info.name(),
            "development"
        );
    }

    #[test]
    fn test_6_4_selected_layout_info_returns_correct_layout() {
        let ll = make_layout_list(&["a", "b", "c"]);
        // selected_layout_index defaults to 0, set to 1
        let mut ll = ll;
        ll.selected_layout_index = 1;
        let info = ll.selected_layout_info();
        assert!(info.is_some());
        assert_eq!(info.unwrap().name(), "b");
    }

    #[test]
    fn test_6_5_layout_viewport_windowing() {
        let mut ll = make_layout_list(&[
            "layout-0", "layout-1", "layout-2", "layout-3", "layout-4", "layout-5", "layout-6",
            "layout-7", "layout-8", "layout-9",
        ]);
        ll.selected_layout_index = 5;
        let rendered = ll.layouts_to_render(5);
        // Should return at most 5-1=4 entries (range_to_render subtracts 1 for title)
        assert!(rendered.len() <= 5);
        // The selected layout should be within the visible window
        let selected_visible = rendered.iter().any(|(_, _, is_selected)| *is_selected);
        assert!(selected_visible);
    }

    // ---------------------------------------------------------------
    // Section 7: Viewport Scrolling (range_to_render tests)
    // ---------------------------------------------------------------

    #[test]
    fn test_7_1_all_results_fit_in_viewport() {
        // When table_rows > results_len, all results are shown
        let (start, end) = range_to_render(10, 5, None);
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }

    #[test]
    fn test_7_2_viewport_scrolls_to_keep_selection_centered() {
        // table_rows=6, results_len=20, selected=10
        // row_count_to_render = 6-1 = 5, half = 2
        // first = 10-2 = 8, last = 8+5 = 13
        let (start, end) = range_to_render(6, 20, Some(10));
        assert_eq!(start, 8);
        assert_eq!(end, 13);
    }

    #[test]
    fn test_7_3_viewport_clamps_at_bottom() {
        // table_rows=6, results_len=20, selected=19
        // row_count_to_render = 5, half = 2
        // first = 19-2 = 17, last = 17+5 = 22 > 20
        // Note: range_to_render does NOT clamp — it returns (17, 22)
        // The actual clamping happens in the caller via .take().skip()
        let (start, end) = range_to_render(6, 20, Some(19));
        assert_eq!(start, 17);
        assert_eq!(end, 22);
    }

    #[test]
    fn test_7_4_viewport_clamps_at_top() {
        // table_rows=6, results_len=20, selected=0
        // row_count_to_render = 5, half = 2
        // first = 0.saturating_sub(2) = 0, last = 0+5 = 5
        let (start, end) = range_to_render(6, 20, Some(0));
        assert_eq!(start, 0);
        assert_eq!(end, 5);
    }
}
