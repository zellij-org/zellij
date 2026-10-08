use ansi_term::ANSIStrings;
use std::collections::BTreeMap;
use unicode_width::UnicodeWidthStr;

use crate::session_indicator::SessionIndicator;
use crate::tab_bar::{LinePart, ARROW_SEPARATOR};
use zellij_tile::prelude::*;
use zellij_tile_utils::style;

fn get_current_title_len(current_title: &[LinePart]) -> usize {
    current_title.iter().map(|p| p.len).sum()
}

fn dim_style(bg: PaletteColor) -> ansi_term::Style {
    ansi_term::Style::new()
        .on(match bg {
            PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
            PaletteColor::EightBit(c) => ansi_term::Color::Fixed(c),
        })
        .dimmed()
}

fn populate_tabs_in_tab_line(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_after_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    cols: usize,
    palette: Styling,
    capabilities: PluginCapabilities,
    dimmed: bool,
) {
    let mut middle_size = get_current_title_len(tabs_to_render);

    let mut total_left = 0;
    let mut total_right = 0;
    loop {
        let left_count = tabs_before_active.len();
        let right_count = tabs_after_active.len();

        let left_more_tab_index = left_count.saturating_sub(1);
        let collapsed_left = left_more_message(
            left_count,
            palette,
            tab_separator(capabilities),
            left_more_tab_index,
            dimmed,
        );

        let right_more_tab_index = left_count + tabs_to_render.len();
        let collapsed_right = right_more_message(
            right_count,
            palette,
            tab_separator(capabilities),
            right_more_tab_index,
            dimmed,
        );

        let total_size = collapsed_left.len + middle_size + collapsed_right.len;

        if total_size > cols {
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
        if (total_left <= total_right || !right_fits) && left_fits {
            let tab = tabs_before_active.pop().unwrap();
            middle_size += tab.len;
            total_left += tab.len;
            tabs_to_render.insert(0, tab);
        } else if right_fits {
            let tab = tabs_after_active.remove(0);
            middle_size += tab.len;
            total_right += tab.len;
            tabs_to_render.push(tab);
        } else {
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
    dimmed: bool,
) -> LinePart {
    if tab_count_to_the_left == 0 {
        return LinePart::default();
    }
    let more_text = if tab_count_to_the_left < 10000 {
        format!(" ← +{} ", tab_count_to_the_left)
    } else {
        " ← +many ".to_string()
    };
    let more_text_len = more_text.width() + 2 * separator.width();
    let (text_color, sep_color) = (
        palette.ribbon_unselected.base,
        palette.text_unselected.background,
    );
    let text_style = if dimmed {
        style!(text_color, palette.ribbon_unselected.background).italic()
    } else {
        style!(text_color, palette.ribbon_unselected.background).bold()
    };
    let left_separator = style!(sep_color, palette.ribbon_unselected.background).paint(separator);
    let more_styled_text = text_style.paint(more_text);
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
    dimmed: bool,
) -> LinePart {
    if tab_count_to_the_right == 0 {
        return LinePart::default();
    };
    let more_text = if tab_count_to_the_right < 10000 {
        format!(" +{} → ", tab_count_to_the_right)
    } else {
        " +many → ".to_string()
    };
    let more_text_len = more_text.width() + 2 * separator.width();
    let (text_color, sep_color) = (
        palette.ribbon_unselected.base,
        palette.text_unselected.background,
    );
    let text_style = if dimmed {
        style!(text_color, palette.ribbon_unselected.background).italic()
    } else {
        style!(text_color, palette.ribbon_unselected.background).bold()
    };
    let left_separator = style!(sep_color, palette.ribbon_unselected.background).paint(separator);
    let more_styled_text = text_style.paint(more_text);
    let right_separator = style!(palette.ribbon_unselected.background, sep_color).paint(separator);
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
    palette: Styling,
    cols: usize,
    dimmed: bool,
    breadcrumb_ancestry: &[String],
) -> (Vec<LinePart>, Option<(usize, usize)>) {
    let prefix_text = " Zellij ".to_string();

    let running_text_len = prefix_text.chars().count();
    let text_color = palette.text_unselected.base;
    let bg_color = palette.text_unselected.background;
    let prefix_style = if dimmed {
        dim_style(bg_color)
    } else {
        style!(text_color, bg_color).bold()
    };
    let prefix_styled_text = prefix_style.paint(prefix_text);
    let mut parts = vec![LinePart {
        part: prefix_styled_text.to_string(),
        len: running_text_len,
        tab_index: None,
    }];
    let mut breadcrumb_range = None;
    if !breadcrumb_ancestry.is_empty() {
        if let Some(name) = session_name {
            let ancestor_text = format!("({} ▸ ", breadcrumb_ancestry.join(" ▸ "));
            let ancestor_len = ancestor_text.width();
            let name_text = name.to_string();
            let name_len = name_text.width();
            let closing_text = ") ".to_string();
            let closing_len = closing_text.width();
            let ancestor_style = if dimmed {
                dim_style(bg_color)
            } else {
                style!(text_color, bg_color).italic()
            };
            let name_style = if dimmed {
                dim_style(bg_color)
            } else {
                style!(palette.text_unselected.emphasis_0, bg_color).bold()
            };
            let closing_style = if dimmed {
                dim_style(bg_color)
            } else {
                style!(text_color, bg_color).bold()
            };
            if cols.saturating_sub(running_text_len) >= ancestor_len + name_len + closing_len {
                let ancestor_start = running_text_len;
                let ancestor_end = ancestor_start + ancestor_len;
                breadcrumb_range = Some((ancestor_start, ancestor_end));
                parts.push(LinePart {
                    part: ancestor_style.paint(ancestor_text).to_string(),
                    len: ancestor_len,
                    tab_index: None,
                });
                parts.push(LinePart {
                    part: name_style.paint(name_text).to_string(),
                    len: name_len,
                    tab_index: None,
                });
                parts.push(LinePart {
                    part: closing_style.paint(closing_text).to_string(),
                    len: closing_len,
                    tab_index: None,
                });
            }
        }
        return (parts, breadcrumb_range);
    }
    if let Some(name) = session_name {
        let name_part = format!("({}) ", name);
        let name_part_len = name_part.width();
        let name_part_styled_text = prefix_style.paint(name_part);
        if cols.saturating_sub(running_text_len) >= name_part_len {
            parts.push(LinePart {
                part: name_part_styled_text.to_string(),
                len: name_part_len,
                tab_index: None,
            })
        }
    }
    (parts, breadcrumb_range)
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
    session_indicator: &SessionIndicator,
    background: &PaletteColor,
    active_pane_scroll: Option<(usize, usize)>,
    hint: Option<&BTreeMap<usize, StyledText>>,
    is_alternate_tab: bool,
    new_tab_button_is_hovered: bool,
    dimmed: bool,
    breadcrumb_ancestry: &[String],
) -> (
    Vec<LinePart>,
    Option<(usize, usize)>,
    Option<(usize, usize)>,
    Option<(usize, usize)>,
) {
    let mut tabs_after_active = all_tabs.split_off(active_tab_index);
    let mut tabs_before_active = all_tabs;
    let active_tab = if !tabs_after_active.is_empty() {
        tabs_after_active.remove(0)
    } else {
        tabs_before_active.pop().unwrap()
    };
    let (mut prefix, breadcrumb_range) = match hide_session_name {
        true => tab_line_prefix(None, palette, cols, dimmed, &[]),
        false => tab_line_prefix(session_name, palette, cols, dimmed, breadcrumb_ancestry),
    };

    let prefix_len = get_current_title_len(&prefix);
    let indicator_budget = cols.saturating_sub(prefix_len + active_tab.len);
    let mut scroll_part = active_pane_scroll
        .filter(|(position, _)| *position > 0)
        .map(|scroll| scroll_status(scroll, palette, dimmed));
    let scroll_len = scroll_part.as_ref().map(|s| s.len + 1).unwrap_or(0);
    let mut session_indicator_text =
        match session_indicator.fitting_variant(indicator_budget.saturating_sub(scroll_len)) {
            Some(text) if scroll_part.is_some() => Some(text),
            _ => {
                let text = session_indicator.fitting_variant(indicator_budget);
                if text.is_some() {
                    scroll_part = None;
                }
                text
            },
        };
    let mut swap_layout_indicator = match session_indicator_text.as_ref() {
        Some(text) => Some(LinePart {
            part: String::new(),
            len: SessionIndicator::width_of(text)
                + scroll_part.as_ref().map(|s| s.len + 1).unwrap_or(0),
            tab_index: None,
        }),
        None => scroll_part.take(),
    };

    let hint_budget = cols.saturating_sub(prefix_len + active_tab.len);
    if let Some(hint_part) = hint.and_then(|hint| hint_line_part(hint, hint_budget, dimmed)) {
        swap_layout_indicator = Some(hint_part);
        session_indicator_text = None;
        scroll_part = None;
    }
    let indicator_len = swap_layout_indicator.as_ref().map(|s| s.len).unwrap_or(0);
    if indicator_len > 0 && prefix_len + active_tab.len + indicator_len > cols {
        swap_layout_indicator = None;
        session_indicator_text = None;
        scroll_part = None;
    }

    let non_tab_len = prefix_len + swap_layout_indicator.as_ref().map(|s| s.len).unwrap_or(0);

    let mut tabs_to_render = vec![active_tab];

    populate_tabs_in_tab_line(
        &mut tabs_before_active,
        &mut tabs_after_active,
        &mut tabs_to_render,
        cols.saturating_sub(non_tab_len),
        palette,
        capabilities,
        dimmed,
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

    let supports_arrow_fonts = !capabilities.arrow_fonts;
    let new_tab_button_background = if supports_arrow_fonts {
        if new_tab_button_is_hovered {
            palette.ribbon_unselected.emphasis_1
        } else {
            palette.ribbon_unselected.background
        }
    } else if is_alternate_tab {
        palette.ribbon_unselected.emphasis_1
    } else {
        palette.ribbon_unselected.background
    };
    let new_tab_button =
        new_tab_button_line_part(palette, capabilities, new_tab_button_background, dimmed);
    let used = prefix.iter().map(|p| p.len).sum::<usize>();
    let current_indicator_len = swap_layout_indicator.as_ref().map(|s| s.len).unwrap_or(0);
    let room = cols
        .saturating_sub(used)
        .saturating_sub(current_indicator_len);
    let new_tab_button_range = if room >= new_tab_button.len {
        let button_start = used;
        let button_end = button_start + new_tab_button.len;
        prefix.push(new_tab_button);
        Some((button_start, button_end))
    } else {
        None
    };

    let mut session_indicator_range = None;
    if let Some(mut swap_layout_indicator) = swap_layout_indicator.take() {
        let remaining_space = cols
            .saturating_sub(prefix.iter().fold(0, |len, part| len + part.len))
            .saturating_sub(swap_layout_indicator.len);
        if let Some(text) = session_indicator_text.as_ref() {
            let (scroll_text, scroll_width) = match scroll_part.as_ref() {
                Some(scroll) => (
                    format!(
                        "{}{}",
                        scroll.part,
                        style!(
                            palette.text_unselected.background,
                            palette.text_unselected.background
                        )
                        .paint(" ")
                    ),
                    scroll.len + 1,
                ),
                None => (String::new(), 0),
            };
            let start =
                prefix.iter().fold(0, |len, part| len + part.len) + remaining_space + scroll_width;
            swap_layout_indicator.part = format!(
                "{}{}",
                scroll_text,
                session_indicator.render(text, start, 0, capabilities.arrow_fonts)
            );
            session_indicator_range = Some((start, start + SessionIndicator::width_of(text)));
        }
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

    (
        prefix,
        new_tab_button_range,
        breadcrumb_range,
        session_indicator_range,
    )
}

fn hint_line_part(
    hint: &BTreeMap<usize, StyledText>,
    max_len: usize,
    dimmed: bool,
) -> Option<LinePart> {
    let fitting_variant = hint
        .range(..=max_len)
        .next_back()
        .map(|(_width, styled_text)| styled_text)?;
    let len = fitting_variant.text.width();
    let text = if dimmed {
        Text::from(fitting_variant.clone()).disabled()
    } else {
        Text::from(fitting_variant.clone())
    };
    let part = serialize_text(&text);
    Some(LinePart {
        part,
        len,
        tab_index: None,
    })
}

fn scroll_status(scroll: (usize, usize), palette: Styling, dimmed: bool) -> LinePart {
    let (position, length) = scroll;
    let text = format!("[ SCROLL {}/{} ]", position, length);
    let text_color = palette.text_unselected.base;
    let bg_color = palette.text_unselected.background;
    let text_style = if dimmed {
        style!(text_color, bg_color).italic()
    } else {
        style!(text_color, bg_color).bold()
    };
    let styled_text = text_style.paint(&text);
    LinePart {
        part: styled_text.to_string(),
        len: text.width(),
        tab_index: None,
    }
}

fn new_tab_button_line_part(
    palette: Styling,
    capabilities: PluginCapabilities,
    background_color: PaletteColor,
    dimmed: bool,
) -> LinePart {
    let separator = tab_separator(capabilities);
    let separator_width = separator.width();
    let foreground_color = palette.ribbon_unselected.base;
    let separator_fill_color = palette.text_unselected.background;
    let background_color = if dimmed {
        palette.ribbon_unselected.background
    } else {
        background_color
    };
    let text_style = if dimmed {
        style!(foreground_color, background_color).italic()
    } else {
        style!(foreground_color, background_color).bold()
    };
    let left_separator = style!(separator_fill_color, background_color).paint(separator);
    let right_separator = style!(background_color, separator_fill_color).paint(separator);
    let text = "+";
    let styled_text = text_style.paint(format!(" {} ", text));
    let part = ANSIStrings(&[left_separator, styled_text, right_separator]).to_string();
    let len = text.width() + (separator_width * 2) + 2;
    LinePart {
        part,
        len,
        tab_index: None,
    }
}
