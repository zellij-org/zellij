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

impl From<termion::event::MouseEvent> for MouseEvent {
    fn from(event: termion::event::MouseEvent) -> Self {
        match event {
            termion::event::MouseEvent::Press(button, x, y) => {
                Self::Press(MouseButton::from(button), Position::new(y - 1, x - 1))
            }
            termion::event::MouseEvent::Release(x, y) => Self::Release(Position::new(y - 1, x - 1)),
            termion::event::MouseEvent::Hold(x, y) => Self::Hold(Position::new(y - 1, x - 1)),
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

impl From<termion::event::MouseButton> for MouseButton {
    fn from(button: termion::event::MouseButton) -> Self {
        match button {
            termion::event::MouseButton::Left => Self::Left,
            termion::event::MouseButton::Right => Self::Right,
            termion::event::MouseButton::Middle => Self::Middle,
            termion::event::MouseButton::WheelUp => Self::WheelUp,
            termion::event::MouseButton::WheelDown => Self::WheelDown,
        }
    }
}
