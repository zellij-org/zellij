use super::*;

#[test]
fn selection_start() {
    let mut selection = Selection::default();
    selection.start(Position::new(10, 10));

    assert!(selection.active);
    assert_eq!(selection.start, Position::new(10, 10));
    assert_eq!(selection.end, Position::new(10, 10));
}

#[test]
fn selection_to() {
    let mut selection = Selection::default();
    selection.start(Position::new(10, 10));
    let is_active = selection.active;
    selection.to(Position::new(20, 30));

    assert_eq!(selection.active, is_active);
    assert_eq!(selection.end, Position::new(20, 30));
}

#[test]
fn selection_end() {
    let mut selection = Selection::default();
    selection.start(Position::new(10, 10));
    selection.end(Position::new(20, 30));

    assert!(!selection.active);
    assert_eq!(selection.end, Position::new(20, 30));
}

#[test]
fn contains() {
    struct TestCase<'a> {
        selection: &'a Selection,
        position: Position,
        result: bool,
    }

    let selection = Selection {
        start: Position::new(10, 5),
        end: Position::new(40, 20),
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    let test_cases = vec![
        TestCase {
            selection: &selection,
            position: Position::new(10, 5),
            result: true,
        },
        TestCase {
            selection: &selection,
            position: Position::new(10, 4),
            result: false,
        },
        TestCase {
            selection: &selection,
            position: Position::new(20, 0),
            result: true,
        },
        TestCase {
            selection: &selection,
            position: Position::new(20, 21),
            result: true,
        },
        TestCase {
            selection: &selection,
            position: Position::new(40, 19),
            result: true,
        },
        TestCase {
            selection: &selection,
            position: Position::new(40, 20),
            result: false,
        },
    ];

    for test_case in test_cases {
        let result = test_case.selection.contains(
            test_case.position.line.0 as usize,
            test_case.position.column.0,
        );
        assert_eq!(result, test_case.result)
    }
}

#[test]
fn sorted() {
    let selection = Selection {
        start: Position::new(1, 1),
        end: Position::new(10, 2),
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };
    let sorted_selection = selection.sorted();
    assert_eq!(selection.start, sorted_selection.start);
    assert_eq!(selection.end, sorted_selection.end);

    let selection = Selection {
        start: Position::new(10, 2),
        end: Position::new(1, 1),
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };
    let sorted_selection = selection.sorted();
    assert_eq!(selection.end, sorted_selection.start);
    assert_eq!(selection.start, sorted_selection.end);
}

#[test]
fn line_indices() {
    let selection = Selection {
        start: Position::new(1, 1),
        end: Position::new(10, 2),
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    assert_eq!(selection.line_indices(), (1..=10))
}

#[test]
fn move_up_inactive() {
    let start = Position::new(10, 1);
    let end = Position::new(20, 2);
    let mut inactive_selection = Selection {
        start,
        end,
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    inactive_selection.move_up(2);
    assert_eq!(inactive_selection.start, Position::new(8, 1));
    assert_eq!(inactive_selection.end, Position::new(18, 2));
    inactive_selection.move_up(10);
    assert_eq!(inactive_selection.start, Position::new(-2, 1));
    assert_eq!(inactive_selection.end, Position::new(8, 2));
}

#[test]
fn move_up_active() {
    let start = Position::new(10, 1);
    let end = Position::new(20, 2);
    let mut inactive_selection = Selection {
        start,
        end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    inactive_selection.move_up(2);
    assert_eq!(inactive_selection.start, Position::new(8, 1));
    assert_eq!(inactive_selection.end, end);
}

#[test]
fn move_down_inactive() {
    let start = Position::new(10, 1);
    let end = Position::new(20, 2);
    let mut inactive_selection = Selection {
        start,
        end,
        active: false,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    inactive_selection.move_down(2);
    assert_eq!(inactive_selection.start, Position::new(12, 1));
    assert_eq!(inactive_selection.end, Position::new(22, 2));
    inactive_selection.move_down(10);
    assert_eq!(inactive_selection.start, Position::new(22, 1));
    assert_eq!(inactive_selection.end, Position::new(32, 2));
}

#[test]
fn move_down_active() {
    let start = Position::new(10, 1);
    let end = Position::new(20, 2);
    let mut inactive_selection = Selection {
        start,
        end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: None,
    };

    inactive_selection.move_down(2);
    assert_eq!(inactive_selection.start, Position::new(12, 1));
    assert_eq!(inactive_selection.end, end);
}

#[test]
fn add_word_to_position_extend_line_above() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(10, 10);
    let last_word_end = Position::new(10, 15);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(9, 5);
    let word_end = Position::new(9, 6);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, word_start);
    assert_eq!(selection.end, selection_end);
}

#[test]
fn add_word_to_position_extend_line_below() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(20, 15);
    let last_word_end = Position::new(20, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(21, 5);
    let word_end = Position::new(21, 6);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, selection_start);
    assert_eq!(selection.end, word_end);
}

#[test]
fn add_word_to_position_reduce_from_above() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(10, 10);
    let last_word_end = Position::new(10, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(11, 5);
    let word_end = Position::new(11, 6);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, word_start);
    assert_eq!(selection.end, selection_end);
}

#[test]
fn add_word_to_position_reduce_from_below() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(20, 10);
    let last_word_end = Position::new(20, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(19, 5);
    let word_end = Position::new(19, 6);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, selection_start);
    assert_eq!(selection.end, word_end);
}

#[test]
fn add_word_to_position_extend_right() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(20, 10);
    let last_word_end = Position::new(20, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(20, 21);
    let word_end = Position::new(20, 23);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, selection_start);
    assert_eq!(selection.end, word_end);
}

#[test]
fn add_word_to_position_extend_left() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(10, 10);
    let last_word_end = Position::new(10, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(10, 5);
    let word_end = Position::new(10, 9);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, word_start);
    assert_eq!(selection.end, selection_end);
}

#[test]
fn add_word_to_position_reduce_from_left() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(10, 10);
    let last_word_end = Position::new(10, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(10, 20);
    let word_end = Position::new(10, 30);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, word_start);
    assert_eq!(selection.end, selection_end);
}

#[test]
fn add_word_to_position_reduce_from_right() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_word_start = Position::new(20, 10);
    let last_word_end = Position::new(20, 20);
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: Some((last_word_start, last_word_end)),
        last_added_line_index: None,
    };
    let word_start = Position::new(20, 5);
    let word_end = Position::new(20, 10);
    selection.add_word_to_position(word_start, word_end);

    assert_eq!(selection.start, selection_start);
    assert_eq!(selection.end, word_end);
}

#[test]
fn add_line_to_position_extend_upwards() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_added_line_index = 10;
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: Some(last_added_line_index),
    };
    let line_index_to_add = 9;
    let last_index_in_line = 21;
    selection.add_line_to_position(line_index_to_add, last_index_in_line);

    assert_eq!(selection.start, Position::new(line_index_to_add as i32, 0));
    assert_eq!(selection.end, selection_end);
}

#[test]
fn add_line_to_position_extend_downwards() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_added_line_index = 20;
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: Some(last_added_line_index),
    };
    let line_index_to_add = 21;
    let last_index_in_line = 21;
    selection.add_line_to_position(line_index_to_add, last_index_in_line);

    assert_eq!(selection.start, selection_start);
    assert_eq!(
        selection.end,
        Position::new(line_index_to_add as i32, last_index_in_line as u16)
    );
}

#[test]
fn add_line_to_position_reduce_from_below() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_added_line_index = 20;
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: Some(last_added_line_index),
    };
    let line_index_to_add = 19;
    let last_index_in_line = 21;
    selection.add_line_to_position(line_index_to_add, last_index_in_line);

    assert_eq!(selection.start, selection_start);
    assert_eq!(
        selection.end,
        Position::new(line_index_to_add as i32, last_index_in_line as u16)
    );
}

#[test]
fn add_line_to_position_reduce_from_above() {
    let selection_start = Position::new(10, 10);
    let selection_end = Position::new(20, 20);
    let last_added_line_index = 10;
    let mut selection = Selection {
        start: selection_start,
        end: selection_end,
        active: true,
        last_added_word_position: None,
        last_added_line_index: Some(last_added_line_index),
    };
    let line_index_to_add = 9;
    let last_index_in_line = 21;
    selection.add_line_to_position(line_index_to_add, last_index_in_line);

    assert_eq!(selection.start, Position::new(line_index_to_add as i32, 0));
    assert_eq!(selection.end, selection_end);
}
