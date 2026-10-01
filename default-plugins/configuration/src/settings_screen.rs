use std::collections::BTreeMap;

use zellij_tile::prelude::*;

use crate::keys_screen::KeysScreen;
use crate::settings::{
    check_text, describe, kdl_for, option_value, section, settings_in, sort_for_display,
    Category, Scope, SettingInfo, SettingKind, CATEGORIES, MISSING_SUFFIX, UNSET_CHOICE,
};
use crate::theme_preview::{PreviewAction, ThemePreview, PREVIEWED_THEME_SETTINGS};
use crate::ui_components::{print_link, take_close_request};

pub const MIN_COLS: usize = 50;
pub const MIN_ROWS: usize = 12;
const HEADER_ROWS: usize = 4;
const MAX_LABEL_WIDTH: usize = 26;
const MAX_VALUE_WIDTH: usize = 28;
const MIN_DROPDOWN_WIDTH: usize = 10;
const MENU_PADDING: usize = 4;
const SECTION_PADDING: usize = 1;
const FOOTER_ROWS: usize = 4;
const CONTENT_WIDTH: usize = 80;
const KEYS_SCREEN_ROWS: usize = 18;
const CLOSE_NOTICE_SECONDS: f64 = 1.5;
const DEFAULT_THEME: &str = "default";
const FILE_PREFIX: &str = "File: ";
const THEME_NOTE: &str = "  A dark or light terminal theme is set and is used instead of this one";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Menu,
    Content,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DialogPurpose {
    ChangedOutside,
    RevertAll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Setting(SettingKey),
    Heading(String),
    Line(String),
}

pub struct SettingsScreen {
    snapshot: ConfigSnapshot,
    menu: SideMenu,
    focus: Focus,
    rows: Vec<Row>,
    elements: FocusGroup<SettingKey>,
    row_offsets: BTreeMap<SettingKey, (usize, usize)>,
    scroll: ScrollView,
    search: TextInput,
    search_active: bool,
    dialog: ConfirmDialog,
    dialog_purpose: Option<DialogPurpose>,
    awaiting_save: Option<Vec<SettingKey>>,
    awaiting_reload: bool,
    notice: Option<String>,
    closing: bool,
    theme_preview: ThemePreview,
    theme_note_shown: bool,
    keys_screen: KeysScreen,
    latest_mode_info: Option<ModeInfo>,
    follow_focus: bool,
    last_size: (usize, usize),
    editing: Option<SettingKey>,
    file_link_area: Option<Rect>,
    file_link_hovered: bool,
}

impl Default for SettingsScreen {
    fn default() -> Self {
        let titles: Vec<&str> = CATEGORIES.iter().map(|c| c.title()).collect();
        SettingsScreen {
            snapshot: ConfigSnapshot::default(),
            menu: SideMenu::new(titles).focused(),
            focus: Focus::Menu,
            rows: vec![],
            elements: FocusGroup::new().wrap(false),
            row_offsets: BTreeMap::new(),
            scroll: ScrollView::new(0),
            search: TextInput::empty()
                .placeholder("search all settings")
                .search_mode(),
            search_active: false,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            awaiting_save: None,
            awaiting_reload: false,
            notice: None,
            closing: false,
            theme_preview: ThemePreview::default(),
            theme_note_shown: false,
            keys_screen: KeysScreen::new(false),
            latest_mode_info: None,
            follow_focus: false,
            last_size: (0, 0),
            editing: None,
            file_link_area: None,
            file_link_hovered: false,
        }
    }
}

fn run_preview_action(action: PreviewAction) {
    match action {
        PreviewAction::Set(key, value) => reconfigure(kdl_for(key, &value), false),
        PreviewAction::Unset(key) => unset_config_setting(key),
        PreviewAction::Revert(key) => revert_config(Some(key)),
    }
}

fn is_plain_key(key: &KeyWithModifier, bare_key: BareKey) -> bool {
    key.bare_key == bare_key && key.has_no_modifiers()
}

fn is_ctrl_key(key: &KeyWithModifier, character: char) -> bool {
    key.is_key_with_ctrl_modifier(BareKey::Char(character))
}

fn is_shift_tab(key: &KeyWithModifier) -> bool {
    key.bare_key == BareKey::Tab && key.has_modifiers(&[KeyModifier::Shift])
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

fn key_hints(prefix: &str, hints: &[(&str, &str)], cols: usize) -> Text {
    let mut line = prefix.to_owned();
    let mut key_ranges = vec![];
    for (index, (key, action)) in hints.iter().enumerate() {
        let separator = if index == 0 { "" } else { ", " };
        let entry = format!("{}{} - {}", separator, key, action);
        if line.chars().count() + entry.chars().count() > cols {
            break;
        }
        let key_start = line.chars().count() + separator.chars().count();
        key_ranges.push(key_start..key_start + key.chars().count());
        line.push_str(&entry);
    }
    let mut text = Text::new(truncate(&line, cols));
    for range in key_ranges {
        text = text.color_range(3, range);
    }
    text
}

impl SettingsScreen {
    pub fn new() -> Self {
        let mut screen = SettingsScreen::default();
        screen.refresh();
        screen.rebuild_rows();
        screen
    }
    fn category(&self) -> Category {
        CATEGORIES
            .get(self.menu.selected_index())
            .copied()
            .unwrap_or(Category::Appearance)
    }
    fn showing_keys_screen(&self) -> bool {
        !self.search_active && self.category().is_keys_screen()
    }
    fn setting(&self, key: SettingKey) -> Option<&ConfigSettingState> {
        self.snapshot.setting(key)
    }
    fn current_value(&self, key: SettingKey) -> Option<String> {
        self.setting(key).and_then(|s| s.current_value.clone())
    }
    pub fn refresh(&mut self) {
        self.snapshot = read_config();
        self.keys_screen.set_snapshot(&self.snapshot);
        if self.theme_note_needed() != self.theme_note_shown
            && !self.elements.has_open_overlay()
            && self.editing.is_none()
        {
            self.rebuild_rows();
        }
        self.sync_elements();
        self.update_menu_labels();
    }
    fn theme_note_needed(&self) -> bool {
        self.current_value(SettingKey::ThemeDark).is_some()
            || self.current_value(SettingKey::ThemeLight).is_some()
    }
    fn update_menu_labels(&mut self) {
        let labels: Vec<String> = CATEGORIES
            .iter()
            .map(|category| {
                let has_unsaved = self.snapshot.settings.iter().any(|setting| {
                    setting.is_unsaved() && {
                        let info = describe(setting.key);
                        info.category == *category
                            || (info.kind == SettingKind::Keybindings && category.is_keys_screen())
                    }
                });
                if has_unsaved {
                    format!("{} ●", category.title())
                } else {
                    category.title().to_owned()
                }
            })
            .collect();
        if labels.as_slice() != self.menu.items() {
            let selected = self.menu.selected_index();
            self.menu.set_items(labels);
            self.menu.set_selected(selected);
        }
    }
    fn displayed_settings(&self) -> Vec<SettingKey> {
        if self.search_active {
            let query = self.search.get_text().trim().to_lowercase();
            let mut keys = SettingKey::all()
                .into_iter()
                .filter(|key| describe(*key).kind != SettingKind::Keybindings)
                .filter(|key| {
                    if query.is_empty() {
                        return true;
                    }
                    let info = describe(*key);
                    fuzzy_match_indices(&query, &info.name.to_lowercase()).is_some()
                        || info.description.to_lowercase().contains(&query)
                        || key.id().contains(&query)
                })
                .collect::<Vec<_>>();
            sort_for_display(&mut keys);
            keys
        } else {
            settings_in(self.category())
        }
    }
    fn read_only_rows(&self) -> Vec<Row> {
        let mut rows = vec![];
        let sections: [(&str, &Vec<String>); 4] = [
            ("Plugin aliases", &self.snapshot.plugin_aliases),
            (
                "Plugins loaded at start (load_plugins)",
                &self.snapshot.load_plugins,
            ),
            ("Environment variables (env)", &self.snapshot.env_vars),
            (
                "Right-click menu items (context_menu, only you)",
                &self.snapshot.context_menu_items,
            ),
        ];
        for (title, lines) in sections {
            rows.push(Row::Heading(title.to_owned()));
            if lines.is_empty() {
                rows.push(Row::Line("  (none)".to_owned()));
            }
            for line in lines {
                rows.push(Row::Line(format!("  {}", line)));
            }
        }
        rows
    }
    fn rebuild_rows(&mut self) {
        self.editing = None;
        self.theme_note_shown = self.theme_note_needed();
        let focused = self.elements.focused_key().copied();
        self.elements = FocusGroup::new().wrap(false);
        self.rows.clear();
        if !self.search_active && self.category() == Category::PluginsEnvironmentAndMenu {
            self.rows = self.read_only_rows();
        } else {
            let mut current_heading: Option<String> = None;
            for key in self.displayed_settings() {
                let Some(element) = self.build_element(key) else {
                    continue;
                };
                let heading = if self.search_active {
                    describe(key).category.title().to_owned()
                } else {
                    section(key).to_owned()
                };
                if current_heading.as_ref() != Some(&heading) {
                    if !heading.is_empty() {
                        self.rows.push(Row::Heading(heading.clone()));
                    }
                    current_heading = Some(heading);
                }
                self.elements.add(key, element);
                self.rows.push(Row::Setting(key));
                if key == SettingKey::Theme && self.theme_note_shown {
                    self.rows.push(Row::Line(THEME_NOTE.to_owned()));
                }
            }
        }
        if let Some(focused) = focused {
            if self.focus == Focus::Content {
                if !self.elements.focus(&focused) {
                    self.elements.focus_first();
                }
            }
        } else if self.focus == Focus::Content {
            self.elements.focus_first();
        }
    }
    fn label_width(&self) -> usize {
        self.displayed_settings()
            .iter()
            .map(|key| describe(*key).name.chars().count())
            .max()
            .unwrap_or(0)
            .min(MAX_LABEL_WIDTH)
            + 2
    }
    fn value_width(&self) -> usize {
        let widest = self
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Setting(key) => Some(*key),
                _ => None,
            })
            .map(|key| {
                let info = describe(key);
                match info.kind {
                    SettingKind::Number { min, max, .. } => {
                        min.to_string().len().max(max.to_string().len())
                    },
                    _ => self
                        .choice_options(key, &info)
                        .iter()
                        .map(|option| option.chars().count())
                        .max()
                        .unwrap_or(0),
                }
            })
            .max()
            .unwrap_or(0);
        (widest + 4).clamp(MIN_DROPDOWN_WIDTH, MAX_VALUE_WIDTH)
    }
    fn theme_options(&self, can_be_unset: bool, current: &Option<String>) -> Vec<String> {
        let mut options: Vec<String> = vec![];
        if can_be_unset {
            options.push(UNSET_CHOICE.to_owned());
        }
        let mut names: Vec<String> = self.snapshot.theme_names.clone();
        names.push(DEFAULT_THEME.to_owned());
        if let Some(current) = current {
            if !names.contains(current) {
                names.push(format!("{}{}", current, MISSING_SUFFIX));
            }
        }
        names.sort();
        names.dedup();
        names.retain(|name| name != DEFAULT_THEME);
        options.push(DEFAULT_THEME.to_owned());
        options.extend(names);
        options
    }
    fn choice_options(&self, key: SettingKey, info: &SettingInfo) -> Vec<String> {
        let current = self.current_value(key);
        match info.kind {
            SettingKind::Choice(choices) => choices.iter().map(|c| c.to_string()).collect(),
            SettingKind::OptionalChoice(choices) => std::iter::once(UNSET_CHOICE.to_owned())
                .chain(choices.iter().map(|c| c.to_string()))
                .collect(),
            SettingKind::OptionalToggle => vec![
                UNSET_CHOICE.to_owned(),
                "true".to_owned(),
                "false".to_owned(),
            ],
            SettingKind::Theme { can_be_unset } => self.theme_options(can_be_unset, &current),
            _ => vec![],
        }
    }
    fn selected_choice_index(&self, key: SettingKey, options: &[String]) -> usize {
        let info = describe(key);
        let value = self.current_value(key).or_else(|| match info.kind {
            SettingKind::Choice(_)
            | SettingKind::Theme {
                can_be_unset: false,
            } => Some(info.default.to_owned()),
            _ => Some(UNSET_CHOICE.to_owned()),
        });
        value
            .and_then(|value| {
                options
                    .iter()
                    .position(|option| option_value(option) == value)
            })
            .unwrap_or(0)
    }
    fn toggle_value(&self, key: SettingKey) -> bool {
        self.current_value(key)
            .unwrap_or_else(|| describe(key).default.to_owned())
            == "true"
    }
    fn number_value(&self, key: SettingKey, min: i64) -> i64 {
        self.current_value(key)
            .and_then(|v| v.parse().ok())
            .or_else(|| describe(key).default.parse().ok())
            .unwrap_or(min)
    }
    fn build_element(&self, key: SettingKey) -> Option<Element> {
        let info = describe(key);
        let label_width = self.label_width();
        let element: Element = match info.kind {
            SettingKind::Toggle => Toggle::new(info.name, self.toggle_value(key))
                .label_width(label_width)
                .into(),
            SettingKind::OptionalToggle
            | SettingKind::Choice(_)
            | SettingKind::OptionalChoice(_)
            | SettingKind::Theme { .. } => {
                let options = self.choice_options(key, &info);
                let selected = self.selected_choice_index(key, &options);
                Dropdown::new(info.name, options)
                    .selected(selected)
                    .label_width(label_width)
                    .into()
            },
            SettingKind::Text(_) => TextInput::new(self.current_value(key).unwrap_or_default())
                .label(info.name)
                .label_width(label_width)
                .placeholder(info.default)
                .into(),
            SettingKind::Number { min, max, step } => {
                NumberStepper::new(info.name, self.number_value(key, min))
                    .min(min)
                    .max(max)
                    .step(step)
                    .label_width(label_width)
                    .into()
            },
            SettingKind::Keybindings => return None,
        };
        Some(element)
    }
    fn sync_elements(&mut self) {
        let keys = self.elements.keys();
        for key in keys {
            let info = describe(key);
            match info.kind {
                SettingKind::Toggle => {
                    let on = self.toggle_value(key);
                    if let Some(toggle) = self.elements.toggle_mut(&key) {
                        toggle.set_on(on);
                    }
                },
                SettingKind::OptionalToggle
                | SettingKind::Choice(_)
                | SettingKind::OptionalChoice(_)
                | SettingKind::Theme { .. } => {
                    let options = self.choice_options(key, &info);
                    let selected = self.selected_choice_index(key, &options);
                    let previewing = self.theme_preview.is_previewing(key);
                    if let Some(dropdown) = self.elements.dropdown_mut(&key) {
                        if !dropdown.is_open() && !previewing {
                            if dropdown.options() != options.as_slice() {
                                dropdown.set_options(options);
                            }
                            dropdown.set_selected(selected);
                        } else if dropdown.options() != options.as_slice() {
                            let highlighted = dropdown
                                .highlighted_index()
                                .and_then(|index| dropdown.options().get(index))
                                .map(|label| option_value(label).to_owned());
                            dropdown.set_options(options.clone());
                            dropdown.set_selected(selected);
                            if let Some(index) = highlighted.and_then(|highlighted| {
                                options
                                    .iter()
                                    .position(|option| option_value(option) == highlighted)
                            }) {
                                dropdown.set_highlighted_index(index);
                            }
                        }
                    }
                },
                SettingKind::Text(_) => {
                    let current = self.current_value(key).unwrap_or_default();
                    let editing = self.editing == Some(key);
                    if let Some(input) = self.elements.text_input_mut(&key) {
                        if !editing && input.get_text() != current {
                            input.set_text(current);
                        }
                    }
                },
                SettingKind::Number { min, .. } => {
                    let value = self.number_value(key, min);
                    if let Some(stepper) = self.elements.number_stepper_mut(&key) {
                        if !stepper.is_editing() {
                            stepper.set_value(value);
                        }
                    }
                },
                SettingKind::Keybindings => {},
            }
        }
    }
    fn set_value(&mut self, key: SettingKey, value: &str) {
        let matches_saved = match self.setting(key).and_then(|s| s.saved_value.as_deref()) {
            Some(saved) => saved == value,
            None => describe(key).default == value,
        };
        if matches_saved {
            revert_config(Some(key));
        } else {
            reconfigure(kdl_for(key, value), false);
        }
        self.refresh();
    }
    fn unset_value(&mut self, key: SettingKey) {
        unset_config_setting(key);
        self.refresh();
    }
    fn revert_focused(&mut self) {
        if let Some(key) = self.elements.focused_key().copied() {
            let is_unsaved = self.setting(key).map(|s| s.is_unsaved()).unwrap_or(false);
            if is_unsaved {
                revert_config(Some(key));
                self.refresh();
                self.force_sync_element(key);
                self.notice = Some(format!("Reverted {}", describe(key).name));
            }
        }
    }
    fn force_sync_element(&mut self, key: SettingKey) {
        let current = self.current_value(key).unwrap_or_default();
        if let Some(input) = self.elements.text_input_mut(&key) {
            input.set_text(current);
        }
    }
    fn save(&mut self) {
        self.awaiting_save = Some(self.snapshot.pending_restart_settings.clone());
        self.awaiting_reload = false;
        save_config();
    }
    pub fn config_file_changed_since_read(&mut self) {
        self.awaiting_save = None;
        let file = self
            .snapshot
            .config_file_path
            .clone()
            .unwrap_or_else(|| "The config file".to_owned());
        let message = format!(
            "{} changed outside Zellij since it was last read. Overwrite applies your unsaved changes to the file as it is now. Reload reads the file again and keeps your changes unsaved.",
            file
        );
        self.dialog = ConfirmDialog::new("Config file changed", message)
            .buttons(vec!["Overwrite", "Reload", "Cancel"])
            .width(64)
            .opened();
        self.clear_all_hover();
        self.dialog_purpose = Some(DialogPurpose::ChangedOutside);
    }
    fn open_revert_all_dialog(&mut self) {
        let count = self.snapshot.unsaved_count();
        let message = format!(
            "Revert {} unsaved change{} to the values in the config file?",
            count,
            if count == 1 { "" } else { "s" }
        );
        self.dialog = ConfirmDialog::new("Revert all?", message)
            .buttons(vec!["Revert all", "Cancel"])
            .width(50)
            .opened();
        self.clear_all_hover();
        self.dialog_purpose = Some(DialogPurpose::RevertAll);
    }
    fn request_save(&mut self) {
        self.save();
    }
    fn request_revert_all(&mut self) {
        if self.snapshot.unsaved_count() > 0 {
            self.open_revert_all_dialog();
        } else {
            self.notice = Some("Nothing to revert".to_owned());
        }
    }
    fn dialog_response(&mut self, response: UiResponse) {
        let purpose = self.dialog_purpose;
        if purpose == Some(DialogPurpose::ChangedOutside) {
            match response {
                UiResponse::Submitted(UiValue::Choice { index: 0, .. }) => {
                    self.awaiting_save = Some(self.snapshot.pending_restart_settings.clone());
                    self.awaiting_reload = false;
                    overwrite_config_file();
                },
                UiResponse::Submitted(UiValue::Choice { index: 1, .. }) => {
                    self.awaiting_save = None;
                    self.awaiting_reload = true;
                    reload_config_file();
                },
                UiResponse::Submitted(_) | UiResponse::Cancelled => {
                    self.notice = Some("Not saved".to_owned());
                },
                _ => {},
            }
        }
        if let UiResponse::Submitted(UiValue::Choice { index: 0, .. }) = response {
            match purpose {
                Some(DialogPurpose::ChangedOutside) => {},
                Some(DialogPurpose::RevertAll) => {
                    revert_config(None);
                    self.refresh();
                    let keys = self.elements.keys();
                    for key in keys {
                        self.force_sync_element(key);
                    }
                    self.notice = Some("All unsaved changes reverted".to_owned());
                },
                None => {},
            }
        }
        if !self.dialog.is_open() {
            self.dialog_purpose = None;
        }
    }
    pub fn begin_close(&mut self) {
        self.restore_theme_preview();
        let unsaved = self.snapshot.unsaved_count();
        if unsaved == 0 {
            close_self();
        } else {
            self.notice = Some(format!(
                "{} unsaved change{} stay applied to this session. Ctrl+a in settings saves them.",
                unsaved,
                if unsaved == 1 { "" } else { "s" }
            ));
            self.closing = true;
            self.clear_all_hover();
            set_timeout(CLOSE_NOTICE_SECONDS);
        }
    }
    pub fn config_written(&mut self) {
        if std::mem::replace(&mut self.awaiting_reload, false) {
            let unsaved = self.snapshot.unsaved_count();
            self.notice = Some(format!(
                "Reloaded the config file; {} unsaved change{} kept",
                unsaved,
                if unsaved == 1 { "" } else { "s" }
            ));
        }
        if let Some(restart_settings) = self.awaiting_save.take() {
            let path = self
                .snapshot
                .config_file_path
                .clone()
                .unwrap_or_else(|| "config file".to_owned());
            self.notice = Some(if restart_settings.is_empty() {
                format!("Saved to {}", path)
            } else {
                let names: Vec<&str> = restart_settings
                    .iter()
                    .map(|key| describe(*key).name)
                    .collect();
                format!("Saved. Restart Zellij to apply: {}", names.join(", "))
            });
        }
        self.refresh();
        self.update_theme_preview();
    }
    pub fn config_write_failed(&mut self, path: Option<String>) {
        self.awaiting_save = None;
        self.awaiting_reload = false;
        self.notice = Some(match path {
            Some(path) => format!("Failed to write the config file: {}", path),
            None => "Failed to write the config file".to_owned(),
        });
    }
    pub fn changes_dropped(&mut self, dropped: Vec<SettingKey>) {
        let names: Vec<&str> = dropped.iter().map(|key| describe(*key).name).collect();
        self.notice = Some(format!(
            "The config file changed; your unsaved change{} to {} {} replaced by the file",
            if names.len() == 1 { "" } else { "s" },
            names.join(", "),
            if names.len() == 1 { "was" } else { "were" }
        ));
        self.theme_preview.file_replaced(&dropped);
        self.refresh();
        self.update_theme_preview();
    }
    pub fn before_close(&mut self) {
        if let Some(action) = self.theme_preview.restore() {
            run_preview_action(action);
        }
    }
    pub fn hidden(&mut self) {
        self.restore_theme_preview();
        if self.elements.has_open_overlay() {
            self.elements.blur();
            if self.focus == Focus::Content {
                let focused = self.elements.focused_key().copied();
                if let Some(focused) = focused {
                    self.elements.focus(&focused);
                }
            }
        }
    }
    pub fn update_keybind_presets(
        &mut self,
        presets: Vec<KeybindPresetInfo>,
        preset_errors: Vec<KeybindPresetWithError>,
    ) {
        self.keys_screen.set_presets(presets, preset_errors);
    }
    pub fn update_mode_info(&mut self, mode_info: ModeInfo) {
        self.keys_screen
            .set_link_color(Some(mode_info.style.colors.text_unselected.emphasis_2));
        self.latest_mode_info = Some(mode_info);
        if self.theme_preview.active_key().is_none() {
            self.refresh();
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        if self.closing {
            close_self();
        }
        let keys_screen_changed = self.keys_screen.handle_timer();
        self.elements.handle_timer() || keys_screen_changed
    }
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.menu.set_focused(focus == Focus::Menu);
        self.search.set_focused(focus == Focus::Search);
        self.keys_screen
            .set_focused(focus == Focus::Content && self.showing_keys_screen());
        match focus {
            Focus::Content => {
                if self.elements.focused_key().is_none() {
                    self.elements.focus_first();
                }
                self.follow_focus = true;
            },
            _ => self.elements.blur(),
        }
    }
    fn open_search(&mut self) {
        self.restore_theme_preview();
        self.search_active = true;
        self.search.clear();
        self.scroll.set_offset(0);
        self.set_focus(Focus::Search);
        self.rebuild_rows();
    }
    fn close_search(&mut self) {
        self.search_active = false;
        self.search.clear();
        self.scroll.set_offset(0);
        self.set_focus(Focus::Menu);
        self.rebuild_rows();
    }
    fn select_category(&mut self) {
        self.restore_theme_preview();
        self.scroll.set_offset(0);
        self.elements.blur();
        self.rebuild_rows();
    }
    fn focused_text_input(&self) -> Option<SettingKey> {
        let key = self.elements.focused_key().copied()?;
        self.elements.text_input(&key).map(|_| key)
    }
    fn start_editing(&mut self, key: SettingKey) {
        self.editing = Some(key);
        self.clear_row_hover();
        if let Some(input) = self.elements.text_input_mut(&key) {
            input.move_to_end();
        }
    }
    fn stop_editing(&mut self, keep_text: bool) {
        if let Some(key) = self.editing.take() {
            if !keep_text {
                self.force_sync_element(key);
            }
        }
    }
    fn stop_editing_if_focus_moved(&mut self) {
        if self.editing.is_some()
            && (self.focus != Focus::Content
                || self.elements.focused_key().copied() != self.editing)
        {
            self.stop_editing(false);
        }
    }
    pub fn handle_key(&mut self, key: KeyWithModifier) -> bool {
        let should_render = self.handle_key_inner(key);
        self.stop_editing_if_focus_moved();
        should_render
    }
    fn handle_key_inner(&mut self, key: KeyWithModifier) -> bool {
        if self.closing {
            close_self();
            return true;
        }
        if self.dialog.is_open() {
            let response = self.dialog.handle_key(&key);
            self.dialog_response(response);
            return true;
        }
        let embedded_keys_screen = self.showing_keys_screen() && self.focus == Focus::Content;
        let keys_capture = embedded_keys_screen
            && (self.keys_screen.is_capturing_keys() || self.keys_screen.dialog_is_open());
        if !keys_capture && self.editing.is_none() {
            self.notice = None;
            if is_ctrl_key(&key, 'a') {
                self.restore_theme_preview();
                self.request_save();
                return true;
            }
            if is_ctrl_key(&key, 'r') {
                self.restore_theme_preview();
                self.request_revert_all();
                return true;
            }
        }
        match self.focus {
            Focus::Menu => self.handle_menu_key(key),
            Focus::Search => self.handle_search_key(key),
            Focus::Content => {
                if self.showing_keys_screen() {
                    self.handle_keys_screen_key(key, keys_capture)
                } else {
                    self.handle_content_key(key)
                }
            },
        }
    }
    fn handle_menu_key(&mut self, key: KeyWithModifier) -> bool {
        if is_plain_key(&key, BareKey::Tab)
            || is_plain_key(&key, BareKey::Right)
            || is_plain_key(&key, BareKey::Enter)
        {
            if self.showing_keys_screen() || !self.rows.is_empty() {
                self.set_focus(Focus::Content);
            }
            return true;
        }
        if is_plain_key(&key, BareKey::Char('/')) {
            self.open_search();
            return true;
        }
        if is_plain_key(&key, BareKey::Esc) {
            if self.search_active {
                self.close_search();
            } else {
                self.begin_close();
            }
            return true;
        }
        if self.search_active {
            return false;
        }
        match self.menu.handle_key(&key) {
            UiResponse::Changed(_) => {
                self.select_category();
                true
            },
            UiResponse::NotHandled => false,
            _ => true,
        }
    }
    fn handle_search_key(&mut self, key: KeyWithModifier) -> bool {
        if is_plain_key(&key, BareKey::Down)
            || is_plain_key(&key, BareKey::Tab)
            || is_plain_key(&key, BareKey::Enter)
        {
            if !self.elements.is_empty() {
                self.elements.blur();
                self.set_focus(Focus::Content);
            }
            return true;
        }
        match self.search.handle_key(&key) {
            UiResponse::Changed(_) => {
                self.scroll.set_offset(0);
                self.rebuild_rows();
                true
            },
            UiResponse::Cancelled => {
                self.close_search();
                true
            },
            UiResponse::NotHandled => false,
            _ => true,
        }
    }
    fn handle_keys_screen_key(&mut self, key: KeyWithModifier, keys_capture: bool) -> bool {
        let moves_focus = is_plain_key(&key, BareKey::Tab) || is_shift_tab(&key);
        let should_render = self.keys_screen.handle_key(key);
        if moves_focus && !keys_capture && !should_render {
            self.set_focus(Focus::Menu);
            return true;
        }
        if take_close_request() {
            self.begin_close();
        } else if self.keys_screen.take_needs_refresh() {
            self.refresh();
        }
        should_render
    }
    fn focused_dropdown_open(&self) -> bool {
        self.elements
            .focused_key()
            .and_then(|k| self.elements.dropdown(k))
            .map(|d| d.is_open())
            .unwrap_or(false)
    }
    fn handle_read_only_key(&mut self, key: KeyWithModifier) -> bool {
        if is_plain_key(&key, BareKey::Down) {
            self.scroll.scroll_by(1);
        } else if is_plain_key(&key, BareKey::Up) {
            self.scroll.scroll_by(-1);
        } else if is_plain_key(&key, BareKey::Left)
            || is_plain_key(&key, BareKey::Tab)
            || is_shift_tab(&key)
        {
            self.set_focus(Focus::Menu);
        } else if is_plain_key(&key, BareKey::Char('/')) {
            self.open_search();
        } else if is_plain_key(&key, BareKey::Esc) {
            self.begin_close();
        } else {
            return self.scroll_key(&key).unwrap_or(false);
        }
        true
    }
    fn handle_content_key(&mut self, key: KeyWithModifier) -> bool {
        if self.elements.is_empty() && !self.search_active {
            return self.handle_read_only_key(key);
        }
        let moves_focus = is_plain_key(&key, BareKey::Tab) || is_shift_tab(&key);
        let mut pass_to_elements = true;
        if let Some(text_key) = self.focused_text_input() {
            if self.editing == Some(text_key) {
                if moves_focus {
                    self.stop_editing(false);
                } else {
                    let event = self.elements.handle_key(&key);
                    self.handle_focus_event(event, false);
                    return true;
                }
            } else if is_plain_key(&key, BareKey::Enter) {
                self.start_editing(text_key);
                return true;
            } else if !moves_focus {
                pass_to_elements = false;
            }
        }
        if pass_to_elements {
            let dropdown_was_open = self.focused_dropdown_open();
            let event = self.elements.handle_key(&key);
            let handled = event.is_handled();
            self.handle_focus_event(event, dropdown_was_open);
            if handled {
                self.follow_focus = true;
                return true;
            }
        }
        if is_plain_key(&key, BareKey::Down) {
            self.elements.focus_next();
            self.follow_focus = true;
        } else if is_plain_key(&key, BareKey::Up) {
            if !self.elements.focus_prev() {
                if self.search_active {
                    self.set_focus(Focus::Search);
                }
            }
            self.follow_focus = true;
        } else if is_plain_key(&key, BareKey::Left) || is_shift_tab(&key) {
            self.restore_theme_preview();
            if self.search_active {
                self.set_focus(Focus::Search);
            } else {
                self.set_focus(Focus::Menu);
            }
        } else if is_plain_key(&key, BareKey::Char('/')) {
            self.open_search();
        } else if is_plain_key(&key, BareKey::Char('r')) {
            self.revert_focused();
        } else if is_plain_key(&key, BareKey::Esc) {
            if self.search_active {
                self.close_search();
            } else {
                self.begin_close();
            }
        } else if let Some(handled) = self.scroll_key(&key) {
            return handled;
        } else {
            return false;
        }
        true
    }
    fn scroll_key(&mut self, key: &KeyWithModifier) -> Option<bool> {
        match self.scroll.handle_key(key) {
            UiResponse::NotHandled => None,
            _ => Some(true),
        }
    }
    fn handle_focus_event(&mut self, event: FocusEvent<SettingKey>, dropdown_was_open: bool) {
        match event {
            FocusEvent::Element { key, response } => {
                self.handle_element_response(key, response, dropdown_was_open)
            },
            FocusEvent::FocusChanged(_) => {
                self.restore_theme_preview();
                self.follow_focus = true;
            },
            FocusEvent::NotHandled => {},
        }
        self.update_theme_preview();
    }
    fn handle_element_response(
        &mut self,
        key: SettingKey,
        response: UiResponse,
        dropdown_was_open: bool,
    ) {
        match response {
            UiResponse::Changed(UiValue::Bool(on)) => self.set_value(key, &on.to_string()),
            UiResponse::Changed(UiValue::Choice { label, .. }) => {
                self.theme_preview.commit(key);
                if label == UNSET_CHOICE {
                    self.unset_value(key);
                } else {
                    self.set_value(key, option_value(&label));
                }
            },
            UiResponse::Changed(UiValue::Number(number)) => {
                self.set_value(key, &number.to_string())
            },
            UiResponse::Submitted(UiValue::Text(text)) => {
                if let SettingKind::Text(check) = describe(key).kind {
                    if let Err(error) = check_text(check, &text) {
                        if !text.is_empty() {
                            self.notice = Some(format!("{}: {}", describe(key).name, error));
                            return;
                        }
                    }
                }
                self.editing = None;
                if text.is_empty() {
                    self.unset_value(key);
                } else {
                    self.set_value(key, &text);
                }
                self.notice = Some(format!("{} applied", describe(key).name));
            },
            UiResponse::Cancelled => {
                if dropdown_was_open {
                    self.restore_theme_preview();
                } else if self.elements.text_input(&key).is_some() {
                    self.stop_editing(false);
                } else if self.search_active {
                    self.close_search();
                } else {
                    self.begin_close();
                }
            },
            _ => {},
        }
    }
    fn open_theme_highlight(&self) -> Option<(SettingKey, String, String)> {
        PREVIEWED_THEME_SETTINGS.iter().find_map(|key| {
            let dropdown = self.elements.dropdown(key).filter(|d| d.is_open())?;
            let highlighted = dropdown
                .highlighted_index()
                .and_then(|index| dropdown.options().get(index))?;
            let shown = dropdown.selected_value().unwrap_or(UNSET_CHOICE);
            Some((
                *key,
                option_value(highlighted).to_owned(),
                option_value(shown).to_owned(),
            ))
        })
    }
    fn update_theme_preview(&mut self) {
        match self.open_theme_highlight() {
            Some((key, highlighted, shown_originally)) => {
                let state = self.setting(key).cloned();
                let actions = self.theme_preview.highlight(
                    key,
                    &highlighted,
                    &shown_originally,
                    state.as_ref(),
                    UNSET_CHOICE,
                );
                let applied_any = !actions.is_empty();
                for action in actions {
                    run_preview_action(action);
                }
                if applied_any {
                    self.refresh();
                }
            },
            None => self.restore_theme_preview(),
        }
    }
    fn restore_theme_preview(&mut self) {
        if self.theme_preview.active_key().is_some() {
            if let Some(action) = self.theme_preview.restore() {
                run_preview_action(action);
            }
            self.refresh();
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        let should_render = self.handle_mouse_inner(mouse);
        self.stop_editing_if_focus_moved();
        should_render
    }
    fn rows_accept_input(&self) -> bool {
        !self.closing && !self.dialog.is_open() && self.editing.is_none()
    }
    fn clear_row_hover(&mut self) {
        self.elements.handle_mouse(Mouse::Hover(-1, 0));
    }
    fn clear_all_hover(&mut self) {
        self.clear_row_hover();
        self.file_link_hovered = false;
        self.menu.handle_mouse(Mouse::Hover(-1, 0));
        self.search.handle_mouse(Mouse::Hover(-1, 0));
        self.scroll.handle_mouse(Mouse::Hover(-1, 0));
    }
    fn handle_mouse_inner(&mut self, mouse: Mouse) -> bool {
        if self.closing {
            return false;
        }
        if self.dialog.is_open() {
            let response = self.dialog.handle_mouse(mouse);
            self.dialog_response(response);
            return true;
        }
        if self.showing_keys_screen() {
            let on_menu = match mouse {
                Mouse::LeftClick(line, column) => self.menu.hit_test(line, column),
                _ => false,
            };
            if !on_menu || self.keys_screen.dialog_is_open() {
                if let Mouse::Hover(..) = mouse {
                    self.menu.handle_mouse(mouse);
                }
                let handled = self.keys_screen.handle_mouse(mouse);
                if handled {
                    if let Mouse::LeftClick(..) = mouse {
                        if self.focus != Focus::Content {
                            self.set_focus(Focus::Content);
                        }
                    }
                    if self.keys_screen.take_needs_refresh() {
                        self.refresh();
                    }
                    return true;
                }
            }
        }
        match mouse {
            Mouse::Hover(line, column) => {
                self.file_link_hovered = self.file_link_at(line, column);
            },
            Mouse::LeftClick(line, column) if self.file_link_at(line, column) => {
                self.stop_editing(false);
                self.open_config_file();
                return true;
            },
            _ => {},
        }
        match mouse {
            Mouse::Hover(..) => {
                self.menu.handle_mouse(mouse);
                self.scroll.handle_mouse(mouse);
                self.search.handle_mouse(mouse);
                if !self.rows_accept_input() {
                    self.clear_row_hover();
                    return true;
                }
                let dropdown_was_open = self.focused_dropdown_open();
                let event = self.elements.handle_mouse(mouse);
                self.handle_focus_event(event, dropdown_was_open);
                return true;
            },
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) => {
                if self.elements.has_open_overlay() {
                    self.elements.handle_mouse(mouse);
                    self.update_theme_preview();
                    return true;
                }
                if self.menu.handle_mouse(mouse).is_handled() {
                    return true;
                }
                return self.scroll.handle_mouse(mouse).is_handled();
            },
            Mouse::LeftClick(line, column) => {
                if !self.elements.has_open_overlay() {
                    if self.menu.hit_test(line, column) {
                        self.restore_theme_preview();
                        if self.search_active {
                            self.search_active = false;
                            self.search.clear();
                        }
                        self.menu.handle_mouse(mouse);
                        self.set_focus(Focus::Menu);
                        self.select_category();
                        return true;
                    }
                    if self.search.hit_test(line, column) {
                        self.open_search();
                        self.search.handle_mouse(mouse);
                        return true;
                    }
                }
                let dropdown_was_open = self.focused_dropdown_open();
                let event = self.elements.handle_mouse(mouse);
                if event.is_handled() {
                    if self.focus != Focus::Content {
                        self.focus = Focus::Content;
                        self.menu.set_focused(false);
                        self.search.set_focused(false);
                    }
                    self.handle_focus_event(event, dropdown_was_open);
                    return true;
                }
                return self.scroll.handle_mouse(mouse).is_handled();
            },
            _ => {},
        }
        let dropdown_was_open = self.focused_dropdown_open();
        let event = self.elements.handle_mouse(mouse);
        let handled = event.is_handled();
        self.handle_focus_event(event, dropdown_was_open);
        handled || self.scroll.handle_mouse(mouse).is_handled()
    }
    fn marker_for(&self, key: SettingKey) -> (String, bool, bool) {
        let setting = self.setting(key);
        let unsaved = setting.map(|s| s.is_unsaved()).unwrap_or(false);
        let set_in_file = setting.map(|s| s.set_in_file).unwrap_or(false);
        let mut marker = String::new();
        if unsaved {
            marker.push_str("● unsaved");
        } else if !set_in_file {
            marker.push_str("○ default");
        }
        if key.requires_restart() {
            if !marker.is_empty() {
                marker.push(' ');
            }
            marker.push_str("⟳ restart");
        }
        (marker, unsaved, !unsaved && !set_in_file)
    }
    fn layout_rows(&mut self) -> Vec<usize> {
        self.row_offsets.clear();
        let mut starts = vec![];
        let mut total = 0;
        for (index, row) in self.rows.iter().enumerate() {
            match row {
                Row::Setting(key) => {
                    starts.push(total);
                    self.row_offsets.insert(*key, (total, 1));
                    total += 1;
                },
                Row::Heading(_) => {
                    if index > 0 {
                        total += SECTION_PADDING;
                    }
                    starts.push(total);
                    total += 1;
                },
                Row::Line(_) => {
                    starts.push(total);
                    total += 1;
                },
            }
        }
        starts.push(total);
        starts
    }
    pub fn render(&mut self, rows: usize, cols: usize) {
        if (rows, cols) != self.last_size {
            self.last_size = (rows, cols);
            self.follow_focus = true;
        }
        if rows < MIN_ROWS || cols < MIN_COLS {
            let message = format!(
                "Pane too small for settings ({}x{} needed)",
                MIN_COLS, MIN_ROWS
            );
            print_text_with_coordinates(
                Text::new(truncate(&message, cols)).color_all(3),
                0,
                rows / 2,
                None,
                None,
            );
            return;
        }
        let menu_width = (self
            .menu
            .items()
            .iter()
            .map(|item| item.chars().count())
            .max()
            .unwrap_or(0)
            + MENU_PADDING)
            .min(cols / 3)
            .max(16);
        let ui_width = (menu_width + 2 + CONTENT_WIDTH).min(cols);
        let ui_height = (HEADER_ROWS + self.natural_body_height() + FOOTER_ROWS).min(rows);
        let x0 = cols.saturating_sub(ui_width) / 2;
        let y0 = rows.saturating_sub(ui_height) / 2;
        self.render_header(x0, y0, ui_width);
        let body_y = y0 + HEADER_ROWS;
        let footer_rows = if self.showing_keys_screen() {
            1
        } else {
            FOOTER_ROWS
        };
        let body_height = ui_height.saturating_sub(HEADER_ROWS + footer_rows);
        if self.search_active {
            self.menu.clear_area();
            self.search.set_match_count(None);
            self.search.render(x0, body_y, menu_width);
            let match_count = self.elements.len();
            let matches = match match_count {
                1 => "1 match".to_owned(),
                count => format!("{} matches", count),
            };
            print_text_with_coordinates(
                Text::new(truncate(&matches, menu_width)).dim_all(),
                x0 + 1,
                body_y + 1 + SECTION_PADDING,
                None,
                None,
            );
        } else {
            self.search.clear_area();
            self.menu.render(x0, body_y, menu_width, body_height);
            self.render_menu_unsaved_markers(x0, body_y, menu_width, body_height);
        }
        let content_x = x0 + menu_width + 2;
        let content_width = ui_width.saturating_sub(menu_width + 2);
        self.elements.clear_areas();
        let showing_keys_screen = self.showing_keys_screen();
        if showing_keys_screen {
            self.scroll.clear_area();
            let keys_rows = body_height.saturating_sub(1);
            self.keys_screen
                .render(content_x, body_y, content_width, keys_rows);
        } else {
            self.render_content(content_x, body_y, content_width, body_height);
        }
        self.render_footer(x0, y0 + ui_height, ui_width);
        self.elements.render_overlays(rows, cols);
        if showing_keys_screen {
            self.keys_screen.render_overlays(rows, cols);
        }
        self.dialog.render_centered(rows, cols);
    }
    fn render_menu_unsaved_markers(&self, x: usize, y: usize, width: usize, height: usize) {
        let items = self.menu.items();
        let has_indicators = items.len() > height;
        let first_line = y + if has_indicators { 1 } else { 0 };
        let visible_rows = if has_indicators {
            height.saturating_sub(2)
        } else {
            height
        };
        let offset = self.menu.scroll_offset();
        for (index, item) in items.iter().enumerate().skip(offset).take(visible_rows) {
            let Some(title) = item.strip_suffix(" ●") else {
                continue;
            };
            let column = x + 1 + title.chars().count() + 1;
            if column + 1 >= x + width {
                continue;
            }
            let mut marker = Text::new("●").color_all(1);
            if index == self.menu.selected_index() || self.menu.hovered_index() == Some(index) {
                marker = marker.selected();
            }
            print_text_with_coordinates(marker, column, first_line + index - offset, None, None);
        }
    }
    fn natural_body_height(&self) -> usize {
        let tallest_category = CATEGORIES
            .iter()
            .filter(|category| {
                !category.is_keys_screen() && **category != Category::PluginsEnvironmentAndMenu
            })
            .map(|category| {
                let keys = settings_in(*category);
                let mut sections: Vec<&str> = keys.iter().map(|key| section(*key)).collect();
                sections.dedup();
                let headings = sections.iter().filter(|name| !name.is_empty()).count();
                keys.len() + headings * (1 + SECTION_PADDING)
            })
            .max()
            .unwrap_or(0);
        tallest_category
            .max(self.menu.items().len())
            .max(KEYS_SCREEN_ROWS)
    }
    fn render_header(&mut self, x: usize, y: usize, cols: usize) {
        let unsaved = self.snapshot.unsaved_count();
        let count = format!(
            "{} unsaved change{}",
            unsaved,
            if unsaved == 1 { "" } else { "s" }
        );
        let mut header = key_hints(
            &format!("{} · ", count),
            &[("<Ctrl a>", "save"), ("<Ctrl r>", "revert all")],
            cols,
        );
        if unsaved > 0 {
            header = header.color_range(1, ..count.chars().count().min(cols));
        }
        print_text_with_coordinates(header, x, y + 2, None, None);
        match self.snapshot.config_file_path.clone() {
            Some(path) => {
                print_text_with_coordinates(Text::new(FILE_PREFIX), x, y, None, None);
                let link_x = x + FILE_PREFIX.chars().count();
                let link = truncate(&path, cols.saturating_sub(FILE_PREFIX.chars().count()));
                self.file_link_area = Some(Rect::new(link_x, y, link.chars().count(), 1));
                self.render_file_link(&link, link_x, y);
            },
            None => {
                self.file_link_area = None;
                print_text_with_coordinates(
                    Text::new(truncate("File: none (settings cannot be saved)", cols)),
                    x,
                    y,
                    None,
                    None,
                );
            },
        }
    }
    fn render_file_link(&self, link: &str, x: usize, y: usize) {
        let color = self
            .latest_mode_info
            .as_ref()
            .map(|mode_info| mode_info.style.colors.text_unselected.emphasis_2);
        print_link(link, x, y, color, self.file_link_hovered);
    }
    fn open_config_file(&mut self) {
        if let Some(path) = self.snapshot.config_file_path.clone() {
            self.restore_theme_preview();
            open_file_floating(FileToOpen::new(path), None, BTreeMap::new());
        }
    }
    fn file_link_at(&self, line: isize, column: usize) -> bool {
        self.file_link_area
            .map(|area| area.contains(line, column))
            .unwrap_or(false)
    }
    fn focused_description(&self) -> Option<String> {
        if self.focus != Focus::Content || self.showing_keys_screen() {
            return None;
        }
        let key = self.elements.focused_key()?;
        let info = describe(*key);
        let scope = match info.scope {
            Scope::OnlyYou => "applies only to you",
            Scope::Everyone => "applies to everyone in this session",
        };
        let restart = if key.requires_restart() {
            ", takes effect after a restart"
        } else {
            ""
        };
        Some(format!(
            "{} (default: {}; {}{})",
            info.description, info.default, scope, restart
        ))
    }
    fn render_footer(&self, x: usize, bottom: usize, cols: usize) {
        let y = bottom.saturating_sub(1);
        let description_y = bottom.saturating_sub(3);
        if self.showing_keys_screen() {
            if let Some(notice) = self.notice.as_ref().or(self.keys_screen.notice()) {
                print_text_with_coordinates(
                    Text::new(truncate(notice, cols)).color_all(3),
                    x,
                    y,
                    None,
                    None,
                );
                return;
            }
        } else if let Some(notice) = &self.notice {
            print_text_with_coordinates(
                Text::new(truncate(notice, cols)).color_all(3),
                x,
                description_y,
                None,
                None,
            );
        } else if let Some(description) = self.focused_description() {
            print_text_with_coordinates(
                Text::new(truncate(&description, cols)).color_all(0),
                x,
                description_y,
                None,
                None,
            );
        }
        let hints: &[(&str, &str)] = match self.focus {
            Focus::Menu => &[
                ("<↓↑>", "category"),
                ("<Tab>", "settings"),
                ("</>", "search"),
                ("<Esc>", "close"),
            ],
            Focus::Search => &[("<↓>", "results"), ("<Esc>", "end search")],
            Focus::Content
                if self.showing_keys_screen() && self.keys_screen.is_editing_folder() =>
            {
                &[("<Enter>", "apply"), ("<Esc>", "cancel")]
            },
            Focus::Content if self.showing_keys_screen() && self.keys_screen.folder_is_focused() => {
                &[("<Tab/↓↑>", "move"), ("<Enter>", "edit"), ("<Esc>", "close")]
            },
            Focus::Content if self.showing_keys_screen() => &[
                ("<Tab/↓↑>", "move"),
                ("<Space>", "change"),
                ("<Esc>", "close"),
            ],
            Focus::Content if self.editing.is_some() => {
                &[("<Enter>", "apply"), ("<Esc>", "cancel")]
            },
            Focus::Content if self.focused_text_input().is_some() => &[
                ("<↓↑>", "move"),
                ("<Enter>", "edit"),
                ("<r>", "revert"),
                ("<←>", "categories"),
                ("</>", "search"),
                ("<Esc>", "close"),
            ],
            Focus::Content => &[
                ("<↓↑>", "move"),
                ("<Space>", "change"),
                ("<r>", "revert"),
                ("<←>", "categories"),
                ("</>", "search"),
                ("<Esc>", "close"),
            ],
        };
        print_text_with_coordinates(key_hints("Help: ", hints, cols), x, y, None, None);
    }
    fn render_content(&mut self, x: usize, y: usize, width: usize, height: usize) {
        let starts = self.layout_rows();
        let total = starts.last().copied().unwrap_or(0);
        self.scroll.set_total_rows(total);
        self.scroll.layout(x, y, width, height);
        if self.follow_focus {
            if let Some((start, row_height)) = self
                .elements
                .focused_key()
                .and_then(|key| self.row_offsets.get(key))
                .copied()
            {
                let heading_above = start > 0
                    && self.rows.iter().zip(starts.iter()).any(|(row, row_start)| {
                        matches!(row, Row::Heading(_)) && *row_start + 1 == start
                    });
                if heading_above {
                    self.scroll.ensure_range_visible(start - 1, row_height + 1);
                } else {
                    self.scroll.ensure_range_visible(start, row_height);
                }
            }
            self.follow_focus = false;
        }
        let content_width = self.scroll.content_width().saturating_sub(1);
        let value_width = self.value_width();
        let rows = self.rows.clone();
        for (row, start) in rows.into_iter().zip(starts.into_iter()) {
            match row {
                Row::Setting(key) => {
                    self.render_setting_row(key, start, x, content_width, value_width)
                },
                Row::Heading(title) => {
                    if let Some(screen_y) = self.scroll.screen_row(start) {
                        print_text_with_coordinates(
                            Text::new(truncate(&title, content_width)).color_all(2),
                            x,
                            screen_y,
                            None,
                            None,
                        );
                    }
                },
                Row::Line(line) => {
                    if let Some(screen_y) = self.scroll.screen_row(start) {
                        print_text_with_coordinates(
                            Text::new(truncate(&line, content_width)),
                            x,
                            screen_y,
                            None,
                            None,
                        );
                    }
                },
            }
        }
        if self.rows.is_empty() {
            print_text_with_coordinates(Text::new("No settings match").dim_all(), x, y, None, None);
        }
        self.scroll.render_indicators();
    }
    fn render_setting_row(
        &mut self,
        key: SettingKey,
        start: usize,
        x: usize,
        width: usize,
        value_width: usize,
    ) {
        let Some(screen_y) = self.scroll.screen_row(start) else {
            return;
        };
        let (marker, unsaved, is_default) = self.marker_for(key);
        let label_width = self.label_width();
        let marker_width = marker.chars().count();
        let value_width = value_width
            .min(width.saturating_sub(label_width + marker_width + 2))
            .max(MIN_DROPDOWN_WIDTH);
        let element_width = label_width + value_width;
        if let Some(element) = self.elements.get_mut(&key) {
            match element {
                Element::Toggle(toggle) => {
                    toggle.set_field_width(value_width);
                    toggle.render(x, screen_y)
                },
                Element::Dropdown(dropdown) => dropdown.render(x, screen_y, element_width),
                Element::TextInput(input) => {
                    input.set_show_cursor(self.editing == Some(key));
                    input.render(x, screen_y, element_width)
                },
                Element::NumberStepper(stepper) => {
                    stepper.set_field_width(value_width);
                    stepper.render(x, screen_y)
                },
                _ => {},
            }
        }
        if !marker.is_empty() {
            let marker_column = x + element_width + 2;
            let marker_x = if marker_column + marker_width <= x + width {
                marker_column
            } else {
                x + width.saturating_sub(marker_width)
            };
            let mut marker_text = Text::new(&marker);
            if unsaved {
                let unsaved_len = "● unsaved".chars().count();
                marker_text = marker_text.color_range(1, ..unsaved_len);
            } else if is_default {
                let default_len = "○ default".chars().count();
                marker_text = marker_text.dim_range(..default_len);
            }
            print_text_with_coordinates(marker_text, marker_x, screen_y, None, None);
        }
    }
}
