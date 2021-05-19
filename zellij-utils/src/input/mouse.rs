use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Point {
    pub line: Line,
    pub column: Column,
}

impl Point {
    pub fn new(line: Line, column: Column) -> Self {
        Self { line, column }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Line(pub u16);
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Column(pub u16);

/// A mouse related event
#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum MouseEvent {
    /// A mouse button was pressed.
    ///
    /// The coordinates are zero-based.
    Press(MouseButton, Point),
    /// A mouse button was released.
    ///
    /// The coordinates are zero-based.
    Release(Point),
    /// A mouse button is held over the given coordinates.
    ///
    /// The coordinates are zero-based.
    Hold(Point),
}

impl From<termion::event::MouseEvent> for MouseEvent {
    fn from(event: termion::event::MouseEvent) -> Self {
        match event {
            termion::event::MouseEvent::Press(button, x, y) => Self::Press(
                MouseButton::from(button),
                Point::new(Line(y - 1), Column(x - 1)),
            ),
            termion::event::MouseEvent::Release(x, y) => {
                Self::Release(Point::new(Line(y - 1), Column(x - 1)))
            }
            termion::event::MouseEvent::Hold(x, y) => {
                Self::Hold(Point::new(Line(y - 1), Column(x - 1)))
            }
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
