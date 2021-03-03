use colored::*;

use crate::{LinePart, ARROW_SEPARATOR};

fn get_current_title_len(current_title: &[LinePart]) -> usize {
   current_title 
        .iter()
        .fold(0, |acc, title_part| acc + title_part.len)
}

fn populate_tabs_in_tab_line(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_after_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    cols: usize
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
        let can_take_next_tab = !tabs_after_active.is_empty() &&
            tabs_after_active.get(0).unwrap().len + current_title_len <= cols;
        let can_take_previous_tab = !tabs_before_active.is_empty() &&
            tabs_before_active.last().unwrap().len + current_title_len <= cols;
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

fn left_more_message(tab_count_to_the_left: usize) -> LinePart {
    if tab_count_to_the_left == 0 {
        return LinePart {
            part: String::new(),
            len: 0
        };
    }
    let more_text = if tab_count_to_the_left < 10000 {
        format!(" ← +{} ", tab_count_to_the_left)
    } else {
        format!(" ← +many ")
    };
    let more_styled_text = format!("{}{}",
        more_text.black().on_yellow(),
        ARROW_SEPARATOR.yellow().on_black(),
    );
    LinePart {
        part: more_styled_text,
        len: more_text.chars().count() + 1 // 1 for the arrow
    }
}

fn right_more_message(tab_count_to_the_right: usize) -> LinePart {
    if tab_count_to_the_right == 0 {
        return LinePart {
            part: String::new(),
            len: 0
        };
    };
    let more_text = if tab_count_to_the_right < 10000 {
        format!(" +{} → ", tab_count_to_the_right)
    } else {
        format!(" +many → ")
    };
    let more_styled_text = format!("{}{}{}",
        ARROW_SEPARATOR.black().on_yellow(),
        more_text.black().on_yellow(),
        ARROW_SEPARATOR.yellow().on_black(),
    );
    LinePart {
        part: more_styled_text,
        len: more_text.chars().count() + 2 // 2 for the arrows
    }
}

fn add_previous_tabs_msg(
    tabs_before_active: &mut Vec<LinePart>,
    tabs_to_render: &mut Vec<LinePart>,
    title_bar: &mut Vec<LinePart>,
    cols: usize
) {
    while get_current_title_len(&tabs_to_render) +
        // get_tabs_before_len(tabs_before_active.len()) >= cols {
        left_more_message(tabs_before_active.len()).len >= cols {
        tabs_before_active.push(tabs_to_render.remove(0));
    }
    let left_more_message = left_more_message(tabs_before_active.len());
    title_bar.push(left_more_message);
}

fn add_next_tabs_msg(
    tabs_after_active: &mut Vec<LinePart>,
    title_bar: &mut Vec<LinePart>,
    cols: usize,
) {
    while get_current_title_len(&title_bar) +
        // get_tabs_after_len(tabs_after_active.len()) >= cols {
        right_more_message(tabs_after_active.len()).len >= cols {
        tabs_after_active.insert(0, title_bar.pop().unwrap());
    }
    let right_more_message = right_more_message(tabs_after_active.len());
    title_bar.push(right_more_message);
}

pub fn tab_line(mut all_tabs: Vec<LinePart>, active_tab_index: usize, cols: usize) -> Vec<LinePart> {
    let mut tabs_to_render: Vec<LinePart> = vec![];
    let mut tabs_after_active = all_tabs.split_off(active_tab_index);
    let mut tabs_before_active = all_tabs;
    let active_tab = if !tabs_after_active.is_empty() {
        tabs_after_active.remove(0)
    } else {
        tabs_before_active.pop().unwrap()
    };
    tabs_to_render.push(active_tab);

    populate_tabs_in_tab_line(
        &mut tabs_before_active,
        &mut tabs_after_active,
        &mut tabs_to_render,
        cols
    );

    let mut tab_line: Vec<LinePart> = vec![];
    if !tabs_before_active.is_empty() {
        add_previous_tabs_msg(
            &mut tabs_before_active,
            &mut tabs_to_render,
            &mut tab_line,
            cols
        );
    }
    tab_line.append(&mut tabs_to_render);
    if !tabs_after_active.is_empty() {
        add_next_tabs_msg(&mut tabs_after_active, &mut tab_line, cols);
    }
    tab_line
}
