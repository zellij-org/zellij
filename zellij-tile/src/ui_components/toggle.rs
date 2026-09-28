use super::widget_common::{
    default_label_width, encode_text, is_activation_key, state_flag, state_value, update_hover,
    widget_dcs, Rect, UiResponse, UiValue, Widget,
};
use zellij_utils::data::{KeyWithModifier, Mouse};

pub const TOGGLE_WIDTH: usize = 5;

#[derive(Debug, Clone)]
pub struct Toggle {
    label: String,
    on: bool,
    label_width: Option<usize>,
    focused: bool,
    disabled: bool,
    hovered: bool,
    area: Option<Rect>,
}

impl Toggle {
    pub fn new(label: impl Into<String>, on: bool) -> Self {
        Toggle {
            label: label.into(),
            on,
            label_width: None,
            focused: false,
            disabled: false,
            hovered: false,
            area: None,
        }
    }
    pub fn label_width(mut self, label_width: usize) -> Self {
        self.label_width = Some(label_width);
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
    pub fn is_on(&self) -> bool {
        self.on
    }
    pub fn set_on(&mut self, on: bool) {
        self.on = on;
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn width(&self) -> usize {
        self.effective_label_width() + TOGGLE_WIDTH
    }
    fn effective_label_width(&self) -> usize {
        self.label_width
            .unwrap_or_else(|| default_label_width(&self.label))
    }
    fn flip(&mut self) -> UiResponse {
        self.on = !self.on;
        UiResponse::Changed(UiValue::Bool(self.on))
    }
    pub fn serialize(&mut self, x: usize, y: usize) -> String {
        let width = self.width();
        self.area = Some(Rect::new(x, y, width, 1));
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "d", self.disabled);
        state_flag(&mut state, "on", self.on);
        state_flag(&mut state, "h", self.hovered && !self.disabled);
        state_value(&mut state, "lw", self.effective_label_width());
        widget_dcs(
            "toggle",
            x,
            y,
            Some(width),
            Some(1),
            &state,
            &[encode_text(&self.label)],
        )
    }
    pub fn render(&mut self, x: usize, y: usize) {
        print!("{}", self.serialize(x, y));
    }
}

impl Widget for Toggle {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if !self.disabled && is_activation_key(key) {
            self.flip()
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::LeftClick(line, column) if self.hit_test(line, column) => {
                if self.disabled {
                    UiResponse::Consumed
                } else {
                    self.flip()
                }
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
