#![allow(clippy::clippy::if_same_then_else)]

use ::nix::pty::Winsize;
use ::std::os::unix::io::RawFd;
use ::vte::Perform;

use crate::boundaries::Rect;
use crate::terminal_pane::terminal_character::{
    AnsiCode, CharacterStyles, NamedColor, TerminalCharacter,
};
use crate::terminal_pane::Scroll;
use crate::utils::logging::{debug_log_to_file, debug_log_to_file_pid_0};
use crate::VteEvent;

#[derive(Clone, Copy, Debug)]
pub struct PositionAndSize {
    pub x: usize,
    pub y: usize,
    pub rows: usize,
    pub columns: usize,
}

impl PositionAndSize {
    pub fn from(winsize: Winsize) -> PositionAndSize {
        PositionAndSize {
            columns: winsize.ws_col as usize,
            rows: winsize.ws_row as usize,
            x: winsize.ws_xpixel as usize,
            y: winsize.ws_ypixel as usize,
        }
    }
}

#[derive(Debug)]
pub struct TerminalPane {
    pub pid: RawFd,
    pub scroll: Scroll,
    pub should_render: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pending_styles: CharacterStyles,
}

impl Rect for &mut TerminalPane {
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
}

impl TerminalPane {
    pub fn new(pid: RawFd, ws: PositionAndSize, x: usize, y: usize) -> TerminalPane {
        let scroll = Scroll::new(ws.columns, ws.rows);
        let pending_styles = CharacterStyles::new();
        let position_and_size = PositionAndSize {
            x,
            y,
            rows: ws.rows,
            columns: ws.columns,
        };
        TerminalPane {
            pid,
            scroll,
            should_render: true,
            pending_styles,
            position_and_size,
            position_and_size_override: None,
        }
    }
    pub fn mark_for_rerender(&mut self) {
        self.should_render = true;
    }
    pub fn handle_event(&mut self, event: VteEvent) {
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
    pub fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.columns -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.columns -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.columns += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.columns += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn reduce_height_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.position_and_size.rows -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn change_size_p(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size = *position_and_size;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    // TODO: merge these two methods
    pub fn change_size(&mut self, ws: &PositionAndSize) {
        self.position_and_size.columns = ws.columns;
        self.position_and_size.rows = ws.rows;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    pub fn is_directly_right_of(&self, other: &TerminalPane) -> bool {
        self.get_x() == other.get_x() + other.get_columns() + 1
    }
    pub fn is_directly_left_of(&self, other: &TerminalPane) -> bool {
        self.get_x() + self.get_columns() + 1 == other.get_x()
    }
    pub fn is_directly_below(&self, other: &TerminalPane) -> bool {
        self.get_y() == other.get_y() + other.get_rows() + 1
    }
    pub fn is_directly_above(&self, other: &TerminalPane) -> bool {
        self.get_y() + self.get_rows() + 1 == other.get_y()
    }
    pub fn horizontally_overlaps_with(&self, other: &TerminalPane) -> bool {
        (self.get_y() >= other.get_y() && self.get_y() <= (other.get_y() + other.get_rows()))
            || ((self.get_y() + self.get_rows()) <= (other.get_y() + other.get_rows())
                && (self.get_y() + self.get_rows()) > other.get_y())
            || (self.get_y() <= other.get_y()
                && (self.get_y() + self.get_rows() >= (other.get_y() + other.get_rows())))
            || (other.get_y() <= self.get_y()
                && (other.get_y() + other.get_rows() >= (self.get_y() + self.get_rows())))
    }
    pub fn get_horizontal_overlap_with(&self, other: &TerminalPane) -> usize {
        std::cmp::min(
            self.get_y() + self.get_rows(),
            other.get_y() + other.get_rows(),
        ) - std::cmp::max(self.get_y(), other.get_y())
    }
    pub fn vertically_overlaps_with(&self, other: &TerminalPane) -> bool {
        (self.get_x() >= other.get_x() && self.get_x() <= (other.get_x() + other.get_columns()))
            || ((self.get_x() + self.get_columns()) <= (other.get_x() + other.get_columns())
                && (self.get_x() + self.get_columns()) > other.get_x())
            || (self.get_x() <= other.get_x()
                && (self.get_x() + self.get_columns() >= (other.get_x() + other.get_columns())))
            || (other.get_x() <= self.get_x()
                && (other.get_x() + other.get_columns() >= (self.get_x() + self.get_columns())))
    }
    pub fn get_vertical_overlap_with(&self, other: &TerminalPane) -> usize {
        std::cmp::min(
            self.get_x() + self.get_columns(),
            other.get_x() + other.get_columns(),
        ) - std::cmp::max(self.get_x(), other.get_x())
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
        self.scroll.change_size(columns, rows);
    }
    pub fn buffer_as_vte_output(&mut self) -> Option<String> {
        // TODO: rename to render
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
            self.mark_for_rerender();
            Some(vte_output)
        } else {
            None
        }
    }
    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.scroll.as_character_lines()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.scroll.cursor_coordinates_on_screen()
    }
    pub fn scroll_up(&mut self, count: usize) {
        self.scroll.move_viewport_up(count);
        self.mark_for_rerender();
    }
    pub fn scroll_down(&mut self, count: usize) {
        self.scroll.move_viewport_down(count);
        self.mark_for_rerender();
    }
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        self.scroll.rotate_scroll_region_up(count);
        self.mark_for_rerender();
    }
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        self.scroll.rotate_scroll_region_down(count);
        self.mark_for_rerender();
    }
    pub fn clear_scroll(&mut self) {
        self.scroll.reset_viewport();
        self.mark_for_rerender();
    }
    pub fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize) {
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
    pub fn reset_size_and_position_override(&mut self) {
        self.position_and_size_override = None;
        self.reflow_lines();
        self.mark_for_rerender();
    }
    fn add_newline(&mut self) {
        self.scroll.add_canonical_line();
        // self.reset_all_ansi_codes(); // TODO: find out if we should be resetting here or not
        self.mark_for_rerender();
    }
    fn move_to_beginning_of_line(&mut self) {
        self.scroll.move_cursor_to_beginning_of_linewrap();
    }
    fn move_cursor_backwards(&mut self, count: usize) {
        self.scroll.move_cursor_backwards(count);
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
        self.scroll.add_character(terminal_character);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            8 => {
                // backspace
                self.move_cursor_backwards(1);
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
            if params.is_empty() || params[0] == 0 {
                // reset all
                self.pending_styles.reset_all();
                if let Some(param1) = params.get(1) {
                    // TODO: this is a case currently found in eg. htop where we get two different
                    // csi 'm' codes in one event.
                    // We should understand why these are happening and then make a more generic
                    // solution for them
                    if *param1 == 1 {
                        // bold
                        self.pending_styles = self
                            .pending_styles
                            .bold(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                }
            } else if params[0] == 39 {
                self.pending_styles = self.pending_styles.foreground(Some(AnsiCode::Reset));
                if let Some(param1) = params.get(1) {
                    // TODO: this is a case currently found in eg. htop where we get two different
                    // csi 'm' codes in one event.
                    // We should understand why these are happening and then make a more generic
                    // solution for them
                    if *param1 == 49 {
                        // TODO: if we need this to fix the bug, we need to make collecting the
                        // second argument in such cases generic
                        self.pending_styles = self.pending_styles.background(Some(AnsiCode::Reset));
                    }
                }
            } else if params[0] == 49 {
                self.pending_styles = self.pending_styles.background(Some(AnsiCode::Reset));
            } else if params[0] == 21 {
                // reset bold
                self.pending_styles = self.pending_styles.bold(Some(AnsiCode::Reset));
            } else if params[0] == 22 {
                // reset bold and dim
                self.pending_styles = self.pending_styles.bold(Some(AnsiCode::Reset));
                self.pending_styles = self.pending_styles.dim(Some(AnsiCode::Reset));
            } else if params[0] == 23 {
                // reset italic
                self.pending_styles = self.pending_styles.italic(Some(AnsiCode::Reset));
            } else if params[0] == 24 {
                // reset underline
                self.pending_styles = self.pending_styles.underline(Some(AnsiCode::Reset));
            } else if params[0] == 25 {
                // reset blink
                self.pending_styles = self.pending_styles.blink_slow(Some(AnsiCode::Reset));
                self.pending_styles = self.pending_styles.blink_fast(Some(AnsiCode::Reset));
            } else if params[0] == 27 {
                // reset reverse
                self.pending_styles = self.pending_styles.reverse(Some(AnsiCode::Reset));
            } else if params[0] == 28 {
                // reset hidden
                self.pending_styles = self.pending_styles.hidden(Some(AnsiCode::Reset));
            } else if params[0] == 29 {
                // reset strike
                self.pending_styles = self.pending_styles.strike(Some(AnsiCode::Reset));
            } else if params[0] == 38 {
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.foreground(Some(AnsiCode::Code(
                            (Some(*param1 as u16), Some(*param2 as u16)),
                        )));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .foreground(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .foreground(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 48 {
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.background(Some(AnsiCode::Code(
                            (Some(*param1 as u16), Some(*param2 as u16)),
                        )));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .background(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .background(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 1 {
                // bold
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.bold(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .bold(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles =
                            self.pending_styles.bold(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 2 {
                // dim
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.dim(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .dim(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles =
                            self.pending_styles.dim(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 3 {
                // italic
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.italic(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .italic(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .italic(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 4 {
                // underline
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.underline(Some(AnsiCode::Code(
                            (Some(*param1 as u16), Some(*param2 as u16)),
                        )));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .underline(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .underline(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 5 {
                // blink slow
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.blink_slow(Some(AnsiCode::Code(
                            (Some(*param1 as u16), Some(*param2 as u16)),
                        )));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .blink_slow(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .blink_slow(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 6 {
                // blink fast
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.blink_fast(Some(AnsiCode::Code(
                            (Some(*param1 as u16), Some(*param2 as u16)),
                        )));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .blink_fast(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .blink_fast(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 7 {
                // reverse
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.reverse(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .reverse(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .reverse(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 8 {
                // hidden
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.hidden(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .hidden(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .hidden(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 9 {
                // strike
                match (params.get(1), params.get(2)) {
                    (Some(param1), Some(param2)) => {
                        self.pending_styles = self.pending_styles.strike(Some(AnsiCode::Code((
                            Some(*param1 as u16),
                            Some(*param2 as u16),
                        ))));
                    }
                    (Some(param1), None) => {
                        self.pending_styles = self
                            .pending_styles
                            .strike(Some(AnsiCode::Code((Some(*param1 as u16), None))));
                    }
                    (_, _) => {
                        self.pending_styles = self
                            .pending_styles
                            .strike(Some(AnsiCode::Code((None, None))));
                    }
                };
            } else if params[0] == 30 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Black)));
            } else if params[0] == 31 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Red)));
            } else if params[0] == 32 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Green)));
            } else if params[0] == 33 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Yellow)));
            } else if params[0] == 34 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Blue)));
            } else if params[0] == 35 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Magenta)));
            } else if params[0] == 36 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::Cyan)));
            } else if params[0] == 37 {
                self.pending_styles = self
                    .pending_styles
                    .foreground(Some(AnsiCode::NamedColor(NamedColor::White)));
            } else if params[0] == 40 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Black)));
            } else if params[0] == 41 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Red)));
            } else if params[0] == 42 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Green)));
            } else if params[0] == 43 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Yellow)));
            } else if params[0] == 44 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Blue)));
            } else if params[0] == 45 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Magenta)));
            } else if params[0] == 46 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::Cyan)));
            } else if params[0] == 47 {
                self.pending_styles = self
                    .pending_styles
                    .background(Some(AnsiCode::NamedColor(NamedColor::White)));
            } else {
                debug_log_to_file_pid_0(format!("unhandled csi m code {:?}", params), self.pid)
                    .unwrap();
            }
        } else if c == 'C' {
            // move cursor forward
            let move_by = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.scroll.move_cursor_forward(move_by);
        } else if c == 'K' {
            // clear line (0 => right, 1 => left, 2 => all)
            if params[0] == 0 {
                self.scroll
                    .clear_canonical_line_right_of_cursor(self.pending_styles);
            } else if params[0] == 1 {
                self.scroll
                    .clear_canonical_line_left_of_cursor(self.pending_styles);
            }
        // TODO: implement 2
        } else if c == 'J' {
            // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
            if params[0] == 0 {
                self.scroll.clear_all_after_cursor();
            } else if params[0] == 2 {
                self.scroll.clear_all();
            }
        // TODO: implement 1
        } else if c == 'H' {
            // goto row/col
            let (row, col) = if params.len() == 1 {
                (params[0] as usize, params[0] as usize)
            } else {
                // we subtract 1 from the column because after we get a cursor goto, the print
                // character should be printed on top of the cursor
                (params[0] as usize, params[1] as usize - 1)
            };
            self.scroll.move_cursor_to(row, col);
        } else if c == 'A' {
            // move cursor up until edge of screen
            let move_up_count = if params[0] == 0 { 1 } else { params[0] };
            self.scroll.move_cursor_up(move_up_count as usize);
        } else if c == 'D' {
            let move_back_count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.scroll.move_cursor_back(move_back_count);
        } else if c == 'l' {
            let first_intermediate_is_questionmark = match _intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                if let Some(&25) = params.get(0) {
                    self.scroll.hide_cursor();
                    self.mark_for_rerender();
                };
            }
        } else if c == 'h' {
            let first_intermediate_is_questionmark = match _intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                if let Some(&25) = params.get(0) {
                    self.scroll.show_cursor();
                    self.mark_for_rerender();
                };
            }
        } else if c == 'r' {
            if params.len() > 1 {
                let top_line_index = params[0] as usize;
                let bottom_line_index = params[1] as usize;
                self.scroll
                    .set_scroll_region(top_line_index, bottom_line_index);
                self.scroll.show_cursor();
            } else {
                self.scroll.clear_scroll_region();
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
            self.scroll
                .delete_lines_in_scroll_region(line_count_to_delete);
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            let line_count_to_add = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.scroll
                .add_empty_lines_in_scroll_region(line_count_to_add);
        } else if c == 'q' {
            // ignore for now to run on mac
        } else if c == 'G' {
            let column = if params[0] == 0 {
                0
            } else {
                // params[0] as usize
                params[0] as usize - 1
            };
            self.scroll.move_cursor_to_column(column);
        } else if c == 'd' {
            // goto line
            let line = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.scroll.move_cursor_to_line(line);
        } else if c == 'X' || c == 'P' {
            // erase characters
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            self.scroll
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
            self.scroll.delete_lines_in_scroll_region(count);
            self.scroll.add_empty_lines_in_scroll_region(count);
        } else {
            let _ = debug_log_to_file(format!("Unhandled csi: {}->{:?}", c, params));
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (byte, intermediates.get(0)) {
            (b'M', None) => {
                self.scroll.move_cursor_up_in_scroll_region(1);
            }
            _ => {
                let _ = debug_log_to_file(format!(
                    "Unhandled esc_dispatch: {}->{:?}",
                    byte, intermediates
                ));
            }
        }
    }
}
