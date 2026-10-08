use crate::compact_bar::{line::tab_separator, LinePart};
use ansi_term::{ANSIString, ANSIStrings};
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn cursors<'a>(
    focused_client_slots: &'a [usize],
    colors: MultiplayerColors,
) -> (Vec<ANSIString<'a>>, usize) {
    let mut len = 0;
    let mut cursors = vec![];
    for display_slot in focused_client_slots.iter() {
        if let Some(color) = client_slot_to_colors(*display_slot, colors) {
            cursors.push(style!(color.1, color.0).paint(" "));
            len += 1;
        }
    }
    len += 2;
    (cursors, len)
}

pub fn render_tab(
    text: String,
    tab: &TabInfo,
    is_alternate_tab: bool,
    palette: Styling,
    separator: &str,
    mode_info: &ModeInfo,
) -> LinePart {
    let dimmed = mode_info.session_ascended == Some(true) || mode_info.session_dimmed == Some(true);
    let highlight_ancestor = tab.active
        && mode_info.session_dimmed == Some(true)
        && mode_info.session_ascended != Some(true)
        && mode_info
            .nested_session_ancestor_tab_highlight
            .unwrap_or(true);
    let focused_clients = tab.other_focused_client_slots.as_slice();
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
    let background_color = if highlight_ancestor {
        crate::ancestor_tab_highlight_color(palette.ribbon_selected.background)
    } else if dimmed {
        palette.ribbon_unselected.background
    } else {
        background_color
    };
    let text_style = if dimmed {
        let foreground = if highlight_ancestor {
            palette.ribbon_selected.base
        } else {
            palette.ribbon_unselected.base
        };
        style!(foreground, background_color).italic()
    } else {
        style!(foreground_color, background_color).bold()
    };
    let left_separator = style!(separator_fill_color, background_color).paint(separator);
    let mut tab_text_len = text.width() + (separator_width * 2) + 2;

    let tab_styled_text = text_style.paint(format!(" {} ", text));

    let right_separator = style!(background_color, separator_fill_color).paint(separator);
    let tab_styled_text = if !focused_clients.is_empty() {
        let (cursor_section, extra_length) =
            cursors(focused_clients, palette.multiplayer_user_colors);
        tab_text_len += extra_length;
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
    palette: Styling,
    capabilities: PluginCapabilities,
    mode_info: &ModeInfo,
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
        palette,
        separator,
        mode_info,
    )
}

pub(crate) fn get_tab_to_focus(
    tab_line: &[LinePart],
    active_tab_idx: usize,
    mouse_click_col: usize,
) -> Option<usize> {
    let clicked_line_part = get_clicked_line_part(tab_line, mouse_click_col)?;
    let clicked_tab_idx = clicked_line_part.tab_index?;
    let clicked_tab_idx = clicked_tab_idx + 1;
    if clicked_tab_idx != active_tab_idx {
        return Some(clicked_tab_idx);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestor_highlight_respects_the_flag_and_session_focus() {
        for flag in [None, Some(true), Some(false)] {
            for (ascended, dimmed) in [
                (None, Some(true)),
                (None, None),
                (Some(true), None),
                (Some(true), Some(true)),
            ] {
                for active in [false, true] {
                    let mut mode_info = ModeInfo {
                        session_ascended: ascended,
                        session_dimmed: dimmed,
                        nested_session_ancestor_tab_highlight: flag,
                        ..Default::default()
                    };
                    mode_info.style.colors.ribbon_selected.background =
                        PaletteColor::Rgb((10, 100, 240));
                    mode_info.style.colors.ribbon_selected.base = PaletteColor::Rgb((11, 12, 13));
                    let palette = mode_info.style.colors;
                    let is_dimmed = ascended == Some(true) || dimmed == Some(true);
                    let highlighted = active
                        && dimmed == Some(true)
                        && ascended != Some(true)
                        && flag != Some(false);
                    let declaration = if highlighted || (active && !is_dimmed) {
                        palette.ribbon_selected
                    } else {
                        palette.ribbon_unselected
                    };
                    let background = if highlighted {
                        PaletteColor::Rgb((132, 177, 247))
                    } else {
                        declaration.background
                    };
                    let style = style!(declaration.base, background);
                    let style = if is_dimmed {
                        style.italic()
                    } else {
                        style.bold()
                    };
                    let fill = palette.text_unselected.background;
                    let expected = ANSIStrings(&[
                        style!(fill, background).paint(""),
                        style.paint(" tab "),
                        style!(background, fill).paint(""),
                    ])
                    .to_string();
                    let tab = TabInfo {
                        active,
                        position: 2,
                        ..Default::default()
                    };
                    let rendered = render_tab("tab".into(), &tab, false, palette, "", &mode_info);
                    assert_eq!(
                        rendered.part, expected,
                        "flag={flag:?}, ascended={ascended:?}, dimmed={dimmed:?}, active={active}"
                    );
                    assert_eq!(rendered.len, 5);
                    assert_eq!(rendered.tab_index, Some(2));
                }
            }
        }
    }
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
