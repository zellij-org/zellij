use crate::tab::Pane;
use ::nix::pty::Winsize;
use ::std::os::unix::io::RawFd;
use std::fmt::Debug;
use std::time::Instant;

use crate::panes::grid::Grid;
use crate::panes::terminal_character::{
    CharacterStyles, TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
};
use crate::pty_bus::VteBytes;

#[derive(PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Debug)]
pub enum PaneId {
    Terminal(RawFd),
    Plugin(u32), // FIXME: Drop the trait object, make this a wrapper for the struct?
}

/// Contains the position and size of a [`Pane`], or more generally of any terminal, measured
/// in character rows and columns.
#[derive(Clone, Copy, Debug, Default)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub columns: usize,
    pub max_rows: Option<usize>,
    pub max_columns: Option<usize>,
}

impl From<Winsize> for PositionAndSize {
    fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            columns: winsize.ws_col as usize,
            rows: winsize.ws_row as usize,
            ..Default::default()
        }
    }
}

pub struct TerminalPane {
    pub grid: Grid,
    pub pid: RawFd,
    pub selectable: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub max_height: Option<usize>,
    pub max_width: Option<usize>,
    pub active_at: Instant,
    vte_parser: vte::Parser,
}

impl Pane for TerminalPane {
    fn x(&self) -> usize {
        self.get_x()
    }
    fn y(&self) -> usize {
        self.get_y()
    }
    fn rows(&self) -> usize {
        self.get_rows()
    }
    fn columns(&self) -> usize {
        self.get_columns()
    }
    fn reset_size_and_position_override(&mut self) {
        self.position_and_size_override = None;
        self.reflow_lines();
    }
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size.columns = position_and_size.columns;
        self.position_and_size.rows = position_and_size.rows;
        self.reflow_lines();
    }
    fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize) {
        let position_and_size_override = PositionAndSize {
            x,
            y,
            rows: size.rows,
            columns: size.columns,
            ..Default::default()
        };
        self.position_and_size_override = Some(position_and_size_override);
        self.reflow_lines();
    }
    fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        for byte in bytes.iter() {
            self.vte_parser.advance(&mut self.grid, *byte);
        }
        self.set_should_render(true);
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
    fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8> {
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
            _ => {}
        };
        input_bytes
    }

    fn position_and_size_override(&self) -> Option<PositionAndSize> {
        self.position_and_size_override
    }
    fn contains_widechar(&self) -> bool {
        self.grid.contains_widechar()
    }
    fn should_render(&self) -> bool {
        self.grid.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn set_max_height(&mut self, max_height: usize) {
        self.max_height = Some(max_height);
    }
    fn set_max_width(&mut self, max_width: usize) {
        self.max_width = Some(max_width);
    }
    fn set_invisible_borders(&mut self, _invisible_borders: bool) {
        unimplemented!();
    }
    fn max_height(&self) -> Option<usize> {
        self.max_height
    }
    fn max_width(&self) -> Option<usize> {
        self.max_width
    }
    fn render(&mut self) -> Option<String> {
        if self.should_render() {
            let mut vte_output = String::new();
            let buffer_lines = &self.read_buffer_as_lines();
            let display_cols = self.get_columns();
            let mut character_styles = CharacterStyles::new();
            if self.grid.clear_viewport_before_rendering {
                for line_index in 0..self.grid.height {
                    let x = self.get_x();
                    let y = self.get_y();
                    vte_output.push_str(&format!(
                        "\u{1b}[{};{}H\u{1b}[m",
                        y + line_index + 1,
                        x + 1
                    )); // goto row/col and reset styles
                    for _col_index in 0..self.grid.width {
                        vte_output.push(EMPTY_TERMINAL_CHARACTER.character);
                    }
                }
                self.grid.clear_viewport_before_rendering = false;
            }
            for (row, line) in buffer_lines.iter().enumerate() {
                let x = self.get_x();
                let y = self.get_y();
                vte_output.push_str(&format!("\u{1b}[{};{}H\u{1b}[m", y + row + 1, x + 1)); // goto row/col and reset styles
                for (col, t_character) in line.iter().enumerate() {
                    if col < display_cols {
                        // in some cases (eg. while resizing) some characters will spill over
                        // before they are corrected by the shell (for the prompt) or by reflowing
                        // lines
                        if let Some(new_styles) =
                            character_styles.update_and_return_diff(&t_character.styles)
                        {
                            // the terminal keeps the previous styles as long as we're in the same
                            // line, so we only want to update the new styles here (this also
                            // includes resetting previous styles as needed)
                            vte_output.push_str(&new_styles.to_string());
                        }
                        vte_output.push(t_character.character);
                    }
                }
                character_styles.clear();
            }
            self.set_should_render(false);
            Some(vte_output)
        } else {
            None
        }
    }
    fn pid(&self) -> PaneId {
        PaneId::Terminal(self.pid)
    }
    fn reduce_height_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.position_and_size.rows -= count;
        self.reflow_lines();
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.reflow_lines();
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.reflow_lines();
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.reflow_lines();
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.columns -= count;
        self.reflow_lines();
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.columns -= count;
        self.reflow_lines();
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.columns += count;
        self.reflow_lines();
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.columns += count;
        self.reflow_lines();
    }
    fn push_down(&mut self, count: usize) {
        self.position_and_size.y += count;
    }
    fn push_right(&mut self, count: usize) {
        self.position_and_size.x += count;
    }
    fn pull_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
    }
    fn pull_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
    }
    fn scroll_up(&mut self, count: usize) {
        self.grid.move_viewport_up(count);
        self.set_should_render(true);
    }
    fn scroll_down(&mut self, count: usize) {
        self.grid.move_viewport_down(count);
        self.set_should_render(true);
    }
    fn clear_scroll(&mut self) {
        self.grid.reset_viewport();
        self.set_should_render(true);
    }

    fn active_at(&self) -> Instant {
        self.active_at
    }

    fn set_active_at(&mut self, time: Instant) {
        self.active_at = time;
    }
}

impl TerminalPane {
    pub fn new(pid: RawFd, position_and_size: PositionAndSize) -> TerminalPane {
        let grid = Grid::new(position_and_size.rows, position_and_size.columns);
        TerminalPane {
            pid,
            grid,
            selectable: true,
            position_and_size,
            position_and_size_override: None,
            max_height: None,
            max_width: None,
            vte_parser: vte::Parser::new(),
            active_at: Instant::now(),
        }
    }
    pub fn get_x(&self) -> usize {
        match self.position_and_size_override {
            Some(position_and_size_override) => position_and_size_override.x,
            None => self.position_and_size.x as usize,
        }
    }
    pub fn get_y(&self) -> usize {
        match self.position_and_size_override {
            Some(position_and_size_override) => position_and_size_override.y,
            None => self.position_and_size.y as usize,
        }
    }
    pub fn get_columns(&self) -> usize {
        match &self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.columns,
            None => self.position_and_size.columns as usize,
        }
    }
    pub fn get_rows(&self) -> usize {
        match &self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.rows,
            None => self.position_and_size.rows as usize,
        }
    }
    fn reflow_lines(&mut self) {
        let rows = self.get_rows();
        let columns = self.get_columns();
        self.grid.change_size(rows, columns);
        self.set_should_render(true);
    }
    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.grid.as_character_lines()
    }
    #[cfg(test)]
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
}
