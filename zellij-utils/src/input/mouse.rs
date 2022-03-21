use serde::{Deserialize, Serialize};

use crate::position::Position;

/// A mouse related event
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEvent {
    /// A mouse button was pressed.
    ///
    /// The coordinates are zero-based.
    Press(MouseButton, Position),
    /// A mouse button was released.
    ///
    /// The coordinates are zero-based.
    Release(Position),
    /// A mouse button is held over the given coordinates.
    ///
    /// The coordinates are zero-based.
    Hold(Position),
}

impl From<termwiz::input::MouseEvent> for MouseEvent {
    fn from(event: termwiz::input::MouseEvent) -> Self {
        if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::LEFT)
        {
            MouseEvent::Press(
                MouseButton::Left,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::RIGHT)
        {
            MouseEvent::Press(
                MouseButton::Right,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::MIDDLE)
        {
            MouseEvent::Press(
                MouseButton::Middle,
                Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
            )
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::VERT_WHEEL)
        {
            if event
                .mouse_buttons
                .contains(termwiz::input::MouseButtons::WHEEL_POSITIVE)
            {
                MouseEvent::Press(
                    MouseButton::WheelUp,
                    Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
                )
            } else {
                MouseEvent::Press(
                    MouseButton::WheelDown,
                    Position::new(event.y.saturating_sub(1) as i32, event.x.saturating_sub(1)),
                )
            }
        } else if event
            .mouse_buttons
            .contains(termwiz::input::MouseButtons::NONE)
        {
            // release
            MouseEvent::Release(Position::new(
                event.y.saturating_sub(1) as i32,
                event.x.saturating_sub(1),
            ))
        } else {
            // this is an unsupported event, we just do this in order to send "something", but if
            // something happens here, we might want to add more specific support
            MouseEvent::Release(Position::new(
                event.y.saturating_sub(1) as i32,
                event.x.saturating_sub(1),
            ))
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum MouseButton {
    /// The left mouse button.
    Left,
    /// The right mouse button.
    Right,
    /// The middle mouse button.
    Middle,
    /// Mouse wheel is going up.
    ///
    /// This event is typically only used with Mouse::Press.
    WheelUp,
    /// Mouse wheel is going down.
    ///
    /// This event is typically only used with Mouse::Press.
    WheelDown,
}
