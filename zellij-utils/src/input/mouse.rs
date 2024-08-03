use serde::{Deserialize, Serialize};

use crate::position::Position;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
/// A mouse event can have any number of buttons (including no
/// buttons) pressed or released.
pub struct MouseButtons {
    pub left: bool,
    pub right: bool,
    pub middle: bool,
    pub wheel_up: bool,
    pub wheel_down: bool,
}

/// A mouse related event
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEvent {
    /// A mouse button was pressed.
    ///
    /// The coordinates are zero-based.
    Press(MouseButtons, Position),
    /// A mouse button was released.
    ///
    /// The coordinates are zero-based.
    Release(MouseButtons, Position),
    /// A mouse button is held over the given coordinates.
    ///
    /// The coordinates are zero-based.
    Motion(MouseButtons, Position),
}

impl MouseButtons {
    fn termwiz_mouse_buttons(mut self, button_bits: &termwiz::input::MouseButtons) -> MouseButtons {
        self.left = button_bits.contains(termwiz::input::MouseButtons::LEFT);
        self.right = button_bits.contains(termwiz::input::MouseButtons::RIGHT);
        self.middle = button_bits.contains(termwiz::input::MouseButtons::MIDDLE);
        self.wheel_up = button_bits.contains(termwiz::input::MouseButtons::VERT_WHEEL)
            && button_bits.contains(termwiz::input::MouseButtons::WHEEL_POSITIVE);
        self.wheel_down = button_bits.contains(termwiz::input::MouseButtons::VERT_WHEEL)
            && !button_bits.contains(termwiz::input::MouseButtons::WHEEL_POSITIVE);

        self
    }
}

impl MouseEvent {
    #[allow(unused)]
    pub fn from_termwiz(mut old_buttons: MouseButtons, event: termwiz::input::MouseEvent) -> Self {
        // We use the state of old_buttons vs new_buttons to determine
        // if this event is a Press, Release, or Motion.  This is an
        // unfortunate side effect of the pre-SGR-encoded X10 mouse
        // protocol design in which release events don't carry
        // information about WHICH button(s) were released, so we have
        // to maintain a wee bit of state in between events.
        //
        // Note that only Left, Right, and Middle are saved in between
        // calls.  WheelUp/WheelDown typically do not generate Release
        // events.
        let mut new_buttons = MouseButtons::termwiz_mouse_buttons(
            MouseButtons {
                left: false,
                right: false,
                middle: false,
                wheel_up: false,
                wheel_down: false,
            },
            &event.mouse_buttons,
        );

        if (new_buttons.left && !old_buttons.left)
            || (new_buttons.right && !old_buttons.right)
            || (new_buttons.middle && !old_buttons.middle)
            || new_buttons.wheel_up
            || new_buttons.wheel_down
        {
            // This is a mouse Press event.  Hang onto the button state.
            old_buttons = new_buttons;
            MouseEvent::Press(
                new_buttons,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::NONE)
            && !old_buttons.left
            && !old_buttons.right
            && !old_buttons.middle
        {
            // This is a mouse Motion event (no buttons are down).
            old_buttons = new_buttons;
            MouseEvent::Motion(
                new_buttons,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::NONE)
            && (old_buttons.left || old_buttons.right || old_buttons.middle)
        {
            // This is a mouse Release event.  Note that we set
            // old_buttons to false (to release), but set ONLY the
            // new_buttons that were released to true before sending
            // the event up.
            if old_buttons.left {
                old_buttons.left = false;
                new_buttons.left = true;
            }
            if old_buttons.right {
                old_buttons.right = false;
                new_buttons.right = true;
            }
            if old_buttons.middle {
                old_buttons.middle = false;
                new_buttons.middle = true;
            }

            MouseEvent::Release(
                new_buttons,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else {
            // Unrecognized mouse state.  Return it as a blank Motion event.
            MouseEvent::Motion(
                MouseButtons {
                    left: false,
                    right: false,
                    middle: false,
                    wheel_up: false,
                    wheel_down: false,
                },
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        }
    }
}
