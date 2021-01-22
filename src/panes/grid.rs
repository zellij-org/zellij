use std::{
    cmp::Ordering,
    fmt::{self, Debug, Formatter},
};

use crate::panes::terminal_character::{
    CharacterStyles, TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
};

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
        }
        None => vec![],
    }
}

fn get_bottom_canonical_row_and_wraps(rows: &mut Vec<Row>) -> Vec<Row> {
    let mut index_of_last_non_canonical_row = None;
    for (i, row) in rows.iter().enumerate().rev() {
        if row.is_canonical {
            index_of_last_non_canonical_row = Some(i);
            break;
        } else {
            index_of_last_non_canonical_row = Some(i);
        }
    }
    match index_of_last_non_canonical_row {
        Some(index_of_last_non_canonical_row) => {
            rows.drain(index_of_last_non_canonical_row..).collect()
        }
        None => vec![],
    }
}

fn transfer_rows_down(
    source: &mut Vec<Row>,
    destination: &mut Vec<Row>,
    count: usize,
    max_src_width: Option<usize>,
    max_dst_width: Option<usize>,
) {
    let mut next_lines: Vec<Row> = vec![];
    let mut lines_added_to_destination: isize = 0;
    loop {
        if lines_added_to_destination as usize == count {
            break;
        }
        if next_lines.is_empty() {
            match source.pop() {
                Some(next_line) => {
                    let mut top_non_canonical_rows_in_dst = get_top_non_canonical_rows(destination);
                    lines_added_to_destination -= top_non_canonical_rows_in_dst.len() as isize;
                    next_lines.push(next_line);
                    next_lines.append(&mut top_non_canonical_rows_in_dst);
                    next_lines = match max_dst_width {
                        Some(max_row_width) => {
                            Row::from_rows(next_lines).split_to_rows_of_length(max_row_width)
                        }
                        None => vec![Row::from_rows(next_lines)],
                    };
                }
                None => break, // no more rows
            }
        }
        destination.insert(0, next_lines.pop().unwrap());
        lines_added_to_destination += 1;
    }
    if !next_lines.is_empty() {
        match max_src_width {
            Some(max_row_width) => {
                let mut excess_rows =
                    Row::from_rows(next_lines).split_to_rows_of_length(max_row_width);
                source.append(&mut excess_rows);
            }
            None => {
                let excess_row = Row::from_rows(next_lines);
                source.push(excess_row);
            }
        }
    }
}

fn transfer_rows_up(
    source: &mut Vec<Row>,
    destination: &mut Vec<Row>,
    count: usize,
    max_src_width: Option<usize>,
    max_dst_width: Option<usize>,
) {
    let mut next_lines: Vec<Row> = vec![];
    for _ in 0..count {
        if next_lines.is_empty() {
            if !source.is_empty() {
                let next_line = source.remove(0);
                if !next_line.is_canonical {
                    let mut bottom_canonical_row_and_wraps_in_dst =
                        get_bottom_canonical_row_and_wraps(destination);
                    next_lines.append(&mut bottom_canonical_row_and_wraps_in_dst);
                }
                next_lines.push(next_line);
                next_lines = match max_dst_width {
                    Some(max_row_width) => {
                        Row::from_rows(next_lines).split_to_rows_of_length(max_row_width)
                    }
                    None => vec![Row::from_rows(next_lines)],
                };
            } else {
                break; // no more rows
            }
        }
        destination.push(next_lines.remove(0));
    }
    if !next_lines.is_empty() {
        match max_src_width {
            Some(max_row_width) => {
                let excess_rows = Row::from_rows(next_lines).split_to_rows_of_length(max_row_width);
                for row in excess_rows {
                    source.insert(0, row);
                }
            }
            None => {
                let excess_row = Row::from_rows(next_lines);
                source.insert(0, excess_row);
            }
        }
    }
}

#[derive(Clone)]
pub struct Grid {
    lines_above: Vec<Row>,
    viewport: Vec<Row>,
    lines_below: Vec<Row>,
    cursor: Cursor,
    scroll_region: Option<(usize, usize)>,
    width: usize,
    height: usize,
}

impl Debug for Grid {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for (i, row) in self.viewport.iter().enumerate() {
            if row.is_canonical {
                writeln!(f, "{:?} (C): {:?}", i, row)?;
            } else {
                writeln!(f, "{:?} (W): {:?}", i, row)?;
            }
        }
        Ok(())
    }
}

impl Grid {
    pub fn new(rows: usize, columns: usize) -> Self {
        Grid {
            lines_above: vec![],
            viewport: vec![],
            lines_below: vec![],
            cursor: Cursor::new(0, 0),
            scroll_region: None,
            width: columns,
            height: rows,
        }
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
    // TODO: merge these two funtions
    fn cursor_index_in_canonical_line(&self) -> usize {
        let mut cursor_canonical_line_index = 0;
        let mut cursor_index_in_canonical_line = 0;
        for (i, line) in self.viewport.iter().enumerate() {
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
        for (i, line) in self.viewport.iter().enumerate() {
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
        if !self.lines_above.is_empty() && self.viewport.len() == self.height {
            let line_to_push_down = self.viewport.pop().unwrap();
            self.lines_below.insert(0, line_to_push_down);
            let line_to_insert_at_viewport_top = self.lines_above.pop().unwrap();
            self.viewport.insert(0, line_to_insert_at_viewport_top);
        }
    }
    pub fn scroll_down_one_line(&mut self) {
        if !self.lines_below.is_empty() && self.viewport.len() == self.height {
            let mut line_to_push_up = self.viewport.remove(0);
            if line_to_push_up.is_canonical {
                self.lines_above.push(line_to_push_up);
            } else {
                let mut last_line_above = self.lines_above.pop().unwrap();
                last_line_above.append(&mut line_to_push_up.columns);
                self.lines_above.push(last_line_above);
            }
            let line_to_insert_at_viewport_bottom = self.lines_below.remove(0);
            self.viewport.push(line_to_insert_at_viewport_bottom);
        }
    }
    pub fn change_size(&mut self, new_rows: usize, new_columns: usize) {
        if new_columns != self.width {
            let mut cursor_canonical_line_index = self.cursor_canonical_line_index();
            let cursor_index_in_canonical_line = self.cursor_index_in_canonical_line();
            let mut viewport_canonical_lines = vec![];
            for mut row in self.viewport.drain(..) {
                if !row.is_canonical
                    && viewport_canonical_lines.is_empty()
                    && !self.lines_above.is_empty()
                {
                    let mut first_line_above = self.lines_above.pop().unwrap();
                    first_line_above.append(&mut row.columns);
                    viewport_canonical_lines.push(first_line_above);
                    cursor_canonical_line_index += 1;
                } else if row.is_canonical {
                    viewport_canonical_lines.push(row);
                } else {
                    match viewport_canonical_lines.last_mut() {
                        Some(last_line) => {
                            last_line.append(&mut row.columns);
                        }
                        None => {
                            // the state is corrupted somehow
                            // this is a bug and I'm not yet sure why it happens
                            // usually it fixes itself and is a result of some race
                            // TODO: investigate why this happens and solve it
                            return;
                        }
                    }
                }
            }
            let mut new_viewport_rows = vec![];
            for mut canonical_line in viewport_canonical_lines {
                let mut canonical_line_parts: Vec<Row> = vec![];
                if canonical_line.columns.is_empty() {
                    canonical_line_parts.push(Row::new().canonical());
                }
                while !canonical_line.columns.is_empty() {
                    let next_wrap = if canonical_line.len() > new_columns {
                        canonical_line.columns.drain(..new_columns)
                    } else {
                        canonical_line.columns.drain(..)
                    };
                    let row = Row::from_columns(next_wrap.collect());
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

            let mut new_cursor_y = self.canonical_line_y_coordinates(cursor_canonical_line_index);
            let new_cursor_x = (cursor_index_in_canonical_line / new_columns)
                + (cursor_index_in_canonical_line % new_columns);
            let current_viewport_row_count = self.viewport.len();
            match current_viewport_row_count.cmp(&self.height) {
                Ordering::Less => {
                    let row_count_to_transfer = self.height - current_viewport_row_count;
                    transfer_rows_down(
                        &mut self.lines_above,
                        &mut self.viewport,
                        row_count_to_transfer,
                        None,
                        Some(new_columns),
                    );
                    let rows_pulled = self.viewport.len() - current_viewport_row_count;
                    new_cursor_y += rows_pulled;
                }
                Ordering::Greater => {
                    let row_count_to_transfer = current_viewport_row_count - self.height;
                    if row_count_to_transfer > new_cursor_y {
                        new_cursor_y = 0;
                    } else {
                        new_cursor_y -= row_count_to_transfer;
                    }
                    transfer_rows_up(
                        &mut self.viewport,
                        &mut self.lines_above,
                        row_count_to_transfer,
                        Some(new_columns),
                        None,
                    );
                }
                Ordering::Equal => {}
            }
            self.cursor.y = new_cursor_y;
            self.cursor.x = new_cursor_x;
        }
        if new_rows != self.height {
            let current_viewport_row_count = self.viewport.len();
            match current_viewport_row_count.cmp(&new_rows) {
                Ordering::Less => {
                    let row_count_to_transfer = new_rows - current_viewport_row_count;
                    transfer_rows_down(
                        &mut self.lines_above,
                        &mut self.viewport,
                        row_count_to_transfer,
                        None,
                        Some(new_columns),
                    );
                    let rows_pulled = self.viewport.len() - current_viewport_row_count;
                    self.cursor.y += rows_pulled;
                }
                Ordering::Greater => {
                    let row_count_to_transfer = current_viewport_row_count - new_rows;
                    if row_count_to_transfer > self.cursor.y {
                        self.cursor.y = 0;
                    } else {
                        self.cursor.y -= row_count_to_transfer;
                    }
                    transfer_rows_up(
                        &mut self.viewport,
                        &mut self.lines_above,
                        row_count_to_transfer,
                        Some(new_columns),
                        None,
                    );
                }
                Ordering::Equal => {}
            }
        }
        self.height = new_rows;
        self.width = new_columns;
        if self.scroll_region.is_some() {
            self.set_scroll_region_to_viewport_size();
        }
    }
    pub fn as_character_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        let mut lines: Vec<Vec<TerminalCharacter>> = self
            .viewport
            .iter()
            .map(|r| {
                let mut line: Vec<TerminalCharacter> = r.columns.iter().copied().collect();
                // pad line
                line.resize(self.width, EMPTY_TERMINAL_CHARACTER);
                line
            })
            .collect();
        let empty_row = vec![EMPTY_TERMINAL_CHARACTER; self.width];
        for _ in lines.len()..self.height {
            lines.push(empty_row.clone());
        }
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
        let row_count_below = self.lines_below.len();
        for _ in 0..row_count_below {
            self.scroll_down_one_line();
        }
    }
    pub fn rotate_scroll_region_up(&mut self, _count: usize) {
        // TBD
    }
    pub fn rotate_scroll_region_down(&mut self, _count: usize) {
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
                self.viewport.remove(scroll_region_top);
                self.viewport
                    .insert(scroll_region_bottom, Row::new().canonical());
                return;
            }
        }
        if self.viewport.len() <= self.cursor.y + 1 {
            let new_row = Row::new().canonical();
            self.viewport.push(new_row);
        }
        if self.cursor.y == self.height - 1 {
            let row_count_to_transfer = 1;
            transfer_rows_up(
                &mut self.viewport,
                &mut self.lines_above,
                row_count_to_transfer,
                Some(self.width),
                None,
            );
        } else {
            self.cursor.y += 1;
        }
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
    pub fn insert_character_at_cursor_position(&mut self, terminal_character: TerminalCharacter) {
        match self.viewport.get_mut(self.cursor.y) {
            Some(row) => row.add_character_at(terminal_character, self.cursor.x),
            None => {
                // pad lines until cursor if they do not exist
                for _ in self.viewport.len()..self.cursor.y {
                    self.viewport.push(Row::new().canonical());
                }
                self.viewport
                    .push(Row::new().with_character(terminal_character).canonical());
            }
        }
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
        // TODO: try to separate adding characters from moving the cursors in this function
        if self.cursor.x < self.width {
            self.insert_character_at_cursor_position(terminal_character);
        } else {
            // line wrap
            self.cursor.x = 0;
            if self.cursor.y == self.height - 1 {
                let row_count_to_transfer = 1;
                transfer_rows_up(
                    &mut self.viewport,
                    &mut self.lines_above,
                    row_count_to_transfer,
                    Some(self.width),
                    None,
                );
                let wrapped_row = Row::new();
                self.viewport.push(wrapped_row);
            } else {
                self.cursor.y += 1;
                if self.viewport.len() <= self.cursor.y {
                    let line_wrapped_row = Row::new();
                    self.viewport.push(line_wrapped_row);
                }
            }
            self.insert_character_at_cursor_position(terminal_character);
        }
        self.move_cursor_forward_until_edge(1);
    }
    pub fn move_cursor_forward_until_edge(&mut self, count: usize) {
        let count_to_move = std::cmp::min(count, self.width - (self.cursor.x));
        self.cursor.x += count_to_move;
    }
    pub fn replace_characters_in_line_after_cursor(&mut self, replace_with: TerminalCharacter) {
        self.viewport
            .get_mut(self.cursor.y)
            .unwrap()
            .truncate(self.cursor.x);
        if self.cursor.x < self.width - 1 {
            let mut characters_to_append = vec![replace_with; self.width - self.cursor.x];
            self.viewport
                .get_mut(self.cursor.y)
                .unwrap()
                .append(&mut characters_to_append);
        }
    }
    pub fn replace_characters_in_line_before_cursor(&mut self, replace_with: TerminalCharacter) {
        let line_part = vec![replace_with; self.cursor.x];
        let row = self.viewport.get_mut(self.cursor.y).unwrap();
        row.replace_beginning_with(line_part);
    }
    pub fn clear_all_after_cursor(&mut self, replace_with: TerminalCharacter) {
        let cursor_row = self.viewport.get_mut(self.cursor.y).unwrap();
        cursor_row.truncate(self.cursor.x);
        let mut replace_with_columns_in_cursor_row = vec![replace_with; self.width - self.cursor.x];
        cursor_row.append(&mut replace_with_columns_in_cursor_row);

        let replace_with_columns = vec![replace_with; self.width];
        self.replace_characters_in_line_after_cursor(replace_with);
        for row in self.viewport.iter_mut().skip(self.cursor.y + 1) {
            row.replace_columns(replace_with_columns.clone());
        }
    }
    pub fn clear_cursor_line(&mut self) {
        self.viewport.get_mut(self.cursor.y).unwrap().truncate(0);
    }
    pub fn clear_all(&mut self, replace_with: TerminalCharacter) {
        let replace_with_columns = vec![replace_with; self.width];
        self.replace_characters_in_line_after_cursor(replace_with);
        for row in self.viewport.iter_mut() {
            row.replace_columns(replace_with_columns.clone());
        }
    }
    fn pad_current_line_until(&mut self, position: usize) {
        let current_row = self.viewport.get_mut(self.cursor.y).unwrap();
        for _ in current_row.len()..position {
            current_row.push(EMPTY_TERMINAL_CHARACTER);
        }
    }
    fn pad_lines_until(&mut self, position: usize) {
        for _ in self.viewport.len()..position {
            self.viewport.push(Row::new().canonical());
        }
    }
    pub fn move_cursor_to(&mut self, x: usize, y: usize) {
        self.cursor.x = x;
        self.cursor.y = y;
        self.pad_lines_until(self.cursor.y + 1);
        self.pad_current_line_until(self.cursor.x);
    }
    pub fn move_cursor_up(&mut self, count: usize) {
        self.cursor.y = if self.cursor.y < count {
            0
        } else {
            self.cursor.y - count
        };
    }
    pub fn move_cursor_up_with_scrolling(&mut self, count: usize) {
        let (scroll_region_top, scroll_region_bottom) =
            self.scroll_region.unwrap_or((0, self.height - 1));
        for _ in 0..count {
            let current_line_index = self.cursor.y;
            if current_line_index == scroll_region_top {
                // if we're at the top line, we create a new line and remove the last line that
                // would otherwise overflow
                self.viewport.remove(scroll_region_bottom);
                self.viewport.insert(current_line_index, Row::new()); // TODO: .canonical() ?
            } else if current_line_index > scroll_region_top
                && current_line_index <= scroll_region_bottom
            {
                self.move_cursor_up(count);
            }
        }
    }
    pub fn move_cursor_down(&mut self, count: usize) {
        let lines_to_add = if self.cursor.y + count > self.height - 1 {
            (self.cursor.y + count) - (self.height - 1)
        } else {
            0
        };
        self.cursor.y = if self.cursor.y + count > self.height - 1 {
            self.height - 1
        } else {
            self.cursor.y + count
        };
        for _ in 0..lines_to_add {
            self.add_canonical_line();
        }
        self.pad_lines_until(self.cursor.y);
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
    pub fn show_cursor(&mut self) {
        self.cursor.is_hidden = false;
    }
    pub fn set_scroll_region(&mut self, top_line_index: usize, bottom_line_index: usize) {
        self.scroll_region = Some((top_line_index, bottom_line_index));
    }
    pub fn clear_scroll_region(&mut self) {
        self.scroll_region = None;
    }
    pub fn set_scroll_region_to_viewport_size(&mut self) {
        self.scroll_region = Some((0, self.height - 1));
    }
    pub fn delete_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            let current_line_index = self.cursor.y;
            if current_line_index >= scroll_region_top && current_line_index <= scroll_region_bottom
            {
                // when deleting lines inside the scroll region, we must make sure it stays the
                // same size (and that other lines below it aren't shifted inside it)
                // so we delete the current line(s) and add an empty line at the end of the scroll
                // region
                for _ in 0..count {
                    self.viewport.remove(current_line_index);
                    self.viewport
                        .insert(scroll_region_bottom, Row::new().canonical());
                }
            }
        }
    }
    pub fn add_empty_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) = self.scroll_region {
            let current_line_index = self.cursor.y;
            if current_line_index >= scroll_region_top && current_line_index <= scroll_region_bottom
            {
                // when adding empty lines inside the scroll region, we must make sure it stays the
                // same size and that lines don't "leak" outside of it
                // so we add an empty line where the cursor currently is, and delete the last line
                // of the scroll region
                for _ in 0..count {
                    self.viewport.remove(scroll_region_bottom);
                    self.viewport
                        .insert(current_line_index, Row::new().canonical());
                }
            }
        }
    }
    pub fn move_cursor_to_column(&mut self, column: usize) {
        self.cursor.x = column;
        self.pad_current_line_until(self.cursor.x);
    }
    pub fn move_cursor_to_line(&mut self, line: usize) {
        self.cursor.y = line;
        self.pad_lines_until(self.cursor.y + 1);
        self.pad_current_line_until(self.cursor.x);
    }
    pub fn replace_with_empty_chars(&mut self, count: usize, empty_char_style: CharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        let pad_until = std::cmp::min(self.width, self.cursor.x + count);
        self.pad_current_line_until(pad_until);
        let current_row = self.viewport.get_mut(self.cursor.y).unwrap();
        for i in 0..count {
            current_row.replace_character_at(empty_character, self.cursor.x + i);
        }
    }
    pub fn erase_characters(&mut self, count: usize, empty_char_style: CharacterStyles) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = empty_char_style;
        let current_row = self.viewport.get_mut(self.cursor.y).unwrap();
        for _ in 0..count {
            current_row.delete_character(self.cursor.x);
        }
        let mut empty_space_to_append = vec![empty_character; count];
        self.viewport
            .get_mut(self.cursor.y)
            .unwrap()
            .append(&mut empty_space_to_append);
    }
}

#[derive(Clone)]
pub struct Row {
    pub columns: Vec<TerminalCharacter>,
    pub is_canonical: bool,
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
            is_canonical: false,
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
        match self.columns.len().cmp(&x) {
            Ordering::Equal => self.columns.push(terminal_character),
            Ordering::Less => {
                self.columns.resize(x, EMPTY_TERMINAL_CHARACTER);
                self.columns.push(terminal_character);
            }
            Ordering::Greater => {
                // this is much more performant than remove/insert
                self.columns.push(terminal_character);
                self.columns.swap_remove(x);
            }
        }
    }
    pub fn replace_character_at(&mut self, terminal_character: TerminalCharacter, x: usize) {
        // this is much more performant than remove/insert
        self.columns.push(terminal_character);
        self.columns.swap_remove(x);
    }
    pub fn replace_columns(&mut self, columns: Vec<TerminalCharacter>) {
        self.columns = columns;
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
        if !current_part.is_empty() {
            parts.push(Row::from_columns(current_part))
        };
        if !parts.is_empty() && self.is_canonical {
            parts.get_mut(0).unwrap().is_canonical = true;
        }
        parts
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
            is_hidden: false,
        }
    }
}
