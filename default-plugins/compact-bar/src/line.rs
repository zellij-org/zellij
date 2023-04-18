use ansi_term::ANSIStrings;
use unicode_width::UnicodeWidthStr;

use crate::{LinePart, ARROW_SEPARATOR};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn get_current_title_len(current_title: &[LinePart]) -> usize {
    current_title.iter().map(|p| p.len).sum()
}

// move elements from before_active and after_active into tabs_to_render while they fit in cols
// adds collapsed_tabs to the left and right if there's left over tabs that don't fit
fn populate_tabs_in_tab_line(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_after_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    cols: usize,
    palette: Palette,
    capabilities: PluginCapabilities,
) {
    let mut middle_size = get_current_title_len(tabs_to_render);

    let mut total_left = 0;
    let mut total_right = 0;
    loop {
        let left_count = tabs_before_active.len();
        let right_count = tabs_after_active.len();

        // left_more_tab_index is the tab to the left of the leftmost visible tab
        let left_more_tab_index = left_count.saturating_sub(1);
        let collapsed_left = left_more_message(
            left_count,
            palette,
            tab_separator(capabilities),
            left_more_tab_index,
        );
        // right_more_tab_index is the tab to the right of the rightmost visible tab
        let right_more_tab_index = left_count + tabs_to_render.len();
        let collapsed_right = right_more_message(
            right_count,
            palette,
            tab_separator(capabilities),
            right_more_tab_index,
        );

        let total_size = collapsed_left.len + middle_size + collapsed_right.len;

        if total_size > cols {
            // break and dont add collapsed tabs to tabs_to_render, they will not fit
            break;
        }

        let left = if let Some(tab) = tabs_before_active.last() {
            tab.len
        } else {
            usize::MAX
        };

        let right = if let Some(tab) = tabs_after_active.first() {
            tab.len
        } else {
            usize::MAX
        };

        // total size is shortened if the next tab to be added is the last one, as that will remove the collapsed tab
        let size_by_adding_left =
            left.saturating_add(total_size)
                .saturating_sub(if left_count == 1 {
                    collapsed_left.len
                } else {
                    0
                });
        let size_by_adding_right =
            right
                .saturating_add(total_size)
                .saturating_sub(if right_count == 1 {
                    collapsed_right.len
                } else {
                    0
                });

        let left_fits = size_by_adding_left <= cols;
        let right_fits = size_by_adding_right <= cols;
        // active tab is kept in the middle by adding to the side that
        // has less width, or if the tab on the other side doesn't fit
        if (total_left <= total_right || !right_fits) && left_fits {
            // add left tab
            let tab = tabs_before_active.pop().unwrap();
            middle_size += tab.len;
            total_left += tab.len;
            tabs_to_render.insert(0, tab);
        } else if right_fits {
            // add right tab
            let tab = tabs_after_active.remove(0);
            middle_size += tab.len;
            total_right += tab.len;
            tabs_to_render.push(tab);
        } else {
            // there's either no space to add more tabs or no more tabs to add, so we're done
            tabs_to_render.insert(0, collapsed_left);
            tabs_to_render.push(collapsed_right);
            break;
        }
    }
}

fn left_more_message(
    tab_count_to_the_left: usize,
    palette: Palette,
    separator: &str,
    tab_index: usize,
) -> LinePart {
    if tab_count_to_the_left == 0 {
        return LinePart::default();
    }
    let more_text = if tab_count_to_the_left < 10000 {
        format!(" ← +{} ", tab_count_to_the_left)
    } else {
        " ← +many ".to_string()
    };
    // 238
    // chars length plus separator length on both sides
    let more_text_len = more_text.width() + 2 * separator.width();
    let (text_color, sep_color) = match palette.theme_hue {
        ThemeHue::Dark => (palette.white, palette.black),
        ThemeHue::Light => (palette.black, palette.white),
    };
    let left_separator = style!(sep_color, palette.orange).paint(separator);
    let more_styled_text = style!(text_color, palette.orange).bold().paint(more_text);
    let right_separator = style!(palette.orange, sep_color).paint(separator);
    let more_styled_text =
        ANSIStrings(&[left_separator, more_styled_text, right_separator]).to_string();
    LinePart {
        part: more_styled_text,
        len: more_text_len,
        tab_index: Some(tab_index),
    }
}

fn right_more_message(
    tab_count_to_the_right: usize,
    palette: Palette,
    separator: &str,
    tab_index: usize,
) -> LinePart {
    if tab_count_to_the_right == 0 {
        return LinePart::default();
    };
    let more_text = if tab_count_to_the_right < 10000 {
        format!(" +{} → ", tab_count_to_the_right)
    } else {
        " +many → ".to_string()
    };
    // chars length plus separator length on both sides
    let more_text_len = more_text.width() + 2 * separator.width();
    let (text_color, sep_color) = match palette.theme_hue {
        ThemeHue::Dark => (palette.white, palette.black),
        ThemeHue::Light => (palette.black, palette.white),
    };
    let left_separator = style!(sep_color, palette.orange).paint(separator);
    let more_styled_text = style!(text_color, palette.orange).bold().paint(more_text);
    let right_separator = style!(palette.orange, sep_color).paint(separator);
    let more_styled_text =
        ANSIStrings(&[left_separator, more_styled_text, right_separator]).to_string();
    LinePart {
        part: more_styled_text,
        len: more_text_len,
        tab_index: Some(tab_index),
    }
}

fn tab_line_prefix(
    session_name: Option<&str>,
    mode: InputMode,
    palette: Palette,
    cols: usize,
) -> Vec<LinePart> {
    let prefix_text = " Zellij ".to_string();

    let prefix_text_len = prefix_text.chars().count();
    let text_color = match palette.theme_hue {
        ThemeHue::Dark => palette.white,
        ThemeHue::Light => palette.black,
    };
    let bg_color = match palette.theme_hue {
        ThemeHue::Dark => palette.black,
        ThemeHue::Light => palette.white,
    };

    let locked_mode_color = palette.magenta;
    let normal_mode_color = palette.green;
    let other_modes_color = palette.orange;

    let prefix_styled_text = style!(text_color, bg_color).bold().paint(prefix_text);
    let mut parts = vec![LinePart {
        part: prefix_styled_text.to_string(),
        len: prefix_text_len,
        tab_index: None,
    }];
    if let Some(name) = session_name {
        let name_part = format!("({}) ", name);
        let name_part_len = name_part.width();
        let text_color = match palette.theme_hue {
            ThemeHue::Dark => palette.white,
            ThemeHue::Light => palette.black,
        };
        let name_part_styled_text = style!(text_color, bg_color).bold().paint(name_part);
        if cols.saturating_sub(prefix_text_len) >= name_part_len {
            parts.push(LinePart {
                part: name_part_styled_text.to_string(),
                len: name_part_len,
                tab_index: None,
            })
        }
    }
    let mode_part = format!("{:?}", mode).to_uppercase();
    let mode_part_padded = format!("{:^8}", mode_part);
    let mode_part_len = mode_part_padded.width();
    let mode_part_styled_text = if mode == InputMode::Locked {
        style!(locked_mode_color, bg_color)
            .bold()
            .paint(mode_part_padded)
    } else if mode == InputMode::Normal {
        style!(normal_mode_color, bg_color)
            .bold()
            .paint(mode_part_padded)
    } else {
        style!(other_modes_color, bg_color)
            .bold()
            .paint(mode_part_padded)
    };
    if cols.saturating_sub(prefix_text_len) >= mode_part_len {
        parts.push(LinePart {
            part: format!("{}", mode_part_styled_text),
            len: mode_part_len,
            tab_index: None,
        })
    }
    parts
}

pub fn tab_separator(capabilities: PluginCapabilities) -> &'static str {
    if !capabilities.arrow_fonts {
        ARROW_SEPARATOR
    } else {
        ""
    }
}

pub fn tab_line(
    session_name: Option<&str>,
    mut all_tabs: Vec<LinePart>,
    active_tab_index: usize,
    cols: usize,
    palette: Palette,
    capabilities: PluginCapabilities,
    hide_session_name: bool,
    mode: InputMode,
    active_swap_layout_name: &Option<String>,
    is_swap_layout_dirty: bool,
) -> Vec<LinePart> {
    let mut tabs_after_active = all_tabs.split_off(active_tab_index);
    let mut tabs_before_active = all_tabs;
    let active_tab = if !tabs_after_active.is_empty() {
        tabs_after_active.remove(0)
    } else {
        tabs_before_active.pop().unwrap()
    };
    let mut prefix = match hide_session_name {
        true => tab_line_prefix(None, mode, palette, cols),
        false => tab_line_prefix(session_name, mode, palette, cols),
    };
    let prefix_len = get_current_title_len(&prefix);

    // if active tab alone won't fit in cols, don't draw any tabs
    if prefix_len + active_tab.len > cols {
        return prefix;
    }

    let mut tabs_to_render = vec![active_tab];

    populate_tabs_in_tab_line(
        &mut tabs_before_active,
        &mut tabs_after_active,
        &mut tabs_to_render,
        cols.saturating_sub(prefix_len),
        palette,
        capabilities,
    );
    prefix.append(&mut tabs_to_render);

    let current_title_len = get_current_title_len(&prefix);
    if current_title_len < cols {
        let mut remaining_space = cols - current_title_len;
        if let Some(swap_layout_status) = swap_layout_status(
            remaining_space,
            active_swap_layout_name,
            is_swap_layout_dirty,
            mode,
            &palette,
            tab_separator(capabilities),
        ) {
            remaining_space -= swap_layout_status.len;
            let mut buffer = String::new();
            for _ in 0..remaining_space {
                buffer.push_str(&style!(palette.black, palette.black).paint(" ").to_string());
            }
            prefix.push(LinePart {
                part: buffer,
                len: remaining_space,
                tab_index: None,
            });
            prefix.push(swap_layout_status);
        }
    }

    prefix
}

fn swap_layout_status(
    max_len: usize,
    swap_layout_name: &Option<String>,
    is_swap_layout_damaged: bool,
    input_mode: InputMode,
    palette: &Palette,
    separator: &str,
) -> Option<LinePart> {
    match swap_layout_name {
        Some(swap_layout_name) => {
            let mut swap_layout_name = format!(" {} ", swap_layout_name);
            swap_layout_name.make_ascii_uppercase();
            let swap_layout_name_len = swap_layout_name.len() + 3;

            let (prefix_separator, swap_layout_name, suffix_separator) =
                if input_mode == InputMode::Locked {
                    (
                        style!(palette.black, palette.fg).paint(separator),
                        style!(palette.black, palette.fg)
                            .italic()
                            .paint(&swap_layout_name),
                        style!(palette.fg, palette.black).paint(separator),
                    )
                } else if is_swap_layout_damaged {
                    (
                        style!(palette.black, palette.fg).paint(separator),
                        style!(palette.black, palette.fg)
                            .bold()
                            .paint(&swap_layout_name),
                        style!(palette.fg, palette.black).paint(separator),
                    )
                } else {
                    (
                        style!(palette.black, palette.green).paint(separator),
                        style!(palette.black, palette.green)
                            .bold()
                            .paint(&swap_layout_name),
                        style!(palette.green, palette.black).paint(separator),
                    )
                };
            let swap_layout_indicator = format!(
                "{}{}{}",
                prefix_separator, swap_layout_name, suffix_separator
            );
            let (part, full_len) = (format!("{}", swap_layout_indicator), swap_layout_name_len);
            let short_len = swap_layout_name_len + 1; // 1 is the space between
            if full_len <= max_len {
                Some(LinePart {
                    part,
                    len: full_len,
                    tab_index: None,
                })
            } else if short_len <= max_len && input_mode != InputMode::Locked {
                Some(LinePart {
                    part: swap_layout_indicator,
                    len: short_len,
                    tab_index: None,
                })
            } else {
                None
            }
        },
        None => None,
    }
}
