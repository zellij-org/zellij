//! Definition of the actions that can be bound to keys.

use super::command::{
    FloatingPaneOptions, OpenFile, PaneOptions, RunCommand, TabOptions, TiledPaneOptions,
};
use super::layout::{Layout, PaneLayout};
use crate::cli::CliAction;
use crate::data::InputMode;
use crate::envs::EnvironmentVariables;
use crate::input::config::{ConfigError, KdlError};
use crate::input::options::OnForceClose;
use miette::{NamedSource, Report};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

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
            _ => Err(format!(
                "Failed to parse Direction. Unknown Direction: {}",
                s
            )),
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
            _ => Err(format!(
                "Failed to parse ResizeDirection. Unknown ResizeDirection: {}",
                s
            )),
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
            _ => Err(format!(
                "Failed to parse SearchDirection. Unknown SearchDirection: {}",
                s
            )),
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
            "CaseSensitivity" | "casesensitivity" | "Casesensitivity" => {
                Ok(SearchOption::CaseSensitivity)
            },
            "WholeWord" | "wholeword" | "Wholeword" => Ok(SearchOption::WholeWord),
            "Wrap" | "wrap" => Ok(SearchOption::Wrap),
            _ => Err(format!(
                "Failed to parse SearchOption. Unknown SearchOption: {}",
                s
            )),
        }
    }
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
    DumpScreen(String, bool),
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
    NewPane(RunCommand, PaneOptions), // String is an optional pane name
    /// Open the file in a new pane using the default editor
    EditFile(OpenFile, PaneOptions), // usize is an optional line number, bool is floating true/false
    /// Open a new floating pane
    NewFloatingPane(RunCommand, FloatingPaneOptions), // String is an optional pane name
    /// Open a new tiled (embedded, non-floating) pane
    NewTiledPane(RunCommand, TiledPaneOptions), // String is an
    // optional pane
    // name
    /// Embed focused pane in tab if floating or float focused pane if embedded
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all floating panes (if any) in the current Tab
    ToggleFloatingPanes,
    /// Close the focus pane.
    CloseFocus,
    PaneNameInput(Vec<u8>),
    UndoRenamePane,
    /// Create a new tab, optionally with a specified tab layout.
    NewTab(Option<PaneLayout>, RunCommand, TabOptions), // the String is the tab name
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
    Run(RunCommand, TiledPaneOptions),
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
    ToggleMouseMode,
}

impl Action {
    pub fn actions_from_cli(
        cli_action: CliAction,
        get_current_dir: Box<dyn Fn() -> PathBuf>,
    ) -> Result<Vec<Action>, String> {
        match cli_action {
            CliAction::Write { bytes } => Ok(vec![Action::Write(bytes)]),
            CliAction::WriteChars { chars } => Ok(vec![Action::WriteChars(chars)]),
            CliAction::Resize { resize_direction } => Ok(vec![Action::Resize(resize_direction)]),
            CliAction::FocusNextPane => Ok(vec![Action::FocusNextPane]),
            CliAction::FocusPreviousPane => Ok(vec![Action::FocusPreviousPane]),
            CliAction::MoveFocus { direction } => Ok(vec![Action::MoveFocus(direction)]),
            CliAction::MoveFocusOrTab { direction } => Ok(vec![Action::MoveFocusOrTab(direction)]),
            CliAction::MovePane { direction } => Ok(vec![Action::MovePane(Some(direction))]),
            CliAction::DumpScreen { path, full } => Ok(vec![Action::DumpScreen(
                path.as_os_str().to_string_lossy().into(),
                full,
            )]),
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
            CliAction::NewPane {
                direction,
                command,
                cwd,
                floating,
                name,
                close_on_exit,
                start_suspended,
                env,
            } => {
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                let hold_on_start = start_suspended;
                let hold_on_close = !close_on_exit;
                let env_vars = env.map_or(EnvironmentVariables::new(), |e| {
                    EnvironmentVariables::from_data(HashMap::from_iter(e))
                });
                let mut command = command.clone();
                let (command, args) = if !command.is_empty() {
                    let (command, args) = (PathBuf::from(command.remove(0)), command);
                    (Some(command), args)
                } else {
                    (None, Vec::new())
                };
                let run_command_action = RunCommand {
                    command,
                    args,
                    cwd,
                    hold_on_close,
                    hold_on_start,
                    env: env_vars,
                };
                if floating {
                    Ok(vec![Action::NewFloatingPane(
                        run_command_action,
                        FloatingPaneOptions { title: name },
                    )])
                } else {
                    Ok(vec![Action::NewTiledPane(
                        run_command_action,
                        TiledPaneOptions {
                            title: name,
                            direction,
                        },
                    )])
                }
            },
            CliAction::Edit {
                direction,
                file,
                line_number,
                floating,
                cwd,
                env,
            } => {
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                let env_vars = env.map_or(EnvironmentVariables::new(), |e| {
                    EnvironmentVariables::from_data(HashMap::from_iter(e))
                });
                Ok(vec![Action::EditFile(
                    OpenFile {
                        file_name: file.clone(),
                        line_number,
                        cwd,
                        env: env_vars,
                    },
                    PaneOptions {
                        title: Some(format!("Editing: {}", file.display())),
                        direction,
                        floating,
                    },
                )])
            },
            CliAction::SwitchMode { input_mode } => {
                Ok(vec![Action::SwitchModeForAllClients(input_mode)])
            },
            CliAction::TogglePaneEmbedOrFloating => Ok(vec![Action::TogglePaneEmbedOrFloating]),
            CliAction::ToggleFloatingPanes => Ok(vec![Action::ToggleFloatingPanes]),
            CliAction::ClosePane => Ok(vec![Action::CloseFocus]),
            CliAction::RenamePane { name } => Ok(vec![
                Action::UndoRenamePane,
                Action::PaneNameInput(name.as_bytes().to_vec()),
            ]),
            CliAction::UndoRenamePane => Ok(vec![Action::UndoRenamePane]),
            CliAction::GoToNextTab => Ok(vec![Action::GoToNextTab]),
            CliAction::GoToPreviousTab => Ok(vec![Action::GoToPreviousTab]),
            CliAction::CloseTab => Ok(vec![Action::CloseTab]),
            CliAction::GoToTab { index } => Ok(vec![Action::GoToTab(index)]),
            CliAction::RenameTab { name } => Ok(vec![
                Action::TabNameInput(vec![0]),
                Action::TabNameInput(name.as_bytes().to_vec()),
            ]),
            CliAction::UndoRenameTab => Ok(vec![Action::UndoRenameTab]),
            CliAction::NewTab {
                name,
                command,
                layout,
                cwd,
                env,
            } => {
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                let env_vars = env.map_or(EnvironmentVariables::new(), |e| {
                    EnvironmentVariables::from_data(HashMap::from_iter(e))
                });
                let mut command = command.clone();
                let (command, args) = if !command.is_empty() {
                    let (command, args) = (PathBuf::from(command.remove(0)), command);
                    (Some(command), args)
                } else {
                    (None, Vec::new())
                };
                let run_command_action = RunCommand {
                    command,
                    args,
                    cwd: cwd.clone(),
                    env: env_vars.clone(),
                    ..Default::default()
                };

                if let Some(layout_path) = layout {
                    let (path_to_raw_layout, raw_layout) =
                        Layout::stringified_from_path_or_default(Some(&layout_path), None)
                            .map_err(|e| format!("Failed to load layout: {}", e))?;
                    let layout = Layout::from_str(&raw_layout, path_to_raw_layout, cwd, env_vars).map_err(|e| {
                        let stringified_error = match e {
                            ConfigError::KdlError(kdl_error) => {
                                let error = kdl_error.add_src(layout_path.as_path().as_os_str().to_string_lossy().to_string(), String::from(raw_layout));
                                let report: Report = error.into();
                                format!("{:?}", report)
                            }
                            ConfigError::KdlDeserializationError(kdl_error) => {
                                let error_message = match kdl_error.kind {
                                    kdl::KdlErrorKind::Context("valid node terminator") => {
                                        format!("Failed to deserialize KDL node. \nPossible reasons:\n{}\n{}\n{}\n{}",
                                        "- Missing `;` after a node name, eg. { node; another_node; }",
                                        "- Missing quotations (\") around an argument node eg. { first_node \"argument_node\"; }",
                                        "- Missing an equal sign (=) between node arguments on a title line. eg. argument=\"value\"",
                                        "- Found an extraneous equal sign (=) between node child arguments and their values. eg. { argument=\"value\" }")
                                    },
                                    _ => String::from(kdl_error.help.unwrap_or("Kdl Deserialization Error")),
                                };
                                let kdl_error = KdlError {
                                    error_message,
                                    src: Some(NamedSource::new(layout_path.as_path().as_os_str().to_string_lossy().to_string(), String::from(raw_layout))),
                                    offset: Some(kdl_error.span.offset()),
                                    len: Some(kdl_error.span.len()),
                                    help_message: None,
                                };
                                let report: Report = kdl_error.into();
                                format!("{:?}", report)
                            },
                            e => format!("{}", e)
                        };
                        stringified_error
                    })?;
                    let mut tabs = layout.tabs();
                    if tabs.len() > 1 {
                        return Err(format!("Tab layout cannot itself have tabs"));
                    } else if !tabs.is_empty() {
                        let (tab_name, layout) = tabs.drain(..).next().unwrap();
                        let name = tab_name.or(name);
                        Ok(vec![Action::NewTab(
                            Some(layout),
                            run_command_action,
                            TabOptions { title: name },
                        )])
                    } else {
                        let layout = layout.new_tab();
                        Ok(vec![Action::NewTab(
                            Some(layout),
                            run_command_action,
                            TabOptions { title: name },
                        )])
                    }
                } else {
                    Ok(vec![Action::NewTab(
                        None,
                        run_command_action,
                        TabOptions { title: name },
                    )])
                }
            },
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
