mod kdl_layout_parser;
use crate::data::{
    BareKey, Direction, FloatingPaneCoordinates, InputMode, KeyWithModifier, LayoutInfo,
    MultiplayerColors, Palette, PaletteColor, PaneInfo, PaneManifest, PermissionType, Resize,
    SessionInfo, StyleDeclaration, Styling, TabInfo, WebSharing, DEFAULT_STYLES,
};
use crate::envs::EnvironmentVariables;
use crate::home::{find_default_config_dir, get_layout_dir};
use crate::input::config::{Config, ConfigError, KdlError};
use crate::input::keybinds::Keybinds;
use crate::input::layout::{
    Layout, PluginUserConfiguration, RunPlugin, RunPluginOrAlias, SplitSize,
};
use crate::input::options::{Clipboard, OnForceClose, Options};
use crate::input::permission::{GrantedPermission, PermissionCache};
use crate::input::plugins::PluginAliases;
use crate::input::theme::{FrameConfig, Theme, Themes, UiConfig};
use crate::input::web_client::WebClientConfig;
use kdl_layout_parser::KdlLayoutParser;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use strum::IntoEnumIterator;
use uuid::Uuid;

use miette::NamedSource;

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

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
                KeyWithModifier::from_str(k).map_err(|_| {
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

pub fn kdl_arguments_that_are_digits<'a>(
    arguments: impl Iterator<Item = &'a KdlEntry>,
) -> Result<Vec<i64>, ConfigError> {
    let mut args: Vec<i64> = vec![];
    for kdl_entry in arguments {
        match kdl_entry.value().as_i64() {
            Some(digit_value) => {
                args.push(digit_value);
            },
            None => {
                return Err(ConfigError::new_kdl_error(
                    format!("Argument must be a digit"),
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
            "Write" => Ok(Action::Write(None, bytes, false)),
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
                    return Ok(Action::NewPane(None, None, false));
                } else if string == "stacked" {
                    return Ok(Action::NewStackedPane(None, None));
                } else {
                    let direction = Direction::from_str(string.as_str()).map_err(|_| {
                        ConfigError::new_kdl_error(
                            format!("Invalid direction: '{}'", string),
                            action_node.span().offset(),
                            action_node.span().len(),
                        )
                    })?;
                    Ok(Action::NewPane(Some(direction), None, false))
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
    pub fn to_kdl(&self) -> Option<KdlNode> {
        match self {
            Action::Quit => Some(KdlNode::new("Quit")),
            Action::Write(_key, bytes, _is_kitty) => {
                let mut node = KdlNode::new("Write");
                for byte in bytes {
                    node.push(KdlValue::Base10(*byte as i64));
                }
                Some(node)
            },
            Action::WriteChars(string) => {
                let mut node = KdlNode::new("WriteChars");
                node.push(string.clone());
                Some(node)
            },
            Action::SwitchToMode(input_mode) => {
                let mut node = KdlNode::new("SwitchToMode");
                node.push(format!("{:?}", input_mode).to_lowercase());
                Some(node)
            },
            Action::Resize(resize, resize_direction) => {
                let mut node = KdlNode::new("Resize");
                let resize = match resize {
                    Resize::Increase => "Increase",
                    Resize::Decrease => "Decrease",
                };
                if let Some(resize_direction) = resize_direction {
                    let resize_direction = match resize_direction {
                        Direction::Left => "left",
                        Direction::Right => "right",
                        Direction::Up => "up",
                        Direction::Down => "down",
                    };
                    node.push(format!("{} {}", resize, resize_direction));
                } else {
                    node.push(format!("{}", resize));
                }
                Some(node)
            },
            Action::FocusNextPane => Some(KdlNode::new("FocusNextPane")),
            Action::FocusPreviousPane => Some(KdlNode::new("FocusPreviousPane")),
            Action::SwitchFocus => Some(KdlNode::new("SwitchFocus")),
            Action::MoveFocus(direction) => {
                let mut node = KdlNode::new("MoveFocus");
                let direction = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                node.push(direction);
                Some(node)
            },
            Action::MoveFocusOrTab(direction) => {
                let mut node = KdlNode::new("MoveFocusOrTab");
                let direction = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                node.push(direction);
                Some(node)
            },
            Action::MovePane(direction) => {
                let mut node = KdlNode::new("MovePane");
                if let Some(direction) = direction {
                    let direction = match direction {
                        Direction::Left => "left",
                        Direction::Right => "right",
                        Direction::Up => "up",
                        Direction::Down => "down",
                    };
                    node.push(direction);
                }
                Some(node)
            },
            Action::MovePaneBackwards => Some(KdlNode::new("MovePaneBackwards")),
            Action::DumpScreen(file, _) => {
                let mut node = KdlNode::new("DumpScreen");
                node.push(file.clone());
                Some(node)
            },
            Action::DumpLayout => Some(KdlNode::new("DumpLayout")),
            Action::EditScrollback => Some(KdlNode::new("EditScrollback")),
            Action::ScrollUp => Some(KdlNode::new("ScrollUp")),
            Action::ScrollDown => Some(KdlNode::new("ScrollDown")),
            Action::ScrollToBottom => Some(KdlNode::new("ScrollToBottom")),
            Action::ScrollToTop => Some(KdlNode::new("ScrollToTop")),
            Action::PageScrollUp => Some(KdlNode::new("PageScrollUp")),
            Action::PageScrollDown => Some(KdlNode::new("PageScrollDown")),
            Action::HalfPageScrollUp => Some(KdlNode::new("HalfPageScrollUp")),
            Action::HalfPageScrollDown => Some(KdlNode::new("HalfPageScrollDown")),
            Action::ToggleFocusFullscreen => Some(KdlNode::new("ToggleFocusFullscreen")),
            Action::TogglePaneFrames => Some(KdlNode::new("TogglePaneFrames")),
            Action::ToggleActiveSyncTab => Some(KdlNode::new("ToggleActiveSyncTab")),
            Action::NewPane(direction, _, _) => {
                let mut node = KdlNode::new("NewPane");
                if let Some(direction) = direction {
                    let direction = match direction {
                        Direction::Left => "left",
                        Direction::Right => "right",
                        Direction::Up => "up",
                        Direction::Down => "down",
                    };
                    node.push(direction);
                }
                Some(node)
            },
            Action::TogglePaneEmbedOrFloating => Some(KdlNode::new("TogglePaneEmbedOrFloating")),
            Action::ToggleFloatingPanes => Some(KdlNode::new("ToggleFloatingPanes")),
            Action::CloseFocus => Some(KdlNode::new("CloseFocus")),
            Action::PaneNameInput(bytes) => {
                let mut node = KdlNode::new("PaneNameInput");
                for byte in bytes {
                    node.push(KdlValue::Base10(*byte as i64));
                }
                Some(node)
            },
            Action::UndoRenamePane => Some(KdlNode::new("UndoRenamePane")),
            Action::NewTab(_, _, _, _, name, should_change_focus_to_new_tab, cwd) => {
                let mut node = KdlNode::new("NewTab");
                let mut children = KdlDocument::new();
                if let Some(name) = name {
                    let mut name_node = KdlNode::new("name");
                    if !should_change_focus_to_new_tab {
                        let mut should_change_focus_to_new_tab_node =
                            KdlNode::new("should_change_focus_to_new_tab");
                        should_change_focus_to_new_tab_node.push(KdlValue::Bool(false));
                        children
                            .nodes_mut()
                            .push(should_change_focus_to_new_tab_node);
                    }
                    name_node.push(name.clone());
                    children.nodes_mut().push(name_node);
                }
                if let Some(cwd) = cwd {
                    let mut cwd_node = KdlNode::new("cwd");
                    cwd_node.push(cwd.display().to_string());
                    children.nodes_mut().push(cwd_node);
                }
                if name.is_some() || cwd.is_some() {
                    node.set_children(children);
                }
                Some(node)
            },
            Action::GoToNextTab => Some(KdlNode::new("GoToNextTab")),
            Action::GoToPreviousTab => Some(KdlNode::new("GoToPreviousTab")),
            Action::CloseTab => Some(KdlNode::new("CloseTab")),
            Action::GoToTab(index) => {
                let mut node = KdlNode::new("GoToTab");
                node.push(KdlValue::Base10(*index as i64));
                Some(node)
            },
            Action::ToggleTab => Some(KdlNode::new("ToggleTab")),
            Action::TabNameInput(bytes) => {
                let mut node = KdlNode::new("TabNameInput");
                for byte in bytes {
                    node.push(KdlValue::Base10(*byte as i64));
                }
                Some(node)
            },
            Action::UndoRenameTab => Some(KdlNode::new("UndoRenameTab")),
            Action::MoveTab(direction) => {
                let mut node = KdlNode::new("MoveTab");
                let direction = match direction {
                    Direction::Left => "left",
                    Direction::Right => "right",
                    Direction::Up => "up",
                    Direction::Down => "down",
                };
                node.push(direction);
                Some(node)
            },
            Action::NewTiledPane(direction, run_command_action, name) => {
                let mut node = KdlNode::new("Run");
                let mut node_children = KdlDocument::new();
                if let Some(run_command_action) = run_command_action {
                    node.push(run_command_action.command.display().to_string());
                    for arg in &run_command_action.args {
                        node.push(arg.clone());
                    }
                    if let Some(cwd) = &run_command_action.cwd {
                        let mut cwd_node = KdlNode::new("cwd");
                        cwd_node.push(cwd.display().to_string());
                        node_children.nodes_mut().push(cwd_node);
                    }
                    if run_command_action.hold_on_start {
                        let mut hos_node = KdlNode::new("hold_on_start");
                        hos_node.push(KdlValue::Bool(true));
                        node_children.nodes_mut().push(hos_node);
                    }
                    if !run_command_action.hold_on_close {
                        let mut hoc_node = KdlNode::new("hold_on_close");
                        hoc_node.push(KdlValue::Bool(false));
                        node_children.nodes_mut().push(hoc_node);
                    }
                }
                if let Some(name) = name {
                    let mut name_node = KdlNode::new("name");
                    name_node.push(name.clone());
                    node_children.nodes_mut().push(name_node);
                }
                if let Some(direction) = direction {
                    let mut direction_node = KdlNode::new("direction");
                    let direction = match direction {
                        Direction::Left => "left",
                        Direction::Right => "right",
                        Direction::Up => "up",
                        Direction::Down => "down",
                    };
                    direction_node.push(direction);
                    node_children.nodes_mut().push(direction_node);
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::NewFloatingPane(run_command_action, name, floating_pane_coordinates) => {
                let mut node = KdlNode::new("Run");
                let mut node_children = KdlDocument::new();
                let mut floating_pane = KdlNode::new("floating");
                floating_pane.push(KdlValue::Bool(true));
                node_children.nodes_mut().push(floating_pane);
                if let Some(run_command_action) = run_command_action {
                    node.push(run_command_action.command.display().to_string());
                    for arg in &run_command_action.args {
                        node.push(arg.clone());
                    }
                    if let Some(cwd) = &run_command_action.cwd {
                        let mut cwd_node = KdlNode::new("cwd");
                        cwd_node.push(cwd.display().to_string());
                        node_children.nodes_mut().push(cwd_node);
                    }
                    if run_command_action.hold_on_start {
                        let mut hos_node = KdlNode::new("hold_on_start");
                        hos_node.push(KdlValue::Bool(true));
                        node_children.nodes_mut().push(hos_node);
                    }
                    if !run_command_action.hold_on_close {
                        let mut hoc_node = KdlNode::new("hold_on_close");
                        hoc_node.push(KdlValue::Bool(false));
                        node_children.nodes_mut().push(hoc_node);
                    }
                }
                if let Some(floating_pane_coordinates) = floating_pane_coordinates {
                    if let Some(x) = floating_pane_coordinates.x {
                        let mut x_node = KdlNode::new("x");
                        match x {
                            SplitSize::Percent(x) => {
                                x_node.push(format!("{}%", x));
                            },
                            SplitSize::Fixed(x) => {
                                x_node.push(KdlValue::Base10(x as i64));
                            },
                        };
                        node_children.nodes_mut().push(x_node);
                    }
                    if let Some(y) = floating_pane_coordinates.y {
                        let mut y_node = KdlNode::new("y");
                        match y {
                            SplitSize::Percent(y) => {
                                y_node.push(format!("{}%", y));
                            },
                            SplitSize::Fixed(y) => {
                                y_node.push(KdlValue::Base10(y as i64));
                            },
                        };
                        node_children.nodes_mut().push(y_node);
                    }
                    if let Some(width) = floating_pane_coordinates.width {
                        let mut width_node = KdlNode::new("width");
                        match width {
                            SplitSize::Percent(width) => {
                                width_node.push(format!("{}%", width));
                            },
                            SplitSize::Fixed(width) => {
                                width_node.push(KdlValue::Base10(width as i64));
                            },
                        };
                        node_children.nodes_mut().push(width_node);
                    }
                    if let Some(height) = floating_pane_coordinates.height {
                        let mut height_node = KdlNode::new("height");
                        match height {
                            SplitSize::Percent(height) => {
                                height_node.push(format!("{}%", height));
                            },
                            SplitSize::Fixed(height) => {
                                height_node.push(KdlValue::Base10(height as i64));
                            },
                        };
                        node_children.nodes_mut().push(height_node);
                    }
                }
                if let Some(name) = name {
                    let mut name_node = KdlNode::new("name");
                    name_node.push(name.clone());
                    node_children.nodes_mut().push(name_node);
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::NewInPlacePane(run_command_action, name) => {
                let mut node = KdlNode::new("Run");
                let mut node_children = KdlDocument::new();
                if let Some(run_command_action) = run_command_action {
                    node.push(run_command_action.command.display().to_string());
                    for arg in &run_command_action.args {
                        node.push(arg.clone());
                    }
                    let mut in_place_node = KdlNode::new("in_place");
                    in_place_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(in_place_node);
                    if let Some(cwd) = &run_command_action.cwd {
                        let mut cwd_node = KdlNode::new("cwd");
                        cwd_node.push(cwd.display().to_string());
                        node_children.nodes_mut().push(cwd_node);
                    }
                    if run_command_action.hold_on_start {
                        let mut hos_node = KdlNode::new("hold_on_start");
                        hos_node.push(KdlValue::Bool(true));
                        node_children.nodes_mut().push(hos_node);
                    }
                    if !run_command_action.hold_on_close {
                        let mut hoc_node = KdlNode::new("hold_on_close");
                        hoc_node.push(KdlValue::Bool(false));
                        node_children.nodes_mut().push(hoc_node);
                    }
                }
                if let Some(name) = name {
                    let mut name_node = KdlNode::new("name");
                    name_node.push(name.clone());
                    node_children.nodes_mut().push(name_node);
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::NewStackedPane(run_command_action, name) => match run_command_action {
                Some(run_command_action) => {
                    let mut node = KdlNode::new("Run");
                    let mut node_children = KdlDocument::new();
                    node.push(run_command_action.command.display().to_string());
                    for arg in &run_command_action.args {
                        node.push(arg.clone());
                    }
                    let mut stacked_node = KdlNode::new("stacked");
                    stacked_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(stacked_node);
                    if let Some(cwd) = &run_command_action.cwd {
                        let mut cwd_node = KdlNode::new("cwd");
                        cwd_node.push(cwd.display().to_string());
                        node_children.nodes_mut().push(cwd_node);
                    }
                    if run_command_action.hold_on_start {
                        let mut hos_node = KdlNode::new("hold_on_start");
                        hos_node.push(KdlValue::Bool(true));
                        node_children.nodes_mut().push(hos_node);
                    }
                    if !run_command_action.hold_on_close {
                        let mut hoc_node = KdlNode::new("hold_on_close");
                        hoc_node.push(KdlValue::Bool(false));
                        node_children.nodes_mut().push(hoc_node);
                    }
                    if let Some(name) = name {
                        let mut name_node = KdlNode::new("name");
                        name_node.push(name.clone());
                        node_children.nodes_mut().push(name_node);
                    }
                    if !node_children.nodes().is_empty() {
                        node.set_children(node_children);
                    }
                    Some(node)
                },
                None => {
                    let mut node = KdlNode::new("NewPane");
                    node.push("stacked");
                    Some(node)
                },
            },
            Action::Detach => Some(KdlNode::new("Detach")),
            Action::LaunchOrFocusPlugin(
                run_plugin_or_alias,
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                skip_plugin_cache,
            ) => {
                let mut node = KdlNode::new("LaunchOrFocusPlugin");
                let mut node_children = KdlDocument::new();
                let location = run_plugin_or_alias.location_string();
                node.push(location);
                if *should_float {
                    let mut should_float_node = KdlNode::new("floating");
                    should_float_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(should_float_node);
                }
                if *move_to_focused_tab {
                    let mut move_to_focused_tab_node = KdlNode::new("move_to_focused_tab");
                    move_to_focused_tab_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(move_to_focused_tab_node);
                }
                if *should_open_in_place {
                    let mut should_open_in_place_node = KdlNode::new("in_place");
                    should_open_in_place_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(should_open_in_place_node);
                }
                if *skip_plugin_cache {
                    let mut skip_plugin_cache_node = KdlNode::new("skip_plugin_cache");
                    skip_plugin_cache_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(skip_plugin_cache_node);
                }
                if let Some(configuration) = run_plugin_or_alias.get_configuration() {
                    for (config_key, config_value) in configuration.inner().iter() {
                        let mut node = KdlNode::new(config_key.clone());
                        node.push(config_value.clone());
                        node_children.nodes_mut().push(node);
                    }
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::LaunchPlugin(
                run_plugin_or_alias,
                should_float,
                should_open_in_place,
                skip_plugin_cache,
                cwd,
            ) => {
                let mut node = KdlNode::new("LaunchPlugin");
                let mut node_children = KdlDocument::new();
                let location = run_plugin_or_alias.location_string();
                node.push(location);
                if *should_float {
                    let mut should_float_node = KdlNode::new("floating");
                    should_float_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(should_float_node);
                }
                if *should_open_in_place {
                    let mut should_open_in_place_node = KdlNode::new("in_place");
                    should_open_in_place_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(should_open_in_place_node);
                }
                if *skip_plugin_cache {
                    let mut skip_plugin_cache_node = KdlNode::new("skip_plugin_cache");
                    skip_plugin_cache_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(skip_plugin_cache_node);
                }
                if let Some(cwd) = &cwd {
                    let mut cwd_node = KdlNode::new("cwd");
                    cwd_node.push(cwd.display().to_string());
                    node_children.nodes_mut().push(cwd_node);
                } else if let Some(cwd) = run_plugin_or_alias.get_initial_cwd() {
                    let mut cwd_node = KdlNode::new("cwd");
                    cwd_node.push(cwd.display().to_string());
                    node_children.nodes_mut().push(cwd_node);
                }
                if let Some(configuration) = run_plugin_or_alias.get_configuration() {
                    for (config_key, config_value) in configuration.inner().iter() {
                        let mut node = KdlNode::new(config_key.clone());
                        node.push(config_value.clone());
                        node_children.nodes_mut().push(node);
                    }
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::Copy => Some(KdlNode::new("Copy")),
            Action::SearchInput(bytes) => {
                let mut node = KdlNode::new("SearchInput");
                for byte in bytes {
                    node.push(KdlValue::Base10(*byte as i64));
                }
                Some(node)
            },
            Action::Search(search_direction) => {
                let mut node = KdlNode::new("Search");
                let direction = match search_direction {
                    SearchDirection::Down => "down",
                    SearchDirection::Up => "up",
                };
                node.push(direction);
                Some(node)
            },
            Action::SearchToggleOption(search_toggle_option) => {
                let mut node = KdlNode::new("SearchToggleOption");
                node.push(format!("{:?}", search_toggle_option));
                Some(node)
            },
            Action::ToggleMouseMode => Some(KdlNode::new("ToggleMouseMode")),
            Action::PreviousSwapLayout => Some(KdlNode::new("PreviousSwapLayout")),
            Action::NextSwapLayout => Some(KdlNode::new("NextSwapLayout")),
            Action::BreakPane => Some(KdlNode::new("BreakPane")),
            Action::BreakPaneRight => Some(KdlNode::new("BreakPaneRight")),
            Action::BreakPaneLeft => Some(KdlNode::new("BreakPaneLeft")),
            Action::KeybindPipe {
                name,
                payload,
                args: _, // currently unsupported
                plugin,
                configuration,
                launch_new,
                skip_cache,
                floating,
                in_place: _, // currently unsupported
                cwd,
                pane_title,
                plugin_id,
            } => {
                if plugin_id.is_some() {
                    log::warn!("Not serializing temporary keybinding MessagePluginId");
                    return None;
                }
                let mut node = KdlNode::new("MessagePlugin");
                let mut node_children = KdlDocument::new();
                if let Some(plugin) = plugin {
                    node.push(plugin.clone());
                }
                if let Some(name) = name {
                    let mut name_node = KdlNode::new("name");
                    name_node.push(name.clone());
                    node_children.nodes_mut().push(name_node);
                }
                if let Some(cwd) = cwd {
                    let mut cwd_node = KdlNode::new("cwd");
                    cwd_node.push(cwd.display().to_string());
                    node_children.nodes_mut().push(cwd_node);
                }
                if let Some(payload) = payload {
                    let mut payload_node = KdlNode::new("payload");
                    payload_node.push(payload.clone());
                    node_children.nodes_mut().push(payload_node);
                }
                if *launch_new {
                    let mut launch_new_node = KdlNode::new("launch_new");
                    launch_new_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(launch_new_node);
                }
                if *skip_cache {
                    let mut skip_cache_node = KdlNode::new("skip_cache");
                    skip_cache_node.push(KdlValue::Bool(true));
                    node_children.nodes_mut().push(skip_cache_node);
                }
                if let Some(floating) = floating {
                    let mut floating_node = KdlNode::new("floating");
                    floating_node.push(KdlValue::Bool(*floating));
                    node_children.nodes_mut().push(floating_node);
                }
                if let Some(title) = pane_title {
                    let mut title_node = KdlNode::new("title");
                    title_node.push(title.clone());
                    node_children.nodes_mut().push(title_node);
                }
                if let Some(configuration) = configuration {
                    // we do this because the constructor removes the relevant config fields from
                    // above, otherwise we would have duplicates
                    let configuration = PluginUserConfiguration::new(configuration.clone());
                    let configuration = configuration.inner();
                    for (config_key, config_value) in configuration.iter() {
                        let mut node = KdlNode::new(config_key.clone());
                        node.push(config_value.clone());
                        node_children.nodes_mut().push(node);
                    }
                }
                if !node_children.nodes().is_empty() {
                    node.set_children(node_children);
                }
                Some(node)
            },
            Action::TogglePanePinned => Some(KdlNode::new("TogglePanePinned")),
            Action::TogglePaneInGroup => Some(KdlNode::new("TogglePaneInGroup")),
            Action::ToggleGroupMarking => Some(KdlNode::new("ToggleGroupMarking")),
            _ => None,
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

impl PaletteColor {
    pub fn to_kdl(&self, color_name: &str) -> KdlNode {
        let mut node = KdlNode::new(color_name);
        match self {
            PaletteColor::Rgb((r, g, b)) => {
                node.push(KdlValue::Base10(*r as i64));
                node.push(KdlValue::Base10(*g as i64));
                node.push(KdlValue::Base10(*b as i64));
            },
            PaletteColor::EightBit(color_index) => {
                node.push(KdlValue::Base10(*color_index as i64));
            },
        }
        node
    }
}

impl StyleDeclaration {
    pub fn to_kdl(&self, declaration_name: &str) -> KdlNode {
        let mut node = KdlNode::new(declaration_name);
        let mut doc = KdlDocument::new();

        doc.nodes_mut().push(self.base.to_kdl("base"));
        doc.nodes_mut().push(self.background.to_kdl("background"));
        doc.nodes_mut().push(self.emphasis_0.to_kdl("emphasis_0"));
        doc.nodes_mut().push(self.emphasis_1.to_kdl("emphasis_1"));
        doc.nodes_mut().push(self.emphasis_2.to_kdl("emphasis_2"));
        doc.nodes_mut().push(self.emphasis_3.to_kdl("emphasis_3"));
        node.set_children(doc);
        node
    }
}

impl MultiplayerColors {
    pub fn to_kdl(&self) -> KdlNode {
        let mut node = KdlNode::new("multiplayer_user_colors");
        let mut doc = KdlDocument::new();
        doc.nodes_mut().push(self.player_1.to_kdl("player_1"));
        doc.nodes_mut().push(self.player_2.to_kdl("player_2"));
        doc.nodes_mut().push(self.player_3.to_kdl("player_3"));
        doc.nodes_mut().push(self.player_4.to_kdl("player_4"));
        doc.nodes_mut().push(self.player_5.to_kdl("player_5"));
        doc.nodes_mut().push(self.player_6.to_kdl("player_6"));
        doc.nodes_mut().push(self.player_7.to_kdl("player_7"));
        doc.nodes_mut().push(self.player_8.to_kdl("player_8"));
        doc.nodes_mut().push(self.player_9.to_kdl("player_9"));
        doc.nodes_mut().push(self.player_10.to_kdl("player_10"));
        node.set_children(doc);
        node
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
                    return Ok(Action::NewTab(None, vec![], None, None, None, true, None));
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
                    cwd.clone(),
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
                    let should_change_focus_to_new_tab = layout.focus.unwrap_or(true);

                    Ok(Action::NewTab(
                        Some(layout),
                        floating_panes_layout,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        name,
                        should_change_focus_to_new_tab,
                        cwd,
                    ))
                } else {
                    let (layout, floating_panes_layout) = layout.new_tab();
                    let should_change_focus_to_new_tab = layout.focus.unwrap_or(true);

                    Ok(Action::NewTab(
                        Some(layout),
                        floating_panes_layout,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        name,
                        should_change_focus_to_new_tab,
                        cwd,
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
                let stacked = command_metadata
                    .and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "stacked"))
                    .unwrap_or(false);
                let run_command_action = RunCommandAction {
                    command: PathBuf::from(command),
                    args,
                    cwd,
                    direction,
                    hold_on_close,
                    hold_on_start,
                    ..Default::default()
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
                let pinned =
                    command_metadata.and_then(|c_m| kdl_child_bool_value_for_entry(c_m, "pinned"));
                if floating {
                    Ok(Action::NewFloatingPane(
                        Some(run_command_action),
                        name,
                        FloatingPaneCoordinates::new(x, y, width, height, pinned),
                    ))
                } else if in_place {
                    Ok(Action::NewInPlacePane(Some(run_command_action), name))
                } else if stacked {
                    Ok(Action::NewStackedPane(Some(run_command_action), name))
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
                    plugin_id: None,
                })
            },
            "MessagePluginId" => {
                let arguments = action_arguments.iter().copied();
                let mut args = kdl_arguments_that_are_digits(arguments)?;
                let plugin_id = if args.is_empty() {
                    None
                } else {
                    Some(args.remove(0) as u32)
                };

                let command_metadata = action_children.iter().next();
                let launch_new = false;
                let skip_cache = false;
                let name = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "name"))
                    .map(|n| n.to_owned());
                let payload = command_metadata
                    .and_then(|c_m| kdl_child_string_value_for_entry(c_m, "payload"))
                    .map(|p| p.to_owned());
                let configuration = None;

                let name = name
                    // if no name is provided, we use a uuid to at least have some sort of identifier for this message
                    .or_else(|| Some(Uuid::new_v4().to_string()));

                Ok(Action::KeybindPipe {
                    name,
                    payload,
                    args: None, // TODO: consider supporting this if there's a need
                    plugin: None,
                    configuration,
                    launch_new,
                    skip_cache,
                    floating: None,
                    in_place: None, // TODO: support this
                    cwd: None,
                    pane_title: None,
                    plugin_id,
                })
            },
            "TogglePanePinned" => Ok(Action::TogglePanePinned),
            "TogglePaneInGroup" => Ok(Action::TogglePaneInGroup),
            "ToggleGroupMarking" => Ok(Action::ToggleGroupMarking),
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
macro_rules! kdl_child_with_name_or_error {
    ( $kdl_node:expr, $name:expr) => {{
        $kdl_node
            .children()
            .and_then(|children| children.nodes().iter().find(|c| c.name().value() == $name))
            .ok_or(ConfigError::new_kdl_error(
                format!("Missing node {}", $name).into(),
                $kdl_node.span().offset(),
                $kdl_node.span().len(),
            ))
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
        let support_kitty_keyboard_protocol = kdl_property_first_arg_as_bool_or_error!(
            kdl_options,
            "support_kitty_keyboard_protocol"
        )
        .map(|(v, _)| v);
        let web_server =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "web_server").map(|(v, _)| v);
        let web_sharing =
            match kdl_property_first_arg_as_string_or_error!(kdl_options, "web_sharing") {
                Some((string, entry)) => Some(WebSharing::from_str(string).map_err(|_| {
                    kdl_parsing_error!(
                        format!("Invalid value for web_sharing: '{}'", string),
                        entry
                    )
                })?),
                None => None,
            };
        let stacked_resize =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "stacked_resize").map(|(v, _)| v);
        let show_startup_tips =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "show_startup_tips")
                .map(|(v, _)| v);
        let show_release_notes =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "show_release_notes")
                .map(|(v, _)| v);
        let advanced_mouse_actions =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "advanced_mouse_actions")
                .map(|(v, _)| v);
        let web_server_ip =
            match kdl_property_first_arg_as_string_or_error!(kdl_options, "web_server_ip") {
                Some((string, entry)) => Some(IpAddr::from_str(string).map_err(|_| {
                    kdl_parsing_error!(
                        format!("Invalid value for web_server_ip: '{}'", string),
                        entry
                    )
                })?),
                None => None,
            };
        let web_server_port =
            kdl_property_first_arg_as_i64_or_error!(kdl_options, "web_server_port")
                .map(|(web_server_port, _entry)| web_server_port as u16);
        let web_server_cert =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "web_server_cert")
                .map(|(string, _entry)| PathBuf::from(string));
        let web_server_key =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "web_server_key")
                .map(|(string, _entry)| PathBuf::from(string));
        let enforce_https_for_localhost =
            kdl_property_first_arg_as_bool_or_error!(kdl_options, "enforce_https_for_localhost")
                .map(|(v, _)| v);
        let post_command_discovery_hook =
            kdl_property_first_arg_as_string_or_error!(kdl_options, "post_command_discovery_hook")
                .map(|(hook, _entry)| hook.to_string());

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
            support_kitty_keyboard_protocol,
            web_server,
            web_sharing,
            stacked_resize,
            show_startup_tips,
            show_release_notes,
            advanced_mouse_actions,
            web_server_ip,
            web_server_port,
            web_server_cert,
            web_server_key,
            enforce_https_for_localhost,
            post_command_discovery_hook,
        })
    }
    pub fn from_string(stringified_keybindings: &String) -> Result<Self, ConfigError> {
        let document: KdlDocument = stringified_keybindings.parse()?;
        Options::from_kdl(&document)
    }
    fn simplified_ui_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Use a simplified UI without special fonts (arrow glyphs)",
            "// Options:",
            "//   - true",
            "//   - false (Default)",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("simplified_ui");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(simplified_ui) = self.simplified_ui {
            let mut node = create_node(simplified_ui);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(true);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn theme_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// Choose the theme that is specified in the themes section.",
            "// Default: default",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("theme");
            node.push(node_value.to_owned());
            node
        };
        if let Some(theme) = &self.theme {
            let mut node = create_node(theme);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("dracula");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn default_mode_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ", "// Choose the base input mode of zellij.", "// Default: normal", "// "
        );

        let create_node = |default_mode: &InputMode| -> KdlNode {
            let mut node = KdlNode::new("default_mode");
            node.push(format!("{:?}", default_mode).to_lowercase());
            node
        };
        if let Some(default_mode) = &self.default_mode {
            let mut node = create_node(default_mode);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(&InputMode::Locked);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn default_shell_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text =
            format!("{}\n{}\n{}\n{}",
            " ",
            "// Choose the path to the default shell that zellij will use for opening new panes",
            "// Default: $SHELL",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("default_shell");
            node.push(node_value.to_owned());
            node
        };
        if let Some(default_shell) = &self.default_shell {
            let mut node = create_node(&default_shell.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("fish");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn default_cwd_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            " ",
            "// Choose the path to override cwd that zellij will use for opening new panes",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("default_cwd");
            node.push(node_value.to_owned());
            node
        };
        if let Some(default_cwd) = &self.default_cwd {
            let mut node = create_node(&default_cwd.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/tmp");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn default_layout_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// The name of the default layout to load on startup",
            "// Default: \"default\"",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("default_layout");
            node.push(node_value.to_owned());
            node
        };
        if let Some(default_layout) = &self.default_layout {
            let mut node = create_node(&default_layout.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("compact");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn layout_dir_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// The folder in which Zellij will look for layouts",
            "// (Requires restart)",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("layout_dir");
            node.push(node_value.to_owned());
            node
        };
        if let Some(layout_dir) = &self.layout_dir {
            let mut node = create_node(&layout_dir.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/tmp");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn theme_dir_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// The folder in which Zellij will look for themes",
            "// (Requires restart)",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("theme_dir");
            node.push(node_value.to_owned());
            node
        };
        if let Some(theme_dir) = &self.theme_dir {
            let mut node = create_node(&theme_dir.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/tmp");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn mouse_mode_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Toggle enabling the mouse mode.",
            "// On certain configurations, or terminals this could",
            "// potentially interfere with copying text.",
            "// Options:",
            "//   - true (default)",
            "//   - false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("mouse_mode");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(mouse_mode) = self.mouse_mode {
            let mut node = create_node(mouse_mode);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn pane_frames_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Toggle having pane frames around the panes",
            "// Options:",
            "//   - true (default, enabled)",
            "//   - false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("pane_frames");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(pane_frames) = self.pane_frames {
            let mut node = create_node(pane_frames);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn mirror_session_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// When attaching to an existing session with other users,",
            "// should the session be mirrored (true)",
            "// or should each user have their own cursor (false)",
            "// (Requires restart)",
            "// Default: false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("mirror_session");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(mirror_session) = self.mirror_session {
            let mut node = create_node(mirror_session);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(true);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn on_force_close_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Choose what to do when zellij receives SIGTERM, SIGINT, SIGQUIT or SIGHUP",
            "// eg. when terminal window with an active zellij session is closed",
            "// (Requires restart)",
            "// Options:",
            "//   - detach (Default)",
            "//   - quit",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("on_force_close");
            node.push(node_value.to_owned());
            node
        };
        if let Some(on_force_close) = &self.on_force_close {
            let mut node = match on_force_close {
                OnForceClose::Detach => create_node("detach"),
                OnForceClose::Quit => create_node("quit"),
            };
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("quit");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn scroll_buffer_size_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Configure the scroll back buffer size",
            "// This is the number of lines zellij stores for each pane in the scroll back",
            "// buffer. Excess number of lines are discarded in a FIFO fashion.",
            "// (Requires restart)",
            "// Valid values: positive integers",
            "// Default value: 10000",
            "// ",
        );

        let create_node = |node_value: usize| -> KdlNode {
            let mut node = KdlNode::new("scroll_buffer_size");
            node.push(KdlValue::Base10(node_value as i64));
            node
        };
        if let Some(scroll_buffer_size) = self.scroll_buffer_size {
            let mut node = create_node(scroll_buffer_size);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(10000);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn copy_command_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Provide a command to execute when copying text. The text will be piped to",
            "// the stdin of the program to perform the copy. This can be used with",
            "// terminal emulators which do not support the OSC 52 ANSI control sequence",
            "// that will be used by default if this option is not set.",
            "// Examples:",
            "//",
            "// copy_command \"xclip -selection clipboard\" // x11",
            "// copy_command \"wl-copy\"                    // wayland",
            "// copy_command \"pbcopy\"                     // osx",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("copy_command");
            node.push(node_value.to_owned());
            node
        };
        if let Some(copy_command) = &self.copy_command {
            let mut node = create_node(copy_command);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("pbcopy");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn copy_clipboard_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Choose the destination for copied text",
            "// Allows using the primary selection buffer (on x11/wayland) instead of the system clipboard.",
            "// Does not apply when using copy_command.",
            "// Options:",
            "//   - system (default)",
            "//   - primary",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("copy_clipboard");
            node.push(node_value.to_owned());
            node
        };
        if let Some(copy_clipboard) = &self.copy_clipboard {
            let mut node = match copy_clipboard {
                Clipboard::Primary => create_node("primary"),
                Clipboard::System => create_node("system"),
            };
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("primary");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn copy_on_select_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// Enable automatic copying (and clearing) of selection when releasing mouse",
            "// Default: true",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("copy_on_select");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(copy_on_select) = self.copy_on_select {
            let mut node = create_node(copy_on_select);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(true);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn scrollback_editor_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            " ",
            "// Path to the default editor to use to edit pane scrollbuffer",
            "// Default: $EDITOR or $VISUAL",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("scrollback_editor");
            node.push(node_value.to_owned());
            node
        };
        if let Some(scrollback_editor) = &self.scrollback_editor {
            let mut node = create_node(&scrollback_editor.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/usr/bin/vim");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn session_name_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// A fixed name to always give the Zellij session.",
            "// Consider also setting `attach_to_session true,`",
            "// otherwise this will error if such a session exists.",
            "// Default: <RANDOM>",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("session_name");
            node.push(node_value.to_owned());
            node
        };
        if let Some(session_name) = &self.session_name {
            let mut node = create_node(&session_name);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("My singleton session");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn attach_to_session_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}",
            " ",
            "// When `session_name` is provided, attaches to that session",
            "// if it is already running or creates it otherwise.",
            "// Default: false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("attach_to_session");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(attach_to_session) = self.attach_to_session {
            let mut node = create_node(attach_to_session);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(true);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn auto_layout_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Toggle between having Zellij lay out panes according to a predefined set of layouts whenever possible",
            "// Options:",
            "//   - true (default)",
            "//   - false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("auto_layout");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(auto_layout) = self.auto_layout {
            let mut node = create_node(auto_layout);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn session_serialization_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Whether sessions should be serialized to the cache folder (including their tabs/panes, cwds and running commands) so that they can later be resurrected",
            "// Options:",
            "//   - true (default)",
            "//   - false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("session_serialization");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(session_serialization) = self.session_serialization {
            let mut node = create_node(session_serialization);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn serialize_pane_viewport_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Whether pane viewports are serialized along with the session, default is false",
            "// Options:",
            "//   - true",
            "//   - false (default)",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("serialize_pane_viewport");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(serialize_pane_viewport) = self.serialize_pane_viewport {
            let mut node = create_node(serialize_pane_viewport);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn scrollback_lines_to_serialize_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}",
            " ",
            "// Scrollback lines to serialize along with the pane viewport when serializing sessions, 0",
            "// defaults to the scrollback size. If this number is higher than the scrollback size, it will",
            "// also default to the scrollback size. This does nothing if `serialize_pane_viewport` is not true.",
            "// ",
        );

        let create_node = |node_value: usize| -> KdlNode {
            let mut node = KdlNode::new("scrollback_lines_to_serialize");
            node.push(KdlValue::Base10(node_value as i64));
            node
        };
        if let Some(scrollback_lines_to_serialize) = self.scrollback_lines_to_serialize {
            let mut node = create_node(scrollback_lines_to_serialize);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(10000);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn styled_underlines_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Enable or disable the rendering of styled and colored underlines (undercurl).",
            "// May need to be disabled for certain unsupported terminals",
            "// (Requires restart)",
            "// Default: true",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("styled_underlines");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(styled_underlines) = self.styled_underlines {
            let mut node = create_node(styled_underlines);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn serialization_interval_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            " ", "// How often in seconds sessions are serialized", "// ",
        );

        let create_node = |node_value: u64| -> KdlNode {
            let mut node = KdlNode::new("serialization_interval");
            node.push(KdlValue::Base10(node_value as i64));
            node
        };
        if let Some(serialization_interval) = self.serialization_interval {
            let mut node = create_node(serialization_interval);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(10000);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn disable_session_metadata_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// Enable or disable writing of session metadata to disk (if disabled, other sessions might not know",
            "// metadata info on this session)",
            "// (Requires restart)",
            "// Default: false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("disable_session_metadata");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(disable_session_metadata) = self.disable_session_metadata {
            let mut node = create_node(disable_session_metadata);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn support_kitty_keyboard_protocol_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!("{}\n{}\n{}\n{}\n{}",
            " ",
            "// Enable or disable support for the enhanced Kitty Keyboard Protocol (the host terminal must also support it)",
            "// (Requires restart)",
            "// Default: true (if the host terminal supports it)",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("support_kitty_keyboard_protocol");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(support_kitty_keyboard_protocol) = self.support_kitty_keyboard_protocol {
            let mut node = create_node(support_kitty_keyboard_protocol);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_server_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            "// Whether to make sure a local web server is running when a new Zellij session starts.",
            "// This web server will allow creating new sessions and attaching to existing ones that have",
            "// opted in to being shared in the browser.",
            "// When enabled, navigate to http://127.0.0.1:8082",
            "// (Requires restart)",
            "// ",
            "// Note: a local web server can still be manually started from within a Zellij session or from the CLI.",
            "// If this is not desired, one can use a version of Zellij compiled without",
            "// `web_server_capability`",
            "// ",
            "// Possible values:",
            "// - true",
            "// - false",
            "// Default: false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("web_server");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(web_server) = self.web_server {
            let mut node = create_node(web_server);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_sharing_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
            "// Whether to allow sessions started in the terminal to be shared through a local web server, assuming one is",
            "// running (see the `web_server` option for more details).",
            "// (Requires restart)",
            "// ",
            "// Note: This is an administrative separation and not intended as a security measure.",
            "// ",
            "// Possible values:",
            "// - \"on\" (allow web sharing through the local web server if it",
            "// is online)",
            "// - \"off\" (do not allow web sharing unless sessions explicitly opt-in to it)",
            "// - \"disabled\" (do not allow web sharing and do not permit sessions started in the terminal to opt-in to it)",
            "// Default: \"off\"",
            "// ",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("web_sharing");
            node.push(node_value.to_owned());
            node
        };
        if let Some(web_sharing) = &self.web_sharing {
            let mut node = match web_sharing {
                WebSharing::On => create_node("on"),
                WebSharing::Off => create_node("off"),
                WebSharing::Disabled => create_node("disabled"),
            };
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("off");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_server_cert_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            "// A path to a certificate file to be used when setting up the web client to serve the",
            "// connection over HTTPs",
            "// ",
        );
        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("web_server_cert");
            node.push(node_value.to_owned());
            node
        };
        if let Some(web_server_cert) = &self.web_server_cert {
            let mut node = create_node(&web_server_cert.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/path/to/cert.pem");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_server_key_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            "// A path to a key file to be used when setting up the web client to serve the",
            "// connection over HTTPs",
            "// ",
        );
        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("web_server_key");
            node.push(node_value.to_owned());
            node
        };
        if let Some(web_server_key) = &self.web_server_key {
            let mut node = create_node(&web_server_key.display().to_string());
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("/path/to/key.pem");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn enforce_https_for_localhost_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}\n{}",
            "/// Whether to enforce https connections to the web server when it is bound to localhost",
            "/// (127.0.0.0/8)",
            "///",
            "/// Note: https is ALWAYS enforced when bound to non-local interfaces",
            "///",
            "/// Default: false",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("enforce_https_for_localhost");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(enforce_https_for_localhost) = self.enforce_https_for_localhost {
            let mut node = create_node(enforce_https_for_localhost);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn stacked_resize_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// Whether to stack panes when resizing beyond a certain size",
            "// Default: true",
            "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("stacked_resize");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(stacked_resize) = self.stacked_resize {
            let mut node = create_node(stacked_resize);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn show_startup_tips_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ", "// Whether to show tips on startup", "// Default: true", "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("show_startup_tips");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(show_startup_tips) = self.show_startup_tips {
            let mut node = create_node(show_startup_tips);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn show_release_notes_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ", "// Whether to show release notes on first version run", "// Default: true", "// ",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("show_release_notes");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(show_release_notes) = self.show_release_notes {
            let mut node = create_node(show_release_notes);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn advanced_mouse_actions_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}",
            " ",
            "// Whether to enable mouse hover effects and pane grouping functionality",
            "// default is true",
        );

        let create_node = |node_value: bool| -> KdlNode {
            let mut node = KdlNode::new("advanced_mouse_actions");
            node.push(KdlValue::Bool(node_value));
            node
        };
        if let Some(advanced_mouse_actions) = self.advanced_mouse_actions {
            let mut node = create_node(advanced_mouse_actions);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(false);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_server_ip_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// The ip address the web server should listen on when it starts",
            "// Default: \"127.0.0.1\"",
            "// (Requires restart)",
        );

        let create_node = |node_value: IpAddr| -> KdlNode {
            let mut node = KdlNode::new("web_server_ip");
            node.push(KdlValue::String(node_value.to_string()));
            node
        };
        if let Some(web_server_ip) = self.web_server_ip {
            let mut node = create_node(web_server_ip);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn web_server_port_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}",
            " ",
            "// The port the web server should listen on when it starts",
            "// Default: 8082",
            "// (Requires restart)",
        );

        let create_node = |node_value: u16| -> KdlNode {
            let mut node = KdlNode::new("web_server_port");
            node.push(KdlValue::Base10(node_value as i64));
            node
        };
        if let Some(web_server_port) = self.web_server_port {
            let mut node = create_node(web_server_port);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node(8082);
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    fn post_command_discovery_hook_to_kdl(&self, add_comments: bool) -> Option<KdlNode> {
        let comment_text = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            " ",
            "// A command to run (will be wrapped with sh -c and provided the RESURRECT_COMMAND env variable) ",
            "// after Zellij attempts to discover a command inside a pane when resurrecting sessions, the STDOUT",
            "// of this command will be used instead of the discovered RESURRECT_COMMAND",
            "// can be useful for removing wrappers around commands",
            "// Note: be sure to escape backslashes and similar characters properly",
        );

        let create_node = |node_value: &str| -> KdlNode {
            let mut node = KdlNode::new("post_command_discovery_hook");
            node.push(node_value.to_owned());
            node
        };
        if let Some(post_command_discovery_hook) = &self.post_command_discovery_hook {
            let mut node = create_node(&post_command_discovery_hook);
            if add_comments {
                node.set_leading(format!("{}\n", comment_text));
            }
            Some(node)
        } else if add_comments {
            let mut node = create_node("echo $RESURRECT_COMMAND | sed <your_regex_here>");
            node.set_leading(format!("{}\n// ", comment_text));
            Some(node)
        } else {
            None
        }
    }
    pub fn to_kdl(&self, add_comments: bool) -> Vec<KdlNode> {
        let mut nodes = vec![];
        if let Some(simplified_ui_node) = self.simplified_ui_to_kdl(add_comments) {
            nodes.push(simplified_ui_node);
        }
        if let Some(theme_node) = self.theme_to_kdl(add_comments) {
            nodes.push(theme_node);
        }
        if let Some(default_mode) = self.default_mode_to_kdl(add_comments) {
            nodes.push(default_mode);
        }
        if let Some(default_shell) = self.default_shell_to_kdl(add_comments) {
            nodes.push(default_shell);
        }
        if let Some(default_cwd) = self.default_cwd_to_kdl(add_comments) {
            nodes.push(default_cwd);
        }
        if let Some(default_layout) = self.default_layout_to_kdl(add_comments) {
            nodes.push(default_layout);
        }
        if let Some(layout_dir) = self.layout_dir_to_kdl(add_comments) {
            nodes.push(layout_dir);
        }
        if let Some(theme_dir) = self.theme_dir_to_kdl(add_comments) {
            nodes.push(theme_dir);
        }
        if let Some(mouse_mode) = self.mouse_mode_to_kdl(add_comments) {
            nodes.push(mouse_mode);
        }
        if let Some(pane_frames) = self.pane_frames_to_kdl(add_comments) {
            nodes.push(pane_frames);
        }
        if let Some(mirror_session) = self.mirror_session_to_kdl(add_comments) {
            nodes.push(mirror_session);
        }
        if let Some(on_force_close) = self.on_force_close_to_kdl(add_comments) {
            nodes.push(on_force_close);
        }
        if let Some(scroll_buffer_size) = self.scroll_buffer_size_to_kdl(add_comments) {
            nodes.push(scroll_buffer_size);
        }
        if let Some(copy_command) = self.copy_command_to_kdl(add_comments) {
            nodes.push(copy_command);
        }
        if let Some(copy_clipboard) = self.copy_clipboard_to_kdl(add_comments) {
            nodes.push(copy_clipboard);
        }
        if let Some(copy_on_select) = self.copy_on_select_to_kdl(add_comments) {
            nodes.push(copy_on_select);
        }
        if let Some(scrollback_editor) = self.scrollback_editor_to_kdl(add_comments) {
            nodes.push(scrollback_editor);
        }
        if let Some(session_name) = self.session_name_to_kdl(add_comments) {
            nodes.push(session_name);
        }
        if let Some(attach_to_session) = self.attach_to_session_to_kdl(add_comments) {
            nodes.push(attach_to_session);
        }
        if let Some(auto_layout) = self.auto_layout_to_kdl(add_comments) {
            nodes.push(auto_layout);
        }
        if let Some(session_serialization) = self.session_serialization_to_kdl(add_comments) {
            nodes.push(session_serialization);
        }
        if let Some(serialize_pane_viewport) = self.serialize_pane_viewport_to_kdl(add_comments) {
            nodes.push(serialize_pane_viewport);
        }
        if let Some(scrollback_lines_to_serialize) =
            self.scrollback_lines_to_serialize_to_kdl(add_comments)
        {
            nodes.push(scrollback_lines_to_serialize);
        }
        if let Some(styled_underlines) = self.styled_underlines_to_kdl(add_comments) {
            nodes.push(styled_underlines);
        }
        if let Some(serialization_interval) = self.serialization_interval_to_kdl(add_comments) {
            nodes.push(serialization_interval);
        }
        if let Some(disable_session_metadata) = self.disable_session_metadata_to_kdl(add_comments) {
            nodes.push(disable_session_metadata);
        }
        if let Some(support_kitty_keyboard_protocol) =
            self.support_kitty_keyboard_protocol_to_kdl(add_comments)
        {
            nodes.push(support_kitty_keyboard_protocol);
        }
        if let Some(web_server) = self.web_server_to_kdl(add_comments) {
            nodes.push(web_server);
        }
        if let Some(web_sharing) = self.web_sharing_to_kdl(add_comments) {
            nodes.push(web_sharing);
        }
        if let Some(web_server_cert) = self.web_server_cert_to_kdl(add_comments) {
            nodes.push(web_server_cert);
        }
        if let Some(web_server_key) = self.web_server_key_to_kdl(add_comments) {
            nodes.push(web_server_key);
        }
        if let Some(enforce_https_for_localhost) =
            self.enforce_https_for_localhost_to_kdl(add_comments)
        {
            nodes.push(enforce_https_for_localhost);
        }
        if let Some(stacked_resize) = self.stacked_resize_to_kdl(add_comments) {
            nodes.push(stacked_resize);
        }
        if let Some(show_startup_tips) = self.show_startup_tips_to_kdl(add_comments) {
            nodes.push(show_startup_tips);
        }
        if let Some(show_release_notes) = self.show_release_notes_to_kdl(add_comments) {
            nodes.push(show_release_notes);
        }
        if let Some(advanced_mouse_actions) = self.advanced_mouse_actions_to_kdl(add_comments) {
            nodes.push(advanced_mouse_actions);
        }
        if let Some(web_server_ip) = self.web_server_ip_to_kdl(add_comments) {
            nodes.push(web_server_ip);
        }
        if let Some(web_server_port) = self.web_server_port_to_kdl(add_comments) {
            nodes.push(web_server_port);
        }
        if let Some(post_command_discovery_hook) =
            self.post_command_discovery_hook_to_kdl(add_comments)
        {
            nodes.push(post_command_discovery_hook);
        }
        nodes
    }
}

impl Layout {
    pub fn from_kdl(
        raw_layout: &str,
        file_name: Option<String>,
        raw_swap_layouts: Option<(&str, &str)>, // raw_swap_layouts swap_layouts_file_name
        cwd: Option<PathBuf>,
    ) -> Result<Self, ConfigError> {
        let mut kdl_layout_parser = KdlLayoutParser::new(raw_layout, cwd, file_name.clone());
        let layout = kdl_layout_parser.parse().map_err(|e| match e {
            ConfigError::KdlError(kdl_error) => ConfigError::KdlError(kdl_error.add_src(
                file_name.unwrap_or_else(|| "N/A".to_owned()),
                String::from(raw_layout),
            )),
            ConfigError::KdlDeserializationError(kdl_error) => kdl_layout_error(
                kdl_error,
                file_name.unwrap_or_else(|| "N/A".to_owned()),
                raw_layout,
            ),
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
    pub fn to_kdl(&self) -> Option<KdlNode> {
        let mut has_env_vars = false;
        let mut env = KdlNode::new("env");
        let mut env_vars = KdlDocument::new();

        let mut stable_sorted = BTreeMap::new();
        for (env_var_name, env_var_value) in self.inner() {
            stable_sorted.insert(env_var_name, env_var_value);
        }
        for (env_key, env_value) in stable_sorted {
            has_env_vars = true;
            let mut variable_key = KdlNode::new(env_key.to_owned());
            variable_key.push(env_value.to_owned());
            env_vars.nodes_mut().push(variable_key);
        }

        if has_env_vars {
            env.set_children(env_vars);
            Some(env)
        } else {
            None
        }
    }
}

impl Keybinds {
    fn bind_keys_in_block(
        block: &KdlNode,
        input_mode_keybinds: &mut HashMap<KeyWithModifier, Vec<Action>>,
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
        input_mode_keybinds: &mut HashMap<KeyWithModifier, Vec<Action>>,
        config_options: &Options,
    ) -> Result<(), ConfigError> {
        let keys: Vec<KeyWithModifier> = keys_from_kdl!(key_block);
        let actions: Vec<Action> = actions_from_kdl!(key_block, config_options);
        for key in keys {
            input_mode_keybinds.insert(key, actions.clone());
        }
        Ok(())
    }
    fn unbind_keys(
        key_block: &KdlNode,
        input_mode_keybinds: &mut HashMap<KeyWithModifier, Vec<Action>>,
    ) -> Result<(), ConfigError> {
        let keys: Vec<KeyWithModifier> = keys_from_kdl!(key_block);
        for key in keys {
            input_mode_keybinds.remove(&key);
        }
        Ok(())
    }
    fn unbind_keys_in_all_modes(
        global_unbind: &KdlNode,
        keybinds_from_config: &mut Keybinds,
    ) -> Result<(), ConfigError> {
        let keys: Vec<KeyWithModifier> = keys_from_kdl!(global_unbind);
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
    ) -> Result<&'a mut HashMap<KeyWithModifier, Vec<Action>>, ConfigError> {
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
    pub fn from_string(
        stringified_keybindings: String,
        base_keybinds: Keybinds,
        config_options: &Options,
    ) -> Result<Self, ConfigError> {
        let document: KdlDocument = stringified_keybindings.parse()?;
        if let Some(kdl_keybinds) = document.get("keybinds") {
            Keybinds::from_kdl(&kdl_keybinds, base_keybinds, config_options)
        } else {
            Err(ConfigError::new_kdl_error(
                format!("Could not find keybinds node"),
                document.span().offset(),
                document.span().len(),
            ))
        }
    }
    // minimize keybind entries for serialization, so that duplicate entries will appear in
    // "shared" nodes later rather than once per mode
    fn minimize_entries(
        &self,
    ) -> BTreeMap<BTreeSet<InputMode>, BTreeMap<KeyWithModifier, Vec<Action>>> {
        let mut minimized: BTreeMap<BTreeSet<InputMode>, BTreeMap<KeyWithModifier, Vec<Action>>> =
            BTreeMap::new();
        let mut flattened: Vec<BTreeMap<KeyWithModifier, Vec<Action>>> = self
            .0
            .iter()
            .map(|(_input_mode, keybind)| keybind.clone().into_iter().collect())
            .collect();
        for keybind in flattened.drain(..) {
            for (key, actions) in keybind.into_iter() {
                let mut appears_in_modes: BTreeSet<InputMode> = BTreeSet::new();
                for (input_mode, keybinds) in self.0.iter() {
                    if keybinds.get(&key) == Some(&actions) {
                        appears_in_modes.insert(*input_mode);
                    }
                }
                minimized
                    .entry(appears_in_modes)
                    .or_insert_with(Default::default)
                    .insert(key, actions);
            }
        }
        minimized
    }
    fn serialize_mode_title_node(&self, input_modes: &BTreeSet<InputMode>) -> KdlNode {
        let all_modes: Vec<InputMode> = InputMode::iter().collect();
        let total_input_mode_count = all_modes.len();
        if input_modes.len() == 1 {
            let input_mode_name =
                format!("{:?}", input_modes.iter().next().unwrap()).to_lowercase();
            KdlNode::new(input_mode_name)
        } else if input_modes.len() == total_input_mode_count {
            KdlNode::new("shared")
        } else if input_modes.len() < total_input_mode_count / 2 {
            let mut node = KdlNode::new("shared_among");
            for input_mode in input_modes {
                node.push(format!("{:?}", input_mode).to_lowercase());
            }
            node
        } else {
            let mut node = KdlNode::new("shared_except");
            let mut modes = all_modes.clone();
            for input_mode in input_modes {
                modes.retain(|m| m != input_mode)
            }
            for mode in modes {
                node.push(format!("{:?}", mode).to_lowercase());
            }
            node
        }
    }
    fn serialize_mode_keybinds(
        &self,
        keybinds: &BTreeMap<KeyWithModifier, Vec<Action>>,
    ) -> KdlDocument {
        let mut mode_keybinds = KdlDocument::new();
        for keybind in keybinds {
            let mut keybind_node = KdlNode::new("bind");
            keybind_node.push(keybind.0.to_kdl());
            let mut actions = KdlDocument::new();
            let mut actions_have_children = false;
            for action in keybind.1 {
                if let Some(kdl_action) = action.to_kdl() {
                    if kdl_action.children().is_some() {
                        actions_have_children = true;
                    }
                    actions.nodes_mut().push(kdl_action);
                }
            }
            if !actions_have_children {
                for action in actions.nodes_mut() {
                    action.set_leading("");
                    action.set_trailing("; ");
                }
                actions.set_leading(" ");
                actions.set_trailing("");
            }
            keybind_node.set_children(actions);
            mode_keybinds.nodes_mut().push(keybind_node);
        }
        mode_keybinds
    }
    pub fn to_kdl(&self, should_clear_defaults: bool) -> KdlNode {
        let mut keybinds_node = KdlNode::new("keybinds");
        if should_clear_defaults {
            keybinds_node.insert("clear-defaults", true);
        }
        let mut minimized = self.minimize_entries();
        let mut keybinds_children = KdlDocument::new();

        macro_rules! encode_single_input_mode {
            ($mode_name:ident) => {{
                if let Some(keybinds) = minimized.remove(&BTreeSet::from([InputMode::$mode_name])) {
                    let mut mode_node =
                        KdlNode::new(format!("{:?}", InputMode::$mode_name).to_lowercase());
                    let mode_keybinds = self.serialize_mode_keybinds(&keybinds);
                    mode_node.set_children(mode_keybinds);
                    keybinds_children.nodes_mut().push(mode_node);
                }
            }};
        }
        // we do this explicitly so that the sorting order of modes in the config is more Human
        // readable - this is actually less code (and clearer) than implementing Ord in this case
        encode_single_input_mode!(Normal);
        encode_single_input_mode!(Locked);
        encode_single_input_mode!(Pane);
        encode_single_input_mode!(Tab);
        encode_single_input_mode!(Resize);
        encode_single_input_mode!(Move);
        encode_single_input_mode!(Scroll);
        encode_single_input_mode!(Search);
        encode_single_input_mode!(Session);

        for (input_modes, keybinds) in minimized {
            if input_modes.is_empty() {
                log::error!("invalid input mode for keybinds: {:#?}", keybinds);
                continue;
            }
            let mut mode_node = self.serialize_mode_title_node(&input_modes);
            let mode_keybinds = self.serialize_mode_keybinds(&keybinds);
            mode_node.set_children(mode_keybinds);
            keybinds_children.nodes_mut().push(mode_node);
        }
        keybinds_node.set_children(keybinds_children);
        keybinds_node
    }
}

impl KeyWithModifier {
    pub fn to_kdl(&self) -> String {
        if self.key_modifiers.is_empty() {
            self.bare_key.to_kdl()
        } else {
            format!(
                "{} {}",
                self.key_modifiers
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
                self.bare_key.to_kdl()
            )
        }
    }
}

impl BareKey {
    pub fn to_kdl(&self) -> String {
        match self {
            BareKey::PageDown => format!("PageDown"),
            BareKey::PageUp => format!("PageUp"),
            BareKey::Left => format!("left"),
            BareKey::Down => format!("down"),
            BareKey::Up => format!("up"),
            BareKey::Right => format!("right"),
            BareKey::Home => format!("home"),
            BareKey::End => format!("end"),
            BareKey::Backspace => format!("backspace"),
            BareKey::Delete => format!("del"),
            BareKey::Insert => format!("insert"),
            BareKey::F(index) => format!("F{}", index),
            BareKey::Char(' ') => format!("space"),
            BareKey::Char(character) => format!("{}", character),
            BareKey::Tab => format!("tab"),
            BareKey::Esc => format!("esc"),
            BareKey::Enter => format!("enter"),
            BareKey::CapsLock => format!("capslock"),
            BareKey::ScrollLock => format!("scrolllock"),
            BareKey::NumLock => format!("numlock"),
            BareKey::PrintScreen => format!("printscreen"),
            BareKey::Pause => format!("pause"),
            BareKey::Menu => format!("menu"),
        }
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
            let sourced_from_external_file = false;
            let config_themes = Themes::from_kdl(kdl_themes, sourced_from_external_file)?;
            config.themes = config.themes.merge(config_themes);
        }
        if let Some(kdl_plugin_aliases) = kdl_config.get("plugins") {
            let config_plugins = PluginAliases::from_kdl(kdl_plugin_aliases)?;
            config.plugins.merge(config_plugins);
        }
        if let Some(kdl_load_plugins) = kdl_config.get("load_plugins") {
            let load_plugins = load_plugins_from_kdl(kdl_load_plugins)?;
            config.background_plugins = load_plugins;
        }
        if let Some(kdl_ui_config) = kdl_config.get("ui") {
            let config_ui = UiConfig::from_kdl(&kdl_ui_config)?;
            config.ui = config.ui.merge(config_ui);
        }
        if let Some(env_config) = kdl_config.get("env") {
            let config_env = EnvironmentVariables::from_kdl(&env_config)?;
            config.env = config.env.merge(config_env);
        }
        if let Some(web_client_config) = kdl_config.get("web_client") {
            let config_web_client = WebClientConfig::from_kdl(&web_client_config)?;
            config.web_client = config.web_client.merge(config_web_client);
        }
        Ok(config)
    }
    pub fn to_string(&self, add_comments: bool) -> String {
        let mut document = KdlDocument::new();

        let clear_defaults = true;
        let keybinds = self.keybinds.to_kdl(clear_defaults);
        document.nodes_mut().push(keybinds);

        if let Some(themes) = self.themes.to_kdl() {
            document.nodes_mut().push(themes);
        }

        let plugins = self.plugins.to_kdl(add_comments);
        document.nodes_mut().push(plugins);

        let load_plugins = load_plugins_to_kdl(&self.background_plugins, add_comments);
        document.nodes_mut().push(load_plugins);

        if let Some(ui_config) = self.ui.to_kdl() {
            document.nodes_mut().push(ui_config);
        }

        if let Some(env) = self.env.to_kdl() {
            document.nodes_mut().push(env);
        }

        document.nodes_mut().push(self.web_client.to_kdl());

        document
            .nodes_mut()
            .append(&mut self.options.to_kdl(add_comments));

        document.to_string()
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
    pub fn to_kdl(&self, add_comments: bool) -> KdlNode {
        let mut plugins = KdlNode::new("plugins");
        let mut plugins_children = KdlDocument::new();
        for (alias_name, plugin_alias) in self.aliases.iter() {
            let mut plugin_alias_node = KdlNode::new(alias_name.clone());
            let mut plugin_alias_children = KdlDocument::new();
            let location_string = plugin_alias.location.display();

            plugin_alias_node.insert("location", location_string);
            let cwd = plugin_alias.initial_cwd.as_ref();
            let mut has_children = false;
            if let Some(cwd) = cwd {
                has_children = true;
                let mut cwd_node = KdlNode::new("cwd");
                cwd_node.push(cwd.display().to_string());
                plugin_alias_children.nodes_mut().push(cwd_node);
            }
            let configuration = plugin_alias.configuration.inner();
            if !configuration.is_empty() {
                has_children = true;
                for (config_key, config_value) in configuration {
                    let mut node = KdlNode::new(config_key.to_owned());
                    if config_value == "true" {
                        node.push(KdlValue::Bool(true));
                    } else if config_value == "false" {
                        node.push(KdlValue::Bool(false));
                    } else {
                        node.push(config_value.to_string());
                    }
                    plugin_alias_children.nodes_mut().push(node);
                }
            }
            if has_children {
                plugin_alias_node.set_children(plugin_alias_children);
            }
            plugins_children.nodes_mut().push(plugin_alias_node);
        }
        plugins.set_children(plugins_children);

        if add_comments {
            plugins.set_leading(format!(
                "\n{}\n{}\n",
                "// Plugin aliases - can be used to change the implementation of Zellij",
                "// changing these requires a restart to take effect",
            ));
        }
        plugins
    }
}

pub fn load_plugins_to_kdl(
    background_plugins: &HashSet<RunPluginOrAlias>,
    add_comments: bool,
) -> KdlNode {
    let mut load_plugins = KdlNode::new("load_plugins");
    let mut load_plugins_children = KdlDocument::new();
    for run_plugin_or_alias in background_plugins.iter() {
        let mut background_plugin_node = KdlNode::new(run_plugin_or_alias.location_string());
        let mut background_plugin_children = KdlDocument::new();

        let cwd = match run_plugin_or_alias {
            RunPluginOrAlias::RunPlugin(run_plugin) => run_plugin.initial_cwd.clone(),
            RunPluginOrAlias::Alias(plugin_alias) => plugin_alias.initial_cwd.clone(),
        };
        let mut has_children = false;
        if let Some(cwd) = cwd.as_ref() {
            has_children = true;
            let mut cwd_node = KdlNode::new("cwd");
            cwd_node.push(cwd.display().to_string());
            background_plugin_children.nodes_mut().push(cwd_node);
        }
        let configuration = match run_plugin_or_alias {
            RunPluginOrAlias::RunPlugin(run_plugin) => {
                Some(run_plugin.configuration.inner().clone())
            },
            RunPluginOrAlias::Alias(plugin_alias) => plugin_alias
                .configuration
                .as_ref()
                .map(|c| c.inner().clone()),
        };
        if let Some(configuration) = configuration {
            if !configuration.is_empty() {
                has_children = true;
                for (config_key, config_value) in configuration {
                    let mut node = KdlNode::new(config_key.to_owned());
                    if config_value == "true" {
                        node.push(KdlValue::Bool(true));
                    } else if config_value == "false" {
                        node.push(KdlValue::Bool(false));
                    } else {
                        node.push(config_value.to_string());
                    }
                    background_plugin_children.nodes_mut().push(node);
                }
            }
        }
        if has_children {
            background_plugin_node.set_children(background_plugin_children);
        }
        load_plugins_children
            .nodes_mut()
            .push(background_plugin_node);
    }
    load_plugins.set_children(load_plugins_children);

    if add_comments {
        load_plugins.set_leading(format!(
            "\n{}\n{}\n{}\n",
            "// Plugins to load in the background when a new session starts",
            "// eg. \"file:/path/to/my-plugin.wasm\"",
            "// eg. \"https://example.com/my-plugin.wasm\"",
        ));
    }
    load_plugins
}

fn load_plugins_from_kdl(
    kdl_load_plugins: &KdlNode,
) -> Result<HashSet<RunPluginOrAlias>, ConfigError> {
    let mut load_plugins: HashSet<RunPluginOrAlias> = HashSet::new();
    if let Some(kdl_load_plugins) = kdl_children_nodes!(kdl_load_plugins) {
        for plugin_block in kdl_load_plugins {
            let url_node = plugin_block.name();
            let string_url = url_node.value();
            let configuration = KdlLayoutParser::parse_plugin_user_configuration(&plugin_block)?;
            let cwd = kdl_get_string_property_or_child_value!(&plugin_block, "cwd")
                .map(|s| PathBuf::from(s));
            let run_plugin_or_alias = RunPluginOrAlias::from_url(
                &string_url,
                &Some(configuration.inner().clone()),
                None,
                cwd.clone(),
            )
            .map_err(|e| {
                ConfigError::new_kdl_error(
                    format!("Failed to parse plugin: {}", e),
                    url_node.span().offset(),
                    url_node.span().len(),
                )
            })?
            .with_initial_cwd(cwd);
            load_plugins.insert(run_plugin_or_alias);
        }
    }
    Ok(load_plugins)
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
    pub fn to_kdl(&self) -> Option<KdlNode> {
        let mut ui_config = KdlNode::new("ui");
        let mut ui_config_children = KdlDocument::new();
        let mut frame_config = KdlNode::new("pane_frames");
        let mut frame_config_children = KdlDocument::new();
        let mut has_ui_config = false;
        if self.pane_frames.rounded_corners {
            has_ui_config = true;
            let mut rounded_corners = KdlNode::new("rounded_corners");
            rounded_corners.push(KdlValue::Bool(true));
            frame_config_children.nodes_mut().push(rounded_corners);
        }
        if self.pane_frames.hide_session_name {
            has_ui_config = true;
            let mut hide_session_name = KdlNode::new("hide_session_name");
            hide_session_name.push(KdlValue::Bool(true));
            frame_config_children.nodes_mut().push(hide_session_name);
        }
        if has_ui_config {
            frame_config.set_children(frame_config_children);
            ui_config_children.nodes_mut().push(frame_config);
            ui_config.set_children(ui_config_children);
            Some(ui_config)
        } else {
            None
        }
    }
}

impl Themes {
    fn style_declaration_from_node(
        style_node: &KdlNode,
        style_descriptor: &str,
    ) -> Result<Option<StyleDeclaration>, ConfigError> {
        let descriptor_node = kdl_child_with_name!(style_node, style_descriptor);

        match descriptor_node {
            Some(descriptor) => {
                let colors = kdl_children_or_error!(
                    descriptor,
                    format!("Missing colors for {}", style_descriptor)
                );
                Ok(Some(StyleDeclaration {
                    base: PaletteColor::try_from(("base", colors))?,
                    background: PaletteColor::try_from(("background", colors)).unwrap_or_default(),
                    emphasis_0: PaletteColor::try_from(("emphasis_0", colors))?,
                    emphasis_1: PaletteColor::try_from(("emphasis_1", colors))?,
                    emphasis_2: PaletteColor::try_from(("emphasis_2", colors))?,
                    emphasis_3: PaletteColor::try_from(("emphasis_3", colors))?,
                }))
            },
            None => Ok(None),
        }
    }

    fn multiplayer_colors(style_node: &KdlNode) -> Result<MultiplayerColors, ConfigError> {
        let descriptor_node = kdl_child_with_name!(style_node, "multiplayer_user_colors");
        match descriptor_node {
            Some(descriptor) => {
                let colors = kdl_children_or_error!(
                    descriptor,
                    format!("Missing colors for {}", "multiplayer_user_colors")
                );
                Ok(MultiplayerColors {
                    player_1: PaletteColor::try_from(("player_1", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_1),
                    player_2: PaletteColor::try_from(("player_2", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_2),
                    player_3: PaletteColor::try_from(("player_3", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_3),
                    player_4: PaletteColor::try_from(("player_4", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_4),
                    player_5: PaletteColor::try_from(("player_5", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_5),
                    player_6: PaletteColor::try_from(("player_6", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_6),
                    player_7: PaletteColor::try_from(("player_7", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_7),
                    player_8: PaletteColor::try_from(("player_8", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_8),
                    player_9: PaletteColor::try_from(("player_9", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_9),
                    player_10: PaletteColor::try_from(("player_10", colors))
                        .unwrap_or(DEFAULT_STYLES.multiplayer_user_colors.player_10),
                })
            },
            None => Ok(DEFAULT_STYLES.multiplayer_user_colors),
        }
    }

    pub fn from_kdl(
        themes_from_kdl: &KdlNode,
        sourced_from_external_file: bool,
    ) -> Result<Self, ConfigError> {
        let mut themes: HashMap<String, Theme> = HashMap::new();
        for theme_config in kdl_children_nodes_or_error!(themes_from_kdl, "no themes found") {
            let theme_name = kdl_name!(theme_config);
            let theme_colors = kdl_children_or_error!(theme_config, "empty theme");
            let palette_color_names = HashSet::from([
                "fg", "bg", "red", "green", "blue", "yellow", "magenta", "orange", "cyan", "black",
                "white",
            ]);
            let theme = if theme_colors
                .nodes()
                .iter()
                .all(|n| palette_color_names.contains(n.name().value()))
            {
                // Older palette based theme definition
                let palette = Palette {
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
                };
                Theme {
                    palette: palette.into(),
                    sourced_from_external_file,
                }
            } else {
                // Newer theme definition with named styles
                let s = Styling {
                    text_unselected: Themes::style_declaration_from_node(
                        theme_config,
                        "text_unselected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.text_unselected))?,
                    text_selected: Themes::style_declaration_from_node(
                        theme_config,
                        "text_selected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.text_selected))?,
                    ribbon_unselected: Themes::style_declaration_from_node(
                        theme_config,
                        "ribbon_unselected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.ribbon_unselected))?,
                    ribbon_selected: Themes::style_declaration_from_node(
                        theme_config,
                        "ribbon_selected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.ribbon_selected))?,
                    table_title: Themes::style_declaration_from_node(theme_config, "table_title")
                        .map(|maybe_style| {
                        maybe_style.unwrap_or(DEFAULT_STYLES.table_title)
                    })?,
                    table_cell_unselected: Themes::style_declaration_from_node(
                        theme_config,
                        "table_cell_unselected",
                    )
                    .map(|maybe_style| {
                        maybe_style.unwrap_or(DEFAULT_STYLES.table_cell_unselected)
                    })?,
                    table_cell_selected: Themes::style_declaration_from_node(
                        theme_config,
                        "table_cell_selected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.table_cell_selected))?,
                    list_unselected: Themes::style_declaration_from_node(
                        theme_config,
                        "list_unselected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.list_unselected))?,
                    list_selected: Themes::style_declaration_from_node(
                        theme_config,
                        "list_selected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.list_selected))?,
                    frame_unselected: Themes::style_declaration_from_node(
                        theme_config,
                        "frame_unselected",
                    )?,
                    frame_selected: Themes::style_declaration_from_node(
                        theme_config,
                        "frame_selected",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.frame_selected))?,
                    frame_highlight: Themes::style_declaration_from_node(
                        theme_config,
                        "frame_highlight",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.frame_highlight))?,
                    exit_code_success: Themes::style_declaration_from_node(
                        theme_config,
                        "exit_code_success",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.exit_code_success))?,
                    exit_code_error: Themes::style_declaration_from_node(
                        theme_config,
                        "exit_code_error",
                    )
                    .map(|maybe_style| maybe_style.unwrap_or(DEFAULT_STYLES.exit_code_error))?,
                    multiplayer_user_colors: Themes::multiplayer_colors(theme_config)
                        .unwrap_or_default(),
                };

                Theme {
                    palette: s,
                    sourced_from_external_file,
                }
            };
            themes.insert(theme_name.into(), theme);
        }
        let themes = Themes::from_data(themes);
        Ok(themes)
    }

    pub fn from_string(
        raw_string: &String,
        sourced_from_external_file: bool,
    ) -> Result<Self, ConfigError> {
        let kdl_config: KdlDocument = raw_string.parse()?;
        let kdl_themes = kdl_config.get("themes").ok_or(ConfigError::new_kdl_error(
            "No theme node found in file".into(),
            kdl_config.span().offset(),
            kdl_config.span().len(),
        ))?;
        let all_themes_in_file = Themes::from_kdl(kdl_themes, sourced_from_external_file)?;
        Ok(all_themes_in_file)
    }

    pub fn from_path(path_to_theme_file: PathBuf) -> Result<Self, ConfigError> {
        // String is the theme name
        let kdl_config = std::fs::read_to_string(&path_to_theme_file)
            .map_err(|e| ConfigError::IoPath(e, path_to_theme_file.clone()))?;
        let sourced_from_external_file = true;
        Themes::from_string(&kdl_config, sourced_from_external_file).map_err(|e| match e {
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
    pub fn to_kdl(&self) -> Option<KdlNode> {
        let mut theme_node = KdlNode::new("themes");
        let mut themes = KdlDocument::new();
        let mut has_themes = false;
        let sorted_themes: BTreeMap<String, Theme> = self.inner().clone().into_iter().collect();
        for (theme_name, theme) in sorted_themes {
            if theme.sourced_from_external_file {
                // we do not serialize themes that have been defined in external files so as not to
                // clog up the configuration file definitions
                continue;
            }
            has_themes = true;
            let mut current_theme_node = KdlNode::new(theme_name.clone());
            let mut current_theme_node_children = KdlDocument::new();

            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.text_unselected.to_kdl("text_unselected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.text_selected.to_kdl("text_selected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.ribbon_selected.to_kdl("ribbon_selected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.ribbon_unselected.to_kdl("ribbon_unselected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.table_title.to_kdl("table_title"));
            current_theme_node_children.nodes_mut().push(
                theme
                    .palette
                    .table_cell_selected
                    .to_kdl("table_cell_selected"),
            );
            current_theme_node_children.nodes_mut().push(
                theme
                    .palette
                    .table_cell_unselected
                    .to_kdl("table_cell_unselected"),
            );
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.list_selected.to_kdl("list_selected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.list_unselected.to_kdl("list_unselected"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.frame_selected.to_kdl("frame_selected"));

            match theme.palette.frame_unselected {
                None => {},
                Some(frame_unselected_style) => {
                    current_theme_node_children
                        .nodes_mut()
                        .push(frame_unselected_style.to_kdl("frame_unselected"));
                },
            }
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.frame_highlight.to_kdl("frame_highlight"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.exit_code_success.to_kdl("exit_code_success"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.exit_code_error.to_kdl("exit_code_error"));
            current_theme_node_children
                .nodes_mut()
                .push(theme.palette.multiplayer_user_colors.to_kdl());
            current_theme_node.set_children(current_theme_node_children);
            themes.nodes_mut().push(current_theme_node);
        }
        if has_themes {
            theme_node.set_children(themes);
            Some(theme_node)
        } else {
            None
        }
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
        let web_client_count = kdl_document
            .get("web_client_count")
            .and_then(|n| n.entries().iter().next())
            .and_then(|e| e.value().as_i64())
            .map(|c| c as usize)
            .unwrap_or(0);
        let web_clients_allowed = kdl_document
            .get("web_clients_allowed")
            .and_then(|n| n.entries().iter().next())
            .and_then(|e| e.value().as_bool())
            .unwrap_or(false);
        let is_current_session = name == current_session_name;
        let mut tab_history = BTreeMap::new();
        if let Some(kdl_tab_history) = kdl_document.get("tab_history").and_then(|p| p.children()) {
            for client_node in kdl_tab_history.nodes() {
                if let Some(client_id) = client_node.children().and_then(|c| {
                    c.get("id")
                        .and_then(|c| c.entries().iter().next().and_then(|e| e.value().as_i64()))
                }) {
                    let mut history = vec![];
                    if let Some(history_entries) = client_node
                        .children()
                        .and_then(|c| c.get("history"))
                        .map(|h| h.entries())
                    {
                        for entry in history_entries {
                            if let Some(entry) = entry.value().as_i64() {
                                history.push(entry as usize);
                            }
                        }
                    }
                    tab_history.insert(client_id as u16, history);
                }
            }
        }
        Ok(SessionInfo {
            name,
            tabs,
            panes,
            connected_clients,
            is_current_session,
            available_layouts,
            web_client_count,
            web_clients_allowed,
            plugins: Default::default(), // we do not serialize plugin information
            tab_history,
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

        let mut web_client_count = KdlNode::new("web_client_count");
        web_client_count.push(self.web_client_count as i64);

        let mut web_clients_allowed = KdlNode::new("web_clients_allowed");
        web_clients_allowed.push(self.web_clients_allowed);

        let mut available_layouts = KdlNode::new("available_layouts");
        let mut available_layouts_children = KdlDocument::new();
        for layout_info in &self.available_layouts {
            let (layout_name, layout_source) = match layout_info {
                LayoutInfo::File(name) => (name.clone(), "file"),
                LayoutInfo::BuiltIn(name) => (name.clone(), "built-in"),
                LayoutInfo::Url(url) => (url.clone(), "url"),
                LayoutInfo::Stringified(_stringified) => ("stringified-layout".to_owned(), "N/A"),
            };
            let mut layout_node = KdlNode::new(format!("{}", layout_name));
            let layout_source = KdlEntry::new_prop("source", layout_source);
            layout_node.entries_mut().push(layout_source);
            available_layouts_children.nodes_mut().push(layout_node);
        }
        available_layouts.set_children(available_layouts_children);

        let mut tab_history = KdlNode::new("tab_history");
        let mut tab_history_children = KdlDocument::new();
        for (client_id, client_tab_history) in &self.tab_history {
            let mut client_document = KdlDocument::new();
            let mut client_node = KdlNode::new("client");
            let mut id = KdlNode::new("id");
            id.push(*client_id as i64);
            client_document.nodes_mut().push(id);
            let mut history = KdlNode::new("history");
            for entry in client_tab_history {
                history.push(*entry as i64);
            }
            client_document.nodes_mut().push(history);
            client_node.set_children(client_document);
            tab_history_children.nodes_mut().push(client_node);
        }
        tab_history.set_children(tab_history_children);

        kdl_document.nodes_mut().push(name);
        kdl_document.nodes_mut().push(tabs);
        kdl_document.nodes_mut().push(panes);
        kdl_document.nodes_mut().push(connected_clients);
        kdl_document.nodes_mut().push(web_clients_allowed);
        kdl_document.nodes_mut().push(web_client_count);
        kdl_document.nodes_mut().push(available_layouts);
        kdl_document.nodes_mut().push(tab_history);
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
        let viewport_rows = optional_int_node!("viewport_rows", usize).unwrap_or(0);
        let viewport_columns = optional_int_node!("viewport_columns", usize).unwrap_or(0);
        let display_area_rows = optional_int_node!("display_area_rows", usize).unwrap_or(0);
        let display_area_columns = optional_int_node!("display_area_columns", usize).unwrap_or(0);
        let is_swap_layout_dirty = bool_node!("is_swap_layout_dirty");
        let selectable_tiled_panes_count =
            optional_int_node!("selectable_tiled_panes_count", usize).unwrap_or(0);
        let selectable_floating_panes_count =
            optional_int_node!("selectable_floating_panes_count", usize).unwrap_or(0);
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
            viewport_rows,
            viewport_columns,
            display_area_rows,
            display_area_columns,
            selectable_tiled_panes_count,
            selectable_floating_panes_count,
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

        let mut viewport_rows = KdlNode::new("viewport_rows");
        viewport_rows.push(self.viewport_rows as i64);
        kdl_doucment.nodes_mut().push(viewport_rows);

        let mut viewport_columns = KdlNode::new("viewport_columns");
        viewport_columns.push(self.viewport_columns as i64);
        kdl_doucment.nodes_mut().push(viewport_columns);

        let mut display_area_columns = KdlNode::new("display_area_columns");
        display_area_columns.push(self.display_area_columns as i64);
        kdl_doucment.nodes_mut().push(display_area_columns);

        let mut display_area_rows = KdlNode::new("display_area_rows");
        display_area_rows.push(self.display_area_rows as i64);
        kdl_doucment.nodes_mut().push(display_area_rows);

        let mut is_swap_layout_dirty = KdlNode::new("is_swap_layout_dirty");
        is_swap_layout_dirty.push(self.is_swap_layout_dirty);
        kdl_doucment.nodes_mut().push(is_swap_layout_dirty);

        let mut selectable_tiled_panes_count = KdlNode::new("selectable_tiled_panes_count");
        selectable_tiled_panes_count.push(self.selectable_tiled_panes_count as i64);
        kdl_doucment.nodes_mut().push(selectable_tiled_panes_count);

        let mut selectable_floating_panes_count = KdlNode::new("selectable_floating_panes_count");
        selectable_floating_panes_count.push(self.selectable_floating_panes_count as i64);
        kdl_doucment
            .nodes_mut()
            .push(selectable_floating_panes_count);

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
            index_in_pane_group: Default::default(), // we don't serialize this
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
            index_in_pane_group: Default::default(), // we don't serialize this
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
            index_in_pane_group: Default::default(), // we don't serialize this
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
                viewport_rows: 10,
                viewport_columns: 10,
                display_area_rows: 10,
                display_area_columns: 10,
                selectable_tiled_panes_count: 10,
                selectable_floating_panes_count: 10,
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
                viewport_rows: 10,
                viewport_columns: 10,
                display_area_rows: 10,
                display_area_columns: 10,
                selectable_tiled_panes_count: 10,
                selectable_floating_panes_count: 10,
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
        plugins: Default::default(),
        web_client_count: 2,
        web_clients_allowed: true,
        tab_history: Default::default(),
    };
    let serialized = session_info.to_string();
    let deserealized = SessionInfo::from_string(&serialized, "not this session").unwrap();
    assert_eq!(session_info, deserealized);
    insta::assert_snapshot!(serialized);
}

#[test]
fn keybinds_to_string() {
    let fake_config = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = true;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    insta::assert_snapshot!(serialized.to_string());
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
}

#[test]
fn keybinds_to_string_without_clearing_defaults() {
    let fake_config = r#"
        keybinds {
            normal {
                bind "Ctrl g" { SwitchToMode "Locked"; }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = false;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    insta::assert_snapshot!(serialized.to_string());
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
}

#[test]
fn keybinds_to_string_with_multiple_actions() {
    let fake_config = r#"
        keybinds {
            normal {
                bind "Ctrl n" { NewPane; SwitchToMode "Locked"; }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = true;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn keybinds_to_string_with_all_actions() {
    let fake_config = r#"
        keybinds {
            normal {
                bind "Ctrl a" { Quit; }
                bind "Ctrl b" { Write 102 111 111; }
                bind "Ctrl c" { WriteChars "hi there!"; }
                bind "Ctrl d" { SwitchToMode "Locked"; }
                bind "Ctrl e" { Resize "Increase"; }
                bind "Ctrl f" { FocusNextPane; }
                bind "Ctrl g" { FocusPreviousPane; }
                bind "Ctrl h" { SwitchFocus; }
                bind "Ctrl i" { MoveFocus "Right"; }
                bind "Ctrl j" { MoveFocusOrTab "Right"; }
                bind "Ctrl k" { MovePane "Right"; }
                bind "Ctrl l" { MovePaneBackwards; }
                bind "Ctrl m" { Resize "Decrease Down"; }
                bind "Ctrl n" { DumpScreen "/tmp/dumped"; }
                bind "Ctrl o" { DumpLayout "/tmp/dumped-layout"; }
                bind "Ctrl p" { EditScrollback; }
                bind "Ctrl q" { ScrollUp; }
                bind "Ctrl r" { ScrollDown; }
                bind "Ctrl s" { ScrollToBottom; }
                bind "Ctrl t" { ScrollToTop; }
                bind "Ctrl u" { PageScrollUp; }
                bind "Ctrl v" { PageScrollDown; }
                bind "Ctrl w" { HalfPageScrollUp; }
                bind "Ctrl x" { HalfPageScrollDown; }
                bind "Ctrl y" { ToggleFocusFullscreen; }
                bind "Ctrl z" { TogglePaneFrames; }
                bind "Alt a" { ToggleActiveSyncTab; }
                bind "Alt b" { NewPane "Right"; }
                bind "Alt c" { TogglePaneEmbedOrFloating; }
                bind "Alt d" { ToggleFloatingPanes; }
                bind "Alt e" { CloseFocus; }
                bind "Alt f" { PaneNameInput 0; }
                bind "Alt g" { UndoRenamePane; }
                bind "Alt h" { NewTab; }
                bind "Alt i" { GoToNextTab; }
                bind "Alt j" { GoToPreviousTab; }
                bind "Alt k" { CloseTab; }
                bind "Alt l" { GoToTab 1; }
                bind "Alt m" { ToggleTab; }
                bind "Alt n" { TabNameInput 0; }
                bind "Alt o" { UndoRenameTab; }
                bind "Alt p" { MoveTab "Right"; }
                bind "Alt q" {
                    Run "ls" "-l" {
                        hold_on_start true;
                        hold_on_close false;
                        cwd "/tmp";
                        name "my cool pane";
                    };
                }
                bind "Alt r" {
                    Run "ls" "-l" {
                        hold_on_start true;
                        hold_on_close false;
                        cwd "/tmp";
                        name "my cool pane";
                        floating true;
                    };
                }
                bind "Alt s" {
                    Run "ls" "-l" {
                        hold_on_start true;
                        hold_on_close false;
                        cwd "/tmp";
                        name "my cool pane";
                        in_place true;
                    };
                }
                bind "Alt t" { Detach; }
                bind "Alt u" {
                    LaunchOrFocusPlugin "zellij:session-manager"{
                        floating true;
                        move_to_focused_tab true;
                        skip_plugin_cache true;
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
                bind "Alt v" {
                    LaunchOrFocusPlugin "zellij:session-manager"{
                        in_place true;
                        move_to_focused_tab true;
                        skip_plugin_cache true;
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
                bind "Alt w" {
                    LaunchPlugin "zellij:session-manager" {
                        floating true;
                        skip_plugin_cache true;
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
                bind "Alt x" {
                    LaunchPlugin "zellij:session-manager"{
                        in_place true;
                        skip_plugin_cache true;
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
                bind "Alt y" { Copy; }
                bind "Alt z" { SearchInput 0; }
                bind "Ctrl Alt a" { Search "Up"; }
                bind "Ctrl Alt b" { SearchToggleOption "CaseSensitivity"; }
                bind "Ctrl Alt c" { ToggleMouseMode; }
                bind "Ctrl Alt d" { PreviousSwapLayout; }
                bind "Ctrl Alt e" { NextSwapLayout; }
                bind "Ctrl Alt g" { BreakPane; }
                bind "Ctrl Alt h" { BreakPaneRight; }
                bind "Ctrl Alt i" { BreakPaneLeft; }
                bind "Ctrl Alt i" { BreakPaneLeft; }
                bind "Ctrl Alt j" {
                    MessagePlugin "zellij:session-manager"{
                        name "message_name";
                        payload "message_payload";
                        cwd "/tmp";
                        launch_new true;
                        skip_cache true;
                        floating true;
                        title "plugin_title";
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = true;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    // uncomment the below lines for more easily debugging a failed assertion here
    //     for (input_mode, input_mode_keybinds) in deserialized.0 {
    //         if let Some(other_input_mode_keybinds) = deserialized_from_serialized.0.get(&input_mode) {
    //             for (keybind, action) in input_mode_keybinds {
    //                 if let Some(other_action) = other_input_mode_keybinds.get(&keybind) {
    //                     assert_eq!(&action, other_action);
    //                 } else {
    //                     eprintln!("keybind: {:?} not found in other", keybind);
    //                 }
    //             }
    //         }
    //     }
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn keybinds_to_string_with_shared_modes() {
    let fake_config = r#"
        keybinds {
            normal {
                bind "Ctrl n" { NewPane; SwitchToMode "Locked"; }
            }
            locked {
                bind "Ctrl n" { NewPane; SwitchToMode "Locked"; }
            }
            shared_except "locked" "pane" {
                bind "Ctrl f" { TogglePaneEmbedOrFloating; }
            }
            shared_among "locked" "pane" {
                bind "Ctrl p" { WriteChars "foo"; }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = true;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn keybinds_to_string_with_multiple_multiline_actions() {
    let fake_config = r#"
        keybinds {
            shared {
                bind "Ctrl n" {
                    NewPane
                    SwitchToMode "Locked"
                    MessagePlugin "zellij:session-manager"{
                        name "message_name";
                        payload "message_payload";
                        cwd "/tmp";
                        launch_new true;
                        skip_cache true;
                        floating true;
                        title "plugin_title";
                        config_key_1 "config_value_1";
                        config_key_2 "config_value_2";
                    };
                }
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Keybinds::from_kdl(
        document.get("keybinds").unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    let clear_defaults = true;
    let serialized = Keybinds::to_kdl(&deserialized, clear_defaults);
    let deserialized_from_serialized = Keybinds::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("keybinds")
            .unwrap(),
        Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn themes_to_string() {
    let fake_config = r#"
        themes {
           dracula {
                fg 248 248 242
                bg 40 42 54
                black 0 0 0
                red 255 85 85
                green 80 250 123
                yellow 241 250 140
                blue 98 114 164
                magenta 255 121 198
                cyan 139 233 253
                white 255 255 255
                orange 255 184 108
            }
        }"#;
    let document: KdlDocument = fake_config.parse().unwrap();
    let sourced_from_external_file = false;
    let deserialized =
        Themes::from_kdl(document.get("themes").unwrap(), sourced_from_external_file).unwrap();
    let serialized = Themes::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = Themes::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("themes")
            .unwrap(),
        sourced_from_external_file,
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config",
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn themes_to_string_with_hex_definitions() {
    let fake_config = r##"
        themes {
            nord {
                fg "#D8DEE9"
                bg "#2E3440"
                black "#3B4252"
                red "#BF616A"
                green "#A3BE8C"
                yellow "#EBCB8B"
                blue "#81A1C1"
                magenta "#B48EAD"
                cyan "#88C0D0"
                white "#E5E9F0"
                orange "#D08770"
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let sourced_from_external_file = false;
    let deserialized =
        Themes::from_kdl(document.get("themes").unwrap(), sourced_from_external_file).unwrap();
    let serialized = Themes::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = Themes::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("themes")
            .unwrap(),
        sourced_from_external_file,
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn themes_to_string_with_eight_bit_definitions() {
    let fake_config = r##"
        themes {
            default {
                fg 1
                bg 10
                black 20
                red 30
                green 40
                yellow 50
                blue 60
                magenta 70
                cyan 80
                white 90
                orange 254
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let sourced_from_external_file = false;
    let deserialized =
        Themes::from_kdl(document.get("themes").unwrap(), sourced_from_external_file).unwrap();
    let serialized = Themes::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = Themes::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("themes")
            .unwrap(),
        sourced_from_external_file,
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn themes_to_string_with_combined_definitions() {
    let fake_config = r##"
        themes {
            default {
                fg 1
                bg 10
                black 20
                red 30
                green 40
                yellow 50
                blue 60
                magenta 70
                cyan 80
                white 255 255 255
                orange "#D08770"
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let sourced_from_external_file = false;
    let deserialized =
        Themes::from_kdl(document.get("themes").unwrap(), sourced_from_external_file).unwrap();
    let serialized = Themes::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = Themes::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("themes")
            .unwrap(),
        sourced_from_external_file,
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn themes_to_string_with_multiple_theme_definitions() {
    let fake_config = r##"
        themes {
           nord {
               fg "#D8DEE9"
               bg "#2E3440"
               black "#3B4252"
               red "#BF616A"
               green "#A3BE8C"
               yellow "#EBCB8B"
               blue "#81A1C1"
               magenta "#B48EAD"
               cyan "#88C0D0"
               white "#E5E9F0"
               orange "#D08770"
           }
           dracula {
                fg 248 248 242
                bg 40 42 54
                black 0 0 0
                red 255 85 85
                green 80 250 123
                yellow 241 250 140
                blue 98 114 164
                magenta 255 121 198
                cyan 139 233 253
                white 255 255 255
                orange 255 184 108
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let sourced_from_external_file = false;
    let deserialized =
        Themes::from_kdl(document.get("themes").unwrap(), sourced_from_external_file).unwrap();
    let serialized = Themes::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = Themes::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("themes")
            .unwrap(),
        sourced_from_external_file,
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn plugins_to_string() {
    let fake_config = r##"
        plugins {
            tab-bar location="zellij:tab-bar"
            status-bar location="zellij:status-bar"
            strider location="zellij:strider"
            compact-bar location="zellij:compact-bar"
            session-manager location="zellij:session-manager"
            welcome-screen location="zellij:session-manager" {
                welcome_screen true
            }
            filepicker location="zellij:strider" {
                cwd "/"
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = PluginAliases::from_kdl(document.get("plugins").unwrap()).unwrap();
    let serialized = PluginAliases::to_kdl(&deserialized, true);
    let deserialized_from_serialized = PluginAliases::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("plugins")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn plugins_to_string_with_file_and_web() {
    let fake_config = r##"
        plugins {
            tab-bar location="https://foo.com/plugin.wasm"
            filepicker location="file:/path/to/my/plugin.wasm" {
                cwd "/"
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = PluginAliases::from_kdl(document.get("plugins").unwrap()).unwrap();
    let serialized = PluginAliases::to_kdl(&deserialized, true);
    let deserialized_from_serialized = PluginAliases::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("plugins")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn ui_config_to_string() {
    let fake_config = r##"
        ui {
            pane_frames {
                rounded_corners true
                hide_session_name true
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = UiConfig::from_kdl(document.get("ui").unwrap()).unwrap();
    let serialized = UiConfig::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = UiConfig::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("ui")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn ui_config_to_string_with_no_ui_config() {
    let fake_config = r##"
        ui {
            pane_frames {
            }
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = UiConfig::from_kdl(document.get("ui").unwrap()).unwrap();
    assert_eq!(UiConfig::to_kdl(&deserialized), None);
}

#[test]
fn env_vars_to_string() {
    let fake_config = r##"
        env {
            foo "bar"
            bar "foo"
            thing 1
            baz "true"
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = EnvironmentVariables::from_kdl(document.get("env").unwrap()).unwrap();
    let serialized = EnvironmentVariables::to_kdl(&deserialized).unwrap();
    let deserialized_from_serialized = EnvironmentVariables::from_kdl(
        serialized
            .to_string()
            .parse::<KdlDocument>()
            .unwrap()
            .get("env")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(serialized.to_string());
}

#[test]
fn env_vars_to_string_with_no_env_vars() {
    let fake_config = r##"
        env {
        }"##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = EnvironmentVariables::from_kdl(document.get("env").unwrap()).unwrap();
    assert_eq!(EnvironmentVariables::to_kdl(&deserialized), None);
}

#[test]
fn config_options_to_string() {
    let fake_config = r##"
        simplified_ui true
        theme "dracula"
        default_mode "locked"
        default_shell "fish"
        default_cwd "/tmp/foo"
        default_layout "compact"
        layout_dir "/tmp/layouts"
        theme_dir "/tmp/themes"
        mouse_mode false
        pane_frames false
        mirror_session true
        on_force_close "quit"
        scroll_buffer_size 100
        copy_command "pbcopy"
        copy_clipboard "system"
        copy_on_select false
        scrollback_editor "vim"
        session_name "my_cool_session"
        attach_to_session false
        auto_layout false
        session_serialization true
        serialize_pane_viewport false
        scrollback_lines_to_serialize 1000
        styled_underlines false
        serialization_interval 1
        disable_session_metadata true
        support_kitty_keyboard_protocol false
        web_server true
        web_sharing "disabled"
    "##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Options::from_kdl(&document).unwrap();
    let mut serialized = Options::to_kdl(&deserialized, false);
    let mut fake_document = KdlDocument::new();
    fake_document.nodes_mut().append(&mut serialized);
    let deserialized_from_serialized =
        Options::from_kdl(&fake_document.to_string().parse::<KdlDocument>().unwrap()).unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_document.to_string());
}

#[test]
fn config_options_to_string_with_comments() {
    let fake_config = r##"
        simplified_ui true
        theme "dracula"
        default_mode "locked"
        default_shell "fish"
        default_cwd "/tmp/foo"
        default_layout "compact"
        layout_dir "/tmp/layouts"
        theme_dir "/tmp/themes"
        mouse_mode false
        pane_frames false
        mirror_session true
        on_force_close "quit"
        scroll_buffer_size 100
        copy_command "pbcopy"
        copy_clipboard "system"
        copy_on_select false
        scrollback_editor "vim"
        session_name "my_cool_session"
        attach_to_session false
        auto_layout false
        session_serialization true
        serialize_pane_viewport false
        scrollback_lines_to_serialize 1000
        styled_underlines false
        serialization_interval 1
        disable_session_metadata true
        support_kitty_keyboard_protocol false
        web_server true
        web_sharing "disabled"
    "##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Options::from_kdl(&document).unwrap();
    let mut serialized = Options::to_kdl(&deserialized, true);
    let mut fake_document = KdlDocument::new();
    fake_document.nodes_mut().append(&mut serialized);
    let deserialized_from_serialized =
        Options::from_kdl(&fake_document.to_string().parse::<KdlDocument>().unwrap()).unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_document.to_string());
}

#[test]
fn config_options_to_string_without_options() {
    let fake_config = r##"
    "##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Options::from_kdl(&document).unwrap();
    let mut serialized = Options::to_kdl(&deserialized, false);
    let mut fake_document = KdlDocument::new();
    fake_document.nodes_mut().append(&mut serialized);
    let deserialized_from_serialized =
        Options::from_kdl(&fake_document.to_string().parse::<KdlDocument>().unwrap()).unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_document.to_string());
}

#[test]
fn config_options_to_string_with_some_options() {
    let fake_config = r##"
        default_layout "compact"
    "##;
    let document: KdlDocument = fake_config.parse().unwrap();
    let deserialized = Options::from_kdl(&document).unwrap();
    let mut serialized = Options::to_kdl(&deserialized, false);
    let mut fake_document = KdlDocument::new();
    fake_document.nodes_mut().append(&mut serialized);
    let deserialized_from_serialized =
        Options::from_kdl(&fake_document.to_string().parse::<KdlDocument>().unwrap()).unwrap();
    assert_eq!(
        deserialized, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_document.to_string());
}

#[test]
fn bare_config_from_default_assets_to_string() {
    let fake_config = Config::from_default_assets().unwrap();
    let fake_config_stringified = fake_config.to_string(false);
    let deserialized_from_serialized = Config::from_kdl(&fake_config_stringified, None).unwrap();
    assert_eq!(
        fake_config, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_config_stringified);
}

#[test]
fn bare_config_from_default_assets_to_string_with_comments() {
    let fake_config = Config::from_default_assets().unwrap();
    let fake_config_stringified = fake_config.to_string(true);
    let deserialized_from_serialized = Config::from_kdl(&fake_config_stringified, None).unwrap();
    assert_eq!(
        fake_config, deserialized_from_serialized,
        "Deserialized serialized config equals original config"
    );
    insta::assert_snapshot!(fake_config_stringified);
}
