//! Handles cli and configuration options
use crate::cli::Command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use zellij_tile::data::InputMode;

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
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

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, StructOpt)]
/// Options that can be set either through the config file,
/// or cli flags - cli flags should take precedence over the config file
/// TODO: In order to correctly parse boolean flags, this is currently split
/// into Options and CliOptions, this could be a good canditate for a macro
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts
    #[structopt(long)]
    #[serde(default)]
    pub simplified_ui: Option<bool>,
    /// Set the default theme
    #[structopt(long)]
    pub theme: Option<String>,
    /// Set the default mode
    #[structopt(long)]
    pub default_mode: Option<InputMode>,
    /// Set the default shell
    #[structopt(long, parse(from_os_str))]
    pub default_shell: Option<PathBuf>,
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[structopt(long, parse(from_os_str))]
    pub layout_dir: Option<PathBuf>,
    #[structopt(long)]
    #[serde(default)]
    /// Disable handling of mouse events
    pub disable_mouse_mode: Option<bool>,
    #[structopt(long)]
    #[serde(default)]
    pub no_pane_frames: Option<bool>,
    /// Set behaviour on force close (quit or detach)
    #[structopt(long)]
    pub on_force_close: Option<OnForceClose>,
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
        let disable_mouse_mode = other.disable_mouse_mode.or(self.disable_mouse_mode);
        let no_pane_frames = other.no_pane_frames.or(self.no_pane_frames);
        let simplified_ui = other.simplified_ui.or(self.simplified_ui);
        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            disable_mouse_mode,
            no_pane_frames,
            on_force_close,
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
        let disable_mouse_mode = merge_bool(other.disable_mouse_mode, self.disable_mouse_mode);
        let no_pane_frames = merge_bool(other.no_pane_frames, self.no_pane_frames);

        let default_mode = other.default_mode.or(self.default_mode);
        let default_shell = other.default_shell.or_else(|| self.default_shell.clone());
        let layout_dir = other.layout_dir.or_else(|| self.layout_dir.clone());
        let theme = other.theme.or_else(|| self.theme.clone());
        let on_force_close = other.on_force_close.or(self.on_force_close);

        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            layout_dir,
            disable_mouse_mode,
            no_pane_frames,
            on_force_close,
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

#[derive(Clone, Default, Debug, PartialEq, StructOpt, Serialize, Deserialize)]
/// Options that can be set through cli flags
/// boolean flags end up toggling boolean options in `Options`
pub struct CliOptions {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts
    #[structopt(long)]
    pub simplified_ui: bool,
    /// Set the default theme
    #[structopt(long)]
    pub theme: Option<String>,
    /// Set the default mode
    #[structopt(long)]
    pub default_mode: Option<InputMode>,
    /// Set the default shell
    #[structopt(long, parse(from_os_str))]
    pub default_shell: Option<PathBuf>,
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[structopt(long, parse(from_os_str))]
    pub layout_dir: Option<PathBuf>,
    #[structopt(long)]
    /// Disable handling of mouse events
    pub disable_mouse_mode: bool,
    #[structopt(long)]
    pub no_pane_frames: bool,
    /// Set behaviour on force close (quit or detach)
    #[structopt(long)]
    pub on_force_close: Option<OnForceClose>,
}

impl From<CliOptions> for Options {
    fn from(cli_options: CliOptions) -> Self {
        let handle_bool = |bool| {
            if bool {
                Some(true)
            } else {
                None
            }
        };

        Self {
            simplified_ui: handle_bool(cli_options.simplified_ui),
            theme: cli_options.theme,
            default_mode: cli_options.default_mode,
            default_shell: cli_options.default_shell,
            layout_dir: cli_options.layout_dir,
            disable_mouse_mode: handle_bool(cli_options.disable_mouse_mode),
            no_pane_frames: handle_bool(cli_options.no_pane_frames),
            on_force_close: cli_options.on_force_close,
        }
    }
}
