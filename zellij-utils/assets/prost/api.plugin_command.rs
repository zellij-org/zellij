#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginCommand {
    #[prost(enumeration="CommandName", tag="1")]
    pub name: i32,
    #[prost(oneof="plugin_command::Payload", tags="2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112")]
    pub payload: ::core::option::Option<plugin_command::Payload>,
}
/// Nested message and enum types in `PluginCommand`.
pub mod plugin_command {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Payload {
        #[prost(message, tag="2")]
        SubscribePayload(super::SubscribePayload),
        #[prost(message, tag="3")]
        UnsubscribePayload(super::UnsubscribePayload),
        #[prost(bool, tag="4")]
        SetSelectablePayload(bool),
        #[prost(message, tag="5")]
        OpenFilePayload(super::OpenFilePayload),
        #[prost(message, tag="6")]
        OpenFileFloatingPayload(super::OpenFilePayload),
        #[prost(message, tag="7")]
        OpenTerminalPayload(super::OpenFilePayload),
        #[prost(message, tag="8")]
        OpenTerminalFloatingPayload(super::OpenFilePayload),
        #[prost(message, tag="9")]
        OpenCommandPanePayload(super::OpenCommandPanePayload),
        #[prost(message, tag="10")]
        OpenCommandPaneFloatingPayload(super::OpenCommandPanePayload),
        #[prost(message, tag="11")]
        SwitchTabToPayload(super::SwitchTabToPayload),
        #[prost(message, tag="12")]
        SetTimeoutPayload(super::SetTimeoutPayload),
        #[prost(message, tag="13")]
        ExecCmdPayload(super::ExecCmdPayload),
        #[prost(message, tag="14")]
        PostMessageToPayload(super::PluginMessagePayload),
        #[prost(message, tag="15")]
        PostMessageToPluginPayload(super::PluginMessagePayload),
        #[prost(bool, tag="16")]
        ShowSelfPayload(bool),
        #[prost(message, tag="17")]
        SwitchToModePayload(super::super::action::SwitchToModePayload),
        #[prost(string, tag="18")]
        NewTabsWithLayoutPayload(::prost::alloc::string::String),
        #[prost(message, tag="19")]
        ResizePayload(super::ResizePayload),
        #[prost(message, tag="20")]
        ResizeWithDirectionPayload(super::ResizePayload),
        #[prost(message, tag="21")]
        MoveFocusPayload(super::MovePayload),
        #[prost(message, tag="22")]
        MoveFocusOrTabPayload(super::MovePayload),
        #[prost(bytes, tag="23")]
        WritePayload(::prost::alloc::vec::Vec<u8>),
        #[prost(string, tag="24")]
        WriteCharsPayload(::prost::alloc::string::String),
        #[prost(message, tag="25")]
        MovePaneWithDirectionPayload(super::MovePayload),
        #[prost(string, tag="26")]
        GoToTabNamePayload(::prost::alloc::string::String),
        #[prost(string, tag="27")]
        FocusOrCreateTabPayload(::prost::alloc::string::String),
        #[prost(uint32, tag="28")]
        GoToTabPayload(u32),
        #[prost(string, tag="29")]
        StartOrReloadPluginPayload(::prost::alloc::string::String),
        #[prost(uint32, tag="30")]
        CloseTerminalPanePayload(u32),
        #[prost(uint32, tag="31")]
        ClosePluginPanePayload(u32),
        #[prost(message, tag="32")]
        FocusTerminalPanePayload(super::super::action::PaneIdAndShouldFloat),
        #[prost(message, tag="33")]
        FocusPluginPanePayload(super::super::action::PaneIdAndShouldFloat),
        #[prost(message, tag="34")]
        RenameTerminalPanePayload(super::IdAndNewName),
        #[prost(message, tag="35")]
        RenamePluginPanePayload(super::IdAndNewName),
        #[prost(message, tag="36")]
        RenameTabPayload(super::IdAndNewName),
        #[prost(string, tag="37")]
        ReportCrashPayload(::prost::alloc::string::String),
        #[prost(message, tag="38")]
        RequestPluginPermissionPayload(super::RequestPluginPermissionPayload),
        #[prost(message, tag="39")]
        SwitchSessionPayload(super::SwitchSessionPayload),
        #[prost(message, tag="40")]
        OpenFileInPlacePayload(super::OpenFilePayload),
        #[prost(message, tag="41")]
        OpenTerminalInPlacePayload(super::OpenFilePayload),
        #[prost(message, tag="42")]
        OpenCommandPaneInPlacePayload(super::OpenCommandPanePayload),
        #[prost(message, tag="43")]
        RunCommandPayload(super::RunCommandPayload),
        #[prost(message, tag="44")]
        WebRequestPayload(super::WebRequestPayload),
        #[prost(string, tag="45")]
        DeleteDeadSessionPayload(::prost::alloc::string::String),
        #[prost(string, tag="46")]
        RenameSessionPayload(::prost::alloc::string::String),
        #[prost(string, tag="47")]
        UnblockCliPipeInputPayload(::prost::alloc::string::String),
        #[prost(string, tag="48")]
        BlockCliPipeInputPayload(::prost::alloc::string::String),
        #[prost(message, tag="49")]
        CliPipeOutputPayload(super::CliPipeOutputPayload),
        #[prost(message, tag="50")]
        MessageToPluginPayload(super::MessageToPluginPayload),
        #[prost(message, tag="60")]
        KillSessionsPayload(super::KillSessionsPayload),
        #[prost(string, tag="61")]
        ScanHostFolderPayload(::prost::alloc::string::String),
        #[prost(message, tag="62")]
        NewTabsWithLayoutInfoPayload(super::NewTabsWithLayoutInfoPayload),
        #[prost(message, tag="63")]
        ReconfigurePayload(super::ReconfigurePayload),
        #[prost(message, tag="64")]
        HidePaneWithIdPayload(super::HidePaneWithIdPayload),
        #[prost(message, tag="65")]
        ShowPaneWithIdPayload(super::ShowPaneWithIdPayload),
        #[prost(message, tag="66")]
        OpenCommandPaneBackgroundPayload(super::OpenCommandPanePayload),
        #[prost(message, tag="67")]
        RerunCommandPanePayload(super::RerunCommandPanePayload),
        #[prost(message, tag="68")]
        ResizePaneIdWithDirectionPayload(super::ResizePaneIdWithDirectionPayload),
        #[prost(message, tag="69")]
        EditScrollbackForPaneWithIdPayload(super::EditScrollbackForPaneWithIdPayload),
        #[prost(message, tag="70")]
        WriteToPaneIdPayload(super::WriteToPaneIdPayload),
        #[prost(message, tag="71")]
        WriteCharsToPaneIdPayload(super::WriteCharsToPaneIdPayload),
        #[prost(message, tag="72")]
        MovePaneWithPaneIdPayload(super::MovePaneWithPaneIdPayload),
        #[prost(message, tag="73")]
        MovePaneWithPaneIdInDirectionPayload(super::MovePaneWithPaneIdInDirectionPayload),
        #[prost(message, tag="74")]
        ClearScreenForPaneIdPayload(super::ClearScreenForPaneIdPayload),
        #[prost(message, tag="75")]
        ScrollUpInPaneIdPayload(super::ScrollUpInPaneIdPayload),
        #[prost(message, tag="76")]
        ScrollDownInPaneIdPayload(super::ScrollDownInPaneIdPayload),
        #[prost(message, tag="77")]
        ScrollToTopInPaneIdPayload(super::ScrollToTopInPaneIdPayload),
        #[prost(message, tag="78")]
        ScrollToBottomInPaneIdPayload(super::ScrollToBottomInPaneIdPayload),
        #[prost(message, tag="79")]
        PageScrollUpInPaneIdPayload(super::PageScrollUpInPaneIdPayload),
        #[prost(message, tag="80")]
        PageScrollDownInPaneIdPayload(super::PageScrollDownInPaneIdPayload),
        #[prost(message, tag="81")]
        TogglePaneIdFullscreenPayload(super::TogglePaneIdFullscreenPayload),
        #[prost(message, tag="82")]
        TogglePaneEmbedOrEjectForPaneIdPayload(super::TogglePaneEmbedOrEjectForPaneIdPayload),
        #[prost(message, tag="83")]
        CloseTabWithIndexPayload(super::CloseTabWithIndexPayload),
        #[prost(message, tag="84")]
        BreakPanesToNewTabPayload(super::BreakPanesToNewTabPayload),
        #[prost(message, tag="85")]
        BreakPanesToTabWithIndexPayload(super::BreakPanesToTabWithIndexPayload),
        #[prost(message, tag="86")]
        ReloadPluginPayload(super::ReloadPluginPayload),
        #[prost(message, tag="87")]
        LoadNewPluginPayload(super::LoadNewPluginPayload),
        #[prost(message, tag="88")]
        RebindKeysPayload(super::RebindKeysPayload),
        #[prost(message, tag="89")]
        ChangeHostFolderPayload(super::ChangeHostFolderPayload),
        #[prost(message, tag="90")]
        SetFloatingPanePinnedPayload(super::SetFloatingPanePinnedPayload),
        #[prost(message, tag="91")]
        StackPanesPayload(super::StackPanesPayload),
        #[prost(message, tag="92")]
        ChangeFloatingPanesCoordinatesPayload(super::ChangeFloatingPanesCoordinatesPayload),
        #[prost(message, tag="93")]
        OpenCommandPaneNearPluginPayload(super::OpenCommandPaneNearPluginPayload),
        #[prost(message, tag="94")]
        OpenTerminalNearPluginPayload(super::OpenTerminalNearPluginPayload),
        #[prost(message, tag="95")]
        OpenTerminalFloatingNearPluginPayload(super::OpenTerminalFloatingNearPluginPayload),
        #[prost(message, tag="96")]
        OpenTerminalInPlaceOfPluginPayload(super::OpenTerminalInPlaceOfPluginPayload),
        #[prost(message, tag="97")]
        OpenCommandPaneFloatingNearPluginPayload(super::OpenCommandPaneFloatingNearPluginPayload),
        #[prost(message, tag="98")]
        OpenCommandPaneInPlaceOfPluginPayload(super::OpenCommandPaneInPlaceOfPluginPayload),
        #[prost(message, tag="99")]
        OpenFileNearPluginPayload(super::OpenFileNearPluginPayload),
        #[prost(message, tag="100")]
        OpenFileFloatingNearPluginPayload(super::OpenFileFloatingNearPluginPayload),
        #[prost(message, tag="101")]
        OpenFileInPlaceOfPluginPayload(super::OpenFileInPlaceOfPluginPayload),
        #[prost(message, tag="102")]
        GroupAndUngroupPanesPayload(super::GroupAndUngroupPanesPayload),
        #[prost(message, tag="103")]
        HighlightAndUnhighlightPanesPayload(super::HighlightAndUnhighlightPanesPayload),
        #[prost(message, tag="104")]
        CloseMultiplePanesPayload(super::CloseMultiplePanesPayload),
        #[prost(message, tag="105")]
        FloatMultiplePanesPayload(super::FloatMultiplePanesPayload),
        #[prost(message, tag="106")]
        EmbedMultiplePanesPayload(super::EmbedMultiplePanesPayload),
        #[prost(message, tag="107")]
        SetSelfMouseSelectionSupportPayload(super::SetSelfMouseSelectionSupportPayload),
        #[prost(message, tag="108")]
        GenerateWebLoginTokenPayload(super::GenerateWebLoginTokenPayload),
        #[prost(message, tag="109")]
        RevokeWebLoginTokenPayload(super::RevokeWebLoginTokenPayload),
        #[prost(message, tag="110")]
        RenameWebLoginTokenPayload(super::RenameWebLoginTokenPayload),
        #[prost(message, tag="111")]
        ReplacePaneWithExistingPanePayload(super::ReplacePaneWithExistingPanePayload),
        #[prost(message, tag="112")]
        NewTabPayload(super::NewTabPayload),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTabPayload {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="2")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReplacePaneWithExistingPanePayload {
    #[prost(message, optional, tag="1")]
    pub pane_id_to_replace: ::core::option::Option<PaneId>,
    #[prost(message, optional, tag="2")]
    pub existing_pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenameWebLoginTokenPayload {
    #[prost(string, tag="1")]
    pub old_name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub new_name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RevokeWebLoginTokenPayload {
    #[prost(string, tag="1")]
    pub token_label: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GenerateWebLoginTokenPayload {
    #[prost(string, optional, tag="1")]
    pub token_label: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetSelfMouseSelectionSupportPayload {
    #[prost(bool, tag="1")]
    pub support_mouse_selection: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EmbedMultiplePanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatMultiplePanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CloseMultiplePanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HighlightAndUnhighlightPanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids_to_highlight: ::prost::alloc::vec::Vec<PaneId>,
    #[prost(message, repeated, tag="2")]
    pub pane_ids_to_unhighlight: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GroupAndUngroupPanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids_to_group: ::prost::alloc::vec::Vec<PaneId>,
    #[prost(message, repeated, tag="2")]
    pub pane_ids_to_ungroup: ::prost::alloc::vec::Vec<PaneId>,
    #[prost(bool, tag="3")]
    pub for_all_clients: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFileInPlaceOfPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(bool, tag="4")]
    pub close_plugin_after_replace: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFileFloatingNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFileNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenCommandPaneInPlaceOfPluginPayload {
    #[prost(message, optional, tag="1")]
    pub command_to_run: ::core::option::Option<super::command::Command>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(bool, tag="4")]
    pub close_plugin_after_replace: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenCommandPaneFloatingNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub command_to_run: ::core::option::Option<super::command::Command>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenTerminalInPlaceOfPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(bool, tag="4")]
    pub close_plugin_after_replace: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenTerminalFloatingNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenTerminalNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenCommandPaneNearPluginPayload {
    #[prost(message, optional, tag="1")]
    pub command_to_run: ::core::option::Option<super::command::Command>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChangeFloatingPanesCoordinatesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids_and_floating_panes_coordinates: ::prost::alloc::vec::Vec<PaneIdAndFloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StackPanesPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetFloatingPanePinnedPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
    #[prost(bool, tag="2")]
    pub should_be_pinned: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChangeHostFolderPayload {
    #[prost(string, tag="1")]
    pub new_host_folder: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RebindKeysPayload {
    #[prost(message, repeated, tag="1")]
    pub keys_to_rebind: ::prost::alloc::vec::Vec<KeyToRebind>,
    #[prost(message, repeated, tag="2")]
    pub keys_to_unbind: ::prost::alloc::vec::Vec<KeyToUnbind>,
    #[prost(bool, tag="3")]
    pub write_config_to_disk: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyToRebind {
    #[prost(enumeration="super::input_mode::InputMode", tag="1")]
    pub input_mode: i32,
    #[prost(message, optional, tag="2")]
    pub key: ::core::option::Option<super::key::Key>,
    #[prost(message, repeated, tag="3")]
    pub actions: ::prost::alloc::vec::Vec<super::action::Action>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyToUnbind {
    #[prost(enumeration="super::input_mode::InputMode", tag="1")]
    pub input_mode: i32,
    #[prost(message, optional, tag="2")]
    pub key: ::core::option::Option<super::key::Key>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LoadNewPluginPayload {
    #[prost(string, tag="1")]
    pub plugin_url: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="2")]
    pub plugin_config: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(bool, tag="3")]
    pub should_load_plugin_in_background: bool,
    #[prost(bool, tag="4")]
    pub should_skip_plugin_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReloadPluginPayload {
    #[prost(uint32, tag="1")]
    pub plugin_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BreakPanesToTabWithIndexPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
    #[prost(uint32, tag="2")]
    pub tab_index: u32,
    #[prost(bool, tag="3")]
    pub should_change_focus_to_target_tab: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BreakPanesToNewTabPayload {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
    #[prost(bool, tag="2")]
    pub should_change_focus_to_new_tab: bool,
    #[prost(string, optional, tag="3")]
    pub new_tab_name: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePaneWithPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePaneWithPaneIdInDirectionPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
    #[prost(message, optional, tag="2")]
    pub direction: ::core::option::Option<super::resize::MoveDirection>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClearScreenForPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollUpInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollDownInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollToTopInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollToBottomInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PageScrollUpInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PageScrollDownInPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePaneIdFullscreenPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePaneEmbedOrEjectForPaneIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CloseTabWithIndexPayload {
    #[prost(uint32, tag="1")]
    pub tab_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteCharsToPaneIdPayload {
    #[prost(string, tag="1")]
    pub chars_to_write: ::prost::alloc::string::String,
    #[prost(message, optional, tag="2")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteToPaneIdPayload {
    #[prost(bytes="vec", tag="1")]
    pub bytes_to_write: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, optional, tag="2")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EditScrollbackForPaneWithIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResizePaneIdWithDirectionPayload {
    #[prost(message, optional, tag="1")]
    pub resize: ::core::option::Option<super::resize::Resize>,
    #[prost(message, optional, tag="2")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ReconfigurePayload {
    #[prost(string, tag="1")]
    pub config: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub write_to_disk: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RerunCommandPanePayload {
    #[prost(uint32, tag="1")]
    pub terminal_pane_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HidePaneWithIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ShowPaneWithIdPayload {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
    #[prost(bool, tag="2")]
    pub should_float_if_hidden: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTabsWithLayoutInfoPayload {
    #[prost(message, optional, tag="1")]
    pub layout_info: ::core::option::Option<super::event::LayoutInfo>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KillSessionsPayload {
    #[prost(string, repeated, tag="1")]
    pub session_names: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliPipeOutputPayload {
    #[prost(string, tag="1")]
    pub pipe_name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub output: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MessageToPluginPayload {
    #[prost(string, optional, tag="1")]
    pub plugin_url: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="2")]
    pub plugin_config: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(string, tag="3")]
    pub message_name: ::prost::alloc::string::String,
    #[prost(string, optional, tag="4")]
    pub message_payload: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="5")]
    pub message_args: ::prost::alloc::vec::Vec<ContextItem>,
    #[prost(message, optional, tag="6")]
    pub new_plugin_args: ::core::option::Option<NewPluginArgs>,
    #[prost(uint32, optional, tag="7")]
    pub destination_plugin_id: ::core::option::Option<u32>,
    #[prost(message, optional, tag="8")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPluginArgs {
    #[prost(bool, optional, tag="1")]
    pub should_float: ::core::option::Option<bool>,
    #[prost(message, optional, tag="2")]
    pub pane_id_to_replace: ::core::option::Option<PaneId>,
    #[prost(string, optional, tag="3")]
    pub pane_title: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="4")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="5")]
    pub skip_cache: bool,
    #[prost(bool, optional, tag="6")]
    pub should_focus: ::core::option::Option<bool>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneId {
    #[prost(enumeration="PaneType", tag="1")]
    pub pane_type: i32,
    #[prost(uint32, tag="2")]
    pub id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchSessionPayload {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="2")]
    pub tab_position: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag="3")]
    pub pane_id: ::core::option::Option<u32>,
    #[prost(bool, optional, tag="4")]
    pub pane_id_is_plugin: ::core::option::Option<bool>,
    #[prost(message, optional, tag="5")]
    pub layout: ::core::option::Option<super::event::LayoutInfo>,
    #[prost(string, optional, tag="6")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RequestPluginPermissionPayload {
    #[prost(enumeration="super::plugin_permission::PermissionType", repeated, tag="1")]
    pub permissions: ::prost::alloc::vec::Vec<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SubscribePayload {
    #[prost(message, optional, tag="1")]
    pub subscriptions: ::core::option::Option<super::event::EventNameList>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnsubscribePayload {
    #[prost(message, optional, tag="1")]
    pub subscriptions: ::core::option::Option<super::event::EventNameList>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFilePayload {
    #[prost(message, optional, tag="1")]
    pub file_to_open: ::core::option::Option<super::file::File>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenCommandPanePayload {
    #[prost(message, optional, tag="1")]
    pub command_to_run: ::core::option::Option<super::command::Command>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(message, repeated, tag="3")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchTabToPayload {
    #[prost(uint32, tag="1")]
    pub tab_index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SetTimeoutPayload {
    #[prost(double, tag="1")]
    pub seconds: f64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExecCmdPayload {
    #[prost(string, repeated, tag="1")]
    pub command_line: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunCommandPayload {
    #[prost(string, repeated, tag="1")]
    pub command_line: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(message, repeated, tag="2")]
    pub env_variables: ::prost::alloc::vec::Vec<EnvVariable>,
    #[prost(string, tag="3")]
    pub cwd: ::prost::alloc::string::String,
    #[prost(message, repeated, tag="4")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WebRequestPayload {
    #[prost(string, tag="1")]
    pub url: ::prost::alloc::string::String,
    #[prost(enumeration="HttpVerb", tag="2")]
    pub verb: i32,
    #[prost(message, repeated, tag="3")]
    pub headers: ::prost::alloc::vec::Vec<super::event::Header>,
    #[prost(bytes="vec", tag="4")]
    pub body: ::prost::alloc::vec::Vec<u8>,
    #[prost(message, repeated, tag="5")]
    pub context: ::prost::alloc::vec::Vec<ContextItem>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnvVariable {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub value: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ContextItem {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub value: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PluginMessagePayload {
    #[prost(message, optional, tag="1")]
    pub message: ::core::option::Option<super::message::Message>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResizePayload {
    #[prost(message, optional, tag="1")]
    pub resize: ::core::option::Option<super::resize::Resize>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePayload {
    #[prost(message, optional, tag="1")]
    pub direction: ::core::option::Option<super::resize::MoveDirection>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneIdAndFloatingPaneCoordinates {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
    #[prost(message, optional, tag="2")]
    pub floating_pane_coordinates: ::core::option::Option<FloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct IdAndNewName {
    /// pane id or tab index
    #[prost(uint32, tag="1")]
    pub id: u32,
    #[prost(string, tag="2")]
    pub new_name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingPaneCoordinates {
    #[prost(message, optional, tag="1")]
    pub x: ::core::option::Option<FixedOrPercentValue>,
    #[prost(message, optional, tag="2")]
    pub y: ::core::option::Option<FixedOrPercentValue>,
    #[prost(message, optional, tag="3")]
    pub width: ::core::option::Option<FixedOrPercentValue>,
    #[prost(message, optional, tag="4")]
    pub height: ::core::option::Option<FixedOrPercentValue>,
    #[prost(bool, optional, tag="5")]
    pub pinned: ::core::option::Option<bool>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FixedOrPercentValue {
    #[prost(enumeration="FixedOrPercent", tag="1")]
    pub r#type: i32,
    #[prost(uint32, tag="2")]
    pub value: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CreateTokenResponse {
    #[prost(string, optional, tag="1")]
    pub token: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="2")]
    pub token_label: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RevokeTokenResponse {
    #[prost(bool, tag="1")]
    pub successfully_revoked: bool,
    #[prost(string, optional, tag="2")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListTokensResponse {
    /// tokens/creation_times should be synchronized
    #[prost(string, repeated, tag="1")]
    pub tokens: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, repeated, tag="2")]
    pub creation_times: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RevokeAllWebTokensResponse {
    #[prost(bool, tag="1")]
    pub successfully_revoked: bool,
    #[prost(string, optional, tag="2")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenameWebTokenResponse {
    #[prost(bool, tag="1")]
    pub successfully_renamed: bool,
    #[prost(string, optional, tag="2")]
    pub error: ::core::option::Option<::prost::alloc::string::String>,
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
    OpenTerminalInPlace = 68,
    OpenCommandInPlace = 69,
    OpenFileInPlace = 70,
    RunCommand = 71,
    WebRequest = 72,
    DeleteDeadSession = 73,
    DeleteAllDeadSessions = 74,
    RenameSession = 75,
    UnblockCliPipeInput = 76,
    BlockCliPipeInput = 77,
    CliPipeOutput = 78,
    MessageToPlugin = 79,
    DisconnectOtherClients = 80,
    KillSessions = 81,
    ScanHostFolder = 82,
    WatchFilesystem = 83,
    DumpSessionLayout = 84,
    CloseSelf = 85,
    NewTabsWithLayoutInfo = 86,
    Reconfigure = 87,
    HidePaneWithId = 88,
    ShowPaneWithId = 89,
    OpenCommandPaneBackground = 90,
    RerunCommandPane = 91,
    ResizePaneIdWithDirection = 92,
    EditScrollbackForPaneWithId = 93,
    WriteToPaneId = 94,
    WriteCharsToPaneId = 95,
    MovePaneWithPaneId = 96,
    MovePaneWithPaneIdInDirection = 97,
    ClearScreenForPaneId = 98,
    ScrollUpInPaneId = 99,
    ScrollDownInPaneId = 100,
    ScrollToTopInPaneId = 101,
    ScrollToBottomInPaneId = 102,
    PageScrollUpInPaneId = 103,
    PageScrollDownInPaneId = 104,
    TogglePaneIdFullscreen = 105,
    TogglePaneEmbedOrEjectForPaneId = 106,
    CloseTabWithIndex = 107,
    BreakPanesToNewTab = 108,
    BreakPanesToTabWithIndex = 109,
    ReloadPlugin = 110,
    LoadNewPlugin = 111,
    RebindKeys = 112,
    ListClients = 113,
    ChangeHostFolder = 114,
    SetFloatingPanePinned = 115,
    StackPanes = 116,
    ChangeFloatingPanesCoordinates = 117,
    OpenCommandPaneNearPlugin = 118,
    OpenTerminalNearPlugin = 119,
    OpenTerminalFloatingNearPlugin = 120,
    OpenTerminalInPlaceOfPlugin = 121,
    OpenCommandPaneFloatingNearPlugin = 122,
    OpenCommandPaneInPlaceOfPlugin = 123,
    OpenFileNearPlugin = 124,
    OpenFileFloatingNearPlugin = 125,
    OpenFileInPlaceOfPlugin = 126,
    StartWebServer = 127,
    GroupAndUngroupPanes = 128,
    HighlightAndUnhighlightPanes = 129,
    CloseMultiplePanes = 130,
    FloatMultiplePanes = 131,
    EmbedMultiplePanes = 132,
    ShareCurrentSession = 133,
    StopSharingCurrentSession = 134,
    StopWebServer = 135,
    QueryWebServerStatus = 136,
    SetSelfMouseSelectionSupport = 137,
    GenerateWebLoginToken = 138,
    RevokeWebLoginToken = 139,
    ListWebLoginTokens = 140,
    RevokeAllWebLoginTokens = 141,
    RenameWebLoginToken = 142,
    InterceptKeyPresses = 143,
    ClearKeyPressesIntercepts = 144,
    ReplacePaneWithExistingPane = 155,
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
            CommandName::OpenTerminalInPlace => "OpenTerminalInPlace",
            CommandName::OpenCommandInPlace => "OpenCommandInPlace",
            CommandName::OpenFileInPlace => "OpenFileInPlace",
            CommandName::RunCommand => "RunCommand",
            CommandName::WebRequest => "WebRequest",
            CommandName::DeleteDeadSession => "DeleteDeadSession",
            CommandName::DeleteAllDeadSessions => "DeleteAllDeadSessions",
            CommandName::RenameSession => "RenameSession",
            CommandName::UnblockCliPipeInput => "UnblockCliPipeInput",
            CommandName::BlockCliPipeInput => "BlockCliPipeInput",
            CommandName::CliPipeOutput => "CliPipeOutput",
            CommandName::MessageToPlugin => "MessageToPlugin",
            CommandName::DisconnectOtherClients => "DisconnectOtherClients",
            CommandName::KillSessions => "KillSessions",
            CommandName::ScanHostFolder => "ScanHostFolder",
            CommandName::WatchFilesystem => "WatchFilesystem",
            CommandName::DumpSessionLayout => "DumpSessionLayout",
            CommandName::CloseSelf => "CloseSelf",
            CommandName::NewTabsWithLayoutInfo => "NewTabsWithLayoutInfo",
            CommandName::Reconfigure => "Reconfigure",
            CommandName::HidePaneWithId => "HidePaneWithId",
            CommandName::ShowPaneWithId => "ShowPaneWithId",
            CommandName::OpenCommandPaneBackground => "OpenCommandPaneBackground",
            CommandName::RerunCommandPane => "RerunCommandPane",
            CommandName::ResizePaneIdWithDirection => "ResizePaneIdWithDirection",
            CommandName::EditScrollbackForPaneWithId => "EditScrollbackForPaneWithId",
            CommandName::WriteToPaneId => "WriteToPaneId",
            CommandName::WriteCharsToPaneId => "WriteCharsToPaneId",
            CommandName::MovePaneWithPaneId => "MovePaneWithPaneId",
            CommandName::MovePaneWithPaneIdInDirection => "MovePaneWithPaneIdInDirection",
            CommandName::ClearScreenForPaneId => "ClearScreenForPaneId",
            CommandName::ScrollUpInPaneId => "ScrollUpInPaneId",
            CommandName::ScrollDownInPaneId => "ScrollDownInPaneId",
            CommandName::ScrollToTopInPaneId => "ScrollToTopInPaneId",
            CommandName::ScrollToBottomInPaneId => "ScrollToBottomInPaneId",
            CommandName::PageScrollUpInPaneId => "PageScrollUpInPaneId",
            CommandName::PageScrollDownInPaneId => "PageScrollDownInPaneId",
            CommandName::TogglePaneIdFullscreen => "TogglePaneIdFullscreen",
            CommandName::TogglePaneEmbedOrEjectForPaneId => "TogglePaneEmbedOrEjectForPaneId",
            CommandName::CloseTabWithIndex => "CloseTabWithIndex",
            CommandName::BreakPanesToNewTab => "BreakPanesToNewTab",
            CommandName::BreakPanesToTabWithIndex => "BreakPanesToTabWithIndex",
            CommandName::ReloadPlugin => "ReloadPlugin",
            CommandName::LoadNewPlugin => "LoadNewPlugin",
            CommandName::RebindKeys => "RebindKeys",
            CommandName::ListClients => "ListClients",
            CommandName::ChangeHostFolder => "ChangeHostFolder",
            CommandName::SetFloatingPanePinned => "SetFloatingPanePinned",
            CommandName::StackPanes => "StackPanes",
            CommandName::ChangeFloatingPanesCoordinates => "ChangeFloatingPanesCoordinates",
            CommandName::OpenCommandPaneNearPlugin => "OpenCommandPaneNearPlugin",
            CommandName::OpenTerminalNearPlugin => "OpenTerminalNearPlugin",
            CommandName::OpenTerminalFloatingNearPlugin => "OpenTerminalFloatingNearPlugin",
            CommandName::OpenTerminalInPlaceOfPlugin => "OpenTerminalInPlaceOfPlugin",
            CommandName::OpenCommandPaneFloatingNearPlugin => "OpenCommandPaneFloatingNearPlugin",
            CommandName::OpenCommandPaneInPlaceOfPlugin => "OpenCommandPaneInPlaceOfPlugin",
            CommandName::OpenFileNearPlugin => "OpenFileNearPlugin",
            CommandName::OpenFileFloatingNearPlugin => "OpenFileFloatingNearPlugin",
            CommandName::OpenFileInPlaceOfPlugin => "OpenFileInPlaceOfPlugin",
            CommandName::StartWebServer => "StartWebServer",
            CommandName::GroupAndUngroupPanes => "GroupAndUngroupPanes",
            CommandName::HighlightAndUnhighlightPanes => "HighlightAndUnhighlightPanes",
            CommandName::CloseMultiplePanes => "CloseMultiplePanes",
            CommandName::FloatMultiplePanes => "FloatMultiplePanes",
            CommandName::EmbedMultiplePanes => "EmbedMultiplePanes",
            CommandName::ShareCurrentSession => "ShareCurrentSession",
            CommandName::StopSharingCurrentSession => "StopSharingCurrentSession",
            CommandName::StopWebServer => "StopWebServer",
            CommandName::QueryWebServerStatus => "QueryWebServerStatus",
            CommandName::SetSelfMouseSelectionSupport => "SetSelfMouseSelectionSupport",
            CommandName::GenerateWebLoginToken => "GenerateWebLoginToken",
            CommandName::RevokeWebLoginToken => "RevokeWebLoginToken",
            CommandName::ListWebLoginTokens => "ListWebLoginTokens",
            CommandName::RevokeAllWebLoginTokens => "RevokeAllWebLoginTokens",
            CommandName::RenameWebLoginToken => "RenameWebLoginToken",
            CommandName::InterceptKeyPresses => "InterceptKeyPresses",
            CommandName::ClearKeyPressesIntercepts => "ClearKeyPressesIntercepts",
            CommandName::ReplacePaneWithExistingPane => "ReplacePaneWithExistingPane",
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
            "OpenTerminalInPlace" => Some(Self::OpenTerminalInPlace),
            "OpenCommandInPlace" => Some(Self::OpenCommandInPlace),
            "OpenFileInPlace" => Some(Self::OpenFileInPlace),
            "RunCommand" => Some(Self::RunCommand),
            "WebRequest" => Some(Self::WebRequest),
            "DeleteDeadSession" => Some(Self::DeleteDeadSession),
            "DeleteAllDeadSessions" => Some(Self::DeleteAllDeadSessions),
            "RenameSession" => Some(Self::RenameSession),
            "UnblockCliPipeInput" => Some(Self::UnblockCliPipeInput),
            "BlockCliPipeInput" => Some(Self::BlockCliPipeInput),
            "CliPipeOutput" => Some(Self::CliPipeOutput),
            "MessageToPlugin" => Some(Self::MessageToPlugin),
            "DisconnectOtherClients" => Some(Self::DisconnectOtherClients),
            "KillSessions" => Some(Self::KillSessions),
            "ScanHostFolder" => Some(Self::ScanHostFolder),
            "WatchFilesystem" => Some(Self::WatchFilesystem),
            "DumpSessionLayout" => Some(Self::DumpSessionLayout),
            "CloseSelf" => Some(Self::CloseSelf),
            "NewTabsWithLayoutInfo" => Some(Self::NewTabsWithLayoutInfo),
            "Reconfigure" => Some(Self::Reconfigure),
            "HidePaneWithId" => Some(Self::HidePaneWithId),
            "ShowPaneWithId" => Some(Self::ShowPaneWithId),
            "OpenCommandPaneBackground" => Some(Self::OpenCommandPaneBackground),
            "RerunCommandPane" => Some(Self::RerunCommandPane),
            "ResizePaneIdWithDirection" => Some(Self::ResizePaneIdWithDirection),
            "EditScrollbackForPaneWithId" => Some(Self::EditScrollbackForPaneWithId),
            "WriteToPaneId" => Some(Self::WriteToPaneId),
            "WriteCharsToPaneId" => Some(Self::WriteCharsToPaneId),
            "MovePaneWithPaneId" => Some(Self::MovePaneWithPaneId),
            "MovePaneWithPaneIdInDirection" => Some(Self::MovePaneWithPaneIdInDirection),
            "ClearScreenForPaneId" => Some(Self::ClearScreenForPaneId),
            "ScrollUpInPaneId" => Some(Self::ScrollUpInPaneId),
            "ScrollDownInPaneId" => Some(Self::ScrollDownInPaneId),
            "ScrollToTopInPaneId" => Some(Self::ScrollToTopInPaneId),
            "ScrollToBottomInPaneId" => Some(Self::ScrollToBottomInPaneId),
            "PageScrollUpInPaneId" => Some(Self::PageScrollUpInPaneId),
            "PageScrollDownInPaneId" => Some(Self::PageScrollDownInPaneId),
            "TogglePaneIdFullscreen" => Some(Self::TogglePaneIdFullscreen),
            "TogglePaneEmbedOrEjectForPaneId" => Some(Self::TogglePaneEmbedOrEjectForPaneId),
            "CloseTabWithIndex" => Some(Self::CloseTabWithIndex),
            "BreakPanesToNewTab" => Some(Self::BreakPanesToNewTab),
            "BreakPanesToTabWithIndex" => Some(Self::BreakPanesToTabWithIndex),
            "ReloadPlugin" => Some(Self::ReloadPlugin),
            "LoadNewPlugin" => Some(Self::LoadNewPlugin),
            "RebindKeys" => Some(Self::RebindKeys),
            "ListClients" => Some(Self::ListClients),
            "ChangeHostFolder" => Some(Self::ChangeHostFolder),
            "SetFloatingPanePinned" => Some(Self::SetFloatingPanePinned),
            "StackPanes" => Some(Self::StackPanes),
            "ChangeFloatingPanesCoordinates" => Some(Self::ChangeFloatingPanesCoordinates),
            "OpenCommandPaneNearPlugin" => Some(Self::OpenCommandPaneNearPlugin),
            "OpenTerminalNearPlugin" => Some(Self::OpenTerminalNearPlugin),
            "OpenTerminalFloatingNearPlugin" => Some(Self::OpenTerminalFloatingNearPlugin),
            "OpenTerminalInPlaceOfPlugin" => Some(Self::OpenTerminalInPlaceOfPlugin),
            "OpenCommandPaneFloatingNearPlugin" => Some(Self::OpenCommandPaneFloatingNearPlugin),
            "OpenCommandPaneInPlaceOfPlugin" => Some(Self::OpenCommandPaneInPlaceOfPlugin),
            "OpenFileNearPlugin" => Some(Self::OpenFileNearPlugin),
            "OpenFileFloatingNearPlugin" => Some(Self::OpenFileFloatingNearPlugin),
            "OpenFileInPlaceOfPlugin" => Some(Self::OpenFileInPlaceOfPlugin),
            "StartWebServer" => Some(Self::StartWebServer),
            "GroupAndUngroupPanes" => Some(Self::GroupAndUngroupPanes),
            "HighlightAndUnhighlightPanes" => Some(Self::HighlightAndUnhighlightPanes),
            "CloseMultiplePanes" => Some(Self::CloseMultiplePanes),
            "FloatMultiplePanes" => Some(Self::FloatMultiplePanes),
            "EmbedMultiplePanes" => Some(Self::EmbedMultiplePanes),
            "ShareCurrentSession" => Some(Self::ShareCurrentSession),
            "StopSharingCurrentSession" => Some(Self::StopSharingCurrentSession),
            "StopWebServer" => Some(Self::StopWebServer),
            "QueryWebServerStatus" => Some(Self::QueryWebServerStatus),
            "SetSelfMouseSelectionSupport" => Some(Self::SetSelfMouseSelectionSupport),
            "GenerateWebLoginToken" => Some(Self::GenerateWebLoginToken),
            "RevokeWebLoginToken" => Some(Self::RevokeWebLoginToken),
            "ListWebLoginTokens" => Some(Self::ListWebLoginTokens),
            "RevokeAllWebLoginTokens" => Some(Self::RevokeAllWebLoginTokens),
            "RenameWebLoginToken" => Some(Self::RenameWebLoginToken),
            "InterceptKeyPresses" => Some(Self::InterceptKeyPresses),
            "ClearKeyPressesIntercepts" => Some(Self::ClearKeyPressesIntercepts),
            "ReplacePaneWithExistingPane" => Some(Self::ReplacePaneWithExistingPane),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum PaneType {
    Terminal = 0,
    Plugin = 1,
}
impl PaneType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            PaneType::Terminal => "Terminal",
            PaneType::Plugin => "Plugin",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Terminal" => Some(Self::Terminal),
            "Plugin" => Some(Self::Plugin),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum HttpVerb {
    Get = 0,
    Post = 1,
    Put = 2,
    Delete = 3,
}
impl HttpVerb {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            HttpVerb::Get => "Get",
            HttpVerb::Post => "Post",
            HttpVerb::Put => "Put",
            HttpVerb::Delete => "Delete",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Get" => Some(Self::Get),
            "Post" => Some(Self::Post),
            "Put" => Some(Self::Put),
            "Delete" => Some(Self::Delete),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum FixedOrPercent {
    Fixed = 0,
    Percent = 1,
}
impl FixedOrPercent {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            FixedOrPercent::Fixed => "Fixed",
            FixedOrPercent::Percent => "Percent",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "Fixed" => Some(Self::Fixed),
            "Percent" => Some(Self::Percent),
            _ => None,
        }
    }
}
