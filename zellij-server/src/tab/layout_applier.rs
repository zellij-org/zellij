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
    input::layout::{FloatingPanesLayout, PaneLayout, Run, RunPluginLocation},
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
        layout: PaneLayout,
        floating_panes_layout: Vec<FloatingPanesLayout>,
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
    fn apply_tiled_panes_layout(
        &mut self,
        layout: PaneLayout,
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
        match layout.position_panes_in_space(&free_space) {
            Ok(positions_in_layout) => {
                let positions_and_size = positions_in_layout.iter();
                let mut new_terminal_ids = new_terminal_ids.iter();

                let mut focus_pane_id: Option<PaneId> = None;
                let mut set_focus_pane_id = |layout: &PaneLayout, pane_id: PaneId| {
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
                self.adjust_viewport().with_context(err_context)?;
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
        floating_panes_layout: Vec<FloatingPanesLayout>,
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
    fn resize_whole_tab(&mut self, new_screen_size: Size) -> Result<()> {
        let err_context = || {
            format!(
                "failed to resize whole tab to new screen size {:?}",
                new_screen_size
            )
        };

        self.floating_panes.resize(new_screen_size);
        // we need to do this explicitly because floating_panes.resize does not do this
        self.floating_panes
            .resize_pty_all_panes(&mut self.os_api)
            .with_context(err_context)?;
        self.tiled_panes.resize(new_screen_size);
        Ok(())
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
    fn adjust_viewport(&mut self) -> Result<()> {
        // here we offset the viewport after applying a tiled panes layout
        // from borderless panes that are on the edges of the
        // screen, this is so that when we don't have pane boundaries (eg. when they were
        // disabled by the user) boundaries won't be drawn around these panes
        // geometrically, we can only do this with panes that are on the edges of the
        // screen - so it's mostly a best-effort thing
        let err_context = "failed to adjust viewport";

        let display_area = {
            let display_area = self.display_area.borrow();
            *display_area
        };
        self.resize_whole_tab(display_area).context(err_context)?;
        let boundary_geoms = self.tiled_panes.borderless_pane_geoms();
        for geom in boundary_geoms {
            self.offset_viewport(&geom)
        }
        self.tiled_panes.set_pane_frames(self.draw_pane_frames);
        Ok(())
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
