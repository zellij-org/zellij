pub use super::generated_api::api::{
    action::{PaneIdAndShouldFloat, SwitchToModePayload},
    event::{EventNameList as ProtobufEventNameList, Header},
    input_mode::InputMode as ProtobufInputMode,
    plugin_command::{
        plugin_command::Payload, BreakPanesToNewTabPayload, BreakPanesToTabWithIndexPayload,
        ChangeFloatingPanesCoordinatesPayload, ChangeHostFolderPayload,
        ClearScreenForPaneIdPayload, CliPipeOutputPayload, CloseMultiplePanesPayload,
        CloseTabWithIndexPayload, CommandName, ContextItem,
        CreateTokenResponse as ProtobufCreateTokenResponse, CreateTokenResponse,
        EditScrollbackForPaneWithIdPayload, EmbedMultiplePanesPayload, EnvVariable, ExecCmdPayload,
        FixedOrPercent as ProtobufFixedOrPercent,
        FixedOrPercentValue as ProtobufFixedOrPercentValue, FloatMultiplePanesPayload,
        FloatingPaneCoordinates as ProtobufFloatingPaneCoordinates, GenerateWebLoginTokenPayload,
        GroupAndUngroupPanesPayload, HidePaneWithIdPayload, HighlightAndUnhighlightPanesPayload,
        HttpVerb as ProtobufHttpVerb, IdAndNewName, KeyToRebind, KeyToUnbind, KillSessionsPayload,
        ListTokensResponse, LoadNewPluginPayload, MessageToPluginPayload,
        MovePaneWithPaneIdInDirectionPayload, MovePaneWithPaneIdPayload, MovePayload,
        NewPluginArgs as ProtobufNewPluginArgs, NewTabPayload, NewTabsWithLayoutInfoPayload,
        OpenCommandPaneFloatingNearPluginPayload, OpenCommandPaneInPlaceOfPluginPayload,
        OpenCommandPaneNearPluginPayload, OpenCommandPanePayload,
        OpenFileFloatingNearPluginPayload, OpenFileInPlaceOfPluginPayload,
        OpenFileNearPluginPayload, OpenFilePayload, OpenTerminalFloatingNearPluginPayload,
        OpenTerminalInPlaceOfPluginPayload, OpenTerminalNearPluginPayload,
        PageScrollDownInPaneIdPayload, PageScrollUpInPaneIdPayload, PaneId as ProtobufPaneId,
        PaneIdAndFloatingPaneCoordinates, PaneType as ProtobufPaneType,
        PluginCommand as ProtobufPluginCommand, PluginMessagePayload, RebindKeysPayload,
        ReconfigurePayload, ReloadPluginPayload, RenameWebLoginTokenPayload,
        RenameWebTokenResponse, ReplacePaneWithExistingPanePayload, RequestPluginPermissionPayload,
        RerunCommandPanePayload, ResizePaneIdWithDirectionPayload, ResizePayload,
        RevokeAllWebTokensResponse, RevokeTokenResponse, RevokeWebLoginTokenPayload,
        RunCommandPayload, ScrollDownInPaneIdPayload, ScrollToBottomInPaneIdPayload,
        ScrollToTopInPaneIdPayload, ScrollUpInPaneIdPayload, SetFloatingPanePinnedPayload,
        SetSelfMouseSelectionSupportPayload, SetTimeoutPayload, ShowPaneWithIdPayload,
        StackPanesPayload, SubscribePayload, SwitchSessionPayload, SwitchTabToPayload,
        TogglePaneEmbedOrEjectForPaneIdPayload, TogglePaneIdFullscreenPayload, UnsubscribePayload,
        WebRequestPayload, WriteCharsToPaneIdPayload, WriteToPaneIdPayload,
    },
    plugin_permission::PermissionType as ProtobufPermissionType,
    resize::ResizeAction as ProtobufResizeAction,
};

use crate::data::{
    ConnectToSession, FloatingPaneCoordinates, HttpVerb, InputMode, KeyWithModifier,
    MessageToPlugin, NewPluginArgs, PaneId, PermissionType, PluginCommand,
};
use crate::input::actions::Action;
use crate::input::layout::SplitSize;

use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::path::PathBuf;

impl Into<FloatingPaneCoordinates> for ProtobufFloatingPaneCoordinates {
    fn into(self) -> FloatingPaneCoordinates {
        FloatingPaneCoordinates {
            x: self
                .x
                .and_then(|x| match ProtobufFixedOrPercent::from_i32(x.r#type) {
                    Some(ProtobufFixedOrPercent::Percent) => {
                        Some(SplitSize::Percent(x.value as usize))
                    },
                    Some(ProtobufFixedOrPercent::Fixed) => Some(SplitSize::Fixed(x.value as usize)),
                    None => None,
                }),
            y: self
                .y
                .and_then(|y| match ProtobufFixedOrPercent::from_i32(y.r#type) {
                    Some(ProtobufFixedOrPercent::Percent) => {
                        Some(SplitSize::Percent(y.value as usize))
                    },
                    Some(ProtobufFixedOrPercent::Fixed) => Some(SplitSize::Fixed(y.value as usize)),
                    None => None,
                }),
            width: self.width.and_then(|width| {
                match ProtobufFixedOrPercent::from_i32(width.r#type) {
                    Some(ProtobufFixedOrPercent::Percent) => {
                        Some(SplitSize::Percent(width.value as usize))
                    },
                    Some(ProtobufFixedOrPercent::Fixed) => {
                        Some(SplitSize::Fixed(width.value as usize))
                    },
                    None => None,
                }
            }),
            height: self.height.and_then(|height| {
                match ProtobufFixedOrPercent::from_i32(height.r#type) {
                    Some(ProtobufFixedOrPercent::Percent) => {
                        Some(SplitSize::Percent(height.value as usize))
                    },
                    Some(ProtobufFixedOrPercent::Fixed) => {
                        Some(SplitSize::Fixed(height.value as usize))
                    },
                    None => None,
                }
            }),
            pinned: self.pinned,
        }
    }
}

impl Into<ProtobufFloatingPaneCoordinates> for FloatingPaneCoordinates {
    fn into(self) -> ProtobufFloatingPaneCoordinates {
        ProtobufFloatingPaneCoordinates {
            x: match self.x {
                Some(SplitSize::Percent(percent)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Percent as i32,
                    value: percent as u32,
                }),
                Some(SplitSize::Fixed(fixed)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Fixed as i32,
                    value: fixed as u32,
                }),
                None => None,
            },
            y: match self.y {
                Some(SplitSize::Percent(percent)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Percent as i32,
                    value: percent as u32,
                }),
                Some(SplitSize::Fixed(fixed)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Fixed as i32,
                    value: fixed as u32,
                }),
                None => None,
            },
            width: match self.width {
                Some(SplitSize::Percent(percent)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Percent as i32,
                    value: percent as u32,
                }),
                Some(SplitSize::Fixed(fixed)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Fixed as i32,
                    value: fixed as u32,
                }),
                None => None,
            },
            height: match self.height {
                Some(SplitSize::Percent(percent)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Percent as i32,
                    value: percent as u32,
                }),
                Some(SplitSize::Fixed(fixed)) => Some(ProtobufFixedOrPercentValue {
                    r#type: ProtobufFixedOrPercent::Fixed as i32,
                    value: fixed as u32,
                }),
                None => None,
            },
            pinned: self.pinned,
        }
    }
}

impl Into<HttpVerb> for ProtobufHttpVerb {
    fn into(self) -> HttpVerb {
        match self {
            ProtobufHttpVerb::Get => HttpVerb::Get,
            ProtobufHttpVerb::Post => HttpVerb::Post,
            ProtobufHttpVerb::Put => HttpVerb::Put,
            ProtobufHttpVerb::Delete => HttpVerb::Delete,
        }
    }
}

impl Into<ProtobufHttpVerb> for HttpVerb {
    fn into(self) -> ProtobufHttpVerb {
        match self {
            HttpVerb::Get => ProtobufHttpVerb::Get,
            HttpVerb::Post => ProtobufHttpVerb::Post,
            HttpVerb::Put => ProtobufHttpVerb::Put,
            HttpVerb::Delete => ProtobufHttpVerb::Delete,
        }
    }
}

impl TryFrom<ProtobufPaneId> for PaneId {
    type Error = &'static str;
    fn try_from(protobuf_pane_id: ProtobufPaneId) -> Result<Self, &'static str> {
        match ProtobufPaneType::from_i32(protobuf_pane_id.pane_type) {
            Some(ProtobufPaneType::Terminal) => Ok(PaneId::Terminal(protobuf_pane_id.id)),
            Some(ProtobufPaneType::Plugin) => Ok(PaneId::Plugin(protobuf_pane_id.id)),
            None => Err("Failed to convert PaneId"),
        }
    }
}

impl TryFrom<PaneId> for ProtobufPaneId {
    type Error = &'static str;
    fn try_from(pane_id: PaneId) -> Result<Self, &'static str> {
        match pane_id {
            PaneId::Terminal(id) => Ok(ProtobufPaneId {
                pane_type: ProtobufPaneType::Terminal as i32,
                id,
            }),
            PaneId::Plugin(id) => Ok(ProtobufPaneId {
                pane_type: ProtobufPaneType::Plugin as i32,
                id,
            }),
        }
    }
}

impl TryFrom<(InputMode, KeyWithModifier, Vec<Action>)> for KeyToRebind {
    type Error = &'static str;
    fn try_from(
        key_to_rebind: (InputMode, KeyWithModifier, Vec<Action>),
    ) -> Result<Self, &'static str> {
        Ok(KeyToRebind {
            input_mode: key_to_rebind.0 as i32,
            key: Some(key_to_rebind.1.try_into()?),
            actions: key_to_rebind
                .2
                .into_iter()
                .filter_map(|a| a.try_into().ok())
                .collect(),
        })
    }
}

impl TryFrom<(InputMode, KeyWithModifier)> for KeyToUnbind {
    type Error = &'static str;
    fn try_from(key_to_unbind: (InputMode, KeyWithModifier)) -> Result<Self, &'static str> {
        Ok(KeyToUnbind {
            input_mode: key_to_unbind.0 as i32,
            key: Some(key_to_unbind.1.try_into()?),
        })
    }
}

fn key_to_rebind_to_plugin_command_assets(
    key_to_rebind: KeyToRebind,
) -> Option<(InputMode, KeyWithModifier, Vec<Action>)> {
    Some((
        ProtobufInputMode::from_i32(key_to_rebind.input_mode)?
            .try_into()
            .ok()?,
        key_to_rebind.key?.try_into().ok()?,
        key_to_rebind
            .actions
            .into_iter()
            .filter_map(|a| a.try_into().ok())
            .collect(),
    ))
}

fn key_to_unbind_to_plugin_command_assets(
    key_to_unbind: KeyToUnbind,
) -> Option<(InputMode, KeyWithModifier)> {
    Some((
        ProtobufInputMode::from_i32(key_to_unbind.input_mode)?
            .try_into()
            .ok()?,
        key_to_unbind.key?.try_into().ok()?,
    ))
}

impl TryFrom<ProtobufPluginCommand> for PluginCommand {
    type Error = &'static str;
    fn try_from(protobuf_plugin_command: ProtobufPluginCommand) -> Result<Self, &'static str> {
        match CommandName::from_i32(protobuf_plugin_command.name) {
            Some(CommandName::Subscribe) => match protobuf_plugin_command.payload {
                Some(Payload::SubscribePayload(subscribe_payload)) => {
                    let protobuf_event_list = subscribe_payload.subscriptions;
                    match protobuf_event_list {
                        Some(protobuf_event_list) => {
                            Ok(PluginCommand::Subscribe(protobuf_event_list.try_into()?))
                        },
                        None => Err("malformed subscription event"),
                    }
                },
                _ => Err("Mismatched payload for Subscribe"),
            },
            Some(CommandName::Unsubscribe) => match protobuf_plugin_command.payload {
                Some(Payload::UnsubscribePayload(unsubscribe_payload)) => {
                    let protobuf_event_list = unsubscribe_payload.subscriptions;
                    match protobuf_event_list {
                        Some(protobuf_event_list) => {
                            Ok(PluginCommand::Unsubscribe(protobuf_event_list.try_into()?))
                        },
                        None => Err("malformed unsubscription event"),
                    }
                },
                _ => Err("Mismatched payload for Unsubscribe"),
            },
            Some(CommandName::SetSelectable) => match protobuf_plugin_command.payload {
                Some(Payload::SetSelectablePayload(should_be_selectable)) => {
                    Ok(PluginCommand::SetSelectable(should_be_selectable))
                },
                _ => Err("Mismatched payload for SetSelectable"),
            },
            Some(CommandName::GetPluginIds) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("GetPluginIds should not have a payload")
                } else {
                    Ok(PluginCommand::GetPluginIds)
                }
            },
            Some(CommandName::GetZellijVersion) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("GetZellijVersion should not have a payload")
                } else {
                    Ok(PluginCommand::GetZellijVersion)
                }
            },
            Some(CommandName::OpenFile) => match protobuf_plugin_command.payload {
                Some(Payload::OpenFilePayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            let context: BTreeMap<String, String> = file_to_open_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenFile(file_to_open.try_into()?, context))
                        },
                        None => Err("Malformed open file payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenFile"),
            },
            Some(CommandName::OpenFileFloating) => match protobuf_plugin_command.payload {
                Some(Payload::OpenFileFloatingPayload(file_to_open_payload)) => {
                    let floating_pane_coordinates = file_to_open_payload
                        .floating_pane_coordinates
                        .map(|f| f.into());
                    let context: BTreeMap<String, String> = file_to_open_payload
                        .context
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => Ok(PluginCommand::OpenFileFloating(
                            file_to_open.try_into()?,
                            floating_pane_coordinates,
                            context,
                        )),
                        None => Err("Malformed open file payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenFileFloating"),
            },
            Some(CommandName::OpenTerminal) => match protobuf_plugin_command.payload {
                Some(Payload::OpenTerminalPayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            Ok(PluginCommand::OpenTerminal(file_to_open.try_into()?))
                        },
                        None => Err("Malformed open terminal payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenTerminal"),
            },
            Some(CommandName::OpenTerminalFloating) => match protobuf_plugin_command.payload {
                Some(Payload::OpenTerminalFloatingPayload(file_to_open_payload)) => {
                    let floating_pane_coordinates = file_to_open_payload
                        .floating_pane_coordinates
                        .map(|f| f.into());
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => Ok(PluginCommand::OpenTerminalFloating(
                            file_to_open.try_into()?,
                            floating_pane_coordinates,
                        )),
                        None => Err("Malformed open terminal floating payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenTerminalFloating"),
            },
            Some(CommandName::OpenCommandPane) => match protobuf_plugin_command.payload {
                Some(Payload::OpenCommandPanePayload(command_to_run_payload)) => {
                    match command_to_run_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> = command_to_run_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenCommandPane(
                                command_to_run.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open open command pane payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenCommandPane"),
            },
            Some(CommandName::OpenCommandPaneFloating) => match protobuf_plugin_command.payload {
                Some(Payload::OpenCommandPaneFloatingPayload(command_to_run_payload)) => {
                    let floating_pane_coordinates = command_to_run_payload
                        .floating_pane_coordinates
                        .map(|f| f.into());
                    match command_to_run_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> = command_to_run_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenCommandPaneFloating(
                                command_to_run.try_into()?,
                                floating_pane_coordinates,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane floating payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenCommandPaneFloating"),
            },
            Some(CommandName::SwitchTabTo) => match protobuf_plugin_command.payload {
                Some(Payload::SwitchTabToPayload(switch_to_tab_payload)) => Ok(
                    PluginCommand::SwitchTabTo(switch_to_tab_payload.tab_index as u32),
                ),
                _ => Err("Mismatched payload for SwitchToTab"),
            },
            Some(CommandName::SetTimeout) => match protobuf_plugin_command.payload {
                Some(Payload::SetTimeoutPayload(set_timeout_payload)) => {
                    Ok(PluginCommand::SetTimeout(set_timeout_payload.seconds))
                },
                _ => Err("Mismatched payload for SetTimeout"),
            },
            Some(CommandName::ExecCmd) => match protobuf_plugin_command.payload {
                Some(Payload::ExecCmdPayload(exec_cmd_payload)) => {
                    Ok(PluginCommand::ExecCmd(exec_cmd_payload.command_line))
                },
                _ => Err("Mismatched payload for ExecCmd"),
            },
            Some(CommandName::PostMessageTo) => match protobuf_plugin_command.payload {
                Some(Payload::PostMessageToPayload(post_message_to_payload)) => {
                    match post_message_to_payload.message {
                        Some(message) => Ok(PluginCommand::PostMessageTo(message.try_into()?)),
                        None => Err("Malformed post message to payload"),
                    }
                },
                _ => Err("Mismatched payload for PostMessageTo"),
            },
            Some(CommandName::PostMessageToPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::PostMessageToPluginPayload(post_message_to_payload)) => {
                    match post_message_to_payload.message {
                        Some(message) => {
                            Ok(PluginCommand::PostMessageToPlugin(message.try_into()?))
                        },
                        None => Err("Malformed post message to plugin payload"),
                    }
                },
                _ => Err("Mismatched payload for PostMessageToPlugin"),
            },
            Some(CommandName::HideSelf) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("HideSelf should not have a payload");
                }
                Ok(PluginCommand::HideSelf)
            },
            Some(CommandName::ShowSelf) => match protobuf_plugin_command.payload {
                Some(Payload::ShowSelfPayload(should_float_if_hidden)) => {
                    Ok(PluginCommand::ShowSelf(should_float_if_hidden))
                },
                _ => Err("Mismatched payload for ShowSelf"),
            },
            Some(CommandName::SwitchToMode) => match protobuf_plugin_command.payload {
                Some(Payload::SwitchToModePayload(switch_to_mode_payload)) => {
                    match ProtobufInputMode::from_i32(switch_to_mode_payload.input_mode) {
                        Some(protobuf_input_mode) => {
                            Ok(PluginCommand::SwitchToMode(protobuf_input_mode.try_into()?))
                        },
                        None => Err("Malformed switch to mode payload"),
                    }
                },
                _ => Err("Mismatched payload for SwitchToMode"),
            },
            Some(CommandName::NewTabsWithLayout) => match protobuf_plugin_command.payload {
                Some(Payload::NewTabsWithLayoutPayload(raw_layout)) => {
                    Ok(PluginCommand::NewTabsWithLayout(raw_layout))
                },
                _ => Err("Mismatched payload for NewTabsWithLayout"),
            },
            Some(CommandName::NewTab) => match protobuf_plugin_command.payload {
                Some(Payload::NewTabPayload(protobuf_new_tab_payload)) => {
                    Ok(PluginCommand::NewTab {
                        name: protobuf_new_tab_payload.name,
                        cwd: protobuf_new_tab_payload.cwd,
                    })
                },
                None => Ok(PluginCommand::NewTab {
                    name: None,
                    cwd: None,
                }),
                _ => Err("Mismatched payload for NewTab"),
            },
            Some(CommandName::GoToNextTab) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("GoToNextTab should not have a payload");
                }
                Ok(PluginCommand::GoToNextTab)
            },
            Some(CommandName::GoToPreviousTab) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("GoToPreviousTab should not have a payload");
                }
                Ok(PluginCommand::GoToPreviousTab)
            },
            Some(CommandName::Resize) => match protobuf_plugin_command.payload {
                Some(Payload::ResizePayload(resize_payload)) => match resize_payload.resize {
                    Some(resize) => Ok(PluginCommand::Resize(resize.try_into()?)),
                    None => Err("Malformed switch resize payload"),
                },
                _ => Err("Mismatched payload for Resize"),
            },
            Some(CommandName::ResizeWithDirection) => match protobuf_plugin_command.payload {
                Some(Payload::ResizeWithDirectionPayload(resize_with_direction_payload)) => {
                    match resize_with_direction_payload.resize {
                        Some(resize) => Ok(PluginCommand::ResizeWithDirection(resize.try_into()?)),
                        None => Err("Malformed switch resize payload"),
                    }
                },
                _ => Err("Mismatched payload for Resize"),
            },
            Some(CommandName::FocusNextPane) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("FocusNextPane should not have a payload");
                }
                Ok(PluginCommand::FocusNextPane)
            },
            Some(CommandName::FocusPreviousPane) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("FocusPreviousPane should not have a payload");
                }
                Ok(PluginCommand::FocusPreviousPane)
            },
            Some(CommandName::MoveFocus) => match protobuf_plugin_command.payload {
                Some(Payload::MoveFocusPayload(move_payload)) => match move_payload.direction {
                    Some(direction) => Ok(PluginCommand::MoveFocus(direction.try_into()?)),
                    None => Err("Malformed move focus payload"),
                },
                _ => Err("Mismatched payload for MoveFocus"),
            },
            Some(CommandName::MoveFocusOrTab) => match protobuf_plugin_command.payload {
                Some(Payload::MoveFocusOrTabPayload(move_payload)) => {
                    match move_payload.direction {
                        Some(direction) => Ok(PluginCommand::MoveFocusOrTab(direction.try_into()?)),
                        None => Err("Malformed move focus or tab payload"),
                    }
                },
                _ => Err("Mismatched payload for MoveFocusOrTab"),
            },
            Some(CommandName::Detach) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("Detach should not have a payload");
                }
                Ok(PluginCommand::Detach)
            },
            Some(CommandName::EditScrollback) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("EditScrollback should not have a payload");
                }
                Ok(PluginCommand::EditScrollback)
            },
            Some(CommandName::Write) => match protobuf_plugin_command.payload {
                Some(Payload::WritePayload(bytes)) => Ok(PluginCommand::Write(bytes)),
                _ => Err("Mismatched payload for Write"),
            },
            Some(CommandName::WriteChars) => match protobuf_plugin_command.payload {
                Some(Payload::WriteCharsPayload(chars)) => Ok(PluginCommand::WriteChars(chars)),
                _ => Err("Mismatched payload for WriteChars"),
            },
            Some(CommandName::ToggleTab) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ToggleTab should not have a payload");
                }
                Ok(PluginCommand::ToggleTab)
            },
            Some(CommandName::MovePane) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("MovePane should not have a payload");
                }
                Ok(PluginCommand::MovePane)
            },
            Some(CommandName::MovePaneWithDirection) => match protobuf_plugin_command.payload {
                Some(Payload::MovePaneWithDirectionPayload(move_payload)) => {
                    match move_payload.direction {
                        Some(direction) => {
                            Ok(PluginCommand::MovePaneWithDirection(direction.try_into()?))
                        },
                        None => Err("Malformed MovePaneWithDirection payload"),
                    }
                },
                _ => Err("Mismatched payload for MovePaneWithDirection"),
            },
            Some(CommandName::ClearScreen) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ClearScreen should not have a payload");
                }
                Ok(PluginCommand::ClearScreen)
            },
            Some(CommandName::ScrollUp) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ScrollUp should not have a payload");
                }
                Ok(PluginCommand::ScrollUp)
            },
            Some(CommandName::ScrollDown) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ScrollDown should not have a payload");
                }
                Ok(PluginCommand::ScrollDown)
            },
            Some(CommandName::ScrollToTop) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ScrollToTop should not have a payload");
                }
                Ok(PluginCommand::ScrollToTop)
            },
            Some(CommandName::ScrollToBottom) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ScrollToBottom should not have a payload");
                }
                Ok(PluginCommand::ScrollToBottom)
            },
            Some(CommandName::PageScrollUp) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("PageScrollUp should not have a payload");
                }
                Ok(PluginCommand::PageScrollUp)
            },
            Some(CommandName::PageScrollDown) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("PageScrollDown should not have a payload");
                }
                Ok(PluginCommand::PageScrollDown)
            },
            Some(CommandName::ToggleFocusFullscreen) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ToggleFocusFullscreen should not have a payload");
                }
                Ok(PluginCommand::ToggleFocusFullscreen)
            },
            Some(CommandName::TogglePaneFrames) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("TogglePaneFrames should not have a payload");
                }
                Ok(PluginCommand::TogglePaneFrames)
            },
            Some(CommandName::TogglePaneEmbedOrEject) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("TogglePaneEmbedOrEject should not have a payload");
                }
                Ok(PluginCommand::TogglePaneEmbedOrEject)
            },
            Some(CommandName::UndoRenamePane) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("UndoRenamePane should not have a payload");
                }
                Ok(PluginCommand::UndoRenamePane)
            },
            Some(CommandName::CloseFocus) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("CloseFocus should not have a payload");
                }
                Ok(PluginCommand::CloseFocus)
            },
            Some(CommandName::ToggleActiveTabSync) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("ToggleActiveTabSync should not have a payload");
                }
                Ok(PluginCommand::ToggleActiveTabSync)
            },
            Some(CommandName::CloseFocusedTab) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("CloseFocusedTab should not have a payload");
                }
                Ok(PluginCommand::CloseFocusedTab)
            },
            Some(CommandName::UndoRenameTab) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("UndoRenameTab should not have a payload");
                }
                Ok(PluginCommand::UndoRenameTab)
            },
            Some(CommandName::QuitZellij) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("QuitZellij should not have a payload");
                }
                Ok(PluginCommand::QuitZellij)
            },
            Some(CommandName::PreviousSwapLayout) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("PreviousSwapLayout should not have a payload");
                }
                Ok(PluginCommand::PreviousSwapLayout)
            },
            Some(CommandName::NextSwapLayout) => {
                if protobuf_plugin_command.payload.is_some() {
                    return Err("NextSwapLayout should not have a payload");
                }
                Ok(PluginCommand::NextSwapLayout)
            },
            Some(CommandName::GoToTabName) => match protobuf_plugin_command.payload {
                Some(Payload::GoToTabNamePayload(tab_name)) => {
                    Ok(PluginCommand::GoToTabName(tab_name))
                },
                _ => Err("Mismatched payload for GoToTabName"),
            },
            Some(CommandName::FocusOrCreateTab) => match protobuf_plugin_command.payload {
                Some(Payload::FocusOrCreateTabPayload(tab_name)) => {
                    Ok(PluginCommand::FocusOrCreateTab(tab_name))
                },
                _ => Err("Mismatched payload for FocusOrCreateTab"),
            },
            Some(CommandName::GoToTab) => match protobuf_plugin_command.payload {
                Some(Payload::GoToTabPayload(tab_index)) => {
                    Ok(PluginCommand::GoToTab(tab_index as u32))
                },
                _ => Err("Mismatched payload for GoToTab"),
            },
            Some(CommandName::StartOrReloadPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::StartOrReloadPluginPayload(url)) => {
                    Ok(PluginCommand::StartOrReloadPlugin(url))
                },
                _ => Err("Mismatched payload for StartOrReloadPlugin"),
            },
            Some(CommandName::CloseTerminalPane) => match protobuf_plugin_command.payload {
                Some(Payload::CloseTerminalPanePayload(pane_id)) => {
                    Ok(PluginCommand::CloseTerminalPane(pane_id as u32))
                },
                _ => Err("Mismatched payload for CloseTerminalPane"),
            },
            Some(CommandName::ClosePluginPane) => match protobuf_plugin_command.payload {
                Some(Payload::ClosePluginPanePayload(pane_id)) => {
                    Ok(PluginCommand::ClosePluginPane(pane_id as u32))
                },
                _ => Err("Mismatched payload for ClosePluginPane"),
            },
            Some(CommandName::FocusTerminalPane) => match protobuf_plugin_command.payload {
                Some(Payload::FocusTerminalPanePayload(payload)) => {
                    let pane_id = payload.pane_id as u32;
                    let should_float = payload.should_float;
                    Ok(PluginCommand::FocusTerminalPane(pane_id, should_float))
                },
                _ => Err("Mismatched payload for ClosePluginPane"),
            },
            Some(CommandName::FocusPluginPane) => match protobuf_plugin_command.payload {
                Some(Payload::FocusPluginPanePayload(payload)) => {
                    let pane_id = payload.pane_id as u32;
                    let should_float = payload.should_float;
                    Ok(PluginCommand::FocusPluginPane(pane_id, should_float))
                },
                _ => Err("Mismatched payload for ClosePluginPane"),
            },
            Some(CommandName::RenameTerminalPane) => match protobuf_plugin_command.payload {
                Some(Payload::RenameTerminalPanePayload(payload)) => {
                    let pane_id = payload.id as u32;
                    let new_name = payload.new_name;
                    Ok(PluginCommand::RenameTerminalPane(pane_id, new_name))
                },
                _ => Err("Mismatched payload for RenameTerminalPane"),
            },
            Some(CommandName::RenamePluginPane) => match protobuf_plugin_command.payload {
                Some(Payload::RenamePluginPanePayload(payload)) => {
                    let pane_id = payload.id as u32;
                    let new_name = payload.new_name;
                    Ok(PluginCommand::RenamePluginPane(pane_id, new_name))
                },
                _ => Err("Mismatched payload for RenamePluginPane"),
            },
            Some(CommandName::RenameTab) => match protobuf_plugin_command.payload {
                Some(Payload::RenameTabPayload(payload)) => {
                    let tab_index = payload.id as u32;
                    let name = payload.new_name;
                    Ok(PluginCommand::RenameTab(tab_index, name))
                },
                _ => Err("Mismatched payload for RenameTab"),
            },
            Some(CommandName::ReportCrash) => match protobuf_plugin_command.payload {
                Some(Payload::ReportCrashPayload(payload)) => {
                    Ok(PluginCommand::ReportPanic(payload))
                },
                _ => Err("Mismatched payload for ReportCrash"),
            },
            Some(CommandName::RequestPluginPermissions) => match protobuf_plugin_command.payload {
                Some(Payload::RequestPluginPermissionPayload(payload)) => {
                    Ok(PluginCommand::RequestPluginPermissions(
                        payload
                            .permissions
                            .iter()
                            .filter_map(|p| ProtobufPermissionType::from_i32(*p))
                            .filter_map(|p| PermissionType::try_from(p).ok())
                            .collect(),
                    ))
                },
                _ => Err("Mismatched payload for RequestPluginPermission"),
            },
            Some(CommandName::SwitchSession) => match protobuf_plugin_command.payload {
                Some(Payload::SwitchSessionPayload(payload)) => {
                    let pane_id = match (payload.pane_id, payload.pane_id_is_plugin) {
                        (Some(pane_id), Some(is_plugin)) => Some((pane_id, is_plugin)),
                        (None, None) => None,
                        _ => {
                            return Err("Malformed payload for SwitchSession, 'pane_id' and 'is_plugin' must be included together or not at all")
                        }
                    };
                    Ok(PluginCommand::SwitchSession(ConnectToSession {
                        name: payload.name,
                        tab_position: payload.tab_position.map(|p| p as usize),
                        pane_id,
                        layout: payload.layout.and_then(|l| l.try_into().ok()),
                        cwd: payload.cwd.map(|c| PathBuf::from(c)),
                    }))
                },
                _ => Err("Mismatched payload for SwitchSession"),
            },
            Some(CommandName::OpenTerminalInPlace) => match protobuf_plugin_command.payload {
                Some(Payload::OpenTerminalInPlacePayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            Ok(PluginCommand::OpenTerminalInPlace(file_to_open.try_into()?))
                        },
                        None => Err("Malformed open terminal in-place payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenTerminalInPlace"),
            },
            Some(CommandName::OpenFileInPlace) => match protobuf_plugin_command.payload {
                Some(Payload::OpenFileInPlacePayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            let context: BTreeMap<String, String> = file_to_open_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenFileInPlace(
                                file_to_open.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open file in place payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenFileInPlace"),
            },
            Some(CommandName::OpenCommandInPlace) => match protobuf_plugin_command.payload {
                Some(Payload::OpenCommandPaneInPlacePayload(command_to_run_payload)) => {
                    match command_to_run_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> = command_to_run_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenCommandPaneInPlace(
                                command_to_run.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane in-place payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenCommandPaneInPlace"),
            },
            Some(CommandName::RunCommand) => match protobuf_plugin_command.payload {
                Some(Payload::RunCommandPayload(run_command_payload)) => {
                    let env_variables: BTreeMap<String, String> = run_command_payload
                        .env_variables
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    let context: BTreeMap<String, String> = run_command_payload
                        .context
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    Ok(PluginCommand::RunCommand(
                        run_command_payload.command_line,
                        env_variables,
                        PathBuf::from(run_command_payload.cwd),
                        context,
                    ))
                },
                _ => Err("Mismatched payload for RunCommand"),
            },
            Some(CommandName::WebRequest) => match protobuf_plugin_command.payload {
                Some(Payload::WebRequestPayload(web_request_payload)) => {
                    let context: BTreeMap<String, String> = web_request_payload
                        .context
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    let headers: BTreeMap<String, String> = web_request_payload
                        .headers
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    let verb = match ProtobufHttpVerb::from_i32(web_request_payload.verb) {
                        Some(verb) => verb.into(),
                        None => {
                            return Err("Unrecognized http verb");
                        },
                    };
                    Ok(PluginCommand::WebRequest(
                        web_request_payload.url,
                        verb,
                        headers,
                        web_request_payload.body,
                        context,
                    ))
                },
                _ => Err("Mismatched payload for WebRequest"),
            },
            Some(CommandName::DeleteDeadSession) => match protobuf_plugin_command.payload {
                Some(Payload::DeleteDeadSessionPayload(dead_session_name)) => {
                    Ok(PluginCommand::DeleteDeadSession(dead_session_name))
                },
                _ => Err("Mismatched payload for DeleteDeadSession"),
            },
            Some(CommandName::DeleteAllDeadSessions) => Ok(PluginCommand::DeleteAllDeadSessions),
            Some(CommandName::RenameSession) => match protobuf_plugin_command.payload {
                Some(Payload::RenameSessionPayload(new_session_name)) => {
                    Ok(PluginCommand::RenameSession(new_session_name))
                },
                _ => Err("Mismatched payload for RenameSession"),
            },
            Some(CommandName::UnblockCliPipeInput) => match protobuf_plugin_command.payload {
                Some(Payload::UnblockCliPipeInputPayload(pipe_name)) => {
                    Ok(PluginCommand::UnblockCliPipeInput(pipe_name))
                },
                _ => Err("Mismatched payload for UnblockPipeInput"),
            },
            Some(CommandName::BlockCliPipeInput) => match protobuf_plugin_command.payload {
                Some(Payload::BlockCliPipeInputPayload(pipe_name)) => {
                    Ok(PluginCommand::BlockCliPipeInput(pipe_name))
                },
                _ => Err("Mismatched payload for BlockPipeInput"),
            },
            Some(CommandName::CliPipeOutput) => match protobuf_plugin_command.payload {
                Some(Payload::CliPipeOutputPayload(CliPipeOutputPayload { pipe_name, output })) => {
                    Ok(PluginCommand::CliPipeOutput(pipe_name, output))
                },
                _ => Err("Mismatched payload for PipeOutput"),
            },
            Some(CommandName::MessageToPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::MessageToPluginPayload(MessageToPluginPayload {
                    plugin_url,
                    plugin_config,
                    message_name,
                    message_payload,
                    message_args,
                    new_plugin_args,
                    destination_plugin_id,
                    floating_pane_coordinates,
                })) => {
                    let plugin_config: BTreeMap<String, String> = plugin_config
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    let message_args: BTreeMap<String, String> = message_args
                        .into_iter()
                        .map(|e| (e.name, e.value))
                        .collect();
                    Ok(PluginCommand::MessageToPlugin(MessageToPlugin {
                        plugin_url,
                        plugin_config,
                        message_name,
                        message_payload,
                        message_args,
                        new_plugin_args: new_plugin_args.and_then(|protobuf_new_plugin_args| {
                            Some(NewPluginArgs {
                                should_float: protobuf_new_plugin_args.should_float,
                                pane_id_to_replace: protobuf_new_plugin_args
                                    .pane_id_to_replace
                                    .and_then(|p_id| PaneId::try_from(p_id).ok()),
                                pane_title: protobuf_new_plugin_args.pane_title,
                                cwd: protobuf_new_plugin_args.cwd.map(|cwd| PathBuf::from(cwd)),
                                skip_cache: protobuf_new_plugin_args.skip_cache,
                                should_focus: protobuf_new_plugin_args.should_focus,
                            })
                        }),
                        destination_plugin_id,
                        floating_pane_coordinates: floating_pane_coordinates
                            .and_then(|f| f.try_into().ok()),
                    }))
                },
                _ => Err("Mismatched payload for MessageToPlugin"),
            },
            Some(CommandName::DisconnectOtherClients) => match protobuf_plugin_command.payload {
                None => Ok(PluginCommand::DisconnectOtherClients),
                _ => Err("Mismatched payload for DisconnectOtherClients"),
            },
            Some(CommandName::KillSessions) => match protobuf_plugin_command.payload {
                Some(Payload::KillSessionsPayload(KillSessionsPayload { session_names })) => {
                    Ok(PluginCommand::KillSessions(session_names))
                },
                _ => Err("Mismatched payload for KillSessions"),
            },
            Some(CommandName::ScanHostFolder) => match protobuf_plugin_command.payload {
                Some(Payload::ScanHostFolderPayload(folder_to_scan)) => {
                    Ok(PluginCommand::ScanHostFolder(PathBuf::from(folder_to_scan)))
                },
                _ => Err("Mismatched payload for ScanHostFolder"),
            },
            Some(CommandName::WatchFilesystem) => match protobuf_plugin_command.payload {
                Some(_) => Err("WatchFilesystem should have no payload, found a payload"),
                None => Ok(PluginCommand::WatchFilesystem),
            },
            Some(CommandName::DumpSessionLayout) => match protobuf_plugin_command.payload {
                Some(_) => Err("DumpSessionLayout should have no payload, found a payload"),
                None => Ok(PluginCommand::DumpSessionLayout),
            },
            Some(CommandName::CloseSelf) => match protobuf_plugin_command.payload {
                Some(_) => Err("CloseSelf should have no payload, found a payload"),
                None => Ok(PluginCommand::CloseSelf),
            },
            Some(CommandName::NewTabsWithLayoutInfo) => match protobuf_plugin_command.payload {
                Some(Payload::NewTabsWithLayoutInfoPayload(new_tabs_with_layout_info_payload)) => {
                    new_tabs_with_layout_info_payload
                        .layout_info
                        .and_then(|layout_info| {
                            Some(PluginCommand::NewTabsWithLayoutInfo(
                                layout_info.try_into().ok()?,
                            ))
                        })
                        .ok_or("Failed to parse NewTabsWithLayoutInfo command")
                },
                _ => Err("Mismatched payload for NewTabsWithLayoutInfo"),
            },
            Some(CommandName::Reconfigure) => match protobuf_plugin_command.payload {
                Some(Payload::ReconfigurePayload(reconfigure_payload)) => {
                    Ok(PluginCommand::Reconfigure(
                        reconfigure_payload.config,
                        reconfigure_payload.write_to_disk,
                    ))
                },
                _ => Err("Mismatched payload for Reconfigure"),
            },
            Some(CommandName::HidePaneWithId) => match protobuf_plugin_command.payload {
                Some(Payload::HidePaneWithIdPayload(hide_pane_with_id_payload)) => {
                    let pane_id = hide_pane_with_id_payload
                        .pane_id
                        .and_then(|p_id| PaneId::try_from(p_id).ok())
                        .ok_or("Failed to parse HidePaneWithId command")?;
                    Ok(PluginCommand::HidePaneWithId(pane_id))
                },
                _ => Err("Mismatched payload for HidePaneWithId"),
            },
            Some(CommandName::ShowPaneWithId) => match protobuf_plugin_command.payload {
                Some(Payload::ShowPaneWithIdPayload(show_pane_with_id_payload)) => {
                    let pane_id = show_pane_with_id_payload
                        .pane_id
                        .and_then(|p_id| PaneId::try_from(p_id).ok())
                        .ok_or("Failed to parse ShowPaneWithId command")?;
                    let should_float_if_hidden = show_pane_with_id_payload.should_float_if_hidden;
                    Ok(PluginCommand::ShowPaneWithId(
                        pane_id,
                        should_float_if_hidden,
                    ))
                },
                _ => Err("Mismatched payload for ShowPaneWithId"),
            },
            Some(CommandName::OpenCommandPaneBackground) => match protobuf_plugin_command.payload {
                Some(Payload::OpenCommandPaneBackgroundPayload(command_to_run_payload)) => {
                    match command_to_run_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> = command_to_run_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenCommandPaneBackground(
                                command_to_run.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane background payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenCommandPaneBackground"),
            },
            Some(CommandName::RerunCommandPane) => match protobuf_plugin_command.payload {
                Some(Payload::RerunCommandPanePayload(rerun_command_pane_payload)) => Ok(
                    PluginCommand::RerunCommandPane(rerun_command_pane_payload.terminal_pane_id),
                ),
                _ => Err("Mismatched payload for RerunCommandPane"),
            },
            Some(CommandName::ResizePaneIdWithDirection) => match protobuf_plugin_command.payload {
                Some(Payload::ResizePaneIdWithDirectionPayload(resize_with_direction_payload)) => {
                    match (
                        resize_with_direction_payload.resize,
                        resize_with_direction_payload.pane_id,
                    ) {
                        (Some(resize), Some(pane_id)) => {
                            Ok(PluginCommand::ResizePaneIdWithDirection(
                                resize.try_into()?,
                                pane_id.try_into()?,
                            ))
                        },
                        _ => Err("Malformed resize_pane_with_id payload"),
                    }
                },
                _ => Err("Mismatched payload for Resize"),
            },
            Some(CommandName::EditScrollbackForPaneWithId) => match protobuf_plugin_command.payload
            {
                Some(Payload::EditScrollbackForPaneWithIdPayload(
                    edit_scrollback_for_pane_with_id_payload,
                )) => match edit_scrollback_for_pane_with_id_payload.pane_id {
                    Some(pane_id) => Ok(PluginCommand::EditScrollbackForPaneWithId(
                        pane_id.try_into()?,
                    )),
                    _ => Err("Malformed edit_scrollback_for_pane_with_id payload"),
                },
                _ => Err("Mismatched payload for EditScrollback"),
            },
            Some(CommandName::WriteToPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::WriteToPaneIdPayload(write_to_pane_id_payload)) => {
                    match write_to_pane_id_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::WriteToPaneId(
                            write_to_pane_id_payload.bytes_to_write,
                            pane_id.try_into()?,
                        )),
                        _ => Err("Malformed write_to_pane_id payload"),
                    }
                },
                _ => Err("Mismatched payload for WriteToPaneId"),
            },
            Some(CommandName::WriteCharsToPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::WriteCharsToPaneIdPayload(write_chars_to_pane_id_payload)) => {
                    match write_chars_to_pane_id_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::WriteCharsToPaneId(
                            write_chars_to_pane_id_payload.chars_to_write,
                            pane_id.try_into()?,
                        )),
                        _ => Err("Malformed write_chars_to_pane_id payload"),
                    }
                },
                _ => Err("Mismatched payload for WriteCharsCharsToPaneId"),
            },
            Some(CommandName::MovePaneWithPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::MovePaneWithPaneIdPayload(move_pane_with_pane_id_payload)) => {
                    match move_pane_with_pane_id_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::MovePaneWithPaneId(pane_id.try_into()?)),
                        _ => Err("Malformed move_pane_with_pane_id payload"),
                    }
                },
                _ => Err("Mismatched payload for MovePaneWithPaneId"),
            },
            Some(CommandName::MovePaneWithPaneIdInDirection) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::MovePaneWithPaneIdInDirectionPayload(move_payload)) => {
                        match (move_payload.direction, move_payload.pane_id) {
                            (Some(direction), Some(pane_id)) => {
                                Ok(PluginCommand::MovePaneWithPaneIdInDirection(
                                    pane_id.try_into()?,
                                    direction.try_into()?,
                                ))
                            },
                            _ => Err("Malformed MovePaneWithPaneIdInDirection payload"),
                        }
                    },
                    _ => Err("Mismatched payload for MovePaneWithDirection"),
                }
            },
            Some(CommandName::ClearScreenForPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::ClearScreenForPaneIdPayload(clear_screen_for_pane_id_payload)) => {
                    match clear_screen_for_pane_id_payload.pane_id {
                        Some(pane_id) => {
                            Ok(PluginCommand::ClearScreenForPaneId(pane_id.try_into()?))
                        },
                        _ => Err("Malformed clear_screen_for_pane_id_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for ClearScreenForPaneId"),
            },
            Some(CommandName::ScrollUpInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::ScrollUpInPaneIdPayload(scroll_up_in_pane_id_payload)) => {
                    match scroll_up_in_pane_id_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::ScrollUpInPaneId(pane_id.try_into()?)),
                        _ => Err("Malformed scroll_up_in_pane_id_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for ScrollUpInPaneId"),
            },
            Some(CommandName::ScrollDownInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::ScrollDownInPaneIdPayload(scroll_down_in_pane_id_payload)) => {
                    match scroll_down_in_pane_id_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::ScrollDownInPaneId(pane_id.try_into()?)),
                        _ => Err("Malformed scroll_down_in_pane_id_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for ScrollDownInPaneId"),
            },
            Some(CommandName::ScrollToTopInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::ScrollToTopInPaneIdPayload(scroll_to_top_in_pane_id_payload)) => {
                    match scroll_to_top_in_pane_id_payload.pane_id {
                        Some(pane_id) => {
                            Ok(PluginCommand::ScrollToTopInPaneId(pane_id.try_into()?))
                        },
                        _ => Err("Malformed scroll_to_top_in_pane_id_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for ScrollToTopInPaneId"),
            },
            Some(CommandName::ScrollToBottomInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::ScrollToBottomInPaneIdPayload(
                    scroll_to_bottom_in_pane_id_payload,
                )) => match scroll_to_bottom_in_pane_id_payload.pane_id {
                    Some(pane_id) => Ok(PluginCommand::ScrollToBottomInPaneId(pane_id.try_into()?)),
                    _ => Err("Malformed scroll_to_bottom_in_pane_id_payload payload"),
                },
                _ => Err("Mismatched payload for ScrollToBottomInPaneId"),
            },
            Some(CommandName::PageScrollUpInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::PageScrollUpInPaneIdPayload(page_scroll_up_in_pane_id_payload)) => {
                    match page_scroll_up_in_pane_id_payload.pane_id {
                        Some(pane_id) => {
                            Ok(PluginCommand::PageScrollUpInPaneId(pane_id.try_into()?))
                        },
                        _ => Err("Malformed page_scroll_up_in_pane_id_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for PageScrollUpInPaneId"),
            },
            Some(CommandName::PageScrollDownInPaneId) => match protobuf_plugin_command.payload {
                Some(Payload::PageScrollDownInPaneIdPayload(
                    page_scroll_down_in_pane_id_payload,
                )) => match page_scroll_down_in_pane_id_payload.pane_id {
                    Some(pane_id) => Ok(PluginCommand::PageScrollDownInPaneId(pane_id.try_into()?)),
                    _ => Err("Malformed page_scroll_down_in_pane_id_payload payload"),
                },
                _ => Err("Mismatched payload for PageScrollDownInPaneId"),
            },
            Some(CommandName::TogglePaneIdFullscreen) => match protobuf_plugin_command.payload {
                Some(Payload::TogglePaneIdFullscreenPayload(toggle_pane_id_fullscreen_payload)) => {
                    match toggle_pane_id_fullscreen_payload.pane_id {
                        Some(pane_id) => {
                            Ok(PluginCommand::TogglePaneIdFullscreen(pane_id.try_into()?))
                        },
                        _ => Err("Malformed toggle_pane_id_fullscreen_payload payload"),
                    }
                },
                _ => Err("Mismatched payload for TogglePaneIdFullscreen"),
            },
            Some(CommandName::TogglePaneEmbedOrEjectForPaneId) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::TogglePaneEmbedOrEjectForPaneIdPayload(
                        toggle_pane_embed_or_eject_payload,
                    )) => match toggle_pane_embed_or_eject_payload.pane_id {
                        Some(pane_id) => Ok(PluginCommand::TogglePaneEmbedOrEjectForPaneId(
                            pane_id.try_into()?,
                        )),
                        _ => Err("Malformed toggle_pane_embed_or_eject_payload payload"),
                    },
                    _ => Err("Mismatched payload for TogglePaneEmbedOrEjectForPaneId"),
                }
            },
            Some(CommandName::CloseTabWithIndex) => match protobuf_plugin_command.payload {
                Some(Payload::CloseTabWithIndexPayload(close_tab_index_payload)) => Ok(
                    PluginCommand::CloseTabWithIndex(close_tab_index_payload.tab_index as usize),
                ),
                _ => Err("Mismatched payload for CloseTabWithIndex"),
            },
            Some(CommandName::BreakPanesToNewTab) => match protobuf_plugin_command.payload {
                Some(Payload::BreakPanesToNewTabPayload(break_panes_to_new_tab_payload)) => {
                    Ok(PluginCommand::BreakPanesToNewTab(
                        break_panes_to_new_tab_payload
                            .pane_ids
                            .into_iter()
                            .filter_map(|p_id| p_id.try_into().ok())
                            .collect(),
                        break_panes_to_new_tab_payload.new_tab_name,
                        break_panes_to_new_tab_payload.should_change_focus_to_new_tab,
                    ))
                },
                _ => Err("Mismatched payload for BreakPanesToNewTab"),
            },
            Some(CommandName::BreakPanesToTabWithIndex) => match protobuf_plugin_command.payload {
                Some(Payload::BreakPanesToTabWithIndexPayload(
                    break_panes_to_tab_with_index_payload,
                )) => Ok(PluginCommand::BreakPanesToTabWithIndex(
                    break_panes_to_tab_with_index_payload
                        .pane_ids
                        .into_iter()
                        .filter_map(|p_id| p_id.try_into().ok())
                        .collect(),
                    break_panes_to_tab_with_index_payload.tab_index as usize,
                    break_panes_to_tab_with_index_payload.should_change_focus_to_target_tab,
                )),
                _ => Err("Mismatched payload for BreakPanesToTabWithIndex"),
            },
            Some(CommandName::ReloadPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::ReloadPluginPayload(reload_plugin_payload)) => {
                    Ok(PluginCommand::ReloadPlugin(reload_plugin_payload.plugin_id))
                },
                _ => Err("Mismatched payload for ReloadPlugin"),
            },
            Some(CommandName::LoadNewPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::LoadNewPluginPayload(load_new_plugin_payload)) => {
                    Ok(PluginCommand::LoadNewPlugin {
                        url: load_new_plugin_payload.plugin_url,
                        config: load_new_plugin_payload
                            .plugin_config
                            .into_iter()
                            .map(|e| (e.name, e.value))
                            .collect(),
                        load_in_background: load_new_plugin_payload
                            .should_load_plugin_in_background,
                        skip_plugin_cache: load_new_plugin_payload.should_skip_plugin_cache,
                    })
                },
                _ => Err("Mismatched payload for LoadNewPlugin"),
            },
            Some(CommandName::RebindKeys) => match protobuf_plugin_command.payload {
                Some(Payload::RebindKeysPayload(rebind_keys_payload)) => {
                    Ok(PluginCommand::RebindKeys {
                        keys_to_rebind: rebind_keys_payload
                            .keys_to_rebind
                            .into_iter()
                            .filter_map(|k| key_to_rebind_to_plugin_command_assets(k))
                            .collect(),
                        keys_to_unbind: rebind_keys_payload
                            .keys_to_unbind
                            .into_iter()
                            .filter_map(|k| key_to_unbind_to_plugin_command_assets(k))
                            .collect(),
                        write_config_to_disk: rebind_keys_payload.write_config_to_disk,
                    })
                },
                _ => Err("Mismatched payload for RebindKeys"),
            },
            Some(CommandName::ListClients) => match protobuf_plugin_command.payload {
                Some(_) => Err("ListClients should have no payload, found a payload"),
                None => Ok(PluginCommand::ListClients),
            },
            Some(CommandName::ChangeHostFolder) => match protobuf_plugin_command.payload {
                Some(Payload::ChangeHostFolderPayload(change_host_folder_payload)) => {
                    Ok(PluginCommand::ChangeHostFolder(PathBuf::from(
                        change_host_folder_payload.new_host_folder,
                    )))
                },
                _ => Err("Mismatched payload for ChangeHostFolder"),
            },
            Some(CommandName::SetFloatingPanePinned) => match protobuf_plugin_command.payload {
                Some(Payload::SetFloatingPanePinnedPayload(set_floating_pane_pinned_payload)) => {
                    match set_floating_pane_pinned_payload
                        .pane_id
                        .and_then(|p| p.try_into().ok())
                    {
                        Some(pane_id) => Ok(PluginCommand::SetFloatingPanePinned(
                            pane_id,
                            set_floating_pane_pinned_payload.should_be_pinned,
                        )),
                        None => Err("PaneId not found!"),
                    }
                },
                _ => Err("Mismatched payload for SetFloatingPanePinned"),
            },
            Some(CommandName::StackPanes) => match protobuf_plugin_command.payload {
                Some(Payload::StackPanesPayload(stack_panes_payload)) => {
                    Ok(PluginCommand::StackPanes(
                        stack_panes_payload
                            .pane_ids
                            .into_iter()
                            .filter_map(|p_id| p_id.try_into().ok())
                            .collect(),
                    ))
                },
                _ => Err("Mismatched payload for StackPanes"),
            },
            Some(CommandName::ChangeFloatingPanesCoordinates) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::ChangeFloatingPanesCoordinatesPayload(
                        change_floating_panes_coordinates_payload,
                    )) => Ok(PluginCommand::ChangeFloatingPanesCoordinates(
                        change_floating_panes_coordinates_payload
                            .pane_ids_and_floating_panes_coordinates
                            .into_iter()
                            .filter_map(|p_id_a_fp| {
                                let pane_id: PaneId = p_id_a_fp.pane_id?.try_into().ok()?;
                                let floating_pane_coordinates: FloatingPaneCoordinates =
                                    p_id_a_fp.floating_pane_coordinates?.try_into().ok()?;
                                Some((pane_id, floating_pane_coordinates))
                            })
                            .collect(),
                    )),
                    _ => Err("Mismatched payload for ChangeFloatingPanesCoordinates"),
                }
            },
            Some(CommandName::OpenCommandPaneNearPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::OpenCommandPaneNearPluginPayload(command_to_run_payload)) => {
                    match command_to_run_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> = command_to_run_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenCommandPaneNearPlugin(
                                command_to_run.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane near plugin payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenCommandPaneNearPlugin"),
            },
            Some(CommandName::OpenTerminalNearPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::OpenTerminalNearPluginPayload(open_terminal_near_plugin_payload)) => {
                    match open_terminal_near_plugin_payload.file_to_open {
                        Some(file_to_open) => Ok(PluginCommand::OpenTerminalNearPlugin(
                            file_to_open.try_into()?,
                        )),
                        None => Err("Malformed open terminal near plugin payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenTerminalNearPluginPayload"),
            },
            Some(CommandName::OpenTerminalFloatingNearPlugin) => match protobuf_plugin_command
                .payload
            {
                Some(Payload::OpenTerminalFloatingNearPluginPayload(
                    open_terminal_floating_near_plugin_payload,
                )) => {
                    let floating_pane_coordinates = open_terminal_floating_near_plugin_payload
                        .floating_pane_coordinates
                        .map(|f| f.into());
                    match open_terminal_floating_near_plugin_payload.file_to_open {
                        Some(file_to_open) => Ok(PluginCommand::OpenTerminalFloatingNearPlugin(
                            file_to_open.try_into()?,
                            floating_pane_coordinates,
                        )),
                        None => Err("Malformed open terminal floating near plugin payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenTerminalFloatingNearPlugin"),
            },
            Some(CommandName::OpenTerminalInPlaceOfPlugin) => match protobuf_plugin_command.payload
            {
                Some(Payload::OpenTerminalInPlaceOfPluginPayload(
                    open_terminal_in_place_of_plugin_payload,
                )) => match open_terminal_in_place_of_plugin_payload.file_to_open {
                    Some(file_to_open) => Ok(PluginCommand::OpenTerminalInPlaceOfPlugin(
                        file_to_open.try_into()?,
                        open_terminal_in_place_of_plugin_payload.close_plugin_after_replace,
                    )),
                    None => Err("Malformed open terminal in place of plugin payload"),
                },
                _ => Err("Mismatched payload for OpenTerminalInPlaceOfPlugin"),
            },
            Some(CommandName::OpenCommandPaneFloatingNearPlugin) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::OpenCommandPaneFloatingNearPluginPayload(
                        open_command_pane_floating_near_plugin,
                    )) => match open_command_pane_floating_near_plugin.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> =
                                open_command_pane_floating_near_plugin
                                    .context
                                    .into_iter()
                                    .map(|e| (e.name, e.value))
                                    .collect();
                            let floating_pane_coordinates = open_command_pane_floating_near_plugin
                                .floating_pane_coordinates
                                .map(|f| f.into());
                            Ok(PluginCommand::OpenCommandPaneFloatingNearPlugin(
                                command_to_run.try_into()?,
                                floating_pane_coordinates,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane floating near plugin payload"),
                    },
                    _ => Err("Mismatched payload for OpenCommandPaneFloatingNearPlugin"),
                }
            },
            Some(CommandName::OpenCommandPaneInPlaceOfPlugin) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::OpenCommandPaneInPlaceOfPluginPayload(
                        open_command_pane_in_place_of_plugin_payload,
                    )) => match open_command_pane_in_place_of_plugin_payload.command_to_run {
                        Some(command_to_run) => {
                            let context: BTreeMap<String, String> =
                                open_command_pane_in_place_of_plugin_payload
                                    .context
                                    .into_iter()
                                    .map(|e| (e.name, e.value))
                                    .collect();
                            Ok(PluginCommand::OpenCommandPaneInPlaceOfPlugin(
                                command_to_run.try_into()?,
                                open_command_pane_in_place_of_plugin_payload
                                    .close_plugin_after_replace,
                                context,
                            ))
                        },
                        None => Err("Malformed open command pane in place of plugin payload"),
                    },
                    _ => Err("Mismatched payload for OpenCommandPaneInPlaceOfPlugin"),
                }
            },
            Some(CommandName::OpenFileNearPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::OpenFileNearPluginPayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            let context: BTreeMap<String, String> = file_to_open_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenFileNearPlugin(
                                file_to_open.try_into()?,
                                context,
                            ))
                        },
                        None => Err("Malformed open file payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenFileNearPlugin"),
            },
            Some(CommandName::OpenFileFloatingNearPlugin) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::OpenFileFloatingNearPluginPayload(file_to_open_payload)) => {
                        let floating_pane_coordinates = file_to_open_payload
                            .floating_pane_coordinates
                            .map(|f| f.into());
                        let context: BTreeMap<String, String> = file_to_open_payload
                            .context
                            .into_iter()
                            .map(|e| (e.name, e.value))
                            .collect();
                        match file_to_open_payload.file_to_open {
                            Some(file_to_open) => Ok(PluginCommand::OpenFileFloatingNearPlugin(
                                file_to_open.try_into()?,
                                floating_pane_coordinates,
                                context,
                            )),
                            None => Err("Malformed open file payload"),
                        }
                    },
                    _ => Err("Mismatched payload for OpenFileFloatingNearPlugin"),
                }
            },
            Some(CommandName::OpenFileInPlaceOfPlugin) => match protobuf_plugin_command.payload {
                Some(Payload::OpenFileInPlaceOfPluginPayload(file_to_open_payload)) => {
                    match file_to_open_payload.file_to_open {
                        Some(file_to_open) => {
                            let context: BTreeMap<String, String> = file_to_open_payload
                                .context
                                .into_iter()
                                .map(|e| (e.name, e.value))
                                .collect();
                            Ok(PluginCommand::OpenFileInPlaceOfPlugin(
                                file_to_open.try_into()?,
                                file_to_open_payload.close_plugin_after_replace,
                                context,
                            ))
                        },
                        None => Err("Malformed open file in place payload"),
                    }
                },
                _ => Err("Mismatched payload for OpenFileInPlaceOfPlugin"),
            },
            Some(CommandName::StartWebServer) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("StartWebServer should not have a payload")
                } else {
                    Ok(PluginCommand::StartWebServer)
                }
            },
            Some(CommandName::StopWebServer) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("StopWebServer should not have a payload")
                } else {
                    Ok(PluginCommand::StopWebServer)
                }
            },
            Some(CommandName::QueryWebServerStatus) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("QueryWebServerStatus should not have a payload")
                } else {
                    Ok(PluginCommand::QueryWebServerStatus)
                }
            },
            Some(CommandName::GroupAndUngroupPanes) => match protobuf_plugin_command.payload {
                Some(Payload::GroupAndUngroupPanesPayload(group_and_ungroup_panes_payload)) => {
                    Ok(PluginCommand::GroupAndUngroupPanes(
                        group_and_ungroup_panes_payload
                            .pane_ids_to_group
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                        group_and_ungroup_panes_payload
                            .pane_ids_to_ungroup
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                        group_and_ungroup_panes_payload.for_all_clients,
                    ))
                },
                _ => Err("Mismatched payload for GroupAndUngroupPanes"),
            },
            Some(CommandName::HighlightAndUnhighlightPanes) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::HighlightAndUnhighlightPanesPayload(
                        highlight_and_unhighlight_panes_payload,
                    )) => Ok(PluginCommand::HighlightAndUnhighlightPanes(
                        highlight_and_unhighlight_panes_payload
                            .pane_ids_to_highlight
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                        highlight_and_unhighlight_panes_payload
                            .pane_ids_to_unhighlight
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                    )),
                    _ => Err("Mismatched payload for HighlightAndUnhighlightPanes"),
                }
            },
            Some(CommandName::CloseMultiplePanes) => match protobuf_plugin_command.payload {
                Some(Payload::CloseMultiplePanesPayload(close_multiple_panes_payload)) => {
                    Ok(PluginCommand::CloseMultiplePanes(
                        close_multiple_panes_payload
                            .pane_ids
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                    ))
                },
                _ => Err("Mismatched payload for CloseMultiplePanes"),
            },
            Some(CommandName::FloatMultiplePanes) => match protobuf_plugin_command.payload {
                Some(Payload::FloatMultiplePanesPayload(float_multiple_panes_payload)) => {
                    Ok(PluginCommand::FloatMultiplePanes(
                        float_multiple_panes_payload
                            .pane_ids
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                    ))
                },
                _ => Err("Mismatched payload for FloatMultiplePanes"),
            },
            Some(CommandName::EmbedMultiplePanes) => match protobuf_plugin_command.payload {
                Some(Payload::EmbedMultiplePanesPayload(embed_multiple_panes_payload)) => {
                    Ok(PluginCommand::EmbedMultiplePanes(
                        embed_multiple_panes_payload
                            .pane_ids
                            .into_iter()
                            .filter_map(|p| p.try_into().ok())
                            .collect(),
                    ))
                },
                _ => Err("Mismatched payload for EmbedMultiplePanes"),
            },
            Some(CommandName::ShareCurrentSession) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("ShareCurrentSession should not have a payload")
                } else {
                    Ok(PluginCommand::ShareCurrentSession)
                }
            },
            Some(CommandName::StopSharingCurrentSession) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("StopSharingCurrentSession should not have a payload")
                } else {
                    Ok(PluginCommand::StopSharingCurrentSession)
                }
            },
            Some(CommandName::SetSelfMouseSelectionSupport) => {
                match protobuf_plugin_command.payload {
                    Some(Payload::SetSelfMouseSelectionSupportPayload(
                        set_self_mouse_selection_support_payload,
                    )) => Ok(PluginCommand::SetSelfMouseSelectionSupport(
                        set_self_mouse_selection_support_payload.support_mouse_selection,
                    )),
                    _ => Err("SetSelfMouseSelectionSupport requires a payload"),
                }
            },
            Some(CommandName::GenerateWebLoginToken) => match protobuf_plugin_command.payload {
                Some(Payload::GenerateWebLoginTokenPayload(generate_web_login_token_payload)) => {
                    Ok(PluginCommand::GenerateWebLoginToken(
                        generate_web_login_token_payload.token_label,
                    ))
                },
                _ => Err("GenerateWebLoginToken requires a payload"),
            },
            Some(CommandName::RevokeWebLoginToken) => match protobuf_plugin_command.payload {
                Some(Payload::RevokeWebLoginTokenPayload(revoke_web_login_token_payload)) => Ok(
                    PluginCommand::RevokeWebLoginToken(revoke_web_login_token_payload.token_label),
                ),
                _ => Err("RevokeWebLoginToken requires a payload"),
            },
            Some(CommandName::ListWebLoginTokens) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("ListWebLoginTokens should not have a payload")
                } else {
                    Ok(PluginCommand::ListWebLoginTokens)
                }
            },
            Some(CommandName::RevokeAllWebLoginTokens) => {
                if protobuf_plugin_command.payload.is_some() {
                    Err("RevokeAllWebLoginTokens should not have a payload")
                } else {
                    Ok(PluginCommand::RevokeAllWebLoginTokens)
                }
            },
            Some(CommandName::RenameWebLoginToken) => match protobuf_plugin_command.payload {
                Some(Payload::RenameWebLoginTokenPayload(rename_web_login_token_payload)) => {
                    Ok(PluginCommand::RenameWebLoginToken(
                        rename_web_login_token_payload.old_name,
                        rename_web_login_token_payload.new_name,
                    ))
                },
                _ => Err("RenameWebLoginToken requires a payload"),
            },
            Some(CommandName::InterceptKeyPresses) => match protobuf_plugin_command.payload {
                Some(_) => Err("InterceptKeyPresses should have no payload, found a payload"),
                None => Ok(PluginCommand::InterceptKeyPresses),
            },
            Some(CommandName::ClearKeyPressesIntercepts) => match protobuf_plugin_command.payload {
                Some(_) => Err("ClearKeyPressesIntercepts should have no payload, found a payload"),
                None => Ok(PluginCommand::ClearKeyPressesIntercepts),
            },
            Some(CommandName::ReplacePaneWithExistingPane) => match protobuf_plugin_command.payload
            {
                Some(Payload::ReplacePaneWithExistingPanePayload(
                    replace_pane_with_other_pane_payload,
                )) => Ok(PluginCommand::ReplacePaneWithExistingPane(
                    replace_pane_with_other_pane_payload
                        .pane_id_to_replace
                        .and_then(|p_id| PaneId::try_from(p_id).ok())
                        .ok_or("Failed to parse ReplacePaneWithExistingPanePayload")?,
                    replace_pane_with_other_pane_payload
                        .existing_pane_id
                        .and_then(|p_id| PaneId::try_from(p_id).ok())
                        .ok_or("Failed to parse ReplacePaneWithExistingPanePayload")?,
                )),
                _ => Err("Mismatched payload for ReplacePaneWithExistingPane"),
            },
            None => Err("Unrecognized plugin command"),
        }
    }
}

impl TryFrom<PluginCommand> for ProtobufPluginCommand {
    type Error = &'static str;
    fn try_from(plugin_command: PluginCommand) -> Result<Self, &'static str> {
        match plugin_command {
            PluginCommand::Subscribe(subscriptions) => {
                let subscriptions: ProtobufEventNameList = subscriptions.try_into()?;
                Ok(ProtobufPluginCommand {
                    name: CommandName::Subscribe as i32,
                    payload: Some(Payload::SubscribePayload(SubscribePayload {
                        subscriptions: Some(subscriptions),
                    })),
                })
            },
            PluginCommand::Unsubscribe(subscriptions) => {
                let subscriptions: ProtobufEventNameList = subscriptions.try_into()?;
                Ok(ProtobufPluginCommand {
                    name: CommandName::Unsubscribe as i32,
                    payload: Some(Payload::UnsubscribePayload(UnsubscribePayload {
                        subscriptions: Some(subscriptions),
                    })),
                })
            },
            PluginCommand::SetSelectable(should_be_selectable) => Ok(ProtobufPluginCommand {
                name: CommandName::SetSelectable as i32,
                payload: Some(Payload::SetSelectablePayload(should_be_selectable)),
            }),
            PluginCommand::GetPluginIds => Ok(ProtobufPluginCommand {
                name: CommandName::GetPluginIds as i32,
                payload: None,
            }),
            PluginCommand::GetZellijVersion => Ok(ProtobufPluginCommand {
                name: CommandName::GetZellijVersion as i32,
                payload: None,
            }),
            PluginCommand::OpenFile(file_to_open, context) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenFile as i32,
                payload: Some(Payload::OpenFilePayload(OpenFilePayload {
                    file_to_open: Some(file_to_open.try_into()?),
                    floating_pane_coordinates: None,
                    context: context
                        .into_iter()
                        .map(|(name, value)| ContextItem { name, value })
                        .collect(),
                })),
            }),
            PluginCommand::OpenFileFloating(file_to_open, floating_pane_coordinates, context) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenFileFloating as i32,
                    payload: Some(Payload::OpenFileFloatingPayload(OpenFilePayload {
                        file_to_open: Some(file_to_open.try_into()?),
                        floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                        context: context
                            .into_iter()
                            .map(|(name, value)| ContextItem { name, value })
                            .collect(),
                    })),
                })
            },
            PluginCommand::OpenTerminal(cwd) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenTerminal as i32,
                payload: Some(Payload::OpenTerminalPayload(OpenFilePayload {
                    file_to_open: Some(cwd.try_into()?),
                    floating_pane_coordinates: None,
                    context: vec![], // will be added in the future
                })),
            }),
            PluginCommand::OpenTerminalFloating(cwd, floating_pane_coordinates) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenTerminalFloating as i32,
                    payload: Some(Payload::OpenTerminalFloatingPayload(OpenFilePayload {
                        file_to_open: Some(cwd.try_into()?),
                        floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                        context: vec![], // will be added in the future
                    })),
                })
            },
            PluginCommand::OpenCommandPane(command_to_run, context) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPane as i32,
                    payload: Some(Payload::OpenCommandPanePayload(OpenCommandPanePayload {
                        command_to_run: Some(command_to_run.try_into()?),
                        floating_pane_coordinates: None,
                        context,
                    })),
                })
            },
            PluginCommand::OpenCommandPaneFloating(
                command_to_run,
                floating_pane_coordinates,
                context,
            ) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPaneFloating as i32,
                    payload: Some(Payload::OpenCommandPaneFloatingPayload(
                        OpenCommandPanePayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                            context,
                        },
                    )),
                })
            },
            PluginCommand::SwitchTabTo(tab_index) => Ok(ProtobufPluginCommand {
                name: CommandName::SwitchTabTo as i32,
                payload: Some(Payload::SwitchTabToPayload(SwitchTabToPayload {
                    tab_index: tab_index,
                })),
            }),
            PluginCommand::SetTimeout(seconds) => Ok(ProtobufPluginCommand {
                name: CommandName::SetTimeout as i32,
                payload: Some(Payload::SetTimeoutPayload(SetTimeoutPayload { seconds })),
            }),
            PluginCommand::ExecCmd(command_line) => Ok(ProtobufPluginCommand {
                name: CommandName::ExecCmd as i32,
                payload: Some(Payload::ExecCmdPayload(ExecCmdPayload { command_line })),
            }),
            PluginCommand::PostMessageTo(plugin_message) => Ok(ProtobufPluginCommand {
                name: CommandName::PostMessageTo as i32,
                payload: Some(Payload::PostMessageToPayload(PluginMessagePayload {
                    message: Some(plugin_message.try_into()?),
                })),
            }),
            PluginCommand::PostMessageToPlugin(plugin_message) => Ok(ProtobufPluginCommand {
                name: CommandName::PostMessageToPlugin as i32,
                payload: Some(Payload::PostMessageToPluginPayload(PluginMessagePayload {
                    message: Some(plugin_message.try_into()?),
                })),
            }),
            PluginCommand::HideSelf => Ok(ProtobufPluginCommand {
                name: CommandName::HideSelf as i32,
                payload: None,
            }),
            PluginCommand::ShowSelf(should_float_if_hidden) => Ok(ProtobufPluginCommand {
                name: CommandName::ShowSelf as i32,
                payload: Some(Payload::ShowSelfPayload(should_float_if_hidden)),
            }),
            PluginCommand::SwitchToMode(input_mode) => Ok(ProtobufPluginCommand {
                name: CommandName::SwitchToMode as i32,
                payload: Some(Payload::SwitchToModePayload(SwitchToModePayload {
                    input_mode: ProtobufInputMode::try_from(input_mode)? as i32,
                })),
            }),
            PluginCommand::NewTabsWithLayout(raw_layout) => Ok(ProtobufPluginCommand {
                name: CommandName::NewTabsWithLayout as i32,
                payload: Some(Payload::NewTabsWithLayoutPayload(raw_layout)),
            }),
            PluginCommand::NewTab { name, cwd } => Ok(ProtobufPluginCommand {
                name: CommandName::NewTab as i32,
                payload: Some(Payload::NewTabPayload(NewTabPayload { name, cwd })),
            }),
            PluginCommand::GoToNextTab => Ok(ProtobufPluginCommand {
                name: CommandName::GoToNextTab as i32,
                payload: None,
            }),
            PluginCommand::GoToPreviousTab => Ok(ProtobufPluginCommand {
                name: CommandName::GoToPreviousTab as i32,
                payload: None,
            }),
            PluginCommand::Resize(resize) => Ok(ProtobufPluginCommand {
                name: CommandName::Resize as i32,
                payload: Some(Payload::ResizePayload(ResizePayload {
                    resize: Some(resize.try_into()?),
                })),
            }),
            PluginCommand::ResizeWithDirection(resize) => Ok(ProtobufPluginCommand {
                name: CommandName::ResizeWithDirection as i32,
                payload: Some(Payload::ResizeWithDirectionPayload(ResizePayload {
                    resize: Some(resize.try_into()?),
                })),
            }),
            PluginCommand::FocusNextPane => Ok(ProtobufPluginCommand {
                name: CommandName::FocusNextPane as i32,
                payload: None,
            }),
            PluginCommand::FocusPreviousPane => Ok(ProtobufPluginCommand {
                name: CommandName::FocusPreviousPane as i32,
                payload: None,
            }),
            PluginCommand::MoveFocus(direction) => Ok(ProtobufPluginCommand {
                name: CommandName::MoveFocus as i32,
                payload: Some(Payload::MoveFocusPayload(MovePayload {
                    direction: Some(direction.try_into()?),
                })),
            }),
            PluginCommand::MoveFocusOrTab(direction) => Ok(ProtobufPluginCommand {
                name: CommandName::MoveFocusOrTab as i32,
                payload: Some(Payload::MoveFocusOrTabPayload(MovePayload {
                    direction: Some(direction.try_into()?),
                })),
            }),
            PluginCommand::Detach => Ok(ProtobufPluginCommand {
                name: CommandName::Detach as i32,
                payload: None,
            }),
            PluginCommand::EditScrollback => Ok(ProtobufPluginCommand {
                name: CommandName::EditScrollback as i32,
                payload: None,
            }),
            PluginCommand::Write(bytes) => Ok(ProtobufPluginCommand {
                name: CommandName::Write as i32,
                payload: Some(Payload::WritePayload(bytes)),
            }),
            PluginCommand::WriteChars(chars) => Ok(ProtobufPluginCommand {
                name: CommandName::WriteChars as i32,
                payload: Some(Payload::WriteCharsPayload(chars)),
            }),
            PluginCommand::ToggleTab => Ok(ProtobufPluginCommand {
                name: CommandName::ToggleTab as i32,
                payload: None,
            }),
            PluginCommand::MovePane => Ok(ProtobufPluginCommand {
                name: CommandName::MovePane as i32,
                payload: None,
            }),
            PluginCommand::MovePaneWithDirection(direction) => Ok(ProtobufPluginCommand {
                name: CommandName::MovePaneWithDirection as i32,
                payload: Some(Payload::MovePaneWithDirectionPayload(MovePayload {
                    direction: Some(direction.try_into()?),
                })),
            }),
            PluginCommand::ClearScreen => Ok(ProtobufPluginCommand {
                name: CommandName::ClearScreen as i32,
                payload: None,
            }),
            PluginCommand::ScrollUp => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollUp as i32,
                payload: None,
            }),
            PluginCommand::ScrollDown => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollDown as i32,
                payload: None,
            }),
            PluginCommand::ScrollToTop => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollToTop as i32,
                payload: None,
            }),
            PluginCommand::ScrollToBottom => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollToBottom as i32,
                payload: None,
            }),
            PluginCommand::PageScrollUp => Ok(ProtobufPluginCommand {
                name: CommandName::PageScrollUp as i32,
                payload: None,
            }),
            PluginCommand::PageScrollDown => Ok(ProtobufPluginCommand {
                name: CommandName::PageScrollDown as i32,
                payload: None,
            }),
            PluginCommand::ToggleFocusFullscreen => Ok(ProtobufPluginCommand {
                name: CommandName::ToggleFocusFullscreen as i32,
                payload: None,
            }),
            PluginCommand::TogglePaneFrames => Ok(ProtobufPluginCommand {
                name: CommandName::TogglePaneFrames as i32,
                payload: None,
            }),
            PluginCommand::TogglePaneEmbedOrEject => Ok(ProtobufPluginCommand {
                name: CommandName::TogglePaneEmbedOrEject as i32,
                payload: None,
            }),
            PluginCommand::UndoRenamePane => Ok(ProtobufPluginCommand {
                name: CommandName::UndoRenamePane as i32,
                payload: None,
            }),
            PluginCommand::CloseFocus => Ok(ProtobufPluginCommand {
                name: CommandName::CloseFocus as i32,
                payload: None,
            }),
            PluginCommand::ToggleActiveTabSync => Ok(ProtobufPluginCommand {
                name: CommandName::ToggleActiveTabSync as i32,
                payload: None,
            }),
            PluginCommand::CloseFocusedTab => Ok(ProtobufPluginCommand {
                name: CommandName::CloseFocusedTab as i32,
                payload: None,
            }),
            PluginCommand::UndoRenameTab => Ok(ProtobufPluginCommand {
                name: CommandName::UndoRenameTab as i32,
                payload: None,
            }),
            PluginCommand::QuitZellij => Ok(ProtobufPluginCommand {
                name: CommandName::QuitZellij as i32,
                payload: None,
            }),
            PluginCommand::PreviousSwapLayout => Ok(ProtobufPluginCommand {
                name: CommandName::PreviousSwapLayout as i32,
                payload: None,
            }),
            PluginCommand::NextSwapLayout => Ok(ProtobufPluginCommand {
                name: CommandName::NextSwapLayout as i32,
                payload: None,
            }),
            PluginCommand::GoToTabName(tab_name) => Ok(ProtobufPluginCommand {
                name: CommandName::GoToTabName as i32,
                payload: Some(Payload::GoToTabNamePayload(tab_name)),
            }),
            PluginCommand::FocusOrCreateTab(tab_name) => Ok(ProtobufPluginCommand {
                name: CommandName::FocusOrCreateTab as i32,
                payload: Some(Payload::FocusOrCreateTabPayload(tab_name)),
            }),
            PluginCommand::GoToTab(tab_index) => Ok(ProtobufPluginCommand {
                name: CommandName::GoToTab as i32,
                payload: Some(Payload::GoToTabPayload(tab_index)),
            }),
            PluginCommand::StartOrReloadPlugin(url) => Ok(ProtobufPluginCommand {
                name: CommandName::StartOrReloadPlugin as i32,
                payload: Some(Payload::StartOrReloadPluginPayload(url)),
            }),
            PluginCommand::CloseTerminalPane(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::CloseTerminalPane as i32,
                payload: Some(Payload::CloseTerminalPanePayload(pane_id)),
            }),
            PluginCommand::ClosePluginPane(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ClosePluginPane as i32,
                payload: Some(Payload::ClosePluginPanePayload(pane_id)),
            }),
            PluginCommand::FocusTerminalPane(pane_id, should_float_if_hidden) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::FocusTerminalPane as i32,
                    payload: Some(Payload::FocusTerminalPanePayload(PaneIdAndShouldFloat {
                        pane_id: pane_id,
                        should_float: should_float_if_hidden,
                    })),
                })
            },
            PluginCommand::FocusPluginPane(pane_id, should_float_if_hidden) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::FocusPluginPane as i32,
                    payload: Some(Payload::FocusPluginPanePayload(PaneIdAndShouldFloat {
                        pane_id: pane_id,
                        should_float: should_float_if_hidden,
                    })),
                })
            },
            PluginCommand::RenameTerminalPane(pane_id, new_name) => Ok(ProtobufPluginCommand {
                name: CommandName::RenameTerminalPane as i32,
                payload: Some(Payload::RenameTerminalPanePayload(IdAndNewName {
                    id: pane_id,
                    new_name,
                })),
            }),
            PluginCommand::RenamePluginPane(pane_id, new_name) => Ok(ProtobufPluginCommand {
                name: CommandName::RenamePluginPane as i32,
                payload: Some(Payload::RenamePluginPanePayload(IdAndNewName {
                    id: pane_id,
                    new_name,
                })),
            }),
            PluginCommand::RenameTab(tab_index, new_name) => Ok(ProtobufPluginCommand {
                name: CommandName::RenameTab as i32,
                payload: Some(Payload::RenameTabPayload(IdAndNewName {
                    id: tab_index,
                    new_name,
                })),
            }),
            PluginCommand::ReportPanic(payload) => Ok(ProtobufPluginCommand {
                name: CommandName::ReportCrash as i32,
                payload: Some(Payload::ReportCrashPayload(payload)),
            }),
            PluginCommand::RequestPluginPermissions(permissions) => Ok(ProtobufPluginCommand {
                name: CommandName::RequestPluginPermissions as i32,
                payload: Some(Payload::RequestPluginPermissionPayload(
                    RequestPluginPermissionPayload {
                        permissions: permissions
                            .iter()
                            .filter_map(|p| ProtobufPermissionType::try_from(*p).ok())
                            .map(|p| p as i32)
                            .collect(),
                    },
                )),
            }),
            PluginCommand::SwitchSession(switch_to_session) => Ok(ProtobufPluginCommand {
                name: CommandName::SwitchSession as i32,
                payload: Some(Payload::SwitchSessionPayload(SwitchSessionPayload {
                    name: switch_to_session.name,
                    tab_position: switch_to_session.tab_position.map(|t| t as u32),
                    pane_id: switch_to_session.pane_id.map(|p| p.0),
                    pane_id_is_plugin: switch_to_session.pane_id.map(|p| p.1),
                    layout: switch_to_session.layout.and_then(|l| l.try_into().ok()),
                    cwd: switch_to_session.cwd.map(|c| c.display().to_string()),
                })),
            }),
            PluginCommand::OpenTerminalInPlace(cwd) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenTerminalInPlace as i32,
                payload: Some(Payload::OpenTerminalInPlacePayload(OpenFilePayload {
                    file_to_open: Some(cwd.try_into()?),
                    floating_pane_coordinates: None,
                    context: vec![], // will be added in the future
                })),
            }),
            PluginCommand::OpenFileInPlace(file_to_open, context) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenFileInPlace as i32,
                payload: Some(Payload::OpenFileInPlacePayload(OpenFilePayload {
                    file_to_open: Some(file_to_open.try_into()?),
                    floating_pane_coordinates: None,
                    context: context
                        .into_iter()
                        .map(|(name, value)| ContextItem { name, value })
                        .collect(),
                })),
            }),
            PluginCommand::OpenCommandPaneInPlace(command_to_run, context) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandInPlace as i32,
                    payload: Some(Payload::OpenCommandPaneInPlacePayload(
                        OpenCommandPanePayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            floating_pane_coordinates: None,
                            context,
                        },
                    )),
                })
            },
            PluginCommand::RunCommand(command_line, env_variables, cwd, context) => {
                let env_variables: Vec<_> = env_variables
                    .into_iter()
                    .map(|(name, value)| EnvVariable { name, value })
                    .collect();
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                let cwd = cwd.display().to_string();
                Ok(ProtobufPluginCommand {
                    name: CommandName::RunCommand as i32,
                    payload: Some(Payload::RunCommandPayload(RunCommandPayload {
                        command_line,
                        env_variables,
                        cwd,
                        context,
                    })),
                })
            },
            PluginCommand::WebRequest(url, verb, headers, body, context) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                let headers: Vec<_> = headers
                    .into_iter()
                    .map(|(name, value)| Header { name, value })
                    .collect();
                let verb: ProtobufHttpVerb = verb.into();
                Ok(ProtobufPluginCommand {
                    name: CommandName::WebRequest as i32,
                    payload: Some(Payload::WebRequestPayload(WebRequestPayload {
                        url,
                        verb: verb as i32,
                        body,
                        headers,
                        context,
                    })),
                })
            },
            PluginCommand::DeleteDeadSession(dead_session_name) => Ok(ProtobufPluginCommand {
                name: CommandName::DeleteDeadSession as i32,
                payload: Some(Payload::DeleteDeadSessionPayload(dead_session_name)),
            }),
            PluginCommand::DeleteAllDeadSessions => Ok(ProtobufPluginCommand {
                name: CommandName::DeleteAllDeadSessions as i32,
                payload: None,
            }),
            PluginCommand::RenameSession(new_session_name) => Ok(ProtobufPluginCommand {
                name: CommandName::RenameSession as i32,
                payload: Some(Payload::RenameSessionPayload(new_session_name)),
            }),
            PluginCommand::UnblockCliPipeInput(pipe_name) => Ok(ProtobufPluginCommand {
                name: CommandName::UnblockCliPipeInput as i32,
                payload: Some(Payload::UnblockCliPipeInputPayload(pipe_name)),
            }),
            PluginCommand::BlockCliPipeInput(pipe_name) => Ok(ProtobufPluginCommand {
                name: CommandName::BlockCliPipeInput as i32,
                payload: Some(Payload::BlockCliPipeInputPayload(pipe_name)),
            }),
            PluginCommand::CliPipeOutput(pipe_name, output) => Ok(ProtobufPluginCommand {
                name: CommandName::CliPipeOutput as i32,
                payload: Some(Payload::CliPipeOutputPayload(CliPipeOutputPayload {
                    pipe_name,
                    output,
                })),
            }),
            PluginCommand::MessageToPlugin(message_to_plugin) => {
                let plugin_config: Vec<_> = message_to_plugin
                    .plugin_config
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                let message_args: Vec<_> = message_to_plugin
                    .message_args
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::MessageToPlugin as i32,
                    payload: Some(Payload::MessageToPluginPayload(MessageToPluginPayload {
                        plugin_url: message_to_plugin.plugin_url,
                        plugin_config,
                        message_name: message_to_plugin.message_name,
                        message_payload: message_to_plugin.message_payload,
                        message_args,
                        new_plugin_args: message_to_plugin.new_plugin_args.map(|m_t_p| {
                            ProtobufNewPluginArgs {
                                should_float: m_t_p.should_float,
                                pane_id_to_replace: m_t_p
                                    .pane_id_to_replace
                                    .and_then(|p_id| ProtobufPaneId::try_from(p_id).ok()),
                                pane_title: m_t_p.pane_title,
                                cwd: m_t_p.cwd.map(|cwd| cwd.display().to_string()),
                                skip_cache: m_t_p.skip_cache,
                                should_focus: m_t_p.should_focus,
                            }
                        }),
                        destination_plugin_id: message_to_plugin.destination_plugin_id,
                        floating_pane_coordinates: message_to_plugin
                            .floating_pane_coordinates
                            .and_then(|f| f.try_into().ok()),
                    })),
                })
            },
            PluginCommand::DisconnectOtherClients => Ok(ProtobufPluginCommand {
                name: CommandName::DisconnectOtherClients as i32,
                payload: None,
            }),
            PluginCommand::KillSessions(session_names) => Ok(ProtobufPluginCommand {
                name: CommandName::KillSessions as i32,
                payload: Some(Payload::KillSessionsPayload(KillSessionsPayload {
                    session_names,
                })),
            }),
            PluginCommand::ScanHostFolder(folder_to_scan) => Ok(ProtobufPluginCommand {
                name: CommandName::ScanHostFolder as i32,
                payload: Some(Payload::ScanHostFolderPayload(
                    folder_to_scan.display().to_string(),
                )),
            }),
            PluginCommand::WatchFilesystem => Ok(ProtobufPluginCommand {
                name: CommandName::WatchFilesystem as i32,
                payload: None,
            }),
            PluginCommand::DumpSessionLayout => Ok(ProtobufPluginCommand {
                name: CommandName::DumpSessionLayout as i32,
                payload: None,
            }),
            PluginCommand::CloseSelf => Ok(ProtobufPluginCommand {
                name: CommandName::CloseSelf as i32,
                payload: None,
            }),
            PluginCommand::NewTabsWithLayoutInfo(new_tabs_with_layout_info_payload) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::NewTabsWithLayoutInfo as i32,
                    payload: Some(Payload::NewTabsWithLayoutInfoPayload(
                        NewTabsWithLayoutInfoPayload {
                            layout_info: new_tabs_with_layout_info_payload.try_into().ok(),
                        },
                    )),
                })
            },
            PluginCommand::Reconfigure(config, write_to_disk) => Ok(ProtobufPluginCommand {
                name: CommandName::Reconfigure as i32,
                payload: Some(Payload::ReconfigurePayload(ReconfigurePayload {
                    config,
                    write_to_disk,
                })),
            }),
            PluginCommand::HidePaneWithId(pane_id_to_hide) => Ok(ProtobufPluginCommand {
                name: CommandName::HidePaneWithId as i32,
                payload: Some(Payload::HidePaneWithIdPayload(HidePaneWithIdPayload {
                    pane_id: ProtobufPaneId::try_from(pane_id_to_hide).ok(),
                })),
            }),
            PluginCommand::ShowPaneWithId(pane_id_to_show, should_float_if_hidden) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::ShowPaneWithId as i32,
                    payload: Some(Payload::ShowPaneWithIdPayload(ShowPaneWithIdPayload {
                        pane_id: ProtobufPaneId::try_from(pane_id_to_show).ok(),
                        should_float_if_hidden,
                    })),
                })
            },
            PluginCommand::OpenCommandPaneBackground(command_to_run, context) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPaneBackground as i32,
                    payload: Some(Payload::OpenCommandPaneBackgroundPayload(
                        OpenCommandPanePayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            floating_pane_coordinates: None,
                            context,
                        },
                    )),
                })
            },
            PluginCommand::RerunCommandPane(terminal_pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::RerunCommandPane as i32,
                payload: Some(Payload::RerunCommandPanePayload(RerunCommandPanePayload {
                    terminal_pane_id,
                })),
            }),
            PluginCommand::ResizePaneIdWithDirection(resize, pane_id) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::ResizePaneIdWithDirection as i32,
                    payload: Some(Payload::ResizePaneIdWithDirectionPayload(
                        ResizePaneIdWithDirectionPayload {
                            resize: Some(resize.try_into()?),
                            pane_id: Some(pane_id.try_into()?),
                        },
                    )),
                })
            },
            PluginCommand::EditScrollbackForPaneWithId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::EditScrollbackForPaneWithId as i32,
                payload: Some(Payload::EditScrollbackForPaneWithIdPayload(
                    EditScrollbackForPaneWithIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::WriteToPaneId(bytes_to_write, pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::WriteToPaneId as i32,
                payload: Some(Payload::WriteToPaneIdPayload(WriteToPaneIdPayload {
                    bytes_to_write,
                    pane_id: Some(pane_id.try_into()?),
                })),
            }),
            PluginCommand::WriteCharsToPaneId(chars_to_write, pane_id) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::WriteCharsToPaneId as i32,
                    payload: Some(Payload::WriteCharsToPaneIdPayload(
                        WriteCharsToPaneIdPayload {
                            chars_to_write,
                            pane_id: Some(pane_id.try_into()?),
                        },
                    )),
                })
            },
            PluginCommand::MovePaneWithPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::MovePaneWithPaneId as i32,
                payload: Some(Payload::MovePaneWithPaneIdPayload(
                    MovePaneWithPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::MovePaneWithPaneIdInDirection(pane_id, direction) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::MovePaneWithPaneIdInDirection as i32,
                    payload: Some(Payload::MovePaneWithPaneIdInDirectionPayload(
                        MovePaneWithPaneIdInDirectionPayload {
                            pane_id: Some(pane_id.try_into()?),
                            direction: Some(direction.try_into()?),
                        },
                    )),
                })
            },
            PluginCommand::ClearScreenForPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ClearScreenForPaneId as i32,
                payload: Some(Payload::ClearScreenForPaneIdPayload(
                    ClearScreenForPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::ScrollUpInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollUpInPaneId as i32,
                payload: Some(Payload::ScrollUpInPaneIdPayload(ScrollUpInPaneIdPayload {
                    pane_id: Some(pane_id.try_into()?),
                })),
            }),
            PluginCommand::ScrollDownInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollDownInPaneId as i32,
                payload: Some(Payload::ScrollDownInPaneIdPayload(
                    ScrollDownInPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::ScrollToTopInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollToTopInPaneId as i32,
                payload: Some(Payload::ScrollToTopInPaneIdPayload(
                    ScrollToTopInPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::ScrollToBottomInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ScrollToBottomInPaneId as i32,
                payload: Some(Payload::ScrollToBottomInPaneIdPayload(
                    ScrollToBottomInPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::PageScrollUpInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::PageScrollUpInPaneId as i32,
                payload: Some(Payload::PageScrollUpInPaneIdPayload(
                    PageScrollUpInPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::PageScrollDownInPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::PageScrollDownInPaneId as i32,
                payload: Some(Payload::PageScrollDownInPaneIdPayload(
                    PageScrollDownInPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::TogglePaneIdFullscreen(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::TogglePaneIdFullscreen as i32,
                payload: Some(Payload::TogglePaneIdFullscreenPayload(
                    TogglePaneIdFullscreenPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::TogglePaneEmbedOrEjectForPaneId(pane_id) => Ok(ProtobufPluginCommand {
                name: CommandName::TogglePaneEmbedOrEjectForPaneId as i32,
                payload: Some(Payload::TogglePaneEmbedOrEjectForPaneIdPayload(
                    TogglePaneEmbedOrEjectForPaneIdPayload {
                        pane_id: Some(pane_id.try_into()?),
                    },
                )),
            }),
            PluginCommand::CloseTabWithIndex(tab_index) => Ok(ProtobufPluginCommand {
                name: CommandName::CloseTabWithIndex as i32,
                payload: Some(Payload::CloseTabWithIndexPayload(
                    CloseTabWithIndexPayload {
                        tab_index: tab_index as u32,
                    },
                )),
            }),
            PluginCommand::BreakPanesToNewTab(
                pane_ids,
                new_tab_name,
                should_change_focus_to_new_tab,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::BreakPanesToNewTab as i32,
                payload: Some(Payload::BreakPanesToNewTabPayload(
                    BreakPanesToNewTabPayload {
                        pane_ids: pane_ids
                            .into_iter()
                            .filter_map(|p_id| p_id.try_into().ok())
                            .collect(),
                        should_change_focus_to_new_tab,
                        new_tab_name,
                    },
                )),
            }),
            PluginCommand::BreakPanesToTabWithIndex(
                pane_ids,
                tab_index,
                should_change_focus_to_target_tab,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::BreakPanesToTabWithIndex as i32,
                payload: Some(Payload::BreakPanesToTabWithIndexPayload(
                    BreakPanesToTabWithIndexPayload {
                        pane_ids: pane_ids
                            .into_iter()
                            .filter_map(|p_id| p_id.try_into().ok())
                            .collect(),
                        tab_index: tab_index as u32,
                        should_change_focus_to_target_tab,
                    },
                )),
            }),
            PluginCommand::ReloadPlugin(plugin_id) => Ok(ProtobufPluginCommand {
                name: CommandName::ReloadPlugin as i32,
                payload: Some(Payload::ReloadPluginPayload(ReloadPluginPayload {
                    plugin_id,
                })),
            }),
            PluginCommand::LoadNewPlugin {
                url,
                config,
                load_in_background,
                skip_plugin_cache,
            } => Ok(ProtobufPluginCommand {
                name: CommandName::LoadNewPlugin as i32,
                payload: Some(Payload::LoadNewPluginPayload(LoadNewPluginPayload {
                    plugin_url: url,
                    plugin_config: config
                        .into_iter()
                        .map(|(name, value)| ContextItem { name, value })
                        .collect(),
                    should_skip_plugin_cache: skip_plugin_cache,
                    should_load_plugin_in_background: load_in_background,
                })),
            }),
            PluginCommand::RebindKeys {
                keys_to_rebind,
                keys_to_unbind,
                write_config_to_disk,
            } => Ok(ProtobufPluginCommand {
                name: CommandName::RebindKeys as i32,
                payload: Some(Payload::RebindKeysPayload(RebindKeysPayload {
                    keys_to_rebind: keys_to_rebind
                        .into_iter()
                        .filter_map(|k| k.try_into().ok())
                        .collect(),
                    keys_to_unbind: keys_to_unbind
                        .into_iter()
                        .filter_map(|k| k.try_into().ok())
                        .collect(),
                    write_config_to_disk,
                })),
            }),
            PluginCommand::ListClients => Ok(ProtobufPluginCommand {
                name: CommandName::ListClients as i32,
                payload: None,
            }),
            PluginCommand::ChangeHostFolder(new_host_folder) => Ok(ProtobufPluginCommand {
                name: CommandName::ChangeHostFolder as i32,
                payload: Some(Payload::ChangeHostFolderPayload(ChangeHostFolderPayload {
                    new_host_folder: new_host_folder.display().to_string(), // TODO: not accurate?
                })),
            }),
            PluginCommand::SetFloatingPanePinned(pane_id, should_be_pinned) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::SetFloatingPanePinned as i32,
                    payload: Some(Payload::SetFloatingPanePinnedPayload(
                        SetFloatingPanePinnedPayload {
                            pane_id: pane_id.try_into().ok(),
                            should_be_pinned,
                        },
                    )),
                })
            },
            PluginCommand::StackPanes(pane_ids) => Ok(ProtobufPluginCommand {
                name: CommandName::StackPanes as i32,
                payload: Some(Payload::StackPanesPayload(StackPanesPayload {
                    pane_ids: pane_ids
                        .into_iter()
                        .filter_map(|p_id| p_id.try_into().ok())
                        .collect(),
                })),
            }),
            PluginCommand::ChangeFloatingPanesCoordinates(
                pane_ids_and_floating_panes_coordinates,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::ChangeFloatingPanesCoordinates as i32,
                payload: Some(Payload::ChangeFloatingPanesCoordinatesPayload(
                    ChangeFloatingPanesCoordinatesPayload {
                        pane_ids_and_floating_panes_coordinates:
                            pane_ids_and_floating_panes_coordinates
                                .into_iter()
                                .filter_map(|(p_id, floating_pane_coordinates)| {
                                    Some(PaneIdAndFloatingPaneCoordinates {
                                        pane_id: Some(p_id.try_into().ok()?),
                                        floating_pane_coordinates: Some(
                                            floating_pane_coordinates.try_into().ok()?,
                                        ),
                                    })
                                })
                                .collect(),
                    },
                )),
            }),
            PluginCommand::OpenCommandPaneNearPlugin(command_to_run, context) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPaneNearPlugin as i32,
                    payload: Some(Payload::OpenCommandPaneNearPluginPayload(
                        OpenCommandPaneNearPluginPayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            floating_pane_coordinates: None,
                            context,
                        },
                    )),
                })
            },
            PluginCommand::OpenCommandPaneFloatingNearPlugin(
                command_to_run,
                floating_pane_coordinates,
                context,
            ) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPaneFloatingNearPlugin as i32,
                    payload: Some(Payload::OpenCommandPaneFloatingNearPluginPayload(
                        OpenCommandPaneFloatingNearPluginPayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                            context,
                        },
                    )),
                })
            },
            PluginCommand::OpenTerminalNearPlugin(cwd) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenTerminalNearPlugin as i32,
                payload: Some(Payload::OpenTerminalNearPluginPayload(
                    OpenTerminalNearPluginPayload {
                        file_to_open: Some(cwd.try_into()?),
                        context: vec![], // will be added in the future
                    },
                )),
            }),
            PluginCommand::OpenTerminalFloatingNearPlugin(cwd, floating_pane_coordinates) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenTerminalFloatingNearPlugin as i32,
                    payload: Some(Payload::OpenTerminalFloatingNearPluginPayload(
                        OpenTerminalFloatingNearPluginPayload {
                            file_to_open: Some(cwd.try_into()?),
                            floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                            context: vec![], // will be added in the future
                        },
                    )),
                })
            },
            PluginCommand::OpenTerminalInPlaceOfPlugin(cwd, close_plugin_after_replace) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenTerminalInPlaceOfPlugin as i32,
                    payload: Some(Payload::OpenTerminalInPlaceOfPluginPayload(
                        OpenTerminalInPlaceOfPluginPayload {
                            file_to_open: Some(cwd.try_into()?),
                            close_plugin_after_replace,
                            context: vec![], // will be added in the future
                        },
                    )),
                })
            },
            PluginCommand::OpenCommandPaneInPlaceOfPlugin(
                command_to_run,
                close_plugin_after_replace,
                context,
            ) => {
                let context: Vec<_> = context
                    .into_iter()
                    .map(|(name, value)| ContextItem { name, value })
                    .collect();
                Ok(ProtobufPluginCommand {
                    name: CommandName::OpenCommandPaneInPlaceOfPlugin as i32,
                    payload: Some(Payload::OpenCommandPaneInPlaceOfPluginPayload(
                        OpenCommandPaneInPlaceOfPluginPayload {
                            command_to_run: Some(command_to_run.try_into()?),
                            close_plugin_after_replace,
                            context,
                        },
                    )),
                })
            },
            PluginCommand::OpenFileNearPlugin(file_to_open, context) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenFileNearPlugin as i32,
                payload: Some(Payload::OpenFileNearPluginPayload(
                    OpenFileNearPluginPayload {
                        file_to_open: Some(file_to_open.try_into()?),
                        floating_pane_coordinates: None,
                        context: context
                            .into_iter()
                            .map(|(name, value)| ContextItem { name, value })
                            .collect(),
                    },
                )),
            }),
            PluginCommand::OpenFileFloatingNearPlugin(
                file_to_open,
                floating_pane_coordinates,
                context,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenFileFloatingNearPlugin as i32,
                payload: Some(Payload::OpenFileFloatingNearPluginPayload(
                    OpenFileFloatingNearPluginPayload {
                        file_to_open: Some(file_to_open.try_into()?),
                        floating_pane_coordinates: floating_pane_coordinates.map(|f| f.into()),
                        context: context
                            .into_iter()
                            .map(|(name, value)| ContextItem { name, value })
                            .collect(),
                    },
                )),
            }),
            PluginCommand::OpenFileInPlaceOfPlugin(
                file_to_open,
                close_plugin_after_replace,
                context,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::OpenFileInPlaceOfPlugin as i32,
                payload: Some(Payload::OpenFileInPlaceOfPluginPayload(
                    OpenFileInPlaceOfPluginPayload {
                        file_to_open: Some(file_to_open.try_into()?),
                        floating_pane_coordinates: None,
                        close_plugin_after_replace,
                        context: context
                            .into_iter()
                            .map(|(name, value)| ContextItem { name, value })
                            .collect(),
                    },
                )),
            }),
            PluginCommand::GroupAndUngroupPanes(
                panes_to_group,
                panes_to_ungroup,
                for_all_clients,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::GroupAndUngroupPanes as i32,
                payload: Some(Payload::GroupAndUngroupPanesPayload(
                    GroupAndUngroupPanesPayload {
                        pane_ids_to_group: panes_to_group
                            .iter()
                            .filter_map(|&p| p.try_into().ok())
                            .collect(),
                        pane_ids_to_ungroup: panes_to_ungroup
                            .iter()
                            .filter_map(|&p| p.try_into().ok())
                            .collect(),
                        for_all_clients,
                    },
                )),
            }),
            PluginCommand::StartWebServer => Ok(ProtobufPluginCommand {
                name: CommandName::StartWebServer as i32,
                payload: None,
            }),
            PluginCommand::StopWebServer => Ok(ProtobufPluginCommand {
                name: CommandName::StopWebServer as i32,
                payload: None,
            }),
            PluginCommand::QueryWebServerStatus => Ok(ProtobufPluginCommand {
                name: CommandName::QueryWebServerStatus as i32,
                payload: None,
            }),
            PluginCommand::HighlightAndUnhighlightPanes(
                panes_to_highlight,
                panes_to_unhighlight,
            ) => Ok(ProtobufPluginCommand {
                name: CommandName::HighlightAndUnhighlightPanes as i32,
                payload: Some(Payload::HighlightAndUnhighlightPanesPayload(
                    HighlightAndUnhighlightPanesPayload {
                        pane_ids_to_highlight: panes_to_highlight
                            .iter()
                            .filter_map(|&p| p.try_into().ok())
                            .collect(),
                        pane_ids_to_unhighlight: panes_to_unhighlight
                            .iter()
                            .filter_map(|&p| p.try_into().ok())
                            .collect(),
                    },
                )),
            }),
            PluginCommand::CloseMultiplePanes(pane_ids) => Ok(ProtobufPluginCommand {
                name: CommandName::CloseMultiplePanes as i32,
                payload: Some(Payload::CloseMultiplePanesPayload(
                    CloseMultiplePanesPayload {
                        pane_ids: pane_ids.iter().filter_map(|&p| p.try_into().ok()).collect(),
                    },
                )),
            }),
            PluginCommand::FloatMultiplePanes(pane_ids) => Ok(ProtobufPluginCommand {
                name: CommandName::FloatMultiplePanes as i32,
                payload: Some(Payload::FloatMultiplePanesPayload(
                    FloatMultiplePanesPayload {
                        pane_ids: pane_ids.iter().filter_map(|&p| p.try_into().ok()).collect(),
                    },
                )),
            }),
            PluginCommand::EmbedMultiplePanes(pane_ids) => Ok(ProtobufPluginCommand {
                name: CommandName::EmbedMultiplePanes as i32,
                payload: Some(Payload::EmbedMultiplePanesPayload(
                    EmbedMultiplePanesPayload {
                        pane_ids: pane_ids.iter().filter_map(|&p| p.try_into().ok()).collect(),
                    },
                )),
            }),
            PluginCommand::ShareCurrentSession => Ok(ProtobufPluginCommand {
                name: CommandName::ShareCurrentSession as i32,
                payload: None,
            }),
            PluginCommand::StopSharingCurrentSession => Ok(ProtobufPluginCommand {
                name: CommandName::StopSharingCurrentSession as i32,
                payload: None,
            }),
            PluginCommand::SetSelfMouseSelectionSupport(support_mouse_selection) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::SetSelfMouseSelectionSupport as i32,
                    payload: Some(Payload::SetSelfMouseSelectionSupportPayload(
                        SetSelfMouseSelectionSupportPayload {
                            support_mouse_selection,
                        },
                    )),
                })
            },
            PluginCommand::GenerateWebLoginToken(token_label) => Ok(ProtobufPluginCommand {
                name: CommandName::GenerateWebLoginToken as i32,
                payload: Some(Payload::GenerateWebLoginTokenPayload(
                    GenerateWebLoginTokenPayload { token_label },
                )),
            }),
            PluginCommand::RevokeWebLoginToken(token_label) => Ok(ProtobufPluginCommand {
                name: CommandName::RevokeWebLoginToken as i32,
                payload: Some(Payload::RevokeWebLoginTokenPayload(
                    RevokeWebLoginTokenPayload { token_label },
                )),
            }),
            PluginCommand::ListWebLoginTokens => Ok(ProtobufPluginCommand {
                name: CommandName::ListWebLoginTokens as i32,
                payload: None,
            }),
            PluginCommand::RevokeAllWebLoginTokens => Ok(ProtobufPluginCommand {
                name: CommandName::RevokeAllWebLoginTokens as i32,
                payload: None,
            }),
            PluginCommand::RenameWebLoginToken(old_name, new_name) => Ok(ProtobufPluginCommand {
                name: CommandName::RenameWebLoginToken as i32,
                payload: Some(Payload::RenameWebLoginTokenPayload(
                    RenameWebLoginTokenPayload { old_name, new_name },
                )),
            }),
            PluginCommand::InterceptKeyPresses => Ok(ProtobufPluginCommand {
                name: CommandName::InterceptKeyPresses as i32,
                payload: None,
            }),
            PluginCommand::ClearKeyPressesIntercepts => Ok(ProtobufPluginCommand {
                name: CommandName::ClearKeyPressesIntercepts as i32,
                payload: None,
            }),
            PluginCommand::ReplacePaneWithExistingPane(pane_id_to_replace, existing_pane_id) => {
                Ok(ProtobufPluginCommand {
                    name: CommandName::ReplacePaneWithExistingPane as i32,
                    payload: Some(Payload::ReplacePaneWithExistingPanePayload(
                        ReplacePaneWithExistingPanePayload {
                            pane_id_to_replace: ProtobufPaneId::try_from(pane_id_to_replace).ok(),
                            existing_pane_id: ProtobufPaneId::try_from(existing_pane_id).ok(),
                        },
                    )),
                })
            },
        }
    }
}
