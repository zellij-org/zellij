#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EventNameList {
    #[prost(enumeration = "EventType", repeated, tag = "1")]
    pub event_types: ::prost::alloc::vec::Vec<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Event {
    #[prost(enumeration = "EventType", tag = "1")]
    pub name: i32,
    #[prost(
        oneof = "event::Payload",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15"
    )]
    pub payload: ::core::option::Option<event::Payload>,
}
/// Nested message and enum types in `Event`.
pub mod event {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag = "2")]
        ModeUpdatePayload(super::ModeUpdatePayload),
        #[prost(message, tag = "3")]
        TabUpdatePayload(super::TabUpdatePayload),
        #[prost(message, tag = "4")]
        PaneUpdatePayload(super::PaneUpdatePayload),
        #[prost(message, tag = "5")]
        KeyPayload(super::super::key::Key),
        #[prost(message, tag = "6")]
        MouseEventPayload(super::MouseEventPayload),
        #[prost(float, tag = "7")]
        TimerPayload(f32),
        #[prost(enumeration = "super::CopyDestination", tag = "8")]
        CopyToClipboardPayload(i32),
        #[prost(bool, tag = "9")]
        VisiblePayload(bool),
        #[prost(message, tag = "10")]
        CustomMessagePayload(super::CustomMessagePayload),
        #[prost(message, tag = "11")]
        FileListPayload(super::FileListPayload),
        #[prost(message, tag = "12")]
        PermissionRequestResultPayload(super::PermissionRequestResultPayload),
        #[prost(message, tag = "13")]
        SessionUpdatePayload(super::SessionUpdatePayload),
        #[prost(message, tag = "14")]
        RunCommandResultPayload(super::RunCommandResultPayload),
        #[prost(message, tag = "15")]
        WebRequestResultPayload(super::WebRequestResultPayload),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SessionUpdatePayload {
    #[prost(message, repeated, tag = "1")]
    pub session_manifests: ::prost::alloc::vec::Vec<SessionManifest>,
    #[prost(message, repeated, tag = "2")]
    pub resurrectable_sessions: ::prost::alloc::vec::Vec<ResurrectableSession>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunCommandResultPayload {
    #[prost(int32, optional, tag = "1")]
    pub exit_code: ::core::option::Option<i32>,
    #[prost(bytes = "vec", tag = "2")]
    pub stdout: ::prost::alloc::vec::Vec<u8>,
    #[prost(bytes = "vec", tag = "3")]
    pub stderr: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag = "4")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WebRequestResultPayload {
    #[prost(int32, tag = "1")]
    pub status: i32,
    #[prost(message, repeated, tag = "2")]
    pub headers: ::prost::alloc::vec::Vec<Header>,
    #[prost(bytes = "vec", tag = "3")]
    pub body: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag = "4")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContextItem {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Header {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PermissionRequestResultPayload {
    #[prost(bool, tag = "1")]
    pub granted: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileListPayload {
    #[prost(string, repeated, tag = "1")]
    pub paths: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, repeated, tag = "2")]
    pub paths_metadata: ::prost::alloc::vec::Vec<FileMetadata>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FileMetadata {
    /// if this is false, the metadata for this file has not been read
    #[prost(bool, tag = "1")]
    pub metadata_is_set: bool,
    #[prost(bool, tag = "2")]
    pub is_dir: bool,
    #[prost(bool, tag = "3")]
    pub is_file: bool,
    #[prost(bool, tag = "4")]
    pub is_symlink: bool,
    #[prost(uint64, tag = "5")]
    pub len: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CustomMessagePayload {
    #[prost(string, tag = "1")]
    pub message_name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub payload: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MouseEventPayload {
    #[prost(enumeration = "MouseEventName", tag = "1")]
    pub mouse_event_name: i32,
    #[prost(oneof = "mouse_event_payload::MouseEventPayload", tags = "2, 3")]
    pub mouse_event_payload: ::core::option::Option<
        mouse_event_payload::MouseEventPayload,
    >,
}
/// Nested message and enum types in `MouseEventPayload`.
pub mod mouse_event_payload {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum MouseEventPayload {
        #[prost(uint32, tag = "2")]
        LineCount(u32),
        #[prost(message, tag = "3")]
        Position(super::super::action::Position),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TabUpdatePayload {
    #[prost(message, repeated, tag = "1")]
    pub tab_info: ::prost::alloc::vec::Vec<TabInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneUpdatePayload {
    #[prost(message, repeated, tag = "1")]
    pub pane_manifest: ::prost::alloc::vec::Vec<PaneManifest>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneManifest {
    #[prost(uint32, tag = "1")]
    pub tab_index: u32,
    #[prost(message, repeated, tag = "2")]
    pub panes: ::prost::alloc::vec::Vec<PaneInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SessionManifest {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "2")]
    pub tabs: ::prost::alloc::vec::Vec<TabInfo>,
    #[prost(message, repeated, tag = "3")]
    pub panes: ::prost::alloc::vec::Vec<PaneManifest>,
    #[prost(uint32, tag = "4")]
    pub connected_clients: u32,
    #[prost(bool, tag = "5")]
    pub is_current_session: bool,
    #[prost(message, repeated, tag = "6")]
    pub available_layouts: ::prost::alloc::vec::Vec<LayoutInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutInfo {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub source: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResurrectableSession {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(uint64, tag = "2")]
    pub creation_time: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneInfo {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(bool, tag = "2")]
    pub is_plugin: bool,
    #[prost(bool, tag = "3")]
    pub is_focused: bool,
    #[prost(bool, tag = "4")]
    pub is_fullscreen: bool,
    #[prost(bool, tag = "5")]
    pub is_floating: bool,
    #[prost(bool, tag = "6")]
    pub is_suppressed: bool,
    #[prost(string, tag = "7")]
    pub title: ::prost::alloc::string::String,
    #[prost(bool, tag = "8")]
    pub exited: bool,
    #[prost(int32, optional, tag = "9")]
    pub exit_status: ::core::option::Option<i32>,
    #[prost(bool, tag = "10")]
    pub is_held: bool,
    #[prost(uint32, tag = "11")]
    pub pane_x: u32,
    #[prost(uint32, tag = "12")]
    pub pane_content_x: u32,
    #[prost(uint32, tag = "13")]
    pub pane_y: u32,
    #[prost(uint32, tag = "14")]
    pub pane_content_y: u32,
    #[prost(uint32, tag = "15")]
    pub pane_rows: u32,
    #[prost(uint32, tag = "16")]
    pub pane_content_rows: u32,
    #[prost(uint32, tag = "17")]
    pub pane_columns: u32,
    #[prost(uint32, tag = "18")]
    pub pane_content_columns: u32,
    #[prost(message, optional, tag = "19")]
    pub cursor_coordinates_in_pane: ::core::option::Option<super::action::Position>,
    #[prost(string, optional, tag = "20")]
    pub terminal_command: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "21")]
    pub plugin_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag = "22")]
    pub is_selectable: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TabInfo {
    #[prost(uint32, tag = "1")]
    pub position: u32,
    #[prost(string, tag = "2")]
    pub name: ::prost::alloc::string::String,
    #[prost(bool, tag = "3")]
    pub active: bool,
    #[prost(uint32, tag = "4")]
    pub panes_to_hide: u32,
    #[prost(bool, tag = "5")]
    pub is_fullscreen_active: bool,
    #[prost(bool, tag = "6")]
    pub is_sync_panes_active: bool,
    #[prost(bool, tag = "7")]
    pub are_floating_panes_visible: bool,
    #[prost(uint32, repeated, tag = "8")]
    pub other_focused_clients: ::prost::alloc::vec::Vec<u32>,
    #[prost(string, optional, tag = "9")]
    pub active_swap_layout_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag = "10")]
    pub is_swap_layout_dirty: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModeUpdatePayload {
    #[prost(enumeration = "super::input_mode::InputMode", tag = "1")]
    pub current_mode: i32,
    #[prost(message, repeated, tag = "2")]
    pub keybinds: ::prost::alloc::vec::Vec<InputModeKeybinds>,
    #[prost(message, optional, tag = "3")]
    pub style: ::core::option::Option<super::style::Style>,
    #[prost(bool, tag = "4")]
    pub arrow_fonts_support: bool,
    #[prost(string, optional, tag = "5")]
    pub session_name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InputModeKeybinds {
    #[prost(enumeration = "super::input_mode::InputMode", tag = "1")]
    pub mode: i32,
    #[prost(message, repeated, tag = "2")]
    pub key_bind: ::prost::alloc::vec::Vec<KeyBind>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyBind {
    #[prost(message, optional, tag = "1")]
    pub key: ::core::option::Option<super::key::Key>,
    #[prost(message, repeated, tag = "2")]
    pub action: ::prost::alloc::vec::Vec<super::action::Action>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum EventType {
    /// / The input mode or relevant metadata changed
    ModeUpdate = 0,
    /// / The tab state in the app was changed
    TabUpdate = 1,
    /// / The pane state in the app was changed
    PaneUpdate = 2,
    /// / A key was pressed while the user is focused on this plugin's pane
    Key = 3,
    /// / A mouse event happened while the user is focused on this plugin's pane
    Mouse = 4,
    /// / A timer expired set by the `set_timeout` method exported by `zellij-tile`.
    Timer = 5,
    /// / Text was copied to the clipboard anywhere in the app
    CopyToClipboard = 6,
    /// / Failed to copy text to clipboard anywhere in the app
    SystemClipboardFailure = 7,
    /// / Input was received anywhere in the app
    InputReceived = 8,
    /// / This plugin became visible or invisible
    Visible = 9,
    /// / A message from one of the plugin's workers
    CustomMessage = 10,
    /// / A file was created somewhere in the Zellij CWD folder
    FileSystemCreate = 11,
    /// / A file was accessed somewhere in the Zellij CWD folder
    FileSystemRead = 12,
    /// / A file was modified somewhere in the Zellij CWD folder
    FileSystemUpdate = 13,
    /// / A file was deleted somewhere in the Zellij CWD folder
    FileSystemDelete = 14,
    PermissionRequestResult = 15,
    SessionUpdate = 16,
    RunCommandResult = 17,
    WebRequestResult = 18,
}
impl EventType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            EventType::ModeUpdate => "ModeUpdate",
            EventType::TabUpdate => "TabUpdate",
            EventType::PaneUpdate => "PaneUpdate",
            EventType::Key => "Key",
            EventType::Mouse => "Mouse",
            EventType::Timer => "Timer",
            EventType::CopyToClipboard => "CopyToClipboard",
            EventType::SystemClipboardFailure => "SystemClipboardFailure",
            EventType::InputReceived => "InputReceived",
            EventType::Visible => "Visible",
            EventType::CustomMessage => "CustomMessage",
            EventType::FileSystemCreate => "FileSystemCreate",
            EventType::FileSystemRead => "FileSystemRead",
            EventType::FileSystemUpdate => "FileSystemUpdate",
            EventType::FileSystemDelete => "FileSystemDelete",
            EventType::PermissionRequestResult => "PermissionRequestResult",
            EventType::SessionUpdate => "SessionUpdate",
            EventType::RunCommandResult => "RunCommandResult",
            EventType::WebRequestResult => "WebRequestResult",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ModeUpdate" => Some(Self::ModeUpdate),
            "TabUpdate" => Some(Self::TabUpdate),
            "PaneUpdate" => Some(Self::PaneUpdate),
            "Key" => Some(Self::Key),
            "Mouse" => Some(Self::Mouse),
            "Timer" => Some(Self::Timer),
            "CopyToClipboard" => Some(Self::CopyToClipboard),
            "SystemClipboardFailure" => Some(Self::SystemClipboardFailure),
            "InputReceived" => Some(Self::InputReceived),
            "Visible" => Some(Self::Visible),
            "CustomMessage" => Some(Self::CustomMessage),
            "FileSystemCreate" => Some(Self::FileSystemCreate),
            "FileSystemRead" => Some(Self::FileSystemRead),
            "FileSystemUpdate" => Some(Self::FileSystemUpdate),
            "FileSystemDelete" => Some(Self::FileSystemDelete),
            "PermissionRequestResult" => Some(Self::PermissionRequestResult),
            "SessionUpdate" => Some(Self::SessionUpdate),
            "RunCommandResult" => Some(Self::RunCommandResult),
            "WebRequestResult" => Some(Self::WebRequestResult),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CopyDestination {
    Command = 0,
    Primary = 1,
    System = 2,
}
impl CopyDestination {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            CopyDestination::Command => "Command",
            CopyDestination::Primary => "Primary",
            CopyDestination::System => "System",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Command" => Some(Self::Command),
            "Primary" => Some(Self::Primary),
            "System" => Some(Self::System),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MouseEventName {
    MouseScrollUp = 0,
    MouseScrollDown = 1,
    MouseLeftClick = 2,
    MouseRightClick = 3,
    MouseHold = 4,
    MouseRelease = 5,
}
impl MouseEventName {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MouseEventName::MouseScrollUp => "MouseScrollUp",
            MouseEventName::MouseScrollDown => "MouseScrollDown",
            MouseEventName::MouseLeftClick => "MouseLeftClick",
            MouseEventName::MouseRightClick => "MouseRightClick",
            MouseEventName::MouseHold => "MouseHold",
            MouseEventName::MouseRelease => "MouseRelease",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "MouseScrollUp" => Some(Self::MouseScrollUp),
            "MouseScrollDown" => Some(Self::MouseScrollDown),
            "MouseLeftClick" => Some(Self::MouseLeftClick),
            "MouseRightClick" => Some(Self::MouseRightClick),
            "MouseHold" => Some(Self::MouseHold),
            "MouseRelease" => Some(Self::MouseRelease),
            _ => None,
        }
    }
}
