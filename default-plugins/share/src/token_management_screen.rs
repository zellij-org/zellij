use zellij_tile::prelude::*;

#[derive(Debug)]
struct ScreenContent {
    title: (String, Text),
    items: Vec<Vec<Text>>,
    help: (String, Text),
    status_message: Option<(String, Text)>,
    max_width: usize,
    new_token_line: Option<(String, Text)>,
}

#[derive(Debug)]
struct Layout {
    base_x: usize,
    base_y: usize,
    title_x: usize,
    new_token_y: usize,
    help_y: usize,
    status_y: usize,
}

#[derive(Debug)]
struct ScrollInfo {
    start_index: usize,
    end_index: usize,
    truncated_top: usize,
    truncated_bottom: usize,
}

#[derive(Debug)]
struct ColumnWidths {
    token: usize,
    date: usize,
    read_only: usize,
    controls: usize,
}

pub struct TokenManagementScreen<'a> {
    token_list: &'a Vec<(String, String, bool)>, // bool -> is_read_only
    selected_list_index: Option<usize>,
    renaming_token: &'a Option<String>,
    entering_new_token_name: &'a Option<String>,
    error: &'a Option<String>,
    info: &'a Option<String>,
    rows: usize,
    cols: usize,
}

impl<'a> TokenManagementScreen<'a> {
    pub fn new(
        token_list: &'a Vec<(String, String, bool)>,
        selected_list_index: Option<usize>,
        renaming_token: &'a Option<String>,
        entering_new_token_name: &'a Option<String>,
        error: &'a Option<String>,
        info: &'a Option<String>,
        rows: usize,
        cols: usize,
    ) -> Self {
        Self {
            token_list,
            selected_list_index,
            renaming_token,
            entering_new_token_name,
            error,
            info,
            rows,
            cols,
        }
    }
    pub fn render(&self) {
        let content = self.build_screen_content();
        let max_height = self.calculate_max_item_height();
        let scrolled_content = self.apply_scroll_truncation(content, max_height);
        let layout = self.calculate_layout(&scrolled_content);
        self.print_items_to_screen(scrolled_content, layout);
    }

    fn calculate_column_widths(&self) -> ColumnWidths {
        let max_table_width = self.cols;

        const MIN_TOKEN_WIDTH: usize = 10;
        const MIN_DATE_WIDTH: usize = 10; // Minimum for just date "YYYY-MM-DD"
        const MIN_READ_ONLY_WIDTH: usize = 5; // Minimum for "RO/RW"
        const MIN_CONTROLS_WIDTH: usize = 6; // Minimum for "(<x>, <r>)"
        const COLUMN_SPACING: usize = 5; // Space between columns (4 columns with table padding)

        let min_total_width = MIN_TOKEN_WIDTH
            + MIN_DATE_WIDTH
            + MIN_READ_ONLY_WIDTH
            + MIN_CONTROLS_WIDTH
            + COLUMN_SPACING;

        if max_table_width <= min_total_width {
            return ColumnWidths {
                token: MIN_TOKEN_WIDTH,
                date: MIN_DATE_WIDTH,
                read_only: MIN_READ_ONLY_WIDTH,
                controls: MIN_CONTROLS_WIDTH,
            };
        }

        const PREFERRED_DATE_WIDTH: usize = 29; // "issued on YYYY-MM-DD HH:MM:SS"
        const PREFERRED_READ_ONLY_WIDTH: usize = 10; // "read-write"
        const PREFERRED_CONTROLS_WIDTH: usize = 24; // "(<x> revoke, <r> rename)"

        let available_width = max_table_width.saturating_sub(COLUMN_SPACING);
        let preferred_fixed_width =
            PREFERRED_DATE_WIDTH + PREFERRED_READ_ONLY_WIDTH + PREFERRED_CONTROLS_WIDTH;

        if available_width >= preferred_fixed_width + MIN_TOKEN_WIDTH {
            // We can use preferred widths for date, read_only, and controls
            ColumnWidths {
                token: available_width.saturating_sub(preferred_fixed_width),
                date: PREFERRED_DATE_WIDTH,
                read_only: PREFERRED_READ_ONLY_WIDTH,
                controls: PREFERRED_CONTROLS_WIDTH,
            }
        } else {
            // Need to balance truncation across all columns
            // Priority: controls > read_only > date > token (token gets remaining space)
            let remaining_width = available_width
                .saturating_sub(MIN_TOKEN_WIDTH)
                .saturating_sub(MIN_DATE_WIDTH)
                .saturating_sub(MIN_READ_ONLY_WIDTH)
                .saturating_sub(MIN_CONTROLS_WIDTH);
            let extra_per_column = remaining_width / 4;

            ColumnWidths {
                token: MIN_TOKEN_WIDTH + extra_per_column,
                date: MIN_DATE_WIDTH + extra_per_column,
                read_only: MIN_READ_ONLY_WIDTH + extra_per_column,
                controls: MIN_CONTROLS_WIDTH + extra_per_column,
            }
        }
    }

    fn truncate_token_name(&self, token: &str, max_width: usize) -> String {
        if token.chars().count() <= max_width {
            return token.to_string();
        }

        if max_width <= 6 {
            // Too small to show anything meaningful
            return "[...]".to_string();
        }

        let truncator = if max_width <= 10 { "[..]" } else { "[...]" };
        let truncator_len = truncator.chars().count();
        let remaining_chars = max_width.saturating_sub(truncator_len);
        let start_chars = remaining_chars / 2;
        let end_chars = remaining_chars.saturating_sub(start_chars);

        let token_chars: Vec<char> = token.chars().collect();
        let start_part: String = token_chars.iter().take(start_chars).collect();
        let end_part: String = token_chars
            .iter()
            .rev()
            .take(end_chars)
            .collect::<String>()
            .chars()
            .rev()
            .collect();

        format!("{}{}{}", start_part, truncator, end_part)
    }

    fn format_date(
        &self,
        created_at: &str,
        max_width: usize,
        include_issued_prefix: bool,
    ) -> String {
        let full_text = if include_issued_prefix {
            format!("issued on {}", created_at)
        } else {
            created_at.to_string()
        };

        if full_text.chars().count() <= max_width {
            return full_text;
        }

        // If we can't fit "issued on", use the date
        if !include_issued_prefix || created_at.chars().count() <= max_width {
            if created_at.chars().count() <= max_width {
                return created_at.to_string();
            }

            // Truncate the date itself if needed
            let chars: Vec<char> = created_at.chars().collect();
            if max_width <= 3 {
                return "...".to_string();
            }
            let truncated: String = chars.iter().take(max_width - 3).collect();
            format!("{}...", truncated)
        } else {
            // Try without "issued on" prefix
            self.format_date(created_at, max_width, false)
        }
    }

    fn format_read_only(&self, is_read_only: bool, max_width: usize) -> String {
        let full_text = if is_read_only {
            "read-only"
        } else {
            "read-write"
        };
        let short_text = if is_read_only { "RO" } else { "RW" };

        let text = if full_text.chars().count() <= max_width {
            full_text
        } else {
            short_text
        };

        // Center the text in the column
        let text_len = text.chars().count();
        if text_len >= max_width {
            return text.to_string();
        }

        let padding = max_width - text_len;
        let left_padding = padding / 2;
        let right_padding = padding - left_padding;

        format!(
            "{}{}{}",
            " ".repeat(left_padding),
            text,
            " ".repeat(right_padding)
        )
    }

    fn format_controls(&self, max_width: usize, is_selected: bool) -> String {
        if !is_selected {
            return " ".repeat(max_width);
        }

        let full_controls = "(<x> revoke, <r> rename)";
        let short_controls = "(<x>, <r>)";

        if full_controls.chars().count() <= max_width {
            full_controls.to_string()
        } else if short_controls.chars().count() <= max_width {
            // Pad the short controls to fill the available width
            let padding = max_width - short_controls.chars().count();
            format!("{}{}", short_controls, " ".repeat(padding))
        } else {
            // Very constrained space
            " ".repeat(max_width)
        }
    }

    fn calculate_max_item_height(&self) -> usize {
        // Calculate fixed UI elements that are always present:
        // - 1 row for title
        // - 1 row for spacing after title (always preserved)
        // - 1 row for the "create new token" line (always visible)
        // - 1 row for spacing before help (always preserved)
        // - 1 row for help text (or status message - they're mutually exclusive)

        let fixed_rows = 4; // title + spacing + help/status + spacing before help
        let create_new_token_rows = 1; // "create new token" line

        let total_fixed_rows = fixed_rows + create_new_token_rows;

        // Calculate available rows for token items
        let available_for_items = self.rows.saturating_sub(total_fixed_rows);

        // Return at least 1 to avoid issues, but this will be the maximum height for token items only
        available_for_items.max(1)
    }

    fn build_screen_content(&self) -> ScreenContent {
        let mut max_width = 0;
        let max_table_width = self.cols;
        let column_widths = self.calculate_column_widths();

        let title_text = "List of Login Tokens";
        let title = Text::new(title_text).color_range(2, ..);
        max_width = std::cmp::max(max_width, title_text.len());

        let mut items = vec![];
        for (i, (token, created_at, read_only)) in self.token_list.iter().enumerate() {
            let is_selected = Some(i) == self.selected_list_index;
            let (row_text, row_items) =
                self.create_token_item(token, created_at, *read_only, is_selected, &column_widths);
            max_width = std::cmp::max(max_width, row_text.chars().count());
            items.push(row_items);
        }

        let (new_token_text, new_token_line) = self.create_new_token_line();
        max_width = std::cmp::max(max_width, new_token_text.chars().count());

        let (help_text, help_line) = self.create_help_line();
        max_width = std::cmp::max(max_width, help_text.chars().count());

        let status_message = self.create_status_message();
        if let Some((ref text, _)) = status_message {
            max_width = std::cmp::max(max_width, text.chars().count());
        }

        max_width = std::cmp::min(max_width, max_table_width);

        ScreenContent {
            title: (title_text.to_string(), title),
            items,
            help: (help_text, help_line),
            status_message,
            max_width,
            new_token_line: Some((new_token_text, new_token_line)),
        }
    }

    fn apply_scroll_truncation(
        &self,
        mut content: ScreenContent,
        max_height: usize,
    ) -> ScreenContent {
        let total_token_items = content.items.len(); // Only token items, not including "create new token"

        // If all token items fit, no need to truncate
        if total_token_items <= max_height {
            return content;
        }

        let scroll_info = self.calculate_scroll_info(total_token_items, max_height);

        // Extract the visible range
        let mut visible_items: Vec<Vec<Text>> = content
            .items
            .into_iter()
            .skip(scroll_info.start_index)
            .take(
                scroll_info
                    .end_index
                    .saturating_sub(scroll_info.start_index),
            )
            .collect();

        // Add truncation indicators
        if scroll_info.truncated_top > 0 {
            self.add_truncation_indicator(&mut visible_items[0], scroll_info.truncated_top);
        }

        if scroll_info.truncated_bottom > 0 {
            let last_idx = visible_items.len().saturating_sub(1);
            self.add_truncation_indicator(
                &mut visible_items[last_idx],
                scroll_info.truncated_bottom,
            );
        }

        content.items = visible_items;
        content
    }

    fn calculate_scroll_info(&self, total_token_items: usize, max_height: usize) -> ScrollInfo {
        // Only consider token items for scrolling (not the "create new token" line)
        // The "create new token" line is always visible and handled separately

        // Find the selected index within the token list only
        let selected_index = if let Some(idx) = self.selected_list_index {
            idx
        } else {
            // If "create new token" is selected or no selection,
            // we don't need to center anything in the token list
            0
        };

        // Calculate how many items to show above and below the selected item
        let items_above = max_height / 2;
        let items_below = max_height.saturating_sub(items_above).saturating_sub(1); // -1 for the selected item itself

        // Calculate the start and end indices
        let start_index = if selected_index < items_above {
            0
        } else if selected_index + items_below >= total_token_items {
            total_token_items.saturating_sub(max_height)
        } else {
            selected_index.saturating_sub(items_above)
        };

        let end_index = std::cmp::min(start_index + max_height, total_token_items);

        ScrollInfo {
            start_index,
            end_index,
            truncated_top: start_index,
            truncated_bottom: total_token_items.saturating_sub(end_index),
        }
    }

    fn add_truncation_indicator(&self, row: &mut Vec<Text>, count: usize) {
        let indicator = format!("+[{}]", count);

        // Replace the last cell (controls column) with the truncation indicator
        if let Some(last_cell) = row.last_mut() {
            *last_cell = Text::new(&indicator).color_range(1, ..);
        }
    }

    fn create_token_item(
        &self,
        token: &str,
        created_at: &str,
        is_read_only: bool,
        is_selected: bool,
        column_widths: &ColumnWidths,
    ) -> (String, Vec<Text>) {
        if is_selected {
            if let Some(new_name) = &self.renaming_token {
                self.create_renaming_item(new_name, created_at, is_read_only, column_widths)
            } else {
                self.create_selected_item(token, created_at, is_read_only, column_widths)
            }
        } else {
            self.create_regular_item(token, created_at, is_read_only, column_widths)
        }
    }

    fn create_renaming_item(
        &self,
        new_name: &str,
        created_at: &str,
        is_read_only: bool,
        column_widths: &ColumnWidths,
    ) -> (String, Vec<Text>) {
        let truncated_name =
            self.truncate_token_name(new_name, column_widths.token.saturating_sub(1)); // -1 for cursor
        let item_text = format!("{}_", truncated_name);
        let date_text = self.format_date(created_at, column_widths.date, true);
        let read_only_text = self.format_read_only(is_read_only, column_widths.read_only);
        let controls_text = " ".repeat(column_widths.controls);

        let token_end = truncated_name.chars().count();
        let items = vec![
            Text::new(&item_text)
                .color_range(0, ..token_end + 1)
                .selected(),
            Text::new(&date_text),
            Text::new(&read_only_text).color_all(1),
            Text::new(&controls_text),
        ];
        (
            format!(
                "{} {} {} {}",
                item_text, date_text, read_only_text, controls_text
            ),
            items,
        )
    }

    fn create_selected_item(
        &self,
        token: &str,
        created_at: &str,
        is_read_only: bool,
        column_widths: &ColumnWidths,
    ) -> (String, Vec<Text>) {
        let mut item_text = self.truncate_token_name(token, column_widths.token);
        if item_text.is_empty() {
            // otherwise the table gets messed up
            item_text.push(' ');
        };
        let date_text = self.format_date(created_at, column_widths.date, true);
        let read_only_text = self.format_read_only(is_read_only, column_widths.read_only);
        let controls_text = self.format_controls(column_widths.controls, true);

        // Determine highlight ranges for controls based on the actual content
        let (x_range, r_range) = if controls_text.contains("revoke") {
            // Full controls: "(<x> revoke, <r> rename)"
            (1..=3, 13..=15)
        } else {
            // Short controls: "(<x>, <r>)"
            (1..=3, 6..=8)
        };

        let controls_colored = if controls_text.trim().is_empty() {
            Text::new(&controls_text).selected()
        } else {
            Text::new(&controls_text)
                .color_range(3, x_range)
                .color_range(3, r_range)
                .selected()
        };

        let items = vec![
            Text::new(&item_text).color_range(0, ..).selected(),
            Text::new(&date_text).selected(),
            Text::new(&read_only_text).color_all(1).selected(),
            controls_colored,
        ];

        (
            format!(
                "{} {} {} {}",
                item_text, date_text, read_only_text, controls_text
            ),
            items,
        )
    }

    fn create_regular_item(
        &self,
        token: &str,
        created_at: &str,
        is_read_only: bool,
        column_widths: &ColumnWidths,
    ) -> (String, Vec<Text>) {
        let mut item_text = self.truncate_token_name(token, column_widths.token);
        if item_text.is_empty() {
            // otherwise the table gets messed up
            item_text.push(' ');
        };
        let date_text = self.format_date(created_at, column_widths.date, true);
        let read_only_text = self.format_read_only(is_read_only, column_widths.read_only);
        let controls_text = " ".repeat(column_widths.controls);

        let items = vec![
            Text::new(&item_text).color_range(0, ..),
            Text::new(&date_text),
            Text::new(&read_only_text).color_all(1),
            Text::new(&controls_text),
        ];
        (
            format!(
                "{} {} {} {}",
                item_text, date_text, read_only_text, controls_text
            ),
            items,
        )
    }

    fn create_new_token_line(&self) -> (String, Text) {
        let full_create_text = "<n> - create new token, <o> - create read-only token".to_string();
        let medium_create_text = "<n> - new token, <o> - read-only".to_string();
        let short_create_text = "<n> - new, <o> - RO".to_string();

        if let Some(name) = &self.entering_new_token_name {
            let max_width = self.cols.saturating_sub(1); // Leave room for cursor
            let truncated_name = if name.chars().count() > max_width {
                let chars: Vec<char> = name.chars().take(max_width).collect();
                chars.into_iter().collect()
            } else {
                name.clone()
            };
            let text = format!("{}_", truncated_name);
            (text.clone(), Text::new(&text).color_range(3, ..))
        } else {
            // Check which text fits
            let (text_to_use, n_range, o_range) = if full_create_text.chars().count() <= self.cols {
                (&full_create_text, 0..=2, 24..=26)
            } else if medium_create_text.chars().count() <= self.cols {
                (&medium_create_text, 0..=2, 16..=19)
            } else {
                (&short_create_text, 0..=2, 11..=14)
            };

            (
                text_to_use.to_string(),
                Text::new(text_to_use)
                    .color_range(3, n_range)
                    .color_range(3, o_range),
            )
        }
    }

    fn create_help_line(&self) -> (String, Text) {
        let (text, highlight_range) = if self.entering_new_token_name.is_some() {
            (
                "Help: Enter optional name for new token, <Enter> to submit",
                41..=47,
            )
        } else if self.renaming_token.is_some() {
            (
                "Help: Enter new name for this token, <Enter> to submit",
                39..=45,
            )
        } else {
            (
                "Help: <Ctrl x> - revoke all tokens, <Esc> - go back",
                6..=13,
            )
        };

        let mut help_line = Text::new(text).color_range(3, highlight_range);

        // Add second highlight for the back option
        if self.entering_new_token_name.is_none() && self.renaming_token.is_none() {
            help_line = help_line.color_range(3, 36..=40);
        }

        (text.to_string(), help_line)
    }

    fn create_status_message(&self) -> Option<(String, Text)> {
        if let Some(error) = &self.error {
            Some((error.clone(), Text::new(error).color_range(3, ..)))
        } else if let Some(info) = &self.info {
            Some((info.clone(), Text::new(info).color_range(1, ..)))
        } else {
            None
        }
    }

    fn calculate_layout(&self, content: &ScreenContent) -> Layout {
        // Calculate fixed UI elements that must always be present:
        // - 1 row for title
        // - 1 row for spacing after title (always preserved)
        // - token items (variable, potentially truncated)
        // - 1 row for "create new token" line
        // - 1 row for spacing before help (always preserved)
        // - 1 row for help text OR status message (mutually exclusive now)

        let fixed_ui_rows = 4; // title + spacing after title + spacing before help + help/status
        let create_new_token_rows = 1;
        let token_item_rows = content.items.len();

        let total_content_rows = fixed_ui_rows + create_new_token_rows + token_item_rows;

        // Only add top/bottom padding if we have extra space
        let base_y = if total_content_rows < self.rows {
            // We have room for padding - center the content
            (self.rows.saturating_sub(total_content_rows)) / 2
        } else {
            // No room for padding - start at the top
            0
        };

        // Calculate positions relative to base_y
        let item_start_y = base_y + 2; // title + spacing after title
        let new_token_y = item_start_y + token_item_rows;
        let help_y = new_token_y + 1 + 1; // new token line + spacing before help

        Layout {
            base_x: (self.cols.saturating_sub(content.max_width) as f64 / 2.0).floor() as usize,
            base_y,
            title_x: self.cols.saturating_sub(content.title.0.len()) / 2,
            new_token_y,
            help_y,
            status_y: help_y, // Status message uses the same position as help
        }
    }

    fn print_items_to_screen(&self, content: ScreenContent, layout: Layout) {
        print_text_with_coordinates(content.title.1, layout.title_x, layout.base_y, None, None);

        let mut table = Table::new().add_row(vec![" ", " ", " ", " "]);
        for item in content.items.into_iter() {
            table = table.add_styled_row(item);
        }

        print_table_with_coordinates(table, layout.base_x, layout.base_y + 1, None, None);

        if let Some((_, new_token_text)) = content.new_token_line {
            print_text_with_coordinates(
                new_token_text,
                layout.base_x,
                layout.new_token_y,
                None,
                None,
            );
        }

        if let Some((_, status_text)) = content.status_message {
            print_text_with_coordinates(status_text, layout.base_x, layout.status_y, None, None);
        } else {
            print_text_with_coordinates(content.help.1, layout.base_x, layout.help_y, None, None);
        }
    }
}
