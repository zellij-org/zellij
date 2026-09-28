use super::{sample, Base, Story};
use crate::frame::Frame;
use zellij_tile::prelude::*;

pub struct ButtonStory {
    base: Base,
    activations: Vec<(&'static str, usize)>,
}

impl ButtonStory {
    pub fn new() -> Self {
        let base = Base::new("Button")
            .intro(vec![
                "A clickable label. Enter, Space or a click activates it. Hovering shows it can be clicked.",
                "A click shows it pressed for 400ms (repeated clicks restart the timer).",
            ])
            .sample(sample("Normal", 10, 1, |x, y| {
                Button::new("Save").serialize(x, y)
            }))
            .sample(sample("Hovered", 10, 1, |x, y| {
                Button::new("Save").hovered().serialize(x, y)
            }))
            .sample(sample("Focused", 10, 1, |x, y| {
                Button::new("Save").focused().serialize(x, y)
            }))
            .sample(sample("Pressed", 10, 1, |x, y| {
                Button::new("Save").pressed().serialize(x, y)
            }))
            .sample(sample("Disabled", 10, 1, |x, y| {
                Button::new("Save").disabled().serialize(x, y)
            }))
            .live("Save", Button::new("Save"), 10, 1)
            .live("Cancel", Button::new("Cancel"), 10, 1)
            .live("Disabled", Button::new("Disabled").disabled(), 12, 1)
            .extra_rows(2);
        ButtonStory {
            base,
            activations: vec![("Save", 0), ("Cancel", 0)],
        }
    }
    fn count(&mut self, status: &Option<String>) {
        if let Some(status) = status {
            for (name, count) in self.activations.iter_mut() {
                if status == &format!("{} → Activated", name) {
                    *count += 1;
                }
            }
        }
    }
}

impl Story for ButtonStory {
    fn title(&self) -> &'static str {
        self.base.title
    }
    fn layout(&mut self, width: usize, height: usize) -> usize {
        self.base.layout(width, height)
    }
    fn render(&mut self, frame: &Frame) {
        self.base.render(frame);
        let summary = self
            .activations
            .iter()
            .map(|(name, count)| format!("{} activated {} times", name, count))
            .collect::<Vec<_>>()
            .join(", ");
        frame.note(self.base.extra_row, 0, &summary);
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        let status = self.base.handle_key(key);
        self.count(&status);
        status
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        let status = self.base.handle_mouse(mouse);
        self.count(&status);
        status
    }
    fn focus_first(&mut self) {
        self.base.focus_first();
    }
    fn blur(&mut self) {
        self.base.blur();
    }
    fn focused_rows(&self) -> Option<(usize, usize)> {
        self.base.focused_rows()
    }
    fn handle_timer(&mut self) -> bool {
        self.base.group.handle_timer()
    }
}
