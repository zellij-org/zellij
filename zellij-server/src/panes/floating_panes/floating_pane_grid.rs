use crate::tab::{MIN_TERMINAL_HEIGHT, MIN_TERMINAL_WIDTH};
use crate::{panes::PaneId, tab::Pane};
use std::cmp::Ordering;
use std::collections::HashMap;
use zellij_utils::pane_size::{Dimension, PaneGeom, Size, Viewport};

use std::cell::RefCell;
use std::rc::Rc;

const RESIZE_INCREMENT_WIDTH: usize = 5;
const RESIZE_INCREMENT_HEIGHT: usize = 2;
const MOVE_INCREMENT_HORIZONTAL: usize = 10;
const MOVE_INCREMENT_VERTICAL: usize = 5;

const MAX_PANES: usize = 100;

pub struct FloatingPaneGrid<'a> {
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
        let display_size_column_difference =
            self.display_area.cols.saturating_sub(self.viewport.cols);

        let mut new_viewport = self.viewport;
        new_viewport.cols = space.cols.saturating_sub(display_size_column_difference);
        new_viewport.rows = space.rows.saturating_sub(display_size_row_difference);

        for (pane_id, pane) in panes.iter_mut() {
            let mut new_pane_geom = pane.current_geom();
            let desired_pane_geom = desired_pane_positions.get(pane_id).unwrap();
            let desired_pane_geom_is_inside_viewport =
                pane_geom_is_inside_viewport(&new_viewport, desired_pane_geom);
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
                if excess_width > 0 && new_pane_geom.x.saturating_sub(excess_width) > new_viewport.x
                {
                    new_pane_geom.x = new_pane_geom.x.saturating_sub(excess_width);
                } else if excess_width > 0
                    && new_pane_geom.cols.as_usize().saturating_sub(excess_width)
                        > MIN_TERMINAL_WIDTH
                {
                    new_pane_geom
                        .cols
                        .set_inner(new_pane_geom.cols.as_usize().saturating_sub(excess_width));
                } else if excess_width > 0 {
                    let reduce_x_by = new_pane_geom.x.saturating_sub(new_viewport.x);
                    let reduced_width = new_pane_geom
                        .cols
                        .as_usize()
                        .saturating_sub(excess_width.saturating_sub(reduce_x_by));
                    new_pane_geom.x = new_viewport.x;
                    new_pane_geom
                        .cols
                        .set_inner(std::cmp::max(reduced_width, MIN_TERMINAL_WIDTH));
                }

                // handle shrink height
                if excess_height > 0
                    && new_pane_geom.y.saturating_sub(excess_height) > new_viewport.y
                {
                    new_pane_geom.y = new_pane_geom.y.saturating_sub(excess_height);
                } else if excess_height > 0
                    && new_pane_geom.rows.as_usize().saturating_sub(excess_height)
                        > MIN_TERMINAL_HEIGHT
                {
                    new_pane_geom
                        .rows
                        .set_inner(new_pane_geom.rows.as_usize().saturating_sub(excess_height));
                } else if excess_height > 0 {
                    let reduce_y_by = new_pane_geom.y.saturating_sub(new_viewport.y);
                    let reduced_height = new_pane_geom
                        .rows
                        .as_usize()
                        .saturating_sub(excess_height.saturating_sub(reduce_y_by));
                    new_pane_geom.y = new_viewport.y;
                    new_pane_geom
                        .rows
                        .set_inner(std::cmp::max(reduced_height, MIN_TERMINAL_HEIGHT));
                }

                // handle expand width
                if extra_width > 0 {
                    let max_right_coords = new_viewport.x + new_viewport.cols;
                    if new_pane_geom.x < desired_pane_geom.x {
                        if desired_pane_geom.x + new_pane_geom.cols.as_usize() <= max_right_coords {
                            new_pane_geom.x = desired_pane_geom.x
                        } else if new_pane_geom.x + new_pane_geom.cols.as_usize() + extra_width
                            < max_right_coords
                        {
                            new_pane_geom.x += extra_width;
                        } else {
                            new_pane_geom.x =
                                max_right_coords.saturating_sub(new_pane_geom.cols.as_usize());
                        }
                    }
                    if new_pane_geom.cols.as_usize() < desired_pane_geom.cols.as_usize() {
                        if new_pane_geom.x + desired_pane_geom.cols.as_usize() <= max_right_coords {
                            new_pane_geom
                                .cols
                                .set_inner(desired_pane_geom.cols.as_usize());
                        } else if new_pane_geom.x + new_pane_geom.cols.as_usize() + extra_width
                            < max_right_coords
                        {
                            new_pane_geom
                                .cols
                                .set_inner(new_pane_geom.cols.as_usize() + extra_width);
                        } else {
                            new_pane_geom.cols.set_inner(
                                new_pane_geom.cols.as_usize()
                                    + (max_right_coords
                                        - (new_pane_geom.x + new_pane_geom.cols.as_usize())),
                            );
                        }
                    }
                }

                // handle expand height
                if extra_height > 0 {
                    let max_bottom_coords = new_viewport.y + new_viewport.rows;
                    if new_pane_geom.y < desired_pane_geom.y {
                        if desired_pane_geom.y + new_pane_geom.rows.as_usize() <= max_bottom_coords
                        {
                            new_pane_geom.y = desired_pane_geom.y
                        } else if new_pane_geom.y + new_pane_geom.rows.as_usize() + extra_height
                            < max_bottom_coords
                        {
                            new_pane_geom.y += extra_height;
                        } else {
                            new_pane_geom.y =
                                max_bottom_coords.saturating_sub(new_pane_geom.rows.as_usize());
                        }
                    }
                    if new_pane_geom.rows.as_usize() < desired_pane_geom.rows.as_usize() {
                        if new_pane_geom.y + desired_pane_geom.rows.as_usize() <= max_bottom_coords
                        {
                            new_pane_geom
                                .rows
                                .set_inner(desired_pane_geom.rows.as_usize());
                        } else if new_pane_geom.y + new_pane_geom.rows.as_usize() + extra_height
                            < max_bottom_coords
                        {
                            new_pane_geom
                                .rows
                                .set_inner(new_pane_geom.rows.as_usize() + extra_height);
                        } else {
                            new_pane_geom.rows.set_inner(
                                new_pane_geom.rows.as_usize()
                                    + (max_bottom_coords
                                        - (new_pane_geom.y + new_pane_geom.rows.as_usize())),
                            );
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
    pub fn find_room_for_new_pane(&self) -> Option<PaneGeom> {
        let panes = self.panes.borrow();
        let pane_geoms: Vec<PaneGeom> = panes.values().map(|p| p.position_and_size()).collect();
        for offset in 0..MAX_PANES / 5 {
            let half_size_middle_geom = half_size_middle_geom(&self.viewport, offset);
            let half_size_top_left_geom = half_size_top_left_geom(&self.viewport, offset);
            let half_size_top_right_geom = half_size_top_right_geom(&self.viewport, offset);
            let half_size_bottom_left_geom = half_size_bottom_left_geom(&self.viewport, offset);
            let half_size_bottom_right_geom = half_size_bottom_right_geom(&self.viewport, offset);
            if pane_geom_is_big_enough(&half_size_middle_geom)
                && pane_geom_is_unoccupied_and_inside_viewport(
                    &self.viewport,
                    &half_size_middle_geom,
                    &pane_geoms,
                )
            {
                return Some(half_size_middle_geom);
            } else if pane_geom_is_big_enough(&half_size_top_left_geom)
                && pane_geom_is_unoccupied_and_inside_viewport(
                    &self.viewport,
                    &half_size_top_left_geom,
                    &pane_geoms,
                )
            {
                return Some(half_size_top_left_geom);
            } else if pane_geom_is_big_enough(&half_size_top_right_geom)
                && pane_geom_is_unoccupied_and_inside_viewport(
                    &self.viewport,
                    &half_size_top_right_geom,
                    &pane_geoms,
                )
            {
                return Some(half_size_top_right_geom);
            } else if pane_geom_is_big_enough(&half_size_bottom_left_geom)
                && pane_geom_is_unoccupied_and_inside_viewport(
                    &self.viewport,
                    &half_size_bottom_left_geom,
                    &pane_geoms,
                )
            {
                return Some(half_size_bottom_left_geom);
            } else if pane_geom_is_big_enough(&half_size_bottom_right_geom)
                && pane_geom_is_unoccupied_and_inside_viewport(
                    &self.viewport,
                    &half_size_bottom_right_geom,
                    &pane_geoms,
                )
            {
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

fn pane_geom_is_unoccupied_and_inside_viewport(
    viewport: &Viewport,
    geom: &PaneGeom,
    existing_geoms: &[PaneGeom],
) -> bool {
    pane_geom_is_inside_viewport(viewport, geom) && !existing_geoms.iter().any(|p| p == geom)
}
