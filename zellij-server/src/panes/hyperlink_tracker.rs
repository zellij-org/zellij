use crate::panes::grid::Row;
use crate::panes::link_handler::LinkHandler;
use crate::panes::terminal_character::{Cursor, LinkAnchor};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

#[derive(Debug, Clone)]
struct DetectedLink {
    url: String,
    cells: Vec<HyperlinkPosition>,
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

    #[inline]
    pub fn update(
        &mut self,
        ch: char,
        cursor: &Cursor,
        viewport: &mut VecDeque<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &Rc<RefCell<LinkHandler>>,
    ) {
        if ch == ' ' && cursor.x == 0 {
            // skip carriage return
            return;
        }
        self.update_while_relevant(ch, cursor, viewport, lines_above, link_handler);
    }

    #[inline]
    pub fn can_begin_a_url(ch: char) -> bool {
        matches!(ch, 'h' | 'f' | 'm')
    }

    #[inline]
    pub fn is_idle(&self) -> bool {
        self.start_position.is_none()
    }

    fn update_while_relevant(
        &mut self,
        ch: char,
        cursor: &Cursor,
        viewport: &mut VecDeque<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &Rc<RefCell<LinkHandler>>,
    ) {
        let current_pos = HyperlinkPosition::from_cursor(cursor);

        if self.should_reset_due_to_cursor_jump(&current_pos, viewport) {
            if self.is_currently_tracking() {
                // Finalize the current URL before resetting
                self.finalize_and_apply(viewport, lines_above, &mut link_handler.borrow_mut());
            } else {
                self.clear();
            }
        }

        if self.is_currently_tracking() {
            if self.is_url_terminator(ch) {
                self.finalize_and_apply(viewport, lines_above, &mut link_handler.borrow_mut());
            } else {
                self.buffer.push(ch);
                self.cursor_positions.push(current_pos.clone());
            }
        } else {
            if Self::can_begin_a_url(ch) {
                self.buffer.push(ch);
                self.cursor_positions.push(current_pos.clone());
                self.start_position = Some(current_pos.clone());
            }
        }

        self.last_cursor = Some(current_pos);
    }

    pub fn offset_cursor_lines_in_range(&mut self, top: isize, bottom: isize, offset: isize) {
        // Offset only positions inside the given row range (a scroll region),
        // used when a scroll region that does not start at the top of the
        // screen scrolls: rows outside it do not move
        for pos in &mut self.cursor_positions {
            if pos.y >= top && pos.y <= bottom {
                pos.y -= offset;
            }
        }
        if let Some(start_pos) = &mut self.start_position {
            if start_pos.y >= top && start_pos.y <= bottom {
                start_pos.y -= offset;
            }
        }
        if let Some(last_cursor) = &mut self.last_cursor {
            if last_cursor.y >= top && last_cursor.y <= bottom {
                last_cursor.y -= offset;
            }
        }
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

    fn should_reset_due_to_cursor_jump(
        &self,
        current_pos: &HyperlinkPosition,
        viewport: &VecDeque<Row>,
    ) -> bool {
        if let Some(last_pos) = &self.last_cursor {
            let is_contiguous = (current_pos.y == last_pos.y && current_pos.x == last_pos.x + 1)
                || (current_pos.y == last_pos.y + 1
                    && current_pos.x == 0
                    && continues_the_line_above(viewport, current_pos.y))
                || (current_pos.y == last_pos.y && current_pos.x == last_pos.x);

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
            let kept = if chars_trimmed > 0 {
                trimmed_len
            } else {
                self.cursor_positions.len()
            };
            let cells: Vec<HyperlinkPosition> =
                self.cursor_positions.iter().take(kept).copied().collect();
            if cells.is_empty() {
                self.clear();
                return;
            }

            let detected_link = DetectedLink {
                url: trimmed_url.clone(),
                cells,
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
        let Some(last) = link.cells.last().copied() else {
            return;
        };

        for cell in &link.cells {
            let Some(row) = row_at(viewport, lines_above, cell.y) else {
                continue;
            };
            let index = row.absolute_character_index(cell.x.max(0) as usize);
            if let Some(character) = row.columns.get_mut(index) {
                character
                    .styles
                    .update(|styles| styles.link_anchor = Some(link_anchor_start.clone()));
            }
        }

        let Some(row) = row_at(viewport, lines_above, last.y) else {
            return;
        };
        let after = row.absolute_character_index(last.x.max(0) as usize + 1);
        if let Some(character) = row.columns.get_mut(after) {
            if character.styles.link_anchor.is_none() {
                character
                    .styles
                    .update(|styles| styles.link_anchor = Some(LinkAnchor::End));
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

fn row_at<'a>(
    viewport: &'a mut VecDeque<Row>,
    lines_above: &'a mut VecDeque<Row>,
    y: isize,
) -> Option<&'a mut Row> {
    if y < 0 {
        let index = lines_above.len() as isize + y;
        return usize::try_from(index)
            .ok()
            .and_then(|index| lines_above.get_mut(index));
    }
    viewport.get_mut(y as usize)
}

fn continues_the_line_above(viewport: &VecDeque<Row>, y: isize) -> bool {
    if y < 0 {
        return false;
    }
    viewport
        .get(y as usize)
        .map(|row| !row.is_canonical)
        .unwrap_or(false)
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
        let mut columns = Vec::new();
        for _ in 0..width {
            columns.push(TerminalCharacter::new(' '));
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

    fn wrapped_viewport(rows: usize, cols: usize) -> VecDeque<Row> {
        let mut viewport = create_test_viewport(rows, cols);
        for row in viewport.iter_mut().skip(1) {
            row.is_canonical = false;
        }
        viewport
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "http://example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
            let binding = link_handler.borrow();
            let links = binding.links();
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "https://secure.example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
            let binding = link_handler.borrow();
            let links = binding.links();
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "ftp://files.example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(
            '\n',
            &cursor,
            &mut viewport,
            &mut lines_above,
            &link_handler,
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "mailto:user@example.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url_with_punct = "http://example.com.";
        let expected_trimmed_url = "http://example.com";

        populate_row_with_text(&mut viewport[0], url_with_punct, 0);

        for (i, ch) in url_with_punct.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url_with_punct.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

        let row = &viewport[0];
        let mut link_id = None;

        let first_char_index = row.absolute_character_index(0);
        if let Some(character) = row.columns.get(first_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                link_id = Some(*id);
            }
        }
        if let Some(id) = link_id {
            let binding = link_handler.borrow();
            let links = binding.links();
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let short_url = "http://";

        populate_row_with_text(&mut viewport[0], short_url, 0);

        for (i, ch) in short_url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(short_url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let partial_url = "http://exam";
        for (i, ch) in partial_url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        assert!(tracker.is_currently_tracking());

        let cursor = create_test_cursor(50, 5);
        tracker.update('h', &cursor, &mut viewport, &mut lines_above, &link_handler);

        assert_eq!(tracker.buffer, "h");
        assert_eq!(tracker.cursor_positions.len(), 1);
    }

    #[test]
    fn test_line_wrap_continuation() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = wrapped_viewport(10, 80);
        let mut lines_above = VecDeque::new();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let cursor1 = create_test_cursor(79, 0);
        tracker.update(
            'h',
            &cursor1,
            &mut viewport,
            &mut lines_above,
            &link_handler,
        );

        let cursor2 = create_test_cursor(0, 1);
        tracker.update(
            't',
            &cursor2,
            &mut viewport,
            &mut lines_above,
            &link_handler,
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let cursor = create_test_cursor(0, 5);
        tracker.update('h', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url_part1 = "http://very-long-";
        let url_part2 = "domain.example.com";
        let full_url = format!("{}{}", url_part1, url_part2);
        viewport[1].is_canonical = false;

        populate_row_with_text(&mut viewport[0], url_part1, 0);
        populate_row_with_text(&mut viewport[1], url_part2, 0);

        for (i, ch) in url_part1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        for (i, ch) in url_part2.chars().enumerate() {
            let cursor = create_test_cursor(i, 1);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url_part2.len(), 1);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

        let row0 = &viewport[0];
        let mut link_id = None;

        let first_char_index = row0.absolute_character_index(0);
        if let Some(character) = row0.columns.get(first_char_index) {
            if let Some(LinkAnchor::Start(id)) = &character.styles.link_anchor {
                link_id = Some(*id);
            }
        }

        if let Some(id) = link_id {
            let binding = link_handler.borrow();
            let links = binding.links();
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

    fn anchors(row: &Row, cols: usize) -> Vec<Option<LinkAnchor>> {
        (0..cols)
            .map(|column| {
                let index = row.absolute_character_index(column);
                row.columns
                    .get(index)
                    .and_then(|character| character.styles.link_anchor)
            })
            .collect()
    }

    fn feed(
        tracker: &mut HyperlinkTracker,
        viewport: &mut VecDeque<Row>,
        lines_above: &mut VecDeque<Row>,
        link_handler: &Rc<RefCell<LinkHandler>>,
        text: &str,
        x: usize,
        y: usize,
    ) {
        populate_row_with_text(&mut viewport[y], text, x);
        for (offset, character) in text.chars().enumerate() {
            let cursor = create_test_cursor(x + offset, y);
            tracker.update(character, &cursor, viewport, lines_above, link_handler);
        }
    }

    #[test]
    fn a_new_line_after_a_url_does_not_continue_it() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 40);
        let mut lines_above = VecDeque::new();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "https://www.fastmail.com";
        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            url,
            0,
            0,
        );
        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            "2",
            0,
            1,
        );

        let stored = link_handler.borrow().links();
        let uris: Vec<String> = stored.values().map(|link| link.uri.clone()).collect();
        assert_eq!(
            uris,
            vec![url.to_owned()],
            "the digit on the line below must not join the url"
        );

        let marked = anchors(&viewport[0], 40);
        assert!(
            marked[..url.len()]
                .iter()
                .all(|anchor| matches!(anchor, Some(LinkAnchor::Start(_)))),
            "every character of the url belongs to the link: {:?}",
            marked
        );
        assert_eq!(
            marked[url.len()],
            Some(LinkAnchor::End),
            "the cell after the url closes it"
        );
        assert!(
            marked[url.len() + 1..]
                .iter()
                .all(|anchor| anchor.is_none()),
            "the rest of the line is not part of the link: {:?}",
            marked
        );
        assert!(
            anchors(&viewport[1], 40)
                .iter()
                .all(|anchor| anchor.is_none()),
            "the line below carries no part of the link"
        );
    }

    #[test]
    fn a_url_that_really_wrapped_keeps_both_of_its_rows() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 20);
        viewport[1].is_canonical = false;
        let mut lines_above = VecDeque::new();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            "https://example.com/",
            0,
            0,
        );
        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            "path x",
            0,
            1,
        );

        let stored = link_handler.borrow().links();
        let uris: Vec<String> = stored.values().map(|link| link.uri.clone()).collect();
        assert_eq!(uris, vec!["https://example.com/path".to_owned()]);

        assert!(
            anchors(&viewport[0], 20)
                .iter()
                .all(|anchor| matches!(anchor, Some(LinkAnchor::Start(_)))),
            "the wrapped row is link to its last column"
        );
        let second = anchors(&viewport[1], 20);
        assert!(second[..4]
            .iter()
            .all(|anchor| matches!(anchor, Some(LinkAnchor::Start(_)))));
        assert_eq!(second[4], Some(LinkAnchor::End));
        assert!(second[5..].iter().all(|anchor| anchor.is_none()));
    }

    #[test]
    fn the_last_character_of_a_url_is_part_of_the_link() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 40);
        let mut lines_above = VecDeque::new();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "https://example.com";
        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            &format!("{} ", url),
            0,
            0,
        );

        let marked = anchors(&viewport[0], 40);
        assert!(
            matches!(marked[url.len() - 1], Some(LinkAnchor::Start(_))),
            "the final character of the url used to be left outside the link"
        );
        assert_eq!(marked[url.len()], Some(LinkAnchor::End));
    }

    #[test]
    fn the_terminator_does_not_clobber_the_link_that_follows_the_url() {
        let mut tracker = HyperlinkTracker::new();
        let mut viewport = create_test_viewport(10, 40);
        let mut lines_above = VecDeque::new();
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "https://example.com";
        let neighbour = link_handler
            .borrow_mut()
            .dispatch_osc8(&[b"8", b"", b"https://example.com/neighbour"])
            .expect("an osc 8 anchor");
        let after = viewport[0].absolute_character_index(url.len());
        viewport[0].columns[after]
            .styles
            .update(|styles| styles.link_anchor = Some(neighbour));

        feed(
            &mut tracker,
            &mut viewport,
            &mut lines_above,
            &link_handler,
            &format!("{} ", url),
            0,
            0,
        );

        assert_eq!(
            anchors(&viewport[0], 40)[url.len()],
            Some(neighbour),
            "the cell after the detected url already belonged to another link"
        );
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
            let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

            let url = "http://example.com";

            populate_row_with_text(&mut viewport[idx], url, 0);

            for (i, ch) in url.chars().enumerate() {
                let cursor = create_test_cursor(i, idx);
                tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
            }

            let cursor = create_test_cursor(url.len(), idx);
            tracker.update(
                *terminator,
                &cursor,
                &mut viewport,
                &mut lines_above,
                &link_handler,
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let cursor = create_test_cursor(0, 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let wrapped = wrapped_viewport(10, 80);
        let fresh_lines = create_test_viewport(10, 80);

        tracker.last_cursor = Some(HyperlinkPosition { x: 5, y: 2 });

        let next_col = HyperlinkPosition { x: 6, y: 2 };
        assert!(!tracker.should_reset_due_to_cursor_jump(&next_col, &wrapped));

        let next_line = HyperlinkPosition { x: 0, y: 3 };
        assert!(
            !tracker.should_reset_due_to_cursor_jump(&next_line, &wrapped),
            "a row that continues the one above it is a wrap"
        );
        assert!(
            tracker.should_reset_due_to_cursor_jump(&next_line, &fresh_lines),
            "a row of its own is a new line, not a wrap"
        );

        let same_pos = HyperlinkPosition { x: 5, y: 2 };
        assert!(!tracker.should_reset_due_to_cursor_jump(&same_pos, &wrapped));

        let jump = HyperlinkPosition { x: 10, y: 5 };
        assert!(tracker.should_reset_due_to_cursor_jump(&jump, &wrapped));
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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url1 = "http://first.com";
        let url2 = "https://second.com";
        let full_text = format!("{} {}", url1, url2);

        populate_row_with_text(&mut viewport[0], &full_text, 0);

        for (i, ch) in url1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url1.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

        for (i, ch) in url2.chars().enumerate() {
            let cursor = create_test_cursor(url1.len() + 1 + i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url1.len() + 1 + url2.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let binding = link_handler.borrow();
        let links = binding.links();

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

        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "http://example.com";
        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        tracker.offset_cursor_lines(2);

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url1 = "http://first.com";
        populate_row_with_text(&mut viewport[0], url1, 0);

        for (i, ch) in url1.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }
        let cursor = create_test_cursor(url1.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

        let url2 = "https://second.com";
        populate_row_with_text(&mut viewport[1], url2, 0);

        for (i, ch) in url2.chars().enumerate() {
            let cursor = create_test_cursor(i, 1);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }
        let cursor = create_test_cursor(url2.len(), 1);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

        let binding = link_handler.borrow();
        let links = binding.links();

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
        let link_handler = Rc::new(RefCell::new(LinkHandler::new()));

        let url = "http://test.com";

        populate_row_with_text(&mut viewport[0], url, 0);

        for (i, ch) in url.chars().enumerate() {
            let cursor = create_test_cursor(i, 0);
            tracker.update(ch, &cursor, &mut viewport, &mut lines_above, &link_handler);
        }

        let cursor = create_test_cursor(url.len(), 0);
        tracker.update(' ', &cursor, &mut viewport, &mut lines_above, &link_handler);

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
                        let binding = link_handler.borrow();
                        let links = binding.links();
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
