//! Definition of the actions that can be bound to keys.

pub use super::command::{OpenFilePayload, RunCommandAction};
use super::layout::{
    FloatingPaneLayout, Layout, PluginAlias, RunPlugin, RunPluginLocation, RunPluginOrAlias,
    SwapFloatingLayout, SwapTiledLayout, TabLayoutInfo, TiledPaneLayout,
};
use crate::cli::CliAction;
use crate::data::{
    CommandOrPlugin, Direction, KeyWithModifier, LayoutInfo, NewPanePlacement, OriginatingPlugin,
    PaneId, Resize, UnblockCondition,
};
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
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Deserialize,
    Serialize,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::EnumIter,
)]
#[strum(ascii_case_insensitive)]
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
    /// Write to a specific pane by ID.
    WriteToPaneId {
        bytes: Vec<u8>,
        pane_id: PaneId,
    },
    /// Write Characters to a specific pane by ID.
    WriteCharsToPaneId {
        chars: String,
        pane_id: PaneId,
    },
    /// Paste text using bracketed paste mode, optionally to a specific pane.
    Paste {
        chars: String,
        pane_id: Option<PaneId>,
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
    /// Dumps the screen to a file or STDOUT
    DumpScreen {
        file_path: Option<String>,
        include_scrollback: bool,
        pane_id: Option<PaneId>,
        ansi: bool,
    },
    /// Dumps
    DumpLayout,
    /// Save the current session state to disk
    SaveSession,
    EditScrollback,
    EditScrollbackRaw,
    /// Scroll up in focus pane.
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
    /// Returns: Created pane ID (format: terminal_<id>)
    NewBlockingPane {
        placement: NewPanePlacement,
        pane_name: Option<String>,
        command: Option<RunCommandAction>,
        unblock_condition: Option<UnblockCondition>,
        near_current_pane: bool,
    },
    /// Open the file in a new pane using the default editor
    /// Returns: Created pane ID (format: terminal_<id>)
    EditFile {
        payload: OpenFilePayload,
        direction: Option<Direction>,
        floating: bool,
        in_place: bool,
        close_replaced_pane: bool,
        start_suppressed: bool,
        coordinates: Option<FloatingPaneCoordinates>,
        near_current_pane: bool,
    },
    /// Open a new floating pane
    /// Returns: Created pane ID (format: terminal_<id> or plugin_<id>)
    NewFloatingPane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
        coordinates: Option<FloatingPaneCoordinates>,
        near_current_pane: bool,
    },
    /// Open a new tiled (embedded, non-floating) pane
    /// Returns: Created pane ID (format: terminal_<id> or plugin_<id>)
    NewTiledPane {
        direction: Option<Direction>,
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
        near_current_pane: bool,
        borderless: Option<bool>,
    },
    /// Open a new pane in place of the focused one, suppressing it instead
    /// Returns: Created pane ID (format: terminal_<id> or plugin_<id>)
    NewInPlacePane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
        near_current_pane: bool,
        pane_id_to_replace: Option<PaneId>,
        close_replaced_pane: bool,
    },
    /// Returns: Created pane ID (format: terminal_<id> or plugin_<id>)
    NewStackedPane {
        command: Option<RunCommandAction>,
        pane_name: Option<String>,
        near_current_pane: bool,
    },
    /// Embed focused pane in tab if floating or float focused pane if embedded
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all floating panes (if any) in the current Tab
    ToggleFloatingPanes,
    /// Show all floating panes in the specified tab (or active tab if tab_id is None)
    ShowFloatingPanes {
        tab_id: Option<usize>,
    },
    /// Hide all floating panes in the specified tab (or active tab if tab_id is None)
    HideFloatingPanes {
        tab_id: Option<usize>,
    },
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
        initial_panes: Option<Vec<CommandOrPlugin>>,
        first_pane_unblock_condition: Option<UnblockCondition>,
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
        near_current_pane: bool,
    },
    /// Set pane default foreground/background color
    SetPaneColor {
        pane_id: PaneId,
        fg: Option<String>,
        bg: Option<String>,
    },
    /// Detach session and exit
    Detach,
    /// Switch to a different session
    SwitchSession {
        name: String,
        tab_position: Option<usize>,
        pane_id: Option<(u32, bool)>, // (id, is_plugin)
        layout: Option<LayoutInfo>,
        cwd: Option<PathBuf>,
    },
    /// Returns: Plugin pane ID (format: plugin_<id>) when creating or focusing plugin
    LaunchOrFocusPlugin {
        plugin: RunPluginOrAlias,
        should_float: bool,
        move_to_focused_tab: bool,
        should_open_in_place: bool,
        close_replaced_pane: bool,
        skip_cache: bool,
    },
    /// Returns: Plugin pane ID (format: plugin_<id>)
    LaunchPlugin {
        plugin: RunPluginOrAlias,
        should_float: bool,
        should_open_in_place: bool,
        close_replaced_pane: bool,
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
    /// Override the layout of the active tab
    OverrideLayout {
        tabs: Vec<TabLayoutInfo>,
        retain_existing_terminal_panes: bool,
        retain_existing_plugin_panes: bool,
        apply_only_to_active_tab: bool,
    },
    /// Query all tab names
    QueryTabNames,
    /// Open a new tiled (embedded, non-floating) plugin pane
    /// Returns: Created pane ID (format: plugin_<id>)
    NewTiledPluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
        cwd: Option<PathBuf>,
    },
    /// Returns: Created pane ID (format: plugin_<id>)
    NewFloatingPluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
        cwd: Option<PathBuf>,
        coordinates: Option<FloatingPaneCoordinates>,
    },
    /// Returns: Created pane ID (format: plugin_<id>)
    NewInPlacePluginPane {
        plugin: RunPluginOrAlias,
        pane_name: Option<String>,
        skip_cache: bool,
        close_replaced_pane: bool,
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
        should_be_in_place_if_hidden: bool,
    },
    FocusPluginPaneWithId {
        pane_id: u32,
        should_float_if_hidden: bool,
        should_be_in_place_if_hidden: bool,
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
    GoToTabById {
        id: u64,
    },
    CloseTabById {
        id: u64,
    },
    RenameTabById {
        id: u64,
        name: String,
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
    ListPanes {
        show_tab: bool,
        show_command: bool,
        show_state: bool,
        show_geometry: bool,
        show_all: bool,
        output_json: bool,
    },
    ListTabs {
        show_state: bool,
        show_dimensions: bool,
        show_panes: bool,
        show_layout: bool,
        show_all: bool,
        output_json: bool,
    },
    CurrentTabInfo {
        output_json: bool,
    },
    TogglePanePinned,
    StackPanes {
        pane_ids: Vec<PaneId>,
    },
    ChangeFloatingPaneCoordinates {
        pane_id: PaneId,
        coordinates: FloatingPaneCoordinates,
    },
    TogglePaneBorderless {
        pane_id: PaneId,
    },
    SetPaneBorderless {
        pane_id: PaneId,
        borderless: bool,
    },
    TogglePaneInGroup,
    ToggleGroupMarking,
    // Pane-targeting CLI-only variants
    ScrollUpByPaneId {
        pane_id: PaneId,
    },
    ScrollDownByPaneId {
        pane_id: PaneId,
    },
    ScrollToTopByPaneId {
        pane_id: PaneId,
    },
    ScrollToBottomByPaneId {
        pane_id: PaneId,
    },
    PageScrollUpByPaneId {
        pane_id: PaneId,
    },
    PageScrollDownByPaneId {
        pane_id: PaneId,
    },
    HalfPageScrollUpByPaneId {
        pane_id: PaneId,
    },
    HalfPageScrollDownByPaneId {
        pane_id: PaneId,
    },
    ResizeByPaneId {
        pane_id: PaneId,
        resize: Resize,
        direction: Option<Direction>,
    },
    MovePaneByPaneId {
        pane_id: PaneId,
        direction: Option<Direction>,
    },
    MovePaneBackwardsByPaneId {
        pane_id: PaneId,
    },
    ClearScreenByPaneId {
        pane_id: PaneId,
    },
    EditScrollbackByPaneId {
        pane_id: PaneId,
    },
    ToggleFocusFullscreenByPaneId {
        pane_id: PaneId,
    },
    TogglePaneEmbedOrFloatingByPaneId {
        pane_id: PaneId,
    },
    CloseFocusByPaneId {
        pane_id: PaneId,
    },
    RenamePaneByPaneId {
        pane_id: PaneId,
        name: Vec<u8>,
    },
    UndoRenamePaneByPaneId {
        pane_id: PaneId,
    },
    TogglePanePinnedByPaneId {
        pane_id: PaneId,
    },
    // Tab-targeting CLI-only variants
    UndoRenameTabByTabId {
        id: u64,
    },
    ToggleActiveSyncTabByTabId {
        id: u64,
    },
    ToggleFloatingPanesByTabId {
        id: u64,
    },
    PreviousSwapLayoutByTabId {
        id: u64,
    },
    NextSwapLayoutByTabId {
        id: u64,
    },
    MoveTabByTabId {
        id: u64,
        direction: Direction,
    },
}

impl Default for Action {
    fn default() -> Self {
        Action::NoOp
    }
}

impl Default for SearchDirection {
    fn default() -> Self {
        SearchDirection::Down
    }
}

impl Default for SearchOption {
    fn default() -> Self {
        SearchOption::CaseSensitivity
    }
}

impl Action {
    /// Checks that two Action are match except their mutable attributes.
    pub fn shallow_eq(&self, other_action: &Action) -> bool {
        match (self, other_action) {
            (Action::NewTab { .. }, Action::NewTab { .. }) => true,
            (Action::LaunchOrFocusPlugin { .. }, Action::LaunchOrFocusPlugin { .. }) => true,
            (Action::LaunchPlugin { .. }, Action::LaunchPlugin { .. }) => true,
            (Action::OverrideLayout { .. }, Action::OverrideLayout { .. }) => true,
            _ => self == other_action,
        }
    }

    pub fn actions_from_cli(
        cli_action: CliAction,
        get_current_dir: Box<dyn Fn() -> PathBuf>,
        config: Option<Config>,
    ) -> Result<Vec<Action>, String> {
        match cli_action {
            CliAction::Write { bytes, pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let parsed_pane_id = PaneId::from_str(&pane_id_str);
                    match parsed_pane_id {
                            Ok(parsed_pane_id) => {
                                Ok(vec![Action::WriteToPaneId {
                                    bytes,
                                    pane_id: parsed_pane_id,
                                }])
                            },
                            Err(_e) => {
                                Err(format!(
                                    "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                    pane_id_str
                                ))
                            }
                        }
                },
                None => Ok(vec![Action::Write {
                    key_with_modifier: None,
                    bytes,
                    is_kitty_keyboard_protocol: false,
                }]),
            },
            CliAction::WriteChars { chars, pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let parsed_pane_id = PaneId::from_str(&pane_id_str);
                    match parsed_pane_id {
                            Ok(parsed_pane_id) => {
                                Ok(vec![Action::WriteCharsToPaneId {
                                    chars,
                                    pane_id: parsed_pane_id,
                                }])
                            },
                            Err(_e) => {
                                Err(format!(
                                    "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                    pane_id_str
                                ))
                            }
                        }
                },
                None => Ok(vec![Action::WriteChars { chars }]),
            },
            CliAction::Paste { chars, pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let parsed_pane_id = PaneId::from_str(&pane_id_str);
                    match parsed_pane_id {
                        Ok(parsed_pane_id) => {
                            Ok(vec![Action::Paste {
                                chars,
                                pane_id: Some(parsed_pane_id),
                            }])
                        },
                        Err(_e) => {
                            Err(format!(
                                "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                pane_id_str
                            ))
                        }
                    }
                },
                None => Ok(vec![Action::Paste {
                    chars,
                    pane_id: None,
                }]),
            },
            CliAction::SendKeys { keys, pane_id } => {
                let mut actions = Vec::new();

                for (index, key_str) in keys.iter().enumerate() {
                    let key = KeyWithModifier::from_str(key_str).map_err(|e| {
                        let suggestion = suggest_key_fix(key_str);
                        format!(
                            "Invalid key at position {}: \"{}\"\n  Error: {}\n{}",
                            index + 1,
                            key_str,
                            e,
                            suggestion
                        )
                    })?;

                    #[cfg(not(target_family = "wasm"))]
                    let bytes = key
                        .serialize_kitty()
                        .map(|s| s.into_bytes())
                        .unwrap_or_else(Vec::new);

                    #[cfg(target_family = "wasm")]
                    let bytes = vec![];

                    match &pane_id {
                        Some(pane_id_str) => {
                            let parsed_pane_id = PaneId::from_str(pane_id_str)
                                .map_err(|_| format!(
                                    "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                    pane_id_str
                                ))?;
                            actions.push(Action::WriteToPaneId {
                                bytes,
                                pane_id: parsed_pane_id,
                            });
                        },
                        None => {
                            actions.push(Action::Write {
                                key_with_modifier: Some(key),
                                bytes,
                                is_kitty_keyboard_protocol: true,
                            });
                        },
                    }
                }

                Ok(actions)
            },
            CliAction::Resize {
                resize,
                direction,
                pane_id,
            } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ResizeByPaneId {
                        pane_id,
                        resize,
                        direction,
                    }])
                },
                None => Ok(vec![Action::Resize { resize, direction }]),
            },
            CliAction::FocusNextPane => Ok(vec![Action::FocusNextPane]),
            CliAction::FocusPreviousPane => Ok(vec![Action::FocusPreviousPane]),
            CliAction::MoveFocus { direction } => Ok(vec![Action::MoveFocus { direction }]),
            CliAction::MoveFocusOrTab { direction } => {
                Ok(vec![Action::MoveFocusOrTab { direction }])
            },
            CliAction::MovePane { direction, pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::MovePaneByPaneId { pane_id, direction }])
                },
                None => Ok(vec![Action::MovePane { direction }]),
            },
            CliAction::MovePaneBackwards { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::MovePaneBackwardsByPaneId { pane_id }])
                },
                None => Ok(vec![Action::MovePaneBackwards]),
            },
            CliAction::MoveTab { direction, tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::MoveTabByTabId {
                    id: id as u64,
                    direction,
                }]),
                None => Ok(vec![Action::MoveTab { direction }]),
            },
            CliAction::Clear { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ClearScreenByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ClearScreen]),
            },
            CliAction::DumpScreen {
                path,
                full,
                pane_id,
                ansi,
            } => match pane_id {
                Some(pane_id_str) => {
                    let parsed_pane_id = PaneId::from_str(&pane_id_str);
                    match parsed_pane_id {
                        Ok(parsed_pane_id) => {
                            Ok(vec![Action::DumpScreen {
                                file_path: path.map(|p| p.as_os_str().to_string_lossy().into()),
                                include_scrollback: full,
                                pane_id: Some(parsed_pane_id),
                                ansi,
                            }])
                        },
                        Err(_e) => {
                            Err(format!(
                                "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                pane_id_str
                            ))
                        }
                    }
                },
                None => Ok(vec![Action::DumpScreen {
                    file_path: path.map(|p| p.as_os_str().to_string_lossy().into()),
                    include_scrollback: full,
                    pane_id: None,
                    ansi,
                }]),
            },
            CliAction::DumpLayout => Ok(vec![Action::DumpLayout]),
            CliAction::SaveSession => Ok(vec![Action::SaveSession]),
            CliAction::EditScrollback { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::EditScrollbackByPaneId { pane_id }])
                },
                None => Ok(vec![Action::EditScrollback]),
            },
            CliAction::ScrollUp { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ScrollUpByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ScrollUp]),
            },
            CliAction::ScrollDown { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ScrollDownByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ScrollDown]),
            },
            CliAction::ScrollToBottom { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ScrollToBottomByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ScrollToBottom]),
            },
            CliAction::ScrollToTop { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ScrollToTopByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ScrollToTop]),
            },
            CliAction::PageScrollUp { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::PageScrollUpByPaneId { pane_id }])
                },
                None => Ok(vec![Action::PageScrollUp]),
            },
            CliAction::PageScrollDown { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::PageScrollDownByPaneId { pane_id }])
                },
                None => Ok(vec![Action::PageScrollDown]),
            },
            CliAction::HalfPageScrollUp { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::HalfPageScrollUpByPaneId { pane_id }])
                },
                None => Ok(vec![Action::HalfPageScrollUp]),
            },
            CliAction::HalfPageScrollDown { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::HalfPageScrollDownByPaneId { pane_id }])
                },
                None => Ok(vec![Action::HalfPageScrollDown]),
            },
            CliAction::ToggleFullscreen { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::ToggleFocusFullscreenByPaneId { pane_id }])
                },
                None => Ok(vec![Action::ToggleFocusFullscreen]),
            },
            CliAction::TogglePaneFrames => Ok(vec![Action::TogglePaneFrames]),
            CliAction::ToggleActiveSyncTab { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::ToggleActiveSyncTabByTabId { id: id as u64 }]),
                None => Ok(vec![Action::ToggleActiveSyncTab]),
            },
            CliAction::NewPane {
                direction,
                command,
                plugin,
                cwd,
                floating,
                in_place,
                close_replaced_pane,
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
                blocking,
                unblock_condition,
                near_current_pane,
                borderless,
            } => {
                let current_dir = get_current_dir();
                // cwd should only be specified in a plugin alias if it was explicitly given to us,
                // otherwise the current_dir might override a cwd defined in the alias itself
                let alias_cwd = cwd.clone().map(|cwd| current_dir.join(cwd));
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir.clone()));
                if blocking || unblock_condition.is_some() {
                    // For blocking panes, we don't support plugins
                    if plugin.is_some() {
                        return Err("Blocking panes do not support plugin variants".to_string());
                    }

                    let command = if !command.is_empty() {
                        let mut command = command.clone();
                        let (command, args) = (PathBuf::from(command.remove(0)), command);
                        let hold_on_start = start_suspended;
                        let hold_on_close = !close_on_exit;
                        Some(RunCommandAction {
                            command,
                            args,
                            cwd,
                            direction,
                            hold_on_close,
                            hold_on_start,
                            ..Default::default()
                        })
                    } else {
                        None
                    };

                    let placement = if floating {
                        NewPanePlacement::Floating(FloatingPaneCoordinates::new(
                            x, y, width, height, pinned, borderless,
                        ))
                    } else if in_place {
                        NewPanePlacement::InPlace {
                            pane_id_to_replace: None,
                            close_replaced_pane,
                            borderless,
                        }
                    } else if stacked {
                        NewPanePlacement::Stacked {
                            pane_id_to_stack_under: None,
                            borderless,
                        }
                    } else {
                        NewPanePlacement::Tiled {
                            direction,
                            borderless,
                        }
                    };

                    Ok(vec![Action::NewBlockingPane {
                        placement,
                        pane_name: name,
                        command,
                        unblock_condition,
                        near_current_pane,
                    }])
                } else if let Some(plugin) = plugin {
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
                            coordinates: FloatingPaneCoordinates::new(
                                x, y, width, height, pinned, borderless,
                            ),
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePluginPane {
                            plugin,
                            pane_name: name,
                            skip_cache: skip_plugin_cache,
                            close_replaced_pane,
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
                            coordinates: FloatingPaneCoordinates::new(
                                x, y, width, height, pinned, borderless,
                            ),
                            near_current_pane,
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePane {
                            command: Some(run_command_action),
                            pane_name: name,
                            near_current_pane,
                            pane_id_to_replace: None, // TODO: support this
                            close_replaced_pane,
                        }])
                    } else if stacked {
                        Ok(vec![Action::NewStackedPane {
                            command: Some(run_command_action),
                            pane_name: name,
                            near_current_pane,
                        }])
                    } else {
                        Ok(vec![Action::NewTiledPane {
                            direction,
                            command: Some(run_command_action),
                            pane_name: name,
                            near_current_pane,
                            borderless,
                        }])
                    }
                } else {
                    if floating {
                        Ok(vec![Action::NewFloatingPane {
                            command: None,
                            pane_name: name,
                            coordinates: FloatingPaneCoordinates::new(
                                x, y, width, height, pinned, borderless,
                            ),
                            near_current_pane,
                        }])
                    } else if in_place {
                        Ok(vec![Action::NewInPlacePane {
                            command: None,
                            pane_name: name,
                            near_current_pane,
                            pane_id_to_replace: None, // TODO: support this
                            close_replaced_pane,
                        }])
                    } else if stacked {
                        Ok(vec![Action::NewStackedPane {
                            command: None,
                            pane_name: name,
                            near_current_pane,
                        }])
                    } else {
                        Ok(vec![Action::NewTiledPane {
                            direction,
                            command: None,
                            pane_name: name,
                            near_current_pane,
                            borderless,
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
                close_replaced_pane,
                cwd,
                x,
                y,
                width,
                height,
                pinned,
                near_current_pane,
                borderless,
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
                    close_replaced_pane,
                    start_suppressed,
                    coordinates: FloatingPaneCoordinates::new(
                        x, y, width, height, pinned, borderless,
                    ),
                    near_current_pane,
                }])
            },
            CliAction::SwitchMode { input_mode } => Ok(vec![Action::SwitchToMode { input_mode }]),
            CliAction::TogglePaneEmbedOrFloating { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::TogglePaneEmbedOrFloatingByPaneId { pane_id }])
                },
                None => Ok(vec![Action::TogglePaneEmbedOrFloating]),
            },
            CliAction::ToggleFloatingPanes { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::ToggleFloatingPanesByTabId { id: id as u64 }]),
                None => Ok(vec![Action::ToggleFloatingPanes]),
            },
            CliAction::ShowFloatingPanes { tab_id } => {
                Ok(vec![Action::ShowFloatingPanes { tab_id }])
            },
            CliAction::HideFloatingPanes { tab_id } => {
                Ok(vec![Action::HideFloatingPanes { tab_id }])
            },
            CliAction::ClosePane { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::CloseFocusByPaneId { pane_id }])
                },
                None => Ok(vec![Action::CloseFocus]),
            },
            CliAction::RenamePane { name, pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::RenamePaneByPaneId {
                        pane_id,
                        name: name.as_bytes().to_vec(),
                    }])
                },
                None => Ok(vec![
                    Action::UndoRenamePane,
                    Action::PaneNameInput {
                        input: name.as_bytes().to_vec(),
                    },
                ]),
            },
            CliAction::UndoRenamePane { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::UndoRenamePaneByPaneId { pane_id }])
                },
                None => Ok(vec![Action::UndoRenamePane]),
            },
            CliAction::GoToNextTab => Ok(vec![Action::GoToNextTab]),
            CliAction::GoToPreviousTab => Ok(vec![Action::GoToPreviousTab]),
            CliAction::CloseTab { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::CloseTabById { id: id as u64 }]),
                None => Ok(vec![Action::CloseTab]),
            },
            CliAction::GoToTab { index } => Ok(vec![Action::GoToTab { index }]),
            CliAction::GoToTabName { name, create } => {
                Ok(vec![Action::GoToTabName { name, create }])
            },
            CliAction::RenameTab { name, tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::RenameTabById {
                    id: id as u64,
                    name,
                }]),
                None => Ok(vec![
                    Action::TabNameInput { input: vec![0] },
                    Action::TabNameInput {
                        input: name.as_bytes().to_vec(),
                    },
                ]),
            },
            CliAction::UndoRenameTab { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::UndoRenameTabByTabId { id: id as u64 }]),
                None => Ok(vec![Action::UndoRenameTab]),
            },
            CliAction::GoToTabById { id } => Ok(vec![Action::GoToTabById { id }]),
            CliAction::CloseTabById { id } => Ok(vec![Action::CloseTabById { id }]),
            CliAction::RenameTabById { id, name } => Ok(vec![Action::RenameTabById { id, name }]),
            CliAction::NewTab {
                name,
                layout,
                layout_dir,
                cwd,
                initial_command,
                initial_plugin,
                close_on_exit,
                start_suspended,
                block_until_exit_success,
                block_until_exit_failure,
                block_until_exit,
            } => {
                let current_dir = get_current_dir();
                let cwd = cwd
                    .map(|cwd| current_dir.join(cwd))
                    .or_else(|| Some(current_dir.clone()));

                // Map CLI flags to UnblockCondition
                let first_pane_unblock_condition = if block_until_exit_success {
                    Some(UnblockCondition::OnExitSuccess)
                } else if block_until_exit_failure {
                    Some(UnblockCondition::OnExitFailure)
                } else if block_until_exit {
                    Some(UnblockCondition::OnAnyExit)
                } else {
                    None
                };

                // Parse initial_panes from initial_command or initial_plugin
                let initial_panes = if let Some(plugin_url) = initial_plugin {
                    let plugin = match RunPluginLocation::parse(&plugin_url, cwd.clone()) {
                        Ok(location) => RunPluginOrAlias::RunPlugin(RunPlugin {
                            _allow_exec_host_cmd: false,
                            location,
                            configuration: Default::default(),
                            initial_cwd: cwd.clone(),
                        }),
                        Err(_) => {
                            let mut plugin_alias =
                                PluginAlias::new(&plugin_url, &None, cwd.clone());
                            plugin_alias.set_caller_cwd_if_not_set(Some(current_dir.clone()));
                            RunPluginOrAlias::Alias(plugin_alias)
                        },
                    };
                    Some(vec![CommandOrPlugin::Plugin(plugin)])
                } else if !initial_command.is_empty() {
                    let mut command: Vec<String> = initial_command.clone();
                    let (command, args) = (
                        PathBuf::from(command.remove(0)),
                        command.into_iter().collect(),
                    );
                    let hold_on_close = !close_on_exit;
                    let hold_on_start = start_suspended;
                    let run_command_action = RunCommandAction {
                        command,
                        args,
                        cwd: cwd.clone(),
                        direction: None,
                        hold_on_close,
                        hold_on_start,
                        ..Default::default()
                    };
                    Some(vec![CommandOrPlugin::Command(run_command_action)])
                } else {
                    None
                };
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
                                initial_panes: initial_panes.clone(),
                                first_pane_unblock_condition,
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
                            initial_panes,
                            first_pane_unblock_condition,
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
                        initial_panes,
                        first_pane_unblock_condition,
                    }])
                }
            },
            CliAction::PreviousSwapLayout { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::PreviousSwapLayoutByTabId { id: id as u64 }]),
                None => Ok(vec![Action::PreviousSwapLayout]),
            },
            CliAction::NextSwapLayout { tab_id } => match tab_id {
                Some(id) => Ok(vec![Action::NextSwapLayoutByTabId { id: id as u64 }]),
                None => Ok(vec![Action::NextSwapLayout]),
            },
            CliAction::OverrideLayout {
                layout,
                layout_dir,
                retain_existing_terminal_panes,
                retain_existing_plugin_panes,
                apply_only_to_active_tab,
            } => {
                // Determine layout_dir: CLI arg > config > default
                let layout_dir = layout_dir
                    .or_else(|| config.and_then(|c| c.options.layout_dir))
                    .or_else(|| get_layout_dir(find_default_config_dir()));

                // Load layout from URL or file path
                let (path_to_raw_layout, raw_layout, swap_layouts) = if let Some(layout_url) =
                    layout.to_str().and_then(|l| {
                        if l.starts_with("http://") || l.starts_with("https://") {
                            Some(l)
                        } else {
                            None
                        }
                    }) {
                    (
                        layout_url.to_owned(),
                        Layout::stringified_from_url(layout_url)
                            .map_err(|e| format!("Failed to load layout from URL: {}", e))?,
                        None,
                    )
                } else {
                    Layout::stringified_from_path_or_default(Some(&layout), layout_dir)
                        .map_err(|e| format!("Failed to load layout: {}", e))?
                };

                // Parse KDL layout
                let layout = Layout::from_str(
                    &raw_layout,
                    path_to_raw_layout,
                    swap_layouts.as_ref().map(|(f, p)| (f.as_str(), p.as_str())),
                    None, // cwd
                )
                .map_err(|e| {
                    let stringified_error = match e {
                        ConfigError::KdlError(kdl_error) => {
                            let error = kdl_error.add_src(
                                layout.as_path().as_os_str().to_string_lossy().to_string(),
                                String::from(raw_layout),
                            );
                            let report: Report = error.into();
                            format!("{:?}", report)
                        },
                        ConfigError::KdlDeserializationError(kdl_error) => {
                            let error_message = kdl_error.to_string();
                            format!("Failed to deserialize KDL layout: {}", error_message)
                        },
                        e => format!("{}", e),
                    };
                    stringified_error
                })?;

                // Convert all tabs to Vec<TabLayoutInfo>
                let tabs: Vec<TabLayoutInfo> = layout
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(index, (tab_name, tiled, floating))| TabLayoutInfo {
                        tab_index: index,
                        tab_name: tab_name.clone(),
                        tiled_layout: tiled.clone(),
                        floating_layouts: floating.clone(),
                        swap_tiled_layouts: Some(layout.swap_tiled_layouts.clone()),
                        swap_floating_layouts: Some(layout.swap_floating_layouts.clone()),
                    })
                    .collect();

                // If no tabs, create default tab
                let tabs = if tabs.is_empty() {
                    let (tiled, floating) = layout.new_tab();
                    vec![TabLayoutInfo {
                        tab_index: 0,
                        tab_name: None,
                        tiled_layout: tiled,
                        floating_layouts: floating,
                        swap_tiled_layouts: Some(layout.swap_tiled_layouts),
                        swap_floating_layouts: Some(layout.swap_floating_layouts),
                    }]
                } else {
                    tabs
                };

                Ok(vec![Action::OverrideLayout {
                    tabs,
                    retain_existing_terminal_panes,
                    retain_existing_plugin_panes,
                    apply_only_to_active_tab,
                }])
            },
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
                close_replaced_pane,
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
                    close_replaced_pane,
                    skip_cache: skip_plugin_cache,
                }])
            },
            CliAction::LaunchPlugin {
                url,
                floating,
                in_place,
                close_replaced_pane,
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
                    close_replaced_pane,
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
            CliAction::ListPanes {
                tab,
                command,
                state,
                geometry,
                all,
                json,
            } => Ok(vec![Action::ListPanes {
                show_tab: tab,
                show_command: command,
                show_state: state,
                show_geometry: geometry,
                show_all: all,
                output_json: json,
            }]),
            CliAction::ListTabs {
                state,
                dimensions,
                panes,
                layout,
                all,
                json,
            } => Ok(vec![Action::ListTabs {
                show_state: state,
                show_dimensions: dimensions,
                show_panes: panes,
                show_layout: layout,
                show_all: all,
                output_json: json,
            }]),
            CliAction::CurrentTabInfo { json } => {
                Ok(vec![Action::CurrentTabInfo { output_json: json }])
            },
            CliAction::TogglePanePinned { pane_id } => match pane_id {
                Some(pane_id_str) => {
                    let pane_id = PaneId::from_str(&pane_id_str)
                        .map_err(|_| format!(
                            "Malformed pane id: {pane_id_str}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)"
                        ))?;
                    Ok(vec![Action::TogglePanePinnedByPaneId { pane_id }])
                },
                None => Ok(vec![Action::TogglePanePinned]),
            },
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
                borderless,
            } => {
                let Some(coordinates) =
                    FloatingPaneCoordinates::new(x, y, width, height, pinned, borderless)
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
            CliAction::TogglePaneBorderless { pane_id } => {
                let parsed_pane_id = PaneId::from_str(&pane_id);
                match parsed_pane_id {
                    Ok(parsed_pane_id) => {
                        Ok(vec![Action::TogglePaneBorderless {
                            pane_id: parsed_pane_id,
                        }])
                    },
                    Err(_e) => {
                        Err(format!(
                            "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                            pane_id
                        ))
                    }
                }
            },
            CliAction::SetPaneBorderless {
                pane_id,
                borderless,
            } => {
                let parsed_pane_id = PaneId::from_str(&pane_id);
                match parsed_pane_id {
                    Ok(parsed_pane_id) => {
                        Ok(vec![Action::SetPaneBorderless {
                            pane_id: parsed_pane_id,
                            borderless,
                        }])
                    },
                    Err(_e) => {
                        Err(format!(
                            "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                            pane_id
                        ))
                    }
                }
            },
            CliAction::SetPaneColor {
                pane_id,
                fg,
                bg,
                reset,
            } => {
                let pane_id_str = match pane_id {
                    Some(id) => id,
                    None => std::env::var("ZELLIJ_PANE_ID").map_err(|_| {
                        "No --pane-id provided and ZELLIJ_PANE_ID is not set".to_string()
                    })?,
                };
                let parsed_pane_id = PaneId::from_str(&pane_id_str);
                match parsed_pane_id {
                    Ok(parsed_pane_id) => {
                        let (fg, bg) = if reset {
                            (None, None)
                        } else {
                            (fg, bg)
                        };
                        Ok(vec![Action::SetPaneColor {
                            pane_id: parsed_pane_id,
                            fg,
                            bg,
                        }])
                    },
                    Err(_e) => Err(format!(
                        "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                        pane_id_str
                    )),
                }
            },
            CliAction::Detach => Ok(vec![Action::Detach]),
            CliAction::SwitchSession {
                name,
                tab_position,
                pane_id,
                layout,
                layout_dir,
                cwd,
            } => {
                let pane_id = match pane_id {
                    Some(stringified_pane_id) => match PaneId::from_str(&stringified_pane_id) {
                        Ok(PaneId::Terminal(id)) => Some((id, false)),
                        Ok(PaneId::Plugin(id)) => Some((id, true)),
                        Err(_e) => {
                            return Err(format!(
                                "Malformed pane id: {}, expecting either a bare integer (eg. 1), a terminal pane id (eg. terminal_1) or a plugin pane id (eg. plugin_1)",
                                stringified_pane_id
                            ));
                        },
                    },
                    None => None,
                };

                let cwd = cwd.map(|cwd| {
                    let current_dir = get_current_dir();
                    current_dir.join(cwd)
                });

                let layout_dir = layout_dir.map(|layout_dir| {
                    let current_dir = get_current_dir();
                    current_dir.join(layout_dir)
                });

                let layout_info = if let Some(layout_path) = layout {
                    let layout_dir = layout_dir
                        .or_else(|| config.and_then(|c| c.options.layout_dir.clone()))
                        .or_else(|| get_layout_dir(find_default_config_dir()));
                    LayoutInfo::from_config(&layout_dir, &Some(layout_path))
                } else {
                    None
                };

                Ok(vec![Action::SwitchSession {
                    name: name.clone(),
                    tab_position: tab_position.clone(),
                    pane_id,
                    layout: layout_info,
                    cwd,
                }])
            },
        }
    }
    pub fn populate_originating_plugin(&mut self, originating_plugin: OriginatingPlugin) {
        match self {
            Action::NewBlockingPane { command, .. }
            | Action::NewFloatingPane { command, .. }
            | Action::NewTiledPane { command, .. }
            | Action::NewInPlacePane { command, .. }
            | Action::NewStackedPane { command, .. } => {
                command
                    .as_mut()
                    .map(|c| c.populate_originating_plugin(originating_plugin));
            },
            Action::Run { command, .. } => {
                command.populate_originating_plugin(originating_plugin);
            },
            Action::EditFile { payload, .. } => {
                payload.originating_plugin = Some(originating_plugin);
            },
            Action::NewTab { initial_panes, .. } => {
                if let Some(initial_panes) = initial_panes.as_mut() {
                    for pane in initial_panes.iter_mut() {
                        match pane {
                            CommandOrPlugin::Command(run_command) => {
                                run_command.populate_originating_plugin(originating_plugin.clone());
                            },
                            _ => {},
                        }
                    }
                }
            },
            _ => {},
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

fn suggest_key_fix(key_str: &str) -> String {
    if key_str.contains('-') {
        return "  Hint: Use spaces instead of hyphens (e.g., \"Ctrl a\" not \"Ctrl-a\")"
            .to_string();
    }

    if key_str.trim().is_empty() {
        return "  Hint: Key string cannot be empty".to_string();
    }

    let parts: Vec<&str> = key_str.split_whitespace().collect();
    if parts.len() > 1 {
        for part in &parts[..parts.len() - 1] {
            let lower = part.to_ascii_lowercase();
            if lower.starts_with("ctr") && lower != "ctrl" {
                return format!("  Hint: Did you mean \"Ctrl\" instead of \"{}\"?", part);
            }
            if !matches!(lower.as_str(), "ctrl" | "alt" | "shift" | "super") {
                return "  Hint: Valid modifiers are: Ctrl, Alt, Shift, Super".to_string();
            }
        }
    }

    "  Hint: Use format like \"Ctrl a\", \"Alt Shift F1\", or \"Enter\"".to_string()
}

impl From<OnForceClose> for Action {
    fn from(ofc: OnForceClose) -> Action {
        match ofc {
            OnForceClose::Quit => Action::Quit,
            OnForceClose::Detach => Action::Detach,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::BareKey;
    use crate::data::KeyModifier;
    use std::path::PathBuf;

    #[test]
    fn test_send_keys_single_key() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["Enter".to_string()],
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Write {
                key_with_modifier,
                bytes,
                is_kitty_keyboard_protocol,
            } => {
                assert!(key_with_modifier.is_some());
                let key = key_with_modifier.as_ref().unwrap();
                assert_eq!(key.bare_key, BareKey::Enter);
                assert!(key.key_modifiers.is_empty());
                assert!(!bytes.is_empty());
                assert_eq!(*is_kitty_keyboard_protocol, true);
            },
            _ => panic!("Expected Write action"),
        }
    }

    #[test]
    fn test_send_keys_with_modifier() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["Ctrl a".to_string()],
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Write {
                key_with_modifier,
                is_kitty_keyboard_protocol,
                ..
            } => {
                assert!(key_with_modifier.is_some());
                let key = key_with_modifier.as_ref().unwrap();
                assert_eq!(key.bare_key, BareKey::Char('a'));
                assert!(key.key_modifiers.contains(&KeyModifier::Ctrl));
                assert_eq!(*is_kitty_keyboard_protocol, true);
            },
            _ => panic!("Expected Write action"),
        }
    }

    #[test]
    fn test_send_keys_multiple_keys() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["Ctrl a".to_string(), "F1".to_string(), "Enter".to_string()],
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 3);
        for action in &actions {
            match action {
                Action::Write {
                    is_kitty_keyboard_protocol,
                    ..
                } => {
                    assert_eq!(*is_kitty_keyboard_protocol, true);
                },
                _ => panic!("Expected Write action"),
            }
        }
    }

    #[test]
    fn test_send_keys_error_hyphen_syntax() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["Ctrl-a".to_string()],
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Use spaces instead of hyphens"));
    }

    #[test]
    fn test_send_keys_error_typo() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["Ctrll a".to_string()],
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Ctrl") || err.contains("modifier"));
    }

    #[test]
    fn test_send_keys_with_pane_id() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["a".to_string()],
            pane_id: Some("terminal_1".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::WriteToPaneId { pane_id, bytes } => {
                assert!(matches!(pane_id, PaneId::Terminal(1)));
                assert!(!bytes.is_empty());
            },
            _ => panic!("Expected WriteToPaneId action"),
        }
    }

    #[test]
    fn test_send_keys_error_invalid_pane_id() {
        let cli_action = CliAction::SendKeys {
            keys: vec!["a".to_string()],
            pane_id: Some("invalid_id".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Malformed pane id"));
    }

    // =============================================
    // Category 1: Pane-targeting tests
    // =============================================

    // 1. ScrollUp
    #[test]
    fn test_scroll_up_with_pane_id() {
        let cli_action = CliAction::ScrollUp {
            pane_id: Some("terminal_5".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollUpByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(5)));
            },
            _ => panic!("Expected ScrollUpByPaneId action"),
        }
    }

    #[test]
    fn test_scroll_up_without_pane_id() {
        let cli_action = CliAction::ScrollUp { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ScrollUp));
    }

    // 2. ScrollDown
    #[test]
    fn test_scroll_down_with_pane_id() {
        let cli_action = CliAction::ScrollDown {
            pane_id: Some("terminal_2".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollDownByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(2)));
            },
            _ => panic!("Expected ScrollDownByPaneId action"),
        }
    }

    #[test]
    fn test_scroll_down_without_pane_id() {
        let cli_action = CliAction::ScrollDown { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ScrollDown));
    }

    // 3. ScrollToTop
    #[test]
    fn test_scroll_to_top_with_pane_id() {
        let cli_action = CliAction::ScrollToTop {
            pane_id: Some("terminal_1".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollToTopByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(1)));
            },
            _ => panic!("Expected ScrollToTopByPaneId action"),
        }
    }

    #[test]
    fn test_scroll_to_top_without_pane_id() {
        let cli_action = CliAction::ScrollToTop { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ScrollToTop));
    }

    // 4. ScrollToBottom
    #[test]
    fn test_scroll_to_bottom_with_pane_id() {
        let cli_action = CliAction::ScrollToBottom {
            pane_id: Some("terminal_4".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollToBottomByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(4)));
            },
            _ => panic!("Expected ScrollToBottomByPaneId action"),
        }
    }

    #[test]
    fn test_scroll_to_bottom_without_pane_id() {
        let cli_action = CliAction::ScrollToBottom { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ScrollToBottom));
    }

    // 5. PageScrollUp
    #[test]
    fn test_page_scroll_up_with_pane_id() {
        let cli_action = CliAction::PageScrollUp {
            pane_id: Some("terminal_6".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::PageScrollUpByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(6)));
            },
            _ => panic!("Expected PageScrollUpByPaneId action"),
        }
    }

    #[test]
    fn test_page_scroll_up_without_pane_id() {
        let cli_action = CliAction::PageScrollUp { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::PageScrollUp));
    }

    // 6. PageScrollDown
    #[test]
    fn test_page_scroll_down_with_pane_id() {
        let cli_action = CliAction::PageScrollDown {
            pane_id: Some("terminal_8".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::PageScrollDownByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(8)));
            },
            _ => panic!("Expected PageScrollDownByPaneId action"),
        }
    }

    #[test]
    fn test_page_scroll_down_without_pane_id() {
        let cli_action = CliAction::PageScrollDown { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::PageScrollDown));
    }

    // 7. HalfPageScrollUp
    #[test]
    fn test_half_page_scroll_up_with_pane_id() {
        let cli_action = CliAction::HalfPageScrollUp {
            pane_id: Some("terminal_10".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::HalfPageScrollUpByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(10)));
            },
            _ => panic!("Expected HalfPageScrollUpByPaneId action"),
        }
    }

    #[test]
    fn test_half_page_scroll_up_without_pane_id() {
        let cli_action = CliAction::HalfPageScrollUp { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::HalfPageScrollUp));
    }

    // 8. HalfPageScrollDown
    #[test]
    fn test_half_page_scroll_down_with_pane_id() {
        let cli_action = CliAction::HalfPageScrollDown {
            pane_id: Some("terminal_12".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::HalfPageScrollDownByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(12)));
            },
            _ => panic!("Expected HalfPageScrollDownByPaneId action"),
        }
    }

    #[test]
    fn test_half_page_scroll_down_without_pane_id() {
        let cli_action = CliAction::HalfPageScrollDown { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::HalfPageScrollDown));
    }

    // 9. Resize
    #[test]
    fn test_resize_with_pane_id() {
        let cli_action = CliAction::Resize {
            resize: Resize::Increase,
            direction: Some(Direction::Left),
            pane_id: Some("terminal_3".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ResizeByPaneId {
                pane_id,
                resize,
                direction,
            } => {
                assert!(matches!(pane_id, PaneId::Terminal(3)));
                assert!(matches!(resize, Resize::Increase));
                assert!(matches!(direction, Some(Direction::Left)));
            },
            _ => panic!("Expected ResizeByPaneId action"),
        }
    }

    #[test]
    fn test_resize_without_pane_id() {
        let cli_action = CliAction::Resize {
            resize: Resize::Increase,
            direction: Some(Direction::Left),
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Resize { resize, direction } => {
                assert!(matches!(resize, Resize::Increase));
                assert!(matches!(direction, Some(Direction::Left)));
            },
            _ => panic!("Expected Resize action"),
        }
    }

    // 10. MovePane
    #[test]
    fn test_move_pane_with_pane_id() {
        let cli_action = CliAction::MovePane {
            direction: Some(Direction::Right),
            pane_id: Some("terminal_9".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::MovePaneByPaneId { pane_id, direction } => {
                assert!(matches!(pane_id, PaneId::Terminal(9)));
                assert!(matches!(direction, Some(Direction::Right)));
            },
            _ => panic!("Expected MovePaneByPaneId action"),
        }
    }

    #[test]
    fn test_move_pane_without_pane_id() {
        let cli_action = CliAction::MovePane {
            direction: Some(Direction::Right),
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::MovePane { direction } => {
                assert!(matches!(direction, Some(Direction::Right)));
            },
            _ => panic!("Expected MovePane action"),
        }
    }

    // 11. MovePaneBackwards
    #[test]
    fn test_move_pane_backwards_with_pane_id() {
        let cli_action = CliAction::MovePaneBackwards {
            pane_id: Some("terminal_11".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::MovePaneBackwardsByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(11)));
            },
            _ => panic!("Expected MovePaneBackwardsByPaneId action"),
        }
    }

    #[test]
    fn test_move_pane_backwards_without_pane_id() {
        let cli_action = CliAction::MovePaneBackwards { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::MovePaneBackwards));
    }

    // 12. Clear
    #[test]
    fn test_clear_with_pane_id() {
        let cli_action = CliAction::Clear {
            pane_id: Some("terminal_14".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ClearScreenByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(14)));
            },
            _ => panic!("Expected ClearScreenByPaneId action"),
        }
    }

    #[test]
    fn test_clear_without_pane_id() {
        let cli_action = CliAction::Clear { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ClearScreen));
    }

    // 13. EditScrollback
    #[test]
    fn test_edit_scrollback_with_pane_id() {
        let cli_action = CliAction::EditScrollback {
            pane_id: Some("terminal_15".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::EditScrollbackByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(15)));
            },
            _ => panic!("Expected EditScrollbackByPaneId action"),
        }
    }

    #[test]
    fn test_edit_scrollback_without_pane_id() {
        let cli_action = CliAction::EditScrollback { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::EditScrollback));
    }

    // 14. ToggleFullscreen
    #[test]
    fn test_toggle_fullscreen_with_pane_id() {
        let cli_action = CliAction::ToggleFullscreen {
            pane_id: Some("terminal_16".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ToggleFocusFullscreenByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(16)));
            },
            _ => panic!("Expected ToggleFocusFullscreenByPaneId action"),
        }
    }

    #[test]
    fn test_toggle_fullscreen_without_pane_id() {
        let cli_action = CliAction::ToggleFullscreen { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ToggleFocusFullscreen));
    }

    // 15. TogglePaneEmbedOrFloating
    #[test]
    fn test_toggle_pane_embed_or_floating_with_pane_id() {
        let cli_action = CliAction::TogglePaneEmbedOrFloating {
            pane_id: Some("terminal_17".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::TogglePaneEmbedOrFloatingByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(17)));
            },
            _ => panic!("Expected TogglePaneEmbedOrFloatingByPaneId action"),
        }
    }

    #[test]
    fn test_toggle_pane_embed_or_floating_without_pane_id() {
        let cli_action = CliAction::TogglePaneEmbedOrFloating { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::TogglePaneEmbedOrFloating));
    }

    // 16. ClosePane
    #[test]
    fn test_close_pane_with_pane_id() {
        let cli_action = CliAction::ClosePane {
            pane_id: Some("terminal_18".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::CloseFocusByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(18)));
            },
            _ => panic!("Expected CloseFocusByPaneId action"),
        }
    }

    #[test]
    fn test_close_pane_without_pane_id() {
        let cli_action = CliAction::ClosePane { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::CloseFocus));
    }

    // 17. RenamePane
    #[test]
    fn test_rename_pane_with_pane_id() {
        let cli_action = CliAction::RenamePane {
            name: "my-pane".to_string(),
            pane_id: Some("terminal_19".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::RenamePaneByPaneId { pane_id, name } => {
                assert!(matches!(pane_id, PaneId::Terminal(19)));
                assert_eq!(name, &"my-pane".as_bytes().to_vec());
            },
            _ => panic!("Expected RenamePaneByPaneId action"),
        }
    }

    #[test]
    fn test_rename_pane_without_pane_id() {
        let cli_action = CliAction::RenamePane {
            name: "my-pane".to_string(),
            pane_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], Action::UndoRenamePane));
        assert!(matches!(actions[1], Action::PaneNameInput { .. }));
    }

    // 18. UndoRenamePane
    #[test]
    fn test_undo_rename_pane_with_pane_id() {
        let cli_action = CliAction::UndoRenamePane {
            pane_id: Some("terminal_20".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::UndoRenamePaneByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(20)));
            },
            _ => panic!("Expected UndoRenamePaneByPaneId action"),
        }
    }

    #[test]
    fn test_undo_rename_pane_without_pane_id() {
        let cli_action = CliAction::UndoRenamePane { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::UndoRenamePane));
    }

    // 19. TogglePanePinned
    #[test]
    fn test_toggle_pane_pinned_with_pane_id() {
        let cli_action = CliAction::TogglePanePinned {
            pane_id: Some("terminal_21".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::TogglePanePinnedByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(21)));
            },
            _ => panic!("Expected TogglePanePinnedByPaneId action"),
        }
    }

    #[test]
    fn test_toggle_pane_pinned_without_pane_id() {
        let cli_action = CliAction::TogglePanePinned { pane_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::TogglePanePinned));
    }

    // Extra pane tests
    #[test]
    fn test_scroll_up_with_plugin_pane_id() {
        let cli_action = CliAction::ScrollUp {
            pane_id: Some("plugin_3".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollUpByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Plugin(3)));
            },
            _ => panic!("Expected ScrollUpByPaneId action with plugin pane id"),
        }
    }

    #[test]
    fn test_scroll_up_with_bare_integer_pane_id() {
        let cli_action = CliAction::ScrollUp {
            pane_id: Some("7".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ScrollUpByPaneId { pane_id } => {
                assert!(matches!(pane_id, PaneId::Terminal(7)));
            },
            _ => panic!("Expected ScrollUpByPaneId action with bare integer pane id"),
        }
    }

    #[test]
    fn test_scroll_up_with_invalid_pane_id() {
        let cli_action = CliAction::ScrollUp {
            pane_id: Some("invalid_id".to_string()),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Malformed pane id"));
    }

    // =============================================
    // Category 1: Tab-targeting tests
    // =============================================

    // 20. CloseTab
    #[test]
    fn test_close_tab_with_tab_id() {
        let cli_action = CliAction::CloseTab { tab_id: Some(5) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::CloseTabById { id } => {
                assert_eq!(*id, 5u64);
            },
            _ => panic!("Expected CloseTabById action"),
        }
    }

    #[test]
    fn test_close_tab_without_tab_id() {
        let cli_action = CliAction::CloseTab { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::CloseTab));
    }

    // 21. RenameTab
    #[test]
    fn test_rename_tab_with_tab_id() {
        let cli_action = CliAction::RenameTab {
            name: "my-tab".to_string(),
            tab_id: Some(3),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::RenameTabById { id, name } => {
                assert_eq!(*id, 3u64);
                assert_eq!(name, "my-tab");
            },
            _ => panic!("Expected RenameTabById action"),
        }
    }

    #[test]
    fn test_rename_tab_without_tab_id() {
        let cli_action = CliAction::RenameTab {
            name: "my-tab".to_string(),
            tab_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(actions[0], Action::TabNameInput { .. }));
        assert!(matches!(actions[1], Action::TabNameInput { .. }));
    }

    // 22. UndoRenameTab
    #[test]
    fn test_undo_rename_tab_with_tab_id() {
        let cli_action = CliAction::UndoRenameTab { tab_id: Some(7) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::UndoRenameTabByTabId { id } => {
                assert_eq!(*id, 7u64);
            },
            _ => panic!("Expected UndoRenameTabByTabId action"),
        }
    }

    #[test]
    fn test_undo_rename_tab_without_tab_id() {
        let cli_action = CliAction::UndoRenameTab { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::UndoRenameTab));
    }

    // 23. ToggleActiveSyncTab
    #[test]
    fn test_toggle_active_sync_tab_with_tab_id() {
        let cli_action = CliAction::ToggleActiveSyncTab { tab_id: Some(2) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ToggleActiveSyncTabByTabId { id } => {
                assert_eq!(*id, 2u64);
            },
            _ => panic!("Expected ToggleActiveSyncTabByTabId action"),
        }
    }

    #[test]
    fn test_toggle_active_sync_tab_without_tab_id() {
        let cli_action = CliAction::ToggleActiveSyncTab { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ToggleActiveSyncTab));
    }

    // 24. ToggleFloatingPanes
    #[test]
    fn test_toggle_floating_panes_with_tab_id() {
        let cli_action = CliAction::ToggleFloatingPanes { tab_id: Some(4) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::ToggleFloatingPanesByTabId { id } => {
                assert_eq!(*id, 4u64);
            },
            _ => panic!("Expected ToggleFloatingPanesByTabId action"),
        }
    }

    #[test]
    fn test_toggle_floating_panes_without_tab_id() {
        let cli_action = CliAction::ToggleFloatingPanes { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::ToggleFloatingPanes));
    }

    // 25. PreviousSwapLayout
    #[test]
    fn test_previous_swap_layout_with_tab_id() {
        let cli_action = CliAction::PreviousSwapLayout { tab_id: Some(6) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::PreviousSwapLayoutByTabId { id } => {
                assert_eq!(*id, 6u64);
            },
            _ => panic!("Expected PreviousSwapLayoutByTabId action"),
        }
    }

    #[test]
    fn test_previous_swap_layout_without_tab_id() {
        let cli_action = CliAction::PreviousSwapLayout { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::PreviousSwapLayout));
    }

    // 26. NextSwapLayout
    #[test]
    fn test_next_swap_layout_with_tab_id() {
        let cli_action = CliAction::NextSwapLayout { tab_id: Some(8) };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::NextSwapLayoutByTabId { id } => {
                assert_eq!(*id, 8u64);
            },
            _ => panic!("Expected NextSwapLayoutByTabId action"),
        }
    }

    #[test]
    fn test_next_swap_layout_without_tab_id() {
        let cli_action = CliAction::NextSwapLayout { tab_id: None };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::NextSwapLayout));
    }

    // 27. MoveTab
    #[test]
    fn test_move_tab_with_tab_id() {
        let cli_action = CliAction::MoveTab {
            direction: Direction::Right,
            tab_id: Some(10),
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::MoveTabByTabId { id, direction } => {
                assert_eq!(*id, 10u64);
                assert!(matches!(direction, Direction::Right));
            },
            _ => panic!("Expected MoveTabByTabId action"),
        }
    }

    #[test]
    fn test_move_tab_without_tab_id() {
        let cli_action = CliAction::MoveTab {
            direction: Direction::Right,
            tab_id: None,
        };
        let result = Action::actions_from_cli(cli_action, Box::new(|| PathBuf::from("/tmp")), None);
        assert!(result.is_ok());
        let actions = result.unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::MoveTab { direction } => {
                assert!(matches!(direction, Direction::Right));
            },
            _ => panic!("Expected MoveTab action"),
        }
    }
}
