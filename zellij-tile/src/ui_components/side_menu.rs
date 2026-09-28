use super::widget_common::{
    encode_text, is_plain, list_rows, scroll_to_include, state_flag, state_value, update_hover,
    widget_dcs, Rect, UiResponse, UiValue, Widget, SCROLL_MARGIN,
};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

#[derive(Debug, Clone)]
pub struct SideMenu {
    items: Vec<String>,
    selected: usize,
    offset: usize,
    visible_rows: usize,
    indicator_rows: bool,
    laid_out: bool,
    focused: bool,
    hovered: bool,
    hovered_row: Option<usize>,
    hovered_indicator: Option<bool>,
    area: Option<Rect>,
}

impl SideMenu {
    pub fn new<S: Into<String>>(items: Vec<S>) -> Self {
        SideMenu {
            items: items.into_iter().map(|i| i.into()).collect(),
            selected: 0,
            offset: 0,
            visible_rows: 0,
            indicator_rows: false,
            laid_out: false,
            focused: false,
            hovered: false,
            hovered_row: None,
            hovered_indicator: None,
            area: None,
        }
    }
    pub fn selected(mut self, index: usize) -> Self {
        self.set_selected(index);
        self
    }
    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }
    pub fn hovered_item(mut self, index: usize) -> Self {
        self.hovered_row = Some(index);
        self
    }
    pub fn items(&self) -> &[String] {
        &self.items
    }
    pub fn set_items<S: Into<String>>(&mut self, items: Vec<S>) {
        self.items = items.into_iter().map(|i| i.into()).collect();
        self.selected = self.selected.min(self.items.len().saturating_sub(1));
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn selected_label(&self) -> Option<&str> {
        self.items.get(self.selected).map(|i| i.as_str())
    }
    pub fn set_selected(&mut self, index: usize) {
        if index < self.items.len() {
            self.selected = index;
            self.reveal();
        }
    }
    pub fn scroll_offset(&self) -> usize {
        self.offset
    }
    fn reveal(&mut self) {
        if self.visible_rows == 0 {
            return;
        }
        self.offset = scroll_to_include(
            self.offset,
            self.selected,
            1,
            self.visible_rows,
            self.items.len(),
            SCROLL_MARGIN,
        );
    }
    fn rows_below(&self) -> usize {
        self.items
            .len()
            .saturating_sub(self.offset + self.visible_rows)
    }
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let previous = self.offset;
        let max_offset = self.items.len().saturating_sub(self.visible_rows);
        self.offset = if delta < 0 {
            self.offset.saturating_sub(delta.unsigned_abs())
        } else {
            (self.offset + delta as usize).min(max_offset)
        };
        self.offset != previous
    }
    fn choice(&self) -> UiValue {
        UiValue::Choice {
            index: self.selected,
            label: self.items.get(self.selected).cloned().unwrap_or_default(),
        }
    }
    fn select(&mut self, index: usize) -> UiResponse {
        if index == self.selected || index >= self.items.len() {
            return UiResponse::Consumed;
        }
        self.set_selected(index);
        UiResponse::Changed(self.choice())
    }
    pub fn serialize(&mut self, x: usize, y: usize, width: usize, height: usize) -> String {
        let (rows, indicators) = list_rows(self.items.len(), height);
        let first_layout = !self.laid_out || rows != self.visible_rows;
        self.visible_rows = rows;
        self.indicator_rows = indicators;
        self.laid_out = true;
        if first_layout {
            self.reveal();
        }
        self.offset = self
            .offset
            .min(self.items.len().saturating_sub(self.visible_rows));
        self.area = Some(Rect::new(x, y, width, height));
        let visible = self.offset..self.offset + self.visible_rows;
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "ind", self.indicator_rows);
        if visible.contains(&self.selected) {
            state_value(&mut state, "sel", self.selected - self.offset);
        }
        if let Some(hovered) = self.hovered_row.filter(|h| visible.contains(h)) {
            state_value(&mut state, "hov", hovered - self.offset);
        }
        if self.indicator_rows {
            state_value(&mut state, "above", self.offset);
            state_value(&mut state, "below", self.rows_below());
            state_flag(&mut state, "hu", self.hovered_indicator == Some(true));
            state_flag(&mut state, "hd", self.hovered_indicator == Some(false));
        }
        let fields: Vec<String> = self
            .items
            .iter()
            .skip(self.offset)
            .take(self.visible_rows)
            .map(|item| encode_text(item))
            .collect();
        widget_dcs(
            "side_menu",
            x,
            y,
            Some(width),
            Some(height),
            &state,
            &fields,
        )
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        print!("{}", self.serialize(x, y, width, height));
    }
    fn indicator_at(&self, line: isize, column: usize) -> Option<bool> {
        let area = self.area?;
        if !self.indicator_rows || !area.contains(line, column) {
            return None;
        }
        let line = line as usize;
        if line == area.y && self.offset > 0 {
            Some(true)
        } else if line + 1 == area.bottom() && self.rows_below() > 0 {
            Some(false)
        } else {
            None
        }
    }
    pub fn item_at(&self, line: isize, column: usize) -> Option<usize> {
        let area = self.area?;
        if !area.contains(line, column) || column + 1 >= area.right() {
            return None;
        }
        let first_line = area.y + if self.indicator_rows { 1 } else { 0 };
        let row = (line as usize).checked_sub(first_line)?;
        if row < self.visible_rows && self.offset + row < self.items.len() {
            Some(self.offset + row)
        } else {
            None
        }
    }
}

impl Widget for SideMenu {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if self.items.is_empty() {
            return UiResponse::NotHandled;
        }
        let last = self.items.len() - 1;
        if is_plain(key, BareKey::Up) {
            let next = if self.selected == 0 {
                last
            } else {
                self.selected - 1
            };
            self.select(next)
        } else if is_plain(key, BareKey::Down) {
            let next = if self.selected == last {
                0
            } else {
                self.selected + 1
            };
            self.select(next)
        } else if is_plain(key, BareKey::Home) {
            self.select(0)
        } else if is_plain(key, BareKey::End) {
            self.select(last)
        } else if is_plain(key, BareKey::Enter) {
            UiResponse::Submitted(self.choice())
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::LeftClick(line, column) => {
                if let Some(up) = self.indicator_at(line, column) {
                    let page = self.visible_rows.max(1) as isize;
                    self.scroll_by(if up { -page } else { page });
                    self.hovered_indicator = self.indicator_at(line, column);
                    return UiResponse::Consumed;
                }
                match self.item_at(line, column) {
                    Some(index) => self.select(index),
                    None if self.hit_test(line, column) => UiResponse::Consumed,
                    None => UiResponse::NotHandled,
                }
            },
            Mouse::Hover(line, column) => {
                let previous = (self.hovered_row, self.hovered_indicator);
                self.hovered_row = self.item_at(line, column);
                self.hovered_indicator = self.indicator_at(line, column);
                let hit = self.hit_test(line, column);
                let response = update_hover(&mut self.hovered, hit);
                if previous != (self.hovered_row, self.hovered_indicator) {
                    UiResponse::Consumed
                } else {
                    response
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
