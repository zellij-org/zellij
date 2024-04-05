//! Things related to [`Screen`]s.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::str;
use std::time::Duration;

use log::{debug, warn};
use zellij_utils::data::{
    Direction, PaneManifest, PluginPermission, Resize, ResizeStrategy, SessionInfo,
};
use zellij_utils::errors::prelude::*;
use zellij_utils::input::command::RunCommand;
use zellij_utils::input::options::Clipboard;
use zellij_utils::pane_size::{Size, SizeInPixels};
use zellij_utils::{
    consts::{session_info_folder_for_session, ZELLIJ_SOCK_DIR},
    envs::set_session_name,
    input::command::TerminalAction,
    input::layout::{
        FloatingPaneLayout, Layout, Run, RunPluginOrAlias, SwapFloatingLayout, SwapTiledLayout,
        TiledPaneLayout,
    },
    position::Position,
};

use crate::background_jobs::BackgroundJob;
use crate::os_input_output::ResizeCache;
use crate::panes::alacritty_functions::xparse_color;
use crate::panes::terminal_character::AnsiCode;
use crate::session_layout_metadata::{PaneLayoutMetadata, SessionLayoutMetadata};

use crate::{
    output::Output,
    panes::sixel::SixelImageStore,
    panes::PaneId,
    plugins::{PluginId, PluginInstruction, PluginRenderAsset},
    pty::{ClientTabIndexOrPaneId, PtyInstruction, VteBytes},
    tab::Tab,
    thread_bus::Bus,
    ui::{
        loading_indication::LoadingIndication,
        overlay::{Overlay, OverlayWindow},
    },
    ClientId, ServerInstruction,
};
use zellij_utils::{
    data::{
        Event, FloatingPaneCoordinates, InputMode, ModeInfo, Palette, PaletteColor,
        PluginCapabilities, Style, TabInfo,
    },
    errors::{ContextType, ScreenContext},
    input::{get_mode_info, options::Options},
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
type ShouldFloat = bool;
type HoldForCommand = Option<RunCommand>;

/// Instructions that can be sent to the [`Screen`].
#[derive(Debug, Clone)]
pub enum ScreenInstruction {
    PtyBytes(u32, VteBytes),
    PluginBytes(Vec<PluginRenderAsset>),
    Render,
    NewPane(
        PaneId,
        Option<InitialTitle>,
        Option<ShouldFloat>,
        HoldForCommand,
        Option<Run>, // invoked with
        Option<FloatingPaneCoordinates>,
        ClientTabIndexOrPaneId,
    ),
    OpenInPlaceEditor(PaneId, ClientId),
    TogglePaneEmbedOrFloating(ClientId),
    ToggleFloatingPanes(ClientId, Option<TerminalAction>),
    HorizontalSplit(PaneId, Option<InitialTitle>, HoldForCommand, ClientId),
    VerticalSplit(PaneId, Option<InitialTitle>, HoldForCommand, ClientId),
    WriteCharacter(Vec<u8>, ClientId),
    Resize(ClientId, ResizeStrategy),
    SwitchFocus(ClientId),
    FocusNextPane(ClientId),
    FocusPreviousPane(ClientId),
    MoveFocusLeft(ClientId),
    MoveFocusLeftOrPreviousTab(ClientId),
    MoveFocusDown(ClientId),
    MoveFocusUp(ClientId),
    MoveFocusRight(ClientId),
    MoveFocusRightOrNextTab(ClientId),
    MovePane(ClientId),
    MovePaneBackwards(ClientId),
    MovePaneUp(ClientId),
    MovePaneDown(ClientId),
    MovePaneRight(ClientId),
    MovePaneLeft(ClientId),
    Exit,
    ClearScreen(ClientId),
    DumpScreen(String, ClientId, bool),
    DumpLayout(Option<PathBuf>, ClientId), // PathBuf is the default configured
    // shell
    DumpLayoutToPlugin(PluginId),
    EditScrollback(ClientId),
    ScrollUp(ClientId),
    ScrollUpAt(Position, ClientId),
    ScrollDown(ClientId),
    ScrollDownAt(Position, ClientId),
    ScrollToBottom(ClientId),
    ScrollToTop(ClientId),
    PageScrollUp(ClientId),
    PageScrollDown(ClientId),
    HalfPageScrollUp(ClientId),
    HalfPageScrollDown(ClientId),
    ClearScroll(ClientId),
    CloseFocusedPane(ClientId),
    ToggleActiveTerminalFullscreen(ClientId),
    TogglePaneFrames,
    SetSelectable(PaneId, bool, usize),
    ClosePane(PaneId, Option<ClientId>),
    HoldPane(
        PaneId,
        Option<i32>,
        RunCommand,
        Option<usize>,
        Option<ClientId>,
    ), // Option<i32> is the exit status, Option<usize> is the tab_index
    UpdatePaneName(Vec<u8>, ClientId),
    UndoRenamePane(ClientId),
    NewTab(
        Option<PathBuf>,
        Option<TerminalAction>,
        Option<TiledPaneLayout>,
        Vec<FloatingPaneLayout>,
        Option<String>,
        (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>), // swap layouts
        ClientId,
    ),
    ApplyLayout(
        TiledPaneLayout,
        Vec<FloatingPaneLayout>,
        Vec<(u32, HoldForCommand)>, // new pane pids
        Vec<(u32, HoldForCommand)>, // new floating pane pids
        HashMap<RunPluginOrAlias, Vec<u32>>,
        usize, // tab_index
        ClientId,
    ),
    SwitchTabNext(ClientId),
    SwitchTabPrev(ClientId),
    ToggleActiveSyncTab(ClientId),
    CloseTab(ClientId),
    GoToTab(u32, Option<ClientId>), // this Option is a hacky workaround, please do not copy this behaviour
    GoToTabName(
        String,
        (Vec<SwapTiledLayout>, Vec<SwapFloatingLayout>), // swap layouts
        Option<TerminalAction>,                          // default_shell
        bool,
        Option<ClientId>,
    ),
    ToggleTab(ClientId),
    UpdateTabName(Vec<u8>, ClientId),
    UndoRenameTab(ClientId),
    MoveTabLeft(ClientId),
    MoveTabRight(ClientId),
    TerminalResize(Size),
    TerminalPixelDimensions(PixelDimensions),
    TerminalBackgroundColor(String),
    TerminalForegroundColor(String),
    TerminalColorRegisters(Vec<(usize, String)>),
    ChangeMode(ModeInfo, ClientId),
    ChangeModeForAllClients(ModeInfo),
    LeftClick(Position, ClientId),
    RightClick(Position, ClientId),
    MiddleClick(Position, ClientId),
    LeftMouseRelease(Position, ClientId),
    RightMouseRelease(Position, ClientId),
    MiddleMouseRelease(Position, ClientId),
    MouseHoldLeft(Position, ClientId),
    MouseHoldRight(Position, ClientId),
    MouseHoldMiddle(Position, ClientId),
    Copy(ClientId),
    AddClient(
        ClientId,
        Option<usize>,       // tab position to focus
        Option<(u32, bool)>, // (pane_id, is_plugin) => pane_id to focus
    ),
    RemoveClient(ClientId),
    AddOverlay(Overlay, ClientId),
    RemoveOverlay(ClientId),
    ConfirmPrompt(ClientId),
    DenyPrompt(ClientId),
    UpdateSearch(Vec<u8>, ClientId),
    SearchDown(ClientId),
    SearchUp(ClientId),
    SearchToggleCaseSensitivity(ClientId),
    SearchToggleWholeWord(ClientId),
    SearchToggleWrap(ClientId),
    AddRedPaneFrameColorOverride(Vec<PaneId>, Option<String>), // Option<String> => optional error text
    ClearPaneFrameColorOverride(Vec<PaneId>),
    PreviousSwapLayout(ClientId),
    NextSwapLayout(ClientId),
    QueryTabNames(ClientId),
    NewTiledPluginPane(
        RunPluginOrAlias,
        Option<String>,
        bool,
        Option<PathBuf>,
        ClientId,
    ), // Option<String> is
    // optional pane title, bool is skip cache, Option<PathBuf> is an optional cwd
    NewFloatingPluginPane(
        RunPluginOrAlias,
        Option<String>,
        bool,
        Option<PathBuf>,
        Option<FloatingPaneCoordinates>,
        ClientId,
    ), // Option<String> is an
    // optional pane title, bool
    // is skip cache, Option<PathBuf> is an optional cwd
    NewInPlacePluginPane(RunPluginOrAlias, Option<String>, PaneId, bool, ClientId), // Option<String> is an
    // optional pane title, bool is skip cache
    StartOrReloadPluginPane(RunPluginOrAlias, Option<String>),
    AddPlugin(
        Option<bool>, // should_float
        bool,         // should be opened in place
        RunPluginOrAlias,
        Option<String>, // pane title
        Option<usize>,  // tab index
        u32,            // plugin id
        Option<PaneId>,
        Option<PathBuf>, // cwd
        Option<ClientId>,
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
    ), // bools are: should_float, move_to_focused_tab, should_open_in_place, Option<PaneId> is the pane id to replace, bool following it is skip_cache
    LaunchPlugin(
        RunPluginOrAlias,
        bool,
        bool,
        Option<PaneId>,
        bool,
        Option<PathBuf>,
        ClientId,
    ), // bools are: should_float, should_open_in_place Option<PaneId> is the pane id to replace, Option<PathBuf> is an optional cwd, bool after is skip_cache
    SuppressPane(PaneId, ClientId),          // bool is should_float
    FocusPaneWithId(PaneId, bool, ClientId), // bool is should_float
    RenamePane(PaneId, Vec<u8>),
    RenameTab(usize, Vec<u8>),
    RequestPluginPermissions(
        u32, // u32 - plugin_id
        PluginPermission,
    ),
    BreakPane(Box<Layout>, Option<TerminalAction>, ClientId),
    BreakPaneRight(ClientId),
    BreakPaneLeft(ClientId),
    UpdateSessionInfos(
        BTreeMap<String, SessionInfo>, // String is the session name
        BTreeMap<String, Duration>,    // resurrectable sessions - <name, created>
    ),
    ReplacePane(
        PaneId,
        HoldForCommand,
        Option<InitialTitle>,
        Option<Run>,
        ClientTabIndexOrPaneId,
    ),
    DumpLayoutToHd,
    RenameSession(String, ClientId), // String -> new name
}

impl From<&ScreenInstruction> for ScreenContext {
    fn from(screen_instruction: &ScreenInstruction) -> Self {
        match *screen_instruction {
            ScreenInstruction::PtyBytes(..) => ScreenContext::HandlePtyBytes,
            ScreenInstruction::PluginBytes(..) => ScreenContext::PluginBytes,
            ScreenInstruction::Render => ScreenContext::Render,
            ScreenInstruction::NewPane(..) => ScreenContext::NewPane,
            ScreenInstruction::OpenInPlaceEditor(..) => ScreenContext::OpenInPlaceEditor,
            ScreenInstruction::TogglePaneEmbedOrFloating(..) => {
                ScreenContext::TogglePaneEmbedOrFloating
            },
            ScreenInstruction::ToggleFloatingPanes(..) => ScreenContext::ToggleFloatingPanes,
            ScreenInstruction::HorizontalSplit(..) => ScreenContext::HorizontalSplit,
            ScreenInstruction::VerticalSplit(..) => ScreenContext::VerticalSplit,
            ScreenInstruction::WriteCharacter(..) => ScreenContext::WriteCharacter,
            ScreenInstruction::Resize(.., strategy) => match strategy {
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
            ScreenInstruction::TogglePaneFrames => ScreenContext::TogglePaneFrames,
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
            ScreenInstruction::LeftClick(..) => ScreenContext::LeftClick,
            ScreenInstruction::RightClick(..) => ScreenContext::RightClick,
            ScreenInstruction::MiddleClick(..) => ScreenContext::MiddleClick,
            ScreenInstruction::LeftMouseRelease(..) => ScreenContext::LeftMouseRelease,
            ScreenInstruction::RightMouseRelease(..) => ScreenContext::RightMouseRelease,
            ScreenInstruction::MiddleMouseRelease(..) => ScreenContext::MiddleMouseRelease,
            ScreenInstruction::MouseHoldLeft(..) => ScreenContext::MouseHoldLeft,
            ScreenInstruction::MouseHoldRight(..) => ScreenContext::MouseHoldRight,
            ScreenInstruction::MouseHoldMiddle(..) => ScreenContext::MouseHoldMiddle,
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
            ScreenInstruction::DumpLayoutToHd => ScreenContext::DumpLayoutToHd,
            ScreenInstruction::RenameSession(..) => ScreenContext::RenameSession,
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
    sixel_image_store: Rc<RefCell<SixelImageStore>>,
    /// The overlay that is drawn on top of [`Pane`]'s', [`Tab`]'s and the [`Screen`]
    overlay: OverlayWindow,
    terminal_emulator_colors: Rc<RefCell<Palette>>,
    terminal_emulator_color_codes: Rc<RefCell<HashMap<usize, String>>>,
    connected_clients: Rc<RefCell<HashSet<ClientId>>>,
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
    default_shell: Option<PathBuf>,
    styled_underlines: bool,
    arrow_fonts: bool,
    layout_dir: Option<PathBuf>,
    default_layout_name: Option<String>,
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
        default_shell: Option<PathBuf>,
        session_serialization: bool,
        serialize_pane_viewport: bool,
        scrollback_lines_to_serialize: Option<usize>,
        styled_underlines: bool,
        arrow_fonts: bool,
        layout_dir: Option<PathBuf>,
    ) -> Self {
        let session_name = mode_info.session_name.clone().unwrap_or_default();
        let session_info = SessionInfo::new(session_name.clone());
        let mut session_infos_on_machine = BTreeMap::new();
        let resurrectable_sessions = BTreeMap::new();
        session_infos_on_machine.insert(session_name.clone(), session_info);
        Screen {
            bus,
            max_panes,
            size: client_attributes.size,
            pixel_dimensions: Default::default(),
            character_cell_size: Rc::new(RefCell::new(None)),
            sixel_image_store: Rc::new(RefCell::new(SixelImageStore::default())),
            style: client_attributes.style,
            connected_clients: Rc::new(RefCell::new(HashSet::new())),
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
                        let all_connected_clients: Vec<ClientId> =
                            self.connected_clients.borrow().iter().copied().collect();
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
        let pane_ids = tab_to_close.get_all_pane_ids();
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

        self.size = new_screen_size;
        for tab in self.tabs.values_mut() {
            tab.resize_whole_tab(new_screen_size)
                .with_context(err_context)?;
            tab.set_force_render();
        }
        self.log_and_report_session_state()
            .with_context(err_context)?;
        self.render(None).with_context(err_context)
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

    /// Renders this [`Screen`], which amounts to rendering its active [`Tab`].
    pub fn render(&mut self, plugin_render_assets: Option<Vec<PluginRenderAsset>>) -> Result<()> {
        let err_context = "failed to render screen";

        let mut output = Output::new(
            self.sixel_image_store.clone(),
            self.character_cell_size.clone(),
            self.styled_underlines,
        );
        let mut tabs_to_close = vec![];
        for (tab_index, tab) in &mut self.tabs {
            if tab.has_selectable_tiled_panes() {
                tab.render(&mut output).context(err_context)?;
            } else if !tab.is_pending() {
                tabs_to_close.push(*tab_index);
            }
        }
        for tab_index in tabs_to_close {
            self.close_tab_at_index(tab_index).context(err_context)?;
        }
        if output.is_dirty() {
            let serialized_output = output.serialize().context(err_context)?;
            let _ = self
                .bus
                .senders
                .send_to_server(ServerInstruction::Render(Some(serialized_output)))
                .context(err_context);
        }
        if let Some(plugin_render_assets) = plugin_render_assets {
            let _ = self
                .bus
                .senders
                .send_to_plugin(PluginInstruction::UnblockCliPipes(plugin_render_assets))
                .context("failed to unblock input pipe");
        }
        Ok(())
    }

    /// Returns a mutable reference to this [`Screen`]'s tabs.
    pub fn get_tabs_mut(&mut self) -> &mut BTreeMap<usize, Tab> {
        &mut self.tabs
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
        client_id: ClientId,
    ) -> Result<()> {
        let err_context = || format!("failed to create new tab for client {client_id:?}",);

        let client_id = if self.get_active_tab(client_id).is_ok() {
            client_id
        } else if let Some(first_client_id) = self.get_first_client_id() {
            first_client_id
        } else {
            client_id
        };

        let tab_name = tab_name.unwrap_or_else(|| String::new());

        let position = self.tabs.len();
        let tab = Tab::new(
            tab_index,
            position,
            tab_name,
            self.size,
            self.character_cell_size.clone(),
            self.sixel_image_store.clone(),
            self.bus
                .os_input
                .as_ref()
                .with_context(err_context)?
                .clone(),
            self.bus.senders.clone(),
            self.max_panes,
            self.style,
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
        );
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
        client_id: ClientId,
    ) -> Result<()> {
        if self.tabs.get(&tab_index).is_none() {
            // TODO: we should prevent this situation with a UI - eg. cannot close tabs with a
            // pending state
            log::error!("Tab with index {tab_index} not found. Cannot apply layout!");
            return Ok(());
        }
        let client_id = if self.get_active_tab(client_id).is_ok() {
            client_id
        } else if let Some(first_client_id) = self.get_first_client_id() {
            first_client_id
        } else {
            client_id
        };
        let err_context = || format!("failed to apply layout for tab {tab_index:?}",);

        // move the relevant clients out of the current tab and place them in the new one
        let drained_clients = if self.session_is_mirrored {
            let client_mode_infos_in_source_tab =
                if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
                    let client_mode_infos_in_source_tab = active_tab.drain_connected_clients(None);
                    if active_tab.has_no_connected_clients() {
                        active_tab.visible(false).with_context(err_context)?;
                    }
                    Some(client_mode_infos_in_source_tab)
                } else {
                    None
                };
            let all_connected_clients: Vec<ClientId> =
                self.connected_clients.borrow().iter().copied().collect();
            for client_id in all_connected_clients {
                self.update_client_tab_focus(client_id, tab_index);
            }
            client_mode_infos_in_source_tab
        } else if let Ok(active_tab) = self.get_active_tab_mut(client_id) {
            let client_mode_info_in_source_tab =
                active_tab.drain_connected_clients(Some(vec![client_id]));
            if active_tab.has_no_connected_clients() {
                active_tab.visible(false).with_context(err_context)?;
            }
            self.update_client_tab_focus(client_id, tab_index);
            Some(client_mode_info_in_source_tab)
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
                tab.visible(true)?;
                if let Some(drained_clients) = drained_clients {
                    tab.add_multiple_clients(drained_clients)?;
                }
                Ok(())
            })
            .with_context(err_context)?;

        if !self.active_tab_indices.contains_key(&client_id) {
            // this means this is a new client and we need to add it to our state properly
            self.add_client(client_id).with_context(err_context)?;
        }

        self.log_and_report_session_state()
            .and_then(|_| self.render(None))
            .with_context(err_context)
    }

    pub fn add_client(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = |tab_index| {
            format!("failed to attach client {client_id} to tab with index {tab_index}")
        };

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
        self.connected_clients.borrow_mut().insert(client_id);
        self.tab_history.insert(client_id, tab_history);
        self.tabs
            .get_mut(&tab_index)
            .with_context(|| err_context(tab_index))?
            .add_client(client_id, None)
            .with_context(|| err_context(tab_index))
    }

    pub fn remove_client(&mut self, client_id: ClientId) -> Result<()> {
        let err_context = || format!("failed to remove client {client_id}");

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
        Ok(())
    }
    fn dump_layout_to_hd(&mut self) -> Result<()> {
        let err_context = || format!("Failed to log and report session state");
        let session_layout_metadata = self.get_layout_metadata(self.default_shell.clone());
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
                                // It only allows printable unicode
                                if buf.iter().all(|u| matches!(u, 0x20..=0x7E | 0xA0..=0xFF)) {
                                    active_tab.name.push_str(c);
                                }
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
        if self.tabs.len() < 2 {
            debug!("cannot move tab to left: only one tab exists");
            return Ok(());
        }
        let Some(client_id) = self.client_id(client_id) else {
            return Ok(());
        };
        let Some(&active_tab_idx) = self.active_tab_indices.get(&client_id) else {
            return Ok(());
        };

        // wraps around: [tab1, tab2, tab3] => [tab1, tab2, tab3]
        //                 ^                                 ^
        //          active_tab_idx                     left_tab_idx
        let left_tab_idx = (active_tab_idx + self.tabs.len() - 1) % self.tabs.len();

        self.switch_tabs(active_tab_idx, left_tab_idx, client_id);
        self.log_and_report_session_state()
            .context("failed to move tab to left")?;
        Ok(())
    }

    fn client_id(&mut self, client_id: ClientId) -> Option<u16> {
        if self.get_active_tab(client_id).is_ok() {
            Some(client_id)
        } else {
            self.get_first_client_id()
        }
    }

    fn switch_tabs(&mut self, active_tab_idx: usize, other_tab_idx: usize, client_id: u16) {
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
        if self.tabs.len() < 2 {
            debug!("cannot move tab to right: only one tab exists");
            return Ok(());
        }
        let Some(client_id) = self.client_id(client_id) else {
            return Ok(());
        };
        let Some(&active_tab_idx) = self.active_tab_indices.get(&client_id) else {
            return Ok(());
        };

        // wraps around: [tab1, tab2, tab3] => [tab1, tab2, tab3]
        //                             ^          ^
        //                     active_tab_idx   right_tab_idx
        let right_tab_idx = (active_tab_idx + 1) % self.tabs.len();

        self.switch_tabs(active_tab_idx, right_tab_idx, client_id);
        self.log_and_report_session_state()
            .context("failed to move active tab to right")?;
        Ok(())
    }

    pub fn change_mode(&mut self, mut mode_info: ModeInfo, client_id: ClientId) -> Result<()> {
        if mode_info.session_name.as_ref() != Some(&self.session_name) {
            mode_info.session_name = Some(self.session_name.clone());
        }
        let previous_mode = self
            .mode_info
            .get(&client_id)
            .unwrap_or(&self.default_mode_info)
            .mode;

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

        self.style = mode_info.style;
        self.mode_info.insert(client_id, mode_info.clone());
        for tab in self.tabs.values_mut() {
            tab.change_mode_info(mode_info.clone(), client_id);
            tab.mark_active_pane_for_rerender(client_id);
            tab.update_input_modes()?;
        }

        if let Some(os_input) = &mut self.bus.os_input {
            let _ =
                os_input.send_to_client(client_id, ServerToClientMsg::SwitchToMode(mode_info.mode));
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
                    plugin_pane_to_move_to_active_tab =
                        tab.extract_pane(plugin_pane_id, Some(client_id));
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
                    Some(client_id),
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
            .map(|(tab_index, _tab)| *tab_index);
        match tab_index {
            Some(tab_index) => {
                self.go_to_tab(tab_index + 1, client_id)?;
                self.tabs
                    .get_mut(&tab_index)
                    .with_context(err_context)?
                    .focus_pane_with_id(pane_id, should_float_if_hidden, client_id)
                    .context("failed to focus pane with id")?;
            },
            None => {
                log::error!("Could not find pane with id: {:?}", pane_id);
            },
        };
        Ok(())
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
                .close_pane(active_pane_id, false, Some(client_id))
                .with_context(err_context)?;
            let active_pane_run_instruction = active_pane.invoked_with().clone();
            let tab_index = self.get_new_tab_index();
            let swap_layouts = (
                default_layout.swap_tiled_layouts.clone(),
                default_layout.swap_floating_layouts.clone(),
            );
            self.new_tab(tab_index, swap_layouts, None, client_id)?;
            let tab = self.tabs.get_mut(&tab_index).with_context(err_context)?;
            let (mut tiled_panes_layout, mut floating_panes_layout) = default_layout.new_tab();
            if pane_to_break_is_floating {
                tab.show_floating_panes();
                tab.add_floating_pane(active_pane, active_pane_id, None, Some(client_id))?;
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
            self.bus.senders.send_to_plugin(PluginInstruction::NewTab(
                None,
                default_shell,
                Some(tiled_panes_layout),
                floating_panes_layout,
                tab_index,
                client_id,
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
            self.unblock_input()?;
        }
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
                    .close_pane(active_pane_id, false, Some(client_id))
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
                new_active_tab.add_floating_pane(
                    active_pane,
                    active_pane_id,
                    None,
                    Some(client_id),
                )?;
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
        self.unblock_input()?;
        self.render(None)?;
        Ok(())
    }
    pub fn replace_pane(
        &mut self,
        new_pane_id: PaneId,
        hold_for_command: HoldForCommand,
        run: Option<Run>,
        pane_title: Option<InitialTitle>,
        client_id_tab_index_or_pane_id: ClientTabIndexOrPaneId,
    ) -> Result<()> {
        let err_context = || format!("failed to replace pane");
        let suppress_pane = |tab: &mut Tab, pane_id: PaneId, new_pane_id: PaneId| {
            let _ = tab.suppress_pane_and_replace_with_pid(pane_id, new_pane_id, run);
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
                    .map(|(tab_index, _tab)| *tab_index);
                match tab_index {
                    Some(tab_index) => {
                        let tab = self.tabs.get_mut(&tab_index).with_context(err_context)?;
                        suppress_pane(tab, pane_id, new_pane_id);
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
    fn unblock_input(&self) -> Result<()> {
        self.bus
            .senders
            .send_to_server(ServerInstruction::UnblockInputThread)
            .context("failed to unblock input")
    }
    fn get_layout_metadata(&self, default_shell: Option<PathBuf>) -> SessionLayoutMetadata {
        let mut session_layout_metadata = SessionLayoutMetadata::new(self.default_layout.clone());
        if let Some(default_shell) = default_shell {
            session_layout_metadata.update_default_shell(default_shell);
        }
        let first_client_id = self.get_first_client_id();
        let active_tab_index =
            first_client_id.and_then(|client_id| self.active_tab_indices.get(&client_id));

        for (tab_index, tab) in self.tabs.values().enumerate() {
            let tab_is_focused = active_tab_index == Some(&tab_index);
            let hide_floating_panes = !tab.are_floating_panes_visible();
            let mut suppressed_panes = HashMap::new();
            for (triggering_pane_id, p) in tab.get_suppressed_panes() {
                suppressed_panes.insert(*triggering_pane_id, p);
            }
            let active_pane_id =
                first_client_id.and_then(|client_id| tab.get_active_pane_id(client_id));
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
                    PaneLayoutMetadata::new(
                        pane_id,
                        p.position_and_size(),
                        p.borderless(),
                        p.invoked_with().clone(),
                        p.custom_title(),
                        active_pane_id == Some(pane_id),
                        if self.serialize_pane_viewport {
                            p.serialize(self.scrollback_lines_to_serialize)
                        } else {
                            None
                        },
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
                    PaneLayoutMetadata::new(
                        pane_id,
                        p.position_and_size(),
                        false, // floating panes are never borderless
                        p.invoked_with().clone(),
                        p.custom_title(),
                        active_pane_id == Some(pane_id),
                        if self.serialize_pane_viewport {
                            p.serialize(self.scrollback_lines_to_serialize)
                        } else {
                            None
                        },
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
}

// The box is here in order to make the
// NewClient enum smaller
#[allow(clippy::boxed_local)]
pub(crate) fn screen_thread_main(
    bus: Bus<ScreenInstruction>,
    max_panes: Option<usize>,
    client_attributes: ClientAttributes,
    config_options: Box<Options>,
    debug: bool,
    default_layout: Box<Layout>,
) -> Result<()> {
    let arrow_fonts = !config_options.simplified_ui.unwrap_or_default();
    let draw_pane_frames = config_options.pane_frames.unwrap_or(true);
    let auto_layout = config_options.auto_layout.unwrap_or(true);
    let session_serialization = config_options.session_serialization.unwrap_or(true);
    let serialize_pane_viewport = config_options.serialize_pane_viewport.unwrap_or(false);
    let scrollback_lines_to_serialize = config_options.scrollback_lines_to_serialize;
    let session_is_mirrored = config_options.mirror_session.unwrap_or(false);
    let layout_dir = config_options.layout_dir;
    let default_shell = config_options.default_shell;
    let default_layout_name = config_options
        .default_layout
        .map(|l| format!("{}", l.display()));
    let copy_options = CopyOptions::new(
        config_options.copy_command,
        config_options.copy_clipboard.unwrap_or_default(),
        config_options.copy_on_select.unwrap_or(true),
    );
    let styled_underlines = config_options.styled_underlines.unwrap_or(true);

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
    );

    let mut pending_tab_ids: HashSet<usize> = HashSet::new();
    let mut pending_tab_switches: HashSet<(usize, ClientId)> = HashSet::new(); // usize is the
                                                                               // tab_index

    let mut plugin_loading_message_cache = HashMap::new();
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
                }
                screen.render(Some(plugin_render_assets))?;
            },
            ScreenInstruction::Render => {
                screen.render(None)?;
            },
            ScreenInstruction::NewPane(
                pid,
                initial_pane_title,
                should_float,
                hold_for_command,
                invoked_with,
                floating_pane_coordinates,
                client_or_tab_index,
            ) => {
                match client_or_tab_index {
                    ClientTabIndexOrPaneId::ClientId(client_id) => {
                        active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| {
                            tab.new_pane(pid,
                               initial_pane_title,
                               should_float,
                               invoked_with,
                               floating_pane_coordinates,
                               Some(client_id)
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
                                should_float,
                                invoked_with,
                                floating_pane_coordinates,
                                None,
                            )?;
                            if let Some(hold_for_command) = hold_for_command {
                                let is_first_run = true;
                                active_tab.hold_pane(pid, None, is_first_run, hold_for_command);
                            }
                        } else {
                            log::error!("Tab index not found: {:?}", tab_index);
                        }
                    },
                    ClientTabIndexOrPaneId::PaneId(_pane_id) => {
                        log::error!("cannot open a pane with a pane id??");
                    },
                };
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::OpenInPlaceEditor(pid, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .replace_active_pane_with_editor_pane(pid, client_id), ?);
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::TogglePaneEmbedOrFloating(client_id) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_pane_embed_or_floating(client_id), ?);
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::ToggleFloatingPanes(client_id, default_shell) => {
                active_tab_and_connected_client_id!(screen, client_id, |tab: &mut Tab, client_id: ClientId| tab
                    .toggle_floating_panes(Some(client_id), default_shell), ?);
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::HorizontalSplit(
                pid,
                initial_pane_title,
                hold_for_command,
                client_id,
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.horizontal_split(pid, initial_pane_title, client_id),
                    ?
                );
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
                    );
                }
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::VerticalSplit(
                pid,
                initial_pane_title,
                hold_for_command,
                client_id,
            ) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.vertical_split(pid, initial_pane_title, client_id),
                    ?
                );
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
                    );
                }
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
                screen.render(None)?;
            },
            ScreenInstruction::WriteCharacter(bytes, client_id) => {
                let mut state_changed = false;
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| {
                        let write_result = match tab.is_sync_panes_active() {
                            true => tab.write_to_terminals_on_current_tab(bytes, client_id),
                            false => tab.write_to_active_terminal(bytes, client_id),
                        };
                        if let Ok(true) = write_result {
                            state_changed = true;
                        }
                        write_result
                    },
                    ?
                );
                if state_changed {
                    screen.log_and_report_session_state()?;
                }
            },
            ScreenInstruction::Resize(client_id, strategy) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.resize(client_id, strategy),
                    ?
                );
                screen.unblock_input()?;
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SwitchFocus(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.unblock_input()?;
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::FocusNextPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_next_pane(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::FocusPreviousPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.focus_previous_pane(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusLeft(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_left(client_id),
                    ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusLeftOrPreviousTab(client_id) => {
                screen.move_focus_left_or_previous_tab(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_down(client_id),
                    ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusRight(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_right(client_id),
                    ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusRightOrNextTab(client_id) => {
                screen.move_focus_right_or_next_tab(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MoveFocusUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_focus_up(client_id),
                    ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ClearScreen(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.clear_active_terminal_screen(
                        client_id,
                    ),
                    ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::DumpScreen(file, client_id, full) => {
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
                screen.unblock_input()?;
            },
            ScreenInstruction::DumpLayout(default_shell, client_id) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata = screen.get_layout_metadata(default_shell);
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::DumpLayout(
                        session_layout_metadata,
                        client_id,
                    ))
                    .with_context(err_context)?;
            },
            ScreenInstruction::DumpLayoutToPlugin(plugin_id) => {
                let err_context = || format!("Failed to dump layout");
                let session_layout_metadata =
                    screen.get_layout_metadata(screen.default_shell.clone());
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
            ScreenInstruction::EditScrollback(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.edit_scrollback(client_id),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_up(client_id)
                );
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::MovePane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneBackwards(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_backwards(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_down(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_up(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneRight(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_right(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::MovePaneLeft(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.move_active_pane_left(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ScrollUpAt(point, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_up(&point, 3, client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.scroll_active_terminal_down(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollDownAt(point, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .handle_scrollwheel_down(&point, 3, client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollToBottom(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_to_bottom(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ScrollToTop(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_to_top(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::PageScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_page(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::PageScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_page(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::HalfPageScrollUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_up_half_page(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::HalfPageScrollDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .scroll_active_terminal_down_half_page(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ClearScroll(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .clear_active_terminal_scroll(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::CloseFocusedPane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.close_focused_pane(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SetSelectable(id, selectable, tab_index) => {
                screen.get_indexed_tab_mut(tab_index).map_or_else(
                    || {
                        log::warn!(
                            "Tab index #{} not found, could not set selectable for plugin #{:?}.",
                            tab_index,
                            id
                        )
                    },
                    |tab| tab.set_pane_selectable(id, selectable),
                );

                screen.render(None)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::ClosePane(id, client_id) => {
                match client_id {
                    Some(client_id) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab.close_pane(
                            id,
                            false,
                            Some(client_id)
                        ));
                    },
                    None => {
                        for tab in screen.tabs.values_mut() {
                            if tab.get_all_pane_ids().contains(&id) {
                                tab.close_pane(id, false, None);
                                break;
                            }
                        }
                    },
                }
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::HoldPane(id, exit_status, run_command, tab_index, client_id) => {
                let is_first_run = false;
                match (client_id, tab_index) {
                    (Some(client_id), _) => {
                        active_tab!(screen, client_id, |tab: &mut Tab| tab.hold_pane(
                            id,
                            exit_status,
                            is_first_run,
                            run_command
                        ));
                    },
                    (_, Some(tab_index)) => match screen.tabs.get_mut(&tab_index) {
                        Some(tab) => tab.hold_pane(id, exit_status, is_first_run, run_command),
                        None => log::warn!(
                            "Tab with index {tab_index} not found. Cannot hold pane with id {:?}",
                            id
                        ),
                    },
                    _ => {
                        for tab in screen.tabs.values_mut() {
                            if tab.get_all_pane_ids().contains(&id) {
                                tab.hold_pane(id, exit_status, is_first_run, run_command);
                                break;
                            }
                        }
                    },
                }
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::UpdatePaneName(c, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_active_pane_name(c, client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::UndoRenamePane(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.undo_active_rename_pane(client_id), ?
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ToggleActiveTerminalFullscreen(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_active_pane_fullscreen(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::TogglePaneFrames => {
                screen.draw_pane_frames = !screen.draw_pane_frames;
                for tab in screen.tabs.values_mut() {
                    tab.set_pane_frames(screen.draw_pane_frames);
                }
                screen.render(None)?;
                screen.unblock_input()?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::SwitchTabNext(client_id) => {
                screen.switch_tab_next(None, true, client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::SwitchTabPrev(client_id) => {
                screen.switch_tab_prev(None, true, client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::CloseTab(client_id) => {
                screen.close_tab(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::NewTab(
                cwd,
                default_shell,
                layout,
                floating_panes_layout,
                tab_name,
                swap_layouts,
                client_id,
            ) => {
                let tab_index = screen.get_new_tab_index();
                pending_tab_ids.insert(tab_index);
                screen.new_tab(tab_index, swap_layouts, tab_name.clone(), client_id)?;
                screen
                    .bus
                    .senders
                    .send_to_plugin(PluginInstruction::NewTab(
                        cwd,
                        default_shell,
                        layout,
                        floating_panes_layout,
                        tab_index,
                        client_id,
                    ))?;
            },
            ScreenInstruction::ApplyLayout(
                layout,
                floating_panes_layout,
                new_pane_pids,
                new_floating_pane_pids,
                new_plugin_ids,
                tab_index,
                client_id,
            ) => {
                screen.apply_layout(
                    layout,
                    floating_panes_layout,
                    new_pane_pids.clone(),
                    new_floating_pane_pids,
                    new_plugin_ids.clone(),
                    tab_index,
                    client_id,
                )?;
                pending_tab_ids.remove(&tab_index);
                if pending_tab_ids.is_empty() {
                    for (tab_index, client_id) in pending_tab_switches.drain() {
                        screen.go_to_tab(tab_index as usize, client_id)?;
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
                    }
                }

                screen.unblock_input()?;
                screen.render(None)?;
                // we do this here in order to recover from a race condition on app start
                // that sometimes causes Zellij to think the terminal window is a different size
                // than it actually is - here, we query the client for its terminal size after
                // we've finished the setup and handle it as we handle a normal resize,
                // while this can affect other instances of a layout being applied, the query is
                // very short and cheap and shouldn't cause any trouble
                if let Some(os_input) = &mut screen.bus.os_input {
                    for client_id in screen.connected_clients.borrow().iter() {
                        let _ = os_input
                            .send_to_client(*client_id, ServerToClientMsg::QueryTerminalSize);
                    }
                }
            },
            ScreenInstruction::GoToTab(tab_index, client_id) => {
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
                        screen.unblock_input()?;
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
                    if let Ok(tab_exists) = screen.go_to_tab_name(tab_name.clone(), client_id) {
                        screen.unblock_input()?;
                        screen.render(None)?;
                        if create && !tab_exists {
                            let tab_index = screen.get_new_tab_index();
                            screen.new_tab(tab_index, swap_layouts, Some(tab_name), client_id)?;
                            screen
                                .bus
                                .senders
                                .send_to_plugin(PluginInstruction::NewTab(
                                    None,
                                    default_shell,
                                    None,
                                    vec![],
                                    tab_index,
                                    client_id,
                                ))?;
                        }
                    }
                }
            },
            ScreenInstruction::UpdateTabName(c, client_id) => {
                screen.update_active_tab_name(c, client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::UndoRenameTab(client_id) => {
                screen.undo_active_rename_tab(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::MoveTabLeft(client_id) => {
                screen.move_active_tab_to_left(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::MoveTabRight(client_id) => {
                screen.move_active_tab_to_right(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
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
            ScreenInstruction::ChangeMode(mode_info, client_id) => {
                screen.change_mode(mode_info, client_id)?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ChangeModeForAllClients(mode_info) => {
                screen.change_mode_for_all_clients(mode_info)?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ToggleActiveSyncTab(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, _client_id: ClientId| tab.toggle_sync_panes_is_active()
                );
                screen.log_and_report_session_state()?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::LeftClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_left_click(&point, client_id), ?);
                screen.log_and_report_session_state()?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::RightClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_right_click(&point, client_id), ?);
                screen.log_and_report_session_state()?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::MiddleClick(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_middle_click(&point, client_id), ?);
                screen.log_and_report_session_state()?;
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::LeftMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_left_mouse_release(&point, client_id), ?);
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::RightMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_right_mouse_release(&point, client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::MiddleMouseRelease(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_middle_mouse_release(&point, client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::MouseHoldLeft(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_left(&point, client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::MouseHoldRight(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_right(&point, client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::MouseHoldMiddle(point, client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .handle_mouse_hold_middle(&point, client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::Copy(client_id) => {
                active_tab!(screen, client_id, |tab: &mut Tab| tab
                    .copy_selection(client_id), ?);
                screen.render(None)?;
            },
            ScreenInstruction::Exit => {
                break;
            },
            ScreenInstruction::ToggleTab(client_id) => {
                screen.toggle_tab(client_id)?;
                screen.unblock_input()?;
                screen.render(None)?;
            },
            ScreenInstruction::AddClient(client_id, tab_position_to_focus, pane_id_to_focus) => {
                screen.add_client(client_id)?;
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
                screen.log_and_report_session_state()?;
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
                screen.unblock_input()?;
            },
            ScreenInstruction::RemoveOverlay(_client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::ConfirmPrompt(_client_id) => {
                let overlay = screen.get_active_overlays_mut().pop();
                let instruction = overlay.and_then(|o| o.prompt_confirm());
                if let Some(instruction) = instruction {
                    screen
                        .bus
                        .senders
                        .send_to_server(*instruction)
                        .context("failed to confirm prompt")?;
                }
                screen.unblock_input()?;
            },
            ScreenInstruction::DenyPrompt(_client_id) => {
                screen.get_active_overlays_mut().pop();
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::UpdateSearch(c, client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.update_search_term(c, client_id), ?
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchDown(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_down(client_id)
                );
                screen.render(None)?;
            },
            ScreenInstruction::SearchUp(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.search_up(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleCaseSensitivity(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab
                        .toggle_search_case_sensitivity(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleWrap(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_wrap(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
            },
            ScreenInstruction::SearchToggleWholeWord(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.toggle_search_whole_words(client_id)
                );
                screen.render(None)?;
                screen.unblock_input()?;
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
            ScreenInstruction::ClearPaneFrameColorOverride(pane_ids) => {
                let all_tabs = screen.get_tabs_mut();
                for pane_id in pane_ids {
                    for tab in all_tabs.values_mut() {
                        if tab.has_pane_with_pid(&pane_id) {
                            tab.clear_pane_frame_color_override(pane_id);
                            break;
                        }
                    }
                }
                screen.render(None)?;
            },
            ScreenInstruction::PreviousSwapLayout(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.previous_swap_layout(Some(client_id)),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::NextSwapLayout(client_id) => {
                active_tab_and_connected_client_id!(
                    screen,
                    client_id,
                    |tab: &mut Tab, client_id: ClientId| tab.next_swap_layout(Some(client_id), true),
                    ?
                );
                screen.render(None)?;
                screen.log_and_report_session_state()?;
                screen.unblock_input()?;
            },
            ScreenInstruction::QueryTabNames(client_id) => {
                let tab_names = screen
                    .get_tabs_mut()
                    .values()
                    .map(|tab| tab.name.clone())
                    .collect::<Vec<String>>();
                screen
                    .bus
                    .senders
                    .send_to_server(ServerInstruction::Log(tab_names, client_id))?;
            },
            ScreenInstruction::NewTiledPluginPane(
                run_plugin,
                pane_title,
                skip_cache,
                cwd,
                client_id,
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
                    ))?;
            },
            ScreenInstruction::NewFloatingPluginPane(
                run_plugin,
                pane_title,
                skip_cache,
                cwd,
                floating_pane_coordinates,
                client_id,
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
                            floating_pane_coordinates,
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
                        ))?;
                },
                None => {
                    log::error!(
                        "Could not find an active tab - is there at least 1 connected user?"
                    );
                },
            },
            ScreenInstruction::StartOrReloadPluginPane(run_plugin, pane_title) => {
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
                client_id,
            ) => {
                let pane_title = pane_title.unwrap_or_else(|| {
                    format!(
                        "({}) - {}",
                        cwd.map(|cwd| cwd.display().to_string())
                            .unwrap_or(".".to_owned()),
                        run_plugin_or_alias.location_string()
                    )
                });
                let run_plugin = Run::Plugin(run_plugin_or_alias);

                if should_be_in_place {
                    if let Some(pane_id_to_replace) = pane_id_to_replace {
                        let client_tab_index_or_pane_id =
                            ClientTabIndexOrPaneId::PaneId(pane_id_to_replace);
                        screen.replace_pane(
                            PaneId::Plugin(plugin_id),
                            None,
                            Some(run_plugin),
                            Some(pane_title),
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
                            should_float,
                            Some(run_plugin),
                            None,
                            Some(client_id),
                        )
                    }, ?);
                } else if let Some(active_tab) =
                    tab_index.and_then(|tab_index| screen.tabs.get_mut(&tab_index))
                {
                    active_tab.new_pane(
                        PaneId::Plugin(plugin_id),
                        Some(pane_title),
                        should_float,
                        Some(run_plugin),
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
                screen.unblock_input()?;
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
                        tab.suppress_pane(pane_id, client_id);
                        drop(screen.render(None));
                        break;
                    }
                }
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::FocusPaneWithId(pane_id, should_float_if_hidden, client_id) => {
                screen.focus_pane_with_id(pane_id, should_float_if_hidden, client_id)?;
                screen.log_and_report_session_state()?;
            },
            ScreenInstruction::RenamePane(pane_id, new_name) => {
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
            ScreenInstruction::RenameTab(tab_index, new_name) => {
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
                    log::error!(
                        "PluginId '{}' not found - cannot request permissions",
                        plugin_id
                    );
                }
            },
            ScreenInstruction::BreakPane(default_layout, default_shell, client_id) => {
                screen.break_pane(default_shell, default_layout, client_id)?;
            },
            ScreenInstruction::BreakPaneRight(client_id) => {
                screen.break_pane_to_new_tab(Direction::Right, client_id)?;
            },
            ScreenInstruction::BreakPaneLeft(client_id) => {
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
                client_id_tab_index_or_pane_id,
            ) => {
                screen.replace_pane(
                    new_pane_id,
                    hold_for_command,
                    invoked_with,
                    pane_title,
                    client_id_tab_index_or_pane_id,
                )?;

                screen.unblock_input()?;
                screen.log_and_report_session_state()?;

                screen.render(None)?;
            },
            ScreenInstruction::DumpLayoutToHd => {
                if screen.session_serialization {
                    screen.dump_layout_to_hd()?;
                }
            },
            ScreenInstruction::RenameSession(name, client_id) => {
                if screen.session_infos_on_machine.contains_key(&name) {
                    let error_text = "A session by this name already exists.";
                    log::error!("{}", error_text);
                    if let Some(os_input) = &mut screen.bus.os_input {
                        let _ = os_input.send_to_client(
                            client_id,
                            ServerToClientMsg::LogError(vec![error_text.to_owned()]),
                        );
                    }
                } else if screen.resurrectable_sessions.contains_key(&name) {
                    let error_text =
                        "A resurrectable session by this name exists, cannot use this name.";
                    log::error!("{}", error_text);
                    if let Some(os_input) = &mut screen.bus.os_input {
                        let _ = os_input.send_to_client(
                            client_id,
                            ServerToClientMsg::LogError(vec![error_text.to_owned()]),
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
                    set_session_name(name);
                }
                screen.unblock_input()?;
            },
        }
    }
    Ok(())
}

#[path = "./unit/screen_tests.rs"]
#[cfg(test)]
mod screen_tests;
