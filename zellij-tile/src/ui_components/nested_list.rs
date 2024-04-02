use super::Text;
use std::borrow::Borrow;
use std::ops::RangeBounds;

#[derive(Debug, Default, Clone)]
pub struct NestedListItem {
    indentation_level: usize,
    content: Text,
}

impl NestedListItem {
    pub fn new<S: AsRef<str>>(text: S) -> Self
    where
        S: ToString,
    {
        NestedListItem {
            content: Text::new(text),
            ..Default::default()
        }
    }
    pub fn indent(mut self, indentation_level: usize) -> Self {
        self.indentation_level = indentation_level;
        self
    }
    pub fn selected(mut self) -> Self {
        self.content = self.content.selected();
        self
    }
    pub fn color_indices(mut self, index_level: usize, indices: Vec<usize>) -> Self {
        self.content = self.content.color_indices(index_level, indices);
        self
    }
    pub fn color_range<R: RangeBounds<usize>>(mut self, index_level: usize, indices: R) -> Self {
        self.content = self.content.color_range(index_level, indices);
        self
    }
    pub fn serialize(&self) -> String {
        let mut serialized = String::new();
        for _ in 0..self.indentation_level {
            serialized.push('|');
        }
        format!("{}{}", serialized, self.content.serialize())
    }
}

/// render a nested list with arbitrary data
pub fn print_nested_list(items: Vec<NestedListItem>) {
    let items = items
        .into_iter()
        .map(|i| i.serialize())
        .collect::<Vec<_>>()
        .join(";");
    print!("\u{1b}Pznested_list;{}\u{1b}\\", items)
}

pub fn print_nested_list_with_coordinates(
    items: Vec<NestedListItem>,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    let items = items
        .into_iter()
        .map(|i| i.serialize())
        .collect::<Vec<_>>()
        .join(";");
    print!(
        "\u{1b}Pznested_list;{}/{}/{}/{};{}\u{1b}\\",
        x, y, width, height, items
    )
}

pub fn serialize_nested_list<I>(items: I) -> String
where
    I: IntoIterator,
    I::Item: Borrow<NestedListItem>,
{
    let items = items
        .into_iter()
        .map(|i| i.borrow().serialize())
        .collect::<Vec<_>>()
        .join(";");
    format!("\u{1b}Pznested_list;{}\u{1b}\\", items)
}

pub fn serialize_nested_list_with_coordinates<I>(
    items: I,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) -> String
where
    I: IntoIterator,
    I::Item: Borrow<NestedListItem>,
{
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    let items = items
        .into_iter()
        .map(|i| i.borrow().serialize())
        .collect::<Vec<_>>()
        .join(";");
    format!(
        "\u{1b}Pznested_list;{}/{}/{}/{};{}\u{1b}\\",
        x, y, width, height, items
    )
}
