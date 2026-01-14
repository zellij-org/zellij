use ansi_term::ANSIStrings;
use unicode_width::UnicodeWidthStr;

use crate::{LinePart, ARROW_SEPARATOR};
use zellij_tile::prelude::actions::Action;
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
    palette: Styling,
    capabilities: PluginCapabilities,
) {
    let mut middle_size = get_current_title_len(tabs_to_render);

    let mut total_left = 0;
    let mut total_right = 0;
    loop {
        let left_count = tabs_before_active.len();
        let right_count = tabs_after_active.len();

        // left_more_tab_index is first tab to the left of the leftmost visible tab
        let left_more_tab_index = left_count.saturating_sub(1);
        let collapsed_left = left_more_message(
            left_count,
            palette,
            tab_separator(capabilities),
            left_more_tab_index,
        );

        // right_more_tab_index is the first tab to the right of the rightmost visible tab
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
    palette: Styling,
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
    let (text_color, sep_color) = (
        palette.ribbon_unselected.base,
        palette.text_unselected.background,
    );
    let left_separator = style!(sep_color, palette.ribbon_unselected.background).paint(separator);
    let more_styled_text = style!(text_color, palette.ribbon_unselected.background)
        .bold()
        .paint(more_text);
    let right_separator = style!(palette.ribbon_unselected.background, sep_color).paint(separator);
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
    palette: Styling,
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
    let (text_color, sep_color) = (
        palette.ribbon_unselected.base,
        palette.text_unselected.background,
    );
    let left_separator = style!(sep_color, palette.ribbon_unselected.background).paint(separator);
    let more_styled_text = style!(text_color, palette.ribbon_unselected.background)
        .bold()
        .paint(more_text);
    let right_separator = style!(palette.ribbon_unselected.background, sep_color).paint(separator);
    let more_styled_text =
        ANSIStrings(&[left_separator, more_styled_text, right_separator]).to_string();
    LinePart {
        part: more_styled_text,
        len: more_text_len,
        tab_index: Some(tab_index),
    }
}

fn tab_line_prefix(session_name: Option<&str>, palette: Styling, cols: usize) -> Vec<LinePart> {
    let prefix_text = " Zellij ".to_string();

    let running_text_len = prefix_text.chars().count();
    let text_color = palette.text_unselected.base;
    let bg_color = palette.text_unselected.background;
    let prefix_styled_text = style!(text_color, bg_color).bold().paint(prefix_text);
    let mut parts = vec![LinePart {
        part: prefix_styled_text.to_string(),
        len: running_text_len,
        tab_index: None,
    }];
    if let Some(name) = session_name {
        let name_part = format!("({}) ", name);
        let name_part_len = name_part.width();
        let text_color = palette.text_unselected.base;
        let name_part_styled_text = style!(text_color, bg_color).bold().paint(name_part);
        if cols.saturating_sub(running_text_len) >= name_part_len {
            parts.push(LinePart {
                part: name_part_styled_text.to_string(),
                len: name_part_len,
                tab_index: None,
            })
        }
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
    palette: Styling,
    capabilities: PluginCapabilities,
    hide_session_name: bool,
    tab_info: Option<&TabInfo>,
    mode_info: &ModeInfo,
    hide_swap_layout_indicator: bool,
    background: &PaletteColor,
) -> Vec<LinePart> {
    let mut tabs_after_active = all_tabs.split_off(active_tab_index);
    let mut tabs_before_active = all_tabs;
    let active_tab = if !tabs_after_active.is_empty() {
        tabs_after_active.remove(0)
    } else {
        tabs_before_active.pop().unwrap()
    };
    let mut prefix = match hide_session_name {
        true => tab_line_prefix(None, palette, cols),
        false => tab_line_prefix(session_name, palette, cols),
    };

    let mut swap_layout_indicator = if hide_swap_layout_indicator {
        None
    } else {
        tab_info.and_then(|tab_info| {
            swap_layout_status(
                &tab_info.active_swap_layout_name,
                tab_info.is_swap_layout_dirty,
                mode_info,
                !capabilities.arrow_fonts,
            )
        })
    };

    let non_tab_len =
        get_current_title_len(&prefix) + swap_layout_indicator.as_ref().map(|s| s.len).unwrap_or(0);

    let mut tabs_to_render = vec![active_tab];

    populate_tabs_in_tab_line(
        &mut tabs_before_active,
        &mut tabs_after_active,
        &mut tabs_to_render,
        cols.saturating_sub(non_tab_len),
        palette,
        capabilities,
    );
    prefix.append(&mut tabs_to_render);
    prefix.append(&mut vec![LinePart {
        part: match background {
            PaletteColor::Rgb((r, g, b)) => format!("\u{1b}[48;2;{};{};{}m\u{1b}[0K", r, g, b),
            PaletteColor::EightBit(color) => format!("\u{1b}[48;5;{}m\u{1b}[0K", color),
        },
        len: 0,
        tab_index: None,
    }]);

    if let Some(mut swap_layout_indicator) = swap_layout_indicator.take() {
        let remaining_space = cols
            .saturating_sub(prefix.iter().fold(0, |len, part| len + part.len))
            .saturating_sub(swap_layout_indicator.len);
        let mut padding = String::new();
        let mut padding_len = 0;
        for _ in 0..remaining_space {
            padding.push_str(
                &style!(
                    palette.text_unselected.background,
                    palette.text_unselected.background
                )
                .paint(" ")
                .to_string(),
            );
            padding_len += 1;
        }
        swap_layout_indicator.part = format!("{}{}", padding, swap_layout_indicator.part);
        swap_layout_indicator.len += padding_len;
        prefix.push(swap_layout_indicator);
    }

    prefix
}

fn swap_layout_status(
    swap_layout_name: &Option<String>,
    is_swap_layout_dirty: bool,
    mode_info: &ModeInfo,
    supports_arrow_fonts: bool,
) -> Option<LinePart> {
    match swap_layout_name {
        Some(swap_layout_name) => {
            let mode_keybinds = mode_info.get_mode_keybinds();
            let prev_next_keys = action_key_group(
                &mode_keybinds,
                &[&[Action::PreviousSwapLayout], &[Action::NextSwapLayout]],
            );
            let mut text = style_key_with_modifier(&prev_next_keys, Some(0));
            text.append(&ribbon_as_line_part(
                &swap_layout_name.to_uppercase(),
                !is_swap_layout_dirty,
                supports_arrow_fonts,
            ));
            Some(text)
        },
        None => None,
    }
}

pub fn ribbon_as_line_part(text: &str, is_selected: bool, supports_arrow_fonts: bool) -> LinePart {
    let ribbon_text = if is_selected {
        Text::new(text).selected()
    } else {
        Text::new(text)
    };
    let part = serialize_ribbon(&ribbon_text);
    let mut len = text.width() + 2;
    if supports_arrow_fonts {
        len += 2;
    };
    LinePart {
        part,
        len,
        tab_index: None,
    }
}

pub fn style_key_with_modifier(keyvec: &[KeyWithModifier], color_index: Option<usize>) -> LinePart {
    if keyvec.is_empty() {
        return LinePart::default();
    }

    let common_modifiers = get_common_modifiers(keyvec.iter().collect());

    let no_common_modifier = common_modifiers.is_empty();
    let modifier_str = common_modifiers
        .iter()
        .map(|m| m.to_string())
        .collect::<Vec<_>>()
        .join("-");

    // Prints the keys
    let key = keyvec
        .iter()
        .map(|key| {
            if no_common_modifier || keyvec.len() == 1 {
                format!("{}", key)
            } else {
                format!("{}", key.strip_common_modifiers(&common_modifiers))
            }
        })
        .collect::<Vec<String>>();

    // Special handling of some pre-defined keygroups
    let key_string = key.join("");
    let key_separator = match &key_string[..] {
        "HJKL" => "",
        "hjkl" => "",
        "←↓↑→" => "",
        "←→" => "",
        "↓↑" => "",
        "[]" => "",
        _ => "|",
    };

    if no_common_modifier || key.len() == 1 {
        let key_string_text = format!(" {} ", key.join(key_separator));
        let text = if let Some(color_index) = color_index {
            Text::new(&key_string_text)
                .color_range(color_index, ..)
                .opaque()
        } else {
            Text::new(&key_string_text).opaque()
        };
        LinePart {
            part: serialize_text(&text),
            len: key_string_text.width(),
            ..Default::default()
        }
    } else {
        let key_string_without_modifier = format!("{}", key.join(key_separator));
        let key_string_text = format!(" {} <{}> ", modifier_str, key_string_without_modifier);
        let text = if let Some(color_index) = color_index {
            Text::new(&key_string_text)
                .color_range(color_index, ..modifier_str.width() + 1)
                .color_range(
                    color_index,
                    modifier_str.width() + 3
                        ..modifier_str.width() + 3 + key_string_without_modifier.width(),
                )
                .opaque()
        } else {
            Text::new(&key_string_text).opaque()
        };
        LinePart {
            part: serialize_text(&text),
            len: key_string_text.width(),
            ..Default::default()
        }
    }
}

pub fn get_common_modifiers(mut keyvec: Vec<&KeyWithModifier>) -> Vec<KeyModifier> {
    if keyvec.is_empty() {
        return vec![];
    }
    let mut common_modifiers = keyvec.pop().unwrap().key_modifiers.clone();
    for key in keyvec {
        common_modifiers = common_modifiers
            .intersection(&key.key_modifiers)
            .cloned()
            .collect();
    }
    common_modifiers.into_iter().collect()
}

pub fn action_key_group(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    actions: &[&[Action]],
) -> Vec<KeyWithModifier> {
    let mut ret = vec![];
    for action in actions {
        ret.extend(action_key(keymap, action));
    }
    ret
}

pub fn action_key(
    keymap: &[(KeyWithModifier, Vec<Action>)],
    action: &[Action],
) -> Vec<KeyWithModifier> {
    keymap
        .iter()
        .filter_map(|(key, acvec)| {
            let matching = acvec
                .iter()
                .zip(action)
                .filter(|(a, b)| a.shallow_eq(b))
                .count();

            if matching == acvec.len() && matching == action.len() {
                Some(key.clone())
            } else {
                None
            }
        })
        .collect::<Vec<KeyWithModifier>>()
}
