use crate::{ui::PaneItem, App};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use zellij_tile::prelude::*;

#[derive(Debug, Default)]
pub struct MarkedIndex {
    pub main_index: usize,
    pub additional_indices: HashSet<usize>,
}

impl MarkedIndex {
    pub fn new(main_index: usize) -> Self {
        MarkedIndex {
            main_index,
            additional_indices: HashSet::new(),
        }
    }
}

impl MarkedIndex {
    pub fn toggle_additional_mark(&mut self) {
        if self.additional_indices.contains(&self.main_index) {
            self.additional_indices.retain(|a| a != &self.main_index);
        } else {
            self.additional_indices.insert(self.main_index);
        }
    }
}

#[derive(Debug)]
pub enum VisibilityAndFocus {
    OnlyLeftSideVisible,
    OnlyRightSideVisible,
    BothSidesVisibleLeftSideFocused,
    BothSidesVisibleRightSideFocused,
}

impl Default for VisibilityAndFocus {
    fn default() -> Self {
        VisibilityAndFocus::OnlyLeftSideVisible
    }
}

impl VisibilityAndFocus {
    pub fn only_left_side_is_focused(&self) -> bool {
        match self {
            VisibilityAndFocus::OnlyLeftSideVisible => true,
            _ => false,
        }
    }
    pub fn left_side_is_focused(&self) -> bool {
        match self {
            VisibilityAndFocus::OnlyLeftSideVisible
            | VisibilityAndFocus::BothSidesVisibleLeftSideFocused => true,
            _ => false,
        }
    }
    pub fn right_side_is_focused(&self) -> bool {
        match self {
            VisibilityAndFocus::OnlyRightSideVisible
            | VisibilityAndFocus::BothSidesVisibleRightSideFocused => true,
            _ => false,
        }
    }
    pub fn hide_left_side(&mut self) {
        *self = VisibilityAndFocus::OnlyRightSideVisible
    }
    pub fn hide_right_side(&mut self) {
        *self = VisibilityAndFocus::OnlyLeftSideVisible
    }
    pub fn focus_right_side(&mut self) {
        *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
    }
    pub fn toggle_focus(&mut self) {
        match self {
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused => {
                *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
            },
            VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            },
            VisibilityAndFocus::OnlyLeftSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
            },
            VisibilityAndFocus::OnlyRightSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            },
        }
    }
    pub fn show_both_sides(&mut self) {
        match self {
            VisibilityAndFocus::OnlyLeftSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            },
            VisibilityAndFocus::OnlyRightSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
            },
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            | VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                // no-op
            },
        }
    }
}

impl App {
    pub fn react_to_zellij_state_update(&mut self, pane_manifest: PaneManifest) {
        let is_first_update = self.right_side_panes.is_empty() && self.left_side_panes.is_empty();
        let panes_on_the_left_before = self.left_side_panes.len();
        let panes_on_the_right_before = self.right_side_panes.len();
        self.update_tab_info(&pane_manifest);
        self.update_panes(pane_manifest);
        if is_first_update && !self.right_side_panes.is_empty() {
            // in this case, the plugin was started with an existing group
            // most likely, the user wants to perform operations just on this group, so we
            // only show the group, giving the option to add more panes
            self.visibility_and_focus.hide_left_side();
        }
        let pane_count_changed = (panes_on_the_left_before != self.left_side_panes.len())
            || (panes_on_the_right_before != self.right_side_panes.len());
        if !is_first_update && pane_count_changed {
            let has_panes_on_the_right = !self.right_side_panes.is_empty();
            let has_panes_on_the_left = !self.left_side_panes.is_empty();
            if has_panes_on_the_right && has_panes_on_the_left {
                self.visibility_and_focus.show_both_sides();
            } else if has_panes_on_the_right {
                self.visibility_and_focus.hide_left_side();
            } else if has_panes_on_the_left {
                self.visibility_and_focus.hide_right_side();
            }
        }
    }
    pub fn update_panes(&mut self, pane_manifest: PaneManifest) {
        let mut all_panes = BTreeMap::new();
        for (_tab_index, pane_infos) in pane_manifest.panes {
            for pane_info in pane_infos {
                if pane_info.is_selectable {
                    if pane_info.is_plugin {
                        all_panes.insert(PaneId::Plugin(pane_info.id), pane_info);
                    } else {
                        all_panes.insert(PaneId::Terminal(pane_info.id), pane_info);
                    }
                }
            }
        }
        self.left_side_panes
            .retain(|p| all_panes.contains_key(&p.id));
        self.right_side_panes
            .retain(|p| all_panes.contains_key(&p.id));
        let mut new_selected_panes: BTreeMap<usize, PaneItem> = BTreeMap::new(); // usize -> index_in_pane_group
        for (pane_id, pane) in all_panes.into_iter() {
            let is_known = self
                .left_side_panes
                .iter()
                .find(|p| p.id == pane_id)
                .is_some()
                || self
                    .right_side_panes
                    .iter()
                    .find(|p| p.id == pane_id)
                    .is_some();
            let index_in_pane_group = self
                .own_client_id
                .and_then(|own_client_id| pane.index_in_pane_group.get(&own_client_id));
            let is_grouped_for_own_client_id = index_in_pane_group.is_some();
            if !is_known {
                if is_grouped_for_own_client_id {
                    if let Some(index_in_pane_group) = index_in_pane_group {
                        // we do this rather than adding them directly to right_side_panes so that
                        // we can make sure they're in the same order as the group is so that
                        // things like stacking order will do the right thing
                        new_selected_panes.insert(
                            *index_in_pane_group,
                            PaneItem {
                                text: pane.title,
                                id: pane_id,
                                color_indices: vec![],
                            },
                        );
                    }
                } else {
                    self.left_side_panes.push(PaneItem {
                        text: pane.title,
                        id: pane_id,
                        color_indices: vec![],
                    });
                }
            } else {
                if is_grouped_for_own_client_id {
                    if let Some(position) =
                        self.left_side_panes.iter().position(|p| p.id == pane_id)
                    {
                        // pane was added to a pane group outside the plugin (eg. with mouse selection)
                        let mut pane = self.left_side_panes.remove(position);
                        pane.clear();
                        self.right_side_panes.push(pane);
                    }
                } else {
                    if let Some(position) =
                        self.right_side_panes.iter().position(|p| p.id == pane_id)
                    {
                        // pane was removed from a pane group outside the plugin (eg. with mouse selection)
                        let mut pane = self.right_side_panes.remove(position);
                        pane.clear();
                        self.left_side_panes.push(pane);
                    }
                }
            }
        }
        for (_index_in_pane_group, pane_item) in new_selected_panes.into_iter() {
            self.right_side_panes.push(pane_item);
        }
    }
    pub fn update_tab_info(&mut self, pane_manifest: &PaneManifest) {
        for (tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                if pane_info.is_plugin && Some(pane_info.id) == self.own_plugin_id {
                    self.own_tab_index = Some(*tab_index);
                }
            }
        }
        self.total_tabs_in_session = Some(pane_manifest.panes.keys().count());
    }
    pub fn update_search_results(&mut self) {
        let mut matches = vec![];
        let matcher = SkimMatcherV2::default().use_cache(true);
        for pane_item in &self.left_side_panes {
            if let Some((score, indices)) =
                matcher.fuzzy_indices(&pane_item.text, &self.search_string)
            {
                let mut pane_item = pane_item.clone();
                pane_item.color_indices = indices;
                matches.push((score, pane_item));
            }
        }
        matches.sort_by(|(a_score, _a), (b_score, _b)| b_score.cmp(&a_score));
        if self.search_string.is_empty() {
            self.search_results = None;
        } else {
            self.search_results = Some(
                matches
                    .into_iter()
                    .map(|(_s, pane_item)| pane_item)
                    .collect(),
            );
        }
    }
    pub fn group_panes_in_zellij(&mut self, pane_ids: Vec<PaneId>) {
        group_and_ungroup_panes(pane_ids, vec![]);
    }
    pub fn ungroup_panes_in_zellij(&mut self, pane_ids: Vec<PaneId>) {
        group_and_ungroup_panes(vec![], pane_ids);
    }
    pub fn update_highlighted_panes(&self) {
        let mut pane_ids_to_highlight = vec![];
        let mut pane_ids_to_unhighlight = vec![];
        if let Some(marked_index) = &self.marked_index {
            if self.visibility_and_focus.left_side_is_focused() {
                if let Some(main_index_pane_id) = self
                    .search_results
                    .as_ref()
                    .and_then(|s| s.get(marked_index.main_index))
                    .or_else(|| self.left_side_panes.get(marked_index.main_index))
                    .map(|p| p.id)
                {
                    pane_ids_to_highlight.push(main_index_pane_id);
                }
                for index in &marked_index.additional_indices {
                    if let Some(pane_id) = self
                        .search_results
                        .as_ref()
                        .and_then(|s| s.get(*index))
                        .or_else(|| self.left_side_panes.get(*index))
                        .map(|p| p.id)
                    {
                        pane_ids_to_highlight.push(pane_id);
                    }
                }
            } else {
                if let Some(main_index_pane_id) = self
                    .right_side_panes
                    .get(marked_index.main_index)
                    .map(|p| p.id)
                {
                    pane_ids_to_highlight.push(main_index_pane_id);
                }
                for index in &marked_index.additional_indices {
                    if let Some(pane_id) = self.right_side_panes.get(*index).map(|p| p.id) {
                        pane_ids_to_highlight.push(pane_id);
                    }
                }
            }
        }
        for pane in &self.left_side_panes {
            if !pane_ids_to_highlight.contains(&pane.id) {
                pane_ids_to_unhighlight.push(pane.id);
            }
        }
        for pane in &self.right_side_panes {
            if !pane_ids_to_highlight.contains(&pane.id) {
                pane_ids_to_unhighlight.push(pane.id);
            }
        }
        highlight_and_unhighlight_panes(pane_ids_to_highlight, pane_ids_to_unhighlight);
    }
    pub fn unhighlight_all_panes(&mut self) {
        let mut pane_ids_to_unhighlight = HashSet::new();
        for pane_item in &self.left_side_panes {
            pane_ids_to_unhighlight.insert(pane_item.id);
        }
        for pane_item in &self.right_side_panes {
            pane_ids_to_unhighlight.insert(pane_item.id);
        }
        highlight_and_unhighlight_panes(vec![], pane_ids_to_unhighlight.into_iter().collect());
    }
    pub fn ungroup_all_panes(&mut self) {
        let mut unselected_panes = vec![];
        for pane_item in self.right_side_panes.iter_mut() {
            pane_item.clear();
            unselected_panes.push(pane_item.id);
        }
        self.left_side_panes.append(&mut self.right_side_panes);
        self.ungroup_panes_in_zellij(unselected_panes);
        self.visibility_and_focus.hide_right_side();
        self.marked_index = None;
    }
    pub fn ungroup_all_panes_and_close_self(&mut self) {
        let mut pane_ids_to_ungroup = HashSet::new();
        for pane_item in &self.left_side_panes {
            pane_ids_to_ungroup.insert(pane_item.id);
        }
        for pane_item in &self.right_side_panes {
            pane_ids_to_ungroup.insert(pane_item.id);
        }
        group_and_ungroup_panes(vec![], pane_ids_to_ungroup.into_iter().collect());
        close_self();
    }
    pub fn group_panes(&mut self, mut marked_index: MarkedIndex, keep_left_side_focused: bool) {
        let mut all_selected_indices: BTreeSet<usize> =
            marked_index.additional_indices.drain().collect();
        all_selected_indices.insert(marked_index.main_index);

        // reverse so that the indices will remain consistent while
        // removing
        let mut selected_panes = vec![];
        for index in all_selected_indices.iter().rev() {
            let index = self
                .search_results
                .as_mut()
                .and_then(|search_results| {
                    if search_results.len() > *index {
                        Some(search_results.remove(*index))
                    } else {
                        None
                    }
                })
                .and_then(|selected_search_result| {
                    self.left_side_panes
                        .iter()
                        .position(|p| p.id == selected_search_result.id)
                })
                .unwrap_or(*index);
            if self.left_side_panes.len() > index {
                let selected_pane = self.left_side_panes.remove(index);
                selected_panes.push(selected_pane);
            }
        }
        let pane_ids_to_make_selected: Vec<PaneId> = selected_panes.iter().map(|p| p.id).collect();
        self.right_side_panes
            .append(&mut selected_panes.into_iter().rev().collect());

        let displayed_list_len = match self.search_results.as_ref() {
            Some(search_results) => search_results.len(),
            None => self.left_side_panes.len(),
        };

        if displayed_list_len == 0 {
            self.handle_left_side_emptied();
        } else if keep_left_side_focused {
            if marked_index.main_index > displayed_list_len.saturating_sub(1) {
                self.marked_index = Some(MarkedIndex::new(displayed_list_len.saturating_sub(1)));
            } else {
                self.marked_index = Some(marked_index);
            }
            self.visibility_and_focus.show_both_sides();
        } else {
            self.visibility_and_focus.focus_right_side();
        }

        self.group_panes_in_zellij(pane_ids_to_make_selected);
        self.update_highlighted_panes();
    }
    pub fn ungroup_panes(&mut self, mut marked_index: MarkedIndex) {
        let mut all_selected_indices: BTreeSet<usize> =
            marked_index.additional_indices.drain().collect();
        all_selected_indices.insert(marked_index.main_index);

        // reverse so that the indices will remain consistent while
        // removing
        let mut selected_panes = vec![];
        for index in all_selected_indices.iter().rev() {
            if self.right_side_panes.len() > *index {
                let mut selected_pane = self.right_side_panes.remove(*index);
                selected_pane.clear();
                selected_panes.push(selected_pane);
            }
        }
        self.ungroup_panes_in_zellij(selected_panes.iter().map(|p| p.id).collect());
        self.left_side_panes
            .append(&mut selected_panes.into_iter().rev().collect());

        if self.right_side_panes.is_empty() {
            self.marked_index = None;
            self.visibility_and_focus.hide_right_side();
        } else if marked_index.main_index > self.right_side_panes.len().saturating_sub(1) {
            self.marked_index = Some(MarkedIndex::new(
                self.right_side_panes.len().saturating_sub(1),
            ));
            self.visibility_and_focus.show_both_sides();
        } else {
            self.marked_index = Some(marked_index);
            self.visibility_and_focus.show_both_sides();
        }
        self.update_highlighted_panes();
    }
    pub fn group_search_results(&mut self, search_results: Vec<PaneItem>) {
        let mut pane_ids_to_make_selected = vec![];
        for search_result in search_results {
            let pane_id = search_result.id;
            pane_ids_to_make_selected.push(pane_id);
            self.left_side_panes.retain(|p| p.id != pane_id);
            self.right_side_panes.push(search_result);
        }
        self.group_panes_in_zellij(pane_ids_to_make_selected);
    }
    pub fn group_all_panes(&mut self) {
        let pane_ids_to_make_selected: Vec<PaneId> =
            self.left_side_panes.iter().map(|p| p.id).collect();
        self.right_side_panes.append(&mut self.left_side_panes);
        self.group_panes_in_zellij(pane_ids_to_make_selected);
    }
    pub fn handle_left_side_emptied(&mut self) {
        self.visibility_and_focus.hide_left_side();
        self.previous_search_string = self.search_string.drain(..).collect();
        self.marked_index = None;
        self.search_results = None;
        self.update_highlighted_panes();
    }
    pub fn move_marked_index_down(&mut self) {
        match self.marked_index.as_mut() {
            Some(marked_index) => {
                let is_searching = self.search_results.is_some();
                let search_result_count =
                    self.search_results.as_ref().map(|s| s.len()).unwrap_or(0);
                if self.visibility_and_focus.left_side_is_focused()
                    && is_searching
                    && marked_index.main_index == search_result_count.saturating_sub(1)
                {
                    marked_index.main_index = 0;
                } else if self.visibility_and_focus.left_side_is_focused()
                    && !is_searching
                    && marked_index.main_index == self.left_side_panes.len().saturating_sub(1)
                {
                    marked_index.main_index = 0;
                } else if self.visibility_and_focus.right_side_is_focused()
                    && marked_index.main_index == self.right_side_panes.len().saturating_sub(1)
                {
                    marked_index.main_index = 0;
                } else {
                    marked_index.main_index += 1
                }
            },
            None => {
                if self.visibility_and_focus.left_side_is_focused() {
                    let is_searching = self.search_results.is_some();
                    let has_search_results = self
                        .search_results
                        .as_ref()
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    if is_searching && has_search_results {
                        self.marked_index = Some(MarkedIndex::new(0));
                    } else if !is_searching && !self.left_side_panes.is_empty() {
                        self.marked_index = Some(MarkedIndex::new(0));
                    }
                } else if self.visibility_and_focus.right_side_is_focused()
                    && !self.right_side_panes.is_empty()
                {
                    self.marked_index = Some(MarkedIndex::new(0));
                }
            },
        }
        self.update_highlighted_panes();
    }
    pub fn move_marked_index_up(&mut self) {
        match self.marked_index.as_mut() {
            Some(marked_index) => {
                if self.visibility_and_focus.left_side_is_focused() && marked_index.main_index == 0
                {
                    if let Some(search_result_count) = self.search_results.as_ref().map(|s| s.len())
                    {
                        marked_index.main_index = search_result_count.saturating_sub(1);
                    } else {
                        marked_index.main_index = self.left_side_panes.len().saturating_sub(1);
                    }
                } else if self.visibility_and_focus.right_side_is_focused()
                    && marked_index.main_index == 0
                {
                    marked_index.main_index = self.right_side_panes.len().saturating_sub(1);
                } else {
                    marked_index.main_index = marked_index.main_index.saturating_sub(1);
                }
            },
            None => {
                if self.visibility_and_focus.left_side_is_focused() {
                    let is_searching = self.search_results.is_some();
                    let has_search_results = self
                        .search_results
                        .as_ref()
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                    if is_searching && has_search_results {
                        let search_results_count =
                            self.search_results.as_ref().map(|s| s.len()).unwrap_or(0);
                        self.marked_index =
                            Some(MarkedIndex::new(search_results_count.saturating_sub(1)));
                    } else if !is_searching && !self.left_side_panes.is_empty() {
                        self.marked_index = Some(MarkedIndex::new(
                            self.left_side_panes.len().saturating_sub(1),
                        ));
                    }
                } else if self.visibility_and_focus.right_side_is_focused()
                    && !self.right_side_panes.is_empty()
                {
                    self.marked_index = Some(MarkedIndex::new(
                        self.right_side_panes.len().saturating_sub(1),
                    ));
                }
            },
        }
        self.update_highlighted_panes();
    }
    pub fn mark_entry(&mut self) {
        if let Some(marked_index) = self.marked_index.as_mut() {
            marked_index.toggle_additional_mark();
            self.update_highlighted_panes();
        }
    }
    pub fn break_grouped_panes_to_new_tab(&mut self) {
        let pane_ids_to_break_to_new_tab: Vec<PaneId> =
            self.right_side_panes.drain(..).map(|p| p.id).collect();
        let title_for_new_tab = if !self.previous_search_string.is_empty() {
            Some(self.previous_search_string.clone())
        } else {
            None
        };
        break_panes_to_new_tab(&pane_ids_to_break_to_new_tab, title_for_new_tab, true);
        self.ungroup_panes_in_zellij(pane_ids_to_break_to_new_tab);
        close_self();
    }
    pub fn stack_grouped_panes(&mut self) {
        let pane_ids_to_stack: Vec<PaneId> =
            self.right_side_panes.drain(..).map(|p| p.id).collect();
        stack_panes(pane_ids_to_stack.clone());
        self.ungroup_panes_in_zellij(pane_ids_to_stack);
        close_self();
    }
    pub fn float_grouped_panes(&mut self) {
        let pane_ids_to_float: Vec<PaneId> =
            self.right_side_panes.drain(..).map(|p| p.id).collect();
        float_multiple_panes(pane_ids_to_float.clone());
        self.ungroup_panes_in_zellij(pane_ids_to_float);
        close_self();
    }
    pub fn embed_grouped_panes(&mut self) {
        let pane_ids_to_embed: Vec<PaneId> =
            self.right_side_panes.drain(..).map(|p| p.id).collect();
        embed_multiple_panes(pane_ids_to_embed.clone());
        self.ungroup_panes_in_zellij(pane_ids_to_embed);
        close_self();
    }
    pub fn break_grouped_panes_right(&mut self) {
        if let Some(own_tab_index) = self.own_tab_index {
            if Some(own_tab_index + 1) < self.total_tabs_in_session {
                let pane_ids_to_break_right: Vec<PaneId> =
                    self.right_side_panes.drain(..).map(|p| p.id).collect();
                break_panes_to_tab_with_index(&pane_ids_to_break_right, own_tab_index + 1, true);
            } else {
                let pane_ids_to_break_to_new_tab: Vec<PaneId> =
                    self.right_side_panes.drain(..).map(|p| p.id).collect();
                let title_for_new_tab = if !self.previous_search_string.is_empty() {
                    Some(self.previous_search_string.clone())
                } else {
                    None
                };
                break_panes_to_new_tab(&pane_ids_to_break_to_new_tab, title_for_new_tab, true);
            }
            close_self();
        }
    }
    pub fn break_grouped_panes_left(&mut self) {
        if let Some(own_tab_index) = self.own_tab_index {
            if own_tab_index > 0 {
                let pane_ids_to_break_left: Vec<PaneId> =
                    self.right_side_panes.drain(..).map(|p| p.id).collect();
                break_panes_to_tab_with_index(
                    &pane_ids_to_break_left,
                    own_tab_index.saturating_sub(1),
                    true,
                );
            } else {
                let pane_ids_to_break_to_new_tab: Vec<PaneId> =
                    self.right_side_panes.drain(..).map(|p| p.id).collect();
                let title_for_new_tab = if !self.previous_search_string.is_empty() {
                    Some(self.previous_search_string.clone())
                } else {
                    None
                };
                break_panes_to_new_tab(&pane_ids_to_break_to_new_tab, title_for_new_tab, true);
            }
            close_self();
        }
    }
    pub fn close_grouped_panes(&mut self) {
        let pane_ids_to_close: Vec<PaneId> =
            self.right_side_panes.drain(..).map(|p| p.id).collect();
        close_multiple_panes(pane_ids_to_close);
        close_self();
    }
}
