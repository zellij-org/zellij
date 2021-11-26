mod first_line;
mod second_line;
mod tip;

use ansi_term::Style;

use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

use first_line::{ctrl_keys, superkey};
use second_line::{
    fullscreen_panes_to_hide, keybinds, locked_fullscreen_panes_to_hide, text_copied_hint,
};
use tip::utils::get_cached_tip_name;

// for more of these, copy paste from: https://en.wikipedia.org/wiki/Box-drawing_character
static ARROW_SEPARATOR: &str = "";
static MORE_MSG: &str = " ... ";

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    tip_name: String,
    mode_info: ModeInfo,
    diplay_text_copied_hint: bool,
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
    // selected mode
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

// I really hate this, but I can't come up with a good solution for this,
// we need different colors from palette for the default theme
// plus here we can add new sources in the future, like Theme
// that can be defined in the config perhaps
fn color_elements(palette: Palette) -> ColoredElements {
    match palette.source {
        PaletteSource::Default => ColoredElements {
            selected_prefix_separator: style!(palette.gray, palette.green),
            selected_char_left_separator: style!(palette.black, palette.green).bold(),
            selected_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_char_right_separator: style!(palette.black, palette.green).bold(),
            selected_styled_text: style!(palette.black, palette.green).bold(),
            selected_suffix_separator: style!(palette.green, palette.gray).bold(),
            unselected_prefix_separator: style!(palette.gray, palette.fg),
            unselected_char_left_separator: style!(palette.black, palette.fg).bold(),
            unselected_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_char_right_separator: style!(palette.black, palette.fg).bold(),
            unselected_styled_text: style!(palette.black, palette.fg).bold(),
            unselected_suffix_separator: style!(palette.fg, palette.gray),
            disabled_prefix_separator: style!(palette.gray, palette.fg),
            disabled_styled_text: style!(palette.gray, palette.fg).dimmed(),
            disabled_suffix_separator: style!(palette.fg, palette.gray),
            selected_single_letter_prefix_separator: style!(palette.gray, palette.green),
            selected_single_letter_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_single_letter_suffix_separator: style!(palette.green, palette.gray),
            unselected_single_letter_prefix_separator: style!(palette.gray, palette.fg),
            unselected_single_letter_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_single_letter_suffix_separator: style!(palette.fg, palette.gray),
            superkey_prefix: style!(palette.white, palette.gray).bold(),
            superkey_suffix_separator: style!(palette.gray, palette.gray),
        },
        PaletteSource::Xresources => ColoredElements {
            selected_prefix_separator: style!(palette.gray, palette.green),
            selected_char_left_separator: style!(palette.fg, palette.green).bold(),
            selected_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_char_right_separator: style!(palette.fg, palette.green).bold(),
            selected_styled_text: style!(palette.gray, palette.green).bold(),
            selected_suffix_separator: style!(palette.green, palette.gray).bold(),
            unselected_prefix_separator: style!(palette.gray, palette.fg),
            unselected_char_left_separator: style!(palette.gray, palette.fg).bold(),
            unselected_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_char_right_separator: style!(palette.gray, palette.fg).bold(),
            unselected_styled_text: style!(palette.gray, palette.fg).bold(),
            unselected_suffix_separator: style!(palette.fg, palette.gray),
            disabled_prefix_separator: style!(palette.gray, palette.fg),
            disabled_styled_text: style!(palette.gray, palette.fg).dimmed(),
            disabled_suffix_separator: style!(palette.fg, palette.gray),
            selected_single_letter_prefix_separator: style!(palette.fg, palette.green),
            selected_single_letter_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_single_letter_suffix_separator: style!(palette.green, palette.fg),
            unselected_single_letter_prefix_separator: style!(palette.fg, palette.gray),
            unselected_single_letter_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_single_letter_suffix_separator: style!(palette.fg, palette.gray),
            superkey_prefix: style!(palette.gray, palette.fg).bold(),
            superkey_suffix_separator: style!(palette.fg, palette.gray),
        },
    }
}

impl ZellijPlugin for State {
    fn load(&mut self) {
        // TODO: Should be able to choose whether to use the cache through config.
        self.tip_name = get_cached_tip_name();
        set_selectable(false);
        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::CopyToClipboard,
            EventType::InputReceived,
        ]);
    }

    fn update(&mut self, event: Event) {
        match event {
            Event::ModeUpdate(mode_info) => {
                self.mode_info = mode_info;
            }
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
            }
            Event::CopyToClipboard => {
                self.diplay_text_copied_hint = true;
            }
            Event::InputReceived => {
                self.diplay_text_copied_hint = false;
            }
            _ => {}
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let separator = if !self.mode_info.capabilities.arrow_fonts {
            ARROW_SEPARATOR
        } else {
            &""
        };

        let colored_elements = color_elements(self.mode_info.palette);
        let superkey = superkey(colored_elements, separator);
        let ctrl_keys = ctrl_keys(
            &self.mode_info,
            cols.saturating_sub(superkey.len),
            separator,
        );

        let first_line = format!("{}{}", superkey, ctrl_keys);

        let mut second_line = LinePart::default();
        for t in self.tabs.iter_mut() {
            if t.active {
                match self.mode_info.mode {
                    InputMode::Normal => {
                        if t.is_fullscreen_active {
                            second_line = if self.diplay_text_copied_hint {
                                text_copied_hint(&self.mode_info.palette)
                            } else {
                                fullscreen_panes_to_hide(&self.mode_info.palette, t.panes_to_hide)
                            }
                        } else {
                            second_line = if self.diplay_text_copied_hint {
                                text_copied_hint(&self.mode_info.palette)
                            } else {
                                keybinds(&self.mode_info, &self.tip_name, cols)
                            }
                        }
                    }
                    InputMode::Locked => {
                        if t.is_fullscreen_active {
                            second_line = if self.diplay_text_copied_hint {
                                text_copied_hint(&self.mode_info.palette)
                            } else {
                                locked_fullscreen_panes_to_hide(
                                    &self.mode_info.palette,
                                    t.panes_to_hide,
                                )
                            }
                        } else {
                            second_line = if self.diplay_text_copied_hint {
                                text_copied_hint(&self.mode_info.palette)
                            } else {
                                keybinds(&self.mode_info, &self.tip_name, cols)
                            }
                        }
                    }
                    _ => {
                        second_line = if self.diplay_text_copied_hint {
                            text_copied_hint(&self.mode_info.palette)
                        } else {
                            keybinds(&self.mode_info, &self.tip_name, cols)
                        }
                    }
                }
            }
        }

        // [48;5;238m is gray background, [0K is so that it fills the rest of the line
        // [m is background reset, [0K is so that it clears the rest of the line
        match self.mode_info.palette.gray {
            PaletteColor::Rgb((r, g, b)) => {
                println!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
            }
            PaletteColor::EightBit(color) => {
                println!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
            }
        }
        println!("\u{1b}[m{}\u{1b}[0K", second_line);
    }
}
