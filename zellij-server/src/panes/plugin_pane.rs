use std::collections::HashMap;
use std::time::Instant;
use std::fmt::{Display, Error, Formatter};

use crate::output::{CharacterChunk, SixelImageChunk};
use crate::panes::{grid::Grid, sixel::SixelImageStore, LinkHandler, PaneId};
use crate::plugins::PluginInstruction;
use crate::pty::VteBytes;
use crate::tab::Pane;
use crate::ui::pane_boundaries_frame::{FrameParams, PaneFrame};
use crate::ClientId;
use std::cell::RefCell;
use std::rc::Rc;
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

macro_rules! get_or_create_grid {
    ($self:ident, $client_id:ident) => {{
        let rows = $self.get_content_rows();
        let cols = $self.get_content_columns();

        $self.grids.entry($client_id).or_insert_with(|| {
            let mut grid = Grid::new(
                rows,
                cols,
                $self.terminal_emulator_colors.clone(),
                $self.terminal_emulator_color_codes.clone(),
                $self.link_handler.clone(),
                $self.character_cell_size.clone(),
                $self.sixel_image_store.clone(),
            );
            grid.hide_cursor();
            grid
        })
    }};
}

#[derive(Debug, Clone)]
pub enum LoadingStatus {
    InProgress,
    Success,
    NotFound,
}

#[derive(Debug, Clone, Default)]
pub struct LoadingIndication {
    loading_from_memory: Option<LoadingStatus>,
    loading_from_hd_cache: Option<LoadingStatus>,
    compiling: Option<LoadingStatus>,
    starting_plugin: Option<LoadingStatus>,
    writing_plugin_to_cache: Option<LoadingStatus>,
    cloning_plugin_for_other_clients: Option<LoadingStatus>,
    error: Option<String>,
    animation_offset: usize,
    plugin_name: String,
    terminal_emulator_colors: Option<Palette>,
    ended: bool,
}

impl LoadingIndication {
    pub fn new(plugin_name: String) -> Self {
        LoadingIndication {
            plugin_name,
            animation_offset: 0,
            ..Default::default()
        }
    }
    pub fn with_colors(mut self, terminal_emulator_colors: Palette) -> Self {
        self.terminal_emulator_colors = Some(terminal_emulator_colors);
        self
    }
    pub fn merge(&mut self, other: LoadingIndication) {
        let current_animation_offset = self.animation_offset;
        let current_terminal_emulator_colors = self.terminal_emulator_colors.take();
        drop(std::mem::replace(self, other));
        self.animation_offset = current_animation_offset;
        self.terminal_emulator_colors = current_terminal_emulator_colors;
    }
    pub fn indicate_loading_plugin_from_memory(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_loading_plugin_from_memory_success(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::Success);
    }
    pub fn indicate_loading_plugin_from_memory_notfound(&mut self) {
        self.loading_from_memory = Some(LoadingStatus::NotFound);
    }
    pub fn indicate_loading_plugin_from_hd_cache(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_loading_plugin_from_hd_cache_success(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::Success);
    }
    pub fn indicate_loading_plugin_from_hd_cache_notfound(&mut self) {
        self.loading_from_hd_cache = Some(LoadingStatus::NotFound);
    }
    pub fn indicate_compiling_plugin(&mut self) {
        self.compiling = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_compiling_plugin_success(&mut self) {
        self.compiling = Some(LoadingStatus::Success);
    }
    pub fn indicate_starting_plugin(&mut self) {
        self.starting_plugin = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_starting_plugin_success(&mut self) {
        self.starting_plugin = Some(LoadingStatus::Success);
    }
    pub fn indicate_writing_plugin_to_cache(&mut self) {
        self.writing_plugin_to_cache = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_writing_plugin_to_cache_success(&mut self) {
        self.writing_plugin_to_cache = Some(LoadingStatus::Success);
    }
    pub fn indicate_cloning_plugin_for_other_clients(&mut self) {
        self.cloning_plugin_for_other_clients = Some(LoadingStatus::InProgress);
    }
    pub fn indicate_cloning_plugin_for_other_clients_success(&mut self) {
        self.cloning_plugin_for_other_clients = Some(LoadingStatus::Success);
    }
    pub fn end(&mut self) {
        self.ended = true;
    }
    pub fn progress_animation_offset(&mut self) {
        if self.animation_offset == 3 {
            self.animation_offset = 0;
        } else {
            self.animation_offset += 1;
        }
    }
    pub fn indicate_loading_error(&mut self, error_text: String) {
        self.error = Some(error_text);
    }
    fn started_loading(&self) -> bool {
        self.loading_from_memory.is_some() ||
        self.loading_from_hd_cache.is_some() ||
        self.compiling.is_some() ||
        self.starting_plugin.is_some() ||
        self.writing_plugin_to_cache.is_some() ||
        self.cloning_plugin_for_other_clients.is_some()
    }
}

// TODO: from zellij-tile-utils??
macro_rules! style {
    ($fg:expr) => {
        ansi_term::Style::new()
            .fg(match $fg {
                PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
                PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
            })
//             .on(match $bg {
//                 PaletteColor::Rgb((r, g, b)) => ansi_term::Color::RGB(r, g, b),
//                 PaletteColor::EightBit(color) => ansi_term::Color::Fixed(color),
//             })
    };
}

impl Display for LoadingIndication {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        // TODO: CONTINUE HERE (22/03 evening) - make this pretty!
        let cyan = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.cyan).bold()
            },
            None => ansi_term::Style::new()
        };
        let green = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.green).bold()
            },
            None => ansi_term::Style::new()
        };
        let yellow = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.yellow).bold()
            },
            None => ansi_term::Style::new()
        };
        let red = match self.terminal_emulator_colors {
            Some(terminal_emulator_colors) => {
                style!(terminal_emulator_colors.red).bold()
            },
            None => ansi_term::Style::new()
        };
        let bold = ansi_term::Style::new().bold().italic();
        let plugin_name = &self.plugin_name;
        let success = green.paint("SUCCESS");
        let failure = red.paint("FAILED");
        let not_found = yellow.paint("NOT FOUND");
        let add_dots = |stringified: &mut String| {
            for _ in 0..self.animation_offset {
                stringified.push('.');
            }
            stringified.push(' ');
        };
        let mut stringified = String::new();
        let loading_text = "Loading";
        let loading_from_memory_text = "Attempting to load from memory";
        let loading_from_hd_cache_text = "Attempting to load from cache";
        let compiling_text = "Compiling WASM";
        let starting_plugin_text = "Starting";
        let writing_plugin_to_cache_text = "Writing to cache";
        let cloning_plugin_for_other_clients_text = "Cloning for other clients";
        if self.started_loading() {
            stringified.push_str(&format!("{} {}...", loading_text, cyan.paint(plugin_name)));
        } else {
            stringified.push_str(&format!("{} {}", bold.paint(loading_text), cyan.italic().paint(plugin_name)));
            add_dots(&mut stringified);
        }
        match self.loading_from_memory {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(loading_from_memory_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{loading_from_memory_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{loading_from_memory_text}... {not_found}"));
            }
            None => {}
        }
        match self.loading_from_hd_cache {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(loading_from_hd_cache_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{loading_from_hd_cache_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{loading_from_hd_cache_text}... {not_found}"));
            }
            None => {}
        }
        match self.compiling {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(compiling_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{compiling_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{compiling_text}... {failure}"));
            }
            None => {}
        }
        match self.starting_plugin {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(starting_plugin_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{starting_plugin_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{starting_plugin_text}... {failure}"));
            }
            None => {}
        }
        match self.writing_plugin_to_cache {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(writing_plugin_to_cache_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{writing_plugin_to_cache_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{writing_plugin_to_cache_text}... {failure}"));
            }
            None => {}
        }
        match self.cloning_plugin_for_other_clients {
            Some(LoadingStatus::InProgress) => {
                stringified.push_str(&format!("\n\r{}", bold.paint(cloning_plugin_for_other_clients_text)));
                add_dots(&mut stringified);
            }
            Some(LoadingStatus::Success) => {
                stringified.push_str(&format!("\n\r{cloning_plugin_for_other_clients_text}... {success}"));
            }
            Some(LoadingStatus::NotFound) => {
                stringified.push_str(&format!("\n\r{cloning_plugin_for_other_clients_text}... {failure}"));
            }
            None => {}
        }
        if let Some(error_text) = &self.error {
            stringified.push_str(&format!("\n\r{} {error_text}", red.bold().paint("ERROR:")));
        }
        write!(f, "{}", stringified)
    }
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
    pane_frame_color_override: Option<(PaletteColor, Option<String>)>,
    invoked_with: Option<Run>,
    loading_indication: LoadingIndication,
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
            link_handler,
            character_cell_size,
            sixel_image_store,
            vte_parsers: HashMap::new(),
            grids: HashMap::new(),
            style,
            pane_frame_color_override: None,
            invoked_with,
            loading_indication,
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
        for &byte in &bytes {
            vte_parser.advance(grid, byte);
        }
        self.should_render.insert(client_id, true);
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        None
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
        if self.loading_indication.ended {
            return;
        }
        self.loading_indication.merge(loading_indication);
        self.handle_plugin_bytes_for_all_clients(self.loading_indication.to_string().as_bytes().to_vec());
    }
    fn progress_animation_offset(&mut self) {
        if self.loading_indication.ended {
            return;
        }
        self.loading_indication.progress_animation_offset();
        self.handle_plugin_bytes_for_all_clients(self.loading_indication.to_string().as_bytes().to_vec());
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
}
