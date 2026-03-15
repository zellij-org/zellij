use crate::panes::grid::Row;
use crate::panes::link_handler::LinkHandler;
use crate::panes::terminal_character::{Cursor, LinkAnchor};
use std::collections::VecDeque;

#[derive(Debug, Clone)]
struct DetectedLink {
    url: String,
    start_position: HyperlinkPosition,
    end_position: HyperlinkPosition,
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
        viewport: &mut VecDeque<Row>,
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
        matches!(
            ch,
            ' ' | '\n'
                | '\r'
                | '\t'
                | '"'
                | '\''
                | '<'
                | '>'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '⏎'
        )
    }

    fn finalize_and_apply(
        &mut self,
        viewport: &mut VecDeque<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &mut LinkHandler,
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
        viewport: &mut VecDeque<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &mut LinkHandler,
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
                let start_x = if y == start_pos.y {
                    start_pos.x.max(0) as usize
                } else {
                    0
                };
                let end_x = if y == end_pos.y {
                    (end_pos.x + 1).max(0) as usize
                } else {
                    row.width()
                };

                // Convert width-based positions to character indices
                let start_char_index = row.absolute_character_index(start_x);
                let end_char_index = row.absolute_character_index(end_x.min(row.width()));

                for char_index in
                    start_char_index..=end_char_index.min(row.columns.len().saturating_sub(1))
                {
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
        if url.len() < 8 {
            return false;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panes::grid::Row;
    use crate::panes::link_handler::LinkHandler;
    use crate::panes::terminal_character::{LinkAnchor, TerminalCharacter};
    use std::collections::VecDeque;

    fn create_test_cursor(x: usize, y: usize) -> Cursor {
        Cursor::new(x, y, true)
    }

    fn create_test_row(width: usize) -> Row {
        let mut columns = VecDeque::new();
        for _ in 0..width {
            columns.push_back(TerminalCharacter::new(' '));
        }
        Row::from_columns(columns).canonical()
    }

    fn populate_row_with_text(row: &mut Row, text: &str, start_x: usize) {
        for (i, ch) in text.chars().enumerate() {
            let char_index = row.absolute_character_index(start_x + i);
            if let Some(character) = row.columns.get_mut(char_index) {
                character.character = ch;
            }
        }
    }

    fn create_test_viewport(rows: usize, cols: usize) -> VecDeque<Row> {
        (0..rows).map(|_| create_test_row(cols)).collect()
    }

    #[test]
    fn test_new_tracker_is_empty() {
        let tracker = HyperlinkTracker::new();
        assert!(tracker.buffer.is_empty());
        assert!(tracker.cursor_positions.is_empty());
        assert!(tracker.start_position.is_none());
        assert!(tracker.last_cursor.is_none());
    }

    #[test]
    fn test_simple_http_url_detection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url = "http://example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        let mut link_id = None;

        for i in 0..url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "Character at position {} should have link anchor",
                    i
                );

                if i == 0 {
                    if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                        link_id = Some(*id);
                    }
                }
            }
        }

        if let Some(id) = link_id {
            let links = link_handler.links();
            let stored_link = links.get(&id);
            assert!(
                stored_link.is_some(),
                "Link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link {
                assert_eq!(link.uri, url, "Stored URL should match the detected URL");
                assert_eq!(link.id, Some(id.to_string()), "Link ID should be set");
            }
        } else {
            panic!("Should have found a link ID");
        }
    }

    #[test]
    fn test_https_url_detection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url = "https://secure.example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        let mut link_id = None;

        for i in 0..url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "HTTPS URL character at position {} should have link anchor",
                    i
                );

                if i == 0 {
                    if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                        link_id = Some(*id);
                    }
                }
            }
        }

        if let Some(id) = link_id {
            let links = link_handler.links();
            let stored_link = links.get(&id);
            assert!(
                stored_link.is_some(),
                "HTTPS link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link {
                assert_eq!(
                    link.uri, url,
                    "Stored HTTPS URL should match the detected URL"
                );
            }
        }
    }

    #[test]
    fn test_ftp_url_detection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url = "ftp://files.example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            '\n',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        for i in 0..url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "FTP URL character at position {} should have link anchor",
                    i
                );
            }
        }
    }

    #[test]
    fn test_mailto_url_detection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url = "mailto:user@example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        for i in 0..url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "Mailto URL character at position {} should have link anchor",
                    i
                );
            }
        }
    }

    #[test]
    fn test_url_with_trailing_punctuation() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url_with_punct = "http://example.com.";
        let expected_trimmed_url = "http://example.com";

        populate_row_with_text(&mut viewport[0], url_with_punct, 0);

        for (i, ch) in url_with_punct.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url_with_punct.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        let mut link_id = None;

        let first_char_index = row.absolute_character_index(0);
        if let Some(character) = row.columns.get(first_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                link_id = Some(*id);
            }
        }
        if let Some(id) = link_id {
            let links = link_handler.links();
            let stored_link = links.get(&id);
            assert!(
                stored_link.is_some(),
                "Link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link {
                assert_eq!(
                    link.uri, expected_trimmed_url,
                    "Stored URL should be trimmed (without trailing punctuation)"
                );
            }
        } else {
            panic!("Should have found a link ID");
        }
    }

    #[test]
    fn test_invalid_url_rejection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let short_url = "http://";

        populate_row_with_text(&mut viewport[0], short_url, 0);

        for (i, ch) in short_url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(short_url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];
        for i in 0..short_url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_none(),
                    "Invalid URL character at position {} should not have link anchor",
                    i
                );
            }
        }
    }

    #[test]
    fn test_cursor_jump_resets_tracking() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let partial_url = "http://exam";
        for (i, ch) in partial_url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        assert!(tracker.is_currently_tracking());

        let cursor = create_test_cursor(50, 5);
        tracker.update(
            'h',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        assert_eq!(tracker.buffer, "h");
        assert_eq!(tracker.cursor_positions.len(), 1);
    }

    #[test]
    fn test_line_wrap_continuation() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let cursor1 = create_test_cursor(79, 0);
        tracker.update(
            'h',
            &cursor1,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let cursor2 = create_test_cursor(0, 1);
        tracker.update(
            't',
            &cursor2,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        assert!(tracker.is_currently_tracking());
        assert_eq!(tracker.buffer, "ht");
        assert_eq!(tracker.cursor_positions.len(), 2);
    }

    #[test]
    fn test_offset_cursor_lines() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let cursor = create_test_cursor(0, 5);
        tracker.update(
            'h',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        tracker.offset_cursor_lines(2);

        assert_eq!(tracker.start_position.unwrap().y, 3);
        assert_eq!(tracker.last_cursor.unwrap().y, 3);
        assert_eq!(tracker.cursor_positions[0].y, 3);
    }

    #[test]
    fn test_multiline_url_detection() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url_part1 = "http://very-long-";
        let url_part2 = "domain.example.com";
        let full_url = format!("{}{}", url_part1, url_part2);

        populate_row_with_text(&mut viewport[0], url_part1, 0);
        populate_row_with_text(&mut viewport[1], url_part2, 0);

        for (i, ch) in url_part1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        for (i, ch) in url_part2.chars().enumerate() {
            let cursor = create_test_cursor(i, 1);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url_part2.len(), 1);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row0 = &viewport[0];
        let mut link_id = None;

        let first_char_index = row0.absolute_character_index(0);
        if let Some(character) = row0.columns.get(first_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                link_id = Some(*id);
            }
        }

        if let Some(id) = link_id {
            let links = link_handler.links();
            let stored_link = links.get(&id);
            assert!(
                stored_link.is_some(),
                "Multiline link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link {
                assert_eq!(
                    link.uri, full_url,
                    "Stored URL should be the complete multiline URL"
                );
            }
        } else {
            panic!("Should have found a link ID for multiline URL");
        }

        let row0 = &viewport[0];
        for i in 0..url_part1.len() {
            let char_index = row0.absolute_character_index(i);
            if let Some(character) = row0.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "Multiline URL part 1 character at position {} should have link anchor",
                    i
                );
            }
        }

        let row1 = &viewport[1];
        for i in 0..url_part2.len() {
            let char_index = row1.absolute_character_index(i);
            if let Some(character) = row1.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "Multiline URL part 2 character at position {} should have link anchor",
                    i
                );
            }
        }
    }

    #[test]
    fn test_url_terminators() {
        let terminators = vec![
            ' ', '\n', '\r', '\t', '"', '\'', '<', '>', '(', ')', '[', ']', '{', '}', '⏎',
        ];

        for (idx, terminator) in terminators.iter().enumerate() {
            if idx >= 10 {
                break;
            }

            let mut tracker = HyperlinkTracker::new();
            let mut viewport = create_test_viewport(10, 80);
            let mut lines_above = VecDeque::new();
            let mut link_handler = LinkHandler::new();

            let url = "http://example.com";

            populate_row_with_text(&mut viewport[idx], url, 0);

            for (i, ch) in url.chars().enumerate() {
                let cursor = create_test_cursor(i, idx);
                tracker.update(
                    ch,
                    &cursor,
                    &mut viewport,
                    &mut lines_above,
                    &mut link_handler,
                );
            }

            let cursor = create_test_cursor(url.len(), idx);
            tracker.update(
                *terminator,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );

            let row = &viewport[idx];
            for i in 0..url.len() {
                let char_index = row.absolute_character_index(i);
                if let Some(character) = row.columns.get(char_index) {
                    assert!(
                        character.styles.link_anchor.is_some(),
                        "URL terminated by {:?} should have link anchor at position {}",
                        terminator,
                        i
                    );
                }
            }
        }
    }

    #[test]
    fn test_skip_carriage_return_at_line_start() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let cursor = create_test_cursor(0, 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        assert!(!tracker.is_currently_tracking());
        assert!(tracker.buffer.is_empty());
    }

    #[test]
    fn test_tracking_state_methods() {
        let mut tracker = HyperlinkTracker::new();

        assert!(!tracker.is_currently_tracking());

        tracker.start_position = Some(HyperlinkPosition { x: 0, y: 0 });
        assert!(tracker.is_currently_tracking());

        tracker.clear();
        assert!(!tracker.is_currently_tracking());
        assert!(tracker.buffer.is_empty());
        assert!(tracker.cursor_positions.is_empty());
        assert!(tracker.start_position.is_none());
    }

    #[test]
    fn test_hyperlink_position_from_cursor() {
        let cursor = create_test_cursor(10, 5);
        let pos = HyperlinkPosition::from_cursor(&cursor);

        assert_eq!(pos.x, 10);
        assert_eq!(pos.y, 5);
    }

    #[test]
    fn test_contiguous_cursor_movement() {
        let mut tracker = HyperlinkTracker::new();

        tracker.last_cursor = Some(HyperlinkPosition { x: 5, y: 2 });

        let next_col = HyperlinkPosition { x: 6, y: 2 };
        assert!(!tracker.should_reset_due_to_cursor_jump(&next_col));

        let next_line = HyperlinkPosition { x: 0, y: 3 };
        assert!(!tracker.should_reset_due_to_cursor_jump(&next_line));

        let same_pos = HyperlinkPosition { x: 5, y: 2 };
        assert!(!tracker.should_reset_due_to_cursor_jump(&same_pos));

        let jump = HyperlinkPosition { x: 10, y: 5 };
        assert!(tracker.should_reset_due_to_cursor_jump(&jump));
    }

    #[test]
    fn test_trim_trailing_punctuation() {
        let tracker = HyperlinkTracker::new();

        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com."),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com,"),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com;"),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com!"),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com?"),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com..."),
            "http://example.com"
        );
        assert_eq!(
            tracker.trim_trailing_punctuation("http://example.com"),
            "http://example.com"
        );
    }

    #[test]
    fn test_is_valid_url() {
        let tracker = HyperlinkTracker::new();

        assert!(tracker.is_valid_url("http://example.com"));
        assert!(tracker.is_valid_url("https://example.com"));
        assert!(tracker.is_valid_url("ftp://files.example.com"));
        assert!(tracker.is_valid_url("mailto:user@example.com"));
        assert!(tracker.is_valid_url("https://sub.domain.example.com/path"));

        assert!(!tracker.is_valid_url("http://"));
        assert!(!tracker.is_valid_url("https://"));
        assert!(!tracker.is_valid_url("ftp://"));
        assert!(!tracker.is_valid_url("mailto:"));
        assert!(!tracker.is_valid_url("mailto:notanemail"));
        assert!(!tracker.is_valid_url("http://nodot"));
        assert!(!tracker.is_valid_url("short"));
        assert!(!tracker.is_valid_url(""));
    }

    #[test]
    fn test_multiple_urls_in_sequence() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url1 = "http://first.com";
        let url2 = "https://second.com";
        let full_text = format!("{} {}", url1, url2);

        populate_row_with_text(&mut viewport[0], &full_text, 0);

        for (i, ch) in url1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url1.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        for (i, ch) in url2.chars().enumerate() {
            let cursor = create_test_cursor(url1.len() + 1 + i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url1.len() + 1 + url2.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];

        let mut first_link_id = None;
        let first_char_index = row.absolute_character_index(0);
        if let Some(character) = row.columns.get(first_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                first_link_id = Some(*id);
            }
        }

        let mut second_link_id = None;
        let second_url_start = url1.len() + 1;
        let second_char_index = row.absolute_character_index(second_url_start);
        if let Some(character) = row.columns.get(second_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                second_link_id = Some(*id);
            }
        }
        let links = link_handler.links();

        if let Some(id1) = first_link_id {
            let stored_link1 = links.get(&id1);
            assert!(
                stored_link1.is_some(),
                "First link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link1 {
                assert_eq!(link.uri, url1, "First stored URL should match");
            }
        } else {
            panic!("Should have found first link ID");
        }

        if let Some(id2) = second_link_id {
            let stored_link2 = links.get(&id2);
            assert!(
                stored_link2.is_some(),
                "Second link should be stored in LinkHandler"
            );
            if let Some(link) = stored_link2 {
                assert_eq!(link.uri, url2, "Second stored URL should match");
            }
        } else {
            panic!("Should have found second link ID");
        }

        assert_ne!(
            first_link_id, second_link_id,
            "Each URL should have a unique link ID"
        );
        assert_eq!(links.len(), 2, "Should have exactly 2 links stored");
    }

    #[test]
    fn test_url_in_lines_above() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(5, 80);
        let mut lines_above = VecDeque::new();

        for _ in 0..3 {
            lines_above.push_back(create_test_row(80));
        }

        let mut link_handler = LinkHandler::new();

        let url = "http://example.com";
        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        tracker.offset_cursor_lines(2);

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let lines_above_index = lines_above.len().saturating_sub(2);
        if let Some(row) = lines_above.get(lines_above_index) {
            for i in 0..url.len() {
                let char_index = row.absolute_character_index(i);
                if let Some(character) = row.columns.get(char_index) {
                    assert!(
                        character.styles.link_anchor.is_some(),
                        "URL in lines_above at position {} should have link anchor",
                        i
                    );
                }
            }
        }
    }

    #[test]
    fn test_link_handler_increments_ids() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url1 = "http://first.com";
        populate_row_with_text(&mut viewport[0], url1, 0);

        for (i, ch) in url1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }
        let cursor = create_test_cursor(url1.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let url2 = "https://second.com";
        populate_row_with_text(&mut viewport[1], url2, 0);

        for (i, ch) in url2.chars().enumerate() {
            let cursor = create_test_cursor(i, 1);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }
        let cursor = create_test_cursor(url2.len(), 1);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let links = link_handler.links();

        assert_eq!(links.len(), 2, "Should have 2 links stored");

        let link_0 = links.get(&0);
        let link_1 = links.get(&1);

        assert!(link_0.is_some(), "Should have link with ID 0");
        assert!(link_1.is_some(), "Should have link with ID 1");

        if let Some(link) = link_0 {
            assert_eq!(link.uri, url1, "First link should have first URL");
            assert_eq!(
                link.id,
                Some("0".to_string()),
                "First link should have ID '0'"
            );
        }

        if let Some(link) = link_1 {
            assert_eq!(link.uri, url2, "Second link should have second URL");
            assert_eq!(
                link.id,
                Some("1".to_string()),
                "Second link should have ID '1'"
            );
        }
    }

    #[test]
    fn test_link_anchor_types() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let mut link_handler = LinkHandler::new();

        let url = "http://test.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(
                ch,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &mut link_handler,
            );
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            ' ',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &mut link_handler,
        );

        let row = &viewport[0];

        let first_char_index = row.absolute_character_index(0);
        if let Some(character) = row.columns.get(first_char_index) {
            assert!(
                character.styles.link_anchor.is_some(),
                "First character should have link anchor"
            );
            if let Some(ref anchor) = character.styles.link_anchor {
                match anchor {
                    LinkAnchor::Start(id) => {
                        let links = link_handler.links();
                        let stored_link = links.get(id);
                        assert!(
                            stored_link.is_some(),
                            "Link ID {} should exist in LinkHandler",
                            id
                        );
                        if let Some(link) = stored_link {
                            assert_eq!(link.uri, url, "Link should contain the correct URL");
                        }
                    },
                    _ => panic!("First character should have Start anchor, got {:?}", anchor),
                }
            }
        }

        let mut expected_link_id = None;
        for i in 0..url.len() {
            let char_index = row.absolute_character_index(i);
            if let Some(character) = row.columns.get(char_index) {
                assert!(
                    character.styles.link_anchor.is_some(),
                    "URL character at position {} should have link anchor",
                    i
                );

                if let Some(ref anchor) = character.styles.link_anchor {
                    match anchor {
                        LinkAnchor::Start(id) => {
                            if expected_link_id.is_none() {
                                expected_link_id = Some(*id);
                            } else {
                                assert_eq!(
                                    expected_link_id.unwrap(),
                                    *id,
                                    "All characters should have the same link ID"
                                );
                            }
                        },
                        LinkAnchor::End => {
                            if i != url.len().saturating_sub(1) {
                                panic!("Only the last character should have End anchor");
                            }
                        },
                    }
                }
            }
        }
    }
}
