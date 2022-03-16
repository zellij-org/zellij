//! `Tab`s holds multiple panes. It tracks their coordinates (x/y) and size,
//! as well as how they should be resized

mod clipboard;
mod copy_command;
pub mod floating_pane_grid;
pub mod pane_resizer;
pub mod tiled_pane_grid;

use copy_command::CopyCommand;
use zellij_utils::input::options::Clipboard;
use zellij_utils::position::{Column, Line};
use zellij_utils::{position::Position, serde, zellij_tile};

use crate::ui::pane_boundaries_frame::FrameParams;
use tiled_pane_grid::{split, TiledPaneGrid};

use self::clipboard::ClipboardProvider;
use crate::{
    os_input_output::ServerOsApi,
    output::{CharacterChunk, Output},
    panes::FloatingPanes,
    panes::{LinkHandler, PaneId, PluginPane, TerminalPane},
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ui::boundaries::Boundaries,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::cmp::Reverse;
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Instant;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str,
};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PaletteColor};
use zellij_utils::{
    input::{
        command::TerminalAction,
        layout::{Direction, Layout, Run},
        parse_keys,
    },
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

// FIXME: This should be replaced by `RESIZE_PERCENT` at some point
pub const MIN_TERMINAL_HEIGHT: usize = 5;
pub const MIN_TERMINAL_WIDTH: usize = 5;

const MAX_PENDING_VTE_EVENTS: usize = 7000;

fn pane_content_offset(position_and_size: &PaneGeom, viewport: &Viewport) -> (usize, usize) {
    // (columns_offset, rows_offset)
    // if the pane is not on the bottom or right edge on the screen, we need to reserve one space
    // from its content to leave room for the boundary between it and the next pane (if it doesn't
    // draw its own frame)
    let columns_offset = if position_and_size.x + position_and_size.cols.as_usize() < viewport.cols
    {
        1
    } else {
        0
    };
    let rows_offset = if position_and_size.y + position_and_size.rows.as_usize() < viewport.rows {
        1
    } else {
        0
    };
    (columns_offset, rows_offset)
}

pub struct TiledPanes {
    panes: BTreeMap<PaneId, Box<dyn Pane>>,
    display_area: Rc<RefCell<Size>>,
    viewport: Rc<RefCell<Viewport>>,
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
    session_is_mirrored: bool,
    active_panes: HashMap<ClientId, PaneId>,
    draw_pane_frames: bool,
    panes_to_hide: HashSet<PaneId>,
    fullscreen_is_active: bool,
}

impl TiledPanes {
    pub fn new(display_area: Rc<RefCell<Size>>, viewport: Rc<RefCell<Viewport>>, connected_clients: Rc<RefCell<HashSet<ClientId>>>, session_is_mirrored: bool, draw_pane_frames: bool) -> Self {
        TiledPanes {
            panes: BTreeMap::new(),
            display_area,
            viewport,
            connected_clients,
            session_is_mirrored,
            active_panes: HashMap::new(),
            draw_pane_frames,
            panes_to_hide: HashSet::new(),
            fullscreen_is_active: false,
        }
    }
    pub fn add_pane(&mut self, pane_id: PaneId, pane: Box<dyn Pane>) {
        self.panes.insert(pane_id, pane);
    }
    pub fn insert_pane(&mut self, pane_id: PaneId, mut pane: Box<dyn Pane>, os_api: &mut Box<dyn ServerOsApi>) {
        // the difference between add_pane and insert_pane is that insert_pane also takes care of
        // adjusting the pane's geom as well as the geom of the panes around it
        // TODO: ideally we should only be doing this and not allowing outsiders to dictate our
        // layout to us!
        let pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        let pane_id_and_split_direction = pane_grid.find_room_for_new_pane();
        if let Some((pane_id_to_split, split_direction)) = pane_id_and_split_direction {
            // this unwrap is safe because floating panes should not be visible if there are no floating panes
            let pane_to_split = self.panes.get_mut(&pane_id_to_split).unwrap();
            let size_of_both_panes = pane_to_split.position_and_size();
            if let Some((first_geom, second_geom)) = split(split_direction, &size_of_both_panes) {
                pane_to_split.set_geom(first_geom);
                pane.set_geom(second_geom);
                self.panes
                    .insert(pane_id, pane);
                // ¯\_(ツ)_/¯
                let relayout_direction = match split_direction {
                    Direction::Vertical => Direction::Horizontal,
                    Direction::Horizontal => Direction::Vertical,
                };
                self.relayout(relayout_direction, os_api);
            }
        }
    }
    pub fn has_room_for_new_pane(&mut self) -> bool {
        let pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        pane_grid.find_room_for_new_pane().is_some()
    }
    pub fn fixed_pane_geoms(&self) -> Vec<Viewport> {
        self.panes.values().filter_map(|p| {
            let geom = p.position_and_size();
            if geom.cols.is_fixed() || geom.rows.is_fixed() {
                Some(geom.into())
            } else {
                None
            }
        })
        .collect()
    }
    pub fn first_selectable_pane_id(&self) -> Option<PaneId> {
        self
            .panes
            .iter()
            .filter(|(_id, pane)| pane.selectable())
            .map(|(id, _)| id.to_owned())
            .next()
    }
    pub fn pane_ids(&self) -> impl Iterator<Item = &PaneId> {
        self.panes.keys()
    }
    pub fn relayout(&mut self, direction: Direction, os_api: &mut Box<dyn ServerOsApi>) {
        let mut pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        let result = match direction {
            Direction::Horizontal => {
                pane_grid.layout(direction, (*self.display_area.borrow()).cols)
            }
            Direction::Vertical => pane_grid.layout(direction, (*self.display_area.borrow()).rows),
        };
        if let Err(e) = &result {
            log::error!("{:?} relayout of the tab failed: {}", direction, e);
        }
        self.set_pane_frames(self.draw_pane_frames, os_api);
    }
    pub fn set_pane_frames(&mut self, draw_pane_frames: bool, os_api: &mut Box<dyn ServerOsApi>) {
        // TODO: the original method had a should_clear_display_before_rendering = true - make sure
        // to always do this in the new paths we are not refactoring
        self.draw_pane_frames = draw_pane_frames;
        let viewport = *self.viewport.borrow();
        for pane in self.panes.values_mut() {
            if !pane.borderless() {
                pane.set_frame(draw_pane_frames);
            }

            #[allow(clippy::if_same_then_else)]
            if draw_pane_frames & !pane.borderless() {
                // there's definitely a frame around this pane, offset its contents
                pane.set_content_offset(Offset::frame(1));
            } else if draw_pane_frames && pane.borderless() {
                // there's no frame around this pane, and the tab isn't handling the boundaries
                // between panes (they each draw their own frames as they please)
                // this one doesn't - do not offset its content
                pane.set_content_offset(Offset::default());
            } else if !is_inside_viewport(&viewport, pane) {
                // this pane is outside the viewport and has no border - it should not have an offset
                pane.set_content_offset(Offset::default());
            } else {
                // no draw_pane_frames and this pane should have a separation to other panes
                // according to its position in the viewport (eg. no separation if its at the
                // viewport bottom) - offset its content accordingly
                let position_and_size = pane.current_geom();
                let (pane_columns_offset, pane_rows_offset) =
                    pane_content_offset(&position_and_size, &viewport);
                pane.set_content_offset(Offset::shift(pane_rows_offset, pane_columns_offset));
            }

            resize_pty!(pane, os_api);
        }
    }
    pub fn can_split_pane_horizontally(&mut self, client_id: ClientId) -> bool {
        if let Some(active_pane_id) = &self.active_panes.get(&client_id) {
            if let Some(active_pane) = self.panes.get_mut(active_pane_id) {
                let full_pane_size = active_pane.position_and_size();
                if full_pane_size.rows.as_usize() < MIN_TERMINAL_HEIGHT * 2 {
                    return false;
                } else {
                    return split(Direction::Horizontal, &full_pane_size).is_some();
                }
            }
        }
        false
    }
    pub fn can_split_pane_vertically(&mut self, client_id: ClientId) -> bool {
        if let Some(active_pane_id) = &self.active_panes.get(&client_id) {
            if let Some(active_pane) = self.panes.get_mut(active_pane_id) {
                let full_pane_size = active_pane.position_and_size();
                if full_pane_size.cols.as_usize() < MIN_TERMINAL_WIDTH * 2 {
                    return false;
                }
                return split(Direction::Vertical, &full_pane_size).is_some();
            }
        }
        false
    }
    pub fn split_pane_horizontally(&mut self, pid: PaneId, mut new_pane: Box<dyn Pane>, os_api: &mut Box<dyn ServerOsApi>, client_id: ClientId) {
        let active_pane_id = &self.active_panes.get(&client_id).unwrap();
        let active_pane = self.panes.get_mut(active_pane_id).unwrap();
        let full_pane_size = active_pane.position_and_size();
        if let Some((top_winsize, bottom_winsize)) = split(Direction::Horizontal, &full_pane_size) {
            active_pane.set_geom(top_winsize);
            new_pane.set_geom(bottom_winsize);
            self.panes.insert(pid, new_pane);
            self.relayout(Direction::Vertical, os_api);
        }
    }
    pub fn split_pane_vertically(&mut self, pid: PaneId, mut new_pane: Box<dyn Pane>, os_api: &mut Box<dyn ServerOsApi>, client_id: ClientId) {
        let active_pane_id = &self.active_panes.get(&client_id).unwrap();
        let active_pane = self.panes.get_mut(active_pane_id).unwrap();
        let full_pane_size = active_pane.position_and_size();
        if let Some((left_winsize, right_winsize)) = split(Direction::Vertical, &full_pane_size) {
            active_pane.set_geom(left_winsize);
            new_pane.set_geom(right_winsize);
            self.panes.insert(pid, new_pane);
            self.relayout(Direction::Horizontal, os_api);
        }
    }
    pub fn focus_pane(&mut self, pane_id: PaneId, client_id: ClientId) {
        self.active_panes.insert(client_id, pane_id);
        if self.session_is_mirrored {
            // move all clients
            let connected_clients: Vec<ClientId> = self.connected_clients.borrow().iter().copied().collect();
            for client_id in connected_clients {
                self.active_panes.insert(client_id, pane_id);
            }
        }
    }
    pub fn clear_active_panes(&mut self) {
        self.active_panes.clear();
    }
    pub fn first_active_pane_id(&self) -> Option<PaneId> {
        self.connected_clients.borrow().iter().next().and_then(|first_client_id| {
            self.active_panes.get(first_client_id).copied()
        })
    }
    pub fn focused_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        self.active_panes.get(&client_id).copied()
    }
    pub fn get_pane(&self, pane_id: PaneId) -> Option<&Box<dyn Pane>> {
        self.panes.get(&pane_id)
    }
    pub fn get_pane_mut(&mut self, pane_id: PaneId) -> Option<&mut Box<dyn Pane>> {
        self.panes.get_mut(&pane_id)
    }
    pub fn get_active_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        self.active_panes.get(&client_id).copied()
    }
    pub fn panes_contain(&self, pane_id: &PaneId) -> bool {
        self.panes.contains_key(pane_id)
    }
    pub fn set_force_render(&mut self) {
        for pane in self.panes.values_mut() {
            pane.set_should_render(true);
            pane.set_should_render_boundaries(true);
            pane.render_full_viewport();
        }
    }
    pub fn has_active_panes(&self) -> bool {
        !self.active_panes.is_empty()
    }
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        connected_clients_in_app: &Rc<RefCell<HashSet<ClientId>>>,
        connected_clients: &HashSet<ClientId>,
        mode_info: &HashMap<ClientId, ModeInfo>,
        default_mode_info: &ModeInfo,
        session_is_mirrored: bool,
        output: &mut Output,
        colors: Palette,
        multiple_users_exist_in_session: bool,
        do_not_color_active_panes: bool,
    ) {
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        let mut client_id_to_boundaries: HashMap<ClientId, Boundaries> = HashMap::new();
        let active_panes = self.active_panes.iter().filter(|(client_id, _pane_id)| connected_clients.contains(client_id)).map(|(client_id, pane_id)| (*client_id, *pane_id)).collect();
        for (kind, pane) in self.panes.iter_mut() {
            if !self.panes_to_hide.contains(&pane.pid()) {
                let mut pane_contents_and_ui = PaneContentsAndUi::new(
                    pane,
                    output,
                    colors,
                    &active_panes,
                    multiple_users_exist_in_session,
                    None,
                );
                if do_not_color_active_panes {
                    pane_contents_and_ui.clear_focused_clients();
                }
                for client_id in &connected_clients {
                    let client_mode = mode_info
                        .get(&client_id)
                        .unwrap_or(default_mode_info)
                        .mode;
                    if let PaneId::Plugin(..) = kind {
                        pane_contents_and_ui.render_pane_contents_for_client(*client_id);
                    }
                    if self.draw_pane_frames {
                        pane_contents_and_ui.render_pane_frame(
                            *client_id,
                            client_mode,
                            self.session_is_mirrored,
                        );
                    } else {
                        let boundaries = client_id_to_boundaries
                            .entry(*client_id)
                            .or_insert_with(|| Boundaries::new(*self.viewport.borrow()));
                        pane_contents_and_ui.render_pane_boundaries(
                            *client_id,
                            client_mode,
                            boundaries,
                            self.session_is_mirrored,
                        );
                    }
                    pane_contents_and_ui.render_terminal_title_if_needed(*client_id, client_mode);
                    // this is done for panes that don't have their own cursor (eg. panes of
                    // another user)
                    pane_contents_and_ui.render_fake_cursor_if_needed(*client_id);
                }
                if let PaneId::Terminal(..) = kind {
                    pane_contents_and_ui.render_pane_contents_to_multiple_clients(
                        connected_clients.iter().copied(),
                    );
                }
            }
        }
        // render boundaries if needed
        for (client_id, boundaries) in &mut client_id_to_boundaries {
            // TODO: add some conditional rendering here so this isn't rendered for every character
            output.add_character_chunks_to_client(*client_id, boundaries.render(), None);
        }
    }
    pub fn get_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.panes.iter()
    }
    pub fn resize(&mut self, new_screen_size: Size, os_api: &mut Box<dyn ServerOsApi>) {
        // this is blocked out to appease the borrow checker
        {
            let mut display_area = self.display_area.borrow_mut();
            let mut viewport = self.viewport.borrow_mut();
            let panes = self
                .panes
                .iter_mut()
                .filter(|(pid, _)| !self.panes_to_hide.contains(pid));
            let Size { rows, cols } = new_screen_size;
            let mut pane_grid = TiledPaneGrid::new(panes, *display_area, *viewport);
            if pane_grid.layout(Direction::Horizontal, cols).is_ok() {
                let column_difference = cols as isize - display_area.cols as isize;
                // FIXME: Should the viewport be an Offset?
                viewport.cols = (viewport.cols as isize + column_difference) as usize;
                display_area.cols = cols;
            } else {
                log::error!("Failed to horizontally resize the tab!!!");
            }
            if pane_grid.layout(Direction::Vertical, rows).is_ok() {
                let row_difference = rows as isize - display_area.rows as isize;
                viewport.rows = (viewport.rows as isize + row_difference) as usize;
                display_area.rows = rows;
            } else {
                log::error!("Failed to vertically resize the tab!!!");
            }
        }
        self.set_pane_frames(self.draw_pane_frames, os_api);
    }
    pub fn resize_active_pane_left(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_pane_left(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn resize_active_pane_right(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_pane_right(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn resize_active_pane_up(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_pane_up(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn resize_active_pane_down(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_pane_down(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn resize_active_pane_increase(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_increase(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn resize_active_pane_decrease(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let mut pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            pane_grid.resize_decrease(&active_pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
        }
    }
    pub fn focus_next_pane(&mut self, client_id: ClientId) {
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        let active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        let next_active_pane_id = pane_grid.next_selectable_pane_id(&active_pane_id);
        for client_id in connected_clients {
            self.active_panes.insert(client_id, next_active_pane_id);
        }
        self.set_pane_active_at(next_active_pane_id);
    }
    pub fn focus_previous_pane(&mut self, client_id: ClientId) {
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        let active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        let next_active_pane_id = pane_grid.previous_selectable_pane_id(&active_pane_id);
        for client_id in connected_clients {
            self.active_panes.insert(client_id, next_active_pane_id);
        }
        self.set_pane_active_at(next_active_pane_id);
    }
    fn set_pane_active_at(&mut self, pane_id: PaneId) {
        if let Some(pane) = self.get_pane_mut(pane_id) {
            pane.set_active_at(Instant::now());
        }
    }
    fn move_focus_left(&mut self, client_id: ClientId) -> bool {
        match self.get_active_pane_id(client_id) {
            Some(active_pane_id) => {
                let pane_grid = TiledPaneGrid::new(
                    &mut self.panes,
                    *self.display_area.borrow(),
                    *self.viewport.borrow(),
                );
                let next_index = pane_grid.next_selectable_pane_id_to_the_left(&active_pane_id);
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

                        self.focus_pane(p, client_id);
                        self.set_pane_active_at(p);

                        return true;
                    }
                    None =>  {
                        return false;
                    }
                }
            },
            None => {
                return false;
            }
        }
    }
    fn move_focus_down(&mut self, client_id: ClientId) -> bool {
        match self.get_active_pane_id(client_id) {
            Some(active_pane_id) => {
                let pane_grid = TiledPaneGrid::new(
                    &mut self.panes,
                    *self.display_area.borrow(),
                    *self.viewport.borrow(),
                );
                let next_index = pane_grid.next_selectable_pane_id_below(&active_pane_id);
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

                        self.focus_pane(p, client_id);
                        self.set_pane_active_at(p);

                        return true;
                    }
                    None =>  {
                        return false;
                    }
                }
            },
            None => {
                return false;
            }
        }
    }
    fn move_focus_up(&mut self, client_id: ClientId) -> bool {
        match self.get_active_pane_id(client_id) {
            Some(active_pane_id) => {
                let pane_grid = TiledPaneGrid::new(
                    &mut self.panes,
                    *self.display_area.borrow(),
                    *self.viewport.borrow(),
                );
                let next_index = pane_grid.next_selectable_pane_id_above(&active_pane_id);
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

                        self.focus_pane(p, client_id);
                        self.set_pane_active_at(p);

                        return true;
                    }
                    None =>  {
                        return false;
                    }
                }
            },
            None => {
                return false;
            }
        }
    }
    fn move_focus_right(&mut self, client_id: ClientId) -> bool {
        match self.get_active_pane_id(client_id) {
            Some(active_pane_id) => {
                let pane_grid = TiledPaneGrid::new(
                    &mut self.panes,
                    *self.display_area.borrow(),
                    *self.viewport.borrow(),
                );
                let next_index = pane_grid.next_selectable_pane_id_to_the_right(&active_pane_id);
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

                        self.focus_pane(p, client_id);
                        self.set_pane_active_at(p);

                        return true;
                    }
                    None =>  {
                        return false;
                    }
                }
            },
            None => {
                return false;
            }
        }
    }
    pub fn move_active_pane(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        let active_pane_id = self.get_active_pane_id(client_id).unwrap();
        let pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        let new_position_id = pane_grid.next_selectable_pane_id(&active_pane_id);
        let current_position = self.panes.get(&active_pane_id).unwrap();
        let prev_geom = current_position.position_and_size();
        let prev_geom_override = current_position.geom_override();

        let new_position = self.panes.get_mut(&new_position_id).unwrap();
        let next_geom = new_position.position_and_size();
        let next_geom_override = new_position.geom_override();
        new_position.set_geom(prev_geom);
        if let Some(geom) = prev_geom_override {
            new_position.get_geom_override(geom);
        }
        resize_pty!(new_position, os_api);
        new_position.set_should_render(true);

        let current_position = self.panes.get_mut(&active_pane_id).unwrap();
        current_position.set_geom(next_geom);
        if let Some(geom) = next_geom_override {
            current_position.get_geom_override(geom);
        }
        resize_pty!(current_position, os_api);
        current_position.set_should_render(true);
    }
    pub fn move_active_pane_down(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            let next_index = pane_grid.next_selectable_pane_id_below(&active_pane_id);
            if let Some(p) = next_index {
                let active_pane_id = self.active_panes.get(&client_id).unwrap();
                let current_position = self.panes.get(active_pane_id).unwrap();
                let prev_geom = current_position.position_and_size();
                let prev_geom_override = current_position.geom_override();

                let new_position = self.panes.get_mut(&p).unwrap();
                let next_geom = new_position.position_and_size();
                let next_geom_override = new_position.geom_override();
                new_position.set_geom(prev_geom);
                if let Some(geom) = prev_geom_override {
                    new_position.get_geom_override(geom);
                }
                resize_pty!(new_position, os_api);
                new_position.set_should_render(true);

                let current_position = self.panes.get_mut(active_pane_id).unwrap();
                current_position.set_geom(next_geom);
                if let Some(geom) = next_geom_override {
                    current_position.get_geom_override(geom);
                }
                resize_pty!(current_position, os_api);
                current_position.set_should_render(true);
            }
        }
    }
    pub fn move_active_pane_left(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            let next_index = pane_grid.next_selectable_pane_id_to_the_left(&active_pane_id);
            if let Some(p) = next_index {
                let active_pane_id = self.active_panes.get(&client_id).unwrap();
                let current_position = self.panes.get(active_pane_id).unwrap();
                let prev_geom = current_position.position_and_size();
                let prev_geom_override = current_position.geom_override();

                let new_position = self.panes.get_mut(&p).unwrap();
                let next_geom = new_position.position_and_size();
                let next_geom_override = new_position.geom_override();
                new_position.set_geom(prev_geom);
                if let Some(geom) = prev_geom_override {
                    new_position.get_geom_override(geom);
                }
                resize_pty!(new_position, os_api);
                new_position.set_should_render(true);

                let current_position = self.panes.get_mut(active_pane_id).unwrap();
                current_position.set_geom(next_geom);
                if let Some(geom) = next_geom_override {
                    current_position.get_geom_override(geom);
                }
                resize_pty!(current_position, os_api);
                current_position.set_should_render(true);
            }
        }
    }
    pub fn move_active_pane_right(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            let next_index = pane_grid.next_selectable_pane_id_to_the_right(&active_pane_id);
            if let Some(p) = next_index {
                let active_pane_id = self.active_panes.get(&client_id).unwrap();
                let current_position = self.panes.get(active_pane_id).unwrap();
                let prev_geom = current_position.position_and_size();
                let prev_geom_override = current_position.geom_override();

                let new_position = self.panes.get_mut(&p).unwrap();
                let next_geom = new_position.position_and_size();
                let next_geom_override = new_position.geom_override();
                new_position.set_geom(prev_geom);
                if let Some(geom) = prev_geom_override {
                    new_position.get_geom_override(geom);
                }
                resize_pty!(new_position, os_api);
                new_position.set_should_render(true);

                let current_position = self.panes.get_mut(active_pane_id).unwrap();
                current_position.set_geom(next_geom);
                if let Some(geom) = next_geom_override {
                    current_position.get_geom_override(geom);
                }
                resize_pty!(current_position, os_api);
                current_position.set_should_render(true);
            }
        }
    }
    pub fn move_active_pane_up(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            let pane_grid = TiledPaneGrid::new(
                &mut self.panes,
                *self.display_area.borrow(),
                *self.viewport.borrow(),
            );
            let next_index = pane_grid.next_selectable_pane_id_above(&active_pane_id);
            if let Some(p) = next_index {
                let active_pane_id = self.active_panes.get(&client_id).unwrap();
                let current_position = self.panes.get(active_pane_id).unwrap();
                let prev_geom = current_position.position_and_size();
                let prev_geom_override = current_position.geom_override();

                let new_position = self.panes.get_mut(&p).unwrap();
                let next_geom = new_position.position_and_size();
                let next_geom_override = new_position.geom_override();
                new_position.set_geom(prev_geom);
                if let Some(geom) = prev_geom_override {
                    new_position.get_geom_override(geom);
                }
                resize_pty!(new_position, os_api);
                new_position.set_should_render(true);

                let current_position = self.panes.get_mut(active_pane_id).unwrap();
                current_position.set_geom(next_geom);
                if let Some(geom) = next_geom_override {
                    current_position.get_geom_override(geom);
                }
                resize_pty!(current_position, os_api);
                current_position.set_should_render(true);
            }
        }
    }
    fn move_clients_out_of_pane(&mut self, pane_id: PaneId) {
        let active_panes: Vec<(ClientId, PaneId)> = self
            .active_panes
            .iter()
            .map(|(cid, pid)| (*cid, *pid))
            .collect();
        match self.panes.iter().find(|(p_id, p)| **p_id != pane_id && p.selectable()).map(|(p_id, _p)| p_id) {
            Some(next_active_pane) => {
                for (client_id, active_pane_id) in active_panes {
                    if active_pane_id == pane_id {
                        self.active_panes.insert(
                            client_id,
                            *next_active_pane
                        );
                    }
                }
            },
            None => self.active_panes.clear()
        }
    }
    pub fn remove_pane(&mut self, pane_id: PaneId, os_api: &mut Box<dyn ServerOsApi>) -> Option<Box<dyn Pane>> {
        let mut pane_grid = TiledPaneGrid::new(
            &mut self.panes,
            *self.display_area.borrow(),
            *self.viewport.borrow(),
        );
        if pane_grid.fill_space_over_pane(pane_id) {
            // successfully filled space over pane
            let closed_pane = self.panes.remove(&pane_id);
            self.move_clients_out_of_pane(pane_id);
            for pane in self.panes.values_mut() {
                resize_pty!(pane, os_api);
            }
            closed_pane
        } else {
            self.panes.remove(&pane_id);
            // this is a bit of a roundabout way to say: this is the last pane and so the tab
            // should be destroyed
            self.active_panes.clear();
            None
        }
    }
    pub fn panes_to_hide_contains(&self, pane_id: PaneId) -> bool {
        self.panes_to_hide.contains(&pane_id)
    }
    pub fn fullscreen_is_active(&self) -> bool {
        self.fullscreen_is_active
    }
    pub fn unset_fullscreen(&mut self, os_api: &mut Box<dyn ServerOsApi>) {
        if self.fullscreen_is_active {
            let first_client_id = {
                let connected_clients = self.connected_clients.borrow();
                *connected_clients.iter().next().unwrap()
            };
            let active_pane_id = self.get_active_pane_id(first_client_id).unwrap();
            let panes_to_hide: Vec<_> = self.panes_to_hide.iter().copied().collect();
            for pane_id in panes_to_hide {
                let pane = self.get_pane_mut(pane_id).unwrap();
                pane.set_should_render(true);
                pane.set_should_render_boundaries(true);
            }
            let viewport_pane_ids: Vec<_> = self
                .panes
                .keys()
                .copied()
                .into_iter()
                .filter(|id| !is_inside_viewport(&*self.viewport.borrow(), self.get_pane(*id).unwrap()))
                .collect();
            for pid in viewport_pane_ids {
                let viewport_pane = self.get_pane_mut(pid).unwrap();
                viewport_pane.reset_size_and_position_override();
            }
            self.panes_to_hide.clear();
            let active_terminal = self.get_pane_mut(active_pane_id).unwrap();
            active_terminal.reset_size_and_position_override();
            self.set_force_render();
            let display_area = *self.display_area.borrow();
            self.resize(display_area, os_api);
            self.fullscreen_is_active = false;
        }
    }
    pub fn toggle_active_pane_fullscreen(&mut self, client_id: ClientId, os_api: &mut Box<dyn ServerOsApi>) {
        if let Some(active_pane_id) = self.get_active_pane_id(client_id) {
            if self.fullscreen_is_active {
                self.unset_fullscreen(os_api);
            } else {
                let pane_ids_to_hide = self.panes.iter().filter_map(|(&id, _pane)| {
                    if id != active_pane_id && is_inside_viewport(&*self.viewport.borrow(), self.get_pane(id).unwrap()) {
                        Some(id)
                    } else {
                        None
                    }
                });
                self.panes_to_hide = pane_ids_to_hide.collect();
                if self.panes_to_hide.is_empty() {
                    // nothing to do, pane is already as fullscreen as it can be, let's bail
                    return;
                } else {
                    // For all of the panes outside of the viewport staying on the fullscreen
                    // screen, switch them to using override positions as well so that the resize
                    // system doesn't get confused by viewport and old panes that no longer line up
                    let viewport_pane_ids: Vec<_> = self
                        .panes
                        .keys()
                        .copied()
                        .into_iter()
                        .filter(|id| !is_inside_viewport(&*self.viewport.borrow(), self.get_pane(*id).unwrap()))
                        .collect();
                    for pid in viewport_pane_ids {
                        let viewport_pane = self.get_pane_mut(pid).unwrap();
                        viewport_pane.get_geom_override(viewport_pane.position_and_size());
                    }
                    let viewport = { *self.viewport.borrow() };
                    let active_terminal = self.get_pane_mut(active_pane_id).unwrap();
                    let full_screen_geom = PaneGeom {
                        x: viewport.x,
                        y: viewport.y,
                        ..Default::default()
                    };
                    active_terminal.get_geom_override(full_screen_geom);
                }
                let connected_client_list: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
                for client_id in connected_client_list {
                    self.focus_pane(active_pane_id, client_id);
                }
                self.set_force_render();
                let display_area = *self.display_area.borrow();
                self.resize(display_area, os_api);
                self.fullscreen_is_active = true;
            }
        }
    }
    pub fn panes_to_hide_count(&self) -> usize {
        self.panes_to_hide.len()
    }
}

pub(crate) struct Tab {
    pub index: usize,
    pub position: usize,
    pub name: String,
    tiled_panes: TiledPanes,
    floating_panes: FloatingPanes,
    max_panes: Option<usize>,
    viewport: Rc<RefCell<Viewport>>, // includes all non-UI panes
    display_area: Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
    os_api: Box<dyn ServerOsApi>,
    pub senders: ThreadSenders,
    synchronize_is_active: bool,
    should_clear_display_before_rendering: bool,
    mode_info: HashMap<ClientId, ModeInfo>,
    default_mode_info: ModeInfo,
    pub colors: Palette,
    connected_clients_in_app: Rc<RefCell<HashSet<ClientId>>>, // TODO: combine this and connected_clients
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
    draw_pane_frames: bool,
    session_is_mirrored: bool,
    pending_vte_events: HashMap<RawFd, Vec<VteBytes>>,
    pub selecting_with_mouse: bool, // this is only pub for the tests TODO: remove this once we combine write_text_to_clipboard with render
    link_handler: Rc<RefCell<LinkHandler>>,
    clipboard_provider: ClipboardProvider,
    // TODO: used only to focus the pane when the layout is loaded
    // it seems that optimization is possible using `active_panes`
    focus_pane_id: Option<PaneId>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub(crate) struct TabData {
    pub position: usize,
    pub name: String,
    pub active: bool,
    pub mode_info: ModeInfo,
    pub colors: Palette,
}

// FIXME: Use a struct that has a pane_type enum, to reduce all of the duplication
pub trait Pane {
    fn x(&self) -> usize;
    fn y(&self) -> usize;
    fn rows(&self) -> usize;
    fn cols(&self) -> usize;
    fn get_content_x(&self) -> usize;
    fn get_content_y(&self) -> usize;
    fn get_content_columns(&self) -> usize;
    fn get_content_rows(&self) -> usize;
    fn reset_size_and_position_override(&mut self);
    fn set_geom(&mut self, position_and_size: PaneGeom);
    fn get_geom_override(&mut self, pane_geom: PaneGeom);
    fn handle_pty_bytes(&mut self, bytes: VteBytes);
    fn cursor_coordinates(&self) -> Option<(usize, usize)>;
    fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8>;
    fn position_and_size(&self) -> PaneGeom;
    fn current_geom(&self) -> PaneGeom;
    fn geom_override(&self) -> Option<PaneGeom>;
    fn should_render(&self) -> bool;
    fn set_should_render(&mut self, should_render: bool);
    fn set_should_render_boundaries(&mut self, _should_render: bool) {}
    fn selectable(&self) -> bool;
    fn set_selectable(&mut self, selectable: bool);
    fn render(
        &mut self,
        client_id: Option<ClientId>,
    ) -> Option<(Vec<CharacterChunk>, Option<String>)>; // TODO: better
    fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Option<(Vec<CharacterChunk>, Option<String>)>; // TODO: better
    fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<String>;
    fn render_terminal_title(&mut self, _input_mode: InputMode) -> String;
    fn update_name(&mut self, name: &str);
    fn pid(&self) -> PaneId;
    fn reduce_height(&mut self, percent: f64);
    fn increase_height(&mut self, percent: f64);
    fn reduce_width(&mut self, percent: f64);
    fn increase_width(&mut self, percent: f64);
    fn push_down(&mut self, count: usize);
    fn push_right(&mut self, count: usize);
    fn pull_left(&mut self, count: usize);
    fn pull_up(&mut self, count: usize);
    fn scroll_up(&mut self, count: usize, client_id: ClientId);
    fn scroll_down(&mut self, count: usize, client_id: ClientId);
    fn clear_scroll(&mut self);
    fn is_scrolled(&self) -> bool;
    fn active_at(&self) -> Instant;
    fn set_active_at(&mut self, instant: Instant);
    fn set_frame(&mut self, frame: bool);
    fn set_content_offset(&mut self, offset: Offset);
    fn cursor_shape_csi(&self) -> String {
        "\u{1b}[0 q".to_string() // default to non blinking block
    }
    fn contains(&self, position: &Position) -> bool {
        match self.geom_override() {
            Some(position_and_size) => position_and_size.contains(position),
            None => self.position_and_size().contains(position),
        }
    }
    fn start_selection(&mut self, _start: &Position, _client_id: ClientId) {}
    fn update_selection(&mut self, _position: &Position, _client_id: ClientId) {}
    fn end_selection(&mut self, _end: &Position, _client_id: ClientId) {}
    fn reset_selection(&mut self) {}
    fn get_selected_text(&self) -> Option<String> {
        None
    }

    fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.cols()
    }
    fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    fn is_right_of(&self, other: &dyn Pane) -> bool {
        self.x() > other.x()
    }
    fn is_directly_right_of(&self, other: &dyn Pane) -> bool {
        self.x() == other.x() + other.cols()
    }
    fn is_left_of(&self, other: &dyn Pane) -> bool {
        self.x() < other.x()
    }
    fn is_directly_left_of(&self, other: &dyn Pane) -> bool {
        self.x() + self.cols() == other.x()
    }
    fn is_below(&self, other: &dyn Pane) -> bool {
        self.y() > other.y()
    }
    fn is_directly_below(&self, other: &dyn Pane) -> bool {
        self.y() == other.y() + other.rows()
    }
    fn is_above(&self, other: &dyn Pane) -> bool {
        self.y() < other.y()
    }
    fn is_directly_above(&self, other: &dyn Pane) -> bool {
        self.y() + self.rows() == other.y()
    }
    fn horizontally_overlaps_with(&self, other: &dyn Pane) -> bool {
        (self.y() >= other.y() && self.y() < (other.y() + other.rows()))
            || ((self.y() + self.rows()) <= (other.y() + other.rows())
                && (self.y() + self.rows()) > other.y())
            || (self.y() <= other.y() && (self.y() + self.rows() >= (other.y() + other.rows())))
            || (other.y() <= self.y() && (other.y() + other.rows() >= (self.y() + self.rows())))
    }
    fn get_horizontal_overlap_with(&self, other: &dyn Pane) -> usize {
        std::cmp::min(self.y() + self.rows(), other.y() + other.rows())
            - std::cmp::max(self.y(), other.y())
    }
    fn vertically_overlaps_with(&self, other: &dyn Pane) -> bool {
        (self.x() >= other.x() && self.x() < (other.x() + other.cols()))
            || ((self.x() + self.cols()) <= (other.x() + other.cols())
                && (self.x() + self.cols()) > other.x())
            || (self.x() <= other.x() && (self.x() + self.cols() >= (other.x() + other.cols())))
            || (other.x() <= self.x() && (other.x() + other.cols() >= (self.x() + self.cols())))
    }
    fn get_vertical_overlap_with(&self, other: &dyn Pane) -> usize {
        std::cmp::min(self.x() + self.cols(), other.x() + other.cols())
            - std::cmp::max(self.x(), other.x())
    }
    fn can_reduce_height_by(&self, reduce_by: usize) -> bool {
        self.rows() > reduce_by && self.rows() - reduce_by >= self.min_height()
    }
    fn can_reduce_width_by(&self, reduce_by: usize) -> bool {
        self.cols() > reduce_by && self.cols() - reduce_by >= self.min_width()
    }
    fn min_width(&self) -> usize {
        MIN_TERMINAL_WIDTH
    }
    fn min_height(&self) -> usize {
        MIN_TERMINAL_HEIGHT
    }
    fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        // TODO: this is only relevant to terminal panes
        // we should probably refactor away from this trait at some point
        vec![]
    }
    fn render_full_viewport(&mut self) {}
    fn relative_position(&self, position_on_screen: &Position) -> Position {
        position_on_screen.relative_to(self.get_content_y(), self.get_content_x())
    }
    fn position_is_on_frame(&self, position_on_screen: &Position) -> bool {
        // TODO: handle cases where we have no frame
        position_on_screen.line() == self.y() as isize
            || position_on_screen.line()
                == (self.y() as isize + self.rows() as isize).saturating_sub(1)
            || position_on_screen.column() == self.x()
            || position_on_screen.column() == (self.x() + self.cols()).saturating_sub(1)
    }
    fn set_borderless(&mut self, borderless: bool);
    fn borderless(&self) -> bool;
    fn handle_right_click(&mut self, _to: &Position, _client_id: ClientId) {}
    fn mouse_mode(&self) -> bool;
}

impl Tab {
    // FIXME: Still too many arguments for clippy to be happy...
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: usize,
        position: usize,
        name: String,
        display_area: Size,
        os_api: Box<dyn ServerOsApi>,
        senders: ThreadSenders,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        colors: Palette,
        draw_pane_frames: bool,
        connected_clients_in_app: Rc<RefCell<HashSet<ClientId>>>,
        session_is_mirrored: bool,
        client_id: ClientId,
        copy_command: Option<String>,
        copy_clipboard: Clipboard,
    ) -> Self {

        let name = if name.is_empty() {
            format!("Tab #{}", index + 1)
        } else {
            name
        };

        let mut connected_clients = HashSet::new();
        connected_clients.insert(client_id);
        let viewport: Viewport = display_area.into();
        let viewport = Rc::new(RefCell::new(viewport));
        let display_area = Rc::new(RefCell::new(display_area));
        let connected_clients = Rc::new(RefCell::new(connected_clients));
        let tiled_panes = TiledPanes::new(display_area.clone(), viewport.clone(), connected_clients.clone(), session_is_mirrored, draw_pane_frames);
        let floating_panes = FloatingPanes::new(display_area.clone(), viewport.clone(), connected_clients.clone());

        let clipboard_provider = match copy_command {
            Some(command) => ClipboardProvider::Command(CopyCommand::new(command)),
            None => ClipboardProvider::Osc52(copy_clipboard),
        };

        Tab {
            index,
            position,
            tiled_panes,
            floating_panes,
            name,
            max_panes,
            viewport,
            display_area,
            synchronize_is_active: false,
            os_api,
            senders,
            should_clear_display_before_rendering: false,
            mode_info: HashMap::new(),
            default_mode_info: mode_info,
            colors,
            draw_pane_frames,
            session_is_mirrored,
            pending_vte_events: HashMap::new(),
            connected_clients_in_app,
            connected_clients,
            selecting_with_mouse: false,
            link_handler: Rc::new(RefCell::new(LinkHandler::new())),
            clipboard_provider,
            focus_pane_id: None,
        }
    }

    pub fn apply_layout(
        &mut self,
        layout: Layout,
        new_pids: Vec<RawFd>,
        tab_index: usize,
        client_id: ClientId,
    ) {
        // TODO: this should be an attribute on Screen instead of full_screen_ws
        let (viewport_cols, viewport_rows) = {
            let viewport = self.viewport.borrow();
            (viewport.cols, viewport.rows)
        };
        let mut free_space = PaneGeom::default();
        free_space.cols.set_inner(viewport_cols);
        free_space.rows.set_inner(viewport_rows);

        let positions_in_layout = layout.position_panes_in_space(&free_space);

        let mut positions_and_size = positions_in_layout.iter();
        //         TODO: can we get rid of this? ideally by making apply_layout part of the tab
        //         constructor?
//         for (pane_kind, terminal_pane) in &mut self.tiled_panes {
//             // for now the layout only supports terminal panes
//             if let PaneId::Terminal(pid) = pane_kind {
//                 match positions_and_size.next() {
//                     Some(&(_, position_and_size)) => {
//                         terminal_pane.reset_size_and_position_override();
//                         terminal_pane.set_geom(position_and_size);
//                     }
//                     None => {
//                         // we filled the entire layout, no room for this pane
//                         // TODO: handle active terminal
//                         self.panes_to_hide.insert(PaneId::Terminal(*pid));
//                     }
//                 }
//             }
//         }
        let mut new_pids = new_pids.iter();

        let mut focus_pane_id: Option<PaneId> = None;
        let mut set_focus_pane_id = |layout: &Layout, pane_id: PaneId| {
            if layout.focus.unwrap_or(false) && focus_pane_id.is_none() {
                focus_pane_id = Some(pane_id);
            }
        };

        for (layout, position_and_size) in positions_and_size {
            // A plugin pane
            if let Some(Run::Plugin(run)) = layout.run.clone() {
                let (pid_tx, pid_rx) = channel();
                let pane_title = run.location.to_string();
                self.senders
                    .send_to_plugin(PluginInstruction::Load(pid_tx, run, tab_index, client_id))
                    .unwrap();
                let pid = pid_rx.recv().unwrap();
                let mut new_plugin = PluginPane::new(
                    pid,
                    *position_and_size,
                    self.senders.to_plugin.as_ref().unwrap().clone(),
                    pane_title,
                    layout.pane_name.clone().unwrap_or_default(),
                );
                new_plugin.set_borderless(layout.borderless);
                self.tiled_panes.add_pane(PaneId::Plugin(pid), Box::new(new_plugin));
                set_focus_pane_id(layout, PaneId::Plugin(pid));
            } else {
                // there are still panes left to fill, use the pids we received in this method
                let pid = new_pids.next().unwrap(); // if this crashes it means we got less pids than there are panes in this layout
                let next_terminal_position = self.get_next_terminal_position();
                let mut new_pane = TerminalPane::new(
                    *pid,
                    *position_and_size,
                    self.colors,
                    next_terminal_position,
                    layout.pane_name.clone().unwrap_or_default(),
                    self.link_handler.clone(),
                );
                new_pane.set_borderless(layout.borderless);
                self.tiled_panes
                    .add_pane(PaneId::Terminal(*pid), Box::new(new_pane));
                set_focus_pane_id(layout, PaneId::Terminal(*pid));
            }
        }
        for unused_pid in new_pids {
            // this is a bit of a hack and happens because we don't have any central location that
            // can query the screen as to how many panes it needs to create a layout
            // fixing this will require a bit of an architecture change
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(PaneId::Terminal(*unused_pid)))
                .unwrap();
        }
        // FIXME: This is another hack to crop the viewport to fixed-size panes. Once you can have
        // non-fixed panes that are part of the viewport, get rid of this!
        let display_area = {
            let display_area = self.display_area.borrow();
            *display_area
        };
        self.resize_whole_tab(display_area);
        let boundary_geoms = self.tiled_panes.fixed_pane_geoms();
//         let boundary_geom: Vec<_> = self
//             .panes
//             .values()
//             .filter_map(|p| {
//                 let geom = p.position_and_size();
//                 if geom.cols.is_fixed() || geom.rows.is_fixed() {
//                     Some(geom.into())
//                 } else {
//                     None
//                 }
//             })
//             .collect();
        for geom in boundary_geoms {
            self.offset_viewport(&geom)
        }
        self.tiled_panes.set_pane_frames(self.draw_pane_frames, &mut self.os_api);

        if let Some(pane_id) = focus_pane_id {
            self.focus_pane_id = Some(pane_id);
            self.tiled_panes.focus_pane(pane_id, client_id);
        } else {
            // This is the end of the nasty viewport hack...
            let next_selectable_pane_id = self.tiled_panes.first_selectable_pane_id();
            match next_selectable_pane_id {
                Some(active_pane_id) => {
                    self.tiled_panes.focus_pane(active_pane_id, client_id);
                }
                None => {
                    // this is very likely a configuration error (layout with no selectable panes)
                    self.tiled_panes.clear_active_panes();
                }
            }
        }
    }
    pub fn update_input_modes(&mut self) {
        // this updates all plugins with the client's input mode
        for client_id in self.connected_clients.borrow().iter() {
            let mode_info = self
                .mode_info
                .get(client_id)
                .unwrap_or(&self.default_mode_info);
            self.senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    Some(*client_id),
                    Event::ModeUpdate(mode_info.clone()),
                ))
                .unwrap();
        }
    }
    pub fn add_client(&mut self, client_id: ClientId, mode_info: Option<ModeInfo>) {
        let first_connected_client = {
            self.connected_clients.borrow().iter().next().copied()
        };
        match first_connected_client {
            Some(first_client_id) => {
                if self.floating_panes.panes_are_visible() {
                    if let Some(first_active_floating_pane_id) =
                        self.floating_panes.first_active_floating_pane_id()
                    {
                        self.floating_panes
                            .focus_pane(first_active_floating_pane_id, client_id);
                    }
                }
                if let Some(first_active_tiled_pane_id) = self.tiled_panes.first_active_pane_id() {
                    self.tiled_panes.focus_pane(first_active_tiled_pane_id, client_id);
                }
                let mut connected_clients = self.connected_clients.borrow_mut();
                connected_clients.insert(client_id);
                self.mode_info.insert(
                    client_id,
                    mode_info.unwrap_or_else(|| self.default_mode_info.clone()),
                );
            }
            None => {
                let mut pane_ids: Vec<PaneId> = self.tiled_panes.pane_ids().copied().collect();
                // let mut pane_ids: Vec<PaneId> = self.panes.keys().copied().collect();
                if pane_ids.is_empty() {
                    // no panes here, bye bye
                    return;
                }
                let focus_pane_id = self.focus_pane_id.unwrap_or_else(|| {
                    pane_ids.sort(); // TODO: make this predictable
                    pane_ids.retain(|p| !self.tiled_panes.panes_to_hide_contains(*p));
                    *pane_ids.get(0).unwrap()
                });
                self.tiled_panes.focus_pane(focus_pane_id, client_id);
                let mut connected_clients = self.connected_clients.borrow_mut();
                connected_clients.insert(client_id);
                self.mode_info.insert(
                    client_id,
                    mode_info.unwrap_or_else(|| self.default_mode_info.clone()),
                );
            }
        }
        // TODO: we might be able to avoid this, we do this so that newly connected clients will
        // necessarily get a full render
        self.set_force_render();
        self.update_input_modes();
    }
    pub fn change_mode_info(&mut self, mode_info: ModeInfo, client_id: ClientId) {
        self.mode_info.insert(client_id, mode_info);
    }
    pub fn add_multiple_clients(&mut self, client_ids_to_mode_infos: Vec<(ClientId, ModeInfo)>) {
        for (client_id, client_mode_info) in client_ids_to_mode_infos {
            self.add_client(client_id, None);
            self.mode_info.insert(client_id, client_mode_info);
        }
    }
    pub fn remove_client(&mut self, client_id: ClientId) {
        self.focus_pane_id = None;
        self.connected_clients.borrow_mut().remove(&client_id);
        self.set_force_render();
    }
    pub fn drain_connected_clients(
        &mut self,
        clients_to_drain: Option<Vec<ClientId>>,
    ) -> Vec<(ClientId, ModeInfo)> {
        log::info!("drain_connected_clients");
        // None => all clients
        let mut client_ids_to_mode_infos = vec![];
        let clients_to_drain =
            clients_to_drain.unwrap_or_else(|| self.connected_clients.borrow_mut().drain().collect());
        for client_id in clients_to_drain {
            client_ids_to_mode_infos.push(self.drain_single_client(client_id));
        }
        log::info!("connected_clients after: {:?}", self.connected_clients);
        client_ids_to_mode_infos
    }
    pub fn drain_single_client(&mut self, client_id: ClientId) -> (ClientId, ModeInfo) {
        let client_mode_info = self
            .mode_info
            .remove(&client_id)
            .unwrap_or_else(|| self.default_mode_info.clone());
        self.connected_clients.borrow_mut().remove(&client_id);
        (client_id, client_mode_info)
    }
    pub fn has_no_connected_clients(&self) -> bool {
        self.connected_clients.borrow().is_empty()
    }
    pub fn toggle_pane_embed_or_floating(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            if let Some(focused_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                if self.tiled_panes.has_room_for_new_pane() {
                    // this unwrap is safe because floating panes should not be visible if there are no floating panes
                    let mut floating_pane_to_embed =
                        self.close_pane(focused_floating_pane_id).unwrap();
                    self.tiled_panes.insert_pane(focused_floating_pane_id, floating_pane_to_embed, &mut self.os_api);
                    self.should_clear_display_before_rendering = true;
                    self.tiled_panes.focus_pane(focused_floating_pane_id, client_id);
                    self.floating_panes.toggle_show_panes(false);
                }
            }
        // } else if let Some(focused_pane_id) = self.active_panes.get(&client_id).copied() {
        } else if let Some(focused_pane_id) = self.tiled_panes.focused_pane_id(client_id) {
            if let Some(new_pane_geom) = self.floating_panes.find_room_for_new_pane() {
                if self.get_selectable_tiled_panes().count() <= 1 {
                    // don't close the only pane on screen...
                    return;
                }
                if let Some(mut embedded_pane_to_float) = self.close_pane(focused_pane_id) {
                    embedded_pane_to_float.set_geom(new_pane_geom);
                    resize_pty!(embedded_pane_to_float, self.os_api);
                    embedded_pane_to_float.set_active_at(Instant::now());
                    self.floating_panes
                        .add_pane(focused_pane_id, embedded_pane_to_float);
                    self.floating_panes.focus_pane(focused_pane_id, client_id);
                    self.floating_panes.toggle_show_panes(true);

//                     // move all clients
//                     let connected_clients: Vec<ClientId> =
//                         self.connected_clients.iter().copied().collect();
//                     for client_id in connected_clients {
//                         self.floating_panes.focus_pane(focused_pane_id, client_id);
//                     }
//                     self.floating_panes.set_force_render();
                }
            }
        }
    }
    pub fn toggle_floating_panes(
        &mut self,
        client_id: ClientId,
        default_shell: Option<TerminalAction>,
    ) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.toggle_show_panes(false);
            self.set_force_render();
        } else {
            self.floating_panes.toggle_show_panes(true);
            match self.floating_panes.first_floating_pane_id() {
                Some(first_floating_pane_id) => {
                    if !self.floating_panes.active_panes_contain(&client_id) {
                        self.floating_panes
                            .focus_pane(first_floating_pane_id, client_id);
                    }
                }
                None => {
                    // there aren't any floating panes, we need to open a new one
                    //
                    // ************************************************************************************************
                    // BEWARE - THIS IS NOT ATOMIC - this sends an instruction to the pty thread to open a new terminal
                    // the pty thread will do its thing and eventually come back to the new_pane
                    // method on this tab which will open a new floating pane because we just
                    // toggled their visibility above us.
                    // If the pty thread takes too long, weird things can happen...
                    // ************************************************************************************************
                    //
                    let instruction = PtyInstruction::SpawnTerminal(
                        default_shell,
                        ClientOrTabIndex::ClientId(client_id),
                    );
                    self.senders.send_to_pty(instruction).unwrap();
                }
            }
            self.floating_panes.set_force_render();
        }
        self.set_force_render();
    }
    pub fn new_pane(&mut self, pid: PaneId, client_id: Option<ClientId>) {
        self.close_down_to_max_terminals();
        if self.floating_panes.panes_are_visible() {
            if let Some(new_pane_geom) = self.floating_panes.find_room_for_new_pane() {
                let next_terminal_position = self.get_next_terminal_position();
                if let PaneId::Terminal(term_pid) = pid {
                    let mut new_pane = TerminalPane::new(
                        term_pid,
                        new_pane_geom,
                        self.colors,
                        next_terminal_position,
                        String::new(),
                        self.link_handler.clone(),
                    );
                    new_pane.set_content_offset(Offset::frame(1)); // floating panes always have a frame
                    resize_pty!(new_pane, self.os_api);
                    self.floating_panes.add_pane(pid, Box::new(new_pane));
                    self.floating_panes.focus_pane_for_all_clients(pid);
                }
            }
        } else {
            if self.tiled_panes.fullscreen_is_active() {
                self.tiled_panes.unset_fullscreen(&mut self.os_api);
            }
            if self.tiled_panes.has_room_for_new_pane() {
                if let PaneId::Terminal(term_pid) = pid {
                    let next_terminal_position = self.get_next_terminal_position();
                    let new_terminal = TerminalPane::new(
                        term_pid,
                        PaneGeom::default(), // the initial size will be set later
                        self.colors,
                        next_terminal_position,
                        String::new(),
                        self.link_handler.clone(),
                    );
                    self.tiled_panes.insert_pane(pid, Box::new(new_terminal), &mut self.os_api);
                    self.should_clear_display_before_rendering = true;
                    if let Some(client_id) = client_id {
                        self.tiled_panes.focus_pane(pid, client_id);
                    }
                }
            }
        }
    }
    pub fn horizontal_split(&mut self, pid: PaneId, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            return;
        }
        self.close_down_to_max_terminals();
        if self.tiled_panes.fullscreen_is_active() {
            self.toggle_active_pane_fullscreen(client_id);
        }
        if self.tiled_panes.can_split_pane_horizontally(client_id) {
            if let PaneId::Terminal(term_pid) = pid {
                let next_terminal_position = self.get_next_terminal_position();
                let new_terminal = TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // the initial size will be set later
                    self.colors,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                );
                self.tiled_panes.split_pane_horizontally(pid, Box::new(new_terminal), &mut self.os_api, client_id);
                self.should_clear_display_before_rendering = true;
                self.tiled_panes.focus_pane(pid, client_id);
            }
        }
    }
    pub fn vertical_split(&mut self, pid: PaneId, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            return;
        }
        self.close_down_to_max_terminals();
        if self.tiled_panes.fullscreen_is_active() {
            self.toggle_active_pane_fullscreen(client_id);
        }
        if self.tiled_panes.can_split_pane_vertically(client_id) {
            if let PaneId::Terminal(term_pid) = pid {
                let next_terminal_position = self.get_next_terminal_position();
                let new_terminal = TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // the initial size will be set later
                    self.colors,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                );
                self.tiled_panes.split_pane_vertically(pid, Box::new(new_terminal), &mut self.os_api, client_id);
                self.should_clear_display_before_rendering = true;
                self.tiled_panes.focus_pane(pid, client_id);
            }
        }
    }
//     pub fn has_active_panes(&self) -> bool {
//         // a tab without active panes is a dead tab and should close
//         // a pane can be active even if there are no connected clients,
//         // we remember that pane for one the client focuses the tab next
//         !self.active_panes.is_empty()
//     }
    pub fn get_active_pane(&self, client_id: ClientId) -> Option<&dyn Pane> {
        self.get_active_pane_id(client_id).and_then(|ap| {
            if self.floating_panes.panes_are_visible() {
                self.floating_panes.get_pane(ap).map(Box::as_ref)
            } else {
                self.tiled_panes.get_pane(ap).map(Box::as_ref)
            }
        })
    }
    pub fn get_active_pane_mut(&mut self, client_id: ClientId) -> Option<&mut Box<dyn Pane>> {
        self.get_active_pane_id(client_id).and_then(|ap| {
            if self.floating_panes.panes_are_visible() {
                self.floating_panes.get_pane_mut(ap)
            } else {
                self.tiled_panes.get_pane_mut(ap)
            }
        })
    }
    pub fn get_active_pane_or_floating_pane_mut(
        &mut self,
        client_id: ClientId,
    ) -> Option<&mut Box<dyn Pane>> {
        if self.floating_panes.panes_are_visible() && self.floating_panes.has_active_panes() {
            self.floating_panes.get_active_pane_mut(client_id)
        } else {
            self.get_active_pane_mut(client_id)
        }
    }
    pub fn get_active_pane_id(&self, client_id: ClientId) -> Option<PaneId> {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.get_active_pane_id(client_id)
        } else {
            self.tiled_panes.get_active_pane_id(client_id)
        }
    }
    fn get_active_terminal_id(&self, client_id: ClientId) -> Option<RawFd> {
        if let Some(PaneId::Terminal(pid)) = self.get_active_pane_id(client_id) {
            Some(pid)
        } else {
            None
        }
    }
    pub fn has_terminal_pid(&self, pid: RawFd) -> bool {
        self.tiled_panes.panes_contain(&PaneId::Terminal(pid))
            || self.floating_panes.panes_contain(&PaneId::Terminal(pid))
    }
    pub fn handle_pty_bytes(&mut self, pid: RawFd, bytes: VteBytes) {
        if let Some(terminal_output) = self
            .tiled_panes
            .get_pane_mut(PaneId::Terminal(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Terminal(pid)))
        {
            // If the pane is scrolled buffer the vte events
            if terminal_output.is_scrolled() {
                self.pending_vte_events.entry(pid).or_default().push(bytes);
                if let Some(evs) = self.pending_vte_events.get(&pid) {
                    // Reset scroll - and process all pending events for this pane
                    if evs.len() >= MAX_PENDING_VTE_EVENTS {
                        terminal_output.clear_scroll();
                        self.process_pending_vte_events(pid);
                    }
                }
                return;
            }
        }
        self.process_pty_bytes(pid, bytes);
    }
    pub fn process_pending_vte_events(&mut self, pid: RawFd) {
        if let Some(pending_vte_events) = self.pending_vte_events.get_mut(&pid) {
            let vte_events: Vec<VteBytes> = pending_vte_events.drain(..).collect();
            for vte_event in vte_events {
                self.process_pty_bytes(pid, vte_event);
            }
        }
    }
    fn process_pty_bytes(&mut self, pid: RawFd, bytes: VteBytes) {
        if let Some(terminal_output) = self
            .tiled_panes
            .get_pane_mut(PaneId::Terminal(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Terminal(pid)))
        {
            terminal_output.handle_pty_bytes(bytes);
            let messages_to_pty = terminal_output.drain_messages_to_pty();
            for message in messages_to_pty {
                self.write_to_pane_id(message, PaneId::Terminal(pid));
            }
        }
    }
    pub fn write_to_terminals_on_current_tab(&mut self, input_bytes: Vec<u8>) {
        let pane_ids = self.get_static_and_floating_pane_ids();
        pane_ids.iter().for_each(|&pane_id| {
            self.write_to_pane_id(input_bytes.clone(), pane_id);
        });
    }
    pub fn write_to_active_terminal(&mut self, input_bytes: Vec<u8>, client_id: ClientId) {
        let pane_id = if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .get_active_pane_id(client_id)
                .unwrap_or_else(|| self.tiled_panes.get_active_pane_id(client_id).unwrap())
        } else {
            self.tiled_panes.get_active_pane_id(client_id).unwrap()
        };
        self.write_to_pane_id(input_bytes, pane_id);
    }
    pub fn write_to_terminal_at(&mut self, input_bytes: Vec<u8>, position: &Position) {
        if self.floating_panes.panes_are_visible() {
            let pane_id = self.floating_panes.get_pane_id_at(position, false);
            if let Some(pane_id) = pane_id {
                self.write_to_pane_id(input_bytes, pane_id);
                return;
            }
        }

        let pane_id = self.get_pane_id_at(position, false);
        if let Some(pane_id) = pane_id {
            self.write_to_pane_id(input_bytes, pane_id);
        }
    }
    pub fn write_to_pane_id(&mut self, input_bytes: Vec<u8>, pane_id: PaneId) {
        match pane_id {
            PaneId::Terminal(active_terminal_id) => {
                let active_terminal = self
                    .floating_panes
                    .get(&pane_id)
                    .unwrap_or_else(|| self.tiled_panes.get_pane(pane_id).unwrap());
                let adjusted_input = active_terminal.adjust_input_to_terminal(input_bytes);
                self.os_api
                    .write_to_tty_stdin(active_terminal_id, &adjusted_input)
                    .expect("failed to write to terminal");
                self.os_api
                    .tcdrain(active_terminal_id)
                    .expect("failed to drain terminal");
            }
            PaneId::Plugin(pid) => {
                for key in parse_keys(&input_bytes) {
                    self.senders
                        .send_to_plugin(PluginInstruction::Update(Some(pid), None, Event::Key(key)))
                        .unwrap()
                }
            }
        }
    }
    pub fn get_active_terminal_cursor_position(
        &self,
        client_id: ClientId,
    ) -> Option<(usize, usize)> {
        // (x, y)
        let active_pane_id = if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .get_active_pane_id(client_id)
                .or_else(|| self.tiled_panes.get_active_pane_id(client_id))?
        } else {
            self.tiled_panes.get_active_pane_id(client_id)?
        };
        let active_terminal = &self
            .floating_panes
            .get(&active_pane_id)
            .or_else(|| self.tiled_panes.get_pane(active_pane_id))?;
        active_terminal
            .cursor_coordinates()
            .map(|(x_in_terminal, y_in_terminal)| {
                let x = active_terminal.x() + x_in_terminal;
                let y = active_terminal.y() + y_in_terminal;
                (x, y)
            })
    }
    pub fn toggle_active_pane_fullscreen(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible(){
            return;
        }
        self.tiled_panes.toggle_active_pane_fullscreen(client_id, &mut self.os_api);
    }
    pub fn is_fullscreen_active(&self) -> bool {
        self.tiled_panes.fullscreen_is_active()
    }
    pub fn are_floating_panes_visible(&self) -> bool {
        self.floating_panes.panes_are_visible()
    }
    pub fn set_force_render(&mut self) {
        self.tiled_panes.set_force_render();
        self.floating_panes.set_force_render();
    }
    pub fn is_sync_panes_active(&self) -> bool {
        self.synchronize_is_active
    }
    pub fn toggle_sync_panes_is_active(&mut self) {
        self.synchronize_is_active = !self.synchronize_is_active;
    }
    pub fn mark_active_pane_for_rerender(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_mut(client_id) {
            active_pane.set_should_render(true);
        }
    }
    fn update_active_panes_in_pty_thread(&self) {
        // this is a bit hacky and we should ideally not keep this state in two different places at
        // some point
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        for client_id in connected_clients {
            self.senders
                .send_to_pty(PtyInstruction::UpdateActivePane(
                    self.get_active_pane_id(client_id),
                    client_id,
                ))
                .unwrap();
        }
    }
    pub fn render(&mut self, output: &mut Output, overlay: Option<String>) {
        let connected_clients: HashSet<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        if connected_clients.is_empty() || !self.tiled_panes.has_active_panes() {
            return;
        }
        self.update_active_panes_in_pty_thread();
        let floating_panes_stack = if self.floating_panes.panes_are_visible() {
            Some(self.floating_panes.stack())
        } else {
            None
        };
        output.add_clients(
            &connected_clients,
            self.link_handler.clone(),
            floating_panes_stack,
        );
        self.hide_cursor_and_clear_display_as_needed(output);

        let multiple_users_exist_in_session =
            { self.connected_clients_in_app.borrow().len() > 1 };
        self.tiled_panes.render(
            &self.connected_clients_in_app,
            &connected_clients,
            &self.mode_info,
            &self.default_mode_info,
            self.session_is_mirrored,
            output,
            self.colors,
            multiple_users_exist_in_session,
            self.floating_panes.panes_are_visible(),
        );
        if self.floating_panes.panes_are_visible() && self.floating_panes.has_active_panes() {
            self.floating_panes.render(
                &self.connected_clients_in_app,
                &connected_clients,
                &self.mode_info,
                &self.default_mode_info,
                self.session_is_mirrored,
                output,
                self.colors,
            );
        }
        // FIXME: Once clients can be distinguished
        if let Some(overlay_vte) = &overlay {
            output.add_post_vte_instruction_to_multiple_clients(
                connected_clients.iter().copied(),
                overlay_vte,
            );
        }
        self.render_cursor(output);
    }
    fn hide_cursor_and_clear_display_as_needed(&mut self, output: &mut Output) {
        let hide_cursor = "\u{1b}[?25l";
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        output.add_pre_vte_instruction_to_multiple_clients(
            connected_clients.iter().copied(),
            hide_cursor,
        );
        if self.should_clear_display_before_rendering {
            let clear_display = "\u{1b}[2J";
            output.add_pre_vte_instruction_to_multiple_clients(
                connected_clients.iter().copied(),
                clear_display,
            );
            self.should_clear_display_before_rendering = false;
        }
    }
    fn render_cursor(&self, output: &mut Output) {
        let connected_clients: Vec<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        for client_id in connected_clients {
            match self.get_active_terminal_cursor_position(client_id) {
                Some((cursor_position_x, cursor_position_y)) => {
                    let show_cursor = "\u{1b}[?25h";
                    let change_cursor_shape =
                        self.get_active_pane(client_id).map(|ap| ap.cursor_shape_csi()).unwrap_or_default();
                    let goto_cursor_position = &format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        cursor_position_y + 1,
                        cursor_position_x + 1,
                        change_cursor_shape
                    ); // goto row/col
                    output.add_post_vte_instruction_to_client(client_id, show_cursor);
                    output.add_post_vte_instruction_to_client(client_id, goto_cursor_position);
                }
                None => {
                    let hide_cursor = "\u{1b}[?25l";
                    output.add_post_vte_instruction_to_client(client_id, hide_cursor);
                }
            }
        }
    }
    fn get_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.tiled_panes.get_panes()
    }
    fn get_selectable_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.get_tiled_panes().filter(|(_, p)| p.selectable())
    }
    fn get_selectable_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.get_tiled_panes().filter(|(_, p)| p.selectable())
    }
    fn get_next_terminal_position(&self) -> usize {
        let tiled_panes_count = self.tiled_panes
            .get_panes()
            .filter(|(k, _)| match k {
                PaneId::Plugin(_) => false,
                PaneId::Terminal(_) => true,
            })
            .count();
        let floating_panes_count = self.floating_panes
            .get_panes()
            .filter(|(k, _)| match k {
                PaneId::Plugin(_) => false,
                PaneId::Terminal(_) => true,
            })
            .count();
        tiled_panes_count + floating_panes_count + 1
    }
    pub fn has_selectable_panes(&self) -> bool {
        let selectable_tiled_panes = self.tiled_panes.get_panes().filter(|(_, p)| p.selectable());
        let selectable_floating_panes = self.floating_panes.get_panes().filter(|(_, p)| p.selectable());
        selectable_tiled_panes.count() > 0 || selectable_floating_panes.count() > 0
    }
    fn next_active_tiled_pane(&self, panes: &[PaneId]) -> Option<PaneId> {
        let mut panes: Vec<_> = panes
            .iter()
            .map(|p_id| self.tiled_panes.get_pane(*p_id).unwrap())
            .collect();
        panes.sort_by_key(|b| Reverse(b.active_at()));

        panes.iter().find(|pane| pane.selectable()).map(|p| p.pid())
    }
    pub fn resize_whole_tab(&mut self, new_screen_size: Size) {
        self.floating_panes.resize(new_screen_size);
        self.tiled_panes.resize(new_screen_size, &mut self.os_api);
        self.should_clear_display_before_rendering = true;
    }
    pub fn resize_left(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_left(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_left(client_id, &mut self.os_api);
        }
    }
    pub fn resize_right(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_right(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_right(client_id, &mut self.os_api);
        }
    }
    pub fn resize_down(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_down(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_down(client_id, &mut self.os_api);
        }
    }
    pub fn resize_up(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_up(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_up(client_id, &mut self.os_api);
        }
    }
    pub fn resize_increase(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_increase(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_increase(client_id, &mut self.os_api);
        }
    }
    pub fn resize_decrease(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_decrease(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" incase of a decrease
                return;
            }
        } else {
            self.tiled_panes.resize_active_pane_decrease(client_id, &mut self.os_api);
        }
    }
    fn set_pane_active_at(&mut self, pane_id: PaneId) {
        if let Some(pane) = self.tiled_panes.get_pane_mut(pane_id) {
            pane.set_active_at(Instant::now());
        } else if let Some(pane) = self.floating_panes.get_pane_mut(pane_id) {
            pane.set_active_at(Instant::now());
        }
    }
    pub fn focus_next_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        self.tiled_panes.focus_next_pane(client_id);
    }
    pub fn focus_previous_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        self.tiled_panes.focus_previous_pane(client_id);
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_left(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus_left(client_id, &self.connected_clients.borrow().iter().copied().collect())
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return false;
            }
            self.tiled_panes.move_focus_left(client_id)
        }
    }
    pub fn move_focus_down(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus_down(client_id, &self.connected_clients.borrow().iter().copied().collect())
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return false;
            }
            self.tiled_panes.move_focus_down(client_id)
        }
    }
    pub fn move_focus_up(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus_up(client_id, &self.connected_clients.borrow().iter().copied().collect())
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return false;
            }
            self.tiled_panes.move_focus_up(client_id)
        }
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_right(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus_right(client_id, &self.connected_clients.borrow().iter().copied().collect())
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return false;
            }
            self.tiled_panes.move_focus_right(client_id)
        }
    }
    pub fn move_active_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        self.tiled_panes.move_active_pane(client_id, &mut self.os_api);
    }
    pub fn move_active_pane_down(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_down(client_id);
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_active_pane_down(client_id, &mut self.os_api);
        }
    }
    pub fn move_active_pane_up(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_up(client_id);
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_active_pane_up(client_id, &mut self.os_api);
        }
    }
    pub fn move_active_pane_right(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_right(client_id);
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_active_pane_right(client_id, &mut self.os_api);
        }
    }
    pub fn move_active_pane_left(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_left(client_id);
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_active_pane_left(client_id, &mut self.os_api);
        }
    }
    fn close_down_to_max_terminals(&mut self) {
        if let Some(max_panes) = self.max_panes {
            let terminals = self.get_tiled_pane_ids();
            for &pid in terminals.iter().skip(max_panes - 1) {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid))
                    .unwrap();
                self.close_pane(pid);
            }
        }
    }
    pub fn get_tiled_pane_ids(&self) -> Vec<PaneId> {
        self.get_tiled_panes().map(|(&pid, _)| pid).collect()
    }
    pub fn get_all_pane_ids(&self) -> Vec<PaneId> {
        // this is here just as a naming thing to make things more explicit
        self.get_static_and_floating_pane_ids()
    }
    pub fn get_static_and_floating_pane_ids(&self) -> Vec<PaneId> {
        self.tiled_panes
            .pane_ids()
            .chain(self.floating_panes.pane_ids())
            .copied()
            .collect()
    }
    pub fn set_pane_selectable(&mut self, id: PaneId, selectable: bool) {
        if let Some(pane) = self.tiled_panes.get_pane_mut(id) {
            pane.set_selectable(selectable);
            if !selectable {
                // there are some edge cases in which this causes a hard crash when there are no
                // other selectable panes - ideally this should never happen unless it's a
                // configuration error - but this *does* sometimes happen with the default
                // configuration as well since we set this at run time. I left this here because
                // this should very rarely happen and I hope in my heart that we will stop setting
                // this at runtime in the default configuration at some point
                //
                // If however this is not the case and we find this does cause crashes, we can
                // solve it by adding a "dangling_clients" struct to Tab which we would fill with
                // the relevant client ids in this case and drain as soon as a new selectable pane
                // is opened
                self.tiled_panes.move_clients_out_of_pane(id);
            }
        }
    }
    pub fn close_pane(&mut self, id: PaneId) -> Option<Box<dyn Pane>> {
        if self.floating_panes.panes_contain(&id) {
            let closed_pane = self.floating_panes.remove_pane(id);
            self.floating_panes.move_clients_out_of_pane(id);
            if !self.floating_panes.has_panes() {
                self.floating_panes.toggle_show_panes(false);
            }
            self.set_force_render();
            self.floating_panes.set_force_render();
            closed_pane
        } else {
            if self.tiled_panes.fullscreen_is_active() {
                self.tiled_panes.unset_fullscreen(&mut self.os_api);
            }
            let closed_pane = self.tiled_panes.remove_pane(id, &mut self.os_api);
            self.set_force_render();
            self.tiled_panes.set_force_render();
            closed_pane
        }
    }
    pub fn close_focused_pane(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            if let Some(active_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                self.close_pane(active_floating_pane_id);
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(active_floating_pane_id))
                    .unwrap();
                return;
            }
        }
        if let Some(active_pane_id) = self.tiled_panes.get_active_pane_id(client_id) {
            self.close_pane(active_pane_id);
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(active_pane_id))
                .unwrap();
        }
    }
    pub fn scroll_active_terminal_up(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.scroll_up(1, client_id);
        }
    }
    pub fn scroll_active_terminal_down(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.scroll_down(1, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd);
                }
            }
        }
    }
    pub fn scroll_active_terminal_up_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // prevent overflow when row == 0
            let scroll_rows = active_pane.rows().max(1) - 1;
            active_pane.scroll_up(scroll_rows, client_id);
        }
    }
    pub fn scroll_active_terminal_down_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let scroll_rows = active_pane.get_content_rows();
            active_pane.scroll_down(scroll_rows, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd);
                }
            }
        }
    }
    pub fn scroll_active_terminal_up_half_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // prevent overflow when row == 0
            let scroll_rows = (active_pane.rows().max(1) - 1) / 2;
            active_pane.scroll_up(scroll_rows, client_id);
        }
    }
    pub fn scroll_active_terminal_down_half_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let scroll_rows = (active_pane.rows().max(1) - 1) / 2;
            active_pane.scroll_down(scroll_rows, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd);
                }
            }
        }
    }
    pub fn scroll_active_terminal_to_bottom(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_scroll();
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd);
                }
            }
        }
    }
    pub fn clear_active_terminal_scroll(&mut self, client_id: ClientId) {
        // TODO: is this a thing?
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_scroll();
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd);
                }
            }
        }
    }
    pub fn scroll_terminal_up(&mut self, point: &Position, lines: usize, client_id: ClientId) {
        if let Some(pane) = self.get_pane_at(point, false) {
            if pane.mouse_mode() {
                let relative_position = pane.relative_position(point);
                let mouse_event = format!(
                    "\u{1b}[<64;{:?};{:?}M",
                    relative_position.column.0 + 1,
                    relative_position.line.0 + 1
                );
                self.write_to_terminal_at(mouse_event.into_bytes(), point);
            } else {
                pane.scroll_up(lines, client_id);
            }
        }
    }
    pub fn scroll_terminal_down(&mut self, point: &Position, lines: usize, client_id: ClientId) {
        if let Some(pane) = self.get_pane_at(point, false) {
            if pane.mouse_mode() {
                let relative_position = pane.relative_position(point);
                let mouse_event = format!(
                    "\u{1b}[<65;{:?};{:?}M",
                    relative_position.column.0 + 1,
                    relative_position.line.0 + 1
                );
                self.write_to_terminal_at(mouse_event.into_bytes(), point);
            } else {
                pane.scroll_down(lines, client_id);
                if !pane.is_scrolled() {
                    if let PaneId::Terminal(pid) = pane.pid() {
                        self.process_pending_vte_events(pid);
                    }
                }
            }
        }
    }
    fn get_pane_at(
        &mut self,
        point: &Position,
        search_selectable: bool,
    ) -> Option<&mut Box<dyn Pane>> {
        if self.floating_panes.panes_are_visible() {
            if let Some(pane_id) = self.floating_panes.get_pane_id_at(point, search_selectable) {
                return self.floating_panes.get_pane_mut(pane_id);
            }
        }
        if let Some(pane_id) = self.get_pane_id_at(point, search_selectable) {
            self.tiled_panes.get_pane_mut(pane_id)
        } else {
            None
        }
    }

    fn get_pane_id_at(&self, point: &Position, search_selectable: bool) -> Option<PaneId> {
        if self.tiled_panes.fullscreen_is_active() && self.is_position_inside_viewport(point) {
            let first_client_id = { self.connected_clients.borrow().iter().copied().next().unwrap() }; // TODO: instead of doing this, record the pane that is in fullscreen
            return self.tiled_panes.get_active_pane_id(first_client_id);
        }
        if search_selectable {
            self.get_selectable_tiled_panes()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        } else {
            self.get_tiled_panes()
                .find(|(_, p)| p.contains(point))
                .map(|(&id, _)| id)
        }
    }
    pub fn handle_left_click(&mut self, position: &Position, client_id: ClientId) {
        self.focus_pane_at(position, client_id);

        let search_selectable = false;
        if self.floating_panes.panes_are_visible()
            && self
                .floating_panes
                .move_pane_with_mouse(*position, search_selectable)
        {
            self.set_force_render();
            return;
        }

        if let Some(pane) = self.get_pane_at(position, false) {
            let relative_position = pane.relative_position(position);

            if pane.mouse_mode() {
                let mouse_event = format!(
                    "\u{1b}[<0;{:?};{:?}M",
                    relative_position.column.0 + 1,
                    relative_position.line.0 + 1
                );
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            } else {
                pane.start_selection(&relative_position, client_id);
                self.selecting_with_mouse = true;
            }
        };
    }
    pub fn handle_right_click(&mut self, position: &Position, client_id: ClientId) {
        self.focus_pane_at(position, client_id);

        if let Some(pane) = self.get_pane_at(position, false) {
            let relative_position = pane.relative_position(position);
            if pane.mouse_mode() {
                let mouse_event = format!(
                    "\u{1b}[<2;{:?};{:?}M",
                    relative_position.column.0 + 1,
                    relative_position.line.0 + 1
                );
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            } else {
                pane.handle_right_click(&relative_position, client_id);
            }
        };
    }
    fn focus_pane_at(&mut self, point: &Position, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            if let Some(clicked_pane) = self.floating_panes.get_pane_id_at(point, true) {
                self.floating_panes.focus_pane(clicked_pane, client_id);
                self.set_pane_active_at(clicked_pane);
                return;
            }
        }
        if let Some(clicked_pane) = self.get_pane_id_at(point, true) {
            self.tiled_panes.focus_pane(clicked_pane, client_id);
            self.set_pane_active_at(clicked_pane);
            if self.floating_panes.panes_are_visible() {
                self.floating_panes.toggle_show_panes(false);
                self.set_force_render();
            }
        }
    }
    pub fn handle_mouse_release(&mut self, position: &Position, client_id: ClientId) {
        if self.floating_panes.panes_are_visible()
            && self.floating_panes.pane_is_being_moved_with_mouse()
        {
            self.floating_panes.stop_moving_pane_with_mouse(*position);
            return;
        }

        let selecting = self.selecting_with_mouse;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            let relative_position = active_pane.relative_position(position);
            if active_pane.mouse_mode() {
                // ensure that coordinates are valid
                let col = (relative_position.column.0 + 1)
                    .max(1)
                    .min(active_pane.get_content_columns());

                let line = (relative_position.line.0 + 1)
                    .max(1)
                    .min(active_pane.get_content_rows() as isize);
                let mouse_event = format!("\u{1b}[<0;{:?};{:?}m", col, line);
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            } else if selecting {
                active_pane.end_selection(&relative_position, client_id);
                let selected_text = active_pane.get_selected_text();
                active_pane.reset_selection();
                if let Some(selected_text) = selected_text {
                    self.write_selection_to_clipboard(&selected_text);
                }
                self.selecting_with_mouse = false;
            }
        }
    }
    pub fn handle_mouse_hold(&mut self, position_on_screen: &Position, client_id: ClientId) {
        let search_selectable = true;

        if self.floating_panes.panes_are_visible()
            && self.floating_panes.pane_is_being_moved_with_mouse()
            && self
                .floating_panes
                .move_pane_with_mouse(*position_on_screen, search_selectable)
        {
            self.set_force_render();
            return;
        }

        let selecting = self.selecting_with_mouse;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            let relative_position = active_pane.relative_position(position_on_screen);
            if active_pane.mouse_mode() {
                // ensure that coordinates are valid
                let col = (relative_position.column.0 + 1)
                    .max(1)
                    .min(active_pane.get_content_columns());

                let line = (relative_position.line.0 + 1)
                    .max(1)
                    .min(active_pane.get_content_rows() as isize);

                let mouse_event = format!("\u{1b}[<32;{:?};{:?}M", col, line);
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            } else if selecting {
                active_pane.update_selection(&relative_position, client_id);
            }
        }
    }

    pub fn copy_selection(&self, client_id: ClientId) {
        let selected_text = self
            .get_active_pane(client_id)
            .and_then(|p| p.get_selected_text());
        if let Some(selected_text) = selected_text {
            self.write_selection_to_clipboard(&selected_text);
            self.senders
                .send_to_plugin(PluginInstruction::Update(
                    None,
                    None,
                    Event::CopyToClipboard(self.clipboard_provider.as_copy_destination()),
                ))
                .unwrap();
        }
    }

    fn write_selection_to_clipboard(&self, selection: &str) {
        let mut output = Output::default();
        let connected_clients: HashSet<ClientId> = { self.connected_clients.borrow().iter().copied().collect() };
        output.add_clients(&connected_clients, self.link_handler.clone(), None);
        let client_ids = connected_clients.iter().copied();
        let clipboard_event =
            match self
                .clipboard_provider
                .set_content(selection, &mut output, client_ids)
            {
                Ok(_) => {
                    let serialized_output = output.serialize();
                    self.senders
                        .send_to_server(ServerInstruction::Render(Some(serialized_output)))
                        .unwrap();
                    Event::CopyToClipboard(self.clipboard_provider.as_copy_destination())
                }
                Err(err) => {
                    log::error!("could not write selection to clipboard: {}", err);
                    Event::SystemClipboardFailure
                }
            };
        self.senders
            .send_to_plugin(PluginInstruction::Update(None, None, clipboard_event))
            .unwrap();
    }
    fn is_inside_viewport(&self, pane_id: &PaneId) -> bool {
        // this is mostly separated to an outside function in order to allow us to pass a clone to
        // it sometimes when we need to get around the borrow checker
        is_inside_viewport(&*self.viewport.borrow(), self.tiled_panes.get_pane(*pane_id).unwrap())
    }
    fn offset_viewport(&mut self, position_and_size: &Viewport) {
        let mut viewport = self.viewport.borrow_mut();
        if position_and_size.x == viewport.x
            && position_and_size.x + position_and_size.cols == viewport.x + viewport.cols
        {
            if position_and_size.y == viewport.y {
                viewport.y += position_and_size.rows;
                viewport.rows -= position_and_size.rows;
            } else if position_and_size.y + position_and_size.rows == viewport.y + viewport.rows {
                viewport.rows -= position_and_size.rows;
            }
        }
        if position_and_size.y == viewport.y
            && position_and_size.y + position_and_size.rows == viewport.y + viewport.rows
        {
            if position_and_size.x == viewport.x {
                viewport.x += position_and_size.cols;
                viewport.cols -= position_and_size.cols;
            } else if position_and_size.x + position_and_size.cols == viewport.x + viewport.cols {
                viewport.cols -= position_and_size.cols;
            }
        }
    }

    pub fn visible(&self, visible: bool) {
        let pids_in_this_tab = self.tiled_panes.pane_ids().filter_map(|p| match p {
            PaneId::Plugin(pid) => Some(pid),
            _ => None,
        });
        for pid in pids_in_this_tab {
            self.senders
                .send_to_plugin(PluginInstruction::Update(
                    Some(*pid),
                    None,
                    Event::Visible(visible),
                ))
                .unwrap();
        }
    }

    pub fn update_active_pane_name(&mut self, buf: Vec<u8>, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = self
                .tiled_panes
                .get_pane_mut(PaneId::Terminal(active_terminal_id))
                .unwrap();

            // It only allows printable unicode, delete and backspace keys.
            let is_updatable = buf.iter().all(|u| matches!(u, 0x20..=0x7E | 0x08 | 0x7F));
            if is_updatable {
                let s = str::from_utf8(&buf).unwrap();
                active_terminal.update_name(s);
            }
        }
    }

    pub fn is_position_inside_viewport(&self, point: &Position) -> bool {
        let Position {
            line: Line(line),
            column: Column(column),
        } = *point;
        let line: usize = line.try_into().unwrap();

        let viewport = self.viewport.borrow();
        line >= viewport.y
            && column >= viewport.x
            && line <= viewport.y + viewport.rows
            && column <= viewport.x + viewport.cols
    }

    pub fn set_pane_frames(&mut self, should_set_pane_frames: bool) {
        self.tiled_panes.set_pane_frames(should_set_pane_frames, &mut self.os_api);
        self.set_force_render();
    }
    pub fn panes_to_hide_count(&self) -> usize {
        self.tiled_panes.panes_to_hide_count()
    }
}

#[allow(clippy::borrowed_box)]
pub fn is_inside_viewport(viewport: &Viewport, pane: &Box<dyn Pane>) -> bool {
    let pane_position_and_size = pane.current_geom();
    pane_position_and_size.y >= viewport.y
        && pane_position_and_size.y + pane_position_and_size.rows.as_usize()
            <= viewport.y + viewport.rows
}

pub fn pane_geom_is_inside_viewport(viewport: &Viewport, geom: &PaneGeom) -> bool {
    geom.y >= viewport.y
        && geom.y + geom.rows.as_usize() <= viewport.y + viewport.rows
        && geom.x >= viewport.x
        && geom.x + geom.cols.as_usize() <= viewport.x + viewport.cols
}

#[cfg(test)]
#[path = "./unit/tab_tests.rs"]
mod tab_tests;

#[cfg(test)]
#[path = "./unit/tab_integration_tests.rs"]
mod tab_integration_tests;
