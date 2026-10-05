use super::widget_common::{
    char_width, default_label_width, encode_text, is_plain, state_flag, state_value, text_width,
    typed_char, update_hover, widget_dcs, Rect, UiResponse, UiValue, Widget,
};
use super::text::Text;
use std::fmt;
use std::rc::Rc;
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier, Mouse};

const MAX_UNDO_STACK_SIZE: usize = 100;
const MIN_FIELD_WIDTH: usize = 5;

pub type Validator = Rc<dyn Fn(&str) -> Result<(), String>>;

#[derive(Clone)]
pub struct TextInput {
    buffer: String,
    cursor_position: usize,
    undo_stack: Vec<(String, usize)>,
    redo_stack: Vec<(String, usize)>,
    last_edit_was_insert: bool,
    label: Text,
    label_width: Option<usize>,
    placeholder: Option<String>,
    validator: Option<Validator>,
    accept: Option<fn(char) -> bool>,
    search_mode: bool,
    match_count: Option<usize>,
    focused: bool,
    disabled: bool,
    hovered: bool,
    scroll_offset: usize,
    area: Option<Rect>,
    text_start_x: usize,
    text_area_width: usize,
    show_cursor: bool,
    accent_brackets: bool,
}

impl fmt::Debug for TextInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TextInput")
            .field("buffer", &self.buffer)
            .field("cursor_position", &self.cursor_position)
            .field("label", &self.label)
            .field("placeholder", &self.placeholder)
            .field("has_validator", &self.validator.is_some())
            .field("search_mode", &self.search_mode)
            .field("match_count", &self.match_count)
            .field("focused", &self.focused)
            .field("disabled", &self.disabled)
            .field("scroll_offset", &self.scroll_offset)
            .field("area", &self.area)
            .finish()
    }
}

impl TextInput {
    pub fn new(initial_text: impl Into<String>) -> Self {
        let initial_text = initial_text.into();
        let cursor_position = initial_text.chars().count();
        Self {
            buffer: initial_text,
            cursor_position,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_edit_was_insert: false,
            label: Text::default(),
            label_width: None,
            placeholder: None,
            validator: None,
            accept: None,
            search_mode: false,
            match_count: None,
            focused: false,
            disabled: false,
            hovered: false,
            scroll_offset: 0,
            area: None,
            text_start_x: 0,
            text_area_width: 0,
            show_cursor: true,
            accent_brackets: false,
        }
    }

    pub fn empty() -> Self {
        Self::new(String::new())
    }

    pub fn label(mut self, label: impl Into<Text>) -> Self {
        self.label = label.into();
        self
    }

    pub fn accent_brackets(mut self) -> Self {
        self.accent_brackets = true;
        self
    }

    pub fn label_width(mut self, label_width: usize) -> Self {
        self.label_width = Some(label_width);
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    pub fn validator(mut self, validator: impl Fn(&str) -> Result<(), String> + 'static) -> Self {
        self.validator = Some(Rc::new(validator));
        self
    }

    pub fn accept(mut self, accept: fn(char) -> bool) -> Self {
        self.accept = Some(accept);
        self
    }

    pub fn search_mode(mut self) -> Self {
        self.search_mode = true;
        self
    }

    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn hovered(mut self) -> Self {
        self.hovered = true;
        self
    }

    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }

    pub fn set_show_cursor(&mut self, show_cursor: bool) {
        self.show_cursor = show_cursor;
    }

    pub fn set_match_count(&mut self, match_count: Option<usize>) {
        self.match_count = match_count;
    }

    pub fn is_search_mode(&self) -> bool {
        self.search_mode
    }

    pub fn validation_error(&self) -> Option<String> {
        self.validator
            .as_ref()
            .and_then(|validator| validator(&self.buffer).err())
    }

    pub fn is_valid(&self) -> bool {
        self.validation_error().is_none()
    }

    pub fn height(&self) -> usize {
        if self.validator.is_some() {
            2
        } else {
            1
        }
    }

    pub fn get_text(&self) -> &str {
        &self.buffer
    }

    pub fn get_cursor_position(&self) -> usize {
        self.cursor_position
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    pub fn get_text_mut(&mut self) -> &mut String {
        &mut self.buffer
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.break_coalescing();
        self.save_undo_state();
        self.cursor_position = text.chars().count();
        self.buffer = text;
    }

    pub fn set_cursor_position(&mut self, pos: usize) {
        let text_len = self.buffer.chars().count();
        self.cursor_position = pos.min(text_len);
    }

    pub fn clear(&mut self) {
        self.break_coalescing();
        self.save_undo_state();
        self.buffer.clear();
        self.cursor_position = 0;
    }

    pub fn insert_char(&mut self, c: char) {
        self.save_undo_state_unless_coalescing();
        let byte_index = self.char_index_to_byte_index(self.cursor_position);
        self.buffer.insert(byte_index, c);
        self.cursor_position += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_position > 0 {
            self.break_coalescing();
            self.save_undo_state();
            self.cursor_position -= 1;
            let byte_index = self.char_index_to_byte_index(self.cursor_position);
            self.buffer.remove(byte_index);
        }
    }

    pub fn delete(&mut self) {
        let len = self.buffer.chars().count();
        if self.cursor_position < len {
            self.break_coalescing();
            self.save_undo_state();
            let byte_index = self.char_index_to_byte_index(self.cursor_position);
            self.buffer.remove(byte_index);
        }
    }

    pub fn delete_word_backward(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        self.break_coalescing();
        self.save_undo_state();
        let old_position = self.cursor_position;
        self.move_word_left();
        let new_position = self.cursor_position;
        let start_byte = self.char_index_to_byte_index(new_position);
        let end_byte = self.char_index_to_byte_index(old_position);
        self.buffer.drain(start_byte..end_byte);
    }

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
        while end_position < len && !chars[end_position].is_whitespace() {
            end_position += 1;
        }
        while end_position < len && chars[end_position].is_whitespace() {
            end_position += 1;
        }
        let start_byte = self.char_index_to_byte_index(start_position);
        let end_byte = self.char_index_to_byte_index(end_position);
        self.buffer.drain(start_byte..end_byte);
    }

    pub fn move_left(&mut self) {
        if self.cursor_position > 0 {
            self.break_coalescing();
            self.cursor_position -= 1;
        }
    }

    pub fn move_right(&mut self) {
        let len = self.buffer.chars().count();
        if self.cursor_position < len {
            self.break_coalescing();
            self.cursor_position += 1;
        }
    }

    pub fn move_to_start(&mut self) {
        self.break_coalescing();
        self.cursor_position = 0;
    }

    pub fn move_to_end(&mut self) {
        self.break_coalescing();
        self.cursor_position = self.buffer.chars().count();
    }

    pub fn move_word_left(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        self.break_coalescing();
        let chars: Vec<char> = self.buffer.chars().collect();
        let mut pos = self.cursor_position;
        while pos > 0 && chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        self.cursor_position = pos;
    }

    pub fn move_word_right(&mut self) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let len = chars.len();
        if self.cursor_position >= len {
            return;
        }
        self.break_coalescing();
        let mut pos = self.cursor_position;
        while pos < len && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < len && chars[pos].is_whitespace() {
            pos += 1;
        }
        self.cursor_position = pos;
    }

    fn char_index_to_byte_index(&self, char_index: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_index)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.buffer.len())
    }

    fn save_undo_state(&mut self) {
        if self.undo_stack.len() >= MAX_UNDO_STACK_SIZE {
            self.undo_stack.remove(0);
        }
        self.undo_stack
            .push((self.buffer.clone(), self.cursor_position));
        self.redo_stack.clear();
    }

    fn save_undo_state_unless_coalescing(&mut self) {
        if !self.last_edit_was_insert {
            self.save_undo_state();
        }
        self.last_edit_was_insert = true;
    }

    fn break_coalescing(&mut self) {
        self.last_edit_was_insert = false;
    }

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

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn drain_text(&mut self) -> String {
        self.cursor_position = 0;
        self.buffer.drain(..).collect()
    }

    fn effective_label_width(&self) -> usize {
        self.label_width
            .unwrap_or_else(|| default_label_width(self.label.content()))
    }

    fn suffix(&self) -> String {
        match self.match_count {
            Some(1) => "1 match".to_owned(),
            Some(count) => format!("{} matches", count),
            None => String::new(),
        }
    }

    fn keep_cursor_visible(&mut self, text_area_width: usize) {
        let chars: Vec<char> = self.buffer.chars().collect();
        let cursor = self.cursor_position.min(chars.len());
        if cursor < self.scroll_offset {
            self.scroll_offset = cursor;
        }
        let width_between = |from: usize, to: usize| -> usize {
            chars[from..to]
                .iter()
                .map(|c| char_width(*c))
                .sum::<usize>()
        };
        while self.scroll_offset < cursor
            && width_between(self.scroll_offset, cursor) + 1 > text_area_width
        {
            self.scroll_offset += 1;
        }
        while self.scroll_offset > 0
            && width_between(self.scroll_offset - 1, chars.len()) + 1 <= text_area_width
        {
            self.scroll_offset -= 1;
        }
    }

    fn visible_text(&self, text_area_width: usize) -> String {
        let mut visible = String::new();
        let mut used = 0;
        for character in self.buffer.chars().skip(self.scroll_offset) {
            let width = char_width(character);
            if used + width > text_area_width {
                break;
            }
            used += width;
            visible.push(character);
        }
        visible
    }

    pub fn serialize(&mut self, x: usize, y: usize, width: usize) -> String {
        let label_width = self.effective_label_width();
        let field_width = width.saturating_sub(label_width).max(MIN_FIELD_WIDTH);
        let inner_width = field_width - 4;
        let suffix = self.suffix();
        let suffix_width = if suffix.is_empty() || text_width(&suffix) + 2 > inner_width {
            0
        } else {
            text_width(&suffix) + 1
        };
        let text_area_width = inner_width - suffix_width;
        self.keep_cursor_visible(text_area_width);
        let height = self.height();
        self.area = Some(Rect::new(x, y, label_width + field_width, height));
        self.text_start_x = x + label_width + 2;
        self.text_area_width = text_area_width;
        let show_placeholder = self.buffer.is_empty() && self.placeholder.is_some();
        let error = self.validation_error();
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "d", self.disabled);
        state_flag(&mut state, "ph", show_placeholder);
        state_flag(&mut state, "err", error.is_some());
        state_flag(&mut state, "h", self.hovered && !self.disabled);
        state_flag(&mut state, "ab", self.accent_brackets);
        state_value(&mut state, "lw", label_width);
        if self.focused && self.show_cursor && !self.disabled {
            let cursor = if show_placeholder {
                0
            } else {
                self.cursor_position.saturating_sub(self.scroll_offset)
            };
            state_value(&mut state, "cur", cursor);
        }
        let text = if show_placeholder {
            self.placeholder.clone().unwrap_or_default()
        } else {
            self.visible_text(text_area_width)
        };
        widget_dcs(
            "text_input",
            x,
            y,
            Some(label_width + field_width),
            Some(height),
            &state,
            &[
                self.label.serialize(),
                encode_text(&text),
                encode_text(&error.unwrap_or_default()),
                encode_text(&suffix),
            ],
        )
    }

    pub fn render(&mut self, x: usize, y: usize, width: usize) {
        print!("{}", self.serialize(x, y, width));
    }

    fn edited(&mut self, before: &str) -> UiResponse {
        if self.buffer != before {
            UiResponse::Changed(UiValue::Text(self.buffer.clone()))
        } else {
            UiResponse::Consumed
        }
    }

    fn apply_key(&mut self, key: &KeyWithModifier) -> Option<()> {
        let ctrl = key.has_modifiers(&[KeyModifier::Ctrl]);
        let alt = key.has_modifiers(&[KeyModifier::Alt]);
        let shift = key.has_modifiers(&[KeyModifier::Shift]);
        match key.bare_key {
            BareKey::Char('Z') if ctrl => {
                self.redo();
            },
            BareKey::Char('z') if ctrl && shift => {
                self.redo();
            },
            BareKey::Char('a') if ctrl => self.move_to_start(),
            BareKey::Char('e') if ctrl => self.move_to_end(),
            BareKey::Char('z') if ctrl => {
                self.undo();
            },
            BareKey::Char('y') if ctrl => {
                self.redo();
            },
            BareKey::Left if ctrl || alt => self.move_word_left(),
            BareKey::Right if ctrl || alt => self.move_word_right(),
            BareKey::Backspace if ctrl || alt => self.delete_word_backward(),
            BareKey::Delete if ctrl || alt => self.delete_word_forward(),
            BareKey::Backspace if key.has_no_modifiers() => self.backspace(),
            BareKey::Delete if key.has_no_modifiers() => self.delete(),
            BareKey::Left if key.has_no_modifiers() => self.move_left(),
            BareKey::Right if key.has_no_modifiers() => self.move_right(),
            BareKey::Home if key.has_no_modifiers() => self.move_to_start(),
            BareKey::End if key.has_no_modifiers() => self.move_to_end(),
            _ => match typed_char(key) {
                Some(character) => {
                    if self.accept.map(|accept| accept(character)).unwrap_or(true) {
                        self.insert_char(character);
                    }
                },
                None => return None,
            },
        }
        Some(())
    }
}

impl Widget for TextInput {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if self.disabled {
            return UiResponse::NotHandled;
        }
        let is_cancel =
            is_plain(key, BareKey::Esc) || key.is_key_with_ctrl_modifier(BareKey::Char('c'));
        if is_cancel {
            if self.search_mode && !self.buffer.is_empty() {
                self.clear();
                return UiResponse::Changed(UiValue::Text(String::new()));
            }
            return UiResponse::Cancelled;
        }
        if is_plain(key, BareKey::Enter) {
            return if self.is_valid() {
                UiResponse::Submitted(UiValue::Text(self.buffer.clone()))
            } else {
                UiResponse::Consumed
            };
        }
        let before = self.buffer.clone();
        match self.apply_key(key) {
            Some(()) => self.edited(&before),
            None => UiResponse::NotHandled,
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::LeftClick(line, column) if self.hit_test(line, column) => {
                if self.disabled {
                    return UiResponse::Consumed;
                }
                let area = self.area.unwrap_or_default();
                if line as usize == area.y
                    && column >= self.text_start_x
                    && column < self.text_start_x + self.text_area_width
                {
                    let mut used = 0;
                    let mut position = self.scroll_offset;
                    for character in self.buffer.chars().skip(self.scroll_offset) {
                        let width = char_width(character);
                        if self.text_start_x + used + width > column {
                            break;
                        }
                        used += width;
                        position += 1;
                    }
                    self.break_coalescing();
                    self.set_cursor_position(position);
                }
                UiResponse::Consumed
            },
            Mouse::Hover(line, column) => {
                let hit = self.hit_test(line, column);
                update_hover(&mut self.hovered, hit)
            },
            _ => UiResponse::NotHandled,
        }
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn is_disabled(&self) -> bool {
        self.disabled
    }
    fn last_area(&self) -> Option<Rect> {
        self.area
    }
    fn clear_area(&mut self) {
        self.area = None;
    }
}

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
        input.cursor_position = 3;
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
        assert_eq!(input.get_cursor_position(), 13);

        input.cursor_position = 6;
        input.insert_char('🐱');
        assert_eq!(input.get_text(), "hello 🐱🦀 world");
    }

    #[test]
    fn test_word_jump_right() {
        let mut input = TextInput::new("hello world foo bar".to_string());
        input.cursor_position = 0;

        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 6);

        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 12);

        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 16);

        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 19);
    }

    #[test]
    fn test_word_jump_left() {
        let mut input = TextInput::new("hello world foo bar".to_string());
        input.move_to_end();
        assert_eq!(input.get_cursor_position(), 19);

        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 16);

        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 12);

        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 6);

        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_word_jump_with_multiple_spaces() {
        let mut input = TextInput::new("hello   world".to_string());
        input.cursor_position = 0;

        input.move_word_right();
        assert_eq!(input.get_cursor_position(), 8);

        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_word_jump_boundaries() {
        let mut input = TextInput::new("test".to_string());

        input.cursor_position = 0;
        input.move_word_left();
        assert_eq!(input.get_cursor_position(), 0);

        input.move_to_end();
        let end_pos = input.get_cursor_position();
        input.move_word_right();
        assert_eq!(input.get_cursor_position(), end_pos);
    }

    #[test]
    fn test_up_down_arrows() {
        let mut input = TextInput::new("hello world".to_string());

        input.cursor_position = 5;
        assert_eq!(input.get_cursor_position(), 5);

        input.move_to_start();
        assert_eq!(input.get_cursor_position(), 0);

        input.cursor_position = 5;

        input.move_to_end();
        assert_eq!(input.get_cursor_position(), 11);
    }

    #[test]
    fn test_delete_word_backward() {
        let mut input = TextInput::new("hello world foo".to_string());

        input.move_to_end();
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello world ");
        assert_eq!(input.get_cursor_position(), 12);

        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello ");
        assert_eq!(input.get_cursor_position(), 6);

        input.delete_word_backward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        input.delete_word_backward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_delete_word_backward_middle() {
        let mut input = TextInput::new("hello world foo".to_string());

        input.cursor_position = 8;
        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello rld foo");
        assert_eq!(input.get_cursor_position(), 6);
    }

    #[test]
    fn test_delete_word_forward() {
        let mut input = TextInput::new("hello world foo".to_string());

        input.cursor_position = 0;
        input.delete_word_forward();
        assert_eq!(input.get_text(), "world foo");
        assert_eq!(input.get_cursor_position(), 0);

        input.delete_word_forward();
        assert_eq!(input.get_text(), "foo");
        assert_eq!(input.get_cursor_position(), 0);

        input.delete_word_forward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        input.delete_word_forward();
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_delete_word_forward_middle() {
        let mut input = TextInput::new("hello world foo".to_string());

        input.cursor_position = 8;
        input.delete_word_forward();
        assert_eq!(input.get_text(), "hello wofoo");
        assert_eq!(input.get_cursor_position(), 8);
    }

    #[test]
    fn test_delete_word_with_multiple_spaces() {
        let mut input = TextInput::new("hello   world".to_string());

        input.cursor_position = 0;
        input.delete_word_forward();
        assert_eq!(input.get_text(), "world");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_undo_redo_basic() {
        let mut input = TextInput::empty();

        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');
        assert_eq!(input.get_text(), "hello");

        assert!(input.can_undo());
        assert!(input.undo());
        assert_eq!(input.get_text(), "");
        assert_eq!(input.get_cursor_position(), 0);

        assert!(input.can_redo());
        assert!(input.redo());
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_undo_coalescing_breaks_on_cursor_move() {
        let mut input = TextInput::empty();

        input.insert_char('h');
        input.insert_char('e');

        input.move_to_start();

        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');

        assert_eq!(input.get_text(), "llohe");

        input.undo();
        assert_eq!(input.get_text(), "he");

        input.undo();
        assert_eq!(input.get_text(), "");
    }

    #[test]
    fn test_undo_backspace() {
        let mut input = TextInput::new("hello".to_string());

        input.backspace();
        assert_eq!(input.get_text(), "hell");

        input.undo();
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_undo_delete() {
        let mut input = TextInput::new("hello".to_string());
        input.cursor_position = 0;

        input.delete();
        assert_eq!(input.get_text(), "ello");

        input.undo();
        assert_eq!(input.get_text(), "hello");
        assert_eq!(input.get_cursor_position(), 0);
    }

    #[test]
    fn test_undo_word_delete() {
        let mut input = TextInput::new("hello world".to_string());

        input.delete_word_backward();
        assert_eq!(input.get_text(), "hello ");

        input.undo();
        assert_eq!(input.get_text(), "hello world");
        assert_eq!(input.get_cursor_position(), 11);
    }

    #[test]
    fn test_undo_clear() {
        let mut input = TextInput::new("hello world".to_string());

        input.clear();
        assert_eq!(input.get_text(), "");

        input.undo();
        assert_eq!(input.get_text(), "hello world");
    }

    #[test]
    fn test_undo_set_text() {
        let mut input = TextInput::new("hello".to_string());

        input.set_text("goodbye".to_string());
        assert_eq!(input.get_text(), "goodbye");

        input.undo();
        assert_eq!(input.get_text(), "hello");
    }

    #[test]
    fn test_redo_clears_on_new_edit() {
        let mut input = TextInput::empty();

        input.insert_char('h');
        input.insert_char('e');
        input.insert_char('l');
        input.insert_char('l');
        input.insert_char('o');

        input.undo();
        assert_eq!(input.get_text(), "");
        assert!(input.can_redo());

        input.insert_char('x');
        assert!(!input.can_redo());
    }

    #[test]
    fn test_multiple_undo_redo() {
        let mut input = TextInput::empty();

        for c in "hello".chars() {
            input.insert_char(c);
        }

        input.move_left();

        for c in "world".chars() {
            input.insert_char(c);
        }

        assert_eq!(input.get_text(), "hellworldo");

        input.undo();
        assert_eq!(input.get_text(), "hello");

        input.undo();
        assert_eq!(input.get_text(), "");

        input.redo();
        assert_eq!(input.get_text(), "hello");

        input.redo();
        assert_eq!(input.get_text(), "hellworldo");
    }

    #[test]
    fn test_undo_stack_limit() {
        let mut input = TextInput::empty();

        for _i in 0..102 {
            input.backspace();
            input.insert_char('x');
        }

        let mut undo_count = 0;
        while input.undo() {
            undo_count += 1;
        }

        assert!(
            undo_count <= 100,
            "Undo count should be at most 100, got {}",
            undo_count
        );
    }

    #[test]
    fn test_undo_redo_empty_stack() {
        let mut input = TextInput::empty();

        assert!(!input.can_undo());
        assert!(!input.undo());

        assert!(!input.can_redo());
        assert!(!input.redo());
    }

    #[test]
    fn test_undo_restores_cursor_position() {
        let mut input = TextInput::new("hello world".to_string());

        input.cursor_position = 5;

        input.insert_char(' ');
        assert_eq!(input.get_text(), "hello  world");
        assert_eq!(input.get_cursor_position(), 6);

        input.undo();
        assert_eq!(input.get_text(), "hello world");
        assert_eq!(input.get_cursor_position(), 5);
    }

    #[test]
    fn test_coalescing_consecutive_inserts() {
        let mut input = TextInput::empty();

        input.insert_char('a');
        input.insert_char('b');
        input.insert_char('c');

        assert_eq!(input.get_text(), "abc");

        input.undo();
        assert_eq!(input.get_text(), "");

        assert!(!input.can_undo());
    }

    #[test]
    fn test_backspace_breaks_coalescing() {
        let mut input = TextInput::empty();

        input.insert_char('a');
        input.insert_char('b');

        input.backspace();
        assert_eq!(input.get_text(), "a");

        input.insert_char('c');
        assert_eq!(input.get_text(), "ac");

        input.undo();
        assert_eq!(input.get_text(), "a");

        input.undo();
        assert_eq!(input.get_text(), "ab");

        input.undo();
        assert_eq!(input.get_text(), "");
    }
}
