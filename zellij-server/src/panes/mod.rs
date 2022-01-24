pub mod alacritty_functions;
pub mod grid;
pub mod link_handler;
pub mod plugin_pane;
pub mod selection;
pub mod terminal_character;
pub mod terminal_pane;

pub use alacritty_functions::*;
pub use grid::*;
pub(crate) use plugin_pane::*;
pub use terminal_character::*;
pub(crate) use terminal_pane::*;

const MIN_TERMINAL_HEIGHT: usize = 5;
const MIN_TERMINAL_WIDTH: usize = 5;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    str,
};
use std::time::Instant;
use zellij_utils::{
    input::{
        layout::{Direction, Layout, Run},
        parse_keys,
    },
    pane_size::{Offset, PaneGeom, Size, Viewport},
};
use zellij_utils::{position::Position, serde, zellij_tile};
use zellij_tile::data::{Event, InputMode, ModeInfo, Palette, PaletteColor};
use crate::{
    os_input_output::ServerOsApi,
    pty::{PtyInstruction, VteBytes},
    thread_bus::ThreadSenders,
    ui::boundaries::Boundaries,
    ui::pane_contents_and_ui::PaneContentsAndUi,
    wasm_vm::PluginInstruction,
    ClientId, ServerInstruction,
};
use crate::ui::pane_boundaries_frame::{FrameParams, PaneFrame};

pub struct PaneStruct { // TODO: rename to Pane after we get rid of the trait
    pub pid: PaneId,
    pub selectable: bool,
    pub geom: PaneGeom,
    pub geom_override: Option<PaneGeom>,
    pub active_at: Instant,
    pub colors: Palette,
    selection_scrolled_at: Instant,
    content_offset: Offset,
    pane_title: String,
    pane_name: String,
    frame: HashMap<ClientId, PaneFrame>,
    borderless: bool,
}

impl PaneStruct { // TODO: rename to Pane after we get rid of the trait
    pub fn x(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).x
    }
    pub fn y(&self) -> usize {
        unimplemented!()
    }
    pub fn rows(&self) -> usize {
        unimplemented!()
    }
    pub fn cols(&self) -> usize {
        unimplemented!()
    }
    pub fn get_content_x(&self) -> usize {
        unimplemented!()
    }
    pub fn get_content_y(&self) -> usize {
        unimplemented!()
    }
    pub fn get_content_columns(&self) -> usize {
        unimplemented!()
    }
    pub fn get_content_rows(&self) -> usize {
        unimplemented!()
    }
    pub fn reset_size_and_position_override(&mut self) {
        unimplemented!()
    }
    pub fn set_geom(&mut self, position_and_size: PaneGeom) {
        unimplemented!()
    }
    pub fn get_geom_override(&mut self, pane_geom: PaneGeom) {
        unimplemented!()
    }
    pub fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        unimplemented!()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        unimplemented!()
    }
    pub fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8> {
        unimplemented!()
    }
    pub fn position_and_size(&self) -> PaneGeom {
        unimplemented!()
    }
    pub fn current_geom(&self) -> PaneGeom {
        unimplemented!()
    }
    pub fn geom_override(&self) -> Option<PaneGeom> {
        unimplemented!()
    }
    pub fn should_render(&self) -> bool {
        unimplemented!()
    }
    pub fn set_should_render(&mut self, should_render: bool) {
        unimplemented!()
    }
    pub fn set_should_render_boundaries(&mut self, _should_render: bool) {
        unimplemented!()
    }
    pub fn selectable(&self) -> bool {
        unimplemented!()
    }
    pub fn set_selectable(&mut self, selectable: bool) {
        unimplemented!()
    }
    pub fn render(&mut self, client_id: Option<ClientId>) -> Option<String> {
        unimplemented!()
    }
    pub fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Option<String> {
        unimplemented!()
    }
    pub fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<String> {
        unimplemented!()
    }
    pub fn update_name(&mut self, name: &str) {
        unimplemented!()
    }
    pub fn pid(&self) -> PaneId {
        unimplemented!()
    }
    pub fn reduce_height(&mut self, percent: f64) {
        unimplemented!()
    }
    pub fn increase_height(&mut self, percent: f64) {
        unimplemented!()
    }
    pub fn reduce_width(&mut self, percent: f64) {
        unimplemented!()
    }
    pub fn increase_width(&mut self, percent: f64) {
        unimplemented!()
    }
    pub fn push_down(&mut self, count: usize) {
        unimplemented!()
    }
    pub fn push_right(&mut self, count: usize) {
        unimplemented!()
    }
    pub fn pull_left(&mut self, count: usize) {
        unimplemented!()
    }
    pub fn pull_up(&mut self, count: usize) {
        unimplemented!()
    }
    pub fn scroll_up(&mut self, count: usize, client_id: ClientId) {
        unimplemented!()
    }
    pub fn scroll_down(&mut self, count: usize, client_id: ClientId) {
        unimplemented!()
    }
    pub fn clear_scroll(&mut self) {
        unimplemented!()
    }
    pub fn is_scrolled(&self) -> bool {
        unimplemented!()
    }
    pub fn active_at(&self) -> Instant {
        unimplemented!()
    }
    pub fn set_active_at(&mut self, instant: Instant) {
        unimplemented!()
    }
    pub fn set_frame(&mut self, frame: bool) {
        unimplemented!()
    }
    pub fn set_content_offset(&mut self, offset: Offset) {
        unimplemented!()
    }
    pub fn cursor_shape_csi(&self) -> String {
        "\u{1b}[0 q".to_string() // default to non blinking block
    }
    pub fn contains(&self, position: &Position) -> bool {
        match self.geom_override() {
            Some(position_and_size) => position_and_size.contains(position),
            None => self.position_and_size().contains(position),
        }
    }
    pub fn start_selection(&mut self, _start: &Position, _client_id: ClientId) {
        unimplemented!()
    }
    pub fn update_selection(&mut self, _position: &Position, _client_id: ClientId) {
        unimplemented!()
    }
    pub fn end_selection(&mut self, _end: Option<&Position>, _client_id: ClientId) {
        unimplemented!()
    }
    pub fn reset_selection(&mut self) {
        unimplemented!()
    }
    pub fn get_selected_text(&self) -> Option<String> {
        unimplemented!()
    }

    pub fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.cols()
    }
    pub fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    pub fn is_directly_right_of(&self, other: &PaneStruct) -> bool {
        self.x() == other.x() + other.cols()
    }
    pub fn is_directly_left_of(&self, other: &PaneStruct) -> bool {
        self.x() + self.cols() == other.x()
    }
    pub fn is_directly_below(&self, other: &PaneStruct) -> bool {
        self.y() == other.y() + other.rows()
    }
    pub fn is_directly_above(&self, other: &PaneStruct) -> bool {
        self.y() + self.rows() == other.y()
    }
    pub fn horizontally_overlaps_with(&self, other: &PaneStruct) -> bool {
        (self.y() >= other.y() && self.y() < (other.y() + other.rows()))
            || ((self.y() + self.rows()) <= (other.y() + other.rows())
                && (self.y() + self.rows()) > other.y())
            || (self.y() <= other.y() && (self.y() + self.rows() >= (other.y() + other.rows())))
            || (other.y() <= self.y() && (other.y() + other.rows() >= (self.y() + self.rows())))
    }
    pub fn get_horizontal_overlap_with(&self, other: &PaneStruct) -> usize {
        std::cmp::min(self.y() + self.rows(), other.y() + other.rows())
            - std::cmp::max(self.y(), other.y())
    }
    pub fn vertically_overlaps_with(&self, other: &PaneStruct) -> bool {
        (self.x() >= other.x() && self.x() < (other.x() + other.cols()))
            || ((self.x() + self.cols()) <= (other.x() + other.cols())
                && (self.x() + self.cols()) > other.x())
            || (self.x() <= other.x() && (self.x() + self.cols() >= (other.x() + other.cols())))
            || (other.x() <= self.x() && (other.x() + other.cols() >= (self.x() + self.cols())))
    }
    pub fn get_vertical_overlap_with(&self, other: &PaneStruct) -> usize {
        std::cmp::min(self.x() + self.cols(), other.x() + other.cols())
            - std::cmp::max(self.x(), other.x())
    }
    pub fn can_reduce_height_by(&self, reduce_by: usize) -> bool {
        self.rows() > reduce_by && self.rows() - reduce_by >= self.min_height()
    }
    pub fn can_reduce_width_by(&self, reduce_by: usize) -> bool {
        self.cols() > reduce_by && self.cols() - reduce_by >= self.min_width()
    }
    pub fn min_width(&self) -> usize {
        MIN_TERMINAL_WIDTH
    }
    pub fn min_height(&self) -> usize {
        MIN_TERMINAL_HEIGHT
    }
    pub fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        // TODO: this is only relevant to terminal panes
        // we should probably refactor away from this trait at some point
        vec![]
    }
    pub fn render_full_viewport(&mut self) {
        unimplemented!()
    }
    pub fn relative_position(&self, position_on_screen: &Position) -> Position {
        position_on_screen.relative_to(self.get_content_y(), self.get_content_x())
    }
    pub fn set_borderless(&mut self, borderless: bool) {
        unimplemented!()
    }
    pub fn borderless(&self) -> bool {
        unimplemented!()
    }
    pub fn handle_right_click(&mut self, _to: &Position, _client_id: ClientId) {
        unimplemented!()
    }
}
