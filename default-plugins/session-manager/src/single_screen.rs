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
        self.selected_index = None;

        if self.search_term.is_empty() {
            let mut results = Vec::new();
            for session in active_sessions {
                results.push(UnifiedSearchResult::ActiveSession {
                    score: 0,
                    indices: vec![],
                    session_name: session.name.clone(),
                    connected_users: session.connected_users,
                    tab_count: session.tabs.len(),
                    pane_count: session.tabs.iter().fold(0, |acc, t| acc + t.panes.len()),
                    is_current_session: session.is_current_session,
                });
            }
            for (name, ctime) in resurrectable_sessions {
                results.push(UnifiedSearchResult::ResurrectableSession {
                    score: 0,
                    indices: vec![],
                    session_name: name.clone(),
                    ctime: *ctime,
                });
            }
            self.unified_results = results;
        } else {
            let matcher = SkimMatcherV2::default().use_cache(true);
            let mut results = Vec::new();

            for session in active_sessions {
                if let Some((score, indices)) =
                    matcher.fuzzy_indices(&session.name, &self.search_term)
                {
                    results.push(UnifiedSearchResult::ActiveSession {
                        score,
                        indices,
                        session_name: session.name.clone(),
                        connected_users: session.connected_users,
                        tab_count: session.tabs.len(),
                        pane_count: session.tabs.iter().fold(0, |acc, t| acc + t.panes.len()),
                        is_current_session: session.is_current_session,
                    });
                }
            }
            for (name, ctime) in resurrectable_sessions {
                if let Some((score, indices)) = matcher.fuzzy_indices(name, &self.search_term) {
                    results.push(UnifiedSearchResult::ResurrectableSession {
                        score,
                        indices,
                        session_name: name.clone(),
                        ctime: *ctime,
                    });
                }
            }
            results.sort_by(|a, b| {
                let score_a = match a {
                    UnifiedSearchResult::ActiveSession { score, .. } => *score,
                    UnifiedSearchResult::ResurrectableSession { score, .. } => *score,
                };
                let score_b = match b {
                    UnifiedSearchResult::ActiveSession { score, .. } => *score,
                    UnifiedSearchResult::ResurrectableSession { score, .. } => *score,
                };
                score_b.cmp(&score_a)
            });
            self.unified_results = results;
        }
    }

    pub fn move_selection_down(&mut self) {
        if self.unified_results.is_empty() {
            return;
        }
        match self.selected_index {
            None => self.selected_index = Some(0),
            Some(i) => {
                if i + 1 < self.unified_results.len() {
                    self.selected_index = Some(i + 1);
                } else {
                    self.selected_index = Some(0);
                }
            },
        }
    }

    pub fn move_selection_up(&mut self) {
        if self.unified_results.is_empty() {
            return;
        }
        match self.selected_index {
            None => self.selected_index = Some(self.unified_results.len().saturating_sub(1)),
            Some(0) => self.selected_index = Some(self.unified_results.len().saturating_sub(1)),
            Some(i) => self.selected_index = Some(i - 1),
        }
    }

    pub fn tab_complete(
        &mut self,
        active_sessions: &[SessionUiInfo],
        resurrectable_sessions: &[(String, Duration)],
    ) {
        if let Some(first_result) = self.unified_results.first() {
            let name = first_result.session_name().to_owned();
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
