use std::ops::Bound;
use std::ops::RangeBounds;
use zellij_utils::data::StyledText;

pub const CUSTOM_COLOR_LEVEL: usize = 9;
pub const CUSTOM_COLOR_MARK: usize = 1 << 30;

#[derive(Debug, Default, Clone)]
pub struct Text {
    text: String,
    selected: bool,
    opaque: bool,
    disabled: bool,
    indices: Vec<Vec<usize>>,
}

impl From<StyledText> for Text {
    fn from(styled_text: StyledText) -> Self {
        Text {
            text: styled_text.text,
            indices: styled_text.indices,
            ..Default::default()
        }
    }
}

impl From<&String> for Text {
    fn from(content: &String) -> Self {
        Text::from(content.clone())
    }
}

impl From<Text> for StyledText {
    fn from(text: Text) -> Self {
        StyledText {
            text: text.text,
            indices: text.indices,
        }
    }
}

impl From<String> for Text {
    fn from(value: String) -> Self {
        Text {
            text: value,
            ..Default::default()
        }
    }
}

impl From<&str> for Text {
    fn from(value: &str) -> Self {
        Text::from(value.to_owned())
    }
}

impl Text {
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self
    }
    pub fn opaque(mut self) -> Self {
        self.opaque = true;
        self
    }
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    pub fn dim_indices(mut self, mut indices: Vec<usize>) -> Self {
        const DIM_LEVEL: usize = 4;
        self.pad_indices(DIM_LEVEL);
        self.indices
            .get_mut(DIM_LEVEL)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn dim_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        const DIM_LEVEL: usize = 4;
        self.pad_indices(DIM_LEVEL);
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
        let indices = start..end;
        self.indices
            .get_mut(DIM_LEVEL)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }
    pub fn dim_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        for (start, end) in self.substring_char_ranges(substr.as_ref()) {
            self = self.dim_range(start..end);
        }
        self
    }
    pub fn dim_all(self) -> Self {
        const DIM_LEVEL: usize = 4;
        self.color_range(DIM_LEVEL, ..)
    }
    pub fn unbold_indices(mut self, mut indices: Vec<usize>) -> Self {
        const UNBOLD_LEVEL: usize = 5;
        self.pad_indices(UNBOLD_LEVEL);
        self.indices
            .get_mut(UNBOLD_LEVEL)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn unbold_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        const UNBOLD_LEVEL: usize = 5;
        self.pad_indices(UNBOLD_LEVEL);
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
        let indices = start..end;
        self.indices
            .get_mut(UNBOLD_LEVEL)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }
    pub fn unbold_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        for (start, end) in self.substring_char_ranges(substr.as_ref()) {
            self = self.unbold_range(start..end);
        }
        self
    }
    pub fn italic_range<R: RangeBounds<usize>>(self, indices: R) -> Self {
        const ITALIC_LEVEL: usize = 8;
        self.color_range(ITALIC_LEVEL, indices)
    }
    pub fn italic_all(self) -> Self {
        const ITALIC_LEVEL: usize = 8;
        self.color_range(ITALIC_LEVEL, ..)
    }
    pub fn unbold_all(self) -> Self {
        const UNBOLD_LEVEL: usize = 5;
        self.color_range(UNBOLD_LEVEL, ..)
    }
    pub fn error_color_indices(mut self, mut indices: Vec<usize>) -> Self {
        const ERROR_COLOR_LEVEL: usize = 6;
        self.pad_indices(ERROR_COLOR_LEVEL);
        self.indices
            .get_mut(ERROR_COLOR_LEVEL)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn error_color_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        const ERROR_COLOR_LEVEL: usize = 6;
        self.pad_indices(ERROR_COLOR_LEVEL);
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
        let indices = start..end;
        self.indices
            .get_mut(ERROR_COLOR_LEVEL)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }
    pub fn error_color_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        for (start, end) in self.substring_char_ranges(substr.as_ref()) {
            self = self.error_color_range(start..end);
        }
        self
    }
    pub fn error_color_nth_substring<S: AsRef<str>>(
        self,
        substr: S,
        occurrence_index: usize,
    ) -> Self {
        match self
            .substring_char_ranges(substr.as_ref())
            .get(occurrence_index)
            .copied()
        {
            Some((start, end)) => self.error_color_range(start..end),
            None => self,
        }
    }

    pub fn error_color_last_substring<S: AsRef<str>>(self, substr: S) -> Self {
        match self.last_substring_char_range(substr.as_ref()) {
            Some((start, end)) => self.error_color_range(start..end),
            None => self,
        }
    }

    pub fn error_color_all(self) -> Self {
        self.error_color_range(..)
    }
    pub fn success_color_indices(mut self, mut indices: Vec<usize>) -> Self {
        const SUCCESS_COLOR_LEVEL: usize = 7;
        self.pad_indices(SUCCESS_COLOR_LEVEL);
        self.indices
            .get_mut(SUCCESS_COLOR_LEVEL)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn success_color_range<R: RangeBounds<usize>>(mut self, indices: R) -> Self {
        const SUCCESS_COLOR_LEVEL: usize = 7;
        self.pad_indices(SUCCESS_COLOR_LEVEL);
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
        let indices = start..end;
        self.indices
            .get_mut(SUCCESS_COLOR_LEVEL)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }
    pub fn success_color_substring<S: AsRef<str>>(mut self, substr: S) -> Self {
        for (start, end) in self.substring_char_ranges(substr.as_ref()) {
            self = self.success_color_range(start..end);
        }
        self
    }
    pub fn success_color_nth_substring<S: AsRef<str>>(
        self,
        substr: S,
        occurrence_index: usize,
    ) -> Self {
        match self
            .substring_char_ranges(substr.as_ref())
            .get(occurrence_index)
            .copied()
        {
            Some((start, end)) => self.success_color_range(start..end),
            None => self,
        }
    }

    pub fn success_color_last_substring<S: AsRef<str>>(self, substr: S) -> Self {
        match self.last_substring_char_range(substr.as_ref()) {
            Some((start, end)) => self.success_color_range(start..end),
            None => self,
        }
    }

    pub fn success_color_all(self) -> Self {
        self.success_color_range(..)
    }
    pub fn color_indices(mut self, index_level: usize, mut indices: Vec<usize>) -> Self {
        self.pad_indices(index_level);
        self.indices
            .get_mut(index_level)
            .map(|i| i.append(&mut indices));
        self
    }
    pub fn rgb_color_range<R: RangeBounds<usize>>(mut self, rgb: [u8; 3], indices: R) -> Self {
        let mark = CUSTOM_COLOR_MARK
            + ((rgb[0] as usize) << 16 | (rgb[1] as usize) << 8 | rgb[2] as usize);
        let existing = self
            .indices
            .iter()
            .enumerate()
            .skip(CUSTOM_COLOR_LEVEL)
            .find(|(_, level)| level.contains(&mark))
            .map(|(level, _)| level);
        let level = match existing {
            Some(level) => level,
            None => {
                let level = self.indices.len().max(CUSTOM_COLOR_LEVEL);
                self.pad_indices(level);
                self.indices[level].push(mark);
                level
            },
        };
        self.color_range(level, indices)
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
        let indices = start..end;
        self.indices
            .get_mut(index_level)
            .map(|i| i.append(&mut indices.into_iter().collect()));
        self
    }

    pub fn color_substring<S: AsRef<str>>(mut self, index_level: usize, substr: S) -> Self {
        for (start, end) in self.substring_char_ranges(substr.as_ref()) {
            self = self.color_range(index_level, start..end);
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
        match self
            .substring_char_ranges(substr.as_ref())
            .get(occurrence_index)
            .copied()
        {
            Some((start, end)) => self.color_range(index_level, start..end),
            None => self,
        }
    }

    pub fn color_last_substring<S: AsRef<str>>(self, index_level: usize, substr: S) -> Self {
        match self.last_substring_char_range(substr.as_ref()) {
            Some((start, end)) => self.color_range(index_level, start..end),
            None => self,
        }
    }

    fn substring_char_ranges(&self, substr: &str) -> Vec<(usize, usize)> {
        let mut ranges = vec![];
        if substr.is_empty() {
            return ranges;
        }
        let substr_chars = substr.chars().count();
        let mut start = 0;
        while let Some(pos) = self.text[start..].find(substr) {
            let abs_pos = start + pos;
            let char_start = self.text[..abs_pos].chars().count();
            ranges.push((char_start, char_start + substr_chars));
            start = abs_pos + substr.len();
        }
        ranges
    }

    fn last_substring_char_range(&self, substr: &str) -> Option<(usize, usize)> {
        if substr.is_empty() {
            return None;
        }
        self.text.rfind(substr).map(|pos| {
            let char_start = self.text[..pos].chars().count();
            (char_start, char_start + substr.chars().count())
        })
    }

    pub fn content(&self) -> &str {
        &self.text
    }
    pub fn width(&self) -> usize {
        zellij_utils::graphemes::display_width(&self.text)
    }
    pub fn is_plain(&self) -> bool {
        self.indices.iter().all(|level| level.is_empty())
    }
    pub fn wrap(&self, width: usize) -> Vec<Text> {
        let width = width.max(1);
        let mut units: Vec<(&str, usize, usize)> = vec![];
        let mut char_index = 0;
        for (cluster, cluster_width) in zellij_utils::graphemes::clusters(&self.text) {
            units.push((cluster, char_index, cluster_width));
            char_index += cluster.chars().count();
        }
        let is_newline = |unit: &(&str, usize, usize)| unit.0.contains('\n');
        let is_whitespace = |unit: &(&str, usize, usize)| unit.0.chars().all(char::is_whitespace);
        let mut lines: Vec<Vec<Option<usize>>> = vec![];
        let mut line: Vec<Option<usize>> = vec![];
        let mut line_width = 0;
        let mut position = 0;
        while position <= units.len() {
            if position == units.len() || is_newline(&units[position]) {
                lines.push(std::mem::take(&mut line));
                line_width = 0;
                position += 1;
                continue;
            }
            if is_whitespace(&units[position]) {
                position += 1;
                continue;
            }
            let word_end = (position..units.len())
                .find(|index| is_whitespace(&units[*index]))
                .unwrap_or(units.len());
            let mut word: Vec<usize> = (position..word_end).collect();
            position = word_end;
            loop {
                let word_width: usize = word.iter().map(|index| units[*index].2).sum();
                let separator = if line.is_empty() { 0 } else { 1 };
                if line_width + separator + word_width <= width {
                    if separator == 1 {
                        line.push(None);
                    }
                    line.extend(word.iter().map(|index| Some(*index)));
                    line_width += separator + word_width;
                    break;
                }
                if !line.is_empty() {
                    lines.push(std::mem::take(&mut line));
                    line_width = 0;
                    continue;
                }
                let mut taken = 0;
                let mut split_at = 0;
                for index in &word {
                    let next = units[*index].2;
                    if taken + next > width && split_at > 0 {
                        break;
                    }
                    taken += next;
                    split_at += 1;
                }
                let rest = word.split_off(split_at);
                lines.push(word.into_iter().map(Some).collect());
                word = rest;
                if word.is_empty() {
                    break;
                }
            }
        }
        while lines.len() > 1 && lines.last().map(|line| line.is_empty()).unwrap_or(false) {
            lines.pop();
        }
        let levels_at = |original: usize| -> Vec<usize> {
            self.indices
                .iter()
                .enumerate()
                .filter(|(_, indices)| indices.contains(&original))
                .map(|(level, _)| level)
                .collect()
        };
        lines
            .into_iter()
            .map(|line| {
                let mut content = String::new();
                let mut indices: Vec<Vec<usize>> = vec![];
                let mut mark = |levels: Vec<usize>, column: usize| {
                    for level in levels {
                        if indices.len() <= level {
                            indices.resize(level + 1, vec![]);
                        }
                        indices[level].push(column);
                    }
                };
                let mut column = 0;
                for (slot, original) in line.iter().enumerate() {
                    match original {
                        Some(original) => {
                            let (cluster, first_char, _) = units[*original];
                            for (offset, character) in cluster.chars().enumerate() {
                                content.push(character);
                                mark(levels_at(first_char + offset), column);
                                column += 1;
                            }
                        },
                        None => {
                            content.push(' ');
                            let before = line[..slot].iter().rev().find_map(|index| *index);
                            let after = line[slot + 1..].iter().find_map(|index| *index);
                            if let (Some(before), Some(after)) = (before, after) {
                                let after_levels = levels_at(units[after].1);
                                let (before_cluster, before_start, _) = units[before];
                                let before_last =
                                    before_start + before_cluster.chars().count().saturating_sub(1);
                                let shared = levels_at(before_last)
                                    .into_iter()
                                    .filter(|level| after_levels.contains(level))
                                    .collect();
                                mark(shared, column);
                            }
                            column += 1;
                        },
                    }
                }
                Text {
                    text: content,
                    selected: self.selected,
                    opaque: self.opaque,
                    disabled: self.disabled,
                    indices,
                }
            })
            .collect()
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
            prefix.push('x');
        }

        if self.opaque {
            prefix.push('z');
        }

        if self.disabled {
            prefix.push('d');
        }

        format!("{}{}{}", prefix, indices, text)
    }
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_custom_colour_gets_one_level_per_colour() {
        let text = Text::from("abcd")
            .rgb_color_range([1, 2, 3], 0..1)
            .rgb_color_range([4, 5, 6], 1..2)
            .rgb_color_range([1, 2, 3], 2..3);
        let first = CUSTOM_COLOR_MARK + 0x010203;
        let second = CUSTOM_COLOR_MARK + 0x040506;
        assert_eq!(text.indices[CUSTOM_COLOR_LEVEL], vec![first, 0, 2]);
        assert_eq!(text.indices[CUSTOM_COLOR_LEVEL + 1], vec![second, 1]);
    }

    fn text_body(text: &str) -> String {
        text.as_bytes()
            .iter()
            .map(|byte| byte.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    #[test]
    fn from_str_equal_from_string() {
        let from_str = Text::from("x").serialize();
        let from_string = Text::from(String::from("x")).serialize();

        assert_eq!(from_str, from_string);
    }

    #[test]
    fn disabled_flag_serializes_with_a_d_prefix() {
        let serialized = Text::from("x").disabled().serialize();
        assert_eq!(serialized, format!("d{}", text_body("x")));
    }

    #[test]
    fn opaque_and_disabled_serialize_in_push_order() {
        let serialized = Text::from("x").disabled().opaque().serialize();
        assert_eq!(serialized, format!("zd{}", text_body("x")));
    }

    #[test]
    fn all_flags_serialize_in_selected_opaque_disabled_order() {
        let serialized = Text::from("x").disabled().opaque().selected().serialize();
        assert_eq!(serialized, format!("xzd{}", text_body("x")));
    }

    #[test]
    fn flags_do_not_disturb_indices_or_text() {
        let serialized = Text::from("Foo bar baz")
            .disabled()
            .color_indices(0, vec![0, 1, 2])
            .serialize();
        assert!(serialized.starts_with('d'));
        assert!(serialized.contains("0,1,2$"));
        assert!(serialized.ends_with(&text_body("Foo bar baz")));
    }

    fn level(text: &Text, level: usize) -> Vec<usize> {
        text.indices.get(level).cloned().unwrap_or_default()
    }

    #[test]
    fn substring_methods_use_character_positions_with_non_ascii_text() {
        let text = Text::from("é foo é foo")
            .dim_substring("foo")
            .unbold_substring("é")
            .error_color_substring("o é")
            .success_color_substring("fo");
        assert_eq!(level(&text, 4), vec![2, 3, 4, 8, 9, 10]);
        assert_eq!(level(&text, 5), vec![0, 6]);
        assert_eq!(level(&text, 6), vec![4, 5, 6]);
        assert_eq!(level(&text, 7), vec![2, 3, 8, 9]);
    }

    #[test]
    fn nth_and_last_substring_methods_use_character_positions_with_non_ascii_text() {
        let text = Text::from("日本 ab 日本 ab")
            .error_color_nth_substring("ab", 1)
            .success_color_nth_substring("日本", 1)
            .color_nth_substring(0, "ab", 0)
            .color_last_substring(1, "本 a");
        assert_eq!(level(&text, 6), vec![9, 10]);
        assert_eq!(level(&text, 7), vec![6, 7]);
        assert_eq!(level(&text, 0), vec![3, 4]);
        assert_eq!(level(&text, 1), vec![7, 8, 9]);
        let last = Text::from("ü x ü y")
            .error_color_last_substring("ü")
            .success_color_last_substring(" y");
        assert_eq!(level(&last, 6), vec![4]);
        assert_eq!(level(&last, 7), vec![5, 6]);
    }

    #[test]
    fn missing_or_empty_substring_leaves_indices_untouched() {
        let text = Text::from("👨‍👩‍👧 ab")
            .error_color_nth_substring("ab", 3)
            .color_last_substring(0, "zz")
            .dim_substring("");
        assert!(text.is_plain());
    }

    #[test]
    fn width_counts_clusters() {
        assert_eq!(Text::from("e\u{301}x").width(), 2);
        assert_eq!(Text::from("👨‍👩‍👧").width(), 2);
    }

    #[test]
    fn wrap_never_splits_a_cluster() {
        let lines = Text::from("e\u{301}e\u{301}e\u{301}").wrap(2);
        let contents: Vec<&str> = lines.iter().map(|line| line.content()).collect();
        assert_eq!(contents, vec!["e\u{301}e\u{301}", "e\u{301}"]);
        let lines = Text::from("👨‍👩‍👧 ab").wrap(3);
        let contents: Vec<&str> = lines.iter().map(|line| line.content()).collect();
        assert_eq!(contents, vec!["👨‍👩‍👧", "ab"]);
    }

    #[test]
    fn wrap_keeps_character_highlights_on_clusters() {
        let lines = Text::from("ab e\u{301}x").color_range(0, 3..6).wrap(2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].content(), "e\u{301}x");
        assert_eq!(level(&lines[1], 0), vec![0, 1, 2]);
    }
}
