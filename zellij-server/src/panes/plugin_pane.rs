use std::collections::{BTreeSet, HashMap};
use std::time::Instant;

use crate::output::{CharacterChunk, SixelImageChunk};
use crate::panes::{
    grid::Grid,
    sixel::SixelImageStore,
    terminal_pane::{BRACKETED_PASTE_BEGIN, BRACKETED_PASTE_END},
    LinkHandler, PaneId,
};
use crate::plugins::PluginInstruction;
use crate::pty::VteBytes;
use crate::tab::{AdjustedInput, Pane};
use crate::ui::{
    loading_indication::LoadingIndication,
    pane_boundaries_frame::{FrameParams, PaneFrame},
};
use crate::ClientId;
use std::cell::RefCell;
use std::rc::Rc;
use zellij_utils::data::{
    BareKey, KeyWithModifier, PermissionStatus, PermissionType, PluginPermission,
};
use zellij_utils::pane_size::{Offset, SizeInPixels};
use zellij_utils::position::Position;
use zellij_utils::{
    channels::SenderWithContext,
    data::{Event, InputMode, Mouse, Palette, PaletteColor, Style},
    errors::prelude::*,
    input::layout::Run,
    pane_size::PaneGeom,
    shared::make_terminal_title,
    vte,
};

macro_rules! style {
    ($fg:expr) => {
        ansi_term::Style::new().fg(match $fg {
            PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
            PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
        })
    };
}

macro_rules! get_or_create_grid {
    ($self:ident, $client_id:ident) => {{
        let rows = $self.get_content_rows();
        let cols = $self.get_content_columns();
        let explicitly_disable_kitty_keyboard_protocol = false; // N/A for plugins

        $self.grids.entry($client_id).or_insert_with(|| {
            let mut grid = Grid::new(
                rows,
                cols,
                $self.terminal_emulator_colors.clone(),
                $self.terminal_emulator_color_codes.clone(),
                $self.link_handler.clone(),
                $self.character_cell_size.clone(),
                $self.sixel_image_store.clone(),
                $self.style.clone(),
                $self.debug,
                $self.arrow_fonts,
                $self.styled_underlines,
                explicitly_disable_kitty_keyboard_protocol,
            );
            grid.hide_cursor();
            grid
        })
    }};
}

pub(crate) struct PluginPane {
    pub pid: u32,
    pub should_render: HashMap<ClientId, bool>,
    pub selectable: bool,
    pub geom: PaneGeom,
    pub geom_override: Option<PaneGeom>,
    pub content_offset: Offset,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub active_at: Instant,
    pub pane_title: String,
    pub pane_name: String,
    pub style: Style,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    link_handler: Rc<RefCell<LinkHandler>>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    vte_parsers: HashMap<ClientId, vte::Parser>,
    grids: HashMap<ClientId, Grid>,
    prev_pane_name: String,
    frame: HashMap<ClientId, PaneFrame>,
    borderless: bool,
    exclude_from_sync: bool,
    pane_frame_color_override: Option<(PaletteColor, Option<String>)>,
    invoked_with: Option<Run>,
    loading_indication: LoadingIndication,
    requesting_permissions: Option<PluginPermission>,
    debug: bool,
    arrow_fonts: bool,
    styled_underlines: bool,
}

impl PluginPane {
    pub fn new(
        pid: u32,
        position_and_size: PaneGeom,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        title: String,
        pane_name: String,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
        link_handler: Rc<RefCell<LinkHandler>>,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        currently_connected_clients: Vec<ClientId>,
        style: Style,
        invoked_with: Option<Run>,
        debug: bool,
        arrow_fonts: bool,
        styled_underlines: bool,
    ) -> Self {
        let loading_indication = LoadingIndication::new(title.clone()).with_colors(style.colors);
        let initial_loading_message = loading_indication.to_string();
        let mut plugin = PluginPane {
            pid,
            should_render: HashMap::new(),
            selectable: true,
            geom: position_and_size,
            geom_override: None,
            send_plugin_instructions,
            active_at: Instant::now(),
            frame: HashMap::new(),
            content_offset: Offset::default(),
            pane_title: title,
            borderless: false,
            pane_name: pane_name.clone(),
            prev_pane_name: pane_name,
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            exclude_from_sync: false,
            link_handler,
            character_cell_size,
            sixel_image_store,
            vte_parsers: HashMap::new(),
            grids: HashMap::new(),
            style,
            pane_frame_color_override: None,
            invoked_with,
            loading_indication,
            requesting_permissions: None,
            debug,
            arrow_fonts,
            styled_underlines,
        };
        for client_id in currently_connected_clients {
            plugin.handle_plugin_bytes(client_id, initial_loading_message.as_bytes().to_vec());
        }
        plugin
    }
}

impl Pane for PluginPane {
    // FIXME: These position and size things should all be moved to default trait implementations,
    // with something like a get_pos_and_sz() method underpinning all of them. Alternatively and
    // preferably, just use an enum and not a trait object
    fn x(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).x
    }
    fn y(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).y
    }
    fn rows(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).rows.as_usize()
    }
    fn cols(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).cols.as_usize()
    }
    fn get_content_x(&self) -> usize {
        self.x() + self.content_offset.left
    }
    fn get_content_y(&self) -> usize {
        self.y() + self.content_offset.top
    }
    fn get_content_columns(&self) -> usize {
        // content columns might differ from the pane's columns if the pane has a frame
        // in that case they would be 2 less
        self.cols()
            .saturating_sub(self.content_offset.left + self.content_offset.right)
    }
    fn get_content_rows(&self) -> usize {
        // content rows might differ from the pane's rows if the pane has a frame
        // in that case they would be 2 less
        self.rows()
            .saturating_sub(self.content_offset.top + self.content_offset.bottom)
    }
    fn reset_size_and_position_override(&mut self) {
        self.geom_override = None;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.geom = position_and_size;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
        self.resize_grids();
        self.set_should_render(true);
    }
    fn handle_plugin_bytes(&mut self, client_id: ClientId, bytes: VteBytes) {
        self.set_client_should_render(client_id, true);

        let mut vte_bytes = bytes;
        if let Some(plugin_permission) = &self.requesting_permissions {
            vte_bytes = self
                .display_request_permission_message(plugin_permission)
                .into();
        }

        let grid = get_or_create_grid!(self, client_id);

        // this is part of the plugin contract, whenever we update the plugin and call its render function, we delete the existing viewport
        // and scroll, reset the cursor position and make sure all the viewport is rendered
        grid.delete_viewport_and_scroll();
        grid.reset_cursor_position();
        grid.render_full_viewport();

        let vte_parser = self
            .vte_parsers
            .entry(client_id)
            .or_insert_with(|| vte::Parser::new());

        for &byte in &vte_bytes {
            vte_parser.advance(grid, byte);
        }

        self.should_render.insert(client_id, true);
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        None
    }
    fn adjust_input_to_terminal(
        &mut self,
        key_with_modifier: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        _raw_input_bytes_are_kitty: bool,
    ) -> Option<AdjustedInput> {
        if let Some(requesting_permissions) = &self.requesting_permissions {
            let permissions = requesting_permissions.permissions.clone();
            if let Some(key_with_modifier) = key_with_modifier {
                match key_with_modifier.bare_key {
                    BareKey::Char('y') if key_with_modifier.has_no_modifiers() => {
                        Some(AdjustedInput::PermissionRequestResult(
                            permissions,
                            PermissionStatus::Granted,
                        ))
                    },
                    BareKey::Char('n') if key_with_modifier.has_no_modifiers() => {
                        Some(AdjustedInput::PermissionRequestResult(
                            permissions,
                            PermissionStatus::Denied,
                        ))
                    },
                    _ => None,
                }
            } else {
                match raw_input_bytes.as_slice() {
                    // Y or y
                    &[89] | &[121] => Some(AdjustedInput::PermissionRequestResult(
                        permissions,
                        PermissionStatus::Granted,
                    )),
                    // N or n
                    &[78] | &[110] => Some(AdjustedInput::PermissionRequestResult(
                        permissions,
                        PermissionStatus::Denied,
                    )),
                    _ => None,
                }
            }
        } else if let Some(key_with_modifier) = key_with_modifier {
            Some(AdjustedInput::WriteKeyToPlugin(key_with_modifier.clone()))
        } else if raw_input_bytes.as_slice() == BRACKETED_PASTE_BEGIN
            || raw_input_bytes.as_slice() == BRACKETED_PASTE_END
        {
            // plugins do not need bracketed paste
            None
        } else {
            Some(AdjustedInput::WriteBytesToTerminal(raw_input_bytes))
        }
    }
    fn position_and_size(&self) -> PaneGeom {
        self.geom
    }
    fn current_geom(&self) -> PaneGeom {
        self.geom_override.unwrap_or(self.geom)
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.geom_override
    }
    fn should_render(&self) -> bool {
        // set should_render for all clients
        self.should_render.values().any(|v| *v)
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.should_render
            .values_mut()
            .for_each(|v| *v = should_render);
    }
    fn render_full_viewport(&mut self) {
        // this marks the pane for a full re-render, rather than just rendering the
        // diff as it usually does with the OutputBuffer
        self.frame.clear();
        for grid in self.grids.values_mut() {
            grid.render_full_viewport();
        }
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn request_permissions_from_user(&mut self, permissions: Option<PluginPermission>) {
        self.requesting_permissions = permissions;
    }
    fn render(
        &mut self,
        client_id: Option<ClientId>,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)>> {
        if client_id.is_none() {
            return Ok(None);
        }
        if let Some(client_id) = client_id {
            if self.should_render.get(&client_id).copied().unwrap_or(false) {
                let content_x = self.get_content_x();
                let content_y = self.get_content_y();
                let rows = self.get_content_rows();
                let columns = self.get_content_columns();
                if rows < 1 || columns < 1 {
                    return Ok(None);
                }
                if let Some(grid) = self.grids.get_mut(&client_id) {
                    match grid.render(content_x, content_y, &self.style) {
                        Ok(rendered_assets) => {
                            self.should_render.insert(client_id, false);
                            return Ok(rendered_assets);
                        },
                        e => return e,
                    }
                }
            }
        }
        Ok(None)
    }
    fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>> {
        if self.borderless {
            return Ok(None);
        }
        if let Some(grid) = self.grids.get(&client_id) {
            let err_context = || format!("failed to render frame for client {client_id}");
            let pane_title = if let Some(text_color_override) = self
                .pane_frame_color_override
                .as_ref()
                .and_then(|(_color, text)| text.as_ref())
            {
                text_color_override.into()
            } else if self.pane_name.is_empty()
                && input_mode == InputMode::RenamePane
                && frame_params.is_main_client
            {
                String::from("Enter name...")
            } else if self.pane_name.is_empty() {
                grid.title
                    .clone()
                    .unwrap_or_else(|| self.pane_title.clone())
            } else {
                self.pane_name.clone()
            };

            let mut frame_geom = self.current_geom();
            if !frame_params.should_draw_pane_frames {
                // in this case the width of the frame needs not include the pane corners
                frame_geom
                    .cols
                    .set_inner(frame_geom.cols.as_usize().saturating_sub(1));
            }
            let mut frame = PaneFrame::new(
                frame_geom.into(),
                grid.scrollback_position_and_length(),
                pane_title,
                frame_params,
            );
            if let Some((frame_color_override, _text)) = self.pane_frame_color_override.as_ref() {
                frame.override_color(*frame_color_override);
            }

            let res = match self.frame.get(&client_id) {
                // TODO: use and_then or something?
                Some(last_frame) => {
                    if &frame != last_frame {
                        if !self.borderless {
                            let frame_output = frame.render().with_context(err_context)?;
                            self.frame.insert(client_id, frame);
                            Some(frame_output)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                },
                None => {
                    if !self.borderless {
                        let frame_output = frame.render().with_context(err_context)?;
                        self.frame.insert(client_id, frame);
                        Some(frame_output)
                    } else {
                        None
                    }
                },
            };
            Ok(res)
        } else {
            Ok(None)
        }
    }
    fn render_fake_cursor(
        &mut self,
        _cursor_color: PaletteColor,
        _text_color: PaletteColor,
    ) -> Option<String> {
        None
    }
    fn render_terminal_title(&mut self, input_mode: InputMode) -> String {
        let pane_title = if self.pane_name.is_empty() && input_mode == InputMode::RenamePane {
            "Enter name..."
        } else if self.pane_name.is_empty() {
            &self.pane_title
        } else {
            &self.pane_name
        };
        make_terminal_title(pane_title)
    }
    fn update_name(&mut self, name: &str) {
        match name {
            "\0" => {
                self.pane_name = String::new();
            },
            "\u{007F}" | "\u{0008}" => {
                //delete and backspace keys
                self.pane_name.pop();
            },
            c => {
                self.pane_name.push_str(c);
            },
        }
    }
    fn pid(&self) -> PaneId {
        PaneId::Plugin(self.pid)
    }
    fn reduce_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows.set_percent(p - percent);
            self.resize_grids();
            self.set_should_render(true);
        }
    }
    fn increase_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows.set_percent(p + percent);
            self.resize_grids();
            self.set_should_render(true);
        }
    }
    fn reduce_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols.set_percent(p - percent);
            self.resize_grids();
            self.set_should_render(true);
        }
    }
    fn increase_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols.set_percent(p + percent);
            self.resize_grids();
            self.set_should_render(true);
        }
    }
    fn push_down(&mut self, count: usize) {
        self.geom.y += count;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn push_right(&mut self, count: usize) {
        self.geom.x += count;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn pull_left(&mut self, count: usize) {
        self.geom.x -= count;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn pull_up(&mut self, count: usize) {
        self.geom.y -= count;
        self.resize_grids();
        self.set_should_render(true);
    }
    fn scroll_up(&mut self, count: usize, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::ScrollUp(count)),
            )]))
            .unwrap();
    }
    fn scroll_down(&mut self, count: usize, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::ScrollDown(count)),
            )]))
            .unwrap();
    }
    fn clear_screen(&mut self) {
        // do nothing
    }
    fn clear_scroll(&mut self) {
        // noop
    }
    fn start_selection(&mut self, start: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::LeftClick(start.line.0, start.column.0)),
            )]))
            .unwrap();
    }
    fn update_selection(&mut self, position: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::Hold(position.line.0, position.column.0)),
            )]))
            .unwrap();
    }
    fn end_selection(&mut self, end: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::Release(end.line(), end.column())),
            )]))
            .unwrap();
    }
    fn is_scrolled(&self) -> bool {
        false
    }

    fn active_at(&self) -> Instant {
        self.active_at
    }

    fn set_active_at(&mut self, time: Instant) {
        self.active_at = time;
    }
    fn set_frame(&mut self, _frame: bool) {
        self.frame.clear();
    }
    fn set_content_offset(&mut self, offset: Offset) {
        self.content_offset = offset;
        self.resize_grids();
    }

    fn store_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.prev_pane_name = self.pane_name.clone()
        }
    }
    fn load_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.pane_name = self.prev_pane_name.clone()
        }
    }

    fn set_borderless(&mut self, borderless: bool) {
        self.borderless = borderless;
    }
    fn borderless(&self) -> bool {
        self.borderless
    }
    fn set_exclude_from_sync(&mut self, exclude_from_sync: bool) {
        self.exclude_from_sync = exclude_from_sync;
    }
    fn exclude_from_sync(&self) -> bool {
        self.exclude_from_sync
    }
    fn handle_right_click(&mut self, to: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(vec![(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::RightClick(to.line.0, to.column.0)),
            )]))
            .unwrap();
    }
    fn add_red_pane_frame_color_override(&mut self, error_text: Option<String>) {
        self.pane_frame_color_override = Some((self.style.colors.red, error_text));
    }
    fn clear_pane_frame_color_override(&mut self) {
        self.pane_frame_color_override = None;
    }
    fn frame_color_override(&self) -> Option<PaletteColor> {
        self.pane_frame_color_override
            .as_ref()
            .map(|(color, _text)| *color)
    }
    fn invoked_with(&self) -> &Option<Run> {
        &self.invoked_with
    }
    fn set_title(&mut self, title: String) {
        self.pane_title = title;
    }
    fn update_loading_indication(&mut self, loading_indication: LoadingIndication) {
        if self.loading_indication.ended && !loading_indication.is_error() {
            return;
        }
        self.loading_indication.merge(loading_indication);
        self.handle_plugin_bytes_for_all_clients(
            self.loading_indication.to_string().as_bytes().to_vec(),
        );
    }
    fn start_loading_indication(&mut self, loading_indication: LoadingIndication) {
        self.loading_indication.merge(loading_indication);
        self.handle_plugin_bytes_for_all_clients(
            self.loading_indication.to_string().as_bytes().to_vec(),
        );
    }
    fn progress_animation_offset(&mut self) {
        if self.loading_indication.ended {
            return;
        }
        self.loading_indication.progress_animation_offset();
        self.handle_plugin_bytes_for_all_clients(
            self.loading_indication.to_string().as_bytes().to_vec(),
        );
    }
    fn current_title(&self) -> String {
        if self.pane_name.is_empty() {
            self.pane_title.to_owned()
        } else {
            self.pane_name.to_owned()
        }
    }
    fn custom_title(&self) -> Option<String> {
        if self.pane_name.is_empty() {
            None
        } else {
            Some(self.pane_name.clone())
        }
    }
    fn rename(&mut self, buf: Vec<u8>) {
        self.pane_name = String::from_utf8_lossy(&buf).to_string();
        self.set_should_render(true);
    }
}

impl PluginPane {
    fn resize_grids(&mut self) {
        let content_rows = self.get_content_rows();
        let content_columns = self.get_content_columns();
        for grid in self.grids.values_mut() {
            grid.change_size(content_rows, content_columns);
        }
        self.set_should_render(true);
    }
    fn set_client_should_render(&mut self, client_id: ClientId, should_render: bool) {
        self.should_render.insert(client_id, should_render);
    }
    fn handle_plugin_bytes_for_all_clients(&mut self, bytes: VteBytes) {
        let client_ids: Vec<ClientId> = self.grids.keys().copied().collect();
        for client_id in client_ids {
            self.handle_plugin_bytes(client_id, bytes.clone());
        }
    }
    fn display_request_permission_message(&self, plugin_permission: &PluginPermission) -> String {
        let bold_white = style!(self.style.colors.white).bold();
        let cyan = style!(self.style.colors.cyan).bold();
        let orange = style!(self.style.colors.orange).bold();
        let green = style!(self.style.colors.green).bold();

        let mut messages = String::new();
        let permissions: BTreeSet<PermissionType> =
            plugin_permission.permissions.clone().into_iter().collect();

        let min_row_count = permissions.len() + 4;

        if self.rows() >= min_row_count {
            messages.push_str(&format!(
                "{} {} {}\n",
                bold_white.paint("Plugin"),
                cyan.paint(&plugin_permission.name),
                bold_white.paint("asks permission to:"),
            ));
            permissions.iter().enumerate().for_each(|(i, p)| {
                messages.push_str(&format!(
                    "\n\r{}. {}",
                    bold_white.paint(&format!("{}", i + 1)),
                    orange.paint(p.display_name())
                ));
            });

            messages.push_str(&format!(
                "\n\n\r{} {}",
                bold_white.paint("Allow?"),
                green.paint("(y/n)"),
            ));
        } else {
            messages.push_str(&format!(
                "{} {}. {} {}",
                bold_white.paint("This plugin asks permission to:"),
                orange.paint(
                    permissions
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                bold_white.paint("Allow?"),
                green.paint("(y/n)"),
            ));
        }

        messages
    }
}
