use super::button::Button;
use super::dropdown::Dropdown;
use super::menu_list::MenuList;
use super::number_stepper::NumberStepper;
use super::side_menu::SideMenu;
use super::text_input::TextInput;
use super::toggle::Toggle;
use super::widget_common::{is_plain, is_shift_tab, Rect, UiResponse, Widget};
use zellij_utils::data::{BareKey, KeyWithModifier, Mouse};

#[derive(Debug, Clone)]
pub enum Element {
    Button(Button),
    Toggle(Toggle),
    Dropdown(Dropdown),
    TextInput(TextInput),
    NumberStepper(NumberStepper),
    MenuList(MenuList),
    SideMenu(SideMenu),
}

macro_rules! element_conversions {
    ($($variant:ident, $as_ref:ident, $as_mut:ident);* $(;)?) => {
        $(
            impl From<$variant> for Element {
                fn from(widget: $variant) -> Self {
                    Element::$variant(widget)
                }
            }
        )*
        impl Element {
            $(
                pub fn $as_ref(&self) -> Option<&$variant> {
                    match self {
                        Element::$variant(widget) => Some(widget),
                        _ => None,
                    }
                }
                pub fn $as_mut(&mut self) -> Option<&mut $variant> {
                    match self {
                        Element::$variant(widget) => Some(widget),
                        _ => None,
                    }
                }
            )*
            pub fn widget(&self) -> &dyn Widget {
                match self {
                    $(Element::$variant(widget) => widget,)*
                }
            }
            pub fn widget_mut(&mut self) -> &mut dyn Widget {
                match self {
                    $(Element::$variant(widget) => widget,)*
                }
            }
        }
    };
}

element_conversions!(
    Button, as_button, as_button_mut;
    Toggle, as_toggle, as_toggle_mut;
    Dropdown, as_dropdown, as_dropdown_mut;
    TextInput, as_text_input, as_text_input_mut;
    NumberStepper, as_number_stepper, as_number_stepper_mut;
    MenuList, as_menu_list, as_menu_list_mut;
    SideMenu, as_side_menu, as_side_menu_mut;
);

#[derive(Debug, Clone, PartialEq)]
pub enum FocusEvent<K> {
    Element { key: K, response: UiResponse },
    FocusChanged(K),
    NotHandled,
}

impl<K> FocusEvent<K> {
    pub fn is_handled(&self) -> bool {
        !matches!(self, FocusEvent::NotHandled)
    }
}

#[derive(Debug, Clone)]
pub struct FocusGroup<K> {
    entries: Vec<(K, Element)>,
    focused: Option<usize>,
    wrap: bool,
}

impl<K> Default for FocusGroup<K> {
    fn default() -> Self {
        FocusGroup {
            entries: vec![],
            focused: None,
            wrap: true,
        }
    }
}

macro_rules! typed_access {
    ($($variant:ident, $get:ident, $get_mut:ident, $as_ref:ident, $as_mut:ident);* $(;)?) => {
        $(
            pub fn $get(&self, key: &K) -> Option<&$variant> {
                self.get(key).and_then(|element| element.$as_ref())
            }
            pub fn $get_mut(&mut self, key: &K) -> Option<&mut $variant> {
                self.get_mut(key).and_then(|element| element.$as_mut())
            }
        )*
    };
}

impl<K: Clone + PartialEq> FocusGroup<K> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, key: K, element: impl Into<Element>) -> Self {
        self.add(key, element);
        self
    }
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.wrap = wrap;
        self
    }
    pub fn add(&mut self, key: K, element: impl Into<Element>) {
        self.entries.push((key, element.into()));
    }
    pub fn remove(&mut self, key: &K) -> Option<Element> {
        let index = self.index_of(key)?;
        let focused_key = self.focused_key().cloned();
        let (_, element) = self.entries.remove(index);
        self.focused = focused_key.and_then(|k| self.index_of(&k));
        Some(element)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn keys(&self) -> Vec<K> {
        self.entries.iter().map(|(k, _)| k.clone()).collect()
    }
    pub fn get(&self, key: &K) -> Option<&Element> {
        self.entries
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, element)| element)
    }
    pub fn get_mut(&mut self, key: &K) -> Option<&mut Element> {
        self.entries
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, element)| element)
    }
    typed_access!(
        Button, button, button_mut, as_button, as_button_mut;
        Toggle, toggle, toggle_mut, as_toggle, as_toggle_mut;
        Dropdown, dropdown, dropdown_mut, as_dropdown, as_dropdown_mut;
        TextInput, text_input, text_input_mut, as_text_input, as_text_input_mut;
        NumberStepper, number_stepper, number_stepper_mut, as_number_stepper, as_number_stepper_mut;
        MenuList, menu_list, menu_list_mut, as_menu_list, as_menu_list_mut;
        SideMenu, side_menu, side_menu_mut, as_side_menu, as_side_menu_mut;
    );
    fn index_of(&self, key: &K) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k == key)
    }
    pub fn focused_key(&self) -> Option<&K> {
        self.focused
            .and_then(|index| self.entries.get(index))
            .map(|(key, _)| key)
    }
    pub fn is_focused(&self, key: &K) -> bool {
        self.focused_key() == Some(key)
    }
    fn set_focus_index(&mut self, index: Option<usize>) {
        if self.focused == index {
            return;
        }
        if let Some((_, element)) = self.focused.and_then(|i| self.entries.get_mut(i)) {
            element.widget_mut().set_focused(false);
        }
        self.focused = index;
        if let Some((_, element)) = index.and_then(|i| self.entries.get_mut(i)) {
            element.widget_mut().set_focused(true);
        }
    }
    pub fn focus(&mut self, key: &K) -> bool {
        match self.index_of(key) {
            Some(index) if !self.entries[index].1.widget().is_disabled() => {
                self.set_focus_index(Some(index));
                true
            },
            _ => false,
        }
    }
    pub fn blur(&mut self) {
        self.set_focus_index(None);
    }
    pub fn focus_first(&mut self) -> bool {
        let first = self
            .entries
            .iter()
            .position(|(_, element)| !element.widget().is_disabled());
        self.set_focus_index(first);
        first.is_some()
    }
    fn next_focusable(&self, forward: bool) -> Option<usize> {
        let count = self.entries.len();
        if count == 0 {
            return None;
        }
        let start = match self.focused {
            Some(index) => index,
            None => {
                return if forward {
                    (0..count).find(|i| !self.entries[*i].1.widget().is_disabled())
                } else {
                    (0..count)
                        .rev()
                        .find(|i| !self.entries[*i].1.widget().is_disabled())
                };
            },
        };
        for step in 1..=count {
            let candidate = if forward {
                let next = start + step;
                if next >= count && !self.wrap {
                    return None;
                }
                next % count
            } else {
                if step > start && !self.wrap {
                    return None;
                }
                (start + count - step % count) % count
            };
            if !self.entries[candidate].1.widget().is_disabled() {
                return Some(candidate);
            }
        }
        None
    }
    pub fn focus_next(&mut self) -> bool {
        match self.next_focusable(true) {
            Some(index) => {
                self.set_focus_index(Some(index));
                true
            },
            None => false,
        }
    }
    pub fn focus_prev(&mut self) -> bool {
        match self.next_focusable(false) {
            Some(index) => {
                self.set_focus_index(Some(index));
                true
            },
            None => false,
        }
    }
    pub fn clear_areas(&mut self) {
        for (_, element) in self.entries.iter_mut() {
            element.widget_mut().clear_area();
        }
    }
    pub fn capturing_key(&self) -> Option<&K> {
        self.entries
            .iter()
            .find(|(_, element)| element.widget().captures_input())
            .map(|(key, _)| key)
    }
    pub fn has_open_overlay(&self) -> bool {
        self.capturing_key().is_some()
    }
    pub fn serialize_overlays(&mut self, rows: usize, cols: usize) -> String {
        let mut serialized = String::new();
        for (_, element) in self.entries.iter_mut() {
            if let Element::Dropdown(dropdown) = element {
                if dropdown.is_open() && dropdown.last_area().is_none() {
                    dropdown.close();
                }
                serialized.push_str(&dropdown.serialize_overlay(rows, cols));
            }
        }
        serialized
    }
    pub fn render_overlays(&mut self, rows: usize, cols: usize) {
        print!("{}", self.serialize_overlays(rows, cols));
    }
    pub fn element_at(&self, line: isize, column: usize) -> Option<&K> {
        self.index_at(line, column).map(|i| &self.entries[i].0)
    }
    fn index_at(&self, line: isize, column: usize) -> Option<usize> {
        self.entries
            .iter()
            .position(|(_, element)| element.widget().hit_test(line, column))
    }
    fn respond(&mut self, index: usize, response: UiResponse) -> FocusEvent<K> {
        FocusEvent::Element {
            key: self.entries[index].0.clone(),
            response,
        }
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> FocusEvent<K> {
        if let Some(index) = self.focused {
            let response = self.entries[index].1.widget_mut().handle_key(key);
            if response.is_handled() {
                return self.respond(index, response);
            }
        }
        let moved = if is_plain(key, BareKey::Tab) {
            self.focus_next()
        } else if is_shift_tab(key) {
            self.focus_prev()
        } else {
            return FocusEvent::NotHandled;
        };
        match (moved, self.focused_key()) {
            (true, Some(key)) => FocusEvent::FocusChanged(key.clone()),
            _ => FocusEvent::NotHandled,
        }
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> FocusEvent<K> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(_, element)| element.widget().captures_input())
        {
            let response = self.entries[index].1.widget_mut().handle_mouse(mouse);
            if response.is_handled() {
                return self.respond(index, response);
            }
        }
        match mouse {
            Mouse::LeftClick(line, column) => {
                let index = match self.index_at(line, column) {
                    Some(index) => index,
                    None => return FocusEvent::NotHandled,
                };
                let focus_changed = if self.entries[index].1.widget().is_disabled() {
                    false
                } else {
                    let changed = self.focused != Some(index);
                    self.set_focus_index(Some(index));
                    changed
                };
                let response = self.entries[index].1.widget_mut().handle_mouse(mouse);
                if response.is_handled() || !focus_changed {
                    self.respond(index, response)
                } else {
                    FocusEvent::FocusChanged(self.entries[index].0.clone())
                }
            },
            Mouse::Hover(..)
            | Mouse::Release(..)
            | Mouse::Hold(..)
            | Mouse::ScrollUp(_)
            | Mouse::ScrollDown(_) => {
                let stop_at_first = matches!(mouse, Mouse::ScrollUp(_) | Mouse::ScrollDown(_));
                let mut result = FocusEvent::NotHandled;
                for index in 0..self.entries.len() {
                    let response = self.entries[index].1.widget_mut().handle_mouse(mouse);
                    if response.is_handled() && !result.is_handled() {
                        result = self.respond(index, response);
                        if stop_at_first {
                            break;
                        }
                    }
                }
                result
            },
            _ => FocusEvent::NotHandled,
        }
    }
    pub fn handle_timer(&mut self) -> bool {
        let mut changed = false;
        for (_, element) in self.entries.iter_mut() {
            changed |= element.widget_mut().handle_timer();
        }
        changed
    }
    pub fn area_of(&self, key: &K) -> Option<Rect> {
        self.get(key)
            .and_then(|element| element.widget().last_area())
    }
}
