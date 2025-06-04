use std::ops::Bound;
use std::ops::RangeBounds;

#[derive(Debug, Default, Clone)]
pub struct Text {
    text: String,
    selected: bool,
    opaque: bool,
    indices: Vec<Vec<usize>>,
}

impl Text {
    pub fn new<S: AsRef<str>>(content: S) -> Self
    where
        S: ToString,
    {
        Text {
            text: content.to_string(),
            selected: false,
            opaque: false,
            indices: vec![],
        }
    }
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }
    pub fn color_indices(mut self, index_level: usize, mut indices: Vec<usize>) -> Self {
        self.pad_indices(index_level);
        self.indices
            .get_mut(index_level)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn color_range<R: RangeBounds<usize>>(mut self, index_level: usize, indices: R) -> Self {
        self.pad_indices(index_level);
        let start = match indices.start_bound() {
            Bound::Unbounded => 0,
            Bound::Included(s) => *s,
            Bound::Excluded(s) => *s,
        };
        let end = match indices.end_bound() {
            Bound::Unbounded => self.text.chars().count(),
            Bound::Included(s) => *s + 1,
            Bound::Excluded(s) => *s,
        };
        let indices = (start..end).into_iter();
        self.indices
            .get_mut(index_level)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }
    pub fn color_substring<S: AsRef<str>>(mut self, index_level: usize, substr: S) -> Self {
        let substr = substr.as_ref();
        let mut start = 0;

        while let Some(pos) = self.text[start..].find(substr) {
            let abs_pos = start + pos;
            // eprintln!("full: '{}', substr: '{}' {}->{}", self.text, substr.as_ref(), abs_pos, abs_pos + substr.len());
            self = self.color_range(index_level, abs_pos..abs_pos + substr.chars().count());
            start = abs_pos + substr.len();
        }

        self
    }

    pub fn color_all(self, index_level: usize) -> Self {
        self.color_range(index_level, ..)
    }

    pub fn color_nth_substring<S: AsRef<str>>(
        self,
        index_level: usize,
        substr: S,
        occurrence_index: usize,
    ) -> Self {
        let substr = substr.as_ref();
        let mut start = 0;
        let mut count = 0;

        while let Some(pos) = self.text[start..].find(substr) {
            if count == occurrence_index {
                let abs_pos = start + pos;
                return self.color_range(index_level, abs_pos..abs_pos + substr.len());
            }
            count += 1;
            start = start + pos + substr.len();
        }

        self
    }
    pub fn content(&self) -> &str {
        &self.text
    }
    fn pad_indices(&mut self, index_level: usize) {
        if self.indices.get(index_level).is_none() {
            for _ in self.indices.len()..=index_level {
                self.indices.push(vec![]);
            }
        }
    }
    pub fn serialize(&self) -> String {
        let text = self
            .text
            .to_string()
            .as_bytes()
            .iter()
            .map(|b| b.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut indices = String::new();
        for index_variants in &self.indices {
            indices.push_str(&format!(
                "{}$",
                index_variants
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        let mut prefix = "".to_owned();

        if self.selected {
            prefix = format!("x{}", prefix);
        }

        if self.opaque {
            prefix = format!("z{}", prefix);
        }

        format!("{}{}{}", prefix, indices, text)
    }
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }
}

pub fn print_text(text: Text) {
    print!("\u{1b}Pztext;{}\u{1b}\\", text.serialize())
}

pub fn print_text_with_coordinates(
    text: Text,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    print!(
        "\u{1b}Pztext;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        text.serialize()
    )
}

pub fn serialize_text(text: &Text) -> String {
    format!("\u{1b}Pztext;{}\u{1b}\\", text.serialize())
}

pub fn serialize_text_with_coordinates(
    text: &Text,
    x: usize,
    y: usize,
    width: Option<usize>,
    height: Option<usize>,
) -> String {
    let width = width.map(|w| w.to_string()).unwrap_or_default();
    let height = height.map(|h| h.to_string()).unwrap_or_default();
    format!(
        "\u{1b}Pztext;{}/{}/{}/{};{}\u{1b}\\",
        x,
        y,
        width,
        height,
        text.serialize()
    )
}
