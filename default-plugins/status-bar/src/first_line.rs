use ansi_term::ANSIStrings;
use zellij_tile::prelude::actions::Action;
use zellij_tile::prelude::*;

use crate::color_elements;
use crate::{action_key, to_normal};
use crate::{ColoredElements, LinePart};

struct KeyShortcut {
    mode: KeyMode,
    action: KeyAction,
    key: Key,
}

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
    //pub fn new(mode: KeyMode, action: KeyAction, bind: KeyBind) -> Self {
    pub fn new(mode: KeyMode, action: KeyAction, key: Key) -> Self {
        KeyShortcut { mode, action, key } //, bind }
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
        if with_prefix {
            format!("{}", self.key)
        } else {
            match self.key {
                Key::F(c) => format!("{}", c),
                Key::Ctrl(c) => format!("{}", c),
                Key::Char(_) => format!("{}", self.key),
                Key::Alt(c) => format!("{}", c),
                _ => String::from("??"),
            }
        }
    }
}

fn long_tile(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    shared_super: bool,
) -> LinePart {
    let key_hint = key.full_text();
    let key_binding = key.letter_shortcut(!shared_super);
    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let prefix_separator = colors.prefix_separator.paint(separator);
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
        len: separator.chars().count()      // Separator
            + 2                             // " <"
            + key_binding.chars().count()   // Key binding
            + 2                             // "> "
            + key_hint.chars().count()      // Key hint (mode)
            + 1                             // " "
            + separator.chars().count(), // Separator
    }
}

fn short_tile(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    shared_super: bool,
) -> LinePart {
    let key_binding = key.letter_shortcut(!shared_super);
    let colors = match key.mode {
        KeyMode::Unselected => palette.unselected,
        KeyMode::UnselectedAlternate => palette.unselected_alternate,
        KeyMode::Selected => palette.selected,
        KeyMode::Disabled => palette.disabled,
    };
    let prefix_separator = colors.prefix_separator.paint(separator);
    let char_shortcut = colors
        .char_shortcut
        .paint(format!(" {} ", key_binding.to_string()));
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
        let key = long_tile(ctrl_key, palette, separator, shared_super);
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
        let key = short_tile(ctrl_key, palette, separator, shared_super);
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
pub fn mode_switch_keys(mode_info: &ModeInfo) -> Vec<&Key> {
    mode_info
        .keybinds
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
                        | InputMode::Session => Some(key),
                        _ => None,
                    };
                }
                if let actions::Action::Quit = vac {
                    return Some(key);
                }
                // Not a `SwitchToMode` or `Quit` action, ignore
                None
            },
        })
        .collect()
}

pub fn superkey(palette: ColoredElements, separator: &str, mode_info: &ModeInfo) -> LinePart {
    // Find a common modifier if any
    let mut prefix_text: &str = "";
    let mut new_prefix;
    for key in mode_switch_keys(mode_info).iter() {
        match key {
            Key::Ctrl(_) => new_prefix = " Ctrl +",
            Key::Alt(_) => new_prefix = " Alt +",
            _ => break,
        }
        if prefix_text.is_empty() {
            prefix_text = new_prefix;
        } else if prefix_text != new_prefix {
            // Prefix changed!
            prefix_text = "";
            break;
        }
    }

    let prefix = palette.superkey_prefix.paint(prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix, suffix_separator]).to_string(),
        len: prefix_text.chars().count() + separator.chars().count(),
    }
}

pub fn to_char(kv: Vec<Key>) -> Key {
    kv.into_iter()
        .filter(|key| {
            // These are general "keybindings" to get back to normal, they aren't interesting here.
            // The user will figure these out for himself if he configured no other.
            !matches!(key, Key::Char('\n') | Key::Char(' ') | Key::Esc)
        })
        .collect::<Vec<Key>>()
        .into_iter()
        .next()
        .unwrap_or(Key::Char('?'))
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize, separator: &str) -> LinePart {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let binds = &help.keybinds;
    // Unselect all by default
    let mut default_keys = vec![
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Lock,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Locked))),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Pane,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Pane))),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Tab,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Tab))),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Resize,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Resize))),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Move,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Move))),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Search,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Scroll))),
        ),
        KeyShortcut::new(
            KeyMode::Unselected,
            KeyAction::Session,
            to_char(action_key!(binds, Action::SwitchToMode(InputMode::Session))),
        ),
        KeyShortcut::new(
            KeyMode::UnselectedAlternate,
            KeyAction::Quit,
            to_char(action_key!(binds, Action::Quit)),
        ),
    ];

    let mode_index = match &help.mode {
        InputMode::Normal | InputMode::Prompt | InputMode::Tmux => None,
        InputMode::Locked => {
            for key in default_keys.iter_mut().skip(1) {
                key.mode = KeyMode::Disabled;
            }
            Some(0)
        },
        InputMode::Pane | InputMode::RenamePane => Some(1),
        InputMode::Tab | InputMode::RenameTab => Some(2),
        InputMode::Resize => Some(3),
        InputMode::Move => Some(4),
        InputMode::Scroll | InputMode::Search | InputMode::EnterSearch => Some(5),
        InputMode::Session => Some(6),
    };
    if let Some(index) = mode_index {
        default_keys[index].mode = KeyMode::Selected;
        default_keys[index].key = to_char(action_key!(binds, to_normal!()));
    }

    if help.mode == InputMode::Tmux {
        // Tmux tile is hidden by default
        default_keys.push(KeyShortcut::new(
            KeyMode::Selected,
            KeyAction::Tmux,
            to_char(action_key!(binds, to_normal!())),
        ));
    }

    key_indicators(max_len, &default_keys, colored_elements, separator, help)
}
