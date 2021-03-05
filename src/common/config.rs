//! Deserializes configuration options.
use std;
//use std::collections::HashMap;
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;

use super::input::{keybinds, macros};

use directories_next::ProjectDirs;
use serde::Deserialize;

/// Intermediate struct
//pub struct KeybingsFromYaml {

//}

/// Intermediate struct
#[derive(Debug, Deserialize)]
pub struct ConfigFromYaml {
    keybinds: Option<keybinds::Keybinds>,
    macros: Option<Vec<macros::Macro>>,
}

///// Deserialized config state
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    pub keybinds: keybinds::Keybinds,
}

#[derive(Debug)]
pub enum ConfigError {
    // from the serde documentation
    // https://serde.rs/error-handling.html
    // One or more variants that can be created by data structures through the
    // `ser::Error` and `de::Error` traits. For example the Serialize impl for
    // Mutex<T> might return an error because the mutex is poisoned, or the
    // Deserialize impl for a struct may return an error because a required
    // field is missing.
    //Message(String),
    // serde_yaml error
    Serde(serde_yaml::Error),
    //Eof,
    // io::Error
    Io(io::Error),
}

impl Config {
    /// Deserializes from given path
    pub fn new(path: &PathBuf) -> Result<Config, ConfigError> {
        let config: Config;
        let config_deserialized: ConfigFromYaml;
        let mut config_string = String::new();

        // TODO fix this unwrap
        match File::open(path) {
            Ok(mut file) => {
                file.read_to_string(&mut config_string)?;
                config_deserialized = serde_yaml::from_str(&config_string)?;
                config = Config {
                    keybinds: config_deserialized
                        .keybinds
                        .unwrap_or_else(|| keybinds::get_default_keybinds().unwrap()),
                }
            }
            Err(e) => {
                // TODO logging, if a file is not found
                // at an expected position - should not
                // panic @a-kenji
                eprintln!("{}", e);
                config = Config::default();
            }
        }
        Ok(config)
    }

    pub fn from_option_or_default(option: Option<PathBuf>) -> Result<Config, ConfigError> {
        let config;
        if let Some(config_path) = option {
            config = Config::new(&config_path)?;
        } else {
        let project_dirs = ProjectDirs::from("org", "Zellij Contributors", "Zellij").unwrap();
        //let config_path = PathBuf::from(project_dirs.config_dir().as_os_str());
        let config_path = project_dirs.config_dir().to_owned().into();
            config = Config::new(&config_path)?;
        }
        return Ok(config);
    }
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        match self {
            //ConfigError::Message(msg) => formatter.write_str(msg),
            //ConfigError::Eof => formatter.write_str("unexpected end of input"),
            //
            ConfigError::Io(ref err) => write!(formatter, "Io error: {}", err),
            ConfigError::Serde(ref err) => write!(formatter, "Serde error: {}", err),
            /* and so forth */
        }
    }
}

impl std::error::Error for ConfigError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            // N.B. Both of these implicitly cast `err` from their concrete
            // types (either `&io::Error` or `&num::ParseIntError`)
            // to a trait object `&Error`. This works because both error types
            // implement `Error`.
            ConfigError::Io(ref err) => Some(err),
            //ConfigError::Message(ref err) => Some(err),
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
