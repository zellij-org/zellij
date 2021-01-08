#![allow(clippy::clippy::if_same_then_else)]

use ::nix::pty::Winsize;
use ::std::os::unix::io::RawFd;
use ::std::collections::VecDeque;
use ::vte::Perform;
use std::fmt::{self, Debug, Formatter};

use crate::boundaries::Rect;
use crate::terminal_pane::terminal_character::{
    AnsiCode, CharacterStyles, NamedColor, TerminalCharacter,
    EMPTY_TERMINAL_CHARACTER
};
// use crate::terminal_pane::Scroll;
use crate::utils::logging::{debug_log_to_file, _debug_log_to_file_pid_3};
use crate::VteEvent;

#[derive(Clone, Default)]
pub struct ScrollBuffer {
    pub rows: Vec<Row>,
    pub pid: RawFd, // TODO: removeme
}

impl Debug for ScrollBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (i, row) in self.rows.iter().enumerate() {
            if row.is_canonical {
                writeln!(f, "{:?} (C): {:?}", i, row)?;
            } else {
                writeln!(f, "{:?} (W): {:?}", i, row)?;
            }
        }
        Ok(())
    }
}

impl ScrollBuffer {
    pub fn new(rows: usize, columns: usize, pid: RawFd) -> Self {
        let rows = vec![];
        ScrollBuffer {
            rows,
            pid,
        }
    }
    pub fn as_character_lines(&self, width: usize, height: usize) -> Vec<Vec<TerminalCharacter>> {
        let mut lines: Vec<Vec<TerminalCharacter>> = self.rows.iter().map(|r| {
            let mut line: Vec<TerminalCharacter> = r.columns.iter().copied().collect();
            for _ in line.len()..width {
                // pad line
                line.push(EMPTY_TERMINAL_CHARACTER);
            }
            line
        }).collect();
        let empty_row = vec![EMPTY_TERMINAL_CHARACTER; width];
        for _ in lines.len()..height {
            lines.push(empty_row.clone());
        }
        lines
    }
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }
    pub fn prepend(&mut self, row: Row) {
        self.rows.insert(0, row);
    }
    pub fn drain_top_rows_into(&mut self, other: &mut ScrollBuffer, count: usize) {
        // first, flatten all excess rows so that we will only append canonical lines
        let mut excess_rows = self.rows.drain(..count).fold(vec![], |mut excess_rows, mut row| {
            if excess_rows.is_empty() || row.is_canonical {
                excess_rows.push(row);
            } else  {
                excess_rows.last_mut().unwrap().append(&mut row.columns);
            }
            excess_rows
        });
        // second, if the first excess row is itself wrapped, we append it to the last
        // row of other above us
        // if other is empty and we have a line wrap, the state is corrupted somehow
        let first_excess_row_is_wrapped = excess_rows.get(0).map(|r| !r.is_canonical).unwrap_or(false);
        if first_excess_row_is_wrapped && other.rows.len() > 0 {
            let mut first_excess_row = excess_rows.remove(0);
            other.rows
                .last_mut()
                .expect("trying to append a wrapped line to an empty scrollbuffer")
                .append(&mut first_excess_row.columns);
        }
        other.rows.append(&mut excess_rows);
    }
    pub fn pull_bottom_rows_from(&mut self, other: &mut ScrollBuffer, max_height: usize, max_width: usize) {
        // TODO: CONTINUE HERE (06/01)
        // debug size change:
        // * cat testfile
        // * split vertically
        // * resize left a few times
        // * resize right a few times
        // * see lines wrapping wrongly
        // here we want to get rows from the bottom of other so that our height will be as close as
        // possible to max_height. We don't know how long the rows on other are, so we need to make
        // sure they are as close to but no longer than max_width
        // let number_of_rows_to_pull = max_height - self.row_count();
        let mut new_rows: VecDeque<Row> = VecDeque::from(self.rows.drain(..).collect::<Vec<Row>>());
        let mut next_lines: Vec<Row> = vec![];
        loop {
            if new_rows.len() == max_height {
                break;
            }
            match other.rows.pop() {
                Some(mut next_canonical_line) => {
                    let mut index_of_last_non_canonical_row = None;
                    for (i, row) in new_rows.iter().enumerate() {
                        if row.is_canonical {
                            break;
                        } else {
                            index_of_last_non_canonical_row = Some(i);
                        }
                    }
                    match index_of_last_non_canonical_row {
                        Some(index_of_last_non_canonical_row) => {
                            let top_non_canonical_rows = new_rows.drain(..=index_of_last_non_canonical_row);
                            next_lines.push(next_canonical_line);
                            next_lines.append(&mut top_non_canonical_rows.collect());
                            next_lines = Row::from_rows(next_lines).split_to_rows_of_length(max_width);
                        },
                        None => {
                            next_lines = next_canonical_line.split_to_rows_of_length(max_width);
                        }
                    }
                },
                None => break, // no more rows
            }
            let number_of_rows_left_to_add = max_height - new_rows.len();
            let rows_to_add = if number_of_rows_left_to_add >= next_lines.len() {
                next_lines.drain(..)
            } else {
                next_lines.drain(next_lines.len() - number_of_rows_left_to_add..)
            };
            for row in rows_to_add.rev() {
                new_rows.push_front(row);
            }
        }
        if !next_lines.is_empty() {
            let mut excess_row = Row::new().canonical();
            // let mut excess_row = Row::new();
            for mut row in next_lines.drain(..) {
                excess_row.append(&mut row.columns);
            }
            other.push_row(excess_row);
        }
        self.rows = Vec::from(new_rows);
        // debug_log_to_file(format!("after pull: {:?}", self));
    }
    pub fn drain_characters_from_bottom(&mut self, desired_width: usize) -> Option<Row> {
        // here we get a desired width for the row to return and do our best to return
        // a row of that width
        if self.rows.last().is_none() {
            // no luck, we have no rows, better luck next time
            return None
        };
        let last_row_length = self.rows.last().unwrap().len();
        if last_row_length > desired_width {
            // our last row has more characters than we were asked for,
            // so we remove them from the last row and return them as a new row
            let first_character_index = last_row_length - desired_width;
            let row_columns = self.rows.last_mut().unwrap().columns.drain(first_character_index..);
            return Some(Row::from_columns(row_columns.collect()));
        } else {
            // we were asked for either exactly as much characters as we have or more
            // so we will return the entire row
            //
            // we will not try to append the row above this to get more characters because
            // this method assumes all rows in self are canonical
            return self.rows.pop();
        }
    }
    pub fn push_row(&mut self, row: Row) {
        self.rows.push(row);
    }
    pub fn add_character_at(&mut self, terminal_character: TerminalCharacter, x: usize, y: usize) {
        match self.rows.get_mut(y) {
            Some(row) => row.add_character_at(terminal_character, x),
            None => {
                // self.rows.push(Row::new().with_character(terminal_character));
                self.rows.push(Row::new().with_character(terminal_character).canonical());
            }
        }
    }
    pub fn replace_character_at(&mut self, terminal_character: TerminalCharacter, x: usize, y: usize) {
        self.rows.get_mut(y).unwrap().replace_character_at(terminal_character, x);
    }
    pub fn truncate_line(&mut self, y: usize, column_index: usize) {
        self.rows.get_mut(y).unwrap().truncate(column_index);
    }
    pub fn append_to_line(&mut self, y: usize, mut to_append: Vec<TerminalCharacter>) {
        self.rows.get_mut(y).unwrap().append(&mut to_append);
    }
    pub fn replace_line_beginning_with(&mut self, y: usize, line_part: Vec<TerminalCharacter>) {
        let mut row = self.rows.get_mut(y).unwrap();
        row.replace_beginning_with(line_part);
    }
    pub fn truncate_rows(&mut self, y: usize) {
        self.rows.truncate(y);
    }
    pub fn clear_all(&mut self) {
        self.rows.clear();
    }
    pub fn pad_lines(&mut self, y: usize) {
        for _ in self.rows.len()..=y {
            self.rows.push(Row::new());
        }
    }
    pub fn pad_line(&mut self, y: usize, x: usize) {
        let mut row = self.rows.get_mut(y).unwrap();
        for _ in row.len()..x {
            row.push(EMPTY_TERMINAL_CHARACTER);
        }
    }
    pub fn remove_line(&mut self, y: usize) {
        self.rows.remove(y);
    }
    pub fn remove_and_return_line(&mut self, y: usize) -> Row {
        self.rows.remove(y)
    }
    pub fn insert_line(&mut self, index: usize, line: Row) {
        self.rows.insert(index, line);
    }
    pub fn delete_character(&mut self, y: usize, x: usize) {
        self.rows.get_mut(y).unwrap().delete_character(x);
    }
    pub fn pop_line(&mut self) -> Option<Row> {
        self.rows.pop()
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    lines_above: ScrollBuffer,
    viewport: ScrollBuffer,
    lines_below: ScrollBuffer,
    cursor: Cursor,
    scroll_region: Option<(usize, usize)>,
    width: usize,
    height: usize,
    pid: RawFd, // TODO: REMOVEME
}

impl Grid {
    pub fn new(rows: usize, columns: usize, pid: RawFd) -> Self {
        Grid {
            lines_above: ScrollBuffer::default(),
            viewport: ScrollBuffer::new(rows, columns, pid),
            lines_below: ScrollBuffer::default(),
            cursor: Cursor::new(0, 0),
            scroll_region: None,
            width: columns,
            height: rows,
            pid,
        }
    }
    fn cursor_canonical_line_index(&self) -> usize {
        let mut cursor_canonical_line_index = 0;
        let mut canonical_lines_traversed = 0;
        for (i, line) in self.viewport.rows.iter().enumerate() {
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
    // TODO: merge these two funtions
    fn cursor_index_in_canonical_line(&self) -> usize {
        let mut cursor_canonical_line_index = 0;
        let mut cursor_index_in_canonical_line = 0;
        for (i, line) in self.viewport.rows.iter().enumerate() {
            if line.is_canonical {
                cursor_canonical_line_index = i;
            }
            if i == self.cursor.y {
                let line_wrap_position_in_line = self.cursor.y - cursor_canonical_line_index;
                cursor_index_in_canonical_line = line_wrap_position_in_line + self.cursor.x;
                break;
            }
        }
        cursor_index_in_canonical_line
    }
    fn canonical_line_y_coordinates(&self, canonical_line_index: usize) -> usize {
        let mut canonical_lines_traversed = 0;
        let mut y_coordinates = 0;
        for (i, line) in self.viewport.rows.iter().enumerate() {
            if line.is_canonical {
                canonical_lines_traversed += 1;
                if canonical_lines_traversed == canonical_line_index + 1 {
                    y_coordinates = i;
                    break;
                }
            }
        }
        y_coordinates
    }
    pub fn scroll_up_one_line(&mut self) {
        if self.lines_above.row_count() > 0 && self.viewport.row_count() == self.height {
            let line_to_push_down = self.viewport.pop_line().unwrap();
            self.lines_below.prepend(line_to_push_down);
            let line_to_insert_at_viewport_top = self.lines_above.pop_line().unwrap();
            self.viewport.prepend(line_to_insert_at_viewport_top);
        }
    }
    pub fn scroll_down_one_line(&mut self) {
        if self.lines_below.row_count() > 0 && self.viewport.row_count() == self.height {
            let mut line_to_push_up = self.viewport.remove_and_return_line(0);
            if line_to_push_up.is_canonical {
                self.lines_above.push_row(line_to_push_up);
            } else {
                let mut last_line_above = self.lines_above.pop_line().unwrap();
                last_line_above.append(&mut line_to_push_up.columns);
                self.lines_above.push_row(last_line_above);
            }
            let line_to_insert_at_viewport_bottom = self.lines_below.remove_and_return_line(0);
            self.viewport.push_row(line_to_insert_at_viewport_bottom);
        }
    }
    pub fn change_size(&mut self, new_rows: usize, new_columns: usize) {
        if new_columns != self.width {
            let mut cursor_canonical_line_index = self.cursor_canonical_line_index();
            let cursor_index_in_canonical_line = self.cursor_index_in_canonical_line();
            let mut viewport_canonical_lines = vec![];
            for mut row in self.viewport.rows.drain(..) {
                if !row.is_canonical && viewport_canonical_lines.is_empty() && self.lines_above.row_count() > 0 {
                    let mut first_line_above = self.lines_above.pop_line().unwrap();
                    first_line_above.append(&mut row.columns);
                    viewport_canonical_lines.push(first_line_above);
                    cursor_canonical_line_index += 1;
                } else if row.is_canonical {
                    viewport_canonical_lines.push(row);
                } else {
                    viewport_canonical_lines.last_mut().unwrap().append(&mut row.columns);
                }
            }
            let mut new_viewport_rows = vec![];
            for mut canonical_line in viewport_canonical_lines {
                let mut canonical_line_parts: Vec<Row> = vec![];
                while canonical_line.columns.len() > 0 {
                    let next_wrap = if canonical_line.len() > new_columns {
                        canonical_line.columns.drain(..new_columns)
                    } else {
                        canonical_line.columns.drain(..)
                    };
                    let row = Row::from_columns(next_wrap.collect());
                    // if there are no more parts, this row is canonical as long as it originall
                    // was canonical (it might not have been for example if it's the first row in
                    // the viewport, and the actual canonical row is above it in the scrollback)
                    let row = if canonical_line_parts.len() == 0 && canonical_line.is_canonical { row.canonical() } else { row };
                    canonical_line_parts.push(row);
                }
                new_viewport_rows.append(&mut canonical_line_parts);
            }
            self.viewport.rows = new_viewport_rows;

            let mut new_cursor_y = self.canonical_line_y_coordinates(cursor_canonical_line_index);
            let new_cursor_x = (cursor_index_in_canonical_line / new_columns) + (cursor_index_in_canonical_line % new_columns);
            let current_viewport_row_count = self.viewport.row_count();
            if current_viewport_row_count < self.height {
                self.viewport.pull_bottom_rows_from(&mut self.lines_above, self.height, new_columns);
                let rows_pulled = self.viewport.row_count() - current_viewport_row_count;
                new_cursor_y += rows_pulled;
            } else if current_viewport_row_count > self.height {
                let rows_to_move = current_viewport_row_count - self.height;
                new_cursor_y -= rows_to_move;
                self.move_lines_above_viewport(rows_to_move);
            }
            self.cursor.y = new_cursor_y;
            self.cursor.x = new_cursor_x;
        }
        if new_rows != self.height {
            let current_viewport_row_count = self.viewport.row_count();
            if current_viewport_row_count < new_rows {
                self.viewport.pull_bottom_rows_from(&mut self.lines_above, new_rows, new_columns);
                let rows_pulled = self.viewport.row_count() - current_viewport_row_count;
                self.cursor.y += rows_pulled;
            } else if current_viewport_row_count > new_rows {
                let rows_to_move = current_viewport_row_count - new_rows;
                self.cursor.y -= rows_to_move;
                self.move_lines_above_viewport(rows_to_move);
            }
        }
        self.height = new_rows;
        self.width = new_columns;
    }
    pub fn as_character_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        let lines = self.viewport.as_character_lines(self.width, self.height);
        lines
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        if self.cursor.is_hidden {
            None
        } else {
            Some((self.cursor.x, self.cursor.y))
        }
    }
    pub fn move_viewport_up(&mut self, count: usize) {
        for _ in 0..count {
            self.scroll_up_one_line();
        }
    }
    pub fn move_viewport_down(&mut self, count: usize) {
        for _ in 0..count {
            self.scroll_down_one_line();
        }
    }
    pub fn reset_viewport(&mut self) {
        let row_count_below = self.lines_below.row_count();
        for _ in 0..row_count_below {
            self.scroll_down_one_line();
        }
    }
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        // TBD
    }
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        // TBD
    }
    pub fn add_canonical_line(&mut self) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            if self.cursor.y == scroll_region_bottom {
                // end of scroll region
                // when we have a scroll region set and we're at its bottom
                // we need to delete its first line, thus shifting all lines in it upwards
                // then we add an empty line at its end which will be filled by the application
                // controlling the scroll region (presumably filled by whatever comes next in the
                // scroll buffer, but that's not something we control)
                self.viewport.remove_line(scroll_region_top);
                self.viewport
                    .insert_line(scroll_region_bottom, Row::new().canonical());
                return;
            }
        }
        if self.viewport.row_count() <= self.cursor.y + 1 {
            let new_row = Row::new().canonical();
            self.viewport.push_row(new_row);
        }
        if self.cursor.y == self.height - 1 {
            let line_count_to_move = 1;
            self.move_lines_above_viewport(line_count_to_move);
        } else {
            self.cursor.y += 1;
        }
        self.cursor.x = 0;
    }
    pub fn move_cursor_to_beginning_of_line(&mut self) {
        self.cursor.x = 0;
    }
    pub fn move_cursor_backwards(&mut self, count: usize) {
        if self.cursor.x > count {
            self.cursor.x -= count;
        } else {
            self.cursor.x = 0;
        }
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
        // TODO: try to separate adding characters from moving the cursors in this function 
        if self.cursor.x < self.width {
            self.viewport.add_character_at(terminal_character, self.cursor.x, self.cursor.y);
        } else {
            // line wrap
            self.cursor.x = 0;
            if self.cursor.y == self.height - 1 {
                let line_count_to_move = 1;
                self.move_lines_above_viewport(line_count_to_move);
                let line_wrapped_row = Row::new();
                self.viewport.push_row(line_wrapped_row);
            } else {
                self.cursor.y += 1;
                if self.viewport.row_count() <= self.cursor.y {
                    let line_wrapped_row = Row::new();
                    self.viewport.push_row(line_wrapped_row);
                }
            }
//            if self.viewport.row_count() <= self.cursor.y {
//                let line_wrapped_row = Row::new();
//                self.viewport.push_row(line_wrapped_row);
//            }
            self.viewport.add_character_at(terminal_character, self.cursor.x, self.cursor.y);
        }
        self.move_cursor_forward_until_edge(1);
    }
    pub fn move_cursor_forward_until_edge(&mut self, count: usize) {
        // let count_to_move = std::cmp::min(count, self.width - (self.cursor.x + 1));
        let count_to_move = std::cmp::min(count, self.width - (self.cursor.x));
        self.cursor.x += count_to_move;
        // self.move_cursor_forward(count_to_move);
    }
    pub fn replace_characters_in_line_after_cursor(&mut self, replace_with: TerminalCharacter) {
        self.viewport.truncate_line(self.cursor.y, self.cursor.x);
        if self.cursor.x < self.width - 1 {
            let characters_to_append = vec![replace_with; self.width - self.cursor.x];
            self.viewport.append_to_line(self.cursor.y, characters_to_append);
        }
    }
    pub fn replace_characters_in_line_before_cursor(&mut self, replace_with: TerminalCharacter) {
        let line_part = vec![replace_with; self.cursor.x];
        self.viewport.replace_line_beginning_with(self.cursor.y, line_part);
    }
    pub fn clear_all_after_cursor(&mut self) {
        self.viewport.truncate_line(self.cursor.y, self.cursor.x);
        self.viewport.truncate_rows(self.cursor.y + 1);
    }
    pub fn clear_all(&mut self) {
        self.viewport.clear_all();
    }
    pub fn move_cursor_to(&mut self, x: usize, y: usize) {
        self.cursor.x = x;
        self.cursor.y = y;
        self.viewport.pad_lines(self.cursor.y);
        self.viewport.pad_line(self.cursor.y, self.cursor.x);
    }
    pub fn move_cursor_up(&mut self, count: usize) {
        self.cursor.y = if self.cursor.y < count { 0 } else { self.cursor.y - count };
    }
    pub fn move_cursor_up_with_scrolling(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) =
            self.scroll_region.unwrap_or((0, self.height - 1));
        for _ in 0..count {
            let current_line_index = self.cursor.y;
            if current_line_index == scroll_region_top {
                // if we're at the top line, we create a new line and remove the last line that
                // would otherwise overflow
                self.viewport.remove_line(scroll_region_bottom);
                self.viewport
                    .insert_line(current_line_index, Row::new());
            } else if current_line_index > scroll_region_top
                && current_line_index <= scroll_region_bottom
            {
                self.move_cursor_up(count);
            }
        }
    }
    pub fn move_cursor_down(&mut self, count: usize) {
        self.cursor.y = if self.cursor.y + count > self.height {
            self.height
        } else {
            self.cursor.y + count
        };
    }
    pub fn move_cursor_back(&mut self, count: usize) {
        if self.cursor.x < count {
            self.cursor.x = 0;
        } else {
            self.cursor.x -= count;
        }
    }
    pub fn hide_cursor(&mut self) {
        self.cursor.is_hidden = true;
    }
    pub fn show_cursor (&mut self) {
        self.cursor.is_hidden = false;
    }
    pub fn set_scroll_region(&mut self, top_line_index: usize, bottom_line_index: usize) {
        self.scroll_region = Some((top_line_index, bottom_line_index));
    }
    pub fn clear_scroll_region(&mut self) {
        self.scroll_region = None;
    }
    pub fn delete_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            let current_line_index = self.cursor.y;
            if current_line_index >= scroll_region_top
                && current_line_index <= scroll_region_bottom
            {
                // when deleting lines inside the scroll region, we must make sure it stays the
                // same size (and that other lines below it aren't shifted inside it)
                // so we delete the current line(s) and add an empty line at the end of the scroll
                // region
                for _ in 0..count {
                    self.viewport.remove_line(current_line_index);
                    self.viewport
                        .insert_line(scroll_region_bottom, Row::new());
                }
            }
        }
    }
    pub fn add_empty_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            let current_line_index = self.cursor.y;
            if current_line_index >= scroll_region_top
                && current_line_index <= scroll_region_bottom
            {
                // when adding empty lines inside the scroll region, we must make sure it stays the
                // same size and that lines don't "leak" outside of it
                // so we add an empty line where the cursor currently is, and delete the last line
                // of the scroll region
                for _ in 0..count {
                    self.viewport.remove_line(scroll_region_bottom);
                    self.viewport
                        .insert_line(current_line_index, Row::new());
                }
            }
        }
    }
    pub fn move_cursor_to_column(&mut self, column: usize) {
        self.cursor.x = column;
        self.viewport.pad_line(self.cursor.y, self.cursor.x);
    }
    pub fn move_cursor_to_line(&mut self, line: usize) {
        self.cursor.y = line;
        self.viewport.pad_lines(self.cursor.y);
        self.viewport.pad_line(self.cursor.y, self.cursor.x);
    }
    pub fn replace_with_empty_chars(&mut self, count: usize, empty_char_style: CharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        let pad_until = std::cmp::min(self.width, self.cursor.x + count);
        self.viewport.pad_line(self.cursor.y, pad_until);
        for i in 0..count {
            self.viewport.replace_character_at(empty_character, self.cursor.x + i, self.cursor.y);
        }
    }
    pub fn erase_characters(&mut self, count: usize, empty_char_style: CharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        for _ in 0..count {
            self.viewport.delete_character(self.cursor.y, self.cursor.x);
        }
        let empty_space_to_append = vec![empty_character; count];
        self.viewport.append_to_line(self.cursor.y, empty_space_to_append);
    }
    fn move_lines_above_viewport(&mut self, count: usize) {
        self.viewport.drain_top_rows_into(&mut self.lines_above, count);
    }
    fn move_cursor_forward(&mut self, count: usize) {
        for _ in 0..count {
            // we could definitely just calculate these
            // but I feel it's much easier to understand like this
            // and the performance tax is not THAT big considering the numbers
            if self.cursor.x + 1 <= self.width {
                // move one column forward
                self.cursor.x += 1;
            } else {
                // move to the beginning of next row
                self.cursor.y += 1;
                self.cursor.x = 0;
            }
        }
    }
}

#[derive(Clone)]
pub struct Row {
    pub columns: Vec<TerminalCharacter>,
    pub is_canonical: bool
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
            columns: vec![],
            is_canonical: false,
        }
    }
    pub fn from_columns(columns: Vec<TerminalCharacter>) -> Self {
        Row {
            columns,
            is_canonical: false
        }
    }
    pub fn from_rows(mut rows: Vec<Row>) -> Self {
        if rows.is_empty() {
            Row::new()
        } else {
            let mut first_row = rows.remove(0);
            for row in rows.iter_mut() {
                first_row.append(&mut row.columns);
            }
            first_row
        }
    }
    pub fn with_character(mut self, terminal_character: TerminalCharacter) -> Self {
        self.columns.push(terminal_character);
        self
    }
    pub fn canonical(mut self) -> Self {
        self.is_canonical = true;
        self
    }
    pub fn add_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        if x == self.columns.len() {
            self.columns.push(terminal_character);
        } else if x > self.columns.len() {
            for _ in self.columns.len()..x {
                self.columns.push(EMPTY_TERMINAL_CHARACTER);
            }
            self.columns.push(terminal_character);
        } else {
            // this is much more performant than remove/insert
            self.columns.push(terminal_character);
            self.columns.swap_remove(x);
        }
    }
    pub fn replace_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        // this is much more performant than remove/insert
        self.columns.push(terminal_character);
        self.columns.swap_remove(x);
    }
    pub fn push(&mut self, terminal_character: TerminalCharacter) {
        self.columns.push(terminal_character);
    }
    pub fn truncate(&mut self, x: usize) {
        self.columns.truncate(x);
    }
    pub fn append(&mut self, to_append: &mut Vec<TerminalCharacter>) {
        self.columns.append(to_append);
    }
    pub fn prepend(&mut self, mut to_prepend: Row) {
        to_prepend.append(&mut self.columns);
        self.columns = to_prepend.columns;
    }
    pub fn replace_beginning_with(&mut self, mut line_part: Vec<TerminalCharacter>) {
        drop(self.columns.drain(0..line_part.len()));
        line_part.append(&mut self.columns);
        self.columns = line_part;
    }
    pub fn len(&self) -> usize {
        self.columns.len()
    }
    pub fn delete_character(&mut self, x: usize) {
        self.columns.remove(x);
    }
    pub fn split_to_rows_of_length(&mut self, max_row_length: usize) -> Vec<Row> {
        let mut parts: Vec<Row> = vec![];
        let mut current_part: Vec<TerminalCharacter> = vec![];
        for character in self.columns.drain(..) {
            if current_part.len() == max_row_length {
                parts.push(Row::from_columns(current_part));
                current_part = vec![];
            }
            current_part.push(character);
        }
        if current_part.len() > 0 {
            parts.push(Row::from_columns(current_part))
        };
        if parts.len() > 0 && self.is_canonical {
            parts.get_mut(0).unwrap().is_canonical = true;
        }
        parts
//        loop {
//            if self.columns.len() > max_row_length {
//                let next_row = self.columns.split_off(max_row_length);
//                parts.push_front(Row::from_columns(next_row));
//            } else if self.columns.len() > 0 {
//                let next_row = self.columns.drain(..);
//                if self.is_canonical {
//                    parts.push_front(Row::from_columns(next_row.collect()).canonical());
//                } else {
//                    parts.push_front(Row::from_columns(next_row.collect()));
//                }
//                break;
//            } else {
//                if let Some(first_row) = parts.remove(0) {
//                    parts.push_front(first_row.canonical());
//                }
//                break;
//            }
//        }
//        Vec::from(parts)
    }
}

#[derive(Clone, Debug)]
pub struct Cursor {
    x: usize,
    y: usize,
    is_hidden: bool,
}

impl Cursor {
    pub fn new(x: usize, y: usize) -> Self {
        Cursor {
            x,
            y,
            is_hidden: false
        }
    }
}


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

    pub grid: Grid,
    pub alternative_grid: Option<Grid>, // for 1049h/l instructions which tell us to switch between these two

    pub pid: RawFd,
    // pub scroll: Scroll,
    pub should_render: bool,
    pub position_and_size: PositionAndSize,
    pub position_and_size_override: Option<PositionAndSize>,
    pub cursor_key_mode: bool, // DECCKM - when set, cursor keys should send ANSI direction codes (eg. "OD") instead of the arrow keys (eg. "[D")
    pending_styles: CharacterStyles,
}

impl Rect for TerminalPane {
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
        // let scroll = Scroll::new(ws.columns, ws.rows);
        let grid = Grid::new(ws.rows, ws.columns, pid);
        let pending_styles = CharacterStyles::new();
        let position_and_size = PositionAndSize {
            x,
            y,
            rows: ws.rows,
            columns: ws.columns,
        };
        TerminalPane {
            pid,
            grid,
            alternative_grid: None,
            should_render: true,
            pending_styles,
            position_and_size,
            position_and_size_override: None,
            cursor_key_mode: false,
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
        self.grid.as_character_lines()
    }
    pub fn cursor_coordinates(&self) -> Option<(usize, usize)> {
        // (x, y)
        self.grid.cursor_coordinates()
    }
    pub fn scroll_up(&mut self, count: usize) {
        self.grid.move_viewport_up(count);
        self.mark_for_rerender();
    }
    pub fn scroll_down(&mut self, count: usize) {
        self.grid.move_viewport_down(count);
        self.mark_for_rerender();
    }
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        self.grid.rotate_scroll_region_up(count);
        self.mark_for_rerender();
    }
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        self.grid.rotate_scroll_region_down(count);
        self.mark_for_rerender();
    }
    pub fn clear_scroll(&mut self) {
        self.grid.reset_viewport();
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
    pub fn adjust_input_to_terminal(&self, input_bytes: Vec<u8>) -> Vec<u8> {
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
    fn add_newline(&mut self) {
        self.grid.add_canonical_line();
        // self.reset_all_ansi_codes(); // TODO: find out if we should be resetting here or not
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
                let terminal_tab_character = TerminalCharacter {
                    character: '\t',
                    styles: self.pending_styles,
                };
                // TODO: handle better with line wrapping
                self.grid.add_character(terminal_tab_character);
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
//                self.scroll
//                    .clear_canonical_line_right_of_cursor(self.pending_styles);
                let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
                char_to_replace.styles = self.pending_styles;
                self.grid
                    .replace_characters_in_line_after_cursor(char_to_replace);
            } else if params[0] == 1 {
//                self.scroll
//                    .clear_canonical_line_left_of_cursor(self.pending_styles);
                let mut char_to_replace = EMPTY_TERMINAL_CHARACTER;
                char_to_replace.styles = self.pending_styles;
                self.grid
                    .replace_characters_in_line_before_cursor(char_to_replace);
            }
        // TODO: implement 2
        } else if c == 'J' {
            // clear all (0 => below, 1 => above, 2 => all, 3 => saved)
            if params[0] == 0 {
                // self.scroll.clear_all_after_cursor();
                self.grid.clear_all_after_cursor();
            } else if params[0] == 2 {
                self.grid.clear_all();
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
            } else {
                if params[0] == 0 {
                    (0, params[1] as usize - 1)
                } else {
                    (params[0] as usize - 1, params[1] as usize - 1)
                }
            };
            // self.scroll.move_cursor_to(row, col);
            self.grid.move_cursor_to(col, row);
        } else if c == 'A' {
            // move cursor up until edge of screen
            let move_up_count = if params[0] == 0 { 1 } else { params[0] };
            // self.scroll.move_cursor_up(move_up_count as usize);
            self.grid.move_cursor_up(move_up_count as usize);
        } else if c == 'B' {
            // move cursor down until edge of screen
            let move_down_count = if params[0] == 0 { 1 } else { params[0] };
            // self.scroll.move_cursor_down(move_down_count as usize);
            self.grid.move_cursor_down(move_down_count as usize);
        } else if c == 'D' {
            let move_back_count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            // self.scroll.move_cursor_back(move_back_count);
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
                            // self.grid = alternative_grid;
                        }
                        self.alternative_grid = None;
//                        self.scroll
//                            .override_current_buffer_with_alternative_buffer();
                    }
                    Some(&25) => {
                        // self.scroll.hide_cursor();
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
                        // self.scroll.show_cursor();
                        self.grid.show_cursor();
                        self.mark_for_rerender();
                    }
                    Some(&1049) => {
                        let columns = self.position_and_size_override.map(|x| x.columns).unwrap_or(self.position_and_size.columns);
                        let rows = self.position_and_size_override.map(|x| x.rows).unwrap_or(self.position_and_size.rows);
                        let current_grid = std::mem::replace(&mut self.grid, Grid::new(rows, columns, self.pid));
                        self.alternative_grid = Some(current_grid);
                        // self.scroll.move_current_buffer_to_alternative_buffer();
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
//                self.scroll
//                    .set_scroll_region(top_line_index, bottom_line_index);
                self.grid
                    .set_scroll_region(top_line_index, bottom_line_index);
                // self.scroll.show_cursor();
                self.grid.show_cursor();
            } else {
                // self.scroll.clear_scroll_region();
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
//            self.scroll
//                .delete_lines_in_scroll_region(line_count_to_delete);
            self.grid
                .delete_lines_in_scroll_region(line_count_to_delete);
        } else if c == 'L' {
            // insert blank lines if inside scroll region
            let line_count_to_add = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
//            self.scroll
//                .add_empty_lines_in_scroll_region(line_count_to_add);
            self.grid
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
            // self.scroll.move_cursor_to_column(column);
            self.grid.move_cursor_to_column(column);
        } else if c == 'd' {
            // goto line
            let line = if params[0] == 0 {
                1
            } else {
                // minus 1 because this is 1 indexed
                params[0] as usize - 1
            };
            // self.scroll.move_cursor_to_line(line);
            self.grid.move_cursor_to_line(line);
        } else if c == 'P' {
            // erase characters
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
            // self.scroll.erase_characters(count, self.pending_styles);
            self.grid.erase_characters(count, self.pending_styles);
        } else if c == 'X' {
            // erase characters and replace with empty characters of current style
            let count = if params[0] == 0 {
                1
            } else {
                params[0] as usize
            };
//            self.scroll
//                .replace_with_empty_chars(count, self.pending_styles);
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
            // self.scroll.delete_lines_in_scroll_region(count);
            // self.scroll.add_empty_lines_in_scroll_region(count);
            self.grid.delete_lines_in_scroll_region(count);
            self.grid.add_empty_lines_in_scroll_region(count);
        } else {
            let _ = debug_log_to_file(format!("Unhandled csi: {}->{:?}", c, params));
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (byte, intermediates.get(0)) {
            (b'M', None) => {
                // self.scroll.move_cursor_up_in_scroll_region(1);
                self.grid.move_cursor_up_with_scrolling(1);
            }
            _ => {}
        }
    }
}
