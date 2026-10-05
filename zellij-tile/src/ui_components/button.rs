use super::widget_common::{
    is_activation_key, state_flag, text_width, update_hover, widget_dcs, Rect,
    UiResponse, Widget,
};
use super::text::Text;
use zellij_utils::data::{KeyWithModifier, Mouse};

pub const BUTTON_PRESS_SECONDS: f64 = 0.4;

#[derive(Debug, Clone)]
pub struct Button {
    label: Text,
    width: Option<usize>,
    focused: bool,
    disabled: bool,
    pressed: bool,
    hovered: bool,
    accent_brackets: bool,
    left_aligned: bool,
    pending_timers: usize,
    area: Option<Rect>,
}

impl Button {
    pub fn new(label: impl Into<Text>) -> Self {
        Button {
            label: label.into(),
            width: None,
            focused: false,
            disabled: false,
            pressed: false,
            hovered: false,
            accent_brackets: false,
            left_aligned: false,
            pending_timers: 0,
            area: None,
        }
    }
    pub fn accent_brackets(mut self) -> Self {
        self.accent_brackets = true;
        self
    }
    pub fn left_aligned(mut self) -> Self {
        self.left_aligned = true;
        self
    }
    pub fn width(mut self, width: usize) -> Self {
        self.width = Some(width);
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
    pub fn pressed(mut self) -> Self {
        self.pressed = true;
        self
    }
    pub fn hovered(mut self) -> Self {
        self.hovered = true;
        self
    }
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }
    pub fn label(&self) -> &str {
        self.label.content()
    }
    pub fn set_label(&mut self, label: impl Into<Text>) {
        self.label = label.into();
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
        if disabled {
            self.pressed = false;
        }
    }
    pub fn is_pressed(&self) -> bool {
        self.pressed
    }
    pub fn natural_width(&self) -> usize {
        self.width.unwrap_or_else(|| text_width(self.label.content()) + 4)
    }
    pub fn serialize(&mut self, x: usize, y: usize) -> String {
        let width = self.natural_width();
        self.area = Some(Rect::new(x, y, width, 1));
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "d", self.disabled);
        state_flag(&mut state, "p", self.pressed && !self.disabled);
        state_flag(&mut state, "h", self.hovered && !self.disabled);
        state_flag(&mut state, "ab", self.accent_brackets);
        state_flag(&mut state, "la", self.left_aligned);
        widget_dcs(
            "button",
            x,
            y,
            Some(width),
            Some(1),
            &state,
            &self.fields(),
        )
    }
    fn fields(&self) -> Vec<String> {
        vec![self.label.serialize()]
    }
    pub fn render(&mut self, x: usize, y: usize) {
        print!("{}", self.serialize(x, y));
    }
}

impl Widget for Button {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if self.disabled {
            return UiResponse::NotHandled;
        }
        if is_activation_key(key) {
            UiResponse::Activated
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::LeftClick(line, column) if self.hit_test(line, column) => {
                if self.disabled {
                    return UiResponse::Consumed;
                }
                self.pressed = true;
                self.pending_timers += 1;
                crate::shim::set_timeout(BUTTON_PRESS_SECONDS);
                UiResponse::Activated
            },
            Mouse::Hold(line, column) if self.pressed && self.hit_test(line, column) => {
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
        if !focused {
            self.pressed = false;
        }
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
    fn handle_timer(&mut self) -> bool {
        if self.pending_timers == 0 {
            return false;
        }
        self.pending_timers -= 1;
        if self.pending_timers == 0 && self.pressed {
            self.pressed = false;
            return true;
        }
        false
    }
}
