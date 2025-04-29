use crate::{
    panes::PaneId,
    tab::{Pane, MIN_TERMINAL_HEIGHT},
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use zellij_utils::{
    errors::prelude::*,
    pane_size::{Dimension, PaneGeom},
};

pub struct StackedPanes<'a> {
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
}

impl<'a> StackedPanes<'a> {
    pub fn new(panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>) -> Self {
        StackedPanes { panes }
    }
    pub fn new_from_btreemap(
        panes: impl IntoIterator<Item = (&'a PaneId, &'a mut Box<dyn Pane>)>,
        panes_to_hide: &HashSet<PaneId>,
    ) -> Self {
        let panes: HashMap<_, _> = panes
            .into_iter()
            .filter(|(p_id, _)| !panes_to_hide.contains(p_id))
            .map(|(p_id, p)| (*p_id, p))
            .collect();
        let panes = Rc::new(RefCell::new(panes));
        StackedPanes { panes }
    }
    pub fn move_down(
        &mut self,
        source_pane_id: &PaneId,
        destination_pane_id: &PaneId,
    ) -> Result<()> {
        let err_context = || format!("Failed to move stacked pane focus down");
        let source_pane_stack_id = self
            .panes
            .borrow()
            .get(source_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .stacked;
        let destination_pane_stack_id = self
            .panes
            .borrow()
            .get(destination_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .stacked;
        if source_pane_stack_id == destination_pane_stack_id {
            let mut panes = self.panes.borrow_mut();
            let source_pane = panes.get_mut(source_pane_id).with_context(err_context)?;
            let mut source_pane_geom = source_pane.position_and_size();
            let mut destination_pane_geom = source_pane_geom.clone();
            destination_pane_geom.y = source_pane_geom.y + 1;
            source_pane_geom.rows = Dimension::fixed(1);
            source_pane.set_geom(source_pane_geom);
            let destination_pane = panes
                .get_mut(&destination_pane_id)
                .with_context(err_context)?;
            destination_pane.set_geom(destination_pane_geom);
        } else if destination_pane_stack_id.is_some() {
            // we're moving down to the highest pane in the stack, we need to expand it and shrink the
            // expanded stack pane
            self.make_highest_pane_in_stack_flexible(destination_pane_id)?;
        }
        Ok(())
    }
    pub fn move_up(&mut self, source_pane_id: &PaneId, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to move stacked pane focus up");
        let source_pane_stack_id = self
            .panes
            .borrow()
            .get(source_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .stacked;
        let destination_pane_stack_id = self
            .panes
            .borrow()
            .get(destination_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .stacked;
        if source_pane_stack_id == destination_pane_stack_id {
            let mut panes = self.panes.borrow_mut();
            let source_pane = panes.get_mut(source_pane_id).with_context(err_context)?;
            let mut source_pane_geom = source_pane.position_and_size();
            let mut destination_pane_geom = source_pane_geom.clone();
            source_pane_geom.y = (source_pane_geom.y + source_pane_geom.rows.as_usize()) - 1; // -1 because we want to be at the last line of the source pane, not the next line over
            source_pane_geom.rows = Dimension::fixed(1);
            source_pane.set_geom(source_pane_geom);
            destination_pane_geom.y -= 1;
            let destination_pane = panes
                .get_mut(&destination_pane_id)
                .with_context(err_context)?;
            destination_pane.set_geom(destination_pane_geom);
        } else if destination_pane_stack_id.is_some() {
            // we're moving up to the lowest pane in the stack, we need to expand it and shrink the
            // expanded stack pane
            self.make_lowest_pane_in_stack_flexible(destination_pane_id)?;
        }
        Ok(())
    }
    pub fn expand_pane(&mut self, pane_id: &PaneId) -> Result<Vec<PaneId>> {
        // returns all the pane ids in the stack
        let err_context = || format!("Failed to focus stacked pane");
        let all_stacked_pane_positions =
            self.positions_in_stack(pane_id).with_context(err_context)?;

        let position_of_flexible_pane = self
            .position_of_flexible_pane(&all_stacked_pane_positions)
            .with_context(err_context)?;
        let (flexible_pane_id, mut flexible_pane) = *all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .with_context(err_context)?;
        if flexible_pane_id != *pane_id {
            let mut panes = self.panes.borrow_mut();
            let height_of_flexible_pane = all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .map(|(_pid, p)| p.rows)
                .with_context(err_context)?;
            let position_of_pane_to_focus = all_stacked_pane_positions
                .iter()
                .position(|(pid, _p)| pid == pane_id)
                .with_context(err_context)?;
            let (_, mut pane_to_focus) = *all_stacked_pane_positions
                .iter()
                .nth(position_of_pane_to_focus)
                .with_context(err_context)?;
            pane_to_focus.rows = height_of_flexible_pane;
            panes
                .get_mut(pane_id)
                .with_context(err_context)?
                .set_geom(pane_to_focus);
            flexible_pane.rows = Dimension::fixed(1);
            panes
                .get_mut(&flexible_pane_id)
                .with_context(err_context)?
                .set_geom(flexible_pane);

            for (i, (pid, _position)) in all_stacked_pane_positions.iter().enumerate() {
                if i > position_of_pane_to_focus && i <= position_of_flexible_pane {
                    // the flexible pane has moved up the stack, we need to push this pane down
                    let pane = panes.get_mut(pid).with_context(err_context)?;
                    let mut pane_position_and_size = pane.position_and_size();
                    pane_position_and_size.y += height_of_flexible_pane.as_usize() - 1;
                    pane.set_geom(pane_position_and_size);
                } else if i > position_of_flexible_pane && i <= position_of_pane_to_focus {
                    // the flexible pane has moved down the stack, we need to pull this pane up
                    let pane = panes.get_mut(pid).with_context(err_context)?;
                    let mut pane_position_and_size = pane.position_and_size();
                    pane_position_and_size.y -= height_of_flexible_pane.as_usize() - 1;
                    pane.set_geom(pane_position_and_size);
                }
            }
        }
        Ok(all_stacked_pane_positions
            .iter()
            .map(|(pane_id, _pane_position)| *pane_id)
            .collect())
    }
    pub fn flexible_pane_id_in_stack(&self, pane_id_in_stack: &PaneId) -> Option<PaneId> {
        let all_stacked_pane_positions = self.positions_in_stack(pane_id_in_stack).ok()?;
        all_stacked_pane_positions
            .iter()
            .find(|(_pid, p)| p.rows.is_percent())
            .map(|(pid, _p)| *pid)
    }
    pub fn position_and_size_of_stack(&self, id: &PaneId) -> Option<PaneGeom> {
        let all_stacked_pane_positions = self.positions_in_stack(id).ok()?;
        let position_of_flexible_pane = self
            .position_of_flexible_pane(&all_stacked_pane_positions)
            .ok()?;
        let (_flexible_pane_id, flexible_pane) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)?;
        let (_, first_pane_in_stack) = all_stacked_pane_positions.first()?;
        let (_, last_pane_in_stack) = all_stacked_pane_positions.last()?;
        let mut rows = flexible_pane.rows;
        rows.set_inner(
            (last_pane_in_stack.y - first_pane_in_stack.y) + last_pane_in_stack.rows.as_usize(),
        );
        Some(PaneGeom {
            y: first_pane_in_stack.y,
            x: first_pane_in_stack.x,
            cols: first_pane_in_stack.cols,
            rows,
            stacked: None, // important because otherwise the minimum stack size will not be
            // respected
            ..Default::default()
        })
    }
    pub fn increase_stack_width(&mut self, id: &PaneId, percent: f64) -> Result<()> {
        let err_context = || format!("Failed to resize panes in stack");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        for (pane_id, _pane_position) in all_stacked_pane_positions {
            self.panes
                .borrow_mut()
                .get_mut(&pane_id)
                .with_context(err_context)?
                .increase_width(percent);
        }
        Ok(())
    }
    pub fn reduce_stack_width(&mut self, id: &PaneId, percent: f64) -> Result<()> {
        let err_context = || format!("Failed to resize panes in stack");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        for (pane_id, _pane_position) in all_stacked_pane_positions {
            self.panes
                .borrow_mut()
                .get_mut(&pane_id)
                .with_context(err_context)?
                .reduce_width(percent);
        }
        Ok(())
    }
    pub fn increase_stack_height(&mut self, id: &PaneId, percent: f64) -> Result<()> {
        let err_context = || format!("Failed to increase_stack_height");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        let position_of_flexible_pane = self
            .position_of_flexible_pane(&all_stacked_pane_positions)
            .with_context(err_context)?;
        let (flexible_pane_id, _flexible_pane) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .with_context(err_context)?;
        self.panes
            .borrow_mut()
            .get_mut(flexible_pane_id)
            .with_context(err_context)?
            .increase_height(percent);
        Ok(())
    }
    pub fn reduce_stack_height(&mut self, id: &PaneId, percent: f64) -> Result<()> {
        let err_context = || format!("Failed to increase_stack_height");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        let position_of_flexible_pane = self
            .position_of_flexible_pane(&all_stacked_pane_positions)
            .with_context(err_context)?;
        let (flexible_pane_id, _flexible_pane) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .with_context(err_context)?;
        self.panes
            .borrow_mut()
            .get_mut(flexible_pane_id)
            .with_context(err_context)?
            .reduce_height(percent);
        Ok(())
    }
    pub fn min_stack_height(&mut self, id: &PaneId) -> Result<usize> {
        let err_context = || format!("Failed to increase_stack_height");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        Ok(all_stacked_pane_positions.len())
    }
    pub fn resize_panes_in_stack(
        &mut self,
        id: &PaneId,
        new_full_stack_geom: PaneGeom,
    ) -> Result<()> {
        let err_context = || format!("Failed to resize panes in stack");
        let all_stacked_pane_positions = self.positions_in_stack(id).with_context(err_context)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        let (_flexible_pane_id, flexible_pane) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .with_context(err_context)?;
        let new_rows = new_full_stack_geom.rows.as_usize();

        let adjust_stack_geoms = |new_flexible_pane_geom: PaneGeom| -> Result<()> {
            let new_flexible_pane_geom_rows = new_flexible_pane_geom.rows.as_usize();
            for (i, (pane_id, pane_geom)) in all_stacked_pane_positions.iter().enumerate() {
                let mut new_pane_geom = if i == position_of_flexible_pane {
                    new_flexible_pane_geom
                } else {
                    *pane_geom
                };
                new_pane_geom.x = new_full_stack_geom.x;
                new_pane_geom.cols = new_full_stack_geom.cols;
                if i <= position_of_flexible_pane {
                    new_pane_geom.y = new_full_stack_geom.y + i;
                } else {
                    new_pane_geom.y = new_full_stack_geom.y + i + (new_flexible_pane_geom_rows - 1);
                }
                self.panes
                    .borrow_mut()
                    .get_mut(&pane_id)
                    .with_context(err_context)?
                    .set_geom(new_pane_geom);
            }
            Ok(())
        };
        let new_rows_for_flexible_pane =
            new_rows.saturating_sub(all_stacked_pane_positions.len()) + 1;
        let mut new_flexible_pane_geom = new_full_stack_geom;
        new_flexible_pane_geom.stacked = flexible_pane.stacked;
        new_flexible_pane_geom.logical_position = flexible_pane.logical_position;
        new_flexible_pane_geom
            .rows
            .set_inner(new_rows_for_flexible_pane);
        adjust_stack_geoms(new_flexible_pane_geom)?;
        Ok(())
    }
    fn pane_is_one_liner(&self, id: &PaneId) -> Result<bool> {
        let err_context = || format!("Cannot determine if pane is one liner or not");
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        Ok(pane_to_close.position_and_size().rows.is_fixed())
    }
    fn positions_in_stack(&self, id: &PaneId) -> Result<Vec<(PaneId, PaneGeom)>> {
        // find the full stack of panes around the given id, sorted by pane location top to bottom
        let err_context = || format!("Failed to find stacked panes");
        let panes = self.panes.borrow();
        let pane_in_stack = panes.get(id).with_context(err_context)?;
        let stack_id = pane_in_stack.position_and_size().stacked;
        let mut all_stacked_pane_positions: Vec<(PaneId, PaneGeom)> = panes
            .iter()
            .filter(|(_pid, p)| {
                p.position_and_size().is_stacked() && p.position_and_size().stacked == stack_id
            })
            .filter(|(_pid, p)| {
                p.position_and_size().x == pane_in_stack.position_and_size().x
                    && p.position_and_size().cols == pane_in_stack.position_and_size().cols
            })
            .map(|(pid, p)| (*pid, p.position_and_size()))
            .collect();
        all_stacked_pane_positions.sort_by(|(_a_pid, a), (_b_pid, b)| a.y.cmp(&b.y));
        Ok(all_stacked_pane_positions)
    }
    fn position_of_current_and_flexible_pane(
        &self,
        current_pane_id: &PaneId,
    ) -> Result<(usize, usize)> {
        // (current_pane, flexible_pane)
        let err_context = || format!("Failed to position_of_current_and_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(current_pane_id)?;
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(current_pane_id).with_context(err_context)?;
        let position_of_current_pane =
            self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        Ok((position_of_current_pane, position_of_flexible_pane))
    }
    fn position_of_current_pane(
        &self,
        all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>,
        pane_to_close: &Box<dyn Pane>,
    ) -> Result<usize> {
        let err_context = || format!("Failed to find position of current pane");
        all_stacked_pane_positions
            .iter()
            .position(|(pid, _p)| pid == &pane_to_close.pid())
            .with_context(err_context)
    }
    fn position_of_flexible_pane(
        &self,
        all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>,
    ) -> Result<usize> {
        let err_context = || format!("Failed to find position of flexible pane");
        all_stacked_pane_positions
            .iter()
            .position(|(_pid, p)| p.rows.is_percent())
            .with_context(err_context)
    }
    pub fn fill_space_over_pane_in_stack(&mut self, id: &PaneId) -> Result<bool> {
        if self.pane_is_one_liner(id)? {
            self.fill_space_over_one_liner_pane(id)
        } else {
            self.fill_space_over_visible_stacked_pane(id)
        }
    }
    pub fn stacked_pane_ids_under_and_over_flexible_panes(
        &self,
    ) -> Result<(HashSet<PaneId>, HashSet<PaneId>)> {
        let mut stacked_pane_ids_under_flexible_panes = HashSet::new();
        let mut stacked_pane_ids_over_flexible_panes = HashSet::new();
        let mut seen = HashSet::new();
        let pane_ids_in_stacks: Vec<PaneId> = {
            self.panes
                .borrow()
                .iter()
                .filter(|(_p_id, p)| p.position_and_size().is_stacked())
                .map(|(p_id, _p)| *p_id)
                .collect()
        };
        for pane_id in pane_ids_in_stacks {
            if !seen.contains(&pane_id) {
                let mut current_pane_is_above_stack = true;
                let positions_in_stack = self.positions_in_stack(&pane_id)?;
                for (pane_id, pane_geom) in positions_in_stack {
                    seen.insert(pane_id);
                    if pane_geom.rows.is_percent() {
                        // this is the flexible pane
                        current_pane_is_above_stack = false;
                        continue;
                    }
                    if current_pane_is_above_stack {
                        stacked_pane_ids_over_flexible_panes.insert(pane_id);
                    } else {
                        stacked_pane_ids_under_flexible_panes.insert(pane_id);
                    }
                }
                seen.insert(pane_id);
            }
        }
        Ok((
            stacked_pane_ids_under_flexible_panes,
            stacked_pane_ids_over_flexible_panes,
        ))
    }
    pub fn stacked_pane_ids_on_top_and_bottom_of_stacks(
        &self,
    ) -> Result<(HashSet<PaneId>, HashSet<PaneId>)> {
        let mut stacked_pane_ids_on_top_of_stacks = HashSet::new();
        let mut stacked_pane_ids_on_bottom_of_stacks = HashSet::new();
        let all_stacks = self.get_all_stacks()?;
        for stack in all_stacks {
            if let Some((first_pane_id, _pane)) = stack.iter().next() {
                stacked_pane_ids_on_top_of_stacks.insert(*first_pane_id);
            }
            if let Some((last_pane_id, _pane)) = stack.iter().last() {
                stacked_pane_ids_on_bottom_of_stacks.insert(*last_pane_id);
            }
        }
        Ok((
            stacked_pane_ids_on_top_of_stacks,
            stacked_pane_ids_on_bottom_of_stacks,
        ))
    }
    pub fn make_room_for_new_pane(&mut self) -> Result<PaneGeom> {
        let err_context = || format!("Failed to add pane to stack");
        let all_stacks = self.get_all_stacks()?;
        for stack in all_stacks {
            if let Some((id_of_flexible_pane_in_stack, _flexible_pane_in_stack)) = stack
                .iter()
                .find(|(_p_id, p)| !p.rows.is_fixed() && p.rows.as_usize() > MIN_TERMINAL_HEIGHT)
            {
                self.make_lowest_pane_in_stack_flexible(id_of_flexible_pane_in_stack)?;
                let all_stacked_pane_positions =
                    self.positions_in_stack(id_of_flexible_pane_in_stack)?;
                let position_of_flexible_pane =
                    self.position_of_flexible_pane(&all_stacked_pane_positions)?;
                let (flexible_pane_id, mut flexible_pane_geom) = *all_stacked_pane_positions
                    .iter()
                    .nth(position_of_flexible_pane)
                    .with_context(err_context)?;
                let mut position_for_new_pane = flexible_pane_geom.clone();
                position_for_new_pane
                    .rows
                    .set_inner(position_for_new_pane.rows.as_usize() - 1);
                position_for_new_pane.y = position_for_new_pane.y + 1;
                flexible_pane_geom.rows = Dimension::fixed(1);
                self.panes
                    .borrow_mut()
                    .get_mut(&flexible_pane_id)
                    .with_context(err_context)?
                    .set_geom(flexible_pane_geom);
                return Ok(position_for_new_pane);
            }
        }
        Err(anyhow!("Not enough room for another pane!"))
    }
    pub fn make_room_for_new_pane_in_stack(&mut self, pane_id: &PaneId) -> Result<PaneGeom> {
        let err_context = || format!("Failed to add pane to stack");

        let stack = self.positions_in_stack(pane_id).with_context(err_context)?;
        if let Some((id_of_flexible_pane_in_stack, _flexible_pane_in_stack)) = stack
            .iter()
            .find(|(_p_id, p)| !p.rows.is_fixed() && p.rows.as_usize() > MIN_TERMINAL_HEIGHT)
        {
            self.make_lowest_pane_in_stack_flexible(id_of_flexible_pane_in_stack)?;
            let all_stacked_pane_positions =
                self.positions_in_stack(id_of_flexible_pane_in_stack)?;
            let position_of_flexible_pane =
                self.position_of_flexible_pane(&all_stacked_pane_positions)?;
            let (flexible_pane_id, mut flexible_pane_geom) = *all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .with_context(err_context)?;
            let mut position_for_new_pane = flexible_pane_geom.clone();
            position_for_new_pane
                .rows
                .set_inner(position_for_new_pane.rows.as_usize() - 1);
            position_for_new_pane.y = position_for_new_pane.y + 1;
            flexible_pane_geom.rows = Dimension::fixed(1);
            self.panes
                .borrow_mut()
                .get_mut(&flexible_pane_id)
                .with_context(err_context)?
                .set_geom(flexible_pane_geom);
            return Ok(position_for_new_pane);
        }
        Err(anyhow!("Not enough room for another pane!"))
    }
    pub fn room_left_in_stack_of_pane_id(&self, pane_id: &PaneId) -> Option<usize> {
        // if the pane is stacked, returns the number of panes possible to add to this stack
        let Ok(stack) = self.positions_in_stack(pane_id) else {
            return None;
        };
        stack.iter().find_map(|(_p_id, p)| {
            if !p.rows.is_fixed() {
                // this is the flexible pane
                Some(p.rows.as_usize().saturating_sub(MIN_TERMINAL_HEIGHT))
            } else {
                None
            }
        })
    }
    pub fn new_stack(&self, root_pane_id: PaneId, pane_count_in_stack: usize) -> Vec<PaneGeom> {
        let mut stacked_geoms = vec![];
        let panes = self.panes.borrow();
        let running_stack_geom = panes.get(&root_pane_id).map(|p| p.position_and_size());
        let Some(mut running_stack_geom) = running_stack_geom else {
            log::error!("Pane not found"); // TODO: better error
            return stacked_geoms;
        };
        let stack_id = self.next_stack_id();
        running_stack_geom.stacked = Some(stack_id);
        let mut pane_index_in_stack = 0;
        loop {
            if pane_index_in_stack == pane_count_in_stack {
                break;
            }
            let is_last_pane_in_stack =
                pane_index_in_stack == pane_count_in_stack.saturating_sub(1);
            let mut geom_for_pane = running_stack_geom.clone();
            if !is_last_pane_in_stack {
                geom_for_pane.rows = Dimension::fixed(1);
                running_stack_geom.y += 1;
                running_stack_geom
                    .rows
                    .set_inner(running_stack_geom.rows.as_usize().saturating_sub(1));
            }
            stacked_geoms.push(geom_for_pane);
            pane_index_in_stack += 1;
        }
        stacked_geoms
    }
    fn extract_geoms_from_stack(
        &self,
        root_pane_id: PaneId,
    ) -> Option<(PaneGeom, Vec<(PaneId, PaneGeom)>)> {
        let panes = self.panes.borrow();
        let mut geom_of_main_pane = panes.get(&root_pane_id).map(|p| p.position_and_size())?;
        let mut extra_stacked_geoms_of_main_pane = vec![];
        if geom_of_main_pane.is_stacked() {
            let other_panes_in_stack = self.positions_in_stack(&root_pane_id).ok()?;
            for other_pane in other_panes_in_stack {
                if other_pane.0 != root_pane_id {
                    // so it is not duplicated
                    extra_stacked_geoms_of_main_pane.push(other_pane);
                }
            }
            let logical_position = geom_of_main_pane.logical_position;
            geom_of_main_pane = self.position_and_size_of_stack(&root_pane_id)?;
            geom_of_main_pane.logical_position = logical_position;
        }
        Some((geom_of_main_pane, extra_stacked_geoms_of_main_pane))
    }
    fn positions_of_panes_and_their_stacks(
        &self,
        pane_ids: Vec<PaneId>,
    ) -> Option<Vec<(PaneId, PaneGeom)>> {
        let mut positions = vec![];
        let panes = self.panes.borrow();
        for pane_id in &pane_ids {
            let geom_of_pane = panes.get(pane_id).map(|p| p.position_and_size())?;
            if geom_of_pane.is_stacked() {
                let mut other_panes_in_stack = self.positions_in_stack(pane_id).ok()?;
                positions.append(&mut other_panes_in_stack);
            } else {
                positions.push((*pane_id, geom_of_pane));
            }
        }
        Some(positions)
    }

    fn combine_geoms_horizontally(
        &self,
        pane_ids_and_geoms: &Vec<(PaneId, PaneGeom)>,
    ) -> Option<PaneGeom> {
        let mut geoms_to_combine = HashSet::new();
        for (other_pane_id, other_geom) in pane_ids_and_geoms {
            if other_geom.is_stacked() {
                geoms_to_combine.insert(self.position_and_size_of_stack(other_pane_id)?);
            } else {
                geoms_to_combine.insert(*other_geom);
            }
        }
        let mut geoms_to_combine: Vec<PaneGeom> = geoms_to_combine.iter().copied().collect();
        geoms_to_combine.sort_by(|a_geom, b_geom| a_geom.x.cmp(&b_geom.x));

        let geom_to_combine = geoms_to_combine.get(0)?;
        geom_to_combine
            .combine_horizontally_with_many(&geoms_to_combine.iter().copied().skip(1).collect())
    }
    fn combine_geoms_vertically(
        &self,
        pane_ids_and_geoms: &Vec<(PaneId, PaneGeom)>,
    ) -> Option<PaneGeom> {
        let mut geoms_to_combine = HashSet::new();
        for (other_pane_id, other_geom) in pane_ids_and_geoms {
            if other_geom.is_stacked() {
                geoms_to_combine.insert(self.position_and_size_of_stack(other_pane_id)?);
            } else {
                geoms_to_combine.insert(*other_geom);
            }
        }
        let mut geoms_to_combine: Vec<PaneGeom> = geoms_to_combine.iter().copied().collect();

        geoms_to_combine.sort_by(|a_geom, b_geom| a_geom.y.cmp(&b_geom.y));
        let geom_to_combine = geoms_to_combine.get(0)?;
        geom_to_combine
            .combine_vertically_with_many(&geoms_to_combine.iter().copied().skip(1).collect())
    }
    pub fn combine_vertically_aligned_panes_to_stack(
        &mut self,
        root_pane_id: &PaneId,
        neighboring_pane_ids: Vec<PaneId>,
    ) -> Result<()> {
        let (geom_of_main_pane, mut extra_stacked_geoms_of_main_pane) = self
            .extract_geoms_from_stack(*root_pane_id)
            .ok_or_else(|| anyhow!("Failed to extract geoms from stack"))?;
        let mut other_pane_ids_and_geoms = self
            .positions_of_panes_and_their_stacks(neighboring_pane_ids)
            .ok_or_else(|| anyhow!("Failed to get pane geoms"))?;
        if other_pane_ids_and_geoms.is_empty() {
            // nothing to do
            return Ok(());
        };
        let Some(geom_to_combine) = self.combine_geoms_horizontally(&other_pane_ids_and_geoms)
        else {
            log::error!("Failed to combine geoms horizontally");
            return Ok(());
        };
        let new_stack_geom = if geom_to_combine.y < geom_of_main_pane.y {
            geom_to_combine.combine_vertically_with(&geom_of_main_pane)
        } else {
            geom_of_main_pane.combine_vertically_with(&geom_to_combine)
        };
        let Some(new_stack_geom) = new_stack_geom else {
            // nothing to do, likely the pane below is fixed
            return Ok(());
        };
        let stack_id = self.next_stack_id();
        // we add the extra panes in the original stack (if any) so that they will be assigned pane
        // positions but not affect the stack geometry
        other_pane_ids_and_geoms.append(&mut extra_stacked_geoms_of_main_pane);
        let mut panes = self.panes.borrow_mut();
        let mut running_y = new_stack_geom.y;
        let mut geom_of_flexible_pane = new_stack_geom.clone();
        geom_of_flexible_pane
            .rows
            .decrease_inner(other_pane_ids_and_geoms.len());
        geom_of_flexible_pane.stacked = Some(stack_id);
        let mut all_stack_geoms = other_pane_ids_and_geoms;
        let original_geom_of_main_pane = panes
            .get(&root_pane_id)
            .ok_or_else(|| anyhow!("Failed to find root geom"))?
            .position_and_size(); // for sorting purposes
        all_stack_geoms.push((*root_pane_id, original_geom_of_main_pane));
        all_stack_geoms.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| {
            if a_geom.y == b_geom.y {
                a_geom.x.cmp(&b_geom.x)
            } else {
                a_geom.y.cmp(&b_geom.y)
            }
        });
        for (pane_id, mut pane_geom) in all_stack_geoms {
            if let Some(pane_in_stack) = panes.get_mut(&pane_id) {
                if &pane_id == root_pane_id {
                    pane_geom.x = new_stack_geom.x;
                    pane_geom.cols = new_stack_geom.cols;
                    pane_geom.y = running_y;
                    pane_geom.rows = geom_of_flexible_pane.rows;
                    pane_geom.stacked = Some(stack_id);
                    running_y += geom_of_flexible_pane.rows.as_usize();
                    pane_in_stack.set_geom(pane_geom);
                } else {
                    pane_geom.x = new_stack_geom.x;
                    pane_geom.cols = new_stack_geom.cols;
                    pane_geom.y = running_y;
                    pane_geom.rows = Dimension::fixed(1);
                    pane_geom.stacked = Some(stack_id);
                    running_y += 1;
                    pane_in_stack.set_geom(pane_geom);
                }
            }
        }
        Ok(())
    }
    pub fn combine_horizontally_aligned_panes_to_stack(
        &mut self,
        root_pane_id: &PaneId,
        neighboring_pane_ids: Vec<PaneId>,
    ) -> Result<()> {
        let (geom_of_main_pane, mut extra_stacked_geoms_of_main_pane) = self
            .extract_geoms_from_stack(*root_pane_id)
            .ok_or_else(|| anyhow!("Failed to extract geoms from stack"))?;
        let mut other_pane_ids_and_geoms = self
            .positions_of_panes_and_their_stacks(neighboring_pane_ids)
            .ok_or_else(|| anyhow!("Failed to get pane geoms"))?;
        if other_pane_ids_and_geoms.is_empty() {
            // nothing to do
            return Ok(());
        };
        let Some(geom_to_combine) = self.combine_geoms_vertically(&other_pane_ids_and_geoms) else {
            log::error!("Failed to combine geoms vertically");
            return Ok(());
        };
        let new_stack_geom = if geom_to_combine.x < geom_of_main_pane.x {
            geom_to_combine.combine_horizontally_with(&geom_of_main_pane)
        } else {
            geom_of_main_pane.combine_horizontally_with(&geom_to_combine)
        };
        let Some(new_stack_geom) = new_stack_geom else {
            // nothing to do, likely the pane below is fixed
            return Ok(());
        };
        let stack_id = self.next_stack_id();
        // we add the extra panes in the original stack (if any) so that they will be assigned pane
        // positions but not affect the stack geometry
        other_pane_ids_and_geoms.append(&mut extra_stacked_geoms_of_main_pane);
        let mut panes = self.panes.borrow_mut();
        let mut running_y = new_stack_geom.y;
        let mut geom_of_flexible_pane = new_stack_geom.clone();
        geom_of_flexible_pane
            .rows
            .decrease_inner(other_pane_ids_and_geoms.len());
        let mut all_stacked_geoms = other_pane_ids_and_geoms;
        let original_geom_of_main_pane = panes
            .get(&root_pane_id)
            .ok_or_else(|| anyhow!("Failed to find root geom"))?
            .position_and_size(); // for sorting purposes
        all_stacked_geoms.push((*root_pane_id, original_geom_of_main_pane));
        all_stacked_geoms.sort_by(|(_a_id, a_geom), (_b_id, b_geom)| {
            if a_geom.x == b_geom.x {
                a_geom.y.cmp(&b_geom.y)
            } else {
                a_geom.x.cmp(&b_geom.x)
            }
        });
        for (pane_id, mut pane_geom) in all_stacked_geoms {
            if &pane_id == root_pane_id {
                if let Some(root_pane) = panes.get_mut(&root_pane_id) {
                    pane_geom.x = new_stack_geom.x;
                    pane_geom.cols = new_stack_geom.cols;
                    pane_geom.y = running_y;
                    pane_geom.rows = geom_of_flexible_pane.rows;
                    pane_geom.stacked = Some(stack_id);
                    pane_geom.logical_position = root_pane.position_and_size().logical_position;
                    root_pane.set_geom(pane_geom);
                    running_y += pane_geom.rows.as_usize();
                }
            } else {
                if let Some(pane_in_stack) = panes.get_mut(&pane_id) {
                    pane_geom.x = new_stack_geom.x;
                    pane_geom.cols = new_stack_geom.cols;
                    pane_geom.y = running_y;
                    pane_geom.rows = Dimension::fixed(1);
                    pane_geom.stacked = Some(stack_id);
                    running_y += 1;
                    pane_in_stack.set_geom(pane_geom);
                }
            }
        }
        Ok(())
    }
    pub fn break_pane_out_of_stack(&mut self, pane_id: &PaneId) -> Option<Vec<PaneId>> {
        let err_context = || "Failed to break pane out of stack";
        let mut pane_ids_that_were_resized = vec![];
        let Some(position_and_size_of_stack) = self.position_and_size_of_stack(pane_id) else {
            log::error!("Could not find stack size for pane id: {:?}", pane_id);
            return None;
        };
        let mut all_stacked_pane_positions = self
            .positions_in_stack(&pane_id)
            .with_context(err_context)
            .ok()?;
        if all_stacked_pane_positions.is_empty() {
            return None;
        }
        let flexible_pane_id_is_on_the_bottom = all_stacked_pane_positions
            .iter()
            .last()
            .map(|(_, last_pane_geom_in_stack)| last_pane_geom_in_stack.rows.is_percent())
            .unwrap_or(false);
        let (
            mut new_position_and_size_of_stack,
            position_and_size_of_broken_out_pane,
            pane_id_to_break_out,
        ) = if flexible_pane_id_is_on_the_bottom {
            self.break_out_stack_geom_upwards(
                position_and_size_of_stack,
                &mut all_stacked_pane_positions,
            )?
        } else {
            self.break_out_stack_geom_downwards(
                position_and_size_of_stack,
                &mut all_stacked_pane_positions,
            )?
        };
        let stack_id = all_stacked_pane_positions
            .iter()
            .next()
            .and_then(|(_, first_geom)| first_geom.stacked)?;
        let flexible_pane_id = self.get_flexible_pane_id(&all_stacked_pane_positions)?;
        self.set_geom_of_broken_out_pane(
            pane_id_to_break_out,
            position_and_size_of_broken_out_pane,
        );
        pane_ids_that_were_resized.push(pane_id_to_break_out);
        new_position_and_size_of_stack.stacked = Some(stack_id);
        self.reset_stack_size(
            &new_position_and_size_of_stack,
            &all_stacked_pane_positions,
            flexible_pane_id,
            &mut pane_ids_that_were_resized,
        );
        Some(pane_ids_that_were_resized)
    }
    pub fn next_stack_id(&self) -> usize {
        let mut highest_stack_id = 0;
        let panes = self.panes.borrow();
        for pane in panes.values() {
            if let Some(stack_id) = pane.position_and_size().stacked {
                highest_stack_id = std::cmp::max(highest_stack_id, stack_id + 1);
            }
        }
        highest_stack_id
    }
    pub fn positions_and_sizes_of_all_stacks(&self) -> Option<HashMap<usize, PaneGeom>> {
        let panes = self.panes.borrow();
        let mut positions_and_sizes_of_all_stacks = HashMap::new();
        for pane in panes.values() {
            if let Some(stack_id) = pane.current_geom().stacked {
                if !positions_and_sizes_of_all_stacks.contains_key(&stack_id) {
                    positions_and_sizes_of_all_stacks
                        .insert(stack_id, self.position_and_size_of_stack(&pane.pid())?);
                }
            }
        }
        Some(positions_and_sizes_of_all_stacks)
    }
    pub fn pane_ids_in_stack(&self, stack_id: usize) -> Vec<PaneId> {
        let panes = self.panes.borrow();
        let mut pane_ids_in_stack = vec![];
        for pane in panes.values() {
            if pane.current_geom().stacked == Some(stack_id) {
                pane_ids_in_stack.push(pane.pid());
            }
        }
        pane_ids_in_stack
    }
    fn reset_stack_size(
        &self,
        new_position_and_size_of_stack: &PaneGeom,
        all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>,
        flexible_pane_id: PaneId,
        pane_ids_that_were_resized: &mut Vec<PaneId>,
    ) {
        let mut running_pane_geom = new_position_and_size_of_stack.clone();
        let only_one_pane_left_in_stack = all_stacked_pane_positions.len() == 1;
        let count_of_one_liners_in_stack =
            all_stacked_pane_positions.iter().len().saturating_sub(1);
        let flexible_pane_row_count = running_pane_geom
            .rows
            .as_usize()
            .saturating_sub(count_of_one_liners_in_stack);
        for (pane_id_in_stack, pane_geom_in_stack) in all_stacked_pane_positions.iter() {
            let logical_position = pane_geom_in_stack.logical_position;
            if only_one_pane_left_in_stack {
                self.set_geom_of_broken_out_pane(*pane_id_in_stack, running_pane_geom);
            } else if pane_id_in_stack == &flexible_pane_id {
                self.set_geom_of_flexible_pane(
                    *pane_id_in_stack,
                    &mut running_pane_geom,
                    flexible_pane_row_count,
                    logical_position,
                );
            } else {
                self.set_geom_of_one_liner_pane(
                    *pane_id_in_stack,
                    &mut running_pane_geom,
                    logical_position,
                );
            }
            pane_ids_that_were_resized.push(*pane_id_in_stack);
        }
    }
    fn set_geom_of_broken_out_pane(
        &self,
        pane_id_to_break_out: PaneId,
        mut position_and_size: PaneGeom,
    ) {
        let mut panes = self.panes.borrow_mut();
        if let Some(pane_to_break_out) = panes.get_mut(&pane_id_to_break_out) {
            let logical_position_of_pane = pane_to_break_out.current_geom().logical_position;
            position_and_size.logical_position = logical_position_of_pane;
            position_and_size.stacked = None;
            pane_to_break_out.set_geom(position_and_size);
        }
    }
    fn set_geom_of_flexible_pane(
        &self,
        pane_id_of_flexible_pane: PaneId,
        running_pane_geom: &mut PaneGeom,
        row_count: usize,
        logical_position: Option<usize>,
    ) {
        let mut flexible_pane_geom = running_pane_geom.clone();
        let mut panes = self.panes.borrow_mut();
        if let Some(pane_in_stack) = panes.get_mut(&pane_id_of_flexible_pane) {
            flexible_pane_geom.rows.set_inner(row_count);
            running_pane_geom.y += flexible_pane_geom.rows.as_usize();
            running_pane_geom
                .rows
                .decrease_inner(flexible_pane_geom.rows.as_usize());
            flexible_pane_geom.logical_position = logical_position;
            pane_in_stack.set_geom(flexible_pane_geom);
        }
    }
    fn set_geom_of_one_liner_pane(
        &self,
        pane_id_of_one_liner_pane: PaneId,
        running_pane_geom: &mut PaneGeom,
        logical_position: Option<usize>,
    ) {
        let mut one_liner_geom = running_pane_geom.clone();
        let mut panes = self.panes.borrow_mut();
        if let Some(pane_in_stack) = panes.get_mut(&pane_id_of_one_liner_pane) {
            one_liner_geom.rows = Dimension::fixed(1);
            running_pane_geom.y += 1;
            running_pane_geom.rows.decrease_inner(1);
            one_liner_geom.logical_position = logical_position;
            pane_in_stack.set_geom(one_liner_geom);
        }
    }
    fn break_out_stack_geom_upwards(
        &self,
        position_and_size_of_stack: PaneGeom,
        all_stacked_pane_positions: &mut Vec<(PaneId, PaneGeom)>,
    ) -> Option<(PaneGeom, PaneGeom, PaneId)> {
        let mut new_position_and_size_of_stack = position_and_size_of_stack.clone();
        let rows_for_broken_out_pane = new_position_and_size_of_stack
            .rows
            .split_out(all_stacked_pane_positions.len() as f64);
        let mut position_and_size_of_broken_out_pane = position_and_size_of_stack.clone();
        position_and_size_of_broken_out_pane.stacked = None;
        position_and_size_of_broken_out_pane.rows = rows_for_broken_out_pane;
        new_position_and_size_of_stack.y = position_and_size_of_broken_out_pane.y
            + position_and_size_of_broken_out_pane.rows.as_usize();
        let pane_id_to_break_out = all_stacked_pane_positions.remove(0).0;
        Some((
            new_position_and_size_of_stack,
            position_and_size_of_broken_out_pane,
            pane_id_to_break_out,
        ))
    }
    fn break_out_stack_geom_downwards(
        &self,
        position_and_size_of_stack: PaneGeom,
        all_stacked_pane_positions: &mut Vec<(PaneId, PaneGeom)>,
    ) -> Option<(PaneGeom, PaneGeom, PaneId)> {
        let mut new_position_and_size_of_stack = position_and_size_of_stack.clone();
        let rows_for_broken_out_pane = new_position_and_size_of_stack
            .rows
            .split_out(all_stacked_pane_positions.len() as f64);
        let mut position_and_size_of_broken_out_pane = position_and_size_of_stack.clone();
        position_and_size_of_broken_out_pane.stacked = None;
        position_and_size_of_broken_out_pane.y =
            new_position_and_size_of_stack.y + new_position_and_size_of_stack.rows.as_usize();
        position_and_size_of_broken_out_pane.rows = rows_for_broken_out_pane;
        let pane_id_to_break_out = all_stacked_pane_positions.pop()?.0;
        Some((
            new_position_and_size_of_stack,
            position_and_size_of_broken_out_pane,
            pane_id_to_break_out,
        ))
    }
    fn get_flexible_pane_id(
        &self,
        all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>,
    ) -> Option<PaneId> {
        let err_context = || "Failed to get flexible pane id";
        let position_of_flexible_pane = self
            .position_of_flexible_pane(&all_stacked_pane_positions)
            .with_context(err_context)
            .ok()?;
        let (flexible_pane_id, _flexible_pane_geom) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .copied()
            .with_context(err_context)
            .ok()?;
        Some(flexible_pane_id)
    }
    fn get_all_stacks(&self) -> Result<Vec<Vec<(PaneId, PaneGeom)>>> {
        let err_context = || "Failed to get positions in stack";
        let panes = self.panes.borrow();
        let all_flexible_panes_in_stack: Vec<PaneId> = panes
            .iter()
            .filter(|(_pid, p)| {
                p.position_and_size().is_stacked() && !p.position_and_size().rows.is_fixed()
            })
            .map(|(pid, _p)| *pid)
            .collect();
        let mut stacks = vec![];
        for pane_id in all_flexible_panes_in_stack {
            stacks.push(
                self.positions_in_stack(&pane_id)
                    .with_context(err_context)?,
            );
        }
        Ok(stacks)
    }
    fn fill_space_over_one_liner_pane(&mut self, id: &PaneId) -> Result<bool> {
        let (position_of_current_pane, position_of_flexible_pane) =
            self.position_of_current_and_flexible_pane(id)?;
        if position_of_current_pane > position_of_flexible_pane {
            self.fill_space_over_one_liner_pane_above_flexible_pane(id)
        } else {
            self.fill_space_over_one_liner_pane_below_flexible_pane(id)
        }
    }
    fn fill_space_over_visible_stacked_pane(&mut self, id: &PaneId) -> Result<bool> {
        let err_context = || format!("Failed to fill_space_over_visible_stacked_pane");
        let all_stacked_pane_positions = self.positions_in_stack(id)?;
        let mut panes = self.panes.borrow_mut();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let position_of_current_pane =
            self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let only_one_pane_remaining_in_stack_after_close = all_stacked_pane_positions.len() == 2;
        if all_stacked_pane_positions.len() > position_of_current_pane + 1 {
            let mut pane_to_close_position_and_size = pane_to_close.position_and_size();
            pane_to_close_position_and_size
                .rows
                .set_inner(pane_to_close_position_and_size.rows.as_usize() + 1);
            let pane_id_below = all_stacked_pane_positions
                .iter()
                .nth(position_of_current_pane + 1)
                .map(|(pid, _)| *pid)
                .with_context(err_context)?;
            if only_one_pane_remaining_in_stack_after_close {
                pane_to_close_position_and_size.stacked = None;
            }
            let pane_below = panes.get_mut(&pane_id_below).with_context(err_context)?;
            pane_below.set_geom(pane_to_close_position_and_size);
            return Ok(true);
        } else if position_of_current_pane > 0 {
            let mut pane_to_close_position_and_size = pane_to_close.position_and_size();
            pane_to_close_position_and_size
                .rows
                .set_inner(pane_to_close_position_and_size.rows.as_usize() + 1);
            pane_to_close_position_and_size.y -= 1;
            let pane_id_above = all_stacked_pane_positions
                .iter()
                .nth(position_of_current_pane - 1)
                .map(|(pid, _)| *pid)
                .with_context(err_context)?;
            if only_one_pane_remaining_in_stack_after_close {
                pane_to_close_position_and_size.stacked = None;
            }
            let pane_above = panes.get_mut(&pane_id_above).with_context(err_context)?;
            pane_above.set_geom(pane_to_close_position_and_size);
            return Ok(true);
        } else {
            return Ok(false);
        }
    }
    fn fill_space_over_one_liner_pane_above_flexible_pane(&mut self, id: &PaneId) -> Result<bool> {
        let err_context =
            || format!("Failed to fill_space_over_one_liner_pane_above_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(id)?;
        let mut panes = self.panes.borrow_mut();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let position_of_current_pane =
            self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        let id_of_flexible_pane = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .map(|(pid, _p)| *pid)
            .with_context(err_context)?;
        let flexible_pane = panes
            .get_mut(&id_of_flexible_pane)
            .with_context(err_context)?;
        let mut flexible_pane_position_and_size = flexible_pane.position_and_size();
        flexible_pane_position_and_size
            .rows
            .set_inner(flexible_pane_position_and_size.rows.as_usize() + 1);
        flexible_pane.set_geom(flexible_pane_position_and_size);
        for (i, (pid, _position)) in all_stacked_pane_positions.iter().enumerate() {
            if i > position_of_flexible_pane && i < position_of_current_pane {
                let pane = panes.get_mut(pid).with_context(err_context)?;
                let mut pane_position_and_size = pane.position_and_size();
                pane_position_and_size.y += 1;
                pane.set_geom(pane_position_and_size);
            }
        }
        Ok(true)
    }
    fn fill_space_over_one_liner_pane_below_flexible_pane(&mut self, id: &PaneId) -> Result<bool> {
        let err_context =
            || format!("Failed to fill_space_over_one_liner_pane_below_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(id)?;
        let mut panes = self.panes.borrow_mut();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let position_of_current_pane =
            self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        let id_of_flexible_pane = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .map(|(pid, _p)| *pid)
            .with_context(err_context)?;
        let flexible_pane = panes
            .get_mut(&id_of_flexible_pane)
            .with_context(err_context)?;
        let mut flexible_pane_position_and_size = flexible_pane.position_and_size();
        flexible_pane_position_and_size
            .rows
            .set_inner(flexible_pane_position_and_size.rows.as_usize() + 1);
        flexible_pane.set_geom(flexible_pane_position_and_size);
        for (i, (pid, _position)) in all_stacked_pane_positions.iter().enumerate() {
            if i > position_of_current_pane && i <= position_of_flexible_pane {
                let pane = panes.get_mut(pid).with_context(err_context)?;
                let mut pane_position_and_size = pane.position_and_size();
                pane_position_and_size.y = pane_position_and_size.y.saturating_sub(1);
                pane.set_geom(pane_position_and_size);
            }
        }
        Ok(true)
    }
    fn make_lowest_pane_in_stack_flexible(&mut self, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to make_lowest_pane_flexible");
        let mut all_stacked_pane_positions = self.positions_in_stack(destination_pane_id)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        if position_of_flexible_pane != all_stacked_pane_positions.len().saturating_sub(1) {
            let mut panes = self.panes.borrow_mut();
            let height_of_flexible_pane = all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .map(|(_pid, p)| p.rows)
                .with_context(err_context)?;
            let (lowest_pane_id, mut lowest_pane_geom) = all_stacked_pane_positions
                .last_mut()
                .with_context(err_context)?;
            lowest_pane_geom.rows = height_of_flexible_pane;
            panes
                .get_mut(lowest_pane_id)
                .with_context(err_context)?
                .set_geom(lowest_pane_geom);
            let (flexible_pane_id, mut flexible_pane_geom) = all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .with_context(err_context)?;
            flexible_pane_geom.rows = Dimension::fixed(1);
            panes
                .get_mut(flexible_pane_id)
                .with_context(err_context)?
                .set_geom(flexible_pane_geom);
            for (i, (pid, _position)) in all_stacked_pane_positions.iter().enumerate() {
                if i > position_of_flexible_pane {
                    let pane = panes.get_mut(pid).with_context(err_context)?;
                    let mut pane_position_and_size = pane.position_and_size();
                    pane_position_and_size.y = pane_position_and_size
                        .y
                        .saturating_sub(height_of_flexible_pane.as_usize() - 1);
                    pane.set_geom(pane_position_and_size);
                }
            }
        }
        Ok(())
    }
    fn make_highest_pane_in_stack_flexible(&mut self, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to make_lowest_pane_flexible");
        let mut all_stacked_pane_positions = self.positions_in_stack(destination_pane_id)?;
        let position_of_flexible_pane =
            self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        if position_of_flexible_pane != 0 {
            let mut panes = self.panes.borrow_mut();
            let height_of_flexible_pane = all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .map(|(_pid, p)| p.rows)
                .with_context(err_context)?;
            let (highest_pane_id, mut highest_pane_geom) = all_stacked_pane_positions
                .first_mut()
                .with_context(err_context)?;
            let y_of_whole_stack = highest_pane_geom.y;
            highest_pane_geom.rows = height_of_flexible_pane;
            panes
                .get_mut(highest_pane_id)
                .with_context(err_context)?
                .set_geom(highest_pane_geom);
            let (flexible_pane_id, mut flexible_pane_geom) = all_stacked_pane_positions
                .iter()
                .nth(position_of_flexible_pane)
                .with_context(err_context)?;
            flexible_pane_geom.rows = Dimension::fixed(1);
            panes
                .get_mut(flexible_pane_id)
                .with_context(err_context)?
                .set_geom(flexible_pane_geom);
            for (i, (pid, _position)) in all_stacked_pane_positions.iter().enumerate() {
                if i > 0 {
                    let pane = panes.get_mut(pid).with_context(err_context)?;
                    let mut pane_position_and_size = pane.position_and_size();
                    pane_position_and_size.y =
                        y_of_whole_stack + height_of_flexible_pane.as_usize() + (i - 1);
                    pane.set_geom(pane_position_and_size);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "./unit/stacked_panes_tests.rs"]
mod stacked_panes_tests;
