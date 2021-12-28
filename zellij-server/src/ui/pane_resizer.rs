use crate::{panes::PaneId, tab::Pane};
use crate::tab::is_inside_viewport;
use cassowary::{
    strength::{REQUIRED, STRONG},
    Expression, Solver, Variable,
    WeightedRelation::EQ,
};
use std::collections::{HashMap, HashSet};
use std::cmp::Reverse;
use zellij_utils::{
    input::layout::Direction,
    pane_size::{Constraint, Dimension, PaneGeom, Size, Viewport},
};

const RESIZE_PERCENT: f64 = 5.0;
type BorderAndPaneIds = (usize, Vec<PaneId>);

pub struct PaneResizer<'a> {
    panes: HashMap<&'a PaneId, &'a mut Box<dyn Pane>>,
    vars: HashMap<PaneId, Variable>,
    solver: Solver,
    display_area: Size, // includes all panes (including eg. the status bar and tab bar in the default layout)
    viewport: Viewport, // includes all non-UI panes
}

// FIXME: Just hold a mutable Pane reference instead of the PaneId, fixed, pos, and size?
// Do this after panes are no longer trait-objects!
#[derive(Debug, Clone, Copy)]
struct Span {
    pid: PaneId,
    direction: Direction,
    pos: usize,
    size: Dimension,
    size_var: Variable,
}

type Grid = Vec<Vec<Span>>;

impl<'a> PaneResizer<'a> {
    pub fn new(panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>, display_area: Size, viewport: Viewport) -> Self {
        let panes: HashMap<_, _> = panes.into_iter().collect();
        let mut vars = HashMap::new();
        for &&k in panes.keys() {
            vars.insert(k, Variable::new());
        }
        PaneResizer {
            panes,
            vars,
            solver: Solver::new(),
            display_area,
            viewport,
        }
    }

    pub fn layout(&mut self, direction: Direction, space: usize) -> Result<(), String> {
        self.solver.reset();
        let grid = self.solve(direction, space)?;
        let spans = self.discretize_spans(grid, space)?;
        self.apply_spans(spans);
        Ok(())
    }
    pub fn resize_pane_left(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
        } else if self.can_reduce_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
        }
    }
    pub fn resize_pane_right(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
        } else if self.can_reduce_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
        }
    }
    pub fn resize_pane_down(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.rows instead of passing it
        } else if self.can_reduce_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.rows instead of passing it
        }
    }
    pub fn resize_pane_up(&mut self, pane_id: &PaneId) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        if self.can_increase_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT) {
            self.increase_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.rows instead of passing it
        } else if self.can_reduce_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT) {
            self.reduce_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.rows instead of passing it
        }
    }
    pub fn resize_increase(&mut self, pane_id: &PaneId) {
        if self.try_increase_pane_and_surroundings_right_and_down(&pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_down(&pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_right_and_up(&pane_id) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left_and_up(&pane_id) {
            return;
        }

        if self.try_increase_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_increase_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_increase_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT) {
            return;
        }
        self.try_increase_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT);
    }
    pub fn resize_decrease(&mut self, pane_id: &PaneId) {
        if self.try_reduce_pane_and_surroundings_left_and_up(&pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_up(&pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right_and_down(&pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left_and_down(&pane_id) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_left(&pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_right(&pane_id, RESIZE_PERCENT) {
            return;
        }
        if self.try_reduce_pane_and_surroundings_up(&pane_id, RESIZE_PERCENT) {
            return;
        }
        self.try_reduce_pane_and_surroundings_down(&pane_id, RESIZE_PERCENT);
    }
    fn can_increase_pane_and_surroundings_right(&self, pane_id: &PaneId, increase_by: f64) -> bool {
        if let Some(panes_to_the_right) = self.pane_ids_directly_right_of(pane_id) {
            panes_to_the_right.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
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
        if let Some(panes_to_the_left) = self.pane_ids_directly_left_of(pane_id) {
            panes_to_the_left.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
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
        if let Some(panes_below) = self.pane_ids_directly_below(pane_id) {
            panes_below.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
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
        if let Some(panes_above) = self.pane_ids_directly_above(pane_id) {
            panes_above.iter().all(|id| {
                let p = self.panes.get(id).unwrap();
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
        let pane = self.panes.get(pane_id).unwrap();
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
        let pane = self.panes.get(pane_id).unwrap();
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
        let pane = self.panes.get(pane_id).unwrap();
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
        let pane = self.panes.get(pane_id).unwrap();
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
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.reduce_height(percent);
    }
    fn increase_pane_height(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.increase_height(percent);
    }
    fn increase_pane_width(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.increase_width(percent);
    }
    fn reduce_pane_width(&mut self, id: &PaneId, percent: f64) {
        let terminal = self.panes.get_mut(id).unwrap();
        terminal.reduce_width(percent);
    }
    fn increase_pane_and_surroundings_up(&mut self, id: &PaneId, percent: f64) {
        let mut terminals_above = self
            .pane_ids_directly_above(id)
            .expect("can't increase pane size up if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.get(t).unwrap().x())
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
            .map(|t| self.panes.get(t).unwrap().x())
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
                return self.panes.get(t).unwrap().y();
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
            .map(|t| self.panes.get(t).unwrap().y())
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
            .map(|t| self.panes.get(t).unwrap().x())
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
            let pane = self.panes.get(terminal_id).unwrap();
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
            .map(|t| self.panes.get(t).unwrap().x())
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
            let pane = self.panes.get(terminal_id).unwrap();
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
            .map(|t| self.panes.get(t).unwrap().y())
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
            let pane = self.panes.get(terminal_id).unwrap();
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
            .map(|t| self.panes.get(t).unwrap().y())
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
            let pane = self.panes.get(terminal_id).unwrap();
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
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(id).unwrap();
        if terminal_to_check.x() == 0 {
            return None;
        }
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in self.panes.iter() {
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                ids.push(*pid);
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
        let terminal_to_check = self.panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in self.panes.iter() {
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                ids.push(*pid);
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
        let terminal_to_check = self.panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in self.panes.iter() {
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                ids.push(*pid);
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
        let terminal_to_check = self.panes.get(id).unwrap();
        // for (&pid, terminal) in self.get_panes() {
        for (&pid, terminal) in self.panes.iter() {
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn panes_top_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.y() == pane.y())
            .collect()
    }
    fn panes_bottom_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.y() + terminal.rows() == pane.y() + pane.rows()
            })
            .collect()
    }
    fn panes_right_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.x() + terminal.cols() == pane.x() + pane.cols()
            })
            .collect()
    }
    fn panes_left_aligned_with_pane(&self, pane: &dyn Pane) -> Vec<&dyn Pane> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(t_id).unwrap().as_ref())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.x() == pane.x())
            .collect()
    }
    fn right_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by_key(|a| Reverse(a.y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_right
                .get(&bottom_terminal_boundary)
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn right_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by_key(|a| a.y());
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the right
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_right
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() + terminal.rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_above(
        &self,
        id: &PaneId,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by_key(|a| Reverse(a.y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_left
                .get(&bottom_terminal_boundary)
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_below(
        &self,
        id: &PaneId,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by_key(|a| a.y());
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the left
        let mut bottom_resize_border = self.viewport.y + self.viewport.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_left
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| {
            // terminal.y() + terminal.rows() < bottom_resize_border
            terminal.y() + terminal.rows() <= bottom_resize_border
        });
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(id)
            .expect("terminal id does not exist")
            .as_ref();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by_key(|a| Reverse(a.x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.cols();
            if terminal_borders_above
                .get(&right_terminal_boundary)
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by_key(|a| a.x());
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                terminals.push(terminal);
            }
        }
        // rightmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_above
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.cols() <= right_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.cols()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_left(
        &self,
        id: &PaneId,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(terminal_to_check);
        bottom_aligned_terminals.sort_by_key(|a| Reverse(a.x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.cols() == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.cols();
            if terminal_borders_below
                .get(&right_terminal_boundary)
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_right(
        &self,
        id: &PaneId,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(id).unwrap().as_ref();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(terminal_to_check);
        bottom_aligned_terminals.sort_by_key(|a| a.x());
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.cols() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut right_resize_border = self.viewport.x + self.viewport.cols;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_below
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.cols() <= right_resize_border);
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.cols()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<PaneId> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn ids_are_flexible(&self, direction: Direction, pane_ids: Option<Vec<PaneId>>) -> bool {
        pane_ids.is_some()
            && pane_ids.unwrap().iter().all(|id| {
                let geom = self.panes[id].current_geom();
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
        let terminal = self
            .panes
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.x() >= left_border_x && terminal.x() + terminal.cols() <= right_border_x
    }
    fn pane_is_between_horizontal_borders(
        &self,
        id: &PaneId,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let terminal = self
            .panes
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.y() >= top_border_y && terminal.y() + terminal.rows() <= bottom_border_y
    }
    fn try_increase_pane_and_surroundings_right(
        &mut self,
        pane_id: &PaneId,
        reduce_by: f64,
    ) -> bool {
        if self.can_increase_pane_and_surroundings_right(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_right(pane_id, reduce_by);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
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
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
            return true;
        }
        false
    }
    fn try_increase_pane_and_surroundings_up(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_increase_pane_and_surroundings_up(pane_id, reduce_by) {
            self.increase_pane_and_surroundings_up(pane_id, reduce_by);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.cols instead of passing it
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
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.cols instead of passing it
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
                    let pane = self.panes.get(p_id).unwrap();
                    let active_pane = self.panes.get(pane_id).unwrap();
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
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_left(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_left(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_left(pane_id, reduce_by);
            let _ = self.layout(Direction::Horizontal, self.display_area.cols); // TODO: use self.display_area.cols instead of passing it
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_up(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_up(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_up(pane_id, reduce_by);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.cols instead of passing it
            return true;
        }
        false
    }
    fn try_reduce_pane_and_surroundings_down(&mut self, pane_id: &PaneId, reduce_by: f64) -> bool {
        if self.can_reduce_pane_and_surroundings_down(pane_id, reduce_by) {
            self.reduce_pane_and_surroundings_down(pane_id, reduce_by);
            let _ = self.layout(Direction::Vertical, self.display_area.rows); // TODO: use self.display_area.cols instead of passing it
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
        is_inside_viewport(&self.viewport, self.panes.get(pane_id).unwrap())
    }
    fn solve(&mut self, direction: Direction, space: usize) -> Result<Grid, String> {
        let grid: Grid = self
            .grid_boundaries(direction)
            .into_iter()
            .map(|b| self.spans_in_boundary(direction, b))
            .collect();

        let constraints: HashSet<_> = grid
            .iter()
            .flat_map(|s| constrain_spans(space, s))
            .collect();

        self.solver
            .add_constraints(&constraints)
            .map_err(|e| format!("{:?}", e))?;

        Ok(grid)
    }

    fn discretize_spans(&mut self, mut grid: Grid, space: usize) -> Result<Vec<Span>, String> {
        let mut rounded_sizes: HashMap<_, _> = grid
            .iter()
            .flatten()
            .map(|s| {
                (
                    s.size_var,
                    stable_round(self.solver.get_value(s.size_var)) as isize,
                )
            })
            .collect();

        // Round f64 pane sizes to usize without gaps or overlap
        let mut finalised = Vec::new();
        for spans in &mut grid {
            let rounded_size: isize = spans.iter().map(|s| rounded_sizes[&s.size_var]).sum();
            let mut error = space as isize - rounded_size;
            let mut flex_spans: Vec<_> = spans
                .iter_mut()
                .filter(|s| !s.size.is_fixed() && !finalised.contains(&s.pid))
                .collect();
            flex_spans.sort_by_key(|s| rounded_sizes[&s.size_var]);
            if error < 0 {
                flex_spans.reverse();
            }
            for span in flex_spans {
                rounded_sizes
                    .entry(span.size_var)
                    .and_modify(|s| *s += error.signum());
                error -= error.signum();
            }
            finalised.extend(spans.iter().map(|s| s.pid));
        }

        // Update span positions based on their rounded sizes
        for spans in &mut grid {
            let mut offset = 0;
            for span in spans {
                span.pos = offset;
                let sz = rounded_sizes[&span.size_var];
                if sz < 1 {
                    return Err("Ran out of room for spans".into());
                }
                span.size.set_inner(sz as usize);
                offset += span.size.as_usize();
            }
        }

        Ok(grid.into_iter().flatten().collect())
    }

    fn apply_spans(&mut self, spans: Vec<Span>) {
        for span in spans {
            let pane = self.panes.get_mut(&span.pid).unwrap();
            let new_geom = match span.direction {
                Direction::Horizontal => PaneGeom {
                    x: span.pos,
                    cols: span.size,
                    ..pane.current_geom()
                },
                Direction::Vertical => PaneGeom {
                    y: span.pos,
                    rows: span.size,
                    ..pane.current_geom()
                },
            };
            if pane.geom_override().is_some() {
                pane.get_geom_override(new_geom);
            } else {
                pane.set_geom(new_geom);
            }
        }
    }

    // FIXME: Functions like this should have unit tests!
    fn grid_boundaries(&self, direction: Direction) -> Vec<(usize, usize)> {
        // Select the spans running *perpendicular* to the direction of resize
        let spans: Vec<Span> = self
            .panes
            .values()
            .map(|p| self.get_span(!direction, p.as_ref()))
            .collect();

        let mut last_edge = 0;
        let mut bounds = Vec::new();
        let mut edges: Vec<usize> = spans.iter().map(|s| s.pos + s.size.as_usize()).collect();
        edges.sort_unstable();
        edges.dedup();
        for next in edges {
            let next_edge = next;
            bounds.push((last_edge, next_edge));
            last_edge = next_edge;
        }
        bounds
    }

    fn spans_in_boundary(&self, direction: Direction, boundary: (usize, usize)) -> Vec<Span> {
        let bwn = |v, (s, e)| s <= v && v < e;
        let mut spans: Vec<_> = self
            .panes
            .values()
            .filter(|p| {
                let s = self.get_span(!direction, p.as_ref());
                let span_bounds = (s.pos, s.pos + s.size.as_usize());
                bwn(span_bounds.0, boundary)
                    || (bwn(boundary.0, span_bounds)
                        && (bwn(boundary.1, span_bounds) || boundary.1 == span_bounds.1))
            })
            .map(|p| self.get_span(direction, p.as_ref()))
            .collect();
        spans.sort_unstable_by_key(|s| s.pos);
        spans
    }

    fn get_span(&self, direction: Direction, pane: &dyn Pane) -> Span {
        let pas = pane.current_geom();
        let size_var = self.vars[&pane.pid()];
        match direction {
            Direction::Horizontal => Span {
                pid: pane.pid(),
                direction,
                pos: pas.x,
                size: pas.cols,
                size_var,
            },
            Direction::Vertical => Span {
                pid: pane.pid(),
                direction,
                pos: pas.y,
                size: pas.rows,
                size_var,
            },
        }
    }
}

fn constrain_spans(space: usize, spans: &[Span]) -> HashSet<cassowary::Constraint> {
    let mut constraints = HashSet::new();

    // Calculating "flexible" space (space not consumed by fixed-size spans)
    let new_flex_space = spans.iter().fold(space, |a, s| {
        if let Constraint::Fixed(sz) = s.size.constraint {
            a.saturating_sub(sz)
        } else {
            a
        }
    });

    // Spans must use all of the available space
    let full_size = spans
        .iter()
        .fold(Expression::from_constant(0.0), |acc, s| acc + s.size_var);
    constraints.insert(full_size | EQ(REQUIRED) | space as f64);

    // Try to maintain ratios and lock non-flexible sizes
    for span in spans {
        match span.size.constraint {
            Constraint::Fixed(s) => constraints.insert(span.size_var | EQ(REQUIRED) | s as f64),
            Constraint::Percent(p) => constraints
                .insert((span.size_var / new_flex_space as f64) | EQ(STRONG) | (p / 100.0)),
        };
    }

    constraints
}

fn stable_round(x: f64) -> f64 {
    ((x * 100.0).round() / 100.0).round()
}
