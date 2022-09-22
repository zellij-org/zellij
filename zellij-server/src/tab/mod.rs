//! `Tab`s holds multiple panes. It tracks their coordinates (x/y) and size,
//! as well as how they should be resized

mod clipboard;
mod copy_command;

use copy_command::CopyCommand;
use std::env::temp_dir;
use uuid::Uuid;
use zellij_utils::position::{Column, Line};
use zellij_utils::{position::Position, serde};

use crate::pty_writer::PtyWriteInstruction;
use crate::screen::CopyOptions;
use crate::ui::pane_boundaries_frame::FrameParams;

use self::clipboard::ClipboardProvider;
use crate::{
    os_input_output::ServerOsApi,
    output::{CharacterChunk, Output, SixelImageChunk},
    panes::sixel::SixelImageStore,
    panes::{FloatingPanes, TiledPanes},
    panes::{LinkHandler, PaneId, PluginPane, TerminalPane},
    pty::{ClientOrTabIndex, PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::os::unix::io::RawFd;
use std::rc::Rc;
use std::sync::mpsc::channel;
use std::time::Instant;
use std::{
    collections::{HashMap, HashSet},
    str,
};
use zellij_utils::{
    data::{Event, InputMode, ModeInfo, Palette, PaletteColor, Style},
    input::{
        command::TerminalAction,
        layout::{Layout, Run},
        parse_keys,
    },
    pane_size::{Offset, PaneGeom, Size, SizeInPixels, Viewport},
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

pub(crate) struct Tab {
    pub index: usize,
    pub position: usize,
    pub name: String,
    pub prev_name: String,
    tiled_panes: TiledPanes,
    floating_panes: FloatingPanes,
    suppressed_panes: HashMap<PaneId, Box<dyn Pane>>,
    max_panes: Option<usize>,
    viewport: Rc<RefCell<Viewport>>, // includes all non-UI panes
    display_area: Rc<RefCell<Size>>, // includes all panes (including eg. the status bar and tab bar in the default layout)
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    os_api: Box<dyn ServerOsApi>,
    pub senders: ThreadSenders,
    synchronize_is_active: bool,
    should_clear_display_before_rendering: bool,
    mode_info: Rc<RefCell<HashMap<ClientId, ModeInfo>>>,
    default_mode_info: ModeInfo,
    pub style: Style,
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
    draw_pane_frames: bool,
    pending_vte_events: HashMap<RawFd, Vec<VteBytes>>,
    pub selecting_with_mouse: bool, // this is only pub for the tests TODO: remove this once we combine write_text_to_clipboard with render
    link_handler: Rc<RefCell<LinkHandler>>,
    clipboard_provider: ClipboardProvider,
    // TODO: used only to focus the pane when the layout is loaded
    // it seems that optimization is possible using `active_panes`
    focus_pane_id: Option<PaneId>,
    copy_on_select: bool,
    last_mouse_hold_position: Option<Position>,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
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
    fn set_geom_override(&mut self, pane_geom: PaneGeom);
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
    ) -> Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)>; // TODO: better
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
    fn dump_screen(&mut self, _client_id: ClientId) -> String {
        "".to_owned()
    }
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
    fn drain_clipboard_update(&mut self) -> Option<String> {
        None
    }
    fn render_full_viewport(&mut self) {}
    fn relative_position(&self, position_on_screen: &Position) -> Position {
        position_on_screen.relative_to(self.get_content_y(), self.get_content_x())
    }
    fn position_is_on_frame(&self, position: &Position) -> bool {
        if !self.contains(position) {
            return false;
        }
        if (self.x()..self.get_content_x()).contains(&position.column()) {
            // position is on left border
            return true;
        }
        if (self.get_content_x() + self.get_content_columns()..(self.x() + self.cols()))
            .contains(&position.column())
        {
            // position is on right border
            return true;
        }
        if (self.y() as isize..self.get_content_y() as isize).contains(&position.line()) {
            // position is on top border
            return true;
        }
        if ((self.get_content_y() + self.get_content_rows()) as isize
            ..(self.y() + self.rows()) as isize)
            .contains(&position.line())
        {
            // position is on bottom border
            return true;
        }
        false
    }
    fn store_pane_name(&mut self);
    fn load_pane_name(&mut self);
    fn set_borderless(&mut self, borderless: bool);
    fn borderless(&self) -> bool;
    // TODO: this should probably be merged with the mouse_right_click
    fn handle_right_click(&mut self, _to: &Position, _client_id: ClientId) {}
    fn mouse_left_click(&self, _position: &Position, _is_held: bool) -> Option<String> {
        None
    }
    fn mouse_left_click_release(&self, _position: &Position) -> Option<String> {
        None
    }
    fn mouse_right_click(&self, _position: &Position, _is_held: bool) -> Option<String> {
        None
    }
    fn mouse_right_click_release(&self, _position: &Position) -> Option<String> {
        None
    }
    fn mouse_middle_click(&self, _position: &Position, _is_held: bool) -> Option<String> {
        None
    }
    fn mouse_middle_click_release(&self, _position: &Position) -> Option<String> {
        None
    }
    fn mouse_scroll_up(&self, _position: &Position) -> Option<String> {
        None
    }
    fn mouse_scroll_down(&self, _position: &Position) -> Option<String> {
        None
    }
    fn get_line_number(&self) -> Option<usize> {
        None
    }
    fn update_search_term(&mut self, _needle: &str) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn search_down(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn search_up(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn toggle_search_case_sensitivity(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn toggle_search_whole_words(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn toggle_search_wrap(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn clear_search(&mut self) {
        // No-op by default (only terminal-panes currently have search capability)
    }
    fn is_alternate_mode_active(&self) -> bool {
        // False by default (only terminal-panes support alternate mode)
        false
    }
}

impl Tab {
    // FIXME: Still too many arguments for clippy to be happy...
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        index: usize,
        position: usize,
        name: String,
        display_area: Size,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        os_api: Box<dyn ServerOsApi>,
        senders: ThreadSenders,
        max_panes: Option<usize>,
        style: Style,
        default_mode_info: ModeInfo,
        draw_pane_frames: bool,
        connected_clients_in_app: Rc<RefCell<HashSet<ClientId>>>,
        session_is_mirrored: bool,
        client_id: ClientId,
        copy_options: CopyOptions,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
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
        let mode_info = Rc::new(RefCell::new(HashMap::new()));

        let tiled_panes = TiledPanes::new(
            display_area.clone(),
            viewport.clone(),
            connected_clients.clone(),
            connected_clients_in_app.clone(),
            mode_info.clone(),
            character_cell_size.clone(),
            session_is_mirrored,
            draw_pane_frames,
            default_mode_info.clone(),
            style,
            os_api.clone(),
        );
        let floating_panes = FloatingPanes::new(
            display_area.clone(),
            viewport.clone(),
            connected_clients.clone(),
            connected_clients_in_app,
            mode_info.clone(),
            session_is_mirrored,
            default_mode_info.clone(),
            style,
        );

        let clipboard_provider = match copy_options.command {
            Some(command) => ClipboardProvider::Command(CopyCommand::new(command)),
            None => ClipboardProvider::Osc52(copy_options.clipboard),
        };

        Tab {
            index,
            position,
            tiled_panes,
            floating_panes,
            suppressed_panes: HashMap::new(),
            name: name.clone(),
            prev_name: name,
            max_panes,
            viewport,
            display_area,
            character_cell_size,
            sixel_image_store,
            synchronize_is_active: false,
            os_api,
            senders,
            should_clear_display_before_rendering: false,
            style,
            mode_info,
            default_mode_info,
            draw_pane_frames,
            pending_vte_events: HashMap::new(),
            connected_clients,
            selecting_with_mouse: false,
            link_handler: Rc::new(RefCell::new(LinkHandler::new())),
            clipboard_provider,
            focus_pane_id: None,
            copy_on_select: copy_options.copy_on_select,
            last_mouse_hold_position: None,
            terminal_emulator_colors,
            terminal_emulator_color_codes,
        }
    }

    pub fn apply_layout(
        &mut self,
        layout: Layout,
        new_pids: Vec<RawFd>,
        tab_index: usize,
        client_id: ClientId,
    ) {
        if self.tiled_panes.has_panes() {
            log::error!(
                "Applying a layout to a tab with existing panes - this is not yet supported!"
            );
        }
        let (viewport_cols, viewport_rows) = {
            let viewport = self.viewport.borrow();
            (viewport.cols, viewport.rows)
        };
        let mut free_space = PaneGeom::default();
        free_space.cols.set_inner(viewport_cols);
        free_space.rows.set_inner(viewport_rows);

        let positions_in_layout = layout.position_panes_in_space(&free_space);

        let positions_and_size = positions_in_layout.iter();
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
                self.tiled_panes
                    .add_pane_with_existing_geom(PaneId::Plugin(pid), Box::new(new_plugin));
                set_focus_pane_id(layout, PaneId::Plugin(pid));
            } else {
                // there are still panes left to fill, use the pids we received in this method
                let pid = new_pids.next().unwrap(); // if this crashes it means we got less pids than there are panes in this layout
                let next_terminal_position = self.get_next_terminal_position();
                let mut new_pane = TerminalPane::new(
                    *pid,
                    *position_and_size,
                    self.style,
                    next_terminal_position,
                    layout.pane_name.clone().unwrap_or_default(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                );
                new_pane.set_borderless(layout.borderless);
                self.tiled_panes
                    .add_pane_with_existing_geom(PaneId::Terminal(*pid), Box::new(new_pane));
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
        for geom in boundary_geoms {
            self.offset_viewport(&geom)
        }
        self.tiled_panes.set_pane_frames(self.draw_pane_frames);
        self.should_clear_display_before_rendering = true;

        if let Some(pane_id) = focus_pane_id {
            self.focus_pane_id = Some(pane_id);
            self.tiled_panes.focus_pane(pane_id, client_id);
        } else {
            // This is the end of the nasty viewport hack...
            let next_selectable_pane_id = self.tiled_panes.first_selectable_pane_id();
            match next_selectable_pane_id {
                Some(active_pane_id) => {
                    self.tiled_panes.focus_pane(active_pane_id, client_id);
                },
                None => {
                    // this is very likely a configuration error (layout with no selectable panes)
                    self.tiled_panes.clear_active_panes();
                },
            }
        }
    }
    pub fn update_input_modes(&mut self) {
        // this updates all plugins with the client's input mode
        let mode_infos = self.mode_info.borrow();
        for client_id in self.connected_clients.borrow().iter() {
            let mode_info = mode_infos.get(client_id).unwrap_or(&self.default_mode_info);
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
        let other_clients_exist_in_tab = { !self.connected_clients.borrow().is_empty() };
        if other_clients_exist_in_tab {
            if let Some(first_active_floating_pane_id) =
                self.floating_panes.first_active_floating_pane_id()
            {
                self.floating_panes
                    .focus_pane(first_active_floating_pane_id, client_id);
            }
            if let Some(first_active_tiled_pane_id) = self.tiled_panes.first_active_pane_id() {
                self.tiled_panes
                    .focus_pane(first_active_tiled_pane_id, client_id);
            }
            self.connected_clients.borrow_mut().insert(client_id);
            self.mode_info.borrow_mut().insert(
                client_id,
                mode_info.unwrap_or_else(|| self.default_mode_info.clone()),
            );
        } else {
            let mut pane_ids: Vec<PaneId> = self.tiled_panes.pane_ids().copied().collect();
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
            self.connected_clients.borrow_mut().insert(client_id);
            self.mode_info.borrow_mut().insert(
                client_id,
                mode_info.unwrap_or_else(|| self.default_mode_info.clone()),
            );
        }
        self.set_force_render();
        self.update_input_modes();
    }
    pub fn change_mode_info(&mut self, mode_info: ModeInfo, client_id: ClientId) {
        self.mode_info.borrow_mut().insert(client_id, mode_info);
    }
    pub fn add_multiple_clients(&mut self, client_ids_to_mode_infos: Vec<(ClientId, ModeInfo)>) {
        for (client_id, client_mode_info) in client_ids_to_mode_infos {
            self.add_client(client_id, None);
            self.mode_info
                .borrow_mut()
                .insert(client_id, client_mode_info);
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
        // None => all clients
        let mut client_ids_to_mode_infos = vec![];
        let clients_to_drain = clients_to_drain
            .unwrap_or_else(|| self.connected_clients.borrow_mut().drain().collect());
        for client_id in clients_to_drain {
            client_ids_to_mode_infos.push(self.drain_single_client(client_id));
        }
        client_ids_to_mode_infos
    }
    pub fn drain_single_client(&mut self, client_id: ClientId) -> (ClientId, ModeInfo) {
        let client_mode_info = self
            .mode_info
            .borrow_mut()
            .remove(&client_id)
            .unwrap_or_else(|| self.default_mode_info.clone());
        self.connected_clients.borrow_mut().remove(&client_id);
        (client_id, client_mode_info)
    }
    pub fn has_no_connected_clients(&self) -> bool {
        self.connected_clients.borrow().is_empty()
    }
    pub fn toggle_pane_embed_or_floating(&mut self, client_id: ClientId) {
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        if self.floating_panes.panes_are_visible() {
            if let Some(focused_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                if self.tiled_panes.has_room_for_new_pane() {
                    // this unwrap is safe because floating panes should not be visible if there are no floating panes
                    let floating_pane_to_embed =
                        self.close_pane(focused_floating_pane_id, true).unwrap();
                    self.tiled_panes
                        .insert_pane(focused_floating_pane_id, floating_pane_to_embed);
                    self.should_clear_display_before_rendering = true;
                    self.tiled_panes
                        .focus_pane(focused_floating_pane_id, client_id);
                    self.floating_panes.toggle_show_panes(false);
                }
            }
        } else if let Some(focused_pane_id) = self.tiled_panes.focused_pane_id(client_id) {
            if let Some(new_pane_geom) = self.floating_panes.find_room_for_new_pane() {
                if self.get_selectable_tiled_panes().count() <= 1 {
                    // don't close the only pane on screen...
                    return;
                }
                if let Some(mut embedded_pane_to_float) = self.close_pane(focused_pane_id, true) {
                    embedded_pane_to_float.set_geom(new_pane_geom);
                    resize_pty!(embedded_pane_to_float, self.os_api);
                    embedded_pane_to_float.set_active_at(Instant::now());
                    self.floating_panes
                        .add_pane(focused_pane_id, embedded_pane_to_float);
                    self.floating_panes.focus_pane(focused_pane_id, client_id);
                    self.floating_panes.toggle_show_panes(true);
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
                },
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
                },
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
                        self.style,
                        next_terminal_position,
                        String::new(),
                        self.link_handler.clone(),
                        self.character_cell_size.clone(),
                        self.sixel_image_store.clone(),
                        self.terminal_emulator_colors.clone(),
                        self.terminal_emulator_color_codes.clone(),
                    );
                    new_pane.set_content_offset(Offset::frame(1)); // floating panes always have a frame
                    resize_pty!(new_pane, self.os_api);
                    self.floating_panes.add_pane(pid, Box::new(new_pane));
                    self.floating_panes.focus_pane_for_all_clients(pid);
                }
            }
        } else {
            if self.tiled_panes.fullscreen_is_active() {
                self.tiled_panes.unset_fullscreen();
            }
            if self.tiled_panes.has_room_for_new_pane() {
                if let PaneId::Terminal(term_pid) = pid {
                    let next_terminal_position = self.get_next_terminal_position();
                    let new_terminal = TerminalPane::new(
                        term_pid,
                        PaneGeom::default(), // the initial size will be set later
                        self.style,
                        next_terminal_position,
                        String::new(),
                        self.link_handler.clone(),
                        self.character_cell_size.clone(),
                        self.sixel_image_store.clone(),
                        self.terminal_emulator_colors.clone(),
                        self.terminal_emulator_color_codes.clone(),
                    );
                    self.tiled_panes.insert_pane(pid, Box::new(new_terminal));
                    self.should_clear_display_before_rendering = true;
                    if let Some(client_id) = client_id {
                        self.tiled_panes.focus_pane(pid, client_id);
                    }
                }
            }
        }
    }
    pub fn suppress_active_pane(&mut self, pid: PaneId, client_id: ClientId) {
        // this method creates a new pane from pid and replaces it with the active pane
        // the active pane is then suppressed (hidden and not rendered) until the current
        // created pane is closed, in which case it will be replaced back by it
        match pid {
            PaneId::Terminal(pid) => {
                let next_terminal_position = self.get_next_terminal_position(); // TODO: this is not accurate in this case
                let new_pane = TerminalPane::new(
                    pid,
                    PaneGeom::default(), // the initial size will be set later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                );
                let replaced_pane = if self.floating_panes.panes_are_visible() {
                    self.floating_panes
                        .replace_active_pane(Box::new(new_pane), client_id)
                } else {
                    self.tiled_panes
                        .replace_active_pane(Box::new(new_pane), client_id)
                };
                match replaced_pane {
                    Some(replaced_pane) => {
                        self.suppressed_panes
                            .insert(PaneId::Terminal(pid), replaced_pane);
                        let current_active_pane = self.get_active_pane(client_id).unwrap(); // this will be the newly replaced pane we just created
                        resize_pty!(current_active_pane, self.os_api);
                    },
                    None => {
                        log::error!("Could not find editor pane to replace - is no pane focused?")
                    },
                }
            },
            PaneId::Plugin(_pid) => {
                // TBD, currently unsupported
            },
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
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                );
                self.tiled_panes
                    .split_pane_horizontally(pid, Box::new(new_terminal), client_id);
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
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                );
                self.tiled_panes
                    .split_pane_vertically(pid, Box::new(new_terminal), client_id);
                self.should_clear_display_before_rendering = true;
                self.tiled_panes.focus_pane(pid, client_id);
            }
        }
    }
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
            || self
                .suppressed_panes
                .values()
                .any(|s_p| s_p.pid() == PaneId::Terminal(pid))
    }
    pub fn handle_pty_bytes(&mut self, pid: RawFd, bytes: VteBytes) {
        if let Some(terminal_output) = self
            .tiled_panes
            .get_pane_mut(PaneId::Terminal(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Terminal(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.pid() == PaneId::Terminal(pid))
            })
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
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.pid() == PaneId::Terminal(pid))
            })
        {
            terminal_output.handle_pty_bytes(bytes);
            let messages_to_pty = terminal_output.drain_messages_to_pty();
            let clipboard_update = terminal_output.drain_clipboard_update();
            for message in messages_to_pty {
                self.write_to_pane_id(message, PaneId::Terminal(pid));
            }
            if let Some(string) = clipboard_update {
                self.write_selection_to_clipboard(&string);
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
        self.clear_search(client_id); // this is an inexpensive operation if empty, if we need more such cleanups we should consider moving this and the rest to some sort of cleanup method
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
                    .or_else(|| self.tiled_panes.get_pane(pane_id))
                    .or_else(|| self.suppressed_panes.get(&pane_id))
                    .unwrap();
                let adjusted_input = active_terminal.adjust_input_to_terminal(input_bytes);

                self.senders
                    .send_to_pty_writer(PtyWriteInstruction::Write(
                        adjusted_input,
                        active_terminal_id,
                    ))
                    .unwrap();
            },
            PaneId::Plugin(pid) => {
                for key in parse_keys(&input_bytes) {
                    self.senders
                        .send_to_plugin(PluginInstruction::Update(Some(pid), None, Event::Key(key)))
                        .unwrap()
                }
            },
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
        if self.floating_panes.panes_are_visible() {
            return;
        }
        self.tiled_panes.toggle_active_pane_fullscreen(client_id);
    }
    pub fn is_fullscreen_active(&self) -> bool {
        self.tiled_panes.fullscreen_is_active()
    }
    pub fn are_floating_panes_visible(&self) -> bool {
        self.floating_panes.panes_are_visible()
    }
    pub fn switch_next_pane_fullscreen(&mut self, client_id: ClientId) {
        if !self.is_fullscreen_active() {
            return;
        }
        self.tiled_panes.switch_next_pane_fullscreen(client_id);
    }
    pub fn switch_prev_pane_fullscreen(&mut self, client_id: ClientId) {
        if !self.is_fullscreen_active() {
            return;
        }
        self.tiled_panes.switch_next_pane_fullscreen(client_id);
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
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
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
        let connected_clients: HashSet<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        if connected_clients.is_empty() || !self.tiled_panes.has_active_panes() {
            return;
        }
        self.update_active_panes_in_pty_thread();

        let floating_panes_stack = self.floating_panes.stack();
        output.add_clients(
            &connected_clients,
            self.link_handler.clone(),
            floating_panes_stack,
        );

        self.hide_cursor_and_clear_display_as_needed(output);
        self.tiled_panes
            .render(output, self.floating_panes.panes_are_visible());
        if self.floating_panes.panes_are_visible() && self.floating_panes.has_active_panes() {
            self.floating_panes.render(output);
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
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
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
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        for client_id in connected_clients {
            match self.get_active_terminal_cursor_position(client_id) {
                Some((cursor_position_x, cursor_position_y)) => {
                    let show_cursor = "\u{1b}[?25h";
                    let change_cursor_shape = self
                        .get_active_pane(client_id)
                        .map(|ap| ap.cursor_shape_csi())
                        .unwrap_or_default();
                    let goto_cursor_position = &format!(
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        cursor_position_y + 1,
                        cursor_position_x + 1,
                        change_cursor_shape
                    ); // goto row/col
                    output.add_post_vte_instruction_to_client(client_id, show_cursor);
                    output.add_post_vte_instruction_to_client(client_id, goto_cursor_position);
                },
                None => {
                    let hide_cursor = "\u{1b}[?25l";
                    output.add_post_vte_instruction_to_client(client_id, hide_cursor);
                },
            }
        }
    }
    fn get_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.tiled_panes.get_panes()
    }
    fn get_selectable_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.get_tiled_panes().filter(|(_, p)| p.selectable())
    }
    fn get_next_terminal_position(&self) -> usize {
        let tiled_panes_count = self
            .tiled_panes
            .get_panes()
            .filter(|(k, _)| match k {
                PaneId::Plugin(_) => false,
                PaneId::Terminal(_) => true,
            })
            .count();
        let floating_panes_count = self
            .floating_panes
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
        let selectable_floating_panes = self
            .floating_panes
            .get_panes()
            .filter(|(_, p)| p.selectable());
        selectable_tiled_panes.count() > 0 || selectable_floating_panes.count() > 0
    }
    pub fn has_selectable_tiled_panes(&self) -> bool {
        let selectable_tiled_panes = self.tiled_panes.get_panes().filter(|(_, p)| p.selectable());
        selectable_tiled_panes.count() > 0
    }
    pub fn resize_whole_tab(&mut self, new_screen_size: Size) {
        self.floating_panes.resize(new_screen_size);
        self.floating_panes.resize_pty_all_panes(&mut self.os_api); // we need to do this explicitly because floating_panes.resize does not do this
        self.tiled_panes.resize(new_screen_size);
        self.should_clear_display_before_rendering = true;
    }
    pub fn resize_left(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_left(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_left(client_id);
        }
    }
    pub fn resize_right(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_right(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_right(client_id);
        }
    }
    pub fn resize_down(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_down(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_down(client_id);
        }
    }
    pub fn resize_up(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_up(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_up(client_id);
        }
    }
    pub fn resize_increase(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_increase(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_increase(client_id);
        }
    }
    pub fn resize_decrease(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane_decrease(client_id, &mut self.os_api);
            if successfully_resized {
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            self.tiled_panes.resize_active_pane_decrease(client_id);
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
            self.switch_next_pane_fullscreen(client_id);
            return;
        }
        self.tiled_panes.focus_next_pane(client_id);
    }
    pub fn focus_previous_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            self.switch_prev_pane_fullscreen(client_id);
            return;
        }
        self.tiled_panes.focus_previous_pane(client_id);
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_left(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_focus_left(
                client_id,
                &self.connected_clients.borrow().iter().copied().collect(),
            )
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                self.switch_next_pane_fullscreen(client_id);
                return true;
            }
            self.tiled_panes.move_focus_left(client_id)
        }
    }
    pub fn move_focus_down(&mut self, client_id: ClientId) -> bool {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_focus_down(
                client_id,
                &self.connected_clients.borrow().iter().copied().collect(),
            )
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
            self.floating_panes.move_focus_up(
                client_id,
                &self.connected_clients.borrow().iter().copied().collect(),
            )
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
            self.floating_panes.move_focus_right(
                client_id,
                &self.connected_clients.borrow().iter().copied().collect(),
            )
        } else {
            if !self.has_selectable_panes() {
                return false;
            }
            if self.tiled_panes.fullscreen_is_active() {
                self.switch_next_pane_fullscreen(client_id);
                return true;
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
        self.tiled_panes.move_active_pane(client_id);
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
            self.tiled_panes.move_active_pane_down(client_id);
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
            self.tiled_panes.move_active_pane_up(client_id);
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
            self.tiled_panes.move_active_pane_right(client_id);
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
            self.tiled_panes.move_active_pane_left(client_id);
        }
    }
    fn close_down_to_max_terminals(&mut self) {
        if let Some(max_panes) = self.max_panes {
            let terminals = self.get_tiled_pane_ids();
            for &pid in terminals.iter().skip(max_panes - 1) {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid))
                    .unwrap();
                self.close_pane(pid, false);
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
    pub fn close_pane(
        &mut self,
        id: PaneId,
        ignore_suppressed_panes: bool,
    ) -> Option<Box<dyn Pane>> {
        // we need to ignore suppressed panes when we toggle a pane to be floating/embedded(tiled)
        // this is because in that case, while we do use this logic, we're not actually closing the
        // pane, we're moving it
        //
        // TODO: separate the "close_pane" logic and the "move_pane_somewhere_else" logic, they're
        // overloaded here and that's not great
        if !ignore_suppressed_panes && self.suppressed_panes.contains_key(&id) {
            return self.replace_pane_with_suppressed_pane(id);
        }
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
                self.tiled_panes.unset_fullscreen();
            }
            let closed_pane = self.tiled_panes.remove_pane(id);
            self.set_force_render();
            self.tiled_panes.set_force_render();
            closed_pane
        }
    }
    pub fn replace_pane_with_suppressed_pane(&mut self, pane_id: PaneId) -> Option<Box<dyn Pane>> {
        self.suppressed_panes
            .remove(&pane_id)
            .and_then(|suppressed_pane| {
                let suppressed_pane_id = suppressed_pane.pid();
                let replaced_pane = if self.are_floating_panes_visible() {
                    self.floating_panes.replace_pane(pane_id, suppressed_pane)
                } else {
                    self.tiled_panes.replace_pane(pane_id, suppressed_pane)
                };
                if let Some(suppressed_pane) = self
                    .floating_panes
                    .get_pane(suppressed_pane_id)
                    .or_else(|| self.tiled_panes.get_pane(suppressed_pane_id))
                {
                    // You may be thinking: why aren't we using the original "suppressed_pane" here,
                    // isn't it the same one?
                    //
                    // Yes, you are right! However, we moved it into its correct environment above
                    // (either floating_panes or tiled_panes) where it received a new geometry based on
                    // the pane there we replaced. Now, we need to update its pty about its new size.
                    // We couldn't do that before, and we can't use the original moved item now - so we
                    // need to refetch it
                    resize_pty!(suppressed_pane, self.os_api);
                }
                replaced_pane
            })
    }
    pub fn close_focused_pane(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            if let Some(active_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                self.close_pane(active_floating_pane_id, false);
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(active_floating_pane_id))
                    .unwrap();
                return;
            }
        }
        if let Some(active_pane_id) = self.tiled_panes.get_active_pane_id(client_id) {
            self.close_pane(active_pane_id, false);
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(active_pane_id))
                .unwrap();
        }
    }
    pub fn dump_active_terminal_screen(&mut self, file: Option<String>, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let dump = active_pane.dump_screen(client_id);
            self.os_api.write_to_file(dump, file);
        }
    }
    pub fn edit_scrollback(&mut self, client_id: ClientId) {
        let mut file = temp_dir();
        file.push(format!("{}.dump", Uuid::new_v4()));
        self.dump_active_terminal_screen(Some(String::from(file.to_string_lossy())), client_id);
        let line_number = self
            .get_active_pane(client_id)
            .and_then(|a_t| a_t.get_line_number());
        self.senders
            .send_to_pty(PtyInstruction::OpenInPlaceEditor(
                file,
                line_number,
                client_id,
            ))
            .unwrap();
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
    pub fn handle_scrollwheel_up(&mut self, point: &Position, lines: usize, client_id: ClientId) {
        if let Some(pane) = self.get_pane_at(point, false) {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_up(&relative_position) {
                self.write_to_terminal_at(mouse_event.into_bytes(), point);
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send UP n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    self.write_to_terminal_at("\u{1b}[A".as_bytes().to_owned(), point)
                }
            } else {
                pane.scroll_up(lines, client_id);
            }
        }
    }
    pub fn handle_scrollwheel_down(&mut self, point: &Position, lines: usize, client_id: ClientId) {
        if let Some(pane) = self.get_pane_at(point, false) {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_down(&relative_position) {
                self.write_to_terminal_at(mouse_event.into_bytes(), point);
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send DOWN n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    self.write_to_terminal_at("\u{1b}[B".as_bytes().to_owned(), point)
                }
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
            let first_client_id = {
                self.connected_clients
                    .borrow()
                    .iter()
                    .copied()
                    .next()
                    .unwrap()
            }; // TODO: instead of doing this, record the pane that is in fullscreen
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
            if let Some(mouse_event) = pane.mouse_left_click(&relative_position, false) {
                if !pane.position_is_on_frame(position) {
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                }
            } else {
                pane.start_selection(&relative_position, client_id);
                if let PaneId::Terminal(_) = pane.pid() {
                    self.selecting_with_mouse = true;
                }
            }
        };
    }
    pub fn handle_right_click(&mut self, position: &Position, client_id: ClientId) {
        self.focus_pane_at(position, client_id);

        if let Some(pane) = self.get_pane_at(position, false) {
            let relative_position = pane.relative_position(position);
            if let Some(mouse_event) = pane.mouse_right_click(&relative_position, false) {
                if !pane.position_is_on_frame(position) {
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                }
            } else {
                pane.handle_right_click(&relative_position, client_id);
            }
        };
    }
    pub fn handle_middle_click(&mut self, position: &Position, client_id: ClientId) {
        self.focus_pane_at(position, client_id);

        if let Some(pane) = self.get_pane_at(position, false) {
            let relative_position = pane.relative_position(position);
            if let Some(mouse_event) = pane.mouse_middle_click(&relative_position, false) {
                if !pane.position_is_on_frame(position) {
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                }
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
    pub fn handle_right_mouse_release(&mut self, position: &Position, client_id: ClientId) {
        self.last_mouse_hold_position = None;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);
        if let Some(active_pane) = active_pane {
            let mut relative_position = active_pane.relative_position(position);
            relative_position.change_column(
                (relative_position.column())
                    .max(0)
                    .min(active_pane.get_content_columns()),
            );

            relative_position.change_line(
                (relative_position.line())
                    .max(0)
                    .min(active_pane.get_content_rows() as isize),
            );

            if let Some(mouse_event) = active_pane.mouse_right_click_release(&relative_position) {
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            }
        }
    }
    pub fn handle_middle_mouse_release(&mut self, position: &Position, client_id: ClientId) {
        self.last_mouse_hold_position = None;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);
        if let Some(active_pane) = active_pane {
            let mut relative_position = active_pane.relative_position(position);
            relative_position.change_column(
                (relative_position.column())
                    .max(0)
                    .min(active_pane.get_content_columns()),
            );

            relative_position.change_line(
                (relative_position.line())
                    .max(0)
                    .min(active_pane.get_content_rows() as isize),
            );

            if let Some(mouse_event) = active_pane.mouse_middle_click_release(&relative_position) {
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            }
        }
    }
    pub fn handle_left_mouse_release(&mut self, position: &Position, client_id: ClientId) {
        self.last_mouse_hold_position = None;

        if self.floating_panes.panes_are_visible()
            && self.floating_panes.pane_is_being_moved_with_mouse()
        {
            self.floating_panes.stop_moving_pane_with_mouse(*position);
            return;
        }

        // read these here to avoid use of borrowed `*self`, since we are holding active_pane
        let selecting = self.selecting_with_mouse;
        let copy_on_release = self.copy_on_select;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            let mut relative_position = active_pane.relative_position(position);
            relative_position.change_column(
                (relative_position.column())
                    .max(0)
                    .min(active_pane.get_content_columns()),
            );

            relative_position.change_line(
                (relative_position.line())
                    .max(0)
                    .min(active_pane.get_content_rows() as isize),
            );

            if let Some(mouse_event) = active_pane.mouse_left_click_release(&relative_position) {
                self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
            } else {
                let relative_position = active_pane.relative_position(position);
                if let PaneId::Terminal(_) = active_pane.pid() {
                    if selecting && copy_on_release {
                        active_pane.end_selection(&relative_position, client_id);
                        let selected_text = active_pane.get_selected_text();
                        active_pane.reset_selection();

                        if let Some(selected_text) = selected_text {
                            self.write_selection_to_clipboard(&selected_text);
                        }
                    }
                } else {
                    active_pane.end_selection(&relative_position, client_id);
                }

                self.selecting_with_mouse = false;
            }
        }
    }
    pub fn handle_mouse_hold_left(
        &mut self,
        position_on_screen: &Position,
        client_id: ClientId,
    ) -> bool {
        // return value indicates whether we should trigger a render
        // determine if event is repeated to enable smooth scrolling
        let is_repeated = if let Some(last_position) = self.last_mouse_hold_position {
            position_on_screen == &last_position
        } else {
            false
        };
        self.last_mouse_hold_position = Some(*position_on_screen);

        let search_selectable = true;

        if self.floating_panes.panes_are_visible()
            && self.floating_panes.pane_is_being_moved_with_mouse()
            && self
                .floating_panes
                .move_pane_with_mouse(*position_on_screen, search_selectable)
        {
            self.set_force_render();
            return !is_repeated; // we don't need to re-render in this case if the pane did not move
                                 // return;
        }

        let selecting = self.selecting_with_mouse;
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            let mut relative_position = active_pane.relative_position(position_on_screen);
            if !is_repeated {
                // ensure that coordinates are valid
                relative_position.change_column(
                    (relative_position.column())
                        .max(0)
                        .min(active_pane.get_content_columns()),
                );

                relative_position.change_line(
                    (relative_position.line())
                        .max(0)
                        .min(active_pane.get_content_rows() as isize),
                );
                if let Some(mouse_event) = active_pane.mouse_left_click(&relative_position, true) {
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                    return true; // we need to re-render in this case so the selection disappears
                }
            } else if selecting {
                active_pane.update_selection(&relative_position, client_id);
                return true; // we need to re-render in this case so the selection is updated
            }
        }
        false // we shouldn't even get here, but might as well not needlessly render if we do
    }
    pub fn handle_mouse_hold_right(
        &mut self,
        position_on_screen: &Position,
        client_id: ClientId,
    ) -> bool {
        // return value indicates whether we should trigger a render
        // determine if event is repeated to enable smooth scrolling
        let is_repeated = if let Some(last_position) = self.last_mouse_hold_position {
            position_on_screen == &last_position
        } else {
            false
        };
        self.last_mouse_hold_position = Some(*position_on_screen);

        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            let mut relative_position = active_pane.relative_position(position_on_screen);
            if !is_repeated {
                relative_position.change_column(
                    (relative_position.column())
                        .max(0)
                        .min(active_pane.get_content_columns()),
                );

                relative_position.change_line(
                    (relative_position.line())
                        .max(0)
                        .min(active_pane.get_content_rows() as isize),
                );
                if let Some(mouse_event) = active_pane.mouse_right_click(&relative_position, true) {
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                    return true; // we need to re-render in this case so the selection disappears
                }
            }
        }
        false // we shouldn't even get here, but might as well not needlessly render if we do
    }
    pub fn handle_mouse_hold_middle(
        &mut self,
        position_on_screen: &Position,
        client_id: ClientId,
    ) -> bool {
        println!("mouse hold middle");
        // return value indicates whether we should trigger a render
        // determine if event is repeated to enable smooth scrolling
        let is_repeated = if let Some(last_position) = self.last_mouse_hold_position {
            position_on_screen == &last_position
        } else {
            false
        };
        println!("is repeated: {:?}", is_repeated);
        self.last_mouse_hold_position = Some(*position_on_screen);

        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);

        if let Some(active_pane) = active_pane {
            println!("can have active pane");
            let mut relative_position = active_pane.relative_position(position_on_screen);
            if !is_repeated {
                relative_position.change_column(
                    (relative_position.column())
                        .max(0)
                        .min(active_pane.get_content_columns()),
                );

                relative_position.change_line(
                    (relative_position.line())
                        .max(0)
                        .min(active_pane.get_content_rows() as isize),
                );
                if let Some(mouse_event) = active_pane.mouse_middle_click(&relative_position, true)
                {
                    log::info!("can have mouse event: {:?}", mouse_event);
                    self.write_to_active_terminal(mouse_event.into_bytes(), client_id);
                    return true; // we need to re-render in this case so the selection disappears
                }
            }
        }
        false // we shouldn't even get here, but might as well not needlessly render if we do
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
        let connected_clients: HashSet<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
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
                },
                Err(err) => {
                    log::error!("could not write selection to clipboard: {}", err);
                    Event::SystemClipboardFailure
                },
            };
        self.senders
            .send_to_plugin(PluginInstruction::Update(None, None, clipboard_event))
            .unwrap();
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
            let active_terminal = if self.are_floating_panes_visible() {
                self.floating_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            } else {
                self.tiled_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            }
            .unwrap();

            // It only allows printable unicode, delete and backspace keys.
            let is_updatable = buf.iter().all(|u| matches!(u, 0x20..=0x7E | 0x08 | 0x7F));
            if is_updatable {
                let s = str::from_utf8(&buf).unwrap();
                active_terminal.update_name(s);
            }
        }
    }

    pub fn undo_active_rename_pane(&mut self, client_id: ClientId) {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = if self.are_floating_panes_visible() {
                self.floating_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            } else {
                self.tiled_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            }
            .unwrap();

            active_terminal.load_pane_name();
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
        self.tiled_panes.set_pane_frames(should_set_pane_frames);
        self.should_clear_display_before_rendering = true;
        self.set_force_render();
    }
    pub fn panes_to_hide_count(&self) -> usize {
        self.tiled_panes.panes_to_hide_count()
    }

    pub fn update_search_term(&mut self, buf: Vec<u8>, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // It only allows printable unicode, delete and backspace keys.
            let is_updatable = buf.iter().all(|u| matches!(u, 0x20..=0x7E | 0x08 | 0x7F));
            if is_updatable {
                let s = str::from_utf8(&buf).unwrap();
                active_pane.update_search_term(s);
            }
        }
    }

    pub fn search_down(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.search_down();
        }
    }

    pub fn search_up(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.search_up();
        }
    }

    pub fn toggle_search_case_sensitivity(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.toggle_search_case_sensitivity();
        }
    }

    pub fn toggle_search_wrap(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.toggle_search_wrap();
        }
    }

    pub fn toggle_search_whole_words(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.toggle_search_whole_words();
        }
    }

    pub fn clear_search(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_search();
        }
    }
}

#[cfg(test)]
#[path = "./unit/tab_tests.rs"]
mod tab_tests;

#[cfg(test)]
#[path = "./unit/tab_integration_tests.rs"]
mod tab_integration_tests;
