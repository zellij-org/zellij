use unicode_width::UnicodeWidthStr;

const BASE_SEPARATOR_WIDTH: usize = 2;

pub fn calculate_available_cmd_width(
    cols: usize,
    folder_width: usize,
    overflow_indicator: Option<&String>,
    chain_width: usize,
    status_width: usize,
) -> usize {
    let mut separator_width = BASE_SEPARATOR_WIDTH;
    if overflow_indicator.is_some() {
        separator_width += 1;
    }
    if status_width > 0 {
        separator_width += 1;
    }

    let overflow_width = overflow_indicator.map(|s| s.chars().count()).unwrap_or(0);

    cols.saturating_sub(folder_width)
        .saturating_sub(chain_width)
        .saturating_sub(status_width)
        .saturating_sub(separator_width)
        .saturating_sub(overflow_width)
        .saturating_sub(2)
        .max(1)
}

pub fn truncate_middle(
    text: &str,
    max_width: usize,
    cursor_position: Option<usize>,
) -> (String, Option<usize>) {
    let text_width = text.width();

    if text_width <= max_width {
        return (text.to_string(), cursor_position);
    }

    if max_width < 5 {
        return truncate_minimal(text, max_width, cursor_position);
    }

    if let Some(cursor_char_idx) = cursor_position {
        return truncate_with_cursor(text, max_width, cursor_char_idx);
    }

    truncate_no_cursor(text, max_width)
}

fn truncate_minimal(
    text: &str,
    max_width: usize,
    cursor_position: Option<usize>,
) -> (String, Option<usize>) {
    let mut result = String::new();
    let mut current_width = 0;
    let mut new_cursor_pos = None;
    let mut char_pos = 0;

    for ch in text.chars() {
        let ch_width = ch.to_string().width();
        if current_width + ch_width <= max_width {
            result.push(ch);
            if let Some(cursor_char) = cursor_position {
                if char_pos == cursor_char {
                    new_cursor_pos = Some(char_pos);
                }
            }
            char_pos += 1;
            current_width += ch_width;
        } else {
            break;
        }
    }
    (result, new_cursor_pos)
}

fn truncate_no_cursor(text: &str, max_width: usize) -> (String, Option<usize>) {
    let available_for_text = max_width.saturating_sub(3);
    let left_width = available_for_text / 2;
    let right_width = available_for_text - left_width;

    let mut left_part = String::new();
    let mut current_width = 0;
    for ch in text.chars() {
        let ch_width = ch.to_string().width();
        if current_width + ch_width <= left_width {
            left_part.push(ch);
            current_width += ch_width;
        } else {
            break;
        }
    }

    let chars: Vec<char> = text.chars().collect();
    let mut right_part = String::new();
    let mut current_width = 0;
    for ch in chars.iter().rev() {
        let ch_width = ch.to_string().width();
        if current_width + ch_width <= right_width {
            right_part.insert(0, *ch);
            current_width += ch_width;
        } else {
            break;
        }
    }

    (format!("{}...{}", left_part, right_part), None)
}

fn truncate_with_cursor(
    text: &str,
    max_width: usize,
    cursor_char_idx: usize,
) -> (String, Option<usize>) {
    let chars: Vec<char> = text.chars().collect();
    let char_widths: Vec<usize> = chars.iter().map(|ch| ch.to_string().width()).collect();

    let width_before_cursor: usize = char_widths[..cursor_char_idx].iter().sum();
    let width_after_cursor: usize = char_widths[cursor_char_idx..].iter().sum();
    let available_one_ellipsis = max_width.saturating_sub(3);

    let (start_idx, end_idx) = if width_before_cursor <= available_one_ellipsis {
        calculate_end_truncation(&chars, &char_widths, available_one_ellipsis)
    } else if width_after_cursor <= available_one_ellipsis {
        calculate_start_truncation(&chars, &char_widths, available_one_ellipsis)
    } else {
        calculate_middle_truncation(&char_widths, max_width, cursor_char_idx)
    };

    let (start_idx, end_idx) =
        adjust_small_truncations(start_idx, end_idx, &char_widths, cursor_char_idx, &chars);
    let (start_idx, end_idx) =
        trim_excess(&char_widths, start_idx, end_idx, max_width, cursor_char_idx);

    build_truncated_result(&chars, start_idx, end_idx, cursor_char_idx)
}

fn calculate_end_truncation(
    chars: &[char],
    char_widths: &[usize],
    available: usize,
) -> (usize, usize) {
    let mut end_idx = 0;
    let mut width = 0;
    while end_idx < chars.len() && width + char_widths[end_idx] <= available {
        width += char_widths[end_idx];
        end_idx += 1;
    }
    (0, end_idx)
}

fn calculate_start_truncation(
    chars: &[char],
    char_widths: &[usize],
    available: usize,
) -> (usize, usize) {
    let mut start_idx = chars.len();
    let mut width = 0;
    while start_idx > 0 && width + char_widths[start_idx - 1] <= available {
        start_idx -= 1;
        width += char_widths[start_idx];
    }
    (start_idx, chars.len())
}

fn calculate_middle_truncation(
    char_widths: &[usize],
    max_width: usize,
    cursor_char_idx: usize,
) -> (usize, usize) {
    let available_both_ellipsis = max_width.saturating_sub(6);
    let target_before = available_both_ellipsis / 2;
    let target_after = available_both_ellipsis - target_before;

    let mut start_idx = cursor_char_idx;
    let mut width_before = 0;
    while start_idx > 0 && width_before + char_widths[start_idx - 1] <= target_before {
        start_idx -= 1;
        width_before += char_widths[start_idx];
    }

    let mut end_idx = cursor_char_idx;
    let mut width_after = 0;
    while end_idx < char_widths.len() && width_after + char_widths[end_idx] <= target_after {
        width_after += char_widths[end_idx];
        end_idx += 1;
    }

    let leftover_before = target_before.saturating_sub(width_before);
    let leftover_after = target_after.saturating_sub(width_after);

    if leftover_before > 0 {
        let mut extra = leftover_before;
        while end_idx < char_widths.len() && char_widths[end_idx] <= extra {
            extra -= char_widths[end_idx];
            end_idx += 1;
        }
    }
    if leftover_after > 0 {
        let mut extra = leftover_after;
        while start_idx > 0 && char_widths[start_idx - 1] <= extra {
            start_idx -= 1;
            extra -= char_widths[start_idx];
        }
    }

    (start_idx, end_idx)
}

fn adjust_small_truncations(
    start_idx: usize,
    end_idx: usize,
    char_widths: &[usize],
    cursor_char_idx: usize,
    chars: &[char],
) -> (usize, usize) {
    let width_truncated_start: usize = char_widths[..start_idx].iter().sum();
    let width_truncated_end: usize = char_widths[end_idx..].iter().sum();

    let mut start_idx = start_idx;
    let mut end_idx = end_idx;

    if width_truncated_start > 0 && width_truncated_start < 3 && end_idx < chars.len() {
        let gained = width_truncated_start;
        start_idx = 0;
        let mut removed = 0;
        while end_idx > cursor_char_idx + 1 && removed < gained {
            end_idx -= 1;
            removed += char_widths[end_idx];
        }
    } else if width_truncated_end > 0 && width_truncated_end < 3 && start_idx > 0 {
        let gained = width_truncated_end;
        end_idx = chars.len();
        let mut removed = 0;
        while start_idx < cursor_char_idx && removed < gained {
            removed += char_widths[start_idx];
            start_idx += 1;
        }
    }

    (start_idx, end_idx)
}

fn trim_excess(
    char_widths: &[usize],
    start_idx: usize,
    end_idx: usize,
    max_width: usize,
    cursor_char_idx: usize,
) -> (usize, usize) {
    let truncate_start = start_idx > 0;
    let truncate_end = end_idx < char_widths.len();
    let ellipsis_width = match (truncate_start, truncate_end) {
        (true, true) => 6,
        (true, false) | (false, true) => 3,
        (false, false) => 0,
    };

    let visible_width: usize = char_widths[start_idx..end_idx].iter().sum();
    let mut start_idx = start_idx;
    let mut end_idx = end_idx;

    if visible_width + ellipsis_width > max_width {
        let mut excess = visible_width + ellipsis_width - max_width;
        if truncate_end {
            while excess > 0 && end_idx > cursor_char_idx + 1 {
                end_idx -= 1;
                excess = excess.saturating_sub(char_widths[end_idx]);
            }
        }
        if excess > 0 && truncate_start {
            while excess > 0 && start_idx < cursor_char_idx {
                excess = excess.saturating_sub(char_widths[start_idx]);
                start_idx += 1;
            }
        }
    }

    (start_idx, end_idx)
}

fn build_truncated_result(
    chars: &[char],
    start_idx: usize,
    end_idx: usize,
    cursor_char_idx: usize,
) -> (String, Option<usize>) {
    let mut result = String::new();
    let mut new_cursor_char_pos = 0;
    let mut current_char_pos = 0;

    if start_idx > 0 {
        result.push_str("...");
        current_char_pos = 3;
    }

    for i in start_idx..end_idx {
        if i == cursor_char_idx {
            new_cursor_char_pos = current_char_pos;
        }
        result.push(chars[i]);
        current_char_pos += 1;
    }

    if cursor_char_idx >= end_idx {
        new_cursor_char_pos = current_char_pos;
    }

    if end_idx < chars.len() {
        result.push_str("...");
    }

    (result, Some(new_cursor_char_pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_middle_no_truncation_needed() {
        let text = "hello world";
        let (result, cursor_pos) = truncate_middle(text, 20, Some(6));
        assert_eq!(result, "hello world");
        assert_eq!(cursor_pos, Some(6));
    }

    #[test]
    fn test_truncate_middle_no_cursor() {
        let text = "this is a very long string that needs truncation";
        let (result, cursor_pos) = truncate_middle(text, 20, None);
        assert!(result.contains("..."));
        assert_eq!(cursor_pos, None);
    }

    #[test]
    fn test_truncate_middle_cursor_at_start() {
        let text = "this is a very long string that needs truncation";
        let (result, cursor_pos) = truncate_middle(text, 20, Some(0));
        assert!(result.starts_with("this"));
        assert!(result.ends_with("..."));
        assert_eq!(cursor_pos, Some(0));
    }

    #[test]
    fn test_truncate_middle_cursor_at_end() {
        let text = "this is a very long string that needs truncation";
        let cursor_char = text.chars().count();
        let (result, _cursor_pos) = truncate_middle(text, 20, Some(cursor_char));
        assert!(result.starts_with("..."));
        assert!(result.ends_with("truncation"));
    }

    #[test]
    fn test_truncate_middle_cursor_in_middle() {
        let text = "this is a very long string that needs truncation";
        let cursor_char = "this is a very ".chars().count();
        let (result, cursor_pos) = truncate_middle(text, 20, Some(cursor_char));
        assert!(result.contains("very"));
        assert!(cursor_pos.is_some());
    }

    #[test]
    fn test_truncate_middle_wide_chars() {
        let text = "こんにちは世界";
        let (result, _) = truncate_middle(text, 10, None);
        assert!(result.width() <= 10);
    }
}
