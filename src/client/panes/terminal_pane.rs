#![allow(clippy::clippy::if_same_then_else)]

use crate::tab::Pane;
use ::nix::pty::Winsize;
use ::std::os::unix::io::RawFd;
use ::vte::Perform;
use std::fmt::Debug;

use crate::panes::grid::Grid;
use crate::panes::terminal_character::{
    CharacterStyles, TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
};
use crate::utils::logging::debug_log_to_file;
use crate::VteEvent;

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

#[derive(Debug)]
pub struct TerminalPane {
    pub grid: Grid,
    pub alternative_grid: Option<Grid>, // for 1049h/l instructions which tell us to switch between these two
    pub pid: RawFd,
    pub should_render: bool,
    pub selectable: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub cursor_key_mode: bool, // DECCKM - when set, cursor keys should send ANSI direction codes (eg. "OD") instead of the arrow keys (eg. "[D")
    pub max_height: Option<usize>,
    pending_styles: CharacterStyles,
    clear_viewport_before_rendering: bool,
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
        self.mark_for_rerender();
    }
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size.columns = position_and_size.columns;
        self.position_and_size.rows = position_and_size.rows;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize) {
        let position_and_size_override = PositionAndSize {
            x,
            y,
            rows: size.rows,
            columns: size.columns,
        };
        self.position_and_size_override = Some(position_and_size_override);
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn handle_event(&mut self, event: VteEvent) {
        match event {
            VteEvent::Print(c) => {
                self.print(c);
                self.mark_for_rerender();
            }
            VteEvent::Execute(byte) => {
                self.execute(byte);
            }
            VteEvent::Hook(params, intermediates, ignore, c) => {
                self.hook(&params, &intermediates, ignore, c);
            }
            VteEvent::Put(byte) => {
                self.put(byte);
            }
            VteEvent::Unhook => {
                self.unhook();
            }
            VteEvent::OscDispatch(params, bell_terminated) => {
                let params: Vec<&[u8]> = params.iter().map(|p| &p[..]).collect();
                self.osc_dispatch(&params[..], bell_terminated);
            }
            VteEvent::CsiDispatch(params, intermediates, ignore, c) => {
                self.csi_dispatch(&params, &intermediates, ignore, c);
            }
            VteEvent::EscDispatch(intermediates, ignore, byte) => {
                self.esc_dispatch(&intermediates, ignore, byte);
            }
        }
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
                if self.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OD".as_bytes().to_vec();
                }
            }
            [27, 91, 67] => {
                // right arrow
                if self.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OC".as_bytes().to_vec();
                }
            }
            [27, 91, 65] => {
                // up arrow
                if self.cursor_key_mode {
                    // please note that in the line below, there is an ANSI escape code (27) at the beginning of the string,
                    // some editors will not show this
                    return "OA".as_bytes().to_vec();
                }
            }
            [27, 91, 66] => {
                // down arrow
                if self.cursor_key_mode {
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
    fn set_max_height(&mut self, max_height: usize) {
        self.max_height = Some(max_height);
    }
    fn set_invisible_borders(&mut self, _invisible_borders: bool) {
        unimplemented!();
    }
    fn max_height(&self) -> Option<usize> {
        self.max_height
    }
    fn render(&mut self) -> Option<String> {
        // if self.should_render {
        if true {
            // while checking should_render rather than rendering each pane every time
            // is more performant, it causes some problems when the pane to the left should be
            // rendered and has wide characters (eg. Chinese characters or emoji)
            // as a (hopefully) temporary hack, we render all panes until we find a better solution
            let mut vte_output = String::new();
            let buffer_lines = &self.read_buffer_as_lines();
            let display_cols = self.get_columns();
            let mut character_styles = CharacterStyles::new();
            if self.clear_viewport_before_rendering {
                for line_index in 0..self.grid.height {
                    let x = self.get_x();
                    let y = self.get_y();
                    vte_output = format!(
                        "{}\u{1b}[{};{}H\u{1b}[m",
                        vte_output,
                        y + line_index + 1,
                        x + 1
                    ); // goto row/col and reset styles
                    for _col_index in 0..self.grid.width {
                        vte_output.push(EMPTY_TERMINAL_CHARACTER.character);
                    }
                }
                self.clear_viewport_before_rendering = false;
            }
            for (row, line) in buffer_lines.iter().enumerate() {
                let x = self.get_x();
                let y = self.get_y();
                vte_output = format!("{}\u{1b}[{};{}H\u{1b}[m", vte_output, y + row + 1, x + 1); // goto row/col and reset styles
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
                            vte_output = format!("{}{}", vte_output, new_styles);
                        }
                        vte_output.push(t_character.character);
                    }
                }
                character_styles.clear();
            }
            self.should_render = false;
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
        self.mark_for_rerender();
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.columns -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn pull_up(&mut self, count: usize) {
        self.position_and_size.y = self.position_and_size.y.checked_sub(count).unwrap_or(0);
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn pull_left(&mut self, count: usize) {
        self.position_and_size.x = self.position_and_size.x.checked_sub(count).unwrap_or(0);
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.columns -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.columns += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.columns += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn scroll_up(&mut self, count: usize) {
        self.grid.move_viewport_up(count);
        self.mark_for_rerender();
    }
    fn scroll_down(&mut self, count: usize) {
        self.grid.move_viewport_down(count);
        self.mark_for_rerender();
    }
    fn clear_scroll(&mut self) {
        self.grid.reset_viewport();
        self.mark_for_rerender();
    }
    fn safe_reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows = self
            .position_and_size
            .rows
            .checked_sub(count)
            .filter(|&new_height| new_height > self.min_height())
            .unwrap_or(self.min_height());
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn safe_reduce_width_left(&mut self, count: usize) {
        self.position_and_size.columns = self
            .position_and_size
            .columns
            .checked_sub(count)
            .filter(|&new_width| new_width > self.min_width())
            .unwrap_or(self.min_width());
        self.reflow_lines();
        self.mark_for_rerender();
    }
}
impl TerminalPane {
    pub fn new(pid: RawFd, position_and_size: PositionAndSize) -> TerminalPane {
        let grid = Grid::new(position_and_size.rows, position_and_size.columns);
        let pending_styles = CharacterStyles::new();
        TerminalPane {
            pid,
            grid,
            alternative_grid: None,
            should_render: true,
            selectable: true,
            pending_styles,
            position_and_size,
            position_and_size_override: None,
            cursor_key_mode: false,
            clear_viewport_before_rendering: false,
            max_height: None,
        }
    }
    pub fn mark_for_rerender(&mut self) {
        self.should_render = true;
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
        if let Some(alternative_grid) = self.alternative_grid.as_mut() {
            alternative_grid.change_size(rows, columns);
        }
    }

    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.grid.as_character_lines()
    }
    #[cfg(test)]
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        self.grid.rotate_scroll_region_up(count);
        self.mark_for_rerender();
    }
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        self.grid.rotate_scroll_region_down(count);
        self.mark_for_rerender();
    }
    fn add_newline(&mut self) {
        let mut pad_character = EMPTY_TERMINAL_CHARACTER;
        pad_character.styles = self.pending_styles;
        self.grid.add_canonical_line(pad_character);
        self.mark_for_rerender();
    }
    fn move_to_beginning_of_line(&mut self) {
        self.grid.move_cursor_to_beginning_of_line();
    }
    fn move_cursor_backwards(&mut self, count: usize) {
        self.grid.move_cursor_backwards(count);
    }
    fn _reset_all_ansi_codes(&mut self) {
        self.pending_styles.clear();
    }
}

impl vte::Perform for TerminalPane {
    fn print(&mut self, c: char) {
        // apparently, building TerminalCharacter like this without a "new" method
        // is a little faster
        let terminal_character = TerminalCharacter {
            character: c,
            styles: self.pending_styles,
        };
        self.grid.add_character(terminal_character);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            8 => {
                // backspace
                self.move_cursor_backwards(1);
            }
            9 => {
                // tab
                self.grid.advance_to_next_tabstop(self.pending_styles);
            }
            10 => {
                // 0a, newline
                self.add_newline();
            }
            13 => {
                // 0d, carriage return
                self.move_to_beginning_of_line();
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &[i64], _intermediates: &[u8], _ignore: bool, _c: char) {
        // TBD
    }

    fn put(&mut self, _byte: u8) {
        // TBD
    }

    fn unhook(&mut self) {
        // TBD
    }

    fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {
        // TBD
    }

    fn csi_dispatch(&mut self, params: &[i64], _intermediates: &[u8], _ignore: bool, c: char) {
        if c == 'm' {
            self.pending_styles.add_style_from_ansi_params(params);
        } else if c == 'C' {
            // move cursor forward
            let move_by = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.grid.move_cursor_forward_until_edge(move_by);
        } else if c == 'K' {
            // clear line (0 => right, 1 => left, 2 => all)
            if params[0] == 0 {
                let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
                char_to_replace.styles = self.pending_styles;
                self.grid
                    .replace_characters_in_line_after_cursor(char_to_replace);
            } else if params[0] == 1 {
                let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
                char_to_replace.styles = self.pending_styles;
                self.grid
                    .replace_characters_in_line_before_cursor(char_to_replace);
            } else if params[0] == 2 {
                self.grid.clear_cursor_line();
            }
        } else if c == 'J' {
            // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
            let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
            char_to_replace.styles = self.pending_styles;
            if params[0] == 0 {
                self.grid.clear_all_after_cursor(char_to_replace);
            } else if params[0] == 2 {
                self.grid.clear_all(char_to_replace);
            }
        // TODO: implement 1
        } else if c == 'H' {
            // goto row/col
            // we subtract 1 from the row/column because these are 1 indexed
            // (except when they are 0, in which case they should be 1
            // don't look at me, I don't make the rules)
            let (row, col) = if params.len() == 1 {
                if params[0] == 0 {
                    (0, params[0] as usize)
                } else {
                    (params[0] as usize - 1, params[0] as usize)
                }
            } else if params[0] == 0 {
                (0, params[1] as usize - 1)
            } else {
                (params[0] as usize - 1, params[1] as usize - 1)
            };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid.move_cursor_to(col, row, pad_character);
        } else if c == 'A' {
            // move cursor up until edge of screen
            let move_up_count = if params[0] == 0 { 1 } else { params[0] };
            self.grid.move_cursor_up(move_up_count as usize);
        } else if c == 'B' {
            // move cursor down until edge of screen
            let move_down_count = if params[0] == 0 { 1 } else { params[0] };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid
                .move_cursor_down(move_down_count as usize, pad_character);
        } else if c == 'D' {
            let move_back_count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.grid.move_cursor_back(move_back_count);
        } else if c == 'l' {
            let first_intermediate_is_questionmark = match _intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                match params.get(0) {
                    Some(&1049) => {
                        if let Some(alternative_grid) = self.alternative_grid.as_mut() {
                            std::mem::swap(&mut self.grid, alternative_grid);
                        }
                        self.alternative_grid = None;
                        self.clear_viewport_before_rendering = true;
                        self.mark_for_rerender();
                    }
                    Some(&25) => {
                        self.grid.hide_cursor();
                        self.mark_for_rerender();
                    }
                    Some(&1) => {
                        self.cursor_key_mode = false;
                    }
                    _ => {}
                };
            }
        } else if c == 'h' {
            let first_intermediate_is_questionmark = match _intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                match params.get(0) {
                    Some(&25) => {
                        self.grid.show_cursor();
                        self.mark_for_rerender();
                    }
                    Some(&1049) => {
                        let columns = self
                            .position_and_size_override
                            .map(|x| x.columns)
                            .unwrap_or(self.position_and_size.columns);
                        let rows = self
                            .position_and_size_override
                            .map(|x| x.rows)
                            .unwrap_or(self.position_and_size.rows);
                        let current_grid =
                            std::mem::replace(&mut self.grid, Grid::new(rows, columns));
                        self.alternative_grid = Some(current_grid);
                        self.clear_viewport_before_rendering = true;
                    }
                    Some(&1) => {
                        self.cursor_key_mode = true;
                    }
                    _ => {}
                };
            }
        } else if c == 'r' {
            if params.len() > 1 {
                // minus 1 because these are 1 indexed
                let top_line_index = params[0] as usize - 1;
                let bottom_line_index = params[1] as usize - 1;
                self.grid
                    .set_scroll_region(top_line_index, bottom_line_index);
                self.grid.show_cursor();
            } else {
                self.grid.clear_scroll_region();
            }
        } else if c == 't' {
            // TBD - title?
        } else if c == 'n' {
            // TBD - device status report
        } else if c == 'c' {
            // TBD - identify terminal
        } else if c == 'M' {
            // delete lines if currently inside scroll region
            let line_count_to_delete = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid
                .delete_lines_in_scroll_region(line_count_to_delete, pad_character);
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            let line_count_to_add = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid
                .add_empty_lines_in_scroll_region(line_count_to_add, pad_character);
        } else if c == 'q' {
            // ignore for now to run on mac
        } else if c == 'G' {
            let column = if params[0] == 0 {
                0
            } else {
                params[0] as usize - 1
            };
            self.grid.move_cursor_to_column(column);
        } else if c == 'd' {
            // goto line
            let line = if params[0] == 0 {
                1
            } else {
                // minus 1 because this is 1 indexed
                params[0] as usize - 1
            };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid.move_cursor_to_line(line, pad_character);
        } else if c == 'P' {
            // erase characters
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.grid.erase_characters(count, self.pending_styles);
        } else if c == 'X' {
            // erase characters and replace with empty characters of current style
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.grid
                .replace_with_empty_chars(count, self.pending_styles);
        } else if c == 'T' {
            /*
             * 124  54  T   SD
             * Scroll down, new lines inserted at top of screen
             * [4T = Scroll down 4, bring previous lines back into view
             */
            let line_count: i64 = *params.get(0).expect("A number of lines was expected.");

            if line_count >= 0 {
                self.rotate_scroll_region_up(line_count as usize);
            } else {
                self.rotate_scroll_region_down(line_count.abs() as usize);
            }
        } else if c == 'S' {
            // move scroll up
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.pending_styles;
            self.grid
                .delete_lines_in_scroll_region(count, pad_character);
            // TODO: since delete_lines_in_scroll_region also adds lines, is the below redundant?
            self.grid
                .add_empty_lines_in_scroll_region(count, pad_character);
        } else {
            let _ = debug_log_to_file(format!("Unhandled csi: {}->{:?}", c, params));
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        if let (b'M', None) = (byte, intermediates.get(0)) {
            self.grid.move_cursor_up_with_scrolling(1);
        }
    }
}
