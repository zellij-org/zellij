//! Handles cli and configuration options
use crate::cli::Command;
use crate::data::{InputMode, WebSharing};
use clap::{ArgEnum, Args};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

use std::net::IpAddr;

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize, ArgEnum)]
pub enum OnForceClose {
    #[serde(alias = "quit")]
    Quit,
    #[serde(alias = "detach")]
    Detach,
}

impl Default for OnForceClose {
    fn default() -> Self {
        Self::Detach
    }
}

impl FromStr for OnForceClose {
    type Err = Box<dyn std::error::Error>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "quit" => Ok(Self::Quit),
            "detach" => Ok(Self::Detach),
            e => Err(e.to_string().into()),
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, Args)]
/// Options that can be set either through the config file,
/// or cli flags - cli flags should take precedence over the config file
/// TODO: In order to correctly parse boolean flags, this is currently split
/// into Options and CliOptions, this could be a good canditate for a macro
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts (true or false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub simplified_ui: Option<bool>,
    /// Set the default theme
    #[clap(long, value_parser)]
    pub theme: Option<String>,
    /// Set the default mode
    #[clap(long, arg_enum, hide_possible_values = true, value_parser)]
    pub default_mode: Option<InputMode>,
    /// Set the default shell
    #[clap(long, value_parser)]
    pub default_shell: Option<PathBuf>,
    /// Set the default cwd
    #[clap(long, value_parser)]
    pub default_cwd: Option<PathBuf>,
    /// Set the default layout
    #[clap(long, value_parser)]
    pub default_layout: Option<PathBuf>,
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[clap(long, value_parser)]
    pub layout_dir: Option<PathBuf>,
    /// Set the theme_dir, defaults to
    /// subdirectory of config dir
    #[clap(long, value_parser)]
    pub theme_dir: Option<PathBuf>,
    #[clap(long, value_parser)]
    #[serde(default)]
    /// Set the handling of mouse events (true or false)
    /// Can be temporarily bypassed by the [SHIFT] key
    pub mouse_mode: Option<bool>,
    #[clap(long, value_parser)]
    #[serde(default)]
    /// Set display of the pane frames (true or false)
    pub pane_frames: Option<bool>,
    #[clap(long, value_parser)]
    #[serde(default)]
    /// Mirror session when multiple users are connected (true or false)
    pub mirror_session: Option<bool>,
    /// Set behaviour on force close (quit or detach)
    #[clap(long, arg_enum, hide_possible_values = true, value_parser)]
    pub on_force_close: Option<OnForceClose>,
    #[clap(long, value_parser)]
    pub scroll_buffer_size: Option<usize>,

    /// Switch to using a user supplied command for clipboard instead of OSC52
    #[clap(long, value_parser)]
    #[serde(default)]
    pub copy_command: Option<String>,

    /// OSC52 destination clipboard
    #[clap(
        long,
        arg_enum,
        ignore_case = true,
        conflicts_with = "copy-command",
        value_parser
    )]
    #[serde(default)]
    pub copy_clipboard: Option<Clipboard>,

    /// Automatically copy when selecting text (true or false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub copy_on_select: Option<bool>,

    /// Explicit full path to open the scrollback editor (default is $EDITOR or $VISUAL)
    #[clap(long, value_parser)]
    pub scrollback_editor: Option<PathBuf>,

    /// The name of the session to create when starting Zellij
    #[clap(long, value_parser)]
    #[serde(default)]
    pub session_name: Option<String>,

    /// Whether to attach to a session specified in "session-name" if it exists
    #[clap(long, value_parser)]
    #[serde(default)]
    pub attach_to_session: Option<bool>,

    /// Whether to lay out panes in a predefined set of layouts whenever possible
    #[clap(long, value_parser)]
    #[serde(default)]
    pub auto_layout: Option<bool>,

    /// Whether sessions should be serialized to the HD so that they can be later resurrected,
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub session_serialization: Option<bool>,

    /// Whether pane viewports are serialized along with the session, default is false
    #[clap(long, value_parser)]
    #[serde(default)]
    pub serialize_pane_viewport: Option<bool>,

    /// Scrollback lines to serialize along with the pane viewport when serializing sessions, 0
    /// defaults to the scrollback size. If this number is higher than the scrollback size, it will
    /// also default to the scrollback size
    #[clap(long, value_parser)]
    #[serde(default)]
    pub scrollback_lines_to_serialize: Option<usize>,

    /// Whether to use ANSI styled underlines
    #[clap(long, value_parser)]
    #[serde(default)]
    pub styled_underlines: Option<bool>,

    /// The interval at which to serialize sessions for resurrection (in seconds)
    #[clap(long, value_parser)]
    pub serialization_interval: Option<u64>,

    /// If true, will disable writing session metadata to disk
    #[clap(long, value_parser)]
    pub disable_session_metadata: Option<bool>,

    /// Whether to enable support for the Kitty keyboard protocol (must also be supported by the
    /// host terminal), defaults to true if the terminal supports it
    #[clap(long, value_parser)]
    #[serde(default)]
    pub support_kitty_keyboard_protocol: Option<bool>,

    /// Whether to make sure a local web server is running when a new Zellij session starts.
    /// This web server will allow creating new sessions and attaching to existing ones that have
    /// opted in to being shared in the browser.
    ///
    /// Note: a local web server can still be manually started from within a Zellij session or from the CLI.
    /// If this is not desired, one can use a version of Zellij compiled without
    /// web_server_capability
    ///
    /// Possible values:
    /// - true
    /// - false
    /// Default: false
    #[clap(long, value_parser)]
    #[serde(default)]
    pub web_server: Option<bool>,

    /// Whether to allow new sessions to be shared through a local web server, assuming one is
    /// running (see the `web_server` option for more details).
    ///
    /// Note: if Zellij was compiled without web_server_capability, this option will be locked to
    /// "disabled"
    ///
    /// Possible values:
    /// - "on" (new sessions will allow web sharing through the local web server if it
    /// is online)
    /// - "off" (new sessions will not allow web sharing unless they explicitly opt-in to it)
    /// - "disabled" (new sessions will not allow web sharing and will not be able to opt-in to it)
    /// Default: "off"
    #[clap(long, value_parser)]
    #[serde(default)]
    pub web_sharing: Option<WebSharing>,

    /// Whether to stack panes when resizing beyond a certain size
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub stacked_resize: Option<bool>,

    /// Whether to show startup tips when starting a new session
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub show_startup_tips: Option<bool>,

    /// Whether to show release notes on first run of a new version
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub show_release_notes: Option<bool>,

    /// Whether to enable mouse hover effects and pane grouping functionality
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub advanced_mouse_actions: Option<bool>,

    // these are intentionally excluded from the CLI options as they must be specified in the
    // configuration file
    pub web_server_ip: Option<IpAddr>,
    pub web_server_port: Option<u16>,
    pub web_server_cert: Option<PathBuf>,
    pub web_server_key: Option<PathBuf>,
    pub enforce_https_for_localhost: Option<bool>,
    /// A command to run after the discovery of running commands when serializing, for the purpose
    /// of manipulating the command (eg. with a regex) before it gets serialized
    #[clap(long, value_parser)]
    pub post_command_discovery_hook: Option<String>,
}

#[derive(ArgEnum, Deserialize, Serialize, Debug, Clone, Copy, PartialEq)]
pub enum Clipboard {
    #[serde(alias = "system")]
    System,
    #[serde(alias = "primary")]
    Primary,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::System
    }
}

impl FromStr for Clipboard {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "System" | "system" => Ok(Self::System),
            "Primary" | "primary" => Ok(Self::Primary),
            _ => Err(format!("No such clipboard: {}", s)),
        }
    }
}

impl Options {
    pub fn from_yaml(from_yaml: Option<Options>) -> Options {
        if let Some(opts) = from_yaml {
            opts
        } else {
            Options::default()
        }
    }
    /// Merges two [`Options`] structs, a `Some` in `other`
    /// will supersede a `Some` in `self`
    // TODO: Maybe a good candidate for a macro?
    pub fn merge(&self, other: Options) -> Options {
        let mouse_mode = other.mouse_mode.or(self.mouse_mode);
        let pane_frames = other.pane_frames.or(self.pane_frames);
        let auto_layout = other.auto_layout.or(self.auto_layout);
        let mirror_session = other.mirror_session.or(self.mirror_session);
        let simplified_ui = other.simplified_ui.or(self.simplified_ui);
        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let default_cwd = other.default_cwd.or_else(|| self.default_cwd.clone());
        let default_layout = other.default_layout.or_else(|| self.default_layout.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme_dir = other.theme_dir.or_else(|| self.theme_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or(self.copy_clipboard);
        let copy_on_select = other.copy_on_select.or(self.copy_on_select);
        let scrollback_editor = other
            .scrollback_editor
            .or_else(|| self.scrollback_editor.clone());
        let session_name = other.session_name.or_else(|| self.session_name.clone());
        let attach_to_session = other
            .attach_to_session
            .or_else(|| self.attach_to_session.clone());
        let session_serialization = other.session_serialization.or(self.session_serialization);
        let serialize_pane_viewport = other
            .serialize_pane_viewport
            .or(self.serialize_pane_viewport);
        let scrollback_lines_to_serialize = other
            .scrollback_lines_to_serialize
            .or(self.scrollback_lines_to_serialize);
        let styled_underlines = other.styled_underlines.or(self.styled_underlines);
        let serialization_interval = other.serialization_interval.or(self.serialization_interval);
        let disable_session_metadata = other
            .disable_session_metadata
            .or(self.disable_session_metadata);
        let support_kitty_keyboard_protocol = other
            .support_kitty_keyboard_protocol
            .or(self.support_kitty_keyboard_protocol);
        let web_server = other.web_server.or(self.web_server);
        let web_sharing = other.web_sharing.or(self.web_sharing);
        let stacked_resize = other.stacked_resize.or(self.stacked_resize);
        let show_startup_tips = other.show_startup_tips.or(self.show_startup_tips);
        let show_release_notes = other.show_release_notes.or(self.show_release_notes);
        let advanced_mouse_actions = other.advanced_mouse_actions.or(self.advanced_mouse_actions);
        let web_server_ip = other.web_server_ip.or(self.web_server_ip);
        let web_server_port = other.web_server_port.or(self.web_server_port);
        let web_server_cert = other
            .web_server_cert
            .or_else(|| self.web_server_cert.clone());
        let web_server_key = other.web_server_key.or_else(|| self.web_server_key.clone());
        let enforce_https_for_localhost = other
            .enforce_https_for_localhost
            .or(self.enforce_https_for_localhost);
        let post_command_discovery_hook = other
            .post_command_discovery_hook
            .or(self.post_command_discovery_hook.clone());

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            default_cwd,
            default_layout,
            layout_dir,
            theme_dir,
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
            copy_on_select,
            scrollback_editor,
            session_name,
            attach_to_session,
            auto_layout,
            session_serialization,
            serialize_pane_viewport,
            scrollback_lines_to_serialize,
            styled_underlines,
            serialization_interval,
            disable_session_metadata,
            support_kitty_keyboard_protocol,
            web_server,
            web_sharing,
            stacked_resize,
            show_startup_tips,
            show_release_notes,
            advanced_mouse_actions,
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
            enforce_https_for_localhost,
            post_command_discovery_hook,
        }
    }

    /// Merges two [`Options`] structs,
    /// - `Some` in `other` will supersede a `Some` in `self`
    /// - `Some(bool)` in `other` will toggle a `Some(bool)` in `self`
    // TODO: Maybe a good candidate for a macro?
    pub fn merge_from_cli(&self, other: Options) -> Options {
        let merge_bool = |opt_other: Option<bool>, opt_self: Option<bool>| {
            if opt_other.is_some() ^ opt_self.is_some() {
                opt_other.or(opt_self)
            } else if opt_other.is_some() && opt_self.is_some() {
                Some(opt_other.unwrap() ^ opt_self.unwrap())
            } else {
                None
            }
        };

        let simplified_ui = merge_bool(other.simplified_ui, self.simplified_ui);
        let mouse_mode = merge_bool(other.mouse_mode, self.mouse_mode);
        let pane_frames = merge_bool(other.pane_frames, self.pane_frames);
        let auto_layout = merge_bool(other.auto_layout, self.auto_layout);
        let mirror_session = merge_bool(other.mirror_session, self.mirror_session);
        let session_serialization =
            merge_bool(other.session_serialization, self.session_serialization);
        let serialize_pane_viewport =
            merge_bool(other.serialize_pane_viewport, self.serialize_pane_viewport);

        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let default_cwd = other.default_cwd.or_else(|| self.default_cwd.clone());
        let default_layout = other.default_layout.or_else(|| self.default_layout.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme_dir = other.theme_dir.or_else(|| self.theme_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or(self.copy_clipboard);
        let copy_on_select = other.copy_on_select.or(self.copy_on_select);
        let scrollback_editor = other
            .scrollback_editor
            .or_else(|| self.scrollback_editor.clone());
        let session_name = other.session_name.or_else(|| self.session_name.clone());
        let attach_to_session = other
            .attach_to_session
            .or_else(|| self.attach_to_session.clone());
        let scrollback_lines_to_serialize = other
            .scrollback_lines_to_serialize
            .or_else(|| self.scrollback_lines_to_serialize.clone());
        let styled_underlines = other.styled_underlines.or(self.styled_underlines);
        let serialization_interval = other.serialization_interval.or(self.serialization_interval);
        let disable_session_metadata = other
            .disable_session_metadata
            .or(self.disable_session_metadata);
        let support_kitty_keyboard_protocol = other
            .support_kitty_keyboard_protocol
            .or(self.support_kitty_keyboard_protocol);
        let web_server = other.web_server.or(self.web_server);
        let web_sharing = other.web_sharing.or(self.web_sharing);
        let stacked_resize = other.stacked_resize.or(self.stacked_resize);
        let show_startup_tips = other.show_startup_tips.or(self.show_startup_tips);
        let show_release_notes = other.show_release_notes.or(self.show_release_notes);
        let advanced_mouse_actions = other.advanced_mouse_actions.or(self.advanced_mouse_actions);
        let web_server_ip = other.web_server_ip.or(self.web_server_ip);
        let web_server_port = other.web_server_port.or(self.web_server_port);
        let web_server_cert = other
            .web_server_cert
            .or_else(|| self.web_server_cert.clone());
        let web_server_key = other.web_server_key.or_else(|| self.web_server_key.clone());
        let enforce_https_for_localhost = other
            .enforce_https_for_localhost
            .or(self.enforce_https_for_localhost);
        let post_command_discovery_hook = other
            .post_command_discovery_hook
            .or_else(|| self.post_command_discovery_hook.clone());

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            default_cwd,
            default_layout,
            layout_dir,
            theme_dir,
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
            copy_on_select,
            scrollback_editor,
            session_name,
            attach_to_session,
            auto_layout,
            session_serialization,
            serialize_pane_viewport,
            scrollback_lines_to_serialize,
            styled_underlines,
            serialization_interval,
            disable_session_metadata,
            support_kitty_keyboard_protocol,
            web_server,
            web_sharing,
            stacked_resize,
            show_startup_tips,
            show_release_notes,
            advanced_mouse_actions,
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
            enforce_https_for_localhost,
            post_command_discovery_hook,
        }
    }

    pub fn from_cli(&self, other: Option<Command>) -> Options {
        if let Some(Command::Options(options)) = other {
            Options::merge_from_cli(self, options.into())
        } else {
            self.to_owned()
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq, Args, Serialize, Deserialize)]
/// Options that can be set through cli flags
/// boolean flags end up toggling boolean options in `Options`
pub struct CliOptions {
    /// Disable handling of mouse events
    #[clap(long, conflicts_with("mouse-mode"), value_parser)]
    pub disable_mouse_mode: bool,
    /// Disable display of pane frames
    #[clap(long, conflicts_with("pane-frames"), value_parser)]
    pub no_pane_frames: bool,
    #[clap(flatten)]
    pub options: Options,
}

impl From<CliOptions> for Options {
    fn from(cli_options: CliOptions) -> Self {
        let mut opts = cli_options.options;

        // TODO: what?
        if cli_options.no_pane_frames {
            opts.pane_frames = Some(false);
        }
        if cli_options.disable_mouse_mode {
            opts.mouse_mode = Some(false);
        }

        Self {
            simplified_ui: opts.simplified_ui,
            theme: opts.theme,
            default_mode: opts.default_mode,
            default_shell: opts.default_shell,
            default_cwd: opts.default_cwd,
            default_layout: opts.default_layout,
            layout_dir: opts.layout_dir,
            theme_dir: opts.theme_dir,
            mouse_mode: opts.mouse_mode,
            pane_frames: opts.pane_frames,
            mirror_session: opts.mirror_session,
            on_force_close: opts.on_force_close,
            scroll_buffer_size: opts.scroll_buffer_size,
            copy_command: opts.copy_command,
            copy_clipboard: opts.copy_clipboard,
            copy_on_select: opts.copy_on_select,
            scrollback_editor: opts.scrollback_editor,
            session_name: opts.session_name,
            attach_to_session: opts.attach_to_session,
            auto_layout: opts.auto_layout,
            session_serialization: opts.session_serialization,
            serialize_pane_viewport: opts.serialize_pane_viewport,
            scrollback_lines_to_serialize: opts.scrollback_lines_to_serialize,
            styled_underlines: opts.styled_underlines,
            serialization_interval: opts.serialization_interval,
            support_kitty_keyboard_protocol: opts.support_kitty_keyboard_protocol,
            web_server: opts.web_server,
            web_sharing: opts.web_sharing,
            stacked_resize: opts.stacked_resize,
            show_startup_tips: opts.show_startup_tips,
            show_release_notes: opts.show_release_notes,
            advanced_mouse_actions: opts.advanced_mouse_actions,
            web_server_ip: opts.web_server_ip,
            web_server_port: opts.web_server_port,
            web_server_cert: opts.web_server_cert,
            web_server_key: opts.web_server_key,
            enforce_https_for_localhost: opts.enforce_https_for_localhost,
            post_command_discovery_hook: opts.post_command_discovery_hook,
            ..Default::default()
        }
    }
}
