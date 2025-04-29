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
            shift: false,
            alt: true,
            ctrl: false,
            position,
        };
        event
    }
}
