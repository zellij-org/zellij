use super::text::Text;
use super::widget_common::{
    encode_text, is_plain, is_shift_tab, state_value, text_width, widget_dcs, Rect, UiResponse,
    UiValue, Widget,
};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

const DEFAULT_WIDTH: usize = 50;
const MIN_WIDTH: usize = 16;
const BUTTON_GAP: usize = 2;

#[derive(Debug, Clone)]
pub struct ConfirmDialog {
    title: String,
    message: Text,
    buttons: Vec<String>,
    selected: usize,
    width: Option<usize>,
    open: bool,
    hovered_button: Option<usize>,
    footer_rows: usize,
    centered: bool,
    area: Option<Rect>,
    button_areas: Vec<Rect>,
}

impl ConfirmDialog {
    pub fn new(title: impl Into<String>, message: impl Into<Text>) -> Self {
        ConfirmDialog {
            title: title.into(),
            message: message.into(),
            buttons: vec!["OK".to_owned(), "Cancel".to_owned()],
            selected: 0,
            width: None,
            open: false,
            hovered_button: None,
            footer_rows: 0,
            centered: false,
            area: None,
            button_areas: vec![],
        }
    }
    pub fn buttons<S: Into<String>>(mut self, buttons: Vec<S>) -> Self {
        self.buttons = buttons.into_iter().map(|b| b.into()).collect();
        self.selected = self.selected.min(self.buttons.len().saturating_sub(1));
        self
    }
    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index.min(self.buttons.len().saturating_sub(1));
        self
    }
    pub fn width(mut self, width: usize) -> Self {
        self.width = Some(width.max(MIN_WIDTH));
        self
    }
    pub fn hovered_button(mut self, index: usize) -> Self {
        self.hovered_button = Some(index);
        self
    }
    pub fn opened(mut self) -> Self {
        self.open = true;
        self
    }
    pub fn centered(mut self) -> Self {
        self.centered = true;
        self
    }
    pub fn footer_rows(mut self, rows: usize) -> Self {
        self.footer_rows = rows;
        self
    }
    pub fn footer_area(&self) -> Option<Rect> {
        let area = self.area?;
        if self.footer_rows == 0 || area.width < 4 {
            return None;
        }
        Some(Rect::new(
            area.x + 2,
            area.y + area.height - 1 - self.footer_rows,
            area.width - 4,
            self.footer_rows,
        ))
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn set_message(&mut self, message: impl Into<Text>) {
        self.message = message.into();
    }
    pub fn button_labels(&self) -> &[String] {
        &self.buttons
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn open(&mut self) {
        self.open = true;
    }
    pub fn open_with_selected(&mut self, index: usize) {
        self.selected = index.min(self.buttons.len().saturating_sub(1));
        self.open = true;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.hovered_button = None;
        self.area = None;
        self.button_areas.clear();
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn button_areas(&self) -> &[Rect] {
        &self.button_areas
    }
    fn button_widths(&self) -> Vec<usize> {
        self.buttons.iter().map(|b| text_width(b) + 4).collect()
    }
    fn lines_for(&self, width: usize) -> Vec<Text> {
        self.message.wrap(width.saturating_sub(4))
    }
    pub fn size_for(&self, cols: usize) -> (usize, usize) {
        let buttons_width: usize = self.button_widths().iter().sum::<usize>()
            + self.buttons.len().saturating_sub(1) * BUTTON_GAP;
        let wanted = self
            .width
            .unwrap_or(DEFAULT_WIDTH)
            .max(text_width(&self.title) + 8)
            .max(buttons_width + 4);
        let width = wanted.min(cols).max(MIN_WIDTH.min(cols));
        (width, self.lines_for(width).len() + 5 + self.footer_rows)
    }
    pub fn serialize(&mut self, x: usize, y: usize, width: usize, max_height: usize) -> String {
        let width = width.max(MIN_WIDTH);
        let mut lines = self.lines_for(width);
        let max_lines = max_height.saturating_sub(5 + self.footer_rows);
        lines.truncate(max_lines);
        let height = lines.len() + 5 + self.footer_rows;
        let widths = self.button_widths();
        let buttons_width: usize =
            widths.iter().sum::<usize>() + widths.len().saturating_sub(1) * BUTTON_GAP;
        let buttons_start = (width.saturating_sub(buttons_width) / 2).max(1);
        let buttons_row = y + height - 2 - self.footer_rows;
        self.button_areas.clear();
        let mut column = x + buttons_start;
        for width in &widths {
            self.button_areas
                .push(Rect::new(column, buttons_row, *width, 1));
            column += width + BUTTON_GAP;
        }
        self.area = Some(Rect::new(x, y, width, height));
        let mut state = vec![];
        state_value(&mut state, "sel", self.selected);
        state_value(&mut state, "nb", self.buttons.len());
        state_value(&mut state, "bx", buttons_start);
        if self.footer_rows > 0 {
            state_value(&mut state, "fr", self.footer_rows);
        }
        if self.centered {
            state.push("c".to_owned());
        }
        if let Some(hovered) = self.hovered_button {
            state_value(&mut state, "hb", hovered);
        }
        let mut fields = vec![encode_text(&self.title)];
        fields.extend(self.buttons.iter().map(|b| encode_text(b)));
        fields.extend(lines.iter().map(|line| line.serialize()));
        widget_dcs("dialog", x, y, Some(width), Some(height), &state, &fields)
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, max_height: usize) {
        print!("{}", self.serialize(x, y, width, max_height));
    }
    pub fn serialize_centered(&mut self, rows: usize, cols: usize) -> String {
        if !self.open {
            return String::new();
        }
        let (width, height) = self.size_for(cols);
        let height = height.min(rows);
        let x = cols.saturating_sub(width) / 2;
        let y = rows.saturating_sub(height) / 2;
        self.serialize(x, y, width, rows)
    }
    pub fn render_centered(&mut self, rows: usize, cols: usize) {
        print!("{}", self.serialize_centered(rows, cols));
    }
    fn choose(&mut self, index: usize) -> UiResponse {
        self.selected = index;
        let label = self.buttons.get(index).cloned().unwrap_or_default();
        self.close();
        UiResponse::Submitted(UiValue::Choice { index, label })
    }
}

impl Widget for ConfirmDialog {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if !self.open {
            return UiResponse::NotHandled;
        }
        let count = self.buttons.len();
        if is_plain(key, BareKey::Esc) {
            self.close();
            return UiResponse::Cancelled;
        }
        if count == 0 {
            return UiResponse::Consumed;
        }
        if is_plain(key, BareKey::Tab) || is_plain(key, BareKey::Right) {
            self.selected = (self.selected + 1) % count;
        } else if is_shift_tab(key) || is_plain(key, BareKey::Left) {
            self.selected = (self.selected + count - 1) % count;
        } else if is_plain(key, BareKey::Enter) || is_plain(key, BareKey::Char(' ')) {
            return self.choose(self.selected);
        }
        UiResponse::Consumed
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        if !self.open {
            return UiResponse::NotHandled;
        }
        let hit = |line: isize, column: usize, areas: &[Rect]| {
            areas.iter().position(|a| a.contains(line, column))
        };
        match mouse {
            Mouse::LeftClick(line, column) => match hit(line, column, &self.button_areas) {
                Some(index) => self.choose(index),
                None => UiResponse::Consumed,
            },
            Mouse::Hover(line, column) => {
                self.hovered_button = hit(line, column, &self.button_areas);
                UiResponse::Consumed
            },
            _ => UiResponse::Consumed,
        }
    }
    fn set_focused(&mut self, _focused: bool) {}
    fn is_focused(&self) -> bool {
        self.open
    }
    fn is_disabled(&self) -> bool {
        false
    }
    fn last_area(&self) -> Option<Rect> {
        self.area
    }
    fn clear_area(&mut self) {
        self.area = None;
        self.button_areas.clear();
    }
    fn captures_input(&self) -> bool {
        self.open
    }
}
