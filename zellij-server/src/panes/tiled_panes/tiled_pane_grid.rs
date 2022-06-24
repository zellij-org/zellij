use super::is_inside_viewport;
use super::pane_resizer::PaneResizer;
use crate::tab::{MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH};
use crate::{panes::PaneId, tab::Pane};
use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use zellij_utils::{
    input::layout::Direction,
    pane_size::{Constraint, Dimension, PaneGeom, Size, Viewport},
};

use std::cell::RefCell;
use std::rc::Rc;

const DEFAULT_RESIZE_PERCENT: f64 = 5.0;
const DEFAULT_CURSOR_HEIGHT_WIDTH_RATIO: usize = 4;

type BorderAndPaneIds = (usize, Vec<PaneId>);

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

    pub fn layout(&mut self, direction: Direction, space: usize) -> Result<(), String> {
        let mut pane_resizer = PaneResizer::new(self.panes.clone());
        pane_resizer.layout(direction, space)
    }
    pub fn resize_pane_left(&mut self, pane_id: &PaneId, constraint: Option<Constraint>) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let increment = match constraint.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.cols as f64),
            Constraint::Percent(percent) => percent,
        };
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.try_increase_pane_and_surroundings_left(pane_id, increment) {
            return;
        }
        self.try_reduce_pane_and_surroundings_left(pane_id, increment);
    }
    pub fn resize_pane_right(&mut self, pane_id: &PaneId, constraint: Option<Constraint>) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let increment = match constraint.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.cols as f64),
            Constraint::Percent(percent) => percent,
        };
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.try_increase_pane_and_surroundings_right(pane_id, increment) {
            return;
        }
        self.try_reduce_pane_and_surroundings_right(pane_id, increment);
    }
    pub fn resize_pane_down(&mut self, pane_id: &PaneId, constraint: Option<Constraint>) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let increment = match constraint.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.rows as f64),
            Constraint::Percent(percent) => percent,
        };
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.try_increase_pane_and_surroundings_down(pane_id, increment) {
            return;
        }
        self.try_reduce_pane_and_surroundings_down(pane_id, increment);
    }
    pub fn resize_pane_up(&mut self, pane_id: &PaneId, constraint: Option<Constraint>) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let increment = match constraint.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.rows as f64),
            Constraint::Percent(percent) => percent,
        };
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.try_increase_pane_and_surroundings_up(pane_id, increment) {
            return;
        }
        self.try_reduce_pane_and_surroundings_up(pane_id, increment);
    }
    pub fn resize_increase(
        &mut self,
        pane_id: &PaneId,
        cx: Option<Constraint>,
        cy: Option<Constraint>,
    ) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let inc_x = match cx.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.cols as f64),
            Constraint::Percent(percent) => percent,
        };
        let inc_y = match cy.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.rows as f64),
            Constraint::Percent(percent) => percent,
        };
        if self.try_increase_pane_and_surroundings_right_and_down(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_down(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_increase_pane_and_surroundings_right_and_up(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_up(pane_id, inc_x, inc_y) {
            return;
        }

        if self.try_increase_pane_and_surroundings_right(pane_id, inc_x) {
            return;
        }
        if self.try_increase_pane_and_surroundings_down(pane_id, inc_y) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left(pane_id, inc_x) {
            return;
        }
        self.try_increase_pane_and_surroundings_up(pane_id, inc_y);
    }
    pub fn resize_decrease(
        &mut self,
        pane_id: &PaneId,
        cx: Option<Constraint>,
        cy: Option<Constraint>,
    ) {
        let default_constraint = Constraint::Percent(DEFAULT_RESIZE_PERCENT);
        let inc_x = match cx.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.cols as f64),
            Constraint::Percent(percent) => percent,
        };
        let inc_y = match cy.unwrap_or(default_constraint) {
            Constraint::Fixed(value) => 100.0 * value as f64 / (self.display_area.rows as f64),
            Constraint::Percent(percent) => percent,
        };
        if self.try_reduce_pane_and_surroundings_left_and_up(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_up(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_down(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left_and_down(pane_id, inc_x, inc_y) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left(pane_id, inc_x) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right(pane_id, inc_x) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_up(pane_id, inc_y) {
            return;
        }
        self.try_reduce_pane_and_surroundings_down(pane_id, inc_y);
    }
    fn can_increase_pane_and_surroundings_right(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(pane_id) {
            panes_to_the_right
                .iter()
                .all(|id| self.can_reduce_pane_width(id, increase_by))
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_left(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(pane_id) {
            panes_to_the_left
                .iter()
                .all(|id| self.can_reduce_pane_width(id, increase_by))
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_down(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_below) = self.pane_ids_directly_below(pane_id) {
            panes_below
                .iter()
                .all(|id| self.can_reduce_pane_height(id, increase_by))
        } else {
            false
        }
    }
    fn can_increase_pane_and_surroundings_up(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_above) = self.pane_ids_directly_above(pane_id) {
            panes_above
                .iter()
                .all(|id| self.can_reduce_pane_height(id, increase_by))
        } else {
            false
        }
    }
    fn can_reduce_pane_width(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let current_fixed_cols = pane.position_and_size().cols.as_usize();
        let will_reduce_by = ((self.display_area.cols as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_cols.saturating_sub(will_reduce_by) < pane.min_width() {
            false
        } else if let Some(cols) = pane.position_and_size().cols.as_percent() {
            cols - reduce_by >= 100.0 * pane.min_width() as f64 / (self.display_area.cols as f64)
        } else {
            false
        }
    }
    fn can_reduce_pane_height(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let panes = self.panes.borrow();
        let pane = panes.get(pane_id).unwrap();
        let current_fixed_rows = pane.position_and_size().rows.as_usize();
        let will_reduce_by = ((self.display_area.rows as f64 / 100.0) * reduce_by) as usize;
        if current_fixed_rows.saturating_sub(will_reduce_by) < pane.min_height() {
            false
        } else if let Some(rows) = pane.position_and_size().rows.as_percent() {
            rows - reduce_by >= 100.0 * pane.min_height() as f64 / (self.display_area.rows as f64)
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_right(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let ids_left = self.pane_ids_directly_left_of(pane_id);
        let flexible_left = self.ids_are_flexible(Direction::Horizontal, ids_left);
        if flexible_left {
            self.can_reduce_pane_width(pane_id, reduce_by)
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_left(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let ids_right = self.pane_ids_directly_right_of(pane_id);
        let flexible_right = self.ids_are_flexible(Direction::Horizontal, ids_right);
        if flexible_right {
            self.can_reduce_pane_width(pane_id, reduce_by)
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_down(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let ids_above = self.pane_ids_directly_above(pane_id);
        let flexible_above = self.ids_are_flexible(Direction::Vertical, ids_above);
        if flexible_above {
            self.can_reduce_pane_height(pane_id, reduce_by)
        } else {
            false
        }
    }
    fn can_reduce_pane_and_surroundings_up(&self, pane_id: &PaneId, reduce_by: f64) -> bool {
        let ids_below = self.pane_ids_directly_below(pane_id);
        let flexible_below = self.ids_are_flexible(Direction::Vertical, ids_below);
        if flexible_below {
            self.can_reduce_pane_height(pane_id, reduce_by)
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
            let min_height = 100.0 * pane.min_height() as f64 / (self.display_area.rows as f64);
            if pane.current_geom().rows.as_percent().unwrap() - percent < min_height {
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
            let min_height = 100.0 * pane.min_height() as f64 / (self.display_area.rows as f64);
            if pane.current_geom().rows.as_percent().unwrap() - percent < min_height {
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
            let min_width = 100.0 * pane.min_width() as f64 / (self.display_area.cols as f64);
            if pane.current_geom().cols.as_percent().unwrap() - percent < min_width {
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
            let min_width = 100.0 * pane.min_width() as f64 / (self.display_area.cols as f64);
            if pane.current_geom().cols.as_percent().unwrap() - percent < min_width {
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
    fn try_increase_pane_and_surroundings_right_and_up(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_increase_pane_right =
            self.can_increase_pane_and_surroundings_right(pane_id, increase_x);
        let can_increase_pane_up = self.can_increase_pane_and_surroundings_up(pane_id, increase_y);
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
            self.try_increase_pane_and_surroundings_right(pane_id, increase_x);
            self.try_increase_pane_and_surroundings_up(pane_id, increase_y);
            if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
            {
                self.try_reduce_pane_and_surroundings_right(
                    &pane_above_with_right_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_left_and_up(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_increase_pane_left =
            self.can_increase_pane_and_surroundings_left(pane_id, increase_x);
        let can_increase_pane_up = self.can_increase_pane_and_surroundings_up(pane_id, increase_y);
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
            self.try_increase_pane_and_surroundings_left(pane_id, increase_x);
            self.try_increase_pane_and_surroundings_up(pane_id, increase_y);
            if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
                self.try_reduce_pane_and_surroundings_left(
                    &pane_above_with_left_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_right_and_down(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_increase_pane_right =
            self.can_increase_pane_and_surroundings_right(pane_id, increase_x);
        let can_increase_pane_down =
            self.can_increase_pane_and_surroundings_down(pane_id, increase_y);
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
            self.try_increase_pane_and_surroundings_right(pane_id, increase_x);
            self.try_increase_pane_and_surroundings_down(pane_id, increase_y);
            if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
            {
                self.try_reduce_pane_and_surroundings_right(
                    &pane_below_with_right_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_increase_pane_and_surroundings_left_and_down(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_increase_pane_left =
            self.can_increase_pane_and_surroundings_left(pane_id, increase_x);
        let can_increase_pane_down =
            self.can_increase_pane_and_surroundings_down(pane_id, increase_y);
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
            self.try_increase_pane_and_surroundings_left(pane_id, increase_x);
            self.try_increase_pane_and_surroundings_down(pane_id, increase_y);
            if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
                self.try_reduce_pane_and_surroundings_left(
                    &pane_below_with_left_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_right_and_up(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_reduce_pane_right =
            self.can_reduce_pane_and_surroundings_right(pane_id, increase_x);
        let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, increase_y);
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
            self.try_reduce_pane_and_surroundings_right(pane_id, increase_x);
            self.try_reduce_pane_and_surroundings_up(pane_id, increase_y);
            if let Some(pane_below_with_left_aligned_border) = pane_below_with_left_aligned_border {
                self.try_increase_pane_and_surroundings_right(
                    &pane_below_with_left_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_left_and_up(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_reduce_pane_left = self.can_reduce_pane_and_surroundings_left(pane_id, increase_x);
        let can_reduce_pane_up = self.can_reduce_pane_and_surroundings_up(pane_id, increase_y);
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
            self.try_reduce_pane_and_surroundings_left(pane_id, increase_x);
            self.try_reduce_pane_and_surroundings_up(pane_id, increase_y);
            if let Some(pane_below_with_right_aligned_border) = pane_below_with_right_aligned_border
            {
                self.try_increase_pane_and_surroundings_left(
                    &pane_below_with_right_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_right_and_down(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_reduce_pane_right =
            self.can_reduce_pane_and_surroundings_right(pane_id, increase_x);
        let can_reduce_pane_down = self.can_reduce_pane_and_surroundings_down(pane_id, increase_y);
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
            self.try_reduce_pane_and_surroundings_right(pane_id, increase_x);
            self.try_reduce_pane_and_surroundings_down(pane_id, increase_y);
            if let Some(pane_above_with_left_aligned_border) = pane_above_with_left_aligned_border {
                self.try_increase_pane_and_surroundings_right(
                    &pane_above_with_left_aligned_border,
                    increase_x,
                );
            }
            true
        } else {
            false
        }
    }
    fn try_reduce_pane_and_surroundings_left_and_down(
        &mut self,
        pane_id: &PaneId,
        increase_x: f64,
        increase_y: f64,
    ) -> bool {
        let can_reduce_pane_left = self.can_reduce_pane_and_surroundings_left(pane_id, increase_x);
        let can_reduce_pane_down = self.can_reduce_pane_and_surroundings_down(pane_id, increase_y);
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
            self.try_reduce_pane_and_surroundings_left(pane_id, increase_x);
            self.try_reduce_pane_and_surroundings_down(pane_id, increase_y);
            if let Some(pane_above_with_right_aligned_border) = pane_above_with_right_aligned_border
            {
                self.try_increase_pane_and_surroundings_left(
                    &pane_above_with_right_aligned_border,
                    increase_x,
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
            },
            Direction::Vertical => {
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
    pub fn find_room_for_new_pane(
        &self,
        cursor_height_width_ratio: Option<usize>,
    ) -> Option<(PaneId, Direction)> {
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
