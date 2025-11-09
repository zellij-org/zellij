//! Things related to [`Screen`]s.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::rc::Rc;
use std::str;
use std::time::{Duration, Instant};

use crate::route::NotificationEnd;

use log::{debug, warn};
use zellij_utils::data::{
    Direction, FloatingPaneCoordinates, KeyWithModifier, NewPanePlacement, PaneContents,
    PaneManifest, PaneScrollbackResponse, PluginPermission, Resize, ResizeStrategy, SessionInfo,
    Styling, WebSharing,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::RunCommand;
use zellij_utils::input::config::Config;
use zellij_utils::input::keybinds::Keybinds;
use zellij_utils::input::mouse::MouseEvent;
use zellij_utils::input::options::Clipboard;
use zellij_utils::pane_size::{Size, SizeInPixels};
use zellij_utils::shared::clean_string_from_control_and_linebreak;
use zellij_utils::{
    consts::{session_info_folder_for_session, ZELLIJ_SOCK_DIR},
    envs::set_session_name,
    input::command::TerminalAction,
    input::layout::{
        FloatingPaneLayout, Layout, Run, RunPluginOrAlias, SplitSize, SwapFloatingLayout,
        SwapTiledLayout, TiledPaneLayout,
    },
    position::Position,
};

use crate::background_jobs::BackgroundJob;
use crate::os_input_output::ResizeCache;
use crate::pane_groups::PaneGroups;
use crate::panes::alacritty_functions::xparse_color;
use crate::panes::terminal_character::AnsiCode;
use crate::panes::terminal_pane::{BRACKETED_PASTE_BEGIN, BRACKETED_PASTE_END};
use crate::session_layout_metadata::{PaneLayoutMetadata, SessionLayoutMetadata};

use crate::{
    output::Output,
    panes::sixel::SixelImageStore,
    panes::PaneId,
    plugins::{PluginId, PluginInstruction, PluginRenderAsset},
    pty::{get_default_shell, ClientTabIndexOrPaneId, PtyInstruction, VteBytes},
    tab::{SuppressedPanes, Tab},
    thread_bus::Bus,
    ui::{
        loading_indication::LoadingIndication,
        overlay::{Overlay, OverlayWindow},
    },
    ClientId, ServerInstruction,
};
use zellij_utils::{
    data::{Event, InputMode, ModeInfo, Palette, PaletteColor, PluginCapabilities, Style, TabInfo},
    errors::{ContextType, ScreenContext},
    input::get_mode_info,
    ipc::{ClientAttributes, PixelDimensions, ServerToClientMsg},
};

/// Get the active tab and call a closure on it
///
/// If no active tab can be found, an error is logged instead.
///
/// # Parameters
///
/// - screen: An instance of `Screen` to operate on
/// - client_id: The client_id, usually taken from the `ScreenInstruction` that's being processed
/// - closure: A closure satisfying `|tab: &mut Tab| -> ()` OR `|tab: &mut Tab| -> Result<T>` (see
///   '?' below)
/// - ?: A literal "?", to append a `?` to the closure when it returns a `Result` type. This
///   argument is optional and not needed when the closure returns `()`
macro_rules! active_tab {
    ($screen:ident, $client_id:ident, $closure:expr) => {
        match $screen.get_active_tab_mut($client_id) {
            Ok(active_tab) => {
                // This could be made more ergonomic by declaring the type of 'active_tab' in the
                // closure, known as "Type Ascription". Then we could hint the type here and forego the
                // "&mut Tab" in all the closures below...
                // See: https://github.com/rust-lang/rust/issues/23416
                $closure(active_tab);
            },
            Err(err) => Err::<(), _>(err).non_fatal(),
        };
    };
    // Same as above, but with an added `?` for when the close returns a `Result` type.
    ($screen:ident, $client_id:ident, $closure:expr, ?) => {
        match $screen.get_active_tab_mut($client_id) {
            Ok(active_tab) => {
            $closure(active_tab)?;
            },
            Err(err) => Err::<(), _>(err).non_fatal(),
        };
    };
}

macro_rules! active_tab_and_connected_client_id {
    ($screen:ident, $client_id:ident, $closure:expr) => {
        match $screen.get_active_tab_mut($client_id) {
            Ok(active_tab) => {
                $closure(active_tab, $client_id);
            },
            Err(_) => {
                if let Some(client_id) = $screen.get_first_client_id() {
                    match $screen.get_active_tab_mut(client_id) {
                        Ok(active_tab) => {
                            $closure(active_tab, client_id);
                        },
                        Err(err) => Err::<(), _>(err).non_fatal(),
                    }
                } else {
                    log::error!("No client ids in screen found");
                };
            },
        }
    };
    // Same as above, but with an added `?` for when the closure returns a `Result` type.
    ($screen:ident, $client_id:ident, $closure:expr, ?) => {
        match $screen.get_active_tab_mut($client_id) {
            Ok(active_tab) => {
                $closure(active_tab, $client_id).non_fatal();
            },
            Err(_) => {
                if let Some(client_id) = $screen.get_first_client_id() {
                    match $screen.get_active_tab_mut(client_id) {
                        Ok(active_tab) => {
                            $closure(active_tab, client_id)?;
                        },
                        Err(err) => Err::<(), _>(err).non_fatal(),
                    }
                } else {
                    log::error!("No client ids in screen found");
                };
            },
        }
    };
}

type InitialTitle = String;
type HoldForCommand = Option<RunCommand>;

/// Instructions that can be sent to the [`Screen`].
#[derive(Debug, Clone)]
pub enum ScreenInstruction {
    PtyBytes(u32, VteBytes),
    PluginBytes(Vec<PluginRenderAsset>),
    Render,
    RenderToClients,
    NewPane(
        PaneId,
        Option<InitialTitle>,
        HoldForCommand,
        Option<Run>, // invoked with
        NewPanePlacement,
        bool, // start suppressed
        ClientTabIndexOrPaneId,
        Option<NotificationEnd>, // completion signal
        bool,                    // set_blocking
    ),
    OpenInPlaceEditor(PaneId, ClientTabIndexOrPaneId),
    TogglePaneEmbedOrFloating(ClientId, Option<NotificationEnd>),
    ToggleFloatingPanes(ClientId, Option<TerminalAction>, Option<NotificationEnd>),
    WriteCharacter(
        Option<KeyWithModifier>,
        Vec<u8>,
        bool,
        ClientId,
        Option<NotificationEnd>,
    ), // bool ->
    // is_kitty_keyboard_protocol
    Resize(ClientId, ResizeStrategy, Option<NotificationEnd>),
    SwitchFocus(ClientId, Option<NotificationEnd>),
    FocusNextPane(ClientId, Option<NotificationEnd>),
    FocusPreviousPane(ClientId, Option<NotificationEnd>),
    MoveFocusLeft(ClientId, Option<NotificationEnd>),
    MoveFocusLeftOrPreviousTab(ClientId, Option<NotificationEnd>),
    MoveFocusDown(ClientId, Option<NotificationEnd>),
    MoveFocusUp(ClientId, Option<NotificationEnd>),
    MoveFocusRight(ClientId, Option<NotificationEnd>),
    MoveFocusRightOrNextTab(ClientId, Option<NotificationEnd>),
    MovePane(ClientId, Option<NotificationEnd>),
    MovePaneBackwards(ClientId, Option<NotificationEnd>),
    MovePaneUp(ClientId, Option<NotificationEnd>),
    MovePaneDown(ClientId, Option<NotificationEnd>),
    MovePaneRight(ClientId, Option<NotificationEnd>),
    MovePaneLeft(ClientId, Option<NotificationEnd>),
    Exit,
    ClearScreen(ClientId, Option<NotificationEnd>),
    DumpScreen(String, ClientId, bool, Option<NotificationEnd>),
    DumpLayout(Option<PathBuf>, ClientId, Option<NotificationEnd>), // PathBuf is the default configured
    // shell
    DumpLayoutToPlugin(PluginId),
    EditScrollback(ClientId, Option<NotificationEnd>),
    GetPaneScrollback {
        pane_id: PaneId,
        client_id: ClientId,
        get_full_scrollback: bool,
        response_channel: crossbeam::channel::Sender<PaneScrollbackResponse>,
    },
    ScrollUp(ClientId, Option<NotificationEnd>),
    ScrollUpAt(Position, ClientId, Option<NotificationEnd>),
    ScrollDown(ClientId, Option<NotificationEnd>),
    ScrollDownAt(Position, ClientId, Option<NotificationEnd>),
    ScrollToBottom(ClientId, Option<NotificationEnd>),
    ScrollToTop(ClientId, Option<NotificationEnd>),
    PageScrollUp(ClientId, Option<NotificationEnd>),
    PageScrollDown(ClientId, Option<NotificationEnd>),
    HalfPageScrollUp(ClientId, Option<NotificationEnd>),
    HalfPageScrollDown(ClientId, Option<NotificationEnd>),
    ClearScroll(ClientId),
    CloseFocusedPane(ClientId, Option<NotificationEnd>),
    ToggleActiveTerminalFullscreen(ClientId, Option<NotificationEnd>),
    TogglePaneFrames(Option<NotificationEnd>),
    SetSelectable(PaneId, bool),
    ClosePane(
        PaneId,
        Option<ClientId>,
        Option<NotificationEnd>,
        Option<i32>,
    ), // i32 -> optional exit
    // status
    HoldPane(PaneId, Option<i32>, RunCommand),
    UpdatePaneName(Vec<u8>, ClientId, Option<NotificationEnd>),
    UndoRenamePane(ClientId, Option<NotificationEnd>),
    NewTab(
        Option<PathBuf>,
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        Option<String>,
        (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>), // swap layouts
        bool,                                            // should_change_focus_to_new_tab
        (ClientId, bool),                                // bool -> is_web_client
        Option<NotificationEnd>,                         // completion signal
    ),
    ApplyLayout(
        TiledPaneLayout,
        Vec<FloatingPaneLayout>,
        Vec<(u32, HoldForCommand)>, // new pane pids
        Vec<(u32, HoldForCommand)>, // new floating pane pids
        HashMap<RunPluginOrAlias, Vec<u32>>,
        usize,                   // tab_index
        bool,                    // should change focus to new tab
        (ClientId, bool),        // bool -> is_web_client
        Option<NotificationEnd>, // completion signal
    ),
    SwitchTabNext(ClientId, Option<NotificationEnd>),
    SwitchTabPrev(ClientId, Option<NotificationEnd>),
    ToggleActiveSyncTab(ClientId, Option<NotificationEnd>),
    CloseTab(ClientId, Option<NotificationEnd>),
    GoToTab(u32, Option<ClientId>, Option<NotificationEnd>), // this Option is a hacky workaround, please do not copy this behaviour
    GoToTabName(
        String,
        (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>), // swap layouts
        Option<TerminalAction>,                          // default_shell
        bool,
        Option<ClientId>,
        Option<NotificationEnd>,
    ),
    ToggleTab(ClientId, Option<NotificationEnd>),
    UpdateTabName(Vec<u8>, ClientId, Option<NotificationEnd>),
    UndoRenameTab(ClientId, Option<NotificationEnd>),
    MoveTabLeft(ClientId, Option<NotificationEnd>),
    MoveTabRight(ClientId, Option<NotificationEnd>),
    TerminalResize(Size),
    TerminalPixelDimensions(PixelDimensions),
    TerminalBackgroundColor(String),
    TerminalForegroundColor(String),
    TerminalColorRegisters(Vec<(usize, String)>),
    ChangeMode(ModeInfo, ClientId, Option<NotificationEnd>),
    ChangeModeForAllClients(ModeInfo, Option<NotificationEnd>),
    MouseEvent(MouseEvent, ClientId, Option<NotificationEnd>),
    Copy(ClientId, Option<NotificationEnd>),
    AddClient(
        ClientId,
        bool,                // is_web_client
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane_id to focus
    ),
    RemoveClient(ClientId),
    AddOverlay(Overlay, ClientId),
    RemoveOverlay(ClientId),
    ConfirmPrompt(ClientId, Option<NotificationEnd>),
    DenyPrompt(ClientId, Option<NotificationEnd>),
    UpdateSearch(Vec<u8>, ClientId, Option<NotificationEnd>),
    SearchDown(ClientId, Option<NotificationEnd>),
    SearchUp(ClientId, Option<NotificationEnd>),
    SearchToggleCaseSensitivity(ClientId, Option<NotificationEnd>),
    SearchToggleWholeWord(ClientId, Option<NotificationEnd>),
    SearchToggleWrap(ClientId, Option<NotificationEnd>),
    AddRedPaneFrameColorOverride(Vec<PaneId>, Option<String>), // Option<String> => optional error text
    ClearPaneFrameColorOverride(Vec<PaneId>),
    PreviousSwapLayout(ClientId, Option<NotificationEnd>),
    NextSwapLayout(ClientId, Option<NotificationEnd>),
    QueryTabNames(ClientId, Option<NotificationEnd>),
    NewTiledPluginPane(
        RunPluginOrAlias,
        Option<String>,
        bool,
        Option<PathBuf>,
        ClientId,
        Option<NotificationEnd>,
    ), // Option<String> is
    // optional pane title, bool is skip cache, Option<PathBuf> is an optional cwd
    NewFloatingPluginPane(
        RunPluginOrAlias,
        Option<String>,
        bool,
        Option<PathBuf>,
        Option<FloatingPaneCoordinates>,
        ClientId,
        Option<NotificationEnd>,
    ), // Option<String> is an
    // optional pane title, bool
    // is skip cache, Option<PathBuf> is an optional cwd
    NewInPlacePluginPane(
        RunPluginOrAlias,
        Option<String>,
        PaneId,
        bool,
        ClientId,
        Option<NotificationEnd>,
    ), // Option<String> is an
    // optional pane title, bool is skip cache
    StartOrReloadPluginPane(RunPluginOrAlias, Option<String>, Option<NotificationEnd>),
    AddPlugin(
        Option<bool>, // should_float
        bool,         // should be opened in place
        RunPluginOrAlias,
        Option<String>, // pane title
        Option<usize>,  // tab index
        u32,            // plugin id
        Option<PaneId>,
        Option<PathBuf>, // cwd
        bool,            // start suppressed
        Option<FloatingPaneCoordinates>,
        Option<bool>, // should focus plugin
        Option<ClientId>,
        Option<NotificationEnd>, // completion signal
    ),
    UpdatePluginLoadingStage(u32, LoadingIndication), // u32 - plugin_id
    StartPluginLoadingIndication(u32, LoadingIndication), // u32 - plugin_id
    ProgressPluginLoadingOffset(u32),                 // u32 - plugin id
    RequestStateUpdateForPlugins,
    LaunchOrFocusPlugin(
        RunPluginOrAlias,
        bool,
        bool,
        bool,
        Option<PaneId>,
        bool,
        ClientId,
        Option<NotificationEnd>,
    ), // bools are: should_float, move_to_focused_tab, should_open_in_place, Option<PaneId> is the pane id to replace, bool following it is skip_cache
    LaunchPlugin(
        RunPluginOrAlias,
        bool,
        bool,
        Option<PaneId>,
        bool,
        Option<PathBuf>,
        ClientId,
        Option<NotificationEnd>,
    ), // bools are: should_float, should_open_in_place Option<PaneId> is the pane id to replace, Option<PathBuf> is an optional cwd, bool after is skip_cache
    SuppressPane(PaneId, ClientId), // bool is should_float
    FocusPaneWithId(PaneId, bool, ClientId, Option<NotificationEnd>), // bool is should_float
    RenamePane(PaneId, Vec<u8>, Option<NotificationEnd>),
    RenameTab(usize, Vec<u8>, Option<NotificationEnd>),
    RequestPluginPermissions(
        u32, // u32 - plugin_id
        PluginPermission,
    ),
    BreakPane(
        Box<Layout>,
        Option<TerminalAction>,
        ClientId,
        Option<NotificationEnd>,
    ),
    BreakPaneRight(ClientId, Option<NotificationEnd>),
    BreakPaneLeft(ClientId, Option<NotificationEnd>),
    UpdateSessionInfos(
        BTreeMap<String, SessionInfo>, // String is the session name
        BTreeMap<String, Duration>,    // resurrectable sessions - <name, created>
    ),
    ReplacePane(
        PaneId,
        HoldForCommand,
        Option<InitialTitle>,
        Option<Run>,
        bool, // close replaced pane
        ClientTabIndexOrPaneId,
        Option<NotificationEnd>, // completion signal
    ),
    SerializeLayoutForResurrection,
    RenameSession(String, ClientId, Option<NotificationEnd>), // String -> new name
    ListClientsMetadata(Option<PathBuf>, ClientId, Option<NotificationEnd>), // Option<PathBuf> - default shell
    Reconfigure {
        client_id: ClientId,
        keybinds: Keybinds,
        default_mode: InputMode,
        theme: Styling,
        simplified_ui: bool,
        default_shell: Option<PathBuf>,
        pane_frames: bool,
        copy_command: Option<String>,
        copy_to_clipboard: Option<Clipboard>,
        copy_on_select: bool,
        auto_layout: bool,
        rounded_corners: bool,
        hide_session_name: bool,
        tabline_prefix_text: Option<String>,
        stacked_resize: bool,
        default_editor: Option<PathBuf>,
        advanced_mouse_actions: bool,
    },
    RerunCommandPane(u32, Option<NotificationEnd>), // u32 - terminal pane id
    ResizePaneWithId(ResizeStrategy, PaneId),
    EditScrollbackForPaneWithId(PaneId, Option<NotificationEnd>),
    WriteToPaneId(Vec<u8>, PaneId),
    MovePaneWithPaneId(PaneId),
    MovePaneWithPaneIdInDirection(PaneId, Direction),
    ClearScreenForPaneId(PaneId),
    ScrollUpInPaneId(PaneId),
    ScrollDownInPaneId(PaneId),
    ScrollToTopInPaneId(PaneId),
    ScrollToBottomInPaneId(PaneId),
    PageScrollUpInPaneId(PaneId),
    PageScrollDownInPaneId(PaneId),
    TogglePaneIdFullscreen(PaneId),
    TogglePaneEmbedOrEjectForPaneId(PaneId),
    CloseTabWithIndex(usize),
    BreakPanesToNewTab {
        pane_ids: Vec<PaneId>,
        default_shell: Option<TerminalAction>,
        should_change_focus_to_new_tab: bool,
        new_tab_name: Option<String>,
        client_id: ClientId,
    },
    BreakPanesToTabWithIndex {
        pane_ids: Vec<PaneId>,
        tab_index: usize,
        should_change_focus_to_new_tab: bool,
        client_id: ClientId,
    },
    ListClientsToPlugin(PluginId, ClientId),
    TogglePanePinned(ClientId, Option<NotificationEnd>),
    SetFloatingPanePinned(PaneId, bool),
    StackPanes(Vec<PaneId>, ClientId, Option<NotificationEnd>),
    ChangeFloatingPanesCoordinates(
        Vec<(PaneId, FloatingPaneCoordinates)>,
        Option<NotificationEnd>,
    ),
    AddHighlightPaneFrameColorOverride(Vec<PaneId>, Option<String>), // Option<String> => optional
    // message
    GroupAndUngroupPanes(Vec<PaneId>, Vec<PaneId>, bool, ClientId), // panes_to_group, panes_to_ungroup, bool -> for all clients
    HighlightAndUnhighlightPanes(Vec<PaneId>, Vec<PaneId>, ClientId), // panes_to_highlight, panes_to_unhighlight
    FloatMultiplePanes(Vec<PaneId>, ClientId),
    EmbedMultiplePanes(Vec<PaneId>, ClientId),
    TogglePaneInGroup(ClientId, Option<NotificationEnd>),
    ToggleGroupMarking(ClientId, Option<NotificationEnd>),
    SessionSharingStatusChange(bool),
    SetMouseSelectionSupport(PaneId, bool),
    InterceptKeyPresses(PluginId, ClientId),
    ClearKeyPressesIntercepts(ClientId),
    ReplacePaneWithExistingPane(PaneId, PaneId),
    AddWatcherClient(ClientId, Size),
    RemoveWatcherClient(ClientId),
    SetFollowedClient(ClientId),
    WatcherTerminalResize(ClientId, Size),
}

impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::PtyBytes(..) => ScreenContext::HandlePtyBytes,
            ScreenInstruction::PluginBytes(..) => ScreenContext::PluginBytes,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::RenderToClients => ScreenContext::RenderToClients,
            ScreenInstruction::NewPane(..) => ScreenContext::NewPane,
            ScreenInstruction::OpenInPlaceEditor(..) => ScreenContext::OpenInPlaceEditor,
            ScreenInstruction::TogglePaneEmbedOrFloating(..) => {
                ScreenContext::TogglePaneEmbedOrFloating
            },
            ScreenInstruction::ToggleFloatingPanes(..) => ScreenContext::ToggleFloatingPanes,
            ScreenInstruction::WriteCharacter(..) => ScreenContext::WriteCharacter,
            ScreenInstruction::Resize(.., strategy, _) => match strategy {
                ResizeStrategy {
                    resize: Resize::Increase,
                    direction,
                    ..
                } => match direction {
                    Some(Direction::Left) => ScreenContext::ResizeIncreaseLeft,
                    Some(Direction::Down) => ScreenContext::ResizeIncreaseDown,
                    Some(Direction::Up) => ScreenContext::ResizeIncreaseUp,
                    Some(Direction::Right) => ScreenContext::ResizeIncreaseRight,
                    None => ScreenContext::ResizeIncreaseAll,
                },
                ResizeStrategy {
                    resize: Resize::Decrease,
                    direction,
                    ..
                } => match direction {
                    Some(Direction::Left) => ScreenContext::ResizeDecreaseLeft,
                    Some(Direction::Down) => ScreenContext::ResizeDecreaseDown,
                    Some(Direction::Up) => ScreenContext::ResizeDecreaseUp,
                    Some(Direction::Right) => ScreenContext::ResizeDecreaseRight,
                    None => ScreenContext::ResizeDecreaseAll,
                },
            },
            ScreenInstruction::SwitchFocus(..) => ScreenContext::SwitchFocus,
            ScreenInstruction::FocusNextPane(..) => ScreenContext::FocusNextPane,
            ScreenInstruction::FocusPreviousPane(..) => ScreenContext::FocusPreviousPane,
            ScreenInstruction::MoveFocusLeft(..) => ScreenContext::MoveFocusLeft,
            ScreenInstruction::MoveFocusLeftOrPreviousTab(..) => {
                ScreenContext::MoveFocusLeftOrPreviousTab
            },
            ScreenInstruction::MoveFocusDown(..) => ScreenContext::MoveFocusDown,
            ScreenInstruction::MoveFocusUp(..) => ScreenContext::MoveFocusUp,
            ScreenInstruction::MoveFocusRight(..) => ScreenContext::MoveFocusRight,
            ScreenInstruction::MoveFocusRightOrNextTab(..) => {
                ScreenContext::MoveFocusRightOrNextTab
            },
            ScreenInstruction::MovePane(..) => ScreenContext::MovePane,
            ScreenInstruction::MovePaneBackwards(..) => ScreenContext::MovePaneBackwards,
            ScreenInstruction::MovePaneDown(..) => ScreenContext::MovePaneDown,
            ScreenInstruction::MovePaneUp(..) => ScreenContext::MovePaneUp,
            ScreenInstruction::MovePaneRight(..) => ScreenContext::MovePaneRight,
            ScreenInstruction::MovePaneLeft(..) => ScreenContext::MovePaneLeft,
            ScreenInstruction::Exit => ScreenContext::Exit,
            ScreenInstruction::ClearScreen(..) => ScreenContext::ClearScreen,
            ScreenInstruction::DumpScreen(..) => ScreenContext::DumpScreen,
            ScreenInstruction::DumpLayout(..) => ScreenContext::DumpLayout,
            ScreenInstruction::DumpLayoutToPlugin(..) => ScreenContext::DumpLayoutToPlugin,
            ScreenInstruction::EditScrollback(..) => ScreenContext::EditScrollback,
            ScreenInstruction::GetPaneScrollback { .. } => ScreenContext::GetPaneScrollback,
            ScreenInstruction::ScrollUp(..) => ScreenContext::ScrollUp,
            ScreenInstruction::ScrollDown(..) => ScreenContext::ScrollDown,
            ScreenInstruction::ScrollToBottom(..) => ScreenContext::ScrollToBottom,
            ScreenInstruction::ScrollToTop(..) => ScreenContext::ScrollToTop,
            ScreenInstruction::PageScrollUp(..) => ScreenContext::PageScrollUp,
            ScreenInstruction::PageScrollDown(..) => ScreenContext::PageScrollDown,
            ScreenInstruction::HalfPageScrollUp(..) => ScreenContext::HalfPageScrollUp,
            ScreenInstruction::HalfPageScrollDown(..) => ScreenContext::HalfPageScrollDown,
            ScreenInstruction::ClearScroll(..) => ScreenContext::ClearScroll,
            ScreenInstruction::CloseFocusedPane(..) => ScreenContext::CloseFocusedPane,
            ScreenInstruction::ToggleActiveTerminalFullscreen(..) => {
                ScreenContext::ToggleActiveTerminalFullscreen
            },
            ScreenInstruction::TogglePaneFrames(..) => ScreenContext::TogglePaneFrames,
            ScreenInstruction::SetSelectable(..) => ScreenContext::SetSelectable,
            ScreenInstruction::ClosePane(..) => ScreenContext::ClosePane,
            ScreenInstruction::HoldPane(..) => ScreenContext::HoldPane,
            ScreenInstruction::UpdatePaneName(..) => ScreenContext::UpdatePaneName,
            ScreenInstruction::UndoRenamePane(..) => ScreenContext::UndoRenamePane,
            ScreenInstruction::NewTab(..) => ScreenContext::NewTab,
            ScreenInstruction::ApplyLayout(..) => ScreenContext::ApplyLayout,
            ScreenInstruction::SwitchTabNext(..) => ScreenContext::SwitchTabNext,
            ScreenInstruction::SwitchTabPrev(..) => ScreenContext::SwitchTabPrev,
            ScreenInstruction::CloseTab(..) => ScreenContext::CloseTab,
            ScreenInstruction::GoToTab(..) => ScreenContext::GoToTab,
            ScreenInstruction::GoToTabName(..) => ScreenContext::GoToTabName,
            ScreenInstruction::UpdateTabName(..) => ScreenContext::UpdateTabName,
            ScreenInstruction::UndoRenameTab(..) => ScreenContext::UndoRenameTab,
            ScreenInstruction::MoveTabLeft(..) => ScreenContext::MoveTabLeft,
            ScreenInstruction::MoveTabRight(..) => ScreenContext::MoveTabRight,
            ScreenInstruction::TerminalResize(..) => ScreenContext::TerminalResize,
            ScreenInstruction::TerminalPixelDimensions(..) => {
                ScreenContext::TerminalPixelDimensions
            },
            ScreenInstruction::TerminalBackgroundColor(..) => {
                ScreenContext::TerminalBackgroundColor
            },
            ScreenInstruction::TerminalForegroundColor(..) => {
                ScreenContext::TerminalForegroundColor
            },
            ScreenInstruction::TerminalColorRegisters(..) => ScreenContext::TerminalColorRegisters,
            ScreenInstruction::ChangeMode(..) => ScreenContext::ChangeMode,
            ScreenInstruction::ChangeModeForAllClients(..) => {
                ScreenContext::ChangeModeForAllClients
            },
            ScreenInstruction::ToggleActiveSyncTab(..) => ScreenContext::ToggleActiveSyncTab,
            ScreenInstruction::ScrollUpAt(..) => ScreenContext::ScrollUpAt,
            ScreenInstruction::ScrollDownAt(..) => ScreenContext::ScrollDownAt,
            ScreenInstruction::MouseEvent(..) => ScreenContext::MouseEvent,
            ScreenInstruction::Copy(..) => ScreenContext::Copy,
            ScreenInstruction::ToggleTab(..) => ScreenContext::ToggleTab,
            ScreenInstruction::AddClient(..) => ScreenContext::AddClient,
            ScreenInstruction::RemoveClient(..) => ScreenContext::RemoveClient,
            ScreenInstruction::AddOverlay(..) => ScreenContext::AddOverlay,
            ScreenInstruction::RemoveOverlay(..) => ScreenContext::RemoveOverlay,
            ScreenInstruction::ConfirmPrompt(..) => ScreenContext::ConfirmPrompt,
            ScreenInstruction::DenyPrompt(..) => ScreenContext::DenyPrompt,
            ScreenInstruction::UpdateSearch(..) => ScreenContext::UpdateSearch,
            ScreenInstruction::SearchDown(..) => ScreenContext::SearchDown,
            ScreenInstruction::SearchUp(..) => ScreenContext::SearchUp,
            ScreenInstruction::SearchToggleCaseSensitivity(..) => {
                ScreenContext::SearchToggleCaseSensitivity
            },
            ScreenInstruction::SearchToggleWholeWord(..) => ScreenContext::SearchToggleWholeWord,
            ScreenInstruction::SearchToggleWrap(..) => ScreenContext::SearchToggleWrap,
            ScreenInstruction::AddRedPaneFrameColorOverride(..) => {
                ScreenContext::AddRedPaneFrameColorOverride
            },
            ScreenInstruction::ClearPaneFrameColorOverride(..) => {
                ScreenContext::ClearPaneFrameColorOverride
            },
            ScreenInstruction::PreviousSwapLayout(..) => ScreenContext::PreviousSwapLayout,
            ScreenInstruction::NextSwapLayout(..) => ScreenContext::NextSwapLayout,
            ScreenInstruction::QueryTabNames(..) => ScreenContext::QueryTabNames,
            ScreenInstruction::NewTiledPluginPane(..) => ScreenContext::NewTiledPluginPane,
            ScreenInstruction::NewFloatingPluginPane(..) => ScreenContext::NewFloatingPluginPane,
            ScreenInstruction::StartOrReloadPluginPane(..) => {
                ScreenContext::StartOrReloadPluginPane
            },
            ScreenInstruction::AddPlugin(..) => ScreenContext::AddPlugin,
            ScreenInstruction::UpdatePluginLoadingStage(..) => {
                ScreenContext::UpdatePluginLoadingStage
            },
            ScreenInstruction::ProgressPluginLoadingOffset(..) => {
                ScreenContext::ProgressPluginLoadingOffset
            },
            ScreenInstruction::StartPluginLoadingIndication(..) => {
                ScreenContext::StartPluginLoadingIndication
            },
            ScreenInstruction::RequestStateUpdateForPlugins => {
                ScreenContext::RequestStateUpdateForPlugins
            },
            ScreenInstruction::LaunchOrFocusPlugin(..) => ScreenContext::LaunchOrFocusPlugin,
            ScreenInstruction::LaunchPlugin(..) => ScreenContext::LaunchPlugin,
            ScreenInstruction::SuppressPane(..) => ScreenContext::SuppressPane,
            ScreenInstruction::FocusPaneWithId(..) => ScreenContext::FocusPaneWithId,
            ScreenInstruction::RenamePane(..) => ScreenContext::RenamePane,
            ScreenInstruction::RenameTab(..) => ScreenContext::RenameTab,
            ScreenInstruction::RequestPluginPermissions(..) => {
                ScreenContext::RequestPluginPermissions
            },
            ScreenInstruction::BreakPane(..) => ScreenContext::BreakPane,
            ScreenInstruction::BreakPaneRight(..) => ScreenContext::BreakPaneRight,
            ScreenInstruction::BreakPaneLeft(..) => ScreenContext::BreakPaneLeft,
            ScreenInstruction::UpdateSessionInfos(..) => ScreenContext::UpdateSessionInfos,
            ScreenInstruction::ReplacePane(..) => ScreenContext::ReplacePane,
            ScreenInstruction::NewInPlacePluginPane(..) => ScreenContext::NewInPlacePluginPane,
            ScreenInstruction::SerializeLayoutForResurrection => {
                ScreenContext::SerializeLayoutForResurrection
            },
            ScreenInstruction::RenameSession(..) => ScreenContext::RenameSession,
            ScreenInstruction::ListClientsMetadata(..) => ScreenContext::ListClientsMetadata,
            ScreenInstruction::Reconfigure { .. } => ScreenContext::Reconfigure,
            ScreenInstruction::RerunCommandPane { .. } => ScreenContext::RerunCommandPane,
            ScreenInstruction::ResizePaneWithId(..) => ScreenContext::ResizePaneWithId,
            ScreenInstruction::EditScrollbackForPaneWithId(..) => {
                ScreenContext::EditScrollbackForPaneWithId
            },
            ScreenInstruction::WriteToPaneId(..) => ScreenContext::WriteToPaneId,
            ScreenInstruction::MovePaneWithPaneId(..) => ScreenContext::MovePaneWithPaneId,
            ScreenInstruction::MovePaneWithPaneIdInDirection(..) => {
                ScreenContext::MovePaneWithPaneIdInDirection
            },
            ScreenInstruction::ClearScreenForPaneId(..) => ScreenContext::ClearScreenForPaneId,
            ScreenInstruction::ScrollUpInPaneId(..) => ScreenContext::ScrollUpInPaneId,
            ScreenInstruction::ScrollDownInPaneId(..) => ScreenContext::ScrollDownInPaneId,
            ScreenInstruction::ScrollToTopInPaneId(..) => ScreenContext::ScrollToTopInPaneId,
            ScreenInstruction::ScrollToBottomInPaneId(..) => ScreenContext::ScrollToBottomInPaneId,
            ScreenInstruction::PageScrollUpInPaneId(..) => ScreenContext::PageScrollUpInPaneId,
            ScreenInstruction::PageScrollDownInPaneId(..) => ScreenContext::PageScrollDownInPaneId,
            ScreenInstruction::TogglePaneIdFullscreen(..) => ScreenContext::TogglePaneIdFullscreen,
            ScreenInstruction::TogglePaneEmbedOrEjectForPaneId(..) => {
                ScreenContext::TogglePaneEmbedOrEjectForPaneId
            },
            ScreenInstruction::CloseTabWithIndex(..) => ScreenContext::CloseTabWithIndex,
            ScreenInstruction::BreakPanesToNewTab { .. } => ScreenContext::BreakPanesToNewTab,
            ScreenInstruction::BreakPanesToTabWithIndex { .. } => {
                ScreenContext::BreakPanesToTabWithIndex
            },
            ScreenInstruction::ListClientsToPlugin(..) => ScreenContext::ListClientsToPlugin,
            ScreenInstruction::TogglePanePinned(..) => ScreenContext::TogglePanePinned,
            ScreenInstruction::SetFloatingPanePinned(..) => ScreenContext::SetFloatingPanePinned,
            ScreenInstruction::StackPanes(..) => ScreenContext::StackPanes,
            ScreenInstruction::ChangeFloatingPanesCoordinates(..) => {
                ScreenContext::ChangeFloatingPanesCoordinates
            },
            ScreenInstruction::AddHighlightPaneFrameColorOverride(..) => {
                ScreenContext::AddHighlightPaneFrameColorOverride
            },
            ScreenInstruction::GroupAndUngroupPanes(..) => ScreenContext::GroupAndUngroupPanes,
            ScreenInstruction::HighlightAndUnhighlightPanes(..) => {
                ScreenContext::HighlightAndUnhighlightPanes
            },
            ScreenInstruction::FloatMultiplePanes(..) => ScreenContext::FloatMultiplePanes,
            ScreenInstruction::EmbedMultiplePanes(..) => ScreenContext::EmbedMultiplePanes,
            ScreenInstruction::TogglePaneInGroup(..) => ScreenContext::TogglePaneInGroup,
            ScreenInstruction::ToggleGroupMarking(..) => ScreenContext::ToggleGroupMarking,
            ScreenInstruction::SessionSharingStatusChange(..) => {
                ScreenContext::SessionSharingStatusChange
            },
            ScreenInstruction::SetMouseSelectionSupport(..) => {
                ScreenContext::SetMouseSelectionSupport
            },
            ScreenInstruction::InterceptKeyPresses(..) => ScreenContext::InterceptKeyPresses,
            ScreenInstruction::ClearKeyPressesIntercepts(..) => {
                ScreenContext::ClearKeyPressesIntercepts
            },
            ScreenInstruction::ReplacePaneWithExistingPane(..) => {
                ScreenContext::ReplacePaneWithExistingPane
            },
            ScreenInstruction::AddWatcherClient(..) => ScreenContext::AddWatcherClient,
            ScreenInstruction::RemoveWatcherClient(..) => ScreenContext::RemoveWatcherClient,
            ScreenInstruction::SetFollowedClient(..) => ScreenContext::SetFollowedClient,
            ScreenInstruction::WatcherTerminalResize(..) => ScreenContext::WatcherTerminalResize, // NEW
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CopyOptions {
    pub command: Option<String>,
    pub clipboard: Clipboard,
    pub copy_on_select: bool,
}

impl CopyOptions {
    pub(crate) fn new(
        copy_command: Option<String>,
        copy_clipboard: Clipboard,
        copy_on_select: bool,
    ) -> Self {
        Self {
            command: copy_command,
            clipboard: copy_clipboard,
            copy_on_select,
        }
    }

    #[cfg(test)]
    pub(crate) fn default() -> Self {
        Self {
            command: None,
            clipboard: Clipboard::default(),
            copy_on_select: true,
        }
    }
}

// We use this to delay rendering when a new tab opens so that we make sure all plugins
// (representing portions of the UI) have been fully loaded before the tab is first rendered (with
// a sensible timeout of 100ms)
#[derive(Debug, Clone)]
pub struct RenderBlocker {
    blocking_plugins: HashMap<u32, Instant>,
    timeout_ms: u64,
}

impl RenderBlocker {
    pub fn new(timeout_ms: u64) -> Self {
        Self {
            blocking_plugins: HashMap::new(),
            timeout_ms,
        }
    }

    pub fn register_blocking_plugin(&mut self, plugin_id: u32) {
        self.blocking_plugins.insert(plugin_id, Instant::now());
    }

    pub fn remove_blocking_plugin(&mut self, plugin_id: u32) {
        self.blocking_plugins.remove(&plugin_id);
    }

    #[cfg(test)]
    pub fn can_render(&mut self) -> bool {
        // we want the tests to be more deterministic and so we always render without any
        // optimizations
        true
    }

    #[cfg(not(test))]
    pub fn can_render(&mut self) -> bool {
        let ret = if self.blocking_plugins.is_empty() {
            true
        } else {
            let timeout = Duration::from_millis(self.timeout_ms);
            let now = Instant::now();

            self.blocking_plugins
                .values()
                .all(|&registered_at| now.duration_since(registered_at) >= timeout)
        };
        if ret {
            self.blocking_plugins.clear();
        }
        ret
    }
}

/// State information for a watcher client
#[derive(Debug, Clone)]
pub(crate) struct WatcherState {
    size: Size,
    should_force_render: bool,
}

impl WatcherState {
    pub fn new(size: Size) -> Self {
        WatcherState {
            size,
            should_force_render: true,
        }
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        self.size = size;
    }

    pub fn should_force_render(&self) -> bool {
        self.should_force_render
    }

    pub fn clear_force_render(&mut self) {
        self.should_force_render = false;
    }

    pub fn set_force_render(&mut self) {
        self.should_force_render = true;
    }
}

/// A [`Screen`] holds multiple [`Tab`]s, each one holding multiple [`panes`](crate::client::panes).
/// It only directly controls which tab is active, delegating the rest to the individual `Tab`.
pub(crate) struct Screen {
    /// A Bus for sending and receiving messages with the other threads.
    pub bus: Bus<ScreenInstruction>,
    /// An optional maximal amount of panes allowed per [`Tab`] in this [`Screen`] instance.
    max_panes: Option<usize>,
    /// A map between this [`Screen`]'s tabs and their ID/key.
    tabs: BTreeMap<usize, Tab>,
    /// The full size of this [`Screen`].
    size: Size,
    pixel_dimensions: PixelDimensions,
    character_cell_size: Rc<RefCell<Option<SizeInPixels>>>,
    stacked_resize: Rc<RefCell<bool>>,
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    /// The overlay that is drawn on top of [`Pane`]'s', [`Tab`]'s and the [`Screen`]
    overlay: OverlayWindow,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    connected_clients: Rc<RefCell<HashMap<ClientId, bool>>>, // bool -> is_web_client
    /// The indices of this [`Screen`]'s active [`Tab`]s.
    active_tab_indices: BTreeMap<ClientId, usize>,
    tab_history: BTreeMap<ClientId, Vec<usize>>,
    mode_info: BTreeMap<ClientId, ModeInfo>,
    default_mode_info: ModeInfo, // TODO: restructure ModeInfo to prevent this duplication
    style: Style,
    draw_pane_frames: bool,
    auto_layout: bool,
    session_serialization: bool,
    serialize_pane_viewport: bool,
    scrollback_lines_to_serialize: Option<usize>,
    session_is_mirrored: bool,
    copy_options: CopyOptions,
    debug: bool,
    session_name: String,
    session_infos_on_machine: BTreeMap<String, SessionInfo>, // String is the session name, can
    // also be this session
    resurrectable_sessions: BTreeMap<String, Duration>, // String is the session name, duration is
    // its creation time
    default_layout: Box<Layout>,
    default_shell: PathBuf,
    styled_underlines: bool,
    arrow_fonts: bool,
    layout_dir: Option<PathBuf>,
    default_layout_name: Option<String>,
    explicitly_disable_kitty_keyboard_protocol: bool,
    default_editor: Option<PathBuf>,
    web_clients_allowed: bool,
    web_sharing: WebSharing,
    current_pane_group: Rc<RefCell<PaneGroups>>,
    advanced_mouse_actions: bool,
    currently_marking_pane_group: Rc<RefCell<HashMap<ClientId, bool>>>,
    // the below are the configured values - the ones that will be set if and when the web server
    // is brought online
    web_server_ip: IpAddr,
    web_server_port: u16,
    render_blocker: RenderBlocker,
    watcher_clients: HashMap<ClientId, WatcherState>,
    followed_client_id: Option<ClientId>,
}

impl Screen {
    /// Creates and returns a new [`Screen`].
    pub fn new(
        bus: Bus<ScreenInstruction>,
        client_attributes: &ClientAttributes,
        max_panes: Option<usize>,
        mode_info: ModeInfo,
        draw_pane_frames: bool,
        auto_layout: bool,
        session_is_mirrored: bool,
        copy_options: CopyOptions,
        debug: bool,
        default_layout: Box<Layout>,
        default_layout_name: Option<String>,
        default_shell: PathBuf,
        session_serialization: bool,
        serialize_pane_viewport: bool,
        scrollback_lines_to_serialize: Option<usize>,
        styled_underlines: bool,
        arrow_fonts: bool,
        layout_dir: Option<PathBuf>,
        explicitly_disable_kitty_keyboard_protocol: bool,
        stacked_resize: bool,
        default_editor: Option<PathBuf>,
        web_clients_allowed: bool,
        web_sharing: WebSharing,
        advanced_mouse_actions: bool,
        web_server_ip: IpAddr,
        web_server_port: u16,
    ) -> Self {
        let session_name = mode_info.session_name.clone().unwrap_or_default();
        let session_info = SessionInfo::new(session_name.clone());
        let mut session_infos_on_machine = BTreeMap::new();
        let resurrectable_sessions = BTreeMap::new();
        session_infos_on_machine.insert(session_name.clone(), session_info);
        let current_pane_group = PaneGroups::new(bus.senders.clone());
        Screen {
            bus,
            max_panes,
            size: client_attributes.size,
            pixel_dimensions: Default::default(),
            character_cell_size: Rc::new(RefCell::new(None)),
            stacked_resize: Rc::new(RefCell::new(stacked_resize)),
            sixel_image_store: Rc::new(RefCell::new(SixelImageStore::default())),
            style: client_attributes.style.clone(),
            connected_clients: Rc::new(RefCell::new(HashMap::new())),
            active_tab_indices: BTreeMap::new(),
            tabs: BTreeMap::new(),
            overlay: OverlayWindow::default(),
            terminal_emulator_colors: Rc::new(RefCell::new(Palette::default())),
            terminal_emulator_color_codes: Rc::new(RefCell::new(HashMap::new())),
            tab_history: BTreeMap::new(),
            mode_info: BTreeMap::new(),
            default_mode_info: mode_info,
            draw_pane_frames,
            auto_layout,
            session_is_mirrored,
            copy_options,
            debug,
            session_name,
            session_infos_on_machine,
            default_layout,
            default_layout_name,
            default_shell,
            session_serialization,
            serialize_pane_viewport,
            scrollback_lines_to_serialize,
            styled_underlines,
            arrow_fonts,
            resurrectable_sessions,
            layout_dir,
            explicitly_disable_kitty_keyboard_protocol,
            default_editor,
            web_clients_allowed,
            web_sharing,
            current_pane_group: Rc::new(RefCell::new(current_pane_group)),
            currently_marking_pane_group: Rc::new(RefCell::new(HashMap::new())),
            advanced_mouse_actions,
            web_server_ip,
            web_server_port,
            render_blocker: RenderBlocker::new(100),
            watcher_clients: HashMap::new(),
            followed_client_id: None,
        }
    }

    /// Returns the index where a new [`Tab`] should be created in this [`Screen`].
    /// Currently, this is right after the last currently existing tab, or `0` if
    /// no tabs exist in this screen yet.
    fn get_new_tab_index(&self) -> usize {
        if let Some(index) = self.tabs.keys().last() {
            *index + 1
        } else {
            0
        }
    }

    fn move_clients_from_closed_tab(
        &mut self,
        client_ids_and_mode_infos: Vec<(ClientId, ModeInfo)>,
    ) -> Result<()> {
        let err_context = || "failed to move clients from closed tab".to_string();

        if self.tabs.is_empty() {
            Err::<(), _>(anyhow!(
                "No tabs left, cannot move clients: {:?} from closed tab",
                client_ids_and_mode_infos
            ))
            .with_context(err_context)
            .non_fatal();

            return Ok(());
        }
        let first_tab_index = *self
            .tabs
            .keys()
            .next()
            .context("screen contained no tabs")
            .with_context(err_context)?;
        for (client_id, client_mode_info) in client_ids_and_mode_infos {
            let client_tab_history = self.tab_history.entry(client_id).or_insert_with(Vec::new);
            if let Some(client_previous_tab) = client_tab_history.pop() {
                if let Some(client_active_tab) = self.tabs.get_mut(&client_previous_tab) {
                    self.active_tab_indices
                        .insert(client_id, client_previous_tab);
                    client_active_tab
                        .add_client(client_id, Some(client_mode_info))
                        .with_context(err_context)?;
                    continue;
                }
            }
            self.active_tab_indices.insert(client_id, first_tab_index);
            self.tabs
                .get_mut(&first_tab_index)
                .with_context(err_context)?
                .add_client(client_id, Some(client_mode_info))
                .with_context(err_context)?;
        }
        Ok(())
    }

    fn move_suppressed_panes_from_closed_tab(
        &mut self,
        suppressed_panes: SuppressedPanes,
    ) -> Result<()> {
        // TODO: this is not entirely accurate, these also sometimes contain a pane who's
        // scrollback is being edited - in this case we need to close it or to move it to the
        // appropriate tab
        let err_context = || "Failed to move suppressed panes from closed tab";
        let first_tab_index = *self
            .tabs
            .keys()
            .next()
            .context("screen contains no tabs")
            .with_context(err_context)?;
        self.tabs
            .get_mut(&first_tab_index)
            .with_context(err_context)?
            .add_suppressed_panes(suppressed_panes);
        Ok(())
    }

    fn move_clients_between_tabs(
        &mut self,
        source_tab_index: usize,
        destination_tab_index: usize,
        update_mode_infos: bool,
        clients_to_move: Option<Vec<ClientId>>,
    ) -> Result<()> {
        let err_context = || {
            format!(
                "failed to move clients from tab {source_tab_index} to tab {destination_tab_index}"
            )
        };

        // None ==> move all clients
        let drained_clients = self
            .get_indexed_tab_mut(source_tab_index)
            .map(|t| t.drain_connected_clients(clients_to_move));
        if let Some(client_mode_info_in_source_tab) = drained_clients {
            let destination_tab = self
                .get_indexed_tab_mut(destination_tab_index)
                .context("failed to get destination tab by index")
                .with_context(err_context)?;
            destination_tab
                .add_multiple_clients(client_mode_info_in_source_tab)
                .with_context(err_context)?;
            if update_mode_infos {
                destination_tab
                    .update_input_modes()
                    .with_context(err_context)?;
            }
            destination_tab.set_force_render();
            destination_tab.visible(true).with_context(err_context)?;
        }
        Ok(())
    }

    fn update_client_tab_focus(&mut self, client_id: ClientId, new_tab_index: usize) {
        match self.active_tab_indices.remove(&client_id) {
            Some(old_active_index) => {
                self.active_tab_indices.insert(client_id, new_tab_index);
                let client_tab_history = self.tab_history.entry(client_id).or_insert_with(Vec::new);
                client_tab_history.retain(|&e| e != new_tab_index);
                client_tab_history.push(old_active_index);
            },
            None => {
                self.active_tab_indices.insert(client_id, new_tab_index);
            },
        }
    }

    /// A helper function to switch to a new tab at specified position.
    fn switch_active_tab(
        &mut self,
        new_tab_pos: usize,
        should_change_pane_focus: Option<Direction>,
        update_mode_infos: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || {
            format!(
            "Failed to switch to active tab at position {new_tab_pos} for client id: {client_id:?}"
        )
        };

        if let Some(new_tab) = self.tabs.values().find(|t| t.position == new_tab_pos) {
            match self.get_active_tab(client_id) {
                Ok(current_tab) => {
                    // If new active tab is same as the current one, do nothing.
                    if current_tab.position == new_tab_pos {
                        return Ok(());
                    }

                    let current_tab_index = current_tab.index;
                    let new_tab_index = new_tab.index;
                    if self.session_is_mirrored {
                        self.move_clients_between_tabs(
                            current_tab_index,
                            new_tab_index,
                            update_mode_infos,
                            None,
                        )
                        .with_context(err_context)?;
                        let all_connected_clients: Vec<ClientId> = self
                            .connected_clients
                            .borrow()
                            .iter()
                            .map(|(c, _i)| *c)
                            .collect();
                        for client_id in all_connected_clients {
                            self.update_client_tab_focus(client_id, new_tab_index);
                            match (
                                should_change_pane_focus,
                                self.get_indexed_tab_mut(new_tab_index),
                            ) {
                                (Some(direction), Some(new_tab)) => {
                                    new_tab.focus_pane_on_edge(direction, client_id);
                                },
                                _ => {},
                            }
                        }
                    } else {
                        self.move_clients_between_tabs(
                            current_tab_index,
                            new_tab_index,
                            update_mode_infos,
                            Some(vec![client_id]),
                        )
                        .with_context(err_context)?;
                        match (
                            should_change_pane_focus,
                            self.get_indexed_tab_mut(new_tab_index),
                        ) {
                            (Some(direction), Some(new_tab)) => {
                                new_tab.focus_pane_on_edge(direction, client_id);
                            },
                            _ => {},
                        }
                        self.update_client_tab_focus(client_id, new_tab_index);
                    }

                    if let Some(current_tab) = self.get_indexed_tab_mut(current_tab_index) {
                        if current_tab.has_no_connected_clients() {
                            current_tab.visible(false).with_context(err_context)?;
                        }
                    } else {
                        Err::<(), _>(anyhow!("Tab index {:?} not found", current_tab_index))
                            .with_context(err_context)
                            .non_fatal();
                    }

                    self.log_and_report_session_state()
                        .with_context(err_context)?;
                    return self.render(None).with_context(err_context);
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    /// A helper function to switch to a new tab with specified name. Return true if tab [name] has
    /// been created, else false.
    fn switch_active_tab_name(&mut self, name: String, client_id: ClientId) -> Result<bool> {
        match self.tabs.values().find(|t| t.name == name) {
            Some(new_tab) => {
                self.switch_active_tab(new_tab.position, None, true, client_id)?;
                Ok(true)
            },
            None => Ok(false),
        }
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the next tab.
    pub fn switch_tab_next(
        &mut self,
        should_change_pane_focus: Option<Direction>,
        update_mode_infos: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to switch to next tab for client {client_id}");

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };

        if let Some(client_id) = client_id {
            match self.get_active_tab(client_id) {
                Ok(active_tab) => {
                    let active_tab_pos = active_tab.position;
                    let new_tab_pos = (active_tab_pos + 1) % self.tabs.len();
                    return self.switch_active_tab(
                        new_tab_pos,
                        should_change_pane_focus,
                        update_mode_infos,
                        client_id,
                    );
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    /// Sets this [`Screen`]'s active [`Tab`] to the previous tab.
    pub fn switch_tab_prev(
        &mut self,
        should_change_pane_focus: Option<Direction>,
        update_mode_infos: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to switch to previous tab for client {client_id}");

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };

        if let Some(client_id) = client_id {
            match self.get_active_tab(client_id) {
                Ok(active_tab) => {
                    let active_tab_pos = active_tab.position;
                    let new_tab_pos = if active_tab_pos == 0 {
                        self.tabs.len() - 1
                    } else {
                        active_tab_pos - 1
                    };

                    return self.switch_active_tab(
                        new_tab_pos,
                        should_change_pane_focus,
                        update_mode_infos,
                        client_id,
                    );
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            }
        }
        Ok(())
    }

    pub fn go_to_tab(&mut self, tab_index: usize, client_id: ClientId) -> Result<()> {
        self.switch_active_tab(tab_index.saturating_sub(1), None, true, client_id)
    }

    pub fn go_to_tab_name(&mut self, name: String, client_id: ClientId) -> Result<bool> {
        self.switch_active_tab_name(name, client_id)
    }

    fn close_tab_at_index(&mut self, tab_index: usize) -> Result<()> {
        let err_context = || format!("failed to close tab at index {tab_index:?}");

        let mut tab_to_close = self.tabs.remove(&tab_index).with_context(err_context)?;
        let mut pane_ids = tab_to_close.get_all_pane_ids();

        // here we extract the suppressed panes (these are background panes that don't care which
        // tab they are in, and in the future we should probably make them global to screen rather
        // than to each tab) and move them to another tab if there is one
        let suppressed_panes = tab_to_close.extract_suppressed_panes();
        for suppressed_pane_id in suppressed_panes.keys() {
            pane_ids.retain(|p| p != suppressed_pane_id);
        }

        let _ = self.bus.senders.send_to_plugin(PluginInstruction::Update(
            pane_ids
                .iter()
                .copied()
                .map(|p_id| (None, None, Event::PaneClosed(p_id.into())))
                .collect(),
        ));

        // below we don't check the result of sending the CloseTab instruction to the pty thread
        // because this might be happening when the app is closing, at which point the pty thread
        // has already closed and this would result in an error
        self.bus
            .senders
            .send_to_pty(PtyInstruction::CloseTab(pane_ids))
            .with_context(err_context)?;
        if self.tabs.is_empty() {
            self.active_tab_indices.clear();
            self.bus
                .senders
                .send_to_server(ServerInstruction::Render(None))
                .with_context(err_context)
        } else {
            let client_mode_infos_in_closed_tab = tab_to_close.drain_connected_clients(None);
            self.move_clients_from_closed_tab(client_mode_infos_in_closed_tab)
                .with_context(err_context)?;
            self.move_suppressed_panes_from_closed_tab(suppressed_panes)
                .with_context(err_context)?;
            let visible_tab_indices: HashSet<usize> =
                self.active_tab_indices.values().copied().collect();
            for t in self.tabs.values_mut() {
                if visible_tab_indices.contains(&t.index) {
                    t.set_force_render();
                    t.visible(true).with_context(err_context)?;
                }
                if t.position > tab_to_close.position {
                    t.position -= 1;
                }
            }
            self.log_and_report_session_state()
                .with_context(err_context)?;
            self.render(None).with_context(err_context)
        }
    }

    // Closes the client_id's focused tab
    pub fn close_tab(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to close tab for client {client_id:?}");

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };

        match client_id {
            Some(client_id) => {
                let active_tab_index = *self
                    .active_tab_indices
                    .get(&client_id)
                    .with_context(err_context)?;
                self.close_tab_at_index(active_tab_index)
                    .with_context(err_context)
            },
            None => Ok(()),
        }
    }

    pub fn resize_to_screen(&mut self, new_screen_size: Size) -> Result<()> {
        let err_context = || format!("failed to resize to screen size: {new_screen_size:#?}");

        if self.size != new_screen_size {
            self.size = new_screen_size;
            for tab in self.tabs.values_mut() {
                tab.resize_whole_tab(new_screen_size)
                    .with_context(err_context)?;
                tab.set_force_render();
            }
            self.log_and_report_session_state()
                .with_context(err_context)?;
            self.render(None).with_context(err_context)
        } else {
            Ok(())
        }
    }

    pub fn update_pixel_dimensions(&mut self, pixel_dimensions: PixelDimensions) {
        self.pixel_dimensions.merge(pixel_dimensions);
        if let Some(character_cell_size) = self.pixel_dimensions.character_cell_size {
            *self.character_cell_size.borrow_mut() = Some(character_cell_size);
        } else if let Some(text_area_size) = self.pixel_dimensions.text_area_size {
            let character_cell_size_height = text_area_size.height / self.size.rows;
            let character_cell_size_width = text_area_size.width / self.size.cols;
            let character_cell_size = SizeInPixels {
                height: character_cell_size_height,
                width: character_cell_size_width,
            };
            *self.character_cell_size.borrow_mut() = Some(character_cell_size);
        }
    }

    pub fn update_terminal_background_color(&mut self, background_color_instruction: String) {
        if let Some(AnsiCode::RgbCode((r, g, b))) =
            xparse_color(background_color_instruction.as_bytes())
        {
            let bg_palette_color = PaletteColor::Rgb((r, g, b));
            self.terminal_emulator_colors.borrow_mut().bg = bg_palette_color;
        }
    }

    pub fn update_terminal_foreground_color(&mut self, foreground_color_instruction: String) {
        if let Some(AnsiCode::RgbCode((r, g, b))) =
            xparse_color(foreground_color_instruction.as_bytes())
        {
            let fg_palette_color = PaletteColor::Rgb((r, g, b));
            self.terminal_emulator_colors.borrow_mut().fg = fg_palette_color;
        }
    }

    pub fn update_terminal_color_registers(&mut self, color_registers: Vec<(usize, String)>) {
        let mut terminal_emulator_color_codes = self.terminal_emulator_color_codes.borrow_mut();
        for (color_register, color_sequence) in color_registers {
            terminal_emulator_color_codes.insert(color_register, color_sequence);
        }
    }

    pub fn render(&mut self, plugin_render_assets: Option<Vec<PluginRenderAsset>>) -> Result<()> {
        // here we schedule the RenderToClients background job which debounces renders every 10ms
        // rather than actually rendering
        //
        // when this job decides to render, it sends back the ScreenInstruction::RenderToClients
        // message, triggering our render_to_clients method which does the actual rendering

        let _ = self
            .bus
            .senders
            .send_to_background_jobs(BackgroundJob::RenderToClients);
        if let Some(plugin_render_assets) = plugin_render_assets {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::UnblockCliPipes(plugin_render_assets))
                .context("failed to unblock input pipe");
        }
        Ok(())
    }

    pub fn render_to_clients(&mut self) -> Result<()> {
        // this method does the actual rendering and is triggered by a debounced BackgroundJob (see
        // the render method for more details)
        let err_context = "failed to render screen";

        // Separate rendering for regular clients and watchers
        let has_regular_clients = self
            .connected_clients
            .borrow()
            .keys()
            .any(|id| !self.watcher_clients.contains_key(id));
        let has_watchers = !self.watcher_clients.is_empty(); // No change needed

        // Track whether non-watcher output was dirty for conditional watcher rendering
        let non_watcher_output_was_dirty;

        // === PHASE 1: Render for regular clients ===
        if has_regular_clients {
            let mut output = Output::new(
                self.sixel_image_store.clone(),
                self.character_cell_size.clone(),
                self.styled_underlines,
            );

            let mut tabs_to_close = vec![];
            for (tab_index, tab) in &mut self.tabs {
                if tab.has_selectable_tiled_panes() {
                    // Pass None for normal client rendering
                    tab.render(&mut output, None).context(err_context)?;
                } else if !tab.is_pending() {
                    tabs_to_close.push(*tab_index);
                }
            }
            for tab_index in tabs_to_close {
                self.close_tab_at_index(tab_index)
                    .context(err_context)
                    .non_fatal();
            }

            let pane_render_report = output.drain_pane_render_report();
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::PaneRenderReport(pane_render_report));

            non_watcher_output_was_dirty = output.is_dirty();
            if non_watcher_output_was_dirty {
                let serialized_output = output.serialize().context(err_context)?;
                let _ = self
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::Render(Some(serialized_output)))
                    .context(err_context);
            }
        } else {
            // No regular clients, output is not dirty
            non_watcher_output_was_dirty = false;
        }

        // === PHASE 2: Render for watchers ===
        if has_watchers {
            if let Some(followed_client_id) = self.followed_client_id {
                // Create fresh output for watchers
                let mut watcher_output = Output::new(
                    self.sixel_image_store.clone(),
                    self.character_cell_size.clone(),
                    self.styled_underlines,
                );

                let focused_tab_index_of_followed_client_id = *self
                    .active_tab_indices
                    .get(&followed_client_id)
                    .unwrap_or(&0);

                if let Some(tab) = self
                    .tabs
                    .get_mut(&focused_tab_index_of_followed_client_id)
                    .as_mut()
                {
                    // Only force render if:
                    // 1. Non-watcher output was dirty, OR
                    // 2. Any watcher needs a forced render (first render or after resize), OR
                    // 3. No non-watcher clients are connected
                    let any_watcher_needs_force_render = self
                        .watcher_clients
                        .values()
                        .any(|state| state.should_force_render());
                    let should_force_render = non_watcher_output_was_dirty
                        || any_watcher_needs_force_render
                        || !has_regular_clients;

                    if should_force_render {
                        tab.set_force_render();
                    }
                    tab.render(&mut watcher_output, Some(followed_client_id))
                        .context(err_context)?;
                }

                // Send the rendered output to all watcher clients
                if watcher_output.is_dirty() {
                    let mut watcher_render_output: HashMap<ClientId, String> = HashMap::new();

                    // For each watcher, clone the output and serialize with size constraints
                    for (watcher_id, watcher_state) in &self.watcher_clients {
                        let mut watcher_specific_output = watcher_output.clone();

                        // Serialize this watcher's output with size constraints (cropping and padding handled inside)
                        let mut serialized_output = watcher_specific_output
                            .serialize_with_size(Some(watcher_state.size()), Some(self.size))
                            .context(err_context)?;

                        // Get the output for the followed client and map it to this watcher
                        if let Some(followed_output) = serialized_output.remove(&followed_client_id)
                        {
                            watcher_render_output.insert(*watcher_id, followed_output);
                        }
                    }

                    // Send to server for delivery to watcher clients
                    if !watcher_render_output.is_empty() {
                        let _ = self
                            .bus
                            .senders
                            .send_to_server(ServerInstruction::Render(Some(watcher_render_output)))
                            .context(err_context);
                    }

                    // Clear force render flag for all watchers after successful render
                    for watcher_state in self.watcher_clients.values_mut() {
                        watcher_state.clear_force_render();
                    }
                }
            }
        }

        Ok(())
    }

    /// Returns a mutable reference to this [`Screen`]'s tabs.
    pub fn get_tabs_mut(&mut self) -> &mut BTreeMap<usize, Tab> {
        &mut self.tabs
    }

    pub fn get_tabs(&self) -> &BTreeMap<usize, Tab> {
        &self.tabs
    }

    /// Returns an immutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab(&self, client_id: ClientId) -> Result<&Tab> {
        match self.active_tab_indices.get(&client_id) {
            Some(tab) => self
                .tabs
                .get(tab)
                .ok_or_else(|| anyhow!("active tab {} does not exist", tab)),
            None => Err(anyhow!("active tab not found for client {:?}", client_id)),
        }
    }

    pub fn get_client_input_mode(&self, client_id: ClientId) -> Option<InputMode> {
        self.get_active_tab(client_id)
            .ok()
            .and_then(|tab| tab.get_client_input_mode(client_id))
    }

    pub fn get_first_client_id(&self) -> Option<ClientId> {
        self.active_tab_indices.keys().next().copied()
    }

    /// Returns an immutable reference to this [`Screen`]'s previous active [`Tab`].
    /// Consumes the last entry in tab history.
    pub fn get_previous_tab(&mut self, client_id: ClientId) -> Result<Option<&Tab>> {
        Ok(
            match self
                .tab_history
                .get_mut(&client_id)
                .with_context(|| {
                    format!("failed to retrieve tab history for client {client_id:?}")
                })?
                .pop()
            {
                Some(tab) => self.tabs.get(&tab),
                None => None,
            },
        )
    }

    /// Returns a mutable reference to this [`Screen`]'s active [`Tab`].
    pub fn get_active_tab_mut(&mut self, client_id: ClientId) -> Result<&mut Tab> {
        match self.active_tab_indices.get(&client_id) {
            Some(tab) => self
                .tabs
                .get_mut(tab)
                .ok_or_else(|| anyhow!("active tab {} does not exist", tab)),
            None => Err(anyhow!("active tab not found for client {:?}", client_id)),
        }
    }

    /// Returns a mutable reference to this [`Screen`]'s active [`Overlays`].
    pub fn get_active_overlays_mut(&mut self) -> &mut Vec<Overlay> {
        &mut self.overlay.overlay_stack
    }

    /// Returns a mutable reference to this [`Screen`]'s indexed [`Tab`].
    pub fn get_indexed_tab_mut(&mut self, tab_index: usize) -> Option<&mut Tab> {
        self.get_tabs_mut().get_mut(&tab_index)
    }

    /// Creates a new [`Tab`] in this [`Screen`]
    pub fn new_tab(
        &mut self,
        tab_index: usize,
        swap_layouts: (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>),
        tab_name: Option<String>,
        client_id: Option<ClientId>,
    ) -> Result<()> {
        let err_context = || format!("failed to create new tab for client {client_id:?}",);

        let client_id = client_id.map(|client_id| {
            if self.get_active_tab(client_id).is_ok() {
                client_id
            } else if let Some(first_client_id) = self.get_first_client_id() {
                first_client_id
            } else {
                client_id
            }
        });

        let tab_name = tab_name.unwrap_or_else(|| String::new());

        let position = self.tabs.len();
        let mut tab = Tab::new(
            tab_index,
            position,
            tab_name,
            self.size,
            self.character_cell_size.clone(),
            self.stacked_resize.clone(),
            self.sixel_image_store.clone(),
            self.bus
                .os_input
                .as_ref()
                .with_context(err_context)?
                .clone(),
            self.bus.senders.clone(),
            self.max_panes,
            self.style.clone(),
            self.default_mode_info.clone(),
            self.draw_pane_frames,
            self.auto_layout,
            self.connected_clients.clone(),
            self.session_is_mirrored,
            client_id,
            self.copy_options.clone(),
            self.terminal_emulator_colors.clone(),
            self.terminal_emulator_color_codes.clone(),
            swap_layouts,
            self.default_shell.clone(),
            self.debug,
            self.arrow_fonts,
            self.styled_underlines,
            self.explicitly_disable_kitty_keyboard_protocol,
            self.default_editor.clone(),
            self.web_clients_allowed,
            self.web_sharing,
            self.current_pane_group.clone(),
            self.currently_marking_pane_group.clone(),
            self.advanced_mouse_actions,
            self.web_server_ip,
            self.web_server_port,
        );
        for (client_id, mode_info) in &self.mode_info {
            tab.change_mode_info(mode_info.clone(), *client_id);
        }
        self.tabs.insert(tab_index, tab);
        Ok(())
    }
    pub fn apply_layout(
        &mut self,
        layout: TiledPaneLayout,
        floating_panes_layout: Vec<FloatingPaneLayout>,
        new_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_floating_terminal_ids: Vec<(u32, HoldForCommand)>,
        new_plugin_ids: HashMap<RunPluginOrAlias, Vec<u32>>,
        tab_index: usize,
        should_change_client_focus: bool,
        client_id_and_is_web_client: (ClientId, bool),
    ) -> Result<()> {
        if self.tabs.get(&tab_index).is_none() {
            // TODO: we should prevent this situation with a UI - eg. cannot close tabs with a
            // pending state
            log::error!("Tab with index {tab_index} not found. Cannot apply layout!");
            return Ok(());
        }
        let (client_id, mut is_web_client) = client_id_and_is_web_client;
        let client_id = if self.get_active_tab(client_id).is_ok() {
            if let Some(connected_client_is_web_client) =
                self.connected_clients.borrow().get(&client_id)
            {
                is_web_client = *connected_client_is_web_client;
            }
            client_id
        } else if let Some(first_client_id) = self.get_first_client_id() {
            if let Some(first_client_is_web_client) =
                self.connected_clients.borrow().get(&first_client_id)
            {
                is_web_client = *first_client_is_web_client;
            }
            first_client_id
        } else {
            client_id
        };
        let err_context = || format!("failed to apply layout for tab {tab_index:?}",);

        // move the relevant clients out of the current tab and place them in the new one
        let drained_clients = if should_change_client_focus {
            if self.session_is_mirrored {
                let client_mode_infos_in_source_tab = if let Ok(active_tab) =
                    self.get_active_tab_mut(client_id)
                {
                    let client_mode_infos_in_source_tab = active_tab.drain_connected_clients(None);
                    if active_tab.has_no_connected_clients() {
                        active_tab
                            .visible(false)
                            .with_context(err_context)
                            .non_fatal();
                    }
                    Some(client_mode_infos_in_source_tab)
                } else {
                    None
                };
                let all_connected_clients: Vec<ClientId> = self
                    .connected_clients
                    .borrow()
                    .iter()
                    .map(|(c, _i)| *c)
                    .collect();
                for client_id in all_connected_clients {
                    self.update_client_tab_focus(client_id, tab_index);
                }
                client_mode_infos_in_source_tab
            } else if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
                let client_mode_info_in_source_tab =
                    active_tab.drain_connected_clients(Some(vec![client_id]));
                if active_tab.has_no_connected_clients() {
                    active_tab
                        .visible(false)
                        .with_context(err_context)
                        .non_fatal();
                }
                self.update_client_tab_focus(client_id, tab_index);
                Some(client_mode_info_in_source_tab)
            } else {
                None
            }
        } else {
            None
        };

        // apply the layout to the new tab
        self.tabs
            .get_mut(&tab_index)
            .context("couldn't find tab with index {tab_index}")
            .and_then(|tab| {
                tab.apply_layout(
                    layout,
                    floating_panes_layout,
                    new_terminal_ids,
                    new_floating_terminal_ids,
                    new_plugin_ids,
                    client_id,
                )?;
                tab.update_input_modes()?;

                if let Some(drained_clients) = drained_clients {
                    tab.visible(true)?;
                    tab.add_multiple_clients(drained_clients)?;
                }
                tab.resize_whole_tab(self.size).with_context(err_context)?;
                tab.set_force_render();
                Ok(())
            })
            .with_context(err_context)?;

        if !self.active_tab_indices.contains_key(&client_id) {
            // this means this is a new client and we need to add it to our state properly
            self.add_client(client_id, is_web_client)
                .with_context(err_context)?;
        }

        self.log_and_report_session_state()
            .and_then(|_| self.render(None))
            .with_context(err_context)
    }

    pub fn add_client(&mut self, client_id: ClientId, is_web_client: bool) -> Result<()> {
        let err_context = |tab_index| {
            format!("failed to attach client {client_id} to tab with index {tab_index}")
        };

        // Set followed_client_id to the first regular client if not already set
        if self.followed_client_id.is_none() && !self.watcher_clients.contains_key(&client_id) {
            self.followed_client_id = Some(client_id);
        }

        let mut tab_history = vec![];
        if let Some((_first_client, first_tab_history)) = self.tab_history.iter().next() {
            tab_history = first_tab_history.clone();
        }

        let tab_index = if let Some((_first_client, first_active_tab_index)) =
            self.active_tab_indices.iter().next()
        {
            *first_active_tab_index
        } else if self.tabs.contains_key(&0) {
            0
        } else if let Some(tab_index) = self.tabs.keys().next() {
            tab_index.to_owned()
        } else {
            bail!("Can't find a valid tab to attach client to!");
        };

        self.active_tab_indices.insert(client_id, tab_index);
        self.connected_clients
            .borrow_mut()
            .insert(client_id, is_web_client);
        self.tab_history.insert(client_id, tab_history);
        self.tabs
            .get_mut(&tab_index)
            .with_context(|| err_context(tab_index))?
            .add_client(client_id, None)
            .with_context(|| err_context(tab_index))
    }

    pub fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to remove client {client_id}");

        // If the followed client disconnected, find the next regular client
        if Some(client_id) == self.followed_client_id {
            // Try to find another regular (non-watcher) client
            self.followed_client_id = self
                .connected_clients
                .borrow()
                .keys()
                .copied()
                .find(|id| !self.watcher_clients.contains_key(id) && id != &client_id);

            // If no regular client remains but we have watchers, keep the old followed_client_id
            // for terminal rendering (plugins will use their last state)
            if self.followed_client_id.is_none() && !self.watcher_clients.is_empty() {
                self.followed_client_id = Some(client_id); // Keep the disconnected client's ID
            }
        }

        for (_, tab) in self.tabs.iter_mut() {
            tab.remove_client(client_id);
            if tab.has_no_connected_clients() {
                tab.visible(false).with_context(err_context)?;
            }
        }
        if self.active_tab_indices.contains_key(&client_id) {
            self.active_tab_indices.remove(&client_id);
        }
        if self.tab_history.contains_key(&client_id) {
            self.tab_history.remove(&client_id);
        }
        self.connected_clients.borrow_mut().remove(&client_id);
        self.log_and_report_session_state()
            .with_context(err_context)
    }

    pub fn add_watcher_client(&mut self, client_id: ClientId) -> Result<()> {
        // Initialize with a default size - will be updated when we receive the actual size
        let default_size = Size { rows: 24, cols: 80 }; // Reasonable default
        self.watcher_clients
            .insert(client_id, WatcherState::new(default_size));

        // Force a full render for the new watcher
        // This ensures they get complete state, not just delta
        self.render(None)?;

        Ok(())
    }

    pub fn remove_watcher_client(&mut self, client_id: ClientId) {
        self.watcher_clients.remove(&client_id);
    }

    pub fn set_followed_client(&mut self, client_id: ClientId) -> Result<()> {
        self.followed_client_id = Some(client_id);
        // Trigger re-render with new followed client
        self.render(None)?;
        Ok(())
    }

    pub fn set_watcher_size(&mut self, client_id: ClientId, size: Size) {
        // Update size if this client is a watcher
        if let Some(watcher_state) = self.watcher_clients.get_mut(&client_id) {
            watcher_state.set_size(size);
            watcher_state.set_force_render();
        }
    }

    // Optional: getter for debugging/monitoring
    pub fn get_watcher_size(&self, client_id: &ClientId) -> Option<Size> {
        self.watcher_clients
            .get(client_id)
            .map(|state| state.size())
    }

    // Optional: get all watcher sizes
    pub fn get_all_watcher_sizes(&self) -> &HashMap<ClientId, WatcherState> {
        &self.watcher_clients
    }

    pub fn generate_and_report_tab_state(&mut self) -> Result<Vec<TabInfo>> {
        let mut plugin_updates = vec![];
        let mut tab_infos_for_screen_state = BTreeMap::new();
        for tab in self.tabs.values() {
            let all_focused_clients: Vec<ClientId> = self
                .active_tab_indices
                .iter()
                .filter(|(_c_id, tab_position)| **tab_position == tab.index)
                .map(|(c_id, _)| c_id)
                .copied()
                .collect();
            let (active_swap_layout_name, is_swap_layout_dirty) = tab.swap_layout_info();
            let tab_viewport = tab.get_viewport();
            let tab_display_area = tab.get_display_area();
            let selectable_tiled_panes_count = tab.get_selectable_tiled_panes_count();
            let selectable_floating_panes_count = tab.get_selectable_floating_panes_count();
            let tab_info_for_screen = TabInfo {
                position: tab.position,
                name: tab.name.clone(),
                active: self.active_tab_indices.values().any(|i| i == &tab.index),
                panes_to_hide: tab.panes_to_hide_count(),
                is_fullscreen_active: tab.is_fullscreen_active(),
                is_sync_panes_active: tab.is_sync_panes_active(),
                are_floating_panes_visible: tab.are_floating_panes_visible(),
                other_focused_clients: all_focused_clients,
                active_swap_layout_name,
                is_swap_layout_dirty,
                viewport_rows: tab_viewport.rows,
                viewport_columns: tab_viewport.cols,
                display_area_rows: tab_display_area.rows,
                display_area_columns: tab_display_area.cols,
                selectable_tiled_panes_count,
                selectable_floating_panes_count,
            };
            tab_infos_for_screen_state.insert(tab.position, tab_info_for_screen);
        }
        for (client_id, active_tab_index) in self.active_tab_indices.iter() {
            let mut plugin_tab_updates = vec![];
            for tab in self.tabs.values() {
                let other_focused_clients: Vec<ClientId> = if self.session_is_mirrored {
                    vec![]
                } else {
                    self.active_tab_indices
                        .iter()
                        .filter(|(c_id, tab_position)| {
                            **tab_position == tab.index && *c_id != client_id
                        })
                        .map(|(c_id, _)| c_id)
                        .copied()
                        .collect()
                };
                let (active_swap_layout_name, is_swap_layout_dirty) = tab.swap_layout_info();
                let tab_viewport = tab.get_viewport();
                let tab_display_area = tab.get_display_area();
                let selectable_tiled_panes_count = tab.get_selectable_tiled_panes_count();
                let selectable_floating_panes_count = tab.get_selectable_floating_panes_count();
                let tab_info_for_plugins = TabInfo {
                    position: tab.position,
                    name: tab.name.clone(),
                    active: *active_tab_index == tab.index,
                    panes_to_hide: tab.panes_to_hide_count(),
                    is_fullscreen_active: tab.is_fullscreen_active(),
                    is_sync_panes_active: tab.is_sync_panes_active(),
                    are_floating_panes_visible: tab.are_floating_panes_visible(),
                    other_focused_clients,
                    active_swap_layout_name,
                    is_swap_layout_dirty,
                    viewport_rows: tab_viewport.rows,
                    viewport_columns: tab_viewport.cols,
                    display_area_rows: tab_display_area.rows,
                    display_area_columns: tab_display_area.cols,
                    selectable_tiled_panes_count,
                    selectable_floating_panes_count,
                };
                plugin_tab_updates.push(tab_info_for_plugins);
            }
            plugin_updates.push((None, Some(*client_id), Event::TabUpdate(plugin_tab_updates)));
        }
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::Update(plugin_updates))
            .context("failed to update tabs")?;
        Ok(tab_infos_for_screen_state.values().cloned().collect())
    }
    fn generate_and_report_pane_state(&mut self) -> Result<PaneManifest> {
        let mut pane_manifest = PaneManifest::default();
        for tab in self.tabs.values() {
            pane_manifest.panes.insert(tab.position, tab.pane_infos());
        }
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                None,
                None,
                Event::PaneUpdate(pane_manifest.clone()),
            )]))
            .context("failed to update tabs")?;

        Ok(pane_manifest)
    }
    fn log_and_report_session_state(&mut self) -> Result<()> {
        let err_context = || format!("Failed to log and report session state");
        // generate own session info
        let pane_manifest = self.generate_and_report_pane_state()?;
        let tab_infos = self.generate_and_report_tab_state()?;
        // in the context of unit/integration tests, we don't need to list available layouts
        // because this is mostly about HD access - it does however throw off the timing in the
        // tests and causes them to flake, which is why we skip it here
        #[cfg(not(test))]
        let available_layouts =
            Layout::list_available_layouts(self.layout_dir.clone(), &self.default_layout_name);
        #[cfg(test)]
        let available_layouts = vec![];
        let session_info = SessionInfo {
            name: self.session_name.clone(),
            tabs: tab_infos,
            panes: pane_manifest,
            connected_clients: self.active_tab_indices.keys().len(),
            is_current_session: true,
            available_layouts,
            web_clients_allowed: self.web_sharing.web_clients_allowed(),
            web_client_count: self
                .connected_clients
                .borrow()
                .iter()
                .filter(|(_client_id, is_web_client)| **is_web_client)
                .count(),
            plugins: Default::default(), // these are filled in by the wasm thread
            tab_history: self.tab_history.clone(),
        };
        self.bus
            .senders
            .send_to_background_jobs(BackgroundJob::ReportSessionInfo(
                self.session_name.to_owned(),
                session_info,
            ))
            .with_context(err_context)?;

        self.bus
            .senders
            .send_to_background_jobs(BackgroundJob::ReadAllSessionInfosOnMachine)
            .with_context(err_context)?;

        // TODO: consider moving this elsewhere
        self.bus
            .senders
            .send_to_background_jobs(BackgroundJob::QueryZellijWebServerStatus)
            .with_context(err_context)?;
        Ok(())
    }
    fn dump_layout_to_hd(&mut self) -> Result<()> {
        let err_context = || format!("Failed to log and report session state");
        let session_layout_metadata = self.get_layout_metadata(Some(self.default_shell.clone()));
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::LogLayoutToHd(session_layout_metadata))
            .with_context(err_context)?;

        Ok(())
    }
    pub fn update_session_infos(
        &mut self,
        new_session_infos: BTreeMap<String, SessionInfo>,
        resurrectable_sessions: BTreeMap<String, Duration>,
    ) -> Result<()> {
        self.session_infos_on_machine = new_session_infos;
        self.resurrectable_sessions = resurrectable_sessions;
        self.bus
            .senders
            .send_to_plugin(PluginInstruction::Update(vec![(
                None,
                None,
                Event::SessionUpdate(
                    self.session_infos_on_machine.values().cloned().collect(),
                    self.resurrectable_sessions
                        .iter()
                        .map(|(n, c)| (n.clone(), c.clone()))
                        .collect(),
                ),
            )]))
            .context("failed to update session info")?;
        Ok(())
    }

    pub fn update_active_tab_name(&mut self, buf: Vec<u8>, client_id: ClientId) -> Result<()> {
        let err_context =
            || format!("failed to update active tabs name for client id: {client_id:?}");

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };

        match client_id {
            Some(client_id) => {
                let s = str::from_utf8(&buf)
                    .with_context(|| format!("failed to construct tab name from buf: {buf:?}"))
                    .with_context(err_context)?;
                match self.get_active_tab_mut(client_id) {
                    Ok(active_tab) => {
                        match s {
                            "\0" => {
                                active_tab.name = String::new();
                            },
                            "\u{007F}" | "\u{0008}" => {
                                // delete and backspace keys
                                active_tab.name.pop();
                            },
                            c => {
                                active_tab
                                    .name
                                    .push_str(&clean_string_from_control_and_linebreak(c));
                            },
                        }
                        self.log_and_report_session_state()
                            .with_context(err_context)
                    },
                    Err(err) => {
                        Err::<(), _>(err).with_context(err_context).non_fatal();
                        Ok(())
                    },
                }
            },
            None => Ok(()),
        }
    }
    pub fn undo_active_rename_tab(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to undo active tab rename for client {}", client_id);

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };
        match client_id {
            Some(client_id) => {
                match self.get_active_tab_mut(client_id) {
                    Ok(active_tab) => {
                        if active_tab.name != active_tab.prev_name {
                            active_tab.name = active_tab.prev_name.clone();
                            self.log_and_report_session_state()
                                .context("failed to undo renaming of active tab")?;
                        }
                    },
                    Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
                };
                Ok(())
            },
            None => Ok(()),
        }
    }

    pub fn move_active_tab_to_left(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || "Failed to move active tab left";
        if self.tabs.len() < 2 {
            debug!("cannot move tab to left: only one tab exists");
            return Ok(());
        }
        let Some(client_id) = self.client_id(client_id) else {
            return Ok(());
        };

        match self.get_active_tab(client_id) {
            Ok(active_tab) => {
                let active_tab_pos = active_tab.position;
                let left_tab_pos = if active_tab_pos == 0 {
                    self.tabs.len() - 1
                } else {
                    active_tab_pos - 1
                };

                self.switch_tabs(active_tab_pos, left_tab_pos, client_id);
                self.log_and_report_session_state()
                    .context("failed to move tab to left")?;
            },
            Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
        }
        Ok(())
    }

    fn client_id(&mut self, client_id: ClientId) -> Option<u16> {
        if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        }
    }

    fn switch_tabs(&mut self, active_tab_pos: usize, other_tab_pos: usize, client_id: u16) {
        let Some(active_tab_idx) = self
            .tabs
            .values()
            .find(|t| t.position == active_tab_pos)
            .map(|t| t.index)
        else {
            log::error!("Failed to find active tab at position: {}", active_tab_pos);
            return;
        };
        let Some(other_tab_idx) = self
            .tabs
            .values()
            .find(|t| t.position == other_tab_pos)
            .map(|t| t.index)
        else {
            log::error!(
                "Failed to find tab to switch to at position: {}",
                other_tab_pos
            );
            return;
        };

        if !self.tabs.contains_key(&active_tab_idx) || !self.tabs.contains_key(&other_tab_idx) {
            warn!(
                "failed to switch tabs: index {} or {} not found in {:?}",
                active_tab_idx,
                other_tab_idx,
                self.tabs.keys()
            );
            return;
        }

        // NOTE: Can `expect` here, because we checked that the keys exist above
        let mut active_tab = self
            .tabs
            .remove(&active_tab_idx)
            .expect("active tab not found");
        let mut other_tab = self
            .tabs
            .remove(&other_tab_idx)
            .expect("other tab not found");

        std::mem::swap(&mut active_tab.index, &mut other_tab.index);
        std::mem::swap(&mut active_tab.position, &mut other_tab.position);

        // now, `active_tab.index` is changed, so we need to update it
        self.active_tab_indices.insert(client_id, active_tab.index);

        self.tabs.insert(active_tab.index, active_tab);
        self.tabs.insert(other_tab.index, other_tab);
    }

    pub fn move_active_tab_to_right(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || "Failed to move active tab right ";
        if self.tabs.len() < 2 {
            debug!("cannot move tab to right: only one tab exists");
            return Ok(());
        }
        let Some(client_id) = self.client_id(client_id) else {
            return Ok(());
        };

        match self.get_active_tab(client_id) {
            Ok(active_tab) => {
                let active_tab_pos = active_tab.position;
                let right_tab_pos = (active_tab_pos + 1) % self.tabs.len();

                self.switch_tabs(active_tab_pos, right_tab_pos, client_id);
                self.log_and_report_session_state()
                    .context("failed to move tab to the right")?;
            },
            Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
        }
        Ok(())
    }

    pub fn change_mode(&mut self, mut mode_info: ModeInfo, client_id: ClientId) -> Result<()> {
        if mode_info.session_name.as_ref() != Some(&self.session_name) {
            mode_info.session_name = Some(self.session_name.clone());
        }

        let previous_mode_info = self
            .mode_info
            .get(&client_id)
            .unwrap_or(&self.default_mode_info);
        let previous_mode = previous_mode_info.mode;
        mode_info.style = previous_mode_info.style.clone();
        mode_info.capabilities = previous_mode_info.capabilities;

        let err_context = || {
            format!(
                "failed to change from mode '{:?}' to mode '{:?}' for client {client_id}",
                previous_mode, mode_info.mode
            )
        };

        // If we leave the Search-related modes, we need to clear all previous searches
        let search_related_modes = [InputMode::EnterSearch, InputMode::Search, InputMode::Scroll];
        if search_related_modes.contains(&previous_mode)
            && !search_related_modes.contains(&mode_info.mode)
        {
            active_tab!(self, client_id, |tab: &mut Tab| tab.clear_search(client_id));
        }

        if previous_mode == InputMode::Scroll
            && (mode_info.mode == InputMode::Normal || mode_info.mode == InputMode::Locked)
        {
            if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
                active_tab
                    .clear_active_terminal_scroll(client_id)
                    .with_context(err_context)?;
            }
        }

        if mode_info.mode == InputMode::RenameTab {
            if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
                active_tab.prev_name = active_tab.name.clone();
            }
        }

        if mode_info.mode == InputMode::RenamePane {
            if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
                if let Some(active_pane) =
                    active_tab.get_active_pane_or_floating_pane_mut(client_id)
                {
                    active_pane.store_pane_name();
                }
            }
        }

        self.style = mode_info.style.clone();
        self.mode_info.insert(client_id, mode_info.clone());
        for tab in self.tabs.values_mut() {
            tab.change_mode_info(mode_info.clone(), client_id);
            tab.mark_active_pane_for_rerender(client_id);
            tab.update_input_modes()?;
        }
        Ok(())
    }
    pub fn change_mode_for_all_clients(&mut self, mode_info: ModeInfo) -> Result<()> {
        let err_context = || {
            format!(
                "failed to change input mode to {:?} for all clients",
                mode_info.mode
            )
        };

        let connected_client_ids: Vec<ClientId> = self.active_tab_indices.keys().copied().collect();
        for client_id in connected_client_ids {
            self.change_mode(mode_info.clone(), client_id)
                .with_context(err_context)?;
        }
        Ok(())
    }
    pub fn move_focus_left_or_previous_tab(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || {
            format!(
                "failed to move focus left or to previous tab for client {}",
                client_id
            )
        };

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };
        if let Some(client_id) = client_id {
            match self.get_active_tab_mut(client_id) {
                Ok(active_tab) => {
                    active_tab
                        .move_focus_left(client_id)
                        .and_then(|success| {
                            if !success {
                                self.switch_tab_prev(Some(Direction::Left), true, client_id)
                                    .context("failed to move focus to previous tab")
                            } else {
                                Ok(())
                            }
                        })
                        .with_context(err_context)?;
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            };
        }
        self.log_and_report_session_state()
            .with_context(err_context)?;
        Ok(())
    }
    pub fn move_focus_right_or_next_tab(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || {
            format!(
                "failed to move focus right or to next tab for client {}",
                client_id
            )
        };

        let client_id = if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        };

        if let Some(client_id) = client_id {
            match self.get_active_tab_mut(client_id) {
                Ok(active_tab) => {
                    active_tab
                        .move_focus_right(client_id)
                        .and_then(|success| {
                            if !success {
                                self.switch_tab_next(Some(Direction::Right), true, client_id)
                                    .context("failed to move focus to next tab")
                            } else {
                                Ok(())
                            }
                        })
                        .with_context(err_context)?;
                },
                Err(err) => Err::<(), _>(err).with_context(err_context).non_fatal(),
            };
        }
        self.log_and_report_session_state()
            .with_context(err_context)?;
        Ok(())
    }
    pub fn toggle_tab(&mut self, client_id: ClientId) -> Result<()> {
        let tab = self
            .get_previous_tab(client_id)
            .context("failed to toggle tabs")?;
        if let Some(t) = tab {
            let position = t.position;
            self.go_to_tab(position + 1, client_id)
                .context("failed to toggle tabs")?;
        };

        self.log_and_report_session_state()
            .context("failed to toggle tabs")?;
        self.render(None)
    }

    pub fn focus_plugin_pane(
        &mut self,
        run_plugin: &RunPluginOrAlias,
        should_float: bool,
        move_to_focused_tab: bool,
        client_id: ClientId,
    ) -> Result<bool> {
        // true => found and focused, false => not
        let err_context = || format!("failed to focus_plugin_pane");
        let mut tab_index_and_plugin_pane_id = None;
        let mut plugin_pane_to_move_to_active_tab = None;
        let focused_tab_index = *self.active_tab_indices.get(&client_id).unwrap_or(&0);
        let all_tabs = self.get_tabs_mut();
        for (tab_index, tab) in all_tabs.iter_mut() {
            if let Some(plugin_pane_id) = tab.find_plugin(&run_plugin) {
                tab_index_and_plugin_pane_id = Some((*tab_index, plugin_pane_id));
                if move_to_focused_tab && focused_tab_index != *tab_index {
                    plugin_pane_to_move_to_active_tab = tab.extract_pane(plugin_pane_id, true);
                }

                break;
            }
        }
        if let Some(plugin_pane_to_move_to_active_tab) = plugin_pane_to_move_to_active_tab.take() {
            let pane_id = plugin_pane_to_move_to_active_tab.pid();
            let new_active_tab = self.get_active_tab_mut(client_id)?;

            if should_float {
                new_active_tab.show_floating_panes();
                new_active_tab.add_floating_pane(
                    plugin_pane_to_move_to_active_tab,
                    pane_id,
                    None,
                    true,
                )?;
            } else {
                new_active_tab.hide_floating_panes();
                new_active_tab.add_tiled_pane(
                    plugin_pane_to_move_to_active_tab,
                    pane_id,
                    Some(client_id),
                )?;
            }
            return Ok(true);
        }
        match tab_index_and_plugin_pane_id {
            Some((tab_index, plugin_pane_id)) => {
                self.go_to_tab(tab_index + 1, client_id)?;
                self.tabs
                    .get_mut(&tab_index)
                    .with_context(err_context)?
                    .focus_pane_with_id(plugin_pane_id, should_float, client_id)
                    .context("failed to focus plugin pane")?;
                self.log_and_report_session_state()
                    .with_context(err_context)?;
                Ok(true)
            },
            None => Ok(false),
        }
    }

    pub fn focus_pane_with_id(
        &mut self,
        pane_id: PaneId,
        should_float_if_hidden: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to focus_plugin_pane");
        let tab_index = self
            .tabs
            .iter()
            .find(|(_tab_index, tab)| tab.has_pane_with_pid(&pane_id))
            .map(|(_tab_index, tab)| tab.position);
        match tab_index {
            Some(tab_index) => {
                self.go_to_tab(tab_index + 1, client_id)?;
                self.tabs
                    .iter_mut()
                    .find(|(_, t)| t.position == tab_index)
                    .map(|(_, t)| t.focus_pane_with_id(pane_id, should_float_if_hidden, client_id))
                    .with_context(err_context)
                    .non_fatal();
            },
            None => {
                log::error!("Could not find pane with id: {:?}", pane_id);
            },
        };
        Ok(())
    }
    pub fn rerun_command_pane_with_id(
        &mut self,
        terminal_pane_id: u32,
        completion_tx: Option<NotificationEnd>,
    ) {
        let mut found = false;
        for tab in self.tabs.values_mut() {
            if tab.has_pane_with_pid(&PaneId::Terminal(terminal_pane_id)) {
                tab.rerun_terminal_pane_with_id(terminal_pane_id, completion_tx);
                found = true;
                break;
            }
        }
        if !found {
            log::error!(
                "Failed to find terminal pane with id: {} to run",
                terminal_pane_id
            );
        }
    }
    pub fn resize_pane_with_id(&mut self, resize: ResizeStrategy, pane_id: PaneId) {
        let mut found = false;
        for tab in self.tabs.values_mut() {
            if tab.has_pane_with_pid(&pane_id) {
                tab.resize_pane_with_id(resize, pane_id).non_fatal();
                found = true;
                break;
            }
        }
        if !found {
            log::error!("Failed to find pane with id: {:?} to resize", pane_id);
        }
    }
    pub fn break_pane(
        &mut self,
        default_shell: Option<TerminalAction>,
        default_layout: Box<Layout>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || "failed break pane out of tab".to_string();
        let active_tab = self.get_active_tab_mut(client_id)?;
        if active_tab.get_selectable_tiled_panes_count() > 1
            || active_tab.get_visible_selectable_floating_panes_count() > 0
        {
            let active_pane_id = active_tab
                .get_active_pane_id(client_id)
                .with_context(err_context)?;
            let pane_to_break_is_floating = active_tab.are_floating_panes_visible();
            let active_pane = active_tab
                .extract_pane(active_pane_id, false)
                .with_context(err_context)?;
            let active_pane_run_instruction = active_pane.invoked_with().clone();
            let tab_index = self.get_new_tab_index();
            let swap_layouts = (
                default_layout.swap_tiled_layouts.clone(),
                default_layout.swap_floating_layouts.clone(),
            );
            self.new_tab(tab_index, swap_layouts, None, Some(client_id))?;
            let tab = self.tabs.get_mut(&tab_index).with_context(err_context)?;
            let (mut tiled_panes_layout, mut floating_panes_layout) = default_layout.new_tab();
            if pane_to_break_is_floating {
                tab.show_floating_panes();
                tab.add_floating_pane(active_pane, active_pane_id, None, true)?;
                if let Some(already_running_layout) = floating_panes_layout
                    .iter_mut()
                    .find(|i| i.run == active_pane_run_instruction)
                {
                    already_running_layout.already_running = true;
                }
            } else {
                tab.add_tiled_pane(active_pane, active_pane_id, Some(client_id))?;
                tiled_panes_layout.ignore_run_instruction(active_pane_run_instruction.clone());
            }
            let should_change_focus_to_new_tab = true;
            let is_web_client = self
                .connected_clients
                .borrow()
                .get(&client_id)
                .copied()
                .unwrap_or(false);
            self.bus.senders.send_to_plugin(PluginInstruction::NewTab(
                None,
                default_shell,
                Some(tiled_panes_layout),
                floating_panes_layout,
                tab_index,
                should_change_focus_to_new_tab,
                (client_id, is_web_client),
                None,
            ))?;
        } else {
            let active_pane_id = active_tab
                .get_active_pane_id(client_id)
                .with_context(err_context)?;
            self.bus
                .senders
                .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                    vec![active_pane_id],
                    "Cannot break single pane out!".into(),
                ))
                .with_context(err_context)?;
        }
        Ok(())
    }
    pub fn break_multiple_panes_to_new_tab(
        &mut self,
        pane_ids: Vec<PaneId>,
        default_shell: Option<TerminalAction>,
        should_change_focus_to_new_tab: bool,
        new_tab_name: Option<String>,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || "failed break multiple panes to a new tab".to_string();

        let all_tabs = self.get_tabs_mut();
        let mut extracted_panes = vec![];
        for pane_id in pane_ids {
            for tab in all_tabs.values_mut() {
                // here we pass None instead of the client_id we have because we do not need to
                // necessarily trigger a relayout for this tab
                if let Some(pane) = tab.extract_pane(pane_id, true).take() {
                    extracted_panes.push(pane);
                    break;
                }
            }
        }

        let (mut tiled_panes_layout, floating_panes_layout) = self.default_layout.new_tab();
        let tab_index = self.get_new_tab_index();
        let swap_layouts = (
            self.default_layout.swap_tiled_layouts.clone(),
            self.default_layout.swap_floating_layouts.clone(),
        );
        if should_change_focus_to_new_tab {
            self.new_tab(tab_index, swap_layouts, None, Some(client_id))?;
        } else {
            self.new_tab(tab_index, swap_layouts, None, None)?;
        }
        let tab = self.tabs.get_mut(&tab_index).with_context(err_context)?;
        if let Some(new_tab_name) = new_tab_name {
            tab.name = new_tab_name.clone();
        }
        for pane in extracted_panes {
            let run_instruction = pane.invoked_with().clone();
            let pane_id = pane.pid();
            // here we pass None instead of the ClientId, because we do not want this pane to be
            // necessarily focused
            tab.add_tiled_pane(pane, pane_id, None)?;
            tiled_panes_layout.ignore_run_instruction(run_instruction.clone());
        }
        let is_web_client = self
            .connected_clients
            .borrow()
            .get(&client_id)
            .copied()
            .unwrap_or(false);
        self.bus.senders.send_to_plugin(PluginInstruction::NewTab(
            None,
            default_shell,
            Some(tiled_panes_layout),
            floating_panes_layout,
            tab_index,
            should_change_focus_to_new_tab,
            (client_id, is_web_client),
            None,
        ))?;
        Ok(())
    }
    pub fn break_pane_to_new_tab(
        &mut self,
        direction: Direction,
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || "failed break pane out of tab".to_string();
        if self.tabs.len() > 1 {
            let (active_pane_id, active_pane, pane_to_break_is_floating) = {
                let active_tab = self.get_active_tab_mut(client_id)?;
                let active_pane_id = active_tab
                    .get_active_pane_id(client_id)
                    .with_context(err_context)?;
                let pane_to_break_is_floating = active_tab.are_floating_panes_visible();
                let active_pane = active_tab
                    .extract_pane(active_pane_id, false)
                    .with_context(err_context)?;
                (active_pane_id, active_pane, pane_to_break_is_floating)
            };
            let update_mode_infos = false;
            match direction {
                Direction::Right | Direction::Down => {
                    self.switch_tab_next(None, update_mode_infos, client_id)?;
                },
                Direction::Left | Direction::Up => {
                    self.switch_tab_prev(None, update_mode_infos, client_id)?;
                },
            };
            let new_active_tab = self.get_active_tab_mut(client_id)?;

            if pane_to_break_is_floating {
                new_active_tab.show_floating_panes();
                new_active_tab.add_floating_pane(active_pane, active_pane_id, None, true)?;
            } else {
                new_active_tab.hide_floating_panes();
                new_active_tab.add_tiled_pane(active_pane, active_pane_id, Some(client_id))?;
            }

            self.log_and_report_session_state()?;
        } else {
            let active_pane_id = {
                let active_tab = self.get_active_tab_mut(client_id)?;
                active_tab
                    .get_active_pane_id(client_id)
                    .with_context(err_context)?
            };
            self.bus
                .senders
                .send_to_background_jobs(BackgroundJob::DisplayPaneError(
                    vec![active_pane_id],
                    "No other tabs to add pane to!".into(),
                ))
                .with_context(err_context)?;
        }
        self.render(None)?;
        Ok(())
    }
    pub fn break_multiple_panes_to_tab_with_index(
        &mut self,
        pane_ids: Vec<PaneId>,
        tab_index: usize,
        should_change_focus_to_new_tab: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let all_tabs = self.get_tabs_mut();
        let has_tab_with_index = all_tabs
            .values()
            .find(|t| t.position == tab_index)
            .is_some();
        if !has_tab_with_index {
            log::error!("Cannot find tab with index: {tab_index}");
            return Ok(());
        }
        let mut extracted_panes = vec![];
        for pane_id in pane_ids {
            for tab in all_tabs.values_mut() {
                if tab.position == tab_index {
                    continue;
                }
                // here we pass None instead of the client_id we have because we do not need to
                // necessarily trigger a relayout for this tab
                let pane_was_floating = tab.pane_id_is_floating(&pane_id);
                if let Some(pane) = tab.extract_pane(pane_id, true).take() {
                    extracted_panes.push((pane_was_floating, pane));
                    break;
                }
            }
        }

        if should_change_focus_to_new_tab {
            self.go_to_tab(tab_index + 1, client_id)?;
        }
        if extracted_panes.is_empty() {
            // nothing to do here...
            return Ok(());
        }
        if let Some(new_active_tab) = self.get_indexed_tab_mut(tab_index) {
            for (pane_was_floating, pane) in extracted_panes {
                let pane_id = pane.pid();
                if pane_was_floating {
                    let floating_pane_coordinates = FloatingPaneCoordinates {
                        x: Some(SplitSize::Fixed(pane.x())),
                        y: Some(SplitSize::Fixed(pane.y())),
                        width: Some(SplitSize::Fixed(pane.cols())),
                        height: Some(SplitSize::Fixed(pane.rows())),
                        pinned: Some(pane.current_geom().is_pinned),
                    };
                    new_active_tab.add_floating_pane(
                        pane,
                        pane_id,
                        Some(floating_pane_coordinates),
                        false,
                    )?;
                } else {
                    // here we pass None instead of the ClientId, because we do not want this pane to be
                    // necessarily focused
                    new_active_tab.add_tiled_pane(pane, pane_id, None)?;
                }
            }
        } else {
            log::error!("Could not find tab with index: {:?}", tab_index);
        }
        self.log_and_report_session_state()?;
        Ok(())
    }
    pub fn replace_pane(
        &mut self,
        new_pane_id: PaneId,
        hold_for_command: HoldForCommand,
        run: Option<Run>,
        pane_title: Option<InitialTitle>,
        close_replaced_pane: bool,
        client_id_tab_index_or_pane_id: ClientTabIndexOrPaneId,
    ) -> Result<()> {
        let suppress_pane = |tab: &mut Tab, pane_id: PaneId, new_pane_id: PaneId| {
            let _ = tab.suppress_pane_and_replace_with_pid(
                pane_id,
                new_pane_id,
                close_replaced_pane,
                run,
                None,
            );
            if let Some(pane_title) = pane_title {
                let _ = tab.rename_pane(pane_title.as_bytes().to_vec(), new_pane_id);
            }
            if let Some(hold_for_command) = hold_for_command {
                let is_first_run = true;
                tab.hold_pane(new_pane_id, None, is_first_run, hold_for_command)
            }
        };
        match client_id_tab_index_or_pane_id {
            ClientTabIndexOrPaneId::ClientId(client_id) => {
                active_tab!(self, client_id, |tab: &mut Tab| {
                    match tab.get_active_pane_id(client_id) {
                        Some(pane_id) => {
                            suppress_pane(tab, pane_id, new_pane_id);
                        },
                        None => {
                            log::error!(
                                "Failed to find active pane for client id: {:?}",
                                client_id
                            );
                        },
                    }
                });
            },
            ClientTabIndexOrPaneId::PaneId(pane_id) => {
                let tab_index = self
                    .tabs
                    .iter()
                    .find(|(_tab_index, tab)| tab.has_pane_with_pid(&pane_id))
                    .map(|(_tab_index, tab)| tab.position);
                match tab_index {
                    Some(tab_index) => {
                        if let Some(tab) =
                            self.tabs.iter_mut().find(|(_, t)| t.position == tab_index)
                        {
                            suppress_pane(tab.1, pane_id, new_pane_id);
                        }
                    },
                    None => {
                        log::error!("Could not find pane with id: {:?}", pane_id);
                    },
                };
            },
            ClientTabIndexOrPaneId::TabIndex(_tab_index) => {
                log::error!("Cannot replace pane with tab index");
            },
        }
        Ok(())
    }
    pub fn replace_pane_with_existing_pane(
        &mut self,
        pane_id_to_replace: PaneId,
        pane_id_of_existing_pane: PaneId,
    ) {
        let Some(tab_index_of_pane_id_to_replace) = self
            .tabs
            .iter()
            .find(|(_tab_index, tab)| tab.has_pane_with_pid(&pane_id_to_replace))
            .map(|(_tab_index, tab)| tab.position)
        else {
            log::error!("Could not find tab");
            return;
        };
        let Some(tab_index_of_existing_pane) = self
            .tabs
            .iter()
            .find(|(_tab_index, tab)| tab.has_pane_with_pid(&pane_id_of_existing_pane))
            .map(|(_tab_index, tab)| tab.position)
        else {
            log::error!("Could not find tab");
            return;
        };
        let Some(extracted_pane_from_other_tab) = self
            .tabs
            .iter_mut()
            .find(|(_, t)| t.position == tab_index_of_existing_pane)
            .and_then(|(_, t)| t.extract_pane(pane_id_of_existing_pane, false))
        else {
            log::error!("Failed to find pane");
            return;
        };
        if let Some(tab) = self
            .tabs
            .iter_mut()
            .find(|(_, t)| t.position == tab_index_of_pane_id_to_replace)
        {
            tab.1.close_pane_and_replace_with_other_pane(
                pane_id_to_replace,
                extracted_pane_from_other_tab,
                None,
            );
        }
        let _ = self.log_and_report_session_state();
    }
    pub fn reconfigure(
        &mut self,
        new_keybinds: Keybinds,
        new_default_mode: InputMode,
        theme: Styling,
        simplified_ui: bool,
        default_shell: Option<PathBuf>,
        pane_frames: bool,
        copy_command: Option<String>,
        copy_to_clipboard: Option<Clipboard>,
        copy_on_select: bool,
        auto_layout: bool,
        rounded_corners: bool,
        hide_session_name: bool,
        tabline_prefix_text: Option<String>,
        stacked_resize: bool,
        default_editor: Option<PathBuf>,
        advanced_mouse_actions: bool,
        client_id: ClientId,
    ) -> Result<()> {
        let should_support_arrow_fonts = !simplified_ui;

        // global configuration
        self.default_mode_info.update_theme(theme);
        self.default_mode_info
            .update_rounded_corners(rounded_corners);
        self.default_shell = default_shell.clone().unwrap_or_else(|| get_default_shell());
        self.default_editor = default_editor.clone().or_else(|| get_default_editor());
        self.auto_layout = auto_layout;
        self.copy_options.command = copy_command.clone();
        self.copy_options.copy_on_select = copy_on_select;
        self.draw_pane_frames = pane_frames;
        self.advanced_mouse_actions = advanced_mouse_actions;
        self.default_mode_info
            .update_arrow_fonts(should_support_arrow_fonts);
        self.default_mode_info
            .update_hide_session_name(hide_session_name);
        self.default_mode_info
            .update_tabline_prefix_text(tabline_prefix_text.clone());
        {
            *self.stacked_resize.borrow_mut() = stacked_resize;
        }
        if let Some(copy_to_clipboard) = copy_to_clipboard {
            self.copy_options.clipboard = copy_to_clipboard;
        }
        for tab in self.tabs.values_mut() {
            tab.update_theme(theme);
            tab.update_rounded_corners(rounded_corners);
            tab.update_default_shell(default_shell.clone());
            tab.update_default_editor(self.default_editor.clone());
            tab.update_auto_layout(auto_layout);
            tab.update_copy_options(&self.copy_options);
            tab.set_pane_frames(pane_frames);
            tab.update_arrow_fonts(should_support_arrow_fonts);
            tab.update_advanced_mouse_actions(advanced_mouse_actions);
        }

        // client specific configuration
        if self.connected_clients_contains(&client_id) {
            let mode_info = self
                .mode_info
                .entry(client_id)
                .or_insert_with(|| self.default_mode_info.clone());
            mode_info.update_keybinds(new_keybinds);
            mode_info.update_default_mode(new_default_mode);
            mode_info.update_theme(theme);
            mode_info.update_arrow_fonts(should_support_arrow_fonts);
            mode_info.update_hide_session_name(hide_session_name);
            mode_info.update_tabline_prefix_text(tabline_prefix_text.clone());
            for tab in self.tabs.values_mut() {
                tab.change_mode_info(mode_info.clone(), client_id);
                tab.mark_active_pane_for_rerender(client_id);
            }
        }

        // this needs to be done separately at the end because it applies some of the above changes
        // and propagates them to plugins
        for tab in self.tabs.values_mut() {
            tab.update_input_modes()?;
        }
        Ok(())
    }
    pub fn toggle_pane_pinned(&mut self, client_id: ClientId) {
        active_tab_and_connected_client_id!(
            self,
            client_id,
            |tab: &mut Tab, client_id: ClientId| {
                tab.toggle_pane_pinned(client_id);
            }
        );
    }
    pub fn set_floating_pane_pinned(&mut self, pane_id: PaneId, should_be_pinned: bool) {
        let mut found = false;
        for tab in self.tabs.values_mut() {
            if tab.has_pane_with_pid(&pane_id) {
                tab.set_floating_pane_pinned(pane_id, should_be_pinned);
                found = true;
                break;
            }
        }
        if !found {
            log::error!(
                "Failed to find pane with id: {:?} to set as pinned",
                pane_id
            );
        }
    }
    pub fn stack_panes(&mut self, mut pane_ids_to_stack: Vec<PaneId>) -> Option<PaneId> {
        // if successful, returns the pane id of the last pane in the stack
        if pane_ids_to_stack.is_empty() {
            log::error!("Got an empty list of pane_ids to stack");
            return None;
        }
        let stack_size = pane_ids_to_stack.len();
        let root_pane_id = pane_ids_to_stack.remove(0);
        let last_pane_id = pane_ids_to_stack.last();
        let Some(root_tab_id) = self
            .tabs
            .iter()
            .find_map(|(tab_id, tab)| {
                if tab.has_pane_with_pid(&root_pane_id) {
                    Some(tab_id)
                } else {
                    None
                }
            })
            .copied()
        else {
            log::error!("Failed to find tab for root_pane_id: {:?}", root_pane_id);
            return None;
        };
        let root_pane_id_is_floating = self
            .tabs
            .get(&root_tab_id)
            .map(|t| t.pane_id_is_floating(&root_pane_id))
            .unwrap_or(false);

        if root_pane_id_is_floating {
            self.tabs.get_mut(&root_tab_id).map(|tab| {
                let _ = tab.toggle_pane_embed_or_floating_for_pane_id(root_pane_id, None);
            });
        }

        let mut panes_to_stack = vec![];
        let target_tab_has_room_for_stack = self
            .tabs
            .get_mut(&root_tab_id)
            .map(|t| t.has_room_for_stack(root_pane_id, stack_size))
            .unwrap_or(false);
        if !target_tab_has_room_for_stack {
            log::error!("No room for stack with root pane id: {:?}", root_pane_id);
            return None;
        }

        for (tab_id, tab) in self.tabs.iter_mut() {
            if tab_id == &root_tab_id {
                // we do this before we extract panes so that the extraction won't trigger a
                // relayout according to the next swapped tiled pane
                tab.set_tiled_panes_damaged();
            }
            for pane_id in &pane_ids_to_stack {
                if tab.has_pane_with_pid(&pane_id) {
                    match tab.extract_pane(*pane_id, false) {
                        Some(pane) => {
                            panes_to_stack.push(pane);
                        },
                        None => {
                            log::error!("Failed to extract pane: {:?}", pane_id);
                        },
                    }
                }
            }
        }
        self.tabs
            .get_mut(&root_tab_id)
            .map(|t| t.stack_panes(root_pane_id, panes_to_stack));
        return last_pane_id.copied();
    }
    pub fn change_floating_panes_coordinates(
        &mut self,
        pane_ids_and_coordinates: Vec<(PaneId, FloatingPaneCoordinates)>,
    ) {
        for (pane_id, coordinates) in pane_ids_and_coordinates {
            for (_tab_id, tab) in self.tabs.iter_mut() {
                if tab.has_pane_with_pid(&pane_id) {
                    tab.change_floating_pane_coordinates(&pane_id, coordinates)
                        .non_fatal();
                    break;
                }
            }
        }
    }
    pub fn handle_mouse_event(&mut self, event: MouseEvent, client_id: ClientId) {
        match self
            .get_active_tab_mut(client_id)
            .and_then(|tab| tab.handle_mouse_event(&event, client_id))
        {
            Ok(mouse_effect) => {
                if let Some(pane_id) = mouse_effect.group_toggle {
                    if self.advanced_mouse_actions {
                        self.toggle_pane_id_in_group(pane_id, &client_id);
                    }
                }
                if let Some(pane_id) = mouse_effect.group_add {
                    if self.advanced_mouse_actions {
                        self.add_pane_id_to_group(pane_id, &client_id);
                    }
                }
                if mouse_effect.ungroup {
                    if self.advanced_mouse_actions {
                        self.clear_pane_group(&client_id);
                    }
                }
                if mouse_effect.state_changed {
                    let _ = self.log_and_report_session_state();
                }
                if !mouse_effect.leave_clipboard_message {
                    let _ = self
                        .bus
                        .senders
                        .send_to_plugin(PluginInstruction::Update(vec![(
                            None,
                            Some(client_id),
                            Event::InputReceived,
                        )]));
                }
                self.render(None).non_fatal();
            },
            Err(e) => {
                log::error!("Failed to process MouseEvent: {}", e);
            },
        }
    }
    pub fn toggle_pane_in_group(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = "Can't add pane to group";
        let active_tab = self
            .get_active_tab(client_id)
            .with_context(|| err_context)?;
        let active_pane_id = active_tab
            .get_active_pane_id(client_id)
            .with_context(|| err_context)?;
        self.toggle_pane_id_in_group(active_pane_id, &client_id);
        let _ = self.log_and_report_session_state();
        Ok(())
    }
    pub fn toggle_group_marking(&mut self, client_id: ClientId) -> Result<()> {
        let (was_marking_before, marking_pane_group_now) = {
            let mut currently_marking_pane_group = self.currently_marking_pane_group.borrow_mut();
            let previous_value = currently_marking_pane_group
                .remove(&client_id)
                .unwrap_or(false);
            let new_value = !previous_value;
            if new_value {
                currently_marking_pane_group.insert(client_id, true);
            }
            (previous_value, new_value)
        };
        if marking_pane_group_now {
            let active_pane_id = self.get_active_pane_id(&client_id);
            if let Some(active_pane_id) = active_pane_id {
                self.add_pane_id_to_group(active_pane_id, &client_id);
            }
        }
        let value_changed = was_marking_before != marking_pane_group_now;
        if value_changed {
            for tab in self.tabs.values_mut() {
                tab.update_input_modes()?;
            }
            let _ = self.log_and_report_session_state();
        }
        Ok(())
    }
    fn get_layout_metadata(&self, default_shell: Option<PathBuf>) -> SessionLayoutMetadata {
        let mut session_layout_metadata = SessionLayoutMetadata::new(self.default_layout.clone());
        if let Some(default_shell) = default_shell {
            session_layout_metadata.update_default_shell(default_shell);
        }
        let first_client_id = self.get_first_client_id();
        let active_tab_index =
            first_client_id.and_then(|client_id| self.active_tab_indices.get(&client_id));

        for (tab_index, tab) in self.tabs.iter() {
            let tab_is_focused = active_tab_index == Some(&tab_index);
            let hide_floating_panes = !tab.are_floating_panes_visible();
            let mut suppressed_panes = HashMap::new();
            for (triggering_pane_id, p) in tab.get_suppressed_panes() {
                suppressed_panes.insert(*triggering_pane_id, p);
            }

            let all_connected_clients: Vec<ClientId> = self
                .connected_clients
                .borrow()
                .iter()
                .map(|(c, _i)| *c)
                .filter(|c| self.active_tab_indices.get(&c) == Some(&tab_index))
                .collect();

            let mut active_pane_ids: HashMap<ClientId, Option<PaneId>> = HashMap::new();
            for connected_client_id in &all_connected_clients {
                active_pane_ids.insert(
                    *connected_client_id,
                    tab.get_active_pane_id(*connected_client_id),
                );
            }

            let tiled_panes: Vec<PaneLayoutMetadata> = tab
                .get_tiled_panes()
                .map(|(pane_id, p)| {
                    // here we look to see if this pane triggers any suppressed pane,
                    // and if so we take that suppressed pane - we do this because this
                    // is currently only the case the scrollback editing panes, and
                    // when dumping the layout we want the "real" pane and not the
                    // editor pane
                    match suppressed_panes.remove(pane_id) {
                        Some((is_scrollback_editor, suppressed_pane)) if *is_scrollback_editor => {
                            (suppressed_pane.pid(), suppressed_pane)
                        },
                        _ => (*pane_id, p),
                    }
                })
                .map(|(pane_id, p)| {
                    let focused_clients: Vec<ClientId> = active_pane_ids
                        .iter()
                        .filter_map(|(c_id, p_id)| {
                            p_id.and_then(|p_id| if p_id == pane_id { Some(*c_id) } else { None })
                        })
                        .collect();
                    PaneLayoutMetadata::new(
                        pane_id,
                        p.position_and_size(),
                        p.borderless(),
                        p.invoked_with().clone(),
                        p.custom_title(),
                        !focused_clients.is_empty(),
                        if self.serialize_pane_viewport {
                            p.serialize(self.scrollback_lines_to_serialize)
                        } else {
                            None
                        },
                        focused_clients,
                    )
                })
                .collect();
            let floating_panes: Vec<PaneLayoutMetadata> = tab
                .get_floating_panes()
                .map(|(pane_id, p)| {
                    // here we look to see if this pane triggers any suppressed pane,
                    // and if so we take that suppressed pane - we do this because this
                    // is currently only the case the scrollback editing panes, and
                    // when dumping the layout we want the "real" pane and not the
                    // editor pane
                    match suppressed_panes.remove(pane_id) {
                        Some((is_scrollback_editor, suppressed_pane)) if *is_scrollback_editor => {
                            (suppressed_pane.pid(), suppressed_pane)
                        },
                        _ => (*pane_id, p),
                    }
                })
                .map(|(pane_id, p)| {
                    let focused_clients: Vec<ClientId> = active_pane_ids
                        .iter()
                        .filter_map(|(c_id, p_id)| {
                            p_id.and_then(|p_id| if p_id == pane_id { Some(*c_id) } else { None })
                        })
                        .collect();
                    PaneLayoutMetadata::new(
                        pane_id,
                        p.position_and_size(),
                        false, // floating panes are never borderless
                        p.invoked_with().clone(),
                        p.custom_title(),
                        !focused_clients.is_empty(),
                        if self.serialize_pane_viewport {
                            p.serialize(self.scrollback_lines_to_serialize)
                        } else {
                            None
                        },
                        focused_clients,
                    )
                })
                .collect();
            session_layout_metadata.add_tab(
                tab.name.clone(),
                tab_is_focused,
                hide_floating_panes,
                tiled_panes,
                floating_panes,
            );
        }
        session_layout_metadata
    }
    fn update_plugin_loading_stage(
        &mut self,
        pid: u32,
        loading_indication: LoadingIndication,
    ) -> bool {
        let all_tabs = self.get_tabs_mut();
        let mut found_plugin = false;
        for tab in all_tabs.values_mut() {
            if tab.has_plugin(pid) {
                found_plugin = true;
                tab.update_plugin_loading_stage(pid, loading_indication);
                break;
            }
        }
        found_plugin
    }
    fn connected_clients_contains(&self, client_id: &ClientId) -> bool {
        self.connected_clients.borrow().contains_key(client_id)
    }
    fn get_client_pane_group(&self, client_id: &ClientId) -> HashSet<PaneId> {
        self.current_pane_group
            .borrow()
            .get_client_pane_group(client_id)
    }
    fn clear_pane_group(&mut self, client_id: &ClientId) {
        self.current_pane_group
            .borrow_mut()
            .clear_pane_group(client_id);
        self.currently_marking_pane_group
            .borrow_mut()
            .remove(client_id);
    }
    fn toggle_pane_id_in_group(&mut self, pane_id: PaneId, client_id: &ClientId) {
        {
            let mut pane_groups = self.current_pane_group.borrow_mut();
            pane_groups.toggle_pane_id_in_group(pane_id, self.size, client_id);
        }
        self.retain_only_existing_panes_in_pane_groups();
    }
    fn add_pane_id_to_group(&mut self, pane_id: PaneId, client_id: &ClientId) {
        {
            let mut pane_groups = self.current_pane_group.borrow_mut();
            pane_groups.add_pane_id_to_group(pane_id, self.size, client_id);
        }
        self.retain_only_existing_panes_in_pane_groups();
    }
    fn add_active_pane_to_group_if_marking(&mut self, client_id: &ClientId) {
        {
            if self
                .currently_marking_pane_group
                .borrow()
                .get(client_id)
                .copied()
                .unwrap_or(false)
            {
                let active_pane_id = self.get_active_pane_id(&client_id);
                if let Some(active_pane_id) = active_pane_id {
                    self.add_pane_id_to_group(active_pane_id, &client_id);
                }
            }
        }
        self.retain_only_existing_panes_in_pane_groups();
    }
    fn get_active_pane_id(&self, client_id: &ClientId) -> Option<PaneId> {
        let active_tab = self.get_active_tab(*client_id).ok()?;
        active_tab.get_active_pane_id(*client_id)
    }

    fn group_and_ungroup_panes(
        &mut self,
        pane_ids_to_group: Vec<PaneId>,
        pane_ids_to_ungroup: Vec<PaneId>,
        for_all_clients: bool,
        client_id: ClientId,
    ) {
        if for_all_clients {
            {
                let mut current_pane_group = self.current_pane_group.borrow_mut();
                current_pane_group.group_and_ungroup_panes_for_all_clients(
                    pane_ids_to_group,
                    pane_ids_to_ungroup,
                    self.size,
                );
            }
        } else {
            {
                let mut current_pane_group = self.current_pane_group.borrow_mut();
                current_pane_group.group_and_ungroup_panes(
                    pane_ids_to_group,
                    pane_ids_to_ungroup,
                    self.size,
                    &client_id,
                );
            }
        }
        self.retain_only_existing_panes_in_pane_groups();
        let _ = self.log_and_report_session_state();
    }
    fn retain_only_existing_panes_in_pane_groups(&mut self) {
        let clients_with_empty_group = {
            let mut clients_with_empty_group = vec![];
            let mut current_pane_group = { self.current_pane_group.borrow().clone_inner() };
            for (client_id, panes_in_group) in current_pane_group.iter_mut() {
                let all_tabs = self.get_tabs();
                panes_in_group.retain(|p_id| {
                    let mut found = false;
                    for tab in all_tabs.values() {
                        if tab.has_pane_with_pid(&p_id) {
                            found = true;
                            break;
                        }
                    }
                    found
                });
                if panes_in_group.is_empty() {
                    clients_with_empty_group.push(*client_id)
                }
            }
            self.current_pane_group
                .borrow_mut()
                .override_groups_with(current_pane_group);
            clients_with_empty_group
        };
        for client_id in &clients_with_empty_group {
            self.currently_marking_pane_group
                .borrow_mut()
                .remove(client_id);
        }
        if !clients_with_empty_group.is_empty() {
            let all_tabs = self.get_tabs_mut();
            for tab in all_tabs.values_mut() {
                let _ = tab.update_input_modes();
            }
        }
    }
}

#[cfg(not(test))]
fn get_default_editor() -> Option<PathBuf> {
    std::env::var("EDITOR")
        .or_else(|_| std::env::var("VISUAL"))
        .map(|e| PathBuf::from(e))
        .ok()
}

#[cfg(test)]
fn get_default_editor() -> Option<PathBuf> {
    None
}

// The box is here in order to make the
// NewClient enum smaller
#[allow(clippy::boxed_local)]
pub(crate) fn screen_thread_main(
    bus: Bus<ScreenInstruction>,
    max_panes: Option<usize>,
    client_attributes: ClientAttributes,
    config: Config,
    debug: bool,
    default_layout: Box<Layout>,
) -> Result<()> {
    let config_options = config.options;
    let arrow_fonts = !config_options.simplified_ui.unwrap_or_default();
    let draw_pane_frames = config_options.pane_frames.unwrap_or(true);
    let auto_layout = config_options.auto_layout.unwrap_or(true);
    let session_serialization = config_options.session_serialization.unwrap_or(true);
    let serialize_pane_viewport = config_options.serialize_pane_viewport.unwrap_or(false);
    let scrollback_lines_to_serialize = config_options.scrollback_lines_to_serialize;
    let session_is_mirrored = config_options.mirror_session.unwrap_or(false);
    let layout_dir = config_options.layout_dir;
    #[cfg(test)]
    let default_shell = config_options
        .default_shell
        .clone()
        .unwrap_or(PathBuf::from("/bin/sh"));
    #[cfg(not(test))]
    let default_shell = config_options
        .default_shell
        .clone()
        .unwrap_or_else(|| get_default_shell());
    let default_editor = config_options
        .scrollback_editor
        .clone()
        .or_else(|| get_default_editor());
    let default_layout_name = config_options
        .default_layout
        .map(|l| format!("{}", l.display()));
    let copy_options = CopyOptions::new(
        config_options.copy_command,
        config_options.copy_clipboard.unwrap_or_default(),
        config_options.copy_on_select.unwrap_or(true),
    );
    let web_server_ip = config_options
        .web_server_ip
        .unwrap_or_else(|| IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    let web_server_port = config_options.web_server_port.unwrap_or(8082);
    let styled_underlines = config_options.styled_underlines.unwrap_or(true);
    let explicitly_disable_kitty_keyboard_protocol = config_options
        .support_kitty_keyboard_protocol
        .map(|e| !e) // this is due to the config options wording, if
        // "support_kitty_keyboard_protocol" is true,
        // explicitly_disable_kitty_keyboard_protocol is false and vice versa
        .unwrap_or(false); // by default, we try to support this if the terminal supports it and
                           // the program running inside a pane requests it
    let stacked_resize = config_options.stacked_resize.unwrap_or(true);
    let web_clients_allowed = config_options
        .web_sharing
        .map(|s| s.web_clients_allowed())
        .unwrap_or(false);
    let web_sharing = config_options.web_sharing.unwrap_or_else(Default::default);
    let advanced_mouse_actions = config_options.advanced_mouse_actions.unwrap_or(true);

    let thread_senders = bus.senders.clone();
    let mut screen = Screen::new(
        bus,
        &client_attributes,
        max_panes,
        get_mode_info(
            config_options.default_mode.unwrap_or_default(),
            &client_attributes,
            PluginCapabilities {
                //  ¯\_(ツ)_/¯
                arrow_fonts: !arrow_fonts,
            },
            &config.keybinds,
            config_options.default_mode,
        ),
        draw_pane_frames,
        auto_layout,
        session_is_mirrored,
        copy_options,
        debug,
        default_layout,
        default_layout_name,
        default_shell,
        session_serialization,
        serialize_pane_viewport,
        scrollback_lines_to_serialize,
        styled_underlines,
        arrow_fonts,
        layout_dir,
        explicitly_disable_kitty_keyboard_protocol,
        stacked_resize,
        default_editor,
        web_clients_allowed,
        web_sharing,
        advanced_mouse_actions,
        web_server_ip,
        web_server_port,
    );

    let mut pending_tab_ids: HashSet<usize> = HashSet::new();
    let mut pending_tab_switches: HashSet<(usize, ClientId)> = HashSet::new(); // usize is the
                                                                               // tab_index
    let mut pending_events_waiting_for_tab: Vec<ScreenInstruction> = vec![];
    let mut pending_events_waiting_for_client: Vec<ScreenInstruction> = vec![];
    let mut plugin_loading_message_cache = HashMap::new();
    let mut keybind_intercepts = HashMap::new();
    loop {
        let (event, mut err_ctx) = screen
            .bus
            .recv()
            .context("failed to receive event on channel")?;
        err_ctx.add_call(ContextType::Screen((&event).into()));
        // here we start caching resizes, so that we'll send them in bulk at the end of each event
        // when this cache is Dropped, for more information, see the comments in PtyWriter
        let _resize_cache = ResizeCache::new(thread_senders.clone());

        match event {
            ScreenInstruction::PtyBytes(pid, vte_bytes) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_terminal_pid(pid) {
                        tab.handle_pty_bytes(pid, vte_bytes)
                            .context("failed to process pty bytes")?;
                        break;
                    }
                }
                let _ = screen
                    .bus
                    .senders
                    .send_to_background_jobs(BackgroundJob::RenderToClients);
            },
            ScreenInstruction::PluginBytes(mut plugin_render_assets) => {
                for plugin_render_asset in plugin_render_assets.iter_mut() {
                    let plugin_id = plugin_render_asset.plugin_id;
                    let client_id = plugin_render_asset.client_id;
                    let vte_bytes = plugin_render_asset.bytes.drain(..).collect();

                    let all_tabs = screen.get_tabs_mut();
                    for tab in all_tabs.values_mut() {
                        if tab.has_plugin(plugin_id) {
                            tab.handle_plugin_bytes(plugin_id, client_id, vte_bytes)
                                .context("failed to process plugin bytes")?;
                            break;
                        }
                    }
                    screen.render_blocker.remove_blocking_plugin(plugin_id);
                }
                screen.render(Some(plugin_render_assets))?;
            },
            ScreenInstruction::Render => {
                screen.render(None)?;
            },
            ScreenInstruction::RenderToClients => {
                // render_blocker.can_render() returning true means that either all pending plugins
                // (only those waiting for a new tab layout to be applied!) have been rendered or
                // that a 100ms timeout has been reached (more info in the RenderBlocker comment)
                if screen.render_blocker.can_render() {
                    screen.render_to_clients()?;
                } else {
                    screen.render(None)?;
                }
            },
            ScreenInstruction::NewPane(
                pid,
                initial_pane_title,
                hold_for_command,
                invoked_with,
                new_pane_placement,
                start_suppressed,
                client_or_tab_index,
                completion_tx,
                set_blocking,
            ) => {
                let blocking_notification = if set_blocking { completion_tx } else { None };

                match client_or_tab_index {
                    ClientTabIndexOrPaneId::ClientId(client_id) => {
                        active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| {
                            tab.new_pane(pid,
                               initial_pane_title,
                               invoked_with,
                               start_suppressed,
                               true,
                               new_pane_placement,
                               Some(client_id),
                               blocking_notification
                           )
                        }, ?);
                        if let Some(hold_for_command) = hold_for_command {
                            let is_first_run = true;
                            active_tab_and_connected_client_id!(
                                screen,
                                client_id,
                                |tab: &mut Tab, _client_id: ClientId| tab.hold_pane(
                                    pid,
                                    None,
                                    is_first_run,
                                    hold_for_command
                                )
                            )
                        }
                    },
                    ClientTabIndexOrPaneId::TabIndex(tab_index) => {
                        if let Some(active_tab) = screen.tabs.get_mut(&tab_index) {
                            active_tab.new_pane(
                                pid,
                                initial_pane_title,
                                invoked_with,
                                start_suppressed,
                                true,
                                new_pane_placement,
                                None,
                                blocking_notification,
                            )?;
                            if let Some(hold_for_command) = hold_for_command {
                                let is_first_run = true;
                                active_tab.hold_pane(pid, None, is_first_run, hold_for_command);
                            }
                        } else {
                            log::error!("Tab index not found: {:?}", tab_index);
                        }
                    },
                    ClientTabIndexOrPaneId::PaneId(pane_id) => {
                        let mut found = false;
                        let all_tabs = screen.get_tabs_mut();
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id) {
                                tab.new_pane(
                                    pid,
                                    initial_pane_title,
                                    invoked_with,
                                    start_suppressed,
                                    true,
                                    new_pane_placement,
                                    None,
                                    blocking_notification, // TODO: is this correct?
                                )?;
                                if let Some(hold_for_command) = hold_for_command {
                                    let is_first_run = true;
                                    tab.hold_pane(pid, None, is_first_run, hold_for_command);
                                }
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            log::error!(
                                "Failed to find tab containing pane with id: {:?}",
                                pane_id
                            );
                        }
                    },
                };
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::OpenInPlaceEditor(pid, client_tab_index_or_pane_id) => {
                match client_tab_index_or_pane_id {
                    ClientTabIndexOrPaneId::ClientId(client_id) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab
                            .replace_active_pane_with_editor_pane(pid, client_id), ?);
                        screen.log_and_report_session_state()?;
                    },
                    ClientTabIndexOrPaneId::TabIndex(_tab_index) => {
                        log::error!("Cannot OpenInPlaceEditor with a TabIndex");
                    },
                    ClientTabIndexOrPaneId::PaneId(pane_id_to_replace) => {
                        let mut found = false;
                        let all_tabs = screen.get_tabs_mut();
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id_to_replace) {
                                tab.replace_pane_with_editor_pane(pid, pane_id_to_replace)
                                    .non_fatal();
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            log::error!(
                                "Could not find pane with id {:?} to replace",
                                pane_id_to_replace
                            );
                        }
                    },
                }

                screen.render(None)?;
            },
            ScreenInstruction::TogglePaneEmbedOrFloating(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_pane_embed_or_floating(client_id), ?);
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::ToggleFloatingPanes(client_id, default_shell, completion_tx) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_floating_panes(Some(client_id), default_shell, completion_tx), ?);
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::WriteCharacter(
                key_with_modifier,
                raw_bytes,
                is_kitty_keyboard_protocol,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                if let Some(plugin_id) = keybind_intercepts.get(&client_id) {
                    if let Some(key_with_modifier) = key_with_modifier {
                        let _ = screen
                            .bus
                            .senders
                            .send_to_plugin(PluginInstruction::Update(vec![(
                                Some(*plugin_id),
                                Some(client_id),
                                Event::InterceptedKeyPress(key_with_modifier),
                            )]));
                        continue;
                    }
                }
                let mut state_changed = false;
                let client_input_mode = screen.get_client_input_mode(client_id);
                match client_input_mode {
                    Some(InputMode::RenameTab) => {
                        if !(raw_bytes == BRACKETED_PASTE_BEGIN || raw_bytes == BRACKETED_PASTE_END)
                        {
                            screen.update_active_tab_name(raw_bytes, client_id)?;
                            state_changed = true;
                        }
                    },
                    _ => {
                        active_tab_and_connected_client_id!(
                            screen,
                            client_id,
                            |tab: &mut Tab, client_id: ClientId| {
                                match client_input_mode {
                                    Some(InputMode::EnterSearch) => {
                                        if !(raw_bytes == BRACKETED_PASTE_BEGIN
                                            || raw_bytes == BRACKETED_PASTE_END)
                                        {
                                            if let Err(e) =
                                                tab.update_search_term(raw_bytes, client_id)
                                            {
                                                log::error!("{}", e);
                                            }
                                        }
                                        state_changed = true;
                                    },
                                    Some(InputMode::RenamePane) => {
                                        if !(raw_bytes == BRACKETED_PASTE_BEGIN
                                            || raw_bytes == BRACKETED_PASTE_END)
                                        {
                                            if let Err(e) =
                                                tab.update_active_pane_name(raw_bytes, client_id)
                                            {
                                                log::error!("{}", e);
                                            }
                                            state_changed = true;
                                        }
                                    },
                                    _ => {
                                        let write_result = match tab.is_sync_panes_active() {
                                            true => tab.write_to_terminals_on_current_tab(
                                                &key_with_modifier,
                                                raw_bytes,
                                                is_kitty_keyboard_protocol,
                                                client_id,
                                            ),
                                            false => tab.write_to_active_terminal(
                                                &key_with_modifier,
                                                raw_bytes,
                                                is_kitty_keyboard_protocol,
                                                client_id,
                                            ),
                                        };
                                        if let Ok(true) = write_result {
                                            state_changed = true;
                                        }
                                    },
                                }
                            }
                        );
                    },
                };
                if state_changed {
                    screen.log_and_report_session_state()?;
                }
                screen.render(None)?;
            },
            ScreenInstruction::Resize(
                client_id,
                strategy,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.resize(client_id, strategy),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SwitchFocus(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::FocusNextPane(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::FocusPreviousPane(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_previous_pane(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusLeft(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_left(client_id),
                    ?
                );
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusLeftOrPreviousTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.move_focus_left_or_previous_tab(client_id)?;
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_down(client_id),
                    ?
                );
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusRight(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_right(client_id),
                    ?
                );
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusRightOrNextTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.move_focus_right_or_next_tab(client_id)?;
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_up(client_id),
                    ?
                );
                screen.add_active_pane_to_group_if_marking(&client_id);
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ClearScreen(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.clear_active_terminal_screen(
                        client_id,
                    ),
                    ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::DumpScreen(
                file,
                client_id,
                full,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.dump_active_terminal_screen(
                        Some(file.to_string()),
                        client_id,
                        full
                    ),
                    ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::DumpLayout(default_shell, client_id, completion_tx) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata = screen.get_layout_metadata(default_shell);
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::DumpLayout(
                        session_layout_metadata,
                        client_id,
                        completion_tx,
                    ))
                    .with_context(err_context)?;
            },
            ScreenInstruction::ListClientsMetadata(default_shell, client_id, completion_tx) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata = screen.get_layout_metadata(default_shell);
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::ListClientsMetadata(
                        session_layout_metadata,
                        client_id,
                        completion_tx,
                    ))
                    .with_context(err_context)?;
            },
            ScreenInstruction::DumpLayoutToPlugin(plugin_id) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata =
                    screen.get_layout_metadata(Some(screen.default_shell.clone()));
                screen
                    .bus
                    .senders
                    .send_to_pty(PtyInstruction::DumpLayoutToPlugin(
                        session_layout_metadata,
                        plugin_id,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            ScreenInstruction::ListClientsToPlugin(plugin_id, client_id) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata =
                    screen.get_layout_metadata(Some(screen.default_shell.clone()));
                screen
                    .bus
                    .senders
                    .send_to_pty(PtyInstruction::ListClientsToPlugin(
                        session_layout_metadata,
                        plugin_id,
                        client_id,
                    ))
                    .with_context(err_context)
                    .non_fatal();
            },
            ScreenInstruction::EditScrollback(client_id, completion_tx) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.edit_scrollback(client_id, completion_tx),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::GetPaneScrollback {
                pane_id,
                client_id,
                get_full_scrollback,
                response_channel,
            } => {
                let mut pane_contents: Option<PaneContents> = None;
                for tab in screen.get_tabs_mut().values() {
                    if let Some(pane) = tab.get_pane_with_id(pane_id) {
                        pane_contents =
                            Some(pane.pane_contents(Some(client_id), get_full_scrollback));
                        break;
                    }
                }
                // Send response back through channel
                let response = match pane_contents {
                    Some(contents) => PaneScrollbackResponse::Ok(contents),
                    None => {
                        log::warn!(
                            "Plugin requested scrollback for pane {:?} but pane was not found",
                            pane_id
                        );
                        PaneScrollbackResponse::Err(format!("Pane {:?} not found", pane_id))
                    },
                };
                if let Err(_) = response_channel.send(response) {
                    // the plugin likely timed out and dropped the receiver
                    log::debug!(
                        "Plugin timed out before pane scrollback response was sent for pane {:?}",
                        pane_id
                    );
                }
            },
            ScreenInstruction::ScrollUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_up(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::MovePane(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneBackwards(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_backwards(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_down(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_up(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneRight(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_right(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneLeft(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_left(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ScrollUpAt(
                point,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_up(&point, 3, client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ScrollDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_down(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ScrollDownAt(
                point,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_down(&point, 3, client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ScrollToBottom(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_to_bottom(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ScrollToTop(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_to_top(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::PageScrollUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_page(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::PageScrollDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_page(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::HalfPageScrollUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_half_page(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::HalfPageScrollDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_half_page(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ClearScroll(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .clear_active_terminal_scroll(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::CloseFocusedPane(client_id, completion_tx) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.close_focused_pane(client_id, completion_tx), ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SetSelectable(pid, selectable) => {
                let all_tabs = screen.get_tabs_mut();
                let mut found_plugin = false;
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pid) {
                        tab.set_pane_selectable(pid, selectable);
                        found_plugin = true;
                        break;
                    }
                }
                if !found_plugin {
                    pending_events_waiting_for_tab
                        .push(ScreenInstruction::SetSelectable(pid, selectable));
                }
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SetMouseSelectionSupport(pid, selection_support) => {
                let all_tabs = screen.get_tabs_mut();
                let mut found_plugin = false;
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pid) {
                        tab.set_mouse_selection_support(pid, selection_support);
                        found_plugin = true;
                        break;
                    }
                }
                if !found_plugin {
                    pending_events_waiting_for_tab.push(
                        ScreenInstruction::SetMouseSelectionSupport(pid, selection_support),
                    );
                }
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ClosePane(
                id,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                // waiting for it
                exit_status,
            ) => {
                match client_id {
                    Some(client_id) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab.close_pane(
                            id,
                            false,
                            exit_status
                        ));
                    },
                    None => {
                        for tab in screen.tabs.values_mut() {
                            if tab.get_all_pane_ids().contains(&id) {
                                tab.close_pane(id, false, exit_status);
                                break;
                            }
                        }
                    },
                }

                screen.log_and_report_session_state()?;
                screen.retain_only_existing_panes_in_pane_groups();
            },
            ScreenInstruction::HoldPane(id, exit_status, run_command) => {
                let is_first_run = false;
                for tab in screen.tabs.values_mut() {
                    if tab.get_all_pane_ids().contains(&id) {
                        tab.hold_pane(id, exit_status, is_first_run, run_command);
                        break;
                    }
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::UpdatePaneName(
                c,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_active_pane_name(c, client_id), ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::UndoRenamePane(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.undo_active_rename_pane(client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::ToggleActiveTerminalFullscreen(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_active_pane_fullscreen(client_id)
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::TogglePaneFrames(
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.draw_pane_frames = !screen.draw_pane_frames;
                for tab in screen.tabs.values_mut() {
                    tab.set_pane_frames(screen.draw_pane_frames);
                }
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SwitchTabNext(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.switch_tab_next(None, true, client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::SwitchTabPrev(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.switch_tab_prev(None, true, client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::CloseTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.close_tab(client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::NewTab(
                cwd,
                default_shell,
                layout,
                floating_panes_layout,
                tab_name,
                swap_layouts,
                should_change_focus_to_new_tab,
                (client_id, is_web_client),
                completion_tx,
            ) => {
                let tab_index = screen.get_new_tab_index();
                pending_tab_ids.insert(tab_index);
                let client_id_for_new_tab = if should_change_focus_to_new_tab {
                    Some(client_id)
                } else {
                    None
                };
                screen.new_tab(
                    tab_index,
                    swap_layouts,
                    tab_name.clone(),
                    client_id_for_new_tab,
                )?;
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::NewTab(
                        cwd,
                        default_shell,
                        layout,
                        floating_panes_layout,
                        tab_index,
                        should_change_focus_to_new_tab,
                        (client_id, is_web_client),
                        completion_tx,
                    ))?;
            },
            ScreenInstruction::ApplyLayout(
                layout,
                floating_panes_layout,
                new_pane_pids,
                new_floating_pane_pids,
                new_plugin_ids,
                tab_index,
                should_change_focus_to_new_tab,
                (client_id, is_web_client),
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.apply_layout(
                    layout,
                    floating_panes_layout,
                    new_pane_pids.clone(),
                    new_floating_pane_pids,
                    new_plugin_ids.clone(),
                    tab_index,
                    should_change_focus_to_new_tab,
                    (client_id, is_web_client),
                )?;
                pending_tab_ids.remove(&tab_index);
                if pending_tab_ids.is_empty() {
                    for (tab_index, client_id) in pending_tab_switches.drain() {
                        screen.go_to_tab(tab_index as usize + 1, client_id)?;
                    }
                    if should_change_focus_to_new_tab {
                        screen.go_to_tab(tab_index as usize + 1, client_id)?;
                    }
                } else if should_change_focus_to_new_tab {
                    let client_id_to_switch = if screen.active_tab_indices.contains_key(&client_id)
                    {
                        Some(client_id)
                    } else {
                        screen.active_tab_indices.keys().next().copied()
                    };
                    if let Some(client_id_to_switch) = client_id_to_switch {
                        pending_tab_switches.insert((tab_index as usize, client_id_to_switch));
                    }
                }

                for plugin_ids in new_plugin_ids.values() {
                    for plugin_id in plugin_ids {
                        if let Some(loading_indication) =
                            plugin_loading_message_cache.remove(plugin_id)
                        {
                            screen.update_plugin_loading_stage(*plugin_id, loading_indication);
                            screen.render(None)?;
                        }
                        screen.render_blocker.register_blocking_plugin(*plugin_id);
                    }
                }

                for event in pending_events_waiting_for_client.drain(..) {
                    screen.bus.senders.send_to_screen(event).non_fatal();
                }

                for event in pending_events_waiting_for_tab.drain(..) {
                    screen.bus.senders.send_to_screen(event).non_fatal();
                }

                screen.render(None)?;
                // we do this here in order to recover from a race condition on app start
                // that sometimes causes Zellij to think the terminal window is a different size
                // than it actually is - here, we query the client for its terminal size after
                // we've finished the setup and handle it as we handle a normal resize,
                // while this can affect other instances of a layout being applied, the query is
                // very short and cheap and shouldn't cause any trouble
                if let Some(os_input) = &mut screen.bus.os_input {
                    for (client_id, _is_web_client) in screen.connected_clients.borrow().iter() {
                        let _ = os_input
                            .send_to_client(*client_id, ServerToClientMsg::QueryTerminalSize);
                    }
                }
            },
            ScreenInstruction::GoToTab(
                tab_index,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                let client_id_to_switch = if client_id.is_none() {
                    None
                } else if screen
                    .active_tab_indices
                    .contains_key(&client_id.expect("This is checked above"))
                {
                    client_id
                } else {
                    screen.active_tab_indices.keys().next().copied()
                };
                match client_id_to_switch {
                    // we must make sure pending_tab_ids is empty because otherwise we cannot be
                    // sure this instruction is applied at the right time (eg. we might have a
                    // pending tab that will become not-pending after this instruction and change
                    // the client focus, which should have happened before this instruction and not
                    // after)
                    Some(client_id) if pending_tab_ids.is_empty() => {
                        screen.go_to_tab(tab_index as usize, client_id)?;
                        screen.render(None)?;
                    },
                    _ => {
                        if let Some(client_id) = client_id {
                            pending_tab_switches.insert((tab_index as usize, client_id));
                        }
                    },
                }
            },
            ScreenInstruction::GoToTabName(
                tab_name,
                swap_layouts,
                default_shell,
                create,
                client_id,
                completion_tx,
            ) => {
                let client_id = if client_id.is_none() {
                    None
                } else if screen
                    .active_tab_indices
                    .contains_key(&client_id.expect("This is checked above"))
                {
                    client_id
                } else {
                    screen.active_tab_indices.keys().next().copied()
                };
                if let Some(client_id) = client_id {
                    let is_web_client = screen
                        .connected_clients
                        .borrow()
                        .get(&client_id)
                        .copied()
                        .unwrap_or(false);
                    if let Ok(tab_exists) = screen.go_to_tab_name(tab_name.clone(), client_id) {
                        screen.render(None)?;
                        if create && !tab_exists {
                            let tab_index = screen.get_new_tab_index();
                            let should_change_focus_to_new_tab = true;
                            screen.new_tab(
                                tab_index,
                                swap_layouts,
                                Some(tab_name),
                                Some(client_id),
                            )?;
                            screen
                                .bus
                                .senders
                                .send_to_plugin(PluginInstruction::NewTab(
                                    None,
                                    default_shell,
                                    None,
                                    vec![],
                                    tab_index,
                                    should_change_focus_to_new_tab,
                                    (client_id, is_web_client),
                                    completion_tx,
                                ))?;
                            continue; // so we don't get to the completion signalling below
                        }
                    }
                }
            },
            ScreenInstruction::UpdateTabName(
                c,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.update_active_tab_name(c, client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::UndoRenameTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.undo_active_rename_tab(client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::MoveTabLeft(client_id, completion_tx) => {
                if pending_tab_ids.is_empty() {
                    screen.move_active_tab_to_left(client_id)?;
                    screen.render(None)?;
                } else {
                    // Defer execution, forward completion_tx
                    pending_events_waiting_for_tab
                        .push(ScreenInstruction::MoveTabLeft(client_id, completion_tx));
                }
            },
            ScreenInstruction::MoveTabRight(client_id, completion_tx) => {
                if pending_tab_ids.is_empty() {
                    screen.move_active_tab_to_right(client_id)?;
                    screen.render(None)?;
                } else {
                    // Defer execution, forward completion_tx
                    pending_events_waiting_for_tab
                        .push(ScreenInstruction::MoveTabRight(client_id, completion_tx));
                }
            },
            ScreenInstruction::TerminalResize(new_size) => {
                screen.resize_to_screen(new_size)?;
                screen.log_and_report_session_state()?; // update tabs so that the ui indication will be send to the plugins
                screen.render(None)?;
            },
            ScreenInstruction::TerminalPixelDimensions(pixel_dimensions) => {
                screen.update_pixel_dimensions(pixel_dimensions);
            },
            ScreenInstruction::TerminalBackgroundColor(background_color_instruction) => {
                screen.update_terminal_background_color(background_color_instruction);
            },
            ScreenInstruction::TerminalForegroundColor(background_color_instruction) => {
                screen.update_terminal_foreground_color(background_color_instruction);
            },
            ScreenInstruction::TerminalColorRegisters(color_registers) => {
                screen.update_terminal_color_registers(color_registers);
            },
            ScreenInstruction::ChangeMode(
                mode_info,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.change_mode(mode_info, client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::ChangeModeForAllClients(
                mode_info,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.change_mode_for_all_clients(mode_info)?;
                screen.render(None)?;
            },
            ScreenInstruction::ToggleActiveSyncTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, _client_id: ClientId| tab.toggle_sync_panes_is_active()
                );
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::MouseEvent(
                event,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.handle_mouse_event(event, client_id);
            },
            ScreenInstruction::Copy(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .copy_selection(client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::Exit => {
                break;
            },
            ScreenInstruction::ToggleTab(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.toggle_tab(client_id)?;
                screen.render(None)?;
            },
            ScreenInstruction::AddClient(
                client_id,
                is_web_client,
                tab_position_to_focus,
                pane_id_to_focus,
            ) => {
                screen.add_client(client_id, is_web_client)?;
                let pane_id = pane_id_to_focus.map(|(pane_id, is_plugin)| {
                    if is_plugin {
                        PaneId::Plugin(pane_id)
                    } else {
                        PaneId::Terminal(pane_id)
                    }
                });
                if let Some(pane_id) = pane_id {
                    screen.focus_pane_with_id(pane_id, true, client_id)?;
                } else if let Some(tab_position_to_focus) = tab_position_to_focus {
                    screen.go_to_tab(tab_position_to_focus, client_id)?;
                }
                for event in pending_events_waiting_for_client.drain(..) {
                    screen.bus.senders.send_to_screen(event).non_fatal();
                }
                screen.log_and_report_session_state()?;

                if is_web_client {
                    // we do this because
                    // we need to query the client for its size, and we must do it only after we've
                    // added it to our state.
                    //
                    // we have to do this specifically for web clients because the browser (as opposed
                    // to a traditional terminal) can only figure out its dimensions after we sent it relevant
                    // state (eg. font, which is controlled by our config and it needs to determine cell size)
                    if let Some(os_input) = &mut screen.bus.os_input {
                        let _ = os_input
                            .send_to_client(client_id, ServerToClientMsg::QueryTerminalSize);
                    }
                }

                screen.render(None)?;
            },
            ScreenInstruction::RemoveClient(client_id) => {
                screen.remove_client(client_id)?;
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::AddOverlay(overlay, _client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.get_active_overlays_mut().push(overlay);
            },
            ScreenInstruction::RemoveOverlay(_client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.render(None)?;
            },
            ScreenInstruction::ConfirmPrompt(
                _client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                let overlay = screen.get_active_overlays_mut().pop();
                let instruction = overlay.and_then(|o| o.prompt_confirm());
                if let Some(instruction) = instruction {
                    screen
                        .bus
                        .senders
                        .send_to_server(*instruction)
                        .context("failed to confirm prompt")?;
                }
            },
            ScreenInstruction::DenyPrompt(
                _client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.get_active_overlays_mut().pop();
                screen.render(None)?;
            },
            ScreenInstruction::UpdateSearch(
                c,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_search_term(c, client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchDown(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_down(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchUp(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_up(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchToggleCaseSensitivity(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_search_case_sensitivity(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchToggleWrap(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_wrap(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchToggleWholeWord(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_whole_words(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::AddRedPaneFrameColorOverride(pane_ids, error_text) => {
                let all_tabs = screen.get_tabs_mut();
                for pane_id in pane_ids {
                    for tab in all_tabs.values_mut() {
                        if tab.has_pane_with_pid(&pane_id) {
                            tab.add_red_pane_frame_color_override(pane_id, error_text.clone());
                            break;
                        }
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::AddHighlightPaneFrameColorOverride(pane_ids, error_text) => {
                let all_tabs = screen.get_tabs_mut();
                for pane_id in pane_ids {
                    for tab in all_tabs.values_mut() {
                        if tab.has_pane_with_pid(&pane_id) {
                            tab.add_highlight_pane_frame_color_override(
                                pane_id,
                                error_text.clone(),
                                None,
                            );
                            break;
                        }
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ClearPaneFrameColorOverride(pane_ids) => {
                let all_tabs = screen.get_tabs_mut();
                for pane_id in pane_ids {
                    for tab in all_tabs.values_mut() {
                        if tab.has_pane_with_pid(&pane_id) {
                            tab.clear_pane_frame_color_override(pane_id, None);
                            break;
                        }
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::PreviousSwapLayout(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, _client_id: ClientId| tab.previous_swap_layout(),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::NextSwapLayout(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, _client_id: ClientId| tab.next_swap_layout(),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::QueryTabNames(client_id, completion_tx) => {
                let tab_names = screen
                    .get_tabs_mut()
                    .values()
                    .map(|tab| tab.name.clone())
                    .collect::<Vec<String>>();
                screen.bus.senders.send_to_server(ServerInstruction::Log(
                    tab_names,
                    client_id,
                    completion_tx,
                ))?;
            },
            ScreenInstruction::NewTiledPluginPane(
                run_plugin,
                pane_title,
                skip_cache,
                cwd,
                client_id,
                completion_tx,
            ) => {
                let tab_index = screen.active_tab_indices.values().next().unwrap_or(&1);
                let size = Size::default();
                let should_float = Some(false);
                let should_be_opened_in_place = false;
                screen
                    .bus
                    .senders
                    .send_to_pty(PtyInstruction::FillPluginCwd(
                        should_float,
                        should_be_opened_in_place,
                        pane_title,
                        run_plugin,
                        *tab_index,
                        None,
                        client_id,
                        size,
                        skip_cache,
                        cwd,
                        None,
                        None,
                        completion_tx,
                    ))?;
            },
            ScreenInstruction::NewFloatingPluginPane(
                run_plugin,
                pane_title,
                skip_cache,
                cwd,
                floating_pane_coordinates,
                client_id,
                completion_tx,
            ) => match screen.active_tab_indices.values().next() {
                Some(tab_index) => {
                    let size = Size::default();
                    let should_float = Some(true);
                    let should_be_opened_in_place = false;
                    screen
                        .bus
                        .senders
                        .send_to_pty(PtyInstruction::FillPluginCwd(
                            should_float,
                            should_be_opened_in_place,
                            pane_title,
                            run_plugin,
                            *tab_index,
                            None,
                            client_id,
                            size,
                            skip_cache,
                            cwd,
                            None,
                            floating_pane_coordinates,
                            completion_tx,
                        ))?;
                },
                None => {
                    log::error!(
                        "Could not find an active tab - is there at least 1 connected user?"
                    );
                },
            },
            ScreenInstruction::NewInPlacePluginPane(
                run_plugin,
                pane_title,
                pane_id_to_replace,
                skip_cache,
                client_id,
                completion_tx,
            ) => match screen.active_tab_indices.values().next() {
                Some(tab_index) => {
                    let size = Size::default();
                    let should_float = None;
                    let should_be_in_place = true;
                    screen
                        .bus
                        .senders
                        .send_to_pty(PtyInstruction::FillPluginCwd(
                            should_float,
                            should_be_in_place,
                            pane_title,
                            run_plugin,
                            *tab_index,
                            Some(pane_id_to_replace),
                            client_id,
                            size,
                            skip_cache,
                            None,
                            None,
                            None,
                            completion_tx,
                        ))?;
                },
                None => {
                    log::error!(
                        "Could not find an active tab - is there at least 1 connected user?"
                    );
                },
            },
            ScreenInstruction::StartOrReloadPluginPane(run_plugin, pane_title, completion_tx) => {
                let tab_index = screen.active_tab_indices.values().next().unwrap_or(&1);
                let size = Size::default();
                let should_float = Some(false);

                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::Reload(
                        should_float,
                        pane_title,
                        run_plugin,
                        *tab_index,
                        size,
                        completion_tx,
                    ))?;
            },
            ScreenInstruction::AddPlugin(
                should_float,
                should_be_in_place,
                run_plugin_or_alias,
                pane_title,
                tab_index,
                plugin_id,
                pane_id_to_replace,
                cwd,
                start_suppressed,
                floating_pane_coordinates,
                should_focus_plugin,
                client_id,
                completion_tx,
            ) => {
                let close_replaced_pane = false; // TODO: support this
                let mut new_pane_placement = NewPanePlacement::default();
                let maybe_should_float = should_float;
                let should_be_tiled = maybe_should_float.map(|f| !f).unwrap_or(false);
                let should_float = maybe_should_float.unwrap_or(false);
                if floating_pane_coordinates.is_some() || should_float {
                    new_pane_placement = NewPanePlacement::with_floating_pane_coordinates(
                        floating_pane_coordinates.clone(),
                    );
                }
                if should_be_tiled {
                    new_pane_placement = NewPanePlacement::Tiled(None);
                }
                if should_be_in_place {
                    new_pane_placement = NewPanePlacement::with_pane_id_to_replace(
                        pane_id_to_replace.map(|id| id.into()),
                        close_replaced_pane,
                    );
                }
                if screen.active_tab_indices.is_empty() && tab_index.is_none() {
                    pending_events_waiting_for_client.push(ScreenInstruction::AddPlugin(
                        maybe_should_float,
                        should_be_in_place,
                        run_plugin_or_alias,
                        pane_title,
                        tab_index,
                        plugin_id,
                        pane_id_to_replace,
                        cwd,
                        start_suppressed,
                        floating_pane_coordinates,
                        should_focus_plugin,
                        client_id,
                        completion_tx,
                    ));
                    continue;
                }
                let pane_title = pane_title.unwrap_or_else(|| {
                    format!(
                        "({}) - {}",
                        cwd.map(|cwd| cwd.display().to_string())
                            .unwrap_or(".".to_owned()),
                        run_plugin_or_alias.location_string()
                    )
                });
                let run_plugin = Run::Plugin(run_plugin_or_alias);

                let close_replaced_pane = false;
                if should_be_in_place {
                    if let Some(pane_id_to_replace) = pane_id_to_replace {
                        let client_tab_index_or_pane_id =
                            ClientTabIndexOrPaneId::PaneId(pane_id_to_replace);
                        screen.replace_pane(
                            PaneId::Plugin(plugin_id),
                            None,
                            Some(run_plugin),
                            Some(pane_title),
                            close_replaced_pane,
                            client_tab_index_or_pane_id,
                        )?;
                    } else if let Some(client_id) = client_id {
                        let client_tab_index_or_pane_id =
                            ClientTabIndexOrPaneId::ClientId(client_id);
                        screen.replace_pane(
                            PaneId::Plugin(plugin_id),
                            None,
                            Some(run_plugin),
                            Some(pane_title),
                            close_replaced_pane,
                            client_tab_index_or_pane_id,
                        )?;
                    } else {
                        log::error!("Must have pane id to replace or connected client_id if replacing a pane");
                    }
                } else if let Some(client_id) = client_id {
                    active_tab_and_connected_client_id!(screen, client_id, |active_tab: &mut Tab, _client_id: ClientId| {
                        active_tab.new_pane(
                            PaneId::Plugin(plugin_id),
                            Some(pane_title),
                            Some(run_plugin),
                            start_suppressed,
                            should_focus_plugin.unwrap_or(true),
                            new_pane_placement,
                            Some(client_id),
                            None,
                        )
                    }, ?);
                } else if let Some(active_tab) =
                    tab_index.and_then(|tab_index| screen.tabs.get_mut(&tab_index))
                {
                    active_tab.new_pane(
                        PaneId::Plugin(plugin_id),
                        Some(pane_title),
                        Some(run_plugin),
                        start_suppressed,
                        should_focus_plugin.unwrap_or(true),
                        new_pane_placement,
                        None,
                        None,
                    )?;
                } else {
                    log::error!("Tab index not found: {:?}", tab_index);
                }
                if let Some(loading_indication) = plugin_loading_message_cache.remove(&plugin_id) {
                    screen.update_plugin_loading_stage(plugin_id, loading_indication);
                    screen.render(None)?;
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::UpdatePluginLoadingStage(pid, loading_indication) => {
                let found_plugin =
                    screen.update_plugin_loading_stage(pid, loading_indication.clone());
                if !found_plugin {
                    plugin_loading_message_cache.insert(pid, loading_indication);
                }
                screen.render(None)?;
            },
            ScreenInstruction::StartPluginLoadingIndication(pid, loading_indication) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_plugin(pid) {
                        tab.start_plugin_loading_indication(pid, loading_indication);
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ProgressPluginLoadingOffset(pid) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_plugin(pid) {
                        tab.progress_plugin_loading_offset(pid);
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::RequestStateUpdateForPlugins => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    tab.update_input_modes()?;
                }
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::LaunchOrFocusPlugin(
                run_plugin,
                should_float,
                move_to_focused_tab,
                should_open_in_place,
                pane_id_to_replace,
                skip_cache,
                client_id,
                completion_tx,
            ) => match pane_id_to_replace {
                Some(pane_id_to_replace) if should_open_in_place => {
                    match screen.active_tab_indices.values().next() {
                        Some(tab_index) => {
                            let size = Size::default();
                            screen
                                .bus
                                .senders
                                .send_to_pty(PtyInstruction::FillPluginCwd(
                                    Some(should_float),
                                    should_open_in_place,
                                    None,
                                    run_plugin,
                                    *tab_index,
                                    Some(pane_id_to_replace),
                                    client_id,
                                    size,
                                    skip_cache,
                                    None,
                                    None,
                                    None,
                                    completion_tx,
                                ))?;
                        },
                        None => {
                            log::error!(
                            "Could not find an active tab - is there at least 1 connected user?"
                        );
                        },
                    }
                },
                _ => {
                    let client_id = if screen.active_tab_indices.contains_key(&client_id) {
                        Some(client_id)
                    } else {
                        screen.get_first_client_id()
                    };
                    let client_id_and_focused_tab = client_id.and_then(|client_id| {
                        screen
                            .active_tab_indices
                            .get(&client_id)
                            .map(|tab_index| (*tab_index, client_id))
                    });
                    match client_id_and_focused_tab {
                        Some((tab_index, client_id)) => {
                            if screen.focus_plugin_pane(
                                &run_plugin,
                                should_float,
                                move_to_focused_tab,
                                client_id,
                            )? {
                                screen.render(None)?;
                                screen.log_and_report_session_state()?;
                            } else {
                                screen
                                    .bus
                                    .senders
                                    .send_to_pty(PtyInstruction::FillPluginCwd(
                                        Some(should_float),
                                        should_open_in_place,
                                        None,
                                        run_plugin,
                                        tab_index,
                                        None,
                                        client_id,
                                        Size::default(),
                                        skip_cache,
                                        None,
                                        None,
                                        None,
                                        completion_tx,
                                    ))?;
                            }
                        },
                        None => {
                            log::error!("No connected clients found - cannot load or focus plugin")
                        },
                    }
                },
            },
            ScreenInstruction::LaunchPlugin(
                run_plugin,
                should_float,
                should_open_in_place,
                pane_id_to_replace,
                skip_cache,
                cwd,
                client_id,
                completion_tx,
            ) => match pane_id_to_replace {
                Some(pane_id_to_replace) => match screen.active_tab_indices.values().next() {
                    Some(tab_index) => {
                        let size = Size::default();
                        screen
                            .bus
                            .senders
                            .send_to_pty(PtyInstruction::FillPluginCwd(
                                Some(should_float),
                                should_open_in_place,
                                None,
                                run_plugin,
                                *tab_index,
                                Some(pane_id_to_replace),
                                client_id,
                                size,
                                skip_cache,
                                cwd,
                                None,
                                None,
                                completion_tx,
                            ))?;
                    },
                    None => {
                        log::error!(
                            "Could not find an active tab - is there at least 1 connected user?"
                        );
                    },
                },
                None => {
                    let client_id = if screen.active_tab_indices.contains_key(&client_id) {
                        Some(client_id)
                    } else {
                        screen.get_first_client_id()
                    };
                    let client_id_and_focused_tab = client_id.and_then(|client_id| {
                        screen
                            .active_tab_indices
                            .get(&client_id)
                            .map(|tab_index| (*tab_index, client_id))
                    });
                    match client_id_and_focused_tab {
                        Some((tab_index, client_id)) => {
                            screen
                                .bus
                                .senders
                                .send_to_pty(PtyInstruction::FillPluginCwd(
                                    Some(should_float),
                                    should_open_in_place,
                                    None,
                                    run_plugin,
                                    tab_index,
                                    None,
                                    client_id,
                                    Size::default(),
                                    skip_cache,
                                    cwd,
                                    None,
                                    None,
                                    completion_tx,
                                ))?;
                        },
                        None => {
                            log::error!("No connected clients found - cannot load or focus plugin")
                        },
                    }
                },
            },
            ScreenInstruction::SuppressPane(pane_id, client_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_non_suppressed_pane_with_pid(&pane_id) {
                        tab.suppress_pane(pane_id, Some(client_id));
                        drop(screen.render(None));
                        break;
                    }
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::FocusPaneWithId(
                pane_id,
                should_float_if_hidden,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.focus_pane_with_id(pane_id, should_float_if_hidden, client_id)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::RenamePane(
                pane_id,
                new_name,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        match tab.rename_pane(new_name, pane_id) {
                            Ok(()) => drop(screen.render(None)),
                            Err(e) => log::error!("Failed to rename pane: {:?}", e),
                        }
                        break;
                    }
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::RenameTab(
                tab_index,
                new_name,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                match screen.tabs.get_mut(&tab_index.saturating_sub(1)) {
                    Some(tab) => {
                        tab.name = String::from_utf8_lossy(&new_name).to_string();
                    },
                    None => {
                        log::error!("Failed to find tab with index: {:?}", tab_index);
                    },
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::RequestPluginPermissions(plugin_id, plugin_permission) => {
                let all_tabs = screen.get_tabs_mut();
                let found = all_tabs.values_mut().any(|tab| {
                    if tab.has_plugin(plugin_id) {
                        tab.request_plugin_permissions(plugin_id, Some(plugin_permission.clone()));
                        true
                    } else {
                        false
                    }
                });

                if !found {
                    log::error!("PluginId '{}' not found - caching request", plugin_id);
                    pending_events_waiting_for_client.push(
                        ScreenInstruction::RequestPluginPermissions(plugin_id, plugin_permission),
                    );
                }
            },
            ScreenInstruction::BreakPane(
                default_layout,
                default_shell,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.break_pane(default_shell, default_layout, client_id)?;
            },
            ScreenInstruction::BreakPaneRight(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.break_pane_to_new_tab(Direction::Right, client_id)?;
            },
            ScreenInstruction::BreakPaneLeft(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.break_pane_to_new_tab(Direction::Left, client_id)?;
            },
            ScreenInstruction::UpdateSessionInfos(new_session_infos, resurrectable_sessions) => {
                screen.update_session_infos(new_session_infos, resurrectable_sessions)?;
            },
            ScreenInstruction::ReplacePane(
                new_pane_id,
                hold_for_command,
                pane_title,
                invoked_with,
                close_replaced_pane,
                client_id_tab_index_or_pane_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.replace_pane(
                    new_pane_id,
                    hold_for_command,
                    invoked_with,
                    pane_title,
                    close_replaced_pane,
                    client_id_tab_index_or_pane_id,
                )?;

                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SerializeLayoutForResurrection => {
                if screen.session_serialization {
                    screen.dump_layout_to_hd()?;
                }
            },
            ScreenInstruction::RenameSession(
                name,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                if screen.session_infos_on_machine.contains_key(&name) {
                    let error_text = "A session by this name already exists.";
                    log::error!("{}", error_text);
                    if let Some(os_input) = &mut screen.bus.os_input {
                        let _ = os_input.send_to_client(
                            client_id,
                            ServerToClientMsg::LogError {
                                lines: vec![error_text.to_owned()],
                            },
                        );
                    }
                } else if screen.resurrectable_sessions.contains_key(&name) {
                    let error_text =
                        "A resurrectable session by this name exists, cannot use this name.";
                    log::error!("{}", error_text);
                    if let Some(os_input) = &mut screen.bus.os_input {
                        let _ = os_input.send_to_client(
                            client_id,
                            ServerToClientMsg::LogError {
                                lines: vec![error_text.to_owned()],
                            },
                        );
                    }
                } else {
                    let err_context = || format!("Failed to rename session");
                    let old_session_name = screen.session_name.clone();

                    // update state
                    screen.session_name = name.clone();
                    screen.default_mode_info.session_name = Some(name.clone());
                    for (_client_id, mode_info) in screen.mode_info.iter_mut() {
                        mode_info.session_name = Some(name.clone());
                    }
                    for (_, tab) in screen.tabs.iter_mut() {
                        tab.rename_session(name.clone()).with_context(err_context)?;
                    }

                    // rename socket file
                    let old_socket_file_path = ZELLIJ_SOCK_DIR.join(&old_session_name);
                    let new_socket_file_path = ZELLIJ_SOCK_DIR.join(&name);
                    if let Err(e) = std::fs::rename(old_socket_file_path, new_socket_file_path) {
                        log::error!("Failed to rename ipc socket: {:?}", e);
                    }

                    // rename session_info folder (TODO: make this atomic, right now there is a
                    // chance background_jobs will re-create this folder before it knows the
                    // session was renamed)
                    let old_session_info_folder =
                        session_info_folder_for_session(&old_session_name);
                    let new_session_info_folder = session_info_folder_for_session(&name);
                    if let Err(e) =
                        std::fs::rename(old_session_info_folder, new_session_info_folder)
                    {
                        log::error!("Failed to rename session_info folder: {:?}", e);
                    }

                    // report
                    screen
                        .log_and_report_session_state()
                        .with_context(err_context)?;

                    // set the env variable
                    set_session_name(name.clone());
                    let connected_client_ids: Vec<ClientId> =
                        screen.active_tab_indices.keys().copied().collect();
                    for client_id in connected_client_ids {
                        if let Some(os_input) = &mut screen.bus.os_input {
                            let _ = os_input.send_to_client(
                                client_id,
                                ServerToClientMsg::RenamedSession { name: name.clone() },
                            );
                        }
                    }
                }
            },
            ScreenInstruction::Reconfigure {
                client_id,
                keybinds,
                default_mode,
                theme,
                simplified_ui,
                default_shell,
                pane_frames,
                copy_to_clipboard,
                copy_command,
                copy_on_select,
                auto_layout,
                rounded_corners,
                hide_session_name,
                tabline_prefix_text,
                stacked_resize,
                default_editor,
                advanced_mouse_actions,
            } => {
                screen
                    .reconfigure(
                        keybinds,
                        default_mode,
                        theme,
                        simplified_ui,
                        default_shell,
                        pane_frames,
                        copy_command,
                        copy_to_clipboard,
                        copy_on_select,
                        auto_layout,
                        rounded_corners,
                        hide_session_name,
                        tabline_prefix_text,
                        stacked_resize,
                        default_editor,
                        advanced_mouse_actions,
                        client_id,
                    )
                    .non_fatal();
            },
            ScreenInstruction::RerunCommandPane(terminal_pane_id, completion_tx) => {
                screen.rerun_command_pane_with_id(terminal_pane_id, completion_tx)
            },
            ScreenInstruction::ResizePaneWithId(resize, pane_id) => {
                screen.resize_pane_with_id(resize, pane_id)
            },
            ScreenInstruction::EditScrollbackForPaneWithId(pane_id, completion_tx) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.edit_scrollback_for_pane_with_id(pane_id, completion_tx)
                            .non_fatal();
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::WriteToPaneId(bytes, pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.write_to_pane_id(&None, bytes, false, pane_id, None, None)
                            .non_fatal();
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::MovePaneWithPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.move_pane(pane_id);
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::MovePaneWithPaneIdInDirection(pane_id, direction) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        match direction {
                            Direction::Down => tab.move_pane_down(pane_id),
                            Direction::Up => tab.move_pane_up(pane_id),
                            Direction::Left => tab.move_pane_left(pane_id),
                            Direction::Right => tab.move_pane_right(pane_id),
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ClearScreenForPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.clear_screen_for_pane_id(pane_id);
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ScrollUpInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_up(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!(
                                "Currently only terminal panes are supported for scrolling up"
                            );
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ScrollDownInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_down(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!(
                                "Currently only terminal panes are supported for scrolling down"
                            );
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ScrollToTopInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_to_top(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!(
                                "Currently only terminal panes are supported for scrolling to top"
                            );
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::ScrollToBottomInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_to_bottom(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!("Currently only terminal panes are supported for scrolling to bottom");
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::PageScrollUpInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_page_up(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!(
                                "Currently only terminal panes are supported for scrolling"
                            );
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::PageScrollDownInPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        if let PaneId::Terminal(terminal_pane_id) = pane_id {
                            tab.scroll_terminal_page_down(terminal_pane_id);
                        } else {
                            // this is because to do this with plugins, we need the client_id -
                            // which we do not have (yet?) in this context...
                            log::error!(
                                "Currently only terminal panes are supported for scrolling"
                            );
                        }
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::TogglePaneIdFullscreen(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.toggle_pane_fullscreen(pane_id);
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::TogglePaneEmbedOrEjectForPaneId(pane_id) => {
                let all_tabs = screen.get_tabs_mut();
                for tab in all_tabs.values_mut() {
                    if tab.has_pane_with_pid(&pane_id) {
                        tab.toggle_pane_embed_or_floating_for_pane_id(pane_id, None)
                            .non_fatal();
                        break;
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::CloseTabWithIndex(tab_index) => {
                screen.close_tab_at_index(tab_index).non_fatal()
            },
            ScreenInstruction::BreakPanesToNewTab {
                pane_ids,
                default_shell,
                should_change_focus_to_new_tab,
                new_tab_name,
                client_id,
            } => {
                screen.break_multiple_panes_to_new_tab(
                    pane_ids,
                    default_shell,
                    should_change_focus_to_new_tab,
                    new_tab_name,
                    client_id,
                )?;
                // TODO: is this a race?
                let pane_group = screen.get_client_pane_group(&client_id);
                if !pane_group.is_empty() {
                    let _ = screen.bus.senders.send_to_background_jobs(
                        BackgroundJob::HighlightPanesWithMessage(
                            pane_group.iter().copied().collect(),
                            "BROKEN OUT".to_owned(),
                        ),
                    );
                }
                screen.clear_pane_group(&client_id);
            },
            ScreenInstruction::BreakPanesToTabWithIndex {
                pane_ids,
                tab_index,
                should_change_focus_to_new_tab,
                client_id,
            } => {
                screen.break_multiple_panes_to_tab_with_index(
                    pane_ids,
                    tab_index,
                    should_change_focus_to_new_tab,
                    client_id,
                )?;
                let pane_group = screen.get_client_pane_group(&client_id);
                if !pane_group.is_empty() {
                    let _ = screen.bus.senders.send_to_background_jobs(
                        BackgroundJob::HighlightPanesWithMessage(
                            pane_group.iter().copied().collect(),
                            "BROKEN OUT".to_owned(),
                        ),
                    );
                }
                screen.clear_pane_group(&client_id);
            },
            ScreenInstruction::TogglePanePinned(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.toggle_pane_pinned(client_id);
            },
            ScreenInstruction::SetFloatingPanePinned(pane_id, should_be_pinned) => {
                screen.set_floating_pane_pinned(pane_id, should_be_pinned);
            },
            ScreenInstruction::StackPanes(
                pane_ids_to_stack,
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                if let Some(last_pane_id) = screen.stack_panes(pane_ids_to_stack) {
                    let _ = screen.focus_pane_with_id(last_pane_id, false, client_id);
                    let _ = screen.render(None);
                    let pane_group = screen.get_client_pane_group(&client_id);
                    if !pane_group.is_empty() {
                        let _ = screen.bus.senders.send_to_background_jobs(
                            BackgroundJob::HighlightPanesWithMessage(
                                pane_group.iter().copied().collect(),
                                "STACKED".to_owned(),
                            ),
                        );
                    }
                    screen.clear_pane_group(&client_id);
                }
            },
            ScreenInstruction::ChangeFloatingPanesCoordinates(
                pane_ids_and_coordinates,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.change_floating_panes_coordinates(pane_ids_and_coordinates);
                let _ = screen.render(None);
            },
            ScreenInstruction::GroupAndUngroupPanes(
                pane_ids_to_group,
                pane_ids_to_ungroup,
                for_all_clients,
                client_id,
            ) => {
                screen.group_and_ungroup_panes(
                    pane_ids_to_group,
                    pane_ids_to_ungroup,
                    for_all_clients,
                    client_id,
                );
                let _ = screen.log_and_report_session_state();
            },
            ScreenInstruction::TogglePaneInGroup(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.toggle_pane_in_group(client_id).non_fatal();
            },
            ScreenInstruction::ToggleGroupMarking(
                client_id,
                _completion_tx, // the action ends here, dropping this will release anything
                                // waiting for it
            ) => {
                screen.toggle_group_marking(client_id).non_fatal();
            },
            ScreenInstruction::SessionSharingStatusChange(web_sharing) => {
                if web_sharing {
                    screen.web_sharing = WebSharing::On;
                } else {
                    screen.web_sharing = WebSharing::Off;
                }

                for tab in screen.tabs.values_mut() {
                    tab.update_web_sharing(screen.web_sharing);
                }
                let _ = screen.log_and_report_session_state();
                let _ = screen.render(None);
            },
            ScreenInstruction::HighlightAndUnhighlightPanes(
                pane_ids_to_highlight,
                pane_ids_to_unhighlight,
                client_id,
            ) => {
                {
                    let all_tabs = screen.get_tabs_mut();
                    for pane_id in pane_ids_to_highlight {
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id) {
                                tab.add_highlight_pane_frame_color_override(
                                    pane_id,
                                    None,
                                    Some(client_id),
                                );
                            }
                        }
                    }
                    for pane_id in pane_ids_to_unhighlight {
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id) {
                                tab.clear_pane_frame_color_override(pane_id, Some(client_id));
                            }
                        }
                    }
                    screen.render(None)?;
                }
                let _ = screen.log_and_report_session_state();
            },
            ScreenInstruction::FloatMultiplePanes(pane_ids_to_float, client_id) => {
                {
                    let all_tabs = screen.get_tabs_mut();
                    let mut ejected_panes_in_group = vec![];
                    for pane_id in pane_ids_to_float {
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id) {
                                if !tab.pane_id_is_floating(&pane_id) {
                                    ejected_panes_in_group.push(pane_id);
                                    tab.toggle_pane_embed_or_floating_for_pane_id(
                                        pane_id,
                                        Some(client_id),
                                    )
                                    .non_fatal();
                                }
                                tab.show_floating_panes();
                            }
                        }
                    }
                    screen.render(None)?;
                    if !ejected_panes_in_group.is_empty() {
                        let _ = screen.bus.senders.send_to_background_jobs(
                            BackgroundJob::HighlightPanesWithMessage(
                                ejected_panes_in_group,
                                "EJECTED".to_owned(),
                            ),
                        );
                    }
                }
                let _ = screen.log_and_report_session_state();
            },
            ScreenInstruction::EmbedMultiplePanes(pane_ids_to_float, client_id) => {
                {
                    let all_tabs = screen.get_tabs_mut();
                    let mut embedded_panes_in_group = vec![];
                    for pane_id in pane_ids_to_float {
                        for tab in all_tabs.values_mut() {
                            if tab.has_pane_with_pid(&pane_id) {
                                if tab.pane_id_is_floating(&pane_id) {
                                    embedded_panes_in_group.push(pane_id);
                                    tab.toggle_pane_embed_or_floating_for_pane_id(
                                        pane_id,
                                        Some(client_id),
                                    )
                                    .non_fatal();
                                }
                                tab.hide_floating_panes();
                            }
                        }
                    }
                    screen.render(None)?;
                    if !embedded_panes_in_group.is_empty() {
                        let _ = screen.bus.senders.send_to_background_jobs(
                            BackgroundJob::HighlightPanesWithMessage(
                                embedded_panes_in_group,
                                "EMBEDDED".to_owned(),
                            ),
                        );
                    }
                }
                let _ = screen.log_and_report_session_state();
            },
            ScreenInstruction::InterceptKeyPresses(plugin_id, client_id) => {
                keybind_intercepts.insert(client_id, plugin_id);
            },
            ScreenInstruction::ClearKeyPressesIntercepts(client_id) => {
                keybind_intercepts.remove(&client_id);
            },
            ScreenInstruction::ReplacePaneWithExistingPane(old_pane_id, new_pane_id) => {
                screen.replace_pane_with_existing_pane(old_pane_id, new_pane_id)
            },
            ScreenInstruction::AddWatcherClient(client_id, size) => {
                screen
                    .add_watcher_client(client_id)
                    .context("failed to add watcher client")?;
                screen.set_watcher_size(client_id, size);
                screen.render(None)?;
            },
            ScreenInstruction::RemoveWatcherClient(client_id) => {
                screen.remove_watcher_client(client_id);
            },
            ScreenInstruction::SetFollowedClient(client_id) => {
                screen
                    .set_followed_client(client_id)
                    .context("failed to set followed client")?;
            },
            ScreenInstruction::WatcherTerminalResize(client_id, size) => {
                // NEW
                screen.set_watcher_size(client_id, size);
                screen.render(None)?;
            },
        }
    }
    Ok(())
}

#[path = "./unit/screen_tests.rs"]
#[cfg(test)]
mod screen_tests;
