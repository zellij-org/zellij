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
        let simplified_ui = if other.simplified_ui {
            true
        } else {
            self.simplified_ui
        };

        let default_mode = match other.default_mode {
            None => self.default_mode,
            other => other,
        };

        let default_shell = match other.default_shell {
            None => self.default_shell.clone(),
            other => other,
        };

        let layout_dir = match other.layout_dir {
            None => self.layout_dir.clone(),
            other => other,
        };

        let theme = match other.theme {
            None => self.theme.clone(),
            other => other,
        };

        let disable_mouse_mode = if other.disable_mouse_mode {
            true
        } else {
            self.disable_mouse_mode
        };

        let on_force_close = match other.on_force_close {
            None => self.on_force_close,
            other => other,
        };

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
