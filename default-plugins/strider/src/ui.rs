use crate::search_state::SearchState;
use crate::controls_line::{ControlsLine, Control};
use crate::selection_controls_area::SelectionControlsArea;
use std::fmt::{Display, Formatter, Result};

pub const CYAN: u8 = 51;
pub const GRAY_LIGHT: u8 = 238;
pub const GRAY_DARK: u8 = 245;
pub const WHITE: u8 = 15;
pub const BLACK: u8 = 16;
pub const RED: u8 = 124;
pub const GREEN: u8 = 154;
pub const ORANGE: u8 = 166;

impl Display for SearchState {
    fn fmt(&self, f: &mut Formatter) -> Result {
        write!(f, "{}", self.render_search_line())?;
        write!(f, "{}", self.render_search_results())?;
        write!(f, "{}", self.render_selection_control_area())?;
        write!(f, "{}", self.render_controls_line())?;
        Ok(())
    }
}

impl SearchState {
    pub fn render_search_line(&self) -> String {
        format!("{}{}\n", styled_text_foreground(CYAN, &bold("SEARCH: ")), self.search_term)
    }
    pub fn render_search_results(&self) -> String {
        let mut space_for_results = self.display_rows.saturating_sub(3); // title and both controls lines
        let mut to_render = String::new();
        for (i, search_result) in self.displayed_search_results.1.iter().enumerate() {
            let result_height = search_result.rendered_height();
            if space_for_results < result_height {
                break;
            }
            space_for_results -= result_height;
            let index_of_selected_result = self.displayed_search_results.0;
            let is_selected = i == index_of_selected_result;
            let is_below_search_result = i > index_of_selected_result;
            let rendered_result = search_result.render(self.display_columns, is_selected, is_below_search_result);
            to_render.push_str(&format!("{}", rendered_result));
            to_render.push('\n')
        }
        to_render
    }
    pub fn render_selection_control_area(&self) -> String {
        let rows_for_results = self.rows_for_results();
        if !self.displayed_search_results.1.is_empty() {
            format!("{}\n", SelectionControlsArea::new(rows_for_results, self.display_columns).render(self.number_of_lines_in_displayed_search_results()))
        } else {
            format!("{}\n", SelectionControlsArea::new(rows_for_results, self.display_columns).render_empty_lines())
        }
    }
    pub fn render_controls_line(&self) -> String {
        let has_results = !self.displayed_search_results.1.is_empty();
        let tiled_floating_control = Control::new_floating_control("Ctrl f", self.should_open_floating);
        let names_contents_control = Control::new_filter_control("Ctrl r", &self.search_filter);
        if self.loading {
            ControlsLine::new(vec![tiled_floating_control, names_contents_control], Some(vec!["Scanning folder", "Scanning", "S"]))
                .with_animation_offset(self.loading_animation_offset)
                .render(self.display_columns, has_results)
        } else {
            ControlsLine::new(vec![tiled_floating_control, names_contents_control], None)
                .render(self.display_columns, has_results)
        }
    }
}

pub fn bold(text: &str) -> String {
    format!("\u{1b}[1m{}\u{1b}[m", text)
}

pub fn underline (text: &str) -> String {
    format!("\u{1b}[4m{}\u{1b}[m", text)
}

pub fn styled_text(foreground_color: u8, background_color: u8, text: &str) -> String {
    format!("\u{1b}[38;5;{};48;5;{}m{}\u{1b}[m", foreground_color, background_color, text)
}

pub fn styled_text_foreground(foreground_color: u8, text: &str) -> String {
    format!("\u{1b}[38;5;{}m{}\u{1b}[m", foreground_color, text)
}

pub fn styled_text_background(background_color: u8, text: &str) -> String {
    format!("\u{1b}[48;5;{}m{}\u{1b}[m", background_color, text)
}

pub fn color_line_to_end(background_color: u8) -> String {
    format!("\u{1b}[48;5;{}m\u{1b}[0K", background_color)
}

pub fn arrow(foreground: u8, background: u8)-> String {
    format!("\u{1b}[38;5;{}m\u{1b}[48;5;{}m", foreground, background)
}

pub fn dot(foreground: u8, background: u8) -> String {
    format!("\u{1b}[38;5;{};48;5;{}m•", foreground, background)
}

