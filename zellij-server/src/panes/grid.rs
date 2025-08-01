use super::sixel::{PixelRect, SixelGrid, SixelImageStore};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use zellij_utils::data::Style;
use zellij_utils::errors::prelude::*;

use std::{
    cmp::Ordering,
    collections::{BTreeSet, VecDeque},
    fmt::{self, Debug, Formatter},
    str,
};

use vte;
use zellij_utils::{
    consts::{DEFAULT_SCROLL_BUFFER_SIZE, SCROLL_BUFFER_SIZE},
    data::{Palette, PaletteColor, Styling},
    input::mouse::{MouseEvent, MouseEventType},
    pane_size::SizeInPixels,
    position::Position,
};

const TABSTOP_WIDTH: usize = 8; // TODO: is this always right?
pub const MAX_TITLE_STACK_SIZE: usize = 1000;

use vte::{Params, Perform};
use zellij_utils::{consts::VERSION, shared::version_number};

use crate::output::{CharacterChunk, OutputBuffer, SixelImageChunk};
use crate::panes::alacritty_functions::{parse_number, xparse_color};
use crate::panes::hyperlink_tracker::HyperlinkTracker;
use crate::panes::link_handler::LinkHandler;
use crate::panes::search::SearchResult;
use crate::panes::selection::Selection;
use crate::panes::terminal_character::{
    AnsiCode, CharsetIndex, Cursor, CursorShape, RcCharacterStyles, StandardCharset,
    TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
};
use crate::ui::components::UiComponentParser;

fn get_top_non_canonical_rows(rows: &mut Vec<Row>) -> Vec<Row> {
    let mut index_of_last_non_canonical_row = None;
    for (i, row) in rows.iter().enumerate() {
        if row.is_canonical {
            break;
        } else {
            index_of_last_non_canonical_row = Some(i);
        }
    }
    match index_of_last_non_canonical_row {
        Some(index_of_last_non_canonical_row) => {
            rows.drain(..=index_of_last_non_canonical_row).collect()
        },
        None => vec![],
    }
}

fn get_lines_above_bottom_canonical_row_and_wraps(rows: &mut VecDeque<Row>) -> Vec<Row> {
    let mut index_of_last_non_canonical_row = None;
    for (i, row) in rows.iter().enumerate().rev() {
        index_of_last_non_canonical_row = Some(i);
        if row.is_canonical {
            break;
        }
    }
    match index_of_last_non_canonical_row {
        Some(index_of_last_non_canonical_row) => {
            rows.drain(index_of_last_non_canonical_row..).collect()
        },
        None => vec![],
    }
}

fn get_viewport_bottom_canonical_row_and_wraps(viewport: &mut Vec<Row>) -> Vec<Row> {
    let mut index_of_last_non_canonical_row = None;
    for (i, row) in viewport.iter().enumerate().rev() {
        index_of_last_non_canonical_row = Some(i);
        if row.is_canonical {
            break;
        }
    }
    match index_of_last_non_canonical_row {
        Some(index_of_last_non_canonical_row) => {
            viewport.drain(index_of_last_non_canonical_row..).collect()
        },
        None => vec![],
    }
}

fn get_top_canonical_row_and_wraps(rows: &mut Vec<Row>) -> Vec<Row> {
    let mut index_of_first_non_canonical_row = None;
    let mut end_index_of_first_canonical_line = None;
    for (i, row) in rows.iter().enumerate() {
        if row.is_canonical && end_index_of_first_canonical_line.is_none() {
            index_of_first_non_canonical_row = Some(i);
            end_index_of_first_canonical_line = Some(i);
            continue;
        }
        if row.is_canonical && end_index_of_first_canonical_line.is_some() {
            break;
        }
        if index_of_first_non_canonical_row.is_some() {
            end_index_of_first_canonical_line = Some(i);
            continue;
        }
    }
    match (
        index_of_first_non_canonical_row,
        end_index_of_first_canonical_line,
    ) {
        (Some(first_index), Some(last_index)) => rows.drain(first_index..=last_index).collect(),
        (Some(first_index), None) => rows.drain(first_index..).collect(),
        _ => vec![],
    }
}

fn transfer_rows_from_lines_above_to_viewport(
    lines_above: &mut VecDeque<Row>,
    viewport: &mut Vec<Row>,
    sixel_grid: &mut SixelGrid,
    count: usize,
    max_viewport_width: usize,
) -> usize {
    let mut next_lines: Vec<Row> = vec![];
    let mut lines_added_to_viewport: isize = 0;
    loop {
        if lines_added_to_viewport as usize == count {
            break;
        }
        if next_lines.is_empty() {
            match lines_above.pop_back() {
                Some(next_line) => {
                    let mut top_non_canonical_rows_in_dst = get_top_non_canonical_rows(viewport);
                    lines_added_to_viewport -= top_non_canonical_rows_in_dst.len() as isize;
                    next_lines.push(next_line);
                    next_lines.append(&mut top_non_canonical_rows_in_dst);
                    next_lines =
                        Row::from_rows(next_lines).split_to_rows_of_length(max_viewport_width);
                    if next_lines.is_empty() {
                        // no more lines at lines_above, the line we popped was probably empty
                        break;
                    }
                },
                None => break, // no more rows
            }
        }
        viewport.insert(0, next_lines.pop().unwrap());
        lines_added_to_viewport += 1;
    }
    if !next_lines.is_empty() {
        let excess_row = Row::from_rows(next_lines);
        bounded_push(lines_above, sixel_grid, excess_row);
    }
    match usize::try_from(lines_added_to_viewport) {
        Ok(n) => n,
        _ => 0,
    }
}

fn transfer_rows_from_viewport_to_lines_above(
    viewport: &mut Vec<Row>,
    lines_above: &mut VecDeque<Row>,
    sixel_grid: &mut SixelGrid,
    count: usize,
    max_viewport_width: usize,
) -> isize {
    let mut transferred_rows_count: isize = 0;
    let drained_lines = std::cmp::min(count, viewport.len());
    for next_line in viewport.drain(..drained_lines) {
        let mut next_lines: Vec<Row> = vec![];
        transferred_rows_count +=
            calculate_row_display_height(next_line.width(), max_viewport_width) as isize;
        if !next_line.is_canonical {
            let mut bottom_canonical_row_and_wraps_in_dst =
                get_lines_above_bottom_canonical_row_and_wraps(lines_above);
            next_lines.append(&mut bottom_canonical_row_and_wraps_in_dst);
        }
        next_lines.push(next_line);
        let dropped_line_width = bounded_push(lines_above, sixel_grid, Row::from_rows(next_lines));
        if let Some(width) = dropped_line_width {
            transferred_rows_count -=
                calculate_row_display_height(width, max_viewport_width) as isize;
        }
    }
    transferred_rows_count
}

fn transfer_rows_from_lines_below_to_viewport(
    lines_below: &mut Vec<Row>,
    viewport: &mut Vec<Row>,
    count: usize,
    max_viewport_width: usize,
) {
    let mut next_lines: Vec<Row> = vec![];
    for _ in 0..count {
        let mut lines_pulled_from_viewport = 0;
        if next_lines.is_empty() {
            if !lines_below.is_empty() {
                let mut top_non_canonical_rows_in_lines_below =
                    get_top_non_canonical_rows(lines_below);
                if !top_non_canonical_rows_in_lines_below.is_empty() {
                    let mut canonical_line = get_viewport_bottom_canonical_row_and_wraps(viewport);
                    lines_pulled_from_viewport += canonical_line.len();
                    canonical_line.append(&mut top_non_canonical_rows_in_lines_below);
                    next_lines =
                        Row::from_rows(canonical_line).split_to_rows_of_length(max_viewport_width);
                } else {
                    let canonical_row = get_top_canonical_row_and_wraps(lines_below);
                    next_lines =
                        Row::from_rows(canonical_row).split_to_rows_of_length(max_viewport_width);
                }
            } else {
                break; // no more rows
            }
        }
        for _ in 0..(lines_pulled_from_viewport + 1) {
            if !next_lines.is_empty() {
                viewport.push(next_lines.remove(0));
            }
        }
    }
    if !next_lines.is_empty() {
        let excess_row = Row::from_rows(next_lines);
        lines_below.insert(0, excess_row);
    }
}

fn bounded_push(vec: &mut VecDeque<Row>, sixel_grid: &mut SixelGrid, value: Row) -> Option<usize> {
    let mut dropped_line_width = None;
    if vec.len() >= *SCROLL_BUFFER_SIZE.get().unwrap() {
        let line = vec.pop_front();
        if let Some(line) = line {
            sixel_grid.offset_grid_top();
            dropped_line_width = Some(line.width());
        }
    }
    vec.push_back(value);
    dropped_line_width
}

pub fn create_horizontal_tabstops(columns: usize) -> BTreeSet<usize> {
    let mut i = TABSTOP_WIDTH;
    let mut horizontal_tabstops = BTreeSet::new();
    loop {
        if i > columns {
            break;
        }
        horizontal_tabstops.insert(i);
        i += TABSTOP_WIDTH;
    }
    horizontal_tabstops
}

fn calculate_row_display_height(row_width: usize, viewport_width: usize) -> usize {
    if row_width <= viewport_width {
        return 1;
    }
    (row_width as f64 / viewport_width as f64).ceil() as usize
}

fn subtract_isize_from_usize(u: usize, i: isize) -> usize {
    if i.is_negative() {
        u - i.abs() as usize
    } else {
        u + i as usize
    }
}

macro_rules! dump_screen {
    ($lines:expr) => {{
        let mut is_first = true;
        let mut buf = String::with_capacity($lines.iter().map(|l| l.len()).sum());

        for line in &$lines {
            if line.is_canonical && !is_first {
                buf.push_str("\n");
            }
            let s: String = (&line.columns).into_iter().map(|x| x.character).collect();
            // Replace the spaces at the end of the line. Sometimes, the lines are
            // collected with spaces until the end of the panel.
            buf.push_str(&s.trim_end_matches(' '));
            is_first = false;
        }
        buf
    }};
}

fn utf8_mouse_coordinates(column: usize, line: isize) -> Vec<u8> {
    let mut coordinates = vec![];
    let mouse_pos_encode = |pos: usize| -> Vec<u8> {
        let pos = 32 + pos;
        let first = 0xC0 + pos / 64;
        let second = 0x80 + (pos & 63);
        vec![first as u8, second as u8]
    };

    if column > 95 {
        coordinates.append(&mut mouse_pos_encode(column));
    } else {
        coordinates.push(32 + column as u8);
    }
    if line > 95 {
        coordinates.append(&mut mouse_pos_encode(line as usize));
    } else {
        coordinates.push(32 + line as u8);
    }
    coordinates
}

#[derive(Clone)]
pub struct Grid {
    pub(crate) lines_above: VecDeque<Row>,
    pub(crate) viewport: Vec<Row>,
    pub(crate) lines_below: Vec<Row>,
    horizontal_tabstops: BTreeSet<usize>,
    alternate_screen_state: Option<AlternateScreenState>,
    cursor: Cursor,
    cursor_is_hidden: bool,
    saved_cursor_position: Option<Cursor>,
    scroll_region: (usize, usize),
    active_charset: CharsetIndex,
    preceding_char: Option<TerminalCharacter>,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    pub(crate) output_buffer: OutputBuffer,
    title_stack: Vec<String>,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    sixel_grid: SixelGrid,
    pub changed_colors: Option<[Option<AnsiCode>; 256]>,
    pub should_render: bool,
    pub lock_renders: bool,
    pub cursor_key_mode: bool, // DECCKM - when set, cursor keys should send ANSI direction codes (eg. "OD") instead of the arrow keys (eg. "[D")
    pub bracketed_paste_mode: bool, // when set, paste instructions to the terminal should be escaped with a special sequence
    pub erasure_mode: bool,         // ERM
    pub sixel_scrolling: bool,      // DECSDM
    pub insert_mode: bool,
    pub disable_linewrap: bool,
    pub new_line_mode: bool, // Automatic newline LNM
    pub clear_viewport_before_rendering: bool,
    pub width: usize,
    pub height: usize,
    pub pending_messages_to_pty: Vec<Vec<u8>>,
    pub selection: Selection,
    pub title: Option<String>,
    pub is_scrolled: bool,
    pub link_handler: Rc<RefCell<LinkHandler>>,
    pub ring_bell: bool,
    scrollback_buffer_lines: usize,
    pub mouse_mode: MouseMode,
    pub mouse_tracking: MouseTracking,
    pub focus_event_tracking: bool,
    pub search_results: SearchResult,
    pub pending_clipboard_update: Option<String>,
    ui_component_bytes: Option<Vec<u8>>,
    style: Style,
    debug: bool,
    arrow_fonts: bool,
    styled_underlines: bool,
    pub supports_kitty_keyboard_protocol: bool, // has the app requested kitty keyboard support?
    explicitly_disable_kitty_keyboard_protocol: bool, // has kitty keyboard support been explicitly
    // disabled by user config?
    click: Click,
    hyperlink_tracker: HyperlinkTracker,
}

const CLICK_TIME_THRESHOLD: u128 = 400; // Doherty Threshold

#[derive(Clone, Debug, Default)]
struct Click {
    position_and_time: Option<(Position, std::time::Instant)>,
    count: usize,
}

impl Click {
    pub fn record_click(&mut self, position: Position) {
        let click_is_same_position_as_last_click = self
            .position_and_time
            .map(|(p, _t)| p == position)
            .unwrap_or(false);
        let click_is_within_time_threshold = self
            .position_and_time
            .map(|(_p, t)| t.elapsed().as_millis() <= CLICK_TIME_THRESHOLD)
            .unwrap_or(false);
        if click_is_same_position_as_last_click && click_is_within_time_threshold {
            self.count += 1;
        } else {
            self.count = 1;
        }
        self.position_and_time = Some((position, std::time::Instant::now()));
        if self.count == 4 {
            self.reset();
        }
    }
    pub fn is_double_click(&self) -> bool {
        self.count == 2
    }
    pub fn is_triple_click(&self) -> bool {
        self.count == 3
    }
    pub fn reset(&mut self) {
        self.count = 0;
    }
}

#[derive(Clone, Debug)]
pub enum MouseMode {
    NoEncoding,
    Utf8,
    Sgr,
}

impl Default for MouseMode {
    fn default() -> Self {
        MouseMode::NoEncoding
    }
}

#[derive(Clone, Debug)]
pub enum MouseTracking {
    Off,
    Normal,
    ButtonEventTracking,
    AnyEventTracking,
}

impl Default for MouseTracking {
    fn default() -> Self {
        MouseTracking::Off
    }
}

impl Debug for Grid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut buffer: Vec<Row> = self.viewport.clone();
        // pad buffer
        for _ in buffer.len()..self.height {
            buffer.push(Row::new().canonical());
        }

        // display sixel placeholder
        let sixel_indication_character = |x| {
            let sixel_indication_word = "Sixel";
            sixel_indication_word
                .chars()
                .nth(x % sixel_indication_word.len())
                .unwrap()
        };
        for image_coordinates in self
            .sixel_grid
            .image_cell_coordinates_in_viewport(self.height, self.lines_above.len())
        {
            let (image_top_edge, image_bottom_edge, image_left_edge, image_right_edge) =
                image_coordinates;
            for y in image_top_edge..image_bottom_edge {
                let row = buffer.get_mut(y).unwrap();
                for x in image_left_edge..image_right_edge {
                    let fake_sixel_terminal_character =
                        TerminalCharacter::new_singlewidth(sixel_indication_character(x));
                    row.add_character_at(fake_sixel_terminal_character, x);
                }
            }
        }

        // display terminal characters with stripped styles
        for (i, row) in buffer.iter().enumerate() {
            let mut cow_row = Cow::Borrowed(row);
            self.search_results
                .mark_search_results_in_row(&mut cow_row, i);
            if row.is_canonical {
                writeln!(f, "{:02?} (C): {:?}", i, cow_row)?;
            } else {
                writeln!(f, "{:02?} (W): {:?}", i, cow_row)?;
            }
        }
        Ok(())
    }
}

impl Grid {
    pub fn new(
        rows: usize,
        columns: usize,
        terminal_emulator_colors: Rc<RefCell<Palette>>,
        terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
        link_handler: Rc<RefCell<LinkHandler>>,
        character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
        sixel_image_store: Rc<RefCell<SixelImageStore>>,
        style: Style, // TODO: consolidate this with terminal_emulator_colors
        debug: bool,
        arrow_fonts: bool,
        styled_underlines: bool,
        explicitly_disable_kitty_keyboard_protocol: bool,
    ) -> Self {
        let sixel_grid = SixelGrid::new(character_cell_size.clone(), sixel_image_store);
        // make sure this is initialized as it is used internally
        // if it was already initialized (which should happen normally unless this is a test or
        // something changed since this comment was written), we get an Error which we ignore
        // I don't know why this needs to be a OneCell, but whatevs
        let _ = SCROLL_BUFFER_SIZE.set(DEFAULT_SCROLL_BUFFER_SIZE);
        Grid {
            lines_above: VecDeque::new(),
            viewport: vec![Row::new().canonical()],
            lines_below: vec![],
            horizontal_tabstops: create_horizontal_tabstops(columns),
            cursor: Cursor::new(0, 0, styled_underlines),
            cursor_is_hidden: false,
            saved_cursor_position: None,
            scroll_region: (0, rows.saturating_sub(1)),
            preceding_char: None,
            width: columns,
            height: rows,
            should_render: true,
            cursor_key_mode: false,
            bracketed_paste_mode: false,
            erasure_mode: false,
            sixel_scrolling: false,
            insert_mode: false,
            disable_linewrap: false,
            new_line_mode: false,
            alternate_screen_state: None,
            clear_viewport_before_rendering: false,
            active_charset: Default::default(),
            pending_messages_to_pty: vec![],
            terminal_emulator_colors,
            terminal_emulator_color_codes,
            output_buffer: Default::default(),
            selection: Default::default(),
            title_stack: vec![],
            title: None,
            changed_colors: None,
            is_scrolled: false,
            link_handler,
            ring_bell: false,
            scrollback_buffer_lines: 0,
            mouse_mode: MouseMode::default(),
            mouse_tracking: MouseTracking::default(),
            focus_event_tracking: false,
            character_cell_size,
            search_results: Default::default(),
            sixel_grid,
            pending_clipboard_update: None,
            ui_component_bytes: None,
            style,
            debug,
            arrow_fonts,
            styled_underlines,
            lock_renders: false,
            supports_kitty_keyboard_protocol: false,
            explicitly_disable_kitty_keyboard_protocol,
            click: Click::default(),
            hyperlink_tracker: HyperlinkTracker::new(),
        }
    }
    pub fn render_full_viewport(&mut self) {
        self.output_buffer.update_all_lines();
    }
    pub fn update_line_for_rendering(&mut self, line_index: usize) {
        self.output_buffer.update_line(line_index);
    }
    pub fn advance_to_next_tabstop(&mut self, styles: RcCharacterStyles) {
        let next_tabstop = self
            .horizontal_tabstops
            .iter()
            .copied()
            .find(|&tabstop| tabstop > self.cursor.x && tabstop < self.width);
        match next_tabstop {
            Some(tabstop) => {
                self.cursor.x = tabstop;
            },
            None => {
                self.cursor.x = self.width.saturating_sub(1);
            },
        }
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = styles;
        self.pad_current_line_until(self.cursor.x, empty_character);
        self.output_buffer.update_line(self.cursor.y);
    }
    pub fn move_to_previous_tabstop(&mut self) {
        let previous_tabstop = self
            .horizontal_tabstops
            .iter()
            .rev()
            .copied()
            .find(|&tabstop| tabstop < self.cursor.x);
        match previous_tabstop {
            Some(tabstop) => {
                self.cursor.x = tabstop;
            },
            None => {
                self.cursor.x = 0;
            },
        }
    }
    pub fn cursor_shape(&self) -> CursorShape {
        self.cursor.get_shape()
    }
    pub fn scrollback_position_and_length(&self) -> (usize, usize) {
        // (position, length)
        (
            self.lines_below.len(),
            (self.scrollback_buffer_lines + self.lines_below.len()),
        )
    }

    fn recalculate_scrollback_buffer_count(&self) -> usize {
        let mut scrollback_buffer_count = 0;
        for row in &self.lines_above {
            let row_width = row.width();
            // rows in lines_above are unwrapped, so we need to account for that
            if row_width > self.width {
                scrollback_buffer_count += calculate_row_display_height(row_width, self.width);
            } else {
                scrollback_buffer_count += 1;
            }
        }
        scrollback_buffer_count
    }

    fn set_horizontal_tabstop(&mut self) {
        self.horizontal_tabstops.insert(self.cursor.x);
    }
    fn clear_tabstop(&mut self, position: usize) {
        self.horizontal_tabstops.remove(&position);
    }
    fn clear_all_tabstops(&mut self) {
        self.horizontal_tabstops.clear();
    }
    fn save_cursor_position(&mut self) {
        self.saved_cursor_position = Some(self.cursor.clone());
    }
    fn restore_cursor_position(&mut self) {
        if let Some(saved_cursor_position) = &self.saved_cursor_position {
            self.cursor = saved_cursor_position.clone();
        }
    }
    fn configure_charset(&mut self, charset: StandardCharset, index: CharsetIndex) {
        self.cursor.charsets[index] = charset;
    }
    fn set_active_charset(&mut self, index: CharsetIndex) {
        self.active_charset = index;
    }
    fn cursor_canonical_line_index(&self) -> usize {
        let mut cursor_canonical_line_index = 0;
        let mut canonical_lines_traversed = 0;
        for (i, line) in self.viewport.iter().enumerate() {
            if line.is_canonical {
                cursor_canonical_line_index = canonical_lines_traversed;
                canonical_lines_traversed += 1;
            }
            if i == self.cursor.y {
                break;
            }
        }
        cursor_canonical_line_index
    }
    // TODO: merge these two functions
    fn cursor_index_in_canonical_line(&self) -> usize {
        let mut cursor_canonical_line_index = 0;
        let mut cursor_index_in_canonical_line = 0;
        for (i, line) in self.viewport.iter().enumerate() {
            if line.is_canonical {
                cursor_canonical_line_index = i;
            }
            if i == self.cursor.y {
                let line_wraps = self.cursor.y.saturating_sub(cursor_canonical_line_index);
                cursor_index_in_canonical_line = (line_wraps * self.width) + self.cursor.x;
                break;
            }
        }
        cursor_index_in_canonical_line
    }
    fn saved_cursor_index_in_canonical_line(&self) -> Option<usize> {
        if let Some(saved_cursor_position) = self.saved_cursor_position.as_ref() {
            let mut cursor_canonical_line_index = 0;
            let mut cursor_index_in_canonical_line = 0;
            for (i, line) in self.viewport.iter().enumerate() {
                if line.is_canonical {
                    cursor_canonical_line_index = i;
                }
                if i == saved_cursor_position.y {
                    let line_wraps = saved_cursor_position.y - cursor_canonical_line_index;
                    cursor_index_in_canonical_line =
                        (line_wraps * self.width) + saved_cursor_position.x;
                    break;
                }
            }
            Some(cursor_index_in_canonical_line)
        } else {
            None
        }
    }
    fn canonical_line_y_coordinates(&self, canonical_line_index: usize) -> usize {
        let mut canonical_lines_traversed = 0;
        let mut y_coordinates = 0;
        for (i, line) in self.viewport.iter().enumerate() {
            if line.is_canonical {
                canonical_lines_traversed += 1;
                y_coordinates = i;
                if canonical_lines_traversed == canonical_line_index + 1 {
                    break;
                }
            }
        }
        y_coordinates
    }

    pub fn scroll_up_one_line(&mut self) -> bool {
        let mut found_something = false;
        if !self.lines_above.is_empty() && self.viewport.len() == self.height {
            self.is_scrolled = true;
            let line_to_push_down = self.viewport.pop().unwrap();
            self.lines_below.insert(0, line_to_push_down);

            let transferred_rows_height = transfer_rows_from_lines_above_to_viewport(
                &mut self.lines_above,
                &mut self.viewport,
                &mut self.sixel_grid,
                1,
                self.width,
            );
            self.scrollback_buffer_lines = self
                .scrollback_buffer_lines
                .saturating_sub(transferred_rows_height);

            self.selection.move_down(1);
            // Move all search-selections down one line as well
            found_something = self
                .search_results
                .move_down(1, &self.viewport, self.height);
        }
        self.output_buffer.update_all_lines();
        found_something
    }
    pub fn scroll_down_one_line(&mut self) -> bool {
        let mut found_something = false;
        if !self.lines_below.is_empty() && self.viewport.len() == self.height {
            let mut line_to_push_up = self.viewport.remove(0);

            self.scrollback_buffer_lines +=
                calculate_row_display_height(line_to_push_up.width(), self.width);

            let line_to_push_up = if line_to_push_up.is_canonical {
                line_to_push_up
            } else {
                match self.lines_above.pop_back() {
                    Some(mut last_line_above) => {
                        last_line_above.append(&mut line_to_push_up.columns);
                        last_line_above
                    },
                    None => {
                        // in this case, this line was not canonical but its beginning line was
                        // dropped out of scope, so we make it canonical and push it up
                        line_to_push_up.canonical()
                    },
                }
            };

            let dropped_line_width =
                bounded_push(&mut self.lines_above, &mut self.sixel_grid, line_to_push_up);
            if let Some(width) = dropped_line_width {
                let dropped_line_height = calculate_row_display_height(width, self.width);

                self.scrollback_buffer_lines = self
                    .scrollback_buffer_lines
                    .saturating_sub(dropped_line_height);
            }

            transfer_rows_from_lines_below_to_viewport(
                &mut self.lines_below,
                &mut self.viewport,
                1,
                self.width,
            );

            self.selection.move_up(1);
            // Move all search-selections up one line as well
            found_something =
                self.search_results
                    .move_up(1, &self.viewport, &self.lines_below, self.height);
            self.output_buffer.update_all_lines();
        }
        if self.lines_below.is_empty() {
            self.is_scrolled = false;
        }
        found_something
    }
    pub fn force_change_size(&mut self, new_rows: usize, new_columns: usize) {
        // this is an ugly hack - it's here because sometimes we need to change_size to the
        // existing size (eg. when resizing an alternative_grid to the current height/width) and
        // the change_size method is a no-op in that case. Should be fixed by making the
        // change_size method atomic
        let intermediate_rows = if new_rows == self.height {
            new_rows + 1
        } else {
            new_rows
        };
        let intermediate_columns = if new_columns == self.width {
            new_columns + 1
        } else {
            new_columns
        };
        self.change_size(intermediate_rows, intermediate_columns);
        self.change_size(new_rows, new_columns);
    }
    pub fn change_size(&mut self, new_rows: usize, new_columns: usize) {
        // Do nothing if this pane hasn't been given a proper size yet
        if new_columns == 0 || new_rows == 0 {
            return;
        }
        if self.alternate_screen_state.is_some() {
            // in alternate screen we do nothing but log the new size, the program in the terminal
            // is in control now...
            self.height = new_rows;
            self.width = new_columns;
            return;
        }
        self.selection.reset();
        self.sixel_grid.character_cell_size_possibly_changed();
        let cursors = if new_columns != self.width {
            self.horizontal_tabstops = create_horizontal_tabstops(new_columns);
            let mut cursor_canonical_line_index = self.cursor_canonical_line_index();
            let cursor_index_in_canonical_line = self.cursor_index_in_canonical_line();
            let saved_cursor_index_in_canonical_line = self.saved_cursor_index_in_canonical_line();
            let mut viewport_canonical_lines = vec![];
            for mut row in self.viewport.drain(..) {
                if !row.is_canonical
                    && viewport_canonical_lines.is_empty()
                    && !self.lines_above.is_empty()
                {
                    let mut first_line_above = self.lines_above.pop_back().unwrap();
                    first_line_above.append(&mut row.columns);
                    viewport_canonical_lines.push(first_line_above);
                    cursor_canonical_line_index += 1;
                } else if row.is_canonical {
                    viewport_canonical_lines.push(row);
                } else {
                    match viewport_canonical_lines.last_mut() {
                        Some(last_line) => {
                            last_line.append(&mut row.columns);
                        },
                        None => {
                            // the state is corrupted somehow
                            // this is a bug and I'm not yet sure why it happens
                            // usually it fixes itself and is a result of some race
                            // TODO: investigate why this happens and solve it
                            return;
                        },
                    }
                }
            }

            // trim lines after the last empty space that has no following character, because
            // terminals don't trim empty lines
            for line in &mut viewport_canonical_lines {
                let mut trim_at = None;
                for (index, character) in line.columns.iter().enumerate() {
                    if character.character != EMPTY_TERMINAL_CHARACTER.character {
                        trim_at = None;
                    } else if trim_at.is_none() {
                        trim_at = Some(index);
                    }
                }
                if let Some(trim_at) = trim_at {
                    let excess_width_until_trim_at = line.excess_width_until(trim_at);
                    line.truncate(trim_at + excess_width_until_trim_at);
                }
            }

            let mut new_viewport_rows = vec![];
            for mut canonical_line in viewport_canonical_lines {
                let mut canonical_line_parts: Vec<Row> = vec![];
                if canonical_line.columns.is_empty() {
                    canonical_line_parts.push(Row::new().canonical());
                }
                while !canonical_line.columns.is_empty() {
                    let next_wrap = canonical_line.drain_until(new_columns);
                    // If the next character is wider than the grid (i.e. there is nothing in
                    // `next_wrap`, then just abort the resizing
                    if next_wrap.is_empty() {
                        break;
                    }
                    let row = Row::from_columns(next_wrap);
                    // if there are no more parts, this row is canonical as long as it originally
                    // was canonical (it might not have been for example if it's the first row in
                    // the viewport, and the actual canonical row is above it in the scrollback)
                    let row = if canonical_line_parts.is_empty() && canonical_line.is_canonical {
                        row.canonical()
                    } else {
                        row
                    };
                    canonical_line_parts.push(row);
                }
                new_viewport_rows.append(&mut canonical_line_parts);
            }

            self.viewport = new_viewport_rows;

            let mut new_cursor_y = self.canonical_line_y_coordinates(cursor_canonical_line_index)
                + (cursor_index_in_canonical_line / new_columns);
            let mut saved_cursor_y_coordinates =
                self.saved_cursor_position.as_ref().map(|saved_cursor| {
                    self.canonical_line_y_coordinates(saved_cursor.y)
                        + saved_cursor_index_in_canonical_line.as_ref().unwrap() / new_columns
                });

            // A cursor at EOL has two equivalent positions - end of this line or beginning of
            // next. If not already at the beginning of line, bias to EOL so add character logic
            // doesn't create spurious canonical lines
            let mut new_cursor_x = cursor_index_in_canonical_line % new_columns;
            if self.cursor.x != 0 && new_cursor_x == 0 {
                new_cursor_y = new_cursor_y.saturating_sub(1);
                new_cursor_x = new_columns
            }
            let saved_cursor_x_coordinates = match (
                saved_cursor_index_in_canonical_line.as_ref(),
                self.saved_cursor_position.as_mut(),
                saved_cursor_y_coordinates.as_mut(),
            ) {
                (
                    Some(saved_cursor_index_in_canonical_line),
                    Some(saved_cursor_position),
                    Some(saved_cursor_y_coordinates),
                ) => {
                    let x = saved_cursor_position.x;
                    let mut new_x = *saved_cursor_index_in_canonical_line % new_columns;
                    let new_y = saved_cursor_y_coordinates;
                    if x != 0 && new_x == 0 {
                        *new_y = new_y.saturating_sub(1);
                        new_x = new_columns
                    }
                    Some(new_x)
                },
                _ => None,
            };
            Some((
                new_cursor_y,
                saved_cursor_y_coordinates,
                new_cursor_x,
                saved_cursor_x_coordinates,
            ))
        } else if new_rows != self.height {
            let saved_cursor_y_coordinates = self
                .saved_cursor_position
                .as_ref()
                .map(|saved_cursor| saved_cursor.y);
            let saved_cursor_x_coordinates = self
                .saved_cursor_position
                .as_ref()
                .map(|saved_cursor| saved_cursor.x);

            Some((
                self.cursor.y,
                saved_cursor_y_coordinates,
                self.cursor.x,
                saved_cursor_x_coordinates,
            ))
        } else {
            None
        };

        if let Some(cursors) = cursors {
            // At this point the x coordinates have been calculated, the y coordinates
            // will be updated within this block
            let (
                mut new_cursor_y,
                mut saved_cursor_y_coordinates,
                new_cursor_x,
                saved_cursor_x_coordinates,
            ) = cursors;

            let current_viewport_row_count = self.viewport.len();
            match current_viewport_row_count.cmp(&new_rows) {
                Ordering::Less => {
                    let row_count_to_transfer = new_rows - current_viewport_row_count;
                    transfer_rows_from_lines_above_to_viewport(
                        &mut self.lines_above,
                        &mut self.viewport,
                        &mut self.sixel_grid,
                        row_count_to_transfer,
                        new_columns,
                    );
                    let rows_pulled = self.viewport.len() - current_viewport_row_count;
                    new_cursor_y += rows_pulled;
                    if let Some(saved_cursor_y_coordinates) = saved_cursor_y_coordinates.as_mut() {
                        *saved_cursor_y_coordinates += rows_pulled;
                    };
                },
                Ordering::Greater => {
                    let row_count_to_transfer = current_viewport_row_count - new_rows;
                    if row_count_to_transfer > new_cursor_y {
                        new_cursor_y = 0;
                    } else {
                        new_cursor_y -= row_count_to_transfer;
                    }
                    if let Some(saved_cursor_y_coordinates) = saved_cursor_y_coordinates.as_mut() {
                        if row_count_to_transfer > *saved_cursor_y_coordinates {
                            *saved_cursor_y_coordinates = 0;
                        } else {
                            *saved_cursor_y_coordinates -= row_count_to_transfer;
                        }
                    }
                    transfer_rows_from_viewport_to_lines_above(
                        &mut self.viewport,
                        &mut self.lines_above,
                        &mut self.sixel_grid,
                        row_count_to_transfer,
                        new_columns,
                    );
                },
                Ordering::Equal => {},
            }
            self.cursor.y = new_cursor_y;
            self.cursor.x = new_cursor_x;
            if let Some(saved_cursor_position) = self.saved_cursor_position.as_mut() {
                match (saved_cursor_x_coordinates, saved_cursor_y_coordinates) {
                    (Some(saved_cursor_x_coordinates), Some(saved_cursor_y_coordinates)) => {
                        saved_cursor_position.x = saved_cursor_x_coordinates;
                        saved_cursor_position.y = saved_cursor_y_coordinates;
                    },
                    _ => log::error!(
                        "invalid state - cannot set saved cursor to {:?} {:?}",
                        saved_cursor_x_coordinates,
                        saved_cursor_y_coordinates
                    ),
                }
            };
        }
        self.height = new_rows;
        self.width = new_columns;
        self.set_scroll_region_to_viewport_size();
        self.scrollback_buffer_lines = self.recalculate_scrollback_buffer_count();
        self.search_results.selections.clear();
        self.search_viewport();
        // If we have thrown out the active element, set it to None
        self.search_results.unset_active_selection_if_nonexistent();
        self.output_buffer.update_all_lines();
    }
    pub fn as_character_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        // this is only used in the tests
        // it's not part of testing the app, but rather is used to interpret the snapshots created
        // by it
        let mut lines: Vec<Vec<TerminalCharacter>> = self
            .viewport
            .iter()
            .map(|r| {
                let excess_width = r.excess_width();
                let mut line: Vec<TerminalCharacter> = r.columns.iter().cloned().collect();
                // pad line
                line.resize(
                    self.width.saturating_sub(excess_width),
                    EMPTY_TERMINAL_CHARACTER,
                );
                line
            })
            .collect();
        let empty_row = vec![EMPTY_TERMINAL_CHARACTER; self.width];
        for _ in lines.len()..self.height {
            lines.push(empty_row.clone());
        }
        lines
    }
    pub fn read_changes(
        &mut self,
        x_offset: usize,
        y_offset: usize,
    ) -> (Vec<CharacterChunk>, Vec<SixelImageChunk>) {
        let changed_character_chunks = self.output_buffer.changed_chunks_in_viewport(
            &self.viewport,
            self.width,
            self.height,
            x_offset,
            y_offset,
        );
        let changed_rects = self
            .output_buffer
            .changed_rects_in_viewport(self.viewport.len());
        let changed_sixel_image_chunks = self.sixel_grid.changed_sixel_chunks_in_viewport(
            changed_rects,
            self.lines_above.len(),
            self.width,
            x_offset,
            y_offset,
        );
        if let Some(image_ids_to_reap) = self.sixel_grid.drain_image_ids_to_reap() {
            self.sixel_grid.reap_images(image_ids_to_reap);
        }
        self.output_buffer.clear();

        (changed_character_chunks, changed_sixel_image_chunks)
    }
    pub fn serialize(&self, scrollback_lines_to_serialize: Option<usize>) -> Option<String> {
        match scrollback_lines_to_serialize {
            Some(scrollback_lines_to_serialize) => {
                let first_index = if scrollback_lines_to_serialize == 0 {
                    0
                } else {
                    self.lines_above
                        .len()
                        .saturating_sub(scrollback_lines_to_serialize)
                };
                let mut to_serialize = vec![];
                for line in self.lines_above.iter().skip(first_index) {
                    to_serialize.push(line.clone());
                }
                for line in &self.viewport {
                    to_serialize.push(line.clone())
                }
                self.output_buffer.serialize(to_serialize.as_slice()).ok()
            },
            None => self.output_buffer.serialize(&self.viewport).ok(),
        }
    }
    pub fn render(
        &mut self,
        content_x: usize,
        content_y: usize,
        style: &Style,
    ) -> Result<Option<(Vec<CharacterChunk>, Option<String>, Vec<SixelImageChunk>)>> {
        if self.lock_renders {
            return Ok(None);
        }
        let mut raw_vte_output = String::new();

        let (mut character_chunks, sixel_image_chunks) = self.read_changes(content_x, content_y);
        for character_chunk in character_chunks.iter_mut() {
            character_chunk.add_changed_colors(self.changed_colors);
            if self
                .selection
                .contains_row(character_chunk.y.saturating_sub(content_y))
            {
                let background_color = match style.colors.text_selected.background {
                    PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                    PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                };
                let foreground_color = match style.colors.text_selected.base {
                    PaletteColor::Rgb(rgb) => AnsiCode::RgbCode(rgb),
                    PaletteColor::EightBit(col) => AnsiCode::ColorIndex(col),
                };

                character_chunk.add_selection_and_colors(
                    self.selection,
                    background_color,
                    Some(foreground_color),
                    content_x,
                    content_y,
                );
            } else if !self.search_results.selections.is_empty() {
                for res in self.search_results.selections.iter() {
                    if res.contains_row(character_chunk.y.saturating_sub(content_y)) {
                        let (select_background_palette, select_foreground_palette) =
                            if Some(res) == self.search_results.active.as_ref() {
                                (
                                    style.colors.text_unselected.emphasis_0,
                                    style.colors.text_unselected.background,
                                )
                            } else {
                                (
                                    style.colors.text_unselected.emphasis_2,
                                    style.colors.text_unselected.background,
                                )
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
        if self.ring_bell {
            let ring_bell = '\u{7}';
            raw_vte_output.push(ring_bell);
            self.ring_bell = false;
        }
        return Ok(Some((
            character_chunks,
            Some(raw_vte_output),
            sixel_image_chunks,
        )));
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        if self.cursor_is_hidden || self.cursor.x >= self.width || self.cursor.y >= self.height {
            None
        } else {
            Some((self.cursor.x, self.cursor.y))
        }
    }
    pub fn is_mid_frame(&self) -> bool {
        self.lock_renders
    }
    /// Clears all buffers with text for a current screen
    pub fn clear_screen(&mut self) {
        if self.alternate_screen_state.is_some() {
            log::warn!("Tried to clear pane with alternate_screen_state");
            return;
        }
        self.reset_terminal_state();
        self.mark_for_rerender();
    }
    /// Dumps all lines above terminal vieport and the viewport itself to a string
    pub fn dump_screen(&self, full: bool) -> String {
        let viewport: String = dump_screen!(self.viewport);
        if !full {
            return viewport;
        }
        let mut scrollback: String = dump_screen!(self.lines_above);
        if !scrollback.is_empty() {
            scrollback.push('\n');
        }
        scrollback.push_str(&viewport);
        scrollback
    }
    pub fn move_viewport_up(&mut self, count: usize) {
        for _ in 0..count {
            self.scroll_up_one_line();
        }
        self.output_buffer.update_all_lines();
    }
    pub fn move_viewport_down(&mut self, count: usize) {
        for _ in 0..count {
            self.scroll_down_one_line();
        }
        self.output_buffer.update_all_lines();
    }
    pub fn reset_viewport(&mut self) {
        let max_lines_to_scroll = *SCROLL_BUFFER_SIZE.get().unwrap() * 2; // while not very elegant, this can prevent minor bugs from becoming showstoppers by sticking the whole app display in an endless loop
        let mut lines_scrolled = 0;
        let should_clear_output_buffer = self.is_scrolled;
        while self.is_scrolled && lines_scrolled < max_lines_to_scroll {
            self.scroll_down_one_line();
            lines_scrolled += 1;
        }
        if should_clear_output_buffer {
            self.output_buffer.update_all_lines();
        }
    }
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        self.pad_lines_until(scroll_region_bottom, EMPTY_TERMINAL_CHARACTER);
        for _ in 0..count {
            if self.cursor.y >= scroll_region_top && self.cursor.y <= scroll_region_bottom {
                if self.viewport.get(scroll_region_bottom).is_some() {
                    self.viewport.remove(scroll_region_bottom);
                }
                let mut pad_character = EMPTY_TERMINAL_CHARACTER;
                pad_character.styles = self.cursor.pending_styles.clone();
                let columns = VecDeque::from(vec![pad_character; self.width]);
                self.viewport
                    .insert(scroll_region_top, Row::from_columns(columns).canonical());
            }
        }
        self.output_buffer.update_all_lines(); // TODO: only update scroll region lines
    }
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        self.pad_lines_until(scroll_region_bottom, EMPTY_TERMINAL_CHARACTER);
        let mut pad_character = EMPTY_TERMINAL_CHARACTER;
        pad_character.styles = self.cursor.pending_styles.clone();
        for _ in 0..count {
            if scroll_region_top < self.viewport.len() {
                self.viewport.remove(scroll_region_top);
            }
            let columns = VecDeque::from(vec![pad_character.clone(); self.width]);
            self.viewport
                .insert(scroll_region_bottom, Row::from_columns(columns).canonical());
        }
        self.output_buffer.update_all_lines(); // TODO: only update scroll region lines
    }
    pub fn fill_viewport(&mut self, character: TerminalCharacter) {
        if self.alternate_screen_state.is_some() {
            self.viewport.clear();
        } else {
            self.transfer_rows_to_lines_above(self.viewport.len())
        };

        for _ in 0..self.height {
            let columns = VecDeque::from(vec![character.clone(); self.width]);
            self.viewport.push(Row::from_columns(columns).canonical());
        }
        self.output_buffer.update_all_lines();
    }
    pub fn add_canonical_line(&mut self) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        self.hyperlink_tracker.update(
            '\n',
            &self.cursor,
            &mut self.viewport,
            &mut self.lines_above,
            &mut self.link_handler.borrow_mut(),
        );
        if self.cursor.y == scroll_region_bottom {
            // end of scroll region
            // when we have a scroll region set and we're at its bottom
            // we need to delete its first line, thus shifting all lines in it upwards
            // then we add an empty line at its end which will be filled by the application
            // controlling the scroll region (presumably filled by whatever comes next in the
            // scroll buffer, but that's not something we control)
            if scroll_region_top >= self.viewport.len() {
                // the state is corrupted
                return;
            }
            if scroll_region_bottom == self.height.saturating_sub(1) && scroll_region_top == 0 {
                if self.alternate_screen_state.is_none() {
                    self.transfer_rows_to_lines_above(1);
                } else {
                    self.viewport.remove(0);
                }

                self.viewport.push(Row::new().canonical());
                self.selection.move_up(1);
            } else {
                self.viewport.remove(scroll_region_top);
                if self.viewport.len() >= scroll_region_bottom {
                    self.viewport
                        .insert(scroll_region_bottom, Row::new().canonical());
                } else {
                    self.viewport.push(Row::new().canonical());
                }
            }
            self.output_buffer.update_all_lines(); // TODO: only update scroll region lines
            return;
        }
        if self.viewport.len() <= self.cursor.y + 1 {
            // FIXME: this should add an empty line with the pad_character
            // but for some reason this breaks rendering in various situations
            // it needs to be investigated and fixed
            let new_row = Row::new().canonical();
            self.viewport.push(new_row);
        }
        if self.cursor.y == self.height.saturating_sub(1) {
            self.output_buffer.update_all_lines();
        } else {
            self.cursor.y += 1;
            self.output_buffer.update_line(self.cursor.y);
        }
    }
    pub fn move_cursor_to_beginning_of_line(&mut self) {
        self.cursor.x = 0;
    }
    pub fn add_character_at_cursor_position(
        &mut self,
        terminal_character: TerminalCharacter,
        should_insert_character: bool,
    ) {
        self.hyperlink_tracker.update(
            terminal_character.character,
            &self.cursor,
            &mut self.viewport,
            &mut self.lines_above,
            &mut self.link_handler.borrow_mut(),
        );
        // this function assumes the current line has enough room for terminal_character (that its
        // width has been checked beforehand)
        match self.viewport.get_mut(self.cursor.y) {
            Some(row) => {
                if self.insert_mode || should_insert_character {
                    row.insert_character_at(terminal_character, self.cursor.x);
                    if row.width() > self.width {
                        row.truncate(self.width);
                    }
                } else {
                    row.add_character_at(terminal_character, self.cursor.x);
                }
                if let Some(character_cell_size) = *self.character_cell_size.borrow() {
                    let scrollback_size_in_pixels =
                        self.lines_above.len() * character_cell_size.height;
                    let absolute_x_in_pixels = self.cursor.x * character_cell_size.width;
                    let absolute_y_in_pixels =
                        scrollback_size_in_pixels + (self.cursor.y * character_cell_size.height);
                    let rect_to_cut_out = PixelRect {
                        x: absolute_x_in_pixels,
                        y: absolute_y_in_pixels as isize,
                        width: character_cell_size.width,
                        height: character_cell_size.height,
                    };
                    if let Some(images_to_cut_out) =
                        self.sixel_grid.cut_off_rect_from_images(rect_to_cut_out)
                    {
                        for (image_id, rect_in_image_to_cut_out) in images_to_cut_out {
                            self.sixel_grid
                                .remove_pixels_from_image(image_id, rect_in_image_to_cut_out);
                        }
                    }
                }
                self.output_buffer.update_line(self.cursor.y);
            },
            None => {
                // pad lines until cursor if they do not exist
                for _ in self.viewport.len()..self.cursor.y {
                    self.viewport.push(Row::new().canonical());
                }
                self.viewport
                    .push(Row::new().with_character(terminal_character).canonical());
                self.output_buffer.update_line(self.cursor.y);
            },
        }
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
        let character_width = terminal_character.width();
        // Drop zero-width Unicode/UTF-8 codepoints, like for example Variation Selectors.
        // This breaks unicode grapheme segmentation, and is the reason why some characters
        // aren't displayed correctly. Refer to this issue for more information:
        //     https://github.com/zellij-org/zellij/issues/1538
        if character_width == 0 {
            return;
        }
        if self.cursor.x + character_width > self.width {
            if self.disable_linewrap {
                return;
            }
            self.line_wrap();
        }
        self.add_character_at_cursor_position(terminal_character, false);
        self.move_cursor_forward_until_edge(character_width);
    }
    pub fn get_character_under_cursor(&self) -> Option<TerminalCharacter> {
        let absolute_x_in_line = self.get_absolute_character_index(self.cursor.x, self.cursor.y)?;
        self.viewport
            .get(self.cursor.y)
            .and_then(|current_line| current_line.columns.get(absolute_x_in_line))
            .cloned()
    }
    pub fn get_absolute_character_index(&self, x: usize, y: usize) -> Option<usize> {
        Some(self.viewport.get(y)?.absolute_character_index(x))
    }
    pub fn move_cursor_forward_until_edge(&mut self, count: usize) {
        let count_to_move = std::cmp::min(count, self.width.saturating_sub(self.cursor.x));
        self.cursor.x += count_to_move;
    }
    pub fn replace_characters_in_line_after_cursor(&mut self, replace_with: TerminalCharacter) {
        if let Some(row) = self.viewport.get_mut(self.cursor.y) {
            row.replace_and_pad_end(self.cursor.x, self.width, replace_with);
        }
        self.output_buffer.update_line(self.cursor.y);
    }
    pub fn replace_characters_in_line_before_cursor(&mut self, replace_with: TerminalCharacter) {
        let row = self.viewport.get_mut(self.cursor.y).unwrap();
        row.replace_and_pad_beginning(self.cursor.x, replace_with);
        self.output_buffer.update_line(self.cursor.y);
    }
    pub fn clear_all_after_cursor(&mut self, replace_with: TerminalCharacter) {
        if let Some(cursor_row) = self.viewport.get_mut(self.cursor.y) {
            cursor_row.truncate(self.cursor.x);
            let replace_with_columns = VecDeque::from(vec![replace_with.clone(); self.width]);
            self.replace_characters_in_line_after_cursor(replace_with);
            for row in self.viewport.iter_mut().skip(self.cursor.y + 1) {
                row.replace_columns(replace_with_columns.clone());
            }
            self.output_buffer.update_all_lines(); // TODO: only update the changed lines
        }
    }
    pub fn clear_all_before_cursor(&mut self, replace_with: TerminalCharacter) {
        if self.viewport.get(self.cursor.y).is_some() {
            let replace_with_columns = VecDeque::from(vec![replace_with.clone(); self.width]);
            self.replace_characters_in_line_before_cursor(replace_with);
            for row in self.viewport.iter_mut().take(self.cursor.y) {
                row.replace_columns(replace_with_columns.clone());
            }
            self.output_buffer.update_all_lines(); // TODO: only update the changed lines
        }
    }
    pub fn clear_cursor_line(&mut self) {
        if let Some(viewport_line) = self.viewport.get_mut(self.cursor.y) {
            viewport_line.truncate(0);
            self.output_buffer.update_line(self.cursor.y);
        }
    }
    pub fn clear_all(&mut self, replace_with: TerminalCharacter) {
        let replace_with_columns = VecDeque::from(vec![replace_with.clone(); self.width]);
        self.replace_characters_in_line_after_cursor(replace_with);
        for row in &mut self.viewport {
            row.replace_columns(replace_with_columns.clone());
        }
        self.output_buffer.update_all_lines();
    }
    fn line_wrap(&mut self) {
        self.cursor.x = 0;
        if self.cursor.y == self.height.saturating_sub(1) {
            if self.alternate_screen_state.is_none() {
                self.transfer_rows_to_lines_above(1);
                self.hyperlink_tracker.offset_cursor_lines(1);
            } else {
                self.viewport.remove(0);
            }
            let wrapped_row = Row::new();
            self.viewport.push(wrapped_row);
            self.selection.move_up(1);
            self.output_buffer.update_all_lines();
        } else {
            self.cursor.y += 1;
            if self.viewport.len() <= self.cursor.y {
                let line_wrapped_row = Row::new();
                self.viewport.push(line_wrapped_row);
                self.output_buffer.update_line(self.cursor.y);
            } else if let Some(current_line) = self.viewport.get_mut(self.cursor.y) {
                current_line.is_canonical = false;
            }
        }
    }
    fn clear_lines_above(&mut self) {
        self.lines_above.clear();
        self.scrollback_buffer_lines = self.recalculate_scrollback_buffer_count();
    }

    fn pad_current_line_until(&mut self, position: usize, pad_character: TerminalCharacter) {
        if self.viewport.get(self.cursor.y).is_none() {
            self.pad_lines_until(self.cursor.y, pad_character.clone());
        }
        if let Some(current_row) = self.viewport.get_mut(self.cursor.y) {
            for _ in current_row.width()..position {
                current_row.push(pad_character.clone());
            }
            self.output_buffer.update_line(self.cursor.y);
        }
    }
    fn pad_lines_until(&mut self, position: usize, pad_character: TerminalCharacter) {
        for _ in self.viewport.len()..=position {
            let columns = VecDeque::from(vec![pad_character.clone(); self.width]);
            self.viewport.push(Row::from_columns(columns).canonical());
            self.output_buffer.update_line(self.viewport.len() - 1);
        }
    }
    pub fn move_cursor_to(&mut self, x: usize, y: usize, pad_character: TerminalCharacter) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        self.cursor.x = std::cmp::min(self.width - 1, x);
        let y_offset = if self.erasure_mode {
            scroll_region_top
        } else {
            0
        };
        if y >= scroll_region_top && y <= scroll_region_bottom {
            self.cursor.y = std::cmp::min(scroll_region_bottom, y + y_offset);
        } else {
            self.cursor.y = std::cmp::min(self.height.saturating_sub(1), y + y_offset);
        }
        self.pad_lines_until(self.cursor.y, pad_character.clone());
        self.pad_current_line_until(self.cursor.x, pad_character);
    }
    pub fn move_cursor_up(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        if self.cursor.y >= scroll_region_top && self.cursor.y <= scroll_region_bottom {
            self.cursor.y = std::cmp::max(self.cursor.y.saturating_sub(count), scroll_region_top);
            return;
        }
        self.cursor.y = if self.cursor.y < count {
            0
        } else {
            self.cursor.y - count
        };
    }
    pub fn move_cursor_up_with_scrolling(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        for _ in 0..count {
            let current_line_index = self.cursor.y;
            if current_line_index == scroll_region_top {
                // if we're at the top line, we create a new line and remove the last line that
                // would otherwise overflow
                if scroll_region_bottom < self.viewport.len() {
                    self.viewport.remove(scroll_region_bottom);
                }

                self.viewport
                    .insert(current_line_index, Row::new().canonical());
            } else if current_line_index > scroll_region_top
                && current_line_index <= scroll_region_bottom
            {
                self.move_cursor_up(count);
            }
        }
        self.output_buffer.update_all_lines();
    }
    pub fn move_cursor_down_until_edge_of_screen(
        &mut self,
        count: usize,
        pad_character: TerminalCharacter,
    ) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        if self.cursor.y >= scroll_region_top && self.cursor.y <= scroll_region_bottom {
            self.cursor.y = std::cmp::min(self.cursor.y + count, scroll_region_bottom);
            return;
        }
        self.cursor.y = std::cmp::min(self.cursor.y + count, self.height - 1);
        self.pad_lines_until(self.cursor.y, pad_character);
    }
    pub fn move_cursor_back(&mut self, count: usize) {
        if self.cursor.x == self.width {
            // on the rightmost screen edge, backspace skips one character
            self.cursor.x -= 1;
        }
        if self.cursor.x < count {
            self.cursor.x = 0;
        } else {
            self.cursor.x -= count;
        }
    }
    pub fn hide_cursor(&mut self) {
        self.cursor_is_hidden = true;
    }
    pub fn show_cursor(&mut self) {
        self.cursor_is_hidden = false;
    }
    pub fn set_scroll_region(&mut self, top_line_index: usize, bottom_line_index: Option<usize>) {
        let bottom_line_index = bottom_line_index.unwrap_or(self.height.saturating_sub(1));
        self.scroll_region = (top_line_index, bottom_line_index);
        let mut pad_character = EMPTY_TERMINAL_CHARACTER;
        pad_character.styles = self.cursor.pending_styles.clone();
        self.move_cursor_to(0, 0, pad_character); // DECSTBM moves the cursor to column 1 line 1 of the page
    }
    pub fn set_scroll_region_to_viewport_size(&mut self) {
        self.scroll_region = (0, self.height.saturating_sub(1));
    }
    pub fn delete_lines_in_scroll_region(
        &mut self,
        count: usize,
        pad_character: TerminalCharacter,
    ) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        let current_line_index = self.cursor.y;
        if current_line_index >= scroll_region_top && current_line_index <= scroll_region_bottom {
            // when deleting lines inside the scroll region, we must make sure it stays the
            // same size (and that other lines below it aren't shifted inside it)
            // so we delete the current line(s) and add an empty line at the end of the scroll
            // region
            for _ in 0..count {
                self.viewport.remove(current_line_index);
                let columns = VecDeque::from(vec![pad_character.clone(); self.width]);
                if self.viewport.len() > scroll_region_bottom {
                    self.viewport
                        .insert(scroll_region_bottom, Row::from_columns(columns).canonical());
                } else {
                    self.viewport.push(Row::from_columns(columns).canonical());
                }
            }
            self.output_buffer.update_all_lines(); // TODO: move accurately
        }
    }
    pub fn add_empty_lines_in_scroll_region(
        &mut self,
        count: usize,
        pad_character: TerminalCharacter,
    ) {
        let (scroll_region_top, scroll_region_bottom) = self.scroll_region;
        let current_line_index = self.cursor.y;
        if current_line_index >= scroll_region_top && current_line_index <= scroll_region_bottom {
            // when adding empty lines inside the scroll region, we must make sure it stays the
            // same size and that lines don't "leak" outside of it
            // so we add an empty line where the cursor currently is, and delete the last line
            // of the scroll region
            for _ in 0..count {
                if scroll_region_bottom < self.viewport.len() {
                    self.viewport.remove(scroll_region_bottom);
                }
                let columns = VecDeque::from(vec![pad_character.clone(); self.width]);
                self.viewport
                    .insert(current_line_index, Row::from_columns(columns).canonical());
            }
            self.output_buffer.update_all_lines(); // TODO: move accurately
        }
    }
    pub fn move_cursor_to_column(&mut self, column: usize) {
        self.cursor.x = column;
        let pad_character = EMPTY_TERMINAL_CHARACTER;
        self.pad_current_line_until(self.cursor.x, pad_character);
    }
    pub fn move_cursor_to_line(&mut self, line: usize, pad_character: TerminalCharacter) {
        self.cursor.y = std::cmp::min(self.height - 1, line);
        self.pad_lines_until(self.cursor.y, pad_character);
        let pad_character = EMPTY_TERMINAL_CHARACTER;
        self.pad_current_line_until(self.cursor.x, pad_character);
    }
    pub fn replace_with_empty_chars(&mut self, count: usize, empty_char_style: RcCharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        let pad_until = std::cmp::min(self.width, self.cursor.x + count);
        self.pad_current_line_until(pad_until, empty_character.clone());
        if let Some(current_row) = self.viewport.get_mut(self.cursor.y) {
            for i in 0..count {
                current_row.replace_character_at(empty_character.clone(), self.cursor.x + i);
            }
            self.output_buffer.update_line(self.cursor.y);
        }
    }
    fn erase_characters(&mut self, count: usize, empty_char_style: RcCharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        if let Some(current_row) = self.viewport.get_mut(self.cursor.y) {
            // pad row if needed
            if current_row.width_cached() < self.width {
                let padding_count = self.width - current_row.width_cached();
                let mut columns_padding =
                    VecDeque::from(vec![EMPTY_TERMINAL_CHARACTER; padding_count]);
                current_row.columns.append(&mut columns_padding);
            }
            for _ in 0..count {
                let deleted_character = current_row.delete_and_return_character(self.cursor.x);
                let excess_width = deleted_character
                    .map(|terminal_character| terminal_character.width())
                    .unwrap_or(0)
                    .saturating_sub(1);
                for _ in 0..excess_width {
                    current_row.insert_character_at(empty_character.clone(), self.cursor.x);
                }
                current_row.push(empty_character.clone());
            }
            self.output_buffer.update_line(self.cursor.y);
        }
    }
    fn add_newline(&mut self) {
        self.add_canonical_line();
        self.mark_for_rerender();
    }
    pub fn mark_for_rerender(&mut self) {
        self.should_render = true;
    }
    pub fn reset_terminal_state(&mut self) {
        self.lines_above = VecDeque::new();
        self.lines_below = vec![];
        self.viewport = vec![Row::new().canonical()];
        self.alternate_screen_state = None;
        self.cursor_key_mode = false;
        self.clear_viewport_before_rendering = true;
        self.cursor = Cursor::new(0, 0, self.styled_underlines);
        self.saved_cursor_position = None;
        self.active_charset = Default::default();
        self.erasure_mode = false;
        self.disable_linewrap = false;
        self.new_line_mode = false;
        self.cursor.change_shape(CursorShape::Initial);
        self.output_buffer.update_all_lines();
        self.changed_colors = None;
        self.scrollback_buffer_lines = 0;
        self.search_results = Default::default();
        self.sixel_scrolling = false;
        self.mouse_mode = MouseMode::NoEncoding;
        self.mouse_tracking = MouseTracking::Off;
        self.focus_event_tracking = false;
        self.cursor_is_hidden = false;
        self.supports_kitty_keyboard_protocol = false;
        self.set_scroll_region_to_viewport_size();
        if let Some(images_to_reap) = self.sixel_grid.clear() {
            self.sixel_grid.reap_images(images_to_reap);
        }
    }
    fn set_preceding_character(&mut self, terminal_character: TerminalCharacter) {
        self.preceding_char = Some(terminal_character);
    }
    pub fn start_selection(&mut self, start: &Position) {
        let old_selection = self.selection;
        self.click.record_click(*start);

        if self.click.is_double_click() {
            let Some((start_position, end_position)) = self.word_around_position(&start) else {
                // no-op
                return;
            };
            self.selection
                .set_start_and_end_positions(start_position, end_position);
            for i in std::cmp::min(start_position.line.0, end_position.line.0)
                ..=std::cmp::max(start_position.line.0, end_position.line.0)
            {
                self.output_buffer.update_line(i as usize);
            }
            self.mark_for_rerender();
            return;
        } else if self.click.is_triple_click() {
            let Some((start_position, end_position)) = self.canonical_line_around_position(&start)
            else {
                // no-op
                return;
            };
            self.selection
                .set_start_and_end_positions(start_position, end_position);
            for i in std::cmp::min(start_position.line.0, end_position.line.0)
                ..=std::cmp::max(start_position.line.0, end_position.line.0)
            {
                self.output_buffer.update_line(i as usize);
            }
            self.mark_for_rerender();
            return;
        }

        self.selection.start(*start);
        self.update_selected_lines(&old_selection, &self.selection.clone());
        self.mark_for_rerender();
    }
    pub fn update_selection(&mut self, to: &Position) {
        let old_selection = self.selection;
        if &old_selection.end != to {
            if self.click.is_double_click() {
                let Some((word_start_position, word_end_position)) = self.word_around_position(&to)
                else {
                    // no-op
                    return;
                };
                self.selection
                    .add_word_to_position(word_start_position, word_end_position);
                let current_selection = self.selection;
                self.update_selected_lines(&old_selection, &current_selection);
                self.mark_for_rerender();
            } else if self.click.is_triple_click() {
                let Some(last_index_in_line) = self.last_index_in_line(&to) else {
                    return;
                };
                self.selection
                    .add_line_to_position(to.line.0, last_index_in_line);
                let current_selection = self.selection;
                self.update_selected_lines(&old_selection, &current_selection);
                self.mark_for_rerender();
            } else {
                self.selection.to(*to);
                self.update_selected_lines(&old_selection, &self.selection.clone());
                self.mark_for_rerender();
            }
        }
    }

    pub fn end_selection(&mut self, end: &Position) {
        if !self.click.is_double_click() && !self.click.is_triple_click() {
            let old_selection = self.selection;
            self.selection.end(*end);
            self.update_selected_lines(&old_selection, &self.selection.clone());
        }
        self.mark_for_rerender();
    }

    pub fn reset_selection(&mut self) {
        let old_selection = self.selection;
        self.selection.reset();
        self.update_selected_lines(&old_selection, &self.selection.clone());
        self.mark_for_rerender();
    }
    pub fn get_selected_text(&self) -> Option<String> {
        if self.selection.is_empty() {
            return None;
        }
        let mut selection: Vec<String> = vec![];

        let sorted_selection = self.selection.sorted();
        let (start, end) = (sorted_selection.start, sorted_selection.end);

        for l in sorted_selection.line_indices() {
            let mut line_selection = String::new();

            // on the first line of the selection, use the selection start column
            // otherwise, start at the beginning of the line
            let start_column = if l == start.line.0 { start.column.0 } else { 0 };

            // same thing on the last line, but with the selection end column
            let end_column = if l == end.line.0 {
                end.column.0
            } else {
                self.width
            };

            if start_column == end_column {
                continue;
            }

            let empty_row =
                Row::from_columns(VecDeque::from(vec![EMPTY_TERMINAL_CHARACTER; self.width]));

            // get the row from lines_above, viewport, or lines below depending on index
            let row = if l < 0 && self.lines_above.len() > l.abs() as usize {
                let offset_from_end = l.abs();
                &self.lines_above[self
                    .lines_above
                    .len()
                    .saturating_sub(offset_from_end as usize)]
            } else if l >= 0 && (l as usize) < self.viewport.len() {
                &self.viewport[l as usize]
            } else if (l as usize) < self.height {
                // index is in viewport but there is no line
                &empty_row
            } else if self.lines_below.len() > (l as usize).saturating_sub(self.viewport.len()) {
                &self.lines_below[(l as usize) - self.viewport.len()]
            } else {
                // can't find the line, this probably it's on the pane border
                // is on the pane border
                continue;
            };

            let mut terminal_col = 0;
            for terminal_character in &row.columns {
                if (start_column..end_column).contains(&terminal_col) {
                    line_selection.push(terminal_character.character);
                }

                terminal_col += terminal_character.width();
            }

            if row.is_canonical {
                selection.push(line_selection);
            } else {
                // rejoin wrapped lines if possible
                match selection.last_mut() {
                    Some(previous_line) => previous_line.push_str(&line_selection),
                    None => selection.push(line_selection),
                }
            }
        }

        // TODO: distinguish whitespace that was output explicitly vs implicitly (e.g add_newline)
        // for example: echo "     " vs empty lines
        // for now trim after building the selection to handle whitespace in wrapped lines
        let selection: Vec<_> = selection.iter().map(|l| l.trim_end()).collect();

        if selection.is_empty() {
            None
        } else {
            Some(selection.join("\n"))
        }
    }
    pub fn absolute_position_in_scrollback(&self) -> usize {
        self.lines_above.len() + self.cursor.y
    }
    pub fn last_index_in_line(&self, position: &Position) -> Option<usize> {
        let position_row = self.viewport.get(position.line.0 as usize)?;
        Some(position_row.last_index_in_line())
    }
    pub fn word_around_position(&self, position: &Position) -> Option<(Position, Position)> {
        let position_row = self.viewport.get(position.line.0 as usize)?;
        let (index_start, index_end) =
            position_row.word_indices_around_character_index(position.column.0)?;

        let mut position_start = Position::new(position.line.0 as i32, index_start as u16);
        let mut position_end = Position::new(position.line.0 as i32, index_end as u16);
        let mut position_row_is_canonical = position_row.is_canonical;

        while !position_row_is_canonical && position_start.column.0 == 0 {
            if let Some(position_row_above) = self
                .viewport
                .get(position_start.line.0.saturating_sub(1) as usize)
            {
                let new_start_index = position_row_above.word_start_index_of_last_character();
                position_start = Position::new(
                    position_start.line.0.saturating_sub(1) as i32,
                    new_start_index as u16,
                );
                position_row_is_canonical = position_row_above.is_canonical;
            } else {
                break;
            }
        }

        let mut column_count_in_row = position_row.columns.len();
        while position_end.column.0 == column_count_in_row {
            if let Some(position_row_below) = self.viewport.get(position_end.line.0 as usize + 1) {
                if position_row_below.is_canonical {
                    break;
                }
                let new_end_index = position_row_below.word_end_index_of_first_character();
                position_end = Position::new(position_end.line.0 as i32 + 1, new_end_index as u16);
                column_count_in_row = position_row_below.columns.len();
            } else {
                break;
            }
        }

        Some((position_start, position_end))
    }
    pub fn canonical_line_around_position(
        &self,
        position: &Position,
    ) -> Option<(Position, Position)> {
        let position_row = self.viewport.get(position.line.0 as usize)?;

        let mut position_start = Position::new(position.line.0 as i32, 0);
        let mut position_end = Position::new(
            position.line.0 as i32,
            (position_row.columns.len() + position_row.excess_width()) as u16,
        );

        let mut found_canonical_row_start = position_row.is_canonical;
        while !found_canonical_row_start {
            if let Some(row_above) = self
                .viewport
                .get(position_start.line.0.saturating_sub(1) as usize)
            {
                position_start.line.0 = position_start.line.0.saturating_sub(1);
                found_canonical_row_start = row_above.is_canonical;
            } else {
                break;
            }
        }

        let mut found_canonical_row_end = false;
        while !found_canonical_row_end {
            if let Some(row_below) = self.viewport.get(position_end.line.0 as usize + 1) {
                if row_below.is_canonical {
                    found_canonical_row_end = true;
                } else {
                    position_end = Position::new(
                        position_end.line.0 as i32 + 1,
                        row_below.columns.len() as u16,
                    );
                }
            } else {
                break;
            }
        }
        Some((position_start, position_end))
    }

    fn update_selected_lines(&mut self, old_selection: &Selection, new_selection: &Selection) {
        for l in old_selection.diff(new_selection, self.height) {
            self.output_buffer.update_line(l as usize);
        }
    }
    fn set_title(&mut self, title: String) {
        self.title = Some(title);
    }
    fn push_current_title_to_stack(&mut self) {
        if self.title_stack.len() > MAX_TITLE_STACK_SIZE {
            self.title_stack.remove(0);
        }
        if let Some(title) = &self.title {
            self.title_stack.push(title.clone());
        }
    }
    fn pop_title_from_stack(&mut self) {
        if let Some(popped_title) = self.title_stack.pop() {
            self.title = Some(popped_title);
        }
    }
    fn transfer_rows_to_lines_above(&mut self, count: usize) {
        let transferred_rows_count = transfer_rows_from_viewport_to_lines_above(
            &mut self.viewport,
            &mut self.lines_above,
            &mut self.sixel_grid,
            count,
            self.width,
        );

        self.scrollback_buffer_lines =
            subtract_isize_from_usize(self.scrollback_buffer_lines, transferred_rows_count);
    }
    fn move_cursor_down_by_pixels(&mut self, pixel_count: usize) {
        if let Some(character_cell_size) = {
            let c = *self.character_cell_size.borrow();
            c
        } {
            // thanks borrow checker
            let pixel_height = character_cell_size.height;
            let to_move = (pixel_count as f64 / pixel_height as f64).ceil() as usize;
            for _ in 0..to_move {
                self.add_canonical_line();
            }
        }
    }
    fn current_cursor_pixel_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        if let Some(character_cell_size) = *self.character_cell_size.borrow() {
            let line_count_in_scrollback = self.lines_above.len();
            let y_coordinates =
                (line_count_in_scrollback + self.cursor.y) * character_cell_size.height;
            let x_coordinates = self.cursor.x * character_cell_size.width;
            Some((x_coordinates, y_coordinates))
        } else {
            None
        }
    }
    fn create_sixel_image(&mut self) {
        if let Some((x_pixel_coordinates, y_pixel_coordinates)) =
            self.current_cursor_pixel_coordinates()
        {
            let (x_pixel_coordinates, y_pixel_coordinates) = if self.sixel_scrolling {
                let scrollback_pixel_height =
                    self.lines_above.len() * self.character_cell_size.borrow().unwrap().height;
                (0, scrollback_pixel_height)
            } else {
                (x_pixel_coordinates, y_pixel_coordinates)
            };
            let new_image_id = self.sixel_grid.next_image_id();
            let new_sixel_image =
                self.sixel_grid
                    .end_image(new_image_id, x_pixel_coordinates, y_pixel_coordinates);
            if let Some(new_sixel_image) = new_sixel_image {
                let (image_pixel_height, _image_pixel_width) = new_sixel_image.pixel_size();
                self.sixel_grid
                    .new_sixel_image(new_image_id, new_sixel_image);
                if !self.sixel_scrolling {
                    self.move_cursor_down_by_pixels(image_pixel_height);
                }
                self.render_full_viewport(); // TODO: this could be optimized if it's a performance bottleneck
            }
        }
    }
    fn mouse_buttons_value_x10(&self, event: &MouseEvent) -> u8 {
        let mut value = 35; // Default to no buttons down.
        if event.event_type == MouseEventType::Release {
            return value;
        }
        if event.left {
            value = 32;
        } else if event.middle {
            value = 33;
        } else if event.right {
            value = 34;
        } else if event.wheel_up {
            value = 68;
        } else if event.wheel_down {
            value = 69;
        }
        if event.event_type == MouseEventType::Motion {
            value += 32;
        }
        if event.shift {
            value |= 0x04;
        }
        if event.alt {
            value |= 0x08;
        }
        if event.ctrl {
            value |= 0x10;
        }
        value
    }
    fn mouse_buttons_value_sgr(&self, event: &MouseEvent) -> u8 {
        let mut value = 3; // Default to no buttons down.
        if event.left {
            value = 0;
        } else if event.middle {
            value = 1;
        } else if event.right {
            value = 2;
        } else if event.wheel_up {
            value = 64;
        } else if event.wheel_down {
            value = 65;
        }
        if event.event_type == MouseEventType::Motion {
            value += 32;
        }
        if event.shift {
            value |= 0x04;
        }
        if event.alt {
            value |= 0x08;
        }
        if event.ctrl {
            value |= 0x10;
        }
        value
    }
    pub fn mouse_event_signal(&self, event: &MouseEvent) -> Option<String> {
        let emit = match (&self.mouse_tracking, event.event_type) {
            (MouseTracking::Off, _) => false,
            (MouseTracking::AnyEventTracking, _) => true,
            (_, MouseEventType::Press | MouseEventType::Release) => true,
            (MouseTracking::ButtonEventTracking, MouseEventType::Motion) => {
                event.left | event.right | event.middle | event.wheel_up | event.wheel_down
            },
            (_, _) => false,
        };

        match (emit, &self.mouse_mode) {
            (true, MouseMode::NoEncoding | MouseMode::Utf8) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', self.mouse_buttons_value_x10(event)];
                msg.append(&mut utf8_mouse_coordinates(
                    event.position.column() + 1,
                    event.position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (true, MouseMode::Sgr) => Some(format!(
                "\u{1b}[<{:?};{:?};{:?}{}",
                self.mouse_buttons_value_sgr(event),
                event.position.column() + 1,
                event.position.line() + 1,
                match event.event_type {
                    MouseEventType::Press | MouseEventType::Motion => 'M',
                    _ => 'm',
                }
            )),
            (_, _) => None,
        }
    }
    pub fn mouse_left_click_signal(&self, position: &Position, is_held: bool) -> Option<String> {
        let utf8_event = || -> Option<String> {
            let button_code = if is_held { b'@' } else { b' ' };
            let mut msg: Vec<u8> = vec![27, b'[', b'M', button_code];
            msg.append(&mut utf8_mouse_coordinates(
                position.column() + 1,
                position.line() + 1,
            ));
            Some(String::from_utf8_lossy(&msg).into())
        };
        let sgr_event = || -> Option<String> {
            let button_code = if is_held { 32 } else { 0 };
            Some(format!(
                "\u{1b}[<{:?};{:?};{:?}M",
                button_code,
                position.column() + 1,
                position.line() + 1
            ))
        };
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, MouseTracking::Normal) if !is_held => {
                utf8_event()
            },
            (
                MouseMode::NoEncoding | MouseMode::Utf8,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => utf8_event(),
            (
                MouseMode::Sgr,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => sgr_event(),
            (MouseMode::Sgr, MouseTracking::Normal) if !is_held => sgr_event(),
            _ => None,
        }
    }
    pub fn mouse_left_click_release_signal(&self, position: &Position) -> Option<String> {
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, _) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', b'#'];
                msg.append(&mut utf8_mouse_coordinates(
                    position.column() + 1,
                    position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (MouseMode::Sgr, _) => {
                let mouse_event = format!(
                    "\u{1b}[<0;{:?};{:?}m",
                    position.column() + 1,
                    position.line() + 1
                );
                Some(mouse_event)
            },
        }
    }
    pub fn mouse_right_click_signal(&self, position: &Position, is_held: bool) -> Option<String> {
        let utf8_event = || -> Option<String> {
            let button_code = if is_held { b'B' } else { b'"' };
            let mut msg: Vec<u8> = vec![27, b'[', b'M', button_code];
            msg.append(&mut utf8_mouse_coordinates(
                position.column() + 1,
                position.line() + 1,
            ));
            Some(String::from_utf8_lossy(&msg).into())
        };
        let sgr_event = || -> Option<String> {
            let button_code = if is_held { 34 } else { 2 };
            Some(format!(
                "\u{1b}[<{:?};{:?};{:?}M",
                button_code,
                position.column() + 1,
                position.line() + 1
            ))
        };
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, MouseTracking::Normal) if !is_held => {
                utf8_event()
            },
            (
                MouseMode::NoEncoding | MouseMode::Utf8,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => utf8_event(),
            (
                MouseMode::Sgr,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => sgr_event(),
            (MouseMode::Sgr, MouseTracking::Normal) if !is_held => sgr_event(),
            _ => None,
        }
    }
    pub fn mouse_right_click_release_signal(&self, position: &Position) -> Option<String> {
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, _) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', b'#'];
                msg.append(&mut utf8_mouse_coordinates(
                    position.column() + 1,
                    position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (MouseMode::Sgr, _) => {
                let mouse_event = format!(
                    "\u{1b}[<2;{:?};{:?}m",
                    position.column() + 1,
                    position.line() + 1
                );
                Some(mouse_event)
            },
        }
    }
    pub fn mouse_middle_click_signal(&self, position: &Position, is_held: bool) -> Option<String> {
        let utf8_event = || -> Option<String> {
            let button_code = if is_held { b'A' } else { b'!' };
            let mut msg: Vec<u8> = vec![27, b'[', b'M', button_code];
            msg.append(&mut utf8_mouse_coordinates(
                position.column() + 1,
                position.line() + 1,
            ));
            Some(String::from_utf8_lossy(&msg).into())
        };
        let sgr_event = || -> Option<String> {
            let button_code = if is_held { 33 } else { 1 };
            Some(format!(
                "\u{1b}[<{:?};{:?};{:?}M",
                button_code,
                position.column() + 1,
                position.line() + 1
            ))
        };
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, MouseTracking::Normal) if !is_held => {
                utf8_event()
            },
            (
                MouseMode::NoEncoding | MouseMode::Utf8,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => utf8_event(),
            (
                MouseMode::Sgr,
                MouseTracking::ButtonEventTracking | MouseTracking::AnyEventTracking,
            ) => sgr_event(),
            (MouseMode::Sgr, MouseTracking::Normal) if !is_held => sgr_event(),
            _ => None,
        }
    }
    pub fn mouse_middle_click_release_signal(&self, position: &Position) -> Option<String> {
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, _) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', b'#'];
                msg.append(&mut utf8_mouse_coordinates(
                    position.column() + 1,
                    position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (MouseMode::Sgr, _) => {
                // TODO: these don't add a +1 because it's done outside, we should change it to
                // happen here for consistency
                let mouse_event = format!(
                    "\u{1b}[<1;{:?};{:?}m",
                    position.column() + 1,
                    position.line() + 1
                );
                Some(mouse_event)
            },
        }
    }
    pub fn mouse_scroll_up_signal(&self, position: &Position) -> Option<String> {
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, _) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', b'`'];
                msg.append(&mut utf8_mouse_coordinates(
                    position.column() + 1,
                    position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (MouseMode::Sgr, _) => {
                let mouse_event = format!(
                    "\u{1b}[<64;{:?};{:?}M",
                    position.column.0 + 1,
                    position.line.0 + 1
                );
                Some(mouse_event)
            },
        }
    }
    pub fn mouse_scroll_down_signal(&self, position: &Position) -> Option<String> {
        match (&self.mouse_mode, &self.mouse_tracking) {
            (_, MouseTracking::Off) => None,
            (MouseMode::NoEncoding | MouseMode::Utf8, _) => {
                let mut msg: Vec<u8> = vec![27, b'[', b'M', b'a'];
                msg.append(&mut utf8_mouse_coordinates(
                    position.column() + 1,
                    position.line() + 1,
                ));
                Some(String::from_utf8_lossy(&msg).into())
            },
            (MouseMode::Sgr, _) => {
                let mouse_event = format!(
                    "\u{1b}[<65;{:?};{:?}M",
                    position.column.0 + 1,
                    position.line.0 + 1
                );
                Some(mouse_event)
            },
        }
    }
    pub fn is_alternate_mode_active(&self) -> bool {
        self.alternate_screen_state.is_some()
    }
    pub fn focus_event(&self) -> Option<String> {
        if self.focus_event_tracking {
            Some("\u{1b}[I".into())
        } else {
            None
        }
    }
    pub fn unfocus_event(&self) -> Option<String> {
        if self.focus_event_tracking {
            Some("\u{1b}[O".into())
        } else {
            None
        }
    }
    pub fn delete_viewport_and_scroll(&mut self) {
        self.lines_above.clear();
        self.viewport.clear();
        self.lines_below.clear();
    }
    pub fn reset_cursor_position(&mut self) {
        self.cursor = Cursor::new(0, 0, self.styled_underlines);
    }
    pub fn lock_renders(&mut self) {
        self.lock_renders = true;
    }
    pub fn unlock_renders(&mut self) {
        self.lock_renders = false;
    }
    pub fn update_theme(&mut self, theme: Styling) {
        self.style.colors = theme.clone();
    }
    pub fn update_arrow_fonts(&mut self, should_support_arrow_fonts: bool) {
        self.arrow_fonts = should_support_arrow_fonts;
    }
    pub fn has_selection(&self) -> bool {
        !self.selection.is_empty()
    }
}

impl Perform for Grid {
    fn print(&mut self, c: char) {
        let c = self.cursor.charsets[self.active_charset].map(c);

        let terminal_character =
            TerminalCharacter::new_styled(c, self.cursor.pending_styles.clone());
        self.set_preceding_character(terminal_character.clone());
        self.add_character(terminal_character);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            7 => {
                self.ring_bell = true;
            },
            8 => {
                // backspace
                self.move_cursor_back(1);
            },
            9 => {
                // tab
                self.advance_to_next_tabstop(self.cursor.pending_styles.clone());
            },
            10 | 11 | 12 => {
                // 0a, newline
                // 0b, vertical tabulation
                // 0c, form feed
                self.add_newline();
            },
            13 => {
                // 0d, carriage return
                self.move_cursor_to_beginning_of_line();
            },
            14 => {
                self.set_active_charset(CharsetIndex::G1);
            },
            15 => {
                self.set_active_charset(CharsetIndex::G0);
            },
            _ => {
                if self.debug {
                    log::warn!("Unhandled execute: {:?}", byte);
                }
            },
        }
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, c: char) {
        if c == 'q' {
            // we only process sixel images if we know the pixel size of each character cell,
            // otherwise we can't reliably display them
            if self.current_cursor_pixel_coordinates().is_some() {
                let max_sixel_height_in_pixels = if self.sixel_scrolling {
                    let character_cell_height = self.character_cell_size.borrow().unwrap().height; // unwrap here is safe because `current_cursor_pixel_coordinates` above is only Some if it exists
                    Some(self.height * character_cell_height)
                } else {
                    None
                };
                self.sixel_grid.start_image(
                    max_sixel_height_in_pixels,
                    intermediates.iter().collect(),
                    params.iter().collect(),
                );
            }
        } else if c == 'z' {
            // UI-component (Zellij internal)
            self.ui_component_bytes = Some(vec![]);
        }
    }

    fn put(&mut self, byte: u8) {
        if self.sixel_grid.is_parsing() {
            self.sixel_grid.handle_byte(byte);
            // we explicitly set this to false here because in the context of Sixel, we only render the
            // image when it's done, i.e. in the unhook method
            self.should_render = false;
        } else if let Some(ui_component_bytes) = self.ui_component_bytes.as_mut() {
            ui_component_bytes.push(byte);
        }
    }

    fn unhook(&mut self) {
        if self.sixel_grid.is_parsing() {
            self.create_sixel_image();
        } else if let Some(mut ui_component_bytes) = self.ui_component_bytes.take() {
            let component_bytes = ui_component_bytes.drain(..);
            let style = self.style.clone();
            let arrow_fonts = self.arrow_fonts;
            UiComponentParser::new(self, style, arrow_fonts)
                .parse(component_bytes.collect())
                .non_fatal();
        }
        self.mark_for_rerender();
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };

        if params.is_empty() || params[0].is_empty() {
            return;
        }

        match params[0] {
            // Set window title.
            b"0" | b"2" => {
                if params.len() >= 2 {
                    let title = params[1..]
                        .iter()
                        .flat_map(|x| str::from_utf8(x))
                        .collect::<Vec<&str>>()
                        .join(";")
                        .trim()
                        .to_owned();
                    self.set_title(title);
                }
            },

            // Set color index.
            b"4" => {
                for chunk in params[1..].chunks(2) {
                    let index = chunk.get(0).and_then(|index| parse_number(index));
                    let color = chunk.get(1).and_then(|color| xparse_color(color));
                    if let (Some(i), Some(c)) = (index, color) {
                        if self.changed_colors.is_none() {
                            self.changed_colors = Some([None; 256]);
                        }
                        self.changed_colors.as_mut().unwrap()[i as usize] = Some(c);
                        return;
                    } else if chunk.get(1).as_ref().and_then(|c| c.get(0)) == Some(&b'?') {
                        if let Some(index) = index {
                            let terminal_emulator_color_codes =
                                self.terminal_emulator_color_codes.borrow();
                            let color = terminal_emulator_color_codes.get(&(index as usize));
                            if let Some(color) = color {
                                let color_response_message =
                                    format!("\u{1b}]4;{};{}{}", index, color, terminator);
                                self.pending_messages_to_pty
                                    .push(color_response_message.as_bytes().to_vec());
                            }
                        }
                    }
                }
            },

            // define hyperlink
            b"8" => {
                if params.len() < 3 {
                    return;
                }
                self.cursor.pending_styles.update(|styles| {
                    styles.link_anchor = self.link_handler.borrow_mut().dispatch_osc8(params)
                })
            },

            // Get/set Foreground (b"10") or background (b"11") colors
            b"10" | b"11" => {
                if params.len() >= 2 {
                    if let Some(mut dynamic_code) = parse_number(params[0]) {
                        for param in &params[1..] {
                            // currently only getting the color sequence is supported,
                            // setting still isn't
                            if param == b"?" {
                                let saved_terminal_color = if dynamic_code == 10 {
                                    Some(self.terminal_emulator_colors.borrow().fg)
                                } else if dynamic_code == 11 {
                                    Some(self.terminal_emulator_colors.borrow().bg)
                                } else {
                                    None
                                };
                                let color_response_message = match saved_terminal_color {
                                    Some(PaletteColor::Rgb((r, g, b))) => {
                                        format!(
                                            "\u{1b}]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
                                            // dynamic_code, color.r, color.g, color.b, terminator
                                            dynamic_code, r, g, b, terminator
                                        )
                                    },
                                    _ => {
                                        format!(
                                            "\u{1b}]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
                                            // dynamic_code, color.r, color.g, color.b, terminator
                                            dynamic_code, 0, 0, 0, terminator
                                        )
                                    },
                                };
                                self.pending_messages_to_pty
                                    .push(color_response_message.as_bytes().to_vec());
                            }
                            dynamic_code += 1;
                        }
                    }
                }
            },

            b"12" => {
                // get/set cursor color currently unimplemented
            },

            // Set cursor style.
            b"50" => {
                if params.len() >= 2
                    && params[1].len() >= 13
                    && params[1][0..12] == *b"CursorShape="
                {
                    let shape = match params[1][12] as char {
                        '0' => Some(CursorShape::Block),
                        '1' => Some(CursorShape::Beam),
                        '2' => Some(CursorShape::Underline),
                        _ => None,
                    };
                    if let Some(cursor_shape) = shape {
                        self.cursor.change_shape(cursor_shape);
                    }
                }
            },

            // Set clipboard.
            b"52" => {
                if params.len() < 3 {
                    return;
                }

                let _clipboard = params[1].get(0).unwrap_or(&b'c');
                match params[2] {
                    b"?" => {
                        // TBD: paste from own clipboard - currently unsupported
                    },
                    base64 => {
                        if let Ok(bytes) = base64::decode(base64) {
                            if let Ok(string) = String::from_utf8(bytes) {
                                self.pending_clipboard_update = Some(string);
                            }
                        };
                    },
                }
            },

            // Reset color index.
            b"104" => {
                // Reset all color indexes when no parameters are given.
                if params.len() == 1 {
                    self.changed_colors = None;
                    return;
                }

                // Reset color indexes given as parameters.
                for param in &params[1..] {
                    if let Some(index) = parse_number(param) {
                        if self.changed_colors.is_some() {
                            self.changed_colors.as_mut().unwrap()[index as usize] = None
                        }
                    }
                }

                // Reset all color indexes when no parameters are given.
                if params.len() == 1 {
                    // TBD - reset all color changes - currently unsupported
                    return;
                }

                // Reset color indexes given as parameters.
                for param in &params[1..] {
                    if let Some(_index) = parse_number(param) {
                        // TBD - reset color index - currently unimplemented
                    }
                }
            },

            // Reset foreground color.
            b"110" => {
                // TBD - reset foreground color - currently unimplemented
            },

            // Reset background color.
            b"111" => {
                // TBD - reset background color - currently unimplemented
            },

            // Reset text cursor color.
            b"112" => {
                // TBD - reset text cursor color - currently unimplemented
            },

            _ => {
                if self.debug {
                    log::warn!("Unhandled osc: {:?}", params);
                }
            },
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, c: char) {
        let mut params_iter = params.iter();
        let mut next_param_or = |default: u16| {
            params_iter
                .next()
                .map(|param| param[0])
                .filter(|&param| param != 0)
                .unwrap_or(default) as usize
        };
        if c == 'm' {
            if intermediates.is_empty() {
                self.cursor
                    .pending_styles
                    .update(|styles| styles.add_style_from_ansi_params(&mut params_iter))
            }
        } else if c == 'C' || c == 'a' {
            // move cursor forward
            let move_by = next_param_or(1);
            self.move_cursor_forward_until_edge(move_by);
        } else if c == 'K' {
            // clear line (0 => right, 1 => left, 2 => all)
            if let Some(clear_type) = params_iter.next().map(|param| param[0]) {
                let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
                if let Some(background_color) = self.cursor.pending_styles.background {
                    char_to_replace
                        .styles
                        .update(|styles| styles.background = Some(background_color));
                }
                if clear_type == 0 {
                    self.replace_characters_in_line_after_cursor(char_to_replace);
                } else if clear_type == 1 {
                    self.replace_characters_in_line_before_cursor(char_to_replace);
                } else if clear_type == 2 {
                    self.clear_cursor_line();
                }
            };
        } else if c == 'J' {
            // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
            let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
            if let Some(background_color) = self.cursor.pending_styles.background {
                char_to_replace
                    .styles
                    .update(|styles| styles.background = Some(background_color));
            }
            if let Some(clear_type) = params_iter.next().map(|param| param[0]) {
                if clear_type == 0 {
                    self.clear_all_after_cursor(char_to_replace);
                } else if clear_type == 1 {
                    self.clear_all_before_cursor(char_to_replace);
                } else if clear_type == 2 {
                    self.set_scroll_region_to_viewport_size();
                    self.fill_viewport(char_to_replace);
                    if let Some(images_to_reap) = self.sixel_grid.clear() {
                        self.sixel_grid.reap_images(images_to_reap);
                    }
                } else if clear_type == 3 {
                    self.clear_lines_above();
                    if let Some(images_to_reap) = self.sixel_grid.clear() {
                        self.sixel_grid.reap_images(images_to_reap);
                    }
                }
            };
        } else if c == 'H' || c == 'f' {
            // goto row/col
            // we subtract 1 from the row/column because these are 1 indexed
            let row = next_param_or(1).saturating_sub(1);
            let col = next_param_or(1).saturating_sub(1);
            self.move_cursor_to(col, row, EMPTY_TERMINAL_CHARACTER);
        } else if c == 'A' {
            // move cursor up until edge of screen
            let move_up_count = next_param_or(1);
            self.move_cursor_up(move_up_count as usize);
        } else if c == 'B' || c == 'e' {
            // move cursor down until edge of screen
            let move_down_count = next_param_or(1);
            let pad_character = EMPTY_TERMINAL_CHARACTER;
            self.move_cursor_down_until_edge_of_screen(move_down_count as usize, pad_character);
        } else if c == 'D' {
            let move_back_count = next_param_or(1);
            self.move_cursor_back(move_back_count);
        } else if c == 'l' {
            let first_intermediate_is_questionmark = match intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                for param in params_iter.map(|param| param[0]) {
                    match param {
                        2026 => {
                            self.unlock_renders();
                        },
                        2004 => {
                            self.bracketed_paste_mode = false;
                        },
                        1049 => {
                            if let Some(mut alternate_screen_state) =
                                self.alternate_screen_state.take()
                            {
                                if let Some(image_ids_to_reap) = self.sixel_grid.clear() {
                                    // reap images before dropping the alternate_screen_state contents
                                    // - we can't implement a drop method for this because the store is
                                    // outside of the alternate_screen_state struct
                                    self.sixel_grid.reap_images(image_ids_to_reap);
                                }
                                alternate_screen_state.apply_contents_to(
                                    &mut self.lines_above,
                                    &mut self.viewport,
                                    &mut self.cursor,
                                    &mut self.sixel_grid,
                                    &mut self.supports_kitty_keyboard_protocol,
                                );
                            }
                            self.alternate_screen_state = None;
                            self.clear_viewport_before_rendering = true;
                            self.force_change_size(self.height, self.width); // the alternative_viewport might have been of a different size...
                            self.mark_for_rerender();
                        },
                        25 => {
                            self.hide_cursor();
                            self.mark_for_rerender();
                        },
                        1 => {
                            self.cursor_key_mode = false;
                        },
                        3 => {
                            // DECCOLM - only side effects
                            self.set_scroll_region_to_viewport_size();
                            self.clear_all(EMPTY_TERMINAL_CHARACTER);
                            self.cursor.x = 0;
                            self.cursor.y = 0;
                        },
                        6 => {
                            self.erasure_mode = false;
                        },
                        7 => {
                            self.disable_linewrap = true;
                        },
                        80 => {
                            self.sixel_scrolling = false;
                        },
                        1000 => {
                            self.mouse_tracking = MouseTracking::Off;
                        },
                        1002 => {
                            self.mouse_tracking = MouseTracking::Off;
                        },
                        1003 => {
                            self.mouse_tracking = MouseTracking::Off;
                        },
                        1004 => {
                            self.focus_event_tracking = false;
                        },
                        1005 => {
                            self.mouse_mode = MouseMode::NoEncoding;
                        },
                        1006 => {
                            self.mouse_mode = MouseMode::NoEncoding;
                        },
                        _ => {},
                    };
                }
            } else {
                for param in params_iter.map(|param| param[0]) {
                    match param {
                        4 => {
                            self.insert_mode = false;
                        },
                        20 => {
                            self.new_line_mode = false;
                        },
                        _ => {},
                    }
                }
            }
        } else if c == 'h' {
            let first_intermediate_is_questionmark = match intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                for param in params_iter.map(|param| param[0]) {
                    match param {
                        25 => {
                            self.show_cursor();
                            self.mark_for_rerender();
                        },
                        2026 => {
                            self.lock_renders();
                        },
                        2004 => {
                            self.bracketed_paste_mode = true;
                        },
                        1049 => {
                            // enter alternate buffer
                            let current_lines_above =
                                std::mem::replace(&mut self.lines_above, VecDeque::new());
                            let current_viewport =
                                std::mem::replace(&mut self.viewport, vec![Row::new().canonical()]);
                            let current_cursor = std::mem::replace(
                                &mut self.cursor,
                                Cursor::new(0, 0, self.styled_underlines),
                            );
                            let current_supports_kitty_keyboard_protocol = std::mem::replace(
                                &mut self.supports_kitty_keyboard_protocol,
                                false,
                            );
                            let sixel_image_store = self.sixel_grid.sixel_image_store.clone();
                            let alternate_sixelgrid = std::mem::replace(
                                &mut self.sixel_grid,
                                SixelGrid::new(self.character_cell_size.clone(), sixel_image_store),
                            );
                            self.alternate_screen_state = Some(AlternateScreenState::new(
                                current_lines_above,
                                current_viewport,
                                current_cursor,
                                alternate_sixelgrid,
                                current_supports_kitty_keyboard_protocol,
                            ));
                            self.clear_viewport_before_rendering = true;
                            self.scrollback_buffer_lines =
                                self.recalculate_scrollback_buffer_count();
                            self.output_buffer.update_all_lines(); // make sure the screen gets cleared in the next render
                        },
                        1 => {
                            self.cursor_key_mode = true;
                        },
                        3 => {
                            // DECCOLM - only side effects
                            self.set_scroll_region_to_viewport_size();
                            self.clear_all(EMPTY_TERMINAL_CHARACTER);
                            self.cursor.x = 0;
                            self.cursor.y = 0;
                        },
                        6 => {
                            self.erasure_mode = true;
                        },
                        7 => {
                            self.disable_linewrap = false;
                        },
                        80 => {
                            self.sixel_scrolling = true;
                        },
                        1000 => {
                            self.mouse_tracking = MouseTracking::Normal;
                        },
                        1002 => {
                            self.mouse_tracking = MouseTracking::ButtonEventTracking;
                        },
                        1003 => {
                            self.mouse_tracking = MouseTracking::AnyEventTracking;
                        },
                        1004 => {
                            self.focus_event_tracking = true;
                        },
                        1005 => {
                            self.mouse_mode = MouseMode::Utf8;
                        },
                        1006 => {
                            self.mouse_mode = MouseMode::Sgr;
                        },
                        _ => {},
                    }
                }
            } else {
                for param in params_iter.map(|param| param[0]) {
                    match param {
                        4 => {
                            self.insert_mode = true;
                        },
                        20 => {
                            self.new_line_mode = true;
                        },
                        _ => {},
                    }
                }
            }
        } else if c == 'p' {
            let first_intermediate_is_questionmark = match intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                for param in params_iter.map(|param| param[0]) {
                    match param {
                        2026 => {
                            let response = "\u{1b}[?2026;2$y";
                            self.pending_messages_to_pty
                                .push(response.as_bytes().to_vec());
                        },
                        _ => {},
                    }
                }
            }
        } else if c == 'r' {
            if params.len() > 1 {
                let top = (next_param_or(1) as usize).saturating_sub(1);
                let bottom = params_iter
                    .next()
                    .map(|param| param[0] as usize)
                    .filter(|&param| param != 0)
                    .map(|bottom| {
                        std::cmp::min(self.height.saturating_sub(1), bottom.saturating_sub(1))
                    });
                self.set_scroll_region(top, bottom);
                if self.erasure_mode {
                    self.move_cursor_to_line(top, EMPTY_TERMINAL_CHARACTER);
                    self.move_cursor_to_beginning_of_line();
                }
            } else {
                self.set_scroll_region_to_viewport_size();
            }
        } else if c == 'M' {
            // delete lines if currently inside scroll region, or otherwise
            // delete lines in the entire viewport
            let line_count_to_delete = next_param_or(1);
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.cursor.pending_styles.clone();
            self.delete_lines_in_scroll_region(line_count_to_delete, pad_character);
        } else if c == 'L' {
            // insert blank lines if inside scroll region, or otherwise insert
            // blank lines in the entire viewport
            let line_count_to_add = next_param_or(1);
            let mut pad_character = EMPTY_TERMINAL_CHARACTER;
            pad_character.styles = self.cursor.pending_styles.clone();
            self.add_empty_lines_in_scroll_region(line_count_to_add, pad_character);
        } else if c == 'G' || c == '`' {
            let column = next_param_or(1).saturating_sub(1);
            let column = std::cmp::min(column, self.width.saturating_sub(1));
            self.move_cursor_to_column(column);
        } else if c == 'g' {
            let clear_type = next_param_or(0);
            if clear_type == 0 {
                self.clear_tabstop(self.cursor.x);
            } else if clear_type == 3 {
                self.clear_all_tabstops();
            }
        } else if c == 'd' {
            // goto line
            let line = next_param_or(1).saturating_sub(1);
            let pad_character = EMPTY_TERMINAL_CHARACTER;
            self.move_cursor_to_line(line, pad_character);
        } else if c == 'P' {
            // erase characters
            let count = next_param_or(1);
            self.erase_characters(count, self.cursor.pending_styles.clone());
        } else if c == 'X' {
            // erase characters and replace with empty characters of current style
            let count = next_param_or(1);
            self.replace_with_empty_chars(count, self.cursor.pending_styles.clone());
        } else if c == 'T' {
            /*
             * 124  54  T   SD
             * Scroll down, new lines inserted at top of screen
             * [4T = Scroll down 4, bring previous lines back into view
             */
            let line_count = next_param_or(1);
            self.rotate_scroll_region_up(line_count as usize);
        } else if c == 'S' {
            let first_intermediate_is_questionmark = match intermediates.get(0) {
                Some(b'?') => true,
                None => false,
                _ => false,
            };
            if first_intermediate_is_questionmark {
                let query_type = params_iter.next();
                let is_query = matches!(params_iter.next(), Some(&[1]));
                if is_query {
                    // XTSMGRAPHICS
                    match query_type {
                        Some(&[1]) => {
                            // number of color registers
                            let response = "\u{1b}[?1;0;65536S";
                            self.pending_messages_to_pty
                                .push(response.as_bytes().to_vec());
                        },
                        Some(&[2]) => {
                            // Sixel graphics geometry in pixels
                            if let Some(character_cell_size) = *self.character_cell_size.borrow() {
                                let sixel_area_geometry = format!(
                                    "\u{1b}[?2;0;{};{}S",
                                    character_cell_size.width * self.width,
                                    character_cell_size.height * self.height,
                                );
                                self.pending_messages_to_pty
                                    .push(sixel_area_geometry.as_bytes().to_vec());
                            }
                        },
                        _ => {
                            // unsupported (eg. ReGIS graphics geometry)
                        },
                    }
                }
            } else {
                // move scroll up
                let count = next_param_or(1);
                self.rotate_scroll_region_down(count);
            }
        } else if c == 's' {
            self.save_cursor_position();
        } else if c == 'u' && intermediates == &[b'>'] {
            // Zellij only supports the first "progressive enhancement" layer of the kitty keyboard
            // protocol
            if !self.explicitly_disable_kitty_keyboard_protocol {
                self.supports_kitty_keyboard_protocol = true;
            }
        } else if c == 'u' && intermediates == &[b'<'] {
            // Zellij only supports the first "progressive enhancement" layer of the kitty keyboard
            // protocol
            if !self.explicitly_disable_kitty_keyboard_protocol {
                self.supports_kitty_keyboard_protocol = false;
            }
        } else if c == 'u' && intermediates == &[b'?'] {
            // Zellij only supports the first "progressive enhancement" layer of the kitty keyboard
            // protocol
            let reply = if self.supports_kitty_keyboard_protocol {
                "\u{1b}[?1u"
            } else {
                "\u{1b}[?0u"
            };
            self.pending_messages_to_pty.push(reply.as_bytes().to_vec());
        } else if c == 'u' && intermediates == &[b'='] {
            // kitty keyboard protocol without the stack, just setting.
            // 0 disables, everything else enables.
            let count = next_param_or(0);
            if !self.explicitly_disable_kitty_keyboard_protocol {
                if count > 0 {
                    self.supports_kitty_keyboard_protocol = true;
                } else {
                    self.supports_kitty_keyboard_protocol = false;
                }
            }
        } else if c == 'u' {
            self.restore_cursor_position();
        } else if c == '@' {
            let count = next_param_or(1);
            for _ in 0..count {
                let mut pad_character = EMPTY_TERMINAL_CHARACTER;
                pad_character.styles = self.cursor.pending_styles.clone();
                self.add_character_at_cursor_position(pad_character, true);
            }
        } else if c == 'b' {
            if let Some(c) = self.preceding_char.clone() {
                for _ in 0..next_param_or(1) {
                    self.add_character(c.clone());
                }
            }
        } else if c == 'E' {
            // Moves cursor to beginning of the line n (default 1) lines down.
            let count = next_param_or(1);
            let pad_character = EMPTY_TERMINAL_CHARACTER;
            self.move_cursor_down_until_edge_of_screen(count, pad_character);
            self.move_cursor_to_beginning_of_line();
        } else if c == 'F' {
            // Moves cursor to beginning of the line n (default 1) lines up.
            let count = next_param_or(1);
            self.move_cursor_up(count);
            self.move_cursor_to_beginning_of_line();
        } else if c == 'I' {
            for _ in 0..next_param_or(1) {
                self.advance_to_next_tabstop(self.cursor.pending_styles.clone());
            }
        } else if c == 'q' {
            let first_intermediate_is_space = matches!(intermediates.get(0), Some(b' '));
            if first_intermediate_is_space {
                // DECSCUSR (CSI Ps SP q) -- Set Cursor Style.
                let cursor_style_id = next_param_or(0);
                let shape = match cursor_style_id {
                    0 => Some(CursorShape::Initial),
                    2 => Some(CursorShape::Block),
                    1 => Some(CursorShape::BlinkingBlock),
                    3 => Some(CursorShape::BlinkingUnderline),
                    4 => Some(CursorShape::Underline),
                    5 => Some(CursorShape::BlinkingBeam),
                    6 => Some(CursorShape::Beam),
                    _ => None,
                };
                if let Some(cursor_shape) = shape {
                    self.cursor.change_shape(cursor_shape);
                }
            } else if matches!(intermediates.get(0), Some(b'>')) {
                let version = version_number(VERSION);
                let xtversion = format!("\u{1b}P>|Zellij({})\u{1b}\\", version);
                self.pending_messages_to_pty
                    .push(xtversion.as_bytes().to_vec());
            }
        } else if c == 'Z' {
            for _ in 0..next_param_or(1) {
                self.move_to_previous_tabstop();
            }
        } else if c == 'c' {
            // identify terminal
            // https://vt100.net/docs/vt510-rm/DA1.html
            match intermediates.get(0) {
                None | Some(0) => {
                    // primary device attributes - VT220 with sixel
                    let terminal_capabilities = "\u{1b}[?62;4c";
                    self.pending_messages_to_pty
                        .push(terminal_capabilities.as_bytes().to_vec());
                },
                Some(b'>') => {
                    // secondary device attributes
                    let version = version_number(VERSION);
                    let text = format!("\u{1b}[>0;{};1c", version);
                    self.pending_messages_to_pty.push(text.as_bytes().to_vec());
                },
                _ => {},
            }
        } else if c == 'n' {
            // DSR - device status report
            // https://vt100.net/docs/vt510-rm/DSR.html
            match next_param_or(0) {
                5 => {
                    // report terminal status
                    let all_good = "\u{1b}[0n";
                    self.pending_messages_to_pty
                        .push(all_good.as_bytes().to_vec());
                },
                6 => {
                    // CPR - cursor position report

                    // Note that this is relative to scrolling region.
                    let offset = self.scroll_region.0; // scroll_region_top
                    let position_report = format!(
                        "\u{1b}[{};{}R",
                        self.cursor.y + 1 - offset,
                        self.cursor.x + 1
                    );
                    self.pending_messages_to_pty
                        .push(position_report.as_bytes().to_vec());
                },
                _ => {},
            }
        } else if c == 'x' {
            // DECREQTPARM - Request Terminal Parameters
            // https://vt100.net/docs/vt100-ug/chapter3.html#DECREQTPARM
            //
            // Respond with (same as xterm): Parity NONE, 8 bits,
            // xmitspeed 38400, recvspeed 38400.  (CLoCk MULtiplier =
            // 1, STP option flags = 0)
            //
            // (xterm used to respond to DECREQTPARM in all modes.
            // Now it seems to only do so when explicitly in VT100 mode.)
            let query = next_param_or(0);
            match query {
                0 | 1 => {
                    let response = format!("\u{1b}[{};1;1;128;128;1;0x", query + 2);
                    self.pending_messages_to_pty
                        .push(response.as_bytes().to_vec());
                },
                _ => {},
            }
        } else if c == 't' {
            match next_param_or(1) as usize {
                14 => {
                    if let Some(character_cell_size) = *self.character_cell_size.borrow() {
                        let text_area_pixel_size_report = format!(
                            "\x1b[4;{};{}t",
                            character_cell_size.height * self.height,
                            character_cell_size.width * self.width
                        );
                        self.pending_messages_to_pty
                            .push(text_area_pixel_size_report.as_bytes().to_vec());
                    }
                },
                16 => {
                    if let Some(character_cell_size) = *self.character_cell_size.borrow() {
                        let character_cell_size_report = format!(
                            "\x1b[6;{};{}t",
                            character_cell_size.height, character_cell_size.width
                        );
                        self.pending_messages_to_pty
                            .push(character_cell_size_report.as_bytes().to_vec());
                    }
                },
                18 => {
                    // report text area
                    let text_area_report = format!("\x1b[8;{};{}t", self.height, self.width);
                    self.pending_messages_to_pty
                        .push(text_area_report.as_bytes().to_vec());
                },
                22 => {
                    self.push_current_title_to_stack();
                },
                23 => {
                    self.pop_title_from_stack();
                },
                _ => {},
            }
        } else {
            if self.debug {
                log::warn!("Unhandled csi: {}->{:?}", c, params);
            }
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (byte, intermediates.get(0)) {
            (b'A', charset_index_symbol) => {
                let charset_index: CharsetIndex = match charset_index_symbol {
                    Some(b'(') => CharsetIndex::G0,
                    Some(b')') => CharsetIndex::G1,
                    Some(b'*') => CharsetIndex::G2,
                    Some(b'+') => CharsetIndex::G3,
                    _ => {
                        // invalid, silently do nothing
                        return;
                    },
                };
                self.configure_charset(StandardCharset::UK, charset_index);
            },
            (b'B', charset_index_symbol) => {
                let charset_index: CharsetIndex = match charset_index_symbol {
                    Some(b'(') => CharsetIndex::G0,
                    Some(b')') => CharsetIndex::G1,
                    Some(b'*') => CharsetIndex::G2,
                    Some(b'+') => CharsetIndex::G3,
                    _ => {
                        // invalid, silently do nothing
                        return;
                    },
                };
                self.configure_charset(StandardCharset::Ascii, charset_index);
            },
            (b'0', charset_index_symbol) => {
                let charset_index: CharsetIndex = match charset_index_symbol {
                    Some(b'(') => CharsetIndex::G0,
                    Some(b')') => CharsetIndex::G1,
                    Some(b'*') => CharsetIndex::G2,
                    Some(b'+') => CharsetIndex::G3,
                    _ => {
                        // invalid, silently do nothing
                        return;
                    },
                };
                self.configure_charset(
                    StandardCharset::SpecialCharacterAndLineDrawing,
                    charset_index,
                );
            },
            (b'D', None) => {
                self.add_newline();
            },
            (b'E', None) => {
                self.add_newline();
                self.move_cursor_to_beginning_of_line();
            },
            (b'M', None) => {
                // TODO: if cursor is at the top, it should go down one
                self.move_cursor_up_with_scrolling(1);
            },
            (b'c', None) => {
                self.reset_terminal_state();
            },
            (b'H', None) => {
                self.set_horizontal_tabstop();
            },
            (b'7', None) => {
                self.save_cursor_position();
            },
            (b'Z', None) => {
                let terminal_capabilities = "\u{1b}[?6c";
                self.pending_messages_to_pty
                    .push(terminal_capabilities.as_bytes().to_vec());
            },
            (b'8', None) => {
                self.restore_cursor_position();
            },
            (b'8', Some(b'#')) => {
                let mut fill_character = EMPTY_TERMINAL_CHARACTER;
                fill_character.character = 'E';
                self.fill_viewport(fill_character);
            },
            _ => {
                if self.debug {
                    log::warn!("Unhandled esc_dispatch: {}->{:?}", byte, intermediates);
                }
            },
        }
    }
}

#[derive(Clone)]
pub struct AlternateScreenState {
    lines_above: VecDeque<Row>,
    viewport: Vec<Row>,
    cursor: Cursor,
    sixel_grid: SixelGrid,
    supports_kitty_keyboard_protocol: bool,
}
impl AlternateScreenState {
    pub fn new(
        lines_above: VecDeque<Row>,
        viewport: Vec<Row>,
        cursor: Cursor,
        sixel_grid: SixelGrid,
        supports_kitty_keyboard_protocol: bool,
    ) -> Self {
        AlternateScreenState {
            lines_above,
            viewport,
            cursor,
            sixel_grid,
            supports_kitty_keyboard_protocol,
        }
    }
    pub fn apply_contents_to(
        &mut self,
        lines_above: &mut VecDeque<Row>,
        viewport: &mut Vec<Row>,
        cursor: &mut Cursor,
        sixel_grid: &mut SixelGrid,
        supports_kitty_keyboard_protocol: &mut bool,
    ) {
        std::mem::swap(&mut self.lines_above, lines_above);
        std::mem::swap(&mut self.viewport, viewport);
        std::mem::swap(&mut self.cursor, cursor);
        std::mem::swap(&mut self.sixel_grid, sixel_grid);
        std::mem::swap(
            &mut self.supports_kitty_keyboard_protocol,
            supports_kitty_keyboard_protocol,
        );
    }
}

#[derive(Clone)]
pub struct Row {
    pub columns: VecDeque<TerminalCharacter>,
    pub is_canonical: bool,
    width: Option<usize>,
}

impl Debug for Row {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for character in &self.columns {
            write!(f, "{:?}", character)?;
        }
        Ok(())
    }
}

impl Row {
    pub fn new() -> Self {
        Row {
            columns: VecDeque::new(),
            is_canonical: false,
            width: None,
        }
    }
    pub fn from_columns(columns: VecDeque<TerminalCharacter>) -> Self {
        Row {
            columns,
            is_canonical: false,
            width: None,
        }
    }
    pub fn from_rows(mut rows: Vec<Row>) -> Self {
        if rows.is_empty() {
            Row::new()
        } else {
            let mut first_row = rows.remove(0);
            for row in &mut rows {
                first_row.append(&mut row.columns);
            }
            first_row
        }
    }
    pub fn with_character(mut self, terminal_character: TerminalCharacter) -> Self {
        self.columns.push_back(terminal_character);
        self.width = None;
        self
    }
    pub fn canonical(mut self) -> Self {
        self.is_canonical = true;
        self
    }
    pub fn width_cached(&mut self) -> usize {
        if self.width.is_some() {
            self.width.unwrap()
        } else {
            let mut width = 0;
            for terminal_character in &self.columns {
                width += terminal_character.width();
            }
            self.width = Some(width);
            width
        }
    }
    pub fn width(&self) -> usize {
        let mut width = 0;
        for terminal_character in &self.columns {
            width += terminal_character.width();
        }
        width
    }
    pub fn excess_width(&self) -> usize {
        let mut acc = 0;
        for terminal_character in &self.columns {
            if terminal_character.width() > 1 {
                acc += terminal_character.width() - 1;
            }
        }
        acc
    }
    pub fn excess_width_until(&self, x: usize) -> usize {
        let mut acc = 0;
        for terminal_character in self.columns.iter().take(x) {
            if terminal_character.width() > 1 {
                acc += terminal_character.width() - 1;
            }
        }
        acc
    }
    pub fn absolute_character_index(&self, x: usize) -> usize {
        // return x's width aware index
        let mut absolute_index = x;
        for (i, terminal_character) in self.columns.iter().enumerate().take(x) {
            if i == absolute_index {
                break;
            }
            if terminal_character.width() > 1 {
                absolute_index = absolute_index.saturating_sub(1);
            }
        }
        absolute_index
    }
    pub fn absolute_character_index_and_position_in_char(&self, x: usize) -> (usize, usize) {
        // returns x's width aware index as well as its position inside the wide char (eg. 1 if
        // it's in the middle of a 2-char wide character)
        let mut accumulated_width = 0;
        let mut absolute_index = x;
        let mut position_inside_character = 0;
        for (i, terminal_character) in self.columns.iter().enumerate() {
            accumulated_width += terminal_character.width();
            absolute_index = i;
            if accumulated_width > x {
                let character_start_position = accumulated_width - terminal_character.width();
                position_inside_character = x - character_start_position;
                break;
            }
        }
        (absolute_index, position_inside_character)
    }
    pub fn add_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        match self.width_cached().cmp(&x) {
            Ordering::Equal => {
                // this is unwrapped because this always happens after self.width_cached()
                *self.width.as_mut().unwrap() += terminal_character.width();
                // adding the character at the end of the current line
                self.columns.push_back(terminal_character);
            },
            Ordering::Less => {
                // adding the character after the end of the current line
                // we pad the line up to the character and then add it
                let width_offset = self.excess_width_until(x);
                self.columns
                    .resize(x.saturating_sub(width_offset), EMPTY_TERMINAL_CHARACTER);
                self.columns.push_back(terminal_character);
                self.width = None;
            },
            Ordering::Greater => {
                // adding the character in the middle of the line
                // we replace the character at its position
                let (absolute_x_index, position_inside_character) =
                    self.absolute_character_index_and_position_in_char(x);
                let character_width = terminal_character.width();
                let replaced_character =
                    std::mem::replace(&mut self.columns[absolute_x_index], terminal_character);
                match character_width.cmp(&replaced_character.width()) {
                    Ordering::Greater => {
                        // the replaced character is narrower than the current character
                        // (eg. we added a wide emoji in place of an English character)
                        // we remove the character after it to make room
                        let position_to_remove = absolute_x_index + 1;
                        if let Some(removed) = self.columns.remove(position_to_remove) {
                            if removed.width() > 1 {
                                // the character we removed is a wide character itself, so we add
                                // padding
                                self.columns
                                    .insert(position_to_remove, EMPTY_TERMINAL_CHARACTER);
                            }
                        }
                    },
                    Ordering::Less => {
                        // the replaced character is wider than the current character
                        // (eg. we added an English character in place of a wide emoji)
                        // we must make sure to add padding either before the character we added
                        // or after it, depending on our position inside said removed wide character
                        // TODO: support characters wider than 2
                        if position_inside_character > 0 {
                            self.columns
                                .insert(absolute_x_index, EMPTY_TERMINAL_CHARACTER);
                        } else {
                            self.columns
                                .insert(absolute_x_index + 1, EMPTY_TERMINAL_CHARACTER);
                        }
                    },
                    _ => {},
                }
                self.width = None;
            },
        }
    }
    pub fn insert_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        let insert_position = self.absolute_character_index(x);
        match self.columns.len().cmp(&insert_position) {
            Ordering::Equal => self.columns.push_back(terminal_character),
            Ordering::Less => {
                self.columns
                    .resize(insert_position, EMPTY_TERMINAL_CHARACTER);
                self.columns.push_back(terminal_character);
            },
            Ordering::Greater => {
                self.columns.insert(insert_position, terminal_character);
            },
        }
        self.width = None;
    }
    pub fn replace_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        let absolute_x_index = self.absolute_character_index(x);
        if let Some(character) = self.columns.get_mut(absolute_x_index) {
            let terminal_character_width = terminal_character.width();
            let character = std::mem::replace(character, terminal_character);
            let excess_width = character.width().saturating_sub(terminal_character_width);
            for _ in 0..excess_width {
                self.columns
                    .insert(absolute_x_index, EMPTY_TERMINAL_CHARACTER);
            }
        }
        self.width = None;
    }
    pub fn replace_columns(&mut self, columns: VecDeque<TerminalCharacter>) {
        self.columns = columns;
        self.width = None;
    }
    pub fn push(&mut self, terminal_character: TerminalCharacter) {
        self.columns.push_back(terminal_character);
        self.width = None;
    }
    pub fn truncate(&mut self, x: usize) {
        let width_offset = self.excess_width_until(x);
        let truncate_position = x.saturating_sub(width_offset);
        if truncate_position < self.columns.len() {
            self.columns.truncate(truncate_position);
        }
        self.width = None;
    }
    pub fn position_accounting_for_widechars(&self, x: usize) -> usize {
        let mut position = x;
        for (index, terminal_character) in self.columns.iter().enumerate() {
            if index == position {
                break;
            }
            if terminal_character.width() > 1 {
                position = position.saturating_sub(terminal_character.width().saturating_sub(1));
            }
        }
        position
    }
    pub fn replace_and_pad_end(
        &mut self,
        from: usize,
        to: usize,
        terminal_character: TerminalCharacter,
    ) {
        let from_position_accounting_for_widechars = self.position_accounting_for_widechars(from);
        let to_position_accounting_for_widechars = self.position_accounting_for_widechars(to);
        let replacement_length = to_position_accounting_for_widechars
            .saturating_sub(from_position_accounting_for_widechars);
        let mut replace_with = VecDeque::from(vec![terminal_character; replacement_length]);
        self.columns
            .truncate(from_position_accounting_for_widechars);
        self.columns.append(&mut replace_with);
        self.width = None;
    }
    pub fn append(&mut self, to_append: &mut VecDeque<TerminalCharacter>) {
        self.columns.append(to_append);
        self.width = None;
    }
    pub fn drain_until(&mut self, x: usize) -> VecDeque<TerminalCharacter> {
        let mut drained_part_len = 0;
        let mut split_pos = 0;
        for next_character in self.columns.iter() {
            // drained_part_len == 0 here is so that if the grid is resized
            // to a size of 1, we won't drop wide characters
            if drained_part_len + next_character.width() <= x || drained_part_len == 0 {
                drained_part_len += next_character.width();
                split_pos += 1
            } else {
                break;
            }
        }
        // Can't use split_off because it doesn't reduce capacity, causing OOM with long lines
        let drained_part = self.columns.drain(..split_pos).collect();
        self.width = None;
        drained_part
    }
    pub fn replace_and_pad_beginning(&mut self, to: usize, terminal_character: TerminalCharacter) {
        let to_position_accounting_for_widechars = self.position_accounting_for_widechars(to);
        let width_of_current_character = self
            .columns
            .get(to_position_accounting_for_widechars)
            .map(|character| character.width())
            .unwrap_or(1);
        let mut replace_with =
            VecDeque::from(vec![terminal_character; to + width_of_current_character]);
        if to_position_accounting_for_widechars > self.columns.len() {
            self.columns.clear();
        } else if to_position_accounting_for_widechars >= self.columns.len() {
            drop(self.columns.drain(0..to_position_accounting_for_widechars));
        } else {
            drop(self.columns.drain(0..=to_position_accounting_for_widechars));
        }
        replace_with.append(&mut self.columns);
        self.width = None;
        self.columns = replace_with;
    }
    pub fn len(&self) -> usize {
        self.columns.len()
    }
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
    pub fn delete_and_return_character(&mut self, x: usize) -> Option<TerminalCharacter> {
        let erase_position = self.absolute_character_index(x);
        if erase_position < self.columns.len() {
            self.width = None;
            self.columns.remove(erase_position)
        } else {
            None
        }
    }
    pub fn split_to_rows_of_length(&mut self, max_row_length: usize) -> Vec<Row> {
        let mut parts: Vec<Row> = vec![];
        let mut current_part: VecDeque<TerminalCharacter> = VecDeque::new();
        let mut current_part_len = 0;
        for character in self.columns.drain(..) {
            if current_part_len + character.width() > max_row_length {
                parts.push(Row::from_columns(current_part));
                current_part = VecDeque::new();
                current_part_len = 0;
            }
            current_part_len += character.width();
            current_part.push_back(character);
        }
        if !current_part.is_empty() {
            parts.push(Row::from_columns(current_part))
        };
        if !parts.is_empty() && self.is_canonical {
            if let Some(part) = parts.get_mut(0) {
                part.is_canonical = true;
            }
        }
        if parts.is_empty() {
            parts.push(self.clone());
        }
        self.width = None;
        parts
    }
    pub fn last_index_in_line(&self) -> usize {
        self.columns.len()
    }
    pub fn word_indices_around_character_index(&self, index: usize) -> Option<(usize, usize)> {
        let absolute_character_index = self.absolute_character_index(index);
        let character_at_index = self.columns.get(absolute_character_index)?;
        if is_selection_boundary_character(character_at_index.character) {
            return Some((index, index + 1));
        }
        let mut end_position = self
            .columns
            .iter()
            .enumerate()
            .skip(absolute_character_index)
            .find_map(|(i, t_c)| {
                if is_selection_boundary_character(t_c.character) {
                    Some(i + self.excess_width_until(i))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.columns.len() + self.excess_width());
        let start_position = self
            .columns
            .iter()
            .enumerate()
            .take(absolute_character_index)
            .rev()
            .find_map(|(i, t_c)| {
                if is_selection_boundary_character(t_c.character) {
                    Some(i + 1 + self.excess_width_until(i))
                } else {
                    None
                }
            })
            .unwrap_or(0);
        if start_position == end_position {
            // so that if this is only one character, it'll still be marked
            end_position += 1;
        }
        Some((start_position, end_position))
    }
    pub fn word_start_index_of_last_character(&self) -> usize {
        self.columns
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, t_c)| {
                if is_selection_boundary_character(t_c.character) {
                    Some(self.absolute_character_index(i + 1))
                } else {
                    None
                }
            })
            .unwrap_or(0)
    }
    pub fn word_end_index_of_first_character(&self) -> usize {
        self.columns
            .iter()
            .enumerate()
            .find_map(|(i, t_c)| {
                if is_selection_boundary_character(t_c.character) {
                    Some(self.absolute_character_index(i))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.columns.len())
    }
}

fn is_selection_boundary_character(character: char) -> bool {
    character.is_ascii_whitespace()
        || character == '['
        || character == ']'
        || character == '{'
        || character == '}'
        || character == '<'
        || character == '>'
        || character == '('
        || character == ')'
}

#[cfg(test)]
#[path = "./unit/grid_tests.rs"]
mod grid_tests;
