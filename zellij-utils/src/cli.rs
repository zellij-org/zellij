use crate::data::{Direction, InputMode, Resize};
use crate::setup::Setup;
use crate::{
    consts::{ZELLIJ_CONFIG_DIR_ENV, ZELLIJ_CONFIG_FILE_ENV},
    input::options::CliOptions,
};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use url::Url;

#[derive(Parser, Default, Debug, Clone, Serialize, Deserialize)]
#[clap(version, name = "zellij")]
pub struct CliArgs {
    /// Maximum panes on screen, caution: opening more panes will close old ones
    #[clap(long, value_parser)]
    pub max_panes: Option<usize>,

    /// Change where zellij looks for plugins
    #[clap(long, value_parser, overrides_with = "data_dir")]
    pub data_dir: Option<PathBuf>,

    /// Run server listening at the specified socket path
    #[clap(long, value_parser, hide = true, overrides_with = "server")]
    pub server: Option<PathBuf>,

    /// Specify name of a new session
    #[clap(long, short, overrides_with = "session", value_parser)]
    pub session: Option<String>,

    /// Name of a predefined layout inside the layout directory or the path to a layout file
    #[clap(short, long, value_parser, overrides_with = "layout")]
    pub layout: Option<PathBuf>,

    /// Change where zellij looks for the configuration file
    #[clap(short, long, overrides_with = "config", env = ZELLIJ_CONFIG_FILE_ENV, value_parser)]
    pub config: Option<PathBuf>,

    /// Change where zellij looks for the configuration directory
    #[clap(long, overrides_with = "config_dir", env = ZELLIJ_CONFIG_DIR_ENV, value_parser)]
    pub config_dir: Option<PathBuf>,

    #[clap(subcommand)]
    pub command: Option<Command>,

    /// Specify emitting additional debug information
    #[clap(short, long, value_parser)]
    pub debug: bool,
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum Command {
    /// Change the behaviour of zellij
    #[clap(name = "options", value_parser)]
    Options(CliOptions),

    /// Setup zellij and check its configuration
    #[clap(name = "setup", value_parser)]
    Setup(Setup),

    /// Explore existing zellij sessions
    #[clap(flatten)]
    Sessions(Sessions),
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum SessionCommand {
    /// Change the behaviour of zellij
    #[clap(name = "options")]
    Options(CliOptions),
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum Sessions {
    /// List active sessions
    #[clap(visible_alias = "ls")]
    ListSessions,

    /// Attach to a session
    #[clap(visible_alias = "a")]
    Attach {
        /// Name of the session to attach to.
        #[clap(value_parser)]
        session_name: Option<String>,

        /// Create a session if one does not exist.
        #[clap(short, long, value_parser)]
        create: bool,

        /// Number of the session index in the active sessions ordered creation date.
        #[clap(long, value_parser)]
        index: Option<usize>,

        /// Change the behaviour of zellij
        #[clap(subcommand, name = "options")]
        options: Option<Box<SessionCommand>>,
    },

    /// Kill the specific session
    #[clap(visible_alias = "k")]
    KillSession {
        /// Name of target session
        #[clap(value_parser)]
        target_session: Option<String>,
    },

    /// Kill all sessions
    #[clap(visible_alias = "ka")]
    KillAllSessions {
        /// Automatic yes to prompts
        #[clap(short, long, value_parser)]
        yes: bool,
    },
    /// Send actions to a specific session
    #[clap(visible_alias = "ac")]
    #[clap(subcommand)]
    Action(CliAction),
    /// Run a command in a new pane
    #[clap(visible_alias = "r")]
    Run {
        /// Command to run
        #[clap(last(true), required(true))]
        command: Vec<String>,

        /// Direction to open the new pane in
        #[clap(short, long, value_parser, conflicts_with("floating"))]
        direction: Option<Direction>,

        /// Change the working directory of the new pane
        #[clap(long, value_parser)]
        cwd: Option<PathBuf>,

        /// Open the new pane in floating mode
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        floating: bool,

        /// Name of the new pane
        #[clap(short, long, value_parser)]
        name: Option<String>,

        /// Close the pane immediately when its command exits
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        close_on_exit: bool,

        /// Start the command suspended, only running after you first presses ENTER
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        start_suspended: bool,
    },
    /// Edit file with default $EDITOR / $VISUAL
    #[clap(visible_alias = "e")]
    Edit {
        file: PathBuf,

        /// Open the file in the specified line number
        #[clap(short, long, value_parser)]
        line_number: Option<usize>,

        /// Direction to open the new pane in
        #[clap(short, long, value_parser, conflicts_with("floating"))]
        direction: Option<Direction>,

        /// Open the new pane in floating mode
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        floating: bool,

        /// Change the working directory of the editor
        #[clap(long, value_parser)]
        cwd: Option<PathBuf>,
    },
    ConvertConfig {
        old_config_file: PathBuf,
    },
    ConvertLayout {
        old_layout_file: PathBuf,
    },
    ConvertTheme {
        old_theme_file: PathBuf,
    },
}

#[derive(Debug, Subcommand, Clone, Serialize, Deserialize)]
pub enum CliAction {
    /// Write bytes to the terminal.
    Write {
        bytes: Vec<u8>,
    },
    /// Write characters to the terminal.
    WriteChars {
        chars: String,
    },
    /// [increase|decrease] the focused panes area at the [left|down|up|right] border.
    Resize {
        resize: Resize,
        direction: Option<Direction>,
    },
    /// Change focus to the next pane
    FocusNextPane,
    /// Change focus to the previous pane
    FocusPreviousPane,
    /// Move the focused pane in the specified direction. [right|left|up|down]
    MoveFocus {
        direction: Direction,
    },
    /// Move focus to the pane or tab (if on screen edge) in the specified direction
    /// [right|left|up|down]
    MoveFocusOrTab {
        direction: Direction,
    },
    /// Change the location of the focused pane in the specified direction or rotate forwrads
    /// [right|left|up|down]
    MovePane {
        direction: Option<Direction>,
    },
    /// Rotate the location of the previous pane backwards
    MovePaneBackwards,
    /// Clear all buffers for a focused pane
    Clear,
    /// Dump the focused pane to a file
    DumpScreen {
        path: PathBuf,

        /// Dump the pane with full scrollback
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        full: bool,
    },
    /// Open the pane scrollback in your default editor
    EditScrollback,
    /// Scroll up in the focused pane
    ScrollUp,
    /// Scroll down in focus pane.
    ScrollDown,
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
    ToggleFullscreen,
    /// Toggle frames around panes in the UI
    TogglePaneFrames,
    /// Toggle between sending text commands to all panes on the current tab and normal mode.
    ToggleActiveSyncTab,
    /// Open a new pane in the specified direction [right|down]
    /// If no direction is specified, will try to use the biggest available space.
    NewPane {
        /// Direction to open the new pane in
        #[clap(short, long, value_parser, conflicts_with("floating"))]
        direction: Option<Direction>,

        #[clap(last(true))]
        command: Vec<String>,

        #[clap(short, long, conflicts_with("command"), conflicts_with("direction"))]
        plugin: Option<String>,

        /// Change the working directory of the new pane
        #[clap(long, value_parser)]
        cwd: Option<PathBuf>,

        /// Open the new pane in floating mode
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        floating: bool,

        /// Name of the new pane
        #[clap(short, long, value_parser)]
        name: Option<String>,

        /// Close the pane immediately when its command exits
        #[clap(
            short,
            long,
            value_parser,
            default_value("false"),
            takes_value(false),
            requires("command")
        )]
        close_on_exit: bool,
        /// Start the command suspended, only running it after the you first press ENTER
        #[clap(
            short,
            long,
            value_parser,
            default_value("false"),
            takes_value(false),
            requires("command")
        )]
        start_suspended: bool,
    },
    /// Open the specified file in a new zellij pane with your default EDITOR
    Edit {
        file: PathBuf,

        /// Direction to open the new pane in
        #[clap(short, long, value_parser, conflicts_with("floating"))]
        direction: Option<Direction>,

        /// Open the file in the specified line number
        #[clap(short, long, value_parser)]
        line_number: Option<usize>,

        /// Open the new pane in floating mode
        #[clap(short, long, value_parser, default_value("false"), takes_value(false))]
        floating: bool,

        /// Change the working directory of the editor
        #[clap(long, value_parser)]
        cwd: Option<PathBuf>,
    },
    /// Switch input mode of all connected clients [locked|pane|tab|resize|move|search|session]
    SwitchMode {
        input_mode: InputMode,
    },
    /// Embed focused pane if floating or float focused pane if embedded
    TogglePaneEmbedOrFloating,
    /// Toggle the visibility of all fdirectionloating panes in the current Tab, open one if none exist
    ToggleFloatingPanes,
    /// Close the focused pane.
    ClosePane,
    /// Renames the focused pane
    RenamePane {
        name: String,
    },
    /// Remove a previously set pane name
    UndoRenamePane,
    /// Go to the next tab.
    GoToNextTab,
    /// Go to the previous tab.
    GoToPreviousTab,
    /// Close the current tab.
    CloseTab,
    /// Go to tab with index [index]
    GoToTab {
        index: u32,
    },
    /// Go to tab with name [name]
    GoToTabName {
        name: String,
        /// Create a tab if one does not exist.
        #[clap(short, long, value_parser)]
        create: bool,
    },
    /// Renames the focused pane
    RenameTab {
        name: String,
    },
    /// Remove a previously set tab name
    UndoRenameTab,
    /// Create a new tab, optionally with a specified tab layout and name
    NewTab {
        /// Layout to use for the new tab
        #[clap(short, long, value_parser)]
        layout: Option<PathBuf>,

        /// Default folder to look for layouts
        #[clap(long, value_parser, requires("layout"))]
        layout_dir: Option<PathBuf>,

        /// Name of the new tab
        #[clap(short, long, value_parser)]
        name: Option<String>,

        /// Change the working directory of the new tab
        #[clap(short, long, value_parser, requires("layout"))]
        cwd: Option<PathBuf>,
    },
    PreviousSwapLayout,
    NextSwapLayout,
    /// Query all tab names
    QueryTabNames,
    StartOrReloadPlugin {
        url: Url,
    },
}
