use crate::ui::boundaries::boundary_type;
use ansi_term::Colour::{Fixed, RGB};
use ansi_term::Style;
use zellij_utils::pane_size::PositionAndSize;
use zellij_utils::zellij_tile::prelude::PaletteColor;

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
    draw_title_only: bool,
    should_render: bool,
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
            should_render: true,
        }
    }
    pub fn frame_title_only(mut self) -> Self {
        // TODO: remove this?
        self.draw_title_only = true;
        self.should_render = true;
        self
    }
    pub fn render_only_title(&mut self, should_render_only_title: bool) {
        if should_render_only_title != self.draw_title_only {
            self.should_render = true;
            self.draw_title_only = should_render_only_title;
        }
    }
    pub fn change_pos_and_size(&mut self, position_and_size: PositionAndSize) {
        if position_and_size != self.position_and_size {
            self.position_and_size = position_and_size;
            self.should_render = true;
        }
    }
    pub fn set_color(&mut self, color: Option<PaletteColor>) {
        if color != self.color {
            self.color = color;
            self.should_render = true;
        }
    }
    pub fn update_scroll(&mut self, scroll_position: (usize, usize)) {
        if scroll_position != self.scroll_position {
            self.scroll_position = scroll_position;
            self.should_render = true;
        }
    }
    pub fn update_title(&mut self, title: Option<&String>) {
        match title {
            Some(title) => {
                if title != &self.title {
                    self.title = title.clone();
                    self.should_render = true;
                }
            }
            None => {
                self.title = self.base_title.clone();
                self.should_render = true;
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
    pub fn content_offset(&self) -> (usize, usize) {
        // (column_difference, row_difference)
        let content_position_and_size = self.content_position_and_size();
        let column_difference = content_position_and_size
            .x
            .saturating_sub(self.position_and_size.x);
        let row_difference = content_position_and_size
            .y
            .saturating_sub(self.position_and_size.y);
        (column_difference, row_difference)
    }
    pub fn set_should_render(&mut self, should_render: bool) {
        self.should_render = should_render;
    }
    fn render_title_right_side(&self, max_length: usize) -> Option<String> {
        if self.scroll_position.0 > 0 || self.scroll_position.1 > 0 {
            let prefix = " SCROLL: ";
            let full_indication =
                format!(" {}/{} ", self.scroll_position.0, self.scroll_position.1);
            let short_indication = format!(" {} ", self.scroll_position.0);
            if prefix.chars().count() + full_indication.chars().count() <= max_length {
                Some(format!("{}{}", prefix, full_indication))
            } else if full_indication.chars().count() <= max_length {
                Some(full_indication)
            } else if short_indication.chars().count() <= max_length {
                Some(short_indication)
            } else {
                None
            }
        } else {
            None
        }
    }
    fn render_title_left_side(&self, max_length: usize) -> Option<String> {
        let middle_truncated_sign = "[..]";
        let middle_truncated_sign_long = "[...]";
        let full_text = format!(" {} ", &self.title);
        if max_length <= 6 {
            None
        } else if full_text.chars().count() <= max_length {
            Some(full_text)
        } else {
            let length_of_each_half = (max_length - middle_truncated_sign.chars().count()) / 2;
            let first_part: String = full_text.chars().take(length_of_each_half).collect();
            let second_part: String = full_text
                .chars()
                .skip(full_text.chars().count() - length_of_each_half)
                .collect();
            let title_left_side = if first_part.chars().count()
                + middle_truncated_sign.chars().count()
                + second_part.chars().count()
                < max_length
            {
                // this means we lost 1 character when dividing the total length into halves
                format!(
                    "{}{}{}",
                    first_part, middle_truncated_sign_long, second_part
                )
            } else {
                format!("{}{}{}", first_part, middle_truncated_sign, second_part)
            };
            Some(title_left_side)
        }
    }
    fn render_title(&self, vte_output: &mut String) {
        let total_title_length = self.position_and_size.cols - 2; // 2 for the left and right corners
        let left_boundary = if self.draw_title_only {
            boundary_type::HORIZONTAL
        } else {
            boundary_type::TOP_LEFT
        };
        let right_boundary = if self.draw_title_only {
            boundary_type::HORIZONTAL
        } else {
            boundary_type::TOP_RIGHT
        };
        let left_side = self.render_title_left_side(total_title_length);
        let right_side = left_side.as_ref().and_then(|left_side| {
            let space_left = total_title_length.saturating_sub(left_side.chars().count() + 1); // 1 for a middle separator
            self.render_title_right_side(space_left)
        });
        let title_text = match (left_side, right_side) {
            (Some(left_side), Some(right_side)) => {
                let mut middle = String::new();
                for _ in
                    (left_side.chars().count() + right_side.chars().count())..total_title_length
                {
                    middle.push_str(boundary_type::HORIZONTAL);
                }
                format!(
                    "{}{}{}{}{}",
                    left_boundary, left_side, middle, right_side, right_boundary
                )
            }
            (Some(left_side), None) => {
                let mut middle_padding = String::new();
                for _ in left_side.chars().count()..total_title_length {
                    middle_padding.push_str(boundary_type::HORIZONTAL);
                }
                format!(
                    "{}{}{}{}",
                    left_boundary, left_side, middle_padding, right_boundary
                )
            }
            _ => {
                let mut middle_padding = String::new();
                for _ in 0..total_title_length {
                    middle_padding.push_str(boundary_type::HORIZONTAL);
                }
                format!("{}{}{}", left_boundary, middle_padding, right_boundary)
            }
        };
        vte_output.push_str(&format!(
            "\u{1b}[{};{}H\u{1b}[m{}",
            self.position_and_size.y + 1, // +1 because goto is 1 indexed
            self.position_and_size.x + 1, // +1 because goto is 1 indexed
            color_string(&title_text, self.color),
        )); // goto row/col + boundary character
    }
    pub fn render(&mut self) -> Option<String> {
        if !self.should_render {
            return None;
        }
        let mut vte_output = String::new();
        if self.draw_title_only {
            self.render_title(&mut vte_output);
        } else {
            for row in
                self.position_and_size.y..(self.position_and_size.y + self.position_and_size.rows)
            {
                if row == self.position_and_size.y {
                    // top row
                    self.render_title(&mut vte_output);
                } else if row == self.position_and_size.y + self.position_and_size.rows - 1 {
                    // bottom row
                    for col in self.position_and_size.x
                        ..(self.position_and_size.x + self.position_and_size.cols)
                    {
                        if col == self.position_and_size.x {
                            // bottom left corner
                            vte_output.push_str(&format!(
                                "\u{1b}[{};{}H\u{1b}[m{}",
                                row + 1, // +1 because goto is 1 indexed
                                col + 1,
                                color_string(boundary_type::BOTTOM_LEFT, self.color),
                            )); // goto row/col + boundary character
                        } else if col == self.position_and_size.x + self.position_and_size.cols - 1
                        {
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
        self.should_render = false;
        Some(vte_output)
    }
}
