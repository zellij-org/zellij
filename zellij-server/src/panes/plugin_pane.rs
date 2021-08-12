use std::sync::mpsc::channel;
use std::time::Instant;
use std::unimplemented;

use crate::panes::{PaneDecoration, PaneId};
use crate::pty::VteBytes;
use crate::tab::Pane;
use crate::ui::pane_boundaries_frame::PaneBoundariesFrame;
use crate::wasm_vm::PluginInstruction;
use zellij_utils::shared::ansi_len;
use zellij_utils::zellij_tile::prelude::PaletteColor;
use zellij_utils::{channels::SenderWithContext, pane_size::PositionAndSize};

pub(crate) struct PluginPane {
    pub pid: u32,
    pub should_render: bool,
    pub selectable: bool,
    pub invisible_borders: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub content_position_and_size: PositionAndSize,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub active_at: Instant,
    pub pane_title: String,
    pane_decoration: PaneDecoration,
}

impl PluginPane {
    pub fn new(
        pid: u32,
        position_and_size: PositionAndSize,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        title: String,
    ) -> Self {
        Self {
            pid,
            should_render: true,
            selectable: true,
            invisible_borders: false,
            position_and_size,
            position_and_size_override: None,
            send_plugin_instructions,
            active_at: Instant::now(),
            pane_decoration: PaneDecoration::ContentOffset((0, 0)),
            content_position_and_size: position_and_size,
            pane_title: title,
        }
    }
    pub fn get_content_x(&self) -> usize {
        self.get_content_posision_and_size().x
    }
    pub fn get_content_y(&self) -> usize {
        self.get_content_posision_and_size().y
    }
    pub fn get_content_columns(&self) -> usize {
        // content columns might differ from the pane's columns if the pane has a frame
        // in that case they would be 2 less
        self.get_content_posision_and_size().cols
    }
    pub fn get_content_rows(&self) -> usize {
        // content rows might differ from the pane's rows if the pane has a frame
        // in that case they would be 2 less
        self.get_content_posision_and_size().rows
    }
    pub fn get_content_posision_and_size(&self) -> PositionAndSize {
        self.content_position_and_size
    }
    fn redistribute_space(&mut self) {
        let position_and_size = self
            .position_and_size_override
            .unwrap_or(self.position_and_size());
        match &mut self.pane_decoration {
            PaneDecoration::BoundariesFrame(boundaries_frame) => {
                boundaries_frame.change_pos_and_size(position_and_size);
                self.content_position_and_size = boundaries_frame.content_position_and_size();
            }
            PaneDecoration::ContentOffset((content_columns_offset, content_rows_offset)) => {
                self.content_position_and_size = position_and_size;
                self.content_position_and_size.cols =
                    position_and_size.cols - *content_columns_offset;
                self.content_position_and_size.rows = position_and_size.rows - *content_rows_offset;
            }
        };
        self.set_should_render(true);
    }
}

impl Pane for PluginPane {
    // FIXME: These position and size things should all be moved to default trait implementations,
    // with something like a get_pos_and_sz() method underpinning all of them. Alternatively and
    // preferably, just use an enum and not a trait object
    fn x(&self) -> usize {
        self.position_and_size_override
            .unwrap_or(self.position_and_size)
            .x
    }
    fn y(&self) -> usize {
        self.position_and_size_override
            .unwrap_or(self.position_and_size)
            .y
    }
    fn rows(&self) -> usize {
        self.position_and_size_override
            .unwrap_or(self.position_and_size)
            .rows
    }
    fn columns(&self) -> usize {
        self.position_and_size_override
            .unwrap_or(self.position_and_size)
            .cols
    }
    fn get_content_columns(&self) -> usize {
        self.get_content_columns()
    }
    fn get_content_rows(&self) -> usize {
        self.get_content_rows()
    }
    fn reset_size_and_position_override(&mut self) {
        self.position_and_size_override = None;
        self.redistribute_space();
        self.should_render = true;
    }
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size = *position_and_size;
        self.redistribute_space();
    }
    // FIXME: This is obviously a bit outdated and needs the x and y moved into `size`
    fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize) {
        let position_and_size_override = PositionAndSize {
            x,
            y,
            rows: size.rows,
            cols: size.cols,
            ..Default::default()
        };
        self.position_and_size_override = Some(position_and_size_override);
        self.redistribute_space();
    }
    fn handle_pty_bytes(&mut self, _event: VteBytes) {
        unimplemented!()
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        None
    }
    fn adjust_input_to_terminal(&self, _input_bytes: Vec<u8>) -> Vec<u8> {
        unimplemented!() // FIXME: Shouldn't need this implmented?
    }
    fn position_and_size(&self) -> PositionAndSize {
        self.position_and_size
    }
    fn position_and_size_override(&self) -> Option<PositionAndSize> {
        self.position_and_size_override
    }
    fn should_render(&self) -> bool {
        self.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.should_render = should_render;
    }
    fn set_should_render_boundaries(&mut self, should_render: bool) {
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            boundaries_frame.set_should_render(should_render);
        }
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn set_invisible_borders(&mut self, invisible_borders: bool) {
        self.invisible_borders = invisible_borders;
    }
    fn set_fixed_height(&mut self, fixed_height: usize) {
        self.position_and_size.rows = fixed_height;
        self.position_and_size.rows_fixed = true;
    }
    fn set_fixed_width(&mut self, fixed_width: usize) {
        self.position_and_size.cols = fixed_width;
        self.position_and_size.cols_fixed = true;
    }
    fn render(&mut self) -> Option<String> {
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
                    self.get_content_rows(),
                    self.get_content_columns(),
                ))
                .unwrap();

            self.should_render = false;
            let contents = buf_rx.recv().unwrap();
            if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
                if let Some(boundaries_frame_vte) = boundaries_frame.render() {
                    vte_output.push_str(&boundaries_frame_vte);
                }
            }
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

                vte_output.push_str(&format!(
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    self.get_content_y() + 1 + index,
                    self.get_content_x() + 1,
                    line_to_print,
                )); // goto row/col and reset styles
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
                    vte_output.push_str(&format!(
                        "\u{1b}[{};{}H\u{1b}[m",
                        y + line_index + 1,
                        x + 1
                    )); // goto row/col and reset styles
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
    fn pid(&self) -> PaneId {
        PaneId::Plugin(self.pid)
    }
    fn reduce_height_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.position_and_size.rows -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.cols -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.cols -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.cols += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.cols += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn push_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn push_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn pull_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn pull_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.redistribute_space();
        self.should_render = true;
    }
    fn scroll_up(&mut self, _count: usize) {
        //unimplemented!()
    }
    fn scroll_down(&mut self, _count: usize) {
        //unimplemented!()
    }
    fn clear_scroll(&mut self) {
        //unimplemented!()
    }
    // FIXME: This need to be reevaluated and deleted if possible.
    // `max` doesn't make sense when things are fixed...
    fn max_height(&self) -> Option<usize> {
        if self.position_and_size.rows_fixed {
            Some(self.position_and_size.rows)
        } else {
            None
        }
    }
    fn max_width(&self) -> Option<usize> {
        if self.position_and_size.cols_fixed {
            Some(self.position_and_size.cols)
        } else {
            None
        }
    }
    fn invisible_borders(&self) -> bool {
        self.invisible_borders
    }

    fn active_at(&self) -> Instant {
        self.active_at
    }

    fn set_active_at(&mut self, time: Instant) {
        self.active_at = time;
    }
    fn set_boundary_color(&mut self, color: Option<PaletteColor>) {
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            boundaries_frame.set_color(color);
        }
    }
    fn offset_content_columns(&mut self, by: usize) {
        if !self.selectable {
            return;
        }
        if let PaneDecoration::ContentOffset(content_offset) = &mut self.pane_decoration {
            content_offset.0 = by;
        } else {
            self.pane_decoration = PaneDecoration::ContentOffset((by, 0));
        }
        self.redistribute_space();
        self.set_should_render(true);
    }
    fn offset_content_rows(&mut self, by: usize) {
        if !self.selectable {
            return;
        }
        if let PaneDecoration::ContentOffset(content_offset) = &mut self.pane_decoration {
            content_offset.1 = by;
        } else {
            self.pane_decoration = PaneDecoration::ContentOffset((0, by));
        }
        self.redistribute_space();
        self.set_should_render(true);
    }
    fn show_boundaries_frame(&mut self, should_render_only_title: bool) {
        if !self.selectable {
            return;
        }
        let position_and_size = self
            .position_and_size_override
            .unwrap_or(self.position_and_size);
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            boundaries_frame.render_only_title(should_render_only_title);
            self.content_position_and_size = boundaries_frame.content_position_and_size();
        } else {
            let mut boundaries_frame =
                PaneBoundariesFrame::new(position_and_size, self.pane_title.clone());
            boundaries_frame.render_only_title(should_render_only_title);
            self.content_position_and_size = boundaries_frame.content_position_and_size();
            self.pane_decoration = PaneDecoration::BoundariesFrame(boundaries_frame);
        }
        self.redistribute_space();
        self.set_should_render(true);
    }
    fn remove_boundaries_frame(&mut self) {
        if !self.selectable {
            return;
        }
        self.pane_decoration = PaneDecoration::ContentOffset((0, 0));
        self.redistribute_space();
        self.set_should_render(true);
    }
}
