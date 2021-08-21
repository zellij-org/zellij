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
fn selection_end_with_position() {
    let mut selection = Selection::default();
    selection.start(Position::new(10, 10));
    selection.end(Some(&Position::new(20, 30)));

    assert!(!selection.active);
    assert_eq!(selection.end, Position::new(20, 30));
}

#[test]
fn selection_end_without_position() {
    let mut selection = Selection::default();
    selection.start(Position::new(10, 10));
    selection.to(Position::new(15, 100));
    selection.end(None);

    assert!(!selection.active);
    assert_eq!(selection.end, Position::new(15, 100));
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
    };
    let sorted_selection = selection.sorted();
    assert_eq!(selection.start, sorted_selection.start);
    assert_eq!(selection.end, sorted_selection.end);

    let selection = Selection {
        start: Position::new(10, 2),
        end: Position::new(1, 1),
        active: false,
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
    };

    inactive_selection.move_down(2);
    assert_eq!(inactive_selection.start, Position::new(12, 1));
    assert_eq!(inactive_selection.end, end);
}
