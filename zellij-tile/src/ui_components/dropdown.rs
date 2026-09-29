use super::menu_list::{MenuItem, MenuList};
use super::widget_common::{
    default_label_width, encode_text, is_activation_key, is_plain, is_shift_tab, state_flag,
    state_value, update_hover, widget_dcs, Rect, UiResponse, UiValue, Widget,
};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

const MIN_FIELD_WIDTH: usize = 6;
const DEFAULT_MAX_LIST_ROWS: usize = 8;

#[derive(Debug, Clone)]
pub struct Dropdown {
    label: String,
    options: Vec<String>,
    selected: usize,
    label_width: Option<usize>,
    max_list_rows: usize,
    focused: bool,
    disabled: bool,
    open: bool,
    opens_upward: bool,
    hovered: bool,
    menu: MenuList,
    area: Option<Rect>,
    list_area: Option<Rect>,
}

impl Dropdown {
    pub fn new<S: Into<String>>(label: impl Into<String>, options: Vec<S>) -> Self {
        Dropdown {
            label: label.into(),
            options: options.into_iter().map(|o| o.into()).collect(),
            selected: 0,
            label_width: None,
            max_list_rows: DEFAULT_MAX_LIST_ROWS,
            focused: false,
            disabled: false,
            open: false,
            opens_upward: false,
            hovered: false,
            menu: MenuList::new(vec![]),
            area: None,
            list_area: None,
        }
    }
    pub fn selected(mut self, index: usize) -> Self {
        self.set_selected(index);
        self
    }
    pub fn label_width(mut self, label_width: usize) -> Self {
        self.label_width = Some(label_width);
        self
    }
    pub fn max_list_rows(mut self, rows: usize) -> Self {
        self.max_list_rows = rows.max(1);
        self
    }
    pub fn focused(mut self) -> Self {
        self.focused = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn hovered(mut self) -> Self {
        self.hovered = true;
        self
    }
    pub fn opened(mut self) -> Self {
        self.open();
        self
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn options(&self) -> &[String] {
        &self.options
    }
    pub fn set_options<S: Into<String>>(&mut self, options: Vec<S>) {
        self.options = options.into_iter().map(|o| o.into()).collect();
        self.selected = self.selected.min(self.options.len().saturating_sub(1));
        if self.open {
            self.open();
        }
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn selected_value(&self) -> Option<&str> {
        self.options.get(self.selected).map(|o| o.as_str())
    }
    pub fn set_selected(&mut self, index: usize) {
        if index < self.options.len() {
            self.selected = index;
        }
    }
    pub fn set_disabled(&mut self, disabled: bool) {
        self.disabled = disabled;
        if disabled {
            self.close();
        }
    }
    pub fn is_open(&self) -> bool {
        self.open
    }
    pub fn highlighted_index(&self) -> Option<usize> {
        if self.open {
            self.menu.highlighted_index()
        } else {
            None
        }
    }
    pub fn opens_upward(&self) -> bool {
        self.opens_upward
    }
    pub fn list_area(&self) -> Option<Rect> {
        if self.open {
            self.list_area
        } else {
            None
        }
    }
    pub fn open(&mut self) {
        if self.options.is_empty() {
            return;
        }
        let items = self
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let item = MenuItem::new(option.clone());
                if index == self.selected {
                    item.marked()
                } else {
                    item
                }
            })
            .collect();
        self.menu = MenuList::new(items)
            .with_border()
            .highlighted(self.selected)
            .focused();
        self.open = true;
    }
    pub fn close(&mut self) {
        self.open = false;
        self.list_area = None;
    }
    fn effective_label_width(&self) -> usize {
        self.label_width
            .unwrap_or_else(|| default_label_width(&self.label))
    }
    fn choice(&self) -> UiValue {
        UiValue::Choice {
            index: self.selected,
            label: self.options.get(self.selected).cloned().unwrap_or_default(),
        }
    }
    fn choose(&mut self, index: usize) -> UiResponse {
        self.set_selected(index);
        self.close();
        UiResponse::Changed(self.choice())
    }
    fn step(&mut self, forward: bool) -> UiResponse {
        let next = if forward {
            (self.selected + 1).min(self.options.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        if next == self.selected {
            UiResponse::Consumed
        } else {
            self.selected = next;
            UiResponse::Changed(self.choice())
        }
    }
    pub fn serialize(&mut self, x: usize, y: usize, width: usize) -> String {
        let label_width = self.effective_label_width();
        let width = width.max(label_width + MIN_FIELD_WIDTH);
        self.area = Some(Rect::new(x, y, width, 1));
        let mut state = vec![];
        state_flag(&mut state, "f", self.focused);
        state_flag(&mut state, "d", self.disabled);
        state_flag(&mut state, "o", self.open);
        state_flag(&mut state, "h", self.hovered && !self.disabled);
        state_value(&mut state, "lw", label_width);
        widget_dcs(
            "dropdown",
            x,
            y,
            Some(width),
            Some(1),
            &state,
            &[
                encode_text(&self.label),
                encode_text(self.selected_value().unwrap_or("")),
            ],
        )
    }
    pub fn render(&mut self, x: usize, y: usize, width: usize) {
        print!("{}", self.serialize(x, y, width));
    }
    pub fn serialize_overlay(&mut self, rows: usize, cols: usize) -> String {
        let area = match (self.open, self.area) {
            (true, Some(area)) => area,
            _ => return String::new(),
        };
        let field_x = area.x + self.effective_label_width();
        let available_width = cols.saturating_sub(field_x);
        let list_width = (area.right().saturating_sub(field_x))
            .max(self.menu.natural_width())
            .min(available_width);
        if list_width < 3 {
            return String::new();
        }
        let wanted = self.options.len().min(self.max_list_rows) + 2;
        let below = rows.saturating_sub(area.bottom());
        let above = area.y;
        let (list_y, height) = if wanted <= below {
            (area.bottom(), wanted)
        } else if wanted <= above {
            (area.y - wanted, wanted)
        } else if above > below {
            (0, above)
        } else {
            (area.bottom(), below)
        };
        if height < 3 {
            return String::new();
        }
        self.opens_upward = list_y < area.y;
        let serialized = self.menu.serialize(field_x, list_y, list_width, height);
        self.list_area = self.menu.last_area();
        serialized
    }
    pub fn render_overlay(&mut self, rows: usize, cols: usize) {
        print!("{}", self.serialize_overlay(rows, cols));
    }
}

impl Widget for Dropdown {
    fn handle_key(&mut self, key: &KeyWithModifier) -> UiResponse {
        if self.disabled {
            return UiResponse::NotHandled;
        }
        if self.open {
            if is_plain(key, BareKey::Esc) {
                self.close();
                return UiResponse::Cancelled;
            }
            if is_plain(key, BareKey::Tab) || is_shift_tab(key) {
                self.close();
                return UiResponse::NotHandled;
            }
            if is_activation_key(key) {
                return match self.menu.highlighted_index() {
                    Some(index) => self.choose(index),
                    None => {
                        self.close();
                        UiResponse::Cancelled
                    },
                };
            }
            self.menu.handle_key(key);
            return UiResponse::Consumed;
        }
        if is_activation_key(key) {
            self.open();
            UiResponse::Consumed
        } else if is_plain(key, BareKey::Left) {
            self.step(false)
        } else if is_plain(key, BareKey::Right) {
            self.step(true)
        } else {
            UiResponse::NotHandled
        }
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> UiResponse {
        if self.disabled {
            return UiResponse::NotHandled;
        }
        if self.open {
            let in_list = |line: isize, column: usize, list: Option<Rect>| {
                list.map(|l| l.contains(line, column)).unwrap_or(false)
            };
            return match mouse {
                Mouse::LeftClick(line, column) if in_list(line, column, self.list_area) => {
                    match self.menu.handle_mouse(mouse) {
                        UiResponse::Submitted(UiValue::Choice { index, .. }) => self.choose(index),
                        _ => UiResponse::Consumed,
                    }
                },
                Mouse::LeftClick(..) | Mouse::RightClick(..) => {
                    self.close();
                    UiResponse::Cancelled
                },
                Mouse::Hover(line, column) | Mouse::Hold(line, column) => {
                    let on_field = self.area.map(|a| a.contains(line, column)).unwrap_or(false);
                    update_hover(&mut self.hovered, on_field);
                    self.menu.handle_mouse(mouse);
                    UiResponse::Consumed
                },
                Mouse::ScrollUp(_) | Mouse::ScrollDown(_) => self.menu.handle_mouse(mouse),
                _ => UiResponse::NotHandled,
            };
        }
        match mouse {
            Mouse::LeftClick(line, column) if self.hit_test(line, column) => {
                self.open();
                UiResponse::Consumed
            },
            Mouse::Hover(line, column) => {
                let hit = self.hit_test(line, column);
                update_hover(&mut self.hovered, hit)
            },
            _ => UiResponse::NotHandled,
        }
    }
    fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        if !focused {
            self.close();
        }
    }
    fn is_focused(&self) -> bool {
        self.focused
    }
    fn is_disabled(&self) -> bool {
        self.disabled
    }
    fn last_area(&self) -> Option<Rect> {
        self.area
    }
    fn clear_area(&mut self) {
        self.area = None;
    }
    fn hit_test(&self, line: isize, column: usize) -> bool {
        self.area.map(|a| a.contains(line, column)).unwrap_or(false)
            || self
                .list_area()
                .map(|a| a.contains(line, column))
                .unwrap_or(false)
    }
    fn captures_input(&self) -> bool {
        self.open
    }
}
