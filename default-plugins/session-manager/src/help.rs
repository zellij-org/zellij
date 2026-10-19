use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

use crate::inbox::key_hint_text;
use crate::panel::truncate_plain;

pub const GAP: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub key: String,
    pub label: String,
}

pub fn hint(key: impl Into<String>, label: impl Into<String>) -> Hint {
    Hint {
        key: key.into(),
        label: label.into(),
    }
}

impl Hint {
    fn text(&self) -> String {
        format!("{} {}", self.key, self.label)
    }
}

fn line_width(items: &[String]) -> usize {
    let content: usize = items.iter().map(|item| item.width()).sum();
    content + GAP * items.len().saturating_sub(1)
}

pub fn footer_lines(hints: &[Hint], more: &Hint, width: usize, max_lines: usize) -> Vec<String> {
    if width == 0 || max_lines == 0 {
        return vec![];
    }
    let mut lines: Vec<Vec<String>> = vec![vec![]];
    let mut shown = 0;
    for hint in hints {
        let item = truncate_plain(&hint.text(), width);
        let current = lines.last_mut().unwrap();
        let mut candidate = current.clone();
        candidate.push(item.clone());
        if line_width(&candidate) <= width {
            current.push(item);
        } else if lines.len() < max_lines {
            lines.push(vec![item]);
        } else {
            break;
        }
        shown += 1;
    }
    if shown < hints.len() {
        let more = truncate_plain(&more.text(), width);
        loop {
            let current = lines.last_mut().unwrap();
            let mut candidate = current.clone();
            candidate.push(more.clone());
            if line_width(&candidate) <= width || current.is_empty() {
                current.push(more);
                break;
            }
            current.pop();
        }
    }
    lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.join(&" ".repeat(GAP)))
        .collect()
}

fn pack(hints: &[Hint], width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<Vec<String>> = vec![vec![]];
    for hint in hints {
        let item = truncate_plain(&hint.text(), width);
        let current = lines.last_mut().unwrap();
        let mut candidate = current.clone();
        candidate.push(item.clone());
        if line_width(&candidate) <= width {
            current.push(item);
        } else if lines.len() < max_lines {
            lines.push(vec![item]);
        } else {
            break;
        }
    }
    lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.join(&" ".repeat(GAP)))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockLine {
    pub text: String,
    pub secondary: bool,
}

pub fn block_lines(
    primary: &[Hint],
    secondary: &[Hint],
    more: &Hint,
    width: usize,
) -> Vec<BlockLine> {
    if width == 0 {
        return vec![];
    }
    let mut lines: Vec<BlockLine> = pack(primary, width, 2)
        .into_iter()
        .map(|text| BlockLine {
            text,
            secondary: false,
        })
        .collect();
    lines.extend(
        footer_lines(secondary, more, width, 1)
            .into_iter()
            .map(|text| BlockLine {
                text,
                secondary: true,
            }),
    );
    lines
}

pub fn line_text(line: &BlockLine) -> Text {
    let mut text = key_hint_ranges(Text::from(line.text.as_str()), &line.text);
    if !line.secondary {
        return text;
    }
    let mut label_start = Some(0);
    for (index, c) in line.text.chars().enumerate() {
        match c {
            '<' => {
                if let Some(start) = label_start.take() {
                    if index > start {
                        text = text.dim_range(start..index);
                    }
                }
            },
            '>' => label_start = Some(index + 1),
            _ => {},
        }
    }
    let count = line.text.chars().count();
    if let Some(start) = label_start {
        if count > start {
            text = text.dim_range(start..count);
        }
    }
    text
}

fn key_hint_ranges(mut text: Text, line: &str) -> Text {
    let mut start = None;
    for (index, c) in line.chars().enumerate() {
        match c {
            '<' => start = Some(index),
            '>' => {
                if let Some(s) = start.take() {
                    text = text.color_range(3, s..index + 1);
                }
            },
            _ => {},
        }
    }
    text
}

pub fn hint_columns(line: &str, hint: &Hint) -> Option<std::ops::Range<usize>> {
    let needle = hint.text();
    let byte = line.find(&needle)?;
    let start = line[..byte].width();
    Some(start..start + needle.width())
}

pub fn render_block(lines: &[BlockLine], x: usize, y: usize, width: usize) {
    for (index, line) in lines.iter().enumerate() {
        print_text_with_coordinates(line_text(line), x, y + index, Some(width), None);
    }
}

pub fn all_keys_lines(hints: &[Hint], width: usize) -> Vec<String> {
    let key_width = hints.iter().map(|h| h.key.width()).max().unwrap_or(0);
    hints
        .iter()
        .map(|h| {
            let padding = key_width.saturating_sub(h.key.width());
            truncate_plain(
                &format!("{}{}  {}", h.key, " ".repeat(padding), h.label),
                width,
            )
        })
        .collect()
}

pub const ALL_KEYS_TITLE: &str = "All keys";
pub const ALL_KEYS_CLOSE: &str = "Press any key to go back";
pub const ALL_KEYS_SCROLL: &str = "<↓↑> more  any other key: back";

pub fn render_all_keys(title: &str, hints: &[Hint], rows: usize, cols: usize, scroll: &mut usize) {
    let inner = cols.saturating_sub(2).max(10);
    let mut lines = all_keys_lines(hints, inner);
    let room = rows.saturating_sub(4).max(1);
    let widest = lines.iter().map(|l| l.width()).max().unwrap_or(0);
    let two_columns =
        lines.len() > room && widest * 2 + 4 <= inner && (lines.len() + 1) / 2 <= room;
    let (columns, closing): (Vec<Vec<String>>, &str) = if two_columns {
        let half = (lines.len() + 1) / 2;
        let right = lines.split_off(half);
        *scroll = 0;
        (vec![lines, right], ALL_KEYS_CLOSE)
    } else if lines.len() > room {
        *scroll = (*scroll).min(lines.len() - room);
        let shown = lines[*scroll..*scroll + room].to_vec();
        (vec![shown], ALL_KEYS_SCROLL)
    } else {
        *scroll = 0;
        (vec![lines], ALL_KEYS_CLOSE)
    };
    let height = columns.iter().map(|c| c.len()).max().unwrap_or(0);
    let column_width = columns
        .iter()
        .flat_map(|c| c.iter().map(|l| l.width()))
        .max()
        .unwrap_or(0);
    let block_width = (column_width * columns.len() + 4 * columns.len().saturating_sub(1))
        .max(title.width())
        .max(closing.width())
        .min(cols);
    let x = cols.saturating_sub(block_width) / 2;
    let y = rows.saturating_sub(height + 4) / 2;
    print_text_with_coordinates(
        Text::from(title).color_range(2, ..),
        x,
        y,
        Some(block_width),
        None,
    );
    for (index, column) in columns.iter().enumerate() {
        let column_x = x + index * (column_width + 4);
        for (row, line) in column.iter().enumerate() {
            print_text_with_coordinates(
                key_hint_text(line),
                column_x,
                y + 2 + row,
                Some(column_width),
                None,
            );
        }
    }
    print_text_with_coordinates(
        key_hint_text(closing).dim_all(),
        x,
        y + 3 + height,
        Some(block_width),
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hints() -> Vec<Hint> {
        vec![
            hint("<ENTER>", "go to pane"),
            hint("<↓↑>", "select"),
            hint("<r>", "reply"),
            hint("<c>", "mark seen"),
            hint("<ESC>", "close"),
            hint("<m>", "mute"),
        ]
    }

    #[test]
    fn everything_fits_on_a_wide_line_without_the_more_hint() {
        let lines = footer_lines(&hints(), &hint("<?>", "all keys"), 200, 2);
        assert_eq!(
            lines,
            vec![
                "<ENTER> go to pane  <↓↑> select  <r> reply  <c> mark seen  <ESC> close  <m> mute"
            ]
        );
    }

    #[test]
    fn a_narrow_footer_keeps_the_most_important_keys_and_points_to_the_rest() {
        let lines = footer_lines(&hints(), &hint("<?>", "all keys"), 32, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("<ENTER> go to pane  <↓↑> select"));
        assert!(lines[1].ends_with("<?> all keys"), "{:?}", lines);
        assert!(lines.iter().all(|l| l.width() <= 32));
        let tiny = footer_lines(&hints(), &hint("<?>", "all keys"), 14, 1);
        assert_eq!(tiny, vec!["<?> all keys"]);
    }

    #[test]
    fn the_all_keys_list_aligns_the_labels() {
        let lines = all_keys_lines(&hints()[..2], 80);
        assert_eq!(lines, vec!["<ENTER>  go to pane", "<↓↑>     select"]);
    }
}
