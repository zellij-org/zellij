//! Definition of the actions that can be bound to keys.

use super::command::{OpenFilePayload, RunCommandAction};
use super::layout::{
    FloatingPaneLayout, Layout, PluginAlias, RunPlugin, RunPluginLocation, RunPluginOrAlias,
    SwapFloatingLayout, SwapTiledLayout, TiledPaneLayout,
};
use crate::cli::CliAction;
use crate::data::{Direction, KeyWithModifier, PaneId, Resize};
use crate::data::{FloatingPaneCoordinates, InputMode};
use crate::home::{find_default_config_dir, get_layout_dir};
use crate::input::config::{Config, ConfigError, KdlError};
use crate::input::mouse::MouseEvent;
use crate::input::options::OnForceClose;
use miette::{NamedSource, Report};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use std::path::PathBuf;
use std::str::FromStr;

use crate::position::Position;

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
    Write {
        key_with_modifier: Option<KeyWithModifier>,
        bytes: Vec<u8>,
        is_kitty_keyboard_protocol: bool,
    },
    /// Write Characters to the terminal.
    WriteChars {
        chars: String,
    },
    /// Switch to the specified input mode.
    SwitchToMode {
        input_mode: InputMode,
    },
    /// Switch all connected clients to the specified input mode.
    SwitchModeForAllClients {
        input_mode: InputMode,
    },
    /// Shrink/enlarge focused pane at specified border
    Resize {
        resize: Resize,
        direction: Option<Direction>,
    },
    /// Switch focus to next pane in specified direction.
    FocusNextPane,
    FocusPreviousPane,
    /// Move the focus pane in specified direction.
    SwitchFocus,
    MoveFocus {
        direction: Direction,
    },
    /// Tries to move the focus pane in specified direction.
    /// If there is no pane in the direction, move to previous/next Tab.
    MoveFocusOrTab {
        direction: Direction,
    },
    MovePane {
        direction: Option<Direction>,
    },
    MovePaneBackwards,
    /// Clear all buffers of a current screen
    ClearScreen,
    /// Dumps the screen to a file
    DumpScreen {
        file_path: String,
        include_scrollback: bool,
    },
    /// Dumps
    DumpLayout,
    /// Scroll up in focus pane.
    EditScrollback,
    ScrollUp,
    /// Scroll up at point
    ScrollUpAt {
        position: Position,
    },
    /// Scroll down in focus pane.
    ScrollDown,
    /// Scroll down at point
    ScrollDownAt {
        position: Position,
    },
    /// Scroll down to bottom in focus pane.
    ScrollToBottom,
    /// Scroll up to top in focus pane.
    ScrollToTop,
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
    NewPane {
        direction: Option<Direction>,
        pane_name: Option<String>,
        start_suppressed: bool,
    },
    /// Open the file in a new pane using the default editor
    EditFile {
        payload: OpenFilePayload,
        direction: Option<Direction>,
        floating: bool,
        in_place: bool,
        start_suppressed: bool,
        coordinates: Option<FloatingPaneCoordinates>,
    },
    /// Open a new floating pane
    NewFloatingPane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
        coordinates: Option<FloatingPaneCoordinates>,
    },
    /// Open a new tiled (embedded, non-floating) pane
    NewTiledPane {
        direction: Option<Direction>,
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
    },
    /// Open a new pane in place of the focused one, suppressing it instead
    NewInPlacePane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
    },
    NewStackedPane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
    },
    /// Embed focused pane in tab if floating or float focused pane if embedded
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all floating panes (if any) in the current Tab
    ToggleFloatingPanes,
    /// Close the focus pane.
    CloseFocus,
    PaneNameInput {
        input: Vec<u8>,
    },
    UndoRenamePane,
    /// Create a new tab, optionally with a specified tab layout.
    NewTab {
        tiled_layout: Option<TiledPaneLayout>,
        floating_layouts: Vec<FloatingPaneLayout>,
        swap_tiled_layouts: Option<Vec<SwapTiledLayout>>,
        swap_floating_layouts: Option<Vec<SwapFloatingLayout>>,
        tab_name: Option<String>,
        should_change_focus_to_new_tab: bool,
        cwd: Option<PathBuf>,
    },
    /// Do nothing.
    NoOp,
    /// Go to the next tab.
    GoToNextTab,
    /// Go to the previous tab.
    GoToPreviousTab,
    /// Close the current tab.
    CloseTab,
    GoToTab {
        index: u32,
    },
    GoToTabName {
        name: String,
        create: bool,
    },
    ToggleTab,
    TabNameInput {
        input: Vec<u8>,
    },
    UndoRenameTab,
    MoveTab {
        direction: Direction,
    },
    /// Run specified command in new pane.
    Run {
        command: RunCommandAction,
    },
    /// Detach session and exit
    Detach,
    LaunchOrFocusPlugin {
        plugin: RunPluginOrAlias,
        should_float: bool,
        move_to_focused_tab: bool,
        should_open_in_place: bool,
        skip_cache: bool,
    },
    LaunchPlugin {
        plugin: RunPluginOrAlias,
        should_float: bool,
        should_open_in_place: bool,
        skip_cache: bool,
        cwd: Option<PathBuf>,
    },
    MouseEvent {
        event: MouseEvent,
    },
    Copy,
    /// Confirm a prompt
    Confirm,
    /// Deny a prompt
    Deny,
    /// Confirm an action that invokes a prompt automatically
    SkipConfirm {
        action: Box<Action>,
    },
    /// Search for String
    SearchInput {
        input: Vec<u8>,
    },
    /// Search for something
    Search {
        direction: SearchDirection,
    },
    /// Toggle case sensitivity of search
    SearchToggleOption {
        option: SearchOption,
    },
    ToggleMouseMode,
    PreviousSwapLayout,
    NextSwapLayout,
    /// Query all tab names
    QueryTabNames,
    /// Open a new tiled (embedded, non-floating) plugin pane
    NewTiledPluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
        cwd: Option<PathBuf>,
    },
    NewFloatingPluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
        cwd: Option<PathBuf>,
        coordinates: Option<FloatingPaneCoordinates>,
    },
    NewInPlacePluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
    },
    StartOrReloadPlugin {
        plugin: RunPluginOrAlias,
    },
    CloseTerminalPane {
        pane_id: u32,
    },
    ClosePluginPane {
        pane_id: u32,
    },
    FocusTerminalPaneWithId {
        pane_id: u32,
        should_float_if_hidden: bool,
    },
    FocusPluginPaneWithId {
        pane_id: u32,
        should_float_if_hidden: bool,
    },
    RenameTerminalPane {
        pane_id: u32,
        name: Vec<u8>,
    },
    RenamePluginPane {
        pane_id: u32,
        name: Vec<u8>,
    },
    RenameTab {
        tab_index: u32,
        name: Vec<u8>,
    },
    BreakPane,
    BreakPaneRight,
    BreakPaneLeft,
    RenameSession {
        name: String,
    },
    CliPipe {
        pipe_id: String,
        name: Option<String>,
        payload: Option<String>,
        args: Option<BTreeMap<String, String>>,
        plugin: Option<String>,
        configuration: Option<BTreeMap<String, String>>,
        launch_new: bool,
        skip_cache: bool,
        floating: Option<bool>,
        in_place: Option<bool>,
        cwd: Option<PathBuf>,
        pane_title: Option<String>,
    },
    KeybindPipe {
        name: Option<String>,
        payload: Option<String>,
        args: Option<BTreeMap<String, String>>,
        plugin: Option<String>,
        plugin_id: Option<u32>, // supercedes plugin if present
        configuration: Option<BTreeMap<String, String>>,
        launch_new: bool,
        skip_cache: bool,
        floating: Option<bool>,
        in_place: Option<bool>,
        cwd: Option<PathBuf>,
        pane_title: Option<String>,
    },
    ListClients,
    TogglePanePinned,
    StackPanes {
        pane_ids: Vec<PaneId>,
    },
    ChangeFloatingPaneCoordinates {
        pane_id: PaneId,
        coordinates: FloatingPaneCoordinates,
    },
    TogglePaneInGroup,
    ToggleGroupMarking,
}

impl Action {
    /// Checks that two Action are match except their mutable attributes.
    pub fn shallow_eq(&self, other_action: &Action) -> bool {
        match (self, other_action) {
            (Action::NewTab { .. }, Action::NewTab { .. }) => true,
            (Action::LaunchOrFocusPlugin { .. }, Action::LaunchOrFocusPlugin { .. }) => true,
            (Action::LaunchPlugin { .. }, Action::LaunchPlugin { .. }) => true,
            _ => self == other_action,
        }
    }

    pub fn actions_from_cli(
        cli_action: CliAction,
        get_current_dir: Box<dyn Fn() -> PathBuf>,
        config: Option<Config>,
    ) -> Result<Vec<Action>, String> {
        match cli_action {
            CliAction::Write { bytes } => Ok(vec![Action::Write {
                key_with_modifier: None,
                bytes,
                is_kitty_keyboard_protocol: false,
            }]),
            CliAction::WriteChars { chars } => Ok(vec![Action::WriteChars { chars }]),
            CliAction::Resize { resize, direction } => {
                Ok(vec![Action::Resize { resize, direction }])
            },
            CliAction::FocusNextPane => Ok(vec![Action::FocusNextPane]),
            CliAction::FocusPreviousPane => Ok(vec![Action::FocusPreviousPane]),
            CliAction::MoveFocus { direction } => Ok(vec![Action::MoveFocus { direction }]),
            CliAction::MoveFocusOrTab { direction } => {
                Ok(vec![Action::MoveFocusOrTab { direction }])
            },
            CliAction::MovePane { direction } => Ok(vec![Action::MovePane { direction }]),
            CliAction::MovePaneBackwards => Ok(vec![Action::MovePaneBackwards]),
            CliAction::MoveTab { direction } => Ok(vec![Action::MoveTab { direction }]),
            CliAction::Clear => Ok(vec![Action::ClearScreen]),
            CliAction::DumpScreen { path, full } => Ok(vec![Action::DumpScreen {
                file_path: path.as_os_str().to_string_lossy().into(),
                include_scrollback: full,
            }]),
            CliAction::DumpLayout => Ok(vec![Action::DumpLayout]),
            CliAction::EditScrollback => Ok(vec![Action::EditScrollback]),
            CliAction::ScrollUp => Ok(vec![Action::ScrollUp]),
            CliAction::ScrollDown => Ok(vec![Action::ScrollDown]),
            CliAction::ScrollToBottom => Ok(vec![Action::ScrollToBottom]),
            CliAction::ScrollToTop => Ok(vec![Action::ScrollToTop]),
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
                plugin,
                cwd,
                floating,
                in_place,
                name,
                close_on_exit,
                start_suspended,
                configuration,
                skip_plugin_cache,
                x,
                y,
                width,
                height,
                pinned,
                stacked,
            } => {
                let current_dir = get_current_dir();
                // cwd should only be specified in a plugin alias if it was explicitly given to us,
                // otherwise the current_dir might override a cwd defined in the alias itself
                let alias_cwd = cwd.clone().map(|cwd| current_dir.join(cwd));
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir.clone()));
                if let Some(plugin) = plugin {
                    let plugin = match RunPluginLocation::parse(&plugin, cwd.clone()) {
                        Ok(location) => {
                            let user_configuration = configuration.unwrap_or_default();
                            RunPluginOrAlias::RunPlugin(RunPlugin {
                                _allow_exec_host_cmd: false,
                                location,
                                configuration: user_configuration,
                                initial_cwd: cwd.clone(),
                            })
                        },
                        Err(_) => {
                            let mut plugin_alias = PluginAlias::new(
                                &plugin,
                                &configuration.map(|c| c.inner().clone()),
                                alias_cwd,
                            );
                            plugin_alias.set_caller_cwd_if_not_set(Some(current_dir));
                            RunPluginOrAlias::Alias(plugin_alias)
                        },
                    };
                    if floating {
                        Ok(vec![Action::NewFloatingPluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: skip_plugin_cache,
                            cwd,
                            coordinates: FloatingPaneCoordinates::new(x, y, width, height, pinned),
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: skip_plugin_cache,
                        }])
                    } else {
                        // it is intentional that a new tiled plugin pane cannot include a
                        // direction
                        // this is because the cli client opening a tiled plugin pane is a
                        // different client than the one opening the pane, and this can potentially
                        // create very confusing races if the client changes focus while the plugin
                        // is being loaded
                        // this is not the case with terminal panes for historical reasons of
                        // backwards compatibility to a time before we had auto layouts
                        Ok(vec![Action::NewTiledPluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: skip_plugin_cache,
                            cwd,
                        }])
                    }
                } else if !command.is_empty() {
                    let mut command = command.clone();
                    let (command, args) = (PathBuf::from(command.remove(0)), command);
                    let hold_on_start = start_suspended;
                    let hold_on_close = !close_on_exit;
                    let run_command_action = RunCommandAction {
                        command,
                        args,
                        cwd,
                        direction,
                        hold_on_close,
                        hold_on_start,
                        ..Default::default()
                    };
                    if floating {
                        Ok(vec![Action::NewFloatingPane {
                            command: Some(run_command_action),
                            pane_name: name,
                            coordinates: FloatingPaneCoordinates::new(x, y, width, height, pinned),
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePane {
                            command: Some(run_command_action),
                            pane_name: name,
                        }])
                    } else if stacked {
                        Ok(vec![Action::NewStackedPane {
                            command: Some(run_command_action),
                            pane_name: name,
                        }])
                    } else {
                        Ok(vec![Action::NewTiledPane {
                            direction,
                            command: Some(run_command_action),
                            pane_name: name,
                        }])
                    }
                } else {
                    if floating {
                        Ok(vec![Action::NewFloatingPane {
                            command: None,
                            pane_name: name,
                            coordinates: FloatingPaneCoordinates::new(x, y, width, height, pinned),
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePane {
                            command: None,
                            pane_name: name,
                        }])
                    } else if stacked {
                        Ok(vec![Action::NewStackedPane {
                            command: None,
                            pane_name: name,
                        }])
                    } else {
                        Ok(vec![Action::NewTiledPane {
                            direction,
                            command: None,
                            pane_name: name,
                        }])
                    }
                }
            },
            CliAction::Edit {
                direction,
                file,
                line_number,
                floating,
                in_place,
                cwd,
                x,
                y,
                width,
                height,
                pinned,
            } => {
                let mut file = file;
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                if file.is_relative() {
                    if let Some(cwd) = cwd.as_ref() {
                        file = cwd.join(file);
                    }
                }
                let start_suppressed = false;
                Ok(vec![Action::EditFile {
                    payload: OpenFilePayload::new(file, line_number, cwd),
                    direction,
                    floating,
                    in_place,
                    start_suppressed,
                    coordinates: FloatingPaneCoordinates::new(x, y, width, height, pinned),
                }])
            },
            CliAction::SwitchMode { input_mode } => {
                Ok(vec![Action::SwitchModeForAllClients { input_mode }])
            },
            CliAction::TogglePaneEmbedOrFloating => Ok(vec![Action::TogglePaneEmbedOrFloating]),
            CliAction::ToggleFloatingPanes => Ok(vec![Action::ToggleFloatingPanes]),
            CliAction::ClosePane => Ok(vec![Action::CloseFocus]),
            CliAction::RenamePane { name } => Ok(vec![
                Action::UndoRenamePane,
                Action::PaneNameInput {
                    input: name.as_bytes().to_vec(),
                },
            ]),
            CliAction::UndoRenamePane => Ok(vec![Action::UndoRenamePane]),
            CliAction::GoToNextTab => Ok(vec![Action::GoToNextTab]),
            CliAction::GoToPreviousTab => Ok(vec![Action::GoToPreviousTab]),
            CliAction::CloseTab => Ok(vec![Action::CloseTab]),
            CliAction::GoToTab { index } => Ok(vec![Action::GoToTab { index }]),
            CliAction::GoToTabName { name, create } => {
                Ok(vec![Action::GoToTabName { name, create }])
            },
            CliAction::RenameTab { name } => Ok(vec![
                Action::TabNameInput { input: vec![0] },
                Action::TabNameInput {
                    input: name.as_bytes().to_vec(),
                },
            ]),
            CliAction::UndoRenameTab => Ok(vec![Action::UndoRenameTab]),
            CliAction::NewTab {
                name,
                layout,
                layout_dir,
                cwd,
            } => {
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                if let Some(layout_path) = layout {
                    let layout_dir = layout_dir
                        .or_else(|| config.and_then(|c| c.options.layout_dir))
                        .or_else(|| get_layout_dir(find_default_config_dir()));

                    let mut should_start_layout_commands_suspended = false;
                    let (path_to_raw_layout, raw_layout, swap_layouts) = if let Some(layout_url) =
                        layout_path.to_str().and_then(|l| {
                            if l.starts_with("http://") || l.starts_with("https://") {
                                Some(l)
                            } else {
                                None
                            }
                        }) {
                        should_start_layout_commands_suspended = true;
                        (
                            layout_url.to_owned(),
                            Layout::stringified_from_url(layout_url)
                                .map_err(|e| format!("Failed to load layout: {}", e))?,
                            None,
                        )
                    } else {
                        Layout::stringified_from_path_or_default(Some(&layout_path), layout_dir)
                            .map_err(|e| format!("Failed to load layout: {}", e))?
                    };
                    let mut layout = Layout::from_str(&raw_layout, path_to_raw_layout, swap_layouts.as_ref().map(|(f, p)| (f.as_str(), p.as_str())), cwd).map_err(|e| {
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
                    if should_start_layout_commands_suspended {
                        layout.recursively_add_start_suspended_including_template(Some(true));
                    }
                    let mut tabs = layout.tabs();
                    if !tabs.is_empty() {
                        let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
                        let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
                        let mut new_tab_actions = vec![];
                        let mut has_focused_tab = tabs
                            .iter()
                            .any(|(_, layout, _)| layout.focus.unwrap_or(false));
                        for (tab_name, layout, floating_panes_layout) in tabs.drain(..) {
                            let name = tab_name.or_else(|| name.clone());
                            let should_change_focus_to_new_tab =
                                layout.focus.unwrap_or_else(|| {
                                    if !has_focused_tab {
                                        has_focused_tab = true;
                                        true
                                    } else {
                                        false
                                    }
                                });
                            new_tab_actions.push(Action::NewTab {
                                tiled_layout: Some(layout),
                                floating_layouts: floating_panes_layout,
                                swap_tiled_layouts: swap_tiled_layouts.clone(),
                                swap_floating_layouts: swap_floating_layouts.clone(),
                                tab_name: name,
                                should_change_focus_to_new_tab,
                                cwd: None, // the cwd is done through the layout
                            });
                        }
                        Ok(new_tab_actions)
                    } else {
                        let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
                        let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());
                        let (layout, floating_panes_layout) = layout.new_tab();
                        let should_change_focus_to_new_tab = true;
                        Ok(vec![Action::NewTab {
                            tiled_layout: Some(layout),
                            floating_layouts: floating_panes_layout,
                            swap_tiled_layouts,
                            swap_floating_layouts,
                            tab_name: name,
                            should_change_focus_to_new_tab,
                            cwd: None, // the cwd is done through the layout
                        }])
                    }
                } else {
                    let should_change_focus_to_new_tab = true;
                    Ok(vec![Action::NewTab {
                        tiled_layout: None,
                        floating_layouts: vec![],
                        swap_tiled_layouts: None,
                        swap_floating_layouts: None,
                        tab_name: name,
                        should_change_focus_to_new_tab,
                        cwd,
                    }])
                }
            },
            CliAction::PreviousSwapLayout => Ok(vec![Action::PreviousSwapLayout]),
            CliAction::NextSwapLayout => Ok(vec![Action::NextSwapLayout]),
            CliAction::QueryTabNames => Ok(vec![Action::QueryTabNames]),
            CliAction::StartOrReloadPlugin { url, configuration } => {
                let current_dir = get_current_dir();
                let run_plugin_or_alias = RunPluginOrAlias::from_url(
                    &url,
                    &configuration.map(|c| c.inner().clone()),
                    None,
                    Some(current_dir),
                )?;
                Ok(vec![Action::StartOrReloadPlugin {
                    plugin: run_plugin_or_alias,
                }])
            },
            CliAction::LaunchOrFocusPlugin {
                url,
                floating,
                in_place,
                move_to_focused_tab,
                configuration,
                skip_plugin_cache,
            } => {
                let current_dir = get_current_dir();
                let run_plugin_or_alias = RunPluginOrAlias::from_url(
                    url.as_str(),
                    &configuration.map(|c| c.inner().clone()),
                    None,
                    Some(current_dir),
                )?;
                Ok(vec![Action::LaunchOrFocusPlugin {
                    plugin: run_plugin_or_alias,
                    should_float: floating,
                    move_to_focused_tab,
                    should_open_in_place: in_place,
                    skip_cache: skip_plugin_cache,
                }])
            },
            CliAction::LaunchPlugin {
                url,
                floating,
                in_place,
                configuration,
                skip_plugin_cache,
            } => {
                let current_dir = get_current_dir();
                let run_plugin_or_alias = RunPluginOrAlias::from_url(
                    &url.as_str(),
                    &configuration.map(|c| c.inner().clone()),
                    None,
                    Some(current_dir.clone()),
                )?;
                Ok(vec![Action::LaunchPlugin {
                    plugin: run_plugin_or_alias,
                    should_float: floating,
                    should_open_in_place: in_place,
                    skip_cache: skip_plugin_cache,
                    cwd: Some(current_dir),
                }])
            },
            CliAction::RenameSession { name } => Ok(vec![Action::RenameSession { name }]),
            CliAction::Pipe {
                name,
                payload,
                args,
                plugin,
                plugin_configuration,
                force_launch_plugin,
                skip_plugin_cache,
                floating_plugin,
                in_place_plugin,
                plugin_cwd,
                plugin_title,
            } => {
                let current_dir = get_current_dir();
                let cwd = plugin_cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir));
                let skip_cache = skip_plugin_cache;
                let pipe_id = Uuid::new_v4().to_string();
                Ok(vec![Action::CliPipe {
                    pipe_id,
                    name,
                    payload,
                    args: args.map(|a| a.inner().clone()), // TODO: no clone somehow
                    plugin,
                    configuration: plugin_configuration.map(|a| a.inner().clone()), // TODO: no clone
                    // somehow
                    launch_new: force_launch_plugin,
                    floating: floating_plugin,
                    in_place: in_place_plugin,
                    cwd,
                    pane_title: plugin_title,
                    skip_cache,
                }])
            },
            CliAction::ListClients => Ok(vec![Action::ListClients]),
            CliAction::TogglePanePinned => Ok(vec![Action::TogglePanePinned]),
            CliAction::StackPanes { pane_ids } => {
                let mut malformed_ids = vec![];
                let pane_ids = pane_ids
                    .iter()
                    .filter_map(
                        |stringified_pane_id| match PaneId::from_str(stringified_pane_id) {
                            Ok(pane_id) => Some(pane_id),
                            Err(_e) => {
                                malformed_ids.push(stringified_pane_id.to_owned());
                                None
                            },
                        },
                    )
                    .collect();
                if !malformed_ids.is_empty() {
                    Err(
                        format!(
                            "Malformed pane ids: {}, expecting a space separated list of either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                            malformed_ids.join(", ")
                        )
                    )
                } else {
                    Ok(vec![Action::StackPanes { pane_ids }])
                }
            },
            CliAction::ChangeFloatingPaneCoordinates {
                pane_id,
                x,
                y,
                width,
                height,
                pinned,
            } => {
                let Some(coordinates) = FloatingPaneCoordinates::new(x, y, width, height, pinned)
                else {
                    return Err(format!("Failed to parse floating pane coordinates"));
                };
                let parsed_pane_id = PaneId::from_str(&pane_id);
                match parsed_pane_id {
                    Ok(parsed_pane_id) => {
                        Ok(vec![Action::ChangeFloatingPaneCoordinates {
                            pane_id: parsed_pane_id,
                            coordinates,
                        }])
                    },
                    Err(_e) => {
                        Err(format!(
                            "Malformed pane id: {}, expecting a space separated list of either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                            pane_id
                        ))
                    }
                }
            },
        }
    }
    pub fn launches_plugin(&self, plugin_url: &str) -> bool {
        match self {
            Action::LaunchPlugin { plugin, .. } => &plugin.location_string() == plugin_url,
            Action::LaunchOrFocusPlugin { plugin, .. } => &plugin.location_string() == plugin_url,
            _ => false,
        }
    }
    pub fn is_mouse_action(&self) -> bool {
        if let Action::MouseEvent { .. } = self {
            return true;
        }
        false
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
