use crate::terminal_pane::PositionAndSize;
use serde::{Deserialize, Serialize};

fn split_space_to_parts_vertically(
    space_to_split: &PositionAndSize,
    percentages: Vec<u8>,
) -> Vec<PositionAndSize> {
    let mut split_parts = vec![];
    let mut current_x_position = space_to_split.x;
    let width = space_to_split.columns - (percentages.len() - 1); // minus space for gaps
    for percentage in percentages.iter() {
        let columns = (width as f32 * (*percentage as f32 / 100.0)) as usize; // TODO: round properly
        split_parts.push(PositionAndSize {
            x: current_x_position,
            y: space_to_split.y,
            columns,
            rows: space_to_split.rows,
        });
        current_x_position += columns + 1; // 1 for gap
    }
    let total_width = split_parts
        .iter()
        .fold(0, |total_width, part| total_width + part.columns);
    if total_width < width {
        // we have some extra space left, let's add it to the last part
        let last_part_index = split_parts.len() - 1;
        let mut last_part = split_parts.get_mut(last_part_index).unwrap();
        last_part.columns += width - total_width;
    }
    split_parts
}

fn split_space_to_parts_horizontally(
    space_to_split: &PositionAndSize,
    percentages: Vec<u8>,
) -> Vec<PositionAndSize> {
    let mut split_parts = vec![];
    let mut current_y_position = space_to_split.y;
    let height = space_to_split.rows - (percentages.len() - 1); // minus space for gaps
    for percentage in percentages.iter() {
        let rows = (height as f32 * (*percentage as f32 / 100.0)) as usize; // TODO: round properly
        split_parts.push(PositionAndSize {
            x: space_to_split.x,
            y: current_y_position,
            columns: space_to_split.columns,
            rows,
        });
        current_y_position += rows + 1; // 1 for gap
    }
    let total_height = split_parts
        .iter()
        .fold(0, |total_height, part| total_height + part.rows);
    if total_height < height {
        // we have some extra space left, let's add it to the last part
        let last_part_index = split_parts.len() - 1;
        let mut last_part = split_parts.get_mut(last_part_index).unwrap();
        last_part.rows += height - total_height;
    }
    split_parts
}

fn split_space(space_to_split: &PositionAndSize, layout: &Layout) -> Vec<PositionAndSize> {
    let mut pane_positions: Vec<PositionAndSize> = vec![];
    let percentages: Vec<u8> = layout
        .parts
        .iter()
        .map(|part| {
            let split_size = part.split_size.as_ref().unwrap(); // TODO: if there is no split size, it should get the remaining "free space"
            match split_size {
                SplitSize::Percent(percent) => *percent,
            }
        })
        .collect();
    let split_parts = match layout.direction {
        Direction::Vertical => split_space_to_parts_vertically(space_to_split, percentages),
        Direction::Horizontal => split_space_to_parts_horizontally(space_to_split, percentages),
    };
    for (i, part) in layout.parts.iter().enumerate() {
        let part_position_and_size = split_parts.get(i).unwrap();
        if part.parts.len() > 0 {
            let mut part_positions = split_space(&part_position_and_size, part);
            pane_positions.append(&mut part_positions);
        } else {
            pane_positions.push(*part_position_and_size);
        }
    }
    pane_positions
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SplitSize {
    Percent(u8), // 1 to 100
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Layout {
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Layout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_size: Option<SplitSize>,
}

impl Layout {
    pub fn total_panes(&self) -> usize {
        let mut total_panes = 0;
        total_panes += self.parts.len();
        for part in self.parts.iter() {
            total_panes += part.total_panes();
        }
        total_panes
    }
    pub fn position_panes_in_space(&self, space: &PositionAndSize) -> Vec<PositionAndSize> {
        split_space(space, &self)
    }
}
