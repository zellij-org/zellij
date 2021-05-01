use crate::{LinePart, ARROW_SEPARATOR};
use ansi_term::{ANSIStrings, Color::RGB, Style};
use zellij_tile::data::Palette;

macro_rules! style {
    ($a:expr, $b:expr) => {
        Style::new()
            .fg(RGB($a.0, $a.1, $a.2))
            .on(RGB($b.0, $b.1, $b.2))
    };
}

pub fn active_tab(text: String, palette: Palette) -> LinePart {
    let left_separator = style!(palette.bg, palette.green).paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the text padding
    let tab_styled_text = style!(palette.bg, palette.green)
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = style!(palette.green, palette.bg).paint(ARROW_SEPARATOR);
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
    let left_separator = style!(palette.bg, palette.bg).paint(ARROW_SEPARATOR);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the padding
    let tab_styled_text = style!(palette.fg, palette.bg)
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = style!(palette.bg, palette.bg).paint(ARROW_SEPARATOR);
    let tab_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, tab_styled_text, right_separator,])
    );
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn tab_style(
    text: String,
    is_active_tab: bool,
    position: usize,
    is_sync_panes_active: bool,
    palette: Palette
) -> LinePart {
    let mut tab_text = if text.is_empty() {
        format!("Tab #{}", position + 1)
    } else {
        text
    };
    if is_sync_panes_active {
        tab_text.push_str(" (Sync)");
    }
    if is_active_tab {
        active_tab(tab_text, palette)
    } else {
        non_active_tab(tab_text, palette)
    }
}
