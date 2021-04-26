use ansi_term::ANSIStrings;
use zellij_tile::prelude::*;

use crate::color_elements;
use crate::{ColoredElements, LinePart, ARROW_SEPARATOR};

struct CtrlKeyShortcut {
    mode: CtrlKeyMode,
    action: CtrlKeyAction,
}

impl CtrlKeyShortcut {
    pub fn new(mode: CtrlKeyMode, action: CtrlKeyAction) -> Self {
        CtrlKeyShortcut { mode, action }
    }
}

enum CtrlKeyAction {
    Lock,
    Pane,
    Tab,
    Resize,
    Scroll,
    Quit,
}

enum CtrlKeyMode {
    Unselected,
    Selected,
    Disabled,
}

impl CtrlKeyShortcut {
    pub fn full_text(&self) -> String {
        match self.action {
            CtrlKeyAction::Lock => String::from("LOCK"),
            CtrlKeyAction::Pane => String::from("PANE"),
            CtrlKeyAction::Tab => String::from("TAB"),
            CtrlKeyAction::Resize => String::from("RESIZE"),
            CtrlKeyAction::Scroll => String::from("SCROLL"),
            CtrlKeyAction::Quit => String::from("QUIT"),
        }
    }
    pub fn shortened_text(&self) -> String {
        match self.action {
            CtrlKeyAction::Lock => String::from("LOCK"),
            CtrlKeyAction::Pane => String::from("ane"),
            CtrlKeyAction::Tab => String::from("ab"),
            CtrlKeyAction::Resize => String::from("esize"),
            CtrlKeyAction::Scroll => String::from("croll"),
            CtrlKeyAction::Quit => String::from("uit"),
        }
    }
    pub fn letter_shortcut(&self) -> char {
        match self.action {
            CtrlKeyAction::Lock => 'g',
            CtrlKeyAction::Pane => 'p',
            CtrlKeyAction::Tab => 't',
            CtrlKeyAction::Resize => 'r',
            CtrlKeyAction::Scroll => 's',
            CtrlKeyAction::Quit => 'q',
        }
    }
}

fn unselected_mode_shortcut(letter: char, text: &str, palette: ColoredElements) -> LinePart {
    let prefix_separator = palette.unselected_prefix_separator.paint(ARROW_SEPARATOR);
    let char_left_separator = palette.unselected_char_left_separator.paint(" <");
    let char_shortcut = palette.unselected_char_shortcut.paint(letter.to_string());
    let char_right_separator = palette.unselected_char_right_separator.paint(">");
    let styled_text = palette.unselected_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.unselected_suffix_separator.paint(ARROW_SEPARATOR);
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

fn selected_mode_shortcut(letter: char, text: &str, palette: ColoredElements) -> LinePart {
    let prefix_separator = palette.selected_prefix_separator.paint(ARROW_SEPARATOR);
    let char_left_separator = palette.selected_char_left_separator.paint(" <".to_string());
    let char_shortcut = palette.selected_char_shortcut.paint(format!("{}", letter));
    let char_right_separator = palette.selected_char_right_separator.paint(">".to_string());
    let styled_text = palette.selected_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.selected_suffix_separator.paint(ARROW_SEPARATOR);
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

fn disabled_mode_shortcut(text: &str, palette: ColoredElements) -> LinePart {
    let prefix_separator = palette.disabled_prefix_separator.paint(ARROW_SEPARATOR);
    let styled_text = palette.disabled_styled_text.paint(format!("{} ", text));
    let suffix_separator = palette.disabled_suffix_separator.paint(ARROW_SEPARATOR);
    LinePart {
        part: format!("{}{}{}", prefix_separator, styled_text, suffix_separator),
        len: text.chars().count() + 2 + 1, // 2 for the arrows, 1 for the padding in the end
    }
}

fn selected_mode_shortcut_single_letter(letter: char, palette: ColoredElements) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = palette
        .selected_single_letter_prefix_separator
        .paint(ARROW_SEPARATOR);
    let char_shortcut = palette
        .selected_single_letter_char_shortcut
        .paint(char_shortcut_text);
    let suffix_separator = palette
        .selected_single_letter_suffix_separator
        .paint(ARROW_SEPARATOR);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn unselected_mode_shortcut_single_letter(letter: char, palette: ColoredElements) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = palette
        .unselected_single_letter_prefix_separator
        .paint(ARROW_SEPARATOR);
    let char_shortcut = palette
        .unselected_single_letter_char_shortcut
        .paint(char_shortcut_text);
    let suffix_separator = palette
        .unselected_single_letter_suffix_separator
        .paint(ARROW_SEPARATOR);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn full_ctrl_key(key: &CtrlKeyShortcut, palette: ColoredElements) -> LinePart {
    let full_text = key.full_text();
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        CtrlKeyMode::Unselected => {
            unselected_mode_shortcut(letter_shortcut, &format!(" {}", full_text), palette)
        }
        CtrlKeyMode::Selected => {
            selected_mode_shortcut(letter_shortcut, &format!(" {}", full_text), palette)
        }
        CtrlKeyMode::Disabled => {
            disabled_mode_shortcut(&format!(" <{}> {}", letter_shortcut, full_text), palette)
        }
    }
}

fn shortened_ctrl_key(key: &CtrlKeyShortcut, palette: ColoredElements) -> LinePart {
    let shortened_text = key.shortened_text();
    let letter_shortcut = key.letter_shortcut();
    let shortened_text = match key.action {
        CtrlKeyAction::Lock => format!(" {}", shortened_text),
        _ => shortened_text,
    };
    match key.mode {
        CtrlKeyMode::Unselected => {
            unselected_mode_shortcut(letter_shortcut, &shortened_text, palette)
        }
        CtrlKeyMode::Selected => selected_mode_shortcut(letter_shortcut, &shortened_text, palette),
        CtrlKeyMode::Disabled => disabled_mode_shortcut(
            &format!(" <{}>{}", letter_shortcut, shortened_text),
            palette,
        ),
        CtrlKeyMode::Disabled => disabled_mode_shortcut(
            &format!(" <{}>{}", letter_shortcut, shortened_text),
            palette,
        ),
    }
}

fn single_letter_ctrl_key(key: &CtrlKeyShortcut, palette: ColoredElements) -> LinePart {
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        CtrlKeyMode::Unselected => unselected_mode_shortcut_single_letter(letter_shortcut, palette),
        CtrlKeyMode::Selected => selected_mode_shortcut_single_letter(letter_shortcut, palette),
        CtrlKeyMode::Disabled => disabled_mode_shortcut(&format!(" {}", letter_shortcut), palette),
    }
}

fn key_indicators(max_len: usize, keys: &[CtrlKeyShortcut], palette: ColoredElements) -> LinePart {
    let mut line_part = LinePart::default();
    for ctrl_key in keys {
        let key = full_ctrl_key(ctrl_key, palette);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = shortened_ctrl_key(ctrl_key, palette);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = single_letter_ctrl_key(ctrl_key, palette);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    line_part
}

pub fn superkey(palette: ColoredElements) -> LinePart {
    let prefix_text = " Ctrl + ";
    let prefix = palette.superkey_prefix.paint(prefix_text);
    let suffix_separator = palette.superkey_suffix_separator.paint(ARROW_SEPARATOR);
    LinePart {
        part: ANSIStrings(&[prefix, suffix_separator]).to_string(),
        len: prefix_text.chars().count(),
    }
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize) -> LinePart {
    let colored_elements = color_elements(help.palette);
    match &help.mode {
        InputMode::Locked => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
        InputMode::Resize => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
        InputMode::Pane => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
        InputMode::Tab | InputMode::RenameTab | InputMode::SearchTab => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
        InputMode::Scroll => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
        InputMode::Normal => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            colored_elements,
        ),
    }
}
