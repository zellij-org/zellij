use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use humantime::format_duration;

use crate::ui::{
    components::{Colors, LineToRender, ListItem},
    SessionUiInfo,
};

use std::time::Duration;

use zellij_tile::shim::*;

#[derive(Debug, Default)]
pub struct ResurrectableSessions {
    pub name_and_creation_time: Vec<(String, Duration)>,
    pub selected_index: Option<usize>,
    pub selected_search_index: Option<usize>,
    pub search_results: Vec<SearchResult>,
    pub is_searching: bool,
}


impl ResurrectableSessions {
    pub fn update(&mut self, list: Vec<(String, Duration)>) {
        self.name_and_creation_time = list;
    }
    pub fn render(&self, rows: usize, columns: usize) {
        // title line
        let mut table = Table::new()
            .add_row(vec!["Session Name", "Creation Time"]);
        // calculate first/last line of table to render
        let (first_row_index_to_render, last_row_index_to_render) = if rows <= self.name_and_creation_time.len() {
            let row_count_to_render = rows.saturating_sub(2); // 1 for the title, one for the more
            let first_row_index_to_render = self.selected_index.unwrap_or(0).saturating_sub(row_count_to_render / 2);
            let mut last_row_index_to_render = first_row_index_to_render + row_count_to_render;
            if first_row_index_to_render > 0 && last_row_index_to_render > first_row_index_to_render + 1 {
                last_row_index_to_render -= 1;
            }
            (first_row_index_to_render, last_row_index_to_render)
        } else {
            let first_row_index_to_render = 0;
            let last_row_index_to_render = self.name_and_creation_time.len();
            (first_row_index_to_render, last_row_index_to_render)
        };

        // above more indication
        if first_row_index_to_render > 0 {
            table = table.add_styled_row(vec![Text::new(format!("+ {} more", first_row_index_to_render)).color_range(1, ..), Text::new(" ")]);
        }

        // table lines
        for i in first_row_index_to_render..last_row_index_to_render {
            if let Some((name, creation_time)) = self.name_and_creation_time.get(i) {
                if Some(i) == self.selected_index {
                    table = table.add_styled_row(vec![Text::new(name).color_range(0, ..).selected(), Text::new(format_duration(creation_time.clone()).to_string()).selected()]);
                } else {
                    table = table.add_styled_row(vec![Text::new(name).color_range(0, ..), Text::new(format_duration(creation_time.clone()).to_string())]);
                }
            }
        }

        // below more indication
        let remaining_session_count_below = self.name_and_creation_time.len().saturating_sub(last_row_index_to_render);
        if remaining_session_count_below > 0 {
            table = table.add_styled_row(vec![Text::new(format!("+ {} more", remaining_session_count_below)).color_range(1, ..), Text::new(" ")]);
        }
        print_table_with_coordinates(table, 0, 0, Some(columns), Some(rows));
    }
    pub fn move_selection_down(&mut self) {
        if let Some(selected_index) = self.selected_index.as_mut() {
            if *selected_index == self.name_and_creation_time.len().saturating_sub(1) {
                *selected_index = 0;
            } else {
                *selected_index = *selected_index + 1;
            }
        } else {
            self.selected_index = Some(0);
        }
    }
    pub fn move_selection_up(&mut self) {
        if let Some(selected_index) = self.selected_index.as_mut() {
            if *selected_index == 0 {
                *selected_index = self.name_and_creation_time.len().saturating_sub(1);
            } else {
                *selected_index = selected_index.saturating_sub(1);
            }
        } else {
            self.selected_index = Some(self.name_and_creation_time.len().saturating_sub(1));
        }
    }
    pub fn get_selected_session_name(&self) -> Option<String> {
        self.selected_index
            .and_then(|i| self.name_and_creation_time.get(i))
            .map(|session_name_and_creation_time| session_name_and_creation_time.0.clone())
    }
    // TODO: CONTINUE HERE - implement these
    // * when deleting a single session, if the selected is the last session, saturating_sub it by
    // 1, if it's the only session, remove the selected - DONE
    // * do an optimistic update in delete_all_sessions - DONE
    // * truncate the results to our rows when rendering - DONE
    // * refactor the render function above - DONE ish
    // * do a fuzzy search
    // * do an "are you sure?" screen when deleting all sessions
    // * indicate the keys
    // * indicate the TAB with ribbons (between live and dead sessions)
    pub fn delete_selected_session(&mut self) {
        self.selected_index
            .and_then(|i| {
                if self.name_and_creation_time.len() > i {
                    // optimistic update
                    if i == 0 {
                        self.selected_index = None;
                    } else if i == self.name_and_creation_time.len().saturating_sub(1) {
                        self.selected_index = Some(i.saturating_sub(1));
                    }
                    Some(self.name_and_creation_time.remove(i))
                } else {
                    None
                }
            })
            .map(|session_name_and_creation_time| delete_dead_session(&session_name_and_creation_time.0));
    }
    pub fn delete_all_sessions(&mut self) {
        // optimistic update
        self.name_and_creation_time = vec![];
        delete_all_dead_sessions();
    }
//     pub fn update_search_term(&mut self, search_term: &str, colors: &Colors) {
//         let mut flattened_assets = self.flatten_assets(colors);
//         let mut matches = vec![];
//         let matcher = SkimMatcherV2::default().use_cache(true);
//         for (list_item, session_name, tab_position, pane_id, is_current_session) in
//             flattened_assets.drain(..)
//         {
//             if let Some((score, indices)) = matcher.fuzzy_indices(&list_item.name, &search_term) {
//                 matches.push(SearchResult::new(
//                     score,
//                     indices,
//                     list_item,
//                     session_name,
//                     tab_position,
//                     pane_id,
//                     is_current_session,
//                 ));
//             }
//         }
//         matches.sort_by(|a, b| b.score.cmp(&a.score));
//         self.search_results = matches;
//         self.is_searching = !search_term.is_empty();
//         self.selected_search_index = Some(0);
//     }
//     fn flatten_assets(
//         &self,
//         colors: &Colors,
//     ) -> Vec<(ListItem, String, Option<usize>, Option<(u32, bool)>, bool)> {
//         // list_item, session_name, tab_position, (pane_id, is_plugin), is_current_session
//         let mut list_items = vec![];
//         for session in &self.session_ui_infos {
//             let session_name = session.name.clone();
//             let is_current_session = session.is_current_session;
//             list_items.push((
//                 ListItem::from_session_info(session, *colors),
//                 session_name.clone(),
//                 None,
//                 None,
//                 is_current_session,
//             ));
//             for tab in &session.tabs {
//                 let tab_position = tab.position;
//                 list_items.push((
//                     ListItem::from_tab_info(session, tab, *colors),
//                     session_name.clone(),
//                     Some(tab_position),
//                     None,
//                     is_current_session,
//                 ));
//                 for pane in &tab.panes {
//                     let pane_id = (pane.pane_id, pane.is_plugin);
//                     list_items.push((
//                         ListItem::from_pane_info(session, tab, pane, *colors),
//                         session_name.clone(),
//                         Some(tab_position),
//                         Some(pane_id),
//                         is_current_session,
//                     ));
//                 }
//             }
//         }
//         list_items
//     }
//     pub fn get_selected_session_name(&self) -> Option<String> {
//         if self.is_searching {
//             self.selected_search_index
//                 .and_then(|i| self.search_results.get(i))
//                 .map(|s| s.session_name.clone())
//         } else {
//             self.selected_index
//                 .0
//                 .and_then(|i| self.session_ui_infos.get(i))
//                 .map(|s_i| s_i.name.clone())
//         }
//     }
//     pub fn selected_is_current_session(&self) -> bool {
//         if self.is_searching {
//             self.selected_search_index
//                 .and_then(|i| self.search_results.get(i))
//                 .map(|s| s.is_current_session)
//                 .unwrap_or(false)
//         } else {
//             self.selected_index
//                 .0
//                 .and_then(|i| self.session_ui_infos.get(i))
//                 .map(|s_i| s_i.is_current_session)
//                 .unwrap_or(false)
//         }
//     }
//     pub fn get_selected_tab_position(&self) -> Option<usize> {
//         if self.is_searching {
//             self.selected_search_index
//                 .and_then(|i| self.search_results.get(i))
//                 .and_then(|s| s.tab_position)
//         } else {
//             self.selected_index
//                 .0
//                 .and_then(|i| self.session_ui_infos.get(i))
//                 .and_then(|s_i| {
//                     self.selected_index
//                         .1
//                         .and_then(|i| s_i.tabs.get(i))
//                         .map(|t| t.position)
//                 })
//         }
//     }
//     pub fn get_selected_pane_id(&self) -> Option<(u32, bool)> {
//         // (pane_id, is_plugin)
//         if self.is_searching {
//             self.selected_search_index
//                 .and_then(|i| self.search_results.get(i))
//                 .and_then(|s| s.pane_id)
//         } else {
//             self.selected_index
//                 .0
//                 .and_then(|i| self.session_ui_infos.get(i))
//                 .and_then(|s_i| {
//                     self.selected_index
//                         .1
//                         .and_then(|i| s_i.tabs.get(i))
//                         .and_then(|t| {
//                             self.selected_index
//                                 .2
//                                 .and_then(|i| t.panes.get(i))
//                                 .map(|p| (p.pane_id, p.is_plugin))
//                         })
//                 })
//         }
//     }
//     pub fn move_selection_down(&mut self) {
//         if self.is_searching {
//             match self.selected_search_index.as_mut() {
//                 Some(search_index) => {
//                     *search_index = search_index.saturating_add(1);
//                 },
//                 None => {
//                     if !self.search_results.is_empty() {
//                         self.selected_search_index = Some(0);
//                     }
//                 },
//             }
//         } else {
//             match self.selected_index {
//                 SelectedIndex(None, None, None) => {
//                     if !self.session_ui_infos.is_empty() {
//                         self.selected_index.0 = Some(0);
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), None, None) => {
//                     if self.session_ui_infos.len() > selected_session + 1 {
//                         self.selected_index.0 = Some(selected_session + 1);
//                     } else {
//                         self.selected_index.0 = None;
//                         self.selected_index.1 = None;
//                         self.selected_index.2 = None;
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
//                     if self
//                         .get_session(selected_session)
//                         .map(|s| s.tabs.len() > selected_tab + 1)
//                         .unwrap_or(false)
//                     {
//                         self.selected_index.1 = Some(selected_tab + 1);
//                     } else {
//                         self.selected_index.1 = Some(0);
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), Some(selected_tab), Some(selected_pane)) => {
//                     if self
//                         .get_session(selected_session)
//                         .and_then(|s| s.tabs.get(selected_tab))
//                         .map(|t| t.panes.len() > selected_pane + 1)
//                         .unwrap_or(false)
//                     {
//                         self.selected_index.2 = Some(selected_pane + 1);
//                     } else {
//                         self.selected_index.2 = Some(0);
//                     }
//                 },
//                 _ => {},
//             }
//         }
//     }
//     pub fn move_selection_up(&mut self) {
//         if self.is_searching {
//             match self.selected_search_index.as_mut() {
//                 Some(search_index) => {
//                     *search_index = search_index.saturating_sub(1);
//                 },
//                 None => {
//                     if !self.search_results.is_empty() {
//                         self.selected_search_index = Some(0);
//                     }
//                 },
//             }
//         } else {
//             match self.selected_index {
//                 SelectedIndex(None, None, None) => {
//                     if !self.session_ui_infos.is_empty() {
//                         self.selected_index.0 = Some(self.session_ui_infos.len().saturating_sub(1))
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), None, None) => {
//                     if selected_session > 0 {
//                         self.selected_index.0 = Some(selected_session - 1);
//                     } else {
//                         self.selected_index.0 = None;
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
//                     if selected_tab > 0 {
//                         self.selected_index.1 = Some(selected_tab - 1);
//                     } else {
//                         let tab_count = self
//                             .get_session(selected_session)
//                             .map(|s| s.tabs.len())
//                             .unwrap_or(0);
//                         self.selected_index.1 = Some(tab_count.saturating_sub(1))
//                     }
//                 },
//                 SelectedIndex(Some(selected_session), Some(selected_tab), Some(selected_pane)) => {
//                     if selected_pane > 0 {
//                         self.selected_index.2 = Some(selected_pane - 1);
//                     } else {
//                         let pane_count = self
//                             .get_session(selected_session)
//                             .and_then(|s| s.tabs.get(selected_tab))
//                             .map(|t| t.panes.len())
//                             .unwrap_or(0);
//                         self.selected_index.2 = Some(pane_count.saturating_sub(1))
//                     }
//                 },
//                 _ => {},
//             }
//         }
//     }
//     fn get_session(&self, index: usize) -> Option<&SessionUiInfo> {
//         self.session_ui_infos.get(index)
//     }
//     pub fn result_expand(&mut self) {
//         // we can't move this to SelectedIndex because the borrow checker is mean
//         match self.selected_index {
//             SelectedIndex(Some(selected_session), None, None) => {
//                 let selected_session_has_tabs = self
//                     .get_session(selected_session)
//                     .map(|s| !s.tabs.is_empty())
//                     .unwrap_or(false);
//                 if selected_session_has_tabs {
//                     self.selected_index.1 = Some(0);
//                 }
//             },
//             SelectedIndex(Some(selected_session), Some(selected_tab), None) => {
//                 let selected_tab_has_panes = self
//                     .get_session(selected_session)
//                     .and_then(|s| s.tabs.get(selected_tab))
//                     .map(|t| !t.panes.is_empty())
//                     .unwrap_or(false);
//                 if selected_tab_has_panes {
//                     self.selected_index.2 = Some(0);
//                 }
//             },
//             _ => {},
//         }
//     }
//     pub fn result_shrink(&mut self) {
//         self.selected_index.result_shrink();
//     }
//     pub fn update_rows(&mut self, rows: usize) {
//         if let Some(search_result_rows_until_selected) = self.selected_search_index.map(|i| {
//             self.search_results
//                 .iter()
//                 .enumerate()
//                 .take(i + 1)
//                 .fold(0, |acc, s| acc + s.1.lines_to_render())
//         }) {
//             if search_result_rows_until_selected > rows
//                 || self.selected_search_index >= Some(self.search_results.len())
//             {
//                 self.selected_search_index = None;
//             }
//         }
//     }
//     pub fn reset_selected_index(&mut self) {
//         self.selected_index.reset();
//     }
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
