use super::is_inside_viewport;
use super::pane_resizer::PaneResizer;
use crate::tab::{MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH};
use crate::{panes::PaneId, tab::Pane};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use zellij_utils::data::{Direction, Resize, ResizeStrategy};
use zellij_utils::{
    errors::prelude::*,
    input::layout::SplitDirection,
    pane_size::{Dimension, PaneGeom, Size, Viewport},
};

use std::cell::RefCell;
use std::rc::Rc;

const RESIZE_PERCENT: f64 = 5.0;
const DEFAULT_CURSOR_HEIGHT_WIDTH_RATIO: usize = 4;

type BorderAndPaneIds = (usize, Vec<PaneId>);

// For error reporting
fn no_pane_id(pane_id: &PaneId) -> String {
    format!("no floating pane with ID {:?} found", pane_id)
}

pub struct TiledPaneGrid<'a> {
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
    display_area: Size, // includes all panes (including eg. the status bar and tab bar in the default layout)
    viewport: Viewport, // includes all non-UI panes
}

impl<'a> TiledPaneGrid<'a> {
    pub fn new(
        panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        panes_to_hide: &HashSet<PaneId>,
        display_area: Size,
        viewport: Viewport,
    ) -> Self {
        let panes: HashMap<_, _> = panes
            .into_iter()
            .filter(|(p_id, _)| !panes_to_hide.contains(p_id))
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        TiledPaneGrid {
            panes: Rc::new(RefCell::new(panes)),
            display_area,
            viewport,
        }
    }

    /// Calculates an area for each pane and sums them all.
    ///
    /// Returns the product of "rows * columns", summed across all panes.
    fn total_panes_area(&self) -> f64 {
        let mut summed_area: f64 = 0.0;

        for pane in self.panes.clone().borrow().values() {
            if let PaneId::Terminal(_id) = pane.pid() {
                let geom = pane.current_geom();
                summed_area += match (geom.rows.as_percent(), geom.cols.as_percent()) {
                    (Some(rows), Some(cols)) => rows * cols,
                    _ => continue,
                };
            } else {
                continue;
            }
        }

        summed_area / (100.0 * 100.0)
    }

    pub fn layout(&mut self, direction: SplitDirection, space: usize) -> Result<()> {
        let mut pane_resizer = PaneResizer::new(self.panes.clone());
        pane_resizer.layout(direction, space)
    }

    // Check if panes in the desired direction can be resized. Returns the maximum resize that's
    // possible (at most `change_by`).
    pub fn can_change_pane_size(
        &self,
        pane_id: &PaneId,
        strategy: &ResizeStrategy,
        change_by: (f64, f64),
    ) -> Result<bool> {
        let err_context = || format!("failed to determine if pane {pane_id:?} can {strategy}");

        let pane_ids = if let Some(direction) = strategy.direction {
            self.pane_ids_directly_next_to(pane_id, &direction)
                .with_context(err_context)?
        } else {
            vec![]
        };

        use zellij_utils::data::Resize::Decrease as Dec;
        use zellij_utils::data::Resize::Increase as Inc;

        if !pane_ids.is_empty() {
            if strategy.direction_horizontal() {
                match strategy.resize {
                    Inc => {
                        for id in pane_ids {
                            if !self
                                .can_reduce_pane_width(&id, change_by.0 as f64)
                                .with_context(err_context)?
                            {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    },
                    Dec => self
                        .can_reduce_pane_width(pane_id, change_by.0 as f64)
                        .with_context(err_context),
                }
            } else if strategy.direction_vertical() {
                match strategy.resize {
                    Inc => {
                        for id in pane_ids {
                            if !self
                                .can_reduce_pane_height(&id, change_by.1 as f64)
                                .with_context(err_context)?
                            {
                                return Ok(false);
                            }
                        }
                        Ok(true)
                    },
                    Dec => self
                        .can_reduce_pane_height(pane_id, change_by.1 as f64)
                        .with_context(err_context),
                }
            } else {
                unimplemented!();
            }
        } else {
            // This is handled in `change_pane_size`, which will perform a check before a resize in
            // any single direction.
            Ok(true)
        }
    }

    /// Change a tiled panes size based on the given strategy.
    ///
    /// Returns true upon successful resize, false otherwise.
    pub fn change_pane_size(
        &mut self,
        pane_id: &PaneId,
        strategy: &ResizeStrategy,
        change_by: (f64, f64),
    ) -> Result<bool> {
        let err_context = || format!("failed to {strategy} by {change_by:?} for pane {pane_id:?}");

        // Default behavior is to only increase pane size, unless the direction being resized to is
        // a boundary. In this case, decrease size from the other side (invert strategy)!
        let strategy = if strategy.resize_increase()  // Only invert when increasing
            && strategy.invert_on_boundaries          // Only invert if configured to do so
            && strategy.direction.is_some()           // Only invert if there's a direction
            && strategy
                .direction
                .and_then(|direction| {
                    // Only invert if there are no neighbor IDs in the given direction
                    self.pane_ids_directly_next_to(pane_id, &direction)
                        .unwrap_or_default()
                        .is_empty()
                        .then_some(true)
                })
                .unwrap_or(false)
        {
            strategy.invert()
        } else {
            *strategy
        };

        if !self
            .can_change_pane_size(pane_id, &strategy, change_by)
            .with_context(err_context)?
        {
            // Resize not possible, quit
            return Ok(false);
        }

        if let Some(direction) = strategy.direction {
            let mut neighbor_terminals = self
                .pane_ids_directly_next_to(pane_id, &direction)
                .with_context(err_context)?;
            if neighbor_terminals.is_empty() {
                // Nothing to do.
                return Ok(false);
            }

            let neighbor_terminal_borders: HashSet<_> = if direction.is_horizontal() {
                neighbor_terminals
                    .iter()
                    .map(|t| self.panes.borrow().get(t).unwrap().y())
                    .collect()
            } else {
                neighbor_terminals
                    .iter()
                    .map(|t| self.panes.borrow().get(t).unwrap().x())
                    .collect()
            };

            let (some_terminals, other_terminals) = match direction {
                Direction::Left | Direction::Right => {
                    let (upper_borders, upper_terminals) = self
                        .left_aligned_contiguous_panes_above(pane_id, &neighbor_terminal_borders);
                    let (lower_borders, lower_terminals) = self
                        .left_aligned_contiguous_panes_below(pane_id, &neighbor_terminal_borders);
                    neighbor_terminals.retain(|t| {
                        self.pane_is_between_horizontal_borders(t, upper_borders, lower_borders)
                    });
                    (upper_terminals, lower_terminals)
                },
                Direction::Down | Direction::Up => {
                    let (left_borders, left_terminals) = self
                        .bottom_aligned_contiguous_panes_to_the_left(
                            pane_id,
                            &neighbor_terminal_borders,
                        );
                    let (right_borders, right_terminals) = self
                        .bottom_aligned_contiguous_panes_to_the_right(
                            pane_id,
                            &neighbor_terminal_borders,
                        );
                    neighbor_terminals.retain(|t| {
                        self.pane_is_between_vertical_borders(t, left_borders, right_borders)
                    });
                    (left_terminals, right_terminals)
                },
            };

            // Perform the resize
            let change_by = match direction {
                Direction::Left | Direction::Right => change_by.0,
                Direction::Down | Direction::Up => change_by.1,
            };

            if strategy.resize_increase() && direction.is_horizontal() {
                [*pane_id]
                    .iter()
                    .chain(&some_terminals)
                    .chain(&other_terminals)
                    .for_each(|pane| self.increase_pane_width(pane, change_by));
                neighbor_terminals
                    .iter()
                    .for_each(|pane| self.reduce_pane_width(pane, change_by));
            } else if strategy.resize_increase() && direction.is_vertical() {
                [*pane_id]
                    .iter()
                    .chain(&some_terminals)
                    .chain(&other_terminals)
                    .for_each(|pane| self.increase_pane_height(pane, change_by));
                neighbor_terminals
                    .iter()
                    .for_each(|pane| self.reduce_pane_height(pane, change_by));
            } else if strategy.resize_decrease() && direction.is_horizontal() {
                [*pane_id]
                    .iter()
                    .chain(&some_terminals)
                    .chain(&other_terminals)
                    .for_each(|pane| self.reduce_pane_width(pane, change_by));
                neighbor_terminals
                    .iter()
                    .for_each(|pane| self.increase_pane_width(pane, change_by));
            } else if strategy.resize_decrease() && direction.is_vertical() {
                [*pane_id]
                    .iter()
                    .chain(&some_terminals)
                    .chain(&other_terminals)
                    .for_each(|pane| self.reduce_pane_height(pane, change_by));
                neighbor_terminals
                    .iter()
                    .for_each(|pane| self.increase_pane_height(pane, change_by));
            } else {
                return Err(anyhow!(
                    "Don't know how to perform resize operation: '{strategy}'"
                ));
            }

            // Update grid
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            if direction.is_horizontal() {
                pane_resizer
                    .layout(SplitDirection::Horizontal, self.display_area.cols)
                    .with_context(err_context)?;
                return Ok(true);
            } else {
                pane_resizer
                    .layout(SplitDirection::Vertical, self.display_area.rows)
                    .with_context(err_context)?;
                return Ok(true);
            }
        } else {
            // Get panes aligned at corners, so we can change their sizes manually afterwards
            let mut aligned_panes = [
                None, // right, below
                None, // left, below
                None, // right, above
                None, // left, above
            ];
            // For the borrow checker
            {
                let panes = self.panes.borrow();
                let active_pane = panes
                    .get(pane_id)
                    .with_context(|| no_pane_id(pane_id))
                    .with_context(err_context)?;

                for p_id in self.viewport_pane_ids_directly_below(pane_id) {
                    let pane = panes
                        .get(&p_id)
                        .with_context(|| no_pane_id(&p_id))
                        .with_context(err_context)?;
                    if active_pane.x() + active_pane.cols() == pane.x() {
                        // right aligned
                        aligned_panes[0] = Some(p_id);
                    } else if active_pane.x() == pane.x() + pane.cols() {
                        // left aligned
                        aligned_panes[1] = Some(p_id);
                    }
                }
                for p_id in self.viewport_pane_ids_directly_above(pane_id) {
                    let pane = panes
                        .get(&p_id)
                        .with_context(|| no_pane_id(&p_id))
                        .with_context(err_context)?;
                    if active_pane.x() + active_pane.cols() == pane.x() {
                        // right aligned
                        aligned_panes[2] = Some(p_id);
                    } else if active_pane.x() == pane.x() + pane.cols() {
                        // left aligned
                        aligned_panes[3] = Some(p_id);
                    }
                }
            }

            // Resize pane in every direction that fits
            let mut result_map = vec![];
            for dir in [
                Direction::Left,
                Direction::Down,
                Direction::Up,
                Direction::Right,
            ] {
                let result = self.change_pane_size(
                    pane_id,
                    &ResizeStrategy {
                        direction: Some(dir),
                        invert_on_boundaries: false,
                        ..strategy
                    },
                    change_by,
                )?;
                result_map.push(result);
            }

            let resize = strategy.resize.invert();
            // left and down
            if result_map[0] && result_map[1] {
                if let Some(pane) = aligned_panes[1] {
                    self.change_pane_size(
                        &pane,
                        &ResizeStrategy::new(resize, Some(Direction::Right)),
                        change_by,
                    )?;
                }
            }
            // left and up
            if result_map[0] && result_map[2] {
                if let Some(pane) = aligned_panes[3] {
                    self.change_pane_size(
                        &pane,
                        &ResizeStrategy::new(resize, Some(Direction::Right)),
                        change_by,
                    )?;
                }
            }

            // right and down
            if result_map[3] && result_map[1] {
                if let Some(pane) = aligned_panes[0] {
                    self.change_pane_size(
                        &pane,
                        &ResizeStrategy::new(resize, Some(Direction::Up)),
                        change_by,
                    )?;
                }
            }

            // right and up
            if result_map[3] && result_map[2] {
                if let Some(pane) = aligned_panes[2] {
                    self.change_pane_size(
                        &pane,
                        &ResizeStrategy::new(resize, Some(Direction::Down)),
                        change_by,
                    )?;
                }
            }
        }

        #[cfg(debug_assertions)]
        {
            let area = self.total_panes_area() * 100.0;
            debug_assert!(
                f64::abs(area - 100.0) < 1.0, // Tolerate a little rounding error
                "area consumed by panes doesn't fill the viewport! Total area is {area} %",
            );
        }

        Ok(true)
    }

    pub fn resize_increase(&mut self, pane_id: &PaneId) {
        if self.try_increase_pane_and_surroundings_right_and_down(pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_down(pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_right_and_up(pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_up(pane_id) {
            return;
        }

        for dir in [
            Direction::Right,
            Direction::Down,
            Direction::Left,
            Direction::Up,
        ] {
            if self
                .try_resize_pane_and_surroundings(
                    pane_id,
                    &ResizeStrategy::new(Resize::Increase, Some(dir)),
                    (RESIZE_PERCENT, RESIZE_PERCENT),
                )
                .unwrap()
            {
                return;
            }
        }
    }

    fn can_resize_pane_and_surroundings(
        &self,
        pane_id: &PaneId,
        strategy: &ResizeStrategy,
        resize_by: (f64, f64),
    ) -> Result<bool> {
        if let Some(direction) = strategy.direction {
            let resize_by = if direction.is_horizontal() {
                resize_by.0
            } else {
                resize_by.1
            };

            if strategy.resize_increase() {
                self.can_increase_pane_and_surroundings(pane_id, &direction, resize_by)
            } else {
                self.can_reduce_pane_and_surroundings(pane_id, &direction, resize_by)
            }
        } else {
            // Not defined
            log::warn!("Requested undefined resize boundary check for {strategy:?}");
            Ok(false)
        }
    }

    fn can_reduce_pane_and_surroundings(
        &self,
        pane_id: &PaneId,
        direction: &Direction,
        reduce_by: f64,
    ) -> Result<bool> {
        let neighbor_ids = self.pane_ids_directly_next_to(pane_id, direction).unwrap();
        let flexible_ids = self
            .ids_are_flexible((*direction).into(), neighbor_ids)
            .unwrap();
        if flexible_ids && direction.is_horizontal() {
            self.can_reduce_pane_width(pane_id, reduce_by)
        } else {
            Ok(false)
        }
    }

    fn can_increase_pane_and_surroundings(
        &self,
        pane_id: &PaneId,
        direction: &Direction,
        increase_by: f64,
    ) -> Result<bool> {
        let err_context = || {
            format!("cannot determine if pane {pane_id:?} and surroundings can be resized by {increase_by} in direction {direction}")
        };

        self.pane_ids_directly_next_to(pane_id, direction)
            .and_then(|neighboring_panes| {
                for pane in neighboring_panes {
                    if direction.is_horizontal()
                        && !self.can_reduce_pane_width(&pane, increase_by)?
                    {
                        return Ok(false);
                    } else if direction.is_vertical()
                        && !self.can_reduce_pane_height(&pane, increase_by)?
                    {
                        return Ok(false);
                    }
                }
                return Ok(true);
            })
            .with_context(err_context)
    }

    fn can_reduce_pane_width(&self, pane_id: &PaneId, reduce_by: f64) -> Result<bool> {
        let err_context =
            || format!("failed to determine if pane {pane_id:?} can reduce width by {reduce_by} %");

        let panes = self.panes.borrow();
        let pane = panes
            .get(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        let current_fixed_cols = pane.position_and_size().cols.as_usize();
        let will_reduce_by = ((self.display_area.cols as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_cols.saturating_sub(will_reduce_by) < pane.min_width() {
            Ok(false)
        } else if let Some(cols) = pane.position_and_size().cols.as_percent() {
            Ok(cols - reduce_by >= RESIZE_PERCENT)
        } else {
            Ok(false)
        }
    }
    fn can_reduce_pane_height(&self, pane_id: &PaneId, reduce_by: f64) -> Result<bool> {
        let err_context = || {
            format!("failed to determine if pane {pane_id:?} can reduce height by {reduce_by} %")
        };

        let panes = self.panes.borrow();
        let pane = panes
            .get(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        let current_fixed_rows = pane.position_and_size().rows.as_usize();
        let will_reduce_by = ((self.display_area.rows as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_rows.saturating_sub(will_reduce_by) < pane.min_height() {
            Ok(false)
        } else if let Some(rows) = pane.position_and_size().rows.as_percent() {
            Ok(rows - reduce_by >= RESIZE_PERCENT)
        } else {
            Ok(false)
        }
    }

    fn reduce_pane_height(&mut self, id: &PaneId, percent: f64) {
        if self.can_reduce_pane_height(id, percent).unwrap() {
            let mut panes = self.panes.borrow_mut();
            let terminal = panes.get_mut(id).unwrap();
            terminal.reduce_height(percent);
        }
    }
    fn increase_pane_height(&mut self, id: &PaneId, percent: f64) {
        let mut panes = self.panes.borrow_mut();
        let terminal = panes.get_mut(id).unwrap();
        terminal.increase_height(percent);
    }
    fn increase_pane_width(&mut self, id: &PaneId, percent: f64) {
        let mut panes = self.panes.borrow_mut();
        let terminal = panes.get_mut(id).unwrap();
        terminal.increase_width(percent);
    }
    fn reduce_pane_width(&mut self, id: &PaneId, percent: f64) {
        if self.can_reduce_pane_width(id, percent).unwrap() {
            let mut panes = self.panes.borrow_mut();
            let terminal = panes.get_mut(id).unwrap();
            terminal.reduce_width(percent);
        }
    }

    // Return a vector of [`PaneId`]s directly adjacent to the given [`PaneId`], if any.
    //
    // The vector is empty for example if the given pane (`id`) is at the boundary of the viewport
    // already.
    fn pane_ids_directly_next_to(&self, id: &PaneId, direction: &Direction) -> Result<Vec<PaneId>> {
        let err_context = || format!("failed to find panes {direction} from pane {id:?}");

        let panes = self.panes.borrow();
        let mut ids = vec![];
        let terminal_to_check = panes
            .get(id)
            .with_context(|| no_pane_id(id))
            .with_context(err_context)?;

        for (&pid, terminal) in panes.iter() {
            // We cannot resize plugin panes, so we do not even bother trying.
            if let PaneId::Plugin(_) = pid {
                continue;
            }

            if match direction {
                Direction::Left => (terminal.x() + terminal.cols()) == terminal_to_check.x(),
                Direction::Down => {
                    terminal.y() == (terminal_to_check.y() + terminal_to_check.rows())
                },
                Direction::Up => (terminal.y() + terminal.rows()) == terminal_to_check.y(),
                Direction::Right => {
                    terminal.x() == (terminal_to_check.x() + terminal_to_check.cols())
                },
            } {
                ids.push(pid);
            }
        }
        Ok(ids)
    }

    fn pane_ids_top_aligned_with_pane_id(&self, pane_id: &PaneId) -> Vec<PaneId> {
        let panes = self.panes.borrow();
        let pane_to_check = panes.get(pane_id).unwrap();
        panes
            .iter()
            .filter(|(p_id, p)| *p_id != pane_id && p.y() == pane_to_check.y())
            .map(|(p_id, _p)| *p_id)
            .collect()
    }
    fn pane_ids_bottom_aligned_with_pane_id(&self, pane_id: &PaneId) -> Vec<PaneId> {
        let panes = self.panes.borrow();
        let pane_to_check = panes.get(pane_id).unwrap();
        panes
            .iter()
            .filter(|(p_id, p)| {
                *p_id != pane_id && p.y() + p.rows() == pane_to_check.y() + pane_to_check.rows()
            })
            .map(|(p_id, _p)| *p_id)
            .collect()
    }
    fn pane_ids_right_aligned_with_pane_id(&self, pane_id: &PaneId) -> Vec<PaneId> {
        let panes = self.panes.borrow();
        let pane_to_check = panes.get(pane_id).unwrap();
        panes
            .iter()
            .filter(|(p_id, p)| {
                *p_id != pane_id && p.x() + p.cols() == pane_to_check.x() + pane_to_check.cols()
            })
            .map(|(p_id, _p)| *p_id)
            .collect()
    }
    fn pane_ids_left_aligned_with_pane_id(&self, pane_id: &PaneId) -> Vec<PaneId> {
        let panes = self.panes.borrow();
        let pane_to_check = panes.get(pane_id).unwrap();
        panes
            .iter()
            .filter(|(p_id, p)| *p_id != pane_id && p.x() == pane_to_check.x())
            .map(|(p_id, _p)| *p_id)
            .collect()
    }
    fn right_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        pane_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut right_aligned_panes: Vec<_> = self
            .pane_ids_right_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        right_aligned_panes.sort_by_key(|a| Reverse(a.y()));
        for pane in right_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.y() + pane.rows() == pane_to_check.y() {
                result_panes.push(pane);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for pane in &result_panes {
            let bottom_pane_boundary = pane.y() + pane.rows();
            if pane_borders_to_the_right
                .get(&bottom_pane_boundary)
                .is_some()
                && top_resize_border < bottom_pane_boundary
            {
                top_resize_border = bottom_pane_boundary;
            }
        }
        result_panes.retain(|pane| pane.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.y()
        } else {
            top_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (top_resize_border, pane_ids)
    }
    fn right_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        pane_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut right_aligned_panes: Vec<_> = self
            .pane_ids_right_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        right_aligned_panes.sort_by_key(|a| a.y());
        for pane in right_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.y() == pane_to_check.y() + pane_to_check.rows() {
                result_panes.push(pane);
            }
        }
        // bottom-most border aligned with a pane border to the right
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for pane in &result_panes {
            let top_pane_boundary = pane.y();
            if pane_borders_to_the_right
                .get(&(top_pane_boundary))
                .is_some()
                && top_pane_boundary < bottom_resize_border
            {
                bottom_resize_border = top_pane_boundary;
            }
        }
        result_panes.retain(|pane| pane.y() + pane.rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.y() + pane_to_check.rows()
        } else {
            bottom_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, pane_ids)
    }
    fn left_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        pane_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut left_aligned_panes: Vec<_> = self
            .pane_ids_left_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        left_aligned_panes.sort_by_key(|a| Reverse(a.y()));
        for pane in left_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.y() + pane.rows() == pane_to_check.y() {
                result_panes.push(pane);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for pane in &result_panes {
            let bottom_pane_boundary = pane.y() + pane.rows();
            if pane_borders_to_the_left
                .get(&bottom_pane_boundary)
                .is_some()
                && top_resize_border < bottom_pane_boundary
            {
                top_resize_border = bottom_pane_boundary;
            }
        }
        result_panes.retain(|pane| pane.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.y()
        } else {
            top_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (top_resize_border, pane_ids)
    }
    fn left_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        pane_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut left_aligned_panes: Vec<_> = self
            .pane_ids_left_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        left_aligned_panes.sort_by_key(|a| a.y());
        for pane in left_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.y() == pane_to_check.y() + pane_to_check.rows() {
                result_panes.push(pane);
            }
        }
        // bottom-most border aligned with a pane border to the left
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for pane in &result_panes {
            let top_pane_boundary = pane.y();
            if pane_borders_to_the_left.get(&(top_pane_boundary)).is_some()
                && top_pane_boundary < bottom_resize_border
            {
                bottom_resize_border = top_pane_boundary;
            }
        }
        result_panes.retain(|pane| pane.y() + pane.rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.y() + pane_to_check.rows()
        } else {
            bottom_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, pane_ids)
    }
    fn top_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        pane_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut top_aligned_panes: Vec<_> = self
            .pane_ids_top_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        top_aligned_panes.sort_by_key(|a| Reverse(a.x()));
        for pane in top_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.x() + pane.cols() == pane_to_check.x() {
                result_panes.push(pane);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for pane in &result_panes {
            let right_pane_boundary = pane.x() + pane.cols();
            if pane_borders_above.get(&right_pane_boundary).is_some()
                && left_resize_border < right_pane_boundary
            {
                left_resize_border = right_pane_boundary
            }
        }
        result_panes.retain(|pane| pane.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.x()
        } else {
            left_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (left_resize_border, pane_ids)
    }
    fn top_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        pane_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut top_aligned_panes: Vec<_> = self
            .pane_ids_top_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        // panes that are next to each other up to current
        top_aligned_panes.sort_by_key(|a| a.x());
        for pane in top_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.x() == pane_to_check.x() + pane_to_check.cols() {
                result_panes.push(pane);
            }
        }
        // rightmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for pane in &result_panes {
            let left_pane_boundary = pane.x();
            if pane_borders_above.get(&left_pane_boundary).is_some()
                && right_resize_border > left_pane_boundary
            {
                right_resize_border = left_pane_boundary;
            }
        }
        result_panes.retain(|pane| pane.x() + pane.cols() <= right_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let right_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.x() + pane_to_check.cols()
        } else {
            right_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (right_resize_border, pane_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        pane_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut bottom_aligned_panes: Vec<_> = self
            .pane_ids_bottom_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        bottom_aligned_panes.sort_by_key(|a| Reverse(a.x()));
        // panes that are next to each other up to current
        for pane in bottom_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.x() + pane.cols() == pane_to_check.x() {
                result_panes.push(pane);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for pane in &result_panes {
            let right_pane_boundary = pane.x() + pane.cols();
            if pane_borders_below.get(&right_pane_boundary).is_some()
                && left_resize_border < right_pane_boundary
            {
                left_resize_border = right_pane_boundary;
            }
        }
        result_panes.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.x()
        } else {
            left_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (left_resize_border, pane_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        pane_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let panes = self.panes.borrow();
        let mut result_panes = vec![];
        let mut bottom_aligned_panes: Vec<_> = self
            .pane_ids_bottom_aligned_with_pane_id(id)
            .iter()
            .map(|p_id| panes.get(p_id).unwrap())
            .collect();
        bottom_aligned_panes.sort_by_key(|a| a.x());
        // panes that are next to each other up to current
        for pane in bottom_aligned_panes {
            let pane_to_check = panes.get(id).unwrap();
            let pane_to_check = result_panes.last().unwrap_or(&pane_to_check);
            if pane.x() == pane_to_check.x() + pane_to_check.cols() {
                result_panes.push(pane);
            }
        }
        // leftmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for pane in &result_panes {
            let left_pane_boundary = pane.x();
            if pane_borders_below.get(&left_pane_boundary).is_some()
                && right_resize_border > left_pane_boundary
            {
                right_resize_border = left_pane_boundary;
            }
        }
        result_panes.retain(|terminal| terminal.x() + terminal.cols() <= right_resize_border);
        let right_resize_border = if result_panes.is_empty() {
            let pane_to_check = panes.get(id).unwrap();
            pane_to_check.x() + pane_to_check.cols()
        } else {
            right_resize_border
        };
        let pane_ids: Vec<PaneId> = result_panes.iter().map(|t| t.pid()).collect();
        (right_resize_border, pane_ids)
    }

    // Returns true if none of the `pane_ids` has a `Fixed` dimension
    fn ids_are_flexible(&self, direction: SplitDirection, pane_ids: Vec<PaneId>) -> Result<bool> {
        let err_context =
            || format!("failed to determine if panes {pane_ids:?} are flexible in {direction:?}");

        let panes = self.panes.borrow();
        for id in pane_ids.iter() {
            let pane_to_check = panes
                .get(&id)
                .with_context(|| no_pane_id(&id))
                .with_context(err_context)?;
            let geom = pane_to_check.current_geom();
            let dimension = match direction {
                SplitDirection::Vertical => geom.rows,
                SplitDirection::Horizontal => geom.cols,
            };
            if dimension.is_fixed() {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn pane_is_between_vertical_borders(
        &self,
        id: &PaneId,
        left_border_x: usize,
        right_border_x: usize,
    ) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(id).unwrap();
        pane.x() >= left_border_x && pane.x() + pane.cols() <= right_border_x
    }
    fn pane_is_between_horizontal_borders(
        &self,
        id: &PaneId,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(id).unwrap();
        pane.y() >= top_border_y && pane.y() + pane.rows() <= bottom_border_y
    }

    fn try_resize_pane_and_surroundings(
        &mut self,
        pane_id: &PaneId,
        strategy: &ResizeStrategy,
        change_by: (f64, f64),
    ) -> Result<bool> {
        let err_context = || format!("failed to try and {strategy} for pane {pane_id:?}");

        if let Some(direction) = strategy.direction {
            let (split, size) = if direction.is_horizontal() {
                (SplitDirection::Horizontal, self.display_area.cols)
            } else {
                (SplitDirection::Vertical, self.display_area.rows)
            };

            if self
                .can_resize_pane_and_surroundings(pane_id, strategy, change_by)
                .with_context(err_context)?
            {
                self.change_pane_size(pane_id, strategy, change_by)
                    .with_context(err_context)?;
                let mut pane_resizer = PaneResizer::new(self.panes.clone());
                let _ = pane_resizer.layout(split, size);
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn try_increase_pane_and_surroundings_right_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_right = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Right, RESIZE_PERCENT)
            .unwrap();
        let can_increase_pane_up = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Up, RESIZE_PERCENT)
            .unwrap();
        if can_increase_pane_right && can_increase_pane_up {
            let pane_above_with_right_aligned_border = self
                .viewport_pane_ids_directly_above(pane_id)
                .iter()
                .copied()
                .find(|p_id| {
                    let panes = self.panes.borrow();
                    let pane = panes.get(p_id).unwrap();
                    let active_pane = panes.get(pane_id).unwrap();
                    active_pane.x() + active_pane.cols() == pane.x()
                });
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Right)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
            {
                self.try_resize_pane_and_surroundings(
                    &pane_above_with_right_aligned_border,
                    &ResizeStrategy::new(Resize::Decrease, Some(Direction::Left)),
                    (RESIZE_PERCENT, RESIZE_PERCENT),
                )
                .unwrap();
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_left_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_left = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Left, RESIZE_PERCENT)
            .unwrap();
        let can_increase_pane_up = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Up, RESIZE_PERCENT)
            .unwrap();
        if can_increase_pane_left && can_increase_pane_up {
            let pane_above_with_left_aligned_border = self
                .viewport_pane_ids_directly_above(pane_id)
                .iter()
                .copied()
                .find(|p_id| {
                    let panes = self.panes.borrow();
                    let pane = panes.get(p_id).unwrap();
                    let active_pane = panes.get(pane_id).unwrap();
                    active_pane.x() == pane.x() + pane.cols()
                });
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Left)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Up)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
                self.try_resize_pane_and_surroundings(
                    pane_id,
                    &ResizeStrategy::new(Resize::Decrease, Some(Direction::Right)),
                    (RESIZE_PERCENT, RESIZE_PERCENT),
                )
                .unwrap();
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_right_and_down(&mut self, pane_id: &PaneId) -> bool {
        let err_context = || {
            format!("failed to increase pane and surroundings right and down from pane {pane_id:?}")
        };

        let can_increase_pane_right = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Right, RESIZE_PERCENT)
            .unwrap();
        let can_increase_pane_down = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Down, RESIZE_PERCENT)
            .unwrap();
        if can_increase_pane_right && can_increase_pane_down {
            let pane_below_with_right_aligned_border = self
                .viewport_pane_ids_directly_below(pane_id)
                .iter()
                .copied()
                .find(|p_id| {
                    let panes = self.panes.borrow();
                    let pane = panes.get(p_id).unwrap();
                    let active_pane = panes.get(pane_id).unwrap();
                    active_pane.x() + active_pane.cols() == pane.x()
                });
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Right)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
            {
                self.try_resize_pane_and_surroundings(
                    pane_id,
                    &ResizeStrategy::new(Resize::Decrease, Some(Direction::Left)),
                    (RESIZE_PERCENT, RESIZE_PERCENT),
                )
                .unwrap();
                //self.try_reduce_pane_and_surroundings_right(
                //    &pane_below_with_right_aligned_border,
                //    RESIZE_PERCENT,
                //);
            }
            true
        } else {
            false
        }
    }

    fn try_increase_pane_and_surroundings_left_and_down(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_left = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Left, RESIZE_PERCENT)
            .unwrap();
        let can_increase_pane_down = self
            .can_increase_pane_and_surroundings(pane_id, &Direction::Down, RESIZE_PERCENT)
            .unwrap();
        if can_increase_pane_left && can_increase_pane_down {
            let pane_below_with_left_aligned_border = self
                .viewport_pane_ids_directly_below(pane_id)
                .iter()
                .copied()
                .find(|p_id| {
                    let panes = self.panes.borrow();
                    let pane = panes.get(p_id).unwrap();
                    let active_pane = panes.get(pane_id).unwrap();
                    active_pane.x() == pane.x() + pane.cols()
                });
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Left)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            self.try_resize_pane_and_surroundings(
                pane_id,
                &ResizeStrategy::new(Resize::Increase, Some(Direction::Down)),
                (RESIZE_PERCENT, RESIZE_PERCENT),
            )
            .unwrap();
            if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
                self.try_resize_pane_and_surroundings(
                    pane_id,
                    &ResizeStrategy::new(Resize::Decrease, Some(Direction::Right)),
                    (RESIZE_PERCENT, RESIZE_PERCENT),
                )
                .unwrap();
            }
            true
        } else {
            false
        }
    }
    //fn try_reduce_pane_and_surroundings_right_and_up(&mut self, pane_id: &PaneId) -> bool {
    //    let can_reduce_pane_right =
    //        self.can_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
    //    let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
    //    if can_reduce_pane_right && can_reduce_pane_up {
    //        let pane_below_with_left_aligned_border = self
    //            .viewport_pane_ids_directly_below(pane_id)
    //            .iter()
    //            .copied()
    //            .find(|p_id| {
    //                let panes = self.panes.borrow();
    //                let pane = panes.get(p_id).unwrap();
    //                let active_pane = panes.get(pane_id).unwrap();
    //                active_pane.x() == pane.x() + pane.cols()
    //            });
    //        self.try_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
    //        self.try_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
    //        if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
    //            self.try_increase_pane_and_surroundings_right(
    //                &pane_below_with_left_aligned_border,
    //                RESIZE_PERCENT,
    //            );
    //        }
    //        true
    //    } else {
    //        false
    //    }
    //}
    //fn try_reduce_pane_and_surroundings_left_and_up(&mut self, pane_id: &PaneId) -> bool {
    //    let can_reduce_pane_left =
    //        self.can_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
    //    let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
    //    if can_reduce_pane_left && can_reduce_pane_up {
    //        let pane_below_with_right_aligned_border = self
    //            .viewport_pane_ids_directly_below(pane_id)
    //            .iter()
    //            .copied()
    //            .find(|p_id| {
    //                let panes = self.panes.borrow();
    //                let pane = panes.get(p_id).unwrap();
    //                let active_pane = panes.get(pane_id).unwrap();
    //                active_pane.x() + active_pane.cols() == pane.x()
    //            });
    //        self.try_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
    //        self.try_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
    //        if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
    //        {
    //            self.try_increase_pane_and_surroundings_left(
    //                &pane_below_with_right_aligned_border,
    //                RESIZE_PERCENT,
    //            );
    //        }
    //        true
    //    } else {
    //        false
    //    }
    //}
    //fn try_reduce_pane_and_surroundings_right_and_down(&mut self, pane_id: &PaneId) -> bool {
    //    let can_reduce_pane_right =
    //        self.can_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
    //    let can_reduce_pane_down =
    //        self.can_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
    //    if can_reduce_pane_right && can_reduce_pane_down {
    //        let pane_above_with_left_aligned_border = self
    //            .viewport_pane_ids_directly_above(pane_id)
    //            .iter()
    //            .copied()
    //            .find(|p_id| {
    //                let panes = self.panes.borrow();
    //                let pane = panes.get(p_id).unwrap();
    //                let active_pane = panes.get(pane_id).unwrap();
    //                active_pane.x() == pane.x() + pane.cols()
    //            });
    //        self.try_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
    //        self.try_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
    //        if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
    //            self.try_increase_pane_and_surroundings_right(
    //                &pane_above_with_left_aligned_border,
    //                RESIZE_PERCENT,
    //            );
    //        }
    //        true
    //    } else {
    //        false
    //    }
    //}
    //fn try_reduce_pane_and_surroundings_left_and_down(&mut self, pane_id: &PaneId) -> bool {
    //    let can_reduce_pane_left =
    //        self.can_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
    //    let can_reduce_pane_down =
    //        self.can_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
    //    if can_reduce_pane_left && can_reduce_pane_down {
    //        let pane_above_with_right_aligned_border = self
    //            .viewport_pane_ids_directly_above(pane_id)
    //            .iter()
    //            .copied()
    //            .find(|p_id| {
    //                let panes = self.panes.borrow();
    //                let pane = panes.get(p_id).unwrap();
    //                let active_pane = panes.get(pane_id).unwrap();
    //                active_pane.x() + active_pane.cols() == pane.x()
    //            });
    //        self.try_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
    //        self.try_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
    //        if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
    //        {
    //            self.try_increase_pane_and_surroundings_left(
    //                &pane_above_with_right_aligned_border,
    //                RESIZE_PERCENT,
    //            );
    //        }
    //        true
    //    } else {
    //        false
    //    }
    //}

    fn viewport_pane_ids_directly_above(&self, pane_id: &PaneId) -> Vec<PaneId> {
        self.pane_ids_directly_next_to(pane_id, &Direction::Up)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| self.is_inside_viewport(id))
            .collect()
    }

    fn viewport_pane_ids_directly_below(&self, pane_id: &PaneId) -> Vec<PaneId> {
        self.pane_ids_directly_next_to(pane_id, &Direction::Down)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| self.is_inside_viewport(id))
            .collect()
    }

    fn is_inside_viewport(&self, pane_id: &PaneId) -> bool {
        is_inside_viewport(&self.viewport, self.panes.borrow().get(pane_id).unwrap())
    }

    pub fn next_selectable_pane_id(&self, current_pane_id: &PaneId) -> PaneId {
        let panes = self.panes.borrow();
        let mut panes: Vec<(&PaneId, &&mut Box<dyn Pane>)> =
            panes.iter().filter(|(_, p)| p.selectable()).collect();
        panes.sort_by(|(_a_id, a_pane), (_b_id, b_pane)| {
            if a_pane.y() == b_pane.y() {
                a_pane.x().cmp(&b_pane.x())
            } else {
                a_pane.y().cmp(&b_pane.y())
            }
        });
        let active_pane_position = panes
            .iter()
            .position(|(id, _)| *id == current_pane_id) // TODO: better
            .unwrap();

        let next_active_pane_id = panes
            .get(active_pane_position + 1)
            .or_else(|| panes.get(0))
            .map(|p| *p.0)
            .unwrap();
        next_active_pane_id
    }

    pub fn previous_selectable_pane_id(&self, current_pane_id: &PaneId) -> PaneId {
        let panes = self.panes.borrow();
        let mut panes: Vec<(&PaneId, &&mut Box<dyn Pane>)> =
            panes.iter().filter(|(_, p)| p.selectable()).collect();
        panes.sort_by(|(_a_id, a_pane), (_b_id, b_pane)| {
            if a_pane.y() == b_pane.y() {
                a_pane.x().cmp(&b_pane.x())
            } else {
                a_pane.y().cmp(&b_pane.y())
            }
        });
        let last_pane = panes.last().unwrap();
        let active_pane_position = panes
            .iter()
            .position(|(id, _)| *id == current_pane_id) // TODO: better
            .unwrap();

        let previous_active_pane_id = if active_pane_position == 0 {
            *last_pane.0
        } else {
            *panes.get(active_pane_position - 1).unwrap().0
        };
        previous_active_pane_id
    }

    pub fn next_selectable_pane_id_to_the_left(&self, current_pane_id: &PaneId) -> Option<PaneId> {
        let panes = self.panes.borrow();
        let current_pane = panes.get(current_pane_id)?;
        let panes: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable())
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let next_index = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_left_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    pub fn next_selectable_pane_id_below(&self, current_pane_id: &PaneId) -> Option<PaneId> {
        let panes = self.panes.borrow();
        let current_pane = panes.get(current_pane_id)?;
        let panes: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable())
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let next_index = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_below(Box::as_ref(current_pane))
                    && c.vertically_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    pub fn next_selectable_pane_id_above(&self, current_pane_id: &PaneId) -> Option<PaneId> {
        let panes = self.panes.borrow();
        let current_pane = panes.get(current_pane_id)?;
        let panes: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable())
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let next_index = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_above(Box::as_ref(current_pane))
                    && c.vertically_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    pub fn next_selectable_pane_id_to_the_right(&self, current_pane_id: &PaneId) -> Option<PaneId> {
        let panes = self.panes.borrow();
        let current_pane = panes.get(current_pane_id)?;
        let panes: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable())
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let next_index = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_right_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    fn horizontal_borders(&self, pane_ids: &[PaneId]) -> HashSet<usize> {
        pane_ids.iter().fold(HashSet::new(), |mut borders, p| {
            let panes = self.panes.borrow();
            let pane = panes.get(p).unwrap();
            borders.insert(pane.y());
            borders.insert(pane.y() + pane.rows());
            borders
        })
    }
    fn vertical_borders(&self, pane_ids: &[PaneId]) -> HashSet<usize> {
        pane_ids.iter().fold(HashSet::new(), |mut borders, p| {
            let panes = self.panes.borrow();
            let pane = panes.get(p).unwrap();
            borders.insert(pane.x());
            borders.insert(pane.x() + pane.cols());
            borders
        })
    }
    fn panes_to_the_left_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let upper_close_border = pane.y();
            let lower_close_border = pane.y() + pane.rows();

            let panes_to_the_left = self
                .pane_ids_directly_next_to(&id, &Direction::Left)
                .unwrap();
            let mut selectable_panes: Vec<_> = panes_to_the_left
                .into_iter()
                .filter(|pid| panes.get(pid).unwrap().selectable())
                .collect();
            let pane_borders_to_the_left = self.horizontal_borders(&selectable_panes);
            if pane_borders_to_the_left.contains(&upper_close_border)
                && pane_borders_to_the_left.contains(&lower_close_border)
            {
                selectable_panes.retain(|t| {
                    self.pane_is_between_horizontal_borders(
                        t,
                        upper_close_border,
                        lower_close_border,
                    )
                });
                return Some(selectable_panes);
            }
        }
        None
    }
    fn panes_to_the_right_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let upper_close_border = pane.y();
            let lower_close_border = pane.y() + pane.rows();

            let panes_to_the_right = self
                .pane_ids_directly_next_to(&id, &Direction::Right)
                .unwrap();
            let mut selectable_panes: Vec<_> = panes_to_the_right
                .into_iter()
                .filter(|pid| panes.get(pid).unwrap().selectable())
                .collect();
            let pane_borders_to_the_right = self.horizontal_borders(&selectable_panes);
            if pane_borders_to_the_right.contains(&upper_close_border)
                && pane_borders_to_the_right.contains(&lower_close_border)
            {
                selectable_panes.retain(|t| {
                    self.pane_is_between_horizontal_borders(
                        t,
                        upper_close_border,
                        lower_close_border,
                    )
                });
                return Some(selectable_panes);
            }
        }
        None
    }
    fn panes_above_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            let panes_above = self.pane_ids_directly_next_to(&id, &Direction::Up).unwrap();
            let mut selectable_panes: Vec<_> = panes_above
                .into_iter()
                .filter(|pid| panes.get(pid).unwrap().selectable())
                .collect();
            let pane_borders_above = self.vertical_borders(&selectable_panes);
            if pane_borders_above.contains(&left_close_border)
                && pane_borders_above.contains(&right_close_border)
            {
                selectable_panes.retain(|t| {
                    self.pane_is_between_vertical_borders(t, left_close_border, right_close_border)
                });
                return Some(selectable_panes);
            }
        }
        None
    }
    fn panes_below_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            let panes_below = self
                .pane_ids_directly_next_to(&id, &Direction::Down)
                .unwrap();
            let mut selectable_panes: Vec<_> = panes_below
                .into_iter()
                .filter(|pid| panes[pid].selectable())
                .collect();
            let pane_borders_below = self.vertical_borders(&selectable_panes);
            if pane_borders_below.contains(&left_close_border)
                && pane_borders_below.contains(&right_close_border)
            {
                selectable_panes.retain(|t| {
                    self.pane_is_between_vertical_borders(t, left_close_border, right_close_border)
                });
                return Some(selectable_panes);
            }
        }
        None
    }
    fn find_panes_to_grow(&self, id: PaneId) -> Option<(Vec<PaneId>, SplitDirection)> {
        if let Some(panes) = self
            .panes_to_the_left_between_aligning_borders(id)
            .or_else(|| self.panes_to_the_right_between_aligning_borders(id))
        {
            return Some((panes, SplitDirection::Horizontal));
        }

        if let Some(panes) = self
            .panes_above_between_aligning_borders(id)
            .or_else(|| self.panes_below_between_aligning_borders(id))
        {
            return Some((panes, SplitDirection::Vertical));
        }

        None
    }
    fn grow_panes(
        &mut self,
        panes: &[PaneId],
        direction: SplitDirection,
        (width, height): (f64, f64),
    ) {
        match direction {
            SplitDirection::Horizontal => {
                for pane_id in panes {
                    self.increase_pane_width(pane_id, width);
                }
            },
            SplitDirection::Vertical => {
                for pane_id in panes {
                    self.increase_pane_height(pane_id, height);
                }
            },
        };
    }
    pub fn fill_space_over_pane(&mut self, id: PaneId) -> bool {
        // true => successfully filled space over pane
        // false => didn't succeed, so didn't do anything
        let (freed_width, freed_height) = {
            let panes = self.panes.borrow_mut();
            let pane_to_close = panes.get(&id).unwrap();
            let freed_space = pane_to_close.position_and_size();
            let freed_width = freed_space.cols.as_percent();
            let freed_height = freed_space.rows.as_percent();
            (freed_width, freed_height)
        };
        if let (Some(freed_width), Some(freed_height)) = (freed_width, freed_height) {
            if let Some((panes_to_grow, direction)) = self.find_panes_to_grow(id) {
                self.grow_panes(&panes_to_grow, direction, (freed_width, freed_height));
                let side_length = match direction {
                    SplitDirection::Vertical => self.display_area.rows,
                    SplitDirection::Horizontal => self.display_area.cols,
                };
                self.panes.borrow_mut().remove(&id);
                let mut pane_resizer = PaneResizer::new(self.panes.clone());
                let _ = pane_resizer.layout(direction, side_length);
                return true;
            }
        }
        false
    }
    pub fn find_room_for_new_pane(
        &self,
        cursor_height_width_ratio: Option<usize>,
    ) -> Option<(PaneId, SplitDirection)> {
        let panes = self.panes.borrow();
        let pane_sequence: Vec<(&PaneId, &&mut Box<dyn Pane>)> =
            panes.iter().filter(|(_, p)| p.selectable()).collect();
        let (_largest_pane_size, pane_id_to_split) = pane_sequence.iter().fold(
            (0, None),
            |(current_largest_pane_size, current_pane_id_to_split), id_and_pane_to_check| {
                let (id_of_pane_to_check, pane_to_check) = id_and_pane_to_check;
                let pane_size = (pane_to_check.rows()
                    * cursor_height_width_ratio.unwrap_or(DEFAULT_CURSOR_HEIGHT_WIDTH_RATIO))
                    * pane_to_check.cols();
                let pane_can_be_split = pane_to_check.cols() >= MIN_TERMINAL_WIDTH
                    && pane_to_check.rows() >= MIN_TERMINAL_HEIGHT
                    && ((pane_to_check.cols() > pane_to_check.min_width() * 2)
                        || (pane_to_check.rows() > pane_to_check.min_height() * 2));
                if pane_can_be_split && pane_size > current_largest_pane_size {
                    (pane_size, Some(*id_of_pane_to_check))
                } else {
                    (current_largest_pane_size, current_pane_id_to_split)
                }
            },
        );
        pane_id_to_split.and_then(|t_id_to_split| {
            let pane_to_split = panes.get(t_id_to_split).unwrap();
            let direction = if pane_to_split.rows()
                * cursor_height_width_ratio.unwrap_or(DEFAULT_CURSOR_HEIGHT_WIDTH_RATIO)
                > pane_to_split.cols()
                && pane_to_split.rows() > pane_to_split.min_height() * 2
            {
                Some(SplitDirection::Horizontal)
            } else if pane_to_split.cols() > pane_to_split.min_width() * 2 {
                Some(SplitDirection::Vertical)
            } else {
                None
            };

            direction.map(|direction| (*t_id_to_split, direction))
        })
    }
}

pub fn split(direction: SplitDirection, rect: &PaneGeom) -> Option<(PaneGeom, PaneGeom)> {
    let space = match direction {
        SplitDirection::Vertical => rect.cols,
        SplitDirection::Horizontal => rect.rows,
    };
    if let Some(p) = space.as_percent() {
        let first_rect = match direction {
            SplitDirection::Vertical => PaneGeom {
                cols: Dimension::percent(p / 2.0),
                ..*rect
            },
            SplitDirection::Horizontal => PaneGeom {
                rows: Dimension::percent(p / 2.0),
                ..*rect
            },
        };
        let second_rect = match direction {
            SplitDirection::Vertical => PaneGeom {
                x: first_rect.x + 1,
                cols: first_rect.cols,
                ..*rect
            },
            SplitDirection::Horizontal => PaneGeom {
                y: first_rect.y + 1,
                rows: first_rect.rows,
                ..*rect
            },
        };
        Some((first_rect, second_rect))
    } else {
        None
    }
}
