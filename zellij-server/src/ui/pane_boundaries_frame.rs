use zellij_utils::pane_size::PositionAndSize;
use zellij_utils::zellij_tile::prelude::PaletteColor;
use crate::ui::boundaries::boundary_type;
use ansi_term::Colour::{Fixed, RGB};
use ansi_term::Style;

fn color_string(character: &str, color: Option<PaletteColor>) -> String {
    match color {
        Some(color) => match color {
            PaletteColor::Rgb((r, g, b)) => {
                format!("{}", RGB(r, g, b).bold().paint(character))
            }
            PaletteColor::EightBit(color) => {
                format!("{}", Fixed(color).bold().paint(character))
            }
        },
        None => format!("{}", Style::new().bold().paint(character)),
    }
}

pub struct PaneBoundariesFrame {
    pub position_and_size: PositionAndSize,
    base_title: String,
    title: String,
    scroll_position: (usize, usize), // (position, length)
    pub color: Option<PaletteColor>,
    pub draw_title_only: bool,
}

impl PaneBoundariesFrame {
    pub fn new(position_and_size: PositionAndSize, title: String) -> Self {
        PaneBoundariesFrame {
            position_and_size,
            color: None,
            base_title: title.clone(),
            title,
            scroll_position: (0, 0),
            draw_title_only: false,
        }
    }
    pub fn frame_title_only(mut self) -> Self {
        self.draw_title_only = true;
        self
    }
    pub fn set_should_render_frame_title(&mut self, should_render_frame_title: bool) {
        self.draw_title_only = should_render_frame_title;
    }
    pub fn change_pos_and_size(&mut self, position_and_size: PositionAndSize) {
        self.position_and_size = position_and_size;
    }
    pub fn set_color(&mut self, color: Option<PaletteColor>) {
        self.color = color;
    }
    pub fn update_scroll(&mut self, scroll_position: (usize, usize)) {
        self.scroll_position = scroll_position;
    }
    pub fn update_title(&mut self, title: Option<&String>) {
        match title {
            Some(title) => {
                self.title = title.clone();
            }
            None => {
                self.title = self.base_title.clone();
            }
        }
    }
    pub fn content_position_and_size(&self) -> PositionAndSize {
        if self.draw_title_only {
            self.position_and_size.reduce_top_line()
        } else {
            self.position_and_size.reduce_outer_frame(1)
        }
    }
    fn render_title(&self, vte_output: &mut String) {
        // TODO: crop title parts to fit length so they don't overflow if they are super long
        if self.draw_title_only {
//             let title_text_prefix = format!("{} {} ", " ", self.title);
//             let title_text_suffix = format!(" SCROLL: {}/{} {}", self.scroll_position.0, self.scroll_position.1, " ");
            let title_text_prefix = format!("{} {} ", boundary_type::HORIZONTAL, self.title);
            let title_text_suffix = if self.scroll_position.0 > 0 || self.scroll_position.1 > 0 {
                format!(" SCROLL: {}/{} {}", self.scroll_position.0, self.scroll_position.1, boundary_type::HORIZONTAL)
            } else {
                format!("{}", boundary_type::HORIZONTAL)
            };
            let mut title_text = String::new();
            title_text.push_str(&title_text_prefix);
            let title_text_length = title_text.chars().count();
            for col in self.position_and_size.x + title_text_length..(self.position_and_size.x + self.position_and_size.cols).saturating_sub(title_text_suffix.chars().count()) {
                // title_text.push_str(boundary_type::HORIZONTAL);
                title_text.push_str(boundary_type::HORIZONTAL);
            }
            title_text.push_str(&title_text_suffix);
            vte_output.push_str(&format!(
                "\u{1b}[{};{}H\u{1b}[m{}",
                self.position_and_size.y + 1, // +1 because goto is 1 indexed
                self.position_and_size.x + 1, // +1 because goto is 1 indexed
                color_string(&title_text, self.color),
            )); // goto row/col + boundary character
        } else {
            let title_text_prefix = format!("{} {} ", boundary_type::TOP_LEFT, self.title);
            let title_text_suffix = if self.scroll_position.0 > 0 || self.scroll_position.1 > 0 {
                format!(" SCROLL: {}/{} {}", self.scroll_position.0, self.scroll_position.1, boundary_type::TOP_RIGHT)
            } else {
                format!("{}", boundary_type::TOP_RIGHT)
            };
            let mut title_text = String::new();
            title_text.push_str(&title_text_prefix);
            let title_text_length = title_text.chars().count();
            for col in self.position_and_size.x + title_text_length..(self.position_and_size.x + self.position_and_size.cols).saturating_sub(title_text_suffix.chars().count()) {
                title_text.push_str(boundary_type::HORIZONTAL);
            }
            title_text.push_str(&title_text_suffix);
            vte_output.push_str(&format!(
                "\u{1b}[{};{}H\u{1b}[m{}",
                self.position_and_size.y + 1, // +1 because goto is 1 indexed
                self.position_and_size.x + 1, // +1 because goto is 1 indexed
                color_string(&title_text, self.color),
            )); // goto row/col + boundary character
        }
    }
    pub fn render(&self) -> String {
        let mut vte_output = String::new();
        if self.draw_title_only {
            self.render_title(&mut vte_output);
        } else {
            for row in self.position_and_size.y..(self.position_and_size.y + self.position_and_size.rows) {
                if row == self.position_and_size.y {
                    // top row
                    self.render_title(&mut vte_output);
                } else if row == self.position_and_size.y + self.position_and_size.rows - 1 {
                    // bottom row
                    for col in self.position_and_size.x..(self.position_and_size.x + self.position_and_size.cols) {
                        if col == self.position_and_size.x {
                            // bottom left corner
                            vte_output.push_str(&format!(
                                "\u{1b}[{};{}H\u{1b}[m{}",
                                row + 1, // +1 because goto is 1 indexed
                                col + 1,
                                color_string(boundary_type::BOTTOM_LEFT, self.color),
                            )); // goto row/col + boundary character
                        } else if col == self.position_and_size.x + self.position_and_size.cols - 1 {
                            // bottom right corner
                            vte_output.push_str(&format!(
                                "\u{1b}[{};{}H\u{1b}[m{}",
                                row + 1, // +1 because goto is 1 indexed
                                col + 1,
                                color_string(boundary_type::BOTTOM_RIGHT, self.color),
                            )); // goto row/col + boundary character
                        } else {
                            vte_output.push_str(&format!(
                                "\u{1b}[{};{}H\u{1b}[m{}",
                                row + 1, // +1 because goto is 1 indexed
                                col + 1,
                                color_string(boundary_type::HORIZONTAL, self.color),
                            )); // goto row/col + boundary character
                        }
                    }
                } else {
                    vte_output.push_str(&format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        row + 1, // +1 because goto is 1 indexed
                        self.position_and_size.x + 1,
                        color_string(boundary_type::VERTICAL, self.color),
                    )); // goto row/col + boundary character
                    vte_output.push_str(&format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        row + 1, // +1 because goto is 1 indexed
                        self.position_and_size.x + self.position_and_size.cols,
                        color_string(boundary_type::VERTICAL, self.color),
                    )); // goto row/col + boundary character
                }
            }
        }
        vte_output
    }
}
