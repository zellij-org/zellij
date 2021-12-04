//! Deserializes configuration options.
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;

use serde::{Deserialize, Serialize};
use std::convert::{TryFrom, TryInto};

use super::keybinds::{Keybinds, KeybindsFromYaml};
use super::options::Options;
use super::plugins::{PluginsConfig, PluginsConfigError, PluginsConfigFromYaml};
use super::theme::ThemesFromYaml;
use crate::cli::{CliArgs, Command};
use crate::setup;

const DEFAULT_CONFIG_FILE_NAME: &str = "config.yaml";

type ConfigResult = Result<Config, ConfigError>;

/// Intermediate deserialization config struct
#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
pub struct ConfigFromYaml {
    #[serde(flatten)]
    pub options: Option<Options>,
    pub keybinds: Option<KeybindsFromYaml>,
    pub themes: Option<ThemesFromYaml>,
    #[serde(default)]
    pub plugins: PluginsConfigFromYaml,
}

/// Main configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Config {
    pub keybinds: Keybinds,
    pub options: Options,
    pub themes: Option<ThemesFromYaml>,
    pub plugins: PluginsConfig,
}

#[derive(Error, Debug)]
pub enum ConfigError {
    // Deserialization error
    #[error("Deserialization error: {0}")]
    Serde(#[from] serde_yaml::Error),
    // Io error
    #[error("IoError: {0}")]
    Io(#[from] io::Error),
    // Io error with path context
    #[error("IoError: {0}, File: {1}")]
    IoPath(io::Error, PathBuf),
    // Internal Deserialization Error
    #[error("FromUtf8Error: {0}")]
    FromUtf8(#[from] std::string::FromUtf8Error),
    // Naming a part in a tab is unsupported
    #[error("There was an error in the layout file, {0}")]
    LayoutNameInTab(#[from] LayoutNameInTabError),
    // Plugins have a semantic error, usually trying to parse two of the same tag
    #[error("PluginsError: {0}")]
    PluginsError(#[from] PluginsConfigError),
}

impl Default for Config {
    fn default() -> Self {
        let keybinds = Keybinds::default();
        let options = Options::default();
        let themes = None;
        let plugins = PluginsConfig::default();

        Config {
            keybinds,
            options,
            themes,
            plugins,
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
        let maybe_config_from_yaml: Option<ConfigFromYaml> = match serde_yaml::from_str(yaml_config)
        {
            Err(e) => {
                // needs direct check, as `[ErrorImpl]` is private
                // https://github.com/dtolnay/serde-yaml/issues/121
                if yaml_config.is_empty() {
                    return Ok(Config::default());
                }
                return Err(ConfigError::Serde(e));
            }
            Ok(config) => config,
        };

        match maybe_config_from_yaml {
            None => Ok(Config::default()),
            Some(config) => config.try_into(),
        }
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
    // TODO Deserialize the Config from bytes &[u8],
    // once serde-yaml supports zero-copy
    pub fn from_default_assets() -> ConfigResult {
        let cfg = String::from_utf8(setup::DEFAULT_CONFIG.to_vec())?;
        Self::from_yaml(cfg.as_str())
    }

    /// Merges two Config structs into one Config struct
    /// `other` overrides `self`.
    pub fn merge(&self, other: Self) -> Self {
        Self {
            // TODO: merge keybinds in a way that preserves "unbind" attribute
            keybinds: self.keybinds.clone(),
            options: self.options.merge(other.options),
            themes: self.themes.clone(), // TODO
            plugins: self.plugins.merge(other.plugins),
        }
    }
}

impl TryFrom<ConfigFromYaml> for Config {
    type Error = ConfigError;

    fn try_from(config_from_yaml: ConfigFromYaml) -> ConfigResult {
        let keybinds = Keybinds::get_default_keybinds_with_config(config_from_yaml.keybinds);
        let options = Options::from_yaml(config_from_yaml.options);
        let themes = config_from_yaml.themes;
        let plugins = PluginsConfig::get_plugins_with_default(config_from_yaml.plugins.try_into()?);
        Ok(Self {
            keybinds,
            options,
            plugins,
            themes,
        })
    }
}

// TODO: Split errors up into separate modules
#[derive(Debug, Clone)]
pub struct LayoutNameInTabError;

impl fmt::Display for LayoutNameInTabError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "LayoutNameInTabError:
The `parts` inside the `tabs` can't be named. For example:
---
tabs:
  - direction: Vertical
    name: main
    parts:
      - direction: Vertical
        name: section # <== The part section can't be named.
      - direction: Vertical
  - direction: Vertical
    name: test
"
        )
    }
}

impl std::error::Error for LayoutNameInTabError {
    fn description(&self) -> &str {
        "The `parts` inside the `tabs` can't be named."
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
