use std::fmt::Write;
use std::sync::mpsc::channel;
use std::time::Instant;

use crate::output::CharacterChunk;
use crate::panes::PaneId;
use crate::pty::VteBytes;
use crate::tab::Pane;
use crate::ui::pane_boundaries_frame::{FrameParams, PaneFrame};
use crate::wasm_vm::PluginInstruction;
use crate::ClientId;
use zellij_utils::pane_size::Offset;
use zellij_utils::position::Position;
use zellij_utils::shared::ansi_len;
use zellij_utils::zellij_tile::prelude::{Event, InputMode, Mouse, PaletteColor};
use zellij_utils::{
    channels::SenderWithContext,
    pane_size::{Dimension, PaneGeom},
    shared::make_terminal_title,
};

pub(crate) struct PluginPane {
    pub pid: u32,
    pub should_render: bool,
    pub selectable: bool,
    pub geom: PaneGeom,
    pub geom_override: Option<PaneGeom>,
    pub content_offset: Offset,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub active_at: Instant,
    pub pane_title: String,
    pub pane_name: String,
    frame: bool,
    borderless: bool,
}

impl PluginPane {
    pub fn new(
        pid: u32,
        position_and_size: PaneGeom,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        title: String,
        pane_name: String,
    ) -> Self {
        Self {
            pid,
            should_render: true,
            selectable: true,
            geom: position_and_size,
            geom_override: None,
            send_plugin_instructions,
            active_at: Instant::now(),
            frame: false,
            content_offset: Offset::default(),
            pane_title: title,
            borderless: false,
            pane_name,
        }
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
        self.should_render = true;
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.geom = position_and_size;
        self.should_render = true;
    }
    fn get_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
        self.should_render = true;
    }
    fn handle_pty_bytes(&mut self, _event: VteBytes) {
        // noop
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        None
    }
    fn adjust_input_to_terminal(&self, _input_bytes: Vec<u8>) -> Vec<u8> {
        // noop
        vec![]
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
        self.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.should_render = should_render;
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
    ) -> Option<(Vec<CharacterChunk>, Option<String>)> {
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
            Some((vec![], Some(vte_output))) // TODO: PluginPanes should have their own grid so that we can return the non-serialized TerminalCharacters and have them participate in the render buffer
        } else {
            None
        }
    }
    fn render_frame(
        &mut self,
        _client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Option<(Vec<CharacterChunk>, Option<String>)> {
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
        // noop
    }
    fn start_selection(&mut self, start: &Position, client_id: ClientId) {
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
    fn end_selection(&mut self, end: &Position, client_id: ClientId) {
        self.send_plugin_instructions
            .send(PluginInstruction::Update(
                Some(self.pid),
                Some(client_id),
                Event::Mouse(Mouse::Release(end.line(), end.column())),
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
