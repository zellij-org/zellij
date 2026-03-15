use super::text_utils::{get_layout_display_info, truncate_with_ellipsis, wrap_text_to_width};
use crate::DisplayLayout;
use zellij_tile::prelude::*;

pub struct LayoutDetail<'a> {
    layout: &'a DisplayLayout,
}

impl<'a> LayoutDetail<'a> {
    pub fn new(layout: &'a DisplayLayout) -> Self {
        Self { layout }
    }

    pub fn render(&self, x: usize, y: usize, max_rows: usize, max_cols: usize) {
        match self.layout {
            DisplayLayout::Valid(_) => {
                self.render_valid_layout(self.layout, x, y, max_rows, max_cols)
            },
            DisplayLayout::Error {
                name: _,
                error: _,
                error_message,
            } => self.render_error(error_message, x, y, max_rows, max_cols),
        }
    }

    pub fn calculate_required_height(&self, max_cols: usize) -> usize {
        match self.layout {
            DisplayLayout::Valid(_) => {
                let (_, metadata_opt) = get_layout_display_info(self.layout);
                let Some(metadata) = metadata_opt else {
                    return 1; // No metadata available
                };
                self.calculate_content_lines(metadata, max_cols)
            },
            DisplayLayout::Error { .. } => {
                3 // Error message + hint line
            },
        }
    }

    fn calculate_content_lines(&self, metadata: &LayoutMetadata, max_cols: usize) -> usize {
        if !self.should_show_tabs_section(metadata) {
            // Panes only
            let panes_lines = self.prepare_panes_content(metadata, max_cols);
            panes_lines.len()
        } else {
            // Tabs and panes side-by-side
            let (left_width, right_width, _) = self.calculate_column_layout(max_cols);
            let tabs_lines = self.prepare_tabs_content(metadata, left_width);
            let panes_lines = self.prepare_panes_content(metadata, right_width);
            tabs_lines.len().max(panes_lines.len())
        }
    }

    fn render_valid_layout(
        &self,
        layout: &DisplayLayout,
        x: usize,
        y: usize,
        max_rows: usize,
        max_cols: usize,
    ) {
        let (name, metadata_opt) = get_layout_display_info(layout);
        let Some(metadata) = metadata_opt else {
            if layout.is_builtin() {
                self.render_built_in_indication(&name, x, y, max_rows, max_cols);
            } else {
                self.render_no_metadata(x, y);
            }
            return;
        };

        self.render_tabs_and_panes(metadata, x, y, max_rows, max_cols);
    }

    fn render_no_metadata(&self, x: usize, y: usize) {
        let msg = Text::new("No metadata available").color_all(1);
        print_text_with_coordinates(msg, x, y + 1, None, None);
    }
    fn render_built_in_indication(
        &self,
        name: &str,
        x: usize,
        y: usize,
        max_rows: usize,
        max_cols: usize,
    ) {
        if max_rows == 0 {
            return;
        }

        let full_text = format!("{} is a built-in layout. Create your own layouts to automate or share workspace setup.", name);
        let wrapped_lines = wrap_text_to_width(&full_text, max_cols);

        let mut current_y = y;
        for line in wrapped_lines.iter().take(max_rows) {
            let text = if line.contains(name) {
                Text::new(line).color_substring(1, name)
            } else {
                Text::new(line)
            };
            print_text_with_coordinates(text, x, current_y, None, None);
            current_y += 1;
        }
    }

    fn render_error(
        &self,
        error_message: &str,
        x: usize,
        y: usize,
        max_rows: usize,
        max_cols: usize,
    ) {
        if max_rows == 0 {
            return;
        }

        let wrapped_lines = wrap_text_to_width(error_message, max_cols);
        let available_rows = max_rows.saturating_sub(3);

        let mut current_y = y + 1; // + 1 (and saturating_sub(3) above) to be aligned witht he
                                   // table on the left
        for line in wrapped_lines.iter().take(available_rows) {
            let text = Text::new(line).error_color_all();
            print_text_with_coordinates(text, x, current_y, None, None);
            current_y += 1;
        }

        let hint = Text::new("<m> - Show detailed error").color_substring(3, "<m>");
        print_text_with_coordinates(hint, x, current_y + 1, None, None); // 1 for gap
    }

    fn render_tabs_and_panes(
        &self,
        metadata: &LayoutMetadata,
        x: usize,
        y: usize,
        max_rows: usize,
        max_cols: usize,
    ) {
        // let max_rows = max_rows.saturating_sub(1);

        if !self.should_show_tabs_section(metadata) {
            self.render_panes_only(metadata, x, y, max_rows, max_cols);
            return;
        }

        let (left_width, right_width, padding) = self.calculate_column_layout(max_cols);
        let right_x = x + left_width + padding;

        let tabs_lines = self.prepare_tabs_content(metadata, left_width);
        let panes_lines = self.prepare_panes_content(metadata, right_width);

        let (tabs_truncated, panes_truncated) =
            self.render_side_by_side_columns(&panes_lines, &tabs_lines, x, right_x, y, max_rows);

        if tabs_truncated {
            self.render_truncation_ellipsis(x, y + max_rows);
        }
        if panes_truncated {
            self.render_truncation_ellipsis(right_x, y + max_rows);
        }
    }

    fn render_side_by_side_columns(
        &self,
        left_lines: &[String],
        right_lines: &[String],
        left_x: usize,
        right_x: usize,
        start_y: usize,
        max_rows: usize,
    ) -> (bool, bool) {
        let max_lines = left_lines.len().max(right_lines.len());
        let mut left_truncated = false;
        let mut right_truncated = false;

        for line_idx in 0..=max_lines {
            if line_idx >= max_rows {
                if line_idx < left_lines.len() {
                    left_truncated = true;
                }
                if line_idx < right_lines.len() {
                    right_truncated = true;
                }
                break;
            }

            let current_y = start_y + line_idx;

            if let Some(line) = left_lines.get(line_idx) {
                self.render_colored_line(line, left_x, current_y, line_idx == 0, 2);
            }

            if let Some(line) = right_lines.get(line_idx) {
                self.render_colored_line(line, right_x, current_y, line_idx == 0, 2);
            }
        }

        (left_truncated, right_truncated)
    }

    fn should_show_tabs_section(&self, metadata: &LayoutMetadata) -> bool {
        metadata.tabs.len() > 1
    }

    fn prepare_tabs_content(&self, metadata: &LayoutMetadata, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        // Add title
        let title = "Tabs:";
        lines.push(truncate_with_ellipsis(title, max_width));

        // Add tabs
        for (i, tab) in metadata.tabs.iter().enumerate() {
            let tab_label = if let Some(name) = &tab.name {
                name.clone()
            } else {
                format!("Tab {}", i + 1)
            };

            // Account for "  - " prefix (4 characters)
            let available_width = max_width.saturating_sub(4);
            let truncated_label = truncate_with_ellipsis(&tab_label, available_width);
            lines.push(format!("  - {}", truncated_label));
        }

        lines
    }

    fn prepare_panes_content(&self, metadata: &LayoutMetadata, max_width: usize) -> Vec<String> {
        let mut lines = Vec::new();

        let (named_panes, terminal_count) = self.collect_pane_info(metadata);

        // Early return if nothing to show
        if !self.has_panes_to_show(&named_panes, terminal_count) {
            return lines;
        }

        // Add title
        let title = "Panes:";
        lines.push(truncate_with_ellipsis(title, max_width));

        // Add named panes
        for pane_name in &named_panes {
            let available_width = max_width.saturating_sub(4);
            let truncated_name = truncate_with_ellipsis(pane_name, available_width);
            lines.push(format!("  - {}", truncated_name));
        }

        // Add terminal count if any
        if terminal_count > 0 {
            let text = if named_panes.is_empty() {
                format!("  {} Terminals", terminal_count)
            } else {
                format!("  +{} Terminals", terminal_count)
            };
            let truncated_text = truncate_with_ellipsis(&text, max_width);
            lines.push(truncated_text);
        }

        lines
    }

    fn calculate_column_layout(&self, max_cols: usize) -> (usize, usize, usize) {
        let left_width = (max_cols - 1) / 2;
        let padding = 1;
        let right_width = max_cols - left_width - padding;
        (left_width, right_width, padding)
    }

    fn render_colored_line(
        &self,
        line: &str,
        x: usize,
        y: usize,
        is_title: bool,
        title_color: usize,
    ) {
        let text = if is_title {
            Text::new(line).color_all(title_color)
        } else {
            Text::new(line)
        };
        print_text_with_coordinates(text, x, y, None, None);
    }

    fn render_truncation_ellipsis(&self, x: usize, y: usize) {
        let ellipsis = Text::new("  [...]").color_all(1);
        print_text_with_coordinates(ellipsis, x, y, None, None);
    }

    fn render_panes_only(
        &self,
        metadata: &LayoutMetadata,
        x: usize,
        y: usize,
        max_rows: usize,
        max_cols: usize,
    ) {
        let panes_lines = self.prepare_panes_content(metadata, max_cols);

        let mut was_truncated = false;
        for (line_idx, pane_line) in panes_lines.iter().enumerate() {
            if line_idx >= max_rows {
                was_truncated = true;
                break;
            }

            let current_y = y + line_idx;
            self.render_colored_line(pane_line, x, current_y, line_idx == 0, 2);
        }

        if was_truncated {
            self.render_truncation_ellipsis(x, y + max_rows);
        }
    }

    fn has_panes_to_show(&self, named_panes: &[String], terminal_count: usize) -> bool {
        !named_panes.is_empty() || terminal_count > 0
    }

    fn collect_pane_info(&self, metadata: &LayoutMetadata) -> (Vec<String>, usize) {
        let mut named_panes = Vec::new();
        let mut terminal_count = 0;

        for tab in &metadata.tabs {
            for pane in &tab.panes {
                // Skip builtin plugin panes
                if pane.is_builtin_plugin {
                    continue;
                }
                match &pane.name {
                    Some(name) => named_panes.push(name.clone()),
                    None => terminal_count += 1,
                }
            }
        }

        (named_panes, terminal_count)
    }
}
