use colored::*;

use crate::{LinePart, ARROW_SEPARATOR};

pub fn active_tab(text: String, is_furthest_to_the_left: bool) -> LinePart {
    let left_separator = if is_furthest_to_the_left {
        " ".black().on_magenta()
    } else {
        ARROW_SEPARATOR.black().on_magenta()
    };
    let right_separator = ARROW_SEPARATOR.magenta().on_black();
    let tab_styled_text = format!("{}{}{}", left_separator, text, right_separator)
        .black()
        .bold()
        .on_magenta();
    let tab_text_len = text.chars().count() + 2; // 2 for left and right separators
    LinePart {
        part: format!("{}", tab_styled_text),
        len: tab_text_len,
    }
}

pub fn non_active_tab(text: String, is_furthest_to_the_left: bool) -> LinePart {
    let left_separator = if is_furthest_to_the_left {
        " ".black().on_green()
    } else {
        ARROW_SEPARATOR.black().on_green()
    };
    let right_separator = ARROW_SEPARATOR.green().on_black();
    let tab_styled_text = format!("{}{}{}", left_separator, text, right_separator)
        .black()
        .bold()
        .on_green();
    let tab_text_len = text.chars().count() + 2; // 2 for the left and right separators
    LinePart {
        part: format!("{}", tab_styled_text),
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
