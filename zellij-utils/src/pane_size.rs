use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    hash::{Hash, Hasher},
};

use crate::data::FloatingPaneCoordinates;
use crate::input::layout::{SplitDirection, SplitSize};
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
    pub(crate) inner: usize,
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
        self.inner = self.inner.saturating_sub(by);
    }

    pub fn is_fixed(&self) -> bool {
        matches!(self.constraint, Constraint::Fixed(_))
    }
    pub fn is_percent(&self) -> bool {
        matches!(self.constraint, Constraint::Percent(_))
    }
    pub fn from_split_size(split_size: SplitSize, full_size: usize) -> Self {
        match split_size {
            SplitSize::Fixed(fixed) => Dimension {
                constraint: Constraint::Fixed(fixed),
                inner: fixed,
            },
            SplitSize::Percent(percent) => Dimension {
                constraint: Constraint::Percent(percent as f64),
                inner: ((percent as f64 / 100.0) * full_size as f64).floor() as usize,
            },
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum Constraint {
    /// Constrains the dimension to a fixed, integer number of rows / columns
    Fixed(usize),
    /// Constrains the dimension to a flexible percent size of the total screen
    Percent(f64),
}

impl Display for Constraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let actual = match self {
            Constraint::Fixed(v) => *v as f64,
            Constraint::Percent(v) => *v,
        };
        write!(f, "{}", actual)?;
        Ok(())
    }
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
    pub fn is_flexible_in_direction(&self, split_direction: SplitDirection) -> bool {
        match split_direction {
            SplitDirection::Vertical => self.cols.is_percent(),
            SplitDirection::Horizontal => self.rows.is_percent(),
        }
    }
    pub fn adjust_coordinates(
        &mut self,
        floating_pane_coordinates: FloatingPaneCoordinates,
        viewport: Viewport,
    ) {
        if let Some(x) = floating_pane_coordinates.x {
            self.x = x.to_fixed(viewport.cols);
        }
        if let Some(y) = floating_pane_coordinates.y {
            self.y = y.to_fixed(viewport.rows);
        }
        if let Some(height) = floating_pane_coordinates.height {
            self.rows = Dimension::from_split_size(height, viewport.rows);
        }
        if let Some(width) = floating_pane_coordinates.width {
            self.cols = Dimension::from_split_size(width, viewport.cols);
        }
        if self.x < viewport.x {
            self.x = viewport.x;
        } else if self.x > viewport.x + viewport.cols {
            self.x = (viewport.x + viewport.cols).saturating_sub(self.cols.as_usize());
        }
        if self.y < viewport.y {
            self.y = viewport.y;
        } else if self.y > viewport.y + viewport.rows {
            self.y = (viewport.y + viewport.rows).saturating_sub(self.rows.as_usize());
        }
        if self.x + self.cols.as_usize() > viewport.x + viewport.cols {
            let new_cols = (viewport.x + viewport.cols).saturating_sub(self.x);
            self.cols.set_inner(new_cols);
        }
        if self.y + self.rows.as_usize() > viewport.y + viewport.rows {
            let new_rows = (viewport.y + viewport.rows).saturating_sub(self.y);
            self.rows.set_inner(new_rows);
        }
    }
}

impl Display for PaneGeom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ ")?;
        write!(f, r#""x": {},"#, self.x)?;
        write!(f, r#""y": {},"#, self.y)?;
        write!(f, r#""cols": {},"#, self.cols.constraint)?;
        write!(f, r#""rows": {},"#, self.rows.constraint)?;
        write!(f, r#""stacked": {}"#, self.is_stacked)?;
        write!(f, " }}")?;

        Ok(())
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
