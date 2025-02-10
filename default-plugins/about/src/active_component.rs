use std::cell::RefCell;
use std::rc::Rc;
use zellij_tile::prelude::*;

use crate::pages::{Page, TextOrCustomRender};

#[derive(Debug)]
pub struct ActiveComponent {
    text_no_hover: TextOrCustomRender,
    text_hover: Option<TextOrCustomRender>,
    left_click_action: Option<ClickAction>,
    last_rendered_coordinates: Option<ComponentCoordinates>,
    pub is_active: bool,
}

impl ActiveComponent {
    pub fn new(text_no_hover: TextOrCustomRender) -> Self {
        ActiveComponent {
            text_no_hover,
            text_hover: None,
            left_click_action: None,
            is_active: false,
            last_rendered_coordinates: None,
        }
    }
    pub fn with_hover(mut self, text_hover: TextOrCustomRender) -> Self {
        self.text_hover = Some(text_hover);
        self
    }
    pub fn with_left_click_action(mut self, left_click_action: ClickAction) -> Self {
        self.left_click_action = Some(left_click_action);
        self
    }
    pub fn render(&mut self, x: usize, y: usize, rows: usize, columns: usize) -> usize {
        let mut component_width = 0;
        match self.text_hover.as_mut() {
            Some(text) if self.is_active => {
                let text_len = text.render(x, y, rows, columns);
                component_width += text_len;
            },
            _ => {
                let text_len = self.text_no_hover.render(x, y, rows, columns);
                component_width += text_len;
            },
        }
        self.last_rendered_coordinates = Some(ComponentCoordinates::new(x, y, 1, columns));
        component_width
    }
    pub fn left_click_action(&mut self) -> Option<Page> {
        match self.left_click_action.take() {
            Some(ClickAction::ChangePage(go_to_page)) => Some(go_to_page()),
            Some(ClickAction::OpenLink(link, executable)) => {
                self.left_click_action =
                    Some(ClickAction::OpenLink(link.clone(), executable.clone()));
                run_command(&[&executable.borrow(), &link], Default::default());
                None
            },
            None => None,
        }
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        let Some(last_rendered_coordinates) = &self.last_rendered_coordinates else {
            return None;
        };
        if last_rendered_coordinates.contains(x, y) {
            self.left_click_action()
        } else {
            None
        }
    }
    pub fn handle_hover_at_position(&mut self, x: usize, y: usize) -> bool {
        let Some(last_rendered_coordinates) = &self.last_rendered_coordinates else {
            return false;
        };
        if last_rendered_coordinates.contains(x, y) && self.text_hover.is_some() {
            self.is_active = true;
            true
        } else {
            false
        }
    }
    pub fn handle_selection(&mut self) -> Option<Page> {
        if self.is_active {
            self.left_click_action()
        } else {
            None
        }
    }
    pub fn column_count(&self) -> usize {
        match self.text_hover.as_ref() {
            Some(text) if self.is_active => text.len(),
            _ => self.text_no_hover.len(),
        }
    }
    pub fn clear_hover(&mut self) {
        self.is_active = false;
    }
}

#[derive(Debug)]
struct ComponentCoordinates {
    x: usize,
    y: usize,
    rows: usize,
    columns: usize,
}

impl ComponentCoordinates {
    pub fn contains(&self, x: usize, y: usize) -> bool {
        x >= self.x && x < self.x + self.columns && y >= self.y && y < self.y + self.rows
    }
}

impl ComponentCoordinates {
    pub fn new(x: usize, y: usize, rows: usize, columns: usize) -> Self {
        ComponentCoordinates {
            x,
            y,
            rows,
            columns,
        }
    }
}

pub enum ClickAction {
    ChangePage(Box<dyn FnOnce() -> Page>),
    OpenLink(String, Rc<RefCell<String>>), // (destination, executable)
}

impl std::fmt::Debug for ClickAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickAction::ChangePage(_) => write!(f, "ChangePage"),
            ClickAction::OpenLink(destination, executable) => {
                write!(f, "OpenLink: {}, {:?}", destination, executable)
            },
        }
    }
}

impl ClickAction {
    pub fn new_change_page<F>(go_to_page: F) -> Self
    where
        F: FnOnce() -> Page + 'static,
    {
        ClickAction::ChangePage(Box::new(go_to_page))
    }
    pub fn new_open_link(destination: String, executable: Rc<RefCell<String>>) -> Self {
        ClickAction::OpenLink(destination, executable)
    }
}
