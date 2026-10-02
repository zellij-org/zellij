use serde::{Deserialize, Serialize};

use crate::position::Position;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
/// A mouse event can have any number of buttons (including no
/// buttons) pressed or released.
pub struct MouseEvent {
    /// A mouse event can current be a Press, Release, or Motion.
    /// Future events could consider double-click and triple-click.
    pub event_type: MouseEventType,

    // Mouse buttons associated with this event.
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,
    #[serde(default)]
    pub wheel_left: bool,
    #[serde(default)]
    pub wheel_right: bool,
    #[serde(default)]
    pub wheel_lines: u16,

    // Keyboard modifier flags can be encoded with events too.  They
    // are not often passed on the wire (instead used for
    // selection/copy-paste and changing terminal properties
    // on-the-fly at the user-facing terminal), but alt-mouseclick
    // usually passes through and is testable on vttest.  termwiz
    // already exposes them too.
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,

    /// The coordinates are zero-based.
    pub position: Position,
}

/// A mouse related event
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEventType {
    /// A mouse button was pressed.
    Press,
    /// A mouse button was released.
    Release,
    /// A mouse button is held over the given coordinates.
    Motion,
}

impl MouseEvent {
    pub fn new() -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position: Position::new(0, 0),
        };
        event
    }
    pub fn new_buttonless_motion(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_press_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_release_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_middle_motion_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: false,
            right: false,
            middle: true,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_right_press_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: true,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_left_press_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_motion_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_release_with_ctrl_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Release,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_left_motion_with_alt_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Motion,
            left: true,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_scroll_up_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: true,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_scroll_down_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: true,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_alt_scroll_up_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: true,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_alt_scroll_down_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: true,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
    pub fn new_ctrl_scroll_up_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: true,
            wheel_down: false,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_ctrl_scroll_down_event(position: Position) -> Self {
        let event = MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: true,
            wheel_left: false,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: true,
            position,
        };
        event
    }
    pub fn new_scroll_left_event(position: Position) -> Self {
        MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: true,
            wheel_right: false,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        }
    }
    pub fn new_scroll_right_event(position: Position) -> Self {
        MouseEvent {
            event_type: MouseEventType::Press,
            left: false,
            right: false,
            middle: false,
            wheel_up: false,
            wheel_down: false,
            wheel_left: false,
            wheel_right: true,
            wheel_lines: 0,
            shift: false,
            alt: false,
            ctrl: false,
            position,
        }
    }
}

impl Default for MouseEvent {
    fn default() -> Self {
        MouseEvent::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_server_contract::client_server_contract::MouseEvent as ProtoMouseEvent;
    use crate::plugin_api::action::ProtobufMouseEventPayload;

    fn touchpad_scroll(lines: u16) -> MouseEvent {
        let mut event = MouseEvent::new_scroll_up_event(Position::new(4, 7));
        event.wheel_lines = lines;
        event
    }

    #[test]
    fn a_classic_wheel_event_carries_no_line_count() {
        assert_eq!(MouseEvent::new_scroll_up_event(Position::new(0, 0)).wheel_lines, 0);
        assert_eq!(MouseEvent::new().wheel_lines, 0);
    }

    #[test]
    fn the_line_count_survives_json() {
        let event = touchpad_scroll(5);
        let json = serde_json::to_string(&event).unwrap();
        let back: MouseEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn json_from_a_peer_without_the_line_count_reads_as_a_classic_step() {
        let event = touchpad_scroll(5);
        let mut value = serde_json::to_value(&event).unwrap();
        value.as_object_mut().unwrap().remove("wheel_lines");
        let back: MouseEvent = serde_json::from_value(value).unwrap();
        assert_eq!(back.wheel_lines, 0);
        assert!(back.wheel_up);
    }

    #[test]
    fn the_line_count_survives_the_client_server_protobuf() {
        for lines in [0, 1, 9, u16::MAX] {
            let event = touchpad_scroll(lines);
            let proto: ProtoMouseEvent = event.into();
            let back = MouseEvent::try_from(proto).unwrap();
            assert_eq!(back, event);
        }
    }

    #[test]
    fn a_protobuf_event_without_the_line_count_reads_as_a_classic_step() {
        let mut proto: ProtoMouseEvent = touchpad_scroll(5).into();
        proto.wheel_lines = 0;
        assert_eq!(MouseEvent::try_from(proto).unwrap().wheel_lines, 0);
    }

    #[test]
    fn the_line_count_survives_the_plugin_protobuf() {
        for lines in [0, 3, u16::MAX] {
            let event = touchpad_scroll(lines);
            let payload = ProtobufMouseEventPayload::try_from(event).unwrap();
            let back = MouseEvent::try_from(payload).unwrap();
            assert_eq!(back, event);
        }
    }
}
