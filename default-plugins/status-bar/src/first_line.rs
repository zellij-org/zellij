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

fn unselected_mode_shortcut(
    letter: &str,
    text: &str,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let prefix_separator = palette.unselected_prefix_separator.paint(separator);
    let char_left_separator = palette.unselected_char_left_separator.paint(" <");
    let char_shortcut = palette.unselected_char_shortcut.paint(letter.to_string());
    let char_right_separator = palette.unselected_char_right_separator.paint(">");
    let styled_text = palette.unselected_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.unselected_suffix_separator.paint(separator);
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
        len: text.chars().count() + 6 + letter.len(), // 2 for the arrows, 3 for the char separators, 1 for the text padding
    }
}

fn unselected_alternate_mode_shortcut(
    letter: &str,
    text: &str,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let prefix_separator = palette
        .unselected_alternate_prefix_separator
        .paint(separator);
    let char_left_separator = palette.unselected_alternate_char_left_separator.paint(" <");
    let char_shortcut = palette
        .unselected_alternate_char_shortcut
        .paint(letter.to_string());
    let char_right_separator = palette.unselected_alternate_char_right_separator.paint(">");
    let styled_text = palette
        .unselected_alternate_styled_text
        .paint(format!("{} ", text));
    let suffix_separator = palette
        .unselected_alternate_suffix_separator
        .paint(separator);
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
        len: text.chars().count() + 6 + letter.len(), // 2 for the arrows, 3 for the char separators, 1 for the text padding
    }
}

fn selected_mode_shortcut(
    letter: &str,
    text: &str,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let prefix_separator = palette.selected_prefix_separator.paint(separator);
    let char_left_separator = palette.selected_char_left_separator.paint(" <".to_string());
    let char_shortcut = palette.selected_char_shortcut.paint(letter.to_string());
    let char_right_separator = palette.selected_char_right_separator.paint(">".to_string());
    let styled_text = palette.selected_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.selected_suffix_separator.paint(separator);
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
        len: text.chars().count() + 6 + letter.len(), // 2 for the arrows, 3 for the char separators, 1 for the text padding
    }
}

fn disabled_mode_shortcut(text: &str, palette: ColoredElements, separator: &str) -> LinePart {
    let prefix_separator = palette.disabled_prefix_separator.paint(separator);
    let styled_text = palette.disabled_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.disabled_suffix_separator.paint(separator);
    LinePart {
        part: format!("{}{}{}", prefix_separator, styled_text, suffix_separator),
        len: text.chars().count() + 2 + 1, // 2 for the arrows, 1 for the padding in the end
    }
}

fn selected_mode_shortcut_single_letter(
    letter: char,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = palette
        .selected_single_letter_prefix_separator
        .paint(separator);
    let char_shortcut = palette
        .selected_single_letter_char_shortcut
        .paint(char_shortcut_text);
    let suffix_separator = palette
        .selected_single_letter_suffix_separator
        .paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn unselected_mode_shortcut_single_letter(
    letter: char,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = palette
        .unselected_single_letter_prefix_separator
        .paint(separator);
    let char_shortcut = palette
        .unselected_single_letter_char_shortcut
        .paint(char_shortcut_text);
    let suffix_separator = palette
        .unselected_single_letter_suffix_separator
        .paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn unselected_alternate_mode_shortcut_single_letter(
    letter: char,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = palette
        .unselected_alternate_single_letter_prefix_separator
        .paint(separator);
    let char_shortcut = palette
        .unselected_alternate_single_letter_char_shortcut
        .paint(char_shortcut_text);
    let suffix_separator = palette
        .unselected_alternate_single_letter_suffix_separator
        .paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn full_ctrl_key(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
    shared_super: bool,
) -> LinePart {
    let full_text = key.full_text();
    let letter_shortcut = key.letter_shortcut(!shared_super);
    match key.mode {
        KeyMode::Unselected => unselected_mode_shortcut(
            &letter_shortcut,
            &format!(" {}", full_text),
            palette,
            separator,
        ),
        KeyMode::UnselectedAlternate => unselected_alternate_mode_shortcut(
            &letter_shortcut,
            &format!(" {}", full_text),
            palette,
            separator,
        ),
        KeyMode::Selected => selected_mode_shortcut(
            &letter_shortcut,
            &format!(" {}", full_text),
            palette,
            separator,
        ),
        KeyMode::Disabled => disabled_mode_shortcut(
            &format!(" <{}> {}", letter_shortcut, full_text),
            palette,
            separator,
        ),
    }
}

fn single_letter_ctrl_key(
    key: &KeyShortcut,
    palette: ColoredElements,
    separator: &str,
) -> LinePart {
    let letter_shortcut = key.letter_shortcut(false).chars().next().unwrap();
    match key.mode {
        KeyMode::Unselected => {
            unselected_mode_shortcut_single_letter(letter_shortcut, palette, separator)
        },
        KeyMode::UnselectedAlternate => {
            unselected_alternate_mode_shortcut_single_letter(letter_shortcut, palette, separator)
        },
        KeyMode::Selected => {
            selected_mode_shortcut_single_letter(letter_shortcut, palette, separator)
        },
        KeyMode::Disabled => {
            disabled_mode_shortcut(&format!(" {}", letter_shortcut), palette, separator)
        },
    }
}

fn key_indicators(
    max_len: usize,
    keys: &[KeyShortcut],
    palette: ColoredElements,
    separator: &str,
    shared_super: bool,
) -> LinePart {
    // Print full-width hints
    let mut line_part = LinePart::default();
    for ctrl_key in keys {
        let key = full_ctrl_key(ctrl_key, palette, separator, shared_super);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }

    // Full-width doesn't fit, try shortened hints (just keybindings, no meanings/actions)
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = single_letter_ctrl_key(ctrl_key, palette, separator);
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

/// Return a Vector of tuples (Key, InputMode) where each "Key" is a shortcut to switch to
/// "InputMode".
pub fn mode_switch_keys(mode_info: &ModeInfo) -> Vec<(&Key, &InputMode)> {
    mode_info
        .keybinds
        .iter()
        .filter_map(|(key, vac)| match vac.first() {
            None => None,
            Some(vac) => {
                if let actions::Action::SwitchToMode(mode) = vac {
                    return Some((key, mode));
                }
                None
            },
        })
        .collect()
}

pub fn superkey(palette: ColoredElements, separator: &str, mode_info: &ModeInfo) -> LinePart {
    // Find a common modifier if any
    let mut prefix_text: &str = "";
    let mut new_prefix;
    for (key, _mode) in mode_switch_keys(mode_info).iter() {
        match key {
            Key::F(_) => new_prefix = " F",
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
        len: prefix_text.chars().count(),
    }
}

pub fn to_char(kv: Vec<Key>) -> Key {
    kv.into_iter()
        .filter(|key| {
            // These are general "keybindings" to get back to normal, they aren't interesting here.
            // The user will figure these out for himself if he configured no other.
            matches!(key, Key::Char('\n') | Key::Char(' ') | Key::Esc)
        })
        .collect::<Vec<Key>>()
        .into_iter()
        .next()
        .unwrap_or(Key::Char('?'))
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize, separator: &str, shared_super: bool) -> LinePart {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    let binds = &help.keybinds;
    // Unselect all by default
    let mut default_keys = [
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

    key_indicators(
        max_len,
        &default_keys,
        colored_elements,
        separator,
        shared_super,
    )
}
