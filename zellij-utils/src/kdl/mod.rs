mod kdl_layout_parser;
use crate::data::{InputMode, Key, Palette, PaletteColor};
use crate::envs::EnvironmentVariables;
use crate::input::command::RunCommand;
use crate::input::config::{Config, ConfigError, KdlError};
use crate::input::keybinds::Keybinds;
use crate::input::layout::{Layout, RunPlugin, RunPluginLocation};
use crate::input::options::{Clipboard, OnForceClose, Options};
use crate::input::plugins::{PluginConfig, PluginTag, PluginType, PluginsConfig};
use crate::input::theme::{FrameConfig, Theme, Themes, UiConfig};
use kdl_layout_parser::KdlLayoutParser;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use strum::IntoEnumIterator;
use url::Url;

use miette::NamedSource;

use kdl::{KdlDocument, KdlEntry, KdlNode};

use std::path::PathBuf;
use std::str::FromStr;

use crate::input::actions::{Action, Direction, ResizeDirection, SearchDirection, SearchOption};
use crate::input::command::RunCommandAction;

#[macro_export]
macro_rules! parse_kdl_action_arguments {
    ( $action_name:expr, $action_arguments:expr, $action_node:expr ) => {{
        if !$action_arguments.is_empty() {
            Err(ConfigError::new_kdl_error(
                format!("Action '{}' must have arguments", $action_name),
                $action_node.span().offset(),
                $action_node.span().len(),
            ))
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
                _ => Err(ConfigError::new_kdl_error(
                    format!("Unsupported action: {:?}", $action_name),
                    $action_node.span().offset(),
                    $action_node.span().len(),
                )),
            }
        }
    }};
}

#[macro_export]
macro_rules! parse_kdl_action_u8_arguments {
    ( $action_name:expr, $action_arguments:expr, $action_node:expr ) => {{
        let mut bytes = vec![];
        for kdl_entry in $action_arguments.iter() {
            match kdl_entry.value().as_i64() {
                Some(int_value) => bytes.push(int_value as u8),
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Arguments for '{}' must be integers", $action_name),
                        kdl_entry.span().offset(),
                        kdl_entry.span().len(),
                    ));
                },
            }
        }
        Action::new_from_bytes($action_name, bytes, $action_node)
    }};
}

#[macro_export]
macro_rules! kdl_parsing_error {
    ( $message:expr, $entry:expr ) => {
        ConfigError::new_kdl_error($message, $entry.span().offset(), $entry.span().len())
    };
}

#[macro_export]
macro_rules! kdl_entries_as_i64 {
    ( $node:expr ) => {
        $node
            .entries()
            .iter()
            .map(|kdl_node| kdl_node.value().as_i64())
    };
}

#[macro_export]
macro_rules! kdl_first_entry_as_string {
    ( $node:expr ) => {
        $node
            .entries()
            .iter()
            .next()
            .and_then(|s| s.value().as_string())
    };
}

#[macro_export]
macro_rules! kdl_first_entry_as_i64 {
    ( $node:expr ) => {
        $node
            .entries()
            .iter()
            .next()
            .and_then(|i| i.value().as_i64())
    };
}

#[macro_export]
macro_rules! entry_count {
    ( $node:expr ) => {{
        $node.entries().iter().len()
    }};
}

#[macro_export]
macro_rules! parse_kdl_action_char_or_string_arguments {
    ( $action_name:expr, $action_arguments:expr, $action_node:expr ) => {{
        let mut chars_to_write = String::new();
        for kdl_entry in $action_arguments.iter() {
            match kdl_entry.value().as_string() {
                Some(string_value) => chars_to_write.push_str(string_value),
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("All entries for action '{}' must be strings", $action_name),
                        kdl_entry.span().offset(),
                        kdl_entry.span().len(),
                    ))
                },
            }
        }
        Action::new_from_string($action_name, chars_to_write, $action_node)
    }};
}

#[macro_export]
macro_rules! kdl_arg_is_truthy {
    ( $kdl_node:expr, $arg_name:expr ) => {
        match $kdl_node.get($arg_name) {
            Some(arg) => match arg.value().as_bool() {
                Some(value) => value,
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Argument must be true or false, found: {}", arg.value()),
                        arg.span().offset(),
                        arg.span().len(),
                    ))
                },
            },
            None => false,
        }
    };
}

#[macro_export]
macro_rules! kdl_children_nodes_or_error {
    ( $kdl_node:expr, $error:expr ) => {
        $kdl_node
            .children()
            .ok_or(ConfigError::new_kdl_error(
                $error.into(),
                $kdl_node.span().offset(),
                $kdl_node.span().len(),
            ))?
            .nodes()
    };
}

#[macro_export]
macro_rules! kdl_children_nodes {
    ( $kdl_node:expr ) => {
        $kdl_node.children().map(|c| c.nodes())
    };
}

#[macro_export]
macro_rules! kdl_property_nodes {
    ( $kdl_node:expr ) => {{
        $kdl_node
            .entries()
            .iter()
            .filter_map(|e| e.name())
            .map(|e| e.value())
    }};
}

#[macro_export]
macro_rules! kdl_children_or_error {
    ( $kdl_node:expr, $error:expr ) => {
        $kdl_node.children().ok_or(ConfigError::new_kdl_error(
            $error.into(),
            $kdl_node.span().offset(),
            $kdl_node.span().len(),
        ))?
    };
}

#[macro_export]
macro_rules! kdl_children {
    ( $kdl_node:expr ) => {
        $kdl_node.children().iter().copied().collect()
    };
}

#[macro_export]
macro_rules! kdl_string_arguments {
    ( $kdl_node:expr ) => {{
        let res: Result<Vec<_>, _> = $kdl_node
            .entries()
            .iter()
            .map(|e| {
                e.value().as_string().ok_or(ConfigError::new_kdl_error(
                    "Not a string".into(),
                    e.span().offset(),
                    e.span().len(),
                ))
            })
            .collect();
        res?
    }};
}

#[macro_export]
macro_rules! kdl_property_names {
    ( $kdl_node:expr ) => {{
        $kdl_node
            .entries()
            .iter()
            .filter_map(|e| e.name())
            .map(|e| e.value())
    }};
}

#[macro_export]
macro_rules! kdl_argument_values {
    ( $kdl_node:expr ) => {
        $kdl_node.entries().iter().collect()
    };
}

#[macro_export]
macro_rules! kdl_name {
    ( $kdl_node:expr ) => {
        $kdl_node.name().value()
    };
}

#[macro_export]
macro_rules! kdl_document_name {
    ( $kdl_node:expr ) => {
        $kdl_node.node().name().value()
    };
}

#[macro_export]
macro_rules! keys_from_kdl {
    ( $kdl_node:expr ) => {
        kdl_string_arguments!($kdl_node)
            .iter()
            .map(|k| {
                Key::from_str(k).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid key: '{}'", k),
                        $kdl_node.span().offset(),
                        $kdl_node.span().len(),
                    )
                })
            })
            .collect::<Result<_, _>>()?
    };
}

#[macro_export]
macro_rules! actions_from_kdl {
    ( $kdl_node:expr ) => {
        kdl_children_nodes_or_error!($kdl_node, "no actions found for key_block")
            .iter()
            .map(|kdl_action| Action::try_from(kdl_action))
            .collect::<Result<_, _>>()?
    };
}

pub fn kdl_arguments_that_are_strings<'a>(
    arguments: impl Iterator<Item = &'a KdlEntry>,
) -> Result<Vec<String>, ConfigError> {
    // pub fn kdl_arguments_that_are_strings <'a>(arguments: impl Iterator<Item=&'a KdlValue>) -> Result<Vec<String>, ConfigError> {
    let mut args: Vec<String> = vec![];
    for kdl_entry in arguments {
        match kdl_entry.value().as_string() {
            Some(string_value) => args.push(string_value.to_string()),
            None => {
                return Err(ConfigError::new_kdl_error(
                    format!("Argument must be a string"),
                    kdl_entry.span().offset(),
                    kdl_entry.span().len(),
                ));
            },
        }
    }
    Ok(args)
}

pub fn kdl_child_string_value_for_entry<'a>(
    command_metadata: &'a KdlDocument,
    entry_name: &'a str,
) -> Option<&'a str> {
    command_metadata
        .get(entry_name)
        .and_then(|cwd| cwd.entries().iter().next())
        .and_then(|cwd_value| cwd_value.value().as_string())
}

impl Action {
    pub fn new_from_bytes(
        action_name: &str,
        bytes: Vec<u8>,
        action_node: &KdlNode,
    ) -> Result<Self, ConfigError> {
        match action_name {
            "Write" => Ok(Action::Write(bytes)),
            "PaneNameInput" => Ok(Action::PaneNameInput(bytes)),
            "TabNameInput" => Ok(Action::TabNameInput(bytes)),
            "SearchInput" => Ok(Action::SearchInput(bytes)),
            "GoToTab" => {
                let tab_index = *bytes.get(0).ok_or_else(|| {
                    ConfigError::new_kdl_error(
                        format!("Missing tab index"),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })? as u32;
                Ok(Action::GoToTab(tab_index))
            },
            _ => Err(ConfigError::new_kdl_error(
                "Failed to parse action".into(),
                action_node.span().offset(),
                action_node.span().len(),
            )),
        }
    }
    pub fn new_from_string(
        action_name: &str,
        string: String,
        action_node: &KdlNode,
    ) -> Result<Self, ConfigError> {
        match action_name {
            "WriteChars" => Ok(Action::WriteChars(string)),
            "SwitchToMode" => match InputMode::from_str(string.as_str()) {
                Ok(input_mode) => Ok(Action::SwitchToMode(input_mode)),
                Err(_e) => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Unknown InputMode '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    ))
                },
            },
            "Resize" => {
                let direction = ResizeDirection::from_str(string.as_str()).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })?;
                Ok(Action::Resize(direction))
            },
            "MoveFocus" => {
                let direction = Direction::from_str(string.as_str()).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })?;
                Ok(Action::MoveFocus(direction))
            },
            "MoveFocusOrTab" => {
                let direction = Direction::from_str(string.as_str()).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })?;
                Ok(Action::MoveFocusOrTab(direction))
            },
            "MovePane" => {
                if string.is_empty() {
                    return Ok(Action::MovePane(None));
                } else {
                    let direction = Direction::from_str(string.as_str()).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid direction: '{}'", string),
                            action_node.span().offset(),
                            action_node.span().len(),
                        )
                    })?;
                    Ok(Action::MovePane(Some(direction)))
                }
            },
            "DumpScreen" => Ok(Action::DumpScreen(string)),
            "NewPane" => {
                if string.is_empty() {
                    return Ok(Action::NewPane(None));
                } else {
                    let direction = Direction::from_str(string.as_str()).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid direction: '{}'", string),
                            action_node.span().offset(),
                            action_node.span().len(),
                        )
                    })?;
                    Ok(Action::NewPane(Some(direction)))
                }
            },
            "SearchToggleOption" => {
                let toggle_option = SearchOption::from_str(string.as_str()).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })?;
                Ok(Action::SearchToggleOption(toggle_option))
            },
            "Search" => {
                let search_direction =
                    SearchDirection::from_str(string.as_str()).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid direction: '{}'", string),
                            action_node.span().offset(),
                            action_node.span().len(),
                        )
                    })?;
                Ok(Action::Search(search_direction))
            },
            _ => Err(ConfigError::new_kdl_error(
                format!("Unsupported action: {}", action_name),
                action_node.span().offset(),
                action_node.span().len(),
            )),
        }
    }
}

impl TryFrom<(&str, &KdlDocument)> for PaletteColor {
    type Error = ConfigError;

    fn try_from(
        (color_name, theme_colors): (&str, &KdlDocument),
    ) -> Result<PaletteColor, Self::Error> {
        let color = theme_colors
            .get(color_name)
            .ok_or(ConfigError::new_kdl_error(
                format!("Missing theme color: {}", color_name),
                theme_colors.span().offset(),
                theme_colors.span().len(),
            ))?;
        let entry_count = entry_count!(color);
        let is_rgb = || entry_count == 3;
        let is_three_digit_hex = || {
            match kdl_first_entry_as_string!(color) {
                // 4 including the '#' character
                Some(s) => entry_count == 1 && s.starts_with('#') && s.len() == 4,
                None => false,
            }
        };
        let is_six_digit_hex = || {
            match kdl_first_entry_as_string!(color) {
                // 7 including the '#' character
                Some(s) => entry_count == 1 && s.starts_with('#') && s.len() == 7,
                None => false,
            }
        };
        let is_eight_bit = || kdl_first_entry_as_i64!(color).is_some() && entry_count == 1;
        if is_rgb() {
            let mut channels = kdl_entries_as_i64!(color);
            let r = channels.next().unwrap().ok_or(ConfigError::new_kdl_error(
                format!("invalid rgb color"),
                color.span().offset(),
                color.span().len(),
            ))? as u8;
            let g = channels.next().unwrap().ok_or(ConfigError::new_kdl_error(
                format!("invalid rgb color"),
                color.span().offset(),
                color.span().len(),
            ))? as u8;
            let b = channels.next().unwrap().ok_or(ConfigError::new_kdl_error(
                format!("invalid rgb color"),
                color.span().offset(),
                color.span().len(),
            ))? as u8;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_three_digit_hex() {
            // eg. #fff (hex, will be converted to rgb)
            let mut s = String::from(kdl_first_entry_as_string!(color).unwrap());
            s.remove(0);
            let r = u8::from_str_radix(&s[0..1], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })? * 0x11;
            let g = u8::from_str_radix(&s[1..2], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })? * 0x11;
            let b = u8::from_str_radix(&s[2..3], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })? * 0x11;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_six_digit_hex() {
            // eg. #ffffff (hex, will be converted to rgb)
            let mut s = String::from(kdl_first_entry_as_string!(color).unwrap());
            s.remove(0);
            let r = u8::from_str_radix(&s[0..2], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })?;
            let g = u8::from_str_radix(&s[2..4], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })?;
            let b = u8::from_str_radix(&s[4..6], 16).map_err(|_| {
                ConfigError::new_kdl_error(
                    "Failed to parse hex color".into(),
                    color.span().offset(),
                    color.span().len(),
                )
            })?;
            Ok(PaletteColor::Rgb((r, g, b)))
        } else if is_eight_bit() {
            let n = kdl_first_entry_as_i64!(color).ok_or(ConfigError::new_kdl_error(
                "Failed to parse color".into(),
                color.span().offset(),
                color.span().len(),
            ))?;
            Ok(PaletteColor::EightBit(n as u8))
        } else {
            Err(ConfigError::new_kdl_error(
                "Failed to parse color".into(),
                color.span().offset(),
                color.span().len(),
            ))
        }
    }
}

impl TryFrom<&KdlNode> for Action {
    type Error = ConfigError;
    fn try_from(kdl_action: &KdlNode) -> Result<Self, Self::Error> {
        let action_name = kdl_name!(kdl_action);
        let action_arguments: Vec<&KdlEntry> = kdl_argument_values!(kdl_action);
        let action_children: Vec<&KdlDocument> = kdl_children!(kdl_action);
        match action_name {
            "Quit" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "FocusNextPane" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "FocusPreviousPane" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "SwitchFocus" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "EditScrollback" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "ScrollUp" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "ScrollDown" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "ScrollToBottom" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "PageScrollUp" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "PageScrollDown" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "HalfPageScrollUp" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "HalfPageScrollDown" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "ToggleFocusFullscreen" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "TogglePaneFrames" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "ToggleActiveSyncTab" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "TogglePaneEmbedOrFloating" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "ToggleFloatingPanes" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "CloseFocus" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "UndoRenamePane" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "NoOp" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "GoToNextTab" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "GoToPreviousTab" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "CloseTab" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "ToggleTab" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "UndoRenameTab" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "Detach" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Copy" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Confirm" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Deny" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Write" => parse_kdl_action_u8_arguments!(action_name, action_arguments, kdl_action),
            "WriteChars" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "SwitchToMode" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "Search" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "Resize" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MoveFocus" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MoveFocusOrTab" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MovePane" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "DumpScreen" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "NewPane" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "PaneNameInput" => {
                parse_kdl_action_u8_arguments!(action_name, action_arguments, kdl_action)
            },
            "NewTab" => Ok(Action::NewTab(None, None)),
            "GoToTab" => parse_kdl_action_u8_arguments!(action_name, action_arguments, kdl_action),
            "TabNameInput" => {
                parse_kdl_action_u8_arguments!(action_name, action_arguments, kdl_action)
            },
            "SearchInput" => {
                parse_kdl_action_u8_arguments!(action_name, action_arguments, kdl_action)
            },
            "SearchToggleOption" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "Run" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_strings(arguments)?;
                if args.is_empty() {
                    return Err(ConfigError::new_kdl_error(
                        "No command found in Run action".into(),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    ));
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
                    hold_on_close: true,
                };
                Ok(Action::Run(run_command_action))
            },
            _ => Err(ConfigError::new_kdl_error(
                format!("Unsupported action: {}", action_name).into(),
                kdl_action.span().offset(),
                kdl_action.span().len(),
            )),
        }
    }
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_string {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node
            .get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_string())
    };
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_string_or_error {
    ( $kdl_node:expr, $property_name:expr ) => {{
        match $kdl_node.get($property_name) {
            Some(property) => match property.entries().iter().next() {
                Some(first_entry) => match first_entry.value().as_string() {
                    Some(string_entry) => Some((string_entry, first_entry)),
                    None => {
                        return Err(ConfigError::new_kdl_error(
                            format!(
                                "Property {} must be a string, found: {}",
                                $property_name,
                                first_entry.value()
                            ),
                            property.span().offset(),
                            property.span().len(),
                        ));
                    },
                },
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Property {} must have a value", $property_name),
                        property.span().offset(),
                        property.span().len(),
                    ));
                },
            },
            None => None,
        }
    }};
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_bool_or_error {
    ( $kdl_node:expr, $property_name:expr ) => {{
        match $kdl_node.get($property_name) {
            Some(property) => match property.entries().iter().next() {
                Some(first_entry) => match first_entry.value().as_bool() {
                    Some(bool_entry) => Some((bool_entry, first_entry)),
                    None => {
                        return Err(ConfigError::new_kdl_error(
                            format!(
                                "Property {} must be true or false, found {}",
                                $property_name,
                                first_entry.value()
                            ),
                            property.span().offset(),
                            property.span().len(),
                        ));
                    },
                },
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Property {} must have a value", $property_name),
                        property.span().offset(),
                        property.span().len(),
                    ));
                },
            },
            None => None,
        }
    }};
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_i64_or_error {
    ( $kdl_node:expr, $property_name:expr ) => {{
        match $kdl_node.get($property_name) {
            Some(property) => match property.entries().iter().next() {
                Some(first_entry) => match first_entry.value().as_i64() {
                    Some(int_entry) => Some((int_entry, first_entry)),
                    None => {
                        return Err(ConfigError::new_kdl_error(
                            format!(
                                "Property {} must be numeric, found {}",
                                $property_name,
                                first_entry.value()
                            ),
                            property.span().offset(),
                            property.span().len(),
                        ));
                    },
                },
                None => {
                    return Err(ConfigError::new_kdl_error(
                        format!("Property {} must have a value", $property_name),
                        property.span().offset(),
                        property.span().len(),
                    ));
                },
            },
            None => None,
        }
    }};
}

#[macro_export]
macro_rules! kdl_has_string_argument {
    ( $kdl_node:expr, $string_argument:expr ) => {
        $kdl_node
            .entries()
            .iter()
            .find(|e| e.value().as_string() == Some($string_argument))
            .is_some()
    };
}

#[macro_export]
macro_rules! kdl_children_property_first_arg_as_string {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node
            .children()
            .and_then(|c| c.get($property_name))
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_string())
    };
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_bool {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node
            .get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_bool())
    };
}

#[macro_export]
macro_rules! kdl_children_property_first_arg_as_bool {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node
            .children()
            .and_then(|c| c.get($property_name))
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_bool())
    };
}

#[macro_export]
macro_rules! kdl_property_first_arg_as_i64 {
    ( $kdl_node:expr, $property_name:expr ) => {
        $kdl_node
            .get($property_name)
            .and_then(|p| p.entries().iter().next())
            .and_then(|p| p.value().as_i64())
    };
}

#[macro_export]
macro_rules! kdl_get_child {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node.children().and_then(|c| c.get($child_name))
    };
}

#[macro_export]
macro_rules! kdl_get_child_entry_bool_value {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node
            .children()
            .and_then(|c| c.get($child_name))
            .and_then(|c| c.get(0))
            .and_then(|c| c.value().as_bool())
    };
}

#[macro_export]
macro_rules! kdl_get_child_entry_string_value {
    ( $kdl_node:expr, $child_name:expr ) => {
        $kdl_node
            .children()
            .and_then(|c| c.get($child_name))
            .and_then(|c| c.get(0))
            .and_then(|c| c.value().as_string())
    };
}

#[macro_export]
macro_rules! kdl_get_bool_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node
            .get($name)
            .and_then(|e| e.value().as_bool())
            .or_else(|| {
                $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.value().as_bool())
            })
    };
}

#[macro_export]
macro_rules! kdl_get_bool_property_or_child_value_with_error {
    ( $kdl_node:expr, $name:expr ) => {
        match $kdl_node.get($name) {
            Some(e) => match e.value().as_bool() {
                Some(bool_value) => Some(bool_value),
                None => {
                    return Err(kdl_parsing_error!(
                        format!(
                            "{} should be either true or false, found {}",
                            $name,
                            e.value()
                        ),
                        e
                    ))
                },
            },
            None => {
                let child_value = $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0));
                match child_value {
                    Some(e) => match e.value().as_bool() {
                        Some(bool_value) => Some(bool_value),
                        None => {
                            return Err(kdl_parsing_error!(
                                format!(
                                    "{} should be either true or false, found {}",
                                    $name,
                                    e.value()
                                ),
                                e
                            ))
                        },
                    },
                    None => {
                        if let Some(child_node) = kdl_child_with_name!($kdl_node, $name) {
                            return Err(kdl_parsing_error!(
                                format!(
                                    "{} must have a value, eg. '{} true'",
                                    child_node.name().value(),
                                    child_node.name().value()
                                ),
                                child_node
                            ));
                        }
                        None
                    },
                }
            },
        }
    };
}

#[macro_export]
macro_rules! kdl_get_string_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node
            .get($name)
            .and_then(|e| e.value().as_string())
            .or_else(|| {
                $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.value().as_string())
            })
    };
}

#[macro_export]
macro_rules! kdl_property_or_child_value_node {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node.get($name).or_else(|| {
            $kdl_node
                .children()
                .and_then(|c| c.get($name))
                .and_then(|c| c.get(0))
        })
    };
}

#[macro_export]
macro_rules! kdl_child_with_name {
    ( $kdl_node:expr, $name:expr ) => {{
        $kdl_node
            .children()
            .and_then(|children| children.nodes().iter().find(|c| c.name().value() == $name))
    }};
}

#[macro_export]
macro_rules! kdl_get_string_property_or_child_value_with_error {
    ( $kdl_node:expr, $name:expr ) => {
        match $kdl_node.get($name) {
            Some(e) => match e.value().as_string() {
                Some(string_value) => Some(string_value),
                None => {
                    return Err(kdl_parsing_error!(
                        format!(
                            "{} should be a string, found {} - not a string",
                            $name,
                            e.value()
                        ),
                        e
                    ))
                },
            },
            None => {
                let child_value = $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0));
                match child_value {
                    Some(e) => match e.value().as_string() {
                        Some(string_value) => Some(string_value),
                        None => {
                            return Err(kdl_parsing_error!(
                                format!(
                                    "{} should be a string, found {} - not a string",
                                    $name,
                                    e.value()
                                ),
                                e
                            ))
                        },
                    },
                    None => {
                        if let Some(child_node) = kdl_child_with_name!($kdl_node, $name) {
                            return Err(kdl_parsing_error!(
                                format!(
                                    "{} must have a value, eg. '{} \"foo\"'",
                                    child_node.name().value(),
                                    child_node.name().value()
                                ),
                                child_node
                            ));
                        }
                        None
                    },
                }
            },
        }
    };
}

#[macro_export]
macro_rules! kdl_get_property_or_child {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node
            .get($name)
            // .and_then(|e| e.value().as_string())
            .or_else(|| {
                $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0))
            })
    };
}

#[macro_export]
macro_rules! kdl_get_int_property_or_child_value {
    ( $kdl_node:expr, $name:expr ) => {
        $kdl_node
            .get($name)
            .and_then(|e| e.value().as_i64())
            .or_else(|| {
                $kdl_node
                    .children()
                    .and_then(|c| c.get($name))
                    .and_then(|c| c.get(0))
                    .and_then(|c| c.value().as_i64())
            })
    };
}

#[macro_export]
macro_rules! kdl_get_string_entry {
    ( $kdl_node:expr, $entry_name:expr ) => {
        $kdl_node
            .get($entry_name)
            .and_then(|e| e.value().as_string())
    };
}

#[macro_export]
macro_rules! kdl_get_int_entry {
    ( $kdl_node:expr, $entry_name:expr ) => {
        $kdl_node.get($entry_name).and_then(|e| e.value().as_i64())
    };
}

impl Options {
    pub fn from_kdl(kdl_options: &KdlDocument) -> Result<Self, ConfigError> {
        let on_force_close =
            match kdl_property_first_arg_as_string_or_error!(kdl_options, "on_force_close") {
                Some((string, entry)) => Some(OnForceClose::from_str(string).map_err(|_| {
                    kdl_parsing_error!(
                        format!("Invalid value for on_force_close: '{}'", string),
                        entry
                    )
                })?),
                None => None,
            };
        let simplified_ui =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "simplified_ui").map(|(v, _)| v);
        let default_shell =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "default_shell")
                .map(|(string, _entry)| PathBuf::from(string));
        let pane_frames =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "pane_frames").map(|(v, _)| v);
        let theme = kdl_property_first_arg_as_string_or_error!(kdl_options, "theme")
            .map(|(theme, _entry)| theme.to_string());
        let default_mode =
            match kdl_property_first_arg_as_string_or_error!(kdl_options, "default_mode") {
                Some((string, entry)) => Some(InputMode::from_str(string).map_err(|_| {
                    kdl_parsing_error!(format!("Invalid input mode: '{}'", string), entry)
                })?),
                None => None,
            };
        let default_layout =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "default_layout")
                .map(|(string, _entry)| PathBuf::from(string));
        let layout_dir = kdl_property_first_arg_as_string_or_error!(kdl_options, "layout_dir")
            .map(|(string, _entry)| PathBuf::from(string));
        let theme_dir = kdl_property_first_arg_as_string_or_error!(kdl_options, "theme_dir")
            .map(|(string, _entry)| PathBuf::from(string));
        let mouse_mode =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "mouse_mode").map(|(v, _)| v);
        let scroll_buffer_size =
            kdl_property_first_arg_as_i64_or_error!(kdl_options, "scroll_buffer_size")
                .map(|(scroll_buffer_size, _entry)| scroll_buffer_size as usize);
        let copy_command = kdl_property_first_arg_as_string_or_error!(kdl_options, "copy_command")
            .map(|(copy_command, _entry)| copy_command.to_string());
        let copy_clipboard =
            match kdl_property_first_arg_as_string_or_error!(kdl_options, "copy_clipboard") {
                Some((string, entry)) => Some(Clipboard::from_str(string).map_err(|_| {
                    kdl_parsing_error!(
                        format!("Invalid value for copy_clipboard: '{}'", string),
                        entry
                    )
                })?),
                None => None,
            };
        let copy_on_select =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "copy_on_select").map(|(v, _)| v);
        let scrollback_editor =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "scrollback_editor")
                .map(|(string, _entry)| PathBuf::from(string));
        let mirror_session =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "mirror_session").map(|(v, _)| v);
        let session_name = kdl_property_first_arg_as_string_or_error!(kdl_options, "session_name")
            .map(|(session_name, _entry)| session_name.to_string());
        let attach_to_session =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "attach_to_session")
                .map(|(v, _)| v);
        Ok(Options {
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
        })
    }
}

impl RunPlugin {
    pub fn from_kdl(kdl_node: &KdlNode) -> Result<Self, ConfigError> {
        let _allow_exec_host_cmd =
            kdl_get_child_entry_bool_value!(kdl_node, "_allow_exec_host_cmd").unwrap_or(false);
        let string_url = kdl_get_child_entry_string_value!(kdl_node, "location").ok_or(
            ConfigError::new_kdl_error(
                "Plugins must have a location".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ),
        )?;
        let url = Url::parse(string_url).map_err(|e| {
            ConfigError::new_kdl_error(
                format!("Failed to parse url: {:?}", e),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            )
        })?;
        let location = RunPluginLocation::try_from(url)?;
        Ok(RunPlugin {
            _allow_exec_host_cmd,
            location,
        })
    }
}
impl Layout {
    pub fn from_kdl(raw_layout: &str, file_name: String) -> Result<Self, ConfigError> {
        KdlLayoutParser::new(raw_layout).parse().map_err(|e| {
            match e {
                ConfigError::KdlError(kdl_error) => ConfigError::KdlError(kdl_error.add_src(file_name, String::from(raw_layout))),
                ConfigError::KdlDeserializationError(kdl_error) => {
                    let error_message = match kdl_error.kind {
                        kdl::KdlErrorKind::Context("valid node terminator") => {
                            format!("Failed to deserialize KDL node. \nPossible reasons:\n{}\n{}\n{}\n{}",
                            "- Missing `;` after a node name, eg. { node; another_node; }",
                            "- Missing quotations (\") around an argument node eg. { first_node \"argument_node\"; }",
                            "- Missing an equal sign (=) between node arguments on a title line. eg. argument=\"value\"",
                            "- Found an extraneous equal sign (=) between node child arguments and their values. eg. { argument=\"value\" }")
                        },
                        _ => String::from(kdl_error.help.unwrap_or("Kdl Deserialization Error")),
                    };
                    let kdl_error = KdlError {
                        error_message,
                        src: Some(NamedSource::new(file_name, String::from(raw_layout))),
                        offset: Some(kdl_error.span.offset()),
                        len: Some(kdl_error.span.len()),
                    };
                    ConfigError::KdlError(kdl_error)
                },
                e => e
            }
        })
    }
}
impl EnvironmentVariables {
    pub fn from_kdl(kdl_env_variables: &KdlNode) -> Result<Self, ConfigError> {
        let mut env: HashMap<String, String> = HashMap::new();
        for env_var in kdl_children_nodes_or_error!(kdl_env_variables, "empty env variable block") {
            let env_var_name = kdl_name!(env_var);
            let env_var_str_value =
                kdl_first_entry_as_string!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_int_value =
                kdl_first_entry_as_i64!(env_var).map(|s| format!("{}", s.to_string()));
            let env_var_value =
                env_var_str_value
                    .or(env_var_int_value)
                    .ok_or(ConfigError::new_kdl_error(
                        format!("Failed to parse env var: {:?}", env_var_name),
                        env_var.span().offset(),
                        env_var.span().len(),
                    ))?;
            env.insert(env_var_name.into(), env_var_value);
        }
        Ok(EnvironmentVariables::from_data(env))
    }
}

impl Keybinds {
    fn bind_keys_in_block(
        block: &KdlNode,
        input_mode_keybinds: &mut HashMap<Key, Vec<Action>>,
    ) -> Result<(), ConfigError> {
        let all_nodes = kdl_children_nodes_or_error!(block, "no keybinding block for mode");
        let bind_nodes = all_nodes.iter().filter(|n| kdl_name!(n) == "bind");
        let unbind_nodes = all_nodes.iter().filter(|n| kdl_name!(n) == "unbind");
        for key_block in bind_nodes {
            Keybinds::bind_actions_for_each_key(key_block, input_mode_keybinds)?;
        }
        // we loop a second time so that the unbinds always happen after the binds
        for key_block in unbind_nodes {
            Keybinds::unbind_keys(key_block, input_mode_keybinds)?;
        }
        for key_block in all_nodes {
            if kdl_name!(key_block) != "bind" && kdl_name!(key_block) != "unbind" {
                return Err(ConfigError::new_kdl_error(
                    format!("Unknown keybind instruction: '{}'", kdl_name!(key_block)),
                    key_block.span().offset(),
                    key_block.span().len(),
                ));
            }
        }
        Ok(())
    }
    pub fn from_kdl(kdl_keybinds: &KdlNode, base_keybinds: Keybinds) -> Result<Self, ConfigError> {
        let clear_defaults = kdl_arg_is_truthy!(kdl_keybinds, "clear-defaults");
        let mut keybinds_from_config = if clear_defaults {
            Keybinds::default()
        } else {
            base_keybinds
        };
        for block in kdl_children_nodes_or_error!(kdl_keybinds, "keybindings with no children") {
            if kdl_name!(block) == "shared_except" || kdl_name!(block) == "shared" {
                let mut modes_to_exclude = vec![];
                for mode_name in kdl_string_arguments!(block) {
                    modes_to_exclude.push(InputMode::from_str(mode_name).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid mode: '{}'", mode_name),
                            block.name().span().offset(),
                            block.name().span().len(),
                        )
                    })?);
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
            if kdl_name!(mode) == "unbind"
                || kdl_name!(mode) == "shared_except"
                || kdl_name!(mode) == "shared_among"
                || kdl_name!(mode) == "shared"
            {
                continue;
            }
            let mut input_mode_keybinds =
                Keybinds::input_mode_keybindings(mode, &mut keybinds_from_config)?;
            Keybinds::bind_keys_in_block(mode, &mut input_mode_keybinds)?;
        }
        if let Some(global_unbind) = kdl_keybinds.children().and_then(|c| c.get("unbind")) {
            Keybinds::unbind_keys_in_all_modes(global_unbind, &mut keybinds_from_config)?;
        };
        Ok(keybinds_from_config)
    }
    fn bind_actions_for_each_key(
        key_block: &KdlNode,
        input_mode_keybinds: &mut HashMap<Key, Vec<Action>>,
    ) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        let actions: Vec<Action> = actions_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.insert(key, actions.clone());
        }
        Ok(())
    }
    fn unbind_keys(
        key_block: &KdlNode,
        input_mode_keybinds: &mut HashMap<Key, Vec<Action>>,
    ) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.remove(&key);
        }
        Ok(())
    }
    fn unbind_keys_in_all_modes(
        global_unbind: &KdlNode,
        keybinds_from_config: &mut Keybinds,
    ) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(global_unbind);
        for mode in keybinds_from_config.0.values_mut() {
            for key in &keys {
                mode.remove(&key);
            }
        }
        Ok(())
    }
    fn input_mode_keybindings<'a>(
        mode: &KdlNode,
        keybinds_from_config: &'a mut Keybinds,
    ) -> Result<&'a mut HashMap<Key, Vec<Action>>, ConfigError> {
        let mode_name = kdl_name!(mode);
        let input_mode = InputMode::from_str(mode_name).map_err(|_| {
            ConfigError::new_kdl_error(
                format!("Invalid mode: '{}'", mode_name),
                mode.name().span().offset(),
                mode.name().span().len(),
            )
        })?;
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
        let command = PathBuf::from(kdl_get_child_entry_string_value!(kdl_node, "cmd").ok_or(
            ConfigError::new_kdl_error(
                "Command must have a cmd value".into(),
                kdl_node.span().offset(),
                kdl_node.span().len(),
            ),
        )?);
        let cwd = kdl_get_child_entry_string_value!(kdl_node, "cwd").map(|c| PathBuf::from(c));
        let args = match kdl_get_child!(kdl_node, "args") {
            Some(kdl_args) => kdl_string_arguments!(kdl_args)
                .iter()
                .map(|s| String::from(*s))
                .collect(),
            None => vec![],
        };
        Ok(RunCommand {
            command,
            args,
            cwd,
            hold_on_close: true,
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
        let config_options = Options::from_kdl(&kdl_config)?;
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
        for plugin_config in
            kdl_children_nodes_or_error!(kdl_plugin_config, "no plugin config found")
        {
            let plugin_name = kdl_name!(plugin_config);
            let plugin_tag = PluginTag::new(plugin_name);
            let path = kdl_children_property_first_arg_as_string!(plugin_config, "path")
                .map(|path| PathBuf::from(path))
                .ok_or(ConfigError::new_kdl_error(
                    "Plugin path not found or invalid".into(),
                    plugin_config.span().offset(),
                    plugin_config.span().len(),
                ))?;
            let allow_exec_host_cmd =
                kdl_children_property_first_arg_as_bool!(plugin_config, "_allow_exec_host_cmd")
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
            let rounded_corners =
                kdl_children_property_first_arg_as_bool!(pane_frames, "rounded_corners")
                    .unwrap_or(false);
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
                },
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
        let mut file = File::open(path_to_theme_file.clone())?;
        let mut kdl_config = String::new();
        file.read_to_string(&mut kdl_config)?;
        let kdl_config: KdlDocument = kdl_config.parse()?;
        let kdl_themes = kdl_config.get("themes").ok_or(ConfigError::new_kdl_error(
            "No theme node found in file".into(),
            kdl_config.span().offset(),
            kdl_config.span().len(),
        ))?;
        let all_themes_in_file = Themes::from_kdl(kdl_themes)?;
        let theme_file_name = path_to_theme_file
            .file_name()
            .ok_or(ConfigError::new_kdl_error(
                "Failed to find file name".into(),
                kdl_config.span().offset(),
                kdl_config.span().len(),
            ))?
            .to_string_lossy()
            .to_string();
        if let Some(theme_name) = theme_file_name.strip_suffix(".kdl") {
            let theme =
                all_themes_in_file
                    .get_theme(theme_name)
                    .ok_or(ConfigError::new_kdl_error(
                        format!(
                            "Not theme with name {} found in file {:?}",
                            theme_name, path_to_theme_file
                        ),
                        kdl_config.span().offset(),
                        kdl_config.span().len(),
                    ))?;
            Ok((theme_name.to_string(), theme.clone()))
        } else {
            Err(ConfigError::new_kdl_error(
                "no theme file found".into(),
                kdl_config.span().offset(),
                kdl_config.span().len(),
            ))
        }
    }
}
