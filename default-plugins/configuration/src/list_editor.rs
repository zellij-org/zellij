use std::any::Any;

use zellij_tile::prelude::*;

use crate::action_picker::{ActionPicker, PickerResponse, DELETE_WIDTH};
use crate::page::{
    actions_summary, is_click, is_move_key, is_plain, is_shift_tab, markers, print_heading, truncate,
    typed, ColumnLayout, Effect, RowLook, RowScroll,
};
use crate::page::{
    action_argument_range, note_dropdown, outside_overlays, print_dim, ColumnStyle,
    BESIDE_SHORT_FIELD, DIM, SHORT_FIELD_WIDTH, SHORT_LABEL_WIDTH,
};

const FORM_LABEL_WIDTH: usize = 9;
const MIN_FIELD_WIDTH: usize = 24;
const DIALOG_MIN_WIDTH: usize = 30;
const DIALOG_WIDE_WIDTH: usize = 84;
const DIALOG_TALL_HEIGHT: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Pairs,
    Actions,
    Choice(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldSpec {
    pub label: &'static str,
    pub kind: FieldKind,
    pub placeholder: &'static str,
    pub shown_when: Option<(usize, &'static str)>,
}

impl FieldSpec {
    pub fn text(label: &'static str, placeholder: &'static str) -> Self {
        FieldSpec {
            label,
            kind: FieldKind::Text,
            placeholder,
            shown_when: None,
        }
    }
    pub fn pairs(label: &'static str) -> Self {
        FieldSpec {
            label,
            kind: FieldKind::Pairs,
            placeholder: "",
            shown_when: None,
        }
    }
    pub fn actions(label: &'static str) -> Self {
        FieldSpec {
            label,
            kind: FieldKind::Actions,
            placeholder: "",
            shown_when: None,
        }
    }
    pub fn choice(label: &'static str, choices: &'static [&'static str]) -> Self {
        FieldSpec {
            label,
            kind: FieldKind::Choice(choices),
            placeholder: "",
            shown_when: None,
        }
    }
    pub fn when(mut self, field: usize, value: &'static str) -> Self {
        self.shown_when = Some((field, value));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Pairs(Vec<(String, String)>),
    Actions(Vec<String>),
}

impl FieldValue {
    pub fn text(&self) -> String {
        match self {
            FieldValue::Text(text) => text.trim().to_owned(),
            _ => String::new(),
        }
    }
    pub fn pairs(&self) -> Vec<(String, String)> {
        match self {
            FieldValue::Pairs(pairs) => pairs
                .iter()
                .filter(|(key, value)| !key.trim().is_empty() || !value.trim().is_empty())
                .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
                .collect(),
            _ => vec![],
        }
    }
    pub fn actions(&self) -> Vec<String> {
        match self {
            FieldValue::Actions(actions) => actions.clone(),
            _ => vec![],
        }
    }
}

pub trait EntryKind {
    type Entry: Clone + PartialEq + std::fmt::Debug + 'static;
    fn title(&self) -> String;
    fn short_title(&self) -> String {
        self.title()
    }
    fn menu_like(&self) -> bool {
        false
    }
    fn heading_note(&self) -> Option<String> {
        None
    }
    fn noun(&self) -> &'static str;
    fn identity(&self, entry: &Self::Entry) -> String;
    fn summary(&self, entry: &Self::Entry) -> String;
    fn columns(&self, entry: &Self::Entry) -> Vec<String> {
        vec![self.summary(entry)]
    }
    fn fields(&self) -> Vec<FieldSpec>;
    fn to_values(&self, entry: &Self::Entry) -> Vec<FieldValue>;
    fn from_values(&self, values: &[FieldValue]) -> Result<Self::Entry, String>;
    fn block_kdl(&self, entries: &[Self::Entry]) -> Result<String, String>;
    fn block_key(&self) -> SettingKey;
    fn entries(&self, blocks: &ConfigBlocks) -> Vec<Self::Entry>;
    fn refresh(&mut self, _snapshot: &ConfigSnapshot) {}
    fn new_values(&self) -> Vec<FieldValue> {
        self.fields()
            .iter()
            .map(|field| match field.kind {
                FieldKind::Text => FieldValue::Text(String::new()),
                FieldKind::Choice(choices) => {
                    FieldValue::Text(choices.first().copied().unwrap_or("").to_owned())
                },
                FieldKind::Pairs => FieldValue::Pairs(vec![]),
                FieldKind::Actions => FieldValue::Actions(vec![]),
            })
            .collect()
    }
    fn ordered(&self) -> bool {
        false
    }
    fn restart_only(&self) -> bool {
        false
    }
    fn unique_identity(&self) -> bool {
        true
    }
    fn separator(&self) -> Option<Self::Entry> {
        None
    }
    fn can_delete(&self, _entry: &Self::Entry, _defaults: &[Self::Entry]) -> Result<(), String> {
        Ok(())
    }
    fn can_restore_defaults(&self) -> bool {
        false
    }
    fn replace_when_reverting(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormKey {
    Text(usize),
    Choice(usize),
    PairKey(usize, usize),
    PairValue(usize, usize),
    AddPair(usize),
    Actions(usize),
    Apply,
    Cancel,
    Delete,
    Revert,
}

impl FormKey {
    fn is_button(&self) -> bool {
        matches!(
            self,
            FormKey::Apply | FormKey::Cancel | FormKey::Delete | FormKey::Revert
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormResult {
    Pending,
    Apply,
    Cancel,
    Delete,
    Revert,
}

pub struct EntryForm {
    editing: Option<usize>,
    insert_after: Option<usize>,
    specs: Vec<FieldSpec>,
    values: Vec<FieldValue>,
    group: FocusGroup<FormKey>,
    picker: Option<(usize, ActionPicker)>,
    error: Option<String>,
    title: String,
    extra_buttons: Vec<FormKey>,
    scroll: RowScroll,
    dialog: bool,
    actions_focused: bool,
    base_mode: Option<InputMode>,
    for_menu: bool,
}

impl EntryForm {
    fn new(
        title: String,
        editing: Option<usize>,
        specs: Vec<FieldSpec>,
        values: Vec<FieldValue>,
        extra_buttons: Vec<FormKey>,
    ) -> Self {
        let mut form = EntryForm {
            editing,
            insert_after: None,
            specs,
            values,
            group: FocusGroup::new().wrap(true),
            picker: None,
            error: None,
            title,
            extra_buttons,
            scroll: RowScroll::default(),
            dialog: false,
            actions_focused: false,
            base_mode: None,
            for_menu: false,
        };
        form.rebuild(None);
        form
    }
    fn into_dialog(mut self, base_mode: Option<InputMode>, for_menu: bool) -> Self {
        self.dialog = true;
        self.base_mode = base_mode;
        self.for_menu = for_menu;
        self.extra_buttons.clear();
        self.rebuild(None);
        if self.editing.is_some() {
            self.group.blur();
        }
        self
    }
    fn shown_actions_field(&self) -> Option<usize> {
        self.specs.iter().enumerate().position(|(index, spec)| {
            matches!(spec.kind, FieldKind::Actions) && self.is_shown(index)
        })
    }
    fn dialog_label_width(&self) -> usize {
        self.label_width().max(FORM_LABEL_WIDTH)
    }
    fn label_width(&self) -> usize {
        self.specs
            .iter()
            .map(|spec| spec.label.chars().count())
            .max()
            .unwrap_or(0)
            .min(24)
            + 2
    }
    fn is_shown(&self, index: usize) -> bool {
        match self.specs[index].shown_when {
            Some((field, value)) => self
                .values
                .get(field)
                .map(|current| current.text() == value)
                .unwrap_or(true),
            None => true,
        }
    }
    fn rebuild(&mut self, focus: Option<FormKey>) {
        if self.dialog {
            self.rebuild_dialog(focus);
            return;
        }
        let label_width = self.label_width();
        let mut group = FocusGroup::new().wrap(true);
        for (index, spec) in self.specs.iter().enumerate() {
            if !self.is_shown(index) {
                continue;
            }
            match (&spec.kind, self.values.get(index)) {
                (FieldKind::Text, Some(FieldValue::Text(text))) => group.add(
                    FormKey::Text(index),
                    TextInput::new(text.clone())
                        .label(spec.label)
                        .label_width(label_width)
                        .placeholder(spec.placeholder),
                ),
                (FieldKind::Choice(choices), Some(FieldValue::Text(text))) => {
                    let selected = choices.iter().position(|c| c == text).unwrap_or(0);
                    group.add(
                        FormKey::Choice(index),
                        Dropdown::new(spec.label, choices.to_vec())
                            .selected(selected)
                            .label_width(label_width),
                    );
                },
                (FieldKind::Pairs, Some(FieldValue::Pairs(pairs))) => {
                    for (pair_index, (key, value)) in pairs.iter().enumerate() {
                        group.add(
                            FormKey::PairKey(index, pair_index),
                            TextInput::new(key.clone())
                                .label("  key")
                                .label_width(label_width)
                                .placeholder("name"),
                        );
                        group.add(
                            FormKey::PairValue(index, pair_index),
                            TextInput::new(value.clone())
                                .label("  value")
                                .label_width(label_width),
                        );
                    }
                    group.add(
                        FormKey::AddPair(index),
                        Button::new(format!("+ Add {}", spec.label.to_lowercase())),
                    );
                },
                (FieldKind::Actions, Some(FieldValue::Actions(actions))) => {
                    let summary = if actions.is_empty() {
                        "(none)".to_owned()
                    } else {
                        actions_summary(actions)
                    };
                    group.add(
                        FormKey::Actions(index),
                        Button::new(format!("{}: {}", spec.label, truncate(&summary, 50))),
                    );
                },
                _ => {},
            }
        }
        group.add(FormKey::Apply, Button::new("Apply"));
        group.add(FormKey::Cancel, Button::new("Cancel"));
        for button in &self.extra_buttons {
            let label = match button {
                FormKey::Delete => "Delete",
                _ => "Revert",
            };
            group.add(*button, Button::new(label));
        }
        self.group = group;
        match focus {
            Some(focus) if self.group.focus(&focus) => {},
            _ => {
                self.group.focus_first();
            },
        }
        self.scroll.follow();
    }
    fn rebuild_dialog(&mut self, focus: Option<FormKey>) {
        let mut group = FocusGroup::new();
        for (index, spec) in self.specs.iter().enumerate() {
            if !self.is_shown(index) {
                continue;
            }
            match (&spec.kind, self.values.get(index)) {
                (FieldKind::Text, Some(FieldValue::Text(text))) => group.add(
                    FormKey::Text(index),
                    TextInput::new(text.clone())
                        .placeholder(spec.placeholder)
                        .accent_brackets(),
                ),
                (FieldKind::Choice(choices), Some(FieldValue::Text(text))) => {
                    let selected = choices.iter().position(|c| c == text).unwrap_or(0);
                    group.add(
                        FormKey::Choice(index),
                        Dropdown::new("", choices.to_vec())
                            .selected(selected)
                            .label_width(0)
                            .accent_brackets(),
                    );
                },
                (FieldKind::Pairs, Some(FieldValue::Pairs(pairs))) => {
                    for (pair_index, (key, value)) in pairs.iter().enumerate() {
                        group.add(
                            FormKey::PairKey(index, pair_index),
                            TextInput::new(key.clone())
                                .placeholder("name")
                                .accent_brackets(),
                        );
                        group.add(
                            FormKey::PairValue(index, pair_index),
                            TextInput::new(value.clone()).accent_brackets(),
                        );
                    }
                    group.add(
                        FormKey::AddPair(index),
                        Button::new(format!("+ Add {}", spec.label.to_lowercase()))
                            .accent_brackets()
                            .left_aligned(),
                    );
                },
                _ => {},
            }
        }
        match self.shown_actions_field() {
            Some(index) => {
                let keep = matches!(&self.picker, Some((current, _)) if *current == index);
                if !keep {
                    let actions = self.values[index].actions();
                    let mut picker =
                        ActionPicker::new("Actions", actions, self.base_mode, self.for_menu);
                    picker.show_list();
                    self.picker = Some((index, picker));
                }
            },
            None => {
                self.picker = None;
                self.actions_focused = false;
                group.add(FormKey::Apply, Button::new("Done").accent_brackets());
                group.add(FormKey::Cancel, Button::new("Cancel").accent_brackets());
            },
        }
        self.group = group;
        match focus {
            Some(focus) if self.group.focus(&focus) => {},
            Some(_) | None => {
                if self.editing.is_none() && !self.actions_focused {
                    let first_text = self
                        .group
                        .keys()
                        .into_iter()
                        .find(|key| matches!(key, FormKey::Text(_)));
                    match first_text {
                        Some(key) => {
                            self.group.focus(&key);
                        },
                        None => {
                            self.group.focus_first();
                        },
                    }
                }
            },
        }
    }
    fn field_keys(&self) -> Vec<FormKey> {
        self.group
            .keys()
            .into_iter()
            .filter(|key| !key.is_button())
            .collect()
    }
    fn focus_actions(&mut self) {
        if let Some((_, picker)) = self.picker.as_mut() {
            self.group.blur();
            self.actions_focused = true;
            picker.select_row(0);
        }
    }
    fn dialog_picker_response(&mut self, response: PickerResponse) -> FormResult {
        match response {
            PickerResponse::Done(_) => FormResult::Apply,
            PickerResponse::Cancelled => FormResult::Cancel,
            PickerResponse::Pending => {
                if let Some((_, picker)) = self.picker.as_mut() {
                    if picker.take_returned_to_list() {
                        self.actions_focused = false;
                        self.group.blur();
                    }
                }
                FormResult::Pending
            },
        }
    }
    fn handle_dialog_key(&mut self, key: &KeyWithModifier) -> FormResult {
        if let Some((_, picker)) = self.picker.as_mut() {
            if !picker.is_listing() {
                let response = picker.handle_key(key);
                return self.dialog_picker_response(response);
            }
        }
        if self.focused_dropdown_open() {
            let event = self.group.handle_key(key);
            return self.handle_event(event, self.base_mode, self.for_menu, false);
        }
        let down = is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab);
        let up = is_plain(key, BareKey::Up) || is_shift_tab(key);
        if self.actions_focused {
            let at_top = self
                .picker
                .as_ref()
                .map(|(_, picker)| picker.selected_row() == 0)
                .unwrap_or(true);
            if up && at_top {
                self.actions_focused = false;
                if let Some(last) = self.field_keys().last().copied() {
                    self.group.focus(&last);
                }
                return FormResult::Pending;
            }
            if let Some((_, picker)) = self.picker.as_mut() {
                let response = picker.handle_key(key);
                return self.dialog_picker_response(response);
            }
            return FormResult::Pending;
        }
        let focused = self.group.focused_key().copied();
        if is_plain(key, BareKey::Esc) {
            return FormResult::Cancel;
        }
        let keys = self.group.keys();
        let position = focused.and_then(|focused| keys.iter().position(|k| *k == focused));
        if down {
            match position {
                None => {
                    self.group.focus_first();
                },
                Some(position) if position + 1 < keys.len() => {
                    self.group.focus(&keys[position + 1]);
                },
                Some(_) => self.focus_actions(),
            }
            return FormResult::Pending;
        }
        if up {
            match position {
                Some(position) if position > 0 => {
                    self.group.focus(&keys[position - 1]);
                },
                _ => {
                    if self.picker.is_some() {
                        self.group.blur();
                        self.actions_focused = true;
                        if let Some((_, picker)) = self.picker.as_mut() {
                            picker.select_row(usize::MAX);
                        }
                    } else if let Some(last) = keys.last() {
                        self.group.focus(last);
                    }
                },
            }
            return FormResult::Pending;
        }
        if focused.is_none() {
            if (typed(key, 'a') || typed(key, 'b')) && self.picker.is_some() {
                self.actions_focused = true;
                if let Some((_, picker)) = self.picker.as_mut() {
                    let response = picker.handle_key(key);
                    return self.dialog_picker_response(response);
                }
            }
            return FormResult::Pending;
        }
        let event = self.group.handle_key(key);
        self.handle_event(event, self.base_mode, self.for_menu, true)
    }
    fn handle_dialog_mouse(&mut self, mouse: Mouse) -> FormResult {
        if let Some((_, picker)) = self.picker.as_mut() {
            if !picker.is_listing() {
                let response = picker.handle_mouse(mouse);
                return self.dialog_picker_response(response);
            }
        }
        let event = self.group.handle_mouse(mouse);
        if event.is_handled() {
            if matches!(mouse, Mouse::LeftClick(..)) {
                self.actions_focused = false;
            }
            return self.handle_event(event, self.base_mode, self.for_menu, false);
        }
        if let Some((_, picker)) = self.picker.as_mut() {
            let response = picker.handle_mouse(mouse);
            if matches!(mouse, Mouse::LeftClick(..)) {
                self.group.blur();
                self.actions_focused = true;
            }
            return self.dialog_picker_response(response);
        }
        FormResult::Pending
    }
    fn render_dialog(&mut self, rows: usize, cols: usize) {
        let label_width = self.dialog_label_width();
        let fields = self.field_keys();
        let field_count = fields.len();
        let picker_rows = self
            .picker
            .as_ref()
            .map(|(_, picker)| picker.wanted_height());
        let takeover = self
            .picker
            .as_ref()
            .map(|(_, picker)| !picker.is_listing())
            .unwrap_or(false);
        if let Some((_, picker)) = self.picker.as_mut() {
            picker.set_label_width(label_width);
        }
        let content_column = self
            .picker
            .as_ref()
            .map(|(_, picker)| picker.content_column())
            .unwrap_or(label_width);
        let longest_value = self
            .values
            .iter()
            .map(|value| match value {
                FieldValue::Text(text) => text.chars().count(),
                _ => 0,
            })
            .max()
            .unwrap_or(0);
        let natural_width = self
            .picker
            .as_ref()
            .map(|(_, picker)| picker.natural_button_width())
            .unwrap_or(0)
            .max(longest_value + 5)
            .max(MIN_FIELD_WIDTH);
        let max_width = cols.saturating_sub(4).max(20);
        let max_height = rows.saturating_sub(2).max(6);
        let has_picker = self.picker.is_some();
        let extra = if has_picker && !takeover { DELETE_WIDTH } else { 0 };
        let (width, height) = match (takeover, picker_rows) {
            (true, Some(Some(wanted))) => (
                (content_column + natural_width + 4)
                    .max(DIALOG_MIN_WIDTH)
                    .min(max_width),
                (wanted + 3).min(max_height),
            ),
            (true, _) => (
                DIALOG_WIDE_WIDTH.min(max_width),
                max_height.min(DIALOG_TALL_HEIGHT),
            ),
            (false, Some(Some(wanted))) => (
                (content_column + natural_width + extra + 4)
                    .max(DIALOG_MIN_WIDTH)
                    .min(max_width),
                (field_count + 3 + wanted + 1).min(max_height),
            ),
            _ => (
                (content_column + natural_width + 4)
                    .max(DIALOG_MIN_WIDTH)
                    .min(max_width),
                (field_count + 6).min(max_height),
            ),
        };
        let dialog_x = cols.saturating_sub(width) / 2;
        let dialog_y = rows.saturating_sub(height) / 2;
        crate::page::render_frame(&self.title, dialog_x, dialog_y, width, height);
        let x = dialog_x + 2;
        let y = dialog_y + 2;
        let inner = width.saturating_sub(4);
        let bottom = dialog_y + height.saturating_sub(2);
        let field_width = natural_width.min(inner.saturating_sub(content_column)).max(5);
        self.group.clear_areas();
        if let Some((_, picker)) = self.picker.as_mut() {
            picker.set_button_width(field_width);
            picker.set_active(self.actions_focused);
        }
        if takeover {
            if let Some((_, picker)) = self.picker.as_mut() {
                picker.render(x, y, inner, (bottom + 1).saturating_sub(y));
                picker.render_overlays(rows, cols);
            }
            crate::page::note_overlay(Rect::new(dialog_x, dialog_y, width, height));
            return;
        }
        let focused_key = self.group.focused_key().copied();
        let mut row = y;
        for key in &fields {
            let label = match key {
                FormKey::Text(index) | FormKey::Choice(index) | FormKey::AddPair(index) => {
                    self.specs[*index].label.to_owned()
                },
                FormKey::PairKey(..) => "  key".to_owned(),
                FormKey::PairValue(..) => "  value".to_owned(),
                _ => String::new(),
            };
            let shows_label = true;
            let focused = Some(*key) == focused_key;
            if shows_label {
                let text = Text::new(truncate(&label, content_column.saturating_sub(1)));
                let text = if focused { text.color_all(0) } else { text };
                print_text_with_coordinates(text, x, row, None, None);
            }
            match self.group.get_mut(key) {
                Some(Element::TextInput(input)) => {
                    input.set_show_cursor(focused);
                    input.render(x + content_column, row, field_width);
                },
                Some(Element::Dropdown(dropdown)) => {
                    dropdown.render(x + content_column, row, field_width)
                },
                Some(Element::Button(button)) => button.render(x + content_column, row),
                _ => {},
            }
            row += 1;
        }
        if let Some(error) = &self.error {
            print_text_with_coordinates(
                Text::new(truncate(error, inner.saturating_sub(content_column)))
                    .error_color_all(),
                x + content_column,
                row,
                None,
                None,
            );
        }
        row += 1;
        if let Some((_, picker)) = self.picker.as_mut() {
            picker.render(x, row, inner, (bottom + 1).saturating_sub(row));
            picker.render_overlays(rows, cols);
        } else {
            let buttons: Vec<FormKey> = self
                .group
                .keys()
                .into_iter()
                .filter(|key| key.is_button())
                .collect();
            let widths: Vec<usize> = buttons
                .iter()
                .filter_map(|key| self.group.button(key).map(|b| b.natural_width()))
                .collect();
            let total = widths.iter().sum::<usize>() + 2 * widths.len().saturating_sub(1);
            let mut column = x + inner.saturating_sub(total) / 2;
            for key in buttons {
                if let Some(Element::Button(button)) = self.group.get_mut(&key) {
                    let button_width = button.natural_width();
                    button.render(column, bottom);
                    column += button_width + 2;
                }
            }
        }
        self.group.render_overlays(rows, cols);
        crate::page::note_group(&self.group);
        crate::page::note_overlay(Rect::new(dialog_x, dialog_y, width, height));
    }
    fn read_values(&mut self) {
        for (index, spec) in self.specs.iter().enumerate() {
            match spec.kind {
                FieldKind::Text => {
                    if let Some(input) = self.group.text_input(&FormKey::Text(index)) {
                        self.values[index] = FieldValue::Text(input.get_text().to_owned());
                    }
                },
                FieldKind::Choice(choices) => {
                    if let Some(dropdown) = self.group.dropdown(&FormKey::Choice(index)) {
                        let label = choices
                            .get(dropdown.selected_index())
                            .copied()
                            .unwrap_or("");
                        self.values[index] = FieldValue::Text(label.to_owned());
                    }
                },
                FieldKind::Pairs => {
                    if let FieldValue::Pairs(pairs) = &mut self.values[index] {
                        for (pair_index, pair) in pairs.iter_mut().enumerate() {
                            if let Some(input) =
                                self.group.text_input(&FormKey::PairKey(index, pair_index))
                            {
                                pair.0 = input.get_text().to_owned();
                            }
                            if let Some(input) = self
                                .group
                                .text_input(&FormKey::PairValue(index, pair_index))
                            {
                                pair.1 = input.get_text().to_owned();
                            }
                        }
                    }
                },
                FieldKind::Actions => {
                    if let Some((picker_index, picker)) = self.picker.as_ref() {
                        if self.dialog && *picker_index == index {
                            self.values[index] = FieldValue::Actions(picker.actions.clone());
                        }
                    }
                },
            }
        }
    }
    pub fn values(&mut self) -> Vec<FieldValue> {
        self.read_values();
        self.values.clone()
    }
    #[cfg(test)]
    pub fn set_text(&mut self, index: usize, text: &str) {
        if let Some(input) = self.group.text_input_mut(&FormKey::Text(index)) {
            input.set_text(text);
        }
        if let Some(value) = self.values.get_mut(index) {
            *value = FieldValue::Text(text.to_owned());
        }
    }
    #[cfg(test)]
    pub fn choose(&mut self, index: usize, label: &str) {
        if let Some(value) = self.values.get_mut(index) {
            *value = FieldValue::Text(label.to_owned());
        }
        self.rebuild(Some(FormKey::Choice(index)));
    }
    pub fn set_actions(&mut self, index: usize, actions: Vec<String>) {
        self.read_values();
        if let Some(value) = self.values.get_mut(index) {
            *value = FieldValue::Actions(actions);
        }
        self.rebuild(Some(FormKey::Actions(index)));
    }
    fn focused_dropdown_open(&self) -> bool {
        self.group
            .focused_key()
            .and_then(|key| self.group.dropdown(key))
            .map(|dropdown| dropdown.is_open())
            .unwrap_or(false)
    }
    fn handle_event(
        &mut self,
        event: FocusEvent<FormKey>,
        base_mode: Option<InputMode>,
        for_menu: bool,
        submit_on_enter: bool,
    ) -> FormResult {
        let FocusEvent::Element { key, response } = event else {
            return FormResult::Pending;
        };
        match (key, response) {
            (FormKey::Apply, UiResponse::Activated) => FormResult::Apply,
            (FormKey::Cancel, UiResponse::Activated) => FormResult::Cancel,
            (FormKey::Delete, UiResponse::Activated) => FormResult::Delete,
            (FormKey::Revert, UiResponse::Activated) => FormResult::Revert,
            (FormKey::AddPair(index), UiResponse::Activated) => {
                self.read_values();
                let pair_index = match &mut self.values[index] {
                    FieldValue::Pairs(pairs) => {
                        pairs.push((String::new(), String::new()));
                        pairs.len() - 1
                    },
                    _ => 0,
                };
                self.rebuild(Some(FormKey::PairKey(index, pair_index)));
                FormResult::Pending
            },
            (FormKey::Actions(index), UiResponse::Activated) => {
                self.read_values();
                let actions = self.values[index].actions();
                let label = self.specs[index].label;
                self.picker = Some((
                    index,
                    ActionPicker::new(label, actions, base_mode, for_menu),
                ));
                FormResult::Pending
            },
            (FormKey::Choice(index), UiResponse::Changed(_)) => {
                self.read_values();
                self.rebuild(Some(FormKey::Choice(index)));
                FormResult::Pending
            },
            (_, UiResponse::Submitted(_)) if submit_on_enter => FormResult::Apply,
            (_, UiResponse::Cancelled) => FormResult::Cancel,
            _ => FormResult::Pending,
        }
    }
    fn handle_picker(&mut self, response: PickerResponse) {
        let Some((index, _)) = self.picker.as_ref() else {
            return;
        };
        let index = *index;
        match response {
            PickerResponse::Done(actions) => {
                self.picker = None;
                self.set_actions(index, actions);
            },
            PickerResponse::Cancelled => self.picker = None,
            PickerResponse::Pending => {},
        }
    }
    fn handle_key(
        &mut self,
        key: &KeyWithModifier,
        base_mode: Option<InputMode>,
        for_menu: bool,
    ) -> FormResult {
        if self.dialog {
            return self.handle_dialog_key(key);
        }
        if let Some((_, picker)) = self.picker.as_mut() {
            let response = picker.handle_key(key);
            self.handle_picker(response);
            return FormResult::Pending;
        }
        let dropdown_open = self.focused_dropdown_open();
        if !dropdown_open {
            if is_plain(key, BareKey::Esc) {
                return FormResult::Cancel;
            }
            if is_plain(key, BareKey::Down) {
                self.group.focus_next();
                self.scroll.follow();
                return FormResult::Pending;
            }
            if is_plain(key, BareKey::Up) {
                self.group.focus_prev();
                self.scroll.follow();
                return FormResult::Pending;
            }
        }
        let focused = self.group.focused_key().copied();
        let event = self.group.handle_key(key);
        self.scroll.follow();
        if let FocusEvent::NotHandled = event {
            if is_plain(key, BareKey::Enter) && focused.is_none() {
                return FormResult::Apply;
            }
        }
        self.handle_event(event, base_mode, for_menu, true)
    }
    fn handle_mouse(
        &mut self,
        mouse: Mouse,
        base_mode: Option<InputMode>,
        for_menu: bool,
    ) -> FormResult {
        if self.dialog {
            return self.handle_dialog_mouse(mouse);
        }
        if let Some((_, picker)) = self.picker.as_mut() {
            let response = picker.handle_mouse(mouse);
            self.handle_picker(response);
            return FormResult::Pending;
        }
        if !self.group.has_open_overlay() && self.scroll.handle_wheel(&mouse).is_some() {
            return FormResult::Pending;
        }
        let event = self.group.handle_mouse(mouse);
        self.handle_event(event, base_mode, for_menu, false)
    }
    fn handle_timer(&mut self) -> bool {
        let picker_changed = self
            .picker
            .as_mut()
            .map(|(_, picker)| picker.handle_timer())
            .unwrap_or(false);
        self.group.handle_timer() || picker_changed
    }
    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.dialog {
            if let Some((_, picker)) = &self.picker {
                if self.actions_focused || !picker.is_listing() {
                    return picker.hints();
                }
            }
            return vec![
                ("<↓↑>", "field"),
                ("<Enter>", "done"),
                ("<Esc>", "cancel"),
            ];
        }
        match &self.picker {
            Some((_, picker)) => picker.hints(),
            None => vec![
                ("<Tab/↓↑>", "field"),
                ("<Enter>", "apply"),
                ("<Esc>", "cancel"),
            ],
        }
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if let Some((_, picker)) = self.picker.as_mut() {
            self.group.clear_areas();
            picker.render(x, y, width, height);
            return;
        }
        print_heading(&self.title, x, y, width);
        self.group.clear_areas();
        let keys = self.group.keys();
        let fields: Vec<FormKey> = keys.iter().copied().filter(|k| !k.is_button()).collect();
        let buttons: Vec<FormKey> = keys.iter().copied().filter(|k| k.is_button()).collect();
        let focused_key = self.group.focused_key().copied();
        let focused_field =
            focused_key.and_then(|focused| fields.iter().position(|k| *k == focused));
        let field_height = height.saturating_sub(5).max(1);
        let visible =
            self.scroll
                .layout(x, y + 2, width, field_height, fields.len(), focused_field);
        let mut row = y + 2;
        for index in visible {
            let key = fields[index];
            let focused = Some(key) == focused_key;
            match self.group.get_mut(&key) {
                Some(Element::TextInput(input)) => {
                    input.set_show_cursor(focused);
                    input.render(x, row, self.scroll.row_width());
                },
                Some(Element::Dropdown(dropdown)) => {
                    dropdown.render(x, row, self.scroll.row_width().min(50))
                },
                Some(Element::Button(button)) => button.render(x, row),
                _ => {},
            }
            row += 1;
        }
        let button_y = row + 1;
        let mut column = x;
        for key in buttons {
            if let Some(Element::Button(button)) = self.group.get_mut(&key) {
                let button_width = button.natural_width();
                if column + button_width <= x + width {
                    button.render(column, button_y);
                }
                column += button_width + 1;
            }
        }
        if let Some(error) = &self.error {
            print_text_with_coordinates(
                Text::new(truncate(error, width)).error_color_all(),
                x,
                button_y + 1,
                None,
                None,
            );
        }
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        if self.dialog {
            self.render_dialog(rows, cols);
            return;
        }
        match self.picker.as_mut() {
            Some((_, picker)) => picker.render_overlays(rows, cols),
            None => {
                self.group.render_overlays(rows, cols);
                crate::page::note_group(&self.group);
            },
        }
    }
}

#[derive(Debug, Clone)]
enum Follow {
    Identity(String),
    Index(usize),
}

pub struct ListEditor<K: EntryKind> {
    pub kind: K,
    current: Vec<K::Entry>,
    saved: Vec<K::Entry>,
    defaults: Vec<K::Entry>,
    form: Option<EntryForm>,
    dialog: ConfirmDialog,
    pending_delete: Option<usize>,
    effects: Vec<Effect>,
    notice: Option<String>,
    base_mode: Option<InputMode>,
    for_menu: bool,
    dialog_form: bool,
    follow: Option<Follow>,
}

impl<K: EntryKind> ListEditor<K> {
    pub fn new(kind: K) -> Self {
        ListEditor {
            kind,
            current: vec![],
            saved: vec![],
            defaults: vec![],
            form: None,
            dialog: ConfirmDialog::new("", ""),
            pending_delete: None,
            effects: vec![],
            notice: None,
            base_mode: None,
            for_menu: false,
            dialog_form: false,
            follow: None,
        }
    }
    pub fn for_menu(mut self) -> Self {
        self.for_menu = true;
        self
    }
    pub fn dialog_form(mut self) -> Self {
        self.dialog_form = true;
        self
    }
    fn form_title(&self, verb: &str) -> String {
        if self.dialog_form {
            let noun: String = self
                .kind
                .noun()
                .split(' ')
                .map(|word| {
                    let mut characters = word.chars();
                    match characters.next() {
                        Some(first) => first.to_uppercase().chain(characters).collect(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<String>>()
                .join(" ");
            format!("{} {}", verb, noun)
        } else {
            format!("{} {}", verb, self.kind.noun())
        }
    }
    fn finish_form(&self, form: EntryForm) -> EntryForm {
        if self.dialog_form {
            form.into_dialog(self.base_mode, self.for_menu)
        } else {
            form
        }
    }
    pub fn set_entries(
        &mut self,
        current: Vec<K::Entry>,
        saved: Vec<K::Entry>,
        defaults: Vec<K::Entry>,
    ) {
        self.current = current;
        self.saved = saved;
        self.defaults = defaults;
    }
    #[cfg(test)]
    pub fn form_mut(&mut self) -> Option<&mut EntryForm> {
        self.form.as_mut()
    }
    fn find<'a>(&self, list: &'a [K::Entry], entry: &K::Entry) -> Option<&'a K::Entry> {
        let identity = self.kind.identity(entry);
        list.iter()
            .find(|other| self.kind.identity(other) == identity)
    }
    pub fn is_unsaved(&self, index: usize) -> bool {
        let Some(entry) = self.current.get(index) else {
            return false;
        };
        if self.kind.ordered() {
            return self.saved.get(index) != Some(entry);
        }
        self.find(&self.saved, entry) != Some(entry)
    }
    pub fn is_default(&self, index: usize) -> bool {
        let Some(entry) = self.current.get(index) else {
            return false;
        };
        if self.kind.ordered() {
            return self.defaults.get(index) == Some(entry) && self.saved == self.defaults;
        }
        self.find(&self.defaults, entry) == Some(entry)
    }
    fn matches_saved(&self, list: &[K::Entry]) -> bool {
        if self.kind.ordered() {
            return list == self.saved.as_slice();
        }
        list.len() == self.saved.len()
            && list
                .iter()
                .all(|entry| self.find(&self.saved, entry) == Some(entry))
    }
    fn apply_list(&mut self, list: Vec<K::Entry>) -> bool {
        self.follow = None;
        if self.matches_saved(&list) && !self.kind.replace_when_reverting() {
            self.effects.push(Effect::Revert(self.kind.block_key()));
        } else {
            match self.kind.block_kdl(&list) {
                Ok(kdl) => self.effects.push(Effect::ReplaceBlocks(kdl)),
                Err(error) => {
                    self.notice = Some(error);
                    return false;
                },
            }
        }
        self.current = list;
        true
    }
    fn follow_entry(&mut self, index: usize) {
        self.follow = match self.current.get(index) {
            Some(entry) if !self.kind.ordered() => {
                Some(Follow::Identity(self.kind.identity(entry)))
            },
            _ => Some(Follow::Index(index)),
        };
    }
    fn insert_position(&self, after: Option<usize>) -> usize {
        if !self.kind.ordered() {
            return self.current.len();
        }
        match after {
            Some(index) => (index + 1).min(self.current.len()),
            None => self.current.len(),
        }
    }
    pub fn open_add(&mut self, after: Option<usize>) {
        let verb = if self.dialog_form { "Add" } else { "New" };
        let mut form = EntryForm::new(
            self.form_title(verb),
            None,
            self.kind.fields(),
            self.kind.new_values(),
            vec![],
        );
        form.insert_after = after;
        self.form = Some(self.finish_form(form));
    }
    pub fn open_edit(&mut self, index: usize) {
        let Some(entry) = self.current.get(index).cloned() else {
            return;
        };
        let mut buttons = vec![FormKey::Delete];
        if self.is_unsaved(index) {
            buttons.push(FormKey::Revert);
        }
        let form = EntryForm::new(
            self.form_title("Edit"),
            Some(index),
            self.kind.fields(),
            self.kind.to_values(&entry),
            buttons,
        );
        self.form = Some(self.finish_form(form));
    }
    pub fn apply_form(&mut self) -> bool {
        let Some(form) = self.form.as_mut() else {
            return false;
        };
        let editing = form.editing;
        let insert_after = form.insert_after;
        let values = form.values();
        let entry = match self.kind.from_values(&values) {
            Ok(entry) => entry,
            Err(error) => {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(error);
                }
                return false;
            },
        };
        if self.kind.unique_identity() {
            let identity = self.kind.identity(&entry);
            let taken = self.current.iter().enumerate().any(|(index, other)| {
                Some(index) != editing && self.kind.identity(other) == identity
            });
            if taken {
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(format!("{} already exists", identity));
                }
                return false;
            }
        }
        let mut list = self.current.clone();
        let position = match editing {
            Some(index) if index < list.len() => {
                list[index] = entry;
                index
            },
            _ => {
                let position = self.insert_position(insert_after);
                list.insert(position, entry);
                position
            },
        };
        self.form = None;
        if self.apply_list(list) {
            self.follow_entry(position);
        }
        true
    }
    pub fn add_separator(&mut self, after: Option<usize>) -> bool {
        let Some(separator) = self.kind.separator() else {
            return false;
        };
        let mut list = self.current.clone();
        let position = self.insert_position(after);
        list.insert(position, separator);
        if self.apply_list(list) {
            self.follow_entry(position);
        }
        true
    }
    pub fn request_delete(&mut self, index: usize) {
        let Some(entry) = self.current.get(index).cloned() else {
            return;
        };
        if let Err(reason) = self.kind.can_delete(&entry, &self.defaults) {
            self.notice = Some(reason);
            return;
        }
        if !crate::page::confirm_removals() {
            self.delete_now(index);
            return;
        }
        self.pending_delete = Some(index);
        self.dialog = ConfirmDialog::new(
            format!("Delete {}?", self.kind.noun()),
            format!("Delete {}?", truncate(&self.kind.summary(&entry), 50)),
        )
        .buttons(crate::page::DELETE_BUTTONS.to_vec())
        .width(64)
        .opened();
    }
    pub fn delete_now(&mut self, index: usize) {
        let mut list = self.current.clone();
        if index < list.len() {
            list.remove(index);
            if self.apply_list(list) {
                self.follow_entry(index.min(self.current.len().saturating_sub(1)));
            }
        }
    }
    pub fn revert(&mut self, index: usize) {
        let Some(entry) = self.current.get(index).cloned() else {
            return;
        };
        if !self.is_unsaved(index) {
            self.notice = Some("Nothing to revert".to_owned());
            return;
        }
        let mut list = self.current.clone();
        if self.kind.ordered() {
            list = self.saved.clone();
            self.notice = Some(format!("Reverted {}", self.kind.title()));
        } else {
            match self.find(&self.saved, &entry).cloned() {
                Some(saved_entry) => list[index] = saved_entry,
                None => {
                    list.remove(index);
                },
            }
            self.notice = Some(format!("Reverted the {}", self.kind.noun()));
        }
        if self.apply_list(list) {
            self.follow_entry(index.min(self.current.len().saturating_sub(1)));
        }
    }
    pub fn restore_defaults(&mut self) -> bool {
        if !self.kind.can_restore_defaults() {
            return false;
        }
        let defaults = self.defaults.clone();
        self.apply_list(defaults);
        self.notice = Some(format!("{} restored to the defaults", self.kind.title()));
        true
    }
    pub fn move_entry(&mut self, index: usize, down: bool) -> bool {
        if !self.kind.ordered() || index >= self.current.len() {
            return false;
        }
        let target = if down {
            index + 1
        } else {
            match index.checked_sub(1) {
                Some(target) => target,
                None => return false,
            }
        };
        if target >= self.current.len() {
            return false;
        }
        let mut list = self.current.clone();
        list.swap(index, target);
        if self.apply_list(list) {
            self.follow_entry(target);
        }
        true
    }
    fn action_labels(&self) -> Vec<String> {
        let mut labels = vec![format!("+ Add {}", self.kind.noun())];
        if self.kind.separator().is_some() {
            labels.push("+ Add separator".to_owned());
        }
        if self.kind.can_restore_defaults() {
            labels.push("Restore defaults".to_owned());
        }
        labels
    }
    fn form_result(&mut self, result: FormResult) {
        let editing = self.form.as_ref().and_then(|form| form.editing);
        match result {
            FormResult::Apply => {
                self.apply_form();
            },
            FormResult::Cancel => self.form = None,
            FormResult::Delete => {
                self.form = None;
                if let Some(index) = editing {
                    self.request_delete(index);
                }
            },
            FormResult::Revert => {
                self.form = None;
                if let Some(index) = editing {
                    self.revert(index);
                }
            },
            FormResult::Pending => {},
        }
    }
    fn dialog_response(&mut self, response: UiResponse) {
        if crate::page::removal_confirmed(&response) {
            if let Some(index) = self.pending_delete.take() {
                self.delete_now(index);
            }
        }
        if !self.dialog.is_open() {
            self.pending_delete = None;
        }
    }
}

pub trait Section {
    fn heading(&self) -> String;
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot);
    fn len(&self) -> usize;
    fn columns(&self, index: usize) -> Vec<String>;
    fn marker(&self, index: usize) -> String;
    fn actions(&self) -> Vec<String>;
    fn run_action(&mut self, action: usize, after: Option<usize>);
    fn open_add(&mut self, after: Option<usize>);
    fn add_separator(&mut self, after: Option<usize>) -> bool;
    fn open_edit(&mut self, index: usize);
    fn request_delete(&mut self, index: usize);
    fn revert(&mut self, index: usize);
    fn restore_defaults(&mut self) -> bool;
    fn has_separators(&self) -> bool;
    fn can_restore_defaults(&self) -> bool;
    fn ordered(&self) -> bool;
    fn move_entry(&mut self, index: usize, down: bool) -> bool;
    fn take_entry(&mut self, index: usize) -> Option<Box<dyn Any>>;
    fn insert_entry(&mut self, index: usize, entry: Box<dyn Any>) -> Result<(), Box<dyn Any>>;
    fn followed(&self) -> Option<usize>;
    fn clear_follow(&mut self);
    fn is_busy(&self) -> bool;
    fn handle_busy_key(&mut self, key: &KeyWithModifier);
    fn handle_busy_mouse(&mut self, mouse: Mouse);
    fn handle_timer(&mut self) -> bool;
    fn render_busy(&mut self, x: usize, y: usize, width: usize, height: usize);
    fn busy_over_list(&self) -> bool {
        false
    }
    fn short_heading(&self) -> String {
        self.heading()
    }
    fn is_separator(&self, _index: usize) -> bool {
        false
    }
    fn menu_like(&self) -> bool {
        false
    }
    fn heading_note(&self) -> Option<String> {
        None
    }
    fn render_overlays(&mut self, rows: usize, cols: usize);
    fn busy_hints(&self) -> Vec<(&'static str, &'static str)>;
    fn take_effects(&mut self) -> Vec<Effect>;
    fn take_notice(&mut self) -> Option<String>;
    fn close(&mut self);
}

impl<K: EntryKind> Section for ListEditor<K> {
    fn short_heading(&self) -> String {
        self.kind.short_title()
    }
    fn menu_like(&self) -> bool {
        self.kind.menu_like()
    }
    fn heading_note(&self) -> Option<String> {
        self.kind.heading_note().or_else(|| {
            if self.kind.restart_only() {
                Some("⟳ applies after a restart".to_owned())
            } else {
                None
            }
        })
    }
    fn is_separator(&self, index: usize) -> bool {
        match (self.current.get(index), self.kind.separator()) {
            (Some(entry), Some(separator)) => {
                self.kind.identity(entry) == self.kind.identity(&separator)
                    && self.kind.columns(entry) == self.kind.columns(&separator)
            },
            _ => false,
        }
    }
    fn heading(&self) -> String {
        if self.kind.restart_only() {
            format!("{} · ⟳ takes effect after a restart", self.kind.title())
        } else {
            self.kind.title()
        }
    }
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        self.kind.refresh(snapshot);
        let current = self.kind.entries(&snapshot.blocks);
        let saved = self.kind.entries(&snapshot.saved_blocks);
        let defaults = self.kind.entries(&snapshot.default_blocks);
        self.set_entries(current, saved, defaults);
    }
    fn len(&self) -> usize {
        self.current.len()
    }
    fn columns(&self, index: usize) -> Vec<String> {
        self.current
            .get(index)
            .map(|entry| self.kind.columns(entry))
            .unwrap_or_default()
    }
    fn marker(&self, index: usize) -> String {
        let unsaved = self.is_unsaved(index);
        markers(unsaved, !unsaved && self.is_default(index), false)
    }
    fn actions(&self) -> Vec<String> {
        self.action_labels()
    }
    fn run_action(&mut self, action: usize, after: Option<usize>) {
        let labels = self.action_labels();
        match labels.get(action).map(|label| label.as_str()) {
            Some("+ Add separator") => {
                self.add_separator(after);
            },
            Some("Restore defaults") => {
                self.restore_defaults();
            },
            Some(_) => self.open_add(after),
            None => {},
        }
    }
    fn open_add(&mut self, after: Option<usize>) {
        ListEditor::open_add(self, after)
    }
    fn add_separator(&mut self, after: Option<usize>) -> bool {
        ListEditor::add_separator(self, after)
    }
    fn open_edit(&mut self, index: usize) {
        ListEditor::open_edit(self, index)
    }
    fn request_delete(&mut self, index: usize) {
        ListEditor::request_delete(self, index)
    }
    fn revert(&mut self, index: usize) {
        ListEditor::revert(self, index)
    }
    fn restore_defaults(&mut self) -> bool {
        ListEditor::restore_defaults(self)
    }
    fn has_separators(&self) -> bool {
        self.kind.separator().is_some()
    }
    fn can_restore_defaults(&self) -> bool {
        self.kind.can_restore_defaults()
    }
    fn ordered(&self) -> bool {
        self.kind.ordered()
    }
    fn move_entry(&mut self, index: usize, down: bool) -> bool {
        ListEditor::move_entry(self, index, down)
    }
    fn take_entry(&mut self, index: usize) -> Option<Box<dyn Any>> {
        if index >= self.current.len() {
            return None;
        }
        let mut list = self.current.clone();
        let entry = list.remove(index);
        if self.apply_list(list) {
            Some(Box::new(entry))
        } else {
            None
        }
    }
    fn insert_entry(&mut self, index: usize, entry: Box<dyn Any>) -> Result<(), Box<dyn Any>> {
        let entry = entry.downcast::<K::Entry>()?;
        let mut list = self.current.clone();
        let position = index.min(list.len());
        list.insert(position, *entry);
        if self.apply_list(list) {
            self.follow_entry(position);
        }
        Ok(())
    }
    fn followed(&self) -> Option<usize> {
        match self.follow.as_ref()? {
            Follow::Index(index) => Some((*index).min(self.current.len().checked_sub(1)?)),
            Follow::Identity(identity) => self
                .current
                .iter()
                .position(|entry| &self.kind.identity(entry) == identity),
        }
    }
    fn clear_follow(&mut self) {
        self.follow = None;
    }
    fn is_busy(&self) -> bool {
        self.form.is_some() || self.dialog.is_open()
    }
    fn handle_busy_key(&mut self, key: &KeyWithModifier) {
        if self.dialog.is_open() {
            let response = self.dialog.handle_key(key);
            self.dialog_response(response);
            return;
        }
        let base_mode = self.base_mode;
        let for_menu = self.for_menu;
        if let Some(form) = self.form.as_mut() {
            let result = form.handle_key(key, base_mode, for_menu);
            self.form_result(result);
        }
    }
    fn handle_busy_mouse(&mut self, mouse: Mouse) {
        if self.dialog.is_open() {
            let response = self.dialog.handle_mouse(mouse);
            self.dialog_response(response);
            return;
        }
        let base_mode = self.base_mode;
        let for_menu = self.for_menu;
        if let Some(form) = self.form.as_mut() {
            let result = form.handle_mouse(mouse, base_mode, for_menu);
            self.form_result(result);
        }
    }
    fn handle_timer(&mut self) -> bool {
        self.form
            .as_mut()
            .map(|form| form.handle_timer())
            .unwrap_or(false)
    }
    fn render_busy(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if let Some(form) = self.form.as_mut() {
            if !form.dialog {
                form.render(x, y, width, height);
            }
        }
    }
    fn busy_over_list(&self) -> bool {
        self.dialog_form || (self.form.is_none() && self.dialog.is_open())
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        if let Some(form) = self.form.as_mut() {
            form.render_overlays(rows, cols);
        }
        self.dialog.render_centered(rows, cols);
    }
    fn busy_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.dialog.is_open() {
            return vec![
                ("<←→>", "choose"),
                ("<Enter>", "confirm"),
                ("<Esc>", "cancel"),
            ];
        }
        self.form
            .as_ref()
            .map(|form| form.hints())
            .unwrap_or_default()
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }
    fn close(&mut self) {
        self.form = None;
        if self.dialog.is_open() {
            self.dialog.close();
        }
        self.pending_delete = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Row {
    Gap,
    Heading(usize),
    Entry(usize, usize),
    Actions(usize),
}

impl Row {
    fn selectable(&self) -> bool {
        matches!(self, Row::Entry(..) | Row::Actions(_))
    }
    fn section(&self) -> Option<usize> {
        match self {
            Row::Heading(section)
            | Row::Entry(section, _)
            | Row::Actions(section) => {
                Some(*section)
            },
            Row::Gap => None,
        }
    }
}

const ACTION_GAP: usize = 3;

pub struct SectionedList {
    pub sections: Vec<Box<dyn Section>>,
    selected: Row,
    action_choice: usize,
    cross_moves: bool,
    scroll: RowScroll,
    action_spans: Vec<(usize, Vec<(usize, usize)>)>,
    hovered_action: Option<(usize, usize)>,
    shared_columns: bool,
    effects: Vec<Effect>,
    notice: Option<String>,
    focused: bool,
    styled: Option<&'static str>,
    search: TextInput,
    filter: Dropdown,
    header_focus: HeaderFocus,
    section_buttons: Vec<Vec<Button>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderFocus {
    Search,
    Filter,
    List,
}

const ALL_SECTIONS: &str = "All";

fn styled_marker(marker: &str) -> String {
    if marker.starts_with('●') {
        "● unsaved".to_owned()
    } else {
        String::new()
    }
}

struct EntryMatch {
    columns: Vec<Vec<usize>>,
    score: (usize, usize),
}

fn match_columns(query: &str, columns: &[String]) -> Option<EntryMatch> {
    let candidate = columns.join(" ");
    let indices = fuzzy_match_indices(query, &candidate)?;
    let first = indices.first().copied().unwrap_or(0);
    let last = indices.last().copied().unwrap_or(0);
    let mut per_column = vec![vec![]; columns.len()];
    let mut start = 0;
    for (column, text) in columns.iter().enumerate() {
        let length = text.chars().count();
        for index in &indices {
            if *index >= start && *index < start + length {
                per_column[column].push(index - start);
            }
        }
        start += length + 1;
    }
    Some(EntryMatch {
        columns: per_column,
        score: (last - first, first),
    })
}

fn plain_entry_styles(
    columns: &[String],
    offset: usize,
    found: Option<&EntryMatch>,
) -> Vec<ColumnStyle> {
    let mut styles = vec![];
    if let Some(name) = columns.get(offset) {
        for index in 0..name.chars().count() {
            let matched = found
                .and_then(|found| found.columns.first())
                .map(|indices| indices.contains(&index))
                .unwrap_or(false);
            styles.push((offset, if matched { 1 } else { 3 }, index..index + 1));
        }
    }
    if let Some(found) = found {
        for (column, indices) in found.columns.iter().enumerate().skip(1) {
            for index in indices {
                styles.push((offset + column, 1, *index..*index + 1));
            }
        }
    }
    styles
}

fn menu_entry_styles(
    columns: &[String],
    offset: usize,
    found: Option<&EntryMatch>,
) -> Vec<ColumnStyle> {
    let mut styles = vec![];
    if let Some(actions) = columns.get(offset + 1) {
        let mut start = 0;
        for action in actions.split("; ") {
            if let Some(range) = action_argument_range(action) {
                styles.push((offset + 1, 0, start + range.start..start + range.end));
            }
            start += action.chars().count() + 2;
        }
    }
    if let Some(shortcut) = columns.get(offset + 2) {
        styles.push((offset + 2, DIM, 0..shortcut.chars().count()));
    }
    if let Some(found) = found {
        for (column, indices) in found.columns.iter().enumerate() {
            for index in indices {
                styles.push((offset + column, 1, *index..*index + 1));
            }
        }
    }
    styles
}

impl SectionedList {
    pub fn new(sections: Vec<Box<dyn Section>>) -> Self {
        SectionedList {
            sections,
            selected: Row::Entry(0, 0),
            action_choice: 0,
            cross_moves: false,
            scroll: RowScroll::default(),
            action_spans: vec![],
            hovered_action: None,
            shared_columns: false,
            effects: vec![],
            notice: None,
            focused: false,
            styled: None,
            search: TextInput::empty()
                .placeholder("/ to search")
                .search_mode(),
            filter: Dropdown::new("Menu", vec![ALL_SECTIONS.to_owned()])
                .label_width(SHORT_LABEL_WIDTH)
                .label_color(0)
                .accent_brackets(),
            header_focus: HeaderFocus::List,
            section_buttons: vec![],
        }
    }
    pub fn styled(mut self, title: &'static str, filter_label: &'static str) -> Self {
        self.styled = Some(title);
        let options: Vec<String> = std::iter::once(ALL_SECTIONS.to_owned())
            .chain(self.sections.iter().map(|section| section.short_heading()))
            .collect();
        self.filter = Dropdown::new(filter_label, options)
            .label_width(SHORT_LABEL_WIDTH)
            .label_color(0)
            .accent_brackets();
        self
    }
    fn query(&self) -> String {
        self.search.get_text().trim().to_lowercase()
    }
    fn searching(&self) -> bool {
        self.styled.is_some() && !self.query().is_empty()
    }
    fn section_shown(&self, section: usize) -> bool {
        let selected = self.filter.selected_index();
        selected == 0 || selected == section + 1
    }
    pub fn captures_keys(&self) -> bool {
        self.is_busy()
            || (self.styled.is_some()
                && (self.header_focus == HeaderFocus::Search || self.filter.is_open()))
    }
    fn select_first_entry(&mut self) {
        let rows = self.rows();
        if let Some(row) = rows.iter().find(|row| row.selectable()).copied() {
            self.select(row);
        }
        self.scroll.reset();
    }
    fn styled_columns(&self, section: usize, entry: usize) -> Vec<String> {
        let mut columns = self.sections[section].columns(entry);
        if self.searching() {
            columns.insert(0, self.sections[section].short_heading());
        }
        columns
    }
    fn header_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        self.styled?;
        match self.header_focus {
            HeaderFocus::Search => {
                if is_plain(key, BareKey::Enter)
                    || is_plain(key, BareKey::Down)
                    || is_plain(key, BareKey::Tab)
                {
                    if self.searching() {
                        self.header_focus = HeaderFocus::List;
                        self.select_first_entry();
                    } else {
                        self.header_focus = HeaderFocus::Filter;
                    }
                    return Some(true);
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    return Some(false);
                }
                if is_plain(key, BareKey::Esc) {
                    if self.searching() {
                        self.search.clear();
                        self.select_first_entry();
                        return Some(true);
                    }
                    return Some(false);
                }
                match self.search.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first_entry();
                        Some(true)
                    },
                    UiResponse::NotHandled => Some(false),
                    _ => Some(true),
                }
            },
            HeaderFocus::Filter => {
                if self.filter.is_open() {
                    if let UiResponse::Changed(_) = self.filter.handle_key(key) {
                        self.select_first_entry();
                    }
                    return Some(true);
                }
                if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
                    self.header_focus = HeaderFocus::List;
                    self.select_first_entry();
                    return Some(true);
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) || typed(key, '/') {
                    self.header_focus = HeaderFocus::Search;
                    return Some(true);
                }
                if is_plain(key, BareKey::Esc) || is_plain(key, BareKey::Left) {
                    return Some(false);
                }
                match self.filter.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first_entry();
                        Some(true)
                    },
                    UiResponse::NotHandled => Some(false),
                    _ => Some(true),
                }
            },
            HeaderFocus::List => {
                if typed(key, '/') {
                    self.header_focus = HeaderFocus::Search;
                    return Some(true);
                }
                if is_plain(key, BareKey::Esc) && self.searching() {
                    self.search.clear();
                    self.select_first_entry();
                    return Some(true);
                }
                if is_plain(key, BareKey::Up) {
                    let rows = self.rows();
                    let first = rows.iter().position(|row| row.selectable());
                    let current = rows.iter().position(|row| *row == self.selected);
                    if first.is_none() || first == current {
                        self.header_focus = if self.searching() {
                            HeaderFocus::Search
                        } else {
                            HeaderFocus::Filter
                        };
                        return Some(true);
                    }
                }
                if is_plain(key, BareKey::Delete) {
                    if let Row::Entry(section, entry) = self.selected {
                        self.sections[section].request_delete(entry);
                        return Some(true);
                    }
                    return Some(false);
                }
                if typed(key, 'd') {
                    return Some(false);
                }
                None
            },
        }
    }
    fn styled_mouse(&mut self, mouse: Mouse) -> Option<bool> {
        self.styled?;
        if self.filter.is_open() || matches!(mouse, Mouse::LeftClick(..) | Mouse::Hover(..)) {
            let response = self.filter.handle_mouse(mouse);
            if let UiResponse::Changed(_) = response {
                self.select_first_entry();
            }
            if response.is_handled() && !matches!(mouse, Mouse::Hover(..)) {
                self.header_focus = HeaderFocus::Filter;
                return Some(true);
            }
            if self.filter.is_open() {
                return Some(true);
            }
        }
        self.search.handle_mouse(mouse);
        if let Some((line, column)) = is_click(&mouse) {
            if self.search.hit_test(line, column) {
                self.header_focus = HeaderFocus::Search;
                return Some(true);
            }
        }
        let mouse = outside_overlays(mouse);
        let mut activated = None;
        let mut changed = false;
        for (section, buttons) in self.section_buttons.iter_mut().enumerate() {
            for (index, button) in buttons.iter_mut().enumerate() {
                match button.handle_mouse(mouse) {
                    UiResponse::Activated => activated = Some((section, index)),
                    UiResponse::NotHandled => {},
                    _ => changed = true,
                }
            }
        }
        if let Some((section, index)) = activated {
            self.header_focus = HeaderFocus::List;
            self.select(Row::Actions(section));
            self.action_choice = index;
            self.sections[section].run_action(index, None);
            self.collect();
            return Some(true);
        }
        if matches!(mouse, Mouse::LeftClick(..)) {
            self.header_focus = HeaderFocus::List;
        }
        if changed {
            if let Mouse::Hover(line, column) = mouse {
                self.hover(line, column);
            }
            return Some(true);
        }
        None
    }
    pub fn with_cross_moves(mut self) -> Self {
        self.cross_moves = true;
        self
    }
    pub fn with_shared_columns(mut self) -> Self {
        self.shared_columns = true;
        self
    }
    fn action_at(&self, row_index: usize, column: usize) -> Option<usize> {
        self.action_spans
            .iter()
            .find(|(span_row, _)| *span_row == row_index)
            .and_then(|(_, spans)| {
                spans
                    .iter()
                    .position(|(start, end)| column >= *start && column < *end)
            })
    }
    fn hover(&mut self, line: isize, column: usize) -> bool {
        let before = (self.scroll.hovered, self.hovered_action);
        let row_index = self.scroll.row_at(line, column);
        let rows = self.rows();
        let hovered_row =
            row_index.filter(|index| matches!(rows.get(*index), Some(Row::Entry(..))));
        self.scroll.hovered = hovered_row;
        self.hovered_action =
            row_index.and_then(|index| self.action_at(index, column).map(|action| (index, action)));
        before != (self.scroll.hovered, self.hovered_action)
    }
    fn layouts(&self, width: usize) -> Vec<ColumnLayout> {
        let rows: Vec<Vec<(Vec<String>, String)>> = self
            .sections
            .iter()
            .map(|section| {
                (0..section.len())
                    .map(|entry| (section.columns(entry), section.marker(entry)))
                    .collect()
            })
            .collect();
        let layout_of = |rows: &[&(Vec<String>, String)]| {
            ColumnLayout::new(
                rows.iter()
                    .map(|(columns, marker)| (columns.as_slice(), marker.as_str())),
                width,
            )
        };
        if self.shared_columns {
            let all: Vec<&(Vec<String>, String)> = rows.iter().flatten().collect();
            let shared = layout_of(&all);
            vec![shared; self.sections.len()]
        } else {
            let mut layouts: Vec<ColumnLayout> = rows
                .iter()
                .map(|section| layout_of(&section.iter().collect::<Vec<_>>()))
                .collect();
            ColumnLayout::align_markers(&mut layouts);
            layouts
        }
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
    #[cfg(test)]
    pub fn selected(&self) -> Row {
        self.selected
    }
    pub fn rows(&self) -> Vec<Row> {
        if self.styled.is_some() {
            return self.styled_rows();
        }
        let mut rows = vec![];
        for (index, section) in self.sections.iter().enumerate() {
            if index > 0 {
                rows.push(Row::Gap);
            }
            rows.push(Row::Heading(index));
            for entry in 0..section.len() {
                rows.push(Row::Entry(index, entry));
            }
            rows.push(Row::Actions(index));
        }
        rows
    }
    fn styled_rows(&self) -> Vec<Row> {
        let mut rows = vec![];
        if self.searching() {
            let query = self.query();
            let mut found: Vec<(Row, (usize, usize))> = vec![];
            for (section_index, section) in self.sections.iter().enumerate() {
                for entry in 0..section.len() {
                    let columns = section.columns(entry);
                    if let Some(matched) = match_columns(&query, &columns) {
                        found.push((Row::Entry(section_index, entry), matched.score));
                    }
                }
            }
            found.sort_by_key(|(_, score)| *score);
            return found.into_iter().map(|(row, _)| row).collect();
        }
        for (index, section) in self.sections.iter().enumerate() {
            if !self.section_shown(index) {
                continue;
            }
            if !rows.is_empty() {
                rows.push(Row::Gap);
            }
            rows.push(Row::Heading(index));
            for entry in 0..section.len() {
                rows.push(Row::Entry(index, entry));
            }
            rows.push(Row::Actions(index));
        }
        rows
    }
    fn add_action_count(&self, section: usize) -> usize {
        self.sections[section].actions().len()
    }
    fn select(&mut self, row: Row) {
        if self.selected != row {
            self.action_choice = 0;
        }
        self.selected = row;
        self.scroll.follow();
    }
    fn normalize_selection(&mut self) {
        if self.styled.is_some() {
            let rows = self.rows();
            if !rows.contains(&self.selected) {
                let fallback = match self.selected {
                    Row::Entry(section, entry) => rows
                        .iter()
                        .filter(|row| matches!(row, Row::Entry(s, _) if *s == section))
                        .copied()
                        .min_by_key(|row| match row {
                            Row::Entry(_, e) => e.abs_diff(entry),
                            _ => usize::MAX,
                        }),
                    _ => None,
                };
                self.selected = fallback
                    .or_else(|| rows.iter().find(|row| row.selectable()).copied())
                    .unwrap_or(Row::Actions(0));
            }
            return;
        }
        let row = match self.selected {
            Row::Entry(section, entry) => match self.sections.get(section) {
                Some(found) if found.len() == 0 => Row::Actions(section),
                Some(found) => Row::Entry(section, entry.min(found.len() - 1)),
                None => Row::Actions(0),
            },
            Row::Actions(section) if section < self.sections.len() => Row::Actions(section),
            _ => Row::Actions(0),
        };
        self.selected = row;
    }
    fn follow_sections(&mut self, clear: bool) {
        for index in 0..self.sections.len() {
            if let Some(entry) = self.sections[index].followed() {
                self.select(Row::Entry(index, entry));
            }
            if clear {
                self.sections[index].clear_follow();
            }
        }
        self.normalize_selection();
    }
    pub fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        for section in self.sections.iter_mut() {
            section.set_snapshot(snapshot);
        }
        self.follow_sections(true);
    }
    fn collect(&mut self) {
        for section in self.sections.iter_mut() {
            let effects = section.take_effects();
            if effects.is_empty() {
                section.clear_follow();
            }
            self.effects.extend(effects);
            if let Some(notice) = section.take_notice() {
                self.notice = Some(notice);
            }
        }
        self.follow_sections(false);
    }
    pub fn busy_section(&self) -> Option<usize> {
        self.sections.iter().position(|section| section.is_busy())
    }
    pub fn is_busy(&self) -> bool {
        self.busy_section().is_some()
    }
    fn move_selection(&mut self, steps: isize) -> bool {
        let rows = self.rows();
        let Some(current) = rows.iter().position(|row| *row == self.selected) else {
            return false;
        };
        let selectable: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.selectable())
            .map(|(index, _)| index)
            .collect();
        let Some(position) = selectable.iter().position(|index| *index == current) else {
            return false;
        };
        let target = (position as isize + steps).clamp(0, selectable.len() as isize - 1) as usize;
        if target == position {
            return false;
        }
        self.select(rows[selectable[target]]);
        true
    }
    fn entry_after(&self) -> (usize, Option<usize>) {
        match self.selected {
            Row::Entry(section, entry) => (section, Some(entry)),
            other => (other.section().unwrap_or(0), None),
        }
    }
    fn move_entry(&mut self, down: bool) {
        let Row::Entry(section, entry) = self.selected else {
            return;
        };
        if !self.sections[section].ordered() {
            self.notice = Some("The order of these entries does not matter".to_owned());
            return;
        }
        if self.sections[section].move_entry(entry, down) {
            return;
        }
        if !self.cross_moves {
            return;
        }
        let target = if down {
            section + 1
        } else {
            match section.checked_sub(1) {
                Some(target) => target,
                None => return,
            }
        };
        if target >= self.sections.len() {
            return;
        }
        let Some(taken) = self.sections[section].take_entry(entry) else {
            return;
        };
        let position = if down { 0 } else { self.sections[target].len() };
        if let Err(taken) = self.sections[target].insert_entry(position, taken) {
            let _ = self.sections[section].insert_entry(entry, taken);
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> bool {
        if let Some(busy) = self.busy_section() {
            self.sections[busy].handle_busy_key(key);
            self.collect();
            return true;
        }
        if let Some(handled) = self.header_key(key) {
            self.collect();
            return handled;
        }
        if self.styled.is_some() && self.header_focus != HeaderFocus::List {
            return false;
        }
        let handled = self.handle_list_key(key);
        self.collect();
        handled
    }
    fn handle_list_key(&mut self, key: &KeyWithModifier) -> bool {
        let (section, after) = self.entry_after();
        if is_plain(key, BareKey::Down) {
            return self.move_selection(1);
        } else if is_plain(key, BareKey::Up) {
            return self.move_selection(-1);
        } else if is_plain(key, BareKey::PageDown) {
            self.move_selection(10);
        } else if is_plain(key, BareKey::PageUp) {
            self.move_selection(-10);
        } else if is_plain(key, BareKey::Home) {
            self.move_selection(-10_000);
        } else if is_plain(key, BareKey::End) {
            self.move_selection(10_000);
        } else if is_plain(key, BareKey::Right) {
            if let Row::Actions(section) = self.selected {
                let count = self.add_action_count(section);
                self.action_choice = (self.action_choice + 1).min(count.saturating_sub(1));
            } else {
                return false;
            }
        } else if is_plain(key, BareKey::Left) {
            match self.selected {
                Row::Actions(_) if self.action_choice > 0 => self.action_choice -= 1,
                _ => return false,
            }
        } else if is_move_key(key, BareKey::Down) {
            self.move_entry(true);
        } else if is_move_key(key, BareKey::Up) {
            self.move_entry(false);
        } else if is_plain(key, BareKey::Enter) || is_plain(key, BareKey::Char(' ')) {
            match self.selected {
                Row::Entry(section, entry) => self.sections[section].open_edit(entry),
                Row::Actions(section) => {
                    self.sections[section].run_action(self.action_choice, None)
                },
                _ => return false,
            }
        } else if typed(key, 'a') {
            self.sections[section].open_add(after);
        } else if typed(key, 's') {
            if !self.sections[section].add_separator(after) {
                return false;
            }
        } else if typed(key, 'd') {
            match self.selected {
                Row::Entry(section, entry) => self.sections[section].request_delete(entry),
                _ => return false,
            }
        } else if typed(key, 'r') {
            match self.selected {
                Row::Entry(section, entry) => self.sections[section].revert(entry),
                _ => return false,
            }
        } else if typed(key, 'R') {
            if !self.sections[section].restore_defaults() {
                return false;
            }
        } else {
            return false;
        }
        true
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if let Some(busy) = self.busy_section() {
            self.sections[busy].handle_busy_mouse(mouse);
            self.collect();
            return true;
        }
        if let Some(handled) = self.styled_mouse(mouse) {
            return handled;
        }
        if let Mouse::Hover(line, column) = mouse {
            return self.hover(line, column);
        }
        if let Some(changed) = self.scroll.handle_wheel(&mouse) {
            return changed;
        }
        let Some((line, column)) = is_click(&mouse) else {
            return false;
        };
        let Some(row_index) = self.scroll.row_at(line, column) else {
            return false;
        };
        let rows = self.rows();
        let Some(row) = rows.get(row_index).copied() else {
            return false;
        };
        match row {
            Row::Entry(section, entry) => {
                if self.focused && self.selected == row {
                    self.sections[section].open_edit(entry);
                } else {
                    self.select(row);
                }
            },
            Row::Actions(section) => match self.action_at(row_index, column) {
                Some(action) => {
                    self.select(row);
                    self.action_choice = action;
                    self.sections[section].run_action(action, None);
                },
                None => self.select(row),
            },
            _ => {},
        }
        self.collect();
        true
    }
    pub fn handle_timer(&mut self) -> bool {
        let mut changed = false;
        for buttons in self.section_buttons.iter_mut() {
            for button in buttons.iter_mut() {
                changed |= button.handle_timer();
            }
        }
        for section in self.sections.iter_mut() {
            changed |= section.handle_timer();
        }
        changed
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        if let Some(busy) = self.busy_section() {
            if !self.sections[busy].busy_over_list() {
                self.scroll.clear();
                self.sections[busy].render_busy(x, y, width, height);
                return;
            }
        }
        if let Some(title) = self.styled {
            self.render_styled(title, x, y, width, height);
            return;
        }
        let rows = self.rows();
        let selected_index = rows.iter().position(|row| *row == self.selected);
        let visible = self
            .scroll
            .layout(x, y, width, height, rows.len(), selected_index);
        self.action_spans.clear();
        let width = self.scroll.row_width();
        let layouts = self.layouts(width);
        for row_index in visible {
            let Some(screen_y) = self.scroll.screen_row(row_index) else {
                continue;
            };
            let is_selected = self.focused && Some(row_index) == selected_index;
            match rows[row_index] {
                Row::Gap => {},
                Row::Heading(section) => {
                    print_heading(&self.sections[section].heading(), x, screen_y, width)
                },
                Row::Entry(section, entry) => {
                    let columns = self.sections[section].columns(entry);
                    let marker = self.sections[section].marker(entry);
                    let look = RowLook::new(is_selected, self.scroll.is_hovered(row_index));
                    layouts[section].print(&columns, &marker, x, screen_y, width, look);
                },
                Row::Actions(section) => {
                    let mut column = x + 2;
                    let mut spans = vec![];
                    for (index, label) in self.sections[section].actions().iter().enumerate() {
                        let label_width = label.chars().count();
                        if column + label_width > x + width {
                            break;
                        }
                        let look = RowLook::new(
                            is_selected && index == self.action_choice,
                            self.hovered_action == Some((row_index, index)),
                        );
                        let text = look.apply(Text::new(label).color_all(3));
                        print_text_with_coordinates(text, column, screen_y, None, None);
                        spans.push((column, column + label_width));
                        column += label_width + ACTION_GAP;
                    }
                    self.action_spans.push((row_index, spans));
                },
            }
        }
    }
    fn render_styled(&mut self, title: &str, x: usize, y: usize, width: usize, height: usize) {
        let list_focused = self.focused && self.header_focus == HeaderFocus::List;
        print_text_with_coordinates(
            Text::new(truncate(title, width)).color_all(2),
            x,
            y,
            None,
            None,
        );
        let search_x = title.chars().count() + 2;
        let search_end = BESIDE_SHORT_FIELD + Button::new("Unmark all").natural_width();
        let search_focused = self.focused && self.header_focus == HeaderFocus::Search;
        self.search.set_focused(search_focused);
        self.search.set_show_cursor(search_focused);
        self.search.render(
            x + search_x,
            y,
            search_end.min(width).saturating_sub(search_x),
        );
        let searching = self.searching();
        let query = self.query();
        self.filter
            .set_focused(self.focused && self.header_focus == HeaderFocus::Filter);
        let list_offset = if searching {
            self.filter.clear_area();
            2
        } else {
            self.filter.render(x, y + 2, SHORT_FIELD_WIDTH.min(width));
            4
        };
        let list_y = y + list_offset;
        let list_height = height.saturating_sub(list_offset).max(1);
        let rows = self.rows();
        let offset = if searching { 1 } else { 0 };
        let entry_rows: Vec<(usize, Vec<String>, String, Vec<ColumnStyle>)> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| match row {
                Row::Entry(section, entry) => {
                    let columns = self.styled_columns(*section, *entry);
                    let found = if searching {
                        match_columns(&query, &columns[offset..])
                    } else {
                        None
                    };
                    let menu_like = self.sections[*section].menu_like();
                    let mut styles = if menu_like {
                        menu_entry_styles(&columns, offset, found.as_ref())
                    } else {
                        plain_entry_styles(&columns, offset, found.as_ref())
                    };
                    if searching {
                        styles.insert(0, (0, DIM, 0..columns[0].chars().count()));
                    }
                    let full_marker = self.sections[*section].marker(*entry);
                    let marker = if menu_like {
                        styled_marker(&full_marker)
                    } else {
                        full_marker
                    };
                    Some((index, columns, marker, styles))
                },
                _ => None,
            })
            .collect();
        let shared = self.shared_columns || searching;
        let section_of = |row_index: usize| match rows[row_index] {
            Row::Entry(section, _) => section,
            _ => 0,
        };
        let group_count = if shared { 1 } else { self.sections.len() };
        let mut naturals: Vec<ColumnLayout> = (0..group_count)
            .map(|group| {
                ColumnLayout::new(
                    entry_rows
                        .iter()
                        .filter(|(index, _, _, _)| shared || section_of(*index) == group)
                        .map(|(_, columns, marker, _)| (columns.as_slice(), marker.as_str())),
                    usize::MAX,
                )
                .with_indent(0)
            })
            .collect();
        ColumnLayout::align_markers(&mut naturals);
        let headings_width = rows
            .iter()
            .filter_map(|row| match row {
                Row::Actions(section) => Some(
                    self.sections[*section]
                        .actions()
                        .iter()
                        .map(|label| label.chars().count() + 5)
                        .sum(),
                ),
                Row::Heading(section) => {
                    Some(self.sections[*section].short_heading().chars().count())
                },
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let content_width = naturals
            .iter()
            .map(|natural| natural.natural_width("● unsaved".chars().count()))
            .max()
            .unwrap_or(0)
            .max(headings_width);
        let list_width = RowScroll::fitted_width(rows.len(), list_height, content_width, width);
        let selected_index = rows.iter().position(|row| *row == self.selected);
        let visible = self.scroll.layout(
            x,
            list_y,
            list_width,
            list_height,
            rows.len(),
            selected_index,
        );
        self.action_spans.clear();
        let row_width = self.scroll.row_width();
        let menu_like = self
            .sections
            .first()
            .map(|section| section.menu_like())
            .unwrap_or(false);
        let shrink_order: Vec<usize> = if menu_like {
            vec![offset + 1, offset, offset + 2, 0]
        } else {
            (0..offset + 3).rev().collect()
        };
        let mut layouts: Vec<ColumnLayout> = naturals
            .iter()
            .map(|natural| {
                let mut natural = natural.clone();
                natural.marker_column = 0;
                natural.fit(row_width, &shrink_order)
            })
            .collect();
        ColumnLayout::align_markers(&mut layouts);
        if rows.is_empty() {
            let empty = if searching {
                "No matching items"
            } else {
                "Nothing here"
            };
            print_dim(empty, x, list_y, width);
        }
        self.section_buttons
            .resize_with(self.sections.len(), Vec::new);
        for buttons in self.section_buttons.iter_mut() {
            for button in buttons.iter_mut() {
                button.clear_area();
            }
        }
        for row_index in visible {
            let Some(screen_y) = self.scroll.screen_row(row_index) else {
                continue;
            };
            let is_selected = list_focused && Some(row_index) == selected_index;
            match rows[row_index] {
                Row::Gap => {},
                Row::Heading(section) => {
                    let heading = self.sections[section].short_heading();
                    let mut line = heading.clone();
                    if let Some(note) = self.sections[section].heading_note() {
                        line.push_str("  ");
                        line.push_str(&note);
                    }
                    let length = heading.chars().count();
                    print_text_with_coordinates(
                        Text::new(truncate(&line, row_width))
                            .color_range(0, ..length)
                            .dim_range(length..),
                        x,
                        screen_y,
                        None,
                        None,
                    );
                },
                Row::Actions(section) => {
                    self.prepare_section_buttons(section);
                    let choice = self.action_choice;
                    let mut column = x;
                    for (index, button) in self.section_buttons[section].iter_mut().enumerate() {
                        let button_width = button.natural_width();
                        if column + button_width > x + row_width.max(list_width) {
                            break;
                        }
                        button.set_focused(is_selected && index == choice);
                        button.render(column, screen_y);
                        column += button_width + 1;
                    }
                },
                Row::Entry(section, entry) if self.sections[section].is_separator(entry) => {
                    let look = RowLook::new(is_selected, self.scroll.is_hovered(row_index));
                    let label = " separator ";
                    let rule = row_width.saturating_sub(label.chars().count());
                    let line = format!(
                        "{}{}{}",
                        "─".repeat(rule / 2),
                        label,
                        "─".repeat(rule - rule / 2)
                    );
                    print_text_with_coordinates(
                        look.apply(Text::new(line).dim_all()),
                        x,
                        screen_y,
                        None,
                        None,
                    );
                },
                Row::Entry(..) => {
                    let Some((_, columns, marker, styles)) = entry_rows
                        .iter()
                        .find(|(index, _, _, _)| *index == row_index)
                    else {
                        continue;
                    };
                    let look = RowLook::new(is_selected, self.scroll.is_hovered(row_index));
                    let group = if shared { 0 } else { section_of(row_index) };
                    layouts[group].print_styled(
                        columns, marker, x, screen_y, row_width, look, styles,
                    );
                },
            }
        }
    }
    fn prepare_section_buttons(&mut self, section: usize) {
        let labels = self.sections[section].actions();
        let buttons = &mut self.section_buttons[section];
        if buttons.len() != labels.len()
            || buttons
                .iter()
                .zip(labels.iter())
                .any(|(button, label)| button.label() != label)
        {
            *buttons = labels
                .iter()
                .map(|label| {
                    let button = Button::new(label.clone()).accent_brackets();
                    button
                })
                .collect();
        }
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        if self.styled.is_some() && self.filter.is_open() {
            self.filter.render_overlay(rows, cols);
            note_dropdown(&self.filter);
        }
        if let Some(busy) = self.busy_section() {
            self.sections[busy].render_overlays(rows, cols);
        }
    }
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if let Some(busy) = self.busy_section() {
            return self.sections[busy].busy_hints();
        }
        if self.styled.is_some() {
            match self.header_focus {
                HeaderFocus::Search => {
                    return vec![
                        ("<type>", "search items and actions"),
                        ("<↓>", "results"),
                        ("<Esc>", "clear"),
                    ]
                },
                HeaderFocus::Filter => {
                    return vec![("<Space>", "choose menu"), ("<↓>", "items"), ("<Esc>", "close")]
                },
                HeaderFocus::List => {},
            }
        }
        let mut hints = vec![("<↓↑>", "move")];
        let delete_key = if self.styled.is_some() { "<Del>" } else { "<d>" };
        match self.selected {
            Row::Actions(_) => {
                hints.push(("<←→>", "choose"));
                hints.push(("<Enter>", "do it"));
            },
            _ => {
                hints.push(("<Enter>", "edit"));
                hints.push((delete_key, "delete"));
                hints.push(("<r>", "revert"));
            },
        }
        hints.push(("<a>", "add"));
        if self.sections.iter().any(|section| section.has_separators()) {
            hints.push(("<s>", "separator"));
        }
        if self.sections.iter().any(|section| section.ordered()) {
            hints.push(("<Shift ↓↑>", "move entry"));
        }
        if self
            .sections
            .iter()
            .any(|section| section.can_restore_defaults())
        {
            hints.push(("<R>", "restore defaults"));
        }
        if self.styled.is_some() {
            hints.push(("</>", "search"));
        }
        hints
    }
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }
    pub fn close(&mut self) {
        for section in self.sections.iter_mut() {
            section.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Pair(String, String);

    struct Pairs {
        ordered: bool,
        key: SettingKey,
    }

    impl EntryKind for Pairs {
        type Entry = Pair;
        fn title(&self) -> String {
            "Pairs".to_owned()
        }
        fn noun(&self) -> &'static str {
            "pair"
        }
        fn identity(&self, entry: &Pair) -> String {
            entry.0.clone()
        }
        fn summary(&self, entry: &Pair) -> String {
            format!("{}={}", entry.0, entry.1)
        }
        fn fields(&self) -> Vec<FieldSpec> {
            vec![FieldSpec::text("Name", ""), FieldSpec::text("Value", "")]
        }
        fn to_values(&self, entry: &Pair) -> Vec<FieldValue> {
            vec![
                FieldValue::Text(entry.0.clone()),
                FieldValue::Text(entry.1.clone()),
            ]
        }
        fn from_values(&self, values: &[FieldValue]) -> Result<Pair, String> {
            let name = values[0].text();
            if name.is_empty() {
                return Err("A name is needed".to_owned());
            }
            Ok(Pair(name, values[1].text()))
        }
        fn block_kdl(&self, entries: &[Pair]) -> Result<String, String> {
            Ok(entries
                .iter()
                .map(|entry| format!("{}={}", entry.0, entry.1))
                .collect::<Vec<_>>()
                .join(","))
        }
        fn block_key(&self) -> SettingKey {
            self.key
        }
        fn entries(&self, _blocks: &ConfigBlocks) -> Vec<Pair> {
            vec![]
        }
        fn ordered(&self) -> bool {
            self.ordered
        }
        fn separator(&self) -> Option<Pair> {
            if self.ordered {
                Some(Pair("-".to_owned(), String::new()))
            } else {
                None
            }
        }
        fn unique_identity(&self) -> bool {
            !self.ordered
        }
    }

    fn pair(name: &str, value: &str) -> Pair {
        Pair(name.to_owned(), value.to_owned())
    }

    fn editor(ordered: bool) -> ListEditor<Pairs> {
        let mut editor = ListEditor::new(Pairs {
            ordered,
            key: SettingKey::Env,
        });
        let entries = vec![pair("A", "1"), pair("B", "2")];
        editor.set_entries(entries.clone(), entries, vec![pair("A", "1")]);
        editor
    }

    fn key(character: char) -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Char(character))
    }

    fn enter() -> KeyWithModifier {
        KeyWithModifier::new(BareKey::Enter)
    }

    fn fill_form(editor: &mut ListEditor<Pairs>, name: &str, value: &str) {
        let form = editor.form_mut().unwrap();
        form.set_text(0, name);
        form.set_text(1, value);
    }

    fn list(ordered: bool) -> SectionedList {
        let mut first = editor(ordered);
        let mut second = editor(ordered);
        second.kind.key = SettingKey::LoadPlugins;
        first.set_entries(
            vec![pair("A", "1"), pair("B", "2")],
            vec![pair("A", "1"), pair("B", "2")],
            vec![],
        );
        second.set_entries(vec![pair("C", "3")], vec![pair("C", "3")], vec![]);
        let mut list = SectionedList::new(vec![Box::new(first), Box::new(second)]);
        if ordered {
            list = list.with_cross_moves();
        }
        list.selected = Row::Entry(0, 0);
        list
    }

    #[test]
    fn adding_an_entry_replaces_the_block_and_marks_it_unsaved() {
        let mut editor = editor(false);
        editor.open_add(None);
        fill_form(&mut editor, "C", "3");
        assert!(editor.apply_form());
        assert_eq!(
            editor.take_effects(),
            vec![Effect::ReplaceBlocks("A=1,B=2,C=3".to_owned())]
        );
        assert!(editor.is_unsaved(2));
        assert!(!editor.is_unsaved(0));
        assert!(editor.is_default(0));
        assert!(!editor.is_default(1));
        assert_eq!(Section::marker(&editor, 2), "● unsaved");
        assert_eq!(Section::marker(&editor, 0), "○ default");
        assert_eq!(editor.followed(), Some(2));
    }

    #[test]
    fn an_invalid_or_duplicate_entry_keeps_the_form_open() {
        let mut editor = editor(false);
        editor.open_add(None);
        fill_form(&mut editor, "", "3");
        assert!(!editor.apply_form());
        fill_form(&mut editor, "A", "3");
        assert!(!editor.apply_form());
        assert!(editor.is_busy());
        assert!(Section::take_effects(&mut editor).is_empty());
    }

    #[test]
    fn editing_an_entry_changes_it_in_place() {
        let mut editor = editor(false);
        editor.open_edit(1);
        fill_form(&mut editor, "B", "20");
        assert!(editor.apply_form());
        assert_eq!(
            Section::take_effects(&mut editor),
            vec![Effect::ReplaceBlocks("A=1,B=20".to_owned())]
        );
    }

    #[test]
    fn deleting_asks_first_and_reverting_restores_the_saved_list() {
        let mut editor = editor(false);
        editor.request_delete(0);
        assert!(editor.is_busy());
        assert!(Section::take_effects(&mut editor).is_empty());
        editor.handle_busy_key(&enter());
        assert_eq!(
            Section::take_effects(&mut editor),
            vec![Effect::ReplaceBlocks("B=2".to_owned())]
        );
        editor.open_add(None);
        fill_form(&mut editor, "A", "9");
        editor.apply_form();
        assert_eq!(
            Section::take_effects(&mut editor),
            vec![Effect::ReplaceBlocks("B=2,A=9".to_owned())]
        );
        assert!(editor.is_unsaved(1));
        Section::revert(&mut editor, 1);
        assert_eq!(
            Section::take_effects(&mut editor),
            vec![Effect::Revert(SettingKey::Env)]
        );
    }

    #[test]
    fn the_edit_form_has_delete_and_revert_buttons() {
        let mut editor = editor(false);
        editor.open_add(None);
        fill_form(&mut editor, "C", "3");
        editor.apply_form();
        Section::take_effects(&mut editor);
        editor.open_edit(2);
        editor.form_result(FormResult::Revert);
        assert!(!editor.is_busy());
        assert_eq!(
            Section::take_effects(&mut editor),
            vec![Effect::Revert(SettingKey::Env)]
        );
        editor.open_edit(0);
        editor.form_result(FormResult::Delete);
        assert!(editor.dialog.is_open());
    }

    #[test]
    fn the_list_moves_between_sections_and_their_action_rows() {
        let mut list = list(false);
        assert_eq!(
            list.rows(),
            vec![
                Row::Heading(0),
                Row::Entry(0, 0),
                Row::Entry(0, 1),
                Row::Actions(0),
                Row::Gap,
                Row::Heading(1),
                Row::Entry(1, 0),
                Row::Actions(1),
            ]
        );
        for _ in 0..3 {
            list.handle_key(&KeyWithModifier::new(BareKey::Down));
        }
        assert_eq!(list.selected(), Row::Entry(1, 0));
        list.handle_key(&KeyWithModifier::new(BareKey::Down));
        list.handle_key(&enter());
        assert!(list.is_busy());
        assert_eq!(list.busy_section(), Some(1));
        list.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert!(!list.is_busy());
        assert!(!list.handle_key(&KeyWithModifier::new(BareKey::Down)));
    }

    #[test]
    fn an_entry_moved_past_the_end_of_its_section_joins_the_next_one() {
        let mut list = list(true);
        list.selected = Row::Entry(0, 1);
        list.handle_key(&KeyWithModifier::new(BareKey::Down).with_alt_modifier());
        assert_eq!(
            list.take_effects(),
            vec![
                Effect::ReplaceBlocks("A=1".to_owned()),
                Effect::ReplaceBlocks("B=2,C=3".to_owned()),
            ]
        );
        assert_eq!(list.selected(), Row::Entry(1, 0));
        list.handle_key(&KeyWithModifier::new(BareKey::Up).with_alt_modifier());
        assert_eq!(
            list.take_effects(),
            vec![
                Effect::Revert(SettingKey::Env),
                Effect::Revert(SettingKey::LoadPlugins),
            ]
        );
        assert_eq!(list.selected(), Row::Entry(0, 1));
    }

    #[test]
    fn a_separator_is_added_after_the_selected_entry() {
        let mut list = list(true);
        list.handle_key(&key('s'));
        assert_eq!(
            list.take_effects(),
            vec![Effect::ReplaceBlocks("A=1,-=,B=2".to_owned())]
        );
        assert_eq!(list.selected(), Row::Entry(0, 1));
    }

    #[test]
    fn reordering_only_works_where_order_matters() {
        let mut unordered = list(false);
        unordered.handle_key(&KeyWithModifier::new(BareKey::Down).with_alt_modifier());
        assert!(unordered.take_effects().is_empty());
        let mut ordered = list(true);
        ordered.handle_key(&KeyWithModifier::new(BareKey::Down).with_alt_modifier());
        assert_eq!(
            ordered.take_effects(),
            vec![Effect::ReplaceBlocks("B=2,A=1".to_owned())]
        );
        assert_eq!(ordered.selected(), Row::Entry(0, 1));
        ordered.handle_key(&KeyWithModifier::new(BareKey::Up).with_alt_modifier());
        assert_eq!(
            ordered.take_effects(),
            vec![Effect::Revert(SettingKey::Env)]
        );
    }

    #[test]
    fn a_choice_field_hides_the_fields_that_depend_on_it() {
        let specs = vec![
            FieldSpec::choice("Kind", &["item", "separator"]),
            FieldSpec::text("Label", "").when(0, "item"),
        ];
        let mut form = EntryForm::new(
            "Edit".to_owned(),
            Some(0),
            specs,
            vec![
                FieldValue::Text("item".to_owned()),
                FieldValue::Text("Hi".to_owned()),
            ],
            vec![],
        );
        assert!(form.group.text_input(&FormKey::Text(1)).is_some());
        form.choose(0, "separator");
        assert!(form.group.text_input(&FormKey::Text(1)).is_none());
        assert_eq!(form.values()[1], FieldValue::Text("Hi".to_owned()));
    }

    #[test]
    fn a_styled_list_searches_every_section() {
        let mut list = list(false).styled("Pairs page", "Show");
        list.set_focused(true);
        assert!(list.handle_key(&key('/')));
        assert!(list.handle_key(&key('c')));
        assert_eq!(list.rows(), vec![Row::Entry(1, 0)]);
        assert!(list.handle_key(&KeyWithModifier::new(BareKey::Esc)));
        assert!(list.rows().contains(&Row::Entry(0, 1)));
    }

    #[test]
    fn a_styled_list_deletes_with_the_delete_key_and_moves_with_shift() {
        let mut list = list(true).styled("Pairs page", "Show");
        list.set_focused(true);
        assert!(list.handle_key(&KeyWithModifier::new(BareKey::Down).with_shift_modifier()));
        assert_eq!(
            list.take_effects(),
            vec![Effect::ReplaceBlocks("B=2,A=1".to_owned())]
        );
        assert!(!list.handle_key(&key('d')));
        assert!(list.handle_key(&KeyWithModifier::new(BareKey::Delete)));
        assert!(list.is_busy());
    }

    #[test]
    fn a_dialog_form_is_filled_from_the_keyboard() {
        let mut editor = editor(false).dialog_form();
        editor.open_add(None);
        assert_eq!(editor.form.as_ref().unwrap().title, "Add Pair");
        Section::handle_busy_key(&mut editor, &key('Z'));
        Section::handle_busy_key(&mut editor, &KeyWithModifier::new(BareKey::Down));
        Section::handle_busy_key(&mut editor, &key('9'));
        Section::handle_busy_key(&mut editor, &enter());
        assert_eq!(
            editor.take_effects(),
            vec![Effect::ReplaceBlocks("A=1,B=2,Z=9".to_owned())]
        );
        assert!(editor.form.is_none());
    }
}
