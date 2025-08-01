mod first_line;
mod one_line_ui;
mod second_line;
mod tip;

use ansi_term::{
    ANSIString,
    Colour::{Fixed, RGB},
    Style,
};

use std::collections::BTreeMap;
use std::fmt::{Display, Error, Formatter};
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::{palette_match, style};

use first_line::first_line;
use one_line_ui::one_line_ui;
use second_line::{
    floating_panes_are_visible, fullscreen_panes_to_hide, keybinds,
    locked_floating_panes_are_visible, locked_fullscreen_panes_to_hide, system_clipboard_error,
    text_copied_hint,
};
use tip::utils::get_cached_tip_name;

// for more of these, copy paste from: https://en.wikipedia.org/wiki/Box-drawing_character
static ARROW_SEPARATOR: &str = "";
static MORE_MSG: &str = " ... ";
/// Shorthand for `Action::SwitchToMode(InputMode::Normal)`.
const TO_NORMAL: Action = Action::SwitchToMode(InputMode::Normal);

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    tip_name: String,
    mode_info: ModeInfo,
    text_copy_destination: Option<CopyDestination>,
    display_system_clipboard_failure: bool,
    classic_ui: bool,
    base_mode_is_locked: bool,
}

register_plugin!(State);

#[derive(Default)]
pub struct LinePart {
    part: String,
    len: usize,
}

impl LinePart {
    pub fn append(&mut self, to_append: &LinePart) {
        self.part.push_str(&to_append.part);
        self.len += to_append.len;
    }
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
fn color_elements(palette: Styling, different_color_alternates: bool) -> ColoredElements {
    let background = palette.text_unselected.background;
    let foreground = palette.text_unselected.base;
    let alternate_background_color = if different_color_alternates {
        palette.ribbon_unselected.base
    } else {
        palette.ribbon_unselected.background
    };
    ColoredElements {
        selected: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_selected.background),
            char_left_separator: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            char_shortcut: style!(
                palette.ribbon_selected.emphasis_0,
                palette.ribbon_selected.background
            )
            .bold(),
            char_right_separator: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            styled_text: style!(
                palette.ribbon_selected.base,
                palette.ribbon_selected.background
            )
            .bold(),
            suffix_separator: style!(palette.ribbon_selected.background, background).bold(),
        },
        unselected: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_unselected.background),
            char_left_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            char_shortcut: style!(
                palette.ribbon_unselected.emphasis_0,
                palette.ribbon_unselected.background
            )
            .bold(),
            char_right_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            styled_text: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .bold(),
            suffix_separator: style!(palette.ribbon_unselected.background, background).bold(),
        },
        unselected_alternate: SegmentStyle {
            prefix_separator: style!(background, alternate_background_color),
            char_left_separator: style!(background, alternate_background_color).bold(),
            char_shortcut: style!(
                palette.ribbon_unselected.emphasis_0,
                alternate_background_color
            )
            .bold(),
            char_right_separator: style!(background, alternate_background_color).bold(),
            styled_text: style!(palette.ribbon_unselected.base, alternate_background_color).bold(),
            suffix_separator: style!(alternate_background_color, background).bold(),
        },
        disabled: SegmentStyle {
            prefix_separator: style!(background, palette.ribbon_unselected.background),
            char_left_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            char_shortcut: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            char_right_separator: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            styled_text: style!(
                palette.ribbon_unselected.base,
                palette.ribbon_unselected.background
            )
            .dimmed()
            .italic(),
            suffix_separator: style!(palette.ribbon_unselected.background, background),
        },
        superkey_prefix: style!(foreground, background).bold(),
        superkey_suffix_separator: style!(background, background),
    }
}

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        // TODO: Should be able to choose whether to use the cache through config.
        self.tip_name = get_cached_tip_name();
        self.classic_ui = configuration
            .get("classic")
            .map(|c| c == "true")
            .unwrap_or(false);
        set_selectable(false);
        subscribe(&[
            EventType::ModeUpdate,
            EventType::TabUpdate,
            EventType::PaneUpdate,
            EventType::CopyToClipboard,
            EventType::InputReceived,
            EventType::SystemClipboardFailure,
        ]);
    }

    fn update(&mut self, event: Event) -> bool {
        let mut should_render = false;
        match event {
            Event::ModeUpdate(mode_info) => {
                if self.mode_info != mode_info {
                    should_render = true;
                }
                self.mode_info = mode_info;
                self.base_mode_is_locked = self.mode_info.base_mode == Some(InputMode::Locked);
            },
            Event::TabUpdate(tabs) => {
                if self.tabs != tabs {
                    should_render = true;
                }
                self.tabs = tabs;
            },
            Event::CopyToClipboard(copy_destination) => {
                match self.text_copy_destination {
                    Some(text_copy_destination) => {
                        if text_copy_destination != copy_destination {
                            should_render = true;
                        }
                    },
                    None => {
                        should_render = true;
                    },
                }
                self.text_copy_destination = Some(copy_destination);
            },
            Event::SystemClipboardFailure => {
                should_render = true;
                self.display_system_clipboard_failure = true;
            },
            Event::InputReceived => {
                if self.text_copy_destination.is_some()
                    || self.display_system_clipboard_failure == true
                {
                    should_render = true;
                }
                self.text_copy_destination = None;
                self.display_system_clipboard_failure = false;
            },
            _ => {},
        };
        should_render
    }

    fn render(&mut self, rows: usize, cols: usize) {
        let supports_arrow_fonts = !self.mode_info.capabilities.arrow_fonts;
        let separator = if supports_arrow_fonts {
            ARROW_SEPARATOR
        } else {
            ""
        };

        let background = self.mode_info.style.colors.text_unselected.background;

        if rows == 1 && !self.classic_ui {
            let fill_bg = match background {
                PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m\u{1b}[0K", r, g, b),
                PaletteColor::EightBit(color) => format!("\u{1b}[48;5;{}m\u{1b}[0K", color),
            };
            let active_tab = self.tabs.iter().find(|t| t.active);
            print!(
                "{}{}",
                one_line_ui(
                    &self.mode_info,
                    active_tab,
                    cols,
                    separator,
                    self.base_mode_is_locked,
                    self.text_copy_destination,
                    self.display_system_clipboard_failure,
                ),
                fill_bg,
            );
            return;
        }

        //TODO: Switch to UI components here
        let active_tab = self.tabs.iter().find(|t| t.active);
        let first_line = first_line(&self.mode_info, active_tab, cols, separator);
        let second_line = self.second_line(cols);

        // [48;5;238m is white background, [0K is so that it fills the rest of the line
        // [m is background reset, [0K is so that it clears the rest of the line
        match background {
            PaletteColor::Rgb((r, g, b)) => {
                if rows > 1 {
                    println!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
                } else {
                    if self.mode_info.mode == InputMode::Normal {
                        print!("{}\u{1b}[48;2;{};{};{}m\u{1b}[0K", first_line, r, g, b);
                    } else {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    }
                }
            },
            PaletteColor::EightBit(color) => {
                if rows > 1 {
                    println!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
                } else {
                    if self.mode_info.mode == InputMode::Normal {
                        print!("{}\u{1b}[48;5;{}m\u{1b}[0K", first_line, color);
                    } else {
                        print!("\u{1b}[m{}\u{1b}[0K", second_line);
                    }
                }
            },
        }

        if rows > 1 {
            print!("\u{1b}[m{}\u{1b}[0K", second_line);
        }
    }
}

impl State {
    fn second_line(&self, cols: usize) -> LinePart {
        let active_tab = self.tabs.iter().find(|t| t.active);

        if let Some(copy_destination) = self.text_copy_destination {
            text_copied_hint(copy_destination)
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

pub fn get_common_modifiers(mut keyvec: Vec<&KeyWithModifier>) -> Vec<KeyModifier> {
    if keyvec.is_empty() {
        return vec![];
    }
    let mut common_modifiers = keyvec.pop().unwrap().key_modifiers.clone();
    for key in keyvec {
        common_modifiers = common_modifiers
            .intersection(&key.key_modifiers)
            .cloned()
            .collect();
    }
    common_modifiers.into_iter().collect()
}

/// Get key from action pattern(s).
///
/// This function takes as arguments a `keymap` that is a `Vec<(Key, Vec<Action>)>` and contains
/// all keybindings for the current mode and one or more `p` patterns which match a sequence of
/// actions to search for. If within the keymap a sequence of actions matching `p` is found, all
/// keys that trigger the action pattern are returned as vector of `Vec<Key>`.
pub fn action_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &[Action],
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter_map(|(key, acvec)| {
            let matching = acvec
                .iter()
                .zip(action)
                .filter(|(a, b)| a.shallow_eq(b))
                .count();

            if matching == acvec.len() && matching == action.len() {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect::<Vec<KeyWithModifier>>()
}

/// Get multiple keys for multiple actions.
///
/// An extension of [`action_key`] that iterates over all action tuples and collects the results.
pub fn action_key_group(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    actions: &[&[Action]],
) -> Vec<KeyWithModifier> {
    let mut ret = vec![];
    for action in actions {
        ret.extend(action_key(keymap, action));
    }
    ret
}

/// Style a vector of [`Key`]s with the given [`Palette`].
///
/// Creates a line segment of style `<KEYS>`, with correct theming applied: The brackets have the
/// regular text color, the enclosed keys are painted green and bold. If the keys share a common
/// modifier (See [`get_common_modifier`]), it is printed in front of the keys, painted green and
/// bold, separated with a `+`: `MOD + <KEYS>`.
///
/// If multiple [`Key`]s are given, the individual keys are separated with a `|` char. This does
/// not apply to the following groups of keys which are treated specially and don't have a
/// separator between them:
///
/// - "hjkl"
/// - "HJKL"
/// - "←↓↑→"
/// - "←→"
/// - "↓↑"
///
/// The returned Vector of [`ANSIString`] is suitable for transformation into an [`ANSIStrings`]
/// type.
pub fn style_key_with_modifier(
    keyvec: &[KeyWithModifier],
    palette: &Styling,
    background: Option<PaletteColor>,
) -> Vec<ANSIString<'static>> {
    if keyvec.is_empty() {
        return vec![];
    }

    let text_color = palette_match!(palette.text_unselected.base);
    let green_color = palette_match!(palette.text_unselected.emphasis_2);
    let orange_color = palette_match!(palette.text_unselected.emphasis_0);
    let mut ret = vec![];

    let common_modifiers = get_common_modifiers(keyvec.iter().collect());

    let no_common_modifier = common_modifiers.is_empty();
    let modifier_str = common_modifiers
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("-");
    let painted_modifier = if modifier_str.is_empty() {
        Style::new().paint("")
    } else {
        if let Some(background) = background {
            let background = palette_match!(background);
            Style::new()
                .fg(orange_color)
                .on(background)
                .bold()
                .paint(modifier_str)
        } else {
            Style::new().fg(orange_color).bold().paint(modifier_str)
        }
    };
    ret.push(painted_modifier);

    // Prints key group start
    let group_start_str = if no_common_modifier { "<" } else { " + <" };
    if let Some(background) = background {
        let background = palette_match!(background);
        ret.push(
            Style::new()
                .fg(text_color)
                .on(background)
                .paint(group_start_str),
        );
    } else {
        ret.push(Style::new().fg(text_color).paint(group_start_str));
    }

    // Prints the keys
    let key = keyvec
        .iter()
        .map(|key| {
            if no_common_modifier {
                format!("{}", key)
            } else {
                let key_modifier_for_key = key
                    .key_modifiers
                    .iter()
                    .filter(|m| !common_modifiers.contains(m))
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                if key_modifier_for_key.is_empty() {
                    format!("{}", key.bare_key)
                } else {
                    format!("{} {}", key_modifier_for_key, key.bare_key)
                }
            }
        })
        .collect::<Vec<String>>();

    // Special handling of some pre-defined keygroups
    let key_string = key.join("");
    let key_separator = match &key_string[..] {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        _ => "|",
    };

    for (idx, key) in key.iter().enumerate() {
        if idx > 0 && !key_separator.is_empty() {
            if let Some(background) = background {
                let background = palette_match!(background);
                ret.push(
                    Style::new()
                        .fg(text_color)
                        .on(background)
                        .paint(key_separator),
                );
            } else {
                ret.push(Style::new().fg(text_color).paint(key_separator));
            }
        }
        if let Some(background) = background {
            let background = palette_match!(background);
            ret.push(
                Style::new()
                    .fg(green_color)
                    .on(background)
                    .bold()
                    .paint(key.clone()),
            );
        } else {
            ret.push(Style::new().fg(green_color).bold().paint(key.clone()));
        }
    }

    let group_end_str = ">";
    if let Some(background) = background {
        let background = palette_match!(background);
        ret.push(
            Style::new()
                .fg(text_color)
                .on(background)
                .paint(group_end_str),
        );
    } else {
        ret.push(Style::new().fg(text_color).paint(group_end_str));
    }

    ret
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use ansi_term::unstyle;
    use ansi_term::ANSIStrings;

    fn big_keymap() -> Vec<(KeyWithModifier, Vec<Action>)> {
        vec![
            (KeyWithModifier::new(BareKey::Char('a')), vec![Action::Quit]),
            (
                KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
                vec![Action::ScrollUp],
            ),
            (
                KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
                vec![Action::ScrollDown],
            ),
            (
                KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier(),
                vec![Action::ScrollDown, Action::SwitchToMode(InputMode::Normal)],
            ),
            (
                KeyWithModifier::new(BareKey::Char('1')),
                vec![TO_NORMAL, Action::SwitchToMode(InputMode::Locked)],
            ),
        ]
    }

    #[test]
    fn common_modifier_with_ctrl_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![KeyModifier::Ctrl]);
    }

    #[test]
    fn common_modifier_with_alt_keys_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![KeyModifier::Alt]);
    }

    #[test]
    fn common_modifier_with_mixed_alt_ctrl_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![]); // no common modifiers
    }

    #[test]
    fn common_modifier_with_any_keys() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('1')),
            KeyWithModifier::new(BareKey::Char('t')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('z')).with_alt_modifier(),
        ];
        let ret = get_common_modifiers(keyvec.iter().collect());
        assert_eq!(ret, vec![]); // no common modifiers
    }

    #[test]
    fn action_key_simple_pattern_match_exact() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[Action::Quit]);
        assert_eq!(ret, vec![KeyWithModifier::new(BareKey::Char('f'))]);
    }

    #[test]
    fn action_key_simple_pattern_match_pattern_too_long() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[Action::Quit, Action::ScrollUp]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_simple_pattern_match_pattern_empty() {
        let keymap = &[(KeyWithModifier::new(BareKey::Char('f')), vec![Action::Quit])];
        let ret = action_key(keymap, &[]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_long_pattern_match_exact() {
        let keymap = big_keymap();
        let ret = action_key(&keymap, &[Action::ScrollDown, TO_NORMAL]);
        assert_eq!(
            ret,
            vec![KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier()]
        );
    }

    #[test]
    fn action_key_long_pattern_match_too_short() {
        let keymap = big_keymap();
        let ret = action_key(&keymap, &[TO_NORMAL]);
        assert_eq!(ret, Vec::new());
    }

    #[test]
    fn action_key_group_single_pattern() {
        let keymap = big_keymap();
        let ret = action_key_group(&keymap, &[&[Action::Quit]]);
        assert_eq!(ret, vec![KeyWithModifier::new(BareKey::Char('a'))]);
    }

    #[test]
    fn action_key_group_two_patterns() {
        let keymap = big_keymap();
        let ret = action_key_group(&keymap, &[&[Action::ScrollDown], &[Action::ScrollUp]]);
        // Mind the order!
        assert_eq!(
            ret,
            vec![
                KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
                KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier()
            ]
        );
    }

    #[test]
    fn style_key_with_modifier_only_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')),
            KeyWithModifier::new(BareKey::Char('b')),
            KeyWithModifier::new(BareKey::Char('c')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<a|b|c>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_hjkl() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('h')),
            KeyWithModifier::new(BareKey::Char('j')),
            KeyWithModifier::new(BareKey::Char('k')),
            KeyWithModifier::new(BareKey::Char('l')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<hjkl>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_all_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left),
            KeyWithModifier::new(BareKey::Down),
            KeyWithModifier::new(BareKey::Up),
            KeyWithModifier::new(BareKey::Right),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<←↓↑→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_left_right_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left),
            KeyWithModifier::new(BareKey::Right),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<←→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_special_group_down_up_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Down),
            KeyWithModifier::new(BareKey::Up),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<↓↑>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_ctrl_modifier_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('d')).with_ctrl_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Ctrl + <a|b|c|d>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_alt_modifier_chars() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('c')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('d')).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <a|b|c|d>".to_string())
    }

    #[test]
    fn style_key_with_modifier_common_alt_modifier_with_special_group_all_arrows() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Left).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Down).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Up).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Right).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <←↓↑→>".to_string())
    }

    #[test]
    fn style_key_with_modifier_ctrl_alt_char_mixed() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Char('a')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char('b')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char('c')),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "<Alt a|Ctrl b|c>".to_string())
    }

    #[test]
    fn style_key_with_modifier_unprintables() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Backspace),
            KeyWithModifier::new(BareKey::Enter),
            KeyWithModifier::new(BareKey::Char(' ')),
            KeyWithModifier::new(BareKey::Tab),
            KeyWithModifier::new(BareKey::PageDown),
            KeyWithModifier::new(BareKey::Delete),
            KeyWithModifier::new(BareKey::Home),
            KeyWithModifier::new(BareKey::End),
            KeyWithModifier::new(BareKey::Insert),
            KeyWithModifier::new(BareKey::Tab),
            KeyWithModifier::new(BareKey::Esc),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(
            ret,
            "<BACKSPACE|ENTER|SPACE|TAB|PgDn|DEL|HOME|END|INS|TAB|ESC>".to_string()
        )
    }

    #[test]
    fn style_key_with_modifier_unprintables_with_common_ctrl_modifier() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Enter).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Char(' ')).with_ctrl_modifier(),
            KeyWithModifier::new(BareKey::Tab).with_ctrl_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Ctrl + <ENTER|SPACE|TAB>".to_string())
    }

    #[test]
    fn style_key_with_modifier_unprintables_with_common_alt_modifier() {
        let keyvec = vec![
            KeyWithModifier::new(BareKey::Enter).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Char(' ')).with_alt_modifier(),
            KeyWithModifier::new(BareKey::Tab).with_alt_modifier(),
        ];
        let palette = Styling::default();

        let ret = style_key_with_modifier(&keyvec, &palette, None);
        let ret = unstyle(&ANSIStrings(&ret));

        assert_eq!(ret, "Alt + <ENTER|SPACE|TAB>".to_string())
    }
}
