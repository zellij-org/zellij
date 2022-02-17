use super::pane_resizer::PaneResizer;
use crate::tab::{is_inside_viewport, MIN_TERMINAL_WIDTH, MIN_TERMINAL_HEIGHT};
use crate::{panes::PaneId, tab::Pane};
use std::cmp::{Ordering, Reverse};
use std::collections::{HashMap, HashSet};
use zellij_utils::{
    input::layout::Direction,
    pane_size::{Dimension, PaneGeom, Size, Viewport},
};

use std::cell::RefCell;
use std::rc::Rc;

const RESIZE_PERCENT: f64 = 5.0;
const RESIZE_INCREMENT_WIDTH: usize = 5;
const RESIZE_INCREMENT_HEIGHT: usize = 2;
const MOVE_INCREMENT_HORIZONTAL: usize = 10;
const MOVE_INCREMENT_VERTICAL: usize = 5;
const CURSOR_HEIGHT_WIDTH_RATIO: usize = 4; // this is not accurate and kind of a magic number, TODO: look into this

const MAX_PANES: usize = 100;
type BorderAndPaneIds = (usize, Vec<PaneId>);

pub struct PaneGrid<'a> {
    // panes: HashMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
    display_area: Size, // includes all panes (including eg. the status bar and tab bar in the default layout)
    viewport: Viewport, // includes all non-UI panes
}

impl<'a> PaneGrid<'a> {
    pub fn new(
        panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        display_area: Size,
        viewport: Viewport,
    ) -> Self {
        let panes: HashMap<_, _> = panes.into_iter().map(|(p_id, p)| (*p_id, p)).collect();
        PaneGrid {
            panes: Rc::new(RefCell::new(panes)),
            display_area,
            viewport,
        }
    }

    pub fn layout(&mut self, direction: Direction, space: usize) -> Result<(), String> {
        let mut pane_resizer = PaneResizer::new(self.panes.clone());
        pane_resizer.layout(direction, space)
    }
    pub fn resize_pane_left(&'a mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            // let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
        } else if self.can_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
        }
    }
    pub fn resize_pane_right(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
        } else if self.can_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
        }
    }
    pub fn resize_pane_down(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
        } else if self.can_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
        }
    }
    pub fn resize_pane_up(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
        } else if self.can_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
        }
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

        if self.try_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT) {
            return;
        }
        self.try_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
    }
    pub fn resize_decrease(&mut self, pane_id: &PaneId) {
        if self.try_reduce_pane_and_surroundings_left_and_up(pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_up(pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_down(pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left_and_down(pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT) {
            return;
        }
        self.try_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
    }
    fn can_increase_pane_and_surroundings_right(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        let panes = self.panes.borrow();
        if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(pane_id) {
            panes_to_the_right.iter().all(|id| {
                let p = panes.get(id).unwrap();
                if let Some(cols) = p.position_and_size().cols.as_percent() {
                    let current_fixed_cols = p.position_and_size().cols.as_usize();
                    let will_reduce_by =
                        ((self.display_area.cols as f64 / 100.0) * increase_by) as usize;
                    cols - increase_by >= RESIZE_PERCENT
                        && current_fixed_cols.saturating_sub(will_reduce_by) >= p.min_width()
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_left(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        let panes = self.panes.borrow();
        if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(pane_id) {
            panes_to_the_left.iter().all(|id| {
                let p = panes.get(id).unwrap();
                if let Some(cols) = p.position_and_size().cols.as_percent() {
                    let current_fixed_cols = p.position_and_size().cols.as_usize();
                    let will_reduce_by =
                        ((self.display_area.cols as f64 / 100.0) * increase_by) as usize;
                    cols - increase_by >= RESIZE_PERCENT
                        && current_fixed_cols.saturating_sub(will_reduce_by) >= p.min_width()
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_down(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        let panes = self.panes.borrow();
        if let Some(panes_below) = self.pane_ids_directly_below(pane_id) {
            panes_below.iter().all(|id| {
                let p = panes.get(id).unwrap();
                if let Some(rows) = p.position_and_size().rows.as_percent() {
                    let current_fixed_rows = p.position_and_size().rows.as_usize();
                    let will_reduce_by =
                        ((self.display_area.rows as f64 / 100.0) * increase_by) as usize;
                    rows - increase_by >= RESIZE_PERCENT
                        && current_fixed_rows.saturating_sub(will_reduce_by) >= p.min_height()
                } else {
                    false
                }
            })
        } else {
            false
        }
    }

    fn can_increase_pane_and_surroundings_up(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        let panes = self.panes.borrow();
        if let Some(panes_above) = self.pane_ids_directly_above(pane_id) {
            panes_above.iter().all(|id| {
                let p = panes.get(id).unwrap();
                if let Some(rows) = p.position_and_size().rows.as_percent() {
                    let current_fixed_rows = p.position_and_size().rows.as_usize();
                    let will_reduce_by =
                        ((self.display_area.rows as f64 / 100.0) * increase_by) as usize;
                    rows - increase_by >= RESIZE_PERCENT
                        && current_fixed_rows.saturating_sub(will_reduce_by) >= p.min_height()
                } else {
                    false
                }
            })
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_right(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        if let Some(cols) = pane.position_and_size().cols.as_percent() {
            let current_fixed_cols = pane.position_and_size().cols.as_usize();
            let will_reduce_by = ((self.display_area.cols as f64 / 100.0) * reduce_by) as usize;
            let ids_left = self.pane_ids_directly_left_of(pane_id);
            let flexible_left = self.ids_are_flexible(Direction::Horizontal, ids_left);
            cols - reduce_by >= RESIZE_PERCENT
                && flexible_left
                && current_fixed_cols.saturating_sub(will_reduce_by) >= pane.min_width()
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_left(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        if let Some(cols) = pane.position_and_size().cols.as_percent() {
            let current_fixed_cols = pane.position_and_size().cols.as_usize();
            let will_reduce_by = ((self.display_area.cols as f64 / 100.0) * reduce_by) as usize;
            let ids_right = self.pane_ids_directly_right_of(pane_id);
            let flexible_right = self.ids_are_flexible(Direction::Horizontal, ids_right);
            cols - reduce_by >= RESIZE_PERCENT
                && flexible_right
                && current_fixed_cols.saturating_sub(will_reduce_by) >= pane.min_width()
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_down(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        if let Some(rows) = pane.position_and_size().rows.as_percent() {
            let current_fixed_rows = pane.position_and_size().rows.as_usize();
            let will_reduce_by = ((self.display_area.rows as f64 / 100.0) * reduce_by) as usize;
            let ids_above = self.pane_ids_directly_above(pane_id);
            let flexible_above = self.ids_are_flexible(Direction::Vertical, ids_above);
            rows - reduce_by >= RESIZE_PERCENT
                && flexible_above
                && current_fixed_rows.saturating_sub(will_reduce_by) >= pane.min_height()
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_up(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        if let Some(rows) = pane.position_and_size().rows.as_percent() {
            let current_fixed_rows = pane.position_and_size().rows.as_usize();
            let will_reduce_by = ((self.display_area.rows as f64 / 100.0) * reduce_by) as usize;
            let ids_below = self.pane_ids_directly_below(pane_id);
            let flexible_below = self.ids_are_flexible(Direction::Vertical, ids_below);
            rows - reduce_by >= RESIZE_PERCENT
                && flexible_below
                && current_fixed_rows.saturating_sub(will_reduce_by) >= pane.min_height()
        } else {
            false
        }
    }
    fn reduce_pane_height(&mut self, id: &PaneId, percent: f64) {
        let mut panes = self.panes.borrow_mut();
        let terminal = panes.get_mut(id).unwrap();
        terminal.reduce_height(percent);
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
        let mut panes = self.panes.borrow_mut();
        let terminal = panes.get_mut(id).unwrap();
        terminal.reduce_width(percent);
    }
    fn increase_pane_and_surroundings_up(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_above = self
            .pane_ids_directly_above(id)
            .expect("can't increase pane size up if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height(id, percent);
        for terminal_id in terminals_above {
            self.reduce_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            self.increase_pane_height(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_down(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_below = self
            .pane_ids_directly_below(id)
            .expect("can't increase pane size down if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height(id, percent);
        for terminal_id in terminals_below {
            self.reduce_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            self.increase_pane_height(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_right(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_right = self
            .pane_ids_directly_right_of(id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| {
                return self.panes.borrow().get(t).unwrap().y();
            })
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width(id, percent);
        for terminal_id in terminals_to_the_right {
            self.reduce_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            self.increase_pane_width(terminal_id, percent);
        }
    }
    fn increase_pane_and_surroundings_left(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_left = self
            .pane_ids_directly_left_of(id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width(id, percent);
        for terminal_id in terminals_to_the_left {
            self.reduce_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            self.increase_pane_width(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_up(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_below = self
            .pane_ids_directly_below(id)
            .expect("can't reduce pane size up if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            let panes = self.panes.borrow();
            let pane = panes.get(terminal_id).unwrap();
            if pane.current_geom().rows.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_height(id, percent);
        for terminal_id in terminals_below {
            self.increase_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            self.reduce_pane_height(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_down(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_above = self
            .pane_ids_directly_above(id)
            .expect("can't reduce pane size down if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            let panes = self.panes.borrow();
            let pane = panes.get(terminal_id).unwrap();
            if pane.current_geom().rows.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_height(id, percent);
        for terminal_id in terminals_above {
            self.increase_pane_height(&terminal_id, percent);
        }
        for terminal_id in terminals_to_the_left.iter().chain(&terminals_to_the_right) {
            self.reduce_pane_height(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_right(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_left = self
            .pane_ids_directly_left_of(id)
            .expect("can't reduce pane size right if there are no terminals to the left");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            let panes = self.panes.borrow();
            let pane = panes.get(terminal_id).unwrap();
            if pane.current_geom().cols.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_width(id, percent);
        for terminal_id in terminals_to_the_left {
            self.increase_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            self.reduce_pane_width(terminal_id, percent);
        }
    }
    fn reduce_pane_and_surroundings_left(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_to_the_right = self
            .pane_ids_directly_right_of(id)
            .expect("can't reduce pane size left if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| self.panes.borrow().get(t).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });

        // FIXME: This checks that we aren't violating the resize constraints of the aligned panes
        // above and below this one. This should be moved to a `can_resize` function eventually.
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            let panes = self.panes.borrow();
            let pane = panes.get(terminal_id).unwrap();
            if pane.current_geom().cols.as_percent().unwrap() - percent < RESIZE_PERCENT {
                return;
            }
        }

        self.reduce_pane_width(id, percent);
        for terminal_id in terminals_to_the_right {
            self.increase_pane_width(&terminal_id, percent);
        }
        for terminal_id in terminals_above.iter().chain(&terminals_below) {
            self.reduce_pane_width(terminal_id, percent);
        }
    }
    fn pane_ids_directly_left_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        let mut ids = vec![];
        let terminal_to_check = panes.get(id).unwrap();
        if terminal_to_check.x() == 0 {
            return None;
        }
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_right_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_below(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_above(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
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
    fn ids_are_flexible(&self, direction: Direction, pane_ids: Option<Vec<PaneId>>) -> bool {
        let panes = self.panes.borrow();
        pane_ids.is_some()
            && pane_ids.unwrap().iter().all(|id| {
                let pane_to_check = panes.get(id).unwrap();
                let geom = pane_to_check.current_geom();
                let dimension = match direction {
                    Direction::Vertical => geom.rows,
                    Direction::Horizontal => geom.cols,
                };
                !dimension.is_fixed()
            })
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
    fn try_increase_pane_and_surroundings_right(
        &mut self,
        pane_id: &PaneId,
        reduce_by: f64,
    ) -> bool {
        if self.can_increase_pane_and_surroundings_right(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_right(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
            return true;
        }
        false
    }
    fn try_increase_pane_and_surroundings_left(
        &mut self,
        pane_id: &PaneId,
        reduce_by: f64,
    ) -> bool {
        if self.can_increase_pane_and_surroundings_left(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_left(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
            return true;
        }
        false
    }
    fn try_increase_pane_and_surroundings_up(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_increase_pane_and_surroundings_up(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_up(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
            return true;
        }
        false
    }
    fn try_increase_pane_and_surroundings_down(
        &mut self,
        pane_id: &PaneId,
        reduce_by: f64,
    ) -> bool {
        if self.can_increase_pane_and_surroundings_down(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_down(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
            return true;
        }
        false
    }
    fn try_increase_pane_and_surroundings_right_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_right =
            self.can_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
        let can_increase_pane_up =
            self.can_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
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
            self.try_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            self.try_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
            {
                self.try_reduce_pane_and_surroundings_right(
                    &pane_above_with_right_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_left_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_left =
            self.can_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
        let can_increase_pane_up =
            self.can_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
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
            self.try_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            self.try_increase_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
                self.try_reduce_pane_and_surroundings_left(
                    &pane_above_with_left_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_right_and_down(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_right =
            self.can_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
        let can_increase_pane_down =
            self.can_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
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
            self.try_increase_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            self.try_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
            {
                self.try_reduce_pane_and_surroundings_right(
                    &pane_below_with_right_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_left_and_down(&mut self, pane_id: &PaneId) -> bool {
        let can_increase_pane_left =
            self.can_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
        let can_increase_pane_down =
            self.can_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
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
            self.try_increase_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            self.try_increase_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
                self.try_reduce_pane_and_surroundings_left(
                    &pane_below_with_left_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_right_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_reduce_pane_right =
            self.can_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
        let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
        if can_reduce_pane_right && can_reduce_pane_up {
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
            self.try_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            self.try_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
                self.try_increase_pane_and_surroundings_right(
                    &pane_below_with_left_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_left_and_up(&mut self, pane_id: &PaneId) -> bool {
        let can_reduce_pane_left =
            self.can_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
        let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
        if can_reduce_pane_left && can_reduce_pane_up {
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
            self.try_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            self.try_reduce_pane_and_surroundings_up(pane_id, RESIZE_PERCENT);
            if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
            {
                self.try_increase_pane_and_surroundings_left(
                    &pane_below_with_right_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_right_and_down(&mut self, pane_id: &PaneId) -> bool {
        let can_reduce_pane_right =
            self.can_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
        let can_reduce_pane_down =
            self.can_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
        if can_reduce_pane_right && can_reduce_pane_down {
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
            self.try_reduce_pane_and_surroundings_right(pane_id, RESIZE_PERCENT);
            self.try_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
                self.try_increase_pane_and_surroundings_right(
                    &pane_above_with_left_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_left_and_down(&mut self, pane_id: &PaneId) -> bool {
        let can_reduce_pane_left =
            self.can_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
        let can_reduce_pane_down =
            self.can_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
        if can_reduce_pane_left && can_reduce_pane_down {
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
            self.try_reduce_pane_and_surroundings_left(pane_id, RESIZE_PERCENT);
            self.try_reduce_pane_and_surroundings_down(pane_id, RESIZE_PERCENT);
            if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
            {
                self.try_increase_pane_and_surroundings_left(
                    &pane_above_with_right_aligned_border,
                    RESIZE_PERCENT,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_right(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_right(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_right(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_left(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_left(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_left(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Horizontal, self.display_area.cols);
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_up(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_up(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_up(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_down(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_down(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_down(pane_id, reduce_by);
            let mut pane_resizer = PaneResizer::new(self.panes.clone());
            let _ = pane_resizer.layout(Direction::Vertical, self.display_area.rows);
            return true;
        }
        false
    }
    fn viewport_pane_ids_directly_above(&self, pane_id: &PaneId) -> Vec<PaneId> {
        self.pane_ids_directly_above(pane_id)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| self.is_inside_viewport(id))
            .collect()
    }
    fn viewport_pane_ids_directly_below(&self, pane_id: &PaneId) -> Vec<PaneId> {
        self.pane_ids_directly_below(pane_id)
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

            if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(&id) {
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
        }
        None
    }
    fn panes_to_the_right_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let upper_close_border = pane.y();
            let lower_close_border = pane.y() + pane.rows();

            if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(&id) {
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
        }
        None
    }
    fn panes_above_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            if let Some(panes_above) = self.pane_ids_directly_above(&id) {
                let mut selectable_panes: Vec<_> = panes_above
                    .into_iter()
                    .filter(|pid| panes.get(pid).unwrap().selectable())
                    .collect();
                let pane_borders_above = self.vertical_borders(&selectable_panes);
                if pane_borders_above.contains(&left_close_border)
                    && pane_borders_above.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn panes_below_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            if let Some(panes_below) = self.pane_ids_directly_below(&id) {
                let mut selectable_panes: Vec<_> = panes_below
                    .into_iter()
                    .filter(|pid| panes[pid].selectable())
                    .collect();
                let pane_borders_below = self.vertical_borders(&selectable_panes);
                if pane_borders_below.contains(&left_close_border)
                    && pane_borders_below.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn find_panes_to_grow(&self, id: PaneId) -> Option<(Vec<PaneId>, Direction)> {
        if let Some(panes) = self
            .panes_to_the_left_between_aligning_borders(id)
            .or_else(|| self.panes_to_the_right_between_aligning_borders(id))
        {
            return Some((panes, Direction::Horizontal));
        }

        if let Some(panes) = self
            .panes_above_between_aligning_borders(id)
            .or_else(|| self.panes_below_between_aligning_borders(id))
        {
            return Some((panes, Direction::Vertical));
        }

        None
    }
    fn grow_panes(&mut self, panes: &[PaneId], direction: Direction, (width, height): (f64, f64)) {
        match direction {
            Direction::Horizontal => {
                for pane_id in panes {
                    self.increase_pane_width(pane_id, width);
                }
            }
            Direction::Vertical => {
                for pane_id in panes {
                    self.increase_pane_height(pane_id, height);
                }
            }
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
                    Direction::Vertical => self.display_area.rows,
                    Direction::Horizontal => self.display_area.cols,
                };
                {
                    let mut panes = self.panes.borrow_mut();
                    (*panes).remove(&id);
                }
                let mut pane_resizer = PaneResizer::new(self.panes.clone());
                let _ = pane_resizer.layout(direction, side_length);
                return true;
            }
        }
        false
    }
    pub fn find_room_for_new_pane(&self) -> Option<(PaneId, Direction)> {
        let panes = self.panes.borrow();
        let pane_sequence: Vec<(&PaneId, &&mut Box<dyn Pane>)> =
            panes.iter().filter(|(_, p)| p.selectable()).collect();
        let (_largest_pane_size, pane_id_to_split) = pane_sequence.iter().fold(
            (0, None),
            |(current_largest_pane_size, current_pane_id_to_split), id_and_pane_to_check| {
                let (id_of_pane_to_check, pane_to_check) = id_and_pane_to_check;
                let pane_size =
                    (pane_to_check.rows() * CURSOR_HEIGHT_WIDTH_RATIO) * pane_to_check.cols();
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
            let direction = if pane_to_split.rows() * CURSOR_HEIGHT_WIDTH_RATIO
                > pane_to_split.cols()
                && pane_to_split.rows() > pane_to_split.min_height() * 2
            {
                Some(Direction::Horizontal)
            } else if pane_to_split.cols() > pane_to_split.min_width() * 2 {
                Some(Direction::Vertical)
            } else {
                None
            };

            direction.map(|direction| (*t_id_to_split, direction))
        })
    }
}

pub fn split(direction: Direction, rect: &PaneGeom) -> Option<(PaneGeom, PaneGeom)> {
    let space = match direction {
        Direction::Vertical => rect.cols,
        Direction::Horizontal => rect.rows,
    };
    if let Some(p) = space.as_percent() {
        let first_rect = match direction {
            Direction::Vertical => PaneGeom {
                cols: Dimension::percent(p / 2.0),
                ..*rect
            },
            Direction::Horizontal => PaneGeom {
                rows: Dimension::percent(p / 2.0),
                ..*rect
            },
        };
        let second_rect = match direction {
            Direction::Vertical => PaneGeom {
                x: first_rect.x + 1,
                cols: first_rect.cols,
                ..*rect
            },
            Direction::Horizontal => PaneGeom {
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

pub struct FloatingPaneGrid<'a> {
    // panes: HashMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
    desired_pane_positions: Rc<RefCell<&'a mut HashMap<PaneId, PaneGeom>>>,
    display_area: Size, // includes all panes (including eg. the status bar and tab bar in the default layout)
    viewport: Viewport, // includes all non-UI panes
}

impl<'a> FloatingPaneGrid<'a> {
    pub fn new(
        panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        desired_pane_positions: &'a mut HashMap<PaneId, PaneGeom>,
        display_area: Size,
        viewport: Viewport,
    ) -> Self {
        let panes: HashMap<_, _> = panes.into_iter().map(|(p_id, p)| (*p_id, p)).collect();
        FloatingPaneGrid {
            panes: Rc::new(RefCell::new(panes)),
            desired_pane_positions: Rc::new(RefCell::new(desired_pane_positions)),
            display_area,
            viewport,
        }
    }

    pub fn layout(&mut self, direction: Direction, space: usize) -> Result<(), String> {
        let mut pane_resizer = PaneResizer::new(self.panes.clone());
        pane_resizer.layout(direction, space)
    }
    pub fn move_pane_by(&mut self, pane_id: PaneId, x: isize, y: isize) {
        // true => succeeded to move, false => failed to move
        let new_pane_position = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes
                .iter_mut()
                .find(|(p_id, _p)| **p_id == pane_id)
                .unwrap()
                .1;
            let mut new_pane_position = pane.position_and_size();
            let min_x = self.viewport.x as isize;
            let min_y = self.viewport.y as isize;
            let max_x = (self.viewport.cols + self.viewport.x)
                .saturating_sub(new_pane_position.cols.as_usize());
            let max_y = (self.viewport.rows + self.viewport.y)
                .saturating_sub(new_pane_position.rows.as_usize());
            let new_x = std::cmp::max(min_x, new_pane_position.x as isize + x);
            let new_x = std::cmp::min(new_x, max_x as isize);
            let new_y = std::cmp::max(min_y, new_pane_position.y as isize + y);
            let new_y = std::cmp::min(new_y, max_y as isize);
            new_pane_position.x = new_x as usize;
            new_pane_position.y = new_y as usize;
            new_pane_position
        };
        self.set_pane_geom(pane_id, new_pane_position);
    }
    fn set_pane_geom(&mut self, pane_id: PaneId, new_pane_geom: PaneGeom) {
        let mut panes = self.panes.borrow_mut();
        let pane = panes
            .iter_mut()
            .find(|(p_id, _p)| **p_id == pane_id)
            .unwrap()
            .1;
        pane.set_geom(new_pane_geom);
        let mut desired_pane_positions = self.desired_pane_positions.borrow_mut();
        desired_pane_positions.insert(pane_id, new_pane_geom);
    }
    pub fn resize(&mut self, space: Size) {
        let mut panes = self.panes.borrow_mut();
        let desired_pane_positions = self.desired_pane_positions.borrow();

        // account for the difference between the viewport (including non-ui pane items which we
        // do not want to override) and the display_area, which is the area we can go over
        let display_size_row_difference = self.display_area.rows.saturating_sub(self.viewport.rows);
        let display_size_column_difference = self.display_area.cols.saturating_sub(self.viewport.cols);

        let mut new_viewport = self.viewport;
        new_viewport.cols = space.cols.saturating_sub(display_size_column_difference);
        new_viewport.rows = space.rows.saturating_sub(display_size_row_difference);

        for (pane_id, pane) in panes.iter_mut() {
            let mut new_pane_geom = pane.current_geom();
            let desired_pane_geom = desired_pane_positions.get(pane_id).unwrap();
            let desired_pane_geom_is_inside_viewport = pane_geom_is_inside_viewport(&new_viewport, &desired_pane_geom);
            let pane_is_in_desired_position = new_pane_geom == *desired_pane_geom;
            if pane_is_in_desired_position && desired_pane_geom_is_inside_viewport {
                continue;
            } else if desired_pane_geom_is_inside_viewport {
                pane.set_geom(*desired_pane_geom);
            } else {
                let pane_right_side = new_pane_geom.x + new_pane_geom.cols.as_usize();
                let pane_bottom_side = new_pane_geom.y + new_pane_geom.rows.as_usize();
                let viewport_right_side = new_viewport.x + new_viewport.cols;
                let viewport_bottom_side = new_viewport.y + new_viewport.rows;
                let excess_width = pane_right_side.saturating_sub(viewport_right_side);
                let excess_height = pane_bottom_side.saturating_sub(viewport_bottom_side);
                let extra_width = viewport_right_side.saturating_sub(pane_right_side);
                let extra_height = viewport_bottom_side.saturating_sub(pane_bottom_side);

                // handle shrink width
                if excess_width > 0 && new_pane_geom.x.saturating_sub(excess_width) > new_viewport.x {
                    new_pane_geom.x = new_pane_geom.x.saturating_sub(excess_width);
                } else if excess_width > 0 && new_pane_geom.cols.as_usize().saturating_sub(excess_width) > MIN_TERMINAL_WIDTH {
                    new_pane_geom.cols.set_inner(new_pane_geom.cols.as_usize().saturating_sub(excess_width));
                } else if excess_width > 0 {
                    let reduce_x_by = new_pane_geom.x.saturating_sub(new_viewport.x);
                    let reduced_width = new_pane_geom.cols
                        .as_usize()
                        .saturating_sub(excess_width.saturating_sub(reduce_x_by));
                    new_pane_geom.x = new_viewport.x;
                    new_pane_geom.cols.set_inner(std::cmp::max(
                        reduced_width,
                        MIN_TERMINAL_WIDTH,
                    ));
                }

                // handle shrink height
                if excess_height > 0 && new_pane_geom.y.saturating_sub(excess_height) > new_viewport.y {
                    new_pane_geom.y = new_pane_geom.y.saturating_sub(excess_height);
                } else if excess_height > 0 && new_pane_geom.rows.as_usize().saturating_sub(excess_height) > MIN_TERMINAL_HEIGHT {
                    new_pane_geom.rows.set_inner(new_pane_geom.rows.as_usize().saturating_sub(excess_height));
                } else if excess_height > 0 {
                    let reduce_y_by = new_pane_geom.y.saturating_sub(new_viewport.y);
                    let reduced_height = new_pane_geom.rows
                        .as_usize()
                        .saturating_sub(excess_height.saturating_sub(reduce_y_by));
                    new_pane_geom.y = new_viewport.y;
                    new_pane_geom.rows.set_inner(std::cmp::max(
                        reduced_height,
                        MIN_TERMINAL_HEIGHT,
                    ));
                }

                // handle expand width
                if extra_width > 0  {
                    let max_right_coords = new_viewport.x + new_viewport.cols;
                    if new_pane_geom.x < desired_pane_geom.x {
                        if desired_pane_geom.x + new_pane_geom.cols.as_usize() <= max_right_coords {
                            new_pane_geom.x = desired_pane_geom.x
                        } else if new_pane_geom.x + new_pane_geom.cols.as_usize() + extra_width < max_right_coords {
                            new_pane_geom.x = new_pane_geom.x + extra_width;
                        } else {
                            new_pane_geom.x = max_right_coords.saturating_sub(new_pane_geom.cols.as_usize());
                        }
                    }
                    if new_pane_geom.cols.as_usize() < desired_pane_geom.cols.as_usize() {
                        if new_pane_geom.x + desired_pane_geom.cols.as_usize() <= max_right_coords {
                            new_pane_geom.cols.set_inner(desired_pane_geom.cols.as_usize());
                        } else if new_pane_geom.x + new_pane_geom.cols.as_usize() + extra_width < max_right_coords {
                            new_pane_geom.cols.set_inner(new_pane_geom.cols.as_usize() + extra_width);
                        } else {
                            new_pane_geom.cols.set_inner(new_pane_geom.cols.as_usize() + (max_right_coords - (new_pane_geom.x + new_pane_geom.cols.as_usize())));
                        }
                    }
                }

                // handle expand height
                if extra_height > 0  {
                    let max_bottom_coords = new_viewport.y + new_viewport.rows;
                    if new_pane_geom.y < desired_pane_geom.y {
                        if desired_pane_geom.y + new_pane_geom.rows.as_usize() <= max_bottom_coords {
                            new_pane_geom.y = desired_pane_geom.y
                        } else if new_pane_geom.y + new_pane_geom.rows.as_usize() + extra_height < max_bottom_coords {
                            new_pane_geom.y = new_pane_geom.y + extra_height;
                        } else {
                            new_pane_geom.y = max_bottom_coords.saturating_sub(new_pane_geom.rows.as_usize());
                        }
                    }
                    if new_pane_geom.rows.as_usize() < desired_pane_geom.rows.as_usize() {
                        if new_pane_geom.y + desired_pane_geom.rows.as_usize() <= max_bottom_coords {
                            new_pane_geom.rows.set_inner(desired_pane_geom.rows.as_usize());
                        } else if new_pane_geom.y + new_pane_geom.rows.as_usize() + extra_height < max_bottom_coords {
                            new_pane_geom.rows.set_inner(new_pane_geom.rows.as_usize() + extra_height);
                        } else {
                            new_pane_geom.rows.set_inner(new_pane_geom.rows.as_usize() + (max_bottom_coords - (new_pane_geom.y + new_pane_geom.rows.as_usize())));
                        }
                    }
                }
                pane.set_geom(new_pane_geom);
            }
        }
    }
    pub fn move_pane_left(&mut self, pane_id: &PaneId) {
        if let Some(move_by) = self.can_move_pane_left(pane_id, MOVE_INCREMENT_HORIZONTAL) {
            self.move_pane_position_left(pane_id, move_by);
        }
    }
    pub fn move_pane_right(&mut self, pane_id: &PaneId) {
        if let Some(move_by) = self.can_move_pane_right(pane_id, MOVE_INCREMENT_HORIZONTAL) {
            self.move_pane_position_right(pane_id, move_by);
        }
    }
    pub fn move_pane_down(&mut self, pane_id: &PaneId) {
        if let Some(move_by) = self.can_move_pane_down(pane_id, MOVE_INCREMENT_VERTICAL) {
            self.move_pane_position_down(pane_id, move_by);
        }
    }
    pub fn move_pane_up(&mut self, pane_id: &PaneId) {
        if let Some(move_by) = self.can_move_pane_up(pane_id, MOVE_INCREMENT_VERTICAL) {
            self.move_pane_position_up(pane_id, move_by);
        }
    }
    fn can_move_pane_left(&self, pane_id: &PaneId, move_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_until_left_screen_edge = pane.x().saturating_sub(self.viewport.x);
        if space_until_left_screen_edge >= move_by {
            Some(move_by)
        } else if space_until_left_screen_edge > 0 {
            Some(space_until_left_screen_edge)
        } else {
            None
        }
    }
    fn can_move_pane_right(&self, pane_id: &PaneId, move_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_until_right_screen_edge =
            (self.viewport.x + self.viewport.cols).saturating_sub(pane.x() + pane.cols());
        if space_until_right_screen_edge >= move_by {
            Some(move_by)
        } else if space_until_right_screen_edge > 0 {
            Some(space_until_right_screen_edge)
        } else {
            None
        }
    }
    fn can_move_pane_up(&self, pane_id: &PaneId, move_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_until_top_screen_edge = pane.y().saturating_sub(self.viewport.y);
        if space_until_top_screen_edge >= move_by {
            Some(move_by)
        } else if space_until_top_screen_edge > 0 {
            Some(space_until_top_screen_edge)
        } else {
            None
        }
    }
    fn can_move_pane_down(&self, pane_id: &PaneId, move_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_until_bottom_screen_edge =
            (self.viewport.y + self.viewport.rows).saturating_sub(pane.y() + pane.rows());
        if space_until_bottom_screen_edge >= move_by {
            Some(move_by)
        } else if space_until_bottom_screen_edge > 0 {
            Some(space_until_bottom_screen_edge)
        } else {
            None
        }
    }
    fn move_pane_position_left(&mut self, pane_id: &PaneId, move_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(pane_id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.x -= move_by;
            current_geom
        };
        self.set_pane_geom(*pane_id, new_pane_geom);
    }
    fn move_pane_position_right(&mut self, pane_id: &PaneId, move_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(pane_id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.x += move_by;
            current_geom
        };
        self.set_pane_geom(*pane_id, new_pane_geom);
    }
    fn move_pane_position_down(&mut self, pane_id: &PaneId, move_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(pane_id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.y += move_by;
            current_geom
        };
        self.set_pane_geom(*pane_id, new_pane_geom);
    }
    fn move_pane_position_up(&mut self, pane_id: &PaneId, move_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(pane_id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.y -= move_by;
            current_geom
        };
        self.set_pane_geom(*pane_id, new_pane_geom);
    }
    pub fn resize_pane_left(&'a mut self, pane_id: &PaneId) {
        if let Some(increase_by) = self.can_increase_pane_size_left(pane_id, RESIZE_INCREMENT_WIDTH)
        {
            self.increase_pane_size_left(pane_id, increase_by);
        } else if let Some(decrease_by) =
            self.can_decrease_pane_size_left(pane_id, RESIZE_INCREMENT_WIDTH)
        {
            self.decrease_pane_size_left(pane_id, decrease_by);
        }
    }
    pub fn resize_pane_right(&mut self, pane_id: &PaneId) {
        if let Some(increase_by) =
            self.can_increase_pane_size_right(pane_id, RESIZE_INCREMENT_WIDTH)
        {
            self.increase_pane_size_right(pane_id, increase_by);
        } else if let Some(decrease_by) =
            self.can_decrease_pane_size_right(pane_id, RESIZE_INCREMENT_WIDTH)
        {
            self.decrease_pane_size_right(pane_id, decrease_by);
        }
    }
    pub fn resize_pane_down(&mut self, pane_id: &PaneId) {
        if let Some(increase_by) =
            self.can_increase_pane_size_down(pane_id, RESIZE_INCREMENT_HEIGHT)
        {
            self.increase_pane_size_down(pane_id, increase_by);
        } else if let Some(decrease_by) =
            self.can_decrease_pane_size_down(pane_id, RESIZE_INCREMENT_HEIGHT)
        {
            self.decrease_pane_size_down(pane_id, decrease_by);
        }
    }
    pub fn resize_pane_up(&mut self, pane_id: &PaneId) {
        if let Some(increase_by) = self.can_increase_pane_size_up(pane_id, RESIZE_INCREMENT_HEIGHT)
        {
            self.increase_pane_size_up(pane_id, increase_by);
        } else if let Some(decrease_by) =
            self.can_decrease_pane_size_up(pane_id, RESIZE_INCREMENT_HEIGHT)
        {
            self.decrease_pane_size_up(pane_id, decrease_by);
        }
    }
    pub fn resize_increase(&mut self, pane_id: &PaneId) {
        if let Some(increase_by) =
            self.can_increase_pane_size_left(pane_id, RESIZE_INCREMENT_WIDTH / 2)
        {
            self.increase_pane_size_left(pane_id, increase_by);
        }
        if let Some(increase_by) =
            self.can_increase_pane_size_right(pane_id, RESIZE_INCREMENT_WIDTH / 2)
        {
            self.increase_pane_size_right(pane_id, increase_by);
        }
        if let Some(increase_by) =
            self.can_increase_pane_size_down(pane_id, RESIZE_INCREMENT_HEIGHT / 2)
        {
            self.increase_pane_size_down(pane_id, increase_by);
        }
        if let Some(increase_by) =
            self.can_increase_pane_size_up(pane_id, RESIZE_INCREMENT_HEIGHT / 2)
        {
            self.increase_pane_size_up(pane_id, increase_by);
        }
    }
    pub fn resize_decrease(&mut self, pane_id: &PaneId) {
        if let Some(decrease_by) =
            self.can_decrease_pane_size_left(pane_id, RESIZE_INCREMENT_WIDTH / 2)
        {
            self.decrease_pane_size_left(pane_id, decrease_by);
        }
        if let Some(decrease_by) =
            self.can_decrease_pane_size_right(pane_id, RESIZE_INCREMENT_WIDTH / 2)
        {
            self.decrease_pane_size_right(pane_id, decrease_by);
        }
        if let Some(decrease_by) =
            self.can_decrease_pane_size_down(pane_id, RESIZE_INCREMENT_HEIGHT / 2)
        {
            self.decrease_pane_size_down(pane_id, decrease_by);
        }
        if let Some(decrease_by) =
            self.can_decrease_pane_size_up(pane_id, RESIZE_INCREMENT_HEIGHT / 2)
        {
            self.decrease_pane_size_up(pane_id, decrease_by);
        }
    }
    fn can_increase_pane_size_left(
        &self,
        pane_id: &PaneId,
        max_increase_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let distance_to_left_edge = pane.x().saturating_sub(self.viewport.x);
        if distance_to_left_edge.saturating_sub(max_increase_by) > 0 {
            Some(max_increase_by)
        } else if distance_to_left_edge > 0 {
            Some(distance_to_left_edge)
        } else {
            None
        }
    }
    fn can_decrease_pane_size_left(
        &self,
        pane_id: &PaneId,
        max_decrease_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_left_to_decrease = pane.cols().saturating_sub(MIN_TERMINAL_WIDTH);
        if space_left_to_decrease.saturating_sub(max_decrease_by) > 0 {
            Some(max_decrease_by)
        } else if space_left_to_decrease > 0 {
            Some(space_left_to_decrease)
        } else {
            None
        }
    }
    fn can_increase_pane_size_right(
        &self,
        pane_id: &PaneId,
        max_increase_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let distance_to_right_edge =
            (self.viewport.x + self.viewport.cols).saturating_sub(pane.x() + pane.cols());
        if pane.x() + pane.cols() + max_increase_by < self.viewport.cols {
            Some(max_increase_by)
        } else if distance_to_right_edge > 0 {
            Some(distance_to_right_edge)
        } else {
            None
        }
    }
    fn can_decrease_pane_size_right(
        &self,
        pane_id: &PaneId,
        max_decrease_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_left_to_decrease = pane.cols().saturating_sub(MIN_TERMINAL_WIDTH);
        let pane_right_edge = pane.x() + pane.cols();
        if space_left_to_decrease.saturating_sub(max_decrease_by) > 0
            && pane.x() + max_decrease_by <= pane_right_edge + MIN_TERMINAL_WIDTH
        {
            Some(max_decrease_by)
        } else if space_left_to_decrease > 0
            && pane.x() + max_decrease_by <= pane_right_edge + MIN_TERMINAL_WIDTH
        {
            Some(space_left_to_decrease)
        } else {
            None
        }
    }
    fn can_increase_pane_size_down(
        &self,
        pane_id: &PaneId,
        max_increase_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let distance_to_bottom_edge =
            (self.viewport.y + self.viewport.rows).saturating_sub(pane.y() + pane.rows());
        if pane.y() + pane.rows() + max_increase_by < self.viewport.rows {
            Some(max_increase_by)
        } else if distance_to_bottom_edge > 0 {
            Some(distance_to_bottom_edge)
        } else {
            None
        }
    }
    fn can_decrease_pane_size_down(
        &self,
        pane_id: &PaneId,
        max_decrease_by: usize,
    ) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_left_to_decrease = pane.rows().saturating_sub(MIN_TERMINAL_HEIGHT);
        let pane_bottom_edge = pane.y() + pane.rows();
        if space_left_to_decrease.saturating_sub(max_decrease_by) > 0
            && pane.y() + max_decrease_by <= pane_bottom_edge + MIN_TERMINAL_HEIGHT
        {
            Some(max_decrease_by)
        } else if space_left_to_decrease > 0
            && pane.y() + max_decrease_by <= pane_bottom_edge + MIN_TERMINAL_HEIGHT
        {
            Some(space_left_to_decrease)
        } else {
            None
        }
    }
    fn can_increase_pane_size_up(&self, pane_id: &PaneId, max_increase_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let distance_to_top_edge = pane.y().saturating_sub(self.viewport.y);
        if distance_to_top_edge.saturating_sub(max_increase_by) > 0 {
            Some(max_increase_by)
        } else if distance_to_top_edge > 0 {
            Some(distance_to_top_edge)
        } else {
            None
        }
    }
    fn can_decrease_pane_size_up(&self, pane_id: &PaneId, max_decrease_by: usize) -> Option<usize> {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let space_left_to_decrease = pane.rows().saturating_sub(MIN_TERMINAL_HEIGHT);
        if space_left_to_decrease.saturating_sub(max_decrease_by) > 0 {
            Some(max_decrease_by)
        } else if space_left_to_decrease > 0 {
            Some(space_left_to_decrease)
        } else {
            None
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
    fn increase_pane_size_left(&mut self, id: &PaneId, increase_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
                let pane = panes.get_mut(id).unwrap();
                let mut current_geom = pane.position_and_size();
                current_geom.x -= increase_by;
                current_geom
                    .cols
                    .set_inner(current_geom.cols.as_usize() + increase_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn decrease_pane_size_left(&mut self, id: &PaneId, decrease_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom
                .cols
                .set_inner(current_geom.cols.as_usize() - decrease_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn increase_pane_size_right(&mut self, id: &PaneId, increase_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom
                .cols
                .set_inner(current_geom.cols.as_usize() + increase_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn decrease_pane_size_right(&mut self, id: &PaneId, decrease_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.x += decrease_by;
            current_geom
                .cols
                .set_inner(current_geom.cols.as_usize() - decrease_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn increase_pane_size_down(&mut self, id: &PaneId, increase_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom
                .rows
                .set_inner(current_geom.rows.as_usize() + increase_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn decrease_pane_size_down(&mut self, id: &PaneId, decrease_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.y += decrease_by;
            current_geom
                .rows
                .set_inner(current_geom.rows.as_usize() - decrease_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn increase_pane_size_up(&mut self, id: &PaneId, increase_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom.y -= increase_by;
            current_geom
                .rows
                .set_inner(current_geom.rows.as_usize() + increase_by);
            pane.set_geom(current_geom);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn decrease_pane_size_up(&mut self, id: &PaneId, decrease_by: usize) {
        let new_pane_geom = {
            let mut panes = self.panes.borrow_mut();
            let pane = panes.get_mut(id).unwrap();
            let mut current_geom = pane.position_and_size();
            current_geom
                .rows
                .set_inner(current_geom.rows.as_usize() - decrease_by);
            current_geom
        };
        self.set_pane_geom(*id, new_pane_geom);
    }
    fn pane_ids_directly_left_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        let mut ids = vec![];
        let terminal_to_check = panes.get(id).unwrap();
        if terminal_to_check.x() == 0 {
            return None;
        }
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_right_of(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_below(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn pane_ids_directly_above(&self, id: &PaneId) -> Option<Vec<PaneId>> {
        let mut ids = vec![];
        let panes = self.panes.borrow();
        let terminal_to_check = panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in panes.iter() {
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
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
                c.is_left_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by(|(_, (_, a)), (_, (_, b))| {
                let x_comparison = a.x().cmp(&b.x());
                match x_comparison {
                    Ordering::Equal => a.y().cmp(&b.y()),
                    _ => x_comparison,
                }
            })
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
                c.is_below(Box::as_ref(current_pane))
                    && c.vertically_overlaps_with(Box::as_ref(current_pane))
            })
            .min_by(|(_, (_, a)), (_, (_, b))| {
                let y_comparison = a.y().cmp(&b.y());
                match y_comparison {
                    Ordering::Equal => b.x().cmp(&a.x()),
                    _ => y_comparison,
                }
            })
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
                c.is_above(Box::as_ref(current_pane))
                    && c.vertically_overlaps_with(Box::as_ref(current_pane))
            })
            .max_by(|(_, (_, a)), (_, (_, b))| {
                let y_comparison = a.y().cmp(&b.y());
                match y_comparison {
                    Ordering::Equal => b.x().cmp(&a.x()),
                    _ => y_comparison,
                }
            })
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
                c.is_right_of(Box::as_ref(current_pane))
                    && c.horizontally_overlaps_with(Box::as_ref(current_pane))
            })
            .min_by(|(_, (_, a)), (_, (_, b))| {
                let x_comparison = a.x().cmp(&b.x());
                match x_comparison {
                    Ordering::Equal => a.y().cmp(&b.y()),
                    _ => x_comparison,
                }
            })
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

            if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(&id) {
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
        }
        None
    }
    fn panes_to_the_right_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let upper_close_border = pane.y();
            let lower_close_border = pane.y() + pane.rows();

            if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(&id) {
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
        }
        None
    }
    fn panes_above_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            if let Some(panes_above) = self.pane_ids_directly_above(&id) {
                let mut selectable_panes: Vec<_> = panes_above
                    .into_iter()
                    .filter(|pid| panes.get(pid).unwrap().selectable())
                    .collect();
                let pane_borders_above = self.vertical_borders(&selectable_panes);
                if pane_borders_above.contains(&left_close_border)
                    && pane_borders_above.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn panes_below_between_aligning_borders(&self, id: PaneId) -> Option<Vec<PaneId>> {
        let panes = self.panes.borrow();
        if let Some(pane) = panes.get(&id) {
            let left_close_border = pane.x();
            let right_close_border = pane.x() + pane.cols();

            if let Some(panes_below) = self.pane_ids_directly_below(&id) {
                let mut selectable_panes: Vec<_> = panes_below
                    .into_iter()
                    .filter(|pid| panes[pid].selectable())
                    .collect();
                let pane_borders_below = self.vertical_borders(&selectable_panes);
                if pane_borders_below.contains(&left_close_border)
                    && pane_borders_below.contains(&right_close_border)
                {
                    selectable_panes.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(selectable_panes);
                }
            }
        }
        None
    }
    fn find_panes_to_grow(&self, id: PaneId) -> Option<(Vec<PaneId>, Direction)> {
        if let Some(panes) = self
            .panes_to_the_left_between_aligning_borders(id)
            .or_else(|| self.panes_to_the_right_between_aligning_borders(id))
        {
            return Some((panes, Direction::Horizontal));
        }

        if let Some(panes) = self
            .panes_above_between_aligning_borders(id)
            .or_else(|| self.panes_below_between_aligning_borders(id))
        {
            return Some((panes, Direction::Vertical));
        }

        None
    }
    fn grow_panes(&mut self, panes: &[PaneId], direction: Direction, (width, height): (f64, f64)) {
        match direction {
            Direction::Horizontal => {
                for pane_id in panes {
                    self.increase_pane_width(pane_id, width);
                }
            }
            Direction::Vertical => {
                for pane_id in panes {
                    self.increase_pane_height(pane_id, height);
                }
            }
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
                    Direction::Vertical => self.display_area.rows,
                    Direction::Horizontal => self.display_area.cols,
                };
                {
                    let mut panes = self.panes.borrow_mut();
                    (*panes).remove(&id);
                }
                let mut pane_resizer = PaneResizer::new(self.panes.clone());
                let _ = pane_resizer.layout(direction, side_length);
                return true;
            }
        }
        false
    }
    pub fn find_room_for_new_pane(&self) -> Option<PaneGeom> {
        let panes = self.panes.borrow();
        let pane_geoms: Vec<PaneGeom> = panes.values().map(|p| p.position_and_size()).collect();
        for offset in 0..MAX_PANES / 5 {
            let half_size_middle_geom = half_size_middle_geom(&self.viewport, offset);
            let half_size_top_left_geom = half_size_top_left_geom(&self.viewport, offset);
            let half_size_top_right_geom = half_size_top_right_geom(&self.viewport, offset);
            let half_size_bottom_left_geom = half_size_bottom_left_geom(&self.viewport, offset);
            let half_size_bottom_right_geom = half_size_bottom_right_geom(&self.viewport, offset);
            if pane_geom_is_big_enough(&half_size_middle_geom) && pane_geom_is_unoccupied_and_inside_viewport(&self.viewport, &half_size_middle_geom, &pane_geoms) {
                return Some(half_size_middle_geom);
            } else if pane_geom_is_big_enough(&half_size_top_left_geom) && pane_geom_is_unoccupied_and_inside_viewport(&self.viewport, &half_size_top_left_geom, &pane_geoms) {
                return Some(half_size_top_left_geom);
            } else if pane_geom_is_big_enough(&half_size_top_right_geom) && pane_geom_is_unoccupied_and_inside_viewport(&self.viewport, &half_size_top_right_geom, &pane_geoms) {
                return Some(half_size_top_right_geom);
            } else if pane_geom_is_big_enough(&half_size_bottom_left_geom) && pane_geom_is_unoccupied_and_inside_viewport(&self.viewport, &half_size_bottom_left_geom, &pane_geoms) {
                return Some(half_size_bottom_left_geom);
            } else if pane_geom_is_big_enough(&half_size_bottom_right_geom) && pane_geom_is_unoccupied_and_inside_viewport(&self.viewport, &half_size_bottom_right_geom, &pane_geoms) {
                return Some(half_size_bottom_right_geom);
            }
        }
        None
    }
}
fn half_size_middle_geom(space: &Viewport, offset: usize) -> PaneGeom {
    let mut geom = PaneGeom {
        x: space.x + (space.cols as f64 / 4.0).round() as usize + offset,
        y: space.y + (space.rows as f64 / 4.0).round() as usize + offset,
        cols: Dimension::fixed(space.cols / 2),
        rows: Dimension::fixed(space.rows / 2),
    };
    geom.cols.set_inner(space.cols / 2);
    geom.rows.set_inner(space.rows / 2);
    geom
}

fn half_size_top_left_geom(space: &Viewport, offset: usize) -> PaneGeom {
    let mut geom = PaneGeom {
        x: space.x + 2 + offset,
        y: space.y + 2 + offset,
        cols: Dimension::fixed(space.cols / 3),
        rows: Dimension::fixed(space.rows / 3),
    };
    geom.cols.set_inner(space.cols / 3);
    geom.rows.set_inner(space.rows / 3);
    geom
}

fn half_size_top_right_geom(space: &Viewport, offset: usize) -> PaneGeom {
    let mut geom = PaneGeom {
        x: ((space.x + space.cols) - (space.cols / 3) - 2).saturating_sub(offset),
        y: space.y + 2 + offset,
        cols: Dimension::fixed(space.cols / 3),
        rows: Dimension::fixed(space.rows / 3),
    };
    geom.cols.set_inner(space.cols / 3);
    geom.rows.set_inner(space.rows / 3);
    geom
}

fn half_size_bottom_left_geom(space: &Viewport, offset: usize) -> PaneGeom {
    let mut geom = PaneGeom {
        x: space.x + 2 + offset,
        y: ((space.y + space.rows) - (space.rows / 3) - 2).saturating_sub(offset),
        cols: Dimension::fixed(space.cols / 3),
        rows: Dimension::fixed(space.rows / 3),
    };
    geom.cols.set_inner(space.cols / 3);
    geom.rows.set_inner(space.rows / 3);
    geom
}

fn half_size_bottom_right_geom(space: &Viewport, offset: usize) -> PaneGeom {
    let mut geom = PaneGeom {
        x: ((space.x + space.cols) - (space.cols / 3) - 2).saturating_sub(offset),
        y: ((space.y + space.rows) - (space.rows / 3) - 2).saturating_sub(offset),
        cols: Dimension::fixed(space.cols / 3),
        rows: Dimension::fixed(space.rows / 3),
    };
    geom.cols.set_inner(space.cols / 3);
    geom.rows.set_inner(space.rows / 3);
    geom
}

fn pane_geom_is_inside_viewport(viewport: &Viewport, geom: &PaneGeom) -> bool {
    geom.y >= viewport.y
        && geom.y + geom.rows.as_usize() <= viewport.y + viewport.rows
        && geom.x >= viewport.x
        && geom.x + geom.cols.as_usize() <= viewport.x + viewport.cols
}

fn pane_geom_is_big_enough(geom: &PaneGeom) -> bool {
    geom.rows.as_usize() >= MIN_TERMINAL_HEIGHT && geom.cols.as_usize() >= MIN_TERMINAL_WIDTH
}

fn pane_geom_is_unoccupied_and_inside_viewport(viewport: &Viewport, geom: &PaneGeom, existing_geoms: &[PaneGeom]) -> bool {
    pane_geom_is_inside_viewport(viewport, geom) &&
        !existing_geoms.iter().find(|p| *p == geom).is_some()
}
