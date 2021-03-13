//! Deserializes configuration options.
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use crate::utils::logging::*;

use directories_next::ProjectDirs;
use serde::Deserialize;

/// Intermediate deserialisation config struct
#[derive(Debug, Deserialize)]
pub struct ConfigFromYaml {
    pub keybinds: Option<KeybindsFromYaml>,
    pub auto_escape: Option<bool>,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub keybinds: Keybinds,
    pub auto_escape: bool,
}

#[derive(Debug)]
pub enum ConfigError {
    // Deserialisation error
    Serde(serde_yaml::Error),
    // Io error
    Io(io::Error),
    // Io error with path context
    IoPath(io::Error, PathBuf),
}

impl Default for Config {
    fn default() -> Self {
        let keybinds = Keybinds::default();
        Config {
            keybinds,
            auto_escape: false,
        }
    }
}

impl Config {
    /// Uses defaults, but lets config override them.
    pub fn from_yaml(yaml_config: &str) -> Result<Config, ConfigError> {
        let config_from_yaml: ConfigFromYaml = serde_yaml::from_str(&yaml_config)?;
        let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
        Ok(Config {
            keybinds,
            auto_escape: config_from_yaml.auto_escape.unwrap_or(false),
        })
    }

    /// Deserializes from given path.
    /// The allow is here, because rust assumes there is no
    /// error handling when logging the error to the log file.
    #[allow(unused_must_use)]
    pub fn new(path: &Path) -> Result<Config, ConfigError> {
        match File::open(path) {
            Ok(mut file) => {
                let mut yaml_config = String::new();
                file.read_to_string(&mut yaml_config)
                    .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
                Ok(Config::from_yaml(&yaml_config)?)
            }
            Err(e) => {
                debug_log_to_file(format!(
                    "{}\nUsing the default configuration!",
                    ConfigError::IoPath(e, path.to_path_buf())
                ));
                Ok(Config::default())
            }
        }
    }

    /// Deserializes the config from an optional path, or a platform specific path,
    /// merges the default configuration - options take precedence.
    pub fn from_option_or_default(option: Option<PathBuf>) -> Result<Config, ConfigError> {
        if let Some(config_path) = option {
            Ok(Config::new(&config_path)?)
        } else {
            let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
            let mut config_path: PathBuf = project_dirs.config_dir().to_owned();
            config_path.push("config.yaml");
            Ok(Config::new(&config_path)?)
        }
    }
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::Io(ref err) => write!(formatter, "IoError: {}", err),
            ConfigError::IoPath(ref err, ref path) => {
                write!(formatter, "IoError: {}, File: {}", err, path.display(),)
            }
            ConfigError::Serde(ref err) => write!(formatter, "Deserialisation error: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            ConfigError::Io(ref err) => Some(err),
            ConfigError::IoPath(ref err, _) => Some(err),
            ConfigError::Serde(ref err) => Some(err),
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(err: io::Error) -> ConfigError {
        ConfigError::Io(err)
    }
}

impl From<serde_yaml::Error> for ConfigError {
    fn from(err: serde_yaml::Error) -> ConfigError {
        ConfigError::Serde(err)
    }
}

// The unit test location.
#[cfg(test)]
#[path = "./ut/config_test.rs"]
mod config_test;
