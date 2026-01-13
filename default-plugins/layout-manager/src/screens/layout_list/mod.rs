mod search;

use super::{KeyResponse, OptimisticUpdate, Screen};
use crate::text_input::InputAction;
use crate::ui::{Controls, LayoutDetail, LayoutsTable};
use crate::DisplayLayout;
use search::SearchState;
use zellij_tile::prelude::*;

#[derive(Clone)]
pub struct LayoutListScreen {
    pub selected_layout_index: usize,
    retain_terminal_panes: bool,
    retain_plugin_panes: bool,
    apply_only_to_active_tab: bool,
    show_more_override_options: bool,
    search_state: SearchState,
    last_rows: usize,
    last_cols: usize,
}

impl Default for LayoutListScreen {
    fn default() -> Self {
        Self {
            selected_layout_index: 0,
            retain_terminal_panes: false,
            retain_plugin_panes: false,
            apply_only_to_active_tab: false,
            show_more_override_options: false,
            search_state: SearchState::new(),
            last_rows: 0,
            last_cols: 0,
        }
    }
}

impl LayoutListScreen {
    pub fn with_selected_index(selected_index: usize) -> Self {
        Self {
            selected_layout_index: selected_index,
            retain_terminal_panes: true,
            retain_plugin_panes: false,
            apply_only_to_active_tab: false,
            show_more_override_options: false,
            search_state: SearchState::new(),
            last_rows: 0,
            last_cols: 0,
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyWithModifier,
        display_layouts: &[DisplayLayout],
    ) -> KeyResponse {
        if self.search_state.is_typing() {
            self.handle_typing_filter_mode(key, display_layouts)
        } else {
            match key.bare_key {
                BareKey::Char('/') if key.has_no_modifiers() => {
                    self.search_state.start_typing();
                    self.update_filter(display_layouts);
                    KeyResponse::render()
                },
                BareKey::Delete if key.has_no_modifiers() => {
                    if let Some(file_name) = self.get_selected_file_name(display_layouts) {
                        let _ = delete_layout(file_name.clone());
                        KeyResponse::none().with_optimistic(OptimisticUpdate::Delete(file_name))
                    } else {
                        KeyResponse::none()
                    }
                },
                BareKey::Char('e') if key.has_no_modifiers() => {
                    self.edit_selected_layout(display_layouts);
                    KeyResponse::none()
                },
                BareKey::Char('i') if key.has_no_modifiers() => {
                    let new_screen = self.start_import_layout();
                    KeyResponse::new_screen(new_screen)
                },
                BareKey::Char('n') if key.has_no_modifiers() => {
                    let new_screen = self.start_new_layout_creation_from_session();
                    KeyResponse::new_screen(new_screen)
                },
                BareKey::Char('r') if key.has_no_modifiers() => {
                    if let Some(new_screen) = self.start_rename_layout(display_layouts) {
                        KeyResponse::new_screen(new_screen)
                    } else {
                        KeyResponse::render()
                    }
                },
                BareKey::Up if key.has_no_modifiers() => {
                    self.navigate_up(display_layouts);
                    KeyResponse::render()
                },
                BareKey::Down if key.has_no_modifiers() => {
                    self.navigate_down(display_layouts);
                    KeyResponse::render()
                },
                BareKey::Enter if key.has_no_modifiers() => {
                    self.open_selected_layout(display_layouts);
                    KeyResponse::none()
                },
                BareKey::Tab if key.has_no_modifiers() => {
                    self.apply_selected_layout(display_layouts);
                    KeyResponse::none()
                },
                BareKey::Char('t') if key.has_no_modifiers() => {
                    self.toggle_retain_options();
                    KeyResponse::render()
                },
                BareKey::Char('a') if key.has_no_modifiers() => {
                    self.toggle_target_option();
                    KeyResponse::render()
                },
                BareKey::Char('m') if key.has_no_modifiers() => {
                    if let Some(new_screen) = self.show_error_detail(display_layouts) {
                        KeyResponse::new_screen(new_screen)
                    } else {
                        KeyResponse::render()
                    }
                },
                BareKey::Char('?') if key.has_no_modifiers() => {
                    self.show_more_override_options = !self.show_more_override_options;
                    KeyResponse::render()
                },
                BareKey::Esc if key.has_no_modifiers() => {
                    self.clear_filter();
                    KeyResponse::render()
                },
                _ => KeyResponse::none(),
            }
        }
    }

    fn handle_typing_filter_mode(
        &mut self,
        key: KeyWithModifier,
        display_layouts: &[DisplayLayout],
    ) -> KeyResponse {
        // Special handling for Enter and Esc
        match key.bare_key {
            BareKey::Esc if key.has_no_modifiers() => {
                self.clear_filter();
                return KeyResponse::render();
            },
            BareKey::Enter if key.has_no_modifiers() => {
                if self.search_state.get_filter_input().is_empty()
                    || self.search_state.get_search_results().is_empty()
                {
                    self.clear_filter();
                } else {
                    self.search_state.stop_typing();
                    show_cursor(None);
                }
                return KeyResponse::render();
            },
            _ => {},
        }

        // Pass all keys to TextInput
        let action = self.search_state.get_filter_input_mut().handle_key(key);

        match action {
            InputAction::Continue => {
                self.update_filter(display_layouts);
                KeyResponse::render()
            },
            InputAction::Cancel => {
                // Ctrl-C or Esc - clear filter
                self.clear_filter();
                KeyResponse::render()
            },
            InputAction::Submit => {
                // Enter handled above
                KeyResponse::none()
            },
            InputAction::Complete => KeyResponse::none(),
            InputAction::NoAction => KeyResponse::none(),
        }
    }

    fn edit_selected_layout(&self, display_layouts: &[DisplayLayout]) {
        if let Some(file_name) = self.get_selected_file_name(display_layouts) {
            let _ = edit_layout(file_name, Default::default());
        }
    }

    fn get_selected_file_name(&self, display_layouts: &[DisplayLayout]) -> Option<String> {
        display_layouts
            .get(self.selected_layout_index)
            .and_then(|layout| layout.file_name())
    }

    fn start_new_layout_creation_from_session(&self) -> Screen {
        match dump_session_layout() {
            Ok((session_layout, Some(session_layout_metadata))) => {
                Screen::NewLayoutFromSession(super::NewLayoutFromCurrentSessionScreen {
                    name_input: crate::text_input::TextInput::empty(),
                    session_layout,
                    current_layout_metadata: session_layout_metadata,
                    editing_name: false,
                    save_current_tab_only: false,
                })
            },
            Ok((_, None)) => Screen::Error(super::ErrorScreen {
                message: "Failed to retrieve session layout metadata".to_string(),
                return_to_screen: Box::new(Screen::LayoutList(self.clone())),
            }),
            Err(error_msg) => Screen::Error(super::ErrorScreen {
                message: format!("Failed to dump session layout: {}", error_msg),
                return_to_screen: Box::new(Screen::LayoutList(self.clone())),
            }),
        }
    }

    fn start_import_layout(&self) -> Screen {
        Screen::ImportLayout(super::ImportLayoutScreen::new())
    }

    fn start_rename_layout(&self, display_layouts: &[DisplayLayout]) -> Option<Screen> {
        if let Some(layout_file_name) = self.get_selected_file_name(display_layouts) {
            Some(Screen::RenameLayout(super::RenameLayoutScreen::new(
                layout_file_name,
                self.selected_layout_index,
            )))
        } else {
            None
        }
    }

    fn navigate_up(&mut self, display_layouts: &[DisplayLayout]) {
        if self.is_searching() {
            // Navigate in search results
            if self.search_state.get_selected_search_index() == 0 {
                self.search_state.set_selected_search_index(
                    self.search_state
                        .get_search_results()
                        .len()
                        .saturating_sub(1),
                );
            } else {
                let new_index = self
                    .search_state
                    .get_selected_search_index()
                    .saturating_sub(1);
                self.search_state.set_selected_search_index(new_index);
            }
            // Keep selected_layout_index synchronized with the actual position
            self.selected_layout_index = self
                .search_state
                .get_search_results()
                .get(self.search_state.get_selected_search_index())
                .map(|r| r.original_index)
                .unwrap_or(0);
        } else {
            // Normal navigation
            if self.selected_layout_index == 0 {
                self.selected_layout_index = display_layouts.len().saturating_sub(1);
            } else {
                self.selected_layout_index -= 1;
            }
        }
        self.apply_only_to_active_tab = self.should_default_to_current_tab(display_layouts);
    }

    fn navigate_down(&mut self, display_layouts: &[DisplayLayout]) {
        if self.is_searching() {
            // Navigate in search results
            if self.search_state.get_selected_search_index() + 1
                >= self.search_state.get_search_results().len()
            {
                self.search_state.set_selected_search_index(0);
            } else {
                let new_index = self.search_state.get_selected_search_index() + 1;
                self.search_state.set_selected_search_index(new_index);
            }
            // Keep selected_layout_index synchronized with the actual position
            self.selected_layout_index = self
                .search_state
                .get_search_results()
                .get(self.search_state.get_selected_search_index())
                .map(|r| r.original_index)
                .unwrap_or(0);
        } else {
            // Normal navigation
            if self.selected_layout_index + 1 >= display_layouts.len() {
                self.selected_layout_index = 0;
            } else {
                self.selected_layout_index += 1;
            }
        }
        self.apply_only_to_active_tab = self.should_default_to_current_tab(display_layouts);
    }

    fn apply_selected_layout(&self, display_layouts: &[DisplayLayout]) {
        if let Some(DisplayLayout::Valid(chosen_layout)) =
            display_layouts.get(self.selected_layout_index)
        {
            override_layout(
                chosen_layout,
                self.retain_terminal_panes,
                self.retain_plugin_panes,
                self.apply_only_to_active_tab,
                Default::default(),
            );
        }
    }

    fn toggle_retain_options(&mut self) {
        // Cycle through: Terminals -> Plugins -> Both -> None -> (back to Terminals)
        match (self.retain_terminal_panes, self.retain_plugin_panes) {
            (true, false) => {
                // Terminals -> Plugins
                self.retain_terminal_panes = false;
                self.retain_plugin_panes = true;
            },
            (false, true) => {
                // Plugins -> Both
                self.retain_terminal_panes = true;
                self.retain_plugin_panes = true;
            },
            (true, true) => {
                // Both -> None
                self.retain_terminal_panes = false;
                self.retain_plugin_panes = false;
            },
            (false, false) => {
                // None -> Terminals
                self.retain_terminal_panes = true;
                self.retain_plugin_panes = false;
            },
        }
    }

    fn toggle_target_option(&mut self) {
        // Toggle between: All Tabs (false) <-> Current (true)
        self.apply_only_to_active_tab = !self.apply_only_to_active_tab;
    }

    fn show_error_detail(&self, display_layouts: &[DisplayLayout]) -> Option<Screen> {
        if let Some(DisplayLayout::Error {
            name,
            error,
            error_message: _,
        }) = display_layouts.get(self.selected_layout_index)
        {
            Some(Screen::ErrorDetail(super::ErrorDetailScreen::new(
                name.clone(),
                error.clone(),
                Box::new(Screen::LayoutList(self.clone())),
            )))
        } else {
            None
        }
    }

    fn should_default_to_current_tab(&self, display_layouts: &[DisplayLayout]) -> bool {
        match display_layouts.get(self.selected_layout_index) {
            Some(DisplayLayout::Valid(LayoutInfo::BuiltIn(_))) => true,
            Some(DisplayLayout::Valid(LayoutInfo::File(_, metadata))) => metadata.tabs.len() == 1,
            _ => false,
        }
    }

    fn open_selected_layout(&self, display_layouts: &[DisplayLayout]) {
        if let Some(DisplayLayout::Valid(chosen_layout)) =
            display_layouts.get(self.selected_layout_index)
        {
            new_tabs_with_layout_info(chosen_layout);
        }
    }

    pub fn render(&mut self, display_layouts: &[DisplayLayout], rows: usize, cols: usize) {
        show_cursor(None);
        // Store terminal dimensions for cursor positioning
        self.last_rows = rows;
        self.last_cols = cols;

        self.update_filter(display_layouts);

        // Calculate base coordinates using the full display_layouts (NOT filtered results)
        let (total_width, total_height) =
            self.calculate_content_dimensions(rows, cols, display_layouts);
        let (base_x, base_y) =
            self.calculate_base_coordinates(rows, cols, total_width, total_height);

        let layouts_to_render = self.effective_layouts(display_layouts);
        let (content_height, controls_y) = self.calculate_layout(rows, &display_layouts);

        let controls = Controls::new(
            self.retain_terminal_panes,
            self.retain_plugin_panes,
            self.apply_only_to_active_tab,
            self.show_more_override_options,
            self.search_state.is_typing(),
            self.search_state.is_active(),
        );
        let controls_width = controls.calculate_width(cols);

        let (table_width, detail_x) =
            self.calculate_horizontal_split(cols, display_layouts, controls_width);

        let table_y = if self.search_state.is_active() || self.search_state.is_typing() {
            1 + base_y
        } else {
            base_y
        };

        if self.search_state.is_active() || self.search_state.is_typing() {
            self.search_state.render_filter_line(base_x, base_y);
        }

        let selected_index = self.effective_selected_index();

        let (visible_layouts, selected_in_window, hidden_above, hidden_below) =
            calculate_visible_window(
                &layouts_to_render,
                selected_index,
                content_height.saturating_sub(1), // account for title row
            );

        let matched_indices = self
            .search_state
            .get_matched_indices_for_visible(hidden_above, visible_layouts.len());

        let has_visible_layouts = !visible_layouts.is_empty();
        LayoutsTable::new(
            visible_layouts,
            selected_in_window,
            hidden_above,
            hidden_below,
        )
        .with_matched_indices(matched_indices)
        .render(
            base_x,
            table_y,
            content_height.saturating_sub(1),
            table_width,
        );

        if has_visible_layouts {
            if let Some(selected_layout) = display_layouts.get(self.selected_layout_index) {
                LayoutDetail::new(selected_layout).render(
                    detail_x + base_x,
                    table_y + 1,
                    content_height.saturating_sub(1),
                    table_width,
                );
            } else {
                // Render "No layout selected" message (moved from LayoutDetail)
                let msg = Text::new("No layout selected").color_all(2);
                print_text_with_coordinates(msg, detail_x + base_x, table_y, None, None);
            }
        }

        controls.render(base_x, controls_y + base_y, cols);
    }

    fn calculate_layout(&self, rows: usize, display_layouts: &[DisplayLayout]) -> (usize, usize) {
        let rows_in_table = display_layouts.len() + 1; // 1 for the title row
        let controls_height = self.get_controls_height();
        let filter_row_height = if self.is_searching() { 1 } else { 0 };
        let padding = 1;
        let mut content_height = std::cmp::max(rows_in_table, 5);
        if content_height + controls_height + padding + filter_row_height >= rows {
            content_height = rows
                .saturating_sub(controls_height)
                .saturating_sub(padding)
                .saturating_sub(filter_row_height)
        }
        let controls_y = content_height + padding + filter_row_height;

        (content_height, controls_y)
    }

    fn get_controls_height(&self) -> usize {
        if self.show_more_override_options {
            5
        } else {
            3
        }
    }

    fn calculate_horizontal_split(
        &self,
        cols: usize,
        display_layouts: &[DisplayLayout],
        controls_width: usize,
    ) -> (usize, usize) {
        let padding = 2;
        let widest_layout_name = display_layouts
            .iter()
            .map(|layout| layout.name_width())
            .max()
            .unwrap_or(0);
        let widest_last_modified = display_layouts
            .iter()
            .map(|layout| layout.last_modified_width())
            .max()
            .unwrap_or(0);

        let content_based_table_width = std::cmp::min(
            (cols / 2).saturating_sub(padding),
            widest_layout_name + widest_last_modified + padding,
        );

        let controls_based_table_width = controls_width.saturating_sub(padding) / 2;

        let table_width = content_based_table_width.max(controls_based_table_width);
        let detail_x = table_width.saturating_add(padding);

        (table_width, detail_x)
    }

    fn calculate_content_dimensions(
        &self,
        rows: usize,
        cols: usize,
        display_layouts: &[DisplayLayout],
    ) -> (usize, usize) {
        let filter_row_height = if self.is_searching() { 1 } else { 0 };
        let (content_height, _) = self.calculate_layout(rows, display_layouts);
        let padding = 1;
        let controls_height = self.get_controls_height();
        let total_height = filter_row_height + content_height + padding + controls_height;

        let controls = Controls::new(
            self.retain_terminal_panes,
            self.retain_plugin_panes,
            self.apply_only_to_active_tab,
            self.show_more_override_options,
            self.search_state.is_typing(),
            self.search_state.is_active(),
        );
        let controls_width = controls.calculate_width(cols);

        let (table_width, detail_x) =
            self.calculate_horizontal_split(cols, display_layouts, controls_width);

        let layout_width = detail_x.saturating_add(table_width);
        let total_width = layout_width.max(controls_width);

        (total_width, total_height)
    }

    fn calculate_base_coordinates(
        &self,
        rows: usize,
        cols: usize,
        total_width: usize,
        total_height: usize,
    ) -> (usize, usize) {
        let base_x = if total_width < cols {
            (cols.saturating_sub(total_width)) / 2
        } else {
            0
        };
        let base_y = if total_height < rows {
            (rows.saturating_sub(total_height)) / 2
        } else {
            0
        };
        (base_x, base_y)
    }

    pub fn update_selected_index(&mut self, display_layouts: &[DisplayLayout]) {
        if self.selected_layout_index >= display_layouts.len() {
            self.selected_layout_index = display_layouts.len().saturating_sub(1);
        }

        // Refresh filter results if filtering is active
        if self.search_state.is_active() && !self.search_state.get_filter_input().is_empty() {
            self.update_filter(display_layouts);
        }

        self.apply_only_to_active_tab = self.should_default_to_current_tab(display_layouts);
    }

    fn is_searching(&self) -> bool {
        (self.search_state.is_active() && !self.search_state.get_search_results().is_empty())
            || self.search_state.is_typing()
    }

    fn effective_selected_index(&self) -> usize {
        if self.is_searching() {
            self.search_state.get_selected_search_index()
        } else {
            self.selected_layout_index
        }
    }

    fn effective_layouts(&self, display_layouts: &[DisplayLayout]) -> Vec<DisplayLayout> {
        if self.is_searching() {
            self.search_state
                .get_search_results()
                .iter()
                .map(|r| r.layout.clone())
                .collect()
        } else {
            display_layouts.to_vec()
        }
    }

    fn update_filter(&mut self, display_layouts: &[DisplayLayout]) {
        let (total_width, total_height) =
            self.calculate_content_dimensions(self.last_rows, self.last_cols, display_layouts);
        let (base_x, base_y) = self.calculate_base_coordinates(
            self.last_rows,
            self.last_cols,
            total_width,
            total_height,
        );

        self.search_state
            .update_filter(display_layouts, base_x, base_y);

        // Update selected_layout_index to match the selected search result
        if let Some(original_index) = self.search_state.get_current_selected_original_index() {
            self.selected_layout_index = original_index;
        }
    }

    fn clear_filter(&mut self) {
        self.search_state.clear_filter();
        self.selected_layout_index = 0;
    }
}

fn calculate_visible_window(
    layouts: &[DisplayLayout],
    selected_index: usize,
    max_visible: usize,
) -> (Vec<DisplayLayout>, usize, usize, usize) {
    // returns: (index_in_rendered, hidden_items_above, hidden_items_below)
    if layouts.is_empty() || max_visible == 0 {
        return (Vec::new(), 0, 0, 0);
    }

    if layouts.len() <= max_visible {
        return (layouts.to_vec(), selected_index, 0, 0);
    }

    let Some(selected_layout) = layouts.get(selected_index) else {
        return (Vec::new(), 0, 0, 0);
    };

    let mut visible = Vec::with_capacity(max_visible);
    visible.push(selected_layout.clone());

    let mut before = selected_index;
    let mut after = selected_index;
    let mut take_before = true;

    while visible.len() < max_visible {
        if take_before && before > 0 {
            before = before.saturating_sub(1);
            if let Some(layout) = layouts.get(before) {
                visible.insert(0, layout.clone());
            }
        } else if !take_before && after + 1 < layouts.len() {
            after += 1;
            if let Some(layout) = layouts.get(after) {
                visible.push(layout.clone());
            }
        } else if before > 0 {
            before = before.saturating_sub(1);
            if let Some(layout) = layouts.get(before) {
                visible.insert(0, layout.clone());
            }
        } else if after + 1 < layouts.len() {
            after += 1;
            if let Some(layout) = layouts.get(after) {
                visible.push(layout.clone());
            }
        } else {
            break;
        }
        take_before = !take_before;
    }

    let adjusted_index = selected_index.saturating_sub(before);
    (
        visible,
        adjusted_index,
        before,
        layouts.len().saturating_sub(after + 1),
    )
}
