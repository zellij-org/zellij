use std::fmt;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
use zellij_utils::data::{BareKey, KeyModifier, KeyWithModifier, Mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Rect {
            x,
            y,
            width,
            height,
        }
    }
    pub fn contains(&self, line: isize, column: usize) -> bool {
        if line < 0 {
            return false;
        }
        let line = line as usize;
        line >= self.y
            && line < self.y + self.height
            && column >= self.x
            && column < self.x + self.width
    }
    pub fn bottom(&self) -> usize {
        self.y + self.height
    }
    pub fn right(&self) -> usize {
        self.x + self.width
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiValue {
    None,
    Bool(bool),
    Index(usize),
    Choice { index: usize, label: String },
    Text(String),
    Number(i64),
}

impl fmt::Display for UiValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiValue::None => Ok(()),
            UiValue::Bool(value) => write!(f, "{}", value),
            UiValue::Index(index) => write!(f, "{}", index),
            UiValue::Choice { label, .. } => write!(f, "{:?}", label),
            UiValue::Text(text) => write!(f, "{:?}", text),
            UiValue::Number(number) => write!(f, "{}", number),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiResponse {
    Changed(UiValue),
    Submitted(UiValue),
    Activated,
    Cancelled,
    Consumed,
    NotHandled,
}

impl UiResponse {
    pub fn is_handled(&self) -> bool {
        !matches!(self, UiResponse::NotHandled)
    }
}

impl fmt::Display for UiResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UiResponse::Changed(value) => write!(f, "Changed({})", value),
            UiResponse::Submitted(value) => write!(f, "Submitted({})", value),
            UiResponse::Activated => write!(f, "Activated"),
            UiResponse::Cancelled => write!(f, "Cancelled"),
            UiResponse::Consumed => write!(f, "Consumed"),
            UiResponse::NotHandled => write!(f, "NotHandled"),
        }
    }
}

pub trait Widget {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse;
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse;
    fn set_focused(&mut self, focused: bool);
    fn is_focused(&self) -> bool;
    fn is_disabled(&self) -> bool;
    fn last_area(&self) -> Option<Rect>;
    fn clear_area(&mut self);
    fn hit_test(&self, line: isize, column: usize) -> bool {
        self.last_area()
            .map(|area| area.contains(line, column))
            .unwrap_or(false)
    }
    fn captures_input(&self) -> bool {
        false
    }
    fn handle_timer(&mut self) -> bool {
        false
    }
}

pub(crate) fn encode_text(text: &str) -> String {
    text.as_bytes()
        .iter()
        .map(|b| b.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

pub(crate) fn widget_dcs(
    name: &str,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
    state: &[String],
    fields: &[String],
) -> String {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    let mut dcs = format!(
        "\u{1b}Pz{};{}/{}/{}/{};{}",
        name,
        x,
        y,
        width,
        height,
        state.join(",")
    );
    for field in fields {
        dcs.push(';');
        dcs.push_str(field);
    }
    dcs.push_str("\u{1b}\\");
    dcs
}

pub(crate) fn state_flag(state: &mut Vec<String>, name: &str, enabled: bool) {
    if enabled {
        state.push(name.to_owned());
    }
}

pub(crate) fn state_value(state: &mut Vec<String>, name: &str, value: impl fmt::Display) {
    state.push(format!("{}={}", name, value));
}

pub(crate) fn text_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

pub(crate) fn char_width(character: char) -> usize {
    character.width().unwrap_or(0)
}

pub(crate) fn default_label_width(label: &str) -> usize {
    if label.is_empty() {
        0
    } else {
        text_width(label) + 1
    }
}

pub(crate) fn is_plain(key: &KeyWithModifier, bare_key: BareKey) -> bool {
    key.is_key_without_modifier(bare_key)
}

pub(crate) fn is_shift_tab(key: &KeyWithModifier) -> bool {
    key.bare_key == BareKey::Tab && key.has_only_modifiers(&[KeyModifier::Shift])
}

pub(crate) fn is_activation_key(key: &KeyWithModifier) -> bool {
    is_plain(key, BareKey::Enter) || is_plain(key, BareKey::Char(' '))
}

pub(crate) fn typed_char(key: &KeyWithModifier) -> Option<char> {
    match key.bare_key {
        BareKey::Char(character)
            if key.has_no_modifiers() || key.has_only_modifiers(&[KeyModifier::Shift]) =>
        {
            Some(character)
        },
        _ => None,
    }
}

pub(crate) fn find_next_starting_with(
    labels: &[(usize, String)],
    current: Option<usize>,
    character: char,
) -> Option<usize> {
    let needle = character.to_lowercase().collect::<String>();
    let start = current
        .and_then(|current| labels.iter().position(|(index, _)| *index == current))
        .map(|position| position + 1)
        .unwrap_or(0);
    let count = labels.len();
    (0..count)
        .map(|step| &labels[(start + step) % count])
        .find(|(_, label)| label.to_lowercase().starts_with(&needle))
        .map(|(index, _)| *index)
}

pub(crate) const SCROLL_MARGIN: usize = 2;

pub(crate) fn scroll_to_include(
    offset: usize,
    index: usize,
    height: usize,
    rows: usize,
    total: usize,
    margin: usize,
) -> usize {
    if rows == 0 {
        return offset;
    }
    let height = height.max(1);
    let margin = margin.min(rows.saturating_sub(height) / 2);
    let mut offset = offset;
    if index < offset + margin {
        offset = index.saturating_sub(margin);
    } else if index + height + margin > offset + rows {
        offset = (index + height + margin).saturating_sub(rows);
    }
    offset.min(total.saturating_sub(rows))
}

pub(crate) fn list_rows(total: usize, space: usize) -> (usize, bool) {
    if total <= space || space < 3 {
        (total.min(space), false)
    } else {
        (space - 2, true)
    }
}

pub(crate) fn update_hover(hovered: &mut bool, hit: bool) -> UiResponse {
    let changed = *hovered != hit;
    *hovered = hit;
    if hit || changed {
        UiResponse::Consumed
    } else {
        UiResponse::NotHandled
    }
}

pub fn fuzzy_match_indices(query: &str, candidate: &str) -> Option<Vec<usize>> {
    let mut indices = vec![];
    let mut query_chars = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .peekable();
    for (index, character) in candidate.chars().enumerate() {
        let wanted = match query_chars.peek() {
            Some(wanted) => *wanted,
            None => break,
        };
        if character.to_lowercase().any(|c| c == wanted) {
            indices.push(index);
            query_chars.next();
        }
    }
    if query_chars.peek().is_none() {
        Some(indices)
    } else {
        None
    }
}
