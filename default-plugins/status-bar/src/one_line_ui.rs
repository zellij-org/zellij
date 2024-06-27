use ansi_term::{ANSIString, ANSIStrings};
use ansi_term::{Style, Color::{Fixed, RGB}};
use zellij_tile_utils::palette_match;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;
use std::collections::{HashMap, BTreeSet};

use crate::{color_elements, MORE_MSG};
use crate::{
    action_key, action_key_group, get_common_modifiers, style_key_with_modifier, TO_NORMAL,
    // second_line::{keybinds, add_shortcut, add_shortcut_selected, add_shortcut_with_inline_key, add_keygroup_separator},
};
use crate::{ColoredElements, LinePart};
use crate::tip::{data::TIPS, TipFn};

#[derive(Debug)]
struct KeyShortcut {
    mode: KeyMode,
    action: KeyAction,
    key: Option<KeyWithModifier>,
}

#[derive(PartialEq, Debug, Clone, Copy)]
enum KeyAction {
    Normal,
    Lock,
    Unlock,
    Pane,
    Tab,
    Resize,
    Search,
    Quit,
    Session,
    Move,
    Tmux,
}

impl From<InputMode> for KeyAction {
    fn from(input_mode: InputMode) -> Self {
        match input_mode {
            InputMode::Normal => KeyAction::Normal,
            InputMode::Locked => KeyAction::Lock,
            InputMode::Pane => KeyAction::Pane,
            InputMode::Tab => KeyAction::Tab,
            InputMode::Resize => KeyAction::Resize,
            InputMode::Search => KeyAction::Search,
            InputMode::Session => KeyAction::Session,
            InputMode::Move => KeyAction::Move,
            InputMode::Tmux => KeyAction::Tmux,
            _ => KeyAction::Normal, // TODO: NO!!
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum KeyMode {
    Unselected,
    UnselectedAlternate,
    Selected,
    Disabled,
}

impl KeyShortcut {
    pub fn new(mode: KeyMode, action: KeyAction, key: Option<KeyWithModifier>) -> Self {
        KeyShortcut { mode, action, key }
    }

    pub fn full_text(&self) -> String {
        match self.action {
            KeyAction::Normal => String::from("UNLOCK"),
            KeyAction::Lock => String::from("LOCK"),
            KeyAction::Unlock => String::from("UNLOCK"),
            KeyAction::Pane => String::from("PANE"),
            KeyAction::Tab => String::from("TAB"),
            KeyAction::Resize => String::from("RESIZE"),
            KeyAction::Search => String::from("SEARCH"),
            KeyAction::Quit => String::from("QUIT"),
            KeyAction::Session => String::from("SESSION"),
            KeyAction::Move => String::from("MOVE"),
            KeyAction::Tmux => String::from("TMUX"),
        }
    }
    pub fn short_text(&self) -> String {
        match self.action {
            KeyAction::Normal => String::from("Un"),
            KeyAction::Lock => String::from("Lo"),
            KeyAction::Unlock => String::from("Un"),
            KeyAction::Pane => String::from("Pa"),
            KeyAction::Tab => String::from("Ta"),
            KeyAction::Resize => String::from("Re"),
            KeyAction::Search => String::from("Se"),
            KeyAction::Quit => String::from("Qu"),
            KeyAction::Session => String::from("Se"),
            KeyAction::Move => String::from("Mo"),
            KeyAction::Tmux => String::from("Tm"),
        }
    }
    pub fn with_shortened_modifiers(&self, common_modifiers: &Vec<KeyModifier>) -> String {
        let key = match &self.key {
            Some(k) => k.strip_common_modifiers(common_modifiers),
            None => return String::from("?"),
        };
        let shortened_modifiers = key
            .key_modifiers
            .iter()
            .map(|m| match m {
                KeyModifier::Ctrl => "^C",
                KeyModifier::Alt => "^A",
                KeyModifier::Super => "^Su",
                KeyModifier::Shift => "^Sh",
                _ => "",
            })
            .collect::<Vec<_>>()
            .join("-");
        if shortened_modifiers.is_empty() {
            format!("{}", key)
        } else {
            format!("{} {}", shortened_modifiers, key.bare_key)
        }
    }
    pub fn letter_shortcut(&self, common_modifiers: &Vec<KeyModifier>) -> String {
        let key = match &self.key {
            Some(k) => k.strip_common_modifiers(common_modifiers),
            None => return String::from("?"),
        };
        format!("{}", key)
    }
    pub fn get_key(&self) -> Option<KeyWithModifier> {
        self.key.clone()
    }
    pub fn get_mode(&self) -> KeyMode {
        self.mode
    }
    pub fn get_action(&self) -> KeyAction {
        self.action
    }
    pub fn is_selected(&self) -> bool {
        match self.mode {
            KeyMode::Selected => true,
            _ => false
        }
    }
}

/// Generate long mode shortcut tile.
///
/// A long mode shortcut tile consists of a leading and trailing `separator`, a keybinding enclosed
/// in `<>` brackets and the name of the mode displayed in capitalized letters next to it. For
/// example, the default long mode shortcut tile for "Locked" mode is: ` <g> LOCK `.
///
/// # Arguments
///
/// - `key`: A [`KeyShortcut`] that defines how the tile is displayed (active/disabled/...), what
///   action it belongs to (roughly equivalent to [`InputMode`]s) and the keybinding to trigger
///   this action.
/// - `palette`: A structure holding styling information.
/// - `separator`: The separator printed before and after the mode shortcut tile. The default is an
///   arrow head-like separator.
/// - `shared_super`: If set to true, all mode shortcut keybindings share a common modifier (see
///   [`get_common_modifier`]) and the modifier belonging to the keybinding is **not** printed in
///   the shortcut tile.
/// - `first_tile`: If set to true, the leading separator for this tile will be ommited so no gap
///   appears on the screen.
fn long_mode_shortcut(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    common_modifiers: &Vec<KeyModifier>,
    first_tile: bool,
) -> LinePart {
    let key_hint = key.full_text();
    let has_common_modifiers = !common_modifiers.is_empty();
    let key_binding = match (&key.mode, &key.key) {
        (KeyMode::Disabled, None) => "".to_string(),
        (_, None) => return LinePart::default(),
        (_, Some(_)) => key.letter_shortcut(common_modifiers),
    };

    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let start_separator = if !has_common_modifiers && first_tile {
        ""
    } else {
        separator
    };
    let prefix_separator = colors.prefix_separator.paint(start_separator);
    let char_left_separator = colors.char_left_separator.paint(" <".to_string());
    let char_shortcut = colors.char_shortcut.paint(key_binding.to_string());
    let char_right_separator = colors.char_right_separator.paint("> ".to_string());
    let styled_text = colors.styled_text.paint(format!("{} ", key_hint));
    let suffix_separator = colors.suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[
            prefix_separator,
            char_left_separator,
            char_shortcut,
            char_right_separator,
            styled_text,
            suffix_separator,
        ])
        .to_string(),
        len: start_separator.chars().count() // Separator
            + 2                              // " <"
            + key_binding.chars().count()    // Key binding
            + 2                              // "> "
            + key_hint.chars().count()       // Key hint (mode)
            + 1                              // " "
            + separator.chars().count(), // Separator
    }
}

fn shortened_modifier_shortcut(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    common_modifiers: &Vec<KeyModifier>,
    first_tile: bool,
) -> LinePart {
    let key_hint = key.full_text();
    let has_common_modifiers = !common_modifiers.is_empty();
    let key_binding = match (&key.mode, &key.key) {
        (KeyMode::Disabled, None) => "".to_string(),
        (_, None) => return LinePart::default(),
        (_, Some(_)) => key.with_shortened_modifiers(common_modifiers),
    };

    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let start_separator = if !has_common_modifiers && first_tile {
        ""
    } else {
        separator
    };
    let prefix_separator = colors.prefix_separator.paint(start_separator);
    let char_left_separator = colors.char_left_separator.paint(" <".to_string());
    let char_shortcut = colors.char_shortcut.paint(key_binding.to_string());
    let char_right_separator = colors.char_right_separator.paint("> ".to_string());
    let styled_text = colors.styled_text.paint(format!("{} ", key_hint));
    let suffix_separator = colors.suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[
            prefix_separator,
            char_left_separator,
            char_shortcut,
            char_right_separator,
            styled_text,
            suffix_separator,
        ])
        .to_string(),
        len: start_separator.chars().count() // Separator
            + 2                              // " <"
            + key_binding.chars().count()    // Key binding
            + 2                              // "> "
            + key_hint.chars().count()       // Key hint (mode)
            + 1                              // " "
            + separator.chars().count(), // Separator
    }
}

/// Generate short mode shortcut tile.
///
/// A short mode shortcut tile consists of a leading and trailing `separator` and a keybinding. For
/// example, the default short mode shortcut tile for "Locked" mode is: ` g `.
///
/// # Arguments
///
/// - `key`: A [`KeyShortcut`] that defines how the tile is displayed (active/disabled/...), what
///   action it belongs to (roughly equivalent to [`InputMode`]s) and the keybinding to trigger
///   this action.
/// - `palette`: A structure holding styling information.
/// - `separator`: The separator printed before and after the mode shortcut tile. The default is an
///   arrow head-like separator.
/// - `shared_super`: If set to true, all mode shortcut keybindings share a common modifier (see
///   [`get_common_modifier`]) and the modifier belonging to the keybinding is **not** printed in
///   the shortcut tile.
/// - `first_tile`: If set to true, the leading separator for this tile will be ommited so no gap
///   appears on the screen.
fn short_mode_shortcut(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    common_modifiers: &Vec<KeyModifier>,
    first_tile: bool,
) -> LinePart {
    let has_common_modifiers = !common_modifiers.is_empty();
    let key_binding = match (&key.mode, &key.key) {
        (KeyMode::Disabled, None) => "".to_string(),
        (_, None) => return LinePart::default(),
        (_, Some(_)) => key.letter_shortcut(common_modifiers),
    };

    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let start_separator = if !has_common_modifiers && first_tile {
        ""
    } else {
        separator
    };
    let prefix_separator = colors.prefix_separator.paint(start_separator);
    let char_shortcut = colors.char_shortcut.paint(format!(" {} ", key_binding));
    let suffix_separator = colors.suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len: separator.chars().count()      // Separator
            + 1                             // " "
            + key_binding.chars().count()   // Key binding
            + 1                             // " "
            + separator.chars().count(), // Separator
    }
}

fn key_indicators(
    max_len: usize,
    keys: &[KeyShortcut],
    palette: ColoredElements,
    separator: &str,
    mode_info: &ModeInfo,
    line_part_to_render: &mut LinePart,
) {
    if keys.is_empty() {
        return;
    }
    // Print full-width hints
    let shared_modifiers = superkey(palette, separator, mode_info, line_part_to_render);
    let mut line_part = LinePart::default();
    for key in keys {
        let line_empty = line_part_to_render.len == 0;
        let key = long_mode_shortcut(key, palette, separator, &shared_modifiers, line_empty);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part_to_render.len + line_part.len < max_len {
        line_part_to_render.part = format!("{}{}", line_part_to_render.part, line_part.part);
        line_part_to_render.len += line_part.len;
        return;
    }

    // Full-width doesn't fit, try shortened modifiers (eg. "^C" instead of "Ctrl")
    let mut line_part = LinePart::default();
    for key in keys {
        let line_empty = line_part.len == 0;
        let key =
            shortened_modifier_shortcut(key, palette, separator, &shared_modifiers, line_empty);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part_to_render.len + line_part.len < max_len {
        line_part_to_render.part  = format!("{}{}", line_part_to_render.part, line_part.part);
        line_part_to_render.len += line_part.len;
        return;
    }

    // Full-width doesn't fit, try shortened hints (just keybindings, no meanings/actions)
    let mut line_part = LinePart::default();
    for key in keys {
        let line_empty = line_part.len == 0;
        let key = short_mode_shortcut(key, palette, separator, &shared_modifiers, line_empty);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part_to_render.len + line_part.len < max_len {
        line_part_to_render.part  = format!("{}{}", line_part_to_render.part, line_part.part);
        line_part_to_render.len += line_part.len;
        return;
    }

    // nothing fits, print nothing
}

fn swap_layout_keycode(mode_info: &ModeInfo, palette: &Palette) -> LinePart {
    let mode_keybinds = mode_info.get_mode_keybinds();
    let prev_next_keys = action_key_group(
        &mode_keybinds,
        &[&[Action::PreviousSwapLayout], &[Action::NextSwapLayout]],
    );
    style_key_with_modifier(&prev_next_keys, palette, Some(palette.black))
//     let prev_next_keys_indicator =
//         style_key_with_modifier(&prev_next_keys, palette, Some(palette.black));
//     let keycode = ANSIStrings(&prev_next_keys_indicator);
//     // TODO: CONTINUE HERE - instead of relying on unstyled_len here and in other places, count the
//     // characters and return them as a LinePart
//     let len = unstyled_len(&keycode).saturating_sub(4);
//     let part = keycode.to_string();
//     LinePart { part, len }
}

fn swap_layout_status(
    max_len: usize,
    swap_layout_name: &Option<String>,
    is_swap_layout_damaged: bool,
    mode_info: &ModeInfo,
    colored_elements: ColoredElements,
    palette: &Palette,
    separator: &str,
) -> Option<LinePart> {
    match swap_layout_name {
        Some(swap_layout_name) => {
            let mut swap_layout_name = format!(" {} ", swap_layout_name);
            swap_layout_name.make_ascii_uppercase();
            let keycode = swap_layout_keycode(mode_info, palette);
            let swap_layout_name_len = swap_layout_name.len() + 2; // 2 for the arrow separators
            macro_rules! style_swap_layout_indicator {
                ($style_name:ident) => {{
                    (
                        colored_elements
                            .$style_name
                            .prefix_separator
                            .paint(separator),
                        colored_elements
                            .$style_name
                            .styled_text
                            .paint(&swap_layout_name),
                        colored_elements
                            .$style_name
                            .suffix_separator
                            .paint(separator),
                    )
                }};
            }
            let (prefix_separator, swap_layout_name, suffix_separator) =
//                 if mode_info.mode == InputMode::Locked {
//                     style_swap_layout_indicator!(disabled)
                if is_swap_layout_damaged {
                    style_swap_layout_indicator!(unselected)
                } else {
                    style_swap_layout_indicator!(selected)
                };
            let swap_layout_indicator = format!(
                "{}{}{}",
                prefix_separator, swap_layout_name, suffix_separator
            );
            let (part, full_len) = 
                (
                    format!(
                        "{}{}",
                        keycode,
                        swap_layout_indicator,
                    ),
                    keycode.len + swap_layout_name_len
                );
            let short_len = swap_layout_name_len + 1; // 1 is the space between
            if full_len <= max_len {
                Some(LinePart {
                    part,
                    len: full_len,
                })
            } else if short_len <= max_len && mode_info.mode != InputMode::Locked {
                Some(LinePart {
                    part: swap_layout_indicator,
                    len: short_len,
                })
            } else {
                None
            }
        },
        None => None,
    }
}

/// Get the keybindings for switching `InputMode`s and `Quit` visible in status bar.
///
/// Return a Vector of `Key`s where each `Key` is a shortcut to switch to some `InputMode` or Quit
/// zellij. Given the vast amount of things a user can configure in their zellij config, this
/// function has some limitations to keep in mind:
///
/// - The vector is not deduplicated: If switching to a certain `InputMode` is bound to multiple
///   `Key`s, all of these bindings will be part of the returned vector. There is also no
///   guaranteed sort order. Which key ends up in the status bar in such a situation isn't defined.
/// - The vector will **not** contain the ' ', '\n' and 'Esc' keys: These are the default bindings
///   to get back to normal mode from any input mode, but they aren't of interest when searching
///   for the super key. If for any input mode the user has bound only these keys to switching back
///   to `InputMode::Normal`, a '?' will be displayed as keybinding instead.
pub fn mode_switch_keys(mode_info: &ModeInfo) -> Vec<KeyWithModifier> {
    mode_info
        .get_mode_keybinds()
        .iter()
        .filter_map(|(key, vac)| match vac.first() {
            // No actions defined, ignore
            None => None,
            Some(vac) => {
                // We ignore certain "default" keybindings that switch back to normal InputMode.
                // These include: ' ', '\n', 'Esc'
                if matches!(
                    key,
                    KeyWithModifier {
                        bare_key: BareKey::Char(' '),
                        ..
                    } | KeyWithModifier {
                        bare_key: BareKey::Enter,
                        ..
                    } | KeyWithModifier {
                        bare_key: BareKey::Esc,
                        ..
                    }
                ) {
                    return None;
                }
                if let actions::Action::SwitchToMode(mode) = vac {
                    return match mode {
                        // Store the keys that switch to displayed modes
                        InputMode::Normal
                        | InputMode::Locked
                        | InputMode::Pane
                        | InputMode::Tab
                        | InputMode::Resize
                        | InputMode::Move
                        | InputMode::Scroll
                        | InputMode::Session => Some(key.clone()),
                        _ => None,
                    };
                }
                if let actions::Action::Quit = vac {
                    return Some(key.clone());
                }
                // Not a `SwitchToMode` or `Quit` action, ignore
                None
            },
        })
        .collect()
}

pub fn superkey(
    palette: ColoredElements,
    separator: &str,
    mode_info: &ModeInfo,
    line_part_to_render: &mut LinePart,
) -> Vec<KeyModifier> {
    // Find a common modifier if any
    let common_modifiers = get_common_modifiers(mode_switch_keys(mode_info).iter().collect());
    if common_modifiers.is_empty() {
        return common_modifiers;
    }

    let prefix_text = if mode_info.capabilities.arrow_fonts {
        // Add extra space in simplified ui
        format!(
            " {} + ",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    } else {
        format!(
            " {} +",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    };

    let prefix = palette.superkey_prefix.paint(&prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    line_part_to_render.part = format!("{}{}", line_part_to_render.part, ANSIStrings(&[prefix, suffix_separator]).to_string());
    line_part_to_render.len += prefix_text.chars().count() + separator.chars().count();
    common_modifiers
}

pub fn to_char(kv: Vec<KeyWithModifier>) -> Option<KeyWithModifier> {
    let key = kv
        .iter()
        .filter(|key| {
            // These are general "keybindings" to get back to normal, they aren't interesting here.
            !matches!(
                key,
                KeyWithModifier {
                    bare_key: BareKey::Enter,
                    ..
                } | KeyWithModifier {
                    bare_key: BareKey::Char(' '),
                    ..
                } | KeyWithModifier {
                    bare_key: BareKey::Esc,
                    ..
                }
            )
        })
        .collect::<Vec<&KeyWithModifier>>()
        .into_iter()
        .next();
    // Maybe the user bound one of the ignored keys?
    if key.is_none() {
        return kv.first().cloned();
    }
    key.cloned()
}

/// Get the [`KeyShortcut`] for a specific [`InputMode`].
///
/// Iterates over the contents of `shortcuts` to find the [`KeyShortcut`] with the [`KeyAction`]
/// matching the [`InputMode`]. Returns a mutable reference to the entry in `shortcuts` if a match
/// is found or `None` otherwise.
///
/// In case multiple entries in `shortcuts` match `mode` (which shouldn't happen), the first match
/// is returned.
fn get_key_shortcut_for_mode<'a>(
    shortcuts: &'a mut [KeyShortcut],
    mode: &InputMode,
) -> Option<&'a mut KeyShortcut> {
    let key_action = match mode {
        InputMode::Normal | InputMode::Prompt | InputMode::Tmux => return None,
        InputMode::Locked => KeyAction::Lock,
        InputMode::Pane | InputMode::RenamePane => KeyAction::Pane,
        InputMode::Tab | InputMode::RenameTab => KeyAction::Tab,
        InputMode::Resize => KeyAction::Resize,
        InputMode::Move => KeyAction::Move,
        InputMode::Scroll | InputMode::Search | InputMode::EnterSearch => KeyAction::Search,
        InputMode::Session => KeyAction::Session,
    };
    for shortcut in shortcuts.iter_mut() {
        if shortcut.action == key_action {
            return Some(shortcut);
        }
    }
    None
}

fn render_current_mode_keybinding(help: &ModeInfo, max_len: usize, separator: &str, line_part_to_render: &mut LinePart) {
    let binds = &help.get_mode_keybinds();
    match help.mode {
        InputMode::Normal => {
            let action_key = action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Locked)],
            );
            let mut key_to_display = action_key
                .iter()
                .find(|k| k.is_key_with_ctrl_modifier(BareKey::Char('g')))
                .or_else(|| action_key.iter().next());
            let key_to_display = if let Some(key_to_display) = key_to_display.take() {
                vec![key_to_display.clone()]
            } else {
                vec![]
            };
            let keybinding = add_shortcut(help, "LOCK", &key_to_display, false);
            if line_part_to_render.len + keybinding.len <= max_len {
                line_part_to_render.append(&keybinding);
            }

        }
        InputMode::Locked => {
            let action_key = action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Normal)],
            );
            let mut key_to_display = action_key
                .iter()
                .find(|k| k.is_key_with_ctrl_modifier(BareKey::Char('g')))
                .or_else(|| action_key.iter().next());
            let key_to_display = if let Some(key_to_display) = key_to_display.take() {
                vec![key_to_display.clone()]
            } else {
                vec![]
            };
            let keybinding = add_shortcut(help, "LOCK", &key_to_display, false);
            if line_part_to_render.len + keybinding.len <= max_len {
                line_part_to_render.append(&keybinding);
            }
        }
        _ => {
            let locked_key_to_display = {
                let action_key = action_key(
                    binds,
                    &[Action::SwitchToMode(InputMode::Locked)], // needs to be base mode
                );
                let mut key_to_display = action_key
                    .iter()
                    .find(|k| k.is_key_with_ctrl_modifier(BareKey::Char('g')))
                    .or_else(|| action_key.iter().next());
                if let Some(key_to_display) = key_to_display.take() {
                    vec![key_to_display.clone()]
                } else {
                    vec![]
                }
            };

//             let normal_key_to_display = {
//                 action_key(
//                     binds,
//                     &[Action::SwitchToMode(InputMode::Normal)],
//                 )
//             };

            let keybinding = add_shortcut(help, "LOCK", &locked_key_to_display, true);
            // let keybinding = add_shortcut_selected(help, &keybinding, &format!("{:?}", help.mode).to_uppercase(), normal_key_to_display);
            if line_part_to_render.len + keybinding.len <= max_len {
                line_part_to_render.append(&keybinding);
            }
        }
    }
}

fn base_mode_locked_mode_indicators(help: &ModeInfo) -> HashMap<InputMode, Vec<KeyShortcut>> {
    let locked_binds = &help.get_keybinds_for_mode(InputMode::Locked);
    let normal_binds = &help.get_keybinds_for_mode(InputMode::Normal);
    let pane_binds = &help.get_keybinds_for_mode(InputMode::Pane);
    let tab_binds = &help.get_keybinds_for_mode(InputMode::Tab);
    let resize_binds = &help.get_keybinds_for_mode(InputMode::Resize);
    let move_binds = &help.get_keybinds_for_mode(InputMode::Move);
    let scroll_binds = &help.get_keybinds_for_mode(InputMode::Scroll);
    let session_binds = &help.get_keybinds_for_mode(InputMode::Session);
    HashMap::from([
        (
            InputMode::Locked,
            vec![
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Unlock,
                    to_char(action_key(locked_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                ),
            ],
        ),
        (
            InputMode::Normal,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Pane)])),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Tab)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Resize)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Move)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Scroll)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Session)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Quit,
                    to_char(action_key(normal_binds, &[Action::Quit])),
                ),
            ]
        ),
        (
            InputMode::Pane,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(pane_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Pane,
                    to_char(action_key(pane_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Tab,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(tab_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Tab,
                    to_char(action_key(tab_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Resize,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(resize_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Resize,
                    to_char(action_key(resize_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Move,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(move_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Move,
                    to_char(action_key(move_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Scroll,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(scroll_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Search,
                    to_char(action_key(scroll_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Session,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Unlock,
                    to_char(action_key(session_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Session,
                    to_char(action_key(session_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        )
    ])
}

fn base_mode_normal_mode_indicators(help: &ModeInfo) -> HashMap<InputMode, Vec<KeyShortcut>> {
    let locked_binds = &help.get_keybinds_for_mode(InputMode::Locked);
    let normal_binds = &help.get_keybinds_for_mode(InputMode::Normal);
    let pane_binds = &help.get_keybinds_for_mode(InputMode::Pane);
    let tab_binds = &help.get_keybinds_for_mode(InputMode::Tab);
    let resize_binds = &help.get_keybinds_for_mode(InputMode::Resize);
    let move_binds = &help.get_keybinds_for_mode(InputMode::Move);
    let scroll_binds = &help.get_keybinds_for_mode(InputMode::Scroll);
    let session_binds = &help.get_keybinds_for_mode(InputMode::Session);
    HashMap::from([
        (
            InputMode::Locked,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Lock,
                    to_char(action_key(locked_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                ),
            ]
        ),
        (
            InputMode::Normal,
            vec![
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Lock,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Locked)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Pane,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Pane)])),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Tab,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Tab)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Resize,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Resize)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Move,
                    to_char(action_key(normal_binds, &[Action::SwitchToMode(InputMode::Move)])),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Search,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Scroll)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::Unselected,
                    KeyAction::Session,
                    to_char(action_key(
                        normal_binds,
                        &[Action::SwitchToMode(InputMode::Session)],
                    )),
                ),
                KeyShortcut::new(
                    KeyMode::UnselectedAlternate,
                    KeyAction::Quit,
                    to_char(action_key(normal_binds, &[Action::Quit])),
                ),
            ]
        ),
        (
            InputMode::Pane,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Pane,
                    to_char(action_key(pane_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Tab,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Tab,
                    to_char(action_key(tab_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Resize,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Resize,
                    to_char(action_key(resize_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Move,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Move,
                    to_char(action_key(move_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Scroll,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Search,
                    to_char(action_key(scroll_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        ),
        (
            InputMode::Session,
            vec![
                KeyShortcut::new(
                    KeyMode::Selected,
                    KeyAction::Session,
                    to_char(action_key(session_binds, &[Action::SwitchToMode(InputMode::Normal)])),
                )
            ]
        )
    ])
}
fn render_mode_key_indicators(help: &ModeInfo, max_len: usize, separator: &str, base_mode_is_locked: bool) -> Option<LinePart> {
    let mut line_part_to_render = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let default_keys = if base_mode_is_locked {
        base_mode_locked_mode_indicators(help)
    } else {
        base_mode_normal_mode_indicators(help)
    };
    match common_modifiers_in_all_modes(&default_keys) {
        Some(modifiers) => {
            if let Some(default_keys) = default_keys.get(&help.mode) {
                let keys_without_common_modifiers: Vec<KeyShortcut> = default_keys.iter().map(|key_shortcut| {
                    let key = key_shortcut.get_key().map(|k| k.strip_common_modifiers(&modifiers));
                    let mode = key_shortcut.get_mode();
                    let action = key_shortcut.get_action();
                    KeyShortcut::new(
                        mode,
                        action,
                        key
                    )
                }).collect();
                render_common_modifiers(&colored_elements, help, &modifiers, &mut line_part_to_render, separator);

                let full_shortcut_list = full_inline_keys_modes_shortcut_list(&keys_without_common_modifiers, help);

                if line_part_to_render.len + full_shortcut_list.len <= max_len {
                    line_part_to_render.append(&full_shortcut_list);
                } else {
                    let shortened_shortcut_list = shortened_inline_keys_modes_shortcut_list(&keys_without_common_modifiers, help);
                    if line_part_to_render.len + shortened_shortcut_list.len <= max_len {
                        line_part_to_render.append(&shortened_shortcut_list);
                    }
                }

            }
        },
        None => {
            if let Some(default_keys) = default_keys.get(&help.mode) {
                let full_shortcut_list = full_modes_shortcut_list(&default_keys, help);
                if line_part_to_render.len + full_shortcut_list.len <= max_len {
                    line_part_to_render.append(&full_shortcut_list);
                } else {
                    let shortened_shortcut_list = shortened_modes_shortcut_list(&default_keys, help);
                    if line_part_to_render.len + shortened_shortcut_list.len <= max_len {
                        line_part_to_render.append(&shortened_shortcut_list);
                    }
                }
            }
        }
    }
    if line_part_to_render.len <= max_len {
        Some(line_part_to_render)
    } else {
        None
    }
}

fn full_inline_keys_modes_shortcut_list(keys_without_common_modifiers: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut full_shortcut_list = LinePart::default();
    for key in keys_without_common_modifiers {
        let is_selected = key.is_selected();
        let shortcut = add_shortcut_with_inline_key(help, &key.full_text(), key.key.as_ref().map(|k| vec![k.clone()]).unwrap_or_else(|| vec![]), is_selected);
        full_shortcut_list.append(&shortcut);
    }
    full_shortcut_list
}

fn shortened_inline_keys_modes_shortcut_list(keys_without_common_modifiers: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut shortened_shortcut_list = LinePart::default();
    for key in keys_without_common_modifiers {
        let is_selected = key.is_selected();
        let shortcut = add_shortcut_with_key_only(help, key.key.as_ref().map(|k| vec![k.clone()]).unwrap_or_else(|| vec![]), is_selected);
        shortened_shortcut_list.append(&shortcut);
    }
    shortened_shortcut_list
}

fn full_modes_shortcut_list(default_keys: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut full_shortcut_list = LinePart::default();
    for key in default_keys {
        let is_selected = key.is_selected();
        full_shortcut_list.append(&add_shortcut(help, &key.full_text(), &key.key.as_ref().map(|k| vec![k.clone()]).unwrap_or_else(|| vec![]), is_selected));
    }
    full_shortcut_list
}

fn shortened_modes_shortcut_list(default_keys: &Vec<KeyShortcut>, help: &ModeInfo) -> LinePart {
    let mut shortened_shortcut_list = LinePart::default();
    for key in default_keys {
        let is_selected = key.is_selected();
        shortened_shortcut_list.append(&add_shortcut(help, &key.short_text(), &key.key.as_ref().map(|k| vec![k.clone()]).unwrap_or_else(|| vec![]), is_selected));
    }
    shortened_shortcut_list
}

fn common_modifiers_in_all_modes(key_shortcuts: &HashMap<InputMode, Vec<KeyShortcut>>) -> Option<Vec<KeyModifier>> {
    let Some(mut common_modifiers) = key_shortcuts.iter().next().and_then(|k| k.1.iter().next().and_then(|k| k.get_key().map(|k| k.key_modifiers.clone()))) else {
        return None;
    };
    for (_mode, key_shortcuts) in key_shortcuts {

        if key_shortcuts.is_empty() {
            return None;
        }
        let Some(mut common_modifiers_for_mode) = key_shortcuts.iter().next().unwrap().get_key().map(|k| k.key_modifiers.clone()) else {
            return None;
        };
        for key in key_shortcuts {
            let Some(key) = key.get_key() else {
                return None;
            };
            common_modifiers_for_mode = common_modifiers_for_mode
                .intersection(&key.key_modifiers)
                .cloned()
                .collect();
        }
        common_modifiers = common_modifiers.intersection(&common_modifiers_for_mode).cloned().collect();
    }
    if common_modifiers.is_empty() {
        return None;
    }
    Some(common_modifiers.into_iter().collect())
}

fn render_common_modifiers(palette: &ColoredElements, mode_info: &ModeInfo, common_modifiers: &Vec<KeyModifier>, line_part_to_render: &mut LinePart, separator: &str) {
    let prefix_text = if mode_info.capabilities.arrow_fonts {
        // Add extra space in simplified ui
        format!(
            " {} + ",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    } else {
        format!(
            " {} +",
            common_modifiers
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("-")
        )
    };

    let prefix = palette.superkey_prefix.paint(&prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    line_part_to_render.part = format!("{}{}", line_part_to_render.part, ANSIStrings(&[prefix, suffix_separator]).to_string());
    line_part_to_render.len += prefix_text.chars().count() + separator.chars().count();
}

fn render_secondary_info(help: &ModeInfo, tab_info: Option<&TabInfo>, max_len: usize, separator: &str) -> Option<LinePart> {
    let mut secondary_info = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let secondary_keybinds = secondary_keybinds(&help, tab_info, max_len, separator);
    secondary_info.append(&secondary_keybinds);
    let remaining_space = max_len.saturating_sub(secondary_info.len).saturating_sub(1); // 1 for the end padding of the line
    let mut padding = String::new();
    let mut padding_len = 0;
    for _ in 0..remaining_space {
        padding.push_str(
            &ANSIStrings(&[colored_elements.superkey_prefix.paint(" ")]).to_string(),
        );
        padding_len += 1;
    }
    secondary_info.part = format!("{}{}", padding, secondary_info.part);
    secondary_info.len += padding_len;
    if secondary_info.len <= max_len {
        Some(secondary_info)
    } else {
        None
    }
}

pub fn one_line_ui(
    help: &ModeInfo,
    tab_info: Option<&TabInfo>,
    mut max_len: usize,
    separator: &str,
    base_mode_is_locked: bool,
) -> LinePart {
    let mut line_part_to_render = LinePart::default();
    let mut append = |line_part: &LinePart, max_len: &mut usize| {
        line_part_to_render.append(line_part);
        *max_len = max_len.saturating_sub(line_part.len);
    };

    render_mode_key_indicators(help, max_len, separator, base_mode_is_locked)
        .map(|mode_key_indicators| append(&mode_key_indicators, &mut max_len))
        .and_then(|_| {
            match help.mode {
                InputMode::Normal | InputMode::Locked => {
                    render_secondary_info(help, tab_info, max_len, separator)
                        .map(|secondary_info| append(&secondary_info, &mut max_len))
                },
                _ => {
                    add_keygroup_separator(help, max_len)
                        .map(|key_group_separator| append(&key_group_separator, &mut max_len))
                        .and_then(|_| keybinds(help, max_len))
                        .map(|keybinds| append(&keybinds, &mut max_len))
                }
            }
        });
    line_part_to_render
}

fn secondary_keybinds(help: &ModeInfo, tab_info: Option<&TabInfo>, max_len: usize, separator: &str) -> LinePart {
    let mut secondary_info = LinePart::default();
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let binds = &help.get_mode_keybinds();

    // New Pane
    let new_pane_action_key = action_key(
        binds,
        &[Action::NewPane(None, None)],
    );
    let mut key_to_display = new_pane_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Char('n')))
        .or_else(|| new_pane_action_key.iter().next());
    let key_to_display = if let Some(key_to_display) = key_to_display.take() {
        vec![key_to_display.clone()]
    } else {
        vec![]
    };
    // secondary_info.append(&add_shortcut(help, "New Pane", key_to_display, false));

    // Move focus
    let mut move_focus_shortcuts: Vec<KeyWithModifier> = vec![];

    // Left
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocusOrTab(Direction::Left)]
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Left))
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Down
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocus(Direction::Down)]
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Down))
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Up
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocus(Direction::Up)]
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Up))
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }
    // Right
    let move_focus_left_action_key = action_key(
        binds,
        &[Action::MoveFocusOrTab(Direction::Right)]
    );
    let move_focus_left_key = move_focus_left_action_key
        .iter()
        .find(|k| k.is_key_with_alt_modifier(BareKey::Right))
        .or_else(|| move_focus_left_action_key.iter().next());
    if let Some(move_focus_left_key) = move_focus_left_key {
        move_focus_shortcuts.push(move_focus_left_key.clone());
    }

    secondary_info.append(&add_shortcut(help, "New Pane", &key_to_display, false));
    secondary_info.append(&add_shortcut(help, "Change Focus", &move_focus_shortcuts, false));

    let swap_layout_indicator = tab_info.and_then(|tab_info| swap_layout_status(
        max_len,
        &tab_info.active_swap_layout_name,
        tab_info.is_swap_layout_dirty,
        help,
        colored_elements,
        &help.style.colors,
        separator,
    ));

    if let Some(swap_layout_indicator) = &swap_layout_indicator {
        secondary_info.append(&swap_layout_indicator);
    }

    if secondary_info.len <= max_len {
        secondary_info
    } else {
        let mut short_line = LinePart::default();
        short_line.append(&add_shortcut(help, "New", &key_to_display, false));
        short_line.append(&add_shortcut(help, "Focus", &move_focus_shortcuts, false));
        if let Some(swap_layout_indicator) = swap_layout_indicator {
            short_line.append(&swap_layout_indicator);
        }
        short_line
    }

}

// fn secondary_keybinds_short(help: &ModeInfo, tab_info: Option<&TabInfo>, max_len: usize, separator: &str) -> LinePart {
//     let mut secondary_info = LinePart::default();
//     let supports_arrow_fonts = !help.capabilities.arrow_fonts;
//     let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
//     let binds = &help.get_mode_keybinds();
// 
//     // New Pane
//     let new_pane_action_key = action_key(
//         binds,
//         &[Action::NewPane(None, None)],
//     );
//     let mut key_to_display = new_pane_action_key
//         .iter()
//         .find(|k| k.is_key_with_alt_modifier(BareKey::Char('n')))
//         .or_else(|| new_pane_action_key.iter().next());
//     let key_to_display = if let Some(key_to_display) = key_to_display.take() {
//         vec![key_to_display.clone()]
//     } else {
//         vec![]
//     };
//     secondary_info.append(&add_shortcut(help, "New", &key_to_display, false));
// 
//     // Move focus
//     let mut move_focus_shortcuts: Vec<KeyWithModifier> = vec![];
// 
//     // Left
//     let move_focus_left_action_key = action_key(
//         binds,
//         &[Action::MoveFocusOrTab(Direction::Left)]
//     );
//     let move_focus_left_key = move_focus_left_action_key
//         .iter()
//         .find(|k| k.is_key_with_alt_modifier(BareKey::Left))
//         .or_else(|| move_focus_left_action_key.iter().next());
//     if let Some(move_focus_left_key) = move_focus_left_key {
//         move_focus_shortcuts.push(move_focus_left_key.clone());
//     }
//     // Down
//     let move_focus_left_action_key = action_key(
//         binds,
//         &[Action::MoveFocus(Direction::Down)]
//     );
//     let move_focus_left_key = move_focus_left_action_key
//         .iter()
//         .find(|k| k.is_key_with_alt_modifier(BareKey::Down))
//         .or_else(|| move_focus_left_action_key.iter().next());
//     if let Some(move_focus_left_key) = move_focus_left_key {
//         move_focus_shortcuts.push(move_focus_left_key.clone());
//     }
//     // Up
//     let move_focus_left_action_key = action_key(
//         binds,
//         &[Action::MoveFocus(Direction::Up)]
//     );
//     let move_focus_left_key = move_focus_left_action_key
//         .iter()
//         .find(|k| k.is_key_with_alt_modifier(BareKey::Up))
//         .or_else(|| move_focus_left_action_key.iter().next());
//     if let Some(move_focus_left_key) = move_focus_left_key {
//         move_focus_shortcuts.push(move_focus_left_key.clone());
//     }
//     // Right
//     let move_focus_left_action_key = action_key(
//         binds,
//         &[Action::MoveFocusOrTab(Direction::Right)]
//     );
//     let move_focus_left_key = move_focus_left_action_key
//         .iter()
//         .find(|k| k.is_key_with_alt_modifier(BareKey::Right))
//         .or_else(|| move_focus_left_action_key.iter().next());
//     if let Some(move_focus_left_key) = move_focus_left_key {
//         move_focus_shortcuts.push(move_focus_left_key.clone());
//     }
// 
//     secondary_info.append(&add_shortcut(help, "Focus", &move_focus_shortcuts, false));
// 
//     if let Some(swap_layout_indicator) = tab_info.and_then(|tab_info| swap_layout_status(
//         max_len,
//         &tab_info.active_swap_layout_name,
//         tab_info.is_swap_layout_dirty,
//         help,
//         colored_elements,
//         &help.style.colors,
//         separator,
//     )) {
//         secondary_info.append(&swap_layout_indicator);
//     }
// 
//     secondary_info
// }

// pub fn add_shortcut(
//     help: &ModeInfo,
//     text: &str,
//     keys: Vec<KeyWithModifier>,
// ) -> LinePart {
//     let separator = crate::ARROW_SEPARATOR; // TODO: from args
//     let selected = false;
//     full_length_shortcut(true, keys, text, help.style.colors, separator, selected)
// }

fn full_length_shortcut(
    is_first_shortcut: bool,
    key: Vec<KeyWithModifier>,
    action: &str,
    palette: Palette,
    arrow_separator: &str,
    selected: bool,
) -> LinePart {
    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    });

    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => palette_match!(palette.black),
        ThemeHue::Light => palette_match!(palette.white),
    };
    let fg_color = match palette.theme_hue {
        ThemeHue::Dark => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
        ThemeHue::Light => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
    };

    ret.append(&style_key_with_modifier(&key, &palette, None)); // TODO: alternate

    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(fg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(fg_color)
            .bold()
            .paint(format!(" {} ", action)),
    );
    bits.push(
        Style::new()
            .fg(fg_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += action.chars().count() + 4; // padding and arrow fonts

    ret
}

pub fn keybinds(help: &ModeInfo, max_width: usize) -> Option<LinePart> {
    // It is assumed that there is at least one TIP data in the TIPS HasMap.
//     let tip_body = TIPS
//         .get(tip_name)
//         .unwrap_or_else(|| TIPS.get("quicknav").unwrap());

    let full_shortcut_list = full_shortcut_list(help);
    if full_shortcut_list.len <= max_width {
        return Some(full_shortcut_list);
    }
    let shortened_shortcut_list = shortened_shortcut_list(help);
    if shortened_shortcut_list.len <= max_width {
        return Some(shortened_shortcut_list);
    }
    Some(best_effort_shortcut_list(help, max_width))
}

pub fn add_shortcut(
    help: &ModeInfo,
    text: &str,
    keys: &Vec<KeyWithModifier>,
    selected: bool,
) -> LinePart {
    let arrow_separator = crate::ARROW_SEPARATOR; // TODO: from args
    let palette = help.style.colors;
    let mut ret = LinePart::default();
    if keys.is_empty() {
        return ret;
    }

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    });

    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => palette_match!(palette.black),
        ThemeHue::Light => palette_match!(palette.white),
    };
    let fg_color = match palette.theme_hue {
        ThemeHue::Dark => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
        ThemeHue::Light => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
    };

    ret.append(&style_key_with_modifier(&keys, &palette, None)); // TODO: alternate

    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(fg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(fg_color)
            .bold()
            .paint(format!(" {} ", text)),
    );
    bits.push(
        Style::new()
            .fg(fg_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += text.chars().count() + 4; // padding and arrow fonts

    ret
}

pub fn add_only_key_shortcut_selected(
    help: &ModeInfo,
    keys: Vec<KeyWithModifier>,
    selected: bool
) -> LinePart {
    let mut ret = LinePart::default();
    let palette = help.style.colors;
    let arrow_separator = crate::ARROW_SEPARATOR; // TODO: from args

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    });

    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => palette_match!(palette.black),
        ThemeHue::Light => palette_match!(palette.white),
    };
    let fg_color = match palette.theme_hue {
        ThemeHue::Dark => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
        ThemeHue::Light => if selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
    };

    let key_string = format!("{}", keys.iter().map(|k| k.to_string()).collect::<Vec<_>>().join("-"));
    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(fg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(fg_color)
            .bold()
            .paint(format!(" {} ", key_string)),
    );
    bits.push(
        Style::new()
            .fg(fg_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += key_string.chars().count() + 4; // padding and arrow fonts

    ret
}

pub fn add_shortcut_with_inline_key(
    help: &ModeInfo,
    text: &str,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
) -> LinePart {
    let arrow_separator = crate::ARROW_SEPARATOR; // TODO: from args
    let palette = help.style.colors;

    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    });

    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => if is_selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
        ThemeHue::Light => if is_selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
    };
    let shortcut_color = match palette.theme_hue {
        ThemeHue::Dark => palette_match!(palette.red),
        ThemeHue::Light => palette_match!(palette.red),
    };

    // ret.append(&style_key_with_modifier(&key, &palette, None)); // TODO: alternate
    let key_string = format!("{}", key.iter().map(|k| k.to_string()).collect::<Vec<_>>().join("-"));

    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(text_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(bg_color)
            .bold()
            .paint(format!(" <")),
    );
    bits.push(
        Style::new()
            .fg(shortcut_color)
            .on(bg_color)
            .bold()
            .paint(&key_string)
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(bg_color)
            .bold()
            .paint(format!("> ")),
    );
    bits.push(
        Style::new()
            .fg(text_color)
            .on(bg_color)
            .bold()
            .paint(format!("{} ", text)),
    );
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(text_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    // TODO: check line length and max length and stuff
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += text.chars().count() + key_string.chars().count() + 7; // padding, group boundaries and arrow fonts

    ret
}

pub fn add_shortcut_with_key_only(
    help: &ModeInfo,
    key: Vec<KeyWithModifier>,
    is_selected: bool,
) -> LinePart {
    let arrow_separator = crate::ARROW_SEPARATOR; // TODO: from args
    let palette = help.style.colors;

    let mut ret = LinePart::default();
    if key.is_empty() {
        return ret;
    }

    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    });

    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => if is_selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
        ThemeHue::Light => if is_selected { palette_match!(palette.green) } else { palette_match!(palette.fg) },
    };
    let shortcut_color = match palette.theme_hue {
        ThemeHue::Dark => palette_match!(palette.red),
        ThemeHue::Light => palette_match!(palette.red),
    };

    // ret.append(&style_key_with_modifier(&key, &palette, None)); // TODO: alternate
    let key_string = format!(" {} ", key.iter().map(|k| k.to_string()).collect::<Vec<_>>().join("-"));

    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(text_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(shortcut_color)
            .on(bg_color)
            .bold()
            .paint(&key_string)
    );
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(text_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    // TODO: check line length and max length and stuff
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += key_string.chars().count() + 2; // 2 => arrow fonts

    ret
}

pub fn add_keygroup_separator (
    help: &ModeInfo,
    max_len: usize,
) -> Option<LinePart> {
    let arrow_separator = crate::ARROW_SEPARATOR; // TODO: from args
    let palette = help.style.colors;

    let mut ret = LinePart::default();

    let separator_color = palette_match!(palette.orange);
    let bg_color = palette_match!(palette.black);
    let mut bits: Vec<ANSIString> = vec![];
    bits.push(
        Style::new()
            .fg(bg_color)
            .on(separator_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    bits.push(
        Style::new()
            .fg(separator_color)
            .on(separator_color)
            .bold()
            .paint(format!(" ")),
    );
    bits.push(
        Style::new()
            .fg(separator_color)
            .on(bg_color)
            .bold()
            .paint(format!("{}", arrow_separator)),
    );
    // TODO: check line length and max length and stuff
    ret.part = format!("{}{}", ret.part, ANSIStrings(&bits));
    ret.len += 3; // padding, group boundaries and arrow fonts

    if ret.len <= max_len {
        Some(ret)
    } else {
        None
    }
}

fn full_shortcut_list(help: &ModeInfo) -> LinePart {
    match help.mode {
        InputMode::Normal => LinePart::default(),
        InputMode::Locked => LinePart::default(),
        _ => full_shortcut_list_nonstandard_mode(help),
    }
}

fn full_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (long, _short, keys) in keys_and_hints.into_iter() {
        line_part.append(&add_shortcut(help, &long, &keys.to_vec(), false));
    }
    line_part
}

#[rustfmt::skip]
fn get_keys_and_hints(mi: &ModeInfo) -> Vec<(String, String, Vec<KeyWithModifier>)> {
    use Action as A;
    use InputMode as IM;
    use Direction as Dir;
    use actions::SearchDirection as SDir;
    use actions::SearchOption as SOpt;

    let mut old_keymap = mi.get_mode_keybinds();
    let s = |string: &str| string.to_string();

    // Find a keybinding to get back to "Normal" input mode. In this case we prefer '\n' over other
    // choices. Do it here before we dedupe the keymap below!
    let to_normal_keys = action_key(&old_keymap, &[TO_NORMAL]);
    let to_normal_key = if to_normal_keys.contains(&KeyWithModifier::new(BareKey::Enter)) {
        vec![KeyWithModifier::new(BareKey::Enter)]
    } else {
        // Yield `vec![key]` if `to_normal_keys` has at least one key, or an empty vec otherwise.
        to_normal_keys.into_iter().take(1).collect()
    };

    // Sort and deduplicate the keybindings first. We sort after the `Key`s, and deduplicate by
    // their `Action` vectors. An unstable sort is fine here because if the user maps anything to
    // the same key again, anything will happen...
    old_keymap.sort_unstable_by(|(keya, _), (keyb, _)| keya.partial_cmp(keyb).unwrap());

    let mut known_actions: Vec<Vec<Action>> = vec![];
    let mut km = vec![];
    for (key, acvec) in old_keymap {
        if known_actions.contains(&acvec) {
            // This action is known already
            continue;
        } else {
            known_actions.push(acvec.to_vec());
            km.push((key, acvec));
        }
    }

    if mi.mode == IM::Pane { vec![
        (s("New"), s("New"), single_action_key(&km, &[A::NewPane(None, None), TO_NORMAL])),
        (s("Change Focus"), s("Move"),
            action_key_group(&km, &[&[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
                &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseFocus, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode(IM::RenamePane), A::PaneNameInput(vec![0])])),
        (s("Toggle Fullscreen"), s("Fullscreen"), single_action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("Toggle Floating"), s("Floating"),
            single_action_key(&km, &[A::ToggleFloatingPanes, TO_NORMAL])),
        (s("Toggle Embed"), s("Embed"), single_action_key(&km, &[A::TogglePaneEmbedOrFloating, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Tab {
        // With the default bindings, "Move focus" for tabs is tricky: It binds all the arrow keys
        // to moving tabs focus (left/up go left, right/down go right). Since we sort the keys
        // above and then dedpulicate based on the actions, we will end up with LeftArrow for
        // "left" and DownArrow for "right". What we really expect is to see LeftArrow and
        // RightArrow.
        // FIXME: So for lack of a better idea we just check this case manually here.
        let old_keymap = mi.get_mode_keybinds();
        let focus_keys_full: Vec<KeyWithModifier> = action_key_group(&old_keymap,
            &[&[A::GoToPreviousTab], &[A::GoToNextTab]]);
        let focus_keys = if focus_keys_full.contains(&KeyWithModifier::new(BareKey::Left))
            && focus_keys_full.contains(&KeyWithModifier::new(BareKey::Right)) {
            vec![KeyWithModifier::new(BareKey::Left), KeyWithModifier::new(BareKey::Right)]
        } else {
            action_key_group(&km, &[&[A::GoToPreviousTab], &[A::GoToNextTab]])
        };

        vec![
        (s("New"), s("New"), single_action_key(&km, &[A::NewTab(None, vec![], None, None, None), TO_NORMAL])),
        (s("Change focus"), s("Move"), focus_keys),
        (s("Close"), s("Close"), single_action_key(&km, &[A::CloseTab, TO_NORMAL])),
        (s("Rename"), s("Rename"),
            single_action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Sync"), s("Sync"), single_action_key(&km, &[A::ToggleActiveSyncTab, TO_NORMAL])),
        (s("Break pane to new tab"), s("Break out"), single_action_key(&km, &[A::BreakPane, TO_NORMAL])),
        (s("Break pane left/right"), s("Break"), action_key_group(&km, &[
            &[Action::BreakPaneLeft, TO_NORMAL],
            &[Action::BreakPaneRight, TO_NORMAL],
        ])),
        (s("Toggle"), s("Toggle"), single_action_key(&km, &[A::ToggleTab])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Resize { vec![
        (s("Increase/Decrease size"), s("Increase/Decrease"),
            action_key_group(&km, &[
                &[A::Resize(Resize::Increase, None)],
                &[A::Resize(Resize::Decrease, None)]
            ])),
        (s("Increase to"), s("Increase"), action_key_group(&km, &[
            &[A::Resize(Resize::Increase, Some(Dir::Left))],
            &[A::Resize(Resize::Increase, Some(Dir::Down))],
            &[A::Resize(Resize::Increase, Some(Dir::Up))],
            &[A::Resize(Resize::Increase, Some(Dir::Right))]
            ])),
        (s("Decrease from"), s("Decrease"), action_key_group(&km, &[
            &[A::Resize(Resize::Decrease, Some(Dir::Left))],
            &[A::Resize(Resize::Decrease, Some(Dir::Down))],
            &[A::Resize(Resize::Decrease, Some(Dir::Up))],
            &[A::Resize(Resize::Decrease, Some(Dir::Right))]
            ])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Move { vec![
        (s("Switch Location"), s("Move"), action_key_group(&km, &[
            &[Action::MovePane(Some(Dir::Left))], &[Action::MovePane(Some(Dir::Down))],
            &[Action::MovePane(Some(Dir::Up))], &[Action::MovePane(Some(Dir::Right))]])),
    ]} else if mi.mode == IM::Scroll { vec![
        (s("Enter search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Edit scrollback in default editor"), s("Edit"),
            single_action_key(&km, &[Action::EditScrollback, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::EnterSearch { vec![
        (s("When done"), s("Done"), action_key(&km, &[A::SwitchToMode(IM::Search)])),
        (s("Cancel"), s("Cancel"),
            action_key(&km, &[A::SearchInput(vec![27]), A::SwitchToMode(IM::Scroll)])),
    ]} else if mi.mode == IM::Search { vec![
        (s("Enter Search term"), s("Search"),
            action_key(&km, &[A::SwitchToMode(IM::EnterSearch), A::SearchInput(vec![0])])),
        (s("Scroll"), s("Scroll"),
            action_key_group(&km, &[&[Action::ScrollDown], &[Action::ScrollUp]])),
        (s("Scroll page"), s("Scroll"),
            action_key_group(&km, &[&[Action::PageScrollDown], &[Action::PageScrollUp]])),
        (s("Scroll half page"), s("Scroll"),
            action_key_group(&km, &[&[Action::HalfPageScrollDown], &[Action::HalfPageScrollUp]])),
        (s("Search down"), s("Down"), action_key(&km, &[A::Search(SDir::Down)])),
        (s("Search up"), s("Up"), action_key(&km, &[A::Search(SDir::Up)])),
        (s("Case sensitive"), s("Case"),
            action_key(&km, &[A::SearchToggleOption(SOpt::CaseSensitivity)])),
        (s("Wrap"), s("Wrap"),
            action_key(&km, &[A::SearchToggleOption(SOpt::Wrap)])),
        (s("Whole words"), s("Whole"),
            action_key(&km, &[A::SearchToggleOption(SOpt::WholeWord)])),
    ]} else if mi.mode == IM::Session { vec![
        (s("Detach"), s("Detach"), action_key(&km, &[Action::Detach])),
        (s("Session Manager"), s("Manager"), session_manager_key(&km)),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if mi.mode == IM::Tmux { vec![
        (s("Move focus"), s("Move"), action_key_group(&km, &[
            &[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
            &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
        (s("Split down"), s("Down"), action_key(&km, &[A::NewPane(Some(Dir::Down), None), TO_NORMAL])),
        (s("Split right"), s("Right"), action_key(&km, &[A::NewPane(Some(Dir::Right), None), TO_NORMAL])),
        (s("Fullscreen"), s("Fullscreen"), action_key(&km, &[A::ToggleFocusFullscreen, TO_NORMAL])),
        (s("New tab"), s("New"), action_key(&km, &[A::NewTab(None, vec![], None, None, None), TO_NORMAL])),
        (s("Rename tab"), s("Rename"),
            action_key(&km, &[A::SwitchToMode(IM::RenameTab), A::TabNameInput(vec![0])])),
        (s("Previous Tab"), s("Previous"), action_key(&km, &[A::GoToPreviousTab, TO_NORMAL])),
        (s("Next Tab"), s("Next"), action_key(&km, &[A::GoToNextTab, TO_NORMAL])),
        (s("Select pane"), s("Select"), to_normal_key),
    ]} else if matches!(mi.mode, IM::RenamePane | IM::RenameTab) { vec![
        (s("When done"), s("Done"), to_normal_key),
        (s("Select pane"), s("Select"), action_key_group(&km, &[
            &[A::MoveFocus(Dir::Left)], &[A::MoveFocus(Dir::Down)],
            &[A::MoveFocus(Dir::Up)], &[A::MoveFocus(Dir::Right)]])),
    ]} else { vec![] }
}

fn shortened_shortcut_list_nonstandard_mode(help: &ModeInfo) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (_, short, keys) in keys_and_hints.into_iter() {
        line_part.append(&add_shortcut(help, &short, &keys.to_vec(), false));
    }
    line_part
}

fn shortened_shortcut_list(help: &ModeInfo) -> LinePart {
    match help.mode {
        InputMode::Normal => LinePart::default(),
        InputMode::Locked => LinePart::default(),
        _ => shortened_shortcut_list_nonstandard_mode(help),
    }
}

fn best_effort_shortcut_list_nonstandard_mode(help: &ModeInfo, max_len: usize) -> LinePart {
    let mut line_part = LinePart::default();
    let keys_and_hints = get_keys_and_hints(help);

    for (_, short, keys) in keys_and_hints.into_iter() {
        let shortcut = add_shortcut(help, &short, &keys.to_vec(), false);
        if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
            line_part.part = format!("{}{}", line_part.part, MORE_MSG);
            line_part.len += MORE_MSG.chars().count();
            break;
        } else {
            line_part.append(&shortcut);
        }
    }
    line_part
}

fn best_effort_shortcut_list(help: &ModeInfo, max_len: usize) -> LinePart {
    match help.mode {
        InputMode::Normal => {
            let line_part = LinePart::default();
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        },
        InputMode::Locked => {
            let line_part = locked_interface_indication(help.style.colors);
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        },
        _ => best_effort_shortcut_list_nonstandard_mode(help, max_len),
    }
}

pub fn single_action_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &[Action],
) -> Vec<KeyWithModifier> {
    let mut matching = keymap
        .iter()
        .find_map(|(key, acvec)| {
            if acvec.iter().next() == action.iter().next() {
                Some(key.clone())
            } else {
                None
            }
        });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}

fn locked_interface_indication(palette: Palette) -> LinePart {
    let locked_text = " -- INTERFACE LOCKED -- ";
    let locked_text_len = locked_text.chars().count();
    let text_color = palette_match!(match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    });
    let locked_styled_text = Style::new().fg(text_color).bold().paint(locked_text);
    LinePart {
        part: locked_styled_text.to_string(),
        len: locked_text_len,
    }
}

pub fn session_manager_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
) -> Vec<KeyWithModifier> {
    let mut matching = keymap
        .iter()
        .find_map(|(key, acvec)| {
            let has_match = acvec
                .iter()
                .find(|a| a.launches_plugin("session-manager"))
                .is_some();
            if has_match {
                Some(key.clone())
            } else {
                None
            }
        });
    if let Some(matching) = matching.take() {
        vec![matching]
    } else {
        vec![]
    }
}
