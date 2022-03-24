mod first_line;
mod second_line;
mod tip;

use ansi_term::Style;

use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

use first_line::{ctrl_keys, superkey};
use second_line::{
    floating_panes_are_visible, fullscreen_panes_to_hide, keybinds,
    locked_floating_panes_are_visible, locked_fullscreen_panes_to_hide, system_clipboard_error,
    text_copied_hint,
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
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
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
    let background = match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    };
    let foreground = match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    };
    match palette.source {
        PaletteSource::Default => ColoredElements {
            selected_prefix_separator: style!(background, palette.green),
            selected_char_left_separator: style!(background, palette.green).bold(),
            selected_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_char_right_separator: style!(background, palette.green).bold(),
            selected_styled_text: style!(background, palette.green).bold(),
            selected_suffix_separator: style!(palette.green, background).bold(),
            unselected_prefix_separator: style!(background, palette.fg),
            unselected_char_left_separator: style!(background, palette.fg).bold(),
            unselected_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_char_right_separator: style!(background, palette.fg).bold(),
            unselected_styled_text: style!(background, palette.fg).bold(),
            unselected_suffix_separator: style!(palette.fg, background),
            disabled_prefix_separator: style!(background, palette.fg),
            disabled_styled_text: style!(background, palette.fg).dimmed(),
            disabled_suffix_separator: style!(palette.fg, background),
            selected_single_letter_prefix_separator: style!(background, palette.green),
            selected_single_letter_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_single_letter_suffix_separator: style!(palette.green, background),
            unselected_single_letter_prefix_separator: style!(background, palette.fg),
            unselected_single_letter_char_shortcut: style!(palette.red, palette.fg).bold().dimmed(),
            unselected_single_letter_suffix_separator: style!(palette.fg, background),
            superkey_prefix: style!(foreground, background).bold(),
            superkey_suffix_separator: style!(background, background),
        },
        PaletteSource::Xresources => ColoredElements {
            selected_prefix_separator: style!(background, palette.green),
            selected_char_left_separator: style!(palette.fg, palette.green).bold(),
            selected_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_char_right_separator: style!(palette.fg, palette.green).bold(),
            selected_styled_text: style!(background, palette.green).bold(),
            selected_suffix_separator: style!(palette.green, background).bold(),
            unselected_prefix_separator: style!(background, palette.fg),
            unselected_char_left_separator: style!(background, palette.fg).bold(),
            unselected_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_char_right_separator: style!(background, palette.fg).bold(),
            unselected_styled_text: style!(background, palette.fg).bold(),
            unselected_suffix_separator: style!(palette.fg, background),
            disabled_prefix_separator: style!(background, palette.fg),
            disabled_styled_text: style!(background, palette.fg).dimmed(),
            disabled_suffix_separator: style!(palette.fg, background),
            selected_single_letter_prefix_separator: style!(palette.fg, palette.green),
            selected_single_letter_char_shortcut: style!(palette.red, palette.green).bold(),
            selected_single_letter_suffix_separator: style!(palette.green, palette.fg),
            unselected_single_letter_prefix_separator: style!(palette.fg, background),
            unselected_single_letter_char_shortcut: style!(palette.red, palette.fg).bold(),
            unselected_single_letter_suffix_separator: style!(palette.fg, background),
            superkey_prefix: style!(background, palette.fg).bold(),
            superkey_suffix_separator: style!(palette.fg, background),
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
            EventType::SystemClipboardFailure,
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
            Event::CopyToClipboard(copy_destination) => {
                self.text_copy_destination = Some(copy_destination);
            }
            Event::SystemClipboardFailure => {
                self.display_system_clipboard_failure = true;
            }
            Event::InputReceived => {
                self.text_copy_destination = None;
                self.display_system_clipboard_failure = false;
            }
            _ => {}
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let separator = if !self.mode_info.capabilities.arrow_fonts {
            ARROW_SEPARATOR
        } else {
            ""
        };

        let colored_elements = color_elements(self.mode_info.style.colors);
        let superkey = superkey(colored_elements, separator);
        let ctrl_keys = ctrl_keys(
            &self.mode_info,
            cols.saturating_sub(superkey.len),
            separator,
        );

        let first_line = format!("{}{}", superkey, ctrl_keys);
        let second_line = self.second_line(cols);

        let background = match self.mode_info.palette.theme_hue {
            ThemeHue::Dark => self.mode_info.palette.black,
            ThemeHue::Light => self.mode_info.palette.white,
        };

        // [48;5;238m is white background, [0K is so that it fills the rest of the line
        // [m is background reset, [0K is so that it clears the rest of the line
        match background {
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

impl State {
    fn second_line(&self, cols: usize) -> LinePart {
        let active_tab = self.tabs.iter().find(|t| t.active);

        if let Some(copy_destination) = self.text_copy_destination {
            text_copied_hint(&self.mode_info.style.colors, copy_destination)
        } else if self.display_system_clipboard_failure {
            system_clipboard_error(&self.mode_info.style.colors)
        } else if let Some(active_tab) = active_tab {
            if active_tab.is_fullscreen_active {
                match self.mode_info.mode {
                    InputMode::Normal => fullscreen_panes_to_hide(
                        &self.mode_info.style.colors,
                        active_tab.panes_to_hide,
                    ),
                    InputMode::Locked => locked_fullscreen_panes_to_hide(
                        &self.mode_info.style.colors,
                        active_tab.panes_to_hide,
                    ),
                    _ => keybinds(&self.mode_info, &self.tip_name, cols),
                }
            } else if active_tab.are_floating_panes_visible {
                match self.mode_info.mode {
                    InputMode::Normal => floating_panes_are_visible(&self.mode_info.style.colors),
                    InputMode::Locked => {
                        locked_floating_panes_are_visible(&self.mode_info.style.colors)
                    }
                    _ => keybinds(&self.mode_info, &self.tip_name, cols),
                }
            } else {
                keybinds(&self.mode_info, &self.tip_name, cols)
            }
        } else {
            LinePart::default()
        }
    }
}
