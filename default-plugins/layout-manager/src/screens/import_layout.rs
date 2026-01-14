use super::{KeyResponse, LayoutListScreen, OptimisticUpdate, Screen};
use crate::errors::{format_kdl_error, ErrorScreen};
use crate::text_input::{InputAction, TextInput};
use crate::ui::{truncate_with_ellipsis_start, LayoutDetail, Title};
use crate::{DisplayLayout, LayoutInfo};
use zellij_tile::prelude::*;

#[derive(Clone)]
pub struct ImportLayoutScreen {
    pub name_input: TextInput,
    pub pasted_text: Option<String>,
    pub parsed_metadata: Option<LayoutMetadata>,
    pub parse_error: Option<String>,
    pub editing_name: bool,
}

impl ImportLayoutScreen {
    pub fn new() -> Self {
        Self {
            name_input: TextInput::empty(),
            pasted_text: None,
            parsed_metadata: None,
            parse_error: None,
            editing_name: false,
        }
    }

    pub fn handle_pasted_text(&mut self, text: String) {
        self.pasted_text = Some(text.clone());

        match parse_layout(&text) {
            Ok(parsed_metadata) => {
                self.parsed_metadata = Some(parsed_metadata);
                self.parse_error = None;
            },
            Err(e) => {
                self.parsed_metadata = None;
                self.parse_error = Some(format_kdl_error(e));
            },
        }
    }

    pub fn handle_key(&mut self, key: KeyWithModifier) -> KeyResponse {
        // Special Esc handling based on edit state
        if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            return self.handle_escape_key();
        }

        if self.editing_name {
            self.handle_editing_mode_key(key)
        } else {
            self.handle_non_editing_mode_key(key)
        }
    }

    fn handle_escape_key(&mut self) -> KeyResponse {
        if self.editing_name {
            self.editing_name = false;
            KeyResponse::render()
        } else {
            KeyResponse::new_screen(self.cancel_import())
        }
    }

    fn handle_editing_mode_key(&mut self, key: KeyWithModifier) -> KeyResponse {
        let action = self.name_input.handle_key(key);

        match action {
            InputAction::Continue => KeyResponse::render(),
            InputAction::Submit => self.exit_editing_mode(),
            InputAction::Cancel => self.exit_editing_mode(),
            InputAction::Complete => KeyResponse::none(),
            InputAction::NoAction => KeyResponse::none(),
        }
    }

    fn handle_non_editing_mode_key(&mut self, key: KeyWithModifier) -> KeyResponse {
        match key.bare_key {
            BareKey::Enter if key.has_no_modifiers() => self.attempt_save_layout(),
            BareKey::Char('r') if key.has_no_modifiers() => self.enter_editing_mode(),
            _ => KeyResponse::none(),
        }
    }

    fn exit_editing_mode(&mut self) -> KeyResponse {
        self.editing_name = false;
        KeyResponse::render()
    }

    fn enter_editing_mode(&mut self) -> KeyResponse {
        self.editing_name = true;
        KeyResponse::render()
    }

    fn cancel_import(&self) -> Screen {
        Screen::LayoutList(LayoutListScreen::default())
    }

    fn attempt_save_layout(&self) -> KeyResponse {
        // Check if we have a pasted layout
        let Some(pasted_text) = &self.pasted_text else {
            return KeyResponse::new_screen(
                self.create_error_screen("Please paste a layout first"),
            );
        };

        // Check if the layout is valid
        if self.parse_error.is_some() {
            return KeyResponse::new_screen(
                self.create_error_screen("Cannot save an invalid layout"),
            );
        }

        // Get the parsed metadata for optimistic update
        let Some(metadata) = &self.parsed_metadata else {
            return KeyResponse::new_screen(
                self.create_error_screen("Cannot save layout without metadata"),
            );
        };

        let layout_name = self.determine_layout_name();

        // Capture optimistic update BEFORE the API call
        let optimistic = OptimisticUpdate::Add {
            name: layout_name.clone(),
            metadata: metadata.clone(),
        };

        match self.save_layout(&layout_name, pasted_text) {
            Ok(()) => KeyResponse::new_screen(Screen::LayoutList(LayoutListScreen::default()))
                .with_optimistic(optimistic),
            Err(error_msg) => KeyResponse::new_screen(self.create_error_screen(&error_msg)),
        }
    }

    fn create_error_screen(&self, message: &str) -> Screen {
        Screen::Error(ErrorScreen {
            message: message.to_string(),
            return_to_screen: Box::new(Screen::ImportLayout(self.clone())),
        })
    }

    fn determine_layout_name(&self) -> String {
        let input_text = self.name_input.get_text();
        if input_text.is_empty() {
            generate_random_name()
        } else {
            input_text.to_string()
        }
    }

    fn save_layout(&self, layout_name: &str, layout_content: &str) -> Result<(), String> {
        save_layout(layout_name.to_owned(), layout_content.to_owned(), false)
            .map_err(|err| format!("Failed to save layout '{}': {}", layout_name, err))?;

        eprintln!("Successfully imported layout: {}", layout_name);
        Ok(())
    }

    fn description_text(&self) -> &str {
        "Paste your layout here."
    }

    fn max_description_width(&self) -> usize {
        self.description_text().chars().count()
    }

    fn render_description(&self, x: usize, y: usize, _width: usize) {
        let text = Text::new(self.description_text());
        print_text_with_coordinates(text, x, y, None, None);
    }

    fn save_as_line_text(&self, max_width: Option<usize>) -> (String, usize) {
        // Returns (text, cursor_position_in_line)
        let input_text = self.name_input.get_text();
        let cursor_pos = self.name_input.get_cursor_position();

        let display_name = if input_text.is_empty() && !self.editing_name {
            "RANDOM"
        } else {
            input_text
        };

        let text_suffix = if self.editing_name {
            "" // No hint when editing
        } else {
            " (<r> Rename)" // Show rename hint when not editing
        };

        let mut text = format!("Save as: {}{}", display_name, text_suffix);
        let mut cursor_position_in_line = 9 + cursor_pos;

        if let Some(max_width) = max_width {
            if text.chars().count() > max_width {
                let truncated_display_name = truncate_with_ellipsis_start(
                    display_name,
                    max_width.saturating_sub(9 + text_suffix.chars().count()),
                );
                text = format!("Save as: {}{}", truncated_display_name, text_suffix);
                let truncated_len = truncated_display_name.chars().count();
                cursor_position_in_line = 9 + cursor_pos.min(truncated_len);
            }
        }

        (text, cursor_position_in_line)
    }

    fn render_save_as_line(&self, x: usize, y: usize, width: usize) {
        let (text, _) = self.save_as_line_text(Some(width));
        let colored = Text::new(&text).color_substring(3, "<r>");
        print_text_with_coordinates(colored, x, y, None, None);
    }

    fn help_text(&self) -> (&str, &[&str]) {
        // Only Enter and Esc - NO <r> here (it's in the save-as line)
        ("<Enter> - Save, <Esc> - Back", &["<Enter>", "<Esc>"])
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
        let title = Text::new("Import Layout").color_all(2);
        print_text_with_coordinates(title, x, y, Some(width), None);
    }

    pub fn render(&self, rows: usize, cols: usize) {
        // Only render modal if we have a valid pasted layout
        if let Some(metadata) = &self.parsed_metadata {
            let display_layout =
                DisplayLayout::Valid(LayoutInfo::File("imported".to_string(), metadata.clone()));

            let layout_detail = LayoutDetail::new(&display_layout);

            let desired_ui_width = std::cmp::max(
                self.help_text().0.chars().count(),
                self.save_as_line_text(None).0.chars().count(),
            );

            let actual_ui_width = std::cmp::min(desired_ui_width, cols);

            // Calculate layout detail dimensions
            let available_cols_for_details = actual_ui_width.saturating_sub(2);
            let desired_details_height =
                layout_detail.calculate_required_height(available_cols_for_details);

            let desired_ui_height = 6 + desired_details_height;
            let base_y = rows.saturating_sub(desired_ui_height) / 2;
            let base_x = cols.saturating_sub(actual_ui_width) / 2;

            // Update cursor BEFORE rendering
            let save_as_line_y = base_y + 2;
            self.update_cursor_position(base_x, save_as_line_y, actual_ui_width);

            // Render components
            self.render_title(base_x, base_y, actual_ui_width);
            self.render_save_as_line(base_x, save_as_line_y, actual_ui_width);

            let details_y = save_as_line_y + 2;

            let actual_details_height = if rows < desired_ui_height {
                rows.saturating_sub(details_y + 2)
            } else {
                desired_details_height
            };

            layout_detail.render(
                base_x + 2,
                details_y,
                actual_details_height,
                available_cols_for_details,
            );

            // Render help text
            let help_y = details_y + actual_details_height + 1;
            self.render_help_text(base_x, help_y, actual_ui_width);
        } else if let Some(error) = &self.parse_error {
            // Show error if parse failed
            Title::new("Import Layout").render(0, 0);
            self.render_parse_error(error, 2);

            let help_y = rows.saturating_sub(2);
            self.render_esc_cancel_help(0, help_y);
        } else {
            // Calculate desired width
            let desired_ui_width = std::cmp::max(
                self.help_text().0.chars().count(),
                self.max_description_width(),
            );

            let actual_ui_width = std::cmp::min(desired_ui_width, cols);
            let actual_ui_height = std::cmp::min(6, rows); // 6 - content rows plus padding

            let base_y = rows.saturating_sub(actual_ui_height) / 2;
            let base_x = cols.saturating_sub(actual_ui_width) / 2;

            Title::new("Import Layout").render(base_x, base_y);
            self.render_description(base_x, base_y + 2, cols);

            let help_y = base_y + 4;
            self.render_esc_cancel_help(base_x, help_y);
        }
    }
    fn render_esc_cancel_help(&self, x: usize, y: usize) {
        let help_text = "<Esc> - Cancel";
        let help_text = Text::new(help_text).color_substring(3, "<Esc>");
        print_text_with_coordinates(help_text, x, y, None, None);
    }

    fn render_parse_error(&self, error: &str, y: usize) {
        // Render error message using ANSI escape sequences
        for (i, line) in error.lines().enumerate() {
            print!("\u{1b}[{};{}H{}", y + i + 1, 1, line);
        }
    }

    pub fn update_cursor_position(&self, base_x: usize, cursor_y: usize, max_width: usize) {
        if self.editing_name {
            let (_, cursor_position_in_line) = self.save_as_line_text(Some(max_width));
            show_cursor(Some((base_x + cursor_position_in_line, cursor_y)))
        } else {
            show_cursor(None)
        }
    }
}
