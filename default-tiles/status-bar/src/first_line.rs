use ansi_term::{ANSIStrings, Style};
use zellij_tile::*;

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

fn unselected_mode_shortcut(letter: char, text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(GRAY).on(BRIGHT_GRAY).paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new()
        .bold()
        .fg(BLACK)
        .on(BRIGHT_GRAY)
        .bold()
        .paint(format!(" <"));
    let char_shortcut = Style::new()
        .bold()
        .fg(RED)
        .on(BRIGHT_GRAY)
        .bold()
        .paint(format!("{}", letter));
    let char_right_separator = Style::new()
        .bold()
        .fg(BLACK)
        .on(BRIGHT_GRAY)
        .bold()
        .paint(format!(">"));
    let styled_text = Style::new()
        .fg(BLACK)
        .on(BRIGHT_GRAY)
        .bold()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new().fg(BRIGHT_GRAY).on(GRAY).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                prefix_separator,
                char_left_separator,
                char_shortcut,
                char_right_separator,
                styled_text,
                suffix_separator
            ])
        ),
        len: text.chars().count() + 7, // 2 for the arrows, 3 for the char separators, 1 for the character, 1 for the text padding
    }
}

fn selected_mode_shortcut(letter: char, text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(GRAY).on(GREEN).paint(ARROW_SEPARATOR);
    let char_left_separator = Style::new()
        .bold()
        .fg(BLACK)
        .on(GREEN)
        .bold()
        .paint(format!(" <"));
    let char_shortcut = Style::new()
        .bold()
        .fg(RED)
        .on(GREEN)
        .bold()
        .paint(format!("{}", letter));
    let char_right_separator = Style::new()
        .bold()
        .fg(BLACK)
        .on(GREEN)
        .bold()
        .paint(format!(">"));
    let styled_text = Style::new()
        .fg(BLACK)
        .on(GREEN)
        .bold()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new().fg(GREEN).on(GRAY).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                prefix_separator,
                char_left_separator,
                char_shortcut,
                char_right_separator,
                styled_text,
                suffix_separator
            ])
        ),
        len: text.chars().count() + 7, // 2 for the arrows, 3 for the char separators, 1 for the character, 1 for the text padding
    }
}

fn disabled_mode_shortcut(text: &str) -> LinePart {
    let prefix_separator = Style::new().fg(GRAY).on(BRIGHT_GRAY).paint(ARROW_SEPARATOR);
    let styled_text = Style::new()
        .fg(GRAY)
        .on(BRIGHT_GRAY)
        .dimmed()
        .paint(format!("{} ", text));
    let suffix_separator = Style::new().fg(BRIGHT_GRAY).on(GRAY).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!("{}{}{}", prefix_separator, styled_text, suffix_separator),
        len: text.chars().count() + 2 + 1, // 2 for the arrows, 1 for the padding in the end
    }
}

fn selected_mode_shortcut_single_letter(letter: char) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = Style::new().fg(GRAY).on(GREEN).paint(ARROW_SEPARATOR);
    let char_shortcut = Style::new()
        .bold()
        .fg(RED)
        .on(GREEN)
        .bold()
        .paint(char_shortcut_text);
    let suffix_separator = Style::new().fg(GREEN).on(GRAY).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator])
        ),
        len,
    }
}

fn unselected_mode_shortcut_single_letter(letter: char) -> LinePart {
    let char_shortcut_text = format!(" {} ", letter);
    let len = char_shortcut_text.chars().count() + 4; // 2 for the arrows, 2 for the padding
    let prefix_separator = Style::new().fg(GRAY).on(BRIGHT_GRAY).paint(ARROW_SEPARATOR);
    let char_shortcut = Style::new()
        .bold()
        .fg(RED)
        .on(BRIGHT_GRAY)
        .bold()
        .paint(char_shortcut_text);
    let suffix_separator = Style::new().fg(BRIGHT_GRAY).on(GRAY).paint(ARROW_SEPARATOR);
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[prefix_separator, char_shortcut, suffix_separator])
        ),
        len,
    }
}

fn full_ctrl_key(key: &CtrlKeyShortcut) -> LinePart {
    let full_text = key.full_text();
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        CtrlKeyMode::Unselected => {
            unselected_mode_shortcut(letter_shortcut, &format!(" {}", full_text))
        }
        CtrlKeyMode::Selected => {
            selected_mode_shortcut(letter_shortcut, &format!(" {}", full_text))
        }
        CtrlKeyMode::Disabled => {
            disabled_mode_shortcut(&format!(" <{}> {}", letter_shortcut, full_text))
        }
    }
}

fn shortened_ctrl_key(key: &CtrlKeyShortcut) -> LinePart {
    let shortened_text = key.shortened_text();
    let letter_shortcut = key.letter_shortcut();
    let shortened_text = match key.action {
        CtrlKeyAction::Lock => format!(" {}", shortened_text),
        _ => shortened_text,
    };
    match key.mode {
        CtrlKeyMode::Unselected => {
            unselected_mode_shortcut(letter_shortcut, &format!("{}", shortened_text))
        }
        CtrlKeyMode::Selected => {
            selected_mode_shortcut(letter_shortcut, &format!("{}", shortened_text))
        }
        CtrlKeyMode::Disabled => {
            disabled_mode_shortcut(&format!(" <{}>{}", letter_shortcut, shortened_text))
        }
    }
}

fn single_letter_ctrl_key(key: &CtrlKeyShortcut) -> LinePart {
    let letter_shortcut = key.letter_shortcut();
    match key.mode {
        CtrlKeyMode::Unselected => unselected_mode_shortcut_single_letter(letter_shortcut),
        CtrlKeyMode::Selected => selected_mode_shortcut_single_letter(letter_shortcut),
        CtrlKeyMode::Disabled => disabled_mode_shortcut(&format!(" {}", letter_shortcut)),
    }
}

fn key_indicators(max_len: usize, keys: &[CtrlKeyShortcut]) -> LinePart {
    let mut line_part = LinePart::default();
    for ctrl_key in keys {
        let key = full_ctrl_key(ctrl_key);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = shortened_ctrl_key(ctrl_key);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    for ctrl_key in keys {
        let key = single_letter_ctrl_key(ctrl_key);
        line_part.part = format!("{}{}", line_part.part, key.part);
        line_part.len += key.len;
    }
    if line_part.len < max_len {
        return line_part;
    }
    line_part = LinePart::default();
    line_part
}

pub fn superkey() -> LinePart {
    let prefix_text = " Ctrl + ";
    let prefix = Style::new().fg(WHITE).on(GRAY).bold().paint(prefix_text);
    LinePart {
        part: format!("{}", prefix),
        len: prefix_text.chars().count(),
    }
}

pub fn ctrl_keys(help: &Help, max_len: usize) -> LinePart {
    match &help.mode {
        InputMode::Locked => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Disabled, CtrlKeyAction::Quit),
            ],
        ),
        InputMode::Resize => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
        ),
        InputMode::Pane => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
        ),
        InputMode::Tab | InputMode::RenameTab => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
        ),
        InputMode::Scroll => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Selected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
        ),
        InputMode::Normal | _ => key_indicators(
            max_len,
            &vec![
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Lock),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Pane),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Tab),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Resize),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Scroll),
                CtrlKeyShortcut::new(CtrlKeyMode::Unselected, CtrlKeyAction::Quit),
            ],
        ),
    }
}
