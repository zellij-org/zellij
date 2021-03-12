//! For Configuration Options

use super::super::config::*;
use std::path::PathBuf;

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
