use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use zellij_tile::prelude::*;

use crate::pages::{Page, TextOrCustomRender};

#[derive(Debug)]
pub struct ActiveComponent {
    text_no_hover: TextOrCustomRender,
    text_hover: Option<TextOrCustomRender>,
    left_click_action: Option<ClickAction>,
    last_rendered_coordinates: Option<ComponentCoordinates>,
    keyboard_selectable: bool,
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
            keyboard_selectable: false,
        }
    }
    pub fn with_hover(mut self, text_hover: TextOrCustomRender) -> Self {
        self.text_hover = Some(text_hover);
        self
    }
    pub fn keyboard_selectable(mut self) -> Self {
        self.keyboard_selectable = true;
        self
    }
    pub fn is_keyboard_selectable(&self) -> bool {
        self.keyboard_selectable
    }
    pub fn with_left_click_action(mut self, left_click_action: ClickAction) -> Self {
        self.left_click_action = Some(left_click_action);
        self
    }
    pub fn render(
        &mut self,
        x: usize,
        y: usize,
        rows: usize,
        columns: usize,
        extend_hit_area_to_end_of_line: bool,
    ) -> usize {
        // Links are also opened by the terminal showing Zellij, on the user's
        // own machine. Our own click handler can't open them on every system,
        // nor where the user is when the session is remote.
        let link = self.link_destination();
        let mut component_width = 0;
        match self.text_hover.as_mut() {
            Some(text) if self.is_active => {
                let text_len = text.render(x, y, rows, columns, link.as_deref());
                component_width += text_len;
            },
            _ => {
                let text_len = self
                    .text_no_hover
                    .render(x, y, rows, columns, link.as_deref());
                component_width += text_len;
            },
        }
        let hit_area_width = if extend_hit_area_to_end_of_line {
            columns
        } else {
            component_width
        };
        self.last_rendered_coordinates = Some(ComponentCoordinates::new(x, y, 1, hit_area_width));
        component_width
    }
    fn link_destination(&self) -> Option<String> {
        match &self.left_click_action {
            Some(ClickAction::OpenLink(destination, _executable))
                if !destination.chars().any(char::is_control) =>
            {
                Some(destination.clone())
            },
            _ => None,
        }
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
            Some(ClickAction::Run(effect)) => {
                effect();
                self.left_click_action = Some(ClickAction::Run(effect));
                None
            },
            None => None,
        }
    }
    pub fn handle_left_click_at_position(&mut self, x: usize, y: usize) -> Option<Page> {
        let Some(last_rendered_coordinates) = &self.last_rendered_coordinates else {
            return None;
        };
        if !last_rendered_coordinates.contains(x, y) {
            return None;
        }
        if self.leaves_click_to_terminal(&mouse_modifiers()) {
            return None;
        }
        self.left_click_action()
    }
    // A click with a modifier held (e.g. Ctrl+click) is the terminal's own
    // gesture for opening the link it is drawn with, so we don't open it a
    // second time. A plain click still opens it ourselves.
    fn leaves_click_to_terminal(&self, modifiers: &BTreeSet<KeyModifier>) -> bool {
        !modifiers.is_empty() && self.link_destination().is_some()
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
    Run(Rc<dyn Fn()>),
}

impl std::fmt::Debug for ClickAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickAction::ChangePage(_) => write!(f, "ChangePage"),
            ClickAction::Run(_) => write!(f, "Run"),
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
    pub fn new_run<F>(effect: F) -> Self
    where
        F: Fn() + 'static,
    {
        ClickAction::Run(Rc::new(effect))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "https://zellij.dev";

    fn link_component(destination: &str) -> ActiveComponent {
        ActiveComponent::new(TextOrCustomRender::Text(Text::from("docs"))).with_left_click_action(
            ClickAction::new_open_link(
                destination.to_owned(),
                Rc::new(RefCell::new("xdg-open".to_owned())),
            ),
        )
    }

    fn held(modifiers: &[KeyModifier]) -> BTreeSet<KeyModifier> {
        modifiers.iter().copied().collect()
    }

    #[test]
    fn a_plain_click_on_a_link_is_ours_to_open() {
        assert!(!link_component(URL).leaves_click_to_terminal(&held(&[])));
    }

    #[test]
    fn a_click_with_a_modifier_on_a_link_is_left_to_the_terminal() {
        for modifier in [
            KeyModifier::Ctrl,
            KeyModifier::Alt,
            KeyModifier::Shift,
            KeyModifier::Super,
        ] {
            assert!(
                link_component(URL).leaves_click_to_terminal(&held(&[modifier])),
                "{:?}",
                modifier
            );
        }
    }

    #[test]
    fn a_link_drawn_without_a_hyperlink_is_still_opened_on_a_modified_click() {
        let component = link_component("https://zellij.dev/\u{7}");
        assert!(!component.leaves_click_to_terminal(&held(&[KeyModifier::Ctrl])));
    }

    #[test]
    fn a_modified_click_on_something_other_than_a_link_still_acts() {
        let component = ActiveComponent::new(TextOrCustomRender::Text(Text::from("next")))
            .with_left_click_action(ClickAction::new_run(|| {}));
        assert!(!component.leaves_click_to_terminal(&held(&[KeyModifier::Ctrl])));
    }
}
