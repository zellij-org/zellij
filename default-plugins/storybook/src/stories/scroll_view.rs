use super::Story;
use crate::frame::{Flow, Frame};
use zellij_tile::prelude::*;

const TOTAL_ROWS: usize = 60;
const LIST_HEIGHT: usize = 12;
const LIST_WIDTH: usize = 30;
const SAMPLE_ROWS: usize = 30;
const SAMPLE_HEIGHT: usize = 8;
const SAMPLE_WIDTH: usize = 18;
const INTRO: [&str; 3] = [
    "Keeps the scroll position for rows taller than their space and shows how many rows are",
    "hidden above and below. The plugin draws only the visible rows. The mouse wheel scrolls",
    "while hovering, a click on ↑/↓ pages. When focused, Up/Down move the highlight, PgUp/PgDn page.",
];

pub struct ScrollViewStory {
    view: ScrollView,
    highlighted: usize,
    focused: bool,
    sample_positions: Vec<(usize, usize)>,
    live_heading_row: usize,
    list_row: usize,
}

impl ScrollViewStory {
    pub fn new() -> Self {
        ScrollViewStory {
            view: ScrollView::new(TOTAL_ROWS),
            highlighted: 0,
            focused: false,
            sample_positions: vec![],
            live_heading_row: 0,
            list_row: 0,
        }
    }
    fn status(&self) -> String {
        format!(
            "Scroll view → row {} highlighted, offset {}",
            self.highlighted + 1,
            self.view.offset()
        )
    }
    fn move_highlight(&mut self, row: usize) {
        self.highlighted = row.min(TOTAL_ROWS - 1);
        self.view.ensure_visible(self.highlighted);
    }
    fn keep_highlight_in_view(&mut self) {
        let range = self.view.visible_range();
        if range.is_empty() {
            return;
        }
        if self.highlighted < range.start {
            self.highlighted = range.start;
        } else if self.highlighted >= range.end {
            self.highlighted = range.end - 1;
        }
    }
    fn serialize_rows(view: &ScrollView, x: usize, highlighted: Option<usize>) -> String {
        let mut serialized = String::new();
        let width = view.content_width();
        for row in view.visible_range() {
            let mut text = Text::new(format!(" Row {:>2}", row + 1));
            if Some(row) == highlighted {
                text = text.selected();
            }
            if let Some(y) = view.screen_row(row) {
                serialized.push_str(&serialize_text_with_coordinates(
                    &text,
                    x,
                    y,
                    Some(width),
                    None,
                ));
            }
        }
        serialized.push_str(&view.serialize_indicators());
        serialized
    }
}

impl Story for ScrollViewStory {
    fn title(&self) -> &'static str {
        "Scroll view"
    }
    fn layout(&mut self, width: usize, _height: usize) -> usize {
        let mut flow = Flow::new(INTRO.len() + 3, width, 3);
        self.sample_positions = (0..3)
            .map(|_| flow.place(SAMPLE_WIDTH, SAMPLE_HEIGHT + 1))
            .collect();
        self.live_heading_row = flow.end_row() + 1;
        self.list_row = self.live_heading_row + 1;
        self.list_row + LIST_HEIGHT + 1
    }
    fn render(&mut self, frame: &Frame) {
        frame.heading(0, "Scroll view");
        for (index, line) in INTRO.iter().enumerate() {
            frame.note(1 + index, 0, line);
        }
        frame.heading(INTRO.len() + 2, "Looks (fixed copies)");
        let captions = ["At the top", "In the middle", "At the bottom"];
        let offsets = [0, SAMPLE_ROWS / 2 - 3, SAMPLE_ROWS];
        for (index, (column, row)) in self.sample_positions.iter().enumerate() {
            frame.caption(*row, *column, captions[index]);
            if let Some(y) = frame.screen_y(row + 1, SAMPLE_HEIGHT) {
                let mut view = ScrollView::new(SAMPLE_ROWS);
                view.layout(frame.x + column, y, SAMPLE_WIDTH, SAMPLE_HEIGHT);
                view.set_offset(offsets[index]);
                print!("{}", Self::serialize_rows(&view, frame.x + column, None));
            }
        }
        frame.heading(
            self.live_heading_row,
            "Live (hover and use the wheel, or Tab / click to focus)",
        );
        self.view.clear_area();
        if let Some(y) = frame.screen_y(self.list_row, LIST_HEIGHT) {
            self.view.set_focused(self.focused);
            self.view
                .layout(frame.x, y, LIST_WIDTH.min(frame.width), LIST_HEIGHT);
            let highlighted = if self.focused {
                Some(self.highlighted)
            } else {
                None
            };
            print!("{}", Self::serialize_rows(&self.view, frame.x, highlighted));
        }
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        if !self.focused {
            return None;
        }
        if key.is_key_without_modifier(BareKey::Up) {
            self.move_highlight(self.highlighted.saturating_sub(1));
        } else if key.is_key_without_modifier(BareKey::Down) {
            self.move_highlight(self.highlighted + 1);
        } else if key.is_key_without_modifier(BareKey::Home) {
            self.move_highlight(0);
        } else if key.is_key_without_modifier(BareKey::End) {
            self.move_highlight(TOTAL_ROWS - 1);
        } else {
            let response = self.view.handle_key(key);
            if !response.is_handled() {
                return None;
            }
            self.keep_highlight_in_view();
            return Some(format!("Scroll view → {}", response));
        }
        Some(self.status())
    }
    fn handle_mouse(&mut self, mouse: Mouse) -> Option<String> {
        let response = self.view.handle_mouse(mouse);
        match mouse {
            Mouse::Hover(..) => None,
            Mouse::ScrollUp(_) | Mouse::ScrollDown(_) if response.is_handled() => {
                self.keep_highlight_in_view();
                Some(format!("Scroll view → {}", response))
            },
            Mouse::LeftClick(line, column) if self.view.hit_test(line, column) => {
                self.focused = true;
                if response.is_handled() {
                    self.keep_highlight_in_view();
                    return Some(format!("Scroll view → {}", response));
                }
                let row = self.view.row_at(line)?;
                self.highlighted = row;
                Some(self.status())
            },
            _ => None,
        }
    }
    fn focus_first(&mut self) {
        self.focused = true;
    }
    fn blur(&mut self) {
        self.focused = false;
    }
    fn focused_rows(&self) -> Option<(usize, usize)> {
        if self.focused {
            Some((self.list_row, LIST_HEIGHT))
        } else {
            None
        }
    }
}
