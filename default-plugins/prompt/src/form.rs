use serde_json::{Map, Value};
use zellij_tile::prelude::*;
use zellij_utils::prompt::{FormField, FormFieldKind, FormSpec};

use crate::outcome::{Answer, Outcome};
use crate::request::Pattern;
use crate::ui::{check_text, print_error, text_width, truncate, Step};

const CHOOSE_ROWS: usize = 6;
const DIALOG_TEXT_WIDTH: usize = 56;
const DIALOG_PADDING: usize = 2;

pub struct FormScreen {
    spec: FormSpec,
    patterns: Vec<Option<Pattern>>,
    group: FocusGroup<usize>,
    errors: Vec<Option<String>>,
    tried: bool,
    scroll: ScrollView,
    follow_focus: bool,
    label_width: usize,
}

fn default_string(field: &FormField, defaults: &Option<Map<String, Value>>) -> Option<Value> {
    defaults
        .as_ref()
        .and_then(|defaults| defaults.get(&field.id).cloned())
        .or_else(|| field.default.clone())
}

fn choose_items(field: &FormField, marked: &[bool]) -> Vec<MenuItem> {
    field
        .options
        .iter()
        .zip(marked.iter())
        .map(|(option, marked)| {
            let item = MenuItem::new(option.clone());
            if *marked {
                item.marked()
            } else {
                item
            }
        })
        .collect()
}

impl FormScreen {
    pub fn new(
        spec: FormSpec,
        patterns: Vec<Option<Pattern>>,
        defaults: Option<Map<String, Value>>,
    ) -> Self {
        let label_width = spec
            .fields
            .iter()
            .filter(|field| field.kind != FormFieldKind::Choose)
            .map(|field| text_width(&field.label) + 1)
            .max()
            .unwrap_or(0)
            .min(24);
        let mut group = FocusGroup::new();
        for (index, field) in spec.fields.iter().enumerate() {
            let default = default_string(field, &defaults);
            match field.kind {
                FormFieldKind::Input => {
                    let text = default
                        .as_ref()
                        .and_then(|d| d.as_str())
                        .unwrap_or_default()
                        .to_owned();
                    let mut input = TextInput::new(text)
                        .label(field.label.clone())
                        .label_width(label_width);
                    if let Some(placeholder) = &field.placeholder {
                        input = input.placeholder(placeholder.clone());
                    }
                    group.add(index, input);
                },
                FormFieldKind::Select => {
                    let selected = default
                        .as_ref()
                        .and_then(|d| d.as_str())
                        .and_then(|d| field.options.iter().position(|o| o == d))
                        .unwrap_or(0);
                    group.add(
                        index,
                        Dropdown::new(field.label.clone(), field.options.clone())
                            .selected(selected)
                            .label_width(label_width)
                            .max_list_rows(field.options.len().clamp(1, CHOOSE_ROWS)),
                    );
                },
                FormFieldKind::Toggle => {
                    let on = default.as_ref().and_then(|d| d.as_bool()).unwrap_or(false);
                    group.add(
                        index,
                        Toggle::new(field.label.clone(), on).label_width(label_width),
                    );
                },
                FormFieldKind::Number => {
                    let mut value = default
                        .as_ref()
                        .and_then(|d| d.as_i64())
                        .or(field.min)
                        .unwrap_or(0);
                    if let Some(min) = field.min {
                        value = value.max(min);
                    }
                    if let Some(max) = field.max {
                        value = value.min(max);
                    }
                    let mut stepper = NumberStepper::new(field.label.clone(), value)
                        .step(field.step.unwrap_or(1))
                        .label_width(label_width);
                    if let Some(min) = field.min {
                        stepper = stepper.min(min);
                    }
                    if let Some(max) = field.max {
                        stepper = stepper.max(max);
                    }
                    group.add(index, stepper);
                },
                FormFieldKind::Choose => {
                    let defaults: Vec<String> = match &default {
                        Some(Value::String(s)) => vec![s.clone()],
                        Some(Value::Array(values)) => values
                            .iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_owned()))
                            .collect(),
                        _ => vec![],
                    };
                    let marked: Vec<bool> = field
                        .options
                        .iter()
                        .map(|option| defaults.contains(option))
                        .collect();
                    group.add(
                        index,
                        MenuList::new(choose_items(field, &marked)).with_border(),
                    );
                },
            }
        }
        let submit_key = spec.fields.len();
        group.add(submit_key, Button::new(spec.submit_label.clone()));
        group.add(submit_key + 1, Button::new(spec.cancel_label.clone()));
        group.focus_first();
        let field_count = spec.fields.len();
        FormScreen {
            spec,
            patterns,
            group,
            errors: vec![None; field_count],
            tried: false,
            scroll: ScrollView::new(0),
            follow_focus: true,
            label_width,
        }
    }
    fn submit_key(&self) -> usize {
        self.spec.fields.len()
    }
    pub fn is_dialog(&self) -> bool {
        self.spec.message.is_some()
    }
    pub fn has_switches(&self) -> bool {
        self.spec.fields.iter().any(|field| {
            matches!(field.kind, FormFieldKind::Toggle | FormFieldKind::Choose)
        })
    }
    fn message_lines(&self, width: usize) -> Vec<Text> {
        match &self.spec.message {
            Some(message) => Text::from(message.clone()).wrap(width.min(DIALOG_TEXT_WIDTH).max(1)),
            None => vec![],
        }
    }
    fn header_rows(&self, width: usize) -> usize {
        if self.is_dialog() {
            1 + self.message_lines(width).len() + 1
        } else {
            0
        }
    }
    fn field_body_width(&self, field: &FormField) -> usize {
        match field.kind {
            FormFieldKind::Choose | FormFieldKind::Select => {
                field
                    .options
                    .iter()
                    .map(|o| text_width(o))
                    .max()
                    .unwrap_or(0)
                    + 12
            },
            FormFieldKind::Toggle if self.is_dialog() => TOGGLE_WIDTH,
            FormFieldKind::Number if self.is_dialog() => 16,
            _ => 30,
        }
    }
    fn fields_width(&self) -> usize {
        self.label_width
            + self
                .spec
                .fields
                .iter()
                .map(|field| self.field_body_width(field))
                .max()
                .unwrap_or(0)
    }
    fn buttons_width(&self) -> usize {
        let width_of = |key: usize| {
            self.group
                .button(&key)
                .map(|b| b.natural_width())
                .unwrap_or(8)
        };
        width_of(self.submit_key()) + 2 + width_of(self.cancel_key())
    }
    fn cancel_key(&self) -> usize {
        self.spec.fields.len() + 1
    }
    fn choose_marked(&self, index: usize) -> Vec<bool> {
        self.group
            .menu_list(&index)
            .map(|list| list.items().iter().map(|item| item.is_marked()).collect())
            .unwrap_or_default()
    }
    fn toggle_choice(&mut self, index: usize, position: Option<usize>) -> Step {
        let Some(field) = self.spec.fields.get(index).cloned() else {
            return Step::Nothing;
        };
        let Some(list) = self.group.menu_list_mut(&index) else {
            return Step::Nothing;
        };
        let Some(position) = position.or_else(|| list.highlighted_index()) else {
            return Step::Nothing;
        };
        let mut marked: Vec<bool> = list.items().iter().map(|item| item.is_marked()).collect();
        let was_marked = marked.get(position).copied().unwrap_or(false);
        if !field.multi {
            marked.iter_mut().for_each(|m| *m = false);
        }
        if let Some(entry) = marked.get_mut(position) {
            *entry = !was_marked;
        }
        list.set_items(choose_items(&field, &marked));
        list.set_highlighted(Some(position));
        self.recheck();
        Step::Redraw
    }
    fn field_error(&self, index: usize) -> Option<String> {
        let field = &self.spec.fields[index];
        match field.kind {
            FormFieldKind::Input => {
                let text = self
                    .group
                    .text_input(&index)
                    .map(|input| input.get_text().to_owned())
                    .unwrap_or_default();
                check_text(&text, field.required, self.patterns[index].as_ref()).err()
            },
            FormFieldKind::Choose => {
                if field.required && !self.choose_marked(index).iter().any(|m| *m) {
                    Some("Choose at least one option".to_owned())
                } else {
                    None
                }
            },
            _ => None,
        }
    }
    fn recheck(&mut self) {
        if self.tried {
            for index in 0..self.spec.fields.len() {
                self.errors[index] = self.field_error(index);
            }
        }
    }
    #[cfg(test)]
    pub fn errors(&self) -> &[Option<String>] {
        &self.errors
    }
    fn commit_number_edits(&mut self) {
        let enter = KeyWithModifier::new(BareKey::Enter);
        for index in 0..self.spec.fields.len() {
            if let Some(stepper) = self.group.number_stepper_mut(&index) {
                if stepper.is_editing() {
                    stepper.handle_key(&enter);
                }
            }
        }
    }
    pub fn values(&self) -> Vec<(String, Value)> {
        self.spec
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                let value = match field.kind {
                    FormFieldKind::Input => Value::String(
                        self.group
                            .text_input(&index)
                            .map(|input| input.get_text().to_owned())
                            .unwrap_or_default(),
                    ),
                    FormFieldKind::Select => self
                        .group
                        .dropdown(&index)
                        .and_then(|dropdown| dropdown.selected_value())
                        .map(|value| Value::String(value.to_owned()))
                        .unwrap_or(Value::Null),
                    FormFieldKind::Toggle => Value::Bool(
                        self.group
                            .toggle(&index)
                            .map(|toggle| toggle.is_on())
                            .unwrap_or(false),
                    ),
                    FormFieldKind::Number => self
                        .group
                        .number_stepper(&index)
                        .map(|stepper| Value::from(stepper.value()))
                        .unwrap_or(Value::Null),
                    FormFieldKind::Choose => {
                        let chosen: Vec<Value> = field
                            .options
                            .iter()
                            .zip(self.choose_marked(index))
                            .filter(|(_, marked)| *marked)
                            .map(|(option, _)| Value::String(option.clone()))
                            .collect();
                        if field.multi {
                            Value::Array(chosen)
                        } else {
                            chosen.into_iter().next().unwrap_or(Value::Null)
                        }
                    },
                };
                (field.id.clone(), value)
            })
            .collect()
    }
    pub fn try_submit(&mut self) -> Step {
        self.commit_number_edits();
        self.tried = true;
        self.recheck();
        if let Some(first_bad) = self.errors.iter().position(|e| e.is_some()) {
            self.group.focus(&first_bad);
            self.follow_focus = true;
            return Step::Redraw;
        }
        Step::Done(Outcome::Answered(Answer::Form(self.values())))
    }
    fn respond(&mut self, event: FocusEvent<usize>, from_mouse: bool) -> Step {
        match event {
            FocusEvent::Element { key, response } => {
                if key == self.submit_key() && response == UiResponse::Activated {
                    return self.try_submit();
                }
                if key == self.cancel_key() && response == UiResponse::Activated {
                    return Step::Done(Outcome::Cancelled);
                }
                let is_choose = self
                    .spec
                    .fields
                    .get(key)
                    .map(|field| field.kind == FormFieldKind::Choose)
                    .unwrap_or(false);
                if is_choose {
                    if let UiResponse::Submitted(UiValue::Choice { index, .. }) = response {
                        return self.toggle_choice(key, Some(index));
                    }
                }
                if !from_mouse {
                    if let UiResponse::Submitted(_) = response {
                        self.group.focus_next();
                        self.follow_focus = true;
                    }
                }
                self.recheck();
                Step::Redraw
            },
            FocusEvent::FocusChanged(_) => {
                self.follow_focus = true;
                Step::Redraw
            },
            FocusEvent::NotHandled => Step::Nothing,
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        if self.group.has_open_overlay() {
            let event = self.group.handle_key(key);
            return self.respond(event, false);
        }
        let focused = self.group.focused_key().copied();
        let focused_is_editing_number = focused
            .and_then(|k| self.group.number_stepper(&k))
            .map(|stepper| stepper.is_editing())
            .unwrap_or(false);
        if key.is_key_without_modifier(BareKey::Esc) && !focused_is_editing_number {
            return Step::Done(Outcome::Cancelled);
        }
        let focused_is_button = focused
            .map(|k| k == self.submit_key() || k == self.cancel_key())
            .unwrap_or(false);
        if self.is_dialog()
            && key.is_key_without_modifier(BareKey::Enter)
            && !focused_is_button
            && !focused_is_editing_number
        {
            return self.try_submit();
        }
        if key.is_key_with_ctrl_modifier(BareKey::Char('a'))
            || key.is_key_with_ctrl_modifier(BareKey::Char('s'))
        {
            return self.try_submit();
        }
        if let Some(focused) = focused {
            let is_choose = self
                .spec
                .fields
                .get(focused)
                .map(|field| field.kind == FormFieldKind::Choose)
                .unwrap_or(false);
            if is_choose && key.is_key_without_modifier(BareKey::Char(' ')) {
                return self.toggle_choice(focused, None);
            }
            let is_number = self.group.number_stepper(&focused).is_some();
            if is_number
                && !focused_is_editing_number
                && key.is_key_without_modifier(BareKey::Enter)
            {
                self.group.focus_next();
                self.follow_focus = true;
                return Step::Redraw;
            }
        }
        if key.is_key_without_modifier(BareKey::Down) && !self.focused_uses_up_down() {
            self.group.focus_next();
            self.follow_focus = true;
            return Step::Redraw;
        }
        if key.is_key_without_modifier(BareKey::Up) && !self.focused_uses_up_down() {
            self.group.focus_prev();
            self.follow_focus = true;
            return Step::Redraw;
        }
        let event = self.group.handle_key(key);
        if !event.is_handled() {
            if self.scroll.handle_key(key).is_handled() {
                return Step::Redraw;
            }
        }
        self.respond(event, false)
    }
    fn focused_uses_up_down(&self) -> bool {
        self.group
            .focused_key()
            .map(|key| self.group.menu_list(key).is_some())
            .unwrap_or(false)
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        let event = self.group.handle_mouse(mouse);
        match mouse {
            Mouse::Hover(..) => {
                self.scroll.handle_mouse(mouse);
                Step::Redraw
            },
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) if !event.is_handled() => {
                Step::redraw_if(self.scroll.handle_mouse(mouse).is_handled())
            },
            _ => self.respond(event, true),
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        self.group.handle_timer()
    }
    fn block_height(&self, index: usize) -> usize {
        let field = &self.spec.fields[index];
        let body = match field.kind {
            FormFieldKind::Choose => {
                1 + self
                    .group
                    .menu_list(&index)
                    .map(|list| list.height_for(field.options.len().min(CHOOSE_ROWS) + 2))
                    .unwrap_or(3)
            },
            _ => 1,
        };
        body + if self.errors[index].is_some() { 1 } else { 0 }
    }
    fn content_height(&self, width: usize) -> usize {
        let fields = (0..self.spec.fields.len())
            .map(|index| self.block_height(index))
            .sum::<usize>()
            + 2;
        let bottom_padding = if self.is_dialog() { 1 } else { 0 };
        self.header_rows(width) + fields + bottom_padding
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.group.clear_areas();
        let total = self.content_height(width.saturating_sub(DIALOG_PADDING * 2));
        self.scroll.set_total_rows(total);
        self.scroll.layout(x, y, width, height);
        let origin = x;
        let full_width = self.scroll.content_width();
        let centered_x = |item_width: usize| origin + full_width.saturating_sub(item_width) / 2;
        let (x, content_width) = if self.is_dialog() {
            let fields_width = self.fields_width().min(full_width);
            (x + full_width.saturating_sub(fields_width) / 2, fields_width)
        } else {
            (x, full_width)
        };
        let message_lines = self.message_lines(width.saturating_sub(DIALOG_PADDING * 2));
        for (index, line) in message_lines.into_iter().enumerate() {
            if let Some(screen_y) = self.scroll.screen_row(1 + index) {
                let line_width = text_width(line.content()).min(full_width);
                print_text_with_coordinates(
                    line,
                    centered_x(line_width),
                    screen_y,
                    Some(line_width),
                    None,
                );
            }
        }
        let mut starts = vec![];
        let mut row = self.header_rows(width.saturating_sub(DIALOG_PADDING * 2));
        for index in 0..self.spec.fields.len() {
            starts.push(row);
            row += self.block_height(index);
        }
        let buttons_row = row + 1;
        if self.follow_focus {
            if let Some(focused) = self.group.focused_key().copied() {
                let (start, block) = if focused < self.spec.fields.len() {
                    (starts[focused], self.block_height(focused))
                } else {
                    (buttons_row, 1)
                };
                self.scroll.ensure_range_visible(start, block);
            }
            self.follow_focus = false;
        }
        for index in 0..self.spec.fields.len() {
            let start = starts[index];
            let block = self.block_height(index);
            if !self.scroll.is_row_range_visible(start, block) {
                continue;
            }
            let Some(screen_y) = self.scroll.screen_row(start) else {
                continue;
            };
            let field = self.spec.fields[index].clone();
            let mut body_rows = 1;
            match field.kind {
                FormFieldKind::Input => {
                    if let Some(input) = self.group.text_input_mut(&index) {
                        input.render(x, screen_y, content_width);
                    }
                },
                FormFieldKind::Select => {
                    if let Some(dropdown) = self.group.dropdown_mut(&index) {
                        dropdown.render(x, screen_y, content_width);
                    }
                },
                FormFieldKind::Toggle => {
                    if let Some(toggle) = self.group.toggle_mut(&index) {
                        toggle.render(x, screen_y);
                    }
                },
                FormFieldKind::Number => {
                    if let Some(stepper) = self.group.number_stepper_mut(&index) {
                        stepper.render(x, screen_y);
                    }
                },
                FormFieldKind::Choose => {
                    let hint = if field.multi {
                        " (Space ticks)"
                    } else {
                        " (Space picks)"
                    };
                    let label = truncate(&format!("{}{}", field.label, hint), content_width);
                    let label_length = text_width(&field.label).min(label.chars().count());
                    print_text_with_coordinates(
                        Text::new(label.clone())
                            .color_range(0, 0..label_length)
                            .dim_range(label_length..),
                        x,
                        screen_y,
                        Some(content_width),
                        None,
                    );
                    let list_height = self.block_height(index)
                        - 1
                        - if self.errors[index].is_some() { 1 } else { 0 };
                    if let Some(list) = self.group.menu_list_mut(&index) {
                        list.render(x, screen_y + 1, content_width.min(60), list_height);
                    }
                    body_rows = 1 + list_height;
                },
            }
            if let Some(error) = &self.errors[index] {
                let indent = if field.kind == FormFieldKind::Choose {
                    0
                } else {
                    self.label_width
                };
                print_error(
                    error,
                    x + indent,
                    screen_y + body_rows,
                    content_width.saturating_sub(indent),
                );
            }
        }
        if self.scroll.is_row_range_visible(buttons_row, 1) {
            if let Some(screen_y) = self.scroll.screen_row(buttons_row) {
                let submit_key = self.submit_key();
                let cancel_key = self.cancel_key();
                let submit_width = self
                    .group
                    .button(&submit_key)
                    .map(|b| b.natural_width())
                    .unwrap_or(8);
                let buttons_x = if self.is_dialog() {
                    centered_x(self.buttons_width())
                } else {
                    x
                };
                if let Some(button) = self.group.button_mut(&submit_key) {
                    button.render(buttons_x, screen_y);
                }
                if let Some(button) = self.group.button_mut(&cancel_key) {
                    button.render(buttons_x + submit_width + 2, screen_y);
                }
            }
        }
        self.scroll.render_indicators();
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.group.render_overlays(rows, cols);
    }
    pub fn desired_size(&self) -> (usize, usize) {
        if self.is_dialog() {
            let message_width = self
                .spec
                .message
                .as_ref()
                .map(|message| text_width(&message.text).min(DIALOG_TEXT_WIDTH))
                .unwrap_or(0);
            let inner = self
                .fields_width()
                .max(message_width)
                .max(self.buttons_width());
            return (
                inner + DIALOG_PADDING * 2,
                self.content_height(inner),
            );
        }
        let widest = self
            .spec
            .fields
            .iter()
            .map(|field| match field.kind {
                FormFieldKind::Choose | FormFieldKind::Select => {
                    field
                        .options
                        .iter()
                        .map(|o| text_width(o))
                        .max()
                        .unwrap_or(0)
                        + 12
                },
                _ => 30,
            })
            .max()
            .unwrap_or(30);
        (self.label_width + widest + 4, self.content_height(usize::MAX))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zellij_utils::prompt::parse_form_spec;

    fn key(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key)
    }

    fn form(text: &str) -> FormScreen {
        let spec = parse_form_spec(text).unwrap();
        let patterns = spec
            .fields
            .iter()
            .map(|field| {
                field
                    .validate
                    .as_ref()
                    .map(|p| Pattern(regex::Regex::new(p).unwrap()))
            })
            .collect();
        FormScreen::new(spec, patterns, None)
    }

    const EXAMPLE: &str = r#"{ "title": "New project",
      "fields": [
        { "id": "name",    "type": "input",  "label": "Name", "validate": "^[a-z-]+$", "required": true },
        { "id": "license", "type": "select", "label": "License", "options": ["MIT", "Apache-2.0"], "default": "MIT" },
        { "id": "ci",      "type": "toggle", "label": "Use CI", "default": true },
        { "id": "port",    "type": "number", "label": "Port", "min": 1, "max": 65535, "default": 8080 },
        { "id": "files",   "type": "choose", "label": "Files", "options": ["a", "b"], "multi": true } ],
      "buttons": { "submit": "Create", "cancel": "Cancel" } }"#;

    #[test]
    fn submit_is_refused_while_a_required_field_is_empty() {
        let mut screen = form(EXAMPLE);
        assert_eq!(screen.try_submit(), Step::Redraw);
        assert_eq!(
            screen.errors()[0].as_deref(),
            Some("This field is required")
        );
    }

    #[test]
    fn submit_is_refused_while_a_check_fails() {
        let mut screen = form(EXAMPLE);
        screen.handle_key(&key(BareKey::Char('X')));
        assert_eq!(screen.try_submit(), Step::Redraw);
        assert!(screen.errors()[0]
            .as_deref()
            .unwrap()
            .starts_with("Must match"));
        screen.handle_key(&key(BareKey::Backspace));
        screen.handle_key(&key(BareKey::Char('x')));
        assert_eq!(screen.errors()[0], None);
    }

    #[test]
    fn the_example_form_answers_one_object_in_field_order() {
        let mut screen = form(EXAMPLE);
        for c in "demo".chars() {
            screen.handle_key(&key(BareKey::Char(c)));
        }
        for _ in 0..4 {
            screen.handle_key(&key(BareKey::Tab));
        }
        assert_eq!(screen.handle_key(&key(BareKey::Char(' '))), Step::Redraw);
        match screen.try_submit() {
            Step::Done(Outcome::Answered(Answer::Form(values))) => {
                assert_eq!(
                    crate::outcome::ordered_object(&values),
                    r#"{"name":"demo","license":"MIT","ci":true,"port":8080,"files":["a"]}"#
                );
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    const DIALOG: &str = r#"{ "message": "Delete Alt n from Normal mode?",
      "fields": [ { "id": "again", "type": "toggle", "label": "Don't ask again" } ],
      "buttons": { "submit": "Delete", "cancel": "Cancel" } }"#;

    #[test]
    fn a_form_with_a_message_submits_on_enter_and_flips_toggles_on_space() {
        let mut screen = form(DIALOG);
        assert!(screen.is_dialog());
        assert_eq!(screen.handle_key(&key(BareKey::Char(' '))), Step::Redraw);
        match screen.handle_key(&key(BareKey::Enter)) {
            Step::Done(Outcome::Answered(Answer::Form(values))) => {
                assert_eq!(values, vec![("again".to_owned(), Value::Bool(true))]);
            },
            other => panic!("unexpected {:?}", other),
        }
        let mut screen = form(DIALOG);
        screen.handle_key(&key(BareKey::Tab));
        screen.handle_key(&key(BareKey::Tab));
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Cancelled)
        );
    }

    #[test]
    fn a_form_with_a_message_leaves_room_for_it_above_the_fields() {
        let dialog = form(DIALOG);
        let (width, height) = dialog.desired_size();
        assert!(width >= "Delete Alt n from Normal mode?".len() + 4);
        let plain = form(r#"{"fields":[{"id":"again","type":"toggle","label":"Don't ask again"}]}"#);
        assert!(!plain.is_dialog());
        assert_eq!(height, plain.desired_size().1 + 4);
        let mut plain = plain;
        plain.handle_key(&key(BareKey::Char(' ')));
        assert_eq!(plain.handle_key(&key(BareKey::Enter)), Step::Redraw);
    }

    #[test]
    fn required_choose_needs_a_ticked_option_and_esc_cancels() {
        let mut screen =
            form(r#"{"fields":[{"id":"f","type":"choose","options":["a","b"],"required":true}]}"#);
        assert_eq!(screen.try_submit(), Step::Redraw);
        assert!(screen.errors()[0].is_some());
        screen.handle_key(&key(BareKey::Down));
        screen.handle_key(&key(BareKey::Char(' ')));
        match screen.try_submit() {
            Step::Done(Outcome::Answered(Answer::Form(values))) => {
                assert_eq!(
                    values,
                    vec![("f".to_owned(), Value::String("b".to_owned()))]
                );
            },
            other => panic!("unexpected {:?}", other),
        }
        let mut screen = form(r#"{"fields":[{"id":"t","type":"toggle"}]}"#);
        assert_eq!(
            screen.handle_key(&key(BareKey::Esc)),
            Step::Done(Outcome::Cancelled)
        );
    }
}
