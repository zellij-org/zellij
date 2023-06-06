use crate::search::ui::{ORANGE, styled_text_foreground, bold};

pub struct SelectionControlsArea {
    display_lines: usize,
    display_columns: usize
}

impl SelectionControlsArea {
    pub fn new(display_lines: usize, display_columns: usize) -> Self {
        SelectionControlsArea {
            display_lines,
            display_columns
        }
    }
    pub fn render(&self, result_count: usize) -> String {
        let mut to_render = String::new();
        let padding = self.display_lines.saturating_sub(result_count);
        for _ in 0..padding {
            to_render.push_str(&self.render_padding_line());
        }
        let selection_controls = self.render_selection_controls();
        to_render.push_str(&selection_controls);
        to_render
    }
    pub fn render_empty_lines(&self) -> String {
        let mut to_render = String::new();
        for _ in 0..self.display_lines {
            to_render.push_str("\n");
        }
        to_render
    }
    fn render_padding_line(&self) -> String {
        format!("│\n")
    }
    fn render_selection_controls(&self) -> String {
        if self.display_columns >= self.full_selection_controls_len() {
            self.render_full_selection_controls()
        } else {
            self.render_truncated_selection_controls()
        }
    }
    fn full_selection_controls_len(&self) -> usize {
        62
    }
    fn render_full_selection_controls(&self) -> String {
        let arrow_tail = "└ ";
        let enter = styled_text_foreground(ORANGE, &bold("<ENTER>"));
        let enter_tip = bold(" - open in editor. ");
        let tab = styled_text_foreground(ORANGE, &bold("<TAB>"));
        let tab_tip = bold(" - open terminal at location.");
        format!("{}{}{}{}{}", arrow_tail, enter, enter_tip, tab, tab_tip)
    }
    fn render_truncated_selection_controls(&self) -> String {
        let arrow_tail = "└ ";
        let enter = styled_text_foreground(ORANGE, &bold("<ENTER>"));
        let enter_tip = bold(" - edit. ");
        let tab = styled_text_foreground(ORANGE, &bold("<TAB>"));
        let tab_tip = bold(" - terminal.");
        format!("{}{}{}{}{}", arrow_tail, enter, enter_tip, tab, tab_tip)
    }
}

