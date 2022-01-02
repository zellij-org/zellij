//! Definition of the actions that can be bound to keys.

use super::command::RunCommandAction;
use super::layout::TabLayout;
use crate::input::options::OnForceClose;
use serde::{Deserialize, Serialize};
use zellij_tile::data::InputMode;

use crate::position::Position;

/// The four directions (left, right, up, down).
#[derive(Eq, Clone, Debug, PartialEq, Deserialize, Serialize)]
#[derive(knuffel::DecodeScalar)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[derive(knuffel::DecodeScalar)]
pub enum ResizeDirection {
    Left,
    Right,
    Up,
    Down,
    Increase,
    Decrease,
}

// As these actions are bound to the default config, please
// do take care when refactoring - or renaming.
// They might need to be adjusted in the default config
// as well `../../assets/config/default.yaml`
/// Actions that can be bound to keys.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[derive(knuffel::Decode)]
pub enum Action {
    /// Quit Zellij.
    Quit,
    /// Write to the terminal.
    Write(#[knuffel(argument, bytes)] Vec<u8>),
    /// Write Characters to the terminal.
    WriteChars(#[knuffel(argument)] String),
    /// Switch to the specified input mode.
    SwitchToMode(#[knuffel(argument)] InputMode),
    /// Resize focus pane in specified direction.
    Resize(#[knuffel(argument)] ResizeDirection),
    /// Switch focus to next pane in specified direction.
    FocusNextPane,
    FocusPreviousPane,
    /// Move the focus pane in specified direction.
    SwitchFocus,
    MoveFocus(#[knuffel(argument)] Direction),
    /// Tries to move the focus pane in specified direction.
    /// If there is no pane in the direction, move to previous/next Tab.
    MoveFocusOrTab(#[knuffel(argument)] Direction),
    MovePane(#[knuffel(argument)] Option<Direction>),
    /// Scroll up in focus pane.
    ScrollUp,
    /// Scroll up at point
    #[knuffel(skip)]
    ScrollUpAt(Position),
    /// Scroll down in focus pane.
    ScrollDown,
    /// Scroll down at point
    #[knuffel(skip)]
    ScrollDownAt(Position),
    /// Scroll down to bottom in focus pane.
    ScrollToBottom,
    /// Scroll up one page in focus pane.
    PageScrollUp,
    /// Scroll down one page in focus pane.
    PageScrollDown,
    /// Scroll up half page in focus pane.
    HalfPageScrollUp,
    /// Scroll down half page in focus pane.
    HalfPageScrollDown,
    /// Toggle between fullscreen focus pane and normal layout.
    ToggleFocusFullscreen,
    /// Toggle frames around panes in the UI
    TogglePaneFrames,
    /// Toggle between sending text commands to all panes on the current tab and normal mode.
    ToggleActiveSyncTab,
    /// Open a new pane in the specified direction (relative to focus).
    /// If no direction is specified, will try to use the biggest available space.
    NewPane(#[knuffel(argument)] Option<Direction>),
    /// Close the focus pane.
    CloseFocus,
    PaneNameInput(#[knuffel(argument, bytes)] Vec<u8>),
    /// Create a new tab, optionally with a specified tab layout.
    NewTab(Option<TabLayout>),
    /// Do nothing.
    NoOp,
    /// Go to the next tab.
    GoToNextTab,
    /// Go to the previous tab.
    GoToPreviousTab,
    /// Close the current tab.
    CloseTab,
    GoToTab(#[knuffel(argument)] u32),
    ToggleTab,
    TabNameInput(#[knuffel(argument, bytes)] Vec<u8>),
    /// Run speficied command in new pane.
    Run(RunCommandAction),
    /// Detach session and exit
    Detach,
    #[knuffel(skip)]
    LeftClick(Position),
    #[knuffel(skip)]
    RightClick(Position),
    #[knuffel(skip)]
    MouseRelease(Position),
    #[knuffel(skip)]
    MouseHold(Position),
    Copy,
    /// Confirm a prompt
    Confirm,
    /// Deny a prompt
    Deny,
    /// Confirm an action that invokes a prompt automatically
    SkipConfirm(Box<Action>),
}

impl From<OnForceClose> for Action {
    fn from(ofc: OnForceClose) -> Action {
        match ofc {
            OnForceClose::Quit => Action::Quit,
            OnForceClose::Detach => Action::Detach,
        }
    }
}
