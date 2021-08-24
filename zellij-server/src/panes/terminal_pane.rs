use zellij_utils::position::Position;
use zellij_utils::zellij_tile::prelude::PaletteColor;
use zellij_utils::{vte, zellij_tile};

use std::fmt::Debug;
use std::os::unix::io::RawFd;
use std::time::{self, Instant};
use zellij_tile::data::Palette;

use zellij_utils::pane_size::PositionAndSize;

use crate::panes::AnsiCode;
use crate::panes::{
    grid::Grid,
    terminal_character::{
        CharacterStyles, CursorShape, TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
    },
};
use crate::pty::VteBytes;
use crate::tab::Pane;

pub const SELECTION_SCROLL_INTERVAL_MS: u64 = 10;

use crate::ui::pane_boundaries_frame::PaneBoundariesFrame;

#[derive(PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Debug)]
pub enum PaneId {
    Terminal(RawFd),
    Plugin(u32), // FIXME: Drop the trait object, make this a wrapper for the struct?
}

pub enum PaneDecoration {
    BoundariesFrame(PaneBoundariesFrame),
    ContentOffset((usize, usize)), // (columns, rows)
}

pub struct TerminalPane {
    pub grid: Grid,
    pub pid: RawFd,
    pub selectable: bool,
    position_and_size: PositionAndSize,
    position_and_size_override: Option<PositionAndSize>,
    pub active_at: Instant,
    pub colors: Palette,
    vte_parser: vte::Parser,
    selection_scrolled_at: time::Instant,
    content_position_and_size: PositionAndSize,
    pane_title: String,
    pane_decoration: PaneDecoration,
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
    fn get_content_columns(&self) -> usize {
        self.get_content_columns()
    }
    fn get_content_rows(&self) -> usize {
        self.get_content_rows()
    }
    fn reset_size_and_position_override(&mut self) {
        self.position_and_size_override = None;
        self.redistribute_space();
    }
    fn change_pos_and_size(&mut self, position_and_size: &PositionAndSize) {
        self.position_and_size = *position_and_size;
        self.redistribute_space();
    }
    fn override_size_and_position(&mut self, x: usize, y: usize, size: &PositionAndSize) {
        self.position_and_size_override = Some(PositionAndSize {
            x,
            y,
            rows: size.rows,
            cols: size.cols,
            ..Default::default()
        });
        self.redistribute_space();
    }
    fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        for byte in bytes.iter() {
            self.vte_parser.advance(&mut self.grid, *byte);
        }
        self.set_should_render(true);
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        let (x_offset, y_offset) = match &self.pane_decoration {
            PaneDecoration::BoundariesFrame(boundries_frame) => {
                let (content_columns_offset, content_rows_offset) =
                    boundries_frame.content_offset();
                (content_columns_offset, content_rows_offset)
            }
            PaneDecoration::ContentOffset(_) => (0, 0),
        };
        self.grid
            .cursor_coordinates()
            .map(|(x, y)| (x + x_offset, y + y_offset))
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
            [27, 91, 50, 48, 48, 126] | [27, 91, 50, 48, 49, 126] => {
                if !self.grid.bracketed_paste_mode {
                    // Zellij itself operates in bracketed paste mode, so the terminal sends these
                    // instructions (bracketed paste start and bracketed paste end respectively)
                    // when pasting input. We only need to make sure not to send them to terminal
                    // panes who do not work in this mode
                    return vec![];
                }
            }
            _ => {}
        };
        input_bytes
    }
    fn position_and_size(&self) -> PositionAndSize {
        self.position_and_size
    }
    fn position_and_size_override(&self) -> Option<PositionAndSize> {
        self.position_and_size_override
    }
    fn should_render(&self) -> bool {
        self.grid.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
    }
    fn set_should_render_boundaries(&mut self, should_render: bool) {
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            boundaries_frame.set_should_render(should_render);
        }
    }
    fn render_full_viewport(&mut self) {
        // this marks the pane for a full re-render, rather than just rendering the
        // diff as it usually does with the OutputBuffer
        self.grid.render_full_viewport();
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn set_fixed_height(&mut self, fixed_height: usize) {
        self.position_and_size.rows = fixed_height;
        self.position_and_size.rows_fixed = true;
    }
    fn set_fixed_width(&mut self, fixed_width: usize) {
        self.position_and_size.cols = fixed_width;
        self.position_and_size.cols_fixed = true;
    }
    fn set_invisible_borders(&mut self, _invisible_borders: bool) {
        unimplemented!();
    }
    fn render(&mut self) -> Option<String> {
        if self.should_render() {
            let mut vte_output = String::new();
            let mut character_styles = CharacterStyles::new();
            if self.grid.clear_viewport_before_rendering {
                for line_index in 0..self.grid.height {
                    let x = self.get_content_x();
                    let y = self.get_content_y();
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
            let max_width = self.get_content_columns();
            for character_chunk in self.grid.read_changes() {
                let pane_x = self.get_content_x();
                let pane_y = self.get_content_y();
                let chunk_absolute_x = pane_x + character_chunk.x;
                let chunk_absolute_y = pane_y + character_chunk.y;
                let terminal_characters = character_chunk.terminal_characters;
                vte_output.push_str(&format!(
                    "\u{1b}[{};{}H\u{1b}[m",
                    chunk_absolute_y + 1,
                    chunk_absolute_x + 1
                )); // goto row/col and reset styles

                let mut chunk_width = character_chunk.x;
                for mut t_character in terminal_characters {
                    // adjust the background of currently selected characters
                    // doing it here is much easier than in grid
                    if self.grid.selection.contains(character_chunk.y, chunk_width) {
                        let color = match self.colors.bg {
                            PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                            PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                        };

                        t_character.styles = t_character.styles.background(Some(color));
                    }
                    chunk_width += t_character.width;
                    if chunk_width > max_width {
                        break;
                    }

                    if let Some(new_styles) = character_styles
                        .update_and_return_diff(&t_character.styles, self.grid.changed_colors)
                    {
                        vte_output.push_str(&new_styles.to_string());
                    }
                    vte_output.push(t_character.character);
                }
                character_styles.clear();
            }
            if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
                boundaries_frame.update_scroll(self.grid.scrollback_position_and_length());
                boundaries_frame.update_title(self.grid.title.as_ref());
                if let Some(boundaries_frame_vte) = boundaries_frame.render() {
                    vte_output.push_str(&boundaries_frame_vte);
                }
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
        self.redistribute_space();
    }
    fn increase_height_down(&mut self, count: usize) {
        self.position_and_size.rows += count;
        self.redistribute_space();
    }
    fn increase_height_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.position_and_size.rows += count;
        self.redistribute_space();
    }
    fn reduce_height_up(&mut self, count: usize) {
        self.position_and_size.rows -= count;
        self.redistribute_space();
    }
    fn reduce_width_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.position_and_size.cols -= count;
        self.redistribute_space();
    }
    fn reduce_width_left(&mut self, count: usize) {
        self.position_and_size.cols -= count;
        self.redistribute_space();
    }
    fn increase_width_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.position_and_size.cols += count;
        self.redistribute_space();
    }
    fn increase_width_right(&mut self, count: usize) {
        self.position_and_size.cols += count;
        self.redistribute_space();
    }
    fn push_down(&mut self, count: usize) {
        self.position_and_size.y += count;
        self.redistribute_space();
    }
    fn push_right(&mut self, count: usize) {
        self.position_and_size.x += count;
        self.redistribute_space();
    }
    fn pull_left(&mut self, count: usize) {
        self.position_and_size.x -= count;
        self.redistribute_space();
    }
    fn pull_up(&mut self, count: usize) {
        self.position_and_size.y -= count;
        self.redistribute_space();
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
    fn cursor_shape_csi(&self) -> String {
        match self.grid.cursor_shape() {
            CursorShape::Initial => "\u{1b}[0 q".to_string(),
            CursorShape::Block => "\u{1b}[2 q".to_string(),
            CursorShape::BlinkingBlock => "\u{1b}[1 q".to_string(),
            CursorShape::Underline => "\u{1b}[4 q".to_string(),
            CursorShape::BlinkingUnderline => "\u{1b}[3 q".to_string(),
            CursorShape::Beam => "\u{1b}[6 q".to_string(),
            CursorShape::BlinkingBeam => "\u{1b}[5 q".to_string(),
        }
    }
    fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        self.grid.pending_messages_to_pty.drain(..).collect()
    }

    fn start_selection(&mut self, start: &Position) {
        self.grid.start_selection(start);
        self.set_should_render(true);
    }

    fn update_selection(&mut self, to: &Position) {
        let should_scroll = self.selection_scrolled_at.elapsed()
            >= time::Duration::from_millis(SELECTION_SCROLL_INTERVAL_MS);
        // TODO: check how far up/down mouse is relative to pane, to increase scroll lines?
        if to.line.0 < 0 && should_scroll {
            self.grid.scroll_up_one_line();
            self.selection_scrolled_at = time::Instant::now();
        } else if to.line.0 as usize >= self.grid.height && should_scroll {
            self.grid.scroll_down_one_line();
            self.selection_scrolled_at = time::Instant::now();
        } else if to.line.0 >= 0 && (to.line.0 as usize) < self.grid.height {
            self.grid.update_selection(to);
        }

        self.set_should_render(true);
    }

    fn end_selection(&mut self, end: Option<&Position>) {
        self.grid.end_selection(end);
        self.set_should_render(true);
    }

    fn reset_selection(&mut self) {
        self.grid.reset_selection();
    }

    fn get_selected_text(&self) -> Option<String> {
        self.grid.get_selected_text()
    }

    fn set_boundary_color(&mut self, color: Option<PaletteColor>) {
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            if boundaries_frame.color != color {
                boundaries_frame.set_color(color);
                self.set_should_render(true);
            }
        }
    }
    fn relative_position(&self, position_on_screen: &Position) -> Position {
        let pane_position_and_size = self.get_content_posision_and_size();
        position_on_screen.relative_to(&pane_position_and_size)
    }
    fn offset_content_columns(&mut self, by: usize) {
        if let PaneDecoration::ContentOffset(content_offset) = &mut self.pane_decoration {
            content_offset.0 = by;
        } else {
            self.pane_decoration = PaneDecoration::ContentOffset((by, 0));
        }
        self.redistribute_space();
    }
    fn offset_content_rows(&mut self, by: usize) {
        if let PaneDecoration::ContentOffset(content_offset) = &mut self.pane_decoration {
            content_offset.1 = by;
        } else {
            self.pane_decoration = PaneDecoration::ContentOffset((0, by));
        }
        self.redistribute_space();
    }
    fn show_boundaries_frame(&mut self, only_title: bool) {
        let position_and_size = self
            .position_and_size_override
            .unwrap_or(self.position_and_size);
        if let PaneDecoration::BoundariesFrame(boundaries_frame) = &mut self.pane_decoration {
            boundaries_frame.render_only_title(only_title);
            self.content_position_and_size = boundaries_frame.content_position_and_size();
        } else {
            let mut boundaries_frame =
                PaneBoundariesFrame::new(position_and_size, self.pane_title.clone());
            boundaries_frame.render_only_title(only_title);
            self.content_position_and_size = boundaries_frame.content_position_and_size();
            self.pane_decoration = PaneDecoration::BoundariesFrame(boundaries_frame);
        }
        self.redistribute_space();
    }
    fn remove_boundaries_frame(&mut self) {
        self.pane_decoration = PaneDecoration::ContentOffset((0, 0));
        self.redistribute_space();
    }
}

impl TerminalPane {
    pub fn new(
        pid: RawFd,
        position_and_size: PositionAndSize,
        palette: Palette,
        pane_position: usize,
    ) -> TerminalPane {
        let initial_pane_title = format!("Pane #{}", pane_position);
        let grid = Grid::new(position_and_size.rows, position_and_size.cols, palette);
        TerminalPane {
            pane_decoration: PaneDecoration::ContentOffset((0, 0)),
            content_position_and_size: position_and_size,
            pid,
            grid,
            selectable: true,
            position_and_size,
            position_and_size_override: None,
            vte_parser: vte::Parser::new(),
            active_at: Instant::now(),
            colors: palette,
            selection_scrolled_at: time::Instant::now(),
            pane_title: initial_pane_title,
        }
    }
    pub fn get_x(&self) -> usize {
        match self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.x,
            None => self.position_and_size.x,
        }
    }
    pub fn get_y(&self) -> usize {
        match self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.y,
            None => self.position_and_size.y,
        }
    }
    pub fn get_columns(&self) -> usize {
        match self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.cols,
            None => self.position_and_size.cols,
        }
    }
    pub fn get_rows(&self) -> usize {
        match self.position_and_size_override.as_ref() {
            Some(position_and_size_override) => position_and_size_override.rows,
            None => self.position_and_size.rows,
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
    fn reflow_lines(&mut self) {
        let rows = self.get_content_rows();
        let columns = self.get_content_columns();
        self.grid.change_size(rows, columns);
        self.set_should_render(true);
    }
    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.grid.as_character_lines()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
    fn redistribute_space(&mut self) {
        let position_and_size = self
            .position_and_size_override
            .unwrap_or_else(|| self.position_and_size());
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
        self.reflow_lines();
    }
}

#[cfg(test)]
#[path = "./unit/terminal_pane_tests.rs"]
mod grid_tests;
