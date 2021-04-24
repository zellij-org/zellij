use crate::colors::{BLACK, BRIGHT_GRAY, GRAY, GREEN};
use crate::{LinePart, ARROW_SEPARATOR};
use ansi_term::{ANSIStrings, Color::RGB, Style};
use zellij_tile::data::Palette;

pub fn active_tab(text: String, palette: Palette) -> LinePart {
    let left_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the text padding
    let tab_styled_text = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.green.0, palette.green.1, palette.green.2))
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = Style::new()
        .fg(RGB(palette.green.0, palette.green.1, palette.green.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .paint(ARROW_SEPARATOR);
    let tab_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, tab_styled_text, right_separator,])
    );
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn non_active_tab(text: String, palette: Palette) -> LinePart {
    let left_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the padding
    let tab_styled_text = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = Style::new()
        .fg(RGB(palette.fg.0, palette.fg.1, palette.fg.2))
        .on(RGB(palette.bg.0, palette.bg.1, palette.bg.2))
        .paint(ARROW_SEPARATOR);
    let tab_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, tab_styled_text, right_separator,])
    );
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn tab_style(text: String, is_active_tab: bool, position: usize, palette: Palette) -> LinePart {
    let tab_text = if text.is_empty() {
        format!("Tab #{}", position + 1)
    } else {
        text
    };
    if is_active_tab {
        active_tab(tab_text, palette)
    } else {
        non_active_tab(tab_text, palette)
    }
}
