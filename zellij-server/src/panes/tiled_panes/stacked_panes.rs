use crate::{panes::PaneId, tab::Pane};
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
        let source_pane_is_stacked = self
            .panes
            .borrow()
            .get(source_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .is_stacked;
        let destination_pane_is_stacked = self
            .panes
            .borrow()
            .get(destination_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .is_stacked;
        if source_pane_is_stacked && destination_pane_is_stacked {
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
        } else if destination_pane_is_stacked {
            // we're moving down to the highest pane in the stack, we need to expand it and shrink the
            // expanded stack pane
            self.make_highest_pane_in_stack_flexible(destination_pane_id)?;
        }
        Ok(())
    }
    pub fn move_up(&mut self, source_pane_id: &PaneId, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to move stacked pane focus up");
        let source_pane_is_stacked = self
            .panes
            .borrow()
            .get(source_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .is_stacked;
        let destination_pane_is_stacked = self
            .panes
            .borrow()
            .get(destination_pane_id)
            .with_context(err_context)?
            .position_and_size()
            .is_stacked;
        if source_pane_is_stacked && destination_pane_is_stacked {
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
        } else if destination_pane_is_stacked {
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
            is_stacked: true, // important because otherwise the minimum stack size will not be
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
        let (flexible_pane_id, flexible_pane) = all_stacked_pane_positions
            .iter()
            .nth(position_of_flexible_pane)
            .with_context(err_context)?;
        let current_rows = all_stacked_pane_positions.len() + (flexible_pane.rows.as_usize() - 1);
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

        if new_rows >= current_rows {
            let extra_rows = new_rows - current_rows;
            let mut new_flexible_pane_geom = *flexible_pane;
            new_flexible_pane_geom
                .rows
                .set_inner(new_flexible_pane_geom.rows.as_usize() + extra_rows);
            self.panes
                .borrow_mut()
                .get_mut(&flexible_pane_id)
                .with_context(err_context)?
                .set_geom(new_flexible_pane_geom);
            adjust_stack_geoms(new_flexible_pane_geom)?;
        } else {
            if new_rows < all_stacked_pane_positions.len() {
                // TODO: test this!! we don't want crashes...
                return Err(anyhow!("Not enough room for stacked panes"));
            }
            let rows_deficit = current_rows - new_rows;
            let mut new_flexible_pane_geom = *flexible_pane;
            new_flexible_pane_geom
                .rows
                .set_inner(new_flexible_pane_geom.rows.as_usize() - rows_deficit);
            self.panes
                .borrow_mut()
                .get_mut(&flexible_pane_id)
                .with_context(err_context)?
                .set_geom(new_flexible_pane_geom);
            adjust_stack_geoms(new_flexible_pane_geom)?;
        }
        Ok(())
    }
    fn pane_is_one_liner(&self, id: &PaneId) -> Result<bool> {
        let err_context = || format!("Cannot determin if pane is one liner or not");
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        Ok(pane_to_close.position_and_size().rows.is_fixed())
    }
    fn positions_in_stack(&self, id: &PaneId) -> Result<Vec<(PaneId, PaneGeom)>> {
        // find the full stack of panes around the given id, sorted by pane location top to bottom
        let err_context = || format!("Failed to find stacked panes");
        let panes = self.panes.borrow();
        let pane_in_stack = panes.get(id).with_context(err_context)?;
        let mut all_stacked_pane_positions: Vec<(PaneId, PaneGeom)> = panes
            .iter()
            .filter(|(_pid, p)| p.position_and_size().is_stacked)
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
                .filter(|(_p_id, p)| p.position_and_size().is_stacked)
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
    pub fn make_room_for_new_pane(&mut self) -> Result<PaneGeom> {
        let err_context = || format!("Failed to add pane to stack");
        let all_stacks = self.get_all_stacks()?;
        for stack in all_stacks {
            if let Some((id_of_flexible_pane_in_stack, _flexible_pane_in_stack)) = stack
                .iter()
                .find(|(_p_id, p)| !p.rows.is_fixed() && p.rows.as_usize() > 1)
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
    fn get_all_stacks(&self) -> Result<Vec<Vec<(PaneId, PaneGeom)>>> {
        let err_context = || "Failed to get positions in stack";
        let panes = self.panes.borrow();
        let all_flexible_panes_in_stack: Vec<PaneId> = panes
            .iter()
            .filter(|(_pid, p)| {
                p.position_and_size().is_stacked && !p.position_and_size().rows.is_fixed()
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
