use crate::state::State;
use crate::ui::components;

const BASE_UI_HEIGHT: usize = 14;
const BASE_UI_WIDTH: usize = 20;

pub fn calculate_longest_help_row_length(max_width: Option<usize>) -> usize {
    components::ALL_HELP_TEXTS
        .iter()
        .map(|text| {
            if let Some(width) = max_width {
                components::truncate_help_line(text, width).1
            } else {
                text.chars().count()
            }
        })
        .max()
        .unwrap_or(0)
}

fn calculate_max_status_width() -> usize {
    "[EXIT CODE: 999]".chars().count()
}

pub fn calculate_ui_width(
    _sequence: &State,
    _cwd_display: &str,
    cols: usize,
    _rows: usize,
) -> usize {
    (cols * 30) / 100
}

pub fn calculate_ui_base_coords(
    sequence: &State,
    cwd_display: &str,
    cols: usize,
    rows: usize,
) -> (usize, usize, usize, usize) {
    let ui_width = cols.saturating_sub(2);
    let max_visible_rows = rows.saturating_sub(3);

    let (_, visible_count, _, _) = calculate_viewport(
        sequence.execution.all_commands.len(),
        max_visible_rows,
        sequence.selection.current_selected_command_index,
        sequence.execution.current_running_command_index,
    );

    let ui_height = visible_count + 5;
    let base_x = 1;
    let base_y = rows.saturating_sub(ui_height) / 2;

    (base_x, base_y, ui_width, ui_height)
}

pub fn calculate_viewport(
    total_commands: usize,
    max_visible: usize,
    selected_index: Option<usize>,
    running_index: usize,
) -> (usize, usize, usize, usize) {
    if total_commands <= max_visible {
        return (0, total_commands, 0, 0);
    }

    let focus_index = selected_index.unwrap_or(running_index);
    let half_visible = max_visible / 2;

    let offset = if focus_index < half_visible {
        0
    } else if focus_index >= total_commands.saturating_sub(half_visible) {
        total_commands.saturating_sub(max_visible)
    } else {
        focus_index.saturating_sub(half_visible)
    };

    let offset = offset.min(total_commands.saturating_sub(max_visible));
    let visible_count = max_visible.min(total_commands.saturating_sub(offset));
    let hidden_above = offset;
    let hidden_below = total_commands.saturating_sub(offset + visible_count);

    (offset, visible_count, hidden_above, hidden_below)
}
