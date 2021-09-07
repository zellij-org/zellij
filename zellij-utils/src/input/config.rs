//! Deserializes configuration options.
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use super::options::Options;
use super::theme::ThemesFromYaml;
use crate::cli::{CliArgs, Command};
use crate::setup;

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
    pub themes: Option<ThemesFromYaml>,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Config {
    pub keybinds: Keybinds,
    pub options: Options,
    pub themes: Option<ThemesFromYaml>,
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
    // Missing the tab section in the layout.
    Layout(LayoutMissingTabSectionError),
    LayoutPartAndTab(LayoutPartAndTabError),
}

impl Default for Config {
    fn default() -> Self {
        let keybinds = Keybinds::default();
        let options = Options::default();
        let themes = None;

        Config {
            keybinds,
            options,
            themes,
        }
    }
}

impl TryFrom<&CliArgs> for Config {
    type Error = ConfigError;

    fn try_from(opts: &CliArgs) -> ConfigResult {
        if let Some(ref path) = opts.config {
            return Config::new(path);
        }

        if let Some(Command::Setup(ref setup)) = opts.command {
            if setup.clean {
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
        let config_from_yaml: ConfigFromYaml = serde_yaml::from_str(yaml_config)?;
        let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
        let options = Options::from_yaml(config_from_yaml.options);
        let themes = config_from_yaml.themes;

        Ok(Config {
            keybinds,
            options,
            themes,
        })
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

// TODO: Split errors up into separate modules
#[derive(Debug, Clone)]
pub struct LayoutMissingTabSectionError;
#[derive(Debug, Clone)]
pub struct LayoutPartAndTabError;

impl fmt::Display for LayoutMissingTabSectionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MissingTabSectionError:
There needs to be exactly one `tabs` section specified in the layout file, for example:
---
direction: Horizontal
parts:
  - direction: Vertical
  - direction: Vertical
    tabs:
      - direction: Vertical
      - direction: Vertical
  - direction: Vertical
"
        )
    }
}

impl std::error::Error for LayoutMissingTabSectionError {
    fn description(&self) -> &str {
        "One tab must be specified per Layout."
    }
}

impl fmt::Display for LayoutPartAndTabError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LayoutPartAndTabError:
The `tabs` and `parts` section should not be specified on the same level in the layout file, for example:
---
direction: Horizontal
parts:
  - direction: Vertical
  - direction: Vertical
tabs:
  - direction: Vertical
  - direction: Vertical
  - direction: Vertical

should rather be specified as:
---
direction: Horizontal
parts:
  - direction: Vertical
  - direction: Vertical
    tabs:
      - direction: Vertical
      - direction: Vertical
      - direction: Vertical
"
        )
    }
}

impl std::error::Error for LayoutPartAndTabError {
    fn description(&self) -> &str {
        "The `tabs` and parts section should not be specified on the same level."
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
            ConfigError::Layout(ref err) => {
                write!(formatter, "There was an error in the layout file, {}", err)
            }
            ConfigError::LayoutPartAndTab(ref err) => {
                write!(formatter, "There was an error in the layout file, {}", err)
            }
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
            ConfigError::Layout(ref err) => Some(err),
            ConfigError::LayoutPartAndTab(ref err) => Some(err),
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

impl From<LayoutMissingTabSectionError> for ConfigError {
    fn from(err: LayoutMissingTabSectionError) -> ConfigError {
        ConfigError::Layout(err)
    }
}

impl From<LayoutPartAndTabError> for ConfigError {
    fn from(err: LayoutPartAndTabError) -> ConfigError {
        ConfigError::LayoutPartAndTab(err)
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
        let opts = CliArgs {
            config: Some(arbitrary_config),
            ..Default::default()
        };
        println!("OPTS= {:?}", opts);
        let result = Config::try_from(&opts);
        assert!(result.is_err());
    }

    #[test]
    fn try_from_cli_args_with_option_clean() {
        use crate::setup::Setup;
        let opts = CliArgs {
            command: Some(Command::Setup(Setup {
                clean: true,
                ..Setup::default()
            })),
            ..Default::default()
        };
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
