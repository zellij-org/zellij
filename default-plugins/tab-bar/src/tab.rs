use crate::{line::tab_separator, LinePart};
use ansi_term::{ANSIString, ANSIStrings};
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn cursors(focused_clients: &[ClientId], palette: Palette) -> (Vec<ANSIString>, usize) {
    // cursor section, text length
    let mut len = 0;
    let mut cursors = vec![];
    for client_id in focused_clients.iter() {
        if let Some(color) = client_id_to_colors(*client_id, palette) {
            cursors.push(style!(color.1, color.0).paint(" "));
            len += 1;
        }
    }
    (cursors, len)
}

pub fn render_tab(
    text: String,
    palette: Palette,
    separator: &str,
    focused_clients: &[ClientId],
    active: bool,
    is_alternate_tab: bool,
    tab_index: usize,
) -> LinePart {
    let separator_width = separator.width();
    let alternate_tab_color = match palette.theme_hue {
        // TODO: only do this if we don't have the arrow capabilities
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    };
    let background_color = if active {
        palette.green
    } else if is_alternate_tab {
        alternate_tab_color
    } else {
        palette.fg
    };
    let foreground_color = match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    };
    let left_separator = style!(foreground_color, background_color).paint(separator);
    let mut tab_text_len = text.width() + (separator_width * 2) + 2; // +2 for padding
    let tab_styled_text = style!(foreground_color, background_color)
        .bold()
        .paint(format!(" {} ", text));

    let right_separator = style!(background_color, foreground_color).paint(separator);
    let tab_styled_text = if !focused_clients.is_empty() {
        let (cursor_section, extra_length) = cursors(focused_clients, palette);
        tab_text_len += extra_length;
        let mut s = String::new();
        let cursor_beginning = style!(foreground_color, background_color)
            .bold()
            .paint("[")
            .to_string();
        let cursor_section = ANSIStrings(&cursor_section).to_string();
        let cursor_end = style!(foreground_color, background_color)
            .bold()
            .paint("]")
            .to_string();
        s.push_str(&left_separator.to_string());
        s.push_str(&tab_styled_text.to_string());
        s.push_str(&cursor_beginning);
        s.push_str(&cursor_section);
        s.push_str(&cursor_end);
        s.push_str(&right_separator.to_string());
        s
    } else {
        ANSIStrings(&[left_separator, tab_styled_text, right_separator]).to_string()
    };

    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
        tab_index: Some(tab_index),
    }
}

pub fn tab_style(
    text: String,
    is_active_tab: bool,
    is_alternate_tab: bool,
    is_sync_panes_active: bool,
    palette: Palette,
    capabilities: PluginCapabilities,
    focused_clients: &[ClientId],
    tab_index: usize,
) -> LinePart {
    let separator = tab_separator(capabilities);
    let mut tab_text = text;
    if is_sync_panes_active {
        tab_text.push_str(" (Sync)");
    }
    // we only color alternate tabs differently if we can't use the arrow fonts to separate them
    let is_alternate_tab = if !capabilities.arrow_fonts {
        false
    } else {
        is_alternate_tab
    };
    render_tab(
        tab_text,
        palette,
        separator,
        focused_clients,
        is_active_tab,
        is_alternate_tab,
        tab_index,
    )
}
