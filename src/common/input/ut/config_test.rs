//! For Configuration Options

use super::super::config::*;
use crate::common::input::keybinds::*;
use crate::common::input::actions::*;
use std::path::PathBuf;

use termion::event::Key;


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

#[test]
fn multiple_keys_mapped_to_one_action() {
    let options = r"
    ---
keybindings:
    Normal:
    - ? - F: 6
        - F: 7
        - F: 8
      : - {GoToTab: 5}
      ";

    let config_options = Config::from_yaml(&options).unwrap();

    assert_eq!(config_options, config_options)
}

//#[test]
//fn merge_keybinds_merges(){
    //let mut self_keybinds = Keybinds::new();
    //let mut self_mode_keybinds = ModeKeybinds::new();
    //self_mode_keybinds.0.insert(Key::F(1), vec![Action::GoToTab(5)]);
    //let mut other_keybinds = Keybinds::new();
    //let mut self_mode_keybinds = ModeKeybinds::new();
    //let mut expected = Keybinds::new();
//}
