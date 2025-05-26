use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use crate::ui::{
    components::{Colors, LineToRender, ListItem},
    SessionUiInfo,
};

#[derive(Debug, Default)]
pub struct SessionList {
    pub session_ui_infos: Vec<SessionUiInfo>,
    pub forbidden_sessions: Vec<SessionUiInfo>,
    pub selected_index: SelectedIndex,
    pub selected_search_index: Option<usize>,
    pub search_results: Vec<SearchResult>,
    pub is_searching: bool,
}

impl SessionList {
    pub fn set_sessions(
        &mut self,
        mut session_ui_infos: Vec<SessionUiInfo>,
        mut forbidden_sessions: Vec<SessionUiInfo>,
    ) {
        session_ui_infos.sort_unstable_by(|a, b| {
            if a.is_current_session {
                std::cmp::Ordering::Less
            } else if b.is_current_session {
                std::cmp::Ordering::Greater
            } else {
                a.name.cmp(&b.name)
            }
        });
        forbidden_sessions.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        self.session_ui_infos = session_ui_infos;
        self.forbidden_sessions = forbidden_sessions;
    }
    pub fn update_search_term(&mut self, search_term: &str, colors: &Colors) {
        let mut flattened_assets = self.flatten_assets(colors);
        let mut matches = vec![];
        let matcher = SkimMatcherV2::default().use_cache(true);
        for (list_item, session_name, tab_position, pane_id, is_current_session) in
            flattened_assets.drain(..)
        {
            if let Some((score, indices)) = matcher.fuzzy_indices(&list_item.name, &search_term) {
                matches.push(SearchResult::new(
                    score,
                    indices,
                    list_item,
                    session_name,
                    tab_position,
                    pane_id,
                    is_current_session,
                ));
            }
        }
        matches.sort_by(|a, b| b.score.cmp(&a.score));
        self.search_results = matches;
        self.is_searching = !search_term.is_empty();
        self.selected_search_index = Some(0);
    }
    fn flatten_assets(
        &self,
        colors: &Colors,
    ) -> Vec<(ListItem, String, Option<usize>, Option<(u32, bool)>, bool)> {
        // list_item, session_name, tab_position, (pane_id, is_plugin), is_current_session
        let mut list_items = vec![];
        for session in &self.session_ui_infos {
            let session_name = session.name.clone();
            let is_current_session = session.is_current_session;
            list_items.push((
                ListItem::from_session_info(session, *colors),
                session_name.clone(),
                None,
                None,
                is_current_session,
            ));
            for tab in &session.tabs {
                let tab_position = tab.position;
                list_items.push((
                    ListItem::from_tab_info(session, tab, *colors),
                    session_name.clone(),
                    Some(tab_position),
                    None,
                    is_current_session,
                ));
                for pane in &tab.panes {
                    let pane_id = (pane.pane_id, pane.is_plugin);
                    list_items.push((
                        ListItem::from_pane_info(session, tab, pane, *colors),
                        session_name.clone(),
                        Some(tab_position),
                        Some(pane_id),
                        is_current_session,
                    ));
                }
            }
        }
        list_items
    }
    pub fn get_selected_session_name(&self) -> Option<String> {
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .map(|s| s.session_name.clone())
        } else {
            self.selected_index
                .0
                .and_then(|i| self.session_ui_infos.get(i))
                .map(|s_i| s_i.name.clone())
        }
    }
    pub fn selected_is_current_session(&self) -> bool {
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .map(|s| s.is_current_session)
                .unwrap_or(false)
        } else {
            self.selected_index
                .0
                .and_then(|i| self.session_ui_infos.get(i))
                .map(|s_i| s_i.is_current_session)
                .unwrap_or(false)
        }
    }
    pub fn get_selected_tab_position(&self) -> Option<usize> {
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .and_then(|s| s.tab_position)
        } else {
            self.selected_index
                .0
                .and_then(|i| self.session_ui_infos.get(i))
                .and_then(|s_i| {
                    self.selected_index
                        .1
                        .and_then(|i| s_i.tabs.get(i))
                        .map(|t| t.position)
                })
        }
    }
    pub fn get_selected_pane_id(&self) -> Option<(u32, bool)> {
        // (pane_id, is_plugin)
        if self.is_searching {
            self.selected_search_index
                .and_then(|i| self.search_results.get(i))
                .and_then(|s| s.pane_id)
        } else {
            self.selected_index
                .0
                .and_then(|i| self.session_ui_infos.get(i))
                .and_then(|s_i| {
                    self.selected_index
                        .1
                        .and_then(|i| s_i.tabs.get(i))
                        .and_then(|t| {
                            self.selected_index
                                .2
                                .and_then(|i| t.panes.get(i))
                                .map(|p| (p.pane_id, p.is_plugin))
                        })
                })
        }
    }
    pub fn move_selection_down(&mut self) {
        if self.is_searching {
            match self.selected_search_index.as_mut() {
                Some(search_index) => {
                    *search_index = search_index.saturating_add(1);
                },
                None => {
                    if !self.search_results.is_empty() {
                        self.selected_search_index = Some(0);
                    }
                },
            }
        } else {
            match self.selected_index {
                SelectedIndex(None, None, None) => {
                    if !self.session_ui_infos.is_empty() {
                        self.selected_index.0 = Some(0);
                    }
                },
                SelectedIndex(Some(selected_session), None, None) => {
                    if self.session_ui_infos.len() > selected_session + 1 {
                        self.selected_index.0 = Some(selected_session + 1);
                    } else {
                        self.selected_index.0 = None;
                        self.selected_index.1 = None;
                        self.selected_index.2 = None;
                    }
                },
                SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
                    if self
                        .get_session(selected_session)
                        .map(|s| s.tabs.len() > selected_tab + 1)
                        .unwrap_or(false)
                    {
                        self.selected_index.1 = Some(selected_tab + 1);
                    } else {
                        self.selected_index.1 = Some(0);
                    }
                },
                SelectedIndex(Some(selected_session), Some(selected_tab), Some(selected_pane)) => {
                    if self
                        .get_session(selected_session)
                        .and_then(|s| s.tabs.get(selected_tab))
                        .map(|t| t.panes.len() > selected_pane + 1)
                        .unwrap_or(false)
                    {
                        self.selected_index.2 = Some(selected_pane + 1);
                    } else {
                        self.selected_index.2 = Some(0);
                    }
                },
                _ => {},
            }
        }
    }
    pub fn move_selection_up(&mut self) {
        if self.is_searching {
            match self.selected_search_index.as_mut() {
                Some(search_index) => {
                    *search_index = search_index.saturating_sub(1);
                },
                None => {
                    if !self.search_results.is_empty() {
                        self.selected_search_index = Some(0);
                    }
                },
            }
        } else {
            match self.selected_index {
                SelectedIndex(None, None, None) => {
                    if !self.session_ui_infos.is_empty() {
                        self.selected_index.0 = Some(self.session_ui_infos.len().saturating_sub(1))
                    }
                },
                SelectedIndex(Some(selected_session), None, None) => {
                    if selected_session > 0 {
                        self.selected_index.0 = Some(selected_session - 1);
                    } else {
                        self.selected_index.0 = None;
                    }
                },
                SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
                    if selected_tab > 0 {
                        self.selected_index.1 = Some(selected_tab - 1);
                    } else {
                        let tab_count = self
                            .get_session(selected_session)
                            .map(|s| s.tabs.len())
                            .unwrap_or(0);
                        self.selected_index.1 = Some(tab_count.saturating_sub(1))
                    }
                },
                SelectedIndex(Some(selected_session), Some(selected_tab), Some(selected_pane)) => {
                    if selected_pane > 0 {
                        self.selected_index.2 = Some(selected_pane - 1);
                    } else {
                        let pane_count = self
                            .get_session(selected_session)
                            .and_then(|s| s.tabs.get(selected_tab))
                            .map(|t| t.panes.len())
                            .unwrap_or(0);
                        self.selected_index.2 = Some(pane_count.saturating_sub(1))
                    }
                },
                _ => {},
            }
        }
    }
    fn get_session(&self, index: usize) -> Option<&SessionUiInfo> {
        self.session_ui_infos.get(index)
    }
    pub fn result_expand(&mut self) {
        // we can't move this to SelectedIndex because the borrow checker is mean
        match self.selected_index {
            SelectedIndex(Some(selected_session), None, None) => {
                let selected_session_has_tabs = self
                    .get_session(selected_session)
                    .map(|s| !s.tabs.is_empty())
                    .unwrap_or(false);
                if selected_session_has_tabs {
                    self.selected_index.1 = Some(0);
                }
            },
            SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
                let selected_tab_has_panes = self
                    .get_session(selected_session)
                    .and_then(|s| s.tabs.get(selected_tab))
                    .map(|t| !t.panes.is_empty())
                    .unwrap_or(false);
                if selected_tab_has_panes {
                    self.selected_index.2 = Some(0);
                }
            },
            _ => {},
        }
    }
    pub fn result_shrink(&mut self) {
        self.selected_index.result_shrink();
    }
    pub fn update_rows(&mut self, rows: usize) {
        if let Some(search_result_rows_until_selected) = self.selected_search_index.map(|i| {
            self.search_results
                .iter()
                .enumerate()
                .take(i + 1)
                .fold(0, |acc, s| acc + s.1.lines_to_render())
        }) {
            if search_result_rows_until_selected > rows
                || self.selected_search_index >= Some(self.search_results.len())
            {
                self.selected_search_index = None;
            }
        }
    }
    pub fn reset_selected_index(&mut self) {
        self.selected_index.reset();
    }
    pub fn has_session(&self, session_name: &str) -> bool {
        self.session_ui_infos.iter().any(|s| s.name == session_name)
    }
    pub fn has_forbidden_session(&self, session_name: &str) -> bool {
        self.forbidden_sessions
            .iter()
            .any(|s| s.name == session_name)
    }
    pub fn update_session_name(&mut self, old_name: &str, new_name: &str) {
        self.session_ui_infos
            .iter_mut()
            .find(|s| s.name == old_name)
            .map(|s| s.name = new_name.to_owned());
    }
    pub fn all_other_sessions(&self) -> Vec<String> {
        self.session_ui_infos
            .iter()
            .filter_map(|s| {
                if !s.is_current_session {
                    Some(s.name.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SelectedIndex(pub Option<usize>, pub Option<usize>, pub Option<usize>);

impl SelectedIndex {
    pub fn tabs_are_visible(&self) -> bool {
        self.1.is_some()
    }
    pub fn panes_are_visible(&self) -> bool {
        self.2.is_some()
    }
    pub fn selected_tab_index(&self) -> Option<usize> {
        self.1
    }
    pub fn session_index_is_selected(&self, index: usize) -> bool {
        self.0 == Some(index)
    }
    pub fn result_shrink(&mut self) {
        match self {
            SelectedIndex(Some(_selected_session), None, None) => self.0 = None,
            SelectedIndex(Some(_selected_session), Some(_selected_tab), None) => self.1 = None,
            SelectedIndex(Some(_selected_session), Some(_selected_tab), Some(_selected_pane)) => {
                self.2 = None
            },
            _ => {},
        }
    }
    pub fn reset(&mut self) {
        self.0 = None;
        self.1 = None;
        self.2 = None;
    }
}

#[derive(Debug)]
pub struct SearchResult {
    score: i64,
    indices: Vec<usize>,
    list_item: ListItem,
    session_name: String,
    tab_position: Option<usize>,
    pane_id: Option<(u32, bool)>,
    is_current_session: bool,
}

impl SearchResult {
    pub fn new(
        score: i64,
        indices: Vec<usize>,
        list_item: ListItem,
        session_name: String,
        tab_position: Option<usize>,
        pane_id: Option<(u32, bool)>,
        is_current_session: bool,
    ) -> Self {
        SearchResult {
            score,
            indices,
            list_item,
            session_name,
            tab_position,
            pane_id,
            is_current_session,
        }
    }
    pub fn lines_to_render(&self) -> usize {
        self.list_item.line_count()
    }
    pub fn render(&self, max_width: usize) -> Vec<LineToRender> {
        self.list_item.render(Some(self.indices.clone()), max_width)
    }
}
