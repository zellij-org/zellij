use std::collections::BTreeMap;

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::mousebind_change_kdl_for;
use zellij_utils::input::mousebinds::MouseBehaviour;

use crate::action_picker::{ActionPicker, PickerResponse, DELETE_WIDTH};
use crate::bindings_list::{scope_text, BindingsList, ListEntry, ListResponse, ListTexts};
use crate::page::{
    actions_summary, changed_by, is_plain, is_shift_tab, note_group, note_overlay, print_dim,
    render_frame, truncate, Effect, Page, PageResponse,
};

const CLICKS: &str = "Clicks";
const MULTI_CLICKS: &str = "Double and triple clicks";
const SCROLLING: &str = "Scrolling";
const FRAME: &str = "Frame";
const PASSED: &str = "Passed to app / Ignored";
const OTHER: &str = "Other actions";
const VARIES: &str = "Varies by mode";
const VARIES_TEXT: &str = "varies by mode";
const CATEGORY_ORDER: [&str; 7] = [
    CLICKS,
    MULTI_CLICKS,
    SCROLLING,
    FRAME,
    PASSED,
    OTHER,
    VARIES,
];

const TEXTS: ListTexts = ListTexts {
    title: "Mouse bindings",
    search_placeholder: "/ to search mouse bindings and actions",
    search_hint: "search mouse bindings and actions",
    items_hint: "bindings",
    plural: "bindings",
    no_matches: "No matching mouse bindings",
    empty: "No mouse bindings · a: add one",
    categories: &CATEGORY_ORDER,
    all_modes_option: true,
};

const LABEL_WIDTH: usize = 28;
const FIELD_WIDTH: usize = 18;
const FORM_DIALOG_MIN_WIDTH: usize = 50;
const FORM_DIALOG_WIDE_WIDTH: usize = 84;
const FORM_DIALOG_TALL_HEIGHT: usize = 24;
const CLICK_LABELS: [&str; 3] = ["Single", "Double", "Triple"];
const TARGETS: [(MouseTarget, &str); 3] = [
    (MouseTarget::Any, "Anywhere"),
    (MouseTarget::Content, "Pane content"),
    (MouseTarget::Frame, "Pane frame"),
];
const APP_FIRST_LABEL: &str = "Let the app have it first";
const SHIFT_NOTE: &str = "Many terminals take Shift-click for themselves";

fn action_parts(action: &str) -> Option<(String, Vec<String>)> {
    let text = action.trim();
    let name_end = text
        .find(|c: char| c.is_whitespace() || c == ';' || c == '{')
        .unwrap_or(text.len());
    let name = &text[..name_end];
    if name.is_empty() {
        return None;
    }
    let mut arguments = vec![];
    let mut chars = text[name_end..].chars().peekable();
    loop {
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }
        match chars.peek() {
            None | Some(';') | Some('{') => break,
            Some('"') => {
                chars.next();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(escaped);
                            }
                        },
                        '"' => break,
                        other => value.push(other),
                    }
                }
                arguments.push(value);
            },
            Some(_) => {
                let mut value = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == ';' || c == '{' {
                        break;
                    }
                    value.push(c);
                    chars.next();
                }
                if !value.contains('=') {
                    arguments.push(value);
                }
            },
        }
    }
    Some((name.to_owned(), arguments))
}

pub fn is_behaviour(action: &str) -> bool {
    match action_parts(action) {
        Some((name, arguments)) => {
            MouseBehaviour::is_behaviour_name(&name)
                && !(name == "MovePane" && !arguments.is_empty())
        },
        None => false,
    }
}

fn behaviour_of(action: &str) -> Option<MouseBehaviour> {
    if !is_behaviour(action) {
        return None;
    }
    let (name, arguments) = action_parts(action)?;
    MouseBehaviour::from_name_and_arguments(&name, &arguments).ok()
}

pub fn default_app_first(actions: &[String]) -> bool {
    match actions {
        [single] => behaviour_of(single)
            .map(|behaviour| behaviour.default_app_first())
            .unwrap_or(false),
        _ => false,
    }
}

fn passes_or_ignores(actions: &[String]) -> bool {
    match actions {
        [single] => matches!(
            behaviour_of(single),
            Some(MouseBehaviour::PassToApp | MouseBehaviour::Ignore)
        ),
        _ => false,
    }
}

pub fn app_first_argument(actions: &[String], app_first: bool) -> Option<bool> {
    if passes_or_ignores(actions) || app_first == default_app_first(actions) {
        None
    } else {
        Some(app_first)
    }
}

pub fn check_actions(trigger: &MouseTrigger, actions: &[String]) -> Result<(), String> {
    if actions.is_empty() {
        return Err("Choose what the binding does".to_owned());
    }
    if let Some(behaviour) = actions.iter().find(|action| is_behaviour(action)) {
        let name = action_parts(behaviour)
            .map(|(name, _)| name)
            .unwrap_or_default();
        if actions.len() > 1 {
            return Err(format!("{} must be the only item in a mouse binding", name));
        }
        if let Some(parsed) = behaviour_of(behaviour) {
            if parsed.needs_wheel() && !trigger.button.is_wheel() {
                return Err(format!("{} only works with a wheel button", name));
            }
        }
    }
    Ok(())
}

fn mouse_category(trigger: &MouseTrigger, actions: &[String]) -> &'static str {
    if passes_or_ignores(actions) {
        PASSED
    } else if trigger.target == MouseTarget::Frame {
        FRAME
    } else if actions.iter().any(|action| !is_behaviour(action)) {
        OTHER
    } else if trigger.button.is_wheel() {
        SCROLLING
    } else if trigger.click_count > 1 {
        MULTI_CLICKS
    } else {
        CLICKS
    }
}

fn entry_actions(entry: &MousebindingEntry) -> Vec<String> {
    if entry.unbound {
        entry.preset_actions.clone().unwrap_or_default()
    } else {
        entry.actions.clone()
    }
}

const EVERY_MODE: [InputMode; 14] = [
    InputMode::Normal,
    InputMode::Locked,
    InputMode::Resize,
    InputMode::Pane,
    InputMode::Tab,
    InputMode::Scroll,
    InputMode::EnterSearch,
    InputMode::Search,
    InputMode::RenameTab,
    InputMode::RenamePane,
    InputMode::Session,
    InputMode::Move,
    InputMode::Prompt,
    InputMode::Tmux,
];

fn same_binding(first: &MousebindingEntry, second: &MousebindingEntry) -> bool {
    first.actions == second.actions
        && first.app_first == second.app_first
        && first.unbound == second.unbound
}

fn is_user_change(entry: &MousebindingEntry) -> bool {
    matches!(
        entry.source,
        KeybindingSource::User | KeybindingSource::Shared(_)
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct MouseRow {
    pub mode: Option<InputMode>,
    pub trigger: MouseTrigger,
    pub entries: Vec<MousebindingEntry>,
    source: KeybindingSource,
    uniform: bool,
}

impl MouseRow {
    pub fn single(entry: MousebindingEntry) -> Self {
        MouseRow {
            mode: Some(entry.mode),
            trigger: entry.trigger.clone(),
            source: entry.source.clone(),
            entries: vec![entry],
            uniform: true,
        }
    }
    pub fn every_mode(trigger: MouseTrigger, entries: Vec<MousebindingEntry>) -> Self {
        let source = if entries
            .iter()
            .any(|entry| entry.source == KeybindingSource::Layout)
        {
            KeybindingSource::Layout
        } else if let Some(user) = entries.iter().find(|entry| is_user_change(entry)) {
            user.source.clone()
        } else {
            KeybindingSource::Preset
        };
        let covers_every_mode = EVERY_MODE
            .iter()
            .all(|mode| entries.iter().any(|entry| entry.mode == *mode));
        let uniform = covers_every_mode
            && entries
                .first()
                .map(|first| entries.iter().all(|entry| same_binding(first, entry)))
                .unwrap_or(false);
        MouseRow {
            mode: None,
            trigger,
            entries,
            source,
            uniform,
        }
    }
    pub fn varies(&self) -> bool {
        !self.uniform
    }
    pub fn is_unbound(&self) -> bool {
        self.entries.iter().all(|entry| entry.unbound)
    }
    pub fn representative(&self) -> Option<&MousebindingEntry> {
        if self.uniform {
            return self.entries.first();
        }
        let bound: Vec<&MousebindingEntry> =
            self.entries.iter().filter(|entry| !entry.unbound).collect();
        bound
            .iter()
            .max_by_key(|candidate| {
                bound
                    .iter()
                    .filter(|entry| same_binding(candidate, entry))
                    .count()
            })
            .copied()
            .or_else(|| self.entries.first())
    }
    fn user_changes(&self) -> Vec<(InputMode, MouseTrigger)> {
        self.entries
            .iter()
            .filter(|entry| is_user_change(entry))
            .map(|entry| (entry.mode, entry.trigger.clone()))
            .collect()
    }
    fn scope(&self) -> String {
        match self.mode {
            Some(_) => scope_text(self.mode),
            None if self.varies() => "some modes".to_owned(),
            None => scope_text(None),
        }
    }
}

pub fn rows_for(entries: Vec<MousebindingEntry>) -> Vec<MouseRow> {
    let mut by_trigger: BTreeMap<MouseTrigger, Vec<MousebindingEntry>> = BTreeMap::new();
    for entry in &entries {
        by_trigger
            .entry(entry.trigger.clone())
            .or_default()
            .push(entry.clone());
    }
    let mut rows: Vec<MouseRow> = entries.into_iter().map(MouseRow::single).collect();
    rows.extend(
        by_trigger
            .into_iter()
            .map(|(trigger, entries)| MouseRow::every_mode(trigger, entries)),
    );
    rows
}

pub fn same_trigger_binding<'a>(
    rows: &'a [MouseRow],
    mode: Option<InputMode>,
    trigger: &MouseTrigger,
    ignore: Option<&MouseTrigger>,
) -> Option<&'a MouseRow> {
    rows.iter().find(|row| {
        row.mode == mode
            && &row.trigger == trigger
            && !row.is_unbound()
            && Some(&row.trigger) != ignore
    })
}

pub fn trigger_warnings(
    rows: &[MouseRow],
    mode: Option<InputMode>,
    trigger: &MouseTrigger,
    ignore: Option<&MouseTrigger>,
) -> Vec<String> {
    let others: Vec<&MouseRow> = rows
        .iter()
        .filter(|row| row.mode == mode && !row.is_unbound() && Some(&row.trigger) != ignore)
        .collect();
    let mut warnings = vec![];
    if trigger.target == MouseTarget::Any {
        if let Some(specific) = others.iter().find(|row| {
            row.trigger.target != MouseTarget::Any
                && row.trigger.same_trigger_ignoring_target(trigger)
        }) {
            warnings.push(format!(
                "{} also has a binding on the pane {}; there the more specific binding wins",
                trigger.to_kdl(),
                specific.trigger.target.name()
            ));
        }
    }
    if trigger.click_count > 1 {
        let single = trigger.clone().with_click_count(1);
        if !others
            .iter()
            .any(|row| row.trigger.same_trigger_ignoring_target(&single))
        {
            warnings.push(format!(
                "{} is not bound in {}, so the first click passes to the app",
                single.to_kdl(),
                scope_text(mode)
            ));
        }
    }
    warnings
}

pub fn removal_effects(row: &MouseRow) -> Result<Vec<Effect>, String> {
    if row.is_unbound() {
        return Err(format!(
            "{} is already deleted; r brings back the preset's binding",
            row.trigger
        ));
    }
    match row.source {
        KeybindingSource::Layout => Err(format!(
            "{} comes from the layout and cannot be changed here",
            row.trigger
        )),
        KeybindingSource::Preset => mousebind_change_kdl_for(row.mode, &row.trigger, None, None)
            .map(|kdl| vec![Effect::Reconfigure(kdl)]),
        KeybindingSource::User | KeybindingSource::Shared(_) => {
            Ok(vec![Effect::ResetMousebinds(row.user_changes())])
        },
    }
}

pub fn binding_effects(
    mode: Option<InputMode>,
    original: Option<&MouseRow>,
    trigger: &MouseTrigger,
    actions: &[String],
    app_first: Option<bool>,
) -> Result<Vec<Effect>, String> {
    check_actions(trigger, actions)?;
    let mut effects = vec![];
    if let Some(original) = original {
        if &original.trigger != trigger && !original.is_unbound() {
            effects.extend(removal_effects(original)?);
        }
    }
    effects.push(Effect::Reconfigure(mousebind_change_kdl_for(
        mode,
        trigger,
        Some(actions),
        app_first,
    )?));
    Ok(effects)
}

impl ListEntry for MouseRow {
    type Id = (Option<InputMode>, MouseTrigger);
    fn id(&self) -> Self::Id {
        (self.mode, self.trigger.clone())
    }
    fn mode(&self) -> Option<InputMode> {
        self.mode
    }
    fn label(&self) -> String {
        self.trigger.to_string()
    }
    fn bound_actions(&self) -> Option<&[String]> {
        if self.varies() {
            return Some(&[]);
        }
        match self.entries.first() {
            Some(entry) if !entry.unbound => Some(&entry.actions),
            _ => None,
        }
    }
    fn columns(&self) -> Vec<String> {
        let actions = if self.varies() {
            VARIES_TEXT.to_owned()
        } else {
            match self.bound_actions() {
                Some(actions) => actions_summary(actions),
                None => "(unbound)".to_owned(),
            }
        };
        vec![self.label(), actions]
    }
    fn dim_actions(&self) -> bool {
        self.varies()
    }
    fn search_actions(&self) -> String {
        if !self.varies() {
            return self.columns()[1].clone();
        }
        let mut summaries: Vec<String> = vec![];
        for entry in self.entries.iter().filter(|entry| !entry.unbound) {
            let summary = actions_summary(&entry.actions);
            if !summaries.contains(&summary) {
                summaries.push(summary);
            }
        }
        std::iter::once(VARIES_TEXT.to_owned())
            .chain(summaries)
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn category(&self) -> &'static str {
        if self.varies() {
            return VARIES;
        }
        self.entries
            .first()
            .map(|entry| mouse_category(&self.trigger, &entry_actions(entry)))
            .unwrap_or(OTHER)
    }
    fn source(&self) -> &KeybindingSource {
        &self.source
    }
    fn has_preset(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.preset_actions.is_some())
    }
    fn is_unsaved(&self) -> bool {
        self.entries.iter().any(|entry| entry.unsaved)
    }
    fn removal_effects(&self) -> Result<Vec<Effect>, String> {
        removal_effects(self)
    }
    fn reset_effect(rows: &[Self]) -> Effect {
        Effect::ResetMousebinds(rows.iter().flat_map(|row| row.user_changes()).collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Button,
    Clicks,
    Ctrl,
    Alt,
    Shift,
    Target,
    AppFirst,
}

fn buttons() -> Vec<MouseButton> {
    vec![
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::ScrollUp,
        MouseButton::ScrollDown,
        MouseButton::ScrollLeft,
        MouseButton::ScrollRight,
    ]
}

struct MouseForm {
    mode: Option<InputMode>,
    original: Option<MouseRow>,
    button: MouseButton,
    clicks: u8,
    ctrl: bool,
    alt: bool,
    shift: bool,
    target: MouseTarget,
    app_first: bool,
    app_first_touched: bool,
    fields: FocusGroup<Field>,
    picker: ActionPicker,
    on_actions: bool,
    error: Option<String>,
}

impl MouseForm {
    fn new(
        mode: Option<InputMode>,
        original: Option<MouseRow>,
        trigger: MouseTrigger,
        actions: Vec<String>,
        app_first: Option<bool>,
    ) -> Self {
        let default = default_app_first(&actions);
        let app_first = app_first.unwrap_or(default);
        let mut picker = ActionPicker::new("Actions", actions, None, false).with_mouse_behaviours();
        picker.show_list();
        picker.set_label_width(LABEL_WIDTH);
        let mut form = MouseForm {
            mode,
            original,
            button: trigger.button,
            clicks: trigger.click_count.clamp(1, 3),
            ctrl: trigger.modifiers.contains(&KeyModifier::Ctrl),
            alt: trigger.modifiers.contains(&KeyModifier::Alt),
            shift: trigger.modifiers.contains(&KeyModifier::Shift),
            target: trigger.target,
            app_first,
            app_first_touched: app_first != default,
            fields: FocusGroup::new().wrap(false),
            picker,
            on_actions: false,
            error: None,
        };
        form.rebuild_fields();
        form.fields.focus_first();
        form
    }
    fn trigger(&self) -> MouseTrigger {
        let mut modifiers = std::collections::BTreeSet::new();
        for (on, modifier) in [
            (self.ctrl, KeyModifier::Ctrl),
            (self.alt, KeyModifier::Alt),
            (self.shift, KeyModifier::Shift),
        ] {
            if on {
                modifiers.insert(modifier);
            }
        }
        let clicks = if self.button.is_wheel() {
            1
        } else {
            self.clicks
        };
        MouseTrigger::new(self.button)
            .with_modifiers(modifiers)
            .with_click_count(clicks)
            .with_target(self.target)
    }
    fn shows_app_first(&self) -> bool {
        !passes_or_ignores(&self.picker.actions)
    }
    fn field_keys(&self) -> Vec<Field> {
        let mut keys = vec![Field::Button];
        if !self.button.is_wheel() {
            keys.push(Field::Clicks);
        }
        keys.extend([Field::Ctrl, Field::Alt, Field::Shift, Field::Target]);
        if self.shows_app_first() {
            keys.push(Field::AppFirst);
        }
        keys
    }
    fn rebuild_fields(&mut self) {
        let focused = self.fields.focused_key().copied();
        let mut fields = FocusGroup::new().wrap(false);
        for key in self.field_keys() {
            match key {
                Field::Button => {
                    let options: Vec<&str> = buttons().iter().map(|button| button.name()).collect();
                    let selected = buttons()
                        .iter()
                        .position(|button| *button == self.button)
                        .unwrap_or(0);
                    fields.add(
                        key,
                        Dropdown::new("Button", options)
                            .selected(selected)
                            .label_width(LABEL_WIDTH),
                    );
                },
                Field::Clicks => fields.add(
                    key,
                    Dropdown::new("Clicks", CLICK_LABELS.to_vec())
                        .selected(self.clicks.saturating_sub(1) as usize)
                        .label_width(LABEL_WIDTH),
                ),
                Field::Ctrl => {
                    fields.add(key, Toggle::new("Ctrl", self.ctrl).label_width(LABEL_WIDTH))
                },
                Field::Alt => {
                    fields.add(key, Toggle::new("Alt", self.alt).label_width(LABEL_WIDTH))
                },
                Field::Shift => fields.add(
                    key,
                    Toggle::new("Shift", self.shift).label_width(LABEL_WIDTH),
                ),
                Field::Target => {
                    let selected = TARGETS
                        .iter()
                        .position(|(target, _)| *target == self.target)
                        .unwrap_or(0);
                    fields.add(
                        key,
                        Dropdown::new(
                            "Where",
                            TARGETS.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
                        )
                        .selected(selected)
                        .label_width(LABEL_WIDTH),
                    );
                },
                Field::AppFirst => fields.add(
                    key,
                    Toggle::new(APP_FIRST_LABEL, self.app_first).label_width(LABEL_WIDTH),
                ),
            }
        }
        if !self.on_actions {
            if let Some(focused) = focused {
                if !fields.focus(&focused) {
                    fields.focus_first();
                }
            }
        }
        self.fields = fields;
    }
    fn apply_change(&mut self, field: Field, value: UiValue) {
        match (field, value) {
            (Field::Button, UiValue::Choice { index, .. }) => {
                if let Some(button) = buttons().get(index) {
                    self.button = *button;
                }
            },
            (Field::Clicks, UiValue::Choice { index, .. }) => {
                self.clicks = (index as u8 + 1).clamp(1, 3);
            },
            (Field::Ctrl, UiValue::Bool(on)) => self.ctrl = on,
            (Field::Alt, UiValue::Bool(on)) => self.alt = on,
            (Field::Shift, UiValue::Bool(on)) => self.shift = on,
            (Field::Target, UiValue::Choice { index, .. }) => {
                if let Some((target, _)) = TARGETS.get(index) {
                    self.target = *target;
                }
            },
            (Field::AppFirst, UiValue::Bool(on)) => {
                self.app_first = on;
                self.app_first_touched = true;
            },
            _ => {},
        }
        self.error = None;
        if self.field_keys() != self.fields.keys() {
            self.rebuild_fields();
        }
    }
    fn sync_after_picker(&mut self) {
        if !self.app_first_touched {
            self.app_first = default_app_first(&self.picker.actions);
        }
        let toggle_differs = self
            .fields
            .toggle(&Field::AppFirst)
            .map(|toggle| toggle.is_on() != self.app_first)
            .unwrap_or(false);
        if toggle_differs || self.field_keys() != self.fields.keys() {
            self.rebuild_fields();
        }
    }
    fn focus_last_field(&mut self) {
        self.on_actions = false;
        if let Some(last) = self.fields.keys().last().copied() {
            self.fields.focus(&last);
        }
    }
    fn focus_actions(&mut self) {
        self.on_actions = true;
        self.fields.blur();
        self.picker.select_row(0);
    }
    fn field_rows(&self) -> usize {
        self.fields.len() + 1
    }
}

#[derive(Debug, Clone, PartialEq)]
enum DialogPurpose {
    Replace(Vec<String>),
}

pub struct MousebindingsScreen {
    list: BindingsList<MouseRow>,
    form: Option<MouseForm>,
    dialog: ConfirmDialog,
    dialog_purpose: Option<DialogPurpose>,
    effects: Vec<Effect>,
    notice: Option<String>,
    preset_name: String,
}

impl Default for MousebindingsScreen {
    fn default() -> Self {
        MousebindingsScreen {
            list: BindingsList::new(TEXTS),
            form: None,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            effects: vec![],
            notice: None,
            preset_name: String::new(),
        }
    }
}

impl MousebindingsScreen {
    pub fn set_entries(&mut self, entries: Vec<MousebindingEntry>) {
        self.list.set_entries(rows_for(entries));
    }
    #[cfg(test)]
    pub fn visible(&self) -> Vec<MouseRow> {
        self.list.visible()
    }
    pub fn open_add(&mut self) {
        let mode = self.list.selected_mode();
        self.form = Some(MouseForm::new(
            mode,
            None,
            MouseTrigger::new(MouseButton::Left),
            vec![],
            None,
        ));
    }
    pub fn open_edit(&mut self) {
        let Some(row) = self.list.selected_entry() else {
            return;
        };
        if row.source == KeybindingSource::Layout {
            self.notice = Some(format!(
                "{} comes from the layout and cannot be changed here",
                row.trigger
            ));
            return;
        }
        let (actions, app_first) = match row.representative() {
            Some(entry) if !entry.unbound => (entry_actions(entry), Some(entry.app_first)),
            Some(entry) => (entry_actions(entry), None),
            None => (vec![], None),
        };
        self.form = Some(MouseForm::new(
            row.mode,
            Some(row.clone()),
            row.trigger.clone(),
            actions,
            app_first,
        ));
    }
    fn close_form(&mut self) {
        self.form = None;
    }
    fn open_dialog(&mut self, title: &str, message: String, purpose: DialogPurpose) {
        self.dialog = ConfirmDialog::new(title, message)
            .buttons(vec!["Replace", "Cancel"])
            .width(64)
            .opened();
        self.dialog_purpose = Some(purpose);
    }
    fn dialog_response(&mut self, response: UiResponse) {
        let confirmed = matches!(
            response,
            UiResponse::Submitted(UiValue::Choice { index: 0, .. })
        );
        let purpose = if self.dialog.is_open() {
            None
        } else {
            self.dialog_purpose.take()
        };
        match purpose {
            Some(DialogPurpose::Replace(actions)) => {
                if confirmed {
                    self.apply_form(actions);
                } else {
                    self.notice = Some("Change the trigger, or Esc to stop".to_owned());
                }
            },
            None => {},
        }
    }
    fn finish_form(&mut self, actions: Vec<String>) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let trigger = form.trigger();
        let mode = form.mode;
        if let Err(error) = check_actions(&trigger, &actions) {
            self.notice = Some(error.clone());
            if let Some(form) = self.form.as_mut() {
                form.error = Some(error);
            }
            return;
        }
        let ignore = form
            .original
            .as_ref()
            .map(|original| original.trigger.clone());
        if let Some(existing) =
            same_trigger_binding(self.list.entries(), mode, &trigger, ignore.as_ref())
        {
            let message = format!(
                "{} is already bound in {} to {}. Replace it?",
                trigger,
                existing.scope(),
                truncate(&existing.columns()[1], 40)
            );
            self.open_dialog(
                "Trigger already bound",
                message,
                DialogPurpose::Replace(actions),
            );
            return;
        }
        self.apply_form(actions);
    }
    fn apply_form(&mut self, actions: Vec<String>) {
        let Some(mut form) = self.form.take() else {
            return;
        };
        let trigger = form.trigger();
        let app_first = app_first_argument(&actions, form.app_first);
        match binding_effects(
            form.mode,
            form.original.as_ref(),
            &trigger,
            &actions,
            app_first,
        ) {
            Ok(effects) => {
                self.effects.extend(effects);
                self.notice = Some(format!("Bound {} in {}", trigger, scope_text(form.mode)));
            },
            Err(error) => {
                self.notice = Some(error.clone());
                form.error = Some(error);
                self.form = Some(form);
            },
        }
    }
    fn handle_picker_response(&mut self, response: PickerResponse) {
        match response {
            PickerResponse::Done(actions) => self.finish_form(actions),
            PickerResponse::Cancelled => self.close_form(),
            PickerResponse::Pending => {},
        }
        if let Some(form) = self.form.as_mut() {
            form.picker.take_returned_to_list();
            form.sync_after_picker();
        }
    }
    fn handle_form_key(&mut self, key: &KeyWithModifier) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let down = is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab);
        let up = is_plain(key, BareKey::Up) || is_shift_tab(key);
        if !form.on_actions {
            let dropdown_open = form.fields.has_open_overlay();
            if !dropdown_open {
                if is_plain(key, BareKey::Esc) {
                    self.close_form();
                    return;
                }
                if down {
                    if !form.fields.focus_next() {
                        form.focus_actions();
                    }
                    return;
                }
                if up {
                    form.fields.focus_prev();
                    return;
                }
            }
            if let FocusEvent::Element {
                key: field,
                response: UiResponse::Changed(value),
            } = form.fields.handle_key(key)
            {
                form.apply_change(field, value);
            }
            return;
        }
        if form.picker.is_listing() && up && form.picker.selected_row() == 0 {
            form.focus_last_field();
            return;
        }
        let response = form.picker.handle_key(key);
        self.handle_picker_response(response);
    }
    fn handle_form_mouse(&mut self, mouse: Mouse) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.picker.is_listing() {
            match form.fields.handle_mouse(mouse) {
                FocusEvent::Element {
                    key: field,
                    response: UiResponse::Changed(value),
                } => {
                    form.on_actions = false;
                    form.apply_change(field, value);
                    return;
                },
                FocusEvent::FocusChanged(_) => {
                    form.on_actions = false;
                    return;
                },
                FocusEvent::Element { .. } if matches!(mouse, Mouse::LeftClick(..)) => {
                    form.on_actions = false;
                    return;
                },
                _ => {},
            }
            if form.fields.has_open_overlay() {
                return;
            }
        }
        if matches!(mouse, Mouse::LeftClick(..)) && !form.on_actions {
            form.on_actions = true;
            form.fields.blur();
        }
        let response = form.picker.handle_mouse(mouse);
        self.handle_picker_response(response);
    }
    fn handle_list_response(&mut self, response: ListResponse) -> PageResponse {
        match response {
            ListResponse::Page(response) => response,
            ListResponse::Add => {
                self.open_add();
                PageResponse::Handled
            },
            ListResponse::Edit => {
                self.open_edit();
                PageResponse::Handled
            },
        }
    }
    fn render_form(&mut self, rows: usize, cols: usize) {
        let warnings = match self.form.as_ref() {
            Some(form) => trigger_warnings(
                self.list.entries(),
                form.mode,
                &form.trigger(),
                form.original.as_ref().map(|original| &original.trigger),
            ),
            None => return,
        };
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let title = if form.original.is_some() {
            "Change Mouse Binding"
        } else {
            "Add Mouse Binding"
        };
        form.picker.set_label_width(LABEL_WIDTH);
        let content_column = form.picker.content_column();
        let max_width = cols.saturating_sub(4).max(20);
        let max_height = rows.saturating_sub(2).max(6);
        let natural_button_width = form.picker.natural_button_width().max(FIELD_WIDTH);
        let extra_rows = warnings.len() + usize::from(form.error.is_some());
        let (dialog_width, dialog_height) = match form.picker.wanted_height() {
            Some(picker_rows) if form.picker.is_listing() => {
                let width = (content_column + natural_button_width + DELETE_WIDTH + 4)
                    .max(LABEL_WIDTH + FIELD_WIDTH + 4)
                    .max(FORM_DIALOG_MIN_WIDTH)
                    .min(max_width);
                let height = 5 + form.field_rows() + extra_rows + picker_rows;
                (width, height.min(max_height).max(6))
            },
            Some(picker_rows) => {
                let width = (content_column
                    + natural_button_width.max(form.picker.action_name_width())
                    + 4)
                .max(FORM_DIALOG_MIN_WIDTH)
                .min(max_width);
                (width, (3 + picker_rows).min(max_height).max(6))
            },
            None => (
                FORM_DIALOG_WIDE_WIDTH.min(max_width),
                max_height.min(FORM_DIALOG_TALL_HEIGHT),
            ),
        };
        let dialog_x = cols.saturating_sub(dialog_width) / 2;
        let dialog_y = rows.saturating_sub(dialog_height) / 2;
        render_frame(title, dialog_x, dialog_y, dialog_width, dialog_height);
        let x = dialog_x + 2;
        let y = dialog_y + 2;
        let width = dialog_width.saturating_sub(4);
        let bottom = dialog_y + dialog_height.saturating_sub(2);
        let button_width = natural_button_width
            .min(width.saturating_sub(content_column))
            .max(5);
        form.picker.set_button_width(button_width);
        if !form.picker.is_listing() {
            form.fields.clear_areas();
            form.picker.set_active(true);
            form.picker
                .render(x, y, width, (bottom + 1).saturating_sub(y));
            form.picker.render_overlays(rows, cols);
            note_overlay(Rect::new(dialog_x, dialog_y, dialog_width, dialog_height));
            return;
        }
        let field_width = (LABEL_WIDTH + FIELD_WIDTH).min(width);
        let mut row = y;
        for key in form.fields.keys() {
            let focused = !form.on_actions && form.fields.is_focused(&key);
            match form.fields.get_mut(&key) {
                Some(Element::Dropdown(dropdown)) => {
                    dropdown.set_focused(focused);
                    dropdown.render(x, row, field_width);
                },
                Some(Element::Toggle(toggle)) => {
                    toggle.set_field_width(FIELD_WIDTH.min(width.saturating_sub(LABEL_WIDTH)));
                    toggle.render(x, row);
                },
                _ => {},
            }
            row += 1;
            if key == Field::Shift {
                print_dim(
                    SHIFT_NOTE,
                    x + LABEL_WIDTH,
                    row,
                    width.saturating_sub(LABEL_WIDTH),
                );
                row += 1;
            }
        }
        for warning in &warnings {
            print_text_with_coordinates(
                Text::from(truncate(&format!("⚠ {}", warning), width)).error_color_all(),
                x,
                row,
                None,
                None,
            );
            row += 1;
        }
        if let Some(error) = &form.error {
            print_text_with_coordinates(
                Text::from(truncate(error, width)).error_color_all(),
                x,
                row,
                None,
                None,
            );
            row += 1;
        }
        row += 1;
        form.picker.set_active(form.on_actions);
        form.picker
            .render(x, row, width, (bottom + 1).saturating_sub(row));
        form.picker.render_overlays(rows, cols);
        form.fields.render_overlays(rows, cols);
        note_group(&form.fields);
        note_overlay(Rect::new(dialog_x, dialog_y, dialog_width, dialog_height));
    }
}

impl Page for MousebindingsScreen {
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        let active = &snapshot.keybinds.active;
        self.preset_name = if active.display_name.is_empty() {
            active.name.clone()
        } else {
            active.display_name.clone()
        };
        self.set_entries(snapshot.mousebindings.clone());
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        if self.dialog.is_open() {
            let response = self.dialog.handle_key(key);
            self.dialog_response(response);
            return PageResponse::Handled;
        }
        if self.form.is_some() {
            self.handle_form_key(key);
            return PageResponse::Handled;
        }
        let response = self.list.handle_key(key);
        self.handle_list_response(response)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> PageResponse {
        if self.dialog.is_open() {
            let (response, dialog_changed) =
                changed_by(&mut self.dialog, |dialog| dialog.handle_mouse(mouse));
            self.dialog_response(response);
            return if dialog_changed || !matches!(mouse, Mouse::Hover(..)) {
                PageResponse::Handled
            } else {
                PageResponse::NotHandled
            };
        }
        if self.form.is_some() {
            self.handle_form_mouse(mouse);
            return PageResponse::Handled;
        }
        let response = self.list.handle_mouse(mouse);
        self.handle_list_response(response)
    }
    fn handle_timer(&mut self) -> bool {
        let form_changed = self
            .form
            .as_mut()
            .map(|form| form.picker.handle_timer() | form.fields.handle_timer())
            .unwrap_or(false);
        form_changed || self.list.handle_timer()
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        print_dim(
            &format!("From preset: {} (change it under Keys)", self.preset_name),
            x,
            y,
            width,
        );
        self.list.render(x, y + 2, width, height.saturating_sub(2));
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.list.render_overlays(rows, cols);
        self.render_form(rows, cols);
        self.dialog.render_centered(rows, cols);
    }
    fn captures_keys(&self) -> bool {
        self.form.is_some() || self.dialog.is_open() || self.list.captures_keys()
    }
    fn hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.dialog.is_open() {
            return vec![("<←→>", "choose"), ("<Enter>", "confirm")];
        }
        if let Some(form) = &self.form {
            if !form.on_actions && form.picker.is_listing() {
                return vec![
                    ("<↓↑>", "field"),
                    ("<Space>", "change"),
                    ("<↓>", "past the last field: actions"),
                    ("<Esc>", "cancel"),
                ];
            }
            return form.picker.hints();
        }
        self.list.hints()
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        let mut effects = std::mem::take(&mut self.effects);
        effects.extend(self.list.take_effects());
        effects
    }
    fn take_notice(&mut self) -> Option<String> {
        self.notice.take().or_else(|| self.list.take_notice())
    }
    fn leave(&mut self) {
        self.form = None;
        self.list.reset_mode();
        if self.dialog.is_open() {
            self.dialog.close();
            self.dialog_purpose = None;
        }
    }
    fn set_focused(&mut self, focused: bool) {
        self.list.set_focused(focused);
    }
    fn prompt_result(&mut self, request_id: u64, result: &PromptResult) -> bool {
        self.list.answer_delete(request_id, result)
    }
}

#[cfg(test)]
mod action_parts_tests {
    use super::*;
    use kdl::{KdlDocument, KdlValue};

    fn kdl_parts(action: &str) -> Option<(String, Vec<String>)> {
        let document: KdlDocument = action.parse().ok()?;
        let node = document.nodes().first()?;
        let arguments = node
            .entries()
            .iter()
            .filter(|entry| entry.name().is_none())
            .map(|entry| match entry.value() {
                KdlValue::String(text) | KdlValue::RawString(text) => text.clone(),
                other => other.to_string(),
            })
            .collect();
        Some((node.name().value().to_owned(), arguments))
    }

    #[test]
    fn the_quick_parser_reads_every_default_action_like_the_kdl_parser() {
        let config = zellij_utils::input::config::Config::from_default_assets().unwrap();
        let mut texts: Vec<String> =
            zellij_utils::input::config_blocks::keybinding_entries(&config, &config, None)
                .into_iter()
                .flat_map(|entry| entry.actions)
                .collect();
        texts.extend(
            zellij_utils::input::config_blocks::mousebinding_entries(&config, &config)
                .into_iter()
                .flat_map(|entry| entry.actions),
        );
        texts.extend(
            [
                "MovePane \"left\"",
                "Scroll 7",
                "ScrollToPrompt \"previous\"",
                "Write 27 91 65",
                "WriteChars \"a \\\"quoted\\\" word\"",
                "NewPane \"down\" name=\"x\"",
                "Run \"ls\" \"-la\" { direction \"Down\"; }",
            ]
            .into_iter()
            .map(|text| text.to_owned()),
        );
        assert!(texts.len() > 100);
        for text in texts {
            assert_eq!(action_parts(&text), kdl_parts(&text), "{}", text);
        }
    }

    #[test]
    fn behaviours_are_told_apart_from_normal_actions() {
        assert!(is_behaviour("MovePane"));
        assert!(!is_behaviour("MovePane \"left\""));
        assert!(is_behaviour("Scroll 3"));
        assert!(!is_behaviour("NewTab"));
        assert_eq!(
            behaviour_of("ResizeScroll \"increase\""),
            Some(MouseBehaviour::ResizeScroll(
                zellij_utils::input::mousebinds::ResizeScrollDirection::Increase
            ))
        );
        assert_eq!(action_parts(""), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bindings_list::ListFocus;
    use std::collections::BTreeSet;

    fn trigger(text: &str, target: MouseTarget) -> MouseTrigger {
        MouseTrigger::parse(text, target).unwrap()
    }

    fn entry(
        mode: InputMode,
        text: &str,
        target: MouseTarget,
        actions: &[&str],
        source: KeybindingSource,
    ) -> MousebindingEntry {
        let actions: Vec<String> = actions.iter().map(|a| a.to_string()).collect();
        MousebindingEntry {
            mode,
            trigger: trigger(text, target),
            app_first: default_app_first(&actions),
            preset_actions: Some(actions.clone()),
            actions,
            source,
            unbound: false,
            unsaved: false,
        }
    }

    fn everywhere(
        text: &str,
        target: MouseTarget,
        actions: &[&str],
        source: KeybindingSource,
    ) -> Vec<MousebindingEntry> {
        EVERY_MODE
            .iter()
            .map(|mode| entry(*mode, text, target, actions, source.clone()))
            .collect()
    }

    fn singles(entries: Vec<MousebindingEntry>) -> Vec<MouseRow> {
        entries.into_iter().map(MouseRow::single).collect()
    }

    fn type_text(screen: &mut MousebindingsScreen, text: &str) {
        for character in text.chars() {
            screen.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
    }

    fn press(screen: &mut MousebindingsScreen, character: char) {
        screen.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
    }

    fn select(screen: &mut MousebindingsScreen, button: MouseButton) {
        screen.list.focus = ListFocus::List;
        screen.list.selected = screen
            .visible()
            .iter()
            .position(|row| row.trigger.button == button)
            .unwrap();
    }

    fn reconfigured(effects: &[Effect]) -> Vec<String> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Reconfigure(kdl) => Some(kdl.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_same_trigger_in_the_same_mode_is_a_conflict() {
        let rows = singles(vec![
            entry(
                InputMode::Normal,
                "Left",
                MouseTarget::Frame,
                &["MovePane"],
                KeybindingSource::Preset,
            ),
            entry(
                InputMode::Locked,
                "Ctrl Left",
                MouseTarget::Any,
                &["Click"],
                KeybindingSource::User,
            ),
        ]);
        let normal = Some(InputMode::Normal);
        let left_frame = trigger("Left", MouseTarget::Frame);
        assert!(same_trigger_binding(&rows, normal, &left_frame, None).is_some());
        assert!(same_trigger_binding(&rows, Some(InputMode::Locked), &left_frame, None).is_none());
        assert!(same_trigger_binding(&rows, None, &left_frame, None).is_none());
        assert!(
            same_trigger_binding(&rows, normal, &trigger("Left", MouseTarget::Any), None).is_none()
        );
        assert!(same_trigger_binding(
            &rows,
            normal,
            &trigger("Double Left", MouseTarget::Frame),
            None
        )
        .is_none());
        assert!(same_trigger_binding(&rows, normal, &left_frame, Some(&left_frame)).is_none());
        let mut unbound = rows[0].entries[0].clone();
        unbound.unbound = true;
        assert!(same_trigger_binding(&singles(vec![unbound]), normal, &left_frame, None).is_none());
    }

    #[test]
    fn a_conflicting_trigger_asks_before_replacing() {
        let mut screen = MousebindingsScreen::default();
        screen.set_entries(everywhere(
            "Middle",
            MouseTarget::Any,
            &["PassToApp"],
            KeybindingSource::Preset,
        ));
        screen.open_add();
        screen.form.as_mut().unwrap().button = MouseButton::Middle;
        screen.finish_form(vec!["ToggleFullscreen".to_owned()]);
        assert!(screen.dialog.is_open());
        assert!(screen.take_effects().is_empty());
        screen.dialog_purpose = None;
        screen.dialog.close();
        screen.apply_form(vec!["ToggleFullscreen".to_owned()]);
        let effects = screen.take_effects();
        assert_eq!(effects.len(), 1);
        assert!(screen.form.is_none());
    }

    #[test]
    fn an_anywhere_binding_is_warned_about_a_more_specific_one() {
        let rows = singles(vec![entry(
            InputMode::Normal,
            "Left",
            MouseTarget::Frame,
            &["MovePane"],
            KeybindingSource::Preset,
        )]);
        let normal = Some(InputMode::Normal);
        let warnings = trigger_warnings(&rows, normal, &trigger("Left", MouseTarget::Any), None);
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("more specific binding wins"),
            "{}",
            warnings[0]
        );
        assert!(
            trigger_warnings(&rows, normal, &trigger("Left", MouseTarget::Content), None)
                .is_empty()
        );
        assert!(trigger_warnings(
            &rows,
            Some(InputMode::Locked),
            &trigger("Left", MouseTarget::Any),
            None
        )
        .is_empty());
        let all = rows_for(everywhere(
            "Left",
            MouseTarget::Frame,
            &["MovePane"],
            KeybindingSource::Preset,
        ));
        assert_eq!(
            trigger_warnings(&all, None, &trigger("Left", MouseTarget::Any), None).len(),
            1
        );
    }

    #[test]
    fn a_double_click_without_a_single_click_binding_is_warned_about() {
        let rows = singles(vec![entry(
            InputMode::Normal,
            "Left",
            MouseTarget::Any,
            &["Select"],
            KeybindingSource::Preset,
        )]);
        let normal = Some(InputMode::Normal);
        let warnings = trigger_warnings(
            &rows,
            normal,
            &trigger("Ctrl Double Left", MouseTarget::Any),
            None,
        );
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("first click passes to the app"),
            "{}",
            warnings[0]
        );
        assert!(trigger_warnings(
            &rows,
            normal,
            &trigger("Double Left", MouseTarget::Any),
            None
        )
        .is_empty());
        assert!(trigger_warnings(
            &rows,
            normal,
            &trigger("Triple Right", MouseTarget::Frame),
            None
        )
        .iter()
        .any(|warning| warning.contains("first click")));
        let all = rows_for(everywhere(
            "Left",
            MouseTarget::Any,
            &["Select"],
            KeybindingSource::Preset,
        ));
        assert!(
            trigger_warnings(&all, None, &trigger("Double Left", MouseTarget::Any), None)
                .is_empty()
        );
        let warnings =
            trigger_warnings(&all, None, &trigger("Double Right", MouseTarget::Any), None);
        assert!(warnings[0].contains("in all modes"), "{}", warnings[0]);
    }

    #[test]
    fn deleting_depends_on_where_the_binding_comes_from() {
        let preset = MouseRow::single(entry(
            InputMode::Normal,
            "Right",
            MouseTarget::Any,
            &["ContextMenu"],
            KeybindingSource::Preset,
        ));
        assert_eq!(
            removal_effects(&preset).unwrap(),
            vec![Effect::Reconfigure(
                mousebind_change_kdl_for(Some(InputMode::Normal), &preset.trigger, None, None)
                    .unwrap()
            )]
        );
        match &removal_effects(&preset).unwrap()[0] {
            Effect::Reconfigure(kdl) => {
                assert!(kdl.contains("normal {"), "{}", kdl);
                assert!(kdl.contains("unbind \"Right\""), "{}", kdl)
            },
            other => panic!("unexpected {:?}", other),
        }
        let user = MouseRow::single(entry(
            InputMode::Normal,
            "Alt Left",
            MouseTarget::Frame,
            &["Ignore"],
            KeybindingSource::User,
        ));
        assert_eq!(
            removal_effects(&user).unwrap(),
            vec![Effect::ResetMousebinds(vec![(
                InputMode::Normal,
                user.trigger.clone()
            )])]
        );
        let layout = MouseRow::single(entry(
            InputMode::Normal,
            "Left",
            MouseTarget::Any,
            &["Click"],
            KeybindingSource::Layout,
        ));
        assert!(removal_effects(&layout).is_err());
        let mut unbound = user.entries[0].clone();
        unbound.unbound = true;
        assert!(removal_effects(&MouseRow::single(unbound)).is_err());
    }

    #[test]
    fn deleting_in_all_modes_unbinds_the_preset_or_resets_own_changes() {
        let right = trigger("Right", MouseTarget::Any);
        let preset = MouseRow::every_mode(
            right.clone(),
            everywhere(
                "Right",
                MouseTarget::Any,
                &["ContextMenu"],
                KeybindingSource::Preset,
            ),
        );
        let effects = removal_effects(&preset).unwrap();
        assert_eq!(
            effects,
            vec![Effect::Reconfigure(
                mousebind_change_kdl_for(None, &right, None, None).unwrap()
            )]
        );
        assert!(reconfigured(&effects)[0].contains("shared {"));
        assert!(reconfigured(&effects)[0].contains("unbind \"Right\""));

        let mut mixed = everywhere(
            "Right",
            MouseTarget::Any,
            &["ContextMenu"],
            KeybindingSource::Preset,
        );
        mixed[0].source = KeybindingSource::User;
        mixed[0].actions = vec!["NewTab".to_owned()];
        mixed[3].source = KeybindingSource::User;
        let row = MouseRow::every_mode(right.clone(), mixed);
        assert_eq!(
            removal_effects(&row).unwrap(),
            vec![Effect::ResetMousebinds(vec![
                (EVERY_MODE[0], right.clone()),
                (EVERY_MODE[3], right.clone())
            ])]
        );

        let mut with_layout = everywhere(
            "Right",
            MouseTarget::Any,
            &["ContextMenu"],
            KeybindingSource::Preset,
        );
        with_layout[5].source = KeybindingSource::Layout;
        let error = removal_effects(&MouseRow::every_mode(right, with_layout)).unwrap_err();
        assert!(error.contains("comes from the layout"), "{}", error);
    }

    #[test]
    fn resetting_depends_on_where_the_binding_comes_from() {
        let mut screen = MousebindingsScreen::default();
        screen.set_entries(vec![
            entry(
                InputMode::Normal,
                "Left",
                MouseTarget::Any,
                &["Click"],
                KeybindingSource::Preset,
            ),
            entry(
                InputMode::Normal,
                "Middle",
                MouseTarget::Any,
                &["Click"],
                KeybindingSource::User,
            ),
            entry(
                InputMode::Normal,
                "Right",
                MouseTarget::Any,
                &["Click"],
                KeybindingSource::Layout,
            ),
        ]);
        screen.list.set_selected_mode(Some(InputMode::Normal));
        select(&mut screen, MouseButton::Left);
        press(&mut screen, 'r');
        assert!(screen.take_effects().is_empty());
        assert!(screen
            .take_notice()
            .unwrap()
            .contains("already uses the preset's binding"));
        select(&mut screen, MouseButton::Middle);
        press(&mut screen, 'r');
        assert_eq!(
            screen.take_effects(),
            vec![Effect::ResetMousebinds(vec![(
                InputMode::Normal,
                trigger("Middle", MouseTarget::Any)
            )])]
        );
        screen.take_notice();
        select(&mut screen, MouseButton::Right);
        press(&mut screen, 'r');
        assert!(screen.take_effects().is_empty());
        assert!(screen
            .take_notice()
            .unwrap()
            .contains("comes from the layout"));
    }

    #[test]
    fn resetting_in_all_modes_resets_every_mode_with_an_own_change() {
        let mut entries = everywhere(
            "Left",
            MouseTarget::Any,
            &["Click"],
            KeybindingSource::Preset,
        );
        let mut middle = everywhere(
            "Middle",
            MouseTarget::Any,
            &["NewTab"],
            KeybindingSource::Preset,
        );
        middle[1].source = KeybindingSource::User;
        middle[2].source = KeybindingSource::User;
        entries.extend(middle);
        let mut screen = MousebindingsScreen::default();
        screen.set_entries(entries);
        select(&mut screen, MouseButton::Left);
        press(&mut screen, 'r');
        assert!(screen.take_effects().is_empty());
        assert!(screen
            .take_notice()
            .unwrap()
            .contains("already uses the preset's binding"));
        select(&mut screen, MouseButton::Middle);
        press(&mut screen, 'r');
        let middle_trigger = trigger("Middle", MouseTarget::Any);
        assert_eq!(
            screen.take_effects(),
            vec![Effect::ResetMousebinds(vec![
                (EVERY_MODE[1], middle_trigger.clone()),
                (EVERY_MODE[2], middle_trigger.clone())
            ])]
        );
        screen.take_notice();
        press(&mut screen, 'A');
        assert_eq!(screen.list.marked.len(), 2);
        press(&mut screen, 'r');
        assert_eq!(
            screen.take_effects(),
            vec![Effect::ResetMousebinds(vec![
                (EVERY_MODE[1], middle_trigger.clone()),
                (EVERY_MODE[2], middle_trigger)
            ])]
        );
    }

    #[test]
    fn changing_the_trigger_removes_the_old_binding_first() {
        let original = MouseRow::single(entry(
            InputMode::Normal,
            "Left",
            MouseTarget::Frame,
            &["MovePane"],
            KeybindingSource::Preset,
        ));
        let normal = Some(InputMode::Normal);
        let effects = binding_effects(
            normal,
            Some(&original),
            &trigger("Alt Left", MouseTarget::Frame),
            &["MovePane".to_owned()],
            None,
        )
        .unwrap();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0], removal_effects(&original).unwrap()[0]);
        match &effects[1] {
            Effect::Reconfigure(kdl) => {
                assert!(kdl.contains("bind \"Alt Left\""), "{}", kdl);
                assert!(kdl.contains("on=\"frame\""), "{}", kdl);
            },
            other => panic!("unexpected {:?}", other),
        }
        let user = MouseRow::single(entry(
            InputMode::Normal,
            "Middle",
            MouseTarget::Any,
            &["Click"],
            KeybindingSource::User,
        ));
        let effects = binding_effects(
            normal,
            Some(&user),
            &trigger("Right", MouseTarget::Any),
            &["Click".to_owned()],
            None,
        )
        .unwrap();
        assert!(matches!(effects[0], Effect::ResetMousebinds(_)));
        let same = binding_effects(
            normal,
            Some(&user),
            &user.trigger,
            &["Ignore".to_owned()],
            None,
        )
        .unwrap();
        assert_eq!(same.len(), 1);

        let everywhere_left = MouseRow::every_mode(
            trigger("Left", MouseTarget::Frame),
            everywhere(
                "Left",
                MouseTarget::Frame,
                &["MovePane"],
                KeybindingSource::Preset,
            ),
        );
        let effects = binding_effects(
            None,
            Some(&everywhere_left),
            &trigger("Alt Left", MouseTarget::Frame),
            &["MovePane".to_owned()],
            None,
        )
        .unwrap();
        let kdl = reconfigured(&effects);
        assert_eq!(kdl.len(), 2);
        assert!(kdl[0].contains("shared {") && kdl[0].contains("unbind \"Left\""));
        assert!(kdl[1].contains("shared {") && kdl[1].contains("bind \"Alt Left\""));
    }

    #[test]
    fn app_first_is_only_written_when_it_differs_from_the_default() {
        let click = vec!["Click".to_owned()];
        let new_pane = vec!["NewPane".to_owned()];
        assert!(default_app_first(&click));
        assert!(!default_app_first(&new_pane));
        assert!(default_app_first(&["Scroll 5".to_owned()]));
        assert!(!default_app_first(&["MovePane".to_owned()]));
        assert_eq!(app_first_argument(&click, true), None);
        assert_eq!(app_first_argument(&click, false), Some(false));
        assert_eq!(app_first_argument(&new_pane, true), Some(true));
        assert_eq!(app_first_argument(&["PassToApp".to_owned()], true), None);
        let kdl = mousebind_change_kdl_for(
            Some(InputMode::Normal),
            &trigger("Left", MouseTarget::Any),
            Some(&click),
            app_first_argument(&click, false),
        )
        .unwrap();
        assert!(kdl.contains("app_first=false"), "{}", kdl);
    }

    #[test]
    fn all_modes_is_chosen_by_default_and_again_after_leaving() {
        let mut screen = MousebindingsScreen::default();
        assert_eq!(screen.list.selected_mode(), None);
        screen.list.set_selected_mode(Some(InputMode::Pane));
        screen.set_entries(everywhere(
            "Left",
            MouseTarget::Any,
            &["Click"],
            KeybindingSource::Preset,
        ));
        assert_eq!(screen.list.selected_mode(), Some(InputMode::Pane));
        screen.leave();
        assert_eq!(screen.list.selected_mode(), None);
    }

    #[test]
    fn all_modes_shows_each_trigger_once_and_marks_the_ones_that_vary() {
        let mut entries = everywhere(
            "Left",
            MouseTarget::Any,
            &["Click"],
            KeybindingSource::Preset,
        );
        let mut right = everywhere(
            "Right",
            MouseTarget::Any,
            &["ContextMenu"],
            KeybindingSource::Preset,
        );
        right[1].actions = vec!["Ignore".to_owned()];
        right[1].source = KeybindingSource::User;
        right[1].unsaved = true;
        entries.extend(right);
        let mut middle = everywhere(
            "Middle",
            MouseTarget::Any,
            &["PassToApp"],
            KeybindingSource::Preset,
        );
        middle.pop();
        entries.extend(middle);
        let mut screen = MousebindingsScreen::default();
        screen.set_entries(entries);
        let rows = screen.visible();
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.mode.is_none()));
        let row = |button: MouseButton| {
            rows.iter()
                .find(|row| row.trigger.button == button)
                .unwrap()
                .clone()
        };
        let left = row(MouseButton::Left);
        assert!(!left.varies());
        assert_eq!(left.columns()[1], "Click");
        assert_eq!(left.category(), CLICKS);
        assert_eq!(left.source(), &KeybindingSource::Preset);
        assert!(!left.is_unsaved());
        let right = row(MouseButton::Right);
        assert!(right.varies());
        assert_eq!(right.columns()[1], VARIES_TEXT);
        assert!(right.dim_actions());
        assert_eq!(right.category(), VARIES);
        assert_eq!(right.source(), &KeybindingSource::User);
        assert!(right.is_unsaved());
        assert_eq!(
            right.representative().unwrap().actions,
            vec!["ContextMenu".to_owned()]
        );
        assert!(row(MouseButton::Middle).varies());
        screen.list.set_selected_mode(Some(InputMode::Locked));
        let locked = screen.visible();
        assert_eq!(locked.len(), 3);
        assert!(locked
            .iter()
            .all(|row| row.mode == Some(InputMode::Locked) && !row.varies()));
    }

    #[test]
    fn adding_in_all_modes_writes_a_shared_block_and_a_mode_writes_its_own() {
        let mut screen = MousebindingsScreen::default();
        screen.open_add();
        screen.form.as_mut().unwrap().button = MouseButton::Middle;
        screen.finish_form(vec!["NewTab".to_owned()]);
        let kdl = reconfigured(&screen.take_effects());
        assert_eq!(
            kdl,
            vec![
                "mousebinds {\n    shared {\n        bind \"Middle\" { NewTab; }\n    }\n}\n"
                    .to_owned()
            ]
        );
        assert!(screen.take_notice().unwrap().contains("in all modes"));

        screen.list.set_selected_mode(Some(InputMode::Normal));
        screen.open_add();
        screen.form.as_mut().unwrap().button = MouseButton::Middle;
        screen.finish_form(vec!["NewTab".to_owned()]);
        let kdl = reconfigured(&screen.take_effects());
        assert_eq!(
            kdl,
            vec![
                "mousebinds {\n    normal {\n        bind \"Middle\" { NewTab; }\n    }\n}\n"
                    .to_owned()
            ]
        );
    }

    #[test]
    fn searching_in_all_modes_finds_each_trigger_once_and_a_mode_searches_every_mode() {
        let mut entries = vec![
            entry(
                InputMode::Normal,
                "Left",
                MouseTarget::Frame,
                &["ToggleFullscreen"],
                KeybindingSource::Preset,
            ),
            entry(
                InputMode::Locked,
                "Double Left",
                MouseTarget::Frame,
                &["ToggleFullscreen"],
                KeybindingSource::Preset,
            ),
            entry(
                InputMode::Pane,
                "ScrollUp",
                MouseTarget::Any,
                &["Scroll 3"],
                KeybindingSource::Preset,
            ),
        ];
        entries.push(entry(
            InputMode::Pane,
            "Left",
            MouseTarget::Frame,
            &["ToggleFullscreen"],
            KeybindingSource::Preset,
        ));
        let mut screen = MousebindingsScreen::default();
        screen.set_entries(entries);
        screen.list.focus_top();
        type_text(&mut screen, "fullscreen");
        let found = screen.visible();
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|row| row.mode.is_none()));
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));

        screen.list.set_selected_mode(Some(InputMode::Normal));
        assert_eq!(screen.visible().len(), 1);
        screen.list.focus_top();
        type_text(&mut screen, "fullscreen");
        let modes: BTreeSet<Option<InputMode>> = screen.visible().iter().map(|e| e.mode).collect();
        assert_eq!(
            modes,
            BTreeSet::from([
                Some(InputMode::Normal),
                Some(InputMode::Locked),
                Some(InputMode::Pane)
            ])
        );
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert_eq!(screen.visible().len(), 1);
    }

    #[test]
    fn bindings_are_grouped_by_what_they_do() {
        let category = |text: &str, target: MouseTarget, actions: &[&str]| {
            MouseRow::single(entry(
                InputMode::Normal,
                text,
                target,
                actions,
                KeybindingSource::Preset,
            ))
            .category()
        };
        assert_eq!(category("Left", MouseTarget::Any, &["Click"]), CLICKS);
        assert_eq!(category("Left", MouseTarget::Content, &["Select"]), CLICKS);
        assert_eq!(
            category("Double Left", MouseTarget::Any, &["Select"]),
            MULTI_CLICKS
        );
        assert_eq!(
            category("ScrollUp", MouseTarget::Any, &["Scroll 3"]),
            SCROLLING
        );
        assert_eq!(category("Left", MouseTarget::Frame, &["MovePane"]), FRAME);
        assert_eq!(category("Middle", MouseTarget::Frame, &["Ignore"]), PASSED);
        assert_eq!(category("Right", MouseTarget::Any, &["PassToApp"]), PASSED);
        assert_eq!(
            category(
                "Ctrl Left",
                MouseTarget::Any,
                &["NewPane", "SwitchToMode \"normal\""]
            ),
            OTHER
        );
        assert_eq!(
            category("Left", MouseTarget::Any, &["MovePane \"left\""]),
            OTHER
        );
    }

    #[test]
    fn a_mouse_behaviour_must_be_the_only_item() {
        let left = trigger("Left", MouseTarget::Any);
        assert!(check_actions(&left, &["Click".to_owned()]).is_ok());
        assert!(check_actions(&left, &["NewPane".to_owned(), "NewTab".to_owned()]).is_ok());
        let error = check_actions(&left, &["Click".to_owned(), "NewPane".to_owned()]).unwrap_err();
        assert!(error.contains("only item"), "{}", error);
        assert!(check_actions(
            &left,
            &["MovePane \"left\"".to_owned(), "NewPane".to_owned()]
        )
        .is_ok());
        assert!(check_actions(&left, &["Scroll 3".to_owned()]).is_err());
        assert!(check_actions(
            &trigger("ScrollDown", MouseTarget::Any),
            &["Scroll 3".to_owned()]
        )
        .is_ok());
        assert!(check_actions(&left, &[]).is_err());

        let mut screen = MousebindingsScreen::default();
        screen.open_add();
        screen.finish_form(vec!["Select".to_owned(), "NewPane".to_owned()]);
        assert!(screen.form.is_some());
        assert!(screen.form.as_ref().unwrap().error.is_some());
        assert!(screen.take_effects().is_empty());
        screen.finish_form(vec!["Select".to_owned()]);
        assert!(screen.form.is_none());
        assert_eq!(screen.take_effects().len(), 1);
    }

    #[test]
    fn the_form_hides_fields_that_do_not_apply() {
        let mut screen = MousebindingsScreen::default();
        screen.open_add();
        let form = screen.form.as_mut().unwrap();
        assert!(form.fields.keys().contains(&Field::Clicks));
        assert!(form.fields.keys().contains(&Field::AppFirst));
        form.apply_change(
            Field::Button,
            UiValue::Choice {
                index: 3,
                label: "ScrollUp".to_owned(),
            },
        );
        assert!(!form.fields.keys().contains(&Field::Clicks));
        assert_eq!(form.trigger().click_count, 1);
        form.picker.actions = vec!["PassToApp".to_owned()];
        form.sync_after_picker();
        assert!(!form.fields.keys().contains(&Field::AppFirst));
        form.picker.actions = vec!["Scroll 3".to_owned()];
        form.sync_after_picker();
        assert!(form.app_first);
        assert!(form.fields.keys().contains(&Field::AppFirst));
    }
}
