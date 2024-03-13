use zellij_tile::prelude::*;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;
use crate::state::ROOT;

pub fn render_instruction_line(y: usize, max_cols: usize) {
    let text = "Help: go back with <Ctrl c>, reset with /, <Ctrl e> - toggle hidden files";
    let text = Text::new(text)
        .color_range(3, 19..27)
        .color_range(3, 40..41)
        .color_range(3, 43..51);
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

pub fn render_search_term(search_term: &str) {
    let prompt = "FIND: ";
    let text = Text::new(format!("{}{}_", prompt, search_term))
        .color_range(2, 0..prompt.len())
        .color_range(3, prompt.len()..);
    print_text(text);
    println!("")
}

pub fn render_current_path(
    initial_cwd: &PathBuf,
    path: &PathBuf,
    path_is_dir: bool,
    handling_filepick: bool,
) {
    let prompt = "PATH: ";
    let initial_cwd = if initial_cwd == &PathBuf::from("/") { "".to_owned() } else { initial_cwd.display().to_string() };
    let mut path = path.strip_prefix(ROOT).unwrap_or_else(|_| path).display().to_string();
    if !path.is_empty() && path_is_dir {
        path = format!("{}/", path);
    }
    let prompt_len = prompt.width();
    let initial_cwd_len = std::cmp::max(initial_cwd.width(), 1);
    let path_len = path.width();
    let enter_tip = if handling_filepick {
        "Select"
    } else if path_is_dir {
        "Open terminal here"
    } else {
        "Open in editor"
    };
    let path_end = prompt_len + initial_cwd_len + path_len;
    let current_path = Text::new(format!("{}{}/{} (<ENTER> - {})", prompt, initial_cwd, path, enter_tip))
        .color_range(2, 0..prompt_len)
        .color_range(0, prompt_len..path_end)
        .color_range(3, path_end + 3..path_end + 10);
    print_text(current_path);
    println!();
    println!();
}
