use std::borrow::Cow;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

pub fn clusters(s: &str) -> impl Iterator<Item = (&str, usize)> {
    s.graphemes(true).map(|cluster| (cluster, cluster_width(cluster)))
}

pub fn cluster_width(cluster: &str) -> usize {
    if cluster.is_ascii() {
        cluster.bytes().filter(|b| !b.is_ascii_control()).count()
    } else {
        cluster.width()
    }
}

pub fn display_width(s: &str) -> usize {
    if s.is_ascii() {
        return s.bytes().filter(|b| !b.is_ascii_control()).count();
    }
    clusters(s).map(|(_, width)| width).sum()
}

fn ellipsis_fits(max_cols: usize, ellipsis: &str) -> Option<usize> {
    let ellipsis_width = display_width(ellipsis);
    if ellipsis_width > max_cols {
        None
    } else {
        Some(max_cols - ellipsis_width)
    }
}

pub fn prefix_within(s: &str, max_cols: usize) -> (&str, usize) {
    let mut used = 0;
    let mut end = 0;
    for (cluster, width) in clusters(s) {
        if used + width > max_cols {
            break;
        }
        used += width;
        end += cluster.len();
    }
    (&s[..end], used)
}

pub fn suffix_within(s: &str, max_cols: usize) -> (&str, usize) {
    let mut used = 0;
    let mut start = s.len();
    for cluster in s.graphemes(true).rev() {
        let width = cluster_width(cluster);
        if used + width > max_cols {
            break;
        }
        used += width;
        start -= cluster.len();
    }
    let mut tail = &s[start..];
    while let Some(first) = tail.graphemes(true).next() {
        if cluster_width(first) == 0 {
            tail = &tail[first.len()..];
        } else {
            break;
        }
    }
    (tail, used)
}

pub fn truncate_end<'a>(s: &'a str, max_cols: usize, ellipsis: &str) -> Cow<'a, str> {
    if display_width(s) <= max_cols {
        return Cow::Borrowed(s);
    }
    match ellipsis_fits(max_cols, ellipsis) {
        Some(room) => {
            let (head, _) = prefix_within(s, room);
            Cow::Owned(format!("{}{}", head, ellipsis))
        },
        None => Cow::Borrowed(prefix_within(s, max_cols).0),
    }
}

pub fn truncate_start<'a>(s: &'a str, max_cols: usize, marker: &str) -> Cow<'a, str> {
    if display_width(s) <= max_cols {
        return Cow::Borrowed(s);
    }
    match ellipsis_fits(max_cols, marker) {
        Some(room) => {
            let (tail, _) = suffix_within(s, room);
            Cow::Owned(format!("{}{}", marker, tail))
        },
        None => Cow::Borrowed(suffix_within(s, max_cols).0),
    }
}

pub fn truncate_middle<'a>(s: &'a str, max_cols: usize, marker: &str) -> Cow<'a, str> {
    if display_width(s) <= max_cols {
        return Cow::Borrowed(s);
    }
    match ellipsis_fits(max_cols, marker) {
        Some(room) => {
            let left_room = (room + 1) / 2;
            let (head, head_used) = prefix_within(s, left_room);
            let (tail, _) = suffix_within(&s[head.len()..], room - head_used);
            Cow::Owned(format!("{}{}{}", head, marker, tail))
        },
        None => Cow::Borrowed(prefix_within(s, max_cols).0),
    }
}

pub fn pop_cluster(s: &mut String) -> Option<String> {
    let last = s.graphemes(true).next_back()?;
    let start = s.len() - last.len();
    let popped = s[start..].to_string();
    s.truncate(start);
    Some(popped)
}

pub fn cluster_index_of_char(s: &str, char_index: usize) -> Option<usize> {
    let mut seen_chars = 0;
    for (cluster_index, cluster) in s.graphemes(true).enumerate() {
        let count = cluster.chars().count();
        if char_index < seen_chars + count {
            return Some(cluster_index);
        }
        seen_chars += count;
    }
    None
}

pub fn cluster_char_ranges(s: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut seen_chars = 0;
    for cluster in s.graphemes(true) {
        let count = cluster.chars().count();
        ranges.push((seen_chars, seen_chars + count));
        seen_chars += count;
    }
    ranges
}

pub fn previous_cluster_start(s: &str, byte_offset: usize) -> usize {
    let mut previous = 0;
    for (index, _) in s.grapheme_indices(true) {
        if index >= byte_offset {
            break;
        }
        previous = index;
    }
    previous
}

pub fn next_cluster_end(s: &str, byte_offset: usize) -> usize {
    for (index, cluster) in s.grapheme_indices(true) {
        let end = index + cluster.len();
        if end > byte_offset {
            return end;
        }
    }
    s.len()
}

pub fn strip_control_characters(s: &str) -> Cow<'_, str> {
    if s.chars().any(is_unsafe_title_char) {
        Cow::Owned(s.chars().filter(|c| !is_unsafe_title_char(*c)).collect())
    } else {
        Cow::Borrowed(s)
    }
}

fn is_unsafe_title_char(c: char) -> bool {
    c.is_control() || c == '\u{2028}' || c == '\u{2029}'
}

#[cfg(test)]
#[path = "./unit/graphemes_tests.rs"]
mod graphemes_tests;
