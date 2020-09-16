use std::io::Write;
use std::collections::{HashSet, BTreeMap};
use nix::pty::Winsize;
use std::os::unix::io::RawFd;
use std::sync::mpsc::{channel, Sender, Receiver};

use crate::os_input_output::OsApi;
use crate::terminal_pane::{TerminalOutput, TerminalCharacter};
use crate::pty_bus::VteEvent;

fn split_vertically_with_gap (rect: &Winsize) -> (Winsize, Winsize) {
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

fn split_horizontally_with_gap (rect: &Winsize) -> (Winsize, Winsize) {
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
    HorizontalSplit(RawFd),
    VerticalSplit(RawFd),
    WriteCharacter(u8),
    ResizeLeft,
    ResizeRight,
    ResizeDown,
    ResizeUp,
    MoveFocus,
    Quit,
}

pub struct Screen {
    pub receiver: Receiver<ScreenInstruction>,
    pub send_screen_instructions: Sender<ScreenInstruction>,
    full_screen_ws: Winsize,
    vertical_separator: TerminalCharacter, // TODO: better
    horizontal_separator: TerminalCharacter, // TODO: better
    terminals: BTreeMap<RawFd, TerminalOutput>, // BTreeMap because we need a predictable order when changing focus
    active_terminal: Option<RawFd>,
    os_api: Box<dyn OsApi>,
}

impl Screen {
    pub fn new (full_screen_ws: &Winsize, os_api: Box<dyn OsApi>) -> Self {
        let (sender, receiver): (Sender<ScreenInstruction>, Receiver<ScreenInstruction>) = channel();
        Screen {
            receiver,
            send_screen_instructions: sender,
            full_screen_ws: full_screen_ws.clone(),
            vertical_separator: TerminalCharacter::new('│').ansi_code(String::from("\u{1b}[m")), // TODO: better
            horizontal_separator: TerminalCharacter::new('─').ansi_code(String::from("\u{1b}[m")), // TODO: better
            terminals: BTreeMap::new(),
            active_terminal: None,
            os_api,
        }
    }
    pub fn horizontal_split(&mut self, pid: RawFd) {
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalOutput::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, new_terminal.display_cols, new_terminal.display_rows);
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x_coords, active_terminal_y_coords) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.display_rows,
                        ws_col: active_terminal.display_cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.x_coords,
                    active_terminal.y_coords
                )
            };
            let (top_winsize, bottom_winsize) = split_horizontally_with_gap(&active_terminal_ws);
            let bottom_half_y = active_terminal_y_coords + top_winsize.ws_row + 1;
            let new_terminal = TerminalOutput::new(pid, bottom_winsize, active_terminal_x_coords, bottom_half_y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, bottom_winsize.ws_col, bottom_winsize.ws_row);

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&top_winsize);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(active_terminal_pid, top_winsize.ws_col, top_winsize.ws_row);
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    pub fn vertical_split(&mut self, pid: RawFd) {
        if self.terminals.is_empty() {
            let x = 0;
            let y = 0;
            let new_terminal = TerminalOutput::new(pid, self.full_screen_ws.clone(), x, y);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, new_terminal.display_cols, new_terminal.display_rows);
            self.terminals.insert(pid, new_terminal);
            self.active_terminal = Some(pid);
        } else {
            // TODO: check minimum size of active terminal
            let (active_terminal_ws, active_terminal_x_coords, active_terminal_y_coords) = {
                let active_terminal = &self.get_active_terminal().unwrap();
                (
                    Winsize {
                        ws_row: active_terminal.display_rows,
                        ws_col: active_terminal.display_cols,
                        ws_xpixel: 0,
                        ws_ypixel: 0,
                    },
                    active_terminal.x_coords,
                    active_terminal.y_coords
                )
            };
            let (left_winszie, right_winsize) = split_vertically_with_gap(&active_terminal_ws);
            let right_side_x = active_terminal_x_coords + left_winszie.ws_col + 1;
            let new_terminal = TerminalOutput::new(pid, right_winsize, right_side_x, active_terminal_y_coords);
            self.os_api.set_terminal_size_using_fd(new_terminal.pid, right_winsize.ws_col, right_winsize.ws_row);

            {
                let active_terminal_id = &self.get_active_terminal_id().unwrap();
                let active_terminal = &mut self.terminals.get_mut(&active_terminal_id).unwrap();
                active_terminal.change_size(&left_winszie);
            }

            self.terminals.insert(pid, new_terminal);
            let active_terminal_pid = self.get_active_terminal_id().unwrap();
            self.os_api.set_terminal_size_using_fd(active_terminal_pid, left_winszie.ws_col, left_winszie.ws_row);
            self.active_terminal = Some(pid);
            self.render();
        }
    }
    fn get_active_terminal (&self) -> Option<&TerminalOutput> {
        match self.active_terminal {
            Some(active_terminal) => self.terminals.get(&active_terminal),
            None => None
        }
    }
    fn get_active_terminal_id (&self) -> Option<RawFd> {
        match self.active_terminal {
            Some(active_terminal) => Some(self.terminals.get(&active_terminal).unwrap().pid),
            None => None
        }
    }
    pub fn handle_pty_event(&mut self, pid: RawFd, event: VteEvent) {
        let terminal_output = self.terminals.get_mut(&pid).unwrap();
        terminal_output.handle_event(event);
    }
    pub fn write_to_active_terminal(&mut self, byte: u8) {
        if let Some(active_terminal_id) = &self.get_active_terminal_id() {
            let mut buffer = [byte];
            self.os_api.write_to_tty_stdin(*active_terminal_id, &mut buffer).expect("failed to write to terminal");
            self.os_api.tcdrain(*active_terminal_id).expect("failed to drain terminal");
        }
    }
    fn get_active_terminal_cursor_position(&self) -> (usize, usize) { // (x, y)
        let active_terminal = &self.get_active_terminal().unwrap();
        let x = active_terminal.x_coords as usize + active_terminal.cursor_position_in_last_line();
        let y = active_terminal.y_coords + active_terminal.display_rows - 1;
        (x, y as usize)
    }
    pub fn render (&mut self) {
        let mut stdout = self.os_api.get_stdout_writer();
        for (_pid, terminal) in self.terminals.iter_mut() {
            if let Some(vte_output) = terminal.buffer_as_vte_output() {

                // write boundaries
                if terminal.x_coords + terminal.display_cols < self.full_screen_ws.ws_col {
                    let boundary_x_coords = terminal.x_coords + terminal.display_cols;
                    let mut vte_output_boundaries = String::new();
                    for row in terminal.y_coords..terminal.y_coords + terminal.display_rows {
                        vte_output_boundaries.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", row + 1, boundary_x_coords + 1)); // goto row/col
                        vte_output_boundaries.push_str(&self.vertical_separator.to_string());
                    }
                    stdout.write_all(&vte_output_boundaries.as_bytes()).expect("cannot write to stdout");
                }
                if terminal.y_coords + terminal.display_rows < self.full_screen_ws.ws_row {
                    let boundary_y_coords = terminal.y_coords + terminal.display_rows;
                    let mut vte_output_boundaries = String::new();
                    for col in terminal.x_coords..terminal.x_coords + terminal.display_cols {
                        vte_output_boundaries.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", boundary_y_coords + 1, col + 1)); // goto row/col
                        vte_output_boundaries.push_str(&self.horizontal_separator.to_string());
                    }
                    stdout.write_all(&vte_output_boundaries.as_bytes()).expect("cannot write to stdout");
                }

                stdout.write_all(&vte_output.as_bytes()).expect("cannot write to stdout");
            }
        }
        let (cursor_position_x, cursor_position_y) = self.get_active_terminal_cursor_position();
        let goto_cursor_position = format!("\u{1b}[{};{}H\u{1b}[m", cursor_position_y + 1, cursor_position_x + 1); // goto row/col
        stdout.write_all(&goto_cursor_position.as_bytes()).expect("cannot write to stdout");
        stdout.flush().expect("could not flush");
    }
    fn terminal_ids_directly_left_of(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        if terminal_to_check.x_coords == 0 {
            return None;
        }
        for (pid, terminal) in self.terminals.iter() {
            if terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords - 1 {
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
            if terminal.x_coords == terminal_to_check.x_coords + terminal_to_check.display_cols + 1 {
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
            if terminal.y_coords == terminal_to_check.y_coords + terminal_to_check.display_rows + 1 {
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
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(*pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above_with_same_left_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut left_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords == terminal_to_check.x_coords)
            .collect();
        left_aligned_terminals.sort_by(|a, b| { b.y_coords.cmp(&a.y_coords)});

        for terminal in left_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below_with_same_left_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut left_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords == terminal_to_check.x_coords)
            .collect();
        left_aligned_terminals.sort_by(|a, b| { a.y_coords.cmp(&b.y_coords)});

        for terminal in left_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.y_coords + terminal_to_check.display_rows + 1 == terminal.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_above_with_same_right_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut right_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords + terminal_to_check.display_cols)
            .collect();
        right_aligned_terminals.sort_by(|a, b| { b.y_coords.cmp(&a.y_coords)});

        for terminal in right_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.y_coords + terminal.display_rows + 1 == terminal_to_check.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_below_with_same_right_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut right_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.x_coords + terminal.display_cols == terminal_to_check.x_coords + terminal_to_check.display_cols)
            .collect();
        right_aligned_terminals.sort_by(|a, b| { a.y_coords.cmp(&b.y_coords)});

        for terminal in right_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.y_coords + terminal_to_check.display_rows + 1 == terminal.y_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_left_with_same_top_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut top_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords == terminal_to_check.y_coords)
            .collect();
        top_aligned_terminals.sort_by(|a, b| { b.x_coords.cmp(&a.x_coords)});

        for terminal in top_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.x_coords + terminal.display_cols + 1 == terminal_to_check.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_left_with_same_bottom_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords + terminal.display_rows == terminal_to_check.y_coords + terminal_to_check.display_rows)
            .collect();
        bottom_aligned_terminals.sort_by(|a, b| { b.x_coords.cmp(&a.x_coords)});

        for terminal in bottom_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal.x_coords + terminal.display_cols + 1 == terminal_to_check.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_right_with_same_top_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut top_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords == terminal_to_check.y_coords)
            .collect();
        top_aligned_terminals.sort_by(|a, b| { a.x_coords.cmp(&b.x_coords)});

        for terminal in top_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.x_coords + terminal_to_check.display_cols + 1 == terminal.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    fn terminal_ids_directly_to_the_right_with_same_bottom_alignment(&self, id: &RawFd) -> Option<Vec<RawFd>> {
        let mut ids = vec![];
        let terminal_to_check = self.terminals.get(id).unwrap();
        let mut bottom_aligned_terminals: Vec<&TerminalOutput> = self.terminals
            .keys()
            .map(|t_id| self.terminals.get(&t_id).unwrap())
            .filter(|terminal| terminal.pid != *id && terminal.y_coords + terminal.display_rows == terminal_to_check.y_coords + terminal_to_check.display_rows)
            .collect();
        bottom_aligned_terminals.sort_by(|a, b| { a.x_coords.cmp(&b.x_coords)});

        for terminal in bottom_aligned_terminals {
            let terminal_to_check = ids
                .last()
                .and_then(|id| self.terminals.get(id))
                .unwrap_or(terminal_to_check);
            if terminal_to_check.x_coords + terminal_to_check.display_cols + 1 == terminal.x_coords {
                ids.push(terminal.pid);
            }
        }
        if ids.is_empty() {
            None
        } else {
            Some(ids)
        }
    }
    pub fn resize_left (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_to_the_left = self.terminal_ids_directly_left_of(&active_terminal_id);
            let terminals_to_the_right = self.terminal_ids_directly_right_of(&active_terminal_id);
            match (terminals_to_the_left, terminals_to_the_right) {
                (_, Some(mut terminals_to_the_right)) => {
                    // reduce to the left
                    let terminal_borders_to_the_right: HashSet<u16> = terminals_to_the_right.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_right.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_right.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.reduce_width_left(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_right.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_right {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_width_left(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_to_the_left), None) => {
                    // increase to the left 
                    let terminal_borders_to_the_left: HashSet<u16> = terminals_to_the_left.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_left.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_left.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.increase_width_left(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_left.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_left {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_width_left(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_left(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_right (&mut self) {
        let count = 10;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_to_the_left = self.terminal_ids_directly_left_of(&active_terminal_id);
            let terminals_to_the_right = self.terminal_ids_directly_right_of(&active_terminal_id);
            match (terminals_to_the_left, terminals_to_the_right) {
                (_, Some(mut terminals_to_the_right)) => {
                    // increase to the right
                    let terminal_borders_to_the_right: HashSet<u16> = terminals_to_the_right.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_right.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_right_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_right.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };

                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.increase_width_right(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_right.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_right {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_width_right(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_to_the_left), None) => {
                    // reduce to the right
                    let terminal_borders_to_the_left: HashSet<u16> = terminals_to_the_left.iter().map(|t| self.terminals.get(t).unwrap().y_coords).collect();
                    let terminals_above_and_upper_resize_border = self.terminal_ids_directly_above_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut upper_resize_border = 0;
                            for terminal in &t {
                                let lower_terminal_boundary = terminal.y_coords + terminal.display_rows;
                                if terminal_borders_to_the_left.get(&(lower_terminal_boundary + 1)).is_some() && upper_resize_border < lower_terminal_boundary {
                                    upper_resize_border = lower_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords >= upper_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, upper_resize_border))
                        });
                    let terminals_below_and_lower_resize_border = self.terminal_ids_directly_below_with_same_left_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut lower_resize_border = self.full_screen_ws.ws_row;
                            for terminal in &t {
                                let upper_terminal_boundary = terminal.y_coords;
                                if terminal_borders_to_the_left.get(&upper_terminal_boundary).is_some() && lower_resize_border > upper_terminal_boundary {
                                    lower_resize_border = upper_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.y_coords + terminal.display_rows <= lower_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, lower_resize_border))
                        });
                    let (terminals_above, upper_resize_border) = match terminals_above_and_upper_resize_border {
                        Some((terminals_above, upper_resize_border)) => (Some(terminals_above), Some(upper_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_below, lower_resize_border) = match terminals_below_and_lower_resize_border {
                        Some((terminals_below, lower_resize_border)) => (Some(terminals_below), Some(lower_resize_border)),
                        None => (None, None),
                    };

                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let upper_resize_border = upper_resize_border.unwrap_or(active_terminal.y_coords);
                    let lower_resize_border = lower_resize_border.unwrap_or(active_terminal.y_coords + active_terminal.display_rows);

                    active_terminal.reduce_width_right(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_to_the_left.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.y_coords >= upper_resize_border && terminal.y_coords + terminal.display_rows <= lower_resize_border
                    });
                    for terminal_id in terminals_to_the_left {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_width_right(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_above) = terminals_above {
                        for terminal_id in terminals_above.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_below) = terminals_below {
                        for terminal_id in terminals_below.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_width_right(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_down (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_below = self.terminal_ids_directly_below(&active_terminal_id);
            let terminals_above = self.terminal_ids_directly_above(&active_terminal_id);
            match (terminals_below, terminals_above) {
                (_, Some(mut terminals_above)) => {
                    // reduce down
                    let terminal_borders_above: HashSet<u16> = terminals_above.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_above.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_above.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border 
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.reduce_height_down(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_above.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border
                    });
                    for terminal_id in terminals_above {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_height_down(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_below), None) => {
                    // increase down
                    let terminal_borders_below: HashSet<u16> = terminals_below.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_below.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_below.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.increase_height_down(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_below.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border 
                    });
                    for terminal_id in terminals_below {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_height_down(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_down(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn resize_up (&mut self) {
        // TODO: find out by how much we actually reduced and only reduce by that much
        let count = 2;
        if let Some(active_terminal_id) = self.get_active_terminal_id() {
            let terminals_below = self.terminal_ids_directly_below(&active_terminal_id);
            let terminals_above = self.terminal_ids_directly_above(&active_terminal_id);
            match (terminals_below, terminals_above) {
                (_, Some(mut terminals_above)) => {
                    // reduce down
                    let terminal_borders_above: HashSet<u16> = terminals_above.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_above.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_top_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_above.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border 
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols);

                    active_terminal.increase_height_up(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_above.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border
                    });
                    for terminal_id in terminals_above {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.reduce_height_up(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.increase_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (Some(mut terminals_below), None) => {
                    // increase down
                    let terminal_borders_below: HashSet<u16> = terminals_below.iter().map(|t| self.terminals.get(t).unwrap().x_coords).collect();
                    let terminals_to_the_left_and_left_resize_border = self.terminal_ids_directly_to_the_left_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut left_resize_border = 0;
                            for terminal in &t {
                                let right_terminal_boundary = terminal.x_coords + terminal.display_cols;
                                if terminal_borders_below.get(&(right_terminal_boundary + 1)).is_some() && left_resize_border < right_terminal_boundary {
                                    left_resize_border = right_terminal_boundary + 1;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords >= left_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, left_resize_border))
                        });
                    let terminals_to_the_right_and_right_resize_border = self.terminal_ids_directly_to_the_right_with_same_bottom_alignment(&active_terminal_id)
                        .and_then(|t| {
                            let terminals: Vec<&TerminalOutput> = t.iter().map(|t| self.terminals.get(t).unwrap()).collect();
                            Some(terminals)
                        })
                        .and_then(|mut t| {
                            let mut right_resize_border = self.full_screen_ws.ws_col;
                            for terminal in &t {
                                let left_terminal_boundary = terminal.x_coords;
                                if terminal_borders_below.get(&left_terminal_boundary).is_some() && right_resize_border > left_terminal_boundary {
                                    right_resize_border = left_terminal_boundary;
                                }
                            }
                            t.retain(|terminal| {
                                terminal.x_coords + terminal.display_cols <= right_resize_border
                            });
                            let terminal_ids: Vec<RawFd> = t.iter().map(|t| t.pid).collect();
                            Some((terminal_ids, right_resize_border))
                        });
                    let (terminals_to_the_left, left_resize_border) = match terminals_to_the_left_and_left_resize_border {
                        Some((terminals_to_the_left, left_resize_border)) => (Some(terminals_to_the_left), Some(left_resize_border)),
                        None => (None, None),
                    };
                    let (terminals_to_the_right, right_resize_border) = match terminals_to_the_right_and_right_resize_border {
                        Some((terminals_to_the_right, right_resize_border)) => (Some(terminals_to_the_right), Some(right_resize_border)),
                        None => (None, None),
                    };
                    let active_terminal = self.terminals.get_mut(&active_terminal_id).unwrap();
                    let left_resize_border = left_resize_border.unwrap_or(active_terminal.x_coords);
                    let right_resize_border = right_resize_border.unwrap_or(active_terminal.x_coords + active_terminal.display_cols); // TODO: + 1?

                    active_terminal.reduce_height_up(count);
                    self.os_api.set_terminal_size_using_fd(
                        active_terminal.pid,
                        active_terminal.display_cols,
                        active_terminal.display_rows
                    );

                    terminals_below.retain(|t| {
                        let terminal = self.terminals.get(t).unwrap();
                        terminal.x_coords >= left_resize_border && terminal.x_coords + terminal.display_cols <= right_resize_border 
                    });
                    for terminal_id in terminals_below {
                        let terminal = self.terminals.get_mut(&terminal_id).unwrap();
                        terminal.increase_height_up(count);
                        self.os_api.set_terminal_size_using_fd(
                            terminal.pid,
                            terminal.display_cols,
                            terminal.display_rows
                        );
                    }

                    if let Some(terminals_to_the_left) = terminals_to_the_left {
                        for terminal_id in terminals_to_the_left.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }

                    if let Some(terminals_to_the_right) = terminals_to_the_right {
                        for terminal_id in terminals_to_the_right.iter() {
                            let terminal = self.terminals.get_mut(terminal_id).unwrap();
                            terminal.reduce_height_up(count);
                            self.os_api.set_terminal_size_using_fd(
                                terminal.pid,
                                terminal.display_cols,
                                terminal.display_rows
                            );
                        }
                    }
                },
                (None, None) => {}
            }
            self.render();
        }
    }
    pub fn move_focus(&mut self) {
        if self.terminals.is_empty() {
            return;
        }
        let active_terminal_id = self.get_active_terminal_id().unwrap();
        let terminal_ids: Vec<RawFd> = self.terminals.keys().copied().collect(); // TODO: better, no allocations
        let first_terminal = terminal_ids.get(0).unwrap();
        let active_terminal_id_position = terminal_ids.iter().position(|id| id == &active_terminal_id).unwrap();
        if let Some(next_terminal) = terminal_ids.get(active_terminal_id_position + 1) {
            self.active_terminal = Some(*next_terminal);
        } else {
            self.active_terminal = Some(*first_terminal);
        }
        self.render();
    }
}
