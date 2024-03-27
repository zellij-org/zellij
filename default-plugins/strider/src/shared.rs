use crate::state::Mode;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

pub fn render_instruction_tip(y: usize, max_cols: usize) {
    if max_cols < 11 {
        return;
    }
    let text = "?: <Ctrl h>";
    let text = Text::new(text).color_range(3, 3..11);
    print_text_with_coordinates(text, 0, y, Some(max_cols), None);
}

enum HelpTextSize {
    Small,
    Medium,
    Large,
}

fn render_help_text(bind: &str, desc: &str, max_cols: usize, y: usize) {
    let len = bind.len();
    let padding = " ".repeat(max_cols.saturating_sub(len + desc.len()));
    let text = format!("{}{}{}", bind, padding, desc);
    let text = Text::new(text).color_range(3, 0..len);
    print_text_with_coordinates(text, 0, y, Some(max_cols), None)
}

pub fn render_instruction_line(max_cols: usize) {
    let text_size = if max_cols > 28 {
        HelpTextSize::Large
    } else if max_cols > 15 {
        HelpTextSize::Medium
    } else if max_cols > 8 {
        HelpTextSize::Small
    } else {
        return;
    };

    let bind = "<Ctrl c>";
    let desc = match text_size {
        HelpTextSize::Large => "Go back",
        HelpTextSize::Medium => "back",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 0);

    let bind = "/";
    let desc = match text_size {
        HelpTextSize::Large => "Go to root",
        HelpTextSize::Medium => "root",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 1);

    let bind = "<Ctrl e>";
    let desc = match text_size {
        HelpTextSize::Large => "Toggle hidden files",
        HelpTextSize::Medium => "hidden",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 2);

    let bind = "<Ctrl r>";
    let desc = match text_size {
        HelpTextSize::Large => "Rename / move file",
        HelpTextSize::Medium => "rename",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 3);

    let bind = "<Ctrl d>";
    let desc = match text_size {
        HelpTextSize::Large => "Delete file",
        HelpTextSize::Medium => "delete",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 4);

    let bind = "<Ctrl y>";
    let desc = match text_size {
        HelpTextSize::Large => "Copy & paste file",
        HelpTextSize::Medium => "copy",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 5);

    let bind = "<Ctrl a>";
    let desc = match text_size {
        HelpTextSize::Large => "Create new file",
        HelpTextSize::Medium => "create",
        _ => "",
    };
    render_help_text(bind, desc, max_cols, 6);
}

pub fn render_list_tip(y: usize, max_cols: usize) {
    let tip = Text::new(format!("(<↓↑> - Navigate, <TAB> - Select)"))
        .color_range(3, 1..5)
        .color_range(3, 18..23);
    print_text_with_coordinates(tip, 0, y, Some(max_cols), None);
}

// returns the list (start_index, selected_index_in_range, end_index)
pub fn calculate_list_bounds(
    result_count: usize,
    max_result_count: usize,
    selected_index_in_all_results: Option<usize>,
) -> (usize, Option<usize>, usize) {
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
        None => (0, None, max_result_count + 1),
    }
}

pub fn render_search_term(search_term: &str, mode: &Mode) {
    let prompt = match mode {
        Mode::Create => "CREATE: ",
        Mode::Copy => "PASTE: ",
        Mode::Delete => "CONFIRM (y): ",
        Mode::Move => "EDIT: ",
        _ => "FIND: ",
    };
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
    max_cols: usize,
) {
    let prompt = "PATH: ";
    let current_path = initial_cwd.join(path);
    let current_path = current_path.display().to_string();
    let prompt_len = prompt.width();
    let current_path_len = current_path.width();

    let enter_tip = if handling_filepick {
        "Select"
    } else if path_is_dir {
        "Open terminal here"
    } else {
        "Open in editor"
    };
    if max_cols > prompt_len + current_path_len + enter_tip.width() + 13 {
        let path_end = prompt_len + current_path_len;
        let current_path = Text::new(format!(
            "{}{} (<ENTER> - {})",
            prompt, current_path, enter_tip
        ))
        .color_range(2, 0..prompt_len)
        .color_range(0, prompt_len..path_end)
        .color_range(3, path_end + 2..path_end + 9);
        print_text(current_path);
    } else {
        let max_path_len = max_cols
            .saturating_sub(prompt_len)
            .saturating_sub(8)
            .saturating_sub(prompt_len);
        let current_path = if current_path_len <= max_path_len {
            current_path
        } else {
            truncate_path(
                initial_cwd.join(path),
                current_path_len.saturating_sub(max_path_len),
            )
        };
        let current_path_len = current_path.width();
        let path_end = prompt_len + current_path_len;
        let current_path = Text::new(format!("{}{} <ENTER>", prompt, current_path))
            .color_range(2, 0..prompt_len)
            .color_range(0, prompt_len..path_end)
            .color_range(3, path_end + 1..path_end + 9);
        print_text(current_path);
    }
    println!();
    println!();
}

fn truncate_path(path: PathBuf, mut char_count_to_remove: usize) -> String {
    let mut truncated = String::new();
    let component_count = path.iter().count();
    for (i, component) in path.iter().enumerate() {
        let mut component_str = component.to_string_lossy().to_string();
        if char_count_to_remove > 0 {
            truncated.push(component_str.remove(0));
            if i != 0 && i + 1 != component_count {
                truncated.push('/');
            }
            char_count_to_remove = char_count_to_remove.saturating_sub(component_str.width() + 1);
        } else {
            truncated.push_str(&component_str);
            if i != 0 && i + 1 != component_count {
                truncated.push('/');
            }
        }
    }
    truncated
}
