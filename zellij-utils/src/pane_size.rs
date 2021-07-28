use nix::pty::Winsize;
use serde::{Deserialize, Serialize};

use crate::position::Position;

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: Dimension,
    pub cols: Dimension,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Dimension {
    // FIXME: Think about if `pub` is the right choice here
    pub constraint: Constraint,
    inner: usize,
}

impl Dimension {
    pub fn fixed(inner: usize) -> Dimension {
        Self {
            constraint: Constraint::Fixed,
            inner,
        }
    }

    pub fn percent(percent: f64) -> Dimension {
        Self {
            constraint: Constraint::Percent(percent),
            inner: 1,
        }
    }

    pub fn as_usize(&self) -> usize {
        self.inner
    }

    // FIXME: Not sold on the existence of this yet, either...
    pub fn set_inner(&mut self, inner: usize) {
        match self.constraint {
            Constraint::Percent(_) => self.inner = inner,
            _ => (),
        };
    }

    // FIXME: Is this really worth keeping around?
    pub fn is_fixed(&self) -> bool {
        self.constraint == Constraint::Fixed
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Constraint {
    /// Constrains the dimension to a fixed, integer number of rows / columns
    Fixed,
    /// Constrains the dimension to a flexible percent size of the total screen
    Percent(f64),
}

impl From<Winsize> for PositionAndSize {
    fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            x: 0,
            y: 0,
            cols: Dimension::fixed(winsize.ws_col as usize),
            rows: Dimension::fixed(winsize.ws_row as usize),
        }
    }
}

impl PositionAndSize {
    pub fn contains(&self, point: &Position) -> bool {
        let col = point.column.0 as usize;
        let row = point.line.0 as usize;
        self.x <= col
            && col < self.x + self.cols.as_usize()
            && self.y <= row
            && row < self.y + self.rows.as_usize()
    }
}
