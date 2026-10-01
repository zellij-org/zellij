use zellij_tile::prelude::*;
use zellij_utils::prompt::ChoiceItem;

use crate::form::FormScreen;
use crate::outcome::{Answer, Outcome};
use crate::request::{Pattern, Request, Spec};
use crate::store::ItemStore;

pub type Hint = (&'static str, &'static str);

pub const MIN_WIDTH: usize = 36;
pub const MAX_WIDTH: usize = 100;
const LIST_ROWS: usize = 12;

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Nothing,
    Redraw,
    Done(Outcome),
}

impl Step {
    pub fn redraw_if(changed: bool) -> Step {
        if changed {
            Step::Redraw
        } else {
            Step::Nothing
        }
    }
}

pub fn truncate(text: &str, width: usize) -> String {
    let count = text.chars().count();
    if count <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    let mut truncated: String = text.chars().take(width.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

pub fn text_width(text: &str) -> usize {
    text.chars().count()
}

fn is_key(key: &KeyWithModifier, bare_key: BareKey) -> bool {
    key.is_key_without_modifier(bare_key)
}

pub fn check_text(text: &str, required: bool, pattern: Option<&Pattern>) -> Result<(), String> {
    if text.is_empty() {
        if required {
            return Err("This field is required".to_owned());
        }
        return Ok(());
    }
    match pattern {
        Some(pattern) if !pattern.is_match(text) => {
            Err(format!("Must match {}", pattern.0.as_str()))
        },
        _ => Ok(()),
    }
}

pub fn hints_width(hints: &[Hint], status: Option<&str>) -> usize {
    let hints: usize = hints
        .iter()
        .enumerate()
        .map(|(index, (key, action))| {
            text_width(key) + text_width(action) + 3 + if index == 0 { 0 } else { 2 }
        })
        .sum();
    hints + status.map(|s| text_width(s) + 2).unwrap_or(0)
}

pub fn hints_text(hints: &[Hint], status: Option<&str>, width: usize) -> Text {
    let status_width = status.map(|s| text_width(s) + 2).unwrap_or(0);
    let room = width.saturating_sub(status_width);
    let mut line = String::new();
    let mut key_ranges = vec![];
    for (index, (key, action)) in hints.iter().enumerate() {
        let separator = if index == 0 { "" } else { ", " };
        let entry = format!("{}{} - {}", separator, key, action);
        if text_width(&line) + text_width(&entry) > room {
            break;
        }
        let key_start = text_width(&line) + text_width(separator);
        key_ranges.push(key_start..key_start + text_width(key));
        line.push_str(&entry);
    }
    let mut status_range = None;
    if let Some(status) = status {
        if text_width(&line) + text_width(status) + 2 <= width {
            let padding = width - text_width(&line) - text_width(status);
            line.push_str(&" ".repeat(padding));
            let start = text_width(&line);
            line.push_str(status);
            status_range = Some(start..start + text_width(status));
        }
    }
    let mut text = Text::new(line);
    for range in key_ranges {
        text = text.color_range(3, range);
    }
    if let Some(range) = status_range {
        text = text.dim_range(range);
    }
    text
}

pub fn print_error(text: &str, x: usize, y: usize, width: usize) {
    let text = truncate(text, width);
    let length = text.chars().count();
    print_text_with_coordinates(
        Text::new(text).error_color_range(0..length),
        x,
        y,
        Some(width),
        None,
    );
}

pub enum Screen {
    Confirm(ConfirmScreen),
    Choose(ChooseScreen),
    Input(InputScreen),
    Number(NumberScreen),
    Toggle(ToggleScreen),
    Select(SelectScreen),
    Menu(MenuScreen),
    Form(FormScreen),
}

impl Screen {
    pub fn new(request: &Request) -> Screen {
        match &request.spec {
            Spec::Confirm {
                message,
                yes,
                no,
                default_yes,
            } => Screen::Confirm(ConfirmScreen::new(
                request
                    .common
                    .title
                    .clone()
                    .unwrap_or_else(|| "Confirm".to_owned()),
                message,
                yes,
                no,
                *default_yes,
            )),
            Spec::Choose {
                items,
                multi,
                selected,
                streaming,
                ..
            } => Screen::Choose(ChooseScreen::new(
                items.clone(),
                *multi,
                selected.clone(),
                *streaming,
            )),
            Spec::Input {
                message,
                placeholder,
                validate,
                required,
                default,
            } => Screen::Input(InputScreen::new(
                message.clone(),
                placeholder.clone(),
                validate.clone(),
                *required,
                default.clone(),
            )),
            Spec::Number {
                message,
                min,
                max,
                step,
                default,
            } => Screen::Number(NumberScreen::new(
                message.clone(),
                *min,
                *max,
                *step,
                *default,
            )),
            Spec::Toggle { message, default } => {
                Screen::Toggle(ToggleScreen::new(message.clone(), *default))
            },
            Spec::Select {
                message,
                options,
                default,
            } => Screen::Select(SelectScreen::new(
                message.clone(),
                options.clone(),
                *default,
            )),
            Spec::Menu { items } => Screen::Menu(MenuScreen::new(items.clone())),
            Spec::Form {
                spec,
                patterns,
                default,
            } => Screen::Form(FormScreen::new(
                spec.clone(),
                patterns.clone(),
                default.clone(),
            )),
        }
    }
    pub fn has_own_frame(&self) -> bool {
        matches!(self, Screen::Confirm(_))
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        match self {
            Screen::Confirm(s) => s.handle_key(key),
            Screen::Choose(s) => s.handle_key(key),
            Screen::Input(s) => s.handle_key(key),
            Screen::Number(s) => s.handle_key(key),
            Screen::Toggle(s) => s.handle_key(key),
            Screen::Select(s) => s.handle_key(key),
            Screen::Menu(s) => s.handle_key(key),
            Screen::Form(s) => s.handle_key(key),
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        match self {
            Screen::Confirm(s) => s.handle_mouse(mouse),
            Screen::Choose(s) => s.handle_mouse(mouse),
            Screen::Input(s) => s.handle_mouse(mouse),
            Screen::Number(s) => s.handle_mouse(mouse),
            Screen::Toggle(s) => s.handle_mouse(mouse),
            Screen::Select(s) => s.handle_mouse(mouse),
            Screen::Menu(s) => s.handle_mouse(mouse),
            Screen::Form(s) => s.handle_mouse(mouse),
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        match self {
            Screen::Form(s) => s.handle_timer(),
            _ => false,
        }
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        match self {
            Screen::Confirm(s) => s.render(x, y, width, height),
            Screen::Choose(s) => s.render(x, y, width, height),
            Screen::Input(s) => s.render(x, y, width, height),
            Screen::Number(s) => s.render(x, y, width, height),
            Screen::Toggle(s) => s.render(x, y, width, height),
            Screen::Select(s) => s.render(x, y, width, height),
            Screen::Menu(s) => s.render(x, y, width, height),
            Screen::Form(s) => s.render(x, y, width, height),
        }
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        match self {
            Screen::Select(s) => s.render_overlays(rows, cols),
            Screen::Form(s) => s.render_overlays(rows, cols),
            _ => {},
        }
    }
    pub fn desired_size(&self) -> (usize, usize) {
        match self {
            Screen::Confirm(s) => s.desired_size(),
            Screen::Choose(s) => s.desired_size(),
            Screen::Input(s) => s.desired_size(),
            Screen::Number(s) => s.desired_size(),
            Screen::Toggle(s) => s.desired_size(),
            Screen::Select(s) => s.desired_size(),
            Screen::Menu(s) => s.desired_size(),
            Screen::Form(s) => s.desired_size(),
        }
    }
    pub fn hints(&self) -> &'static [Hint] {
        match self {
            Screen::Confirm(_) => &[("<←→>", "move"), ("<Enter>", "choose"), ("<Esc>", "cancel")],
            Screen::Choose(s) => s.hints(),
            Screen::Input(_) => &[("<Enter>", "accept"), ("<Esc>", "cancel")],
            Screen::Number(_) => &[
                ("<←→>", "change"),
                ("<Enter>", "accept"),
                ("<Esc>", "cancel"),
            ],
            Screen::Toggle(_) => &[
                ("<Tab/Space>", "switch"),
                ("<Enter>", "accept"),
                ("<Esc>", "cancel"),
            ],
            Screen::Select(_) => &[
                ("<Space>", "open"),
                ("<←→>", "change"),
                ("<Enter>", "accept"),
                ("<Esc>", "cancel"),
            ],
            Screen::Menu(_) => &[("<↓↑>", "move"), ("<Enter>", "choose"), ("<Esc>", "cancel")],
            Screen::Form(_) => &[("<Tab>", "next"), ("<Ctrl s>", "save"), ("<Esc>", "cancel")],
        }
    }
    pub fn status(&self) -> Option<String> {
        match self {
            Screen::Choose(s) => s.status(),
            _ => None,
        }
    }
    pub fn confirm_footer_area(&self) -> Option<Rect> {
        match self {
            Screen::Confirm(s) => s.dialog.footer_area(),
            _ => None,
        }
    }
    pub fn input_ended(&mut self) {
        if let Screen::Choose(s) = self {
            s.input_ended();
        }
    }
}

pub struct ConfirmScreen {
    dialog: ConfirmDialog,
}

impl ConfirmScreen {
    pub fn new(
        title: String,
        message: &str,
        yes: &str,
        no: &str,
        default_yes: Option<bool>,
    ) -> Self {
        let selected = if default_yes == Some(false) { 1 } else { 0 };
        ConfirmScreen {
            dialog: ConfirmDialog::new(title, message)
                .buttons(vec![yes.to_owned(), no.to_owned()])
                .selected(selected)
                .footer_rows(2)
                .centered()
                .opened(),
        }
    }
    fn respond(&mut self, response: UiResponse) -> Step {
        match response {
            UiResponse::Submitted(UiValue::Choice { index, .. }) => {
                Step::Done(Outcome::Answered(Answer::Confirmed(index == 0)))
            },
            UiResponse::Cancelled => Step::Done(Outcome::Cancelled),
            UiResponse::NotHandled => Step::Nothing,
            _ => Step::Redraw,
        }
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        let response = self.dialog.handle_key(key);
        self.respond(response)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        let response = self.dialog.handle_mouse(mouse);
        self.respond(response)
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.dialog.render(x, y, width, height);
    }
    fn desired_size(&self) -> (usize, usize) {
        self.dialog.size_for(MAX_WIDTH - 20)
    }
}

pub const CHOOSE_MEMORY_BUDGET: usize = 11 * 1024 * 1024;
const PER_ITEM_BYTES: usize = 24;

pub struct ChooseScreen {
    items: ItemStore,
    marked: Vec<bool>,
    multi: bool,
    preselected: Vec<String>,
    search: TextInput,
    list: MenuList,
    visible: Vec<u32>,
    highlighted: Option<usize>,
    offset: usize,
    page_rows: usize,
    widest: usize,
    budget: usize,
    skipped: usize,
    reading: bool,
    dirty: bool,
}

impl ChooseScreen {
    pub fn new(
        items: Vec<ChoiceItem>,
        multi: bool,
        preselected: Vec<String>,
        reading: bool,
    ) -> Self {
        Self::with_budget(items, multi, preselected, reading, CHOOSE_MEMORY_BUDGET)
    }
    pub fn with_budget(
        items: Vec<ChoiceItem>,
        multi: bool,
        preselected: Vec<String>,
        reading: bool,
        budget: usize,
    ) -> Self {
        let mut screen = ChooseScreen {
            items: ItemStore::default(),
            marked: vec![],
            multi,
            preselected,
            search: TextInput::empty()
                .search_mode()
                .placeholder("Type to search")
                .focused(),
            list: MenuList::new(vec![]).focused(),
            visible: vec![],
            highlighted: None,
            offset: 0,
            page_rows: LIST_ROWS,
            widest: 0,
            budget,
            skipped: 0,
            reading,
            dirty: true,
        };
        for item in items {
            screen.push_item(item);
        }
        screen.highlighted = screen.preselected_single();
        screen.refresh();
        screen
    }
    fn memory_in_use(&self) -> usize {
        self.items.footprint() + self.items.len() * PER_ITEM_BYTES
    }
    fn push_item(&mut self, item: ChoiceItem) {
        let needed =
            item.value.len() + item.label.as_ref().map(|l| l.len()).unwrap_or(0) + PER_ITEM_BYTES;
        if self.memory_in_use() + needed > self.budget {
            self.skipped += 1;
            return;
        }
        self.widest = self.widest.max(text_width(item.display()));
        let preselected = self.preselected.contains(&item.value);
        self.marked.push(preselected && self.multi);
        self.items.push(&item);
        self.dirty = true;
    }
    pub fn add_item(&mut self, item: ChoiceItem) {
        self.push_item(item);
    }
    pub fn input_ended(&mut self) {
        self.reading = false;
        self.dirty = true;
    }
    pub fn received_count(&self) -> usize {
        self.items.len() + self.skipped
    }
    #[cfg(test)]
    pub fn item_count(&self) -> usize {
        self.items.len()
    }
    #[cfg(test)]
    pub fn window_len(&self) -> usize {
        self.list.items().len()
    }
    #[cfg(test)]
    pub fn visible_values(&mut self) -> Vec<String> {
        self.refresh();
        self.visible
            .iter()
            .map(|index| self.items.value(*index as usize).to_owned())
            .collect()
    }
    fn preselected_single(&self) -> Option<usize> {
        if self.multi {
            return None;
        }
        (0..self.items.len()).find(|index| {
            self.preselected
                .iter()
                .any(|value| value == self.items.value(*index))
        })
    }
    fn highlighted_position(&self) -> Option<usize> {
        let highlighted = self.highlighted?;
        self.visible
            .iter()
            .position(|index| *index as usize == highlighted)
    }
    pub fn refresh(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let query = self.search.get_text().to_owned();
        let searching = !query.trim().is_empty();
        let items = &self.items;
        let visible: Vec<u32> = (0..items.len())
            .filter(|index| {
                !searching || fuzzy_match_indices(&query, items.display(*index)).is_some()
            })
            .map(|index| index as u32)
            .collect();
        self.visible = visible;
        if self.highlighted_position().is_none() {
            self.highlighted = self.visible.first().map(|index| *index as usize);
        }
        self.search.set_match_count(if searching {
            Some(self.visible.len())
        } else {
            None
        });
    }
    fn move_highlight(&mut self, delta: isize) -> Step {
        self.refresh();
        if self.visible.is_empty() {
            return Step::Nothing;
        }
        let current = self.highlighted_position().unwrap_or(0) as isize;
        let last = self.visible.len() as isize - 1;
        let next = (current + delta).clamp(0, last) as usize;
        self.highlighted = Some(self.visible[next] as usize);
        Step::Redraw
    }
    fn toggle_highlighted(&mut self) -> Step {
        self.refresh();
        match self.highlighted {
            Some(index) => {
                self.marked[index] = !self.marked[index];
                Step::Redraw
            },
            None => Step::Nothing,
        }
    }
    fn submit(&mut self) -> Step {
        self.refresh();
        if self.multi {
            let chosen = (0..self.items.len())
                .filter(|index| self.marked[*index])
                .map(|index| self.items.value(index).to_owned())
                .collect();
            return Step::Done(Outcome::Answered(Answer::Choices(chosen)));
        }
        match self.highlighted {
            Some(index) => Step::Done(Outcome::Answered(Answer::Choice(
                self.items.value(index).to_owned(),
            ))),
            None => Step::Nothing,
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        self.refresh();
        if is_key(key, BareKey::Enter) {
            return self.submit();
        }
        if self.multi && (is_key(key, BareKey::Char(' ')) || is_key(key, BareKey::Tab)) {
            return self.toggle_highlighted();
        }
        let page = self.page_rows.max(1) as isize;
        let delta = if is_key(key, BareKey::Up) {
            Some(-1)
        } else if is_key(key, BareKey::Down) {
            Some(1)
        } else if is_key(key, BareKey::PageUp) {
            Some(-page)
        } else if is_key(key, BareKey::PageDown) {
            Some(page)
        } else {
            None
        };
        if let Some(delta) = delta {
            return self.move_highlight(delta);
        }
        match self.search.handle_key(key) {
            UiResponse::Cancelled => Step::Done(Outcome::Cancelled),
            UiResponse::Changed(_) => {
                self.dirty = true;
                self.offset = 0;
                self.highlighted = None;
                self.refresh();
                Step::Redraw
            },
            UiResponse::NotHandled => Step::Nothing,
            _ => Step::Redraw,
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        self.refresh();
        match mouse {
            Mouse::Hover(..) => {
                self.search.handle_mouse(mouse);
                self.list.handle_mouse(mouse);
                Step::Redraw
            },
            Mouse::ScrollUp(lines) => self.move_highlight(-(lines as isize)),
            Mouse::ScrollDown(lines) => self.move_highlight(lines as isize),
            _ => {
                if let UiResponse::Submitted(UiValue::Choice { index, .. }) =
                    self.list.handle_mouse(mouse)
                {
                    if let Some(item) = self.visible.get(self.offset + index).copied() {
                        self.highlighted = Some(item as usize);
                        return if self.multi {
                            self.toggle_highlighted()
                        } else {
                            self.submit()
                        };
                    }
                }
                self.search.handle_mouse(mouse);
                Step::Redraw
            },
        }
    }
    fn window_items(&self, rows: usize, width: usize) -> Vec<MenuItem> {
        let query = self.search.get_text();
        let searching = !query.trim().is_empty();
        self.visible
            .iter()
            .skip(self.offset)
            .take(rows)
            .map(|index| {
                let full_label = self.items.display(*index as usize);
                let label = truncate(full_label, width);
                let shown = label.chars().count();
                let matched = if searching {
                    fuzzy_match_indices(query, full_label)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|index| *index < shown)
                        .collect()
                } else {
                    vec![]
                };
                let item = MenuItem::new(label).matched_indices(matched);
                if self.marked[*index as usize] {
                    item.marked()
                } else {
                    item
                }
            })
            .collect()
    }
    fn print_indicator(&self, text: String, x: usize, y: usize, width: usize) {
        let length = text.chars().count();
        print_text_with_coordinates(
            Text::new(text).color_range(1, 0..length),
            x,
            y,
            Some(width),
            None,
        );
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.refresh();
        self.search.render(x, y, width);
        if self.items.is_empty() && !self.reading {
            print_text_with_coordinates(
                Text::new("Nothing to choose from").dim_all(),
                x + 1,
                y + 1,
                Some(width.saturating_sub(1)),
                None,
            );
            return;
        }
        let list_height = height.saturating_sub(1).max(1);
        let total = self.visible.len();
        let needs_indicators = total > list_height && list_height > 2;
        let rows = if needs_indicators {
            list_height - 2
        } else {
            list_height
        };
        self.page_rows = rows;
        let position = self.highlighted_position().unwrap_or(0);
        if position < self.offset {
            self.offset = position;
        } else if position >= self.offset + rows {
            self.offset = position + 1 - rows;
        }
        self.offset = self.offset.min(total.saturating_sub(rows));
        let window = self.window_items(rows, width);
        self.list.set_items(window);
        self.list.set_highlighted(
            self.highlighted_position()
                .and_then(|p| p.checked_sub(self.offset)),
        );
        let list_y = y + 1 + if needs_indicators { 1 } else { 0 };
        if needs_indicators {
            let above = self.offset;
            let below = total.saturating_sub(self.offset + rows);
            let above_text = if above > 0 {
                format!(" ↑ [+{}]", above)
            } else {
                String::new()
            };
            let below_text = if below > 0 {
                format!(" ↓ [+{}]", below)
            } else {
                String::new()
            };
            self.print_indicator(above_text, x, y + 1, width);
            self.print_indicator(below_text, x, list_y + rows, width);
        }
        self.list.render(x, list_y, width, rows);
    }
    pub fn desired_size(&self) -> (usize, usize) {
        let rows = if self.reading {
            LIST_ROWS
        } else {
            self.items.len().clamp(1, LIST_ROWS)
        };
        let list_height = if self.items.len() > rows {
            rows + 2
        } else {
            rows
        };
        (self.widest + 8, 1 + list_height)
    }
    pub fn hints(&self) -> &'static [Hint] {
        if self.multi {
            &[
                ("<Space>", "tick"),
                ("<Enter>", "confirm"),
                ("<Esc>", "cancel"),
            ]
        } else {
            &[("<↓↑>", "move"), ("<Enter>", "choose"), ("<Esc>", "cancel")]
        }
    }
    pub fn status(&self) -> Option<String> {
        let mut parts = vec![];
        if self.reading {
            parts.push("reading…".to_owned());
        }
        if self.skipped > 0 {
            parts.push(format!("{} lines skipped: input too large", self.skipped));
        }
        if self.multi {
            let ticked = self.marked.iter().filter(|m| **m).count();
            parts.push(format!("{} ticked", ticked));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" · "))
        }
    }
}

pub struct InputScreen {
    input: TextInput,
    required: bool,
    pattern: Option<Pattern>,
    error: Option<String>,
    tried: bool,
}

impl InputScreen {
    pub fn new(
        message: Option<String>,
        placeholder: Option<String>,
        pattern: Option<Pattern>,
        required: bool,
        default: Option<String>,
    ) -> Self {
        let mut input = TextInput::new(default.unwrap_or_default()).focused();
        if let Some(message) = message {
            input = input.label(message);
        }
        if let Some(placeholder) = placeholder {
            input = input.placeholder(placeholder);
        }
        InputScreen {
            input,
            required,
            pattern,
            error: None,
            tried: false,
        }
    }
    fn check(&mut self) -> bool {
        match check_text(self.input.get_text(), self.required, self.pattern.as_ref()) {
            Ok(()) => {
                self.error = None;
                true
            },
            Err(error) => {
                self.error = Some(error);
                false
            },
        }
    }
    #[cfg(test)]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        if is_key(key, BareKey::Esc) {
            return Step::Done(Outcome::Cancelled);
        }
        if is_key(key, BareKey::Enter) {
            self.tried = true;
            return if self.check() {
                Step::Done(Outcome::Answered(Answer::Text(
                    self.input.get_text().to_owned(),
                )))
            } else {
                Step::Redraw
            };
        }
        let response = self.input.handle_key(key);
        if self.tried {
            self.check();
        }
        Step::redraw_if(response.is_handled())
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        Step::redraw_if(self.input.handle_mouse(mouse).is_handled())
    }
    fn render(&mut self, x: usize, y: usize, width: usize, _height: usize) {
        self.input.render(x, y, width);
        if let Some(error) = &self.error {
            print_error(error, x, y + 1, width);
        }
    }
    fn desired_size(&self) -> (usize, usize) {
        (60, 2)
    }
}

pub struct NumberScreen {
    message: String,
    min: Option<i64>,
    max: Option<i64>,
    stepper: NumberStepper,
    error: Option<String>,
}

pub fn range_error(value: i64, min: Option<i64>, max: Option<i64>) -> Option<String> {
    match (min, max) {
        (Some(min), Some(max)) if value < min || value > max => {
            Some(format!("Must be between {} and {}", min, max))
        },
        (Some(min), None) if value < min => Some(format!("Must be at least {}", min)),
        (None, Some(max)) if value > max => Some(format!("Must be at most {}", max)),
        _ => None,
    }
}

impl NumberScreen {
    pub fn new(
        message: Option<String>,
        min: Option<i64>,
        max: Option<i64>,
        step: i64,
        default: Option<i64>,
    ) -> Self {
        let mut value = default.unwrap_or(0);
        if let Some(min) = min {
            value = value.max(min);
        }
        if let Some(max) = max {
            value = value.min(max);
        }
        let mut stepper = NumberStepper::new("", value).step(step).focused();
        if let Some(min) = min {
            stepper = stepper.min(min);
        }
        if let Some(max) = max {
            stepper = stepper.max(max);
        }
        NumberScreen {
            message: message.unwrap_or_default(),
            min,
            max,
            stepper,
            error: None,
        }
    }
    #[cfg(test)]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn typed_value_error(&self) -> Option<String> {
        if !self.stepper.is_editing() {
            return None;
        }
        let text = self.stepper.edit_text().trim();
        if text.is_empty() || text == "-" {
            return Some("Type a whole number".to_owned());
        }
        match text.parse::<i64>() {
            Ok(value) => range_error(value, self.min, self.max),
            Err(_) => Some("Type a whole number".to_owned()),
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        if is_key(key, BareKey::Enter) {
            if let Some(error) = self.typed_value_error() {
                self.error = Some(error);
                return Step::Redraw;
            }
            self.error = None;
            if self.stepper.is_editing() {
                self.stepper.handle_key(key);
            }
            return Step::Done(Outcome::Answered(Answer::Number(self.stepper.value())));
        }
        if is_key(key, BareKey::Esc) && !self.stepper.is_editing() {
            return Step::Done(Outcome::Cancelled);
        }
        let key = if is_key(key, BareKey::Up) {
            KeyWithModifier::new(BareKey::Right)
        } else if is_key(key, BareKey::Down) {
            KeyWithModifier::new(BareKey::Left)
        } else {
            key.clone()
        };
        let handled = self.stepper.handle_key(&key).is_handled();
        if self.error.is_some() {
            self.error = self.typed_value_error();
        }
        Step::redraw_if(handled || self.error.is_none())
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        Step::redraw_if(self.stepper.handle_mouse(mouse).is_handled())
    }
    fn render(&mut self, x: usize, y: usize, width: usize, _height: usize) {
        let message = truncate(&self.message, width);
        let length = message.chars().count();
        let message_left = width.saturating_sub(length) / 2;
        print_text_with_coordinates(
            Text::new(message).color_range(0, 0..length),
            x + message_left,
            y,
            Some(width.saturating_sub(message_left)),
            None,
        );
        let left = width.saturating_sub(self.stepper.width()) / 2;
        self.stepper.render(x + left, y + 2);
        if let Some(error) = &self.error {
            let error = truncate(error, width);
            let error_left = width.saturating_sub(error.chars().count()) / 2;
            print_error(
                &error,
                x + error_left,
                y + 3,
                width.saturating_sub(error_left),
            );
        }
    }
    fn desired_size(&self) -> (usize, usize) {
        (
            text_width(&self.message).max(self.stepper.width()).max(24) + 4,
            4,
        )
    }
}

pub struct ToggleScreen {
    message: String,
    toggle: Toggle,
}

impl ToggleScreen {
    pub fn new(message: Option<String>, on: bool) -> Self {
        ToggleScreen {
            message: message.unwrap_or_default(),
            toggle: Toggle::new("", on).focused(),
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        if is_key(key, BareKey::Enter) {
            return Step::Done(Outcome::Answered(Answer::Bool(self.toggle.is_on())));
        }
        if is_key(key, BareKey::Esc) {
            return Step::Done(Outcome::Cancelled);
        }
        if is_key(key, BareKey::Left) {
            self.toggle.set_on(false);
            return Step::Redraw;
        }
        if is_key(key, BareKey::Right) {
            self.toggle.set_on(true);
            return Step::Redraw;
        }
        if is_key(key, BareKey::Tab) {
            let on = self.toggle.is_on();
            self.toggle.set_on(!on);
            return Step::Redraw;
        }
        Step::redraw_if(self.toggle.handle_key(key).is_handled())
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        Step::redraw_if(self.toggle.handle_mouse(mouse).is_handled())
    }
    fn render(&mut self, x: usize, y: usize, width: usize, _height: usize) {
        let message = truncate(&self.message, width);
        let length = message.chars().count();
        let message_left = width.saturating_sub(length) / 2;
        print_text_with_coordinates(
            Text::new(message).color_range(0, 0..length),
            x + message_left,
            y,
            Some(width.saturating_sub(message_left)),
            None,
        );
        let left = width.saturating_sub(self.toggle.width()) / 2;
        self.toggle.render(x + left, y + 2);
    }
    fn desired_size(&self) -> (usize, usize) {
        (text_width(&self.message).max(self.toggle.width()) + 4, 4)
    }
}

pub struct SelectScreen {
    dropdown: Dropdown,
    list_rows: usize,
}

impl SelectScreen {
    pub fn new(message: String, options: Vec<String>, default: Option<usize>) -> Self {
        let list_rows = options.len().clamp(1, 8);
        let mut dropdown = Dropdown::new(message, options)
            .max_list_rows(list_rows)
            .focused();
        if let Some(default) = default {
            dropdown = dropdown.selected(default);
        }
        SelectScreen {
            dropdown,
            list_rows,
        }
    }
    pub fn selected_value(&self) -> Option<String> {
        self.dropdown.selected_value().map(|v| v.to_owned())
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        if !self.dropdown.is_open() {
            if is_key(key, BareKey::Enter) {
                return match self.selected_value() {
                    Some(value) => Step::Done(Outcome::Answered(Answer::Choice(value))),
                    None => Step::Nothing,
                };
            }
            if is_key(key, BareKey::Esc) {
                return Step::Done(Outcome::Cancelled);
            }
            if is_key(key, BareKey::Down) {
                self.dropdown.open();
                return Step::Redraw;
            }
        }
        Step::redraw_if(self.dropdown.handle_key(key).is_handled())
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        Step::redraw_if(self.dropdown.handle_mouse(mouse).is_handled())
    }
    fn render(&mut self, x: usize, y: usize, width: usize, _height: usize) {
        self.dropdown.render(x, y, width);
    }
    fn render_overlays(&mut self, rows: usize, cols: usize) {
        self.dropdown.render_overlay(rows, cols);
    }
    fn desired_size(&self) -> (usize, usize) {
        let widest = self
            .dropdown
            .options()
            .iter()
            .map(|option| text_width(option))
            .max()
            .unwrap_or(0);
        (
            text_width(self.dropdown.label()) + widest + 10,
            1 + self.list_rows + 2,
        )
    }
}

pub struct MenuScreen {
    items: Vec<ChoiceItem>,
    list: MenuList,
}

impl MenuScreen {
    pub fn new(items: Vec<ChoiceItem>) -> Self {
        let menu_items = items
            .iter()
            .map(|item| MenuItem::new(item.display()))
            .collect();
        MenuScreen {
            items,
            list: MenuList::new(menu_items).focused(),
        }
    }
    fn respond(&mut self, response: UiResponse) -> Step {
        match response {
            UiResponse::Submitted(UiValue::Choice { index, .. }) => match self.items.get(index) {
                Some(item) => Step::Done(Outcome::Answered(Answer::Choice(item.value.clone()))),
                None => Step::Nothing,
            },
            UiResponse::Cancelled => Step::Done(Outcome::Cancelled),
            UiResponse::NotHandled => Step::Nothing,
            _ => Step::Redraw,
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Step {
        let response = self.list.handle_key(key);
        self.respond(response)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Step {
        let response = self.list.handle_mouse(mouse);
        self.respond(response)
    }
    fn render(&mut self, x: usize, y: usize, width: usize, height: usize) {
        self.list.render(x, y, width, height);
    }
    fn desired_size(&self) -> (usize, usize) {
        (
            self.list.natural_width() + 2,
            self.list.height_for(LIST_ROWS),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(bare_key: BareKey) -> KeyWithModifier {
        KeyWithModifier::new(bare_key)
    }

    fn items(values: &[&str]) -> Vec<ChoiceItem> {
        values.iter().map(|v| ChoiceItem::plain(*v)).collect()
    }

    #[test]
    fn choose_filters_fuzzily_and_keeps_input_order() {
        let mut screen = ChooseScreen::new(
            items(&["feature/login", "main", "fix/logout", "docs"]),
            false,
            vec![],
            false,
        );
        for c in "lo".chars() {
            screen.handle_key(&key(BareKey::Char(c)));
        }
        assert_eq!(
            screen.visible_values(),
            vec!["feature/login".to_owned(), "fix/logout".to_owned()]
        );
        assert_eq!(screen.handle_key(&key(BareKey::Down)), Step::Redraw);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("fix/logout".to_owned())))
        );
    }

    #[test]
    fn choose_esc_clears_the_search_first_then_cancels() {
        let mut screen = ChooseScreen::new(items(&["a", "b"]), false, vec![], false);
        screen.handle_key(&key(BareKey::Char('b')));
        assert_eq!(screen.handle_key(&key(BareKey::Esc)), Step::Redraw);
        assert_eq!(screen.visible_values().len(), 2);
        assert_eq!(
            screen.handle_key(&key(BareKey::Esc)),
            Step::Done(Outcome::Cancelled)
        );
    }

    #[test]
    fn choose_multi_ticks_items_and_returns_them_in_input_order() {
        let mut screen =
            ChooseScreen::new(items(&["a", "b", "c"]), true, vec!["c".to_owned()], false);
        screen.handle_key(&key(BareKey::Char(' ')));
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choices(vec![
                "a".to_owned(),
                "c".to_owned()
            ])))
        );
    }

    #[test]
    fn choose_multi_with_nothing_ticked_answers_with_nothing() {
        let mut screen = ChooseScreen::new(items(&["a", "b"]), true, vec![], false);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choices(vec![])))
        );
    }

    #[test]
    fn choose_single_starts_on_the_preselected_item() {
        let mut screen =
            ChooseScreen::new(items(&["a", "b", "c"]), false, vec!["b".to_owned()], false);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("b".to_owned())))
        );
    }

    #[test]
    fn choose_accepts_items_while_reading() {
        let mut screen = ChooseScreen::new(items(&["a"]), false, vec![], true);
        screen.add_item(ChoiceItem::labeled("b", "Bee"));
        screen.input_ended();
        assert_eq!(screen.item_count(), 2);
        screen.handle_key(&key(BareKey::Char('e')));
        assert_eq!(screen.visible_values(), vec!["b".to_owned()]);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("b".to_owned())))
        );
    }

    #[test]
    fn choose_keeps_only_the_rows_on_screen_in_the_list_element() {
        let many: Vec<ChoiceItem> = (0..5000)
            .map(|i| ChoiceItem::plain(format!("item {}", i)))
            .collect();
        let mut screen = ChooseScreen::new(many, false, vec![], false);
        screen.render(0, 0, 40, 10);
        assert!(screen.window_len() <= 9, "{}", screen.window_len());
        for _ in 0..20 {
            screen.handle_key(&key(BareKey::Down));
        }
        screen.render(0, 0, 40, 10);
        assert!(screen.window_len() <= 9);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("item 20".to_owned())))
        );
    }

    #[test]
    fn choose_skips_lines_beyond_its_memory_budget_instead_of_crashing() {
        let mut screen = ChooseScreen::with_budget(vec![], false, vec![], true, 64 * 1024 + 200);
        for i in 0..10 {
            screen.add_item(ChoiceItem::plain(format!("line {}", i)));
        }
        assert_eq!(screen.received_count(), 10);
        assert!(screen.item_count() < 10);
        let skipped = 10 - screen.item_count();
        assert!(screen
            .status()
            .unwrap()
            .contains(&format!("{} lines skipped: input too large", skipped)));
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("line 0".to_owned())))
        );
    }

    #[test]
    fn choose_scrolls_the_window_with_the_mouse_wheel() {
        let many: Vec<ChoiceItem> = (0..50)
            .map(|i| ChoiceItem::plain(format!("item {}", i)))
            .collect();
        let mut screen = ChooseScreen::new(many, false, vec![], false);
        screen.handle_mouse(Mouse::ScrollDown(3));
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("item 3".to_owned())))
        );
    }

    #[test]
    fn input_refuses_text_that_fails_the_check() {
        let pattern = Pattern(regex::Regex::new("^[a-z]+$").unwrap());
        let mut screen = InputScreen::new(None, None, Some(pattern), true, None);
        assert_eq!(screen.handle_key(&key(BareKey::Enter)), Step::Redraw);
        assert_eq!(screen.error(), Some("This field is required"));
        screen.handle_key(&key(BareKey::Char('A')));
        assert!(screen.error().unwrap().starts_with("Must match"));
        assert_eq!(screen.handle_key(&key(BareKey::Enter)), Step::Redraw);
        screen.handle_key(&key(BareKey::Backspace));
        screen.handle_key(&key(BareKey::Char('a')));
        assert_eq!(screen.error(), None);
        assert_eq!(
            screen.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Text("a".to_owned())))
        );
    }

    #[test]
    fn number_toggle_and_select_answer_their_values() {
        let mut number = NumberScreen::new(None, Some(1), Some(10), 1, Some(5));
        number.handle_key(&key(BareKey::Up));
        assert_eq!(
            number.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Number(6)))
        );
        assert_eq!(number.desired_size().1, 4);
        let mut toggle = ToggleScreen::new(None, false);
        toggle.handle_key(&key(BareKey::Char(' ')));
        assert_eq!(
            toggle.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Bool(true)))
        );
        toggle.handle_key(&key(BareKey::Tab));
        assert_eq!(
            toggle.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Bool(false)))
        );
        let mut select = SelectScreen::new(
            "License".to_owned(),
            vec!["MIT".to_owned(), "Apache-2.0".to_owned()],
            None,
        );
        select.handle_key(&key(BareKey::Right));
        assert_eq!(
            select.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("Apache-2.0".to_owned())))
        );
        assert_eq!(
            select.handle_key(&key(BareKey::Esc)),
            Step::Done(Outcome::Cancelled)
        );
    }

    #[test]
    fn number_refuses_a_typed_value_outside_the_range() {
        let mut number = NumberScreen::new(None, Some(1), Some(10), 1, Some(5));
        number.handle_key(&key(BareKey::Char('4')));
        number.handle_key(&key(BareKey::Char('2')));
        assert_eq!(number.handle_key(&key(BareKey::Enter)), Step::Redraw);
        assert_eq!(number.error(), Some("Must be between 1 and 10"));
        number.handle_key(&key(BareKey::Backspace));
        assert_eq!(number.error(), None);
        assert_eq!(
            number.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Number(4)))
        );
        let mut at_least = NumberScreen::new(None, Some(3), None, 1, None);
        at_least.handle_key(&key(BareKey::Char('2')));
        assert_eq!(at_least.handle_key(&key(BareKey::Enter)), Step::Redraw);
        assert_eq!(at_least.error(), Some("Must be at least 3"));
        assert_eq!(
            range_error(0, None, Some(-1)),
            Some("Must be at most -1".to_owned())
        );
    }

    #[test]
    fn menu_prints_the_value_of_the_chosen_item() {
        let mut menu = MenuScreen::new(vec![
            ChoiceItem::labeled("open", "Open"),
            ChoiceItem::labeled("rm", "Delete"),
        ]);
        menu.handle_key(&key(BareKey::Down));
        assert_eq!(
            menu.handle_key(&key(BareKey::Enter)),
            Step::Done(Outcome::Answered(Answer::Choice("rm".to_owned())))
        );
    }

    fn levels_of(text: &Text) -> Vec<String> {
        let serialized = text.serialize();
        let mut parts: Vec<String> = serialized.split('$').map(|p| p.to_owned()).collect();
        parts.pop();
        parts
    }

    #[test]
    fn help_keys_use_emphasis_3_and_the_status_is_dimmed() {
        let text = hints_text(
            &[("<Enter>", "accept"), ("<Esc>", "cancel")],
            Some("2 ticked"),
            60,
        );
        assert!(text
            .content()
            .starts_with("<Enter> - accept, <Esc> - cancel"));
        assert!(text.content().ends_with("2 ticked"));
        let levels = levels_of(&text);
        assert_eq!(levels.len(), 5, "{:?}", levels);
        let indices = |level: &str| -> Vec<usize> {
            level
                .split(',')
                .filter(|i| !i.is_empty())
                .map(|i| i.parse().unwrap())
                .collect()
        };
        assert_eq!(
            indices(&levels[3]),
            vec![0, 1, 2, 3, 4, 5, 6, 18, 19, 20, 21, 22]
        );
        assert_eq!(indices(&levels[4]), (52..60).collect::<Vec<_>>());
    }

    #[test]
    fn help_entries_that_do_not_fit_are_dropped_from_the_end() {
        let text = hints_text(&[("<Enter>", "accept"), ("<Esc>", "cancel")], None, 20);
        assert_eq!(text.content(), "<Enter> - accept");
    }

    #[test]
    fn toggle_layout_has_the_label_a_blank_line_the_switch_and_a_blank_line() {
        let toggle = ToggleScreen::new(Some("Enable CI".to_owned()), true);
        assert_eq!(toggle.desired_size().1, 4);
    }

    #[test]
    fn long_text_is_cut_to_fit() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
