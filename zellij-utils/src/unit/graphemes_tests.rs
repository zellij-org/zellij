use super::*;

const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
const THUMBS_TONE: &str = "\u{1F44D}\u{1F3FD}";
const FLAG_DE: &str = "\u{1F1E9}\u{1F1EA}";
const E_ACUTE: &str = "e\u{0301}";
const HANGUL_LVT: &str = "\u{1100}\u{1161}\u{11A8}";
const ENGLAND: &str = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
const KEYCAP: &str = "1\u{FE0F}\u{20E3}";
const KSSA: &str = "\u{0915}\u{094D}\u{0937}";
const WARNING_VS16: &str = "\u{26A0}\u{FE0F}";

#[test]
fn display_width_of_every_category() {
    let cases: &[(&str, usize)] = &[
        ("abc", 3),
        ("你好", 4),
        (E_ACUTE, 1),
        (WARNING_VS16, 2),
        ("\u{231A}\u{FE0E}", 1),
        (KEYCAP, 2),
        (THUMBS_TONE, 2),
        (FLAG_DE, 2),
        (FAMILY, 2),
        ("\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}", 2),
        (ENGLAND, 2),
        (KSSA, 2),
        (HANGUL_LVT, 2),
        ("\u{FF76}\u{FF9E}", 1),
        ("\u{00AD}", 0),
        ("\u{17D8}", 3),
    ];
    for (text, width) in cases {
        assert_eq!(display_width(text), *width, "text={:?}", text);
    }
}

#[test]
fn clusters_keep_sequences_whole() {
    let text = format!("a{}{}b", FAMILY, FLAG_DE);
    let parts: Vec<(&str, usize)> = clusters(&text).collect();
    assert_eq!(parts, vec![("a", 1), (FAMILY, 2), (FLAG_DE, 2), ("b", 1)]);
}

fn assert_no_split(original: &str, truncated: &str) {
    for piece in truncated.split('…') {
        for cluster in clusters(piece).map(|(c, _)| c) {
            assert!(
                original.contains(cluster),
                "cluster {:?} not part of {:?}",
                cluster,
                original
            );
        }
        if let Some((first, _)) = clusters(piece).next() {
            assert_ne!(
                cluster_width(first),
                0,
                "piece starts with a zero-width cluster"
            );
        }
    }
}

#[test]
fn truncate_end_never_splits_a_cluster() {
    let text = format!("{}{}{}{}abc", FAMILY, E_ACUTE, FLAG_DE, THUMBS_TONE);
    for max in 0..12 {
        let truncated = truncate_end(&text, max, "…");
        let width = display_width(&truncated);
        assert!(width <= max, "max={} got={:?}", max, truncated);
        if display_width(&text) > max && max > 0 {
            assert!(width + 1 >= max, "max={} got={:?}", max, truncated);
        }
        assert_no_split(&text, &truncated);
    }
}

#[test]
fn truncate_start_never_splits_a_cluster() {
    let text = format!("abc{}{}{}", FAMILY, E_ACUTE, FLAG_DE);
    for max in 0..10 {
        let truncated = truncate_start(&text, max, "…");
        let width = display_width(&truncated);
        assert!(width <= max, "max={} got={:?}", max, truncated);
        if display_width(&text) > max && max > 0 {
            assert!(width + 1 >= max, "max={} got={:?}", max, truncated);
        }
        assert_no_split(&text, &truncated);
    }
}

#[test]
fn truncate_middle_never_splits_a_cluster() {
    let text = format!("{}{}abcdef{}{}", FAMILY, KSSA, E_ACUTE, FLAG_DE);
    for max in 0..14 {
        let truncated = truncate_middle(&text, max, "…");
        let width = display_width(&truncated);
        assert!(width <= max, "max={} got={:?}", max, truncated);
        if display_width(&text) > max && max > 0 {
            assert!(width + 1 >= max, "max={} got={:?}", max, truncated);
        }
        assert_no_split(&text, &truncated);
    }
}

#[test]
fn truncation_keeps_short_text_borrowed() {
    assert!(matches!(truncate_end("abc", 3, "…"), Cow::Borrowed("abc")));
    assert!(matches!(
        truncate_start("abc", 5, "…"),
        Cow::Borrowed("abc")
    ));
    assert!(matches!(
        truncate_middle("abc", 3, "…"),
        Cow::Borrowed("abc")
    ));
}

#[test]
fn pop_cluster_removes_whole_clusters() {
    for cluster in [FAMILY, FLAG_DE, E_ACUTE, HANGUL_LVT, ENGLAND, KEYCAP] {
        let mut text = format!("x{}", cluster);
        assert_eq!(pop_cluster(&mut text).as_deref(), Some(cluster));
        assert_eq!(text, "x");
    }
    let mut empty = String::new();
    assert_eq!(pop_cluster(&mut empty), None);
}

#[test]
fn cluster_index_of_char_maps_character_positions() {
    let text = format!("a{}b", FAMILY);
    assert_eq!(cluster_index_of_char(&text, 0), Some(0));
    assert_eq!(cluster_index_of_char(&text, 1), Some(1));
    assert_eq!(cluster_index_of_char(&text, 5), Some(1));
    assert_eq!(cluster_index_of_char(&text, 6), Some(2));
    assert_eq!(cluster_index_of_char(&text, 7), None);
}

#[test]
fn cluster_navigation_moves_by_whole_clusters() {
    let text = format!("a{}b", FAMILY);
    let family_end = 1 + FAMILY.len();
    assert_eq!(next_cluster_end(&text, 1), family_end);
    assert_eq!(previous_cluster_start(&text, family_end), 1);
    assert_eq!(previous_cluster_start(&text, 1), 0);
}

#[test]
fn strip_control_characters_keeps_clusters() {
    let text = format!("a\u{1b}]0;x\u{07}{}\n", FAMILY);
    assert_eq!(strip_control_characters(&text), format!("a]0;x{}", FAMILY));
}
