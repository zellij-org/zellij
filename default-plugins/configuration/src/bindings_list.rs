use std::collections::BTreeSet;

use zellij_tile::prelude::*;

use crate::page::{
    action_argument_range, action_display_text, actions_summary, changed_by, confirm_removals,
    is_click, is_plain, is_shift_tab, markers, note_dropdown, outside_overlays, print_dim,
    removal_confirmed, removal_prompt, truncate, typed, ColumnLayout, ColumnStyle, Effect,
    PageResponse, RowLook, RowScroll, BESIDE_SHORT_FIELD, DIM, SHORT_FIELD_WIDTH,
    SHORT_LABEL_WIDTH,
};

const MATCH_COLOR: usize = 1;
pub const ALL_CATEGORIES: &str = "All";
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

pub const ALL_MODES: &str = "All modes";

pub fn scope_text(mode: Option<InputMode>) -> String {
    match mode {
        Some(mode) => format!("{} mode", mode_name(mode)),
        None => "all modes".to_owned(),
    }
}

pub trait ListEntry: Clone {
    type Id: Ord + Clone;
    fn id(&self) -> Self::Id;
    fn mode(&self) -> Option<InputMode>;
    fn label(&self) -> String;
    fn bound_actions(&self) -> Option<&[String]>;
    fn category(&self) -> &'static str;
    fn source(&self) -> &KeybindingSource;
    fn has_preset(&self) -> bool;
    fn is_unsaved(&self) -> bool;
    fn removal_effects(&self) -> Result<Vec<Effect>, String>;
    fn reset_effect(entries: &[Self]) -> Effect;
    fn dim_actions(&self) -> bool {
        false
    }
    fn search_actions(&self) -> String {
        self.columns().get(1).cloned().unwrap_or_default()
    }
    fn columns(&self) -> Vec<String> {
        let actions = match self.bound_actions() {
            Some(actions) => actions_summary(actions),
            None => "(unbound)".to_owned(),
        };
        vec![self.label(), actions]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ListTexts {
    pub title: &'static str,
    pub search_placeholder: &'static str,
    pub search_hint: &'static str,
    pub items_hint: &'static str,
    pub plural: &'static str,
    pub no_matches: &'static str,
    pub empty: &'static str,
    pub categories: &'static [&'static str],
    pub all_modes_option: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ListItem {
    Gap,
    Heading(String),
    Entry(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListFocus {
    Search,
    Mode,
    Filter,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListResponse {
    Page(PageResponse),
    Add,
    Edit,
}

impl From<PageResponse> for ListResponse {
    fn from(response: PageResponse) -> Self {
        ListResponse::Page(response)
    }
}

pub struct SearchMatch {
    pub key: Vec<usize>,
    pub actions: Vec<usize>,
    pub score: (usize, usize),
}

pub fn match_columns(query: &str, label: &str, actions: &str) -> Option<SearchMatch> {
    let label_length = label.chars().count();
    let candidate = format!("{} {}", label, actions);
    let indices = fuzzy_match_indices(query, &candidate)?;
    let first = indices.first().copied().unwrap_or(0);
    let last = indices.last().copied().unwrap_or(0);
    Some(SearchMatch {
        key: indices
            .iter()
            .copied()
            .filter(|index| *index < label_length)
            .collect(),
        actions: indices
            .iter()
            .copied()
            .filter(|index| *index > label_length)
            .map(|index| index - label_length - 1)
            .collect(),
        score: (last - first, first),
    })
}

pub fn match_list_entry<E: ListEntry>(query: &str, entry: &E) -> Option<SearchMatch> {
    match_columns(query, &entry.label(), &entry.search_actions())
}

fn entry_styles<E: ListEntry>(
    entry: &E,
    label_column_text: &str,
    label_column: usize,
    found: Option<&SearchMatch>,
) -> Vec<ColumnStyle> {
    let mut styles = vec![];
    let column_length = label_column_text.chars().count();
    let label_length = entry.label().chars().count();
    let label_start = column_length.saturating_sub(label_length);
    for index in 0..label_length {
        let matched = found.map(|m| m.key.contains(&index)).unwrap_or(false);
        let color = if matched { MATCH_COLOR } else { 3 };
        styles.push((
            label_column,
            color,
            label_start + index..label_start + index + 1,
        ));
    }
    if let Some(actions) = entry.bound_actions() {
        let mut offset = 0;
        for action in actions {
            let text = action_display_text(action);
            let length = text.chars().count();
            if let Some(range) = action_argument_range(&text) {
                styles.push((
                    label_column + 1,
                    0,
                    offset + range.start..offset + range.end,
                ));
            }
            offset += length + 2;
        }
    }
    if entry.dim_actions() {
        let length = entry
            .columns()
            .get(1)
            .map(|c| c.chars().count())
            .unwrap_or(0);
        styles.push((label_column + 1, DIM, 0..length));
    }
    if let Some(found) = found {
        for index in &found.actions {
            styles.push((label_column + 1, MATCH_COLOR, *index..*index + 1));
        }
    }
    styles
}

fn group_by_category<E: ListEntry>(
    in_mode: Vec<E>,
    categories: &[&'static str],
) -> (Vec<E>, Vec<ListItem>) {
    let mut entries = vec![];
    let mut items = vec![];
    for category in categories {
        let members: Vec<&E> = in_mode
            .iter()
            .filter(|entry| entry.category() == *category)
            .collect();
        if members.is_empty() {
            continue;
        }
        if !items.is_empty() {
            items.push(ListItem::Gap);
        }
        items.push(ListItem::Heading((*category).to_owned()));
        for entry in members {
            items.push(ListItem::Entry(entries.len()));
            entries.push(entry.clone());
        }
    }
    (entries, items)
}

pub struct BindingsList<E: ListEntry> {
    texts: ListTexts,
    entries: Vec<E>,
    mode_selector: Dropdown,
    filter: Dropdown,
    search: TextInput,
    pub focus: ListFocus,
    focused: bool,
    pub selected: usize,
    scroll: RowScroll,
    items: Vec<ListItem>,
    pub marked: BTreeSet<E::Id>,
    anchor: Option<usize>,
    mark_buttons: Vec<(Button, Option<String>)>,
    pending_deletes: Vec<(u64, Vec<E>)>,
    effects: Vec<Effect>,
    notice: Option<String>,
}

impl<E: ListEntry> BindingsList<E> {
    pub fn new(texts: ListTexts) -> Self {
        let modes: Vec<String> = texts
            .all_modes_option
            .then(|| ALL_MODES.to_owned())
            .into_iter()
            .chain(all_modes().into_iter().map(mode_name))
            .collect();
        BindingsList {
            texts,
            entries: vec![],
            mode_selector: Dropdown::new(Text::from("Mode").color_all(0), modes)
                .label_width(SHORT_LABEL_WIDTH)
                .accent_brackets(),
            filter: Dropdown::new(
                Text::from("Filter").color_all(0),
                std::iter::once(ALL_CATEGORIES)
                    .chain(texts.categories.iter().copied())
                    .collect::<Vec<_>>(),
            )
            .label_width(SHORT_LABEL_WIDTH)
            .accent_brackets(),
            search: TextInput::empty()
                .placeholder(texts.search_placeholder)
                .search_mode(),
            focus: ListFocus::List,
            focused: false,
            selected: 0,
            scroll: RowScroll::default(),
            items: vec![],
            marked: BTreeSet::new(),
            anchor: None,
            mark_buttons: vec![],
            pending_deletes: vec![],
            effects: vec![],
            notice: None,
        }
    }
    pub fn entries(&self) -> &[E] {
        &self.entries
    }
    pub fn selected_mode(&self) -> Option<InputMode> {
        let index = self.mode_selector.selected_index();
        if self.texts.all_modes_option {
            if index == 0 {
                return None;
            }
            return all_modes().get(index - 1).copied();
        }
        Some(all_modes().get(index).copied().unwrap_or(InputMode::Normal))
    }
    pub fn mode(&self) -> InputMode {
        self.selected_mode().unwrap_or(InputMode::Normal)
    }
    #[cfg(test)]
    pub fn set_selected_mode(&mut self, mode: Option<InputMode>) {
        let offset = usize::from(self.texts.all_modes_option);
        let index = match mode {
            Some(mode) => all_modes()
                .iter()
                .position(|candidate| *candidate == mode)
                .map(|index| index + offset)
                .unwrap_or(0),
            None => 0,
        };
        self.mode_selector.set_selected(index);
        self.select_first();
    }
    pub fn reset_mode(&mut self) {
        if self.mode_selector.is_open() {
            self.mode_selector.close();
        }
        self.mode_selector.set_selected(0);
        self.select_first();
        self.scroll.reset();
    }
    pub fn is_dragging(&self) -> bool {
        self.scroll.is_dragging()
    }
    pub fn set_entries(&mut self, entries: Vec<E>) {
        self.entries = entries;
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }
    pub fn focus_top(&mut self) {
        self.focus = ListFocus::Search;
    }
    pub fn has_open_dropdown(&self) -> bool {
        self.mode_selector.is_open() || self.filter.is_open()
    }
    pub fn captures_keys(&self) -> bool {
        self.has_open_dropdown() || self.focus == ListFocus::Search
    }
    pub fn take_effects(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.effects)
    }
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }
    pub fn visible(&self) -> Vec<E> {
        self.view().0
    }
    fn category_filter(&self) -> Option<&'static str> {
        let label = self.filter.selected_value()?;
        self.texts
            .categories
            .iter()
            .copied()
            .find(|category| *category == label)
    }
    fn sync_filter(&mut self) {
        if self.filter.is_open() {
            return;
        }
        let mode = self.selected_mode();
        let candidates: Vec<&E> = self
            .entries
            .iter()
            .filter(|entry| entry.mode() == mode)
            .collect();
        let options: Vec<String> = std::iter::once(ALL_CATEGORIES)
            .chain(
                self.texts
                    .categories
                    .iter()
                    .copied()
                    .filter(|category| candidates.iter().any(|e| e.category() == *category)),
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
    pub fn searching(&self) -> bool {
        !self.query().is_empty()
    }
    fn view(&self) -> (Vec<E>, Vec<ListItem>) {
        let query = self.query();
        let mode = self.selected_mode();
        if !query.is_empty() {
            let mut found: Vec<(E, (usize, usize))> = self
                .entries
                .iter()
                .filter(|entry| entry.mode().is_none() == mode.is_none())
                .filter_map(|entry| {
                    match_list_entry(&query, entry).map(|found| (entry.clone(), found.score))
                })
                .collect();
            found.sort_by_key(|(_, score)| *score);
            let entries: Vec<E> = found.into_iter().map(|(entry, _)| entry).collect();
            let items = (0..entries.len()).map(ListItem::Entry).collect();
            return (entries, items);
        }
        let filter = self.category_filter();
        let in_mode: Vec<E> = self
            .entries
            .iter()
            .filter(|entry| {
                entry.mode() == mode
                    && filter
                        .map(|category| entry.category() == category)
                        .unwrap_or(true)
            })
            .cloned()
            .collect();
        group_by_category(in_mode, self.texts.categories)
    }
    pub fn selected_entry(&self) -> Option<E> {
        self.visible().get(self.selected).cloned()
    }
    pub fn answer_delete(&mut self, request_id: u64, result: &PromptResult) -> bool {
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
    fn reset_selected(&mut self) {
        let plural = self.texts.plural;
        let targets = self.targets();
        if targets.len() > 1 {
            let resettable: Vec<E> = targets
                .iter()
                .filter(|entry| {
                    matches!(
                        entry.source(),
                        KeybindingSource::User | KeybindingSource::Shared(_)
                    )
                })
                .cloned()
                .collect();
            let skipped = targets.len() - resettable.len();
            if resettable.is_empty() {
                self.notice = Some(format!(
                    "None of the marked {} differ from the preset",
                    plural
                ));
                return;
            }
            self.notice = Some(if skipped > 0 {
                format!(
                    "Reset {} {} to the preset; {} already use it or come from the layout",
                    resettable.len(),
                    plural,
                    skipped
                )
            } else {
                format!("Reset {} {} to the preset", resettable.len(), plural)
            });
            self.effects.push(E::reset_effect(&resettable));
            self.clear_marks();
            return;
        }
        let Some(entry) = targets.into_iter().next() else {
            return;
        };
        let label = entry.label();
        match entry.source() {
            KeybindingSource::User | KeybindingSource::Shared(_) => {
                self.effects
                    .push(E::reset_effect(std::slice::from_ref(&entry)));
                self.notice = Some(if entry.has_preset() {
                    format!("{} is back to the preset's binding", label)
                } else {
                    format!("{} is no longer bound (the preset has no binding)", label)
                });
                self.clear_marks();
            },
            KeybindingSource::Preset => {
                self.notice = Some(format!("{} already uses the preset's binding", label))
            },
            KeybindingSource::Layout => {
                self.notice = Some(format!(
                    "{} comes from the layout and cannot be changed here",
                    label
                ))
            },
        }
    }
    fn remove_now(&mut self, entries: Vec<E>) {
        let plural = self.texts.plural;
        let mut removed = 0;
        let mut last_error = None;
        for entry in &entries {
            match entry.removal_effects() {
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
                "Deleted {} from {}",
                entry.label(),
                scope_text(entry.mode())
            )),
            (removed, entries, _) if removed < entries.len() => Some(format!(
                "Deleted {} {}; {} could not be deleted",
                removed,
                plural,
                entries.len() - removed
            )),
            (removed, _, _) => Some(format!("Deleted {} {}", removed, plural)),
        };
        self.clear_marks();
    }
    fn request_delete(&mut self) {
        let plural = self.texts.plural;
        let targets: Vec<E> = self.targets();
        let removable: Vec<E> = targets
            .iter()
            .filter(|entry| entry.removal_effects().is_ok())
            .cloned()
            .collect();
        if removable.is_empty() {
            if let Some(entry) = targets.first() {
                if let Err(error) = entry.removal_effects() {
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
                let label = entry.label();
                let label_start = "Delete ".chars().count();
                let label_end = label_start + label.chars().count();
                Text::from(format!(
                    "Delete {} from {}?",
                    label,
                    scope_text(entry.mode())
                ))
                .color_range(3, label_start..label_end)
            },
            entries => {
                let skipped = targets.len() - entries.len();
                let mut question = format!("Delete {} {}?", entries.len(), plural);
                if skipped > 0 {
                    question.push_str(&format!(
                        " {} marked {} cannot be deleted and stay.",
                        skipped, plural
                    ));
                }
                Text::from(question)
            },
        };
        let request_id = prompt(removal_prompt(question));
        self.pending_deletes.push((request_id, removable));
    }
    fn select_first(&mut self) {
        self.selected = 0;
        self.clear_marks();
    }
    pub fn clear_marks(&mut self) {
        self.marked.clear();
        self.anchor = None;
    }
    fn is_marked(&self, entry: &E) -> bool {
        self.marked.contains(&entry.id())
    }
    fn toggle_mark(&mut self, index: usize) {
        if let Some(entry) = self.visible().get(index) {
            let id = entry.id();
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
            self.marked.insert(entry.id());
        }
    }
    fn targets(&self) -> Vec<E> {
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
                    .map(|category| entry.category() == category)
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
            let id = entries[*index].id();
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
        self.focus = ListFocus::List;
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
    fn click_entry(&mut self, row: usize) -> bool {
        let modifiers = mouse_modifiers();
        if modifiers.contains(&KeyModifier::Shift) {
            let anchor = self.anchor.unwrap_or(self.selected);
            self.mark_range(anchor, row);
            self.anchor = Some(anchor);
            self.selected = row;
            self.focus = ListFocus::List;
            return false;
        }
        if modifiers.contains(&KeyModifier::Ctrl) {
            if self.marked.is_empty() && self.focus == ListFocus::List && self.focused {
                self.toggle_mark(self.selected);
            }
            self.toggle_mark(row);
            self.anchor = Some(row);
            self.selected = row;
            self.focus = ListFocus::List;
            return false;
        }
        let had_marks = !self.marked.is_empty();
        self.clear_marks();
        self.anchor = Some(row);
        if self.focused && self.focus == ListFocus::List && row == self.selected && !had_marks {
            true
        } else {
            self.selected = row;
            self.focus = ListFocus::List;
            false
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
    fn entry_at(&self, line: isize, column: usize) -> Option<usize> {
        self.scroll
            .row_at(line, column)
            .and_then(|row| match self.items.get(row) {
                Some(ListItem::Entry(index)) => Some(*index),
                _ => None,
            })
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> ListResponse {
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
                self.focus = ListFocus::Filter;
                return PageResponse::Handled.into();
            }
            if self.filter.is_open() {
                return if !is_hover || filter_changed {
                    PageResponse::Handled.into()
                } else {
                    PageResponse::NotHandled.into()
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
                return PageResponse::Handled.into();
            }
        }
        let rows_hover = self.scroll.hover(&mouse);
        let search_response = self.search.handle_mouse(mouse);
        if is_hover && search_response.is_handled() {
            hover_changed = true;
        }
        if self.mode_selector.is_open() || matches!(mouse, Mouse::LeftClick(..) | Mouse::Hover(..))
        {
            let (response, selector_changed) = changed_by(&mut self.mode_selector, |selector| {
                selector.handle_mouse(mouse)
            });
            hover_changed |= selector_changed;
            if let UiResponse::Changed(_) = response {
                self.select_first();
                self.scroll.reset();
            }
            if response.is_handled() && !is_hover {
                self.focus = ListFocus::Mode;
                return PageResponse::Handled.into();
            }
        }
        if let Some(rows_changed) = rows_hover {
            return if rows_changed || hover_changed {
                PageResponse::Handled.into()
            } else {
                PageResponse::NotHandled.into()
            };
        }
        if let Mouse::Hold(line, column) = mouse {
            if !self.scroll.is_dragging() {
                let row = self.entry_at(line, column);
                if let (Some(row), Some(anchor)) = (row, self.anchor) {
                    if row != anchor || !self.marked.is_empty() {
                        self.mark_range(anchor, row);
                        self.selected = row;
                        self.focus = ListFocus::List;
                    }
                    return PageResponse::Handled.into();
                }
            }
        }
        if let Some(changed) = self.scroll.handle_wheel(&mouse) {
            return if changed {
                PageResponse::Handled.into()
            } else {
                PageResponse::NotHandled.into()
            };
        }
        let Some((line, column)) = is_click(&mouse) else {
            return PageResponse::NotHandled.into();
        };
        if self.search.hit_test(line, column) {
            self.focus = ListFocus::Search;
            return PageResponse::Handled.into();
        }
        match self.entry_at(line, column) {
            Some(row) => {
                if self.click_entry(row) {
                    ListResponse::Edit
                } else {
                    PageResponse::Handled.into()
                }
            },
            None => PageResponse::NotHandled.into(),
        }
    }
    fn handle_list_key(&mut self, key: &KeyWithModifier) -> ListResponse {
        self.scroll.follow();
        let count = self.visible().len();
        let shift = key.has_modifiers(&[KeyModifier::Shift]);
        if shift && key.bare_key == BareKey::Down {
            self.extend_selection(true);
            return PageResponse::Handled.into();
        }
        if shift && key.bare_key == BareKey::Up {
            self.extend_selection(false);
            return PageResponse::Handled.into();
        }
        if is_plain(key, BareKey::Char(' ')) {
            self.toggle_mark(self.selected);
            self.anchor = Some(self.selected);
            return PageResponse::Handled.into();
        }
        if typed(key, 'A') && key.bare_key == BareKey::Char('A') {
            self.toggle_group(None);
            return PageResponse::Handled.into();
        }
        if typed(key, 'C') && key.bare_key == BareKey::Char('C') {
            let category = self
                .visible()
                .get(self.selected)
                .map(|entry| entry.category().to_owned());
            if let Some(category) = category {
                self.toggle_group(Some(&category));
            }
            return PageResponse::Handled.into();
        }
        if is_plain(key, BareKey::Esc) && !self.marked.is_empty() {
            self.clear_marks();
            return PageResponse::Handled.into();
        }
        if is_plain(key, BareKey::Down) {
            if self.selected + 1 < count {
                self.selected += 1;
            }
        } else if is_plain(key, BareKey::Up) {
            if self.selected == 0 {
                self.focus = if self.searching() {
                    ListFocus::Search
                } else {
                    ListFocus::Filter
                };
            } else {
                self.selected -= 1;
            }
        } else if typed(key, '/') {
            self.focus = ListFocus::Search;
        } else if typed(key, 'a') {
            return ListResponse::Add;
        } else if is_plain(key, BareKey::Enter) {
            return ListResponse::Edit;
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
                return PageResponse::Close.into();
            }
        } else if is_plain(key, BareKey::Tab) || is_plain(key, BareKey::Left) || is_shift_tab(key) {
            return PageResponse::LeaveToMenu.into();
        } else {
            return PageResponse::NotHandled.into();
        }
        PageResponse::Handled.into()
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> ListResponse {
        match self.focus {
            ListFocus::Mode => {
                if self.mode_selector.is_open() {
                    if let UiResponse::Changed(_) = self.mode_selector.handle_key(key) {
                        self.select_first();
                    }
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
                    self.focus = ListFocus::Filter;
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    self.focus = ListFocus::Search;
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Left) {
                    return PageResponse::LeaveToMenu.into();
                }
                if is_plain(key, BareKey::Esc) {
                    return PageResponse::Close.into();
                }
                if typed(key, '/') {
                    self.focus = ListFocus::Search;
                    return PageResponse::Handled.into();
                }
                match self.mode_selector.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        PageResponse::Handled.into()
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled.into(),
                    _ => PageResponse::Handled.into(),
                }
            },
            ListFocus::Filter => {
                if self.filter.is_open() {
                    if let UiResponse::Changed(_) = self.filter.handle_key(key) {
                        self.select_first();
                        self.scroll.reset();
                    }
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Down) || is_plain(key, BareKey::Tab) {
                    self.focus = ListFocus::List;
                    self.scroll.follow();
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    self.focus = ListFocus::Mode;
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Left) {
                    return PageResponse::LeaveToMenu.into();
                }
                if is_plain(key, BareKey::Esc) {
                    return PageResponse::Close.into();
                }
                if typed(key, '/') {
                    self.focus = ListFocus::Search;
                    return PageResponse::Handled.into();
                }
                match self.filter.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        self.scroll.reset();
                        PageResponse::Handled.into()
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled.into(),
                    _ => PageResponse::Handled.into(),
                }
            },
            ListFocus::Search => {
                if is_plain(key, BareKey::Enter)
                    || is_plain(key, BareKey::Down)
                    || is_plain(key, BareKey::Tab)
                {
                    if self.searching() {
                        self.focus = ListFocus::List;
                        self.select_first();
                        self.scroll.follow();
                    } else {
                        self.focus = ListFocus::Mode;
                    }
                    return PageResponse::Handled.into();
                }
                if is_plain(key, BareKey::Up) || is_shift_tab(key) {
                    return PageResponse::LeaveUp.into();
                }
                if is_plain(key, BareKey::Esc) {
                    if self.searching() {
                        self.search.clear();
                        self.select_first();
                        self.scroll.reset();
                        return PageResponse::Handled.into();
                    }
                    return PageResponse::Close.into();
                }
                match self.search.handle_key(key) {
                    UiResponse::Changed(_) => {
                        self.select_first();
                        self.scroll.reset();
                        PageResponse::Handled.into()
                    },
                    UiResponse::NotHandled => PageResponse::NotHandled.into(),
                    _ => PageResponse::Handled.into(),
                }
            },
            ListFocus::List => self.handle_list_key(key),
        }
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        let title = self.texts.title;
        print_text_with_coordinates(
            Text::from(truncate(title, width)).color_all(2),
            x,
            y,
            None,
            None,
        );
        let search_x = title.chars().count() + 2;
        let search_end = BESIDE_SHORT_FIELD + Button::new("Unmark all").natural_width();
        let search_width = search_end.min(width).saturating_sub(search_x);
        let search_focused = self.focused && self.focus == ListFocus::Search;
        self.search.set_focused(search_focused);
        self.search.set_show_cursor(search_focused);
        self.search.render(x + search_x, y, search_width);
        let searching = self.searching();
        let mode_column = searching && self.selected_mode().is_some();
        let query = self.query();
        self.mode_selector
            .set_focused(self.focused && self.focus == ListFocus::Mode);
        self.filter
            .set_focused(self.focused && self.focus == ListFocus::Filter);
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
        let cursor_shown = self.focused && self.focus == ListFocus::List;
        let rows: Vec<(Vec<String>, String, Vec<ColumnStyle>)> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let mut columns = entry.columns();
                if mode_column {
                    columns.insert(0, entry.mode().map(mode_name).unwrap_or_default());
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
                    match_list_entry(&query, entry)
                } else {
                    None
                };
                let label_column = if mode_column { 1 } else { 0 };
                let mut styles =
                    entry_styles(entry, &columns[label_column], label_column, found.as_ref());
                if mode_column {
                    styles.insert(0, (0, DIM, 0..columns[0].chars().count()));
                }
                (columns, markers(entry.is_unsaved(), false, false), styles)
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
                self.texts.no_matches
            } else {
                self.texts.empty
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
                        Text::from(truncate(label, row_width)).color_all(0),
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
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        if self.filter.is_open() {
            self.filter.render_overlay(rows, cols);
            note_dropdown(&self.filter);
        }
        if self.mode_selector.is_open() {
            self.mode_selector.render_overlay(rows, cols);
            note_dropdown(&self.mode_selector);
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        let mut changed = false;
        for (button, _) in self.mark_buttons.iter_mut() {
            changed |= button.handle_timer();
        }
        changed
    }
    pub fn clear_areas(&mut self) {
        self.scroll.clear();
        self.mode_selector.clear_area();
        self.filter.clear_area();
        self.search.clear_area();
        self.mark_buttons.clear();
    }
    pub fn hints(&self) -> Vec<(&'static str, &'static str)> {
        match self.focus {
            ListFocus::Search => vec![
                ("<type>", self.texts.search_hint),
                ("<↓>", "results"),
                ("<Esc>", "clear"),
            ],
            ListFocus::Mode => vec![
                ("<Space>", "choose mode"),
                ("<↓>", "filter"),
                ("<Esc>", "close"),
            ],
            ListFocus::Filter => vec![
                ("<Space>", "choose category"),
                ("<↓>", self.texts.items_hint),
                ("<Esc>", "close"),
            ],
            ListFocus::List if !self.marked.is_empty() => vec![
                ("<Del>", "delete marked"),
                ("<r>", "reset marked"),
                ("<Space>", "mark"),
                ("<Shift ↓↑>", "extend"),
                ("<A/C>", "all / category"),
                ("<Esc>", "unmark"),
            ],
            ListFocus::List => vec![
                ("<a>", "add"),
                ("<Enter>", "edit"),
                ("<Del>", "delete"),
                ("<r>", "reset"),
                ("<Space/Shift ↓↑/A>", "mark"),
                ("</>", "search"),
            ],
        }
    }
}
