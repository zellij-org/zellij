//! Handles cli and configuration options
use crate::cli::Command;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use structopt::StructOpt;
use zellij_tile::data::InputMode;

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, StructOpt)]
/// Options that can be set either through the config file,
/// or cli flags
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
    /// Set the layout_dir, defaults to
    /// subdirectory of config dir
    #[structopt(long, parse(from_os_str))]
    pub layout_dir: Option<PathBuf>,
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

        let layout_dir = match other.layout_dir {
            None => self.layout_dir.clone(),
            other => other,
        };

        let theme = match other.theme {
            None => self.theme.clone(),
            other => other,
        };

        Options {
            simplified_ui,
            theme,
            default_mode,
            layout_dir,
        }
    }

    pub fn from_cli(&self, other: Option<Command>) -> Options {
        if let Some(Command::Options(options)) = other {
            Options::merge(&self, options)
        } else {
            self.to_owned()
        }
    }
}
