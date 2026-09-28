use super::{sample, Base, Story};
use crate::frame::Frame;
use zellij_tile::prelude::*;

const LABEL_WIDTH: usize = 14;
const FIELD_WIDTH: usize = 44;
const SAMPLE_ITEMS: [&str; 12] = [
    "apple",
    "apricot",
    "banana",
    "blackberry",
    "blueberry",
    "cherry",
    "grape",
    "grapefruit",
    "lemon",
    "lime",
    "mango",
    "pineapple",
];

fn numbers_only(value: &str) -> Result<(), String> {
    if value.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        Err("only digits are allowed".to_owned())
    }
}

pub struct TextInputStory {
    base: Base,
    matches: Vec<(&'static str, Vec<usize>)>,
}

impl TextInputStory {
    pub fn new() -> Self {
        let mut search = TextInput::empty()
            .label("Search")
            .label_width(LABEL_WIDTH)
            .placeholder("filter the fruit below")
            .search_mode();
        search.set_match_count(Some(SAMPLE_ITEMS.len()));
        let base = Base::new("Text input")
            .intro(vec![
                "One line with a cursor and sideways scrolling. Ctrl/Alt+arrows jump words,",
                "Ctrl+z / Ctrl+y undo and redo. Enter is refused while the check fails.",
                "Search mode reports every change and Esc clears the text before cancelling.",
                "The search below fuzzy-matches the fruit list and colours the matched letters.",
            ])
            .sample(sample("Empty, placeholder", 26, 1, |x, y| {
                TextInput::empty()
                    .placeholder("type a name")
                    .serialize(x, y, 26)
            }))
            .sample(sample("Hovered", 26, 1, |x, y| {
                TextInput::new("hello").hovered().serialize(x, y, 26)
            }))
            .sample(sample("Focused", 26, 1, |x, y| {
                TextInput::new("hello").focused().serialize(x, y, 26)
            }))
            .sample(sample("Disabled", 26, 1, |x, y| {
                TextInput::new("read only").disabled().serialize(x, y, 26)
            }))
            .sample(sample("Error", 26, 2, |x, y| {
                TextInput::new("12ab")
                    .validator(numbers_only)
                    .serialize(x, y, 26)
            }))
            .sample(sample("Longer than the field", 26, 1, |x, y| {
                TextInput::new("a value that is much longer than the field")
                    .focused()
                    .serialize(x, y, 26)
            }))
            .live(
                "Name",
                TextInput::empty()
                    .label("Name")
                    .label_width(LABEL_WIDTH)
                    .placeholder("your name"),
                FIELD_WIDTH,
                1,
            )
            .live(
                "Port",
                TextInput::new("8080")
                    .label("Port")
                    .label_width(LABEL_WIDTH)
                    .validator(numbers_only),
                FIELD_WIDTH,
                2,
            )
            .live(
                "Long value",
                TextInput::new(
                    "/home/user/projects/a/very/deep/directory/structure/that/keeps/going/file.txt",
                )
                .label("Long value")
                .label_width(LABEL_WIDTH),
                FIELD_WIDTH,
                1,
            )
            .live("Search", search, FIELD_WIDTH, 1)
            .extra_rows(SAMPLE_ITEMS.len() + 1);
        TextInputStory {
            base,
            matches: SAMPLE_ITEMS.iter().map(|item| (*item, vec![])).collect(),
        }
    }
    fn update_matches(&mut self, query: &str) {
        self.matches = SAMPLE_ITEMS
            .iter()
            .filter_map(|item| fuzzy_match_indices(query, item).map(|indices| (*item, indices)))
            .collect();
        let count = self.matches.len();
        if let Some(search) = self.base.group.text_input_mut(&"Search") {
            search.set_match_count(Some(count));
        }
    }
    fn after_event(&mut self) {
        let query = self
            .base
            .group
            .text_input(&"Search")
            .map(|s| s.get_text().to_owned())
            .unwrap_or_default();
        self.update_matches(&query);
    }
}

impl Story for TextInputStory {
    fn title(&self) -> &'static str {
        self.base.title
    }
    fn layout(&mut self, width: usize, height: usize) -> usize {
        self.base.layout(width, height)
    }
    fn render(&mut self, frame: &Frame) {
        self.base.render(frame);
        let row = self.base.extra_row;
        frame.caption(row, LABEL_WIDTH, "Matches:");
        for (index, (item, indices)) in self.matches.iter().enumerate() {
            frame.text(
                row + 1 + index,
                LABEL_WIDTH + 2,
                Text::new(item).color_indices(3, indices.clone()),
            );
        }
    }
    fn render_overlays(&mut self, frame: &Frame) {
        self.base.render_overlays(frame);
    }
    fn handle_key(&mut self, key: &KeyWithModifier) -> Option<String> {
        let status = self.base.handle_key(key);
        self.after_event();
        status
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
}
