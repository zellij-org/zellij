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
use std::collections::{BTreeMap, HashMap};
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
    connected_clients: Rc<RefCell<HashMap<ClientId, bool>>>,
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
    explicitly_disable_kitty_keyboard_protocol: bool,
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
        connected_clients: &Rc<RefCell<HashMap<ClientId, bool>>>,
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
        explicitly_disable_kitty_keyboard_protocol: bool,
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
            explicitly_disable_kitty_keyboard_protocol,
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
        let hide_floating_panes = layout.hide_floating_panes;
        self.apply_tiled_panes_layout(layout, new_terminal_ids, &mut new_plugin_ids, client_id)?;
        let layout_has_floating_panes = self.apply_floating_panes_layout(
            floating_panes_layout,
            new_floating_terminal_ids,
            &mut new_plugin_ids,
        )?;
        let should_show_floating_panes = layout_has_floating_panes && !hide_floating_panes;
        return Ok(should_show_floating_panes);
    }
    pub fn apply_tiled_panes_layout_to_existing_panes(
        &mut self,
        layout: &TiledPaneLayout,
    ) -> Result<()> {
        let positions_in_layout = self.flatten_layout(layout, true)?;

        let mut existing_tab_state = ExistingTabState::new(self.tiled_panes.drain());

        let mut pane_applier = PaneApplier::new(
            &mut self.tiled_panes,
            &mut self.floating_panes,
            &self.senders,
            &self.character_cell_size,
        );
        let mut positions_left_without_exact_matches = vec![];

        // look for exact matches (eg. panes that expect a specific command or plugin to run in them)
        for (layout, position_and_size) in positions_in_layout {
            match existing_tab_state
                .find_and_extract_exact_match_pane(&layout.run, position_and_size.logical_position)
            {
                Some(pane) => {
                    pane_applier.apply_position_and_size_to_tiled_pane(
                        pane,
                        position_and_size,
                        layout,
                    );
                },
                None => {
                    positions_left_without_exact_matches.push((layout, position_and_size));
                },
            }
        }

        // look for matches according to the logical position in the layout
        let mut positions_left = vec![];
        for (layout, position_and_size) in positions_left_without_exact_matches {
            if let Some(pane) = existing_tab_state.find_and_extract_pane_with_same_logical_position(
                position_and_size.logical_position,
            ) {
                pane_applier.apply_position_and_size_to_tiled_pane(pane, position_and_size, layout);
            } else {
                positions_left.push((layout, position_and_size));
            }
        }

        // fill the remaining panes by order of their logical position
        for (layout, position_and_size) in positions_left {
            // now let's try to find panes on a best-effort basis
            if let Some(pane) =
                existing_tab_state.find_and_extract_pane(position_and_size.logical_position)
            {
                pane_applier.apply_position_and_size_to_tiled_pane(pane, position_and_size, layout);
            }
        }

        // add the rest of the panes where tiled_panes finds room for them (eg. if the layout had
        // less panes than we've got in our state)
        let remaining_pane_ids: Vec<PaneId> = existing_tab_state.pane_ids();
        pane_applier.handle_remaining_tiled_pane_ids(remaining_pane_ids, existing_tab_state);
        pane_applier.finalize_tiled_state();

        LayoutApplier::offset_viewport(
            self.viewport.clone(),
            self.tiled_panes,
            self.draw_pane_frames,
        );
        Ok(())
    }
    fn flatten_layout(
        &self,
        layout: &TiledPaneLayout,
        has_existing_panes: bool,
    ) -> Result<Vec<(TiledPaneLayout, PaneGeom)>> {
        let free_space = self.total_space_for_tiled_panes();
        let tiled_panes_count = if has_existing_panes {
            Some(self.tiled_panes.visible_panes_count())
        } else {
            None
        };
        let focus_layout_if_not_focused = if has_existing_panes { false } else { true };
        let mut positions_in_layout = layout
            .position_panes_in_space(
                &free_space,
                tiled_panes_count,
                false,
                focus_layout_if_not_focused,
            )
            .or_else(|_e| {
                // in the error branch, we try to recover by positioning the panes in the space but
                // ignoring the percentage sizes (passing true as the third argument), this is a hack
                // around some issues with the constraint system that should be addressed in a systemic
                // manner
                layout.position_panes_in_space(
                    &free_space,
                    tiled_panes_count,
                    true,
                    focus_layout_if_not_focused,
                )
            })
            .map_err(|e| anyhow!(e))?;
        let mut logical_position = 0;
        for (_layout, position_and_size) in positions_in_layout.iter_mut() {
            position_and_size.logical_position = Some(logical_position);
            logical_position += 1;
        }
        Ok(positions_in_layout)
    }
    fn apply_tiled_panes_layout(
        &mut self,
        layout: TiledPaneLayout,
        mut new_terminal_ids: Vec<(u32, HoldForCommand)>,
        mut new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to apply tiled panes layout");
        let mut positions_in_layout = self.flatten_layout(&layout, false)?;
        let run_instructions_without_a_location = self.position_run_instructions_to_ignore(
            &layout.run_instructions_to_ignore,
            &mut positions_in_layout,
        );
        let focus_pane_id = self.position_new_panes(
            &mut new_terminal_ids,
            &mut new_plugin_ids,
            &mut positions_in_layout,
        )?;
        self.handle_run_instructions_without_a_location(
            run_instructions_without_a_location,
            &mut new_terminal_ids,
        );
        self.adjust_viewport().with_context(err_context)?;
        self.set_focused_tiled_pane(focus_pane_id, client_id);
        Ok(())
    }
    fn position_run_instructions_to_ignore(
        &mut self,
        run_instructions_to_ignore: &Vec<Option<Run>>,
        positions_in_layout: &mut Vec<(TiledPaneLayout, PaneGeom)>,
    ) -> Vec<Option<Run>> {
        // here we try to find rooms for the panes that are already running (represented by
        // run_instructions_to_ignore), we try to either find an explicit position (the new
        // layout has a pane with the exact run instruction) or an otherwise free position
        // (the new layout has a pane with None as its run instruction, eg. just `pane` in the
        // layout)
        let mut run_instructions_without_a_location = vec![];
        for run_instruction in run_instructions_to_ignore.clone().drain(..) {
            if self
                .place_running_pane_in_exact_match_location(&run_instruction, positions_in_layout)
            {
                // found exact match
            } else if self
                .place_running_pane_in_empty_location(&run_instruction, positions_in_layout)
            {
                // found empty location
            } else {
                // no room! we'll add it below after we place everything else
                run_instructions_without_a_location.push(run_instruction);
            }
        }
        run_instructions_without_a_location
    }
    fn position_new_panes(
        &mut self,
        new_terminal_ids: &mut Vec<(u32, HoldForCommand)>,
        new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
        positions_in_layout: &mut Vec<(TiledPaneLayout, PaneGeom)>,
    ) -> Result<Option<PaneId>> {
        // here we open new panes for each run instruction in the layout with the details
        // we got from the plugin thread and pty thread
        // let positions_and_size = positions_in_layout.iter();
        let mut focus_pane_id: Option<PaneId> = None;
        let mut set_focus_pane_id = |layout: &TiledPaneLayout, pane_id: PaneId| {
            if layout.focus.unwrap_or(false) && focus_pane_id.is_none() {
                focus_pane_id = Some(pane_id);
            }
        };
        for (layout, position_and_size) in positions_in_layout {
            if let Some(Run::Plugin(run)) = layout.run.clone() {
                let pid =
                    self.new_tiled_plugin_pane(run, new_plugin_ids, &position_and_size, &layout)?;
                set_focus_pane_id(&layout, PaneId::Plugin(pid));
            } else if !new_terminal_ids.is_empty() {
                // there are still panes left to fill, use the pids we received in this method
                let (pid, hold_for_command) = new_terminal_ids.remove(0);
                self.new_terminal_pane(pid, &hold_for_command, &position_and_size, &layout)?;
                set_focus_pane_id(&layout, PaneId::Terminal(pid));
            }
        }
        Ok(focus_pane_id)
    }
    fn handle_run_instructions_without_a_location(
        &mut self,
        run_instructions_without_a_location: Vec<Option<Run>>,
        new_terminal_ids: &mut Vec<(u32, HoldForCommand)>,
    ) {
        for run_instruction in run_instructions_without_a_location {
            self.tiled_panes
                .assign_geom_for_pane_with_run(run_instruction);
        }
        for (unused_pid, _) in new_terminal_ids {
            let _ = self.senders.send_to_pty(PtyInstruction::ClosePane(
                PaneId::Terminal(*unused_pid),
                None,
            ));
        }
    }
    fn new_tiled_plugin_pane(
        &mut self,
        run: RunPluginOrAlias,
        new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
        position_and_size: &PaneGeom,
        layout: &TiledPaneLayout,
    ) -> Result<u32> {
        let err_context = || format!("Failed to start new plugin pane");
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
            self.connected_clients.borrow().keys().copied().collect(),
            self.style.clone(),
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
        Ok(pid)
    }
    fn new_floating_plugin_pane(
        &mut self,
        run: RunPluginOrAlias,
        new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
        position_and_size: PaneGeom,
        floating_pane_layout: &FloatingPaneLayout,
    ) -> Result<Option<PaneId>> {
        let mut pid_to_focus = None;
        let err_context = || format!("Failed to create new floating plugin pane");
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
            floating_pane_layout.name.clone().unwrap_or_default(),
            self.sixel_image_store.clone(),
            self.terminal_emulator_colors.clone(),
            self.terminal_emulator_color_codes.clone(),
            self.link_handler.clone(),
            self.character_cell_size.clone(),
            self.connected_clients.borrow().keys().copied().collect(),
            self.style.clone(),
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
            pid_to_focus = Some(PaneId::Plugin(pid));
        }
        Ok(pid_to_focus)
    }
    fn new_floating_terminal_pane(
        &mut self,
        pid: &u32,
        hold_for_command: &HoldForCommand,
        position_and_size: PaneGeom,
        floating_pane_layout: &FloatingPaneLayout,
    ) -> Result<Option<PaneId>> {
        let mut pane_id_to_focus = None;
        let next_terminal_position =
            get_next_terminal_position(&self.tiled_panes, &self.floating_panes);
        let initial_title = match &floating_pane_layout.run {
            Some(Run::Command(run_command)) => Some(run_command.to_string()),
            _ => None,
        };
        let mut new_pane = TerminalPane::new(
            *pid,
            position_and_size,
            self.style.clone(),
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
            self.explicitly_disable_kitty_keyboard_protocol,
            None,
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
            pane_id_to_focus = Some(PaneId::Terminal(*pid));
        }
        Ok(pane_id_to_focus)
    }
    fn new_terminal_pane(
        &mut self,
        pid: u32,
        hold_for_command: &HoldForCommand,
        position_and_size: &PaneGeom,
        layout: &TiledPaneLayout,
    ) -> Result<()> {
        let next_terminal_position =
            get_next_terminal_position(&self.tiled_panes, &self.floating_panes);
        let initial_title = match &layout.run {
            Some(Run::Command(run_command)) => Some(run_command.to_string()),
            _ => None,
        };
        let mut new_pane = TerminalPane::new(
            pid,
            *position_and_size,
            self.style.clone(),
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
            self.explicitly_disable_kitty_keyboard_protocol,
            None,
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
            .add_pane_with_existing_geom(PaneId::Terminal(pid), Box::new(new_pane));
        Ok(())
    }
    fn place_running_pane_in_exact_match_location(
        &mut self,
        run_instruction: &Option<Run>,
        positions_in_layout: &mut Vec<(TiledPaneLayout, PaneGeom)>,
    ) -> bool {
        let mut found_exact_match = false;

        if let Some(position) = positions_in_layout
            .iter()
            .position(|(layout, _position_and_size)| &layout.run == run_instruction)
        {
            let (layout, position_and_size) = positions_in_layout.remove(position);
            self.tiled_panes.set_geom_for_pane_with_run(
                layout.run,
                position_and_size,
                layout.borderless,
            );
            found_exact_match = true;
        }
        found_exact_match
    }
    fn place_running_pane_in_empty_location(
        &mut self,
        run_instruction: &Option<Run>,
        positions_in_layout: &mut Vec<(TiledPaneLayout, PaneGeom)>,
    ) -> bool {
        let mut found_empty_location = false;
        if let Some(position) = positions_in_layout
            .iter()
            .position(|(layout, _position_and_size)| layout.run.is_none())
        {
            let (layout, position_and_size) = positions_in_layout.remove(position);
            self.tiled_panes.set_geom_for_pane_with_run(
                run_instruction.clone(),
                position_and_size,
                layout.borderless,
            );
            found_empty_location = true;
        }
        found_empty_location
    }
    fn apply_floating_panes_layout(
        &mut self,
        mut floating_panes_layout: Vec<FloatingPaneLayout>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        mut new_plugin_ids: &mut HashMap<RunPluginOrAlias, Vec<u32>>,
    ) -> Result<bool> {
        let layout_has_floating_panes = !floating_panes_layout.is_empty();

        let mut logical_position = 0;
        for floating_pane_layout in floating_panes_layout.iter_mut() {
            floating_pane_layout.logical_position = Some(logical_position);
            logical_position += 1;
        }

        let floating_panes_layout = floating_panes_layout.iter();
        let mut focused_floating_pane = None;
        let mut new_floating_terminal_ids = new_floating_terminal_ids.iter();
        for floating_pane_layout in floating_panes_layout {
            let position_and_size = self
                .floating_panes
                .position_floating_pane_layout(&floating_pane_layout)?;
            let pid_to_focus = if floating_pane_layout.already_running {
                self.floating_panes.set_geom_for_pane_with_run(
                    floating_pane_layout.run.clone(),
                    position_and_size,
                );
                None
            } else if let Some(Run::Plugin(run)) = floating_pane_layout.run.clone() {
                self.new_floating_plugin_pane(
                    run,
                    &mut new_plugin_ids,
                    position_and_size,
                    &floating_pane_layout,
                )?
            } else if let Some((pid, hold_for_command)) = new_floating_terminal_ids.next() {
                self.new_floating_terminal_pane(
                    pid,
                    hold_for_command,
                    position_and_size,
                    floating_pane_layout,
                )?
            } else {
                None
            };
            if let Some(pid_to_focus) = pid_to_focus {
                focused_floating_pane = Some(pid_to_focus);
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
    ) -> Result<bool> {
        let layout_has_floating_panes = self.floating_panes.has_panes();
        let mut positions_in_layout = floating_panes_layout.clone();
        let mut logical_position = 0;
        for floating_pane_layout in positions_in_layout.iter_mut() {
            floating_pane_layout.logical_position = Some(logical_position);
            logical_position += 1;
        }
        let mut existing_tab_state = ExistingTabState::new(self.floating_panes.drain());
        let mut pane_applier = PaneApplier::new(
            &mut self.tiled_panes,
            &mut self.floating_panes,
            &self.senders,
            &self.character_cell_size,
        );
        let mut panes_to_apply = vec![];
        let mut positions_left = vec![];

        // look for exact matches, first by pane contents and then by logical position
        for floating_pane_layout in positions_in_layout {
            match existing_tab_state
                .find_and_extract_exact_match_pane(
                    &floating_pane_layout.run,
                    floating_pane_layout.logical_position,
                )
                .or_else(|| {
                    existing_tab_state.find_and_extract_pane_with_same_logical_position(
                        floating_pane_layout.logical_position,
                    )
                }) {
                Some(pane) => {
                    panes_to_apply.push((pane, floating_pane_layout));
                },
                None => {
                    positions_left.push(floating_pane_layout);
                },
            }
        }

        // fill the remaining panes by order of their logical position
        for floating_pane_layout in positions_left {
            if let Some(pane) =
                existing_tab_state.find_and_extract_pane(floating_pane_layout.logical_position)
            {
                panes_to_apply.push((pane, floating_pane_layout));
            }
        }

        // here we apply positioning to all panes by the order we found them
        // this is because the positioning decisions themselves rely on this order for geoms that
        // contain partial positioning information (eg. just x coords with no y or size) or no
        // positioning information at all
        for (pane, floating_pane_layout) in panes_to_apply.drain(..) {
            pane_applier
                .apply_floating_panes_layout_to_floating_pane(pane, floating_pane_layout)?;
        }

        // here we apply positioning on a best-effort basis to any remaining panes we've got (these
        // are panes that exist in the tab state but not in the desired layout)
        pane_applier.handle_remaining_floating_pane_ids(existing_tab_state, logical_position);
        pane_applier.finalize_floating_panes_state();

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
}

impl ExistingTabState {
    pub fn new(existing_panes: BTreeMap<PaneId, Box<dyn Pane>>) -> Self {
        ExistingTabState { existing_panes }
    }
    pub fn find_and_extract_exact_match_pane(
        &mut self,
        run: &Option<Run>,
        logical_position: Option<usize>,
    ) -> Option<Box<dyn Pane>> {
        let candidates = self.pane_candidates();
        if let Some(current_pane_id_with_same_contents) =
            self.find_pane_id_with_same_contents(&candidates, run, logical_position)
        {
            return self
                .existing_panes
                .remove(&current_pane_id_with_same_contents);
        }
        None
    }
    pub fn find_and_extract_pane_with_same_logical_position(
        &mut self,
        logical_position: Option<usize>,
    ) -> Option<Box<dyn Pane>> {
        let candidates = self.pane_candidates();
        if let Some(current_pane_id_with_same_logical_position) =
            self.find_pane_id_with_same_logical_position(&candidates, logical_position)
        {
            return self
                .existing_panes
                .remove(&current_pane_id_with_same_logical_position);
        } else {
            return None;
        }
    }
    pub fn find_and_extract_pane(
        &mut self,
        logical_position: Option<usize>,
    ) -> Option<Box<dyn Pane>> {
        let candidates = self.pane_candidates();
        if let Some(current_pane_id_with_same_logical_position) =
            self.find_pane_id_with_same_logical_position(&candidates, logical_position)
        {
            return self
                .existing_panes
                .remove(&current_pane_id_with_same_logical_position);
        } else {
            match candidates.iter().next().map(|(pid, _p)| *pid).copied() {
                Some(first_candidate) => self.existing_panes.remove(&first_candidate),
                None => None,
            }
        }
    }
    pub fn pane_ids(&self) -> Vec<PaneId> {
        self.existing_panes.keys().copied().collect()
    }
    pub fn remove_pane(&mut self, pane_id: &PaneId) -> Option<Box<dyn Pane>> {
        self.existing_panes.remove(pane_id)
    }
    fn pane_candidates(&self) -> Vec<(&PaneId, &Box<dyn Pane>)> {
        let mut candidates: Vec<_> = self.existing_panes.iter().collect();
        candidates.sort_by(|(a_id, a), (b_id, b)| {
            let a_logical_position = a.position_and_size().logical_position;
            let b_logical_position = b.position_and_size().logical_position;
            if a_logical_position != b_logical_position {
                a_logical_position.cmp(&b_logical_position)
            } else {
                a_id.cmp(&b_id)
            }
        });
        candidates
    }
    fn find_pane_id_with_same_contents(
        &self,
        candidates: &Vec<(&PaneId, &Box<dyn Pane>)>,
        run: &Option<Run>,
        pane_logical_position: Option<usize>,
    ) -> Option<PaneId> {
        if run.is_none() {
            return None;
        }
        let panes_with_same_contents = candidates
            .iter()
            .filter(|(_pid, p)| p.invoked_with() == run)
            .collect::<Vec<_>>();
        if panes_with_same_contents.len() > 1 {
            panes_with_same_contents
                .iter()
                .find(|(_pid, p)| p.position_and_size().logical_position == pane_logical_position)
                .map(|(pid, _p)| *pid)
                .copied()
                .or_else(|| {
                    panes_with_same_contents
                        .iter()
                        .next()
                        .map(|(pid, _p)| *pid)
                        .copied()
                })
        } else {
            panes_with_same_contents
                .iter()
                .next()
                .map(|(pid, _p)| *pid)
                .copied()
        }
    }
    fn find_pane_id_with_same_logical_position(
        &self,
        candidates: &Vec<(&PaneId, &Box<dyn Pane>)>,
        logical_position: Option<usize>,
    ) -> Option<PaneId> {
        candidates
            .iter()
            .find(|(_pid, p)| p.position_and_size().logical_position == logical_position)
            .map(|(pid, _p)| *pid)
            .copied()
    }
}

struct PaneApplier<'a> {
    new_focused_pane_id: Option<PaneId>,
    pane_ids_expanded_in_stack: Vec<PaneId>,
    tiled_panes: &'a mut TiledPanes,
    floating_panes: &'a mut FloatingPanes,
    senders: ThreadSenders,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
}

impl<'a> PaneApplier<'a> {
    pub fn new(
        tiled_panes: &'a mut TiledPanes,
        floating_panes: &'a mut FloatingPanes,
        senders: &ThreadSenders,
        character_cell_size: &Rc<RefCell<Option<SizeInPixels>>>,
    ) -> Self {
        PaneApplier {
            new_focused_pane_id: None,
            pane_ids_expanded_in_stack: vec![],
            tiled_panes,
            floating_panes,
            senders: senders.clone(),
            character_cell_size: character_cell_size.clone(),
        }
    }
    pub fn apply_position_and_size_to_tiled_pane(
        &mut self,
        mut pane: Box<dyn Pane>,
        position_and_size: PaneGeom,
        layout: TiledPaneLayout,
    ) {
        self.apply_layout_properties_to_pane(&mut pane, &layout, Some(position_and_size));
        if layout.focus.unwrap_or(false) {
            self.new_focused_pane_id = Some(pane.pid());
        }
        if layout.is_expanded_in_stack {
            self.pane_ids_expanded_in_stack.push(pane.pid());
        }
        let _ = resize_pty!(pane, self.os_api, self.senders, self.character_cell_size);
        self.tiled_panes
            .add_pane_with_existing_geom(pane.pid(), pane);
    }
    pub fn apply_floating_panes_layout_to_floating_pane(
        &mut self,
        mut pane: Box<dyn Pane>,
        floating_panes_layout: FloatingPaneLayout,
    ) -> Result<()> {
        let position_and_size = self
            .floating_panes
            .position_floating_pane_layout(&floating_panes_layout)?;
        if let Some(pane_title) = floating_panes_layout.name.as_ref() {
            pane.set_title(pane_title.into());
        }
        if floating_panes_layout.focus.unwrap_or(false) {
            self.new_focused_pane_id = Some(pane.pid());
        }
        self.apply_position_and_size_to_floating_pane(pane, position_and_size);
        Ok(())
    }
    pub fn apply_position_and_size_to_floating_pane(
        &mut self,
        mut pane: Box<dyn Pane>,
        position_and_size: PaneGeom,
    ) {
        pane.set_geom(position_and_size);
        let _ = resize_pty!(pane, self.os_api, self.senders, self.character_cell_size);
        self.floating_panes.add_pane(pane.pid(), pane);
    }

    pub fn handle_remaining_tiled_pane_ids(
        &mut self,
        remaining_pane_ids: Vec<PaneId>,
        mut existing_tab_state: ExistingTabState,
    ) {
        for pane_id in remaining_pane_ids {
            if let Some(pane) = existing_tab_state.remove_pane(&pane_id) {
                self.tiled_panes.insert_pane(pane.pid(), pane, None);
            }
        }
    }
    pub fn handle_remaining_floating_pane_ids(
        &mut self,
        mut existing_tab_state: ExistingTabState,
        logical_position: usize,
    ) {
        let remaining_pane_ids: Vec<PaneId> = existing_tab_state.pane_ids();
        for pane_id in remaining_pane_ids {
            match self.floating_panes.find_room_for_new_pane() {
                Some(mut position_and_size) => {
                    if let Some(pane) = existing_tab_state.remove_pane(&pane_id) {
                        position_and_size.logical_position = Some(logical_position);
                        self.apply_position_and_size_to_floating_pane(pane, position_and_size);
                    }
                },
                None => {
                    log::error!("could not find room for pane!")
                },
            }
        }
    }
    pub fn finalize_tiled_state(&mut self) {
        // do some housekeeping to apply various layout properties to panes
        for pane_id in &self.pane_ids_expanded_in_stack {
            self.tiled_panes.expand_pane_in_stack(*pane_id);
        }
        if let Some(pane_id) = self.new_focused_pane_id {
            self.tiled_panes.focus_pane_for_all_clients(pane_id);
        }
        self.tiled_panes.reapply_pane_focus();
    }
    pub fn finalize_floating_panes_state(&mut self) {
        // do some housekeeping to apply various layout properties to panes
        if let Some(pane_id) = self.new_focused_pane_id {
            self.floating_panes.focus_pane_for_all_clients(pane_id);
        }
        self.floating_panes.reapply_pane_focus();
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
}
