
use crate::{App, VisibilityAndFocus};
use zellij_tile::prelude::*;

const TOP_LEFT_CORNER_CHARACTER: &'static str = "┌";
const TOP_RIGHT_CORNER_CHARACTER: &'static str = "┐";
const BOTTOM_LEFT_CORNER_CHARACTER: &'static str = "└";
const BOTTOM_RIGHT_CORNER_CHARACTER: &'static str = "┘";
const BOUNDARY_CHARACTER: &'static str = "│";
const HORIZONTAL_BOUNDARY_CHARACTER: &'static str = "─";

#[derive(Debug, Clone)]
pub struct PaneItem {
    pub text: String,
    pub id: PaneId,
    pub color_indices: Vec<usize>,
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

// rendering code
impl App {
    pub fn render_close_shortcut(&self, cols: usize) {
        let should_render_close_shortcut = self.visibility_and_focus.left_side_is_focused() && self.marked_index.is_none();
        if should_render_close_shortcut {
            let x_coordinates_right_padding = if self.visibility_and_focus.only_left_side_is_focused() { 5 } else { 1 };
            let ctrl_c_shortcut_text = "<Ctrl c> - Close";
            let ctrl_c_shortcut = Text::new(ctrl_c_shortcut_text).color_range(3, ..=7);
            print_text_with_coordinates(ctrl_c_shortcut, cols.saturating_sub(ctrl_c_shortcut_text.chars().count()).saturating_sub(x_coordinates_right_padding), 0, None, None);
        }
    }
    pub fn render_tab_shortcut(&self, cols: usize, rows: usize) {
        match self.visibility_and_focus {
            VisibilityAndFocus::BothSidesVisibleRightSideFocused => {
                let side_width = self.calculate_side_width(cols);
                let tab_shortcut = Text::new("<TAB> - select more panes").color_range(3, ..=4);
                print_text_with_coordinates(tab_shortcut, side_width + 6, rows.saturating_sub(2), None, None);
            }
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused => {
                let side_width = self.calculate_side_width(cols);
                let tab_shortcut_text = "<TAB> - browse selected panes";
                let tab_shortcut = Text::new(tab_shortcut_text).color_range(3, ..=4);
                print_text_with_coordinates(tab_shortcut, side_width.saturating_sub(tab_shortcut_text.chars().count() + 1), rows.saturating_sub(2), None, None);
            }
            VisibilityAndFocus::OnlyRightSideVisible => {
                let tab_shortcut = Text::new("<TAB> - select more panes").color_range(3, ..=4);
                print_text_with_coordinates(tab_shortcut, 4, rows.saturating_sub(2), None, None);
            }
            VisibilityAndFocus::OnlyLeftSideVisible => {
                // not visible
            }
        };
    }
    pub fn render_left_side(&self, rows: usize, cols: usize, is_focused: bool) {
        let title_y = 0;
        let left_side_base_x = 1;
        let list_y = 2;
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
            if let Some(marked_index) = self.marked_index.as_ref().map(|i| i.main_index) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), left_side_base_x + 1, (list_y + marked_index).saturating_sub(extra_pane_count_on_top_left), None, None);
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
            if self.marked_index.is_some() {
                print_text_with_coordinates(space_shortcut.clone(), controls_x, list_y + left_side_panes.len() + 2, None, None);
                print_text_with_coordinates(escape_shortcut.clone(), controls_x + space_shortcut_text.chars().count() + 1, list_y + left_side_panes.len() + 2, None, None);
            }
        }
    }
    pub fn render_right_side(&self, rows: usize, cols: usize, is_focused: bool) {
        let side_width = self.calculate_side_width(cols);
        let right_side_base_x = match self.visibility_and_focus {
            VisibilityAndFocus::OnlyLeftSideVisible | VisibilityAndFocus::OnlyRightSideVisible => 1,
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused | VisibilityAndFocus::BothSidesVisibleRightSideFocused => side_width + 4,
        };
        let title_y = 0;
        let list_y: usize = 2;
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
            if let Some(marked_index) = self.marked_index.as_ref().map(|i| i.main_index) {
                print_text_with_coordinates(Text::new(">").color_range(3, ..).selected(), right_side_base_x+ 1, (list_y + marked_index).saturating_sub(extra_pane_count_on_top_right), None, None);
            }
        }
        if is_focused && !self.right_side_panes.is_empty() {
            print_text_with_coordinates(right_side_controls_1, right_side_base_x + 1, list_y + right_side_pane_count + 1, None, None);
            print_text_with_coordinates(right_side_controls_2, right_side_base_x + 1, list_y + right_side_pane_count + 3, None, None);
            print_text_with_coordinates(right_side_controls_3, right_side_base_x + 1, list_y + right_side_pane_count + 4, None, None);
            print_text_with_coordinates(right_side_controls_4, right_side_base_x + 1, list_y + right_side_pane_count + 5, None, None);
        }
    }
    pub fn render_help_line(&self, rows: usize, cols: usize) {
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
    pub fn render_focus_boundary(&self, rows: usize, cols: usize) {
        let side_width = self.calculate_side_width(cols);
        let x = match self.visibility_and_focus {
            VisibilityAndFocus::OnlyRightSideVisible => 0,
            VisibilityAndFocus::BothSidesVisibleLeftSideFocused |
            VisibilityAndFocus::BothSidesVisibleRightSideFocused |
            VisibilityAndFocus::OnlyLeftSideVisible => side_width + 2
        };
        let y = 0;
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
    pub fn calculate_side_width(&self, cols: usize) -> usize {
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

// ui components
impl App {
    fn filter_panes_prompt(&self) -> (&'static str, Text) {
        let search_prompt_text = if self.search_string.is_empty() { "ALL PANES " } else { "FILTER: " };
        let search_prompt = if self.visibility_and_focus.left_side_is_focused() { Text::new(&search_prompt_text).color_range(2, ..) } else { Text::new(&search_prompt_text) };
        (search_prompt_text, search_prompt)
    }
    fn filter(&self, max_width: usize) -> Text {
        let search_string_text = if self.marked_index.is_none() && self.search_string.is_empty() {
            let full = "[Type filter term...]";
            let short = "[...]";
            if max_width >= full.chars().count() {
                full.to_owned()
            } else {
                short.to_owned()
            }
        } else if self.marked_index.is_none() && !self.search_string.is_empty() {
            if max_width >= self.search_string.chars().count() + 1 {
                format!("{}_", self.search_string)
            } else {
                let truncated: String = self.search_string.chars().rev().take(max_width.saturating_sub(4)).collect::<Vec<_>>().iter().rev().collect();
                format!("...{}_", truncated)
            }
        } else if self.marked_index.is_some() && !self.search_string.is_empty() {
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
                .marked_index
                .as_ref()
                .map(|s| s.main_index.saturating_sub(max_list_height / 2))
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
            if Some(i) == self.marked_index.as_ref().map(|s| s.main_index) && self.visibility_and_focus.left_side_is_focused() {
                item = item.selected();
                if self.marked_index.as_ref().map(|s| s.additional_indices.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.marked_index.as_ref().map(|s| s.additional_indices.contains(&i)).unwrap_or(false) && self.visibility_and_focus.left_side_is_focused() {
                item = item.selected();
            }
            left_side_panes.push(item);
        }
        let extra_panes_on_top = first_item_index;
        let extra_panes_on_bottom = item_count.saturating_sub(last_item_index + 1);
        let extra_selected_item_count_on_top = if self.visibility_and_focus.left_side_is_focused() {
            self.marked_index.as_ref().map(|s| s.additional_indices.iter().filter(|a| a < &&first_item_index).count()).unwrap_or(0)
        } else {
            0
        };
        let extra_selected_item_count_on_bottom = if self.visibility_and_focus.left_side_is_focused() {
            self.marked_index.as_ref().map(|s| s.additional_indices.iter().filter(|a| a > &&last_item_index).count()).unwrap_or(0)
        } else {
            0
        };
        (extra_panes_on_top, extra_panes_on_bottom, extra_selected_item_count_on_top, extra_selected_item_count_on_bottom, left_side_panes)
    }
    fn left_side_controls(&self, max_width: usize) -> (&'static str, Text, &'static str, Text, &'static str, Text) {
        // returns three components and their text
        let (enter_select_panes_text, enter_select_panes) = if self.marked_index.is_some() {
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
        if self.marked_index.is_some() {
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
                .marked_index
                .as_ref()
                .map(|s| s.main_index.saturating_sub(max_list_height / 2))
                .unwrap_or(0)
        };
        let last_item_index = std::cmp::min((max_list_height + first_item_index).saturating_sub(1), item_count.saturating_sub(1));

        let max_width_for_item = max_width.saturating_sub(3); // 3 for the list bulletin
        for (i, pane_item) in self.right_side_panes.iter().enumerate().skip(first_item_index) {
            if i > last_item_index {
                break;
            }
            let mut item = pane_item.render(max_width_for_item);
            if &Some(i) == &self.marked_index.as_ref().map(|s| s.main_index) && self.visibility_and_focus.right_side_is_focused() {
                item = item.selected();
                if self.marked_index.as_ref().map(|s| s.additional_indices.contains(&i)).unwrap_or(false) {
                    item = item.selected().color_range(1, ..);
                }
            } else if self.marked_index.as_ref().map(|s| s.additional_indices.contains(&i)).unwrap_or(false) && self.visibility_and_focus.right_side_is_focused() {
                item = item.selected();
            }
            right_side_panes.push(item);
        }

        let extra_panes_on_top = first_item_index;
        let extra_panes_on_bottom = self.right_side_panes.iter().len().saturating_sub(last_item_index + 1);
        let extra_selected_item_count_on_top = if self.visibility_and_focus.left_side_is_focused() {
            0
        } else {
            self.marked_index.as_ref().map(|s| s.additional_indices.iter().filter(|a| a < &&first_item_index).count()).unwrap_or(0)
        };
        let extra_selected_item_count_on_bottom = if self.visibility_and_focus.left_side_is_focused() {
            0
        } else {
            self.marked_index.as_ref().map(|s| s.additional_indices.iter().filter(|a| a > &&last_item_index).count()).unwrap_or(0)
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
