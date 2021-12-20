use crate::ui::boundaries::boundary_type;
use crate::ClientId;
use ansi_term::Colour::{Fixed, RGB};
use ansi_term::Style;
use zellij_utils::pane_size::Viewport;
use zellij_utils::zellij_tile::prelude::{client_id_to_colors, Palette, PaletteColor};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use std::fmt::Write;

fn color_string(character: &str, color: Option<PaletteColor>) -> String {
    match color {
        Some(PaletteColor::Rgb((r, g, b))) => RGB(r, g, b).bold().paint(character).to_string(),
        Some(PaletteColor::EightBit(color)) => Fixed(color).bold().paint(character).to_string(),
        None => Style::new().bold().paint(character).to_string(),
    }
}

fn background_color(character: &str, color: Option<PaletteColor>) -> String {
    match color {
        Some(PaletteColor::Rgb((r, g, b))) => {
            Style::new().on(RGB(r, g, b)).paint(character).to_string()
        }
        Some(PaletteColor::EightBit(color)) => {
            Style::new().on(Fixed(color)).paint(character).to_string()
        }
        None => character.to_string(),
    }
}

pub struct FrameParams {
    pub focused_client: Option<ClientId>,
    pub is_main_client: bool,
    pub other_focused_clients: Vec<ClientId>,
    pub colors: Palette,
    pub color: Option<PaletteColor>,
    pub other_cursors_exist_in_session: bool,
}

#[derive(Default, PartialEq)]
pub struct PaneFrame {
    pub geom: Viewport,
    pub title: String,
    pub scroll_position: (usize, usize), // (position, length)
    pub colors: Palette,
    pub color: Option<PaletteColor>,
    pub focused_client: Option<ClientId>,
    pub is_main_client: bool,
    pub other_cursors_exist_in_session: bool,
    pub other_focused_clients: Vec<ClientId>,
}

impl PaneFrame {
    pub fn new(
        geom: Viewport,
        scroll_position: (usize, usize),
        main_title: String,
        frame_params: FrameParams,
    ) -> Self {
        PaneFrame {
            geom,
            title: main_title,
            scroll_position,
            colors: frame_params.colors,
            color: frame_params.color,
            focused_client: frame_params.focused_client,
            is_main_client: frame_params.is_main_client,
            other_focused_clients: frame_params.other_focused_clients,
            other_cursors_exist_in_session: frame_params.other_cursors_exist_in_session,
        }
    }
    fn client_cursor(&self, client_id: ClientId) -> String {
        let color = client_id_to_colors(client_id, self.colors);
        background_color(" ", color.map(|c| c.0))
    }
    fn render_title_right_side(&self, max_length: usize) -> Option<(String, usize)> {
        // string and length because of color
        if self.scroll_position.0 > 0 || self.scroll_position.1 > 0 {
            let prefix = " SCROLL: ";
            let full_indication =
                format!(" {}/{} ", self.scroll_position.0, self.scroll_position.1);
            let short_indication = format!(" {} ", self.scroll_position.0);
            let full_indication_len = full_indication.chars().count();
            let short_indication_len = short_indication.chars().count();
            let prefix_len = prefix.chars().count();
            if prefix_len + full_indication_len <= max_length {
                Some((
                    color_string(&format!("{}{}", prefix, full_indication), self.color),
                    prefix_len + full_indication_len,
                ))
            } else if full_indication_len <= max_length {
                Some((
                    color_string(&full_indication, self.color),
                    full_indication_len,
                ))
            } else if short_indication_len <= max_length {
                Some((
                    color_string(&short_indication, self.color),
                    short_indication_len,
                ))
            } else {
                None
            }
        } else {
            None
        }
    }
    fn render_my_focus(&self, max_length: usize) -> Option<(String, usize)> {
        let left_separator = color_string(boundary_type::VERTICAL_LEFT, self.color);
        let right_separator = color_string(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = "MY FOCUS";
        let full_indication = format!(
            "{} {} {}",
            left_separator,
            color_string(full_indication_text, self.color),
            right_separator
        );
        let full_indication_len = full_indication_text.width() + 4; // 2 for separators 2 for padding
        let short_indication_text = "ME";
        let short_indication = format!(
            "{} {} {}",
            left_separator,
            color_string(short_indication_text, self.color),
            right_separator
        );
        let short_indication_len = short_indication_text.width() + 4; // 2 for separators 2 for padding
        if full_indication_len <= max_length {
            Some((full_indication, full_indication_len))
        } else if short_indication_len <= max_length {
            Some((short_indication, short_indication_len))
        } else {
            None
        }
    }
    fn render_my_and_others_focus(&self, max_length: usize) -> Option<(String, usize)> {
        let left_separator = color_string(boundary_type::VERTICAL_LEFT, self.color);
        let right_separator = color_string(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = "MY FOCUS AND:";
        let short_indication_text = "+";
        let mut full_indication = color_string(full_indication_text, self.color);
        let mut full_indication_len = full_indication_text.width();
        let mut short_indication = color_string(short_indication_text, self.color);
        let mut short_indication_len = short_indication_text.width();
        for client_id in &self.other_focused_clients {
            let text = format!(" {}", self.client_cursor(*client_id));
            full_indication_len += 2;
            full_indication.push_str(&text);
            short_indication_len += 2;
            short_indication.push_str(&text);
        }
        if full_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            Some((
                format!("{} {} {}", left_separator, full_indication, right_separator),
                full_indication_len + 4,
            ))
        } else if short_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            Some((
                format!(
                    "{} {} {}",
                    left_separator, short_indication, right_separator
                ),
                short_indication_len + 4,
            ))
        } else {
            None
        }
    }
    fn render_other_focused_users(&self, max_length: usize) -> Option<(String, usize)> {
        let left_separator = color_string(boundary_type::VERTICAL_LEFT, self.color);
        let right_separator = color_string(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = if self.other_focused_clients.len() == 1 {
            "FOCUSED USER:"
        } else {
            "FOCUSED USERS:"
        };
        let middle_indication_text = "U:";
        let mut full_indication = color_string(full_indication_text, self.color);
        let mut full_indication_len = full_indication_text.width();
        let mut middle_indication = color_string(middle_indication_text, self.color);
        let mut middle_indication_len = middle_indication_text.width();
        let mut short_indication = String::from("");
        let mut short_indication_len = 0;
        for client_id in &self.other_focused_clients {
            let text = format!(" {}", self.client_cursor(*client_id));
            full_indication_len += 2;
            full_indication.push_str(&text);
            middle_indication_len += 2;
            middle_indication.push_str(&text);
            short_indication_len += 2;
            short_indication.push_str(&text);
        }
        if full_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            Some((
                format!("{} {} {}", left_separator, full_indication, right_separator),
                full_indication_len + 4,
            ))
        } else if middle_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            Some((
                format!(
                    "{} {} {}",
                    left_separator, middle_indication, right_separator
                ),
                middle_indication_len + 4,
            ))
        } else if short_indication_len + 3 <= max_length {
            // 2 for separators, 1 for padding
            Some((
                format!("{}{} {}", left_separator, short_indication, right_separator),
                short_indication_len + 3,
            ))
        } else {
            None
        }
    }
    fn render_title_middle(&self, max_length: usize) -> Option<(String, usize)> {
        // string and length because of color
        if self.is_main_client
            && self.other_focused_clients.is_empty()
            && !self.other_cursors_exist_in_session
        {
            None
        } else if self.is_main_client
            && self.other_focused_clients.is_empty()
            && self.other_cursors_exist_in_session
        {
            self.render_my_focus(max_length)
        } else if self.is_main_client && !self.other_focused_clients.is_empty() {
            self.render_my_and_others_focus(max_length)
        } else if !self.other_focused_clients.is_empty() {
            self.render_other_focused_users(max_length)
        } else {
            None
        }
    }
    fn render_title_left_side(&self, max_length: usize) -> Option<(String, usize)> {
        let middle_truncated_sign = "[..]";
        let middle_truncated_sign_long = "[...]";
        let full_text = format!(" {} ", &self.title);
        if max_length <= 6 || self.title.is_empty() {
            None
        } else if full_text.width() <= max_length {
            Some((
                color_string(&full_text, self.color),
                full_text.chars().count(),
            ))
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
            Some((
                color_string(&title_left_side, self.color),
                title_left_side.chars().count(),
            ))
        }
    }
    fn three_part_title_line(
        &self,
        left_side: &str,
        left_side_len: &usize,
        middle: &str,
        middle_len: &usize,
        right_side: &str,
        right_side_len: &usize,
    ) -> String {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = String::new();
        let left_side_start_position = self.geom.x + 1;
        let middle_start_position = self.geom.x + (total_title_length / 2) - (middle_len / 2) + 1;
        let right_side_start_position =
            (self.geom.x + self.geom.cols - 1).saturating_sub(*right_side_len);

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.push_str(&color_string(boundary_type::TOP_LEFT, self.color));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.push_str(&color_string(boundary_type::TOP_RIGHT, self.color));
            } else if col == left_side_start_position {
                title_line.push_str(left_side);
                col += left_side_len;
                continue;
            } else if col == middle_start_position {
                title_line.push_str(middle);
                col += middle_len;
                continue;
            } else if col == right_side_start_position {
                title_line.push_str(right_side);
                col += right_side_len;
                continue;
            } else {
                title_line.push_str(&color_string(boundary_type::HORIZONTAL, self.color));
                // TODO: BETTER
            }
            if col == self.geom.x + self.geom.cols - 1 {
                break;
            }
            col += 1;
        }
        title_line
    }
    fn left_and_middle_title_line(
        &self,
        left_side: &str,
        left_side_len: &usize,
        middle: &str,
        middle_len: &usize,
    ) -> String {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = String::new();
        let left_side_start_position = self.geom.x + 1;
        let middle_start_position = self.geom.x + (total_title_length / 2) - (*middle_len / 2) + 1;

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.push_str(&color_string(boundary_type::TOP_LEFT, self.color));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.push_str(&color_string(boundary_type::TOP_RIGHT, self.color));
            } else if col == left_side_start_position {
                title_line.push_str(left_side);
                col += *left_side_len;
                continue;
            } else if col == middle_start_position {
                title_line.push_str(middle);
                col += *middle_len;
                continue;
            } else {
                title_line.push_str(&color_string(boundary_type::HORIZONTAL, self.color));
                // TODO: BETTER
            }
            if col == self.geom.x + self.geom.cols - 1 {
                break;
            }
            col += 1;
        }
        title_line
    }
    fn middle_only_title_line(&self, middle: &str, middle_len: &usize) -> String {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = String::new();
        let middle_start_position = self.geom.x + (total_title_length / 2) - (*middle_len / 2) + 1;

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.push_str(&color_string(boundary_type::TOP_LEFT, self.color));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.push_str(&color_string(boundary_type::TOP_RIGHT, self.color));
            } else if col == middle_start_position {
                title_line.push_str(middle);
                col += *middle_len;
                continue;
            } else {
                title_line.push_str(&color_string(boundary_type::HORIZONTAL, self.color));
                // TODO: BETTER
            }
            if col == self.geom.x + self.geom.cols - 1 {
                break;
            }
            col += 1;
        }
        title_line
    }
    fn two_part_title_line(
        &self,
        left_side: &str,
        left_side_len: &usize,
        right_side: &str,
        right_side_len: &usize,
    ) -> String {
        let left_boundary = color_string(boundary_type::TOP_LEFT, self.color);
        let right_boundary = color_string(boundary_type::TOP_RIGHT, self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle = String::new();
        for _ in (left_side_len + right_side_len)..total_title_length {
            middle.push_str(boundary_type::HORIZONTAL);
        }
        format!(
            "{}{}{}{}{}",
            left_boundary,
            left_side,
            color_string(&middle, self.color),
            color_string(right_side, self.color),
            &right_boundary
        )
    }
    fn left_only_title_line(&self, left_side: &str, left_side_len: &usize) -> String {
        let left_boundary = color_string(boundary_type::TOP_LEFT, self.color);
        let right_boundary = color_string(boundary_type::TOP_RIGHT, self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle_padding = String::new();
        for _ in *left_side_len..total_title_length {
            middle_padding.push_str(boundary_type::HORIZONTAL);
        }
        format!(
            "{}{}{}{}",
            left_boundary,
            left_side,
            color_string(&middle_padding, self.color),
            &right_boundary
        )
    }
    fn empty_title_line(&self) -> String {
        let left_boundary = color_string(boundary_type::TOP_LEFT, self.color);
        let right_boundary = color_string(boundary_type::TOP_RIGHT, self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle_padding = String::new();
        for _ in 0..total_title_length {
            middle_padding.push_str(boundary_type::HORIZONTAL);
        }
        format!(
            "{}{}{}",
            left_boundary,
            color_string(&middle_padding, self.color),
            right_boundary
        )
    }
    fn title_line_with_middle(&self, middle: &str, middle_len: &usize) -> String {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let length_of_each_side = total_title_length.saturating_sub(*middle_len + 2) / 2;
        let mut left_side = self.render_title_left_side(length_of_each_side);
        let mut right_side = self.render_title_right_side(length_of_each_side);

        match (&mut left_side, &mut right_side) {
            (Some((left_side, left_side_len)), Some((right_side, right_side_len))) => self
                .three_part_title_line(
                    left_side,
                    left_side_len,
                    middle,
                    middle_len,
                    right_side,
                    right_side_len,
                ),
            (Some((left_side, left_side_len)), None) => {
                self.left_and_middle_title_line(left_side, left_side_len, middle, middle_len)
            }
            _ => self.middle_only_title_line(middle, middle_len),
        }
    }
    fn title_line_without_middle(&self) -> String {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let left_side = self.render_title_left_side(total_title_length);
        let right_side = left_side.as_ref().and_then(|(_left_side, left_side_len)| {
            let space_left = total_title_length.saturating_sub(*left_side_len + 1); // 1 for a middle separator
            self.render_title_right_side(space_left)
        });
        match (left_side, right_side) {
            (Some((left_side, left_side_len)), Some((right_side, right_side_len))) => {
                self.two_part_title_line(&left_side, &left_side_len, &right_side, &right_side_len)
            }
            (Some((left_side, left_side_len)), None) => {
                self.left_only_title_line(&left_side, &left_side_len)
            }
            _ => self.empty_title_line(),
        }
    }
    fn render_title(&self, vte_output: &mut String) {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners

        if let Some((middle, middle_length)) = &self.render_title_middle(total_title_length) {
            let title_text = self.title_line_with_middle(middle, middle_length);
            write!(
                vte_output,
                "\u{1b}[{};{}H\u{1b}[m{}",
                self.geom.y + 1, // +1 because goto is 1 indexed
                self.geom.x + 1, // +1 because goto is 1 indexed
                color_string(&title_text, self.color),
            )
            .unwrap(); // goto row/col + boundary character
        } else {
            let title_text = self.title_line_without_middle();
            write!(
                vte_output,
                "\u{1b}[{};{}H\u{1b}[m{}",
                self.geom.y + 1, // +1 because goto is 1 indexed
                self.geom.x + 1, // +1 because goto is 1 indexed
                color_string(&title_text, self.color),
            )
            .unwrap(); // goto row/col + boundary character
        }
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
                    let boundary = if col == self.geom.x {
                        // bottom left corner
                        boundary_type::BOTTOM_LEFT
                    } else if col == self.geom.x + self.geom.cols - 1 {
                        // bottom right corner
                        boundary_type::BOTTOM_RIGHT
                    } else {
                        boundary_type::HORIZONTAL
                    };

                    let boundary_rendered = color_string(boundary, self.color);
                    write!(
                        &mut vte_output,
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        row + 1,
                        col + 1,
                        boundary_rendered
                    )
                    .unwrap();
                }
            } else {
                write!(
                    &mut vte_output,
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    row + 1, // +1 because goto is 1 indexed
                    self.geom.x + 1,
                    color_string(boundary_type::VERTICAL, self.color),
                )
                .unwrap(); // goto row/col + boundary character
                write!(
                    &mut vte_output,
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    row + 1, // +1 because goto is 1 indexed
                    self.geom.x + self.geom.cols,
                    color_string(boundary_type::VERTICAL, self.color),
                )
                .unwrap(); // goto row/col + boundary character
            }
        }
        vte_output
    }
}
