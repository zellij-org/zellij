//! Definition of the actions that can be bound to keys.

use super::command::RunCommandAction;
use super::layout::{Layout, PaneLayout};
use crate::cli::CliAction;
use crate::data::InputMode;
use crate::input::options::OnForceClose;
use serde::{Deserialize, Serialize};

use std::str::FromStr;
use std::path::PathBuf;

use crate::position::Position;

/// The four directions (left, right, up, down).
#[derive(Eq, Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}
impl FromStr for Direction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Left" | "left" => Ok(Direction::Left),
            "Right" | "right" => Ok(Direction::Right),
            "Up" | "up" => Ok(Direction::Up),
            "Down" | "down" => Ok(Direction::Down),
            _ => Err(format!("Failed to parse Direction. Unknown Direction: {}", s)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum ResizeDirection {
    Left,
    Right,
    Up,
    Down,
    Increase,
    Decrease,
}
impl FromStr for ResizeDirection {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Left" | "left" => Ok(ResizeDirection::Left),
            "Right" | "right" => Ok(ResizeDirection::Right),
            "Up" | "up" => Ok(ResizeDirection::Up),
            "Down" | "down" => Ok(ResizeDirection::Down),
            "Increase" | "increase" | "+" => Ok(ResizeDirection::Increase),
            "Decrease" | "decrease" | "-" => Ok(ResizeDirection::Decrease),
            _ => Err(format!("Failed to parse ResizeDirection. Unknown ResizeDirection: {}", s)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SearchDirection {
    Down,
    Up,
}

impl FromStr for SearchDirection {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Down" | "down" => Ok(SearchDirection::Down),
            "Up" | "up" => Ok(SearchDirection::Up),
            _ => Err(format!("Failed to parse SearchDirection. Unknown SearchDirection: {}", s)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum SearchOption {
    CaseSensitivity,
    WholeWord,
    Wrap,
}

impl FromStr for SearchOption {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CaseSensitivity" | "casesensitivity" | "Casesensitivity" => Ok(SearchOption::CaseSensitivity),
            "WholeWord" | "wholeword" | "Wholeword" => Ok(SearchOption::WholeWord),
            "Wrap" | "wrap" => Ok(SearchOption::Wrap),
            _ => Err(format!("Failed to parse SearchOption. Unknown SearchOption: {}", s)),
        }
    }
}

fn split_escaped_whitespace(s: &str) -> Vec<String> {
    s.split_ascii_whitespace().map(|s| String::from(s))
        .fold(vec![], |mut acc, part| {
            if let Some(previous_part) = acc.last_mut() {
                if previous_part.ends_with('\\') {
                    previous_part.push(' ');
                    previous_part.push_str(&part);
                    return acc;
                }
            }
            acc.push(part);
            acc
        })
}

fn split_command_and_args(command: String, args: Option<String>) -> (PathBuf, Vec<String>) {
    let mut full_command = split_escaped_whitespace(&command);
    let mut command = None;
    let mut command_args = vec![];
    for part in full_command.drain(..) {
        if command.is_none() {
            command = Some(part);
        } else {
            command_args.push(part);
        }
    }
    let command = PathBuf::from(command.unwrap());
    if let Some(additional_args) = args.map(|args| split_escaped_whitespace(&args)).as_mut() {
        command_args.append(additional_args);
    }
    (command, command_args)
}

// As these actions are bound to the default config, please
// do take care when refactoring - or renaming.
// They might need to be adjusted in the default config
// as well `../../assets/config/default.yaml`
/// Actions that can be bound to keys.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum Action {
    /// Quit Zellij.
    Quit,
    /// Write to the terminal.
    Write(Vec<u8>),
    /// Write Characters to the terminal.
    WriteChars(String),
    /// Switch to the specified input mode.
    SwitchToMode(InputMode),
    /// Switch all connected clients to the specified input mode.
    SwitchModeForAllClients(InputMode),
    /// Resize focus pane in specified direction.
    Resize(ResizeDirection),
    /// Switch focus to next pane in specified direction.
    FocusNextPane,
    FocusPreviousPane,
    /// Move the focus pane in specified direction.
    SwitchFocus,
    MoveFocus(Direction),
    /// Tries to move the focus pane in specified direction.
    /// If there is no pane in the direction, move to previous/next Tab.
    MoveFocusOrTab(Direction),
    MovePane(Option<Direction>),
    /// Dumps the screen to a file
    DumpScreen(String),
    /// Scroll up in focus pane.
    EditScrollback,
    ScrollUp,
    /// Scroll up at point
    ScrollUpAt(Position),
    /// Scroll down in focus pane.
    ScrollDown,
    /// Scroll down at point
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
    NewPane(Option<Direction>),
    /// Open the file in a new pane using the default editor
    EditFile(PathBuf, Option<usize>, Option<Direction>, Option<bool>), // usize is an optional line number, bool is floating true/false
    /// Open a new floating pane
    NewFloatingPane(Option<RunCommandAction>),
    /// Open a new tiled (embedded, non-floating) pane
    NewTiledPane(Option<Direction>, Option<RunCommandAction>),
    /// Embed focused pane in tab if floating or float focused pane if embedded
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all floating panes (if any) in the current Tab
    ToggleFloatingPanes,
    /// Close the focus pane.
    CloseFocus,
    PaneNameInput(Vec<u8>),
    UndoRenamePane,
    /// Create a new tab, optionally with a specified tab layout.
    NewTab(Option<PaneLayout>, Option<String>), // the String is the tab name
    /// Do nothing.
    NoOp,
    /// Go to the next tab.
    GoToNextTab,
    /// Go to the previous tab.
    GoToPreviousTab,
    /// Close the current tab.
    CloseTab,
    GoToTab(u32),
    ToggleTab,
    TabNameInput(Vec<u8>),
    UndoRenameTab,
    /// Run specified command in new pane.
    Run(RunCommandAction),
    /// Detach session and exit
    Detach,
    LeftClick(Position),
    RightClick(Position),
    MiddleClick(Position),
    LeftMouseRelease(Position),
    RightMouseRelease(Position),
    MiddleMouseRelease(Position),
    MouseHoldLeft(Position),
    MouseHoldRight(Position),
    MouseHoldMiddle(Position),
    Copy,
    /// Confirm a prompt
    Confirm,
    /// Deny a prompt
    Deny,
    /// Confirm an action that invokes a prompt automatically
    SkipConfirm(Box<Action>),
    /// Search for String
    SearchInput(Vec<u8>),
    /// Search for something
    Search(SearchDirection),
    /// Toggle case sensitivity of search
    SearchToggleOption(SearchOption),
}

impl Action {
    pub fn actions_from_cli(cli_action: CliAction) -> Result<Vec<Action>, String> {
        match cli_action {
            CliAction::Quit => Ok(vec![Action::Quit]),
            CliAction::Write { bytes } => Ok(vec![Action::Write(bytes)]),
            CliAction::WriteChars { chars } => Ok(vec![Action::WriteChars(chars)]),
            CliAction::Resize { resize_direction } => Ok(vec![Action::Resize(resize_direction)]),
            CliAction::FocusNextPane => Ok(vec![Action::FocusNextPane]),
            CliAction::FocusPreviousPane => Ok(vec![Action::FocusPreviousPane]),
            CliAction::MoveFocus { direction } => Ok(vec![Action::MoveFocus(direction)]),
            CliAction::MoveFocusOrTab { direction } => Ok(vec![Action::MoveFocusOrTab(direction)]),
            CliAction::MovePane { direction } => Ok(vec![Action::MovePane(Some(direction))]),
            CliAction::DumpScreen { path } => Ok(vec![Action::DumpScreen(path.as_os_str().to_string_lossy().into())]),
            CliAction::EditScrollback => Ok(vec![Action::EditScrollback]),
            CliAction::ScrollUp => Ok(vec![Action::ScrollUp]),
            CliAction::ScrollDown => Ok(vec![Action::ScrollDown]),
            CliAction::ScrollToBottom => Ok(vec![Action::ScrollToBottom]),
            CliAction::PageScrollUp => Ok(vec![Action::PageScrollUp]),
            CliAction::PageScrollDown => Ok(vec![Action::PageScrollDown]),
            CliAction::HalfPageScrollUp => Ok(vec![Action::HalfPageScrollUp]),
            CliAction::HalfPageScrollDown => Ok(vec![Action::HalfPageScrollDown]),
            CliAction::ToggleFullscreen => Ok(vec![Action::ToggleFocusFullscreen]),
            CliAction::TogglePaneFrames => Ok(vec![Action::TogglePaneFrames]),
            CliAction::ToggleActiveSyncTab => Ok(vec![Action::ToggleActiveSyncTab]),
            CliAction::NewPane { direction, command, cwd, args, floating } => {
                match command {
                    Some(command) => {
                        let (command, args) = split_command_and_args(command, args);
                        let run_command_action = RunCommandAction { command, args, cwd, direction };
                        match floating {
                            Some(true) => Ok(vec![Action::NewFloatingPane(Some(run_command_action))]),
                            _ => Ok(vec![Action::NewTiledPane(direction, Some(run_command_action))]),
                        }
                    },
                    None => {
                        match floating {
                            Some(true) => Ok(vec![Action::NewFloatingPane(None)]),
                            _ => Ok(vec![Action::NewTiledPane(direction, None)]),
                        }
                    }
                }
            }
            CliAction::Edit { direction, file, line_number, floating } => Ok(vec![Action::EditFile(file, line_number, direction, floating)]),
            CliAction::SwitchMode { input_mode } => Ok(vec![Action::SwitchModeForAllClients(input_mode)]),
            CliAction::TogglePaneEmbedOrFloating => Ok(vec![Action::TogglePaneEmbedOrFloating]),
            CliAction::ToggleFloatingPanes => Ok(vec![Action::ToggleFloatingPanes]),
            CliAction::ClosePane => Ok(vec![Action::CloseFocus]),
            CliAction::RenamePane { name }=> Ok(vec![Action::PaneNameInput(name.as_bytes().to_vec())]),
            CliAction::UndoRenamePane => Ok(vec![Action::UndoRenamePane]),
            CliAction::GoToNextTab => Ok(vec![Action::GoToNextTab]),
            CliAction::GoToPreviousTab => Ok(vec![Action::GoToPreviousTab]),
            CliAction::CloseTab => Ok(vec![Action::CloseTab]),
            CliAction::GoToTab { index } => Ok(vec![Action::GoToTab(index)]),
            CliAction::RenameTab { name } => Ok(vec![
                Action::TabNameInput(vec![0]),
                Action::TabNameInput(name.as_bytes().to_vec())
            ]),
            CliAction::UndoRenameTab => Ok(vec![Action::UndoRenameTab]),
            CliAction::SearchInput { input } => Ok(vec![Action::SearchInput(input.as_bytes().to_vec())]),
            CliAction::SearchNext => Ok(vec![Action::Search(SearchDirection::Down)]),
            CliAction::SearchPrevious => Ok(vec![Action::Search(SearchDirection::Up)]),
            CliAction::NewTab { name, layout } => {
                if let Some(layout_path) = layout {
                    let raw_layout = Layout::stringified_from_path_or_default(Some(&layout_path), None).map_err(|e| format!("Failed to load layout: {}", e))?;
                    let layout = Layout::from_str(&raw_layout).map_err(|e| format!("Failed to parse layout: {}", e))?;
                    let mut tabs = layout.tabs();
                    if tabs.len() > 1 {
                        return Err(format!("Tab layout cannot itself have tabs"));
                    } else if !tabs.is_empty() {
                        let (tab_name, layout) = tabs.drain(..).next().unwrap();
                        let name = tab_name.or(name);
                        Ok(vec![Action::NewTab(Some(layout), name)])
                    } else {
                        let layout = layout.new_tab();
                        Ok(vec![Action::NewTab(Some(layout), name)])
                    }
                } else {
                    Ok(vec![Action::NewTab(None, name)])
                }
            }
        }
    }
}

impl From<OnForceClose> for Action {
    fn from(ofc: OnForceClose) -> Action {
        match ofc {
            OnForceClose::Quit => Action::Quit,
            OnForceClose::Detach => Action::Detach,
        }
    }
}
