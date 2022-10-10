use crate::output::{CharacterChunk, SixelImageChunk};
use crate::panes::sixel::SixelImageStore;
use crate::panes::{
    grid::Grid,
    terminal_character::{TerminalCharacter, EMPTY_TERMINAL_CHARACTER},
};
use crate::panes::{AnsiCode, LinkHandler};
use crate::pty::VteBytes;
use crate::tab::{Pane, AdjustedInput};
use crate::ClientId;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::rc::Rc;
use std::time::{self, Instant};
use zellij_utils::pane_size::Offset;
use zellij_utils::input::command::RunCommand;
use zellij_utils::{
    data::{InputMode, Palette, PaletteColor, Style},
    pane_size::SizeInPixels,
    pane_size::{Dimension, PaneGeom},
    position::Position,
    shared::make_terminal_title,
    vte,
};

use crate::ui::pane_boundaries_frame::{FrameParams, PaneFrame};

pub const SELECTION_SCROLL_INTERVAL_MS: u64 = 10;

// Some keys in different formats but are used in the code
const LEFT_ARROW: &[u8] = &[27, 91, 68];
const RIGHT_ARROW: &[u8] = &[27, 91, 67];
const UP_ARROW: &[u8] = &[27, 91, 65];
const DOWN_ARROW: &[u8] = &[27, 91, 66];
const HOME_KEY: &[u8] = &[27, 91, 72];
const END_KEY: &[u8] = &[27, 91, 70];
const BRACKETED_PASTE_BEGIN: &[u8] = &[27, 91, 50, 48, 48, 126];
const BRACKETED_PASTE_END: &[u8] = &[27, 91, 50, 48, 49, 126];
const ENTER: &[u8] = &[13]; // TODO: check this to be sure it fits all types of ENTER
const CTRL_C: &[u8] = &[3]; // TODO: check this to be sure it fits all types of CTRL_C (with mac, etc)
const TERMINATING_STRING: &str = "\0";
const DELETE_KEY: &str = "\u{007F}";
const BACKSPACE_KEY: &str = "\u{0008}";

/// The ansi encoding of some keys
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum AnsiEncoding {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
}

impl AnsiEncoding {
    /// Returns the ANSI representation of the entries.
    /// NOTE: There is an ANSI escape code (27) at the beginning of the string,
    ///       some editors will not show this
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Left => "OD".as_bytes(),
            Self::Right => "OC".as_bytes(),
            Self::Up => "OA".as_bytes(),
            Self::Down => "OB".as_bytes(),
            Self::Home => &[27, 79, 72], // ESC O H
            Self::End => &[27, 79, 70],  // ESC O F
        }
    }

    pub fn as_vec_bytes(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Hash, Clone, Copy, Debug)]
pub enum PaneId {
    Terminal(u32),
    Plugin(u32), // FIXME: Drop the trait object, make this a wrapper for the struct?
}

// FIXME: This should hold an os_api handle so that terminal panes can set their own size via FD in
// their `reflow_lines()` method. Drop a Box<dyn ServerOsApi> in here somewhere.
#[allow(clippy::too_many_arguments)]
pub struct TerminalPane {
    pub grid: Grid,
    pub pid: u32,
    pub selectable: bool,
    pub geom: PaneGeom,
    pub geom_override: Option<PaneGeom>,
    pub active_at: Instant,
    pub style: Style,
    vte_parser: vte::Parser,
    selection_scrolled_at: time::Instant,
    content_offset: Offset,
    pane_title: String,
    pane_name: String,
    prev_pane_name: String,
    frame: HashMap<ClientId, PaneFrame>,
    borderless: bool,
    fake_cursor_locations: HashSet<(usize, usize)>, // (x, y) - these hold a record of previous fake cursors which we need to clear on render
    search_term: String,
    is_held: Option<(Option<i32>, RunCommand)>, // a "held" pane means that its command has exited and its waiting for a
                   // possible user instruction to be re-run
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
    fn cols(&self) -> usize {
        self.get_columns()
    }
    fn get_content_x(&self) -> usize {
        self.get_x() + self.content_offset.left
    }
    fn get_content_y(&self) -> usize {
        self.get_y() + self.content_offset.top
    }
    fn get_content_columns(&self) -> usize {
        // content columns might differ from the pane's columns if the pane has a frame
        // in that case they would be 2 less
        self.get_columns()
            .saturating_sub(self.content_offset.left + self.content_offset.right)
    }
    fn get_content_rows(&self) -> usize {
        // content rows might differ from the pane's rows if the pane has a frame
        // in that case they would be 2 less
        self.get_rows()
            .saturating_sub(self.content_offset.top + self.content_offset.bottom)
    }
    fn reset_size_and_position_override(&mut self) {
        self.geom_override = None;
        self.reflow_lines();
    }
    fn set_geom(&mut self, position_and_size: PaneGeom) {
        self.geom = position_and_size;
        self.reflow_lines();
    }
    fn set_geom_override(&mut self, pane_geom: PaneGeom) {
        self.geom_override = Some(pane_geom);
        self.reflow_lines();
    }
    fn handle_pty_bytes(&mut self, bytes: VteBytes) {
        self.set_should_render(true);
        for &byte in &bytes {
            self.vte_parser.advance(&mut self.grid, byte);
        }
    }
    fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        let Offset { top, left, .. } = self.content_offset;
        self.grid
            .cursor_coordinates()
            .map(|(x, y)| (x + left, y + top))
    }
    fn adjust_input_to_terminal(&mut self, input_bytes: Vec<u8>) -> Option<AdjustedInput> {
        // there are some cases in which the terminal state means that input sent to it
        // needs to be adjusted.
        // here we match against those cases - if need be, we adjust the input and if not
        // we send back the original input
        if let Some((_exit_status, run_command)) = &self.is_held {
            match input_bytes.as_slice() {
                ENTER => {
                    let run_command = run_command.clone();
                    self.is_held = None;
                    self.grid.reset_terminal_state();
                    self.set_should_render(true);
                    Some(AdjustedInput::ReRunCommandInThisPane(run_command))
                }
                CTRL_C => {
                    Some(AdjustedInput::CloseThisPane)
                }
                _ => None
            }
        } else {
            if self.grid.new_line_mode {
                if let &[13] = input_bytes.as_slice() {
                    // LNM - carriage return is followed by linefeed
                    return Some(AdjustedInput::WriteBytesToTerminal("\u{0d}\u{0a}".as_bytes().to_vec()));
                };
            }
            if self.grid.cursor_key_mode {
                match input_bytes.as_slice() {
                    LEFT_ARROW => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::Left.as_vec_bytes()));
                    },
                    RIGHT_ARROW => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::Right.as_vec_bytes()));
                    },
                    UP_ARROW => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::Up.as_vec_bytes()));
                    },
                    DOWN_ARROW => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::Down.as_vec_bytes()));
                    },

                    HOME_KEY => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::Home.as_vec_bytes()));
                    },
                    END_KEY => {
                        return Some(AdjustedInput::WriteBytesToTerminal(AnsiEncoding::End.as_vec_bytes()));
                    },
                    _ => {},
                };
            }

            if !self.grid.bracketed_paste_mode {
                // Zellij itself operates in bracketed paste mode, so the terminal sends these
                // instructions (bracketed paste start and bracketed paste end respectively)
                // when pasting input. We only need to make sure not to send them to terminal
                // panes who do not work in this mode
                match input_bytes.as_slice() {
                    BRACKETED_PASTE_BEGIN | BRACKETED_PASTE_END => return Some(AdjustedInput::WriteBytesToTerminal(vec![])),
                    _ => {},
                }
            }
            Some(AdjustedInput::WriteBytesToTerminal(input_bytes))
        }
    }
    fn position_and_size(&self) -> PaneGeom {
        self.geom
    }
    fn current_geom(&self) -> PaneGeom {
        self.geom_override.unwrap_or(self.geom)
    }
    fn geom_override(&self) -> Option<PaneGeom> {
        self.geom_override
    }
    fn should_render(&self) -> bool {
        self.grid.should_render
    }
    fn set_should_render(&mut self, should_render: bool) {
        self.grid.should_render = should_render;
    }
    fn render_full_viewport(&mut self) {
        // this marks the pane for a full re-render, rather than just rendering the
        // diff as it usually does with the OutputBuffer
        self.frame.clear();
        self.grid.render_full_viewport();
    }
    fn selectable(&self) -> bool {
        self.selectable
    }
    fn set_selectable(&mut self, selectable: bool) {
        self.selectable = selectable;
    }
    fn render(
        &mut self,
        _client_id: Option<ClientId>,
    ) -> Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)> {
        if self.should_render() {
            let mut raw_vte_output = String::new();
            let content_x = self.get_content_x();
            let content_y = self.get_content_y();

            let (mut character_chunks, sixel_image_chunks) =
                self.grid.read_changes(content_x, content_y);
            for character_chunk in character_chunks.iter_mut() {
                character_chunk.add_changed_colors(self.grid.changed_colors);
                if self
                    .grid
                    .selection
                    .contains_row(character_chunk.y.saturating_sub(content_y))
                {
                    let background_color = match self.style.colors.bg {
                        PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                        PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                    };
                    character_chunk.add_selection_and_colors(
                        self.grid.selection,
                        background_color,
                        None,
                        content_x,
                        content_y,
                    );
                } else if !self.grid.search_results.selections.is_empty() {
                    for res in self.grid.search_results.selections.iter() {
                        if res.contains_row(character_chunk.y.saturating_sub(content_y)) {
                            let (select_background_palette, select_foreground_palette) =
                                if Some(res) == self.grid.search_results.active.as_ref() {
                                    (self.style.colors.orange, self.style.colors.black)
                                } else {
                                    (self.style.colors.green, self.style.colors.black)
                                };
                            let background_color = match select_background_palette {
                                PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                                PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                            };
                            let foreground_color = match select_foreground_palette {
                                PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                                PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                            };
                            character_chunk.add_selection_and_colors(
                                *res,
                                background_color,
                                Some(foreground_color),
                                content_x,
                                content_y,
                            );
                        }
                    }
                }
            }
            if self.grid.ring_bell {
                let ring_bell = '\u{7}';
                raw_vte_output.push(ring_bell);
                self.grid.ring_bell = false;
            }
            self.set_should_render(false);
            Some((character_chunks, Some(raw_vte_output), sixel_image_chunks))
        } else {
            None
        }
    }
    fn render_frame(
        &mut self,
        client_id: ClientId,
        frame_params: FrameParams,
        input_mode: InputMode,
    ) -> Option<(Vec<CharacterChunk>, Option<String>)> {
        // TODO: remove the cursor stuff from here
        let pane_title = if let Some((_exit_status, run_command)) = &self.is_held {
            format!("{}", run_command)
        } else if self.pane_name.is_empty()
            && input_mode == InputMode::RenamePane
            && frame_params.is_main_client
        {
            String::from("Enter name...")
        } else if input_mode == InputMode::EnterSearch
            && frame_params.is_main_client
            && self.search_term.is_empty()
        {
            String::from("Enter search...")
        } else if (input_mode == InputMode::EnterSearch || input_mode == InputMode::Search)
            && !self.search_term.is_empty()
        {
            let mut modifier_text = String::new();
            if self.grid.search_results.has_modifiers_set() {
                let mut modifiers = Vec::new();
                modifier_text.push_str(" [");
                if self.grid.search_results.case_insensitive {
                    modifiers.push("c")
                }
                if self.grid.search_results.whole_word_only {
                    modifiers.push("o")
                }
                if self.grid.search_results.wrap_search {
                    modifiers.push("w")
                }
                modifier_text.push_str(&modifiers.join(", "));
                modifier_text.push(']');
            }
            format!("SEARCHING: {}{}", self.search_term, modifier_text)
        } else if self.pane_name.is_empty() {
            self.grid
                .title
                .clone()
                .unwrap_or_else(|| self.pane_title.clone())
        } else {
            self.pane_name.clone()
        };

        let mut frame = PaneFrame::new(
            self.current_geom().into(),
            self.grid.scrollback_position_and_length(),
            pane_title,
            frame_params,
        );
        if let Some((exit_status, _run_command)) = &self.is_held {
            frame.add_exit_status(exit_status.as_ref().copied());
        }

        match self.frame.get(&client_id) {
            // TODO: use and_then or something?
            Some(last_frame) => {
                if &frame != last_frame {
                    if !self.borderless {
                        let frame_output = frame.render();
                        self.frame.insert(client_id, frame);
                        Some(frame_output)
                    } else {
                        None
                    }
                } else {
                    None
                }
            },
            None => {
                if !self.borderless {
                    let frame_output = frame.render();
                    self.frame.insert(client_id, frame);
                    Some(frame_output)
                } else {
                    None
                }
            },
        }
    }
    fn render_fake_cursor(
        &mut self,
        cursor_color: PaletteColor,
        text_color: PaletteColor,
    ) -> Option<String> {
        let mut vte_output = None;
        if let Some((cursor_x, cursor_y)) = self.cursor_coordinates() {
            let mut character_under_cursor = self
                .grid
                .get_character_under_cursor()
                .unwrap_or(EMPTY_TERMINAL_CHARACTER);
            character_under_cursor.styles.background = Some(cursor_color.into());
            character_under_cursor.styles.foreground = Some(text_color.into());
            // we keep track of these so that we can clear them up later (see render function)
            self.fake_cursor_locations.insert((cursor_y, cursor_x));
            let mut fake_cursor = format!(
                "\u{1b}[{};{}H\u{1b}[m{}",           // goto row column and clear styles
                self.get_content_y() + cursor_y + 1, // + 1 because goto is 1 indexed
                self.get_content_x() + cursor_x + 1,
                &character_under_cursor.styles,
            );
            fake_cursor.push(character_under_cursor.character);
            vte_output = Some(fake_cursor);
        }
        vte_output
    }
    fn render_terminal_title(&mut self, input_mode: InputMode) -> String {
        let pane_title = if self.pane_name.is_empty() && input_mode == InputMode::RenamePane {
            "Enter name..."
        } else if self.pane_name.is_empty() {
            self.grid.title.as_deref().unwrap_or(&self.pane_title)
        } else {
            &self.pane_name
        };
        make_terminal_title(pane_title)
    }
    fn update_name(&mut self, name: &str) {
        match name {
            TERMINATING_STRING => {
                self.pane_name = String::new();
            },
            DELETE_KEY | BACKSPACE_KEY => {
                self.pane_name.pop();
            },
            c => {
                self.pane_name.push_str(c);
            },
        }
        self.set_should_render(true);
    }
    fn pid(&self) -> PaneId {
        PaneId::Terminal(self.pid)
    }
    fn reduce_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows = Dimension::percent(p - percent);
            self.set_should_render(true);
        }
    }
    fn increase_height(&mut self, percent: f64) {
        if let Some(p) = self.geom.rows.as_percent() {
            self.geom.rows = Dimension::percent(p + percent);
            self.set_should_render(true);
        }
    }
    fn reduce_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols = Dimension::percent(p - percent);
            self.set_should_render(true);
        }
    }
    fn increase_width(&mut self, percent: f64) {
        if let Some(p) = self.geom.cols.as_percent() {
            self.geom.cols = Dimension::percent(p + percent);
            self.set_should_render(true);
        }
    }
    fn push_down(&mut self, count: usize) {
        self.geom.y += count;
        self.reflow_lines();
    }
    fn push_right(&mut self, count: usize) {
        self.geom.x += count;
        self.reflow_lines();
    }
    fn pull_left(&mut self, count: usize) {
        self.geom.x -= count;
        self.reflow_lines();
    }
    fn pull_up(&mut self, count: usize) {
        self.geom.y -= count;
        self.reflow_lines();
    }
    fn dump_screen(&mut self, _client_id: ClientId) -> String {
        self.grid.dump_screen()
    }
    fn scroll_up(&mut self, count: usize, _client_id: ClientId) {
        self.grid.move_viewport_up(count);
        self.set_should_render(true);
    }
    fn scroll_down(&mut self, count: usize, _client_id: ClientId) {
        self.grid.move_viewport_down(count);
        self.set_should_render(true);
    }
    fn clear_scroll(&mut self) {
        self.grid.reset_viewport();
        self.set_should_render(true);
    }
    fn is_scrolled(&self) -> bool {
        self.grid.is_scrolled
    }

    fn active_at(&self) -> Instant {
        self.active_at
    }

    fn set_active_at(&mut self, time: Instant) {
        self.active_at = time;
    }
    fn cursor_shape_csi(&self) -> String {
        self.grid.cursor_shape().get_csi_str().to_string()
    }
    fn drain_messages_to_pty(&mut self) -> Vec<Vec<u8>> {
        self.grid.pending_messages_to_pty.drain(..).collect()
    }

    fn drain_clipboard_update(&mut self) -> Option<String> {
        self.grid.pending_clipboard_update.take()
    }

    fn start_selection(&mut self, start: &Position, _client_id: ClientId) {
        self.grid.start_selection(start);
        self.set_should_render(true);
    }

    fn update_selection(&mut self, to: &Position, _client_id: ClientId) {
        let should_scroll = self.selection_scrolled_at.elapsed()
            >= time::Duration::from_millis(SELECTION_SCROLL_INTERVAL_MS);
        let cursor_at_the_bottom = to.line.0 < 0 && should_scroll;
        let cursor_at_the_top = to.line.0 as usize >= self.grid.height && should_scroll;
        let cursor_in_the_middle = to.line.0 >= 0 && (to.line.0 as usize) < self.grid.height;

        // TODO: check how far up/down mouse is relative to pane, to increase scroll lines?
        if cursor_at_the_bottom {
            self.grid.scroll_up_one_line();
            self.selection_scrolled_at = time::Instant::now();
        } else if cursor_at_the_top {
            self.grid.scroll_down_one_line();
            self.selection_scrolled_at = time::Instant::now();
        } else if cursor_in_the_middle {
            self.grid.update_selection(to);
        }

        self.set_should_render(true);
    }

    fn end_selection(&mut self, end: &Position, _client_id: ClientId) {
        self.grid.end_selection(end);
        self.set_should_render(true);
    }

    fn reset_selection(&mut self) {
        self.grid.reset_selection();
    }

    fn get_selected_text(&self) -> Option<String> {
        self.grid.get_selected_text()
    }

    fn set_frame(&mut self, _frame: bool) {
        self.frame.clear();
    }

    fn set_content_offset(&mut self, offset: Offset) {
        self.content_offset = offset;
        self.reflow_lines();
    }

    fn store_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.prev_pane_name = self.pane_name.clone()
        }
    }
    fn load_pane_name(&mut self) {
        if self.pane_name != self.prev_pane_name {
            self.pane_name = self.prev_pane_name.clone()
        }
    }

    fn set_borderless(&mut self, borderless: bool) {
        self.borderless = borderless;
    }
    fn borderless(&self) -> bool {
        self.borderless
    }

    fn mouse_left_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_left_click_signal(position, is_held)
    }
    fn mouse_left_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_left_click_release_signal(position)
    }
    fn mouse_right_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_right_click_signal(position, is_held)
    }
    fn mouse_right_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_right_click_release_signal(position)
    }
    fn mouse_middle_click(&self, position: &Position, is_held: bool) -> Option<String> {
        self.grid.mouse_middle_click_signal(position, is_held)
    }
    fn mouse_middle_click_release(&self, position: &Position) -> Option<String> {
        self.grid.mouse_middle_click_release_signal(position)
    }
    fn mouse_scroll_up(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_up_signal(position)
    }
    fn mouse_scroll_down(&self, position: &Position) -> Option<String> {
        self.grid.mouse_scroll_down_signal(position)
    }
    fn get_line_number(&self) -> Option<usize> {
        // + 1 because the absolute position in the scrollback is 0 indexed and this should be 1 indexed
        Some(self.grid.absolute_position_in_scrollback() + 1)
    }

    fn update_search_term(&mut self, needle: &str) {
        match needle {
            TERMINATING_STRING => {
                self.search_term = String::new();
            },
            DELETE_KEY | BACKSPACE_KEY => {
                self.search_term.pop();
            },
            c => {
                self.search_term.push_str(c);
            },
        }
        self.grid.clear_search();
        if !self.search_term.is_empty() {
            self.grid.set_search_string(&self.search_term);
        }
        self.set_should_render(true);
    }
    fn search_down(&mut self) {
        if self.search_term.is_empty() {
            return; // No-op
        }
        self.grid.search_down();
        self.set_should_render(true);
    }
    fn search_up(&mut self) {
        if self.search_term.is_empty() {
            return; // No-op
        }
        self.grid.search_up();
        self.set_should_render(true);
    }
    fn toggle_search_case_sensitivity(&mut self) {
        self.grid.toggle_search_case_sensitivity();
        self.set_should_render(true);
    }
    fn toggle_search_whole_words(&mut self) {
        self.grid.toggle_search_whole_words();
        self.set_should_render(true);
    }
    fn toggle_search_wrap(&mut self) {
        self.grid.toggle_search_wrap();
    }
    fn clear_search(&mut self) {
        self.grid.clear_search();
        self.search_term.clear();
    }
    fn is_alternate_mode_active(&self) -> bool {
        self.grid.is_alternate_mode_active()
    }
    fn hold(&mut self, exit_status: Option<i32>, run_command: RunCommand) {
        self.is_held = Some((exit_status, run_command));
        self.set_should_render(true);
    }
}

impl TerminalPane {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        pid: u32,
        position_and_size: PaneGeom,
        style: Style,
        pane_index: usize,
        pane_name: String,
        link_handler: Rc<RefCell<LinkHandler>>,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
        initial_pane_title: Option<String>,
    ) -> TerminalPane {
        let initial_pane_title = initial_pane_title.unwrap_or_else(|| format!("Pane #{}", pane_index));
        let grid = Grid::new(
            position_and_size.rows.as_usize(),
            position_and_size.cols.as_usize(),
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            link_handler,
            character_cell_size,
            sixel_image_store,
        );
        TerminalPane {
            frame: HashMap::new(),
            content_offset: Offset::default(),
            pid,
            grid,
            selectable: true,
            geom: position_and_size,
            geom_override: None,
            vte_parser: vte::Parser::new(),
            active_at: Instant::now(),
            style,
            selection_scrolled_at: time::Instant::now(),
            pane_title: initial_pane_title,
            pane_name: pane_name.clone(),
            prev_pane_name: pane_name,
            borderless: false,
            fake_cursor_locations: HashSet::new(),
            search_term: String::new(),
            is_held: None,
        }
    }
    pub fn get_x(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.x,
            None => self.geom.x,
        }
    }
    pub fn get_y(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.y,
            None => self.geom.y,
        }
    }
    pub fn get_columns(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.cols.as_usize(),
            None => self.geom.cols.as_usize(),
        }
    }
    pub fn get_rows(&self) -> usize {
        match self.geom_override {
            Some(position_and_size_override) => position_and_size_override.rows.as_usize(),
            None => self.geom.rows.as_usize(),
        }
    }
    fn reflow_lines(&mut self) {
        let rows = self.get_content_rows();
        let cols = self.get_content_columns();
        self.grid.change_size(rows, cols);
        self.set_should_render(true);
    }
    pub fn read_buffer_as_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        self.grid.as_character_lines()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
}

#[cfg(test)]
#[path = "./unit/terminal_pane_tests.rs"]
mod grid_tests;

#[cfg(test)]
#[path = "./unit/search_in_pane_tests.rs"]
mod search_tests;
