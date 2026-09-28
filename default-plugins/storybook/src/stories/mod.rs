mod button;
mod confirm_dialog;
mod dropdown;
mod existing;
mod focus_helper;
mod menu_list;
mod number_stepper;
mod scroll_view;
mod side_menu;
mod text_input;
mod toggle;

use crate::frame::{Flow, Frame};
use zellij_tile::prelude::*;

pub trait Story {
    fn title(&self) -> &'static str;
    fn layout(&mut self, width: usize, height: usize) -> usize;
    fn render(&mut self, frame: &Frame);
    fn render_overlays(&mut self, _frame: &Frame) {}
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String>;
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String>;
    fn focus_first(&mut self);
    fn blur(&mut self);
    fn focused_rows(&self) -> Option<(usize, usize)>;
    fn is_modal(&self) -> bool {
        false
    }
    fn handle_timer(&mut self) -> bool {
        false
    }
}

pub fn all_stories() -> Vec<Box<dyn Story>> {
    vec![
        Box::new(menu_list::story()),
        Box::new(dropdown::story()),
        Box::new(toggle::story()),
        Box::new(text_input::TextInputStory::new()),
        Box::new(number_stepper::story()),
        Box::new(scroll_view::ScrollViewStory::new()),
        Box::new(side_menu::story()),
        Box::new(confirm_dialog::ConfirmDialogStory::new()),
        Box::new(button::ButtonStory::new()),
        Box::new(focus_helper::FocusHelperStory::new()),
        Box::new(existing::ExistingStory::new(existing::Kind::Text)),
        Box::new(existing::ExistingStory::new(existing::Kind::Table)),
        Box::new(existing::ExistingStory::new(existing::Kind::NestedList)),
        Box::new(existing::ExistingStory::new(existing::Kind::Ribbon)),
    ]
}

pub fn describe(event: FocusEvent<&'static str>) -> Option<String> {
    match event {
        FocusEvent::Element { key, response } => Some(format!("{} → {}", key, response)),
        FocusEvent::FocusChanged(key) => Some(format!("Focus → {}", key)),
        FocusEvent::NotHandled => None,
    }
}

pub fn leaves_story(
    key: &KeyWithModifier,
    had_overlay: bool,
    event: &FocusEvent<&'static str>,
) -> bool {
    !had_overlay
        && key.is_key_without_modifier(BareKey::Esc)
        && matches!(
            event,
            FocusEvent::Element {
                response: UiResponse::Cancelled,
                ..
            }
        )
}

pub fn draw_element(element: &mut Element, x: usize, y: usize, width: usize, height: usize) {
    match element {
        Element::Button(button) => button.render(x, y),
        Element::Toggle(toggle) => toggle.render(x, y),
        Element::Dropdown(dropdown) => dropdown.render(x, y, width),
        Element::TextInput(input) => input.render(x, y, width),
        Element::NumberStepper(stepper) => stepper.render(x, y),
        Element::MenuList(menu) => menu.render(x, y, width, height),
        Element::SideMenu(menu) => menu.render(x, y, width, height),
    }
}

pub fn element_height(element: &Element, max_height: usize) -> usize {
    match element {
        Element::TextInput(input) => input.height(),
        Element::MenuList(menu) => menu.height_for(max_height),
        Element::SideMenu(_) => max_height,
        _ => 1,
    }
}

pub struct Sample {
    pub caption: &'static str,
    pub width: usize,
    pub height: usize,
    pub draw: Box<dyn Fn(usize, usize) -> String>,
}

pub fn sample(
    caption: &'static str,
    width: usize,
    height: usize,
    draw: impl Fn(usize, usize) -> String + 'static,
) -> Sample {
    Sample {
        caption,
        width,
        height,
        draw: Box::new(draw),
    }
}

pub struct Slot {
    pub key: &'static str,
    pub width: usize,
    pub max_height: usize,
    pub pin_bottom: bool,
    pub row: usize,
    pub height: usize,
}

pub struct Base {
    pub title: &'static str,
    pub intro: Vec<&'static str>,
    pub samples: Vec<Sample>,
    pub group: FocusGroup<&'static str>,
    pub slots: Vec<Slot>,
    pub extra_rows: usize,
    pub extra_row: usize,
    sample_positions: Vec<(usize, usize)>,
    looks_row: usize,
    live_row: usize,
    height: usize,
}

impl Base {
    pub fn new(title: &'static str) -> Self {
        Base {
            title,
            intro: vec![],
            samples: vec![],
            group: FocusGroup::new(),
            slots: vec![],
            extra_rows: 0,
            extra_row: 0,
            sample_positions: vec![],
            looks_row: 0,
            live_row: 0,
            height: 0,
        }
    }
    pub fn intro(mut self, lines: Vec<&'static str>) -> Self {
        self.intro = lines;
        self
    }
    pub fn sample(mut self, sample: Sample) -> Self {
        self.samples.push(sample);
        self
    }
    pub fn live(
        mut self,
        key: &'static str,
        element: impl Into<Element>,
        width: usize,
        max_height: usize,
    ) -> Self {
        self.group.add(key, element);
        self.slots.push(Slot {
            key,
            width,
            max_height,
            pin_bottom: false,
            row: 0,
            height: 1,
        });
        self
    }
    pub fn live_at_bottom(
        mut self,
        key: &'static str,
        element: impl Into<Element>,
        width: usize,
    ) -> Self {
        self = self.live(key, element, width, 1);
        if let Some(slot) = self.slots.last_mut() {
            slot.pin_bottom = true;
        }
        self
    }
    pub fn extra_rows(mut self, rows: usize) -> Self {
        self.extra_rows = rows;
        self
    }
    pub fn slot(&self, key: &'static str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.key == key)
    }
    pub fn layout(&mut self, width: usize, height: usize) -> usize {
        let mut row = 1 + self.intro.len() + 1;
        self.sample_positions.clear();
        if !self.samples.is_empty() {
            self.looks_row = row;
            let mut flow = Flow::new(row + 1, width, 3);
            for sample in &self.samples {
                self.sample_positions
                    .push(flow.place(sample.width.min(width), sample.height + 1));
            }
            row = flow.end_row() + 1;
        }
        self.live_row = row;
        row += 1;
        for slot in self.slots.iter_mut() {
            let element = self.group.get(&slot.key);
            slot.height = element
                .map(|e| element_height(e, slot.max_height))
                .unwrap_or(1);
            if slot.pin_bottom {
                row = row.max(height.saturating_sub(slot.height + self.extra_rows));
            }
            slot.row = row;
            row += slot.height + 1;
        }
        self.extra_row = row;
        self.height = row + self.extra_rows;
        self.height
    }
    pub fn render(&mut self, frame: &Frame) {
        self.group.clear_areas();
        frame.heading(0, self.title);
        for (index, line) in self.intro.iter().enumerate() {
            frame.note(1 + index, 0, line);
        }
        if !self.samples.is_empty() {
            frame.heading(self.looks_row, "Looks (fixed copies)");
        }
        for (sample, (column, row)) in self.samples.iter().zip(self.sample_positions.iter()) {
            frame.caption(*row, *column, sample.caption);
            if let Some(y) = frame.screen_y(row + 1, sample.height) {
                if column + sample.width <= frame.width {
                    print!("{}", (sample.draw)(frame.x + column, y));
                }
            }
        }
        if !self.slots.is_empty() {
            frame.heading(self.live_row, "Live (use Tab, keys and the mouse)");
        }
        for slot in &self.slots {
            if let (Some(y), Some(element)) = (
                frame.screen_y(slot.row, slot.height),
                self.group.get_mut(&slot.key),
            ) {
                draw_element(
                    element,
                    frame.x,
                    y,
                    slot.width.min(frame.width),
                    slot.max_height,
                );
            }
        }
    }
    pub fn render_overlays(&mut self, frame: &Frame) {
        self.group.render_overlays(frame.bottom(), frame.pane_cols);
    }
    pub fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        let had_overlay = self.group.has_open_overlay();
        let event = self.group.handle_key(key);
        if leaves_story(key, had_overlay, &event) {
            return None;
        }
        describe(event)
    }
    pub fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        describe(self.group.handle_mouse(mouse))
    }
    pub fn focus_first(&mut self) {
        self.group.focus_first();
    }
    pub fn blur(&mut self) {
        self.group.blur();
    }
    pub fn focused_rows(&self) -> Option<(usize, usize)> {
        let key = self.group.focused_key()?;
        self.slot(key).map(|slot| (slot.row, slot.height))
    }
}

impl Story for Base {
    fn title(&self) -> &'static str {
        self.title
    }
    fn layout(&mut self, width: usize, height: usize) -> usize {
        Base::layout(self, width, height)
    }
    fn render(&mut self, frame: &Frame) {
        Base::render(self, frame)
    }
    fn render_overlays(&mut self, frame: &Frame) {
        Base::render_overlays(self, frame)
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        Base::handle_key(self, key)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        Base::handle_mouse(self, mouse)
    }
    fn focus_first(&mut self) {
        Base::focus_first(self)
    }
    fn blur(&mut self) {
        Base::blur(self)
    }
    fn focused_rows(&self) -> Option<(usize, usize)> {
        Base::focused_rows(self)
    }
    fn handle_timer(&mut self) -> bool {
        self.group.handle_timer()
    }
}
