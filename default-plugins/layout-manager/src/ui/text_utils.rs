use crate::DisplayLayout;
use unicode_width::UnicodeWidthStr;
use zellij_tile::prelude::*;

pub fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.chars().count() <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return text.chars().take(max_width).collect();
    }
    // Reserve 3 characters for "..."
    let truncate_at = max_width.saturating_sub(3);
    format!("{}...", text.chars().take(truncate_at).collect::<String>())
}

pub fn truncate_with_ellipsis_start(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.chars().count() <= max_width {
        return text.to_string();
    }
    if max_width <= 3 {
        return text
            .chars()
            .rev()
            .take(max_width)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
    }
    let truncate_at = max_width.saturating_sub(3);
    let suffix: String = text
        .chars()
        .rev()
        .take(truncate_at)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{}", suffix)
}

struct AnsiString<'a> {
    content: &'a str,
}

impl<'a> AnsiString<'a> {
    fn new(content: &'a str) -> Self {
        Self { content }
    }

    fn stripped_content(&self) -> String {
        strip_ansi_escapes::strip_str(self.content)
    }

    fn display_width(&self) -> usize {
        let stripped = self.stripped_content();
        UnicodeWidthStr::width(stripped.as_str())
    }

    fn fits_within(&self, max_width: usize) -> bool {
        self.display_width() <= max_width
    }

    fn truncate_to_width(&self, max_width: usize) -> String {
        if max_width == 0 {
            return String::new();
        }

        if self.fits_within(max_width) {
            return self.content.to_string();
        }

        let mut truncated = String::new();
        for ch in self.content.chars() {
            let test_string = format!("{}{}", truncated, ch);
            let test_ansi = AnsiString::new(&test_string);

            if test_ansi.display_width() > max_width {
                break;
            }

            truncated.push(ch);
        }

        truncated
    }

    fn with_reset_code(content: String) -> String {
        format!("{}\u{1b}[0m", content)
    }
}

pub fn truncate_line_with_ansi(line: &str, max_width: usize) -> String {
    let ansi_line = AnsiString::new(line);
    let truncated = ansi_line.truncate_to_width(max_width);
    AnsiString::with_reset_code(truncated)
}

pub fn wrap_text_to_width(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return Vec::new();
    }
    if max_width < 3 {
        return text.chars().map(|c| c.to_string()).collect();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in words.iter() {
        let word_to_add = if word.len() > max_width {
            truncate_with_ellipsis(word, max_width)
        } else {
            word.to_string()
        };

        if current_line.is_empty() {
            current_line = word_to_add;
        } else {
            let test_line = format!("{} {}", current_line, word_to_add);
            if test_line.len() <= max_width {
                current_line = test_line;
            } else {
                lines.push(current_line);
                current_line = word_to_add;
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    lines
}

pub fn get_layout_display_info(layout: &DisplayLayout) -> (String, Option<&LayoutMetadata>) {
    match layout {
        DisplayLayout::Valid(info) => match info {
            LayoutInfo::BuiltIn(name) => (name.clone(), None),
            LayoutInfo::File(path, metadata) => {
                let name = path.split('/').last().unwrap_or(path).to_string();
                (name, Some(metadata))
            },
            LayoutInfo::Url(url) => {
                let name = url.split('/').last().unwrap_or(url).to_string();
                (name, None)
            },
            LayoutInfo::Stringified(_) => ("raw".to_string(), None),
        },
        DisplayLayout::Error { name, .. } => (name.clone(), None),
    }
}

fn format_elapsed(unix_epoch_str: &str) -> String {
    let Ok(timestamp) = unix_epoch_str.parse::<i64>() else {
        return " ".to_string();
    };

    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let diff = now - timestamp;

    if diff < 60 {
        "now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

pub fn get_last_modified_string(metadata: Option<&LayoutMetadata>, is_builtin: bool) -> String {
    if let Some(metadata) = metadata {
        format_elapsed(&metadata.update_time)
    } else if is_builtin {
        "Built-in".to_string()
    } else {
        " ".to_string()
    }
}
