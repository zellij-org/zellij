//! Handles cli and configuration options
use crate::cli::Command;
use clap::{ArgEnum, Args};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use zellij_tile::data::InputMode;

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
    #[clap(long)]
    #[serde(default)]
    pub simplified_ui: Option<bool>,
    /// Set the default theme
    #[clap(long)]
    pub theme: Option<String>,
    /// Set the default mode
    #[clap(long, arg_enum, hide_possible_values = true)]
    pub default_mode: Option<InputMode>,
    /// Set the default shell
    #[clap(long, parse(from_os_str))]
    pub default_shell: Option<PathBuf>,
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[clap(long, parse(from_os_str))]
    pub layout_dir: Option<PathBuf>,
    #[clap(long)]
    #[serde(default)]
    /// Set the handling of mouse events (true or false)
    /// Can be temporarily bypassed by the [SHIFT] key
    pub mouse_mode: Option<bool>,
    #[clap(long)]
    #[serde(default)]
    /// Set display of the pane frames (true or false)
    pub pane_frames: Option<bool>,
    #[clap(long)]
    #[serde(default)]
    /// Mirror session when multiple users are connected (true or false)
    pub mirror_session: Option<bool>,
    /// Set behaviour on force close (quit or detach)
    #[clap(long, arg_enum, hide_possible_values = true)]
    pub on_force_close: Option<OnForceClose>,
    #[clap(long)]
    pub scroll_buffer_size: Option<usize>,

    /// Switch to using a user supplied command for clipboard instead of OSC52
    #[clap(long)]
    #[serde(default)]
    pub copy_command: Option<String>,

    /// OSC52 destination clipboard
    #[clap(long, arg_enum, ignore_case = true, conflicts_with = "copy-command")]
    #[serde(default)]
    pub copy_clipboard: Option<Clipboard>,
}

#[derive(ArgEnum, Deserialize, Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Clipboard {
    System,
    Primary,
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::System
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
    /// will supercede a `Some` in `self`
    // TODO: Maybe a good candidate for a macro?
    pub fn merge(&self, other: Options) -> Options {
        let mouse_mode = other.mouse_mode.or(self.mouse_mode);
        let pane_frames = other.pane_frames.or(self.pane_frames);
        let mirror_session = other.mirror_session.or(self.mirror_session);
        let simplified_ui = other.simplified_ui.or(self.simplified_ui);
        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or_else(|| self.copy_clipboard.clone());

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
        }
    }

    /// Merges two [`Options`] structs,
    /// - `Some` in `other` will supercede a `Some` in `self`
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
        let mirror_session = merge_bool(other.mirror_session, self.mirror_session);

        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);
        let scroll_buffer_size = other.scroll_buffer_size.or(self.scroll_buffer_size);
        let copy_command = other.copy_command.or_else(|| self.copy_command.clone());
        let copy_clipboard = other.copy_clipboard.or_else(|| self.copy_clipboard.clone());

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
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
    #[clap(long, conflicts_with("mouse-mode"))]
    pub disable_mouse_mode: bool,
    /// Disable display of pane frames
    #[clap(long, conflicts_with("pane-frames"))]
    pub no_pane_frames: bool,
    #[clap(flatten)]
    options: Options,
}

impl From<CliOptions> for Options {
    fn from(cli_options: CliOptions) -> Self {
        let mut opts = cli_options.options;

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
            layout_dir: opts.layout_dir,
            mouse_mode: opts.mouse_mode,
            pane_frames: opts.pane_frames,
            mirror_session: opts.mirror_session,
            on_force_close: opts.on_force_close,
            scroll_buffer_size: opts.scroll_buffer_size,
            copy_command: opts.copy_command,
            copy_clipboard: opts.copy_clipboard,
        }
    }
}
