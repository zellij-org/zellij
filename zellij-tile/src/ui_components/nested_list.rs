use super::Text;
use std::borrow::Borrow;

#[derive(Debug, Default, Clone)]
pub struct NestedListItem {
    indentation_level: usize,
    content: Text,
}

impl NestedListItem {
    pub fn new(text: Text) -> Self {
        NestedListItem {
            content: text,
            ..Default::default()
        }
    }
    pub fn selected(mut self) -> Self {
        self.content = self.content.selected();
        self
    }
    pub fn indent(mut self, indentation_level: usize) -> Self {
        self.indentation_level = indentation_level;
        self
    }
    pub fn success_color_last_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        self.content = self.content.success_color_last_substring(substr);
        self
    }
    pub fn error_color_last_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        self.content = self.content.error_color_last_substring(substr);
        self
    }
    pub fn color_all(mut self, index_level: usize) -> Self {
        self.content = self.content.color_all(index_level);
        self
    }
    pub fn color_substring<S: AsRef<str>>(mut self, index_level: usize, substr: S) -> Self {
        self.content = self.content.color_substring(index_level, substr);
        self
    }
    pub fn color_nth_substring<S: AsRef<str>>(
        mut self,
        index_level: usize,
        substr: S,
        occurrence_index: usize,
    ) -> Self {
        self.content = self
            .content
            .color_nth_substring(index_level, substr, occurrence_index);
        self
    }
    pub fn color_last_substring<S: AsRef<str>>(mut self, index_level: usize, substr: S) -> Self {
        self.content = self.content.color_last_substring(index_level, substr);
        self
    }
    pub fn dim_all(mut self) -> Self {
        self.content = self.content.dim_all();
        self
    }
    pub fn dim_indices(mut self, indices: Vec<usize>) -> Self {
        self.content = self.content.dim_indices(indices);
        self
    }
    pub fn dim_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        self.content = self.content.dim_range(indices);
        self
    }
    pub fn dim_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        self.content = self.content.dim_substring(substr);
        self
    }
    pub fn unbold_all(mut self) -> Self {
        self.content = self.content.unbold_all();
        self
    }
    pub fn unbold_indices(mut self, indices: Vec<usize>) -> Self {
        self.content = self.content.unbold_indices(indices);
        self
    }
    pub fn unbold_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        self.content = self.content.unbold_range(indices);
        self
    }
    pub fn unbold_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        self.content = self.content.unbold_substring(substr);
        self
    }
    pub fn disabled(mut self) -> Self {
        self.content = self.content.disabled();
        self
    }
    pub fn content(&self) -> &str {
        self.content.content()
    }
    pub fn len(&self) -> usize {
        self.content.len()
    }
    pub fn is_empty(&self) -> bool {
        self.content.len() == 0
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
