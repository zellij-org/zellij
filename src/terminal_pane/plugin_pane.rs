#![allow(clippy::clippy::if_same_then_else)]

use crate::{
    pty_bus::VteEvent, tab::Pane, utils::shared::*, wasm_vm::PluginInstruction, SenderWithContext,
};

use std::{iter, os::unix::prelude::RawFd, sync::mpsc::channel};

use crate::terminal_pane::PositionAndSize;

pub struct PluginPane {
    pub pid: u32,
    pub should_render: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
}

impl PluginPane {
    pub fn new(
        pid: u32,
        position_and_size: PositionAndSize,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
    ) -> Self {
        Self {
            pid,
            should_render: true,
            position_and_size,
            position_and_size_override: None,
            send_plugin_instructions,
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
            .columns
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
            columns: size.columns,
        };
        self.position_and_size_override = Some(position_and_size_override);
        self.should_render = true;
    }
    fn handle_event(&mut self, event: VteEvent) {
        todo!()
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        None
    }
    fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8> {
        todo!() // FIXME: Shouldn't need this implmented?
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
    fn render(&mut self) -> Option<String> {
        // if self.should_render {
        if true {
            // while checking should_render rather than rendering each pane every time
            // is more performant, it causes some problems when the pane to the left should be
            // rendered and has wide characters (eg. Chinese characters or emoji)
            // as a (hopefully) temporary hack, we render all panes until we find a better solution
            let (buf_tx, buf_rx) = channel();

            self.send_plugin_instructions
                .send(PluginInstruction::Draw(
                    buf_tx,
                    self.pid,
                    self.rows(),
                    self.columns(),
                ))
                .unwrap();

            self.should_render = false;
            let goto_plugin_coordinates = format!("\u{1b}[{};{}H", self.y() + 1, self.x() + 1); // goto row/col and reset styles
            let reset_styles = "\u{1b}[m";
            let buf = pad_to_size(&buf_rx.recv().unwrap(), self.rows(), self.columns());
            let vte_output = format!("{}{}{}", goto_plugin_coordinates, reset_styles, buf);
            Some(vte_output)
        } else {
            None
        }
    }
    // FIXME: Really shouldn't be in this trait...
    fn pid(&self) -> RawFd {
        // FIXME: This looks like a really bad idea!
        100 + self.pid as RawFd
    }
    fn reduce_height_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.position_and_size.rows -= count;
        self.should_render = true;
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.should_render = true;
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.should_render = true;
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.should_render = true;
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.columns -= count;
        self.should_render = true;
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.columns -= count;
        self.should_render = true;
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.columns += count;
        self.should_render = true;
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.columns += count;
        self.should_render = true;
    }
    fn scroll_up(&mut self, count: usize) {
        todo!()
    }
    fn scroll_down(&mut self, count: usize) {
        todo!()
    }
    fn clear_scroll(&mut self) {
        todo!()
    }
}
