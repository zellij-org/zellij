#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Size {
    #[prost(uint32, tag="1")]
    pub cols: u32,
    #[prost(uint32, tag="2")]
    pub rows: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PixelDimensions {
    #[prost(message, optional, tag="1")]
    pub text_area_size: ::core::option::Option<SizeInPixels>,
    #[prost(message, optional, tag="2")]
    pub character_cell_size: ::core::option::Option<SizeInPixels>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SizeInPixels {
    #[prost(uint32, tag="1")]
    pub width: u32,
    #[prost(uint32, tag="2")]
    pub height: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneReference {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(bool, tag="2")]
    pub is_plugin: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ColorRegister {
    #[prost(uint32, tag="1")]
    pub index: u32,
    #[prost(string, tag="2")]
    pub color: ::prost::alloc::string::String,
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
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CharKey {
    #[prost(string, tag="1")]
    pub character: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FunctionKey {
    #[prost(uint32, tag="1")]
    pub function_number: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Style {
    #[prost(message, optional, tag="1")]
    pub colors: ::core::option::Option<Styling>,
    #[prost(bool, tag="2")]
    pub rounded_corners: bool,
    #[prost(bool, tag="3")]
    pub hide_session_name: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Styling {
    #[prost(message, optional, tag="1")]
    pub base: ::core::option::Option<PaletteColor>,
    #[prost(message, optional, tag="2")]
    pub background: ::core::option::Option<PaletteColor>,
    #[prost(message, optional, tag="3")]
    pub emphasis_0: ::core::option::Option<PaletteColor>,
    #[prost(message, optional, tag="4")]
    pub emphasis_1: ::core::option::Option<PaletteColor>,
    #[prost(message, optional, tag="5")]
    pub emphasis_2: ::core::option::Option<PaletteColor>,
    #[prost(message, optional, tag="6")]
    pub emphasis_3: ::core::option::Option<PaletteColor>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaletteColor {
    #[prost(oneof="palette_color::ColorType", tags="1, 2, 3")]
    pub color_type: ::core::option::Option<palette_color::ColorType>,
}
/// Nested message and enum types in `PaletteColor`.
pub mod palette_color {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum ColorType {
        #[prost(message, tag="1")]
        Rgb(super::RgbColor),
        #[prost(uint32, tag="2")]
        EightBit(u32),
        #[prost(enumeration="super::AnsiCode", tag="3")]
        Ansi(i32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RgbColor {
    #[prost(uint32, tag="1")]
    pub r: u32,
    #[prost(uint32, tag="2")]
    pub g: u32,
    #[prost(uint32, tag="3")]
    pub b: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Action {
    #[prost(oneof="action::ActionType", tags="1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94")]
    pub action_type: ::core::option::Option<action::ActionType>,
}
/// Nested message and enum types in `Action`.
pub mod action {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum ActionType {
        #[prost(message, tag="1")]
        Quit(super::QuitAction),
        #[prost(message, tag="2")]
        Write(super::WriteAction),
        #[prost(message, tag="3")]
        WriteChars(super::WriteCharsAction),
        #[prost(message, tag="4")]
        SwitchToMode(super::SwitchToModeAction),
        #[prost(message, tag="5")]
        SwitchModeForAllClients(super::SwitchModeForAllClientsAction),
        #[prost(message, tag="6")]
        Resize(super::ResizeAction),
        #[prost(message, tag="7")]
        FocusNextPane(super::FocusNextPaneAction),
        #[prost(message, tag="8")]
        FocusPreviousPane(super::FocusPreviousPaneAction),
        #[prost(message, tag="9")]
        SwitchFocus(super::SwitchFocusAction),
        #[prost(message, tag="10")]
        MoveFocus(super::MoveFocusAction),
        #[prost(message, tag="11")]
        MoveFocusOrTab(super::MoveFocusOrTabAction),
        #[prost(message, tag="12")]
        MovePane(super::MovePaneAction),
        #[prost(message, tag="13")]
        MovePaneBackwards(super::MovePaneBackwardsAction),
        #[prost(message, tag="14")]
        ClearScreen(super::ClearScreenAction),
        #[prost(message, tag="15")]
        DumpScreen(super::DumpScreenAction),
        #[prost(message, tag="16")]
        DumpLayout(super::DumpLayoutAction),
        #[prost(message, tag="17")]
        EditScrollback(super::EditScrollbackAction),
        #[prost(message, tag="18")]
        ScrollUp(super::ScrollUpAction),
        #[prost(message, tag="19")]
        ScrollUpAt(super::ScrollUpAtAction),
        #[prost(message, tag="20")]
        ScrollDown(super::ScrollDownAction),
        #[prost(message, tag="21")]
        ScrollDownAt(super::ScrollDownAtAction),
        #[prost(message, tag="22")]
        ScrollToBottom(super::ScrollToBottomAction),
        #[prost(message, tag="23")]
        ScrollToTop(super::ScrollToTopAction),
        #[prost(message, tag="24")]
        PageScrollUp(super::PageScrollUpAction),
        #[prost(message, tag="25")]
        PageScrollDown(super::PageScrollDownAction),
        #[prost(message, tag="26")]
        HalfPageScrollUp(super::HalfPageScrollUpAction),
        #[prost(message, tag="27")]
        HalfPageScrollDown(super::HalfPageScrollDownAction),
        #[prost(message, tag="28")]
        ToggleFocusFullscreen(super::ToggleFocusFullscreenAction),
        #[prost(message, tag="29")]
        TogglePaneFrames(super::TogglePaneFramesAction),
        #[prost(message, tag="30")]
        ToggleActiveSyncTab(super::ToggleActiveSyncTabAction),
        #[prost(message, tag="31")]
        NewPane(super::NewPaneAction),
        #[prost(message, tag="32")]
        EditFile(super::EditFileAction),
        #[prost(message, tag="33")]
        NewFloatingPane(super::NewFloatingPaneAction),
        #[prost(message, tag="34")]
        NewTiledPane(super::NewTiledPaneAction),
        #[prost(message, tag="35")]
        NewInPlacePane(super::NewInPlacePaneAction),
        #[prost(message, tag="36")]
        NewStackedPane(super::NewStackedPaneAction),
        #[prost(message, tag="37")]
        TogglePaneEmbedOrFloating(super::TogglePaneEmbedOrFloatingAction),
        #[prost(message, tag="38")]
        ToggleFloatingPanes(super::ToggleFloatingPanesAction),
        #[prost(message, tag="39")]
        CloseFocus(super::CloseFocusAction),
        #[prost(message, tag="40")]
        PaneNameInput(super::PaneNameInputAction),
        #[prost(message, tag="41")]
        UndoRenamePane(super::UndoRenamePaneAction),
        #[prost(message, tag="42")]
        NewTab(super::NewTabAction),
        #[prost(message, tag="43")]
        NoOp(super::NoOpAction),
        #[prost(message, tag="44")]
        GoToNextTab(super::GoToNextTabAction),
        #[prost(message, tag="45")]
        GoToPreviousTab(super::GoToPreviousTabAction),
        #[prost(message, tag="46")]
        CloseTab(super::CloseTabAction),
        #[prost(message, tag="47")]
        GoToTab(super::GoToTabAction),
        #[prost(message, tag="48")]
        GoToTabName(super::GoToTabNameAction),
        #[prost(message, tag="49")]
        ToggleTab(super::ToggleTabAction),
        #[prost(message, tag="50")]
        TabNameInput(super::TabNameInputAction),
        #[prost(message, tag="51")]
        UndoRenameTab(super::UndoRenameTabAction),
        #[prost(message, tag="52")]
        MoveTab(super::MoveTabAction),
        #[prost(message, tag="53")]
        Run(super::RunAction),
        #[prost(message, tag="54")]
        Detach(super::DetachAction),
        #[prost(message, tag="55")]
        LaunchOrFocusPlugin(super::LaunchOrFocusPluginAction),
        #[prost(message, tag="56")]
        LaunchPlugin(super::LaunchPluginAction),
        #[prost(message, tag="57")]
        MouseEvent(super::MouseEventAction),
        #[prost(message, tag="58")]
        Copy(super::CopyAction),
        #[prost(message, tag="59")]
        Confirm(super::ConfirmAction),
        #[prost(message, tag="60")]
        Deny(super::DenyAction),
        #[prost(message, tag="61")]
        SkipConfirm(::prost::alloc::boxed::Box<super::SkipConfirmAction>),
        #[prost(message, tag="62")]
        SearchInput(super::SearchInputAction),
        #[prost(message, tag="63")]
        Search(super::SearchAction),
        #[prost(message, tag="64")]
        SearchToggleOption(super::SearchToggleOptionAction),
        #[prost(message, tag="65")]
        ToggleMouseMode(super::ToggleMouseModeAction),
        #[prost(message, tag="66")]
        PreviousSwapLayout(super::PreviousSwapLayoutAction),
        #[prost(message, tag="67")]
        NextSwapLayout(super::NextSwapLayoutAction),
        #[prost(message, tag="68")]
        QueryTabNames(super::QueryTabNamesAction),
        #[prost(message, tag="69")]
        NewTiledPluginPane(super::NewTiledPluginPaneAction),
        #[prost(message, tag="70")]
        NewFloatingPluginPane(super::NewFloatingPluginPaneAction),
        #[prost(message, tag="71")]
        NewInPlacePluginPane(super::NewInPlacePluginPaneAction),
        #[prost(message, tag="72")]
        StartOrReloadPlugin(super::StartOrReloadPluginAction),
        #[prost(message, tag="73")]
        CloseTerminalPane(super::CloseTerminalPaneAction),
        #[prost(message, tag="74")]
        ClosePluginPane(super::ClosePluginPaneAction),
        #[prost(message, tag="75")]
        FocusTerminalPaneWithId(super::FocusTerminalPaneWithIdAction),
        #[prost(message, tag="76")]
        FocusPluginPaneWithId(super::FocusPluginPaneWithIdAction),
        #[prost(message, tag="77")]
        RenameTerminalPane(super::RenameTerminalPaneAction),
        #[prost(message, tag="78")]
        RenamePluginPane(super::RenamePluginPaneAction),
        #[prost(message, tag="79")]
        RenameTab(super::RenameTabAction),
        #[prost(message, tag="80")]
        BreakPane(super::BreakPaneAction),
        #[prost(message, tag="81")]
        BreakPaneRight(super::BreakPaneRightAction),
        #[prost(message, tag="82")]
        BreakPaneLeft(super::BreakPaneLeftAction),
        #[prost(message, tag="83")]
        RenameSession(super::RenameSessionAction),
        #[prost(message, tag="84")]
        CliPipe(super::CliPipeAction),
        #[prost(message, tag="85")]
        KeybindPipe(super::KeybindPipeAction),
        #[prost(message, tag="86")]
        ListClients(super::ListClientsAction),
        #[prost(message, tag="87")]
        TogglePanePinned(super::TogglePanePinnedAction),
        #[prost(message, tag="88")]
        StackPanes(super::StackPanesAction),
        #[prost(message, tag="89")]
        ChangeFloatingPaneCoordinates(super::ChangeFloatingPaneCoordinatesAction),
        #[prost(message, tag="90")]
        TogglePaneInGroup(super::TogglePaneInGroupAction),
        #[prost(message, tag="91")]
        ToggleGroupMarking(super::ToggleGroupMarkingAction),
        #[prost(message, tag="92")]
        SwitchSession(super::SwitchSessionAction),
        #[prost(message, tag="93")]
        NewBlockingPane(super::NewBlockingPaneAction),
        #[prost(message, tag="94")]
        OverrideLayout(super::OverrideLayoutAction),
    }
}
// Action message definitions (all 92 variants)

/// Simple action types (no data)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QuitAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusNextPaneAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusPreviousPaneAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchFocusAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePaneBackwardsAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClearScreenAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DumpLayoutAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EditScrollbackAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollUpAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollDownAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollToBottomAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollToTopAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PageScrollUpAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PageScrollDownAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HalfPageScrollUpAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct HalfPageScrollDownAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleFocusFullscreenAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePaneFramesAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleActiveSyncTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePaneEmbedOrFloatingAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleFloatingPanesAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CloseFocusAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UndoRenamePaneAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NoOpAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GoToNextTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GoToPreviousTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CloseTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UndoRenameTabAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DetachAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CopyAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConfirmAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DenyAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleMouseModeAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PreviousSwapLayoutAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NextSwapLayoutAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OverrideLayoutAction {
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
    pub retain_existing_terminal_panes: bool,
    #[prost(bool, tag="7")]
    pub retain_existing_plugin_panes: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryTabNamesAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BreakPaneAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BreakPaneRightAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BreakPaneLeftAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ListClientsAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePanePinnedAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TogglePaneInGroupAction {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ToggleGroupMarkingAction {
}
/// Complex action types (with data)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteAction {
    #[prost(message, optional, tag="1")]
    pub key_with_modifier: ::core::option::Option<KeyWithModifier>,
    #[prost(uint32, repeated, tag="2")]
    pub bytes: ::prost::alloc::vec::Vec<u32>,
    #[prost(bool, tag="3")]
    pub is_kitty_keyboard_protocol: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WriteCharsAction {
    #[prost(string, tag="1")]
    pub chars: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchToModeAction {
    #[prost(enumeration="InputMode", tag="1")]
    pub input_mode: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchModeForAllClientsAction {
    #[prost(enumeration="InputMode", tag="1")]
    pub input_mode: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ResizeAction {
    #[prost(enumeration="ResizeType", tag="1")]
    pub resize: i32,
    #[prost(enumeration="Direction", optional, tag="2")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveFocusAction {
    #[prost(enumeration="Direction", tag="1")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveFocusOrTabAction {
    #[prost(enumeration="Direction", tag="1")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MovePaneAction {
    #[prost(enumeration="Direction", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DumpScreenAction {
    #[prost(string, tag="1")]
    pub file_path: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub include_scrollback: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollUpAtAction {
    #[prost(message, optional, tag="1")]
    pub position: ::core::option::Option<Position>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ScrollDownAtAction {
    #[prost(message, optional, tag="1")]
    pub position: ::core::option::Option<Position>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPaneAction {
    #[prost(enumeration="Direction", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub start_suppressed: bool,
    #[prost(bool, tag="4")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EditFileAction {
    #[prost(message, optional, tag="1")]
    pub payload: ::core::option::Option<OpenFilePayload>,
    #[prost(enumeration="Direction", optional, tag="2")]
    pub direction: ::core::option::Option<i32>,
    #[prost(bool, tag="3")]
    pub floating: bool,
    #[prost(bool, tag="4")]
    pub in_place: bool,
    #[prost(bool, tag="5")]
    pub start_suppressed: bool,
    #[prost(message, optional, tag="6")]
    pub coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(bool, tag="7")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewFloatingPaneAction {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag="3")]
    pub coordinates: ::core::option::Option<FloatingPaneCoordinates>,
    #[prost(bool, tag="7")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTiledPaneAction {
    #[prost(enumeration="Direction", optional, tag="1")]
    pub direction: ::core::option::Option<i32>,
    #[prost(message, optional, tag="2")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(string, optional, tag="3")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="7")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewInPlacePaneAction {
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
pub struct NewStackedPaneAction {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewBlockingPaneAction {
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
pub struct PaneNameInputAction {
    #[prost(uint32, repeated, tag="1")]
    pub input: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTabAction {
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
pub struct GoToTabAction {
    #[prost(uint32, tag="1")]
    pub index: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct GoToTabNameAction {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(bool, tag="2")]
    pub create: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TabNameInputAction {
    #[prost(uint32, repeated, tag="1")]
    pub input: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MoveTabAction {
    #[prost(enumeration="Direction", tag="1")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RunAction {
    #[prost(message, optional, tag="1")]
    pub command: ::core::option::Option<RunCommandAction>,
    #[prost(bool, tag="2")]
    pub near_current_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LaunchOrFocusPluginAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
    #[prost(bool, tag="2")]
    pub should_float: bool,
    #[prost(bool, tag="3")]
    pub move_to_focused_tab: bool,
    #[prost(bool, tag="4")]
    pub should_open_in_place: bool,
    #[prost(bool, tag="5")]
    pub skip_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LaunchPluginAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
    #[prost(bool, tag="2")]
    pub should_float: bool,
    #[prost(bool, tag="3")]
    pub should_open_in_place: bool,
    #[prost(bool, tag="4")]
    pub skip_cache: bool,
    #[prost(string, optional, tag="5")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MouseEventAction {
    #[prost(message, optional, tag="1")]
    pub event: ::core::option::Option<MouseEvent>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SkipConfirmAction {
    #[prost(message, optional, boxed, tag="1")]
    pub action: ::core::option::Option<::prost::alloc::boxed::Box<Action>>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SearchInputAction {
    #[prost(uint32, repeated, tag="1")]
    pub input: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SearchAction {
    #[prost(enumeration="SearchDirection", tag="1")]
    pub direction: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SearchToggleOptionAction {
    #[prost(enumeration="SearchOption", tag="1")]
    pub option: i32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewTiledPluginPaneAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub skip_cache: bool,
    #[prost(string, optional, tag="4")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewFloatingPluginPaneAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub skip_cache: bool,
    #[prost(string, optional, tag="4")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(message, optional, tag="5")]
    pub coordinates: ::core::option::Option<FloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewInPlacePluginPaneAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
    #[prost(string, optional, tag="2")]
    pub pane_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub skip_cache: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StartOrReloadPluginAction {
    #[prost(message, optional, tag="1")]
    pub plugin: ::core::option::Option<RunPluginOrAlias>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CloseTerminalPaneAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClosePluginPaneAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusTerminalPaneWithIdAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(bool, tag="2")]
    pub should_float_if_hidden: bool,
    #[prost(bool, tag="3")]
    pub should_be_in_place_if_hidden: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FocusPluginPaneWithIdAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(bool, tag="2")]
    pub should_float_if_hidden: bool,
    #[prost(bool, tag="3")]
    pub should_be_in_place_if_hidden: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenameTerminalPaneAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(uint32, repeated, tag="2")]
    pub name: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenamePluginPaneAction {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(uint32, repeated, tag="2")]
    pub name: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenameTabAction {
    #[prost(uint32, tag="1")]
    pub tab_index: u32,
    #[prost(uint32, repeated, tag="2")]
    pub name: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenameSessionAction {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliPipeAction {
    #[prost(string, tag="1")]
    pub pipe_id: ::prost::alloc::string::String,
    #[prost(string, optional, tag="2")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="3")]
    pub payload: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(map="string, string", tag="4")]
    pub args: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    #[prost(string, optional, tag="5")]
    pub plugin: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(map="string, string", tag="6")]
    pub configuration: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    #[prost(bool, tag="7")]
    pub launch_new: bool,
    #[prost(bool, tag="8")]
    pub skip_cache: bool,
    #[prost(bool, optional, tag="9")]
    pub floating: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="10")]
    pub in_place: ::core::option::Option<bool>,
    #[prost(string, optional, tag="11")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="12")]
    pub pane_title: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeybindPipeAction {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="2")]
    pub payload: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(map="string, string", tag="3")]
    pub args: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    #[prost(string, optional, tag="4")]
    pub plugin: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="5")]
    pub plugin_id: ::core::option::Option<u32>,
    #[prost(map="string, string", tag="6")]
    pub configuration: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
    #[prost(bool, tag="7")]
    pub launch_new: bool,
    #[prost(bool, tag="8")]
    pub skip_cache: bool,
    #[prost(bool, optional, tag="9")]
    pub floating: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="10")]
    pub in_place: ::core::option::Option<bool>,
    #[prost(string, optional, tag="11")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="12")]
    pub pane_title: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StackPanesAction {
    #[prost(message, repeated, tag="1")]
    pub pane_ids: ::prost::alloc::vec::Vec<PaneId>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChangeFloatingPaneCoordinatesAction {
    #[prost(message, optional, tag="1")]
    pub pane_id: ::core::option::Option<PaneId>,
    #[prost(message, optional, tag="2")]
    pub coordinates: ::core::option::Option<FloatingPaneCoordinates>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Position {
    /// Changed from uint32 to int32 to support negative line numbers
    #[prost(int32, tag="1")]
    pub line: i32,
    /// Changed from uint32 to uint64 to support large column numbers
    #[prost(uint64, tag="2")]
    pub column: u64,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliAssets {
    #[prost(string, optional, tag="1")]
    pub config_file_path: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="2")]
    pub config_dir: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="3")]
    pub should_ignore_config: bool,
    #[prost(message, optional, tag="4")]
    pub configuration_options: ::core::option::Option<Options>,
    #[prost(message, optional, tag="5")]
    pub layout: ::core::option::Option<LayoutInfo>,
    #[prost(message, optional, tag="6")]
    pub terminal_window_size: ::core::option::Option<Size>,
    #[prost(string, optional, tag="7")]
    pub data_dir: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, tag="8")]
    pub is_debug: bool,
    #[prost(uint32, optional, tag="9")]
    pub max_panes: ::core::option::Option<u32>,
    #[prost(bool, tag="10")]
    pub force_run_layout_commands: bool,
    #[prost(string, optional, tag="11")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutInfo {
    #[prost(oneof="layout_info::LayoutType", tags="1, 2, 3, 4")]
    pub layout_type: ::core::option::Option<layout_info::LayoutType>,
}
/// Nested message and enum types in `LayoutInfo`.
pub mod layout_info {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum LayoutType {
        #[prost(string, tag="1")]
        FilePath(::prost::alloc::string::String),
        #[prost(string, tag="2")]
        BuiltinName(::prost::alloc::string::String),
        #[prost(string, tag="3")]
        Url(::prost::alloc::string::String),
        #[prost(string, tag="4")]
        Stringified(::prost::alloc::string::String),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectToSession {
    #[prost(string, optional, tag="1")]
    pub name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="2")]
    pub tab_position: ::core::option::Option<u32>,
    #[prost(message, optional, tag="3")]
    pub pane_id: ::core::option::Option<PaneIdWithPlugin>,
    #[prost(message, optional, tag="4")]
    pub layout: ::core::option::Option<LayoutInfo>,
    #[prost(string, optional, tag="5")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchSessionAction {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag="2")]
    pub tab_position: ::core::option::Option<u32>,
    #[prost(message, optional, tag="3")]
    pub pane_id: ::core::option::Option<PaneIdWithPlugin>,
    #[prost(message, optional, tag="4")]
    pub layout: ::core::option::Option<LayoutInfo>,
    #[prost(string, optional, tag="5")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPanePlacement {
    #[prost(oneof="new_pane_placement::PlacementType", tags="1, 2, 3, 4, 5")]
    pub placement_type: ::core::option::Option<new_pane_placement::PlacementType>,
}
/// Nested message and enum types in `NewPanePlacement`.
pub mod new_pane_placement {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PlacementType {
        #[prost(bool, tag="1")]
        NoPreference(bool),
        #[prost(enumeration="super::Direction", tag="2")]
        Tiled(i32),
        #[prost(message, tag="3")]
        Floating(super::FloatingPaneCoordinates),
        #[prost(message, tag="4")]
        InPlace(super::NewPanePlacementInPlace),
        #[prost(message, tag="5")]
        Stacked(super::PaneId),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NewPanePlacementInPlace {
    #[prost(message, optional, tag="1")]
    pub pane_id_to_replace: ::core::option::Option<PaneId>,
    #[prost(bool, tag="2")]
    pub close_replaced_pane: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneIdWithPlugin {
    #[prost(uint32, tag="1")]
    pub pane_id: u32,
    #[prost(bool, tag="2")]
    pub is_plugin: bool,
}
// Additional supporting types for Action messages

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct OpenFilePayload {
    #[prost(string, tag="1")]
    pub file_to_open: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag="2")]
    pub line_number: ::core::option::Option<u32>,
    /// Renumbered after removing column_number
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
    /// Added missing originating_plugin field
    #[prost(message, optional, tag="4")]
    pub originating_plugin: ::core::option::Option<OriginatingPlugin>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingPaneCoordinates {
    #[prost(message, optional, tag="1")]
    pub x: ::core::option::Option<FloatingCoordinate>,
    #[prost(message, optional, tag="2")]
    pub y: ::core::option::Option<FloatingCoordinate>,
    #[prost(message, optional, tag="3")]
    pub width: ::core::option::Option<FloatingCoordinate>,
    #[prost(message, optional, tag="4")]
    pub height: ::core::option::Option<FloatingCoordinate>,
    /// Added missing pinned field
    #[prost(bool, optional, tag="5")]
    pub pinned: ::core::option::Option<bool>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FloatingCoordinate {
    #[prost(oneof="floating_coordinate::CoordinateType", tags="1, 2")]
    pub coordinate_type: ::core::option::Option<floating_coordinate::CoordinateType>,
}
/// Nested message and enum types in `FloatingCoordinate`.
pub mod floating_coordinate {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum CoordinateType {
        #[prost(uint32, tag="1")]
        Fixed(u32),
        #[prost(float, tag="2")]
        Percent(f32),
    }
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
    /// Added missing direction field
    #[prost(enumeration="Direction", optional, tag="4")]
    pub direction: ::core::option::Option<i32>,
    /// Renumbered
    #[prost(bool, tag="5")]
    pub hold_on_close: bool,
    /// Renumbered
    #[prost(bool, tag="6")]
    pub hold_on_start: bool,
    /// Added missing originating_plugin field
    #[prost(message, optional, tag="7")]
    pub originating_plugin: ::core::option::Option<OriginatingPlugin>,
    /// Added missing use_terminal_title field
    #[prost(bool, tag="8")]
    pub use_terminal_title: bool,
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
    pub run: ::core::option::Option<Run>,
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
    /// NOTE: run_instructions_to_ignore is not represented here because it's a field used only inside the server itself and not part of the server/client contract
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
    pub run: ::core::option::Option<Run>,
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
pub struct MouseEvent {
    #[prost(enumeration="MouseEventType", tag="1")]
    pub event_type: i32,
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
    #[prost(message, optional, tag="10")]
    pub position: ::core::option::Option<Position>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct PaneId {
    #[prost(oneof="pane_id::PaneType", tags="1, 2")]
    pub pane_type: ::core::option::Option<pane_id::PaneType>,
}
/// Nested message and enum types in `PaneId`.
pub mod pane_id {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum PaneType {
        #[prost(uint32, tag="1")]
        Terminal(u32),
        #[prost(uint32, tag="2")]
        Plugin(u32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SplitSize {
    #[prost(oneof="split_size::SizeType", tags="1, 2")]
    pub size_type: ::core::option::Option<split_size::SizeType>,
}
/// Nested message and enum types in `SplitSize`.
pub mod split_size {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum SizeType {
        /// 1 to 100
        #[prost(uint32, tag="1")]
        Percent(u32),
        /// absolute number
        #[prost(uint32, tag="2")]
        Fixed(u32),
    }
}
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
        /// 1 to 100
        #[prost(uint32, tag="1")]
        Percent(u32),
        /// absolute number
        #[prost(uint32, tag="2")]
        Fixed(u32),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LayoutConstraintWithValue {
    #[prost(enumeration="LayoutConstraint", tag="1")]
    pub constraint_type: i32,
    /// Only used for MAX_PANES, MIN_PANES, EXACT_PANES
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
        /// For FILE
        #[prost(string, tag="2")]
        FilePath(::prost::alloc::string::String),
        /// For ZELLIJ
        #[prost(message, tag="3")]
        ZellijTag(super::PluginTag),
        /// For REMOTE
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
pub struct OriginatingPlugin {
    #[prost(uint32, tag="1")]
    pub plugin_id: u32,
    /// ClientId is u16, but using uint32 for protobuf compatibility
    #[prost(uint32, tag="2")]
    pub client_id: u32,
    /// Context is BTreeMap<String, String>
    #[prost(map="string, string", tag="3")]
    pub context: ::std::collections::HashMap<::prost::alloc::string::String, ::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Run {
    #[prost(oneof="run::RunType", tags="1, 2, 3, 4")]
    pub run_type: ::core::option::Option<run::RunType>,
}
/// Nested message and enum types in `Run`.
pub mod run {
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
pub struct RunEditFileAction {
    #[prost(string, tag="1")]
    pub file_path: ::prost::alloc::string::String,
    #[prost(uint32, optional, tag="2")]
    pub line_number: ::core::option::Option<u32>,
    #[prost(string, optional, tag="3")]
    pub cwd: ::core::option::Option<::prost::alloc::string::String>,
}
// Config Options message - comprehensive configuration structure

#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Options {
    #[prost(bool, optional, tag="1")]
    pub simplified_ui: ::core::option::Option<bool>,
    #[prost(string, optional, tag="2")]
    pub theme: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration="InputMode", optional, tag="3")]
    pub default_mode: ::core::option::Option<i32>,
    #[prost(string, optional, tag="4")]
    pub default_shell: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="5")]
    pub default_cwd: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="6")]
    pub default_layout: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="7")]
    pub layout_dir: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="8")]
    pub theme_dir: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag="9")]
    pub mouse_mode: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="10")]
    pub pane_frames: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="11")]
    pub mirror_session: ::core::option::Option<bool>,
    #[prost(enumeration="OnForceClose", optional, tag="12")]
    pub on_force_close: ::core::option::Option<i32>,
    #[prost(uint32, optional, tag="13")]
    pub scroll_buffer_size: ::core::option::Option<u32>,
    #[prost(string, optional, tag="14")]
    pub copy_command: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(enumeration="Clipboard", optional, tag="15")]
    pub copy_clipboard: ::core::option::Option<i32>,
    #[prost(bool, optional, tag="16")]
    pub copy_on_select: ::core::option::Option<bool>,
    #[prost(string, optional, tag="17")]
    pub scrollback_editor: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="18")]
    pub session_name: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag="19")]
    pub attach_to_session: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="20")]
    pub auto_layout: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="21")]
    pub session_serialization: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="22")]
    pub serialize_pane_viewport: ::core::option::Option<bool>,
    #[prost(uint32, optional, tag="23")]
    pub scrollback_lines_to_serialize: ::core::option::Option<u32>,
    #[prost(bool, optional, tag="24")]
    pub styled_underlines: ::core::option::Option<bool>,
    #[prost(uint64, optional, tag="25")]
    pub serialization_interval: ::core::option::Option<u64>,
    #[prost(bool, optional, tag="26")]
    pub disable_session_metadata: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="27")]
    pub support_kitty_keyboard_protocol: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="28")]
    pub web_server: ::core::option::Option<bool>,
    #[prost(enumeration="WebSharing", optional, tag="29")]
    pub web_sharing: ::core::option::Option<i32>,
    #[prost(bool, optional, tag="30")]
    pub stacked_resize: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="31")]
    pub show_startup_tips: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="32")]
    pub show_release_notes: ::core::option::Option<bool>,
    #[prost(bool, optional, tag="33")]
    pub advanced_mouse_actions: ::core::option::Option<bool>,
    #[prost(string, optional, tag="34")]
    pub web_server_ip: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(uint32, optional, tag="35")]
    pub web_server_port: ::core::option::Option<u32>,
    #[prost(string, optional, tag="36")]
    pub web_server_cert: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(string, optional, tag="37")]
    pub web_server_key: ::core::option::Option<::prost::alloc::string::String>,
    #[prost(bool, optional, tag="38")]
    pub enforce_https_for_localhost: ::core::option::Option<bool>,
    #[prost(string, optional, tag="39")]
    pub post_command_discovery_hook: ::core::option::Option<::prost::alloc::string::String>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum AnsiCode {
    Unspecified = 0,
    Black = 1,
    Red = 2,
    Green = 3,
    Yellow = 4,
    Blue = 5,
    Magenta = 6,
    Cyan = 7,
    White = 8,
    BrightBlack = 9,
    BrightRed = 10,
    BrightGreen = 11,
    BrightYellow = 12,
    BrightBlue = 13,
    BrightMagenta = 14,
    BrightCyan = 15,
    BrightWhite = 16,
}
impl AnsiCode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            AnsiCode::Unspecified => "ANSI_CODE_UNSPECIFIED",
            AnsiCode::Black => "BLACK",
            AnsiCode::Red => "RED",
            AnsiCode::Green => "GREEN",
            AnsiCode::Yellow => "YELLOW",
            AnsiCode::Blue => "BLUE",
            AnsiCode::Magenta => "MAGENTA",
            AnsiCode::Cyan => "CYAN",
            AnsiCode::White => "WHITE",
            AnsiCode::BrightBlack => "BRIGHT_BLACK",
            AnsiCode::BrightRed => "BRIGHT_RED",
            AnsiCode::BrightGreen => "BRIGHT_GREEN",
            AnsiCode::BrightYellow => "BRIGHT_YELLOW",
            AnsiCode::BrightBlue => "BRIGHT_BLUE",
            AnsiCode::BrightMagenta => "BRIGHT_MAGENTA",
            AnsiCode::BrightCyan => "BRIGHT_CYAN",
            AnsiCode::BrightWhite => "BRIGHT_WHITE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ANSI_CODE_UNSPECIFIED" => Some(Self::Unspecified),
            "BLACK" => Some(Self::Black),
            "RED" => Some(Self::Red),
            "GREEN" => Some(Self::Green),
            "YELLOW" => Some(Self::Yellow),
            "BLUE" => Some(Self::Blue),
            "MAGENTA" => Some(Self::Magenta),
            "CYAN" => Some(Self::Cyan),
            "WHITE" => Some(Self::White),
            "BRIGHT_BLACK" => Some(Self::BrightBlack),
            "BRIGHT_RED" => Some(Self::BrightRed),
            "BRIGHT_GREEN" => Some(Self::BrightGreen),
            "BRIGHT_YELLOW" => Some(Self::BrightYellow),
            "BRIGHT_BLUE" => Some(Self::BrightBlue),
            "BRIGHT_MAGENTA" => Some(Self::BrightMagenta),
            "BRIGHT_CYAN" => Some(Self::BrightCyan),
            "BRIGHT_WHITE" => Some(Self::BrightWhite),
            _ => None,
        }
    }
}
/// Supporting enums and messages
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum InputMode {
    Unspecified = 0,
    Normal = 1,
    Locked = 2,
    Resize = 3,
    Pane = 4,
    Tab = 5,
    Scroll = 6,
    EnterSearch = 7,
    Search = 8,
    RenameTab = 9,
    RenamePane = 10,
    Session = 11,
    Move = 12,
    Prompt = 13,
    Tmux = 14,
}
impl InputMode {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            InputMode::Unspecified => "INPUT_MODE_UNSPECIFIED",
            InputMode::Normal => "INPUT_MODE_NORMAL",
            InputMode::Locked => "INPUT_MODE_LOCKED",
            InputMode::Resize => "INPUT_MODE_RESIZE",
            InputMode::Pane => "INPUT_MODE_PANE",
            InputMode::Tab => "INPUT_MODE_TAB",
            InputMode::Scroll => "INPUT_MODE_SCROLL",
            InputMode::EnterSearch => "INPUT_MODE_ENTER_SEARCH",
            InputMode::Search => "INPUT_MODE_SEARCH",
            InputMode::RenameTab => "INPUT_MODE_RENAME_TAB",
            InputMode::RenamePane => "INPUT_MODE_RENAME_PANE",
            InputMode::Session => "INPUT_MODE_SESSION",
            InputMode::Move => "INPUT_MODE_MOVE",
            InputMode::Prompt => "INPUT_MODE_PROMPT",
            InputMode::Tmux => "INPUT_MODE_TMUX",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "INPUT_MODE_UNSPECIFIED" => Some(Self::Unspecified),
            "INPUT_MODE_NORMAL" => Some(Self::Normal),
            "INPUT_MODE_LOCKED" => Some(Self::Locked),
            "INPUT_MODE_RESIZE" => Some(Self::Resize),
            "INPUT_MODE_PANE" => Some(Self::Pane),
            "INPUT_MODE_TAB" => Some(Self::Tab),
            "INPUT_MODE_SCROLL" => Some(Self::Scroll),
            "INPUT_MODE_ENTER_SEARCH" => Some(Self::EnterSearch),
            "INPUT_MODE_SEARCH" => Some(Self::Search),
            "INPUT_MODE_RENAME_TAB" => Some(Self::RenameTab),
            "INPUT_MODE_RENAME_PANE" => Some(Self::RenamePane),
            "INPUT_MODE_SESSION" => Some(Self::Session),
            "INPUT_MODE_MOVE" => Some(Self::Move),
            "INPUT_MODE_PROMPT" => Some(Self::Prompt),
            "INPUT_MODE_TMUX" => Some(Self::Tmux),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Direction {
    Unspecified = 0,
    Left = 1,
    Right = 2,
    Up = 3,
    Down = 4,
}
impl Direction {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Direction::Unspecified => "DIRECTION_UNSPECIFIED",
            Direction::Left => "DIRECTION_LEFT",
            Direction::Right => "DIRECTION_RIGHT",
            Direction::Up => "DIRECTION_UP",
            Direction::Down => "DIRECTION_DOWN",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "DIRECTION_LEFT" => Some(Self::Left),
            "DIRECTION_RIGHT" => Some(Self::Right),
            "DIRECTION_UP" => Some(Self::Up),
            "DIRECTION_DOWN" => Some(Self::Down),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum UnblockCondition {
    Unspecified = 0,
    OnExitSuccess = 1,
    OnExitFailure = 2,
    OnAnyExit = 3,
}
impl UnblockCondition {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            UnblockCondition::Unspecified => "UNBLOCK_CONDITION_UNSPECIFIED",
            UnblockCondition::OnExitSuccess => "UNBLOCK_CONDITION_ON_EXIT_SUCCESS",
            UnblockCondition::OnExitFailure => "UNBLOCK_CONDITION_ON_EXIT_FAILURE",
            UnblockCondition::OnAnyExit => "UNBLOCK_CONDITION_ON_ANY_EXIT",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "UNBLOCK_CONDITION_UNSPECIFIED" => Some(Self::Unspecified),
            "UNBLOCK_CONDITION_ON_EXIT_SUCCESS" => Some(Self::OnExitSuccess),
            "UNBLOCK_CONDITION_ON_EXIT_FAILURE" => Some(Self::OnExitFailure),
            "UNBLOCK_CONDITION_ON_ANY_EXIT" => Some(Self::OnAnyExit),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ResizeType {
    Unspecified = 0,
    Increase = 1,
    Decrease = 2,
}
impl ResizeType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ResizeType::Unspecified => "RESIZE_TYPE_UNSPECIFIED",
            ResizeType::Increase => "RESIZE_TYPE_INCREASE",
            ResizeType::Decrease => "RESIZE_TYPE_DECREASE",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "RESIZE_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "RESIZE_TYPE_INCREASE" => Some(Self::Increase),
            "RESIZE_TYPE_DECREASE" => Some(Self::Decrease),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum ExitReason {
    Unspecified = 0,
    Normal = 1,
    NormalDetached = 2,
    ForceDetached = 3,
    CannotAttach = 4,
    Disconnect = 5,
    WebClientsForbidden = 6,
    Error = 7,
    CustomExitStatus = 8,
}
impl ExitReason {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            ExitReason::Unspecified => "EXIT_REASON_UNSPECIFIED",
            ExitReason::Normal => "EXIT_REASON_NORMAL",
            ExitReason::NormalDetached => "EXIT_REASON_NORMAL_DETACHED",
            ExitReason::ForceDetached => "EXIT_REASON_FORCE_DETACHED",
            ExitReason::CannotAttach => "EXIT_REASON_CANNOT_ATTACH",
            ExitReason::Disconnect => "EXIT_REASON_DISCONNECT",
            ExitReason::WebClientsForbidden => "EXIT_REASON_WEB_CLIENTS_FORBIDDEN",
            ExitReason::Error => "EXIT_REASON_ERROR",
            ExitReason::CustomExitStatus => "EXIT_REASON_CUSTOM_EXIT_STATUS",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "EXIT_REASON_UNSPECIFIED" => Some(Self::Unspecified),
            "EXIT_REASON_NORMAL" => Some(Self::Normal),
            "EXIT_REASON_NORMAL_DETACHED" => Some(Self::NormalDetached),
            "EXIT_REASON_FORCE_DETACHED" => Some(Self::ForceDetached),
            "EXIT_REASON_CANNOT_ATTACH" => Some(Self::CannotAttach),
            "EXIT_REASON_DISCONNECT" => Some(Self::Disconnect),
            "EXIT_REASON_WEB_CLIENTS_FORBIDDEN" => Some(Self::WebClientsForbidden),
            "EXIT_REASON_ERROR" => Some(Self::Error),
            "EXIT_REASON_CUSTOM_EXIT_STATUS" => Some(Self::CustomExitStatus),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum MouseEventType {
    Unspecified = 0,
    Press = 1,
    Release = 2,
    Motion = 3,
}
impl MouseEventType {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            MouseEventType::Unspecified => "MOUSE_EVENT_TYPE_UNSPECIFIED",
            MouseEventType::Press => "MOUSE_EVENT_TYPE_PRESS",
            MouseEventType::Release => "MOUSE_EVENT_TYPE_RELEASE",
            MouseEventType::Motion => "MOUSE_EVENT_TYPE_MOTION",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "MOUSE_EVENT_TYPE_UNSPECIFIED" => Some(Self::Unspecified),
            "MOUSE_EVENT_TYPE_PRESS" => Some(Self::Press),
            "MOUSE_EVENT_TYPE_RELEASE" => Some(Self::Release),
            "MOUSE_EVENT_TYPE_MOTION" => Some(Self::Motion),
            _ => None,
        }
    }
}
// Note: Old MousePressEvent, MouseReleaseEvent, MouseHoldEvent, MouseScrollEvent,
// MouseButton, and ScrollDirection removed - replaced with unified MouseEvent structure

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SearchDirection {
    Unspecified = 0,
    /// Changed from FORWARD to match Rust enum
    Down = 1,
    /// Changed from BACKWARD to match Rust enum
    Up = 2,
}
impl SearchDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SearchDirection::Unspecified => "SEARCH_DIRECTION_UNSPECIFIED",
            SearchDirection::Down => "SEARCH_DIRECTION_DOWN",
            SearchDirection::Up => "SEARCH_DIRECTION_UP",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SEARCH_DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "SEARCH_DIRECTION_DOWN" => Some(Self::Down),
            "SEARCH_DIRECTION_UP" => Some(Self::Up),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum SearchOption {
    Unspecified = 0,
    /// Changed from CASE_SENSITIVE to match Rust enum
    CaseSensitivity = 1,
    /// Changed from WHOLE_WORDS to match Rust enum
    WholeWord = 2,
    /// Unchanged - already matches
    Wrap = 3,
}
impl SearchOption {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            SearchOption::Unspecified => "SEARCH_OPTION_UNSPECIFIED",
            SearchOption::CaseSensitivity => "SEARCH_OPTION_CASE_SENSITIVITY",
            SearchOption::WholeWord => "SEARCH_OPTION_WHOLE_WORD",
            SearchOption::Wrap => "SEARCH_OPTION_WRAP",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "SEARCH_OPTION_UNSPECIFIED" => Some(Self::Unspecified),
            "SEARCH_OPTION_CASE_SENSITIVITY" => Some(Self::CaseSensitivity),
            "SEARCH_OPTION_WHOLE_WORD" => Some(Self::WholeWord),
            "SEARCH_OPTION_WRAP" => Some(Self::Wrap),
            _ => None,
        }
    }
}
// Additional missing supporting types

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
pub enum OnForceClose {
    Unspecified = 0,
    Quit = 1,
    Detach = 2,
}
impl OnForceClose {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            OnForceClose::Unspecified => "ON_FORCE_CLOSE_UNSPECIFIED",
            OnForceClose::Quit => "ON_FORCE_CLOSE_QUIT",
            OnForceClose::Detach => "ON_FORCE_CLOSE_DETACH",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "ON_FORCE_CLOSE_UNSPECIFIED" => Some(Self::Unspecified),
            "ON_FORCE_CLOSE_QUIT" => Some(Self::Quit),
            "ON_FORCE_CLOSE_DETACH" => Some(Self::Detach),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum Clipboard {
    Unspecified = 0,
    System = 1,
    Primary = 2,
}
impl Clipboard {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            Clipboard::Unspecified => "CLIPBOARD_UNSPECIFIED",
            Clipboard::System => "CLIPBOARD_SYSTEM",
            Clipboard::Primary => "CLIPBOARD_PRIMARY",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "CLIPBOARD_UNSPECIFIED" => Some(Self::Unspecified),
            "CLIPBOARD_SYSTEM" => Some(Self::System),
            "CLIPBOARD_PRIMARY" => Some(Self::Primary),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum StackDirection {
    Unspecified = 0,
    Horizontal = 1,
    Vertical = 2,
}
impl StackDirection {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            StackDirection::Unspecified => "STACK_DIRECTION_UNSPECIFIED",
            StackDirection::Horizontal => "STACK_DIRECTION_HORIZONTAL",
            StackDirection::Vertical => "STACK_DIRECTION_VERTICAL",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "STACK_DIRECTION_UNSPECIFIED" => Some(Self::Unspecified),
            "STACK_DIRECTION_HORIZONTAL" => Some(Self::Horizontal),
            "STACK_DIRECTION_VERTICAL" => Some(Self::Vertical),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, ::prost::Enumeration)]
#[repr(i32)]
pub enum WebSharing {
    Unspecified = 0,
    On = 1,
    Off = 2,
    Disabled = 3,
}
impl WebSharing {
    /// String value of the enum field names used in the ProtoBuf definition.
    ///
    /// The values are not transformed in any way and thus are considered stable
    /// (if the ProtoBuf definition does not change) and safe for programmatic use.
    pub fn as_str_name(&self) -> &'static str {
        match self {
            WebSharing::Unspecified => "WEB_SHARING_UNSPECIFIED",
            WebSharing::On => "WEB_SHARING_ON",
            WebSharing::Off => "WEB_SHARING_OFF",
            WebSharing::Disabled => "WEB_SHARING_DISABLED",
        }
    }
    /// Creates an enum from field names used in the ProtoBuf definition.
    pub fn from_str_name(value: &str) -> ::core::option::Option<Self> {
        match value {
            "WEB_SHARING_UNSPECIFIED" => Some(Self::Unspecified),
            "WEB_SHARING_ON" => Some(Self::On),
            "WEB_SHARING_OFF" => Some(Self::Off),
            "WEB_SHARING_DISABLED" => Some(Self::Disabled),
            _ => None,
        }
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ServerToClientMsg {
    #[prost(oneof="server_to_client_msg::Message", tags="1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13")]
    pub message: ::core::option::Option<server_to_client_msg::Message>,
}
/// Nested message and enum types in `ServerToClientMsg`.
pub mod server_to_client_msg {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Message {
        #[prost(message, tag="1")]
        Render(super::RenderMsg),
        #[prost(message, tag="2")]
        UnblockInputThread(super::UnblockInputThreadMsg),
        #[prost(message, tag="3")]
        Exit(super::ExitMsg),
        #[prost(message, tag="4")]
        Connected(super::ConnectedMsg),
        #[prost(message, tag="5")]
        Log(super::LogMsg),
        #[prost(message, tag="6")]
        LogError(super::LogErrorMsg),
        #[prost(message, tag="7")]
        SwitchSession(super::SwitchSessionMsg),
        #[prost(message, tag="8")]
        UnblockCliPipeInput(super::UnblockCliPipeInputMsg),
        #[prost(message, tag="9")]
        CliPipeOutput(super::CliPipeOutputMsg),
        #[prost(message, tag="10")]
        QueryTerminalSize(super::QueryTerminalSizeMsg),
        #[prost(message, tag="11")]
        StartWebServer(super::StartWebServerMsg),
        #[prost(message, tag="12")]
        RenamedSession(super::RenamedSessionMsg),
        #[prost(message, tag="13")]
        ConfigFileUpdated(super::ConfigFileUpdatedMsg),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenderMsg {
    #[prost(string, tag="1")]
    pub content: ::prost::alloc::string::String,
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnblockInputThreadMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ExitMsg {
    #[prost(enumeration="ExitReason", tag="1")]
    pub exit_reason: i32,
    #[prost(string, optional, tag="2")]
    pub payload: ::core::option::Option<::prost::alloc::string::String>,
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnectedMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogMsg {
    #[prost(string, repeated, tag="1")]
    pub lines: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct LogErrorMsg {
    #[prost(string, repeated, tag="1")]
    pub lines: ::prost::alloc::vec::Vec<::prost::alloc::string::String>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SwitchSessionMsg {
    #[prost(message, optional, tag="1")]
    pub connect_to_session: ::core::option::Option<ConnectToSession>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct UnblockCliPipeInputMsg {
    #[prost(string, tag="1")]
    pub pipe_name: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CliPipeOutputMsg {
    #[prost(string, tag="1")]
    pub pipe_name: ::prost::alloc::string::String,
    #[prost(string, tag="2")]
    pub output: ::prost::alloc::string::String,
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct QueryTerminalSizeMsg {
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct StartWebServerMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct RenamedSessionMsg {
    #[prost(string, tag="1")]
    pub name: ::prost::alloc::string::String,
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConfigFileUpdatedMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientToServerMsg {
    #[prost(oneof="client_to_server_msg::Message", tags="1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16")]
    pub message: ::core::option::Option<client_to_server_msg::Message>,
}
/// Nested message and enum types in `ClientToServerMsg`.
pub mod client_to_server_msg {
    #[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Oneof)]
    pub enum Message {
        #[prost(message, tag="1")]
        DetachSession(super::DetachSessionMsg),
        #[prost(message, tag="2")]
        TerminalPixelDimensions(super::TerminalPixelDimensionsMsg),
        #[prost(message, tag="3")]
        BackgroundColor(super::BackgroundColorMsg),
        #[prost(message, tag="4")]
        ForegroundColor(super::ForegroundColorMsg),
        #[prost(message, tag="5")]
        ColorRegisters(super::ColorRegistersMsg),
        #[prost(message, tag="6")]
        TerminalResize(super::TerminalResizeMsg),
        #[prost(message, tag="7")]
        FirstClientConnected(super::FirstClientConnectedMsg),
        #[prost(message, tag="8")]
        AttachClient(super::AttachClientMsg),
        #[prost(message, tag="9")]
        Action(super::ActionMsg),
        #[prost(message, tag="10")]
        Key(super::KeyMsg),
        #[prost(message, tag="11")]
        ClientExited(super::ClientExitedMsg),
        #[prost(message, tag="12")]
        KillSession(super::KillSessionMsg),
        #[prost(message, tag="13")]
        ConnStatus(super::ConnStatusMsg),
        #[prost(message, tag="14")]
        WebServerStarted(super::WebServerStartedMsg),
        #[prost(message, tag="15")]
        FailedToStartWebServer(super::FailedToStartWebServerMsg),
        #[prost(message, tag="16")]
        AttachWatcherClient(super::AttachWatcherClientMsg),
    }
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DetachSessionMsg {
    #[prost(uint32, repeated, tag="1")]
    pub client_ids: ::prost::alloc::vec::Vec<u32>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TerminalPixelDimensionsMsg {
    #[prost(message, optional, tag="1")]
    pub pixel_dimensions: ::core::option::Option<PixelDimensions>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BackgroundColorMsg {
    #[prost(string, tag="1")]
    pub color: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ForegroundColorMsg {
    #[prost(string, tag="1")]
    pub color: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ColorRegistersMsg {
    #[prost(message, repeated, tag="1")]
    pub color_registers: ::prost::alloc::vec::Vec<ColorRegister>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct TerminalResizeMsg {
    #[prost(message, optional, tag="1")]
    pub new_size: ::core::option::Option<Size>,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FirstClientConnectedMsg {
    #[prost(message, optional, tag="1")]
    pub cli_assets: ::core::option::Option<CliAssets>,
    #[prost(bool, tag="2")]
    pub is_web_client: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AttachClientMsg {
    #[prost(message, optional, tag="1")]
    pub cli_assets: ::core::option::Option<CliAssets>,
    #[prost(uint32, optional, tag="2")]
    pub tab_position_to_focus: ::core::option::Option<u32>,
    #[prost(message, optional, tag="3")]
    pub pane_to_focus: ::core::option::Option<PaneReference>,
    #[prost(bool, tag="4")]
    pub is_web_client: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AttachWatcherClientMsg {
    #[prost(message, optional, tag="1")]
    pub terminal_size: ::core::option::Option<Size>,
    #[prost(bool, tag="2")]
    pub is_web_client: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ActionMsg {
    #[prost(message, optional, tag="1")]
    pub action: ::core::option::Option<Action>,
    #[prost(uint32, optional, tag="2")]
    pub terminal_id: ::core::option::Option<u32>,
    #[prost(uint32, optional, tag="3")]
    pub client_id: ::core::option::Option<u32>,
    #[prost(bool, tag="4")]
    pub is_cli_client: bool,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KeyMsg {
    #[prost(message, optional, tag="1")]
    pub key: ::core::option::Option<KeyWithModifier>,
    #[prost(uint32, repeated, tag="2")]
    pub raw_bytes: ::prost::alloc::vec::Vec<u32>,
    #[prost(bool, tag="3")]
    pub is_kitty_keyboard_protocol: bool,
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ClientExitedMsg {
}
/// Empty message
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct KillSessionMsg {
}
/// Empty message (just indicates connection status request)
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ConnStatusMsg {
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct WebServerStartedMsg {
    #[prost(string, tag="1")]
    pub base_url: ::prost::alloc::string::String,
}
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct FailedToStartWebServerMsg {
    #[prost(string, tag="1")]
    pub error: ::prost::alloc::string::String,
}
