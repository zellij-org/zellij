use nix::pty::Winsize;
use serde::{Deserialize, Serialize};

use crate::input::mouse::Point;

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub cols: usize,
    // FIXME: Honestly, these shouldn't exist and rows / columns should be enums like:
    // Dimension::Flex(usize) / Dimension::Fixed(usize), but 400+ compiler errors is more than
    // I'm in the mood for right now...
    pub rows_fixed: bool,
    pub cols_fixed: bool,
}

impl From<Winsize> for PositionAndSize {
    fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            cols: winsize.ws_col as usize,
            rows: winsize.ws_row as usize,
            ..Default::default()
        }
    }
}

impl PositionAndSize {
    pub fn contains(&self, point: &Point) -> bool {
        let col = point.column.0 as usize;
        let row = point.line.0 as usize;
        self.x <= col && col <= self.x + self.columns && self.y <= row && row <= self.y + self.rows
    }
}
