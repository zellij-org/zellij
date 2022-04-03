use crate::output::CharacterChunk;
use crate::panes::{AnsiCode, CharacterStyles, TerminalCharacter, EMPTY_TERMINAL_CHARACTER};
use crate::ui::boundaries::boundary_type;
use crate::ClientId;
use zellij_tile::prelude::Style;
use zellij_utils::pane_size::Viewport;
use zellij_utils::zellij_tile::prelude::{client_id_to_colors, PaletteColor};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn foreground_color(characters: &str, color: Option<PaletteColor>) -> Vec<TerminalCharacter> {
    let mut colored_string = Vec::with_capacity(characters.chars().count());
    for character in characters.chars() {
        let styles = match color {
            Some(palette_color) => {
                let mut styles = CharacterStyles::new();
                styles.reset_all();
                styles
                    .foreground(Some(AnsiCode::from(palette_color)))
                    .bold(Some(AnsiCode::On))
            }
            None => {
                let mut styles = CharacterStyles::new();
                styles.reset_all();
                styles.bold(Some(AnsiCode::On))
            }
        };
        let terminal_character = TerminalCharacter {
            character,
            styles,
            width: character.width().unwrap_or(0),
        };
        colored_string.push(terminal_character);
    }
    colored_string
}

fn background_color(characters: &str, color: Option<PaletteColor>) -> Vec<TerminalCharacter> {
    let mut colored_string = Vec::with_capacity(characters.chars().count());
    for character in characters.chars() {
        let styles = match color {
            Some(palette_color) => {
                let mut styles = CharacterStyles::new();
                styles.reset_all();
                styles
                    .background(Some(AnsiCode::from(palette_color)))
                    .bold(Some(AnsiCode::On))
            }
            None => {
                let mut styles = CharacterStyles::new();
                styles.reset_all();
                styles
            }
        };
        let terminal_character = TerminalCharacter {
            character,
            styles,
            width: character.width().unwrap_or(0),
        };
        colored_string.push(terminal_character);
    }
    colored_string
}

pub struct FrameParams {
    pub focused_client: Option<ClientId>,
    pub is_main_client: bool,
    pub other_focused_clients: Vec<ClientId>,
    pub style: Style,
    pub color: Option<PaletteColor>,
    pub other_cursors_exist_in_session: bool,
}

#[derive(Default, PartialEq)]
pub struct PaneFrame {
    pub geom: Viewport,
    pub title: String,
    pub scroll_position: (usize, usize), // (position, length)
    pub style: Style,
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
            style: frame_params.style,
            color: frame_params.color,
            focused_client: frame_params.focused_client,
            is_main_client: frame_params.is_main_client,
            other_focused_clients: frame_params.other_focused_clients,
            other_cursors_exist_in_session: frame_params.other_cursors_exist_in_session,
        }
    }
    fn client_cursor(&self, client_id: ClientId) -> Vec<TerminalCharacter> {
        let color = client_id_to_colors(client_id, self.style.colors);
        background_color(" ", color.map(|c| c.0))
    }
    fn get_corner(&self, corner: &'static str) -> &'static str {
        if self.style.rounded_corners {
            match corner {
                boundary_type::TOP_RIGHT => boundary_type::TOP_RIGHT_ROUND,
                boundary_type::TOP_LEFT => boundary_type::TOP_LEFT_ROUND,
                boundary_type::BOTTOM_RIGHT => boundary_type::BOTTOM_RIGHT_ROUND,
                boundary_type::BOTTOM_LEFT => boundary_type::BOTTOM_LEFT_ROUND,
                _ => corner,
            }
        } else {
            corner
        }
    }
    fn render_title_right_side(
        &self,
        max_length: usize,
    ) -> Option<(Vec<TerminalCharacter>, usize)> {
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
                    foreground_color(&format!("{}{}", prefix, full_indication), self.color),
                    prefix_len + full_indication_len,
                ))
            } else if full_indication_len <= max_length {
                Some((
                    foreground_color(&full_indication, self.color),
                    full_indication_len,
                ))
            } else if short_indication_len <= max_length {
                Some((
                    foreground_color(&short_indication, self.color),
                    short_indication_len,
                ))
            } else {
                None
            }
        } else {
            None
        }
    }
    fn render_my_focus(&self, max_length: usize) -> Option<(Vec<TerminalCharacter>, usize)> {
        let mut left_separator = foreground_color(boundary_type::VERTICAL_LEFT, self.color);
        let mut right_separator = foreground_color(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = "MY FOCUS";
        let mut full_indication = vec![];
        full_indication.append(&mut left_separator);
        full_indication.push(EMPTY_TERMINAL_CHARACTER);
        full_indication.append(&mut foreground_color(full_indication_text, self.color));
        full_indication.push(EMPTY_TERMINAL_CHARACTER);
        full_indication.append(&mut right_separator);
        let full_indication_len = full_indication_text.width() + 4; // 2 for separators 2 for padding
        let short_indication_text = "ME";
        let mut short_indication = vec![];
        short_indication.append(&mut left_separator);
        short_indication.push(EMPTY_TERMINAL_CHARACTER);
        short_indication.append(&mut foreground_color(short_indication_text, self.color));
        short_indication.push(EMPTY_TERMINAL_CHARACTER);
        short_indication.append(&mut right_separator);
        let short_indication_len = short_indication_text.width() + 4; // 2 for separators 2 for padding
        if full_indication_len <= max_length {
            Some((full_indication, full_indication_len))
        } else if short_indication_len <= max_length {
            Some((short_indication, short_indication_len))
        } else {
            None
        }
    }
    fn render_my_and_others_focus(
        &self,
        max_length: usize,
    ) -> Option<(Vec<TerminalCharacter>, usize)> {
        let mut left_separator = foreground_color(boundary_type::VERTICAL_LEFT, self.color);
        let mut right_separator = foreground_color(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = "MY FOCUS AND:";
        let short_indication_text = "+";
        let mut full_indication = foreground_color(full_indication_text, self.color);
        let mut full_indication_len = full_indication_text.width();
        let mut short_indication = foreground_color(short_indication_text, self.color);
        let mut short_indication_len = short_indication_text.width();
        for client_id in &self.other_focused_clients {
            let mut text = self.client_cursor(*client_id);
            full_indication_len += 2;
            full_indication.push(EMPTY_TERMINAL_CHARACTER);
            full_indication.append(&mut text.clone());
            short_indication_len += 2;
            short_indication.append(&mut text);
        }
        if full_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            let mut ret = vec![];
            ret.append(&mut left_separator);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut full_indication);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut right_separator);
            Some((ret, full_indication_len + 4))
        } else if short_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            let mut ret = vec![];
            ret.append(&mut left_separator);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut short_indication);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut right_separator);
            Some((ret, short_indication_len + 4))
        } else {
            None
        }
    }
    fn render_other_focused_users(
        &self,
        max_length: usize,
    ) -> Option<(Vec<TerminalCharacter>, usize)> {
        let mut left_separator = foreground_color(boundary_type::VERTICAL_LEFT, self.color);
        let mut right_separator = foreground_color(boundary_type::VERTICAL_RIGHT, self.color);
        let full_indication_text = if self.other_focused_clients.len() == 1 {
            "FOCUSED USER:"
        } else {
            "FOCUSED USERS:"
        };
        let middle_indication_text = "U:";
        let mut full_indication = foreground_color(full_indication_text, self.color);
        let mut full_indication_len = full_indication_text.width();
        let mut middle_indication = foreground_color(middle_indication_text, self.color);
        let mut middle_indication_len = middle_indication_text.width();
        let mut short_indication = vec![];
        let mut short_indication_len = 0;
        for client_id in &self.other_focused_clients {
            let mut text = self.client_cursor(*client_id);
            full_indication_len += 2;
            full_indication.push(EMPTY_TERMINAL_CHARACTER);
            full_indication.append(&mut text.clone());
            middle_indication_len += 2;
            middle_indication.push(EMPTY_TERMINAL_CHARACTER);
            middle_indication.append(&mut text.clone());
            short_indication_len += 2;
            short_indication.push(EMPTY_TERMINAL_CHARACTER);
            short_indication.append(&mut text);
        }
        if full_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            let mut ret = vec![];
            ret.append(&mut left_separator);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut full_indication);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut right_separator);
            Some((ret, full_indication_len + 4))
        } else if middle_indication_len + 4 <= max_length {
            // 2 for separators, 2 for padding
            let mut ret = vec![];
            ret.append(&mut left_separator);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut middle_indication);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut right_separator);
            Some((ret, middle_indication_len + 4))
        } else if short_indication_len + 3 <= max_length {
            // 2 for separators, 1 for padding
            let mut ret = vec![];
            ret.append(&mut left_separator);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut short_indication);
            ret.push(EMPTY_TERMINAL_CHARACTER);
            ret.append(&mut right_separator);
            Some((ret, short_indication_len + 3))
        } else {
            None
        }
    }
    fn render_title_middle(&self, max_length: usize) -> Option<(Vec<TerminalCharacter>, usize)> {
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
    fn render_title_left_side(&self, max_length: usize) -> Option<(Vec<TerminalCharacter>, usize)> {
        let middle_truncated_sign = "[..]";
        let middle_truncated_sign_long = "[...]";
        let full_text = format!(" {} ", &self.title);
        if max_length <= 6 || self.title.is_empty() {
            None
        } else if full_text.width() <= max_length {
            Some((
                foreground_color(&full_text, self.color),
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

            let (title_left_side, title_length) = if first_part.width()
                + middle_truncated_sign.width()
                + second_part.width()
                < max_length
            {
                // this means we lost 1 character when dividing the total length into halves
                (
                    format!(
                        "{}{}{}",
                        first_part, middle_truncated_sign_long, second_part
                    ),
                    first_part.width() + middle_truncated_sign_long.width() + second_part.width(),
                )
            } else {
                (
                    format!("{}{}{}", first_part, middle_truncated_sign, second_part),
                    first_part.width() + middle_truncated_sign.width() + second_part.width(),
                )
            };
            Some((foreground_color(&title_left_side, self.color), title_length))
        }
    }
    fn three_part_title_line(
        &self,
        mut left_side: Vec<TerminalCharacter>,
        left_side_len: &usize,
        mut middle: Vec<TerminalCharacter>,
        middle_len: &usize,
        mut right_side: Vec<TerminalCharacter>,
        right_side_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = vec![];
        let left_side_start_position = self.geom.x + 1;
        let middle_start_position = self.geom.x + (total_title_length / 2) - (middle_len / 2) + 1;
        let right_side_start_position =
            (self.geom.x + self.geom.cols - 1).saturating_sub(*right_side_len);

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_LEFT),
                    self.color,
                ));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_RIGHT),
                    self.color,
                ));
            } else if col == left_side_start_position {
                title_line.append(&mut left_side);
                col += left_side_len;
                continue;
            } else if col == middle_start_position {
                title_line.append(&mut middle);
                col += middle_len;
                continue;
            } else if col == right_side_start_position {
                title_line.append(&mut right_side);
                col += right_side_len;
                continue;
            } else {
                title_line.append(&mut foreground_color(boundary_type::HORIZONTAL, self.color));
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
        mut left_side: Vec<TerminalCharacter>,
        left_side_len: &usize,
        mut middle: Vec<TerminalCharacter>,
        middle_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = vec![];
        let left_side_start_position = self.geom.x + 1;
        let middle_start_position = self.geom.x + (total_title_length / 2) - (*middle_len / 2) + 1;

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_LEFT),
                    self.color,
                ));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_RIGHT),
                    self.color,
                ));
            } else if col == left_side_start_position {
                title_line.append(&mut left_side);
                col += *left_side_len;
                continue;
            } else if col == middle_start_position {
                title_line.append(&mut middle);
                col += *middle_len;
                continue;
            } else {
                title_line.append(&mut foreground_color(boundary_type::HORIZONTAL, self.color));
            }
            if col == self.geom.x + self.geom.cols - 1 {
                break;
            }
            col += 1;
        }
        title_line
    }
    fn middle_only_title_line(
        &self,
        mut middle: Vec<TerminalCharacter>,
        middle_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut title_line = vec![];
        let middle_start_position = self.geom.x + (total_title_length / 2) - (*middle_len / 2) + 1;

        let mut col = self.geom.x;
        loop {
            if col == self.geom.x {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_LEFT),
                    self.color,
                ));
            } else if col == self.geom.x + self.geom.cols - 1 {
                title_line.append(&mut foreground_color(
                    self.get_corner(boundary_type::TOP_RIGHT),
                    self.color,
                ));
            } else if col == middle_start_position {
                title_line.append(&mut middle);
                col += *middle_len;
                continue;
            } else {
                title_line.append(&mut foreground_color(boundary_type::HORIZONTAL, self.color));
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
        mut left_side: Vec<TerminalCharacter>,
        left_side_len: &usize,
        mut right_side: Vec<TerminalCharacter>,
        right_side_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let mut left_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_LEFT), self.color);
        let mut right_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_RIGHT), self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle = String::new();
        for _ in (left_side_len + right_side_len)..total_title_length {
            middle.push_str(boundary_type::HORIZONTAL);
        }
        let mut ret = vec![];
        ret.append(&mut left_boundary);
        ret.append(&mut left_side);
        ret.append(&mut foreground_color(&middle, self.color));
        ret.append(&mut right_side);
        ret.append(&mut right_boundary);
        ret
    }
    fn left_only_title_line(
        &self,
        mut left_side: Vec<TerminalCharacter>,
        left_side_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let mut left_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_LEFT), self.color);
        let mut right_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_RIGHT), self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle_padding = String::new();
        for _ in *left_side_len..total_title_length {
            middle_padding.push_str(boundary_type::HORIZONTAL);
        }
        let mut ret = vec![];
        ret.append(&mut left_boundary);
        ret.append(&mut left_side);
        ret.append(&mut foreground_color(&middle_padding, self.color));
        ret.append(&mut right_boundary);
        ret
    }
    fn empty_title_line(&self) -> Vec<TerminalCharacter> {
        let mut left_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_LEFT), self.color);
        let mut right_boundary =
            foreground_color(self.get_corner(boundary_type::TOP_RIGHT), self.color);
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let mut middle_padding = String::new();
        for _ in 0..total_title_length {
            middle_padding.push_str(boundary_type::HORIZONTAL);
        }
        let mut ret = vec![];
        ret.append(&mut left_boundary);
        ret.append(&mut foreground_color(&middle_padding, self.color));
        ret.append(&mut right_boundary);
        ret
    }
    fn title_line_with_middle(
        &self,
        middle: Vec<TerminalCharacter>,
        middle_len: &usize,
    ) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let length_of_each_side = total_title_length.saturating_sub(*middle_len + 2) / 2;
        let left_side = self.render_title_left_side(length_of_each_side);
        let right_side = self.render_title_right_side(length_of_each_side);

        match (left_side, right_side) {
            (Some((left_side, left_side_len)), Some((right_side, right_side_len))) => self
                .three_part_title_line(
                    left_side,
                    &left_side_len,
                    middle,
                    middle_len,
                    right_side,
                    &right_side_len,
                ),
            (Some((left_side, left_side_len)), None) => {
                self.left_and_middle_title_line(left_side, &left_side_len, middle, middle_len)
            }
            _ => self.middle_only_title_line(middle, middle_len),
        }
    }
    fn title_line_without_middle(&self) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners
        let left_side = self.render_title_left_side(total_title_length);
        let right_side = left_side.as_ref().and_then(|(_left_side, left_side_len)| {
            let space_left = total_title_length.saturating_sub(*left_side_len + 1); // 1 for a middle separator
            self.render_title_right_side(space_left)
        });
        match (left_side, right_side) {
            (Some((left_side, left_side_len)), Some((right_side, right_side_len))) => {
                self.two_part_title_line(left_side, &left_side_len, right_side, &right_side_len)
            }
            (Some((left_side, left_side_len)), None) => {
                self.left_only_title_line(left_side, &left_side_len)
            }
            _ => self.empty_title_line(),
        }
    }
    fn render_title(&self) -> Vec<TerminalCharacter> {
        let total_title_length = self.geom.cols.saturating_sub(2); // 2 for the left and right corners

        self.render_title_middle(total_title_length)
            .map(|(middle, middle_length)| self.title_line_with_middle(middle, &middle_length))
            .or_else(|| Some(self.title_line_without_middle()))
            .unwrap()
    }
    pub fn render(&self) -> (Vec<CharacterChunk>, Option<String>) {
        let mut character_chunks = vec![];
        for row in 0..self.geom.rows {
            if row == 0 {
                // top row
                let title = self.render_title();
                let x = self.geom.x;
                let y = self.geom.y + row;
                character_chunks.push(CharacterChunk::new(title, x, y));
            } else if row == self.geom.rows - 1 {
                // bottom row
                let mut bottom_row = vec![];
                for col in 0..self.geom.cols {
                    let boundary = if col == 0 {
                        // bottom left corner
                        self.get_corner(boundary_type::BOTTOM_LEFT)
                    } else if col == self.geom.cols - 1 {
                        // bottom right corner
                        self.get_corner(boundary_type::BOTTOM_RIGHT)
                    } else {
                        boundary_type::HORIZONTAL
                    };

                    let mut boundary_character = foreground_color(boundary, self.color);
                    bottom_row.append(&mut boundary_character);
                }
                let x = self.geom.x;
                let y = self.geom.y + row;
                character_chunks.push(CharacterChunk::new(bottom_row, x, y));
            } else {
                let boundary_character_left = foreground_color(boundary_type::VERTICAL, self.color);
                let boundary_character_right =
                    foreground_color(boundary_type::VERTICAL, self.color);

                let x = self.geom.x;
                let y = self.geom.y + row;
                character_chunks.push(CharacterChunk::new(boundary_character_left, x, y));

                let x = (self.geom.x + self.geom.cols).saturating_sub(1);
                let y = self.geom.y + row;
                character_chunks.push(CharacterChunk::new(boundary_character_right, x, y));
            }
        }
        (character_chunks, None)
    }
}
