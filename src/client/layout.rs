use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::{fs::File, io::prelude::*};

use crate::panes::PositionAndSize;

fn split_space_to_parts_vertically(
    space_to_split: &PositionAndSize,
    sizes: Vec<Option<SplitSize>>,
) -> Vec<PositionAndSize> {
    let mut split_parts = Vec::new();
    let mut current_x_position = space_to_split.x;
    let mut current_width = 0;
    let max_width = space_to_split.columns - (sizes.len() - 1); // minus space for gaps

    let mut parts_to_grow = Vec::new();

    // First fit in the parameterized sizes
    for size in sizes {
        let (columns, max_columns) = match size {
            Some(SplitSize::Percent(percent)) => {
                ((max_width as f32 * (percent as f32 / 100.0)) as usize, None)
            } // TODO: round properly
            Some(SplitSize::Fixed(size)) => (size as usize, Some(size as usize)),
            None => {
                parts_to_grow.push(current_x_position);
                (
                    1, // This is grown later on
                    None,
                )
            }
        };
        split_parts.push(PositionAndSize {
            x: current_x_position,
            y: space_to_split.y,
            columns,
            rows: space_to_split.rows,
            max_columns,
            ..Default::default()
        });
        current_width += columns;
        current_x_position += columns + 1; // 1 for gap
    }

    if current_width > max_width {
        panic!("Layout contained too many columns to fit onto the screen!");
    }

    let mut last_flexible_index = split_parts.len() - 1;
    if let Some(new_columns) = (max_width - current_width).checked_div(parts_to_grow.len()) {
        current_width = 0;
        current_x_position = 0;
        for (idx, part) in split_parts.iter_mut().enumerate() {
            part.x = current_x_position;
            if parts_to_grow.contains(&part.x) {
                part.columns = new_columns;
                last_flexible_index = idx;
            }
            current_width += part.columns;
            current_x_position += part.columns + 1; // 1 for gap
        }
    }

    if current_width < max_width {
        // we have some extra space left, let's add it to the last flexible part
        let extra = max_width - current_width;
        let mut last_part = split_parts.get_mut(last_flexible_index).unwrap();
        last_part.columns += extra;
        for part in (&mut split_parts[last_flexible_index + 1..]).iter_mut() {
            part.x += extra;
        }
    }
    split_parts
}

fn split_space_to_parts_horizontally(
    space_to_split: &PositionAndSize,
    sizes: Vec<Option<SplitSize>>,
) -> Vec<PositionAndSize> {
    let mut split_parts = Vec::new();
    let mut current_y_position = space_to_split.y;
    let mut current_height = 0;
    let max_height = space_to_split.rows - (sizes.len() - 1); // minus space for gaps

    let mut parts_to_grow = Vec::new();

    for size in sizes {
        let (rows, max_rows) = match size {
            Some(SplitSize::Percent(percent)) => (
                (max_height as f32 * (percent as f32 / 100.0)) as usize,
                None,
            ), // TODO: round properly
            Some(SplitSize::Fixed(size)) => (size as usize, Some(size as usize)),
            None => {
                parts_to_grow.push(current_y_position);
                (
                    1, // This is grown later on
                    None,
                )
            }
        };
        split_parts.push(PositionAndSize {
            x: space_to_split.x,
            y: current_y_position,
            columns: space_to_split.columns,
            rows,
            max_rows,
            ..Default::default()
        });
        current_height += rows;
        current_y_position += rows + 1; // 1 for gap
    }

    if current_height > max_height {
        panic!("Layout contained too many rows to fit onto the screen!");
    }

    let mut last_flexible_index = split_parts.len() - 1;
    if let Some(new_rows) = (max_height - current_height).checked_div(parts_to_grow.len()) {
        current_height = 0;
        current_y_position = 0;

        for (idx, part) in split_parts.iter_mut().enumerate() {
            part.y = current_y_position;
            if parts_to_grow.contains(&part.y) {
                part.rows = new_rows;
                last_flexible_index = idx;
            }
            current_height += part.rows;
            current_y_position += part.rows + 1; // 1 for gap
        }
    }

    if current_height < max_height {
        // we have some extra space left, let's add it to the last flexible part
        let extra = max_height - current_height;
        let mut last_part = split_parts.get_mut(last_flexible_index).unwrap();
        last_part.rows += extra;
        for part in (&mut split_parts[last_flexible_index + 1..]).iter_mut() {
            part.y += extra;
        }
    }
    split_parts
}

fn split_space(
    space_to_split: &PositionAndSize,
    layout: &Layout,
) -> Vec<(Layout, PositionAndSize)> {
    let mut pane_positions = Vec::new();
    let sizes: Vec<Option<SplitSize>> = layout.parts.iter().map(|part| part.split_size).collect();

    let split_parts = match layout.direction {
        Direction::Vertical => split_space_to_parts_vertically(space_to_split, sizes),
        Direction::Horizontal => split_space_to_parts_horizontally(space_to_split, sizes),
    };
    for (i, part) in layout.parts.iter().enumerate() {
        let part_position_and_size = split_parts.get(i).unwrap();
        if !part.parts.is_empty() {
            let mut part_positions = split_space(&part_position_and_size, part);
            pane_positions.append(&mut part_positions);
        } else {
            pane_positions.push((part.clone(), *part_position_and_size));
        }
    }
    pane_positions
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Direction {
    Horizontal,
    Vertical,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum SplitSize {
    Percent(u8), // 1 to 100
    Fixed(u16),  // An absolute number of columns or rows
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Layout {
    pub direction: Direction,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<Layout>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split_size: Option<SplitSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin: Option<PathBuf>,
}

impl Layout {
    pub fn new(layout_path: &Path, data_dir: &Path) -> Self {
        let layout_dir = data_dir.join("layouts/");
        let mut layout_file = File::open(&layout_path)
            .or_else(|_| File::open(&layout_dir.join(&layout_path).with_extension("yaml")))
            .unwrap_or_else(|_| panic!("cannot find layout {}", &layout_path.display()));

        let mut layout = String::new();
        layout_file
            .read_to_string(&mut layout)
            .unwrap_or_else(|_| panic!("could not read layout {}", &layout_path.display()));
        let layout: Layout = serde_yaml::from_str(&layout)
            .unwrap_or_else(|_| panic!("could not parse layout {}", &layout_path.display()));
        layout
    }

    // It wants to use Path here, but that doesn't compile.
    #[warn(clippy::ptr_arg)]
    pub fn from_defaults(layout_path: &PathBuf, data_dir: &Path) -> Self {
        Self::new(
            &data_dir
                .join("layouts/")
                .join(layout_path)
                .with_extension("yaml"),
            &data_dir,
        )
    }

    pub fn total_terminal_panes(&self) -> usize {
        let mut total_panes = 0;
        total_panes += self.parts.len();
        for part in self.parts.iter() {
            if part.plugin.is_none() {
                total_panes += part.total_terminal_panes();
            }
        }
        total_panes
    }

    pub fn position_panes_in_space(
        &self,
        space: &PositionAndSize,
    ) -> Vec<(Layout, PositionAndSize)> {
        split_space(space, &self)
    }
}
