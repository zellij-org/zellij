use std::collections::{BTreeMap, BTreeSet};

use zellij_tile::prelude::*;

use crate::settings::kdl_string;
use crate::ui_components::{print_link, request_close};

const LABEL_WIDTH: usize = 16;
const PRESET_EXPLANATION: &str =
    "Presets change how modes are reached and help avoid key clashes with other programs";
const FIELD_WIDTH: usize = 44;
const CUSTOM_PRESET: &str = "custom";
const SAVED_PRESET_NAME: &str = "my-keybindings";
const MODIFIER_CHOICES: [&str; 8] = [
    "Ctrl",
    "Alt",
    "Super",
    "Ctrl Alt",
    "Ctrl Shift",
    "Alt Shift",
    "Ctrl Super",
    "Alt Super",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeysField {
    Preset,
    Primary,
    Secondary,
    Unlock,
    SaveAsPreset,
    Save,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum KeysDialog {
    SwitchToPreset(String),
}

pub struct KeysScreen {
    is_setup_wizard: bool,
    presets: Vec<KeybindPresetInfo>,
    preset_errors: Vec<KeybindPresetWithError>,
    selection: KeybindsSelectionSnapshot,
    configured_default_mode: Option<String>,
    elements: FocusGroup<KeysField>,
    element_rows: Vec<(KeysField, usize)>,
    capturing_unlock: bool,
    file_link_area: Option<Rect>,
    file_link_target: Option<String>,
    file_link_hovered: bool,
    link_color: Option<PaletteColor>,
    notice: Option<String>,
    dialog: ConfirmDialog,
    dialog_purpose: Option<KeysDialog>,
    needs_refresh: bool,
    focused: bool,
    naming: Option<PresetNaming>,
    config_file_path: Option<String>,
}

struct PresetNaming {
    dialog: ConfirmDialog,
    input: TextInput,
    then_switch_to: Option<String>,
    error: Option<String>,
}

const NAMING_WIDTH: usize = 60;

impl KeysScreen {
    pub fn new(is_setup_wizard: bool) -> Self {
        let mut screen = KeysScreen {
            is_setup_wizard,
            presets: vec![],
            preset_errors: vec![],
            selection: KeybindsSelectionSnapshot::default(),
            configured_default_mode: None,
            elements: FocusGroup::new().wrap(is_setup_wizard),
            element_rows: vec![],
            capturing_unlock: false,
            file_link_area: None,
            file_link_target: None,
            file_link_hovered: false,
            link_color: None,
            notice: None,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            needs_refresh: false,
            focused: is_setup_wizard,
            naming: None,
            config_file_path: None,
        };
        screen.rebuild();
        screen
    }
    pub fn is_capturing_keys(&self) -> bool {
        self.capturing_unlock || self.naming.is_some()
    }
    pub fn has_open_overlay(&self) -> bool {
        self.elements.has_open_overlay()
    }
    pub fn dialog_is_open(&self) -> bool {
        self.dialog.is_open() || self.naming.is_some()
    }
    pub fn take_needs_refresh(&mut self) -> bool {
        std::mem::replace(&mut self.needs_refresh, false)
    }
    pub fn notice(&self) -> Option<&String> {
        if self.is_setup_wizard {
            None
        } else {
            self.notice.as_ref()
        }
    }
    pub fn set_notice(&mut self, notice: Option<String>) {
        self.notice = notice;
    }
    pub fn set_presets(
        &mut self,
        presets: Vec<KeybindPresetInfo>,
        preset_errors: Vec<KeybindPresetWithError>,
    ) {
        if self.presets != presets || self.preset_errors != preset_errors {
            self.presets = presets;
            self.preset_errors = preset_errors;
            self.rebuild();
        }
    }
    pub fn set_snapshot(&mut self, snapshot: &ConfigSnapshot) {
        self.config_file_path = snapshot.config_file_path.clone();
        self.configured_default_mode = snapshot
            .setting(SettingKey::DefaultMode)
            .and_then(|setting| setting.current_value.clone());
        if self.selection != snapshot.keybinds {
            self.selection = snapshot.keybinds.clone();
            self.rebuild();
        }
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if focused {
            if self.elements.focused_key().is_none() {
                self.elements.focus_first();
            }
        } else {
            self.capturing_unlock = false;
            self.naming = None;
            self.elements.blur();
        }
    }
    fn uses(&self, placeholder: &str) -> bool {
        self.selection.active.uses_placeholder(placeholder)
    }
    fn leader_value(&self, placeholder: &str) -> Option<String> {
        self.selection
            .active_values
            .get(placeholder)
            .cloned()
            .or_else(|| match placeholder {
                "primary" => self.selection.primary.clone(),
                "secondary" => self.selection.secondary.clone(),
                "unlock" => self.selection.unlock.clone(),
                _ => None,
            })
    }
    fn preset_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.presets.iter().map(|p| p.name.clone()).collect();
        let active = self.selection.active.name.clone();
        if !active.is_empty() && !names.contains(&active) {
            names.push(active);
        }
        if names.is_empty() {
            names.push("default".to_owned());
        }
        names
    }
    fn modifier_choices(&self, current: &Option<String>) -> Vec<String> {
        let mut choices: Vec<String> = MODIFIER_CHOICES.iter().map(|c| c.to_string()).collect();
        if let Some(current) = current {
            if !choices.contains(current) {
                choices.push(current.clone());
            }
        }
        choices
    }
    fn rebuild(&mut self) {
        let focused = self.elements.focused_key().copied();
        self.elements = FocusGroup::new().wrap(self.is_setup_wizard);
        if self.is_custom() {
            let mut names = vec![CUSTOM_PRESET.to_owned()];
            names.extend(self.preset_names());
            self.elements.add(
                KeysField::Preset,
                Dropdown::new(Text::new("Preset").color_all(2), names)
                    .selected(0)
                    .label_width(self.preset_label_width())
                    .accent_brackets(),
            );
            self.elements.add(
                KeysField::SaveAsPreset,
                Button::new("Save as a preset").accent_brackets(),
            );
        } else {
            let names = self.preset_names();
            let selected = names
                .iter()
                .position(|name| *name == self.selection.active.name)
                .unwrap_or(0);
            self.elements.add(
                KeysField::Preset,
                Dropdown::new(Text::new("Preset").color_all(2), names)
                    .selected(selected)
                    .label_width(self.preset_label_width())
                    .accent_brackets(),
            );
            if !self.is_setup_wizard && self.preset_file().is_none() {
                self.elements.add(
                    KeysField::SaveAsPreset,
                    Button::new("Save as a preset").accent_brackets(),
                );
            }
            for (field, placeholder, label) in [
                (KeysField::Primary, "primary", "Primary key"),
                (KeysField::Secondary, "secondary", "Secondary key"),
            ] {
                if self.uses(placeholder) {
                    let current = self.leader_value(placeholder);
                    let choices = self.modifier_choices(&current);
                    let selected = current
                        .and_then(|current| choices.iter().position(|c| *c == current))
                        .unwrap_or(0);
                    self.elements.add(
                        field,
                        Dropdown::new(label, choices)
                            .selected(selected)
                            .label_width(LABEL_WIDTH)
                            .accent_brackets(),
                    );
                }
            }
            if self.uses("unlock") {
                let label = if self.capturing_unlock {
                    "press a key…".to_owned()
                } else {
                    self.leader_value("unlock")
                        .unwrap_or_else(|| "not set".to_owned())
                };
                self.elements.add(
                    KeysField::Unlock,
                    Button::new(format!(
                        " {:<width$}",
                        label,
                        width = FIELD_WIDTH - LABEL_WIDTH - 3
                    ))
                    .width(FIELD_WIDTH - LABEL_WIDTH),
                );
            }
            if self.is_setup_wizard {
                self.elements
                    .add(KeysField::Save, Button::new("Apply and save"));
            }
        }
        if self.focused {
            let refocused = focused
                .map(|key| self.elements.focus(&key))
                .unwrap_or(false);
            if !refocused {
                self.elements.focus_first();
            }
        }
    }
    fn apply(&mut self, kdl: String) {
        reconfigure(kdl, false);
        self.needs_refresh = true;
    }
    fn preset_label_width(&self) -> usize {
        if self.is_setup_wizard {
            LABEL_WIDTH
        } else {
            crate::page::SHORT_LABEL_WIDTH
        }
    }
    fn preset_field_width(&self, width: usize) -> usize {
        if self.is_setup_wizard {
            FIELD_WIDTH.min(width)
        } else {
            crate::page::SHORT_FIELD_WIDTH.min(width)
        }
    }
    fn is_custom(&self) -> bool {
        self.selection.clears_defaults && !self.is_setup_wizard
    }
    pub fn focus_preset(&mut self) {
        self.elements.focus(&KeysField::Preset);
    }
    pub fn render_preset_row(&mut self, x: usize, y: usize, width: usize) {
        self.elements.clear_areas();
        self.file_link_area = None;
        let field_width = self.preset_field_width(width);
        if let Some(dropdown) = self.elements.dropdown_mut(&KeysField::Preset) {
            dropdown.render(x, y, field_width);
        }
    }
    fn choose_preset(&mut self, name: &str) {
        if self.is_custom() {
            if name != CUSTOM_PRESET {
                self.open_switch_dialog(name.to_owned());
            }
            return;
        }
        if name == self.selection.active.name
            && self.selection.error.is_none()
            && !self.selection.set_on_command_line
        {
            return;
        }
        self.apply(format!("keybinds preset={}", kdl_string(name)));
    }
    fn modifier_set(value: &str) -> BTreeSet<String> {
        value
            .split_whitespace()
            .map(|part| part.to_lowercase())
            .collect()
    }
    fn set_leader(&mut self, placeholder: &str, value: String) {
        if value.trim().is_empty() {
            self.notice = Some(format!("The {} key cannot be empty", placeholder));
            self.rebuild();
            return;
        }
        let other = match placeholder {
            "primary" => Some("secondary"),
            "secondary" => Some("primary"),
            _ => None,
        };
        if let Some(other) = other.filter(|other| self.uses(other)) {
            if let Some(other_value) = self.leader_value(other) {
                if Self::modifier_set(&other_value) == Self::modifier_set(&value) {
                    self.notice = Some(format!(
                        "The primary and secondary keys must differ (both would be {})",
                        value
                    ));
                    self.rebuild();
                    return;
                }
            }
        }
        if self.leader_value(placeholder).as_deref() == Some(value.as_str()) {
            return;
        }
        if self.selection.set_on_command_line {
            let mut attributes = format!("preset={}", kdl_string(&self.selection.active.name));
            for other in ["primary", "secondary", "unlock"] {
                if other == placeholder || !self.uses(other) {
                    continue;
                }
                if let Some(other_value) = self.selection.active_values.get(other) {
                    attributes.push_str(&format!(" {}={}", other, kdl_string(other_value)));
                }
            }
            self.apply(format!(
                "keybinds {} {}={}",
                attributes,
                placeholder,
                kdl_string(&value)
            ));
        } else {
            self.apply(format!("keybinds {}={}", placeholder, kdl_string(&value)));
        }
    }
    fn unique_preset_name(&self, base: &str) -> String {
        let taken = |name: &str| self.presets.iter().any(|preset| preset.name == name);
        if !taken(base) {
            return base.to_owned();
        }
        let mut index = 2;
        loop {
            let candidate = format!("{}-{}", base, index);
            if !taken(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }
    fn start_naming(&mut self, then_switch_to: Option<String>) {
        let name = if self.is_custom() {
            self.unique_preset_name(SAVED_PRESET_NAME)
        } else {
            self.copy_name_for(&self.selection.active.name)
        };
        let mut input = TextInput::new(name).label("Name").label_width(6).focused();
        input.move_to_end();
        let (title, save_label, explanation) = match &then_switch_to {
            Some(preset) => (
                "Save your keybindings first",
                "Save and switch",
                format!(
                    "Your keybindings will be saved as a preset in your keybinds folder, then the {} preset will be used. You can go back to them at any time by choosing the saved preset here.",
                    preset
                ),
            ),
            None if !self.is_custom() => (
                "Save as a preset",
                "Save",
                format!(
                    "The {} preset will be saved as a new preset in your keybinds folder and used from now on. You can then edit its file to change any key.",
                    self.selection.active.display_name
                ),
            ),
            None => (
                "Save as a preset",
                "Save",
                "Your keybindings will be saved as a preset in your keybinds folder and used from now on. The keybinds block in your config file will be replaced by the preset's name.".to_owned(),
            ),
        };
        let dialog = ConfirmDialog::new(title, format!("{}\n\n\n", explanation))
            .buttons(vec![save_label, "Cancel"])
            .width(NAMING_WIDTH)
            .opened();
        self.naming = Some(PresetNaming {
            dialog,
            input,
            then_switch_to,
            error: None,
        });
    }
    fn finish_naming(&mut self) {
        let Some(naming) = self.naming.as_mut() else {
            return;
        };
        naming.dialog.open();
        let name = naming.input.get_text().trim().to_owned();
        if !is_usable_preset_name(&name) {
            naming.error = Some("Use letters, digits, '-' and '_' for the name".to_owned());
            return;
        }
        if self.presets.iter().any(|preset| preset.name == name) {
            if let Some(naming) = self.naming.as_mut() {
                naming.error = Some(format!("A preset called {} already exists", name));
            }
            return;
        }
        if !self.is_custom() {
            let preset = self.selection.active.name.clone();
            match copy_keybind_preset(&preset, &name) {
                Ok(saved) => {
                    self.naming = None;
                    self.notice = Some(format!(
                        "Saved the {} preset as {} and switched to it",
                        preset, saved
                    ));
                    self.apply(format!("keybinds preset={}", kdl_string(&saved)));
                },
                Err(error) => {
                    if let Some(naming) = self.naming.as_mut() {
                        naming.error = Some(format!("Could not save: {}", error));
                    }
                },
            }
            return;
        }
        match save_keybinds_as_preset(&name) {
            Ok(saved) => {
                let target = self
                    .naming
                    .take()
                    .and_then(|naming| naming.then_switch_to)
                    .unwrap_or_else(|| saved.clone());
                self.switch_from_custom(&target, Some(&saved));
            },
            Err(error) => {
                if let Some(naming) = self.naming.as_mut() {
                    naming.error = Some(format!("Could not save: {}", error));
                }
            },
        }
    }
    fn switch_from_custom(&mut self, preset: &str, saved: Option<&str>) {
        unset_config_setting(SettingKey::Keybinds);
        if self.configured_default_mode.is_some() {
            unset_config_setting(SettingKey::DefaultMode);
        }
        self.apply(format!("keybinds preset={}", kdl_string(preset)));
        self.notice = Some(match saved {
            Some(saved) if saved == preset => format!(
                "Saved your keybindings as the preset {} and switched to it",
                saved
            ),
            Some(saved) => format!(
                "Switched to the {} preset; your keybindings are saved as the preset {}",
                preset, saved
            ),
            None => format!("Switched to the {} preset", preset),
        });
    }
    fn cancel_naming(&mut self) {
        self.naming = None;
        self.notice = Some("Not saved".to_owned());
    }
    fn naming_choice(&mut self, response: UiResponse) {
        match response {
            UiResponse::Submitted(UiValue::Choice { index: 0, .. }) => self.finish_naming(),
            UiResponse::Submitted(_) | UiResponse::Cancelled => self.cancel_naming(),
            _ => {},
        }
    }
    fn handle_naming_key(&mut self, key: &KeyWithModifier) {
        let Some(naming) = self.naming.as_mut() else {
            return;
        };
        let is_tab = key.bare_key == BareKey::Tab;
        if key.has_no_modifiers() && key.bare_key == BareKey::Esc {
            self.cancel_naming();
            return;
        }
        if key.has_no_modifiers() && key.bare_key == BareKey::Enter {
            if naming.dialog.selected_index() == 0 {
                self.finish_naming();
            } else {
                self.cancel_naming();
            }
            return;
        }
        if is_tab {
            let response = naming.dialog.handle_key(key);
            self.naming_choice(response);
            return;
        }
        if let UiResponse::Changed(_) = naming.input.handle_key(key) {
            naming.error = None;
        }
    }
    fn naming_input_position(
        naming: &PresetNaming,
        rows: usize,
        cols: usize,
    ) -> (usize, usize, usize) {
        let (width, height) = naming.dialog.size_for(cols);
        let height = height.min(rows);
        let x = cols.saturating_sub(width) / 2;
        let y = rows.saturating_sub(height) / 2;
        let buttons_y = y + height.saturating_sub(2);
        (x + 2, buttons_y.saturating_sub(3), width.saturating_sub(4))
    }
    fn copy_name_for(&self, preset: &str) -> String {
        let stem = std::path::Path::new(preset)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(preset);
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        let base = format!("{}-custom", stem);
        let taken = |name: &str| self.presets.iter().any(|preset| preset.name == name);
        if !taken(&base) {
            return base;
        }
        let mut index = 2;
        loop {
            let candidate = format!("{}-{}", base, index);
            if !taken(&candidate) {
                return candidate;
            }
            index += 1;
        }
    }
    fn open_switch_dialog(&mut self, preset: String) {
        let mut message = format!("Your keybinds block has clear-defaults=true, so it defines every key itself. Switching removes that block and uses the {} preset instead. Save your keybindings as a preset first to be able to go back to them by choosing it here.", preset);
        if let Some(default_mode) = &self.configured_default_mode {
            message.push_str(&format!(
                " The default mode (\"{}\") is removed as well, so the preset's own starting mode applies.",
                default_mode
            ));
        }
        self.dialog = ConfirmDialog::new("Switch to a preset?", message)
            .buttons(vec![
                "Save mine, then switch",
                "Switch without saving",
                "Cancel",
            ])
            .width(66)
            .opened();
        self.dialog_purpose = Some(KeysDialog::SwitchToPreset(preset));
    }
    fn dialog_response(&mut self, response: UiResponse) {
        let choice = match response {
            UiResponse::Submitted(UiValue::Choice { index, .. }) => Some(index),
            _ => None,
        };
        if let (Some(choice @ (0 | 1)), Some(KeysDialog::SwitchToPreset(preset))) =
            (choice, self.dialog_purpose.clone())
        {
            if choice == 0 {
                self.start_naming(Some(preset));
            } else {
                self.switch_from_custom(&preset, None);
            }
        }
        if !self.dialog.is_open() {
            self.dialog_purpose = None;
            if self.is_custom() {
                if let Some(dropdown) = self.elements.dropdown_mut(&KeysField::Preset) {
                    dropdown.set_selected(0);
                }
            }
        }
    }
    fn save(&mut self) {
        overwrite_config_file();
        request_close();
    }
    fn handle_focus_event(&mut self, event: FocusEvent<KeysField>) -> bool {
        match event {
            FocusEvent::Element { key, response } => {
                match (key, response) {
                    (KeysField::Preset, UiResponse::Changed(UiValue::Choice { label, .. })) => {
                        self.choose_preset(&label)
                    },
                    (KeysField::Primary, UiResponse::Changed(UiValue::Choice { label, .. })) => {
                        self.set_leader("primary", label)
                    },
                    (KeysField::Secondary, UiResponse::Changed(UiValue::Choice { label, .. })) => {
                        self.set_leader("secondary", label)
                    },
                    (KeysField::Unlock, UiResponse::Activated) => {
                        self.capturing_unlock = true;
                        self.notice = Some("Press the new unlock key (Esc cancels)".to_owned());
                        self.rebuild();
                    },
                    (KeysField::SaveAsPreset, UiResponse::Activated) => {
                        self.start_naming(None);
                    },
                    (KeysField::Save, UiResponse::Activated) => self.save(),
                    (_, UiResponse::Cancelled) => {},
                    _ => {},
                }
                true
            },
            FocusEvent::FocusChanged(_) => true,
            FocusEvent::NotHandled => false,
        }
    }
    fn capture_unlock_key(&mut self, key: KeyWithModifier) {
        self.capturing_unlock = false;
        if key.bare_key == BareKey::Esc && key.has_no_modifiers() {
            self.notice = None;
            self.rebuild();
            return;
        }
        self.notice = None;
        self.rebuild();
        self.set_leader("unlock", key.to_kdl());
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        if self.dialog.is_open() {
            let response = self.dialog.handle_key(&key);
            self.dialog_response(response);
            return true;
        }
        if self.capturing_unlock {
            self.capture_unlock_key(key);
            return true;
        }
        if self.naming.is_some() {
            self.handle_naming_key(&key);
            return true;
        }
        self.notice = None;
        if self.is_setup_wizard && key.is_key_with_ctrl_modifier(BareKey::Char('a')) {
            self.save();
            return true;
        }
        let event = self.elements.handle_key(&key);
        if self.handle_focus_event(event) {
            return true;
        }
        if key.has_no_modifiers() {
            match key.bare_key {
                BareKey::Down => {
                    return self.elements.focus_next();
                },
                BareKey::Up => {
                    self.elements.focus_prev();
                    return true;
                },
                BareKey::Esc => {
                    request_close();
                    return true;
                },
                _ => {},
            }
        }
        if key.is_key_with_ctrl_modifier(BareKey::Char('c')) {
            request_close();
            return true;
        }
        false
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        if self.dialog.is_open() {
            let response = self.dialog.handle_mouse(mouse);
            self.dialog_response(response);
            return true;
        }
        if self.capturing_unlock {
            return false;
        }
        if let Some(naming) = self.naming.as_mut() {
            let response = naming.dialog.handle_mouse(mouse);
            if matches!(response, UiResponse::Submitted(_)) {
                self.naming_choice(response);
                return true;
            }
            if let Some(naming) = self.naming.as_mut() {
                naming.input.handle_mouse(mouse);
            }
            return true;
        }
        match mouse {
            Mouse::LeftClick(line, column) if self.file_link_at(line, column) => {
                if let Some(path) = self.file_link_target.clone() {
                    open_file_floating(FileToOpen::new(path), None, BTreeMap::new());
                }
                return true;
            },
            Mouse::Hover(line, column) => {
                let hovered = !self.elements.has_open_overlay()
                    && !crate::page::under_overlay(line, column)
                    && self.file_link_at(line, column);
                let changed = hovered != self.file_link_hovered;
                self.file_link_hovered = hovered;
                let event = self.elements.handle_mouse(mouse);
                return self.handle_focus_event(event) || changed;
            },
            _ => {},
        }
        let event = self.elements.handle_mouse(mouse);
        self.handle_focus_event(event)
    }
    pub fn handle_timer(&mut self) -> bool {
        self.elements.handle_timer()
    }
    fn with_leader_values(&self, text: &str) -> String {
        let mut substituted = text.to_owned();
        for placeholder in ["primary", "secondary", "unlock"] {
            if let Some(value) = self.leader_value(placeholder) {
                substituted = substituted.replace(&format!("{{{}}}", placeholder), &value);
            }
        }
        substituted
    }
    fn example_line(&self, keys: &str, text: &str, width: usize) -> Text {
        let keys = self.with_leader_values(keys);
        let text = self.with_leader_values(text);
        let prefix = "";
        let line = format!("{}{} {}", prefix, keys, text);
        let keys_start = prefix.chars().count();
        let keys_end = keys_start + keys.chars().count();
        let mut styled = Text::new(truncate(&line, width))
            .unbold_all()
            .color_range(3, keys_start..keys_end);
        let text_start = keys_end + 1;
        let mut offset = 0;
        for word in text.split(' ') {
            let word_length = word.chars().count();
            let is_mode_name = word_length > 1 && word.chars().all(|c| c.is_ascii_uppercase());
            if is_mode_name && text_start + offset + word_length <= width {
                styled =
                    styled.color_range(2, text_start + offset..text_start + offset + word_length);
            }
            offset += word_length + 1;
        }
        styled
    }
    fn preset_file(&self) -> Option<String> {
        if self.selection.active.source == KeybindPresetSource::BuiltIn {
            None
        } else {
            self.selection.active.path.clone()
        }
    }
    pub fn set_link_color(&mut self, color: Option<PaletteColor>) {
        self.link_color = color;
    }
    fn file_link_at(&self, line: isize, column: usize) -> bool {
        self.file_link_area
            .map(|area| area.contains(line, column))
            .unwrap_or(false)
    }
    pub fn content_height(&self, width: usize) -> usize {
        self.content(width).3
    }
    pub fn summary(&self) -> String {
        if self.is_custom() {
            return "Every key is defined in your config file".to_owned();
        }
        let leaders: Vec<String> = ["primary", "secondary", "unlock"]
            .iter()
            .filter(|placeholder| self.selection.active.uses_placeholder(placeholder))
            .filter_map(|placeholder| {
                self.leader_value(placeholder)
                    .map(|value| format!("{} {}", placeholder, value))
            })
            .collect();
        if leaders.is_empty() {
            "No leader keys".to_owned()
        } else {
            format!("Leader keys: {}", leaders.join(" · "))
        }
    }
    pub fn focus_last(&mut self) {
        if let Some(last) = self.elements.keys().last().copied() {
            self.elements.focus(&last);
        }
    }
    fn content(
        &self,
        width: usize,
    ) -> (
        Vec<(usize, Text)>,
        Vec<(KeysField, usize)>,
        Option<(usize, usize, String)>,
        usize,
    ) {
        let mut lines: Vec<(usize, Text)> = vec![];
        let mut rows: Vec<(KeysField, usize)> = vec![];
        let mut link_row: Option<(usize, usize, String)> = None;
        let fit = |text: &str| truncate(text, width);
        let mut row = 0;
        if self.is_setup_wizard {
            lines.push((
                row,
                Text::new(fit(
                    "Hi there! How would you like your keybindings to work?",
                ))
                .color_all(2),
            ));
            row += 1;
            lines.push((
                row,
                Text::new(fit("You can change this later in the settings screen.")).dim_all(),
            ));
            row += 2;
        }
        if self.is_custom() {
            rows.push((KeysField::Preset, row));
            rows.push((KeysField::SaveAsPreset, row));
            row += 2;
            let prefix = "Using custom keybinds block from";
            match self.config_file_path.clone() {
                Some(path) => {
                    let prefix_lines = wrap_words(prefix, width);
                    let last = prefix_lines.last().cloned().unwrap_or_default();
                    let last_width = last.chars().count();
                    for (index, line) in prefix_lines.iter().enumerate() {
                        lines.push((row + index, Text::new(line).unbold_all().dim_all()));
                    }
                    row += prefix_lines.len().saturating_sub(1);
                    if last_width + 1 + path.chars().count() <= width {
                        link_row = Some((row, last_width + 1, path));
                    } else {
                        row += 1;
                        link_row = Some((row, 0, path));
                    }
                    row += 1;
                },
                None => {
                    for line in wrap_words(&format!("{} your config file", prefix), width) {
                        lines.push((row, Text::new(line).unbold_all().dim_all()));
                        row += 1;
                    }
                },
            }
            row += 1;
        } else {
            rows.push((KeysField::Preset, row));
            if self.elements.get(&KeysField::SaveAsPreset).is_some() {
                rows.push((KeysField::SaveAsPreset, row));
            }
            if self.selection.set_on_command_line {
                lines.push((
                    row + 1,
                    Text::new(fit(
                        "Given on the command line for this session; choosing here replaces it",
                    ))
                    .dim_all(),
                ));
                row += 1;
            }
            row += 2;
            let active = self.selection.active.clone();
            let is_file_preset = self.preset_file().is_some();
            if let Some(path) = self.preset_file() {
                link_row = Some((row, 0, path));
                row += 1;
            } else {
                let description = active
                    .description
                    .as_ref()
                    .map(|description| self.with_leader_values(description))
                    .unwrap_or_else(|| PRESET_EXPLANATION.to_owned());
                if self.is_setup_wizard {
                    lines.push((row, Text::new(fit(&description)).unbold_all().dim_all()));
                    row += 1;
                }
                for (keys, text) in &active.examples {
                    lines.push((row, self.example_line(keys, text, width)));
                    row += 1;
                }
            }
            row += 1;
            let mut has_leaders = false;
            for field in [KeysField::Primary, KeysField::Secondary] {
                if self.elements.get(&field).is_some() {
                    rows.push((field, row));
                    row += 1;
                    has_leaders = true;
                }
            }
            if self.elements.get(&KeysField::Unlock).is_some() {
                lines.push((row, Text::new("Unlock key")));
                rows.push((KeysField::Unlock, row));
                row += 1;
                has_leaders = true;
            }
            if !has_leaders && !is_file_preset {
                lines.push((
                    row,
                    Text::new(fit(
                        "This preset has fixed keys; it has no leader keys to set.",
                    ))
                    .dim_all(),
                ));
                row += 1;
            }
            if self.elements.get(&KeysField::Save).is_some() {
                row += 1;
                rows.push((KeysField::Save, row));
                row += 1;
            }
        }
        if self.selection.set_by_layout {
            lines.push((
                row,
                Text::new(fit(
                    "The current layout chooses its own preset; it replaces this choice while it is loaded.",
                ))
                .dim_all(),
            ));
            row += 1;
        }
        if let Some(error) = &self.selection.error {
            lines.push((
                row,
                Text::new(fit(&format!("Using the default preset instead: {}", error)))
                    .color_all(3),
            ));
            row += 1;
        }
        if let Some(broken) = self.preset_errors.first() {
            let more = self.preset_errors.len() - 1;
            let suffix = if more > 0 {
                format!(" (and {} more)", more)
            } else {
                String::new()
            };
            lines.push((
                row,
                Text::new(fit(&format!(
                    "Preset {} failed to load: {}{}",
                    broken.name, broken.error, suffix
                )))
                .color_all(3),
            ));
            row += 1;
        }
        if let Some(notice) = self.notice.as_ref().filter(|_| self.is_setup_wizard) {
            lines.push((row, Text::new(fit(notice)).color_all(3)));
            row += 1;
        }
        let total = lines
            .iter()
            .map(|(line_row, _)| *line_row + 1)
            .chain(rows.iter().map(|(_, field_row)| *field_row + 1))
            .chain(link_row.iter().map(|(link_line, _, _)| *link_line + 1))
            .max()
            .unwrap_or(0)
            .max(row.min(1));
        (lines, rows, link_row, total)
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        let (lines, rows, link_row, _) = self.content(width);
        let focused_row = self
            .elements
            .focused_key()
            .and_then(|focused| rows.iter().find(|(field, _)| field == focused))
            .map(|(_, field_row)| *field_row)
            .unwrap_or(0);
        let offset = (focused_row + 1).saturating_sub(height);
        let visible = |content_row: usize| content_row >= offset && content_row < offset + height;
        let lines: Vec<(usize, Text)> = lines
            .into_iter()
            .filter(|(line_row, _)| visible(*line_row))
            .map(|(line_row, text)| (line_row - offset, text))
            .collect();
        let rows: Vec<(KeysField, usize)> = rows
            .into_iter()
            .filter(|(_, field_row)| visible(*field_row))
            .map(|(field, field_row)| (field, field_row - offset))
            .collect();
        let link_row = link_row
            .filter(|(link_line, _, _)| visible(*link_line))
            .map(|(link_line, column, path)| (link_line - offset, column, path));
        self.elements.clear_areas();
        self.element_rows = rows;
        self.file_link_area = None;
        self.file_link_target = None;
        if let Some((link_line, column, path)) = link_row {
            let link = truncate(&path, width.saturating_sub(column));
            self.file_link_area = Some(Rect::new(
                x + column,
                y + link_line,
                link.chars().count(),
                1,
            ));
            print_link(
                &link,
                x + column,
                y + link_line,
                self.link_color,
                self.file_link_hovered,
            );
            self.file_link_target = Some(path);
        }
        for (line_row, text) in lines {
            if line_row < height {
                print_text_with_coordinates(text, x, y + line_row, None, None);
            }
        }
        let save_width = self
            .elements
            .button(&KeysField::SaveAsPreset)
            .map(|button| button.natural_width() + 2)
            .filter(|_| {
                self.element_rows
                    .iter()
                    .any(|(field, _)| *field == KeysField::SaveAsPreset)
            })
            .unwrap_or(0);
        let preset_width = self
            .preset_field_width(width)
            .min(width.saturating_sub(save_width + 1));
        let save_x = if self.is_setup_wizard {
            preset_width + 2
        } else {
            crate::page::BESIDE_SHORT_FIELD
        };
        let field_width = FIELD_WIDTH.min(width);
        let element_rows = self.element_rows.clone();
        for (field, field_row) in element_rows {
            if field_row >= height {
                continue;
            }
            let screen_y = y + field_row;
            match self.elements.get_mut(&field) {
                Some(Element::Dropdown(dropdown)) => {
                    let dropdown_width = if field == KeysField::Preset {
                        preset_width
                    } else {
                        field_width
                    };
                    dropdown.render(x, screen_y, dropdown_width);
                },
                Some(Element::Button(button)) => {
                    let button_x = match field {
                        KeysField::Unlock => x + LABEL_WIDTH,
                        KeysField::SaveAsPreset => x + save_x,
                        _ => x,
                    };
                    button.render(button_x, screen_y)
                },
                _ => {},
            }
        }
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.elements.render_overlays(rows, cols);
        crate::page::note_group(&self.elements);
        self.dialog.render_centered(rows, cols);
        if let Some(naming) = self.naming.as_mut() {
            naming.dialog.render_centered(rows, cols);
            let (x, y, width) = Self::naming_input_position(naming, rows, cols);
            naming.input.set_show_cursor(true);
            naming.input.render(x, y, width);
            if let Some(error) = &naming.error {
                print_text_with_coordinates(
                    Text::new(truncate(error, width)).error_color_all(),
                    x,
                    y + 1,
                    None,
                    None,
                );
            }
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_owned()
    } else if width == 0 {
        String::new()
    } else {
        let mut truncated: String = text.chars().take(width.saturating_sub(1)).collect();
        truncated.push('…');
        truncated
    }
}

fn is_usable_preset_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![];
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_owned();
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        while word.chars().count() > width {
            let head: String = word.chars().take(width).collect();
            word = word.chars().skip(width).collect();
            if !line.is_empty() {
                lines.push(std::mem::take(&mut line));
            }
            lines.push(head);
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}
