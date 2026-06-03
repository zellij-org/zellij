//! Pure terminal-cell and ANSI helpers reused across screens: cursor
//! moves, visible-width measurement, horizontal slicing, cell-width text
//! fitting, and relative-time formatting.

use unicode_width::UnicodeWidthStr;

/// Resets the active style, emitted between UI cells so an SGR bleed
/// from the embedded viewport cannot contaminate the chrome.
pub(crate) const RESET: &str = "\x1b[0m";

/// ANSI cursor move. ANSI is 1-based; the plugin render area is 0-based.
pub(crate) fn move_to(row: usize, col: usize) -> String {
    format!("\x1b[{};{}H", row + 1, col + 1)
}

/// `Active <time> ago` relative to `now`, or `"—"` when no activity has
/// been recorded (the activity cache is delta-only).
pub(crate) fn format_time_ago(then_unix_secs: Option<u64>, now_unix_secs: u64) -> String {
    let Some(then) = then_unix_secs else {
        return "—".to_string();
    };
    let diff = now_unix_secs.saturating_sub(then);
    let body = if diff < 5 {
        "just now".to_string()
    } else if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    };
    format!("Active {}", body)
}

/// Pad with trailing spaces or truncate (with a trailing `…`) so the
/// cell width is exactly `width`. Width 0 returns empty.
pub(crate) fn pad_or_truncate(text: &str, width: usize) -> String {
    let text_w = UnicodeWidthStr::width(text);
    if width == 0 {
        return String::new();
    }
    if text_w == width {
        return text.to_string();
    }
    if text_w < width {
        return format!("{}{}", text, " ".repeat(width - text_w));
    }
    if width == 1 {
        // No room for both a char and an ellipsis: take one single-cell char.
        let first = text.chars().find(|c| char_width(*c) <= 1);
        return first.map(String::from).unwrap_or_else(|| " ".to_string());
    }
    let mut out = String::new();
    let mut taken = 0;
    let target = width - 1;
    for ch in text.chars() {
        let w = char_width(ch);
        if taken + w > target {
            break;
        }
        out.push(ch);
        taken += w;
    }
    out.push('…');
    let out_w = UnicodeWidthStr::width(out.as_str());
    out.push_str(&" ".repeat(width.saturating_sub(out_w)));
    out
}

fn char_width(ch: char) -> usize {
    let mut buf = [0u8; 4];
    UnicodeWidthStr::width(ch.encode_utf8(&mut buf) as &str)
}

/// Width of `text` in cells, ignoring ANSI escape sequences.
pub(crate) fn visible_width(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut width = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i = skip_escape(bytes, i);
            continue;
        }
        let ch_len = utf8_char_len(bytes[i]).max(1);
        if let Some(s) = bytes.get(i..i + ch_len).and_then(|b| std::str::from_utf8(b).ok()) {
            width += UnicodeWidthStr::width(s);
        }
        i += ch_len;
    }
    width
}

/// Slice `line` so that, emitted at column 0, it renders the cells the
/// original would have shown at `[h_offset, h_offset + max_cols)`. ANSI
/// escapes are preserved verbatim so SGR state carries into the window;
/// a trailing `RESET` stops it bleeding into the next row. Wide chars
/// straddling the left boundary become a space (column alignment);
/// those straddling the right are dropped (caller pads with `\x1b[K`).
pub(crate) fn slice_ansi_visible(line: &str, h_offset: usize, max_cols: usize) -> String {
    if max_cols == 0 {
        return String::new();
    }
    let bytes = line.as_bytes();
    let right_edge = h_offset.saturating_add(max_cols);
    let mut out = String::new();
    let mut cell = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            // Replay every escape walked past, visible or not, so the
            // first cell inside the window has the correct SGR state.
            let end = skip_escape(bytes, i);
            if let Ok(esc) = std::str::from_utf8(&bytes[i..end]) {
                out.push_str(esc);
            }
            i = end;
            continue;
        }
        let ch_len = utf8_char_len(bytes[i]).max(1);
        let end = (i + ch_len).min(bytes.len());
        let Ok(ch) = std::str::from_utf8(&bytes[i..end]) else {
            i = end;
            continue;
        };
        let w = UnicodeWidthStr::width(ch);
        if w == 0 {
            // Combining marks ride along with the previous emitted cell;
            // dropped at the slice start to avoid an orphan mark.
            if cell > h_offset && cell <= right_edge && !out.is_empty() {
                out.push_str(ch);
            }
        } else if cell + w <= h_offset {
            // Still left of the window.
        } else if cell < h_offset {
            out.push(' '); // wide char straddling the left boundary
        } else if cell >= right_edge || cell + w > right_edge {
            break; // at or past the right edge (straddlers dropped)
        } else {
            out.push_str(ch);
        }
        cell += w;
        i = end;
    }
    out.push_str(RESET);
    out
}

/// Advance past the ANSI escape starting at `start`, returning the index
/// of the first byte after it. Handles CSI, OSC, and stray two-byte
/// escapes; a coarse parse, sufficient for cell measurement and slicing.
fn skip_escape(bytes: &[u8], start: usize) -> usize {
    let mut i = start + 1;
    match bytes.get(i) {
        Some(b'[') => {
            // CSI: ESC [ <params> <final 0x40..=0x7E>
            i += 1;
            while i < bytes.len() && !(0x40..=0x7e).contains(&bytes[i]) {
                i += 1;
            }
            (i + 1).min(bytes.len())
        },
        Some(b']') => {
            // OSC: ESC ] <body> (BEL | ESC \)
            i += 1;
            while i < bytes.len()
                && bytes[i] != 0x07
                && !(bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'\\'))
            {
                i += 1;
            }
            match bytes.get(i) {
                Some(0x07) => i + 1,
                Some(0x1b) => (i + 2).min(bytes.len()),
                _ => i,
            }
        },
        Some(_) => i + 1,
        None => i,
    }
}

fn utf8_char_len(byte: u8) -> usize {
    match byte {
        b if b < 0xc0 => 1,
        b if b < 0xe0 => 2,
        b if b < 0xf0 => 3,
        _ => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Strip the trailing `RESET` so tests assert the visible cells.
    fn visible(s: &str) -> &str {
        s.strip_suffix(RESET).unwrap_or(s)
    }

    #[test]
    fn ascii_slice_inside_line() {
        assert_eq!(visible(&slice_ansi_visible("abcdefghij", 2, 4)), "cdef");
    }

    #[test]
    fn ascii_slice_at_left_edge() {
        assert_eq!(visible(&slice_ansi_visible("abcdefghij", 0, 4)), "abcd");
    }

    #[test]
    fn ascii_slice_past_right_edge() {
        assert_eq!(visible(&slice_ansi_visible("abcd", 1, 10)), "bcd");
    }

    #[test]
    fn empty_when_offset_past_line_width() {
        let sliced = slice_ansi_visible("abcd", 10, 4);
        assert_eq!(visible(&sliced), "");
        assert!(sliced.ends_with(RESET));
    }

    #[test]
    fn max_cols_zero_returns_empty() {
        assert_eq!(slice_ansi_visible("abcd", 0, 0), "");
    }

    #[test]
    fn ansi_escape_preserved_when_in_window() {
        let sliced = slice_ansi_visible("\x1b[31mred\x1b[0m end", 0, 7);
        assert!(sliced.contains("\x1b[31m"));
        assert!(sliced.contains("\x1b[0m"));
        assert!(sliced.contains("red"));
        assert!(sliced.contains("end"));
    }

    #[test]
    fn ansi_escape_replayed_when_offset_skips_text() {
        // Window covers the 'b' cells; the skipped-past red escape must
        // still appear so the visible region renders with correct SGR.
        let sliced = slice_ansi_visible("\x1b[31maaaa\x1b[32mbbbb", 4, 4);
        assert!(sliced.contains("\x1b[31m"));
        assert!(sliced.contains("\x1b[32m"));
        assert!(sliced.contains("bbbb"));
        assert!(!sliced.contains("aaaa"));
    }

    #[test]
    fn wide_char_straddling_left_boundary_becomes_space() {
        // "中" (2 cells) sits at cells 0..2; the slice starts at cell 1.
        assert_eq!(visible(&slice_ansi_visible("中abc", 1, 3)), " ab");
    }

    #[test]
    fn wide_char_straddling_right_boundary_dropped() {
        // "中" spans cells 2..4; the window [0, 3) clips its right half.
        assert_eq!(visible(&slice_ansi_visible("ab中cd", 0, 3)), "ab");
    }

    #[test]
    fn wide_char_entirely_inside_window() {
        assert_eq!(visible(&slice_ansi_visible("ab中cd", 0, 4)), "ab中");
    }
}
