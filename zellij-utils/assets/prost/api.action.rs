#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Action {
    #[prost(enumeration = "ActionName", tag = "1")]
    pub name: i32,
    #[prost(
        oneof = "action::OptionalPayload",
        tags = "2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48"
    )]
    pub optional_payload: ::core::option::Option<action::OptionalPayload>,
}
/// Nested message and enum types in `Action`.
pub mod action {
    #[allow(clippy::derive_partial_eq_without_eq)]
    #[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum OptionalPayload {
        #[prost(message, tag = "2")]
        SwitchToModePayload(super::SwitchToModePayload),
        #[prost(message, tag = "3")]
        WritePayload(super::WritePayload),
        #[prost(message, tag = "4")]
        WriteCharsPayload(super::WriteCharsPayload),
        #[prost(message, tag = "5")]
        SwitchModeForAllClientsPayload(super::SwitchToModePayload),
        #[prost(message, tag = "6")]
        ResizePayload(super::super::resize::Resize),
        #[prost(enumeration = "super::super::resize::ResizeDirection", tag = "7")]
        MoveFocusPayload(i32),
        #[prost(enumeration = "super::super::resize::ResizeDirection", tag = "8")]
        MoveFocusOrTabPayload(i32),
        #[prost(message, tag = "9")]
        MovePanePayload(super::MovePanePayload),
        #[prost(message, tag = "10")]
        DumpScreenPayload(super::DumpScreenPayload),
        #[prost(message, tag = "11")]
        ScrollUpAtPayload(super::ScrollAtPayload),
        #[prost(message, tag = "12")]
        ScrollDownAtPayload(super::ScrollAtPayload),
        #[prost(message, tag = "13")]
        NewPanePayload(super::NewPanePayload),
        #[prost(message, tag = "14")]
        EditFilePayload(super::EditFilePayload),
        #[prost(message, tag = "15")]
        NewFloatingPanePayload(super::NewFloatingPanePayload),
        #[prost(message, tag = "16")]
        NewTiledPanePayload(super::NewTiledPanePayload),
        #[prost(bytes, tag = "17")]
        PaneNameInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(uint32, tag = "18")]
        GoToTabPayload(u32),
        #[prost(message, tag = "19")]
        GoToTabNamePayload(super::GoToTabNamePayload),
        #[prost(bytes, tag = "20")]
        TabNameInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(message, tag = "21")]
        RunPayload(super::RunCommandAction),
        #[prost(message, tag = "22")]
        LeftClickPayload(super::Position),
        #[prost(message, tag = "23")]
        RightClickPayload(super::Position),
        #[prost(message, tag = "24")]
        MiddleClickPayload(super::Position),
        #[prost(message, tag = "25")]
        LaunchOrFocusPluginPayload(super::LaunchOrFocusPluginPayload),
        #[prost(message, tag = "26")]
        LeftMouseReleasePayload(super::Position),
        #[prost(message, tag = "27")]
        RightMouseReleasePayload(super::Position),
        #[prost(message, tag = "28")]
        MiddleMouseReleasePayload(super::Position),
        #[prost(message, tag = "29")]
        MouseHoldLeftPayload(super::Position),
        #[prost(message, tag = "30")]
        MouseHoldRightPayload(super::Position),
        #[prost(message, tag = "31")]
        MouseHoldMiddlePayload(super::Position),
        #[prost(bytes, tag = "32")]
        SearchInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(enumeration = "super::SearchDirection", tag = "33")]
        SearchPayload(i32),
        #[prost(enumeration = "super::SearchOption", tag = "34")]
        SearchToggleOptionPayload(i32),
        #[prost(message, tag = "35")]
        NewTiledPluginPanePayload(super::NewPluginPanePayload),
        #[prost(message, tag = "36")]
        NewFloatingPluginPanePayload(super::NewPluginPanePayload),
        #[prost(string, tag = "37")]
        StartOrReloadPluginPayload(::prost::alloc::string::String),
        #[prost(uint32, tag = "38")]
        CloseTerminalPanePayload(u32),
        #[prost(uint32, tag = "39")]
        ClosePluginPanePayload(u32),
        #[prost(message, tag = "40")]
        FocusTerminalPaneWithIdPayload(super::PaneIdAndShouldFloat),
        #[prost(message, tag = "41")]
        FocusPluginPaneWithIdPayload(super::PaneIdAndShouldFloat),
        #[prost(message, tag = "42")]
        RenameTerminalPanePayload(super::IdAndName),
        #[prost(message, tag = "43")]
        RenamePluginPanePayload(super::IdAndName),
        #[prost(message, tag = "44")]
        RenameTabPayload(super::IdAndName),
        #[prost(string, tag = "45")]
        RenameSessionPayload(::prost::alloc::string::String),
        #[prost(message, tag = "46")]
        LaunchPluginPayload(super::LaunchOrFocusPluginPayload),
        #[prost(message, tag = "47")]
        MessagePayload(super::CliPipePayload),
        #[prost(enumeration = "super::MoveTabDirection", tag = "48")]
        MoveTabPayload(i32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliPipePayload {
    #[prost(string, optional, tag = "1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, tag = "2")]
    pub payload: ::prost::alloc::string::String,
    #[prost(message, repeated, tag = "3")]
    pub args: ::prost::alloc::vec::Vec<NameAndValue>,
    #[prost(string, optional, tag = "4")]
    pub plugin: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdAndName {
    #[prost(bytes = "vec", tag = "1")]
    pub name: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag = "2")]
    pub id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneIdAndShouldFloat {
    #[prost(uint32, tag = "1")]
    pub pane_id: u32,
    #[prost(bool, tag = "2")]
    pub should_float: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPluginPanePayload {
    #[prost(string, tag = "1")]
    pub plugin_url: ::prost::alloc::string::String,
    #[prost(string, optional, tag = "2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag = "3")]
    pub skip_plugin_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LaunchOrFocusPluginPayload {
    #[prost(string, tag = "1")]
    pub plugin_url: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub should_float: bool,
    #[prost(message, optional, tag = "3")]
    pub plugin_configuration: ::core::option::Option<PluginConfiguration>,
    #[prost(bool, tag = "4")]
    pub move_to_focused_tab: bool,
    #[prost(bool, tag = "5")]
    pub should_open_in_place: bool,
    #[prost(bool, tag = "6")]
    pub skip_plugin_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GoToTabNamePayload {
    #[prost(string, tag = "1")]
    pub tab_name: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub create: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewFloatingPanePayload {
    #[prost(message, optional, tag = "1")]
    pub command: ::core::option::Option<RunCommandAction>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTiledPanePayload {
    #[prost(message, optional, tag = "1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(enumeration = "super::resize::ResizeDirection", optional, tag = "2")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePanePayload {
    #[prost(enumeration = "super::resize::ResizeDirection", optional, tag = "1")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EditFilePayload {
    #[prost(string, tag = "1")]
    pub file_to_edit: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag = "2")]
    pub line_number: ::core::option::Option<u32>,
    #[prost(string, optional, tag = "3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration = "super::resize::ResizeDirection", optional, tag = "4")]
    pub direction: ::core::option::Option<i32>,
    #[prost(bool, tag = "5")]
    pub should_float: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollAtPayload {
    #[prost(message, optional, tag = "1")]
    pub position: ::core::option::Option<Position>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPanePayload {
    #[prost(enumeration = "super::resize::ResizeDirection", optional, tag = "1")]
    pub direction: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchToModePayload {
    #[prost(enumeration = "super::input_mode::InputMode", tag = "1")]
    pub input_mode: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WritePayload {
    #[prost(bytes = "vec", tag = "1")]
    pub bytes_to_write: ::prost::alloc::vec::Vec<u8>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteCharsPayload {
    #[prost(string, tag = "1")]
    pub chars: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DumpScreenPayload {
    #[prost(string, tag = "1")]
    pub file_path: ::prost::alloc::string::String,
    #[prost(bool, tag = "2")]
    pub include_scrollback: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Position {
    #[prost(int64, tag = "1")]
    pub line: i64,
    #[prost(int64, tag = "2")]
    pub column: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunCommandAction {
    #[prost(string, tag = "1")]
    pub command: ::prost::alloc::string::String,
    #[prost(string, repeated, tag = "2")]
    pub args: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, optional, tag = "3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration = "super::resize::ResizeDirection", optional, tag = "4")]
    pub direction: ::core::option::Option<i32>,
    #[prost(string, optional, tag = "5")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag = "6")]
    pub hold_on_close: bool,
    #[prost(bool, tag = "7")]
    pub hold_on_start: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginConfiguration {
    #[prost(message, repeated, tag = "1")]
    pub name_and_value: ::prost::alloc::vec::Vec<NameAndValue>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NameAndValue {
    #[prost(string, tag = "1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub value: ::prost::alloc::string::String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SearchDirection {
    Up = 0,
    Down = 1,
}
impl SearchDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SearchDirection::Up => "Up",
            SearchDirection::Down => "Down",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SearchOption {
    CaseSensitivity = 0,
    WholeWord = 1,
    Wrap = 2,
}
impl SearchOption {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SearchOption::CaseSensitivity => "CaseSensitivity",
            SearchOption::WholeWord => "WholeWord",
            SearchOption::Wrap => "Wrap",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "CaseSensitivity" => Some(Self::CaseSensitivity),
            "WholeWord" => Some(Self::WholeWord),
            "Wrap" => Some(Self::Wrap),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MoveTabDirection {
    Left = 0,
    Right = 1,
}
impl MoveTabDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MoveTabDirection::Left => "Left",
            MoveTabDirection::Right => "Right",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Left" => Some(Self::Left),
            "Right" => Some(Self::Right),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ActionName {
    Quit = 0,
    Write = 1,
    WriteChars = 2,
    SwitchToMode = 3,
    SwitchModeForAllClients = 4,
    Resize = 5,
    FocusNextPane = 6,
    FocusPreviousPane = 7,
    SwitchFocus = 8,
    MoveFocus = 9,
    MoveFocusOrTab = 10,
    MovePane = 11,
    MovePaneBackwards = 12,
    ClearScreen = 13,
    DumpScreen = 14,
    EditScrollback = 15,
    ScrollUp = 16,
    ScrollUpAt = 17,
    ScrollDown = 18,
    ScrollDownAt = 19,
    ScrollToBottom = 20,
    ScrollToTop = 21,
    PageScrollUp = 22,
    PageScrollDown = 23,
    HalfPageScrollUp = 24,
    HalfPageScrollDown = 25,
    ToggleFocusFullscreen = 26,
    TogglePaneFrames = 27,
    ToggleActiveSyncTab = 28,
    NewPane = 29,
    EditFile = 30,
    NewFloatingPane = 31,
    NewTiledPane = 32,
    TogglePaneEmbedOrFloating = 33,
    ToggleFloatingPanes = 34,
    CloseFocus = 35,
    PaneNameInput = 36,
    UndoRenamePane = 37,
    NewTab = 38,
    NoOp = 39,
    GoToNextTab = 40,
    GoToPreviousTab = 41,
    CloseTab = 42,
    GoToTab = 43,
    GoToTabName = 44,
    ToggleTab = 45,
    TabNameInput = 46,
    UndoRenameTab = 47,
    Run = 48,
    Detach = 49,
    LeftClick = 50,
    RightClick = 51,
    MiddleClick = 52,
    LaunchOrFocusPlugin = 53,
    LeftMouseRelease = 54,
    RightMouseRelease = 55,
    MiddleMouseRelease = 56,
    MouseHoldLeft = 57,
    MouseHoldRight = 58,
    MouseHoldMiddle = 59,
    SearchInput = 60,
    Search = 61,
    SearchToggleOption = 62,
    ToggleMouseMode = 63,
    PreviousSwapLayout = 64,
    NextSwapLayout = 65,
    QueryTabNames = 66,
    NewTiledPluginPane = 67,
    NewFloatingPluginPane = 68,
    StartOrReloadPlugin = 69,
    CloseTerminalPane = 70,
    ClosePluginPane = 71,
    FocusTerminalPaneWithId = 72,
    FocusPluginPaneWithId = 73,
    RenameTerminalPane = 74,
    RenamePluginPane = 75,
    RenameTab = 76,
    BreakPane = 77,
    BreakPaneRight = 78,
    BreakPaneLeft = 79,
    RenameSession = 80,
    LaunchPlugin = 81,
    CliPipe = 82,
    MoveTab = 83,
    KeybindPipe = 84,
}
impl ActionName {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ActionName::Quit => "Quit",
            ActionName::Write => "Write",
            ActionName::WriteChars => "WriteChars",
            ActionName::SwitchToMode => "SwitchToMode",
            ActionName::SwitchModeForAllClients => "SwitchModeForAllClients",
            ActionName::Resize => "Resize",
            ActionName::FocusNextPane => "FocusNextPane",
            ActionName::FocusPreviousPane => "FocusPreviousPane",
            ActionName::SwitchFocus => "SwitchFocus",
            ActionName::MoveFocus => "MoveFocus",
            ActionName::MoveFocusOrTab => "MoveFocusOrTab",
            ActionName::MovePane => "MovePane",
            ActionName::MovePaneBackwards => "MovePaneBackwards",
            ActionName::ClearScreen => "ClearScreen",
            ActionName::DumpScreen => "DumpScreen",
            ActionName::EditScrollback => "EditScrollback",
            ActionName::ScrollUp => "ScrollUp",
            ActionName::ScrollUpAt => "ScrollUpAt",
            ActionName::ScrollDown => "ScrollDown",
            ActionName::ScrollDownAt => "ScrollDownAt",
            ActionName::ScrollToBottom => "ScrollToBottom",
            ActionName::ScrollToTop => "ScrollToTop",
            ActionName::PageScrollUp => "PageScrollUp",
            ActionName::PageScrollDown => "PageScrollDown",
            ActionName::HalfPageScrollUp => "HalfPageScrollUp",
            ActionName::HalfPageScrollDown => "HalfPageScrollDown",
            ActionName::ToggleFocusFullscreen => "ToggleFocusFullscreen",
            ActionName::TogglePaneFrames => "TogglePaneFrames",
            ActionName::ToggleActiveSyncTab => "ToggleActiveSyncTab",
            ActionName::NewPane => "NewPane",
            ActionName::EditFile => "EditFile",
            ActionName::NewFloatingPane => "NewFloatingPane",
            ActionName::NewTiledPane => "NewTiledPane",
            ActionName::TogglePaneEmbedOrFloating => "TogglePaneEmbedOrFloating",
            ActionName::ToggleFloatingPanes => "ToggleFloatingPanes",
            ActionName::CloseFocus => "CloseFocus",
            ActionName::PaneNameInput => "PaneNameInput",
            ActionName::UndoRenamePane => "UndoRenamePane",
            ActionName::NewTab => "NewTab",
            ActionName::NoOp => "NoOp",
            ActionName::GoToNextTab => "GoToNextTab",
            ActionName::GoToPreviousTab => "GoToPreviousTab",
            ActionName::CloseTab => "CloseTab",
            ActionName::GoToTab => "GoToTab",
            ActionName::GoToTabName => "GoToTabName",
            ActionName::ToggleTab => "ToggleTab",
            ActionName::TabNameInput => "TabNameInput",
            ActionName::UndoRenameTab => "UndoRenameTab",
            ActionName::Run => "Run",
            ActionName::Detach => "Detach",
            ActionName::LeftClick => "LeftClick",
            ActionName::RightClick => "RightClick",
            ActionName::MiddleClick => "MiddleClick",
            ActionName::LaunchOrFocusPlugin => "LaunchOrFocusPlugin",
            ActionName::LeftMouseRelease => "LeftMouseRelease",
            ActionName::RightMouseRelease => "RightMouseRelease",
            ActionName::MiddleMouseRelease => "MiddleMouseRelease",
            ActionName::MouseHoldLeft => "MouseHoldLeft",
            ActionName::MouseHoldRight => "MouseHoldRight",
            ActionName::MouseHoldMiddle => "MouseHoldMiddle",
            ActionName::SearchInput => "SearchInput",
            ActionName::Search => "Search",
            ActionName::SearchToggleOption => "SearchToggleOption",
            ActionName::ToggleMouseMode => "ToggleMouseMode",
            ActionName::PreviousSwapLayout => "PreviousSwapLayout",
            ActionName::NextSwapLayout => "NextSwapLayout",
            ActionName::QueryTabNames => "QueryTabNames",
            ActionName::NewTiledPluginPane => "NewTiledPluginPane",
            ActionName::NewFloatingPluginPane => "NewFloatingPluginPane",
            ActionName::StartOrReloadPlugin => "StartOrReloadPlugin",
            ActionName::CloseTerminalPane => "CloseTerminalPane",
            ActionName::ClosePluginPane => "ClosePluginPane",
            ActionName::FocusTerminalPaneWithId => "FocusTerminalPaneWithId",
            ActionName::FocusPluginPaneWithId => "FocusPluginPaneWithId",
            ActionName::RenameTerminalPane => "RenameTerminalPane",
            ActionName::RenamePluginPane => "RenamePluginPane",
            ActionName::RenameTab => "RenameTab",
            ActionName::BreakPane => "BreakPane",
            ActionName::BreakPaneRight => "BreakPaneRight",
            ActionName::BreakPaneLeft => "BreakPaneLeft",
            ActionName::RenameSession => "RenameSession",
            ActionName::LaunchPlugin => "LaunchPlugin",
            ActionName::CliPipe => "CliPipe",
            ActionName::MoveTab => "MoveTab",
            ActionName::KeybindPipe => "KeybindPipe",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Quit" => Some(Self::Quit),
            "Write" => Some(Self::Write),
            "WriteChars" => Some(Self::WriteChars),
            "SwitchToMode" => Some(Self::SwitchToMode),
            "SwitchModeForAllClients" => Some(Self::SwitchModeForAllClients),
            "Resize" => Some(Self::Resize),
            "FocusNextPane" => Some(Self::FocusNextPane),
            "FocusPreviousPane" => Some(Self::FocusPreviousPane),
            "SwitchFocus" => Some(Self::SwitchFocus),
            "MoveFocus" => Some(Self::MoveFocus),
            "MoveFocusOrTab" => Some(Self::MoveFocusOrTab),
            "MovePane" => Some(Self::MovePane),
            "MovePaneBackwards" => Some(Self::MovePaneBackwards),
            "ClearScreen" => Some(Self::ClearScreen),
            "DumpScreen" => Some(Self::DumpScreen),
            "EditScrollback" => Some(Self::EditScrollback),
            "ScrollUp" => Some(Self::ScrollUp),
            "ScrollUpAt" => Some(Self::ScrollUpAt),
            "ScrollDown" => Some(Self::ScrollDown),
            "ScrollDownAt" => Some(Self::ScrollDownAt),
            "ScrollToBottom" => Some(Self::ScrollToBottom),
            "ScrollToTop" => Some(Self::ScrollToTop),
            "PageScrollUp" => Some(Self::PageScrollUp),
            "PageScrollDown" => Some(Self::PageScrollDown),
            "HalfPageScrollUp" => Some(Self::HalfPageScrollUp),
            "HalfPageScrollDown" => Some(Self::HalfPageScrollDown),
            "ToggleFocusFullscreen" => Some(Self::ToggleFocusFullscreen),
            "TogglePaneFrames" => Some(Self::TogglePaneFrames),
            "ToggleActiveSyncTab" => Some(Self::ToggleActiveSyncTab),
            "NewPane" => Some(Self::NewPane),
            "EditFile" => Some(Self::EditFile),
            "NewFloatingPane" => Some(Self::NewFloatingPane),
            "NewTiledPane" => Some(Self::NewTiledPane),
            "TogglePaneEmbedOrFloating" => Some(Self::TogglePaneEmbedOrFloating),
            "ToggleFloatingPanes" => Some(Self::ToggleFloatingPanes),
            "CloseFocus" => Some(Self::CloseFocus),
            "PaneNameInput" => Some(Self::PaneNameInput),
            "UndoRenamePane" => Some(Self::UndoRenamePane),
            "NewTab" => Some(Self::NewTab),
            "NoOp" => Some(Self::NoOp),
            "GoToNextTab" => Some(Self::GoToNextTab),
            "GoToPreviousTab" => Some(Self::GoToPreviousTab),
            "CloseTab" => Some(Self::CloseTab),
            "GoToTab" => Some(Self::GoToTab),
            "GoToTabName" => Some(Self::GoToTabName),
            "ToggleTab" => Some(Self::ToggleTab),
            "TabNameInput" => Some(Self::TabNameInput),
            "UndoRenameTab" => Some(Self::UndoRenameTab),
            "Run" => Some(Self::Run),
            "Detach" => Some(Self::Detach),
            "LeftClick" => Some(Self::LeftClick),
            "RightClick" => Some(Self::RightClick),
            "MiddleClick" => Some(Self::MiddleClick),
            "LaunchOrFocusPlugin" => Some(Self::LaunchOrFocusPlugin),
            "LeftMouseRelease" => Some(Self::LeftMouseRelease),
            "RightMouseRelease" => Some(Self::RightMouseRelease),
            "MiddleMouseRelease" => Some(Self::MiddleMouseRelease),
            "MouseHoldLeft" => Some(Self::MouseHoldLeft),
            "MouseHoldRight" => Some(Self::MouseHoldRight),
            "MouseHoldMiddle" => Some(Self::MouseHoldMiddle),
            "SearchInput" => Some(Self::SearchInput),
            "Search" => Some(Self::Search),
            "SearchToggleOption" => Some(Self::SearchToggleOption),
            "ToggleMouseMode" => Some(Self::ToggleMouseMode),
            "PreviousSwapLayout" => Some(Self::PreviousSwapLayout),
            "NextSwapLayout" => Some(Self::NextSwapLayout),
            "QueryTabNames" => Some(Self::QueryTabNames),
            "NewTiledPluginPane" => Some(Self::NewTiledPluginPane),
            "NewFloatingPluginPane" => Some(Self::NewFloatingPluginPane),
            "StartOrReloadPlugin" => Some(Self::StartOrReloadPlugin),
            "CloseTerminalPane" => Some(Self::CloseTerminalPane),
            "ClosePluginPane" => Some(Self::ClosePluginPane),
            "FocusTerminalPaneWithId" => Some(Self::FocusTerminalPaneWithId),
            "FocusPluginPaneWithId" => Some(Self::FocusPluginPaneWithId),
            "RenameTerminalPane" => Some(Self::RenameTerminalPane),
            "RenamePluginPane" => Some(Self::RenamePluginPane),
            "RenameTab" => Some(Self::RenameTab),
            "BreakPane" => Some(Self::BreakPane),
            "BreakPaneRight" => Some(Self::BreakPaneRight),
            "BreakPaneLeft" => Some(Self::BreakPaneLeft),
            "RenameSession" => Some(Self::RenameSession),
            "LaunchPlugin" => Some(Self::LaunchPlugin),
            "CliPipe" => Some(Self::CliPipe),
            "MoveTab" => Some(Self::MoveTab),
            "KeybindPipe" => Some(Self::KeybindPipe),
            _ => None,
        }
    }
}
