use super::widget_common::{
    encode_text, find_next_starting_with, is_plain, list_rows, scroll_to_include, state_flag,
    state_value, text_width, typed_char, update_hover, widget_dcs, Rect, UiResponse, UiValue,
    Widget, SCROLL_MARGIN,
};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    label: String,
    shortcut: Option<String>,
    disabled: bool,
    separator: bool,
    marked: bool,
    matched: Vec<usize>,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        MenuItem {
            label: label.into(),
            shortcut: None,
            disabled: false,
            separator: false,
            marked: false,
            matched: vec![],
        }
    }
    pub fn separator() -> Self {
        MenuItem {
            label: String::new(),
            shortcut: None,
            disabled: false,
            separator: true,
            marked: false,
            matched: vec![],
        }
    }
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn marked(mut self) -> Self {
        self.marked = true;
        self
    }
    pub fn matched_indices(mut self, indices: Vec<usize>) -> Self {
        self.matched = indices;
        self
    }
    pub fn matched(&self) -> &[usize] {
        &self.matched
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn shortcut_text(&self) -> Option<&str> {
        self.shortcut.as_deref()
    }
    pub fn is_separator(&self) -> bool {
        self.separator
    }
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
    pub fn is_marked(&self) -> bool {
        self.marked
    }
    pub fn is_selectable(&self) -> bool {
        !self.separator && !self.disabled
    }
    pub fn set_marked(&mut self, marked: bool) {
        self.marked = marked;
    }
    fn serialize(&self) -> String {
        if self.separator {
            return "-".to_owned();
        }
        let mut serialized = String::new();
        if self.disabled {
            serialized.push('d');
        }
        if self.marked {
            serialized.push('m');
        }
        serialized.push_str(&encode_text(&self.label));
        if let Some(shortcut) = &self.shortcut {
            serialized.push(':');
            serialized.push_str(&encode_text(shortcut));
        }
        if !self.matched.is_empty() {
            serialized.push('/');
            serialized.push_str(
                &self
                    .matched
                    .iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        serialized
    }
    fn natural_width(&self) -> usize {
        text_width(&self.label)
            + 4
            + self
                .shortcut
                .as_ref()
                .map(|s| text_width(s) + 1)
                .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndicatorHover {
    Above,
    Below,
}

#[derive(Debug, Clone)]
pub struct MenuList {
    items: Vec<MenuItem>,
    highlighted: Option<usize>,
    offset: usize,
    border: bool,
    focused: bool,
    visible_rows: usize,
    indicator_rows: bool,
    laid_out: bool,
    hovered: bool,
    indicator_hover: Option<IndicatorHover>,
    area: Option<Rect>,
}

impl MenuList {
    pub fn new(items: Vec<MenuItem>) -> Self {
        let highlighted = items.iter().position(|i| i.is_selectable());
        MenuList {
            items,
            highlighted,
            offset: 0,
            border: false,
            focused: false,
            visible_rows: 0,
            indicator_rows: false,
            laid_out: false,
            hovered: false,
            indicator_hover: None,
            area: None,
        }
    }
    pub fn with_border(mut self) -> Self {
        self.border = true;
        self
    }
    pub fn highlighted(mut self, index: usize) -> Self {
        self.set_highlighted(Some(index));
        self
    }
    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }
    pub fn items(&self) -> &[MenuItem] {
        &self.items
    }
    pub fn set_items(&mut self, items: Vec<MenuItem>) {
        self.items = items;
        let still_valid = self
            .highlighted
            .and_then(|i| self.items.get(i))
            .map(|i| i.is_selectable())
            .unwrap_or(false);
        if !still_valid {
            self.highlighted = self.items.iter().position(|i| i.is_selectable());
        }
        self.offset = self
            .offset
            .min(self.items.len().saturating_sub(self.visible_rows));
    }
    pub fn highlighted_index(&self) -> Option<usize> {
        self.highlighted
    }
    pub fn highlighted_item(&self) -> Option<&MenuItem> {
        self.highlighted.and_then(|i| self.items.get(i))
    }
    pub fn set_highlighted(&mut self, index: Option<usize>) {
        self.highlighted = index.filter(|i| {
            self.items
                .get(*i)
                .map(|item| item.is_selectable())
                .unwrap_or(false)
        });
        self.reveal(SCROLL_MARGIN);
    }
    pub fn scroll_offset(&self) -> usize {
        self.offset
    }
    pub fn has_border(&self) -> bool {
        self.border
    }
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }
    pub fn natural_width(&self) -> usize {
        let border = if self.border { 2 } else { 0 };
        self.items
            .iter()
            .map(|i| i.natural_width())
            .max()
            .unwrap_or(0)
            + border
            + 1
    }
    fn rows_for(&self, max_height: usize) -> (usize, bool) {
        if self.border {
            (self.items.len().min(max_height.saturating_sub(2)), false)
        } else {
            list_rows(self.items.len(), max_height)
        }
    }
    pub fn height_for(&self, max_height: usize) -> usize {
        let (rows, indicators) = self.rows_for(max_height);
        rows + if self.border || indicators { 2 } else { 0 }
    }
    fn rows_above(&self) -> usize {
        self.offset
    }
    fn rows_below(&self) -> usize {
        self.items
            .len()
            .saturating_sub(self.offset + self.visible_rows)
    }
    pub fn serialize(&mut self, x: usize, y: usize, width: usize, max_height: usize) -> String {
        let (rows, indicators) = self.rows_for(max_height);
        let first_layout = !self.laid_out || self.visible_rows != rows;
        self.visible_rows = rows;
        self.indicator_rows = indicators;
        self.laid_out = true;
        if first_layout {
            self.reveal(SCROLL_MARGIN);
        }
        self.offset = self
            .offset
            .min(self.items.len().saturating_sub(self.visible_rows));
        let height = self.height_for(max_height);
        self.area = Some(Rect::new(x, y, width, height));
        let mut state = vec![];
        state_flag(&mut state, "b", self.border);
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "ind", self.indicator_rows);
        if let Some(highlighted) = self
            .highlighted
            .filter(|h| *h >= self.offset && *h < self.offset + rows)
        {
            state_value(&mut state, "hl", highlighted - self.offset);
        }
        state_value(&mut state, "above", self.rows_above());
        state_value(&mut state, "below", self.rows_below());
        state_flag(
            &mut state,
            "hu",
            self.indicator_hover == Some(IndicatorHover::Above),
        );
        state_flag(
            &mut state,
            "hd",
            self.indicator_hover == Some(IndicatorHover::Below),
        );
        let fields: Vec<String> = self
            .items
            .iter()
            .skip(self.offset)
            .take(rows)
            .map(|item| item.serialize())
            .collect();
        widget_dcs("menu", x, y, Some(width), Some(height), &state, &fields)
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, max_height: usize) {
        print!("{}", self.serialize(x, y, width, max_height));
    }
    fn first_item_line(&self) -> Option<usize> {
        let area = self.area?;
        Some(
            area.y
                + if self.border || self.indicator_rows {
                    1
                } else {
                    0
                },
        )
    }
    pub fn item_at(&self, line: isize, column: usize) -> Option<usize> {
        let area = self.area?;
        if !area.contains(line, column) {
            return None;
        }
        let border = if self.border { 1 } else { 0 };
        if column < area.x + border || column + border >= area.right() {
            return None;
        }
        let row = (line as usize).checked_sub(self.first_item_line()?)?;
        if row >= self.visible_rows {
            return None;
        }
        Some(self.offset + row)
    }
    fn indicator_at(&self, line: isize, column: usize) -> Option<IndicatorHover> {
        let area = self.area?;
        if !(self.border || self.indicator_rows) || !area.contains(line, column) {
            return None;
        }
        let line = line as usize;
        if line == area.y && self.rows_above() > 0 {
            Some(IndicatorHover::Above)
        } else if line + 1 == area.bottom() && self.rows_below() > 0 {
            Some(IndicatorHover::Below)
        } else {
            None
        }
    }
    fn reveal(&mut self, margin: usize) {
        if self.visible_rows == 0 {
            return;
        }
        if let Some(highlighted) = self.highlighted {
            self.offset = scroll_to_include(
                self.offset,
                highlighted,
                1,
                self.visible_rows,
                self.items.len(),
                margin,
            );
        }
    }
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let previous = self.offset;
        let max_offset = self.items.len().saturating_sub(self.visible_rows);
        self.offset = if delta < 0 {
            self.offset.saturating_sub(delta.unsigned_abs())
        } else {
            (self.offset + delta as usize).min(max_offset)
        };
        let window = self.offset..self.offset + self.visible_rows;
        let outside = self
            .highlighted
            .map(|h| !window.contains(&h))
            .unwrap_or(false);
        if outside {
            let candidates: Vec<usize> = window
                .clone()
                .filter(|i| {
                    self.items
                        .get(*i)
                        .map(|i| i.is_selectable())
                        .unwrap_or(false)
                })
                .collect();
            let replacement = if delta < 0 {
                candidates.last().copied()
            } else {
                candidates.first().copied()
            };
            if replacement.is_some() {
                self.highlighted = replacement;
            }
        }
        self.offset != previous
    }
    fn step(&mut self, forward: bool, count: usize) {
        let selectable: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_selectable())
            .map(|(index, _)| index)
            .collect();
        if selectable.is_empty() {
            self.highlighted = None;
            return;
        }
        let position = self
            .highlighted
            .and_then(|h| selectable.iter().position(|s| *s == h));
        let len = selectable.len();
        let next = match position {
            None => {
                if forward {
                    0
                } else {
                    len - 1
                }
            },
            Some(position) if count == 1 => {
                if forward {
                    (position + 1) % len
                } else {
                    (position + len - 1) % len
                }
            },
            Some(position) => {
                if forward {
                    (position + count).min(len - 1)
                } else {
                    position.saturating_sub(count)
                }
            },
        };
        self.highlighted = Some(selectable[next]);
        self.reveal(SCROLL_MARGIN);
    }
    fn choice(&self, index: usize) -> UiValue {
        UiValue::Choice {
            index,
            label: self.items[index].label.clone(),
        }
    }
    fn jump_to(&mut self, character: char) -> bool {
        let labels: Vec<(usize, String)> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.is_selectable())
            .map(|(index, item)| (index, item.label.clone()))
            .collect();
        match find_next_starting_with(&labels, self.highlighted, character) {
            Some(index) => {
                self.highlighted = Some(index);
                self.reveal(SCROLL_MARGIN);
                true
            },
            None => false,
        }
    }
}

impl Widget for MenuList {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        let page = self.visible_rows.max(1);
        if is_plain(key, BareKey::Up) {
            self.step(false, 1);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::Down) {
            self.step(true, 1);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::PageUp) {
            self.step(false, page);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::PageDown) {
            self.step(true, page);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::Home) {
            self.highlighted = None;
            self.step(true, 1);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::End) {
            self.highlighted = None;
            self.step(false, 1);
            UiResponse::Consumed
        } else if is_plain(key, BareKey::Enter) {
            match self.highlighted {
                Some(index) => UiResponse::Submitted(self.choice(index)),
                None => UiResponse::Consumed,
            }
        } else if is_plain(key, BareKey::Esc) {
            UiResponse::Cancelled
        } else if let Some(character) = typed_char(key).filter(|c| c.is_alphanumeric()) {
            self.jump_to(character);
            UiResponse::Consumed
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::Hover(line, column) | Mouse::Hold(line, column) => {
                let hit = self.hit_test(line, column);
                let previous_indicator = self.indicator_hover;
                self.indicator_hover = self.indicator_at(line, column);
                let mut response = update_hover(&mut self.hovered, hit);
                if let Some(index) = self.item_at(line, column) {
                    if self.items[index].is_selectable() {
                        self.highlighted = Some(index);
                    }
                }
                if previous_indicator != self.indicator_hover {
                    response = UiResponse::Consumed;
                }
                response
            },
            Mouse::LeftClick(line, column) => {
                if let Some(indicator) = self.indicator_at(line, column) {
                    let page = self.visible_rows.max(1) as isize;
                    self.scroll_by(if indicator == IndicatorHover::Above {
                        -page
                    } else {
                        page
                    });
                    self.indicator_hover = self.indicator_at(line, column);
                    return UiResponse::Consumed;
                }
                match self.item_at(line, column) {
                    Some(index) if self.items[index].is_selectable() => {
                        self.highlighted = Some(index);
                        UiResponse::Submitted(self.choice(index))
                    },
                    _ if self.hit_test(line, column) => UiResponse::Consumed,
                    _ => UiResponse::NotHandled,
                }
            },
            Mouse::ScrollUp(count) if self.hovered => {
                self.scroll_by(-(count.max(1) as isize));
                UiResponse::Consumed
            },
            Mouse::ScrollDown(count) if self.hovered => {
                self.scroll_by(count.max(1) as isize);
                UiResponse::Consumed
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
        false
    }
    fn last_area(&self) -> Option<Rect> {
        self.area
    }
    fn clear_area(&mut self) {
        self.area = None;
    }
}
