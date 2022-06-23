use ansi_term::ANSIStrings;
use zellij_tile::prelude::*;

use crate::color_elements;
use crate::{ColoredElements, LinePart};

struct KeyShortcut {
    mode: KeyMode,
    action: KeyAction,
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
    pub fn new(mode: KeyMode, action: KeyAction) -> Self {
        KeyShortcut { mode, action } //, bind }
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
    pub fn letter_shortcut(&self) -> char {
        match self.action {
            KeyAction::Lock => 'g',
            KeyAction::Pane => 'p',
            KeyAction::Tab => 't',
            KeyAction::Resize => 'n',
            KeyAction::Search => 's',
            KeyAction::Quit => 'q',
            KeyAction::Session => 'o',
            KeyAction::Move => 'h',
        }
    }
}

fn unselected_mode_shortcut(
    letter: char,
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
        len: text.chars().count() + 7, // 2 for the arrows, 3 for the char separators, 1 for the character, 1 for the text padding
    }
}

fn unselected_alternate_mode_shortcut(
    letter: char,
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
        len: text.chars().count() + 7, // 2 for the arrows, 3 for the char separators, 1 for the character, 1 for the text padding
    }
}

fn selected_mode_shortcut(
    letter: char,
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
        len: text.chars().count() + 7, // 2 for the arrows, 3 for the char separators, 1 for the character, 1 for the text padding
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

fn full_ctrl_key(key: &KeyShortcut, palette: ColoredElements, separator: &str) -> LinePart {
    let full_text = key.full_text();
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        KeyMode::Unselected => unselected_mode_shortcut(
            letter_shortcut,
            &format!(" {}", full_text),
            palette,
            separator,
        ),
        KeyMode::UnselectedAlternate => unselected_alternate_mode_shortcut(
            letter_shortcut,
            &format!(" {}", full_text),
            palette,
            separator,
        ),
        KeyMode::Selected => selected_mode_shortcut(
            letter_shortcut,
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
    let letter_shortcut = key.letter_shortcut();
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
) -> LinePart {
    // Print full-width hints
    let mut line_part = LinePart::default();
    for ctrl_key in keys {
        let key = full_ctrl_key(ctrl_key, palette, separator);
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

pub fn superkey(palette: ColoredElements, separator: &str) -> LinePart {
    let prefix_text = if separator.is_empty() {
        " Ctrl + "
    } else {
        " Ctrl +"
    };
    let prefix = palette.superkey_prefix.paint(prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(separator);
    LinePart {
        part: ANSIStrings(&[prefix, suffix_separator]).to_string(),
        len: prefix_text.chars().count(),
    }
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize, separator: &str) -> LinePart {
    let supports_arrow_fonts = !help.capabilities.arrow_fonts;
    let colored_elements = color_elements(help.style.colors, !supports_arrow_fonts);
    // Unselect all by default
    let mut default_keys = [
        KeyShortcut::new(KeyMode::Unselected, KeyAction::Lock),
        KeyShortcut::new(KeyMode::UnselectedAlternate, KeyAction::Pane),
        KeyShortcut::new(KeyMode::Unselected, KeyAction::Tab),
        KeyShortcut::new(KeyMode::UnselectedAlternate, KeyAction::Resize),
        KeyShortcut::new(KeyMode::Unselected, KeyAction::Move),
        KeyShortcut::new(KeyMode::UnselectedAlternate, KeyAction::Search),
        KeyShortcut::new(KeyMode::Unselected, KeyAction::Session),
        KeyShortcut::new(KeyMode::UnselectedAlternate, KeyAction::Quit),
    ];

    match &help.mode {
        InputMode::Normal | InputMode::Prompt | InputMode::Tmux => (),
        InputMode::Locked => {
            default_keys[0].mode = KeyMode::Selected;
            for key in default_keys.iter_mut().skip(1) {
                key.mode = KeyMode::Disabled;
            }
        },
        InputMode::Pane | InputMode::RenamePane => default_keys[1].mode = KeyMode::Selected,
        InputMode::Tab | InputMode::RenameTab => default_keys[2].mode = KeyMode::Selected,
        InputMode::Resize => default_keys[3].mode = KeyMode::Selected,
        InputMode::Move => default_keys[4].mode = KeyMode::Selected,
        InputMode::Scroll | InputMode::Search | InputMode::EnterSearch => {
            default_keys[5].mode = KeyMode::Selected
        },
        InputMode::Session => default_keys[6].mode = KeyMode::Selected,
    }

    key_indicators(max_len, &default_keys, colored_elements, separator)
}
