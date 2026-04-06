use super::{ErrorScreen, KeyResponse, LayoutListScreen, OptimisticUpdate, Screen};
use crate::text_input::{InputAction, TextInput};
use crate::ui::truncate_with_ellipsis_start;
use zellij_tile::prelude::*;

#[derive(Clone)]
pub struct RenameLayoutScreen {
    pub name_input: TextInput,
    pub old_file_name: String,
    pub selected_layout_index: usize,
}

impl RenameLayoutScreen {
    pub fn new(old_file_name: String, selected_layout_index: usize) -> Self {
        Self {
            name_input: TextInput::new(old_file_name.clone()),
            old_file_name,
            selected_layout_index,
        }
    }

    pub fn handle_key(&mut self, key: KeyWithModifier) -> KeyResponse {
        let action = self.name_input.handle_key(key);

        match action {
            InputAction::Continue => KeyResponse::render(),
            InputAction::Submit => self.attempt_rename(),
            InputAction::Cancel => KeyResponse::new_screen(self.cancel_rename()),
            InputAction::Complete => KeyResponse::none(),
            InputAction::NoAction => KeyResponse::none(),
        }
    }

    fn cancel_rename(&self) -> Screen {
        Screen::LayoutList(LayoutListScreen::with_selected_index(
            self.selected_layout_index,
        ))
    }

    fn attempt_rename(&self) -> KeyResponse {
        let name_input_text = self.name_input.get_text();

        if name_input_text.is_empty() {
            show_cursor(None);
            return KeyResponse::new_screen(
                self.create_error_screen("Layout name cannot be empty"),
            );
        }

        let optimistic = OptimisticUpdate::Rename {
            old_name: self.old_file_name.clone(),
            new_name: name_input_text.to_string(),
        };

        match rename_layout(&self.old_file_name, name_input_text) {
            Ok(_) => KeyResponse::new_screen(Screen::LayoutList(
                LayoutListScreen::with_selected_index(self.selected_layout_index),
            ))
            .with_optimistic(optimistic),
            Err(error_msg) => KeyResponse::new_screen(self.create_error_screen(&error_msg)),
        }
    }

    fn create_error_screen(&self, message: &str) -> Screen {
        show_cursor(None);
        Screen::Error(ErrorScreen {
            message: message.to_string(),
            return_to_screen: Box::new(Screen::RenameLayout(self.clone())),
        })
    }

    fn rename_line_text(&self, max_width: Option<usize>) -> (String, usize) {
        // Returns (text, cursor_position_in_line)
        let prompt = "Rename Layout: ";
        let prompt_len = prompt.chars().count();

        let input_text = self.name_input.get_text();
        let cursor_pos = self.name_input.get_cursor_position();

        let mut text = format!("{}{}", prompt, input_text);
        let mut cursor_position_in_line = prompt_len + cursor_pos;

        if let Some(max_width) = max_width {
            if text.chars().count() > max_width {
                let truncated_name =
                    truncate_with_ellipsis_start(input_text, max_width.saturating_sub(prompt_len));
                text = format!("{}{}", prompt, truncated_name);
                let truncated_len = truncated_name.chars().count();
                cursor_position_in_line = prompt_len + cursor_pos.min(truncated_len);
            }
        }

        (text, cursor_position_in_line)
    }

    fn render_rename_line(&self, x: usize, y: usize, width: usize) {
        let (text, _) = self.rename_line_text(Some(width));
        let colored = Text::new(&text);
        print_text_with_coordinates(colored, x, y, None, None);
    }

    fn help_text(&self) -> (&str, &[&str]) {
        ("<Enter> - Save, <Esc> - Cancel", &["<Enter>", "<Esc>"])
    }

    fn render_help_text(&self, x: usize, y: usize, _width: usize) {
        let (text, items_to_color) = self.help_text();
        let mut text_obj = Text::new(text);
        for item in items_to_color {
            text_obj = text_obj.color_substring(3, item);
        }
        print_text_with_coordinates(text_obj, x, y, None, None);
    }

    fn render_title(&self, x: usize, y: usize, width: usize) {
        let title = Text::new("Rename Layout").color_all(2);
        print_text_with_coordinates(title, x, y, Some(width), None);
    }

    fn update_cursor_position(&self, base_x: usize, cursor_y: usize, max_width: usize) {
        // Always show cursor since rename is always in editing mode
        let (_, cursor_position_in_line) = self.rename_line_text(Some(max_width));
        show_cursor(Some((base_x + cursor_position_in_line, cursor_y)))
    }

    pub fn render(&self, rows: usize, cols: usize) {
        // Calculate desired width based on rename line and help text
        let desired_ui_width = std::cmp::max(
            self.help_text().0.chars().count(),
            self.rename_line_text(None).0.chars().count(),
        );

        // Leave at least 4 columns margin (2 on each side) to prevent text from reaching screen edge
        let max_allowed_width = cols.saturating_sub(4);
        let actual_ui_width = std::cmp::min(desired_ui_width, max_allowed_width);

        // Calculate total height: title(1) + spacing(1) + rename_line(1) + spacing(1) + help(1) = 5
        let desired_ui_height = 5;
        let actual_ui_height = std::cmp::min(desired_ui_height, rows);

        let base_y = rows.saturating_sub(actual_ui_height) / 2;
        let base_x = cols.saturating_sub(actual_ui_width) / 2;

        // Update cursor BEFORE rendering (critical!)
        let rename_line_y = base_y + 2;
        self.update_cursor_position(base_x, rename_line_y, actual_ui_width);

        // Render components
        self.render_title(base_x, base_y, actual_ui_width);
        self.render_rename_line(base_x, rename_line_y, actual_ui_width);

        // Help text at bottom
        let help_y = base_y + 4;
        self.render_help_text(base_x, help_y, actual_ui_width);
    }
}
