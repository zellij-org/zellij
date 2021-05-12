//! Handles cli and configuration options
use crate::cli::ConfigCli;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize, StructOpt)]
/// Options that can be set either through the config file,
/// or cli flags
/// intermediate struct
pub struct Options {
    /// Allow plugins to use a more simplified layout
    /// that is compatible with more fonts
    #[structopt(long)]
    pub simplified_ui: Option<bool>,
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize, Serialize)]
/// Merged version of the [`Options`] struct
// TODO: Maybe a good candidate for a macro?
pub struct ConfigOptions {
    pub simplified_ui: bool,
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
        let simplified_ui = if let Some(bool) = other.simplified_ui {
            Some(bool)
        } else {
            self.simplified_ui
        };

        Options { simplified_ui }
    }

    pub fn from_cli(&self, other: Option<ConfigCli>) -> Options {
        if let Some(ConfigCli::Options(options)) = other {
            Options::merge(&self, options)
        } else {
            self.to_owned()
        }
    }
}

impl From<Options> for ConfigOptions {
    fn from(options: Options) -> ConfigOptions {
        let simplified_ui = options.simplified_ui;

        ConfigOptions {
            simplified_ui: simplified_ui.unwrap_or_default(),
        }
    }
}
