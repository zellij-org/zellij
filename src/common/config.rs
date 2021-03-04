//! Deserializes configuration options.
use std;
use std::collections::HashMap;
use std::error;
use std::fmt::{self, Display};
use std::fs::File;
use std::io::{self,Read};
use std::path::PathBuf;

use super::input::{keybinds,handler};

use serde::Deserialize;

/// Intermediate struct
//pub struct KeybingsFromYaml {

//}


/// Intermediate struct
//#[derive(Debug, Deserialize)]
pub struct ConfigFromYaml {
    keybinds: HashMap<handler::InputMode,Vec<keybinds::Keybinds>>,
}

///// Deserialized config state
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    keybinds: Vec<keybinds::Keybinds>,
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
    Io(io::Error)
}

impl Config {
    /// Deserializes from given path
    pub fn new(path: &PathBuf) -> Result<Config,ConfigError> {
        let config_deserialized: Config;
        let mut config_string = String::new();

        match File::open(path) {
            Ok(mut file) => {
                file.read_to_string(&mut config_string)?;
                config_deserialized = serde_yaml::from_str(&config_string)?;
            }
            Err(_) => {
                // TODO logging, if a file is not found
                // at an expected position - should not
                // panic @a-kenji
                config_deserialized = Config::default();
            }
        }
            Ok(config_deserialized)
    }
}

//impl de::Error for ConfigError {
    //fn custom<T: Display>(msg: T) -> Self {
        //ConfigError::Message(msg.to_string())
    //}
//}

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
    fn description(&self) -> &str {
        match *self {
            //ConfigError::Message(ref err) => err,
            ConfigError::Io(ref err) => err.to_string().as_str(),
            ConfigError::Serde(ref err) => err.to_string().as_str(),
        }
    }

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

//impl From<de::Error::Message> for ConfigError {
    //fn from(err: de::Error::Message) -> ConfigError {
        //ConfigError::Message(err)
    //}
//}

