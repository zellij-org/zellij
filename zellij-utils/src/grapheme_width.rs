use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

/// Returns the display width of a single grapheme cluster as the grid assigns it.
///
/// This is the canonical width policy for Zellij: it matches what
/// `TerminalCharacter::push_scalar` computes when building a cell, so that
/// consumers of serialised pane content (e.g. `PaneContents::get_selected_text`)
/// use the same column arithmetic as the terminal grid itself.
///
/// Key rules that differ from a plain `UnicodeWidthStr::width` call:
/// - Spacing Combining Marks (Unicode category Mc, e.g. Indic vowel signs like ि or ா)
///   are treated as width 0 — they attach to the preceding base character visually.
/// - Keycap sequences (#/*/0–9 + U+FE0F + U+20E3) are always width 1.
/// - Everything else (VS16 widening, ZWJ sequences, flag pairs, skin tones) defers
///   to `UnicodeWidthStr::width` on the full grapheme string.
pub fn grapheme_display_width(g: &str) -> usize {
    let mut chars = g.chars();
    let base = match chars.next() {
        None => return 0,
        Some(c) => c,
    };

    let base_width = UnicodeWidthChar::width(base).unwrap_or(0);

    // Fast path: single-codepoint grapheme (the vast majority of cells).
    let rest_start = base.len_utf8();
    if rest_start == g.len() {
        return base_width;
    }

    // Multi-codepoint grapheme — apply the same special cases as push_scalar.
    let is_keycap_base = matches!(base, '#' | '*' | '0'..='9');
    let mut width = base_width;

    for c in chars {
        if c == '\u{20E3}' && is_keycap_base {
            // Combining Enclosing Keycap on a keycap base — always width 1.
            return 1;
        } else if c == '\u{FE0F}' && is_keycap_base {
            // VS16 on a keycap base: don't widen.
        } else if UnicodeWidthChar::width(c) != Some(1) || ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
        {
            // Recompute from full string: handles VS16 widening, ZWJ sequences,
            // flag pairs (RI + RI), skin-tone modifiers, etc.
            width = UnicodeWidthStr::width(g);
            break;
        }
        // Width-1 non-RI characters are Mc spacing combining marks.
        // They visually attach to the base glyph and contribute 0 extra columns.
    }

    width
}

#[cfg(test)]
mod tests {
    use super::grapheme_display_width;

    // ── Width tests ───────────────────────────────────────────────────────────

    #[test]
    fn ascii_is_width_1() {
        assert_eq!(grapheme_display_width("a"), 1);
        assert_eq!(grapheme_display_width("Z"), 1);
    }

    #[test]
    fn cjk_is_width_2() {
        assert_eq!(grapheme_display_width("中"), 2);
        assert_eq!(grapheme_display_width("한"), 2);
    }

    #[test]
    fn devanagari_with_vowel_sign_is_width_1() {
        // ह (U+0939) + ि (U+093F, Mc) — renders in one cell.
        let g = "हि";
        assert_eq!(grapheme_display_width(g), 1, "हि must be width 1");
    }

    #[test]
    fn tamil_with_vowel_sign_is_width_1() {
        // க (U+0B95) + ி (U+0BBF, Mc) — renders in one cell.
        let g = "கி";
        assert_eq!(grapheme_display_width(g), 1, "கி must be width 1");
    }

    #[test]
    fn combining_grave_does_not_change_width() {
        // a (width 1) + U+0300 (combining grave, Mn, width 0) → width 1.
        let g = "a\u{0300}";
        assert_eq!(grapheme_display_width(g), 1);
    }

    #[test]
    fn keycap_sequence_is_width_1() {
        // '#' + VS16 (U+FE0F) + U+20E3 (Combining Enclosing Keycap).
        let g = "#\u{FE0F}\u{20E3}";
        assert_eq!(grapheme_display_width(g), 1);
    }

    #[test]
    fn flag_sequence_is_width_2() {
        // U+1F1EF (RI J) + U+1F1F5 (RI P) → JP flag, width 2.
        let g = "\u{1F1EF}\u{1F1F5}";
        assert_eq!(grapheme_display_width(g), 2);
    }

    #[test]
    fn zwj_emoji_is_width_2() {
        // Woman technologist: U+1F469 + ZWJ + U+1F4BB.
        let g = "\u{1F469}\u{200D}\u{1F4BB}";
        assert_eq!(grapheme_display_width(g), 2, "👩‍💻 must be width 2");
    }

    #[test]
    fn vs16_widens_text_symbol() {
        // U+2764 (heavy black heart, text presentation, width 1) + VS16 → width 2.
        let g = "\u{2764}\u{FE0F}";
        assert_eq!(grapheme_display_width(g), 2);
    }

    #[test]
    fn empty_string_is_width_0() {
        assert_eq!(grapheme_display_width(""), 0);
    }

    // ── Selection extraction tests ────────────────────────────────────────────
    // These mirror how data.rs uses grapheme_display_width to slice pane content.

    use unicode_segmentation::UnicodeSegmentation;

    fn extract_cols(line: &str, start_col: usize, end_col: usize) -> String {
        let mut col = 0;
        let mut result = String::new();
        let mut capturing = false;
        for g in line.graphemes(true) {
            let w = grapheme_display_width(g);
            if col >= start_col && !capturing {
                capturing = true;
            }
            if col >= end_col {
                break;
            }
            if capturing {
                result.push_str(g);
            }
            col += w;
        }
        result
    }

    #[test]
    fn select_after_indic_grapheme() {
        // "हिx": हि occupies col 0 (width 1), x occupies col 1 (width 1).
        let line = "हिx";
        assert_eq!(extract_cols(line, 0, 1), "हि");
        assert_eq!(extract_cols(line, 1, 2), "x");
    }

    #[test]
    fn select_after_flag() {
        // JP flag (width 2) then 'a' at col 2.
        let line = "\u{1F1EF}\u{1F1F5}a";
        assert_eq!(extract_cols(line, 0, 2), "\u{1F1EF}\u{1F1F5}");
        assert_eq!(extract_cols(line, 2, 3), "a");
    }

    #[test]
    fn select_combining_mark_cell() {
        // "à" (a + U+0300, width 1) then "b" at col 1.
        let line = "a\u{0300}b";
        assert_eq!(extract_cols(line, 0, 1), "a\u{0300}");
        assert_eq!(extract_cols(line, 1, 2), "b");
    }

    #[test]
    fn multiline_selection_uses_same_width_rules() {
        // First line: "हिa" (cols 0=हि, 1=a); second line: "bc".
        let first = "हिa";
        let second = "bc";
        // From col 1 of first line to end, then full second line to col 1.
        let mut result = String::new();
        result.push_str(&extract_cols(first, 1, usize::MAX));
        result.push('\n');
        result.push_str(&extract_cols(second, 0, 1));
        assert_eq!(result, "a\nb");
    }
}
