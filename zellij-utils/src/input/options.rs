//! Handles cli and configuration options
use crate::cli::Command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;
use structopt::StructOpt;
use zellij_tile::data::InputMode;

#[derive(Copy, Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum OnForceClose {
    Quit,
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
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts
    #[structopt(long)]
    #[serde(default)]
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
    #[serde(default)]
    /// Disable handling of mouse events
    pub disable_mouse_mode: bool,
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
        let merge_bool = |opt_other, opt_self| if opt_other { true } else { opt_self };

        let simplified_ui = merge_bool(other.simplified_ui, self.simplified_ui);
        let disable_mouse_mode = merge_bool(other.disable_mouse_mode, self.disable_mouse_mode);

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
            on_force_close,
        }
    }

    pub fn from_cli(&self, other: Option<Command>) -> Options {
        if let Some(Command::Options(options)) = other {
            Options::merge(self, options)
        } else {
            self.to_owned()
        }
    }
}
