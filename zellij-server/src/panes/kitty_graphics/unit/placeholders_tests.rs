use super::super::placeholders::*;

#[test]
fn table_is_sorted_for_binary_search() {
    for window in KITTY_DIACRITICS.windows(2) {
        assert!(window[0] < window[1]);
    }
}

#[test]
fn interner_roundtrip_and_dedup() {
    let mut interner = PlaceholderInterner::default();
    let a = PlaceholderCell {
        image_row: 3,
        image_column: 7,
        image_id_msb: 0,
    };
    let b = PlaceholderCell {
        image_row: 297,
        image_column: 297,
        image_id_msb: 255,
    };
    let encoded_a = interner.encode(a).unwrap();
    let encoded_b = interner.encode(b).unwrap();
    assert_eq!(interner.encode(a), Some(encoded_a));
    assert_eq!(interner.decode(encoded_a), Some(a));
    assert_eq!(interner.decode(encoded_b), Some(b));
    assert_ne!(encoded_a, encoded_b);
    assert!(is_in_encoded_range(encoded_a));
    assert!((encoded_b as u32) < PLACEHOLDER_CHAR as u32);
}

#[test]
fn decode_rejects_foreign_codepoints() {
    let interner = PlaceholderInterner::default();
    assert_eq!(interner.decode(PLACEHOLDER_CHAR), None);
    assert_eq!(interner.decode('a'), None);
    assert!(!is_in_encoded_range(PLACEHOLDER_CHAR));
    assert!(!is_in_encoded_range('\u{F0000}'));
}

#[test]
fn diacritic_lookup() {
    assert_eq!(diacritic_index('\u{0305}'), Some(0));
    assert_eq!(diacritic_index('\u{030D}'), Some(1));
    assert_eq!(diacritic_index('\u{1D244}'), Some(297));
    assert_eq!(diacritic_index('a'), None);
}
