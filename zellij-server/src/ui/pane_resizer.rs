use crate::{os_input_output::ServerOsApi, panes::PaneId, tab::Pane};
use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashSet},
};
use zellij_utils::pane_size::PositionAndSize;

pub(crate) struct PaneResizer<'a> {
    panes: &'a mut BTreeMap<PaneId, Box<dyn Pane>>,
    os_api: &'a mut Box<dyn ServerOsApi>,
}

// TODO: currently there are some functions here duplicated with Tab
// all resizing functions should move here

impl<'a> PaneResizer<'a> {
    pub fn new(
        panes: &'a mut BTreeMap<PaneId, Box<dyn Pane>>,
        os_api: &'a mut Box<dyn ServerOsApi>,
    ) -> Self {
        PaneResizer { panes, os_api }
    }
    pub fn resize(
        &mut self,
        mut current_size: PositionAndSize,
        new_size: PositionAndSize,
    ) -> Option<(isize, isize)> {
        // (column_difference, row_difference)
        let mut successfully_resized = false;
        let mut column_difference: isize = 0;
        let mut row_difference: isize = 0;
        match new_size.cols.cmp(&current_size.cols) {
            Ordering::Greater => {
                let increase_by = new_size.cols - current_size.cols;
                if let Some(panes_to_resize) = find_increasable_vertical_chain(
                    self.panes,
                    increase_by,
                    current_size.cols,
                    current_size.rows,
                ) {
                    self.increase_panes_right_and_push_adjacents_right(
                        panes_to_resize,
                        increase_by,
                    );
                    column_difference = new_size.cols as isize - current_size.cols as isize;
                    current_size.cols = (current_size.cols as isize + column_difference) as usize;
                    successfully_resized = true;
                };
            }
            Ordering::Less => {
                let reduce_by = current_size.cols - new_size.cols;
                if let Some(panes_to_resize) = find_reducible_vertical_chain(
                    self.panes,
                    reduce_by,
                    current_size.cols,
                    current_size.rows,
                ) {
                    self.reduce_panes_left_and_pull_adjacents_left(panes_to_resize, reduce_by);
                    column_difference = new_size.cols as isize - current_size.cols as isize;
                    current_size.cols = (current_size.cols as isize + column_difference) as usize;
                    successfully_resized = true;
                };
            }
            Ordering::Equal => (),
        }
        match new_size.rows.cmp(&current_size.rows) {
            Ordering::Greater => {
                let increase_by = new_size.rows - current_size.rows;
                if let Some(panes_to_resize) = find_increasable_horizontal_chain(
                    self.panes,
                    increase_by,
                    current_size.cols,
                    current_size.rows,
                ) {
                    self.increase_panes_down_and_push_down_adjacents(panes_to_resize, increase_by);
                    row_difference = new_size.rows as isize - current_size.rows as isize;
                    current_size.rows = (current_size.rows as isize + row_difference) as usize;
                    successfully_resized = true;
                };
            }
            Ordering::Less => {
                let reduce_by = current_size.rows - new_size.rows;
                if let Some(panes_to_resize) = find_reducible_horizontal_chain(
                    self.panes,
                    reduce_by,
                    current_size.cols,
                    current_size.rows,
                ) {
                    self.reduce_panes_up_and_pull_adjacents_up(panes_to_resize, reduce_by);
                    row_difference = new_size.rows as isize - current_size.rows as isize;
                    current_size.rows = (current_size.rows as isize + row_difference) as usize;
                    successfully_resized = true;
                };
            }
            Ordering::Equal => (),
        }
        if successfully_resized {
            Some((column_difference, row_difference))
        } else {
            None
        }
    }
    fn reduce_panes_left_and_pull_adjacents_left(
        &mut self,
        panes_to_reduce: Vec<PaneId>,
        reduce_by: usize,
    ) {
        let mut pulled_panes: HashSet<PaneId> = HashSet::new();
        for pane_id in panes_to_reduce {
            let (pane_x, pane_y, pane_columns, pane_rows) = {
                let pane = self.panes.get(&pane_id).unwrap();
                (pane.x(), pane.y(), pane.columns(), pane.rows())
            };
            let panes_to_pull = self.panes.values_mut().filter(|p| {
                p.x() > pane_x + pane_columns
                    && (p.y() <= pane_y && p.y() + p.rows() >= pane_y
                        || p.y() >= pane_y && p.y() + p.rows() <= pane_y + pane_rows)
            });
            for pane in panes_to_pull {
                if !pulled_panes.contains(&pane.pid()) {
                    pane.pull_left(reduce_by);
                    pulled_panes.insert(pane.pid());
                }
            }
            self.reduce_pane_width_left(&pane_id, reduce_by);
        }
    }
    fn reduce_panes_up_and_pull_adjacents_up(
        &mut self,
        panes_to_reduce: Vec<PaneId>,
        reduce_by: usize,
    ) {
        let mut pulled_panes: HashSet<PaneId> = HashSet::new();
        for pane_id in panes_to_reduce {
            let (pane_x, pane_y, pane_columns, pane_rows) = {
                let pane = self.panes.get(&pane_id).unwrap();
                (pane.x(), pane.y(), pane.columns(), pane.rows())
            };
            let panes_to_pull = self.panes.values_mut().filter(|p| {
                p.y() > pane_y + pane_rows
                    && (p.x() <= pane_x && p.x() + p.columns() >= pane_x
                        || p.x() >= pane_x && p.x() + p.columns() <= pane_x + pane_columns)
            });
            for pane in panes_to_pull {
                if !pulled_panes.contains(&pane.pid()) {
                    pane.pull_up(reduce_by);
                    pulled_panes.insert(pane.pid());
                }
            }
            self.reduce_pane_height_up(&pane_id, reduce_by);
        }
    }
    fn increase_panes_down_and_push_down_adjacents(
        &mut self,
        panes_to_increase: Vec<PaneId>,
        increase_by: usize,
    ) {
        let mut pushed_panes: HashSet<PaneId> = HashSet::new();
        for pane_id in panes_to_increase {
            let (pane_x, pane_y, pane_columns, pane_rows) = {
                let pane = self.panes.get(&pane_id).unwrap();
                (pane.x(), pane.y(), pane.columns(), pane.rows())
            };
            let panes_to_push = self.panes.values_mut().filter(|p| {
                p.y() > pane_y + pane_rows
                    && (p.x() <= pane_x && p.x() + p.columns() >= pane_x
                        || p.x() >= pane_x && p.x() + p.columns() <= pane_x + pane_columns)
            });
            for pane in panes_to_push {
                if !pushed_panes.contains(&pane.pid()) {
                    pane.push_down(increase_by);
                    pushed_panes.insert(pane.pid());
                }
            }
            self.increase_pane_height_down(&pane_id, increase_by);
        }
    }
    fn increase_panes_right_and_push_adjacents_right(
        &mut self,
        panes_to_increase: Vec<PaneId>,
        increase_by: usize,
    ) {
        let mut pushed_panes: HashSet<PaneId> = HashSet::new();
        for pane_id in panes_to_increase {
            let (pane_x, pane_y, pane_columns, pane_rows) = {
                let pane = self.panes.get(&pane_id).unwrap();
                (pane.x(), pane.y(), pane.columns(), pane.rows())
            };
            let panes_to_push = self.panes.values_mut().filter(|p| {
                p.x() > pane_x + pane_columns
                    && (p.y() <= pane_y && p.y() + p.rows() >= pane_y
                        || p.y() >= pane_y && p.y() + p.rows() <= pane_y + pane_rows)
            });
            for pane in panes_to_push {
                if !pushed_panes.contains(&pane.pid()) {
                    pane.push_right(increase_by);
                    pushed_panes.insert(pane.pid());
                }
            }
            self.increase_pane_width_right(&pane_id, increase_by);
        }
    }
    fn reduce_pane_height_up(&mut self, id: &PaneId, count: usize) {
        let pane = self.panes.get_mut(id).unwrap();
        pane.reduce_height_up(count);
        if let PaneId::Terminal(pid) = id {
            self.os_api
                .set_terminal_size_using_fd(*pid, pane.columns() as u16, pane.rows() as u16);
        }
    }
    fn increase_pane_height_down(&mut self, id: &PaneId, count: usize) {
        let pane = self.panes.get_mut(id).unwrap();
        pane.increase_height_down(count);
        if let PaneId::Terminal(pid) = pane.pid() {
            self.os_api
                .set_terminal_size_using_fd(pid, pane.columns() as u16, pane.rows() as u16);
        }
    }
    fn increase_pane_width_right(&mut self, id: &PaneId, count: usize) {
        let pane = self.panes.get_mut(id).unwrap();
        pane.increase_width_right(count);
        if let PaneId::Terminal(pid) = pane.pid() {
            self.os_api
                .set_terminal_size_using_fd(pid, pane.columns() as u16, pane.rows() as u16);
        }
    }
    fn reduce_pane_width_left(&mut self, id: &PaneId, count: usize) {
        let pane = self.panes.get_mut(id).unwrap();
        pane.reduce_width_left(count);
        if let PaneId::Terminal(pid) = pane.pid() {
            self.os_api
                .set_terminal_size_using_fd(pid, pane.columns() as u16, pane.rows() as u16);
        }
    }
}

fn find_next_increasable_horizontal_pane(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    right_of: &dyn Pane,
    increase_by: usize,
) -> Option<PaneId> {
    let next_pane_candidates = panes.values().filter(
        |p| {
            p.x() == right_of.x() + right_of.columns() + 1 && p.horizontally_overlaps_with(right_of)
        }, // TODO: the name here is wrong, it should be vertically_overlaps_with
    );
    let resizable_candidates =
        next_pane_candidates.filter(|p| p.can_increase_height_by(increase_by));
    resizable_candidates.fold(None, |next_pane_id, p| match next_pane_id {
        Some(next_pane) => {
            let next_pane = panes.get(&next_pane).unwrap();
            if next_pane.y() < p.y() {
                next_pane_id
            } else {
                Some(p.pid())
            }
        }
        None => Some(p.pid()),
    })
}

fn find_next_increasable_vertical_pane(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    below: &dyn Pane,
    increase_by: usize,
) -> Option<PaneId> {
    let next_pane_candidates = panes.values().filter(
        |p| p.y() == below.y() + below.rows() + 1 && p.vertically_overlaps_with(below), // TODO: the name here is wrong, it should be horizontally_overlaps_with
    );
    let resizable_candidates =
        next_pane_candidates.filter(|p| p.can_increase_width_by(increase_by));
    resizable_candidates.fold(None, |next_pane_id, p| match next_pane_id {
        Some(next_pane) => {
            let next_pane = panes.get(&next_pane).unwrap();
            if next_pane.x() < p.x() {
                next_pane_id
            } else {
                Some(p.pid())
            }
        }
        None => Some(p.pid()),
    })
}

fn find_next_reducible_vertical_pane(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    below: &dyn Pane,
    reduce_by: usize,
) -> Option<PaneId> {
    let next_pane_candidates = panes.values().filter(
        |p| p.y() == below.y() + below.rows() + 1 && p.vertically_overlaps_with(below), // TODO: the name here is wrong, it should be horizontally_overlaps_with
    );
    let resizable_candidates = next_pane_candidates.filter(|p| p.can_reduce_width_by(reduce_by));
    resizable_candidates.fold(None, |next_pane_id, p| match next_pane_id {
        Some(next_pane) => {
            let next_pane = panes.get(&next_pane).unwrap();
            if next_pane.x() < p.x() {
                next_pane_id
            } else {
                Some(p.pid())
            }
        }
        None => Some(p.pid()),
    })
}

fn find_next_reducible_horizontal_pane(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    right_of: &dyn Pane,
    reduce_by: usize,
) -> Option<PaneId> {
    let next_pane_candidates = panes.values().filter(
        |p| {
            p.x() == right_of.x() + right_of.columns() + 1 && p.horizontally_overlaps_with(right_of)
        }, // TODO: the name here is wrong, it should be vertically_overlaps_with
    );
    let resizable_candidates = next_pane_candidates.filter(|p| p.can_reduce_height_by(reduce_by));
    resizable_candidates.fold(None, |next_pane_id, p| match next_pane_id {
        Some(next_pane) => {
            let next_pane = panes.get(&next_pane).unwrap();
            if next_pane.y() < p.y() {
                next_pane_id
            } else {
                Some(p.pid())
            }
        }
        None => Some(p.pid()),
    })
}

fn find_increasable_horizontal_chain(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    increase_by: usize,
    screen_width: usize,
    screen_height: usize, // TODO: this is the previous size (make this clearer)
) -> Option<Vec<PaneId>> {
    let mut horizontal_coordinate = 0;
    loop {
        if horizontal_coordinate == screen_height {
            return None;
        }

        match panes
            .values()
            .find(|p| p.x() == 0 && p.y() == horizontal_coordinate)
        {
            Some(leftmost_pane) => {
                if !leftmost_pane.can_increase_height_by(increase_by) {
                    horizontal_coordinate = leftmost_pane.y() + leftmost_pane.rows() + 1;
                    continue;
                }
                let mut panes_to_resize = vec![];
                let mut current_pane = leftmost_pane;
                loop {
                    panes_to_resize.push(current_pane.pid());
                    if current_pane.x() + current_pane.columns() == screen_width {
                        return Some(panes_to_resize);
                    }
                    match find_next_increasable_horizontal_pane(
                        panes,
                        current_pane.as_ref(),
                        increase_by,
                    ) {
                        Some(next_pane_id) => {
                            current_pane = panes.get(&next_pane_id).unwrap();
                        }
                        None => {
                            horizontal_coordinate = leftmost_pane.y() + leftmost_pane.rows() + 1;
                            break;
                        }
                    };
                }
            }
            None => {
                return None;
            }
        }
    }
}

fn find_increasable_vertical_chain(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    increase_by: usize,
    screen_width: usize,
    screen_height: usize, // TODO: this is the previous size (make this clearer)
) -> Option<Vec<PaneId>> {
    let mut vertical_coordinate = 0;
    loop {
        if vertical_coordinate == screen_width {
            return None;
        }

        match panes
            .values()
            .find(|p| p.y() == 0 && p.x() == vertical_coordinate)
        {
            Some(topmost_pane) => {
                if !topmost_pane.can_increase_width_by(increase_by) {
                    vertical_coordinate = topmost_pane.x() + topmost_pane.columns() + 1;
                    continue;
                }
                let mut panes_to_resize = vec![];
                let mut current_pane = topmost_pane;
                loop {
                    panes_to_resize.push(current_pane.pid());
                    if current_pane.y() + current_pane.rows() == screen_height {
                        return Some(panes_to_resize);
                    }
                    match find_next_increasable_vertical_pane(
                        panes,
                        current_pane.as_ref(),
                        increase_by,
                    ) {
                        Some(next_pane_id) => {
                            current_pane = panes.get(&next_pane_id).unwrap();
                        }
                        None => {
                            vertical_coordinate = topmost_pane.x() + topmost_pane.columns() + 1;
                            break;
                        }
                    };
                }
            }
            None => {
                return None;
            }
        }
    }
}

fn find_reducible_horizontal_chain(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    reduce_by: usize,
    screen_width: usize,
    screen_height: usize, // TODO: this is the previous size (make this clearer)
) -> Option<Vec<PaneId>> {
    let mut horizontal_coordinate = 0;
    loop {
        if horizontal_coordinate == screen_height {
            return None;
        }

        match panes
            .values()
            .find(|p| p.x() == 0 && p.y() == horizontal_coordinate)
        {
            Some(leftmost_pane) => {
                if !leftmost_pane.can_reduce_height_by(reduce_by) {
                    horizontal_coordinate = leftmost_pane.y() + leftmost_pane.rows() + 1;
                    continue;
                }
                let mut panes_to_resize = vec![];
                let mut current_pane = leftmost_pane;
                loop {
                    panes_to_resize.push(current_pane.pid());
                    if current_pane.x() + current_pane.columns() == screen_width {
                        return Some(panes_to_resize);
                    }
                    match find_next_reducible_horizontal_pane(
                        panes,
                        current_pane.as_ref(),
                        reduce_by,
                    ) {
                        Some(next_pane_id) => {
                            current_pane = panes.get(&next_pane_id).unwrap();
                        }
                        None => {
                            horizontal_coordinate = leftmost_pane.y() + leftmost_pane.rows() + 1;
                            break;
                        }
                    };
                }
            }
            None => {
                return None;
            }
        }
    }
}

fn find_reducible_vertical_chain(
    panes: &BTreeMap<PaneId, Box<dyn Pane>>,
    increase_by: usize,
    screen_width: usize,
    screen_height: usize, // TODO: this is the previous size (make this clearer)
) -> Option<Vec<PaneId>> {
    let mut vertical_coordinate = 0;
    loop {
        if vertical_coordinate == screen_width {
            return None;
        }

        match panes
            .values()
            .find(|p| p.y() == 0 && p.x() == vertical_coordinate)
        {
            Some(topmost_pane) => {
                if !topmost_pane.can_reduce_width_by(increase_by) {
                    vertical_coordinate = topmost_pane.x() + topmost_pane.columns() + 1;
                    continue;
                }
                let mut panes_to_resize = vec![];
                let mut current_pane = topmost_pane;
                loop {
                    panes_to_resize.push(current_pane.pid());
                    if current_pane.y() + current_pane.rows() == screen_height {
                        return Some(panes_to_resize);
                    }
                    match find_next_reducible_vertical_pane(
                        panes,
                        current_pane.as_ref(),
                        increase_by,
                    ) {
                        Some(next_pane_id) => {
                            current_pane = panes.get(&next_pane_id).unwrap();
                        }
                        None => {
                            vertical_coordinate = topmost_pane.x() + topmost_pane.columns() + 1;
                            break;
                        }
                    };
                }
            }
            None => {
                return None;
            }
        }
    }
}
