use std::collections::{BTreeMap, BTreeSet};

use zellij_tile::prelude::*;

use crate::settings::kdl_string;
use crate::ui_components::{print_link, request_close};

const LABEL_WIDTH: usize = 16;
const FIELD_WIDTH: usize = 44;
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
    Copy,
    SwitchToPreset,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeysDialog {
    SwitchToPreset,
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
    file_link_hovered: bool,
    link_color: Option<PaletteColor>,
    notice: Option<String>,
    dialog: ConfirmDialog,
    dialog_purpose: Option<KeysDialog>,
    needs_refresh: bool,
    pending_mode_switch: bool,
    focused: bool,
}

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
            file_link_hovered: false,
            link_color: None,
            notice: None,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            needs_refresh: false,
            pending_mode_switch: false,
            focused: is_setup_wizard,
        };
        screen.rebuild();
        screen
    }
    pub fn is_capturing_keys(&self) -> bool {
        self.capturing_unlock
    }
    pub fn dialog_is_open(&self) -> bool {
        self.dialog.is_open()
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
        self.configured_default_mode = snapshot
            .setting(SettingKey::DefaultMode)
            .and_then(|setting| setting.current_value.clone());
        if self.pending_mode_switch {
            self.pending_mode_switch = false;
            let default_mode = snapshot.keybinds.default_mode.unwrap_or(InputMode::Normal);
            switch_to_input_mode(&default_mode);
        }
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
        if self.selection.clears_defaults && !self.is_setup_wizard {
            self.elements
                .add(KeysField::SwitchToPreset, Button::new("Switch to a preset"));
        } else {
            let names = self.preset_names();
            let selected = names
                .iter()
                .position(|name| *name == self.selection.active.name)
                .unwrap_or(0);
            self.elements.add(
                KeysField::Preset,
                Dropdown::new("Preset", names)
                    .selected(selected)
                    .label_width(LABEL_WIDTH),
            );
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
                            .label_width(LABEL_WIDTH),
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
            } else {
                self.elements.add(
                    KeysField::Copy,
                    Button::new("Copy to my keybinds folder to edit"),
                );
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
    fn choose_preset(&mut self, name: &str) {
        if name == self.selection.active.name
            && self.selection.error.is_none()
            && !self.selection.set_on_command_line
        {
            return;
        }
        self.pending_mode_switch = true;
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
    fn copy_preset(&mut self) {
        let preset = self.selection.active.name.clone();
        let new_name = self.copy_name_for(&preset);
        match copy_keybind_preset(&preset, &new_name) {
            Ok(new_name) => {
                self.notice = Some(format!(
                    "Copied to the keybinds folder as {}.kdl and selected",
                    new_name
                ));
                self.apply(format!("keybinds preset={}", kdl_string(&new_name)));
            },
            Err(error) => self.notice = Some(format!("Could not copy the preset: {}", error)),
        }
    }
    fn open_switch_dialog(&mut self) {
        let mut message = "Your keybinds block has clear-defaults=true, so it defines every key itself. Switching removes that block and uses the default preset instead.".to_owned();
        if let Some(default_mode) = &self.configured_default_mode {
            message.push_str(&format!(
                " The default mode (\"{}\") is removed as well, so the preset's own starting mode applies.",
                default_mode
            ));
        }
        self.dialog = ConfirmDialog::new("Switch to a preset?", message)
            .buttons(vec!["Switch", "Cancel"])
            .width(60)
            .opened();
        self.dialog_purpose = Some(KeysDialog::SwitchToPreset);
    }
    fn dialog_response(&mut self, response: UiResponse) {
        if let UiResponse::Submitted(UiValue::Choice { index: 0, .. }) = response {
            if self.dialog_purpose == Some(KeysDialog::SwitchToPreset) {
                unset_config_setting(SettingKey::Keybinds);
                if self.configured_default_mode.is_some() {
                    unset_config_setting(SettingKey::DefaultMode);
                }
                self.pending_mode_switch = true;
                self.apply("keybinds preset=\"default\"".to_owned());
                self.notice = Some("Switched to the default preset".to_owned());
            }
        }
        if !self.dialog.is_open() {
            self.dialog_purpose = None;
        }
    }
    fn save(&mut self) {
        save_config();
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
                    (KeysField::Copy, UiResponse::Activated) => self.copy_preset(),
                    (KeysField::SwitchToPreset, UiResponse::Activated) => self.open_switch_dialog(),
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
                    self.elements.focus_next();
                    return true;
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
        match mouse {
            Mouse::LeftClick(line, column) if self.file_link_at(line, column) => {
                if let Some(path) = self.preset_file() {
                    open_file_floating(FileToOpen::new(path), None, BTreeMap::new());
                }
                return true;
            },
            Mouse::Hover(line, column) => {
                let hovered = self.file_link_at(line, column);
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
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        let mut lines: Vec<(usize, Text)> = vec![];
        let mut rows: Vec<(KeysField, usize)> = vec![];
        let mut link_row: Option<(usize, String)> = None;
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
        } else {
            for line in wrap(
                "Keybinding presets change how modes are accessed and can help prevent key collisions with other programs.",
                width,
            ) {
                lines.push((row, Text::new(line).unbold_all()));
                row += 1;
            }
            row += 1;
        }
        if self.selection.clears_defaults && !self.is_setup_wizard {
            lines.push((row, Text::new(fit("Custom keybindings")).color_all(3)));
            row += 1;
            for line in [
                "Your config file has a keybinds block with clear-defaults=true.",
                "It defines every key itself, so no preset is used.",
            ] {
                lines.push((row, Text::new(fit(line))));
                row += 1;
            }
            row += 1;
            rows.push((KeysField::SwitchToPreset, row));
            row += 2;
        } else {
            rows.push((KeysField::Preset, row));
            row += 2;
            let active = self.selection.active.clone();
            if let Some(path) = self.preset_file() {
                link_row = Some((row, path));
                row += 1;
            } else {
                if let Some(description) = &active.description {
                    let description = self.with_leader_values(description);
                    lines.push((row, Text::new(fit(&description)).unbold_all()));
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
            if !has_leaders {
                lines.push((
                    row,
                    Text::new(fit(
                        "This preset has fixed keys; it has no leader keys to set.",
                    ))
                    .dim_all(),
                ));
                row += 1;
            }
            for field in [KeysField::Copy, KeysField::Save] {
                if self.elements.get(&field).is_some() {
                    row += 1;
                    rows.push((field, row));
                    row += 1;
                }
            }
            if self.selection.set_on_command_line {
                row += 1;
                lines.push((
                    row,
                    Text::new(fit(
                        "Given on the command line for this session; choosing here replaces it",
                    ))
                    .dim_all(),
                ));
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
        }
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
            .filter(|(link_line, _)| visible(*link_line))
            .map(|(link_line, path)| (link_line - offset, path));
        self.elements.clear_areas();
        self.element_rows = rows;
        self.file_link_area = None;
        if let Some((link_line, path)) = link_row {
            let link = truncate(&path, width);
            self.file_link_area = Some(Rect::new(x, y + link_line, link.chars().count(), 1));
            print_link(
                &link,
                x,
                y + link_line,
                self.link_color,
                self.file_link_hovered,
            );
        }
        for (line_row, text) in lines {
            if line_row < height {
                print_text_with_coordinates(text, x, y + line_row, None, None);
            }
        }
        let field_width = FIELD_WIDTH.min(width);
        let element_rows = self.element_rows.clone();
        for (field, field_row) in element_rows {
            if field_row >= height {
                continue;
            }
            let screen_y = y + field_row;
            match self.elements.get_mut(&field) {
                Some(Element::Dropdown(dropdown)) => {
                    dropdown.render(x, screen_y, field_width);
                    if field == KeysField::Preset && self.selection.set_on_command_line {
                        let tag_x = field_width + 2;
                        if tag_x < width {
                            print_text_with_coordinates(
                                Text::new(truncate("command line", width - tag_x)).dim_all(),
                                x + tag_x,
                                screen_y,
                                None,
                                None,
                            );
                        }
                    }
                },
                Some(Element::Button(button)) => {
                    let button_x = if field == KeysField::Unlock {
                        x + LABEL_WIDTH
                    } else {
                        x
                    };
                    button.render(button_x, screen_y)
                },
                _ => {},
            }
        }
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.elements.render_overlays(rows, cols);
        self.dialog.render_centered(rows, cols);
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![];
    let mut current = String::new();
    for word in text.split_whitespace() {
        let needed = if current.is_empty() {
            word.chars().count()
        } else {
            current.chars().count() + 1 + word.chars().count()
        };
        if needed > width && !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
        .into_iter()
        .map(|line| truncate(&line, width))
        .collect()
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
