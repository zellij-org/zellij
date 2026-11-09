use serde::{Deserialize, Serialize};
use std::{
    fmt::Display,
    hash::{Hash, Hasher},
};
use thiserror::Error;

use crate::data::FloatingPaneCoordinates;
use crate::input::layout::{PercentOrFixed, SplitDirection, SplitSize};
use crate::position::Position;

/// Why a pane could not be fitted into a viewport without inventing invalid geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FitViewportError {
    #[error("viewport has no positive size")]
    EmptyViewport,
    #[error("pane geometry collapsed to an empty size")]
    ZeroSizedPane,
}

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Default, Debug, Serialize, Deserialize)]
pub struct PaneGeom {
    pub x: usize,
    pub y: usize,
    pub rows: Dimension,
    pub cols: Dimension,
    pub stacked: Option<usize>,          // usize - stack id
    pub is_pinned: bool,                 // only relevant to floating panes
    pub logical_position: Option<usize>, // relevant when placing this pane in a layout
}

impl PartialEq for PaneGeom {
    fn eq(&self, other: &Self) -> bool {
        // compare all except is_pinned
        // NOTE: Keep this in sync with what the `Hash` trait impl does.
        self.x == other.x
            && self.y == other.y
            && self.rows == other.rows
            && self.cols == other.cols
            && self.stacked == other.stacked
    }
}

impl std::hash::Hash for PaneGeom {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // NOTE: Keep this in sync with what the `PartiqlEq` trait impl does.
        self.x.hash(state);
        self.y.hash(state);
        self.rows.hash(state);
        self.cols.hash(state);
        self.stacked.hash(state);
    }
}

impl Eq for PaneGeom {}

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

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Size {
    pub rows: usize,
    pub cols: usize,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct SizeInPixels {
    pub height: usize,
    pub width: usize,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct Insets {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
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
    pub fn from_percent_or_fixed(size: PercentOrFixed, full_size: usize) -> Self {
        match size {
            PercentOrFixed::Fixed(fixed) => Dimension {
                constraint: Constraint::Fixed(fixed),
                inner: fixed,
            },
            PercentOrFixed::Percent(percent) => Dimension {
                constraint: Constraint::Percent(percent as f64),
                inner: ((percent as f64 / 100.0) * full_size as f64).floor() as usize,
            },
        }
    }
    pub fn split_out(&mut self, by: f64) -> Self {
        match self.constraint {
            Constraint::Percent(percent) => {
                let split_out_value = percent / by;
                let split_out_inner_value = self.inner / by as usize;
                self.constraint = Constraint::Percent(percent - split_out_value);
                self.inner = self.inner.saturating_sub(split_out_inner_value);
                let mut split_out_dimension = Self::percent(split_out_value);
                split_out_dimension.inner = split_out_inner_value;
                split_out_dimension
            },
            Constraint::Fixed(fixed) => {
                let split_out_value = fixed / by as usize;
                self.constraint = Constraint::Fixed(fixed - split_out_value);
                Self::fixed(split_out_value)
            },
        }
    }
    pub fn reduce_by(&mut self, by: f64, by_inner: usize) {
        match self.constraint {
            Constraint::Percent(percent) => {
                self.constraint = Constraint::Percent(percent - by);
                self.inner = self.inner.saturating_sub(by_inner);
            },
            Constraint::Fixed(_fixed) => {
                log::error!("Cannot reduce_by fixed dimensions");
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
    pub fn apply_floating_pane_position(
        &mut self,
        x: Option<PercentOrFixed>,
        y: Option<PercentOrFixed>,
        width: Option<PercentOrFixed>,
        height: Option<PercentOrFixed>,
        viewport: Viewport,
    ) {
        if let Some(width) = &width {
            self.cols = Dimension::fixed(width.to_fixed(viewport.cols));
        }
        if let Some(height) = &height {
            self.rows = Dimension::fixed(height.to_fixed(viewport.rows));
        }
        self.x = match &x {
            Some(x) => viewport.x + x.to_fixed(viewport.cols),
            None if width.is_some() => {
                viewport.x + viewport.cols.saturating_sub(self.cols.as_usize()) / 2
            },
            None => self.x,
        };
        self.y = match &y {
            Some(y) => viewport.y + y.to_fixed(viewport.rows),
            None if height.is_some() => {
                viewport.y + viewport.rows.saturating_sub(self.rows.as_usize()) / 2
            },
            None => self.y,
        };
    }
    /// Fit this rectangle into `viewport` under one shared contract:
    /// keep a positive size (at least `min_cols`×`min_rows`, capped by the
    /// viewport) and shift the origin so the whole pane stays inside.
    /// Exact edge and beyond-edge origins are treated the same; an empty
    /// viewport is an explicit error instead of manufacturing 0×0 geometry.
    pub fn fit_into_viewport_with_minimum(
        &mut self,
        viewport: Viewport,
        min_cols: usize,
        min_rows: usize,
    ) -> Result<(), FitViewportError> {
        if !viewport.has_positive_size() {
            return Err(FitViewportError::EmptyViewport);
        }

        let min_cols = min_cols.max(1).min(viewport.cols);
        let min_rows = min_rows.max(1).min(viewport.rows);
        let cols = self.cols.as_usize().max(min_cols).min(viewport.cols);
        let rows = self.rows.as_usize().max(min_rows).min(viewport.rows);
        self.cols.set_inner(cols);
        self.rows.set_inner(rows);

        let max_x = viewport.x + viewport.cols.saturating_sub(cols);
        let max_y = viewport.y + viewport.rows.saturating_sub(rows);
        if self.x < viewport.x {
            self.x = viewport.x;
        } else if self.x > max_x {
            self.x = max_x;
        }
        if self.y < viewport.y {
            self.y = viewport.y;
        } else if self.y > max_y {
            self.y = max_y;
        }

        if !self.is_at_least_minimum_size() {
            return Err(FitViewportError::ZeroSizedPane);
        }
        Ok(())
    }

    /// Fit with a content minimum of 1×1 cell.
    pub fn fit_into_viewport(&mut self, viewport: Viewport) -> Result<(), FitViewportError> {
        self.fit_into_viewport_with_minimum(viewport, 1, 1)
    }

    pub fn adjust_coordinates(
        &mut self,
        floating_pane_coordinates: FloatingPaneCoordinates,
        viewport: Viewport,
    ) -> Result<(), FitViewportError> {
        self.apply_floating_pane_position(
            floating_pane_coordinates.x,
            floating_pane_coordinates.y,
            floating_pane_coordinates.width,
            floating_pane_coordinates.height,
            viewport,
        );
        self.fit_into_viewport(viewport)
    }
    pub fn combine_vertically_with(&self, geom_below: &PaneGeom) -> Option<Self> {
        match (self.rows.constraint, geom_below.rows.constraint) {
            (Constraint::Percent(self_percent), Constraint::Percent(geom_below_percent)) => {
                let mut combined = self.clone();
                combined.rows = Dimension::percent(self_percent + geom_below_percent);
                combined.rows.inner = self.rows.inner + geom_below.rows.inner;
                Some(combined)
            },
            _ => {
                log::error!("Can't combine fixed panes");
                None
            },
        }
    }
    pub fn combine_horizontally_with(&self, geom_to_the_right: &PaneGeom) -> Option<Self> {
        match (self.cols.constraint, geom_to_the_right.cols.constraint) {
            (Constraint::Percent(self_percent), Constraint::Percent(geom_to_the_right_percent)) => {
                let mut combined = self.clone();
                combined.cols = Dimension::percent(self_percent + geom_to_the_right_percent);
                combined.cols.inner = self.cols.inner + geom_to_the_right.cols.inner;
                Some(combined)
            },
            _ => {
                log::error!("Can't combine fixed panes");
                None
            },
        }
    }
    pub fn combine_vertically_with_many(&self, geoms_below: &Vec<PaneGeom>) -> Option<Self> {
        // here we expect the geoms to be sorted by their y and be contiguous (i.e. same x and
        // width, no overlaps) and be below self
        let mut combined = self.clone();
        for geom_below in geoms_below {
            match (combined.rows.constraint, geom_below.rows.constraint) {
                (
                    Constraint::Percent(combined_percent),
                    Constraint::Percent(geom_below_percent),
                ) => {
                    let new_rows_inner = combined.rows.inner + geom_below.rows.inner;
                    combined.rows = Dimension::percent(combined_percent + geom_below_percent);
                    combined.rows.inner = new_rows_inner;
                },
                _ => {
                    log::error!("Can't combine fixed panes");
                    return None;
                },
            }
        }
        Some(combined)
    }
    pub fn combine_horizontally_with_many(
        &self,
        geoms_to_the_right: &Vec<PaneGeom>,
    ) -> Option<Self> {
        // here we expect the geoms to be sorted by their x and be contiguous (i.e. same x and
        // width, no overlaps) and be right of self
        let mut combined = self.clone();
        for geom_to_the_right in geoms_to_the_right {
            match (combined.cols.constraint, geom_to_the_right.cols.constraint) {
                (
                    Constraint::Percent(combined_percent),
                    Constraint::Percent(geom_to_the_right_percent),
                ) => {
                    let new_cols = combined.cols.inner + geom_to_the_right.cols.inner;
                    combined.cols =
                        Dimension::percent(combined_percent + geom_to_the_right_percent);
                    combined.cols.inner = new_cols;
                },
                _ => {
                    log::error!("Can't combine fixed panes");
                    return None;
                },
            }
        }
        Some(combined)
    }
    pub fn is_stacked(&self) -> bool {
        self.stacked.is_some()
    }
}

impl Display for PaneGeom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ ")?;
        write!(f, r#""x": {},"#, self.x)?;
        write!(f, r#""y": {},"#, self.y)?;
        write!(f, r#""cols": {},"#, self.cols.constraint)?;
        write!(f, r#""rows": {},"#, self.rows.constraint)?;
        write!(f, r#""stacked": {:?}"#, self.stacked)?;
        write!(f, r#""logical_position": {:?}"#, self.logical_position)?;
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

    pub fn shift_right(right: usize) -> Self {
        Self {
            right,
            ..Default::default()
        }
    }

    pub fn shift_right_top_and_bottom(right: usize, top: usize, bottom: usize) -> Self {
        Self {
            right,
            top,
            bottom,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Viewport {
        Viewport {
            x: 20,
            y: 5,
            cols: 100,
            rows: 40,
        }
    }

    fn floating_at(x: usize, y: usize, width: usize, height: usize) -> FloatingPaneCoordinates {
        FloatingPaneCoordinates {
            x: Some(PercentOrFixed::Fixed(x)),
            y: Some(PercentOrFixed::Fixed(y)),
            width: Some(PercentOrFixed::Fixed(width)),
            height: Some(PercentOrFixed::Fixed(height)),
            pinned: None,
            borderless: None,
            border_style: None,
        }
    }

    #[test]
    fn floating_positions_are_relative_to_a_nonzero_viewport_origin() {
        let viewport = viewport();
        let mut geom = PaneGeom::default();
        geom.apply_floating_pane_position(
            Some(PercentOrFixed::Percent(50)),
            Some(PercentOrFixed::Percent(50)),
            Some(PercentOrFixed::Fixed(10)),
            Some(PercentOrFixed::Fixed(4)),
            viewport,
        );
        assert_eq!((geom.x, geom.y), (70, 25));

        let mut centered = PaneGeom::default();
        centered.apply_floating_pane_position(
            None,
            None,
            Some(PercentOrFixed::Fixed(10)),
            Some(PercentOrFixed::Fixed(4)),
            viewport,
        );
        assert_eq!((centered.x, centered.y), (65, 23));
    }

    #[test]
    fn exact_right_edge_keeps_width_like_beyond_edge() {
        let viewport = viewport();
        let mut on_edge = PaneGeom::default();
        on_edge
            .adjust_coordinates(floating_at(100, 0, 30, 4), viewport)
            .expect("exact edge must fit");
        let mut beyond = PaneGeom::default();
        beyond
            .adjust_coordinates(floating_at(101, 0, 30, 4), viewport)
            .expect("beyond edge must fit");

        assert_eq!((on_edge.x, on_edge.cols.as_usize()), (90, 30));
        assert_eq!((beyond.x, beyond.cols.as_usize()), (90, 30));
        assert_eq!(on_edge.x + on_edge.cols.as_usize(), viewport.x + viewport.cols);
    }

    #[test]
    fn exact_bottom_edge_keeps_height_like_beyond_edge() {
        let viewport = viewport();
        let mut on_edge = PaneGeom::default();
        on_edge
            .adjust_coordinates(floating_at(0, 40, 10, 8), viewport)
            .expect("exact edge must fit");
        let mut beyond = PaneGeom::default();
        beyond
            .adjust_coordinates(floating_at(0, 41, 10, 8), viewport)
            .expect("beyond edge must fit");

        assert_eq!((on_edge.y, on_edge.rows.as_usize()), (37, 8));
        assert_eq!((beyond.y, beyond.rows.as_usize()), (37, 8));
    }

    #[test]
    fn out_of_range_origin_is_shifted_back_into_the_viewport() {
        let viewport = viewport();
        let mut geom = PaneGeom::default();
        geom.adjust_coordinates(floating_at(500, 500, 12, 6), viewport)
            .expect("out of range must fit");
        assert_eq!(geom.x, viewport.x + viewport.cols - 12);
        assert_eq!(geom.y, viewport.y + viewport.rows - 6);
        assert!(geom.is_at_least_minimum_size());
    }

    #[test]
    fn zero_viewport_is_an_explicit_error() {
        let mut geom = PaneGeom {
            cols: Dimension::fixed(10),
            rows: Dimension::fixed(4),
            ..Default::default()
        };
        assert_eq!(
            geom.fit_into_viewport(Viewport::default()),
            Err(FitViewportError::EmptyViewport)
        );
        assert_eq!(
            geom.adjust_coordinates(floating_at(0, 0, 10, 4), Viewport::default()),
            Err(FitViewportError::EmptyViewport)
        );
    }

    #[test]
    fn minimum_frame_and_content_sizes_are_preserved_when_the_viewport_allows() {
        let viewport = viewport();
        let mut content = PaneGeom {
            cols: Dimension::fixed(0),
            rows: Dimension::fixed(0),
            ..Default::default()
        };
        content
            .fit_into_viewport(viewport)
            .expect("content minimum");
        assert_eq!((content.cols.as_usize(), content.rows.as_usize()), (1, 1));

        let mut framed = PaneGeom {
            cols: Dimension::fixed(2),
            rows: Dimension::fixed(2),
            ..Default::default()
        };
        framed
            .fit_into_viewport_with_minimum(viewport, 5, 5)
            .expect("frame minimum");
        assert_eq!((framed.cols.as_usize(), framed.rows.as_usize()), (5, 5));
        assert_eq!((framed.x, framed.y), (viewport.x, viewport.y));
    }

    #[test]
    fn large_width_is_capped_to_the_viewport() {
        let viewport = viewport();
        let mut geom = PaneGeom::default();
        geom.adjust_coordinates(floating_at(10, 2, 1000, 4), viewport)
            .expect("oversized width must fit");
        assert_eq!(geom.cols.as_usize(), viewport.cols);
        assert_eq!(geom.x, viewport.x);
        assert!(geom.x + geom.cols.as_usize() <= viewport.x + viewport.cols);
    }
}
