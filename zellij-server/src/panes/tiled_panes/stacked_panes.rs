use crate::{panes::PaneId, tab::Pane};
use zellij_utils::{
    errors::prelude::*,
    pane_size::{Dimension, PaneGeom},
};
use std::cell::RefCell;
use std::rc::Rc;
use std::collections::HashMap;

pub struct StackedPanes <'a>{
    panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>,
}

impl <'a>StackedPanes <'a>{
    pub fn new(panes: Rc<RefCell<HashMap<PaneId, &'a mut Box<dyn Pane>>>>) -> Self {
        StackedPanes {
            panes
        }
    }
    pub fn move_down(&mut self, source_pane_id: &PaneId, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to move stacked pane focus down");
        let mut panes = self.panes.borrow_mut();
        let source_pane = panes.get_mut(source_pane_id).with_context(err_context)?;
        let mut source_pane_geom = source_pane.position_and_size();
        let mut destination_pane_geom = source_pane_geom.clone();
        destination_pane_geom.y = source_pane_geom.y + 1;
        source_pane_geom.rows = Dimension::fixed(1);
        source_pane.set_geom(source_pane_geom);
        let destination_pane = panes.get_mut(&destination_pane_id).with_context(err_context)?;
        destination_pane.set_geom(destination_pane_geom);
        Ok(())
    }
    pub fn move_up(&mut self, source_pane_id: &PaneId, destination_pane_id: &PaneId) -> Result<()> {
        let err_context = || format!("Failed to move stacked pane focus up");
        let mut panes = self.panes.borrow_mut();
        let source_pane = panes.get_mut(source_pane_id).with_context(err_context)?;
        let mut source_pane_geom = source_pane.position_and_size();
        let mut destination_pane_geom = source_pane_geom.clone();
        source_pane_geom.y = (source_pane_geom.y + source_pane_geom.rows.as_usize()) - 1; // -1 because we want to be at the last line of the source pane, not the next line over
        source_pane_geom.rows = Dimension::fixed(1);
        source_pane.set_geom(source_pane_geom);
        destination_pane_geom.y -= 1;
        let destination_pane = panes.get_mut(&destination_pane_id).with_context(err_context)?;
        destination_pane.set_geom(destination_pane_geom);
        Ok(())
    }
    pub fn flexible_pane_id_in_stack(&self, pane_id_in_stack: &PaneId) -> Option<PaneId> {
        let all_stacked_pane_positions = self.positions_in_stack(pane_id_in_stack).ok()?;
        all_stacked_pane_positions.iter().find(|(_pid, p)| p.rows.is_percent()).map(|(pid, _p)| *pid)
    }
    fn pane_is_one_liner(&self, id: &PaneId) -> Result<bool> {
        let err_context = || format!("Cannot determin if pane is one liner or not");
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        Ok(pane_to_close.position_and_size().rows.as_usize() == 1)
    }
    fn positions_in_stack(&self, id: &PaneId) -> Result<Vec<(PaneId, PaneGeom)>> {
        // find the full stack of panes around the given id, sorted by pane location top to bottom
        let err_context = || format!("Failed to find stacked panes");
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let mut all_stacked_pane_positions: Vec<(PaneId, PaneGeom)> = panes
            .iter()
            .filter(|(_pid, p)| p.position_and_size().is_stacked)
            .filter(|(_pid, p)| p.position_and_size().x == pane_to_close.position_and_size().x && p.position_and_size().cols == pane_to_close.position_and_size().cols)
            .map(|(pid, p)| (*pid, p.position_and_size()))
            .collect();
        all_stacked_pane_positions.sort_by(|(_a_pid, a), (_b_pid, b)| {
            a.y.cmp(&b.y)
        });
        Ok(all_stacked_pane_positions)
    }
    fn position_of_current_and_flexible_pane(&self, current_pane_id: &PaneId) -> Result<(usize, usize)> { // (current_pane,
        let err_context = || format!("Failed to position_of_current_and_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(current_pane_id)?;
        let panes = self.panes.borrow();
        let pane_to_close = panes.get(current_pane_id).with_context(err_context)?;
        let position_of_current_pane = self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane = self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        Ok((position_of_current_pane, position_of_flexible_pane))
    }
    fn position_of_current_pane(&self, all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>, pane_to_close: &Box<dyn Pane>) -> Result<usize> {
        let err_context = || format!("Failed to find position of current pane");
        all_stacked_pane_positions.iter().position(|(pid, _p)| pid == &pane_to_close.pid()).with_context(err_context)
    }
    fn position_of_flexible_pane(&self, all_stacked_pane_positions: &Vec<(PaneId, PaneGeom)>) -> Result<usize> {
        let err_context = || format!("Failed to find position of flexible pane");
        all_stacked_pane_positions.iter().position(|(_pid, p)| p.rows.is_percent()).with_context(err_context)
    }
    pub fn fill_space_over_pane_in_stack(&mut self, id: &PaneId) -> Result<bool> {
        if self.pane_is_one_liner(id)? {
            self.fill_space_over_one_liner_pane(id)
        } else {
            self.fill_space_over_visible_stacked_pane(id)
        }
    }
    fn fill_space_over_one_liner_pane(&mut self, id: &PaneId) -> Result<bool> {
        let (position_of_current_pane, position_of_flexible_pane) = self.position_of_current_and_flexible_pane(id)?;
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
        let position_of_current_pane = self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        if all_stacked_pane_positions.len() > position_of_current_pane + 1 {
            let mut pane_to_close_position_and_size = pane_to_close.position_and_size();
            pane_to_close_position_and_size.rows.set_inner(pane_to_close_position_and_size.rows.as_usize() + 1);
            let pane_id_below = all_stacked_pane_positions.iter().nth(position_of_current_pane + 1).map(|(pid, _)| *pid).with_context(err_context)?;
            let pane_below = panes.get_mut(&pane_id_below).with_context(err_context)?;
            pane_below.set_geom(pane_to_close_position_and_size);
            return Ok(true);
        } else if position_of_current_pane > 0 {
            let mut pane_to_close_position_and_size = pane_to_close.position_and_size();
            pane_to_close_position_and_size.rows.set_inner(pane_to_close_position_and_size.rows.as_usize() + 1);
            pane_to_close_position_and_size.y -= 1;
            let pane_id_above = all_stacked_pane_positions.iter().nth(position_of_current_pane - 1).map(|(pid, _)| *pid).with_context(err_context)?;
            let pane_above = panes.get_mut(&pane_id_above).with_context(err_context)?;
            pane_above.set_geom(pane_to_close_position_and_size);
            return Ok(true);
        } else {
            return Ok(false);
        }
    }
    fn fill_space_over_one_liner_pane_above_flexible_pane(&mut self, id: &PaneId) -> Result<bool> {
        let err_context = || format!("Failed to fill_space_over_one_liner_pane_above_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(id)?;
        let mut panes = self.panes.borrow_mut();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let position_of_current_pane = self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane = self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        let id_of_flexible_pane = all_stacked_pane_positions.iter().nth(position_of_flexible_pane).map(|(pid, _p)| *pid).with_context(err_context)?;
        let flexible_pane = panes.get_mut(&id_of_flexible_pane).with_context(err_context)?;
        let mut flexible_pane_position_and_size = flexible_pane.position_and_size();
        flexible_pane_position_and_size.rows.set_inner(flexible_pane_position_and_size.rows.as_usize() + 1);
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
        let err_context = || format!("Failed to fill_space_over_one_liner_pane_below_flexible_pane");
        let all_stacked_pane_positions = self.positions_in_stack(id)?;
        let mut panes = self.panes.borrow_mut();
        let pane_to_close = panes.get(id).with_context(err_context)?;
        let position_of_current_pane = self.position_of_current_pane(&all_stacked_pane_positions, &pane_to_close)?;
        let position_of_flexible_pane = self.position_of_flexible_pane(&all_stacked_pane_positions)?;
        let id_of_flexible_pane = all_stacked_pane_positions.iter().nth(position_of_flexible_pane).map(|(pid, _p)| *pid).with_context(err_context)?;
        let flexible_pane = panes.get_mut(&id_of_flexible_pane).with_context(err_context)?;
        let mut flexible_pane_position_and_size = flexible_pane.position_and_size();
        flexible_pane_position_and_size.rows.set_inner(flexible_pane_position_and_size.rows.as_usize() + 1);
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
}
