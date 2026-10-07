use crate::input_handler::from_termwiz;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::position::Position;
use zellij_utils::vendored::termwiz::input::{InputEvent, InputParser};

fn mouse_event_from_host_terminal(bytes: &[u8]) -> MouseEvent {
    let mut events = InputParser::new().parse_as_vec(bytes, false);
    assert_eq!(events.len(), 1, "expected a single event, got {:?}", events);
    match events.remove(0) {
        InputEvent::Mouse(event) => from_termwiz(&mut MouseEvent::new(), event),
        other => panic!("expected a mouse event, got {:?}", other),
    }
}

#[test]
fn horizontal_scroll_left_from_host_terminal_is_read_as_left() {
    let event = mouse_event_from_host_terminal(b"\x1b[<66;42;12M");
    assert!(event.wheel_left);
    assert!(!event.wheel_right);
    assert!(!event.wheel_up && !event.wheel_down);
    assert_eq!(event.position, Position::new(11, 41));
}

#[test]
fn horizontal_scroll_right_from_host_terminal_is_read_as_right() {
    let event = mouse_event_from_host_terminal(b"\x1b[<67;42;12M");
    assert!(event.wheel_right);
    assert!(!event.wheel_left);
    assert!(!event.wheel_up && !event.wheel_down);
    assert_eq!(event.position, Position::new(11, 41));
}
