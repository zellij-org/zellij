use ansi_term::{ANSIStrings, Color::RGB, Style};
use zellij_tile::prelude::*;

use crate::colors::{BLACK, BRIGHT_GRAY, GRAY, GREEN, RED, WHITE};
use crate::{LinePart, ARROW_SEPARATOR};

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

fn unselected_mode_shortcut(letter: char, text: &str, palette: Palette) -> LinePart {
    let prefix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new()
        .bold()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(" <");
    let char_shortcut = Style::new()
        .bold()
        .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(letter.to_string());
    let char_right_separator = Style::new()
        .bold()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(">");
    let styled_text = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .paint(ARROW_SEPARATOR);
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

fn selected_mode_shortcut(letter: char, text: &str, palette: Palette) -> LinePart {
    let prefix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(format!(" <"));
    let char_shortcut = Style::new()
        .bold()
        .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(format!("{}", letter));
    let char_right_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(format!(">"));
    let styled_text = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new()
        .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .paint(ARROW_SEPARATOR);
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

fn disabled_mode_shortcut(text: &str, palette: Palette) -> LinePart {
    let prefix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    let styled_text = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .dimmed()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    LinePart {
        part: format!("{}{}{}", prefix_separator, styled_text, suffix_separator),
        len: text.chars().count() + 2 + 1, // 2 for the arrows, 1 for the padding in the end
    }
}

fn selected_mode_shortcut_single_letter(letter: char, palette: Palette) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .paint(ARROW_SEPARATOR);
    let char_shortcut = Style::new()
        .bold()
        .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(char_shortcut_text);
    let suffix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn unselected_mode_shortcut_single_letter(letter: char, palette: Palette) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    let char_shortcut = Style::new()
        .bold()
        .fg(RGB(palette.red.0, palette.red.1, palette.red.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(char_shortcut_text);
    let suffix_separator = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .paint(ARROW_SEPARATOR);
    LinePart {
        part: ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator]).to_string(),
        len,
    }
}

fn full_ctrl_key(key: &CtrlKeyShortcut, palette: Palette) -> LinePart {
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

fn shortened_ctrl_key(key: &CtrlKeyShortcut, palette: Palette) -> LinePart {
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

fn single_letter_ctrl_key(key: &CtrlKeyShortcut, palette: Palette) -> LinePart {
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        CtrlKeyMode::Unselected => unselected_mode_shortcut_single_letter(letter_shortcut, palette),
        CtrlKeyMode::Selected => selected_mode_shortcut_single_letter(letter_shortcut, palette),
        CtrlKeyMode::Disabled => disabled_mode_shortcut(&format!(" {}", letter_shortcut), palette),
    }
}

fn key_indicators(max_len: usize, keys: &[CtrlKeyShortcut], palette: Palette) -> LinePart {
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

pub fn superkey(palette: Palette) -> LinePart {
    let prefix_text = " Ctrl + ";
    let prefix = Style::new()
        .fg(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .on(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .bold()
        .paint(prefix_text);
    LinePart {
        part: prefix.to_string(),
        len: prefix_text.chars().count(),
    }
}

pub fn ctrl_keys(help: &ModeInfo, max_len: usize) -> LinePart {
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
            help.palette,
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
            help.palette,
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
            help.palette,
        ),
        InputMode::Tab | InputMode::RenameTab => key_indicators(
            max_len,
            &[
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
            help.palette,
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
            help.palette,
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
            help.palette,
        ),
    }
}
