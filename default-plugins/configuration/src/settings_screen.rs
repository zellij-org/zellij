use std::collections::BTreeMap;

use zellij_tile::prelude::*;

use crate::blocks_screen::BlockPage;
use crate::keybindings_screen::KeybindingsScreen;
use crate::keys_screen::KeysScreen;
use crate::page::{changed_by, note_group, outside_overlays, run_effects, Page, PageResponse};
use crate::settings::{
    check_text, describe, is_row_kind, kdl_for, mode_choice_label, option_value, section, settings_in,
    sort_for_display, Category, Scope, SettingInfo, SettingKind, CATEGORIES, MISSING_SUFFIX,
    INPUT_MODES, UNSET_CHOICE,
};
use crate::theme_preview::{PreviewAction, ThemePreview, PREVIEWED_THEME_SETTINGS};
use crate::themes_screen::ThemesScreen;
use crate::ui_components::{print_link, take_close_request};

pub const MIN_COLS: usize = 50;
pub const MIN_ROWS: usize = 12;
const HEADER_ROWS: usize = 4;
const MAX_LABEL_WIDTH: usize = 26;
const MAX_VALUE_WIDTH: usize = 28;
const MIN_DROPDOWN_WIDTH: usize = 10;
const TEXT_FIELD_WIDTH: usize = 24;
const MENU_PADDING: usize = 4;
const SECTION_PADDING: usize = 1;
const FOOTER_ROWS: usize = 4;
const CONTENT_WIDTH: usize = 80;
const FULL_SCREEN_WIDTH: usize = 140;
const KEYS_SCREEN_ROWS: usize = 18;
const CLOSE_NOTICE_SECONDS: f64 = 1.5;
const DEFAULT_THEME: &str = "default";
const FILE_PREFIX: &str = "File: ";
const WINDOW_TITLE: &str = "Configuration";
const KEYBINDINGS_MIN_ROWS: usize = 8;
const PAGE_CATEGORIES: [Category; 3] = [
    Category::Themes,
    Category::ContextMenu,
    Category::PluginsAndEnvironment,
];
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SaveState {
    Idle,
    AwaitingSave,
    AwaitingOverwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileChangeReaction {
    AskWhatToDo,
    Ignore,
    ShowReloaded,
}

fn reaction_to_file_change(state: SaveState) -> FileChangeReaction {
    match state {
        SaveState::AwaitingSave => FileChangeReaction::AskWhatToDo,
        SaveState::AwaitingOverwrite => FileChangeReaction::Ignore,
        SaveState::Idle => FileChangeReaction::ShowReloaded,
    }
}

const OVERWRITE_CHOICE: &str = "overwrite";
const RELOAD_CHOICE: &str = "reload";
const CANCEL_CHOICE: &str = "cancel";

const RELOADED_FROM_OUTSIDE: &str =
    "The config file was changed outside the settings and has been reloaded.";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Row {
    Setting(SettingKey),
    Heading(String),
    LabelledHeading(String, String),
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
    pending_prompts: Vec<(u64, DialogPurpose)>,
    awaiting_save: Option<Vec<SettingKey>>,
    awaiting_reload: bool,
    save_state: SaveState,
    notice: Option<String>,
    closing: bool,
    theme_preview: ThemePreview,
    theme_note_shown: bool,
    keys_screen: KeysScreen,
    keybindings_screen: KeybindingsScreen,
    bindings_focused: bool,
    bindings_area_y: Option<usize>,
    keys_summary_y: Option<usize>,
    bindings_hint_y: Option<usize>,
    blocks_screen: BlockPage,
    menu_screen: BlockPage,
    themes_screen: ThemesScreen,
    latest_mode_info: Option<ModeInfo>,
    follow_focus: bool,
    last_size: (usize, usize),
    editing: Option<SettingKey>,
    file_link_area: Option<Rect>,
    window_title: String,
    file_link_hovered: bool,
    last_mouse: Option<(isize, usize)>,
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
            pending_prompts: vec![],
            awaiting_save: None,
            awaiting_reload: false,
            save_state: SaveState::Idle,
            notice: None,
            closing: false,
            theme_preview: ThemePreview::default(),
            theme_note_shown: false,
            keys_screen: KeysScreen::new(false),
            keybindings_screen: KeybindingsScreen::default(),
            bindings_focused: false,
            bindings_area_y: None,
            keys_summary_y: None,
            bindings_hint_y: None,
            blocks_screen: BlockPage::plugins_and_environment(),
            menu_screen: BlockPage::right_click_menu(),
            themes_screen: ThemesScreen::default(),
            latest_mode_info: None,
            follow_focus: false,
            last_size: (0, 0),
            editing: None,
            file_link_area: None,
            window_title: WINDOW_TITLE.to_owned(),
            file_link_hovered: false,
            last_mouse: None,
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
    fn showing_page(&self) -> bool {
        !self.search_active && self.category().is_page()
    }
    fn page_for(&mut self, category: Category) -> Option<&mut dyn Page> {
        match category {
            Category::Themes => Some(&mut self.themes_screen),
            Category::ContextMenu => Some(&mut self.menu_screen),
            Category::PluginsAndEnvironment => Some(&mut self.blocks_screen),
            _ => None,
        }
    }
    fn page_ref(&self, category: Category) -> Option<&dyn Page> {
        match category {
            Category::Themes => Some(&self.themes_screen),
            Category::ContextMenu => Some(&self.menu_screen),
            Category::PluginsAndEnvironment => Some(&self.blocks_screen),
            _ => None,
        }
    }
    fn page(&mut self) -> Option<&mut dyn Page> {
        if self.search_active {
            return None;
        }
        let category = self.category();
        self.page_for(category)
    }
    fn page_captures_keys(&self) -> bool {
        if !self.showing_page() || self.focus != Focus::Content {
            return false;
        }
        self.page_ref(self.category())
            .map(|page| page.captures_keys())
            .unwrap_or(false)
    }
    fn showing_bindings(&self) -> bool {
        self.showing_keys_screen() && self.bindings_focused && self.focus == Focus::Content
    }
    fn page_hints(&self) -> Vec<(&'static str, &'static str)> {
        if self.showing_bindings() {
            return self.keybindings_screen.hints();
        }
        self.page_ref(self.category())
            .map(|page| page.hints())
            .unwrap_or_default()
    }
    fn collect_page_results(&mut self) -> bool {
        let Some(page) = self.page() else {
            return false;
        };
        let effects = page.take_effects();
        let notice = page.take_notice();
        let changed = notice.is_some() || !effects.is_empty();
        if notice.is_some() {
            self.notice = notice;
        }
        if run_effects(effects) {
            self.refresh();
        }
        changed
    }
    fn collect_keybindings_results(&mut self) -> bool {
        let effects = self.keybindings_screen.take_effects();
        let mut changed = !effects.is_empty();
        if let Some(notice) = self.keybindings_screen.take_notice() {
            self.notice = Some(notice);
            changed = true;
        }
        if run_effects(effects) {
            self.refresh();
        }
        changed
    }
    fn enter_bindings(&mut self) {
        self.bindings_focused = true;
        self.keybindings_screen.focus_top();
        self.set_focus(Focus::Content);
    }
    fn enter_preset(&mut self, from_below: bool) {
        self.bindings_focused = false;
        if from_below {
            self.keys_screen.focus_last();
        }
        self.set_focus(Focus::Content);
    }
    fn leave_pages(&mut self) {
        let mut effects = vec![];
        self.keybindings_screen.leave();
        effects.extend(self.keybindings_screen.take_effects());
        for category in PAGE_CATEGORIES {
            if let Some(page) = self.page_for(category) {
                page.leave();
                effects.extend(page.take_effects());
            }
        }
        if run_effects(effects) {
            self.refresh();
        }
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
        self.keybindings_screen.set_snapshot(&self.snapshot);
        self.blocks_screen.set_snapshot(&self.snapshot);
        self.menu_screen.set_snapshot(&self.snapshot);
        self.themes_screen.set_snapshot(&self.snapshot);
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
                .filter(|key| is_row_kind(describe(*key).kind))
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
    fn rebuild_rows(&mut self) {
        self.editing = None;
        self.theme_note_shown = self.theme_note_needed();
        let focused = self.elements.focused_key().copied();
        self.elements = FocusGroup::new().wrap(false);
        self.rows.clear();
        if !self.search_active && self.category() == Category::FoldersAndFiles {
            let folder = self
                .snapshot
                .config_file_path
                .as_ref()
                .and_then(|path| std::path::Path::new(path).parent())
                .map(|folder| folder.display().to_string())
                .unwrap_or_else(|| "none".to_owned());
            self.rows
                .push(Row::LabelledHeading("Config folder".to_owned(), folder));
        }
        if !self.showing_page() {
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
        let has_text = self.rows.iter().any(|row| match row {
            Row::Setting(key) => matches!(describe(*key).kind, SettingKind::Text(_)),
            _ => false,
        });
        let base = (widest + 4).clamp(MIN_DROPDOWN_WIDTH, MAX_VALUE_WIDTH);
        if has_text {
            base.max(TEXT_FIELD_WIDTH)
        } else {
            base
        }
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
            SettingKind::Choice(choices) if choices == INPUT_MODES => {
                choices.iter().map(|c| mode_choice_label(c)).collect()
            },
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
            SettingKind::Keybindings | SettingKind::Block => return None,
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
                SettingKind::Keybindings | SettingKind::Block => {},
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
        self.save_state = SaveState::AwaitingSave;
        save_config();
    }
    pub fn config_file_changed_since_read(&mut self) {
        match reaction_to_file_change(self.save_state) {
            FileChangeReaction::AskWhatToDo => self.open_changed_outside_dialog(),
            FileChangeReaction::Ignore => {},
            FileChangeReaction::ShowReloaded => {
                self.refresh();
                self.update_theme_preview();
                self.notice = Some(RELOADED_FROM_OUTSIDE.to_owned());
            },
        }
    }
    fn open_changed_outside_dialog(&mut self) {
        self.awaiting_save = None;
        let file = self
            .snapshot
            .config_file_path
            .clone()
            .unwrap_or_else(|| "The config file".to_owned());
        let request = PromptRequest::menu(vec![
            ChoiceItem::labeled(
                OVERWRITE_CHOICE,
                "Overwrite: apply your unsaved changes to the file as it is now",
            ),
            ChoiceItem::labeled(
                RELOAD_CHOICE,
                "Reload: read the file again and keep your changes unsaved",
            ),
            ChoiceItem::labeled(CANCEL_CHOICE, "Cancel"),
        ])
        .title(format!("{} changed outside Zellij", file));
        self.clear_all_hover();
        self.ask(request, DialogPurpose::ChangedOutside);
    }
    fn ask(&mut self, request: PromptRequest, purpose: DialogPurpose) {
        let request_id = prompt(request);
        self.pending_prompts.push((request_id, purpose));
    }
    fn open_revert_all_dialog(&mut self) {
        let count = self.snapshot.unsaved_count();
        let message = format!(
            "Revert {} unsaved change{} to the values in the config file?",
            count,
            if count == 1 { "" } else { "s" }
        );
        let request = PromptRequest::confirm(message)
            .title("Revert all?")
            .yes("Revert all")
            .no("Cancel");
        self.clear_all_hover();
        self.ask(request, DialogPurpose::RevertAll);
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
    fn answer(&mut self, purpose: DialogPurpose, result: &PromptResult) {
        let choice = match result {
            PromptResult::Answered(PromptValue::Choice(choice)) => Some(choice.as_str()),
            _ => None,
        };
        match purpose {
            DialogPurpose::ChangedOutside => match choice {
                Some(OVERWRITE_CHOICE) => {
                    self.awaiting_save = Some(self.snapshot.pending_restart_settings.clone());
                    self.awaiting_reload = false;
                    self.save_state = SaveState::AwaitingOverwrite;
                    overwrite_config_file();
                },
                Some(RELOAD_CHOICE) => {
                    self.awaiting_save = None;
                    self.awaiting_reload = true;
                    self.save_state = SaveState::Idle;
                    reload_config_file();
                },
                _ => {
                    self.save_state = SaveState::Idle;
                    self.notice = Some("Not saved".to_owned());
                },
            },
            DialogPurpose::RevertAll => {
                if *result == PromptResult::Confirmed(true) {
                    revert_config(None);
                    self.refresh();
                    let keys = self.elements.keys();
                    for key in keys {
                        self.force_sync_element(key);
                    }
                    self.notice = Some("All unsaved changes reverted".to_owned());
                }
            },
        }
    }
    pub fn prompt_result(&mut self, request_id: u64, result: PromptResult) -> bool {
        if let Some(index) = self
            .pending_prompts
            .iter()
            .position(|(pending_id, _)| *pending_id == request_id)
        {
            let (_, purpose) = self.pending_prompts.remove(index);
            self.answer(purpose, &result);
            return true;
        }
        if self.keybindings_screen.prompt_result(request_id, &result) {
            self.collect_keybindings_results();
            return true;
        }
        for category in PAGE_CATEGORIES {
            let Some(page) = self.page_for(category) else {
                continue;
            };
            if page.prompt_result(request_id, &result) {
                let effects = page.take_effects();
                let notice = page.take_notice();
                if notice.is_some() {
                    self.notice = notice;
                }
                if run_effects(effects) {
                    self.refresh();
                }
                return true;
            }
        }
        false
    }
    pub fn begin_close(&mut self) {
        self.restore_theme_preview();
        self.leave_pages();
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
        self.save_state = SaveState::Idle;
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
        self.save_state = SaveState::Idle;
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
        self.leave_pages();
    }
    pub fn hidden(&mut self) {
        self.restore_theme_preview();
        self.leave_pages();
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
        self.keybindings_screen.set_mode_info(&mode_info);
        self.themes_screen.set_mode_info(&mode_info);
        self.latest_mode_info = Some(mode_info);
        if self.theme_preview.active_key().is_none() {
            self.refresh();
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        if self.closing {
            close_self();
        }
        let keys_screen_changed =
            self.keys_screen.handle_timer() | self.keybindings_screen.handle_timer();
        let mut pages_changed = false;
        for category in PAGE_CATEGORIES {
            if let Some(page) = self.page_for(category) {
                pages_changed |= page.handle_timer();
            }
        }
        self.elements.handle_timer() || keys_screen_changed || pages_changed
    }
    fn set_focus(&mut self, focus: Focus) {
        self.focus = focus;
        self.menu.set_focused(focus == Focus::Menu);
        self.search.set_focused(focus == Focus::Search);
        if focus == Focus::Menu {
            self.bindings_focused = false;
        }
        let keys_content = focus == Focus::Content && self.showing_keys_screen();
        self.keys_screen
            .set_focused(keys_content && !self.bindings_focused);
        self.keybindings_screen
            .set_focused(keys_content && self.bindings_focused);
        let page_focused = focus == Focus::Content && self.showing_page();
        for category in PAGE_CATEGORIES {
            let focused = page_focused && self.category() == category;
            if let Some(page) = self.page_for(category) {
                page.set_focused(focused);
            }
        }
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
        self.leave_pages();
        self.bindings_focused = false;
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
        let embedded_keys_screen = self.showing_keys_screen() && self.focus == Focus::Content;
        let keys_capture = (embedded_keys_screen
            && if self.bindings_focused {
                self.keybindings_screen.captures_keys()
            } else {
                self.keys_screen.is_capturing_keys() || self.keys_screen.dialog_is_open()
            })
            || self.page_captures_keys();
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
        if self.showing_keys_screen()
            && !keys_capture
            && self.editing.is_none()
            && self.focus != Focus::Search
            && is_plain_key(&key, BareKey::Char('/'))
        {
            self.bindings_focused = true;
            self.keybindings_screen.focus_top();
            self.set_focus(Focus::Content);
            return true;
        }
        match self.focus {
            Focus::Menu => self.handle_menu_key(key),
            Focus::Search => self.handle_search_key(key),
            Focus::Content => {
                if self.showing_keys_screen() && self.bindings_focused {
                    self.handle_bindings_key(key)
                } else if self.showing_keys_screen() {
                    self.handle_keys_screen_key(key, keys_capture)
                } else if self.showing_page() {
                    self.handle_page_key(key)
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
            if self.showing_keys_screen() || self.showing_page() || !self.rows.is_empty() {
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
        let moves_down = is_plain_key(&key, BareKey::Tab) || is_plain_key(&key, BareKey::Down);
        let moves_back = is_shift_tab(&key);
        let should_render = self.keys_screen.handle_key(key);
        if !keys_capture && !should_render {
            if moves_down {
                self.enter_bindings();
                return true;
            }
            if moves_back {
                self.set_focus(Focus::Menu);
                return true;
            }
        }
        if take_close_request() {
            self.begin_close();
        } else if self.keys_screen.take_needs_refresh() {
            self.refresh();
        }
        should_render
    }
    fn handle_bindings_key(&mut self, key: KeyWithModifier) -> bool {
        let response = self.keybindings_screen.handle_key(&key);
        self.collect_keybindings_results();
        match response {
            PageResponse::Handled => true,
            PageResponse::LeaveToMenu => {
                self.set_focus(Focus::Menu);
                true
            },
            PageResponse::LeaveUp => {
                self.enter_preset(true);
                true
            },
            PageResponse::Close => {
                self.begin_close();
                true
            },
            PageResponse::NotHandled => false,
        }
    }
    fn handle_page_key(&mut self, key: KeyWithModifier) -> bool {
        let response = match self.page() {
            Some(page) => page.handle_key(&key),
            None => return false,
        };
        self.collect_page_results();
        match response {
            PageResponse::Handled => true,
            PageResponse::LeaveToMenu | PageResponse::LeaveUp => {
                self.set_focus(Focus::Menu);
                true
            },
            PageResponse::Close => {
                self.begin_close();
                true
            },
            PageResponse::NotHandled => {
                if is_plain_key(&key, BareKey::Char('/')) {
                    self.open_search();
                    return true;
                }
                false
            },
        }
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
    fn handle_focus_event(
        &mut self,
        event: FocusEvent<SettingKey>,
        dropdown_was_open: bool,
    ) -> bool {
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
        self.update_theme_preview()
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
    fn update_theme_preview(&mut self) -> bool {
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
                applied_any
            },
            None => self.restore_theme_preview(),
        }
    }
    fn restore_theme_preview(&mut self) -> bool {
        if self.theme_preview.active_key().is_none() {
            return false;
        }
        if let Some(action) = self.theme_preview.restore() {
            run_preview_action(action);
        }
        self.refresh();
        true
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> bool {
        let should_render = self.handle_mouse_inner(mouse);
        self.stop_editing_if_focus_moved();
        should_render
    }
    fn rows_accept_input(&self) -> bool {
        !self.closing && self.editing.is_none()
    }
    fn clear_row_hover(&mut self) -> bool {
        self.elements.handle_mouse(Mouse::Hover(-1, 0)).is_handled()
    }
    fn hover_menu(&mut self, mouse: Mouse) -> bool {
        changed_by(&mut self.menu, |menu| menu.handle_mouse(mouse)).1
    }
    fn open_dropdown_state(&self) -> Option<String> {
        self.elements.keys().into_iter().find_map(|key| {
            self.elements
                .dropdown(&key)
                .filter(|dropdown| dropdown.is_open())
                .map(|dropdown| format!("{:?}", dropdown))
        })
    }
    fn hover_rows(&mut self, mouse: Mouse) -> bool {
        if !self.rows_accept_input() {
            return self.clear_row_hover();
        }
        let dropdown_was_open = self.focused_dropdown_open();
        let dropdown_before = self.open_dropdown_state();
        let event = self.elements.handle_mouse(mouse);
        let rows_changed = match dropdown_before {
            Some(before) => self.open_dropdown_state() != Some(before),
            None => event.is_handled(),
        };
        let preview_changed = self.handle_focus_event(event, dropdown_was_open);
        rows_changed || preview_changed
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
        match mouse {
            Mouse::Hover(line, column) | Mouse::LeftClick(line, column) => {
                self.last_mouse = Some((line, column));
            },
            _ => {},
        }
        let mouse = match mouse {
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) => {
                if let Some((line, column)) = self.last_mouse {
                    self.menu.handle_mouse(Mouse::Hover(line, column));
                }
                mouse
            },
            _ => mouse,
        };
        let link_changed = match mouse {
            Mouse::Hover(line, column) => {
                let hovered =
                    self.file_link_at(line, column) && !crate::page::under_overlay(line, column);
                let changed = hovered != self.file_link_hovered;
                self.file_link_hovered = hovered;
                changed
            },
            Mouse::LeftClick(line, column) if self.file_link_at(line, column) => {
                self.stop_editing(false);
                self.open_config_file();
                return true;
            },
            _ => false,
        };
        let changed = self.route_mouse(mouse);
        changed || link_changed
    }
    fn route_mouse(&mut self, mouse: Mouse) -> bool {
        if self.showing_page() {
            let page_busy = self.page_captures_keys();
            match mouse {
                Mouse::LeftClick(line, column)
                    if !page_busy && self.menu.hit_test(line, column) => {},
                Mouse::Hover(..) => {
                    let menu_changed = self.hover_menu(mouse);
                    let page_changed = match self.page() {
                        Some(page) => page.handle_mouse(mouse) == PageResponse::Handled,
                        None => false,
                    };
                    let results_changed = self.collect_page_results();
                    return menu_changed || page_changed || results_changed;
                },
                Mouse::ScrollUp(_) | Mouse::ScrollDown(_)
                    if !page_busy && self.menu.handle_mouse(mouse).is_handled() =>
                {
                    return true;
                },
                _ => {
                    let response = match self.page() {
                        Some(page) => page.handle_mouse(mouse),
                        None => PageResponse::NotHandled,
                    };
                    self.collect_page_results();
                    if response == PageResponse::NotHandled {
                        return false;
                    }
                    if matches!(mouse, Mouse::LeftClick(..)) && self.focus != Focus::Content {
                        self.set_focus(Focus::Content);
                    }
                    return true;
                },
            }
        }
        if self.showing_keys_screen() {
            if let Some(handled) = self.handle_keys_category_mouse(mouse) {
                return handled;
            }
        }
        if self.showing_keys_screen() {
            let on_menu = match mouse {
                Mouse::LeftClick(line, column) => self.menu.hit_test(line, column),
                _ => false,
            };
            if !on_menu || self.keys_screen.dialog_is_open() {
                let handled = self.keys_screen.handle_mouse(mouse);
                if handled {
                    if let Mouse::Hover(..) = mouse {
                        self.hover_menu(mouse);
                    }
                    if let Mouse::LeftClick(..) = mouse {
                        if self.focus != Focus::Content || self.bindings_focused {
                            self.bindings_focused = false;
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
            Mouse::Hover(..) => {
                let menu_changed = self.hover_menu(mouse);
                let scroll_changed =
                    changed_by(&mut self.scroll, |scroll| scroll.handle_mouse(mouse)).1;
                let search_changed = self.search.handle_mouse(mouse).is_handled();
                let rows_changed = self.hover_rows(mouse);
                return menu_changed || scroll_changed || search_changed || rows_changed;
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
                let delta = match mouse {
                    Mouse::ScrollUp(lines) => -(lines.max(1) as isize),
                    Mouse::ScrollDown(lines) => lines.max(1) as isize,
                    _ => 0,
                };
                return self.scroll.scroll_by(delta);
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
                Row::Heading(_) | Row::LabelledHeading(..) => {
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
        let full_screen = self.showing_page()
            && self
                .page_ref(self.category())
                .map(|page| page.full_screen())
                .unwrap_or(false);
        let ui_width = if full_screen {
            cols.saturating_sub(2).min(FULL_SCREEN_WIDTH)
        } else {
            (menu_width + 2 + CONTENT_WIDTH).min(cols)
        };
        let ui_height = if full_screen {
            rows.saturating_sub(2)
        } else {
            (HEADER_ROWS + self.natural_body_height() + FOOTER_ROWS).min(rows)
        };
        let x0 = cols.saturating_sub(ui_width) / 2;
        let y0 = rows.saturating_sub(ui_height) / 2;
        self.update_window_title();
        self.render_header(x0, y0, ui_width);
        let body_y = y0 + HEADER_ROWS;
        let footer_rows = if self.showing_keys_screen() || self.showing_page() {
            1
        } else {
            FOOTER_ROWS
        };
        let body_height = ui_height.saturating_sub(HEADER_ROWS + footer_rows);
        if full_screen {
            self.menu.clear_area();
            self.search.clear_area();
        } else if self.search_active {
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
        let (content_x, content_width) = if full_screen {
            (x0, ui_width)
        } else {
            (x0 + menu_width + 2, ui_width.saturating_sub(menu_width + 2))
        };
        self.elements.clear_areas();
        let showing_keys_screen = self.showing_keys_screen();
        let showing_page = self.showing_page();
        if showing_keys_screen {
            self.scroll.clear_area();
            let keys_rows = body_height.saturating_sub(1);
            self.render_keys_category(content_x, body_y, content_width, keys_rows);
        } else if showing_page {
            self.scroll.clear_area();
            let page_rows = body_height.saturating_sub(1);
            if let Some(page) = self.page() {
                page.render(content_x, body_y, content_width, page_rows);
            }
        } else {
            self.render_content(content_x, body_y, content_width, body_height);
        }
        self.render_footer(x0, y0 + ui_height, ui_width);
        self.elements.render_overlays(rows, cols);
        note_group(&self.elements);
        if showing_keys_screen {
            self.keys_screen.render_overlays(rows, cols);
            self.keybindings_screen.render_overlays(rows, cols);
        }
        if showing_page {
            if let Some(page) = self.page() {
                page.render_overlays(rows, cols);
            }
        }
    }
    fn render_keys_category(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.bindings_area_y = None;
        self.keys_summary_y = None;
        self.bindings_hint_y = None;
        let natural = self.keys_screen.content_height(width);
        if natural + 2 + KEYBINDINGS_MIN_ROWS <= height {
            self.keys_screen.render(x, y, width, natural);
            let bindings_y = y + natural + 1;
            self.keybindings_screen
                .render(x, bindings_y, width, height - natural - 1);
            self.bindings_area_y = Some(bindings_y);
        } else if !self.bindings_focused {
            self.keybindings_screen.clear_areas();
            self.keys_screen.render(x, y, width, height);
            if natural + 2 <= height {
                let hint_y = y + natural + 1;
                print_text_with_coordinates(
                    Text::new(truncate(
                        "Keybindings ↓ (Tab past the last field, or click here)",
                        width,
                    ))
                    .color_range(2, ..11),
                    x,
                    hint_y,
                    None,
                    None,
                );
                self.bindings_hint_y = Some(hint_y);
            }
        } else {
            self.keys_screen.render_preset_row(x, y, width);
            let summary = format!(
                "{} · ↑ at the top or click above to change",
                self.keys_screen.summary()
            );
            print_text_with_coordinates(
                Text::new(truncate(&summary, width)).dim_all(),
                x,
                y + 1,
                None,
                None,
            );
            self.keys_summary_y = Some(y);
            self.keybindings_screen
                .render(x, y + 3, width, height.saturating_sub(3));
            self.bindings_area_y = Some(y + 3);
        }
    }
    fn handle_keys_category_mouse(&mut self, mouse: Mouse) -> Option<bool> {
        if self.keys_screen.dialog_is_open()
            || (self.keys_screen.has_open_overlay() && !matches!(mouse, Mouse::Hover(..)))
        {
            return None;
        }
        let on_menu = match mouse {
            Mouse::LeftClick(line, column) => self.menu.hit_test(line, column),
            _ => false,
        };
        let bindings_busy = self.bindings_focused && self.keybindings_screen.is_busy();
        if on_menu && !bindings_busy {
            return None;
        }
        if let Mouse::Hover(..) = mouse {
            let menu_changed = self.hover_menu(outside_overlays(mouse));
            let keys_changed = self.keys_screen.handle_mouse(mouse);
            let bindings_changed =
                self.keybindings_screen.handle_mouse(mouse) == PageResponse::Handled;
            let results_changed = self.collect_keybindings_results();
            return Some(menu_changed || keys_changed || bindings_changed || results_changed);
        }
        let line = match mouse {
            _ if self.keybindings_screen.is_dragging() => None,
            Mouse::LeftClick(line, _)
            | Mouse::RightClick(line, _)
            | Mouse::Hold(line, _)
            | Mouse::Release(line, _) => Some(line),
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) => self.last_mouse.map(|(line, _)| line),
            _ => None,
        };
        let at = |row: Option<usize>| match (row, line) {
            (Some(row), Some(line)) => line == row as isize,
            _ => false,
        };
        if !bindings_busy && matches!(mouse, Mouse::LeftClick(..)) {
            if at(self.keys_summary_y) || at(self.keys_summary_y.map(|row| row + 1)) {
                self.enter_preset(false);
                self.keys_screen.focus_preset();
                return Some(true);
            }
            if at(self.bindings_hint_y) {
                self.enter_bindings();
                return Some(true);
            }
        }
        let in_bindings = bindings_busy
            || self.keybindings_screen.is_dragging()
            || match (line, self.bindings_area_y) {
                (Some(line), Some(top)) => line >= top as isize,
                (None, _) => self.bindings_focused,
                _ => false,
            };
        if !in_bindings {
            return None;
        }
        let response = self.keybindings_screen.handle_mouse(mouse);
        self.collect_keybindings_results();
        if response == PageResponse::NotHandled {
            return Some(false);
        }
        if matches!(mouse, Mouse::LeftClick(..)) && !self.bindings_focused {
            self.bindings_focused = true;
            self.set_focus(Focus::Content);
        } else if matches!(mouse, Mouse::LeftClick(..)) && self.focus != Focus::Content {
            self.set_focus(Focus::Content);
        }
        Some(true)
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
            .filter(|category| category.has_rows())
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
        let page_hints = if self.showing_page() {
            self.page_ref(self.category())
                .and_then(|page| page.header_hints())
        } else {
            None
        };
        let hints =
            page_hints.unwrap_or_else(|| vec![("<Ctrl a>", "save"), ("<Ctrl r>", "revert all")]);
        let mut header = key_hints(&format!("{} · ", count), &hints, cols);
        if unsaved > 0 {
            header = header.color_range(1, ..count.chars().count().min(cols));
        }
        print_text_with_coordinates(header, x, y + 2, None, None);
        match self.header_file() {
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
    fn header_file(&self) -> Option<String> {
        let page_file = if self.showing_page() {
            self.page_ref(self.category())
                .and_then(|page| page.header_file())
        } else {
            None
        };
        page_file.or_else(|| self.snapshot.config_file_path.clone())
    }
    fn update_window_title(&mut self) {
        let title = if self.showing_page() {
            self.page_ref(self.category())
                .and_then(|page| page.window_title())
        } else {
            None
        }
        .unwrap_or_else(|| WINDOW_TITLE.to_owned());
        if title != self.window_title {
            rename_plugin_pane(get_plugin_ids().plugin_id, &title);
            self.window_title = title;
        }
    }
    fn open_config_file(&mut self) {
        if let Some(path) = self.header_file() {
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
        if self.focus != Focus::Content || self.showing_keys_screen() || self.showing_page() {
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
        if self.showing_page() || self.showing_bindings() {
            if let Some(notice) = self.notice.as_ref() {
                print_text_with_coordinates(
                    Text::new(truncate(notice, cols)).color_all(3),
                    x,
                    y,
                    None,
                    None,
                );
            } else if self.focus == Focus::Content {
                let hints = self.page_hints();
                print_text_with_coordinates(key_hints("Help: ", &hints, cols), x, y, None, None);
            } else {
                let hints = [
                    ("<↓↑>", "category"),
                    ("<Tab>", "open"),
                    ("</>", "search"),
                    ("<Esc>", "close"),
                ];
                print_text_with_coordinates(key_hints("Help: ", &hints, cols), x, y, None, None);
            }
            return;
        }
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
                Row::LabelledHeading(title, value) => {
                    if let Some(screen_y) = self.scroll.screen_row(start) {
                        let line = format!("{}  {}", title, value);
                        let title_length = title.chars().count().min(content_width);
                        print_text_with_coordinates(
                            Text::new(truncate(&line, content_width))
                                .color_range(2, ..title_length),
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
        let full_width_text = self.elements.text_input(&key).is_some()
            && !self.search_active
            && self.category() == Category::FoldersAndFiles;
        let widest_marker = self
            .rows
            .iter()
            .filter_map(|row| match row {
                Row::Setting(key) => Some(self.marker_for(*key).0.chars().count()),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let value_width = if full_width_text {
            width.saturating_sub(label_width + marker_width + 2)
        } else {
            value_width.min(width.saturating_sub(label_width + widest_marker + 2))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: usize = 200;
    const COLS: usize = 160;

    fn rendered_screen() -> SettingsScreen {
        let mut screen = SettingsScreen::default();
        screen.rebuild_rows();
        screen.render(ROWS, COLS);
        screen
    }

    fn menu_item_position(screen: &SettingsScreen) -> (isize, usize) {
        for line in 0..ROWS as isize {
            for column in 0..COLS {
                if screen.menu.item_at(line, column).is_some() {
                    return (line, column);
                }
            }
        }
        panic!("the menu was not laid out");
    }

    #[test]
    fn hovering_an_inert_area_does_not_redraw() {
        let mut screen = rendered_screen();
        assert!(!screen.handle_mouse(Mouse::Hover(0, 0)));
        assert!(!screen.handle_mouse(Mouse::Hover(1, 0)));
    }

    #[test]
    fn hovering_a_new_menu_item_redraws_once() {
        let mut screen = rendered_screen();
        let (line, column) = menu_item_position(&screen);
        assert!(screen.handle_mouse(Mouse::Hover(line, column)));
        assert!(screen.menu.hovered_index().is_some());
        assert!(!screen.handle_mouse(Mouse::Hover(line, column)));
        assert!(screen.handle_mouse(Mouse::Hover(0, 0)));
        assert!(screen.menu.hovered_index().is_none());
    }

    #[test]
    fn a_file_change_asks_only_while_an_own_save_is_pending() {
        assert_eq!(
            reaction_to_file_change(SaveState::Idle),
            FileChangeReaction::ShowReloaded
        );
        assert_eq!(
            reaction_to_file_change(SaveState::AwaitingSave),
            FileChangeReaction::AskWhatToDo
        );
        assert_eq!(
            reaction_to_file_change(SaveState::AwaitingOverwrite),
            FileChangeReaction::Ignore
        );
    }

    fn last_prompt(screen: &SettingsScreen) -> (u64, DialogPurpose) {
        *screen.pending_prompts.last().expect("a prompt was asked")
    }

    fn chosen(choice: &str) -> PromptResult {
        PromptResult::Answered(PromptValue::Choice(choice.to_owned()))
    }

    #[test]
    fn the_save_state_follows_saves_and_prompt_answers() {
        let mut screen = SettingsScreen::default();
        assert_eq!(screen.save_state, SaveState::Idle);
        screen.save();
        assert_eq!(screen.save_state, SaveState::AwaitingSave);
        screen.config_file_changed_since_read();
        let (request_id, purpose) = last_prompt(&screen);
        assert_eq!(purpose, DialogPurpose::ChangedOutside);
        assert!(screen.prompt_result(request_id, chosen(OVERWRITE_CHOICE)));
        assert_eq!(screen.save_state, SaveState::AwaitingOverwrite);
        assert!(screen.pending_prompts.is_empty());
        screen.config_file_changed_since_read();
        assert!(screen.pending_prompts.is_empty());
        assert_eq!(screen.save_state, SaveState::AwaitingOverwrite);
        screen.config_write_failed(None);
        assert_eq!(screen.save_state, SaveState::Idle);

        screen.save();
        screen.config_file_changed_since_read();
        let (request_id, _) = last_prompt(&screen);
        assert!(screen.prompt_result(request_id, PromptResult::Cancelled));
        assert_eq!(screen.save_state, SaveState::Idle);
        assert_eq!(screen.notice.as_deref(), Some("Not saved"));

        screen.save();
        screen.config_file_changed_since_read();
        let (request_id, _) = last_prompt(&screen);
        assert!(screen.prompt_result(request_id, chosen(RELOAD_CHOICE)));
        assert_eq!(screen.save_state, SaveState::Idle);
        assert!(screen.awaiting_reload);
        assert!(!screen.prompt_result(request_id, chosen(RELOAD_CHOICE)));
    }
}
