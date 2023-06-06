use serde::{Serialize, Deserialize};
use crate::ui::{GREEN, ORANGE, GRAY_LIGHT, bold, underline, styled_text, styled_text_foreground, styled_text_background};
use unicode_width::UnicodeWidthStr;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SearchResult {
    File {
        path: String,
        score: i64,
        indices: Vec<usize>,
    },
    LineInFile {
        path: String,
        line: String,
        line_number: usize,
        score: i64,
        indices: Vec<usize>,
    },
}

impl SearchResult {
    pub fn new_file_name(score: i64, indices: Vec<usize>, path: String) -> Self {
        SearchResult::File {
            path,
            score,
            indices,
        }
    }
    pub fn new_file_line(
        score: i64,
        indices: Vec<usize>,
        path: String,
        line: String,
        line_number: usize,
    ) -> Self {
        SearchResult::LineInFile {
            path,
            score,
            indices,
            line,
            line_number,
        }
    }
    pub fn score(&self) -> i64 {
        match self {
            SearchResult::File { score, .. } => *score,
            SearchResult::LineInFile { score, .. } => *score,
        }
    }
    pub fn rendered_height(&self) -> usize {
        match self {
            SearchResult::File { .. } => 1,
            SearchResult::LineInFile { .. } => 2,
        }
    }
    pub fn is_same_entry(&self, other: &Self) -> bool {
        match (&self, other) {
            (SearchResult::File { path: my_path, .. }, SearchResult::File { path: other_path, .. }) => my_path == other_path,
            (SearchResult::LineInFile { path: my_path, line_number: my_line_number, .. }, SearchResult::LineInFile { path: other_path, line_number: other_line_number, .. }) => {
                my_path == other_path && my_line_number == other_line_number
            },
            _ => false
        }
    }
    pub fn render(&self, max_width: usize, is_selected: bool, is_below_search_result: bool) -> String {
        let max_width = max_width.saturating_sub(4); // for the UI left line separator
        match self {
            SearchResult::File { path, indices, .. } => self.render_file_result(path, indices, is_selected, is_below_search_result, max_width),
            SearchResult::LineInFile {
                path,
                line,
                line_number,
                indices,
                ..
            } => self.render_line_in_file_result(path, line, *line_number, indices, is_selected, is_below_search_result, max_width)
        }
    }
    fn render_file_result(&self, path: &String, indices: &Vec<usize>, is_selected: bool, is_below_search_result: bool, max_width: usize) -> String {
        if is_selected {
            let line = self.render_line_with_indices(path, indices, max_width.saturating_sub(3), Some(GREEN));
            let selection_arrow = styled_text_foreground(ORANGE, "┌>");
            format!("{} {}", selection_arrow, line)
        } else {
            let line_prefix = if is_below_search_result { "│ " } else { "  " };
            let line =
                self.render_line_with_indices(path, indices, max_width.saturating_sub(3), None);
            format!("{} {}", line_prefix, line)
        }
    }
    fn render_line_in_file_result(&self, path: &String, line: &String, line_number: usize, indices: &Vec<usize>, is_selected: bool, is_below_search_result: bool, max_width: usize) -> String {
        let line_number_prefix_text = format!("└ {} ", line_number);
        let max_width_of_line_in_file = max_width.saturating_sub(3).saturating_sub(line_number_prefix_text.width());
        if is_selected {
            let file_name_line = self.render_line_with_indices(path, &vec![], max_width.saturating_sub(3), Some(GREEN));
            let line_in_file = self.render_line_with_indices(line, indices, max_width_of_line_in_file, Some(GREEN));
            let line_number_prefix = styled_text_foreground(GREEN, &bold(&line_number_prefix_text));
            format!("{} {}\n│  {}{}", styled_text_foreground(ORANGE, "┌>"), file_name_line, line_number_prefix, line_in_file)
        } else {
            let file_name_line = self.render_line_with_indices(path, &vec![], max_width.saturating_sub(3), None);
            let line_in_file = self.render_line_with_indices(line, indices, max_width_of_line_in_file, None);
            let line_number_prefix = bold(&line_number_prefix_text);
            let line_prefix = if is_below_search_result { "│ " } else { "  "};
            format!("{} {}\n{} {}{}", line_prefix, file_name_line, line_prefix, line_number_prefix, line_in_file)
        }
    }
    fn render_line_with_indices(
        &self,
        line_to_render: &String,
        indices: &Vec<usize>,
        max_width: usize,
        foreground_color: Option<u8>,
    ) -> String {
        let non_index_character_style = |c: &str| match foreground_color {
            Some(foreground_color) => styled_text_foreground(foreground_color, &bold(c)),
            None => bold(c),
        };
        let index_character_style = |c: &str| match foreground_color {
            Some(foreground_color) => styled_text(foreground_color, GRAY_LIGHT, &bold(&underline(c))),
            None => styled_text_background(GRAY_LIGHT, &bold(&underline(c))),
        };

        let truncate_positions = self.truncate_line_with_indices(line_to_render, indices, max_width);
        let truncate_start_position = truncate_positions.map(|p| p.0).unwrap_or(0);
        let truncate_end_position = truncate_positions.map(|p| p.1).unwrap_or(line_to_render.chars().count());
        let mut visible_portion = String::new();
        for (i, character) in line_to_render.chars().enumerate() {
            if i >= truncate_start_position && i <= truncate_end_position {
                if indices.contains(&i) {
                    visible_portion.push_str(&index_character_style(&character.to_string()));
                } else {
                    visible_portion.push_str(&non_index_character_style(&character.to_string()));
                }
            }
        }
        if truncate_positions.is_some() {
            let left_truncate_sign = if truncate_start_position == 0 { "" } else { ".." };
            let right_truncate_sign = if truncate_end_position == line_to_render.chars().count() { "" } else { ".." };
            format!("{}{}{}", non_index_character_style(left_truncate_sign), visible_portion, non_index_character_style(right_truncate_sign))
        } else {
            visible_portion
        }
    }
    fn truncate_line_with_indices(&self, line_to_render: &String, indices: &Vec<usize>, max_width: usize) -> Option<(usize, usize)> {
        let first_index = indices.get(0).copied().unwrap_or(0);
        let last_index = indices.last().copied().unwrap_or_else(|| std::cmp::min(line_to_render.chars().count(), max_width));
        if line_to_render.width() <= max_width {
            // there's enough room, no need to truncate
            None
        } else if last_index.saturating_sub(first_index) < max_width {
            // truncate around the indices
            let mut width_remaining = max_width.saturating_sub(1).saturating_sub(last_index.saturating_sub(first_index));

            let mut string_start_position = first_index;
            let mut string_end_position = last_index;

            let mut i = 0;
            loop {
                if i >= width_remaining {
                    break;
                }
                if string_start_position > 0 && string_end_position < line_to_render.chars().count() {
                    let take_from_start = i % 2 == 0;
                    if take_from_start {
                        string_start_position -= 1;
                        if string_start_position == 0 {
                            width_remaining += 2; // no need for truncating dots
                        }
                    } else {
                        string_end_position += 1;
                        if string_end_position == line_to_render.chars().count() {
                            width_remaining += 2; // no need for truncating dots
                        }
                    }
                } else if string_end_position < line_to_render.chars().count() {
                    string_end_position += 1;
                    if string_end_position == line_to_render.chars().count() {
                        width_remaining += 2; // no need for truncating dots
                    }
                } else if string_start_position > 0 {
                    string_start_position -= 1;
                    if string_start_position == 0 {
                        width_remaining += 2; // no need for truncating dots
                    }
                } else {
                    break;
                }
                i += 1;
            }
            Some((string_start_position, string_end_position))
        } else if !indices.is_empty() {
            // no room for all indices, remove the last one and try again
            let mut new_indices = indices.clone();
            drop(new_indices.pop());
            self.truncate_line_with_indices(line_to_render, &new_indices, max_width)
        } else {
            Some((first_index, last_index))
        }
    }
}
