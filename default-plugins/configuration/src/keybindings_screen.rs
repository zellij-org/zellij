use std::collections::BTreeSet;
use std::str::FromStr;

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::keybind_change_kdl;

use crate::action_picker::{size_button, ActionPicker, PickerResponse, DELETE_WIDTH};
use crate::bindings_list::{mode_name, BindingsList, ListEntry, ListResponse, ListTexts};
use crate::page::{
    actions_summary, changed_by, is_plain, is_shift_tab, note_overlay, render_frame, truncate,
    typed, Effect, Page, PageResponse,
};

const FORM_LABEL_WIDTH: usize = 9;
const KEY_GETTING_READY: &str = "getting ready…";
const FORM_DIALOG_MIN_WIDTH: usize = 30;
const FORM_DIALOG_WIDE_WIDTH: usize = 84;
const FORM_DIALOG_TALL_HEIGHT: usize = 24;

pub fn is_save_key(key: &KeyWithModifier) -> bool {
    key.is_key_with_ctrl_modifier(BareKey::Char('a'))
}

pub fn same_mode_binding<'a>(
    entries: &'a [KeybindingEntry],
    mode: InputMode,
    key: &KeyWithModifier,
    ignore: Option<&KeyWithModifier>,
) -> Option<&'a KeybindingEntry> {
    entries.iter().find(|entry| {
        entry.mode == mode && &entry.key == key && !entry.unbound && Some(&entry.key) != ignore
    })
}

fn modifier_names(key: &KeyWithModifier) -> BTreeSet<String> {
    key.key_modifiers
        .iter()
        .map(|modifier| modifier.to_string().to_lowercase())
        .collect()
}

pub fn leader_warnings(
    entries: &[KeybindingEntry],
    selection: &KeybindsSelectionSnapshot,
    mode: InputMode,
    key: &KeyWithModifier,
) -> Vec<String> {
    let mut warnings = vec![];
    if let Some(unlock) = selection
        .active_values
        .get("unlock")
        .and_then(|value| KeyWithModifier::from_str(value).ok())
    {
        if &unlock == key {
            warnings.push(format!("{} is the unlock key of this preset", key));
        }
    }
    let pressed = modifier_names(key);
    for placeholder in ["primary", "secondary"] {
        let Some(value) = selection.active_values.get(placeholder) else {
            continue;
        };
        let leader: BTreeSet<String> = value
            .split_whitespace()
            .map(|word| word.to_lowercase())
            .collect();
        if leader.is_empty() || leader != pressed {
            continue;
        }
        let leader_mode = entries.iter().find(|entry| {
            &entry.key == key
                && entry.mode != mode
                && matches!(entry.mode, InputMode::Normal | InputMode::Locked)
                && entry.source == KeybindingSource::Preset
                && !entry.unbound
        });
        if let Some(entry) = leader_mode {
            warnings.push(format!(
                "{} is a {} leader key ({}) in {} mode",
                key,
                placeholder,
                value,
                mode_name(entry.mode)
            ));
        }
    }
    warnings
}

const CATEGORY_ORDER: [&str; 12] = [
    "Switch Modes",
    "Focus",
    "Panes",
    "Move",
    "Resize",
    "Tabs",
    "Scroll",
    "Search",
    "Layouts",
    "Input",
    "Session and plugins",
    "Other",
];

fn action_name(action: &str) -> &str {
    action
        .split(|c: char| c.is_whitespace() || c == '{' || c == ';')
        .next()
        .unwrap_or("")
}

pub fn category_of(actions: &[String]) -> &'static str {
    let main = actions
        .iter()
        .map(|action| action_name(action))
        .find(|name| *name != "SwitchToMode");
    let Some(name) = main else {
        return if actions.is_empty() {
            "Other"
        } else {
            "Switch Modes"
        };
    };
    match name {
        "MoveFocus" | "MoveFocusOrTab" | "FocusNextPane" | "FocusPreviousPane" | "SwitchFocus"
        | "FocusLastPane" => "Focus",
        "MovePane" | "MovePaneBackwards" => "Move",
        "Resize" => "Resize",
        "NewTab"
        | "CloseTab"
        | "GoToTab"
        | "GoToNextTab"
        | "GoToPreviousTab"
        | "ToggleTab"
        | "TabNameInput"
        | "UndoRenameTab"
        | "MoveTab"
        | "ToggleActiveSyncTab"
        | "BreakPane"
        | "BreakPaneLeft"
        | "BreakPaneRight" => "Tabs",
        "NextSwapLayout"
        | "PreviousSwapLayout"
        | "ApplyTiledSwapLayout"
        | "ApplyFloatingSwapLayout" => "Layouts",
        "Write" | "WriteChars" | "Copy" => "Input",
        "Detach"
        | "Quit"
        | "SwitchSession"
        | "LaunchOrFocusPlugin"
        | "LaunchPlugin"
        | "MessagePlugin"
        | "DismissInfoPopups"
        | "OpenContextMenu" => "Session and plugins",
        name if name.starts_with("Search") => "Search",
        name if name.contains("Scroll")
            || name == "EditScrollback"
            || name == "SelectCommandAtScrollPosition"
            || name == "CopyLastCommandOutput"
            || name.ends_with("Prompt") =>
        {
            "Scroll"
        },
        name if name.starts_with("New")
            || name.starts_with("Close")
            || name.starts_with("Toggle")
            || name.contains("Pane")
            || name == "Run"
            || name == "Clear" =>
        {
            "Panes"
        },
        _ => "Other",
    }
}

fn entry_category(entry: &KeybindingEntry) -> &'static str {
    if entry.unbound {
        category_of(&entry.preset_actions.clone().unwrap_or_default())
    } else {
        category_of(&entry.actions)
    }
}

const TEXTS: ListTexts = ListTexts {
    title: "Keybindings",
    search_placeholder: "/ to search keys and actions",
    search_hint: "search keys and actions",
    items_hint: "keys",
    plural: "keys",
    no_matches: "No matching keys",
    empty: "No keys · a: bind a key",
    categories: &CATEGORY_ORDER,
    all_modes_option: false,
};

impl ListEntry for KeybindingEntry {
    type Id = (InputMode, KeyWithModifier);
    fn id(&self) -> Self::Id {
        (self.mode, self.key.clone())
    }
    fn mode(&self) -> Option<InputMode> {
        Some(self.mode)
    }
    fn label(&self) -> String {
        self.key.to_string()
    }
    fn bound_actions(&self) -> Option<&[String]> {
        if self.unbound {
            None
        } else {
            Some(&self.actions)
        }
    }
    fn category(&self) -> &'static str {
        entry_category(self)
    }
    fn source(&self) -> &KeybindingSource {
        &self.source
    }
    fn has_preset(&self) -> bool {
        self.preset_actions.is_some()
    }
    fn is_unsaved(&self) -> bool {
        self.unsaved
    }
    fn removal_effects(&self) -> Result<Vec<Effect>, String> {
        removal_effects(self)
    }
    fn reset_effect(entries: &[Self]) -> Effect {
        Effect::ResetKeys(
            entries
                .iter()
                .map(|entry| (entry.mode, entry.key.clone()))
                .collect(),
        )
    }
}

pub fn removal_effects(entry: &KeybindingEntry) -> Result<Vec<Effect>, String> {
    if entry.unbound {
        return Err(format!(
            "{} is already deleted; r brings back the preset's binding",
            entry.key
        ));
    }
    match entry.source {
        KeybindingSource::Layout => Err(format!(
            "{} comes from the layout and cannot be changed here",
            entry.key
        )),
        KeybindingSource::Preset => keybind_change_kdl(entry.mode, &entry.key, None)
            .map(|kdl| vec![Effect::Reconfigure(kdl)]),
        KeybindingSource::User | KeybindingSource::Shared(_) => {
            Ok(vec![Effect::ResetKeys(vec![(
                entry.mode,
                entry.key.clone(),
            )])])
        },
    }
}

pub fn binding_effects(
    mode: InputMode,
    original: Option<&KeybindingEntry>,
    key: &KeyWithModifier,
    actions: &[String],
) -> Result<Vec<Effect>, String> {
    let mut effects = vec![];
    if let Some(original) = original {
        if &original.key != key && !original.unbound {
            effects.extend(removal_effects(original)?);
        }
    }
    effects.push(Effect::Reconfigure(keybind_change_kdl(
        mode,
        key,
        Some(actions),
    )?));
    Ok(effects)
}

#[derive(Debug, Clone, PartialEq)]
enum DialogPurpose {
    Replace(KeyWithModifier),
    SaveKey(KeyWithModifier),
}

struct BindingForm {
    mode: InputMode,
    original: Option<KeybindingEntry>,
    key: Option<KeyWithModifier>,
    capturing: bool,
    picker: ActionPicker,
    warnings: Vec<String>,
    key_button: Button,
    focus: FormFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FormFocus {
    Nothing,
    Key,
    Actions,
}

pub struct KeybindingsScreen {
    list: BindingsList<KeybindingEntry>,
    selection: KeybindsSelectionSnapshot,
    form: Option<BindingForm>,
    dialog: ConfirmDialog,
    dialog_purpose: Option<DialogPurpose>,
    effects: Vec<Effect>,
    notice: Option<String>,
    client_mode: InputMode,
    latest_mode: InputMode,
    base_mode: InputMode,
    capture_return_mode: Option<InputMode>,
}

impl Default for KeybindingsScreen {
    fn default() -> Self {
        KeybindingsScreen {
            list: BindingsList::new(TEXTS),
            selection: KeybindsSelectionSnapshot::default(),
            form: None,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            effects: vec![],
            notice: None,
            client_mode: InputMode::Normal,
            latest_mode: InputMode::Normal,
            base_mode: InputMode::Normal,
            capture_return_mode: None,
        }
    }
}

impl KeybindingsScreen {
    pub fn is_dragging(&self) -> bool {
        self.list.is_dragging()
    }
    pub fn set_entries(
        &mut self,
        entries: Vec<KeybindingEntry>,
        selection: KeybindsSelectionSnapshot,
    ) {
        self.list.set_entries(entries);
        self.selection = selection;
    }
    #[cfg(test)]
    pub fn visible(&self) -> Vec<KeybindingEntry> {
        self.list.visible()
    }
    fn start_capture(&mut self) {
        if let Some(form) = self.form.as_mut() {
            form.capturing = true;
            if self.capture_return_mode.is_none() {
                self.capture_return_mode = Some(self.client_mode);
                self.effects.push(Effect::SwitchMode(InputMode::Locked));
            }
        }
    }
    fn end_capture(&mut self) {
        if let Some(form) = self.form.as_mut() {
            form.capturing = false;
        }
        if let Some(mode) = self.capture_return_mode.take() {
            self.effects.push(Effect::SwitchMode(mode));
        }
    }
    pub fn open_add(&mut self) {
        let mode = self.list.mode();
        self.form = Some(BindingForm {
            mode,
            original: None,
            key: None,
            capturing: false,
            picker: ActionPicker::new("Actions", vec![], Some(self.base_mode), false),
            warnings: vec![],
            key_button: Button::new("").accent_brackets().left_aligned(),
            focus: FormFocus::Key,
        });
        self.start_capture();
    }
    pub fn open_edit(&mut self) {
        let Some(entry) = self.list.selected_entry() else {
            return;
        };
        if entry.source == KeybindingSource::Layout {
            self.notice = Some(format!(
                "{} comes from the layout and cannot be changed here",
                entry.key
            ));
            return;
        }
        let actions = if entry.unbound {
            entry.preset_actions.clone().unwrap_or_default()
        } else {
            entry.actions.clone()
        };
        let warnings =
            leader_warnings(self.list.entries(), &self.selection, entry.mode, &entry.key);
        self.form = Some(BindingForm {
            mode: entry.mode,
            key: Some(entry.key.clone()),
            capturing: false,
            picker: ActionPicker::new("Actions", actions, Some(self.base_mode), false),
            warnings,
            key_button: Button::new("").accent_brackets().left_aligned(),
            focus: FormFocus::Nothing,
            original: Some(entry),
        });
    }
    pub fn captured(&mut self, key: KeyWithModifier) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let mode = form.mode;
        if is_save_key(&key) {
            self.open_dialog(
                "Ctrl a is the save key",
                format!(
                    "Ctrl a saves the settings. Bound in {} mode it may no longer reach this screen. Bind it anyway?",
                    mode_name(mode)
                ),
                vec!["Bind it", "Cancel"],
                DialogPurpose::SaveKey(key),
            );
            return;
        }
        self.check_conflict_then_accept(key);
    }
    fn check_conflict_then_accept(&mut self, key: KeyWithModifier) {
        let Some(form) = self.form.as_ref() else {
            return;
        };
        let mode = form.mode;
        let ignore = form.original.as_ref().map(|original| original.key.clone());
        if let Some(existing) = same_mode_binding(self.list.entries(), mode, &key, ignore.as_ref())
        {
            let message = format!(
                "{} is already bound in {} mode to {}. Replace it?",
                key,
                mode_name(mode),
                truncate(&actions_summary(&existing.actions), 40)
            );
            self.open_dialog(
                "Key already bound",
                message,
                vec!["Replace", "Cancel"],
                DialogPurpose::Replace(key),
            );
            return;
        }
        self.accept_key(key);
    }
    fn accept_key(&mut self, key: KeyWithModifier) {
        let warnings = self
            .form
            .as_ref()
            .map(|form| leader_warnings(self.list.entries(), &self.selection, form.mode, &key))
            .unwrap_or_default();
        if let Some(form) = self.form.as_mut() {
            form.key = Some(key);
            form.warnings = warnings;
            form.focus = if form.picker.is_listing() {
                FormFocus::Key
            } else {
                FormFocus::Actions
            };
        }
        self.end_capture();
    }
    fn open_dialog(
        &mut self,
        title: &str,
        message: String,
        buttons: Vec<&str>,
        purpose: DialogPurpose,
    ) {
        self.dialog = ConfirmDialog::new(title, message)
            .buttons(buttons)
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
            Some(DialogPurpose::SaveKey(key)) => {
                if confirmed {
                    self.check_conflict_then_accept(key);
                } else {
                    self.notice = Some("Press another key, or Esc to stop".to_owned());
                }
            },
            Some(DialogPurpose::Replace(key)) => {
                if confirmed {
                    self.accept_key(key);
                } else {
                    self.notice = Some("Press another key, or Esc to stop".to_owned());
                }
            },
            None => {},
        }
    }
    fn apply_form(&mut self, actions: Vec<String>) {
        let Some(form) = self.form.take() else {
            return;
        };
        let Some(key) = form.key.clone() else {
            self.notice = Some("No key was chosen".to_owned());
            return;
        };
        match binding_effects(form.mode, form.original.as_ref(), &key, &actions) {
            Ok(effects) => {
                self.effects.extend(effects);
                self.notice = Some(format!("Bound {} in {} mode", key, mode_name(form.mode)));
            },
            Err(error) => {
                self.notice = Some(error);
                self.form = Some(form);
            },
        }
    }
    fn close_form(&mut self) {
        self.end_capture();
        self.form = None;
    }
    fn handle_picker_response(&mut self, response: PickerResponse) {
        match response {
            PickerResponse::Done(actions) => self.apply_form(actions),
            PickerResponse::Cancelled => self.close_form(),
            PickerResponse::Pending => {},
        }
        if let Some(form) = self.form.as_mut() {
            if form.picker.take_returned_to_list() {
                form.focus = FormFocus::Nothing;
            }
        }
    }
    fn handle_form_mouse(&mut self, mouse: Mouse) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.capturing {
            return;
        }
        if matches!(form.key_button.handle_mouse(mouse), UiResponse::Activated) {
            form.focus = FormFocus::Key;
            self.start_capture();
            return;
        }
        if form.key.is_some() {
            if matches!(mouse, Mouse::LeftClick(..)) {
                form.focus = FormFocus::Actions;
            }
            let response = form.picker.handle_mouse(mouse);
            self.handle_picker_response(response);
        }
    }
    fn handle_form_key(&mut self, key: &KeyWithModifier) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        if form.capturing {
            if is_plain(key, BareKey::Esc) {
                self.end_capture();
                if self.form.as_ref().map(|f| f.key.is_none()).unwrap_or(false) {
                    self.notice = Some("Enter captures a key, Esc again leaves".to_owned());
                }
            } else {
                self.captured(key.clone());
            }
            return;
        }
        if form.key.is_none() {
            if is_plain(key, BareKey::Enter) {
                self.start_capture();
            } else if is_plain(key, BareKey::Esc) {
                self.close_form();
            }
            return;
        }
        if form.picker.is_listing() {
            let down = is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab);
            let up = is_plain(key, BareKey::Up) || is_shift_tab(key);
            if typed(key, 'k') {
                form.focus = FormFocus::Key;
                self.start_capture();
                return;
            }
            match form.focus {
                FormFocus::Nothing => {
                    if down {
                        form.focus = FormFocus::Key;
                    } else if up {
                        form.focus = FormFocus::Actions;
                        form.picker.select_row(usize::MAX);
                    } else if is_plain(key, BareKey::Esc) {
                        self.close_form();
                    } else if typed(key, 'a') || typed(key, 'b') {
                        form.focus = FormFocus::Actions;
                        let response = form.picker.handle_key(key);
                        self.handle_picker_response(response);
                    }
                    return;
                },
                FormFocus::Key => {
                    if is_plain(key, BareKey::Enter) || typed(key, ' ') {
                        self.start_capture();
                    } else if down {
                        form.focus = FormFocus::Actions;
                        form.picker.select_row(0);
                    } else if is_plain(key, BareKey::Esc) {
                        self.close_form();
                    }
                    return;
                },
                FormFocus::Actions => {
                    if up && form.picker.selected_row() == 0 {
                        form.focus = FormFocus::Key;
                        return;
                    }
                },
            }
        }
        let response = form.picker.handle_key(key);
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
        let latest_mode = self.latest_mode;
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let title = if form.original.is_some() {
            "Change Key"
        } else {
            "Add Key"
        };
        let key_label = if form.capturing && latest_mode != InputMode::Locked {
            KEY_GETTING_READY.to_owned()
        } else if form.capturing {
            "press a key…".to_owned()
        } else {
            form.key
                .as_ref()
                .map(|key| key.to_string())
                .unwrap_or_else(|| "none".to_owned())
        };
        let showing_actions = form.key.is_some() && !form.capturing;
        form.picker.set_label_width(FORM_LABEL_WIDTH);
        let content_column = form.picker.content_column();
        let max_width = cols.saturating_sub(4).max(20);
        let natural_button_width = form
            .picker
            .natural_button_width()
            .max(key_label.chars().count() + 4)
            .max(KEY_GETTING_READY.chars().count() + 4);
        let max_height = rows.saturating_sub(2).max(6);
        let wanted = if showing_actions {
            form.picker.wanted_height()
        } else {
            Some(0)
        };
        let (dialog_width, dialog_height) = match wanted {
            Some(picker_rows) if form.picker.is_listing() || !showing_actions => {
                let width = (content_column + natural_button_width + DELETE_WIDTH + 4)
                    .max(FORM_DIALOG_MIN_WIDTH)
                    .min(max_width);
                let height = 5 + form.warnings.len() + picker_rows;
                (width, height.min(max_height).max(6))
            },
            Some(picker_rows) => {
                let height = 3 + picker_rows;
                let width = (content_column
                    + natural_button_width.max(form.picker.action_name_width())
                    + 4)
                .max(FORM_DIALOG_MIN_WIDTH)
                .min(max_width);
                (width, height.min(max_height).max(6))
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
        if showing_actions && form.picker.is_editing_args() {
            let button_width = natural_button_width
                .min(width.saturating_sub(content_column))
                .max(5);
            form.picker.set_button_width(button_width);
            form.picker.set_active(true);
            form.picker
                .render(x, y, width, (bottom + 1).saturating_sub(y));
            form.picker.render_overlays(rows, cols);
            note_overlay(Rect::new(dialog_x, dialog_y, dialog_width, dialog_height));
            return;
        }
        let key_active = form.capturing || form.focus == FormFocus::Key;
        let label = Text::from("Key");
        let label = if key_active {
            label.color_all(0)
        } else {
            label
        };
        print_text_with_coordinates(label, x, y, None, None);
        let button_width = natural_button_width
            .min(width.saturating_sub(content_column))
            .max(5);
        size_button(&mut form.key_button, button_width);
        let shown_key = truncate(&key_label, button_width.saturating_sub(4));
        let shown_key = if form.key.is_some() && !form.capturing {
            Text::from(shown_key).color_all(3)
        } else {
            Text::from(shown_key)
        };
        form.key_button.set_label(shown_key);
        form.key_button.set_focused(key_active);
        form.key_button.render(x + content_column, y);
        form.picker.set_button_width(button_width);
        let mut row = y + 1;
        for warning in &form.warnings {
            print_text_with_coordinates(
                Text::from(truncate(
                    &format!("⚠ {}", warning),
                    width.saturating_sub(content_column),
                ))
                .error_color_all(),
                x + content_column,
                row,
                None,
                None,
            );
            row += 1;
        }
        row += 1;
        form.picker
            .set_active(form.focus == FormFocus::Actions && !form.capturing);
        if showing_actions {
            form.picker
                .render(x, row, width, (bottom + 1).saturating_sub(row));
            form.picker.render_overlays(rows, cols);
        }
        note_overlay(Rect::new(dialog_x, dialog_y, dialog_width, dialog_height));
    }
    pub fn focus_top(&mut self) {
        self.list.focus_top();
    }
    pub fn is_busy(&self) -> bool {
        self.form.is_some() || self.dialog.is_open() || self.list.has_open_dropdown()
    }
    pub fn clear_areas(&mut self) {
        self.list.clear_areas();
        if let Some(form) = self.form.as_mut() {
            form.key_button.clear_area();
        }
    }
}

impl Page for KeybindingsScreen {
    fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        self.set_entries(snapshot.keybindings.clone(), snapshot.keybinds.clone());
    }
    fn set_mode_info(&mut self, mode_info: &ModeInfo) {
        if self.capture_return_mode.is_none() {
            self.client_mode = mode_info.mode;
        }
        self.latest_mode = mode_info.mode;
        self.base_mode = mode_info.base_mode.unwrap_or(InputMode::Normal);
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
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.list.render(x, y, width, height);
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
        let picker_changed = self
            .form
            .as_mut()
            .map(|form| form.picker.handle_timer() | form.key_button.handle_timer())
            .unwrap_or(false);
        picker_changed || self.list.handle_timer()
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
            if form.capturing {
                return vec![("<any key>", "bind it"), ("<Esc>", "stop")];
            }
            if form.key.is_none() {
                return vec![("<Enter>", "capture a key"), ("<Esc>", "leave")];
            }
            if form.focus == FormFocus::Key && form.picker.is_listing() {
                return vec![
                    ("<Enter>", "press a new key"),
                    ("<↓>", "actions"),
                    ("<Esc>", "cancel"),
                ];
            }
            let mut hints = form.picker.hints();
            if form.picker.is_listing() {
                hints.insert(0, ("<k>", "change key"));
            }
            return hints;
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
        if self.form.is_some() {
            self.close_form();
        }
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
mod tests {
    use super::*;
    use crate::bindings_list::{match_list_entry, ListFocus, SearchMatch};

    fn match_entry(query: &str, entry: &KeybindingEntry) -> Option<SearchMatch> {
        match_list_entry(query, entry)
    }

    fn key(text: &str) -> KeyWithModifier {
        KeyWithModifier::from_str(text).unwrap()
    }

    fn entry(mode: InputMode, key_text: &str, source: KeybindingSource) -> KeybindingEntry {
        KeybindingEntry {
            mode,
            key: key(key_text),
            actions: vec!["NewPane".to_owned()],
            source,
            unbound: false,
            preset_actions: Some(vec!["NewPane".to_owned()]),
            unsaved: false,
        }
    }

    fn selection() -> KeybindsSelectionSnapshot {
        let mut selection = KeybindsSelectionSnapshot::default();
        selection
            .active_values
            .insert("primary".to_owned(), "Ctrl".to_owned());
        selection
            .active_values
            .insert("unlock".to_owned(), "Ctrl g".to_owned());
        selection
    }

    #[test]
    fn a_key_bound_in_the_same_mode_is_a_conflict() {
        let entries = vec![
            entry(InputMode::Normal, "Alt n", KeybindingSource::Preset),
            entry(InputMode::Pane, "Alt y", KeybindingSource::User),
        ];
        assert!(same_mode_binding(&entries, InputMode::Normal, &key("Alt n"), None).is_some());
        assert!(same_mode_binding(&entries, InputMode::Pane, &key("Alt n"), None).is_none());
        assert!(same_mode_binding(
            &entries,
            InputMode::Normal,
            &key("Alt n"),
            Some(&key("Alt n"))
        )
        .is_none());
        let mut unbound = entry(InputMode::Normal, "Alt x", KeybindingSource::User);
        unbound.unbound = true;
        assert!(same_mode_binding(&[unbound], InputMode::Normal, &key("Alt x"), None).is_none());
    }

    #[test]
    fn leader_and_unlock_keys_are_warned_about() {
        let entries = vec![entry(InputMode::Normal, "Ctrl p", KeybindingSource::Preset)];
        let warnings = leader_warnings(&entries, &selection(), InputMode::Pane, &key("Ctrl p"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("primary leader key"));
        let warnings = leader_warnings(&entries, &selection(), InputMode::Tab, &key("Ctrl g"));
        assert_eq!(
            warnings,
            vec!["Ctrl g is the unlock key of this preset".to_owned()]
        );
        assert!(leader_warnings(&entries, &selection(), InputMode::Pane, &key("Alt p")).is_empty());
        assert!(is_save_key(&key("Ctrl a")));
        assert!(!is_save_key(&key("Alt a")));
    }

    #[test]
    fn removing_a_preset_key_unbinds_it_and_removing_a_user_key_resets_it() {
        let preset = entry(InputMode::Normal, "Alt n", KeybindingSource::Preset);
        assert_eq!(
            removal_effects(&preset).unwrap(),
            vec![Effect::Reconfigure(
                keybind_change_kdl(InputMode::Normal, &key("Alt n"), None).unwrap()
            )]
        );
        let user = entry(InputMode::Normal, "Alt y", KeybindingSource::User);
        assert_eq!(
            removal_effects(&user).unwrap(),
            vec![Effect::ResetKeys(vec![(InputMode::Normal, key("Alt y"))])]
        );
        assert!(
            removal_effects(&entry(InputMode::Normal, "Alt y", KeybindingSource::Layout)).is_err()
        );
    }

    #[test]
    fn rebinding_to_another_key_removes_the_old_one() {
        let original = entry(InputMode::Normal, "Alt n", KeybindingSource::Preset);
        let effects = binding_effects(
            InputMode::Normal,
            Some(&original),
            &key("Alt m"),
            &["NewPane".to_owned()],
        )
        .unwrap();
        assert_eq!(effects.len(), 2);
        match &effects[1] {
            Effect::Reconfigure(kdl) => {
                assert!(kdl.contains("bind \"Alt m\""), "{}", kdl);
                let config = zellij_utils::input::config::Config::from_kdl(kdl, None).unwrap();
                assert!(
                    config.keybinds_layers.user.changes.modes[&InputMode::Normal]
                        .bind
                        .contains_key(&key("Alt m"))
                );
            },
            other => panic!("unexpected {:?}", other),
        }
    }

    #[test]
    fn capturing_switches_to_locked_mode_and_back() {
        let mut screen = KeybindingsScreen::default();
        screen.set_entries(
            vec![entry(InputMode::Normal, "Alt n", KeybindingSource::Preset)],
            selection(),
        );
        screen.open_add();
        assert_eq!(
            screen.take_effects(),
            vec![Effect::SwitchMode(InputMode::Locked)]
        );
        screen.handle_key(&key("Alt n"));
        assert!(screen.dialog.is_open());
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        screen.handle_key(&key("Alt y"));
        assert_eq!(
            screen.take_effects(),
            vec![Effect::SwitchMode(InputMode::Normal)]
        );
        assert_eq!(screen.form.as_ref().unwrap().key, Some(key("Alt y")));
    }

    #[test]
    fn a_search_matches_keys_and_actions_and_splits_the_matched_letters() {
        let found = match_entry(
            "altnew",
            &entry(InputMode::Normal, "Alt n", KeybindingSource::Preset),
        )
        .unwrap();
        assert_eq!(found.key, vec![0, 1, 2, 4]);
        assert_eq!(found.actions, vec![1, 2]);
        assert!(match_entry(
            "zzz",
            &entry(InputMode::Normal, "Alt n", KeybindingSource::Preset)
        )
        .is_none());
    }

    #[test]
    fn searching_lists_keys_of_every_mode() {
        let mut screen = KeybindingsScreen::default();
        screen.set_entries(
            vec![
                entry(InputMode::Normal, "Alt n", KeybindingSource::Preset),
                entry(InputMode::Pane, "n", KeybindingSource::Preset),
                entry(InputMode::Tab, "x", KeybindingSource::Preset),
            ],
            selection(),
        );
        assert_eq!(screen.visible().len(), 1);
        screen.handle_key(&key("/"));
        for character in ['n', 'e', 'w'] {
            screen.handle_key(&KeyWithModifier::new(BareKey::Char(character)));
        }
        let modes: Vec<InputMode> = screen.visible().iter().map(|entry| entry.mode).collect();
        assert_eq!(modes.len(), 3);
        assert!(modes.contains(&InputMode::Pane));
        assert!(modes.contains(&InputMode::Tab));
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert_eq!(screen.visible().len(), 1);
    }

    #[test]
    fn marking_a_category_marks_every_key_in_it() {
        let mut screen = KeybindingsScreen::default();
        screen.set_entries(
            vec![
                entry(InputMode::Normal, "Alt n", KeybindingSource::Preset),
                entry(InputMode::Normal, "Alt m", KeybindingSource::Preset),
            ],
            selection(),
        );
        screen.list.focus = ListFocus::List;
        screen.handle_key(&KeyWithModifier::new(BareKey::Char('C')));
        assert_eq!(screen.list.marked.len(), 2);
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert!(screen.list.marked.is_empty());
        screen.handle_key(&KeyWithModifier::new(BareKey::Char(' ')));
        screen.handle_key(&KeyWithModifier::new(BareKey::Down).with_shift_modifier());
        assert_eq!(screen.list.marked.len(), 2);
    }
}
