//! Definition of the actions that can be bound to keys.

use super::command::RunCommandAction;
use super::layout::TabLayout;
use crate::data::InputMode;
use crate::input::options::OnForceClose;
use serde::{Deserialize, Serialize};

use crate::position::Position;

/// The four directions (left, right, up, down).
#[derive(Eq, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
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
///
/// Actions are accessible to the user via keybindings in `config.yaml`. All actions that don't
/// document what they serialize to (i.e. what text in the `config.yaml` maps to the specific
/// action), serialize to their exact name by default. For example: [`Action::Quit`] serializes to
/// `Quit` in `config.yaml`.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Action {
    /// Quit Zellij.
    Quit,
    /// Write raw bytes to the focused pane.
    ///
    /// Serializes to e.g.: `Write: [1, 2, 3]`.
    Write(Vec<u8>),
    /// Write Characters to the focused pane.
    ///
    /// Serializes to e.g.: `WriteChars: "Foobar"`.
    WriteChars(String),
    /// Switch to the specified [input mode](`InputMode`).
    ///
    /// Serializes to e.g.: `SwitchToMode: Locked`.
    SwitchToMode(InputMode),
    /// Resize focused pane in the specified [direction](`ResizeDirection`).
    ///
    /// Serializes to e.g.: `Resize: Left`.
    Resize(ResizeDirection),
    /// Switch focus to next pane to the right or below if on screen edge.
    FocusNextPane,
    /// Switch focus to previous pane to the left or above if on screen edge.
    FocusPreviousPane,
    /// Switch focus to pane with the next ID.
    ///
    /// Legacy, prefer using `FocusNextPane` or `FocusPreviousPane`.
    SwitchFocus,
    /// Switch focus towards the pane with greatest overlap in specified [direction](`Direction`).
    ///
    /// Serializes to e.g.: `MoveFocus: Up`.
    MoveFocus(Direction),
    /// Tries to move the focused pane in specified [direction](`Direction`).
    ///
    /// If there is no pane in the direction, move to previous/next Tab.
    /// Serializes to e.g.: `MoveFocusOrTab: Up`.
    MoveFocusOrTab(Direction),
    /// Move focused pane in specified [direction](`Direction`).
    ///
    /// If no direction is specified, move pane clockwise to next pane.
    /// Serializes to e.g.: `MovePane: ` or `MovePane: Up`.
    MovePane(Option<Direction>),
    /// Dumps the focused panes scrollback to a file.
    ///
    /// Serializes to e.g.: `DumpScreen: "/tmp/dump.txt"`.
    DumpScreen(String),
    /// Edit focused panes scrollback in default editor `$EDITOR`, or `$VISUAL`.
    EditScrollback,
    /// Scroll up one line in the focused pane.
    ScrollUp,
    /// Scroll up at given point.
    ///
    /// Triggered when scrolling while mouse selection is active.
    /// Not meant for direct user-interaction.
    ScrollUpAt(Position),
    /// Scroll down one line in the focused pane.
    ScrollDown,
    /// Scroll down at given point.
    ///
    /// Triggered when scrolling while mouse selection is active.
    /// Not meant for direct user-interaction.
    ScrollDownAt(Position),
    /// Scroll down to bottom in focused pane.
    ScrollToBottom,
    /// Scroll up one page in the focused pane.
    PageScrollUp,
    /// Scroll down one page in the focused pane.
    PageScrollDown,
    /// Scroll up half page in the focused pane.
    HalfPageScrollUp,
    /// Scroll down half page in the focused pane.
    HalfPageScrollDown,
    /// Toggle between fullscreen focused pane and normal layout.
    ToggleFocusFullscreen,
    /// Toggle frames around panes in the UI.
    TogglePaneFrames,
    /// Toggle between sending text commands to all panes on the current tab and just the focused
    /// pane.
    ToggleActiveSyncTab,
    /// Open a new pane in the specified direction (relative to focus).
    ///
    /// If no direction is specified, will try to use the biggest available space.
    /// Serializes to e.g.: `NewPane: ` or `NewPane: Right`.
    NewPane(Option<Direction>),
    /// Embed focused pane in tab if floating or float focused pane if embedded.
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all floating panes (if any) in the current Tab.
    ToggleFloatingPanes,
    /// Close the focused pane.
    CloseFocus,
    /// Rename a pane.
    ///
    /// Not meant for direct user-interaction.
    PaneNameInput(Vec<u8>),
    /// Undo pane renaming.
    ///
    /// Not meant for direct user-interaction.
    UndoRenamePane,
    /// Create a new tab, optionally with a specified tab layout.
    ///
    /// Serializes to e.g.: `NewTab: ` to create a new default tab.
    /// You can specify a layout, too. This creates a vertically split tab:
    /// ```ignore
    /// NewTab: {direction: Vertical,
    /// parts: [
    ///     direction: Vertical,
    ///     direction: Horizontal,
    /// ],}
    /// ```
    /// Any valid layout specification can be used.
    NewTab(Option<TabLayout>),
    /// Do nothing.
    ///
    /// Not meant for direct user-interaction.
    NoOp,
    /// Go to the next tab.
    GoToNextTab,
    /// Go to the previous tab.
    GoToPreviousTab,
    /// Close the current tab.
    CloseTab,
    /// Go to the tab with specified index.
    ///
    /// Counting begins at 1.
    /// Serializes to e.g.: `GoToTab: 1`.
    GoToTab(u32),
    /// Switch between the most recently used tabs.
    ToggleTab,
    /// Rename a tab.
    ///
    /// Not meant for direct user-interaction.
    TabNameInput(Vec<u8>),
    /// Undo tab renaming.
    ///
    /// Not meant for direct user-interaction.
    UndoRenameTab,
    /// Run specified command in new pane.
    ///
    /// To execute `ls` in a vertically split pane, use:
    /// ```ignore
    /// Run: {cmd: "/usr/bin/ls", args: ["-l"], cwd: "/home/ahartmann", direction: Left}
    /// ```
    Run(RunCommandAction),
    /// Detach from the currently running Zellij session and exit.
    Detach,
    /// Generated when clicking into application window.
    ///
    /// Not meant for direct user-interaction.
    LeftClick(Position),
    /// Generated when clicking into application window.
    ///
    /// Not meant for direct user-interaction.
    RightClick(Position),
    /// Generated when releasing the mouse button in the application window.
    ///
    /// Not meant for direct user-interaction.
    MouseRelease(Position),
    /// Generated when holding the mouse button in the application window.
    ///
    /// Not meant for direct user-interaction.
    MouseHold(Position),
    /// Copy the current selection.
    Copy,
    /// Confirm a prompt.
    ///
    /// Not meant for direct user-interaction.
    Confirm,
    /// Deny a prompt.
    ///
    /// Not meant for direct user-interaction.
    Deny,
    /// Confirm an action that invokes a prompt automatically.
    ///
    /// Not meant for direct user-interaction.
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ActionsFromYaml(Vec<Action>);

impl ActionsFromYaml {
    /// Get a reference to the actions from yaml's actions.
    pub fn actions(&self) -> &[Action] {
        self.0.as_ref()
    }
}
