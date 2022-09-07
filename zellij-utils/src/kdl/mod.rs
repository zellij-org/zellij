mod kdl_layout_parser;
use kdl_layout_parser::KdlLayoutParser;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;
use crate::envs::EnvironmentVariables;
use crate::input::command::RunCommand;
use crate::input::keybinds::Keybinds;
use crate::input::layout::{Layout, RunPlugin, RunPluginLocation};
use crate::input::config::{Config, ConfigError};
use url::Url;
use crate::data::{InputMode, Key, CharOrArrow, PaletteColor, Palette};
use crate::input::options::{Options, OnForceClose, Clipboard};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use crate::input::plugins::{PluginsConfig, PluginsConfigError, PluginConfig, PluginType, PluginTag};
use crate::input::theme::{UiConfig, Theme, Themes, FrameConfig};
use crate::cli::{CliArgs, Command};
use crate::setup;

use kdl::{KdlDocument, KdlValue, KdlNode};

use std::str::FromStr;
use std::path::PathBuf;

use crate::input::actions::{Action, ResizeDirection, Direction, SearchOption, SearchDirection};
use crate::input::command::RunCommandAction;

#[macro_export]
macro_rules! parse_kdl_action_arguments {
    ( $action_name:expr, $action_arguments:expr ) => {
        {
            if !$action_arguments.is_empty() {
                Err(format!("Failed to parse action: {}", $action_name).into())
            } else {
                match $action_name {
                    "Quit" => Ok(Action::Quit),
                    "FocusNextPane" => Ok(Action::FocusNextPane),
                    "FocusPreviousPane" => Ok(Action::FocusPreviousPane),
                    "SwitchFocus" => Ok(Action::SwitchFocus),
                    "EditScrollback" => Ok(Action::EditScrollback),
                    "ScrollUp" => Ok(Action::ScrollUp),
                    "ScrollDown" => Ok(Action::ScrollDown),
                    "ScrollToBottom" => Ok(Action::ScrollToBottom),
                    "PageScrollUp" => Ok(Action::PageScrollUp),
                    "PageScrollDown" => Ok(Action::PageScrollDown),
                    "HalfPageScrollUp" => Ok(Action::HalfPageScrollUp),
                    "HalfPageScrollDown" => Ok(Action::HalfPageScrollDown),
                    "ToggleFocusFullscreen" => Ok(Action::ToggleFocusFullscreen),
                    "TogglePaneFrames" => Ok(Action::TogglePaneFrames),
                    "ToggleActiveSyncTab" => Ok(Action::ToggleActiveSyncTab),
                    "TogglePaneEmbedOrFloating" => Ok(Action::TogglePaneEmbedOrFloating),
                    "ToggleFloatingPanes" => Ok(Action::ToggleFloatingPanes),
                    "CloseFocus" => Ok(Action::CloseFocus),
                    "UndoRenamePane" => Ok(Action::UndoRenamePane),
                    "NoOp" => Ok(Action::NoOp),
                    "GoToNextTab" => Ok(Action::GoToNextTab),
                    "GoToPreviousTab" => Ok(Action::GoToPreviousTab),
                    "CloseTab" => Ok(Action::CloseTab),
                    "ToggleTab" => Ok(Action::ToggleTab),
                    "UndoRenameTab" => Ok(Action::UndoRenameTab),
                    "Detach" => Ok(Action::Detach),
                    "Copy" => Ok(Action::Copy),
                    "Confirm" => Ok(Action::Confirm),
                    "Deny" => Ok(Action::Deny),
                    _ => Err(format!("Error parsing enum variant: {:?}", $action_name).into())
                }
            }
        }
    };
}

#[macro_export]
macro_rules! parse_kdl_action_u8_arguments {
    ( $action_name:expr, $action_arguments:expr ) => {{
        let mut bytes = vec![];
        for kdl_value in $action_arguments.iter() {
            match kdl_value.as_i64() {
                Some(int_value) => bytes.push(int_value as u8),
                None => {
                    return Err(format!("Failed to parse action: {}", $action_name).into());
                }
            }
        };
        Action::new_from_bytes($action_name, bytes)
    }}
}
#[macro_export]
macro_rules! kdl_entries_as_i64 {
    ( $node:expr ) => {
        $node.entries().iter().map(|kdl_node| kdl_node.value().as_i64())
    }
}

#[macro_export]
macro_rules! kdl_first_entry_as_string {
    ( $node:expr ) => {
        $node.entries().iter().next().and_then(|s| s.value().as_string())
    }
}

#[macro_export]
macro_rules! kdl_first_entry_as_i64 {
    ( $node:expr ) => {
        $node.entries().iter().next().and_then(|i| i.value().as_i64())
    }
}

#[macro_export]
macro_rules! entry_count {
    ( $node:expr ) => {{
        $node.entries().iter().len()
    }}
}

#[macro_export]
macro_rules! parse_kdl_action_char_or_string_arguments {
    ( $action_name:expr, $action_arguments:expr ) => {{
        let mut chars_to_write = String::new();
        for kdl_value in $action_arguments.iter() {
            match kdl_value.as_string() {
                Some(string_value) => chars_to_write.push_str(string_value),
                None => {
                    return Err(format!("Failed to parse action: {}", $action_name).into());
                }
            }
        };
        Action::new_from_string($action_name, chars_to_write)
    }}
}

#[macro_export]
macro_rules! kdl_arg_is_truthy {
    ( $kdl_node:expr, $arg_name:expr ) => {
        $kdl_node.get($arg_name).and_then(|c| c.value().as_bool()).unwrap_or(false)
    }
}

#[macro_export]
macro_rules! kdl_children_nodes_or_error {
    ( $kdl_node:expr, $error:expr ) => {
        $kdl_node.children().ok_or(ConfigError::KdlParsingError($error.into()))?.nodes()
    }
}

#[macro_export]
macro_rules! kdl_children_nodes {
    ( $kdl_node:expr ) => {
        $kdl_node.children().map(|c| c.nodes())
    }
}

#[macro_export]
macro_rules! kdl_children_or_error {
    ( $kdl_node:expr, $error:expr ) => {
        $kdl_node.children().ok_or(ConfigError::KdlParsingError($error.into()))?
    }
}

#[macro_export]
macro_rules! kdl_children {
    ( $kdl_node:expr ) => {
        $kdl_node.children().iter().copied().collect()
    }
}

#[macro_export]
macro_rules! kdl_string_arguments {
    ( $kdl_node:expr ) => {{
        let res: Result<Vec<_>, _> = $kdl_node.entries().iter().map(|e| e.value().as_string().ok_or(ConfigError::KdlParsingError("Not a string".into()))).collect();
        res?
    }}
}

#[macro_export]
macro_rules! kdl_argument_values {
    ( $kdl_node:expr ) => {
        $kdl_node.entries().iter().map(|arg| arg.value()).collect()
    }
}

#[macro_export]
macro_rules! kdl_name {
    ( $kdl_node:expr ) => {
        $kdl_node.name().value()
    }
}

#[macro_export]
macro_rules! kdl_document_name {
    ( $kdl_node:expr ) => {
        $kdl_node.node().name().value()
    }
}

#[macro_export]
macro_rules! keys_from_kdl {
    ( $kdl_node:expr ) => {
        kdl_string_arguments!($kdl_node)
            .iter()
            .map(|k| Key::from_str(k))
            .collect::<Result<_, _>>()?
    }
}

#[macro_export]
macro_rules! actions_from_kdl {
    ( $kdl_node:expr ) => {
        kdl_children_nodes_or_error!($kdl_node, "no actions found for key_block")
            .iter()
            .map(|kdl_action| Action::try_from(kdl_action))
            .collect::<Result<_, _>>()?
    }
}


pub fn kdl_arguments_that_are_strings <'a>(arguments: impl Iterator<Item=&'a KdlValue>) -> Result<Vec<String>, String> {
    let mut args: Vec<String> = vec![];
    for kdl_value in arguments {
        match kdl_value.as_string() {
            Some(string_value) => args.push(string_value.to_string()),
            None => {
                return Err(format!("Failed to parse kdl arguments"));
            }
        }
    }
    Ok(args)
}

pub fn kdl_child_string_value_for_entry <'a>(command_metadata: &'a KdlDocument, entry_name: &'a str) -> Option<&'a str> {
    command_metadata
        .get(entry_name)
        .and_then(|cwd| cwd.entries().iter().next())
        .and_then(|cwd_value| cwd_value.value().as_string())
}

impl Action {
    pub fn new_from_bytes(action_name: &str, bytes: Vec<u8>) -> Result<Self, Box<dyn std::error::Error>> {
        match action_name {
            "Write" => {
                Ok(Action::Write(bytes))
            },
            "PaneNameInput" => {
                Ok(Action::PaneNameInput(bytes))
            }
            "TabNameInput" => {
                Ok(Action::TabNameInput(bytes))
            }
            "GoToTab" => {
                let tab_index = *bytes
                    .get(0)
                    .ok_or_else(|| format!("Cannot create action: {} from bytes: {:?}", action_name, bytes))?
                    as u32;
                Ok(Action::GoToTab(tab_index))
            }
            _ => Err(format!("Cannot create action: {} from bytes: {:?}", action_name, bytes).into()),
        }
    }
    pub fn new_from_string(action_name: &str, string: String) -> Result<Self, Box<dyn std::error::Error>> {
        match action_name {
            "WriteChars" => Ok(Action::WriteChars(string)),
            "SwitchToMode" => {
                match InputMode::from_str(string.as_str()) {
                    Ok(input_mode) => Ok(Action::SwitchToMode(input_mode)),
                    Err(_e) => return Err(format!("Failed to parse SwitchToMode. Unknown InputMode: {}", string).into()),
                }
            },
            "Resize" => {
                let direction = ResizeDirection::from_str(string.as_str())?;
                Ok(Action::Resize(direction))
            }
            "MoveFocus" => {
                let direction = Direction::from_str(string.as_str())?;
                Ok(Action::MoveFocus(direction))
            }
            "MoveFocusOrTab" => {
                let direction = Direction::from_str(string.as_str())?;
                Ok(Action::MoveFocusOrTab(direction))
            }
            "MovePane" => {
                if string.is_empty() {
                    return Ok(Action::MovePane(None));
                } else {
                    let direction = Direction::from_str(string.as_str())?;
                    Ok(Action::MovePane(Some(direction)))
                }
            }
            "DumpScreen" => {
                Ok(Action::DumpScreen(string))
            }
            "NewPane" => {
                if string.is_empty() {
                    return Ok(Action::NewPane(None));
                } else {
                    let direction = Direction::from_str(string.as_str())?;
                    Ok(Action::NewPane(Some(direction)))
                }
            }
            "SearchToggleOption" => {
                let toggle_option = SearchOption::from_str(string.as_str())?;
                Ok(Action::SearchToggleOption(toggle_option))
            }
            "Search" => {
                let search_direction = SearchDirection::from_str(string.as_str())?;
                Ok(Action::Search(search_direction))
            }
            _ => Err(format!("Cannot create action: '{}' from string: '{:?}'", action_name, string).into()),
        }
    }
}

impl TryFrom<(&str, &KdlDocument)> for PaletteColor {
    type Error = Box<dyn std::error::Error>;

    fn try_from((color_name, theme_colors): (&str, &KdlDocument)) -> Result<PaletteColor, Self::Error> {
        let color = theme_colors.get(color_name).ok_or(format!("Failed to parse color"))?;
        let entry_count = entry_count!(color);
        let is_rgb = || entry_count == 3;
        let is_three_digit_hex = || {
            match kdl_first_entry_as_string!(color) {
                // 4 including the '#' character
                Some(s) => entry_count == 1 && s.starts_with('#') && s.len() == 4,
                None => false
            }
        };
        let is_six_digit_hex = || {
            match kdl_first_entry_as_string!(color) {
                // 7 including the '#' character
                Some(s) => entry_count == 1 && s.starts_with('#') && s.len() == 7,
                None => false,
            }
        };
        let is_eight_bit = || {
            kdl_first_entry_as_i64!(color).is_some() && entry_count == 1
        };
        if is_rgb() {
            let mut channels = kdl_entries_as_i64!(color);
            let r = channels.next().unwrap().ok_or(format!("invalid color"))? as u8;
            let g = channels.next().unwrap().ok_or(format!("invalid_color"))? as u8;
            let b = channels.next().unwrap().ok_or(format!("invalid_color"))? as u8;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_three_digit_hex() {
            // eg. #fff (hex, will be converted to rgb)
            let mut s = String::from(kdl_first_entry_as_string!(color).unwrap());
            s.remove(0);
            // TODO: test this
            // TODO: why do we need the * 0x11 here?
            let r = u8::from_str_radix(&s[0..1], 16).map_err(|e| format!("Failed to parse color: {}", e))? * 0x11;
            let g = u8::from_str_radix(&s[1..2], 16).map_err(|e| format!("Failed to parse color: {}", e))? * 0x11;
            let b = u8::from_str_radix(&s[2..3], 16).map_err(|e| format!("Failed to parse color: {}", e))? * 0x11;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_six_digit_hex() {
            // eg. #ffffff (hex, will be converted to rgb)
            let mut s = String::from(kdl_first_entry_as_string!(color).unwrap());
            s.remove(0);
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|e| format!("Failed to parse color: {}", e))?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|e| format!("Failed to parse color: {}", e))?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|e| format!("Failed to parse color: {}", e))?;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_eight_bit() {
            let n = kdl_first_entry_as_i64!(color).ok_or(format!("Failed to parse color"))?;
            Ok(PaletteColor::EightBit(n as u8)) // TODO: test values greater than u8 bounds
        } else {
            Err("Failed to parse color".into())
        }
    }
}

impl TryFrom<&KdlNode> for Action {
    type Error = Box<dyn std::error::Error>;
    fn try_from(kdl_action: &KdlNode) -> Result<Self, Self::Error> {

        let action_name = kdl_name!(kdl_action);
        let action_arguments: Vec<&KdlValue> = kdl_argument_values!(kdl_action);
        let action_children: Vec<&KdlDocument> = kdl_children!(kdl_action);
        match action_name {
            "Quit" => parse_kdl_action_arguments!(action_name, action_arguments),
            "FocusNextPane" => parse_kdl_action_arguments!(action_name, action_arguments),
            "FocusPreviousPane" => parse_kdl_action_arguments!(action_name, action_arguments),
            "SwitchFocus" => parse_kdl_action_arguments!(action_name, action_arguments),
            "EditScrollback" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ScrollUp" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ScrollDown" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ScrollToBottom" => parse_kdl_action_arguments!(action_name, action_arguments),
            "PageScrollUp" => parse_kdl_action_arguments!(action_name, action_arguments),
            "PageScrollDown" => parse_kdl_action_arguments!(action_name, action_arguments),
            "HalfPageScrollUp" => parse_kdl_action_arguments!(action_name, action_arguments),
            "HalfPageScrollDown" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ToggleFocusFullscreen" => parse_kdl_action_arguments!(action_name, action_arguments),
            "TogglePaneFrames" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ToggleActiveSyncTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "TogglePaneEmbedOrFloating" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ToggleFloatingPanes" => parse_kdl_action_arguments!(action_name, action_arguments),
            "CloseFocus" => parse_kdl_action_arguments!(action_name, action_arguments),
            "UndoRenamePane" => parse_kdl_action_arguments!(action_name, action_arguments),
            "NoOp" => parse_kdl_action_arguments!(action_name, action_arguments),
            "GoToNextTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "GoToPreviousTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "CloseTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "ToggleTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "UndoRenameTab" => parse_kdl_action_arguments!(action_name, action_arguments),
            "Detach" => parse_kdl_action_arguments!(action_name, action_arguments),
            "Copy" => parse_kdl_action_arguments!(action_name, action_arguments),
            "Confirm" => parse_kdl_action_arguments!(action_name, action_arguments),
            "Deny" => parse_kdl_action_arguments!(action_name, action_arguments),
            "Write" => parse_kdl_action_u8_arguments!(action_name, action_arguments),
            "WriteChars" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "SwitchToMode" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "Search" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "Resize" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "MoveFocus" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "MoveFocusOrTab" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "MovePane" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "DumpScreen" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "NewPane" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "PaneNameInput" => parse_kdl_action_u8_arguments!(action_name, action_arguments),
            "NewTab" => Ok(Action::NewTab(None, None)), // TODO: consider the Some(TabLayout, "tab_name") case...
            "GoToTab" => parse_kdl_action_u8_arguments!(action_name, action_arguments),
            "TabNameInput" => parse_kdl_action_u8_arguments!(action_name, action_arguments),
            "SearchToggleOption" => parse_kdl_action_char_or_string_arguments!(action_name, action_arguments),
            "Run" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_strings(arguments)?;
                if args.is_empty() {
                    return Err("No command found in Run action".into());
                }
                let command = args.remove(0);
                let command_metadata = action_children.iter().next();
                let cwd = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "cwd"))
                    .map(|cwd_string| PathBuf::from(cwd_string));
                let direction = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "direction"))
                    .and_then(|direction_string| Direction::from_str(direction_string).ok());
                let run_command_action = RunCommandAction {
                    command: PathBuf::from(command),
                    args,
                    cwd,
                    direction,
                };
                Ok(Action::Run(run_command_action))
            }
            _ => {
                Err(format!("Failed to parse action: {}", action_name).into())
            }
        }
    }
}

impl TryFrom<&KdlValue> for Key {
    type Error = String;
    fn try_from(kdl_value: &KdlValue) -> Result<Self, String> {
        let key_str = kdl_value.as_string();
        if key_str.is_none() {
            return Err(format!("Failed to parse key: {}", kdl_value));
        }
        let key_str = key_str.unwrap();
        let mut modifier: Option<&str> = None;
        let mut main_key: Option<&str> = None;
        for (index, part) in key_str.split_ascii_whitespace().enumerate() {
            // TODO: handle F(u8)
            if index == 0 && (part == "Ctrl" || part == "Alt") {
                modifier = Some(part);
            } else if main_key.is_none() {
                main_key = Some(part)
            }
        }
        match (modifier, main_key) {
            (Some("Ctrl"), Some(main_key)) => {
                let mut key_chars = main_key.chars();
                let key_count = main_key.chars().count();
                if key_count == 1 {
                    let key_char = key_chars.next().unwrap();
                    Ok(Key::Ctrl(key_char))
                } else {
                    Err(format!("Failed to parse key: {}", key_str))
                }
            },
            (Some("Alt"), Some(main_key)) => {
                match main_key {
                    // why crate::data::Direction and not just Direction?
                    // Because it's a different type that we export in this wasm mandated soup - we
                    // don't like it either! This will be solved as we chip away at our tech-debt
                    "Left" => Ok(Key::Alt(CharOrArrow::Direction(crate::data::Direction::Left))),
                    "Right" => Ok(Key::Alt(CharOrArrow::Direction(crate::data::Direction::Right))),
                    "Up" => Ok(Key::Alt(CharOrArrow::Direction(crate::data::Direction::Up))),
                    "Down" => Ok(Key::Alt(CharOrArrow::Direction(crate::data::Direction::Down))),
                    _ => {
                        let mut key_chars = main_key.chars();
                        let key_count = main_key.chars().count();
                        if key_count == 1 {
                            let key_char = key_chars.next().unwrap();
                            Ok(Key::Alt(CharOrArrow::Char(key_char)))
                        } else {
                            Err(format!("Failed to parse key: {}", key_str))
                        }
                    }
                }
            },
            (None, Some(main_key)) => {
                match main_key {
                    "Backspace" => Ok(Key::Backspace),
                    "Left" => Ok(Key::Left),
                    "Right" => Ok(Key::Right),
                    "Up" => Ok(Key::Up),
                    "Down" => Ok(Key::Down),
                    "Home" => Ok(Key::Home),
                    "End" => Ok(Key::End),
                    "PageUp" => Ok(Key::PageUp),
                    "PageDown" => Ok(Key::PageDown),
                    "Tab" => Ok(Key::BackTab),
                    "Delete" => Ok(Key::Delete),
                    "Insert" => Ok(Key::Insert),
                    "Space" => Ok(Key::Char(' ')),
                    "Enter" => Ok(Key::Char('\n')),
                    "Esc" => Ok(Key::Esc),
                    _ => {
                        let mut key_chars = main_key.chars();
                        let key_count = main_key.chars().count();
                        if key_count == 1 {
                            let key_char = key_chars.next().unwrap();
                            Ok(Key::Char(key_char))
                        } else if key_count > 1 {
                            if let Some(first_char) = key_chars.next() {
                                if first_char == 'F' {
                                    let f_index: String = key_chars.collect();
                                    let f_index: u8 = f_index.parse().map_err(|e| format!("Failed to parse F index: {}", e))?;
                                    if f_index >= 1 && f_index <= 12 {
                                        return Ok(Key::F(f_index));
                                    }
                                }
                            }
                            Err(format!("Failed to parse key: {}", key_str))
                        } else {
                            Err(format!("Failed to parse key: {}", key_str))
                        }
                    }
                }
            }
            _ => Err(format!("Failed to parse key: {}", key_str))
        }
    }
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_string {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node.get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_string())
    }
}

#[macro_export]
macro_rules! kdl_has_string_argument {
    ( $kdl_node:expr, $string_argument:expr ) => {
        $kdl_node.entries().iter().find(|e| e.value().as_string() == Some($string_argument)).is_some()
    }
}

#[macro_export]
macro_rules! kdl_children_property_first_arg_as_string {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node.children()
            .and_then(|c| c.get($property_name))
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_string())
    }
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_bool {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node.get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_bool())
    }
}

#[macro_export]
macro_rules! kdl_children_property_first_arg_as_bool {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node.children()
            .and_then(|c| c.get($property_name))
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_bool())
    }
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_i64 {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node.get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_i64())
    }
}

#[macro_export]
macro_rules! kdl_get_child {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node.children()
            .and_then(|c| c.get($child_name))
    }
}

#[macro_export]
macro_rules! kdl_get_child_entry_bool_value {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node.children()
            .and_then(|c| c.get($child_name))
            .and_then(|c| c.get(0))
            .and_then(|c| c.value().as_bool())
    }
}

#[macro_export]
macro_rules! kdl_get_child_entry_string_value {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node.children()
            .and_then(|c| c.get($child_name))
            .and_then(|c| c.get(0))
            .and_then(|c| c.value().as_string())
    }
}

#[macro_export]
macro_rules! kdl_get_bool_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node.get($name)
            .and_then(|e| e.value().as_bool())
            .or_else(|| $kdl_node.children()
                .and_then(|c| c.get($name))
                .and_then(|c| c.get(0))
                .and_then(|c| c.value().as_bool())
            )
    }
}

#[macro_export]
macro_rules! kdl_get_string_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node.get($name)
            .and_then(|e| e.value().as_string())
            .or_else(|| $kdl_node.children()
                .and_then(|c| c.get($name))
                .and_then(|c| c.get(0))
                .and_then(|c| c.value().as_string())
            )
    }
}

#[macro_export]
macro_rules! kdl_get_int_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node.get($name)
            .and_then(|e| e.value().as_i64())
            .or_else(|| $kdl_node.children()
                .and_then(|c| c.get($name))
                .and_then(|c| c.get(0))
                .and_then(|c| c.value().as_i64())
            )
    }
}

#[macro_export]
macro_rules! kdl_get_string_entry {
    ( $kdl_node:expr, $entry_name:expr ) => {
        $kdl_node.get($entry_name)
            .and_then(|e| e.value().as_string())
    }
}

#[macro_export]
macro_rules! kdl_get_int_entry {
    ( $kdl_node:expr, $entry_name:expr ) => {
        $kdl_node.get($entry_name)
            .and_then(|e| e.value().as_i64())
    }
}


impl Options {
    pub fn from_kdl(kdl_options: &KdlDocument) -> Self {
        let on_force_close = kdl_property_first_arg_as_string!(kdl_options, "on_force_close")
            .and_then(|arg| OnForceClose::from_str(arg).ok());
        let simplified_ui = kdl_property_first_arg_as_bool!(kdl_options, "simplified_ui");
        let default_shell = kdl_property_first_arg_as_string!(kdl_options, "default_shell")
            .map(|default_shell| PathBuf::from(default_shell));
        let pane_frames = kdl_property_first_arg_as_bool!(kdl_options, "pane_frames");
        let theme = kdl_property_first_arg_as_string!(kdl_options, "theme")
            .map(|theme| theme.to_string());
        let default_mode = kdl_property_first_arg_as_string!(kdl_options, "default_mode")
            .and_then(|default_mode| InputMode::from_str(default_mode).ok());
        let default_layout = kdl_property_first_arg_as_string!(kdl_options, "default_layout")
            .map(|default_layout| PathBuf::from(default_layout));
        let layout_dir = kdl_property_first_arg_as_string!(kdl_options, "layout_dir")
            .map(|layout_dir| PathBuf::from(layout_dir));
        let theme_dir = kdl_property_first_arg_as_string!(kdl_options, "theme_dir")
            .map(|theme_dir| PathBuf::from(theme_dir));
        let mouse_mode = kdl_property_first_arg_as_bool!(kdl_options, "mouse_mode");
        let scroll_buffer_size = kdl_property_first_arg_as_i64!(kdl_options, "scroll_buffer_size")
            .map(|scroll_buffer_size| scroll_buffer_size as usize);
        let copy_command = kdl_property_first_arg_as_string!(kdl_options, "copy_command")
            .map(|copy_command| copy_command.to_string());
        let copy_clipboard = kdl_property_first_arg_as_string!(kdl_options, "copy_clipboard")
            .and_then(|on_force_close| Clipboard::from_str(on_force_close).ok());
        let copy_on_select = kdl_property_first_arg_as_bool!(kdl_options, "copy_on_select");
        let scrollback_editor = kdl_property_first_arg_as_string!(kdl_options, "scrollback_editor")
            .map(|scrollback_editor| PathBuf::from(scrollback_editor));
        let mirror_session = kdl_property_first_arg_as_bool!(kdl_options, "mirror_session");
        let session_name = kdl_property_first_arg_as_string!(kdl_options, "session_name").map(|s| s.into());
        let attach_to_session = kdl_property_first_arg_as_bool!(kdl_options, "attach_to_session");
        Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            default_layout,
            layout_dir,
            theme_dir,
            mouse_mode,
            pane_frames,
            mirror_session,
            on_force_close,
            scroll_buffer_size,
            copy_command,
            copy_clipboard,
            copy_on_select,
            scrollback_editor,
            session_name,
            attach_to_session,
        }
    }
}

impl RunPlugin {
    pub fn from_kdl(kdl_node: &KdlNode) -> Result<Self, ConfigError> {
        let _allow_exec_host_cmd = kdl_get_child_entry_bool_value!(kdl_node, "_allow_exec_host_cmd").unwrap_or(false);
        let string_url = kdl_get_child_entry_string_value!(kdl_node, "location").ok_or(ConfigError::KdlParsingError("Plugins must have a location".into()))?;
        let url = Url::parse(string_url).map_err(|e| ConfigError::KdlParsingError(format!("Failed to aprse url: {:?}", e)))?;
        let location = RunPluginLocation::try_from(url)?;
        Ok(RunPlugin {
            _allow_exec_host_cmd,
            location,
        })
    }
}
impl Layout {
    pub fn from_kdl(kdl_layout: &KdlDocument) -> Result<Self, ConfigError> {
        KdlLayoutParser::new(&kdl_layout).parse()
    }
}
impl EnvironmentVariables {
    pub fn from_kdl(kdl_env_variables: &KdlNode) -> Result<Self, ConfigError> {
        let mut env: HashMap<String, String> = HashMap::new();
        for env_var in kdl_children_nodes_or_error!(kdl_env_variables, "empty env variable block") {
            let env_var_name = kdl_name!(env_var);
            let env_var_str_value = kdl_first_entry_as_string!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_int_value = kdl_first_entry_as_i64!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_value = env_var_str_value
                .or(env_var_int_value)
                .ok_or::<Box<dyn std::error::Error>>(format!("Failed to parse env var: {:?}", env_var_name).into())?;
            env.insert(env_var_name.into(), env_var_value);
        }
        Ok(EnvironmentVariables::from_data(env))
    }
}

impl Keybinds {
    fn bind_keys_in_block(block: &KdlNode, input_mode_keybinds: &mut HashMap<Key, Vec<Action>>) -> Result<(), ConfigError> {
        let bind_nodes = kdl_children_nodes_or_error!(block, "no keybinding block for mode").iter().filter(|n| kdl_name!(n) == "bind");
        let unbind_nodes = kdl_children_nodes_or_error!(block, "no keybinding block for mode").iter().filter(|n| kdl_name!(n) == "unbind");
        for key_block in bind_nodes {
            Keybinds::bind_actions_for_each_key(key_block, input_mode_keybinds)?;
        }
        // we loop twice so that the unbinds always happen after the binds
        for key_block in unbind_nodes {
            Keybinds::unbind_keys(key_block, input_mode_keybinds)?;
        }
        Ok(())
    }
    pub fn from_kdl(kdl_keybinds: &KdlNode, base_keybinds: Keybinds) -> Result<Self, ConfigError> {
        let clear_defaults = kdl_arg_is_truthy!(kdl_keybinds, "clear-defaults");
        let mut keybinds_from_config = if clear_defaults { Keybinds::default() } else { base_keybinds };
        for block in kdl_children_nodes_or_error!(kdl_keybinds, "keybindings with no children") {
            if kdl_name!(block) == "shared_except" || kdl_name!(block) == "shared" {
                let mut modes_to_exclude = vec![];
                for mode_name in kdl_string_arguments!(block) {
                    modes_to_exclude.push(InputMode::from_str(mode_name)?);
                }
                for mode in InputMode::iter() {
                    if modes_to_exclude.contains(&mode) {
                        continue;
                    }
                    let mut input_mode_keybinds = keybinds_from_config.get_input_mode_mut(&mode);
                    Keybinds::bind_keys_in_block(block, &mut input_mode_keybinds)?;
                }
            }
            if kdl_name!(block) == "shared_among" {
                let mut modes_to_include = vec![];
                for mode_name in kdl_string_arguments!(block) {
                    modes_to_include.push(InputMode::from_str(mode_name)?);
                }
                for mode in InputMode::iter() {
                    if !modes_to_include.contains(&mode) {
                        continue;
                    }
                    let mut input_mode_keybinds = keybinds_from_config.get_input_mode_mut(&mode);
                    Keybinds::bind_keys_in_block(block, &mut input_mode_keybinds)?;
                }
            }
        }
        for mode in kdl_children_nodes_or_error!(kdl_keybinds, "keybindings with no children") {
            if kdl_name!(mode) == "unbind" || kdl_name!(mode) == "shared_except" || kdl_name!(mode) == "shared_among" || kdl_name!(mode) == "shared" {
                continue;
            }
            let mut input_mode_keybinds = Keybinds::input_mode_keybindings(mode, &mut keybinds_from_config)?;
            Keybinds::bind_keys_in_block(mode, &mut input_mode_keybinds)?;
        }
        if let Some(global_unbind) = kdl_keybinds.children().and_then(|c| c.get("unbind")) {
            Keybinds::unbind_keys_in_all_modes(global_unbind, &mut keybinds_from_config)?;
        };
        Ok(keybinds_from_config)
    }
    fn bind_actions_for_each_key(key_block: &KdlNode, input_mode_keybinds: &mut HashMap<Key, Vec<Action>>) -> Result<(), ConfigError>{
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        let actions: Vec<Action> = actions_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.insert(key, actions.clone());
        }
        Ok(())
    }
    fn unbind_keys(key_block: &KdlNode, input_mode_keybinds: &mut HashMap<Key, Vec<Action>>) -> Result<(), ConfigError>{
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.remove(&key);
        }
        Ok(())
    }
    fn unbind_keys_in_all_modes(global_unbind: &KdlNode, keybinds_from_config: &mut Keybinds) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(global_unbind);
        for mode in keybinds_from_config.0.values_mut() {
            for key in &keys {
                mode.remove(&key);
            }
        }
        Ok(())
    }
    fn input_mode_keybindings <'a>(mode: &KdlNode, keybinds_from_config: &'a mut Keybinds) -> Result<&'a mut HashMap<Key, Vec<Action>>, ConfigError> {
        let mode_name = kdl_name!(mode);
        let input_mode = InputMode::from_str(mode_name)?;
        let input_mode_keybinds = keybinds_from_config.get_input_mode_mut(&input_mode);
        let clear_defaults_for_mode = kdl_arg_is_truthy!(mode, "clear-defaults");
        if clear_defaults_for_mode {
            input_mode_keybinds.clear();
        }
        Ok(input_mode_keybinds)
    }
}

impl RunCommand {
    pub fn from_kdl(kdl_node: &KdlNode) -> Result<Self, ConfigError> {
        let command = PathBuf::from(kdl_get_child_entry_string_value!(kdl_node, "cmd").ok_or(ConfigError::KdlParsingError("Command must have a cmd value".into()))?);
        let cwd = kdl_get_child_entry_string_value!(kdl_node, "cwd").map(|c| PathBuf::from(c));
        let args = match kdl_get_child!(kdl_node, "args") {
            Some(kdl_args) => {
                kdl_string_arguments!(kdl_args).iter().map(|s| String::from(*s)).collect()
            },
            None => vec![]
        };
        Ok(RunCommand {
            command,
            args,
            cwd,
        })
    }
}

impl Config {
    pub fn from_kdl(kdl_config: &str, base_config: Option<Config>) -> Result<Config, ConfigError> {
        let mut config = base_config.unwrap_or_else(|| Config::default());
        let kdl_config: KdlDocument = kdl_config.parse()?;
        // TODO: handle cases where we have more than one of these blocks (eg. two "keybinds")
        // this should give an informative parsing error
        if let Some(kdl_keybinds) = kdl_config.get("keybinds") {
            config.keybinds = Keybinds::from_kdl(&kdl_keybinds, config.keybinds)?;
        }
        let config_options = Options::from_kdl(&kdl_config);
        config.options = config.options.merge(config_options);
        if let Some(kdl_themes) = kdl_config.get("themes") {
            let config_themes = Themes::from_kdl(kdl_themes)?;
            config.themes = config.themes.merge(config_themes);
        }
        if let Some(kdl_plugin_config) = kdl_config.get("plugins") {
            let config_plugins = PluginsConfig::from_kdl(kdl_plugin_config)?;
            config.plugins = config.plugins.merge(config_plugins);
        }
        if let Some(kdl_ui_config) = kdl_config.get("ui") {
            let config_ui = UiConfig::from_kdl(&kdl_ui_config)?;
            config.ui = config.ui.merge(config_ui);
        }
        if let Some(env_config) = kdl_config.get("env") {
            let config_env = EnvironmentVariables::from_kdl(&env_config)?;
            config.env = config.env.merge(config_env);
        }
        Ok(config)
    }
}

impl PluginsConfig {
    pub fn from_kdl(kdl_plugin_config: &KdlNode) -> Result<Self, ConfigError> {
        let mut plugins: HashMap<PluginTag, PluginConfig> = HashMap::new();
        for plugin_config in kdl_children_nodes_or_error!(kdl_plugin_config, "no plugin config found") {
            let plugin_name = kdl_name!(plugin_config);
            let plugin_tag = PluginTag::new(plugin_name);
            let path = kdl_children_property_first_arg_as_string!(plugin_config, "path")
                .map(|path| PathBuf::from(path))
                .ok_or::<Box<dyn std::error::Error>>("Plugin path not found".into())?;
            let allow_exec_host_cmd = kdl_children_property_first_arg_as_bool!(plugin_config, "_allow_exec_host_cmd")
                .unwrap_or(false);
            let plugin_config = PluginConfig {
                path,
                run: PluginType::Pane(None),
                location: RunPluginLocation::Zellij(plugin_tag.clone()),
                _allow_exec_host_cmd: allow_exec_host_cmd,
            };
            plugins.insert(plugin_tag, plugin_config);
        }
        Ok(PluginsConfig(plugins))
    }
}
impl UiConfig {
    pub fn from_kdl(kdl_ui_config: &KdlNode) -> Result<UiConfig, ConfigError> {
        let mut ui_config = UiConfig::default();
        if let Some(pane_frames) = kdl_get_child!(kdl_ui_config, "pane_frames") {
            let rounded_corners = kdl_children_property_first_arg_as_bool!(pane_frames, "rounded_corners").unwrap_or(false);
            let frame_config = FrameConfig { rounded_corners };
            ui_config.pane_frames = frame_config;
        }
        Ok(ui_config)
    }
}

impl Themes {
    pub fn from_kdl(themes_from_kdl: &KdlNode) -> Result<Self, ConfigError> {
        let mut themes: HashMap<String, Theme> = HashMap::new();
        for theme_config in kdl_children_nodes_or_error!(themes_from_kdl, "no themes found") {
            let theme_name = kdl_name!(theme_config);
            let theme_colors = kdl_children_or_error!(theme_config, "empty theme");
            let theme = Theme {
                palette: Palette {
                    fg: PaletteColor::try_from(("fg", theme_colors))?,
                    bg: PaletteColor::try_from(("bg", theme_colors))?,
                    red: PaletteColor::try_from(("red", theme_colors))?,
                    green: PaletteColor::try_from(("green", theme_colors))?,
                    yellow: PaletteColor::try_from(("yellow", theme_colors))?,
                    blue: PaletteColor::try_from(("blue", theme_colors))?,
                    magenta: PaletteColor::try_from(("magenta", theme_colors))?,
                    orange: PaletteColor::try_from(("orange", theme_colors))?,
                    cyan: PaletteColor::try_from(("cyan", theme_colors))?,
                    black: PaletteColor::try_from(("black", theme_colors))?,
                    white: PaletteColor::try_from(("white", theme_colors))?,
                    ..Default::default()
                }
            };
            themes.insert(theme_name.into(), theme);
        }
        let themes = Themes::from_data(themes);
        Ok(themes)
    }
}

impl Theme {
    pub fn from_path(path_to_theme_file: PathBuf) -> Result<(String, Self), ConfigError> {
        // String is the theme name
        let mut file = File::open(path_to_theme_file)?;
        let mut kdl_config = String::new();
        file.read_to_string(&mut kdl_config);
        let kdl_config: KdlDocument = kdl_config.parse()?;
        let kdl_config = kdl_config.nodes().get(0).ok_or(ConfigError::KdlParsingError("No theme found in file".into()))?;
        let theme_name = kdl_name!(kdl_config);
        let theme_colors = kdl_children_or_error!(kdl_config, "empty theme");
        Ok((theme_name.into(), Theme {
            palette: Palette {
                fg: PaletteColor::try_from(("fg", theme_colors))?,
                bg: PaletteColor::try_from(("bg", theme_colors))?,
                red: PaletteColor::try_from(("red", theme_colors))?,
                green: PaletteColor::try_from(("green", theme_colors))?,
                yellow: PaletteColor::try_from(("yellow", theme_colors))?,
                blue: PaletteColor::try_from(("blue", theme_colors))?,
                magenta: PaletteColor::try_from(("magenta", theme_colors))?,
                orange: PaletteColor::try_from(("orange", theme_colors))?,
                cyan: PaletteColor::try_from(("cyan", theme_colors))?,
                black: PaletteColor::try_from(("black", theme_colors))?,
                white: PaletteColor::try_from(("white", theme_colors))?,
                ..Default::default()
            }
        }))
    }
}
