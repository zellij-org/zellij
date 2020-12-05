/// This module is for defining the set of actions that can be taken in response to a keybind
/// and also passing actions back to the handler for dispatch.
use super::handler;

pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

pub enum Action {
    /// Quit mosaic
    Quit,
    /// Switch to the specified input mode
    ToMode(handler::InputMode),
    /// Resize focus pane in specified direction
    Resize(Direction),
    /// Switch focus to next pane in specified direction
    SwitchFocus(Direction),
    /// Scroll up in focus pane
    ScrollUp,
    /// Scroll down in focus pane
    ScrollDown,
    /// Toggle focus pane between fullscreen and normal layout
    ToggleFocusFullscreen,
    /// Open a new pane in specified direction (relative to focus)
    NewPane(Direction),
    /// Close focus pane
    CloseFocus,
}