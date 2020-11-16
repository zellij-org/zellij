use nix::pty::Winsize;
use std::collections::{BTreeMap, HashSet};
use std::io::Write;
use std::os::unix::io::RawFd;
use std::sync::mpsc::{Receiver, Sender};

use crate::boundaries::Boundaries;
use crate::layout::Layout;
use crate::os_input_output::OsApi;
use crate::pty_bus::{PtyInstruction, VteEvent};
use crate::terminal_pane::{PositionAndSize, TerminalPane};
use crate::AppInstruction;

/*
 * Screen
 *
 * this holds multiple panes (currently terminal panes) which are currently displayed on screen
 * it tracks their coordinates (x/y) and size, as well as how they should be resized
 *
 */

fn _debug_log_to_file(message: String) {
    use std::fs::OpenOptions;
    use std::io::prelude::*;
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open("/tmp/mosaic-log.txt")
        .unwrap();
    file.write_all(message.as_bytes()).unwrap();
    file.write_all("\n".as_bytes()).unwrap();
}

const CURSOR_HEIGHT_WIDGH_RATIO: usize = 4; // this is not accurate and kind of a magic number, TODO: look into this

type BorderAndPaneIds = (usize, Vec<RawFd>);

fn split_vertically_with_gap(rect: &Winsize) -> (Winsize, Winsize) {
    let width_of_each_half = (rect.ws_col - 1) / 2;
    let mut first_rect = rect.clone();
    let mut second_rect = rect.clone();
    if rect.ws_col % 2 == 0 {
        first_rect.ws_col = width_of_each_half + 1;
    } else {
        first_rect.ws_col = width_of_each_half;
    }
    second_rect.ws_col = width_of_each_half;
    (first_rect, second_rect)
}

fn split_horizontally_with_gap(rect: &Winsize) -> (Winsize, Winsize) {
    let height_of_each_half = (rect.ws_row - 1) / 2;
    let mut first_rect = rect.clone();
    let mut second_rect = rect.clone();
    if rect.ws_row % 2 == 0 {
        first_rect.ws_row = height_of_each_half + 1;
    } else {
        first_rect.ws_row = height_of_each_half;
    }
    second_rect.ws_row = height_of_each_half;
    (first_rect, second_rect)
}

#[derive(Debug)]
pub enum ScreenInstruction {
    Pty(RawFd, VteEvent),
    Render,
    NewPane(RawFd),
    HorizontalSplit(RawFd),
    VerticalSplit(RawFd),
    WriteCharacter([u8; 10]),
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    MoveFocus,
    Quit,
    ScrollUp,
    ScrollDown,
    ClearScroll,
    CloseFocusedPane,
    ToggleActiveTerminalFullscreen,
    ClosePane(RawFd),
    ApplyLayout((Layout, Vec<RawFd>)),
}

pub struct Screen {
    pub receiver: Receiver<ScreenInstruction>,
    max_panes: Option<usize>,
    send_pty_instructions: Sender<PtyInstruction>,
    send_app_instructions: Sender<AppInstruction>,
    full_screen_ws: Winsize,
    terminals: BTreeMap<RawFd, TerminalPane>, // BTreeMap because we need a predictable order when changing focus
    panes_to_hide: HashSet<RawFd>,
    active_terminal: Option<RawFd>,
    os_api: Box<dyn OsApi>,
    fullscreen_is_active: bool,
}

impl Screen {
    pub fn new(
        receive_screen_instructions: Receiver<ScreenInstruction>,
        send_pty_instructions: Sender<PtyInstruction>,
        send_app_instructions: Sender<AppInstruction>,
        full_screen_ws: &Winsize,
        os_api: Box<dyn OsApi>,
        max_panes: Option<usize>,
    ) -> Self {
        Screen {
            receiver: receive_screen_instructions,
            max_panes,
            send_pty_instructions,
            send_app_instructions,
            full_screen_ws: full_screen_ws.clone(),
            terminals: BTreeMap::new(),
            panes_to_hide: HashSet::new(),
            active_terminal: None,
            os_api,
            fullscreen_is_active: false,
        }
    }

    pub fn apply_layout(&mut self, layout: Layout, new_pids: Vec<RawFd>) {
        self.panes_to_hide.clear();
        // TODO: this should be an attribute on Screen instead of full_screen_ws
        let free_space = PositionAndSize {
            x: 0,
            y: 0,
            rows: self.full_screen_ws.ws_row as usize,
            columns: self.full_screen_ws.ws_col as usize,
        };
        let positions_in_layout = layout.position_panes_in_space(&free_space);
        let mut positions_and_size = positions_in_layout.iter();
        for (pid, terminal_pane) in self.terminals.iter_mut() {
            match positions_and_size.next() {
                Some(position_and_size) => {
                    terminal_pane.reset_size_and_position_override();
                    terminal_pane.change_size_p(&position_and_size);
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
        let mut new_pids = new_pids.iter();
        for position_and_size in positions_and_size {
            // there are still panes left to fill, use the pids we received in this method
            let pid = new_pids.next().unwrap(); // if this crashes it means we got less pids than there are panes in this layout
            let mut new_terminal = TerminalPane::new(
                *pid,
                self.full_screen_ws.clone(),
                position_and_size.x,
                position_and_size.y,
            );
            new_terminal.change_size_p(position_and_size);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.get_columns() as u16,
                new_terminal.get_rows() as u16,
            );
            self.terminals.insert(*pid, new_terminal);
        }
        for unused_pid in new_pids {
            // this is a bit of a hack and happens because we don't have any central location that
            // can query the screen as to how many panes it needs to create a layout
            // fixing this will require a bit of an architecture change
            self.send_pty_instructions
                .send(PtyInstruction::ClosePane(*unused_pid))
                .unwrap();
        }
        self.active_terminal = Some(*self.terminals.iter().next().unwrap().0);
        self.render();
    }

    pub fn toggle_fullscreen_is_active(&mut self) {
        self.fullscreen_is_active = !self.fullscreen_is_active;
    }

    pub fn new_pane(&mut self, pid: RawFd) {
        self.close_down_to_max_terminals();
        if self.fullscreen_is_active {
            self.toggle_active_terminal_fullscreen();
        }
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.get_columns() as u16,
                new_terminal.get_rows() as u16,
            );
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal

            let (_longest_edge, terminal_id_to_split) = self.terminals.iter().fold(
                (0, 0),
                |(current_longest_edge, current_terminal_id_to_split), id_and_terminal_to_check| {
                    let (id_of_terminal_to_check, terminal_to_check) = id_and_terminal_to_check;
                    let terminal_size = (terminal_to_check.get_rows() * CURSOR_HEIGHT_WIDGH_RATIO)
                        * terminal_to_check.get_columns();
                    if terminal_size > current_longest_edge {
                        (terminal_size, *id_of_terminal_to_check)
                    } else {
                        (current_longest_edge, current_terminal_id_to_split)
                    }
                },
            );
            let terminal_to_split = self.terminals.get_mut(&terminal_id_to_split).unwrap();
            let terminal_ws = Winsize {
                ws_row: terminal_to_split.get_rows() as u16,
                ws_col: terminal_to_split.get_columns() as u16,
                ws_xpixel: terminal_to_split.get_x() as u16,
                ws_ypixel: terminal_to_split.get_y() as u16,
            };
            if terminal_to_split.get_rows() * CURSOR_HEIGHT_WIDGH_RATIO
                > terminal_to_split.get_columns()
            {
                let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&terminal_ws);
                let bottom_half_y = terminal_ws.ws_ypixel + top_winsize.ws_row + 1;
                let new_terminal = TerminalPane::new(
                    pid,
                    bottom_winsize,
                    terminal_ws.ws_xpixel as usize,
                    bottom_half_y as usize,
                );
                self.os_api.set_terminal_size_using_fd(
                    new_terminal.pid,
                    bottom_winsize.ws_col,
                    bottom_winsize.ws_row,
                );
                terminal_to_split.change_size(&top_winsize);
                self.terminals.insert(pid, new_terminal);
                self.os_api.set_terminal_size_using_fd(
                    terminal_id_to_split,
                    top_winsize.ws_col,
                    top_winsize.ws_row,
                );
                self.active_terminal = Some(pid);
            } else {
                let (left_winszie, right_winsize) = split_vertically_with_gap(&terminal_ws);
                let right_side_x = (terminal_ws.ws_xpixel + left_winszie.ws_col + 1) as usize;
                let new_terminal = TerminalPane::new(
                    pid,
                    right_winsize,
                    right_side_x,
                    terminal_ws.ws_ypixel as usize,
                );
                self.os_api.set_terminal_size_using_fd(
                    new_terminal.pid,
                    right_winsize.ws_col,
                    right_winsize.ws_row,
                );
                terminal_to_split.change_size(&left_winszie);
                self.terminals.insert(pid, new_terminal);
                self.os_api.set_terminal_size_using_fd(
                    terminal_id_to_split,
                    left_winszie.ws_col,
                    left_winszie.ws_row,
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
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.get_columns() as u16,
                new_terminal.get_rows() as u16,
            );
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x, active_terminal_y) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.get_rows() as u16,
                        ws_col: active_terminal.get_columns() as u16,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.get_x(),
                    active_terminal.get_y(),
                )
            };
            let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&active_terminal_ws);
            let bottom_half_y = active_terminal_y + top_winsize.ws_row as usize + 1;
            let new_terminal =
                TerminalPane::new(pid, bottom_winsize, active_terminal_x, bottom_half_y);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                bottom_winsize.ws_col,
                bottom_winsize.ws_row,
            );

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&top_winsize);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_pid,
                top_winsize.ws_col,
                top_winsize.ws_row,
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
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalPane::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                new_terminal.get_columns() as u16,
                new_terminal.get_rows() as u16,
            );
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x, active_terminal_y) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.get_rows() as u16,
                        ws_col: active_terminal.get_columns() as u16,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.get_x(),
                    active_terminal.get_y(),
                )
            };
            let (left_winszie, right_winsize) = split_vertically_with_gap(&active_terminal_ws);
            let right_side_x = active_terminal_x + left_winszie.ws_col as usize + 1;
            let new_terminal =
                TerminalPane::new(pid, right_winsize, right_side_x, active_terminal_y);
            self.os_api.set_terminal_size_using_fd(
                new_terminal.pid,
                right_winsize.ws_col,
                right_winsize.ws_row,
            );

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&left_winszie);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_pid,
                left_winszie.ws_col,
                left_winszie.ws_row,
            );
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    fn get_active_terminal(&self) -> Option<&TerminalPane> {
        match self.active_terminal {
            Some(active_terminal) => self.terminals.get(&active_terminal),
            None => None,
        }
    }
    fn get_active_terminal_id(&self) -> Option<RawFd> {
        match self.active_terminal {
            Some(active_terminal) => Some(self.terminals.get(&active_terminal).unwrap().pid),
            None => None,
        }
    }
    pub fn handle_pty_event(&mut self, pid: RawFd, event: VteEvent) {
        let terminal_output = self.terminals.get_mut(&pid).unwrap();
        terminal_output.handle_event(event);
    }
    pub fn write_to_active_terminal(&mut self, bytes: [u8; 10]) {
        if let Some(active_terminal_id) = &self.get_active_terminal_id() {
            // this is a bit of a hack and is done in order not to send trailing
            // zeros to the terminal (because they mess things up)
            // TODO: fix this by only sending around the exact bytes read from stdin
            let mut trimmed_bytes = vec![];
            for byte in bytes.iter() {
                if *byte == 0 {
                    break;
                } else {
                    trimmed_bytes.push(*byte);
                }
            }
            self.os_api
                .write_to_tty_stdin(*active_terminal_id, &mut trimmed_bytes)
                .expect("failed to write to terminal");
            self.os_api
                .tcdrain(*active_terminal_id)
                .expect("failed to drain terminal");
        }
    }
    fn get_active_terminal_cursor_position(&self) -> Option<(usize, usize)> {
        // (x, y)
        let active_terminal = &self.get_active_terminal().unwrap();
        active_terminal
            .cursor_coordinates()
            .and_then(|(x_in_terminal, y_in_terminal)| {
                let x = active_terminal.get_x() + x_in_terminal;
                let y = active_terminal.get_y() + y_in_terminal;
                Some((x, y))
            })
    }
    pub fn toggle_active_terminal_fullscreen(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self
                .get_active_terminal()
                .unwrap()
                .position_and_size_override
                .is_some()
            {
                for terminal_id in self.panes_to_hide.iter() {
                    self.terminals.get_mut(terminal_id).unwrap().should_render = true;
                }
                self.panes_to_hide.clear();
                let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.reset_size_and_position_override();
            } else {
                let all_ids_except_current = self
                    .terminals
                    .keys()
                    .filter(|id| **id != active_terminal_id);
                self.panes_to_hide = all_ids_except_current.copied().collect();
                let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.override_size_and_position(0, 0, &self.full_screen_ws);
            }
            let active_terminal = self.terminals.get(&active_terminal_id).unwrap();
            self.os_api.set_terminal_size_using_fd(
                active_terminal_id,
                active_terminal.get_columns() as u16,
                active_terminal.get_rows() as u16,
            );
            self.render();
            self.toggle_fullscreen_is_active();
        }
    }
    pub fn render(&mut self) {
        if self.active_terminal.is_none() {
            // we might not have an active terminal if we closed the last pane
            // in that case, we should not render as the app is exiting
            return;
        }
        let mut stdout = self.os_api.get_stdout_writer();
        let mut boundaries =
            Boundaries::new(self.full_screen_ws.ws_col, self.full_screen_ws.ws_row);
        for (pid, terminal) in self.terminals.iter_mut() {
            if !self.panes_to_hide.contains(pid) {
                boundaries.add_rect(&terminal);
                if let Some(vte_output) = terminal.buffer_as_vte_output() {
                    stdout
                        .write_all(&vte_output.as_bytes())
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
    fn terminal_ids_directly_left_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        if terminal_to_check.get_x() == 0 {
            return None;
        }
        for (pid, terminal) in self.terminals.iter() {
            if terminal.get_x() + terminal.get_columns() == terminal_to_check.get_x() - 1 {
                ids.push(*pid);
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
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.get_x() == terminal_to_check.get_x() + terminal_to_check.get_columns() + 1 {
                ids.push(*pid);
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
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.get_y() == terminal_to_check.get_y() + terminal_to_check.get_rows() + 1 {
                ids.push(*pid);
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
        let terminal_to_check = self.terminals.get(id).unwrap();
        for (pid, terminal) in self.terminals.iter() {
            if terminal.get_y() + terminal.get_rows() + 1 == terminal_to_check.get_y() {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn panes_top_aligned_with_pane(&self, pane: &TerminalPane) -> Vec<&TerminalPane> {
        self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != pane.pid && terminal.get_y() == pane.get_y())
            .collect()
    }
    fn panes_bottom_aligned_with_pane(&self, pane: &TerminalPane) -> Vec<&TerminalPane> {
        self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| {
                terminal.pid != pane.pid
                    && terminal.get_y() + terminal.get_rows() == pane.get_y() + pane.get_rows()
            })
            .collect()
    }
    fn panes_right_aligned_with_pane(&self, pane: &TerminalPane) -> Vec<&TerminalPane> {
        self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| {
                terminal.pid != pane.pid
                    && terminal.get_x() + terminal.get_columns()
                        == pane.get_x() + pane.get_columns()
            })
            .collect()
    }
    fn panes_left_aligned_with_pane(&self, pane: &TerminalPane) -> Vec<&TerminalPane> {
        self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != pane.pid && terminal.get_x() == pane.get_x())
            .collect()
    }
    fn right_aligned_contiguous_panes_above(
        &self,
        id: &RawFd,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).expect("terminal id does not exist");
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by(|a, b| b.get_y().cmp(&a.get_y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_y() + terminal.get_rows() + 1 == terminal_to_check.get_y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.get_y() + terminal.get_rows();
            if terminal_borders_to_the_right
                .get(&(bottom_terminal_boundary + 1))
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.get_y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.get_y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (top_resize_border, terminal_ids)
    }
    fn right_aligned_contiguous_panes_below(
        &self,
        id: &RawFd,
        terminal_borders_to_the_right: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).expect("terminal id does not exist");
        let mut right_aligned_terminals = self.panes_right_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        right_aligned_terminals.sort_by(|a, b| a.get_y().cmp(&b.get_y()));
        for terminal in right_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_y() == terminal_to_check.get_y() + terminal_to_check.get_rows() + 1 {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the right
        let mut bottom_resize_border = self.full_screen_ws.ws_row as usize;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.get_y();
            if terminal_borders_to_the_right
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| terminal.get_y() + terminal.get_rows() <= bottom_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.get_y() + terminal_to_check.get_rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_above(
        &self,
        id: &RawFd,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).expect("terminal id does not exist");
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by(|a, b| b.get_y().cmp(&a.get_y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_y() + terminal.get_rows() + 1 == terminal_to_check.get_y() {
                terminals.push(terminal);
            }
        }
        // top-most border aligned with a pane border to the right
        let mut top_resize_border = 0;
        for terminal in &terminals {
            let bottom_terminal_boundary = terminal.get_y() + terminal.get_rows();
            if terminal_borders_to_the_left
                .get(&(bottom_terminal_boundary + 1))
                .is_some()
                && top_resize_border < bottom_terminal_boundary
            {
                top_resize_border = bottom_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.get_y() >= top_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let top_resize_border = if terminals.is_empty() {
            terminal_to_check.get_y()
        } else {
            top_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (top_resize_border, terminal_ids)
    }
    fn left_aligned_contiguous_panes_below(
        &self,
        id: &RawFd,
        terminal_borders_to_the_left: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).expect("terminal id does not exist");
        let mut left_aligned_terminals = self.panes_left_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        left_aligned_terminals.sort_by(|a, b| a.get_y().cmp(&b.get_y()));
        for terminal in left_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_y() == terminal_to_check.get_y() + terminal_to_check.get_rows() + 1 {
                terminals.push(terminal);
            }
        }
        // bottom-most border aligned with a pane border to the left
        let mut bottom_resize_border = self.full_screen_ws.ws_row as usize;
        for terminal in &terminals {
            let top_terminal_boundary = terminal.get_y();
            if terminal_borders_to_the_left
                .get(&(top_terminal_boundary))
                .is_some()
                && top_terminal_boundary < bottom_resize_border
            {
                bottom_resize_border = top_terminal_boundary;
            }
        }
        terminals.retain(|terminal| {
            // terminal.get_y() + terminal.get_rows() < bottom_resize_border
            terminal.get_y() + terminal.get_rows() <= bottom_resize_border
        });
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let bottom_resize_border = if terminals.is_empty() {
            terminal_to_check.get_y() + terminal_to_check.get_rows()
        } else {
            bottom_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (bottom_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_left(
        &self,
        id: &RawFd,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).expect("terminal id does not exist");
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by(|a, b| b.get_x().cmp(&a.get_x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_x() + terminal.get_columns() + 1 == terminal_to_check.get_x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.get_x() + terminal.get_columns();
            if terminal_borders_above
                .get(&(right_terminal_boundary + 1))
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.get_x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.get_x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (left_resize_border, terminal_ids)
    }
    fn top_aligned_contiguous_panes_to_the_right(
        &self,
        id: &RawFd,
        terminal_borders_above: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut top_aligned_terminals = self.panes_top_aligned_with_pane(&terminal_to_check);
        // terminals that are next to each other up to current
        top_aligned_terminals.sort_by(|a, b| a.get_x().cmp(&b.get_x()));
        for terminal in top_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_x() == terminal_to_check.get_x() + terminal_to_check.get_columns() + 1 {
                terminals.push(terminal);
            }
        }
        // rightmost border aligned with a pane border above
        let mut right_resize_border = self.full_screen_ws.ws_col as usize;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.get_x();
            if terminal_borders_above
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals
            .retain(|terminal| terminal.get_x() + terminal.get_columns() <= right_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.get_x() + terminal_to_check.get_columns()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (right_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_left(
        &self,
        id: &RawFd,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(&terminal_to_check);
        bottom_aligned_terminals.sort_by(|a, b| b.get_x().cmp(&a.get_x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_x() + terminal.get_columns() + 1 == terminal_to_check.get_x() {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut left_resize_border = 0;
        for terminal in &terminals {
            let right_terminal_boundary = terminal.get_x() + terminal.get_columns();
            if terminal_borders_below
                .get(&(right_terminal_boundary + 1))
                .is_some()
                && left_resize_border < right_terminal_boundary
            {
                left_resize_border = right_terminal_boundary + 1;
            }
        }
        terminals.retain(|terminal| terminal.get_x() >= left_resize_border);
        // if there are no adjacent panes to resize, we use the border of the main pane we're
        // resizing
        let left_resize_border = if terminals.is_empty() {
            terminal_to_check.get_x()
        } else {
            left_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (left_resize_border, terminal_ids)
    }
    fn bottom_aligned_contiguous_panes_to_the_right(
        &self,
        id: &RawFd,
        terminal_borders_below: &HashSet<usize>,
    ) -> BorderAndPaneIds {
        let mut terminals = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals = self.panes_bottom_aligned_with_pane(&terminal_to_check);
        bottom_aligned_terminals.sort_by(|a, b| a.get_x().cmp(&b.get_x()));
        // terminals that are next to each other up to current
        for terminal in bottom_aligned_terminals {
            let terminal_to_check = terminals.last().unwrap_or(&terminal_to_check);
            if terminal.get_x() == terminal_to_check.get_x() + terminal_to_check.get_columns() + 1 {
                terminals.push(terminal);
            }
        }
        // leftmost border aligned with a pane border above
        let mut right_resize_border = self.full_screen_ws.ws_col as usize;
        for terminal in &terminals {
            let left_terminal_boundary = terminal.get_x();
            if terminal_borders_below
                .get(&left_terminal_boundary)
                .is_some()
                && right_resize_border > left_terminal_boundary
            {
                right_resize_border = left_terminal_boundary;
            }
        }
        terminals
            .retain(|terminal| terminal.get_x() + terminal.get_columns() <= right_resize_border);
        let right_resize_border = if terminals.is_empty() {
            terminal_to_check.get_x() + terminal_to_check.get_columns()
        } else {
            right_resize_border
        };
        let terminal_ids: Vec<RawFd> = terminals.iter().map(|t| t.pid).collect();
        (right_resize_border, terminal_ids)
    }
    fn reduce_pane_height_down(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(id).unwrap();
        terminal.reduce_height_down(count);
        self.os_api.set_terminal_size_using_fd(
            *id,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn reduce_pane_height_up(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(id).unwrap();
        terminal.reduce_height_up(count);
        self.os_api.set_terminal_size_using_fd(
            *id,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn increase_pane_height_down(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.increase_height_down(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn increase_pane_height_up(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.increase_height_up(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn increase_pane_width_right(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.increase_width_right(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn increase_pane_width_left(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.increase_width_left(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn reduce_pane_width_right(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.reduce_width_right(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn reduce_pane_width_left(&mut self, id: &RawFd, count: usize) {
        let terminal = self.terminals.get_mut(&id).unwrap();
        terminal.reduce_width_left(count);
        self.os_api.set_terminal_size_using_fd(
            terminal.pid,
            terminal.get_columns() as u16,
            terminal.get_rows() as u16,
        );
    }
    fn pane_is_between_vertical_borders(
        &self,
        id: &RawFd,
        left_border_x: usize,
        right_border_x: usize,
    ) -> bool {
        let terminal = self
            .terminals
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.get_x() >= left_border_x
            && terminal.get_x() + terminal.get_columns() <= right_border_x
    }
    fn pane_is_between_horizontal_borders(
        &self,
        id: &RawFd,
        top_border_y: usize,
        bottom_border_y: usize,
    ) -> bool {
        let terminal = self
            .terminals
            .get(id)
            .expect("could not find terminal to check between borders");
        terminal.get_y() >= top_border_y
            && terminal.get_y() + terminal.get_rows() <= bottom_border_y
    }
    fn reduce_pane_and_surroundings_up(&mut self, id: &RawFd, count: usize) {
        let mut terminals_below = self
            .terminal_ids_directly_below(&id)
            .expect("can't reduce pane size up if there are no terminals below");
        let terminal_borders_below: HashSet<usize> = terminals_below
            .iter()
            .map(|t| self.terminals.get(t).unwrap().get_x())
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
            .map(|t| self.terminals.get(t).unwrap().get_x())
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
            .map(|t| self.terminals.get(t).unwrap().get_y())
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
            .map(|t| self.terminals.get(t).unwrap().get_y())
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
            .map(|t| self.terminals.get(t).unwrap().get_x())
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
            .map(|t| self.terminals.get(t).unwrap().get_x())
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
            .map(|t| self.terminals.get(t).unwrap().get_y())
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
            .map(|t| self.terminals.get(t).unwrap().get_y())
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
        let pane = self.terminals.get(pane_id).expect("pane does not exist");
        pane.get_y() > 0
    }
    fn panes_exist_below(&self, pane_id: &RawFd) -> bool {
        let pane = self.terminals.get(pane_id).expect("pane does not exist");
        pane.get_y() + pane.get_rows() < self.full_screen_ws.ws_row as usize
    }
    fn panes_exist_to_the_right(&self, pane_id: &RawFd) -> bool {
        let pane = self.terminals.get(pane_id).expect("pane does not exist");
        pane.get_x() + pane.get_columns() < self.full_screen_ws.ws_col as usize
    }
    fn panes_exist_to_the_left(&self, pane_id: &RawFd) -> bool {
        let pane = self.terminals.get(pane_id).expect("pane does not exist");
        pane.get_x() > 0
    }
    pub fn resize_right(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_to_the_right(&active_terminal_id) {
                self.increase_pane_and_surroundings_right(&active_terminal_id, count);
                self.render();
            } else if self.panes_exist_to_the_left(&active_terminal_id) {
                self.reduce_pane_and_surroundings_right(&active_terminal_id, count);
                self.render();
            }
        }
    }
    pub fn resize_left(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_to_the_right(&active_terminal_id) {
                self.reduce_pane_and_surroundings_left(&active_terminal_id, count);
                self.render();
            } else if self.panes_exist_to_the_left(&active_terminal_id) {
                self.increase_pane_and_surroundings_left(&active_terminal_id, count);
                self.render();
            }
        }
    }
    pub fn resize_down(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_above(&active_terminal_id) {
                self.reduce_pane_and_surroundings_down(&active_terminal_id, count);
                self.render();
            } else if self.panes_exist_below(&active_terminal_id) {
                self.increase_pane_and_surroundings_down(&active_terminal_id, count);
                self.render();
            }
        }
    }
    pub fn resize_up(&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            if self.panes_exist_above(&active_terminal_id) {
                self.increase_pane_and_surroundings_up(&active_terminal_id, count);
                self.render();
            } else if self.panes_exist_below(&active_terminal_id) {
                self.reduce_pane_and_surroundings_up(&active_terminal_id, count);
                self.render();
            }
        }
    }
    pub fn move_focus(&mut self) {
        if self.terminals.is_empty() {
            return;
        }
        if self.fullscreen_is_active {
            return;
        }
        let active_terminal_id = self.get_active_terminal_id().unwrap();
        let terminal_ids: Vec<RawFd> = self.terminals.keys().copied().collect(); // TODO: better, no allocations
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
    fn horizontal_borders(&self, terminals: &[RawFd]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.terminals.get(t).unwrap();
            borders.insert(terminal.get_y());
            borders.insert(terminal.get_y() + terminal.get_rows() + 1); // 1 for the border width
            borders
        })
    }
    fn vertical_borders(&self, terminals: &[RawFd]) -> HashSet<usize> {
        terminals.iter().fold(HashSet::new(), |mut borders, t| {
            let terminal = self.terminals.get(t).unwrap();
            borders.insert(terminal.get_x());
            borders.insert(terminal.get_x() + terminal.get_columns() + 1); // 1 for the border width
            borders
        })
    }
    fn terminals_to_the_left_between_aligning_borders(&self, id: RawFd) -> Option<Vec<RawFd>> {
        if let Some(terminal) = &self.terminals.get(&id) {
            let upper_close_border = terminal.get_y();
            let lower_close_border = terminal.get_y() + terminal.get_rows() + 1;

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
        if let Some(terminal) = &self.terminals.get(&id) {
            let upper_close_border = terminal.get_y();
            let lower_close_border = terminal.get_y() + terminal.get_rows() + 1;

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
        if let Some(terminal) = &self.terminals.get(&id) {
            let left_close_border = terminal.get_x();
            let right_close_border = terminal.get_x() + terminal.get_columns() + 1;

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
        if let Some(terminal) = &self.terminals.get(&id) {
            let left_close_border = terminal.get_x();
            let right_close_border = terminal.get_x() + terminal.get_columns() + 1;

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
            if self.terminals.len() >= max_panes {
                for _ in max_panes..=self.terminals.len() {
                    let first_pid = *self.terminals.iter().next().unwrap().0;
                    self.send_pty_instructions
                        .send(PtyInstruction::ClosePane(first_pid))
                        .unwrap();
                    self.close_pane_without_rerender(first_pid); // TODO: do not render yet
                }
            }
        }
    }
    pub fn close_pane(&mut self, id: RawFd) {
        if self.terminals.get(&id).is_some() {
            self.close_pane_without_rerender(id);
            self.render();
        }
    }
    pub fn close_pane_without_rerender(&mut self, id: RawFd) {
        if let Some(terminal_to_close) = &self.terminals.get(&id) {
            let terminal_to_close_width = terminal_to_close.get_columns();
            let terminal_to_close_height = terminal_to_close.get_rows();
            if let Some(terminals) = self.terminals_to_the_left_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    &self.increase_pane_width_right(&terminal_id, terminal_to_close_width + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_to_the_right_between_aligning_borders(id)
            {
                for terminal_id in terminals.iter() {
                    &self.increase_pane_width_left(&terminal_id, terminal_to_close_width + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_above_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    &self.increase_pane_height_down(&terminal_id, terminal_to_close_height + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else if let Some(terminals) = self.terminals_below_between_aligning_borders(id) {
                for terminal_id in terminals.iter() {
                    &self.increase_pane_height_up(&terminal_id, terminal_to_close_height + 1);
                    // 1 for the border
                }
                if self.active_terminal == Some(id) {
                    self.active_terminal = Some(*terminals.last().unwrap());
                }
            } else {
            }
            self.terminals.remove(&id);
            if self.terminals.is_empty() {
                self.active_terminal = None;
                let _ = self.send_app_instructions.send(AppInstruction::Exit);
            }
        }
    }
    pub fn close_focused_pane(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            self.send_pty_instructions
                .send(PtyInstruction::ClosePane(active_terminal_id))
                .unwrap();
            self.close_pane(active_terminal_id);
        }
    }
    pub fn scroll_active_terminal_up(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
            active_terminal.scroll_up(1);
            self.render();
        }
    }
    pub fn scroll_active_terminal_down(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
            active_terminal.scroll_down(1);
            self.render();
        }
    }
    pub fn clear_active_terminal_scroll(&mut self) {
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
            active_terminal.clear_scroll();
        }
    }
}
