use crate::panes::terminal_character::{
    Cursor,
    LinkAnchor
};
use crate::panes::grid::Row;
use crate::panes::link_handler::LinkHandler;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
struct DetectedLink {
    pub url: String,
    pub start_position: HyperlinkPosition,
    pub end_position: HyperlinkPosition,
}

#[derive(Debug, Clone, Copy)]
struct HyperlinkPosition {
    x: isize,
    y: isize,
}

impl HyperlinkPosition {
    fn from_cursor(cursor: &Cursor) -> Self {
        Self {
            x: cursor.x as isize,
            y: cursor.y as isize,
        }
    }
}

#[derive(Clone)]
pub struct HyperlinkTracker {
    buffer: String,
    cursor_positions: Vec<HyperlinkPosition>,
    start_position: Option<HyperlinkPosition>,
    last_cursor: Option<HyperlinkPosition>,
}

impl HyperlinkTracker {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_positions: Vec::new(),
            start_position: None,
            last_cursor: None,
        }
    }

    pub fn update(
        &mut self, 
        ch: char, 
        cursor: &Cursor,
        viewport: &mut Vec<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &mut LinkHandler,
    ) {

        if ch == ' ' && cursor.x == 0 {
            // skip carriage return
            return;
        }

        let current_pos = HyperlinkPosition::from_cursor(cursor);

        // Check if cursor moved non-contiguously
        if self.should_reset_due_to_cursor_jump(&current_pos) {
            if self.is_currently_tracking() {
                // Finalize the current URL before resetting
                self.finalize_and_apply(viewport, lines_above, link_handler);
            } else {
                self.clear();
            }
        }

        if self.is_currently_tracking() {
            if self.is_url_terminator(ch) {
                self.finalize_and_apply(viewport, lines_above, link_handler);
            } else {
                self.buffer.push(ch);
                self.cursor_positions.push(current_pos.clone());
            }
        } else {
            if matches!(ch, 'h' | 'f' | 'm') {
                self.buffer.push(ch);
                self.cursor_positions.push(current_pos.clone());
                self.start_position = Some(current_pos.clone());
            }
        }

        self.last_cursor = Some(current_pos);
    }

    pub fn offset_cursor_lines(&mut self, offset: isize) {
        // Offset all stored cursor positions
        for pos in &mut self.cursor_positions {
            pos.y -= offset;
        }
        
        if let Some(start_pos) = &mut self.start_position {
            start_pos.y -= offset;
        }
        
        if let Some(last_cursor) = &mut self.last_cursor {
            last_cursor.y -= offset;
        }
    }

    fn should_reset_due_to_cursor_jump(&self, current_pos: &HyperlinkPosition) -> bool {
        if let Some(last_pos) = &self.last_cursor {
            // Check if cursor moved non-contiguously
            let is_contiguous = 
                // Same line, next column
                (current_pos.y == last_pos.y && current_pos.x == last_pos.x + 1) ||
                // Next line, first column (line wrap)
                (current_pos.y == last_pos.y + 1 && current_pos.x == 0) ||
                // Same position (overwrite)
                (current_pos.y == last_pos.y && current_pos.x == last_pos.x);
            
            !is_contiguous
        } else {
            false
        }
    }

    fn is_currently_tracking(&self) -> bool {
        self.start_position.is_some()
    }

    fn is_url_terminator(&self, ch: char) -> bool {
        matches!(ch, ' ' | '\n' | '\r' | '\t' | '"' | '\'' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}' | '⏎')
    }

    fn finalize_and_apply(
        &mut self, 
        viewport: &mut Vec<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &mut LinkHandler
    ) {
        let original_len = self.buffer.chars().count();
        let trimmed_url = self.trim_trailing_punctuation(&self.buffer);
        let trimmed_len = trimmed_url.chars().count();
        
        if self.is_valid_url(&trimmed_url) {
            // Calculate how many characters we trimmed
            let chars_trimmed = original_len.saturating_sub(trimmed_len);
            
            // Find the end position by walking back from the last position
            let end_position = if chars_trimmed > 0 && trimmed_len > 0 {
                // Use the position of the last character that's actually in the trimmed URL
                self.cursor_positions.get(trimmed_len.saturating_sub(1))
            } else {
                // No trimming occurred, use the last position
                self.cursor_positions.last()
            };
            let Some(end_position) = end_position.copied() else {
                return;
            };

            let detected_link = DetectedLink {
                url: trimmed_url.clone(),
                start_position: self.start_position.clone().unwrap(),
                end_position,
            };

            self.apply_hyperlink_to_grid(&detected_link, viewport, lines_above, link_handler);
        }

        self.clear();
    }

    fn apply_hyperlink_to_grid(
        &self,
        link: &DetectedLink,
        viewport: &mut Vec<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &mut LinkHandler
    ) {

        let link_anchor_start = link_handler.new_link_from_url(link.url.clone());
        
        let start_pos = &link.start_position;
        let end_pos = &link.end_position;
        
        for y in start_pos.y..=end_pos.y {
            let row = if y < 0 {
                // Row is in lines_above
                let lines_above_index = (lines_above.len() as isize + y) as usize;
                lines_above.get_mut(lines_above_index)
            } else if (y as usize) < viewport.len() {
                // Row is in viewport
                viewport.get_mut(y as usize)
            } else {
                // Row is beyond bounds, skip
                None
            };

            if let Some(row) = row {

                let start_x = if y == start_pos.y { start_pos.x.max(0) as usize } else { 0 };
                let end_x = if y == end_pos.y { (end_pos.x + 1).max(0) as usize } else { row.width() };
                
                // Convert width-based positions to character indices
                let start_char_index = row.absolute_character_index(start_x);
                let end_char_index = row.absolute_character_index(end_x.min(row.width()));

                for char_index in start_char_index..=end_char_index.min(row.columns.len().saturating_sub(1)) {
                    if let Some(character) = row.columns.get_mut(char_index) {
                        character.styles.update(|styles| {
                            if y == start_pos.y && char_index == start_char_index {
                                // First character gets the start anchor
                                styles.link_anchor = Some(link_anchor_start.clone());
                            } else if y == end_pos.y && char_index == end_char_index {
                                // Last character gets the end anchor
                                styles.link_anchor = Some(LinkAnchor::End);
                            } else {
                                // Middle characters get the same start anchor
                                styles.link_anchor = Some(link_anchor_start.clone());
                            }
                        });
                    }
                }
            }
        }
    }

    fn trim_trailing_punctuation(&self, url: &str) -> String {
        let mut chars: Vec<char> = url.chars().collect();
        
        while let Some(&last_char) = chars.last() {
            if matches!(last_char, '.' | ',' | ';' | '!' | '?' | '\n' | '\r' | '⏎') {
                chars.pop();
            } else {
                break;
            }
        }
        
        chars.into_iter().collect()
    }

    fn is_valid_url(&self, url: &str) -> bool {
        if url.len() < 8 { return false; }
        
        if url.starts_with("http://") || url.starts_with("https://") {
            if let Some(protocol_end) = url.find("://") {
                let after_protocol = &url[protocol_end.saturating_add(3)..];
                return !after_protocol.is_empty() && after_protocol.contains('.');
            }
        }
        
        if url.starts_with("ftp://") {
            let after_protocol = url.get(6..).unwrap_or("");
            return !after_protocol.is_empty();
        }
        
        if url.starts_with("mailto:") {
            let after_colon = url.get(7..).unwrap_or("");
            return after_colon.contains('@');
        }
        
        false
    }

    fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_positions.clear();
        self.start_position = None;
        // Don't clear last_cursor here - we need it for jump detection
    }
}
