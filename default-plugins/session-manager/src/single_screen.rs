use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::path::PathBuf;
use std::time::Duration;

use crate::new_session_info::LayoutList;
use crate::ui::SessionUiInfo;

#[derive(Debug, Clone, PartialEq)]
pub enum SingleScreenMode {
    SearchAndSelect,
    SelectingLayout,
}

impl Default for SingleScreenMode {
    fn default() -> Self {
        SingleScreenMode::SearchAndSelect
    }
}

#[derive(Debug)]
pub enum UnifiedSearchResult {
    ActiveSession {
        score: i64,
        indices: Vec<usize>,
        session_name: String,
        connected_users: usize,
        tab_count: usize,
        pane_count: usize,
        is_current_session: bool,
        creation_time: Duration,
    },
    ResurrectableSession {
        score: i64,
        indices: Vec<usize>,
        session_name: String,
        ctime: Duration,
    },
}

impl UnifiedSearchResult {
    pub fn session_name(&self) -> &str {
        match self {
            UnifiedSearchResult::ActiveSession { session_name, .. } => session_name.as_str(),
            UnifiedSearchResult::ResurrectableSession { session_name, .. } => session_name.as_str(),
        }
    }
    fn score(&self) -> i64 {
        match self {
            UnifiedSearchResult::ActiveSession { score, .. } => *score,
            UnifiedSearchResult::ResurrectableSession { score, .. } => *score,
        }
    }
    /// Ordering by type (active before resurrectable), then by creation time ascending
    /// (smaller elapsed duration = more recently created = appears first).
    fn cmp_by_type_then_recency(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (
                UnifiedSearchResult::ActiveSession {
                    creation_time: ct_a,
                    ..
                },
                UnifiedSearchResult::ActiveSession {
                    creation_time: ct_b,
                    ..
                },
            ) => ct_a.cmp(ct_b),
            (
                UnifiedSearchResult::ResurrectableSession { ctime: ct_a, .. },
                UnifiedSearchResult::ResurrectableSession { ctime: ct_b, .. },
            ) => ct_a.cmp(ct_b),
            (
                UnifiedSearchResult::ActiveSession { .. },
                UnifiedSearchResult::ResurrectableSession { .. },
            ) => std::cmp::Ordering::Less,
            (
                UnifiedSearchResult::ResurrectableSession { .. },
                UnifiedSearchResult::ActiveSession { .. },
            ) => std::cmp::Ordering::Greater,
        }
    }
}

#[derive(Default)]
pub struct SingleScreenState {
    pub search_term: String,
    pub unified_results: Vec<UnifiedSearchResult>,
    pub selected_index: Option<usize>,
    pub mode: SingleScreenMode,
    pub layout_list: LayoutList,
    pub new_session_folder: Option<PathBuf>,
    pub is_welcome_screen: bool,
}

impl std::fmt::Debug for SingleScreenState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleScreenState")
            .field("search_term", &self.search_term)
            .field("selected_index", &self.selected_index)
            .field("mode", &self.mode)
            .field("result_count", &self.unified_results.len())
            .finish()
    }
}

impl SingleScreenState {
    pub fn update_search_term(
        &mut self,
        active_sessions: &[SessionUiInfo],
        resurrectable_sessions: &[(String, Duration)],
    ) {
        let previously_selected_name = self.previously_selected_name();
        self.selected_index = None;

        if self.search_term.is_empty() {
            self.unified_results =
                Self::collect_all_sessions(active_sessions, resurrectable_sessions);
            self.unified_results
                .sort_by(|a, b| a.cmp_by_type_then_recency(b));
        } else {
            self.unified_results = Self::collect_fuzzy_matched_sessions(
                &self.search_term,
                active_sessions,
                resurrectable_sessions,
            );
            self.unified_results.sort_by(|a, b| {
                let score_cmp = b.score().cmp(&a.score());
                if score_cmp != std::cmp::Ordering::Equal {
                    return score_cmp;
                }
                a.cmp_by_type_then_recency(b)
            });
        }

        self.restore_selection(previously_selected_name);
    }

    fn previously_selected_name(&self) -> Option<String> {
        self.selected_index
            .and_then(|i| self.unified_results.get(i))
            .map(|r| r.session_name().to_owned())
    }

    fn restore_selection(&mut self, previously_selected_name: Option<String>) {
        if let Some(prev_name) = previously_selected_name {
            self.selected_index = self
                .unified_results
                .iter()
                .position(|r| r.session_name() == prev_name);
        }
    }

    fn collect_all_sessions(
        active_sessions: &[SessionUiInfo],
        resurrectable_sessions: &[(String, Duration)],
    ) -> Vec<UnifiedSearchResult> {
        let mut results = Vec::new();
        for session in active_sessions {
            results.push(Self::active_session_to_result(session, 0, vec![]));
        }
        for (name, ctime) in resurrectable_sessions {
            results.push(UnifiedSearchResult::ResurrectableSession {
                score: 0,
                indices: vec![],
                session_name: name.clone(),
                ctime: *ctime,
            });
        }
        results
    }

    fn collect_fuzzy_matched_sessions(
        search_term: &str,
        active_sessions: &[SessionUiInfo],
        resurrectable_sessions: &[(String, Duration)],
    ) -> Vec<UnifiedSearchResult> {
        let matcher = SkimMatcherV2::default().use_cache(true);
        let mut results = Vec::new();

        for session in active_sessions {
            if let Some((score, indices)) = matcher.fuzzy_indices(&session.name, search_term) {
                results.push(Self::active_session_to_result(session, score, indices));
            }
        }
        for (name, ctime) in resurrectable_sessions {
            if let Some((score, indices)) = matcher.fuzzy_indices(name, search_term) {
                results.push(UnifiedSearchResult::ResurrectableSession {
                    score,
                    indices,
                    session_name: name.clone(),
                    ctime: *ctime,
                });
            }
        }
        results
    }

    fn active_session_to_result(
        session: &SessionUiInfo,
        score: i64,
        indices: Vec<usize>,
    ) -> UnifiedSearchResult {
        UnifiedSearchResult::ActiveSession {
            score,
            indices,
            session_name: session.name.clone(),
            connected_users: session.connected_users,
            tab_count: session.tabs.len(),
            pane_count: session.tabs.iter().fold(0, |acc, t| acc + t.panes.len()),
            is_current_session: session.is_current_session,
            creation_time: session.creation_time,
        }
    }

    fn is_current_session(&self, index: usize) -> bool {
        matches!(
            self.unified_results.get(index),
            Some(UnifiedSearchResult::ActiveSession {
                is_current_session: true,
                ..
            })
        )
    }

    fn next_selectable_down(&self, from: usize) -> Option<usize> {
        let len = self.unified_results.len();
        for offset in 1..=len {
            let candidate = (from + offset) % len;
            if !self.is_current_session(candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn next_selectable_up(&self, from: usize) -> Option<usize> {
        let len = self.unified_results.len();
        for offset in 1..=len {
            let candidate = (from + len - offset) % len;
            if !self.is_current_session(candidate) {
                return Some(candidate);
            }
        }
        None
    }

    pub fn move_selection_down(&mut self) {
        if self.unified_results.is_empty() {
            return;
        }
        match self.selected_index {
            None => {
                self.selected_index =
                    self.next_selectable_down(self.unified_results.len().saturating_sub(1));
            },
            Some(i) => {
                self.selected_index = self.next_selectable_down(i);
            },
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.unified_results.is_empty() {
            return;
        }
        match self.selected_index {
            None => {
                self.selected_index = self.next_selectable_up(0);
            },
            Some(i) => {
                self.selected_index = self.next_selectable_up(i);
            },
        }
    }

    pub fn tab_complete(
        &mut self,
        active_sessions: &[SessionUiInfo],
        resurrectable_sessions: &[(String, Duration)],
    ) {
        let first_non_current = self.unified_results.iter().find(|r| {
            !matches!(
                r,
                UnifiedSearchResult::ActiveSession {
                    is_current_session: true,
                    ..
                }
            )
        });
        if let Some(result) = first_non_current {
            let name = result.session_name().to_owned();
            self.search_term = name;
            self.update_search_term(active_sessions, resurrectable_sessions);
        }
    }

    pub fn get_selected_result(&self) -> Option<&UnifiedSearchResult> {
        self.selected_index
            .and_then(|i| self.unified_results.get(i))
    }

    pub fn transition_to_layout_selection(&mut self) {
        self.mode = SingleScreenMode::SelectingLayout;
    }

    pub fn transition_to_search(&mut self) {
        self.mode = SingleScreenMode::SearchAndSelect;
        self.layout_list.layout_search_term.clear();
        self.layout_list.clear_selection();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_active_session(
        name: &str,
        tabs: usize,
        panes_per_tab: usize,
        connected: usize,
        is_current: bool,
        creation_secs: u64,
    ) -> SessionUiInfo {
        SessionUiInfo {
            name: name.to_string(),
            tabs: (0..tabs)
                .map(|i| crate::ui::TabUiInfo {
                    name: format!("tab-{}", i),
                    panes: (0..panes_per_tab)
                        .map(|j| crate::ui::PaneUiInfo {
                            name: format!("pane-{}", j),
                            exit_code: None,
                            pane_id: j as u32,
                            is_plugin: false,
                        })
                        .collect(),
                    position: i,
                })
                .collect(),
            connected_users: connected,
            is_current_session: is_current,
            creation_time: Duration::from_secs(creation_secs),
        }
    }

    fn make_resurrectable(name: &str, ctime_secs: u64) -> (String, Duration) {
        (name.to_string(), Duration::from_secs(ctime_secs))
    }

    // ---------------------------------------------------------------
    // Section 1: Session Display and Sorting
    // ---------------------------------------------------------------

    #[test]
    fn test_1_1_active_sessions_sorted_by_recency() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let active = vec![
            make_active_session("sess-300", 1, 1, 1, false, 300),
            make_active_session("sess-100", 1, 1, 1, false, 100),
            make_active_session("sess-200", 1, 1, 1, false, 200),
        ];
        state.update_search_term(&active, &[]);
        assert_eq!(state.unified_results.len(), 3);
        // Ascending creation_time: 100, 200, 300 (most recent first)
        assert_eq!(state.unified_results[0].session_name(), "sess-100");
        assert_eq!(state.unified_results[1].session_name(), "sess-200");
        assert_eq!(state.unified_results[2].session_name(), "sess-300");
        for r in &state.unified_results {
            assert!(matches!(r, UnifiedSearchResult::ActiveSession { .. }));
        }
    }

    #[test]
    fn test_1_2_resurrectable_sessions_sorted_by_recency() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let resurrectable = vec![
            make_resurrectable("res-300", 300),
            make_resurrectable("res-100", 100),
            make_resurrectable("res-200", 200),
        ];
        state.update_search_term(&[], &resurrectable);
        assert_eq!(state.unified_results.len(), 3);
        assert_eq!(state.unified_results[0].session_name(), "res-100");
        assert_eq!(state.unified_results[1].session_name(), "res-200");
        assert_eq!(state.unified_results[2].session_name(), "res-300");
        for r in &state.unified_results {
            assert!(matches!(
                r,
                UnifiedSearchResult::ResurrectableSession { .. }
            ));
        }
    }

    #[test]
    fn test_1_3_mixed_active_and_resurrectable_active_first() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let active = vec![
            make_active_session("active-200", 1, 1, 1, false, 200),
            make_active_session("active-100", 1, 1, 1, false, 100),
        ];
        let resurrectable = vec![
            make_resurrectable("res-50", 50),
            make_resurrectable("res-150", 150),
        ];
        state.update_search_term(&active, &resurrectable);
        assert_eq!(state.unified_results.len(), 4);
        assert_eq!(state.unified_results[0].session_name(), "active-100");
        assert_eq!(state.unified_results[1].session_name(), "active-200");
        assert_eq!(state.unified_results[2].session_name(), "res-50");
        assert_eq!(state.unified_results[3].session_name(), "res-150");
    }

    #[test]
    fn test_1_4_current_session_included_in_results() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let active = vec![
            make_active_session("current-sess", 1, 1, 1, true, 100),
            make_active_session("other-sess", 1, 1, 1, false, 200),
        ];
        state.update_search_term(&active, &[]);
        assert_eq!(state.unified_results.len(), 2);
        // Current session should be present (filtering is done in renderer)
        let has_current = state.unified_results.iter().any(|r| match r {
            UnifiedSearchResult::ActiveSession {
                is_current_session, ..
            } => *is_current_session,
            _ => false,
        });
        assert!(has_current);
    }

    #[test]
    fn test_1_5_pane_counts_reflect_tab_structure() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let active = vec![make_active_session("sess", 3, 2, 1, false, 100)];
        state.update_search_term(&active, &[]);
        assert_eq!(state.unified_results.len(), 1);
        match &state.unified_results[0] {
            UnifiedSearchResult::ActiveSession {
                tab_count,
                pane_count,
                ..
            } => {
                assert_eq!(*tab_count, 3);
                assert_eq!(*pane_count, 6);
            },
            _ => panic!("Expected ActiveSession"),
        }
    }

    #[test]
    fn test_1_6_empty_sessions_list() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        state.update_search_term(&[], &[]);
        assert!(state.unified_results.is_empty());
    }

    // ---------------------------------------------------------------
    // Section 2: Search and Fuzzy Matching
    // ---------------------------------------------------------------

    #[test]
    fn test_2_1_empty_search_term_shows_all() {
        let mut state = SingleScreenState::default();
        state.search_term = String::new();
        let active = vec![
            make_active_session("a", 1, 1, 1, false, 100),
            make_active_session("b", 1, 1, 1, false, 200),
            make_active_session("c", 1, 1, 1, false, 300),
        ];
        let resurrectable = vec![make_resurrectable("d", 400), make_resurrectable("e", 500)];
        state.update_search_term(&active, &resurrectable);
        assert_eq!(state.unified_results.len(), 5);
    }

    #[test]
    fn test_2_2_typing_filters_results() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("alpha", 1, 1, 1, false, 100),
            make_active_session("beta", 1, 1, 1, false, 200),
            make_active_session("gamma", 1, 1, 1, false, 300),
        ];
        state.search_term = "alp".to_string();
        state.update_search_term(&active, &[]);
        assert_eq!(state.unified_results.len(), 1);
        assert_eq!(state.unified_results[0].session_name(), "alpha");
        match &state.unified_results[0] {
            UnifiedSearchResult::ActiveSession { score, indices, .. } => {
                assert!(*score > 0);
                assert!(!indices.is_empty());
            },
            _ => panic!("Expected ActiveSession"),
        }
    }

    #[test]
    fn test_2_3_fuzzy_matching_non_contiguous() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("my-project", 1, 1, 1, false, 100),
            make_active_session("mapping", 1, 1, 1, false, 200),
        ];
        state.search_term = "mprj".to_string();
        state.update_search_term(&active, &[]);
        // At least "my-project" should match (m, p, r, j fuzzy)
        let has_my_project = state
            .unified_results
            .iter()
            .any(|r| r.session_name() == "my-project");
        assert!(has_my_project);
    }

    #[test]
    fn test_2_4_score_based_ordering() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("abc", 1, 1, 1, false, 100),
            make_active_session("xabc", 1, 1, 1, false, 200),
            make_active_session("xxabc", 1, 1, 1, false, 300),
        ];
        state.search_term = "abc".to_string();
        state.update_search_term(&active, &[]);
        assert!(!state.unified_results.is_empty());
        // "abc" (exact match) should have the highest score and appear first
        assert_eq!(state.unified_results[0].session_name(), "abc");
        // Verify descending score order
        let scores: Vec<i64> = state
            .unified_results
            .iter()
            .map(|r| match r {
                UnifiedSearchResult::ActiveSession { score, .. } => *score,
                UnifiedSearchResult::ResurrectableSession { score, .. } => *score,
            })
            .collect();
        for i in 0..scores.len().saturating_sub(1) {
            assert!(scores[i] >= scores[i + 1]);
        }
    }

    #[test]
    fn test_2_5_tie_breaking_active_before_resurrectable() {
        let mut state = SingleScreenState::default();
        let active = vec![make_active_session("test-session", 1, 1, 1, false, 100)];
        let resurrectable = vec![make_resurrectable("test-session", 100)];
        state.search_term = "test".to_string();
        state.update_search_term(&active, &resurrectable);
        assert!(state.unified_results.len() >= 2);
        // Find positions of active and resurrectable with the same name
        let active_pos = state
            .unified_results
            .iter()
            .position(|r| {
                matches!(r, UnifiedSearchResult::ActiveSession { session_name, .. } if session_name == "test-session")
            });
        let resurrectable_pos = state
            .unified_results
            .iter()
            .position(|r| {
                matches!(r, UnifiedSearchResult::ResurrectableSession { session_name, .. } if session_name == "test-session")
            });
        assert!(active_pos.unwrap() < resurrectable_pos.unwrap());
    }

    #[test]
    fn test_2_6_tie_breaking_more_recent_first_at_equal_score_and_type() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("test-a", 1, 1, 1, false, 100),
            make_active_session("test-b", 1, 1, 1, false, 50),
        ];
        state.search_term = "test".to_string();
        state.update_search_term(&active, &[]);
        // At equal scores, ascending creation_time places test-b (50s) before test-a (100s)
        let pos_a = state
            .unified_results
            .iter()
            .position(|r| r.session_name() == "test-a");
        let pos_b = state
            .unified_results
            .iter()
            .position(|r| r.session_name() == "test-b");
        if let (Some(a), Some(b)) = (pos_a, pos_b) {
            // If scores are equal, test-b should come first (more recent)
            let score_a = match &state.unified_results[a] {
                UnifiedSearchResult::ActiveSession { score, .. } => *score,
                _ => 0,
            };
            let score_b = match &state.unified_results[b] {
                UnifiedSearchResult::ActiveSession { score, .. } => *score,
                _ => 0,
            };
            if score_a == score_b {
                assert!(
                    b < a,
                    "test-b (50s, more recent) should appear before test-a (100s)"
                );
            }
        }
    }

    #[test]
    fn test_2_7_search_term_matches_nothing() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("alpha", 1, 1, 1, false, 100),
            make_active_session("beta", 1, 1, 1, false, 200),
        ];
        state.search_term = "zzz".to_string();
        state.update_search_term(&active, &[]);
        assert!(state.unified_results.is_empty());
    }

    #[test]
    fn test_2_8_selection_preserved_across_search_updates() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("alpha", 1, 1, 1, false, 100),
            make_active_session("beta", 1, 1, 1, false, 200),
            make_active_session("gamma", 1, 1, 1, false, 300),
        ];
        // Step 1: empty search, populate results
        state.search_term = String::new();
        state.update_search_term(&active, &[]);
        // Step 2: select "beta" (find its index)
        let beta_idx = state
            .unified_results
            .iter()
            .position(|r| r.session_name() == "beta")
            .unwrap();
        state.selected_index = Some(beta_idx);
        // Step 3: search for "a" — beta does not match
        state.search_term = "a".to_string();
        state.update_search_term(&active, &[]);
        // "beta" does not contain "a" in a way that fuzzy matches well.
        // Check: if beta is not in results, selected_index should be None
        let beta_still_present = state
            .unified_results
            .iter()
            .any(|r| r.session_name() == "beta");
        if !beta_still_present {
            assert_eq!(state.selected_index, None);
        } else {
            // If beta still matches, selected_index should point to its new position
            let new_beta_idx = state
                .unified_results
                .iter()
                .position(|r| r.session_name() == "beta")
                .unwrap();
            assert_eq!(state.selected_index, Some(new_beta_idx));
        }
    }

    // ---------------------------------------------------------------
    // Section 3: Selection and Navigation
    // ---------------------------------------------------------------

    fn setup_results_with_current(state: &mut SingleScreenState, names: &[(&str, bool)]) {
        // Directly populate unified_results for navigation tests
        state.unified_results = names
            .iter()
            .enumerate()
            .map(
                |(i, (name, is_current))| UnifiedSearchResult::ActiveSession {
                    score: 0,
                    indices: vec![],
                    session_name: name.to_string(),
                    connected_users: 1,
                    tab_count: 1,
                    pane_count: 1,
                    is_current_session: *is_current,
                    creation_time: Duration::from_secs(i as u64 * 100),
                },
            )
            .collect();
    }

    #[test]
    fn test_3_1_down_from_none_selects_first_non_current() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[("current", true), ("other-a", false), ("other-b", false)],
        );
        state.selected_index = None;
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(1));
    }

    #[test]
    fn test_3_2_down_from_last_wraps_to_first_non_current() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[("other-a", false), ("current", true), ("other-b", false)],
        );
        state.selected_index = Some(2); // other-b, the last
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(0)); // wraps, skips current at 1
    }

    #[test]
    fn test_3_3_up_from_none_selects_last_non_current() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[("other-a", false), ("current", true), ("other-b", false)],
        );
        state.selected_index = None;
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(2)); // other-b
    }

    #[test]
    fn test_3_4_up_from_first_wraps_to_last_non_current() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[("other-a", false), ("current", true), ("other-b", false)],
        );
        state.selected_index = Some(0);
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(2)); // wraps, skips current
    }

    #[test]
    fn test_3_5_navigation_with_only_current_session() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(&mut state, &[("current", true)]);
        state.selected_index = None;
        state.move_selection_down();
        assert_eq!(state.selected_index, None);
        state.move_selection_up();
        assert_eq!(state.selected_index, None);
    }

    #[test]
    fn test_3_6_navigation_with_empty_results() {
        let mut state = SingleScreenState::default();
        state.unified_results = vec![];
        state.selected_index = None;
        state.move_selection_down();
        assert_eq!(state.selected_index, None);
        state.move_selection_up();
        assert_eq!(state.selected_index, None);
    }

    #[test]
    fn test_3_7_sequential_down_visits_all_non_current() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[
                ("a", false),
                ("current", true),
                ("b", false),
                ("c", false),
                ("d", false),
            ],
        );
        state.selected_index = None;
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(0)); // "a"
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(2)); // "b", skip current at 1
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(3)); // "c"
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(4)); // "d"
        state.move_selection_down();
        assert_eq!(state.selected_index, Some(0)); // "a", wrapped
    }

    #[test]
    fn test_3_8_sequential_up_visits_all_non_current_in_reverse() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(
            &mut state,
            &[
                ("a", false),
                ("current", true),
                ("b", false),
                ("c", false),
                ("d", false),
            ],
        );
        state.selected_index = None;
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(4)); // "d"
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(3)); // "c"
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(2)); // "b"
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(0)); // "a", skip current at 1
        state.move_selection_up();
        assert_eq!(state.selected_index, Some(4)); // "d", wrapped
    }

    #[test]
    fn test_3_9_get_selected_result_returns_correct_result() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(&mut state, &[("a", false), ("b", false), ("c", false)]);
        state.selected_index = Some(1);
        let result = state.get_selected_result();
        assert!(result.is_some());
        assert_eq!(result.unwrap().session_name(), "b");
    }

    #[test]
    fn test_3_10_get_selected_result_returns_none_when_nothing_selected() {
        let mut state = SingleScreenState::default();
        setup_results_with_current(&mut state, &[("a", false), ("b", false), ("c", false)]);
        state.selected_index = None;
        assert!(state.get_selected_result().is_none());
    }

    // ---------------------------------------------------------------
    // Section 4: Tab Completion
    // ---------------------------------------------------------------

    #[test]
    fn test_4_1_basic_tab_completion() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("mine", 1, 1, 1, true, 100),
            make_active_session("alpha", 1, 1, 1, false, 200),
            make_active_session("beta", 1, 1, 1, false, 300),
        ];
        state.search_term = String::new();
        state.update_search_term(&active, &[]);
        state.tab_complete(&active, &[]);
        // First non-current in sorted results should be "alpha" or whichever is first
        // With empty search, sorted by creation_time ascending: mine(100), alpha(200), beta(300)
        // First non-current is "alpha"
        assert_eq!(state.search_term, "alpha");
    }

    #[test]
    fn test_4_2_tab_completion_only_current_session_noop() {
        let mut state = SingleScreenState::default();
        let active = vec![make_active_session("mine", 1, 1, 1, true, 100)];
        state.search_term = String::new();
        state.update_search_term(&active, &[]);
        state.tab_complete(&active, &[]);
        assert_eq!(state.search_term, "");
    }

    #[test]
    fn test_4_3_tab_completion_picks_resurrectable_if_no_non_current_active() {
        let mut state = SingleScreenState::default();
        let active = vec![make_active_session("mine", 1, 1, 1, true, 100)];
        let resurrectable = vec![make_resurrectable("old-session", 100)];
        state.search_term = String::new();
        state.update_search_term(&active, &resurrectable);
        state.tab_complete(&active, &resurrectable);
        assert_eq!(state.search_term, "old-session");
    }

    #[test]
    fn test_4_4_tab_completion_with_partial_search_term() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("mine", 1, 1, 1, true, 100),
            make_active_session("alpha", 1, 1, 1, false, 200),
            make_active_session("beta", 1, 1, 1, false, 300),
        ];
        let resurrectable: Vec<(String, Duration)> = vec![];
        state.search_term = "be".to_string();
        state.update_search_term(&active, &resurrectable);
        state.tab_complete(&active, &resurrectable);
        assert_eq!(state.search_term, "beta");
    }

    #[test]
    fn test_4_5_tab_completion_with_empty_results_noop() {
        let mut state = SingleScreenState::default();
        let active = vec![
            make_active_session("alpha", 1, 1, 1, false, 100),
            make_active_session("beta", 1, 1, 1, false, 200),
        ];
        state.search_term = "zzz".to_string();
        state.update_search_term(&active, &[]);
        // Results should be empty — no matches
        assert!(state.unified_results.is_empty());
        state.tab_complete(&active, &[]);
        assert_eq!(state.search_term, "zzz"); // unchanged
    }

    // ---------------------------------------------------------------
    // Section 5: Mode Transitions
    // ---------------------------------------------------------------

    #[test]
    fn test_5_1_transition_to_layout_selection() {
        let mut state = SingleScreenState::default();
        state.mode = SingleScreenMode::SearchAndSelect;
        state.search_term = "my-session".to_string();
        state.transition_to_layout_selection();
        assert_eq!(state.mode, SingleScreenMode::SelectingLayout);
        assert_eq!(state.search_term, "my-session"); // preserved
    }

    #[test]
    fn test_5_2_transition_back_to_search() {
        let mut state = SingleScreenState::default();
        state.mode = SingleScreenMode::SelectingLayout;
        state.layout_list.layout_search_term = "some-layout".to_string();
        state.layout_list.selected_layout_index = 3;
        state.transition_to_search();
        assert_eq!(state.mode, SingleScreenMode::SearchAndSelect);
        assert_eq!(state.layout_list.layout_search_term, "");
        assert_eq!(state.layout_list.selected_layout_index, 0);
    }

    #[test]
    fn test_5_3_round_trip_transition_preserves_search_term() {
        let mut state = SingleScreenState::default();
        state.search_term = "my-new-session".to_string();
        state.mode = SingleScreenMode::SearchAndSelect;
        state.transition_to_layout_selection();
        assert_eq!(state.mode, SingleScreenMode::SelectingLayout);
        state.transition_to_search();
        assert_eq!(state.search_term, "my-new-session");
        assert_eq!(state.mode, SingleScreenMode::SearchAndSelect);
    }
}
