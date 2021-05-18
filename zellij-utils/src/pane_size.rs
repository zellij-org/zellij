use nix::pty::Winsize;
use serde::{Deserialize, Serialize};

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub columns: usize,
    pub max_rows: Option<usize>,
    pub max_columns: Option<usize>,
}

impl From<Winsize> for PositionAndSize {
    fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            columns: winsize.ws_col as usize,
            rows: winsize.ws_row as usize,
            ..Default::default()
        }
    }
}
