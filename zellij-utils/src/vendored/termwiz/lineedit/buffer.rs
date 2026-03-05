use unicode_segmentation::GraphemeCursor;

use super::actions::Movement;

pub struct LineEditBuffer {
    line: String,
    /// byte index into the UTF-8 string data of the insertion
    /// point.  This is NOT the number of graphemes!
    cursor: usize,
}

impl Default for LineEditBuffer {
    fn default() -> Self {
        Self {
            line: String::new(),
            cursor: 0,
        }
    }
}

impl LineEditBuffer {
    pub fn new(line: &str, cursor: usize) -> Self {
        let mut buffer = Self::default();
        buffer.set_line_and_cursor(line, cursor);
        return buffer;
    }

    pub fn get_line(&self) -> &str {
        return &self.line;
    }

    pub fn get_cursor(&self) -> usize {
        return self.cursor;
    }

    pub fn insert_char(&mut self, c: char) {
        self.line.insert(self.cursor, c);
        let mut cursor = GraphemeCursor::new(self.cursor, self.line.len(), false);
        if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
            self.cursor = pos;
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        self.line.insert_str(self.cursor, text);
        self.cursor += text.len();
    }

    /// The cursor position is the byte index into the line UTF-8 bytes.
    /// Panics: the cursor must be the first byte in a UTF-8 code point
    /// sequence or the end of the provided line.
    pub fn set_line_and_cursor(&mut self, line: &str, cursor: usize) {
        assert!(
            line.is_char_boundary(cursor),
            "cursor {} is not a char boundary of the new line {}",
            cursor,
            line
        );
        self.line = line.to_string();
        self.cursor = cursor;
    }

    pub fn kill_text(&mut self, kill_movement: Movement, move_movement: Movement) {
        let kill_pos = self.eval_movement(kill_movement);
        let new_cursor = self.eval_movement(move_movement);

        let (lower, upper) = if kill_pos < self.cursor {
            (kill_pos, self.cursor)
        } else {
            (self.cursor, kill_pos)
        };

        self.line.replace_range(lower..upper, "");

        // Clamp to the line length, otherwise a kill to end of line
        // command will leave the cursor way off beyond the end of
        // the line.
        self.cursor = new_cursor.min(self.line.len());
    }

    pub fn clear(&mut self) {
        self.line.clear();
        self.cursor = 0;
    }

    pub fn exec_movement(&mut self, movement: Movement) {
        self.cursor = self.eval_movement(movement);
    }

    /// Compute the cursor position after applying movement
    fn eval_movement(&self, movement: Movement) -> usize {
        match movement {
            Movement::BackwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.prev_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            },
            Movement::BackwardWord(rep) => {
                let char_indices: Vec<(usize, char)> = self.line.char_indices().collect();
                if char_indices.is_empty() {
                    return self.cursor;
                }
                let mut char_position = char_indices
                    .iter()
                    .position(|(idx, _)| *idx == self.cursor)
                    .unwrap_or(char_indices.len() - 1);

                for _ in 0..rep {
                    if char_position == 0 {
                        break;
                    }

                    let mut found = None;
                    for prev in (0..char_position - 1).rev() {
                        if char_indices[prev].1.is_whitespace() {
                            found = Some(prev + 1);
                            break;
                        }
                    }

                    char_position = found.unwrap_or(0);
                }
                char_indices[char_position].0
            },
            Movement::ForwardWord(rep) => {
                let char_indices: Vec<(usize, char)> = self.line.char_indices().collect();
                if char_indices.is_empty() {
                    return self.cursor;
                }
                let mut char_position = char_indices
                    .iter()
                    .position(|(idx, _)| *idx == self.cursor)
                    .unwrap_or_else(|| char_indices.len());

                for _ in 0..rep {
                    // Skip any non-whitespace characters
                    while char_position < char_indices.len()
                        && !char_indices[char_position].1.is_whitespace()
                    {
                        char_position += 1;
                    }

                    // Skip any whitespace characters
                    while char_position < char_indices.len()
                        && char_indices[char_position].1.is_whitespace()
                    {
                        char_position += 1;
                    }

                    // We are now on the start of the next word
                }
                char_indices
                    .get(char_position)
                    .map(|(i, _)| *i)
                    .unwrap_or_else(|| self.line.len())
            },
            Movement::ForwardChar(rep) => {
                let mut position = self.cursor;
                for _ in 0..rep {
                    let mut cursor = GraphemeCursor::new(position, self.line.len(), false);
                    if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
                        position = pos;
                    } else {
                        break;
                    }
                }
                position
            },
            Movement::StartOfLine => 0,
            Movement::EndOfLine => {
                let mut cursor =
                    GraphemeCursor::new(self.line.len().saturating_sub(1), self.line.len(), false);
                if let Ok(Some(pos)) = cursor.next_boundary(&self.line, 0) {
                    pos
                } else {
                    self.cursor
                }
            },
            Movement::None => self.cursor,
        }
    }
}
