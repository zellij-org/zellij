mod floating_pane_grid;
use zellij_utils::position::Position;

use crate::tab::Pane;
use floating_pane_grid::FloatingPaneGrid;

use crate::{
    os_input_output::ServerOsApi,
    output::{FloatingPanesStack, Output},
    panes::PaneId,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    ClientId,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::time::Instant;
use zellij_utils::{
    data::{ModeInfo, Style},
    pane_size::{Offset, PaneGeom, Size, Viewport},
};

macro_rules! resize_pty {
    ($pane:expr, $os_input:expr) => {
        if let PaneId::Terminal(ref pid) = $pane.pid() {
            // FIXME: This `set_terminal_size_using_fd` call would be best in
            // `TerminalPane::reflow_lines`
            $os_input.set_terminal_size_using_fd(
                *pid,
                $pane.get_content_columns() as u16,
                $pane.get_content_rows() as u16,
            );
        }
    };
}

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
    active_panes: HashMap<ClientId, PaneId>,
    show_panes: bool,
    pane_being_moved_with_mouse: Option<(PaneId, Position)>,
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
            active_panes: HashMap::new(),
            pane_being_moved_with_mouse: None,
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
    ) -> Option<Box<dyn Pane>> {
        self.active_panes
            .get(&client_id)
            .copied()
            .and_then(|active_pane_id| self.replace_pane(active_pane_id, pane))
    }
    pub fn replace_pane(
        &mut self,
        pane_id: PaneId,
        mut with_pane: Box<dyn Pane>,
    ) -> Option<Box<dyn Pane>> {
        let with_pane_id = with_pane.pid();
        with_pane.set_content_offset(Offset::frame(1));
        let removed_pane = self.panes.remove(&pane_id).map(|removed_pane| {
            let removed_pane_id = removed_pane.pid();
            let with_pane_id = with_pane.pid();
            let removed_pane_geom = removed_pane.current_geom();
            with_pane.set_geom(removed_pane_geom);
            self.panes.insert(with_pane_id, with_pane);
            let z_index = self
                .z_indices
                .iter()
                .position(|pane_id| pane_id == &removed_pane_id)
                .unwrap();
            self.z_indices.remove(z_index);
            self.z_indices.insert(z_index, with_pane_id);
            removed_pane
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
    pub fn toggle_show_panes(&mut self, should_show_floating_panes: bool) {
        self.show_panes = should_show_floating_panes;
    }
    pub fn active_panes_contain(&self, client_id: &ClientId) -> bool {
        self.active_panes.contains_key(client_id)
    }
    pub fn panes_contain(&self, pane_id: &PaneId) -> bool {
        self.panes.contains_key(pane_id)
    }
    pub fn find_room_for_new_pane(&mut self) -> Option<PaneGeom> {
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
    pub fn first_floating_pane_id(&self) -> Option<PaneId> {
        self.panes.keys().next().copied()
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
    pub fn set_pane_frames(&mut self, os_api: &mut Box<dyn ServerOsApi>) {
        for pane in self.panes.values_mut() {
            // floating panes should always have a frame unless explicitly set otherwise
            if !pane.borderless() {
                pane.set_frame(true);
                pane.set_content_offset(Offset::frame(1));
            } else {
                pane.set_content_offset(Offset::default());
            }
            resize_pty!(pane, os_api);
        }
    }
    pub fn render(&mut self, output: &mut Output) {
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        let mut floating_panes: Vec<_> = self.panes.iter_mut().collect();
        floating_panes.sort_by(|(a_id, _a_pane), (b_id, _b_pane)| {
            self.z_indices
                .iter()
                .position(|id| id == *a_id)
                .unwrap()
                .cmp(&self.z_indices.iter().position(|id| id == *b_id).unwrap())
        });

        for (z_index, (kind, pane)) in floating_panes.iter_mut().enumerate() {
            let mut active_panes = self.active_panes.clone();
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
            );
            for client_id in &connected_clients {
                let client_mode = self
                    .mode_info
                    .borrow()
                    .get(client_id)
                    .unwrap_or(&self.default_mode_info)
                    .mode;
                pane_contents_and_ui.render_pane_frame(
                    *client_id,
                    client_mode,
                    self.session_is_mirrored,
                );
                if let PaneId::Plugin(..) = kind {
                    pane_contents_and_ui.render_pane_contents_for_client(*client_id);
                }
                // this is done for panes that don't have their own cursor (eg. panes of
                // another user)
                pane_contents_and_ui.render_fake_cursor_if_needed(*client_id);
            }
            if let PaneId::Terminal(..) = kind {
                pane_contents_and_ui
                    .render_pane_contents_to_multiple_clients(connected_clients.iter().copied());
            }
        }
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
        floating_pane_grid.resize(new_screen_size);
        self.set_force_render();
    }
    pub fn resize_pty_all_panes(&mut self, os_api: &mut Box<dyn ServerOsApi>) {
        for pane in self.panes.values_mut() {
            resize_pty!(pane, os_api);
        }
    }
    pub fn resize_active_pane_left(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_pane_left(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    pub fn resize_active_pane_right(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_pane_right(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    pub fn resize_active_pane_down(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_pane_down(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    pub fn resize_active_pane_up(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_pane_up(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    pub fn resize_active_pane_increase(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_increase(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    pub fn resize_active_pane_decrease(
        &mut self,
        client_id: ClientId,
        os_api: &mut Box<dyn ServerOsApi>,
    ) -> bool {
        // true => successfully resized
        let display_area = *self.display_area.borrow();
        let viewport = *self.viewport.borrow();
        if let Some(active_floating_pane_id) = self.active_panes.get(&client_id) {
            let mut floating_pane_grid = FloatingPaneGrid::new(
                &mut self.panes,
                &mut self.desired_pane_positions,
                display_area,
                viewport,
            );
            floating_pane_grid.resize_decrease(active_floating_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            self.set_force_render();
            return true;
        }
        false
    }
    fn set_pane_active_at(&mut self, pane_id: PaneId) {
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            pane.set_active_at(Instant::now());
        }
    }
    pub fn move_focus_left(
        &mut self,
        client_id: ClientId,
        connected_clients: &HashSet<ClientId>,
    ) -> bool {
        // true => successfully moved
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
            let next_index =
                floating_pane_grid.next_selectable_pane_id_to_the_left(&active_pane_id);
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
                    return true;
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
                self.active_panes.clear();
                self.z_indices.clear();
            },
        }
        false
    }
    pub fn move_focus_right(
        &mut self,
        client_id: ClientId,
        connected_clients: &HashSet<ClientId>,
    ) -> bool {
        // true => successfully moved
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
            let next_index =
                floating_pane_grid.next_selectable_pane_id_to_the_right(&active_pane_id);
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
                    return true;
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
                self.active_panes.clear();
                self.z_indices.clear();
            },
        }
        false
    }
    pub fn move_focus_up(
        &mut self,
        client_id: ClientId,
        connected_clients: &HashSet<ClientId>,
    ) -> bool {
        // true => successfully moved
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
            let next_index = floating_pane_grid.next_selectable_pane_id_above(&active_pane_id);
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

                    self.set_force_render();
                    self.set_pane_active_at(p);
                    return true;
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
                self.active_panes.clear();
                self.z_indices.clear();
            },
        }
        false
    }
    pub fn move_focus_down(
        &mut self,
        client_id: ClientId,
        connected_clients: &HashSet<ClientId>,
    ) -> bool {
        // true => successfully moved
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
            let next_index = floating_pane_grid.next_selectable_pane_id_below(&active_pane_id);
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
                    return true;
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
                self.active_panes.clear();
                self.z_indices.clear();
            },
        }
        false
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
            floating_pane_grid.move_pane_down(active_pane_id);
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
            floating_pane_grid.move_pane_up(active_pane_id);
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
            floating_pane_grid.move_pane_left(active_pane_id);
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
            floating_pane_grid.move_pane_right(active_pane_id);
            self.set_force_render();
        }
    }
    pub fn move_clients_out_of_pane(&mut self, pane_id: PaneId) {
        let active_panes: Vec<(ClientId, PaneId)> = self
            .active_panes
            .iter()
            .map(|(cid, pid)| (*cid, *pid))
            .collect();
        let next_active_pane = self.panes.keys().next().copied();
        for (client_id, active_pane_id) in active_panes {
            if active_pane_id == pane_id {
                match next_active_pane {
                    Some(next_active_pane) => {
                        self.active_panes.insert(client_id, next_active_pane);
                        self.focus_pane(next_active_pane, client_id);
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
            self.active_panes.insert(client_id, pane_id);
        }
        self.z_indices.retain(|p_id| *p_id != pane_id);
        self.z_indices.push(pane_id);
        self.set_pane_active_at(pane_id);
        self.set_force_render();
    }
    pub fn focus_pane(&mut self, pane_id: PaneId, client_id: ClientId) {
        self.active_panes.insert(client_id, pane_id);
        self.focus_pane_for_all_clients(pane_id);
    }
    pub fn defocus_pane(&mut self, pane_id: PaneId, client_id: ClientId) {
        self.z_indices.retain(|p_id| *p_id != pane_id);
        self.active_panes.remove(&client_id);
        self.set_force_render();
    }
    pub fn get_pane(&self, pane_id: PaneId) -> Option<&Box<dyn Pane>> {
        self.panes.get(&pane_id)
    }
    pub fn get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Box<dyn Pane>> {
        self.panes.get_mut(&pane_id)
    }
    pub fn get_pane_id_at(&self, point: &Position, search_selectable: bool) -> Option<PaneId> {
        if search_selectable {
            // TODO: better - loop through z-indices and check each one if it contains the point
            let mut selectable_panes: Vec<_> =
                self.panes.iter().filter(|(_, p)| p.selectable()).collect();
            selectable_panes.sort_by(|(a_id, _a_pane), (b_id, _b_pane)| {
                self.z_indices
                    .iter()
                    .position(|id| id == *b_id)
                    .unwrap()
                    .cmp(&self.z_indices.iter().position(|id| id == *a_id).unwrap())
            });
            selectable_panes
                .iter()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        } else {
            let mut panes: Vec<_> = self.panes.iter().collect();
            panes.sort_by(|(a_id, _a_pane), (b_id, _b_pane)| {
                self.z_indices
                    .iter()
                    .position(|id| id == *b_id)
                    .unwrap()
                    .cmp(&self.z_indices.iter().position(|id| id == *a_id).unwrap())
            });
            panes
                .iter()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        }
    }
    pub fn get_pane_at_mut(
        &mut self,
        position: &Position,
        search_selectable: bool,
    ) -> Option<&mut Box<dyn Pane>> {
        self.get_pane_id_at(position, search_selectable)
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
        floating_pane_grid.move_pane_by(pane_id, move_x_by, move_y_by);
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
    fn move_clients_between_panes(&mut self, from_pane_id: PaneId, to_pane_id: PaneId) {
        let clients_in_pane: Vec<ClientId> = self
            .active_panes
            .iter()
            .filter(|(_cid, pid)| **pid == from_pane_id)
            .map(|(cid, _pid)| *cid)
            .collect();
        for client_id in clients_in_pane {
            self.active_panes.remove(&client_id);
            self.active_panes.insert(client_id, to_pane_id);
        }
    }
}
