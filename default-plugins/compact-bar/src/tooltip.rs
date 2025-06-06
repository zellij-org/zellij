use crate::keybind_utils::KeybindProcessor;
use zellij_tile::prelude::*;

pub struct TooltipRenderer<'a> {
    mode_info: &'a ModeInfo,
}

impl<'a> TooltipRenderer<'a> {
    pub fn new(mode_info: &'a ModeInfo) -> Self {
        Self { mode_info }
    }

    pub fn render(&self, rows: usize, cols: usize) {
        let current_mode = self.mode_info.mode;

        if current_mode == InputMode::Normal {
            let (text_components, tooltip_rows, tooltip_columns) =
                self.normal_mode_tooltip(current_mode);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;

            for (text, ribbon, x, y) in text_components {
                let text_width = text.content().chars().count();
                let ribbon_content_width = ribbon.content().chars().count();
                let ribbon_total_width = ribbon_content_width + 4;
                let total_element_width = text_width + ribbon_total_width + 1;

                // Check if this element would exceed the available columns and render an ellipses
                // if it does
                if base_x + x + total_element_width > cols {
                    let remaining_space = cols.saturating_sub(base_x + x);
                    let ellipsis = Text::new("...");
                    print_text_with_coordinates(
                        ellipsis,
                        base_x + x,
                        base_y + y,
                        Some(remaining_space),
                        None,
                    );
                    break;
                }

                print_text_with_coordinates(text, base_x + x, base_y + y, None, None);
                print_ribbon_with_coordinates(
                    ribbon,
                    base_x + x + text_width + 1,
                    base_y + y,
                    None,
                    None,
                );
            }
        } else {
            let (table, tooltip_rows, tooltip_columns) = self.other_mode_tooltip(current_mode);
            let base_x = cols.saturating_sub(tooltip_columns) / 2;
            let base_y = rows.saturating_sub(tooltip_rows) / 2;
            print_table_with_coordinates(table, base_x, base_y, None, None);
        }
    }

    pub fn calculate_dimensions(&self, current_mode: InputMode) -> (usize, usize) {
        match current_mode {
            InputMode::Normal => {
                let (_, tooltip_rows, tooltip_cols) = self.normal_mode_tooltip(current_mode);
                (tooltip_rows, tooltip_cols)
            },
            _ => {
                let (_, tooltip_rows, tooltip_cols) = self.other_mode_tooltip(current_mode);
                (tooltip_rows + 1, tooltip_cols) // + 1 for the invisible table title
            },
        }
    }

    fn normal_mode_tooltip(
        &self,
        current_mode: InputMode,
    ) -> (Vec<(Text, Text, usize, usize)>, usize, usize) {
        let actions = KeybindProcessor::get_predetermined_actions(self.mode_info, current_mode);
        let y = 0;
        let mut running_x = 0;
        let mut components = Vec::new();
        let mut max_columns = 0;

        for (key, description) in actions {
            let text = Text::new(&key).color_all(3);
            let ribbon = Text::new(&description);

            let line_length = key.chars().count() + 1 + description.chars().count();

            components.push((text, ribbon, running_x, y));
            running_x += line_length + 5;
            max_columns = max_columns.max(running_x);
        }

        let total_rows = 1;
        (components, total_rows, max_columns)
    }

    fn other_mode_tooltip(&self, current_mode: InputMode) -> (Table, usize, usize) {
        let actions = KeybindProcessor::get_predetermined_actions(self.mode_info, current_mode);
        let actions_vec: Vec<_> = actions.into_iter().collect();

        let mut table = Table::new().add_row(vec![" ".to_owned(); 2]);
        let mut row_count = 1; // Start with header row

        if actions_vec.is_empty() {
            let tooltip_text = match self.mode_info.mode {
                InputMode::EnterSearch => "Entering search term...".to_owned(),
                InputMode::RenameTab => "Renaming tab...".to_owned(),
                InputMode::RenamePane => "Renaming pane...".to_owned(),
                _ => {
                    format!("{:?}", self.mode_info.mode)
                },
            };
            let total_width = tooltip_text.chars().count();
            table = table.add_styled_row(vec![Text::new(tooltip_text).color_all(0)]);
            row_count += 1;
            (table, row_count, total_width)
        } else {
            let mut key_width = 0;
            let mut action_width = 0;
            for (key, description) in actions_vec.into_iter() {
                let description_formatted = format!("- {}", description);
                key_width = key_width.max(key.chars().count());
                action_width = action_width.max(description_formatted.chars().count());
                table = table.add_styled_row(vec![
                    Text::new(&key).color_all(3),
                    Text::new(description_formatted),
                ]);
                row_count += 1;
            }

            let total_width = key_width + action_width + 1; // +1 for separator
            (table, row_count, total_width)
        }
    }
}
