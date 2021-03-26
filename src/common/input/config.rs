//! Deserializes configuration options.
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use crate::cli::ConfigCli;

use directories_next::ProjectDirs;
use serde::Deserialize;

type ConfigResult = Result<Config, ConfigError>;

/// Intermediate deserialisation config struct
#[derive(Debug, Deserialize)]
pub struct ConfigFromYaml {
    pub keybinds: Option<KeybindsFromYaml>,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub keybinds: Keybinds,
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
        Config { keybinds }
    }
}

impl Config {
    /// Uses defaults, but lets config override them.
    pub fn from_yaml(yaml_config: &str) -> ConfigResult {
        let config_from_yaml: ConfigFromYaml = serde_yaml::from_str(&yaml_config)?;
        let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
        Ok(Config { keybinds })
    }

    /// Deserializes from given path.
    #[allow(unused_must_use)]
    pub fn new(path: &Path) -> ConfigResult {
        match File::open(path) {
            Ok(mut file) => {
                let mut yaml_config = String::new();
                file.read_to_string(&mut yaml_config)
                    .map_err(|e| ConfigError::IoPath(e, path.to_path_buf()))?;
                Ok(Config::from_yaml(&yaml_config)?)
            }
            Err(e) => Err(ConfigError::IoPath(e, path.into())),
        }
    }

    /// Deserializes the config from a default platform specific path,
    /// merges the default configuration - options take precedence.
    fn from_default_path() -> ConfigResult {
        let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
        let mut config_path: PathBuf = project_dirs.config_dir().to_owned();
        config_path.push("config.yaml");

        match Config::new(&config_path) {
            Ok(config) => Ok(config),
            Err(ConfigError::IoPath(_,_)) => Ok(Config::default()),
            Err(e) => Err(e),
        }
    }

    /// Entry point of the configuration
    pub fn from_cli_config(cli_config: Option<ConfigCli>) -> ConfigResult {
        match cli_config {
            Some(ConfigCli::Config { clean, .. }) if clean => Ok(Config::default()),
            Some(ConfigCli::Config { path, .. }) if path.is_some()=> Ok(Config::new(&path.unwrap())?),
            Some(_) | None => Ok(Config::from_default_path()?),
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
mod config_test {
    use super::*;

    #[test]
    fn no_config_file_equals_default_config() {
        let no_file = PathBuf::from(r"../fixtures/config/config.yamlll");
        let config = Config::from_option_or_default(Some(no_file)).unwrap();
        let default = Config::default();
        assert_eq!(config, default);
    }

    #[test]
    fn no_config_option_file_equals_default_config() {
        let config = Config::from_option_or_default(None).unwrap();
        let default = Config::default();
        assert_eq!(config, default);
    }
}
