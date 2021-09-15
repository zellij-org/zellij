use crate::ui::boundaries::boundary_type;
use ansi_term::Colour::{Fixed, RGB};
use ansi_term::Style;
use zellij_utils::pane_size::Viewport;
use zellij_utils::zellij_tile::prelude::PaletteColor;

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

#[derive(Default, PartialEq)]
pub struct PaneFrame {
    pub geom: Viewport,
    pub title: String,
    pub scroll_position: (usize, usize), // (position, length)
    pub color: Option<PaletteColor>,
}

impl PaneFrame {
    fn render_title_right_side(&self, max_length: usize) -> Option<String> {
        if self.scroll_position.0 > 0 || self.scroll_position.1 > 0 {
            let prefix = " SCROLL: ";
            let full_indication =
                format!(" {}/{} ", self.scroll_position.0, self.scroll_position.1);
            let short_indication = format!(" {} ", self.scroll_position.0);
            if prefix.width() + full_indication.width() <= max_length {
                Some(format!("{}{}", prefix, full_indication))
            } else if full_indication.width() <= max_length {
                Some(full_indication)
            } else if short_indication.width() <= max_length {
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
        if max_length <= 6 || self.title.is_empty() {
            None
        } else if full_text.width() <= max_length {
            Some(full_text)
        } else {
            let length_of_each_half = (max_length - middle_truncated_sign.width()) / 2;

            let mut first_part: String = String::with_capacity(length_of_each_half);
            for char in full_text.chars() {
                if first_part.width() + char.width().unwrap_or(0) > length_of_each_half {
                    break;
                } else {
                    first_part.push(char);
                }
            }

            let mut second_part: String = String::with_capacity(length_of_each_half);
            for char in full_text.chars().rev() {
                if second_part.width() + char.width().unwrap_or(0) > length_of_each_half {
                    break;
                } else {
                    second_part.insert(0, char);
                }
            }

            let title_left_side =
                if first_part.width() + middle_truncated_sign.width() + second_part.width()
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
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let left_boundary = boundary_type::TOP_LEFT;
        let right_boundary = boundary_type::TOP_RIGHT;
        let left_side = self.render_title_left_side(total_title_length);
        let right_side = left_side.as_ref().and_then(|left_side| {
            let space_left = total_title_length.saturating_sub(left_side.width() + 1); // 1 for a middle separator
            self.render_title_right_side(space_left)
        });
        let title_text = match (left_side, right_side) {
            (Some(left_side), Some(right_side)) => {
                let mut middle = String::new();
                for _ in (left_side.width() + right_side.width())..total_title_length {
                    middle.push_str(boundary_type::HORIZONTAL);
                }
                format!(
                    "{}{}{}{}{}",
                    left_boundary, left_side, middle, right_side, right_boundary
                )
            }
            (Some(left_side), None) => {
                let mut middle_padding = String::new();
                for _ in left_side.width()..total_title_length {
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
            self.geom.y + 1, // +1 because goto is 1 indexed
            self.geom.x + 1, // +1 because goto is 1 indexed
            color_string(&title_text, self.color),
        )); // goto row/col + boundary character
    }
    pub fn render(&self) -> String {
        let mut vte_output = String::new();
        for row in self.geom.y..(self.geom.y + self.geom.rows) {
            if row == self.geom.y {
                // top row
                self.render_title(&mut vte_output);
            } else if row == self.geom.y + self.geom.rows - 1 {
                // bottom row
                for col in self.geom.x..(self.geom.x + self.geom.cols) {
                    if col == self.geom.x {
                        // bottom left corner
                        vte_output.push_str(&format!(
                            "\u{1b}[{};{}H\u{1b}[m{}",
                            row + 1, // +1 because goto is 1 indexed
                            col + 1,
                            color_string(boundary_type::BOTTOM_LEFT, self.color),
                        )); // goto row/col + boundary character
                    } else if col == self.geom.x + self.geom.cols - 1 {
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
                    self.geom.x + 1,
                    color_string(boundary_type::VERTICAL, self.color),
                )); // goto row/col + boundary character
                vte_output.push_str(&format!(
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    row + 1, // +1 because goto is 1 indexed
                    self.geom.x + self.geom.cols,
                    color_string(boundary_type::VERTICAL, self.color),
                )); // goto row/col + boundary character
            }
        }
        vte_output
    }
}
