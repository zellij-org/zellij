use zellij_tile::prelude::*;

const MAX_UNDO_STACK_SIZE: usize = 100;

/// Action returned by TextInput after handling a key event
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(test, derive(Eq))]
pub enum InputAction {
    /// Continue editing
    Continue,
    /// User pressed Enter to submit
    Submit,
    /// User pressed Esc to cancel
    Cancel,
    /// User pressed Tab to request completion
    Complete,
    /// Key was not handled by the input
    NoAction,
}

/// A reusable text input component with cursor support and standard editing keybindings
#[derive(Debug, Clone)]
pub struct TextInput {
    buffer: String,
    cursor_position: usize, // Character position (0-based), NOT byte position
    undo_stack: Vec<(String, usize)>, // (buffer, cursor) snapshots
    redo_stack: Vec<(String, usize)>,
    last_edit_was_insert: bool, // For coalescing consecutive inserts
}

impl TextInput {
    /// Create a new TextInput with the given initial text
    /// Cursor is positioned at the end of the text
    pub fn new(initial_text: String) -> Self {
        let cursor_position = initial_text.chars().count();
        Self {
            buffer: initial_text,
            cursor_position,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_was_insert: false,
        }
    }

    /// Create an empty TextInput
    pub fn empty() -> Self {
        Self::new(String::new())
    }

    /// Get the current text
    pub fn get_text(&self) -> &str {
        &self.buffer
    }

    /// Get the cursor position (in characters, not bytes)
    pub fn get_cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Check if the input is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get a shorthand for cursor_position
    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Get mutable access to the underlying buffer for direct manipulation
    pub fn get_text_mut(&mut self) -> &mut String {
        &mut self.buffer
    }

    /// Set the text and move cursor to the end
    pub fn set_text(&mut self, text: String) {
        self.break_coalescing();
        self.save_undo_state();
        self.cursor_position = text.chars().count();
        self.buffer = text;
    }

    /// Set cursor position (clamped to text length)
    pub fn set_cursor_position(&mut self, pos: usize) {
        let text_len = self.buffer.chars().count();
        self.cursor_position = pos.min(text_len);
    }

    /// Clear all text and reset cursor
    pub fn clear(&mut self) {
        self.break_coalescing();
        self.save_undo_state();
        self.buffer.clear();
        self.cursor_position = 0;
    }

    /// Insert a character at the current cursor position
    pub fn insert_char(&mut self, c: char) {
        self.save_undo_state_unless_coalescing();
        // Convert cursor position (char index) to byte index
        let byte_index = self.char_index_to_byte_index(self.cursor_position);
        self.buffer.insert(byte_index, c);
        self.cursor_position += 1;
    }

    /// Delete the character before the cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor_position > 0 {
            self.break_coalescing();
            self.save_undo_state();
            self.cursor_position -= 1;
            let byte_index = self.char_index_to_byte_index(self.cursor_position);
            self.buffer.remove(byte_index);
        }
    }

    /// Delete the character at the cursor position (delete key)
    pub fn delete(&mut self) {
        let len = self.buffer.chars().count();
        if self.cursor_position < len {
            self.break_coalescing();
            self.save_undo_state();
            let byte_index = self.char_index_to_byte_index(self.cursor_position);
            self.buffer.remove(byte_index);
        }
    }

    /// Delete the word before the cursor (Ctrl/Alt + Backspace)
    pub fn delete_word_backward(&mut self) {
        if self.cursor_position == 0 {
            return;
        }

        self.break_coalescing();
        self.save_undo_state();

        let old_position = self.cursor_position;
        self.move_word_left();
        let new_position = self.cursor_position;

        // Delete from new position to old position
        let start_byte = self.char_index_to_byte_index(new_position);
        let end_byte = self.char_index_to_byte_index(old_position);
        self.buffer.drain(start_byte..end_byte);
    }

    /// Delete the word after the cursor (Ctrl/Alt + Delete)
    pub fn delete_word_forward(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let len = chars.len();

        if self.cursor_position >= len {
            return;
        }

        self.break_coalescing();
        self.save_undo_state();

        let start_position = self.cursor_position;
        let mut end_position = start_position;

        // Skip the current word
        while end_position < len && !chars[end_position].is_whitespace() {
            end_position += 1;
        }

        // Skip any whitespace after the word
        while end_position < len && chars[end_position].is_whitespace() {
            end_position += 1;
        }

        // Delete from start to end position
        let start_byte = self.char_index_to_byte_index(start_position);
        let end_byte = self.char_index_to_byte_index(end_position);
        self.buffer.drain(start_byte..end_byte);
    }

    /// Move cursor one position to the left
    pub fn move_left(&mut self) {
        if self.cursor_position > 0 {
            self.break_coalescing();
            self.cursor_position -= 1;
        }
    }

    /// Move cursor one position to the right
    pub fn move_right(&mut self) {
        let len = self.buffer.chars().count();
        if self.cursor_position < len {
            self.break_coalescing();
            self.cursor_position += 1;
        }
    }

    /// Move cursor to the start of the text (Ctrl-A / Home)
    pub fn move_to_start(&mut self) {
        self.break_coalescing();
        self.cursor_position = 0;
    }

    /// Move cursor to the end of the text (Ctrl-E / End)
    pub fn move_to_end(&mut self) {
        self.break_coalescing();
        self.cursor_position = self.buffer.chars().count();
    }

    /// Move cursor to the start of the previous word (Ctrl/Alt + Left)
    pub fn move_word_left(&mut self) {
        if self.cursor_position == 0 {
            return;
        }

        self.break_coalescing();

        let chars: Vec<char> = self.buffer.chars().collect();
        let mut pos = self.cursor_position;

        // Skip any whitespace immediately to the left
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        // Skip the word characters
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }

        self.cursor_position = pos;
    }

    /// Move cursor to the start of the next word (Ctrl/Alt + Right)
    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let len = chars.len();

        if self.cursor_position >= len {
            return;
        }

        self.break_coalescing();

        let mut pos = self.cursor_position;

        // Skip the current word
        while pos < len && !chars[pos].is_whitespace() {
            pos += 1;
        }

        // Skip any whitespace
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }

        self.cursor_position = pos;
    }

    /// Handle a key event and return the appropriate action
    /// This is the main entry point for key handling
    pub fn handle_key(&mut self, key: KeyWithModifier) -> InputAction {
        // Check for Ctrl modifiers
        if key.has_modifiers(&[KeyModifier::Ctrl]) {
            match key.bare_key {
                BareKey::Char('a') => {
                    self.move_to_start();
                    return InputAction::Continue;
                },
                BareKey::Char('e') => {
                    self.move_to_end();
                    return InputAction::Continue;
                },
                BareKey::Char('c') => {
                    // Ctrl-C clears the prompt
                    return InputAction::Cancel;
                },
                BareKey::Char('z') => {
                    // Ctrl-Z: Undo
                    self.undo();
                    return InputAction::Continue;
                },
                BareKey::Char('y') => {
                    // Ctrl-Y: Redo
                    self.redo();
                    return InputAction::Continue;
                },
                BareKey::Left => {
                    self.move_word_left();
                    return InputAction::Continue;
                },
                BareKey::Right => {
                    self.move_word_right();
                    return InputAction::Continue;
                },
                BareKey::Backspace => {
                    self.delete_word_backward();
                    return InputAction::Continue;
                },
                BareKey::Delete => {
                    self.delete_word_forward();
                    return InputAction::Continue;
                },
                _ => {},
            }
        }

        // Check for Ctrl+Shift modifiers (alternative redo: Ctrl+Shift+Z)
        if key.has_modifiers(&[KeyModifier::Ctrl, KeyModifier::Shift]) {
            match key.bare_key {
                BareKey::Char('Z') => {
                    // Ctrl-Shift-Z: Redo (alternative)
                    self.redo();
                    return InputAction::Continue;
                },
                _ => {},
            }
        }

        // Check for Alt modifiers
        if key.has_modifiers(&[KeyModifier::Alt]) {
            match key.bare_key {
                BareKey::Left => {
                    self.move_word_left();
                    return InputAction::Continue;
                },
                BareKey::Right => {
                    self.move_word_right();
                    return InputAction::Continue;
                },
                BareKey::Backspace => {
                    self.delete_word_backward();
                    return InputAction::Continue;
                },
                BareKey::Delete => {
                    self.delete_word_forward();
                    return InputAction::Continue;
                },
                _ => {},
            }
        }

        // Handle bare keys (no modifiers)
        match key.bare_key {
            BareKey::Enter => InputAction::Submit,
            BareKey::Esc => InputAction::Cancel,
            BareKey::Tab => InputAction::Complete,
            BareKey::Backspace => {
                self.backspace();
                InputAction::Continue
            },
            BareKey::Delete => {
                self.delete();
                InputAction::Continue
            },
            BareKey::Left => {
                self.move_left();
                InputAction::Continue
            },
            BareKey::Right => {
                self.move_right();
                InputAction::Continue
            },
            BareKey::Home => {
                self.move_to_start();
                InputAction::Continue
            },
            BareKey::End => {
                self.move_to_end();
                InputAction::Continue
            },
            BareKey::Char(c) => {
                self.insert_char(c);
                InputAction::Continue
            },
            _ => InputAction::NoAction,
        }
    }

    /// Helper: Convert character index to byte index
    fn char_index_to_byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.buffer.len())
    }

    /// Save current state to undo stack before making changes
    fn save_undo_state(&mut self) {
        if self.undo_stack.len() >= MAX_UNDO_STACK_SIZE {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push((self.buffer.clone(), self.cursor_position));
        self.redo_stack.clear();
    }

    /// Save state only if not coalescing with previous insert
    fn save_undo_state_unless_coalescing(&mut self) {
        if !self.last_edit_was_insert {
            self.save_undo_state();
        }
        self.last_edit_was_insert = true;
    }

    /// Mark that a non-insert edit occurred (breaks coalescing)
    fn break_coalescing(&mut self) {
        self.last_edit_was_insert = false;
    }

    /// Undo last change
    pub fn undo(&mut self) -> bool {
        if let Some((buffer, cursor)) = self.undo_stack.pop() {
            self.redo_stack
                .push((self.buffer.clone(), self.cursor_position));
            self.buffer = buffer;
            self.cursor_position = cursor;
            self.break_coalescing();
            true
        } else {
            false
        }
    }

    /// Redo last undone change
    pub fn redo(&mut self) -> bool {
        if let Some((buffer, cursor)) = self.redo_stack.pop() {
            self.undo_stack
                .push((self.buffer.clone(), self.cursor_position));
            self.buffer = buffer;
            self.cursor_position = cursor;
            self.break_coalescing();
            true
        } else {
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn drain_text(&mut self) -> String {
        self.cursor_position = 0;
        self.buffer.drain(..).collect()
    }
}

// run with:
// cargo test --lib --target x86_64-unknown-linux-gnu
//
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_empty() {
        let input = TextInput::new("hello".to_string());
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 5);

        let empty = TextInput::empty();
        assert_eq!(empty.get_text(), "");
        assert_eq!(empty.get_cursor_position(), 0);
    }

    #[test]
    fn test_insert_char() {
        let mut input = TextInput::new("helo".to_string());
        input.cursor_position = 3; // Position after "hel"
        input.insert_char('l');
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 4);
    }

    #[test]
    fn test_backspace() {
        let mut input = TextInput::new("hello".to_string());
        input.backspace();
        assert_eq!(input.get_text(), "hell");
        assert_eq!(input.get_cursor_position(), 4);

        // Backspace at start does nothing
        input.cursor_position = 0;
        input.backspace();
        assert_eq!(input.get_text(), "hell");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_delete() {
        let mut input = TextInput::new("hello".to_string());
        input.cursor_position = 0;
        input.delete();
        assert_eq!(input.get_text(), "ello");
        assert_eq!(input.get_cursor_position(), 0);

        // Delete at end does nothing
        input.move_to_end();
        input.delete();
        assert_eq!(input.get_text(), "ello");
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = TextInput::new("hello".to_string());
        assert_eq!(input.get_cursor_position(), 5);

        input.move_left();
        assert_eq!(input.get_cursor_position(), 4);

        input.move_right();
        assert_eq!(input.get_cursor_position(), 5);

        input.move_to_start();
        assert_eq!(input.get_cursor_position(), 0);

        input.move_to_end();
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_unicode_support() {
        let mut input = TextInput::new("hello 🦀 world".to_string());
        assert_eq!(input.get_cursor_position(), 13); // 13 characters

        input.cursor_position = 6; // After "hello "
        input.insert_char('🐱');
        assert_eq!(input.get_text(), "hello 🐱🦀 world");
    }

    #[test]
    fn test_word_jump_right() {
        let mut input = TextInput::new("hello world foo bar".to_string());
        input.cursor_position = 0;

        // Jump from start to "world"
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 6); // After "hello "

        // Jump to "foo"
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 12); // After "world "

        // Jump to "bar"
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 16); // After "foo "

        // Jump to end
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 19); // At end
    }

    #[test]
    fn test_word_jump_left() {
        let mut input = TextInput::new("hello world foo bar".to_string());
        input.move_to_end();
        assert_eq!(input.get_cursor_position(), 19);

        // Jump back to "bar"
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 16); // Start of "bar"

        // Jump back to "foo"
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 12); // Start of "foo"

        // Jump back to "world"
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 6); // Start of "world"

        // Jump back to "hello"
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0); // Start of "hello"
    }

    #[test]
    fn test_word_jump_with_multiple_spaces() {
        let mut input = TextInput::new("hello   world".to_string());
        input.cursor_position = 0;

        // Jump over multiple spaces
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 8); // After "hello   ", at start of "world"

        // Jump back should skip spaces
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0); // Back to start of "hello"
    }

    #[test]
    fn test_word_jump_boundaries() {
        let mut input = TextInput::new("test".to_string());

        // At start - word left does nothing
        input.cursor_position = 0;
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0);

        // At end - word right does nothing
        input.move_to_end();
        let end_pos = input.get_cursor_position();
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), end_pos);
    }

    #[test]
    fn test_up_down_arrows() {
        let mut input = TextInput::new("hello world".to_string());

        // Start in the middle
        input.cursor_position = 5;
        assert_eq!(input.get_cursor_position(), 5);

        // Up arrow should go to start
        input.move_to_start();
        assert_eq!(input.get_cursor_position(), 0);

        // Move back to middle
        input.cursor_position = 5;

        // Down arrow should go to end
        input.move_to_end();
        assert_eq!(input.get_cursor_position(), 11);
    }

    #[test]
    fn test_delete_word_backward() {
        let mut input = TextInput::new("hello world foo".to_string());

        // Delete "foo" from end
        input.move_to_end();
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello world ");
        assert_eq!(input.get_cursor_position(), 12);

        // Delete "world "
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello ");
        assert_eq!(input.get_cursor_position(), 6);

        // Delete "hello "
        input.delete_word_backward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        // Delete on empty buffer does nothing
        input.delete_word_backward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_delete_word_backward_middle() {
        let mut input = TextInput::new("hello world foo".to_string());

        // Position in middle of "world"
        input.cursor_position = 8; // After "hello wo"
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello rld foo");
        assert_eq!(input.get_cursor_position(), 6); // After "hello "
    }

    #[test]
    fn test_delete_word_forward() {
        let mut input = TextInput::new("hello world foo".to_string());

        // Delete "hello " from start
        input.cursor_position = 0;
        input.delete_word_forward();
        assert_eq!(input.get_text(), "world foo");
        assert_eq!(input.get_cursor_position(), 0);

        // Delete "world "
        input.delete_word_forward();
        assert_eq!(input.get_text(), "foo");
        assert_eq!(input.get_cursor_position(), 0);

        // Delete "foo"
        input.delete_word_forward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        // Delete on empty buffer does nothing
        input.delete_word_forward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_delete_word_forward_middle() {
        let mut input = TextInput::new("hello world foo".to_string());

        // Position in middle of "world"
        input.cursor_position = 8; // After "hello wo"
        input.delete_word_forward();
        assert_eq!(input.get_text(), "hello wofoo");
        assert_eq!(input.get_cursor_position(), 8); // Same position, text deleted forward
    }

    #[test]
    fn test_delete_word_with_multiple_spaces() {
        let mut input = TextInput::new("hello   world".to_string());

        // Delete forward includes trailing spaces
        input.cursor_position = 0;
        input.delete_word_forward();
        assert_eq!(input.get_text(), "world");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_undo_redo_basic() {
        let mut input = TextInput::empty();

        // Type "hello"
        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');
        assert_eq!(input.get_text(), "hello");

        // Undo should remove all characters (coalesced into one undo entry)
        assert!(input.can_undo());
        assert!(input.undo());
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        // Redo should restore "hello"
        assert!(input.can_redo());
        assert!(input.redo());
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_undo_coalescing_breaks_on_cursor_move() {
        let mut input = TextInput::empty();

        // Type "he"
        input.insert_char('h');
        input.insert_char('e');

        // Move cursor to start (breaks coalescing)
        input.move_to_start();

        // Type "llo"
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');

        assert_eq!(input.get_text(), "llohe");

        // First undo removes "llo" (second coalesced group)
        input.undo();
        assert_eq!(input.get_text(), "he");

        // Second undo removes "he" (first coalesced group)
        input.undo();
        assert_eq!(input.get_text(), "");
    }

    #[test]
    fn test_undo_backspace() {
        let mut input = TextInput::new("hello".to_string());

        // Backspace once
        input.backspace();
        assert_eq!(input.get_text(), "hell");

        // Undo should restore "hello"
        input.undo();
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_undo_delete() {
        let mut input = TextInput::new("hello".to_string());
        input.cursor_position = 0;

        // Delete first character
        input.delete();
        assert_eq!(input.get_text(), "ello");

        // Undo should restore "hello"
        input.undo();
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_undo_word_delete() {
        let mut input = TextInput::new("hello world".to_string());

        // Delete "world" backward
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello ");

        // Undo should restore "hello world"
        input.undo();
        assert_eq!(input.get_text(), "hello world");
        assert_eq!(input.get_cursor_position(), 11);
    }

    #[test]
    fn test_undo_clear() {
        let mut input = TextInput::new("hello world".to_string());

        // Clear the buffer
        input.clear();
        assert_eq!(input.get_text(), "");

        // Undo should restore the text
        input.undo();
        assert_eq!(input.get_text(), "hello world");
    }

    #[test]
    fn test_undo_set_text() {
        let mut input = TextInput::new("hello".to_string());

        // Replace with new text
        input.set_text("goodbye".to_string());
        assert_eq!(input.get_text(), "goodbye");

        // Undo should restore "hello"
        input.undo();
        assert_eq!(input.get_text(), "hello");
    }

    #[test]
    fn test_redo_clears_on_new_edit() {
        let mut input = TextInput::empty();

        // Type "hello"
        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');

        // Undo
        input.undo();
        assert_eq!(input.get_text(), "");
        assert!(input.can_redo());

        // Make a new edit (should clear redo stack)
        input.insert_char('x');
        assert!(!input.can_redo());
    }

    #[test]
    fn test_multiple_undo_redo() {
        let mut input = TextInput::empty();

        // First edit: type "hello"
        for c in "hello".chars() {
            input.insert_char(c);
        }

        // Break coalescing
        input.move_left();

        // Second edit: type "world"
        for c in "world".chars() {
            input.insert_char(c);
        }

        assert_eq!(input.get_text(), "hellworldo");

        // Undo "world"
        input.undo();
        assert_eq!(input.get_text(), "hello");

        // Undo "hello"
        input.undo();
        assert_eq!(input.get_text(), "");

        // Redo "hello"
        input.redo();
        assert_eq!(input.get_text(), "hello");

        // Redo "world"
        input.redo();
        assert_eq!(input.get_text(), "hellworldo");
    }

    #[test]
    fn test_undo_stack_limit() {
        let mut input = TextInput::empty();

        // Perform 102 separate edits (breaking coalescing each time)
        for _i in 0..102 {
            input.backspace(); // Break coalescing
            input.insert_char('x');
        }

        // Should have at most 100 undo entries
        let mut undo_count = 0;
        while input.undo() {
            undo_count += 1;
        }

        // We should have 100 undo entries (the stack limit)
        // Plus the final state change from the last coalescing break
        assert!(
            undo_count <= 100,
            "Undo count should be at most 100, got {}",
            undo_count
        );
    }

    #[test]
    fn test_undo_redo_empty_stack() {
        let mut input = TextInput::empty();

        // Undo on empty stack should return false
        assert!(!input.can_undo());
        assert!(!input.undo());

        // Redo on empty stack should return false
        assert!(!input.can_redo());
        assert!(!input.redo());
    }

    #[test]
    fn test_undo_restores_cursor_position() {
        let mut input = TextInput::new("hello world".to_string());

        // Move cursor to position 5 (before "world")
        input.cursor_position = 5;

        // Insert a space
        input.insert_char(' ');
        assert_eq!(input.get_text(), "hello  world");
        assert_eq!(input.get_cursor_position(), 6);

        // Undo should restore both text and cursor position
        input.undo();
        assert_eq!(input.get_text(), "hello world");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_coalescing_consecutive_inserts() {
        let mut input = TextInput::empty();

        // Type several characters
        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        assert_eq!(input.get_text(), "abc");

        // Single undo should remove all three (they were coalesced)
        input.undo();
        assert_eq!(input.get_text(), "");

        // No more undo available
        assert!(!input.can_undo());
    }

    #[test]
    fn test_backspace_breaks_coalescing() {
        let mut input = TextInput::empty();

        // Type "ab"
        input.insert_char('a');
        input.insert_char('b');

        // Backspace
        input.backspace();
        assert_eq!(input.get_text(), "a");

        // Type "c"
        input.insert_char('c');
        assert_eq!(input.get_text(), "ac");

        // Undo should remove just "c"
        input.undo();
        assert_eq!(input.get_text(), "a");

        // Undo should remove backspace operation
        input.undo();
        assert_eq!(input.get_text(), "ab");

        // Undo should remove "ab"
        input.undo();
        assert_eq!(input.get_text(), "");
    }
}
