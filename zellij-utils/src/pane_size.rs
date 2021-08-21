use serde::{Deserialize, Serialize};

use crate::position::Position;

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct PaneGeom {
    pub x: usize,
    pub y: usize,
    pub rows: Dimension,
    pub cols: Dimension,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Viewport {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub cols: usize,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Offset {
    pub top: usize,
    pub bottom: usize,
    pub right: usize,
    pub left: usize,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Size {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Dimension {
    // FIXME: Think about if `pub` is the right choice here
    pub constraint: Constraint,
    inner: usize,
}

impl Default for Dimension {
    fn default() -> Self {
        Self {
            constraint: Constraint::Percent(100.0),
            inner: 0,
        }
    }
}

impl Dimension {
    pub fn fixed(size: usize) -> Dimension {
        Self {
            constraint: Constraint::Fixed(size),
            inner: 0,
        }
    }

    pub fn percent(percent: f64) -> Dimension {
        Self {
            constraint: Constraint::Percent(percent),
            inner: 0,
        }
    }

    pub fn as_usize(&self) -> usize {
        self.inner
    }

    pub fn as_percent(&self) -> Option<f64> {
        if let Constraint::Percent(p) = self.constraint {
            Some(p)
        } else {
            None
        }
    }

    // FIXME: Not sold on the existence of this yet, either...
    pub fn set_inner(&mut self, inner: usize) {
        self.inner = inner;
    }

    // FIXME: Is this really worth keeping around?
    pub fn is_fixed(&self) -> bool {
        matches!(self.constraint, Constraint::Fixed(_))
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Constraint {
    /// Constrains the dimension to a fixed, integer number of rows / columns
    Fixed(usize),
    /// Constrains the dimension to a flexible percent size of the total screen
    Percent(f64),
}

impl PaneGeom {
    pub fn contains(&self, point: &Position) -> bool {
        let col = point.column.0 as usize;
        let row = point.line.0 as usize;
        self.x <= col
            && col < self.x + self.cols.as_usize()
            && self.y <= row
            && row < self.y + self.rows.as_usize()
    }
}

impl Offset {
    pub fn frame(size: usize) -> Self {
        Self {
            top: size,
            bottom: size,
            right: size,
            left: size,
        }
    }

    pub fn shift(top: usize, left: usize) -> Self {
        Self {
            top,
            left,
            ..Default::default()
        }
    }
}

impl From<PaneGeom> for Viewport {
    fn from(pane: PaneGeom) -> Self {
        Self {
            x: pane.x,
            y: pane.y,
            rows: pane.rows.as_usize(),
            cols: pane.cols.as_usize(),
        }
    }
}

impl From<Viewport> for Size {
    fn from(viewport: Viewport) -> Self {
        Self {
            rows: viewport.rows,
            cols: viewport.cols,
        }
    }
}

impl From<Size> for Viewport {
    fn from(size: Size) -> Self {
        Self {
            rows: size.rows,
            cols: size.cols,
            ..Default::default()
        }
    }
}
