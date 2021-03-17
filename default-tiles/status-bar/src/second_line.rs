// use colored::*;
use ansi_term::{ANSIStrings, Style};
use zellij_tile::*;

use crate::colors::{BLACK, GREEN, ORANGE, WHITE};
use crate::{LinePart, MORE_MSG};

fn full_length_shortcut(is_first_shortcut: bool, letter: &str, description: &str) -> LinePart {
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().on(BLACK).fg(WHITE).paint(separator);
    let shortcut_len = letter.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().on(BLACK).fg(WHITE).paint("<");
    let shortcut = Style::new().on(BLACK).fg(GREEN).bold().paint(letter);
    let shortcut_right_separator = Style::new().on(BLACK).fg(WHITE).paint("> ");
    let description_len = description.chars().count();
    let description = Style::new().on(BLACK).fg(WHITE).bold().paint(description);
    let len = shortcut_len + description_len + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description
            ])
        ),
        len,
    }
}

fn first_word_shortcut(is_first_shortcut: bool, letter: &str, description: &str) -> LinePart {
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().on(BLACK).fg(WHITE).paint(separator);
    let shortcut_len = letter.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().on(BLACK).fg(WHITE).paint("<");
    let shortcut = Style::new().on(BLACK).fg(GREEN).bold().paint(letter);
    let shortcut_right_separator = Style::new().on(BLACK).fg(WHITE).paint("> ");
    let description_first_word = description.split(' ').next().unwrap_or("");
    let description_first_word_length = description_first_word.chars().count();
    let description_first_word = Style::new()
        .on(BLACK)
        .fg(WHITE)
        .bold()
        .paint(description_first_word);
    let len = shortcut_len + description_first_word_length + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description_first_word,
            ])
        ),
        len,
    }
}

fn locked_interface_indication() -> LinePart {
    let locked_text = " -- INTERFACE LOCKED -- ";
    let locked_text_len = locked_text.chars().count();
    let locked_styled_text = Style::new().on(BLACK).fg(WHITE).bold().paint(locked_text);
    LinePart {
        part: format!("{}", locked_styled_text),
        len: locked_text_len,
    }
}

fn select_pane_shortcut(is_first_shortcut: bool) -> LinePart {
    let shortcut = "ENTER";
    let description = "Select pane";
    let separator = if is_first_shortcut { " " } else { " / " };
    let separator = Style::new().on(BLACK).fg(WHITE).paint(separator);
    let shortcut_len = shortcut.chars().count() + 3; // 2 for <>'s around shortcut, 1 for the space
    let shortcut_left_separator = Style::new().on(BLACK).fg(WHITE).paint("<");
    let shortcut = Style::new().on(BLACK).fg(ORANGE).bold().paint(shortcut);
    let shortcut_right_separator = Style::new().on(BLACK).fg(WHITE).paint("> ");
    let description_len = description.chars().count();
    let description = Style::new().on(BLACK).fg(WHITE).bold().paint(description);
    let len = shortcut_len + description_len + separator.chars().count();
    LinePart {
        part: format!(
            "{}",
            ANSIStrings(&[
                separator,
                shortcut_left_separator,
                shortcut,
                shortcut_right_separator,
                description
            ])
        ),
        len,
    }
}

fn full_shortcut_list(help: &Help) -> LinePart {
    match help.mode {
        InputMode::Locked => locked_interface_indication(),
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in help.keybinds.iter().enumerate() {
                let shortcut = full_length_shortcut(i == 0, &letter, &description);
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut,);
            }
            if help.mode != InputMode::Normal {
                let select_pane_shortcut = select_pane_shortcut(help.keybinds.len() == 0);
                line_part.len += select_pane_shortcut.len;
                line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            }
            line_part
        }
    }
}

fn shortened_shortcut_list(help: &Help) -> LinePart {
    match help.mode {
        InputMode::Locked => locked_interface_indication(),
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in help.keybinds.iter().enumerate() {
                let shortcut = first_word_shortcut(i == 0, &letter, &description);
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut,);
            }
            if help.mode != InputMode::Normal {
                let select_pane_shortcut = select_pane_shortcut(help.keybinds.len() == 0);
                line_part.len += select_pane_shortcut.len;
                line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            }
            line_part
        }
    }
}

fn best_effort_shortcut_list(help: &Help, max_len: usize) -> LinePart {
    match help.mode {
        InputMode::Locked => {
            let line_part = locked_interface_indication();
            if line_part.len <= max_len {
                line_part
            } else {
                LinePart::default()
            }
        }
        _ => {
            let mut line_part = LinePart::default();
            for (i, (letter, description)) in help.keybinds.iter().enumerate() {
                let shortcut = first_word_shortcut(i == 0, &letter, &description);
                if line_part.len + shortcut.len + MORE_MSG.chars().count() > max_len {
                    // TODO: better
                    line_part.part = format!("{}{}", line_part.part, MORE_MSG);
                    line_part.len += MORE_MSG.chars().count();
                    break;
                }
                line_part.len += shortcut.len;
                line_part.part = format!("{}{}", line_part.part, shortcut,);
            }
            let select_pane_shortcut = select_pane_shortcut(help.keybinds.len() == 0);
            if line_part.len + select_pane_shortcut.len <= max_len {
                line_part.len += select_pane_shortcut.len;
                line_part.part = format!("{}{}", line_part.part, select_pane_shortcut,);
            }
            line_part
        }
    }
}

pub fn keybinds(help: &Help, max_width: usize) -> LinePart {
    let full_shortcut_list = full_shortcut_list(help);
    if full_shortcut_list.len <= max_width {
        return full_shortcut_list;
    }
    let shortened_shortcut_list = shortened_shortcut_list(help);
    if shortened_shortcut_list.len <= max_width {
        return shortened_shortcut_list;
    }
    return best_effort_shortcut_list(help, max_width);
}
