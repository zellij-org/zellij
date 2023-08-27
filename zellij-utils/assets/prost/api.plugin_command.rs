#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginCommand {
    #[prost(enumeration = "CommandName", tag = "1")]
    pub name: i32,
    #[prost(
        oneof = "plugin_command::Payload",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39"
    )]
    pub payload: ::core::option::Option<plugin_command::Payload>,
}
/// Nested message and enum types in `PluginCommand`.
pub mod plugin_command {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag = "2")]
        SubscribePayload(super::SubscribePayload),
        #[prost(message, tag = "3")]
        UnsubscribePayload(super::UnsubscribePayload),
        #[prost(bool, tag = "4")]
        SetSelectablePayload(bool),
        #[prost(message, tag = "5")]
        OpenFilePayload(super::OpenFilePayload),
        #[prost(message, tag = "6")]
        OpenFileFloatingPayload(super::OpenFilePayload),
        #[prost(message, tag = "7")]
        OpenTerminalPayload(super::OpenFilePayload),
        #[prost(message, tag = "8")]
        OpenTerminalFloatingPayload(super::OpenFilePayload),
        #[prost(message, tag = "9")]
        OpenCommandPanePayload(super::OpenCommandPanePayload),
        #[prost(message, tag = "10")]
        OpenCommandPaneFloatingPayload(super::OpenCommandPanePayload),
        #[prost(message, tag = "11")]
        SwitchTabToPayload(super::SwitchTabToPayload),
        #[prost(message, tag = "12")]
        SetTimeoutPayload(super::SetTimeoutPayload),
        #[prost(message, tag = "13")]
        ExecCmdPayload(super::ExecCmdPayload),
        #[prost(message, tag = "14")]
        PostMessageToPayload(super::PluginMessagePayload),
        #[prost(message, tag = "15")]
        PostMessageToPluginPayload(super::PluginMessagePayload),
        #[prost(bool, tag = "16")]
        ShowSelfPayload(bool),
        #[prost(message, tag = "17")]
        SwitchToModePayload(super::super::action::SwitchToModePayload),
        #[prost(string, tag = "18")]
        NewTabsWithLayoutPayload(::prost::alloc::string::String),
        #[prost(message, tag = "19")]
        ResizePayload(super::ResizePayload),
        #[prost(message, tag = "20")]
        ResizeWithDirectionPayload(super::ResizePayload),
        #[prost(message, tag = "21")]
        MoveFocusPayload(super::MovePayload),
        #[prost(message, tag = "22")]
        MoveFocusOrTabPayload(super::MovePayload),
        #[prost(bytes, tag = "23")]
        WritePayload(::prost::alloc::vec::Vec<u8>),
        #[prost(string, tag = "24")]
        WriteCharsPayload(::prost::alloc::string::String),
        #[prost(message, tag = "25")]
        MovePaneWithDirectionPayload(super::MovePayload),
        #[prost(string, tag = "26")]
        GoToTabNamePayload(::prost::alloc::string::String),
        #[prost(string, tag = "27")]
        FocusOrCreateTabPayload(::prost::alloc::string::String),
        #[prost(uint32, tag = "28")]
        GoToTabPayload(u32),
        #[prost(string, tag = "29")]
        StartOrReloadPluginPayload(::prost::alloc::string::String),
        #[prost(uint32, tag = "30")]
        CloseTerminalPanePayload(u32),
        #[prost(uint32, tag = "31")]
        ClosePluginPanePayload(u32),
        #[prost(message, tag = "32")]
        FocusTerminalPanePayload(super::super::action::PaneIdAndShouldFloat),
        #[prost(message, tag = "33")]
        FocusPluginPanePayload(super::super::action::PaneIdAndShouldFloat),
        #[prost(message, tag = "34")]
        RenameTerminalPanePayload(super::IdAndNewName),
        #[prost(message, tag = "35")]
        RenamePluginPanePayload(super::IdAndNewName),
        #[prost(message, tag = "36")]
        RenameTabPayload(super::IdAndNewName),
        #[prost(string, tag = "37")]
        ReportCrashPayload(::prost::alloc::string::String),
        #[prost(message, tag = "38")]
        RequestPluginPermissionPayload(super::RequestPluginPermissionPayload),
        #[prost(message, tag = "39")]
        SwitchSessionPayload(super::SwitchSessionPayload),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchSessionPayload {
    #[prost(string, optional, tag = "1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag = "2")]
    pub tab_position: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag = "3")]
    pub pane_id: ::core::option::Option<u32>,
    #[prost(bool, optional, tag = "4")]
    pub pane_id_is_plugin: ::core::option::Option<bool>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RequestPluginPermissionPayload {
    #[prost(
        enumeration = "super::plugin_permission::PermissionType",
        repeated,
        tag = "1"
    )]
    pub permissions: ::prost::alloc::vec::Vec<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscribePayload {
    #[prost(message, optional, tag = "1")]
    pub subscriptions: ::core::option::Option<super::event::EventNameList>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnsubscribePayload {
    #[prost(message, optional, tag = "1")]
    pub subscriptions: ::core::option::Option<super::event::EventNameList>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFilePayload {
    #[prost(message, optional, tag = "1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenCommandPanePayload {
    #[prost(message, optional, tag = "1")]
    pub command_to_run: ::core::option::Option<super::command::Command>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchTabToPayload {
    #[prost(uint32, tag = "1")]
    pub tab_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTimeoutPayload {
    #[prost(double, tag = "1")]
    pub seconds: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecCmdPayload {
    #[prost(string, repeated, tag = "1")]
    pub command_line: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginMessagePayload {
    #[prost(message, optional, tag = "1")]
    pub message: ::core::option::Option<super::message::Message>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResizePayload {
    #[prost(message, optional, tag = "1")]
    pub resize: ::core::option::Option<super::resize::Resize>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePayload {
    #[prost(message, optional, tag = "1")]
    pub direction: ::core::option::Option<super::resize::MoveDirection>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdAndNewName {
    /// pane id or tab index
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub new_name: ::prost::alloc::string::String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum CommandName {
    Subscribe = 0,
    Unsubscribe = 1,
    SetSelectable = 2,
    GetPluginIds = 3,
    GetZellijVersion = 4,
    OpenFile = 5,
    OpenFileFloating = 6,
    OpenTerminal = 7,
    OpenTerminalFloating = 8,
    OpenCommandPane = 9,
    OpenCommandPaneFloating = 10,
    SwitchTabTo = 11,
    SetTimeout = 12,
    ExecCmd = 13,
    PostMessageTo = 14,
    PostMessageToPlugin = 15,
    HideSelf = 16,
    ShowSelf = 17,
    SwitchToMode = 18,
    NewTabsWithLayout = 19,
    NewTab = 20,
    GoToNextTab = 21,
    GoToPreviousTab = 22,
    Resize = 23,
    ResizeWithDirection = 24,
    FocusNextPane = 25,
    FocusPreviousPane = 26,
    MoveFocus = 27,
    MoveFocusOrTab = 28,
    Detach = 29,
    EditScrollback = 30,
    Write = 31,
    WriteChars = 32,
    ToggleTab = 33,
    MovePane = 34,
    MovePaneWithDirection = 35,
    ClearScreen = 36,
    ScrollUp = 37,
    ScrollDown = 38,
    ScrollToTop = 39,
    ScrollToBottom = 40,
    PageScrollUp = 41,
    PageScrollDown = 42,
    ToggleFocusFullscreen = 43,
    TogglePaneFrames = 44,
    TogglePaneEmbedOrEject = 45,
    UndoRenamePane = 46,
    CloseFocus = 47,
    ToggleActiveTabSync = 48,
    CloseFocusedTab = 49,
    UndoRenameTab = 50,
    QuitZellij = 51,
    PreviousSwapLayout = 52,
    NextSwapLayout = 53,
    GoToTabName = 54,
    FocusOrCreateTab = 55,
    GoToTab = 56,
    StartOrReloadPlugin = 57,
    CloseTerminalPane = 58,
    ClosePluginPane = 59,
    FocusTerminalPane = 60,
    FocusPluginPane = 61,
    RenameTerminalPane = 62,
    RenamePluginPane = 63,
    RenameTab = 64,
    ReportCrash = 65,
    RequestPluginPermissions = 66,
    SwitchSession = 67,
}
impl CommandName {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            CommandName::Subscribe => "Subscribe",
            CommandName::Unsubscribe => "Unsubscribe",
            CommandName::SetSelectable => "SetSelectable",
            CommandName::GetPluginIds => "GetPluginIds",
            CommandName::GetZellijVersion => "GetZellijVersion",
            CommandName::OpenFile => "OpenFile",
            CommandName::OpenFileFloating => "OpenFileFloating",
            CommandName::OpenTerminal => "OpenTerminal",
            CommandName::OpenTerminalFloating => "OpenTerminalFloating",
            CommandName::OpenCommandPane => "OpenCommandPane",
            CommandName::OpenCommandPaneFloating => "OpenCommandPaneFloating",
            CommandName::SwitchTabTo => "SwitchTabTo",
            CommandName::SetTimeout => "SetTimeout",
            CommandName::ExecCmd => "ExecCmd",
            CommandName::PostMessageTo => "PostMessageTo",
            CommandName::PostMessageToPlugin => "PostMessageToPlugin",
            CommandName::HideSelf => "HideSelf",
            CommandName::ShowSelf => "ShowSelf",
            CommandName::SwitchToMode => "SwitchToMode",
            CommandName::NewTabsWithLayout => "NewTabsWithLayout",
            CommandName::NewTab => "NewTab",
            CommandName::GoToNextTab => "GoToNextTab",
            CommandName::GoToPreviousTab => "GoToPreviousTab",
            CommandName::Resize => "Resize",
            CommandName::ResizeWithDirection => "ResizeWithDirection",
            CommandName::FocusNextPane => "FocusNextPane",
            CommandName::FocusPreviousPane => "FocusPreviousPane",
            CommandName::MoveFocus => "MoveFocus",
            CommandName::MoveFocusOrTab => "MoveFocusOrTab",
            CommandName::Detach => "Detach",
            CommandName::EditScrollback => "EditScrollback",
            CommandName::Write => "Write",
            CommandName::WriteChars => "WriteChars",
            CommandName::ToggleTab => "ToggleTab",
            CommandName::MovePane => "MovePane",
            CommandName::MovePaneWithDirection => "MovePaneWithDirection",
            CommandName::ClearScreen => "ClearScreen",
            CommandName::ScrollUp => "ScrollUp",
            CommandName::ScrollDown => "ScrollDown",
            CommandName::ScrollToTop => "ScrollToTop",
            CommandName::ScrollToBottom => "ScrollToBottom",
            CommandName::PageScrollUp => "PageScrollUp",
            CommandName::PageScrollDown => "PageScrollDown",
            CommandName::ToggleFocusFullscreen => "ToggleFocusFullscreen",
            CommandName::TogglePaneFrames => "TogglePaneFrames",
            CommandName::TogglePaneEmbedOrEject => "TogglePaneEmbedOrEject",
            CommandName::UndoRenamePane => "UndoRenamePane",
            CommandName::CloseFocus => "CloseFocus",
            CommandName::ToggleActiveTabSync => "ToggleActiveTabSync",
            CommandName::CloseFocusedTab => "CloseFocusedTab",
            CommandName::UndoRenameTab => "UndoRenameTab",
            CommandName::QuitZellij => "QuitZellij",
            CommandName::PreviousSwapLayout => "PreviousSwapLayout",
            CommandName::NextSwapLayout => "NextSwapLayout",
            CommandName::GoToTabName => "GoToTabName",
            CommandName::FocusOrCreateTab => "FocusOrCreateTab",
            CommandName::GoToTab => "GoToTab",
            CommandName::StartOrReloadPlugin => "StartOrReloadPlugin",
            CommandName::CloseTerminalPane => "CloseTerminalPane",
            CommandName::ClosePluginPane => "ClosePluginPane",
            CommandName::FocusTerminalPane => "FocusTerminalPane",
            CommandName::FocusPluginPane => "FocusPluginPane",
            CommandName::RenameTerminalPane => "RenameTerminalPane",
            CommandName::RenamePluginPane => "RenamePluginPane",
            CommandName::RenameTab => "RenameTab",
            CommandName::ReportCrash => "ReportCrash",
            CommandName::RequestPluginPermissions => "RequestPluginPermissions",
            CommandName::SwitchSession => "SwitchSession",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Subscribe" => Some(Self::Subscribe),
            "Unsubscribe" => Some(Self::Unsubscribe),
            "SetSelectable" => Some(Self::SetSelectable),
            "GetPluginIds" => Some(Self::GetPluginIds),
            "GetZellijVersion" => Some(Self::GetZellijVersion),
            "OpenFile" => Some(Self::OpenFile),
            "OpenFileFloating" => Some(Self::OpenFileFloating),
            "OpenTerminal" => Some(Self::OpenTerminal),
            "OpenTerminalFloating" => Some(Self::OpenTerminalFloating),
            "OpenCommandPane" => Some(Self::OpenCommandPane),
            "OpenCommandPaneFloating" => Some(Self::OpenCommandPaneFloating),
            "SwitchTabTo" => Some(Self::SwitchTabTo),
            "SetTimeout" => Some(Self::SetTimeout),
            "ExecCmd" => Some(Self::ExecCmd),
            "PostMessageTo" => Some(Self::PostMessageTo),
            "PostMessageToPlugin" => Some(Self::PostMessageToPlugin),
            "HideSelf" => Some(Self::HideSelf),
            "ShowSelf" => Some(Self::ShowSelf),
            "SwitchToMode" => Some(Self::SwitchToMode),
            "NewTabsWithLayout" => Some(Self::NewTabsWithLayout),
            "NewTab" => Some(Self::NewTab),
            "GoToNextTab" => Some(Self::GoToNextTab),
            "GoToPreviousTab" => Some(Self::GoToPreviousTab),
            "Resize" => Some(Self::Resize),
            "ResizeWithDirection" => Some(Self::ResizeWithDirection),
            "FocusNextPane" => Some(Self::FocusNextPane),
            "FocusPreviousPane" => Some(Self::FocusPreviousPane),
            "MoveFocus" => Some(Self::MoveFocus),
            "MoveFocusOrTab" => Some(Self::MoveFocusOrTab),
            "Detach" => Some(Self::Detach),
            "EditScrollback" => Some(Self::EditScrollback),
            "Write" => Some(Self::Write),
            "WriteChars" => Some(Self::WriteChars),
            "ToggleTab" => Some(Self::ToggleTab),
            "MovePane" => Some(Self::MovePane),
            "MovePaneWithDirection" => Some(Self::MovePaneWithDirection),
            "ClearScreen" => Some(Self::ClearScreen),
            "ScrollUp" => Some(Self::ScrollUp),
            "ScrollDown" => Some(Self::ScrollDown),
            "ScrollToTop" => Some(Self::ScrollToTop),
            "ScrollToBottom" => Some(Self::ScrollToBottom),
            "PageScrollUp" => Some(Self::PageScrollUp),
            "PageScrollDown" => Some(Self::PageScrollDown),
            "ToggleFocusFullscreen" => Some(Self::ToggleFocusFullscreen),
            "TogglePaneFrames" => Some(Self::TogglePaneFrames),
            "TogglePaneEmbedOrEject" => Some(Self::TogglePaneEmbedOrEject),
            "UndoRenamePane" => Some(Self::UndoRenamePane),
            "CloseFocus" => Some(Self::CloseFocus),
            "ToggleActiveTabSync" => Some(Self::ToggleActiveTabSync),
            "CloseFocusedTab" => Some(Self::CloseFocusedTab),
            "UndoRenameTab" => Some(Self::UndoRenameTab),
            "QuitZellij" => Some(Self::QuitZellij),
            "PreviousSwapLayout" => Some(Self::PreviousSwapLayout),
            "NextSwapLayout" => Some(Self::NextSwapLayout),
            "GoToTabName" => Some(Self::GoToTabName),
            "FocusOrCreateTab" => Some(Self::FocusOrCreateTab),
            "GoToTab" => Some(Self::GoToTab),
            "StartOrReloadPlugin" => Some(Self::StartOrReloadPlugin),
            "CloseTerminalPane" => Some(Self::CloseTerminalPane),
            "ClosePluginPane" => Some(Self::ClosePluginPane),
            "FocusTerminalPane" => Some(Self::FocusTerminalPane),
            "FocusPluginPane" => Some(Self::FocusPluginPane),
            "RenameTerminalPane" => Some(Self::RenameTerminalPane),
            "RenamePluginPane" => Some(Self::RenamePluginPane),
            "RenameTab" => Some(Self::RenameTab),
            "ReportCrash" => Some(Self::ReportCrash),
            "RequestPluginPermissions" => Some(Self::RequestPluginPermissions),
            "SwitchSession" => Some(Self::SwitchSession),
            _ => None,
        }
    }
}
