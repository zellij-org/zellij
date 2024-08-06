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

    fn termwiz_mouse_convert(&mut self, event: &termwiz::input::MouseEvent) {
        let button_bits = &event.mouse_buttons;
        self.left = button_bits.contains(termwiz::input::MouseButtons::LEFT);
        self.right = button_bits.contains(termwiz::input::MouseButtons::RIGHT);
        self.middle = button_bits.contains(termwiz::input::MouseButtons::MIDDLE);
        self.wheel_up = button_bits.contains(termwiz::input::MouseButtons::VERT_WHEEL)
            && button_bits.contains(termwiz::input::MouseButtons::WHEEL_POSITIVE);
        self.wheel_down = button_bits.contains(termwiz::input::MouseButtons::VERT_WHEEL)
            && !button_bits.contains(termwiz::input::MouseButtons::WHEEL_POSITIVE);

        let mods = &event.modifiers;
        self.shift = mods.contains(termwiz::input::Modifiers::SHIFT);
        self.alt = mods.contains(termwiz::input::Modifiers::ALT);
        self.ctrl = mods.contains(termwiz::input::Modifiers::CTRL);
    }

    pub fn from_termwiz(old_event: &mut MouseEvent, event: termwiz::input::MouseEvent) -> Self {
        // We use the state of old_event vs new_event to determine if
        // this event is a Press, Release, or Motion.  This is an
        // unfortunate side effect of the pre-SGR-encoded X10 mouse
        // protocol design in which release events don't carry
        // information about WHICH button(s) were released, so we have
        // to maintain a wee bit of state in between events.
        //
        // Note that only Left, Right, and Middle are saved in between
        // calls.  WheelUp/WheelDown typically do not generate Release
        // events.
        let mut new_event = MouseEvent::new();
        new_event.termwiz_mouse_convert(&event);
        new_event.position =
            Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1));

        if (new_event.left && !old_event.left)
            || (new_event.right && !old_event.right)
            || (new_event.middle && !old_event.middle)
            || new_event.wheel_up
            || new_event.wheel_down
        {
            // This is a mouse Press event.
            new_event.event_type = MouseEventType::Press;

            // Hang onto the button state.
            *old_event = new_event;
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::NONE)
            && !old_event.left
            && !old_event.right
            && !old_event.middle
        {
            // This is a mouse Motion event (no buttons are down).
            new_event.event_type = MouseEventType::Motion;

            // Hang onto the button state.
            *old_event = new_event;
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::NONE)
            && (old_event.left || old_event.right || old_event.middle)
        {
            // This is a mouse Release event.  Note that we set
            // old_event.{button} to false (to release), but set ONLY
            // the new_event that were released to true before sending
            // the event up.
            if old_event.left {
                old_event.left = false;
                new_event.left = true;
            }
            if old_event.right {
                old_event.right = false;
                new_event.right = true;
            }
            if old_event.middle {
                old_event.middle = false;
                new_event.middle = true;
            }
            new_event.event_type = MouseEventType::Release;
        } else {
            // Unrecognized mouse state.  Return it as a blank Motion event.
            new_event.event_type = MouseEventType::Motion;
        }

        new_event
    }
}
