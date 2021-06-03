use nix::pty::Winsize;
use serde::{Deserialize, Serialize};

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: Dimension,
    pub cols: Dimension,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Dimension {
    /// Constrains the dimension to a fixed, integer number of rows / columns
    Fixed(usize),
    /// Constrains the dimension to a flexible percent size of the total screen
    Percent(u8),
}

impl From<Winsize> for PositionAndSize {
    fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            x: 0,
            y: 0,
            cols: Dimension::Fixed(winsize.ws_col as usize),
            rows: Dimension::Fixed(winsize.ws_row as usize),
        }
    }
}
