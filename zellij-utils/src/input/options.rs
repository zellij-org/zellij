//! Handles cli and configuration options
use crate::cli::Command;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, StructOpt)]
/// Options that can be set either through the config file,
/// or cli flags
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts
    #[structopt(long)]
    pub simplified_ui: bool,
    /// Set the default theme
    #[structopt(long)]
    pub theme: Option<String>,
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

        let theme = None;

        Options {
            simplified_ui,
            theme,
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
