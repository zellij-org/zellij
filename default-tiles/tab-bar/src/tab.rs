use ansi_term::{Style, ANSIStrings};
use crate::{LinePart, ARROW_SEPARATOR};
use crate::colors::{GRAY, GREEN, BLACK, BRIGHT_GRAY};

pub fn active_tab(text: String) -> LinePart {
    let left_separator = Style::new().fg(GRAY).on(GREEN).paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the text padding
    let tab_styled_text = Style::new().fg(BLACK).on(GREEN).bold().paint(format!(" {} ", text));
    let right_separator = Style::new().fg(GREEN).on(GRAY).paint(ARROW_SEPARATOR);
    let tab_styled_text = format!("{}", ANSIStrings(&[
        left_separator,
        tab_styled_text,
        right_separator,
    ]));
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn non_active_tab(text: String) -> LinePart {
    let left_separator = Style::new().fg(GRAY).on(BRIGHT_GRAY).paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the padding
    let tab_styled_text = Style::new().fg(BLACK).on(BRIGHT_GRAY).bold().paint(format!(" {} ", text));
    let right_separator = Style::new().fg(BRIGHT_GRAY).on(GRAY).paint(ARROW_SEPARATOR);
    let tab_styled_text = format!("{}", ANSIStrings(&[
        left_separator,
        tab_styled_text,
        right_separator,
    ]));
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn tab_style(text: String, is_active_tab: bool, position: usize) -> LinePart {
    let tab_text = if text.is_empty() {
        format!("Tab #{}", position + 1)
    } else {
        text
    };
    if is_active_tab {
        active_tab(tab_text)
    } else {
        non_active_tab(tab_text)
    }
}
