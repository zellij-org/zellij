pub use super::generated_api::api::{
    action::{
        action::OptionalPayload,
        command_or_plugin::CommandOrPluginType,
        pane_run::RunType,
        run_plugin_location_data::LocationData,
        run_plugin_or_alias::PluginType,
        Action as ProtobufAction,
        ActionName as ProtobufActionName,
        BareKey as ProtobufBareKey,
        // New layout-related types
        CommandOrPlugin as ProtobufCommandOrPlugin,
        DumpScreenPayload,
        EditFilePayload,
        FloatingPaneCoordinates as ProtobufFloatingPaneCoordinates,
        FloatingPaneLayout as ProtobufFloatingPaneLayout,
        FloatingPlacement as ProtobufFloatingPlacement,
        GoToTabNamePayload,
        IdAndName,
        InPlaceConfig as ProtobufInPlaceConfig,
        KeyModifier as ProtobufKeyModifier,
        KeyWithModifier as ProtobufKeyWithModifier,
        LaunchOrFocusPluginPayload,
        LayoutConstraint as ProtobufLayoutConstraint,
        LayoutConstraintFloatingPair as ProtobufLayoutConstraintFloatingPair,
        LayoutConstraintTiledPair as ProtobufLayoutConstraintTiledPair,
        LayoutConstraintWithValue as ProtobufLayoutConstraintWithValue,
        MouseEventPayload as ProtobufMouseEventPayload,
        MovePanePayload,
        MoveTabDirection as ProtobufMoveTabDirection,
        NameAndValue as ProtobufNameAndValue,
        NewBlockingPanePayload,
        NewFloatingPanePayload,
        NewInPlacePanePayload,
        NewPanePayload,
        NewPanePlacement as ProtobufNewPanePlacement,
        NewPluginPanePayload,
        NewTabPayload,
        NewTiledPanePayload,
        OverrideLayoutPayload,
        PaneId as ProtobufPaneId,
        PaneIdAndShouldFloat,
        PaneRun as ProtobufPaneRun,
        PercentOrFixed as ProtobufPercentOrFixed,
        PluginAlias as ProtobufPluginAlias,
        PluginConfiguration as ProtobufPluginConfiguration,
        PluginTag as ProtobufPluginTag,
        PluginUserConfiguration as ProtobufPluginUserConfiguration,
        Position as ProtobufPosition,
        RunCommandAction as ProtobufRunCommandAction,
        RunEditFileAction as ProtobufRunEditFileAction,
        RunPlugin as ProtobufRunPlugin,
        RunPluginLocation as ProtobufRunPluginLocation,
        RunPluginLocationData as ProtobufRunPluginLocationData,
        RunPluginOrAlias as ProtobufRunPluginOrAlias,
        ScrollAtPayload,
        SearchDirection as ProtobufSearchDirection,
        SearchOption as ProtobufSearchOption,
        SplitDirection as ProtobufSplitDirection,
        SplitSize as ProtobufSplitSize,
        StackedPlacement as ProtobufStackedPlacement,
        SwapFloatingLayout as ProtobufSwapFloatingLayout,
        SwapTiledLayout as ProtobufSwapTiledLayout,
        SwitchToModePayload,
        TiledPaneLayout as ProtobufTiledPaneLayout,
        TiledPlacement as ProtobufTiledPlacement,
        UnblockCondition as ProtobufUnblockCondition,
        WriteCharsPayload,
        WritePayload,
    },
    input_mode::InputMode as ProtobufInputMode,
    resize::{Resize as ProtobufResize, ResizeDirection as ProtobufResizeDirection},
};
use crate::data::{
    CommandOrPlugin, Direction, FloatingPaneCoordinates, InputMode, KeyWithModifier,
    NewPanePlacement, PaneId, PluginTag, ResizeStrategy, UnblockCondition,
};
use crate::errors::prelude::*;
use crate::input::actions::Action;
use crate::input::actions::{SearchDirection, SearchOption};
use crate::input::command::{OpenFilePayload, RunCommandAction};
use crate::input::layout::SplitSize;
use crate::input::layout::{
    FloatingPaneLayout, LayoutConstraint, PercentOrFixed, PluginAlias, PluginUserConfiguration,
    Run, RunPlugin, RunPluginLocation, RunPluginOrAlias, SplitDirection, SwapFloatingLayout,
    SwapTiledLayout, TiledPaneLayout,
};
use crate::input::mouse::{MouseEvent, MouseEventType};
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
                    let key_with_modifier = write_payload
                        .key_with_modifier
                        .and_then(|k| k.try_into().ok());
                    Ok(Action::Write {
                        key_with_modifier,
                        bytes: write_payload.bytes_to_write,
                        is_kitty_keyboard_protocol: write_payload.is_kitty_keyboard_protocol,
                    })
                },
                _ => Err("Wrong payload for Action::Write"),
            },
            Some(ProtobufActionName::WriteChars) => match protobuf_action.optional_payload {
                Some(OptionalPayload::WriteCharsPayload(write_chars_payload)) => {
                    Ok(Action::WriteChars {
                        chars: write_chars_payload.chars,
                    })
                },
                _ => Err("Wrong payload for Action::WriteChars"),
            },
            Some(ProtobufActionName::SwitchToMode) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SwitchToModePayload(switch_to_mode_payload)) => {
                    let input_mode: InputMode =
                        ProtobufInputMode::from_i32(switch_to_mode_payload.input_mode)
                            .ok_or("Malformed input mode for SwitchToMode Action")?
                            .try_into()?;
                    Ok(Action::SwitchToMode { input_mode })
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
                        Ok(Action::SwitchModeForAllClients { input_mode })
                    },
                    _ => Err("Wrong payload for Action::SwitchModeForAllClients"),
                }
            },
            Some(ProtobufActionName::Resize) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ResizePayload(resize_payload)) => {
                    let resize_strategy: ResizeStrategy = resize_payload.try_into()?;
                    Ok(Action::Resize {
                        resize: resize_strategy.resize,
                        direction: resize_strategy.direction,
                    })
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
                    Ok(Action::MoveFocus { direction })
                },
                _ => Err("Wrong payload for Action::MoveFocus"),
            },
            Some(ProtobufActionName::MoveFocusOrTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MoveFocusOrTabPayload(move_focus_or_tab_payload)) => {
                    let direction: Direction =
                        ProtobufResizeDirection::from_i32(move_focus_or_tab_payload)
                            .ok_or("Malformed resize direction for Action::MoveFocusOrTab")?
                            .try_into()?;
                    Ok(Action::MoveFocusOrTab { direction })
                },
                _ => Err("Wrong payload for Action::MoveFocusOrTab"),
            },
            Some(ProtobufActionName::MovePane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MovePanePayload(payload)) => {
                    let direction: Option<Direction> = payload
                        .direction
                        .and_then(|d| ProtobufResizeDirection::from_i32(d))
                        .and_then(|d| d.try_into().ok());
                    Ok(Action::MovePane { direction })
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
                    Ok(Action::DumpScreen {
                        file_path,
                        include_scrollback,
                    })
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
                    Ok(Action::ScrollUpAt { position })
                },
                _ => Err("Wrong payload for Action::ScrollUpAt"),
            },
            Some(ProtobufActionName::ScrollDownAt) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ScrollDownAtPayload(payload)) => {
                    let position = payload
                        .position
                        .ok_or("ScrollDownAtPayload must have a position")?
                        .try_into()?;
                    Ok(Action::ScrollDownAt { position })
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
                    Ok(Action::NewPane {
                        direction,
                        pane_name,
                        start_suppressed: false,
                    })
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
                    let near_current_pane = payload.near_current_pane;
                    let should_float = payload.should_float;
                    let should_be_in_place = false;
                    Ok(Action::EditFile {
                        payload: OpenFilePayload::new(file_to_edit, line_number, cwd),
                        direction,
                        floating: should_float,
                        in_place: should_be_in_place,
                        start_suppressed: false,
                        coordinates: None,
                        near_current_pane,
                    })
                },
                _ => Err("Wrong payload for Action::NewPane"),
            },
            Some(ProtobufActionName::NewFloatingPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewFloatingPanePayload(payload)) => {
                    let near_current_pane = payload.near_current_pane;
                    if let Some(payload) = payload.command {
                        let pane_name = payload.pane_name.clone();
                        let run_command_action: RunCommandAction = payload.try_into()?;
                        Ok(Action::NewFloatingPane {
                            command: Some(run_command_action),
                            pane_name,
                            coordinates: None,
                            near_current_pane,
                        })
                    } else {
                        Ok(Action::NewFloatingPane {
                            command: None,
                            pane_name: None,
                            coordinates: None,
                            near_current_pane,
                        })
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
                    let near_current_pane = payload.near_current_pane;
                    if let Some(payload) = payload.command {
                        let pane_name = payload.pane_name.clone();
                        let run_command_action: RunCommandAction = payload.try_into()?;
                        Ok(Action::NewTiledPane {
                            direction,
                            command: Some(run_command_action),
                            pane_name,
                            near_current_pane,
                        })
                    } else {
                        Ok(Action::NewTiledPane {
                            direction,
                            command: None,
                            pane_name: None,
                            near_current_pane,
                        })
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
                    Ok(Action::PaneNameInput { input: bytes })
                },
                _ => Err("Wrong payload for Action::PaneNameInput"),
            },
            Some(ProtobufActionName::UndoRenamePane) => match protobuf_action.optional_payload {
                Some(_) => Err("UndoRenamePane should not have a payload"),
                None => Ok(Action::UndoRenamePane),
            },
            Some(ProtobufActionName::NewTab) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::NewTabPayload(payload)) => {
                        // New behavior: extract all fields from payload
                        let tiled_layout =
                            payload.tiled_layout.map(|l| l.try_into()).transpose()?;

                        let floating_layouts = payload
                            .floating_layouts
                            .into_iter()
                            .map(|l| l.try_into())
                            .collect::<Result<Vec<_>, _>>()?;

                        let swap_tiled_layouts = if payload.swap_tiled_layouts.is_empty() {
                            None
                        } else {
                            Some(
                                payload
                                    .swap_tiled_layouts
                                    .into_iter()
                                    .map(|l| l.try_into())
                                    .collect::<Result<Vec<_>, _>>()?,
                            )
                        };

                        let swap_floating_layouts = if payload.swap_floating_layouts.is_empty() {
                            None
                        } else {
                            Some(
                                payload
                                    .swap_floating_layouts
                                    .into_iter()
                                    .map(|l| l.try_into())
                                    .collect::<Result<Vec<_>, _>>()?,
                            )
                        };

                        let tab_name = payload.tab_name;
                        let should_change_focus_to_new_tab = payload.should_change_focus_to_new_tab;
                        let cwd = payload.cwd.map(PathBuf::from);

                        let initial_panes = if payload.initial_panes.is_empty() {
                            None
                        } else {
                            Some(
                                payload
                                    .initial_panes
                                    .into_iter()
                                    .map(|p| p.try_into())
                                    .collect::<Result<Vec<_>, _>>()?,
                            )
                        };

                        let first_pane_unblock_condition = payload
                            .first_pane_unblock_condition
                            .and_then(|uc| ProtobufUnblockCondition::from_i32(uc))
                            .and_then(|uc| uc.try_into().ok());

                        Ok(Action::NewTab {
                            tiled_layout,
                            floating_layouts,
                            swap_tiled_layouts,
                            swap_floating_layouts,
                            tab_name,
                            should_change_focus_to_new_tab,
                            cwd,
                            initial_panes,
                            first_pane_unblock_condition,
                        })
                    },
                    None => {
                        // Backwards compatibility: accept None payload for existing plugins
                        // Return the same defaults as before
                        Ok(Action::NewTab {
                            tiled_layout: None,
                            floating_layouts: vec![],
                            swap_tiled_layouts: None,
                            swap_floating_layouts: None,
                            tab_name: None,
                            should_change_focus_to_new_tab: true,
                            cwd: None,
                            initial_panes: None,
                            first_pane_unblock_condition: None,
                        })
                    },
                    _ => Err("Wrong payload for Action::NewTab"),
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
                Some(OptionalPayload::GoToTabPayload(index)) => Ok(Action::GoToTab { index }),
                _ => Err("Wrong payload for Action::GoToTab"),
            },
            Some(ProtobufActionName::GoToTabName) => match protobuf_action.optional_payload {
                Some(OptionalPayload::GoToTabNamePayload(payload)) => {
                    let tab_name = payload.tab_name;
                    let create = payload.create;
                    Ok(Action::GoToTabName {
                        name: tab_name,
                        create,
                    })
                },
                _ => Err("Wrong payload for Action::GoToTabName"),
            },
            Some(ProtobufActionName::ToggleTab) => match protobuf_action.optional_payload {
                Some(_) => Err("ToggleTab should not have a payload"),
                None => Ok(Action::ToggleTab),
            },
            Some(ProtobufActionName::TabNameInput) => match protobuf_action.optional_payload {
                Some(OptionalPayload::TabNameInputPayload(bytes)) => {
                    Ok(Action::TabNameInput { input: bytes })
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
                    Ok(Action::MoveTab { direction })
                },
                _ => Err("Wrong payload for Action::MoveTab"),
            },
            Some(ProtobufActionName::Run) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RunPayload(run_command_action)) => {
                    let run_command_action = run_command_action.try_into()?;
                    Ok(Action::Run {
                        command: run_command_action,
                        near_current_pane: false,
                    })
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
                    Ok(Action::MouseEvent {
                        event: MouseEvent::new_left_press_event(position),
                    })
                },
                _ => Err("Wrong payload for Action::LeftClick"),
            },
            Some(ProtobufActionName::RightClick) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RightClickPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseEvent {
                        event: MouseEvent::new_right_press_event(position),
                    })
                },
                _ => Err("Wrong payload for Action::RightClick"),
            },
            Some(ProtobufActionName::MiddleClick) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MiddleClickPayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseEvent {
                        event: MouseEvent::new_middle_press_event(position),
                    })
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
                        Ok(Action::LaunchOrFocusPlugin {
                            plugin: run_plugin_or_alias,
                            should_float,
                            move_to_focused_tab,
                            should_open_in_place,
                            skip_cache: skip_plugin_cache,
                        })
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
                    Ok(Action::LaunchPlugin {
                        plugin: run_plugin_or_alias,
                        should_float,
                        should_open_in_place,
                        skip_cache: skip_plugin_cache,
                        cwd: None,
                    })
                },
                _ => Err("Wrong payload for Action::LaunchOrFocusPlugin"),
            },
            Some(ProtobufActionName::LeftMouseRelease) => match protobuf_action.optional_payload {
                Some(OptionalPayload::LeftMouseReleasePayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseEvent {
                        event: MouseEvent::new_left_release_event(position),
                    })
                },
                _ => Err("Wrong payload for Action::LeftMouseRelease"),
            },
            Some(ProtobufActionName::RightMouseRelease) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RightMouseReleasePayload(payload)) => {
                    let position = payload.try_into()?;
                    Ok(Action::MouseEvent {
                        event: MouseEvent::new_right_release_event(position),
                    })
                },
                _ => Err("Wrong payload for Action::RightMouseRelease"),
            },
            Some(ProtobufActionName::MiddleMouseRelease) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::MiddleMouseReleasePayload(payload)) => {
                        let position = payload.try_into()?;
                        Ok(Action::MouseEvent {
                            event: MouseEvent::new_middle_release_event(position),
                        })
                    },
                    _ => Err("Wrong payload for Action::MiddleMouseRelease"),
                }
            },
            Some(ProtobufActionName::MouseEvent) => match protobuf_action.optional_payload {
                Some(OptionalPayload::MouseEventPayload(payload)) => {
                    let event = payload.try_into()?;
                    Ok(Action::MouseEvent { event })
                },
                _ => Err("Wrong payload for Action::MouseEvent"),
            },
            Some(ProtobufActionName::SearchInput) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SearchInputPayload(payload)) => {
                    Ok(Action::SearchInput { input: payload })
                },
                _ => Err("Wrong payload for Action::SearchInput"),
            },
            Some(ProtobufActionName::Search) => match protobuf_action.optional_payload {
                Some(OptionalPayload::SearchPayload(search_direction)) => Ok(Action::Search {
                    direction: ProtobufSearchDirection::from_i32(search_direction)
                        .ok_or("Malformed payload for Action::Search")?
                        .try_into()?,
                }),
                _ => Err("Wrong payload for Action::Search"),
            },
            Some(ProtobufActionName::SearchToggleOption) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::SearchToggleOptionPayload(search_option)) => {
                        Ok(Action::SearchToggleOption {
                            option: ProtobufSearchOption::from_i32(search_option)
                                .ok_or("Malformed payload for Action::SearchToggleOption")?
                                .try_into()?,
                        })
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
            Some(ProtobufActionName::OverrideLayout) => match protobuf_action.optional_payload {
                Some(OptionalPayload::OverrideLayoutPayload(payload)) => {
                    let tiled_layout = payload.tiled_layout.map(|l| l.try_into()).transpose()?;

                    let floating_layouts = payload
                        .floating_layouts
                        .into_iter()
                        .map(|l| l.try_into())
                        .collect::<Result<Vec<_>, _>>()?;

                    let swap_tiled_layouts = if payload.swap_tiled_layouts.is_empty() {
                        None
                    } else {
                        Some(
                            payload
                                .swap_tiled_layouts
                                .into_iter()
                                .map(|l| l.try_into())
                                .collect::<Result<Vec<_>, _>>()?,
                        )
                    };

                    let swap_floating_layouts = if payload.swap_floating_layouts.is_empty() {
                        None
                    } else {
                        Some(
                            payload
                                .swap_floating_layouts
                                .into_iter()
                                .map(|l| l.try_into())
                                .collect::<Result<Vec<_>, _>>()?,
                        )
                    };

                    let tab_name = payload.tab_name.filter(|s| !s.is_empty());

                    Ok(Action::OverrideLayout {
                        tiled_layout,
                        floating_layouts,
                        swap_tiled_layouts,
                        swap_floating_layouts,
                        tab_name,
                        retain_existing_terminal_panes: payload.retain_existing_terminal_panes,
                        retain_existing_plugin_panes: payload.retain_existing_plugin_panes,
                    })
                },
                Some(_) => Err("Mismatched payload for OverrideLayout"),
                None => Err("Missing payload for OverrideLayout"),
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
                        Ok(Action::NewTiledPluginPane {
                            plugin: run_plugin,
                            pane_name,
                            skip_cache: skip_plugin_cache,
                            cwd: None,
                        })
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
                        Ok(Action::NewFloatingPluginPane {
                            plugin: run_plugin,
                            pane_name,
                            skip_cache: skip_plugin_cache,
                            cwd: None,
                            coordinates: None,
                        })
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

                        Ok(Action::StartOrReloadPlugin {
                            plugin: run_plugin_or_alias,
                        })
                    },
                    _ => Err("Wrong payload for Action::StartOrReloadPlugin"),
                }
            },
            Some(ProtobufActionName::CloseTerminalPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::CloseTerminalPanePayload(payload)) => {
                    Ok(Action::CloseTerminalPane { pane_id: payload })
                },
                _ => Err("Wrong payload for Action::CloseTerminalPane"),
            },
            Some(ProtobufActionName::ClosePluginPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::ClosePluginPanePayload(payload)) => {
                    Ok(Action::ClosePluginPane { pane_id: payload })
                },
                _ => Err("Wrong payload for Action::ClosePluginPane"),
            },
            Some(ProtobufActionName::FocusTerminalPaneWithId) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::FocusTerminalPaneWithIdPayload(payload)) => {
                        let terminal_pane_id = payload.pane_id;
                        let should_float_if_hidden = payload.should_float;
                        let should_be_in_place_if_hidden = payload.should_be_in_place;
                        Ok(Action::FocusTerminalPaneWithId {
                            pane_id: terminal_pane_id,
                            should_float_if_hidden,
                            should_be_in_place_if_hidden,
                        })
                    },
                    _ => Err("Wrong payload for Action::FocusTerminalPaneWithId"),
                }
            },
            Some(ProtobufActionName::FocusPluginPaneWithId) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::FocusPluginPaneWithIdPayload(payload)) => {
                        let plugin_pane_id = payload.pane_id;
                        let should_float_if_hidden = payload.should_float;
                        let should_be_in_place_if_hidden = payload.should_be_in_place;
                        Ok(Action::FocusPluginPaneWithId {
                            pane_id: plugin_pane_id,
                            should_float_if_hidden,
                            should_be_in_place_if_hidden,
                        })
                    },
                    _ => Err("Wrong payload for Action::FocusPluginPaneWithId"),
                }
            },
            Some(ProtobufActionName::RenameTerminalPane) => {
                match protobuf_action.optional_payload {
                    Some(OptionalPayload::RenameTerminalPanePayload(payload)) => {
                        let terminal_pane_id = payload.id;
                        let new_pane_name = payload.name;
                        Ok(Action::RenameTerminalPane {
                            pane_id: terminal_pane_id,
                            name: new_pane_name,
                        })
                    },
                    _ => Err("Wrong payload for Action::RenameTerminalPane"),
                }
            },
            Some(ProtobufActionName::RenamePluginPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RenamePluginPanePayload(payload)) => {
                    let plugin_pane_id = payload.id;
                    let new_pane_name = payload.name;
                    Ok(Action::RenamePluginPane {
                        pane_id: plugin_pane_id,
                        name: new_pane_name,
                    })
                },
                _ => Err("Wrong payload for Action::RenamePluginPane"),
            },
            Some(ProtobufActionName::RenameTab) => match protobuf_action.optional_payload {
                Some(OptionalPayload::RenameTabPayload(payload)) => {
                    let tab_index = payload.id;
                    let new_tab_name = payload.name;
                    Ok(Action::RenameTab {
                        tab_index,
                        name: new_tab_name,
                    })
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
                    Ok(Action::RenameSession { name })
                },
                _ => Err("Wrong payload for Action::RenameSession"),
            },
            Some(ProtobufActionName::TogglePanePinned) => match protobuf_action.optional_payload {
                Some(_) => Err("TogglePanePinned should not have a payload"),
                None => Ok(Action::TogglePanePinned),
            },
            Some(ProtobufActionName::TogglePaneInGroup) => match protobuf_action.optional_payload {
                Some(_) => Err("TogglePaneInGroup should not have a payload"),
                None => Ok(Action::TogglePaneInGroup),
            },
            Some(ProtobufActionName::ToggleGroupMarking) => {
                match protobuf_action.optional_payload {
                    Some(_) => Err("ToggleGroupMarking should not have a payload"),
                    None => Ok(Action::ToggleGroupMarking),
                }
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
                    plugin_id: None,
                }),
            },
            Some(ProtobufActionName::NewStackedPane) => match protobuf_action.optional_payload {
                Some(_) => Err("NewStackedPane should not have a payload"),
                None => Ok(Action::NewStackedPane {
                    command: None,
                    pane_name: None,
                    near_current_pane: false,
                }),
            },
            Some(ProtobufActionName::NewBlockingPane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewBlockingPanePayload(payload)) => {
                    let placement: NewPanePlacement = payload
                        .placement
                        .ok_or("NewBlockingPanePayload must have a placement")?
                        .try_into()?;
                    let pane_name = payload.pane_name;
                    let command = payload.command.and_then(|c| c.try_into().ok());
                    let unblock_condition = payload
                        .unblock_condition
                        .and_then(|uc| ProtobufUnblockCondition::from_i32(uc))
                        .and_then(|uc| uc.try_into().ok());
                    let near_current_pane = payload.near_current_pane;
                    Ok(Action::NewBlockingPane {
                        placement,
                        pane_name,
                        command,
                        unblock_condition,
                        near_current_pane,
                    })
                },
                _ => Err("Wrong payload for Action::NewBlockingPane"),
            },
            Some(ProtobufActionName::NewInPlacePane) => match protobuf_action.optional_payload {
                Some(OptionalPayload::NewInPlacePanePayload(payload)) => {
                    let near_current_pane = payload.near_current_pane;
                    let pane_id_to_replace =
                        payload.pane_id_to_replace.and_then(|p| p.try_into().ok());
                    let close_replace_pane = payload.close_replace_pane;
                    if let Some(command) = payload.command {
                        let pane_name = command.pane_name.clone();
                        let run_command_action: RunCommandAction = command.try_into()?;
                        Ok(Action::NewInPlacePane {
                            command: Some(run_command_action),
                            pane_name,
                            near_current_pane,
                            pane_id_to_replace,
                            close_replace_pane,
                        })
                    } else {
                        Ok(Action::NewInPlacePane {
                            command: None,
                            pane_name: payload.pane_name,
                            near_current_pane,
                            pane_id_to_replace,
                            close_replace_pane,
                        })
                    }
                },
                _ => Err("Wrong payload for Action::NewInPlacePane"),
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
            Action::Write {
                key_with_modifier,
                bytes,
                is_kitty_keyboard_protocol,
            } => {
                let protobuf_key_with_modifier = key_with_modifier.and_then(|k| k.try_into().ok());
                Ok(ProtobufAction {
                    name: ProtobufActionName::Write as i32,
                    optional_payload: Some(OptionalPayload::WritePayload(WritePayload {
                        key_with_modifier: protobuf_key_with_modifier,
                        bytes_to_write: bytes,
                        is_kitty_keyboard_protocol,
                    })),
                })
            },
            Action::WriteChars {
                chars: chars_to_write,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::WriteChars as i32,
                optional_payload: Some(OptionalPayload::WriteCharsPayload(WriteCharsPayload {
                    chars: chars_to_write,
                })),
            }),
            Action::SwitchToMode { input_mode } => {
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
            Action::SwitchModeForAllClients { input_mode } => {
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
            Action::Resize { resize, direction } => {
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
            Action::MoveFocus { direction } => {
                let direction: ProtobufResizeDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveFocus as i32,
                    optional_payload: Some(OptionalPayload::MoveFocusPayload(direction as i32)),
                })
            },
            Action::MoveFocusOrTab { direction } => {
                let direction: ProtobufResizeDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveFocusOrTab as i32,
                    optional_payload: Some(OptionalPayload::MoveFocusOrTabPayload(
                        direction as i32,
                    )),
                })
            },
            Action::MovePane { direction } => {
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
            Action::DumpScreen {
                file_path,
                include_scrollback,
            } => Ok(ProtobufAction {
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
            Action::ScrollUpAt { position } => {
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
            Action::ScrollDownAt { position } => {
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
            Action::NewPane {
                direction,
                pane_name: new_pane_name,
                start_suppressed: _start_suppressed,
            } => {
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
            Action::EditFile {
                payload: open_file_payload,
                direction,
                floating: should_float,
                in_place: _should_be_in_place,
                start_suppressed: _start_suppressed,
                coordinates: _floating_pane_coordinates,
                near_current_pane,
            } => {
                let file_to_edit = open_file_payload.path.display().to_string();
                let cwd = open_file_payload.cwd.map(|cwd| cwd.display().to_string());
                let direction: Option<i32> = direction
                    .and_then(|d| ProtobufResizeDirection::try_from(d).ok())
                    .map(|d| d as i32);
                let line_number = open_file_payload.line_number.map(|l| l as u32);
                Ok(ProtobufAction {
                    name: ProtobufActionName::EditFile as i32,
                    optional_payload: Some(OptionalPayload::EditFilePayload(EditFilePayload {
                        file_to_edit,
                        line_number,
                        should_float,
                        direction,
                        cwd,
                        near_current_pane,
                    })),
                })
            },
            Action::NewFloatingPane {
                command: run_command_action,
                pane_name,
                coordinates: _coordinates,
                near_current_pane,
            } => {
                let command = run_command_action.and_then(|r| {
                    let mut protobuf_run_command_action: ProtobufRunCommandAction =
                        r.try_into().ok()?;
                    protobuf_run_command_action.pane_name = pane_name;
                    Some(protobuf_run_command_action)
                });
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewFloatingPane as i32,
                    optional_payload: Some(OptionalPayload::NewFloatingPanePayload(
                        NewFloatingPanePayload {
                            command,
                            near_current_pane,
                        },
                    )),
                })
            },
            Action::NewTiledPane {
                direction,
                command: run_command_action,
                pane_name,
                near_current_pane,
            } => {
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
                        NewTiledPanePayload {
                            direction,
                            command,
                            near_current_pane,
                        },
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
            Action::PaneNameInput { input: bytes } => Ok(ProtobufAction {
                name: ProtobufActionName::PaneNameInput as i32,
                optional_payload: Some(OptionalPayload::PaneNameInputPayload(bytes)),
            }),
            Action::UndoRenamePane => Ok(ProtobufAction {
                name: ProtobufActionName::UndoRenamePane as i32,
                optional_payload: None,
            }),
            Action::NewTab {
                tiled_layout,
                floating_layouts,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
                should_change_focus_to_new_tab,
                cwd,
                initial_panes,
                first_pane_unblock_condition,
            } => {
                // Always send payload (even if all fields are default)
                let protobuf_tiled_layout = tiled_layout
                    .as_ref()
                    .map(|l| l.clone().try_into())
                    .transpose()?;

                let protobuf_floating_layouts = floating_layouts
                    .iter()
                    .map(|l| l.clone().try_into())
                    .collect::<Result<Vec<_>, _>>()?;

                let protobuf_swap_tiled_layouts = swap_tiled_layouts
                    .as_ref()
                    .map(|layouts| {
                        layouts
                            .iter()
                            .map(|l| l.clone().try_into())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let protobuf_swap_floating_layouts = swap_floating_layouts
                    .as_ref()
                    .map(|layouts| {
                        layouts
                            .iter()
                            .map(|l| l.clone().try_into())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let cwd_string = cwd.as_ref().map(|p| p.display().to_string());

                let protobuf_initial_panes = initial_panes
                    .as_ref()
                    .map(|panes| {
                        panes
                            .iter()
                            .map(|p| p.clone().try_into())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let protobuf_first_pane_unblock_condition = first_pane_unblock_condition
                    .map(|uc| {
                        let protobuf_uc: ProtobufUnblockCondition = uc.try_into().ok()?;
                        Some(protobuf_uc as i32)
                    })
                    .flatten();

                Ok(ProtobufAction {
                    name: ProtobufActionName::NewTab as i32,
                    optional_payload: Some(OptionalPayload::NewTabPayload(NewTabPayload {
                        tiled_layout: protobuf_tiled_layout,
                        floating_layouts: protobuf_floating_layouts,
                        swap_tiled_layouts: protobuf_swap_tiled_layouts,
                        swap_floating_layouts: protobuf_swap_floating_layouts,
                        tab_name: tab_name.clone(),
                        should_change_focus_to_new_tab,
                        cwd: cwd_string,
                        initial_panes: protobuf_initial_panes,
                        first_pane_unblock_condition: protobuf_first_pane_unblock_condition,
                    })),
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
            Action::GoToTab { index: tab_index } => Ok(ProtobufAction {
                name: ProtobufActionName::GoToTab as i32,
                optional_payload: Some(OptionalPayload::GoToTabPayload(tab_index)),
            }),
            Action::GoToTabName {
                name: tab_name,
                create,
            } => Ok(ProtobufAction {
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
            Action::TabNameInput { input: bytes } => Ok(ProtobufAction {
                name: ProtobufActionName::TabNameInput as i32,
                optional_payload: Some(OptionalPayload::TabNameInputPayload(bytes)),
            }),
            Action::UndoRenameTab => Ok(ProtobufAction {
                name: ProtobufActionName::UndoRenameTab as i32,
                optional_payload: None,
            }),
            Action::MoveTab { direction } => {
                let direction: ProtobufMoveTabDirection = direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MoveTab as i32,
                    optional_payload: Some(OptionalPayload::MoveTabPayload(direction as i32)),
                })
            },
            Action::Run {
                command: run_command_action,
                near_current_pane: _,
            } => {
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
            Action::LaunchOrFocusPlugin {
                plugin: run_plugin_or_alias,
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                skip_cache: skip_plugin_cache,
            } => {
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
            Action::LaunchPlugin {
                plugin: run_plugin_or_alias,
                should_float,
                should_open_in_place,
                skip_cache: skip_plugin_cache,
                cwd: _cwd,
            } => {
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
            Action::MouseEvent { event } => {
                let payload: ProtobufMouseEventPayload = event.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::MouseEvent as i32,
                    optional_payload: Some(OptionalPayload::MouseEventPayload(payload)),
                })
            },
            Action::SearchInput { input: bytes } => Ok(ProtobufAction {
                name: ProtobufActionName::SearchInput as i32,
                optional_payload: Some(OptionalPayload::SearchInputPayload(bytes)),
            }),
            Action::Search {
                direction: search_direction,
            } => {
                let search_direction: ProtobufSearchDirection = search_direction.try_into()?;
                Ok(ProtobufAction {
                    name: ProtobufActionName::Search as i32,
                    optional_payload: Some(OptionalPayload::SearchPayload(search_direction as i32)),
                })
            },
            Action::SearchToggleOption {
                option: search_option,
            } => {
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
            Action::OverrideLayout {
                tiled_layout,
                floating_layouts,
                swap_tiled_layouts,
                swap_floating_layouts,
                tab_name,
                retain_existing_terminal_panes,
                retain_existing_plugin_panes,
            } => {
                let protobuf_tiled_layout = tiled_layout.map(|l| l.try_into()).transpose()?;

                let protobuf_floating_layouts = floating_layouts
                    .into_iter()
                    .map(|l| l.try_into())
                    .collect::<Result<Vec<_>, _>>()?;

                let protobuf_swap_tiled_layouts = swap_tiled_layouts
                    .map(|layouts| {
                        layouts
                            .into_iter()
                            .map(|l| l.try_into())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                let protobuf_swap_floating_layouts = swap_floating_layouts
                    .map(|layouts| {
                        layouts
                            .into_iter()
                            .map(|l| l.try_into())
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .transpose()?
                    .unwrap_or_default();

                Ok(ProtobufAction {
                    name: ProtobufActionName::OverrideLayout as i32,
                    optional_payload: Some(OptionalPayload::OverrideLayoutPayload(
                        OverrideLayoutPayload {
                            tiled_layout: protobuf_tiled_layout,
                            floating_layouts: protobuf_floating_layouts,
                            swap_tiled_layouts: protobuf_swap_tiled_layouts,
                            swap_floating_layouts: protobuf_swap_floating_layouts,
                            tab_name: tab_name.clone(),
                            retain_existing_terminal_panes,
                            retain_existing_plugin_panes,
                        },
                    )),
                })
            },
            Action::QueryTabNames => Ok(ProtobufAction {
                name: ProtobufActionName::QueryTabNames as i32,
                optional_payload: None,
            }),
            Action::NewTiledPluginPane {
                plugin: run_plugin,
                pane_name,
                skip_cache: skip_plugin_cache,
                cwd: _cwd,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::NewTiledPluginPane as i32,
                optional_payload: Some(OptionalPayload::NewTiledPluginPanePayload(
                    NewPluginPanePayload {
                        plugin_url: run_plugin.location_string(),
                        pane_name,
                        skip_plugin_cache,
                    },
                )),
            }),
            Action::NewFloatingPluginPane {
                plugin: run_plugin,
                pane_name,
                skip_cache: skip_plugin_cache,
                cwd: _cwd,
                coordinates: _coordinates,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::NewFloatingPluginPane as i32,
                optional_payload: Some(OptionalPayload::NewFloatingPluginPanePayload(
                    NewPluginPanePayload {
                        plugin_url: run_plugin.location_string(),
                        pane_name,
                        skip_plugin_cache,
                    },
                )),
            }),
            Action::StartOrReloadPlugin { plugin: run_plugin } => Ok(ProtobufAction {
                name: ProtobufActionName::StartOrReloadPlugin as i32,
                optional_payload: Some(OptionalPayload::StartOrReloadPluginPayload(
                    run_plugin.location_string(),
                )),
            }),
            Action::CloseTerminalPane {
                pane_id: terminal_pane_id,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::CloseTerminalPane as i32,
                optional_payload: Some(OptionalPayload::CloseTerminalPanePayload(terminal_pane_id)),
            }),
            Action::ClosePluginPane {
                pane_id: plugin_pane_id,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::ClosePluginPane as i32,
                optional_payload: Some(OptionalPayload::ClosePluginPanePayload(plugin_pane_id)),
            }),
            Action::FocusTerminalPaneWithId {
                pane_id: terminal_pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::FocusTerminalPaneWithId as i32,
                optional_payload: Some(OptionalPayload::FocusTerminalPaneWithIdPayload(
                    PaneIdAndShouldFloat {
                        pane_id: terminal_pane_id,
                        should_float: should_float_if_hidden,
                        should_be_in_place: should_be_in_place_if_hidden,
                    },
                )),
            }),
            Action::FocusPluginPaneWithId {
                pane_id: plugin_pane_id,
                should_float_if_hidden,
                should_be_in_place_if_hidden,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::FocusPluginPaneWithId as i32,
                optional_payload: Some(OptionalPayload::FocusPluginPaneWithIdPayload(
                    PaneIdAndShouldFloat {
                        pane_id: plugin_pane_id,
                        should_float: should_float_if_hidden,
                        should_be_in_place: should_be_in_place_if_hidden,
                    },
                )),
            }),
            Action::RenameTerminalPane {
                pane_id: terminal_pane_id,
                name: new_name,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::RenameTerminalPane as i32,
                optional_payload: Some(OptionalPayload::RenameTerminalPanePayload(IdAndName {
                    name: new_name,
                    id: terminal_pane_id,
                })),
            }),
            Action::RenamePluginPane {
                pane_id: plugin_pane_id,
                name: new_name,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::RenamePluginPane as i32,
                optional_payload: Some(OptionalPayload::RenamePluginPanePayload(IdAndName {
                    name: new_name,
                    id: plugin_pane_id,
                })),
            }),
            Action::RenameTab {
                tab_index,
                name: new_name,
            } => Ok(ProtobufAction {
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
            Action::RenameSession { name: session_name } => Ok(ProtobufAction {
                name: ProtobufActionName::RenameSession as i32,
                optional_payload: Some(OptionalPayload::RenameSessionPayload(session_name)),
            }),
            Action::KeybindPipe { .. } => Ok(ProtobufAction {
                name: ProtobufActionName::KeybindPipe as i32,
                optional_payload: None,
            }),
            Action::TogglePanePinned { .. } => Ok(ProtobufAction {
                name: ProtobufActionName::TogglePanePinned as i32,
                optional_payload: None,
            }),
            Action::TogglePaneInGroup { .. } => Ok(ProtobufAction {
                name: ProtobufActionName::TogglePaneInGroup as i32,
                optional_payload: None,
            }),
            Action::ToggleGroupMarking { .. } => Ok(ProtobufAction {
                name: ProtobufActionName::ToggleGroupMarking as i32,
                optional_payload: None,
            }),
            Action::NewStackedPane {
                command: _,
                pane_name: _,
                near_current_pane: _,
            } => Ok(ProtobufAction {
                name: ProtobufActionName::NewStackedPane as i32,
                optional_payload: None,
            }),
            Action::NewBlockingPane {
                placement,
                pane_name,
                command,
                unblock_condition,
                near_current_pane,
            } => {
                let placement: ProtobufNewPanePlacement = placement.try_into()?;
                let command = command.and_then(|c| {
                    let protobuf_command: ProtobufRunCommandAction = c.try_into().ok()?;
                    Some(protobuf_command)
                });
                let unblock_condition = unblock_condition
                    .map(|uc| {
                        let protobuf_uc: ProtobufUnblockCondition = uc.try_into().ok()?;
                        Some(protobuf_uc as i32)
                    })
                    .flatten();
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewBlockingPane as i32,
                    optional_payload: Some(OptionalPayload::NewBlockingPanePayload(
                        NewBlockingPanePayload {
                            placement: Some(placement),
                            pane_name,
                            command,
                            unblock_condition,
                            near_current_pane,
                        },
                    )),
                })
            },
            Action::NewInPlacePane {
                command: run_command_action,
                pane_name,
                near_current_pane,
                pane_id_to_replace,
                close_replace_pane,
            } => {
                let command = run_command_action.and_then(|r| {
                    let mut protobuf_run_command_action: ProtobufRunCommandAction =
                        r.try_into().ok()?;
                    let pane_name = pane_name.and_then(|n| n.try_into().ok());
                    protobuf_run_command_action.pane_name = pane_name;
                    Some(protobuf_run_command_action)
                });
                let pane_id_to_replace = pane_id_to_replace.and_then(|p| p.try_into().ok());
                Ok(ProtobufAction {
                    name: ProtobufActionName::NewInPlacePane as i32,
                    optional_payload: Some(OptionalPayload::NewInPlacePanePayload(
                        NewInPlacePanePayload {
                            command,
                            pane_name: None, // pane_name is already embedded in command
                            near_current_pane,
                            pane_id_to_replace,
                            close_replace_pane,
                        },
                    )),
                })
            },
            Action::NoOp
            | Action::Confirm
            | Action::NewInPlacePluginPane {
                plugin: _,
                pane_name: _,
                skip_cache: _,
            }
            | Action::Deny
            | Action::Copy
            | Action::DumpLayout
            | Action::CliPipe { .. }
            | Action::ListClients
            | Action::StackPanes { pane_ids: _ }
            | Action::ChangeFloatingPaneCoordinates {
                pane_id: _,
                coordinates: _,
            }
            | Action::SkipConfirm { action: _ }
            | Action::SwitchSession { .. } => Err("Unsupported action"),
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
            ..Default::default()
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

impl TryFrom<ProtobufMouseEventPayload> for MouseEvent {
    type Error = &'static str;
    fn try_from(protobuf_event: ProtobufMouseEventPayload) -> Result<Self, &'static str> {
        Ok(MouseEvent {
            event_type: match protobuf_event.event_type as u32 {
                0 => MouseEventType::Press,
                1 => MouseEventType::Release,
                _ => MouseEventType::Motion,
            },
            left: protobuf_event.left as bool,
            right: protobuf_event.right as bool,
            middle: protobuf_event.middle as bool,
            wheel_up: protobuf_event.wheel_up as bool,
            wheel_down: protobuf_event.wheel_down as bool,
            shift: protobuf_event.shift as bool,
            alt: protobuf_event.alt as bool,
            ctrl: protobuf_event.ctrl as bool,
            position: Position::new(protobuf_event.line as i32, protobuf_event.column as u16),
        })
    }
}

impl TryFrom<MouseEvent> for ProtobufMouseEventPayload {
    type Error = &'static str;
    fn try_from(event: MouseEvent) -> Result<Self, &'static str> {
        Ok(ProtobufMouseEventPayload {
            event_type: match event.event_type {
                MouseEventType::Press => 0,
                MouseEventType::Release => 1,
                MouseEventType::Motion => 2,
            } as u32,
            left: event.left as bool,
            right: event.right as bool,
            middle: event.middle as bool,
            wheel_up: event.wheel_up as bool,
            wheel_down: event.wheel_down as bool,
            shift: event.shift as bool,
            alt: event.alt as bool,
            ctrl: event.ctrl as bool,
            line: event.position.line.0 as i64,
            column: event.position.column.0 as i64,
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

impl TryFrom<ProtobufKeyWithModifier> for KeyWithModifier {
    type Error = &'static str;
    fn try_from(protobuf_key: ProtobufKeyWithModifier) -> Result<Self, &'static str> {
        let bare_key = match ProtobufBareKey::from_i32(protobuf_key.bare_key) {
            Some(ProtobufBareKey::PageDown) => crate::data::BareKey::PageDown,
            Some(ProtobufBareKey::PageUp) => crate::data::BareKey::PageUp,
            Some(ProtobufBareKey::Left) => crate::data::BareKey::Left,
            Some(ProtobufBareKey::Down) => crate::data::BareKey::Down,
            Some(ProtobufBareKey::Up) => crate::data::BareKey::Up,
            Some(ProtobufBareKey::Right) => crate::data::BareKey::Right,
            Some(ProtobufBareKey::Home) => crate::data::BareKey::Home,
            Some(ProtobufBareKey::End) => crate::data::BareKey::End,
            Some(ProtobufBareKey::Backspace) => crate::data::BareKey::Backspace,
            Some(ProtobufBareKey::Delete) => crate::data::BareKey::Delete,
            Some(ProtobufBareKey::Insert) => crate::data::BareKey::Insert,
            Some(ProtobufBareKey::F1) => crate::data::BareKey::F(1),
            Some(ProtobufBareKey::F2) => crate::data::BareKey::F(2),
            Some(ProtobufBareKey::F3) => crate::data::BareKey::F(3),
            Some(ProtobufBareKey::F4) => crate::data::BareKey::F(4),
            Some(ProtobufBareKey::F5) => crate::data::BareKey::F(5),
            Some(ProtobufBareKey::F6) => crate::data::BareKey::F(6),
            Some(ProtobufBareKey::F7) => crate::data::BareKey::F(7),
            Some(ProtobufBareKey::F8) => crate::data::BareKey::F(8),
            Some(ProtobufBareKey::F9) => crate::data::BareKey::F(9),
            Some(ProtobufBareKey::F10) => crate::data::BareKey::F(10),
            Some(ProtobufBareKey::F11) => crate::data::BareKey::F(11),
            Some(ProtobufBareKey::F12) => crate::data::BareKey::F(12),
            Some(ProtobufBareKey::Char) => {
                if let Some(character) = protobuf_key.character {
                    let ch = character
                        .chars()
                        .next()
                        .ok_or("BareKey::Char requires a character")?;
                    crate::data::BareKey::Char(ch)
                } else {
                    return Err("BareKey::Char requires a character");
                }
            },
            Some(ProtobufBareKey::Tab) => crate::data::BareKey::Tab,
            Some(ProtobufBareKey::Esc) => crate::data::BareKey::Esc,
            Some(ProtobufBareKey::Enter) => crate::data::BareKey::Enter,
            Some(ProtobufBareKey::CapsLock) => crate::data::BareKey::CapsLock,
            Some(ProtobufBareKey::ScrollLock) => crate::data::BareKey::ScrollLock,
            Some(ProtobufBareKey::NumLock) => crate::data::BareKey::NumLock,
            Some(ProtobufBareKey::PrintScreen) => crate::data::BareKey::PrintScreen,
            Some(ProtobufBareKey::Pause) => crate::data::BareKey::Pause,
            Some(ProtobufBareKey::Menu) => crate::data::BareKey::Menu,
            _ => return Err("Unknown BareKey"),
        };

        let mut key_modifiers = std::collections::BTreeSet::new();
        for modifier in protobuf_key.key_modifiers {
            let key_modifier = match ProtobufKeyModifier::from_i32(modifier) {
                Some(ProtobufKeyModifier::Ctrl) => crate::data::KeyModifier::Ctrl,
                Some(ProtobufKeyModifier::Alt) => crate::data::KeyModifier::Alt,
                Some(ProtobufKeyModifier::Shift) => crate::data::KeyModifier::Shift,
                Some(ProtobufKeyModifier::Super) => crate::data::KeyModifier::Super,
                _ => continue,
            };
            key_modifiers.insert(key_modifier);
        }

        Ok(KeyWithModifier {
            bare_key,
            key_modifiers,
        })
    }
}

impl TryFrom<KeyWithModifier> for ProtobufKeyWithModifier {
    type Error = &'static str;
    fn try_from(key: KeyWithModifier) -> Result<Self, &'static str> {
        let (bare_key, character) = match key.bare_key {
            crate::data::BareKey::PageDown => (ProtobufBareKey::PageDown as i32, None),
            crate::data::BareKey::PageUp => (ProtobufBareKey::PageUp as i32, None),
            crate::data::BareKey::Left => (ProtobufBareKey::Left as i32, None),
            crate::data::BareKey::Down => (ProtobufBareKey::Down as i32, None),
            crate::data::BareKey::Up => (ProtobufBareKey::Up as i32, None),
            crate::data::BareKey::Right => (ProtobufBareKey::Right as i32, None),
            crate::data::BareKey::Home => (ProtobufBareKey::Home as i32, None),
            crate::data::BareKey::End => (ProtobufBareKey::End as i32, None),
            crate::data::BareKey::Backspace => (ProtobufBareKey::Backspace as i32, None),
            crate::data::BareKey::Delete => (ProtobufBareKey::Delete as i32, None),
            crate::data::BareKey::Insert => (ProtobufBareKey::Insert as i32, None),
            crate::data::BareKey::F(1) => (ProtobufBareKey::F1 as i32, None),
            crate::data::BareKey::F(2) => (ProtobufBareKey::F2 as i32, None),
            crate::data::BareKey::F(3) => (ProtobufBareKey::F3 as i32, None),
            crate::data::BareKey::F(4) => (ProtobufBareKey::F4 as i32, None),
            crate::data::BareKey::F(5) => (ProtobufBareKey::F5 as i32, None),
            crate::data::BareKey::F(6) => (ProtobufBareKey::F6 as i32, None),
            crate::data::BareKey::F(7) => (ProtobufBareKey::F7 as i32, None),
            crate::data::BareKey::F(8) => (ProtobufBareKey::F8 as i32, None),
            crate::data::BareKey::F(9) => (ProtobufBareKey::F9 as i32, None),
            crate::data::BareKey::F(10) => (ProtobufBareKey::F10 as i32, None),
            crate::data::BareKey::F(11) => (ProtobufBareKey::F11 as i32, None),
            crate::data::BareKey::F(12) => (ProtobufBareKey::F12 as i32, None),
            crate::data::BareKey::Char(c) => (ProtobufBareKey::Char as i32, Some(c.to_string())),
            crate::data::BareKey::Tab => (ProtobufBareKey::Tab as i32, None),
            crate::data::BareKey::Esc => (ProtobufBareKey::Esc as i32, None),
            crate::data::BareKey::Enter => (ProtobufBareKey::Enter as i32, None),
            crate::data::BareKey::CapsLock => (ProtobufBareKey::CapsLock as i32, None),
            crate::data::BareKey::ScrollLock => (ProtobufBareKey::ScrollLock as i32, None),
            crate::data::BareKey::NumLock => (ProtobufBareKey::NumLock as i32, None),
            crate::data::BareKey::PrintScreen => (ProtobufBareKey::PrintScreen as i32, None),
            crate::data::BareKey::Pause => (ProtobufBareKey::Pause as i32, None),
            crate::data::BareKey::Menu => (ProtobufBareKey::Menu as i32, None),
            _ => return Err("Unsupported BareKey"),
        };

        let key_modifiers: Vec<i32> = key
            .key_modifiers
            .iter()
            .map(|m| match m {
                crate::data::KeyModifier::Ctrl => ProtobufKeyModifier::Ctrl as i32,
                crate::data::KeyModifier::Alt => ProtobufKeyModifier::Alt as i32,
                crate::data::KeyModifier::Shift => ProtobufKeyModifier::Shift as i32,
                crate::data::KeyModifier::Super => ProtobufKeyModifier::Super as i32,
            })
            .collect();

        Ok(ProtobufKeyWithModifier {
            bare_key,
            key_modifiers,
            character,
        })
    }
}

// UnblockCondition conversions
impl TryFrom<ProtobufUnblockCondition> for UnblockCondition {
    type Error = &'static str;
    fn try_from(protobuf_uc: ProtobufUnblockCondition) -> Result<Self, &'static str> {
        match protobuf_uc {
            ProtobufUnblockCondition::UnblockOnExitSuccess => Ok(UnblockCondition::OnExitSuccess),
            ProtobufUnblockCondition::UnblockOnExitFailure => Ok(UnblockCondition::OnExitFailure),
            ProtobufUnblockCondition::UnblockOnAnyExit => Ok(UnblockCondition::OnAnyExit),
        }
    }
}

impl TryFrom<UnblockCondition> for ProtobufUnblockCondition {
    type Error = &'static str;
    fn try_from(uc: UnblockCondition) -> Result<Self, &'static str> {
        match uc {
            UnblockCondition::OnExitSuccess => Ok(ProtobufUnblockCondition::UnblockOnExitSuccess),
            UnblockCondition::OnExitFailure => Ok(ProtobufUnblockCondition::UnblockOnExitFailure),
            UnblockCondition::OnAnyExit => Ok(ProtobufUnblockCondition::UnblockOnAnyExit),
        }
    }
}

impl TryFrom<i32> for UnblockCondition {
    type Error = &'static str;
    fn try_from(value: i32) -> Result<Self, &'static str> {
        match ProtobufUnblockCondition::from_i32(value) {
            Some(uc) => uc.try_into(),
            None => Err("Invalid UnblockCondition value"),
        }
    }
}

// PaneId conversions
impl TryFrom<ProtobufPaneId> for PaneId {
    type Error = &'static str;
    fn try_from(protobuf_pane_id: ProtobufPaneId) -> Result<Self, &'static str> {
        use super::generated_api::api::action::pane_id::PaneIdVariant;

        match protobuf_pane_id.pane_id_variant {
            Some(PaneIdVariant::Terminal(id)) => Ok(PaneId::Terminal(id)),
            Some(PaneIdVariant::Plugin(id)) => Ok(PaneId::Plugin(id)),
            None => Err("PaneId must have either terminal or plugin id"),
        }
    }
}

impl TryFrom<PaneId> for ProtobufPaneId {
    type Error = &'static str;
    fn try_from(pane_id: PaneId) -> Result<Self, &'static str> {
        use super::generated_api::api::action::pane_id::PaneIdVariant;

        let pane_id_variant = match pane_id {
            PaneId::Terminal(id) => Some(PaneIdVariant::Terminal(id)),
            PaneId::Plugin(id) => Some(PaneIdVariant::Plugin(id)),
        };
        Ok(ProtobufPaneId { pane_id_variant })
    }
}

// SplitSize conversions
impl TryFrom<ProtobufSplitSize> for SplitSize {
    type Error = &'static str;
    fn try_from(protobuf_split_size: ProtobufSplitSize) -> Result<Self, &'static str> {
        use super::generated_api::api::action::split_size::SplitSizeVariant;

        match protobuf_split_size.split_size_variant {
            Some(SplitSizeVariant::Percent(p)) => Ok(SplitSize::Percent(p as usize)),
            Some(SplitSizeVariant::Fixed(f)) => Ok(SplitSize::Fixed(f as usize)),
            None => Err("SplitSize must have either percent or fixed value"),
        }
    }
}

impl TryFrom<SplitSize> for ProtobufSplitSize {
    type Error = &'static str;
    fn try_from(split_size: SplitSize) -> Result<Self, &'static str> {
        use super::generated_api::api::action::split_size::SplitSizeVariant;

        let split_size_variant = match split_size {
            SplitSize::Percent(p) => Some(SplitSizeVariant::Percent(p as u32)),
            SplitSize::Fixed(f) => Some(SplitSizeVariant::Fixed(f as u32)),
        };
        Ok(ProtobufSplitSize { split_size_variant })
    }
}

// FloatingPaneCoordinates conversions
impl TryFrom<ProtobufFloatingPaneCoordinates> for FloatingPaneCoordinates {
    type Error = &'static str;
    fn try_from(protobuf_coords: ProtobufFloatingPaneCoordinates) -> Result<Self, &'static str> {
        Ok(FloatingPaneCoordinates {
            x: protobuf_coords.x.and_then(|x| x.try_into().ok()),
            y: protobuf_coords.y.and_then(|y| y.try_into().ok()),
            width: protobuf_coords.width.and_then(|w| w.try_into().ok()),
            height: protobuf_coords.height.and_then(|h| h.try_into().ok()),
            pinned: protobuf_coords.pinned,
        })
    }
}

impl TryFrom<FloatingPaneCoordinates> for ProtobufFloatingPaneCoordinates {
    type Error = &'static str;
    fn try_from(coords: FloatingPaneCoordinates) -> Result<Self, &'static str> {
        Ok(ProtobufFloatingPaneCoordinates {
            x: coords.x.and_then(|x| x.try_into().ok()),
            y: coords.y.and_then(|y| y.try_into().ok()),
            width: coords.width.and_then(|w| w.try_into().ok()),
            height: coords.height.and_then(|h| h.try_into().ok()),
            pinned: coords.pinned,
        })
    }
}

// NewPanePlacement conversions
impl TryFrom<ProtobufNewPanePlacement> for NewPanePlacement {
    type Error = &'static str;
    fn try_from(protobuf_placement: ProtobufNewPanePlacement) -> Result<Self, &'static str> {
        use super::generated_api::api::action::new_pane_placement::PlacementVariant;

        match protobuf_placement.placement_variant {
            Some(PlacementVariant::NoPreference(_)) => Ok(NewPanePlacement::NoPreference),
            Some(PlacementVariant::Tiled(tiled)) => {
                let direction = tiled
                    .direction
                    .and_then(|d| ProtobufResizeDirection::from_i32(d))
                    .and_then(|d| d.try_into().ok());
                Ok(NewPanePlacement::Tiled(direction))
            },
            Some(PlacementVariant::Floating(floating)) => {
                let coords = floating.coordinates.and_then(|c| c.try_into().ok());
                Ok(NewPanePlacement::Floating(coords))
            },
            Some(PlacementVariant::InPlace(config)) => {
                let pane_id_to_replace =
                    config.pane_id_to_replace.and_then(|id| id.try_into().ok());
                Ok(NewPanePlacement::InPlace {
                    pane_id_to_replace,
                    close_replaced_pane: config.close_replaced_pane,
                })
            },
            Some(PlacementVariant::Stacked(stacked)) => {
                let pane_id = stacked.pane_id.and_then(|id| id.try_into().ok());
                Ok(NewPanePlacement::Stacked(pane_id))
            },
            None => Err("NewPanePlacement must have a placement variant"),
        }
    }
}

impl TryFrom<NewPanePlacement> for ProtobufNewPanePlacement {
    type Error = &'static str;
    fn try_from(placement: NewPanePlacement) -> Result<Self, &'static str> {
        use super::generated_api::api::action::new_pane_placement::PlacementVariant;

        let placement_variant = match placement {
            NewPanePlacement::NoPreference => Some(PlacementVariant::NoPreference(true)),
            NewPanePlacement::Tiled(direction) => {
                let direction = direction.and_then(|d| {
                    let protobuf_direction: ProtobufResizeDirection = d.try_into().ok()?;
                    Some(protobuf_direction as i32)
                });
                Some(PlacementVariant::Tiled(ProtobufTiledPlacement {
                    direction,
                }))
            },
            NewPanePlacement::Floating(coords) => {
                let coordinates = coords.and_then(|c| c.try_into().ok());
                Some(PlacementVariant::Floating(ProtobufFloatingPlacement {
                    coordinates,
                }))
            },
            NewPanePlacement::InPlace {
                pane_id_to_replace,
                close_replaced_pane,
            } => {
                let pane_id_to_replace = pane_id_to_replace.and_then(|id| id.try_into().ok());
                Some(PlacementVariant::InPlace(ProtobufInPlaceConfig {
                    pane_id_to_replace,
                    close_replaced_pane,
                }))
            },
            NewPanePlacement::Stacked(pane_id) => {
                let pane_id = pane_id.and_then(|id| id.try_into().ok());
                Some(PlacementVariant::Stacked(ProtobufStackedPlacement {
                    pane_id,
                }))
            },
        };

        Ok(ProtobufNewPanePlacement { placement_variant })
    }
}

// Layout type conversions

impl TryFrom<ProtobufPercentOrFixed> for PercentOrFixed {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufPercentOrFixed) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::percent_or_fixed::SizeType;
        match protobuf.size_type {
            Some(SizeType::Percent(p)) => Ok(PercentOrFixed::Percent(p as usize)),
            Some(SizeType::Fixed(f)) => Ok(PercentOrFixed::Fixed(f as usize)),
            None => Err("PercentOrFixed must have a size_type"),
        }
    }
}

impl TryFrom<PercentOrFixed> for ProtobufPercentOrFixed {
    type Error = &'static str;
    fn try_from(internal: PercentOrFixed) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::percent_or_fixed::SizeType;
        let size_type = match internal {
            PercentOrFixed::Percent(p) => Some(SizeType::Percent(p as u32)),
            PercentOrFixed::Fixed(f) => Some(SizeType::Fixed(f as u32)),
        };
        Ok(ProtobufPercentOrFixed { size_type })
    }
}

impl TryFrom<ProtobufSplitDirection> for SplitDirection {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufSplitDirection) -> Result<Self, Self::Error> {
        match protobuf {
            ProtobufSplitDirection::Horizontal => Ok(SplitDirection::Horizontal),
            ProtobufSplitDirection::Vertical => Ok(SplitDirection::Vertical),
            ProtobufSplitDirection::Unspecified => Err("SplitDirection cannot be unspecified"),
        }
    }
}

impl TryFrom<SplitDirection> for ProtobufSplitDirection {
    type Error = &'static str;
    fn try_from(internal: SplitDirection) -> Result<Self, Self::Error> {
        Ok(match internal {
            SplitDirection::Horizontal => ProtobufSplitDirection::Horizontal,
            SplitDirection::Vertical => ProtobufSplitDirection::Vertical,
        })
    }
}

impl TryFrom<ProtobufLayoutConstraint> for LayoutConstraint {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufLayoutConstraint) -> Result<Self, Self::Error> {
        match protobuf {
            ProtobufLayoutConstraint::MaxPanes => Ok(LayoutConstraint::MaxPanes(0)),
            ProtobufLayoutConstraint::MinPanes => Ok(LayoutConstraint::MinPanes(0)),
            ProtobufLayoutConstraint::ExactPanes => Ok(LayoutConstraint::ExactPanes(0)),
            ProtobufLayoutConstraint::NoConstraint => Ok(LayoutConstraint::NoConstraint),
            ProtobufLayoutConstraint::Unspecified => Err("LayoutConstraint cannot be unspecified"),
        }
    }
}

impl TryFrom<LayoutConstraint> for ProtobufLayoutConstraint {
    type Error = &'static str;
    fn try_from(internal: LayoutConstraint) -> Result<Self, Self::Error> {
        Ok(match internal {
            LayoutConstraint::MaxPanes(_) => ProtobufLayoutConstraint::MaxPanes,
            LayoutConstraint::MinPanes(_) => ProtobufLayoutConstraint::MinPanes,
            LayoutConstraint::ExactPanes(_) => ProtobufLayoutConstraint::ExactPanes,
            LayoutConstraint::NoConstraint => ProtobufLayoutConstraint::NoConstraint,
        })
    }
}

impl TryFrom<ProtobufLayoutConstraintWithValue> for LayoutConstraint {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufLayoutConstraintWithValue) -> Result<Self, Self::Error> {
        let constraint_type = ProtobufLayoutConstraint::from_i32(protobuf.constraint_type)
            .ok_or("Invalid constraint type")?;
        match constraint_type {
            ProtobufLayoutConstraint::MaxPanes => Ok(LayoutConstraint::MaxPanes(
                protobuf.value.unwrap_or(0) as usize,
            )),
            ProtobufLayoutConstraint::MinPanes => Ok(LayoutConstraint::MinPanes(
                protobuf.value.unwrap_or(0) as usize,
            )),
            ProtobufLayoutConstraint::ExactPanes => Ok(LayoutConstraint::ExactPanes(
                protobuf.value.unwrap_or(0) as usize,
            )),
            ProtobufLayoutConstraint::NoConstraint => Ok(LayoutConstraint::NoConstraint),
            ProtobufLayoutConstraint::Unspecified => Err("LayoutConstraint cannot be unspecified"),
        }
    }
}

impl TryFrom<LayoutConstraint> for ProtobufLayoutConstraintWithValue {
    type Error = &'static str;
    fn try_from(internal: LayoutConstraint) -> Result<Self, Self::Error> {
        let (constraint_type, value) = match internal {
            LayoutConstraint::MaxPanes(v) => {
                (ProtobufLayoutConstraint::MaxPanes as i32, Some(v as u32))
            },
            LayoutConstraint::MinPanes(v) => {
                (ProtobufLayoutConstraint::MinPanes as i32, Some(v as u32))
            },
            LayoutConstraint::ExactPanes(v) => {
                (ProtobufLayoutConstraint::ExactPanes as i32, Some(v as u32))
            },
            LayoutConstraint::NoConstraint => (ProtobufLayoutConstraint::NoConstraint as i32, None),
        };
        Ok(ProtobufLayoutConstraintWithValue {
            constraint_type,
            value,
        })
    }
}

impl TryFrom<ProtobufPluginTag> for PluginTag {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufPluginTag) -> Result<Self, Self::Error> {
        Ok(PluginTag::new(protobuf.tag))
    }
}

impl TryFrom<PluginTag> for ProtobufPluginTag {
    type Error = &'static str;
    fn try_from(internal: PluginTag) -> Result<Self, Self::Error> {
        Ok(ProtobufPluginTag {
            tag: internal.into(),
        })
    }
}

impl TryFrom<ProtobufPluginUserConfiguration> for PluginUserConfiguration {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufPluginUserConfiguration) -> Result<Self, Self::Error> {
        let btree_map: BTreeMap<String, String> = protobuf.configuration.into_iter().collect();
        Ok(PluginUserConfiguration::new(btree_map))
    }
}

impl TryFrom<PluginUserConfiguration> for ProtobufPluginUserConfiguration {
    type Error = &'static str;
    fn try_from(internal: PluginUserConfiguration) -> Result<Self, Self::Error> {
        let configuration = internal.inner().clone().into_iter().collect();
        Ok(ProtobufPluginUserConfiguration { configuration })
    }
}

impl TryFrom<ProtobufRunPluginLocationData> for RunPluginLocation {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufRunPluginLocationData) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::run_plugin_location_data::LocationData;
        match protobuf.location_data {
            Some(LocationData::FilePath(path)) => Ok(RunPluginLocation::File(PathBuf::from(path))),
            Some(LocationData::ZellijTag(tag)) => Ok(RunPluginLocation::Zellij(tag.try_into()?)),
            Some(LocationData::RemoteUrl(url)) => Ok(RunPluginLocation::Remote(url)),
            None => Err("RunPluginLocationData must have location_data"),
        }
    }
}

impl TryFrom<RunPluginLocation> for ProtobufRunPluginLocationData {
    type Error = &'static str;
    fn try_from(internal: RunPluginLocation) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::{
            run_plugin_location_data::LocationData,
            RunPluginLocation as ProtobufRunPluginLocationType,
        };
        let (location_type, location_data) = match internal {
            RunPluginLocation::File(path) => (
                ProtobufRunPluginLocationType::File as i32,
                Some(LocationData::FilePath(path.display().to_string())),
            ),
            RunPluginLocation::Zellij(tag) => (
                ProtobufRunPluginLocationType::Zellij as i32,
                Some(LocationData::ZellijTag(tag.try_into()?)),
            ),
            RunPluginLocation::Remote(url) => (
                ProtobufRunPluginLocationType::Remote as i32,
                Some(LocationData::RemoteUrl(url)),
            ),
        };
        Ok(ProtobufRunPluginLocationData {
            location_type,
            location_data,
        })
    }
}

impl TryFrom<ProtobufRunPlugin> for RunPlugin {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufRunPlugin) -> Result<Self, Self::Error> {
        let location = protobuf
            .location
            .ok_or("RunPlugin must have location")?
            .try_into()?;
        let configuration = protobuf
            .configuration
            .ok_or("RunPlugin must have configuration")?
            .try_into()?;
        let initial_cwd = protobuf.initial_cwd.map(PathBuf::from);
        Ok(RunPlugin {
            _allow_exec_host_cmd: protobuf.allow_exec_host_cmd,
            location,
            configuration,
            initial_cwd,
        })
    }
}

impl TryFrom<RunPlugin> for ProtobufRunPlugin {
    type Error = &'static str;
    fn try_from(internal: RunPlugin) -> Result<Self, Self::Error> {
        Ok(ProtobufRunPlugin {
            allow_exec_host_cmd: internal._allow_exec_host_cmd,
            location: Some(internal.location.try_into()?),
            configuration: Some(internal.configuration.try_into()?),
            initial_cwd: internal.initial_cwd.map(|p| p.display().to_string()),
        })
    }
}

impl TryFrom<ProtobufPluginAlias> for PluginAlias {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufPluginAlias) -> Result<Self, Self::Error> {
        let configuration = protobuf.configuration.map(|c| c.try_into()).transpose()?;
        let initial_cwd = protobuf.initial_cwd.map(PathBuf::from);
        let run_plugin = protobuf.run_plugin.map(|r| r.try_into()).transpose()?;
        Ok(PluginAlias {
            name: protobuf.name,
            configuration,
            initial_cwd,
            run_plugin,
        })
    }
}

impl TryFrom<PluginAlias> for ProtobufPluginAlias {
    type Error = &'static str;
    fn try_from(internal: PluginAlias) -> Result<Self, Self::Error> {
        Ok(ProtobufPluginAlias {
            name: internal.name,
            configuration: internal.configuration.map(|c| c.try_into()).transpose()?,
            initial_cwd: internal.initial_cwd.map(|p| p.display().to_string()),
            run_plugin: internal.run_plugin.map(|r| r.try_into()).transpose()?,
        })
    }
}

impl TryFrom<ProtobufRunPluginOrAlias> for RunPluginOrAlias {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufRunPluginOrAlias) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::run_plugin_or_alias::PluginType;
        match protobuf.plugin_type {
            Some(PluginType::Plugin(plugin)) => Ok(RunPluginOrAlias::RunPlugin(plugin.try_into()?)),
            Some(PluginType::Alias(alias)) => Ok(RunPluginOrAlias::Alias(alias.try_into()?)),
            None => Err("RunPluginOrAlias must have plugin_type"),
        }
    }
}

impl TryFrom<RunPluginOrAlias> for ProtobufRunPluginOrAlias {
    type Error = &'static str;
    fn try_from(internal: RunPluginOrAlias) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::run_plugin_or_alias::PluginType;
        let plugin_type = match internal {
            RunPluginOrAlias::RunPlugin(plugin) => Some(PluginType::Plugin(plugin.try_into()?)),
            RunPluginOrAlias::Alias(alias) => Some(PluginType::Alias(alias.try_into()?)),
        };
        Ok(ProtobufRunPluginOrAlias { plugin_type })
    }
}

impl TryFrom<ProtobufRunEditFileAction> for Run {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufRunEditFileAction) -> Result<Self, Self::Error> {
        let file_path = PathBuf::from(protobuf.file_path);
        let line_number = protobuf.line_number.map(|n| n as usize);
        let cwd = protobuf.cwd.map(PathBuf::from);
        Ok(Run::EditFile(file_path, line_number, cwd))
    }
}

impl TryFrom<ProtobufPaneRun> for Run {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufPaneRun) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::pane_run::RunType;
        use crate::input::command::RunCommand;
        match protobuf.run_type {
            Some(RunType::Command(cmd)) => {
                let run_command_action: RunCommandAction = cmd.try_into()?;
                let run_command: RunCommand = run_command_action.into();
                Ok(Run::Command(run_command))
            },
            Some(RunType::Plugin(plugin)) => Ok(Run::Plugin(plugin.try_into()?)),
            Some(RunType::EditFile(edit)) => edit.try_into(),
            Some(RunType::Cwd(cwd)) => Ok(Run::Cwd(PathBuf::from(cwd))),
            None => Err("PaneRun must have run_type"),
        }
    }
}

impl TryFrom<Run> for ProtobufPaneRun {
    type Error = &'static str;
    fn try_from(internal: Run) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::pane_run::RunType;
        let run_type = match internal {
            Run::Command(cmd) => {
                let run_command_action: RunCommandAction = cmd.into();
                Some(RunType::Command(run_command_action.try_into()?))
            },
            Run::Plugin(plugin) => Some(RunType::Plugin(plugin.try_into()?)),
            Run::EditFile(file_path, line_number, cwd) => {
                Some(RunType::EditFile(ProtobufRunEditFileAction {
                    file_path: file_path.display().to_string(),
                    line_number: line_number.map(|n| n as u32),
                    cwd: cwd.map(|p| p.display().to_string()),
                }))
            },
            Run::Cwd(cwd) => Some(RunType::Cwd(cwd.display().to_string())),
        };
        Ok(ProtobufPaneRun { run_type })
    }
}

impl TryFrom<ProtobufCommandOrPlugin> for CommandOrPlugin {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufCommandOrPlugin) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::command_or_plugin::CommandOrPluginType;
        match protobuf.command_or_plugin_type {
            Some(CommandOrPluginType::Command(cmd)) => {
                Ok(CommandOrPlugin::Command(cmd.try_into()?))
            },
            Some(CommandOrPluginType::Plugin(plugin)) => {
                Ok(CommandOrPlugin::Plugin(plugin.try_into()?))
            },
            None => Err("CommandOrPlugin must have command_or_plugin_type"),
        }
    }
}

impl TryFrom<CommandOrPlugin> for ProtobufCommandOrPlugin {
    type Error = &'static str;
    fn try_from(internal: CommandOrPlugin) -> Result<Self, Self::Error> {
        use super::generated_api::api::action::command_or_plugin::CommandOrPluginType;
        let command_or_plugin_type = match internal {
            CommandOrPlugin::Command(cmd) => Some(CommandOrPluginType::Command(cmd.try_into()?)),
            CommandOrPlugin::Plugin(plugin) => {
                Some(CommandOrPluginType::Plugin(plugin.try_into()?))
            },
        };
        Ok(ProtobufCommandOrPlugin {
            command_or_plugin_type,
        })
    }
}

impl TryFrom<ProtobufTiledPaneLayout> for TiledPaneLayout {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufTiledPaneLayout) -> Result<Self, Self::Error> {
        let children_split_direction =
            ProtobufSplitDirection::from_i32(protobuf.children_split_direction)
                .ok_or("Invalid split direction")?
                .try_into()?;
        let children = protobuf
            .children
            .into_iter()
            .map(|c| c.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        let split_size = protobuf.split_size.map(|s| s.try_into()).transpose()?;
        let run = protobuf.run.map(|r| r.try_into()).transpose()?;
        let focus = protobuf.focus.and_then(|f| {
            if f == "true" {
                Some(true)
            } else if f == "false" {
                Some(false)
            } else {
                None
            }
        });
        let run_instructions_to_ignore = vec![]; // Not serialized in protobuf
        Ok(TiledPaneLayout {
            children_split_direction,
            name: protobuf.name,
            children,
            split_size,
            run,
            borderless: protobuf.borderless,
            focus,
            external_children_index: protobuf.external_children_index.map(|i| i as usize),
            children_are_stacked: protobuf.children_are_stacked,
            is_expanded_in_stack: protobuf.is_expanded_in_stack,
            exclude_from_sync: protobuf.exclude_from_sync,
            run_instructions_to_ignore,
            hide_floating_panes: protobuf.hide_floating_panes,
            pane_initial_contents: protobuf.pane_initial_contents,
        })
    }
}

impl TryFrom<TiledPaneLayout> for ProtobufTiledPaneLayout {
    type Error = &'static str;
    fn try_from(internal: TiledPaneLayout) -> Result<Self, Self::Error> {
        let children_split_direction: ProtobufSplitDirection =
            internal.children_split_direction.try_into()?;
        let children = internal
            .children
            .into_iter()
            .map(|c| c.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        let split_size = internal.split_size.map(|s| s.try_into()).transpose()?;
        let run = internal.run.map(|r| r.try_into()).transpose()?;
        let focus = internal.focus.map(|f| f.to_string());
        Ok(ProtobufTiledPaneLayout {
            children_split_direction: children_split_direction as i32,
            name: internal.name,
            children,
            split_size,
            run,
            borderless: internal.borderless,
            focus,
            external_children_index: internal.external_children_index.map(|i| i as u32),
            children_are_stacked: internal.children_are_stacked,
            is_expanded_in_stack: internal.is_expanded_in_stack,
            exclude_from_sync: internal.exclude_from_sync,
            hide_floating_panes: internal.hide_floating_panes,
            pane_initial_contents: internal.pane_initial_contents,
        })
    }
}

impl TryFrom<ProtobufFloatingPaneLayout> for FloatingPaneLayout {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufFloatingPaneLayout) -> Result<Self, Self::Error> {
        let height = protobuf.height.map(|h| h.try_into()).transpose()?;
        let width = protobuf.width.map(|w| w.try_into()).transpose()?;
        let x = protobuf.x.map(|x| x.try_into()).transpose()?;
        let y = protobuf.y.map(|y| y.try_into()).transpose()?;
        let run = protobuf.run.map(|r| r.try_into()).transpose()?;
        Ok(FloatingPaneLayout {
            name: protobuf.name,
            height,
            width,
            x,
            y,
            pinned: protobuf.pinned,
            run,
            focus: protobuf.focus,
            already_running: protobuf.already_running,
            pane_initial_contents: protobuf.pane_initial_contents,
            logical_position: protobuf.logical_position.map(|p| p as usize),
        })
    }
}

impl TryFrom<FloatingPaneLayout> for ProtobufFloatingPaneLayout {
    type Error = &'static str;
    fn try_from(internal: FloatingPaneLayout) -> Result<Self, Self::Error> {
        let height = internal.height.map(|h| h.try_into()).transpose()?;
        let width = internal.width.map(|w| w.try_into()).transpose()?;
        let x = internal.x.map(|x| x.try_into()).transpose()?;
        let y = internal.y.map(|y| y.try_into()).transpose()?;
        let run = internal.run.map(|r| r.try_into()).transpose()?;
        Ok(ProtobufFloatingPaneLayout {
            name: internal.name,
            height,
            width,
            x,
            y,
            pinned: internal.pinned,
            run,
            focus: internal.focus,
            already_running: internal.already_running,
            pane_initial_contents: internal.pane_initial_contents,
            logical_position: internal.logical_position.map(|p| p as u32),
        })
    }
}

impl TryFrom<ProtobufLayoutConstraintTiledPair> for (LayoutConstraint, TiledPaneLayout) {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufLayoutConstraintTiledPair) -> Result<Self, Self::Error> {
        let constraint = protobuf
            .constraint
            .ok_or("LayoutConstraintTiledPair must have constraint")?
            .try_into()?;
        let layout = protobuf
            .layout
            .ok_or("LayoutConstraintTiledPair must have layout")?
            .try_into()?;
        Ok((constraint, layout))
    }
}

impl TryFrom<(LayoutConstraint, TiledPaneLayout)> for ProtobufLayoutConstraintTiledPair {
    type Error = &'static str;
    fn try_from(internal: (LayoutConstraint, TiledPaneLayout)) -> Result<Self, Self::Error> {
        Ok(ProtobufLayoutConstraintTiledPair {
            constraint: Some(internal.0.try_into()?),
            layout: Some(internal.1.try_into()?),
        })
    }
}

impl TryFrom<ProtobufLayoutConstraintFloatingPair> for (LayoutConstraint, Vec<FloatingPaneLayout>) {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufLayoutConstraintFloatingPair) -> Result<Self, Self::Error> {
        let constraint = protobuf
            .constraint
            .ok_or("LayoutConstraintFloatingPair must have constraint")?
            .try_into()?;
        let layouts = protobuf
            .layouts
            .into_iter()
            .map(|l| l.try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok((constraint, layouts))
    }
}

impl TryFrom<(LayoutConstraint, Vec<FloatingPaneLayout>)> for ProtobufLayoutConstraintFloatingPair {
    type Error = &'static str;
    fn try_from(
        internal: (LayoutConstraint, Vec<FloatingPaneLayout>),
    ) -> Result<Self, Self::Error> {
        Ok(ProtobufLayoutConstraintFloatingPair {
            constraint: Some(internal.0.try_into()?),
            layouts: internal
                .1
                .into_iter()
                .map(|l| l.try_into())
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

impl TryFrom<ProtobufSwapTiledLayout> for SwapTiledLayout {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufSwapTiledLayout) -> Result<Self, Self::Error> {
        let constraint_map: BTreeMap<LayoutConstraint, TiledPaneLayout> = protobuf
            .constraint_map
            .into_iter()
            .map(|pair| pair.try_into())
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok((constraint_map, protobuf.name))
    }
}

impl TryFrom<SwapTiledLayout> for ProtobufSwapTiledLayout {
    type Error = &'static str;
    fn try_from(internal: SwapTiledLayout) -> Result<Self, Self::Error> {
        let constraint_map = internal
            .0
            .into_iter()
            .map(|(constraint, layout)| (constraint, layout).try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProtobufSwapTiledLayout {
            constraint_map,
            name: internal.1,
        })
    }
}

impl TryFrom<ProtobufSwapFloatingLayout> for SwapFloatingLayout {
    type Error = &'static str;
    fn try_from(protobuf: ProtobufSwapFloatingLayout) -> Result<Self, Self::Error> {
        let constraint_map: BTreeMap<LayoutConstraint, Vec<FloatingPaneLayout>> = protobuf
            .constraint_map
            .into_iter()
            .map(|pair| pair.try_into())
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok((constraint_map, protobuf.name))
    }
}

impl TryFrom<SwapFloatingLayout> for ProtobufSwapFloatingLayout {
    type Error = &'static str;
    fn try_from(internal: SwapFloatingLayout) -> Result<Self, Self::Error> {
        let constraint_map = internal
            .0
            .into_iter()
            .map(|(constraint, layouts)| (constraint, layouts).try_into())
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProtobufSwapFloatingLayout {
            constraint_map,
            name: internal.1,
        })
    }
}
