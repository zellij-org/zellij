mod kdl_layout_parser;
use crate::data::{
    Direction, FloatingPaneCoordinates, InputMode, Key, LayoutInfo, Palette, PaletteColor,
    PaneInfo, PaneManifest, PermissionType, Resize, SessionInfo, TabInfo,
};
use crate::envs::EnvironmentVariables;
use crate::home::{find_default_config_dir, get_layout_dir};
use crate::input::config::{Config, ConfigError, KdlError};
use crate::input::keybinds::Keybinds;
use crate::input::layout::{Layout, RunPlugin, RunPluginOrAlias};
use crate::input::options::{Clipboard, OnForceClose, Options};
use crate::input::permission::{GrantedPermission, PermissionCache};
use crate::input::plugins::PluginAliases;
use crate::input::theme::{FrameConfig, Theme, Themes, UiConfig};
use kdl_layout_parser::KdlLayoutParser;
use std::collections::{BTreeMap, HashMap, HashSet};
use strum::IntoEnumIterator;
use uuid::Uuid;

use miette::NamedSource;

use kdl::{KdlDocument, KdlEntry, KdlNode};

use std::path::PathBuf;
use std::str::FromStr;

use crate::input::actions::{Action, SearchDirection, SearchOption};
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
                "ScrollToTop" => Ok(Action::ScrollToTop),
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
                "ToggleMouseMode" => Ok(Action::ToggleMouseMode),
                "PreviousSwapLayout" => Ok(Action::PreviousSwapLayout),
                "NextSwapLayout" => Ok(Action::NextSwapLayout),
                "Clear" => Ok(Action::ClearScreen),
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
macro_rules! kdl_first_entry_as_bool {
    ( $node:expr ) => {
        $node
            .entries()
            .iter()
            .next()
            .and_then(|i| i.value().as_bool())
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
    ( $kdl_node:expr, $config_options:expr ) => {
        kdl_children_nodes_or_error!($kdl_node, "no actions found for key_block")
            .iter()
            .map(|kdl_action| Action::try_from((kdl_action, $config_options)))
            .collect::<Result<_, _>>()?
    };
}

pub fn kdl_arguments_that_are_strings<'a>(
    arguments: impl Iterator<Item = &'a KdlEntry>,
) -> Result<Vec<String>, ConfigError> {
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

pub fn kdl_child_bool_value_for_entry<'a>(
    command_metadata: &'a KdlDocument,
    entry_name: &'a str,
) -> Option<bool> {
    command_metadata
        .get(entry_name)
        .and_then(|cwd| cwd.entries().iter().next())
        .and_then(|cwd_value| cwd_value.value().as_bool())
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
                let mut resize: Option<Resize> = None;
                let mut direction: Option<Direction> = None;
                for word in string.to_ascii_lowercase().split_whitespace() {
                    match Resize::from_str(word) {
                        Ok(value) => resize = Some(value),
                        Err(_) => match Direction::from_str(word) {
                            Ok(value) => direction = Some(value),
                            Err(_) => {
                                return Err(ConfigError::new_kdl_error(
                                    format!(
                                    "failed to read either of resize type or direction from '{}'",
                                    word
                                ),
                                    action_node.span().offset(),
                                    action_node.span().len(),
                                ))
                            },
                        },
                    }
                }
                let resize = resize.unwrap_or(Resize::Increase);
                Ok(Action::Resize(resize, direction))
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
            "MoveTab" => {
                let direction = Direction::from_str(string.as_str()).map_err(|_| {
                    ConfigError::new_kdl_error(
                        format!("Invalid direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    )
                })?;
                if direction.is_vertical() {
                    Err(ConfigError::new_kdl_error(
                        format!("Invalid horizontal direction: '{}'", string),
                        action_node.span().offset(),
                        action_node.span().len(),
                    ))
                } else {
                    Ok(Action::MoveTab(direction))
                }
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
            "MovePaneBackwards" => Ok(Action::MovePaneBackwards),
            "DumpScreen" => Ok(Action::DumpScreen(string, false)),
            "DumpLayout" => Ok(Action::DumpLayout),
            "NewPane" => {
                if string.is_empty() {
                    return Ok(Action::NewPane(None, None));
                } else {
                    let direction = Direction::from_str(string.as_str()).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid direction: '{}'", string),
                            action_node.span().offset(),
                            action_node.span().len(),
                        )
                    })?;
                    Ok(Action::NewPane(Some(direction), None))
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
            "RenameSession" => Ok(Action::RenameSession(string)),
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

impl TryFrom<(&KdlNode, &Options)> for Action {
    type Error = ConfigError;
    fn try_from((kdl_action, config_options): (&KdlNode, &Options)) -> Result<Self, Self::Error> {
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
            "ScrollToTop" => {
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
            "ToggleMouseMode" => {
                parse_kdl_action_arguments!(action_name, action_arguments, kdl_action)
            },
            "Detach" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Copy" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
            "Clear" => parse_kdl_action_arguments!(action_name, action_arguments, kdl_action),
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
            "ResizeNew" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MoveFocus" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MoveTab" => parse_kdl_action_char_or_string_arguments!(
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
            "MovePaneBackwards" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "DumpScreen" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "DumpLayout" => parse_kdl_action_char_or_string_arguments!(
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
            "NewTab" => {
                let command_metadata = action_children.iter().next();
                if command_metadata.is_none() {
                    return Ok(Action::NewTab(None, vec![], None, None, None));
                }

                let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

                let layout = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "layout"))
                    .map(|layout_string| PathBuf::from(layout_string))
                    .or_else(|| config_options.default_layout.clone());
                let cwd = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "cwd"))
                    .map(|cwd_string| PathBuf::from(cwd_string))
                    .map(|cwd| current_dir.join(cwd));
                let name = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "name"))
                    .map(|name_string| name_string.to_string());

                let layout_dir = config_options
                    .layout_dir
                    .clone()
                    .or_else(|| get_layout_dir(find_default_config_dir()));
                let (path_to_raw_layout, raw_layout, swap_layouts) =
                    Layout::stringified_from_path_or_default(layout.as_ref(), layout_dir).map_err(
                        |e| {
                            ConfigError::new_kdl_error(
                                format!("Failed to load layout: {}", e),
                                kdl_action.span().offset(),
                                kdl_action.span().len(),
                            )
                        },
                    )?;

                let layout = Layout::from_str(
                    &raw_layout,
                    path_to_raw_layout,
                    swap_layouts.as_ref().map(|(f, p)| (f.as_str(), p.as_str())),
                    cwd,
                )
                .map_err(|e| {
                    ConfigError::new_kdl_error(
                        format!("Failed to load layout: {}", e),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    )
                })?;

                let swap_tiled_layouts = Some(layout.swap_tiled_layouts.clone());
                let swap_floating_layouts = Some(layout.swap_floating_layouts.clone());

                let mut tabs = layout.tabs();
                if tabs.len() > 1 {
                    return Err(ConfigError::new_kdl_error(
                        "Tab layout cannot itself have tabs".to_string(),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    ));
                } else if !tabs.is_empty() {
                    let (tab_name, layout, floating_panes_layout) = tabs.drain(..).next().unwrap();
                    let name = tab_name.or(name);

                    Ok(Action::NewTab(
                        Some(layout),
                        floating_panes_layout,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        name,
                    ))
                } else {
                    let (layout, floating_panes_layout) = layout.new_tab();

                    Ok(Action::NewTab(
                        Some(layout),
                        floating_panes_layout,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        name,
                    ))
                }
            },
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
                let name = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "name"))
                    .map(|name_string| name_string.to_string());
                let direction = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "direction"))
                    .and_then(|direction_string| Direction::from_str(direction_string).ok());
                let hold_on_close = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "close_on_exit"))
                    .and_then(|close_on_exit| Some(!close_on_exit))
                    .unwrap_or(true);
                let hold_on_start = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "start_suspended"))
                    .unwrap_or(false);
                let floating = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "floating"))
                    .unwrap_or(false);
                let in_place = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "in_place"))
                    .unwrap_or(false);
                let run_command_action = RunCommandAction {
                    command: PathBuf::from(command),
                    args,
                    cwd,
                    direction,
                    hold_on_close,
                    hold_on_start,
                };
                let x = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "x"))
                    .map(|s| s.to_owned());
                let y = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "y"))
                    .map(|s| s.to_owned());
                let width = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "width"))
                    .map(|s| s.to_owned());
                let height = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "height"))
                    .map(|s| s.to_owned());
                if floating {
                    Ok(Action::NewFloatingPane(
                        Some(run_command_action),
                        name,
                        FloatingPaneCoordinates::new(x, y, width, height),
                    ))
                } else if in_place {
                    Ok(Action::NewInPlacePane(Some(run_command_action), name))
                } else {
                    Ok(Action::NewTiledPane(
                        direction,
                        Some(run_command_action),
                        name,
                    ))
                }
            },
            "LaunchOrFocusPlugin" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_strings(arguments)?;
                if args.is_empty() {
                    return Err(ConfigError::new_kdl_error(
                        "No plugin found to launch in LaunchOrFocusPlugin".into(),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    ));
                }
                let plugin_path = args.remove(0);

                let command_metadata = action_children.iter().next();
                let should_float = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "floating"))
                    .unwrap_or(false);
                let move_to_focused_tab = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "move_to_focused_tab"))
                    .unwrap_or(false);
                let should_open_in_place = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "in_place"))
                    .unwrap_or(false);
                let skip_plugin_cache = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "skip_plugin_cache"))
                    .unwrap_or(false);
                let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let configuration = KdlLayoutParser::parse_plugin_user_configuration(&kdl_action)?;
                let initial_cwd = kdl_get_string_property_or_child_value!(kdl_action, "cwd")
                    .map(|s| PathBuf::from(s));
                let run_plugin_or_alias = RunPluginOrAlias::from_url(
                    &plugin_path,
                    &Some(configuration.inner().clone()),
                    None,
                    Some(current_dir),
                )
                .map_err(|e| {
                    ConfigError::new_kdl_error(
                        format!("Failed to parse plugin: {}", e),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    )
                })?
                .with_initial_cwd(initial_cwd);
                Ok(Action::LaunchOrFocusPlugin(
                    run_plugin_or_alias,
                    should_float,
                    move_to_focused_tab,
                    should_open_in_place,
                    skip_plugin_cache,
                ))
            },
            "LaunchPlugin" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_strings(arguments)?;
                if args.is_empty() {
                    return Err(ConfigError::new_kdl_error(
                        "No plugin found to launch in LaunchPlugin".into(),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    ));
                }
                let plugin_path = args.remove(0);

                let command_metadata = action_children.iter().next();
                let should_float = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "floating"))
                    .unwrap_or(false);
                let should_open_in_place = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "in_place"))
                    .unwrap_or(false);
                let skip_plugin_cache = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "skip_plugin_cache"))
                    .unwrap_or(false);
                let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let configuration = KdlLayoutParser::parse_plugin_user_configuration(&kdl_action)?;
                let run_plugin_or_alias = RunPluginOrAlias::from_url(
                    &plugin_path,
                    &Some(configuration.inner().clone()),
                    None,
                    Some(current_dir),
                )
                .map_err(|e| {
                    ConfigError::new_kdl_error(
                        format!("Failed to parse plugin: {}", e),
                        kdl_action.span().offset(),
                        kdl_action.span().len(),
                    )
                })?;
                Ok(Action::LaunchPlugin(
                    run_plugin_or_alias,
                    should_float,
                    should_open_in_place,
                    skip_plugin_cache,
                    None, // we explicitly do not send the current dir here so that it will be
                          // filled from the active pane == better UX
                ))
            },
            "PreviousSwapLayout" => Ok(Action::PreviousSwapLayout),
            "NextSwapLayout" => Ok(Action::NextSwapLayout),
            "BreakPane" => Ok(Action::BreakPane),
            "BreakPaneRight" => Ok(Action::BreakPaneRight),
            "BreakPaneLeft" => Ok(Action::BreakPaneLeft),
            "RenameSession" => parse_kdl_action_char_or_string_arguments!(
                action_name,
                action_arguments,
                kdl_action
            ),
            "MessagePlugin" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_strings(arguments)?;
                let plugin_path = if args.is_empty() {
                    None
                } else {
                    Some(args.remove(0))
                };

                let command_metadata = action_children.iter().next();
                let launch_new = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "launch_new"))
                    .unwrap_or(false);
                let skip_cache = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "skip_cache"))
                    .unwrap_or(false);
                let should_float = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "floating"))
                    .unwrap_or(false);
                let name = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "name"))
                    .map(|n| n.to_owned());
                let payload = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "payload"))
                    .map(|p| p.to_owned());
                let title = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "title"))
                    .map(|t| t.to_owned());
                let configuration = KdlLayoutParser::parse_plugin_user_configuration(&kdl_action)?;
                let configuration = if configuration.inner().is_empty() {
                    None
                } else {
                    Some(configuration.inner().clone())
                };
                let cwd = kdl_get_string_property_or_child_value!(kdl_action, "cwd")
                    .map(|s| PathBuf::from(s));

                let name = name
                    // first we try to take the explicitly supplied message name
                    // then we use the plugin, to facilitate using aliases
                    .or_else(|| plugin_path.clone())
                    // then we use a uuid to at least have some sort of identifier for this message
                    .or_else(|| Some(Uuid::new_v4().to_string()));

                Ok(Action::KeybindPipe {
                    name,
                    payload,
                    args: None, // TODO: consider supporting this if there's a need
                    plugin: plugin_path,
                    configuration,
                    launch_new,
                    skip_cache,
                    floating: Some(should_float),
                    in_place: None, // TODO: support this
                    cwd,
                    pane_title: title,
                })
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
        $kdl_node.get($name).or_else(|| {
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
        let default_cwd = kdl_property_first_arg_as_string_or_error!(kdl_options, "default_cwd")
            .map(|(string, _entry)| PathBuf::from(string));
        let pane_frames =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "pane_frames").map(|(v, _)| v);
        let auto_layout =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "auto_layout").map(|(v, _)| v);
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
        let session_serialization =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "session_serialization")
                .map(|(v, _)| v);
        let serialize_pane_viewport =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "serialize_pane_viewport")
                .map(|(v, _)| v);
        let scrollback_lines_to_serialize =
            kdl_property_first_arg_as_i64_or_error!(kdl_options, "scrollback_lines_to_serialize")
                .map(|(v, _)| v as usize);
        let styled_underlines =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "styled_underlines")
                .map(|(v, _)| v);
        let serialization_interval =
            kdl_property_first_arg_as_i64_or_error!(kdl_options, "serialization_interval")
                .map(|(scroll_buffer_size, _entry)| scroll_buffer_size as u64);
        let disable_session_metadata =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "disable_session_metadata")
                .map(|(v, _)| v);
        Ok(Options {
            simplified_ui,
            theme,
            default_mode,
            default_shell,
            default_cwd,
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
            auto_layout,
            session_serialization,
            serialize_pane_viewport,
            scrollback_lines_to_serialize,
            styled_underlines,
            serialization_interval,
            disable_session_metadata,
        })
    }
}

impl Layout {
    pub fn from_kdl(
        raw_layout: &str,
        file_name: String,
        raw_swap_layouts: Option<(&str, &str)>, // raw_swap_layouts swap_layouts_file_name
        cwd: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let mut kdl_layout_parser = KdlLayoutParser::new(raw_layout, cwd, file_name.clone());
        let layout = kdl_layout_parser.parse().map_err(|e| match e {
            ConfigError::KdlError(kdl_error) => {
                ConfigError::KdlError(kdl_error.add_src(file_name, String::from(raw_layout)))
            },
            ConfigError::KdlDeserializationError(kdl_error) => {
                kdl_layout_error(kdl_error, file_name, raw_layout)
            },
            e => e,
        })?;
        match raw_swap_layouts {
            Some((raw_swap_layout_filename, raw_swap_layout)) => {
                // here we use the same parser to parse the swap layout so that we can reuse assets
                // (eg. pane and tab templates)
                kdl_layout_parser
                    .parse_external_swap_layouts(raw_swap_layout, layout)
                    .map_err(|e| match e {
                        ConfigError::KdlError(kdl_error) => {
                            ConfigError::KdlError(kdl_error.add_src(
                                String::from(raw_swap_layout_filename),
                                String::from(raw_swap_layout),
                            ))
                        },
                        ConfigError::KdlDeserializationError(kdl_error) => kdl_layout_error(
                            kdl_error,
                            raw_swap_layout_filename.into(),
                            raw_swap_layout,
                        ),
                        e => e,
                    })
            },
            None => Ok(layout),
        }
    }
}

fn kdl_layout_error(kdl_error: kdl::KdlError, file_name: String, raw_layout: &str) -> ConfigError {
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
        help_message: None,
    };
    ConfigError::KdlError(kdl_error)
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
        config_options: &Options,
    ) -> Result<(), ConfigError> {
        let all_nodes = kdl_children_nodes_or_error!(block, "no keybinding block for mode");
        let bind_nodes = all_nodes.iter().filter(|n| kdl_name!(n) == "bind");
        let unbind_nodes = all_nodes.iter().filter(|n| kdl_name!(n) == "unbind");
        for key_block in bind_nodes {
            Keybinds::bind_actions_for_each_key(key_block, input_mode_keybinds, config_options)?;
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
    pub fn from_kdl(
        kdl_keybinds: &KdlNode,
        base_keybinds: Keybinds,
        config_options: &Options,
    ) -> Result<Self, ConfigError> {
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
                    Keybinds::bind_keys_in_block(block, &mut input_mode_keybinds, config_options)?;
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
                    Keybinds::bind_keys_in_block(block, &mut input_mode_keybinds, config_options)?;
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
            Keybinds::bind_keys_in_block(mode, &mut input_mode_keybinds, config_options)?;
        }
        if let Some(global_unbind) = kdl_keybinds.children().and_then(|c| c.get("unbind")) {
            Keybinds::unbind_keys_in_all_modes(global_unbind, &mut keybinds_from_config)?;
        };
        Ok(keybinds_from_config)
    }
    fn bind_actions_for_each_key(
        key_block: &KdlNode,
        input_mode_keybinds: &mut HashMap<Key, Vec<Action>>,
        config_options: &Options,
    ) -> Result<(), ConfigError> {
        let keys: Vec<Key> = keys_from_kdl!(key_block);
        let actions: Vec<Action> = actions_from_kdl!(key_block, config_options);
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

impl Config {
    pub fn from_kdl(kdl_config: &str, base_config: Option<Config>) -> Result<Config, ConfigError> {
        let mut config = base_config.unwrap_or_else(|| Config::default());
        let kdl_config: KdlDocument = kdl_config.parse()?;

        let config_options = Options::from_kdl(&kdl_config)?;
        config.options = config.options.merge(config_options);

        // TODO: handle cases where we have more than one of these blocks (eg. two "keybinds")
        // this should give an informative parsing error
        if let Some(kdl_keybinds) = kdl_config.get("keybinds") {
            config.keybinds = Keybinds::from_kdl(&kdl_keybinds, config.keybinds, &config.options)?;
        }
        if let Some(kdl_themes) = kdl_config.get("themes") {
            let config_themes = Themes::from_kdl(kdl_themes)?;
            config.themes = config.themes.merge(config_themes);
        }
        if let Some(kdl_plugin_aliases) = kdl_config.get("plugins") {
            let config_plugins = PluginAliases::from_kdl(kdl_plugin_aliases)?;
            config.plugins.merge(config_plugins);
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

impl PluginAliases {
    pub fn from_kdl(kdl_plugin_aliases: &KdlNode) -> Result<PluginAliases, ConfigError> {
        let mut aliases: BTreeMap<String, RunPlugin> = BTreeMap::new();
        if let Some(kdl_plugin_aliases) = kdl_children_nodes!(kdl_plugin_aliases) {
            for alias_definition in kdl_plugin_aliases {
                let alias_name = kdl_name!(alias_definition);
                if let Some(string_url) =
                    kdl_get_string_property_or_child_value!(alias_definition, "location")
                {
                    let configuration =
                        KdlLayoutParser::parse_plugin_user_configuration(&alias_definition)?;
                    let initial_cwd =
                        kdl_get_string_property_or_child_value!(alias_definition, "cwd")
                            .map(|s| PathBuf::from(s));
                    let run_plugin = RunPlugin::from_url(string_url)?
                        .with_configuration(configuration.inner().clone())
                        .with_initial_cwd(initial_cwd);
                    aliases.insert(alias_name.to_owned(), run_plugin);
                }
            }
        }
        Ok(PluginAliases { aliases })
    }
}

impl UiConfig {
    pub fn from_kdl(kdl_ui_config: &KdlNode) -> Result<UiConfig, ConfigError> {
        let mut ui_config = UiConfig::default();
        if let Some(pane_frames) = kdl_get_child!(kdl_ui_config, "pane_frames") {
            let rounded_corners =
                kdl_children_property_first_arg_as_bool!(pane_frames, "rounded_corners")
                    .unwrap_or(false);
            let hide_session_name =
                kdl_get_child_entry_bool_value!(pane_frames, "hide_session_name").unwrap_or(false);
            let frame_config = FrameConfig {
                rounded_corners,
                hide_session_name,
            };
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

    pub fn from_string(raw_string: &String) -> Result<Self, ConfigError> {
        let kdl_config: KdlDocument = raw_string.parse()?;
        let kdl_themes = kdl_config.get("themes").ok_or(ConfigError::new_kdl_error(
            "No theme node found in file".into(),
            kdl_config.span().offset(),
            kdl_config.span().len(),
        ))?;
        let all_themes_in_file = Themes::from_kdl(kdl_themes)?;
        Ok(all_themes_in_file)
    }

    pub fn from_path(path_to_theme_file: PathBuf) -> Result<Self, ConfigError> {
        // String is the theme name
        let kdl_config = std::fs::read_to_string(&path_to_theme_file)
            .map_err(|e| ConfigError::IoPath(e, path_to_theme_file.clone()))?;
        Themes::from_string(&kdl_config).map_err(|e| match e {
            ConfigError::KdlError(kdl_error) => ConfigError::KdlError(
                kdl_error.add_src(path_to_theme_file.display().to_string(), kdl_config),
            ),
            e => e,
        })
    }

    pub fn from_dir(path_to_theme_dir: PathBuf) -> Result<Self, ConfigError> {
        let mut themes = Themes::default();
        for entry in std::fs::read_dir(&path_to_theme_dir)
            .map_err(|e| ConfigError::IoPath(e, path_to_theme_dir.clone()))?
        {
            let entry = entry.map_err(|e| ConfigError::IoPath(e, path_to_theme_dir.clone()))?;
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "kdl" {
                    themes = themes.merge(Themes::from_path(path)?);
                }
            }
        }
        Ok(themes)
    }
}

impl PermissionCache {
    pub fn from_string(raw_string: String) -> Result<GrantedPermission, ConfigError> {
        let kdl_document: KdlDocument = raw_string.parse()?;

        let mut granted_permission = GrantedPermission::default();

        for node in kdl_document.nodes() {
            if let Some(children) = node.children() {
                let key = kdl_name!(node);
                let permissions: Vec<PermissionType> = children
                    .nodes()
                    .iter()
                    .filter_map(|p| {
                        let v = kdl_name!(p);
                        PermissionType::from_str(v).ok()
                    })
                    .collect();

                granted_permission.insert(key.into(), permissions);
            }
        }

        Ok(granted_permission)
    }

    pub fn to_string(granted: &GrantedPermission) -> String {
        let mut kdl_doucment = KdlDocument::new();

        granted.iter().for_each(|(k, v)| {
            let mut node = KdlNode::new(k.as_str());
            let mut children = KdlDocument::new();

            let permissions: HashSet<PermissionType> = v.clone().into_iter().collect();
            permissions.iter().for_each(|f| {
                let n = KdlNode::new(f.to_string().as_str());
                children.nodes_mut().push(n);
            });

            node.set_children(children);
            kdl_doucment.nodes_mut().push(node);
        });

        kdl_doucment.fmt();
        kdl_doucment.to_string()
    }
}

impl SessionInfo {
    pub fn from_string(raw_session_info: &str, current_session_name: &str) -> Result<Self, String> {
        let kdl_document: KdlDocument = raw_session_info
            .parse()
            .map_err(|e| format!("Failed to parse kdl document: {}", e))?;
        let name = kdl_document
            .get("name")
            .and_then(|n| n.entries().iter().next())
            .and_then(|e| e.value().as_string())
            .map(|s| s.to_owned())
            .ok_or("Failed to parse session name")?;
        let connected_clients = kdl_document
            .get("connected_clients")
            .and_then(|n| n.entries().iter().next())
            .and_then(|e| e.value().as_i64())
            .map(|c| c as usize)
            .ok_or("Failed to parse connected_clients")?;
        let tabs: Vec<TabInfo> = kdl_document
            .get("tabs")
            .and_then(|t| t.children())
            .and_then(|c| {
                let mut tab_nodes = vec![];
                for tab_node in c.nodes() {
                    if let Some(tab) = tab_node.children() {
                        tab_nodes.push(TabInfo::decode_from_kdl(tab).ok()?);
                    }
                }
                Some(tab_nodes)
            })
            .ok_or("Failed to parse tabs")?;
        let panes: PaneManifest = kdl_document
            .get("panes")
            .and_then(|p| p.children())
            .map(|p| PaneManifest::decode_from_kdl(p))
            .ok_or("Failed to parse panes")?;
        let available_layouts: Vec<LayoutInfo> = kdl_document
            .get("available_layouts")
            .and_then(|p| p.children())
            .map(|e| {
                e.nodes()
                    .iter()
                    .filter_map(|n| {
                        let layout_name = n.name().value().to_owned();
                        let layout_source = n
                            .entries()
                            .iter()
                            .find(|e| e.name().map(|n| n.value()) == Some("source"))
                            .and_then(|e| e.value().as_string());
                        match layout_source {
                            Some(layout_source) => match layout_source {
                                "built-in" => Some(LayoutInfo::BuiltIn(layout_name)),
                                "file" => Some(LayoutInfo::File(layout_name)),
                                _ => None,
                            },
                            None => None,
                        }
                    })
                    .collect()
            })
            .ok_or("Failed to parse available_layouts")?;
        let is_current_session = name == current_session_name;
        Ok(SessionInfo {
            name,
            tabs,
            panes,
            connected_clients,
            is_current_session,
            available_layouts,
        })
    }
    pub fn to_string(&self) -> String {
        let mut kdl_document = KdlDocument::new();

        let mut name = KdlNode::new("name");
        name.push(self.name.clone());

        let mut connected_clients = KdlNode::new("connected_clients");
        connected_clients.push(self.connected_clients as i64);

        let mut tabs = KdlNode::new("tabs");
        let mut tab_children = KdlDocument::new();
        for tab_info in &self.tabs {
            let mut tab = KdlNode::new("tab");
            let kdl_tab_info = tab_info.encode_to_kdl();
            tab.set_children(kdl_tab_info);
            tab_children.nodes_mut().push(tab);
        }
        tabs.set_children(tab_children);

        let mut panes = KdlNode::new("panes");
        panes.set_children(self.panes.encode_to_kdl());

        let mut available_layouts = KdlNode::new("available_layouts");
        let mut available_layouts_children = KdlDocument::new();
        for layout_info in &self.available_layouts {
            let (layout_name, layout_source) = match layout_info {
                LayoutInfo::File(name) => (name.clone(), "file"),
                LayoutInfo::BuiltIn(name) => (name.clone(), "built-in"),
            };
            let mut layout_node = KdlNode::new(format!("{}", layout_name));
            let layout_source = KdlEntry::new_prop("source", layout_source);
            layout_node.entries_mut().push(layout_source);
            available_layouts_children.nodes_mut().push(layout_node);
        }
        available_layouts.set_children(available_layouts_children);

        kdl_document.nodes_mut().push(name);
        kdl_document.nodes_mut().push(tabs);
        kdl_document.nodes_mut().push(panes);
        kdl_document.nodes_mut().push(connected_clients);
        kdl_document.nodes_mut().push(available_layouts);
        kdl_document.fmt();
        kdl_document.to_string()
    }
}

impl TabInfo {
    pub fn decode_from_kdl(kdl_document: &KdlDocument) -> Result<Self, String> {
        macro_rules! int_node {
            ($name:expr, $type:ident) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_i64())
                    .map(|e| e as $type)
                    .ok_or(format!("Failed to parse tab {}", $name))?
            }};
        }
        macro_rules! string_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_owned())
                    .ok_or(format!("Failed to parse tab {}", $name))?
            }};
        }
        macro_rules! optional_string_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_owned())
            }};
        }
        macro_rules! bool_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_bool())
                    .ok_or(format!("Failed to parse tab {}", $name))?
            }};
        }

        let position = int_node!("position", usize);
        let name = string_node!("name");
        let active = bool_node!("active");
        let panes_to_hide = int_node!("panes_to_hide", usize);
        let is_fullscreen_active = bool_node!("is_fullscreen_active");
        let is_sync_panes_active = bool_node!("is_sync_panes_active");
        let are_floating_panes_visible = bool_node!("are_floating_panes_visible");
        let mut other_focused_clients = vec![];
        if let Some(tab_other_focused_clients) = kdl_document
            .get("other_focused_clients")
            .map(|n| n.entries())
        {
            for entry in tab_other_focused_clients {
                if let Some(entry_parsed) = entry.value().as_i64() {
                    other_focused_clients.push(entry_parsed as u16);
                }
            }
        }
        let active_swap_layout_name = optional_string_node!("active_swap_layout_name");
        let is_swap_layout_dirty = bool_node!("is_swap_layout_dirty");
        Ok(TabInfo {
            position,
            name,
            active,
            panes_to_hide,
            is_fullscreen_active,
            is_sync_panes_active,
            are_floating_panes_visible,
            other_focused_clients,
            active_swap_layout_name,
            is_swap_layout_dirty,
        })
    }
    pub fn encode_to_kdl(&self) -> KdlDocument {
        let mut kdl_doucment = KdlDocument::new();

        let mut position = KdlNode::new("position");
        position.push(self.position as i64);
        kdl_doucment.nodes_mut().push(position);

        let mut name = KdlNode::new("name");
        name.push(self.name.clone());
        kdl_doucment.nodes_mut().push(name);

        let mut active = KdlNode::new("active");
        active.push(self.active);
        kdl_doucment.nodes_mut().push(active);

        let mut panes_to_hide = KdlNode::new("panes_to_hide");
        panes_to_hide.push(self.panes_to_hide as i64);
        kdl_doucment.nodes_mut().push(panes_to_hide);

        let mut is_fullscreen_active = KdlNode::new("is_fullscreen_active");
        is_fullscreen_active.push(self.is_fullscreen_active);
        kdl_doucment.nodes_mut().push(is_fullscreen_active);

        let mut is_sync_panes_active = KdlNode::new("is_sync_panes_active");
        is_sync_panes_active.push(self.is_sync_panes_active);
        kdl_doucment.nodes_mut().push(is_sync_panes_active);

        let mut are_floating_panes_visible = KdlNode::new("are_floating_panes_visible");
        are_floating_panes_visible.push(self.are_floating_panes_visible);
        kdl_doucment.nodes_mut().push(are_floating_panes_visible);

        if !self.other_focused_clients.is_empty() {
            let mut other_focused_clients = KdlNode::new("other_focused_clients");
            for client_id in &self.other_focused_clients {
                other_focused_clients.push(*client_id as i64);
            }
            kdl_doucment.nodes_mut().push(other_focused_clients);
        }

        if let Some(active_swap_layout_name) = self.active_swap_layout_name.as_ref() {
            let mut active_swap_layout = KdlNode::new("active_swap_layout_name");
            active_swap_layout.push(active_swap_layout_name.to_string());
            kdl_doucment.nodes_mut().push(active_swap_layout);
        }

        let mut is_swap_layout_dirty = KdlNode::new("is_swap_layout_dirty");
        is_swap_layout_dirty.push(self.is_swap_layout_dirty);
        kdl_doucment.nodes_mut().push(is_swap_layout_dirty);

        kdl_doucment
    }
}

impl PaneManifest {
    pub fn decode_from_kdl(kdl_doucment: &KdlDocument) -> Self {
        let mut panes: HashMap<usize, Vec<PaneInfo>> = HashMap::new();
        for node in kdl_doucment.nodes() {
            if node.name().to_string() == "pane" {
                if let Some(pane_document) = node.children() {
                    if let Ok((tab_position, pane_info)) = PaneInfo::decode_from_kdl(pane_document)
                    {
                        let panes_in_tab_position =
                            panes.entry(tab_position).or_insert_with(Vec::new);
                        panes_in_tab_position.push(pane_info);
                    }
                }
            }
        }
        PaneManifest { panes }
    }
    pub fn encode_to_kdl(&self) -> KdlDocument {
        let mut kdl_doucment = KdlDocument::new();
        for (tab_position, panes) in &self.panes {
            for pane in panes {
                let mut pane_node = KdlNode::new("pane");
                let mut pane = pane.encode_to_kdl();

                let mut position_node = KdlNode::new("tab_position");
                position_node.push(*tab_position as i64);
                pane.nodes_mut().push(position_node);

                pane_node.set_children(pane);
                kdl_doucment.nodes_mut().push(pane_node);
            }
        }
        kdl_doucment
    }
}

impl PaneInfo {
    pub fn decode_from_kdl(kdl_document: &KdlDocument) -> Result<(usize, Self), String> {
        // usize is the tab position
        macro_rules! int_node {
            ($name:expr, $type:ident) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_i64())
                    .map(|e| e as $type)
                    .ok_or(format!("Failed to parse pane {}", $name))?
            }};
        }
        macro_rules! optional_int_node {
            ($name:expr, $type:ident) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_i64())
                    .map(|e| e as $type)
            }};
        }
        macro_rules! bool_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_bool())
                    .ok_or(format!("Failed to parse pane {}", $name))?
            }};
        }
        macro_rules! string_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_owned())
                    .ok_or(format!("Failed to parse pane {}", $name))?
            }};
        }
        macro_rules! optional_string_node {
            ($name:expr) => {{
                kdl_document
                    .get($name)
                    .and_then(|n| n.entries().iter().next())
                    .and_then(|e| e.value().as_string())
                    .map(|s| s.to_owned())
            }};
        }
        let tab_position = int_node!("tab_position", usize);
        let id = int_node!("id", u32);

        let is_plugin = bool_node!("is_plugin");
        let is_focused = bool_node!("is_focused");
        let is_fullscreen = bool_node!("is_fullscreen");
        let is_floating = bool_node!("is_floating");
        let is_suppressed = bool_node!("is_suppressed");
        let title = string_node!("title");
        let exited = bool_node!("exited");
        let exit_status = optional_int_node!("exit_status", i32);
        let is_held = bool_node!("is_held");
        let pane_x = int_node!("pane_x", usize);
        let pane_content_x = int_node!("pane_content_x", usize);
        let pane_y = int_node!("pane_y", usize);
        let pane_content_y = int_node!("pane_content_y", usize);
        let pane_rows = int_node!("pane_rows", usize);
        let pane_content_rows = int_node!("pane_content_rows", usize);
        let pane_columns = int_node!("pane_columns", usize);
        let pane_content_columns = int_node!("pane_content_columns", usize);
        let cursor_coordinates_in_pane = kdl_document
            .get("cursor_coordinates_in_pane")
            .map(|n| {
                let mut entries = n.entries().iter();
                (entries.next(), entries.next())
            })
            .and_then(|(x, y)| {
                let x = x.and_then(|x| x.value().as_i64()).map(|x| x as usize);
                let y = y.and_then(|y| y.value().as_i64()).map(|y| y as usize);
                match (x, y) {
                    (Some(x), Some(y)) => Some((x, y)),
                    _ => None,
                }
            });
        let terminal_command = optional_string_node!("terminal_command");
        let plugin_url = optional_string_node!("plugin_url");
        let is_selectable = bool_node!("is_selectable");

        let pane_info = PaneInfo {
            id,
            is_plugin,
            is_focused,
            is_fullscreen,
            is_floating,
            is_suppressed,
            title,
            exited,
            exit_status,
            is_held,
            pane_x,
            pane_content_x,
            pane_y,
            pane_content_y,
            pane_rows,
            pane_content_rows,
            pane_columns,
            pane_content_columns,
            cursor_coordinates_in_pane,
            terminal_command,
            plugin_url,
            is_selectable,
        };
        Ok((tab_position, pane_info))
    }
    pub fn encode_to_kdl(&self) -> KdlDocument {
        let mut kdl_doucment = KdlDocument::new();
        macro_rules! int_node {
            ($name:expr, $val:expr) => {{
                let mut att = KdlNode::new($name);
                att.push($val as i64);
                kdl_doucment.nodes_mut().push(att);
            }};
        }
        macro_rules! bool_node {
            ($name:expr, $val:expr) => {{
                let mut att = KdlNode::new($name);
                att.push($val);
                kdl_doucment.nodes_mut().push(att);
            }};
        }
        macro_rules! string_node {
            ($name:expr, $val:expr) => {{
                let mut att = KdlNode::new($name);
                att.push($val);
                kdl_doucment.nodes_mut().push(att);
            }};
        }

        int_node!("id", self.id);
        bool_node!("is_plugin", self.is_plugin);
        bool_node!("is_focused", self.is_focused);
        bool_node!("is_fullscreen", self.is_fullscreen);
        bool_node!("is_floating", self.is_floating);
        bool_node!("is_suppressed", self.is_suppressed);
        string_node!("title", self.title.to_string());
        bool_node!("exited", self.exited);
        if let Some(exit_status) = self.exit_status {
            int_node!("exit_status", exit_status);
        }
        bool_node!("is_held", self.is_held);
        int_node!("pane_x", self.pane_x);
        int_node!("pane_content_x", self.pane_content_x);
        int_node!("pane_y", self.pane_y);
        int_node!("pane_content_y", self.pane_content_y);
        int_node!("pane_rows", self.pane_rows);
        int_node!("pane_content_rows", self.pane_content_rows);
        int_node!("pane_columns", self.pane_columns);
        int_node!("pane_content_columns", self.pane_content_columns);
        if let Some((cursor_x, cursor_y)) = self.cursor_coordinates_in_pane {
            let mut cursor_coordinates_in_pane = KdlNode::new("cursor_coordinates_in_pane");
            cursor_coordinates_in_pane.push(cursor_x as i64);
            cursor_coordinates_in_pane.push(cursor_y as i64);
            kdl_doucment.nodes_mut().push(cursor_coordinates_in_pane);
        }
        if let Some(terminal_command) = &self.terminal_command {
            string_node!("terminal_command", terminal_command.to_string());
        }
        if let Some(plugin_url) = &self.plugin_url {
            string_node!("plugin_url", plugin_url.to_string());
        }
        bool_node!("is_selectable", self.is_selectable);
        kdl_doucment
    }
}

pub fn parse_plugin_user_configuration(
    plugin_block: &KdlNode,
) -> Result<BTreeMap<String, String>, ConfigError> {
    let mut configuration = BTreeMap::new();
    for user_configuration_entry in plugin_block.entries() {
        let name = user_configuration_entry.name();
        let value = user_configuration_entry.value();
        if let Some(name) = name {
            let name = name.to_string();
            if KdlLayoutParser::is_a_reserved_plugin_property(&name) {
                continue;
            }
            configuration.insert(name, value.to_string());
        }
    }
    if let Some(user_config) = kdl_children_nodes!(plugin_block) {
        for user_configuration_entry in user_config {
            let config_entry_name = kdl_name!(user_configuration_entry);
            if KdlLayoutParser::is_a_reserved_plugin_property(&config_entry_name) {
                continue;
            }
            let config_entry_str_value = kdl_first_entry_as_string!(user_configuration_entry)
                .map(|s| format!("{}", s.to_string()));
            let config_entry_int_value = kdl_first_entry_as_i64!(user_configuration_entry)
                .map(|s| format!("{}", s.to_string()));
            let config_entry_bool_value = kdl_first_entry_as_bool!(user_configuration_entry)
                .map(|s| format!("{}", s.to_string()));
            let config_entry_children = user_configuration_entry
                .children()
                .map(|s| format!("{}", s.to_string().trim()));
            let config_entry_value = config_entry_str_value
                .or(config_entry_int_value)
                .or(config_entry_bool_value)
                .or(config_entry_children)
                .ok_or(ConfigError::new_kdl_error(
                    format!(
                        "Failed to parse plugin block configuration: {:?}",
                        user_configuration_entry
                    ),
                    plugin_block.span().offset(),
                    plugin_block.span().len(),
                ))?;
            configuration.insert(config_entry_name.into(), config_entry_value);
        }
    }
    Ok(configuration)
}

#[test]
fn serialize_and_deserialize_session_info() {
    let session_info = SessionInfo::default();
    let serialized = session_info.to_string();
    let deserealized = SessionInfo::from_string(&serialized, "not this session").unwrap();
    assert_eq!(session_info, deserealized);
    insta::assert_snapshot!(serialized);
}

#[test]
fn serialize_and_deserialize_session_info_with_data() {
    let panes_list = vec![
        PaneInfo {
            id: 1,
            is_plugin: false,
            is_focused: true,
            is_fullscreen: true,
            is_floating: false,
            is_suppressed: false,
            title: "pane 1".to_owned(),
            exited: false,
            exit_status: None,
            is_held: false,
            pane_x: 0,
            pane_content_x: 1,
            pane_y: 0,
            pane_content_y: 1,
            pane_rows: 5,
            pane_content_rows: 4,
            pane_columns: 22,
            pane_content_columns: 21,
            cursor_coordinates_in_pane: Some((0, 0)),
            terminal_command: Some("foo".to_owned()),
            plugin_url: None,
            is_selectable: true,
        },
        PaneInfo {
            id: 1,
            is_plugin: true,
            is_focused: true,
            is_fullscreen: true,
            is_floating: false,
            is_suppressed: false,
            title: "pane 1".to_owned(),
            exited: false,
            exit_status: None,
            is_held: false,
            pane_x: 0,
            pane_content_x: 1,
            pane_y: 0,
            pane_content_y: 1,
            pane_rows: 5,
            pane_content_rows: 4,
            pane_columns: 22,
            pane_content_columns: 21,
            cursor_coordinates_in_pane: Some((0, 0)),
            terminal_command: None,
            plugin_url: Some("i_am_a_fake_plugin".to_owned()),
            is_selectable: true,
        },
    ];
    let mut panes = HashMap::new();
    panes.insert(0, panes_list);
    let session_info = SessionInfo {
        name: "my session name".to_owned(),
        tabs: vec![
            TabInfo {
                position: 0,
                name: "tab 1".to_owned(),
                active: true,
                panes_to_hide: 1,
                is_fullscreen_active: true,
                is_sync_panes_active: false,
                are_floating_panes_visible: true,
                other_focused_clients: vec![2, 3],
                active_swap_layout_name: Some("BASE".to_owned()),
                is_swap_layout_dirty: true,
            },
            TabInfo {
                position: 1,
                name: "tab 2".to_owned(),
                active: true,
                panes_to_hide: 0,
                is_fullscreen_active: false,
                is_sync_panes_active: true,
                are_floating_panes_visible: true,
                other_focused_clients: vec![2, 3],
                active_swap_layout_name: None,
                is_swap_layout_dirty: false,
            },
        ],
        panes: PaneManifest { panes },
        connected_clients: 2,
        is_current_session: false,
        available_layouts: vec![
            LayoutInfo::File("layout1".to_owned()),
            LayoutInfo::BuiltIn("layout2".to_owned()),
            LayoutInfo::File("layout3".to_owned()),
        ],
    };
    let serialized = session_info.to_string();
    let deserealized = SessionInfo::from_string(&serialized, "not this session").unwrap();
    assert_eq!(session_info, deserealized);
    insta::assert_snapshot!(serialized);
}
