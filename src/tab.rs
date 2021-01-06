use crate::pty_bus::{PtyInstruction, VteEvent};
use crate::terminal_pane::{PositionAndSize, TerminalPane};
use crate::{boundaries::Boundaries, terminal_pane::PluginPane};
use crate::{layout::Layout, wasm_vm::PluginInstruction};
use crate::{os_input_output::OsApi, utils::shared::pad_to_size};
use crate::{AppInstruction, SenderWithContext};
use std::collections::{BTreeMap, HashSet};
use std::os::unix::io::RawFd;
use std::{io::Write, sync::mpsc::channel};

/*
 * Tab
 *
 * this holds multiple panes (currently terminal panes) which are currently displayed
 * when this tab is active.
 * it tracks their coordinates (x/y) and size, as well as how they should be resized
 *
 */

const CURSOR_HEIGHT_WIDTH_RATIO: usize = 4; // this is not accurate and kind of a magic number, TODO: look into this

type BorderAndPaneIds = (usize, Vec<RawFd>);

fn split_vertically_with_gap(rect: &PositionAndSize) -> (PositionAndSize, PositionAndSize) {
    let width_of_each_half = (rect.columns - 1) / 2;
    let mut first_rect = *rect;
    let mut second_rect = *rect;
    if rect.columns % 2 == 0 {
        first_rect.columns = width_of_each_half + 1;
    } else {
        first_rect.columns = width_of_each_half;
    }
    second_rect.x = first_rect.x + first_rect.columns + 1;
    second_rect.columns = width_of_each_half;
    (first_rect, second_rect)
}

fn split_horizontally_with_gap(rect: &PositionAndSize) -> (PositionAndSize, PositionAndSize) {
    let height_of_each_half = (rect.rows - 1) / 2;
    let mut first_rect = *rect;
    let mut second_rect = *rect;
    if rect.rows % 2 == 0 {
        first_rect.rows = height_of_each_half + 1;
    } else {
        first_rect.rows = height_of_each_half;
    }
    second_rect.y = first_rect.y + first_rect.rows + 1;
    second_rect.rows = height_of_each_half;
    (first_rect, second_rect)
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy)]
enum PaneKind {
    Terminal(RawFd),
    PluginPane(u32), // FIXME: Drop the trait object, make this a wrapper for the struct?
    BuiltInPane(u32),
}
pub struct Tab {
    pub index: usize,
    panes: BTreeMap<PaneKind, Box<dyn Pane>>,
    panes_to_hide: HashSet<RawFd>,
    active_terminal: Option<RawFd>,
    max_panes: Option<usize>,
    full_screen_ws: PositionAndSize,
    fullscreen_is_active: bool,
    os_api: Box<dyn OsApi>,
    pub send_pty_instructions: SenderWithContext<PtyInstruction>,
    pub send_plugin_instructions: SenderWithContext<PluginInstruction>,
    pub send_app_instructions: SenderWithContext<AppInstruction>,
}

pub trait Pane {
    fn x(&self) -> usize;
    fn y(&self) -> usize;
    fn rows(&self) -> usize;
    fn columns(&self) -> usize;
    fn reset_size_and_position_override(&mut self);
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize);
    fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize);
    fn handle_event(&mut self, event: VteEvent);
    fn cursor_coordinates(&self) -> Option<(usize, usize)>;
    fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8>;

    fn position_and_size_override(&self) -> Option<PositionAndSize>;
    fn should_render(&self) -> bool;
    fn set_should_render(&mut self, should_render: bool);
    fn render(&mut self) -> Option<String>;
    fn pid(&self) -> RawFd;
    fn reduce_height_down(&mut self, count: usize);
    fn increase_height_down(&mut self, count: usize);
    fn increase_height_up(&mut self, count: usize);
    fn reduce_height_up(&mut self, count: usize);
    fn increase_width_right(&mut self, count: usize);
    fn reduce_width_right(&mut self, count: usize);
    fn reduce_width_left(&mut self, count: usize);
    fn increase_width_left(&mut self, count: usize);
    fn scroll_up(&mut self, count: usize);
    fn scroll_down(&mut self, count: usize);
    fn clear_scroll(&mut self);

    fn right_boundary_x_coords(&self) -> usize {
        self.x() + self.columns()
    }
    fn bottom_boundary_y_coords(&self) -> usize {
        self.y() + self.rows()
    }
    fn is_directly_right_of(&self, other: &Box<dyn Pane>) -> bool {
        self.x() == other.x() + other.columns() + 1
    }
    fn is_directly_left_of(&self, other: &Box<dyn Pane>) -> bool {
        self.x() + self.columns() + 1 == other.x()
    }
    fn is_directly_below(&self, other: &Box<dyn Pane>) -> bool {
        self.y() == other.y() + other.rows() + 1
    }
    fn is_directly_above(&self, other: &Box<dyn Pane>) -> bool {
        self.y() + self.rows() + 1 == other.y()
    }
    fn horizontally_overlaps_with(&self, other: &Box<dyn Pane>) -> bool {
        (self.y() >= other.y() && self.y() <= (other.y() + other.rows()))
            || ((self.y() + self.rows()) <= (other.y() + other.rows())
                && (self.y() + self.rows()) > other.y())
            || (self.y() <= other.y() && (self.y() + self.rows() >= (other.y() + other.rows())))
            || (other.y() <= self.y() && (other.y() + other.rows() >= (self.y() + self.rows())))
    }
    fn get_horizontal_overlap_with(&self, other: &Box<dyn Pane>) -> usize {
        std::cmp::min(self.y() + self.rows(), other.y() + other.rows())
            - std::cmp::max(self.y(), other.y())
    }
    fn vertically_overlaps_with(&self, other: &Box<dyn Pane>) -> bool {
        (self.x() >= other.x() && self.x() <= (other.x() + other.columns()))
            || ((self.x() + self.columns()) <= (other.x() + other.columns())
                && (self.x() + self.columns()) > other.x())
            || (self.x() <= other.x()
                && (self.x() + self.columns() >= (other.x() + other.columns())))
            || (other.x() <= self.x()
                && (other.x() + other.columns() >= (self.x() + self.columns())))
    }
    fn get_vertical_overlap_with(&self, other: &Box<dyn Pane>) -> usize {
        std::cmp::min(self.x() + self.columns(), other.x() + other.columns())
            - std::cmp::max(self.x(), other.x())
    }
}

impl Tab {
    pub fn new(
        index: usize,
        full_screen_ws: &PositionAndSize,
        mut os_api: Box<dyn OsApi>,
        send_pty_instructions: SenderWithContext<PtyInstruction>,
        send_plugin_instructions: SenderWithContext<PluginInstruction>,
        send_app_instructions: SenderWithContext<AppInstruction>,
        max_panes: Option<usize>,
        pane_id: Option<RawFd>,
    ) -> Self {
        let panes = if let Some(pid) = pane_id {
            let new_terminal = TerminalPane::new(pid, *full_screen_ws);
            os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.columns() as u16,
                new_terminal.rows() as u16,
            );
            let mut panes: BTreeMap<PaneKind, Box<dyn Pane>> = BTreeMap::new();
            panes.insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            panes
        } else {
            BTreeMap::new()
        };
        Tab {
            index: index,
            panes,
            max_panes,
            panes_to_hide: HashSet::new(),
            active_terminal: pane_id,
            full_screen_ws: *full_screen_ws,
            fullscreen_is_active: false,
            os_api,
            send_app_instructions,
            send_pty_instructions,
            send_plugin_instructions,
        }
    }

    pub fn apply_layout(&mut self, layout: Layout, new_pids: Vec<RawFd>) {
        // TODO: this should be an attribute on Screen instead of full_screen_ws
        let free_space = PositionAndSize {
            x: 0,
            y: 0,
            rows: self.full_screen_ws.rows,
            columns: self.full_screen_ws.columns,
        };
        self.panes_to_hide.clear();
        let positions_in_layout = layout.position_panes_in_space(&free_space);
        let mut positions_and_size = positions_in_layout.iter();
        for (pane_kind, terminal_pane) in self.panes.iter_mut() {
            // for now the layout only supports terminal panes
            if let PaneKind::Terminal(pid) = pane_kind {
                match positions_and_size.next() {
                    Some((_, position_and_size)) => {
                        terminal_pane.reset_size_and_position_override();
                        terminal_pane.change_pos_and_size(&position_and_size);
                        self.os_api.set_terminal_size_using_fd(
                            *pid,
                            position_and_size.columns as u16,
                            position_and_size.rows as u16,
                        );
                    }
                    None => {
                        // we filled the entire layout, no room for this pane
                        // TODO: handle active terminal
                        self.panes_to_hide.insert(*pid);
                    }
                }
            }
        }
        let mut new_pids = new_pids.iter();
        for (layout, position_and_size) in positions_and_size {
            // Just a regular terminal
            if let Some(plugin) = &layout.plugin {
                let (pid_tx, pid_rx) = channel();
                self.send_plugin_instructions
                    .send(PluginInstruction::Load(pid_tx, plugin.clone()))
                    .unwrap();
                let pid = pid_rx.recv().unwrap();
                let new_plugin = PluginPane::new(
                    pid,
                    *position_and_size,
                    self.send_plugin_instructions.clone(),
                );
                self.panes
                    .insert(PaneKind::PluginPane(pid), Box::new(new_plugin));
            } else {
                // there are still panes left to fill, use the pids we received in this method
                let pid = new_pids.next().unwrap(); // if this crashes it means we got less pids than there are panes in this layout
                let new_terminal = TerminalPane::new(*pid, *position_and_size);
                self.os_api.set_terminal_size_using_fd(
                    new_terminal.pid,
                    new_terminal.columns() as u16,
                    new_terminal.rows() as u16,
                );
                self.panes
                    .insert(PaneKind::Terminal(*pid), Box::new(new_terminal));
            }
        }
        for unused_pid in new_pids {
            // this is a bit of a hack and happens because we don't have any central location that
            // can query the screen as to how many panes it needs to create a layout
            // fixing this will require a bit of an architecture change
            self.send_pty_instructions
                .send(PtyInstruction::ClosePane(*unused_pid))
                .unwrap();
        }
        self.active_terminal = self
            .panes
            .iter()
            .filter_map(|(pane_kind, _)| match pane_kind {
                PaneKind::Terminal(pid) => Some(*pid),
                _ => None,
            })
            .next();
        self.render();
    }
    pub fn new_pane(&mut self, pid: RawFd) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_terminal_fullscreen();
        }
        if !self.has_terminal_panes() {
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.columns() as u16,
                new_terminal.rows() as u16,
            );
            self.panes
                .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal

            let (_longest_edge, terminal_id_to_split) = self.get_terminals().fold(
                (0, 0),
                |(current_longest_edge, current_terminal_id_to_split), id_and_terminal_to_check| {
                    let (id_of_terminal_to_check, terminal_to_check) = id_and_terminal_to_check;
                    let terminal_size = (terminal_to_check.rows() * CURSOR_HEIGHT_WIDTH_RATIO)
                        * terminal_to_check.columns();
                    if terminal_size > current_longest_edge {
                        (terminal_size, id_of_terminal_to_check)
                    } else {
                        (current_longest_edge, current_terminal_id_to_split)
                    }
                },
            );
            let terminal_to_split = self
                .panes
                .get_mut(&PaneKind::Terminal(terminal_id_to_split))
                .unwrap();
            let terminal_ws = PositionAndSize {
                rows: terminal_to_split.rows(),
                columns: terminal_to_split.columns(),
                x: terminal_to_split.x(),
                y: terminal_to_split.y(),
            };
            if terminal_to_split.rows() * CURSOR_HEIGHT_WIDTH_RATIO > terminal_to_split.columns() {
                let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&terminal_ws);
                let new_terminal = TerminalPane::new(pid, bottom_winsize);
                self.os_api.set_terminal_size_using_fd(
                    new_terminal.pid,
                    bottom_winsize.columns as u16,
                    bottom_winsize.rows as u16,
                );
                terminal_to_split.change_pos_and_size(&top_winsize);
                self.panes
                    .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
                self.os_api.set_terminal_size_using_fd(
                    terminal_id_to_split,
                    top_winsize.columns as u16,
                    top_winsize.rows as u16,
                );
                self.active_terminal = Some(pid);
            } else {
                let (left_winszie, right_winsize) = split_vertically_with_gap(&terminal_ws);
                let new_terminal = TerminalPane::new(pid, right_winsize);
                self.os_api.set_terminal_size_using_fd(
                    new_terminal.pid,
                    right_winsize.columns as u16,
                    right_winsize.rows as u16,
                );
                terminal_to_split.change_pos_and_size(&left_winszie);
                self.panes
                    .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
                self.os_api.set_terminal_size_using_fd(
                    terminal_id_to_split,
                    left_winszie.columns as u16,
                    left_winszie.rows as u16,
                );
            }
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    pub fn horizontal_split(&mut self, pid: RawFd) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_terminal_fullscreen();
        }
        if !self.has_terminal_panes() {
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.columns() as u16,
                new_terminal.rows() as u16,
            );
            self.panes
                .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let active_terminal_id = &self.get_active_terminal_id().unwrap();
            let active_terminal = self
                .panes
                .get_mut(&PaneKind::Terminal(*active_terminal_id))
                .unwrap();
            let terminal_ws = PositionAndSize {
                x: active_terminal.x(),
                y: active_terminal.y(),
                rows: active_terminal.rows(),
                columns: active_terminal.columns(),
            };
            let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&terminal_ws);
            let new_terminal = TerminalPane::new(pid, bottom_winsize);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                bottom_winsize.columns as u16,
                bottom_winsize.rows as u16,
            );

            active_terminal.change_pos_and_size(&top_winsize);

            self.panes
                .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_pid,
                top_winsize.columns as u16,
                top_winsize.rows as u16,
            );
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    pub fn vertical_split(&mut self, pid: RawFd) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_terminal_fullscreen();
        }
        if !self.has_terminal_panes() {
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.columns() as u16,
                new_terminal.rows() as u16,
            );
            self.panes
                .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let active_terminal_id = &self.get_active_terminal_id().unwrap();
            let active_terminal = self
                .panes
                .get_mut(&PaneKind::Terminal(*active_terminal_id))
                .unwrap();
            let terminal_ws = PositionAndSize {
                x: active_terminal.x(),
                y: active_terminal.y(),
                rows: active_terminal.rows(),
                columns: active_terminal.columns(),
            };
            let (left_winszie, right_winsize) = split_vertically_with_gap(&terminal_ws);
            let new_terminal = TerminalPane::new(pid, right_winsize);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                right_winsize.columns as u16,
                right_winsize.rows as u16,
            );

            active_terminal.change_pos_and_size(&left_winszie);

            self.panes
                .insert(PaneKind::Terminal(pid), Box::new(new_terminal));
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_pid,
                left_winszie.columns as u16,
                left_winszie.rows as u16,
            );
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    pub fn get_active_terminal(&self) -> Option<&Box<dyn Pane>> {
        match self.active_terminal {
            Some(active_terminal) => self.panes.get(&PaneKind::Terminal(active_terminal)),
            None => None,
        }
    }
    fn get_active_terminal_id(&self) -> Option<RawFd> {
        self.active_terminal
    }
    pub fn handle_pty_event(&mut self, pid: RawFd, event: VteEvent) {
        // if we don't have the terminal in self.terminals it's probably because
        // of a race condition where the terminal was created in pty_bus but has not
        // yet been created in Screen. These events are currently not buffered, so
        // if you're debugging seemingly randomly missing stdout data, this is
        // the reason
        if let Some(terminal_output) = self.panes.get_mut(&PaneKind::Terminal(pid)) {
            terminal_output.handle_event(event);
        }
    }
    pub fn write_to_active_terminal(&mut self, input_bytes: Vec<u8>) {
        if let Some(active_terminal_id) = &self.get_active_terminal_id() {
            let active_terminal = self.get_active_terminal().unwrap();
            let mut adjusted_input = active_terminal.adjust_input_to_terminal(input_bytes);
            self.os_api
                .write_to_tty_stdin(*active_terminal_id, &mut adjusted_input)
                .expect("failed to write to terminal");
            self.os_api
                .tcdrain(*active_terminal_id)
                .expect("failed to drain terminal");
        }
    }
    pub fn get_active_terminal_cursor_position(&self) -> Option<(usize, usize)> {
        // (x, y)
        let active_terminal = &self.get_active_terminal().unwrap();
        active_terminal
            .cursor_coordinates()
            .map(|(x_in_terminal, y_in_terminal)| {
                let x = active_terminal.x() + x_in_terminal;
                let y = active_terminal.y() + y_in_terminal;
                (x, y)
            })
    }
    pub fn toggle_active_terminal_fullscreen(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self
                .get_active_terminal()
                .unwrap()
                .position_and_size_override()
                .is_some()
            {
                for terminal_id in self.panes_to_hide.iter() {
                    self.panes
                        .get_mut(&PaneKind::Terminal(*terminal_id))
                        .unwrap()
                        .set_should_render(true);
                }
                self.panes_to_hide.clear();
                let active_terminal = self
                    .panes
                    .get_mut(&PaneKind::Terminal(active_terminal_id))
                    .unwrap();
                active_terminal.reset_size_and_position_override();
            } else {
                let terminals = self.get_terminals();
                let all_ids_except_current = terminals.filter_map(|(id, _)| {
                    if id != active_terminal_id {
                        Some(id)
                    } else {
                        None
                    }
                });
                self.panes_to_hide = all_ids_except_current.collect();
                let active_terminal = self
                    .panes
                    .get_mut(&PaneKind::Terminal(active_terminal_id))
                    .unwrap();
                active_terminal.override_size_and_position(0, 0, &self.full_screen_ws);
            }
            let active_terminal = self
                .panes
                .get(&PaneKind::Terminal(active_terminal_id))
                .unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_id,
                active_terminal.columns() as u16,
                active_terminal.rows() as u16,
            );
            self.render();
            self.toggle_fullscreen_is_active();
        }
    }
    pub fn toggle_fullscreen_is_active(&mut self) {
        self.fullscreen_is_active = !self.fullscreen_is_active;
    }
    pub fn render(&mut self) {
        if self.active_terminal.is_none() {
            // we might not have an active terminal if we closed the last pane
            // in that case, we should not render as the app is exiting
            return;
        }
        let mut stdout = self.os_api.get_stdout_writer();
        let mut boundaries = Boundaries::new(
            self.full_screen_ws.columns as u16,
            self.full_screen_ws.rows as u16,
        );
        for (_, terminal) in self.panes.iter_mut() {
            if !self.panes_to_hide.contains(&terminal.pid()) {
                boundaries.add_rect(&terminal);
                if let Some(vte_output) = terminal.render() {
                    // FIXME: Use Termion for cursor and style clearing?
                    write!(
                        stdout,
                        "\u{1b}[{};{}H\u{1b}[m{}",
                        terminal.y() + 1,
                        terminal.x() + 1,
                        pad_to_size(&vte_output, terminal.rows(), terminal.columns())
                    )
                    .expect("cannot write to stdout");
                }
            }
        }

        // TODO: only render (and calculate) boundaries if there was a resize
        let vte_output = boundaries.vte_output();
        stdout
            .write_all(&vte_output.as_bytes())
            .expect("cannot write to stdout");

        match self.get_active_terminal_cursor_position() {
            Some((cursor_position_x, cursor_position_y)) => {
                let show_cursor = "\u{1b}[?25h";
                let goto_cursor_position = format!(
                    "\u{1b}[{};{}H\u{1b}[m",
                    cursor_position_y + 1,
                    cursor_position_x + 1
                ); // goto row/col
                stdout
                    .write_all(&show_cursor.as_bytes())
                    .expect("cannot write to stdout");
                stdout
                    .write_all(&goto_cursor_position.as_bytes())
                    .expect("cannot write to stdout");
                stdout.flush().expect("could not flush");
            }
            None => {
                let hide_cursor = "\u{1b}[?25l";
                stdout
                    .write_all(&hide_cursor.as_bytes())
                    .expect("cannot write to stdout");
                stdout.flush().expect("could not flush");
            }
        }
    }
    fn get_terminals(&self) -> impl Iterator<Item = (RawFd, &Box<dyn Pane>)> {
        self.panes
            .iter()
            .filter_map(|(pane_kind, terminal_pane)| match pane_kind {
                PaneKind::Terminal(pid) => Some((*pid, terminal_pane)),
                _ => None,
            })
    }
    fn has_terminal_panes(&self) -> bool {
        let mut all_terminals = self.get_terminals();
        all_terminals.next().is_some()
    }
    fn terminal_ids_directly_left_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        if terminal_to_check.x() == 0 {
            return None;
        }
        for (pid, terminal) in self.get_terminals() {
            if terminal.x() + terminal.columns() == terminal_to_check.x() - 1 {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_right_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        for (pid, terminal) in self.get_terminals() {
            if terminal.x() == terminal_to_check.x() + terminal_to_check.columns() + 1 {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        for (pid, terminal) in self.get_terminals() {
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() + 1 {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        for (pid, terminal) in self.get_terminals() {
            if terminal.y() + terminal.rows() + 1 == terminal_to_check.y() {
                ids.push(pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn panes_top_aligned_with_pane(&self, pane: &Box<dyn Pane>) -> Vec<&Box<dyn Pane>> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.y() == pane.y())
            .collect()
    }
    fn panes_bottom_aligned_with_pane(&self, pane: &Box<dyn Pane>) -> Vec<&Box<dyn Pane>> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(&t_id).unwrap())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.y() + terminal.rows() == pane.y() + pane.rows()
            })
            .collect()
    }
    fn panes_right_aligned_with_pane(&self, pane: &Box<dyn Pane>) -> Vec<&Box<dyn Pane>> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(&t_id).unwrap())
            .filter(|terminal| {
                terminal.pid() != pane.pid()
                    && terminal.x() + terminal.columns() == pane.x() + pane.columns()
            })
            .collect()
    }
    fn panes_left_aligned_with_pane(&self, pane: &&Box<dyn Pane>) -> Vec<&Box<dyn Pane>> {
        self.panes
            .keys()
            .map(|t_id| self.panes.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid() != pane.pid() && terminal.x() == pane.x())
            .collect()
    }
    fn right_aligned_contiguous_panes_above(
        &self,
        id: &RawFd,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("terminal id does not exist");
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by(|a, b| b.y().cmp(&a.y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() + 1 == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_right
                .get(&(bottom_terminal_boundary + 1))
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn right_aligned_contiguous_panes_below(
        &self,
        id: &RawFd,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("terminal id does not exist");
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by(|a, b| a.y().cmp(&b.y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() + 1 {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the right
        let mut bottom_resize_border = self.full_screen_ws.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_right
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.y() + terminal.rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_above(
        &self,
        id: &RawFd,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("terminal id does not exist");
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by(|a, b| b.y().cmp(&a.y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() + terminal.rows() + 1 == terminal_to_check.y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.y() + terminal.rows();
            if terminal_borders_to_the_left
                .get(&(bottom_terminal_boundary + 1))
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (top_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_below(
        &self,
        id: &RawFd,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("terminal id does not exist");
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by(|a, b| a.y().cmp(&b.y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.y() == terminal_to_check.y() + terminal_to_check.rows() + 1 {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the left
        let mut bottom_resize_border = self.full_screen_ws.rows;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.y();
            if terminal_borders_to_the_left
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| {
            // terminal.y() + terminal.rows() < bottom_resize_border
            terminal.y() + terminal.rows() <= bottom_resize_border
        });
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.y() + terminal_to_check.rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_left(
        &self,
        id: &RawFd,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("terminal id does not exist");
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by(|a, b| b.x().cmp(&a.x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.columns() + 1 == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.columns();
            if terminal_borders_above
                .get(&(right_terminal_boundary + 1))
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_right(
        &self,
        id: &RawFd,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by(|a, b| a.x().cmp(&b.x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.columns() + 1 {
                terminals.push(terminal);
            }
        }
        // rightmost border aligned with a pane border above
        let mut right_resize_border = self.full_screen_ws.columns;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_above
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.columns() <= right_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.columns()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_left(
        &self,
        id: &RawFd,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(&terminal_to_check);
        bottom_aligned_terminals.sort_by(|a, b| b.x().cmp(&a.x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() + terminal.columns() + 1 == terminal_to_check.x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.x() + terminal.columns();
            if terminal_borders_below
                .get(&(right_terminal_boundary + 1))
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (left_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_right(
        &self,
        id: &RawFd,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.panes.get(&PaneKind::Terminal(*id)).unwrap();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(&terminal_to_check);
        bottom_aligned_terminals.sort_by(|a, b| a.x().cmp(&b.x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.x() == terminal_to_check.x() + terminal_to_check.columns() + 1 {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut right_resize_border = self.full_screen_ws.columns;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.x();
            if terminal_borders_below
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.x() + terminal.columns() <= right_resize_border);
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.x() + terminal_to_check.columns()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid()).collect();
        (right_resize_border, terminal_ids)
    }
    fn reduce_pane_height_down(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.reduce_height_down(count);
        self.os_api.set_terminal_size_using_fd(
            *id,
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn reduce_pane_height_up(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.reduce_height_up(count);
        self.os_api.set_terminal_size_using_fd(
            *id,
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn increase_pane_height_down(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.increase_height_down(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn increase_pane_height_up(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.increase_height_up(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn increase_pane_width_right(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.increase_width_right(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn increase_pane_width_left(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.increase_width_left(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn reduce_pane_width_right(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.reduce_width_right(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn reduce_pane_width_left(&mut self, id: &RawFd, count: usize) {
        let terminal = self.panes.get_mut(&PaneKind::Terminal(*id)).unwrap();
        terminal.reduce_width_left(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid(),
            terminal.columns() as u16,
            terminal.rows() as u16,
        );
    }
    fn pane_is_between_vertical_borders(
        &self,
        id: &RawFd,
        left_border_x: usize,
        right_border_x: usize,
    ) -> bool {
        let terminal = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("could not find terminal to check between borders");
        terminal.x() >= left_border_x && terminal.x() + terminal.columns() <= right_border_x
    }
    fn pane_is_between_horizontal_borders(
        &self,
        id: &RawFd,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let terminal = self
            .panes
            .get(&PaneKind::Terminal(*id))
            .expect("could not find terminal to check between borders");
        terminal.y() >= top_border_y && terminal.y() + terminal.rows() <= bottom_border_y
    }
    fn reduce_pane_and_surroundings_up(&mut self, id: &RawFd, count: usize) {
        let mut terminals_below = self
            .terminal_ids_directly_below(&id)
            .expect("can't reduce pane size up if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(&id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(&id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.reduce_pane_height_up(&id, count);
        for terminal_id in terminals_below {
            self.increase_pane_height_up(&terminal_id, count);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.reduce_pane_height_up(&terminal_id, count);
        }
    }
    fn reduce_pane_and_surroundings_down(&mut self, id: &RawFd, count: usize) {
        let mut terminals_above = self
            .terminal_ids_directly_above(&id)
            .expect("can't reduce pane size down if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(&id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(&id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.reduce_pane_height_down(&id, count);
        for terminal_id in terminals_above {
            self.increase_pane_height_down(&terminal_id, count);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.reduce_pane_height_down(&terminal_id, count);
        }
    }
    fn reduce_pane_and_surroundings_right(&mut self, id: &RawFd, count: usize) {
        let mut terminals_to_the_left = self
            .terminal_ids_directly_left_of(&id)
            .expect("can't reduce pane size right if there are no terminals to the left");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(&id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(&id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.reduce_pane_width_right(&id, count);
        for terminal_id in terminals_to_the_left {
            self.increase_pane_width_right(&terminal_id, count);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.reduce_pane_width_right(&terminal_id, count);
        }
    }
    fn reduce_pane_and_surroundings_left(&mut self, id: &RawFd, count: usize) {
        let mut terminals_to_the_right = self
            .terminal_ids_directly_right_of(&id)
            .expect("can't reduce pane size left if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(&id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(&id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.reduce_pane_width_left(&id, count);
        for terminal_id in terminals_to_the_right {
            self.increase_pane_width_left(&terminal_id, count);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.reduce_pane_width_left(&terminal_id, count);
        }
    }
    fn increase_pane_and_surroundings_up(&mut self, id: &RawFd, count: usize) {
        let mut terminals_above = self
            .terminal_ids_directly_above(&id)
            .expect("can't increase pane size up if there are no terminals above");
        let terminal_borders_above: HashSet<usize> = terminals_above
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.top_aligned_contiguous_panes_to_the_left(&id, &terminal_borders_above);
        let (right_resize_border, terminals_to_the_right) =
            self.top_aligned_contiguous_panes_to_the_right(&id, &terminal_borders_above);
        terminals_above.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height_up(&id, count);
        for terminal_id in terminals_above {
            self.reduce_pane_height_up(&terminal_id, count);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.increase_pane_height_up(&terminal_id, count);
        }
    }
    fn increase_pane_and_surroundings_down(&mut self, id: &RawFd, count: usize) {
        let mut terminals_below = self
            .terminal_ids_directly_below(&id)
            .expect("can't increase pane size down if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().x())
            .collect();
        let (left_resize_border, terminals_to_the_left) =
            self.bottom_aligned_contiguous_panes_to_the_left(&id, &terminal_borders_below);
        let (right_resize_border, terminals_to_the_right) =
            self.bottom_aligned_contiguous_panes_to_the_right(&id, &terminal_borders_below);
        terminals_below.retain(|t| {
            self.pane_is_between_vertical_borders(t, left_resize_border, right_resize_border)
        });
        self.increase_pane_height_down(&id, count);
        for terminal_id in terminals_below {
            self.reduce_pane_height_down(&terminal_id, count);
        }
        for terminal_id in terminals_to_the_left
            .iter()
            .chain(terminals_to_the_right.iter())
        {
            self.increase_pane_height_down(&terminal_id, count);
        }
    }
    fn increase_pane_and_surroundings_right(&mut self, id: &RawFd, count: usize) {
        let mut terminals_to_the_right = self
            .terminal_ids_directly_right_of(&id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_right: HashSet<usize> = terminals_to_the_right
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.right_aligned_contiguous_panes_above(&id, &terminal_borders_to_the_right);
        let (bottom_resize_border, terminals_below) =
            self.right_aligned_contiguous_panes_below(&id, &terminal_borders_to_the_right);
        terminals_to_the_right.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width_right(&id, count);
        for terminal_id in terminals_to_the_right {
            self.reduce_pane_width_right(&terminal_id, count);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.increase_pane_width_right(&terminal_id, count);
        }
    }
    fn increase_pane_and_surroundings_left(&mut self, id: &RawFd, count: usize) {
        let mut terminals_to_the_left = self
            .terminal_ids_directly_left_of(&id)
            .expect("can't increase pane size right if there are no terminals to the right");
        let terminal_borders_to_the_left: HashSet<usize> = terminals_to_the_left
            .iter()
            .map(|t| self.panes.get(&PaneKind::Terminal(*t)).unwrap().y())
            .collect();
        let (top_resize_border, terminals_above) =
            self.left_aligned_contiguous_panes_above(&id, &terminal_borders_to_the_left);
        let (bottom_resize_border, terminals_below) =
            self.left_aligned_contiguous_panes_below(&id, &terminal_borders_to_the_left);
        terminals_to_the_left.retain(|t| {
            self.pane_is_between_horizontal_borders(t, top_resize_border, bottom_resize_border)
        });
        self.increase_pane_width_left(&id, count);
        for terminal_id in terminals_to_the_left {
            self.reduce_pane_width_left(&terminal_id, count);
        }
        for terminal_id in terminals_above.iter().chain(terminals_below.iter()) {
            self.increase_pane_width_left(&terminal_id, count);
        }
    }
    fn panes_exist_above(&self, pane_id: &RawFd) -> bool {
        let pane = self
            .panes
            .get(&PaneKind::Terminal(*pane_id))
            .expect("pane does not exist");
        pane.y() > 0
    }
    fn panes_exist_below(&self, pane_id: &RawFd) -> bool {
        let pane = self
            .panes
            .get(&PaneKind::Terminal(*pane_id))
            .expect("pane does not exist");
        pane.y() + pane.rows() < self.full_screen_ws.rows
    }
    fn panes_exist_to_the_right(&self, pane_id: &RawFd) -> bool {
        let pane = self
            .panes
            .get(&PaneKind::Terminal(*pane_id))
            .expect("pane does not exist");
        pane.x() + pane.columns() < self.full_screen_ws.columns
    }
    fn panes_exist_to_the_left(&self, pane_id: &RawFd) -> bool {
        let pane = self
            .panes
            .get(&PaneKind::Terminal(*pane_id))
            .expect("pane does not exist");
        pane.x() > 0
    }
    pub fn resize_right(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_to_the_right(&active_terminal_id) {
                self.increase_pane_and_surroundings_right(&active_terminal_id, count);
            } else if self.panes_exist_to_the_left(&active_terminal_id) {
                self.reduce_pane_and_surroundings_right(&active_terminal_id, count);
            }
            self.render();
        }
    }
    pub fn resize_left(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_to_the_right(&active_terminal_id) {
                self.reduce_pane_and_surroundings_left(&active_terminal_id, count);
            } else if self.panes_exist_to_the_left(&active_terminal_id) {
                self.increase_pane_and_surroundings_left(&active_terminal_id, count);
            }
            self.render();
        }
    }
    pub fn resize_down(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_above(&active_terminal_id) {
                self.reduce_pane_and_surroundings_down(&active_terminal_id, count);
            } else if self.panes_exist_below(&active_terminal_id) {
                self.increase_pane_and_surroundings_down(&active_terminal_id, count);
            }
            self.render();
        }
    }
    pub fn resize_up(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_above(&active_terminal_id) {
                self.increase_pane_and_surroundings_up(&active_terminal_id, count);
            } else if self.panes_exist_below(&active_terminal_id) {
                self.reduce_pane_and_surroundings_up(&active_terminal_id, count);
            }
            self.render();
        }
    }
    pub fn move_focus(&mut self) {
        if !self.has_terminal_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal_id = self.get_active_terminal_id().unwrap();
        let terminal_ids: Vec<RawFd> = self
            .get_terminals()
            .filter_map(|(pid, _)| Some(pid))
            .collect(); // TODO: better, no allocations
        let first_terminal = terminal_ids.get(0).unwrap();
        let active_terminal_id_position = terminal_ids
            .iter()
            .position(|id| id == &active_terminal_id)
            .unwrap();
        if let Some(next_terminal) = terminal_ids.get(active_terminal_id_position + 1) {
            self.active_terminal = Some(*next_terminal);
        } else {
            self.active_terminal = Some(*first_terminal);
        }
        self.render();
    }
    pub fn move_focus_left(&mut self) {
        if !self.has_terminal_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal = self.get_active_terminal();
        if let Some(active) = active_terminal {
            let terminals = self.get_terminals();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_left_of(&active) && c.horizontally_overlaps_with(&active)
                })
                .max_by_key(|(_, (_, c))| c.get_horizontal_overlap_with(&active))
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(p) => {
                    self.active_terminal = Some(p);
                }
                None => {
                    self.active_terminal = Some(active.pid());
                }
            }
        } else {
            self.active_terminal = Some(active_terminal.unwrap().pid());
        }
        self.render();
    }
    pub fn move_focus_down(&mut self) {
        if !self.has_terminal_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal = self.get_active_terminal();
        if let Some(active) = active_terminal {
            let terminals = self.get_terminals();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_below(&active) && c.vertically_overlaps_with(&active)
                })
                .max_by_key(|(_, (_, c))| c.get_vertical_overlap_with(&active))
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(p) => {
                    self.active_terminal = Some(p);
                }
                None => {
                    self.active_terminal = Some(active.pid());
                }
            }
        } else {
            self.active_terminal = Some(active_terminal.unwrap().pid());
        }
        self.render();
    }
    pub fn move_focus_up(&mut self) {
        if !self.has_terminal_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal = self.get_active_terminal();
        if let Some(active) = active_terminal {
            let terminals = self.get_terminals();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_above(&active) && c.vertically_overlaps_with(&active)
                })
                .max_by_key(|(_, (_, c))| c.get_vertical_overlap_with(&active))
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(p) => {
                    self.active_terminal = Some(p);
                }
                None => {
                    self.active_terminal = Some(active.pid());
                }
            }
        } else {
            self.active_terminal = Some(active_terminal.unwrap().pid());
        }
        self.render();
    }
    pub fn move_focus_right(&mut self) {
        if !self.has_terminal_panes() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal = self.get_active_terminal();
        if let Some(active) = active_terminal {
            let terminals = self.get_terminals();
            let next_index = terminals
                .enumerate()
                .filter(|(_, (_, c))| {
                    c.is_directly_right_of(&active) && c.horizontally_overlaps_with(&active)
                })
                .max_by_key(|(_, (_, c))| c.get_horizontal_overlap_with(&active))
                .map(|(_, (pid, _))| pid);
            match next_index {
                Some(p) => {
                    self.active_terminal = Some(p);
                }
                None => {
                    self.active_terminal = Some(active.pid());
                }
            }
        } else {
            self.active_terminal = Some(active_terminal.unwrap().pid());
        }
        self.render();
    }
    fn horizontal_borders(&self, terminals: &[RawFd]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.panes.get(&PaneKind::Terminal(*t)).unwrap();
            borders.insert(terminal.y());
            borders.insert(terminal.y() + terminal.rows() + 1); // 1 for the border width
            borders
        })
    }
    fn vertical_borders(&self, terminals: &[RawFd]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.panes.get(&PaneKind::Terminal(*t)).unwrap();
            borders.insert(terminal.x());
            borders.insert(terminal.x() + terminal.columns() + 1); // 1 for the border width
            borders
        })
    }
    fn terminals_to_the_left_between_aligning_borders(&self, id: RawFd) -> Option<Vec<RawFd>> {
        if let Some(terminal) = &self.panes.get(&PaneKind::Terminal(id)) {
            let upper_close_border = terminal.y();
            let lower_close_border = terminal.y() + terminal.rows() + 1;

            if let Some(mut terminals_to_the_left) = self.terminal_ids_directly_left_of(&id) {
                let terminal_borders_to_the_left = self.horizontal_borders(&terminals_to_the_left);
                if terminal_borders_to_the_left.contains(&upper_close_border)
                    && terminal_borders_to_the_left.contains(&lower_close_border)
                {
                    terminals_to_the_left.retain(|t| {
                        self.pane_is_between_horizontal_borders(
                            t,
                            upper_close_border,
                            lower_close_border,
                        )
                    });
                    return Some(terminals_to_the_left);
                }
            }
        }
        None
    }
    fn terminals_to_the_right_between_aligning_borders(&self, id: RawFd) -> Option<Vec<RawFd>> {
        if let Some(terminal) = &self.panes.get(&PaneKind::Terminal(id)) {
            let upper_close_border = terminal.y();
            let lower_close_border = terminal.y() + terminal.rows() + 1;

            if let Some(mut terminals_to_the_right) = self.terminal_ids_directly_right_of(&id) {
                let terminal_borders_to_the_right =
                    self.horizontal_borders(&terminals_to_the_right);
                if terminal_borders_to_the_right.contains(&upper_close_border)
                    && terminal_borders_to_the_right.contains(&lower_close_border)
                {
                    terminals_to_the_right.retain(|t| {
                        self.pane_is_between_horizontal_borders(
                            t,
                            upper_close_border,
                            lower_close_border,
                        )
                    });
                    return Some(terminals_to_the_right);
                }
            }
        }
        None
    }
    fn terminals_above_between_aligning_borders(&self, id: RawFd) -> Option<Vec<RawFd>> {
        if let Some(terminal) = &self.panes.get(&PaneKind::Terminal(id)) {
            let left_close_border = terminal.x();
            let right_close_border = terminal.x() + terminal.columns() + 1;

            if let Some(mut terminals_above) = self.terminal_ids_directly_above(&id) {
                let terminal_borders_above = self.vertical_borders(&terminals_above);
                if terminal_borders_above.contains(&left_close_border)
                    && terminal_borders_above.contains(&right_close_border)
                {
                    terminals_above.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(terminals_above);
                }
            }
        }
        None
    }
    fn terminals_below_between_aligning_borders(&self, id: RawFd) -> Option<Vec<RawFd>> {
        if let Some(terminal) = &self.panes.get(&PaneKind::Terminal(id)) {
            let left_close_border = terminal.x();
            let right_close_border = terminal.x() + terminal.columns() + 1;

            if let Some(mut terminals_below) = self.terminal_ids_directly_below(&id) {
                let terminal_borders_below = self.vertical_borders(&terminals_below);
                if terminal_borders_below.contains(&left_close_border)
                    && terminal_borders_below.contains(&right_close_border)
                {
                    terminals_below.retain(|t| {
                        self.pane_is_between_vertical_borders(
                            t,
                            left_close_border,
                            right_close_border,
                        )
                    });
                    return Some(terminals_below);
                }
            }
        }
        None
    }
    fn close_down_to_max_terminals(&mut self) {
        if let Some(max_panes) = self.max_panes {
            let terminals = self.get_terminal_pane_ids();
            for pid in terminals.iter().skip(max_panes - 1) {
                self.send_pty_instructions
                    .send(PtyInstruction::ClosePane(*pid))
                    .unwrap();
                self.close_pane_without_rerender(*pid);
            }
        }
    }
    pub fn get_terminal_pane_ids(&mut self) -> Vec<RawFd> {
        self.get_terminals()
            .filter_map(|(pid, _)| Some(pid))
            .collect()
    }
    pub fn close_pane(&mut self, id: RawFd) {
        if self.panes.get(&PaneKind::Terminal(id)).is_some() {
            self.close_pane_without_rerender(id);
        }
    }
    pub fn close_pane_without_rerender(&mut self, id: RawFd) {
        if let Some(terminal_to_close) = &self.panes.get(&PaneKind::Terminal(id)) {
            let terminal_to_close_width = terminal_to_close.columns();
            let terminal_to_close_height = terminal_to_close.rows();
            if let Some(terminals) = self.terminals_to_the_left_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    self.increase_pane_width_right(&terminal_id, terminal_to_close_width + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_to_the_right_between_aligning_borders(id)
            {
                for terminal_id in terminals.iter() {
                    self.increase_pane_width_left(&terminal_id, terminal_to_close_width + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_above_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    self.increase_pane_height_down(&terminal_id, terminal_to_close_height + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_below_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    self.increase_pane_height_up(&terminal_id, terminal_to_close_height + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else {
            }
            self.panes.remove(&PaneKind::Terminal(id));
            if !self.has_terminal_panes() {
                self.active_terminal = None;
            }
        }
    }
    pub fn close_focused_pane(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            self.close_pane(active_terminal_id);
            self.send_pty_instructions
                .send(PtyInstruction::ClosePane(active_terminal_id))
                .unwrap();
        }
    }
    pub fn scroll_active_terminal_up(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self
                .panes
                .get_mut(&PaneKind::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.scroll_up(1);
            self.render();
        }
    }
    pub fn scroll_active_terminal_down(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self
                .panes
                .get_mut(&PaneKind::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.scroll_down(1);
            self.render();
        }
    }
    pub fn clear_active_terminal_scroll(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self
                .panes
                .get_mut(&PaneKind::Terminal(active_terminal_id))
                .unwrap();
            active_terminal.clear_scroll();
        }
    }
}
