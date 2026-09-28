use super::Story;
use crate::frame::Frame;
use zellij_tile::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Table,
    NestedList,
    Ribbon,
}

pub struct ExistingStory {
    kind: Kind,
}

impl ExistingStory {
    pub fn new(kind: Kind) -> Self {
        ExistingStory { kind }
    }
    fn intro(&self) -> &'static str {
        match self.kind {
            Kind::Text => {
                "Styled text: emphasis levels 0-3, error, success, dim, selected, opaque, disabled."
            },
            Kind::Table => "A table with a title row and selected cells.",
            Kind::NestedList => "An indented list with selected items.",
            Kind::Ribbon => "Ribbons: normal, selected and disabled.",
        }
    }
    fn body_height(&self) -> usize {
        match self.kind {
            Kind::Text => 8,
            Kind::Table => 6,
            Kind::NestedList => 6,
            Kind::Ribbon => 3,
        }
    }
    fn render_text(&self, frame: &Frame, row: usize) {
        let samples = vec![
            Text::new("Emphasis 0, emphasis 1, emphasis 2, emphasis 3")
                .color_substring(0, "Emphasis 0")
                .color_substring(1, "emphasis 1")
                .color_substring(2, "emphasis 2")
                .color_substring(3, "emphasis 3"),
            Text::new("Error and success colours")
                .error_color_substring("Error")
                .success_color_substring("success"),
            Text::new("Dimmed and unbolded parts")
                .dim_substring("Dimmed")
                .unbold_substring("unbolded"),
            Text::new("Selected text").selected(),
            Text::new("Opaque text").opaque(),
            Text::new("Disabled text").disabled(),
        ];
        for (index, text) in samples.into_iter().enumerate() {
            frame.text(row + index, 0, text);
        }
    }
    fn render_table(&self, frame: &Frame, row: usize) {
        if let Some(y) = frame.screen_y(row, 5) {
            let table = Table::new()
                .add_row(vec!["Element", "Primitive", "Helper"])
                .add_row(vec!["Menu list", "menu", "MenuList"])
                .add_styled_row(vec![
                    Text::new("Dropdown").selected(),
                    Text::new("dropdown").selected(),
                    Text::new("Dropdown").selected(),
                ])
                .add_row(vec!["Toggle", "toggle", "Toggle"])
                .add_row(vec!["Button", "button", "Button"]);
            print!(
                "{}",
                serialize_table_with_coordinates(&table, frame.x, y, Some(frame.width), None)
            );
        }
    }
    fn render_nested_list(&self, frame: &Frame, row: usize) {
        if let Some(y) = frame.screen_y(row, 5) {
            let items = vec![
                NestedListItem::new("Elements"),
                NestedListItem::new("Menu list").indent(1),
                NestedListItem::new("Dropdown").indent(1).selected(),
                NestedListItem::new("Open list").indent(2),
                NestedListItem::new("Focus helper"),
            ];
            print!(
                "{}",
                serialize_nested_list_with_coordinates(items, frame.x, y, Some(30), None)
            );
        }
    }
    fn render_ribbon(&self, frame: &Frame, row: usize) {
        if let Some(y) = frame.screen_y(row, 1) {
            print!(
                "{}",
                serialize_ribbon_line_with_coordinates(
                    vec![
                        Text::new("Normal"),
                        Text::new("Selected").selected(),
                        Text::new("Disabled").disabled()
                    ],
                    frame.x,
                    y,
                    None,
                    None,
                )
            );
        }
    }
}

impl Story for ExistingStory {
    fn title(&self) -> &'static str {
        match self.kind {
            Kind::Text => "Text",
            Kind::Table => "Table",
            Kind::NestedList => "Nested list",
            Kind::Ribbon => "Ribbon",
        }
    }
    fn layout(&mut self, _width: usize, _height: usize) -> usize {
        3 + self.body_height()
    }
    fn render(&mut self, frame: &Frame) {
        frame.heading(0, self.title());
        frame.note(1, 0, self.intro());
        match self.kind {
            Kind::Text => self.render_text(frame, 3),
            Kind::Table => self.render_table(frame, 3),
            Kind::NestedList => self.render_nested_list(frame, 3),
            Kind::Ribbon => self.render_ribbon(frame, 3),
        }
    }
    fn handle_key(&mut self, _key: &KeyWithModifier) -> Option<String> {
        None
    }
    fn handle_mouse(&mut self, _mouse: Mouse) -> Option<String> {
        None
    }
    fn focus_first(&mut self) {}
    fn blur(&mut self) {}
    fn focused_rows(&self) -> Option<(usize, usize)> {
        None
    }
}
