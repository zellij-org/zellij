//! Deserializes configuration options.
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use crate::cli::ConfigCli;
use crate::common::install;

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
    // Internal Deserialisation Error
    FromUtf8(std::string::FromUtf8Error),
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

    /// Gets default configuration from assets
    // TODO Deserialize the Configuration from bytes &[u8],
    // once serde-yaml supports zero-copy
    pub fn from_default_assets() -> ConfigResult {
        Self::from_yaml(String::from_utf8(install::DEFAULT_CONFIG.to_vec())?.as_str())
    }

    /// Entry point of the configuration
    #[cfg(not(test))]
    pub fn from_cli_config(
        location: Option<PathBuf>,
        cli_config: Option<ConfigCli>,
        config_dir: Option<PathBuf>,
    ) -> ConfigResult {
        if let Some(path) = location {
            return Config::new(&path);
        }

        if let Some(ConfigCli::Config { clean, .. }) = cli_config {
            if clean {
                return Config::from_default_assets();
            }
        }

        if let Some(config) = config_dir {
            let path = config.join("config.yaml");
            if path.exists() {
                Config::new(&path)
            } else {
                Config::from_default_assets()
            }
        } else {
            Config::from_default_assets()
        }
    }

    /// In order not to mess up tests from changing configurations
    #[cfg(test)]
    pub fn from_cli_config(
        _: Option<PathBuf>,
        _: Option<ConfigCli>,
        _: Option<PathBuf>,
    ) -> ConfigResult {
        Ok(Config::default())
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
            ConfigError::FromUtf8(ref err) => write!(formatter, "FromUtf8Error: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            ConfigError::Io(ref err) => Some(err),
            ConfigError::IoPath(ref err, _) => Some(err),
            ConfigError::Serde(ref err) => Some(err),
            ConfigError::FromUtf8(ref err) => Some(err),
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

impl From<std::string::FromUtf8Error> for ConfigError {
    fn from(err: std::string::FromUtf8Error) -> ConfigError {
        ConfigError::FromUtf8(err)
    }
}

// The unit test location.
#[cfg(test)]
mod config_test {
    use super::*;

    #[test]
    fn clean_option_equals_default_config() {
        let cli_config = ConfigCli::Config { clean: true };
        let config = Config::from_cli_config(None, Some(cli_config), None).unwrap();
        let default = Config::default();
        assert_eq!(config, default);
    }

    #[test]
    fn no_config_option_file_equals_default_config() {
        let config = Config::from_cli_config(None, None, None).unwrap();
        let default = Config::default();
        assert_eq!(config, default);
    }
}
