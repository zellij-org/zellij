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
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use zellij_utils::{
    data::{Palette, Style},
    input::layout::{FloatingPaneLayout, Run, RunPluginOrAlias, TiledPaneLayout},
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
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
    style: Style,
    display_area: Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
    tiled_panes: &'a mut TiledPanes,
    floating_panes: &'a mut FloatingPanes,
    draw_pane_frames: bool,
    focus_pane_id: &'a mut Option<PaneId>,
    os_api: Box<dyn ServerOsApi>,
    debug: bool,
    arrow_fonts: bool,
    styled_underlines: bool,
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
        connected_clients: &Rc<RefCell<HashSet<ClientId>>>,
        style: &Style,
        display_area: &Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
        tiled_panes: &'a mut TiledPanes,
        floating_panes: &'a mut FloatingPanes,
        draw_pane_frames: bool,
        focus_pane_id: &'a mut Option<PaneId>,
        os_api: &Box<dyn ServerOsApi>,
        debug: bool,
        arrow_fonts: bool,
        styled_underlines: bool,
    ) -> Self {
        let viewport = viewport.clone();
        let senders = senders.clone();
        let sixel_image_store = sixel_image_store.clone();
        let link_handler = link_handler.clone();
        let terminal_emulator_colors = terminal_emulator_colors.clone();
        let terminal_emulator_color_codes = terminal_emulator_color_codes.clone();
        let character_cell_size = character_cell_size.clone();
        let connected_clients = connected_clients.clone();
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
            connected_clients,
            style,
            display_area,
            tiled_panes,
            floating_panes,
            draw_pane_frames,
            focus_pane_id,
            os_api,
            debug,
            arrow_fonts,
            styled_underlines,
        }
    }
    pub fn apply_layout(
        &mut self,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        mut new_plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<bool> {
        // true => should_show_floating_panes
        let layout_name = layout.name.clone();
        let hide_floating_panes = layout.hide_floating_panes;
        self.apply_tiled_panes_layout(layout, new_terminal_ids, &mut new_plugin_ids, client_id)?;
        let layout_has_floating_panes = self.apply_floating_panes_layout(
            floating_panes_layout,
            new_floating_terminal_ids,
            &mut new_plugin_ids,
            layout_name,
        )?;
        let should_show_floating_panes = layout_has_floating_panes && !hide_floating_panes;
        return Ok(should_show_floating_panes);
    }
    pub fn apply_tiled_panes_layout_to_existing_panes(
        &mut self,
        layout: &TiledPaneLayout,
        refocus_pane: bool,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let free_space = self.total_space_for_tiled_panes();
        let tiled_panes_count = self.tiled_panes.visible_panes_count();
        let positions_in_layout =
            match layout.position_panes_in_space(&free_space, Some(tiled_panes_count), false) {
                Ok(positions_in_layout) => positions_in_layout,
                // in the error branch, we try to recover by positioning the panes in the space but
                // ignoring the percentage sizes (passing true as the third argument), this is a hack
                // around some issues with the constraint system that should be addressed in a systemic
                // manner
                Err(_e) => layout
                    .position_panes_in_space(&free_space, Some(tiled_panes_count), true)
                    .map_err(|e| anyhow!(e))?,
            };
        let currently_focused_pane_id =
            client_id.and_then(|client_id| self.tiled_panes.focused_pane_id(client_id));
        let mut existing_tab_state =
            ExistingTabState::new(self.tiled_panes.drain(), currently_focused_pane_id);
        let mut pane_focuser = PaneFocuser::new(refocus_pane);
        let mut positions_left = vec![];
        for (layout, position_and_size) in positions_in_layout {
            // first try to find panes with contents matching the layout exactly
            match existing_tab_state.find_and_extract_exact_match_pane(
                &layout.run,
                &position_and_size,
                true,
            ) {
                Some(mut pane) => {
                    self.apply_layout_properties_to_pane(
                        &mut pane,
                        &layout,
                        Some(position_and_size),
                    );
                    pane_focuser.set_pane_id_in_focused_location(layout.focus, &pane);
                    pane_focuser.set_expanded_stacked_pane(layout.is_expanded_in_stack, &pane);
                    resize_pty!(pane, self.os_api, self.senders, self.character_cell_size)?;
                    self.tiled_panes
                        .add_pane_with_existing_geom(pane.pid(), pane);
                },
                None => {
                    positions_left.push((layout, position_and_size));
                },
            }
        }
        for (layout, position_and_size) in positions_left {
            // now let's try to find panes on a best-effort basis
            if let Some(mut pane) = existing_tab_state.find_and_extract_pane(
                &layout.run,
                &position_and_size,
                layout.focus.unwrap_or(false),
                true,
            ) {
                self.apply_layout_properties_to_pane(&mut pane, &layout, Some(position_and_size));
                pane_focuser.set_pane_id_in_focused_location(layout.focus, &pane);
                pane_focuser.set_expanded_stacked_pane(layout.is_expanded_in_stack, &pane);
                resize_pty!(pane, self.os_api, self.senders, self.character_cell_size)?;
                self.tiled_panes
                    .add_pane_with_existing_geom(pane.pid(), pane);
            }
        }
        let remaining_pane_ids: Vec<PaneId> = existing_tab_state.pane_ids();
        for pane_id in remaining_pane_ids {
            if let Some(mut pane) = existing_tab_state.remove_pane(&pane_id) {
                self.apply_layout_properties_to_pane(&mut pane, &layout, None);
                self.tiled_panes.insert_pane(pane.pid(), pane);
            }
        }
        pane_focuser.focus_tiled_pane(&mut self.tiled_panes);
        LayoutApplier::offset_viewport(
            self.viewport.clone(),
            self.tiled_panes,
            self.draw_pane_frames,
        );
        Ok(())
    }
    fn apply_tiled_panes_layout(
        &mut self,
        layout: TiledPaneLayout,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to apply tiled panes layout");
        let free_space = self.total_space_for_tiled_panes();
        let mut positions_in_layout = match layout.position_panes_in_space(&free_space, None, false)
        {
            Ok(positions_in_layout) => positions_in_layout,
            // in the error branch, we try to recover by positioning the panes in the space but
            // ignoring the percentage sizes (passing true as the third argument), this is a hack
            // around some issues with the constraint system that should be addressed in a systemic
            // manner
            Err(_e) => layout
                .position_panes_in_space(&free_space, None, true)
                .map_err(|e| anyhow!(e))?,
        };
        let mut run_instructions_to_ignore = layout.run_instructions_to_ignore.clone();
        let mut new_terminal_ids = new_terminal_ids.iter();

        let mut focus_pane_id: Option<PaneId> = None;
        let mut set_focus_pane_id = |layout: &TiledPaneLayout, pane_id: PaneId| {
            if layout.focus.unwrap_or(false) && focus_pane_id.is_none() {
                focus_pane_id = Some(pane_id);
            }
        };

        // first, try to find rooms for the panes that are already running (represented by
        // run_instructions_to_ignore), we try to either find an explicit position (the new
        // layout has a pane with the exact run instruction) or an otherwise free position
        // (the new layout has a pane with None as its run instruction)
        for run_instruction in run_instructions_to_ignore.drain(..) {
            if let Some(position) = positions_in_layout
                .iter()
                .position(|(layout, _position_and_size)| &layout.run == &run_instruction)
            {
                let (layout, position_and_size) = positions_in_layout.remove(position);
                self.tiled_panes.set_geom_for_pane_with_run(
                    layout.run,
                    position_and_size,
                    layout.borderless,
                );
            } else if let Some(position) = positions_in_layout
                .iter()
                .position(|(layout, _position_and_size)| layout.run.is_none())
            {
                let (layout, position_and_size) = positions_in_layout.remove(position);
                self.tiled_panes.set_geom_for_pane_with_run(
                    run_instruction,
                    position_and_size,
                    layout.borderless,
                );
            } else {
                log::error!(
                    "Failed to find room for run instruction: {:?}",
                    run_instruction
                );
            }
        }

        // then, we open new panes for each run instruction in the layout with the details
        // we got from the plugin thread and pty thread
        let positions_and_size = positions_in_layout.iter();
        for (layout, position_and_size) in positions_and_size {
            if let Some(Run::Plugin(run)) = layout.run.clone() {
                let pane_title = run.location_string();
                let pid = new_plugin_ids
                    .get_mut(&run)
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
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    layout.run.clone(),
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                );
                if let Some(pane_initial_contents) = &layout.pane_initial_contents {
                    new_plugin.handle_pty_bytes(pane_initial_contents.as_bytes().into());
                    new_plugin.handle_pty_bytes("\n\r".as_bytes().into());
                }

                new_plugin.set_borderless(layout.borderless);
                if let Some(exclude_from_sync) = layout.exclude_from_sync {
                    new_plugin.set_exclude_from_sync(exclude_from_sync);
                }
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
                        self.debug,
                        self.arrow_fonts,
                        self.styled_underlines,
                    );
                    if let Some(pane_initial_contents) = &layout.pane_initial_contents {
                        new_pane.handle_pty_bytes(pane_initial_contents.as_bytes().into());
                        new_pane.handle_pty_bytes("\n\r".as_bytes().into());
                    }
                    new_pane.set_borderless(layout.borderless);
                    if let Some(exclude_from_sync) = layout.exclude_from_sync {
                        new_pane.set_exclude_from_sync(exclude_from_sync);
                    }
                    if let Some(held_command) = hold_for_command {
                        new_pane.hold(None, true, held_command.clone());
                    }
                    self.tiled_panes
                        .add_pane_with_existing_geom(PaneId::Terminal(*pid), Box::new(new_pane));
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
        Ok(())
    }
    fn apply_floating_panes_layout(
        &mut self,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
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
            let position_and_size = self
                .floating_panes
                .position_floating_pane_layout(&floating_pane_layout);
            if floating_pane_layout.already_running {
                self.floating_panes.set_geom_for_pane_with_run(
                    floating_pane_layout.run.clone(),
                    position_and_size,
                );
            } else if let Some(Run::Plugin(run)) = floating_pane_layout.run.clone() {
                let pane_title = run.location_string();
                let pid = new_plugin_ids
                    .get_mut(&run)
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
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    floating_pane_layout.run.clone(),
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                );
                if let Some(pane_initial_contents) = &floating_pane_layout.pane_initial_contents {
                    new_pane.handle_pty_bytes(pane_initial_contents.as_bytes().into());
                    new_pane.handle_pty_bytes("\n\r".as_bytes().into());
                }
                new_pane.set_borderless(false);
                new_pane.set_content_offset(Offset::frame(1));
                resize_pty!(
                    new_pane,
                    self.os_api,
                    self.senders,
                    self.character_cell_size
                )?;
                self.floating_panes
                    .add_pane(PaneId::Plugin(pid), Box::new(new_pane));
                if floating_pane_layout.focus.unwrap_or(false) {
                    focused_floating_pane = Some(PaneId::Plugin(pid));
                }
            } else if let Some((pid, hold_for_command)) = new_floating_terminal_ids.next() {
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
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                );
                if let Some(pane_initial_contents) = &floating_pane_layout.pane_initial_contents {
                    new_pane.handle_pty_bytes(pane_initial_contents.as_bytes().into());
                    new_pane.handle_pty_bytes("\n\r".as_bytes().into());
                }
                new_pane.set_borderless(false);
                new_pane.set_content_offset(Offset::frame(1));
                if let Some(held_command) = hold_for_command {
                    new_pane.hold(None, true, held_command.clone());
                }
                resize_pty!(
                    new_pane,
                    self.os_api,
                    self.senders,
                    self.character_cell_size
                )?;
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
        refocus_pane: bool,
        client_id: Option<ClientId>,
    ) -> Result<bool> {
        // true => has floating panes
        let mut layout_has_floating_panes = false;
        let layout_has_focused_pane = floating_panes_layout
            .iter()
            .find(|f| f.focus.map(|f| f).unwrap_or(false))
            .is_some();
        let floating_panes_layout = floating_panes_layout.iter();
        let currently_focused_pane_id = self
            .floating_panes
            .active_pane_id_or_focused_pane_id(client_id);
        let mut existing_tab_state =
            ExistingTabState::new(self.floating_panes.drain(), currently_focused_pane_id);
        let mut pane_focuser = PaneFocuser::new(refocus_pane);
        for floating_pane_layout in floating_panes_layout {
            let position_and_size = self
                .floating_panes
                .position_floating_pane_layout(&floating_pane_layout);
            let is_focused = floating_pane_layout.focus.unwrap_or(false);
            if let Some(mut pane) = existing_tab_state.find_and_extract_pane(
                &floating_pane_layout.run,
                &position_and_size,
                is_focused,
                false,
            ) {
                layout_has_floating_panes = true;
                self.apply_floating_pane_layout_properties_to_pane(
                    &mut pane,
                    Some(&floating_pane_layout),
                    position_and_size,
                );
                let pane_is_focused = floating_pane_layout
                    .focus
                    .or(Some(!layout_has_focused_pane));
                pane_focuser.set_pane_id_in_focused_location(pane_is_focused, &pane);
                resize_pty!(pane, self.os_api, self.senders, self.character_cell_size)?;
                self.floating_panes.add_pane(pane.pid(), pane);
            }
        }
        let remaining_pane_ids: Vec<PaneId> = existing_tab_state.pane_ids();
        for pane_id in remaining_pane_ids {
            match self.floating_panes.find_room_for_new_pane() {
                Some(position_and_size) => {
                    if let Some(mut pane) = existing_tab_state.remove_pane(&pane_id) {
                        layout_has_floating_panes = true;
                        self.apply_floating_pane_layout_properties_to_pane(
                            &mut pane,
                            None,
                            position_and_size,
                        );
                        pane_focuser
                            .set_pane_id_in_focused_location(Some(!layout_has_focused_pane), &pane);
                        resize_pty!(pane, self.os_api, self.senders, self.character_cell_size)?;
                        self.floating_panes.add_pane(pane.pid(), pane);
                    }
                },
                None => {
                    log::error!("could not find room for pane!")
                },
            }
        }

        if layout_has_floating_panes {
            pane_focuser.focus_floating_pane(&mut self.floating_panes, &mut self.os_api);
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
    pub fn offset_viewport(
        viewport: Rc<RefCell<Viewport>>,
        tiled_panes: &mut TiledPanes,
        draw_pane_frames: bool,
    ) {
        let boundary_geoms = tiled_panes.non_selectable_pane_geoms_inside_viewport();
        {
            // curly braces here is so that we free viewport immediately when we're done
            let mut viewport = viewport.borrow_mut();
            for position_and_size in boundary_geoms {
                if position_and_size.x == viewport.x
                    && position_and_size.x + position_and_size.cols == viewport.x + viewport.cols
                {
                    if position_and_size.y == viewport.y {
                        viewport.y += position_and_size.rows;
                        viewport.rows -= position_and_size.rows;
                    } else if position_and_size.y + position_and_size.rows
                        == viewport.y + viewport.rows
                    {
                        viewport.rows -= position_and_size.rows;
                    }
                }
                if position_and_size.y == viewport.y
                    && position_and_size.y + position_and_size.rows == viewport.y + viewport.rows
                {
                    if position_and_size.x == viewport.x {
                        viewport.x += position_and_size.cols;
                        viewport.cols -= position_and_size.cols;
                    } else if position_and_size.x + position_and_size.cols
                        == viewport.x + viewport.cols
                    {
                        viewport.cols -= position_and_size.cols;
                    }
                }
            }
        }
        tiled_panes.set_pane_frames(draw_pane_frames);
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
        LayoutApplier::offset_viewport(
            self.viewport.clone(),
            self.tiled_panes,
            self.draw_pane_frames,
        );
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
    fn apply_layout_properties_to_pane(
        &self,
        pane: &mut Box<dyn Pane>,
        layout: &TiledPaneLayout,
        position_and_size: Option<PaneGeom>,
    ) {
        if let Some(position_and_size) = position_and_size {
            pane.set_geom(position_and_size);
        }
        pane.set_borderless(layout.borderless);
        if let Some(pane_title) = layout.name.as_ref() {
            pane.set_title(pane_title.into());
        }
    }
    fn apply_floating_pane_layout_properties_to_pane(
        &self,
        pane: &mut Box<dyn Pane>,
        floating_pane_layout: Option<&FloatingPaneLayout>,
        position_and_size: PaneGeom,
    ) {
        pane.set_geom(position_and_size);
        pane.set_borderless(false);
        if let Some(pane_title) = floating_pane_layout.and_then(|f| f.name.clone()) {
            pane.set_title(pane_title);
        }
        pane.set_content_offset(Offset::frame(1));
    }
    fn total_space_for_tiled_panes(&self) -> PaneGeom {
        // for tiled panes we need to take the display area rather than the viewport because the
        // viewport can potentially also be changed
        let (display_area_cols, display_area_rows) = {
            let display_area = self.display_area.borrow();
            (display_area.cols, display_area.rows)
        };

        let mut free_space = PaneGeom::default();
        free_space.cols.set_inner(display_area_cols);
        free_space.rows.set_inner(display_area_rows);
        free_space
    }
}

struct ExistingTabState {
    existing_panes: BTreeMap<PaneId, Box<dyn Pane>>,
    currently_focused_pane_id: Option<PaneId>,
}

impl ExistingTabState {
    pub fn new(
        existing_panes: BTreeMap<PaneId, Box<dyn Pane>>,
        currently_focused_pane_id: Option<PaneId>,
    ) -> Self {
        ExistingTabState {
            existing_panes,
            currently_focused_pane_id,
        }
    }
    pub fn find_and_extract_exact_match_pane(
        &mut self,
        run: &Option<Run>,
        position_and_size: &PaneGeom,
        default_to_closest_position: bool,
    ) -> Option<Box<dyn Pane>> {
        let candidates = self.pane_candidates(run, position_and_size, default_to_closest_position);
        if let Some(current_pane_id_with_same_contents) =
            self.find_pane_id_with_same_contents_and_location(&candidates, run, position_and_size)
        {
            return self
                .existing_panes
                .remove(&current_pane_id_with_same_contents);
        }
        None
    }
    pub fn find_and_extract_pane(
        &mut self,
        run: &Option<Run>,
        position_and_size: &PaneGeom,
        is_focused: bool,
        default_to_closest_position: bool,
    ) -> Option<Box<dyn Pane>> {
        let candidates = self.pane_candidates(run, position_and_size, default_to_closest_position);
        if let Some(current_pane_id_with_same_contents) =
            self.find_pane_id_with_same_contents(&candidates, run)
        {
            return self
                .existing_panes
                .remove(&current_pane_id_with_same_contents);
        } else if let Some(currently_focused_pane_id) =
            self.find_focused_pane_id(is_focused, &candidates)
        {
            return self.existing_panes.remove(&currently_focused_pane_id);
        } else if let Some(same_position_candidate_id) = candidates
            .iter()
            .find(|(_, p)| p.position_and_size() == *position_and_size)
            .map(|(pid, _p)| *pid)
            .copied()
        {
            return self.existing_panes.remove(&same_position_candidate_id);
        } else if let Some(first_candidate) =
            candidates.iter().next().map(|(pid, _p)| *pid).copied()
        {
            return self.existing_panes.remove(&first_candidate);
        }
        None
    }
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.existing_panes.keys().copied().collect()
    }
    pub fn remove_pane(&mut self, pane_id: &PaneId) -> Option<Box<dyn Pane>> {
        self.existing_panes.remove(pane_id)
    }
    fn pane_candidates(
        &self,
        run: &Option<Run>,
        position_and_size: &PaneGeom,
        default_to_closest_position: bool,
    ) -> Vec<(&PaneId, &Box<dyn Pane>)> {
        let mut candidates: Vec<_> = self.existing_panes.iter().collect();
        candidates.sort_by(|(a_id, a), (b_id, b)| {
            let a_invoked_with = a.invoked_with();
            let b_invoked_with = b.invoked_with();
            if Run::is_same_category(run, a_invoked_with)
                && !Run::is_same_category(run, b_invoked_with)
            {
                std::cmp::Ordering::Less
            } else if Run::is_same_category(run, b_invoked_with)
                && !Run::is_same_category(run, a_invoked_with)
            {
                std::cmp::Ordering::Greater
            } else if Run::is_terminal(a_invoked_with) && !Run::is_terminal(b_invoked_with) {
                // we place terminals before everything else because when we can't find
                // an exact match, we need to prefer terminals are more often than not
                // we'd be doing the right thing here
                std::cmp::Ordering::Less
            } else if Run::is_terminal(b_invoked_with) && !Run::is_terminal(a_invoked_with) {
                std::cmp::Ordering::Greater
            } else {
                // try to find the closest pane
                if default_to_closest_position {
                    let abs = |a, b| (a as isize - b as isize).abs();
                    let a_x_distance = abs(a.position_and_size().x, position_and_size.x);
                    let a_y_distance = abs(a.position_and_size().y, position_and_size.y);
                    let b_x_distance = abs(b.position_and_size().x, position_and_size.x);
                    let b_y_distance = abs(b.position_and_size().y, position_and_size.y);
                    (a_x_distance + a_y_distance).cmp(&(b_x_distance + b_y_distance))
                } else {
                    a_id.cmp(&b_id) // just so it's a stable sort
                }
            }
        });
        candidates
    }
    fn find_focused_pane_id(
        &self,
        is_focused: bool,
        candidates: &Vec<(&PaneId, &Box<dyn Pane>)>,
    ) -> Option<PaneId> {
        if is_focused {
            candidates
                .iter()
                .find(|(pid, _p)| Some(**pid) == self.currently_focused_pane_id)
                .map(|(pid, _p)| *pid)
                .copied()
        } else {
            None
        }
    }
    fn find_pane_id_with_same_contents(
        &self,
        candidates: &Vec<(&PaneId, &Box<dyn Pane>)>,
        run: &Option<Run>,
    ) -> Option<PaneId> {
        candidates
            .iter()
            .find(|(_pid, p)| p.invoked_with() == run)
            .map(|(pid, _p)| *pid)
            .copied()
    }
    fn find_pane_id_with_same_contents_and_location(
        &self,
        candidates: &Vec<(&PaneId, &Box<dyn Pane>)>,
        run: &Option<Run>,
        position: &PaneGeom,
    ) -> Option<PaneId> {
        candidates
            .iter()
            .find(|(_pid, p)| p.invoked_with() == run && p.position_and_size() == *position)
            .map(|(pid, _p)| *pid)
            .copied()
    }
}

#[derive(Default, Debug)]
struct PaneFocuser {
    refocus_pane: bool,
    pane_id_in_focused_location: Option<PaneId>,
    expanded_stacked_pane_ids: Vec<PaneId>,
}

impl PaneFocuser {
    pub fn new(refocus_pane: bool) -> Self {
        PaneFocuser {
            refocus_pane,
            ..Default::default()
        }
    }
    pub fn set_pane_id_in_focused_location(
        &mut self,
        is_focused: Option<bool>,
        pane: &Box<dyn Pane>,
    ) {
        if is_focused.unwrap_or(false) && pane.selectable() {
            self.pane_id_in_focused_location = Some(pane.pid());
        }
    }
    pub fn set_expanded_stacked_pane(&mut self, is_expanded_in_stack: bool, pane: &Box<dyn Pane>) {
        if is_expanded_in_stack && pane.selectable() {
            self.expanded_stacked_pane_ids.push(pane.pid());
        }
    }
    pub fn focus_tiled_pane(&self, tiled_panes: &mut TiledPanes) {
        let mut panes_in_stack = vec![];
        for pane_id in &self.expanded_stacked_pane_ids {
            panes_in_stack.append(&mut tiled_panes.expand_pane_in_stack(*pane_id));
        }
        match self.pane_id_in_focused_location {
            Some(pane_id_in_focused_location) => {
                if self.refocus_pane {
                    tiled_panes.reapply_pane_focus();
                    if !panes_in_stack.contains(&pane_id_in_focused_location) {
                        // we do not change stacked panes locations because this has already been done above
                        tiled_panes.switch_active_pane_with(pane_id_in_focused_location);
                    }
                } else {
                    tiled_panes.reapply_pane_focus();
                }
            },
            None => {
                tiled_panes.reapply_pane_focus();
            },
        }
        for pane_id in &self.expanded_stacked_pane_ids {
            tiled_panes.expand_pane_in_stack(*pane_id);
        }
    }
    pub fn focus_floating_pane(
        &self,
        floating_panes: &mut FloatingPanes,
        os_api: &mut Box<dyn ServerOsApi>,
    ) {
        floating_panes.reapply_pane_focus();
        if let Some(pane_id_in_focused_location) = self.pane_id_in_focused_location {
            if self.refocus_pane {
                floating_panes.switch_active_pane_with(os_api, pane_id_in_focused_location);
            }
        }
    }
}
