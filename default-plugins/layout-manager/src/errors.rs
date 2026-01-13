use crate::screens::{KeyResponse, Screen};
use crate::ui::{truncate_line_with_ansi, wrap_text_to_width, ErrorMessage, MultiLineErrorMessage};
use zellij_tile::prelude::*;

/// Format a layout parsing error into a detailed error string
pub fn format_kdl_error(error: LayoutParsingError) -> String {
    match error {
        LayoutParsingError::KdlError {
            mut kdl_error,
            file_name,
            source_code,
        } => {
            use miette::{GraphicalReportHandler, NamedSource, Report};

            kdl_error.help_message =
                Some("https://zellij.dev/documentation/creating-a-layout.html".to_owned());
            let report: Report = kdl_error.into();
            let report = report.with_source_code(NamedSource::new(file_name, source_code));

            let handler = GraphicalReportHandler::new();
            let mut output = String::new();
            handler.render_report(&mut output, report.as_ref()).unwrap();
            output
        },
        LayoutParsingError::SyntaxError => {
            format!("Failed to deserialize KDL node. \nPossible reasons:\n{}\n{}\n{}\n{}",
            "- Missing `;` after a node name, eg. { node; another_node; }",
            "- Missing quotations (\") around an argument node eg. { first_node \"argument_node\"; }",
            "- Missing an equal sign (=) between node arguments on a title line. eg. argument=\"value\"",
            "- Found an extraneous equal sign (=) between node child arguments and their values. eg. { argument=\"value\" }")
        },
    }
}

#[derive(Clone)]
pub struct ErrorScreen {
    pub message: String,
    pub return_to_screen: Box<Screen>,
}

impl ErrorScreen {
    pub fn handle_key(&mut self, _key: KeyWithModifier) -> KeyResponse {
        KeyResponse::new_screen((*self.return_to_screen).clone())
    }

    pub fn render(&self, rows: usize, cols: usize) {
        if self.message.chars().count() > cols.saturating_sub(4) {
            let max_width = cols.saturating_sub(4);
            let max_rows = rows.saturating_sub(4);
            let lines = wrap_text_to_width(&self.message, max_width);
            let base_x = 2;
            let base_y = rows.saturating_sub(4 + lines.len()) / 2;
            MultiLineErrorMessage::new(lines).render(base_x, base_y, max_rows);
        } else {
            let desired_width = self.message.chars().count();
            let base_y = rows.saturating_sub(5) / 2;
            let base_x = cols.saturating_sub(desired_width) / 2;
            ErrorMessage::new(&self.message).render(base_x, base_y);
        }
    }
}

#[derive(Clone)]
pub struct ErrorDetailScreen {
    pub layout_name: String,
    pub detailed_error: String,
    pub return_to_screen: Box<Screen>,
}

impl ErrorDetailScreen {
    pub fn new(layout_name: String, detailed_error: String, return_to_screen: Box<Screen>) -> Self {
        Self {
            layout_name,
            detailed_error,
            return_to_screen,
        }
    }

    pub fn handle_key(&mut self, _key: KeyWithModifier) -> KeyResponse {
        KeyResponse::new_screen((*self.return_to_screen).clone())
    }

    pub fn render(&self, rows: usize, cols: usize) {
        // Header: show layout name
        let header = format!("Error in layout: {}", self.layout_name);
        let header_text = Text::new(&header).error_color_all();
        print_text_with_coordinates(header_text, 1, 0, None, None);

        // Calculate available space for error content
        let header_height = 2; // Header + gap
        let available_rows = rows.saturating_sub(header_height);
        let available_cols = cols.saturating_sub(2); // Padding on sides

        // Render error lines with middle truncation
        let error_lines: Vec<&str> = self.detailed_error.lines().collect();
        let total_lines = error_lines.len();

        if total_lines <= available_rows {
            // All lines fit, show them all
            for (i, line) in error_lines.iter().enumerate() {
                let truncated = truncate_line_with_ansi(line, available_cols);
                print!("\u{1b}[{};{}H{}", header_height + i + 1, 2, truncated);
            }
        } else {
            // Need to truncate middle - show beginning, indicator, and end
            let omitted_indicator_lines = 1; // Reserve 1 line for "... X lines omitted ..."
            let lines_for_content = available_rows.saturating_sub(omitted_indicator_lines);

            // Split content space: 60% for beginning, 40% for end
            let beginning_lines = (lines_for_content as f32 * 0.6).ceil() as usize;
            let end_lines = lines_for_content.saturating_sub(beginning_lines);

            let omitted_count = total_lines.saturating_sub(beginning_lines + end_lines);

            let mut current_row = 0;

            // Render beginning lines
            for line in error_lines.iter().take(beginning_lines) {
                let truncated = truncate_line_with_ansi(line, available_cols);
                print!(
                    "\u{1b}[{};{}H{}",
                    header_height + current_row + 1,
                    2,
                    truncated
                );
                current_row += 1;
            }

            // Render omission indicator
            let indicator = format!("... {} lines omitted ...", omitted_count);
            let indicator_text = Text::new(&indicator).color_range(0, ..);
            print_text_with_coordinates(
                indicator_text,
                (cols.saturating_sub(indicator.chars().count())) / 2,
                header_height + current_row,
                None,
                None,
            );
            current_row += 1;

            // Render end lines
            let start_index = total_lines.saturating_sub(end_lines);
            for line in error_lines.iter().skip(start_index) {
                let truncated = truncate_line_with_ansi(line, available_cols);
                print!(
                    "\u{1b}[{};{}H{}",
                    header_height + current_row + 1,
                    2,
                    truncated
                );
                current_row += 1;
            }
        }
    }
}
