use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

use zellij_tile::prelude::*;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::rc::Rc;

const UI_ROWS: usize = 20;
const UI_COLUMNS: usize = 90;

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
}

#[derive(Debug, Default)]
struct App {
    own_plugin_id: Option<u32>,
    own_client_id: Option<ClientId>,
    error: Option<String>,
    search_string: String,
    left_side_panes: Vec<PaneItem>,
    right_side_panes: Vec<PaneItem>,
    search_results: Option<Vec<PaneItem>>,
    is_searching: bool,
    selected_index: Option<SelectedIndex>,
}

register_plugin!(App);

impl ZellijPlugin for App {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
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

        self.is_searching = true;

//         // MOCK DATA
//         self.left_side_panes.push(PaneItem { text: "vim src/main.rs".to_owned() });
//         self.left_side_panes.push(PaneItem { text: "htop".to_owned() });
//         self.left_side_panes.push(PaneItem { text: "fish".to_owned() });
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::PaneUpdate(pane_manifest) => {
                // TODO: if selected_index.is_some() or search_string is not empty, add this to
                //    self.pending_pane_update and do it once these conditions are ripe
                self.update_panes(pane_manifest);
                should_render = true;
            }
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Tab if key.has_no_modifiers() => {
                        self.is_searching = !self.is_searching;
                        self.selected_index = None;
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Char(character) if key.has_no_modifiers() && self.is_searching && self.selected_index.is_none() => {
                        self.search_string.push(character);
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Backspace if key.has_no_modifiers() && self.is_searching && self.selected_index.is_none() => {
                        self.search_string.pop();
                        self.update_search_results();
                        should_render = true;
                    }
                    BareKey::Enter if key.has_no_modifiers() => {
                        if self.is_searching {
                            if let Some(mut selected_index) = self.selected_index.take() {
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
                                let selecting_search_results = self.search_results.is_some();
                                self.search_results = None;
                                self.search_string.clear();

                                if self.left_side_panes.is_empty() || selecting_search_results {
                                    self.selected_index = None;
                                    self.is_searching = false;
                                } else if selected_index.main_selected > self.left_side_panes.len().saturating_sub(1) {
                                    self.selected_index = Some(SelectedIndex::new(self.left_side_panes.len().saturating_sub(1)));
                                } else {
                                    selected_index.additional_selected.clear();
                                    self.selected_index = Some(selected_index);
                                }
                                self.update_highlighted_panes();
                                self.group_panes_in_zellij(pane_ids_to_make_selected);
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
                                self.is_searching = false;
                                self.search_string.clear();
                                self.selected_index = None;
                                self.search_results = None;
                                self.update_highlighted_panes();
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Left if key.has_no_modifiers() && !self.is_searching => {
                        if !self.is_searching {
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
                                    self.is_searching = true;
                                } else if selected_index.main_selected > self.right_side_panes.len().saturating_sub(1) {
                                    self.selected_index = Some(SelectedIndex::new(self.right_side_panes.len().saturating_sub(1)));
                                } else {
                                    selected_index.additional_selected.clear();
                                    self.selected_index = Some(selected_index);
                                }
                                should_render = true;
                                self.update_highlighted_panes();
                            }
                        }
                    }
                    BareKey::Right if key.has_no_modifiers() && self.is_searching => {
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
                                self.is_searching = false;
                                self.search_string.clear();
                                self.search_results = None;
                            } else if selected_index.main_selected > displayed_list_len.saturating_sub(1) {
                                self.selected_index = Some(SelectedIndex::new(displayed_list_len.saturating_sub(1)));
                            } else {
                                selected_index.additional_selected.clear();
                                self.selected_index = Some(selected_index);
                            }
                            self.update_highlighted_panes();
                            should_render = true;
                        }
                    }
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        if !self.is_searching {
                            // this means we're in the selection panes part and we want to clear
                            // them
                            let mut unselected_panes = vec![];
                            for pane_item in self.right_side_panes.iter_mut() {
                                pane_item.clear();
                                unselected_panes.push(pane_item.id);
                            }
                            self.left_side_panes.append(&mut self.right_side_panes);
                            self.ungroup_panes_in_zellij(unselected_panes);
                        }
                        self.is_searching = true;
                        self.selected_index = None;
                        should_render = true;
                    }
                    BareKey::Down if key.has_no_modifiers() => {
                        match self.selected_index.as_mut() {
                            Some(selected_index) =>{
                                if self.is_searching && selected_index.main_selected == self.left_side_panes.len().saturating_sub(1) {
                                    selected_index.main_selected = 0;
                                } else if !self.is_searching && selected_index.main_selected == self.right_side_panes.len().saturating_sub(1) {
                                    selected_index.main_selected = 0;
                                } else {
                                    selected_index.main_selected += 1
                                }
                            },
                            None => {
                                self.selected_index = Some(SelectedIndex::new(0));
                            }
                        }
                        self.update_highlighted_panes();
                        should_render = true;
                    }
                    BareKey::Up if key.has_no_modifiers() => {
                        match self.selected_index.as_mut() {
                            Some(selected_index) =>{
                                if self.is_searching && selected_index.main_selected == 0 {
                                    selected_index.main_selected = self.left_side_panes.len().saturating_sub(1);
                                } else if !self.is_searching && selected_index.main_selected == 0 {
                                    selected_index.main_selected = self.right_side_panes.len().saturating_sub(1);
                                } else {
                                    selected_index.main_selected = selected_index.main_selected.saturating_sub(1);
                                }
                            },
                            None => {
                                if self.is_searching {
                                    self.selected_index = Some(SelectedIndex::new(self.left_side_panes.len().saturating_sub(1)));
                                } else {
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
        let tab_text = " <TAB> ";
        let side_width = (cols / 2).saturating_sub(2).saturating_sub((tab_text.chars().count() + 1) / 2);

        let search_prompt_text = "FILTER PANES: ";
        let search_prompt = if self.is_searching { Text::new(&search_prompt_text).color_range(2, ..) } else { Text::new(&search_prompt_text) };
        let search_string_text = if self.selected_index.is_none() {
            format!("{}_", self.search_string)
        } else if self.selected_index.is_some() && !self.search_string.is_empty() {
            format!("{}", self.search_string)
        } else {
            format!("")
        };
        let search_string = Text::new(search_string_text).color_range(3, ..);
        let mut left_side_panes = vec![];
        let pane_items_on_the_left = self.search_results.as_ref().unwrap_or_else(|| &self.left_side_panes);
        for (i, pane_item) in pane_items_on_the_left.iter().enumerate() {
            let mut item = NestedListItem::new(&pane_item.text)
                .color_range(0, ..)
                .color_indices(3, pane_item.color_indices.iter().copied().collect());
            if Some(i) == self.selected_index.as_ref().map(|s| s.main_selected) && self.is_searching {
                item = item.selected();
                if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) && self.is_searching {
                item = item.selected();
            }
            left_side_panes.push(item);
        }

        let (enter_stage_panes_text, enter_stage_panes) = if self.selected_index.is_some() {
            let enter_stage_panes_text = "<ENTER> - select, <↓↑> - navigate";
            let enter_stage_panes = Text::new(enter_stage_panes_text).color_range(3, ..=6).color_range(3, 18..=21);
            (enter_stage_panes_text, enter_stage_panes)
        } else {
            let enter_stage_panes_text = "<ENTER> - select all, <↓↑> - navigate";
            let enter_stage_panes = Text::new(enter_stage_panes_text).color_range(3, ..=6).color_range(3, 21..=25);
            (enter_stage_panes_text, enter_stage_panes)
        };

        let help_line_text = "Help: Select panes on the left, then perform operations on the right.";
        let help_line = Text::new(help_line_text);
        let space_shortcut_text = "<SPACE> - mark many,";
        let space_shortcut = Text::new(space_shortcut_text).color_range(3, ..=6);
        let (escape_shortcut_text, escape_shortcut) = if self.selected_index.is_some() {
            let escape_shortcut_text = "<ESC> - remove marks";
            let escape_shortcut = Text::new(escape_shortcut_text).color_range(3, ..=4);
            (escape_shortcut_text, escape_shortcut)
        } else {
            let escape_shortcut_text = "<ESC> - Close";
            let escape_shortcut = Text::new(escape_shortcut_text).color_range(3, ..=4);
            (escape_shortcut_text, escape_shortcut)
        };

        let staged_prompt_text = "SELECTED PANES: ";
        let staged_prompt = if self.is_searching { Text::new(staged_prompt_text) } else { Text::new(staged_prompt_text).color_range(2, ..) };
        let mut right_side_panes = vec![];
        for (i, pane_item) in self.right_side_panes.iter().enumerate() {
            let mut item = NestedListItem::new(&pane_item.text).color_range(0, ..);
            if &Some(i) == &self.selected_index.as_ref().map(|s| s.main_selected) && !self.is_searching {
                item = item.selected();
                if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.selected_index.as_ref().map(|s| s.additional_selected.contains(&i)).unwrap_or(false) && !self.is_searching {
                item = item.selected();
            }
            right_side_panes.push(item);
        }

        let right_side_controls_text_1 = "<←↓↑> - navigate, <Ctrl c> - clear";
        let right_side_controls_1 = Text::new(right_side_controls_text_1).color_range(3, ..=4).color_range(3, 18..=25);

        let right_side_controls_text_2 = "<b> - break out, <s> - stack, <c> - close";
        let right_side_controls_2 = Text::new(right_side_controls_text_2).color_range(3, ..=2).color_range(3, 17..=19).color_range(3, 30..=32);
        let right_side_controls_text_3 = "<r> - break right, <l> - break left";
        let right_side_controls_3 = Text::new(right_side_controls_text_3).color_range(3, ..=2).color_range(3, 19..=21);

        let right_side_controls_text_4 = "<Enter> - group";
        let right_side_controls_4 = Text::new(right_side_controls_text_4).color_range(3, ..=6);

        let left_side_base_x = 2;
        let right_side_base_x = side_width + 1 + tab_text.chars().count() + 1;
        let prompt_y = 1;
        let list_y = 3;

        let left_boundary_start = 0;
        let left_boundary_end = left_boundary_start + side_width + 1;
        let tab_shortcut = Text::new(tab_text).color_range(3, ..);
        print_text_with_coordinates(tab_shortcut, left_boundary_end + 1, 0, None, None);
        let middle_border_x = left_boundary_end + 4;
        for i in prompt_y..rows.saturating_sub(1) {
            // middle border
            if i == prompt_y && self.is_searching {
                print_text_with_coordinates(Text::new(TOP_RIGHT_CORNER_CHARACTER), middle_border_x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x.saturating_sub(1), i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x.saturating_sub(2), i, None, None);
            } else if i == prompt_y && !self.is_searching {
                print_text_with_coordinates(Text::new(TOP_LEFT_CORNER_CHARACTER), middle_border_x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x + 1, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x + 2, i, None, None);
            } else if i == rows.saturating_sub(2) && self.is_searching {
                print_text_with_coordinates(Text::new(BOTTOM_RIGHT_CORNER_CHARACTER), middle_border_x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x.saturating_sub(1), i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x.saturating_sub(2), i, None, None);
            } else if i == rows.saturating_sub(2) && !self.is_searching {
                print_text_with_coordinates(Text::new(BOTTOM_LEFT_CORNER_CHARACTER), middle_border_x, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x + 1, i, None, None);
                print_text_with_coordinates(Text::new(HORIZONTAL_BOUNDARY_CHARACTER), middle_border_x + 2, i, None, None);
            } else {
                print_text_with_coordinates(Text::new(BOUNDARY_CHARACTER), middle_border_x, i, None, None);
            }
        }

        print_text_with_coordinates(search_prompt, left_side_base_x, prompt_y, None, None);
        if self.is_searching {
            print_text_with_coordinates(search_string, left_side_base_x + search_prompt_text.chars().count(), prompt_y, None, None);
        }
        print_nested_list_with_coordinates(left_side_panes.clone(), left_side_base_x, list_y, Some(side_width), None);
        if self.is_searching {
            if let Some(selected_index) = self.selected_index.as_ref().map(|i| i.main_selected) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), left_side_base_x + 1, list_y + selected_index, None, None);
            }
        }

        if self.is_searching && !left_side_panes.is_empty() {
            let controls_x = 1;
            print_text_with_coordinates(enter_stage_panes, controls_x, list_y + left_side_panes.len() + 1, None, None);
            if self.selected_index.is_some() {
                print_text_with_coordinates(space_shortcut.clone(), controls_x, list_y + left_side_panes.len() + 2, None, None);
                print_text_with_coordinates(escape_shortcut.clone(), controls_x + space_shortcut_text.chars().count() + 1, list_y + left_side_panes.len() + 2, None, None);
            }
        }

        print_text_with_coordinates(staged_prompt, right_side_base_x, prompt_y, None, None);
        print_nested_list_with_coordinates(right_side_panes, right_side_base_x, list_y, Some(side_width), None);
        if !self.is_searching {
            if let Some(selected_index) = self.selected_index.as_ref().map(|i| i.main_selected) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), right_side_base_x+ 1, list_y + selected_index, None, None);
            }
        }
        if self.is_searching && !self.right_side_panes.is_empty() {
        } else if !self.is_searching && !self.right_side_panes.is_empty() {
            print_text_with_coordinates(right_side_controls_1, right_side_base_x, list_y + self.right_side_panes.len() + 1, None, None);
            print_text_with_coordinates(right_side_controls_2, right_side_base_x, list_y + self.right_side_panes.len() + 3, None, None);
            print_text_with_coordinates(right_side_controls_3, right_side_base_x, list_y + self.right_side_panes.len() + 4, None, None);
            print_text_with_coordinates(right_side_controls_4, right_side_base_x, list_y + self.right_side_panes.len() + 5, None, None);
        }
        if self.selected_index.is_none() {
            print_text_with_coordinates(escape_shortcut, cols.saturating_sub(escape_shortcut_text.chars().count()).saturating_sub(1), 0, None, None);
        }
        print_text_with_coordinates(help_line, 0, rows, None, None);
    }
}

impl App {
    fn update_panes(&mut self, pane_manifest: PaneManifest) {

        let mut all_panes = BTreeMap::new();

        for (tab_index, pane_infos) in pane_manifest.panes {
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
            if self.is_searching {
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
