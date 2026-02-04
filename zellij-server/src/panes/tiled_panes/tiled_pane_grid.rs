use super::is_inside_viewport;
use super::pane_resizer::PaneResizer;
use super::stacked_panes::StackedPanes;
use crate::tab::{MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH};
use crate::{panes::PaneId, tab::Pane};
use std::cmp::{Ordering, Reverse};
use std::collections::{HashMap, HashSet};
use zellij_utils::data::{Direction, Resize, ResizeStrategy};
use zellij_utils::{
    errors::prelude::*,
    input::layout::SplitDirection,
    pane_size::{Dimension, PaneGeom, Size, Viewport},
};

use std::cell::RefCell;
use std::rc::Rc;

pub const RESIZE_PERCENT: f64 = 5.0;
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

    pub fn layout(&mut self, direction: SplitDirection, space: usize) -> Result<()> {
        let mut pane_resizer = PaneResizer::new(self.panes.clone());
        pane_resizer.layout(direction, space)
    }
    pub fn get_pane_geom(&self, pane_id: &PaneId) -> Option<PaneGeom> {
        let panes = self.panes.borrow();
        let pane_to_check = panes.get(pane_id)?;
        let pane_geom = pane_to_check.current_geom();
        if pane_geom.is_stacked() {
            let mut stack_geom =
                StackedPanes::new(self.panes.clone()).position_and_size_of_stack(&pane_id);
            if let Some(stack_geom) = stack_geom.as_mut() {
                stack_geom.stacked = pane_geom.stacked; // to get the stack id
            }
            stack_geom
        } else {
            Some(pane_geom)
        }
    }

    fn pane_is_flexible(&self, direction: SplitDirection, pane_id: &PaneId) -> Result<bool> {
        let err_context =
            || format!("failed to determine if pane {pane_id:?} is flexible in {direction:?}");

        let pane_geom = self
            .get_pane_geom(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        Ok(!match direction {
            SplitDirection::Vertical => pane_geom.rows,
            SplitDirection::Horizontal => pane_geom.cols,
        }
        .is_fixed())
    }
    fn neighbor_pane_ids(&self, pane_id: &PaneId, direction: Direction) -> Result<Vec<PaneId>> {
        let err_context = || format!("Failed to get neighboring panes");
        // Shorthand
        use Direction as Dir;
        let mut neighbor_terminals = self
            .pane_ids_directly_next_to(pane_id, &direction)
            .with_context(err_context)?;

        let neighbor_terminal_borders: HashSet<_> = if direction.is_horizontal() {
            neighbor_terminals
                .iter()
                .filter_map(|t| self.get_pane_geom(t).map(|p| p.y))
                .collect()
        } else {
            neighbor_terminals
                .iter()
                .filter_map(|t| self.get_pane_geom(t).map(|p| p.x))
                .collect()
        };

        // Only return those neighbors that are aligned and between pane borders
        let (some_direction, other_direction) = match direction {
            Dir::Left | Dir::Right => (Dir::Up, Dir::Down),
            Dir::Down | Dir::Up => (Dir::Left, Dir::Right),
        };
        let (some_borders, _some_terminals) = self
            .contiguous_panes_with_alignment(
                pane_id,
                &neighbor_terminal_borders,
                &direction,
                &some_direction,
            )
            .with_context(err_context)?;
        let (other_borders, _other_terminals) = self
            .contiguous_panes_with_alignment(
                pane_id,
                &neighbor_terminal_borders,
                &direction,
                &other_direction,
            )
            .with_context(err_context)?;
        neighbor_terminals.retain(|t| {
            if direction.is_horizontal() {
                self.pane_is_between_horizontal_borders(t, some_borders, other_borders)
            } else {
                self.pane_is_between_vertical_borders(t, some_borders, other_borders)
            }
        });
        Ok(neighbor_terminals)
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

        if let Some(direction) = strategy.direction {
            if !self
                .pane_is_flexible(direction.into(), pane_id)
                .unwrap_or(false)
            {
                let pane_ids = match pane_id {
                    PaneId::Terminal(id) => vec![(*id, true)],
                    PaneId::Plugin(id) => vec![(*id, false)],
                };
                return Err(ZellijError::CantResizeFixedPanes { pane_ids })
                    .with_context(err_context);
            }
            let pane_ids = self
                .neighbor_pane_ids(pane_id, direction)
                .with_context(err_context)?;

            let fixed_panes: Vec<PaneId> = pane_ids
                .iter()
                .filter(|p| !self.pane_is_flexible(direction.into(), p).unwrap_or(false))
                .copied()
                .collect();
            if !fixed_panes.is_empty() {
                let mut pane_ids = vec![];
                for fixed_pane in fixed_panes {
                    match fixed_pane {
                        PaneId::Terminal(id) => pane_ids.push((id, true)),
                        PaneId::Plugin(id) => pane_ids.push((id, false)),
                    };
                }
                return Err(ZellijError::CantResizeFixedPanes { pane_ids })
                    .with_context(err_context);
            }
            if pane_ids.is_empty() {
                // TODO: proper error
                return Ok(false);
            }

            if direction.is_horizontal() {
                match strategy.resize {
                    Resize::Increase => {
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
                    Resize::Decrease => self
                        .can_reduce_pane_width(pane_id, change_by.0 as f64)
                        .with_context(err_context),
                }
            } else {
                match strategy.resize {
                    Resize::Increase => {
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
                    Resize::Decrease => self
                        .can_reduce_pane_height(pane_id, change_by.1 as f64)
                        .with_context(err_context),
                }
            }
        } else {
            // Undirected resize, this is checked elsewhere
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
        // Shorthand
        use Direction as Dir;
        let mut fixed_panes_blocking_resize = vec![];

        // Default behavior is to only increase pane size, unless the direction being resized to is
        // a boundary. In this case, decrease size from the other side (invert strategy)!
        let can_invert_strategy_if_needed = strategy.resize_increase()  // Only invert when increasing
            && strategy.invert_on_boundaries          // Only invert if configured to do so
            && strategy.direction.is_some(); // Only invert if there's a direction

        let can_change_pane_size_in_main_direction = self
            .can_change_pane_size(pane_id, &strategy, change_by)
            .unwrap_or_else(|err| {
                if let Some(ZellijError::CantResizeFixedPanes { pane_ids }) =
                    err.downcast_ref::<ZellijError>()
                {
                    fixed_panes_blocking_resize.append(&mut pane_ids.clone());
                }
                false
            });
        let can_change_pane_size_in_inverted_direction = if can_invert_strategy_if_needed {
            let strategy = strategy.invert();
            self.can_change_pane_size(pane_id, &strategy, change_by)
                .unwrap_or_else(|err| {
                    if let Some(ZellijError::CantResizeFixedPanes { pane_ids }) =
                        err.downcast_ref::<ZellijError>()
                    {
                        fixed_panes_blocking_resize.append(&mut pane_ids.clone());
                    }
                    false
                })
        } else {
            false
        };
        if strategy.direction.is_some()
            && !can_change_pane_size_in_main_direction
            && !can_change_pane_size_in_inverted_direction
        {
            // we can't resize in any direction, not playing the blame game, but I'm looking at
            // you: fixed_panes_blocking_resize
            return Err(ZellijError::CantResizeFixedPanes {
                pane_ids: fixed_panes_blocking_resize,
            })
            .with_context(err_context);
        }
        let strategy = if can_change_pane_size_in_main_direction {
            *strategy
        } else {
            strategy.invert()
        };

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
                    .filter_map(|p| self.get_pane_geom(p).map(|p| p.y))
                    .collect()
            } else {
                neighbor_terminals
                    .iter()
                    .filter_map(|p| self.get_pane_geom(p).map(|p| p.x))
                    .collect()
            };

            // Only resize those neighbors that are aligned and between pane borders
            let (some_direction, other_direction) = match direction {
                Dir::Left | Dir::Right => (Dir::Up, Dir::Down),
                Dir::Down | Dir::Up => (Dir::Left, Dir::Right),
            };
            let (some_borders, some_terminals) = self
                .contiguous_panes_with_alignment(
                    pane_id,
                    &neighbor_terminal_borders,
                    &direction,
                    &some_direction,
                )
                .with_context(err_context)?;
            let (other_borders, other_terminals) = self
                .contiguous_panes_with_alignment(
                    pane_id,
                    &neighbor_terminal_borders,
                    &direction,
                    &other_direction,
                )
                .with_context(err_context)?;
            neighbor_terminals.retain(|t| {
                if direction.is_horizontal() {
                    self.pane_is_between_horizontal_borders(t, some_borders, other_borders)
                } else {
                    self.pane_is_between_vertical_borders(t, some_borders, other_borders)
                }
            });

            // Perform the resize
            let change_by = match direction {
                Dir::Left | Dir::Right => change_by.0,
                Dir::Down | Dir::Up => change_by.1,
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
                ))
                .with_context(err_context);
            }

            // Update grid
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            if direction.is_horizontal() {
                pane_resizer
                    .layout(SplitDirection::Horizontal, self.display_area.cols)
                    .with_context(err_context)?;
            } else {
                pane_resizer
                    .layout(SplitDirection::Vertical, self.display_area.rows)
                    .with_context(err_context)?;
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
                // let panes = self.panes.borrow();
                let active_pane = self
                    .get_pane_geom(pane_id)
                    .with_context(|| no_pane_id(pane_id))
                    .with_context(err_context)?;

                for p_id in self.viewport_pane_ids_directly_below(pane_id) {
                    let pane = self
                        .get_pane_geom(&p_id)
                        .with_context(|| no_pane_id(&p_id))
                        .with_context(err_context)?;
                    if active_pane.x + active_pane.cols.as_usize() == pane.x {
                        // right aligned
                        aligned_panes[0] = Some(p_id);
                    } else if active_pane.x == pane.x + pane.cols.as_usize() {
                        // left aligned
                        aligned_panes[1] = Some(p_id);
                    }
                }
                for p_id in self.viewport_pane_ids_directly_above(pane_id) {
                    let pane = self
                        .get_pane_geom(&p_id)
                        .with_context(|| no_pane_id(&p_id))
                        .with_context(err_context)?;
                    if active_pane.x + active_pane.cols.as_usize() == pane.x {
                        // right aligned
                        aligned_panes[2] = Some(p_id);
                    } else if active_pane.x == pane.x + pane.cols.as_usize() {
                        // left aligned
                        aligned_panes[3] = Some(p_id);
                    }
                }
            }

            // Resize pane in every direction that fits
            let options = [
                (Dir::Right, Some(Dir::Down), Some(Dir::Left), 0),
                (Dir::Left, Some(Dir::Down), Some(Dir::Right), 1),
                (Dir::Right, Some(Dir::Up), Some(Dir::Left), 2),
                (Dir::Left, Some(Dir::Up), Some(Dir::Right), 3),
                (Dir::Right, None, None, 0),
                (Dir::Down, None, None, 0),
                (Dir::Left, None, None, 0),
                (Dir::Up, None, None, 0),
            ];

            for (main_dir, sub_dir, adjust_dir, adjust_pane) in options {
                if let Some(sub_dir) = sub_dir {
                    let main_strategy = ResizeStrategy {
                        direction: Some(main_dir),
                        invert_on_boundaries: false,
                        ..strategy
                    };
                    let sub_strategy = ResizeStrategy {
                        direction: Some(sub_dir),
                        invert_on_boundaries: false,
                        ..strategy
                    };

                    // TODO: instead of unwrap_or(false) here we need to do the same with the fixed
                    // panes error above, only make sure that we only error if we cannot resize in
                    // any directions and have blocking fixed panes
                    if self
                        .can_change_pane_size(pane_id, &main_strategy, change_by)
                        .unwrap_or(false)
                        && self
                            .can_change_pane_size(pane_id, &sub_strategy, change_by)
                            .unwrap_or(false)
                    {
                        let result = self
                            .change_pane_size(pane_id, &main_strategy, change_by)
                            .and_then(|ret| {
                                Ok(ret
                                    && self.change_pane_size(pane_id, &sub_strategy, change_by)?)
                            })
                            .and_then(|ret| {
                                if let Some(aligned_pane) = aligned_panes[adjust_pane] {
                                    Ok(ret
                                        && self.change_pane_size(
                                            &aligned_pane,
                                            &ResizeStrategy {
                                                direction: adjust_dir,
                                                invert_on_boundaries: false,
                                                resize: strategy.resize.invert(),
                                            },
                                            change_by,
                                        )?)
                                } else {
                                    Ok(ret)
                                }
                            })
                            .with_context(err_context)?;
                        return Ok(result);
                    }
                } else {
                    let new_strategy = ResizeStrategy {
                        direction: Some(main_dir),
                        invert_on_boundaries: false,
                        ..strategy
                    };
                    if self
                        .change_pane_size(pane_id, &new_strategy, change_by)
                        .unwrap_or(false)
                    {
                        return Ok(true);
                    }
                }
            }
            return Ok(false);
        }

        Ok(true)
    }

    fn can_reduce_pane_width(&self, pane_id: &PaneId, reduce_by: f64) -> Result<bool> {
        let err_context =
            || format!("failed to determine if pane {pane_id:?} can reduce width by {reduce_by} %");

        let pane = self
            .get_pane_geom(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        let current_fixed_cols = pane.cols.as_usize();
        let will_reduce_by = ((self.display_area.cols as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_cols.saturating_sub(will_reduce_by) < MIN_TERMINAL_WIDTH {
            Ok(false)
        } else if let Some(cols) = pane.cols.as_percent() {
            Ok(cols - reduce_by >= RESIZE_PERCENT)
        } else {
            Ok(false)
        }
    }
    fn can_reduce_pane_height(&self, pane_id: &PaneId, reduce_by: f64) -> Result<bool> {
        let err_context = || {
            format!("failed to determine if pane {pane_id:?} can reduce height by {reduce_by} %")
        };

        let pane = self
            .get_pane_geom(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        let min_terminal_height = if pane.is_stacked() {
            StackedPanes::new(self.panes.clone()).min_stack_height(pane_id)?
        } else {
            MIN_TERMINAL_HEIGHT
        };
        let current_fixed_rows = pane.rows.as_usize();
        let will_reduce_by = ((self.display_area.rows as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_rows.saturating_sub(will_reduce_by) < min_terminal_height {
            Ok(false)
        } else if let Some(rows) = pane.rows.as_percent() {
            Ok(rows - reduce_by >= RESIZE_PERCENT)
        } else {
            Ok(false)
        }
    }

    fn reduce_pane_height(&mut self, id: &PaneId, percent: f64) {
        if self.can_reduce_pane_height(id, percent).unwrap_or(false) {
            let current_pane_is_stacked = self
                .panes
                .borrow()
                .get(id)
                .unwrap()
                .current_geom()
                .is_stacked();
            if current_pane_is_stacked {
                let _ = StackedPanes::new(self.panes.clone()).reduce_stack_height(&id, percent);
            } else {
                let mut panes = self.panes.borrow_mut();
                let terminal = panes.get_mut(id).unwrap();
                terminal.reduce_height(percent);
            }
        }
    }
    fn increase_pane_height(&mut self, id: &PaneId, percent: f64) {
        let current_pane_is_stacked = self
            .panes
            .borrow()
            .get(id)
            .unwrap()
            .current_geom()
            .is_stacked();
        if current_pane_is_stacked {
            let _ = StackedPanes::new(self.panes.clone()).increase_stack_height(&id, percent);
        } else {
            let mut panes = self.panes.borrow_mut();
            let terminal = panes.get_mut(id).unwrap();
            terminal.increase_height(percent);
        }
    }
    fn increase_pane_width(&mut self, id: &PaneId, percent: f64) {
        let current_pane_is_stacked = self
            .panes
            .borrow()
            .get(id)
            .unwrap()
            .current_geom()
            .is_stacked();
        if current_pane_is_stacked {
            let _ = StackedPanes::new(self.panes.clone()).increase_stack_width(&id, percent);
        } else {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            pane.increase_width(percent);
        }
    }
    fn reduce_pane_width(&mut self, id: &PaneId, percent: f64) {
        if self.can_reduce_pane_width(id, percent).unwrap() {
            let current_pane_is_stacked = self
                .panes
                .borrow()
                .get(id)
                .unwrap()
                .current_geom()
                .is_stacked();
            if current_pane_is_stacked {
                let _ = StackedPanes::new(self.panes.clone()).reduce_stack_width(&id, percent);
            } else {
                let mut panes = self.panes.borrow_mut();
                let terminal = panes.get_mut(id).unwrap();
                terminal.reduce_width(percent);
            }
        }
    }

    /// Return a vector of [`PaneId`]s directly adjacent to the given [`PaneId`], if any.
    ///
    /// The vector is empty for example if the given pane (`id`) is at the boundary of the viewport
    /// already.
    fn pane_ids_directly_next_to(&self, id: &PaneId, direction: &Direction) -> Result<Vec<PaneId>> {
        let err_context = || format!("failed to find panes {direction} from pane {id:?}");

        let mut ids = vec![];
        let pane_geom_to_check = self
            .get_pane_geom(id)
            .with_context(|| no_pane_id(id))
            .with_context(err_context)?;

        let panes = self.panes.borrow();
        let mut seen = HashSet::new();
        for pid in panes.keys() {
            let pane = self
                .get_pane_geom(pid)
                .with_context(|| no_pane_id(id))
                .with_context(err_context)?;
            if seen.contains(&pane) {
                continue;
            } else {
                seen.insert(pane);
            }
            if match direction {
                Direction::Left => (pane.x + pane.cols.as_usize()) == pane_geom_to_check.x,
                Direction::Down => {
                    pane.y == (pane_geom_to_check.y + pane_geom_to_check.rows.as_usize())
                },
                Direction::Up => (pane.y + pane.rows.as_usize()) == pane_geom_to_check.y,
                Direction::Right => {
                    pane.x == (pane_geom_to_check.x + pane_geom_to_check.cols.as_usize())
                },
            } {
                ids.push(*pid);
            }
        }

        Ok(ids)
    }

    /// Return a vector of [`PaneId`]s aligned with the given [`PaneId`] on the `direction` border.
    fn pane_ids_aligned_with(
        &self,
        pane_id: &PaneId,
        direction: &Direction,
    ) -> Result<Vec<PaneId>> {
        let err_context = || format!("failed to find panes aligned {direction} with {pane_id:?}");

        let pane_to_check = self
            .get_pane_geom(pane_id)
            .with_context(|| no_pane_id(pane_id))
            .with_context(err_context)?;
        let mut result = vec![];

        let panes = self.panes.borrow();
        let mut seen = HashSet::new();
        for (pid, _pane) in panes.iter() {
            let pane = self
                .get_pane_geom(pid)
                .with_context(|| no_pane_id(pane_id))
                .with_context(err_context)?;
            if seen.contains(&pane) || pid == pane_id {
                continue;
            } else {
                seen.insert(pane);
            }

            if match direction {
                Direction::Left => pane.x == pane_to_check.x,
                Direction::Down => {
                    (pane.y + pane.rows.as_usize())
                        == (pane_to_check.y + pane_to_check.rows.as_usize())
                },
                Direction::Up => pane.y == pane_to_check.y,
                Direction::Right => {
                    (pane.x + pane.cols.as_usize())
                        == (pane_to_check.x + pane_to_check.cols.as_usize())
                },
            } {
                result.push(*pid)
            }
        }
        Ok(result)
    }

    /// Searches for contiguous panes
    fn contiguous_panes_with_alignment(
        &self,
        id: &PaneId,
        border: &HashSet<usize>,
        alignment: &Direction,
        direction: &Direction,
    ) -> Result<BorderAndPaneIds> {
        let err_context = || {
            format!("failed to find contiguous panes {direction} from pane {id:?} with {alignment} alignment")
        };
        let input_error =
            anyhow!("Invalid combination of alignment ({alignment}) and direction ({direction})");

        let pane_to_check = self
            .get_pane_geom(id)
            .with_context(|| no_pane_id(id))
            .with_context(err_context)?;
        let mut result = vec![];
        let mut aligned_panes: Vec<_> = self
            .pane_ids_aligned_with(id, alignment)
            .and_then(|pane_ids| {
                Ok(pane_ids
                    .iter()
                    .filter_map(|p_id| self.get_pane_geom(p_id).map(|pane_geom| (*p_id, pane_geom)))
                    .collect())
            })
            .with_context(err_context)?;

        use Direction::Down as D;
        use Direction::Left as L;
        use Direction::Right as R;
        use Direction::Up as U;

        match (alignment, direction) {
            (&R, &U) | (&L, &U) => aligned_panes.sort_by_key(|(_, a)| Reverse(a.y)),
            (&R, &D) | (&L, &D) => aligned_panes.sort_by_key(|(_, a)| a.y),
            (&D, &L) | (&U, &L) => aligned_panes.sort_by_key(|(_, a)| Reverse(a.x)),
            (&D, &R) | (&U, &R) => aligned_panes.sort_by_key(|(_, a)| a.x),
            _ => return Err(input_error).with_context(err_context),
        };

        for (pid, pane) in aligned_panes {
            let pane_to_check = result
                .last()
                .map(|(_pid, pane)| pane)
                .unwrap_or(&pane_to_check);
            if match (alignment, direction) {
                (&R, &U) | (&L, &U) => (pane.y + pane.rows.as_usize()) == pane_to_check.y,
                (&R, &D) | (&L, &D) => pane.y == (pane_to_check.y + pane_to_check.rows.as_usize()),
                (&D, &L) | (&U, &L) => (pane.x + pane.cols.as_usize()) == pane_to_check.x,
                (&D, &R) | (&U, &R) => pane.x == (pane_to_check.x + pane_to_check.cols.as_usize()),
                _ => return Err(input_error).with_context(err_context),
            } {
                result.push((pid, pane));
            }
        }

        let mut resize_border = match direction {
            &L => 0,
            &D => self.viewport.y + self.viewport.rows,
            &U => 0,
            &R => self.viewport.x + self.viewport.cols,
        };

        for (_, pane) in &result {
            let pane_boundary = match direction {
                &L => pane.x + pane.cols.as_usize(),
                &D => pane.y,
                &U => pane.y + pane.rows.as_usize(),
                &R => pane.x,
            };
            if border.get(&pane_boundary).is_some() {
                match direction {
                    &R | &D => {
                        if pane_boundary < resize_border {
                            resize_border = pane_boundary
                        }
                    },
                    &L | &U => {
                        if pane_boundary > resize_border {
                            resize_border = pane_boundary
                        }
                    },
                }
            }
        }
        result.retain(|(_pid, pane)| match direction {
            &L => pane.x >= resize_border,
            &D => (pane.y + pane.rows.as_usize()) <= resize_border,
            &U => pane.y >= resize_border,
            &R => (pane.x + pane.cols.as_usize()) <= resize_border,
        });

        let resize_border = if result.is_empty() {
            match direction {
                &L => pane_to_check.x,
                &D => pane_to_check.y + pane_to_check.rows.as_usize(),
                &U => pane_to_check.y,
                &R => pane_to_check.x + pane_to_check.cols.as_usize(),
            }
        } else {
            resize_border
        };
        let pane_ids: Vec<PaneId> = result.iter().map(|(pid, _t)| *pid).collect();

        Ok((resize_border, pane_ids))
    }

    fn pane_is_between_vertical_borders(
        &self,
        id: &PaneId,
        left_border_x: usize,
        right_border_x: usize,
    ) -> bool {
        let pane = self.get_pane_geom(id).unwrap();
        pane.x >= left_border_x && pane.x + pane.cols.as_usize() <= right_border_x
    }

    fn pane_is_between_horizontal_borders(
        &self,
        id: &PaneId,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let pane = self.get_pane_geom(id).unwrap();
        pane.y >= top_border_y && pane.y + pane.rows.as_usize() <= bottom_border_y
    }

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
        let next_pane = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_left_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (_, pane))| pane);
        let next_pane_is_stacked = next_pane
            .map(|p| p.current_geom().is_stacked())
            .unwrap_or(false);
        if next_pane_is_stacked {
            if let Some(next_pane_id) = next_pane.map(|p| p.pid()) {
                return StackedPanes::new(self.panes.clone())
                    .flexible_pane_id_in_stack(&next_pane_id);
            }
        }
        next_pane.map(|p| p.pid())
    }
    pub fn progress_stack_up_if_in_stack(&mut self, source_pane_id: &PaneId) -> Option<PaneId> {
        let destination_pane_id_in_stack = {
            let panes = self.panes.borrow();
            let source_pane = panes.get(source_pane_id)?;
            let pane_list: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
                .iter()
                .filter(|(_, p)| p.selectable())
                .map(|(p_id, p)| (*p_id, p))
                .collect();
            let destination_pane_id = pane_list
                .iter()
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_above(Box::as_ref(source_pane))
                        && c.vertically_overlaps_with(Box::as_ref(source_pane))
                        && c.current_geom().is_stacked()
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid)
                .copied();
            destination_pane_id
        };

        match destination_pane_id_in_stack {
            Some(destination_pane_id) => {
                StackedPanes::new(self.panes.clone())
                    .move_up(source_pane_id, &destination_pane_id)
                    .ok()?;
                Some(destination_pane_id)
            },
            None => None,
        }
    }
    pub fn progress_stack_down_if_in_stack(&mut self, source_pane_id: &PaneId) -> Option<PaneId> {
        let destination_pane_id_in_stack = {
            let panes = self.panes.borrow();
            let source_pane = panes.get(source_pane_id)?;
            let pane_list: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
                .iter()
                .filter(|(_, p)| p.selectable())
                .map(|(p_id, p)| (*p_id, p))
                .collect();
            let destination_pane_id = pane_list
                .iter()
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_below(Box::as_ref(source_pane))
                        && c.vertically_overlaps_with(Box::as_ref(source_pane))
                        && c.current_geom().is_stacked()
                })
                .max_by_key(|(_, (_, c))| c.active_at())
                .map(|(_, (pid, _))| pid)
                .copied();
            destination_pane_id
        };

        match destination_pane_id_in_stack {
            Some(destination_pane_id) => {
                StackedPanes::new(self.panes.clone())
                    .move_down(source_pane_id, &destination_pane_id)
                    .ok()?;
                Some(destination_pane_id)
            },
            None => None,
        }
    }
    pub fn next_selectable_pane_id_below(
        &self,
        current_pane_id: &PaneId,
        include_panes_in_stack: bool,
    ) -> Option<PaneId> {
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
                if include_panes_in_stack {
                    c.is_directly_below(Box::as_ref(current_pane))
                        && c.vertically_overlaps_with(Box::as_ref(current_pane))
                } else {
                    c.is_directly_below(Box::as_ref(current_pane))
                        && c.vertically_overlaps_with(Box::as_ref(current_pane))
                        && !c.current_geom().is_stacked()
                }
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    pub fn pane_id_on_edge(&self, direction: Direction) -> Option<PaneId> {
        let panes = self.panes.borrow();
        let panes: Vec<(PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable())
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let next_index = panes
            .iter()
            .enumerate()
            .max_by(|(_, (_, a)), (_, (_, b))| match direction {
                Direction::Left => {
                    let x_comparison = a.x().cmp(&b.x());
                    match x_comparison {
                        Ordering::Equal => a.active_at().cmp(&b.active_at()),
                        _ => x_comparison,
                    }
                },
                Direction::Right => {
                    let x_comparison = b.x().cmp(&a.x());
                    match x_comparison {
                        Ordering::Equal => a.active_at().cmp(&b.active_at()),
                        _ => x_comparison,
                    }
                },
                Direction::Up => {
                    let y_comparison = a.y().cmp(&b.y());
                    match y_comparison {
                        Ordering::Equal => a.active_at().cmp(&b.active_at()),
                        _ => y_comparison,
                    }
                },
                Direction::Down => {
                    let y_comparison = b.y().cmp(&a.y());
                    match y_comparison {
                        Ordering::Equal => a.active_at().cmp(&b.active_at()),
                        _ => y_comparison,
                    }
                },
            })
            .map(|(_, (pid, _))| pid)
            .copied();
        next_index
    }
    pub fn next_selectable_pane_id_above(
        &self,
        current_pane_id: &PaneId,
        include_panes_in_stack: bool,
    ) -> Option<PaneId> {
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
                if include_panes_in_stack {
                    c.is_directly_above(Box::as_ref(current_pane))
                        && c.vertically_overlaps_with(Box::as_ref(current_pane))
                } else {
                    c.is_directly_above(Box::as_ref(current_pane))
                        && c.vertically_overlaps_with(Box::as_ref(current_pane))
                        && !c.current_geom().is_stacked()
                }
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
        let next_pane = panes
            .iter()
            .enumerate()
            .filter(|(_, (_, c))| {
                c.is_directly_right_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by_key(|(_, (_, c))| c.active_at())
            .map(|(_, (_pid, pane))| pane)
            .copied();
        let next_pane_is_stacked = next_pane
            .map(|p| p.current_geom().is_stacked())
            .unwrap_or(false);
        if next_pane_is_stacked {
            if let Some(next_pane_id) = next_pane.map(|p| p.pid()) {
                return StackedPanes::new(self.panes.clone())
                    .flexible_pane_id_in_stack(&next_pane_id);
            }
        }
        next_pane.map(|p| p.pid())
    }
    fn horizontal_borders(&self, pane_ids: &[PaneId]) -> HashSet<usize> {
        pane_ids.iter().fold(HashSet::new(), |mut borders, p| {
            let panes = self.panes.borrow();
            let pane = panes.get(p).unwrap();
            if pane.current_geom().is_stacked() {
                let pane_geom = StackedPanes::new(self.panes.clone())
                    .position_and_size_of_stack(&pane.pid())
                    .unwrap();
                borders.insert(pane_geom.y);
                borders.insert(pane_geom.y + pane_geom.rows.as_usize());
            } else {
                borders.insert(pane.y());
                borders.insert(pane.y() + pane.rows());
            }
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
        let (freed_width, freed_height, pane_to_close_is_stacked) = {
            let panes = self.panes.borrow_mut();
            let Some(pane_to_close) = panes.get(&id) else {
                return false;
            };
            let freed_space = pane_to_close.position_and_size();
            let freed_width = freed_space.cols.as_percent();
            let freed_height = freed_space.rows.as_percent();
            let pane_to_close_is_stacked = pane_to_close.current_geom().is_stacked();
            (freed_width, freed_height, pane_to_close_is_stacked)
        };
        if pane_to_close_is_stacked {
            let successfully_filled_space = StackedPanes::new(self.panes.clone())
                .fill_space_over_pane_in_stack(&id)
                .unwrap_or(false);
            if successfully_filled_space {
                return true;
            }
        }
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
        } else {
            // best effort resize - we just remove the pane and relayout everything
            // this might happen if we are closing a fixed pane
            self.panes.borrow_mut().remove(&id);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(SplitDirection::Horizontal, self.display_area.cols);
            let _ = pane_resizer.layout(SplitDirection::Vertical, self.display_area.rows);
            return true;
        }
        false
    }
    pub fn find_room_for_new_pane(
        &self,
        cursor_height_width_ratio: Option<usize>,
    ) -> Option<(PaneId, SplitDirection)> {
        let panes = self.panes.borrow();
        let pane_sequence: Vec<(&PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| p.selectable() && !p.current_geom().is_stacked())
            .collect();
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
            let Some(pane_to_split) = panes.get(t_id_to_split) else {
                return None;
            };
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
    pub fn split_pane(
        &self,
        active_pane_id: &PaneId,
        cursor_height_width_ratio: Option<usize>,
    ) -> Option<(PaneId, SplitDirection)> {
        // right now the minimum here is hard-coded to a sane "I don't want my terminal smaller"
        // number, but we might want to change this to be a percentage of the current screen if it
        // feels better
        let panes = self.panes.borrow();
        let Some(pane_to_split) = panes.get(active_pane_id) else {
            return None;
        };
        let direction = if pane_to_split.rows()
            * cursor_height_width_ratio.unwrap_or(DEFAULT_CURSOR_HEIGHT_WIDTH_RATIO)
            > pane_to_split.cols()
            && pane_to_split.rows() > 10 * 2
        {
            Some(SplitDirection::Horizontal)
        } else if pane_to_split.cols() > 30 * 2 {
            Some(SplitDirection::Vertical)
        } else {
            None
        };

        direction.map(|direction| (*active_pane_id, direction))
    }
    pub fn has_room_for_new_stacked_pane(&self) -> bool {
        let panes = self.panes.borrow();
        let flexible_pane_in_stack: Vec<(&PaneId, &&mut Box<dyn Pane>)> = panes
            .iter()
            .filter(|(_, p)| {
                p.selectable() && p.current_geom().is_stacked() && !p.current_geom().rows.is_fixed()
            })
            .collect();
        flexible_pane_in_stack
            .iter()
            .any(|(_p_id, p)| p.current_geom().rows.as_usize() > MIN_TERMINAL_HEIGHT)
    }
    pub fn room_left_in_stack_of_pane_id(&mut self, pane_id: &PaneId) -> Option<usize> {
        StackedPanes::new(self.panes.clone()).room_left_in_stack_of_pane_id(pane_id)
    }
    pub fn make_room_in_stack_for_pane(&mut self) -> Result<PaneGeom> {
        StackedPanes::new(self.panes.clone()).make_room_for_new_pane()
    }
    pub fn make_room_in_stack_of_pane_id_for_pane(&mut self, pane_id: &PaneId) -> Result<PaneGeom> {
        StackedPanes::new(self.panes.clone()).make_room_for_new_pane_in_stack(pane_id)
    }
    fn pane_ids_have_the_same_y(&self, pane_ids: &[PaneId]) -> bool {
        let panes = self.panes.borrow();
        let mut pane_y = None;
        let mut panes_all_have_the_same_y = true;
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                match pane_y {
                    Some(pane_y) => {
                        if pane_y != pane.position_and_size().y {
                            panes_all_have_the_same_y = false;
                        }
                    },
                    None => {
                        pane_y = Some(pane.position_and_size().y);
                    },
                }
            }
        }
        panes_all_have_the_same_y
    }
    fn group_panes_by_highest_y(&self, pane_ids: &[PaneId]) -> (Vec<PaneId>, Vec<PaneId>) {
        // returns (left to right)
        // 1. panes with the highest y (geometrically closer to the bottom)
        // 2. if there are panes left, returns them as the second group
        let panes = self.panes.borrow();
        let mut highest_pane_y = 0;
        let mut first_group = vec![];
        let mut second_group = vec![];
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                let pane_geom = pane.position_and_size();
                let previous_highest_y = highest_pane_y;
                highest_pane_y = std::cmp::max(pane_geom.y, highest_pane_y);
                if previous_highest_y != highest_pane_y {
                    second_group.append(&mut first_group);
                }
                if pane_geom.y == highest_pane_y {
                    first_group.push(*p_id);
                } else {
                    second_group.push(*p_id);
                }
            }
        }
        (first_group, second_group)
    }
    fn group_panes_by_highest_x(&self, pane_ids: &[PaneId]) -> (Vec<PaneId>, Vec<PaneId>) {
        // returns (left to right)
        // 1. panes with the highest x (geometrically closer to the left)
        // 2. if there are panes remaining, returns them as the second group
        let panes = self.panes.borrow();
        let mut highest_pane_x = 0;
        let mut first_group = vec![];
        let mut second_group = vec![];
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                let pane_geom = pane.position_and_size();
                let previous_highest_x = highest_pane_x;
                highest_pane_x = std::cmp::max(pane_geom.x, highest_pane_x);
                if previous_highest_x != highest_pane_x {
                    second_group.append(&mut first_group);
                }
                if pane_geom.x == highest_pane_x {
                    first_group.push(*p_id);
                } else {
                    second_group.push(*p_id);
                }
            }
        }
        (first_group, second_group)
    }
    fn group_panes_by_lowest_rows(&self, pane_ids: &[PaneId]) -> (Vec<PaneId>, Vec<PaneId>) {
        // returns (left to right)
        // 1. panes with the lowest rows
        // 2. if there are panes left, returns them as the second group
        // we expect all given panes here to have the same y
        let panes = self.panes.borrow();
        let mut lowest_row_count = None;
        let mut first_group = vec![];
        let mut second_group = vec![];
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                let pane_geom = pane.position_and_size();
                let previous_lowest_row_count =
                    lowest_row_count.unwrap_or_else(|| pane_geom.rows.as_usize());
                lowest_row_count = Some(std::cmp::min(
                    pane_geom.rows.as_usize(),
                    lowest_row_count.unwrap_or_else(|| pane_geom.rows.as_usize()),
                ));
                if Some(previous_lowest_row_count) != lowest_row_count {
                    second_group.append(&mut first_group);
                }
                if Some(pane_geom.rows.as_usize()) == lowest_row_count {
                    first_group.push(*p_id);
                } else {
                    second_group.push(*p_id);
                }
            }
        }
        (first_group, second_group)
    }
    fn group_panes_by_lowest_cols(&self, pane_ids: &[PaneId]) -> (Vec<PaneId>, Vec<PaneId>) {
        // returns (left to right)
        // 1. panes with the lowest cols
        // 2. if there are panes left, returns them as the second group
        // we expect all given panes here to have the same x
        let panes = self.panes.borrow();
        let mut lowest_col_count = None;
        let mut first_group = vec![];
        let mut second_group = vec![];
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                let pane_geom = pane.position_and_size();
                let previous_lowest_col_count =
                    lowest_col_count.unwrap_or_else(|| pane_geom.cols.as_usize());
                lowest_col_count = Some(std::cmp::min(
                    pane_geom.cols.as_usize(),
                    lowest_col_count.unwrap_or_else(|| pane_geom.cols.as_usize()),
                ));
                if Some(previous_lowest_col_count) != lowest_col_count {
                    second_group.append(&mut first_group);
                }
                if Some(pane_geom.cols.as_usize()) == lowest_col_count {
                    first_group.push(*p_id);
                } else {
                    second_group.push(*p_id);
                }
            }
        }
        (first_group, second_group)
    }
    fn pane_ids_have_the_same_x(&self, pane_ids: &[PaneId]) -> bool {
        let panes = self.panes.borrow();
        let mut pane_x = None;
        let mut panes_all_have_the_same_x = true;
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                match pane_x {
                    Some(pane_x) => {
                        if pane_x != pane.position_and_size().x {
                            panes_all_have_the_same_x = false;
                        }
                    },
                    None => {
                        pane_x = Some(pane.position_and_size().x);
                    },
                }
            }
        }
        panes_all_have_the_same_x
    }
    fn pane_ids_have_the_same_height(&self, pane_ids: &[PaneId]) -> bool {
        let panes = self.panes.borrow();
        let mut pane_height = None;
        let mut panes_all_have_the_same_height = true;
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                match pane_height {
                    Some(pane_height) => {
                        if pane_height != pane.position_and_size().rows.as_usize() {
                            panes_all_have_the_same_height = false;
                        }
                    },
                    None => {
                        pane_height = Some(pane.position_and_size().rows.as_usize());
                    },
                }
            }
        }
        panes_all_have_the_same_height
    }
    fn pane_ids_have_the_same_width(&self, pane_ids: &[PaneId]) -> bool {
        let panes = self.panes.borrow();
        let mut pane_width = None;
        let mut panes_all_have_the_same_width = true;
        for p_id in pane_ids {
            if let Some(pane) = panes.get(p_id) {
                match pane_width {
                    Some(pane_width) => {
                        if pane_width != pane.position_and_size().cols.as_usize() {
                            panes_all_have_the_same_width = false;
                        }
                    },
                    None => {
                        pane_width = Some(pane.position_and_size().cols.as_usize());
                    },
                }
            }
        }
        panes_all_have_the_same_width
    }
    pub fn direct_neighboring_pane_ids_above(&self, root_pane_id: &PaneId) -> Vec<PaneId> {
        // here we look for panes that are directly above the provided root pane but that do not
        // exceed its vertical borders (x and x + cols)
        let Some(root_pane_geom) = self.get_pane_geom(root_pane_id) else {
            log::error!("Could nto find root pane geom");
            return vec![];
        };
        let Some(neighbor_pane_ids) = self.neighbor_pane_ids(root_pane_id, Direction::Up).ok()
        else {
            log::error!("Could not find neighbor pane ids above");
            return vec![];
        };
        let neighbor_pane_ids = neighbor_pane_ids
            .iter()
            .filter(|pane_id| {
                self.pane_is_between_vertical_borders(
                    pane_id,
                    root_pane_geom.x,
                    root_pane_geom.x + root_pane_geom.cols.as_usize(),
                )
            })
            .copied()
            .collect::<Vec<_>>();
        // we only want to return these if they cover the entire vertical surface of the root pane id
        // (as in - one of the panes contains its x and one of the panes contains its x + cols)
        let mut boundaries_of_pane_ids = vec![];
        for p_id in &neighbor_pane_ids {
            let mut vertical_boundaries_of_pane = self.get_vertical_boundaries_of_pane(p_id);
            boundaries_of_pane_ids.append(&mut vertical_boundaries_of_pane);
        }
        if boundaries_of_pane_ids.contains(&root_pane_geom.x)
            && boundaries_of_pane_ids.contains(&(root_pane_geom.x + root_pane_geom.cols.as_usize()))
        {
            neighbor_pane_ids
        } else {
            vec![]
        }
    }
    fn get_vertical_boundaries_of_pane(&self, pane_id: &PaneId) -> Vec<usize> {
        let Some(geom_of_pane) = self.get_pane_geom(pane_id) else {
            log::error!("Could not find geom of pawne");
            return vec![];
        };
        vec![
            geom_of_pane.x,
            geom_of_pane.x + geom_of_pane.cols.as_usize(),
        ]
    }
    fn get_horizontal_boundaries_of_pane(&self, pane_id: &PaneId) -> Vec<usize> {
        let Some(geom_of_pane) = self.get_pane_geom(pane_id) else {
            log::error!("Could not find geom of pawne");
            return vec![];
        };
        vec![
            geom_of_pane.y,
            geom_of_pane.y + geom_of_pane.rows.as_usize(),
        ]
    }
    fn fill_geom_holes_horizontally_upwards(
        &mut self,
        pane_ids_to_expand: &[PaneId],
        holes: &[PaneId],
    ) -> Result<()> {
        // here we fill in the pane_ids_to_expand over the holes horizontally and then shorten the
        // height of the holes by the pane_ids_to_expand (squeeze them upwards)
        // we expect the pane_ids_to_expand to all have the same y and height and for the holes to
        // all be higher
        let err_context = || format!("Failed to fill_geom_holes_horizontally_upwards");
        let mut panes_to_expand = vec![];
        let mut hole_panes = vec![];
        for p_id in pane_ids_to_expand {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            panes_to_expand.push((p_id, pane.position_and_size()));
        }
        for p_id in holes {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            hole_panes.push((p_id, pane.position_and_size()));
        }
        panes_to_expand.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.x.cmp(&b_geom.x));
        hole_panes.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.x.cmp(&b_geom.x));
        let mut uncovered_hole = None;
        for (hole_id, mut hole_geom) in hole_panes {
            let hole_x = hole_geom.x;
            let Some((pane_id_with_closest_x, mut pane_geom_with_closest_x)) = panes_to_expand
                .iter()
                .find(|(_p_id, p_geom)| p_geom.x >= hole_x)
            else {
                // can happen if the last geom was a hole
                uncovered_hole = Some((hole_id, hole_geom));
                continue;
            };
            pane_geom_with_closest_x
                .cols
                .increase_inner(hole_geom.cols.as_usize());
            pane_geom_with_closest_x.x = hole_x;
            hole_geom.rows.reduce_by(
                pane_geom_with_closest_x
                    .rows
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_x.rows.as_usize(),
            );
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_x)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_x);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        if let Some((hole_id, mut hole_geom)) = uncovered_hole {
            let (pane_id_with_closest_x, mut pane_geom_with_closest_x) =
                panes_to_expand.last().with_context(err_context)?;
            pane_geom_with_closest_x
                .cols
                .increase_inner(hole_geom.cols.as_usize());
            hole_geom.rows.reduce_by(
                pane_geom_with_closest_x
                    .rows
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_x.rows.as_usize(),
            );
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_x)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_x);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        Ok(())
    }
    fn fill_geom_holes_horizontally_downwards(
        &mut self,
        pane_ids_to_expand: &[PaneId],
        holes: &[PaneId],
    ) -> Result<()> {
        // here we fill in the pane_ids_to_expand over the holes horizontally and then shorten the
        // height of the holes by the pane_ids_to_expand, as well as increase their y by the same
        // count (squeeze them downwards)
        // we expect the pane_ids_to_expand to all have the same y and height and for the holes to
        // all be higher and have the same y
        let err_context = || format!("Failed to fill_geom_holes_horizontally_downwards");
        let mut panes_to_expand = vec![];
        let mut hole_panes = vec![];
        for p_id in pane_ids_to_expand {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            panes_to_expand.push((p_id, pane.position_and_size()));
        }
        for p_id in holes {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            hole_panes.push((p_id, pane.position_and_size()));
        }
        panes_to_expand.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.x.cmp(&b_geom.x));
        hole_panes.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.x.cmp(&b_geom.x));
        let mut uncovered_hole = None;
        for (hole_id, mut hole_geom) in hole_panes {
            let hole_x = hole_geom.x;
            let Some((pane_id_with_closest_x, mut pane_geom_with_closest_x)) = panes_to_expand
                .iter()
                .find(|(_p_id, p_geom)| p_geom.x >= hole_x)
            else {
                // can happen if the last geom was a hole
                uncovered_hole = Some((hole_id, hole_geom));
                continue;
            };
            pane_geom_with_closest_x
                .cols
                .increase_inner(hole_geom.cols.as_usize());
            pane_geom_with_closest_x.x = hole_x;
            hole_geom.rows.reduce_by(
                pane_geom_with_closest_x
                    .rows
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_x.rows.as_usize(),
            );
            hole_geom.y += pane_geom_with_closest_x.rows.as_usize();
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_x)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_x);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        if let Some((hole_id, mut hole_geom)) = uncovered_hole {
            let (pane_id_with_closest_x, mut pane_geom_with_closest_x) =
                panes_to_expand.last().with_context(err_context)?;
            pane_geom_with_closest_x
                .cols
                .increase_inner(hole_geom.cols.as_usize());
            hole_geom.rows.reduce_by(
                pane_geom_with_closest_x
                    .rows
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_x.rows.as_usize(),
            );
            hole_geom.y += pane_geom_with_closest_x.rows.as_usize();
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_x)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_x);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        Ok(())
    }
    fn fill_geom_holes_vertically_to_the_right(
        &mut self,
        pane_ids_to_expand: &[PaneId],
        holes: &[PaneId],
    ) -> Result<()> {
        // here we fill in the pane_ids_to_expand over the holes vertically and then shorten the
        // width of the holes by the pane_ids_to_expand, as well as increase their x by the same
        // count (squeeze them to the right)
        // we expect the pane_ids_to_expand to all have the same x and width and for the holes to
        // all be wider and have the same x
        let err_context = || format!("Failed to fill_geom_holes_vertically_to_the_right");
        let mut panes_to_expand = vec![];
        let mut hole_panes = vec![];
        for p_id in pane_ids_to_expand {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            panes_to_expand.push((p_id, pane.position_and_size()));
        }
        for p_id in holes {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            hole_panes.push((p_id, pane.position_and_size()));
        }
        panes_to_expand.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.y.cmp(&b_geom.y));
        hole_panes.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.y.cmp(&b_geom.y));
        let mut uncovered_hole = None;
        for (hole_id, mut hole_geom) in hole_panes {
            let hole_y = hole_geom.y;
            let Some((pane_id_with_closest_y, mut pane_geom_with_closest_y)) = panes_to_expand
                .iter()
                .find(|(_p_id, p_geom)| p_geom.y >= hole_y)
            else {
                // can happen if the last geom was a hole
                uncovered_hole = Some((hole_id, hole_geom));
                continue;
            };
            pane_geom_with_closest_y
                .rows
                .increase_inner(hole_geom.rows.as_usize());
            pane_geom_with_closest_y.y = hole_y;
            hole_geom.cols.reduce_by(
                pane_geom_with_closest_y
                    .cols
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_y.cols.as_usize(),
            );
            hole_geom.x += pane_geom_with_closest_y.cols.as_usize();
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_y)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_y);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        if let Some((hole_id, mut hole_geom)) = uncovered_hole {
            let (pane_id_with_closest_y, mut pane_geom_with_closest_y) =
                panes_to_expand.last().with_context(err_context)?;
            pane_geom_with_closest_y
                .rows
                .increase_inner(hole_geom.rows.as_usize());
            hole_geom.cols.reduce_by(
                pane_geom_with_closest_y
                    .cols
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_y.cols.as_usize(),
            );
            hole_geom.x += pane_geom_with_closest_y.cols.as_usize();
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_y)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_y);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        Ok(())
    }
    fn fill_geom_holes_vertically_to_the_left(
        &mut self,
        pane_ids_to_expand: &[PaneId],
        holes: &[PaneId],
    ) -> Result<()> {
        // here we fill in the pane_ids_to_expand over the holes vertically and then shorten the
        // width of the holes by the pane_ids_to_expand's width (squeeze them to the right)
        // we expect the pane_ids_to_expand to all have the same x and width and for the holes to
        // all be wider
        let err_context = || format!("Failed to fill_geom_holes_vertically_to_the_left");
        let mut panes_to_expand = vec![];
        let mut hole_panes = vec![];
        for p_id in pane_ids_to_expand {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            panes_to_expand.push((p_id, pane.position_and_size()));
        }
        for p_id in holes {
            let panes = self.panes.borrow();
            let pane = panes.get(&p_id).with_context(err_context)?;
            hole_panes.push((p_id, pane.position_and_size()));
        }
        panes_to_expand.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.y.cmp(&b_geom.y));
        hole_panes.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| a_geom.y.cmp(&b_geom.y));
        let mut uncovered_hole = None;
        for (hole_id, mut hole_geom) in hole_panes {
            let hole_y = hole_geom.y;
            let Some((pane_id_with_closest_y, mut pane_geom_with_closest_y)) = panes_to_expand
                .iter()
                .find(|(_p_id, p_geom)| p_geom.y >= hole_y)
            else {
                // can happen if the last geom was a hole
                uncovered_hole = Some((hole_id, hole_geom));
                continue;
            };
            pane_geom_with_closest_y
                .rows
                .increase_inner(hole_geom.rows.as_usize());
            pane_geom_with_closest_y.y = hole_y;
            hole_geom.cols.reduce_by(
                pane_geom_with_closest_y
                    .cols
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_y.cols.as_usize(),
            );
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_y)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_y);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        if let Some((hole_id, mut hole_geom)) = uncovered_hole {
            let (pane_id_with_closest_y, mut pane_geom_with_closest_y) =
                panes_to_expand.last().with_context(err_context)?;
            pane_geom_with_closest_y
                .rows
                .increase_inner(hole_geom.rows.as_usize());
            hole_geom.cols.reduce_by(
                pane_geom_with_closest_y
                    .cols
                    .as_percent()
                    .with_context(err_context)?,
                pane_geom_with_closest_y.cols.as_usize(),
            );
            self.panes
                .borrow_mut()
                .get_mut(&pane_id_with_closest_y)
                .with_context(err_context)?
                .set_geom(pane_geom_with_closest_y);
            self.panes
                .borrow_mut()
                .get_mut(&hole_id)
                .with_context(err_context)?
                .set_geom(hole_geom);
        }
        Ok(())
    }
    pub fn stack_pane_up(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let mut neighboring_pane_ids_above = self.direct_neighboring_pane_ids_above(pane_id);
        if !self.pane_ids_have_the_same_y(&neighboring_pane_ids_above) {
            let (panes_with_highest_y, leftover_panes) =
                self.group_panes_by_highest_y(&neighboring_pane_ids_above);
            if let Err(e) =
                self.fill_geom_holes_horizontally_upwards(&panes_with_highest_y, &leftover_panes)
            {
                log::error!("Failed to fill_geom_holes_horizontally upwards: {}", e);
                return None;
            }
            neighboring_pane_ids_above = panes_with_highest_y;
        }
        let pane_is_selectable = |pane_id| {
            self.panes
                .borrow()
                .get(pane_id)
                .map(|pane| pane.selectable())
                .unwrap_or(false)
        };
        if neighboring_pane_ids_above.is_empty()
            || neighboring_pane_ids_above
                .iter()
                .any(|p| !pane_is_selectable(p))
        {
            return None;
        }
        StackedPanes::new(self.panes.clone())
            .combine_vertically_aligned_panes_to_stack(&pane_id, neighboring_pane_ids_above)
            .non_fatal();
        StackedPanes::new(self.panes.clone())
            .expand_pane(&pane_id)
            .non_fatal();
        Some(vec![*pane_id])
    }
    pub fn unstack_pane_up(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let pane_is_stacked = self
            .get_pane_geom(pane_id)
            .map(|pane_geom| pane_geom.is_stacked())
            .unwrap_or(false);
        if pane_is_stacked {
            StackedPanes::new(self.panes.clone()).break_pane_out_of_stack(&pane_id)
        } else {
            None
        }
    }
    pub fn direct_neighboring_pane_ids_below(&self, root_pane_id: &PaneId) -> Vec<PaneId> {
        // here we look for panes that are directly below the provided root pane but that do not
        // exceed its vertical borders (x and x + cols)
        let Some(root_pane_geom) = self.get_pane_geom(root_pane_id) else {
            log::error!("Could nto find root pane geom");
            return vec![];
        };
        let Some(neighbor_pane_ids) = self.neighbor_pane_ids(root_pane_id, Direction::Down).ok()
        else {
            log::error!("Could not find neighbor pane ids above");
            return vec![];
        };
        let neighbor_pane_ids = neighbor_pane_ids
            .iter()
            .filter(|pane_id| {
                self.pane_is_between_vertical_borders(
                    pane_id,
                    root_pane_geom.x,
                    root_pane_geom.x + root_pane_geom.cols.as_usize(),
                )
            })
            .copied()
            .collect::<Vec<_>>();
        // we only want to return these if they cover the entire vertical surface of the root pane id
        // (as in - one of the panes contains its x and one of the panes contains its x + cols)
        let mut boundaries_of_pane_ids = vec![];
        for p_id in &neighbor_pane_ids {
            let mut vertical_boundaries_of_pane = self.get_vertical_boundaries_of_pane(p_id);
            boundaries_of_pane_ids.append(&mut vertical_boundaries_of_pane);
        }
        if boundaries_of_pane_ids.contains(&root_pane_geom.x)
            && boundaries_of_pane_ids.contains(&(root_pane_geom.x + root_pane_geom.cols.as_usize()))
        {
            neighbor_pane_ids
        } else {
            vec![]
        }
    }
    pub fn stack_pane_down(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let mut neighboring_pane_ids_below = self.direct_neighboring_pane_ids_below(pane_id);
        if !self.pane_ids_have_the_same_height(&neighboring_pane_ids_below) {
            let (panes_with_lowest_rows, leftover_panes) =
                self.group_panes_by_lowest_rows(&neighboring_pane_ids_below);
            if let Err(e) = self
                .fill_geom_holes_horizontally_downwards(&panes_with_lowest_rows, &leftover_panes)
            {
                log::error!("Failed to fill_geom_holes_horizontally downwards: {}", e);
                return None;
            }
            neighboring_pane_ids_below = panes_with_lowest_rows;
        }
        let pane_is_selectable = |pane_id| {
            self.panes
                .borrow()
                .get(pane_id)
                .map(|pane| pane.selectable())
                .unwrap_or(false)
        };
        if neighboring_pane_ids_below.is_empty()
            || neighboring_pane_ids_below
                .iter()
                .any(|p| !pane_is_selectable(p))
        {
            return None;
        }
        StackedPanes::new(self.panes.clone())
            .combine_vertically_aligned_panes_to_stack(&pane_id, neighboring_pane_ids_below)
            .non_fatal();
        StackedPanes::new(self.panes.clone())
            .expand_pane(&pane_id)
            .non_fatal();
        Some(vec![*pane_id])
    }
    pub fn direct_neighboring_pane_ids_to_the_left(&self, root_pane_id: &PaneId) -> Vec<PaneId> {
        // here we look for panes that are directly to the left the provided root pane but that do not
        // exceed its horizontal borders (y and y + rows)
        let Some(root_pane_geom) = self.get_pane_geom(root_pane_id) else {
            log::error!("Could nto find root pane geom");
            return vec![];
        };
        let Some(neighbor_pane_ids) = self.neighbor_pane_ids(root_pane_id, Direction::Left).ok()
        else {
            log::error!("Could not find neighbor pane ids to the left");
            return vec![];
        };
        let neighbor_pane_ids = neighbor_pane_ids
            .iter()
            .filter(|pane_id| {
                self.pane_is_between_horizontal_borders(
                    pane_id,
                    root_pane_geom.y,
                    root_pane_geom.y + root_pane_geom.rows.as_usize(),
                )
            })
            .copied()
            .collect::<Vec<_>>();
        // we only want to return these if they cover the entire horizontal surface of the root pane id
        // (as in - one of the panes contains its y and one of the panes contains its y + rows)
        let mut boundaries_of_pane_ids = vec![];
        for p_id in &neighbor_pane_ids {
            let mut horizontal_boundaries_of_pane = self.get_horizontal_boundaries_of_pane(p_id);
            boundaries_of_pane_ids.append(&mut horizontal_boundaries_of_pane);
        }
        if boundaries_of_pane_ids.contains(&root_pane_geom.y)
            && boundaries_of_pane_ids.contains(&(root_pane_geom.y + root_pane_geom.rows.as_usize()))
        {
            neighbor_pane_ids
        } else {
            vec![]
        }
    }
    pub fn stack_pane_left(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let mut neighboring_pane_ids_to_the_left =
            self.direct_neighboring_pane_ids_to_the_left(pane_id);
        if !self.pane_ids_have_the_same_x(&neighboring_pane_ids_to_the_left) {
            let (panes_with_highest_x, leftover_panes) =
                self.group_panes_by_highest_x(&neighboring_pane_ids_to_the_left);
            if let Err(e) =
                self.fill_geom_holes_vertically_to_the_left(&panes_with_highest_x, &leftover_panes)
            {
                log::error!("Failed to fill_geom_holes_vertically_to_the_left: {}", e);
                return None;
            }
            neighboring_pane_ids_to_the_left = panes_with_highest_x;
        }
        let pane_is_selectable = |pane_id| {
            self.panes
                .borrow()
                .get(pane_id)
                .map(|pane| pane.selectable())
                .unwrap_or(false)
        };
        if neighboring_pane_ids_to_the_left.is_empty()
            || neighboring_pane_ids_to_the_left
                .iter()
                .any(|p| !pane_is_selectable(p))
        {
            return None;
        }
        StackedPanes::new(self.panes.clone())
            .combine_horizontally_aligned_panes_to_stack(&pane_id, neighboring_pane_ids_to_the_left)
            .non_fatal();
        StackedPanes::new(self.panes.clone())
            .expand_pane(&pane_id)
            .non_fatal();
        Some(vec![*pane_id])
    }
    pub fn direct_neighboring_pane_ids_to_the_right(&self, root_pane_id: &PaneId) -> Vec<PaneId> {
        // here we look for panes that are directly to the right the provided root pane but that do not
        // exceed its horizontal borders (y and y + rows)
        let Some(root_pane_geom) = self.get_pane_geom(root_pane_id) else {
            log::error!("Could nto find root pane geom");
            return vec![];
        };
        let Some(neighbor_pane_ids) = self.neighbor_pane_ids(root_pane_id, Direction::Right).ok()
        else {
            log::error!("Could not find neighbor pane ids to the right");
            return vec![];
        };
        let neighbor_pane_ids = neighbor_pane_ids
            .iter()
            .filter(|pane_id| {
                self.pane_is_between_horizontal_borders(
                    pane_id,
                    root_pane_geom.y,
                    root_pane_geom.y + root_pane_geom.rows.as_usize(),
                )
            })
            .copied()
            .collect::<Vec<_>>();
        // we only want to return these if they cover the entire horizontal surface of the root pane id
        // (as in - one of the panes contains its y and one of the panes contains its y + rows)
        let mut boundaries_of_pane_ids = vec![];
        for p_id in &neighbor_pane_ids {
            let mut horizontal_boundaries_of_pane = self.get_horizontal_boundaries_of_pane(p_id);
            boundaries_of_pane_ids.append(&mut horizontal_boundaries_of_pane);
        }
        if boundaries_of_pane_ids.contains(&root_pane_geom.y)
            && boundaries_of_pane_ids.contains(&(root_pane_geom.y + root_pane_geom.rows.as_usize()))
        {
            neighbor_pane_ids
        } else {
            vec![]
        }
    }
    pub fn stack_pane_right(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let mut neighboring_pane_ids_to_the_right =
            self.direct_neighboring_pane_ids_to_the_right(pane_id);
        if !self.pane_ids_have_the_same_width(&neighboring_pane_ids_to_the_right) {
            let (panes_with_lowest_cols, leftover_panes) =
                self.group_panes_by_lowest_cols(&neighboring_pane_ids_to_the_right);
            if let Err(e) = self
                .fill_geom_holes_vertically_to_the_right(&panes_with_lowest_cols, &leftover_panes)
            {
                log::error!("Failed to fill_geom_holes_vertically_to_the_right: {}", e);
                return None;
            }
            neighboring_pane_ids_to_the_right = panes_with_lowest_cols;
        }
        let pane_is_selectable = |pane_id| {
            self.panes
                .borrow()
                .get(pane_id)
                .map(|pane| pane.selectable())
                .unwrap_or(false)
        };
        if neighboring_pane_ids_to_the_right.is_empty()
            || neighboring_pane_ids_to_the_right
                .iter()
                .any(|p| !pane_is_selectable(p))
        {
            return None;
        }
        if neighboring_pane_ids_to_the_right.is_empty() {
            return None;
        }
        StackedPanes::new(self.panes.clone())
            .combine_horizontally_aligned_panes_to_stack(
                &pane_id,
                neighboring_pane_ids_to_the_right,
            )
            .non_fatal();
        StackedPanes::new(self.panes.clone())
            .expand_pane(&pane_id)
            .non_fatal();
        Some(vec![*pane_id])
    }
    pub fn next_stack_id(&self) -> usize {
        StackedPanes::new(self.panes.clone()).next_stack_id()
    }
    pub fn make_pane_stacked(&mut self, pane_id: &PaneId) -> Result<()> {
        let mut geom_of_active_pane = self
            .get_pane_geom(pane_id)
            .ok_or_else(|| anyhow!("Failed to get pane geom"))?;
        geom_of_active_pane.stacked = Some(self.next_stack_id());
        self.panes
            .borrow_mut()
            .get_mut(pane_id)
            .ok_or_else(|| anyhow!("Failed to get pane geom"))?
            .set_geom(geom_of_active_pane);
        Ok(())
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
                logical_position: None,
                ..*rect
            },
            SplitDirection::Horizontal => PaneGeom {
                y: first_rect.y + 1,
                rows: first_rect.rows,
                logical_position: None,
                ..*rect
            },
        };
        Some((first_rect, second_rect))
    } else {
        None
    }
}
