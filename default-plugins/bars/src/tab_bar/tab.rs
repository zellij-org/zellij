use crate::tab_bar::{line::tab_separator, LinePart};
use ansi_term::{ANSIString, ANSIStrings};
use zellij_tile::graphemes;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn cursors<'a>(
    focused_client_slots: &'a [usize],
    multiplayer_colors: MultiplayerColors,
) -> (Vec<ANSIString<'a>>, usize) {
    let mut len = 0;
    let mut cursors = vec![];
    for display_slot in focused_client_slots.iter() {
        if let Some(color) = client_slot_to_colors(*display_slot, multiplayer_colors) {
            cursors.push(style!(color.1, color.0).paint(" "));
            len += 1;
        }
    }
    (cursors, len)
}

pub(crate) fn fit_tab_text(text: String, max_width: usize) -> String {
    if max_width == 0 || graphemes::display_width(&text) <= max_width {
        text
    } else {
        graphemes::truncate_end(&text, max_width, "…").into_owned()
    }
}

pub fn render_tab(
    text: String,
    tab: &TabInfo,
    is_alternate_tab: bool,
    is_hovered: bool,
    is_dragged: bool,
    palette: Styling,
    separator: &str,
    dimmed: bool,
    max_width: usize,
) -> LinePart {
    let focused_clients = tab.other_focused_client_slots.as_slice();
    let separator_width = graphemes::display_width(separator);

    let alternate_tab_color = if is_alternate_tab {
        palette.ribbon_unselected.emphasis_1
    } else {
        palette.ribbon_unselected.background
    };
    let background_color = if tab.active {
        palette.ribbon_selected.background
    } else if is_hovered {
        palette.ribbon_unselected.emphasis_1
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
    let background_color = if dimmed {
        palette.ribbon_unselected.background
    } else {
        background_color
    };
    let text_style = if dimmed {
        style!(palette.ribbon_unselected.base, background_color).italic()
    } else {
        style!(foreground_color, background_color).bold()
    };
    let text_style = if is_dragged {
        text_style.italic()
    } else {
        text_style
    };
    let left_separator = style!(separator_fill_color, background_color).paint(separator);
    let right_separator_style = style!(background_color, separator_fill_color);
    let cursor_info = if focused_clients.is_empty() {
        None
    } else {
        Some(cursors(focused_clients, palette.multiplayer_user_colors))
    };
    let cursor_len = cursor_info
        .as_ref()
        .map(|(_, extra_length)| extra_length + 2)
        .unwrap_or(0);
    let decoration_len = (separator_width * 2) + 2 + cursor_len;
    let text = fit_tab_text(text, max_width.saturating_sub(decoration_len));
    let tab_text_len = graphemes::display_width(&text) + decoration_len;
    let tab_styled_text = text_style.paint(format!(" {} ", text));

    let right_separator = right_separator_style.paint(separator);
    let tab_styled_text = if let Some((cursor_section, _)) = cursor_info {
        let mut s = String::new();
        let cursor_beginning = text_style.paint("[").to_string();
        let cursor_section = ANSIStrings(&cursor_section).to_string();
        let cursor_end = text_style.paint("]").to_string();
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
        tab_index: Some(tab.position),
    }
}

pub fn tab_style(
    mut tabname: String,
    tab: &TabInfo,
    mut is_alternate_tab: bool,
    is_hovered: bool,
    is_dragged: bool,
    palette: Styling,
    capabilities: PluginCapabilities,
    dimmed: bool,
    max_width: usize,
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
    if !capabilities.arrow_fonts {
        is_alternate_tab = false;
    }

    render_tab(
        tabname,
        tab,
        is_alternate_tab,
        is_hovered,
        is_dragged,
        palette,
        separator,
        dimmed,
        max_width,
    )
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
