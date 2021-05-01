use ansi_term::ANSIStrings;

use crate::{LinePart, ARROW_SEPARATOR};
use zellij_tile::prelude::*;

fn get_current_title_len(current_title: &[LinePart]) -> usize {
    current_title
        .iter()
        .fold(0, |acc, title_part| acc + title_part.len)
}

fn populate_tabs_in_tab_line(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_after_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    cols: usize,
) {
    let mut take_next_tab_from_tabs_after = true;
    loop {
        if tabs_before_active.is_empty() && tabs_after_active.is_empty() {
            break;
        }
        let current_title_len = get_current_title_len(&tabs_to_render);
        if current_title_len >= cols {
            break;
        }
        let should_take_next_tab = take_next_tab_from_tabs_after;
        let can_take_next_tab = !tabs_after_active.is_empty()
            && tabs_after_active.get(0).unwrap().len + current_title_len <= cols;
        let can_take_previous_tab = !tabs_before_active.is_empty()
            && tabs_before_active.last().unwrap().len + current_title_len <= cols;
        if should_take_next_tab && can_take_next_tab {
            let next_tab = tabs_after_active.remove(0);
            tabs_to_render.push(next_tab);
            take_next_tab_from_tabs_after = false;
        } else if can_take_previous_tab {
            let previous_tab = tabs_before_active.pop().unwrap();
            tabs_to_render.insert(0, previous_tab);
            take_next_tab_from_tabs_after = true;
        } else if can_take_next_tab {
            let next_tab = tabs_after_active.remove(0);
            tabs_to_render.push(next_tab);
            take_next_tab_from_tabs_after = false;
        } else {
            break;
        }
    }
}

fn left_more_message(tab_count_to_the_left: usize, palette: Palette) -> LinePart {
    if tab_count_to_the_left == 0 {
        return LinePart {
            part: String::new(),
            len: 0,
        };
    }
    let more_text = if tab_count_to_the_left < 10000 {
        format!(" ← +{} ", tab_count_to_the_left)
    } else {
        " ← +many ".to_string()
    };
    // 238
    let more_text_len = more_text.chars().count() + 2; // 2 for the arrows
    let left_separator = style!(palette.fg, palette.orange).paint(ARROW_SEPARATOR);
    let more_styled_text = style!(palette.fg, palette.orange).bold().paint(more_text);
    let right_separator = style!(palette.orange, palette.bg).paint(ARROW_SEPARATOR);
    let more_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, more_styled_text, right_separator,])
    );
    LinePart {
        part: more_styled_text,
        len: more_text_len,
    }
}

fn right_more_message(tab_count_to_the_right: usize, palette: Palette) -> LinePart {
    if tab_count_to_the_right == 0 {
        return LinePart {
            part: String::new(),
            len: 0,
        };
    };
    let more_text = if tab_count_to_the_right < 10000 {
        format!(" +{} → ", tab_count_to_the_right)
    } else {
        " +many → ".to_string()
    };
    let more_text_len = more_text.chars().count() + 1; // 2 for the arrow
    let left_separator = style!(palette.fg, palette.orange).paint(ARROW_SEPARATOR);
    let more_styled_text = style!(palette.fg, palette.orange).bold().paint(more_text);
    let right_separator = style!(palette.orange, palette.bg).paint(ARROW_SEPARATOR);
    let more_styled_text = format!(
        "{}",
        ANSIStrings(&[left_separator, more_styled_text, right_separator,])
    );
    LinePart {
        part: more_styled_text,
        len: more_text_len,
    }
}

fn add_previous_tabs_msg(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    title_bar: &mut Vec<LinePart>,
    cols: usize,
    palette: Palette,
) {
    while get_current_title_len(&tabs_to_render)
        + left_more_message(tabs_before_active.len(), palette).len
        >= cols
    {
        tabs_before_active.push(tabs_to_render.remove(0));
    }
    let left_more_message = left_more_message(tabs_before_active.len(), palette);
    title_bar.push(left_more_message);
}

fn add_next_tabs_msg(
    tabs_after_active: &mut Vec<LinePart>,
    title_bar: &mut Vec<LinePart>,
    cols: usize,
    palette: Palette,
) {
    while get_current_title_len(&title_bar)
        + right_more_message(tabs_after_active.len(), palette).len
        >= cols
    {
        tabs_after_active.insert(0, title_bar.pop().unwrap());
    }
    let right_more_message = right_more_message(tabs_after_active.len(), palette);
    title_bar.push(right_more_message);
}

fn tab_line_prefix(palette: Palette) -> LinePart {
    let prefix_text = " Zellij ".to_string();
    let prefix_text_len = prefix_text.chars().count();
    let prefix_styled_text = style!(palette.fg, palette.bg).bold().paint(prefix_text);
    LinePart {
        part: format!("{}", prefix_styled_text),
        len: prefix_text_len,
    }
}

pub fn tab_line(
    mut all_tabs: Vec<LinePart>,
    active_tab_index: usize,
    cols: usize,
    palette: Palette,
) -> Vec<LinePart> {
    let mut tabs_to_render: Vec<LinePart> = vec![];
    let mut tabs_after_active = all_tabs.split_off(active_tab_index);
    let mut tabs_before_active = all_tabs;
    let active_tab = if !tabs_after_active.is_empty() {
        tabs_after_active.remove(0)
    } else {
        tabs_before_active.pop().unwrap()
    };
    tabs_to_render.push(active_tab);

    let prefix = tab_line_prefix(palette);
    populate_tabs_in_tab_line(
        &mut tabs_before_active,
        &mut tabs_after_active,
        &mut tabs_to_render,
        cols - prefix.len,
    );

    let mut tab_line: Vec<LinePart> = vec![];
    if !tabs_before_active.is_empty() {
        add_previous_tabs_msg(
            &mut tabs_before_active,
            &mut tabs_to_render,
            &mut tab_line,
            cols - prefix.len,
            palette,
        );
    }
    tab_line.append(&mut tabs_to_render);
    if !tabs_after_active.is_empty() {
        add_next_tabs_msg(
            &mut tabs_after_active,
            &mut tab_line,
            cols - prefix.len,
            palette,
        );
    }
    tab_line.insert(0, prefix);
    tab_line
}
