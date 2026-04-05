use crate::{line::tab_separator, LinePart};
use ansi_term::{ANSIString, ANSIStrings};
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn cursors<'a>(
    focused_clients: &'a [ClientId],
    multiplayer_colors: MultiplayerColors,
) -> (Vec<ANSIString<'a>>, usize) {
    // cursor section, text length
    let mut len = 0;
    let mut cursors = vec![];
    for client_id in focused_clients.iter() {
        if let Some(color) = client_id_to_colors(*client_id, multiplayer_colors) {
            cursors.push(style!(color.1, color.0).paint(" "));
            len += 1;
        }
    }
    (cursors, len)
}

pub fn render_tab(
    text: String,
    tab: &TabInfo,
    is_alternate_tab: bool,
    palette: Styling,
    separator: &str,
) -> LinePart {
    let focused_clients = tab.other_focused_clients.as_slice();
    let separator_width = separator.width();

    let alternate_tab_color = if is_alternate_tab {
        palette.ribbon_unselected.emphasis_1
    } else {
        palette.ribbon_unselected.background
    };
    let background_color = if tab.active {
        palette.ribbon_selected.background
    } else if is_alternate_tab {
        alternate_tab_color
    } else {
        palette.ribbon_unselected.background
    };
    let foreground_color = if tab.is_flashing_bell {
        if tab.active {
            palette.ribbon_selected.emphasis_3
        } else {
            palette.ribbon_unselected.emphasis_3
        }
    } else if tab.active {
        palette.ribbon_selected.base
    } else {
        palette.ribbon_unselected.base
    };

    let separator_fill_color = palette.text_unselected.background;
    let left_separator = style!(separator_fill_color, background_color).paint(separator);
    let mut tab_text_len = text.width() + (separator_width * 2) + 2; // +2 for padding
    let tab_styled_text = style!(foreground_color, background_color)
        .bold()
        .paint(format!(" {} ", text));

    let right_separator = style!(background_color, separator_fill_color).paint(separator);
    let tab_styled_text = if !focused_clients.is_empty() {
        let (cursor_section, extra_length) =
            cursors(focused_clients, palette.multiplayer_user_colors);
        tab_text_len += extra_length + 2; // 2 for cursor_beginning and cursor_end
        let cursor_beginning = style!(foreground_color, background_color).bold().paint("[");
        let cursor_section = ANSIStrings(&cursor_section);
        let cursor_end = style!(foreground_color, background_color).bold().paint("]");
        format!("{left_separator}{tab_styled_text}{cursor_beginning}{cursor_section}{cursor_end}{right_separator}")
    } else {
        ANSIStrings(&[left_separator, tab_styled_text, right_separator]).to_string()
    };

    LinePart {
        part: tab_styled_text,
        len: tab_text_len,
        tab_index: Some(tab.position),
    }
}

pub fn tab_style(
    mut tabname: String,
    tab: &TabInfo,
    mut is_alternate_tab: bool,
    palette: Styling,
    capabilities: PluginCapabilities,
) -> LinePart {
    let separator = tab_separator(capabilities);

    if tab.is_fullscreen_active {
        tabname.push_str(" (FULLSCREEN)");
    } else if tab.is_sync_panes_active {
        tabname.push_str(" (SYNC)");
    }
    if tab.has_bell_notification || tab.is_flashing_bell {
        tabname.push_str(" [!]");
    }
    // we only color alternate tabs differently if we can't use the arrow fonts to separate them
    if !capabilities.arrow_fonts {
        is_alternate_tab = false;
    }

    render_tab(tabname, tab, is_alternate_tab, palette, separator)
}

pub(crate) fn get_tab_to_focus(
    tab_line: &[LinePart],
    active_tab_idx: usize,
    mouse_click_col: usize,
) -> Option<usize> {
    let clicked_line_part = get_clicked_line_part(tab_line, mouse_click_col)?;
    let clicked_tab_idx = clicked_line_part.tab_index?;
    // tabs are indexed starting from 1 so we need to add 1
    let clicked_tab_idx = clicked_tab_idx + 1;
    if clicked_tab_idx != active_tab_idx {
        return Some(clicked_tab_idx);
    }
    None
}

pub(crate) fn get_clicked_line_part(
    tab_line: &[LinePart],
    mouse_click_col: usize,
) -> Option<&LinePart> {
    let mut len = 0;
    for tab_line_part in tab_line {
        if mouse_click_col >= len && mouse_click_col < len + tab_line_part.len {
            return Some(tab_line_part);
        }
        len += tab_line_part.len;
    }
    None
}
