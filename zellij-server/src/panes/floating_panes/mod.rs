mod floating_pane_grid;
use zellij_utils::{
    data::{Direction, ResizeStrategy},
    position::Position,
};

use crate::resize_pty;
use crate::tab::Pane;
use floating_pane_grid::FloatingPaneGrid;

use crate::{
    os_input_output::ServerOsApi,
    output::{FloatingPanesStack, Output},
    panes::{ActivePanes, PaneId},
    plugins::PluginInstruction,
    thread_bus::ThreadSenders,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    ClientId,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::time::Instant;
use zellij_utils::{
    data::{ModeInfo, Style},
    errors::prelude::*,
    input::command::RunCommand,
    input::layout::FloatingPaneLayout,
    pane_size::{Dimension, Offset, PaneGeom, Size, Viewport},
};

const RESIZE_INCREMENT_WIDTH: usize = 5;
const RESIZE_INCREMENT_HEIGHT: usize = 2;

pub struct FloatingPanes {
    panes: BTreeMap<PaneId, Box<dyn Pane>>,
    display_area: Rc<RefCell<Size>>,
    viewport: Rc<RefCell<Viewport>>,
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
    connected_clients_in_app: Rc<RefCell<HashSet<ClientId>>>,
    mode_info: Rc<RefCell<HashMap<ClientId, ModeInfo>>>,
    default_mode_info: ModeInfo,
    style: Style,
    session_is_mirrored: bool,
    desired_pane_positions: HashMap<PaneId, PaneGeom>, // this represents the positions of panes the user moved with intention, rather than by resizing the terminal window
    z_indices: Vec<PaneId>,
    active_panes: ActivePanes,
    show_panes: bool,
    pane_being_moved_with_mouse: Option<(PaneId, Position)>,
    senders: ThreadSenders,
    next_geoms: VecDeque<PaneGeom>, // TODO: remove this?
}

#[allow(clippy::borrowed_box)]
#[allow(clippy::too_many_arguments)]
impl FloatingPanes {
    pub fn new(
        display_area: Rc<RefCell<Size>>,
        viewport: Rc<RefCell<Viewport>>,
        connected_clients: Rc<RefCell<HashSet<ClientId>>>,
        connected_clients_in_app: Rc<RefCell<HashSet<ClientId>>>,
        mode_info: Rc<RefCell<HashMap<ClientId, ModeInfo>>>,
        session_is_mirrored: bool,
        default_mode_info: ModeInfo,
        style: Style,
        os_input: Box<dyn ServerOsApi>,
        senders: ThreadSenders,
    ) -> Self {
        FloatingPanes {
            panes: BTreeMap::new(),
            display_area,
            viewport,
            connected_clients,
            connected_clients_in_app,
            mode_info,
            session_is_mirrored,
            default_mode_info,
            style,
            desired_pane_positions: HashMap::new(),
            z_indices: vec![],
            show_panes: false,
            active_panes: ActivePanes::new(&os_input),
            pane_being_moved_with_mouse: None,
            senders,
            next_geoms: VecDeque::new(),
        }
    }
    pub fn stack(&self) -> Option<FloatingPanesStack> {
        if self.panes_are_visible() {
            let layers = self
                .z_indices
                .iter()
                .map(|pane_id| self.panes.get(pane_id).unwrap().position_and_size())
                .collect();
            Some(FloatingPanesStack { layers })
        } else {
            None
        }
    }
    pub fn pane_ids(&self) -> impl Iterator<Item = &PaneId> {
        self.panes.keys()
    }
    pub fn add_pane(&mut self, pane_id: PaneId, pane: Box<dyn Pane>) {
        self.desired_pane_positions
            .insert(pane_id, pane.position_and_size());
        self.panes.insert(pane_id, pane);
        self.z_indices.push(pane_id);
    }
    pub fn replace_active_pane(
        &mut self,
        pane: Box<dyn Pane>,
        client_id: ClientId,
    ) -> Result<Box<dyn Pane>> {
        self.active_panes
            .get(&client_id)
            .with_context(|| format!("failed to determine active pane for client {client_id}"))
            .copied()
            .and_then(|active_pane_id| self.replace_pane(active_pane_id, pane))
            .with_context(|| format!("failed to replace active pane for client {client_id}"))
    }
    pub fn replace_pane(
        &mut self,
        pane_id: PaneId,
        mut with_pane: Box<dyn Pane>,
    ) -> Result<Box<dyn Pane>> {
        let err_context = || format!("failed to replace pane {pane_id:?} with pane");

        let with_pane_id = with_pane.pid();
        with_pane.set_content_offset(Offset::frame(1));
        let removed_pane = self
            .panes
            .remove(&pane_id)
            .with_context(|| format!("failed to remove unknown pane with ID {pane_id:?}"))
            .and_then(|removed_pane| {
                let removed_pane_id = removed_pane.pid();
                let with_pane_id = with_pane.pid();
                let removed_pane_geom = removed_pane.current_geom();
                with_pane.set_geom(removed_pane_geom);
                self.panes.insert(with_pane_id, with_pane);
                let z_index = self
                    .z_indices
                    .iter()
                    .position(|pane_id| pane_id == &removed_pane_id)
                    .context("no z-index found for pane to be removed with ID {removed_pane_id:?}")
                    .with_context(err_context)?;
                self.z_indices.remove(z_index);
                self.z_indices.insert(z_index, with_pane_id);
                Ok(removed_pane)
            });

        // update the desired_pane_positions to relate to the new pane
        if let Some(desired_pane_position) = self.desired_pane_positions.remove(&pane_id) {
            self.desired_pane_positions
                .insert(with_pane_id, desired_pane_position);
        }

        // move clients from the previously active pane to the new pane we just inserted
        self.move_clients_between_panes(pane_id, with_pane_id);
        removed_pane
    }
    pub fn remove_pane(&mut self, pane_id: PaneId) -> Option<Box<dyn Pane>> {
        self.z_indices.retain(|p_id| *p_id != pane_id);
        self.desired_pane_positions.remove(&pane_id);
        self.panes.remove(&pane_id)
    }
    pub fn hold_pane(
        &mut self,
        pane_id: PaneId,
        exit_status: Option<i32>,
        is_first_run: bool,
        run_command: RunCommand,
    ) {
        self.panes
            .get_mut(&pane_id)
            .map(|p| p.hold(exit_status, is_first_run, run_command));
    }
    pub fn get(&self, pane_id: &PaneId) -> Option<&Box<dyn Pane>> {
        self.panes.get(pane_id)
    }
    pub fn get_mut(&mut self, pane_id: &PaneId) -> Option<&mut Box<dyn Pane>> {
        self.panes.get_mut(pane_id)
    }
    pub fn get_active_pane(&self, client_id: ClientId) -> Option<&Box<dyn Pane>> {
        self.active_panes
            .get(&client_id)
            .and_then(|active_pane_id| self.panes.get(active_pane_id))
    }
    pub fn get_active_pane_mut(&mut self, client_id: ClientId) -> Option<&mut Box<dyn Pane>> {
        self.active_panes
            .get(&client_id)
            .and_then(|active_pane_id| self.panes.get_mut(active_pane_id))
    }
    pub fn panes_are_visible(&self) -> bool {
        self.show_panes
    }
    pub fn has_active_panes(&self) -> bool {
        !self.active_panes.is_empty()
    }
    pub fn has_panes(&self) -> bool {
        !self.panes.is_empty()
    }
    pub fn active_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        self.active_panes.get(&client_id).copied()
    }
    pub fn active_pane_id_or_focused_pane_id(&self, client_id: Option<ClientId>) -> Option<PaneId> {
        // returns the focused pane of any client_id - should be safe because the way things are
        // set up at the time of writing, all clients are focused on the same floating pane due to
        // z_index issues
        client_id.and_then(|client_id| self.active_panes.get(&client_id).copied()).or_else(|| self.panes.keys().next().copied())
    }
    pub fn toggle_show_panes(&mut self, should_show_floating_panes: bool) {
        self.show_panes = should_show_floating_panes;
        if should_show_floating_panes {
            self.active_panes.focus_all_panes(&mut self.panes);
        } else {
            self.active_panes.unfocus_all_panes(&mut self.panes);
        }
    }
    pub fn active_panes_contain(&self, client_id: &ClientId) -> bool {
        self.active_panes.contains_key(client_id)
    }
    pub fn panes_contain(&self, pane_id: &PaneId) -> bool {
        self.panes.contains_key(pane_id)
    }
    pub fn add_next_geom(&mut self, next_geom: PaneGeom) {
        // TODO: removeme
        if true { // TODO: config flag to cancel this behaviour - classic_pane_algorithm?
            self.next_geoms.push_back(next_geom);
        }
    }
    pub fn find_room_for_new_pane(&mut self) -> Option<PaneGeom> {
        match self.next_geoms.pop_front() {
            Some(new_pane_geom) => Some(new_pane_geom),
            None => {
                let display_area = *self.display_area.borrow();
                let viewport = *self.viewport.borrow();
                let floating_pane_grid = FloatingPaneGrid::new(
                    &mut self.panes,
                    &mut self.desired_pane_positions,
                    display_area,
                    viewport,
                );
                floating_pane_grid.find_room_for_new_pane()
            }
        }
    }
    pub fn position_floating_pane_layout(
        &mut self,
        floating_pane_layout: &FloatingPaneLayout,
    ) -> PaneGeom {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        let floating_pane_grid = FloatingPaneGrid::new(
            &mut self.panes,
            &mut self.desired_pane_positions,
            display_area,
            viewport,
        );
        let mut position = floating_pane_grid.find_room_for_new_pane().unwrap(); // TODO: no unwrap
        if let Some(x) = &floating_pane_layout.x {
            position.x = x.to_position(display_area.cols);
        }
        if let Some(y) = &floating_pane_layout.y {
            position.y = y.to_position(display_area.rows);
        }
        if let Some(width) = &floating_pane_layout.width {
            position.cols = Dimension::fixed(width.to_position(display_area.cols));
        }
        if let Some(height) = &floating_pane_layout.height {
            position.rows = Dimension::fixed(height.to_position(display_area.rows));
        }
        if position.cols.as_usize() > display_area.cols {
            position.cols = Dimension::fixed(display_area.cols);
        }
        if position.rows.as_usize() > display_area.rows {
            position.rows = Dimension::fixed(display_area.rows);
        }
        if position.x + position.cols.as_usize() > display_area.cols {
            position.x = position
                .x
                .saturating_sub((position.x + position.cols.as_usize()) - display_area.cols);
        }
        if position.y + position.rows.as_usize() > display_area.rows {
            position.y = position
                .y
                .saturating_sub((position.y + position.rows.as_usize()) - display_area.rows);
        }
        position
    }
    pub fn first_floating_pane_id(&self) -> Option<PaneId> {
        self.panes.keys().next().copied()
    }
    pub fn last_floating_pane_id(&self) -> Option<PaneId> {
        self.panes.keys().last().copied()
    }
    pub fn first_active_floating_pane_id(&self) -> Option<PaneId> {
        self.active_panes.values().next().copied()
    }
    pub fn set_force_render(&mut self) {
        for pane in self.panes.values_mut() {
            pane.set_should_render(true);
            pane.set_should_render_boundaries(true);
            pane.render_full_viewport();
        }
    }
    pub fn set_pane_frames(&mut self, os_api: &mut Box<dyn ServerOsApi>) -> Result<()> {
        let err_context =
            |pane_id: &PaneId| format!("failed to activate frame on pane {pane_id:?}");

        for pane in self.panes.values_mut() {
            // floating panes should always have a frame unless explicitly set otherwise
            if !pane.borderless() {
                pane.set_frame(true);
                pane.set_content_offset(Offset::frame(1));
            } else {
                pane.set_content_offset(Offset::default());
            }
            resize_pty!(pane, os_api, self.senders).with_context(|| err_context(&pane.pid()))?;
        }
        Ok(())
    }
    pub fn render(&mut self, output: &mut Output) -> Result<()> {
        let err_context = || "failed to render output";
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        let mut floating_panes: Vec<_> = self.panes.iter_mut().collect();
        floating_panes.sort_by(|(a_id, _a_pane), (b_id, _b_pane)| {
            self.z_indices
                .iter()
                .position(|id| id == *a_id)
                .with_context(err_context)
                .fatal()
                .cmp(
                    &self
                        .z_indices
                        .iter()
                        .position(|id| id == *b_id)
                        .with_context(err_context)
                        .fatal(),
                )
        });

        for (z_index, (kind, pane)) in floating_panes.iter_mut().enumerate() {
            let mut active_panes = self.active_panes.clone_active_panes();
            let multiple_users_exist_in_session =
                { self.connected_clients_in_app.borrow().len() > 1 };
            active_panes.retain(|c_id, _| self.connected_clients.borrow().contains(c_id));
            let mut pane_contents_and_ui = PaneContentsAndUi::new(
                pane,
                output,
                self.style,
                &active_panes,
                multiple_users_exist_in_session,
                Some(z_index + 1), // +1 because 0 is reserved for non-floating panes
                false,
                false
            );
            for client_id in &connected_clients {
                let client_mode = self
                    .mode_info
                    .borrow()
                    .get(client_id)
                    .unwrap_or(&self.default_mode_info)
                    .mode;
                pane_contents_and_ui
                    .render_pane_frame(*client_id, client_mode, self.session_is_mirrored)
                    .with_context(err_context)?;
                if let PaneId::Plugin(..) = kind {
                    pane_contents_and_ui
                        .render_pane_contents_for_client(*client_id)
                        .with_context(err_context)?;
                }
                // this is done for panes that don't have their own cursor (eg. panes of
                // another user)
                pane_contents_and_ui
                    .render_fake_cursor_if_needed(*client_id)
                    .with_context(err_context)?;
            }
            if let PaneId::Terminal(..) = kind {
                pane_contents_and_ui
                    .render_pane_contents_to_multiple_clients(connected_clients.iter().copied())
                    .with_context(err_context)?;
            }
        }
        Ok(())
    }

    pub fn resize(&mut self, new_screen_size: Size) {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        let mut floating_pane_grid = FloatingPaneGrid::new(
            &mut self.panes,
            &mut self.desired_pane_positions,
            display_area,
            viewport,
        );
        floating_pane_grid.resize(new_screen_size).unwrap();
        self.set_force_render();
    }

    pub fn resize_pty_all_panes(&mut self, os_api: &mut Box<dyn ServerOsApi>) -> Result<()> {
        for pane in self.panes.values_mut() {
            resize_pty!(pane, os_api, self.senders)
                .with_context(|| format!("failed to resize PTY in pane {:?}", pane.pid()))?;
        }
        Ok(())
    }

    pub fn resize_active_pane(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
        strategy: &ResizeStrategy,
    ) -> Result<bool> {
        // true => successfully resized
        let err_context =
            || format!("failed to {strategy} for active floating pane for client {client_id}");

        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid
                .change_pane_size(
                    active_floating_pane_id,
                    strategy,
                    (RESIZE_INCREMENT_WIDTH, RESIZE_INCREMENT_HEIGHT),
                )
                .with_context(err_context)?;

            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api, self.senders).with_context(err_context)?;
            }
            self.set_force_render();
            return Ok(true);
        }
        Ok(false)
    }

    fn set_pane_active_at(&mut self, pane_id: PaneId) {
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.set_active_at(Instant::now());
        }
    }

    pub fn move_focus(
        &mut self,
        client_id: ClientId,
        connected_clients: &HashSet<ClientId>,
        direction: &Direction,
    ) -> Result<bool> {
        // true => successfully moved
        let _err_context = || {
            format!("failed to move focus of floating pane {direction:?} for client {client_id}")
        };

        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        let active_pane_id = self.active_panes.get(&client_id).copied();
        let updated_active_pane = if let Some(active_pane_id) = active_pane_id {
            let floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            let next_index = match direction {
                Direction::Left => {
                    floating_pane_grid.next_selectable_pane_id_to_the_left(&active_pane_id)
                },
                Direction::Down => {
                    floating_pane_grid.next_selectable_pane_id_below(&active_pane_id)
                },
                Direction::Up => floating_pane_grid.next_selectable_pane_id_above(&active_pane_id),
                Direction::Right => {
                    floating_pane_grid.next_selectable_pane_id_to_the_right(&active_pane_id)
                },
            };
            match next_index {
                Some(p) => {
                    // render previously active pane so that its frame does not remain actively
                    // colored
                    let previously_active_pane = self
                        .panes
                        .get_mut(self.active_panes.get(&client_id).unwrap())
                        .unwrap();

                    previously_active_pane.set_should_render(true);
                    // we render the full viewport to remove any ui elements that might have been
                    // there before (eg. another user's cursor)
                    previously_active_pane.render_full_viewport();

                    let next_active_pane = self.panes.get_mut(&p).unwrap();
                    next_active_pane.set_should_render(true);
                    // we render the full viewport to remove any ui elements that might have been
                    // there before (eg. another user's cursor)
                    next_active_pane.render_full_viewport();

                    // move all clients
                    let connected_clients: Vec<ClientId> =
                        connected_clients.iter().copied().collect();
                    for client_id in connected_clients {
                        self.focus_pane(p, client_id);
                    }
                    self.set_pane_active_at(p);

                    self.set_force_render();
                    return Ok(true);
                },
                None => Some(active_pane_id),
            }
        } else {
            active_pane_id
        };
        match updated_active_pane {
            Some(updated_active_pane) => {
                let connected_clients: Vec<ClientId> = connected_clients.iter().copied().collect();
                for client_id in connected_clients {
                    self.focus_pane(updated_active_pane, client_id);
                }
                self.set_pane_active_at(updated_active_pane);
                self.set_force_render();
            },
            None => {
                // TODO: can this happen?
                self.active_panes.clear(&mut self.panes);
                self.z_indices.clear();
            },
        }
        Ok(false)
    }

    pub fn move_active_pane_down(&mut self, client_id: ClientId) {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.move_pane_down(active_pane_id).unwrap();
            self.set_force_render();
        }
    }
    pub fn move_active_pane_up(&mut self, client_id: ClientId) {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.move_pane_up(active_pane_id).unwrap();
            self.set_force_render();
        }
    }
    pub fn move_active_pane_left(&mut self, client_id: ClientId) {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.move_pane_left(active_pane_id).unwrap();
            self.set_force_render();
        }
    }
    pub fn move_active_pane_right(&mut self, client_id: ClientId) {
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.move_pane_right(active_pane_id).unwrap();
            self.set_force_render();
        }
    }
    pub fn move_clients_out_of_pane(&mut self, pane_id: PaneId) {
        let active_panes: Vec<(ClientId, PaneId)> = self
            .active_panes
            .iter()
            .map(|(cid, pid)| (*cid, *pid))
            .collect();

        // find the most recently active pane
        let mut next_active_pane_candidates: Vec<(&PaneId, &Box<dyn Pane>)> = self
            .panes
            .iter()
            .filter(|(_p_id, p)| p.selectable())
            .collect();
        next_active_pane_candidates.sort_by(|(_pane_id_a, pane_a), (_pane_id_b, pane_b)| {
            pane_a.active_at().cmp(&pane_b.active_at())
        });
        let next_active_pane_id = next_active_pane_candidates
            .last()
            .map(|(pane_id, _pane)| **pane_id);

        for (client_id, active_pane_id) in active_panes {
            if active_pane_id == pane_id {
                match next_active_pane_id {
                    Some(next_active_pane_id) => {
                        self.active_panes
                            .insert(client_id, next_active_pane_id, &mut self.panes);
                        self.focus_pane(next_active_pane_id, client_id);
                    },
                    None => {
                        self.defocus_pane(pane_id, client_id);
                    },
                }
            }
        }
    }
    pub fn focus_pane_for_all_clients(&mut self, pane_id: PaneId) {
        let connected_clients: Vec<ClientId> =
            self.connected_clients.borrow().iter().copied().collect();
        for client_id in connected_clients {
            self.active_panes
                .insert(client_id, pane_id, &mut self.panes);
        }
        self.z_indices.retain(|p_id| *p_id != pane_id);
        self.z_indices.push(pane_id);
        self.set_pane_active_at(pane_id);
        self.set_force_render();
    }
    pub fn focus_pane(&mut self, pane_id: PaneId, client_id: ClientId) {
        self.active_panes
            .insert(client_id, pane_id, &mut self.panes);
        self.focus_pane_for_all_clients(pane_id);
    }
    pub fn focus_pane_if_client_not_focused(&mut self, pane_id: PaneId, client_id: ClientId) {
        if self.active_panes.get(&client_id).is_none() {
            self.focus_pane(pane_id, client_id)
        }
    }
    pub fn defocus_pane(&mut self, pane_id: PaneId, client_id: ClientId) {
        self.z_indices.retain(|p_id| *p_id != pane_id);
        self.active_panes.remove(&client_id, &mut self.panes);
        self.set_force_render();
    }
    pub fn get_pane(&self, pane_id: PaneId) -> Option<&Box<dyn Pane>> {
        self.panes.get(&pane_id)
    }
    pub fn get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Box<dyn Pane>> {
        self.panes.get_mut(&pane_id)
    }
    pub fn get_pane_id_at(
        &self,
        point: &Position,
        search_selectable: bool,
    ) -> Result<Option<PaneId>> {
        let _err_context = || format!("failed to determine floating pane at point {point:?}");

        // TODO: better - loop through z-indices and check each one if it contains the point
        let mut panes: Vec<_> = if search_selectable {
            self.panes.iter().filter(|(_, p)| p.selectable()).collect()
        } else {
            self.panes.iter().collect()
        };
        panes.sort_by(|(a_id, _a_pane), (b_id, _b_pane)| {
            // TODO: continue
            Ord::cmp(
                &self.z_indices.iter().position(|id| id == *b_id).unwrap(),
                &self.z_indices.iter().position(|id| id == *a_id).unwrap(),
            )
        });
        Ok(panes
            .iter()
            .find(|(_, p)| p.contains(point))
            .map(|(&id, _)| id))
    }
    pub fn get_pane_at_mut(
        &mut self,
        position: &Position,
        search_selectable: bool,
    ) -> Option<&mut Box<dyn Pane>> {
        self.get_pane_id_at(position, search_selectable)
            .unwrap()
            .and_then(|pane_id| self.panes.get_mut(&pane_id))
    }
    pub fn set_pane_being_moved_with_mouse(&mut self, pane_id: PaneId, position: Position) {
        self.pane_being_moved_with_mouse = Some((pane_id, position));
    }
    pub fn pane_is_being_moved_with_mouse(&self) -> bool {
        self.pane_being_moved_with_mouse.is_some()
    }
    pub fn move_pane_to_position(&mut self, click_position: &Position) -> bool {
        // true => changed position
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        let (pane_id, previous_position) = self.pane_being_moved_with_mouse.unwrap();
        if click_position == &previous_position {
            return false;
        }
        let move_x_by = click_position.column() as isize - previous_position.column() as isize;
        let move_y_by = click_position.line() as isize - previous_position.line() as isize;
        let mut floating_pane_grid = FloatingPaneGrid::new(
            &mut self.panes,
            &mut self.desired_pane_positions,
            display_area,
            viewport,
        );
        floating_pane_grid
            .move_pane_by(pane_id, move_x_by, move_y_by)
            .unwrap();
        self.set_pane_being_moved_with_mouse(pane_id, *click_position);
        self.set_force_render();
        true
    }
    pub fn move_pane_with_mouse(&mut self, position: Position, search_selectable: bool) -> bool {
        // true => handled, false => not handled (eg. no pane at this position)
        let show_panes = self.show_panes;
        if self.pane_being_moved_with_mouse.is_some() {
            self.move_pane_to_position(&position);
            self.set_force_render();
            return true;
        } else if let Some(pane) = self.get_pane_at_mut(&position, search_selectable) {
            let clicked_on_frame = pane.position_is_on_frame(&position);
            if show_panes && clicked_on_frame {
                let pid = pane.pid();
                if self.pane_being_moved_with_mouse.is_none() {
                    self.set_pane_being_moved_with_mouse(pid, position);
                }
                self.move_pane_to_position(&position);
                self.set_force_render();
                return true;
            }
        };
        false
    }
    pub fn stop_moving_pane_with_mouse(&mut self, position: Position) {
        if self.pane_being_moved_with_mouse.is_some() {
            self.move_pane_to_position(&position);
            self.set_force_render();
        };
        self.pane_being_moved_with_mouse = None;
    }
    pub fn get_active_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        self.active_panes.get(&client_id).copied()
    }
    pub fn get_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.panes.iter()
    }
    pub fn visible_panes_count(&self) -> usize {
        self.panes.len()
    }
    pub fn drain(&mut self) -> BTreeMap<PaneId, Box<dyn Pane>> {
        self.z_indices.clear();
        self.desired_pane_positions.clear();
        match self.panes.iter().next().map(|(pid, _p)| *pid) {
            Some(first_pid) => self.panes.split_off(&first_pid),
            None => BTreeMap::new(),
        }
    }
    fn move_clients_between_panes(&mut self, from_pane_id: PaneId, to_pane_id: PaneId) {
        let clients_in_pane: Vec<ClientId> = self
            .active_panes
            .iter()
            .filter(|(_cid, pid)| **pid == from_pane_id)
            .map(|(cid, _pid)| *cid)
            .collect();
        for client_id in clients_in_pane {
            self.active_panes.remove(&client_id, &mut self.panes);
            self.active_panes
                .insert(client_id, to_pane_id, &mut self.panes);
        }
    }
}
