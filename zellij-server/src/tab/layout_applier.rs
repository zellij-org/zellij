use zellij_utils::errors::prelude::*;

use crate::resize_pty;
use crate::tab::{get_next_terminal_position, HoldForCommand, Pane};

use crate::{
    os_input_output::ServerOsApi,
    panes::sixel::SixelImageStore,
    panes::{FloatingPanes, TiledPanes},
    panes::{LinkHandler, PaneId, PluginPane, TerminalPane},
    plugins::PluginInstruction,
    pty::PtyInstruction,
    thread_bus::ThreadSenders,
    ClientId,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::{
    data::{Palette, Style},
    input::layout::{FloatingPaneLayout, TiledPaneLayout, Run, RunPluginLocation},
    pane_size::{Offset, PaneGeom, Size, SizeInPixels, Viewport},
};

pub struct LayoutApplier<'a> {
    viewport: Rc<RefCell<Viewport>>, // includes all non-UI panes
    senders: ThreadSenders,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    link_handler: Rc<RefCell<LinkHandler>>,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    style: Style,
    display_area: Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
    tiled_panes: &'a mut TiledPanes,
    floating_panes: &'a mut FloatingPanes,
    draw_pane_frames: bool,
    focus_pane_id: &'a mut Option<PaneId>,
    os_api: Box<dyn ServerOsApi>,
}

impl<'a> LayoutApplier<'a> {
    pub fn new(
        viewport: &Rc<RefCell<Viewport>>,
        senders: &ThreadSenders,
        sixel_image_store: &Rc<RefCell<SixelImageStore>>,
        link_handler: &Rc<RefCell<LinkHandler>>,
        terminal_emulator_colors: &Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: &Rc<RefCell<HashMap<usize, String>>>,
        character_cell_size: &Rc<RefCell<Option<SizeInPixels>>>,
        style: &Style,
        display_area: &Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
        tiled_panes: &'a mut TiledPanes,
        floating_panes: &'a mut FloatingPanes,
        draw_pane_frames: bool,
        focus_pane_id: &'a mut Option<PaneId>,
        os_api: &Box<dyn ServerOsApi>,
    ) -> Self {
        let viewport = viewport.clone();
        let senders = senders.clone();
        let sixel_image_store = sixel_image_store.clone();
        let link_handler = link_handler.clone();
        let terminal_emulator_colors = terminal_emulator_colors.clone();
        let terminal_emulator_color_codes = terminal_emulator_color_codes.clone();
        let character_cell_size = character_cell_size.clone();
        let style = style.clone();
        let display_area = display_area.clone();
        let os_api = os_api.clone();
        LayoutApplier {
            viewport,
            senders,
            sixel_image_store,
            link_handler,
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            character_cell_size,
            style,
            display_area,
            tiled_panes,
            floating_panes,
            draw_pane_frames,
            focus_pane_id,
            os_api,
        }
    }
    pub fn apply_layout(
        &mut self,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        mut new_plugin_ids: HashMap<RunPluginLocation, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<bool> {
        // true => layout has floating panes
        let layout_name = layout.name.clone();
        self.apply_tiled_panes_layout(layout, new_terminal_ids, &mut new_plugin_ids, client_id)?;
        let layout_has_floating_panes = self.apply_floating_panes_layout(
            floating_panes_layout,
            new_floating_terminal_ids,
            &mut new_plugin_ids,
            layout_name,
        )?;
        return Ok(layout_has_floating_panes);
    }
    pub fn apply_layout_to_existing_panes(
        &mut self,
        layout: &TiledPaneLayout,
        floating_panes_layout: &Vec<FloatingPaneLayout>,
        client_id: Option<ClientId>,
    ) -> Result<bool> {
        // true => layout has floating panes
        // let active_tiled_panes = self.tiled_panes.active_panes();
        let layout_name = layout.name.clone();
        self.apply_tiled_panes_layout_to_existing_panes(layout, client_id)?;
        // self.tiled_panes.set_active_panes(active_tiled_panes);
        let layout_has_floating_panes = self.apply_floating_panes_layout_to_existing_panes(floating_panes_layout, layout_name, client_id)?;
        return Ok(layout_has_floating_panes);
    }
    pub fn apply_tiled_panes_layout_to_existing_panes(&mut self, layout: &TiledPaneLayout, client_id: Option<ClientId>) -> Result<()> {
        let err_context = || format!("failed to apply tiled panes layout");
        // for tiled panes we need to take the display area rather than the viewport because the
        // viewport can potentially also be changed
        let (display_area_cols, display_area_rows) = {
            let display_area = self.display_area.borrow();
            (display_area.cols, display_area.rows)
        };
        let mut free_space = PaneGeom::default();
        free_space.cols.set_inner(display_area_cols);
        free_space.rows.set_inner(display_area_rows);
        let tiled_panes_count = self.tiled_panes.visible_panes_count();
        match layout.position_panes_in_space(&free_space, Some(tiled_panes_count)) {
            Ok(positions_in_layout) => {
                let positions_and_size = positions_in_layout.iter();
                let currently_focused_pane_id = client_id.and_then(|client_id| self.tiled_panes.focused_pane_id(client_id));

                let mut focused_pane_position_and_size: Option<PaneGeom> = None;
                let mut set_focused_pane_position_and_size = |layout: &TiledPaneLayout, pane_position_and_size: &PaneGeom| {
                    if layout.focus.unwrap_or(false) {
                        focused_pane_position_and_size = Some(*pane_position_and_size);
                    }
                };

                let mut existing_panes = self.tiled_panes.drain();
                let mut find_and_extract_pane = |run: &Option<Run>, position_and_size: &PaneGeom, is_focused: bool| -> Option<Box<dyn Pane>> {
                    let mut candidates: Vec<_> = existing_panes.iter().filter(|(_, p)| p.invoked_with() == run).collect();
                    candidates.sort_by(|(a_id, a), (b_id, b)| {
                        // be sure the focused pane is last so that if we have to use it explicitly
                        // in the layout, it's still available
                        if Some(**a_id) == currently_focused_pane_id {
                            std::cmp::Ordering::Greater
                        } else if Some(**b_id) == currently_focused_pane_id {
                            std::cmp::Ordering::Less
                        } else {
                            // if none of the panes are focused, try to find the closest pane
                            let abs = |a, b| (a as isize - b as isize).abs();
                            let a_x_distance = abs(a.position_and_size().x, position_and_size.x);
                            let a_y_distance = abs(a.position_and_size().y, position_and_size.y);
                            let b_x_distance = abs(b.position_and_size().x, position_and_size.x);
                            let b_y_distance = abs(b.position_and_size().y, position_and_size.y);
                            (a_x_distance + a_y_distance).cmp(&(b_x_distance + b_y_distance))
                        }
                    });
                    let find_focused_pane_id = || {
                        if is_focused {
                            candidates.iter().find(|(pid, _p)| Some(**pid) == currently_focused_pane_id).map(|(pid, _p)| *pid).copied()
                        } else {
                            None
                        }
                    };
                    if let Some(currently_focused_pane_id) = find_focused_pane_id() {
                        return existing_panes.remove(&currently_focused_pane_id);
                    } else if let Some(same_position_candidate_id) = candidates.iter().find(|(_, p)| p.position_and_size() == *position_and_size).map(|(pid, _p)| *pid).copied() {
                        return existing_panes.remove(&same_position_candidate_id);
                    } else if let Some(first_candidate) = candidates.iter().next().map(|(pid, _p)| *pid).copied() {
                        return existing_panes.remove(&first_candidate);
                    }
                    None
                };
                for (layout, position_and_size) in positions_and_size {
                    let is_focused = layout.focus.unwrap_or(false);
                    if let Some(mut pane) = find_and_extract_pane(&layout.run, &position_and_size, is_focused) {
                        // TODO: pane title and other layout attributes
                        pane.set_geom(*position_and_size);
                        let pane_pid = pane.pid();
                        pane.set_borderless(layout.borderless);
                        resize_pty!(pane, self.os_api, self.senders)?;
                        let pane_is_selectable = pane.selectable();
                        self.tiled_panes
                            .add_pane_with_existing_geom(pane_pid, pane);
                        if pane_is_selectable {
                            set_focused_pane_position_and_size(layout, position_and_size);
                        }
                    }
                }
                let remaining_pane_ids: Vec<PaneId> = existing_panes.keys().copied().collect();
                for pane_id in remaining_pane_ids {
                    if let Some(mut pane) = existing_panes.remove(&pane_id) {
                        // TODO: pane title and other layout attributes
                        let pane_pid = pane.pid();
                        pane.set_borderless(layout.borderless);
                        self.tiled_panes
                            .insert_pane(pane_pid, pane);
                    }
                }

                // TODO: what if existing_panes is not empty at this point?
                // self.adjust_viewport(); // TODO: ???
                if let Some(pane_position_and_size) = focused_pane_position_and_size {
                    if let Some(client_id) = client_id {
                        self.tiled_panes.focus_pane_at_position(pane_position_and_size, client_id);
                    } else {
                        // TODO: ??? this can happen eg. when closing panes
                    }
                }
            },
            Err(e) => {
                Err::<(), _>(anyError::msg(e))
                    .with_context(err_context)
                    .non_fatal(); // TODO: propagate this to the user
            },
        };
        Ok(())
    }
    fn apply_tiled_panes_layout(
        &mut self,
        layout: TiledPaneLayout,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: &mut HashMap<RunPluginLocation, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to apply tiled panes layout");
        let (viewport_cols, viewport_rows) = {
            let viewport = self.viewport.borrow();
            (viewport.cols, viewport.rows)
        };
        let mut free_space = PaneGeom::default();
        free_space.cols.set_inner(viewport_cols);
        free_space.rows.set_inner(viewport_rows);
        match layout.position_panes_in_space(&free_space, None) {
            Ok(positions_in_layout) => {
                let positions_and_size = positions_in_layout.iter();
                let mut new_terminal_ids = new_terminal_ids.iter();

                let mut focus_pane_id: Option<PaneId> = None;
                let mut set_focus_pane_id = |layout: &TiledPaneLayout, pane_id: PaneId| {
                    if layout.focus.unwrap_or(false) && focus_pane_id.is_none() {
                        focus_pane_id = Some(pane_id);
                    }
                };

                for (layout, position_and_size) in positions_and_size {
                    // A plugin pane
                    if let Some(Run::Plugin(run)) = layout.run.clone() {
                        let pane_title = run.location.to_string();
                        let pid = new_plugin_ids
                            .get_mut(&run.location)
                            .and_then(|ids| ids.pop())
                            .with_context(err_context)?;
                        let mut new_plugin = PluginPane::new(
                            pid,
                            *position_and_size,
                            self.senders
                                .to_plugin
                                .as_ref()
                                .with_context(err_context)?
                                .clone(),
                            pane_title,
                            layout.name.clone().unwrap_or_default(),
                            self.sixel_image_store.clone(),
                            self.terminal_emulator_colors.clone(),
                            self.terminal_emulator_color_codes.clone(),
                            self.link_handler.clone(),
                            self.character_cell_size.clone(),
                            self.style,
                            layout.run.clone(),
                        );
                        new_plugin.set_borderless(layout.borderless);
                        self.tiled_panes
                            .add_pane_with_existing_geom(PaneId::Plugin(pid), Box::new(new_plugin));
                        set_focus_pane_id(layout, PaneId::Plugin(pid));
                    } else {
                        // there are still panes left to fill, use the pids we received in this method
                        if let Some((pid, hold_for_command)) = new_terminal_ids.next() {
                            let next_terminal_position =
                                get_next_terminal_position(&self.tiled_panes, &self.floating_panes);
                            let initial_title = match &layout.run {
                                Some(Run::Command(run_command)) => Some(run_command.to_string()),
                                _ => None,
                            };
                            let mut new_pane = TerminalPane::new(
                                *pid,
                                *position_and_size,
                                self.style,
                                next_terminal_position,
                                layout.name.clone().unwrap_or_default(),
                                self.link_handler.clone(),
                                self.character_cell_size.clone(),
                                self.sixel_image_store.clone(),
                                self.terminal_emulator_colors.clone(),
                                self.terminal_emulator_color_codes.clone(),
                                initial_title,
                                layout.run.clone(),
                            );
                            new_pane.set_borderless(layout.borderless);
                            if let Some(held_command) = hold_for_command {
                                new_pane.hold(None, true, held_command.clone());
                            }
                            self.tiled_panes.add_pane_with_existing_geom(
                                PaneId::Terminal(*pid),
                                Box::new(new_pane),
                            );
                            set_focus_pane_id(layout, PaneId::Terminal(*pid));
                        }
                    }
                }
                for (unused_pid, _) in new_terminal_ids {
                    self.senders
                        .send_to_pty(PtyInstruction::ClosePane(PaneId::Terminal(*unused_pid)))
                        .with_context(err_context)?;
                }
                self.adjust_viewport();
                self.set_focused_tiled_pane(focus_pane_id, client_id);
            },
            Err(e) => {
                for (unused_pid, _) in new_terminal_ids {
                    self.senders
                        .send_to_pty(PtyInstruction::ClosePane(PaneId::Terminal(unused_pid)))
                        .with_context(err_context)?;
                }
                Err::<(), _>(anyError::msg(e))
                    .with_context(err_context)
                    .non_fatal(); // TODO: propagate this to the user
            },
        };
        Ok(())
    }
    fn apply_floating_panes_layout(
        &mut self,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: &mut HashMap<RunPluginLocation, Vec<u32>>,
        layout_name: Option<String>,
    ) -> Result<bool> {
        // true => has floating panes
        let err_context = || format!("Failed to apply_floating_panes_layout");
        let mut layout_has_floating_panes = false;
        let floating_panes_layout = floating_panes_layout.iter();
        let mut focused_floating_pane = None;
        let mut new_floating_terminal_ids = new_floating_terminal_ids.iter();
        for floating_pane_layout in floating_panes_layout {
            layout_has_floating_panes = true;
            if let Some(Run::Plugin(run)) = floating_pane_layout.run.clone() {
                let position_and_size = self
                    .floating_panes
                    .position_floating_pane_layout(&floating_pane_layout);
                let pane_title = run.location.to_string();
                let pid = new_plugin_ids
                    .get_mut(&run.location)
                    .and_then(|ids| ids.pop())
                    .with_context(err_context)?;
                let mut new_pane = PluginPane::new(
                    pid,
                    position_and_size,
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    pane_title,
                    layout_name.clone().unwrap_or_default(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.style,
                    floating_pane_layout.run.clone(),
                );
                new_pane.set_borderless(false);
                new_pane.set_content_offset(Offset::frame(1));
                resize_pty!(new_pane, self.os_api, self.senders)?;
                self.floating_panes
                    .add_pane(PaneId::Plugin(pid), Box::new(new_pane));
                if floating_pane_layout.focus.unwrap_or(false) {
                    focused_floating_pane = Some(PaneId::Plugin(pid));
                }
            } else if let Some((pid, hold_for_command)) = new_floating_terminal_ids.next() {
                let position_and_size = self
                    .floating_panes
                    .position_floating_pane_layout(&floating_pane_layout);
                let next_terminal_position =
                    get_next_terminal_position(&self.tiled_panes, &self.floating_panes);
                let initial_title = match &floating_pane_layout.run {
                    Some(Run::Command(run_command)) => Some(run_command.to_string()),
                    _ => None,
                };
                let mut new_pane = TerminalPane::new(
                    *pid,
                    position_and_size,
                    self.style,
                    next_terminal_position,
                    floating_pane_layout.name.clone().unwrap_or_default(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    initial_title,
                    floating_pane_layout.run.clone(),
                );
                new_pane.set_borderless(false);
                new_pane.set_content_offset(Offset::frame(1));
                if let Some(held_command) = hold_for_command {
                    new_pane.hold(None, true, held_command.clone());
                }
                resize_pty!(new_pane, self.os_api, self.senders)?;
                self.floating_panes
                    .add_pane(PaneId::Terminal(*pid), Box::new(new_pane));
                if floating_pane_layout.focus.unwrap_or(false) {
                    focused_floating_pane = Some(PaneId::Terminal(*pid));
                }
            }
        }
        if let Some(focused_floating_pane) = focused_floating_pane {
            self.floating_panes
                .focus_pane_for_all_clients(focused_floating_pane);
        }
        if layout_has_floating_panes {
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn apply_floating_panes_layout_to_existing_panes(
        &mut self,
        floating_panes_layout: &Vec<FloatingPaneLayout>,
        layout_name: Option<String>,
        client_id: Option<ClientId>,
    ) -> Result<bool> {
        // true => has floating panes
        let err_context = || format!("Failed to apply_floating_panes_layout");
        let mut layout_has_floating_panes = false;
        let floating_panes_layout = floating_panes_layout.iter();
        let currently_focused_pane_id = self.floating_panes.active_pane_id_or_focused_pane_id(client_id);

        let mut existing_panes = self.floating_panes.drain();
        let mut find_and_extract_pane = |run: &Option<Run>, position_and_size: &PaneGeom, is_focused: bool| -> Option<Box<dyn Pane>> {
            let mut candidates: Vec<_> = existing_panes.iter().filter(|(_, p)| p.invoked_with() == run).collect();
            candidates.sort_by(|(a_id, a), (b_id, b)| {
                // be sure the focused pane is last so that if we have to use it explicitly
                // in the layout, it's still available
                if Some(**a_id) == currently_focused_pane_id {
                    std::cmp::Ordering::Greater
                } else if Some(**b_id) == currently_focused_pane_id {
                    std::cmp::Ordering::Less
                } else {
                    // if none of the panes are focused, try to find the closest pane
                    let abs = |a, b| (a as isize - b as isize).abs();
                    let a_x_distance = abs(a.position_and_size().x, position_and_size.x);
                    let a_y_distance = abs(a.position_and_size().y, position_and_size.y);
                    let b_x_distance = abs(b.position_and_size().x, position_and_size.x);
                    let b_y_distance = abs(b.position_and_size().y, position_and_size.y);
                    (a_x_distance + a_y_distance).cmp(&(b_x_distance + b_y_distance))
                }
            });
            let find_focused_pane_id = || {
                if is_focused {
                    candidates.iter().find(|(pid, p)| Some(**pid) == currently_focused_pane_id).map(|(pid, _p)| *pid).copied()
                } else {
                    None
                }
            };
            if let Some(currently_focused_pane_id) = find_focused_pane_id() {
                return existing_panes.remove(&currently_focused_pane_id);
            } else if let Some(same_position_candidate_id) = candidates.iter().find(|(_, p)| p.position_and_size() == *position_and_size).map(|(pid, _p)| *pid).copied() {
                return existing_panes.remove(&same_position_candidate_id);
            } else if let Some(first_candidate) = candidates.iter().next().map(|(pid, _p)| *pid).copied() {
                return existing_panes.remove(&first_candidate);
            }
            None
        };

        for floating_pane_layout in floating_panes_layout {
            let position_and_size = self
                .floating_panes
                .position_floating_pane_layout(&floating_pane_layout);
            let is_focused = floating_pane_layout.focus.unwrap_or(false);
            match find_and_extract_pane(&floating_pane_layout.run, &position_and_size, is_focused) {
                Some(mut pane) => {
                    layout_has_floating_panes = true;
                    // TODO: pane title and other layout attributes
                    pane.set_geom(position_and_size);
                    let pane_pid = pane.pid();
                    pane.set_borderless(false);
                    pane.set_content_offset(Offset::frame(1));
                    resize_pty!(pane, self.os_api, self.senders)?;
                    self.floating_panes
                        .add_pane(pane_pid, pane);
                },
                None => {
                    // self.floating_panes.add_next_geom(position_and_size);
                }
            }
        }
        let remaining_pane_ids: Vec<PaneId> = existing_panes.keys().copied().collect();
        for pane_id in remaining_pane_ids {
            match self.floating_panes.find_room_for_new_pane() {
                Some(position_and_size) => {
                    if let Some(mut pane) = existing_panes.remove(&pane_id) {
                        layout_has_floating_panes = true;
                        // TODO: pane title and other layout attributes
                        pane.set_geom(position_and_size);
                        let pane_pid = pane.pid();
                        pane.set_borderless(false);
                        pane.set_content_offset(Offset::frame(1));
                        resize_pty!(pane, self.os_api, self.senders)?;
                        self.floating_panes
                            .add_pane(pane_pid, pane);
                    }

                },
                None => {
                    log::error!("could not find room for pane!")
                }
            }
        }
        if layout_has_floating_panes {
            if let Some(currently_focused_pane_id) = currently_focused_pane_id {
                // we have to do this explicitly to make sure the z-indices still do what we want
                // them to
                match client_id {
                    Some(client_id) => self.floating_panes.focus_pane(currently_focused_pane_id, client_id),
                    None => self.floating_panes.focus_pane_for_all_clients(currently_focused_pane_id),
                };
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn resize_whole_tab(&mut self, new_screen_size: Size) {
        self.floating_panes.resize(new_screen_size);
        self.floating_panes
            .resize_pty_all_panes(&mut self.os_api)
            .unwrap(); // we need to do this explicitly because floating_panes.resize does not do this
        self.tiled_panes.resize(new_screen_size);
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
    fn adjust_viewport(&mut self) {
        // here we offset the viewport after applying a tiled panes layout
        // from borderless panes that are on the edges of the
        // screen, this is so that when we don't have pane boundaries (eg. when they were
        // disabled by the user) boundaries won't be drawn around these panes
        // geometrically, we can only do this with panes that are on the edges of the
        // screen - so it's mostly a best-effort thing
        let display_area = {
            let display_area = self.display_area.borrow();
            *display_area
        };
        self.resize_whole_tab(display_area);
        let boundary_geoms = self.tiled_panes.borderless_pane_geoms();
        for geom in boundary_geoms {
            self.offset_viewport(&geom)
        }
        self.tiled_panes.set_pane_frames(self.draw_pane_frames);
    }
    fn set_focused_tiled_pane(&mut self, focus_pane_id: Option<PaneId>, client_id: ClientId) {
        if let Some(pane_id) = focus_pane_id {
            *self.focus_pane_id = Some(pane_id);
            self.tiled_panes.focus_pane(pane_id, client_id);
        } else {
            let next_selectable_pane_id = self.tiled_panes.first_selectable_pane_id();
            match next_selectable_pane_id {
                Some(active_pane_id) => {
                    self.tiled_panes.focus_pane(active_pane_id, client_id);
                },
                None => {
                    self.tiled_panes.clear_active_panes();
                },
            }
        }
    }
}
