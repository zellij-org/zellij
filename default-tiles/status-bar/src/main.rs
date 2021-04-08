mod first_line;
mod second_line;

use std::fmt::{Display, Error, Formatter};
use zellij_tile::{prelude::*, data::Theme};

use first_line::{ctrl_keys, superkey};
use second_line::keybinds;

pub mod colors {
    use ansi_term::Colour::{self, Fixed};
    pub const WHITE: Colour = Fixed(255);
    pub const BLACK: Colour = Fixed(16);
    pub const GREEN: Colour = Fixed(154);
    pub const ORANGE: Colour = Fixed(166);
    pub const GRAY: Colour = Fixed(238);
    pub const BRIGHT_GRAY: Colour = Fixed(245);
    pub const RED: Colour = Fixed(88);
}

// for more of these, copy paste from: https://en.wikipedia.org/wiki/Box-drawing_character
static ARROW_SEPARATOR: &str = "";
static MORE_MSG: &str = " ... ";

#[derive(Default)]
struct State {
    mode_info: ModeInfo,
}

register_tile!(State);

#[derive(Default)]
pub struct LinePart {
    part: String,
    len: usize,
}

impl Display for LinePart {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        write!(f, "{}", self.part)
    }
}

impl ZellijTile for State {
    fn load(&mut self) {
        set_selectable(false);
        set_invisible_borders(true);
        set_max_height(2);
        subscribe(&[EventType::ModeUpdate]);
    }

    fn update(&mut self, event: Event) {
        if let Event::ModeUpdate(mode_info) = event {
            self.mode_info = mode_info;
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let superkey = superkey(self.mode_info.palette);
        let ctrl_keys = ctrl_keys(&self.mode_info, cols - superkey.len);

        let first_line = format!("{}{}", superkey, ctrl_keys);
        let second_line = keybinds(&self.mode_info, cols);

        let first_line_color = match self.mode_info.palette.theme {
            Theme::Light => self.mode_info.palette.black,
            Theme::Dark => self.mode_info.palette.white,
        };
        let second_line_color = match self.mode_info.palette.theme {
            Theme::Light => self.mode_info.palette.bg,
            Theme::Dark => self.mode_info.palette.bg,
        };

        // [48;5;238m is gray background, [0K is so that it fills the rest of the line
        // [48;5;16m is black background, [0K is so that it fills the rest of the line
        println!(
            "{}\x1B[38;2;{};{};{}m\u{1b}[0K",
            first_line,
            first_line_color.0,
            first_line_color.1,
            first_line_color.2
        );
        println!(
            "{}\u{1b}[{};{};{}m\u{1b}[0K",
            second_line,
            second_line_color.0,
            second_line_color.1,
            second_line_color.2
        );
    }
}
