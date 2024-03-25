pub use super::generated_api::api::{
    action::{
        action::OptionalPayload, Action as ProtobufAction, ActionName as ProtobufActionName,
        DumpScreenPayload, EditFilePayload, GoToTabNamePayload, IdAndName,
        LaunchOrFocusPluginPayload, MovePanePayload, MoveTabDirection as ProtobufMoveTabDirection,
        NameAndValue as ProtobufNameAndValue, NewFloatingPanePayload, NewPanePayload,
        NewPluginPanePayload, NewTiledPanePayload, PaneIdAndShouldFloat,
        PluginConfiguration as ProtobufPluginConfiguration, Position as ProtobufPosition,
        RunCommandAction as ProtobufRunCommandAction, ScrollAtPayload,
        SearchDirection as ProtobufSearchDirection, SearchOption as ProtobufSearchOption,
        SwitchToModePayload, WriteCharsPayload, WritePayload,
    },
    input_mode::InputMode as ProtobufInputMode,
    resize::{Resize as ProtobufResize, ResizeDirection as ProtobufResizeDirection},
};
use crate::data::{Direction, InputMode, ResizeStrategy};
use crate::errors::prelude::*;
use crate::input::actions::Action;
use crate::input::actions::{SearchDirection, SearchOption};
use crate::input::command::RunCommandAction;
use crate::input::layout::{
    PluginUserConfiguration, RunPlugin, RunPluginLocation, RunPluginOrAlias,
};
use crate::position::Position;

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::path::PathBuf;

impl TryFrom<ProtobufAction> for Action {
    type Error = &'static str;
    fn try_from(protobuf_action: ProtobufAction) -> Result<Self, &'static str> {
        match ProtobufActionName::from_i32(protobuf_action.name) {
            Some(ProtobufActionName::Quit) => match protobuf_action.optional_payload {
                Some(_) => Err("The Quit Action should not have a payload"),
                None => Ok(Action::Quit),
            },
            Some(ProtobufActionName::Write) => match protobuf_action.optional_payload {
                Some(OptionalPayload::WritePayload(write_payload)) => {
                    Ok(Action::Write(write_payload.bytes_to_write))
                },
                _ => Err("Wrong payload for Action::Write"),
            },
            Some(ProtobufActionName::WriteChars) => match protobuf_action.optional_payload {
                Some(OptionalPayload::WriteCharsPayload(write_chars_payload)) => {
                    Ok(Action::WriteChars(write_chars_payload.chars))
                },
                _ => Err("Wrong payload for Action::WriteChars"),
            },
            Some(ProtobufActionName::SwitchToMode) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SwitchToModePayload(switch_to_mode_payload)) => {
                    let input_mode: InputMode =
                        ProtobufInputMode::from_i32(switch_to_mode_payload.input_mode)
                            .ok_or("Malformed input mode for SwitchToMode Action")?
                            .try_into()?;
                    Ok(Action::SwitchToMode(input_mode))
                },
                _ => Err("Wrong payload for Action::SwitchToModePayload"),
            },
            Some(ProtobufActionName::SwitchModeForAllClients) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::SwitchModeForAllClientsPayload(
                        switch_to_mode_payload,
                    )) => {
                        let input_mode: InputMode =
                            ProtobufInputMode::from_i32(switch_to_mode_payload.input_mode)
                                .ok_or("Malformed input mode for SwitchToMode Action")?
                                .try_into()?;
                        Ok(Action::SwitchModeForAllClients(input_mode))
                    },
                    _ => Err("Wrong payload for Action::SwitchModeForAllClients"),
                }
            },
            Some(ProtobufActionName::Resize) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ResizePayload(resize_payload)) => {
                    let resize_strategy: ResizeStrategy = resize_payload.try_into()?;
                    Ok(Action::Resize(
                        resize_strategy.resize,
                        resize_strategy.direction,
                    ))
                },
                _ => Err("Wrong payload for Action::Resize"),
            },
            Some(ProtobufActionName::FocusNextPane) => match protobuf_action.optional_payload {
                Some(_) => Err("FocusNextPane should not have a payload"),
                None => Ok(Action::FocusNextPane),
            },
            Some(ProtobufActionName::FocusPreviousPane) => match protobuf_action.optional_payload {
                Some(_) => Err("FocusPreviousPane should not have a payload"),
                None => Ok(Action::FocusPreviousPane),
            },
            Some(ProtobufActionName::SwitchFocus) => match protobuf_action.optional_payload {
                Some(_) => Err("SwitchFocus should not have a payload"),
                None => Ok(Action::SwitchFocus),
            },
            Some(ProtobufActionName::MoveFocus) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MoveFocusPayload(move_focus_payload)) => {
                    let direction: Direction =
                        ProtobufResizeDirection::from_i32(move_focus_payload)
                            .ok_or("Malformed resize direction for Action::MoveFocus")?
                            .try_into()?;
                    Ok(Action::MoveFocus(direction))
                },
                _ => Err("Wrong payload for Action::MoveFocus"),
            },
            Some(ProtobufActionName::MoveFocusOrTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MoveFocusOrTabPayload(move_focus_or_tab_payload)) => {
                    let direction: Direction =
                        ProtobufResizeDirection::from_i32(move_focus_or_tab_payload)
                            .ok_or("Malformed resize direction for Action::MoveFocusOrTab")?
                            .try_into()?;
                    Ok(Action::MoveFocusOrTab(direction))
                },
                _ => Err("Wrong payload for Action::MoveFocusOrTab"),
            },
            Some(ProtobufActionName::MovePane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MovePanePayload(payload)) => {
                    let direction: Option<Direction> = payload
                        .direction
                        .and_then(|d| ProtobufResizeDirection::from_i32(d))
                        .and_then(|d| d.try_into().ok());
                    Ok(Action::MovePane(direction))
                },
                _ => Err("Wrong payload for Action::MovePane"),
            },
            Some(ProtobufActionName::MovePaneBackwards) => match protobuf_action.optional_payload {
                Some(_) => Err("MovePaneBackwards should not have a payload"),
                None => Ok(Action::MovePaneBackwards),
            },
            Some(ProtobufActionName::ClearScreen) => match protobuf_action.optional_payload {
                Some(_) => Err("ClearScreen should not have a payload"),
                None => Ok(Action::ClearScreen),
            },
            Some(ProtobufActionName::DumpScreen) => match protobuf_action.optional_payload {
                Some(OptionalPayload::DumpScreenPayload(payload)) => {
                    let file_path = payload.file_path;
                    let include_scrollback = payload.include_scrollback;
                    Ok(Action::DumpScreen(file_path, include_scrollback))
                },
                _ => Err("Wrong payload for Action::DumpScreen"),
            },
            Some(ProtobufActionName::EditScrollback) => match protobuf_action.optional_payload {
                Some(_) => Err("EditScrollback should not have a payload"),
                None => Ok(Action::EditScrollback),
            },
            Some(ProtobufActionName::ScrollUp) => match protobuf_action.optional_payload {
                Some(_) => Err("ScrollUp should not have a payload"),
                None => Ok(Action::ScrollUp),
            },
            Some(ProtobufActionName::ScrollDown) => match protobuf_action.optional_payload {
                Some(_) => Err("ScrollDown should not have a payload"),
                None => Ok(Action::ScrollDown),
            },
            Some(ProtobufActionName::ScrollUpAt) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ScrollUpAtPayload(payload)) => {
                    let position = payload
                        .position
                        .ok_or("ScrollUpAtPayload must have a position")?
                        .try_into()?;
                    Ok(Action::ScrollUpAt(position))
                },
                _ => Err("Wrong payload for Action::ScrollUpAt"),
            },
            Some(ProtobufActionName::ScrollDownAt) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ScrollDownAtPayload(payload)) => {
                    let position = payload
                        .position
                        .ok_or("ScrollDownAtPayload must have a position")?
                        .try_into()?;
                    Ok(Action::ScrollDownAt(position))
                },
                _ => Err("Wrong payload for Action::ScrollDownAt"),
            },
            Some(ProtobufActionName::ScrollToBottom) => match protobuf_action.optional_payload {
                Some(_) => Err("ScrollToBottom should not have a payload"),
                None => Ok(Action::ScrollToBottom),
            },
            Some(ProtobufActionName::ScrollToTop) => match protobuf_action.optional_payload {
                Some(_) => Err("ScrollToTop should not have a payload"),
                None => Ok(Action::ScrollToTop),
            },
            Some(ProtobufActionName::PageScrollUp) => match protobuf_action.optional_payload {
                Some(_) => Err("PageScrollUp should not have a payload"),
                None => Ok(Action::PageScrollUp),
            },
            Some(ProtobufActionName::PageScrollDown) => match protobuf_action.optional_payload {
                Some(_) => Err("PageScrollDown should not have a payload"),
                None => Ok(Action::PageScrollDown),
            },
            Some(ProtobufActionName::HalfPageScrollUp) => match protobuf_action.optional_payload {
                Some(_) => Err("HalfPageScrollUp should not have a payload"),
                None => Ok(Action::HalfPageScrollUp),
            },
            Some(ProtobufActionName::HalfPageScrollDown) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("HalfPageScrollDown should not have a payload"),
                    None => Ok(Action::HalfPageScrollDown),
                }
            },
            Some(ProtobufActionName::ToggleFocusFullscreen) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("ToggleFocusFullscreen should not have a payload"),
                    None => Ok(Action::ToggleFocusFullscreen),
                }
            },
            Some(ProtobufActionName::TogglePaneFrames) => match protobuf_action.optional_payload {
                Some(_) => Err("TogglePaneFrames should not have a payload"),
                None => Ok(Action::TogglePaneFrames),
            },
            Some(ProtobufActionName::ToggleActiveSyncTab) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("ToggleActiveSyncTab should not have a payload"),
                    None => Ok(Action::ToggleActiveSyncTab),
                }
            },
            Some(ProtobufActionName::NewPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewPanePayload(payload)) => {
                    let direction: Option<Direction> = payload
                        .direction
                        .and_then(|d| ProtobufResizeDirection::from_i32(d))
                        .and_then(|d| d.try_into().ok());
                    let pane_name = payload.pane_name;
                    Ok(Action::NewPane(direction, pane_name))
                },
                _ => Err("Wrong payload for Action::NewPane"),
            },
            Some(ProtobufActionName::EditFile) => match protobuf_action.optional_payload {
                Some(OptionalPayload::EditFilePayload(payload)) => {
                    let file_to_edit = PathBuf::from(payload.file_to_edit);
                    let line_number: Option<usize> = payload.line_number.map(|l| l as usize);
                    let cwd: Option<PathBuf> = payload.cwd.map(|p| PathBuf::from(p));
                    let direction: Option<Direction> = payload
                        .direction
                        .and_then(|d| ProtobufResizeDirection::from_i32(d))
                        .and_then(|d| d.try_into().ok());
                    let should_float = payload.should_float;
                    let should_be_in_place = false;
                    Ok(Action::EditFile(
                        file_to_edit,
                        line_number,
                        cwd,
                        direction,
                        should_float,
                        should_be_in_place,
                        None,
                    ))
                },
                _ => Err("Wrong payload for Action::NewPane"),
            },
            Some(ProtobufActionName::NewFloatingPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewFloatingPanePayload(payload)) => {
                    if let Some(payload) = payload.command {
                        let pane_name = payload.pane_name.clone();
                        let run_command_action: RunCommandAction = payload.try_into()?;
                        Ok(Action::NewFloatingPane(
                            Some(run_command_action),
                            pane_name,
                            None,
                        ))
                    } else {
                        Ok(Action::NewFloatingPane(None, None, None))
                    }
                },
                _ => Err("Wrong payload for Action::NewFloatingPane"),
            },
            Some(ProtobufActionName::NewTiledPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewTiledPanePayload(payload)) => {
                    let direction: Option<Direction> = payload
                        .direction
                        .and_then(|d| ProtobufResizeDirection::from_i32(d))
                        .and_then(|d| d.try_into().ok());
                    if let Some(payload) = payload.command {
                        let pane_name = payload.pane_name.clone();
                        let run_command_action: RunCommandAction = payload.try_into()?;
                        Ok(Action::NewTiledPane(
                            direction,
                            Some(run_command_action),
                            pane_name,
                        ))
                    } else {
                        Ok(Action::NewTiledPane(direction, None, None))
                    }
                },
                _ => Err("Wrong payload for Action::NewTiledPane"),
            },
            Some(ProtobufActionName::TogglePaneEmbedOrFloating) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("TogglePaneEmbedOrFloating should not have a payload"),
                    None => Ok(Action::TogglePaneEmbedOrFloating),
                }
            },
            Some(ProtobufActionName::ToggleFloatingPanes) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("ToggleFloatingPanes should not have a payload"),
                    None => Ok(Action::ToggleFloatingPanes),
                }
            },
            Some(ProtobufActionName::CloseFocus) => match protobuf_action.optional_payload {
                Some(_) => Err("CloseFocus should not have a payload"),
                None => Ok(Action::CloseFocus),
            },
            Some(ProtobufActionName::PaneNameInput) => match protobuf_action.optional_payload {
                Some(OptionalPayload::PaneNameInputPayload(bytes)) => {
                    Ok(Action::PaneNameInput(bytes))
                },
                _ => Err("Wrong payload for Action::PaneNameInput"),
            },
            Some(ProtobufActionName::UndoRenamePane) => match protobuf_action.optional_payload {
                Some(_) => Err("UndoRenamePane should not have a payload"),
                None => Ok(Action::UndoRenamePane),
            },
            Some(ProtobufActionName::NewTab) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("NewTab should not have a payload"),
                    None => {
                        // we do not serialize the layouts of this action
                        Ok(Action::NewTab(None, vec![], None, None, None))
                    },
                }
            },
            Some(ProtobufActionName::NoOp) => match protobuf_action.optional_payload {
                Some(_) => Err("NoOp should not have a payload"),
                None => Ok(Action::NoOp),
            },
            Some(ProtobufActionName::GoToNextTab) => match protobuf_action.optional_payload {
                Some(_) => Err("GoToNextTab should not have a payload"),
                None => Ok(Action::GoToNextTab),
            },
            Some(ProtobufActionName::GoToPreviousTab) => match protobuf_action.optional_payload {
                Some(_) => Err("GoToPreviousTab should not have a payload"),
                None => Ok(Action::GoToPreviousTab),
            },
            Some(ProtobufActionName::CloseTab) => match protobuf_action.optional_payload {
                Some(_) => Err("CloseTab should not have a payload"),
                None => Ok(Action::CloseTab),
            },
            Some(ProtobufActionName::GoToTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::GoToTabPayload(index)) => Ok(Action::GoToTab(index)),
                _ => Err("Wrong payload for Action::GoToTab"),
            },
            Some(ProtobufActionName::GoToTabName) => match protobuf_action.optional_payload {
                Some(OptionalPayload::GoToTabNamePayload(payload)) => {
                    let tab_name = payload.tab_name;
                    let create = payload.create;
                    Ok(Action::GoToTabName(tab_name, create))
                },
                _ => Err("Wrong payload for Action::GoToTabName"),
            },
            Some(ProtobufActionName::ToggleTab) => match protobuf_action.optional_payload {
                Some(_) => Err("ToggleTab should not have a payload"),
                None => Ok(Action::ToggleTab),
            },
            Some(ProtobufActionName::TabNameInput) => match protobuf_action.optional_payload {
                Some(OptionalPayload::TabNameInputPayload(bytes)) => {
                    Ok(Action::TabNameInput(bytes))
                },
                _ => Err("Wrong payload for Action::TabNameInput"),
            },
            Some(ProtobufActionName::UndoRenameTab) => match protobuf_action.optional_payload {
                Some(_) => Err("UndoRenameTab should not have a payload"),
                None => Ok(Action::UndoRenameTab),
            },
            Some(ProtobufActionName::MoveTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MoveTabPayload(move_tab_payload)) => {
                    let direction: Direction = ProtobufMoveTabDirection::from_i32(move_tab_payload)
                        .ok_or("Malformed move tab direction for Action::MoveTab")?
                        .try_into()?;
                    Ok(Action::MoveTab(direction))
                },
                _ => Err("Wrong payload for Action::MoveTab"),
            },
            Some(ProtobufActionName::Run) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RunPayload(run_command_action)) => {
                    let run_command_action = run_command_action.try_into()?;
                    Ok(Action::Run(run_command_action))
                },
                _ => Err("Wrong payload for Action::Run"),
            },
            Some(ProtobufActionName::Detach) => match protobuf_action.optional_payload {
                Some(_) => Err("Detach should not have a payload"),
                None => Ok(Action::Detach),
            },
            Some(ProtobufActionName::LeftClick) => match protobuf_action.optional_payload {
                Some(OptionalPayload::LeftClickPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::LeftClick(position))
                },
                _ => Err("Wrong payload for Action::LeftClick"),
            },
            Some(ProtobufActionName::RightClick) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RightClickPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::RightClick(position))
                },
                _ => Err("Wrong payload for Action::RightClick"),
            },
            Some(ProtobufActionName::MiddleClick) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MiddleClickPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MiddleClick(position))
                },
                _ => Err("Wrong payload for Action::MiddleClick"),
            },
            Some(ProtobufActionName::LaunchOrFocusPlugin) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::LaunchOrFocusPluginPayload(payload)) => {
                        let configuration: PluginUserConfiguration = payload
                            .plugin_configuration
                            .and_then(|p| PluginUserConfiguration::try_from(p).ok())
                            .unwrap_or_default();
                        let run_plugin_or_alias = RunPluginOrAlias::from_url(
                            &payload.plugin_url.as_str(),
                            &Some(configuration.inner().clone()),
                            None,
                            None,
                        )
                        .map_err(|_| "Malformed LaunchOrFocusPlugin payload")?;
                        let should_float = payload.should_float;
                        let move_to_focused_tab = payload.move_to_focused_tab;
                        let should_open_in_place = payload.should_open_in_place;
                        let skip_plugin_cache = payload.skip_plugin_cache;
                        Ok(Action::LaunchOrFocusPlugin(
                            run_plugin_or_alias,
                            should_float,
                            move_to_focused_tab,
                            should_open_in_place,
                            skip_plugin_cache,
                        ))
                    },
                    _ => Err("Wrong payload for Action::LaunchOrFocusPlugin"),
                }
            },
            Some(ProtobufActionName::LaunchPlugin) => match protobuf_action.optional_payload {
                Some(OptionalPayload::LaunchOrFocusPluginPayload(payload)) => {
                    let configuration: PluginUserConfiguration = payload
                        .plugin_configuration
                        .and_then(|p| PluginUserConfiguration::try_from(p).ok())
                        .unwrap_or_default();
                    let run_plugin_or_alias = RunPluginOrAlias::from_url(
                        &payload.plugin_url.as_str(),
                        &Some(configuration.inner().clone()),
                        None,
                        None,
                    )
                    .map_err(|_| "Malformed LaunchOrFocusPlugin payload")?;
                    let should_float = payload.should_float;
                    let _move_to_focused_tab = payload.move_to_focused_tab; // not actually used in
                                                                            // this action
                    let should_open_in_place = payload.should_open_in_place;
                    let skip_plugin_cache = payload.skip_plugin_cache;
                    Ok(Action::LaunchPlugin(
                        run_plugin_or_alias,
                        should_float,
                        should_open_in_place,
                        skip_plugin_cache,
                        None,
                    ))
                },
                _ => Err("Wrong payload for Action::LaunchOrFocusPlugin"),
            },
            Some(ProtobufActionName::LeftMouseRelease) => match protobuf_action.optional_payload {
                Some(OptionalPayload::LeftMouseReleasePayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::LeftMouseRelease(position))
                },
                _ => Err("Wrong payload for Action::LeftMouseRelease"),
            },
            Some(ProtobufActionName::RightMouseRelease) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RightMouseReleasePayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::RightMouseRelease(position))
                },
                _ => Err("Wrong payload for Action::RightMouseRelease"),
            },
            Some(ProtobufActionName::MiddleMouseRelease) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::MiddleMouseReleasePayload(payload)) => {
                        let position = payload.try_into()?;
                        Ok(Action::MiddleMouseRelease(position))
                    },
                    _ => Err("Wrong payload for Action::MiddleMouseRelease"),
                }
            },
            Some(ProtobufActionName::MouseHoldLeft) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MouseHoldLeftPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseHoldLeft(position))
                },
                _ => Err("Wrong payload for Action::MouseHoldLeft"),
            },
            Some(ProtobufActionName::MouseHoldRight) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MouseHoldRightPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseHoldRight(position))
                },
                _ => Err("Wrong payload for Action::MouseHoldRight"),
            },
            Some(ProtobufActionName::MouseHoldMiddle) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MouseHoldMiddlePayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseHoldMiddle(position))
                },
                _ => Err("Wrong payload for Action::MouseHoldMiddle"),
            },
            Some(ProtobufActionName::SearchInput) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SearchInputPayload(payload)) => {
                    Ok(Action::SearchInput(payload))
                },
                _ => Err("Wrong payload for Action::SearchInput"),
            },
            Some(ProtobufActionName::Search) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SearchPayload(search_direction)) => Ok(Action::Search(
                    ProtobufSearchDirection::from_i32(search_direction)
                        .ok_or("Malformed payload for Action::Search")?
                        .try_into()?,
                )),
                _ => Err("Wrong payload for Action::Search"),
            },
            Some(ProtobufActionName::SearchToggleOption) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::SearchToggleOptionPayload(search_option)) => {
                        Ok(Action::SearchToggleOption(
                            ProtobufSearchOption::from_i32(search_option)
                                .ok_or("Malformed payload for Action::SearchToggleOption")?
                                .try_into()?,
                        ))
                    },
                    _ => Err("Wrong payload for Action::SearchToggleOption"),
                }
            },
            Some(ProtobufActionName::ToggleMouseMode) => match protobuf_action.optional_payload {
                Some(_) => Err("ToggleMouseMode should not have a payload"),
                None => Ok(Action::ToggleMouseMode),
            },
            Some(ProtobufActionName::PreviousSwapLayout) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("PreviousSwapLayout should not have a payload"),
                    None => Ok(Action::PreviousSwapLayout),
                }
            },
            Some(ProtobufActionName::NextSwapLayout) => match protobuf_action.optional_payload {
                Some(_) => Err("NextSwapLayout should not have a payload"),
                None => Ok(Action::NextSwapLayout),
            },
            Some(ProtobufActionName::QueryTabNames) => match protobuf_action.optional_payload {
                Some(_) => Err("QueryTabNames should not have a payload"),
                None => Ok(Action::QueryTabNames),
            },
            Some(ProtobufActionName::NewTiledPluginPane) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::NewTiledPluginPanePayload(payload)) => {
                        let run_plugin_location =
                            RunPluginLocation::parse(&payload.plugin_url, None)
                                .map_err(|_| "Malformed NewTiledPluginPane payload")?;
                        let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
                            location: run_plugin_location,
                            _allow_exec_host_cmd: false,
                            configuration: PluginUserConfiguration::default(),
                            ..Default::default()
                        });
                        let pane_name = payload.pane_name;
                        let skip_plugin_cache = payload.skip_plugin_cache;
                        Ok(Action::NewTiledPluginPane(
                            run_plugin,
                            pane_name,
                            skip_plugin_cache,
                            None,
                        ))
                    },
                    _ => Err("Wrong payload for Action::NewTiledPluginPane"),
                }
            },
            Some(ProtobufActionName::NewFloatingPluginPane) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::NewFloatingPluginPanePayload(payload)) => {
                        let run_plugin_location =
                            RunPluginLocation::parse(&payload.plugin_url, None)
                                .map_err(|_| "Malformed NewTiledPluginPane payload")?;
                        let run_plugin = RunPluginOrAlias::RunPlugin(RunPlugin {
                            location: run_plugin_location,
                            _allow_exec_host_cmd: false,
                            configuration: PluginUserConfiguration::default(),
                            ..Default::default()
                        });
                        let pane_name = payload.pane_name;
                        let skip_plugin_cache = payload.skip_plugin_cache;
                        Ok(Action::NewFloatingPluginPane(
                            run_plugin,
                            pane_name,
                            skip_plugin_cache,
                            None,
                            None,
                        ))
                    },
                    _ => Err("Wrong payload for Action::MiddleClick"),
                }
            },
            Some(ProtobufActionName::StartOrReloadPlugin) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::StartOrReloadPluginPayload(payload)) => {
                        let run_plugin_or_alias =
                            RunPluginOrAlias::from_url(&payload.as_str(), &None, None, None)
                                .map_err(|_| "Malformed LaunchOrFocusPlugin payload")?;

                        Ok(Action::StartOrReloadPlugin(run_plugin_or_alias))
                    },
                    _ => Err("Wrong payload for Action::StartOrReloadPlugin"),
                }
            },
            Some(ProtobufActionName::CloseTerminalPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::CloseTerminalPanePayload(payload)) => {
                    Ok(Action::CloseTerminalPane(payload))
                },
                _ => Err("Wrong payload for Action::CloseTerminalPane"),
            },
            Some(ProtobufActionName::ClosePluginPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ClosePluginPanePayload(payload)) => {
                    Ok(Action::ClosePluginPane(payload))
                },
                _ => Err("Wrong payload for Action::ClosePluginPane"),
            },
            Some(ProtobufActionName::FocusTerminalPaneWithId) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::FocusTerminalPaneWithIdPayload(payload)) => {
                        let terminal_pane_id = payload.pane_id;
                        let should_float_if_hidden = payload.should_float;
                        Ok(Action::FocusTerminalPaneWithId(
                            terminal_pane_id,
                            should_float_if_hidden,
                        ))
                    },
                    _ => Err("Wrong payload for Action::FocusTerminalPaneWithId"),
                }
            },
            Some(ProtobufActionName::FocusPluginPaneWithId) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::FocusPluginPaneWithIdPayload(payload)) => {
                        let plugin_pane_id = payload.pane_id;
                        let should_float_if_hidden = payload.should_float;
                        Ok(Action::FocusPluginPaneWithId(
                            plugin_pane_id,
                            should_float_if_hidden,
                        ))
                    },
                    _ => Err("Wrong payload for Action::FocusPluginPaneWithId"),
                }
            },
            Some(ProtobufActionName::RenameTerminalPane) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::RenameTerminalPanePayload(payload)) => {
                        let terminal_pane_id = payload.id;
                        let new_pane_name = payload.name;
                        Ok(Action::RenameTerminalPane(terminal_pane_id, new_pane_name))
                    },
                    _ => Err("Wrong payload for Action::RenameTerminalPane"),
                }
            },
            Some(ProtobufActionName::RenamePluginPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RenamePluginPanePayload(payload)) => {
                    let plugin_pane_id = payload.id;
                    let new_pane_name = payload.name;
                    Ok(Action::RenamePluginPane(plugin_pane_id, new_pane_name))
                },
                _ => Err("Wrong payload for Action::RenamePluginPane"),
            },
            Some(ProtobufActionName::RenameTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RenameTabPayload(payload)) => {
                    let tab_index = payload.id;
                    let new_tab_name = payload.name;
                    Ok(Action::RenameTab(tab_index, new_tab_name))
                },
                _ => Err("Wrong payload for Action::RenameTab"),
            },
            Some(ProtobufActionName::BreakPane) => match protobuf_action.optional_payload {
                Some(_) => Err("BreakPane should not have a payload"),
                None => Ok(Action::BreakPane),
            },
            Some(ProtobufActionName::BreakPaneRight) => match protobuf_action.optional_payload {
                Some(_) => Err("BreakPaneRight should not have a payload"),
                None => Ok(Action::BreakPaneRight),
            },
            Some(ProtobufActionName::BreakPaneLeft) => match protobuf_action.optional_payload {
                Some(_) => Err("BreakPaneLeft should not have a payload"),
                None => Ok(Action::BreakPaneLeft),
            },
            Some(ProtobufActionName::RenameSession) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RenameSessionPayload(name)) => {
                    Ok(Action::RenameSession(name))
                },
                _ => Err("Wrong payload for Action::RenameSession"),
            },
            Some(ProtobufActionName::KeybindPipe) => match protobuf_action.optional_payload {
                Some(_) => Err("KeybindPipe should not have a payload"),
                // TODO: at some point we might want to support a payload here
                None => Ok(Action::KeybindPipe {
                    name: None,
                    payload: None,
                    args: None,
                    plugin: None,
                    configuration: None,
                    launch_new: false,
                    skip_cache: false,
                    floating: None,
                    in_place: None,
                    cwd: None,
                    pane_title: None,
                }),
            },
            _ => Err("Unknown Action"),
        }
    }
}

impl TryFrom<Action> for ProtobufAction {
    type Error = &'static str;
    fn try_from(action: Action) -> Result<Self, &'static str> {
        match action {
            Action::Quit => Ok(ProtobufAction {
                name: ProtobufActionName::Quit as i32,
                optional_payload: None,
            }),
            Action::Write(bytes) => Ok(ProtobufAction {
                name: ProtobufActionName::Write as i32,
                optional_payload: Some(OptionalPayload::WritePayload(WritePayload {
                    bytes_to_write: bytes,
                })),
            }),
            Action::WriteChars(chars_to_write) => Ok(ProtobufAction {
                name: ProtobufActionName::WriteChars as i32,
                optional_payload: Some(OptionalPayload::WriteCharsPayload(WriteCharsPayload {
                    chars: chars_to_write,
                })),
            }),
            Action::SwitchToMode(input_mode) => {
                let input_mode: ProtobufInputMode = input_mode.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::SwitchToMode as i32,
                    optional_payload: Some(OptionalPayload::SwitchToModePayload(
                        SwitchToModePayload {
                            input_mode: input_mode as i32,
                        },
                    )),
                })
            },
            Action::SwitchModeForAllClients(input_mode) => {
                let input_mode: ProtobufInputMode = input_mode.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::SwitchModeForAllClients as i32,
                    optional_payload: Some(OptionalPayload::SwitchModeForAllClientsPayload(
                        SwitchToModePayload {
                            input_mode: input_mode as i32,
                        },
                    )),
                })
            },
            Action::Resize(resize, direction) => {
                let mut resize: ProtobufResize = resize.try_into()?;
                resize.direction = direction.and_then(|d| {
                    let resize_direction: ProtobufResizeDirection = d.try_into().ok()?;
                    Some(resize_direction as i32)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::Resize as i32,
                    optional_payload: Some(OptionalPayload::ResizePayload(resize)),
                })
            },
            Action::FocusNextPane => Ok(ProtobufAction {
                name: ProtobufActionName::FocusNextPane as i32,
                optional_payload: None,
            }),
            Action::FocusPreviousPane => Ok(ProtobufAction {
                name: ProtobufActionName::FocusPreviousPane as i32,
                optional_payload: None,
            }),
            Action::SwitchFocus => Ok(ProtobufAction {
                name: ProtobufActionName::SwitchFocus as i32,
                optional_payload: None,
            }),
            Action::MoveFocus(direction) => {
                let direction: ProtobufResizeDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveFocus as i32,
                    optional_payload: Some(OptionalPayload::MoveFocusPayload(direction as i32)),
                })
            },
            Action::MoveFocusOrTab(direction) => {
                let direction: ProtobufResizeDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveFocusOrTab as i32,
                    optional_payload: Some(OptionalPayload::MoveFocusOrTabPayload(
                        direction as i32,
                    )),
                })
            },
            Action::MovePane(direction) => {
                let direction = direction.and_then(|direction| {
                    let protobuf_direction: ProtobufResizeDirection = direction.try_into().ok()?;
                    Some(protobuf_direction as i32)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::MovePane as i32,
                    optional_payload: Some(OptionalPayload::MovePanePayload(MovePanePayload {
                        direction,
                    })),
                })
            },
            Action::MovePaneBackwards => Ok(ProtobufAction {
                name: ProtobufActionName::MovePaneBackwards as i32,
                optional_payload: None,
            }),
            Action::ClearScreen => Ok(ProtobufAction {
                name: ProtobufActionName::ClearScreen as i32,
                optional_payload: None,
            }),
            Action::DumpScreen(file_path, include_scrollback) => Ok(ProtobufAction {
                name: ProtobufActionName::DumpScreen as i32,
                optional_payload: Some(OptionalPayload::DumpScreenPayload(DumpScreenPayload {
                    file_path,
                    include_scrollback,
                })),
            }),
            Action::EditScrollback => Ok(ProtobufAction {
                name: ProtobufActionName::EditScrollback as i32,
                optional_payload: None,
            }),
            Action::ScrollUp => Ok(ProtobufAction {
                name: ProtobufActionName::ScrollUp as i32,
                optional_payload: None,
            }),
            Action::ScrollUpAt(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::ScrollUpAt as i32,
                    optional_payload: Some(OptionalPayload::ScrollUpAtPayload(ScrollAtPayload {
                        position: Some(position),
                    })),
                })
            },
            Action::ScrollDown => Ok(ProtobufAction {
                name: ProtobufActionName::ScrollDown as i32,
                optional_payload: None,
            }),
            Action::ScrollDownAt(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::ScrollDownAt as i32,
                    optional_payload: Some(OptionalPayload::ScrollDownAtPayload(ScrollAtPayload {
                        position: Some(position),
                    })),
                })
            },
            Action::ScrollToBottom => Ok(ProtobufAction {
                name: ProtobufActionName::ScrollToBottom as i32,
                optional_payload: None,
            }),
            Action::ScrollToTop => Ok(ProtobufAction {
                name: ProtobufActionName::ScrollToTop as i32,
                optional_payload: None,
            }),
            Action::PageScrollUp => Ok(ProtobufAction {
                name: ProtobufActionName::PageScrollUp as i32,
                optional_payload: None,
            }),
            Action::PageScrollDown => Ok(ProtobufAction {
                name: ProtobufActionName::PageScrollDown as i32,
                optional_payload: None,
            }),
            Action::HalfPageScrollUp => Ok(ProtobufAction {
                name: ProtobufActionName::HalfPageScrollUp as i32,
                optional_payload: None,
            }),
            Action::HalfPageScrollDown => Ok(ProtobufAction {
                name: ProtobufActionName::HalfPageScrollDown as i32,
                optional_payload: None,
            }),
            Action::ToggleFocusFullscreen => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleFocusFullscreen as i32,
                optional_payload: None,
            }),
            Action::TogglePaneFrames => Ok(ProtobufAction {
                name: ProtobufActionName::TogglePaneFrames as i32,
                optional_payload: None,
            }),
            Action::ToggleActiveSyncTab => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleActiveSyncTab as i32,
                optional_payload: None,
            }),
            Action::NewPane(direction, new_pane_name) => {
                let direction = direction.and_then(|direction| {
                    let protobuf_direction: ProtobufResizeDirection = direction.try_into().ok()?;
                    Some(protobuf_direction as i32)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewPane as i32,
                    optional_payload: Some(OptionalPayload::NewPanePayload(NewPanePayload {
                        direction,
                        pane_name: new_pane_name,
                    })),
                })
            },
            Action::EditFile(
                path_to_file,
                line_number,
                cwd,
                direction,
                should_float,
                _should_be_in_place,
                _floating_pane_coordinates,
            ) => {
                let file_to_edit = path_to_file.display().to_string();
                let cwd = cwd.map(|cwd| cwd.display().to_string());
                let direction: Option<i32> = direction
                    .and_then(|d| ProtobufResizeDirection::try_from(d).ok())
                    .map(|d| d as i32);
                let line_number = line_number.map(|l| l as u32);
                Ok(ProtobufAction {
                    name: ProtobufActionName::EditFile as i32,
                    optional_payload: Some(OptionalPayload::EditFilePayload(EditFilePayload {
                        file_to_edit,
                        line_number,
                        should_float,
                        direction,
                        cwd,
                    })),
                })
            },
            Action::NewFloatingPane(run_command_action, pane_name, _coordinates) => {
                let command = run_command_action.and_then(|r| {
                    let mut protobuf_run_command_action: ProtobufRunCommandAction =
                        r.try_into().ok()?;
                    protobuf_run_command_action.pane_name = pane_name;
                    Some(protobuf_run_command_action)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewFloatingPane as i32,
                    optional_payload: Some(OptionalPayload::NewFloatingPanePayload(
                        NewFloatingPanePayload { command },
                    )),
                })
            },
            Action::NewTiledPane(direction, run_command_action, pane_name) => {
                let direction = direction.and_then(|direction| {
                    let protobuf_direction: ProtobufResizeDirection = direction.try_into().ok()?;
                    Some(protobuf_direction as i32)
                });
                let command = run_command_action.and_then(|r| {
                    let mut protobuf_run_command_action: ProtobufRunCommandAction =
                        r.try_into().ok()?;
                    let pane_name = pane_name.and_then(|n| n.try_into().ok());
                    protobuf_run_command_action.pane_name = pane_name;
                    Some(protobuf_run_command_action)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewTiledPane as i32,
                    optional_payload: Some(OptionalPayload::NewTiledPanePayload(
                        NewTiledPanePayload { direction, command },
                    )),
                })
            },
            Action::TogglePaneEmbedOrFloating => Ok(ProtobufAction {
                name: ProtobufActionName::TogglePaneEmbedOrFloating as i32,
                optional_payload: None,
            }),
            Action::ToggleFloatingPanes => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleFloatingPanes as i32,
                optional_payload: None,
            }),
            Action::CloseFocus => Ok(ProtobufAction {
                name: ProtobufActionName::CloseFocus as i32,
                optional_payload: None,
            }),
            Action::PaneNameInput(bytes) => Ok(ProtobufAction {
                name: ProtobufActionName::PaneNameInput as i32,
                optional_payload: Some(OptionalPayload::PaneNameInputPayload(bytes)),
            }),
            Action::UndoRenamePane => Ok(ProtobufAction {
                name: ProtobufActionName::UndoRenamePane as i32,
                optional_payload: None,
            }),
            Action::NewTab(..) => {
                // we do not serialize the various newtab payloads
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewTab as i32,
                    optional_payload: None,
                })
            },
            Action::GoToNextTab => Ok(ProtobufAction {
                name: ProtobufActionName::GoToNextTab as i32,
                optional_payload: None,
            }),
            Action::GoToPreviousTab => Ok(ProtobufAction {
                name: ProtobufActionName::GoToPreviousTab as i32,
                optional_payload: None,
            }),
            Action::CloseTab => Ok(ProtobufAction {
                name: ProtobufActionName::CloseTab as i32,
                optional_payload: None,
            }),
            Action::GoToTab(tab_index) => Ok(ProtobufAction {
                name: ProtobufActionName::GoToTab as i32,
                optional_payload: Some(OptionalPayload::GoToTabPayload(tab_index)),
            }),
            Action::GoToTabName(tab_name, create) => Ok(ProtobufAction {
                name: ProtobufActionName::GoToTabName as i32,
                optional_payload: Some(OptionalPayload::GoToTabNamePayload(GoToTabNamePayload {
                    tab_name,
                    create,
                })),
            }),
            Action::ToggleTab => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleTab as i32,
                optional_payload: None,
            }),
            Action::TabNameInput(bytes) => Ok(ProtobufAction {
                name: ProtobufActionName::TabNameInput as i32,
                optional_payload: Some(OptionalPayload::TabNameInputPayload(bytes)),
            }),
            Action::UndoRenameTab => Ok(ProtobufAction {
                name: ProtobufActionName::UndoRenameTab as i32,
                optional_payload: None,
            }),
            Action::MoveTab(direction) => {
                let direction: ProtobufMoveTabDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveTab as i32,
                    optional_payload: Some(OptionalPayload::MoveTabPayload(direction as i32)),
                })
            },
            Action::Run(run_command_action) => {
                let run_command_action: ProtobufRunCommandAction = run_command_action.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::Run as i32,
                    optional_payload: Some(OptionalPayload::RunPayload(run_command_action)),
                })
            },
            Action::Detach => Ok(ProtobufAction {
                name: ProtobufActionName::Detach as i32,
                optional_payload: None,
            }),
            Action::LeftClick(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::LeftClick as i32,
                    optional_payload: Some(OptionalPayload::LeftClickPayload(position)),
                })
            },
            Action::RightClick(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::RightClick as i32,
                    optional_payload: Some(OptionalPayload::RightClickPayload(position)),
                })
            },
            Action::MiddleClick(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MiddleClick as i32,
                    optional_payload: Some(OptionalPayload::MiddleClickPayload(position)),
                })
            },
            Action::LaunchOrFocusPlugin(
                run_plugin_or_alias,
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                skip_plugin_cache,
            ) => {
                let configuration = run_plugin_or_alias.get_configuration().unwrap_or_default();
                Ok(ProtobufAction {
                    name: ProtobufActionName::LaunchOrFocusPlugin as i32,
                    optional_payload: Some(OptionalPayload::LaunchOrFocusPluginPayload(
                        LaunchOrFocusPluginPayload {
                            plugin_url: run_plugin_or_alias.location_string(),
                            should_float,
                            move_to_focused_tab,
                            should_open_in_place,
                            plugin_configuration: Some(configuration.try_into()?),
                            skip_plugin_cache,
                        },
                    )),
                })
            },
            Action::LaunchPlugin(
                run_plugin_or_alias,
                should_float,
                should_open_in_place,
                skip_plugin_cache,
                _cwd,
            ) => {
                let configuration = run_plugin_or_alias.get_configuration().unwrap_or_default();
                Ok(ProtobufAction {
                    name: ProtobufActionName::LaunchPlugin as i32,
                    optional_payload: Some(OptionalPayload::LaunchOrFocusPluginPayload(
                        LaunchOrFocusPluginPayload {
                            plugin_url: run_plugin_or_alias.location_string(),
                            should_float,
                            move_to_focused_tab: false,
                            should_open_in_place,
                            plugin_configuration: Some(configuration.try_into()?),
                            skip_plugin_cache,
                        },
                    )),
                })
            },
            Action::LeftMouseRelease(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::LeftMouseRelease as i32,
                    optional_payload: Some(OptionalPayload::LeftMouseReleasePayload(position)),
                })
            },
            Action::RightMouseRelease(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::RightMouseRelease as i32,
                    optional_payload: Some(OptionalPayload::RightMouseReleasePayload(position)),
                })
            },
            Action::MiddleMouseRelease(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MiddleMouseRelease as i32,
                    optional_payload: Some(OptionalPayload::MiddleMouseReleasePayload(position)),
                })
            },
            Action::MouseHoldLeft(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MouseHoldLeft as i32,
                    optional_payload: Some(OptionalPayload::MouseHoldLeftPayload(position)),
                })
            },
            Action::MouseHoldRight(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MouseHoldRight as i32,
                    optional_payload: Some(OptionalPayload::MouseHoldRightPayload(position)),
                })
            },
            Action::MouseHoldMiddle(position) => {
                let position: ProtobufPosition = position.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MouseHoldMiddle as i32,
                    optional_payload: Some(OptionalPayload::MouseHoldMiddlePayload(position)),
                })
            },
            Action::SearchInput(bytes) => Ok(ProtobufAction {
                name: ProtobufActionName::SearchInput as i32,
                optional_payload: Some(OptionalPayload::SearchInputPayload(bytes)),
            }),
            Action::Search(search_direction) => {
                let search_direction: ProtobufSearchDirection = search_direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::Search as i32,
                    optional_payload: Some(OptionalPayload::SearchPayload(search_direction as i32)),
                })
            },
            Action::SearchToggleOption(search_option) => {
                let search_option: ProtobufSearchOption = search_option.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::SearchToggleOption as i32,
                    optional_payload: Some(OptionalPayload::SearchToggleOptionPayload(
                        search_option as i32,
                    )),
                })
            },
            Action::ToggleMouseMode => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleMouseMode as i32,
                optional_payload: None,
            }),
            Action::PreviousSwapLayout => Ok(ProtobufAction {
                name: ProtobufActionName::PreviousSwapLayout as i32,
                optional_payload: None,
            }),
            Action::NextSwapLayout => Ok(ProtobufAction {
                name: ProtobufActionName::NextSwapLayout as i32,
                optional_payload: None,
            }),
            Action::QueryTabNames => Ok(ProtobufAction {
                name: ProtobufActionName::QueryTabNames as i32,
                optional_payload: None,
            }),
            Action::NewTiledPluginPane(run_plugin, pane_name, skip_plugin_cache, _cwd) => {
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewTiledPluginPane as i32,
                    optional_payload: Some(OptionalPayload::NewTiledPluginPanePayload(
                        NewPluginPanePayload {
                            plugin_url: run_plugin.location_string(),
                            pane_name,
                            skip_plugin_cache,
                        },
                    )),
                })
            },
            Action::NewFloatingPluginPane(
                run_plugin,
                pane_name,
                skip_plugin_cache,
                _cwd,
                _coordinates,
            ) => Ok(ProtobufAction {
                name: ProtobufActionName::NewFloatingPluginPane as i32,
                optional_payload: Some(OptionalPayload::NewFloatingPluginPanePayload(
                    NewPluginPanePayload {
                        plugin_url: run_plugin.location_string(),
                        pane_name,
                        skip_plugin_cache,
                    },
                )),
            }),
            Action::StartOrReloadPlugin(run_plugin) => Ok(ProtobufAction {
                name: ProtobufActionName::StartOrReloadPlugin as i32,
                optional_payload: Some(OptionalPayload::StartOrReloadPluginPayload(
                    run_plugin.location_string(),
                )),
            }),
            Action::CloseTerminalPane(terminal_pane_id) => Ok(ProtobufAction {
                name: ProtobufActionName::CloseTerminalPane as i32,
                optional_payload: Some(OptionalPayload::CloseTerminalPanePayload(terminal_pane_id)),
            }),
            Action::ClosePluginPane(plugin_pane_id) => Ok(ProtobufAction {
                name: ProtobufActionName::ClosePluginPane as i32,
                optional_payload: Some(OptionalPayload::ClosePluginPanePayload(plugin_pane_id)),
            }),
            Action::FocusTerminalPaneWithId(terminal_pane_id, should_float_if_hidden) => {
                Ok(ProtobufAction {
                    name: ProtobufActionName::FocusTerminalPaneWithId as i32,
                    optional_payload: Some(OptionalPayload::FocusTerminalPaneWithIdPayload(
                        PaneIdAndShouldFloat {
                            pane_id: terminal_pane_id,
                            should_float: should_float_if_hidden,
                        },
                    )),
                })
            },
            Action::FocusPluginPaneWithId(plugin_pane_id, should_float_if_hidden) => {
                Ok(ProtobufAction {
                    name: ProtobufActionName::FocusPluginPaneWithId as i32,
                    optional_payload: Some(OptionalPayload::FocusPluginPaneWithIdPayload(
                        PaneIdAndShouldFloat {
                            pane_id: plugin_pane_id,
                            should_float: should_float_if_hidden,
                        },
                    )),
                })
            },
            Action::RenameTerminalPane(terminal_pane_id, new_name) => Ok(ProtobufAction {
                name: ProtobufActionName::RenameTerminalPane as i32,
                optional_payload: Some(OptionalPayload::RenameTerminalPanePayload(IdAndName {
                    name: new_name,
                    id: terminal_pane_id,
                })),
            }),
            Action::RenamePluginPane(plugin_pane_id, new_name) => Ok(ProtobufAction {
                name: ProtobufActionName::RenamePluginPane as i32,
                optional_payload: Some(OptionalPayload::RenamePluginPanePayload(IdAndName {
                    name: new_name,
                    id: plugin_pane_id,
                })),
            }),
            Action::RenameTab(tab_index, new_name) => Ok(ProtobufAction {
                name: ProtobufActionName::RenameTab as i32,
                optional_payload: Some(OptionalPayload::RenameTabPayload(IdAndName {
                    name: new_name,
                    id: tab_index,
                })),
            }),
            Action::BreakPane => Ok(ProtobufAction {
                name: ProtobufActionName::BreakPane as i32,
                optional_payload: None,
            }),
            Action::BreakPaneRight => Ok(ProtobufAction {
                name: ProtobufActionName::BreakPaneRight as i32,
                optional_payload: None,
            }),
            Action::BreakPaneLeft => Ok(ProtobufAction {
                name: ProtobufActionName::BreakPaneLeft as i32,
                optional_payload: None,
            }),
            Action::RenameSession(session_name) => Ok(ProtobufAction {
                name: ProtobufActionName::RenameSession as i32,
                optional_payload: Some(OptionalPayload::RenameSessionPayload(session_name)),
            }),
            Action::KeybindPipe { .. } => Ok(ProtobufAction {
                name: ProtobufActionName::KeybindPipe as i32,
                optional_payload: None,
            }),
            Action::NoOp
            | Action::Confirm
            | Action::NewInPlacePane(..)
            | Action::NewInPlacePluginPane(..)
            | Action::Deny
            | Action::Copy
            | Action::DumpLayout
            | Action::CliPipe { .. }
            | Action::SkipConfirm(..) => Err("Unsupported action"),
        }
    }
}

impl TryFrom<ProtobufSearchOption> for SearchOption {
    type Error = &'static str;
    fn try_from(protobuf_search_option: ProtobufSearchOption) -> Result<Self, &'static str> {
        match protobuf_search_option {
            ProtobufSearchOption::CaseSensitivity => Ok(SearchOption::CaseSensitivity),
            ProtobufSearchOption::WholeWord => Ok(SearchOption::WholeWord),
            ProtobufSearchOption::Wrap => Ok(SearchOption::Wrap),
        }
    }
}

impl TryFrom<SearchOption> for ProtobufSearchOption {
    type Error = &'static str;
    fn try_from(search_option: SearchOption) -> Result<Self, &'static str> {
        match search_option {
            SearchOption::CaseSensitivity => Ok(ProtobufSearchOption::CaseSensitivity),
            SearchOption::WholeWord => Ok(ProtobufSearchOption::WholeWord),
            SearchOption::Wrap => Ok(ProtobufSearchOption::Wrap),
        }
    }
}

impl TryFrom<ProtobufSearchDirection> for SearchDirection {
    type Error = &'static str;
    fn try_from(protobuf_search_direction: ProtobufSearchDirection) -> Result<Self, &'static str> {
        match protobuf_search_direction {
            ProtobufSearchDirection::Up => Ok(SearchDirection::Up),
            ProtobufSearchDirection::Down => Ok(SearchDirection::Down),
        }
    }
}

impl TryFrom<SearchDirection> for ProtobufSearchDirection {
    type Error = &'static str;
    fn try_from(search_direction: SearchDirection) -> Result<Self, &'static str> {
        match search_direction {
            SearchDirection::Up => Ok(ProtobufSearchDirection::Up),
            SearchDirection::Down => Ok(ProtobufSearchDirection::Down),
        }
    }
}

impl TryFrom<ProtobufMoveTabDirection> for Direction {
    type Error = &'static str;
    fn try_from(
        protobuf_move_tab_direction: ProtobufMoveTabDirection,
    ) -> Result<Self, &'static str> {
        match protobuf_move_tab_direction {
            ProtobufMoveTabDirection::Left => Ok(Direction::Left),
            ProtobufMoveTabDirection::Right => Ok(Direction::Right),
        }
    }
}

impl TryFrom<Direction> for ProtobufMoveTabDirection {
    type Error = &'static str;
    fn try_from(direction: Direction) -> Result<Self, &'static str> {
        match direction {
            Direction::Left => Ok(ProtobufMoveTabDirection::Left),
            Direction::Right => Ok(ProtobufMoveTabDirection::Right),
            _ => Err("Wrong direction for ProtobufMoveTabDirection"),
        }
    }
}

impl TryFrom<ProtobufRunCommandAction> for RunCommandAction {
    type Error = &'static str;
    fn try_from(
        protobuf_run_command_action: ProtobufRunCommandAction,
    ) -> Result<Self, &'static str> {
        let command = PathBuf::from(protobuf_run_command_action.command);
        let args: Vec<String> = protobuf_run_command_action.args;
        let cwd: Option<PathBuf> = protobuf_run_command_action.cwd.map(|c| PathBuf::from(c));
        let direction: Option<Direction> = protobuf_run_command_action
            .direction
            .and_then(|d| ProtobufResizeDirection::from_i32(d))
            .and_then(|d| d.try_into().ok());
        let hold_on_close = protobuf_run_command_action.hold_on_close;
        let hold_on_start = protobuf_run_command_action.hold_on_start;
        Ok(RunCommandAction {
            command,
            args,
            cwd,
            direction,
            hold_on_close,
            hold_on_start,
        })
    }
}

impl TryFrom<RunCommandAction> for ProtobufRunCommandAction {
    type Error = &'static str;
    fn try_from(run_command_action: RunCommandAction) -> Result<Self, &'static str> {
        let command = run_command_action.command.display().to_string();
        let args: Vec<String> = run_command_action.args;
        let cwd = run_command_action.cwd.map(|c| c.display().to_string());
        let direction = run_command_action.direction.and_then(|p| {
            let direction: ProtobufResizeDirection = p.try_into().ok()?;
            Some(direction as i32)
        });
        let hold_on_close = run_command_action.hold_on_close;
        let hold_on_start = run_command_action.hold_on_start;
        Ok(ProtobufRunCommandAction {
            command,
            args,
            cwd,
            direction,
            hold_on_close,
            hold_on_start,
            pane_name: None,
        })
    }
}

impl TryFrom<ProtobufPosition> for Position {
    type Error = &'static str;
    fn try_from(protobuf_position: ProtobufPosition) -> Result<Self, &'static str> {
        Ok(Position::new(
            protobuf_position.line as i32,
            protobuf_position.column as u16,
        ))
    }
}

impl TryFrom<Position> for ProtobufPosition {
    type Error = &'static str;
    fn try_from(position: Position) -> Result<Self, &'static str> {
        Ok(ProtobufPosition {
            line: position.line.0 as i64,
            column: position.column.0 as i64,
        })
    }
}

impl TryFrom<ProtobufPluginConfiguration> for PluginUserConfiguration {
    type Error = &'static str;
    fn try_from(plugin_configuration: ProtobufPluginConfiguration) -> Result<Self, &'static str> {
        let mut converted = BTreeMap::new();
        for name_and_value in plugin_configuration.name_and_value {
            converted.insert(name_and_value.name, name_and_value.value);
        }
        Ok(PluginUserConfiguration::new(converted))
    }
}

impl TryFrom<PluginUserConfiguration> for ProtobufPluginConfiguration {
    type Error = &'static str;
    fn try_from(plugin_configuration: PluginUserConfiguration) -> Result<Self, &'static str> {
        let mut converted = vec![];
        for (name, value) in plugin_configuration.inner() {
            let name_and_value = ProtobufNameAndValue {
                name: name.to_owned(),
                value: value.to_owned(),
            };
            converted.push(name_and_value);
        }
        Ok(ProtobufPluginConfiguration {
            name_and_value: converted,
        })
    }
}

impl TryFrom<&ProtobufPluginConfiguration> for BTreeMap<String, String> {
    type Error = &'static str;
    fn try_from(plugin_configuration: &ProtobufPluginConfiguration) -> Result<Self, &'static str> {
        let mut converted = BTreeMap::new();
        for name_and_value in &plugin_configuration.name_and_value {
            converted.insert(
                name_and_value.name.to_owned(),
                name_and_value.value.to_owned(),
            );
        }
        Ok(converted)
    }
}
