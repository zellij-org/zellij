use super::widget_common::{
    is_plain, scroll_to_include, state_flag, state_value, update_hover, widget_dcs, Rect,
    UiResponse, UiValue, Widget, SCROLL_MARGIN,
};
use std::ops::Range;
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndicatorHover {
    Above,
    Below,
}

#[derive(Debug, Clone, Default)]
pub struct ScrollView {
    total_rows: usize,
    offset: usize,
    focused: bool,
    hovered: bool,
    indicator_hover: Option<IndicatorHover>,
    area: Option<Rect>,
}

impl ScrollView {
    pub fn new(total_rows: usize) -> Self {
        ScrollView {
            total_rows,
            ..Default::default()
        }
    }
    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }
    pub fn total_rows(&self) -> usize {
        self.total_rows
    }
    pub fn set_total_rows(&mut self, total_rows: usize) {
        self.total_rows = total_rows;
        self.clamp();
    }
    pub fn offset(&self) -> usize {
        self.offset
    }
    pub fn set_offset(&mut self, offset: usize) {
        self.offset = offset;
        self.clamp();
    }
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }
    pub fn layout(&mut self, x: usize, y: usize, width: usize, height: usize) -> Range<usize> {
        self.area = Some(Rect::new(x, y, width, height));
        self.clamp();
        self.visible_range()
    }
    pub fn needs_indicators(&self) -> bool {
        self.area
            .map(|a| self.total_rows > a.height && a.height >= 2)
            .unwrap_or(false)
    }
    pub fn viewport_height(&self) -> usize {
        self.area.map(|a| a.height).unwrap_or(0)
    }
    pub fn content_y(&self) -> usize {
        self.area.map(|a| a.y).unwrap_or(0)
    }
    pub fn gutter_width(&self) -> usize {
        if self.needs_indicators() {
            7 + self.total_rows.to_string().len()
        } else {
            0
        }
    }
    fn gutter_x(&self) -> usize {
        self.area
            .map(|a| a.right().saturating_sub(self.gutter_width()))
            .unwrap_or(0)
    }
    pub fn content_width(&self) -> usize {
        self.area
            .map(|a| a.width.saturating_sub(self.gutter_width()))
            .unwrap_or(0)
    }
    pub fn visible_range(&self) -> Range<usize> {
        let end = (self.offset + self.viewport_height()).min(self.total_rows);
        self.offset.min(end)..end
    }
    pub fn rows_above(&self) -> usize {
        self.visible_range().start
    }
    pub fn rows_below(&self) -> usize {
        self.total_rows.saturating_sub(self.visible_range().end)
    }
    pub fn screen_row(&self, row: usize) -> Option<usize> {
        if self.area.is_some() && self.visible_range().contains(&row) {
            Some(self.content_y() + row - self.offset)
        } else {
            None
        }
    }
    pub fn row_at(&self, line: isize) -> Option<usize> {
        if line < 0 {
            return None;
        }
        let row = (line as usize).checked_sub(self.content_y())? + self.offset;
        if self.visible_range().contains(&row) {
            Some(row)
        } else {
            None
        }
    }
    pub fn is_row_range_visible(&self, start: usize, height: usize) -> bool {
        start >= self.offset && start + height <= self.offset + self.viewport_height()
    }
    pub fn ensure_visible(&mut self, row: usize) {
        self.ensure_range_visible(row, 1);
    }
    pub fn ensure_range_visible(&mut self, start: usize, height: usize) {
        self.offset = scroll_to_include(
            self.offset,
            start,
            height,
            self.viewport_height(),
            self.total_rows,
            SCROLL_MARGIN,
        );
        self.clamp();
    }
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let previous = self.offset;
        if delta < 0 {
            self.offset = self.offset.saturating_sub(delta.unsigned_abs());
        } else {
            self.offset = self.offset.saturating_add(delta as usize);
        }
        self.clamp();
        self.offset != previous
    }
    pub fn page_up(&mut self) -> bool {
        self.scroll_by(-(self.viewport_height().max(1) as isize))
    }
    pub fn page_down(&mut self) -> bool {
        self.scroll_by(self.viewport_height().max(1) as isize)
    }
    fn max_offset(&self) -> usize {
        self.total_rows.saturating_sub(self.viewport_height())
    }
    fn clamp(&mut self) {
        if self.area.is_some() {
            self.offset = self.offset.min(self.max_offset());
        }
    }
    fn moved(&self, moved: bool) -> UiResponse {
        if moved {
            UiResponse::Changed(UiValue::Index(self.offset))
        } else {
            UiResponse::Consumed
        }
    }
    fn indicator_at(&self, line: isize, column: usize) -> Option<IndicatorHover> {
        let area = self.area?;
        if !self.needs_indicators() || !area.contains(line, column) {
            return None;
        }
        let line = line as usize;
        if column <= self.gutter_x() {
            return None;
        }
        if line == area.y && self.rows_above() > 0 {
            Some(IndicatorHover::Above)
        } else if line + 1 == area.bottom() && self.rows_below() > 0 {
            Some(IndicatorHover::Below)
        } else {
            None
        }
    }
    pub fn serialize_indicators(&self) -> String {
        let area = match self.area {
            Some(area) if self.needs_indicators() && area.width > 0 => area,
            _ => return String::new(),
        };
        let mut state = vec![];
        state_flag(&mut state, "bar", true);
        state_flag(&mut state, "f", self.focused);
        state_value(&mut state, "tot", self.total_rows);
        state_value(&mut state, "off", self.offset);
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
        widget_dcs(
            "scroll_indicator",
            self.gutter_x(),
            area.y,
            Some(self.gutter_width()),
            Some(area.height),
            &state,
            &[],
        )
    }
    pub fn render_indicators(&self) {
        print!("{}", self.serialize_indicators());
    }
}

impl Widget for ScrollView {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if is_plain(key, BareKey::PageUp) {
            let moved = self.page_up();
            self.moved(moved)
        } else if is_plain(key, BareKey::PageDown) {
            let moved = self.page_down();
            self.moved(moved)
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::Hover(line, column) => {
                let hit = self.hit_test(line, column);
                let previous = self.indicator_hover;
                self.indicator_hover = self.indicator_at(line, column);
                let response = update_hover(&mut self.hovered, hit);
                if previous != self.indicator_hover {
                    UiResponse::Consumed
                } else {
                    response
                }
            },
            Mouse::ScrollUp(count) if self.hovered => {
                let moved = self.scroll_by(-(count.max(1) as isize));
                self.moved(moved)
            },
            Mouse::ScrollDown(count) if self.hovered => {
                let moved = self.scroll_by(count.max(1) as isize);
                self.moved(moved)
            },
            Mouse::LeftClick(line, column) | Mouse::Hold(line, column)
                if self.needs_indicators()
                    && column == self.gutter_x()
                    && self.hit_test(line, column) =>
            {
                let area = self.area.unwrap_or_default();
                let row = line as usize - area.y;
                let target = if area.height <= 1 {
                    0
                } else {
                    row * self.max_offset() / (area.height - 1)
                };
                let previous = self.offset;
                self.set_offset(target);
                self.moved(self.offset != previous)
            },
            Mouse::LeftClick(line, column) => match self.indicator_at(line, column) {
                Some(IndicatorHover::Above) => {
                    let moved = self.page_up();
                    self.indicator_hover = self.indicator_at(line, column);
                    self.moved(moved)
                },
                Some(IndicatorHover::Below) => {
                    let moved = self.page_down();
                    self.indicator_hover = self.indicator_at(line, column);
                    self.moved(moved)
                },
                None => UiResponse::NotHandled,
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
