use ansi_term::ANSIStrings;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

use crate::color_elements;
use crate::{action_key, get_common_modifier, TO_NORMAL};
use crate::{ColoredElements, LinePart};

struct KeyShortcut {
    mode: KeyMode,
    action: KeyAction,
    key: Option<Key>,
}

#[derive(PartialEq)]
enum KeyAction {
    Lock,
    Pane,
    Tab,
    Resize,
    Search,
    Quit,
    Session,
    Move,
    Tmux,
}

enum KeyMode {
    Unselected,
    UnselectedAlternate,
    Selected,
    Disabled,
}

impl KeyShortcut {
    pub fn new(mode: KeyMode, action: KeyAction, key: Option<Key>) -> Self {
        KeyShortcut { mode, action, key }
    }

    pub fn full_text(&self) -> String {
        match self.action {
            KeyAction::Lock => String::from("LOCK"),
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
    pub fn letter_shortcut(&self, with_prefix: bool) -> String {
        let key = match self.key {
            Some(k) => k,
            None => return String::from("?"),
        };
        if with_prefix {
            format!("{}", key)
        } else {
            match key {
                Key::F(c) => format!("{}", c),
                Key::Ctrl(c) => format!("{}", c),
                Key::Char(_) => format!("{}", key),
                Key::Alt(c) => format!("{}", c),
                _ => String::from("??"),
            }
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
    shared_super: bool,
    first_tile: bool,
) -> LinePart {
    let key_hint = key.full_text();
    let key_binding = match (&key.mode, &key.key) {
        (KeyMode::Disabled, None) => "".to_string(),
        (_, None) => return LinePart::default(),
        (_, Some(_)) => key.letter_shortcut(!shared_super),
    };

    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let start_separator = if !shared_super && first_tile {
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
    shared_super: bool,
    first_tile: bool,
) -> LinePart {
    let key_binding = match (&key.mode, &key.key) {
        (KeyMode::Disabled, None) => "".to_string(),
        (_, None) => return LinePart::default(),
        (_, Some(_)) => key.letter_shortcut(!shared_super),
    };

    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let start_separator = if !shared_super && first_tile {
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
) -> LinePart {
    // Print full-width hints
    let mut line_part = superkey(palette, separator, mode_info);
    let shared_super = line_part.len > 0;
    for ctrl_key in keys {
        let line_empty = line_part.len == 0;
        let key = long_mode_shortcut(ctrl_key, palette, separator, shared_super, line_empty);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }

    // Full-width doesn't fit, try shortened hints (just keybindings, no meanings/actions)
    line_part = superkey(palette, separator, mode_info);
    let shared_super = line_part.len > 0;
    for ctrl_key in keys {
        let line_empty = line_part.len == 0;
        let key = short_mode_shortcut(ctrl_key, palette, separator, shared_super, line_empty);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }

    // Shortened doesn't fit, print nothing
    line_part = LinePart::default();
    line_part
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
pub fn mode_switch_keys(mode_info: &ModeInfo) -> Vec<Key> {
    mode_info
        .get_mode_keybinds()
        .iter()
        .filter_map(|(key, vac)| match vac.first() {
            // No actions defined, ignore
            None => None,
            Some(vac) => {
                // We ignore certain "default" keybindings that switch back to normal InputMode.
                // These include: ' ', '\n', 'Esc'
                if matches!(key, Key::Char(' ') | Key::Char('\n') | Key::Esc) {
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
                        | InputMode::Session => Some(*key),
                        _ => None,
                    };
                }
                if let actions::Action::Quit = vac {
                    return Some(*key);
                }
                // Not a `SwitchToMode` or `Quit` action, ignore
                None
            },
        })
        .collect()
}

pub fn superkey(palette: ColoredElements, separator: &str, mode_info: &ModeInfo) -> LinePart {
    // Find a common modifier if any
    let prefix_text = match get_common_modifier(mode_switch_keys(mode_info).iter().collect()) {
        Some(text) => {
            if mode_info.capabilities.arrow_fonts {
                // Add extra space in simplified ui
                format!(" {} + ", text)
            } else {
                format!(" {} +", text)
            }
        },
        _ => return LinePart::default(),
    };

    let prefix = palette.superkey_prefix.paint(&prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix, suffix_separator]).to_string(),
        len: prefix_text.chars().count() + separator.chars().count(),
    }
}

pub fn to_char(kv: Vec<Key>) -> Option<Key> {
    let key = kv
        .iter()
        .filter(|key| {
            // These are general "keybindings" to get back to normal, they aren't interesting here.
            !matches!(key, Key::Char('\n') | Key::Char(' ') | Key::Esc)
        })
        .collect::<Vec<&Key>>()
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
        InputMode::Normal | InputMode::Prompt | InputMode::Tmux | InputMode::Passthrough => {
            return None
        },
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

fn passthrough_mode(palette: &ColoredElements, max_len: usize, separator: &str) -> LinePart {
    let colors = palette.selected;
    let text = " PASSTHROUGH ";
    let line_part = LinePart {
        part: ANSIStrings(&[
            colors.styled_text.paint(text),
            colors.suffix_separator.paint(separator),
        ])
        .to_string(),
        len: text.chars().count() + separator.chars().count(),
    };

    if line_part.len > max_len {
        LinePart::default()
    } else {
        line_part
    }
}

pub fn first_line(help: &ModeInfo, max_len: usize, separator: &str) -> LinePart {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let binds = &help.get_mode_keybinds();

    if help.mode == InputMode::Passthrough {
        return passthrough_mode(&colored_elements, max_len, separator);
    }

    // Unselect all by default
    let mut default_keys = vec![
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Lock,
            to_char(action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Locked)],
            )),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Pane,
            to_char(action_key(binds, &[Action::SwitchToMode(InputMode::Pane)])),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Tab,
            to_char(action_key(binds, &[Action::SwitchToMode(InputMode::Tab)])),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Resize,
            to_char(action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Resize)],
            )),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Move,
            to_char(action_key(binds, &[Action::SwitchToMode(InputMode::Move)])),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Search,
            to_char(action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Scroll)],
            )),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Session,
            to_char(action_key(
                binds,
                &[Action::SwitchToMode(InputMode::Session)],
            )),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Quit,
            to_char(action_key(binds, &[Action::Quit])),
        ),
    ];

    if let Some(key_shortcut) = get_key_shortcut_for_mode(&mut default_keys, &help.mode) {
        key_shortcut.mode = KeyMode::Selected;
        key_shortcut.key = to_char(action_key(binds, &[TO_NORMAL]));
    }

    // In locked mode we must disable all other mode keybindings
    if help.mode == InputMode::Locked {
        for key in default_keys.iter_mut().skip(1) {
            key.mode = KeyMode::Disabled;
        }
    }

    if help.mode == InputMode::Tmux {
        // Tmux tile is hidden by default
        default_keys.push(KeyShortcut::new(
            KeyMode::Selected,
            KeyAction::Tmux,
            to_char(action_key(binds, &[TO_NORMAL])),
        ));
    }

    key_indicators(max_len, &default_keys, colored_elements, separator, help)
}

#[cfg(test)]
/// Unit tests.
///
/// Note that we cheat a little here, because the number of things one may want to test is endless,
/// and creating a Mockup of [`ModeInfo`] by hand for all these testcases is nothing less than
/// torture. Hence, we test the most atomic units thoroughly ([`long_mode_shortcut`] and
/// [`short_mode_shortcut`]) and then test the public API ([`first_line`]) to ensure correct
/// operation.
mod tests {
    use super::*;

    fn colored_elements() -> ColoredElements {
        let palette = Palette::default();
        color_elements(palette, false)
    }

    // Strip style information from `LinePart` and return a raw String instead
    fn unstyle(line_part: LinePart) -> String {
        let string = line_part.to_string();

        let re = regex::Regex::new(r"\x1b\[[0-9;]*m").unwrap();
        let string = re.replace_all(&string, "".to_string());

        string.to_string()
    }

    #[test]
    fn long_mode_shortcut_selected_with_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    // Displayed like selected(alternate), but different styling
    fn long_mode_shortcut_unselected_with_binding() {
        let key = KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Session,
            Some(Key::Char('0')),
        );
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    // Treat exactly like "unselected" variant
    fn long_mode_shortcut_unselected_alternate_with_binding() {
        let key = KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Session,
            Some(Key::Char('0')),
        );
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    // KeyShortcuts without binding are only displayed when "disabled" (for locked mode indications)
    fn long_mode_shortcut_selected_without_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, None);
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "".to_string());
    }

    #[test]
    // First tile doesn't print a starting separator
    fn long_mode_shortcut_selected_with_binding_first_tile() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, true);
        let ret = unstyle(ret);

        assert_eq!(ret, " <0> SESSION +".to_string());
    }

    #[test]
    // Modifier is the superkey, mustn't appear in angled brackets
    fn long_mode_shortcut_selected_with_ctrl_binding_shared_superkey() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Ctrl('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", true, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    // Modifier must be in the angled brackets
    fn long_mode_shortcut_selected_with_ctrl_binding_no_shared_superkey() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Ctrl('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <Ctrl+0> SESSION +".to_string());
    }

    #[test]
    // Must be displayed as usual, but it is styled to be greyed out which we don't test here
    fn long_mode_shortcut_disabled_with_binding() {
        let key = KeyShortcut::new(KeyMode::Disabled, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    // Must be displayed but without keybinding
    fn long_mode_shortcut_disabled_without_binding() {
        let key = KeyShortcut::new(KeyMode::Disabled, KeyAction::Session, None);
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <> SESSION +".to_string());
    }

    #[test]
    // Test all at once
    // Note that when "shared_super" is true, the tile **cannot** be the first on the line, so we
    // ignore **first** here.
    fn long_mode_shortcut_selected_with_ctrl_binding_and_shared_super_and_first_tile() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Ctrl('0')));
        let color = colored_elements();

        let ret = long_mode_shortcut(&key, color, "+", true, true);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ <0> SESSION +".to_string());
    }

    #[test]
    fn short_mode_shortcut_selected_with_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_selected_with_ctrl_binding_no_shared_super() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Ctrl('0')));
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ Ctrl+0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_selected_with_ctrl_binding_shared_super() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Ctrl('0')));
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", true, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_selected_with_binding_first_tile() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, true);
        let ret = unstyle(ret);

        assert_eq!(ret, " 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_unselected_with_binding() {
        let key = KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Session,
            Some(Key::Char('0')),
        );
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_unselected_alternate_with_binding() {
        let key = KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Session,
            Some(Key::Char('0')),
        );
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_disabled_with_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, Some(Key::Char('0')));
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "+ 0 +".to_string());
    }

    #[test]
    fn short_mode_shortcut_selected_without_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, None);
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "".to_string());
    }

    #[test]
    fn short_mode_shortcut_unselected_without_binding() {
        let key = KeyShortcut::new(KeyMode::Unselected, KeyAction::Session, None);
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "".to_string());
    }

    #[test]
    fn short_mode_shortcut_unselected_alternate_without_binding() {
        let key = KeyShortcut::new(KeyMode::UnselectedAlternate, KeyAction::Session, None);
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "".to_string());
    }

    #[test]
    fn short_mode_shortcut_disabled_without_binding() {
        let key = KeyShortcut::new(KeyMode::Selected, KeyAction::Session, None);
        let color = colored_elements();

        let ret = short_mode_shortcut(&key, color, "+", false, false);
        let ret = unstyle(ret);

        assert_eq!(ret, "".to_string());
    }

    #[test]
    // Observe: Modes missing in between aren't displayed!
    fn first_line_default_layout_shared_super() {
        #[rustfmt::skip]
        let mode_info = ModeInfo{
            mode: InputMode::Normal,
            keybinds : vec![
                (InputMode::Normal, vec![
                    (Key::Ctrl('a'), vec![Action::SwitchToMode(InputMode::Pane)]),
                    (Key::Ctrl('b'), vec![Action::SwitchToMode(InputMode::Resize)]),
                    (Key::Ctrl('c'), vec![Action::SwitchToMode(InputMode::Move)]),
                ]),
            ],
            ..ModeInfo::default()
        };

        let ret = first_line(&mode_info, 500, ">");
        let ret = unstyle(ret);

        assert_eq!(
            ret,
            " Ctrl + >> <a> PANE >> <b> RESIZE >> <c> MOVE >".to_string()
        );
    }

    #[test]
    fn first_line_default_layout_no_shared_super() {
        #[rustfmt::skip]
        let mode_info = ModeInfo{
            mode: InputMode::Normal,
            keybinds : vec![
                (InputMode::Normal, vec![
                    (Key::Ctrl('a'), vec![Action::SwitchToMode(InputMode::Pane)]),
                    (Key::Ctrl('b'), vec![Action::SwitchToMode(InputMode::Resize)]),
                    (Key::Char('c'), vec![Action::SwitchToMode(InputMode::Move)]),
                ]),
            ],
            ..ModeInfo::default()
        };

        let ret = first_line(&mode_info, 500, ">");
        let ret = unstyle(ret);

        assert_eq!(
            ret,
            " <Ctrl+a> PANE >> <Ctrl+b> RESIZE >> <c> MOVE >".to_string()
        );
    }

    #[test]
    fn first_line_default_layout_unprintables() {
        #[rustfmt::skip]
        let mode_info = ModeInfo{
            mode: InputMode::Normal,
            keybinds : vec![
                (InputMode::Normal, vec![
                    (Key::Ctrl('a'), vec![Action::SwitchToMode(InputMode::Locked)]),
                    (Key::Backspace, vec![Action::SwitchToMode(InputMode::Pane)]),
                    (Key::Char('\n'), vec![Action::SwitchToMode(InputMode::Tab)]),
                    (Key::Char('\t'), vec![Action::SwitchToMode(InputMode::Resize)]),
                    (Key::Left, vec![Action::SwitchToMode(InputMode::Move)]),
                ]),
            ],
            ..ModeInfo::default()
        };

        let ret = first_line(&mode_info, 500, ">");
        let ret = unstyle(ret);

        assert_eq!(
            ret,
            " <Ctrl+a> LOCK >> <BACKSPACE> PANE >> <ENTER> TAB >> <TAB> RESIZE >> <←> MOVE >"
                .to_string()
        );
    }

    #[test]
    fn first_line_short_layout_shared_super() {
        #[rustfmt::skip]
        let mode_info = ModeInfo{
            mode: InputMode::Normal,
            keybinds : vec![
                (InputMode::Normal, vec![
                    (Key::Ctrl('a'), vec![Action::SwitchToMode(InputMode::Locked)]),
                    (Key::Ctrl('b'), vec![Action::SwitchToMode(InputMode::Pane)]),
                    (Key::Ctrl('c'), vec![Action::SwitchToMode(InputMode::Tab)]),
                    (Key::Ctrl('d'), vec![Action::SwitchToMode(InputMode::Resize)]),
                    (Key::Ctrl('e'), vec![Action::SwitchToMode(InputMode::Move)]),
                ]),
            ],
            ..ModeInfo::default()
        };

        let ret = first_line(&mode_info, 50, ">");
        let ret = unstyle(ret);

        assert_eq!(ret, " Ctrl + >> a >> b >> c >> d >> e >".to_string());
    }

    #[test]
    fn first_line_short_simplified_ui_shared_super() {
        #[rustfmt::skip]
        let mode_info = ModeInfo{
            mode: InputMode::Normal,
            keybinds : vec![
                (InputMode::Normal, vec![
                    (Key::Ctrl('a'), vec![Action::SwitchToMode(InputMode::Pane)]),
                    (Key::Ctrl('b'), vec![Action::SwitchToMode(InputMode::Resize)]),
                    (Key::Ctrl('c'), vec![Action::SwitchToMode(InputMode::Move)]),
                ]),
            ],
            ..ModeInfo::default()
        };

        let ret = first_line(&mode_info, 30, "");
        let ret = unstyle(ret);

        assert_eq!(ret, " Ctrl +  a  b  c ".to_string());
    }
}
