mod first_line;
mod second_line;

use ansi_term::{Color::RGB, Style};

use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::*;

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

register_plugin!(State);

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

#[derive(Clone, Copy)]
pub struct ColoredElements {
    // slected mode
    pub selected_prefix_separator: Style,
    pub selected_char_left_separator: Style,
    pub selected_char_shortcut: Style,
    pub selected_char_right_separator: Style,
    pub selected_styled_text: Style,
    pub selected_suffix_separator: Style,
    // unselected mode
    pub unselected_prefix_separator: Style,
    pub unselected_char_left_separator: Style,
    pub unselected_char_shortcut: Style,
    pub unselected_char_right_separator: Style,
    pub unselected_styled_text: Style,
    pub unselected_suffix_separator: Style,
    // disabled mode
    pub disabled_prefix_separator: Style,
    pub disabled_styled_text: Style,
    pub disabled_suffix_separator: Style,
    // selected single letter
    pub selected_single_letter_prefix_separator: Style,
    pub selected_single_letter_char_shortcut: Style,
    pub selected_single_letter_suffix_separator: Style,
    // unselected single letter
    pub unselected_single_letter_prefix_separator: Style,
    pub unselected_single_letter_char_shortcut: Style,
    pub unselected_single_letter_suffix_separator: Style,
    // superkey
    pub superkey_prefix: Style,
    pub superkey_suffix_separator: Style,
}

impl ColoredElements {
    pub fn new(
        selected_prefix_separator: Style,
        selected_char_left_separator: Style,
        selected_char_shortcut: Style,
        selected_char_right_separator: Style,
        selected_styled_text: Style,
        selected_suffix_separator: Style,
        unselected_prefix_separator: Style,
        unselected_char_left_separator: Style,
        unselected_char_shortcut: Style,
        unselected_char_right_separator: Style,
        unselected_styled_text: Style,
        unselected_suffix_separator: Style,
        disabled_prefix_separator: Style,
        disabled_styled_text: Style,
        disabled_suffix_separator: Style,
        selected_single_letter_prefix_separator: Style,
        selected_single_letter_char_shortcut: Style,
        selected_single_letter_suffix_separator: Style,
        unselected_single_letter_prefix_separator: Style,
        unselected_single_letter_char_shortcut: Style,
        unselected_single_letter_suffix_separator: Style,
        superkey_prefix: Style,
        superkey_suffix_separator: Style,
    ) -> Self {
        Self {
            selected_prefix_separator,
            selected_char_left_separator,
            selected_char_shortcut,
            selected_char_right_separator,
            selected_styled_text,
            selected_suffix_separator,
            unselected_prefix_separator,
            unselected_char_left_separator,
            unselected_char_shortcut,
            unselected_char_right_separator,
            unselected_styled_text,
            unselected_suffix_separator,
            disabled_prefix_separator,
            disabled_styled_text,
            disabled_suffix_separator,
            selected_single_letter_prefix_separator,
            selected_single_letter_char_shortcut,
            selected_single_letter_suffix_separator,
            unselected_single_letter_prefix_separator,
            unselected_single_letter_char_shortcut,
            unselected_single_letter_suffix_separator,
            superkey_prefix,
            superkey_suffix_separator,
        }
    }
}

fn color_elements(palette: Palette) -> ColoredElements {
    match palette.source {
        PaletteSource::Default => ColoredElements::new(
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2)),
            Style::new()
                .fg(RGB(palette.black.0, palette.black.1, palette.black.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.black.0, palette.black.1, palette.black.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.black.0, palette.black.1, palette.black.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.black.0, palette.black.1, palette.black.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .dimmed(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.white.0, palette.white.1, palette.white.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
        ),
        PaletteSource::Xresources => ColoredElements::new(
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2)),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .bold()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .dimmed(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.green.0, palette.green.1, palette.green.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2)),
            Style::new()
                .bold()
                .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
            Style::new()
                .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
                .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .bold(),
            Style::new()
                .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
                .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2)),
        ),
    }
}

impl ZellijPlugin for State {
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
        let colored_elements = color_elements(self.mode_info.palette);
        let superkey = superkey(colored_elements);
        let ctrl_keys = ctrl_keys(&self.mode_info, cols - superkey.len);

        let first_line = format!("{}{}", superkey, ctrl_keys);
        let second_line = keybinds(&self.mode_info, cols);

        // [48;5;238m is gray background, [0K is so that it fills the rest of the line
        // [m is background reset, [0K is so that it clears the rest of the line
        println!(
            "{}\u{1b}[48;2;{};{};{}m\u{1b}[0K",
            first_line,
            self.mode_info.palette.bg.0,
            self.mode_info.palette.bg.1,
            self.mode_info.palette.bg.2
        );
        println!("\u{1b}[m{}\u{1b}[0K", second_line);
    }
}
