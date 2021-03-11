use ansi_term::{Style, ANSIStrings};
use ansi_term::Colour::Fixed;
use crate::{LinePart, ARROW_SEPARATOR};

pub fn active_tab(text: String, is_furthest_to_the_left: bool) -> LinePart {
    let left_separator = if is_furthest_to_the_left {
        Style::new().fg(Fixed(238)).on(Fixed(154)).paint(ARROW_SEPARATOR)
    } else {
        Style::new().fg(Fixed(238)).on(Fixed(154)).paint(ARROW_SEPARATOR)
    };
    let tab_text_len = if is_furthest_to_the_left {
        text.chars().count() + 2 // 1 for the right separators
    } else {
        text.chars().count() + 2 // 2 for left and right separators
    };
    let tab_styled_text = Style::new().fg(Fixed(16)).on(Fixed(154)).bold().paint(text);
    let right_separator = Style::new().fg(Fixed(154)).on(Fixed(238)).paint(ARROW_SEPARATOR);
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

pub fn non_active_tab(text: String, is_furthest_to_the_left: bool) -> LinePart {
    let left_separator = if is_furthest_to_the_left {
        Style::new().fg(Fixed(238)).on(Fixed(245)).paint(ARROW_SEPARATOR)
    } else {
        Style::new().fg(Fixed(238)).on(Fixed(245)).paint(ARROW_SEPARATOR)
    };
    let tab_text_len = if is_furthest_to_the_left {
        text.chars().count() + 2 // 1 for the right separators
    } else {
        text.chars().count() + 2 // 2 for left and right separators
    };
    let tab_styled_text = Style::new().fg(Fixed(16)).on(Fixed(245)).bold().paint(text);
    let right_separator = Style::new().fg(Fixed(245)).on(Fixed(238)).paint(ARROW_SEPARATOR);
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

pub fn tab(text: String, is_active_tab: bool, is_furthest_to_the_left: bool) -> LinePart {
    if is_active_tab {
        active_tab(text, is_furthest_to_the_left)
    } else {
        non_active_tab(text, is_furthest_to_the_left)
    }
}

pub fn nameless_tab(index: usize, is_active_tab: bool) -> LinePart {
    let tab_text = format!(" Tab #{} ", index + 1);
    tab(tab_text, is_active_tab, index == 0)
}
