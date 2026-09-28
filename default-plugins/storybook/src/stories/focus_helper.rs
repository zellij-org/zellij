use super::{Base, Story};
use crate::frame::Frame;
use zellij_tile::prelude::*;

const LABEL_WIDTH: usize = 10;

pub struct FocusHelperStory {
    base: Base,
}

impl FocusHelperStory {
    pub fn new() -> Self {
        let base = Base::new("Focus helper")
            .intro(vec![
                "FocusGroup keeps the order of the elements below. Tab / Shift+Tab move focus",
                "(the disabled button is skipped), keys go to the focused element, and a click",
                "goes to whatever element was drawn at that spot and focuses it.",
                "An open dropdown gets clicks first, so its list can cover other elements.",
            ])
            .live(
                "Name",
                TextInput::empty()
                    .label("Name")
                    .label_width(LABEL_WIDTH)
                    .placeholder("type here"),
                36,
                1,
            )
            .live(
                "Enabled",
                Toggle::new("Enabled", true).label_width(LABEL_WIDTH),
                20,
                1,
            )
            .live(
                "Color",
                Dropdown::new("Color", vec!["red", "green", "blue", "yellow", "magenta"])
                    .label_width(LABEL_WIDTH),
                30,
                1,
            )
            .live(
                "Size",
                NumberStepper::new("Size", 12)
                    .min(6)
                    .max(48)
                    .label_width(LABEL_WIDTH),
                30,
                1,
            )
            .live("Skipped", Button::new("Skipped").disabled(), 12, 1)
            .live("Apply", Button::new("Apply"), 12, 1)
            .extra_rows(2);
        FocusHelperStory { base }
    }
}

impl Story for FocusHelperStory {
    fn title(&self) -> &'static str {
        self.base.title
    }
    fn layout(&mut self, width: usize, height: usize) -> usize {
        self.base.layout(width, height)
    }
    fn render(&mut self, frame: &Frame) {
        self.base.render(frame);
        let order = self
            .base
            .group
            .keys()
            .iter()
            .map(|key| {
                if self.base.group.is_focused(key) {
                    format!("[{}]", key)
                } else {
                    key.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" → ");
        frame.note(self.base.extra_row, 0, &format!("Focus order: {}", order));
    }
    fn render_overlays(&mut self, frame: &Frame) {
        self.base.render_overlays(frame);
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        self.base.handle_key(key)
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        self.base.handle_mouse(mouse)
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
