use zellij_tile::prelude::*;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;
use crate::state::ROOT;

pub fn render_instruction_line(y: usize, max_cols: usize) {
    let text = "Help: Type or select path <ENTER> when done, autocomplete with <TAB> go back with <Ctrl c>, reset with /";
    let text = Text::new(text)
        .color_range(3, 26..33)
        .color_range(3, 63..68)
        .color_range(3, 82..90)
        .color_range(3, 103..104);
    print_text_with_coordinates(text, 0, y, None, None);
}

// returns the list (start_index, selected_index_in_range, end_index)
pub fn calculate_list_bounds(result_count: usize, max_result_count: usize, selected_index_in_all_results: Option<usize>) -> (usize, Option<usize>, usize) {
    match selected_index_in_all_results {
        Some(selected_index_in_all_results) => {
            let mut room_in_list = max_result_count;
            let mut start_index = selected_index_in_all_results;
            let mut end_index = selected_index_in_all_results + 1;
            let mut alternate = false;
            loop {
                if room_in_list == 0 {
                    break;
                }
                if !alternate && start_index > 0 {
                    start_index -= 1;
                    room_in_list -= 1;
                } else if alternate && end_index < result_count {
                    end_index += 1;
                    room_in_list -= 1;
                } else if start_index > 0 {
                    start_index -= 1;
                    room_in_list -= 1;
                } else if end_index < result_count {
                    end_index += 1;
                    room_in_list -= 1;
                } else {
                    break;
                }
                alternate = !alternate;
            }
            (start_index, Some(selected_index_in_all_results), end_index)
        },
        None => (0, None, max_result_count + 1)
    }
}

pub fn render_current_path(initial_cwd: &PathBuf, path: &PathBuf, search_term: &str) {
    let prompt = "PATH: ";
    let initial_cwd = if initial_cwd == &PathBuf::from("/") { "".to_owned() } else { initial_cwd.display().to_string() };
    let mut path = path.strip_prefix(ROOT).unwrap_or_else(|_| path).display().to_string();
    if !path.is_empty() {
        path = format!("{}/", path);
    }
    let prompt_len = prompt.width();
    let initial_cwd_len = std::cmp::max(initial_cwd.width(), 1);
    let path_len = path.width();
    let search_term_len = search_term.width();
    let current_path = Text::new(format!("{}{}/{}{}_", prompt, initial_cwd, path, search_term))
        .color_range(2, 0..prompt_len)
        .color_range(0, prompt_len..prompt_len + initial_cwd_len + path_len)
        .color_range(3, prompt_len + initial_cwd_len + path_len..prompt_len + initial_cwd_len + path_len + search_term_len + 1);
    print_text(current_path);
    println!();
    println!();
}

