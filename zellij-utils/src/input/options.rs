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
    /// Theme name to apply when the host terminal reports a dark color palette
    /// (CSI 2031 / DSR 997). Requires `theme_light` to also be set; if either
    /// is missing the static `theme` remains authoritative.
    #[clap(long, value_parser)]
    pub theme_dark: Option<String>,
    /// Theme name to apply when the host terminal reports a light color palette
    /// (CSI 2031 / DSR 997). Requires `theme_dark` to also be set; if either
    /// is missing the static `theme` remains authoritative.
    #[clap(long, value_parser)]
    pub theme_light: Option<String>,
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

    /// Enable OSC8 hyperlink output (true or false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc8_hyperlinks: Option<bool>,

    /// OSC 1337 (WezTerm/iTerm2) master switch. When `false`, no OSC 1337
    /// sub-command is forwarded to the host terminal regardless of the
    /// per-sub-command toggles below. (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_passthrough: Option<bool>,

    /// Forward OSC 1337 `File=` inline image sequences (`inline=1` only;
    /// `inline=0` host-side downloads are always blocked). (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_inline_images: Option<bool>,

    /// Forward OSC 1337 `SetMark` (records a scrollback mark in the host
    /// terminal). (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_set_mark: Option<bool>,

    /// Forward OSC 1337 `CurrentDir=` (host terminal shell-integration
    /// breadcrumbs). (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_current_dir: Option<bool>,

    /// Forward OSC 1337 `HighlightCursorLine=`. (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_highlight_cursor_line: Option<bool>,

    /// Forward OSC 1337 `UnicodeVersion=`. (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_unicode_version: Option<bool>,

    /// Forward OSC 1337 `SetUserVar=` (fires Lua callbacks in WezTerm).
    /// Off by default because the host terminal's user-var handlers may
    /// trigger arbitrary actions. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_set_user_var: Option<bool>,

    /// Forward OSC 1337 `SetProfile=` (mutates host terminal profile —
    /// keybindings, colors, font). Off by default. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_set_profile: Option<bool>,

    /// Forward OSC 1337 `SetBadgeFormat=` (host terminal badge UI). Off
    /// by default. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_set_badge_format: Option<bool>,

    /// Forward OSC 1337 `ClearScrollback` (erases the host terminal's
    /// scrollback, not just the pane's). Off by default. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_clear_scrollback: Option<bool>,

    /// Forward OSC 1337 `Copy=` / `CopyToClipboard=` / `EndCopy` (direct
    /// clipboard write). Off by default — clipboard injection is a known
    /// vector for malicious remote processes. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_clipboard_copy: Option<bool>,

    /// Forward OSC 1337 `StealFocus` (forces host terminal window to
    /// foreground). Off by default. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_steal_focus: Option<bool>,

    /// Forward OSC 1337 `RequestAttention=yes|once|no|fireworks` (host
    /// terminal dock-bouncing / window highlighting). Off by default —
    /// same risk class as `StealFocus`. (default: false)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_request_attention: Option<bool>,

    /// Forward OSC 1337 `RemoteHost=user@host` (shell-integration login
    /// metadata). Pure metadata, on by default. (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_remote_host: Option<bool>,

    /// Forward OSC 1337 `ShellIntegrationVersion=<version>[;<shell>]`
    /// (shell-integration version advertisement). Pure metadata, on by
    /// default. (default: true)
    #[clap(long, value_parser)]
    #[serde(default)]
    pub osc1337_shell_integration_version: Option<bool>,

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

    /// Whether to enable mouse hover visual effects (frame highlight and help text)
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub mouse_hover_effects: Option<bool>,

    /// Whether to show visual bell indicators (pane/tab frame flash and [!] suffix)
    /// default is true
    #[clap(long, value_parser)]
    #[serde(default)]
    pub visual_bell: Option<bool>,

    /// Whether to focus panes on mouse hover (true or false)
    /// default is false
    #[clap(long, value_parser)]
    #[serde(default)]
    pub focus_follows_mouse: Option<bool>,

    /// Whether clicking a pane to focus it also sends the click into the pane (true or false)
    /// default is false
    #[clap(long, value_parser)]
    #[serde(default)]
    pub mouse_click_through: Option<bool>,

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

    /// Number of async worker tasks to spawn per active client.
    ///
    /// Allocating few tasks may result in resource contention and lags. Small values (around 4)
    /// should typically work best. Set to 0 to use the number of (physical) CPU cores.
    /// NOTE: This only applies to web clients at the moment.
    #[clap(long)]
    pub client_async_worker_tasks: Option<usize>,
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
        let theme_dark = other.theme_dark.or_else(|| self.theme_dark.clone());
        let theme_light = other.theme_light.or_else(|| self.theme_light.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or(self.copy_clipboard);
        let copy_on_select = other.copy_on_select.or(self.copy_on_select);
        let osc8_hyperlinks = other.osc8_hyperlinks.or(self.osc8_hyperlinks);
        let osc1337_passthrough = other.osc1337_passthrough.or(self.osc1337_passthrough);
        let osc1337_inline_images = other.osc1337_inline_images.or(self.osc1337_inline_images);
        let osc1337_set_mark = other.osc1337_set_mark.or(self.osc1337_set_mark);
        let osc1337_current_dir = other.osc1337_current_dir.or(self.osc1337_current_dir);
        let osc1337_highlight_cursor_line = other
            .osc1337_highlight_cursor_line
            .or(self.osc1337_highlight_cursor_line);
        let osc1337_unicode_version = other
            .osc1337_unicode_version
            .or(self.osc1337_unicode_version);
        let osc1337_set_user_var = other.osc1337_set_user_var.or(self.osc1337_set_user_var);
        let osc1337_set_profile = other.osc1337_set_profile.or(self.osc1337_set_profile);
        let osc1337_set_badge_format = other
            .osc1337_set_badge_format
            .or(self.osc1337_set_badge_format);
        let osc1337_clear_scrollback = other
            .osc1337_clear_scrollback
            .or(self.osc1337_clear_scrollback);
        let osc1337_clipboard_copy = other.osc1337_clipboard_copy.or(self.osc1337_clipboard_copy);
        let osc1337_steal_focus = other.osc1337_steal_focus.or(self.osc1337_steal_focus);
        let osc1337_request_attention = other
            .osc1337_request_attention
            .or(self.osc1337_request_attention);
        let osc1337_remote_host = other.osc1337_remote_host.or(self.osc1337_remote_host);
        let osc1337_shell_integration_version = other
            .osc1337_shell_integration_version
            .or(self.osc1337_shell_integration_version);
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
        let mouse_hover_effects = other.mouse_hover_effects.or(self.mouse_hover_effects);
        let visual_bell = other.visual_bell.or(self.visual_bell);
        let focus_follows_mouse = other.focus_follows_mouse.or(self.focus_follows_mouse);
        let mouse_click_through = other.mouse_click_through.or(self.mouse_click_through);
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
        let client_async_worker_tasks = other
            .client_async_worker_tasks
            .or(self.client_async_worker_tasks);

        Options {
            simplified_ui,
            theme,
            theme_dark,
            theme_light,
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
            osc8_hyperlinks,
            osc1337_passthrough,
            osc1337_inline_images,
            osc1337_set_mark,
            osc1337_current_dir,
            osc1337_highlight_cursor_line,
            osc1337_unicode_version,
            osc1337_set_user_var,
            osc1337_set_profile,
            osc1337_set_badge_format,
            osc1337_clear_scrollback,
            osc1337_clipboard_copy,
            osc1337_steal_focus,
            osc1337_request_attention,
            osc1337_remote_host,
            osc1337_shell_integration_version,
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
            mouse_hover_effects,
            visual_bell,
            focus_follows_mouse,
            mouse_click_through,
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
            enforce_https_for_localhost,
            post_command_discovery_hook,
            client_async_worker_tasks,
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
        let theme_dark = other.theme_dark.or_else(|| self.theme_dark.clone());
        let theme_light = other.theme_light.or_else(|| self.theme_light.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or(self.copy_clipboard);
        let copy_on_select = other.copy_on_select.or(self.copy_on_select);
        let osc8_hyperlinks = other.osc8_hyperlinks.or(self.osc8_hyperlinks);
        let osc1337_passthrough = merge_bool(other.osc1337_passthrough, self.osc1337_passthrough);
        let osc1337_inline_images =
            merge_bool(other.osc1337_inline_images, self.osc1337_inline_images);
        let osc1337_set_mark = merge_bool(other.osc1337_set_mark, self.osc1337_set_mark);
        let osc1337_current_dir = merge_bool(other.osc1337_current_dir, self.osc1337_current_dir);
        let osc1337_highlight_cursor_line = merge_bool(
            other.osc1337_highlight_cursor_line,
            self.osc1337_highlight_cursor_line,
        );
        let osc1337_unicode_version =
            merge_bool(other.osc1337_unicode_version, self.osc1337_unicode_version);
        let osc1337_set_user_var =
            merge_bool(other.osc1337_set_user_var, self.osc1337_set_user_var);
        let osc1337_set_profile = merge_bool(other.osc1337_set_profile, self.osc1337_set_profile);
        let osc1337_set_badge_format = merge_bool(
            other.osc1337_set_badge_format,
            self.osc1337_set_badge_format,
        );
        let osc1337_clear_scrollback = merge_bool(
            other.osc1337_clear_scrollback,
            self.osc1337_clear_scrollback,
        );
        let osc1337_clipboard_copy =
            merge_bool(other.osc1337_clipboard_copy, self.osc1337_clipboard_copy);
        let osc1337_steal_focus = merge_bool(other.osc1337_steal_focus, self.osc1337_steal_focus);
        let osc1337_request_attention = merge_bool(
            other.osc1337_request_attention,
            self.osc1337_request_attention,
        );
        let osc1337_remote_host = merge_bool(other.osc1337_remote_host, self.osc1337_remote_host);
        let osc1337_shell_integration_version = merge_bool(
            other.osc1337_shell_integration_version,
            self.osc1337_shell_integration_version,
        );
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
        let mouse_hover_effects = other.mouse_hover_effects.or(self.mouse_hover_effects);
        let visual_bell = other.visual_bell.or(self.visual_bell);
        let focus_follows_mouse = merge_bool(other.focus_follows_mouse, self.focus_follows_mouse);
        let mouse_click_through = merge_bool(other.mouse_click_through, self.mouse_click_through);
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
        let client_async_worker_tasks = other
            .client_async_worker_tasks
            .or(self.client_async_worker_tasks);

        Options {
            simplified_ui,
            theme,
            theme_dark,
            theme_light,
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
            osc8_hyperlinks,
            osc1337_passthrough,
            osc1337_inline_images,
            osc1337_set_mark,
            osc1337_current_dir,
            osc1337_highlight_cursor_line,
            osc1337_unicode_version,
            osc1337_set_user_var,
            osc1337_set_profile,
            osc1337_set_badge_format,
            osc1337_clear_scrollback,
            osc1337_clipboard_copy,
            osc1337_steal_focus,
            osc1337_request_attention,
            osc1337_remote_host,
            osc1337_shell_integration_version,
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
            mouse_hover_effects,
            visual_bell,
            focus_follows_mouse,
            mouse_click_through,
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
            enforce_https_for_localhost,
            post_command_discovery_hook,
            client_async_worker_tasks,
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
