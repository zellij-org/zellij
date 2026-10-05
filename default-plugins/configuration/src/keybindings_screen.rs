use std::collections::BTreeSet;
use std::str::FromStr;

use zellij_tile::prelude::*;
use zellij_utils::input::config_blocks::keybind_change_kdl;

use crate::action_picker::{size_button, ActionPicker, PickerResponse, DELETE_WIDTH};
use crate::page::{
    DIM,
    BESIDE_SHORT_FIELD, SHORT_FIELD_WIDTH, SHORT_LABEL_WIDTH,
    action_argument_range, action_display_text, actions_summary, changed_by, confirm_removals, is_click, is_plain, is_shift_tab,
    markers, note_dropdown, note_overlay, outside_overlays, render_frame, print_dim, removal_confirmed,
    removal_prompt, truncate, typed, ColumnLayout, ColumnStyle, Effect, Page, PageResponse,
    RowLook, RowScroll,
};

const FORM_LABEL_WIDTH: usize = 9;
const MATCH_COLOR: usize = 1;
const KEY_GETTING_READY: &str = "getting ready…";
const FORM_DIALOG_MIN_WIDTH: usize = 30;
const FORM_DIALOG_WIDE_WIDTH: usize = 84;
const FORM_DIALOG_TALL_HEIGHT: usize = 24;
const EDITABLE_MODES: [InputMode; 9] = [
    InputMode::Normal,
    InputMode::Locked,
    InputMode::Pane,
    InputMode::Tab,
    InputMode::Resize,
    InputMode::Move,
    InputMode::Scroll,
    InputMode::Session,
    InputMode::Tmux,
];
const MORE_MODES: [InputMode; 4] = [
    InputMode::Search,
    InputMode::EnterSearch,
    InputMode::RenameTab,
    InputMode::RenamePane,
];

pub fn all_modes() -> Vec<InputMode> {
    EDITABLE_MODES
        .iter()
        .chain(MORE_MODES.iter())
        .copied()
        .collect()
}

pub fn mode_name(mode: InputMode) -> String {
    format!("{:?}", mode)
}

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

const TITLE: &str = "Keybindings";

#[derive(Debug, Clone, PartialEq, Eq)]
enum ListItem {
    Gap,
    Heading(String),
    Entry(usize),
}

const ALL_CATEGORIES: &str = "All";
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

fn group_by_category(in_mode: Vec<KeybindingEntry>) -> (Vec<KeybindingEntry>, Vec<ListItem>) {
    let mut entries = vec![];
    let mut items = vec![];
    for category in CATEGORY_ORDER {
        let members: Vec<&KeybindingEntry> = in_mode
            .iter()
            .filter(|entry| entry_category(entry) == category)
            .collect();
        if members.is_empty() {
            continue;
        }
        if !items.is_empty() {
            items.push(ListItem::Gap);
        }
        items.push(ListItem::Heading(category.to_owned()));
        for entry in members {
            items.push(ListItem::Entry(entries.len()));
            entries.push(entry.clone());
        }
    }
    (entries, items)
}

fn entry_columns(entry: &KeybindingEntry) -> Vec<String> {
    let key_text = entry.key.to_string();
    let actions = if entry.unbound {
        "(unbound)".to_owned()
    } else {
        actions_summary(&entry.actions)
    };
    vec![key_text, actions]
}

pub struct SearchMatch {
    key: Vec<usize>,
    actions: Vec<usize>,
    score: (usize, usize),
}

pub fn match_entry(query: &str, entry: &KeybindingEntry) -> Option<SearchMatch> {
    let key_text = entry.key.to_string();
    let key_length = key_text.chars().count();
    let columns = entry_columns(entry);
    let candidate = format!("{} {}", key_text, columns[1]);
    let indices = fuzzy_match_indices(query, &candidate)?;
    let first = indices.first().copied().unwrap_or(0);
    let last = indices.last().copied().unwrap_or(0);
    Some(SearchMatch {
        key: indices
            .iter()
            .copied()
            .filter(|index| *index < key_length)
            .collect(),
        actions: indices
            .iter()
            .copied()
            .filter(|index| *index > key_length)
            .map(|index| index - key_length - 1)
            .collect(),
        score: (last - first, first),
    })
}

fn entry_styles(
    entry: &KeybindingEntry,
    key_column_text: &str,
    key_column: usize,
    found: Option<&SearchMatch>,
) -> Vec<ColumnStyle> {
    let mut styles = vec![];
    let column_length = key_column_text.chars().count();
    let key_length = entry.key.to_string().chars().count();
    let key_start = column_length.saturating_sub(key_length);
    for index in 0..key_length {
        let matched = found.map(|m| m.key.contains(&index)).unwrap_or(false);
        let color = if matched { MATCH_COLOR } else { 3 };
        styles.push((key_column, color, key_start + index..key_start + index + 1));
    }
    if !entry.unbound {
        let mut offset = 0;
        for action in &entry.actions {
            let text = action_display_text(action);
            let length = text.chars().count();
            if let Some(range) = action_argument_range(&text) {
                styles.push((key_column + 1, 0, offset + range.start..offset + range.end));
            }
            offset += length + 2;
        }
    }
    if let Some(found) = found {
        for index in &found.actions {
            styles.push((key_column + 1, MATCH_COLOR, *index..*index + 1));
        }
    }
    styles
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Mode,
    Filter,
    List,
}

pub struct KeybindingsScreen {
    entries: Vec<KeybindingEntry>,
    selection: KeybindsSelectionSnapshot,
    mode_selector: Dropdown,
    filter: Dropdown,
    search: TextInput,
    focus: Focus,
    focused: bool,
    selected: usize,
    form: Option<BindingForm>,
    dialog: ConfirmDialog,
    dialog_purpose: Option<DialogPurpose>,
    pending_deletes: Vec<(u64, Vec<KeybindingEntry>)>,
    effects: Vec<Effect>,
    notice: Option<String>,
    client_mode: InputMode,
    latest_mode: InputMode,
    base_mode: InputMode,
    capture_return_mode: Option<InputMode>,
    scroll: RowScroll,
    items: Vec<ListItem>,
    marked: BTreeSet<(InputMode, KeyWithModifier)>,
    anchor: Option<usize>,
    mark_buttons: Vec<(Button, Option<String>)>,
}

impl Default for KeybindingsScreen {
    fn default() -> Self {
        let modes: Vec<String> = all_modes().into_iter().map(mode_name).collect();
        KeybindingsScreen {
            entries: vec![],
            selection: KeybindsSelectionSnapshot::default(),
            mode_selector: Dropdown::new(Text::new("Mode").color_all(0), modes)
                .label_width(SHORT_LABEL_WIDTH)
                .accent_brackets(),
            filter: Dropdown::new(
                Text::new("Filter").color_all(0),
                std::iter::once(ALL_CATEGORIES)
                    .chain(CATEGORY_ORDER.iter().copied())
                    .collect::<Vec<_>>(),
            )
            .label_width(SHORT_LABEL_WIDTH)
            .accent_brackets(),
            search: TextInput::empty()
                .placeholder("/ to search keys and actions")
                .search_mode(),
            focus: Focus::List,
            focused: false,
            selected: 0,
            form: None,
            dialog: ConfirmDialog::new("", ""),
            dialog_purpose: None,
            pending_deletes: vec![],
            effects: vec![],
            notice: None,
            client_mode: InputMode::Normal,
            latest_mode: InputMode::Normal,
            base_mode: InputMode::Normal,
            capture_return_mode: None,
            scroll: RowScroll::default(),
            items: vec![],
            marked: BTreeSet::new(),
            anchor: None,
            mark_buttons: vec![],
        }
    }
}

impl KeybindingsScreen {
    pub fn mode(&self) -> InputMode {
        all_modes()
            .get(self.mode_selector.selected_index())
            .copied()
            .unwrap_or(InputMode::Normal)
    }
    pub fn is_dragging(&self) -> bool {
        self.scroll.is_dragging()
    }
    pub fn set_entries(
        &mut self,
        entries: Vec<KeybindingEntry>,
        selection: KeybindsSelectionSnapshot,
    ) {
        self.entries = entries;
        self.selection = selection;
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }
    pub fn visible(&self) -> Vec<KeybindingEntry> {
        self.view().0
    }
    fn category_filter(&self) -> Option<&'static str> {
        let label = self.filter.selected_value()?;
        CATEGORY_ORDER
            .iter()
            .copied()
            .find(|category| *category == label)
    }
    fn sync_filter(&mut self) {
        if self.filter.is_open() {
            return;
        }
        let mode = self.mode();
        let candidates: Vec<KeybindingEntry> = self
            .entries
            .iter()
            .filter(|entry| entry.mode == mode)
            .cloned()
            .collect();
        let options: Vec<String> = std::iter::once(ALL_CATEGORIES)
            .chain(
                CATEGORY_ORDER
                    .iter()
                    .copied()
                    .filter(|category| candidates.iter().any(|e| entry_category(e) == *category)),
            )
            .map(|label| label.to_owned())
            .collect();
        if self.filter.options() != options.as_slice() {
            let current = self.filter.selected_value().map(|label| label.to_owned());
            let index = current
                .and_then(|current| options.iter().position(|option| *option == current))
                .unwrap_or(0);
            self.filter.set_options(options);
            self.filter.set_selected(index);
        }
    }
    fn query(&self) -> String {
        self.search.get_text().trim().to_lowercase()
    }
    fn searching(&self) -> bool {
        !self.query().is_empty()
    }
    fn view(&self) -> (Vec<KeybindingEntry>, Vec<ListItem>) {
        let query = self.query();
        if !query.is_empty() {
            let mut found: Vec<(KeybindingEntry, (usize, usize))> = self
                .entries
                .iter()
                .filter_map(|entry| {
                    match_entry(&query, entry).map(|found| (entry.clone(), found.score))
                })
                .collect();
            found.sort_by_key(|(_, score)| *score);
            let entries: Vec<KeybindingEntry> =
                found.into_iter().map(|(entry, _)| entry).collect();
            let items = (0..entries.len()).map(ListItem::Entry).collect();
            return (entries, items);
        }
        let filter = self.category_filter();
        let mode = self.mode();
        let in_mode: Vec<KeybindingEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.mode == mode
                    && filter
                        .map(|category| entry_category(entry) == category)
                        .unwrap_or(true)
            })
            .cloned()
            .collect();
        group_by_category(in_mode)
    }
    fn selected_entry(&self) -> Option<KeybindingEntry> {
        self.visible().get(self.selected).cloned()
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
        let mode = self.mode();
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
        let Some(entry) = self.selected_entry() else {
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
        let warnings = leader_warnings(&self.entries, &self.selection, entry.mode, &entry.key);
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
        if let Some(existing) = same_mode_binding(&self.entries, mode, &key, ignore.as_ref()) {
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
            .map(|form| leader_warnings(&self.entries, &self.selection, form.mode, &key))
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
    fn answer_delete(&mut self, request_id: u64, result: &PromptResult) -> bool {
        let Some(index) = self
            .pending_deletes
            .iter()
            .position(|(pending_id, _)| *pending_id == request_id)
        else {
            return false;
        };
        let (_, entries) = self.pending_deletes.remove(index);
        if removal_confirmed(result) {
            self.remove_now(entries);
        }
        true
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
    fn handle_list_mouse(&mut self, mouse: Mouse) -> PageResponse {
        let is_hover = matches!(mouse, Mouse::Hover(..));
        let mut hover_changed = false;
        if !self.mode_selector.is_open()
            && (self.filter.is_open() || matches!(mouse, Mouse::LeftClick(..) | Mouse::Hover(..)))
        {
            let (response, filter_changed) =
                changed_by(&mut self.filter, |filter| filter.handle_mouse(mouse));
            hover_changed |= filter_changed;
            if let UiResponse::Changed(_) = response {
                self.select_first();
                self.scroll.reset();
            }
            if response.is_handled() && !is_hover {
                self.focus = Focus::Filter;
                return PageResponse::Handled;
            }
            if self.filter.is_open() {
                return if !is_hover || filter_changed {
                    PageResponse::Handled
                } else {
                    PageResponse::NotHandled
                };
            }
        }
        if !self.mode_selector.is_open() {
            let mouse = outside_overlays(mouse);
            let mut activated = None;
            for (button, category) in self.mark_buttons.iter_mut() {
                let was_hovered = button.is_hovered();
                if matches!(button.handle_mouse(mouse), UiResponse::Activated) {
                    activated = Some(category.clone());
                }
                hover_changed |= was_hovered != button.is_hovered();
            }
            if let Some(category) = activated {
                self.toggle_group(category.as_deref());
                return PageResponse::Handled;
            }
        }
        let rows_hover = self.scroll.hover(&mouse);
        let search_response = self.search.handle_mouse(mouse);
        if is_hover && search_response.is_handled() {
            hover_changed = true;
        }
        if self.mode_selector.is_open() || matches!(mouse, Mouse::LeftClick(..) | Mouse::Hover(..))
        {
            let (response, selector_changed) =
                changed_by(&mut self.mode_selector, |selector| selector.handle_mouse(mouse));
            hover_changed |= selector_changed;
            if let UiResponse::Changed(_) = response {
                self.select_first();
                self.scroll.reset();
            }
            if response.is_handled() && !is_hover {
                self.focus = Focus::Mode;
                return PageResponse::Handled;
            }
        }
        if let Some(rows_changed) = rows_hover {
            return if rows_changed || hover_changed {
                PageResponse::Handled
            } else {
                PageResponse::NotHandled
            };
        }
        if let Mouse::Hold(line, column) = mouse {
            if !self.scroll.is_dragging() {
                let row =
                    self.scroll
                        .row_at(line, column)
                        .and_then(|row| match self.items.get(row) {
                            Some(ListItem::Entry(index)) => Some(*index),
                            _ => None,
                        });
                if let (Some(row), Some(anchor)) = (row, self.anchor) {
                    if row != anchor || !self.marked.is_empty() {
                        self.mark_range(anchor, row);
                        self.selected = row;
                        self.focus = Focus::List;
                    }
                    return PageResponse::Handled;
                }
            }
        }
        if let Some(changed) = self.scroll.handle_wheel(&mouse) {
            return if changed {
                PageResponse::Handled
            } else {
                PageResponse::NotHandled
            };
        }
        let Some((line, column)) = is_click(&mouse) else {
            return PageResponse::NotHandled;
        };
        if self.search.hit_test(line, column) {
            self.focus = Focus::Search;
            return PageResponse::Handled;
        }
        let entry_row =
            self.scroll
                .row_at(line, column)
                .and_then(|row| match self.items.get(row) {
                    Some(ListItem::Entry(index)) => Some(*index),
                    _ => None,
                });
        match entry_row {
            Some(row) => {
                self.click_entry(row);
                PageResponse::Handled
            },
            None => PageResponse::NotHandled,
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
    fn reset_selected(&mut self) {
        let targets = self.targets();
        if targets.len() > 1 {
            let keys: Vec<(InputMode, KeyWithModifier)> = targets
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.source,
                        KeybindingSource::User | KeybindingSource::Shared(_)
                    )
                })
                .map(|entry| (entry.mode, entry.key.clone()))
                .collect();
            let skipped = targets.len() - keys.len();
            if keys.is_empty() {
                self.notice = Some("None of the marked keys differ from the preset".to_owned());
                return;
            }
            self.notice = Some(if skipped > 0 {
                format!(
                    "Reset {} keys to the preset; {} already use it or come from the layout",
                    keys.len(),
                    skipped
                )
            } else {
                format!("Reset {} keys to the preset", keys.len())
            });
            self.effects.push(Effect::ResetKeys(keys));
            self.clear_marks();
            return;
        }
        let Some(entry) = targets.into_iter().next() else {
            return;
        };
        match entry.source {
            KeybindingSource::User | KeybindingSource::Shared(_) => {
                self.effects
                    .push(Effect::ResetKeys(vec![(entry.mode, entry.key.clone())]));
                self.notice = Some(match entry.preset_actions {
                    Some(_) => format!("{} is back to the preset's binding", entry.key),
                    None => format!(
                        "{} is no longer bound (the preset has no binding)",
                        entry.key
                    ),
                });
                self.clear_marks();
            },
            KeybindingSource::Preset => {
                self.notice = Some(format!("{} already uses the preset's binding", entry.key))
            },
            KeybindingSource::Layout => {
                self.notice = Some(format!(
                    "{} comes from the layout and cannot be changed here",
                    entry.key
                ))
            },
        }
    }
    fn remove_now(&mut self, entries: Vec<KeybindingEntry>) {
        let mut removed = 0;
        let mut last_error = None;
        for entry in &entries {
            match removal_effects(entry) {
                Ok(effects) => {
                    self.effects.extend(effects);
                    removed += 1;
                },
                Err(error) => last_error = Some(error),
            }
        }
        self.notice = match (removed, entries.as_slice(), last_error) {
            (0, _, Some(error)) => Some(error),
            (1, [entry], _) => Some(format!(
                "Deleted {} from {} mode",
                entry.key,
                mode_name(entry.mode)
            )),
            (removed, entries, _) if removed < entries.len() => Some(format!(
                "Deleted {} keys; {} could not be deleted",
                removed,
                entries.len() - removed
            )),
            (removed, _, _) => Some(format!("Deleted {} keys", removed)),
        };
        self.clear_marks();
    }
    fn request_delete(&mut self) {
        let targets: Vec<KeybindingEntry> = self.targets();
        let removable: Vec<KeybindingEntry> = targets
            .iter()
            .filter(|entry| removal_effects(entry).is_ok())
            .cloned()
            .collect();
        if removable.is_empty() {
            if let Some(entry) = targets.first() {
                if let Err(error) = removal_effects(entry) {
                    self.notice = Some(error);
                }
            }
            return;
        }
        if !confirm_removals() {
            self.remove_now(removable);
            return;
        }
        let question = match removable.as_slice() {
            [entry] => {
                let key = entry.key.to_string();
                let key_start = "Delete ".chars().count();
                let key_end = key_start + key.chars().count();
                Text::new(format!("Delete {} from {} mode?", key, mode_name(entry.mode)))
                    .color_range(3, key_start..key_end)
            },
            entries => {
                let skipped = targets.len() - entries.len();
                let mut question = format!("Delete {} keys?", entries.len());
                if skipped > 0 {
                    question.push_str(&format!(
                        " {} marked keys cannot be deleted and stay.",
                        skipped
                    ));
                }
                Text::new(question)
            },
        };
        let request_id = prompt(removal_prompt(question));
        self.pending_deletes.push((request_id, removable));
    }
    fn handle_list_key(&mut self, key: &KeyWithModifier) -> PageResponse {
        self.scroll.follow();
        let count = self.visible().len();
        let shift = key.has_modifiers(&[KeyModifier::Shift]);
        if shift && key.bare_key == BareKey::Down {
            self.extend_selection(true);
            return PageResponse::Handled;
        }
        if shift && key.bare_key == BareKey::Up {
            self.extend_selection(false);
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Char(' ')) {
            self.toggle_mark(self.selected);
            self.anchor = Some(self.selected);
            return PageResponse::Handled;
        }
        if typed(key, 'A') && key.bare_key == BareKey::Char('A') {
            self.toggle_group(None);
            return PageResponse::Handled;
        }
        if typed(key, 'C') && key.bare_key == BareKey::Char('C') {
            let category = self
                .visible()
                .get(self.selected)
                .map(|entry| entry_category(entry).to_owned());
            if let Some(category) = category {
                self.toggle_group(Some(&category));
            }
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Esc) && !self.marked.is_empty() {
            self.clear_marks();
            return PageResponse::Handled;
        }
        if is_plain(key, BareKey::Down) {
            if self.selected + 1 < count {
                self.selected += 1;
            }
        } else if is_plain(key, BareKey::Up) {
            if self.selected == 0 {
                self.focus = if self.searching() {
                    Focus::Search
                } else {
                    Focus::Filter
                };
            } else {
                self.selected -= 1;
            }
        } else if typed(key, '/') {
            self.focus = Focus::Search;
        } else if typed(key, 'a') {
            self.open_add();
        } else if is_plain(key, BareKey::Enter) {
            self.open_edit();
        } else if is_plain(key, BareKey::Delete) {
            self.request_delete();
        } else if typed(key, 'r') {
            self.reset_selected();
        } else if is_plain(key, BareKey::PageDown) {
            self.selected = (self.selected + 10).min(count.saturating_sub(1));
        } else if is_plain(key, BareKey::PageUp) {
            self.selected = self.selected.saturating_sub(10);
        } else if is_plain(key, BareKey::Esc) {
            if self.searching() {
                self.search.clear();
                self.select_first();
                self.scroll.reset();
            } else {
                return PageResponse::Close;
            }
        } else if is_plain(key, BareKey::Tab) || is_plain(key, BareKey::Left) || is_shift_tab(key) {
            return PageResponse::LeaveToMenu;
        } else {
            return PageResponse::NotHandled;
        }
        PageResponse::Handled
    }
}

impl KeybindingsScreen {
    fn select_first(&mut self) {
        self.selected = 0;
        self.clear_marks();
    }
    fn clear_marks(&mut self) {
        self.marked.clear();
        self.anchor = None;
    }
    fn is_marked(&self, entry: &KeybindingEntry) -> bool {
        self.marked.contains(&(entry.mode, entry.key.clone()))
    }
    fn toggle_mark(&mut self, index: usize) {
        if let Some(entry) = self.visible().get(index) {
            let id = (entry.mode, entry.key.clone());
            if !self.marked.remove(&id) {
                self.marked.insert(id);
            }
        }
    }
    fn mark_range(&mut self, from: usize, to: usize) {
        let entries = self.visible();
        self.marked.clear();
        for entry in entries
            .iter()
            .skip(from.min(to))
            .take(from.max(to) - from.min(to) + 1)
        {
            self.marked.insert((entry.mode, entry.key.clone()));
        }
    }
    fn targets(&self) -> Vec<KeybindingEntry> {
        if self.marked.is_empty() {
            return self.selected_entry().into_iter().collect();
        }
        self.entries
            .iter()
            .filter(|entry| self.is_marked(entry))
            .cloned()
            .collect()
    }
    fn group_indices(&self, category: Option<&str>) -> Vec<usize> {
        self.visible()
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                category
                    .map(|category| entry_category(entry) == category)
                    .unwrap_or(true)
            })
            .map(|(index, _)| index)
            .collect()
    }
    fn group_is_marked(&self, category: Option<&str>) -> bool {
        let entries = self.visible();
        let indices = self.group_indices(category);
        !indices.is_empty() && indices.iter().all(|index| self.is_marked(&entries[*index]))
    }
    fn toggle_group(&mut self, category: Option<&str>) {
        let entries = self.visible();
        let indices = self.group_indices(category);
        let unmark = self.group_is_marked(category);
        for index in &indices {
            let entry = &entries[*index];
            let id = (entry.mode, entry.key.clone());
            if unmark {
                self.marked.remove(&id);
            } else {
                self.marked.insert(id);
            }
        }
        if let Some(first) = indices.first() {
            if !unmark {
                self.selected = *first;
                self.anchor = Some(*first);
            }
        }
        if self.marked.is_empty() {
            self.anchor = None;
        }
        self.focus = Focus::List;
    }
    fn mark_label(&self, category: Option<&str>) -> &'static str {
        if self.group_is_marked(category) {
            "Unmark all"
        } else {
            "Mark all"
        }
    }
    fn print_mark_button(
        &mut self,
        previous: &mut Vec<(Button, Option<String>)>,
        category: Option<String>,
        x: usize,
        y: usize,
        max_x: usize,
    ) {
        let label = self.mark_label(category.as_deref());
        let mut button = match previous.iter().position(|(_, c)| *c == category) {
            Some(index) => previous.remove(index).0,
            None => Button::new(label).accent_brackets(),
        };
        button.set_label(label);
        if x + button.natural_width() > max_x {
            return;
        }
        button.render(x, y);
        self.mark_buttons.push((button, category));
    }
    fn click_entry(&mut self, row: usize) {
        let modifiers = mouse_modifiers();
        if modifiers.contains(&KeyModifier::Shift) {
            let anchor = self.anchor.unwrap_or(self.selected);
            self.mark_range(anchor, row);
            self.anchor = Some(anchor);
            self.selected = row;
            self.focus = Focus::List;
            return;
        }
        if modifiers.contains(&KeyModifier::Ctrl) {
            if self.marked.is_empty() && self.focus == Focus::List && self.focused {
                self.toggle_mark(self.selected);
            }
            self.toggle_mark(row);
            self.anchor = Some(row);
            self.selected = row;
            self.focus = Focus::List;
            return;
        }
        let had_marks = !self.marked.is_empty();
        self.clear_marks();
        self.anchor = Some(row);
        if self.focused && self.focus == Focus::List && row == self.selected && !had_marks {
            self.open_edit();
        } else {
            self.selected = row;
            self.focus = Focus::List;
        }
    }
    fn extend_selection(&mut self, down: bool) {
        let count = self.visible().len();
        let anchor = *self.anchor.get_or_insert(self.selected);
        let target = if down {
            (self.selected + 1).min(count.saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.selected = target;
        self.mark_range(anchor, target);
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
        let label = Text::new("Key");
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
            Text::new(shown_key).color_all(3)
        } else {
            Text::new(shown_key)
        };
        form.key_button.set_label(shown_key);
        form.key_button.set_focused(key_active);
        form.key_button.render(x + content_column, y);
        form.picker.set_button_width(button_width);
        let mut row = y + 1;
        for warning in &form.warnings {
            print_text_with_coordinates(
                Text::new(truncate(
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
        self.focus = Focus::Search;
    }
    pub fn is_busy(&self) -> bool {
        self.form.is_some()
            || self.dialog.is_open()
            || self.mode_selector.is_open()
            || self.filter.is_open()
    }
    pub fn clear_areas(&mut self) {
        self.scroll.clear();
        self.mode_selector.clear_area();
        self.filter.clear_area();
        self.search.clear_area();
        self.mark_buttons.clear();
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
        match self.focus {
            Focus::Mode => {
                if self.mode_selector.is_open() {
                    if let UiResponse::Changed(_) = self.mode_selector.handle_key(key) {
                        self.select_first();
                    }
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
                    self.focus = Focus::Filter;
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    self.focus = Focus::Search;
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Left) {
                    return PageResponse::LeaveToMenu;
                }
                if is_plain(key, BareKey::Esc) {
                    return PageResponse::Close;
                }
                if typed(key, '/') {
                    self.focus = Focus::Search;
                    return PageResponse::Handled;
                }
                match self.mode_selector.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        PageResponse::Handled
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled,
                    _ => PageResponse::Handled,
                }
            },
            Focus::Filter => {
                if self.filter.is_open() {
                    if let UiResponse::Changed(_) = self.filter.handle_key(key) {
                        self.select_first();
                        self.scroll.reset();
                    }
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
                    self.focus = Focus::List;
                    self.scroll.follow();
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    self.focus = Focus::Mode;
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Left) {
                    return PageResponse::LeaveToMenu;
                }
                if is_plain(key, BareKey::Esc) {
                    return PageResponse::Close;
                }
                if typed(key, '/') {
                    self.focus = Focus::Search;
                    return PageResponse::Handled;
                }
                match self.filter.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        self.scroll.reset();
                        PageResponse::Handled
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled,
                    _ => PageResponse::Handled,
                }
            },
            Focus::Search => {
                if is_plain(key, BareKey::Enter)
                    || is_plain(key, BareKey::Down)
                    || is_plain(key, BareKey::Tab)
                {
                    if self.searching() {
                        self.focus = Focus::List;
                        self.select_first();
                        self.scroll.follow();
                    } else {
                        self.focus = Focus::Mode;
                    }
                    return PageResponse::Handled;
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    return PageResponse::LeaveUp;
                }
                if is_plain(key, BareKey::Esc) {
                    if self.searching() {
                        self.search.clear();
                        self.select_first();
                        self.scroll.reset();
                        return PageResponse::Handled;
                    }
                    return PageResponse::Close;
                }
                match self.search.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        self.scroll.reset();
                        PageResponse::Handled
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled,
                    _ => PageResponse::Handled,
                }
            },
            Focus::List => self.handle_list_key(key),
        }
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        print_text_with_coordinates(
            Text::new(truncate(TITLE, width)).color_all(2),
            x,
            y,
            None,
            None,
        );
        let search_x = TITLE.chars().count() + 2;
        let search_end = BESIDE_SHORT_FIELD + Button::new("Unmark all").natural_width();
        let search_width = search_end
            .min(width)
            .saturating_sub(search_x);
        let search_focused = self.focused && self.focus == Focus::Search;
        self.search.set_focused(search_focused);
        self.search.set_show_cursor(search_focused);
        self.search.render(x + search_x, y, search_width);
        let searching = self.searching();
        let query = self.query();
        self.mode_selector
            .set_focused(self.focused && self.focus == Focus::Mode);
        self.filter
            .set_focused(self.focused && self.focus == Focus::Filter);
        self.sync_filter();
        let list_offset = if searching {
            self.mode_selector.clear_area();
            self.filter.clear_area();
            2
        } else {
            self.mode_selector
                .render(x, y + 2, SHORT_FIELD_WIDTH.min(width));
            self.filter.render(x, y + 3, SHORT_FIELD_WIDTH.min(width));
            5
        };
        let mut previous_buttons = std::mem::take(&mut self.mark_buttons);
        let (entries, items) = self.view();
        let list_y = y + list_offset;
        let list_height = height.saturating_sub(list_offset).max(1);
        let selected_item = items
            .iter()
            .position(|item| *item == ListItem::Entry(self.selected));
        let marking = !self.marked.is_empty();
        let cursor_shown = self.focused && self.focus == Focus::List;
        let rows: Vec<(Vec<String>, String, Vec<ColumnStyle>)> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let mut columns = entry_columns(entry);
                if searching {
                    columns.insert(0, mode_name(entry.mode));
                }
                if marking {
                    let mark = if self.is_marked(entry) {
                        "✓"
                    } else if cursor_shown && index == self.selected {
                        "›"
                    } else {
                        " "
                    };
                    columns[0] = format!("{} {}", mark, columns[0]);
                }
                let found = if searching {
                    match_entry(&query, entry)
                } else {
                    None
                };
                let key_column = if searching { 1 } else { 0 };
                let mut styles =
                    entry_styles(entry, &columns[key_column], key_column, found.as_ref());
                if searching {
                    styles.insert(0, (0, DIM, 0..columns[0].chars().count()));
                }
                (columns, markers(entry.unsaved, false, false), styles)
            })
            .collect();
        let natural = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker, _)| (columns.as_slice(), marker.as_str())),
            usize::MAX,
        )
        .with_indent(0);
        let button_width = Button::new("Unmark all").natural_width();
        let headings_width = items
            .iter()
            .filter_map(|item| match item {
                ListItem::Heading(label) => Some(label.chars().count() + 2 + button_width),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let content_width = natural
            .natural_width("● unsaved".chars().count())
            .max(headings_width);
        let list_width = RowScroll::fitted_width(items.len(), list_height, content_width, width);
        let visible = self.scroll.layout(
            x,
            list_y,
            list_width,
            list_height,
            items.len(),
            selected_item,
        );
        self.items = items.clone();
        if entries.is_empty() {
            let empty = if searching {
                "No matching keys"
            } else {
                "No keys · a: bind a key"
            };
            print_dim(empty, x, list_y, width);
            return;
        }
        let row_width = self.scroll.row_width();
        if !searching {
            self.print_mark_button(
                &mut previous_buttons,
                None,
                x + BESIDE_SHORT_FIELD,
                y + 3,
                x + width,
            );
        }
        let layout = ColumnLayout::new(
            rows.iter()
                .map(|(columns, marker, _)| (columns.as_slice(), marker.as_str())),
            row_width,
        )
        .with_indent(0);
        for item_index in visible {
            let Some(screen_y) = self.scroll.screen_row(item_index) else {
                continue;
            };
            match &items[item_index] {
                ListItem::Gap => {},
                ListItem::Heading(label) => {
                    print_text_with_coordinates(
                        Text::new(truncate(label, row_width)).color_all(0),
                        x,
                        screen_y,
                        None,
                        None,
                    );
                    let label_x = x + label.chars().count() + 2;
                    self.print_mark_button(
                        &mut previous_buttons,
                        Some(label.clone()),
                        label_x,
                        screen_y,
                        x + row_width,
                    );
                },
                ListItem::Entry(index) => {
                    let (columns, marker, styles) = &rows[*index];
                    let selected = if marking {
                        self.is_marked(&entries[*index])
                    } else {
                        cursor_shown && *index == self.selected
                    };
                    let look = RowLook::new(selected, self.scroll.is_hovered(item_index));
                    layout.print_styled(columns, marker, x, screen_y, row_width, look, styles);
                },
            }
        }
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
        self.handle_list_mouse(mouse)
    }
    fn handle_timer(&mut self) -> bool {
        let picker_changed = self
            .form
            .as_mut()
            .map(|form| form.picker.handle_timer() | form.key_button.handle_timer())
            .unwrap_or(false);
        let mut buttons_changed = false;
        for (button, _) in self.mark_buttons.iter_mut() {
            buttons_changed |= button.handle_timer();
        }
        picker_changed || buttons_changed
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        if self.filter.is_open() {
            self.filter.render_overlay(rows, cols);
            note_dropdown(&self.filter);
        }
        if self.mode_selector.is_open() {
            self.mode_selector.render_overlay(rows, cols);
            note_dropdown(&self.mode_selector);
        }
        self.render_form(rows, cols);
        self.dialog.render_centered(rows, cols);
    }
    fn captures_keys(&self) -> bool {
        self.form.is_some()
            || self.dialog.is_open()
            || self.mode_selector.is_open()
            || self.filter.is_open()
            || self.focus == Focus::Search
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
        match self.focus {
            Focus::Search => vec![
                ("<type>", "search keys and actions"),
                ("<↓>", "results"),
                ("<Esc>", "clear"),
            ],
            Focus::Mode => vec![
                ("<Space>", "choose mode"),
                ("<↓>", "filter"),
                ("<Esc>", "close"),
            ],
            Focus::Filter => vec![
                ("<Space>", "choose category"),
                ("<↓>", "keys"),
                ("<Esc>", "close"),
            ],
            Focus::List if !self.marked.is_empty() => vec![
                ("<Del>", "delete marked"),
                ("<r>", "reset marked"),
                ("<Space>", "mark"),
                ("<Shift ↓↑>", "extend"),
                ("<A/C>", "all / category"),
                ("<Esc>", "unmark"),
            ],
            Focus::List => vec![
                ("<a>", "add"),
                ("<Enter>", "edit"),
                ("<Del>", "delete"),
                ("<r>", "reset"),
                ("<Space/Shift ↓↑/A>", "mark"),
                ("</>", "search"),
            ],
        }
    }
    fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
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
        self.focused = focused;
    }
    fn prompt_result(&mut self, request_id: u64, result: &PromptResult) -> bool {
        self.answer_delete(request_id, result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        screen.focus = Focus::List;
        screen.handle_key(&KeyWithModifier::new(BareKey::Char('C')));
        assert_eq!(screen.marked.len(), 2);
        screen.handle_key(&KeyWithModifier::new(BareKey::Esc));
        assert!(screen.marked.is_empty());
        screen.handle_key(&KeyWithModifier::new(BareKey::Char(' ')));
        screen.handle_key(&KeyWithModifier::new(BareKey::Down).with_shift_modifier());
        assert_eq!(screen.marked.len(), 2);
    }
}
