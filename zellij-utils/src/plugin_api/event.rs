pub use super::generated_api::api::{
    action::{Action as ProtobufAction, Position as ProtobufPosition},
    event::{
        event::Payload as ProtobufEventPayload, CopyDestination as ProtobufCopyDestination,
        Event as ProtobufEvent, EventNameList as ProtobufEventNameList,
        EventType as ProtobufEventType, InputModeKeybinds as ProtobufInputModeKeybinds,
        KeyBind as ProtobufKeyBind, ModeUpdatePayload as ProtobufModeUpdatePayload,
        PaneInfo as ProtobufPaneInfo, PaneManifest as ProtobufPaneManifest,
        TabInfo as ProtobufTabInfo, *,
    },
    input_mode::InputMode as ProtobufInputMode,
    key::Key as ProtobufKey,
    style::Style as ProtobufStyle,
};
use crate::data::{
    CopyDestination, Event, EventType, InputMode, Key, ModeInfo, Mouse, PaneInfo, PaneManifest,
    PermissionStatus, PluginCapabilities, Style, TabInfo,
};

use crate::errors::prelude::*;
use crate::input::actions::Action;

use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::path::PathBuf;

impl TryFrom<ProtobufEvent> for Event {
    type Error = &'static str;
    fn try_from(protobuf_event: ProtobufEvent) -> Result<Self, &'static str> {
        match ProtobufEventType::from_i32(protobuf_event.name) {
            Some(ProtobufEventType::ModeUpdate) => match protobuf_event.payload {
                Some(ProtobufEventPayload::ModeUpdatePayload(protobuf_mode_update_payload)) => {
                    let mode_info: ModeInfo = protobuf_mode_update_payload.try_into()?;
                    Ok(Event::ModeUpdate(mode_info))
                },
                _ => Err("Malformed payload for the ModeUpdate Event"),
            },
            Some(ProtobufEventType::TabUpdate) => match protobuf_event.payload {
                Some(ProtobufEventPayload::TabUpdatePayload(protobuf_tab_info_payload)) => {
                    let mut tab_infos: Vec<TabInfo> = vec![];
                    for protobuf_tab_info in protobuf_tab_info_payload.tab_info {
                        tab_infos.push(TabInfo::try_from(protobuf_tab_info)?);
                    }
                    Ok(Event::TabUpdate(tab_infos))
                },
                _ => Err("Malformed payload for the TabUpdate Event"),
            },
            Some(ProtobufEventType::PaneUpdate) => match protobuf_event.payload {
                Some(ProtobufEventPayload::PaneUpdatePayload(protobuf_pane_update_payload)) => {
                    let mut pane_manifest: HashMap<usize, Vec<PaneInfo>> = HashMap::new();
                    for protobuf_pane_manifest in protobuf_pane_update_payload.pane_manifest {
                        let tab_index = protobuf_pane_manifest.tab_index as usize;
                        let mut panes = vec![];
                        for protobuf_pane_info in protobuf_pane_manifest.panes {
                            panes.push(protobuf_pane_info.try_into()?);
                        }
                        if pane_manifest.contains_key(&tab_index) {
                            return Err("Duplicate tab definition in pane manifest");
                        }
                        pane_manifest.insert(tab_index, panes);
                    }
                    Ok(Event::PaneUpdate(PaneManifest {
                        panes: pane_manifest,
                    }))
                },
                _ => Err("Malformed payload for the PaneUpdate Event"),
            },
            Some(ProtobufEventType::Key) => match protobuf_event.payload {
                Some(ProtobufEventPayload::KeyPayload(protobuf_key)) => {
                    Ok(Event::Key(protobuf_key.try_into()?))
                },
                _ => Err("Malformed payload for the Key Event"),
            },
            Some(ProtobufEventType::Mouse) => match protobuf_event.payload {
                Some(ProtobufEventPayload::MouseEventPayload(protobuf_mouse)) => {
                    Ok(Event::Mouse(protobuf_mouse.try_into()?))
                },
                _ => Err("Malformed payload for the Mouse Event"),
            },
            Some(ProtobufEventType::Timer) => match protobuf_event.payload {
                Some(ProtobufEventPayload::TimerPayload(seconds)) => {
                    Ok(Event::Timer(seconds as f64))
                },
                _ => Err("Malformed payload for the Timer Event"),
            },
            Some(ProtobufEventType::CopyToClipboard) => match protobuf_event.payload {
                Some(ProtobufEventPayload::CopyToClipboardPayload(copy_to_clipboard)) => {
                    let protobuf_copy_to_clipboard =
                        ProtobufCopyDestination::from_i32(copy_to_clipboard)
                            .ok_or("Malformed copy to clipboard payload")?;
                    Ok(Event::CopyToClipboard(
                        protobuf_copy_to_clipboard.try_into()?,
                    ))
                },
                _ => Err("Malformed payload for the Copy To Clipboard Event"),
            },
            Some(ProtobufEventType::SystemClipboardFailure) => match protobuf_event.payload {
                None => Ok(Event::SystemClipboardFailure),
                _ => Err("Malformed payload for the system clipboard failure Event"),
            },
            Some(ProtobufEventType::InputReceived) => match protobuf_event.payload {
                None => Ok(Event::InputReceived),
                _ => Err("Malformed payload for the input received Event"),
            },
            Some(ProtobufEventType::Visible) => match protobuf_event.payload {
                Some(ProtobufEventPayload::VisiblePayload(is_visible)) => {
                    Ok(Event::Visible(is_visible))
                },
                _ => Err("Malformed payload for the visible Event"),
            },
            Some(ProtobufEventType::CustomMessage) => match protobuf_event.payload {
                Some(ProtobufEventPayload::CustomMessagePayload(custom_message_payload)) => {
                    Ok(Event::CustomMessage(
                        custom_message_payload.message_name,
                        custom_message_payload.payload,
                    ))
                },
                _ => Err("Malformed payload for the custom message Event"),
            },
            Some(ProtobufEventType::FileSystemCreate) => match protobuf_event.payload {
                Some(ProtobufEventPayload::FileListPayload(file_list_payload)) => {
                    let file_paths = file_list_payload
                        .paths
                        .iter()
                        .map(|p| PathBuf::from(p))
                        .collect();
                    Ok(Event::FileSystemCreate(file_paths))
                },
                _ => Err("Malformed payload for the file system create Event"),
            },
            Some(ProtobufEventType::FileSystemRead) => match protobuf_event.payload {
                Some(ProtobufEventPayload::FileListPayload(file_list_payload)) => {
                    let file_paths = file_list_payload
                        .paths
                        .iter()
                        .map(|p| PathBuf::from(p))
                        .collect();
                    Ok(Event::FileSystemRead(file_paths))
                },
                _ => Err("Malformed payload for the file system read Event"),
            },
            Some(ProtobufEventType::FileSystemUpdate) => match protobuf_event.payload {
                Some(ProtobufEventPayload::FileListPayload(file_list_payload)) => {
                    let file_paths = file_list_payload
                        .paths
                        .iter()
                        .map(|p| PathBuf::from(p))
                        .collect();
                    Ok(Event::FileSystemUpdate(file_paths))
                },
                _ => Err("Malformed payload for the file system update Event"),
            },
            Some(ProtobufEventType::FileSystemDelete) => match protobuf_event.payload {
                Some(ProtobufEventPayload::FileListPayload(file_list_payload)) => {
                    let file_paths = file_list_payload
                        .paths
                        .iter()
                        .map(|p| PathBuf::from(p))
                        .collect();
                    Ok(Event::FileSystemDelete(file_paths))
                },
                _ => Err("Malformed payload for the file system delete Event"),
            },
            Some(ProtobufEventType::PermissionRequestResult) => match protobuf_event.payload {
                Some(ProtobufEventPayload::PermissionRequestResultPayload(payload)) => {
                    if payload.granted {
                        Ok(Event::PermissionRequestResult(PermissionStatus::Granted))
                    } else {
                        Ok(Event::PermissionRequestResult(PermissionStatus::Denied))
                    }
                },
                _ => Err("Malformed payload for the file system delete Event"),
            },
            None => Err("Unknown Protobuf Event"),
        }
    }
}

impl TryFrom<Event> for ProtobufEvent {
    type Error = &'static str;
    fn try_from(event: Event) -> Result<Self, &'static str> {
        match event {
            Event::ModeUpdate(mode_info) => {
                let protobuf_mode_update_payload = mode_info.try_into()?;
                Ok(ProtobufEvent {
                    name: ProtobufEventType::ModeUpdate as i32,
                    payload: Some(event::Payload::ModeUpdatePayload(
                        protobuf_mode_update_payload,
                    )),
                })
            },
            Event::TabUpdate(tab_infos) => {
                let mut protobuf_tab_infos = vec![];
                for tab_info in tab_infos {
                    protobuf_tab_infos.push(tab_info.try_into()?);
                }
                let tab_update_payload = TabUpdatePayload {
                    tab_info: protobuf_tab_infos,
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::TabUpdate as i32,
                    payload: Some(event::Payload::TabUpdatePayload(tab_update_payload)),
                })
            },
            Event::PaneUpdate(pane_manifest) => {
                let mut protobuf_pane_manifests = vec![];
                for (tab_index, pane_infos) in pane_manifest.panes {
                    let mut protobuf_pane_infos = vec![];
                    for pane_info in pane_infos {
                        protobuf_pane_infos.push(pane_info.try_into()?);
                    }
                    protobuf_pane_manifests.push(ProtobufPaneManifest {
                        tab_index: tab_index as u32,
                        panes: protobuf_pane_infos,
                    });
                }
                Ok(ProtobufEvent {
                    name: ProtobufEventType::PaneUpdate as i32,
                    payload: Some(event::Payload::PaneUpdatePayload(PaneUpdatePayload {
                        pane_manifest: protobuf_pane_manifests,
                    })),
                })
            },
            Event::Key(key) => Ok(ProtobufEvent {
                name: ProtobufEventType::Key as i32,
                payload: Some(event::Payload::KeyPayload(key.try_into()?)),
            }),
            Event::Mouse(mouse_event) => {
                let protobuf_mouse_payload = mouse_event.try_into()?;
                Ok(ProtobufEvent {
                    name: ProtobufEventType::Mouse as i32,
                    payload: Some(event::Payload::MouseEventPayload(protobuf_mouse_payload)),
                })
            },
            Event::Timer(seconds) => Ok(ProtobufEvent {
                name: ProtobufEventType::Timer as i32,
                payload: Some(event::Payload::TimerPayload(seconds as f32)),
            }),
            Event::CopyToClipboard(clipboard_destination) => {
                let protobuf_copy_destination: ProtobufCopyDestination =
                    clipboard_destination.try_into()?;
                Ok(ProtobufEvent {
                    name: ProtobufEventType::CopyToClipboard as i32,
                    payload: Some(event::Payload::CopyToClipboardPayload(
                        protobuf_copy_destination as i32,
                    )),
                })
            },
            Event::SystemClipboardFailure => Ok(ProtobufEvent {
                name: ProtobufEventType::SystemClipboardFailure as i32,
                payload: None,
            }),
            Event::InputReceived => Ok(ProtobufEvent {
                name: ProtobufEventType::InputReceived as i32,
                payload: None,
            }),
            Event::Visible(is_visible) => Ok(ProtobufEvent {
                name: ProtobufEventType::Visible as i32,
                payload: Some(event::Payload::VisiblePayload(is_visible)),
            }),
            Event::CustomMessage(message, payload) => Ok(ProtobufEvent {
                name: ProtobufEventType::CustomMessage as i32,
                payload: Some(event::Payload::CustomMessagePayload(CustomMessagePayload {
                    message_name: message,
                    payload,
                })),
            }),
            Event::FileSystemCreate(paths) => {
                let file_list_payload = FileListPayload {
                    paths: paths.iter().map(|p| p.display().to_string()).collect(),
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::FileSystemCreate as i32,
                    payload: Some(event::Payload::FileListPayload(file_list_payload)),
                })
            },
            Event::FileSystemRead(paths) => {
                let file_list_payload = FileListPayload {
                    paths: paths.iter().map(|p| p.display().to_string()).collect(),
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::FileSystemRead as i32,
                    payload: Some(event::Payload::FileListPayload(file_list_payload)),
                })
            },
            Event::FileSystemUpdate(paths) => {
                let file_list_payload = FileListPayload {
                    paths: paths.iter().map(|p| p.display().to_string()).collect(),
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::FileSystemUpdate as i32,
                    payload: Some(event::Payload::FileListPayload(file_list_payload)),
                })
            },
            Event::FileSystemDelete(paths) => {
                let file_list_payload = FileListPayload {
                    paths: paths.iter().map(|p| p.display().to_string()).collect(),
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::FileSystemDelete as i32,
                    payload: Some(event::Payload::FileListPayload(file_list_payload)),
                })
            },
            Event::PermissionRequestResult(permission_status) => {
                let granted = match permission_status {
                    PermissionStatus::Granted => true,
                    PermissionStatus::Denied => false,
                };
                Ok(ProtobufEvent {
                    name: ProtobufEventType::PermissionRequestResult as i32,
                    payload: Some(event::Payload::PermissionRequestResultPayload(
                        PermissionRequestResultPayload { granted },
                    )),
                })
            },
        }
    }
}

impl TryFrom<CopyDestination> for ProtobufCopyDestination {
    type Error = &'static str;
    fn try_from(copy_destination: CopyDestination) -> Result<Self, &'static str> {
        match copy_destination {
            CopyDestination::Command => Ok(ProtobufCopyDestination::Command),
            CopyDestination::Primary => Ok(ProtobufCopyDestination::Primary),
            CopyDestination::System => Ok(ProtobufCopyDestination::System),
        }
    }
}

impl TryFrom<ProtobufCopyDestination> for CopyDestination {
    type Error = &'static str;
    fn try_from(protobuf_copy_destination: ProtobufCopyDestination) -> Result<Self, &'static str> {
        match protobuf_copy_destination {
            ProtobufCopyDestination::Command => Ok(CopyDestination::Command),
            ProtobufCopyDestination::Primary => Ok(CopyDestination::Primary),
            ProtobufCopyDestination::System => Ok(CopyDestination::System),
        }
    }
}

impl TryFrom<MouseEventPayload> for Mouse {
    type Error = &'static str;
    fn try_from(mouse_event_payload: MouseEventPayload) -> Result<Self, &'static str> {
        match MouseEventName::from_i32(mouse_event_payload.mouse_event_name) {
            Some(MouseEventName::MouseScrollUp) => match mouse_event_payload.mouse_event_payload {
                Some(mouse_event_payload::MouseEventPayload::LineCount(line_count)) => {
                    Ok(Mouse::ScrollUp(line_count as usize))
                },
                _ => Err("Malformed payload for mouse scroll up"),
            },
            Some(MouseEventName::MouseScrollDown) => {
                match mouse_event_payload.mouse_event_payload {
                    Some(mouse_event_payload::MouseEventPayload::LineCount(line_count)) => {
                        Ok(Mouse::ScrollDown(line_count as usize))
                    },
                    _ => Err("Malformed payload for mouse scroll down"),
                }
            },
            Some(MouseEventName::MouseLeftClick) => match mouse_event_payload.mouse_event_payload {
                Some(mouse_event_payload::MouseEventPayload::Position(position)) => Ok(
                    Mouse::LeftClick(position.line as isize, position.column as usize),
                ),
                _ => Err("Malformed payload for mouse left click"),
            },
            Some(MouseEventName::MouseRightClick) => {
                match mouse_event_payload.mouse_event_payload {
                    Some(mouse_event_payload::MouseEventPayload::Position(position)) => Ok(
                        Mouse::RightClick(position.line as isize, position.column as usize),
                    ),
                    _ => Err("Malformed payload for mouse right click"),
                }
            },
            Some(MouseEventName::MouseHold) => match mouse_event_payload.mouse_event_payload {
                Some(mouse_event_payload::MouseEventPayload::Position(position)) => Ok(
                    Mouse::Hold(position.line as isize, position.column as usize),
                ),
                _ => Err("Malformed payload for mouse hold"),
            },
            Some(MouseEventName::MouseRelease) => match mouse_event_payload.mouse_event_payload {
                Some(mouse_event_payload::MouseEventPayload::Position(position)) => Ok(
                    Mouse::Release(position.line as isize, position.column as usize),
                ),
                _ => Err("Malformed payload for mouse release"),
            },
            None => Err("Malformed payload for MouseEventName"),
        }
    }
}

impl TryFrom<Mouse> for MouseEventPayload {
    type Error = &'static str;
    fn try_from(mouse: Mouse) -> Result<Self, &'static str> {
        match mouse {
            Mouse::ScrollUp(number_of_lines) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseScrollUp as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::LineCount(
                    number_of_lines as u32,
                )),
            }),
            Mouse::ScrollDown(number_of_lines) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseScrollDown as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::LineCount(
                    number_of_lines as u32,
                )),
            }),
            Mouse::LeftClick(line, column) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseLeftClick as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::Position(
                    ProtobufPosition {
                        line: line as i64,
                        column: column as i64,
                    },
                )),
            }),
            Mouse::RightClick(line, column) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseRightClick as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::Position(
                    ProtobufPosition {
                        line: line as i64,
                        column: column as i64,
                    },
                )),
            }),
            Mouse::Hold(line, column) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseHold as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::Position(
                    ProtobufPosition {
                        line: line as i64,
                        column: column as i64,
                    },
                )),
            }),
            Mouse::Release(line, column) => Ok(MouseEventPayload {
                mouse_event_name: MouseEventName::MouseRelease as i32,
                mouse_event_payload: Some(mouse_event_payload::MouseEventPayload::Position(
                    ProtobufPosition {
                        line: line as i64,
                        column: column as i64,
                    },
                )),
            }),
        }
    }
}

impl TryFrom<ProtobufPaneInfo> for PaneInfo {
    type Error = &'static str;
    fn try_from(protobuf_pane_info: ProtobufPaneInfo) -> Result<Self, &'static str> {
        Ok(PaneInfo {
            id: protobuf_pane_info.id,
            is_plugin: protobuf_pane_info.is_plugin,
            is_focused: protobuf_pane_info.is_focused,
            is_fullscreen: protobuf_pane_info.is_fullscreen,
            is_floating: protobuf_pane_info.is_floating,
            is_suppressed: protobuf_pane_info.is_suppressed,
            title: protobuf_pane_info.title,
            exited: protobuf_pane_info.exited,
            exit_status: protobuf_pane_info.exit_status,
            is_held: protobuf_pane_info.is_held,
            pane_x: protobuf_pane_info.pane_x as usize,
            pane_content_x: protobuf_pane_info.pane_content_x as usize,
            pane_y: protobuf_pane_info.pane_y as usize,
            pane_content_y: protobuf_pane_info.pane_content_y as usize,
            pane_rows: protobuf_pane_info.pane_rows as usize,
            pane_content_rows: protobuf_pane_info.pane_content_rows as usize,
            pane_columns: protobuf_pane_info.pane_columns as usize,
            pane_content_columns: protobuf_pane_info.pane_content_columns as usize,
            cursor_coordinates_in_pane: protobuf_pane_info
                .cursor_coordinates_in_pane
                .map(|position| (position.column as usize, position.line as usize)),
            terminal_command: protobuf_pane_info.terminal_command,
            plugin_url: protobuf_pane_info.plugin_url,
            is_selectable: protobuf_pane_info.is_selectable,
        })
    }
}

impl TryFrom<PaneInfo> for ProtobufPaneInfo {
    type Error = &'static str;
    fn try_from(pane_info: PaneInfo) -> Result<Self, &'static str> {
        Ok(ProtobufPaneInfo {
            id: pane_info.id,
            is_plugin: pane_info.is_plugin,
            is_focused: pane_info.is_focused,
            is_fullscreen: pane_info.is_fullscreen,
            is_floating: pane_info.is_floating,
            is_suppressed: pane_info.is_suppressed,
            title: pane_info.title,
            exited: pane_info.exited,
            exit_status: pane_info.exit_status,
            is_held: pane_info.is_held,
            pane_x: pane_info.pane_x as u32,
            pane_content_x: pane_info.pane_content_x as u32,
            pane_y: pane_info.pane_y as u32,
            pane_content_y: pane_info.pane_content_y as u32,
            pane_rows: pane_info.pane_rows as u32,
            pane_content_rows: pane_info.pane_content_rows as u32,
            pane_columns: pane_info.pane_columns as u32,
            pane_content_columns: pane_info.pane_content_columns as u32,
            cursor_coordinates_in_pane: pane_info.cursor_coordinates_in_pane.map(|(x, y)| {
                ProtobufPosition {
                    column: x as i64,
                    line: y as i64,
                }
            }),
            terminal_command: pane_info.terminal_command,
            plugin_url: pane_info.plugin_url,
            is_selectable: pane_info.is_selectable,
        })
    }
}

impl TryFrom<ProtobufTabInfo> for TabInfo {
    type Error = &'static str;
    fn try_from(protobuf_tab_info: ProtobufTabInfo) -> Result<Self, &'static str> {
        Ok(TabInfo {
            position: protobuf_tab_info.position as usize,
            name: protobuf_tab_info.name,
            active: protobuf_tab_info.active,
            panes_to_hide: protobuf_tab_info.panes_to_hide as usize,
            is_fullscreen_active: protobuf_tab_info.is_fullscreen_active,
            is_sync_panes_active: protobuf_tab_info.is_sync_panes_active,
            are_floating_panes_visible: protobuf_tab_info.are_floating_panes_visible,
            other_focused_clients: protobuf_tab_info
                .other_focused_clients
                .iter()
                .map(|c| *c as u16)
                .collect(),
            active_swap_layout_name: protobuf_tab_info.active_swap_layout_name,
            is_swap_layout_dirty: protobuf_tab_info.is_swap_layout_dirty,
        })
    }
}

impl TryFrom<TabInfo> for ProtobufTabInfo {
    type Error = &'static str;
    fn try_from(tab_info: TabInfo) -> Result<Self, &'static str> {
        Ok(ProtobufTabInfo {
            position: tab_info.position as u32,
            name: tab_info.name,
            active: tab_info.active,
            panes_to_hide: tab_info.panes_to_hide as u32,
            is_fullscreen_active: tab_info.is_fullscreen_active,
            is_sync_panes_active: tab_info.is_sync_panes_active,
            are_floating_panes_visible: tab_info.are_floating_panes_visible,
            other_focused_clients: tab_info
                .other_focused_clients
                .iter()
                .map(|c| *c as u32)
                .collect(),
            active_swap_layout_name: tab_info.active_swap_layout_name,
            is_swap_layout_dirty: tab_info.is_swap_layout_dirty,
        })
    }
}

impl TryFrom<ProtobufModeUpdatePayload> for ModeInfo {
    type Error = &'static str;
    fn try_from(
        mut protobuf_mode_update_payload: ProtobufModeUpdatePayload,
    ) -> Result<Self, &'static str> {
        let current_mode: InputMode =
            ProtobufInputMode::from_i32(protobuf_mode_update_payload.current_mode)
                .ok_or("Malformed InputMode in the ModeUpdate Event")?
                .try_into()?;
        let keybinds: Vec<(InputMode, Vec<(Key, Vec<Action>)>)> = protobuf_mode_update_payload
            .keybinds
            .iter_mut()
            .filter_map(|k| {
                let input_mode: InputMode = ProtobufInputMode::from_i32(k.mode)
                    .ok_or("Malformed InputMode in the ModeUpdate Event")
                    .ok()?
                    .try_into()
                    .ok()?;
                let mut keybinds: Vec<(Key, Vec<Action>)> = vec![];
                for mut protobuf_keybind in k.key_bind.drain(..) {
                    let key: Key = protobuf_keybind.key.unwrap().try_into().ok()?;
                    let mut actions: Vec<Action> = vec![];
                    for action in protobuf_keybind.action.drain(..) {
                        if let Ok(action) = action.try_into() {
                            actions.push(action);
                        }
                    }
                    keybinds.push((key, actions));
                }
                Some((input_mode, keybinds))
            })
            .collect();
        let style: Style = protobuf_mode_update_payload
            .style
            .and_then(|m| m.try_into().ok())
            .ok_or("malformed payload for mode_info")?;
        let session_name = protobuf_mode_update_payload.session_name;
        let capabilities = PluginCapabilities {
            arrow_fonts: protobuf_mode_update_payload.arrow_fonts_support,
        };
        let mode_info = ModeInfo {
            mode: current_mode,
            keybinds,
            style,
            capabilities,
            session_name,
        };
        Ok(mode_info)
    }
}

impl TryFrom<ModeInfo> for ProtobufModeUpdatePayload {
    type Error = &'static str;
    fn try_from(mode_info: ModeInfo) -> Result<Self, &'static str> {
        let current_mode: ProtobufInputMode = mode_info.mode.try_into()?;
        let style: ProtobufStyle = mode_info.style.try_into()?;
        let arrow_fonts_support: bool = mode_info.capabilities.arrow_fonts;
        let session_name = mode_info.session_name;
        let mut protobuf_input_mode_keybinds: Vec<ProtobufInputModeKeybinds> = vec![];
        for (input_mode, input_mode_keybinds) in mode_info.keybinds {
            let mode: ProtobufInputMode = input_mode.try_into()?;
            let mut keybinds: Vec<ProtobufKeyBind> = vec![];
            for (key, actions) in input_mode_keybinds {
                let protobuf_key: ProtobufKey = key.try_into()?;
                let mut protobuf_actions: Vec<ProtobufAction> = vec![];
                for action in actions {
                    if let Ok(protobuf_action) = action.try_into() {
                        protobuf_actions.push(protobuf_action);
                    }
                }
                let key_bind = ProtobufKeyBind {
                    key: Some(protobuf_key),
                    action: protobuf_actions,
                };
                keybinds.push(key_bind);
            }
            let input_mode_keybind = ProtobufInputModeKeybinds {
                mode: mode as i32,
                key_bind: keybinds,
            };
            protobuf_input_mode_keybinds.push(input_mode_keybind);
        }
        Ok(ProtobufModeUpdatePayload {
            current_mode: current_mode as i32,
            style: Some(style),
            keybinds: protobuf_input_mode_keybinds,
            arrow_fonts_support,
            session_name,
        })
    }
}

impl TryFrom<ProtobufEventNameList> for HashSet<EventType> {
    type Error = &'static str;
    fn try_from(protobuf_event_name_list: ProtobufEventNameList) -> Result<Self, &'static str> {
        let event_types: Vec<ProtobufEventType> = protobuf_event_name_list
            .event_types
            .iter()
            .filter_map(|i| ProtobufEventType::from_i32(*i))
            .collect();
        let event_types: Vec<EventType> = event_types
            .iter()
            .filter_map(|e| EventType::try_from(*e).ok())
            .collect();
        Ok(event_types.into_iter().collect())
    }
}

impl TryFrom<HashSet<EventType>> for ProtobufEventNameList {
    type Error = &'static str;
    fn try_from(event_types: HashSet<EventType>) -> Result<Self, &'static str> {
        let protobuf_event_name_list = ProtobufEventNameList {
            event_types: event_types
                .iter()
                .filter_map(|e| ProtobufEventType::try_from(*e).ok())
                .map(|e| e as i32)
                .collect(),
        };
        Ok(protobuf_event_name_list)
    }
}

impl TryFrom<ProtobufEventType> for EventType {
    type Error = &'static str;
    fn try_from(protobuf_event_type: ProtobufEventType) -> Result<Self, &'static str> {
        Ok(match protobuf_event_type {
            ProtobufEventType::ModeUpdate => EventType::ModeUpdate,
            ProtobufEventType::TabUpdate => EventType::TabUpdate,
            ProtobufEventType::PaneUpdate => EventType::PaneUpdate,
            ProtobufEventType::Key => EventType::Key,
            ProtobufEventType::Mouse => EventType::Mouse,
            ProtobufEventType::Timer => EventType::Timer,
            ProtobufEventType::CopyToClipboard => EventType::CopyToClipboard,
            ProtobufEventType::SystemClipboardFailure => EventType::SystemClipboardFailure,
            ProtobufEventType::InputReceived => EventType::InputReceived,
            ProtobufEventType::Visible => EventType::Visible,
            ProtobufEventType::CustomMessage => EventType::CustomMessage,
            ProtobufEventType::FileSystemCreate => EventType::FileSystemCreate,
            ProtobufEventType::FileSystemRead => EventType::FileSystemRead,
            ProtobufEventType::FileSystemUpdate => EventType::FileSystemUpdate,
            ProtobufEventType::FileSystemDelete => EventType::FileSystemDelete,
            ProtobufEventType::PermissionRequestResult => EventType::PermissionRequestResult,
        })
    }
}

impl TryFrom<EventType> for ProtobufEventType {
    type Error = &'static str;
    fn try_from(event_type: EventType) -> Result<Self, &'static str> {
        Ok(match event_type {
            EventType::ModeUpdate => ProtobufEventType::ModeUpdate,
            EventType::TabUpdate => ProtobufEventType::TabUpdate,
            EventType::PaneUpdate => ProtobufEventType::PaneUpdate,
            EventType::Key => ProtobufEventType::Key,
            EventType::Mouse => ProtobufEventType::Mouse,
            EventType::Timer => ProtobufEventType::Timer,
            EventType::CopyToClipboard => ProtobufEventType::CopyToClipboard,
            EventType::SystemClipboardFailure => ProtobufEventType::SystemClipboardFailure,
            EventType::InputReceived => ProtobufEventType::InputReceived,
            EventType::Visible => ProtobufEventType::Visible,
            EventType::CustomMessage => ProtobufEventType::CustomMessage,
            EventType::FileSystemCreate => ProtobufEventType::FileSystemCreate,
            EventType::FileSystemRead => ProtobufEventType::FileSystemRead,
            EventType::FileSystemUpdate => ProtobufEventType::FileSystemUpdate,
            EventType::FileSystemDelete => ProtobufEventType::FileSystemDelete,
            EventType::PermissionRequestResult => ProtobufEventType::PermissionRequestResult,
        })
    }
}

#[test]
fn serialize_mode_update_event() {
    use prost::Message;
    let mode_update_event = Event::ModeUpdate(Default::default());
    let protobuf_event: ProtobufEvent = mode_update_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        mode_update_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_mode_update_event_with_non_default_values() {
    use crate::data::{Direction, Palette, PaletteColor, ThemeHue};
    use prost::Message;
    let mode_update_event = Event::ModeUpdate(ModeInfo {
        mode: InputMode::Locked,
        keybinds: vec![
            (
                InputMode::Locked,
                vec![(
                    Key::Alt(crate::data::CharOrArrow::Char('b')),
                    vec![Action::SwitchToMode(InputMode::Normal)],
                )],
            ),
            (
                InputMode::Tab,
                vec![(
                    Key::Alt(crate::data::CharOrArrow::Direction(Direction::Up)),
                    vec![Action::SwitchToMode(InputMode::Pane)],
                )],
            ),
            (
                InputMode::Pane,
                vec![
                    (
                        Key::Ctrl('b'),
                        vec![
                            Action::SwitchToMode(InputMode::Tmux),
                            Action::Write(vec![10]),
                        ],
                    ),
                    (Key::Char('a'), vec![Action::WriteChars("foo".to_owned())]),
                ],
            ),
        ],
        style: Style {
            colors: Palette {
                source: crate::data::PaletteSource::Default,
                theme_hue: ThemeHue::Light,
                fg: PaletteColor::Rgb((1, 1, 1)),
                bg: PaletteColor::Rgb((200, 200, 200)),
                black: PaletteColor::EightBit(1),
                red: PaletteColor::EightBit(2),
                green: PaletteColor::EightBit(2),
                yellow: PaletteColor::EightBit(2),
                blue: PaletteColor::EightBit(2),
                magenta: PaletteColor::EightBit(2),
                cyan: PaletteColor::EightBit(2),
                white: PaletteColor::EightBit(2),
                orange: PaletteColor::EightBit(2),
                gray: PaletteColor::EightBit(2),
                purple: PaletteColor::EightBit(2),
                gold: PaletteColor::EightBit(2),
                silver: PaletteColor::EightBit(2),
                pink: PaletteColor::EightBit(2),
                brown: PaletteColor::Rgb((222, 221, 220)),
            },
            rounded_corners: true,
            hide_session_name: false,
        },
        capabilities: PluginCapabilities { arrow_fonts: false },
        session_name: Some("my awesome test session".to_owned()),
    });
    let protobuf_event: ProtobufEvent = mode_update_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        mode_update_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_tab_update_event() {
    use prost::Message;
    let tab_update_event = Event::TabUpdate(Default::default());
    let protobuf_event: ProtobufEvent = tab_update_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        tab_update_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_tab_update_event_with_non_default_values() {
    use prost::Message;
    let tab_update_event = Event::TabUpdate(vec![
        TabInfo {
            position: 0,
            name: "First tab".to_owned(),
            active: true,
            panes_to_hide: 2,
            is_fullscreen_active: true,
            is_sync_panes_active: false,
            are_floating_panes_visible: true,
            other_focused_clients: vec![2, 3, 4],
            active_swap_layout_name: Some("my cool swap layout".to_owned()),
            is_swap_layout_dirty: false,
        },
        TabInfo {
            position: 1,
            name: "Secondtab".to_owned(),
            active: false,
            panes_to_hide: 5,
            is_fullscreen_active: false,
            is_sync_panes_active: true,
            are_floating_panes_visible: true,
            other_focused_clients: vec![1, 5, 111],
            active_swap_layout_name: None,
            is_swap_layout_dirty: true,
        },
        TabInfo::default(),
    ]);
    let protobuf_event: ProtobufEvent = tab_update_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        tab_update_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_pane_update_event() {
    use prost::Message;
    let pane_update_event = Event::PaneUpdate(Default::default());
    let protobuf_event: ProtobufEvent = pane_update_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        pane_update_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_key_event() {
    use prost::Message;
    let key_event = Event::Key(Key::Ctrl('a'));
    let protobuf_event: ProtobufEvent = key_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        key_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_mouse_event() {
    use prost::Message;
    let mouse_event = Event::Mouse(Mouse::LeftClick(1, 1));
    let protobuf_event: ProtobufEvent = mouse_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        mouse_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_mouse_event_without_position() {
    use prost::Message;
    let mouse_event = Event::Mouse(Mouse::ScrollUp(17));
    let protobuf_event: ProtobufEvent = mouse_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        mouse_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_timer_event() {
    use prost::Message;
    let timer_event = Event::Timer(1.5);
    let protobuf_event: ProtobufEvent = timer_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        timer_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_copy_to_clipboard_event() {
    use prost::Message;
    let copy_event = Event::CopyToClipboard(CopyDestination::Primary);
    let protobuf_event: ProtobufEvent = copy_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        copy_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_clipboard_failure_event() {
    use prost::Message;
    let copy_event = Event::SystemClipboardFailure;
    let protobuf_event: ProtobufEvent = copy_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        copy_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_input_received_event() {
    use prost::Message;
    let input_received_event = Event::InputReceived;
    let protobuf_event: ProtobufEvent = input_received_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        input_received_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_visible_event() {
    use prost::Message;
    let visible_event = Event::Visible(true);
    let protobuf_event: ProtobufEvent = visible_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        visible_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_custom_message_event() {
    use prost::Message;
    let custom_message_event = Event::CustomMessage("foo".to_owned(), "bar".to_owned());
    let protobuf_event: ProtobufEvent = custom_message_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        custom_message_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_file_system_create_event() {
    use prost::Message;
    let file_system_event =
        Event::FileSystemCreate(vec!["/absolute/path".into(), "./relative_path".into()]);
    let protobuf_event: ProtobufEvent = file_system_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        file_system_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_file_system_read_event() {
    use prost::Message;
    let file_system_event =
        Event::FileSystemRead(vec!["/absolute/path".into(), "./relative_path".into()]);
    let protobuf_event: ProtobufEvent = file_system_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        file_system_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_file_system_update_event() {
    use prost::Message;
    let file_system_event =
        Event::FileSystemUpdate(vec!["/absolute/path".into(), "./relative_path".into()]);
    let protobuf_event: ProtobufEvent = file_system_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        file_system_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}

#[test]
fn serialize_file_system_delete_event() {
    use prost::Message;
    let file_system_event =
        Event::FileSystemDelete(vec!["/absolute/path".into(), "./relative_path".into()]);
    let protobuf_event: ProtobufEvent = file_system_event.clone().try_into().unwrap();
    let serialized_protobuf_event = protobuf_event.encode_to_vec();
    let deserialized_protobuf_event: ProtobufEvent =
        Message::decode(serialized_protobuf_event.as_slice()).unwrap();
    let deserialized_event: Event = deserialized_protobuf_event.try_into().unwrap();
    assert_eq!(
        file_system_event, deserialized_event,
        "Event properly serialized/deserialized without change"
    );
}
