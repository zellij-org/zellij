use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

use crate::position::Position;

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize, Eq, Hash)]
pub struct PaneGeom {
    pub x: usize,
    pub y: usize,
    pub rows: Dimension,
    pub cols: Dimension,
    pub is_stacked: bool,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Viewport {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub cols: usize,
}

impl Viewport {
    pub fn has_positive_size(&self) -> bool {
        self.rows > 0 && self.cols > 0
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Offset {
    pub top: usize,
    pub bottom: usize,
    pub right: usize,
    pub left: usize,
}

#[derive(Clone, Copy, Default, PartialEq, Debug, Serialize, Deserialize)]
pub struct Size {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SizeInPixels {
    pub height: usize,
    pub width: usize,
}

#[derive(Eq, Clone, Copy, PartialEq, Debug, Serialize, Deserialize, Hash)]
pub struct Dimension {
    pub constraint: Constraint,
    inner: usize,
}

impl Default for Dimension {
    fn default() -> Self {
        Self::percent(100.0)
    }
}

impl Dimension {
    pub fn fixed(size: usize) -> Dimension {
        Self {
            constraint: Constraint::Fixed(size),
            inner: size,
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

    pub fn as_percent(&self) -> Option<f64> {
        if let Constraint::Percent(p) = self.constraint {
            Some(p)
        } else {
            None
        }
    }

    pub fn set_percent(&mut self, percent: f64) {
        self.constraint = Constraint::Percent(percent);
    }

    pub fn set_inner(&mut self, inner: usize) {
        self.inner = inner;
    }

    pub fn adjust_inner(&mut self, full_size: usize) -> f64 {
        // returns the leftover from
        // rounding if any
        // TODO: elsewhere?
        match self.constraint {
            Constraint::Percent(percent) => {
                let new_inner = (percent / 100.0) * full_size as f64;
                let rounded = new_inner.floor();
                let leftover = rounded - new_inner;
                self.set_inner(rounded as usize);
                leftover
                // self.set_inner(((percent / 100.0) * full_size as f64).round() as usize);
            },
            Constraint::Fixed(fixed_size) => {
                self.set_inner(fixed_size);
                0.0
            },
        }
    }
    pub fn increase_inner(&mut self, by: usize) {
        self.inner += by;
    }
    pub fn decrease_inner(&mut self, by: usize) {
        self.inner -= by;
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self.constraint, Constraint::Fixed(_))
    }
    pub fn is_percent(&self) -> bool {
        matches!(self.constraint, Constraint::Percent(_))
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Constraint {
    /// Constrains the dimension to a fixed, integer number of rows / columns
    Fixed(usize),
    /// Constrains the dimension to a flexible percent size of the total screen
    Percent(f64),
}

#[allow(clippy::derive_hash_xor_eq)]
impl Hash for Constraint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Constraint::Fixed(size) => size.hash(state),
            Constraint::Percent(size) => (*size as usize).hash(state),
        }
    }
}

impl Eq for Constraint {}

impl PaneGeom {
    pub fn contains(&self, point: &Position) -> bool {
        let col = point.column.0 as usize;
        let row = point.line.0 as usize;
        self.x <= col
            && col < self.x + self.cols.as_usize()
            && self.y <= row
            && row < self.y + self.rows.as_usize()
    }
    pub fn is_at_least_minimum_size(&self) -> bool {
        self.rows.as_usize() > 0 && self.cols.as_usize() > 0
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

    pub fn shift_right_and_top(right: usize, top: usize) -> Self {
        Self {
            right,
            top,
            ..Default::default()
        }
    }

    // FIXME: This should be top and left, not bottom and right, but `boundaries.rs` would need
    // some changing
    pub fn shift(bottom: usize, right: usize) -> Self {
        Self {
            bottom,
            right,
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

impl From<Size> for Viewport {
    fn from(size: Size) -> Self {
        Self {
            rows: size.rows,
            cols: size.cols,
            ..Default::default()
        }
    }
}

impl From<&PaneGeom> for Size {
    fn from(pane_geom: &PaneGeom) -> Self {
        Self {
            rows: pane_geom.rows.as_usize(),
            cols: pane_geom.cols.as_usize(),
        }
    }
}

impl From<&Size> for PaneGeom {
    fn from(size: &Size) -> Self {
        let mut rows = Dimension::percent(100.0);
        let mut cols = Dimension::percent(100.0);
        rows.set_inner(size.rows);
        cols.set_inner(size.cols);
        Self {
            rows,
            cols,
            ..Default::default()
        }
    }
}
