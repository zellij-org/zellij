use crate::path_formatting::format_cwd;
use crate::state::{CommandStatus, State};
use crate::ui::truncation::{calculate_available_cmd_width, truncate_middle};
use std::path::PathBuf;
use zellij_tile::prelude::*;

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

const HELP_RUNNING_WITH_SELECTION: &str = "<Ctrl c> - interrupt, <Enter> - run from selected";
const HELP_RUNNING_NO_SELECTION: &str = "<Ctrl c> - interrupt, <↓↑> - navigate";
const HELP_STOPPED_WITH_SELECTION: &str =
    "<Ctrl w> - close all, <Enter> - run from selected, <e> - edit";
const HELP_STOPPED_NO_SELECTION: &str =
    "<Ctrl w> - close all, <↓↑> - navigate, <Enter> - run from first";

const HELP_ONE_PENDING_COMMAND: &str = "<Enter> - run, <Ctrl Enter> - add command";

const HELP_ALL_COMMANDS_PENDING: &str =
    "<Enter> - run, <Ctrl Enter> - add command, <↓↑> - navigate";
const HELP_ALL_COMMANDS_PENDING_WITH_SELECTION: &str =
    "<Enter> - run, <Ctrl Enter> - add command, <↓↑> - navigate, <e> - edit selected";

const HELP_EDITING_FIRST_LINE: &str =
    "<Enter> - accept, <Ctrl Enter> - add command, <↓↑> - navigate";

fn select_help_text(sequence: &State) -> &'static str {
    let is_running = sequence
        .execution
        .all_commands
        .iter()
        .any(|command| matches!(command.get_status(), CommandStatus::Running(_)));
    let all_pending = sequence
        .execution
        .all_commands
        .iter()
        .all(|command| matches!(command.get_status(), CommandStatus::Pending));
    let has_selection = sequence.selection.current_selected_command_index.is_some();
    let is_editing = sequence.editing.editing_input.is_some();

    if all_pending && !sequence.execution.is_running {
        if sequence.execution.all_commands.len() == 1 {
            HELP_ONE_PENDING_COMMAND
        } else if has_selection && !is_editing {
            HELP_ALL_COMMANDS_PENDING_WITH_SELECTION
        } else if is_editing {
            HELP_EDITING_FIRST_LINE
        } else {
            HELP_ALL_COMMANDS_PENDING
        }
    } else if is_running {
        if has_selection {
            HELP_RUNNING_WITH_SELECTION
        } else {
            HELP_RUNNING_NO_SELECTION
        }
    } else {
        if has_selection && !is_editing {
            HELP_STOPPED_WITH_SELECTION
        } else if is_editing {
            HELP_EDITING_FIRST_LINE
        } else {
            HELP_STOPPED_NO_SELECTION
        }
    }
}

fn style_help_text(text: &str) -> Text {
    Text::new(text)
        .color_substring(3, "<Ctrl c>")
        .color_substring(3, "<Ctrl w>")
        .color_substring(3, "<↓↑>")
        .color_substring(3, "<Esc>")
        .color_substring(3, "<Enter>")
        .color_substring(3, "<Ctrl Enter>")
        .color_substring(3, "<e>")
}

pub fn render_help_lines(
    sequence: &State,
    max_width: Option<usize>,
) -> (Text, usize, Option<(Text, usize)>) {
    let is_editing = sequence.editing.editing_input.is_some();

    if is_editing {
        let help_text = select_help_text(sequence);
        let (truncated_text, help_len) = if let Some(width) = max_width {
            truncate_help_line(help_text, width)
        } else {
            (help_text.to_string(), help_text.chars().count())
        };
        let first_line = (style_help_text(&truncated_text).unbold_all(), help_len);

        let editing_help_text = "Navigate with cd, chain with ||, &&, ; ";
        let editing_help = Text::new(editing_help_text)
            .color_substring(3, "cd")
            .color_substring(3, "||")
            .color_substring(3, "&&")
            .color_substring(3, ";")
            .unbold_all();
        let editing_len = editing_help_text.len();

        return (
            first_line.0,
            first_line.1,
            Some((editing_help, editing_len)),
        );
    }

    let help_text = select_help_text(sequence);

    if let Some(width) = max_width {
        if let Some((first_line, second_line)) = split_help_line(help_text, width) {
            let first_len = first_line.chars().count();
            let first_styled = style_help_text(&first_line).unbold_all();

            let second_line_width = second_line.chars().count();
            let (second_styled, second_len) = if second_line_width > width {
                let (truncated, len) = truncate_help_line(&second_line, width);
                (style_help_text(&truncated).unbold_all(), len)
            } else {
                (
                    style_help_text(&second_line).unbold_all(),
                    second_line_width,
                )
            };

            return (first_styled, first_len, Some((second_styled, second_len)));
        }

        let (truncated_text, help_len) = truncate_help_line(help_text, width);
        (
            style_help_text(&truncated_text).unbold_all(),
            help_len,
            None,
        )
    } else {
        let help_len = help_text.chars().count();
        (style_help_text(help_text).unbold_all(), help_len, None)
    }
}

fn split_help_line(help_text: &str, max_width: usize) -> Option<(String, String)> {
    let text_width = help_text.chars().count();

    if text_width <= max_width {
        return None;
    }

    let keybindings = parse_help_keybindings(help_text);

    if keybindings.is_empty() {
        return None;
    }

    for split_point in 1..keybindings.len() {
        let first_part: Vec<String> = keybindings
            .iter()
            .take(split_point)
            .map(|(key, _, action)| format!("{} - {}", key, action))
            .collect();
        let first_line = first_part.join(", ");

        let second_part: Vec<String> = keybindings
            .iter()
            .skip(split_point)
            .map(|(key, _, action)| format!("{} - {}", key, action))
            .collect();
        let second_line = second_part.join(", ");

        if first_line.chars().count() <= max_width && second_line.chars().count() <= max_width {
            return Some((first_line, second_line));
        }
    }

    None
}

fn folder_cell(cwd: &Option<PathBuf>) -> (Text, String) {
    let cwd_display = if let Some(cwd) = cwd {
        format_cwd(cwd)
    } else {
        "~".to_string()
    };
    let folder_display = format!("{} >", cwd_display);
    let text = Text::new(&folder_display).color_range(0, 0..cwd_display.len());
    (text, folder_display)
}

fn command_cell(text: &str) -> Text {
    let text = if text.is_empty() { " " } else { text };
    Text::new(text)
}

fn chain_cell(chain_type: &crate::state::ChainType) -> Text {
    let text = chain_type_to_str(chain_type);
    if !text.is_empty() {
        Text::new(text).color_range(1, 0..text.len())
    } else {
        Text::new(text)
    }
}

fn get_spinner_frame(frame: usize) -> &'static str {
    SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
}

pub fn format_status_text(status: &CommandStatus, spinner_frame: usize) -> String {
    match status {
        CommandStatus::Running(_) => {
            let spinner = get_spinner_frame(spinner_frame);
            format!("[RUNNING] {}", spinner)
        },
        CommandStatus::Interrupted(_) => "[INTERRUPTED]".to_string(),
        CommandStatus::Exited(Some(code), _) => format!("[EXIT CODE: {}]", code),
        CommandStatus::Exited(None, _) => "[EXITED]".to_string(),
        CommandStatus::Pending => " ".to_string(),
    }
}

fn apply_status_color(text: Text, status: &CommandStatus, status_str: &str) -> Text {
    match status {
        CommandStatus::Running(_) => text.color_range(3, 0..status_str.len()),
        CommandStatus::Interrupted(_) => text.error_color_range(0..status_str.len()),
        CommandStatus::Exited(Some(0), _) => {
            let number_start = 12;
            let number_end = status_str.len() - 1;
            text.success_color_range(number_start..number_end)
        },
        CommandStatus::Exited(Some(_), _) => {
            let number_start = 12;
            let number_end = status_str.len() - 1;
            text.error_color_range(number_start..number_end)
        },
        _ => text,
    }
}

fn status_cell(status: &CommandStatus, spinner_frame: usize) -> Text {
    let status_str = format_status_text(status, spinner_frame);
    let text = Text::new(&status_str);
    apply_status_color(text, status, &status_str)
}

fn apply_row_styles(cells: Vec<Text>, is_selected: bool) -> Vec<Text> {
    let mut styled_cells = cells;

    if is_selected {
        styled_cells = styled_cells.into_iter().map(|c| c.selected()).collect();
    }

    styled_cells
}

fn overflow_cell(indicator: &str, is_selected: bool) -> Text {
    let mut text = Text::new(indicator).dim_all();
    if is_selected {
        text = text.selected();
    }
    text
}

fn command_sequence_row(
    state: &State,
    cwd: &Option<PathBuf>,
    truncated_cmd_text: &str,
    chain_type: &crate::state::ChainType,
    status: &crate::state::CommandStatus,
    is_selected: bool,
    overflow_indicator: Option<String>,
) -> (Vec<Text>, usize) {
    let (folder_text, folder_display) = folder_cell(cwd);

    let mut cells = vec![
        folder_text,
        command_cell(truncated_cmd_text),
        chain_cell(chain_type),
        status_cell(status, state.layout.spinner_frame),
    ];

    cells = apply_row_styles(cells, is_selected);

    let truncated_cmd_text = if truncated_cmd_text.is_empty() {
        " "
    } else {
        truncated_cmd_text
    };
    let mut row_length = folder_display.chars().count()
        + truncated_cmd_text.chars().count()
        + chain_type_to_str(chain_type).chars().count()
        + format_status_text(status, state.layout.spinner_frame)
            .chars()
            .count()
        + 3;

    if let Some(indicator) = overflow_indicator {
        let indicator_len = indicator.chars().count();
        cells.push(overflow_cell(&indicator, is_selected));
        row_length += indicator_len + 1;
    }

    (cells, row_length)
}

pub fn build_table_header(has_overflow: bool) -> Table {
    let header_cols = if has_overflow {
        vec![" ", " ", " ", " ", " "]
    } else {
        vec![" ", " ", " ", " "]
    };
    Table::new().add_row(header_cols)
}

fn calculate_overflow_indicator(
    visible_idx: usize,
    visible_count: usize,
    hidden_above: usize,
    hidden_below: usize,
) -> Option<String> {
    if visible_idx == 0 && hidden_above > 0 {
        Some(format!("[+{}]", hidden_above))
    } else if visible_idx == visible_count.saturating_sub(1) && hidden_below > 0 {
        Some(format!("[+{}]", hidden_below))
    } else if hidden_above > 0 || hidden_below > 0 {
        Some(format!(" "))
    } else {
        None
    }
}

pub fn calculate_row_layout_info(
    index: usize,
    offset: usize,
    visible_count: usize,
    hidden_above: usize,
    hidden_below: usize,
    cols: usize,
    folder_width: usize,
    max_chain_width: usize,
    max_status_width: usize,
) -> Option<(Option<String>, usize)> {
    if index < offset || index >= offset + visible_count {
        return None;
    }

    let visible_idx = index.saturating_sub(offset);
    let overflow_indicator =
        calculate_overflow_indicator(visible_idx, visible_count, hidden_above, hidden_below);

    let available_cmd_width = calculate_available_cmd_width(
        cols,
        folder_width,
        overflow_indicator.as_ref(),
        max_chain_width,
        max_status_width,
    );

    Some((overflow_indicator, available_cmd_width))
}

pub fn add_command_row(
    table: Table,
    state: &State,
    index: usize,
    offset: usize,
    visible_count: usize,
    hidden_above: usize,
    hidden_below: usize,
) -> Table {
    let command = match state.execution.all_commands.get(index) {
        Some(cmd) => cmd,
        None => return table,
    };

    let cmd_text = &command.get_text();
    let chain_type = &command.get_chain_type();
    let status = &command.get_status();
    let command_cwd = command.get_cwd().or_else(|| state.cwd.clone());

    let cols = state.own_columns.unwrap_or(80);
    let longest_cwd_display = state.execution.longest_cwd_display(&state.cwd);
    let folder_display = format!("{} >", longest_cwd_display);
    let folder_width = folder_display.chars().count();

    let (max_chain_width, max_status_width) =
        calculate_max_widths(&state.execution.all_commands, state.layout.spinner_frame);

    let Some((overflow_indicator, available_cmd_width)) = calculate_row_layout_info(
        index,
        offset,
        visible_count,
        hidden_above,
        hidden_below,
        cols,
        folder_width,
        max_chain_width,
        max_status_width,
    ) else {
        return table;
    };

    if let Some(text_input) = &state.editing.editing_input {
        if state.selection.current_selected_command_index == Some(index) {
            let (truncated_editing_text, _) = truncate_middle(
                text_input.get_text(),
                available_cmd_width,
                Some(text_input.cursor_position()),
            );
            let (row, _) = command_sequence_row(
                state,
                &command_cwd,
                &truncated_editing_text,
                chain_type,
                &CommandStatus::Pending,
                false,
                overflow_indicator,
            );
            return table.add_styled_row(row);
        }
    }

    let truncated_cmd_text = truncate_middle(cmd_text, available_cmd_width, None).0;

    let (row, _) = command_sequence_row(
        state,
        &command_cwd,
        &truncated_cmd_text,
        chain_type,
        status,
        state.selection.current_selected_command_index == Some(index),
        overflow_indicator,
    );

    table.add_styled_row(row)
}

pub fn calculate_max_widths(
    commands: &[crate::state::CommandEntry],
    spinner_frame: usize,
) -> (usize, usize) {
    let max_chain_width = commands
        .iter()
        .map(|cmd| chain_type_to_str(&cmd.get_chain_type()).chars().count())
        .max()
        .unwrap_or(0);

    let max_status_width = commands
        .iter()
        .map(|cmd| {
            format_status_text(&cmd.get_status(), spinner_frame)
                .chars()
                .count()
        })
        .max()
        .unwrap_or(0);

    (max_chain_width, max_status_width)
}

pub fn calculate_longest_line(
    longest_cwd_display: &str,
    longest_command: usize,
    max_chain_width: usize,
    max_status_width: usize,
) -> usize {
    let folder_display = format!("{} >", longest_cwd_display);
    let cell_padding = 3;
    folder_display.chars().count()
        + longest_command
        + max_chain_width
        + max_status_width
        + cell_padding
}

pub fn chain_type_to_str(chain_type: &crate::state::ChainType) -> &'static str {
    match chain_type {
        crate::state::ChainType::And => "AND",
        crate::state::ChainType::Or => "OR",
        crate::state::ChainType::Then => "THEN",
        crate::state::ChainType::None => " ",
    }
}

pub fn truncate_help_line(help_text: &str, max_width: usize) -> (String, usize) {
    let text_width = help_text.chars().count();

    if text_width <= max_width {
        return (help_text.to_string(), text_width);
    }

    let keybindings = parse_help_keybindings(help_text);

    if let Some(result) = try_shortened_help(&keybindings, max_width) {
        return result;
    }

    if let Some(result) = try_keys_spaced(&keybindings, max_width) {
        return result;
    }

    if let Some(result) = try_keys_tight(&keybindings, max_width) {
        return result;
    }

    truncate_with_ellipsis(
        &keybindings
            .iter()
            .map(|(k, _, _)| *k)
            .collect::<Vec<_>>()
            .join("/"),
        max_width,
    )
}

fn parse_help_keybindings(help_text: &str) -> Vec<(&str, &str, &str)> {
    help_text
        .split(", ")
        .filter_map(|part| {
            part.find(" - ").map(|dash_pos| {
                let key = &part[..dash_pos];
                let action = &part[dash_pos + 3..];
                let first_word = action.split_whitespace().next().unwrap_or(action);
                (key, first_word, action)
            })
        })
        .collect()
}

fn try_shortened_help(
    keybindings: &[(&str, &str, &str)],
    max_width: usize,
) -> Option<(String, usize)> {
    let shortened: Vec<String> = keybindings
        .iter()
        .map(|(key, first_word, _)| format!("{} - {}", key, first_word))
        .collect();
    let shortened_text = shortened.join(", ");
    let shortened_width = shortened_text.chars().count();

    if shortened_width <= max_width {
        Some((shortened_text, shortened_width))
    } else {
        None
    }
}

fn try_keys_spaced(
    keybindings: &[(&str, &str, &str)],
    max_width: usize,
) -> Option<(String, usize)> {
    let keys_spaced_text = keybindings
        .iter()
        .map(|(key, _, _)| *key)
        .collect::<Vec<_>>()
        .join(" / ");
    let keys_spaced_width = keys_spaced_text.chars().count();

    if keys_spaced_width <= max_width {
        Some((keys_spaced_text, keys_spaced_width))
    } else {
        None
    }
}

fn try_keys_tight(keybindings: &[(&str, &str, &str)], max_width: usize) -> Option<(String, usize)> {
    let keys_tight_text = keybindings
        .iter()
        .map(|(key, _, _)| *key)
        .collect::<Vec<_>>()
        .join("/");
    let keys_tight_width = keys_tight_text.chars().count();

    if keys_tight_width <= max_width {
        Some((keys_tight_text, keys_tight_width))
    } else {
        None
    }
}

fn truncate_with_ellipsis(text: &str, max_width: usize) -> (String, usize) {
    let available = max_width.saturating_sub(3);
    let mut truncated = String::new();
    let mut current_width = 0;

    for ch in text.chars() {
        if current_width + 1 <= available {
            truncated.push(ch);
            current_width += 1;
        } else {
            break;
        }
    }

    truncated.push_str("...");
    let truncated_width = truncated.chars().count();
    (truncated, truncated_width)
}
