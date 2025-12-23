#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PercentOrFixed {
    #[prost(oneof="percent_or_fixed::SizeType", tags="1, 2")]
    pub size_type: ::core::option::Option<percent_or_fixed::SizeType>,
}
/// Nested message and enum types in `PercentOrFixed`.
pub mod percent_or_fixed {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum SizeType {
        #[prost(uint32, tag="1")]
        Percent(u32),
        #[prost(uint32, tag="2")]
        Fixed(u32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutConstraintWithValue {
    #[prost(enumeration="LayoutConstraint", tag="1")]
    pub constraint_type: i32,
    #[prost(uint32, optional, tag="2")]
    pub value: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginUserConfiguration {
    #[prost(map="string, string", tag="1")]
    pub configuration: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunPluginLocationData {
    #[prost(enumeration="RunPluginLocation", tag="1")]
    pub location_type: i32,
    #[prost(oneof="run_plugin_location_data::LocationData", tags="2, 3, 4")]
    pub location_data: ::core::option::Option<run_plugin_location_data::LocationData>,
}
/// Nested message and enum types in `RunPluginLocationData`.
pub mod run_plugin_location_data {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum LocationData {
        #[prost(string, tag="2")]
        FilePath(::prost::alloc::string::String),
        #[prost(message, tag="3")]
        ZellijTag(super::PluginTag),
        #[prost(string, tag="4")]
        RemoteUrl(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginTag {
    #[prost(string, tag="1")]
    pub tag: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunPlugin {
    #[prost(bool, tag="1")]
    pub allow_exec_host_cmd: bool,
    #[prost(message, optional, tag="2")]
    pub location: ::core::option::Option<RunPluginLocationData>,
    #[prost(message, optional, tag="3")]
    pub configuration: ::core::option::Option<PluginUserConfiguration>,
    #[prost(string, optional, tag="4")]
    pub initial_cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginAlias {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub configuration: ::core::option::Option<PluginUserConfiguration>,
    #[prost(string, optional, tag="3")]
    pub initial_cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag="4")]
    pub run_plugin: ::core::option::Option<RunPlugin>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunPluginOrAlias {
    #[prost(oneof="run_plugin_or_alias::PluginType", tags="1, 2")]
    pub plugin_type: ::core::option::Option<run_plugin_or_alias::PluginType>,
}
/// Nested message and enum types in `RunPluginOrAlias`.
pub mod run_plugin_or_alias {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PluginType {
        #[prost(message, tag="1")]
        Plugin(super::RunPlugin),
        #[prost(message, tag="2")]
        Alias(super::PluginAlias),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunEditFileAction {
    #[prost(string, tag="1")]
    pub file_path: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag="2")]
    pub line_number: ::core::option::Option<u32>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneRun {
    #[prost(oneof="pane_run::RunType", tags="1, 2, 3, 4")]
    pub run_type: ::core::option::Option<pane_run::RunType>,
}
/// Nested message and enum types in `PaneRun`.
pub mod pane_run {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum RunType {
        #[prost(message, tag="1")]
        Command(super::RunCommandAction),
        #[prost(message, tag="2")]
        Plugin(super::RunPluginOrAlias),
        #[prost(message, tag="3")]
        EditFile(super::RunEditFileAction),
        #[prost(string, tag="4")]
        Cwd(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TiledPaneLayout {
    #[prost(enumeration="SplitDirection", tag="1")]
    pub children_split_direction: i32,
    #[prost(string, optional, tag="2")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="3")]
    pub children: ::prost::alloc::vec::Vec<TiledPaneLayout>,
    #[prost(message, optional, tag="4")]
    pub split_size: ::core::option::Option<SplitSize>,
    #[prost(message, optional, tag="5")]
    pub run: ::core::option::Option<PaneRun>,
    #[prost(bool, tag="6")]
    pub borderless: bool,
    #[prost(string, optional, tag="7")]
    pub focus: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag="8")]
    pub exclude_from_sync: ::core::option::Option<bool>,
    #[prost(bool, tag="9")]
    pub children_are_stacked: bool,
    #[prost(uint32, optional, tag="10")]
    pub external_children_index: ::core::option::Option<u32>,
    #[prost(bool, tag="11")]
    pub is_expanded_in_stack: bool,
    #[prost(bool, tag="12")]
    pub hide_floating_panes: bool,
    #[prost(string, optional, tag="13")]
    pub pane_initial_contents: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingPaneLayout {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag="2")]
    pub height: ::core::option::Option<PercentOrFixed>,
    #[prost(message, optional, tag="3")]
    pub width: ::core::option::Option<PercentOrFixed>,
    #[prost(message, optional, tag="4")]
    pub x: ::core::option::Option<PercentOrFixed>,
    #[prost(message, optional, tag="5")]
    pub y: ::core::option::Option<PercentOrFixed>,
    #[prost(bool, optional, tag="6")]
    pub pinned: ::core::option::Option<bool>,
    #[prost(message, optional, tag="7")]
    pub run: ::core::option::Option<PaneRun>,
    #[prost(bool, optional, tag="8")]
    pub focus: ::core::option::Option<bool>,
    #[prost(bool, tag="9")]
    pub already_running: bool,
    #[prost(string, optional, tag="10")]
    pub pane_initial_contents: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="11")]
    pub logical_position: ::core::option::Option<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwapTiledLayout {
    #[prost(message, repeated, tag="1")]
    pub constraint_map: ::prost::alloc::vec::Vec<LayoutConstraintTiledPair>,
    #[prost(string, optional, tag="2")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwapFloatingLayout {
    #[prost(message, repeated, tag="1")]
    pub constraint_map: ::prost::alloc::vec::Vec<LayoutConstraintFloatingPair>,
    #[prost(string, optional, tag="2")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutConstraintTiledPair {
    #[prost(message, optional, tag="1")]
    pub constraint: ::core::option::Option<LayoutConstraintWithValue>,
    #[prost(message, optional, tag="2")]
    pub layout: ::core::option::Option<TiledPaneLayout>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutConstraintFloatingPair {
    #[prost(message, optional, tag="1")]
    pub constraint: ::core::option::Option<LayoutConstraintWithValue>,
    #[prost(message, repeated, tag="2")]
    pub layouts: ::prost::alloc::vec::Vec<FloatingPaneLayout>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CommandOrPlugin {
    #[prost(oneof="command_or_plugin::CommandOrPluginType", tags="1, 2")]
    pub command_or_plugin_type: ::core::option::Option<command_or_plugin::CommandOrPluginType>,
}
/// Nested message and enum types in `CommandOrPlugin`.
pub mod command_or_plugin {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum CommandOrPluginType {
        #[prost(message, tag="1")]
        Command(super::RunCommandAction),
        #[prost(message, tag="2")]
        Plugin(super::RunPluginOrAlias),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTabPayload {
    #[prost(message, optional, tag="1")]
    pub tiled_layout: ::core::option::Option<TiledPaneLayout>,
    #[prost(message, repeated, tag="2")]
    pub floating_layouts: ::prost::alloc::vec::Vec<FloatingPaneLayout>,
    #[prost(message, repeated, tag="3")]
    pub swap_tiled_layouts: ::prost::alloc::vec::Vec<SwapTiledLayout>,
    #[prost(message, repeated, tag="4")]
    pub swap_floating_layouts: ::prost::alloc::vec::Vec<SwapFloatingLayout>,
    #[prost(string, optional, tag="5")]
    pub tab_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="6")]
    pub should_change_focus_to_new_tab: bool,
    #[prost(string, optional, tag="7")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="8")]
    pub initial_panes: ::prost::alloc::vec::Vec<CommandOrPlugin>,
    #[prost(enumeration="UnblockCondition", optional, tag="9")]
    pub first_pane_unblock_condition: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OverrideLayoutPayload {
    #[prost(message, optional, tag="1")]
    pub tiled_layout: ::core::option::Option<TiledPaneLayout>,
    #[prost(message, repeated, tag="2")]
    pub floating_layouts: ::prost::alloc::vec::Vec<FloatingPaneLayout>,
    #[prost(message, repeated, tag="3")]
    pub swap_tiled_layouts: ::prost::alloc::vec::Vec<SwapTiledLayout>,
    #[prost(message, repeated, tag="4")]
    pub swap_floating_layouts: ::prost::alloc::vec::Vec<SwapFloatingLayout>,
    #[prost(string, optional, tag="5")]
    pub tab_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="10")]
    pub retain_existing_terminal_panes: bool,
    #[prost(bool, tag="11")]
    pub retain_existing_plugin_panes: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Action {
    #[prost(enumeration="ActionName", tag="1")]
    pub name: i32,
    #[prost(oneof="action::OptionalPayload", tags="2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53")]
    pub optional_payload: ::core::option::Option<action::OptionalPayload>,
}
/// Nested message and enum types in `Action`.
pub mod action {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum OptionalPayload {
        #[prost(message, tag="2")]
        SwitchToModePayload(super::SwitchToModePayload),
        #[prost(message, tag="3")]
        WritePayload(super::WritePayload),
        #[prost(message, tag="4")]
        WriteCharsPayload(super::WriteCharsPayload),
        #[prost(message, tag="5")]
        SwitchModeForAllClientsPayload(super::SwitchToModePayload),
        #[prost(message, tag="6")]
        ResizePayload(super::super::resize::Resize),
        #[prost(enumeration="super::super::resize::ResizeDirection", tag="7")]
        MoveFocusPayload(i32),
        #[prost(enumeration="super::super::resize::ResizeDirection", tag="8")]
        MoveFocusOrTabPayload(i32),
        #[prost(message, tag="9")]
        MovePanePayload(super::MovePanePayload),
        #[prost(message, tag="10")]
        DumpScreenPayload(super::DumpScreenPayload),
        #[prost(message, tag="11")]
        ScrollUpAtPayload(super::ScrollAtPayload),
        #[prost(message, tag="12")]
        ScrollDownAtPayload(super::ScrollAtPayload),
        #[prost(message, tag="13")]
        NewPanePayload(super::NewPanePayload),
        #[prost(message, tag="14")]
        EditFilePayload(super::EditFilePayload),
        #[prost(message, tag="15")]
        NewFloatingPanePayload(super::NewFloatingPanePayload),
        #[prost(message, tag="16")]
        NewTiledPanePayload(super::NewTiledPanePayload),
        #[prost(bytes, tag="17")]
        PaneNameInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(uint32, tag="18")]
        GoToTabPayload(u32),
        #[prost(message, tag="19")]
        GoToTabNamePayload(super::GoToTabNamePayload),
        #[prost(bytes, tag="20")]
        TabNameInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(message, tag="21")]
        RunPayload(super::RunCommandAction),
        #[prost(message, tag="22")]
        LeftClickPayload(super::Position),
        #[prost(message, tag="23")]
        RightClickPayload(super::Position),
        #[prost(message, tag="24")]
        MiddleClickPayload(super::Position),
        #[prost(message, tag="25")]
        LaunchOrFocusPluginPayload(super::LaunchOrFocusPluginPayload),
        #[prost(message, tag="26")]
        LeftMouseReleasePayload(super::Position),
        #[prost(message, tag="27")]
        RightMouseReleasePayload(super::Position),
        #[prost(message, tag="28")]
        MiddleMouseReleasePayload(super::Position),
        #[prost(bytes, tag="32")]
        SearchInputPayload(::prost::alloc::vec::Vec<u8>),
        #[prost(enumeration="super::SearchDirection", tag="33")]
        SearchPayload(i32),
        #[prost(enumeration="super::SearchOption", tag="34")]
        SearchToggleOptionPayload(i32),
        #[prost(message, tag="35")]
        NewTiledPluginPanePayload(super::NewPluginPanePayload),
        #[prost(message, tag="36")]
        NewFloatingPluginPanePayload(super::NewPluginPanePayload),
        #[prost(string, tag="37")]
        StartOrReloadPluginPayload(::prost::alloc::string::String),
        #[prost(uint32, tag="38")]
        CloseTerminalPanePayload(u32),
        #[prost(uint32, tag="39")]
        ClosePluginPanePayload(u32),
        #[prost(message, tag="40")]
        FocusTerminalPaneWithIdPayload(super::PaneIdAndShouldFloat),
        #[prost(message, tag="41")]
        FocusPluginPaneWithIdPayload(super::PaneIdAndShouldFloat),
        #[prost(message, tag="42")]
        RenameTerminalPanePayload(super::IdAndName),
        #[prost(message, tag="43")]
        RenamePluginPanePayload(super::IdAndName),
        #[prost(message, tag="44")]
        RenameTabPayload(super::IdAndName),
        #[prost(string, tag="45")]
        RenameSessionPayload(::prost::alloc::string::String),
        #[prost(message, tag="46")]
        LaunchPluginPayload(super::LaunchOrFocusPluginPayload),
        #[prost(message, tag="47")]
        MessagePayload(super::CliPipePayload),
        #[prost(enumeration="super::MoveTabDirection", tag="48")]
        MoveTabPayload(i32),
        #[prost(message, tag="49")]
        MouseEventPayload(super::MouseEventPayload),
        #[prost(message, tag="50")]
        NewBlockingPanePayload(super::NewBlockingPanePayload),
        #[prost(message, tag="51")]
        NewTabPayload(super::NewTabPayload),
        #[prost(message, tag="52")]
        NewInPlacePanePayload(super::NewInPlacePanePayload),
        #[prost(message, tag="53")]
        OverrideLayoutPayload(super::OverrideLayoutPayload),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliPipePayload {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, tag="2")]
    pub payload: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="3")]
    pub args: ::prost::alloc::vec::Vec<NameAndValue>,
    #[prost(string, optional, tag="4")]
    pub plugin: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdAndName {
    #[prost(bytes="vec", tag="1")]
    pub name: ::prost::alloc::vec::Vec<u8>,
    #[prost(uint32, tag="2")]
    pub id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneIdAndShouldFloat {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(bool, tag="2")]
    pub should_float: bool,
    #[prost(bool, tag="3")]
    pub should_be_in_place: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPluginPanePayload {
    #[prost(string, tag="1")]
    pub plugin_url: ::prost::alloc::string::String,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub skip_plugin_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LaunchOrFocusPluginPayload {
    #[prost(string, tag="1")]
    pub plugin_url: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub should_float: bool,
    #[prost(message, optional, tag="3")]
    pub plugin_configuration: ::core::option::Option<PluginConfiguration>,
    #[prost(bool, tag="4")]
    pub move_to_focused_tab: bool,
    #[prost(bool, tag="5")]
    pub should_open_in_place: bool,
    #[prost(bool, tag="6")]
    pub skip_plugin_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GoToTabNamePayload {
    #[prost(string, tag="1")]
    pub tab_name: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub create: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewFloatingPanePayload {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(bool, tag="2")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTiledPanePayload {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="2")]
    pub direction: ::core::option::Option<i32>,
    #[prost(bool, tag="3")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePanePayload {
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EditFilePayload {
    #[prost(string, tag="1")]
    pub file_to_edit: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag="2")]
    pub line_number: ::core::option::Option<u32>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="4")]
    pub direction: ::core::option::Option<i32>,
    #[prost(bool, tag="5")]
    pub should_float: bool,
    #[prost(bool, tag="6")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollAtPayload {
    #[prost(message, optional, tag="1")]
    pub position: ::core::option::Option<Position>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPanePayload {
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockingPanePayload {
    #[prost(message, optional, tag="1")]
    pub placement: ::core::option::Option<NewPanePlacement>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag="3")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(enumeration="UnblockCondition", optional, tag="4")]
    pub unblock_condition: ::core::option::Option<i32>,
    #[prost(bool, tag="5")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewInPlacePanePayload {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub near_current_pane: bool,
    #[prost(message, optional, tag="4")]
    pub pane_id_to_replace: ::core::option::Option<PaneId>,
    #[prost(bool, tag="5")]
    pub close_replace_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchToModePayload {
    #[prost(enumeration="super::input_mode::InputMode", tag="1")]
    pub input_mode: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WritePayload {
    #[prost(bytes="vec", tag="1")]
    pub bytes_to_write: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag="2")]
    pub key_with_modifier: ::core::option::Option<KeyWithModifier>,
    #[prost(bool, tag="3")]
    pub is_kitty_keyboard_protocol: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteCharsPayload {
    #[prost(string, tag="1")]
    pub chars: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DumpScreenPayload {
    #[prost(string, tag="1")]
    pub file_path: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub include_scrollback: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Position {
    #[prost(int64, tag="1")]
    pub line: i64,
    #[prost(int64, tag="2")]
    pub column: i64,
}
/// SplitSize represents a dimension that can be either a percentage or fixed size
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SplitSize {
    #[prost(oneof="split_size::SplitSizeVariant", tags="1, 2")]
    pub split_size_variant: ::core::option::Option<split_size::SplitSizeVariant>,
}
/// Nested message and enum types in `SplitSize`.
pub mod split_size {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum SplitSizeVariant {
        /// 1 to 100
        #[prost(uint32, tag="1")]
        Percent(u32),
        /// absolute number of columns or rows
        #[prost(uint32, tag="2")]
        Fixed(u32),
    }
}
/// PaneId identifies either a terminal or plugin pane
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneId {
    #[prost(oneof="pane_id::PaneIdVariant", tags="1, 2")]
    pub pane_id_variant: ::core::option::Option<pane_id::PaneIdVariant>,
}
/// Nested message and enum types in `PaneId`.
pub mod pane_id {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PaneIdVariant {
        #[prost(uint32, tag="1")]
        Terminal(u32),
        #[prost(uint32, tag="2")]
        Plugin(u32),
    }
}
/// FloatingPaneCoordinates specifies the position and size of a floating pane
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingPaneCoordinates {
    #[prost(message, optional, tag="1")]
    pub x: ::core::option::Option<SplitSize>,
    #[prost(message, optional, tag="2")]
    pub y: ::core::option::Option<SplitSize>,
    #[prost(message, optional, tag="3")]
    pub width: ::core::option::Option<SplitSize>,
    #[prost(message, optional, tag="4")]
    pub height: ::core::option::Option<SplitSize>,
    #[prost(bool, optional, tag="5")]
    pub pinned: ::core::option::Option<bool>,
}
/// NewPanePlacement specifies where a new pane should be placed
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPanePlacement {
    #[prost(oneof="new_pane_placement::PlacementVariant", tags="1, 2, 3, 4, 5")]
    pub placement_variant: ::core::option::Option<new_pane_placement::PlacementVariant>,
}
/// Nested message and enum types in `NewPanePlacement`.
pub mod new_pane_placement {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PlacementVariant {
        #[prost(bool, tag="1")]
        NoPreference(bool),
        #[prost(message, tag="2")]
        Tiled(super::TiledPlacement),
        #[prost(message, tag="3")]
        Floating(super::FloatingPlacement),
        #[prost(message, tag="4")]
        InPlace(super::InPlaceConfig),
        #[prost(message, tag="5")]
        Stacked(super::StackedPlacement),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TiledPlacement {
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingPlacement {
    #[prost(message, optional, tag="1")]
    pub coordinates: ::core::option::Option<FloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct InPlaceConfig {
    #[prost(message, optional, tag="1")]
    pub pane_id_to_replace: ::core::option::Option<PaneId>,
    #[prost(bool, tag="2")]
    pub close_replaced_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StackedPlacement {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MouseEventPayload {
    #[prost(uint32, tag="1")]
    pub event_type: u32,
    #[prost(bool, tag="2")]
    pub left: bool,
    #[prost(bool, tag="3")]
    pub right: bool,
    #[prost(bool, tag="4")]
    pub middle: bool,
    #[prost(bool, tag="5")]
    pub wheel_up: bool,
    #[prost(bool, tag="6")]
    pub wheel_down: bool,
    #[prost(bool, tag="7")]
    pub shift: bool,
    #[prost(bool, tag="8")]
    pub alt: bool,
    #[prost(bool, tag="9")]
    pub ctrl: bool,
    #[prost(int64, tag="10")]
    pub line: i64,
    #[prost(int64, tag="11")]
    pub column: i64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunCommandAction {
    #[prost(string, tag="1")]
    pub command: ::prost::alloc::string::String,
    #[prost(string, repeated, tag="2")]
    pub args: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration="super::resize::ResizeDirection", optional, tag="4")]
    pub direction: ::core::option::Option<i32>,
    #[prost(string, optional, tag="5")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="6")]
    pub hold_on_close: bool,
    #[prost(bool, tag="7")]
    pub hold_on_start: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginConfiguration {
    #[prost(message, repeated, tag="1")]
    pub name_and_value: ::prost::alloc::vec::Vec<NameAndValue>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NameAndValue {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub value: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyWithModifier {
    #[prost(enumeration="BareKey", tag="1")]
    pub bare_key: i32,
    #[prost(enumeration="KeyModifier", repeated, tag="2")]
    pub key_modifiers: ::prost::alloc::vec::Vec<i32>,
    /// Only set when bare_key is CHAR
    #[prost(string, optional, tag="3")]
    pub character: ::core::option::Option<::prost::alloc::string::String>,
}
// Layout and related types

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SplitDirection {
    Unspecified = 0,
    Horizontal = 1,
    Vertical = 2,
}
impl SplitDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SplitDirection::Unspecified => "SPLIT_DIRECTION_UNSPECIFIED",
            SplitDirection::Horizontal => "SPLIT_DIRECTION_HORIZONTAL",
            SplitDirection::Vertical => "SPLIT_DIRECTION_VERTICAL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SPLIT_DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "SPLIT_DIRECTION_HORIZONTAL" => Some(Self::Horizontal),
            "SPLIT_DIRECTION_VERTICAL" => Some(Self::Vertical),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum LayoutConstraint {
    Unspecified = 0,
    MaxPanes = 1,
    MinPanes = 2,
    ExactPanes = 3,
    NoConstraint = 4,
}
impl LayoutConstraint {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            LayoutConstraint::Unspecified => "LAYOUT_CONSTRAINT_UNSPECIFIED",
            LayoutConstraint::MaxPanes => "LAYOUT_CONSTRAINT_MAX_PANES",
            LayoutConstraint::MinPanes => "LAYOUT_CONSTRAINT_MIN_PANES",
            LayoutConstraint::ExactPanes => "LAYOUT_CONSTRAINT_EXACT_PANES",
            LayoutConstraint::NoConstraint => "LAYOUT_CONSTRAINT_NO_CONSTRAINT",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "LAYOUT_CONSTRAINT_UNSPECIFIED" => Some(Self::Unspecified),
            "LAYOUT_CONSTRAINT_MAX_PANES" => Some(Self::MaxPanes),
            "LAYOUT_CONSTRAINT_MIN_PANES" => Some(Self::MinPanes),
            "LAYOUT_CONSTRAINT_EXACT_PANES" => Some(Self::ExactPanes),
            "LAYOUT_CONSTRAINT_NO_CONSTRAINT" => Some(Self::NoConstraint),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum RunPluginLocation {
    Unspecified = 0,
    File = 1,
    Zellij = 2,
    Remote = 3,
}
impl RunPluginLocation {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            RunPluginLocation::Unspecified => "RUN_PLUGIN_LOCATION_UNSPECIFIED",
            RunPluginLocation::File => "RUN_PLUGIN_LOCATION_FILE",
            RunPluginLocation::Zellij => "RUN_PLUGIN_LOCATION_ZELLIJ",
            RunPluginLocation::Remote => "RUN_PLUGIN_LOCATION_REMOTE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "RUN_PLUGIN_LOCATION_UNSPECIFIED" => Some(Self::Unspecified),
            "RUN_PLUGIN_LOCATION_FILE" => Some(Self::File),
            "RUN_PLUGIN_LOCATION_ZELLIJ" => Some(Self::Zellij),
            "RUN_PLUGIN_LOCATION_REMOTE" => Some(Self::Remote),
            _ => None,
        }
    }
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
    TogglePanePinned = 85,
    MouseEvent = 86,
    TogglePaneInGroup = 87,
    ToggleGroupMarking = 88,
    NewStackedPane = 89,
    SwitchSession = 90,
    NewBlockingPane = 91,
    NewInPlacePane = 92,
    OverrideLayout = 93,
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
            ActionName::TogglePanePinned => "TogglePanePinned",
            ActionName::MouseEvent => "MouseEvent",
            ActionName::TogglePaneInGroup => "TogglePaneInGroup",
            ActionName::ToggleGroupMarking => "ToggleGroupMarking",
            ActionName::NewStackedPane => "NewStackedPane",
            ActionName::SwitchSession => "SwitchSession",
            ActionName::NewBlockingPane => "NewBlockingPane",
            ActionName::NewInPlacePane => "NewInPlacePane",
            ActionName::OverrideLayout => "OverrideLayout",
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
            "TogglePanePinned" => Some(Self::TogglePanePinned),
            "MouseEvent" => Some(Self::MouseEvent),
            "TogglePaneInGroup" => Some(Self::TogglePaneInGroup),
            "ToggleGroupMarking" => Some(Self::ToggleGroupMarking),
            "NewStackedPane" => Some(Self::NewStackedPane),
            "SwitchSession" => Some(Self::SwitchSession),
            "NewBlockingPane" => Some(Self::NewBlockingPane),
            "NewInPlacePane" => Some(Self::NewInPlacePane),
            "OverrideLayout" => Some(Self::OverrideLayout),
            _ => None,
        }
    }
}
/// UnblockCondition specifies when a blocking pane should unblock
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum UnblockCondition {
    UnblockOnExitSuccess = 0,
    UnblockOnExitFailure = 1,
    UnblockOnAnyExit = 2,
}
impl UnblockCondition {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            UnblockCondition::UnblockOnExitSuccess => "UNBLOCK_ON_EXIT_SUCCESS",
            UnblockCondition::UnblockOnExitFailure => "UNBLOCK_ON_EXIT_FAILURE",
            UnblockCondition::UnblockOnAnyExit => "UNBLOCK_ON_ANY_EXIT",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "UNBLOCK_ON_EXIT_SUCCESS" => Some(Self::UnblockOnExitSuccess),
            "UNBLOCK_ON_EXIT_FAILURE" => Some(Self::UnblockOnExitFailure),
            "UNBLOCK_ON_ANY_EXIT" => Some(Self::UnblockOnAnyExit),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum BareKey {
    Unspecified = 0,
    PageDown = 1,
    PageUp = 2,
    Left = 3,
    Down = 4,
    Up = 5,
    Right = 6,
    Home = 7,
    End = 8,
    Backspace = 9,
    Delete = 10,
    Insert = 11,
    F1 = 12,
    F2 = 13,
    F3 = 14,
    F4 = 15,
    F5 = 16,
    F6 = 17,
    F7 = 18,
    F8 = 19,
    F9 = 20,
    F10 = 21,
    F11 = 22,
    F12 = 23,
    Char = 24,
    Tab = 25,
    Esc = 26,
    Enter = 27,
    CapsLock = 28,
    ScrollLock = 29,
    NumLock = 30,
    PrintScreen = 31,
    Pause = 32,
    Menu = 33,
}
impl BareKey {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            BareKey::Unspecified => "BARE_KEY_UNSPECIFIED",
            BareKey::PageDown => "BARE_KEY_PAGE_DOWN",
            BareKey::PageUp => "BARE_KEY_PAGE_UP",
            BareKey::Left => "BARE_KEY_LEFT",
            BareKey::Down => "BARE_KEY_DOWN",
            BareKey::Up => "BARE_KEY_UP",
            BareKey::Right => "BARE_KEY_RIGHT",
            BareKey::Home => "BARE_KEY_HOME",
            BareKey::End => "BARE_KEY_END",
            BareKey::Backspace => "BARE_KEY_BACKSPACE",
            BareKey::Delete => "BARE_KEY_DELETE",
            BareKey::Insert => "BARE_KEY_INSERT",
            BareKey::F1 => "BARE_KEY_F1",
            BareKey::F2 => "BARE_KEY_F2",
            BareKey::F3 => "BARE_KEY_F3",
            BareKey::F4 => "BARE_KEY_F4",
            BareKey::F5 => "BARE_KEY_F5",
            BareKey::F6 => "BARE_KEY_F6",
            BareKey::F7 => "BARE_KEY_F7",
            BareKey::F8 => "BARE_KEY_F8",
            BareKey::F9 => "BARE_KEY_F9",
            BareKey::F10 => "BARE_KEY_F10",
            BareKey::F11 => "BARE_KEY_F11",
            BareKey::F12 => "BARE_KEY_F12",
            BareKey::Char => "BARE_KEY_CHAR",
            BareKey::Tab => "BARE_KEY_TAB",
            BareKey::Esc => "BARE_KEY_ESC",
            BareKey::Enter => "BARE_KEY_ENTER",
            BareKey::CapsLock => "BARE_KEY_CAPS_LOCK",
            BareKey::ScrollLock => "BARE_KEY_SCROLL_LOCK",
            BareKey::NumLock => "BARE_KEY_NUM_LOCK",
            BareKey::PrintScreen => "BARE_KEY_PRINT_SCREEN",
            BareKey::Pause => "BARE_KEY_PAUSE",
            BareKey::Menu => "BARE_KEY_MENU",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "BARE_KEY_UNSPECIFIED" => Some(Self::Unspecified),
            "BARE_KEY_PAGE_DOWN" => Some(Self::PageDown),
            "BARE_KEY_PAGE_UP" => Some(Self::PageUp),
            "BARE_KEY_LEFT" => Some(Self::Left),
            "BARE_KEY_DOWN" => Some(Self::Down),
            "BARE_KEY_UP" => Some(Self::Up),
            "BARE_KEY_RIGHT" => Some(Self::Right),
            "BARE_KEY_HOME" => Some(Self::Home),
            "BARE_KEY_END" => Some(Self::End),
            "BARE_KEY_BACKSPACE" => Some(Self::Backspace),
            "BARE_KEY_DELETE" => Some(Self::Delete),
            "BARE_KEY_INSERT" => Some(Self::Insert),
            "BARE_KEY_F1" => Some(Self::F1),
            "BARE_KEY_F2" => Some(Self::F2),
            "BARE_KEY_F3" => Some(Self::F3),
            "BARE_KEY_F4" => Some(Self::F4),
            "BARE_KEY_F5" => Some(Self::F5),
            "BARE_KEY_F6" => Some(Self::F6),
            "BARE_KEY_F7" => Some(Self::F7),
            "BARE_KEY_F8" => Some(Self::F8),
            "BARE_KEY_F9" => Some(Self::F9),
            "BARE_KEY_F10" => Some(Self::F10),
            "BARE_KEY_F11" => Some(Self::F11),
            "BARE_KEY_F12" => Some(Self::F12),
            "BARE_KEY_CHAR" => Some(Self::Char),
            "BARE_KEY_TAB" => Some(Self::Tab),
            "BARE_KEY_ESC" => Some(Self::Esc),
            "BARE_KEY_ENTER" => Some(Self::Enter),
            "BARE_KEY_CAPS_LOCK" => Some(Self::CapsLock),
            "BARE_KEY_SCROLL_LOCK" => Some(Self::ScrollLock),
            "BARE_KEY_NUM_LOCK" => Some(Self::NumLock),
            "BARE_KEY_PRINT_SCREEN" => Some(Self::PrintScreen),
            "BARE_KEY_PAUSE" => Some(Self::Pause),
            "BARE_KEY_MENU" => Some(Self::Menu),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum KeyModifier {
    Unspecified = 0,
    Ctrl = 1,
    Alt = 2,
    Shift = 3,
    Super = 4,
}
impl KeyModifier {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            KeyModifier::Unspecified => "KEY_MODIFIER_UNSPECIFIED",
            KeyModifier::Ctrl => "KEY_MODIFIER_CTRL",
            KeyModifier::Alt => "KEY_MODIFIER_ALT",
            KeyModifier::Shift => "KEY_MODIFIER_SHIFT",
            KeyModifier::Super => "KEY_MODIFIER_SUPER",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "KEY_MODIFIER_UNSPECIFIED" => Some(Self::Unspecified),
            "KEY_MODIFIER_CTRL" => Some(Self::Ctrl),
            "KEY_MODIFIER_ALT" => Some(Self::Alt),
            "KEY_MODIFIER_SHIFT" => Some(Self::Shift),
            "KEY_MODIFIER_SUPER" => Some(Self::Super),
            _ => None,
        }
    }
}
