use std::collections::VecDeque;
use std::fmt::{self, Debug, Formatter};

use crate::terminal_pane::terminal_character::{
    CharacterStyles, TerminalCharacter, EMPTY_TERMINAL_CHARACTER,
};

/*
 * Scroll
 *
 * holds the terminal buffer and controls the viewport (which part of it we see)
 * its functions include line-wrapping and tracking the location of the cursor
 *
 */

/*
 * CanonicalLine vs. WrappedFragment
 *
 * If the terminal had infinite width and we did not need to line wrap, the CanonicalLine would
 * be our only unit of line separation.
 * Each CanonicalLine has one or more WrappedFragments, which are re-calculated when the terminal is
 * resized, or when characters are added to the line
 *
 */

#[derive(Clone)]
pub struct CanonicalLine {
    pub wrapped_fragments: Vec<WrappedFragment>,
}

impl CanonicalLine {
    pub fn new() -> Self {
        CanonicalLine {
            wrapped_fragments: vec![WrappedFragment::new()],
        }
    }
    pub fn add_new_wrap(&mut self, terminal_character: TerminalCharacter) {
        let mut new_fragment = WrappedFragment::new();
        new_fragment.add_character(terminal_character, 0);
        self.wrapped_fragments.push(new_fragment);
    }
    pub fn change_width(&mut self, new_width: usize) {
        let characters = self.flattened_characters();
        let wrapped_fragments =
            CanonicalLine::fill_fragments_up_to_width(characters, new_width, None);
        self.wrapped_fragments = wrapped_fragments;
    }
    pub fn clear_after(&mut self, fragment_index: usize, column_index: usize) {
        let fragment_to_clear = self
            .wrapped_fragments
            .get_mut(fragment_index)
            .expect("fragment out of bounds");
        fragment_to_clear.clear_after_and_including(column_index);
        self.wrapped_fragments.truncate(fragment_index + 1);
    }
    pub fn replace_with_empty_chars(
        &mut self,
        fragment_index: usize,
        from_col: usize,
        count: usize,
        style_of_empty_space: CharacterStyles,
    ) {
        let mut characters_replaced = 0;
        let mut column_position_in_fragment = from_col;
        let mut current_fragment = fragment_index;
        let mut empty_space_character = EMPTY_TERMINAL_CHARACTER;
        empty_space_character.styles = style_of_empty_space;
        loop {
            if characters_replaced == count {
                break;
            }
            match self.wrapped_fragments.get_mut(current_fragment) {
                Some(fragment_to_clear) => {
                    let fragment_characters_count = fragment_to_clear.characters.len();
                    if fragment_characters_count >= column_position_in_fragment {
                        fragment_to_clear
                            .add_character(empty_space_character, column_position_in_fragment);
                        column_position_in_fragment += 1;
                        characters_replaced += 1;
                    } else {
                        current_fragment += 1;
                        column_position_in_fragment = 0;
                    }
                }
                None => {
                    // end of line, nothing more to clear
                    break;
                }
            }
        }
    }
    pub fn erase_chars(
        &mut self,
        fragment_index: usize,
        from_col: usize,
        count: usize,
        style_of_empty_space: CharacterStyles,
    ) {
        let mut empty_character = EMPTY_TERMINAL_CHARACTER;
        empty_character.styles = style_of_empty_space;
        let current_width = self.wrapped_fragments.get(0).unwrap().characters.len();
        let mut characters = self.flattened_characters();
        let absolute_position_of_character = fragment_index * current_width + from_col;
        for _ in 0..count {
            characters.remove(absolute_position_of_character);
        }
        let wrapped_fragments = CanonicalLine::fill_fragments_up_to_width(
            characters,
            current_width,
            Some(empty_character),
        );
        self.wrapped_fragments = wrapped_fragments;
    }
    pub fn replace_with_empty_chars_after_cursor(
        &mut self,
        fragment_index: usize,
        from_col: usize,
        total_columns: usize,
        style_of_empty_space: CharacterStyles,
    ) {
        let mut empty_char_character = EMPTY_TERMINAL_CHARACTER;
        empty_char_character.styles = style_of_empty_space;
        let current_fragment = self.wrapped_fragments.get_mut(fragment_index).unwrap();
        let fragment_characters_count = current_fragment.characters.len();

        for i in from_col..fragment_characters_count {
            current_fragment.add_character(empty_char_character, i);
        }

        for i in fragment_characters_count..total_columns {
            current_fragment.add_character(empty_char_character, i);
        }

        self.wrapped_fragments.truncate(fragment_index + 1);
    }
    pub fn replace_with_empty_chars_before_cursor(
        &mut self,
        fragment_index: usize,
        until_col: usize,
        style_of_empty_space: CharacterStyles,
    ) {
        let mut empty_char_character = EMPTY_TERMINAL_CHARACTER;
        empty_char_character.styles = style_of_empty_space;

        if fragment_index > 0 {
            for i in 0..(fragment_index - 1) {
                let fragment = self.wrapped_fragments.get_mut(i).unwrap();
                for i in 0..fragment.characters.len() {
                    fragment.add_character(empty_char_character, i);
                }
            }
        }
        let current_fragment = self.wrapped_fragments.get_mut(fragment_index).unwrap();
        for i in 0..until_col {
            current_fragment.add_character(empty_char_character, i);
        }
    }
    fn flattened_characters(&self) -> Vec<TerminalCharacter> {
        self.wrapped_fragments
            .iter()
            .fold(
                Vec::with_capacity(self.wrapped_fragments.len()),
                |mut characters, wrapped_fragment| {
                    characters.push(wrapped_fragment.characters.iter().copied());
                    characters
                },
            )
            .into_iter()
            .flatten()
            .collect()
    }
    fn fill_fragments_up_to_width(
        mut characters: Vec<TerminalCharacter>,
        width: usize,
        padding: Option<TerminalCharacter>,
    ) -> Vec<WrappedFragment> {
        let mut wrapped_fragments = vec![];
        while !characters.is_empty() {
            if characters.len() > width {
                wrapped_fragments.push(WrappedFragment::from_vec(
                    characters.drain(..width).collect(),
                ));
            } else {
                let mut last_fragment = WrappedFragment::from_vec(characters.drain(..).collect());
                let last_fragment_len = last_fragment.characters.len();
                if let Some(empty_char_character) = padding {
                    if last_fragment_len < width {
                        for _ in last_fragment_len..width {
                            last_fragment.characters.push(empty_char_character);
                        }
                    }
                }
                wrapped_fragments.push(last_fragment);
            }
        }
        if wrapped_fragments.is_empty() {
            wrapped_fragments.push(WrappedFragment::new());
        }
        wrapped_fragments
    }
}

impl Debug for CanonicalLine {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for wrapped_fragment in &self.wrapped_fragments {
            writeln!(f, "{:?}", wrapped_fragment)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct WrappedFragment {
    pub characters: Vec<TerminalCharacter>,
}

impl WrappedFragment {
    pub fn new() -> Self {
        WrappedFragment { characters: vec![] }
    }
    pub fn add_character(
        &mut self,
        terminal_character: TerminalCharacter,
        position_in_line: usize,
    ) {
        if position_in_line == self.characters.len() {
            self.characters.push(terminal_character);
        } else {
            // this is much more performant than remove/insert
            self.characters.push(terminal_character);
            self.characters.swap_remove(position_in_line);
        }
    }
    pub fn from_vec(characters: Vec<TerminalCharacter>) -> Self {
        WrappedFragment { characters }
    }
    pub fn clear_after_and_including(&mut self, character_index: usize) {
        self.characters.truncate(character_index);
    }
}

impl Debug for WrappedFragment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for character in &self.characters {
            write!(f, "{:?}", character)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CursorPosition {
    line_index: (usize, usize), // (canonical line index, fragment index in line)
    column_index: usize,        // 0 is the first character from the pane edge
}

impl CursorPosition {
    pub fn new() -> Self {
        CursorPosition {
            line_index: (0, 0),
            column_index: 0,
        }
    }
    pub fn move_forward(&mut self, count: usize) {
        // TODO: panic if out of bounds?
        self.column_index += count;
    }
    pub fn move_backwards(&mut self, count: usize) {
        self.column_index -= count;
    }
    pub fn move_to_next_linewrap(&mut self) {
        self.line_index.1 += 1;
    }
    pub fn move_to_next_canonical_line(&mut self) {
        self.line_index.0 += 1;
    }
    pub fn _move_to_prev_canonical_line(&mut self) {
        self.line_index.0 -= 1;
    }
    pub fn move_to_beginning_of_linewrap(&mut self) {
        self.column_index = 0;
    }
    pub fn move_to_beginning_of_canonical_line(&mut self) {
        self.column_index = 0;
        self.line_index.1 = 0;
    }
    pub fn move_up_by_canonical_lines(&mut self, count: usize) {
        let current_canonical_line_position = self.line_index.0;
        if count > current_canonical_line_position {
            self.line_index = (0, 0);
        } else {
            self.line_index = (current_canonical_line_position - count, 0);
        }
    }
    pub fn move_down_by_canonical_lines(&mut self, count: usize) {
        // this method does not verify that we will not overflow
        let current_canonical_line_position = self.line_index.0;
        self.line_index = (current_canonical_line_position + count, 0);
    }
    pub fn move_to_canonical_line(&mut self, index: usize) {
        self.line_index = (index, 0);
    }
    pub fn move_to_column(&mut self, col: usize) {
        self.column_index = col;
    }
    pub fn reset(&mut self) {
        self.column_index = 0;
        self.line_index = (0, 0);
    }
}

pub struct Scroll {
    canonical_lines: Vec<CanonicalLine>,
    cursor_position: CursorPosition,
    total_columns: usize,
    lines_in_view: usize,
    viewport_bottom_offset: Option<usize>,
    scroll_region: Option<(usize, usize)>, // start line, end line (if set, this is the area the will scroll)
    show_cursor: bool,
    alternative_buffer: Option<Vec<CanonicalLine>>,
    alternative_cursor_position: Option<CursorPosition>,
}

impl Scroll {
    pub fn new(total_columns: usize, lines_in_view: usize) -> Self {
        let mut canonical_lines = vec![];
        canonical_lines.push(CanonicalLine::new());
        let cursor_position = CursorPosition::new();
        Scroll {
            canonical_lines: vec![CanonicalLine::new()], // The rest will be created by newlines explicitly
            total_columns,
            lines_in_view,
            cursor_position,
            viewport_bottom_offset: None,
            scroll_region: None,
            show_cursor: true,
            alternative_buffer: None,
            alternative_cursor_position: None,
        }
    }
    pub fn as_character_lines(&self) -> Vec<Vec<TerminalCharacter>> {
        let mut lines: VecDeque<Vec<TerminalCharacter>> = VecDeque::new(); // TODO: with capacity lines_from_end?
        let canonical_lines = self.canonical_lines.iter().rev();
        let mut lines_to_skip = self.viewport_bottom_offset.unwrap_or(0);
        'gather_lines: for current_canonical_line in canonical_lines {
            for wrapped_fragment in current_canonical_line.wrapped_fragments.iter().rev() {
                let mut line: Vec<TerminalCharacter> =
                    wrapped_fragment.characters.iter().copied().collect();
                if lines_to_skip > 0 {
                    lines_to_skip -= 1;
                } else {
                    for _ in line.len()..self.total_columns {
                        // pad line if needed
                        line.push(EMPTY_TERMINAL_CHARACTER);
                    }
                    lines.push_front(line);
                }
                if lines.len() == self.lines_in_view {
                    break 'gather_lines;
                }
            }
        }
        if lines.len() < self.lines_in_view {
            // pad lines in case we don't have enough scrollback to fill the view
            let empty_line = vec![EMPTY_TERMINAL_CHARACTER; self.total_columns];

            for _ in lines.len()..self.lines_in_view {
                // pad lines in case we didn't have enough
                lines.push_back(empty_line.clone());
            }
        }
        Vec::from(lines)
    }
    pub fn add_character(&mut self, terminal_character: TerminalCharacter) {
        let (canonical_line_position, wrapped_fragment_index_in_line) =
            self.cursor_position.line_index;
        let cursor_position_in_line = self.cursor_position.column_index;
        let current_line = self
            .canonical_lines
            .get_mut(canonical_line_position)
            .expect("cursor out of bounds");
        let current_wrapped_fragment = current_line
            .wrapped_fragments
            .get_mut(wrapped_fragment_index_in_line)
            .expect("cursor out of bounds");

        if cursor_position_in_line < self.total_columns {
            current_wrapped_fragment.add_character(terminal_character, cursor_position_in_line);
            self.cursor_position.move_forward(1);
        } else {
            current_line.add_new_wrap(terminal_character);
            self.cursor_position.move_to_next_linewrap();
            self.cursor_position.move_to_beginning_of_linewrap();
            self.cursor_position.move_forward(1);
        }
    }
    pub fn show_cursor(&mut self) {
        self.show_cursor = true;
    }
    pub fn hide_cursor(&mut self) {
        self.show_cursor = false;
    }
    pub fn add_canonical_line(&mut self) {
        let current_canonical_line_index = self.cursor_position.line_index.0;
        if let Some((scroll_region_top, scroll_region_bottom)) =
            self.scroll_region_absolute_indices()
        {
            if current_canonical_line_index == scroll_region_bottom {
                // end of scroll region
                // when we have a scroll region set and we're at its bottom
                // we need to delete its first line, thus shifting all lines in it upwards
                // then we add an empty line at its end which will be filled by the application
                // controlling the scroll region (presumably filled by whatever comes next in the
                // scroll buffer, but that's not something we control)
                self.canonical_lines.remove(scroll_region_top);
                self.canonical_lines
                    .insert(scroll_region_bottom, CanonicalLine::new());
                return;
            }
        }

        use std::cmp::Ordering;
        match current_canonical_line_index.cmp(&(self.canonical_lines.len() - 1)) {
            Ordering::Equal => {
                self.canonical_lines.push(CanonicalLine::new());
                self.cursor_position.move_to_next_canonical_line();
                self.cursor_position.move_to_beginning_of_canonical_line();
            }
            Ordering::Less => {
                self.cursor_position.move_to_next_canonical_line();
                self.cursor_position.move_to_beginning_of_canonical_line();
            }
            _ => panic!("cursor out of bounds, cannot add_canonical_line"),
        }
    }
    pub fn cursor_coordinates_on_screen(&self) -> Option<(usize, usize)> {
        // (x, y)
        if !self.show_cursor {
            return None;
        }
        let (canonical_line_cursor_position, line_wrap_cursor_position) =
            self.cursor_position.line_index;
        let x = self.cursor_position.column_index;
        let mut y = 0;
        let indices_and_canonical_lines = self.canonical_lines.iter().enumerate().rev();
        for (current_index, current_line) in indices_and_canonical_lines {
            if current_index == canonical_line_cursor_position {
                y += current_line.wrapped_fragments.len() - line_wrap_cursor_position;
                break;
            } else {
                y += current_line.wrapped_fragments.len();
            }
        }
        let total_lines = self
            .canonical_lines
            .iter()
            .fold(0, |total_lines, current_line| {
                total_lines + current_line.wrapped_fragments.len()
            }); // TODO: is this performant enough? should it be cached or kept track of?
        let y = if total_lines < self.lines_in_view {
            total_lines - y
        } else if y > self.lines_in_view {
            self.lines_in_view
        } else {
            self.lines_in_view - y
        };
        Some((x, y))
    }
    pub fn move_cursor_forward(&mut self, count: usize) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        let current_fragment = current_canonical_line
            .wrapped_fragments
            .get_mut(current_line_wrap_position)
            .expect("cursor out of bounds");
        let move_count = if current_cursor_column_position + count > self.total_columns {
            // move to last column in the current line wrap
            self.total_columns - current_cursor_column_position
        } else {
            count
        };

        for _ in current_fragment.characters.len()..current_cursor_column_position + move_count {
            current_fragment.characters.push(EMPTY_TERMINAL_CHARACTER);
        }
        self.cursor_position.move_forward(move_count);
    }
    pub fn move_cursor_back(&mut self, count: usize) {
        let current_cursor_column_position = self.cursor_position.column_index;
        if current_cursor_column_position < count {
            self.cursor_position.move_to_beginning_of_linewrap();
        } else {
            self.cursor_position.move_backwards(count);
        }
    }
    pub fn move_cursor_to_beginning_of_linewrap(&mut self) {
        self.cursor_position.move_to_beginning_of_linewrap();
    }
    pub fn _move_cursor_to_beginning_of_canonical_line(&mut self) {
        self.cursor_position.move_to_beginning_of_canonical_line();
    }
    pub fn move_cursor_backwards(&mut self, count: usize) {
        self.cursor_position.move_backwards(count);
    }
    pub fn move_cursor_up(&mut self, count: usize) {
        self.cursor_position.move_up_by_canonical_lines(count);
    }
    pub fn move_cursor_down(&mut self, count: usize) {
        let current_canonical_line = self.cursor_position.line_index.0;
        let max_count = (self.canonical_lines.len() - 1) - current_canonical_line;
        let count = std::cmp::min(count, max_count);
        self.cursor_position.move_down_by_canonical_lines(count);
    }
    pub fn change_size(&mut self, columns: usize, lines: usize) {
        for canonical_line in self.canonical_lines.iter_mut() {
            canonical_line.change_width(columns);
        }
        let cursor_line = self
            .canonical_lines
            .get(self.cursor_position.line_index.0)
            .expect("cursor out of bounds");
        if cursor_line.wrapped_fragments.len() <= self.cursor_position.line_index.1 {
            self.cursor_position.line_index.1 = cursor_line.wrapped_fragments.len() - 1;
        }
        self.lines_in_view = lines;
        self.total_columns = columns;
    }
    pub fn clear_canonical_line_right_of_cursor(&mut self, style_of_empty_space: CharacterStyles) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        current_canonical_line.replace_with_empty_chars_after_cursor(
            current_line_wrap_position,
            current_cursor_column_position,
            self.total_columns,
            style_of_empty_space,
        );
    }
    pub fn clear_canonical_line_left_of_cursor(&mut self, style_of_empty_space: CharacterStyles) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        current_canonical_line.replace_with_empty_chars_before_cursor(
            current_line_wrap_position,
            current_cursor_column_position,
            style_of_empty_space,
        );
    }
    pub fn clear_all_after_cursor(&mut self) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        current_canonical_line
            .clear_after(current_line_wrap_position, current_cursor_column_position);
        self.canonical_lines
            .truncate(current_canonical_line_index + 1);
    }
    pub fn replace_with_empty_chars(
        &mut self,
        count: usize,
        style_of_empty_space: CharacterStyles,
    ) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        current_canonical_line.replace_with_empty_chars(
            current_line_wrap_position,
            current_cursor_column_position,
            count,
            style_of_empty_space,
        );
    }
    pub fn erase_characters(&mut self, count: usize, style_of_empty_space: CharacterStyles) {
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_cursor_column_position = self.cursor_position.column_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        current_canonical_line.erase_chars(
            current_line_wrap_position,
            current_cursor_column_position,
            count,
            style_of_empty_space,
        );
    }
    pub fn clear_all(&mut self) {
        self.canonical_lines.clear();
        self.canonical_lines.push(CanonicalLine::new());
        self.cursor_position.reset();
    }
    pub fn move_cursor_to(&mut self, line: usize, col: usize) {
        let line_on_screen = if self.canonical_lines.len() > self.lines_in_view {
            line + (self.canonical_lines.len() - self.lines_in_view)
        } else {
            line
        };
        if self.canonical_lines.len() > line_on_screen {
            self.cursor_position.move_to_canonical_line(line_on_screen);
        } else {
            for _ in self.canonical_lines.len()..=line_on_screen {
                self.canonical_lines.push(CanonicalLine::new());
            }
            self.cursor_position.move_to_canonical_line(line_on_screen);
        }
        let (current_canonical_line_index, current_line_wrap_position) =
            self.cursor_position.line_index;
        let current_canonical_line = self
            .canonical_lines
            .get_mut(current_canonical_line_index)
            .expect("cursor out of bounds");
        let current_fragment = current_canonical_line
            .wrapped_fragments
            .get_mut(current_line_wrap_position)
            .expect("cursor out of bounds");

        for _ in current_fragment.characters.len()..col {
            current_fragment.characters.push(EMPTY_TERMINAL_CHARACTER);
        }
        self.cursor_position.move_to_column(col);
    }
    pub fn move_cursor_to_column(&mut self, col: usize) {
        let current_canonical_line = self.cursor_position.line_index.0;
        self.move_cursor_to(current_canonical_line, col);
    }
    pub fn move_cursor_to_line(&mut self, line: usize) {
        let current_column = self.cursor_position.column_index;
        self.move_cursor_to(line, current_column);
    }
    pub fn move_current_buffer_to_alternative_buffer(&mut self) {
        self.alternative_buffer = Some(self.canonical_lines.drain(..).collect());
        self.alternative_cursor_position = Some(self.cursor_position);
        self.cursor_position.reset();
    }
    pub fn override_current_buffer_with_alternative_buffer(&mut self) {
        if let Some(alternative_buffer) = self.alternative_buffer.as_mut() {
            self.canonical_lines = alternative_buffer.drain(..).collect();
        }
        if let Some(alternative_cursor_position) = self.alternative_cursor_position {
            self.cursor_position = alternative_cursor_position;
        }
        self.alternative_buffer = None;
        self.alternative_cursor_position = None;
    }
    pub fn set_scroll_region(&mut self, top_line: usize, bottom_line: usize) {
        self.scroll_region = Some((top_line, bottom_line));
    }
    pub fn clear_scroll_region(&mut self) {
        self.scroll_region = None;
    }
    fn scroll_region_absolute_indices(&mut self) -> Option<(usize, usize)> {
        if self.scroll_region.is_none() {
            return None;
        };
        if self.canonical_lines.len() > self.lines_in_view {
            let absolute_top = self.canonical_lines.len() - 1 - self.lines_in_view;
            let absolute_bottom = self.canonical_lines.len() - 1;
            Some((absolute_top, absolute_bottom))
        } else {
            Some((self.scroll_region.unwrap().0, self.scroll_region.unwrap().1))
        }
    }
    pub fn delete_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) =
            self.scroll_region_absolute_indices()
        {
            let current_canonical_line_index = self.cursor_position.line_index.0;
            if current_canonical_line_index >= scroll_region_top
                && current_canonical_line_index <= scroll_region_bottom
            {
                // when deleting lines inside the scroll region, we must make sure it stays the
                // same size (and that other lines below it aren't shifted inside it)
                // so we delete the current line(s) and add an empty line at the end of the scroll
                // region
                for _ in 0..count {
                    self.canonical_lines.remove(current_canonical_line_index);
                    self.canonical_lines
                        .insert(scroll_region_bottom, CanonicalLine::new());
                }
            }
        }
    }
    pub fn add_empty_lines_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) =
            self.scroll_region_absolute_indices()
        {
            let current_canonical_line_index = self.cursor_position.line_index.0;
            if current_canonical_line_index >= scroll_region_top
                && current_canonical_line_index <= scroll_region_bottom
            {
                // when adding empty lines inside the scroll region, we must make sure it stays the
                // same size and that lines don't "leak" outside of it
                // so we add an empty line where the cursor currently is, and delete the last line
                // of the scroll region
                for _ in 0..count {
                    self.canonical_lines.remove(scroll_region_bottom);
                    self.canonical_lines
                        .insert(current_canonical_line_index, CanonicalLine::new());
                }
            }
        }
    }
    pub fn move_cursor_up_in_scroll_region(&mut self, count: usize) {
        if let Some((scroll_region_top, scroll_region_bottom)) =
            self.scroll_region_absolute_indices()
        {
            // the scroll region indices start at 1, so we need to adjust them
            for _ in 0..count {
                let current_canonical_line_index = self.cursor_position.line_index.0;
                if current_canonical_line_index == scroll_region_top {
                    // if we're at the top line, we create a new line and remove the last line that
                    // would otherwise overflow
                    self.canonical_lines.remove(scroll_region_bottom);
                    self.canonical_lines
                        .insert(current_canonical_line_index, CanonicalLine::new());
                } else if current_canonical_line_index > scroll_region_top
                    && current_canonical_line_index <= scroll_region_bottom
                {
                    self.move_cursor_up(count);
                }
            }
        }
    }
    /// [scroll_up](https://github.com/alacritty/alacritty/blob/ec42b42ce601808070462111c0c28edb0e89babb/alacritty_terminal/src/grid/mod.rs#L261)
    /// This function takes the first line of the scroll region and moves it to the bottom (count times)
    pub fn rotate_scroll_region_up(&mut self, count: usize) {
        if let Some((_, scroll_region_bottom)) = self.scroll_region_absolute_indices() {
            if self.show_cursor {
                let scroll_region_bottom_index = scroll_region_bottom - 1;
                self.cursor_position
                    .move_to_canonical_line(scroll_region_bottom_index);

                let new_empty_lines = vec![CanonicalLine::new(); count];
                self.canonical_lines.splice(
                    scroll_region_bottom_index..scroll_region_bottom_index + 1,
                    new_empty_lines,
                );

                self.cursor_position
                    .move_to_canonical_line(scroll_region_bottom_index + count);
            }
        }
    }
    /// [scroll_down](https://github.com/alacritty/alacritty/blob/ec42b42ce601808070462111c0c28edb0e89babb/alacritty_terminal/src/grid/mod.rs#L221)
    /// This function takes the last line of the scroll region and moves it to the top (count times)
    pub fn rotate_scroll_region_down(&mut self, count: usize) {
        if let Some((scroll_region_top, _)) = self.scroll_region_absolute_indices() {
            if self.show_cursor {
                let scroll_region_top_index = scroll_region_top - 1;
                self.cursor_position
                    .move_to_canonical_line(scroll_region_top_index);

                let new_empty_lines = vec![CanonicalLine::new(); count];
                self.canonical_lines.splice(
                    scroll_region_top_index..scroll_region_top_index,
                    new_empty_lines,
                );

                self.cursor_position
                    .move_to_canonical_line(scroll_region_top_index + count);
            }
        }
    }
    pub fn move_viewport_up(&mut self, count: usize) {
        if let Some(current_offset) = self.viewport_bottom_offset.as_mut() {
            *current_offset += count;
        } else {
            self.viewport_bottom_offset = Some(count);
        }
    }
    pub fn move_viewport_down(&mut self, count: usize) {
        if let Some(current_offset) = self.viewport_bottom_offset.as_mut() {
            if *current_offset > count {
                *current_offset -= count;
            } else {
                self.viewport_bottom_offset = None;
            }
        }
    }
    pub fn reset_viewport(&mut self) {
        self.viewport_bottom_offset = None;
    }
}

impl Debug for Scroll {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for line in &self.canonical_lines {
            writeln!(f, "{:?}", line)?;
        }
        Ok(())
    }
}
