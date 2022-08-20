//! Definition of the actions that can be bound to keys.

// use super::layout::TabLayout;
use crate::data::{InputMode, Key, CharOrArrow, PaletteColor};
use crate::input::options::{Options, OnForceClose, Clipboard};

use kdl::{KdlDocument, KdlValue, KdlNode};

use std::str::FromStr;
use std::path::PathBuf;

use crate::input::actions::{Action, ResizeDirection, Direction};
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
        $kdl_node.get("clear-defaults").and_then(|c| c.value().as_bool()).unwrap_or(false)
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
                match InputMode::try_from(string.as_str()) {
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

// impl TryFrom<(&str, Vec<&KdlValue>, Vec<&KdlDocument>)> for Action {
impl TryFrom<&KdlNode> for Action {
    type Error = Box<dyn std::error::Error>;
    // fn try_from((action_name, action_arguments, action_children): (&str, Vec<&KdlValue>, Vec<&KdlDocument>)) -> Result<Self, Self::Error> {
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
            .and_then(|default_mode| InputMode::try_from(default_mode).ok());
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
