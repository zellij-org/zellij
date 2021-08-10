use crate::{line::tab_separator, LinePart};
use ansi_term::{ANSIStrings, Style};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

pub fn active_tab(text: String, palette: Option<Palette>, separator: &str) -> LinePart {
    let left_separator_style = if let Some(palette) = palette {
        style!(palette.cyan, palette.green)
    } else {
        Style::new()
    };
    let left_separator = left_separator_style.paint(separator);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the text padding
    let tab_style = if let Some(palette) = palette {
        style!(palette.black, palette.green)
    } else {
        Style::new()
    };
    let tab_styled_text = tab_style.bold().paint(format!(" {} ", text));
    let right_separator_style = if let Some(palette) = palette {
        style!(palette.green, palette.cyan)
    } else {
        Style::new()
    };
    let right_separator = right_separator_style.paint(separator);
    let tab_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, tab_styled_text, right_separator,])
    );
    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
    }
}

pub fn non_active_tab(text: String, palette: Option<Palette>, separator: &str) -> LinePart {
    let left_separator_style = if let Some(palette) = palette {
        style!(palette.cyan, palette.fg)
    } else {
        Style::new()
    };
    let left_separator = left_separator_style.paint(separator);
    let tab_text_len = text.chars().count() + 4; // 2 for left and right separators, 2 for the padding
    let tab_style = if let Some(palette) = palette {
        style!(palette.black, palette.fg)
    } else {
        Style::new()
    };
    let tab_styled_text = tab_style.bold().paint(format!(" {} ", text));
    let right_separator_style = if let Some(palette) = palette {
        style!(palette.fg, palette.cyan)
    } else {
        Style::new()
    };
    let right_separator = right_separator_style.paint(separator);
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
    palette: Option<Palette>,
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
