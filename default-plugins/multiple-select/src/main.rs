use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use zellij_tile::prelude::*;

use std::collections::{BTreeMap, BTreeSet, HashSet};

const TOP_LEFT_CORNER_CHARACTER: &'static str = "┌";
const TOP_RIGHT_CORNER_CHARACTER: &'static str = "┐";
const BOTTOM_LEFT_CORNER_CHARACTER: &'static str = "└";
const BOTTOM_RIGHT_CORNER_CHARACTER: &'static str = "┘";
const BOUNDARY_CHARACTER: &'static str = "│";
const HORIZONTAL_BOUNDARY_CHARACTER: &'static str = "─";

#[derive(Debug, Default)]
struct SelectedIndex {
    pub main_selected: usize,
    pub additional_selected: HashSet<usize>,
}

impl SelectedIndex {
    pub fn new(main_selected: usize) -> Self {
        SelectedIndex {
            main_selected,
            additional_selected: HashSet::new(),
        }
    }
}

impl SelectedIndex {
    pub fn toggle_additional_mark(&mut self) {
        if self.additional_selected.contains(&self.main_selected) {
            self.additional_selected.retain(|a| a != &self.main_selected);
        } else {
            self.additional_selected.insert(self.main_selected);
        }
    }
}

#[derive(Debug, Clone)]
struct PaneItem {
    text: String,
    id: PaneId,
    color_indices: Vec<usize>,
}

impl PaneItem {
    pub fn clear(&mut self) {
        self.color_indices.clear();
    }
    pub fn render(&self, max_width_for_item: usize) -> NestedListItem {
        let pane_item_text_len = self.text.chars().count();
        if pane_item_text_len <= max_width_for_item {
            NestedListItem::new(&self.text)
                .color_range(0, ..)
                .color_indices(3, self.color_indices.iter().copied().collect())
        } else {
            let length_of_each_half = max_width_for_item.saturating_sub(3) / 2;
            let first_half: String = self.text.chars().take(length_of_each_half).collect();
            let second_half: String = self.text.chars().rev().take(length_of_each_half).collect::<Vec<_>>().iter().rev().collect();
            let second_half_start_index = pane_item_text_len.saturating_sub(length_of_each_half);
            let adjusted_indices: Vec<usize> = self.color_indices.iter().filter_map(|i| {
                if i < &length_of_each_half {
                    Some(*i)
                } else if i >= &second_half_start_index {
                    Some(i.saturating_sub(second_half_start_index) + length_of_each_half + 3) //3 for the bulletin
                } else {
                    None
                }
            }).collect();
            NestedListItem::new(format!("{}...{}", first_half, second_half))
                .color_range(0, ..)
                .color_indices(3, adjusted_indices)
        }
    }
}

#[derive(Debug)]
enum VisibilityAndFocus {
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
    pub fn left_side_is_focused(&self) -> bool {
        match self {
            VisibilityAndFocus::OnlyLeftSideVisible | VisibilityAndFocus::BothSidesVisibleLeftSideFocused => true,
            _ => false
        }
    }
    pub fn right_side_is_focused(&self) -> bool {
        match self {
            VisibilityAndFocus::OnlyRightSideVisible | VisibilityAndFocus::BothSidesVisibleRightSideFocused => true,
            _ => false
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
            }
            VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            }
            VisibilityAndFocus::OnlyLeftSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
            }
            VisibilityAndFocus::OnlyRightSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            }
        }
    }
    pub fn show_both_sides(&mut self) {
        match self {
            VisibilityAndFocus::OnlyLeftSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleLeftSideFocused
            }
            VisibilityAndFocus::OnlyRightSideVisible => {
                *self = VisibilityAndFocus::BothSidesVisibleRightSideFocused
            }
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused | VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                // no-op
            }
        }
    }
}

#[derive(Debug, Default)]
struct App {
    own_plugin_id: Option<u32>,
    own_client_id: Option<ClientId>,
    own_tab_index: Option<usize>,
    total_tabs_in_session: Option<usize>,
    search_string: String,
    previous_search_string: String, // used eg. for the new tab title when breaking panes
    left_side_panes: Vec<PaneItem>,
    right_side_panes: Vec<PaneItem>,
    search_results: Option<Vec<PaneItem>>,
    visibility_and_focus: VisibilityAndFocus,
    selected_index: Option<SelectedIndex>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::Key,
            EventType::Mouse,
            EventType::ModeUpdate,
            EventType::RunCommandResult,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigWasWrittenToDisk,
        ]);
        let plugin_ids = get_plugin_ids();
        self.own_plugin_id = Some(plugin_ids.plugin_id);
        self.own_client_id = Some(plugin_ids.client_id);
        rename_plugin_pane(plugin_ids.plugin_id, "Multiple Select");
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::PaneUpdate(pane_manifest) => {
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
                let pane_count_changed = (panes_on_the_left_before != self.left_side_panes.len()) || (panes_on_the_right_before != self.right_side_panes.len());
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
                should_render = true;
            }
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Tab if key.has_no_modifiers() => {
                        self.visibility_and_focus.toggle_focus();
                        self.selected_index = None;
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Char(character) if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() && self.selected_index.is_none() => {
                        self.search_string.push(character);
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Backspace if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() && self.selected_index.is_none() => {
                        self.search_string.pop();
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Enter if key.has_no_modifiers() => {
                        if self.visibility_and_focus.left_side_is_focused() {
                            if let Some(selected_index) = self.selected_index.take() {
                                let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.iter().copied().collect();
                                all_selected_indices.insert(selected_index.main_selected);

                                // reverse so that the indices will remain consistent while
                                // removing
                                let mut selected_panes = vec![];
                                for index in all_selected_indices.iter().rev() {
                                    let index = self.search_results
                                        .as_ref()
                                        .and_then(|search_results| search_results.get(*index).map(|i| i.id))
                                        .and_then(|selected_search_result_id| self.left_side_panes.iter().position(|p| p.id == selected_search_result_id))
                                        .unwrap_or(*index);
                                    if self.left_side_panes.len() > index {
                                        let selected_pane = self.left_side_panes.remove(index);
                                        selected_panes.push(selected_pane);
                                    }
                                }
                                let pane_ids_to_make_selected: Vec<PaneId> = selected_panes.iter().map(|p| p.id).collect();
                                self.right_side_panes.append(&mut selected_panes.into_iter().rev().collect());
                                let emptied_search_results = self.search_results.as_ref().map(|s| s.is_empty()).unwrap_or(false);
                                self.search_results = None;
                                self.previous_search_string = self.search_string.drain(..).collect();

                                if self.left_side_panes.is_empty() || emptied_search_results {
                                    self.selected_index = None;
                                    self.visibility_and_focus.hide_left_side();
                                } else {
                                    self.selected_index = None;
                                    self.visibility_and_focus.focus_right_side();
                                }
                                self.group_panes_in_zellij(pane_ids_to_make_selected);
                                self.update_highlighted_panes();
                            } else {
                                if let Some(search_results) = self.search_results.take() {
                                    let mut pane_ids_to_make_selected = vec![];
                                    for search_result in search_results {
                                        let pane_id = search_result.id;
                                        pane_ids_to_make_selected.push(pane_id);
                                        self.left_side_panes.retain(|p| p.id != pane_id);
                                        self.right_side_panes.push(search_result);
                                    }
                                    self.group_panes_in_zellij(pane_ids_to_make_selected);
                                } else {
                                    let pane_ids_to_make_selected: Vec<PaneId> = self.left_side_panes.iter().map(|p| p.id).collect();
                                    self.right_side_panes.append(&mut self.left_side_panes);
                                    self.group_panes_in_zellij(pane_ids_to_make_selected);
                                }
                                self.visibility_and_focus.hide_left_side();
                                self.previous_search_string = self.search_string.drain(..).collect();
                                self.selected_index = None;
                                self.search_results = None;
                                self.update_highlighted_panes();
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Left if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        if self.visibility_and_focus.right_side_is_focused() {
                            if let Some(mut selected_index) = self.selected_index.take() {
                                let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.iter().copied().collect();
                                all_selected_indices.insert(selected_index.main_selected);

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
                                self.left_side_panes.append(&mut selected_panes.into_iter().rev().collect());

                                if self.right_side_panes.is_empty() {
                                    self.selected_index = None;
                                    self.visibility_and_focus.hide_right_side();
                                } else if selected_index.main_selected > self.right_side_panes.len().saturating_sub(1) {
                                    self.selected_index = Some(SelectedIndex::new(self.right_side_panes.len().saturating_sub(1)));
                                    self.visibility_and_focus.show_both_sides();
                                } else {
                                    selected_index.additional_selected.clear();
                                    self.selected_index = Some(selected_index);
                                    self.visibility_and_focus.show_both_sides();
                                }
                                should_render = true;
                                self.update_highlighted_panes();
                            }
                        }
                    }
                    BareKey::Right if key.has_no_modifiers() && self.visibility_and_focus.left_side_is_focused() => {
                        if let Some(mut selected_index) = self.selected_index.take() {
                            let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.drain().collect();
                            all_selected_indices.insert(selected_index.main_selected);

                            // reverse so that the indices will remain consistent while
                            // removing
                            let mut selected_panes = vec![];
                            for index in all_selected_indices.iter().rev() {
                                let index = self.search_results
                                    .as_mut()
                                    .and_then(|search_results| {
                                        if search_results.len() > *index {
                                            Some(search_results.remove(*index))
                                        } else {
                                            None
                                        }
                                    })
                                    .and_then(|selected_search_result| self.left_side_panes.iter().position(|p| p.id == selected_search_result.id))
                                    .unwrap_or(*index);
                                if self.left_side_panes.len() > index {
                                    let mut selected_pane = self.left_side_panes.remove(index);
                                    selected_pane.clear();
                                    selected_panes.push(selected_pane);
                                }
                            }
                            self.group_panes_in_zellij(selected_panes.iter().map(|p| p.id).collect());
                            self.right_side_panes.append(&mut selected_panes.into_iter().rev().collect());
                            let displayed_list_len = match self.search_results.as_ref() {
                                Some(search_results) => search_results.len(),
                                None => self.left_side_panes.len()
                            };

                            if displayed_list_len == 0 {
                                self.selected_index = None;
                                self.visibility_and_focus.hide_left_side();
                                self.search_string.clear();
                                self.search_results = None;
                            } else if selected_index.main_selected > displayed_list_len.saturating_sub(1) {
                                self.selected_index = Some(SelectedIndex::new(displayed_list_len.saturating_sub(1)));
                                self.visibility_and_focus.show_both_sides();
                            } else {
                                selected_index.additional_selected.clear();
                                self.selected_index = Some(selected_index);
                                self.visibility_and_focus.show_both_sides();
                            }
                            self.update_highlighted_panes();
                            should_render = true;
                        }
                    }
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        if self.visibility_and_focus.right_side_is_focused() {
                            // this means we're in the selection panes part and we want to clear
                            // them
                            let mut unselected_panes = vec![];
                            for pane_item in self.right_side_panes.iter_mut() {
                                pane_item.clear();
                                unselected_panes.push(pane_item.id);
                            }
                            self.left_side_panes.append(&mut self.right_side_panes);
                            self.ungroup_panes_in_zellij(unselected_panes);
                            self.visibility_and_focus.hide_right_side();
                            self.selected_index = None;
                        } else if self.visibility_and_focus.left_side_is_focused() {
                            if self.selected_index.is_some() {
                                self.selected_index = None;
                                self.update_highlighted_panes();
                            } else {
                                close_self();
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Down if key.has_no_modifiers() => {
                        match self.selected_index.as_mut() {
                            Some(selected_index) =>{
                                let is_searching = self.search_results.is_some();
                                let search_result_count = self.search_results.as_ref().map(|s| s.len()).unwrap_or(0);
                                if self.visibility_and_focus.left_side_is_focused() && is_searching && selected_index.main_selected == search_result_count.saturating_sub(1) {
                                    selected_index.main_selected = 0;
                                } else if self.visibility_and_focus.left_side_is_focused() && !is_searching && selected_index.main_selected == self.left_side_panes.len().saturating_sub(1) {
                                    selected_index.main_selected = 0;
                                } else if self.visibility_and_focus.right_side_is_focused() && selected_index.main_selected == self.right_side_panes.len().saturating_sub(1) {
                                    selected_index.main_selected = 0;
                                } else {
                                    selected_index.main_selected += 1
                                }
                            },
                            None => {
                                if self.visibility_and_focus.left_side_is_focused() {
                                    let is_searching = self.search_results.is_some();
                                    let has_search_results = self.search_results.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
                                    if is_searching && has_search_results {
                                        self.selected_index = Some(SelectedIndex::new(0));
                                    } else if !is_searching && !self.left_side_panes.is_empty() {
                                        self.selected_index = Some(SelectedIndex::new(0));
                                    }
                                } else if self.visibility_and_focus.right_side_is_focused() && !self.right_side_panes.is_empty() {
                                    self.selected_index = Some(SelectedIndex::new(0));
                                }
                            }
                        }
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Up if key.has_no_modifiers() => {
                        match self.selected_index.as_mut() {
                            Some(selected_index) =>{
                                if self.visibility_and_focus.left_side_is_focused() && selected_index.main_selected == 0 {
                                    if let Some(search_result_count) = self.search_results.as_ref().map(|s| s.len()) {
                                        selected_index.main_selected = search_result_count.saturating_sub(1);
                                    } else {
                                        selected_index.main_selected = self.left_side_panes.len().saturating_sub(1);
                                    }
                                } else if self.visibility_and_focus.right_side_is_focused() && selected_index.main_selected == 0 {
                                    selected_index.main_selected = self.right_side_panes.len().saturating_sub(1);
                                } else {
                                    selected_index.main_selected = selected_index.main_selected.saturating_sub(1);
                                }
                            },
                            None => {
                                if self.visibility_and_focus.left_side_is_focused() {
                                    let is_searching = self.search_results.is_some();
                                    let has_search_results = self.search_results.as_ref().map(|s| !s.is_empty()).unwrap_or(false);
                                    if is_searching && has_search_results {
                                        let search_results_count = self.search_results.as_ref().map(|s| s.len()).unwrap_or(0);
                                        self.selected_index = Some(SelectedIndex::new(search_results_count.saturating_sub(1)));
                                    } else if !is_searching && !self.left_side_panes.is_empty() {
                                        self.selected_index = Some(SelectedIndex::new(self.left_side_panes.len().saturating_sub(1)));
                                    }
                                } else if self.visibility_and_focus.right_side_is_focused() && !self.right_side_panes.is_empty() {
                                    self.selected_index = Some(SelectedIndex::new(self.right_side_panes.len().saturating_sub(1)));
                                }
                            }
                        }
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Char(' ') if key.has_no_modifiers() && self.selected_index.is_some() => {
                        if let Some(selected_index) = self.selected_index.as_mut() {
                            selected_index.toggle_additional_mark();
                            self.update_highlighted_panes();
                            should_render = true;
                        }
                    }
                    BareKey::Char('b') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        let pane_ids_to_break_to_new_tab: Vec<PaneId> = self
                            .right_side_panes
                            .iter()
                            .map(|p| p.id)
                            .collect();
                        let title_for_new_tab = if !self.previous_search_string.is_empty() {
                            Some(self.previous_search_string.clone())
                        } else {
                            None
                        };
                        break_panes_to_new_tab(&pane_ids_to_break_to_new_tab, title_for_new_tab, true);
                        self.ungroup_panes_in_zellij(pane_ids_to_break_to_new_tab);
                        close_self();
                    }
                    BareKey::Char('s') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        let pane_ids_to_stack: Vec<PaneId> = self
                            .right_side_panes
                            .iter()
                            .map(|p| p.id)
                            .collect();
                        stack_panes(pane_ids_to_stack.clone());
                        self.ungroup_panes_in_zellij(pane_ids_to_stack);
                        close_self();
                    }
                    BareKey::Char('f') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        let pane_ids_to_float: Vec<PaneId> = self
                            .right_side_panes
                            .iter()
                            .map(|p| p.id)
                            .collect();
                        float_multiple_panes(pane_ids_to_float.clone());
                        self.ungroup_panes_in_zellij(pane_ids_to_float);
                        close_self();
                    }
                    BareKey::Char('e') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        let pane_ids_to_embed: Vec<PaneId> = self
                            .right_side_panes
                            .iter()
                            .map(|p| p.id)
                            .collect();
                        embed_multiple_panes(pane_ids_to_embed.clone());
                        self.ungroup_panes_in_zellij(pane_ids_to_embed);
                        close_self();
                    }
                    BareKey::Char('r') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        if let Some(own_tab_index) = self.own_tab_index {
                            if Some(own_tab_index + 1) < self.total_tabs_in_session {
                                let pane_ids_to_break_right: Vec<PaneId> = self
                                    .right_side_panes
                                    .iter()
                                    .map(|p| p.id)
                                    .collect();
                                break_panes_to_tab_with_index(
                                    &pane_ids_to_break_right,
                                    own_tab_index + 1,
                                    true
                                );
                            } else {
                                let pane_ids_to_break_to_new_tab: Vec<PaneId> = self
                                    .right_side_panes
                                    .iter()
                                    .map(|p| p.id)
                                    .collect();
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
                    BareKey::Char('l') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        if let Some(own_tab_index) = self.own_tab_index {
                            if own_tab_index > 0 {
                                let pane_ids_to_break_left: Vec<PaneId> = self
                                    .right_side_panes
                                    .iter()
                                    .map(|p| p.id)
                                    .collect();
                                break_panes_to_tab_with_index(
                                    &pane_ids_to_break_left,
                                    own_tab_index.saturating_sub(1),
                                    true
                                );
                            } else {
                                let pane_ids_to_break_to_new_tab: Vec<PaneId> = self
                                    .right_side_panes
                                    .iter()
                                    .map(|p| p.id)
                                    .collect();
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
                    BareKey::Char('c') if key.has_no_modifiers() && self.visibility_and_focus.right_side_is_focused() => {
                        let pane_ids_to_close: Vec<PaneId> = self
                            .right_side_panes
                            .iter()
                            .map(|p| p.id)
                            .collect();
                        close_multiple_panes(
                            pane_ids_to_close,
                        );
                        close_self();
                    }
                    BareKey::Esc if key.has_no_modifiers() => {
                        if self.selected_index.is_some() {
                            self.selected_index = None;
                            self.update_highlighted_panes();
                        } else {
                            close_self();
                        }
                        should_render = true;
                    }
                    _ => {}
                }
            },
            _ => {},
        }
        should_render
    }
    fn render(&mut self, rows: usize, cols: usize) {
        self.render_close_shortcut(cols);
        self.render_tab_shortcut(cols);
        match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible => self.render_left_side(rows, cols, true),
            VisibilityAndFocus::OnlyRightSideVisible => self.render_right_side(rows, cols, true),
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused => {
                self.render_left_side(rows, cols, true);
                self.render_right_side(rows, cols, false);
            }
            VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                self.render_left_side(rows, cols, false);
                self.render_right_side(rows, cols, true);
            }
        }
        self.render_focus_boundary(rows, cols);
        self.render_help_line(rows, cols);
    }
}

impl App {
    fn update_panes(&mut self, pane_manifest: PaneManifest) {
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
        self.left_side_panes.retain(|p| all_panes.contains_key(&p.id));
        self.right_side_panes.retain(|p| all_panes.contains_key(&p.id));
        for (pane_id, pane) in all_panes.into_iter() {
            let is_known = self.left_side_panes.iter().find(|p| p.id == pane_id).is_some() || self.right_side_panes.iter().find(|p| p.id == pane_id).is_some();
            let is_grouped_for_own_client_id = self.own_client_id.map(|client_id| pane.is_grouped_for_clients.contains(&client_id)).unwrap_or(false);
            if !is_known {
                if is_grouped_for_own_client_id {
                    self.right_side_panes.push(PaneItem { text: pane.title, id: pane_id, color_indices: vec![] });
                } else {
                    self.left_side_panes.push(PaneItem { text: pane.title, id: pane_id, color_indices: vec![] });
                }
            } else {
                if is_grouped_for_own_client_id {
                    if let Some(position) = self.left_side_panes.iter().position(|p| p.id == pane_id) {
                        // pane was added to a pane group outside the plugin (eg. with mouse selection)
                        let mut pane = self.left_side_panes.remove(position);
                        pane.clear();
                        self.right_side_panes.push(pane);
                    }
                } else {
                    if let Some(position) = self.right_side_panes.iter().position(|p| p.id == pane_id) {
                        // pane was removed from a pane group outside the plugin (eg. with mouse selection)
                        let mut pane = self.right_side_panes.remove(position);
                        pane.clear();
                        self.left_side_panes.push(pane);
                    }
                }
            }
        }
    }
    fn update_tab_info(&mut self, pane_manifest: &PaneManifest) {
        for (tab_index, pane_infos) in &pane_manifest.panes {
            for pane_info in pane_infos {
                if pane_info.is_plugin && Some(pane_info.id) == self.own_plugin_id {
                    self.own_tab_index = Some(*tab_index);
                }
            }
        }
        self.total_tabs_in_session = Some(pane_manifest.panes.keys().count());
    }
    fn update_search_results(&mut self) {
        let mut matches = vec![];
        let matcher = SkimMatcherV2::default().use_cache(true);
        for pane_item in &self.left_side_panes {
            if let Some((score, indices)) = matcher.fuzzy_indices(&pane_item.text, &self.search_string) {
                let mut pane_item = pane_item.clone();
                pane_item.color_indices = indices;
                matches.push((score, pane_item));
            }
        }
        matches.sort_by(|(a_score, _a), (b_score, _b)| b_score.cmp(&a_score));
        if self.search_string.is_empty() {
            self.search_results = None;
        } else {
            self.search_results = Some(matches.into_iter().map(|(_s, pane_item)| pane_item).collect());
        }
    }
    fn group_panes_in_zellij(&mut self, pane_ids: Vec<PaneId>) {
        group_and_ungroup_panes(pane_ids, vec![]);
    }
    fn ungroup_panes_in_zellij(&mut self, pane_ids: Vec<PaneId>) {
        group_and_ungroup_panes(vec![], pane_ids);
    }
    fn update_highlighted_panes(&self) {
        let mut pane_ids_to_highlight = vec![];
        let mut pane_ids_to_unhighlight = vec![];
        if let Some(selected_index) = &self.selected_index {
            if self.visibility_and_focus.left_side_is_focused() {
                if let Some(main_selected_pane_id) = self.search_results
                    .as_ref()
                    .and_then(|s| s.get(selected_index.main_selected))
                    .or_else(|| self.left_side_panes.get(selected_index.main_selected))
                    .map(|p| p.id)
                {
                    pane_ids_to_highlight.push(main_selected_pane_id);
                }
                for index in &selected_index.additional_selected {
                    if let Some(pane_id) = self.search_results
                        .as_ref()
                        .and_then(|s| s.get(*index))
                        .or_else(|| self.left_side_panes.get(*index))
                        .map(|p| p.id)
                    {
                        pane_ids_to_highlight.push(pane_id);
                    }
                }
            } else {
                if let Some(main_selected_pane_id) = self.right_side_panes.get(selected_index.main_selected).map(|p| p.id) {
                    pane_ids_to_highlight.push(main_selected_pane_id);
                }
                for index in &selected_index.additional_selected {
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
}

// ui components
impl App {
    fn filter_panes_prompt(&self) -> (&'static str, Text) {
        let search_prompt_text = if self.search_string.is_empty() { "ALL PANES " } else { "FILTER: " };
        let search_prompt = if self.visibility_and_focus.left_side_is_focused() { Text::new(&search_prompt_text).color_range(2, ..) } else { Text::new(&search_prompt_text) };
        (search_prompt_text, search_prompt)
    }
    fn filter(&self, max_width: usize) -> Text {
        let search_string_text = if self.selected_index.is_none() && self.search_string.is_empty() {
            let full = "[Type filter term...]";
            let short = "[...]";
            if max_width >= full.chars().count() {
                full.to_owned()
            } else {
                short.to_owned()
            }
        } else if self.selected_index.is_none() && !self.search_string.is_empty() {
            if max_width >= self.search_string.chars().count() + 1 {
                format!("{}_", self.search_string)
            } else {
                let truncated: String = self.search_string.chars().rev().take(max_width.saturating_sub(4)).collect::<Vec<_>>().iter().rev().collect();
                format!("...{}_", truncated)
            }
        } else if self.selected_index.is_some() && !self.search_string.is_empty() {
            if max_width >= self.search_string.chars().count() {
                format!("{}", self.search_string)
            } else {
                let truncated: String = self.search_string.chars().rev().take(max_width.saturating_sub(4)).collect::<Vec<_>>().iter().rev().collect();
                format!("...{}", truncated)
            }
        } else {
            format!("")
        };
        Text::new(&search_string_text).color_range(3, ..)
    }
    fn left_side_panes_list(&self, max_width: usize, max_list_height: usize) -> (usize, usize, usize, usize, Vec<NestedListItem>) {
        // returns: extra_pane_count_on_top, extra_pane_count_on_bottom,
        // extra_selected_item_count_on_top, extra_selected_item_count_on_bottom, list
        let mut left_side_panes = vec![];
        let pane_items_on_the_left = self.search_results.as_ref().unwrap_or_else(|| &self.left_side_panes);
        let max_width_for_item = max_width.saturating_sub(3); // 3 for the list bulletin
        let item_count = pane_items_on_the_left.iter().count();
        let first_item_index = if self.visibility_and_focus.left_side_is_focused() {
            self
                .selected_index
                .as_ref()
                .map(|s| s.main_selected.saturating_sub(max_list_height / 2))
                .unwrap_or(0)
        } else {
            0
        };
        let last_item_index = std::cmp::min((max_list_height + first_item_index).saturating_sub(1), item_count.saturating_sub(1));
        for (i, pane_item) in pane_items_on_the_left.iter().enumerate().skip(first_item_index) {
            if i > last_item_index {
                break;
            }
            let mut item = pane_item.render(max_width_for_item);
            if Some(i) == self.selected_index.as_ref().map(|s| s.main_selected) && self.visibility_and_focus.left_side_is_focused() {
                item = item.selected();
                if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) && self.visibility_and_focus.left_side_is_focused() {
                item = item.selected();
            }
            left_side_panes.push(item);
        }
        let extra_panes_on_top = first_item_index;
        let extra_panes_on_bottom = item_count.saturating_sub(last_item_index + 1);
        let extra_selected_item_count_on_top = if self.visibility_and_focus.left_side_is_focused() {
            self.selected_index.as_ref().map(|s| s.additional_selected.iter().filter(|a| a < &&first_item_index).count()).unwrap_or(0)
        } else {
            0
        };
        let extra_selected_item_count_on_bottom = if self.visibility_and_focus.left_side_is_focused() {
            self.selected_index.as_ref().map(|s| s.additional_selected.iter().filter(|a| a > &&last_item_index).count()).unwrap_or(0)
        } else {
            0
        };
        (extra_panes_on_top, extra_panes_on_bottom, extra_selected_item_count_on_top, extra_selected_item_count_on_bottom, left_side_panes)
    }
    fn left_side_controls(&self, max_width: usize) -> (&'static str, Text, &'static str, Text, &'static str, Text) {
        // returns three components and their text
        let (enter_select_panes_text, enter_select_panes) = if self.selected_index.is_some() {
            let enter_select_panes_text_full = "<ENTER> - select, <↓↑→> - navigate";
            let enter_select_panes_text_short = "<ENTER> / <↓↑→>...";
            if max_width >= enter_select_panes_text_full.chars().count() {
                let enter_select_panes_full = Text::new(enter_select_panes_text_full).color_range(3, ..=6).color_range(3, 18..=22);
                (enter_select_panes_text_full, enter_select_panes_full)
            } else {
                let enter_select_panes_short = Text::new(enter_select_panes_text_short).color_range(3, ..=6).color_range(3, 10..=14);
                (enter_select_panes_text_short, enter_select_panes_short)
            }
        } else {
            let enter_select_panes_text_full = "<ENTER> - select all, <↓↑> - navigate";
            let enter_select_panes_text_short = "<ENTER> / <↓↑>...";
            if max_width >= enter_select_panes_text_full.chars().count() {
                let enter_select_panes_full = Text::new(enter_select_panes_text_full).color_range(3, ..=6).color_range(3, 21..=25);
                (enter_select_panes_text_full, enter_select_panes_full)
            } else {
                let enter_select_panes_short = Text::new(enter_select_panes_text_short).color_range(3, ..=6).color_range(3, 10..=13);
                (enter_select_panes_text_short, enter_select_panes_short)
            }
        };
        let space_shortcut_text_full = "<SPACE> - mark many,";
        let space_shortcut_text_short = "<SPACE> /";
        if self.selected_index.is_some() {
            let escape_shortcut_text_full = "<Ctrl c> - remove marks";
            let escape_shortcut_text_short = "<Ctrl c>...";
            let (escape_shortcut, space_shortcut, escape_shortcut_text, space_shortcut_text) = if max_width >= space_shortcut_text_full.chars().count() + escape_shortcut_text_full.chars().count() {
                (
                    Text::new(escape_shortcut_text_full).color_range(3, ..=7),
                    Text::new(space_shortcut_text_full).color_range(3, ..=6),
                    escape_shortcut_text_full,
                    space_shortcut_text_full,
                )
            } else {
                (
                    Text::new(escape_shortcut_text_short).color_range(3, ..=7),
                    Text::new(space_shortcut_text_short).color_range(3, ..=6),
                    escape_shortcut_text_short,
                    space_shortcut_text_short,
                )
            };
            (
                enter_select_panes_text,
                enter_select_panes,
                space_shortcut_text,
                space_shortcut,
                escape_shortcut_text,
                escape_shortcut
            )
        } else {
            let escape_shortcut_text = if self.right_side_panes.is_empty() { "<Ctrl c> - Close" } else { "" };
            let escape_shortcut = Text::new(escape_shortcut_text).color_range(3, ..=7);
            let space_shortcut = Text::new(space_shortcut_text_full).color_range(3, ..=6);
            (
                enter_select_panes_text,
                enter_select_panes,
                space_shortcut_text_full,
                space_shortcut,
                escape_shortcut_text,
                escape_shortcut
            )
        }
    }
    fn selected_panes_title(&self) -> Text {
        let selected_prompt_text = "SELECTED PANES: ";
        let selected_prompt = if self.visibility_and_focus.left_side_is_focused() { Text::new(selected_prompt_text) } else { Text::new(selected_prompt_text).color_range(2, ..) };
        selected_prompt
    }
    fn right_side_panes_list(&self, max_width: usize, max_list_height: usize) -> (usize, usize, usize, usize, Vec<NestedListItem>) {
        // returns: extra_pane_count_on_top, extra_pane_count_on_bottom,
        // extra_selected_item_count_on_top, extra_selected_item_count_on_bottom, list
        let mut right_side_panes = vec![];
        let item_count = self.right_side_panes.iter().count();
        let first_item_index = if self.visibility_and_focus.left_side_is_focused() {
            0
        } else {
            self
                .selected_index
                .as_ref()
                .map(|s| s.main_selected.saturating_sub(max_list_height / 2))
                .unwrap_or(0)
        };
        let last_item_index = std::cmp::min((max_list_height + first_item_index).saturating_sub(1), item_count.saturating_sub(1));

        let max_width_for_item = max_width.saturating_sub(3); // 3 for the list bulletin
        for (i, pane_item) in self.right_side_panes.iter().enumerate().skip(first_item_index) {
            if i > last_item_index {
                break;
            }
            let mut item = pane_item.render(max_width_for_item);
            if &Some(i) == &self.selected_index.as_ref().map(|s| s.main_selected) && self.visibility_and_focus.right_side_is_focused() {
                item = item.selected();
                if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) && self.visibility_and_focus.right_side_is_focused() {
                item = item.selected();
            }
            right_side_panes.push(item);
        }

        let extra_panes_on_top = first_item_index;
        let extra_panes_on_bottom = self.right_side_panes.iter().len().saturating_sub(last_item_index + 1);
        let extra_selected_item_count_on_top = if self.visibility_and_focus.left_side_is_focused() {
            0
        } else {
            self.selected_index.as_ref().map(|s| s.additional_selected.iter().filter(|a| a < &&first_item_index).count()).unwrap_or(0)
        };
        let extra_selected_item_count_on_bottom = if self.visibility_and_focus.left_side_is_focused() {
            0
        } else {
            self.selected_index.as_ref().map(|s| s.additional_selected.iter().filter(|a| a > &&last_item_index).count()).unwrap_or(0)
        };
        (extra_panes_on_top, extra_panes_on_bottom, extra_selected_item_count_on_top, extra_selected_item_count_on_bottom, right_side_panes)
    }
    fn right_side_controls(&self, cols: usize) -> (Text, Text, Text, Text) {
        let right_side_controls_text_1_full = "<←↓↑> - navigate, <Ctrl c> - clear";
        let right_side_controls_text_1_short = "<←↓↑>/<Ctrl c>...";
        let right_side_controls_1 = if cols >= right_side_controls_text_1_full.chars().count() {
            Text::new(right_side_controls_text_1_full).color_range(3, ..=4).color_range(3, 18..=25)
        } else {
            Text::new(right_side_controls_text_1_short).color_range(3, ..=4).color_range(3, 6..=13)
        };
        let right_side_controls_text_2_full = "<b> - break out, <s> - stack, <c> - close";
        let right_side_controls_text_2_short = "<b>/<s>/<c>...";
        let right_side_controls_2 = if cols >= right_side_controls_text_2_full.chars().count() {
            Text::new(right_side_controls_text_2_full).color_range(3, ..=2).color_range(3, 17..=19).color_range(3, 30..=32)
        } else {
            Text::new(right_side_controls_text_2_short).color_range(3, ..=2).color_range(3, 4..=6).color_range(3, 8..=10)
        };
        let right_side_controls_text_3_full = "<r> - break right, <l> - break left";
        let right_side_controls_text_3_short = "<r>/<l>...";
        let right_side_controls_3 = if cols >= right_side_controls_text_3_full.chars().count() {
            Text::new(right_side_controls_text_3_full).color_range(3, ..=2).color_range(3, 19..=21)
        } else {
            Text::new(right_side_controls_text_3_short).color_range(3, ..=2).color_range(3, 4..=6)
        };
        let right_side_controls_text_4_full = "<e> - embed, <f> - float";
        let right_side_controls_text_4_short = "<e>/<f>...";
        let right_side_controls_4 = if cols >= right_side_controls_text_4_full.chars().count() {
            Text::new(right_side_controls_text_4_full).color_range(3, ..=2).color_range(3, 13..=15)
        } else {
            Text::new(right_side_controls_text_4_short).color_range(3, ..=2).color_range(3, 4..=6)
        };
        (
            right_side_controls_1,
            right_side_controls_2,
            right_side_controls_3,
            right_side_controls_4,
        )
    }
    fn print_extra_pane_count(&self, count: usize, selected_count: usize, y: usize, list_x: usize, list_width: usize) {
        let extra_count_text = if selected_count > 0 {
            format!("[+{} ({} selected)]", count, selected_count)
        } else {
            format!("[+{}]", count)
        };
        let extra_count = Text::new(&extra_count_text).color_range(1, ..);
        print_text_with_coordinates(extra_count, (list_x + list_width).saturating_sub(extra_count_text.chars().count()), y, None, None);
    }
}

// rendering code
impl App {
    fn render_close_shortcut(&self, cols: usize) {
        let should_render_close_shortcut = self.visibility_and_focus.left_side_is_focused() && self.selected_index.is_none();
        if should_render_close_shortcut {
            let ctrl_c_shortcut_text = "<Ctrl c> - Close";
            let ctrl_c_shortcut = Text::new(ctrl_c_shortcut_text).color_range(3, ..=7);
            print_text_with_coordinates(ctrl_c_shortcut, cols.saturating_sub(ctrl_c_shortcut_text.chars().count()).saturating_sub(1), 0, None, None);
        }
    }
    fn render_tab_shortcut(&self, cols: usize) {
        match self.visibility_and_focus {
            VisibilityAndFocus::BothSidesVisibleRightSideFocused | VisibilityAndFocus::BothSidesVisibleLeftSideFocused => {
                let side_width = self.calculate_side_width(cols);
                let tab_shortcut = Text::new("<TAB>").color_range(3, ..=4);
                print_text_with_coordinates(tab_shortcut, side_width, 0, None, None);
            }
            VisibilityAndFocus::OnlyRightSideVisible => {
                let tab_shortcut = Text::new("<TAB> - select more panes").color_range(3, ..=4);
                print_text_with_coordinates(tab_shortcut, 0, 0, None, None);
            }
            VisibilityAndFocus::OnlyLeftSideVisible => {
                // not visible
            }
        };
    }
    fn render_left_side(&self, rows: usize, cols: usize, is_focused: bool) {
        let title_y = 1;
        let left_side_base_x = 1;
        let list_y = 3;
        let side_width = self.calculate_side_width(cols);
        let max_left_list_height = rows.saturating_sub(8);
        let (
            extra_pane_count_on_top_left,
            extra_pane_count_on_bottom_left,
            extra_selected_item_count_on_top_left,
            extra_selected_item_count_on_bottom_left,
            left_side_panes
        ) = self.left_side_panes_list(side_width, max_left_list_height);
        let (filter_prompt_text, filter_prompt) = self.filter_panes_prompt();
        let filter = self.filter(side_width.saturating_sub(filter_prompt_text.chars().count() + 1));
        let (
            _enter_select_panes_text,
            enter_select_panes,
            space_shortcut_text,
            space_shortcut,
            _escape_shortcut_text,
            escape_shortcut
        ) = self.left_side_controls(side_width);
        print_text_with_coordinates(filter_prompt, left_side_base_x, title_y, None, None);
        if is_focused {
            print_text_with_coordinates(filter, left_side_base_x + filter_prompt_text.chars().count(), title_y, None, None);
        }
        print_nested_list_with_coordinates(left_side_panes.clone(), left_side_base_x, list_y, Some(side_width), None);
        if is_focused {
            if let Some(selected_index) = self.selected_index.as_ref().map(|i| i.main_selected) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), left_side_base_x + 1, (list_y + selected_index).saturating_sub(extra_pane_count_on_top_left), None, None);
            }
        }
        if extra_pane_count_on_top_left > 0 {
            self.print_extra_pane_count(extra_pane_count_on_top_left, extra_selected_item_count_on_top_left, list_y.saturating_sub(1), left_side_base_x, side_width);
        }
        if extra_pane_count_on_bottom_left > 0 {
            self.print_extra_pane_count(extra_pane_count_on_bottom_left, extra_selected_item_count_on_bottom_left, list_y + left_side_panes.len(), left_side_base_x, side_width);
        }
        if is_focused && !left_side_panes.is_empty() {
            let controls_x = 1;
            print_text_with_coordinates(enter_select_panes, controls_x, list_y + left_side_panes.len() + 1, None, None);
            if self.selected_index.is_some() {
                print_text_with_coordinates(space_shortcut.clone(), controls_x, list_y + left_side_panes.len() + 2, None, None);
                print_text_with_coordinates(escape_shortcut.clone(), controls_x + space_shortcut_text.chars().count() + 1, list_y + left_side_panes.len() + 2, None, None);
            }
        }
    }
    fn render_right_side(&self, rows: usize, cols: usize, is_focused: bool) {
        let side_width = self.calculate_side_width(cols);
        let right_side_base_x = match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible | VisibilityAndFocus::OnlyRightSideVisible => 1,
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused | VisibilityAndFocus::BothSidesVisibleRightSideFocused => side_width + 4,
        };
        let title_y = 1;
        let list_y: usize = 3;
        let max_right_list_height = rows.saturating_sub(11);
        let selected_prompt = self.selected_panes_title();
        let (
            extra_pane_count_on_top_right,
            extra_pane_count_on_bottom_right,
            extra_selected_item_count_on_top_right,
            extra_selected_item_count_on_bottom_right,
            right_side_panes
        ) = self.right_side_panes_list(side_width, max_right_list_height);
        let right_side_pane_count = right_side_panes.len();
        let (
            right_side_controls_1,
            right_side_controls_2,
            right_side_controls_3,
            right_side_controls_4,
        ) = self.right_side_controls(side_width);
        if extra_pane_count_on_top_right > 0 {
            self.print_extra_pane_count(extra_pane_count_on_top_right, extra_selected_item_count_on_top_right, list_y.saturating_sub(1), right_side_base_x, side_width);
        }
        if extra_pane_count_on_bottom_right > 0 {
            self.print_extra_pane_count(extra_pane_count_on_bottom_right, extra_selected_item_count_on_bottom_right, list_y + right_side_panes.len(), right_side_base_x, side_width);
        }
        print_text_with_coordinates(selected_prompt, right_side_base_x + 3, title_y, None, None);
        print_nested_list_with_coordinates(right_side_panes, right_side_base_x, list_y, Some(side_width), None);
        if is_focused {
            if let Some(selected_index) = self.selected_index.as_ref().map(|i| i.main_selected) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), right_side_base_x+ 1, (list_y + selected_index).saturating_sub(extra_pane_count_on_top_right), None, None);
            }
        }
        if is_focused && !self.right_side_panes.is_empty() {
            print_text_with_coordinates(right_side_controls_1, right_side_base_x + 1, list_y + right_side_pane_count + 1, None, None);
            print_text_with_coordinates(right_side_controls_2, right_side_base_x + 1, list_y + right_side_pane_count + 3, None, None);
            print_text_with_coordinates(right_side_controls_3, right_side_base_x + 1, list_y + right_side_pane_count + 4, None, None);
            print_text_with_coordinates(right_side_controls_4, right_side_base_x + 1, list_y + right_side_pane_count + 5, None, None);
        }
    }
    fn render_help_line(&self, rows: usize, cols: usize) {
        let help_line_text = match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible => {
                let full_help_line = "Help: Select one or more panes to group for bulk operations";
                let short_help_line = "Help: Select panes to group";
                if cols >= full_help_line.chars().count() {
                    full_help_line
                } else {
                    short_help_line
                }
            },
            VisibilityAndFocus::OnlyRightSideVisible => {
                let full_help_line = "Help: Perform bulk operations on all selected panes";
                let short_help_line = "Help: Perform bulk operations";
                if cols >= full_help_line.chars().count() {
                    full_help_line
                } else {
                    short_help_line
                }
            }
            _ => {
                let full_help_line = "Help: Select panes on the left, then perform operations on the right.";
                let short_help_line = "Help: Group panes for bulk operations";
                if cols >= full_help_line.chars().count() {
                    full_help_line
                } else {
                    short_help_line
                }
            }
        };
        let help_line = Text::new(help_line_text);
        print_text_with_coordinates(help_line, 0, rows, None, None);
    }
    fn render_focus_boundary(&self, rows: usize, cols: usize) {
        let side_width = self.calculate_side_width(cols);
        let x = match self.visibility_and_focus {
            VisibilityAndFocus::OnlyRightSideVisible => 0,
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused |
            VisibilityAndFocus::BothSidesVisibleRightSideFocused |
            VisibilityAndFocus::OnlyLeftSideVisible => side_width + 2
        };
        let y = 1;
        let height = rows.saturating_sub(2);
        for i in y..=height {
            if i == y && self.visibility_and_focus.left_side_is_focused() {
                print_text_with_coordinates(Text::new(TOP_RIGHT_CORNER_CHARACTER), x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x.saturating_sub(1), i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x.saturating_sub(2), i, None, None);
            } else if i == y && !self.visibility_and_focus.left_side_is_focused() {
                print_text_with_coordinates(Text::new(TOP_LEFT_CORNER_CHARACTER), x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x + 1, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x + 2, i, None, None);
            } else if i == height && self.visibility_and_focus.left_side_is_focused() {
                print_text_with_coordinates(Text::new(BOTTOM_RIGHT_CORNER_CHARACTER), x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x.saturating_sub(1), i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x.saturating_sub(2), i, None, None);
            } else if i == height && !self.visibility_and_focus.left_side_is_focused() {
                print_text_with_coordinates(Text::new(BOTTOM_LEFT_CORNER_CHARACTER), x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x + 1, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), x + 2, i, None, None);
            } else {
                print_text_with_coordinates(Text::new(BOUNDARY_CHARACTER), x, i, None, None);
            }
        }
    }
    fn calculate_side_width(&self, cols: usize) -> usize {
        match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible | VisibilityAndFocus::OnlyRightSideVisible => {
                cols.saturating_sub(4)
            }
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused | VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                (cols / 2).saturating_sub(3)
            }
        }
    }
}
