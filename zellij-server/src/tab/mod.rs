//! `Tab`s holds multiple panes. It tracks their coordinates (x/y) and size,
//! as well as how they should be resized

mod clipboard;
mod copy_command;
mod layout_applier;
mod swap_layouts;

use copy_command::CopyCommand;
use serde;
use std::env::temp_dir;
use std::net::IpAddr;
use std::path::PathBuf;
use uuid::Uuid;
use zellij_utils::data::{
    Direction, KeyWithModifier, PaneInfo, PermissionStatus, PermissionType, PluginPermission,
    ResizeStrategy, WebSharing,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::RunCommand;
use zellij_utils::input::mouse::{MouseEvent, MouseEventType};
use zellij_utils::position::Position;
use zellij_utils::position::{Column, Line};
use zellij_utils::shared::clean_string_from_control_and_linebreak;

use crate::background_jobs::BackgroundJob;
use crate::pane_groups::PaneGroups;
use crate::pty_writer::PtyWriteInstruction;
use crate::screen::CopyOptions;
use crate::ui::{loading_indication::LoadingIndication, pane_boundaries_frame::FrameParams};
use layout_applier::LayoutApplier;
use swap_layouts::SwapLayouts;

use self::clipboard::ClipboardProvider;
use crate::{
    os_input_output::ServerOsApi,
    output::{CharacterChunk, Output, SixelImageChunk},
    panes::floating_panes::floating_pane_grid::half_size_middle_geom,
    panes::sixel::SixelImageStore,
    panes::{FloatingPanes, TiledPanes},
    panes::{LinkHandler, PaneId, PluginPane, TerminalPane},
    plugins::PluginInstruction,
    pty::{ClientTabIndexOrPaneId, NewPanePlacement, PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ClientId, ServerInstruction,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str,
};
use zellij_utils::{
    data::{
        Event, FloatingPaneCoordinates, InputMode, ModeInfo, Palette, PaletteColor, Style, Styling,
    },
    input::{
        command::TerminalAction,
        layout::{
            FloatingPaneLayout, Run, RunPluginOrAlias, SwapFloatingLayout, SwapTiledLayout,
            TiledPaneLayout,
        },
        parse_keys,
    },
    pane_size::{Offset, PaneGeom, Size, SizeInPixels, Viewport},
};

#[macro_export]
macro_rules! resize_pty {
    ($pane:expr, $os_input:expr, $senders:expr) => {{
        match $pane.pid() {
            PaneId::Terminal(ref pid) => {
                $senders
                    .send_to_pty_writer(PtyWriteInstruction::ResizePty(
                        *pid,
                        $pane.get_content_columns() as u16,
                        $pane.get_content_rows() as u16,
                        None,
                        None,
                    ))
                    .with_context(err_context);
            },
            PaneId::Plugin(ref pid) => {
                let err_context = || format!("failed to resize plugin {pid}");
                $senders
                    .send_to_plugin(PluginInstruction::Resize(
                        *pid,
                        $pane.get_content_columns(),
                        $pane.get_content_rows(),
                    ))
                    .with_context(err_context)
            },
        }
    }};
    ($pane:expr, $os_input:expr, $senders:expr, $character_cell_size:expr) => {{
        let (width_in_pixels, height_in_pixels) = {
            let character_cell_size = $character_cell_size.borrow();
            match *character_cell_size {
                Some(size_in_pixels) => {
                    let width_in_pixels =
                        (size_in_pixels.width * $pane.get_content_columns()) as u16;
                    let height_in_pixels =
                        (size_in_pixels.height * $pane.get_content_rows()) as u16;
                    (Some(width_in_pixels), Some(height_in_pixels))
                },
                None => (None, None),
            }
        };
        match $pane.pid() {
            PaneId::Terminal(ref pid) => {
                use crate::PtyWriteInstruction;
                let err_context = || format!("Failed to send resize pty instruction");
                $senders
                    .send_to_pty_writer(PtyWriteInstruction::ResizePty(
                        *pid,
                        $pane.get_content_columns() as u16,
                        $pane.get_content_rows() as u16,
                        width_in_pixels,
                        height_in_pixels,
                    ))
                    .with_context(err_context)
            },
            PaneId::Plugin(ref pid) => {
                let err_context = || format!("failed to resize plugin {pid}");
                $senders
                    .send_to_plugin(PluginInstruction::Resize(
                        *pid,
                        $pane.get_content_columns(),
                        $pane.get_content_rows(),
                    ))
                    .with_context(err_context)
            },
        }
    }};
}

// FIXME: This should be replaced by `RESIZE_PERCENT` at some point
pub const MIN_TERMINAL_HEIGHT: usize = 5;
pub const MIN_TERMINAL_WIDTH: usize = 5;

const MAX_PENDING_VTE_EVENTS: usize = 7000;

type HoldForCommand = Option<RunCommand>;
pub type SuppressedPanes = HashMap<PaneId, (bool, Box<dyn Pane>)>; // bool => is scrollback editor

enum BufferedTabInstruction {
    SetPaneSelectable(PaneId, bool),
    HandlePtyBytes(u32, VteBytes),
    HoldPane(PaneId, Option<i32>, bool, RunCommand), // Option<i32> is the exit status, bool is is_first_run
}

#[derive(Debug, Default, Copy, Clone)]
pub struct MouseEffect {
    pub state_changed: bool,
    pub leave_clipboard_message: bool,
    pub group_toggle: Option<PaneId>,
    pub group_add: Option<PaneId>,
    pub ungroup: bool,
}

impl MouseEffect {
    pub fn state_changed() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn leave_clipboard_message() -> Self {
        MouseEffect {
            state_changed: false,
            leave_clipboard_message: true,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn state_changed_and_leave_clipboard_message() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: true,
            group_toggle: None,
            group_add: None,
            ungroup: false,
        }
    }
    pub fn group_toggle(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: Some(pane_id),
            group_add: None,
            ungroup: false,
        }
    }
    pub fn group_add(pane_id: PaneId) -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: Some(pane_id),
            ungroup: false,
        }
    }
    pub fn ungroup() -> Self {
        MouseEffect {
            state_changed: true,
            leave_clipboard_message: false,
            group_toggle: None,
            group_add: None,
            ungroup: true,
        }
    }
}

pub(crate) struct Tab {
    pub index: usize,
    pub position: usize,
    pub name: String,
    pub prev_name: String,
    tiled_panes: TiledPanes,
    floating_panes: FloatingPanes,
    suppressed_panes: SuppressedPanes,
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
    auto_layout: bool,
    pending_vte_events: HashMap<u32, Vec<VteBytes>>,
    pub selecting_with_mouse_in_pane: Option<PaneId>, // this is only pub for the tests
    link_handler: Rc<RefCell<LinkHandler>>,
    clipboard_provider: ClipboardProvider,
    // TODO: used only to focus the pane when the layout is loaded
    // it seems that optimization is possible using `active_panes`
    focus_pane_id: Option<PaneId>,
    copy_on_select: bool,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    pids_waiting_resize: HashSet<u32>, // u32 is the terminal_id
    cursor_positions_and_shape: HashMap<ClientId, (usize, usize, String)>, // (x_position,
    // y_position,
    // cursor_shape_csi)
    is_pending: bool, // a pending tab is one that is still being loaded or otherwise waiting
    pending_instructions: Vec<BufferedTabInstruction>, // instructions that came while the tab was
    // pending and need to be re-applied
    swap_layouts: SwapLayouts,
    default_shell: PathBuf,
    default_editor: Option<PathBuf>,
    debug: bool,
    arrow_fonts: bool,
    styled_underlines: bool,
    explicitly_disable_kitty_keyboard_protocol: bool,
    web_clients_allowed: bool,
    web_sharing: WebSharing,
    mouse_hover_pane_id: HashMap<ClientId, PaneId>,
    current_pane_group: Rc<RefCell<PaneGroups>>,
    advanced_mouse_actions: bool,
    currently_marking_pane_group: Rc<RefCell<HashMap<ClientId, bool>>>,
    connected_clients_in_app: Rc<RefCell<HashMap<ClientId, bool>>>, // bool -> is_web_client
    // the below are the configured values - the ones that will be set if and when the web server
    // is brought online
    web_server_ip: IpAddr,
    web_server_port: u16,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(crate = "self::serde")]
pub(crate) struct TabData {
    pub position: usize,
    pub name: String,
    pub active: bool,
    pub mode_info: ModeInfo,
    pub colors: Styling,
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
    fn handle_pty_bytes(&mut self, _bytes: VteBytes) {}
    fn handle_plugin_bytes(&mut self, _client_id: ClientId, _bytes: VteBytes) {}
    fn cursor_coordinates(&self) -> Option<(usize, usize)>;
    fn is_mid_frame(&self) -> bool {
        false
    }
    fn adjust_input_to_terminal(
        &mut self,
        _key_with_modifier: &Option<KeyWithModifier>,
        _raw_input_bytes: Vec<u8>,
        _raw_input_bytes_are_kitty: bool,
        _client_id: Option<ClientId>,
    ) -> Option<AdjustedInput> {
        None
    }
    fn position_and_size(&self) -> PaneGeom;
    fn current_geom(&self) -> PaneGeom;
    fn geom_override(&self) -> Option<PaneGeom>;
    fn should_render(&self) -> bool;
    fn set_should_render(&mut self, should_render: bool);
    fn set_should_render_boundaries(&mut self, _should_render: bool) {}
    fn selectable(&self) -> bool;
    fn set_selectable(&mut self, selectable: bool);
    fn request_permissions_from_user(&mut self, _permissions: Option<PluginPermission>) {}
    fn render(
        &mut self,
        client_id: Option<ClientId>,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)>>; // TODO: better
    fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>)>>; // TODO: better
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
    fn clear_screen(&mut self);
    fn dump_screen(&self, _full: bool, _client_id: Option<ClientId>) -> String {
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
    fn get_content_offset(&self) -> Offset {
        Offset::default()
    }
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
    fn reset_selection(&mut self, _client_id: Option<ClientId>) {}
    fn supports_mouse_selection(&self) -> bool {
        true
    }
    fn get_selected_text(&self, _client_id: ClientId) -> Option<String> {
        None
    }

    fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.cols()
    }
    fn right_boundary_x_content_coords(&self) -> usize {
        self.get_content_x() + self.get_content_columns()
    }
    fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    fn bottom_boundary_y_content_coords(&self) -> usize {
        self.get_content_y() + self.get_content_rows()
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
    // TODO: get rid of this in favor of intercept_mouse_event_on_frame
    fn intercept_left_mouse_click(&mut self, _position: &Position, _client_id: ClientId) -> bool {
        let intercepted = false;
        intercepted
    }
    fn intercept_mouse_event_on_frame(
        &mut self,
        _event: &MouseEvent,
        _client_id: ClientId,
    ) -> bool {
        let intercepted = false;
        intercepted
    }
    fn store_pane_name(&mut self);
    fn load_pane_name(&mut self);
    fn set_borderless(&mut self, borderless: bool);
    fn borderless(&self) -> bool;
    fn set_exclude_from_sync(&mut self, exclude_from_sync: bool);
    fn exclude_from_sync(&self) -> bool;

    // TODO: this should probably be merged with the mouse_right_click
    fn handle_right_click(&mut self, _to: &Position, _client_id: ClientId) {}
    fn mouse_event(&self, _event: &MouseEvent, _client_id: ClientId) -> Option<String> {
        None
    }
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
    fn focus_event(&self) -> Option<String> {
        None
    }
    fn unfocus_event(&self) -> Option<String> {
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
    fn hold(&mut self, _exit_status: Option<i32>, _is_first_run: bool, _run_command: RunCommand) {
        // No-op by default, only terminal panes support holding
    }
    fn add_red_pane_frame_color_override(&mut self, _error_text: Option<String>);
    fn add_highlight_pane_frame_color_override(
        &mut self,
        _text: Option<String>,
        _client_id: Option<ClientId>,
    ) {
    }
    fn clear_pane_frame_color_override(&mut self, _client_id: Option<ClientId>);
    fn frame_color_override(&self) -> Option<PaletteColor>;
    fn invoked_with(&self) -> &Option<Run>;
    fn set_title(&mut self, title: String);
    fn update_loading_indication(&mut self, _loading_indication: LoadingIndication) {} // only relevant for plugins
    fn start_loading_indication(&mut self, _loading_indication: LoadingIndication) {} // only relevant for plugins
    fn progress_animation_offset(&mut self) {} // only relevant for plugins
    fn current_title(&self) -> String;
    fn custom_title(&self) -> Option<String>;
    fn is_held(&self) -> bool {
        false
    }
    fn exited(&self) -> bool {
        false
    }
    fn exit_status(&self) -> Option<i32> {
        None
    }
    fn rename(&mut self, _buf: Vec<u8>) {}
    fn serialize(&self, _scrollback_lines_to_serialize: Option<usize>) -> Option<String> {
        None
    }
    fn rerun(&mut self) -> Option<RunCommand> {
        None
    } // only relevant to terminal panes
    fn update_theme(&mut self, _theme: Styling) {}
    fn update_arrow_fonts(&mut self, _should_support_arrow_fonts: bool) {}
    fn update_rounded_corners(&mut self, _rounded_corners: bool) {}
    fn set_should_be_suppressed(&mut self, _should_be_suppressed: bool) {}
    fn query_should_be_suppressed(&self) -> bool {
        false
    }
    fn drain_fake_cursors(&mut self) -> Option<HashSet<(usize, usize)>> {
        None
    }
    fn toggle_pinned(&mut self) {}
    fn set_pinned(&mut self, _should_be_pinned: bool) {}
    fn reset_logical_position(&mut self) {}
    fn set_mouse_selection_support(&mut self, _selection_support: bool) {}
}

#[derive(Clone, Debug)]
pub enum AdjustedInput {
    WriteBytesToTerminal(Vec<u8>),
    ReRunCommandInThisPane(RunCommand),
    PermissionRequestResult(Vec<PermissionType>, PermissionStatus),
    CloseThisPane,
    DropToShellInThisPane { working_dir: Option<PathBuf> },
    WriteKeyToPlugin(KeyWithModifier),
}
pub fn get_next_terminal_position(
    tiled_panes: &TiledPanes,
    floating_panes: &FloatingPanes,
) -> usize {
    let tiled_panes_count = tiled_panes
        .get_panes()
        .filter(|(k, _)| match k {
            PaneId::Plugin(_) => false,
            PaneId::Terminal(_) => true,
        })
        .count();
    let floating_panes_count = floating_panes
        .get_panes()
        .filter(|(k, _)| match k {
            PaneId::Plugin(_) => false,
            PaneId::Terminal(_) => true,
        })
        .count();
    tiled_panes_count + floating_panes_count + 1
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
        stacked_resize: Rc<RefCell<bool>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        os_api: Box<dyn ServerOsApi>,
        senders: ThreadSenders,
        max_panes: Option<usize>,
        style: Style,
        default_mode_info: ModeInfo,
        draw_pane_frames: bool,
        auto_layout: bool,
        connected_clients_in_app: Rc<RefCell<HashMap<ClientId, bool>>>, // bool -> is_web_client
        session_is_mirrored: bool,
        client_id: Option<ClientId>,
        copy_options: CopyOptions,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
        swap_layouts: (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>),
        default_shell: PathBuf,
        debug: bool,
        arrow_fonts: bool,
        styled_underlines: bool,
        explicitly_disable_kitty_keyboard_protocol: bool,
        default_editor: Option<PathBuf>,
        web_clients_allowed: bool,
        web_sharing: WebSharing,
        current_pane_group: Rc<RefCell<PaneGroups>>,
        currently_marking_pane_group: Rc<RefCell<HashMap<ClientId, bool>>>,
        advanced_mouse_actions: bool,
        web_server_ip: IpAddr,
        web_server_port: u16,
    ) -> Self {
        let name = if name.is_empty() {
            format!("Tab #{}", index + 1)
        } else {
            name
        };

        let mut connected_clients = HashSet::new();
        if let Some(client_id) = client_id {
            connected_clients.insert(client_id);
        }
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
            stacked_resize.clone(),
            session_is_mirrored,
            draw_pane_frames,
            default_mode_info.clone(),
            style,
            os_api.clone(),
            senders.clone(),
        );
        let floating_panes = FloatingPanes::new(
            display_area.clone(),
            viewport.clone(),
            connected_clients.clone(),
            connected_clients_in_app.clone(),
            mode_info.clone(),
            character_cell_size.clone(),
            session_is_mirrored,
            default_mode_info.clone(),
            style,
            os_api.clone(),
            senders.clone(),
        );

        let clipboard_provider = match copy_options.command {
            Some(command) => ClipboardProvider::Command(CopyCommand::new(command)),
            None => ClipboardProvider::Osc52(copy_options.clipboard),
        };
        let swap_layouts = SwapLayouts::new(swap_layouts, display_area.clone());

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
            auto_layout,
            pending_vte_events: HashMap::new(),
            connected_clients,
            selecting_with_mouse_in_pane: None,
            link_handler: Rc::new(RefCell::new(LinkHandler::new())),
            clipboard_provider,
            focus_pane_id: None,
            copy_on_select: copy_options.copy_on_select,
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            pids_waiting_resize: HashSet::new(),
            cursor_positions_and_shape: HashMap::new(),
            is_pending: true, // will be switched to false once the layout is applied
            pending_instructions: vec![],
            swap_layouts,
            default_shell,
            debug,
            arrow_fonts,
            styled_underlines,
            explicitly_disable_kitty_keyboard_protocol,
            default_editor,
            web_clients_allowed,
            web_sharing,
            mouse_hover_pane_id: HashMap::new(),
            current_pane_group,
            currently_marking_pane_group,
            advanced_mouse_actions,
            connected_clients_in_app,
            web_server_ip,
            web_server_port,
        }
    }

    pub fn apply_layout(
        &mut self,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        client_id: ClientId,
    ) -> Result<()> {
        self.swap_layouts
            .set_base_layout((layout.clone(), floating_panes_layout.clone()));
        match LayoutApplier::new(
            &self.viewport,
            &self.senders,
            &self.sixel_image_store,
            &self.link_handler,
            &self.terminal_emulator_colors,
            &self.terminal_emulator_color_codes,
            &self.character_cell_size,
            &self.connected_clients,
            &self.style,
            &self.display_area,
            &mut self.tiled_panes,
            &mut self.floating_panes,
            self.draw_pane_frames,
            &mut self.focus_pane_id,
            &self.os_api,
            self.debug,
            self.arrow_fonts,
            self.styled_underlines,
            self.explicitly_disable_kitty_keyboard_protocol,
        )
        .apply_layout(
            layout,
            floating_panes_layout,
            new_terminal_ids,
            new_floating_terminal_ids,
            new_plugin_ids,
            client_id,
        ) {
            Ok(should_show_floating_panes) => {
                if should_show_floating_panes && !self.floating_panes.panes_are_visible() {
                    self.toggle_floating_panes(Some(client_id), None)
                        .non_fatal();
                } else if !should_show_floating_panes && self.floating_panes.panes_are_visible() {
                    self.toggle_floating_panes(Some(client_id), None)
                        .non_fatal();
                }
                self.tiled_panes.reapply_pane_frames();
                self.is_pending = false;
                self.apply_buffered_instructions().non_fatal();
            },
            Err(e) => {
                // TODO: this should only happen due to an erroneous layout created by user
                // configuration that was somehow not caught in our KDL layout parser
                // we should still be able to properly recover from this with a useful error
                // message though
                log::error!("Failed to apply layout: {}", e);
            },
        }
        Ok(())
    }
    pub fn swap_layout_info(&self) -> (Option<String>, bool) {
        if self.floating_panes.panes_are_visible() {
            self.swap_layouts.floating_layout_info()
        } else {
            let selectable_tiled_panes =
                self.tiled_panes.get_panes().filter(|(_, p)| p.selectable());
            if selectable_tiled_panes.count() > 1 {
                self.swap_layouts.tiled_layout_info()
            } else {
                // no layout for single pane
                (None, false)
            }
        }
    }
    fn relayout_floating_panes(&mut self, search_backwards: bool) -> Result<()> {
        if let Some(layout_candidate) = self
            .swap_layouts
            .swap_floating_panes(&self.floating_panes, search_backwards)
        {
            LayoutApplier::new(
                &self.viewport,
                &self.senders,
                &self.sixel_image_store,
                &self.link_handler,
                &self.terminal_emulator_colors,
                &self.terminal_emulator_color_codes,
                &self.character_cell_size,
                &self.connected_clients,
                &self.style,
                &self.display_area,
                &mut self.tiled_panes,
                &mut self.floating_panes,
                self.draw_pane_frames,
                &mut self.focus_pane_id,
                &self.os_api,
                self.debug,
                self.arrow_fonts,
                self.styled_underlines,
                self.explicitly_disable_kitty_keyboard_protocol,
            )
            .apply_floating_panes_layout_to_existing_panes(&layout_candidate)
            .non_fatal();
        }
        self.set_force_render();
        self.senders
            .send_to_pty_writer(PtyWriteInstruction::ApplyCachedResizes)
            .with_context(|| format!("failed to apply cached resizes"))?;
        Ok(())
    }
    fn relayout_tiled_panes(&mut self, search_backwards: bool) -> Result<()> {
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        if let Some(layout_candidate) = self
            .swap_layouts
            .swap_tiled_panes(&self.tiled_panes, search_backwards)
        {
            let application_res = LayoutApplier::new(
                &self.viewport,
                &self.senders,
                &self.sixel_image_store,
                &self.link_handler,
                &self.terminal_emulator_colors,
                &self.terminal_emulator_color_codes,
                &self.character_cell_size,
                &self.connected_clients,
                &self.style,
                &self.display_area,
                &mut self.tiled_panes,
                &mut self.floating_panes,
                self.draw_pane_frames,
                &mut self.focus_pane_id,
                &self.os_api,
                self.debug,
                self.arrow_fonts,
                self.styled_underlines,
                self.explicitly_disable_kitty_keyboard_protocol,
            )
            .apply_tiled_panes_layout_to_existing_panes(&layout_candidate);
            if application_res.is_err() {
                self.swap_layouts.set_is_tiled_damaged();
                application_res.non_fatal();
            }
        } else {
            self.swap_layouts.set_is_tiled_damaged();
        }
        self.tiled_panes.reapply_pane_frames();
        let display_area = *self.display_area.borrow();
        // we do this so that the new swap layout has a chance to pass through the constraint system
        self.tiled_panes.resize(display_area);
        self.set_should_clear_display_before_rendering();
        self.senders
            .send_to_pty_writer(PtyWriteInstruction::ApplyCachedResizes)
            .with_context(|| format!("failed to apply cached resizes"))?;
        Ok(())
    }
    pub fn previous_swap_layout(&mut self) -> Result<()> {
        let search_backwards = true;
        if self.floating_panes.panes_are_visible() {
            self.relayout_floating_panes(search_backwards)?;
        } else {
            self.relayout_tiled_panes(search_backwards)?;
        }
        Ok(())
    }
    pub fn next_swap_layout(&mut self) -> Result<()> {
        let search_backwards = false;
        if self.floating_panes.panes_are_visible() {
            self.relayout_floating_panes(search_backwards)?;
        } else {
            self.relayout_tiled_panes(search_backwards)?;
        }
        Ok(())
    }
    pub fn apply_buffered_instructions(&mut self) -> Result<()> {
        let buffered_instructions: Vec<BufferedTabInstruction> =
            self.pending_instructions.drain(..).collect();
        for buffered_instruction in buffered_instructions {
            match buffered_instruction {
                BufferedTabInstruction::SetPaneSelectable(pane_id, selectable) => {
                    self.set_pane_selectable(pane_id, selectable);
                },
                BufferedTabInstruction::HandlePtyBytes(terminal_id, bytes) => {
                    self.handle_pty_bytes(terminal_id, bytes)?;
                },
                BufferedTabInstruction::HoldPane(
                    terminal_id,
                    exit_status,
                    is_first_run,
                    run_command,
                ) => {
                    self.hold_pane(terminal_id, exit_status, is_first_run, run_command);
                },
            }
        }
        Ok(())
    }
    pub fn rename_session(&mut self, new_session_name: String) -> Result<()> {
        {
            let mode_infos = &mut self.mode_info.borrow_mut();
            for (_client_id, mode_info) in mode_infos.iter_mut() {
                mode_info.session_name = Some(new_session_name.clone());
            }
            self.default_mode_info.session_name = Some(new_session_name);
        }
        self.update_input_modes()
    }
    pub fn update_input_modes(&mut self) -> Result<()> {
        // this updates all plugins with the client's input mode
        let mode_infos = self.mode_info.borrow();
        let mut plugin_updates = vec![];
        let currently_marking_pane_group = self.currently_marking_pane_group.borrow();
        for client_id in self.connected_clients.borrow().iter() {
            let mut mode_info = mode_infos
                .get(client_id)
                .unwrap_or(&self.default_mode_info)
                .clone();
            mode_info.shell = Some(self.default_shell.clone());
            mode_info.editor = self.default_editor.clone();
            mode_info.web_clients_allowed = Some(self.web_clients_allowed);
            mode_info.web_sharing = Some(self.web_sharing);
            mode_info.currently_marking_pane_group =
                currently_marking_pane_group.get(client_id).copied();
            mode_info.web_server_ip = Some(self.web_server_ip);
            mode_info.web_server_port = Some(self.web_server_port);
            mode_info.is_web_client = self
                .connected_clients_in_app
                .borrow()
                .get(&client_id)
                .copied();
            if cfg!(feature = "web_server_capability") {
                mode_info.web_server_capability = Some(true);
            } else {
                mode_info.web_server_capability = Some(false);
            }
            plugin_updates.push((None, Some(*client_id), Event::ModeUpdate(mode_info)));
        }
        self.senders
            .send_to_plugin(PluginInstruction::Update(plugin_updates))
            .with_context(|| format!("failed to update plugins with mode info"))?;
        Ok(())
    }
    pub fn add_client(&mut self, client_id: ClientId, mode_info: Option<ModeInfo>) -> Result<()> {
        let other_clients_exist_in_tab = { !self.connected_clients.borrow().is_empty() };
        if other_clients_exist_in_tab {
            if let Some(first_active_floating_pane_id) =
                self.floating_panes.first_active_floating_pane_id()
            {
                self.floating_panes
                    .focus_pane_if_client_not_focused(first_active_floating_pane_id, client_id);
            }
            if let Some(first_active_tiled_pane_id) = self.tiled_panes.first_active_pane_id() {
                self.tiled_panes
                    .focus_pane_if_client_not_focused(first_active_tiled_pane_id, client_id);
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
                return Ok(());
            }
            let focus_pane_id = if let Some(id) = self.focus_pane_id {
                id
            } else {
                pane_ids.sort(); // TODO: make this predictable
                pane_ids.retain(|p| !self.tiled_panes.panes_to_hide_contains(*p));
                *(pane_ids.get(0).with_context(|| {
                    format!("failed to acquire id of focused pane while adding client {client_id}",)
                })?)
            };
            self.tiled_panes
                .focus_pane_if_client_not_focused(focus_pane_id, client_id);
            self.floating_panes
                .focus_first_pane_if_client_not_focused(client_id);
            self.connected_clients.borrow_mut().insert(client_id);
            self.mode_info.borrow_mut().insert(
                client_id,
                mode_info.unwrap_or_else(|| self.default_mode_info.clone()),
            );
        }
        self.set_force_render();
        Ok(())
    }

    pub fn change_mode_info(&mut self, mode_info: ModeInfo, client_id: ClientId) {
        self.mode_info.borrow_mut().insert(client_id, mode_info);
    }

    pub fn add_multiple_clients(
        &mut self,
        client_ids_to_mode_infos: Vec<(ClientId, ModeInfo)>,
    ) -> Result<()> {
        for (client_id, client_mode_info) in client_ids_to_mode_infos {
            self.add_client(client_id, None)
                .context("failed to add clients")?;
            self.mode_info
                .borrow_mut()
                .insert(client_id, client_mode_info);
        }
        Ok(())
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
    pub fn pane_id_is_floating(&self, pane_id: &PaneId) -> bool {
        self.floating_panes.panes_contain(pane_id)
    }
    pub fn toggle_pane_embed_or_floating(&mut self, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to toggle embedded/floating pane for client {client_id}");
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        if self.floating_panes.panes_are_visible() {
            if let Some(focused_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                if self.tiled_panes.has_room_for_new_pane() {
                    let floating_pane_to_embed = self
                        .extract_pane(focused_floating_pane_id, true)
                        .with_context(|| format!(
                        "failed to find floating pane (ID: {focused_floating_pane_id:?}) to embed for client {client_id}",
                    ))
                        .with_context(err_context)?;
                    self.hide_floating_panes();
                    self.add_tiled_pane(
                        floating_pane_to_embed,
                        focused_floating_pane_id,
                        Some(client_id),
                    )?;
                }
            }
        } else if let Some(focused_pane_id) = self.tiled_panes.focused_pane_id(client_id) {
            if self.get_selectable_tiled_panes().count() <= 1 {
                // don't close the only pane on screen...
                return Ok(());
            }
            if let Some(embedded_pane_to_float) = self.extract_pane(focused_pane_id, true) {
                self.show_floating_panes();
                self.add_floating_pane(embedded_pane_to_float, focused_pane_id, None, true)?;
            }
        }
        Ok(())
    }
    pub fn toggle_pane_embed_or_floating_for_pane_id(
        &mut self,
        pane_id: PaneId,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let err_context = || {
            format!(
                "failed to toggle embedded/floating pane for pane_id {:?}",
                pane_id
            )
        };
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        if self.floating_panes.panes_contain(&pane_id) {
            if self.tiled_panes.has_room_for_new_pane() {
                let floating_pane_to_embed = self
                    .extract_pane(pane_id, true)
                    .with_context(|| {
                        format!("failed to find floating pane (ID: {pane_id:?}) to embed",)
                    })
                    .with_context(err_context)?;
                self.add_tiled_pane(floating_pane_to_embed, pane_id, client_id)?;
            }
        } else if self.tiled_panes.panes_contain(&pane_id) {
            if self.get_selectable_tiled_panes().count() <= 1 {
                log::error!("Cannot float the last tiled pane...");
                // don't close the only pane on screen...
                return Ok(());
            }
            if let Some(embedded_pane_to_float) = self.extract_pane(pane_id, true) {
                self.add_floating_pane(embedded_pane_to_float, pane_id, None, true)?;
            }
        }
        Ok(())
    }
    pub fn toggle_floating_panes(
        &mut self,
        client_id: Option<ClientId>,
        default_shell: Option<TerminalAction>,
    ) -> Result<()> {
        if self.floating_panes.panes_are_visible() {
            self.hide_floating_panes();
            self.set_force_render();
        } else {
            self.show_floating_panes();
            match self.floating_panes.last_selectable_floating_pane_id() {
                Some(last_selectable_floating_pane_id) => match client_id {
                    Some(client_id) => {
                        if !self.floating_panes.active_panes_contain(&client_id) {
                            self.floating_panes
                                .focus_pane(last_selectable_floating_pane_id, client_id);
                        }
                    },
                    None => {
                        self.floating_panes
                            .focus_pane_for_all_clients(last_selectable_floating_pane_id);
                    },
                },
                None => {
                    let name = None;
                    let client_id_or_tab_index = match client_id {
                        Some(client_id) => ClientTabIndexOrPaneId::ClientId(client_id),
                        None => ClientTabIndexOrPaneId::TabIndex(self.index),
                    };
                    let should_start_suppressed = false;
                    let instruction = PtyInstruction::SpawnTerminal(
                        default_shell,
                        name,
                        NewPanePlacement::Floating(None),
                        should_start_suppressed,
                        client_id_or_tab_index,
                    );
                    self.senders
                        .send_to_pty(instruction)
                        .with_context(|| format!("failed to open a floating pane for client"))?;
                },
            }
            self.floating_panes.set_force_render();
        }
        self.set_force_render();
        Ok(())
    }
    pub fn new_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        start_suppressed: bool,
        should_focus_pane: bool,
        new_pane_placement: NewPanePlacement,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        match new_pane_placement {
            NewPanePlacement::NoPreference => self.new_no_preference_pane(
                pid,
                initial_pane_title,
                invoked_with,
                start_suppressed,
                should_focus_pane,
                client_id,
            ),
            NewPanePlacement::Tiled(None) => self.new_tiled_pane(
                pid,
                initial_pane_title,
                invoked_with,
                start_suppressed,
                should_focus_pane,
                client_id,
            ),
            NewPanePlacement::Tiled(Some(direction)) => {
                if let Some(client_id) = client_id {
                    if direction == Direction::Left || direction == Direction::Right {
                        self.vertical_split(pid, initial_pane_title, client_id)?;
                    } else {
                        self.horizontal_split(pid, initial_pane_title, client_id)?;
                    }
                }
                Ok(())
            },
            NewPanePlacement::Floating(floating_pane_coordinates) => self.new_floating_pane(
                pid,
                initial_pane_title,
                invoked_with,
                start_suppressed,
                should_focus_pane,
                floating_pane_coordinates,
            ),
            NewPanePlacement::InPlace {
                pane_id_to_replace,
                close_replaced_pane,
            } => self.new_in_place_pane(
                pid,
                initial_pane_title,
                invoked_with,
                pane_id_to_replace,
                close_replaced_pane,
                client_id,
            ),
            NewPanePlacement::Stacked(pane_id_to_stack_under) => self.new_stacked_pane(
                pid,
                initial_pane_title,
                invoked_with,
                start_suppressed,
                should_focus_pane,
                pane_id_to_stack_under,
                client_id,
            ),
        }
    }
    pub fn new_no_preference_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        start_suppressed: bool,
        should_focus_pane: bool,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let err_context = || format!("failed to create new pane with id {pid:?}");
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
        let mut new_pane = match pid {
            PaneId::Terminal(term_pid) => {
                let next_terminal_position = self.get_next_terminal_position();
                Box::new(TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    initial_pane_title,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                )) as Box<dyn Pane>
            },
            PaneId::Plugin(plugin_pid) => {
                Box::new(PluginPane::new(
                    plugin_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    initial_pane_title.unwrap_or("".to_owned()),
                    String::new(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                )) as Box<dyn Pane>
            },
        };

        if start_suppressed {
            // this pane needs to start in the background (suppressed), only accessible if a plugin takes it out
            // of there in one way or another
            // we need to do some bookkeeping for this pane, namely setting its geom and
            // content_offset so that things will appear properly in the terminal - we set it to
            // the default geom of the first floating pane - this is just in order to give it some
            // reasonable size, when it is shown - if needed - it will be given the proper geom as if it were
            // resized
            let viewport = { self.viewport.borrow().clone() };
            let new_pane_geom = half_size_middle_geom(&viewport, 0);
            new_pane.set_active_at(Instant::now());
            new_pane.set_geom(new_pane_geom);
            new_pane.set_content_offset(Offset::frame(1));
            resize_pty!(
                new_pane,
                self.os_api,
                self.senders,
                self.character_cell_size
            )
            .with_context(err_context)?;
            let is_scrollback_editor = false;
            self.suppressed_panes
                .insert(pid, (is_scrollback_editor, new_pane));
            Ok(())
        } else if should_focus_pane {
            if self.floating_panes.panes_are_visible() {
                self.add_floating_pane(new_pane, pid, None, true)
            } else {
                self.add_tiled_pane(new_pane, pid, client_id)
            }
        } else {
            if self.floating_panes.panes_are_visible() {
                self.add_floating_pane(new_pane, pid, None, false)
            } else {
                self.add_tiled_pane(new_pane, pid, client_id)
            }
        }
    }
    pub fn new_tiled_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        start_suppressed: bool,
        should_focus_pane: bool,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let err_context = || format!("failed to create new pane with id {pid:?}");
        if should_focus_pane {
            self.hide_floating_panes();
        }
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
        let mut new_pane = match pid {
            PaneId::Terminal(term_pid) => {
                let next_terminal_position = self.get_next_terminal_position();
                Box::new(TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    initial_pane_title,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                )) as Box<dyn Pane>
            },
            PaneId::Plugin(plugin_pid) => {
                Box::new(PluginPane::new(
                    plugin_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    initial_pane_title.unwrap_or("".to_owned()),
                    String::new(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                )) as Box<dyn Pane>
            },
        };

        if start_suppressed {
            // this pane needs to start in the background (suppressed), only accessible if a plugin takes it out
            // of there in one way or another
            // we need to do some bookkeeping for this pane, namely setting its geom and
            // content_offset so that things will appear properly in the terminal - we set it to
            // the default geom of the first floating pane - this is just in order to give it some
            // reasonable size, when it is shown - if needed - it will be given the proper geom as if it were
            // resized
            let viewport = { self.viewport.borrow().clone() };
            let new_pane_geom = half_size_middle_geom(&viewport, 0);
            new_pane.set_active_at(Instant::now());
            new_pane.set_geom(new_pane_geom);
            new_pane.set_content_offset(Offset::frame(1));
            resize_pty!(
                new_pane,
                self.os_api,
                self.senders,
                self.character_cell_size
            )
            .with_context(err_context)?;
            let is_scrollback_editor = false;
            self.suppressed_panes
                .insert(pid, (is_scrollback_editor, new_pane));
            Ok(())
        } else {
            self.add_tiled_pane(new_pane, pid, client_id)
        }
    }
    pub fn new_floating_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        start_suppressed: bool,
        should_focus_pane: bool,
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
    ) -> Result<()> {
        let err_context = || format!("failed to create new pane with id {pid:?}");
        if should_focus_pane {
            self.show_floating_panes();
        }
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
        let mut new_pane = match pid {
            PaneId::Terminal(term_pid) => {
                let next_terminal_position = self.get_next_terminal_position();
                Box::new(TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    initial_pane_title,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                )) as Box<dyn Pane>
            },
            PaneId::Plugin(plugin_pid) => {
                Box::new(PluginPane::new(
                    plugin_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    initial_pane_title.unwrap_or("".to_owned()),
                    String::new(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                )) as Box<dyn Pane>
            },
        };

        if start_suppressed {
            // this pane needs to start in the background (suppressed), only accessible if a plugin takes it out
            // of there in one way or another
            // we need to do some bookkeeping for this pane, namely setting its geom and
            // content_offset so that things will appear properly in the terminal - we set it to
            // the default geom of the first floating pane - this is just in order to give it some
            // reasonable size, when it is shown - if needed - it will be given the proper geom as if it were
            // resized
            let viewport = { self.viewport.borrow().clone() };
            let new_pane_geom = half_size_middle_geom(&viewport, 0);
            new_pane.set_active_at(Instant::now());
            new_pane.set_geom(new_pane_geom);
            new_pane.set_content_offset(Offset::frame(1));
            resize_pty!(
                new_pane,
                self.os_api,
                self.senders,
                self.character_cell_size
            )
            .with_context(err_context)?;
            let is_scrollback_editor = false;
            self.suppressed_panes
                .insert(pid, (is_scrollback_editor, new_pane));
            Ok(())
        } else {
            self.add_floating_pane(new_pane, pid, floating_pane_coordinates, should_focus_pane)
        }
    }
    pub fn new_in_place_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        pane_id_to_replace: Option<PaneId>,
        close_replaced_pane: bool,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        match (pane_id_to_replace, client_id) {
            (Some(pane_id_to_replace), _) => {
                self.suppress_pane_and_replace_with_pid(
                    pane_id_to_replace,
                    pid,
                    close_replaced_pane,
                    invoked_with,
                )?;
            },
            (None, Some(client_id)) => match self.get_active_pane_id(client_id) {
                Some(active_pane_id) => {
                    self.suppress_pane_and_replace_with_pid(
                        active_pane_id,
                        pid,
                        close_replaced_pane,
                        invoked_with,
                    )?;
                },
                None => {
                    log::error!("Cannot find active pane");
                },
            },
            _ => {
                log::error!("Must have pane id to replace or client id to start pane in place>");
            },
        }
        if let Some(initial_pane_title) = initial_pane_title {
            let _ = self.rename_pane(initial_pane_title.as_bytes().to_vec(), pid);
        }
        Ok(())
    }
    pub fn new_stacked_pane(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        invoked_with: Option<Run>,
        start_suppressed: bool,
        should_focus_pane: bool,
        pane_id_to_stack_under: Option<PaneId>,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let err_context = || format!("failed to create new pane with id {pid:?}");
        if should_focus_pane {
            self.hide_floating_panes();
        }
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
        let mut new_pane = match pid {
            PaneId::Terminal(term_pid) => {
                let next_terminal_position = self.get_next_terminal_position();
                Box::new(TerminalPane::new(
                    term_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    initial_pane_title,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                )) as Box<dyn Pane>
            },
            PaneId::Plugin(plugin_pid) => {
                Box::new(PluginPane::new(
                    plugin_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    initial_pane_title.unwrap_or("".to_owned()),
                    String::new(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    invoked_with,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                )) as Box<dyn Pane>
            },
        };

        if start_suppressed {
            // this pane needs to start in the background (suppressed), only accessible if a plugin takes it out
            // of there in one way or another
            // we need to do some bookkeeping for this pane, namely setting its geom and
            // content_offset so that things will appear properly in the terminal - we set it to
            // the default geom of the first floating pane - this is just in order to give it some
            // reasonable size, when it is shown - if needed - it will be given the proper geom as if it were
            // resized
            let viewport = { self.viewport.borrow().clone() };
            let new_pane_geom = half_size_middle_geom(&viewport, 0);
            new_pane.set_active_at(Instant::now());
            new_pane.set_geom(new_pane_geom);
            new_pane.set_content_offset(Offset::frame(1));
            resize_pty!(
                new_pane,
                self.os_api,
                self.senders,
                self.character_cell_size
            )
            .with_context(err_context)?;
            let is_scrollback_editor = false;
            self.suppressed_panes
                .insert(pid, (is_scrollback_editor, new_pane));
            Ok(())
        } else {
            if let Some(pane_id_to_stack_under) = pane_id_to_stack_under {
                // TODO: also focus pane if should_focus_pane? in cases where we did this from the CLI in an unfocused
                // pane...
                self.add_stacked_pane_to_pane_id(new_pane, pid, pane_id_to_stack_under)
            } else if let Some(client_id) = client_id {
                self.add_stacked_pane_to_active_pane(new_pane, pid, client_id)
            } else {
                log::error!("Must have client id or pane id to stack pane");
                return Ok(());
            }
        }
    }
    pub fn replace_active_pane_with_editor_pane(
        &mut self,
        pid: PaneId,
        client_id: ClientId,
    ) -> Result<()> {
        // this method creates a new pane from pid and replaces it with the active pane
        // the active pane is then suppressed (hidden and not rendered) until the current
        // created pane is closed, in which case it will be replaced back by it
        let err_context = || format!("failed to suppress active pane for client {client_id}");

        match pid {
            PaneId::Terminal(pid) => {
                let new_pane = self.new_scrollback_editor_pane(pid);
                let replaced_pane = if self.floating_panes.panes_are_visible() {
                    self.floating_panes
                        .replace_active_pane(Box::new(new_pane), client_id)
                        .ok()
                } else {
                    self.tiled_panes
                        .replace_active_pane(Box::new(new_pane), client_id)
                };
                match replaced_pane {
                    Some(replaced_pane) => {
                        self.insert_scrollback_editor_replaced_pane(replaced_pane, pid);
                        self.get_active_pane(client_id)
                            .with_context(|| format!("no active pane found for client {client_id}"))
                            .and_then(|current_active_pane| {
                                resize_pty!(
                                    current_active_pane,
                                    self.os_api,
                                    self.senders,
                                    self.character_cell_size
                                )
                            })
                            .with_context(err_context)?;
                    },
                    None => {
                        Err::<(), _>(anyhow!(
                            "Could not find editor pane to replace - is no pane focused?"
                        ))
                        .with_context(err_context)
                        .non_fatal();
                    },
                }
            },
            PaneId::Plugin(_pid) => {
                // TBD, currently unsupported
            },
        }
        Ok(())
    }
    pub fn replace_pane_with_editor_pane(
        &mut self,
        pid: PaneId,
        pane_id_to_replace: PaneId,
    ) -> Result<()> {
        // this method creates a new pane from pid and replaces it with the pane iwth the given pane_id_to_replace
        // the pane with the given pane_id_to_replace is then suppressed (hidden and not rendered) until the current
        // created pane is closed, in which case it will be replaced back by it
        let err_context = || format!("failed to suppress pane");

        match pid {
            PaneId::Terminal(pid) => {
                let new_pane = self.new_scrollback_editor_pane(pid);
                let replaced_pane = if self.floating_panes.panes_contain(&pane_id_to_replace) {
                    self.floating_panes
                        .replace_pane(pane_id_to_replace, Box::new(new_pane))
                        .ok()
                } else if self.tiled_panes.panes_contain(&pane_id_to_replace) {
                    self.tiled_panes
                        .replace_pane(pane_id_to_replace, Box::new(new_pane))
                } else if self
                    .suppressed_panes
                    .values()
                    .any(|s_p| s_p.1.pid() == pane_id_to_replace)
                {
                    log::error!("Cannot replace suppressed pane");
                    None
                } else {
                    // not a thing
                    None
                };
                match replaced_pane {
                    Some(replaced_pane) => {
                        resize_pty!(
                            replaced_pane,
                            self.os_api,
                            self.senders,
                            self.character_cell_size
                        )
                        .non_fatal();
                        self.insert_scrollback_editor_replaced_pane(replaced_pane, pid);
                    },
                    None => {
                        Err::<(), _>(anyhow!("Could not find editor pane to replace"))
                            .with_context(err_context)
                            .non_fatal();
                    },
                }
            },
            PaneId::Plugin(_pid) => {
                // TBD, currently unsupported
            },
        }
        Ok(())
    }
    pub fn suppress_pane_and_replace_with_pid(
        &mut self,
        old_pane_id: PaneId,
        new_pane_id: PaneId,
        close_replaced_pane: bool,
        run: Option<Run>,
    ) -> Result<()> {
        // this method creates a new pane from pid and replaces it with the active pane
        // the active pane is then suppressed (hidden and not rendered) until the current
        // created pane is closed, in which case it will be replaced back by it
        let err_context = || format!("failed to suppress active pane");

        match new_pane_id {
            PaneId::Terminal(new_pane_id) => {
                let next_terminal_position = self.get_next_terminal_position(); // TODO: this is not accurate in this case
                let new_pane = TerminalPane::new(
                    new_pane_id,
                    PaneGeom::default(), // the initial size will be set later
                    self.style,
                    next_terminal_position,
                    String::new(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    None,
                    run,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                );
                let replaced_pane = if self.floating_panes.panes_contain(&old_pane_id) {
                    self.floating_panes
                        .replace_pane(old_pane_id, Box::new(new_pane))
                        .ok()
                } else {
                    self.tiled_panes
                        .replace_pane(old_pane_id, Box::new(new_pane))
                };
                if close_replaced_pane {
                    if let Some(pid) = replaced_pane.as_ref().map(|p| p.pid()) {
                        self.senders
                            .send_to_pty(PtyInstruction::ClosePane(pid))
                            .with_context(err_context)?;
                    }
                    drop(replaced_pane);
                } else {
                    match replaced_pane {
                        Some(replaced_pane) => {
                            let _ = resize_pty!(
                                replaced_pane,
                                self.os_api,
                                self.senders,
                                self.character_cell_size
                            );
                            let is_scrollback_editor = false;
                            self.suppressed_panes.insert(
                                PaneId::Terminal(new_pane_id),
                                (is_scrollback_editor, replaced_pane),
                            );
                        },
                        None => {
                            Err::<(), _>(anyhow!(
                                "Could not find editor pane to replace - is no pane focused?"
                            ))
                            .with_context(err_context)
                            .non_fatal();
                        },
                    }
                }
            },
            PaneId::Plugin(plugin_pid) => {
                let new_pane = PluginPane::new(
                    plugin_pid,
                    PaneGeom::default(), // this will be filled out later
                    self.senders
                        .to_plugin
                        .as_ref()
                        .with_context(err_context)?
                        .clone(),
                    String::new(),
                    String::new(),
                    self.sixel_image_store.clone(),
                    self.terminal_emulator_colors.clone(),
                    self.terminal_emulator_color_codes.clone(),
                    self.link_handler.clone(),
                    self.character_cell_size.clone(),
                    self.connected_clients.borrow().iter().copied().collect(),
                    self.style,
                    run,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                );
                let replaced_pane = if self.floating_panes.panes_contain(&old_pane_id) {
                    self.floating_panes
                        .replace_pane(old_pane_id, Box::new(new_pane))
                        .ok()
                } else {
                    self.tiled_panes
                        .replace_pane(old_pane_id, Box::new(new_pane))
                };
                if close_replaced_pane {
                    drop(replaced_pane);
                } else {
                    match replaced_pane {
                        Some(replaced_pane) => {
                            let _ = resize_pty!(
                                replaced_pane,
                                self.os_api,
                                self.senders,
                                self.character_cell_size
                            );
                            let is_scrollback_editor = false;
                            self.suppressed_panes.insert(
                                PaneId::Plugin(plugin_pid),
                                (is_scrollback_editor, replaced_pane),
                            );
                        },
                        None => {
                            Err::<(), _>(anyhow!(
                                "Could not find editor pane to replace - is no pane focused?"
                            ))
                            .with_context(err_context)
                            .non_fatal();
                        },
                    }
                }
            },
        }
        Ok(())
    }
    pub fn close_pane_and_replace_with_other_pane(
        &mut self,
        pane_id_to_replace: PaneId,
        pane_to_replace_with: Box<dyn Pane>,
    ) {
        let mut replaced_pane = if self.floating_panes.panes_contain(&pane_id_to_replace) {
            self.floating_panes
                .replace_pane(pane_id_to_replace, pane_to_replace_with)
                .ok()
        } else {
            self.tiled_panes
                .replace_pane(pane_id_to_replace, pane_to_replace_with)
        };
        if let Some(replaced_pane) = replaced_pane.take() {
            let pane_id = replaced_pane.pid();
            let _ = self.senders.send_to_pty(PtyInstruction::ClosePane(pane_id));
            let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
                None,
                None,
                Event::PaneClosed(pane_id.into()),
            )]));
            drop(replaced_pane);
        }
    }
    pub fn horizontal_split(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context =
            || format!("failed to split pane {pid:?} horizontally for client {client_id}");
        if self.floating_panes.panes_are_visible() {
            return Ok(());
        }
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
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
                    initial_pane_title,
                    None,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                );
                self.tiled_panes
                    .split_pane_horizontally(pid, Box::new(new_terminal), client_id);
                self.set_should_clear_display_before_rendering();
                self.tiled_panes.focus_pane(pid, client_id);
                self.swap_layouts.set_is_tiled_damaged();
            }
        } else {
            log::error!("No room to split pane horizontally");
            if let Some(active_pane_id) = self.tiled_panes.get_active_pane_id(client_id) {
                self.senders
                    .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                        vec![active_pane_id],
                        "CAN'T SPLIT!".into(),
                    ))
                    .with_context(err_context)?;
            }
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(pid))
                .with_context(err_context)?;
            return Ok(());
        }
        Ok(())
    }
    pub fn vertical_split(
        &mut self,
        pid: PaneId,
        initial_pane_title: Option<String>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context =
            || format!("failed to split pane {pid:?} vertically for client {client_id}");
        if self.floating_panes.panes_are_visible() {
            return Ok(());
        }
        self.close_down_to_max_terminals()
            .with_context(err_context)?;
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
                    initial_pane_title,
                    None,
                    self.debug,
                    self.arrow_fonts,
                    self.styled_underlines,
                    self.explicitly_disable_kitty_keyboard_protocol,
                );
                self.tiled_panes
                    .split_pane_vertically(pid, Box::new(new_terminal), client_id);
                self.set_should_clear_display_before_rendering();
                self.tiled_panes.focus_pane(pid, client_id);
                self.swap_layouts.set_is_tiled_damaged();
            }
        } else {
            log::error!("No room to split pane vertically");
            if let Some(active_pane_id) = self.tiled_panes.get_active_pane_id(client_id) {
                self.senders
                    .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                        vec![active_pane_id],
                        "CAN'T SPLIT!".into(),
                    ))
                    .with_context(err_context)?;
            }
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(pid))
                .with_context(err_context)?;
            return Ok(());
        }
        Ok(())
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
    pub fn get_pane_with_id(&self, pane_id: PaneId) -> Option<&dyn Pane> {
        self.floating_panes
            .get_pane(pane_id)
            .map(Box::as_ref)
            .or_else(|| self.tiled_panes.get_pane(pane_id).map(Box::as_ref))
            .or_else(|| self.suppressed_panes.get(&pane_id).map(|p| p.1.as_ref()))
    }
    pub fn get_pane_with_id_mut(&mut self, pane_id: PaneId) -> Option<&mut Box<dyn Pane>> {
        self.floating_panes
            .get_pane_mut(pane_id)
            .or_else(|| self.tiled_panes.get_pane_mut(pane_id))
            .or_else(|| {
                self.suppressed_panes
                    .get_mut(&pane_id)
                    .map(|(_, pane)| pane)
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
    fn get_active_terminal_id(&self, client_id: ClientId) -> Option<u32> {
        if let Some(PaneId::Terminal(pid)) = self.get_active_pane_id(client_id) {
            Some(pid)
        } else {
            None
        }
    }
    pub fn has_terminal_pid(&self, pid: u32) -> bool {
        self.tiled_panes.panes_contain(&PaneId::Terminal(pid))
            || self.floating_panes.panes_contain(&PaneId::Terminal(pid))
            || self
                .suppressed_panes
                .values()
                .any(|s_p| s_p.1.pid() == PaneId::Terminal(pid))
    }
    pub fn has_plugin(&self, plugin_id: u32) -> bool {
        self.tiled_panes.panes_contain(&PaneId::Plugin(plugin_id))
            || self
                .floating_panes
                .panes_contain(&PaneId::Plugin(plugin_id))
            || self
                .suppressed_panes
                .values()
                .any(|s_p| s_p.1.pid() == PaneId::Plugin(plugin_id))
    }
    pub fn has_pane_with_pid(&self, pid: &PaneId) -> bool {
        self.tiled_panes.panes_contain(pid)
            || self.floating_panes.panes_contain(pid)
            || self
                .suppressed_panes
                .values()
                .any(|s_p| s_p.1.pid() == *pid)
    }
    pub fn has_non_suppressed_pane_with_pid(&self, pid: &PaneId) -> bool {
        self.tiled_panes.panes_contain(pid) || self.floating_panes.panes_contain(pid)
    }
    pub fn handle_pty_bytes(&mut self, pid: u32, bytes: VteBytes) -> Result<()> {
        if self.is_pending {
            self.pending_instructions
                .push(BufferedTabInstruction::HandlePtyBytes(pid, bytes));
            return Ok(());
        }
        let err_context = || format!("failed to handle pty bytes from fd {pid}");
        if let Some(terminal_output) = self
            .tiled_panes
            .get_pane_mut(PaneId::Terminal(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Terminal(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Terminal(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            // If the pane is scrolled buffer the vte events
            if terminal_output.is_scrolled() {
                self.pending_vte_events.entry(pid).or_default().push(bytes);
                if let Some(evs) = self.pending_vte_events.get(&pid) {
                    // Reset scroll - and process all pending events for this pane
                    if evs.len() >= MAX_PENDING_VTE_EVENTS {
                        terminal_output.clear_scroll();
                        self.process_pending_vte_events(pid)
                            .with_context(err_context)?;
                    }
                }
                return Ok(());
            }
        }
        self.process_pty_bytes(pid, bytes).with_context(err_context)
    }
    pub fn handle_plugin_bytes(
        &mut self,
        pid: u32,
        client_id: ClientId,
        bytes: VteBytes,
    ) -> Result<()> {
        if let Some(plugin_pane) = self
            .tiled_panes
            .get_pane_mut(PaneId::Plugin(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Plugin(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Plugin(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            plugin_pane.handle_plugin_bytes(client_id, bytes);
        }
        Ok(())
    }
    pub fn process_pending_vte_events(&mut self, pid: u32) -> Result<()> {
        if let Some(pending_vte_events) = self.pending_vte_events.get_mut(&pid) {
            let vte_events: Vec<VteBytes> = pending_vte_events.drain(..).collect();
            for vte_event in vte_events {
                self.process_pty_bytes(pid, vte_event)
                    .context("failed to process pending vte events")?;
            }
        }
        Ok(())
    }
    fn process_pty_bytes(&mut self, pid: u32, bytes: VteBytes) -> Result<()> {
        let err_context = || format!("failed to process pty bytes from pid {pid}");

        if let Some(terminal_output) = self
            .tiled_panes
            .get_pane_mut(PaneId::Terminal(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Terminal(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Terminal(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            if self.pids_waiting_resize.remove(&pid) {
                resize_pty!(
                    terminal_output,
                    self.os_api,
                    self.senders,
                    self.character_cell_size
                )
                .with_context(err_context)?;
            }
            terminal_output.handle_pty_bytes(bytes);
            let messages_to_pty = terminal_output.drain_messages_to_pty();
            let clipboard_update = terminal_output.drain_clipboard_update();
            for message in messages_to_pty {
                self.write_to_pane_id_without_preprocessing(message, PaneId::Terminal(pid))
                    .with_context(err_context)?;
            }
            if let Some(string) = clipboard_update {
                self.write_selection_to_clipboard(&string)
                    .with_context(err_context)?;
            }
        }
        Ok(())
    }

    pub fn write_to_terminals_on_current_tab(
        &mut self,
        key_with_modifier: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
        client_id: ClientId,
    ) -> Result<bool> {
        // returns true if a UI update should be triggered (eg. when closing a command pane with
        // ctrl-c)
        let mut should_trigger_ui_change = false;
        let pane_ids = self.get_static_and_floating_pane_ids();
        for pane_id in pane_ids {
            let ui_change_triggered = self
                .write_to_pane_id(
                    key_with_modifier,
                    raw_input_bytes.clone(),
                    raw_input_bytes_are_kitty,
                    pane_id,
                    Some(client_id),
                )
                .context("failed to write to terminals on current tab")?;
            if ui_change_triggered {
                should_trigger_ui_change = true;
            }
        }
        Ok(should_trigger_ui_change)
    }

    pub fn write_to_active_terminal(
        &mut self,
        key_with_modifier: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
        client_id: ClientId,
    ) -> Result<bool> {
        // returns true if a UI update should be triggered (eg. if a command pane
        // was closed with ctrl-c)
        let err_context = || {
            format!(
                "failed to write to active terminal for client {client_id} - msg: {raw_input_bytes:?}"
            )
        };

        self.clear_search(client_id); // this is an inexpensive operation if empty, if we need more such cleanups we should consider moving this and the rest to some sort of cleanup method
        let pane_id = if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .get_active_pane_id(client_id)
                .or_else(|| self.tiled_panes.get_active_pane_id(client_id))
                .ok_or_else(|| {
                    anyhow!(format!(
                        "failed to find active pane id for client {client_id}"
                    ))
                })
                .with_context(err_context)?
        } else {
            self.tiled_panes
                .get_active_pane_id(client_id)
                .with_context(err_context)?
        };
        // Can't use 'err_context' here since it borrows 'raw_input_bytes'
        self.write_to_pane_id(
            key_with_modifier,
            raw_input_bytes,
            raw_input_bytes_are_kitty,
            pane_id,
            Some(client_id),
        )
        .with_context(|| format!("failed to write to active terminal for client {client_id}"))
    }

    pub fn write_to_terminal_at(
        &mut self,
        input_bytes: Vec<u8>,
        position: &Position,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to write to terminal at position {position:?}");

        if self.floating_panes.panes_are_visible() {
            let pane_id = self
                .floating_panes
                .get_pane_id_at(position, false)
                .with_context(err_context)?;
            if let Some(pane_id) = pane_id {
                self.write_to_pane_id(&None, input_bytes, false, pane_id, Some(client_id))
                    .with_context(err_context)?;
                return Ok(());
            }
        }

        let pane_id = self
            .get_pane_id_at(position, false)
            .with_context(err_context)?;
        if let Some(pane_id) = pane_id {
            self.write_to_pane_id(&None, input_bytes, false, pane_id, Some(client_id))
                .with_context(err_context)?;
            return Ok(());
        }
        Ok(())
    }

    pub fn write_to_pane_id(
        &mut self,
        key_with_modifier: &Option<KeyWithModifier>,
        raw_input_bytes: Vec<u8>,
        raw_input_bytes_are_kitty: bool,
        pane_id: PaneId,
        client_id: Option<ClientId>,
    ) -> Result<bool> {
        // returns true if we need to update the UI (eg. when a command pane is closed with ctrl-c)
        let err_context = || format!("failed to write to pane with id {pane_id:?}");

        let mut should_update_ui = false;
        let is_sync_panes_active = self.is_sync_panes_active();

        let active_pane = self
            .floating_panes
            .get_mut(&pane_id)
            .or_else(|| self.tiled_panes.get_pane_mut(pane_id))
            .or_else(|| self.suppressed_panes.get_mut(&pane_id).map(|p| &mut p.1))
            .ok_or_else(|| anyhow!(format!("failed to find pane with id {pane_id:?}")))
            .with_context(err_context)?;

        // We always write for non-synced terminals.
        // However if the terminal is part of a tab-sync, we need to
        // check if the terminal should receive input or not (depending on its
        // 'exclude_from_sync' configuration).
        let should_not_write_to_terminal = is_sync_panes_active && active_pane.exclude_from_sync();

        if should_not_write_to_terminal {
            return Ok(should_update_ui);
        }

        match pane_id {
            PaneId::Terminal(active_terminal_id) => {
                match active_pane.adjust_input_to_terminal(
                    key_with_modifier,
                    raw_input_bytes,
                    raw_input_bytes_are_kitty,
                    client_id,
                ) {
                    Some(AdjustedInput::WriteBytesToTerminal(adjusted_input)) => {
                        self.senders
                            .send_to_pty_writer(PtyWriteInstruction::Write(
                                adjusted_input,
                                active_terminal_id,
                            ))
                            .with_context(err_context)?;
                    },
                    Some(AdjustedInput::ReRunCommandInThisPane(command)) => {
                        self.pids_waiting_resize.insert(active_terminal_id);
                        self.senders
                            .send_to_pty(PtyInstruction::ReRunCommandInPane(
                                PaneId::Terminal(active_terminal_id),
                                command,
                            ))
                            .with_context(err_context)?;
                        should_update_ui = true;
                    },
                    Some(AdjustedInput::CloseThisPane) => {
                        self.close_pane(PaneId::Terminal(active_terminal_id), false);
                        should_update_ui = true;
                    },
                    Some(AdjustedInput::DropToShellInThisPane { working_dir }) => {
                        self.pids_waiting_resize.insert(active_terminal_id);
                        self.senders
                            .send_to_pty(PtyInstruction::DropToShellInPane {
                                pane_id: PaneId::Terminal(active_terminal_id),
                                shell: Some(self.default_shell.clone()),
                                working_dir,
                            })
                            .with_context(err_context)?;
                        should_update_ui = true;
                    },
                    Some(_) => {},
                    None => {},
                }
            },
            PaneId::Plugin(pid) => match active_pane.adjust_input_to_terminal(
                key_with_modifier,
                raw_input_bytes,
                raw_input_bytes_are_kitty,
                client_id,
            ) {
                Some(AdjustedInput::WriteKeyToPlugin(key_with_modifier)) => {
                    self.senders
                        .send_to_plugin(PluginInstruction::Update(vec![(
                            Some(pid),
                            client_id,
                            Event::Key(key_with_modifier),
                        )]))
                        .with_context(err_context)?;
                },
                Some(AdjustedInput::WriteBytesToTerminal(adjusted_input)) => {
                    let mut plugin_updates = vec![];
                    for key in parse_keys(&adjusted_input) {
                        plugin_updates.push((Some(pid), client_id, Event::Key(key)));
                    }
                    self.senders
                        .send_to_plugin(PluginInstruction::Update(plugin_updates))
                        .with_context(err_context)?;
                },
                Some(AdjustedInput::PermissionRequestResult(permissions, status)) => {
                    if active_pane.query_should_be_suppressed() {
                        active_pane.set_should_be_suppressed(false);
                        self.suppress_pane(PaneId::Plugin(pid), client_id);
                    }
                    self.request_plugin_permissions(pid, None);
                    self.senders
                        .send_to_plugin(PluginInstruction::PermissionRequestResult(
                            pid,
                            client_id,
                            permissions,
                            status,
                            None,
                        ))
                        .with_context(err_context)?;
                    should_update_ui = true;
                },
                Some(_) => {},
                None => {},
            },
        }
        Ok(should_update_ui)
    }
    pub fn write_to_pane_id_without_preprocessing(
        &mut self,
        raw_input_bytes: Vec<u8>,
        pane_id: PaneId,
    ) -> Result<bool> {
        // returns true if we need to update the UI (eg. when a command pane is closed with ctrl-c)
        let err_context = || format!("failed to write to pane with id {pane_id:?}");

        let mut should_update_ui = false;

        match pane_id {
            PaneId::Terminal(active_terminal_id) => {
                self.senders
                    .send_to_pty_writer(PtyWriteInstruction::Write(
                        raw_input_bytes,
                        active_terminal_id,
                    ))
                    .with_context(err_context)?;
                should_update_ui = true;
            },
            PaneId::Plugin(_pid) => {
                log::error!("Unsupported plugin action");
            },
        }
        Ok(should_update_ui)
    }
    pub fn active_terminal_is_mid_frame(&self, client_id: ClientId) -> Option<bool> {
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
        Some(active_terminal.is_mid_frame())
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
    pub fn toggle_pane_fullscreen(&mut self, pane_id: PaneId) {
        if self.tiled_panes.panes_contain(&pane_id) {
            self.tiled_panes.toggle_pane_fullscreen(pane_id);
        } else {
            log::error!("No tiled pane with id: {:?} found", pane_id);
        }
    }
    pub fn is_fullscreen_active(&self) -> bool {
        self.tiled_panes.fullscreen_is_active()
    }
    pub fn are_floating_panes_visible(&self) -> bool {
        self.floating_panes.panes_are_visible()
    }
    pub fn focus_pane_left_fullscreen(&mut self, client_id: ClientId) -> bool {
        if !self.is_fullscreen_active() {
            return false;
        }

        return self.tiled_panes.focus_pane_left_fullscreen(client_id);
    }
    pub fn focus_pane_right_fullscreen(&mut self, client_id: ClientId) -> bool {
        if !self.is_fullscreen_active() {
            return false;
        }

        return self.tiled_panes.focus_pane_right_fullscreen(client_id);
    }
    pub fn focus_pane_up_fullscreen(&mut self, client_id: ClientId) {
        if !self.is_fullscreen_active() {
            return;
        }

        self.tiled_panes.focus_pane_up_fullscreen(client_id);
    }
    pub fn focus_pane_down_fullscreen(&mut self, client_id: ClientId) {
        if !self.is_fullscreen_active() {
            return;
        }

        self.tiled_panes.focus_pane_down_fullscreen(client_id);
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
        self.tiled_panes.switch_prev_pane_fullscreen(client_id);
    }
    pub fn set_force_render(&mut self) {
        self.tiled_panes.set_force_render();
        self.floating_panes.set_force_render();
    }
    pub fn set_should_clear_display_before_rendering(&mut self) {
        self.should_clear_display_before_rendering = true;
        self.floating_panes.set_force_render(); // we do this to make sure pinned panes are
                                                // rendered even if their surface is not visible
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
    fn update_active_panes_in_pty_thread(&self) -> Result<()> {
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
                .with_context(|| format!("failed to update active pane for client {client_id}"))?;
        }
        Ok(())
    }

    pub fn render(&mut self, output: &mut Output) -> Result<()> {
        let err_context = || "failed to render tab".to_string();

        let connected_clients: HashSet<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        if connected_clients.is_empty() || !self.tiled_panes.has_active_panes() {
            return Ok(());
        }
        self.update_active_panes_in_pty_thread()
            .with_context(err_context)?;

        let floating_panes_stack = self.floating_panes.stack();
        output.add_clients(
            &connected_clients,
            self.link_handler.clone(),
            floating_panes_stack,
        );

        let current_pane_group: HashMap<ClientId, Vec<PaneId>> =
            { self.current_pane_group.borrow().clone_inner() };
        self.tiled_panes
            .render(
                output,
                self.floating_panes.panes_are_visible(),
                &self.mouse_hover_pane_id,
                current_pane_group.clone(),
            )
            .with_context(err_context)?;
        if (self.floating_panes.panes_are_visible() && self.floating_panes.has_active_panes())
            || self.floating_panes.has_pinned_panes()
        {
            self.floating_panes
                .render(output, &self.mouse_hover_pane_id, current_pane_group)
                .with_context(err_context)?;
        }

        self.render_cursor(output);
        if output.has_rendered_assets() {
            self.hide_cursor_and_clear_display_as_needed(output);
        }

        Ok(())
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
    fn render_cursor(&mut self, output: &mut Output) {
        let connected_clients: Vec<ClientId> =
            { self.connected_clients.borrow().iter().copied().collect() };
        for client_id in connected_clients {
            match self
                .get_active_terminal_cursor_position(client_id)
                .and_then(|(cursor_position_x, cursor_position_y)| {
                    // TODO: get active_pane_z_index and pass it to cursor_is_visible so we do the
                    // right thing if the cursor is in a floating pane
                    if self.floating_panes.panes_are_visible() {
                        Some((cursor_position_x, cursor_position_y))
                    } else if output.cursor_is_visible(cursor_position_x, cursor_position_y) {
                        Some((cursor_position_x, cursor_position_y))
                    } else {
                        None
                    }
                }) {
                Some((cursor_position_x, cursor_position_y)) => {
                    let desired_cursor_shape = self
                        .get_active_pane(client_id)
                        .map(|ap| ap.cursor_shape_csi())
                        .unwrap_or_default();
                    let cursor_changed_position_or_shape = self
                        .cursor_positions_and_shape
                        .get(&client_id)
                        .map(|(previous_x, previous_y, previous_shape)| {
                            previous_x != &cursor_position_x
                                || previous_y != &cursor_position_y
                                || previous_shape != &desired_cursor_shape
                        })
                        .unwrap_or(true);
                    let active_terminal_is_mid_frame = self
                        .active_terminal_is_mid_frame(client_id)
                        .unwrap_or(false);

                    if active_terminal_is_mid_frame {
                        // no-op, this means the active terminal is currently rendering a frame,
                        // which means the cursor can be jumping around and we definitely do not
                        // want to render it
                        //
                        // (I felt this was clearer than expanding the if conditional below)
                    } else if output.is_dirty() || cursor_changed_position_or_shape {
                        let show_cursor = "\u{1b}[?25h";
                        let goto_cursor_position = &format!(
                            "\u{1b}[{};{}H\u{1b}[m{}",
                            cursor_position_y + 1,
                            cursor_position_x + 1,
                            desired_cursor_shape
                        ); // goto row/col
                        output.add_post_vte_instruction_to_client(client_id, show_cursor);
                        output.add_post_vte_instruction_to_client(client_id, goto_cursor_position);
                        self.cursor_positions_and_shape.insert(
                            client_id,
                            (cursor_position_x, cursor_position_y, desired_cursor_shape),
                        );
                    }
                },
                None => {
                    let hide_cursor = "\u{1b}[?25l";
                    output.add_post_vte_instruction_to_client(client_id, hide_cursor);
                },
            }
        }
    }
    pub(crate) fn get_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.tiled_panes.get_panes()
    }
    pub(crate) fn get_floating_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.floating_panes.get_panes()
    }
    pub(crate) fn get_suppressed_panes(
        &self,
    ) -> impl Iterator<Item = (&PaneId, &(bool, Box<dyn Pane>))> {
        // bool => is_scrollback_editor
        self.suppressed_panes.iter()
    }
    fn get_selectable_tiled_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.get_tiled_panes().filter(|(_, p)| p.selectable())
    }
    fn get_selectable_floating_panes(&self) -> impl Iterator<Item = (&PaneId, &Box<dyn Pane>)> {
        self.get_floating_panes().filter(|(_, p)| p.selectable())
    }
    pub fn get_selectable_tiled_panes_count(&self) -> usize {
        self.get_selectable_tiled_panes().count()
    }
    pub fn get_selectable_floating_panes_count(&self) -> usize {
        self.get_selectable_floating_panes().count()
    }
    pub fn get_visible_selectable_floating_panes_count(&self) -> usize {
        if self.are_floating_panes_visible() {
            self.get_selectable_floating_panes().count()
        } else {
            0
        }
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
    pub fn resize_whole_tab(&mut self, new_screen_size: Size) -> Result<()> {
        let err_context = || format!("failed to resize whole tab (index {})", self.index);
        self.floating_panes.resize(new_screen_size);
        // we need to do this explicitly because floating_panes.resize does not do this
        self.floating_panes
            .resize_pty_all_panes(&mut self.os_api)
            .with_context(err_context)?;
        self.tiled_panes.resize(new_screen_size);
        if self.auto_layout && !self.swap_layouts.is_floating_damaged() {
            // we do this only for floating panes, because the constraint system takes care of the
            // tiled panes
            self.swap_layouts.set_is_floating_damaged();
            let _ = self.relayout_floating_panes(false);
        }
        if self.auto_layout && !self.swap_layouts.is_tiled_damaged() && !self.is_fullscreen_active()
        {
            self.swap_layouts.set_is_tiled_damaged();
            let _ = self.relayout_tiled_panes(false);
        }
        self.set_should_clear_display_before_rendering();
        self.senders
            .send_to_pty_writer(PtyWriteInstruction::ApplyCachedResizes)
            .with_context(|| format!("failed to update plugins with mode info"))?;
        Ok(())
    }
    pub fn resize(&mut self, client_id: ClientId, strategy: ResizeStrategy) -> Result<()> {
        let err_context = || format!("unable to resize pane");
        if self.floating_panes.panes_are_visible() {
            let successfully_resized = self
                .floating_panes
                .resize_active_pane(client_id, &mut self.os_api, &strategy)
                .with_context(err_context)?;
            if successfully_resized {
                self.swap_layouts.set_is_floating_damaged();
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else {
            match self.tiled_panes.resize_active_pane(client_id, &strategy) {
                Ok(_) => {
                    self.swap_layouts.set_is_tiled_damaged();
                },
                Err(err) => match err.downcast_ref::<ZellijError>() {
                    Some(ZellijError::CantResizeFixedPanes { pane_ids }) => {
                        let mut pane_ids_to_error = vec![];
                        for (id, is_terminal) in pane_ids {
                            if *is_terminal {
                                pane_ids_to_error.push(PaneId::Terminal(*id));
                            } else {
                                pane_ids_to_error.push(PaneId::Plugin(*id));
                            };
                        }
                        self.senders
                            .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                                pane_ids_to_error,
                                "FIXED!".into(),
                            ))
                            .with_context(err_context)?;
                    },
                    _ => Err::<(), _>(err).non_fatal(),
                },
            }
        }
        Ok(())
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
    pub fn focus_pane_on_edge(&mut self, direction: Direction, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.focus_pane_on_edge(direction, client_id);
        } else if self.has_selectable_panes() && !self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.focus_pane_on_edge(direction, client_id);
        }
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_left(&mut self, client_id: ClientId) -> Result<bool> {
        let err_context = || format!("failed to move focus left for client {}", client_id);

        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus(
                    client_id,
                    &self.connected_clients.borrow().iter().copied().collect(),
                    &Direction::Left,
                )
                .with_context(err_context)
        } else {
            if !self.has_selectable_panes() {
                return Ok(false);
            }
            if self.tiled_panes.fullscreen_is_active() {
                return Ok(self.focus_pane_left_fullscreen(client_id));
            }
            Ok(self.tiled_panes.move_focus_left(client_id))
        }
    }
    pub fn move_focus_down(&mut self, client_id: ClientId) -> Result<bool> {
        let err_context = || format!("failed to move focus down for client {}", client_id);

        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus(
                    client_id,
                    &self.connected_clients.borrow().iter().copied().collect(),
                    &Direction::Down,
                )
                .with_context(err_context)
        } else {
            if !self.has_selectable_panes() {
                return Ok(false);
            }
            if self.tiled_panes.fullscreen_is_active() {
                self.focus_pane_down_fullscreen(client_id);
                return Ok(true);
            }
            Ok(self.tiled_panes.move_focus_down(client_id))
        }
    }
    pub fn move_focus_up(&mut self, client_id: ClientId) -> Result<bool> {
        let err_context = || format!("failed to move focus up for client {}", client_id);

        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus(
                    client_id,
                    &self.connected_clients.borrow().iter().copied().collect(),
                    &Direction::Up,
                )
                .with_context(err_context)
        } else {
            if !self.has_selectable_panes() {
                return Ok(false);
            }
            if self.tiled_panes.fullscreen_is_active() {
                self.focus_pane_up_fullscreen(client_id);
                return Ok(true);
            }
            Ok(self.tiled_panes.move_focus_up(client_id))
        }
    }
    // returns a boolean that indicates whether the focus moved
    pub fn move_focus_right(&mut self, client_id: ClientId) -> Result<bool> {
        let err_context = || format!("failed to move focus right for client {}", client_id);

        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_focus(
                    client_id,
                    &self.connected_clients.borrow().iter().copied().collect(),
                    &Direction::Right,
                )
                .with_context(err_context)
        } else {
            if !self.has_selectable_panes() {
                return Ok(false);
            }
            if self.tiled_panes.fullscreen_is_active() {
                return Ok(self.focus_pane_right_fullscreen(client_id));
            }
            Ok(self.tiled_panes.move_focus_right(client_id))
        }
    }
    pub fn move_active_pane(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        let search_backwards = false;
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_active_pane(search_backwards, &mut self.os_api, client_id);
        } else {
            self.tiled_panes
                .move_active_pane(search_backwards, client_id);
        }
    }
    pub fn move_pane(&mut self, pane_id: PaneId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        let search_backwards = false;
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_pane(search_backwards, pane_id);
        } else {
            self.tiled_panes.move_pane(search_backwards, pane_id);
        }
    }
    pub fn move_active_pane_backwards(&mut self, client_id: ClientId) {
        if !self.has_selectable_panes() {
            return;
        }
        if self.tiled_panes.fullscreen_is_active() {
            return;
        }
        let search_backwards = true;
        if self.floating_panes.panes_are_visible() {
            self.floating_panes
                .move_active_pane(search_backwards, &mut self.os_api, client_id);
        } else {
            self.tiled_panes
                .move_active_pane(search_backwards, client_id);
        }
    }
    pub fn move_active_pane_down(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_down(client_id);
            self.swap_layouts.set_is_floating_damaged();
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
    pub fn move_pane_down(&mut self, pane_id: PaneId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_pane_down(pane_id);
            self.swap_layouts.set_is_floating_damaged();
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_pane_down(pane_id);
        }
    }
    pub fn move_active_pane_up(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_up(client_id);
            self.swap_layouts.set_is_floating_damaged();
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
    pub fn move_pane_up(&mut self, pane_id: PaneId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_pane_up(pane_id);
            self.swap_layouts.set_is_floating_damaged();
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_pane_up(pane_id);
        }
    }
    pub fn move_active_pane_right(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_right(client_id);
            self.swap_layouts.set_is_floating_damaged();
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
    pub fn move_pane_right(&mut self, pane_id: PaneId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_pane_right(pane_id);
            self.swap_layouts.set_is_floating_damaged();
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_pane_right(pane_id);
        }
    }
    pub fn move_active_pane_left(&mut self, client_id: ClientId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_active_pane_left(client_id);
            self.swap_layouts.set_is_floating_damaged();
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
    pub fn move_pane_left(&mut self, pane_id: PaneId) {
        if self.floating_panes.panes_are_visible() {
            self.floating_panes.move_pane_left(pane_id);
            self.swap_layouts.set_is_floating_damaged();
            self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" behind
        } else {
            if !self.has_selectable_panes() {
                return;
            }
            if self.tiled_panes.fullscreen_is_active() {
                return;
            }
            self.tiled_panes.move_pane_left(pane_id);
        }
    }
    fn close_down_to_max_terminals(&mut self) -> Result<()> {
        if let Some(max_panes) = self.max_panes {
            let terminals = self.get_tiled_pane_ids();
            for &pid in terminals.iter().skip(max_panes - 1) {
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(pid))
                    .context("failed to close down to max terminals")?;
                self.close_pane(pid, false);
            }
        }
        Ok(())
    }
    pub fn get_tiled_pane_ids(&self) -> Vec<PaneId> {
        self.get_tiled_panes().map(|(&pid, _)| pid).collect()
    }
    pub fn get_all_pane_ids(&self) -> Vec<PaneId> {
        let mut static_and_floating_pane_ids = self.get_static_and_floating_pane_ids();
        let mut suppressed_pane_ids = self
            .suppressed_panes
            .values()
            .map(|(_key, pane)| pane.pid())
            .collect();
        static_and_floating_pane_ids.append(&mut suppressed_pane_ids);
        static_and_floating_pane_ids
    }
    pub fn get_static_and_floating_pane_ids(&self) -> Vec<PaneId> {
        self.tiled_panes
            .pane_ids()
            .chain(self.floating_panes.pane_ids())
            .copied()
            .collect()
    }
    pub fn set_pane_selectable(&mut self, id: PaneId, selectable: bool) {
        if self.is_pending {
            self.pending_instructions
                .push(BufferedTabInstruction::SetPaneSelectable(id, selectable));
            return;
        }
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
        } else if let Some(pane) = self.floating_panes.get_pane_mut(id) {
            pane.set_selectable(selectable);
        }
        // we do this here because if there is a non-selectable pane on the edge, we consider it
        // outside the viewport (a ui-pane, eg. the status-bar and tab-bar) and need to adjust for it
        LayoutApplier::offset_viewport(
            self.viewport.clone(),
            &mut self.tiled_panes,
            self.draw_pane_frames,
        );
    }
    pub fn set_mouse_selection_support(&mut self, pane_id: PaneId, selection_support: bool) {
        if let Some(pane) = self.get_pane_with_id_mut(pane_id) {
            pane.set_mouse_selection_support(selection_support);
        }
    }
    pub fn close_pane(&mut self, id: PaneId, ignore_suppressed_panes: bool) {
        // we need to ignore suppressed panes when we toggle a pane to be floating/embedded(tiled)
        // this is because in that case, while we do use this logic, we're not actually closing the
        // pane, we're moving it
        if !ignore_suppressed_panes && self.suppressed_panes.contains_key(&id) {
            return match self.replace_pane_with_suppressed_pane(id) {
                Ok(_pane) => {},
                Err(e) => {
                    Err::<(), _>(e)
                        .with_context(|| format!("failed to close pane {:?}", id))
                        .non_fatal();
                },
            };
        }
        if self.floating_panes.panes_contain(&id) {
            let _closed_pane = self.floating_panes.remove_pane(id);
            self.floating_panes.move_clients_out_of_pane(id);
            if !self.floating_panes.has_selectable_panes() {
                self.swap_layouts.reset_floating_damage();
                self.hide_floating_panes();
            }
            self.set_force_render();
            self.floating_panes.set_force_render();
            if self.auto_layout
                && !self.swap_layouts.is_floating_damaged()
                && self.floating_panes.visible_panes_count() > 0
            {
                self.swap_layouts.set_is_floating_damaged();
                // only relayout if the user is already "in" a layout, otherwise this might be
                // confusing
                let _ = self.relayout_floating_panes(false);
            }
        } else {
            if self.tiled_panes.fullscreen_is_active() {
                self.tiled_panes.unset_fullscreen();
            }
            let _closed_pane = self.tiled_panes.remove_pane(id);
            self.set_force_render();
            self.tiled_panes.set_force_render();
            if self.auto_layout && !self.swap_layouts.is_tiled_damaged() {
                self.swap_layouts.set_is_tiled_damaged();
                // only relayout if the user is already "in" a layout, otherwise this might be
                // confusing
                let _ = self.relayout_tiled_panes(false);
            }
        };
        let _ = self.senders.send_to_plugin(PluginInstruction::Update(vec![(
            None,
            None,
            Event::PaneClosed(id.into()),
        )]));
    }
    pub fn extract_pane(
        &mut self,
        id: PaneId,
        dont_swap_if_suppressed: bool,
    ) -> Option<Box<dyn Pane>> {
        if !dont_swap_if_suppressed && self.suppressed_panes.contains_key(&id) {
            // this is done for the scrollback editor
            return match self.replace_pane_with_suppressed_pane(id) {
                Ok(mut pane) => {
                    // we do this so that the logical index will not affect ordering in the target tab
                    if let Some(pane) = pane.as_mut() {
                        pane.reset_logical_position();
                    }
                    pane
                },
                Err(e) => {
                    Err::<(), _>(e)
                        .with_context(|| format!("failed to close pane {:?}", id))
                        .non_fatal();
                    None
                },
            };
        }
        if self.floating_panes.panes_contain(&id) {
            let mut closed_pane = self.floating_panes.remove_pane(id);
            self.floating_panes.move_clients_out_of_pane(id);
            if !self.floating_panes.has_panes() {
                self.swap_layouts.reset_floating_damage();
                self.hide_floating_panes();
            }
            self.set_force_render();
            self.floating_panes.set_force_render();
            if self.auto_layout
                && !self.swap_layouts.is_floating_damaged()
                && self.floating_panes.visible_panes_count() > 0
            {
                self.swap_layouts.set_is_floating_damaged();
                // only relayout if the user is already "in" a layout, otherwise this might be
                // confusing
                let _ = self.relayout_floating_panes(false);
            }
            // we do this so that the logical index will not affect ordering in the target tab
            if let Some(closed_pane) = closed_pane.as_mut() {
                closed_pane.reset_logical_position();
            }
            closed_pane
        } else if self.tiled_panes.panes_contain(&id) {
            if self.tiled_panes.fullscreen_is_active() {
                self.tiled_panes.unset_fullscreen();
            }
            let mut closed_pane = self.tiled_panes.remove_pane(id);
            self.set_force_render();
            self.tiled_panes.set_force_render();
            if self.auto_layout && !self.swap_layouts.is_tiled_damaged() {
                self.swap_layouts.set_is_tiled_damaged();
                // only relayout if the user is already "in" a layout, otherwise this might be
                // confusing
                let _ = self.relayout_tiled_panes(false);
            }
            // we do this so that the logical index will not affect ordering in the target tab
            if let Some(closed_pane) = closed_pane.as_mut() {
                closed_pane.reset_logical_position();
            }
            closed_pane
        } else if self.suppressed_panes.contains_key(&id) {
            self.suppressed_panes.remove(&id).map(|s_p| s_p.1)
        } else {
            None
        }
    }
    pub fn hold_pane(
        &mut self,
        id: PaneId,
        exit_status: Option<i32>,
        is_first_run: bool,
        run_command: RunCommand,
    ) {
        if self.is_pending {
            self.pending_instructions
                .push(BufferedTabInstruction::HoldPane(
                    id,
                    exit_status,
                    is_first_run,
                    run_command,
                ));
            return;
        }
        if self.floating_panes.panes_contain(&id) {
            self.floating_panes
                .hold_pane(id, exit_status, is_first_run, run_command);
        } else if self.tiled_panes.panes_contain(&id) {
            self.tiled_panes
                .hold_pane(id, exit_status, is_first_run, run_command);
        } else if let Some(pane) = self.suppressed_panes.values_mut().find(|p| p.1.pid() == id) {
            pane.1.hold(exit_status, is_first_run, run_command);
        }
    }
    pub fn replace_pane_with_suppressed_pane(
        &mut self,
        pane_id: PaneId,
    ) -> Result<Option<Box<dyn Pane>>> {
        self.suppressed_panes
            .remove(&pane_id)
            .with_context(|| {
                format!(
                    "couldn't find pane with id {:?} in suppressed panes",
                    pane_id
                )
            })
            .and_then(|(_is_scrollback_editor, suppressed_pane)| {
                let suppressed_pane_id = suppressed_pane.pid();
                let replaced_pane = if self.are_floating_panes_visible() {
                    Some(self.floating_panes.replace_pane(pane_id, suppressed_pane)).transpose()?
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
                    resize_pty!(
                        suppressed_pane,
                        self.os_api,
                        self.senders,
                        self.character_cell_size
                    )?;
                }
                Ok(replaced_pane)
            })
            .with_context(|| {
                format!(
                    "failed to replace active pane with suppressed pane {:?}",
                    pane_id
                )
            })
    }
    pub fn close_focused_pane(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = |pane_id| {
            format!("failed to close focused pane (ID {pane_id:?}) for client {client_id}")
        };

        if self.floating_panes.panes_are_visible() {
            if let Some(active_floating_pane_id) = self.floating_panes.active_pane_id(client_id) {
                self.close_pane(active_floating_pane_id, false);
                self.senders
                    .send_to_pty(PtyInstruction::ClosePane(active_floating_pane_id))
                    .with_context(|| err_context(active_floating_pane_id))?;
                return Ok(());
            }
        }
        if let Some(active_pane_id) = self.tiled_panes.get_active_pane_id(client_id) {
            self.close_pane(active_pane_id, false);
            self.senders
                .send_to_pty(PtyInstruction::ClosePane(active_pane_id))
                .with_context(|| err_context(active_pane_id))?;
        }
        Ok(())
    }
    pub fn clear_active_terminal_screen(&mut self, client_id: ClientId) -> Result<()> {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_screen();
        }
        Ok(())
    }
    pub fn clear_screen_for_pane_id(&mut self, pane_id: PaneId) {
        if let Some(pane) = self.get_pane_with_id_mut(pane_id) {
            pane.clear_screen();
        }
    }
    pub fn dump_active_terminal_screen(
        &mut self,
        file: Option<String>,
        client_id: ClientId,
        full: bool,
    ) -> Result<()> {
        let err_context =
            || format!("failed to dump active terminal screen for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let dump = active_pane.dump_screen(full, Some(client_id));
            self.os_api
                .write_to_file(dump, file)
                .with_context(err_context)?;
        }
        Ok(())
    }
    pub fn dump_terminal_screen(
        &mut self,
        file: Option<String>,
        pane_id: PaneId,
        full: bool,
    ) -> Result<()> {
        if let Some(pane) = self.get_pane_with_id(pane_id) {
            let dump = pane.dump_screen(full, None);
            self.os_api.write_to_file(dump, file).non_fatal()
        }
        Ok(())
    }
    pub fn edit_scrollback(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to edit scrollback for client {client_id}");

        let mut file = temp_dir();
        file.push(format!("{}.dump", Uuid::new_v4()));
        self.dump_active_terminal_screen(
            Some(String::from(file.to_string_lossy())),
            client_id,
            true,
        )
        .with_context(err_context)?;
        let line_number = self
            .get_active_pane(client_id)
            .and_then(|a_t| a_t.get_line_number());
        self.senders
            .send_to_pty(PtyInstruction::OpenInPlaceEditor(
                file,
                line_number,
                ClientTabIndexOrPaneId::ClientId(client_id),
            ))
            .with_context(err_context)
    }
    pub fn edit_scrollback_for_pane_with_id(&mut self, pane_id: PaneId) -> Result<()> {
        if let PaneId::Terminal(_terminal_pane_id) = pane_id {
            let mut file = temp_dir();
            file.push(format!("{}.dump", Uuid::new_v4()));
            self.dump_terminal_screen(Some(String::from(file.to_string_lossy())), pane_id, true)
                .non_fatal();
            let line_number = self
                .get_pane_with_id(pane_id)
                .and_then(|a_t| a_t.get_line_number());
            self.senders.send_to_pty(PtyInstruction::OpenInPlaceEditor(
                file,
                line_number,
                ClientTabIndexOrPaneId::PaneId(pane_id),
            ))
        } else {
            log::error!("Editing plugin pane scrollback is currently unsupported.");
            Ok(())
        }
    }
    pub fn scroll_active_terminal_up(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.scroll_up(1, client_id);
        }
    }

    pub fn scroll_terminal_up(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            let fictitious_client_id = 1; // this is not checked for terminal panes and we
                                          // don't have an actual client id here
                                          // TODO: traits were a mistake
            terminal_pane.scroll_up(1, fictitious_client_id);
        }
    }

    pub fn scroll_active_terminal_down(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to scroll down active pane for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.scroll_down(1, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }

    pub fn scroll_terminal_down(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            let fictitious_client_id = 1; // this is not checked for terminal panes and we
                                          // don't have an actual client id here
                                          // TODO: traits were a mistake
            terminal_pane.scroll_down(1, fictitious_client_id);
        }
    }

    pub fn scroll_active_terminal_up_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // prevent overflow when row == 0
            let scroll_rows = active_pane.rows().max(1).saturating_sub(1);
            active_pane.scroll_up(scroll_rows, client_id);
        }
    }

    pub fn scroll_terminal_page_up(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            let fictitious_client_id = 1; // this is not checked for terminal panes and we
                                          // don't have an actual client id here
                                          // TODO: traits were a mistake
                                          // prevent overflow when row == 0
            let scroll_rows = terminal_pane.rows().max(1).saturating_sub(1);
            terminal_pane.scroll_up(scroll_rows, fictitious_client_id);
        }
    }

    pub fn scroll_active_terminal_down_page(&mut self, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to scroll down one page in active pane for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let scroll_rows = active_pane.get_content_rows();
            active_pane.scroll_down(scroll_rows, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }

    pub fn scroll_terminal_page_down(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            let fictitious_client_id = 1; // this is not checked for terminal panes and we
                                          // don't have an actual client id here
                                          // TODO: traits were a mistake
            let scroll_rows = terminal_pane.get_content_rows();
            terminal_pane.scroll_down(scroll_rows, fictitious_client_id);
            if !terminal_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = terminal_pane.pid() {
                    self.process_pending_vte_events(raw_fd).non_fatal()
                }
            }
        }
    }

    pub fn scroll_active_terminal_up_half_page(&mut self, client_id: ClientId) {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // prevent overflow when row == 0
            let scroll_rows = (active_pane.rows().max(1).saturating_sub(1)) / 2;
            active_pane.scroll_up(scroll_rows, client_id);
        }
    }

    pub fn scroll_active_terminal_down_half_page(&mut self, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to scroll down half a page in active pane for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            let scroll_rows = (active_pane.rows().max(1) - 1) / 2;
            active_pane.scroll_down(scroll_rows, client_id);
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }

    pub fn scroll_active_terminal_to_bottom(&mut self, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to scroll to bottom in active pane for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_scroll();
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }

    pub fn scroll_terminal_to_bottom(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            terminal_pane.clear_scroll();
            if !terminal_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = terminal_pane.pid() {
                    self.process_pending_vte_events(raw_fd).non_fatal();
                }
            }
        }
    }

    pub fn scroll_active_terminal_to_top(&mut self, client_id: ClientId) -> Result<()> {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_scroll();
            if let Some(size) = active_pane.get_line_number() {
                active_pane.scroll_up(size, client_id);
            }
        }
        Ok(())
    }

    pub fn scroll_terminal_to_top(&mut self, terminal_pane_id: u32) {
        if let Some(terminal_pane) = self.get_pane_with_id_mut(PaneId::Terminal(terminal_pane_id)) {
            terminal_pane.clear_scroll();
            if let Some(size) = terminal_pane.get_line_number() {
                let fictitious_client_id = 1; // this is not checked for terminal panes and we
                                              // don't have an actual client id here
                                              // TODO: traits were a mistake
                terminal_pane.scroll_up(size, fictitious_client_id);
            }
        }
    }

    pub fn clear_active_terminal_scroll(&mut self, client_id: ClientId) -> Result<()> {
        // TODO: is this a thing?
        let err_context =
            || format!("failed to clear scroll in active pane for client {client_id}");

        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            active_pane.clear_scroll();
            if !active_pane.is_scrolled() {
                if let PaneId::Terminal(raw_fd) = active_pane.pid() {
                    self.process_pending_vte_events(raw_fd)
                        .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_scrollwheel_up(
        &mut self,
        point: &Position,
        lines: usize,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!("failed to handle scrollwheel up at position {point:?} for client {client_id}")
        };

        if let Some(pane) = self.get_pane_at(point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_up(&relative_position) {
                self.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send UP n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    self.write_to_terminal_at("\u{1b}[A".as_bytes().to_owned(), point, client_id)
                        .with_context(err_context)?;
                }
            } else {
                pane.scroll_up(lines, client_id);
            }
        }
        Ok(MouseEffect::default())
    }

    pub fn handle_scrollwheel_down(
        &mut self,
        point: &Position,
        lines: usize,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || {
            format!(
                "failed to handle scrollwheel down at position {point:?} for client {client_id}"
            )
        };

        if let Some(pane) = self.get_pane_at(point, false).with_context(err_context)? {
            let relative_position = pane.relative_position(point);
            if let Some(mouse_event) = pane.mouse_scroll_down(&relative_position) {
                self.write_to_terminal_at(mouse_event.into_bytes(), point, client_id)
                    .with_context(err_context)?;
            } else if pane.is_alternate_mode_active() {
                // faux scrolling, send DOWN n times
                // do n separate writes to make sure the sequence gets adjusted for cursor keys mode
                for _ in 0..lines {
                    self.write_to_terminal_at("\u{1b}[B".as_bytes().to_owned(), point, client_id)
                        .with_context(err_context)?;
                }
            } else {
                pane.scroll_down(lines, client_id);
                if !pane.is_scrolled() {
                    if let PaneId::Terminal(pid) = pane.pid() {
                        self.process_pending_vte_events(pid)
                            .with_context(err_context)?;
                    }
                }
            }
        }
        Ok(MouseEffect::default())
    }

    fn get_pane_at(
        &mut self,
        point: &Position,
        search_selectable: bool,
    ) -> Result<Option<&mut Box<dyn Pane>>> {
        let err_context = || format!("failed to get pane at position {point:?}");

        if self.floating_panes.panes_are_visible() {
            if let Some(pane_id) = self
                .floating_panes
                .get_pane_id_at(point, search_selectable)
                .with_context(err_context)?
            {
                return Ok(self.floating_panes.get_pane_mut(pane_id));
            }
        } else if self.floating_panes.has_pinned_panes() {
            if let Some(pane_id) = self
                .floating_panes
                .get_pinned_pane_id_at(point, search_selectable)
                .with_context(err_context)?
            {
                return Ok(self.floating_panes.get_pane_mut(pane_id));
            }
        }
        if let Some(pane_id) = self
            .get_pane_id_at(point, search_selectable)
            .with_context(err_context)?
        {
            Ok(self.tiled_panes.get_pane_mut(pane_id))
        } else {
            Ok(None)
        }
    }

    fn get_pane_id_at(
        &mut self,
        point: &Position,
        search_selectable: bool,
    ) -> Result<Option<PaneId>> {
        let err_context = || format!("failed to get id of pane at position {point:?}");

        if self.tiled_panes.fullscreen_is_active()
            && self
                .is_position_inside_viewport(point)
                .with_context(err_context)?
        {
            // TODO: instead of doing this, record the pane that is in fullscreen
            let first_client_id = self
                .connected_clients
                .borrow()
                .iter()
                .copied()
                .next()
                .with_context(err_context)?;
            return Ok(self.tiled_panes.get_active_pane_id(first_client_id));
        }

        let (stacked_pane_ids_under_flexible_pane, _stacked_pane_ids_over_flexible_pane) = {
            self.tiled_panes
                .stacked_pane_ids_under_and_over_flexible_panes()
                .with_context(err_context)?
        };
        let pane_contains_point = |p: &Box<dyn Pane>,
                                   point: &Position,
                                   stacked_pane_ids_under_flexible_pane: &HashSet<PaneId>|
         -> bool {
            let is_flexible_in_stack =
                p.current_geom().is_stacked() && !p.current_geom().rows.is_fixed();
            let is_stacked_under = stacked_pane_ids_under_flexible_pane.contains(&p.pid());
            let geom_to_compare_against = if is_stacked_under && !self.draw_pane_frames {
                // these sort of panes are one-liner panes under a flexible pane in a stack when we
                // don't draw pane frames - because the whole stack's content is offset to allow
                // room for the boundary between panes, they are actually drawn 1 line above where
                // they are
                let mut geom = p.current_geom();
                geom.y = geom.y.saturating_sub(p.get_content_offset().bottom);
                geom
            } else if is_flexible_in_stack && !self.draw_pane_frames {
                // these sorts of panes are flexible panes inside a stack when we don't draw pane
                // frames - because the whole stack's content is offset to give room for the
                // boundary between panes, we need to take this offset into account when figuring
                // out whether the position is inside them
                let mut geom = p.current_geom();
                geom.rows.decrease_inner(p.get_content_offset().bottom);
                geom
            } else {
                p.current_geom()
            };
            geom_to_compare_against.contains(point)
        };

        if search_selectable {
            Ok(self
                .get_selectable_tiled_panes()
                .find(|(_, p)| pane_contains_point(p, point, &stacked_pane_ids_under_flexible_pane))
                .map(|(&id, _)| id))
        } else {
            Ok(self
                .get_tiled_panes()
                .find(|(_, p)| pane_contains_point(p, point, &stacked_pane_ids_under_flexible_pane))
                .map(|(&id, _)| id))
        }
    }

    // returns true if the mouse event caused some sort of tab/pane state change that needs to be
    // reported to plugins
    pub fn handle_mouse_event(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");

        let active_pane_id = self
            .get_active_pane_id(client_id)
            .ok_or(anyhow!("Failed to find pane at position"))?;

        if event.left {
            // left mouse click
            let pane_id_at_position = self
                .get_pane_at(&event.position, false)
                .with_context(err_context)?
                .ok_or_else(|| anyhow!("Failed to find pane at position"))?
                .pid();
            match event.event_type {
                MouseEventType::Press if event.alt => {
                    self.mouse_hover_pane_id.remove(&client_id);
                    Ok(MouseEffect::group_toggle(pane_id_at_position))
                },
                MouseEventType::Motion if event.alt => {
                    Ok(MouseEffect::group_add(pane_id_at_position))
                },
                MouseEventType::Press => {
                    if pane_id_at_position == active_pane_id {
                        self.handle_active_pane_left_mouse_press(event, client_id)
                    } else {
                        self.handle_inactive_pane_left_mouse_press(event, client_id)
                    }
                },
                MouseEventType::Motion => self.handle_left_mouse_motion(event, client_id),
                MouseEventType::Release => self.handle_left_mouse_release(event, client_id),
            }
        } else if event.wheel_up {
            self.handle_scrollwheel_up(&event.position, 3, client_id)
        } else if event.wheel_down {
            self.handle_scrollwheel_down(&event.position, 3, client_id)
        } else if event.right && event.alt {
            self.mouse_hover_pane_id.remove(&client_id);
            Ok(MouseEffect::ungroup())
        } else if event.right {
            self.handle_right_click(&event, client_id)
        } else if event.middle {
            self.handle_middle_click(&event, client_id)
        } else {
            self.handle_mouse_no_click(&event, client_id)
        }
    }
    fn write_mouse_event_to_active_pane(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");
        let active_pane = self.get_active_pane_or_floating_pane_mut(client_id);
        if let Some(active_pane) = active_pane {
            let relative_position = active_pane.relative_position(&event.position);
            let mut pass_event = *event;
            pass_event.position = relative_position;
            if let Some(mouse_event) = active_pane.mouse_event(&pass_event, client_id) {
                if !active_pane.position_is_on_frame(&event.position) {
                    self.write_to_active_terminal(
                        &None,
                        mouse_event.into_bytes(),
                        false,
                        client_id,
                    )
                    .with_context(err_context)?;
                }
            }
        }
        Ok(())
    }
    // returns true if the mouse event caused some sort of tab/pane state change that needs to be
    // reported to plugins
    fn handle_active_pane_left_mouse_press(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");
        let floating_panes_are_visible = self.floating_panes.panes_are_visible();
        let pane_at_position = self
            .get_pane_at(&event.position, false)
            .with_context(err_context)?
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;
        if pane_at_position.position_is_on_frame(&event.position) {
            // intercept frame click eg. for toggling pinned
            let intercepted = pane_at_position.intercept_mouse_event_on_frame(&event, client_id);
            if intercepted {
                self.set_force_render();
                return Ok(MouseEffect::state_changed());
            } else if floating_panes_are_visible {
                // start moving if floating pane
                let search_selectable = false;
                if self
                    .floating_panes
                    .move_pane_with_mouse(event.position, search_selectable)
                {
                    self.swap_layouts.set_is_floating_damaged();
                    self.set_force_render();
                    return Ok(MouseEffect::state_changed());
                }
            }
        } else {
            let relative_position = pane_at_position.relative_position(&event.position);
            if let Some(mouse_event) = pane_at_position.mouse_left_click(&relative_position, false)
            {
                // send click to terminal if needed (eg. the program inside
                // requested mouse mode)
                if !pane_at_position.position_is_on_frame(&event.position) {
                    self.write_to_active_terminal(
                        &None,
                        mouse_event.into_bytes(),
                        false,
                        client_id,
                    )
                    .with_context(err_context)?;
                }
            } else {
                // start selection for copy/paste
                let mut leave_clipboard_message = false;
                pane_at_position.start_selection(&relative_position, client_id);
                if pane_at_position.get_selected_text(client_id).is_some() {
                    leave_clipboard_message = true;
                }
                if pane_at_position.supports_mouse_selection() {
                    self.selecting_with_mouse_in_pane = Some(pane_at_position.pid());
                }
                if leave_clipboard_message {
                    return Ok(MouseEffect::leave_clipboard_message());
                }
            }
        }
        Ok(MouseEffect::default())
    }
    fn handle_inactive_pane_left_mouse_press(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");
        if !self.floating_panes.panes_are_visible() {
            if let Ok(Some(pane_id)) = self
                .floating_panes
                .get_pinned_pane_id_at(&event.position, true)
            {
                // here, the floating panes are not visible, but there is a pinned pane (always
                // visible) that has been clicked on - so we make the entire surface visible and
                // focus it
                self.show_floating_panes();
                self.floating_panes.focus_pane(pane_id, client_id);
                return Ok(MouseEffect::state_changed());
            } else if let Ok(Some(_pane_id)) = self
                .floating_panes
                .get_pinned_pane_id_at(&event.position, false)
            {
                // here, the floating panes are not visible, but there is a pinned pane (always
                // visible) that has been clicked on - this pane however is not selectable
                // (we know this because we passed "false" to get_pinned_pane_id_at)
                // so we don't do anything
                return Ok(MouseEffect::default());
            }
        }
        let active_pane_id_before_click = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;
        self.focus_pane_at(&event.position, client_id)
            .with_context(err_context)?;

        if let Some(pane_at_position) = self.unselectable_pane_at_position(&event.position) {
            let relative_position = pane_at_position.relative_position(&event.position);
            // we use start_selection here because it has a client_id,
            // ideally we should add client_id to mouse_left_click and others, but this should be
            // dealt with as part of the trait removal refactoring
            pane_at_position.start_selection(&relative_position, client_id);
        }

        if self.floating_panes.panes_are_visible() {
            let search_selectable = false;
            // we do this because this might be the beginning of the user dragging a pane
            // that was not focused
            // TODO: rename move_pane_with_mouse to "start_moving_pane_with_mouse"?
            let moved_pane_with_mouse = self
                .floating_panes
                .move_pane_with_mouse(event.position, search_selectable);
            if moved_pane_with_mouse {
                return Ok(MouseEffect::state_changed());
            } else {
                return Ok(MouseEffect::default());
            }
        }
        let active_pane_id_after_click = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;
        if active_pane_id_before_click != active_pane_id_after_click {
            // focus changed, need to report it
            Ok(MouseEffect::state_changed())
        } else {
            Ok(MouseEffect::default())
        }
    }
    fn handle_left_mouse_motion(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");
        let pane_is_being_moved_with_mouse = self.floating_panes.pane_is_being_moved_with_mouse();
        let active_pane_id = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;
        if pane_is_being_moved_with_mouse {
            let search_selectable = false;
            if self
                .floating_panes
                .move_pane_with_mouse(event.position, search_selectable)
            {
                self.swap_layouts.set_is_floating_damaged();
                self.set_force_render();
                return Ok(MouseEffect::state_changed());
            }
        } else if let Some(pane_id_with_selection) = self.selecting_with_mouse_in_pane {
            if let Some(pane_with_selection) = self.get_pane_with_id_mut(pane_id_with_selection) {
                let relative_position = pane_with_selection.relative_position(&event.position);
                pane_with_selection.update_selection(&relative_position, client_id);
            }
        } else {
            let pane_at_position = self
                .get_pane_at(&event.position, false)
                .with_context(err_context)?
                .ok_or_else(|| anyhow!("Failed to find pane at position"))?;
            if pane_at_position.pid() == active_pane_id {
                self.write_mouse_event_to_active_pane(event, client_id)?;
            }
        }
        Ok(MouseEffect::default())
    }
    fn handle_left_mouse_release(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context =
            || format!("failed to handle mouse event {event:?} for client {client_id}");
        let mut leave_clipboard_message = false;
        let floating_panes_are_visible = self.floating_panes.panes_are_visible();
        let copy_on_release = self.copy_on_select;

        if let Some(pane_with_selection) = self
            .selecting_with_mouse_in_pane
            .and_then(|p_id| self.get_pane_with_id_mut(p_id))
        {
            let mut relative_position = pane_with_selection.relative_position(&event.position);

            relative_position.change_column(
                (relative_position.column())
                    .max(0)
                    .min(pane_with_selection.get_content_columns()),
            );

            relative_position.change_line(
                (relative_position.line())
                    .max(0)
                    .min(pane_with_selection.get_content_rows() as isize),
            );

            if let Some(mouse_event) =
                pane_with_selection.mouse_left_click_release(&relative_position)
            {
                self.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                    .with_context(err_context)?;
            } else {
                let relative_position = pane_with_selection.relative_position(&event.position);
                pane_with_selection.end_selection(&relative_position, client_id);
                if pane_with_selection.supports_mouse_selection() {
                    if copy_on_release {
                        let selected_text = pane_with_selection.get_selected_text(client_id);

                        if let Some(selected_text) = selected_text {
                            leave_clipboard_message = true;
                            self.write_selection_to_clipboard(&selected_text)
                                .with_context(err_context)?;
                        }
                    }
                }

                self.selecting_with_mouse_in_pane = None;
            }
        } else if floating_panes_are_visible && self.floating_panes.pane_is_being_moved_with_mouse()
        {
            self.floating_panes
                .stop_moving_pane_with_mouse(event.position);
        } else {
            let active_pane_id = self
                .get_active_pane_id(client_id)
                .ok_or(anyhow!("Failed to find pane at position"))?;
            let pane_id_at_position = self
                .get_pane_at(&event.position, false)
                .with_context(err_context)?
                .ok_or_else(|| anyhow!("Failed to find pane at position"))?
                .pid();
            if active_pane_id == pane_id_at_position {
                self.write_mouse_event_to_active_pane(event, client_id)?;
            }
        }
        if leave_clipboard_message {
            Ok(MouseEffect::leave_clipboard_message())
        } else {
            Ok(MouseEffect::default())
        }
    }

    pub fn handle_right_click(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to handle mouse right click for client {client_id}");

        let absolute_position = event.position;
        let active_pane_id = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;

        if let Some(pane) = self
            .get_pane_at(&absolute_position, false)
            .with_context(err_context)?
        {
            if pane.pid() == active_pane_id {
                let relative_position = pane.relative_position(&absolute_position);
                let mut event_for_pane = event.clone();
                event_for_pane.position = relative_position;
                if let Some(mouse_event) = pane.mouse_event(&event_for_pane, client_id) {
                    if !pane.position_is_on_frame(&absolute_position) {
                        self.write_to_active_terminal(
                            &None,
                            mouse_event.into_bytes(),
                            false,
                            client_id,
                        )
                        .with_context(err_context)?;
                    }
                } else {
                    pane.handle_right_click(&relative_position, client_id);
                }
            }
        };
        Ok(MouseEffect::default())
    }

    fn handle_middle_click(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to handle mouse middle click for client {client_id}");
        let absolute_position = event.position;

        let active_pane_id = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;

        if let Some(pane) = self
            .get_pane_at(&absolute_position, false)
            .with_context(err_context)?
        {
            if pane.pid() == active_pane_id {
                let relative_position = pane.relative_position(&absolute_position);
                let mut event_for_pane = event.clone();
                event_for_pane.position = relative_position;
                if let Some(mouse_event) = pane.mouse_event(&event_for_pane, client_id) {
                    if !pane.position_is_on_frame(&absolute_position) {
                        self.write_to_active_terminal(
                            &None,
                            mouse_event.into_bytes(),
                            false,
                            client_id,
                        )
                        .with_context(err_context)?;
                    }
                }
            }
        };
        Ok(MouseEffect::default())
    }

    fn handle_mouse_no_click(
        &mut self,
        event: &MouseEvent,
        client_id: ClientId,
    ) -> Result<MouseEffect> {
        let err_context = || format!("failed to handle mouse no click for client {client_id}");
        let absolute_position = event.position;

        let active_pane_id = self
            .get_active_pane_id(client_id)
            .ok_or_else(|| anyhow!("Failed to find pane at position"))?;

        if let Some(pane) = self
            .get_pane_at(&absolute_position, false)
            .with_context(err_context)?
        {
            if pane.pid() == active_pane_id {
                let relative_position = pane.relative_position(&absolute_position);
                let mut event_for_pane = event.clone();
                event_for_pane.position = relative_position;
                if let Some(mouse_event) = pane.mouse_event(&event_for_pane, client_id) {
                    if !pane.position_is_on_frame(&absolute_position) {
                        self.write_to_active_terminal(
                            &None,
                            mouse_event.into_bytes(),
                            false,
                            client_id,
                        )
                        .with_context(err_context)?;
                    }
                }
                self.mouse_hover_pane_id.remove(&client_id);
            } else {
                let pane_id = pane.pid();
                // if the pane is not selectable, we don't want to create a hover effect over it
                // we do however want to remove the hover effect from other panes
                let pane_is_selectable = pane.selectable();
                if self.advanced_mouse_actions && pane_is_selectable {
                    self.mouse_hover_pane_id.insert(client_id, pane_id);
                } else if self.advanced_mouse_actions {
                    self.mouse_hover_pane_id.remove(&client_id);
                }
            }
        };
        Ok(MouseEffect::leave_clipboard_message())
    }

    fn unselectable_pane_at_position(&mut self, point: &Position) -> Option<&mut Box<dyn Pane>> {
        // the repetition in this function is to appease the borrow checker, I don't like it either
        let floating_panes_are_visible = self.floating_panes.panes_are_visible();
        if floating_panes_are_visible {
            if let Ok(Some(clicked_pane_id)) = self.floating_panes.get_pane_id_at(point, true) {
                if let Some(pane) = self.floating_panes.get_pane_mut(clicked_pane_id) {
                    if !pane.selectable() {
                        return Some(pane);
                    }
                }
            } else if let Ok(Some(clicked_pane_id)) = self.get_pane_id_at(point, false) {
                if let Some(pane) = self.tiled_panes.get_pane_mut(clicked_pane_id) {
                    if !pane.selectable() {
                        return Some(pane);
                    }
                }
            }
        } else if let Ok(Some(clicked_pane_id)) = self.get_pane_id_at(point, false) {
            if let Some(pane) = self.tiled_panes.get_pane_mut(clicked_pane_id) {
                if !pane.selectable() {
                    return Some(pane);
                }
            }
        }
        None
    }
    fn focus_pane_at(&mut self, point: &Position, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to focus pane at position {point:?} for client {client_id}");

        if self.floating_panes.panes_are_visible() {
            if let Some(clicked_pane) = self
                .floating_panes
                .get_pane_id_at(point, true)
                .with_context(err_context)?
            {
                self.floating_panes.focus_pane(clicked_pane, client_id);
                self.set_pane_active_at(clicked_pane);
                return Ok(());
            }
        }
        if let Some(clicked_pane) = self.get_pane_id_at(point, true).with_context(err_context)? {
            self.tiled_panes.focus_pane(clicked_pane, client_id);
            self.set_pane_active_at(clicked_pane);
            if self.floating_panes.panes_are_visible() {
                self.hide_floating_panes();
                self.set_force_render();
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn handle_right_mouse_release(
        &mut self,
        position: &Position,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || {
            format!("failed to handle right mouse release at position {position:?} for client {client_id}")
        };

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
                self.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                    .with_context(err_context)?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    fn handle_middle_mouse_release(
        &mut self,
        position: &Position,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || {
            format!("failed to handle middle mouse release at position {position:?} for client {client_id}")
        };

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
                self.write_to_active_terminal(&None, mouse_event.into_bytes(), false, client_id)
                    .with_context(err_context)?;
            }
        }
        Ok(())
    }
    pub fn copy_selection(&self, client_id: ClientId) -> Result<()> {
        let selected_text = self
            .get_active_pane(client_id)
            .and_then(|p| p.get_selected_text(client_id));
        if let Some(selected_text) = selected_text {
            self.write_selection_to_clipboard(&selected_text)
                .with_context(|| {
                    format!("failed to write selection to clipboard for client {client_id}")
                })?;
            self.senders
                .send_to_plugin(PluginInstruction::Update(vec![(
                    None,
                    None,
                    Event::CopyToClipboard(self.clipboard_provider.as_copy_destination()),
                )]))
                .with_context(|| {
                    format!("failed to inform plugins about copy selection for client {client_id}")
                })
                .non_fatal();
        }
        Ok(())
    }

    fn write_selection_to_clipboard(&self, selection: &str) -> Result<()> {
        let err_context = || format!("failed to write selection to clipboard: '{}'", selection);

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
                Ok(_) => output
                    .serialize()
                    .and_then(|serialized_output| {
                        self.senders
                            .send_to_server(ServerInstruction::Render(Some(serialized_output)))
                    })
                    .and_then(|_| {
                        Ok(Event::CopyToClipboard(
                            self.clipboard_provider.as_copy_destination(),
                        ))
                    })
                    .with_context(err_context)?,
                Err(err) => {
                    Err::<(), _>(err).with_context(err_context).non_fatal();
                    Event::SystemClipboardFailure
                },
            };
        self.senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                None,
                None,
                clipboard_event,
            )]))
            .context("failed to notify plugins about new clipboard event")
            .non_fatal();

        Ok(())
    }
    pub fn visible(&mut self, visible: bool) -> Result<()> {
        let pids_in_this_tab = self.tiled_panes.pane_ids().filter_map(|p| match p {
            PaneId::Plugin(pid) => Some(pid),
            _ => None,
        });
        let mut plugin_updates = vec![];
        for pid in pids_in_this_tab {
            plugin_updates.push((Some(*pid), None, Event::Visible(visible)));
        }
        self.senders
            .send_to_plugin(PluginInstruction::Update(plugin_updates))
            .with_context(|| format!("failed to set visibility of tab to {visible}"))?;
        if !visible {
            self.mouse_hover_pane_id.clear();
        }
        Ok(())
    }

    pub fn update_active_pane_name(&mut self, buf: Vec<u8>, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to update name of active pane to '{buf:?}' for client {client_id}");
        let s = str::from_utf8(&buf).with_context(err_context)?;
        self.get_active_pane_mut(client_id)
            .with_context(|| format!("no active pane found for client {client_id}"))
            .map(|active_pane| {
                let to_update = match s {
                    "\u{007F}" | "\u{0008}" => {
                        // delete and backspace keys
                        s
                    },
                    _ => &clean_string_from_control_and_linebreak(s),
                };
                active_pane.update_name(&to_update);
            })?;
        Ok(())
    }

    pub fn rename_pane(&mut self, buf: Vec<u8>, pane_id: PaneId) -> Result<()> {
        let err_context = || {
            format!(
                "failed to update name of active pane to '{buf:?}' for pane_id {:?}",
                pane_id
            )
        };
        let pane = self
            .floating_panes
            .get_pane_mut(pane_id)
            .or_else(|| self.tiled_panes.get_pane_mut(pane_id))
            .or_else(|| {
                self.suppressed_panes
                    .get_mut(&pane_id)
                    .map(|s_p| &mut s_p.1)
            })
            .with_context(err_context)?;
        pane.rename(buf);
        Ok(())
    }

    pub fn undo_active_rename_pane(&mut self, client_id: ClientId) -> Result<()> {
        if let Some(active_terminal_id) = self.get_active_terminal_id(client_id) {
            let active_terminal = if self.are_floating_panes_visible() {
                self.floating_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            } else {
                self.tiled_panes
                    .get_pane_mut(PaneId::Terminal(active_terminal_id))
            }
            .with_context(|| {
                format!("failed to undo rename of active pane for client {client_id}")
            })?;

            active_terminal.load_pane_name();
        }
        Ok(())
    }

    pub fn is_position_inside_viewport(&self, point: &Position) -> Result<bool> {
        let Position {
            line: Line(line),
            column: Column(column),
        } = *point;
        let line: usize = line.try_into().with_context(|| {
            format!("failed to determine if position {point:?} is inside viewport")
        })?;

        let viewport = self.viewport.borrow();
        Ok(line >= viewport.y
            && column >= viewport.x
            && line <= viewport.y + viewport.rows
            && column <= viewport.x + viewport.cols)
    }

    pub fn set_pane_frames(&mut self, should_set_pane_frames: bool) {
        self.tiled_panes.set_pane_frames(should_set_pane_frames);
        self.draw_pane_frames = should_set_pane_frames;
        self.set_should_clear_display_before_rendering();
        self.set_force_render();
    }
    pub fn panes_to_hide_count(&self) -> usize {
        self.tiled_panes.panes_to_hide_count()
    }

    pub fn update_search_term(&mut self, buf: Vec<u8>, client_id: ClientId) -> Result<()> {
        if let Some(active_pane) = self.get_active_pane_or_floating_pane_mut(client_id) {
            // It only allows terminating char(\0), printable unicode, delete and backspace keys.
            // TODO: we should really remove this limitation to allow searching for emojis and
            // other wide chars - currently the search mechanism itself ignores wide chars, so we
            // should first fix that before removing this condition
            let is_updatable = buf
                .iter()
                .all(|u| matches!(u, 0x00 | 0x20..=0x7E | 0x08 | 0x7F));
            if is_updatable {
                let s = str::from_utf8(&buf).with_context(|| {
                    format!("failed to update search term to '{buf:?}' for client {client_id}")
                })?;
                active_pane.update_search_term(s);
            }
        }
        Ok(())
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

    pub fn is_pending(&self) -> bool {
        self.is_pending
    }

    pub fn add_red_pane_frame_color_override(
        &mut self,
        pane_id: PaneId,
        error_text: Option<String>,
    ) {
        if let Some(pane) = self
            .tiled_panes
            .get_pane_mut(pane_id)
            .or_else(|| self.floating_panes.get_pane_mut(pane_id))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == pane_id)
                    .map(|s_p| &mut s_p.1)
            })
        {
            pane.add_red_pane_frame_color_override(error_text);
        }
    }
    pub fn add_highlight_pane_frame_color_override(
        &mut self,
        pane_id: PaneId,
        error_text: Option<String>,
        client_id: Option<ClientId>,
    ) {
        if let Some(pane) = self
            .tiled_panes
            .get_pane_mut(pane_id)
            .or_else(|| self.floating_panes.get_pane_mut(pane_id))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == pane_id)
                    .map(|s_p| &mut s_p.1)
            })
        {
            pane.add_highlight_pane_frame_color_override(error_text, client_id);
        }
    }
    pub fn clear_pane_frame_color_override(
        &mut self,
        pane_id: PaneId,
        client_id: Option<ClientId>,
    ) {
        if let Some(pane) = self
            .tiled_panes
            .get_pane_mut(pane_id)
            .or_else(|| self.floating_panes.get_pane_mut(pane_id))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == pane_id)
                    .map(|s_p| &mut s_p.1)
            })
        {
            pane.clear_pane_frame_color_override(client_id);
        }
    }
    pub fn update_plugin_loading_stage(&mut self, pid: u32, loading_indication: LoadingIndication) {
        if let Some(plugin_pane) = self
            .tiled_panes
            .get_pane_mut(PaneId::Plugin(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Plugin(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Plugin(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            plugin_pane.update_loading_indication(loading_indication);
        }
    }
    pub fn start_plugin_loading_indication(
        &mut self,
        pid: u32,
        loading_indication: LoadingIndication,
    ) {
        if let Some(plugin_pane) = self
            .tiled_panes
            .get_pane_mut(PaneId::Plugin(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Plugin(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Plugin(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            plugin_pane.start_loading_indication(loading_indication);
        }
    }
    pub fn progress_plugin_loading_offset(&mut self, pid: u32) {
        if let Some(plugin_pane) = self
            .tiled_panes
            .get_pane_mut(PaneId::Plugin(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Plugin(pid)))
            .or_else(|| {
                self.suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Plugin(pid))
                    .map(|s_p| &mut s_p.1)
            })
        {
            plugin_pane.progress_animation_offset();
        }
    }
    pub fn show_floating_panes(&mut self) {
        // this function is to be preferred to directly invoking floating_panes.toggle_show_panes(true)
        self.floating_panes.toggle_show_panes(true);
        self.tiled_panes.unfocus_all_panes();
        self.set_force_render();
    }

    pub fn hide_floating_panes(&mut self) {
        // this function is to be preferred to directly invoking
        // floating_panes.toggle_show_panes(false)
        self.floating_panes.toggle_show_panes(false);
        self.tiled_panes.focus_all_panes();
        self.set_force_render();
    }

    pub fn find_plugin(&self, run_plugin_or_alias: &RunPluginOrAlias) -> Option<PaneId> {
        self.tiled_panes
            .get_plugin_pane_id(run_plugin_or_alias)
            .or_else(|| self.floating_panes.get_plugin_pane_id(run_plugin_or_alias))
            .or_else(|| {
                self.suppressed_panes
                    .iter()
                    .find(|(_id, (_, pane))| {
                        run_plugin_or_alias.is_equivalent_to_run(pane.invoked_with())
                    })
                    .map(|(id, _)| *id)
            })
    }

    pub fn focus_pane_with_id(
        &mut self,
        pane_id: PaneId,
        should_float: bool,
        client_id: ClientId,
    ) -> Result<()> {
        // TODO: should error if pane is not selectable
        self.tiled_panes
            .focus_pane_if_exists(pane_id, client_id)
            .map(|_| self.hide_floating_panes())
            .or_else(|_| {
                let focused_floating_pane =
                    self.floating_panes.focus_pane_if_exists(pane_id, client_id);
                if focused_floating_pane.is_ok() {
                    self.show_floating_panes()
                };
                focused_floating_pane
            })
            .or_else(|_| match self.suppressed_panes.remove(&pane_id) {
                Some(mut pane) => {
                    pane.1.set_selectable(true);
                    if should_float {
                        self.show_floating_panes();
                        self.add_floating_pane(pane.1, pane_id, None, true)
                    } else {
                        self.hide_floating_panes();
                        self.add_tiled_pane(pane.1, pane_id, Some(client_id))
                    }
                },
                None => Ok(()),
            })
    }
    pub fn focus_suppressed_pane_for_all_clients(&mut self, pane_id: PaneId) {
        match self.suppressed_panes.remove(&pane_id) {
            Some(pane) => {
                self.show_floating_panes();
                self.add_floating_pane(pane.1, pane_id, None, true)
                    .non_fatal();
                self.floating_panes.focus_pane_for_all_clients(pane_id);
            },
            None => {
                log::error!("Could not find suppressed pane wiht id: {:?}", pane_id);
            },
        }
    }
    pub fn suppress_pane(&mut self, pane_id: PaneId, _client_id: Option<ClientId>) {
        // this method places a pane in the suppressed pane with its own ID - this means we'll
        // not take it out of there when another pane is closed (eg. like happens with the
        // scrollback editor), but it has to take itself out on its own (eg. a plugin using the
        // show_self() method)
        if let Some(pane) = self.extract_pane(pane_id, true) {
            let is_scrollback_editor = false;
            self.suppressed_panes
                .insert(pane_id, (is_scrollback_editor, pane));
        }
    }
    pub fn pane_infos(&self) -> Vec<PaneInfo> {
        let mut pane_info = vec![];
        let current_pane_group = { self.current_pane_group.borrow().clone_inner() };
        let mut tiled_pane_info = self.tiled_panes.pane_info(&current_pane_group);
        let mut floating_pane_info = self.floating_panes.pane_info(&current_pane_group);
        pane_info.append(&mut tiled_pane_info);
        pane_info.append(&mut floating_pane_info);
        for (pane_id, (_is_scrollback_editor, pane)) in self.suppressed_panes.iter() {
            let mut pane_info_for_suppressed_pane =
                pane_info_for_pane(pane_id, pane, &current_pane_group);
            pane_info_for_suppressed_pane.is_floating = false;
            pane_info_for_suppressed_pane.is_suppressed = true;
            pane_info_for_suppressed_pane.is_focused = false;
            pane_info_for_suppressed_pane.is_fullscreen = false;
            pane_info.push(pane_info_for_suppressed_pane);
        }
        pane_info
    }
    pub fn add_floating_pane(
        &mut self,
        mut pane: Box<dyn Pane>,
        pane_id: PaneId,
        floating_pane_coordinates: Option<FloatingPaneCoordinates>,
        should_focus_new_pane: bool,
    ) -> Result<()> {
        let err_context = || format!("failed to add floating pane");
        if let Some(mut new_pane_geom) = self.floating_panes.find_room_for_new_pane() {
            if let Some(floating_pane_coordinates) = floating_pane_coordinates {
                let viewport = self.viewport.borrow();
                if let Some(pinned) = floating_pane_coordinates.pinned.as_ref() {
                    pane.set_pinned(*pinned);
                }
                new_pane_geom.adjust_coordinates(floating_pane_coordinates, *viewport);
                self.swap_layouts.set_is_floating_damaged();
            }
            pane.set_active_at(Instant::now());
            pane.set_geom(new_pane_geom);
            pane.set_content_offset(Offset::frame(1)); // floating panes always have a frame
            pane.render_full_viewport(); // to make sure the frame is re-rendered
            resize_pty!(pane, self.os_api, self.senders, self.character_cell_size)
                .with_context(err_context)?;
            self.floating_panes.add_pane(pane_id, pane);
            if should_focus_new_pane {
                self.floating_panes.focus_pane_for_all_clients(pane_id);
            }
        }
        if self.auto_layout && !self.swap_layouts.is_floating_damaged() {
            // only do this if we're already in this layout, otherwise it might be
            // confusing and not what the user intends
            self.swap_layouts.set_is_floating_damaged(); // we do this so that we won't skip to the
                                                         // next layout
            self.relayout_floating_panes(false)?;
        }
        Ok(())
    }
    pub fn add_tiled_pane(
        &mut self,
        mut pane: Box<dyn Pane>,
        pane_id: PaneId,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        let should_auto_layout = self.auto_layout && !self.swap_layouts.is_tiled_damaged();
        if self.tiled_panes.has_room_for_new_pane() {
            pane.set_active_at(Instant::now());
            if should_auto_layout {
                // no need to relayout here, we'll do it when reapplying the swap layout
                // below
                self.tiled_panes
                    .insert_pane_without_relayout(pane_id, pane, client_id);
            } else {
                self.tiled_panes.insert_pane(pane_id, pane, client_id);
            }
            self.set_should_clear_display_before_rendering();
            if let Some(client_id) = client_id {
                self.tiled_panes.focus_pane(pane_id, client_id);
            }
        }
        if should_auto_layout {
            // only do this if we're already in this layout, otherwise it might be
            // confusing and not what the user intends
            self.swap_layouts.set_is_tiled_damaged(); // we do this so that we won't skip to the
                                                      // next layout
            self.relayout_tiled_panes(false)?;
        }
        Ok(())
    }
    pub fn add_stacked_pane_to_pane_id(
        &mut self,
        pane: Box<dyn Pane>,
        pane_id: PaneId,
        root_pane_id: PaneId,
    ) -> Result<()> {
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        self.tiled_panes
            .add_pane_to_stack_of_pane_id(pane_id, pane, root_pane_id);
        self.set_should_clear_display_before_rendering();
        self.tiled_panes.expand_pane_in_stack(pane_id); // so that it will get focused by all
                                                        // clients
        self.swap_layouts.set_is_tiled_damaged();
        Ok(())
    }
    pub fn add_stacked_pane_to_active_pane(
        &mut self,
        pane: Box<dyn Pane>,
        pane_id: PaneId,
        client_id: ClientId,
    ) -> Result<()> {
        if self.tiled_panes.fullscreen_is_active() {
            self.tiled_panes.unset_fullscreen();
        }
        self.tiled_panes
            .add_pane_to_stack_of_active_pane(pane_id, pane, client_id);
        self.tiled_panes.focus_pane(pane_id, client_id);
        self.swap_layouts.set_is_tiled_damaged();
        Ok(())
    }
    pub fn request_plugin_permissions(&mut self, pid: u32, permissions: Option<PluginPermission>) {
        let mut should_focus_pane = false;
        if let Some(plugin_pane) = self
            .tiled_panes
            .get_pane_mut(PaneId::Plugin(pid))
            .or_else(|| self.floating_panes.get_pane_mut(PaneId::Plugin(pid)))
            .or_else(|| {
                let mut suppressed_pane = self
                    .suppressed_panes
                    .values_mut()
                    .find(|s_p| s_p.1.pid() == PaneId::Plugin(pid))
                    .map(|s_p| &mut s_p.1);
                if let Some(suppressed_pane) = suppressed_pane.as_mut() {
                    if permissions.is_some() {
                        // here what happens is that we're requesting permissions for a pane that
                        // is suppressed, meaning the user cannot see the permission request
                        // so we temporarily focus this pane as a floating pane, marking it so that
                        // once the permissions are accepted/rejected by the user, it will be
                        // suppressed again
                        suppressed_pane.set_should_be_suppressed(true);
                        should_focus_pane = true;
                    }
                }
                suppressed_pane
            })
        {
            plugin_pane.request_permissions_from_user(permissions);
        }
        if should_focus_pane {
            self.focus_suppressed_pane_for_all_clients(PaneId::Plugin(pid));
        }
    }
    pub fn rerun_terminal_pane_with_id(&mut self, terminal_pane_id: u32) {
        let pane_id = PaneId::Terminal(terminal_pane_id);
        match self
            .floating_panes
            .get_mut(&pane_id)
            .or_else(|| self.tiled_panes.get_pane_mut(pane_id))
            .or_else(|| self.suppressed_panes.get_mut(&pane_id).map(|p| &mut p.1))
        {
            Some(pane_to_rerun) => {
                if let Some(command_to_rerun) = pane_to_rerun.rerun() {
                    self.pids_waiting_resize.insert(terminal_pane_id);
                    let _ = self.senders.send_to_pty(PtyInstruction::ReRunCommandInPane(
                        pane_id,
                        command_to_rerun,
                    ));
                } else {
                    log::error!("Pane is still running!")
                }
            },
            None => {
                log::error!(
                    "Failed to find terminal pane with id {} to rerun in tab",
                    terminal_pane_id
                );
            },
        }
    }
    pub fn resize_pane_with_id(&mut self, strategy: ResizeStrategy, pane_id: PaneId) -> Result<()> {
        let err_context = || format!("unable to resize pane");
        if self.floating_panes.panes_contain(&pane_id) {
            let successfully_resized = self
                .floating_panes
                .resize_pane_with_id(strategy, pane_id)
                .with_context(err_context)?;
            if successfully_resized {
                self.swap_layouts.set_is_floating_damaged();
                self.swap_layouts.set_is_tiled_damaged();
                self.set_force_render(); // we force render here to make sure the panes under the floating pane render and don't leave "garbage" in case of a decrease
            }
        } else if self.tiled_panes.panes_contain(&pane_id) {
            match self
                .tiled_panes
                .resize_pane_with_id(strategy, pane_id, None)
            {
                Ok(_) => {},
                Err(err) => match err.downcast_ref::<ZellijError>() {
                    Some(ZellijError::CantResizeFixedPanes { pane_ids }) => {
                        let mut pane_ids_to_error = vec![];
                        for (id, is_terminal) in pane_ids {
                            if *is_terminal {
                                pane_ids_to_error.push(PaneId::Terminal(*id));
                            } else {
                                pane_ids_to_error.push(PaneId::Plugin(*id));
                            };
                        }
                        self.senders
                            .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                                pane_ids_to_error,
                                "FIXED!".into(),
                            ))
                            .with_context(err_context)?;
                    },
                    _ => Err::<(), _>(err).non_fatal(),
                },
            }
        } else if self
            .suppressed_panes
            .values()
            .any(|s_p| s_p.1.pid() == pane_id)
        {
            log::error!("Cannot resize suppressed panes");
        }
        Ok(())
    }
    pub fn update_theme(&mut self, theme: Styling) {
        self.style.colors = theme;
        self.floating_panes.update_pane_themes(theme);
        self.tiled_panes.update_pane_themes(theme);
        for (_, pane) in self.suppressed_panes.values_mut() {
            pane.update_theme(theme);
        }
    }
    pub fn update_rounded_corners(&mut self, rounded_corners: bool) {
        self.style.rounded_corners = rounded_corners;
        self.floating_panes
            .update_pane_rounded_corners(rounded_corners);
        self.tiled_panes
            .update_pane_rounded_corners(rounded_corners);
        for (_, pane) in self.suppressed_panes.values_mut() {
            pane.update_rounded_corners(rounded_corners);
        }
    }
    pub fn update_arrow_fonts(&mut self, should_support_arrow_fonts: bool) {
        self.arrow_fonts = should_support_arrow_fonts;
        self.floating_panes
            .update_pane_arrow_fonts(should_support_arrow_fonts);
        self.tiled_panes
            .update_pane_arrow_fonts(should_support_arrow_fonts);
        for (_, pane) in self.suppressed_panes.values_mut() {
            pane.update_arrow_fonts(should_support_arrow_fonts);
        }
    }
    pub fn update_default_shell(&mut self, mut default_shell: Option<PathBuf>) {
        if let Some(default_shell) = default_shell.take() {
            self.default_shell = default_shell;
        }
    }
    pub fn update_default_editor(&mut self, mut default_editor: Option<PathBuf>) {
        if let Some(default_editor) = default_editor.take() {
            self.default_editor = Some(default_editor);
        }
    }
    pub fn update_copy_options(&mut self, copy_options: &CopyOptions) {
        self.clipboard_provider = match &copy_options.command {
            Some(command) => ClipboardProvider::Command(CopyCommand::new(command.clone())),
            None => ClipboardProvider::Osc52(copy_options.clipboard),
        };
        self.copy_on_select = copy_options.copy_on_select;
    }
    pub fn update_auto_layout(&mut self, auto_layout: bool) {
        self.auto_layout = auto_layout;
    }
    pub fn update_advanced_mouse_actions(&mut self, advanced_mouse_actions: bool) {
        self.advanced_mouse_actions = advanced_mouse_actions;
    }
    pub fn update_web_sharing(&mut self, web_sharing: WebSharing) {
        let old_value = self.web_sharing;
        self.web_sharing = web_sharing;
        if old_value != self.web_sharing {
            let _ = self.update_input_modes();
        }
    }
    pub fn extract_suppressed_panes(&mut self) -> SuppressedPanes {
        self.suppressed_panes.drain().collect()
    }
    pub fn add_suppressed_panes(&mut self, mut suppressed_panes: SuppressedPanes) {
        for (pane_id, suppressed_pane_entry) in suppressed_panes.drain() {
            self.suppressed_panes.insert(pane_id, suppressed_pane_entry);
        }
    }
    pub fn toggle_pane_pinned(&mut self, client_id: ClientId) {
        if let Some(pane) = self.get_active_pane_mut(client_id) {
            pane.toggle_pinned();
            self.set_force_render();
        }
    }
    pub fn set_floating_pane_pinned(&mut self, pane_id: PaneId, should_be_pinned: bool) {
        if let Some(pane) = self.get_pane_with_id_mut(pane_id) {
            pane.set_pinned(should_be_pinned);
            self.set_force_render();
        }
    }
    pub fn has_room_for_stack(&mut self, root_pane_id: PaneId, stack_size: usize) -> bool {
        if self.floating_panes.panes_contain(&root_pane_id)
            || self.suppressed_panes.contains_key(&root_pane_id)
        {
            log::error!("Root pane of stack cannot be floating or suppressed");
            return false;
        }
        if self.pane_is_stacked(root_pane_id) {
            let room_left_in_stack = self
                .tiled_panes
                .room_left_in_stack_of_pane_id(&root_pane_id)
                .unwrap_or(0);
            stack_size <= room_left_in_stack
        } else {
            self.get_pane_with_id(root_pane_id)
                .map(|p| p.position_and_size().rows.as_usize() >= stack_size + MIN_TERMINAL_HEIGHT)
                .unwrap_or(false)
        }
    }
    pub fn set_tiled_panes_damaged(&mut self) {
        self.swap_layouts.set_is_tiled_damaged();
    }
    pub fn stack_panes(&mut self, root_pane_id: PaneId, mut panes_to_stack: Vec<Box<dyn Pane>>) {
        if panes_to_stack.is_empty() {
            // nothing to do
            return;
        }
        self.swap_layouts.set_is_tiled_damaged(); // TODO: verify we can do all the below first
        if self.pane_is_stacked(root_pane_id) {
            for pane in panes_to_stack.drain(..) {
                self.tiled_panes.add_pane_to_stack(&root_pane_id, pane);
            }
        } else {
            // + 1 for the root pane
            let mut stack_geoms = self
                .tiled_panes
                .stack_panes(root_pane_id, panes_to_stack.len() + 1);
            if stack_geoms.is_empty() {
                log::error!("Failed to find room for stacked panes");
                return;
            }
            self.tiled_panes
                .set_geom_for_pane_with_id(&root_pane_id, stack_geoms.remove(0));
            let mut focused_pane_id_in_stack = None;
            for mut pane in panes_to_stack.drain(..) {
                let pane_id = pane.pid();
                let stack_geom = stack_geoms.remove(0);
                pane.set_geom(stack_geom);
                self.tiled_panes.add_pane_with_existing_geom(pane_id, pane);
                if self.tiled_panes.pane_id_is_focused(&pane_id) {
                    focused_pane_id_in_stack = Some(pane_id);
                }
            }
            // if we had a focused pane in the stack, we expand it
            if let Some(focused_pane_id_in_stack) = focused_pane_id_in_stack {
                self.tiled_panes
                    .expand_pane_in_stack(focused_pane_id_in_stack);
            } else if self.tiled_panes.pane_id_is_focused(&root_pane_id) {
                self.tiled_panes.expand_pane_in_stack(root_pane_id);
            }
        }
    }
    pub fn change_floating_pane_coordinates(
        &mut self,
        pane_id: &PaneId,
        floating_pane_coordinates: FloatingPaneCoordinates,
    ) -> Result<()> {
        if !self.floating_panes.panes_contain(pane_id) {
            // if these panes are not floating, we make them floating (assuming doing so wouldn't
            // be removing the last selectable tiled pane in the tab, which would close it)
            if (self.tiled_panes.panes_contain(&pane_id)
                && self.get_selectable_tiled_panes().count() <= 1)
                || self.suppressed_panes.contains_key(pane_id)
            {
                if let Some(pane) = self.extract_pane(*pane_id, true) {
                    self.add_floating_pane(pane, *pane_id, None, false)?;
                }
            }
        }
        self.floating_panes
            .change_pane_coordinates(*pane_id, floating_pane_coordinates)?;
        self.set_force_render();
        self.swap_layouts.set_is_floating_damaged();
        Ok(())
    }
    pub fn get_viewport(&self) -> Viewport {
        self.viewport.borrow().clone()
    }
    pub fn get_display_area(&self) -> Size {
        self.display_area.borrow().clone()
    }
    pub fn get_client_input_mode(&self, client_id: ClientId) -> Option<InputMode> {
        self.mode_info.borrow().get(&client_id).map(|m| m.mode)
    }
    fn new_scrollback_editor_pane(&self, pid: u32) -> TerminalPane {
        let next_terminal_position = self.get_next_terminal_position();
        let mut new_pane = TerminalPane::new(
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
            None,
            None,
            self.debug,
            self.arrow_fonts,
            self.styled_underlines,
            self.explicitly_disable_kitty_keyboard_protocol,
        );
        new_pane.update_name("EDITING SCROLLBACK"); // we do this here and not in the
                                                    // constructor so it won't be overrided
                                                    // by the editor
        new_pane
    }
    fn insert_scrollback_editor_replaced_pane(
        &mut self,
        replaced_pane: Box<dyn Pane>,
        terminal_pane_id: u32,
    ) {
        let is_scrollback_editor = true;
        self.suppressed_panes.insert(
            PaneId::Terminal(terminal_pane_id),
            (is_scrollback_editor, replaced_pane),
        );
    }
    fn pane_is_stacked(&self, pane_id: PaneId) -> bool {
        self.get_pane_with_id(pane_id)
            .map(|p| p.position_and_size().stacked.is_some())
            .unwrap_or(false)
    }
}

pub fn pane_info_for_pane(
    pane_id: &PaneId,
    pane: &Box<dyn Pane>,
    current_pane_group: &HashMap<ClientId, Vec<PaneId>>,
) -> PaneInfo {
    let mut pane_info = PaneInfo::default();
    pane_info.pane_x = pane.x();
    pane_info.pane_content_x = pane.get_content_x();
    pane_info.pane_y = pane.y();
    pane_info.pane_content_y = pane.get_content_y();
    pane_info.pane_rows = pane.rows();
    pane_info.pane_content_rows = pane.get_content_rows();
    pane_info.pane_columns = pane.cols();
    pane_info.pane_content_columns = pane.get_content_columns();
    pane_info.cursor_coordinates_in_pane = pane.cursor_coordinates();
    pane_info.is_selectable = pane.selectable();
    pane_info.title = pane.current_title();
    pane_info.exited = pane.exited();
    pane_info.exit_status = pane.exit_status();
    pane_info.is_held = pane.is_held();
    let index_in_pane_group: BTreeMap<ClientId, usize> = current_pane_group
        .iter()
        .filter_map(|(client_id, pane_ids)| {
            if let Some(position) = pane_ids.iter().position(|p_id| p_id == &pane.pid()) {
                Some((*client_id, position))
            } else {
                None
            }
        })
        .collect();
    pane_info.index_in_pane_group = index_in_pane_group;

    match pane_id {
        PaneId::Terminal(terminal_id) => {
            pane_info.id = *terminal_id;
            pane_info.is_plugin = false;
            pane_info.terminal_command = pane.invoked_with().as_ref().and_then(|c| match c {
                Run::Command(run_command) => Some(run_command.to_string()),
                _ => None,
            });
        },
        PaneId::Plugin(plugin_id) => {
            pane_info.id = *plugin_id;
            pane_info.is_plugin = true;
            pane_info.plugin_url = pane.invoked_with().as_ref().and_then(|c| match c {
                Run::Plugin(run_plugin_or_alias) => Some(run_plugin_or_alias.location_string()),
                _ => None,
            });
        },
    }
    pane_info
}

#[cfg(test)]
#[path = "./unit/tab_tests.rs"]
mod tab_tests;

#[cfg(test)]
#[path = "./unit/tab_integration_tests.rs"]
mod tab_integration_tests;
