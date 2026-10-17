use super::text::Text;
use super::text_input::TextInput;
use super::widget_common::{
    default_label_width, encode_text, is_plain, state_flag, state_value, typed_char, update_hover,
    widget_dcs, Rect, UiResponse, UiValue, Widget,
};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

const MIN_FIELD_WIDTH: usize = 7;

fn accepts_number_char(character: char) -> bool {
    character.is_ascii_digit() || character == '-'
}

fn accepts_decimal_char(character: char) -> bool {
    accepts_number_char(character) || character == '.'
}

fn scale(decimals: u32) -> i64 {
    10i64.saturating_pow(decimals)
}

pub fn format_scaled(value: i64, decimals: u32) -> String {
    if decimals == 0 {
        return value.to_string();
    }
    let scale = scale(decimals);
    let sign = if value < 0 { "-" } else { "" };
    let whole = (value / scale).abs();
    let fraction = (value % scale).abs();
    if fraction == 0 {
        return format!("{}{}", sign, whole);
    }
    let digits = format!("{:0width$}", fraction, width = decimals as usize);
    format!("{}{}.{}", sign, whole, digits.trim_end_matches('0'))
}

pub fn parse_scaled(text: &str, decimals: u32) -> Option<i64> {
    let text = text.trim();
    if decimals == 0 {
        return text.parse::<i64>().ok();
    }
    let number = text
        .parse::<f64>()
        .ok()
        .filter(|number| number.is_finite())?;
    let scaled = (number * scale(decimals) as f64).round();
    if scaled.abs() >= i64::MAX as f64 {
        return None;
    }
    Some(scaled as i64)
}

#[derive(Debug, Clone)]
pub struct NumberStepper {
    label: Text,
    value: i64,
    step: i64,
    decimals: u32,
    min: Option<i64>,
    max: Option<i64>,
    label_width: Option<usize>,
    field_width: Option<usize>,
    input: TextInput,
    editing: bool,
    focused: bool,
    disabled: bool,
    hovered: bool,
    hovered_arrow: Option<bool>,
    area: Option<Rect>,
    field_x: usize,
    field_width_drawn: usize,
}

impl NumberStepper {
    pub fn new(label: impl Into<Text>, value: i64) -> Self {
        NumberStepper {
            label: label.into(),
            value,
            step: 1,
            decimals: 0,
            min: None,
            max: None,
            label_width: None,
            field_width: None,
            input: TextInput::empty().accept(accepts_number_char),
            editing: false,
            focused: false,
            disabled: false,
            hovered: false,
            hovered_arrow: None,
            area: None,
            field_x: 0,
            field_width_drawn: 0,
        }
    }
    pub fn step(mut self, step: i64) -> Self {
        self.step = step.max(1);
        self
    }
    pub fn decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals.min(9);
        self.input = TextInput::empty().accept(self.char_filter());
        self
    }
    pub fn min(mut self, min: i64) -> Self {
        self.min = Some(min);
        self.value = self.clamp(self.value);
        self
    }
    pub fn max(mut self, max: i64) -> Self {
        self.max = Some(max);
        self.value = self.clamp(self.value);
        self
    }
    pub fn label_width(mut self, label_width: usize) -> Self {
        self.label_width = Some(label_width);
        self
    }
    pub fn field_width(mut self, field_width: usize) -> Self {
        self.field_width = Some(field_width.max(MIN_FIELD_WIDTH));
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
    pub fn editing(mut self, text: impl Into<String>) -> Self {
        self.start_editing(text.into());
        self
    }
    pub fn label(&self) -> &str {
        self.label.content()
    }
    pub fn value(&self) -> i64 {
        self.value
    }
    pub fn set_value(&mut self, value: i64) {
        self.value = self.clamp(value);
        self.editing = false;
    }
    pub fn display_value(&self) -> String {
        format_scaled(self.value, self.decimals)
    }
    fn char_filter(&self) -> fn(char) -> bool {
        if self.decimals > 0 {
            accepts_decimal_char
        } else {
            accepts_number_char
        }
    }
    pub fn set_field_width(&mut self, field_width: usize) {
        self.field_width = Some(field_width.max(MIN_FIELD_WIDTH));
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
    }
    pub fn is_editing(&self) -> bool {
        self.editing
    }
    pub fn edit_text(&self) -> &str {
        self.input.get_text()
    }
    pub fn at_min(&self) -> bool {
        self.min.map(|min| self.value <= min).unwrap_or(false)
    }
    pub fn at_max(&self) -> bool {
        self.max.map(|max| self.value >= max).unwrap_or(false)
    }
    pub fn width(&self) -> usize {
        self.effective_label_width() + self.effective_field_width()
    }
    fn clamp(&self, value: i64) -> i64 {
        let value = self.min.map(|min| value.max(min)).unwrap_or(value);
        self.max.map(|max| value.min(max)).unwrap_or(value)
    }
    fn effective_label_width(&self) -> usize {
        self.label_width
            .unwrap_or_else(|| default_label_width(self.label.content()))
    }
    fn effective_field_width(&self) -> usize {
        self.field_width.unwrap_or_else(|| {
            let digits = [Some(self.value), self.min, self.max]
                .iter()
                .flatten()
                .map(|v| format_scaled(*v, self.decimals).len())
                .max()
                .unwrap_or(1);
            (digits + 4).max(MIN_FIELD_WIDTH)
        })
    }
    fn start_editing(&mut self, text: String) {
        self.input = TextInput::new(text).accept(self.char_filter());
        self.editing = true;
    }
    fn commit(&mut self) -> bool {
        if !self.editing {
            return false;
        }
        self.editing = false;
        let previous = self.value;
        if let Some(parsed) = parse_scaled(self.input.get_text(), self.decimals) {
            self.value = self.clamp(parsed);
        }
        self.value != previous
    }
    fn step_by(&mut self, direction: i64) -> UiResponse {
        let committed = self.commit();
        let next = self.clamp(self.value.saturating_add(direction * self.step));
        if next != self.value || committed {
            self.value = next;
            UiResponse::Changed(UiValue::Number(self.value))
        } else {
            UiResponse::Consumed
        }
    }
    pub fn serialize(&mut self, x: usize, y: usize) -> String {
        let label_width = self.effective_label_width();
        let field_width = self.effective_field_width();
        self.area = Some(Rect::new(x, y, label_width + field_width, 1));
        self.field_x = x + label_width;
        self.field_width_drawn = field_width;
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "d", self.disabled);
        state_flag(&mut state, "e", self.editing);
        state_flag(&mut state, "lmin", self.at_min());
        state_flag(&mut state, "lmax", self.at_max());
        state_flag(&mut state, "h", self.hovered && !self.disabled);
        state_flag(
            &mut state,
            "hdec",
            self.hovered_arrow == Some(false) && !self.disabled,
        );
        state_flag(
            &mut state,
            "hinc",
            self.hovered_arrow == Some(true) && !self.disabled,
        );
        state_value(&mut state, "lw", label_width);
        let text = if self.editing {
            state_value(&mut state, "cur", self.input.get_cursor_position());
            self.input.get_text().to_owned()
        } else {
            self.display_value()
        };
        widget_dcs(
            "stepper",
            x,
            y,
            Some(label_width + field_width),
            Some(1),
            &state,
            &[self.label.serialize(), encode_text(&text)],
        )
    }
    pub fn render(&mut self, x: usize, y: usize) {
        print!("{}", self.serialize(x, y));
    }
    fn arrow_at(&self, column: usize) -> Option<bool> {
        if column == self.field_x {
            Some(false)
        } else if column + 1 == self.field_x + self.field_width_drawn {
            Some(true)
        } else {
            None
        }
    }
}

impl Widget for NumberStepper {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if self.disabled {
            return UiResponse::NotHandled;
        }
        if is_plain(key, BareKey::Left) {
            return self.step_by(-1);
        }
        if is_plain(key, BareKey::Right) {
            return self.step_by(1);
        }
        if is_plain(key, BareKey::Enter) {
            return if self.editing {
                if self.commit() {
                    UiResponse::Changed(UiValue::Number(self.value))
                } else {
                    UiResponse::Consumed
                }
            } else {
                UiResponse::NotHandled
            };
        }
        if is_plain(key, BareKey::Esc) {
            return if self.editing {
                self.editing = false;
                UiResponse::Cancelled
            } else {
                UiResponse::NotHandled
            };
        }
        if let Some(character) = typed_char(key) {
            if !(self.char_filter())(character) {
                return UiResponse::NotHandled;
            }
            if character == '-' && self.min.map(|min| min >= 0).unwrap_or(false) {
                return UiResponse::Consumed;
            }
            if !self.editing {
                self.start_editing(String::new());
            }
            self.input.handle_key(key);
            return UiResponse::Consumed;
        }
        if is_plain(key, BareKey::Backspace) || is_plain(key, BareKey::Delete) {
            if !self.editing {
                self.start_editing(self.display_value());
            }
            self.input.handle_key(key);
            return UiResponse::Consumed;
        }
        UiResponse::NotHandled
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        match mouse {
            Mouse::LeftClick(line, column) if self.hit_test(line, column) => {
                if self.disabled {
                    return UiResponse::Consumed;
                }
                match self.arrow_at(column) {
                    Some(false) => self.step_by(-1),
                    Some(true) => self.step_by(1),
                    None => UiResponse::Consumed,
                }
            },
            Mouse::Hover(line, column) => {
                let hit = self.hit_test(line, column);
                let previous_arrow = self.hovered_arrow;
                self.hovered_arrow = self.arrow_at(column).filter(|_| hit);
                let response = update_hover(&mut self.hovered, hit);
                if previous_arrow != self.hovered_arrow {
                    UiResponse::Consumed
                } else {
                    response
                }
            },
            _ => UiResponse::NotHandled,
        }
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.commit();
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
}
