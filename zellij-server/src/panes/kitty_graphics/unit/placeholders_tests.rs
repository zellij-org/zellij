use super::super::placeholders::*;
use crate::panes::terminal_character::TerminalCharacter;

fn placeholder(marks: &[char]) -> TerminalCharacter {
    let mut cell = TerminalCharacter::new(PLACEHOLDER_CHAR);
    for mark in marks {
        assert!(cell.attach(*mark));
    }
    cell
}

fn cell(image_row: u32, image_column: u32, image_id_msb: u32) -> PlaceholderCell {
    PlaceholderCell {
        image_row,
        image_column,
        image_id_msb,
    }
}

#[test]
fn table_is_sorted_for_binary_search() {
    for window in KITTY_DIACRITICS.windows(2) {
        assert!(window[0] < window[1]);
    }
}

#[test]
fn diacritic_lookup() {
    assert_eq!(diacritic_index('\u{0305}'), Some(0));
    assert_eq!(diacritic_index('\u{030D}'), Some(1));
    assert_eq!(diacritic_index('\u{1D244}'), Some(297));
    assert_eq!(diacritic_index('a'), None);
}

#[test]
fn all_three_diacritics_are_decoded() {
    let decoded = decode_placeholder(&placeholder(&['\u{030D}', '\u{030E}', '\u{0310}']), None);
    assert_eq!(decoded, cell(1, 2, 3));
}

#[test]
fn missing_diacritics_follow_the_left_to_right_rules() {
    let left = Some(cell(1, 4, 7));
    // no diacritics: same row, next column, same id byte
    assert_eq!(decode_placeholder(&placeholder(&[]), left), cell(1, 5, 7));
    // no left neighbour: everything defaults to zero
    assert_eq!(decode_placeholder(&placeholder(&[]), None), cell(0, 0, 0));
    // row only, same row as the left cell: continue it
    assert_eq!(
        decode_placeholder(&placeholder(&['\u{030D}']), left),
        cell(1, 5, 7)
    );
    // row only, different row: start a new row at column 0
    assert_eq!(
        decode_placeholder(&placeholder(&['\u{0305}']), left),
        cell(0, 0, 0)
    );
    // row and column continuing the left cell: inherit the id byte
    assert_eq!(
        decode_placeholder(&placeholder(&['\u{030D}', '\u{033D}']), left),
        cell(1, 5, 7)
    );
    // row and column not continuing it: id byte is zero
    assert_eq!(
        decode_placeholder(&placeholder(&['\u{030D}', '\u{0305}']), left),
        cell(1, 0, 0)
    );
}
