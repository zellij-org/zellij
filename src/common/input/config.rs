//! Deserializes configuration options.
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use super::options::Options;
use crate::cli::{CliArgs, ConfigCli};
use crate::common::setup;

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

const DEFAULT_CONFIG_FILE_NAME: &str = "config.yaml";

type ConfigResult = Result<Config, ConfigError>;

/// Intermediate deserialization config struct
#[derive(Debug, Deserialize)]
pub struct ConfigFromYaml {
    #[serde(flatten)]
    pub options: Option<Options>,
    pub keybinds: Option<KeybindsFromYaml>,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub keybinds: Keybinds,
    pub options: Options,
}

#[derive(Debug)]
pub enum ConfigError {
    // Deserialization error
    Serde(serde_yaml::Error),
    // Io error
    Io(io::Error),
    // Io error with path context
    IoPath(io::Error, PathBuf),
    // Internal Deserialization Error
    FromUtf8(std::string::FromUtf8Error),
}

impl Default for Config {
    fn default() -> Self {
        let keybinds = Keybinds::default();
        let options = Options::default();
        Config { keybinds, options }
    }
}

impl TryFrom<&CliArgs> for Config {
    type Error = ConfigError;

    fn try_from(opts: &CliArgs) -> ConfigResult {
        if let Some(ref path) = opts.config {
            return Config::new(&path);
        }

        if let Some(ConfigCli::Setup { clean, .. }) = opts.option {
            if clean {
                return Config::from_default_assets();
            }
        }

        let config_dir = opts
            .config_dir
            .clone()
            .or_else(setup::find_default_config_dir);

        if let Some(ref config) = config_dir {
            let path = config.join(DEFAULT_CONFIG_FILE_NAME);
            if path.exists() {
                Config::new(&path)
            } else {
                Config::from_default_assets()
            }
        } else {
            Config::from_default_assets()
        }
    }
}

impl Config {
    /// Uses defaults, but lets config override them.
    pub fn from_yaml(yaml_config: &str) -> ConfigResult {
        let config_from_yaml: ConfigFromYaml = serde_yaml::from_str(&yaml_config)?;
        let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
        let options = Options::from_yaml(config_from_yaml.options);
        Ok(Config { keybinds, options })
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
        Self::from_yaml(String::from_utf8(setup::DEFAULT_CONFIG.to_vec())?.as_str())
    }
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConfigError::Io(ref err) => write!(formatter, "IoError: {}", err),
            ConfigError::IoPath(ref err, ref path) => {
                write!(formatter, "IoError: {}, File: {}", err, path.display(),)
            }
            ConfigError::Serde(ref err) => write!(formatter, "Deserialization error: {}", err),
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
    use std::io::Write;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn try_from_cli_args_with_config() {
        let arbitrary_config = PathBuf::from("nonexistent.yaml");
        let mut opts = CliArgs::default();
        opts.config = Some(arbitrary_config);
        println!("OPTS= {:?}", opts);
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_option_clean() {
        let mut opts = CliArgs::default();
        opts.option = Some(ConfigCli::Config { clean: true });
        let result = Config::try_from(&opts);
        assert!(result.is_ok());
    }

    #[test]
    fn try_from_cli_args_with_config_dir() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        File::create(tmp.path().join(DEFAULT_CONFIG_FILE_NAME))
            .unwrap()
            .write_all(b"keybinds: invalid\n")
            .unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_config_dir_without_config() {
        let mut opts = CliArgs::default();
        let tmp = tempdir().unwrap();
        opts.config_dir = Some(tmp.path().to_path_buf());
        let result = Config::try_from(&opts);
        assert_eq!(result.unwrap(), Config::default());
    }

    #[test]
    fn try_from_cli_args_default() {
        let opts = CliArgs::default();
        let result = Config::try_from(&opts);
        assert_eq!(result.unwrap(), Config::default());
    }
}
