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
    Moved(Position),
}

impl From<crossterm::event::MouseEvent> for MouseEvent {
    fn from(event: crossterm::event::MouseEvent) -> Self {
        use crossterm::event::MouseEventKind;
        let line = event.row.saturating_sub(1) as i32;
        let column = event.column.saturating_sub(1);
        let position = Position::new(line, column);

        match event.kind {
            MouseEventKind::Down(button) => Self::Press(
                MouseButton::from(button),
                position,
            ),
            MouseEventKind::ScrollDown => Self::Press(MouseButton::WheelDown, position),
            MouseEventKind::ScrollUp => Self::Press(MouseButton::WheelUp, position),
            MouseEventKind::Up(_) => Self::Release(position),
            MouseEventKind::Drag(_) => Self::Hold(position),
            MouseEventKind::Moved => Self::Moved(position),
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

impl From<crossterm::event::MouseButton> for MouseButton {
    fn from(button: crossterm::event::MouseButton) -> Self {
        match button {
            crossterm::event::MouseButton::Left => Self::Left,
            crossterm::event::MouseButton::Right => Self::Right,
            crossterm::event::MouseButton::Middle => Self::Middle,
        }
    }
}
