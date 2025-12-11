use zellij_tile::prelude::*;

pub fn reposition_plugin_for_sequence(
    plugin_id: u32,
    total_cols: usize,
    total_rows: usize,
    total_commands: usize,
) {
    // Width: 30% of viewport
    let width = std::cmp::max((total_cols * 30) / 100, 50);

    // Height: UI overhead + commands, capped at 25% of viewport
    let overhead_rows = 6;
    let height = (overhead_rows + total_commands).min((total_rows * 25) / 100);

    // Position: top-right with 1-space margins
    let x = total_cols.saturating_sub(width);
    let y = 1;

    change_floating_pane_coordinates_absolute(
        plugin_id,
        Some(x),
        Some(y),
        Some(width),
        Some(height),
        true,
    );
}

pub fn change_floating_pane_coordinates_absolute(
    own_plugin_id: u32,
    x: Option<usize>,
    y: Option<usize>,
    width: Option<usize>,
    height: Option<usize>,
    should_be_pinned: bool,
) {
    let coordinates = FloatingPaneCoordinates::new(
        x.map(|x| x.to_string()),
        y.map(|y| y.to_string()),
        width.map(|width| width.to_string()),
        height.map(|height| height.to_string()),
        Some(should_be_pinned),
    );
    if let Some(coordinates) = coordinates {
        change_floating_panes_coordinates(vec![(PaneId::Plugin(own_plugin_id), coordinates)]);
    }
}
