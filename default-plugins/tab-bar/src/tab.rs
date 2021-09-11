use crate::{line::tab_separator, LinePart};
use ansi_term::ANSIStrings;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

pub fn active_tab(text: String, palette: Palette, separator: &str) -> LinePart {
    let left_separator = style!(palette.cyan, palette.green).paint(separator);
    let tab_text_len = text.width() + 2 + separator.width() * 2; // 2 for left and right separators, 2 for the text padding
    let tab_styled_text = style!(palette.black, palette.green)
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = style!(palette.green, palette.cyan).paint(separator);
    let tab_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, tab_styled_text, right_separator,])
    );
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn non_active_tab(text: String, palette: Palette, separator: &str) -> LinePart {
    let left_separator = style!(palette.cyan, palette.fg).paint(separator);
    let tab_text_len = text.width() + 2 + separator.width() * 2; // 2 for left and right separators, 2 for the text padding
    let tab_styled_text = style!(palette.black, palette.fg)
        .bold()
        .paint(format!(" {} ", text));
    let right_separator = style!(palette.fg, palette.cyan).paint(separator);
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
    is_sync_panes_active: bool,
    palette: Palette,
    capabilities: PluginCapabilities,
) -> LinePart {
    let separator = tab_separator(capabilities);
    let mut tab_text = text;
    if is_sync_panes_active {
        tab_text.push_str(" (Sync)");
    }
    if is_active_tab {
        active_tab(tab_text, palette, separator)
    } else {
        non_active_tab(tab_text, palette, separator)
    }
}
