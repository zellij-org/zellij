mod errors;
mod screens;
mod text_input;
mod ui;

use errors::format_kdl_error;
use screens::{KeyResponse, OptimisticUpdate, Screen};
use std::collections::BTreeMap;
use ui::{get_last_modified_string, get_layout_display_info};
use zellij_tile::prelude::*;

#[derive(Clone)]
pub enum DisplayLayout {
    Valid(LayoutInfo),
    Error {
        name: String,
        error: String,
        error_message: String,
    },
}

impl DisplayLayout {
    fn name(&self) -> String {
        match self {
            DisplayLayout::Valid(info) => match info {
                LayoutInfo::BuiltIn(name) => name.clone(),
                LayoutInfo::File(path, _) => path.split('/').last().unwrap_or(path).to_string(),
                LayoutInfo::Url(url) => url.split('/').last().unwrap_or(url).to_string(),
                LayoutInfo::Stringified(_) => "raw".to_string(),
            },
            DisplayLayout::Error { name, .. } => name.clone(),
        }
    }
    fn file_name(&self) -> Option<String> {
        match self {
            DisplayLayout::Valid(LayoutInfo::File(file_name, _)) => Some(file_name.clone()),
            DisplayLayout::Error { name, .. } => Some(name.clone()),
            _ => None,
        }
    }
    fn is_error(&self) -> bool {
        match self {
            DisplayLayout::Error { .. } => true,
            _ => false,
        }
    }
    fn is_builtin(&self) -> bool {
        matches!(self, DisplayLayout::Valid(LayoutInfo::BuiltIn(..)))
    }

    fn builtin_sort_priority(&self) -> usize {
        let name = self.name().to_lowercase();
        match name.as_str() {
            "default" => 0,
            "compact" => 1,
            _ => 2,
        }
    }

    fn get_update_time(&self) -> Option<i64> {
        match self {
            DisplayLayout::Valid(LayoutInfo::File(_, metadata)) => {
                metadata.update_time.parse::<i64>().ok()
            },
            _ => None,
        }
    }

    fn compare_builtin_layouts(&self, other: &DisplayLayout) -> std::cmp::Ordering {
        match self
            .builtin_sort_priority()
            .cmp(&other.builtin_sort_priority())
        {
            std::cmp::Ordering::Equal => {
                self.name().to_lowercase().cmp(&other.name().to_lowercase())
            },
            priority_order => priority_order,
        }
    }

    fn compare_for_display(&self, other: &DisplayLayout) -> std::cmp::Ordering {
        match (self.is_builtin(), other.is_builtin()) {
            (false, false) => {
                // Sort by update_time (newest first)
                match (self.get_update_time(), other.get_update_time()) {
                    (Some(self_time), Some(other_time)) => {
                        // Reverse comparison for descending order
                        other_time.cmp(&self_time)
                    },
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => {
                        // Fallback to alphabetical
                        self.name().to_lowercase().cmp(&other.name().to_lowercase())
                    },
                }
            },
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (true, true) => self.compare_builtin_layouts(other),
        }
    }

    fn name_width(&self) -> usize {
        self.name().chars().count()
    }
    fn last_modified_width(&self) -> usize {
        let display_info = get_layout_display_info(&self);
        get_last_modified_string(display_info.1, self.is_builtin())
            .chars()
            .count()
    }
}

#[derive(Default)]
struct State {
    display_layouts: Vec<DisplayLayout>,
    screen: Screen,
}

impl State {
    fn handle_key_event(&mut self, key: KeyWithModifier) -> KeyResponse {
        match &mut self.screen {
            Screen::LayoutList(layout_list_screen) => {
                layout_list_screen.handle_key(key, &self.display_layouts)
            },
            Screen::NewLayoutFromSession(new_layout_from_session_screen) => {
                new_layout_from_session_screen.handle_key(key)
            },
            Screen::ImportLayout(import_layout_screen) => import_layout_screen.handle_key(key),
            Screen::RenameLayout(rename_layout_screen) => rename_layout_screen.handle_key(key),
            Screen::Error(ref mut error_screen) => error_screen.handle_key(key),
            Screen::ErrorDetail(ref mut error_detail_screen) => error_detail_screen.handle_key(key),
        }
    }

    fn apply_key_response(&mut self, response: KeyResponse) -> bool {
        let mut should_render = false;

        if let Some(update) = response.optimistic_update {
            self.apply_optimistic_update(update);
            should_render = true;
        }

        if let Some(new_screen) = response.new_screen {
            self.screen = new_screen;
        }

        should_render || response.should_render
    }

    fn apply_optimistic_update(&mut self, update: OptimisticUpdate) {
        match update {
            OptimisticUpdate::Delete(file_name) => {
                self.optimistic_delete_layout(&file_name);
            },
            OptimisticUpdate::Rename { old_name, new_name } => {
                self.optimistic_rename_layout(&old_name, &new_name);
            },
            OptimisticUpdate::Add { name, metadata } => {
                self.optimistic_add_layout(name, metadata);
            },
        }
    }

    fn optimistic_add_layout(&mut self, name: String, metadata: LayoutMetadata) {
        let placeholder = DisplayLayout::Valid(LayoutInfo::File(name, metadata));

        self.display_layouts.push(placeholder);
        self.display_layouts
            .sort_by(|a, b| a.compare_for_display(b));

        if let Screen::LayoutList(ref mut screen) = self.screen {
            screen.update_selected_index(&self.display_layouts);
        }
    }

    fn optimistic_delete_layout(&mut self, file_name: &str) {
        self.display_layouts
            .retain(|layout| layout.file_name() != Some(file_name.to_string()));

        if let Screen::LayoutList(ref mut screen) = self.screen {
            screen.update_selected_index(&self.display_layouts);
        }
    }

    fn optimistic_rename_layout(&mut self, old_name: &str, new_name: &str) {
        for layout in &mut self.display_layouts {
            match layout {
                DisplayLayout::Valid(LayoutInfo::File(file_name, _)) => {
                    if file_name == old_name {
                        let _ = std::mem::replace(file_name, new_name.to_string());
                        break;
                    }
                },
                DisplayLayout::Error { ref mut name, .. } => {
                    if name == old_name {
                        let _ = std::mem::replace(name, new_name.to_string());
                        break;
                    }
                },
                _ => {},
            }
        }

        self.display_layouts
            .sort_by(|a, b| a.compare_for_display(b));

        if let Screen::LayoutList(ref mut screen) = self.screen {
            screen.update_selected_index(&self.display_layouts);
        }
    }
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        subscribe(&[
            EventType::PastedText,
            EventType::SessionUpdate,
            EventType::Key,
            EventType::AvailableLayoutInfo,
            EventType::PermissionRequestResult,
        ]);
        rename_plugin_pane(get_plugin_ids().plugin_id, "Layout Manager");
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;

        match event {
            Event::PastedText(text) => {
                if let Screen::ImportLayout(ref mut import_screen) = self.screen {
                    import_screen.handle_pasted_text(text);
                    should_render = true;
                }
            },
            Event::AvailableLayoutInfo(available_layouts, layouts_with_errors) => {
                // Convert valid layouts to DisplayLayout::Valid
                let mut display_layouts: Vec<DisplayLayout> = available_layouts
                    .into_iter()
                    .map(DisplayLayout::Valid)
                    .collect();

                // Convert error layouts to DisplayLayout::Error
                for layout_error in &layouts_with_errors {
                    let stringified_error = format_kdl_error(layout_error.error.clone());
                    let error_message = match &layout_error.error {
                        LayoutParsingError::KdlError { kdl_error, .. } => {
                            format!("Layout Error: {}", kdl_error.error_message)
                        },
                        LayoutParsingError::SyntaxError => "KDL parsing error".to_owned(),
                    };

                    display_layouts.push(DisplayLayout::Error {
                        name: layout_error.layout_name.clone(),
                        error_message,
                        error: stringified_error,
                    });
                }

                display_layouts.sort_by(|a, b| a.compare_for_display(b));

                self.display_layouts = display_layouts;
                if let Screen::LayoutList(ref mut screen) = self.screen {
                    screen.update_selected_index(&self.display_layouts);
                }
                should_render = true;
            },
            Event::Key(key) => {
                let response = self.handle_key_event(key);
                should_render = self.apply_key_response(response);
            },
            _ => {},
        }

        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        match &mut self.screen {
            Screen::LayoutList(layout_list_screen) => {
                layout_list_screen.render(&self.display_layouts, rows, cols)
            },
            Screen::NewLayoutFromSession(new_layout_from_session_screen) => {
                new_layout_from_session_screen.render(rows, cols)
            },
            Screen::ImportLayout(import_layout_screen) => import_layout_screen.render(rows, cols),
            Screen::RenameLayout(rename_layout_screen) => rename_layout_screen.render(rows, cols),
            Screen::Error(ref error_screen) => error_screen.render(rows, cols),
            Screen::ErrorDetail(ref error_detail_screen) => error_detail_screen.render(rows, cols),
        }
    }
}
