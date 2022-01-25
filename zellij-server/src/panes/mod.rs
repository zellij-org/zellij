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

pub enum PaneKind {
    Terminal,
    Plugin,
}

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
    kind: PaneKind,
}

impl PaneStruct {
    pub fn new_from_plugin(plugin: PluginPane) -> Self {
        unimplemented!()
    }
    pub fn new_from_terminal(plugin: TerminalPane) -> Self {
        unimplemented!()
    }
}

impl PaneStruct { // TODO: rename to Pane after we get rid of the trait
    pub fn x(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).x
    }
    pub fn y(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).y
    }
    pub fn rows(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).rows.as_usize()
    }
    pub fn cols(&self) -> usize {
        self.geom_override.unwrap_or(self.geom).cols.as_usize()
    }
    pub fn get_content_x(&self) -> usize {
        self.x() + self.content_offset.left
    }
    pub fn get_content_y(&self) -> usize {
        self.y() + self.content_offset.top
    }
    pub fn get_content_columns(&self) -> usize {
        self.cols()
            .saturating_sub(self.content_offset.left + self.content_offset.right)
    }
    pub fn get_content_rows(&self) -> usize {
        self.rows()
            .saturating_sub(self.content_offset.top + self.content_offset.bottom)
    }
    pub fn reset_size_and_position_override(&mut self) {
        self.geom_override = None;
        self.reflow_lines();
    }
    pub fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.geom = position_and_size;
        self.reflow_lines();
    }
    pub fn get_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
        self.reflow_lines();
    }
    pub fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        for &byte in &bytes {
            self.vte_parser.advance(&mut self.grid, byte);
        }
        self.set_should_render(true);
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        match kind {
            PaneKind::Plugin => None,
            PaneKind::Terminal => {
                let Offset { top, left, .. } = self.content_offset;
                self.grid
                    .cursor_coordinates()
                    .map(|(x, y)| (x + left, y + top))
            }
        }
    }
    pub fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8> {
        // there are some cases in which the terminal state means that input sent to it
        // needs to be adjusted.
        // here we match against those cases - if need be, we adjust the input and if not
        // we send back the original input
        match input_bytes.as_slice() {
            [27, 91, 68] => {
                // left arrow
                if self.grid.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OD".as_bytes().to_vec();
                }
            }
            [27, 91, 67] => {
                // right arrow
                if self.grid.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OC".as_bytes().to_vec();
                }
            }
            [27, 91, 65] => {
                // up arrow
                if self.grid.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OA".as_bytes().to_vec();
                }
            }
            [27, 91, 66] => {
                // down arrow
                if self.grid.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OB".as_bytes().to_vec();
                }
            }
            [27, 91, 50, 48, 48, 126] | [27, 91, 50, 48, 49, 126] => {
                if !self.grid.bracketed_paste_mode {
                    // Zellij itself operates in bracketed paste mode, so the terminal sends these
                    // instructions (bracketed paste start and bracketed paste end respectively)
                    // when pasting input. We only need to make sure not to send them to terminal
                    // panes who do not work in this mode
                    return vec![];
                }
            }
            _ => {}
        };
        input_bytes
    }
    pub fn position_and_size(&self) -> PaneGeom {
        self.geom
    }
    pub fn current_geom(&self) -> PaneGeom {
        self.geom_override.unwrap_or(self.geom)
    }
    pub fn geom_override(&self) -> Option<PaneGeom> {
        self.geom_override
    }
    pub fn should_render(&self) -> bool {
        self.grid.should_render
    }
    pub fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
    }
    pub fn selectable(&self) -> bool {
        self.selectable
    }
    pub fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    pub fn render(&mut self, client_id: Option<ClientId>) -> Option<String> {
        match kind {
            PaneKind::Plugin => self.render_plugin(client_id),
            PaneKind::Terminal => self.render_terminal(client_id)
        }
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
    fn reflow_lines(&mut self) {
        let rows = self.rows();
        let cols = self.cols();
        self.grid.change_size(rows, cols);
        self.set_should_render(true);
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
    }
    fn render_pane(&mut self, _client_id: Option<ClientId>) -> Option<String> {
        // we don't use client_id because terminal panes render the same for all users
        if self.should_render() {
            let mut vte_output = String::new();
            let mut character_styles = CharacterStyles::new();
            let content_x = self.get_content_x();
            let content_y = self.get_content_y();
            if self.grid.clear_viewport_before_rendering {
                for line_index in 0..self.grid.height {
                    write!(
                        &mut vte_output,
                        "\u{1b}[{};{}H\u{1b}[m",
                        content_y + line_index + 1,
                        content_x + 1
                    )
                    .unwrap(); // goto row/col and reset styles
                    for _col_index in 0..self.grid.width {
                        vte_output.push(EMPTY_TERMINAL_CHARACTER.character);
                    }
                }
                self.grid.clear_viewport_before_rendering = false;
            }
            // here we clear the previous cursor locations by adding an empty style-less character
            // in their location, this is done before the main rendering logic so that if there
            // actually is another character there, it will be overwritten
            for (y, x) in self.fake_cursor_locations.drain() {
                // we need to make sure to update the line in the line buffer so that if there's
                // another character there it'll override it and we won't create holes with our
                // empty character
                self.grid.update_line_for_rendering(y);
                let x = content_x + x;
                let y = content_y + y;
                write!(
                    &mut vte_output,
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    y + 1,
                    x + 1,
                    EMPTY_TERMINAL_CHARACTER.character
                )
                .unwrap();
            }
            let max_width = self.get_content_columns();
            for character_chunk in self.grid.read_changes() {
                let pane_x = self.get_content_x();
                let pane_y = self.get_content_y();
                let chunk_absolute_x = pane_x + character_chunk.x;
                let chunk_absolute_y = pane_y + character_chunk.y;
                let terminal_characters = character_chunk.terminal_characters;
                write!(
                    &mut vte_output,
                    "\u{1b}[{};{}H\u{1b}[m",
                    chunk_absolute_y + 1,
                    chunk_absolute_x + 1
                )
                .unwrap(); // goto row/col and reset styles

                let mut chunk_width = character_chunk.x;
                for mut t_character in terminal_characters {
                    // adjust the background of currently selected characters
                    // doing it here is much easier than in grid
                    if self.grid.selection.contains(character_chunk.y, chunk_width) {
                        let color = match self.colors.bg {
                            PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                            PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                        };

                        t_character.styles = t_character.styles.background(Some(color));
                    }
                    chunk_width += t_character.width;
                    if chunk_width > max_width {
                        break;
                    }

                    if let Some(new_styles) = character_styles
                        .update_and_return_diff(&t_character.styles, self.grid.changed_colors)
                    {
                        write!(
                            &mut vte_output,
                            "{}{}",
                            new_styles,
                            self.grid.link_handler.output_osc8(new_styles.link_anchor)
                        )
                        .unwrap();
                    }

                    vte_output.push(t_character.character);
                }
                character_styles.clear();
            }
            if self.grid.ring_bell {
                let ring_bell = '\u{7}';
                vte_output.push(ring_bell);
                self.grid.ring_bell = false;
            }
            self.set_should_render(false);
            Some(vte_output)
        } else {
            None
        }
    }
    fn render_terminal(&mut self, client_id: Option<ClientId>) -> Option<String> {
        // this is a bit of a hack but works in a pinch
        client_id?;
        let client_id = client_id.unwrap();
        // if self.should_render {
        if true {
            // while checking should_render rather than rendering each pane every time
            // is more performant, it causes some problems when the pane to the left should be
            // rendered and has wide characters (eg. Chinese characters or emoji)
            // as a (hopefully) temporary hack, we render all panes until we find a better solution
            let mut vte_output = String::new();
            let (buf_tx, buf_rx) = channel();

            self.send_plugin_instructions
                .send(PluginInstruction::Render(
                    buf_tx,
                    self.pid,
                    client_id,
                    self.get_content_rows(),
                    self.get_content_columns(),
                ))
                .unwrap();

            self.should_render = false;
            let contents = buf_rx.recv().unwrap();
            for (index, line) in contents.lines().enumerate() {
                let actual_len = ansi_len(line);
                let line_to_print = if actual_len > self.get_content_columns() {
                    let mut line = String::from(line);
                    line.truncate(self.get_content_columns());
                    line
                } else {
                    [
                        line,
                        &str::repeat(" ", self.get_content_columns() - ansi_len(line)),
                    ]
                    .concat()
                };

                write!(
                    &mut vte_output,
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    self.get_content_y() + 1 + index,
                    self.get_content_x() + 1,
                    line_to_print,
                )
                .unwrap(); // goto row/col and reset styles
                let line_len = line_to_print.len();
                if line_len < self.get_content_columns() {
                    // pad line
                    for _ in line_len..self.get_content_columns() {
                        vte_output.push(' ');
                    }
                }
            }
            let total_line_count = contents.lines().count();
            if total_line_count < self.get_content_rows() {
                // pad lines
                for line_index in total_line_count..self.get_content_rows() {
                    let x = self.get_content_x();
                    let y = self.get_content_y();
                    write!(
                        &mut vte_output,
                        "\u{1b}[{};{}H\u{1b}[m",
                        y + line_index + 1,
                        x + 1
                    )
                    .unwrap(); // goto row/col and reset styles
                    for _col_index in 0..self.get_content_columns() {
                        vte_output.push(' ');
                    }
                }
            }
            Some(vte_output)
        } else {
            None
        }
    }
    fn render_frame(
        &mut self,
        _client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Option<String> {
        // FIXME: This is a hack that assumes all fixed-size panes are borderless. This
        // will eventually need fixing!
        if self.frame && !(self.geom.rows.is_fixed() || self.geom.cols.is_fixed()) {
            let pane_title = if self.pane_name.is_empty()
                && input_mode == InputMode::RenamePane
                && frame_params.is_main_client
            {
                String::from("Enter name...")
            } else if self.pane_name.is_empty() {
                self.pane_title.clone()
            } else {
                self.pane_name.clone()
            };
            let frame = PaneFrame::new(
                self.current_geom().into(),
                (0, 0), // scroll position
                pane_title,
                frame_params,
            );
            Some(frame.render())
        } else {
            None
        }
    }
    fn render_fake_cursor(
        &mut self,
        _cursor_color: PaletteColor,
        _text_color: PaletteColor,
    ) -> Option<String> {
        None
    }
    fn update_name(&mut self, name: &str) {
        match name {
            "\0" => {
                self.pane_name = String::new();
            }
            "\u{007F}" | "\u{0008}" => {
                //delete and backspace keys
                self.pane_name.pop();
            }
            c => {
                self.pane_name.push_str(c);
            }
        }
    }
    fn pid(&self) -> PaneId {
        PaneId::Plugin(self.pid)
    }
    fn reduce_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows = Dimension::percent(p - percent);
            self.should_render = true;
        }
    }
    fn increase_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows = Dimension::percent(p + percent);
            self.should_render = true;
        }
    }
    fn reduce_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols = Dimension::percent(p - percent);
            self.should_render = true;
        }
    }
    fn increase_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols = Dimension::percent(p + percent);
            self.should_render = true;
        }
    }
    fn push_down(&mut self, count: usize) {
        self.geom.y += count;
        self.should_render = true;
    }
    fn push_right(&mut self, count: usize) {
        self.geom.x += count;
        self.should_render = true;
    }
    fn pull_left(&mut self, count: usize) {
        self.geom.x -= count;
        self.should_render = true;
    }
    fn pull_up(&mut self, count: usize) {
        self.geom.y -= count;
        self.should_render = true;
    }
    fn scroll_up(&mut self, count: usize, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::ScrollUp(count)),
            ))
            .unwrap();
    }
    fn scroll_down(&mut self, count: usize, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::ScrollDown(count)),
            ))
            .unwrap();
    }
    fn clear_scroll(&mut self) {
        unimplemented!();
    }
    fn start_selection(&mut self, start: &Position, client_id: ClientId) {
        log::info!("plugin pane send left click plugin instruction");
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::LeftClick(start.line.0, start.column.0)),
            ))
            .unwrap();
    }
    fn update_selection(&mut self, position: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::Hold(position.line.0, position.column.0)),
            ))
            .unwrap();
    }
    fn end_selection(&mut self, end: Option<&Position>, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::Release(
                    end.map(|Position { line, column }| (line.0, column.0)),
                )),
            ))
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
    fn set_frame(&mut self, frame: bool) {
        self.frame = frame;
    }
    fn set_content_offset(&mut self, offset: Offset) {
        self.content_offset = offset;
    }
    fn set_borderless(&mut self, borderless: bool) {
        self.borderless = borderless;
    }
    fn borderless(&self) -> bool {
        self.borderless
    }
    fn handle_right_click(&mut self, to: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::RightClick(to.line.0, to.column.0)),
            ))
            .unwrap();
    }
}
