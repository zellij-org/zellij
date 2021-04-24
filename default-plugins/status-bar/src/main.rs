mod first_line;
mod second_line;

use ansi_term::{Color::RGB, Style};

use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::*;

use first_line::{ctrl_keys, superkey};
use second_line::keybinds;

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

macro_rules! rgb {
    ($a:expr) => {
        RGB($a.0, $a.1, $a.2)
    };
}
macro_rules! style {
    ($a:expr, $b:expr) => {
        Style::new()
            .fg(RGB($a.0, $a.1, $a.2))
            .on(RGB($b.0, $b.1, $b.2))
    };
}

// I really hate this, but I can't come up with a good solution for this,
// we need different colors from palette for the default theme
// plus here we can add new sources in the future, like Theme
// that can be defined in the config perhaps
fn color_elements(palette: Palette) -> ColoredElements {
    match palette.source {
        PaletteSource::Default => ColoredElements::new(
            style!(palette.bg, palette.green),
            style!(palette.black, palette.green).bold(),
            style!(palette.red, palette.green).bold(),
            style!(palette.black, palette.green).bold(),
            style!(palette.black, palette.green).bold(),
            style!(palette.green, palette.bg).bold(),
            style!(palette.bg, palette.fg),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.red, palette.fg).bold(),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.black, palette.fg).bold(),
            style!(palette.fg, palette.bg),
            style!(palette.fg, palette.bg),
            style!(palette.bg, palette.fg).dimmed(),
            style!(palette.fg, palette.bg),
            style!(palette.fg, palette.green),
            style!(palette.red, palette.green).bold(),
            style!(palette.green, palette.fg),
            style!(palette.fg, palette.bg),
            style!(palette.red, palette.fg).bold(),
            style!(palette.fg, palette.bg),
            style!(palette.white, palette.bg).bold(),
            Style::new().fg(rgb!(palette.bg)).on(rgb!(palette.bg)),
        ),
        PaletteSource::Xresources => ColoredElements::new(
            style!(palette.bg, palette.green),
            style!(palette.fg, palette.green).bold(),
            style!(palette.red, palette.green).bold(),
            style!(palette.fg, palette.green).bold(),
            style!(palette.bg, palette.green).bold(),
            style!(palette.green, palette.bg).bold(),
            style!(palette.bg, palette.fg),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.red, palette.fg).bold(),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.fg, palette.bg),
            style!(palette.fg, palette.bg),
            style!(palette.bg, palette.fg).dimmed(),
            style!(palette.fg, palette.bg),
            style!(palette.fg, palette.green),
            style!(palette.red, palette.green).bold(),
            style!(palette.green, palette.fg),
            style!(palette.fg, palette.bg),
            style!(palette.red, palette.fg).bold(),
            style!(palette.fg, palette.bg),
            style!(palette.bg, palette.fg).bold(),
            style!(palette.fg, palette.bg),
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
