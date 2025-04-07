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

#[derive(Debug, Default)]
struct PaneItem {
    text: String,
}

#[derive(Debug, Default)]
struct App {
    own_plugin_id: Option<u32>,
    error: Option<String>,
    search_string: String,
    left_side_panes: Vec<PaneItem>,
    right_side_panes: Vec<PaneItem>,
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
            EventType::FailedToWriteConfigToDisk,
            EventType::ConfigWasWrittenToDisk,
        ]);
        let own_plugin_id = get_plugin_ids().plugin_id;
        self.own_plugin_id = Some(own_plugin_id);

        self.is_searching = true;

        // MOCK DATA
        self.left_side_panes.push(PaneItem { text: "vim src/main.rs".to_owned() });
        self.left_side_panes.push(PaneItem { text: "htop".to_owned() });
        self.left_side_panes.push(PaneItem { text: "fish".to_owned() });
    }
    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::Key(key) => {
                match key.bare_key {
                    BareKey::Tab if key.has_no_modifiers() => {
                        self.is_searching = !self.is_searching;
                        self.selected_index = None;
                        should_render = true;
                    }
                    BareKey::Char(character) if key.has_no_modifiers() && self.is_searching && self.selected_index.is_none() => {
                        self.search_string.push(character);
                        should_render = true;
                    }
                    BareKey::Backspace if key.has_no_modifiers() && self.is_searching && self.selected_index.is_none() => {
                        self.search_string.pop();
                        should_render = true;
                    }
                    BareKey::Enter if key.has_no_modifiers() => {
                        if self.is_searching {
                            if let Some(selected_index) = self.selected_index.take() {
                                let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.iter().copied().collect();
                                all_selected_indices.insert(selected_index.main_selected);

                                // reverse so that the indices will remain consistent while
                                // removing
                                let mut selected_panes = vec![];
                                for index in all_selected_indices.iter().rev() {
                                    if self.left_side_panes.len() > *index {
                                        let selected_pane = self.left_side_panes.remove(*index);
                                        selected_panes.push(selected_pane);
                                    }
                                }
                                self.right_side_panes.append(&mut selected_panes.into_iter().rev().collect());

                                if self.left_side_panes.is_empty() {
                                    self.selected_index = None;
                                    self.is_searching = false;
                                } else if selected_index.main_selected > self.left_side_panes.len().saturating_sub(1) {
                                    self.selected_index = Some(SelectedIndex::new(self.left_side_panes.len().saturating_sub(1)));
                                } else {
                                    self.selected_index = Some(selected_index);
                                }
                            } else {
                                self.right_side_panes.append(&mut self.left_side_panes);
                                self.is_searching = false;
                                self.search_string.clear();
                                self.selected_index = None;
                            }
                        }
                        should_render = true;
                    }
                    BareKey::Left if key.has_no_modifiers() && !self.is_searching => {
                        if !self.is_searching {
                            if let Some(selected_index) = self.selected_index.take() {
                                let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.iter().copied().collect();
                                all_selected_indices.insert(selected_index.main_selected);

                                // reverse so that the indices will remain consistent while
                                // removing
                                let mut selected_panes = vec![];
                                for index in all_selected_indices.iter().rev() {
                                    if self.right_side_panes.len() > *index {
                                        let selected_pane = self.right_side_panes.remove(*index);
                                        selected_panes.push(selected_pane);
                                    }
                                }
                                self.left_side_panes.append(&mut selected_panes.into_iter().rev().collect());

                                if self.right_side_panes.is_empty() {
                                    self.selected_index = None;
                                    self.is_searching = true;
                                } else if selected_index.main_selected > self.right_side_panes.len().saturating_sub(1) {
                                    self.selected_index = Some(SelectedIndex::new(self.right_side_panes.len().saturating_sub(1)));
                                } else {
                                    self.selected_index = Some(selected_index);
                                }
                                should_render = true;
                            }
                        }
                    }
                    BareKey::Right if key.has_no_modifiers() && self.is_searching => {
                        if let Some(selected_index) = self.selected_index.take() {
                            let mut all_selected_indices: BTreeSet<usize> = selected_index.additional_selected.iter().copied().collect();
                            all_selected_indices.insert(selected_index.main_selected);

                            // reverse so that the indices will remain consistent while
                            // removing
                            let mut selected_panes = vec![];
                            for index in all_selected_indices.iter().rev() {
                                if self.left_side_panes.len() > *index {
                                    let selected_pane = self.left_side_panes.remove(*index);
                                    selected_panes.push(selected_pane);
                                }
                            }
                            self.right_side_panes.append(&mut selected_panes.into_iter().rev().collect());

                            if self.left_side_panes.is_empty() {
                                self.selected_index = None;
                                self.is_searching = false;
                            } else if selected_index.main_selected > self.left_side_panes.len().saturating_sub(1) {
                                self.selected_index = Some(SelectedIndex::new(self.left_side_panes.len().saturating_sub(1)));
                            } else {
                                self.selected_index = Some(selected_index);
                            }
                            should_render = true;
                        }
                    }
                    BareKey::Char('c') if key.has_modifiers(&[KeyModifier::Ctrl]) => {
                        if !self.is_searching {
                            // this means we're in the selection panes part and we want to clear
                            // them
                            self.left_side_panes.append(&mut self.right_side_panes);
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
                        should_render = true;
                    }
                    BareKey::Char(' ') if key.has_no_modifiers() && self.selected_index.is_some() => {
                        if let Some(selected_index) = self.selected_index.as_mut() {
                            selected_index.toggle_additional_mark();
                            should_render = true;
                        }
                    }
                    BareKey::Esc if key.has_no_modifiers() => {
                        if self.selected_index.is_some() {
                            self.selected_index = None;
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
        let search_string_text = format!("{}_", self.search_string);
        let search_string = Text::new(search_string_text).color_range(3, ..);
        let mut left_side_panes = vec![];
        for (i, pane_item) in self.left_side_panes.iter().enumerate() {
            let mut item = NestedListItem::new(&pane_item.text).color_range(0, ..);
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

        if self.selected_index.is_some() {
            print_text_with_coordinates(space_shortcut.clone(), controls_x, list_y + self.left_side_panes.len() + 2, None, None);
            print_text_with_coordinates(escape_shortcut.clone(), controls_x + space_shortcut_text.chars().count() + 1, list_y + self.left_side_panes.len() + 2, None, None);
        }

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
        print_nested_list_with_coordinates(left_side_panes, left_side_base_x, list_y, Some(side_width), None);
        if self.is_searching {
            if let Some(selected_index) = self.selected_index.as_ref().map(|i| i.main_selected) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), left_side_base_x + 1, list_y + selected_index, None, None);
            }
        }

        if self.is_searching && !self.left_side_panes.is_empty() {
            let controls_x = 1;
            print_text_with_coordinates(enter_stage_panes, controls_x, list_y + self.left_side_panes.len() + 1, None, None);
            if self.selected_index.is_some() {
                print_text_with_coordinates(space_shortcut.clone(), controls_x, list_y + self.left_side_panes.len() + 2, None, None);
                print_text_with_coordinates(escape_shortcut.clone(), controls_x + space_shortcut_text.chars().count() + 1, list_y + self.left_side_panes.len() + 2, None, None);
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
