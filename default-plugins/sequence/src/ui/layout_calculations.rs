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
