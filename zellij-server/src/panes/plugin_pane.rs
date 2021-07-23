use std::sync::mpsc::channel;
use std::time::Instant;
use std::unimplemented;

use crate::panes::PaneId;
use crate::pty::VteBytes;
use crate::tab::Pane;
use crate::wasm_vm::PluginInstruction;
use zellij_utils::{channels::SenderWithContext, pane_size::PositionAndSize};
use zellij_utils::zellij_tile::prelude::PaletteColor;
use zellij_utils::logging::debug_log_to_file;
use crate::ui::pane_boundaries_frame::PaneBoundariesFrame;

pub(crate) struct PluginPane {
    pub pid: u32,
    pub should_render: bool,
    pub selectable: bool,
    pub invisible_borders: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub contents_position_and_size: PositionAndSize, // TODO: do we need this?
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub active_at: Instant,
    boundaries_frame: Option<PaneBoundariesFrame>,
}

impl PluginPane {
    pub fn new(
        pid: u32,
        position_and_size: PositionAndSize,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        title: String,
        draw_boundaries_frame: bool,
        frame_title_only: bool,
    ) -> Self {
        let (boundaries_frame, contents_position_and_size) = if draw_boundaries_frame {
            if frame_title_only {
                if position_and_size.rows > 2 && position_and_size.cols > 2 {
                    // hacky hacky, hack hack hack
                    let boundaries_frame = Some(PaneBoundariesFrame::new(position_and_size, title).frame_title_only());
                    let contents_position_and_size = position_and_size.reduce_top_line();
                    (boundaries_frame, contents_position_and_size)
                } else {
                    let boundaries_frame = None;
                    let contents_position_and_size = position_and_size;
                    (boundaries_frame, contents_position_and_size)
                }
            } else {
                if position_and_size.rows > 2 && position_and_size.cols > 2 {
                    // hacky hacky, hack hack hack
                    let boundaries_frame = Some(PaneBoundariesFrame::new(position_and_size, title));
                    let contents_position_and_size = position_and_size.reduce_outer_frame(1);
                    (boundaries_frame, contents_position_and_size)
                } else {
                    let boundaries_frame = None;
                    let contents_position_and_size = position_and_size;
                    (boundaries_frame, contents_position_and_size)
                }
            }
        } else {
            let boundaries_frame = None;
            let contents_position_and_size = position_and_size;
            (boundaries_frame, contents_position_and_size)
        };
        Self {
            pid,
            should_render: true,
            selectable: true,
            invisible_borders: false,
            position_and_size,
            position_and_size_override: None,
            send_plugin_instructions,
            active_at: Instant::now(),
            boundaries_frame,
            contents_position_and_size,
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
        match (self.boundaries_frame.as_ref(), self.position_and_size_override.as_ref()) {
            (Some(boundaries_frame), _) => {
                // boundaries_frame.position_and_size.reduce_outer_frame(1)
                boundaries_frame.content_position_and_size()
            }
            (None, Some(position_and_size_override)) => *position_and_size_override,
            _ => self.position_and_size,
        }
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
    fn reset_size_and_position_override(&mut self) {
        self.position_and_size_override = None;
        self.should_render = true;
    }
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size = *position_and_size;
        self.should_render = true;
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
        self.should_render = true;
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
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
        if !selectable {
            self.boundaries_frame = None;
            self.contents_position_and_size = self.position_and_size;
            // TODO: position_and_size_override?
        } else {
            // TBD
        }
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
//                     self.rows(),
//                     self.columns(),
                    self.get_content_rows(),
                    self.get_content_columns(),
                ))
                .unwrap();

            self.should_render = false;
            // Some(buf_rx.recv().unwrap())
            let contents = buf_rx.recv().unwrap();
            if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
                vte_output.push_str(&boundaries_frame.render());
            }
            for (index, line) in contents.lines().enumerate() {
                // TODO: adjust to size (was removed from tab render)
                vte_output.push_str(&format!(
                    "\u{1b}[{};{}H\u{1b}[m{}",
                    self.get_content_y() + 1 + index,
                    self.get_content_x() + 1,
                    line,
                )); // goto row/col and reset styles
                let line_len = line.chars().count();
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
            // vte_output.push_str(&contents);
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
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.cols -= count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.cols -= count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.cols += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.cols += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
        self.should_render = true;
    }
    fn push_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
    }
    fn push_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
    }
    fn pull_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
    }
    fn pull_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            boundaries_frame.change_pos_and_size(self.position_and_size);
        }
    }
    fn scroll_up(&mut self, _count: usize) {
        unimplemented!()
    }
    fn scroll_down(&mut self, _count: usize) {
        unimplemented!()
    }
    fn clear_scroll(&mut self) {
        unimplemented!()
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
        if let Some(boundaries_frame) = self.boundaries_frame.as_mut() {
            if boundaries_frame.color != color {
                boundaries_frame.set_color(color);
                self.should_render = true;
            }
        }
    }
}
