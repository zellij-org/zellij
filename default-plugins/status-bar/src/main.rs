mod first_line;
mod second_line;
mod tip;

use ansi_term::Style;

use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

use first_line::ctrl_keys;
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
    pub selected: SegmentStyle,
    pub unselected: SegmentStyle,
    pub unselected_alternate: SegmentStyle,
    pub disabled: SegmentStyle,
    // superkey
    pub superkey_prefix: Style,
    pub superkey_suffix_separator: Style,
}

#[derive(Clone, Copy)]
pub struct SegmentStyle {
    pub prefix_separator: Style,
    pub char_left_separator: Style,
    pub char_shortcut: Style,
    pub char_right_separator: Style,
    pub styled_text: Style,
    pub suffix_separator: Style,
}

// I really hate this, but I can't come up with a good solution for this,
// we need different colors from palette for the default theme
// plus here we can add new sources in the future, like Theme
// that can be defined in the config perhaps
fn color_elements(palette: Palette, different_color_alternates: bool) -> ColoredElements {
    let background = match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    };
    let foreground = match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    };
    let alternate_background_color = if different_color_alternates {
        match palette.theme_hue {
            ThemeHue::Dark => palette.white,
            ThemeHue::Light => palette.black,
        }
    } else {
        palette.fg
    };
    match palette.source {
        PaletteSource::Default => ColoredElements {
            selected: SegmentStyle {
                prefix_separator: style!(background, palette.green),
                char_left_separator: style!(background, palette.green).bold(),
                char_shortcut: style!(palette.red, palette.green).bold(),
                char_right_separator: style!(background, palette.green).bold(),
                styled_text: style!(background, palette.green).bold(),
                suffix_separator: style!(palette.green, background).bold(),
            },
            unselected: SegmentStyle {
                prefix_separator: style!(background, palette.fg),
                char_left_separator: style!(background, palette.fg).bold(),
                char_shortcut: style!(palette.red, palette.fg).bold(),
                char_right_separator: style!(background, palette.fg).bold(),
                styled_text: style!(background, palette.fg).bold(),
                suffix_separator: style!(palette.fg, background),
            },
            unselected_alternate: SegmentStyle {
                prefix_separator: style!(background, alternate_background_color),
                char_left_separator: style!(background, alternate_background_color).bold(),
                char_shortcut: style!(palette.red, alternate_background_color).bold(),
                char_right_separator: style!(background, alternate_background_color).bold(),
                styled_text: style!(background, alternate_background_color).bold(),
                suffix_separator: style!(alternate_background_color, background),
            },
            disabled: SegmentStyle {
                prefix_separator: style!(background, palette.fg),
                char_left_separator: style!(background, palette.fg).dimmed().italic(),
                char_shortcut: style!(background, palette.fg).dimmed().italic(),
                char_right_separator: style!(background, palette.fg).dimmed().italic(),
                styled_text: style!(background, palette.fg).dimmed().italic(),
                suffix_separator: style!(palette.fg, background),
            },
            superkey_prefix: style!(foreground, background).bold(),
            superkey_suffix_separator: style!(background, background),
        },
        PaletteSource::Xresources => ColoredElements {
            selected: SegmentStyle {
                prefix_separator: style!(background, palette.green),
                char_left_separator: style!(palette.fg, palette.green).bold(),
                char_shortcut: style!(palette.red, palette.green).bold(),
                char_right_separator: style!(palette.fg, palette.green).bold(),
                styled_text: style!(background, palette.green).bold(),
                suffix_separator: style!(palette.green, background).bold(),
            },
            unselected: SegmentStyle {
                prefix_separator: style!(background, palette.fg),
                char_left_separator: style!(background, palette.fg).bold(),
                char_shortcut: style!(palette.red, palette.fg).bold(),
                char_right_separator: style!(background, palette.fg).bold(),
                styled_text: style!(background, palette.fg).bold(),
                suffix_separator: style!(palette.fg, background),
            },
            unselected_alternate: SegmentStyle {
                prefix_separator: style!(background, alternate_background_color),
                char_left_separator: style!(background, alternate_background_color).bold(),
                char_shortcut: style!(palette.red, alternate_background_color).bold(),
                char_right_separator: style!(background, alternate_background_color).bold(),
                styled_text: style!(background, alternate_background_color).bold(),
                suffix_separator: style!(alternate_background_color, background),
            },
            disabled: SegmentStyle {
                prefix_separator: style!(background, palette.fg),
                char_left_separator: style!(background, palette.fg).dimmed(),
                char_shortcut: style!(background, palette.fg).dimmed(),
                char_right_separator: style!(background, palette.fg).dimmed(),
                styled_text: style!(background, palette.fg).dimmed(),
                suffix_separator: style!(palette.fg, background),
            },
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
            },
            Event::TabUpdate(tabs) => {
                self.tabs = tabs;
            },
            Event::CopyToClipboard(copy_destination) => {
                self.text_copy_destination = Some(copy_destination);
            },
            Event::SystemClipboardFailure => {
                self.display_system_clipboard_failure = true;
            },
            Event::InputReceived => {
                self.text_copy_destination = None;
                self.display_system_clipboard_failure = false;
            },
            _ => {},
        }
    }

    fn render(&mut self, _rows: usize, cols: usize) {
        let supports_arrow_fonts = !self.mode_info.capabilities.arrow_fonts;
        let separator = if supports_arrow_fonts {
            ARROW_SEPARATOR
        } else {
            ""
        };

        let first_line = ctrl_keys(&self.mode_info, cols, separator);

        let second_line = self.second_line(cols);

        let background = match self.mode_info.style.colors.theme_hue {
            ThemeHue::Dark => self.mode_info.style.colors.black,
            ThemeHue::Light => self.mode_info.style.colors.white,
        };

        // [48;5;238m is white background, [0K is so that it fills the rest of the line
        // [m is background reset, [0K is so that it clears the rest of the line
        match background {
            PaletteColor::Rgb((r, g, b)) => {
                println!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
            },
            PaletteColor::EightBit(color) => {
                println!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
            },
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
                    InputMode::Normal => floating_panes_are_visible(&self.mode_info),
                    InputMode::Locked => {
                        locked_floating_panes_are_visible(&self.mode_info.style.colors)
                    },
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

/// Get a common modifier key from a key vector.
///
/// Iterates over all keys, skipping keys mentioned in `to_ignore` and returns any found common
/// modifier key.
pub fn get_common_modifier(keyvec: Vec<&Key>) -> Option<String> {
    let mut modifier = "";
    let mut new_modifier;
    for key in keyvec.iter() {
        match key {
            Key::Ctrl(_) => new_modifier = "Ctrl",
            Key::Alt(_) => new_modifier = "Alt",
            _ => return None,
        }
        if modifier.is_empty() {
            modifier = new_modifier;
        } else if modifier != new_modifier {
            // Prefix changed!
            return None;
        }
    }
    match modifier.is_empty() {
        true => None,
        false => Some(modifier.to_string()),
    }
}

/// Get key from action pattern(s).
///
/// This macro takes as arguments a `keymap` that is a `Vec<(Key, Vec<Action>)>` and contains all
/// keybindings for the current mode and one or more `p` patterns which match a sequence of actions
/// to search for. If within the keymap a sequence of actions matching `p` is found, all keys that
/// trigger the action pattern are returned as vector of `Vec<Key>`.
// TODO: Accept multiple sequences of patterns, possible separated by '|', and bin them together
// into one group under 'text'.
#[macro_export]
macro_rules! action_key {
    ($keymap:ident, $( $p:pat ),+) => {
        //let mut ret: Vec<Key>;
        $keymap.iter().
            filter_map(|(key, acvec)| {
                match matches!(acvec.as_slice(), &[$($p),+]) {
                    true => Some(*key),
                    false => None
                }
            })
            .collect::<Vec<Key>>()
    };
}

/// Helper macro to represent common pattern.
///
/// Expands verbosely to `Action::SwitchToMode(InputMode::Normal)`, which is an action that often
/// repeats in the keybindings configuration. We need it to expand to verbose rust code (i.e. a
/// "Textual replacement", similar to C `#define`) so it gets picked up as proper pattern in the
/// `action_key!` macro.
#[macro_export]
macro_rules! to_normal {
    () => {
        Action::SwitchToMode(InputMode::Normal)
    };
}
